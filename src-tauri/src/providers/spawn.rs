//! The subprocess plumbing every engine shares: finding the binary, widening `PATH`, spawning in a
//! process group of its own, feeding stdin, draining both pipes, and stopping on cancel or timeout.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::{Invocation, CANCELLED};

/// The directories the AI CLI installers drop their binaries in — prepended to `PATH` (and
/// searched by [`resolve_binary`]) so a bare `claude`/`agy`/`opencode` resolves regardless of how
/// the app inherited its environment. On macOS a GUI app launched from Finder gets launchd's
/// minimal `PATH` (`shell_env` widens it to the login shell's, and this covers what no profile
/// mentions); on Windows an app already running when the CLI was installed keeps the stale
/// pre-install `PATH`. An absolute binary path set in Settings ignores all this.
#[cfg(not(target_os = "windows"))]
fn install_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".claude/local"));
        dirs.push(home.join(".opencode/bin"));
        // grok's install script puts it here and only a profile line puts it on `PATH`.
        dirs.push(home.join(".grok/bin"));
        dirs.push(home.join(".bun/bin"));
        dirs.push(home.join("Library/pnpm"));
        dirs.push(home.join(".npm-global/bin"));
        // Node version managers, by their stable alias for "the default version" rather than any
        // concrete one. fnm's own `PATH` entry is a per-shell-session symlink under
        // `fnm_multishells/<pid>_<timestamp>` that names a shell which has since exited.
        dirs.push(home.join(".local/share/fnm/aliases/default/bin")); // fnm
        dirs.push(home.join(".volta/bin")); // Volta
        dirs.push(home.join(".asdf/shims")); // asdf
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs
}

/// Windows carries more of the weight than its Unix counterpart: `PATH` is registry state, and a
/// process keeps the copy it started with — install a CLI while the app is running and it stays
/// invisible until a restart. The node-manager entries are where `npm i -g`, `pnpm add -g` and
/// `bun add -g` actually land, so a CLI installed a minute ago is found without one.
#[cfg(target_os = "windows")]
fn install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin")); // native installer (Claude)
        dirs.push(home.join(".claude").join("local"));
        dirs.push(home.join(".opencode").join("bin")); // opencode install script
        dirs.push(home.join(".grok").join("bin")); // grok install script
    }
    if let Some(appdata) = dirs::data_dir() {
        // npm's global bin (`dirs::data_dir()` is Roaming AppData).
        dirs.push(appdata.join("npm"));
        // fnm: Node on Windows puts `node.exe` and the `.cmd` shims in the prefix root.
        dirs.push(appdata.join("fnm").join("aliases").join("default"));
    }
    if let Some(local) = dirs::data_local_dir() {
        // Antigravity CLI (`agy`) installs to `%LOCALAPPDATA%\agy\bin`.
        dirs.push(local.join("agy").join("bin"));
        // The Codex desktop app ships the CLI here and does *not* put it on `PATH`.
        dirs.push(local.join("Programs").join("OpenAI").join("Codex").join("bin"));
        // pnpm's global bin (`PNPM_HOME`) and Volta's shim directory.
        dirs.push(local.join("pnpm"));
        dirs.push(local.join("Volta").join("bin"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".bun").join("bin"));
    }
    dirs
}

/// The install dirs followed by the process's current `PATH` — the ordered search space for both
/// setting the child's `PATH` and resolving the program name.
pub(crate) fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = install_dirs();
    if let Some(current) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&current));
    }
    dirs
}

fn apply_path(cmd: &mut Command, dirs: &[PathBuf]) {
    if let Ok(joined) = std::env::join_paths(dirs) {
        cmd.env("PATH", joined);
    }
}

/// Non-Windows: nothing to resolve — the child's widened `PATH` finds the binary and Unix has no
/// executable-extension quirk.
#[cfg(not(target_os = "windows"))]
pub(crate) fn resolve_binary(binary: &str, _dirs: &[PathBuf]) -> String {
    binary.to_string()
}

