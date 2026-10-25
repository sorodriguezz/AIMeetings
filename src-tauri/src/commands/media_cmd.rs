//! «Convertir video a audio»: a video (or any audio file) made into the light `.m4a` this app keeps —
//! one channel at the 16 kHz the speech models read, AAC at a speech bitrate. A meeting downloaded
//! as a video becomes something to import, share or keep, at a few tens of megabytes an hour.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::meetings::audio::{self, Channel};
use crate::meetings::encode;

const PROGRESS_EVENT: &str = "media:convert";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    run_id: String,
    /// `decode` while the video is read, `encode` while the audio is written.
    stage: &'static str,
    fraction: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Converted {
    /// What was written: `target`, or a WAV beside it when the system has no AAC encoder.
    path: String,
    bytes: u64,
    /// What the original weighed, to show what was saved.
    source_bytes: u64,
    duration_ms: i64,
}

#[tauri::command]
pub async fn media_extract_audio(app: AppHandle, path: String, target: String, run_id: String) -> Result<Converted, String> {
    let source = PathBuf::from(&path);
    if !source.is_file() {
        return Err(format!("{path} is not a file"));
    }
    let target = unused(PathBuf::from(&target));
    let source_bytes = std::fs::metadata(&source).map(|m| m.len()).unwrap_or(0);
    tokio::task::spawn_blocking(move || {
        let pieces = std::env::temp_dir().join(format!("aimeetings-convert-{}", uuid::Uuid::new_v4()));
        let emit = |stage: &'static str, fraction: f32| {
            let _ = app.emit(PROGRESS_EVENT, ProgressEvent { run_id: run_id.clone(), stage, fraction });
        };
        let result = (|| {
            let mut last = -1.0f32;
            encode::decode_to_pieces(&source, &pieces, &[Channel::Mic], |fraction| {
                if fraction - last >= 0.01 {
                    last = fraction;
                    emit("decode", fraction);
                }
            })?;
            emit("encode", 0.0);
            let written = encode::compress_into(&pieces, &[Channel::Mic], &target)?;
            let bytes = std::fs::metadata(&written).map(|m| m.len()).unwrap_or(0);
            let duration_ms = audio::channel_ms(&pieces, Channel::Mic);
            emit("encode", 1.0);
            Ok(Converted { path: written.to_string_lossy().into_owned(), bytes, source_bytes, duration_ms })
        })();
        let _ = std::fs::remove_dir_all(&pieces);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `target`, or `name (2).m4a`, `name (3).m4a`… when a file is already there: a conversion never
/// replaces something the user has.
fn unused(target: PathBuf) -> PathBuf {
    if !target.exists() {
        return target;
    }
    let stem = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = target.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "m4a".into());
    (2..)
        .map(|n| target.with_file_name(format!("{stem} ({n}).{ext}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversion_never_replaces_a_file() {
        let dir = std::env::temp_dir().join(format!("aim-unused-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("clase.m4a");
        assert_eq!(unused(target.clone()), target);
        std::fs::write(&target, b"x").unwrap();
        assert_eq!(unused(target.clone()), dir.join("clase (2).m4a"));
        std::fs::write(dir.join("clase (2).m4a"), b"x").unwrap();
        assert_eq!(unused(target), dir.join("clase (3).m4a"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
