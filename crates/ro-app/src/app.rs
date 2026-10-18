//! The eframe application: the sign-in gate, then the main window and the overlay viewport.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Frame, Margin, Ui, ViewportBuilder, ViewportCommand, ViewportId};
use tokio::runtime::Handle;

use crate::host::Host;
use crate::interop::overlay_input::OVERLAY_TITLE;
use crate::session::{Session, overlay_viewport_id};
use crate::state::Notify;
use crate::state::sign_in::{SignInModel, SignInState};
use crate::ui::theme::APP_BG;
use crate::ui::{fonts, theme};
use crate::views::main_window::MainWindow;
use crate::views::overlay::OverlayView;
use crate::views::sign_in;

pub const MAIN_TITLE: &str = "RustOverlay Agent";
pub const MAIN_SIZE: [f32; 2] = [1080.0, 640.0];
pub const SIGN_IN_TITLE: &str = "RustOverlay";

pub struct AgentApp {
    host: Arc<Host>,
    runtime: Handle,
    sign_in: Option<Arc<SignInModel>>,
    sign_in_height: f32,
    session: Option<Session>,
    main_window: MainWindow,
    overlay: Option<OverlayView>,
}

impl AgentApp {
    /// `enrolled` decides between the main window and the sign-in gate. Closing the gate without
    /// signing in is a decision to quit: nothing else has been started.
    pub fn new(cc: &eframe::CreationContext<'_>, host: Arc<Host>, runtime: Handle, enrolled: bool) -> Self {
        crate::startup::mark("window+gl");
        fonts::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        crate::startup::mark("fonts+theme");

        let mut app = Self {
            host,
            runtime,
            sign_in: None,
            sign_in_height: 0.0,
            session: None,
            main_window: MainWindow::default(),
            overlay: None,
        };

        if enrolled {
            app.start(&cc.egui_ctx);
        } else {
            let ctx = cc.egui_ctx.clone();
            let notify: Notify = Arc::new(move || ctx.request_repaint_of(ViewportId::ROOT));
            app.sign_in = Some(SignInModel::new(app.host.enrollment.clone(), app.runtime.clone(), notify));
        }
        crate::startup::mark("session");
        app
    }

    fn start(&mut self, ctx: &egui::Context) {
        let session = Session::start(self.host.clone(), self.runtime.clone(), ctx);
        self.overlay = Some(OverlayView {
            model: session.overlay.clone(),
            settings: self.host.overlay_settings.clone(),
            textures: session.textures.clone(),
            sized: Arc::new(Mutex::new(egui::Vec2::ZERO)),
        });
        self.session = Some(session);
    }

    /// Returns true while the gate is still up.
    fn gate(&mut self, ui: &mut Ui) -> bool {
        let Some(model) = self.sign_in.clone() else { return false };

        if model.status().state == SignInState::SignedIn {
            self.sign_in = None;
            let ctx = ui.ctx().clone();
            ctx.send_viewport_cmd(ViewportCommand::Title(MAIN_TITLE.into()));
            ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(MAIN_SIZE.into()));
            self.start(&ctx);
            return false;
        }

        let used = Frame::new()
            .fill(APP_BG)
            .inner_margin(Margin::same(20))
            .show(ui, |ui| sign_in::show(ui, &model, None))
            .response
            .rect
            .height();

        // `SizeToContent="Height"`.
        if (used - self.sign_in_height).abs() > 0.5 {
            self.sign_in_height = used;
            ui.ctx().send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(sign_in::WIDTH, used)));
        }
        true
    }

    fn show_overlay(&self, ctx: &egui::Context) {
        let Some(view) = self.overlay.clone() else { return };

        // Never hidden through winit; an unwanted panel draws nothing instead (see `OverlayInput::show`).
        let builder = ViewportBuilder::default()
            .with_title(OVERLAY_TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_taskbar(false)
            .with_resizable(false)
            .with_active(false)
            .with_inner_size([1.0, 1.0]);

        ctx.show_viewport_deferred(overlay_viewport_id(), builder, move |ui, _| view.show(ui));
    }
}

impl eframe::App for AgentApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        if self.gate(ui) {
            return;
        }

        let Some(session) = &self.session else { return };
        egui::CentralPanel::no_frame().frame(Frame::new().fill(APP_BG)).show(ui, |ui| self.main_window.show(ui, session));
        self.show_overlay(ui.ctx());
        crate::startup::report("first frame");
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(session) = &mut self.session {
            session.shutdown();
        }
    }

    /// Transparent, so the overlay viewport shows the game through it; the main window paints its
    /// own background.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}
