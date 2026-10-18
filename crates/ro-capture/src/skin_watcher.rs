use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use ro_vision::items::{Catalogue, SkinCatalog};

use crate::vision_service::save_cache;
use crate::{icon_signatures, SignatureCache, StopToken};

/// How often the cache is swept regardless of whether any event arrived.
const BACKSTOP: Duration = Duration::from_secs(2 * 60);

/// How often new signatures are written out. Each save rewrites the whole cache, megabytes of it,
/// so a burst of skins must not cost one save per sweep.
const SAVE_EVERY: Duration = Duration::from_secs(60);

/// Watches Steam's skin cache and builds signatures for whatever appears in it.
///
/// Steam fills the cache as the player meets skins, so a catalogue built at launch goes stale
/// within minutes on a populated server. The watcher only raises a flag; every read of the
/// directory and every mutation happens in [`Self::sweep`], on whichever thread calls it, so
/// nothing here is shared across threads but one boolean.
///
/// Gotcha: directory watches silently drop events when their buffer overflows, and Steam writes
/// these in bursts — hundreds of directories in a sitting. Joining a populated server is exactly
/// when that happens, and a dropped event leaves a skin unmatchable with nothing to show for it.
/// [`BACKSTOP`] is the sweep that does not depend on events arriving.
pub struct SkinWatcher {
    catalog: SkinCatalog,
    cache: Arc<SignatureCache>,
    stop: StopToken,
    stirred: Arc<AtomicBool>,
    swept_at: Option<Instant>,
    saved_at: Instant,
    _watcher: Option<RecommendedWatcher>,
}

impl SkinWatcher {
    pub fn new(catalog: SkinCatalog, directory: &Path, cache: Arc<SignatureCache>, stop: StopToken) -> Self {
        let stirred = Arc::new(AtomicBool::new(true));
        let watcher = if directory.is_dir() { watch(directory, Arc::clone(&stirred)) } else { None };

        Self { catalog, cache, stop, stirred, swept_at: None, saved_at: Instant::now(), _watcher: watcher }
    }

    /// The catalogue being filled, for its unmapped-label report.
    pub fn catalog(&self) -> &SkinCatalog {
        &self.catalog
    }

    /// Signatures for skins not seen before, empty when there is nothing new or nothing to do yet.
    ///
    /// Cheap to call every frame: without an event and inside the backstop interval it returns
    /// immediately. Skins whose icon is still being written are dropped from the catalogue rather
    /// than half-loaded, so the next sweep reconsiders them. New signatures reach disk at most every
    /// [`SAVE_EVERY`]; the caller saves the cache once more on the way out.
    pub fn sweep(&mut self, now: Instant) -> Catalogue {
        let built = self.rescan(now);
        if is_due(Some(self.saved_at), now, SAVE_EVERY) {
            save_cache(&self.cache);
            self.saved_at = now;
        }
        built
    }

    fn rescan(&mut self, now: Instant) -> Catalogue {
        if !self.stirred.load(Ordering::SeqCst) && !is_due(self.swept_at, now, BACKSTOP) {
            return Catalogue::new();
        }

        self.stirred.store(false, Ordering::SeqCst);
        let first = self.swept_at.replace(now).is_none();

        let found = self.catalog.rescan();
        // A stopped sweep must not prune: the skins it never reached are still on disk.
        let Ok((built, unreadable)) = icon_signatures::build_cached_until(found, &self.cache, &self.stop) else {
            return Catalogue::new();
        };
        // The first rescan looks up every skin on disk, so anything still untouched is gone.
        if first {
            self.cache.retain_touched();
        }
        if built.is_empty() && unreadable.is_empty() {
            return built;
        }

        for face in &unreadable {
            self.catalog.forget(face);
        }

        let retry = if unreadable.is_empty() { String::new() } else { format!(", {} still downloading", unreadable.len()) };
        tracing::info!("Skin cache: {} new icons{retry}", built.len());

        built
    }
}

fn is_due(last: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= interval)
}

/// Nothing is read in the handler on purpose: the event can arrive while Steam is still writing
/// the directory, and `sweep` is where that is handled once rather than per event.
fn watch(directory: &Path, stirred: Arc<AtomicBool>) -> Option<RecommendedWatcher> {
    let handler = move |event: notify::Result<Event>| match event {
        // TODO(port): an overflow could stir the next sweep instead of waiting out the backstop.
        Ok(event) if event.need_rescan() => on_error("event buffer overflowed"),
        Ok(event) if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))) => {
            stirred.store(true, Ordering::SeqCst);
        }
        Ok(_) => {}
        Err(error) => on_error(&error.to_string()),
    };

    let started = notify::recommended_watcher(handler)
        .and_then(|mut watcher| watcher.watch(directory, RecursiveMode::Recursive).map(|()| watcher));

    match started {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            // A cache on a removable or permission-restricted library still sweeps on the backstop.
            tracing::warn!(%error, "Cannot watch the skin cache; falling back to periodic sweeps");
            None
        }
    }
}

/// Not fatal: the backstop sweep finds whatever the dropped events would have.
fn on_error(error: &str) {
    tracing::warn!(error, "Skin cache watcher faulted; relying on periodic sweeps");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sweep_is_always_due() {
        assert!(is_due(None, Instant::now(), BACKSTOP));
    }

    #[test]
    fn the_backstop_fires_after_two_minutes() {
        let at = Instant::now();

        assert!(!is_due(Some(at), at + BACKSTOP - Duration::from_millis(1), BACKSTOP));
        assert!(is_due(Some(at), at + BACKSTOP, BACKSTOP));
    }

    #[test]
    fn saves_wait_out_their_interval() {
        let at = Instant::now();

        assert!(!is_due(Some(at), at + SAVE_EVERY / 2, SAVE_EVERY));
        assert!(is_due(Some(at), at + SAVE_EVERY, SAVE_EVERY));
    }
}
