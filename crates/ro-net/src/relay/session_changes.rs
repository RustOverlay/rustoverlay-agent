use std::time::Duration;

use tokio::sync::watch;

/// Writes to the active team or the install token, either of which makes a live session stale.
pub(super) struct SessionChanges {
    team: watch::Receiver<u64>,
    token: watch::Receiver<u64>,
}

impl SessionChanges {
    pub(super) fn new(team: watch::Receiver<u64>, token: watch::Receiver<u64>) -> Self {
        Self { team, token }
    }

    /// Call before reading the stores, so a write that lands after the read still counts.
    pub(super) fn mark_seen(&mut self) {
        self.team.mark_unchanged();
        self.token.mark_unchanged();
    }

    /// Resolves on the next write to either store. Cancel-safe.
    pub(super) async fn next(&mut self) {
        tokio::select! {
            Ok(()) = self.team.changed() => {}
            Ok(()) = self.token.changed() => {}
            // Both stores dropped: nothing can change any more.
            else => std::future::pending::<()>().await,
        }
    }

    /// Waits up to `limit`, or for a change only when there is none. True when a change woke it.
    pub(super) async fn wait(&mut self, limit: Option<Duration>) -> bool {
        let Some(limit) = limit else {
            self.next().await;
            return true;
        };
        tokio::select! {
            () = self.next() => true,
            () = tokio::time::sleep(limit) => false,
        }
    }
}
