use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use ro_core::config::ClientConfig;
use ro_core::install::{KeyBindings, RustInstallLocator};
use ro_vision::hosting::LoadoutState;
use ro_vision::hud::{CountReader, GlyphAtlas, InventoryReader};
use ro_vision::items::{
    builtin_skin_faces, is_default_skin_category, retain_covering_skins, without_partial_skins, Catalogue, IconFace,
    IconMatcher, ItemCatalog, SkinCatalog,
};

use crate::fetched_skins::{FetchedSkins, SkinIconSource};
use crate::frame_source::FrameSource;
use crate::vision_pass::VisionPass;
use crate::{icon_signatures, rust_window, FontGlyphAtlas, KeyWatcher, SignatureCache, SkinWatcher, StopToken};

/// Resting cadence. A loadout changes on a human timescale, so this is ample.
const REST: Duration = Duration::from_secs(1);

/// How often the inventory key is sampled. One `GetAsyncKeyState` call.
const POLL: Duration = Duration::from_millis(50);

/// Frame cadence for [`BOOST`] after the inventory key is pressed.
///
/// A player can open and shut the inventory inside a second, which the resting cadence would miss
/// entirely. The panel animates open, so the press itself is the wrong moment to read; reading
/// repeatedly afterwards catches the panel wherever the animation lands, and catches the shut too.
const ALERT: Duration = Duration::from_millis(100);

/// How long a keypress keeps the cadence at [`ALERT`].
const BOOST: Duration = Duration::from_secs(2);

/// Backoff when Rust is not running, not focused, or the catalogue is unavailable.
const IDLE: Duration = Duration::from_secs(2);

const SIGNATURE_CACHE_NAME: &str = "icon-signatures.bin";

/// Where the capture pipeline may write diagnostic artefacts. Supplied by the host, which owns the
/// data directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureDiagnostics {
    pub data_directory: PathBuf,
}

/// Reads the HUD off the screen until stopped, publishing to [`LoadoutState`].
///
/// Passive throughout: it reads the composited desktop and the player's own config files, and never
/// touches the Rust process. [`Self::run`] blocks, so the host gives it a thread of its own.
pub struct VisionService {
    locator: Arc<dyn RustInstallLocator>,
    frames: Arc<dyn FrameSource>,
    loadout: Arc<Mutex<LoadoutState>>,
    diagnostics: CaptureDiagnostics,
    skin_icons: Option<Arc<dyn SkinIconSource>>,
}

impl VisionService {
    pub fn new(
        locator: Arc<dyn RustInstallLocator>,
        frames: Arc<dyn FrameSource>,
        loadout: Arc<Mutex<LoadoutState>>,
        diagnostics: CaptureDiagnostics,
    ) -> Self {
        Self { locator, frames, loadout, diagnostics, skin_icons: None }
    }

    /// Where icons of approved skins no local file has come from. Without one, those skins stay unread.
    pub fn with_skin_icons(mut self, source: Arc<dyn SkinIconSource>) -> Self {
        self.skin_icons = Some(source);
        self
    }

