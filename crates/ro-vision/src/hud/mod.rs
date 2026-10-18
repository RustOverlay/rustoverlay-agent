//! Port of `Core/Hud`: where the HUD sits, and the readers that read it.

mod count_reader;
mod frame;
mod glyphs;
mod held_item_reader;
mod hud_geometry;
mod inventory_geometry;
mod inventory_panel;
mod inventory_reader;
mod slot_hash;
mod slot_history;
mod vitals_reader;

pub use count_reader::{CountReader, SlotCount};
pub use frame::{ArrayFrameBuffer, FrameBuffer, Rgb};
pub use glyphs::{GlyphAtlas, GlyphSet, GlyphTemplate};
pub use held_item_reader::selected_slot;
pub use hud_geometry::{HudGeometry, VitalsRow};
pub use inventory_geometry::{InventoryGeometry, SlotRect};
pub use inventory_panel::is_open as inventory_is_open;
pub use inventory_reader::{InventoryReader, InventoryReading, SlotReading};
pub use slot_hash::of as slot_hash;
pub use slot_history::{CountRejection, SlotHistory};
pub use vitals_reader::{BarMeasurement, VitalsReader, VitalsReading};

/// Rec. 601 luma, as every reader computes it.
#[inline]
pub(crate) fn luminance(p: Rgb) -> f64 {
    f64::from(p.r) * 0.299 + f64::from(p.g) * 0.587 + f64::from(p.b) * 0.114
}
