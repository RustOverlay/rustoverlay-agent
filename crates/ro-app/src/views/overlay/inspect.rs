//! The selected teammate's whole inventory, on screen only while the hold key is down. The main and
//! belt grids keep their blanks, since they place by position; the wear row closes up around what
//! the teammate is actually wearing.

use eframe::egui::{self, Align2, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

use super::paint::{icon_in, layout, text_in};
use crate::state::inspect::{BELT_COLUMNS, InspectCell, InspectView, MAIN_COLUMNS};
use crate::ui::fonts::semibold;
use crate::ui::textures::IconTextures;
use crate::ui::theme::{
    ACCENT, AMBIGUOUS_MARK, CELL_BORDER, CELL_EMPTY_FILL, CELL_FILL, FONT_SIZE_CAPTION, OVERLAY_STALE, RADIUS_SMALL,
    TEXT_PRIMARY,
};

const LABEL_COLUMN: f32 = 34.0;
const ROW_GAP: f32 = 3.0;

/// Draws the grids from `top_left`; returns the size they took.
pub fn draw(painter: &Painter, textures: &IconTextures, top_left: Pos2, view: &InspectView, cell_size: f32, scale: f32) -> Vec2 {
    let pitch = (cell_size + 2.0) * scale;
    let content_x = top_left.x + LABEL_COLUMN * scale;
    let mut y = top_left.y;
    let mut right = content_x;

    label(painter, egui::pos2(top_left.x, y), "wear", scale);
    let mut x = content_x;
    for cell in view.wear.iter().filter(|c| !c.is_empty) {
        square(painter, textures, egui::pos2(x, y), cell, cell_size, scale);
        x += pitch;
    }
    if view.has_backpack() {
        x += 10.0 * scale;
        x += label(painter, egui::pos2(x, y), "pack", scale) + 6.0 * scale;
        square(painter, textures, egui::pos2(x, y), &view.backpack[0], cell_size, scale);
        x += pitch;
    }
    right = right.max(x);
    y += pitch;

    for (name, cells, columns) in [("main", &view.main, MAIN_COLUMNS), ("belt", &view.belt, BELT_COLUMNS)] {
        y += ROW_GAP * scale;
        label(painter, egui::pos2(top_left.x, y), name, scale);
        for (i, cell) in cells.iter().enumerate() {
            let at = egui::pos2(content_x + (i % columns) as f32 * pitch, y + (i / columns) as f32 * pitch);
            square(painter, textures, at, cell, cell_size, scale);
        }
        right = right.max(content_x + columns as f32 * pitch);
        y += cells.len().div_ceil(columns) as f32 * pitch;
    }

    egui::vec2(right - top_left.x, y - top_left.y)
}

/// `GridLabel`: caption-sized, top-aligned, 3 down. Returns its width.
fn label(painter: &Painter, at: Pos2, text: &str, scale: f32) -> f32 {
    let galley = layout(painter, text, egui::FontId::proportional(FONT_SIZE_CAPTION * scale), None, false);
    let width = galley.size().x;
    let rect = Rect::from_min_size(at + egui::vec2(0.0, 3.0 * scale), galley.size());
    text_in(painter, rect, Align2::LEFT_TOP, galley, OVERLAY_STALE, scale);
    width
}

/// One square, 1 in from its slot on every side.
fn square(painter: &Painter, textures: &IconTextures, at: Pos2, cell: &InspectCell, cell_size: f32, scale: f32) {
    let rect = Rect::from_min_size(at + egui::vec2(1.0, 1.0) * scale, egui::vec2(cell_size, cell_size) * scale);
    let radius = CornerRadius::same(RADIUS_SMALL);
    let fill = if cell.is_empty { CELL_EMPTY_FILL } else { CELL_FILL };
    let border = if cell.held { Stroke::new(2.0 * scale, ACCENT) } else { Stroke::new(scale, CELL_BORDER) };
    painter.rect(rect, radius, fill, border, StrokeKind::Inside);

    if let Some(icon) = &cell.icon {
        icon_in(painter, textures, rect.shrink(3.0 * scale), icon);
    }

    if !cell.count.is_empty() {
        let galley = layout(painter, &cell.count, semibold(FONT_SIZE_CAPTION * scale), None, false);
        let area = Rect::from_min_max(rect.min, rect.max - egui::vec2(3.0, 1.0) * scale);
        text_in(painter, area, Align2::RIGHT_BOTTOM, galley, TEXT_PRIMARY, scale);
    }

    if cell.ambiguous {
        let corner = egui::pos2(rect.min.x + 2.0 * scale, rect.max.y - 2.0 * scale);
        let side = 7.0 * scale;
        let points = vec![corner, corner + egui::vec2(side, 0.0), corner + egui::vec2(side, -side)];
        painter.add(Shape::convex_polygon(points, AMBIGUOUS_MARK, Stroke::NONE));
    }
}
