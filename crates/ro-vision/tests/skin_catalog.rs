//! Built on a temporary tree shaped like Steam's workshop cache. Nothing here decodes an image, so
//! the icons are placeholders — see `SkinCatalog::forget` for the half that does.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ro_vision::items::{ItemCatalog, SkinCatalog};
use tempfile::TempDir;

struct Tree {
    root: TempDir,
}

impl Tree {
    fn new() -> Self {
        let tree = Self { root: tempfile::Builder::new().prefix("skins-").tempdir().unwrap() };
        fs::create_dir_all(tree.items()).unwrap();
        fs::create_dir_all(tree.skins()).unwrap();

        tree.item(1545779598, "rifle.ak", "Assault Rifle", "Weapon");
        tree.item(-194953424, "metal.facemask", "Metal Facemask", "Weapon");
        tree.item(-1543487707, "door.hinged.metal", "Sheet Metal Door", "Construction");
        tree.item(-110921842, "locker", "Locker", "Items");
        tree
    }

    fn items(&self) -> PathBuf {
        self.root.path().join("items")
    }

    fn skins(&self) -> PathBuf {
        self.root.path().join("workshop")
    }

    fn item(&self, id: i32, short_name: &str, name: &str, category: &str) {
        let json = format!(r#"{{"itemid":{id},"shortname":"{short_name}","Name":"{name}","Category":"{category}"}}"#);
        fs::write(self.items().join(format!("{short_name}.json")), json).unwrap();
        fs::write(self.items().join(format!("{short_name}.png")), "").unwrap();
    }

    /// `complete: false` leaves the manifest out, as Steam does while still downloading.
    fn skin(&self, id: &str, item_type: &str, complete: bool) -> PathBuf {
        let path = self.skins().join(id);
        fs::create_dir_all(&path).unwrap();

        fs::write(path.join("icon.png"), "").unwrap();
        // The store thumbnail, which sits beside the icon and is not what a slot draws.
        fs::write(path.join("workshop_icon.png"), "").unwrap();

        if complete {
            write_manifest(&path, &format!(r#"{{"ItemType":"{item_type}"}}"#));
        }

        path
    }

    fn catalog(&self) -> Arc<ItemCatalog> {
        Arc::new(ItemCatalog::load(&self.items()).unwrap())
    }

    fn load(&self) -> SkinCatalog {
        SkinCatalog::new(self.catalog(), self.skins(), None)
    }
}

fn write_manifest(path: &Path, text: &str) {
    fs::write(path.join("manifest.txt"), text).unwrap();
}

/// The skin resolves — the label is right and the item exists — and is dropped anyway. A locker is
/// a full-canvas dark rectangle, which is what an empty slot in shadow looks like.
#[test]
fn a_skin_of_an_item_outside_the_allowed_categories_is_not_a_face() {
    let tree = Tree::new();
    tree.skin("200", "Locker", true);

    assert!(tree.load().rescan().is_empty());
}

#[test]
fn a_widened_allow_list_admits_it() {
    let tree = Tree::new();
    tree.skin("201", "Locker", true);

    let mut catalog = SkinCatalog::new(tree.catalog(), tree.skins(), Some(HashSet::from(["Items".to_owned()])));
    let faces = catalog.rescan();

    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].item.short_name, "locker");
}

/// An excluded category is settled, not retried — the alternative is a parse every sweep.
#[test]
fn an_excluded_skin_is_not_reconsidered_on_the_next_sweep() {
    let tree = Tree::new();
    tree.skin("202", "Locker", true);

    let mut catalog = tree.load();
    catalog.rescan();

    assert!(catalog.rescan().is_empty());
    assert_eq!(catalog.unmapped_labels().count(), 0);
}

#[test]
fn resolves_a_label_that_matches_an_item_name() {
    let tree = Tree::new();
    tree.skin("100", "Metal Facemask", true);

    let faces = tree.load().rescan();

    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].item.short_name, "metal.facemask");
    assert_eq!(faces[0].icon_path, Some(tree.skins().join("100").join("icon.png")));
}

#[test]
fn resolves_a_label_only_the_table_knows() {
    // The workshop calls it AK47; the definition calls itself Assault Rifle.
    let tree = Tree::new();
    tree.skin("101", "AK47", true);

    let faces = tree.load().rescan();

    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].item.short_name, "rifle.ak");
}