    pub fn run(&self, stop: &StopToken) {
        let install = match self.locator.locate() {
            Some(install) if install.has_item_bundles() => install,
            _ => {
                tracing::warn!("No Rust item bundles found; screen reading is disabled");
                return;
            }
        };

        // The definitions never change while the game runs. The icons do: Steam keeps filling the
        // skin cache, which is why SkinWatcher exists and why the matcher is rebuilt rather than
        // built once.
        let catalogue = match ItemCatalog::load(&install.items_directory()) {
            Ok(catalogue) => Arc::new(catalogue),
            Err(error) => return startup_failed("item catalogue", &error),
        };
        if stop.is_stopped() {
            return;
        }

        let cache = Arc::new(SignatureCache::open(self.diagnostics.data_directory.join(SIGNATURE_CACHE_NAME)));
        let shipped = builtin_skin_faces(&catalogue, &install.items_directory(), |i| is_default_skin_category(&i.category));
        let faces = catalogue.with_icons().map(|i| IconFace::from(Arc::clone(i))).chain(shipped);
        let built = icon_signatures::build_cached_until(faces, &cache, stop);
        save_cache(&cache);
        let Ok((signatures, _)) = built else {
            return;
        };
        let (mut signatures, partial) = without_partial_skins(signatures);
        tracing::info!("Item catalogue ready: {} icons ({partial} partial skin icons left out)", signatures.len());
        if stop.is_stopped() {
            return;
        }

        let fetched = self.skin_icons.clone().map(|source| FetchedSkins::new(source, Arc::clone(&catalogue), Arc::clone(&cache)));
        let skins_directory = install.skins_directory();
        let skins = SkinCatalog::new(catalogue, &skins_directory, None);
        let mut skin_watcher = SkinWatcher::new(skins, &skins_directory, Arc::clone(&cache), stop.clone());

        // Read once, like the key bindings: Rust only writes client.cfg on exit, so re-reading it
        // mid-session cannot see a change the player has just made anyway.
        let config_path = install.client_config_path();
        let config = if config_path.is_file() {
            match ClientConfig::load(&config_path) {
                Ok(config) => config,
                Err(error) => return startup_failed("client.cfg", &error),
            }
        } else {
            ClientConfig::empty()
        };

        let ui_scale = config.ui_scale();
        tracing::info!("UI scale is {ui_scale}");

        let glyphs = match FontGlyphAtlas::new() {
            Ok(glyphs) => Arc::new(glyphs),
            Err(error) => return startup_failed("count glyphs", &error),
        };
        if stop.is_stopped() {
            return;
        }
        let mut pass = VisionPass::new(reader_over(&signatures, &glyphs), ui_scale);

        let keys = match KeyBindings::load(&install.keys_config_path()) {
            Ok(keys) => keys,
            Err(error) => return startup_failed("keys.cfg", &error),
        };
        let mut watcher = KeyWatcher::new(keys.inventory_key());
        let note = if watcher.is_watchable() { "" } else { " (unrecognised, not watched)" };
        tracing::info!("Inventory key is {}{note}", keys.inventory_key());

        let mut reported_skin_labels = HashSet::new();
        let mut boosted_until: Option<Instant> = None;
        let mut next_read: Option<Instant> = None;

        // The key is sampled on its own clock, far faster than frames are read. Tying the two
        // together would sample it once a second at rest, and a press and release inside that
        // second — the very case this exists for — leaves no trace to find afterwards.
        while !stop.is_stopped() {
            if !rust_window::is_foreground() {
                stop.wait(IDLE);
                continue;
            }

            // Scheduling runs on the monotonic clock so a wall-clock step cannot stall reads.
            let now = Instant::now();
            if watcher.was_pressed() {
                boosted_until = Some(now + BOOST);
            }

            // Rebuilding is a new matcher over signatures already in hand, so it costs nothing next
            // to the decode that produced them.
            let mut arrived = skin_watcher.sweep(now);
            retain_covering_skins(&mut arrived, &signatures);
            if !arrived.is_empty() {
                warn_about_unmapped_skins(skin_watcher.catalog(), &mut reported_skin_labels);
            }
            if let Some(fetched) = &fetched {
                arrived.extend(fetched.take(&signatures));
            }
            if !arrived.is_empty() {
                signatures.extend(arrived);
                pass.reader = reader_over(&signatures, &glyphs);
            }

            if next_read.is_none_or(|at| now >= at) {
                self.read_once(&mut pass, SystemTime::now(), fetched.as_ref().map(FetchedSkins::source));
                let boosted = boosted_until.is_some_and(|until| now < until);
                next_read = Some(now + if boosted { ALERT } else { REST });
            }

            stop.wait(POLL);
        }

        save_cache(&cache);
    }

    /// A frame can fail for reasons that resolve themselves — the window closing mid-grab, a
    /// display mode change — so this logs and keeps the loop alive, panics included.
    fn read_once(&self, pass: &mut VisionPass, now: SystemTime, skins: Option<&dyn SkinIconSource>) {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            if pass.coordinates_are_real() {
                pass.read(self.frames.as_ref(), &self.loadout, &self.diagnostics.data_directory, now, skins).map(|_| ())
            } else {
                Ok(())
            }
        }));

        match outcome {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "Frame read failed"),
            Err(_) => tracing::warn!("Frame read failed: the recognition pass panicked"),
        }
    }
}

fn reader_over(signatures: &Catalogue, glyphs: &Arc<FontGlyphAtlas>) -> InventoryReader {
    let atlas: Arc<dyn GlyphAtlas> = Arc::clone(glyphs) as Arc<dyn GlyphAtlas>;
    InventoryReader::new(Arc::new(IconMatcher::new(signatures.clone())), Arc::new(CountReader::new(atlas)))
}

/// Reports workshop labels no item answers to, which is how `SkinItemTypes` drift surfaces rather
/// than becoming a silent gap in the catalogue.
fn warn_about_unmapped_skins(skins: &SkinCatalog, reported: &mut HashSet<String>) {
    for (label, count) in skins.unmapped_labels() {
        if reported.insert(label.to_owned()) {
            tracing::warn!("Skin label {label} matches no item; {count} skin(s) unmatchable");
        }
    }
}

/// Not fatal: a cache that cannot be written only costs the next launch a full decode.
pub(crate) fn save_cache(cache: &SignatureCache) {
    if let Err(error) = cache.save() {
        tracing::warn!(%error, "Could not save the icon signature cache");
    }
}

/// TODO(port): C# let these exceptions fault the hosted service; here the loop just never starts.
fn startup_failed(what: &str, error: &dyn std::error::Error) {
    tracing::error!(%error, "Could not load the {what}; screen reading is disabled");
}
