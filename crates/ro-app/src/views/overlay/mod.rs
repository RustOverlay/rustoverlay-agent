//! The teammate panel drawn over the game (OverlayWindow.xaml): one row per teammate, then the
//! selected teammate's inventory while the hold key is down.

mod inspect;
mod paint;

use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui::{self, Align2, Rect, Sense, Ui, Vec2};
use ro_core::hosting::OverlaySettingsStore;

use self::paint::{icon_in, layout, text_in};
use crate::state::overlay::OverlayModel;
use crate::state::overlay_member::{HealthState, OverlayMemberRow};
use crate::ui::fonts::semibold;
use crate::ui::textures::IconTextures;
use crate::ui::theme::{
    ACCENT, FONT_SIZE_BODY, HEALTH_CRITICAL, HEALTH_FULL, HEALTH_HURT, HEALTH_TRACK, OVERLAY_STALE, TEXT_PRIMARY,
};

/// Column widths of the row grid, unscaled.
const COLUMNS: [f32; 10] = [12.0, 96.0, 86.0, 30.0, 26.0, 72.0, 22.0, 24.0, 22.0, 24.0];
const ROW_HEIGHT: f32 = 18.0;
const ROW_GAP: f32 = 3.0;
const ICON: f32 = 18.0;
const MARGIN: f32 = 2.0;

/// Everything the overlay viewport's paint callback needs; cheap to clone into it.
#[derive(Clone)]
pub struct OverlayView {
    pub model: Arc<OverlayModel>,
    pub settings: Arc<OverlaySettingsStore>,
    pub textures: Arc<IconTextures>,
    /// The size last asked of the window, so it is resized only when the content changes.
    pub sized: Arc<Mutex<Vec2>>,
}

impl OverlayView {
    pub fn show(&self, ui: &mut Ui) {
        let options = self.settings.current();
        // One factor for the whole overlay: see OverlayOptions.scale on why it is not per element.
        let scale = options.scale as f32;
        let state = self.model.state();
        // Hidden is parked off-screen by OverlayInput; shrinking as well would only resize it twice.
        if !state.panel_visible {
            return;
        }

        let origin = ui.max_rect().min + egui::vec2(MARGIN, MARGIN) * scale;
        let painter = ui.painter().clone();
        let mut y = origin.y;
        let width: f32 = COLUMNS.iter().sum::<f32>() * scale;

        for member in &state.members {
            let rect = Rect::from_min_size(egui::pos2(origin.x, y), egui::vec2(width, ROW_HEIGHT * scale));
            self.row(&painter, rect, member, scale);
            y += (ROW_HEIGHT + ROW_GAP) * scale;
        }

        // Laid out even while closed, clipped away, so the window is already big enough and opening
        // it costs no resize; a resize is a round trip through the window manager, and drawing is
        // ~80 µs. It also uploads the icons before the first open.
        let mut extent = egui::vec2(width, y - origin.y);
        let inspect_scale = scale * options.inspect.scale as f32;
        let top_left = egui::pos2(origin.x + 12.0 * scale, y + 5.0 * scale);
        let target = if state.inspect.visible { painter.clone() } else { painter.with_clip_rect(Rect::NOTHING) };
        let drawn = inspect::draw(&target, &self.textures, top_left, &state.inspect, options.inspect.cell_size as f32, inspect_scale);
        extent.x = extent.x.max(drawn.x + 12.0 * scale);
        extent.y += 5.0 * scale + drawn.y;
        drop(state);

        let size = extent + egui::vec2(MARGIN, MARGIN) * 2.0 * scale;
        ui.allocate_rect(Rect::from_min_size(ui.max_rect().min, size), Sense::hover());
        self.fit_window(ui.ctx(), size);
    }

