//! Checks GitHub releases for a newer build, downloads it, and stages it to apply on exit
//! (`UpdateService`).
//!
//! Deliberately never applied and restarted mid-session. The agent is running because the user is
//! in a raid, and taking their team's vitals off the screen mid-fight is worse than shipping a fix
//! a session late.

use std::time::Duration;

use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateManager};

const RELEASES_URL: &str = "https://github.com/RustOverlay/rustoverlay-agent";

const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub async fn run() {
    // An error under `cargo run` and for anyone running from a build output: there is no install
    // for Velopack to replace, and every call below would fail.
    let manager = match UpdateManager::new(GithubSource::new(RELEASES_URL, None, false), None, None) {
        Ok(manager) => manager,
        Err(e) => {
            tracing::debug!(reason = %e, "Not a Velopack install; updates disabled");
            return;
        }
    };

    let mut timer = tokio::time::interval(INTERVAL);
    loop {
        timer.tick().await;

        let checking = manager.clone();
        match tokio::task::spawn_blocking(move || stage(&checking)).await {
            Ok(Ok(Some(version))) => {
                tracing::info!("Update {version} staged; applies when the agent closes");
                return;
            }
            Ok(Ok(None)) => {}
            // Unauthenticated GitHub allows 60 requests an hour per IP, so a check failing is
            // routine rather than exceptional and must never take the agent down with it.
            Ok(Err(e)) => tracing::warn!(error = %e, "Update check failed"),
            Err(e) => tracing::warn!(error = %e, "Update check failed"),
        }
    }
}

/// Velopack's calls block, so this runs off the async workers.
fn stage(manager: &UpdateManager) -> Result<Option<String>, velopack::Error> {
    let UpdateCheck::UpdateAvailable(update) = manager.check_for_updates()? else { return Ok(None) };

    let version = update.TargetFullRelease.Version.clone();
    manager.download_updates(&update, None)?;
    // As C#'s WaitExitThenApplyUpdates defaults: not silent, and relaunch once applied.
    manager.wait_exit_then_apply_updates(&update.TargetFullRelease, false, true, Vec::<String>::new())?;

    Ok(Some(version))
}
