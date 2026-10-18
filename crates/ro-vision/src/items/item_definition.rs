use std::path::PathBuf;

/// One entry from `Bundles/items`. Read from the player's own install, never vendored — the icons
/// and definitions are Facepunch's, and reading them locally means the catalogue always matches the
/// installed game version.
///
/// Equality and hashing are by value, as the C# record's were; `SlotHistory` intersects readings
/// through a set of these.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ItemDefinition {
    pub item_id: i32,
    pub short_name: String,
    pub name: String,
    pub category: String,
    pub is_wearable: bool,
    pub is_holdable: bool,
    pub has_condition_bar: bool,
    pub stack_size: i32,
    pub icon_path: Option<PathBuf>,
}

impl ItemDefinition {
    /// Items that can occupy an armour slot. Narrows matching from ~1250 to ~170.
    pub fn is_armour_candidate(&self) -> bool {
        self.is_wearable
    }
}
