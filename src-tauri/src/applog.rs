//! The application log: one rotating file under the state root, so a failure on someone else's
//! machine can be read after the fact.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Rotate once the live file passes this, keeping exactly one previous generation
/// (`aimeetings.log` → `aimeetings.log.1`, overwriting whatever `.1` held).
///
/// 2 MB is roughly a week of ordinary use and comfortably more than any single startup or migration
/// writes, which is the span that has to survive intact: the whole point is that the user can be
/// asked for the log *after* the thing went wrong. One generation rather than five because the
/// failures this exists for are read within a day of happening; a log from three rotations ago
/// describes a version that is no longer installed.
const ROTATE_AT: u64 = 2 * 1024 * 1024;

struct Sink {
    file: Mutex<Option<File>>,
    path: PathBuf,
}

static SINK: OnceLock<Sink> = OnceLock::new();

/// Opens the log. Call once, from `run()`, before anything that might want to log — which today
/// means before the reset sweep and before the migration.
///
/// Idempotent and unfailing: called twice, the second call is ignored; unable to create the
/// directory or open the file, every later call becomes a no-op and the app proceeds. An app that
/// refused to start because it could not write its log would be a worse app than one that starts
/// without one.
pub fn init() {
    let path = crate::paths::logs_dir().join("aimeetings.log");
    let file = open(&path);
    let _ = SINK.set(Sink { file: Mutex::new(file), path });

    // Written unconditionally so that every session in the file starts with something that
    // identifies the build and the layout. A support log whose first line is a stack trace with no
    // version above it costs a round trip to make sense of.
    info(&format!(
        "AIMeetings {} starting · state={} · cache={}",
        env!("CARGO_PKG_VERSION"),
        crate::paths::state_dir().to_string_lossy(),
        crate::paths::cache_dir().to_string_lossy(),
    ));

    // After the banner, so the log always carries the version above whatever comes next — and after
    // the caller has had its chance to read the *previous* session's marker.
    mark_running();
}

/// The marker whose *presence* at startup means the last session never got to shut down.
///
/// Written after the log opens and deleted on a clean exit, so a launch that finds it left over is
/// a launch after a crash, a force-quit or a power cut. Cheap enough to be unconditional — one
/// zero-byte file per session — and it is the only evidence that survives the kind of death that
/// leaves nothing in the log, because the process never reached a line it could write.
fn running_marker() -> PathBuf {
    crate::paths::state_dir().join("session-running")
}

/// Whether the previous session ended without saying goodbye. Call once, at startup, *before*
/// [`init`] has had a chance to write this session's own marker.
pub fn last_session_was_unclean() -> bool {
    running_marker().exists()
}

/// Records that this session is running. Idempotent.
fn mark_running() {
    let path = running_marker();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, b"");
}

/// Records a clean shutdown. Anything that does not reach this leaves the marker behind, which is
/// exactly the signal [`last_session_was_unclean`] reads on the next launch.
pub fn mark_clean_exit() {
    info("clean shutdown");
    let _ = std::fs::remove_file(running_marker());
}

fn open(path: &PathBuf) -> Option<File> {
    let dir = path.parent()?;
    std::fs::create_dir_all(dir).ok()?;
    OpenOptions::new().create(true).append(true).open(path).ok()
}

pub fn info(message: &str) {
    write("INFO ", message);
}

pub fn warn(message: &str) {
    write("WARN ", message);
}

/// Routes every panic into the log file before the default handler runs.
///
/// Panics unwind here rather than aborting — `Cargo.toml` says why, at length — so a panic on a
/// worker thread kills that thread and leaves the app running, and a panic inside a Tauri command
/// leaves the caller's promise hanging forever. Both are *invisible* without this: there is no
/// console on Windows and no window early enough to show one, so the single most useful piece of
/// evidence about a hang went nowhere at all.
///
/// The default hook is kept and chained rather than replaced: it is what prints to stderr under
/// `tauri dev`, and losing that would trade one blind spot for another.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `payload_as_str` is still unstable, so the two shapes a panic payload actually takes are
        // matched by hand.
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let at = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("<unnamed>").to_string();
        error(&format!("panic on thread '{name}' at {at}: {payload}"));
        previous(info);
    }));
}

pub fn error(message: &str) {
    write("ERROR", message);
}

/// Appends one line, or silently does nothing.
///
/// `Local` rather than `Utc` for the timestamp, against the usual instinct. This log is read by one
/// person on one machine, correlating it with "it broke when I opened the app this morning"; a UTC
/// stamp makes them do arithmetic to answer the only question they have. The offset is printed too,
/// so a log pasted into an issue is still unambiguous.
///
/// A poisoned mutex is recovered from rather than propagated, the same way `terminal.rs` and
/// `secrets.rs` do: a thread that panicked while holding the log is not a reason for the next
/// thread to panic too, and unwinding is kept process-wide precisely so this is possible (see the
/// note on `panic = "abort"` in `Cargo.toml`).
fn write(level: &str, message: &str) {
    let Some(sink) = SINK.get() else { return };
    let mut guard = match sink.file.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(file) = guard.as_mut() else { return };

    if file.metadata().map(|m| m.len()).unwrap_or(0) >= ROTATE_AT {
        // Drop the handle before renaming: on Windows a file with an open handle cannot be renamed,
        // and a failed rotation that left the handle open would retry on every line from here on.
        *guard = None;
        let _ = std::fs::rename(&sink.path, sink.path.with_extension("log.1"));
        *guard = open(&sink.path);
        let Some(_) = guard.as_mut() else { return };
    }

    let Some(file) = guard.as_mut() else { return };
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f%:z");
    let _ = writeln!(file, "{stamp}  {level}  {message}");
    let _ = file.flush();
}

/// The tail of the current log, for the "copy diagnostics" button.
pub fn tail(max_bytes: usize) -> String {
    let path = crate::paths::logs_dir().join("aimeetings.log");
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let start = text.len().saturating_sub(max_bytes);
    let start = (start..text.len()).find(|&i| text.is_char_boundary(i)).unwrap_or(text.len());
    text[start..].to_string()
}
