//! The `rustoverlay.v1` loadout messages, as plain data.
//!
//! `ro-vision` sits below `ro-net`, which owns the protobuf codegen, so the consumer-side readers
//! here (`LoadoutCounts`, `InspectedLoadout`) take this mirror instead. Field names and types match
//! what prost generates from proto/rustoverlay/v1/team.proto, so converting is field-for-field.

/// One occupied square. `item_id` holds several candidates when the matcher could not separate
/// look-alikes; collapsing that to one would present a coin flip as fact.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    pub index: u32,
    pub item_id: Vec<i32>,
    pub count: i32,
    /// False means a magazine count.
    pub is_stack: bool,
}

/// Timestamped per container: the main grid, wear row and backpack can only be read while the
/// panel is open and go stale the moment it shuts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub read_at_ms: i64,
    pub slots: Vec<Slot>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Loadout {
    pub sampled_at_ms: i64,
    pub belt: Option<Container>,
    pub main: Option<Container>,
    pub wear: Option<Container>,
    pub backpack: Option<Container>,
    /// Belt index 0-5. Absent when nothing is held, or when the belt is too uniformly blue to tell.
    pub held_slot: Option<u32>,
}
