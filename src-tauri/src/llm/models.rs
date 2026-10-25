//! What is on disk, and what it costs.
//!
//! Deliberately the only place that decides whether a model counts as installed, and it decides it
//! by looking: the file named by [`ModelSpec::file`] either exists under [`dir`] or it does not.
//! There is no row in the database recording the answer, and that is the point — a row can say
//! "installed" about a file a user deleted from Finder, or "missing" about one restored from a
//! backup, and every such disagreement surfaces as the feature being mysteriously broken.
//!
//! `crate::download` is what makes this trustworthy: it writes to `<file>.part` and renames only
//! after the SHA-256 matches, so the presence of the final name is proof of a complete, verified
//! download rather than of an attempt.

use std::path::PathBuf;

use super::catalogue::ModelSpec;

/// Where weights live: `paths::models_dir()`, which is `state_dir()/models`.
///
/// Reserved for exactly this before there was anything to put in it, and already carved out of the
/// reset plan — see the doc comment on `paths::models_dir` and the test in `paths` that pins the
/// exclusion. A "reset the app" that silently discarded eight gigabytes the user waited on would
/// be the single most expensive mistake this feature could make.
pub fn dir() -> PathBuf {
    crate::paths::models_dir().join("llm")
}

/// Deletes partial downloads (`<file>.part`) nobody has touched in a week — a multi-gigabyte model
/// whose download was abandoned otherwise kept its half on disk for good. A week is long enough
/// for a paused download to be resumed; after that it starts over.
pub fn sweep_partials() {
    let Ok(entries) = std::fs::read_dir(dir()) else { return };
    let week = std::time::Duration::from_secs(7 * 24 * 3600);
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().ends_with(".part") {
            continue;
        }
        let stale = entry.metadata().and_then(|m| m.modified()).ok().and_then(|at| at.elapsed().ok()).is_some_and(|age| age > week);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn path_of(spec: &ModelSpec) -> PathBuf {
    dir().join(spec.file)
}

/// Whether `spec`'s weights are present and verified.
pub fn is_installed(spec: &ModelSpec) -> bool {
    complete(spec, &path_of(spec))
}

/// `path` holds all of `spec`'s bytes. The download only renames a file into place after its digest
/// matched, so the size is a cheap stand-in for it — and it catches a file that reached the final name
/// some other way cut short (found 2026-10-21: 72 MB of a 2.8 GB model, which llama-server then
/// refused as «corrupted or incomplete» on every start while the pane called it downloaded).
pub fn complete(spec: &ModelSpec, path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() == spec.size_bytes)
}

/// Bytes of a download that was interrupted, or `None` when there is no partial file.
///
/// Surfaced so the settings row can say "resume (1.2 GB of 1.5 GB)" instead of offering what looks
/// like a fresh download of a file that is nearly there.
pub fn partial_bytes(spec: &ModelSpec) -> Option<u64> {
    let part = dir().join(format!("{}.part", spec.file));
    std::fs::metadata(part).ok().map(|meta| meta.len()).filter(|&len| len > 0)
}

/// Total bytes of every model file and partial download under [`dir`].
///
/// Walked rather than summed from the catalogue, so weights left behind by a build whose catalogue
/// has since changed still show up. A user looking at "Models: 3.1 GB" wants the number the disk
/// would report, not the number this binary can account for.
pub fn disk_used() -> u64 {
    let Ok(entries) = std::fs::read_dir(dir()) else { return 0 };
    entries
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| meta.is_file())
        .map(|meta| meta.len())
        .sum()
}

/// Removes `spec`'s weights and any partial download of them.
///
/// The caller is responsible for stopping the engine first when the model being deleted is the one
/// it has open — see `commands::llm_cmd::llm_delete_model`. On Windows this is not advice:
/// a file mapped by a running process cannot be removed, so deleting the active model without
/// stopping it fails with a message about the file being in use.
pub fn delete(spec: &ModelSpec) -> Result<(), String> {
    let weights = path_of(spec);
    if weights.is_file() {
        std::fs::remove_file(&weights).map_err(|e| {
            format!("Couldn't delete {}: {e}", weights.display())
        })?;
    }
    // Best effort. A leftover `.part` costs disk but breaks nothing, and failing the whole delete
    // over it would leave the user unable to reclaim the gigabytes that actually matter.
    let _ = std::fs::remove_file(dir().join(format!("{}.part", spec.file)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_file_under_the_final_name_is_not_complete() {
        let spec = &super::super::catalogue::EXEC_CATALOGUE[0].spec;
        let root = std::env::temp_dir().join(format!("aim-llm-models-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp dir");
        let path = root.join(spec.file);
        std::fs::write(&path, b"short").expect("write");
        assert!(!complete(spec, &path));
        let _ = std::fs::remove_dir_all(&root);
    }
}
