use std::sync::Arc;

use tokio::sync::watch;

/// A counter a store bumps on every write, so a long-lived reader learns its copy went stale.
/// Clones share the counter.
#[derive(Debug, Clone)]
pub struct Revision(Arc<watch::Sender<u64>>);

impl Default for Revision {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(0)))
    }
}

impl Revision {
    pub fn bump(&self) {
        self.0.send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn watch(&self) -> watch::Receiver<u64> {
        self.0.subscribe()
    }
}