    /// Grow-only: the spare area is transparent and click-through, while each resize is a window
    /// manager round trip, and teammates' different wear rows would otherwise resize it on every switch.
    fn fit_window(&self, ctx: &egui::Context, size: Vec2) {
        let mut sized = self.sized.lock().unwrap_or_else(PoisonError::into_inner);
        let size = sized.max(size);
        if (*sized - size).length() > 0.5 {
            *sized = size;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size.max(egui::vec2(1.0, 1.0))));
        }
    }

    fn row(&self, painter: &egui::Painter, rect: Rect, member: &OverlayMemberRow, scale: f32) {
        let mut x = rect.min.x;
        let cells: Vec<Rect> = COLUMNS
            .iter()
            .map(|w| {
                let cell = Rect::from_min_max(egui::pos2(x, rect.min.y), egui::pos2(x + w * scale, rect.max.y));
                x += w * scale;
                cell
            })
            .collect();
        let font = egui::FontId::proportional(FONT_SIZE_BODY * scale);
        let inset = |cell: Rect, left: f32, right: f32| {
            Rect::from_min_max(cell.min + egui::vec2(left * scale, 0.0), cell.max - egui::vec2(right * scale, 0.0))
        };
        let plain = |cell: Rect, text: &str, align: Align2, color, italic: bool| {
            let galley = layout(painter, text, font.clone(), None, italic);
            text_in(painter, cell, align, galley, color, scale);
        };

        // Transparent rather than absent, so marking a row never shifts the columns.
        if member.is_selected {
            plain(cells[0], "\u{203A}", Align2::LEFT_CENTER, ACCENT, false);
        }

        let name = inset(cells[1], 0.0, 8.0);
        let galley = layout(painter, &member.display_name, semibold(FONT_SIZE_BODY * scale), Some(name.width()), false);
        text_in(painter, name, Align2::LEFT_CENTER, galley, TEXT_PRIMARY, scale);

        if member.has_health {
            health_bar(painter, inset(cells[2], 0.0, 8.0), member, scale);
        }
        plain(inset(cells[3], 0.0, 10.0), &member.health, Align2::RIGHT_CENTER, TEXT_PRIMARY, false);

        let icon_cell = |cell: Rect| Rect::from_center_size(cell.center(), egui::vec2(ICON, ICON) * scale);
        if let Some(icon) = &member.weapon_icon {
            icon_in(painter, &self.textures, icon_cell(cells[4]), icon);
        }
        plain(inset(cells[5], 4.0, 10.0), &format!("{} {}", member.held, member.ammo), Align2::LEFT_CENTER, TEXT_PRIMARY, false);

        let (meds_color, walls_color) = (stale_color(member.meds_stale), stale_color(member.walls_stale));
        if let Some(icon) = &member.med_icon {
            icon_in(painter, &self.textures, icon_cell(cells[6]), icon);
        }
        plain(inset(cells[7], 4.0, 0.0), &member.meds, Align2::LEFT_CENTER, meds_color, member.meds_stale);
        if let Some(icon) = &member.wall_icon {
            icon_in(painter, &self.textures, icon_cell(cells[8]), icon);
        }
        plain(inset(cells[9], 4.0, 0.0), &member.walls, Align2::LEFT_CENTER, walls_color, member.walls_stale);
    }
}

/// A stale count comes from a main grid last seen before the inventory shut.
fn stale_color(stale: bool) -> egui::Color32 {
    if stale { OVERLAY_STALE } else { TEXT_PRIMARY }
}

/// Square, unlike everything else in the theme, to read as the game's own bar.
fn health_bar(painter: &egui::Painter, cell: Rect, member: &OverlayMemberRow, scale: f32) {
    let track = Rect::from_center_size(cell.center(), egui::vec2(cell.width(), 4.0 * scale));
    painter.rect_filled(track, 0.0, HEALTH_TRACK);

    let fill = match member.health_state {
        HealthState::Full => HEALTH_FULL,
        HealthState::Hurt => HEALTH_HURT,
        HealthState::Critical => HEALTH_CRITICAL,
    };
    let filled = Rect::from_min_size(track.min, egui::vec2(track.width() * member.health_fraction as f32, track.height()));
    painter.rect_filled(filled, 0.0, fill);
}
