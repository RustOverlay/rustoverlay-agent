//! Helpers shared by the ported test files. Not every file uses every helper.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use ro_vision::items::{IconSignature, ItemCatalog, ItemDefinition};

pub const N: usize = IconSignature::SIZE;

/// Stand-in for C#'s `string.GetHashCode()`, which the tests only use as an arbitrary seed or id.
pub fn string_hash(s: &str) -> u32 {
    s.bytes().fold(2166136261u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619))
}

/// A holdable weapon named after itself, as the C# tests' `Item(shortName)` built.
pub fn item(short_name: &str) -> Arc<ItemDefinition> {
    Arc::new(ItemDefinition {
        item_id: string_hash(short_name) as i32,
        short_name: short_name.to_owned(),
        name: short_name.to_owned(),
        category: "Weapon".to_owned(),
        is_wearable: false,
        is_holdable: true,
        has_condition_bar: false,
        stack_size: 1,
        icon_path: Some(PathBuf::from("icon.png")),
    })
}

pub fn opaque() -> Vec<f32> {
    vec![1.0; N * N]
}

/// A signature painted cell by cell at uniform opacity.
pub fn signature(paint: impl Fn(usize, usize) -> (f32, f32, f32), opacity: f32) -> IconSignature {
    let mut rgb = vec![0f32; N * N * 3];

    for y in 0..N {
        for x in 0..N {
            let (r, g, b) = paint(x, y);
            let cell = (y * N + x) * 3;
            (rgb[cell], rgb[cell + 1], rgb[cell + 2]) = (r, g, b);
        }
    }

    IconSignature::new(rgb, vec![opacity; N * N])
}

/// The local install's item bundles, or None where Rust is absent so the caller can skip.
///
/// Deviation: C# located the install through the Steam registry (`SteamRustInstallLocator`, which
/// lives in ro-core); this checks `RUST_INSTALL` and then the default Steam library path.
pub fn local_catalog() -> Option<ItemCatalog> {
    let root = std::env::var_os("RUST_INSTALL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Rust"));
    let items = root.join("Bundles").join("items");

    if !items.is_dir() {
        eprintln!("No local Rust install with item bundles — skipped.");
        return None;
    }

    Some(ItemCatalog::load(&items).expect("item bundles are readable"))
}
