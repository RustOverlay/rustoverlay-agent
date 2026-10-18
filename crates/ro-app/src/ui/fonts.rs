//! Segoe UI for text, Cascadia Mono (or Consolas) for data, Segoe MDL2 Assets for glyph buttons —
//! the WPF theme's `UiFont` / `DataFont`, read from the Windows font directory. A missing file
//! leaves egui's bundled font in its place.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};

const SEMIBOLD: &str = "semibold";
const ICONS: &str = "icons";

/// WPF's `FontWeight="SemiBold"`.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Segoe MDL2 Assets, for the delete glyph.
pub fn icons(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback_proportional = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fallback_monospace = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    let mut load = |name: &str, files: &[&str]| -> Option<String> {
        let bytes = files.iter().find_map(|file| std::fs::read(font_directory().join(file)).ok())?;
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
        Some(name.to_owned())
    };

    let regular = load("segoe-ui", &["segoeui.ttf"]);
    let semibold = load("segoe-ui-semibold", &["seguisb.ttf", "segoeuib.ttf"]);
    let mono = load("data-font", &["CascadiaMono.ttf", "consola.ttf"]);
    let glyphs = load("mdl2", &["segmdl2.ttf", "SegoeIcons.ttf"]);

    let family = |primary: Option<String>, fallback: &[String]| -> Vec<String> {
        primary.into_iter().chain(fallback.iter().cloned()).collect()
    };

    fonts.families.insert(FontFamily::Proportional, family(regular.clone(), &fallback_proportional));
    fonts.families.insert(FontFamily::Monospace, family(mono, &fallback_monospace));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), family(semibold.or(regular), &fallback_proportional));
    fonts.families.insert(FontFamily::Name(ICONS.into()), family(glyphs, &fallback_proportional));

    ctx.set_fonts(fonts);
}

fn font_directory() -> PathBuf {
    std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("Fonts")
}
