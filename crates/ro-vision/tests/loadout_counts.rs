//! The counts an overlay row shows. The invariants worth holding keep a wrong number off the
//! panel: an ambiguous slot abstains, and a stale container says so.

use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ro_vision::items::wire::{Container, Loadout, Slot};
use ro_vision::items::{ItemCatalog, ItemDefinition, LoadoutCounts};

/// 2026-08-28T18:00:00Z.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_787_940_000)
}

const SYRINGE: i32 = 1079279582;
const WALL: i32 = 1373240771;
const RIFLE: i32 = 1545779598;
const AMMO_556: i32 = -1211166256;
const AMMO_HV: i32 = 1712070256;
const AMMO_PISTOL: i32 = 785728077;
const SNOWBALL: i32 = 550753330;
const BOW: i32 = 1443579727;

fn item(id: i32, short_name: &str, category: &str, stack: i32, holdable: bool) -> ItemDefinition {
    ItemDefinition {
        item_id: id,
        short_name: short_name.to_owned(),
        name: short_name.to_owned(),
        category: category.to_owned(),
        is_wearable: false,
        is_holdable: holdable,
        has_condition_bar: false,
        stack_size: stack,
        icon_path: None,
    }
}

static COUNTS: LazyLock<LoadoutCounts> = LazyLock::new(|| {
    let catalog = ItemCatalog::from_items(vec![
        item(SYRINGE, "syringe.medical", "Medical", 2, false),
        item(WALL, "barricade.wood.cover", "Construction", 3, false),
        item(RIFLE, "rifle.ak", "Weapon", 1, true),
        item(BOW, "bow.hunting", "Weapon", 1, true),
        item(AMMO_556, "ammo.rifle", "Ammunition", 128, false),
        item(AMMO_HV, "ammo.rifle.hv", "Ammunition", 128, false),
        item(AMMO_PISTOL, "ammo.pistol", "Ammunition", 128, false),
        item(SNOWBALL, "ammo.snowballgun", "Ammunition", 128, false),
    ]);

    LoadoutCounts::new(Some(Arc::new(catalog)), Duration::from_secs(5))
});

fn stack(index: u32, count: i32, ids: &[i32]) -> Slot {
    Slot { index, count, is_stack: true, item_id: ids.to_vec() }
}

fn single(index: u32, ids: &[i32]) -> Slot {
    Slot { index, item_id: ids.to_vec(), ..Slot::default() }
}

fn container(read_at: SystemTime, slots: Vec<Slot>) -> Container {
    Container { read_at_ms: read_at.duration_since(UNIX_EPOCH).unwrap().as_millis() as i64, slots }
}

fn belt(slots: Vec<Slot>) -> Loadout {
    Loadout { belt: Some(container(now(), slots)), ..Loadout::default() }
}

fn with_held_rifle(mut loadout: Loadout) -> Loadout {
    let slots = &mut loadout.belt.as_mut().unwrap().slots;
    let index = slots.len() as u32;
    slots.push(single(index, &[RIFLE]));
    loadout.held_slot = Some(index);

    loadout
}

fn value(result: Option<ro_vision::items::CountResult>) -> Option<i32> {
    result.map(|r| r.value)
}

#[test]
fn syringes_sum_across_every_slot_that_holds_them() {
    // syringe.medical stacks to 2, so a real load is several slots, not one.
    let loadout = belt(vec![stack(0, 2, &[SYRINGE]), stack(1, 2, &[SYRINGE]), stack(2, 1, &[SYRINGE])]);

    assert_eq!(value(COUNTS.meds(Some(&loadout), now())), Some(5));
}

#[test]
fn a_lone_item_counts_as_one_despite_carrying_no_number() {
    // Rust draws no number on a single item, so CountReader reports nothing and count stays 0.
    assert_eq!(value(COUNTS.meds(Some(&belt(vec![single(0, &[SYRINGE])])), now())), Some(1));
}

#[test]
fn an_ambiguous_slot_counts_when_every_candidate_agrees() {
    let loadout = with_held_rifle(belt(vec![stack(0, 64, &[AMMO_556, AMMO_HV])]));

    assert_eq!(value(COUNTS.spare_ammo(Some(&loadout), now())), Some(64));
}

#[test]
fn an_ambiguous_slot_abstains_when_the_candidates_disagree() {
    // ammo.rifle and ammo.snowballgun tie at 0.884 in the corpus; one is a bullet and one is not.
    let loadout = with_held_rifle(belt(vec![stack(0, 64, &[AMMO_556, SNOWBALL]), stack(1, 30, &[AMMO_556])]));

    assert_eq!(value(COUNTS.spare_ammo(Some(&loadout), now())), Some(30));
}

