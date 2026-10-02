// What the window says while the scan runs, below the progress bar.
//
// Scribe or Minutes not on this computer (and the site has a build for it): cycle through the missing ones, each with
// its icon, what it does, "Free" and an Install button that installs it right here (src-tauri/src/companions.rs).
// Both already here, or nothing to offer: the Diagnostic, with a button to the inquiry form on gosubstrate.com.

export interface Offer {
  app: string;
  name: string;
  installed: boolean;
  offered: boolean;
}

export interface AppSlide {
  kind: "app";
  app: string;
  name: string;
  icon: string;
  tagline: string;
  body: string;
  page: string;
}

export interface DiagnosticSlide {
  kind: "diagnostic";
  title: string;
  body: string;
  url: string;
}

export type Slide = AppSlide | DiagnosticSlide;

// The site's own words for each app (gosubstrate.com/apps/<app>/).
const APPS: Record<string, Omit<AppSlide, "kind" | "app" | "name">> = {
  scribe: {
    icon: "/scribe.png",
    tagline: "Dictation for every app",
    body: "Hold a shortcut, speak, and your words appear wherever your cursor is. Speech is turned into text on your computer.",
    page: "https://gosubstrate.com/apps/scribe/",
  },
  minutes: {
    icon: "/minutes.png",
    tagline: "Meeting notes, no bot",
    body: "Records meetings on your computer, writes a live transcript and a clear summary. No bot joins the call, and your notes stay with you.",
    page: "https://gosubstrate.com/apps/minutes/",
  },
};

export const DIAGNOSTIC: DiagnosticSlide = {
  kind: "diagnostic",
  title: "Want experts to help your organization level up?",
  body: "The census is the self-serve read. The Substrate Diagnostic maps your whole organization on the AI ladder and hands back a 90-day route up it.",
  url: "https://gosubstrate.com/diagnostic/?ref=census-app#contact",
};

/** Seconds each slide stays before the next one. */
export const DWELL = 9;

export function slides(offers: Offer[]): Slide[] {
  const missing = offers.filter((o) => !o.installed && o.offered && APPS[o.app]);
  if (!missing.length) return [DIAGNOSTIC];
  return missing.map((o) => ({ kind: "app", app: o.app, name: o.name, ...APPS[o.app] }));
}

/** An install's progress in words: "Downloading 12 of 21 MB". */
export function installWords(phase: string, received = 0, total: number | null = null, name = ""): string {
  const mb = (n: number) => Math.max(1, Math.round(n / 1048576));
  switch (phase) {
    case "checking":
      return "Finding the latest version";
    case "downloading":
      return total ? `Downloading ${mb(received)} of ${mb(total)} MB` : "Downloading";
    case "installing":
      return "Installing";
    case "opening":
      return `Opening ${name}`;
    case "done":
      return `Installed. ${name} is ready.`;
    default:
      return "";
  }
}
