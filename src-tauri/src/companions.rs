//! Companion apps. While the scan runs, the window offers Substrate Scribe and Substrate Minutes when they are not
//! on this computer, and installs one when the person presses Install. When both are here it offers the Diagnostic
//! instead (src/promo.ts).
//!
//! Detection only checks where each app lives; it never opens or reads the apps' data:
//! - macOS: `<Name>.app` in `/Applications` or `~/Applications` whose Info.plist names the bundle id, else Spotlight
//!   (`mdfind`) by bundle id, which finds the app wherever it was put.
//! - Windows: `<Name>\<exe>.exe` under `%LOCALAPPDATA%` (a per-user install) or `%ProgramFiles%`.
//! - Linux: the binary on the usual PATH folders (a .deb), or an AppImage named for the app in `~/Applications`.
//!
//! Install is the same flow Substrate Strata uses on the Mac (STRATA-09), on all three systems:
//! 1. Ask `GET https://gosubstrate.com/api/apps/<app>/latest` for this platform's installer: a URL on the same
//!    origin, its size and its SHA-256. No version is hard-coded here.
//! 2. Download it to a temp folder and refuse it unless the size and the SHA-256 match.
//! 3. macOS: mount the DMG out of sight, copy the one `.app` whose bundle id matches into `/Applications` (or
//!    `~/Applications` when that is not writable; never asks for a password), detach. Windows: run the installer
//!    silently (`/S`). Linux: keep the AppImage as `~/Applications/<Name>.AppImage`, executable.
//! 4. Delete the download and open the app.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const ORIGIN: &str = "https://gosubstrate.com";
/// No Substrate installer is near this; anything larger is not ours.
const MAX_BYTES: u64 = 1 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Companion {
    /// The site's slug: `/apps/<app>/`, `/api/apps/<app>/latest`.
    pub app: &'static str,
    pub name: &'static str,
    pub bundle_id: &'static str,
    /// The binary's name on Windows and Linux.
    pub exe: &'static str,
}

pub const COMPANIONS: [Companion; 2] = [
    Companion { app: "scribe", name: "Substrate Scribe", bundle_id: "com.gosubstrate.scribe", exe: "substrate-scribe" },
    Companion { app: "minutes", name: "Substrate Minutes", bundle_id: "com.gosubstrate.minutes", exe: "substrate-minutes" },
];

pub fn find(app: &str) -> Option<&'static Companion> {
    COMPANIONS.iter().find(|c| c.app == app)
}

/// What the window shows for one app.
#[derive(Clone, Debug, Serialize)]
pub struct Detected {
    pub app: String,
    pub name: String,
    pub installed: bool,
}

pub fn detect() -> Vec<Detected> {
    let home = home();
    COMPANIONS.iter().map(|c| Detected { app: c.app.into(), name: c.name.into(), installed: installed_at(c, &home).is_some() }).collect()
}

fn home() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default()
}

/// The site's download platform for this computer, or None (no installer offered).
pub fn platform() -> Option<&'static str> {
    platform_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn platform_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("mac-arm64"),
        ("macos", "x86_64") => Some("mac-x64"),
        ("windows", "x86_64") => Some("windows-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        _ => None,
    }
}

// ---------- detection ----------

#[cfg(target_os = "macos")]
fn bundle_id_of(app: &Path) -> Option<String> {
    let v = plist::Value::from_file(app.join("Contents/Info.plist")).ok()?;
    Some(v.as_dictionary()?.get("CFBundleIdentifier")?.as_string()?.to_string())
}

/// Where the app is on this computer, if it is.
#[cfg(target_os = "macos")]
pub fn installed_at(c: &Companion, home: &Path) -> Option<PathBuf> {
    let roots = [PathBuf::from("/Applications"), home.join("Applications")];
    let direct = roots.iter().map(|r| r.join(format!("{}.app", c.name))).find(|p| bundle_id_of(p).as_deref() == Some(c.bundle_id));
    direct.or_else(|| {
        let out = Command::new("/usr/bin/mdfind").arg(format!("kMDItemCFBundleIdentifier == '{}'", c.bundle_id)).output().ok()?;
        String::from_utf8_lossy(&out.stdout).lines().map(PathBuf::from).find(|p| bundle_id_of(p).as_deref() == Some(c.bundle_id))
    })
}