#[test]
fn an_id_the_catalogue_does_not_know_abstains() {
    assert_eq!(value(COUNTS.meds(Some(&belt(vec![stack(0, 4, &[12345])])), now())), Some(0));
}

#[test]
fn spare_ammo_ignores_rounds_the_held_weapon_cannot_fire() {
    let loadout = with_held_rifle(belt(vec![stack(0, 120, &[AMMO_556]), stack(1, 60, &[AMMO_PISTOL])]));

    assert_eq!(value(COUNTS.spare_ammo(Some(&loadout), now())), Some(120));
}

#[test]
fn a_weapon_with_no_mapped_ammunition_reports_nothing_rather_than_everything() {
    let mut loadout = belt(vec![stack(0, 120, &[AMMO_556]), single(1, &[BOW])]);
    loadout.held_slot = Some(1);

    assert_eq!(COUNTS.spare_ammo(Some(&loadout), now()), None);
    assert_eq!(COUNTS.unmapped_weapon(Some(&loadout)).as_deref(), Some("bow.hunting"));
}

#[test]
fn a_mapped_weapon_is_not_reported_as_drift() {
    assert_eq!(COUNTS.unmapped_weapon(Some(&with_held_rifle(belt(vec![single(0, &[RIFLE])])))), None);
}

#[test]
fn a_contributing_stale_main_grid_marks_the_total() {
    let four_minutes_ago = now() - Duration::from_secs(240);
    let mut loadout = belt(vec![stack(0, 2, &[SYRINGE])]);
    loadout.main = Some(container(four_minutes_ago, vec![stack(0, 2, &[SYRINGE])]));

    let meds = COUNTS.meds(Some(&loadout), now()).expect("counted");

    assert_eq!(meds.value, 4);
    assert!(meds.stale);
    assert_eq!(meds.stale_as_of, Some(four_minutes_ago));
}

#[test]
fn a_stale_main_grid_that_contributes_nothing_leaves_the_total_live() {
    let mut loadout = belt(vec![stack(0, 2, &[SYRINGE])]);
    loadout.main = Some(container(now() - Duration::from_secs(240), vec![stack(0, 500, &[AMMO_556])]));

    let meds = COUNTS.meds(Some(&loadout), now()).expect("counted");

    assert_eq!(meds.value, 2);
    assert!(!meds.stale);
    assert_eq!(meds.stale_as_of, None);
}

#[test]
fn an_unrepresentable_main_grid_timestamp_counts_as_no_timestamp() {
    let mut loadout = belt(vec![stack(0, 2, &[SYRINGE])]);
    loadout.main = Some(Container { read_at_ms: i64::MAX, slots: vec![stack(0, 2, &[SYRINGE])] });

    let meds = COUNTS.meds(Some(&loadout), now()).expect("counted");

    assert_eq!(meds.value, 4);
    assert!(!meds.stale);
}

#[test]
fn wire_counts_that_would_overflow_saturate() {
    let mut loadout = belt(vec![stack(0, i32::MAX, &[SYRINGE]), stack(1, i32::MAX, &[SYRINGE])]);
    loadout.main = Some(container(now(), vec![stack(0, i32::MAX, &[SYRINGE])]));

    assert_eq!(value(COUNTS.meds(Some(&loadout), now())), Some(i32::MAX));
}

#[test]
fn walls_come_from_the_barricade_cover_alone() {
    let loadout = belt(vec![stack(0, 3, &[WALL]), stack(1, 3, &[SYRINGE])]);

    assert_eq!(value(COUNTS.walls(Some(&loadout), now())), Some(3));
}

#[test]
fn magazine_rounds_are_the_bare_number_on_the_held_slot() {
    let mut loadout = belt(vec![Slot { index: 0, count: 24, is_stack: false, item_id: vec![RIFLE] }]);
    loadout.held_slot = Some(0);

    assert_eq!(LoadoutCounts::magazine_rounds(Some(&loadout)), Some(24));
}

#[test]
fn a_stack_count_on_the_held_slot_is_not_a_magazine() {
    let mut loadout = belt(vec![stack(0, 64, &[AMMO_556])]);
    loadout.held_slot = Some(0);

    assert_eq!(LoadoutCounts::magazine_rounds(Some(&loadout)), None);
}

#[test]
fn an_unread_loadout_is_unknown_rather_than_zero() {
    assert_eq!(COUNTS.meds(None, now()), None);
    assert_eq!(COUNTS.meds(Some(&Loadout::default()), now()), None);
    assert_eq!(
        LoadoutCounts::new(None, Duration::from_secs(5)).meds(Some(&belt(vec![single(0, &[SYRINGE])])), now()),
        None
    );
}
