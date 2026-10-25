//! Battery saving: a level set from Settings that lets background work back off. Without the rest
//! of the original power module, this keeps only the gate the call detector and the local model
//! server read.

use std::sync::atomic::{AtomicU8, Ordering};

static SAVING: AtomicU8 = AtomicU8::new(0);

/// Whether saving is at least at `at` (1 = light, 2 = strong, 3 = maximum).
pub fn saving(at: u8) -> bool {
    SAVING.load(Ordering::Relaxed) >= at
}

pub fn set_saving(level: u8) {
    SAVING.store(level.min(3), Ordering::Relaxed);
}

/// How long a local model server may sit idle before it is let go: its own limit, or 2 min when
/// saving strongly.
pub fn idle_limit(normal: std::time::Duration) -> std::time::Duration {
    if saving(2) { normal.min(std::time::Duration::from_secs(120)) } else { normal }
}
