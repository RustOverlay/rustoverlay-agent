use super::wire::{Container, Loadout};
use crate::hud::InventoryGeometry;

/// One square of a teammate's inventory, present whether or not anything is in it.
///
/// An empty `item_id` covers two cases the wire cannot separate: the square was empty, and the
/// matcher could not read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub item_id: Option<i32>,
    pub ambiguous: bool,
    pub count: i32,
    pub is_stack: bool,
    pub held: bool,
}

impl Cell {
    pub const EMPTY: Cell = Cell { item_id: None, ambiguous: false, count: 0, is_stack: false, held: false };

    pub const fn is_empty(&self) -> bool {
        self.item_id.is_none()
    }
}

const BACKPACK_SLOTS: usize = 1;

/// A [`Loadout`] spread back out into the grids it was read from.
///
/// `Slot::index` is positional and empty slots are never sent, so a consumer that draws a grid has
/// to scatter the sparse list back into fixed-length rows. Sizes come from [`InventoryGeometry`] —
/// the constants the screen reader scans against, so the drawn grid cannot drift from the read one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectedLoadout {
    pub belt: Vec<Cell>,
    pub main: Vec<Cell>,
    pub wear: Vec<Cell>,
    pub backpack: Vec<Cell>,
}

impl InspectedLoadout {
    /// All four grids at their full size; no loadout yields empty squares, not none.
    pub fn from_loadout(loadout: Option<&Loadout>) -> Self {
        let held = loadout.and_then(|l| l.held_slot);

        Self {
            belt: spread(loadout.and_then(|l| l.belt.as_ref()), InventoryGeometry::BELT_SLOTS, held),
            main: spread(
                loadout.and_then(|l| l.main.as_ref()),
                InventoryGeometry::MAIN_COLUMNS * InventoryGeometry::MAIN_ROWS,
                None,
            ),
            wear: spread(loadout.and_then(|l| l.wear.as_ref()), InventoryGeometry::WEAR_SLOTS, None),
            backpack: spread(loadout.and_then(|l| l.backpack.as_ref()), BACKPACK_SLOTS, None),
        }
    }
}

fn spread(container: Option<&Container>, size: usize, held_index: Option<u32>) -> Vec<Cell> {
    let mut cells = vec![Cell::EMPTY; size];
    let Some(container) = container else { return cells };

    for slot in &container.slots {
        // A container that grew in a game update must cost a square, not panic.
        let Some(&first) = slot.item_id.first() else { continue };
        if slot.index as usize >= size {
            continue;
        }

        cells[slot.index as usize] = Cell {
            item_id: Some(first),
            ambiguous: slot.item_id.len() > 1,
            count: slot.count,
            is_stack: slot.is_stack,
            held: Some(slot.index) == held_index,
        };
    }

    cells
}
