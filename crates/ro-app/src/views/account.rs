//! The Team tab's account strip while signed in, and its sign-in card while signed out.

use std::sync::{Arc, PoisonError};

use eframe::egui::{self, RichText, Ui};
use ro_net::enrollment::SignedOut;

use super::sign_in;
use crate::session::Session;
use crate::state::account::account_line;
use crate::state::sign_in::{SignInModel, SignInState};
use crate::ui::fonts::semibold;
use crate::ui::theme::{FONT_SIZE_BODY, HEALTH_FULL, TEXT_SUBTLE, WARNING};

/// Names the server only when it is not the default, where naming it tells the player something.
fn rejected_reason(session: &Session) -> String {
    let backend = &session.host.options.backend;
    let server = match backend.base_url.host_str() {
        Some(host) if !backend.is_default() => host.to_owned(),
        _ => "The server".to_owned(),
    };
    format!("{server} no longer recognises this install, usually because the server was reset. Nothing is shared with your team until you sign in again.")
}

pub fn strip(ui: &mut Ui, session: &Session) {
    let Some(account) = session.account.lock().unwrap_or_else(PoisonError::into_inner).clone() else { return };
    let in_game = session.feed.lock().unwrap_or_else(PoisonError::into_inner).in_game().cloned();
    let line = account_line(&account, in_game.as_ref());

    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(HEALTH_FULL));
        ui.label(RichText::new(&line.name).font(semibold(FONT_SIZE_BODY)));
        if let Some(detail) = &line.detail {
            ui.add_space(6.0);
            ui.label(RichText::new(detail).color(if line.mismatch { WARNING } else { TEXT_SUBTLE }));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Sign out").on_hover_text("Forget this install's sign-in; you can sign in again as anyone").clicked() {
                session.sign_out();
            }
        });
    });

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(10.0);
}

/// Holds the sign-in in progress, so switching tabs mid-sign-in does not lose it.
#[derive(Default)]
pub struct SignInCard {
    model: Option<Arc<SignInModel>>,
}

impl SignInCard {
    pub fn show(&mut self, ui: &mut Ui, session: &Session) {
        let model = self
            .model
            .get_or_insert_with(|| {
                SignInModel::new(session.host.enrollment.clone(), session.runtime.clone(), session.notify_main.clone())
            })
            .clone();

        if model.status().state == SignInState::SignedIn {
            self.model = None;
            session.signed_in_again();
            return;
        }

        let reason = (session.host.auth.signed_out() == Some(SignedOut::Rejected)).then(|| rejected_reason(session));
        ui.add_space((ui.available_height() * 0.12).max(0.0));
        ui.vertical_centered(|ui| {
            ui.set_max_width(sign_in::WIDTH);
            sign_in::show(ui, &model, reason.as_deref());
        });
    }
}