/// Windows: turn a bare command name into a full path *including its extension*. `CreateProcess`
/// only auto-appends `.exe`, so a Node CLI installed as a `<name>.cmd` shim — how `opencode`,
/// `cline` and npm's `claude` land — is invisible to a bare `Command::new("opencode")`. Resolving
/// to the full `.cmd` path lets `std::process::Command` route it through `cmd.exe` with correct
/// argument escaping. A real `.exe` is preferred over the `.cmd`/`.bat` shim. A name that already
/// has a path separator or extension is trusted as-is.
#[cfg(target_os = "windows")]
pub(crate) fn resolve_binary(binary: &str, dirs: &[PathBuf]) -> String {
    if binary.contains('/') || binary.contains('\\') || Path::new(binary).extension().is_some() {
        if !Path::new(binary).is_file() {
            if let Some(installed) = mid_update_fallback(binary) {
                return installed.to_string_lossy().into_owned();
            }
        }
        return binary.to_string();
    }
    for dir in dirs {
        for ext in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{binary}.{ext}"));
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    if let Some(installed) = mid_update_fallback(binary) {
        return installed.to_string_lossy().into_owned();
    }
    binary.to_string()
}

/// A Claude Code install caught part-way through replacing itself, and the version to run instead.
///
/// On Windows the native updater cannot overwrite a running `claude.exe`, so it renames it out of
/// the way and then copies the new executable in — and between the two (for good, when the copy
/// fails) `~\.local\bin\claude.exe` does not exist. Every version it installed is still on disk as
/// `<data>\claude\versions\<version>\claude.exe`, so the newest one runs in the meantime.
#[cfg(target_os = "windows")]
fn mid_update_fallback(binary: &str) -> Option<PathBuf> {
    let stem = Path::new(binary).file_stem()?.to_string_lossy().to_ascii_lowercase();
    if stem != "claude" {
        return None;
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".local").join("share")))?;
    newest_installed_version(&data.join("claude").join("versions"), "claude.exe")
}

/// The newest complete version under an installer's `versions` folder, as the file to execute.
/// Only names that are dot-separated numbers count, compared numerically.
#[cfg(target_os = "windows")]
fn newest_installed_version(root: &Path, executable: &str) -> Option<PathBuf> {
    let mut newest: Option<(Vec<u64>, PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(version) = name.split('.').map(|part| part.parse::<u64>().ok()).collect::<Option<Vec<u64>>>() else { continue };
        if version.len() < 2 {
            continue;
        }
        let file = entry.path().join(executable);
        if file.is_file() && newest.as_ref().is_none_or(|(seen, _)| version > *seen) {
            newest = Some((version, file));
        }
    }
    newest.map(|(_, file)| file)
}

