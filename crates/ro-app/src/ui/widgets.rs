//! The reusable control styles from Controls.xaml: card, section title, caption, banner, the
//! danger glyph button and the accent-underline tabs.

use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, Response, RichText, Sense, Stroke, Ui};

use super::fonts::{icons, semibold};
use super::theme::{
    ACCENT, CARD_BORDER, DANGER, FONT_SIZE_BODY, FONT_SIZE_CAPTION, KILL_FILL, RADIUS, RADIUS_SMALL, SURFACE,
    SURFACE_ALT, TEXT_CAPTION, TEXT_PRIMARY, TEXT_SUBTLE,
};

/// `Card`: everything lives in one. Surface fill, alpha border, radius 8, padding 12.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    card_padded(ui, 12, add)
}

pub fn card_padded<R>(ui: &mut Ui, padding: i8, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, CARD_BORDER))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin::same(padding))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// `SectionTitle`: small, semibold, caption-coloured, 6 below.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(semibold(FONT_SIZE_CAPTION)).color(TEXT_CAPTION));
    ui.add_space(2.0);
}

/// `Caption`: secondary text.
pub fn caption(ui: &mut Ui, text: &str) -> Response {
    ui.label(RichText::new(text).color(TEXT_SUBTLE))
}

/// A tinted notice: warning or error text on its own fill and border.
pub fn banner<R>(ui: &mut Ui, fill: Color32, border: Color32, padding: Margin, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(RADIUS_SMALL))
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// `DangerIcon`: a destructive action that only turns red under the cursor. A permanently red bin
/// on every row would shout over Connect, which is what the list is actually for.
pub fn danger_icon(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(glyph.to_owned(), icons(15.0), TEXT_CAPTION);
    let size = galley.size() + egui::vec2(18.0, 12.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let hot = response.hovered() || response.is_pointer_button_down_on();
    if hot {
        let fill = if response.is_pointer_button_down_on() { KILL_FILL.gamma_multiply(0.7) } else { KILL_FILL };
        ui.painter().rect_filled(rect, CornerRadius::same(RADIUS), fill);
    }
    let color = if hot { DANGER } else { TEXT_CAPTION };
    ui.painter().galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);

    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tooltip)
}

/// `TabItem`: no boxes, a 3px accent underline under the selected tab, semibold when selected.
pub fn tab_strip(ui: &mut Ui, labels: &[&str], selected: &mut usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (index, label) in labels.iter().enumerate() {
            let is_selected = *selected == index;
            let font = if is_selected { semibold(FONT_SIZE_BODY) } else { egui::FontId::proportional(FONT_SIZE_BODY) };
            let galley = ui.painter().layout_no_wrap((*label).to_owned(), font, TEXT_PRIMARY);
            let padding = egui::vec2(14.0, 8.0);
            let size = galley.size() + padding * 2.0 + egui::vec2(0.0, 3.0);
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());

            let body = egui::Rect::from_min_max(rect.min, rect.max - egui::vec2(0.0, 3.0));
            if response.hovered() {
                ui.painter().rect_filled(body, CornerRadius::same(RADIUS_SMALL), SURFACE_ALT);
            }
            if is_selected {
                let bar = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 3.0), rect.max);
                ui.painter().rect_filled(bar, CornerRadius::same(2), ACCENT);
            }

            let color = if is_selected || response.hovered() { TEXT_PRIMARY } else { TEXT_SUBTLE };
            ui.painter().galley_with_override_text_color(body.center() - galley.size() / 2.0, galley, color);

            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *selected = index;
            }
        }
    });
}
