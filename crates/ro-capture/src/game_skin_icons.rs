//! Approved skins' icons out of Rust's own asset bundles, as PNG files the signature pipeline reads
//! like any other icon. Nothing is downloaded.

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

use ro_core::install::{RustInstall, SkinDefinitions};
use ro_unity::SkinIconIndex;

/// Steam's list of approved skins and the game's index of their icons, together.
pub struct GameSkinIcons {
    definitions: SkinDefinitions,
    bundles: SkinIconIndex,
}

impl GameSkinIcons {
    /// None, logged, when Steam has no schema cached or the bundles cannot be read — the second is
    /// what a game update that changes the bundle format looks like, and matching carries on.
    pub fn load(steam: &Path, install: &RustInstall) -> Option<Self> {
        let Some(definitions) = SkinDefinitions::load(steam) else {
            tracing::info!("No Steam item schema cached; approved skins without a local icon stay unread");
            return None;
        };
        let textures = install.root().join("Bundles").join("textures");
        match SkinIconIndex::build(&textures) {
            Ok(bundles) => {
                tracing::info!(skins = definitions.skin_count(), icons = bundles.len(), "Indexed approved skins' icons in the game's bundles");
                Some(Self { definitions, bundles })
            }
            Err(error) => {
                tracing::warn!(path = %textures.display(), %error, "Could not index the game's skin bundles; approved skins stay unread");
                None
            }
        }
    }

    pub fn has_skins(&self, item_short_name: &str) -> bool {
        !self.definitions.for_item(item_short_name).is_empty()
    }

    /// The item's skin icons as PNGs under `directory`, extracting any not already there. A skin
    /// newer than the installed game has no icon in its bundles and is left out.
    pub fn extract(&self, item_short_name: &str, directory: &Path) -> Vec<PathBuf> {
        if std::fs::create_dir_all(directory).is_err() {
            return Vec::new();
        }

        let mut paths = Vec::new();
        for skin in self.definitions.for_item(item_short_name) {
            let path = directory.join(format!("{}.png", skin.workshop_id));
            if !path.is_file() {
                match self.bundles.read_icon(skin.workshop_id).and_then(|icon| icon.map(|i| write_png(&path, &i)).transpose()) {
                    Ok(Some(())) => {}
                    Ok(None) => continue,
                    Err(error) => {
                        tracing::debug!(workshop_id = skin.workshop_id, %error, "Could not extract a skin icon");
                        continue;
                    }
                }
            }
            paths.push(path);
        }
        paths
    }
}

fn write_png(path: &Path, icon: &ro_unity::RgbaImage) -> io::Result<()> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), icon.width, icon.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = icon.pixels.iter().flatten().copied().collect();
    encoder.write_header().and_then(|mut w| w.write_image_data(&bytes)).map_err(io::Error::other)
}
