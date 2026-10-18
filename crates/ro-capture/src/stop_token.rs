use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

/// Work given up because its [`StopToken`] was stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stopped;

/// Asks a capture thread to stop, and wakes it from a wait when it is asked. Stands in for the
/// host's `CancellationToken`; clones share one flag.
#[derive(Clone, Debug, Default)]
pub struct StopToken {
    shared: Arc<(Mutex<bool>, Condvar)>,
}

impl StopToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        let (stopped, wake) = &*self.shared;
        *stopped.lock().unwrap_or_else(PoisonError::into_inner) = true;
        wake.notify_all();
    }

    pub fn is_stopped(&self) -> bool {
        *self.shared.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sleeps for `timeout`, or less if stopped meanwhile. True when stopped.
    pub fn wait(&self, timeout: Duration) -> bool {
        let (stopped, wake) = &*self.shared;
        let guard = stopped.lock().unwrap_or_else(PoisonError::into_inner);
        let (guard, _) = wake
            .wait_timeout_while(guard, timeout, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn a_wait_runs_its_course_unless_stopped() {
        let token = StopToken::new();

        assert!(!token.wait(Duration::from_millis(5)));
        assert!(!token.is_stopped());
    }

    #[test]
    fn stopping_wakes_a_waiter_at_once() {
        let token = StopToken::new();
        let stopper = token.clone();
        let started = Instant::now();

        let thread = std::thread::spawn(move || token.wait(Duration::from_secs(30)));
        stopper.stop();

        assert!(thread.join().unwrap());
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(stopper.wait(Duration::from_secs(30)));
    }
}
