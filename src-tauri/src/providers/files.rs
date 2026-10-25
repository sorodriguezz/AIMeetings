//! The files a prompt sometimes has to become, and the promise that none of them outlives its call.
//!
//! Several engines cannot always be handed their brief as an argument: Claude takes its system
//! prompt through `--system-prompt-file`, agy and Cline are pointed at a file their model then
//! reads, grok reads `--prompt-file`, opencode attaches the whole brief with `--file`, Codex reads
//! `model_instructions_file`. A brief is the whole prompt — here, a meeting's transcript — so it is
//! never written where anybody else could read it, and never left behind.
//!
//! Three rules:
//!
//! - **Private.** One directory per user (`aimeetings-ai-<uid>` under the temp directory), created
//!   `0700` and checked to be a real directory this user owns before anything is put in it — a name
//!   in a shared `/tmp` can be claimed first by somebody else, and a symlink there would aim our
//!   writes wherever its owner liked. Inside it, one directory per call; every file `0600`, created
//!   with `create_new` so an existing file is never written through.
//! - **Scoped to the call.** A [`CallFiles`] owns its directory and deletes it, with everything an
//!   engine wrote into it, when dropped — after the process has exited, because `run_text` holds it
//!   until then. By every route out, which is why it is a guard rather than a call.
//! - **Swept.** A process that dies without running destructors leaves its call directories behind.
//!   [`sweep_stale`] removes the ones older than any call can live, at the next launch.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// The per-user directory's name, under the temp directory.
///
/// The uid is in the name on Unix because `/tmp` is shared there: without it the first user to run
/// the app would own the one name every other user needs. Windows' temp directory is already the
/// user's own (`%LOCALAPPDATA%\Temp`), so the plain name is enough.
fn root_name() -> String {
    #[cfg(unix)]
    {
        // SAFETY: `getuid` takes nothing, cannot fail and has no side effects.
        format!("aimeetings-ai-{}", unsafe { libc::getuid() })
    }
    #[cfg(not(unix))]
    {
        "aimeetings-ai".to_string()
    }
}

/// Where the per-user directory may live, in order of preference: the temp directory, which every
/// one of these CLIs already reads from, then the app's own cache for the case the checks in
/// [`ensure_private`] exist for — the temp name taken by somebody else, or not a directory at all.
fn roots() -> Vec<PathBuf> {
    vec![std::env::temp_dir().join(root_name()), crate::paths::cache_dir().join("ai-prompts")]
}

/// The first usable root, resolved once per process.
fn base() -> Option<PathBuf> {
    static BASE: OnceLock<Option<PathBuf>> = OnceLock::new();
    let resolved = BASE.get_or_init(|| roots().into_iter().find(|root| ensure_private(root)));
    // Re-checked on every use rather than trusted: the user clearing their temp directory can take
    // it away from under a running app, and a call that then fell back to an inline prompt for no
    // visible reason would be a mystery.
    let dir = resolved.as_ref()?;
    ensure_private(dir).then(|| dir.clone())
}

/// Creates `dir` if it is missing and answers whether it is safe to write prompts into: a real
/// directory (never a symlink), owned by this user, and closed to everybody else.
fn ensure_private(dir: &Path) -> bool {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if builder.create(dir).is_err() {
        return false;
    }
    is_private(dir)
}

/// [`ensure_private`] without the creating: whether an existing `dir` is ours to use — and, which
/// is why the sweep asks this first, ours to delete from.
fn is_private(dir: &Path) -> bool {
    // `symlink_metadata`, not `metadata`: a symlink planted under our name must read as what it is.
    let Ok(meta) = std::fs::symlink_metadata(dir) else { return false };
    if !meta.file_type().is_dir() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // SAFETY: as in `root_name`.
        if meta.uid() != unsafe { libc::getuid() } {
            return false;
        }
        // Ours but opened up — by a umask of 000, or by hand. Closed again rather than refused.
        if meta.permissions().mode() & 0o077 != 0
            && std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).is_err()
        {
            return false;
        }
    }
    true
}

/// Writes `content` to a new file `dir/name`, readable by this user alone.
///
/// `create_new`: a file already at that path — which a name with a fresh UUID in it only ever is if
/// somebody put it there — is refused rather than written through.
pub fn write_into(dir: &Path, name: &str, content: &str) -> Option<PathBuf> {
    use std::io::Write;
    let path = dir.join(name);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).ok()?;
    if file.write_all(content.as_bytes()).is_err() {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path)
}

/// Deletes one path this module created, whatever it turned out to be. `remove_dir_all` does not
/// follow a symlink inside the tree — it removes the link — so a directory an engine wrote a link
/// into cannot take anything outside it down with it.
fn remove(path: &Path) {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => {
            let _ = std::fs::remove_dir_all(path);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(path);
        }
        Err(_) => {}
    }
}

