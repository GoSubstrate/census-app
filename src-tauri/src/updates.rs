//! The Tauri updater, asked once when the window opens (src/main.ts). The endpoint and the minisign public key are in
//! tauri.conf.json (`plugins.updater`): gosubstrate.com's `/api/apps/census/update/{target}/{arch}/{version}`, which
//! answers 204 until a newer signed build is registered there. An update installs only when the person presses
//! Update on the start screen, never mid-scan, and the app restarts into it.

use std::sync::Mutex;

use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::Shared;

/// The update the last check found, held until the person installs it.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// The newer version gosubstrate.com offers, or None. A failed check (offline, server down) is None too: the census
/// itself does not need the updater.
#[tauri::command]
pub async fn check_update(app: AppHandle, pending: State<'_, Pending>) -> Result<Option<String>, String> {
    let found = match app.updater() {
        Ok(u) => u.check().await.ok().flatten(),
        Err(_) => None,
    };
    let version = found.as_ref().map(|u| u.version.clone());
    *pending.0.lock().unwrap() = found;
    Ok(version)
}

/// Download, verify (the plugin refuses a bundle whose signature does not match the baked-in key) and install the
/// pending update, then restart into it.
#[tauri::command]
pub async fn install_update(app: AppHandle, pending: State<'_, Pending>, runner: State<'_, Shared>) -> Result<(), String> {
    if runner.busy.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Wait for the census to finish.".into());
    }
    let update = pending.0.lock().unwrap().take().ok_or("No update is waiting.")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("The update did not install: {e}"))?;
    app.restart();
}
