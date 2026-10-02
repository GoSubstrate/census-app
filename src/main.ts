import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Tracker, parseLine, type CensusEvent } from "./progress";

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

async function start() {
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
  try {
    await invoke("start_census");
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
  if (result?.dashboard_url) {
    $("score").textContent = result.score == null ? "?" : String(result.score);
    $("band").textContent = result.band ?? "";
    $("headline").textContent = result.headline ?? "";
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

listen<Output>("census", (e) => onOutput(e.payload));
$("run").addEventListener("click", start);
$("retry").addEventListener("click", start);
$("again").addEventListener("click", (e) => (e.preventDefault(), start()));
$("cancel").addEventListener("click", cancel);
$("what").addEventListener("click", (e) => (e.preventDefault(), openUrl(PRIVACY_URL)));
$("open").addEventListener("click", () => result?.dashboard_url && openUrl(result.dashboard_url));
$("copy").addEventListener("click", (e) => result?.dashboard_url && copy(result.dashboard_url, e.currentTarget as HTMLElement));
$("copylog").addEventListener("click", (e) => copy(log.join("\n"), e.currentTarget as HTMLElement));
show("idle");