/// What one call's engine wrote to hand over its prompt: a private directory of its own, created on
/// first use and deleted — whole — when this is dropped.
///
/// Every method answers `None` when the write failed. Each caller has an inline fallback, as the
/// original engines did: a prompt that cannot become a file is no reason to lose the call.
#[derive(Debug, Default)]
pub struct CallFiles {
    dir: OnceLock<Option<PathBuf>>,
}

impl CallFiles {
    pub fn new() -> Self {
        Self::default()
    }

    /// This call's directory, created on first use.
    fn root(&self) -> Option<&Path> {
        self.dir
            .get_or_init(|| {
                let dir = base()?.join(uuid::Uuid::new_v4().simple().to_string());
                ensure_private(&dir).then_some(dir)
            })
            .as_deref()
    }

    /// Writes `content` as a new private file named `<stem>-<uuid>.<extension>`.
    pub fn write(&self, stem: &str, extension: &str, content: &str) -> Option<PathBuf> {
        let root = self.root()?;
        write_into(root, &format!("{stem}-{}.{extension}", uuid::Uuid::new_v4().simple()), content)
    }

    /// A new, empty private directory named `<stem>-<uuid>` — agy's `--add-dir` scope, the empty
    /// working directory every text-only run is given.
    pub fn dir(&self, stem: &str) -> Option<PathBuf> {
        let dir = self.root()?.join(format!("{stem}-{}", uuid::Uuid::new_v4().simple()));
        ensure_private(&dir).then_some(dir)
    }
}

impl Drop for CallFiles {
    fn drop(&mut self) {
        if let Some(Some(dir)) = self.dir.get() {
            remove(dir);
        }
    }
}

/// How old a call directory has to be before the sweep takes it. Far past the longest a call may
/// run (`run_text`'s overall timeout), so a sweep can never race a call that is still reading.
const STALE_AFTER: Duration = Duration::from_secs(6 * 60 * 60);

/// Removes the call directories a crashed or force-quit session left behind. Run once at startup;
/// silent on every failure.
pub fn sweep_stale() {
    for root in roots() {
        sweep_root(&root, STALE_AFTER);
    }
}

fn sweep_root(root: &Path, age: Duration) {
    // Checked, never assumed: sweeping a directory somebody else planted under our name — or a
    // symlink pointing into the user's files — is the one way this function could do harm.
    if !is_private(root) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        if older_than(&entry.path(), age) {
            remove(&entry.path());
        }
    }
}

fn older_than(path: &Path, age: Duration) -> bool {
    std::fs::symlink_metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed >= age)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lifecycle the whole module exists for: written, there while the call holds it, gone the
    /// moment the call lets go — directories and what is inside them included.
    #[test]
    fn a_prompt_file_lives_exactly_as_long_as_its_call() {
        let files = CallFiles::new();
        let path = files.write("test-brief", "txt", "the whole transcript").expect("written");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "the whole transcript");
        let scope = files.dir("test-scope").expect("created");
        let inside = write_into(&scope, "brief.txt", "x").expect("written inside");
        drop(files);
        assert!(!path.exists() && !scope.exists() && !inside.exists());
    }

    /// A call that never wrote anything creates nothing.
    #[test]
    fn an_unused_call_leaves_no_directory() {
        let files = CallFiles::new();
        assert!(files.dir.get().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn prompts_are_readable_by_their_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let files = CallFiles::new();
        let path = files.write("test-mode", "txt", "secret").expect("written");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "got {mode:o}");
        let dir_mode = std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "got {dir_mode:o}");
    }

    /// Somebody else's file at the name is refused, never written through.
    #[test]
    fn an_existing_file_is_never_overwritten() {
        let files = CallFiles::new();
        let dir = files.dir("test-exists").unwrap();
        std::fs::write(dir.join("taken.txt"), "theirs").unwrap();
        assert!(write_into(&dir, "taken.txt", "ours").is_none());
        assert_eq!(std::fs::read_to_string(dir.join("taken.txt")).unwrap(), "theirs");
    }

    /// A symlink planted under our name is not a directory we may use — or sweep.
    #[cfg(unix)]
    #[test]
    fn a_symlink_is_never_taken_for_our_directory() {
        let files = CallFiles::new();
        let scratch = files.dir("test-link").unwrap();
        let target = scratch.join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "the user's").unwrap();
        let link = scratch.join("planted");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(!is_private(&link));
        assert!(!ensure_private(&link));
        sweep_root(&link, Duration::ZERO);
        assert!(target.join("keep.txt").exists(), "nothing behind the link was touched");
    }

    /// The startup sweep takes only what is old enough.
    #[test]
    fn the_sweep_removes_only_what_is_stale() {
        let files = CallFiles::new();
        let root = files.dir("test-sweep").unwrap();
        std::fs::create_dir(root.join("old")).unwrap();
        sweep_root(&root, Duration::from_secs(3600));
        assert!(root.join("old").exists(), "too young to be stale");
        sweep_root(&root, Duration::ZERO);
        assert!(!root.join("old").exists());
    }
}
