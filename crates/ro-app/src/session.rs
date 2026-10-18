//! A signed-in run of the agent: the models the windows draw, the listeners that keep them current,
//! and the services feeding them. Built once the sign-in gate has passed.

use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui::{self, ViewportId};
use ro_core::config::ClientConfig;
use tokio::runtime::Handle;

use crate::background::Services;
use crate::host::Host;
use crate::interop::overlay_input::OverlayInputThread;
use crate::state::Notify;
use crate::state::agent::{CaptureNotice, load_client_config};
use crate::state::account::Player;
use crate::state::feed::EventFeed;
use crate::state::overlay::OverlayModel;
use crate::state::team::TeamModel;
use crate::ui::textures::IconTextures;

pub const OVERLAY_VIEWPORT: &str = "overlay";

pub fn overlay_viewport_id() -> ViewportId {
    ViewportId::from_hash_of(OVERLAY_VIEWPORT)
}

pub struct Session {
    pub host: Arc<Host>,
    pub runtime: Handle,
    pub feed: Arc<Mutex<EventFeed>>,
    /// Read from the token at sign-in rather than per frame: loading it is a DPAPI decrypt.
    pub account: Mutex<Option<Player>>,
    pub team: Arc<TeamModel>,
    pub overlay: Arc<OverlayModel>,
    pub notice: CaptureNotice,
    /// Read once: Rust only writes client.cfg on exit.
    pub client_config: ClientConfig,
    pub textures: Arc<IconTextures>,
    pub notify_main: Notify,
    services: Services,
    overlay_input: OverlayInputThread,
}

impl Session {
    pub fn start(host: Arc<Host>, runtime: Handle, ctx: &egui::Context) -> Self {
        let main_ctx = ctx.clone();
        // Not `request_repaint`: off the UI thread that targets whichever viewport is mid-pass.
        let notify_main: Notify = Arc::new(move || main_ctx.request_repaint_of(ViewportId::ROOT));
        let overlay_ctx = ctx.clone();
        let notify_overlay: Notify = Arc::new(move || overlay_ctx.request_repaint_of(overlay_viewport_id()));

        let feed =
            Arc::new(Mutex::new(EventFeed::new(usize::try_from(host.options.agent.max_displayed_events).unwrap_or(0))));
        let account = Mutex::new(host.tokens.load().map(|t| Player::from_token(&t)));

        let team = TeamModel::new(
            host.teams.clone(),
            host.active_team.clone(),
            host.room.clone(),
            host.auth.clone(),
            host.catalog.clone(),
            runtime.clone(),
            notify_main.clone(),
        );
        let overlay = OverlayModel::new(
            host.room.clone(),
            host.catalog.clone(),
            host.icons.clone(),
            host.overlay_settings.clone(),
            host.own_steam_id(),
            notify_overlay.clone(),
        );

        subscribe(&host, &team, &overlay, &notify_main, &notify_overlay);

        let services = Services::start(&host, &runtime, feed.clone(), notify_main.clone());
        let overlay_input =
            OverlayInputThread::spawn(host.overlay_settings.clone(), overlay.clone(), notify_overlay.clone());

        Self {
            client_config: load_client_config(host.locator.as_ref()),
            host,
            runtime,
            feed,
            account,
            team,
            overlay,
            notice: CaptureNotice::default(),
            textures: Arc::new(IconTextures::default()),
            notify_main,
            services,
            overlay_input,
        }
    }

    /// Re-reads the account after a fresh sign-in, which may be a different Steam account.
    pub fn signed_in_again(&self) {
        self.host.auth.sign_in();
        let account = self.host.tokens.load().map(|t| Player::from_token(&t));
        self.overlay.set_own_steam_id(account.as_ref().map_or(0, |a| a.steam_id));
        *self.account.lock().unwrap_or_else(PoisonError::into_inner) = account;

        // The uplink resumes on its own once a token is back; the team list does not, because the
        // 401 that started this left it holding whatever was on screen beforehand.
        self.team.refresh();
    }

    /// Clearing the token also ends the broadcast; see `AuthState::sign_out`.
    pub fn sign_out(&self) {
        self.host.auth.sign_out_by_player();
        *self.account.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    pub fn shutdown(&mut self) {
        self.overlay_input.shutdown();
        self.services.shutdown();
    }
}

/// Every listener runs on the thread that changed the state, so each only updates a model behind
/// its mutex and asks for a repaint.
fn subscribe(host: &Host, team: &Arc<TeamModel>, overlay: &Arc<OverlayModel>, main: &Notify, drawn: &Notify) {
    let repaint = main.clone();
    host.status.on_changed(move |_| repaint());

    let repaint = main.clone();
    host.auth.subscribe(move |_| repaint());

    let (team, overlay) = (team.clone(), overlay.clone());
    host.room.subscribe(move |_| {
        team.on_room_changed();
        overlay.reconcile();
    });

    // Runs under the loadout lock on the capture thread, so it must not touch the state itself.
    let repaint = main.clone();
    host.loadout.lock().unwrap_or_else(std::sync::PoisonError::into_inner).subscribe(move |_| repaint());

    let (repaint, redraw) = (main.clone(), drawn.clone());
    host.overlay_settings.on_changed(move || {
        repaint();
        redraw();
    });
}
