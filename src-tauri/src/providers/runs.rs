//! The calls in flight, by an id the frontend chose, so a Stop button can reach the one it means.
//!
//! A caller [`register`]s a run id, passes the token to `run_text`, and [`finish`]es it when the
//! call is over — whatever the outcome. [`cancel`] fires the token; `run_text` then stops the
//! process tree and ends with `Err(CANCELLED)`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tokio_util::sync::CancellationToken;

fn registry() -> &'static Mutex<HashMap<String, CancellationToken>> {
    static RUNS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();
    RUNS.get_or_init(Default::default)
}

/// A poisoned lock is recovered from rather than propagated: a thread that panicked while holding
/// the registry is no reason for every later Stop to fail too.
fn with<T>(f: impl FnOnce(&mut HashMap<String, CancellationToken>) -> T) -> T {
    let mut guard = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

/// A fresh token for `run_id`. A run already registered under the same id is cancelled: two calls
/// cannot share one Stop button, and the older one would otherwise be unreachable.
pub fn register(run_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    if let Some(previous) = with(|runs| runs.insert(run_id.to_string(), token.clone())) {
        previous.cancel();
    }
    token
}

/// Forgets `run_id`. Idempotent.
pub fn finish(run_id: &str) {
    with(|runs| runs.remove(run_id));
}

/// Cancels `run_id`, answering whether there was such a run.
pub fn cancel(run_id: &str) -> bool {
    match with(|runs| runs.remove(run_id)) {
        Some(token) => {
            token.cancel();
            true
        }
        None => false,
    }
}

/// Whether `run_id` is registered right now.
pub fn is_running(run_id: &str) -> bool {
    with(|runs| runs.contains_key(run_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_run_can_be_cancelled_once() {
        let token = register("test-runs-a");
        assert!(is_running("test-runs-a"));
        assert!(cancel("test-runs-a"));
        assert!(token.is_cancelled());
        assert!(!cancel("test-runs-a"), "already gone");
    }

    #[test]
    fn a_finished_run_is_forgotten_and_not_cancelled() {
        let token = register("test-runs-b");
        finish("test-runs-b");
        assert!(!is_running("test-runs-b"));
        assert!(!cancel("test-runs-b"));
        assert!(!token.is_cancelled());
    }

    #[test]
    fn registering_the_same_id_again_cancels_the_older_run() {
        let older = register("test-runs-c");
        let newer = register("test-runs-c");
        assert!(older.is_cancelled() && !newer.is_cancelled());
        finish("test-runs-c");
    }
}
