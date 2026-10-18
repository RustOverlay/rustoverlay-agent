//! Window-level state that is not a tab of its own: the tail status line, the client.cfg warnings
//! and the debug frame grab (`AgentViewModel`).

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ro_capture::{FrameSource, frame_image};
use ro_core::config::ClientConfig;
use ro_core::hosting::{AgentStatus, TailState};
use ro_core::install::RustInstallLocator;

use super::Notify;
use crate::format::grouped;

/// Read once at startup, because Rust only writes client.cfg on exit — switching to borderless
/// clears the fullscreen warning on the next launch and not before, which its text has to say.
pub fn load_client_config(locator: &dyn RustInstallLocator) -> ClientConfig {
    let Some(path) = locator.locate().map(|install| install.client_config_path()).filter(|p| p.is_file()) else {
        return ClientConfig::empty();
    };

    ClientConfig::load(&path).unwrap_or_else(|e| {
        // TODO(port): C# let this exception take down the view model, and with it the window.
        tracing::error!(error = %e, path = %path.display(), "Could not read client.cfg");
        ClientConfig::empty()
    })
}

/// A topmost window cannot composite over true exclusive fullscreen, so the panel would simply
/// never be seen. Says so rather than shipping a surface that silently fails to draw.
pub fn fullscreen_warning(config: &ClientConfig) -> Option<&'static str> {
    config.overlay_will_be_occluded().then_some(
        "graphics.screenmode is exclusive fullscreen — the overlay cannot draw over it. Switch to borderless, then restart the agent.",
    )
}

pub fn status_text(state: TailState, detail: &str, received: u64) -> String {
    match state {
        TailState::Idle => "Idle".into(),
        TailState::Tailing => format!("Tailing — {} events", grouped(received)),
        TailState::NoLog => detail.into(),
        TailState::Faulted => format!("Faulted — {detail}"),
    }
}

pub fn status_is_problem(status: &AgentStatus) -> bool {
    matches!(status.state(), TailState::NoLog | TailState::Faulted)
}

/// The line beside the grab button.
#[derive(Clone, Default)]
pub struct CaptureNotice {
    text: Arc<Mutex<String>>,
}

impl CaptureNotice {
    pub fn get(&self) -> String {
        self.text.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn set(&self, text: String, notify: &Notify) {
        *self.text.lock().unwrap_or_else(PoisonError::into_inner) = text;
        notify();
    }
}

/// Saves a frame of the Rust client for HUD calibration work, after a countdown.
///
/// The delay is not a nicety. Desktop capture reads whatever is composited at the window's
/// coordinates, so Rust must be the visible window at the moment of the grab — which it cannot be
/// while the user is clicking the button. Alt-tab during the countdown.
pub async fn grab_frame(frames: Arc<dyn FrameSource>, data_directory: &Path, notice: CaptureNotice, notify: Notify) {
    for remaining in (1..=5).rev() {
        notice.set(format!("Alt-tab to Rust — capturing in {remaining}…"), &notify);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let directory = data_directory.join(frame_image::DIRECTORY_NAME);
    let message = tokio::task::spawn_blocking(move || {
        let mut saved = None;
        let grabbed = frames.read(&mut |frame| saved = Some(frame_image::save(frame, &directory)));

        match (grabbed, saved) {
            (Ok(true), Some(Ok(path))) => {
                format!("Saved {}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
            }
            (Ok(true), Some(Err(e))) => format!("Capture failed — {e}"),
            (Ok(_), _) => "Rust is not running".into(),
            (Err(e), _) => format!("Capture failed — {e}"),
        }
    })
    .await
    .unwrap_or_else(|e| format!("Capture failed — {e}"));

    notice.set(message, &notify);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_per_tail_state() {
        assert_eq!(status_text(TailState::Idle, "x", 0), "Idle");
        assert_eq!(status_text(TailState::Tailing, "path", 12345), "Tailing — 12,345 events");
        assert_eq!(status_text(TailState::NoLog, "Rust install not found", 0), "Rust install not found");
        assert_eq!(status_text(TailState::Faulted, "boom", 0), "Faulted — boom");
    }

    #[test]
    fn the_fullscreen_warning_follows_client_cfg() {
        let config = ClientConfig::parse(["graphics.screenmode \"0\""]);

        assert!(fullscreen_warning(&config).is_some());
        assert!(fullscreen_warning(&ClientConfig::empty()).is_none());
    }
}