#[test]
fn faces_of_one_item_stay_distinct() {
    let tree = Tree::new();
    tree.skin("102", "AK47", true);
    tree.skin("103", "AK47", true);

    let faces = tree.load().rescan();

    assert_eq!(faces.len(), 2);
    assert_eq!(faces.iter().map(|f| &f.item).collect::<HashSet<_>>().len(), 1);
    assert_eq!(faces.iter().map(|f| &f.icon_path).collect::<HashSet<_>>().len(), 2);
}

#[test]
fn a_label_no_item_answers_to_is_counted_rather_than_guessed() {
    let tree = Tree::new();
    tree.skin("104", "Plasma Rifle", true);

    let mut catalog = tree.load();

    assert!(catalog.rescan().is_empty());
    assert_eq!(catalog.unmapped_count("Plasma Rifle"), Some(1));
}

#[test]
fn unmapped_labels_list_in_first_seen_order() {
    // One sweep per label, so the order is fixed by the test rather than by directory enumeration.
    let tree = Tree::new();
    let mut catalog = tree.load();
    let labels = ["Zap Gun", "Plasma Rifle", "Mace", "Blaster", "Laser Sword", "Arc Caster"];

    for (i, label) in labels.iter().enumerate() {
        tree.skin(&format!("11{i}"), label, true);
        catalog.rescan();
    }
    tree.skin("120", "plasma rifle", true);
    catalog.rescan();

    let listed: Vec<_> = catalog.unmapped_labels().collect();

    assert_eq!(listed.iter().map(|(label, _)| *label).collect::<Vec<_>>(), labels);
    assert_eq!(listed[1], ("Plasma Rifle", 2));
}

#[test]
fn a_second_sweep_over_an_unchanged_cache_finds_nothing() {
    // What keeps the watcher from rebuilding the matcher on every pass.
    let tree = Tree::new();
    tree.skin("105", "AK47", true);
    tree.skin("106", "Plasma Rifle", true);

    let mut catalog = tree.load();

    assert_eq!(catalog.rescan().len(), 1);
    assert!(catalog.rescan().is_empty());
    assert_eq!(catalog.unmapped_count("Plasma Rifle"), Some(1));
}

#[test]
fn a_directory_still_being_written_is_read_again_later() {
    // Steam writes a skin's files one at a time, so a sweep can land between the icon and the
    // manifest. Remembering it then would lose the skin until restart.
    let tree = Tree::new();
    let path = tree.skin("107", "AK47", false);
    let mut catalog = tree.load();

    assert!(catalog.rescan().is_empty());

    write_manifest(&path, r#"{"ItemType":"AK47"}"#);
    let faces = catalog.rescan();

    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].item.short_name, "rifle.ak");
}

#[test]
fn a_malformed_manifest_is_retried_rather_than_written_off() {
    let tree = Tree::new();
    let path = tree.skin("108", "AK47", true);
    write_manifest(&path, r#"{"ItemType":"AK4"#);

    let mut catalog = tree.load();
    assert!(catalog.rescan().is_empty());

    write_manifest(&path, r#"{"ItemType":"AK47"}"#);
    assert_eq!(catalog.rescan().len(), 1);
}

#[test]
fn a_forgotten_face_is_offered_again() {
    // Only the decode can see a truncated PNG, so it hands the face back rather than the catalogue
    // swallowing it.
    let tree = Tree::new();
    tree.skin("109", "AK47", true);
    let mut catalog = tree.load();

    let faces = catalog.rescan();
    assert_eq!(faces.len(), 1);
    assert!(catalog.rescan().is_empty());

    catalog.forget(&faces[0]);

    assert_eq!(catalog.rescan(), faces);
}

#[test]
fn a_missing_cache_is_not_an_error() {
    // A player who has never met a skin has no workshop directory at all.
    let tree = Tree::new();
    let mut catalog = SkinCatalog::new(tree.catalog(), tree.root.path().join("absent"), None);

    assert!(catalog.rescan().is_empty());
    assert_eq!(catalog.unmapped_labels().count(), 0);
}
