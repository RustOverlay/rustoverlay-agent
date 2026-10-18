//! Composition root: the only place that knows the full object graph. Replaces `AgentHost` and
//! `AddAgentCore`; there is no container, so every service is built here once and shared by `Arc`.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ro_capture::{
    CaptureDiagnostics, CaptureError, DuplicationFrameSource, FallbackFrameSource, Frame, FrameSource,
    ScreenFrameSource, duplication_is_available,
};
use ro_audio::ShotFeed;
use ro_core::hosting::{AgentStatus, GameEventBus, OverlaySettingsStore};
use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
use ro_net::enrollment::{AuthState, DpapiTokenStore, EnrollmentService, SystemBrowserLauncher, TokenStore};
use ro_net::relay::{ActiveTeamStore, TeamRoomState};
use ro_net::teams::TeamsClient;
use ro_vision::hosting::LoadoutState;
use ro_vision::items::ItemCatalog;

use crate::icons::ItemIcons;
use crate::settings::Options;

/// `HttpClient`'s default timeout; reqwest has none unless asked.
const HTTP_TIMEOUT: Duration = Duration::from_secs(100);

pub struct Host {
    pub data_directory: PathBuf,
    pub options: Options,
    pub tokens: Arc<dyn TokenStore>,
    pub active_team: ActiveTeamStore,
    pub overlay_settings: Arc<OverlaySettingsStore>,
    pub diagnostics: CaptureDiagnostics,
    pub frames: Arc<dyn FrameSource>,
    pub locator: Arc<dyn RustInstallLocator>,
    pub catalog: Arc<LazyCatalog>,
    pub icons: Arc<ItemIcons>,
    pub auth: Arc<AuthState>,
    pub bus: GameEventBus,
    pub status: Arc<AgentStatus>,
    pub room: Arc<TeamRoomState>,
    pub loadout: Arc<Mutex<LoadoutState>>,
    pub shots: Arc<Mutex<ShotFeed>>,
    pub enrollment: Arc<EnrollmentService>,
    pub teams: Arc<TeamsClient>,
}

impl Host {
    pub fn build(data_directory: PathBuf, options: Options) -> io::Result<Self> {
        let tokens: Arc<dyn TokenStore> =
            Arc::new(DpapiTokenStore::new(&data_directory, &options.backend.file_name("install", "token"))?);
        crate::startup::mark("token store");
        let http = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build().map_err(io::Error::other)?;
        crate::startup::mark("http client");
        let frames = DeferredFrameSource::probe_in_background();
        let locator: Arc<dyn RustInstallLocator> = Arc::new(SteamRustInstallLocator::new(Some(&options.agent)));
        let auth = Arc::new(AuthState::new(tokens.clone()));

        Ok(Self {
            active_team: ActiveTeamStore::new(&data_directory, &options.backend.file_name("active-team", "json")),
            overlay_settings: Arc::new(OverlaySettingsStore::new(&data_directory, options.overlay.clone())),
            diagnostics: CaptureDiagnostics { data_directory: data_directory.clone() },
            frames,
            catalog: Arc::new(LazyCatalog::new(locator.clone())),
            icons: Arc::new(ItemIcons::new()),
            bus: GameEventBus::new(),
            status: Arc::new(AgentStatus::new()),
            room: Arc::new(TeamRoomState::new()),
            loadout: Arc::new(Mutex::new(LoadoutState::new())),
            shots: Arc::new(Mutex::new(ShotFeed::new())),
            enrollment: Arc::new(EnrollmentService::new(
                http.clone(),
                tokens.clone(),
                Arc::new(SystemBrowserLauncher),
                options.backend.clone(),
            )),
            teams: Arc::new(TeamsClient::new(http, tokens.clone(), auth.clone(), &options.backend)),
            locator,
            auth,
            tokens,
            data_directory,
            options,
        })
    }

    /// The signed-in player's SteamID, or 0 when there is no token.
    pub fn own_steam_id(&self) -> u64 {
        self.tokens.load().map_or(0, |token| token.steam_id as u64)
    }
}

/// Shared by the capture loop and the debug grab button — see `FrameSource` on why implementations
/// must tolerate that. Duplication is probed rather than assumed: it is unavailable in some
/// remote-desktop sessions and on virtualised display drivers. The probe only covers the primary
/// display, so `FallbackFrameSource` still watches it at runtime.
fn frame_source() -> Arc<dyn FrameSource> {
    if let Err(reason) = duplication_is_available() {
        tracing::warn!(
            "Display duplication unavailable ({reason}); falling back to desktop capture, which cannot read exclusive fullscreen"
        );
        return Arc::new(ScreenFrameSource::new());
    }

    Arc::new(FallbackFrameSource::new(Box::new(DuplicationFrameSource::new()), Box::new(ScreenFrameSource::new())))
}

/// [`frame_source`], chosen on a background thread. The duplication probe creates a D3D11 device
/// and costs 150–250 ms, which on the UI thread was over half the time to the first window; a read
/// that arrives before the probe finishes waits for it.
struct DeferredFrameSource {
    chosen: Arc<OnceLock<Arc<dyn FrameSource>>>,
}

impl DeferredFrameSource {
    fn probe_in_background() -> Arc<dyn FrameSource> {
        let chosen: Arc<OnceLock<Arc<dyn FrameSource>>> = Arc::new(OnceLock::new());
        let probing = chosen.clone();
        let spawned = std::thread::Builder::new().name("capture-probe".into()).spawn(move || {
            probing.get_or_init(frame_source);
        });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "Could not probe capture in the background; probing on first read");
        }
        Arc::new(Self { chosen })
    }
}

impl FrameSource for DeferredFrameSource {
    fn read(&self, use_frame: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError> {
        self.chosen.get_or_init(frame_source).read(use_frame)
    }
}

/// Definitions only — no icon hashing, so this is cheap. Shared because both the team list and the
/// overlay resolve item ids, and parsing 1200-odd definitions twice buys nothing. Names are
/// cosmetic: a missing catalogue renders raw item ids rather than blanking the row.
pub struct LazyCatalog {
    locator: Arc<dyn RustInstallLocator>,
    value: OnceLock<Option<Arc<ItemCatalog>>>,
}

impl LazyCatalog {
    pub fn new(locator: Arc<dyn RustInstallLocator>) -> Self {
        Self { locator, value: OnceLock::new() }
    }

    pub fn get(&self) -> Option<Arc<ItemCatalog>> {
        self.value.get_or_init(|| self.load()).clone()
    }

    fn load(&self) -> Option<Arc<ItemCatalog>> {
        let install = self.locator.locate()?;
        let directory = install.items_directory();
        if !directory.is_dir() {
            return None;
        }

        match ItemCatalog::load(&directory) {
            Ok(catalog) => Some(Arc::new(catalog)),
            Err(e) => {
                // TODO(port): C# let this exception escape the Lazy and fault its caller.
                tracing::error!(error = %e, "Could not load the item catalogue");
                None
            }
        }
    }
}
