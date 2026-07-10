//! Shared cooperative cancellation primitive.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe flag used to signal cooperative cancellation.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// Create a new uncancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Signal cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Return the shared atomic backing this token.
    pub fn inner(&self) -> &Arc<AtomicBool> {
        &self.0
    }
}
