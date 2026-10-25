//! The Models screen: this machine, every model the app can download — speech, voices, language —
//! with what it weighs, what it needs, how fast it would run here and which one is recommended, and
//! the downloads themselves.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::db::{queries, Db};
use crate::download::{self, FetchError};
use crate::hardware::{budget, machine};
use crate::llm::{self, catalogue, models};

/// Progress of a language model's download: `{ id, phase, done, total, error }`.
pub const LLM_DOWNLOAD_EVENT: &str = "llm:download";

#[derive(Default)]
pub struct LlmRegistry {
    install: Mutex<Option<(String, CancellationToken)>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineInfo {
    /// `apple` · `pc` · `modest` — what the speech estimates are read for.
    class: crate::voice_fit::Class,
    cpu: String,
    /// Physical cores.
    cores: usize,
    /// Logical processors.
    threads: usize,
    ram_bytes: u64,
    ram_bandwidth: Option<u64>,
    /// `unified` · `discrete` · `integrated` · `unknown`.
    gpu: machine::GpuKind,
    gpu_name: Option<String>,
    /// VRAM, or the share of unified memory the GPU may use.
    gpu_bytes: Option<u64>,
    os: &'static str,
    arch: &'static str,
    /// Bytes the downloaded models take on disk.
    models_bytes: u64,
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

fn machine_info() -> MachineInfo {
    let hardware = machine::machine_blocking();
    let voice = crate::voice_fit::machine();
    MachineInfo {
        class: voice.class,
        cpu: voice.cpu,
        cores: voice.cores,
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(voice.cores),
        ram_bytes: hardware.ram_bytes,
        ram_bandwidth: hardware.ram_bandwidth,
        gpu: hardware.gpu,
        gpu_name: hardware.gpu_name.or(voice.gpu),
        gpu_bytes: hardware.gpu_bytes,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        models_bytes: dir_size(&crate::paths::models_dir()),
    }
}

/// One language model, judged for this machine at the window it would run with.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmRow {
    id: &'static str,
    label: &'static str,
    params: &'static str,
    licence: &'static str,
    licence_url: &'static str,
    size_bytes: u64,
    installed: bool,
    partial_bytes: Option<u64>,
    /// Weights + the window's KV cache + working memory, in bytes.
    need_bytes: u64,
    /// The window the figures are for.
    ctx: u32,
    max_ctx: u32,
    fit: budget::Fit,
    /// Estimated tokens a second; `None` when a memory's speed is unknown.
    write_tps: Option<f64>,
    pace: budget::Pace,
    /// Share of the model llama.cpp would keep on the GPU (0–1).
    gpu_share: f64,
    recommended: bool,
    /// Runs only through Ollama on an NVIDIA card.
    cuda_only: bool,
    active: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    machine: MachineInfo,
    speech: crate::dictation::Status,
    fit: crate::voice_fit::Fit,
    voices: crate::meetings::Status,
    llm: Vec<LlmRow>,
    llm_config: llm::Config,
    /// Whether this install carries the llama.cpp engine at all.
    llm_engine: bool,
    llm_status: llm::server::Status,
}

fn llm_rows(config: &llm::Config, hardware: &machine::Machine) -> Vec<LlmRow> {
    let active_id = if config.model.is_empty() { catalogue::DEFAULT_EXEC_MODEL_ID } else { config.model.as_str() };
    let judged: Vec<(LlmRow, budget::Fit, Option<f64>)> = catalogue::EXEC_CATALOGUE
        .iter()
        .map(|entry| {
            // Judged at the same window for every row, so the rows compare; the chosen one at its own.
            let ctx = if config.ctx > 0 && entry.spec.id == active_id { config.ctx.min(entry.max_ctx) } else { 16_384.min(entry.max_ctx) };
            let shape = budget::Shape { bytes: entry.spec.size_bytes, kv_bytes_per_token: entry.kv_bytes_per_token, active_share: entry.active_share() };
            let need = budget::memory_need(entry.spec.size_bytes, entry.kv_bytes_per_token, ctx);
            let fit = if entry.cuda_only && hardware.gpu != machine::GpuKind::Discrete { budget::Fit::DoesNotFit } else { budget::fit(need, 0, hardware) };
            let tps = budget::write_speed(&shape, ctx, 0, hardware);
            let row = LlmRow {
                id: entry.spec.id,
                label: entry.spec.label,
                params: entry.spec.params,
                licence: entry.spec.licence,
                licence_url: entry.licence_url,
                size_bytes: entry.spec.size_bytes,
                installed: models::is_installed(&entry.spec),
                partial_bytes: models::partial_bytes(&entry.spec),
                need_bytes: need,
                ctx,
                max_ctx: entry.max_ctx,
                fit,
                write_tps: tps,
                pace: budget::pace(tps),
                gpu_share: budget::gpu_share(&shape, ctx, 0, hardware),
                recommended: false,
                cuda_only: entry.cuda_only,
                active: config.backend == llm::Backend::Bundled && entry.spec.id == active_id,
            };
            (row, fit, tps)
        })
        .collect();
    let candidates: Vec<(budget::Fit, Option<f64>)> =
        judged.iter().map(|(row, fit, tps)| if row.cuda_only { (budget::Fit::DoesNotFit, None) } else { (*fit, *tps) }).collect();
    let pick = budget::recommend(&candidates);
    judged
        .into_iter()
        .enumerate()
        .map(|(index, (mut row, _, _))| {
            row.recommended = Some(index) == pick;
            row
        })
        .collect()
}

#[tauri::command]
pub async fn models_overview(db: State<'_, Db>) -> Result<Overview, String> {
    let (fit, config) = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        (crate::voice_fit::fit(&conn), llm::config(&conn))
    };
    let (machine, rows) = tokio::task::spawn_blocking(move || {
        let hardware = machine::machine_blocking();
        let rows = llm_rows(&config, &hardware);
        (machine_info(), (rows, config))
    })
    .await
    .map_err(|e| e.to_string())?;
    let (llm, llm_config) = rows;
    Ok(Overview {
        machine,
        speech: crate::dictation::status(),
        fit,
        voices: crate::meetings::status(),
        llm,
        llm_config,
        llm_engine: llm::server::is_available(),
        llm_status: llm::server::status(),
    })
}