#[cfg(windows)]
pub fn installed_at(c: &Companion, _home: &Path) -> Option<PathBuf> {
    ["LOCALAPPDATA", "ProgramFiles"]
        .iter()
        .filter_map(|v| std::env::var_os(v))
        .map(|d| PathBuf::from(d).join(c.name).join(format!("{}.exe", c.exe)))
        .find(|p| p.is_file())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn installed_at(c: &Companion, home: &Path) -> Option<PathBuf> {
    let bins = ["/usr/bin", "/usr/local/bin", "/opt/bin"].iter().map(PathBuf::from).chain([home.join(".local/bin")]);
    let on_path = bins.map(|d| d.join(c.exe)).find(|p| p.is_file());
    on_path.or_else(|| appimage_in(&home.join("Applications"), c))
}

/// An AppImage for `c` in `dir`: `Substrate Scribe.AppImage`, or the site's file name `Substrate-Scribe_<v>_....AppImage`.
#[cfg(all(unix, not(target_os = "macos")))]
fn appimage_in(dir: &Path, c: &Companion) -> Option<PathBuf> {
    let dashed = c.name.replace(' ', "-");
    fs::read_dir(dir).ok()?.filter_map(|e| e.ok().map(|e| e.path())).find(|p| {
        let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        n.ends_with(".AppImage") && (n.starts_with(c.name) || n.starts_with(&dashed))
    })
}

// ---------- the release ----------

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// Read `latest`'s answer for `platform`: a URL on `origin`, a size and a SHA-256, or a reason there is none.
pub fn release_from(answer: &Value, origin: &str, platform: &str) -> Result<Release, String> {
    let p = answer.get("platforms").and_then(|p| p.get(platform)).filter(|p| !p.is_null()).ok_or(format!("no {platform} build"))?;
    let s = |k: &str| p.get(k).and_then(Value::as_str).map(String::from);
    let (Some(version), Some(url), Some(size), Some(sha256)) = (s("version"), s("url"), p.get("size").and_then(Value::as_u64), s("sha256")) else {
        return Err("the answer lacks a url, size or sha256".into());
    };
    let sha256 = sha256.to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the sha256 is not 64 hex digits".into());
    }
    if !url.starts_with(&format!("{}/", origin.trim_end_matches('/'))) {
        return Err(format!("{url} is not on {origin}"));
    }
    if size == 0 || size > MAX_BYTES {
        return Err(format!("a size of {size} bytes"));
    }
    Ok(Release { version, url, size, sha256 })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .user_agent(concat!("SubstrateCensus/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn latest(c: &Companion) -> Result<Value, String> {
    let mut res = agent().get(&format!("{ORIGIN}/api/apps/{}/latest", c.app)).call().map_err(|e| e.to_string())?;
    res.body_mut().read_json::<Value>().map_err(|e| e.to_string())
}

fn download(url: &str, to: &Path, progress: &dyn Fn(u64)) -> Result<u64, String> {
    let mut res = agent().get(url).call().map_err(|e| e.to_string())?;
    let mut reader = res.body_mut().with_config().limit(MAX_BYTES + 1).reader();
    let mut out = fs::File::create(to).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut received, mut last) = (0u64, 0u64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        received += n as u64;
        if received - last >= 1 << 20 {
            last = received;
            progress(received);
        }
    }
    out.sync_all().map_err(|e| e.to_string())?;
    progress(received);
    Ok(received)
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

// ---------- install ----------

/// One step of an install, for the window: `checking`, `downloading`, `installing`, `opening`.
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub app: String,
    pub phase: &'static str,
    pub received: u64,
    pub total: Option<u64>,
}

/// Where an install goes. The real one: `Target::system()`; the tests point it at a temp folder and open nothing.
pub struct Target {
    /// macOS: the Applications folders to try in order. Linux: the folder the AppImage goes into.
    pub dirs: Vec<PathBuf>,
    /// Downloads and the mount point live here.
    pub work: PathBuf,
    pub open: bool,
}

impl Target {
    pub fn system() -> Self {
        let home = home();
        let dirs = if cfg!(target_os = "macos") { vec![PathBuf::from("/Applications"), home.join("Applications")] } else { vec![home.join("Applications")] };
        Self { dirs, work: std::env::temp_dir().join("substrate-census"), open: true }
    }
}

/// Install `c` (see the top of this file). Returns where it went, or one plain sentence for the person.
pub fn install(c: &Companion, t: &Target, progress: &dyn Fn(Progress)) -> Result<PathBuf, String> {
    let step = |phase: &'static str, received: u64, total: Option<u64>| progress(Progress { app: c.app.into(), phase, received, total });
    let plain = |what: &str| format!("{} could not be installed: {what}. Try again, or download it from gosubstrate.com/apps/{}/.", c.name, c.app);
    let platform = platform().ok_or_else(|| plain("there is no build for this computer"))?;
    step("checking", 0, None);
    let rel = latest(c).and_then(|a| release_from(&a, ORIGIN, platform)).map_err(|e| plain(&e))?;

    fs::create_dir_all(&t.work).map_err(|e| plain(&e.to_string()))?;
    let ext = rel.url.rsplit('.').next().unwrap_or("bin");
    let file = t.work.join(format!("{}-{}.{ext}", c.app, rel.version));
    struct Remove(PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _cleanup = Remove(file.clone());
    step("downloading", 0, Some(rel.size));
    let got = download(&rel.url, &file, &|r| step("downloading", r, Some(rel.size))).map_err(|_| plain("the download did not finish"))?;
    if got != rel.size || sha256_file(&file).ok().as_deref() != Some(rel.sha256.as_str()) {
        return Err(plain("the download did not match what gosubstrate.com published"));
    }
    step("installing", got, Some(rel.size));
    let at = place(c, &file, t).map_err(|e| plain(&e))?;
    if t.open {
        step("opening", got, Some(rel.size));
        if let Err(e) = open(&at) {
            eprintln!("census: installed {} but could not open it: {e}", at.display());
        }
    }
    Ok(at)
}

#[cfg(unix)]
fn writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else { return false };
    // SAFETY: a valid NUL-terminated path; access() only reads it.
    dir.is_dir() && unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// The first of `dirs` this user can write to; the last one is created when it is missing.
#[cfg(unix)]
fn first_writable(dirs: &[PathBuf]) -> Option<PathBuf> {
    for (i, d) in dirs.iter().enumerate() {
        if writable(d) {
            return Some(d.clone());
        }
        if i == dirs.len() - 1 && !d.exists() && fs::create_dir_all(d).is_ok() && writable(d) {
            return Some(d.clone());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn place(c: &Companion, dmg: &Path, t: &Target) -> Result<PathBuf, String> {
    let mnt = t.work.join(format!("mnt-{}", c.app));
    let _ = Command::new("/usr/bin/hdiutil").args(["detach", "-force"]).arg(&mnt).output();
    fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
    let ok = Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"])
        .arg(&mnt)
        .arg(dmg)
        .output()
        .map_err(|e| e.to_string())?;
    if !ok.status.success() {
        return Err("the disk image did not open".into());
    }
    // Detach on every way out.
    struct Detach(PathBuf);
    impl Drop for Detach {
        fn drop(&mut self) {
            let _ = Command::new("/usr/bin/hdiutil").arg("detach").arg(&self.0).output();
            let _ = Command::new("/usr/bin/hdiutil").args(["detach", "-force"]).arg(&self.0).output();
        }
    }
    let _mounted = Detach(mnt.clone());
    let src = fs::read_dir(&mnt)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "app") && bundle_id_of(p).as_deref() == Some(c.bundle_id))
        .ok_or("the disk image holds no such app")?;
    let dir = first_writable(&t.dirs).ok_or("no Applications folder can be written to")?;
    let name = src.file_name().ok_or("no app name")?.to_owned();
    let dest = dir.join(&name);
    if dest.exists() && bundle_id_of(&dest).as_deref() != Some(c.bundle_id) {
        return Err(format!("{} is another app", dest.display()));
    }
    // Copy beside it, then swap it in, so a copy that stops half way leaves nothing named like the app.
    let partial = dir.join(format!(".{}.census-partial", name.to_string_lossy()));
    let _ = fs::remove_dir_all(&partial);
    let copied = Command::new("/usr/bin/ditto").arg(&src).arg(&partial).output().map_err(|e| e.to_string())?;
    if !copied.status.success() {
        let _ = fs::remove_dir_all(&partial);
        return Err("the copy into Applications failed".into());
    }
    if dest.exists() {
        let _ = fs::remove_dir_all(&dest);
    }
    fs::rename(&partial, &dest).map_err(|e| {
        let _ = fs::remove_dir_all(&partial);
        e.to_string()
    })?;
    Ok(dest)
}

#[cfg(windows)]
fn place(c: &Companion, setup: &Path, _t: &Target) -> Result<PathBuf, String> {
    use std::os::windows::process::CommandExt;
    let status = Command::new(setup).arg("/S").creation_flags(0x0800_0000).status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("the installer stopped".into());
    }
    installed_at(c, &home()).ok_or_else(|| "the installer finished but the app is not where it should be".into())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn place(c: &Companion, appimage: &Path, t: &Target) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = first_writable(&t.dirs).ok_or("~/Applications cannot be written to")?;
    let dest = dir.join(format!("{}.AppImage", c.name));
    fs::copy(appimage, &dest).map_err(|e| e.to_string())?;
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn open(at: &Path) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("/usr/bin/open");
        c.arg(at);
        c
    } else {
        Command::new(at)
    };
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHA: &str = "0e9491fad8e7e8fe9ef82a2fe4296dca3abd4124d87337a053de9063f93a7cc2";

    fn answer(p: Value) -> Value {
        json!({ "app": "scribe", "platforms": { "mac-arm64": p, "mac-x64": null } })
    }

    #[test]
    fn platforms() {
        assert_eq!(platform_for("macos", "aarch64"), Some("mac-arm64"));
        assert_eq!(platform_for("macos", "x86_64"), Some("mac-x64"));
        assert_eq!(platform_for("windows", "x86_64"), Some("windows-x64"));
        assert_eq!(platform_for("linux", "x86_64"), Some("linux-x64"));
        assert_eq!(platform_for("linux", "aarch64"), None);
    }

    #[test]
    fn a_release_needs_our_origin_a_size_and_a_sha() {
        let good = json!({ "version": "1.3.10", "url": "https://gosubstrate.com/dl/apps/scribe/1.3.10/x.dmg", "size": 21, "sha256": SHA.to_uppercase() });
        let r = release_from(&answer(good.clone()), ORIGIN, "mac-arm64").unwrap();
        assert_eq!((r.version.as_str(), r.size, r.sha256.as_str()), ("1.3.10", 21, SHA));
        assert!(release_from(&answer(good.clone()), ORIGIN, "mac-x64").unwrap_err().contains("no mac-x64 build"));
        let mut other = good.clone();
        other["url"] = json!("https://evil.example/x.dmg");
        assert!(release_from(&answer(other), ORIGIN, "mac-arm64").is_err());
        let mut nosha = good.clone();
        nosha["sha256"] = json!(null);
        assert!(release_from(&answer(nosha), ORIGIN, "mac-arm64").is_err());
        let mut badsha = good.clone();
        badsha["sha256"] = json!("abc");
        assert!(release_from(&answer(badsha), ORIGIN, "mac-arm64").is_err());
        let mut huge = good;
        huge["size"] = json!(MAX_BYTES + 1);
        assert!(release_from(&answer(huge), ORIGIN, "mac-arm64").is_err());
    }

    #[test]
    fn sha256_of_a_file() {
        let d = std::env::temp_dir().join(format!("census-sha-{}", std::process::id()));
        fs::write(&d, b"abc").unwrap();
        assert_eq!(sha256_file(&d).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = fs::remove_file(&d);
    }

    /// The real thing against gosubstrate.com, into a temp folder, opening nothing:
    /// `cargo test --lib companions -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn installs_scribe_from_the_site() {
        let root = std::env::temp_dir().join(format!("census-install-{}", std::process::id()));
        let t = Target { dirs: vec![root.join("Applications")], work: root.join("work"), open: false };
        let at = install(&COMPANIONS[0], &t, &|p| eprintln!("{} {} {}/{:?}", p.app, p.phase, p.received, p.total)).unwrap();
        eprintln!("installed at {}", at.display());
        assert!(at.starts_with(root.join("Applications")));
        #[cfg(target_os = "macos")]
        assert_eq!(bundle_id_of(&at).as_deref(), Some("com.gosubstrate.scribe"));
        let _ = fs::remove_dir_all(&root);
    }
}
