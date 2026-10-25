//! Settings › AI: which CLI analyses meetings, how each one is found, the user's own CLIs, and a
//! one-line test that says whether the chosen one answers.
//!
//! The database lock is never held across an `.await`: every command reads or writes what it needs
//! in a block of its own and lets go before anything slow happens.

use serde::Serialize;
use tauri::State;

use crate::db::provider_queries::{self, CliProviderInput, CliProviderRow};
use crate::db::queries::{get_setting, set_setting};
use crate::db::Db;
use crate::providers::{self, key, runs, Provider, TextCall, DEFAULT_PROVIDER, LOCAL};

fn db_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn with_db<T>(db: &Db, f: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let conn = db.0.lock().map_err(db_err)?;
    f(&conn).map_err(db_err)
}

/// One built-in engine as Settings shows it: what it is, how the user configured it, and where its
/// binary resolves right now.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub default_binary: &'static str,
    /// The user's override, or empty.
    pub binary_path: String,
    /// The configured model, or empty for the CLI's own default.
    pub model: String,
    /// The resolved path of the binary that would run, or `None` when it is not found.
    pub found: Option<String>,
    pub setup_url: &'static str,
    pub install_command: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersList {
    pub builtins: Vec<BuiltinStatus>,
    pub custom: Vec<CliProviderRow>,
    /// The `ai_provider` setting: a built-in id, `local`, or a custom `cli-…` id.
    pub selected: String,
}

fn selected(conn: &rusqlite::Connection) -> rusqlite::Result<String> {
    Ok(get_setting(conn, key::PROVIDER)?.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| DEFAULT_PROVIDER.to_string()))
}

#[tauri::command]
pub async fn providers_list(db: State<'_, Db>) -> Result<ProvidersList, String> {
    let (configured, custom, selected) = with_db(&db, |conn| {
        let mut configured = Vec::new();
        for info in providers::builtins() {
            let binary_path = get_setting(conn, &key::binary_path(info.id))?.unwrap_or_default().trim().to_string();
            let model = get_setting(conn, &key::model(info.id))?.unwrap_or_default().trim().to_string();
            configured.push((info, binary_path, model));
        }
        Ok((configured, provider_queries::list(conn)?, selected(conn)?))
    })?;
    // Looking for six binaries is a few dozen `stat`s — off the async runtime all the same.
    let builtins = tauri::async_runtime::spawn_blocking(move || {
        configured
            .into_iter()
            .map(|(info, binary_path, model)| {
                let binary = if binary_path.is_empty() { info.default_binary } else { binary_path.as_str() };
                let found = providers::find_binary(binary).map(|path| path.to_string_lossy().into_owned());
                BuiltinStatus {
                    id: info.id,
                    label: info.label,
                    default_binary: info.default_binary,
                    binary_path,
                    model,
                    found,
                    setup_url: info.setup_url,
                    install_command: info.install_command,
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(db_err)?;
    Ok(ProvidersList { builtins, custom, selected })
}

/// Saves a built-in engine's binary override (empty for its default) and model (empty for the
/// CLI's own default).
#[tauri::command]
pub fn providers_set_builtin(db: State<'_, Db>, id: String, binary_path: String, model: String) -> Result<(), String> {
    let info = providers::builtin(id.trim()).ok_or_else(|| format!("unknown AI provider '{id}'"))?;
    with_db(&db, |conn| {
        set_setting(conn, &key::binary_path(info.id), binary_path.trim())?;
        set_setting(conn, &key::model(info.id), model.trim())
    })
}

#[tauri::command]
pub fn providers_save_custom(db: State<'_, Db>, input: CliProviderInput) -> Result<CliProviderRow, String> {
    if input.name.trim().is_empty() {
        return Err("give the provider a name".to_string());
    }
    if input.binary.trim().is_empty() {
        return Err("name the program to run".to_string());
    }
    if let Some(id) = input.id.as_deref().filter(|id| !id.is_empty()) {
        if !providers::is_custom_id(id) {
            return Err(format!("'{id}' is not a custom provider"));
        }
    }
    with_db(&db, |conn| provider_queries::save(conn, &input))
}

/// Deletes a custom provider; if it was the one selected, analysis goes back to the default.
#[tauri::command]
pub fn providers_delete_custom(db: State<'_, Db>, id: String) -> Result<(), String> {
    if !providers::is_custom_id(&id) {
        return Err(format!("'{id}' is not a custom provider"));
    }
    with_db(&db, |conn| {
        provider_queries::delete(conn, &id)?;
        if selected(conn)? == id {
            set_setting(conn, key::PROVIDER, DEFAULT_PROVIDER)?;
        }
        Ok(())
    })
}

/// Chooses what analyses meetings: a built-in id, `local`, or an existing custom id.
#[tauri::command]
pub fn providers_select(db: State<'_, Db>, id: String) -> Result<(), String> {
    let id = id.trim().to_string();
    with_db(&db, |conn| {
        let known = id == LOCAL || providers::builtin(&id).is_some() || (providers::is_custom_id(&id) && provider_queries::get(conn, &id)?.is_some());
        if !known {
            return Ok(Err(format!("unknown AI provider '{id}'")));
        }
        set_setting(conn, key::PROVIDER, &id).map(Ok)
    })?
}

/// How long the test waits before giving up — a signed-out CLI can sit at a login prompt forever.
const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Asks the provider for a one-word answer and returns what it said (or why it could not). Can be
/// stopped with `providers_cancel_run("test-<id>")`.
#[tauri::command]
pub async fn providers_test(db: State<'_, Db>, id: String) -> Result<String, String> {
    let (provider, model): (Provider, String) = {
        let conn = db.0.lock().map_err(db_err)?;
        providers::resolve(&conn, Some(&id))?
    };
    let run_id = format!("test-{}", provider.id());
    let token = runs::register(&run_id);
    let call = TextCall { system: "", ask: "Reply with exactly: OK", data: "", model: &model };
    let run = providers::run_text(&provider, call, token.clone());
    tokio::pin!(run);
    let outcome = tokio::select! {
        outcome = &mut run => outcome,
        _ = tokio::time::sleep(TEST_TIMEOUT) => {
            // Cancelled rather than dropped, so the process tree is stopped before this returns.
            token.cancel();
            let _ = run.await;
            Err(format!("{} did not answer within {} seconds", provider.label(), TEST_TIMEOUT.as_secs()))
        }
    };
    runs::finish(&run_id);
    outcome
}

/// Stops a call registered under `run_id` (see `providers::runs`), answering whether there was one.
#[tauri::command]
pub fn providers_cancel_run(run_id: String) -> Result<bool, String> {
    Ok(runs::cancel(&run_id))
}