/// Measures a speech model on this machine (a three-second utterance) and stores the result.
#[tauri::command]
pub async fn speech_measure(app: AppHandle, model_id: String) -> Result<crate::voice_fit::Measured, String> {
    use tauri::Manager;
    let id = model_id.clone();
    let measured = tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        crate::voice_fit::measure_and_store(&db, &id)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(measured)
}

// ---------------------------------------------------------------------------------------------
// Language models
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LlmProgress {
    id: String,
    phase: &'static str,
    done: u64,
    total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn emit_llm(app: &AppHandle, id: &str, phase: &'static str, done: u64, total: u64, error: Option<String>) {
    let _ = app.emit(LLM_DOWNLOAD_EVENT, LlmProgress { id: id.into(), phase, done, total, error });
}

/// Downloads a language model (resumable, verified). One at a time.
#[tauri::command]
pub async fn llm_install(app: AppHandle, registry: State<'_, LlmRegistry>, model_id: String) -> Result<(), String> {
    let entry = catalogue::find(&model_id).ok_or_else(|| format!("Unknown model: {model_id}"))?;
    let cancel = CancellationToken::new();
    {
        let mut slot = registry.install.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("Another model is downloading".into());
        }
        *slot = Some((model_id.clone(), cancel.clone()));
    }
    let spec = entry.spec;
    let result = download::fetch(&spec.url(), spec.size_bytes, spec.sha256, &models::path_of(&spec), &cancel, |done, total| {
        emit_llm(&app, spec.id, "downloading", done, total, None)
    })
    .await;
    if let Ok(mut slot) = registry.install.lock() {
        *slot = None;
    }
    match result {
        Ok(()) => {
            emit_llm(&app, spec.id, "done", spec.size_bytes, spec.size_bytes, None);
            Ok(())
        }
        Err(FetchError::Cancelled) => {
            emit_llm(&app, spec.id, "cancelled", 0, 0, None);
            Err("cancelled".into())
        }
        Err(FetchError::Failed(message)) => {
            emit_llm(&app, spec.id, "failed", 0, 0, Some(message.clone()));
            Err(message)
        }
    }
}

