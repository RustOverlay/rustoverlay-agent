use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::wire::{Container, Loadout, Slot};
use super::{ItemCatalog, ItemDefinition, WeaponAmmo};
use crate::text::eq_ignore_case;

/// A derived count, and whether a stale container contributed to it.
///
/// `stale_as_of` is the panel-backed container's own timestamp, not the loadout's. It is set only
/// when that container actually contributed, so a belt-only total never reads as stale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountResult {
    pub value: i32,
    pub stale: bool,
    pub stale_as_of: Option<SystemTime>,
}

/// Reads teammate-facing totals out of a [`Loadout`] — meds, walls, and ammunition.
///
/// Consumer-side on purpose: everything here is arithmetic over slot data already on the wire, so
/// adding a count needs no field number and no contracts release, and it works against teammates
/// running an older agent.
pub struct LoadoutCounts {
    catalog: Option<Arc<ItemCatalog>>,
    stale_after: Duration,
}

impl LoadoutCounts {
    pub const MED_SHORT_NAME: &str = "syringe.medical";

    pub const WALL_SHORT_NAME: &str = "barricade.wood.cover";

    pub fn new(catalog: Option<Arc<ItemCatalog>>, stale_after: Duration) -> Self {
        Self { catalog, stale_after }
    }

    /// The counted items themselves, for a consumer that wants to draw their icons.
    pub fn med_item(&self) -> Option<&Arc<ItemDefinition>> {
        self.catalog.as_ref()?.by_short_name(Self::MED_SHORT_NAME)
    }

    pub fn wall_item(&self) -> Option<&Arc<ItemDefinition>> {
        self.catalog.as_ref()?.by_short_name(Self::WALL_SHORT_NAME)
    }

    /// Medical syringes across the belt and the main grid.
    pub fn meds(&self, loadout: Option<&Loadout>, now: SystemTime) -> Option<CountResult> {
        self.total(loadout, now, |item| eq_ignore_case(&item.short_name, Self::MED_SHORT_NAME))
    }

    /// Wooden barricade covers across the belt and the main grid.
    pub fn walls(&self, loadout: Option<&Loadout>, now: SystemTime) -> Option<CountResult> {
        self.total(loadout, now, |item| eq_ignore_case(&item.short_name, Self::WALL_SHORT_NAME))
    }

    /// Spare rounds for the held weapon. None when nothing is held, when the weapon is not in
    /// [`WeaponAmmo`], or when there is no catalogue to resolve ids against.
    pub fn spare_ammo(&self, loadout: Option<&Loadout>, now: SystemTime) -> Option<CountResult> {
        let weapon = self.held_item(loadout)?;
        let ammunition = WeaponAmmo::for_weapon(&weapon.short_name)?;

        self.total(loadout, now, |item| ammunition.contains(&item.short_name))
    }

    /// A held firearm this table cannot match ammunition to, for logging drift after a game
    /// update. None when the weapon is mapped, when nothing is held, or when it is not a firearm.
    pub fn unmapped_weapon(&self, loadout: Option<&Loadout>) -> Option<String> {
        self.held_item(loadout)
            .filter(|weapon| weapon.is_holdable && WeaponAmmo::for_weapon(&weapon.short_name).is_none())
            .map(|weapon| weapon.short_name.clone())
    }

    /// What the teammate is holding, or None when the belt did not say.
    pub fn held_item(&self, loadout: Option<&Loadout>) -> Option<&Arc<ItemDefinition>> {
        let catalog = self.catalog.as_ref()?;
        let slot = held_slot(loadout)?;

        // The candidate set is only useful here if it names one item; two guns is not a held weapon.
        match slot.item_id.as_slice() {
            [only] => catalog.by_id(*only),
            _ => None,
        }
    }

    /// Rounds loaded in the held weapon. Rust writes a magazine as a bare number and a stack as
    /// `x104`, which is the only thing separating them — see `CountReader`.
    pub fn magazine_rounds(loadout: Option<&Loadout>) -> Option<i32> {
        held_slot(loadout).filter(|slot| !slot.is_stack && slot.count > 0).map(|slot| slot.count)
    }

    fn total(&self, loadout: Option<&Loadout>, now: SystemTime, matches: impl Fn(&ItemDefinition) -> bool) -> Option<CountResult> {
        let catalog = self.catalog.as_deref()?;
        let loadout = loadout?;

        let (belt, main) = (loadout.belt.as_ref(), loadout.main.as_ref());
        if belt.is_none() && main.is_none() {
            return None;
        }

        let from_main = sum(catalog, main, &matches);
        let total = sum(catalog, belt, &matches).saturating_add(from_main);

        // Wire data: a timestamp SystemTime cannot hold is no timestamp, not a panic.
        let read_at = main
            .filter(|m| m.read_at_ms > 0)
            .and_then(|m| UNIX_EPOCH.checked_add(Duration::from_millis(m.read_at_ms as u64)));

        // Only a contributing stale container makes the total stale; an empty main grid says nothing.
        let stale = from_main > 0
            && read_at.is_some_and(|at| now.duration_since(at).is_ok_and(|age| age > self.stale_after));

        Some(CountResult { value: total, stale, stale_as_of: if stale { read_at } else { None } })
    }
}

fn held_slot(loadout: Option<&Loadout>) -> Option<&Slot> {
    let loadout = loadout?;
    let held = loadout.held_slot?;

    loadout.belt.as_ref()?.slots.iter().find(|s| s.index == held)
}

fn sum(catalog: &ItemCatalog, container: Option<&Container>, matches: &impl Fn(&ItemDefinition) -> bool) -> i32 {
    container.map_or(0, |c| c.slots.iter().filter(|s| unanimous(catalog, s, matches)).map(quantity).fold(0, i32::saturating_add))
}

/// Whether every candidate id agrees, which is the only safe way to classify an ambiguous slot.
///
/// The matcher is explicit that picking the best-scoring candidate is a coin flip. Agreement makes
/// the ambiguity irrelevant — two syringe variants are both meds — while disagreement, as in
/// `ammo.rifle` or `ammo.snowballgun`, has to abstain. An id the catalogue does not know cannot
/// agree, so it abstains too.
fn unanimous(catalog: &ItemCatalog, slot: &Slot, matches: &impl Fn(&ItemDefinition) -> bool) -> bool {
    !slot.item_id.is_empty() && slot.item_id.iter().all(|&id| catalog.by_id(id).is_some_and(|item| matches(item)))
}

/// How many items a slot holds. A single item is drawn with no number at all, so a zero count
/// means one, not none. A non-stack count is a magazine and never a quantity.
fn quantity(slot: &Slot) -> i32 {
    if slot.is_stack && slot.count > 0 { slot.count } else { 1 }
}
