//! Debug tab (DebugTab.xaml): tail status, the frame grab, located gunshots, and what the screen
//! reader sees. The player's own inventory belongs here and nowhere else.

use std::sync::PoisonError;
use std::time::SystemTime;

use eframe::egui::{self, RichText, Ui};

use crate::session::Session;
use crate::state::agent::{grab_frame, status_is_problem, status_text};
use crate::state::loadout_view::LoadoutView;
use crate::state::shot_view::ShotView;
use crate::ui::theme::{ACCENT, DANGER};
use crate::ui::widgets::{caption, section_title};

pub fn show(ui: &mut Ui, session: &Session) {
    let host = &session.host;

    section_title(ui, "LOG TAIL");
    let received = session.feed.lock().unwrap_or_else(PoisonError::into_inner).received();
    let status = status_text(host.status.state(), &host.status.detail(), received);
    let text = RichText::new(status);
    ui.label(if status_is_problem(&host.status) { text.color(DANGER) } else { text });
    ui.add_space(16.0);

    ui.horizontal(|ui| {
        if ui.button("Grab HUD frame (5s)").clicked() {
            let (frames, directory) = (host.frames.clone(), host.data_directory.clone());
            let (notice, notify) = (session.notice.clone(), session.notify_main.clone());
            session.runtime.spawn(async move { grab_frame(frames, &directory, notice, notify).await });
        }
        ui.label(RichText::new(session.notice.get()).color(ACCENT));
    });
    ui.add_space(16.0);

    let view = LoadoutView::of(&host.loadout.lock().unwrap_or_else(PoisonError::into_inner), SystemTime::now());

    let shots = ShotView::of(&host.shots.lock().unwrap_or_else(PoisonError::into_inner));

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section_title(ui, "GUNSHOTS (GAME AUDIO)");
        caption(ui, &shots.status);
        ui.add_space(4.0);
        if shots.shots.is_empty() {
            caption(ui, "No shots heard yet");
        }
        lines(ui, &shots.shots);
        ui.add_space(16.0);

        section_title(ui, "WHAT THE SCREEN READER SEES");
        caption(ui, &view.summary);
        ui.add_space(4.0);
        caption(ui, &view.vitals);
        ui.add_space(4.0);
        caption(ui, &view.held);
        ui.add_space(16.0);

        section_title(ui, "BELT");
        lines(ui, &view.belt);
        ui.add_space(16.0);

        section_title(ui, "WORN");
        lines(ui, &view.wear);
        ui.add_space(16.0);

        section_title(ui, "INVENTORY");
        if !view.main_state.is_empty() {
            caption(ui, &view.main_state);
            ui.add_space(4.0);
        }
        lines(ui, &view.main);
    });
}

fn lines(ui: &mut Ui, lines: &[String]) {
    for line in lines {
        ui.label(RichText::new(line).monospace());
    }
}
