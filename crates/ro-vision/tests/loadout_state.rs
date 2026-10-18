//! Retention of the containers that are only drawn while the inventory panel is open.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ro_vision::hosting::LoadoutState;
use ro_vision::hud::{InventoryReading, SlotReading, SlotRect};

/// 2026-08-09T10:19:00Z.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_786_270_740)
}

fn at(seconds: u64) -> SystemTime {
    now() + Duration::from_secs(seconds)
}

fn slot(x: i32) -> SlotReading {
    SlotReading { slot: SlotRect::new(x, 0, 68), item: None, count: None }
}

fn reading(main_slots: i32) -> InventoryReading {
    InventoryReading { belt: vec![slot(0)], main: (1..=main_slots).map(slot).collect(), wear: Vec::new(), backpack: None }
}

#[test]
fn nothing_is_retained_before_the_panel_has_ever_been_open() {
    let mut state = LoadoutState::new();
    state.set(reading(0), None, None, false, now());

    assert!(state.last_open_inventory().is_none());
    assert_eq!(state.hidden_since(), None);
}

#[test]
fn the_panel_is_kept_after_it_shuts() {
    let mut state = LoadoutState::new();
    state.set(reading(4), None, None, true, now());
    state.set(reading(0), None, None, false, at(1));

    assert!(!state.inventory_open());
    assert_eq!(state.last_open_inventory().unwrap().main.len(), 4);
    assert_eq!(state.hidden_since(), Some(at(1)));
}

#[test]
fn staleness_is_measured_from_the_first_shut_frame_not_the_latest() {
    let mut state = LoadoutState::new();
    state.set(reading(4), None, None, true, now());
    state.set(reading(0), None, None, false, at(1));
    state.set(reading(0), None, None, false, at(9));

    assert_eq!(state.hidden_since(), Some(at(1)));
}

#[test]
fn reopening_replaces_the_retained_panel_and_clears_staleness() {
    let mut state = LoadoutState::new();
    state.set(reading(4), None, None, true, now());
    state.set(reading(0), None, None, false, at(1));
    state.set(reading(2), None, None, true, at(2));

    assert_eq!(state.last_open_inventory().unwrap().main.len(), 2);
    assert_eq!(state.hidden_since(), None);
}

#[test]
fn listeners_hear_every_set_after_it_has_applied() {
    // Not a C# test: the `Changed` event becomes `subscribe`, so the port checks it fires.
    let heard = Arc::new(AtomicU32::new(0));
    let mut state = LoadoutState::new();
    let counter = Arc::clone(&heard);
    state.subscribe(move |s| {
        assert!(s.read_at().is_some());
        counter.fetch_add(1, Ordering::SeqCst);
    });

    state.set(reading(0), None, None, false, now());
    state.set(reading(0), None, None, false, at(1));

    assert_eq!(heard.load(Ordering::SeqCst), 2);
}
