//! One roster entry (TeamMemberCard.xaml).

use eframe::egui::{self, RichText, Sense, Ui};

use crate::state::team_member::TeamMemberCard;
use crate::ui::fonts::semibold;
use crate::ui::theme::{ACCENT, FONT_SIZE_BODY, TEXT_CAPTION, TEXT_SUBTLE};
use crate::ui::widgets::card;

pub fn show(ui: &mut Ui, member: &TeamMemberCard) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, if member.online { ACCENT } else { TEXT_CAPTION });
            ui.add_space(4.0);
            ui.label(RichText::new(&member.display_name).font(semibold(FONT_SIZE_BODY)));
            ui.add_space(4.0);
            ui.label(RichText::new(member.presence).color(TEXT_SUBTLE));
        });

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(&member.vitals);
        });
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(RichText::new(&member.held).color(TEXT_SUBTLE));
        });
    });
    ui.add_space(8.0);
}