/// Locates `binary` the same way a run will: an explicit path is checked as-is, a bare name is
/// looked up across the install dirs and `PATH` (trying Windows' executable extensions, since the
/// npm-installed CLIs land as `.cmd` shims). `None` means launching it would fail.
pub fn find_binary(binary: &str) -> Option<PathBuf> {
    let binary = binary.trim();
    if binary.is_empty() {
        return None;
    }
    let path = Path::new(binary);
    if path.is_absolute() || binary.contains('/') || binary.contains('\\') {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        #[cfg(target_os = "windows")]
        return mid_update_fallback(binary);
        #[cfg(not(target_os = "windows"))]
        return None;
    }
    #[cfg(target_os = "windows")]
    let extensions: &[&str] = &["exe", "cmd", "bat", ""];
    #[cfg(not(target_os = "windows"))]
    let extensions: &[&str] = &[""];

    for dir in search_dirs() {
        for ext in extensions {
            let candidate = if ext.is_empty() { dir.join(binary) } else { dir.join(format!("{binary}.{ext}")) };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(installed) = mid_update_fallback(binary) {
        return Some(installed);
    }
    None
}

/// Whether `program` is run through `cmd.exe`. `program` is the resolved path, which on Windows
/// carries its extension (see [`resolve_binary`]), so the extension is the whole answer — and
/// `cmd.exe` rejects any argument containing a newline ("batch file arguments are invalid").
pub(crate) fn is_batch_shim(program: &str) -> bool {
    let lower = program.trim().to_ascii_lowercase();
    lower.ends_with(".cmd") || lower.ends_with(".bat")
}

/// Keeps a Claude Code process this app starts from updating itself. The updater replaces the
/// executable in place, and on Windows that leaves a window in which `claude.exe` does not exist —
/// so a process started to summarise a meeting must never be the one that opens it under another
/// run. `DISABLE_AUTOUPDATER` is the switch the CLI reads. Set by file name, so a custom provider
/// pointed at `claude` is covered too; the Claude engine also sets it explicitly.
fn keep_claude_from_updating(cmd: &mut Command, program: &str) {
    let stem = Path::new(program)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if stem == "claude" {
        cmd.env("DISABLE_AUTOUPDATER", "1");
    }
}

/// What a launch that never started is reported as. The OS's own words stay, because they are what
/// a person searches for; a missing binary also says where to fix it.
fn launch_error(binary: &str, error: &std::io::Error) -> String {
    let mut message = format!("failed to launch '{binary}': {error}");
    if error.kind() == std::io::ErrorKind::NotFound {
        message.push_str(" — is it installed? Its path can be set under Settings › AI providers.");
    } else if cfg!(target_os = "windows") && matches!(error.raw_os_error(), Some(5) | Some(32) | Some(33)) {
        message.push_str(
            " — the executable is locked or being replaced (usually a self-update in progress, \
             sometimes an antivirus scan); try again in a few seconds",
        );
    }
    message
}

/// The pauses between attempts to start a binary that is momentarily locked or being replaced —
/// about nine seconds in all, which covers the copy of a ~200 MB executable and the antivirus scan
/// that follows it without making a genuinely broken install wait long to be reported.
const LAUNCH_RETRY_MS: [u64; 5] = [400, 800, 1_500, 2_500, 4_000];

/// Whether an OS error from starting a process is worth another attempt in a moment. Windows codes,
/// and meaningful only there: a sharing or lock violation (32, 33) or access denied (5) is a file
/// being written or replaced; a missing file (2, 3) counts only when the path was found a moment ago.
fn transient_launch_code(code: Option<i32>, resolved: bool) -> bool {
    match code {
        Some(5) | Some(32) | Some(33) => true,
        Some(2) | Some(3) => resolved,
        _ => false,
    }
}

/// Starts `cmd`, giving a Windows binary that is being swapped out under us a few seconds to come
/// back before calling it a failure. Stops waiting the moment the call is cancelled.
async fn spawn_patiently(
    cmd: &mut Command,
    binary: &str,
    program: &str,
    cancel: &CancellationToken,
) -> Result<tokio::process::Child, String> {
    let resolved = Path::new(program).is_absolute();
    let mut waits = LAUNCH_RETRY_MS.iter();
    loop {
        let error = match cmd.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => error,
        };
        let retry = cfg!(target_os = "windows") && transient_launch_code(error.raw_os_error(), resolved);
        match waits.next() {
            Some(ms) if retry => {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(*ms)) => {}
                    _ = cancel.cancelled() => return Err(CANCELLED.to_string()),
                }
            }
            _ => return Err(launch_error(binary, &error)),
        }
    }
}

/// How much of a pipe's *beginning* and *end* is kept for the engine's interpreter.
///
/// **Head *and* tail, because the interpreters read both ends**: `claude` and `agy` scan backwards
/// for the last `result` line (tail); `codex`, `grok` and `opencode` read a reply that a single
/// turn puts at the end and, on failure, an error that can be the first thing printed (head). The
/// elision can only ever land inside an event log, where the middle is chatter nobody parses.
const COLLECT_EDGE_CAP: usize = 2 * 1024 * 1024;

