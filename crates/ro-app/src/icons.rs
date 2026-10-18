//! The player's own item icons, decoded small and kept.
//!
//! Read from the install, never bundled — the icons are Facepunch's, and the matcher already reads
//! them from the same place. Decoded to [`SIZE`] rather than at source: the files are 512x512 and the
//! overlay draws them at a fraction of that, so full-size decodes would cost megabytes to throw away.
//! Decoding happens off the UI thread, wherever the overlay model is rebuilt; the view only uploads
//! the finished pixels as a texture (see `ui::textures`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui::ColorImage;
use image::imageops::FilterType;

/// Sized for the largest consumer, the inspector's grid squares; the compact row draws the same
/// bitmap at 18 and downscaling is free.
pub const SIZE: u32 = 48;

/// A decoded icon, keyed by its path so the view can cache one texture per file.
#[derive(Clone, Debug)]
pub struct Icon {
    pub key: String,
    pub image: Arc<ColorImage>,
}

impl PartialEq for Icon {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

#[derive(Default)]
pub struct ItemIcons {
    cache: Mutex<HashMap<String, Option<Icon>>>,
}

impl ItemIcons {
    pub fn new() -> Self {
        Self::default()
    }

    /// None for an item with no icon on disk, which the panel renders as text instead.
    pub fn load(&self, path: Option<&Path>) -> Option<Icon> {
        let path = path?;
        // OrdinalIgnoreCase, as the C# cache was keyed.
        let key = path.to_string_lossy().to_lowercase();

        if let Some(cached) = self.cache.lock().unwrap_or_else(PoisonError::into_inner).get(&key) {
            return cached.clone();
        }

        let decoded = decode(path).map(|image| Icon { key: key.clone(), image: Arc::new(image) });
        self.cache.lock().unwrap_or_else(PoisonError::into_inner).insert(key, decoded.clone());
        decoded
    }
}

/// An icon is cosmetic; a missing or corrupt one costs a picture, not a row. The whole file is read
/// up front, so it is never held open where a game update would need to replace it.
fn decode(path: &Path) -> Option<ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let full = image::load_from_memory(&bytes).ok()?.to_rgba8();
    if full.width() == 0 {
        return None;
    }

    // DecodePixelWidth: the width is fixed and the height follows the aspect ratio.
    let height = ((full.height() as f64 * SIZE as f64 / full.width() as f64).round() as u32).max(1);
    let small = image::imageops::resize(&full, SIZE, height, FilterType::Triangle);

    Some(ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_no_icon_and_is_remembered() {
        let icons = ItemIcons::new();
        let path = Path::new("Z:/definitely/not/here.png");

        assert!(icons.load(Some(path)).is_none());
        assert!(icons.load(None).is_none());
        assert_eq!(icons.cache.lock().unwrap().len(), 1);
    }

    #[test]
    fn decodes_to_the_fixed_width_and_shares_by_case_insensitive_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Icon.png");
        image::RgbaImage::from_pixel(512, 256, image::Rgba([10, 20, 30, 255])).save(&path).unwrap();

        let icons = ItemIcons::new();
        let icon = icons.load(Some(&path)).unwrap();
        let again = icons.load(Some(&dir.path().join("ICON.PNG"))).unwrap();

        assert_eq!(icon.image.size, [48, 24]);
        assert!(Arc::ptr_eq(&icon.image, &again.image));
    }
}
