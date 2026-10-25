//! The speech models: what is installed, installing and removing the engines (whisper.cpp,
//! sherpa-onnx) and their models, and the microphone (which inputs there are, whether the system
//! lets us use them). See `crate::dictation` and `crate::asr`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio_util::sync::CancellationToken;

use crate::dictation::{self, engine, permission, EngineSpec, Listener, WhisperModel};
use crate::download::{self, FetchError};

/// Progress of an install, for Settings: `item` is `"engine"` or a model's id.
pub const EVENT: &str = "dictation:download";

/// The install under way. One at a time: the engine and a model share one pane and one disk.
#[derive(Default)]
pub struct DictationRegistry {
    install: Mutex<Option<CancellationToken>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    item: String,
    /// `downloading` · `unpacking` · `done` · `failed` · `cancelled`.
    phase: &'static str,
    done: u64,
    total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn emit(app: &AppHandle, item: &str, phase: &'static str, done: u64, total: u64, error: Option<String>) {
    let _ = app.emit(EVENT, Progress { item: item.to_string(), phase, done, total, error });
}

#[tauri::command]
pub fn dictation_status() -> dictation::Status {
    dictation::status()
}

/// Downloads `model_id`, verified, with the library that runs it when that is not there yet: a
/// whisper model and whisper.cpp into `models/whisper/`, or Parakeet / Canary and sherpa-onnx into
/// `models/asr/`.
#[tauri::command]
pub async fn dictation_install(app: AppHandle, registry: State<'_, DictationRegistry>, model_id: String) -> Result<(), String> {
    let listener = dictation::listener(&model_id).ok_or_else(|| format!("Unknown dictation model: {model_id}"))?;
    let cancel = CancellationToken::new();
    {
        let mut slot = registry.install.lock().map_err(|e| e.to_string())?;
        if slot.is_some() {
            return Err("Another dictation download is under way".into());
        }
        *slot = Some(cancel.clone());
    }
    let result = match listener {
        Listener::Whisper(model) => match dictation::ENGINE {
            Some(spec) => install(&app, spec, model, &cancel).await,
            None => Err("Speech models are not available on this platform".into()),
        },
        Listener::Sherpa(model) => install_sherpa_model(&app, model, &cancel).await,
    };
    if let Ok(mut slot) = registry.install.lock() {
        *slot = None;
    }
    // Its speed here, measured rather than estimated (`voice_fit`).
    if result.is_ok() {
        crate::voice_fit::measure_in_background(&app, &model_id);
    }
    result
}

/// sherpa-onnx (when it is not in yet) and a Parakeet / Canary archive, on the model's row: one bar
/// for both, as a voice download draws it.
async fn install_sherpa_model(app: &AppHandle, model: &'static crate::asr::AsrModel, cancel: &CancellationToken) -> Result<(), String> {
    if !crate::asr::supported() {
        return Err("These models are not available on this computer".into());
    }
    let library = crate::meetings::SHERPA.map_or(0, |spec| spec.size);
    let need_library = crate::meetings::sherpa_library().is_none();
    let offset = if need_library { library } else { 0 };
    let total = model.size + offset;
    crate::meetings::ensure_sherpa(cancel, |done| emit(app, model.id, "downloading", done, total, None))
        .await
        .map_err(|e| settle(app, model.id, e))?;
    let archive = crate::asr::root().join("downloads").join(model.archive);
    download::fetch(model.url, model.size, model.sha256, &archive, cancel, |done, _| emit(app, model.id, "downloading", offset + done, total, None))
        .await
        .map_err(|e| settle(app, model.id, e))?;
    emit(app, model.id, "unpacking", total, total, None);
    let from = archive.clone();
    let unpacked = tokio::task::spawn_blocking(move || crate::asr::unpack(model, &from)).await.map_err(|e| e.to_string()).and_then(|r| r);
    if let Err(message) = unpacked {
        emit(app, model.id, "failed", 0, 0, Some(message.clone()));
        return Err(message);
    }
    let _ = std::fs::remove_file(&archive);
    emit(app, model.id, "done", total, total, None);
    Ok(())
}

/// What a failed or stopped download tells the pane, and the caller.
fn settle(app: &AppHandle, item: &str, error: FetchError) -> String {
    match error {
        FetchError::Cancelled => {
            emit(app, item, "cancelled", 0, 0, None);
            "cancelled".to_string()
        }
        FetchError::Failed(message) => {
            emit(app, item, "failed", 0, 0, Some(message.clone()));
            message
        }
    }
}

async fn install(app: &AppHandle, spec: EngineSpec, model: &'static WhisperModel, cancel: &CancellationToken) -> Result<(), String> {
    if dictation::engine_library().is_none() {
        let archive = dictation::root().join("downloads").join(spec.archive);
        download::fetch(spec.url, spec.size, spec.sha256, &archive, cancel, |done, total| emit(app, "engine", "downloading", done, total, None))
            .await
            .map_err(|e| settle(app, "engine", e))?;
        emit(app, "engine", "unpacking", spec.size, spec.size, None);
        let (from, to) = (archive.clone(), dictation::engine_dir());
        let unpacked = tokio::task::spawn_blocking(move || dictation::unpack_engine(&spec, &from, &to))
            .await
            .map_err(|e| e.to_string())
            .and_then(|result| result);
        if let Err(message) = unpacked {
            emit(app, "engine", "failed", 0, 0, Some(message.clone()));
            return Err(message);
        }
        // The archive carries every Apple platform (or every tool); what was kept is all that is needed.
        let _ = std::fs::remove_file(&archive);
        emit(app, "engine", "done", spec.size, spec.size, None);
    }
    let path = dictation::model_path(model);
    download::fetch(&model.url(), model.size_bytes, model.sha256, &path, cancel, |done, total| {
        emit(app, model.id, "downloading", done, total, None)
    })
    .await
    .map_err(|e| settle(app, model.id, e))?;
    emit(app, model.id, "done", model.size_bytes, model.size_bytes, None);
    Ok(())
}

#[tauri::command]
pub fn dictation_cancel_install(registry: State<DictationRegistry>) {
    if let Some(cancel) = registry.install.lock().ok().and_then(|slot| slot.clone()) {
        cancel.cancel();
    }
}

/// Deletes one model — and the engine with the last one, so "remove" leaves nothing of dictation
/// behind. The loaded model is freed first: Windows cannot delete a file a process has open.
#[tauri::command]
pub async fn dictation_remove(model_id: String) -> Result<(), String> {
    let model = match dictation::listener(&model_id).ok_or_else(|| format!("Unknown dictation model: {model_id}"))? {
        Listener::Whisper(model) => model,
        Listener::Sherpa(model) => {
            tokio::task::spawn_blocking(move || crate::asr::remove(model)).await.map_err(|e| e.to_string())?;
            // sherpa-onnx goes with the last thing that runs on it — a voice may still need it.
            if !crate::meetings::sherpa_still_needed() {
                crate::meetings::remove_sherpa();
            }
            return Ok(());
        }
    };
    tokio::task::spawn_blocking(engine::unload).await.map_err(|e| e.to_string())?;
    let path = dictation::model_path(model);
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| format!("Couldn't delete {}: {e}", path.display()))?;
    }
    let _ = std::fs::remove_file(path.with_file_name(format!("{}.part", model.file)));
    if dictation::MODELS.iter().all(|m| !dictation::model_path(m).is_file()) {
        // Best effort: a library this process has loaded cannot be deleted on Windows until it quits.
        let _ = std::fs::remove_dir_all(dictation::engine_dir());
        let _ = std::fs::remove_dir_all(dictation::root().join("downloads"));
    }
    Ok(())
}

/// The inputs Settings' picker offers — by name, without asking for anything first.
#[tauri::command]
pub async fn dictation_inputs() -> Vec<MicInput> {
    #[cfg(any(target_os = "macos", windows))]
    return tokio::task::spawn_blocking(dictation::capture::inputs).await.unwrap_or_default();
    #[cfg(not(any(target_os = "macos", windows)))]
    Vec::new()
}

#[cfg(any(target_os = "macos", windows))]
type MicInput = dictation::capture::InputDevice;
#[cfg(not(any(target_os = "macos", windows)))]
type MicInput = ();

/// Whether the system lets this app use the microphone, without asking.
#[tauri::command]
pub fn dictation_mic_permission() -> permission::Permission {
    permission::status()
}

/// The system's own question (macOS), when it has not been asked yet — and the answer.
#[tauri::command]
pub async fn dictation_mic_request() -> permission::Permission {
    tokio::task::spawn_blocking(permission::request).await.unwrap_or(permission::Permission::Unknown)
}

/// The system's microphone privacy page.
#[tauri::command]
pub fn dictation_mic_settings() -> Result<(), String> {
    permission::open_settings()
}

