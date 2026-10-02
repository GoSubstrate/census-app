import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Tracker, parseLine, type CensusEvent } from "./progress";
import { DWELL, installWords, slides, type Offer, type Slide } from "./promo";

// What the Rust side forwards from the running scanner (src-tauri/src/lib.rs, `Output`).
type Output = { kind: "line"; text: string } | { kind: "exit"; code: number | null };

type View = "idle" | "running" | "done" | "failed";
const PRIVACY_URL = "https://gosubstrate.com/census/#privacy";
const LOG_LINES = 200;

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

let tracker = new Tracker();
let log: string[] = [];
let lastError = "";
let result: Extract<CensusEvent, { ev: "result" }> | null = null;
let timer = 0;
let running = false;

function show(view: View) {
  document.querySelectorAll<HTMLElement>(".view").forEach((el) => (el.hidden = el.dataset.view !== view));
}

function paint() {
  const p = tracker.read();
  const pct = Math.floor(p.percent);
  $("fill").style.width = `${p.percent}%`;
  $("pct").textContent = `${pct}%`;
  $("step").textContent = p.text;
  if (p.detail) $("detail").textContent = p.detail;
  $("fill").parentElement!.setAttribute("aria-valuenow", String(pct));
}

// ---------- the promo under the bar (src/promo.ts) ----------

type InstallEvent = { app: string; phase: string; received?: number; total?: number | null; message?: string };

let deck: Slide[] = [];
let slideAt = 0;
let rotate = 0;
let hovering = false;
const installs = new Map<string, InstallEvent>(); // the latest event per app, kept across slides

const esc = (t: string) => t.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

function statusOf(app: string, name: string): { text: string; cls: string; busy: boolean; done: boolean } {
  const e = installs.get(app);
  if (!e) return { text: "", cls: "", busy: false, done: false };
  if (e.phase === "error") return { text: e.message || "The install did not finish.", cls: "bad", busy: false, done: false };
  if (e.phase === "done") return { text: installWords("done", 0, null, name), cls: "ok", busy: false, done: true };
  return { text: installWords(e.phase, e.received, e.total ?? null, name), cls: "", busy: true, done: false };
}

function renderPromo() {
  const el = $("promo");
  if (!deck.length) return void (el.hidden = true);
  const s = deck[slideAt % deck.length];
  let card: string;
  if (s.kind === "app") {
    const st = statusOf(s.app, s.name);
    const button = st.done ? "" : `<button class="primary" data-install="${s.app}" ${st.busy ? "disabled" : ""}>${st.busy ? "Installing…" : "Install"}</button>`;
    card = `<div class="promo-card" data-slide="${s.app}">
      <div class="promo-head"><img class="promo-icon" src="${s.icon}" alt="" width="44" height="44" />
        <div><div class="promo-name">${esc(s.name)} <span class="pill">Free</span></div><div class="promo-tag">${esc(s.tagline)}</div></div></div>
      <p class="promo-body">${esc(s.body)}</p>
      <div class="promo-actions">${button}<a href="#" class="quiet" data-open="${s.page}">Learn more</a>
        <span class="promo-status ${st.cls}">${esc(st.text)}</span></div></div>`;
  } else {
    card = `<div class="promo-card" data-slide="diagnostic"><span class="promo-eyebrow">For your organization</span>
      <p class="promo-title">${esc(s.title)}</p><p class="promo-body">${esc(s.body)}</p>
      <div class="promo-actions"><button class="primary" data-open="${s.url}">Get in touch</button></div></div>`;
  }
  const dots = deck.length > 1
    ? `<div class="promo-dots">${deck.map((_, i) => `<button data-dot="${i}" aria-label="Show ${i + 1} of ${deck.length}" aria-current="${i === slideAt % deck.length}"></button>`).join("")}</div>`
    : "";
  el.innerHTML = card + dots;
  el.hidden = false;
}

// Stays put while hovered or while the shown app is installing; otherwise the next slide every DWELL seconds.
function tick() {
  const s = deck[slideAt % deck.length];
  if (hovering || deck.length < 2 || (s && s.kind === "app" && statusOf(s.app, s.name).busy)) return;
  slideAt = (slideAt + 1) % deck.length;
  renderPromo();
}

async function showPromo() {
  window.clearInterval(rotate);
  deck = [];
  slideAt = 0;
  $("promo").hidden = true;
  let offers: Offer[] = [];
  try {
    offers = await invoke<Offer[]>("companions");
  } catch {
    /* no answer: the Diagnostic */
  }
  if (!running) return;
  deck = slides(offers);
  renderPromo();
  rotate = window.setInterval(tick, DWELL * 1000);
}

$("promo").addEventListener("mouseenter", () => (hovering = true));
$("promo").addEventListener("mouseleave", () => (hovering = false));
$("promo").addEventListener("click", (e) => {
  const t = e.target as HTMLElement;
  const open = t.closest<HTMLElement>("[data-open]");
  if (open) {
    e.preventDefault();
    openUrl(open.dataset.open!);
    return;
  }
  const dot = t.closest<HTMLElement>("[data-dot]");
  if (dot) {
    slideAt = Number(dot.dataset.dot);
    return renderPromo();
  }
  const install = t.closest<HTMLElement>("[data-install]");
  if (install) {
    const app = install.dataset.install!;
    installs.set(app, { app, phase: "checking" });
    renderPromo();
    invoke("install_companion", { which: app }).catch((err) =>
      (installs.set(app, { app, phase: "error", message: String(err) }), renderPromo()),
    );
  }
});