/// What a capped buffer says where the missing bytes were. Not JSON and not starting with `{`, so
/// every line-oriented interpreter skips it exactly as it skips a CLI's plain-text banners.
fn elision_marker(bytes: usize) -> String {
    format!("…[AIMeetings: {bytes} bytes of intermediate output omitted]…\n")
}

/// The head + tail of one pipe, bounded. See [`COLLECT_EDGE_CAP`].
#[derive(Default)]
struct Collected {
    head: Vec<u8>,
    tail: Vec<u8>,
    elided: usize,
}

impl Collected {
    fn push(&mut self, chunk: &[u8]) {
        if self.head.len() < COLLECT_EDGE_CAP {
            let room = COLLECT_EDGE_CAP - self.head.len();
            let take = room.min(chunk.len());
            self.head.extend_from_slice(&chunk[..take]);
            if take == chunk.len() {
                return;
            }
            self.tail.extend_from_slice(&chunk[take..]);
        } else {
            self.tail.extend_from_slice(chunk);
        }
        // Trimmed only once the tail is twice its allowance, so the memmove is paid once per
        // megabyte rather than once per read. The cut lands on a line boundary because the
        // interpreters parse lines: a tail starting mid-JSON would be a fragment that never parses.
        if self.tail.len() <= COLLECT_EDGE_CAP * 2 {
            return;
        }
        let overflow = self.tail.len() - COLLECT_EDGE_CAP;
        let cut = self.tail[overflow..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|at| overflow + at + 1)
            .unwrap_or(overflow);
        self.tail.drain(..cut);
        self.elided += cut;
    }

    fn finish(mut self) -> Vec<u8> {
        if self.elided == 0 {
            self.head.extend_from_slice(&self.tail);
            return self.head;
        }
        // The head was cut at a byte count, so its last line is half a line: dropped, and counted.
        let keep = self.head.iter().rposition(|b| *b == b'\n').map(|at| at + 1).unwrap_or(0);
        self.elided += self.head.len() - keep;
        self.head.truncate(keep);
        self.head.extend_from_slice(elision_marker(self.elided).as_bytes());
        self.head.extend_from_slice(&self.tail);
        self.head
    }
}

/// Drains one of the child's pipes into raw bytes. Bytes rather than a `String` accumulator on
/// purpose — a multi-byte character split across two reads would decode into replacement
/// characters if each chunk were decoded on its own.
async fn pump<R: tokio::io::AsyncRead + Unpin>(pipe: Option<R>) -> Vec<u8> {
    let mut collected = Collected::default();
    let Some(mut pipe) = pipe else { return collected.finish() };
    let mut buf = [0u8; 8192];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => collected.push(&buf[..n]),
        }
    }
    collected.finish()
}

/// A finished process, its output decoded and stripped of escape codes.
pub(crate) struct Finished {
    pub success: bool,
    /// `exit status: 1`, `signal: 9 (SIGKILL)` — what the interpreters quote in their errors.
    pub status: String,
    pub stdout: String,
    pub stderr: String,
}

