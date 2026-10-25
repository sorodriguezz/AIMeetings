use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where Tauri unpacked the bundled resources (the `llama-server` runtime). Recorded at startup so
/// the layers that need it stay free of Tauri types.
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Records that directory in its plain form — never Windows' verbatim `\\?\C:\…`, which some
/// non-Rust consumers read as a UNC share.
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCE_DIR.set(dunce::simplified(&dir).to_path_buf());
}

/// `None` outside a packaged app (a `cargo test`).
pub fn resource_dir() -> Option<&'static Path> {
    RESOURCE_DIR.get().map(PathBuf::as_path)
}

const APP_DIR: &str = "AIMeetings";

/// Redirects both roots under one directory — for tests and for profiles that do not persist.
const HOME_OVERRIDE: &str = "AIMEETINGS_HOME";

struct Roots {
    state: PathBuf,
    cache: PathBuf,
}

fn roots() -> &'static Roots {
    static ROOTS: OnceLock<Roots> = OnceLock::new();
    ROOTS.get_or_init(|| {
        if let Some(home) = std::env::var_os(HOME_OVERRIDE).filter(|v| !v.is_empty()) {
            let home = PathBuf::from(home);
            return Roots { state: home.join("state"), cache: home.join("cache") };
        }
        let fallback = || std::env::temp_dir().join(APP_DIR);
        // Windows keeps the cache under the state root: `dirs::cache_dir()` there is the same
        // `%LOCALAPPDATA%`, and a sibling `AIMeetings` folder would collide with the state one.
        #[cfg(target_os = "windows")]
        {
            let state = dirs::data_local_dir().map(|d| d.join(APP_DIR)).unwrap_or_else(fallback);
            Roots { cache: state.join("cache"), state }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let state = dirs::data_dir().map(|d| d.join(APP_DIR)).unwrap_or_else(fallback);
            let cache = dirs::cache_dir().map(|d| d.join(APP_DIR)).unwrap_or_else(|| state.join("cache"));
            Roots { state, cache }
        }
    })
}

/// The database, recordings, logs and models.
///
/// `%LOCALAPPDATA%\AIMeetings` · `~/Library/Application Support/AIMeetings` · `~/.local/share/AIMeetings`
pub fn state_dir() -> PathBuf {
    roots().state.clone()
}

/// Regenerable files only.
pub fn cache_dir() -> PathBuf {
    roots().cache.clone()
}

pub fn db_path() -> PathBuf {
    state_dir().join("aimeetings.db")
}

pub fn logs_dir() -> PathBuf {
    state_dir().join("logs")
}

/// Downloaded weights and engines: whisper.cpp, sherpa-onnx, speech and language models.
pub fn models_dir() -> PathBuf {
    state_dir().join("models")
}

/// The login shell's `PATH`, cached between launches (see `shell_env`). Not on Windows, which
/// probes no login shell.
#[cfg(not(target_os = "windows"))]
pub fn shell_path_cache() -> PathBuf {
    cache_dir().join("shell-path")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(state_dir())?;
    std::fs::create_dir_all(cache_dir())?;
    std::fs::create_dir_all(logs_dir())?;
    Ok(())
}