listen<InstallEvent>("companion", (e) => {
  installs.set(e.payload.app, e.payload);
  const s = deck[slideAt % deck.length];
  if (s && s.kind === "app" && s.app === e.payload.app) renderPromo();
});

// ---------- the optional email ----------
// Kept on this computer only (localStorage), so "Run it again" does not ask twice. Sent once, inside the scan.
const EMAIL_KEY = "census.email";
const EMAIL_RE = /^[^\s@"'`$<>;|&\\]+@[^\s@"'`$<>;|&\\]+\.[^\s@"'`$<>;|&\\]+$/;
const HINT = "We email you the dashboard link, and follow up with how to move up the ladder.";
const emailEl = $<HTMLInputElement>("email");
emailEl.value = localStorage.getItem(EMAIL_KEY) || "";

/** The typed address: "" when empty, null when it is not an address. */
function readEmail(): string | null {
  const v = emailEl.value.trim();
  if (!v) return "";
  return v.length <= 254 && EMAIL_RE.test(v) ? v : null;
}

emailEl.addEventListener("input", () => {
  emailEl.removeAttribute("aria-invalid");
  $("emailHint").textContent = HINT;
  $("emailHint").classList.remove("bad");
});
emailEl.addEventListener("keydown", (e) => e.key === "Enter" && start());

let sentTo = "";

async function start() {
  const email = readEmail();
  if (email === null) {
    emailEl.setAttribute("aria-invalid", "true");
    $("emailHint").textContent = "That does not look like an email address. Fix it, or clear it to run without one.";
    $("emailHint").classList.add("bad");
    emailEl.focus();
    show("idle");
    return;
  }
  if (email) localStorage.setItem(EMAIL_KEY, email);
  else localStorage.removeItem(EMAIL_KEY);
  sentTo = email;
  tracker = new Tracker();
  log = [];
  lastError = "";
  result = null;
  $("detail").textContent = "Fetching the latest scanner from gosubstrate.com";
  paint();
  show("running");
  running = true;
  window.clearInterval(timer);
  timer = window.setInterval(paint, 250);
  showPromo();
  try {
    const theme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    await invoke("start_census", { email: sentTo || null, theme });
  } catch (e) {
    finish(null, String(e));
  }
}

function onOutput(o: Output) {
  if (o.kind === "exit") return finish(o.code);
  log.push(o.text);
  if (log.length > LOG_LINES) log.shift();
  const e = parseLine(o.text);
  if (!e) return;
  tracker.apply(e);
  if (e.ev === "error") lastError = e.message;
  if (e.ev === "result") result = e;
  paint();
}

function finish(code: number | null, launchError = "") {
  if (!running) return;
  running = false;
  window.clearInterval(timer);
  window.clearInterval(rotate);
  if (result?.dashboard_url) {
    $("score").textContent = result.score == null ? "?" : String(result.score);
    $("band").textContent = result.band ?? "";
    $("headline").textContent = result.headline ?? "";
    $("sent").textContent = sentTo ? `We are emailing this link to ${sentTo}.` : "";
    $("sent").hidden = !sentTo;
    show("done");
    return;
  }
  $("error").textContent =
    launchError ||
    (lastError ? `The scanner said: ${lastError}` : code == null ? "The scan was stopped." : `The scanner exited (code ${code}) before it sent a result. Check your internet connection and try again.`);
  $("log").textContent = log.join("\n");
  show("failed");
}

async function cancel() {
  await invoke("cancel_census").catch(() => {});
}

function copy(text: string, button: HTMLElement) {
  navigator.clipboard.writeText(text).then(() => {
    const label = button.textContent;
    button.textContent = "Copied";
    window.setTimeout(() => (button.textContent = label), 1400);
  });
}

// The whole background drags the window: a press on anything that is not a control starts a native drag. One thin
// strip was the only handle before, and without core:window:allow-start-dragging it did nothing.
const CONTROLS = "button, a, input, textarea, select, summary, details, pre, label, [data-no-drag]";
document.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || (e.target as HTMLElement).closest(CONTROLS)) return;
  e.preventDefault();
  getCurrentWindow().startDragging().catch(() => {});
});

const quit = () => invoke("quit").catch(() => window.close());

listen<Output>("census", (e) => onOutput(e.payload));
$("quit").addEventListener("click", quit);
$("quitFailed").addEventListener("click", quit);
$("run").addEventListener("click", start);
$("retry").addEventListener("click", start);
$("again").addEventListener("click", (e) => (e.preventDefault(), start()));
$("cancel").addEventListener("click", cancel);
$("what").addEventListener("click", (e) => (e.preventDefault(), openUrl(PRIVACY_URL)));
$("open").addEventListener("click", () => result?.dashboard_url && openUrl(result.dashboard_url));
$("copy").addEventListener("click", (e) => result?.dashboard_url && copy(result.dashboard_url, e.currentTarget as HTMLElement));
$("copylog").addEventListener("click", (e) => copy(log.join("\n"), e.currentTarget as HTMLElement));
show("idle");
