//! Small things every pane needs: settings, writing a file the user picked, revealing a folder,
//! and the diagnostics a bug report wants.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::db::{queries, Db};

/// Emitted when a setting changes, so every pane that reads it updates.
pub const SETTING_EVENT: &str = "settings:changed";

#[tauri::command]
pub fn get_settings(db: State<Db>, keys: Vec<String>) -> Result<HashMap<String, String>, String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    queries::get_settings(&conn, &keys).map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
struct SettingChanged {
    key: String,
    value: String,
}

#[tauri::command]
pub fn set_setting(app: AppHandle, db: State<Db>, key: String, value: String) -> Result<(), String> {
    {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        queries::set_setting(&conn, &key, &value).map_err(|e| e.to_string())?;
    }
    if key == "power_saving" {
        crate::power::set_saving(value.trim().parse().unwrap_or(0));
    }
    let _ = app.emit(SETTING_EVENT, SettingChanged { key, value });
    Ok(())
}

/// Writes `bytes` to a path the user chose in a save dialog — an exported transcript.
#[tauri::command(async)]
pub fn write_file_bytes(path: String, bytes: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, bytes).map_err(|e| format!("Couldn't write {path}: {e}"))
}

#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    let target = if path.is_dir() { path } else { path.parent().map(|p| p.to_path_buf()).unwrap_or(path) };
    open::that(&target).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("x-apple.systempreferences:") || url.starts_with("ms-settings:")) {
        return Err("Only web and system-settings links open from here".into());
    }
    open::that(&url).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    state_dir: String,
    log_tail: String,
}

#[tauri::command]
pub fn app_diagnostics() -> Diagnostics {
    Diagnostics {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        state_dir: crate::paths::state_dir().to_string_lossy().into_owned(),
        log_tail: crate::applog::tail(20_000),
    }
}

/// Right before an update is installed: nothing of ours may hold a file the installer replaces —
/// the local model server above all (Windows cannot overwrite a running executable). Refused while
/// a meeting is being recorded or transcribed; the window asks the user to wait.
#[tauri::command]
pub async fn prepare_update(app: AppHandle) -> Result<(), String> {
    if crate::commands::meetings_cmd::busy_count() > 0 {
        return Err("A meeting is being recorded or transcribed; update when it finishes.".into());
    }
    crate::llm::server::shutdown().await;
    crate::commands::meetings_cmd::shutdown(&app);
    Ok(())
}
