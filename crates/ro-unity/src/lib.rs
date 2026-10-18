//! Approved-skin icons read straight from Rust's installed Unity asset bundles
//! (`<Rust>/Bundles/textures/textures.*.bundle`). Read-only; see `research/skin-icons-in-bundles.md`.

mod bc3;
mod common_strings;
mod cursor;
mod serialized_file;
mod texture;
mod type_tree;
mod unityfs;

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use cursor::invalid;
use serialized_file::SerializedFile;
use texture::TextureInfo;

const CLASS_TEXTURE2D: i32 = 28;
const CLASS_ASSET_BUNDLE: i32 = 142;

/// Decoded icon: straight alpha, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IconLocation {
    bundle: usize,
    /// Absolute offset of the pixel data in the bundle file.
    offset: u64,
    size: u32,
    width: u32,
    height: u32,
    format: i32,
}

/// Workshop id -> icon location, valid only for the game build it was built from.
#[derive(Debug)]
pub struct SkinIconIndex {
    bundles: Vec<PathBuf>,
    icons: HashMap<u64, IconLocation>,
}

impl SkinIconIndex {
    /// Indexes `assets/skins/<workshopid>/icon.png` textures in `textures_dir/textures.*.bundle`.
    pub fn build(textures_dir: &Path) -> io::Result<Self> {
        let mut bundles = Vec::new();
        for entry in std::fs::read_dir(textures_dir)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if name.starts_with("textures.") && name.ends_with(".bundle") {
                bundles.push(path);
            }
        }
        bundles.sort_by_key(|p| (bundle_number(p), p.clone()));

        let mut icons = HashMap::new();
        for (i, path) in bundles.iter().enumerate() {
            index_bundle(path, i, &mut icons)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))?;
        }
        Ok(Self { bundles, icons })
    }

    /// Decodes mip 0 of the icon, or `None` when the workshop id is not baked into this build.
    pub fn read_icon(&self, workshop_id: u64) -> io::Result<Option<RgbaImage>> {
        let Some(loc) = self.icons.get(&workshop_id) else {
            return Ok(None);
        };
        let need = texture::mip0_len(loc.format, loc.width, loc.height)?;
        if need > loc.size as usize {
            return Err(invalid(format!(
                "icon {workshop_id}: mip 0 needs {need} bytes, stream has {}",
                loc.size
            )));
        }
        let mut f = open_shared(&self.bundles[loc.bundle])?;
        f.seek(SeekFrom::Start(loc.offset))?;
        let mut data = vec![0; need];
        f.read_exact(&mut data)?;
        texture::decode(loc.format, &data, loc.width, loc.height).map(Some)
    }

    pub fn len(&self) -> usize {
        self.icons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.icons.is_empty()
    }
}

/// Std opens with read+write+delete sharing, so this never blocks a running game.
fn open_shared(path: &Path) -> io::Result<File> {
    File::open(path)
}

fn bundle_number(path: &Path) -> u32 {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let mid = name
        .trim_start_matches("textures.")
        .trim_end_matches(".bundle");
    mid.parse().unwrap_or(u32::MAX)
}

fn icon_workshop_id(container_path: &str) -> Option<u64> {
    let id = container_path
        .strip_prefix("assets/skins/")?
        .strip_suffix("/icon.png")?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}

fn index_bundle(
    path: &Path,
    bundle: usize,
    icons: &mut HashMap<u64, IconLocation>,
) -> io::Result<()> {
    let mut f = open_shared(path)?;
    let nodes = unityfs::read_directory(&mut f)?;
    let cab = nodes
        .iter()
        .find(|n| n.flags & unityfs::NODE_SERIALIZED_FILE != 0);
    let cab = cab.ok_or_else(|| invalid("no SerializedFile node"))?;
    let mut bytes =
        vec![0; usize::try_from(cab.size).map_err(|_| invalid("SerializedFile too large"))?];
    f.seek(SeekFrom::Start(cab.offset))?;
    f.read_exact(&mut bytes)?;
    let sf = SerializedFile::parse(bytes)?;

    for bundle_id in sf.path_ids_of_class(CLASS_ASSET_BUNDLE) {
        let asset_bundle = sf.read_object(bundle_id)?;
        for entry in asset_bundle.field("m_Container")?.array()? {
            let (key, info) = entry.pair()?;
            let Some(workshop_id) = icon_workshop_id(key.str()?) else {
                continue;
            };
            let asset = info.field("asset")?;
            let path_id = asset.field("m_PathID")?.int()?;
            if asset.field("m_FileID")?.int()? != 0 || sf.class_id(path_id) != Some(CLASS_TEXTURE2D)
            {
                continue;
            }
            let tex = TextureInfo::from_value(&sf.read_object(path_id)?)?;
            if tex.stream_path.is_empty() {
                continue;
            }
            let node = nodes.iter().find(|n| n.name == tex.stream_node_name());
            let node =
                node.ok_or_else(|| invalid(format!("stream node {} missing", tex.stream_path)))?;
            if tex.stream_offset + u64::from(tex.stream_size) > node.size {
                return Err(invalid(format!(
                    "icon {workshop_id} stream runs past {}",
                    node.name
                )));
            }
            let loc = IconLocation {
                bundle,
                offset: node.offset + tex.stream_offset,
                size: tex.stream_size,
                width: tex.width,
                height: tex.height,
                format: tex.format,
            };
            icons.insert(workshop_id, loc);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_path_and_bundle_order() {
        assert_eq!(
            icon_workshop_id("assets/skins/3208884292/icon.png"),
            Some(3208884292)
        );
        assert_eq!(
            icon_workshop_id("assets/skins/3208884292/_MainTex0.png"),
            None
        );
        assert_eq!(icon_workshop_id("assets/skins//icon.png"), None);
        assert_eq!(icon_workshop_id("assets/skins/12a/icon.png"), None);
        assert!(
            bundle_number(Path::new("x/textures.2.bundle"))
                < bundle_number(Path::new("textures.10.bundle"))
        );
    }
}

#[cfg(test)]
mod parity_tests;
