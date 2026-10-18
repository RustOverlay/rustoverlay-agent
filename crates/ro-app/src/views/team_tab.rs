//! Team tab (TeamTab.xaml): the account, then the lobby while disconnected or the roster while
//! broadcasting. Signed out, it is the sign-in card alone.

use eframe::egui::{self, Margin, RichText, Ui};
use ro_net::teams::TeamDto;

use super::{account, team_member_card};
use crate::session::Session;
use crate::state::team::{TeamModel, normalize_join_code_entry};
use crate::ui::fonts::semibold;
use crate::ui::theme::{ACCENT, DANGER, FONT_SIZE_BODY, KILL_FILL, TEXT_SUBTLE};
use crate::ui::widgets::{banner, caption, card, danger_icon, section_title};

/// Segoe MDL2 "Delete".
const DELETE_GLYPH: &str = "\u{E74D}";

#[derive(Default)]
pub struct TeamTab {
    confirm_leave: Option<TeamDto>,
    sign_in: account::SignInCard,
}

impl TeamTab {
    pub fn show(&mut self, ui: &mut Ui, session: &Session) {
        if session.host.auth.is_signed_out() {
            self.sign_in.show(ui, session);
            return;
        }
        account::strip(ui, session);

        let model = &session.team;

        let error = model.state().error.clone();
        if !error.is_empty() {
            banner(ui, KILL_FILL, DANGER, Margin::symmetric(10, 6), |ui| {
                ui.label(RichText::new(error).color(DANGER));
            });
            ui.add_space(12.0);
        }

        if model.state().active.is_some() {
            connected(ui, model);
        } else {
            self.lobby(ui, model);
        }

        self.confirm(ui.ctx(), model);
    }

    fn lobby(&mut self, ui: &mut Ui, model: &std::sync::Arc<TeamModel>) {
        section_title(ui, "YOUR TEAMS");

        let teams = model.state().teams.clone();
        let footer_height = 110.0;
        let list_height = (ui.available_height() - footer_height).max(40.0);

        if teams.is_empty() {
            ui.allocate_ui(egui::vec2(ui.available_width(), list_height), |ui| {
                ui.set_min_height(list_height);
                caption(ui, "You are not in a team yet. Create one, or enter a teammate's join code.");
            });
        } else {
            // Not shrunk to its rows, or the New Team footer rides up and down with the team count.
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(list_height).show(ui, |ui| {
                for team in &teams {
                    self.team_row(ui, model, team);
                }
            });
        }

        ui.add_space(16.0);
        section_title(ui, "NEW TEAM");
        ui.horizontal(|ui| {
            let mut state = model.state();
            ui.add(egui::TextEdit::singleline(&mut state.new_team_name).desired_width(200.0));
            drop(state);
            if ui.button("Create").clicked() {
                model.create();
            }

            ui.add_space(24.0);
            let mut state = model.state();
            ui.add(egui::TextEdit::singleline(&mut state.join_code_entry).desired_width(120.0).char_limit(12));
            normalize_join_code_entry(&mut state.join_code_entry);
            drop(state);
            if ui.button("Join by code").clicked() {
                model.join();
            }
        });
        ui.add_space(10.0);
        caption(
            ui,
            "Connecting to a team shares your health and inventory with it, read from your screen while Rust is focused. Disconnect to stop.",
        );
    }

    fn team_row(&mut self, ui: &mut Ui, model: &std::sync::Arc<TeamModel>, team: &TeamDto) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(&team.name).font(semibold(FONT_SIZE_BODY)));
                    ui.label(
                        RichText::new(format!("{} · {} members", team.join_code, team.member_count()))
                            .monospace()
                            .color(TEXT_SUBTLE),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if danger_icon(ui, DELETE_GLYPH, "Leave this team").clicked() {
                        self.confirm_leave = Some(team.clone());
                    }
                    if ui.button("Connect").clicked() {
                        model.connect(team);
                    }
                });
            });
        });
        ui.add_space(8.0);
    }

    /// Confirmed because leaving takes the join code with it: getting back in means asking a member
    /// for the code, and an owner leaving hands the team to someone else.
    fn confirm(&mut self, ctx: &egui::Context, model: &std::sync::Arc<TeamModel>) {
        let Some(team) = self.confirm_leave.clone() else { return };

        let mut decided = None;
        let response = egui::Modal::new(egui::Id::new("leave-team")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(RichText::new("Leave team").font(semibold(FONT_SIZE_BODY)));
            ui.add_space(8.0);
            ui.label(format!("Leave \"{}\"? You will need the join code to get back in.", team.name));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    decided = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decided = Some(false);
                }
            });
        });

        if decided == Some(true) {
            model.leave(team);
        }
        if decided.is_some() || response.should_close() {
            self.confirm_leave = None;
        }
    }
}

fn connected(ui: &mut Ui, model: &std::sync::Arc<TeamModel>) {
    let active = model.state().active.clone().unwrap_or_default();

    section_title(ui, &active.name);
    ui.horizontal(|ui| {
        caption(ui, "Join code");
        ui.label(RichText::new(&active.join_code).monospace().size(18.0).color(ACCENT));
        ui.add_space(4.0);
        if ui.button("Rotate").clicked() {
            model.rotate_code();
        }
        if ui.button("Disconnect").clicked() {
            model.disconnect();
        }
    });
    ui.add_space(8.0);
    caption(ui, &model.connection());
    ui.add_space(12.0);

    let members = model.state().members.clone();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for member in &members {
            team_member_card::show(ui, member);
        }
    });
}
