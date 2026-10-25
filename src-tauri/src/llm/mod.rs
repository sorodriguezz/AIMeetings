//! The local language model: what writes minutes and answers questions when the analysis must
//! stay on this computer (a local-only meeting or folder) or the user picked "Local" as provider.
//!
//! Three backends, one protocol (`/v1/chat/completions`): the **bundled** `llama-server` running a
//! GGUF downloaded from [`catalogue`], **Ollama**, or any **OpenAI-compatible** server.

pub mod catalogue;
pub mod client;
pub mod models;
pub mod server;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::queries::get_setting;
use crate::hardware::{budget, machine};

pub mod key {
    /// `bundled` · `ollama` · `openai`.
    pub const BACKEND: &str = "llm_backend";
    /// A catalogue id for `bundled`, a model name otherwise.
    pub const MODEL: &str = "llm_model";
    /// The context window, in tokens; `0` = suggested for this machine.
    pub const CTX: &str = "llm_ctx";
    /// The server's address for `ollama` and `openai`.
    pub const URL: &str = "llm_url";
}

/// The keychain entry of an OpenAI-compatible server's key.
pub const API_KEY_SECRET: &str = "llm-api-key";

pub const OLLAMA_URL: &str = "http://127.0.0.1:11434";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Bundled,
    Ollama,
    Openai,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub backend: Backend,
    pub model: String,
    /// `0` = suggested.
    pub ctx: u32,
    pub url: String,
}

pub fn config(conn: &Connection) -> Config {
    let get = |name: &str| get_setting(conn, name).ok().flatten().unwrap_or_default();
    let backend = match get(key::BACKEND).trim() {
        "ollama" => Backend::Ollama,
        "openai" => Backend::Openai,
        _ => Backend::Bundled,
    };
    let url = match backend {
        Backend::Ollama if get(key::URL).trim().is_empty() => OLLAMA_URL.to_string(),
        _ => get(key::URL).trim().to_string(),
    };
    Config { backend, model: get(key::MODEL).trim().to_string(), ctx: get(key::CTX).trim().parse().unwrap_or(0), url }
}

/// A model ready to answer: where, which, and with how much window.
pub struct Live {
    pub endpoint: client::Endpoint,
    pub model: String,
    pub ctx: u32,
    pub label: String,
}

/// The window a bundled model runs with: the user's, or the largest that keeps it comfortable here.
pub fn bundled_ctx(entry: &catalogue::ExecModelSpec, chosen: u32) -> u32 {
    if chosen > 0 {
        return chosen.min(entry.max_ctx);
    }
    let machine = machine::machine_blocking();
    budget::suggest_ctx(Some(entry.spec.size_bytes), Some(entry.kv_bytes_per_token), Some(entry.max_ctx), 0, &machine)
}

/// Starts (or reuses) whatever [`config`] names. Waits for the bundled engine to load its model.
pub async fn connect(config: &Config) -> Result<Live, String> {
    match config.backend {
        Backend::Bundled => {
            let id = if config.model.is_empty() { catalogue::DEFAULT_EXEC_MODEL_ID } else { config.model.as_str() };
            let entry = catalogue::find(id).ok_or_else(|| format!("The local model {id} isn't in this version's list. Pick another under Models."))?;
            if entry.cuda_only {
                return Err(format!("{} needs an NVIDIA card through Ollama; the bundled engine can't run it.", entry.spec.label));
            }
            let ctx = tokio::task::spawn_blocking({
                let chosen = config.ctx;
                move || bundled_ctx(entry, chosen)
            })
            .await
            .map_err(|e| e.to_string())?;
            let machine = tokio::task::spawn_blocking(machine::machine_blocking).await.map_err(|e| e.to_string())?;
            let shape = budget::Shape { bytes: entry.spec.size_bytes, kv_bytes_per_token: entry.kv_bytes_per_token, active_share: entry.active_share() };
            let launch = server::Launch {
                model_id: entry.spec.id.to_string(),
                ctx,
                no_reasoning: true,
                cpu_moe_layers: budget::cpu_moe_layers(&shape, entry.moe_layers, ctx, 0, &machine),
            };
            let engine = server::ensure_ready(entry, models::path_of(&entry.spec), launch).await?;
            Ok(Live { endpoint: client::Endpoint::new(&engine.base_url(), None), model: entry.spec.id.to_string(), ctx, label: entry.spec.label.to_string() })
        }
        Backend::Ollama | Backend::Openai => {
            if config.model.is_empty() {
                return Err("Choose which model the local server should use, under Models › Analysis.".into());
            }
            let key = if config.backend == Backend::Openai { crate::secrets::get_secret(API_KEY_SECRET).ok().flatten() } else { None };
            let url = if config.url.is_empty() { OLLAMA_URL } else { config.url.as_str() };
            Ok(Live {
                endpoint: client::Endpoint::new(url, key),
                model: config.model.clone(),
                ctx: if config.ctx > 0 { config.ctx } else { 16_384 },
                label: config.model.clone(),
            })
        }
    }
}
