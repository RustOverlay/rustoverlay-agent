use std::path::PathBuf;
use std::sync::Arc;

use super::ItemDefinition;

/// One picture an item can present in a slot: the item itself, plus the icon actually drawn for it.
///
/// A vanilla item has exactly one face; a skinned one has as many as the player has skins cached
/// for it. See docs/item-matching.md.
///
/// Gotcha: this exists rather than a second `ItemDefinition` carrying the skin's path because
/// definitions compare by value, and `SlotHistory` intersects readings through a set of them. Two
/// frames disagreeing only about which picture an AK wore would intersect to nothing and withdraw
/// a correct read. Faces stay inside the matcher; `IconMatch` only ever names the definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconFace {
    pub item: Arc<ItemDefinition>,
    pub icon_path: Option<PathBuf>,
}

impl IconFace {
    pub fn new(item: Arc<ItemDefinition>, icon_path: Option<PathBuf>) -> Self {
        Self { item, icon_path }
    }

    /// Whether this is a skin's picture rather than the item's own icon.
    pub fn is_skin(&self) -> bool {
        self.icon_path != self.item.icon_path
    }
}

/// Every item is trivially a face of itself, which keeps an unskinned catalogue written as one.
impl From<Arc<ItemDefinition>> for IconFace {
    fn from(item: Arc<ItemDefinition>) -> Self {
        let icon_path = item.icon_path.clone();
        Self { item, icon_path }
    }
}
