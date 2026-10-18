//! Maps what the screen reader saw onto the wire contract.

use std::time::SystemTime;

use ro_vision::hosting::LoadoutState;
use ro_vision::hud::{SlotReading, VitalsReading};

use crate::{proto, unix_millis};

pub fn loadout_to_proto(state: &LoadoutState) -> proto::Loadout {
    let read_at = state.read_at();
    let mut loadout = proto::Loadout {
        sampled_at_ms: millis(read_at),
        held_slot: state.held_slot().map(|held| held as u32),
        ..Default::default()
    };

    if let Some(live) = state.inventory() {
        loadout.belt = Some(to_container(&live.belt, read_at));
    }

    // Only the belt is legible with the panel shut, so these carry their own, older stamp.
    if let Some(panel) = state.last_open_inventory() {
        let panel_read_at = state.hidden_since().or(read_at);
        loadout.main = Some(to_container(&panel.main, panel_read_at));
        loadout.wear = Some(to_container(&panel.wear, panel_read_at));
        if let Some(backpack) = &panel.backpack {
            loadout.backpack = Some(to_container(std::slice::from_ref(backpack), panel_read_at));
        }
    }

    loadout
}

pub fn vitals_to_proto(reading: &VitalsReading, sampled_at: Option<SystemTime>) -> proto::Vitals {
    proto::Vitals {
        sampled_at_ms: millis(sampled_at),
        health: reading.health.unwrap_or(0.0),
        hydration: reading.hydration.unwrap_or(0.0),
        calories: reading.calories.unwrap_or(0.0),
        confidence: reading.confidence,
        ..Default::default()
    }
}

fn to_container(slots: &[SlotReading], read_at: Option<SystemTime>) -> proto::Container {
    proto::Container {
        read_at_ms: millis(read_at),
        slots: slots.iter().enumerate().filter_map(|(index, slot)| to_slot(slot, index)).collect(),
    }
}

/// None for an empty or unreadable slot — those are omitted, never sent as blanks.
fn to_slot(reading: &SlotReading, index: usize) -> Option<proto::Slot> {
    let item = reading.item.as_ref()?;

    // The matcher excludes the winner from its look-alike set, so both halves are needed.
    let item_id = std::iter::once(&item.item)
        .chain(&item.indistinguishable)
        .map(|candidate| candidate.item_id)
        .collect();

    let (count, is_stack) = reading.count.map_or((0, false), |c| (c.value, c.is_stack));
    Some(proto::Slot { index: index as u32, item_id, count, is_stack })
}

fn millis(at: Option<SystemTime>) -> i64 {
    unix_millis(at.unwrap_or_else(SystemTime::now))
}