#[tauri::command]
pub fn llm_cancel_install(registry: State<LlmRegistry>) {
    if let Some((_, cancel)) = registry.install.lock().ok().and_then(|slot| slot.clone()) {
        cancel.cancel();
    }
}

/// Deletes a model's weights — stopping the engine first when it has that model open (Windows
/// cannot delete a mapped file).
#[tauri::command]
pub async fn llm_delete(model_id: String) -> Result<(), String> {
    let entry = catalogue::find(&model_id).ok_or_else(|| format!("Unknown model: {model_id}"))?;
    if llm::server::running_model().as_deref() == Some(entry.spec.id) {
        llm::server::shutdown().await;
    }
    models::delete(&entry.spec)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInput {
    backend: llm::Backend,
    model: String,
    ctx: u32,
    url: String,
    /// `Some("")` clears the key; `None` leaves it.
    api_key: Option<String>,
}

#[tauri::command]
pub fn llm_set_config(db: State<Db>, config: LlmConfigInput) -> Result<(), String> {
    let conn = db.0.lock().map_err(|e| e.to_string())?;
    let backend = match config.backend {
        llm::Backend::Bundled => "bundled",
        llm::Backend::Ollama => "ollama",
        llm::Backend::Openai => "openai",
    };
    for (key, value) in [(llm::key::BACKEND, backend.to_string()), (llm::key::MODEL, config.model.trim().to_string()), (llm::key::CTX, config.ctx.to_string()), (llm::key::URL, config.url.trim().to_string())] {
        queries::set_setting(&conn, key, &value).map_err(|e| e.to_string())?;
    }
    match config.api_key.as_deref().map(str::trim) {
        Some("") => crate::secrets::delete_secret(llm::API_KEY_SECRET)?,
        Some(key) => crate::secrets::set_secret(llm::API_KEY_SECRET, key)?,
        None => {}
    }
    Ok(())
}

/// The models a local server (Ollama, or the user's OpenAI-compatible one) offers.
#[tauri::command]
pub async fn llm_server_models(backend: llm::Backend, url: String) -> Result<Vec<String>, String> {
    let key = if backend == llm::Backend::Openai { crate::secrets::get_secret(llm::API_KEY_SECRET).ok().flatten() } else { None };
    let url = if url.trim().is_empty() { llm::OLLAMA_URL.to_string() } else { url };
    llm::client::list_models(&llm::client::Endpoint::new(&url, key)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    model: String,
    /// Tokens a second it wrote.
    write_tps: f64,
    seconds: f64,
}

/// Starts the configured local model and times a short answer — the measured speed, beside the
/// estimate.
#[tauri::command]
pub async fn llm_probe(db: State<'_, Db>) -> Result<Probe, String> {
    let config = {
        let conn = db.0.lock().map_err(|e| e.to_string())?;
        llm::config(&conn)
    };
    let live = llm::connect(&config).await?;
    let request = llm::client::ChatRequest {
        model: &live.model,
        system: "You are a helpful assistant.",
        user: "Escribe tres frases sobre por qué conviene tomar actas de las reuniones.",
        max_tokens: 160,
        temperature: 0.2,
    };
    let started = std::time::Instant::now();
    let outcome = llm::client::chat(&live.endpoint, &request, &CancellationToken::new()).await?;
    let seconds = started.elapsed().as_secs_f64();
    let tokens = outcome.completion_tokens.unwrap_or((outcome.text.len() / 4) as u64) as f64;
    Ok(Probe { model: live.label, write_tps: if seconds > 0.0 { tokens / seconds } else { 0.0 }, seconds })
}
