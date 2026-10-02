//! Substrate Census: a one-window wrapper around the public census scanner.
//!
//! It runs the exact command the census page hands out (`curl ... | bash` on macOS and Linux, `irm ... | iex`'s
//! scriptblock form on Windows), always fetching the live script from gosubstrate.com, with one extra flag:
//! `--events`. With it the scanner writes one `@@census {json}` line to stderr per step. This side forwards every
//! stderr line to the window unchanged; the window (src/progress.ts) turns the event lines into a progress bar and
//! keeps the rest as a log. Nothing here reads, stores or sends anything itself.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};

mod companions;

const BASE: &str = "https://gosubstrate.com/census";
/// How long a login shell may take to start the scanner before we give up on it and run without it.
const START_TIMEOUT: Duration = Duration::from_secs(60);

/// What the window receives on the `census` event.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Output {
    Line { text: String },
    Exit { code: Option<i32> },
}

#[derive(Default)]
struct Runner {
    /// The running attempt's process (group leader on Unix), if any.
    pid: Mutex<Option<u32>>,
    busy: AtomicBool,
    cancelled: AtomicBool,
}
type Shared = Arc<Runner>;

#[tauri::command]
fn start_census(app: AppHandle, runner: State<'_, Shared>) -> Result<(), String> {
    if runner.busy.swap(true, Ordering::SeqCst) {
        return Err("A census is already running.".into());
    }
    runner.cancelled.store(false, Ordering::SeqCst);
    let runner = runner.inner().clone();
    std::thread::spawn(move || {
        let code = run_scanner(&app, &runner);
        runner.busy.store(false, Ordering::SeqCst);
        let code = if runner.cancelled.load(Ordering::SeqCst) { None } else { code };
        let _ = app.emit("census", Output::Exit { code });
    });
    Ok(())
}

/// Scribe and Minutes: installed here or not, and whether the site has a build for this computer to offer.
#[derive(Serialize)]
struct Offer {
    app: String,
    name: String,
    installed: bool,
    offered: bool,
}

#[tauri::command]
async fn companions() -> Vec<Offer> {
    let platform = companions::platform();
    companions::detect()
        .into_iter()
        .map(|d| {
            let offered = !d.installed
                && platform.is_some_and(|p| {
                    ureq::get(&format!("{}/api/apps/{}/latest", companions::ORIGIN, d.app))
                        .header("user-agent", concat!("SubstrateCensus/", env!("CARGO_PKG_VERSION")))
                        .call()
                        .ok()
                        .and_then(|mut r| r.body_mut().read_json::<serde_json::Value>().ok())
                        .is_some_and(|a| companions::release_from(&a, companions::ORIGIN, p).is_ok())
                });
            Offer { app: d.app, name: d.name, installed: d.installed, offered }
        })
        .collect()
}

/// Install Scribe or Minutes in the background; progress and the outcome arrive on the `companion` event.
#[tauri::command]
fn install_companion(app: AppHandle, which: String) -> Result<(), String> {
    let c = companions::find(&which).ok_or("unknown app")?;
    std::thread::spawn(move || {
        let emit = |v: serde_json::Value| {
            let _ = app.emit("companion", v);
        };
        let res = companions::install(c, &companions::Target::system(), &|p| emit(json!(p)));
        match res {
            Ok(at) => emit(json!({ "app": c.app, "phase": "done", "path": at.to_string_lossy() })),
            Err(message) => emit(json!({ "app": c.app, "phase": "error", "message": message })),
        }
    });
    Ok(())
}

#[tauri::command]
fn cancel_census(runner: State<'_, Shared>) {
    runner.cancelled.store(true, Ordering::SeqCst);
    if let Some(pid) = *runner.pid.lock().unwrap() {
        kill_tree(pid);
    }
}

/// Runs each way of starting the scanner in turn until one of them actually starts it (prints an event).
/// Returns the scanner's exit code.
fn run_scanner(app: &AppHandle, runner: &Shared) -> Option<i32> {
    let attempts = attempts();
    let last = attempts.len() - 1;
    for (i, mut cmd) in attempts.into_iter().enumerate() {
        if runner.cancelled.load(Ordering::SeqCst) {
            return None;
        }
        let started = Arc::new(AtomicBool::new(false));
        let child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) if i < last => {
                note(app, &format!("could not start {:?}: {e}", cmd.get_program()));
                continue;
            }
            Err(e) => {
                note(app, &format!("could not start {:?}: {e}", cmd.get_program()));
                return Some(-1);
            }
        };
        let pid = child.id();
        *runner.pid.lock().unwrap() = Some(pid);
        watchdog(pid, started.clone(), runner.clone());
        let code = pump(app, &mut child, &started);
        *runner.pid.lock().unwrap() = None;
        if started.load(Ordering::SeqCst) || runner.cancelled.load(Ordering::SeqCst) || i == last {
            return code;
        }
        note(app, "the login shell did not start the scanner; trying again without it");
    }
    None
}