/// Runs one prepared invocation to its end, or until `cancel` fires or `timeout` passes.
pub(crate) async fn run(
    binary: &str,
    program: &str,
    dirs: &[PathBuf],
    inv: Invocation,
    cancel: &CancellationToken,
    timeout: Duration,
) -> Result<Finished, String> {
    if cancel.is_cancelled() {
        return Err(CANCELLED.to_string());
    }
    // `proc::command` carries the no-console-window flag on Windows.
    let mut cmd = crate::proc::command(program);
    cmd.args(&inv.args);
    if let Some(dir) = &inv.cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    apply_path(&mut cmd, dirs);
    keep_claude_from_updating(&mut cmd, program);
    for (name, value) in &inv.env {
        cmd.env(name, value);
    }
    // The process group is what makes a cancel stop the *whole* call: these CLIs spawn helpers and
    // subagents of their own, and killing the one process we hold a handle to leaves them running.
    // `proc::kill_tree` is the other half.
    crate::proc::own_process_group(&mut cmd);
    // A safety net for a caller that drops this future instead of cancelling: the direct child at
    // least goes with it.
    cmd.kill_on_drop(true);

    let mut child = spawn_patiently(&mut cmd, binary, program, cancel).await?;

    // Feed stdin from a separate task, concurrently with draining the output: an engine that
    // *ignores* stdin would otherwise deadlock once the OS pipe buffer fills (here the write just
    // fails with `BrokenPipe` when the child exits, which is ignored), and one that reads it still
    // needs EOF — dropping the handle at the end of the task sends it.
    let stdin_content = inv.stdin;
    let mut stdin_handle = child.stdin.take();
    let writer = tokio::spawn(async move {
        if let Some(mut stdin) = stdin_handle.take() {
            if !stdin_content.is_empty() {
                let _ = stdin.write_all(stdin_content.as_bytes()).await;
            }
            // `stdin` drops here → EOF.
        }
    });
    // Both pipes drained concurrently with the wait: reading them only after the process exits
    // would deadlock any CLI whose output outgrows the OS pipe buffer.
    let stdout_task = tokio::spawn(pump(child.stdout.take()));
    let stderr_task = tokio::spawn(pump(child.stderr.take()));

    enum Stopped {
        Cancelled,
        TimedOut,
    }
    let ended = tokio::select! {
        status = child.wait() => Ok(status),
        _ = cancel.cancelled() => Err(Stopped::Cancelled),
        _ = tokio::time::sleep(timeout) => Err(Stopped::TimedOut),
    };
    let status = match ended {
        Ok(status) => status.map_err(|e| format!("failed to wait for '{binary}': {e}"))?,
        Err(stopped) => {
            crate::proc::kill_tree(&mut child).await;
            // The pumps end once the pipes close with the process. Bounded, though: a descendant
            // that left the process group can hold a pipe open indefinitely.
            let handles = [stdout_task.abort_handle(), stderr_task.abort_handle(), writer.abort_handle()];
            if tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(stdout_task, stderr_task, writer) })
                .await
                .is_err()
            {
                handles.iter().for_each(tokio::task::AbortHandle::abort);
            }
            return Err(match stopped {
                Stopped::Cancelled => CANCELLED.to_string(),
                Stopped::TimedOut => format!(
                    "{binary} did not finish within {} minutes and was stopped",
                    timeout.as_secs() / 60
                ),
            });
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let _ = writer.await;
    // Stripped here rather than per engine: every CLI colourizes, and no interpreter wants to see
    // escape bytes. Each raw buffer is dropped the moment its stripped copy exists.
    let stdout_text = super::text::strip_ansi(&String::from_utf8_lossy(&stdout));
    drop(stdout);
    let stderr_text = super::text::strip_ansi(&String::from_utf8_lossy(&stderr));
    drop(stderr);
    Ok(Finished { success: status.success(), status: status.to_string(), stdout: stdout_text, stderr: stderr_text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_shim_is_known_by_its_extension() {
        assert!(is_batch_shim(r"C:\Users\u\AppData\Roaming\npm\opencode.cmd"));
        assert!(is_batch_shim("X.BAT"));
        assert!(!is_batch_shim("/opt/homebrew/bin/opencode"));
        assert!(!is_batch_shim(r"C:\Users\u\.local\bin\claude.exe"));
    }

    #[test]
    fn a_small_output_comes_back_byte_for_byte() {
        let mut collected = Collected::default();
        collected.push(b"hola ");
        collected.push("ñandú\n".as_bytes());
        assert_eq!(collected.finish(), "hola ñandú\n".as_bytes());
    }

    /// A runaway stream keeps its head and its tail, cut at line boundaries, with a marker that no
    /// interpreter mistakes for an event.
    #[test]
    fn a_runaway_output_keeps_both_ends() {
        let mut collected = Collected::default();
        collected.push(b"{\"first\":true}\n");
        let line = format!("{}\n", "x".repeat(1023));
        for _ in 0..(7 * 1024) {
            collected.push(line.as_bytes());
        }
        collected.push(b"{\"type\":\"result\"}\n");
        let out = String::from_utf8(collected.finish()).unwrap();
        assert!(out.starts_with("{\"first\":true}\n"));
        assert!(out.ends_with("{\"type\":\"result\"}\n"));
        assert!(out.contains("bytes of intermediate output omitted"));
        assert!(out.len() < 5 * COLLECT_EDGE_CAP);
        for l in out.lines() {
            assert!(l.starts_with('x') || l.starts_with('{') || l.starts_with('…'), "a fragment survived: {l}");
        }
    }

    #[test]
    fn only_lock_and_vanished_file_errors_are_worth_waiting_for() {
        assert!(transient_launch_code(Some(32), false));
        assert!(transient_launch_code(Some(2), true));
        assert!(!transient_launch_code(Some(2), false), "a name that never resolved is not installed");
        assert!(!transient_launch_code(None, true));
    }

    #[test]
    fn an_empty_or_missing_binary_is_not_found() {
        assert!(find_binary("").is_none());
        assert!(find_binary("/definitely/not/here/claude").is_none());
        assert!(find_binary("aimeetings-no-such-cli-1b2c").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_binary_on_path_is_found() {
        assert!(find_binary("sh").is_some());
    }

    fn sh(script: &str, stdin: &str) -> Invocation {
        Invocation {
            args: vec!["-c".into(), script.into()],
            stdin: stdin.into(),
            cwd: None,
            env: vec![("AIMEETINGS_TEST".into(), "1".into())],
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stdin_reaches_the_process_and_both_pipes_come_back() {
        let dirs = search_dirs();
        let done = run("sh", "sh", &dirs, sh("cat; echo \"$AIMEETINGS_TEST\" >&2", "hola\n"), &CancellationToken::new(), Duration::from_secs(20))
            .await
            .unwrap();
        assert!(done.success);
        assert_eq!(done.stdout, "hola\n");
        assert_eq!(done.stderr.trim(), "1");
    }

    /// A process that ignores its stdin entirely must not deadlock a large write.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_ignores_stdin_does_not_deadlock() {
        let big = "x".repeat(4 * 1024 * 1024);
        let done = run("sh", "sh", &search_dirs(), sh("echo ok", &big), &CancellationToken::new(), Duration::from_secs(20))
            .await
            .unwrap();
        assert_eq!(done.stdout.trim(), "ok");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_cancel_stops_the_process_and_says_so() {
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            trigger.cancel();
        });
        let started = std::time::Instant::now();
        let error = run("sh", "sh", &search_dirs(), sh("sleep 30", ""), &cancel, Duration::from_secs(60))
            .await
            .err()
            .unwrap();
        assert_eq!(error, CANCELLED);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_hung_process_is_stopped_at_the_timeout() {
        let error = run("sh", "sh", &search_dirs(), sh("sleep 30", ""), &CancellationToken::new(), Duration::from_millis(300))
            .await
            .err()
            .unwrap();
        assert!(error.contains("did not finish"), "{error}");
    }

    #[tokio::test]
    async fn a_missing_binary_is_a_launch_error() {
        let error = run(
            "aimeetings-no-such-cli-1b2c",
            "aimeetings-no-such-cli-1b2c",
            &search_dirs(),
            sh("", ""),
            &CancellationToken::new(),
            Duration::from_secs(5),
        )
        .await
        .err()
        .unwrap();
        assert!(error.starts_with("failed to launch 'aimeetings-no-such-cli-1b2c'"), "{error}");
    }
}
