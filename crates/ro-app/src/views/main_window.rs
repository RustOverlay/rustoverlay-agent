//! The main window (MainWindow.xaml): the warning banners over the three tabs.

use eframe::egui::{self, Frame, Margin, RichText, Ui};

use super::{debug_tab, settings_tab, team_tab};
use crate::session::Session;
use crate::state::agent::fullscreen_warning;
use crate::ui::theme::{CARD_BORDER, SURFACE, WARNING, WOUND_FILL};
use crate::ui::widgets::{banner, tab_strip};

const TABS: [&str; 3] = ["Team", "Settings", "Debug"];
const TEAM_TAB: usize = 0;
const SETTINGS_TAB: usize = 1;

#[derive(Default)]
pub struct MainWindow {
    tab: usize,
    shown_tab: Option<usize>,
    team: team_tab::TeamTab,
    settings: settings_tab::SettingsTab,
}

impl MainWindow {
    pub fn show(&mut self, ui: &mut Ui, session: &Session) {
        let outer = Margin::same(16);
        Frame::new().inner_margin(outer).show(ui, |ui| {
            if session.host.auth.is_signed_out() && self.tab != TEAM_TAB {
                self.signed_out_banner(ui);
                ui.add_space(16.0);
            }

            if let Some(warning) = fullscreen_warning(&session.client_config) {
                banner(ui, WOUND_FILL, WARNING, Margin::symmetric(12, 8), |ui| {
                    ui.label(RichText::new(warning).color(WARNING));
                });
                ui.add_space(16.0);
            }

            tab_strip(ui, &TABS, &mut self.tab);
            ui.add_space(10.0);

            Frame::new()
                .fill(SURFACE)
                .stroke(egui::Stroke::new(1.0, CARD_BORDER))
                .corner_radius(egui::CornerRadius::same(crate::ui::theme::RADIUS))
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.show_tab(ui, session);
                });
        });
    }

    fn show_tab(&mut self, ui: &mut Ui, session: &Session) {
        // WPF loads a tab's content each time it is selected, and the Team tab refreshes on load.
        if self.shown_tab != Some(self.tab) {
            self.shown_tab = Some(self.tab);
            if self.tab == TEAM_TAB {
                session.team.refresh();
            }
        }

        match self.tab {
            TEAM_TAB => self.team.show(ui, session),
            SETTINGS_TAB => self.settings.show(ui, session),
            _ => debug_tab::show(ui, session),
        }
    }

    /// Deliberately not a dialog: one raised the moment the token dies would steal focus out of
    /// fullscreen Rust. The Team tab holds the sign-in itself.
    fn signed_out_banner(&mut self, ui: &mut Ui) {
        banner(ui, WOUND_FILL, WARNING, Margin::symmetric(12, 8), |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Signed out — nothing is shared with your team.").color(WARNING));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Sign in").clicked() {
                        self.tab = TEAM_TAB;
                    }
                });
            });
        });
    }
}
