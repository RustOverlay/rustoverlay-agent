//! Holds the hand-authored tables against the local install, skipped where Rust is absent. Its job
//! is to catch a game update renaming an item out from under a table with no other way to notice.

mod common;

use std::collections::BTreeSet;

use common::local_catalog;
use ro_vision::items::{LoadoutCounts, WeaponAmmo};

#[test]
fn every_mapped_weapon_still_exists_and_is_holdable() {
    let Some(catalog) = local_catalog() else { return };

    let missing: Vec<&str> = WeaponAmmo::BY_WEAPON.iter().map(|(w, _)| *w).filter(|w| catalog.by_short_name(w).is_none()).collect();

    assert!(missing.is_empty(), "unresolved weapons {missing:?}");
    for (weapon, _) in WeaponAmmo::BY_WEAPON {
        assert!(catalog.by_short_name(weapon).unwrap().is_holdable, "{weapon}");
    }
}

#[test]
fn every_mapped_round_still_exists_and_is_ammunition() {
    let Some(catalog) = local_catalog() else { return };

    let rounds: BTreeSet<&str> = WeaponAmmo::BY_WEAPON.iter().flat_map(|(_, ammo)| ammo.rounds().iter().copied()).collect();
    let missing: Vec<&str> = rounds.iter().copied().filter(|r| catalog.by_short_name(r).is_none()).collect();

    assert!(missing.is_empty(), "unresolved rounds {missing:?}");
    for round in rounds {
        assert_eq!(catalog.by_short_name(round).unwrap().category, "Ammunition", "{round}");
    }
}

#[test]
fn the_counted_items_still_resolve_to_what_the_panel_expects() {
    let Some(catalog) = local_catalog() else { return };

    let syringe = catalog.by_short_name(LoadoutCounts::MED_SHORT_NAME).expect("syringe is defined");
    let wall = catalog.by_short_name(LoadoutCounts::WALL_SHORT_NAME).expect("wall is defined");

    assert_eq!(syringe.category, "Medical");
    assert_eq!(wall.category, "Construction");

    // Both stack small, which is why the counts sum across slots rather than reading one.
    eprintln!("{} stacks to {}", syringe.short_name, syringe.stack_size);
    eprintln!("{} stacks to {}", wall.short_name, wall.stack_size);
    assert!((1..=10).contains(&syringe.stack_size));
    assert!((1..=10).contains(&wall.stack_size));
}

#[test]
fn the_map_covers_the_guns_a_squad_actually_fights_with() {
    // Deviation: C# returned early without an install, though this assertion needs none.
    // Not every holdable Weapon takes ammunition — melee, throwables and mod attachments share
    // the category — so this names the ones whose absence would be a real gap.
    let expected = [
        "rifle.ak", "rifle.bolt", "rifle.lr300", "rifle.semiauto", "rifle.sks", "smg.mp5", "smg.thompson", "pistol.m92",
        "pistol.python", "pistol.revolver", "shotgun.pump", "shotgun.double", "shotgun.spas12",
    ];

    for weapon in expected {
        assert!(WeaponAmmo::for_weapon(weapon).is_some(), "{weapon}");
    }
}
