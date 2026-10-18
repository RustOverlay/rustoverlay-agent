use std::collections::HashMap;
use std::sync::Arc;

use rayon::prelude::*;

use super::{inventory_panel, slot_hash, CountReader, FrameBuffer, InventoryGeometry, SlotCount, SlotRect};
use crate::items::{FrameSlotSampler, IconMatch, IconMatcher, ItemDefinition};

/// What one slot holds. Both are None for an empty slot, and `item` is None on its own whenever
/// the matcher would rather abstain than guess.
#[derive(Clone, Debug, PartialEq)]
pub struct SlotReading {
    pub slot: SlotRect,
    pub item: Option<IconMatch>,
    pub count: Option<SlotCount>,
}

/// One frame's containers. `main`, `wear` and `backpack` are empty unless the inventory was open —
/// those slots are simply not drawn otherwise.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventoryReading {
    pub belt: Vec<SlotReading>,
    pub main: Vec<SlotReading>,
    pub wear: Vec<SlotReading>,
    pub backpack: Option<SlotReading>,
}

/// One slot to visit, and how.
#[derive(Clone, Copy)]
struct Visit {
    slot: SlotRect,
    wear_only: bool,
    has_count: bool,
    over_world: bool,
}

/// Reads every visible slot of a frame: what it holds, and how many.
///
/// Stateful, deliberately: hold one per capture loop rather than constructing it per frame, or the
/// unchanged-slot skip never fires.
pub struct InventoryReader {
    matcher: Arc<IconMatcher>,
    counts: Arc<CountReader>,
    previous: HashMap<SlotRect, (u64, SlotReading)>,
}

impl InventoryReader {
    pub fn new(matcher: Arc<IconMatcher>, counts: Arc<CountReader>) -> Self {
        Self { matcher, counts, previous: HashMap::new() }
    }

    pub fn matcher(&self) -> &Arc<IconMatcher> {
        &self.matcher
    }

    /// Reads the frame, detecting for itself whether the inventory panel is drawn. Callers holding
    /// a real frame should use this one — guessing the panel wrong invents items.
    pub fn read<F: FrameBuffer + Sync + ?Sized>(&mut self, frame: &F, geometry: &InventoryGeometry) -> InventoryReading {
        let open = inventory_panel::is_open(frame, geometry);
        self.read_with_panel(frame, geometry, open)
    }

    /// Reads the frame with the panel state given, reusing the previous answer for any slot whose
    /// pixels have not changed. For tests and calibration.
    ///
    /// Slots are independent, so they are matched in parallel; and an inventory is static across
    /// most frames, so in the steady state almost every slot is skipped. The cache is keyed on
    /// [`SlotRect`], so a resolution or UI-scale change moves every slot and invalidates itself.
    pub fn read_with_panel<F: FrameBuffer + Sync + ?Sized>(
        &mut self,
        frame: &F,
        geometry: &InventoryGeometry,
        inventory_open: bool,
    ) -> InventoryReading {
        // A minimised window captures 0x0.
        if frame.width() <= 0 || frame.height() <= 0 {
            return InventoryReading::default();
        }

        let belt = geometry.belt();
        let main: &[SlotRect] = if inventory_open { geometry.main() } else { &[] };
        let wear: &[SlotRect] = if inventory_open { geometry.wear() } else { &[] };

        let mut visits = Vec::with_capacity(belt.len() + main.len() + wear.len() + 1);

        // Panel shut: an empty belt slot is landscape the matcher will name — see minimum_departure.
        let visit = |slot, wear_only, has_count, over_world| Visit { slot, wear_only, has_count, over_world };
        visits.extend(belt.iter().map(|&s| visit(s, false, true, !inventory_open)));
        visits.extend(main.iter().map(|&s| visit(s, false, true, false)));
        visits.extend(wear.iter().map(|&s| visit(s, true, true, false)));

        // The backpack shows how full it is, `0/28`, not a stack count, so CountReader does not apply.
        if inventory_open {
            visits.push(visit(geometry.backpack(), true, false, false));
        }

        let previous = &self.previous;
        let (matcher, counts) = (&*self.matcher, &*self.counts);

        let read: Vec<(u64, SlotReading)> = visits
            .par_iter()
            .map(|v| {
                let hash = slot_hash(frame, v.slot);
                if let Some((cached_hash, reading)) = previous.get(&v.slot)
                    && *cached_hash == hash
                {
                    return (hash, reading.clone());
                }

                let wearable = |item: &ItemDefinition| item.is_wearable;
                let filter: Option<&dyn Fn(&ItemDefinition) -> bool> = if v.wear_only { Some(&wearable) } else { None };
                // Mid-resize or a large UI scale can push a slot off the frame; the matcher reads it unclamped.
                let on_frame = v.slot.icon_area().lies_within(frame.width(), frame.height());
                let item = if on_frame { matcher.identify_sampler(&FrameSlotSampler::new(frame, v.slot), filter, v.over_world) } else { None };
                let count = if v.has_count { counts.read(frame, v.slot) } else { None };

                (hash, SlotReading { slot: v.slot, item, count })
            })
            .collect();

        // Rebuilt rather than merged: that is what expires entries whose geometry no longer exists.
        self.previous = visits.iter().zip(&read).map(|(v, entry)| (v.slot, entry.clone())).collect();

        let mut readings = read.into_iter().map(|(_, reading)| reading);
        let belt = readings.by_ref().take(belt.len()).collect();
        let main = readings.by_ref().take(main.len()).collect();
        let wear = readings.by_ref().take(wear.len()).collect();

        InventoryReading { belt, main, wear, backpack: if inventory_open { readings.next() } else { None } }
    }
}
