use std::sync::{Mutex, PoisonError};

use chrono::{TimeDelta, Utc};
use ro_core::capture::BackendHealth;

use crate::frame_source::{CaptureError, Frame, FrameSource};
use crate::rust_window;

/// Ten seconds of failure at the resting cadence — long past a mid-drag straddle.
const TOLERATED: i32 = 10;

const COOL_DOWN: TimeDelta = TimeDelta::minutes(1);

/// Reads through the preferred backend, falling back to a second one when it stops producing
/// frames.
///
/// No frame is lost to a fallback: a failed attempt is retried on the other backend immediately,
/// and the demotion only decides which one is tried first. See [`BackendHealth`] for why the
/// demotion expires.
pub struct FallbackFrameSource {
    preferred: Box<dyn FrameSource>,
    fallback: Box<dyn FrameSource>,
    health: Mutex<BackendHealth>,
}

impl FallbackFrameSource {
    pub fn new(preferred: Box<dyn FrameSource>, fallback: Box<dyn FrameSource>) -> Self {
        Self { preferred, fallback, health: Mutex::new(BackendHealth::new(TOLERATED, COOL_DOWN)) }
    }
}

impl FrameSource for FallbackFrameSource {
    fn read(&self, use_frame: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError> {
        let mut health = self.health.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Utc::now();

        if !health.use_preferred(now) {
            return self.fallback.read(use_frame);
        }

        match self.preferred.read(use_frame) {
            Ok(true) => {
                health.record_success();
                return Ok(true);
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "Preferred capture backend failed; trying desktop capture"),
        }

        // Rust not being on screen is not a backend failure, and counting it would demote the
        // preferred backend before the game had ever started.
        if rust_window::client_bounds().is_none() {
            return Ok(false);
        }

        if health.record_failure(now) {
            tracing::warn!(
                "Preferred capture backend produced nothing {TOLERATED} reads running; preferring desktop capture \
                 for {} before trying it again",
                ro_core::timespan::format(COOL_DOWN)
            );
        }

        self.fallback.read(use_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Failing;

    impl FrameSource for Failing {
        fn read(&self, _: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError> {
            Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_OUTOFMEMORY).into())
        }
    }

    struct Empty;

    impl FrameSource for Empty {
        fn read(&self, _: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError> {
            Ok(false)
        }
    }

    #[test]
    fn a_preferred_error_does_not_escape() {
        let source = FallbackFrameSource::new(Box::new(Failing), Box::new(Empty));

        assert!(matches!(source.read(&mut |_| {}), Ok(false)));
    }
}
