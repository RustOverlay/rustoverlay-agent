//! One tailing session of `LogTailService`, minus the retry loop that owns it (that is ro-app's).

use std::any::Any;
use std::future::Future;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use super::{AgentOptions, AgentStatus, ChannelClosed, GameEventBus, TailState};
use crate::install::{RustInstall, RustInstallLocator};
use crate::log_pipeline::LogPipeline;
use crate::logs::LogTailer;

/// How long to wait before looking again when there is nothing to tail. Back off, or a missing
/// install spins the CPU.
pub const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Lines handled between yields, so a replay of the whole log does not hold the worker thread.
const LINES_PER_YIELD: usize = 1024;

#[derive(Debug)]
pub enum TailError {
    Io(io::Error),
    Closed(ChannelClosed),
    Panicked(String),
}

impl std::fmt::Display for TailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Closed(e) => e.fmt(f),
            Self::Panicked(message) => write!(f, "the log tail panicked: {message}"),
        }
    }
}

impl std::error::Error for TailError {}

/// Finds the log to tail, reporting through `status` why there is none.
pub fn find_log(locator: &dyn RustInstallLocator, status: &AgentStatus, from_start: bool) -> Option<RustInstall> {
    let Some(install) = locator.locate() else {
        status.set(TailState::NoLog, "Rust install not found");
        return None;
    };

    if !install.has_log() {
        status.set(TailState::NoLog, "output_log.txt missing — remove -nolog from Steam launch options");
        return None;
    }

    let log = install.log_path().display().to_string();
    status.set(TailState::Tailing, if from_start { format!("{log} (replaying from start)") } else { log });
    tracing::info!(replay = from_start, "Tailing log");

    Some(install)
}

/// The read position, kept across sessions so a retry resumes where the fault left off instead of
/// skipping to the end (or, with `ReplayFromStart`, replaying everything again).
#[derive(Debug, Default)]
pub struct LogTail {
    tailer: Option<LogTailer>,
}

impl LogTail {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tails the log until it fails, publishing every parsed event. Returns `Ok` only when there
    /// was no log to tail; drop the future to stop. A panic comes back as [`TailError::Panicked`].
    /// The caller sets `Faulted` on error and retries after [`RETRY_DELAY`].
    pub async fn session(
        &mut self,
        locator: &dyn RustInstallLocator,
        bus: &GameEventBus,
        status: &AgentStatus,
        options: &AgentOptions,
    ) -> Result<(), TailError> {
        CatchUnwind(Box::pin(self.run(locator, bus, status, options)))
            .await
            .unwrap_or_else(|payload| Err(TailError::Panicked(panic_message(&*payload))))
    }

    async fn run(
        &mut self,
        locator: &dyn RustInstallLocator,
        bus: &GameEventBus,
        status: &AgentStatus,
        options: &AgentOptions,
    ) -> Result<(), TailError> {
        let from_start = options.replay_from_start && self.tailer.is_none();
        let Some(install) = find_log(locator, status, from_start) else { return Ok(()) };
        let log_path = install.log_path();

        // A different log is a different file: start it afresh, at its end, as C# did.
        if self.tailer.as_ref().is_some_and(|t| t.path() != log_path) {
            self.tailer = None;
        }
        let tailer = self
            .tailer
            .get_or_insert_with(|| LogTailer::new(log_path, options.poll_interval.to_std().ok(), from_start));

        let mut pipeline = LogPipeline::new(options);
        loop {
            let lines = tailer.next_lines().await.map_err(TailError::Io)?;
            for (i, line) in lines.iter().enumerate() {
                for event in pipeline.push_line(line) {
                    bus.publish(&event).await.map_err(TailError::Closed)?;
                }
                if (i + 1) % LINES_PER_YIELD == 0 {
                    tokio::task::yield_now().await;
                }
            }
        }
    }
}

/// `FutureExt::catch_unwind` without the dependency. Not polled again once it has panicked.
struct CatchUnwind<F>(Pin<Box<F>>);

