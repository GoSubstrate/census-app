import { describe, expect, test } from "bun:test";
import { DIAGNOSTIC, installWords, slides } from "./promo";

const offer = (app: string, installed: boolean, offered = true) => ({ app, name: `Substrate ${app}`, installed, offered });

describe("slides", () => {
  test("both missing: cycle Scribe then Minutes, each with its icon and page", () => {
    const s = slides([offer("scribe", false), offer("minutes", false)]);
    expect(s.map((x) => (x.kind === "app" ? x.app : x.kind))).toEqual(["scribe", "minutes"]);
    expect(s[0]).toMatchObject({ icon: "/scribe.png", page: "https://gosubstrate.com/apps/scribe/" });
  });
  test("one missing: only that one", () => {
    expect(slides([offer("scribe", true), offer("minutes", false)]).map((x) => x.kind === "app" && x.app)).toEqual(["minutes"]);
  });
  test("both installed: the Diagnostic", () => {
    expect(slides([offer("scribe", true), offer("minutes", true)])).toEqual([DIAGNOSTIC]);
  });
  test("missing but no build for this computer: the Diagnostic, never a dead Install button", () => {
    expect(slides([offer("scribe", false, false), offer("minutes", true)])).toEqual([DIAGNOSTIC]);
  });
  test("the scan cannot tell (no answer): the Diagnostic", () => {
    expect(slides([])).toEqual([DIAGNOSTIC]);
  });
});

test("install words", () => {
  expect(installWords("downloading", 12 * 1048576, 21 * 1048576)).toBe("Downloading 12 of 21 MB");
  expect(installWords("done", 0, null, "Substrate Scribe")).toBe("Installed. Substrate Scribe is ready.");
});
