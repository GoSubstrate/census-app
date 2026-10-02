import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { STEPS, Tracker, parseLine } from "./progress";

// A real macOS run (scanner 1.33.0 with --events), thinned and anonymized.
const FIXTURE = readFileSync(join(import.meta.dir, "fixture-macos.txt"), "utf8").split("\n");

describe("parseLine", () => {
  test("reads an event line", () => {
    expect(parseLine('@@census {"ev":"done","label":"ides","summary":"8 AI editor extension(s)"}')).toEqual({
      ev: "done", label: "ides", summary: "8 AI editor extension(s)",
    });
  });
  test("ignores plain output, broken JSON and objects without ev", () => {
    expect(parseLine("  [done] detection 24 AI tools")).toBeNull();
    expect(parseLine('@@census {"ev":')).toBeNull();
    expect(parseLine('@@census {"label":"x"}')).toBeNull();
  });
  test("keeps escaped non-ASCII from the Windows scanner", () => {
    expect(parseLine('@@census {"ev":"done","label":"ides","summary":"3 \\u00b7 4"}')).toMatchObject({ summary: "3 · 4" });
  });
});

describe("Tracker", () => {
  test("the step slices only go forward and end at 99 before the result", () => {
    for (let i = 1; i < STEPS.length; i++) expect(STEPS[i].end).toBeGreaterThan(STEPS[i - 1].end);
    expect(STEPS.at(-1)!.end).toBe(99);
  });

  test("a real run moves the bar forward only, and the result fills it", () => {
    let clock = 0;
    const t = new Tracker(() => clock);
    let last = 0;
    for (const line of FIXTURE) {
      const e = parseLine(line);
      if (!e) continue;
      clock += 500;
      t.apply(e);
      const p = t.read().percent;
      expect(p).toBeGreaterThanOrEqual(last);
      last = p;
    }
    expect(t.read()).toMatchObject({ percent: 100, text: "Done" });
  });

  test("a spinner step eases toward its end but never reaches it", () => {
    let clock = 0;
    const t = new Tracker(() => clock);
    t.apply({ ev: "phase", label: "scanning", total: 0 });
    const start = t.read().percent;
    expect(start).toBe(36);
    clock = 45_000;
    const mid = t.read().percent;
    expect(mid).toBeGreaterThan(start);
    clock = 10 * 60_000;
    expect(t.read().percent).toBeLessThan(68);
    t.apply({ ev: "done", label: "rules", summary: "68 rules file(s)" });
    expect(t.read()).toMatchObject({ percent: 68, detail: "68 rules file(s)", text: "Counting skills" });
  });

  test("ticks fill a counted step in proportion", () => {
    const t = new Tracker(() => 0);
    t.apply({ ev: "phase", label: "detection", total: 100 });
    t.apply({ ev: "tick", label: "detection", cur: 50, total: 100 });
    expect(t.read().percent).toBeCloseTo(7 + (22 - 7) / 2);
  });

  test("an unknown label shows its text and moves nothing", () => {
    const t = new Tracker(() => 0);
    t.apply({ ev: "done", label: "catalog" });
    t.apply({ ev: "phase", label: "new thing", total: 0 });
    expect(t.read()).toMatchObject({ percent: 7, text: "New thing" });
  });
});