impl<F: Future> Future for CatchUnwind<F> {
    type Output = Result<F::Output, Box<dyn Any + Send>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = self.0.as_mut();
        match panic::catch_unwind(AssertUnwindSafe(|| inner.poll(cx))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use chrono::TimeDelta;

    use super::*;
    use crate::events::GameEvent;
    use crate::hosting::{DeliveryMode, Subscription};

    struct At(PathBuf);

    impl RustInstallLocator for At {
        fn locate(&self) -> Option<RustInstall> {
            Some(RustInstall::new(&self.0))
        }
    }

    fn identity(n: u64) -> String {
        format!("2026-08-02T21:30:{n:02}.000Z|0x6e08|SteamID: 7656119000000000{n} (player{n})\n")
    }

    fn append(path: &Path, text: &str) {
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
    }

    fn names(subscription: &Subscription) -> Vec<String> {
        std::iter::from_fn(|| subscription.try_recv())
            .filter_map(|e| match e {
                GameEvent::Identity(i) => i.name,
                _ => None,
            })
            .collect()
    }

    /// Runs one session until it has had time to read what is there, then cancels it.
    async fn briefly(tail: &mut LogTail, locator: &At, bus: &GameEventBus, status: &AgentStatus, options: &AgentOptions) {
        let session = tail.session(locator, bus, status, options);
        assert!(tokio::time::timeout(Duration::from_millis(300), session).await.is_err());
    }

    fn setup(replay: bool, initial: &str) -> (tempfile::TempDir, At, AgentOptions) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("output_log.txt"), initial).unwrap();
        let options = AgentOptions { replay_from_start: replay, poll_interval: TimeDelta::milliseconds(5), ..Default::default() };
        let locator = At(dir.path().to_path_buf());
        (dir, locator, options)
    }

    #[tokio::test]
    async fn a_retry_resumes_where_the_last_session_stopped() {
        let (dir, locator, options) = setup(false, &identity(1));
        let (bus, status) = (GameEventBus::new(), AgentStatus::new());
        let events = bus.subscribe("test", 64, DeliveryMode::Lossy);
        let mut tail = LogTail::new();

        briefly(&mut tail, &locator, &bus, &status, &options).await;
        append(&dir.path().join("output_log.txt"), &(identity(2) + &identity(3)));
        briefly(&mut tail, &locator, &bus, &status, &options).await;

        assert_eq!(names(&events), ["player2"]);
    }

    #[tokio::test]
    async fn a_retry_does_not_replay_the_log_again() {
        let (_dir, locator, options) = setup(true, &(identity(1) + &identity(2)));
        let (bus, status) = (GameEventBus::new(), AgentStatus::new());
        let events = bus.subscribe("test", 64, DeliveryMode::Lossy);
        let mut tail = LogTail::new();

        briefly(&mut tail, &locator, &bus, &status, &options).await;
        assert_eq!(names(&events), ["player1"]);
        assert!(status.detail().ends_with("(replaying from start)"));

        briefly(&mut tail, &locator, &bus, &status, &options).await;
        assert!(names(&events).is_empty());
        assert!(!status.detail().contains("replaying"));
    }

    struct PanicsOnce(AtomicBool);

    impl RustInstallLocator for PanicsOnce {
        fn locate(&self) -> Option<RustInstall> {
            assert!(self.0.swap(true, Ordering::SeqCst), "locator exploded");
            None
        }
    }

    #[tokio::test]
    async fn a_panic_is_returned_as_an_error_so_the_caller_retries() {
        let locator = PanicsOnce(AtomicBool::new(false));
        let (bus, status, options) = (GameEventBus::new(), AgentStatus::new(), AgentOptions::default());
        let mut tail = LogTail::new();

        let first = tail.session(&locator, &bus, &status, &options).await;
        assert!(matches!(&first, Err(TailError::Panicked(m)) if m.contains("locator exploded")), "{first:?}");

        assert!(tail.session(&locator, &bus, &status, &options).await.is_ok());
        assert_eq!(status.state(), TailState::NoLog);
    }
}
