use std::sync::Arc;

use ro_core::hosting::log_tail::{LogTail, RETRY_DELAY};
use ro_core::hosting::{AgentOptions, AgentStatus, GameEventBus, TailState};
use ro_core::install::RustInstallLocator;

/// Tails the client log for as long as the runtime runs, publishing parsed events to the bus
/// (`LogTailService`). Stopped by dropping the task; the caller then marks the status idle and
/// completes the bus.
pub async fn run(locator: Arc<dyn RustInstallLocator>, bus: GameEventBus, status: Arc<AgentStatus>, options: AgentOptions) {
    let mut tail = LogTail::new();
    loop {
        if let Err(e) = tail.session(locator.as_ref(), &bus, &status, &options).await {
            tracing::error!(error = %e, "Log tail faulted");
            status.set(TailState::Faulted, e.to_string());
        }

        // Back off, or a missing install spins the CPU.
        tokio::time::sleep(RETRY_DELAY).await;
    }
}
