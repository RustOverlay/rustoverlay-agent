use chrono::{DateTime, TimeDelta, Utc};

/// Decides when to stop asking the preferred capture backend for frames, and when to try it again.
///
/// Startup probing cannot answer this on its own: duplication can be available on the primary
/// display and unusable on the one the game is actually on. The demotion is on a timer rather than
/// permanent because the reverse is just as common, and a session demoted for good would lose
/// exclusive-fullscreen capture for the rest of the run.
#[derive(Debug, Clone)]
pub struct BackendHealth {
    tolerated: i32,
    cool_down: TimeDelta,
    failures: i32,
    demoted_until: DateTime<Utc>,
}

impl BackendHealth {
    pub fn new(tolerated: i32, cool_down: TimeDelta) -> Self {
        Self { tolerated, cool_down, failures: 0, demoted_until: DateTime::<Utc>::MIN_UTC }
    }

    pub fn use_preferred(&self, now: DateTime<Utc>) -> bool {
        now >= self.demoted_until
    }

    /// Resets the run rather than decrementing it, so an intermittent failure never accumulates
    /// its way to a demotion over a long session.
    pub fn record_success(&mut self) {
        self.failures = 0;
    }

    /// Records a failed frame. True if this one demoted the backend, for logging once.
    pub fn record_failure(&mut self, now: DateTime<Utc>) -> bool {
        self.failures += 1;
        if self.failures < self.tolerated {
            return false;
        }

        self.failures = 0;
        self.demoted_until = now + self.cool_down;

        true
    }
}
