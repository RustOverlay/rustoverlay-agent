//! The only place a colour is written down (Theme/Tokens.xaml and Controls.xaml). Rationale for
//! the palette, and what it was derived from, is in docs/ui-design.md.

use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Visuals, style::ScrollStyle};

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// From WPF's `#AARRGGBB`.
const fn argb(hex: u32) -> Color32 {
    Color32::from_rgba_unmultiplied_const((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, (hex >> 24) as u8)
}

// Surface ladder: window, card, input. Depth comes from stacking these, never from shadows.
pub const APP_BG: Color32 = rgb(0x0F1114);
pub const SURFACE: Color32 = rgb(0x171A20);
pub const SURFACE_ALT: Color32 = rgb(0x1E222A);
pub const SURFACE_HOVER: Color32 = rgb(0x262B35);

/// White at alpha, not an opaque grey: one brush composites over every surface above.
pub const CARD_BORDER: Color32 = argb(0x28FFFFFF);

pub const TEXT_PRIMARY: Color32 = rgb(0xE6E8EC);
pub const TEXT_SUBTLE: Color32 = rgb(0x7C8598);
pub const TEXT_CAPTION: Color32 = rgb(0x5A6475);

/// Cool, so the whole warm end of the spectrum stays free for combat semantics.
pub const ACCENT: Color32 = rgb(0x60CDFF);
pub const ACCENT_DARK: Color32 = rgb(0x1A4D6B);

pub const WARNING: Color32 = rgb(0xD9A441);
pub const DANGER: Color32 = rgb(0xF04747);

pub const KILL_FILL: Color32 = argb(0x33F04747);
pub const WOUND_FILL: Color32 = argb(0x33EF6C33);

/// Drawn straight onto the game with no surface behind it, so contrast comes from a shadow.
pub const OVERLAY_STALE: Color32 = rgb(0xA8B2C4);

// Inspector squares. Dark enough to sit an icon on whatever terrain is behind them.
pub const CELL_FILL: Color32 = argb(0xA60F1114);
pub const CELL_EMPTY_FILL: Color32 = argb(0x4D0F1114);
pub const CELL_BORDER: Color32 = argb(0x1FFFFFFF);

/// The matcher offered look-alikes and the icon shown is only the first of them.
pub const AMBIGUOUS_MARK: Color32 = rgb(0xD9A441);

// Health, spending the warm end the accent was kept clear of. The track is light, not dark: on a
// dark panel an empty bar has to still read as a bar, or nearly dead and not yet read look alike.
pub const HEALTH_TRACK: Color32 = argb(0x26FFFFFF);
pub const HEALTH_FULL: Color32 = rgb(0x5FD68B);
pub const HEALTH_HURT: Color32 = rgb(0xD9A441);
pub const HEALTH_CRITICAL: Color32 = rgb(0xF04747);

pub const RADIUS: u8 = 8;
pub const RADIUS_SMALL: u8 = 4;

pub const FONT_SIZE_BODY: f32 = 12.0;
pub const FONT_SIZE_CAPTION: f32 = 11.0;
pub const FONT_SIZE_TITLE: f32 = 20.0;

/// Applies the control styles to every widget egui draws.
pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let mut visuals = Visuals::dark();
        visuals.override_text_color = Some(TEXT_PRIMARY);
        visuals.panel_fill = APP_BG;
        visuals.window_fill = SURFACE;
        visuals.window_stroke = Stroke::new(1.0, CARD_BORDER);
        visuals.window_corner_radius = CornerRadius::same(RADIUS);
        visuals.window_shadow = egui::Shadow::NONE;
        visuals.popup_shadow = egui::Shadow::NONE;
        visuals.extreme_bg_color = APP_BG;
        visuals.faint_bg_color = SURFACE_ALT;
        visuals.hyperlink_color = ACCENT;
        visuals.selection.bg_fill = ACCENT_DARK;
        visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        visuals.text_cursor.stroke = Stroke::new(1.0, ACCENT);

        let radius = CornerRadius::same(RADIUS);
        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, CARD_BORDER);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        widgets.noninteractive.corner_radius = radius;

        widgets.inactive.bg_fill = SURFACE_ALT;
        widgets.inactive.weak_bg_fill = SURFACE_ALT;
        widgets.inactive.bg_stroke = Stroke::new(1.0, CARD_BORDER);
        widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        widgets.inactive.corner_radius = radius;

        widgets.hovered.bg_fill = SURFACE_HOVER;
        widgets.hovered.weak_bg_fill = SURFACE_HOVER;
        widgets.hovered.bg_stroke = Stroke::new(1.0, CARD_BORDER);
        widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        widgets.hovered.corner_radius = radius;
        widgets.hovered.expansion = 0.0;

        widgets.active.bg_fill = ACCENT_DARK;
        widgets.active.weak_bg_fill = ACCENT_DARK;
        widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
        widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
        widgets.active.corner_radius = radius;
        widgets.active.expansion = 0.0;

        widgets.open = widgets.active;
        style.visuals = visuals;

        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.window_margin = Margin::same(12);
        // Hairline scrollbars: thumb only, no track.
        style.spacing.scroll = ScrollStyle { bar_width: 4.0, floating: false, ..ScrollStyle::solid() };

        use egui::{FontFamily, FontId, TextStyle};
        style.text_styles = [
            (TextStyle::Small, FontId::new(FONT_SIZE_CAPTION, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(FONT_SIZE_BODY, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(FONT_SIZE_BODY, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(FONT_SIZE_TITLE, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(FONT_SIZE_BODY, FontFamily::Monospace)),
        ]
        .into();
    });
}
