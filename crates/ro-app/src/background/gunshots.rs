//! The gunshot locator thread: game-audio capture plus detection, feeding `Host::shots`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use ro_audio::AudioProfile;

use crate::host::Host;
use crate::state::Notify;

/// A profile here replaces the built-in one, e.g. one recalibrated to this listener's volume.
const PROFILE_FILE: &str = "audio-profile.json";

pub struct GunshotThread {
    stop: Arc<AtomicBool>,
    done: Receiver<()>,
}

pub fn start(host: &Host, notify: Notify) -> GunshotThread {
    let profile = AudioProfile::load_or_builtin(&host.data_directory.join(PROFILE_FILE));
    let (stop, feed) = (Arc::new(AtomicBool::new(false)), host.shots.clone());
    let stopping = stop.clone();
    let (finished, done) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("gunshots".into()).spawn(move || {
        ro_audio::live::run_locator(&stopping, profile, &feed, &|| notify());
        let _ = finished.send(());
    });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "Could not start the gunshot locator");
    }
    GunshotThread { stop, done }
}

impl GunshotThread {
    pub fn stop(self, timeout: Duration) {
        self.stop.store(true, Ordering::SeqCst);
        if let Err(RecvTimeoutError::Timeout) = self.done.recv_timeout(timeout) {
            tracing::warn!("The gunshot locator did not stop in time; leaving it to process exit");
        }
    }
}
