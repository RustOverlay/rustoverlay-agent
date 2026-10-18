//! Runs against the item bundles in the local install, skipped where Rust is absent — the
//! definitions cannot be vendored into the repo, so this is the only place the real schema is
//! exercised.

mod common;

use common::local_catalog;

#[test]
fn loads_the_local_item_bundles() {
    let Some(catalog) = local_catalog() else { return };

    eprintln!("definitions    {}", catalog.items().len());
    eprintln!("with icons     {}", catalog.with_icons().count());
    eprintln!("wearable       {}", catalog.wearable().count());
    eprintln!("holdable       {}", catalog.holdable().count());
    eprintln!("condition bar  {}", catalog.items().iter().filter(|i| i.has_condition_bar).count());

    assert!(!catalog.items().is_empty());
    assert!(catalog.items().iter().all(|i| !i.short_name.trim().is_empty()));
}

#[test]
fn resolves_a_known_weapon_with_its_metadata() {
    let Some(catalog) = local_catalog() else { return };

    let ak = catalog.by_short_name("rifle.ak").expect("rifle.ak is defined");

    assert_eq!(ak.name, "Assault Rifle");
    assert_eq!(ak.category, "Weapon");
    assert!(ak.is_holdable);
    assert!(!ak.is_wearable);
    assert!(ak.has_condition_bar);
    assert!(ak.icon_path.is_some());
    assert_eq!(catalog.by_id(ak.item_id), Some(ak));
}

#[test]
fn armour_candidates_are_a_small_fraction_of_the_catalogue() {
    let Some(catalog) = local_catalog() else { return };

    // The point of the wearable flag: an armour slot matches against a fraction of the corpus.
    let wearable = catalog.wearable().count();
    assert!((50..=catalog.items().len() / 3).contains(&wearable), "{wearable} wearable");
    assert!(catalog.wearable().all(|i| i.is_wearable));
}
