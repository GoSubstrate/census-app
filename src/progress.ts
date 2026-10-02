// Turns the scanner's `@@census` event stream into one progress bar.
//
// The scanner (gosubstrate.com/census/substrate-census-*.sh / .ps1, run with --events) announces each step by
// label. The labels are the same on macOS, Linux and Windows. Each step owns a slice of the bar, sized by how
// long it usually takes; the slices end at the percentages below. A step with a known total (detection,
// forensics) fills its slice tick by tick; a step without one (a spinner: scanning, artifacts, submitting) eases
// toward the end of its slice over time, so the bar never stalls and never lies about being done.
//
// A label we do not know (a newer scanner) moves nothing but still shows its text. The bar only goes forward.

export type CensusEvent =
  | { ev: "start"; script_version?: string; os?: string }
  | { ev: "phase"; label: string; total: number; detail?: string }
  | { ev: "tick"; label: string; cur: number; total: number }
  | { ev: "done"; label: string; summary?: string }
  | { ev: "error"; message: string }
  | { ev: "result"; score: number | null; band?: string; dashboard_url?: string; headline?: string };

interface Step {
  label: string;
  end: number; // where this step's slice of the bar ends, 0..100
  text: string; // what the window says while it runs
  ease?: number; // seconds to cover ~63% of a spinner step's slice
}

export const STEPS: Step[] = [
  { label: "start", end: 2, text: "Starting the census" },
  { label: "identity", end: 4, text: "Identifying this computer" },
  { label: "catalog", end: 7, text: "Loading the tool catalog" },
  { label: "detection", end: 22, text: "Detecting AI tools" },
  { label: "forensics", end: 36, text: "Probing agent setups" },
  { label: "scanning", end: 68, text: "Analyzing your projects", ease: 45 },
  { label: "skills", end: 71, text: "Counting skills" },
  { label: "lsp", end: 73, text: "Checking language servers" },
  { label: "optimizations", end: 75, text: "Checking agent settings" },
  { label: "artifacts", end: 81, text: "Checking app capabilities", ease: 8 },
  { label: "providers", end: 83, text: "Checking model providers" },
  { label: "ides", end: 87, text: "Checking editor extensions", ease: 6 },
  { label: "browsers", end: 91, text: "Checking browser extensions", ease: 6 },
  { label: "power clis", end: 93, text: "Checking command-line tools" },
  { label: "probe", end: 95, text: "Checking for follow-ups", ease: 4 },
  { label: "submitting", end: 99, text: "Scoring your setup", ease: 6 },
];

// Notes that close a step they did not open: the project scan ends with these three.
const ALIAS: Record<string, string> = { rules: "scanning", depth: "scanning", agents: "scanning" };

const index = (label: string) => STEPS.findIndex((s) => s.label === (ALIAS[label] ?? label));
const startOf = (i: number) => (i > 0 ? STEPS[i - 1].end : 0);
const titleCase = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

export interface Progress {
  percent: number; // 0..100, never goes down
  text: string; // the current step, in words
  detail: string; // the last finding, e.g. "24 AI tools detected across 10 categories"
}

export class Tracker {
  private floor = 0;
  private active = -1;
  private activeSince = 0;
  private spinning = false;
  private text = "Starting the census";
  private detail = "";

  constructor(private now: () => number = () => Date.now()) {}

  apply(e: CensusEvent): void {
    switch (e.ev) {
      case "start":
        this.raise(STEPS[0].end);
        this.text = STEPS[1].text;
        break;
      case "phase": {
        const i = index(e.label);
        this.text = i >= 0 ? STEPS[i].text : titleCase(e.label);
        if (i < 0) break;
        this.raise(startOf(i));
        this.active = i;
        this.activeSince = this.now();
        this.spinning = !(e.total > 0);
        break;
      }
      case "tick": {
        const i = index(e.label);
        if (i < 0 || !(e.total > 0)) break;
        const f = Math.min(1, Math.max(0, e.cur / e.total));
        this.raise(startOf(i) + (STEPS[i].end - startOf(i)) * f);
        break;
      }
      case "done": {
        if (e.summary) this.detail = e.summary;
        const i = index(e.label);
        if (i < 0) break;
        this.raise(STEPS[i].end);
        if (i === this.active) this.active = -1;
        const next = STEPS[i + 1];
        if (next) this.text = next.text;
        break;
      }
      case "result":
        this.raise(100);
        this.active = -1;
        this.text = "Done";
        break;
      case "error":
        break;
    }
  }

  // The bar right now. A spinner step eases toward 92% of its slice: 1 - e^(-t/ease).
  read(): Progress {
    let p = this.floor;
    if (this.active >= 0 && this.spinning) {
      const s = STEPS[this.active], a = startOf(this.active), t = (this.now() - this.activeSince) / 1000;
      p = Math.max(p, a + (s.end - a) * 0.92 * (1 - Math.exp(-t / (s.ease ?? 5))));
    }
    return { percent: Math.min(100, p), text: this.text, detail: this.detail };
  }

  private raise(p: number) {
    this.floor = Math.max(this.floor, Math.min(100, p));
  }
}

// One stderr line from the scanner, or null when it is not an event.
export function parseLine(line: string): CensusEvent | null {
  const m = /^@@census (\{.*\})\s*$/.exec(line);
  if (!m) return null;
  try {
    const v = JSON.parse(m[1]);
    return v && typeof v.ev === "string" ? (v as CensusEvent) : null;
  } catch {
    return null;
  }
}