/// Forwards stderr line by line, drains stdout (the scanner only prints its payload there, on a failed submit),
/// and waits for the exit.
fn pump(app: &AppHandle, child: &mut Child, started: &AtomicBool) -> Option<i32> {
    let mut stdout = child.stdout.take();
    let drain = std::thread::spawn(move || {
        if let Some(out) = stdout.as_mut() {
            let _ = std::io::copy(out, &mut std::io::sink());
        }
    });
    if let Some(err) = child.stderr.take() {
        let mut reader = BufReader::new(err);
        let mut buf = Vec::new();
        while matches!(reader.read_until(b'\n', &mut buf), Ok(n) if n > 0) {
            let text = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
            buf.clear();
            if text.starts_with("@@census ") {
                started.store(true, Ordering::SeqCst);
            }
            let _ = app.emit("census", Output::Line { text });
        }
        let _ = reader.read_to_end(&mut buf);
    }
    let _ = drain.join();
    child.wait().ok().and_then(|s| s.code())
}

/// Kills an attempt that has printed no event within START_TIMEOUT: a shell rc file that waits or loops.
fn watchdog(pid: u32, started: Arc<AtomicBool>, runner: Shared) {
    std::thread::spawn(move || {
        let t0 = Instant::now();
        while t0.elapsed() < START_TIMEOUT {
            std::thread::sleep(Duration::from_millis(500));
            if started.load(Ordering::SeqCst) || *runner.pid.lock().unwrap() != Some(pid) {
                return;
            }
        }
        if *runner.pid.lock().unwrap() == Some(pid) {
            kill_tree(pid);
        }
    });
}

/// A line of our own in the log, marked so it is never mistaken for scanner output.
fn note(app: &AppHandle, msg: &str) {
    let _ = app.emit("census", Output::Line { text: format!("[census app] {msg}") });
}

// ---------- the command, per platform ----------

/// macOS and Linux: the census page's one-liner, run by the person's own login shell. A Finder- or launcher-
/// started app gets a bare environment (PATH=/usr/bin:/bin:...), so tools installed by Homebrew, npm or in
/// ~/.local/bin would be invisible and the score would come out lower than the same scan in a terminal. An
/// interactive login shell reads the same rc files a terminal does. The second attempt, if that shell fails to
/// start the scanner, is plain bash with the common install folders added to PATH.
#[cfg(unix)]
fn attempts() -> Vec<Command> {
    use std::os::unix::process::CommandExt;
    let os = if cfg!(target_os = "macos") { "osx" } else { "linux" };
    let line = format!("curl -fsSL {BASE}/substrate-census-{os}.sh | bash -s -- --events");
    let shell = login_shell();
    let mut first = Command::new(&shell);
    first.args(["-i", "-l", "-c", &line]);
    let mut second = Command::new("/bin/bash");
    second.args(["-c", &line]);
    let path = fuller_path();
    [first, second]
        .into_iter()
        .map(|mut c| {
            c.env("PATH", &path).env("SUBSTRATE_EVENTS", "1").env("TERM", "dumb").env("NO_COLOR", "1");
            // Its own process group, so cancel and quit stop the whole pipeline (shell, curl, bash, children).
            c.process_group(0);
            c
        })
        .collect()
}

#[cfg(unix)]
fn login_shell() -> String {
    const KNOWN: [&str; 5] = ["zsh", "bash", "fish", "ksh", "sh"];
    let fallback = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
    std::env::var("SHELL")
        .ok()
        .filter(|s| {
            let name = std::path::Path::new(s).file_name().and_then(|n| n.to_str()).unwrap_or("");
            KNOWN.contains(&name) && std::path::Path::new(s).exists()
        })
        .unwrap_or_else(|| fallback.to_string())
}

#[cfg(unix)]
fn fuller_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs: Vec<String> = std::env::var("PATH").unwrap_or_default().split(':').filter(|d| !d.is_empty()).map(String::from).collect();
    let extra = [
        "/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin",
        "~/.local/bin", "~/bin", "~/.cargo/bin", "~/.bun/bin", "~/.deno/bin", "~/.volta/bin", "~/.npm-global/bin",
    ];
    for d in extra {
        let d = d.replacen('~', &home, 1);
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs.join(":")
}

/// Windows: the census page's scriptblock form, with --events. No console window.
#[cfg(windows)]
fn attempts() -> Vec<Command> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         try {{ [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 }} catch {{}}; \
         & ([scriptblock]::Create((irm {BASE}/substrate-census-windows.ps1))) --events"
    );
    let mut c = Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .env("SUBSTRATE_EVENTS", "1")
        .env("NO_COLOR", "1")
        .creation_flags(CREATE_NO_WINDOW);
    vec![c]
}

#[cfg(unix)]
fn kill_tree(pid: u32) {
    let group = -(pid as i32);
    unsafe { libc::kill(group, libc::SIGTERM) };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        unsafe { libc::kill(group, libc::SIGKILL) };
    });
}

#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .status();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![start_census, cancel_census, companions, install_companion])
        .build(tauri::generate_context!())
        .expect("error while building Substrate Census")
        .run(|app, event| {
            // Quitting mid-scan stops the scan too.
            if let RunEvent::Exit = event {
                if let Some(pid) = *app.state::<Shared>().pid.lock().unwrap() {
                    kill_tree(pid);
                }
            }
        });
}
