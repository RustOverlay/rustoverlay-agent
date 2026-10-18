//! What the screen reader saw, as it reaches a teammate. The invariants that matter are the two the
//! matcher earns its accuracy with: abstention stays abstention, and ambiguity stays ambiguity.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ro_net::relay::loadout_to_proto;
use ro_vision::hosting::LoadoutState;
use ro_vision::hud::{InventoryReading, SlotCount, SlotRect, SlotReading};
use ro_vision::items::{IconMatch, ItemDefinition};

/// 2026-08-09T10:19:00Z.
fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_786_270_740)
}

fn millis(at: SystemTime) -> i64 {
    at.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn item(item_id: i32, short_name: &str) -> Arc<ItemDefinition> {
    Arc::new(ItemDefinition {
        item_id,
        short_name: short_name.into(),
        name: short_name.into(),
        category: "Ammunition".into(),
        is_wearable: false,
        is_holdable: false,
        has_condition_bar: false,
        stack_size: 128,
        icon_path: None,
    })
}

fn empty(x: i32) -> SlotReading {
    SlotReading { slot: SlotRect::new(x, 0, 68), item: None, count: None }
}

fn filled(x: i32, item: IconMatch, count: Option<SlotCount>) -> SlotReading {
    SlotReading { slot: SlotRect::new(x, 0, 68), item: Some(item), count }
}

fn reading(belt: Vec<SlotReading>, main: Vec<SlotReading>) -> InventoryReading {
    InventoryReading { belt, main, wear: Vec::new(), backpack: None }
}

#[test]
fn an_unreadable_slot_is_omitted_rather_than_sent_blank() {
    let rifle = IconMatch::new(item(-1123473824, "rifle.ak"), 0.98);
    let mut state = LoadoutState::new();
    state.set(reading(vec![empty(0), filled(1, rifle, None), empty(2)], vec![]), None, None, false, now());

    let belt = loadout_to_proto(&state).belt.unwrap();

    assert_eq!(belt.slots.len(), 1);
    // Index is positional, so dropping slot 0 must not shift the rifle down to it.
    assert_eq!(belt.slots[0].index, 1);
}

#[test]
fn an_ambiguous_slot_carries_every_candidate() {
    let ammo = IconMatch {
        indistinguishable: vec![item(815896488, "ammo.rifle.hv")],
        ..IconMatch::new(item(-1211166256, "ammo.rifle"), 0.93)
    };
    let mut state = LoadoutState::new();
    state.set(reading(vec![filled(0, ammo, Some(SlotCount::new(64, true)))], vec![]), None, None, false, now());

    let slot = &loadout_to_proto(&state).belt.unwrap().slots[0];

    // The winner is excluded from the matcher's look-alike set, so both halves must be sent.
    assert_eq!(slot.item_id, [-1211166256, 815896488]);
    assert_eq!(slot.count, 64);
    assert!(slot.is_stack);
}

#[test]
fn a_confident_slot_carries_exactly_one_candidate() {
    let rifle = IconMatch::new(item(-1123473824, "rifle.ak"), 0.99);
    let mut state = LoadoutState::new();
    state.set(reading(vec![filled(0, rifle, Some(SlotCount::new(24, false)))], vec![]), None, None, false, now());

    let slot = &loadout_to_proto(&state).belt.unwrap().slots[0];

    assert_eq!(slot.item_id, [-1123473824]);
    assert!(!slot.is_stack);
}

#[test]
fn the_hidden_containers_carry_their_own_older_stamp() {
    let scrap = IconMatch::new(item(-932201673, "scrap"), 0.97);
    let later = now() + Duration::from_secs(4 * 60);
    let mut state = LoadoutState::new();
    state.set(reading(vec![], vec![filled(0, scrap.clone(), None)]), None, None, true, now());
    state.set(reading(vec![filled(0, scrap, None)], vec![]), None, None, false, later);

    let loadout = loadout_to_proto(&state);

    // The belt is live; the main grid is as it was when the panel shut, and says so.
    assert_eq!(loadout.belt.unwrap().read_at_ms, millis(later));
    let main = loadout.main.unwrap();
    assert_eq!(main.read_at_ms, millis(later));
    assert_eq!(main.slots.len(), 1);
}

#[test]
fn an_unknown_held_slot_is_absent_rather_than_zero() {
    let mut state = LoadoutState::new();
    state.set(reading(vec![], vec![]), None, None, false, now());

    assert_eq!(loadout_to_proto(&state).held_slot, None);
}

#[test]
fn the_held_slot_survives_the_wire() {
    let mut state = LoadoutState::new();
    state.set(reading(vec![], vec![]), Some(3), None, false, now());

    assert_eq!(loadout_to_proto(&state).held_slot, Some(3));
}
