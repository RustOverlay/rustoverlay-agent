//! One teammate's whole inventory, drawn while the hold key is down (`InspectViewModel`).
//!
//! The four grids always hold their full size and only their squares' contents change: the main and
//! belt grids lay out by position, so the blanks have to be present for anything to land in the
//! right column. The wear row hides its empties in the view instead.

use ro_net::relay::TeamMemberView;
use ro_vision::hud::InventoryGeometry;
use ro_vision::items::wire::Loadout;
use ro_vision::items::{Cell, InspectedLoadout, ItemCatalog};

use crate::icons::{Icon, ItemIcons};

pub const MAIN_COLUMNS: usize = InventoryGeometry::MAIN_COLUMNS;
pub const BELT_COLUMNS: usize = InventoryGeometry::BELT_SLOTS;

/// One square of the inspected teammate's inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectCell {
    pub icon: Option<Icon>,
    pub count: String,
    /// The matcher offered several look-alikes and this is only the first of them.
    pub ambiguous: bool,
    pub held: bool,
    pub is_empty: bool,
}

impl InspectCell {
    fn of(cell: &Cell, catalog: Option<&ItemCatalog>, icons: &ItemIcons) -> Self {
        let item = cell.item_id.and_then(|id| catalog?.by_id(id));
        Self {
            icon: icons.load(item.and_then(|i| i.icon_path.as_deref())),
            count: quantity(cell),
            ambiguous: cell.ambiguous,
            held: cell.held,
            is_empty: cell.is_empty(),
        }
    }
}

/// Blank for a single item, matching how Rust draws its own squares. A non-stack count is a
/// magazine and is shown whenever the weapon has rounds in it.
fn quantity(cell: &Cell) -> String {
    match cell {
        c if c.is_empty() => String::new(),
        c if c.is_stack && c.count > 1 => c.count.to_string(),
        c if !c.is_stack && c.count > 0 => c.count.to_string(),
        _ => String::new(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct InspectView {
    pub belt: Vec<InspectCell>,
    pub main: Vec<InspectCell>,
    pub wear: Vec<InspectCell>,
    pub backpack: Vec<InspectCell>,
    /// Whose name is not repeated here: the selected row is already marked on the list.
    pub visible: bool,
}

impl Default for InspectView {
    fn default() -> Self {
        Self::of(None, None, &ItemIcons::new())
    }
}

impl InspectView {
    /// A hidden view of `member`'s inventory. Loads icons, so build it outside any lock.
    pub fn of(member: Option<&TeamMemberView>, catalog: Option<&ItemCatalog>, icons: &ItemIcons) -> Self {
        let loadout: Option<Loadout> = member.and_then(|m| m.loadout.clone()).map(Into::into);
        let spread = InspectedLoadout::from_loadout(loadout.as_ref());
        let fill = |cells: &[Cell]| -> Vec<InspectCell> { cells.iter().map(|c| InspectCell::of(c, catalog, icons)).collect() };

        Self {
            belt: fill(&spread.belt),
            main: fill(&spread.main),
            wear: fill(&spread.wear),
            backpack: fill(&spread.backpack),
            visible: false,
        }
    }

    /// Hides the pack square and its label together, rather than leaving a lone blank.
    pub fn has_backpack(&self) -> bool {
        self.backpack.first().is_some_and(|c| !c.is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(count: i32, is_stack: bool) -> Cell {
        Cell { item_id: Some(1), ambiguous: false, count, is_stack, held: false }
    }

    #[test]
    fn counts_follow_how_rust_draws_its_squares() {
        assert_eq!(quantity(&Cell::EMPTY), "");
        assert_eq!(quantity(&cell(1, true)), "");
        assert_eq!(quantity(&cell(64, true)), "64");
        assert_eq!(quantity(&cell(0, false)), "");
        assert_eq!(quantity(&cell(30, false)), "30");
    }

    #[test]
    fn an_empty_inspector_still_has_every_square() {
        let view = InspectView::default();

        assert_eq!(view.belt.len(), InventoryGeometry::BELT_SLOTS);
        assert_eq!(view.main.len(), InventoryGeometry::MAIN_COLUMNS * InventoryGeometry::MAIN_ROWS);
        assert_eq!(view.wear.len(), InventoryGeometry::WEAR_SLOTS);
        assert!(!view.has_backpack());
        assert!(view.main.iter().all(|c| c.is_empty));
    }
}
