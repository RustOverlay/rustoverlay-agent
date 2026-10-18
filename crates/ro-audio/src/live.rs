use std::cell::RefCell;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, PoisonError};

use crate::capture::{run_game_capture, CaptureStatus};
use crate::{AudioProfile, Locator, ShotFeed};

/// Captures the game, locates every shot into `feed` and calls `changed` after each update, until
/// `stop` is set. Blocks; run it on its own thread.
pub fn run_locator(stop: &AtomicBool, profile: AudioProfile, feed: &Mutex<ShotFeed>, changed: &(dyn Fn() + Sync)) {
    // Both callbacks need the locator; they never run at the same time.
    let locator = RefCell::new(Locator::new(profile));
    run_game_capture(
        stop,
        &mut |block| {
            let located = locator.borrow_mut().push(&block);
            if located.is_empty() {
                return;
            }
            let mut feed = feed.lock().unwrap_or_else(PoisonError::into_inner);
            for shot in located {
                tracing::debug!(estimate = ?shot.estimate, "Gunshot located");
                feed.push(shot.estimate);
            }
            drop(feed);
            changed();
        },
        &mut |status| {
            if matches!(status, CaptureStatus::Capturing { .. }) {
                locator.borrow_mut().reset();
            }
            feed.lock().unwrap_or_else(PoisonError::into_inner).set_status(status.to_string());
            changed();
        },
    );
}
