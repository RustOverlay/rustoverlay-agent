//! Drawing helpers for the overlay, which sits straight on the game with no surface behind it: every
//! glyph carries its own contrast via a drop shadow.

use std::sync::Arc;

use eframe::egui::{self, Align2, Color32, FontId, Galley, Painter, Pos2, Rect, text::LayoutJob};

use crate::icons::Icon;
use crate::ui::textures::IconTextures;

/// WPF's `DropShadowEffect` (depth 1 down, blur 4, black at 0.95), approximated by a few offset
/// copies since egui has no blur.
pub fn shadowed(painter: &Painter, pos: Pos2, galley: Arc<Galley>, color: Color32, scale: f32) {
    let shadow = Color32::from_black_alpha(140);
    for (dx, dy) in [(0.0, 1.0), (-1.0, 1.0), (1.0, 1.0), (0.0, 2.0)] {
        painter.galley_with_override_text_color(pos + egui::vec2(dx, dy) * scale, galley.clone(), shadow);
    }
    painter.galley_with_override_text_color(pos, galley, color);
}

/// One line of text, cut with an ellipsis past `max_width` when given (`TextTrimming`).
pub fn layout(painter: &Painter, text: &str, font: FontId, max_width: Option<f32>, italic: bool) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat { font_id: font, italics: italic, color: Color32::WHITE, ..Default::default() },
    );
    if let Some(width) = max_width {
        job.wrap.max_width = width;
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
    }
    painter.layout_job(job)
}

/// Text placed against a side of `rect`, vertically centred, with its shadow.
pub fn text_in(painter: &Painter, rect: Rect, align: Align2, galley: Arc<Galley>, color: Color32, scale: f32) {
    let pos = align.align_size_within_rect(galley.size(), rect).min;
    shadowed(&painter.with_clip_rect(rect.expand(2.0 * scale)), pos, galley, color, scale);
}

/// An icon fitted inside `rect` keeping its aspect ratio (`Stretch="Uniform"`).
pub fn icon_in(painter: &Painter, textures: &IconTextures, rect: Rect, icon: &Icon) {
    let texture = textures.get(painter.ctx(), icon);
    let [w, h] = icon.image.size;
    let fit = (rect.width() / w as f32).min(rect.height() / h as f32);
    let size = egui::vec2(w as f32, h as f32) * fit;
    let target = Rect::from_center_size(rect.center(), size);
    painter.image(texture.id(), target, Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
}
