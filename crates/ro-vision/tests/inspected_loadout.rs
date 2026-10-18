//! Scattering a sparse loadout back into the grids it was read from. The invariants worth holding
//! are positional: a slot has to land in the square it came from, and a container that grew must
//! cost a square rather than panic.

use ro_vision::hud::InventoryGeometry;
use ro_vision::items::wire::{Container, Loadout, Slot};
use ro_vision::items::InspectedLoadout;

const RIFLE: i32 = 1545779598;
const AMMO_556: i32 = -1211166256;
const AMMO_HV: i32 = 1712070256;

const MAIN_SIZE: usize = InventoryGeometry::MAIN_COLUMNS * InventoryGeometry::MAIN_ROWS;

fn slot(index: u32, count: i32, is_stack: bool, ids: &[i32]) -> Slot {
    Slot { index, count, is_stack, item_id: ids.to_vec() }
}

fn container(slots: Vec<Slot>) -> Option<Container> {
    Some(Container { slots, ..Container::default() })
}

fn inspect(loadout: Loadout) -> InspectedLoadout {
    InspectedLoadout::from_loadout(Some(&loadout))
}

#[test]
fn every_grid_is_full_size_even_with_nothing_in_it() {
    let loadout = InspectedLoadout::from_loadout(None);

    assert_eq!(loadout.belt.len(), InventoryGeometry::BELT_SLOTS);
    assert_eq!(loadout.main.len(), MAIN_SIZE);
    assert_eq!(loadout.wear.len(), InventoryGeometry::WEAR_SLOTS);
    assert_eq!(loadout.backpack.len(), 1);
    assert!(loadout.main.iter().all(|cell| cell.is_empty()));
}

#[test]
fn a_slot_lands_in_the_square_its_index_names() {
    // Sparse and out of order: only slots 0 and 5 are occupied, and 5 arrives first.
    let loadout = inspect(Loadout {
        belt: container(vec![slot(5, 3, true, &[AMMO_556]), slot(0, 30, false, &[RIFLE])]),
        ..Loadout::default()
    });

    assert_eq!(loadout.belt[0].item_id, Some(RIFLE));
    assert_eq!(loadout.belt[5].item_id, Some(AMMO_556));
    assert!(loadout.belt[1..5].iter().all(|cell| cell.is_empty()));
}

#[test]
fn a_slot_past_the_end_of_the_grid_is_dropped_rather_than_thrown() {
    let loadout = inspect(Loadout {
        belt: container(vec![slot(0, 1, true, &[AMMO_556]), slot(99, 1, true, &[AMMO_556])]),
        ..Loadout::default()
    });

    assert_eq!(loadout.belt.len(), InventoryGeometry::BELT_SLOTS);
    assert!(!loadout.belt[0].is_empty());
}

#[test]
fn held_marks_exactly_the_belt_square_the_loadout_names() {
    let loadout = inspect(Loadout {
        held_slot: Some(2),
        belt: container(vec![slot(0, 1, true, &[AMMO_556]), slot(2, 30, false, &[RIFLE])]),
        ..Loadout::default()
    });

    assert!(loadout.belt[2].held);
    assert_eq!(loadout.belt.iter().filter(|cell| cell.held).count(), 1);
}

#[test]
fn held_marks_nothing_when_the_belt_was_too_uniform_to_tell() {
    let loadout = inspect(Loadout { belt: container(vec![slot(0, 30, false, &[RIFLE])]), ..Loadout::default() });

    assert!(!loadout.belt.iter().any(|cell| cell.held));
}

#[test]
fn a_slot_the_matcher_could_not_separate_keeps_the_first_id_and_says_so() {
    let loadout = inspect(Loadout {
        main: container(vec![slot(0, 120, true, &[AMMO_556, AMMO_HV]), slot(1, 120, true, &[AMMO_556])]),
        ..Loadout::default()
    });

    assert_eq!(loadout.main[0].item_id, Some(AMMO_556));
    assert!(loadout.main[0].ambiguous);
    assert!(!loadout.main[1].ambiguous);
}

#[test]
fn a_slot_with_no_candidates_at_all_stays_empty() {
    let loadout = inspect(Loadout { main: container(vec![slot(0, 5, true, &[])]), ..Loadout::default() });

    assert!(loadout.main[0].is_empty());
}

#[test]
fn wear_and_backpack_come_through_their_own_containers() {
    let loadout = inspect(Loadout {
        wear: container(vec![slot(3, 1, false, &[RIFLE])]),
        backpack: container(vec![slot(0, 1, false, &[AMMO_556])]),
        ..Loadout::default()
    });

    assert_eq!(loadout.wear[3].item_id, Some(RIFLE));
    assert_eq!(loadout.backpack[0].item_id, Some(AMMO_556));
}
