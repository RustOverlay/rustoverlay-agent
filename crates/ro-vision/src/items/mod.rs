//! Port of `Core/Items`: the item catalogue and the icon matcher that resolves a slot against it.

mod builtin_skins;
mod frame_slot_sampler;
mod icon_face;
mod icon_match;
mod icon_matcher;
mod icon_signature;
mod inspected_loadout;
mod item_catalog;
mod item_definition;
mod loadout_counts;
mod moments;
mod skin_catalog;
mod skin_coverage;
mod skin_item_types;
mod slot_background;
mod slot_crop;
mod slot_sampler;
mod weapon_ammo;
pub mod wire;

pub use builtin_skins::builtin_skin_faces;
pub use frame_slot_sampler::FrameSlotSampler;
pub use icon_face::IconFace;
pub use icon_match::IconMatch;
pub use icon_matcher::{Candidates, Catalogue, IconMatcher};
pub use icon_signature::IconSignature;
pub use inspected_loadout::{Cell, InspectedLoadout};
pub use item_catalog::ItemCatalog;
pub use item_definition::ItemDefinition;
pub use loadout_counts::{CountResult, LoadoutCounts};
pub use skin_catalog::{is_default_skin_category, SkinCatalog, DEFAULT_CATEGORIES as DEFAULT_SKIN_CATEGORIES};
pub use skin_coverage::{retain_covering_skins, without_partial_skins, MINIMUM_COVERAGE as MINIMUM_SKIN_COVERAGE};
pub use skin_item_types::short_name_for_label as skin_short_name_for_label;
pub use slot_background::SlotBackground;
pub use slot_sampler::{FixedSlotSampler, SlotSampler};
pub use weapon_ammo::{AmmoSet, WeaponAmmo};
