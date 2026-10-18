//! The skin icons Facepunch ships beside the item icons, `<shortname>.<skin>.skin.png`.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use super::{IconFace, ItemCatalog, ItemDefinition};

const SUFFIX: &str = ".skin.png";

/// Owners whose skins are flat full-canvas patterns, which is what an empty slot over plain ground
/// looks like; `wallpaper.wall` is filed as a Tool, so the category list lets its 57 skins through.
const FLAT_PATTERNS: &[&str] = &["wallpaper."];

/// Each shipped skin icon as a face of the item it repaints, for items `admit` accepts.
///
/// The owner is the longest short name the file name starts with, up to a `.` or `_`:
/// `rifle.ak.ak47u_diver.skin.png` belongs to `rifle.ak`, not `rifle`. A name no item answers to,
/// or whose item has no icon of its own to be a variant of, is skipped.
pub fn builtin_skin_faces(catalog: &ItemCatalog, items_directory: &Path, admit: impl Fn(&ItemDefinition) -> bool) -> Vec<IconFace> {
    let Ok(entries) = fs::read_dir(items_directory) else { return Vec::new() };
    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| stem(p).is_some() && p.is_file()).collect();
    // read_dir order is the filesystem's; sorted, the catalogue is the same on every machine.
    paths.sort();

    paths
        .into_iter()
        .filter_map(|path| {
            let item = owner(catalog, stem(&path)?)?;
            let flat = FLAT_PATTERNS.iter().any(|p| item.short_name.starts_with(p));
            (item.icon_path.is_some() && !flat && admit(item)).then(|| IconFace::new(Arc::clone(item), Some(path)))
        })
        .collect()
}

fn stem(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?;
    let cut = name.len().checked_sub(SUFFIX.len())?;
    name.get(cut..).is_some_and(|s| s.eq_ignore_ascii_case(SUFFIX)).then(|| &name[..cut])
}

fn owner<'a>(catalog: &'a ItemCatalog, stem: &str) -> Option<&'a Arc<ItemDefinition>> {
    stem.char_indices().rev().filter(|&(_, c)| c == '.' || c == '_').find_map(|(at, _)| catalog.by_short_name(&stem[..at]))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn definition(short_name: &str) -> ItemDefinition {
        ItemDefinition {
            item_id: 0,
            short_name: short_name.into(),
            name: short_name.into(),
            category: "Weapon".into(),
            is_wearable: false,
            is_holdable: true,
            has_condition_bar: false,
            stack_size: 1,
            icon_path: Some(PathBuf::from(format!("{short_name}.png"))),
        }
    }

    fn catalog() -> ItemCatalog {
        ItemCatalog::from_items(vec![definition("rifle"), definition("rifle.ak"), definition("door.hinged.metal"), definition("wallpaper.wall")])
    }

    #[test]
    fn a_skin_belongs_to_the_longest_short_name_it_starts_with() {
        let catalog = catalog();
        assert_eq!(owner(&catalog, "rifle.ak.ak47u_diver").map(|i| i.short_name.as_str()), Some("rifle.ak"));
        assert_eq!(owner(&catalog, "door.hinged.metal_medieval").map(|i| i.short_name.as_str()), Some("door.hinged.metal"));
    }

    #[test]
    fn a_name_no_item_answers_to_has_no_owner() {
        assert!(owner(&catalog(), "lr300.item.lr330.space").is_none());
    }

    #[test]
    fn only_skin_icons_have_a_stem() {
        assert_eq!(stem(Path::new("x/rifle.ak.diver.SKIN.png")), Some("rifle.ak.diver"));
        assert_eq!(stem(Path::new("x/rifle.ak.png")), None);
        assert_eq!(stem(Path::new("x/.skin.png")), Some(""));
    }

    #[test]
    fn shipped_skins_are_read_from_the_directory_in_name_order() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["rifle.ak.zeta.skin.png", "rifle.ak.alpha.skin.png", "rifle.ak.png", "nobody.thing.skin.png", "wallpaper.wall.brick.skin.png"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }

        let faces = builtin_skin_faces(&catalog(), dir.path(), |_| true);
        let names: Vec<_> = faces.iter().map(|f| f.icon_path.as_ref().unwrap().file_name().unwrap().to_owned()).collect();

        assert_eq!(names, ["rifle.ak.alpha.skin.png", "rifle.ak.zeta.skin.png"]);
        assert!(faces.iter().all(|f| f.item.short_name == "rifle.ak" && f.is_skin()));
    }
}
