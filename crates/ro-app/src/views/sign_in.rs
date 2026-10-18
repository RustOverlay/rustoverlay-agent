//! The sign-in card (SignInWindow.xaml). The whole first-launch window when this install has no
//! token, and the Team tab's content whenever it is signed out.

use std::sync::Arc;

use eframe::egui::{self, CornerRadius, Margin, RichText, Ui};

use crate::state::sign_in::SignInModel;
use crate::ui::fonts::semibold;
use crate::ui::theme::{ACCENT, APP_BG, DANGER, FONT_SIZE_BODY, FONT_SIZE_TITLE, RADIUS, WARNING, WOUND_FILL};
use crate::ui::widgets::{banner, caption, card_padded};

pub const WIDTH: f32 = 440.0;

/// `reason` says why an existing sign-in ended; None on a first launch or after signing out by choice.
pub fn show(ui: &mut Ui, model: &Arc<SignInModel>, reason: Option<&str>) {
    let (enabled, action, waiting, error) = {
        let status = model.status();
        let error = status.has_failed().then(|| status.error.clone().unwrap_or_default());
        (status.can_sign_in(), status.action_text(), status.is_waiting(), error)
    };

    card_padded(ui, 24, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Sign in to RustOverlay").font(semibold(FONT_SIZE_TITLE)));
            ui.add_space(10.0);
            caption(
                ui,
                "Teammates find you by your Steam account. You sign in on Steam's own page — RustOverlay never sees your password.",
            );

            if let Some(reason) = reason {
                ui.add_space(14.0);
                banner(ui, WOUND_FILL, WARNING, Margin::symmetric(10, 6), |ui| {
                    ui.label(RichText::new(reason).color(WARNING));
                });
            }

            ui.add_space(20.0);
            let button = egui::Button::new(RichText::new(action).font(semibold(FONT_SIZE_BODY + 1.0)).color(APP_BG))
                .fill(ACCENT)
                .corner_radius(CornerRadius::same(RADIUS))
                .min_size(egui::vec2(220.0, 36.0));
            if ui.add_enabled(enabled, button).clicked() {
                model.sign_in();
            }

            if waiting {
                ui.add_space(14.0);
                ui.spinner();
                caption(ui, "Finish signing in in your browser…");
            }
            if let Some(error) = error {
                ui.add_space(14.0);
                ui.label(RichText::new(error).color(DANGER));
            }
        });
    });
}
