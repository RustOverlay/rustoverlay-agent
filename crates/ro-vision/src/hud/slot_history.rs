use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::{slot_hash, FrameBuffer, InventoryReading, SlotCount, SlotReading, SlotRect};
use crate::items::{IconMatch, ItemDefinition};

/// A count that disagreed with one already confirmed for the same slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CountRejection {
    pub slot: usize,
    pub confirmed: SlotCount,
    pub seen: Option<SlotCount>,
}

/// Withholds a belt slot's identity and count until more than one frame has agreed on them.
///
/// An empty belt slot drawn over the world is a rectangle of landscape, and some icon out of 1249
/// always resembles it. Across frames such inventions are obvious: over 129 captured strips no two
/// adjacent frames ever invented the same item, while a real item was named in nearly every frame
/// it was present. Only the belt with the panel shut is filtered; panel slots are already clean.
/// State, so hold one per capture loop. See docs/item-matching.md.
pub struct SlotHistory {
    /// Agreeing readings needed before an identity is reported.
    pub confirm_after: u32,
    /// Contradicting readings needed before a reported identity is withdrawn.
    pub clear_after: u32,
    /// Gap after which nothing confirmed is trusted until a fresh frame agrees with it. Frames stop
    /// arriving whenever Rust loses focus, and the belt may be anything by the time they resume.
    pub max_gap: Duration,
    slots: HashMap<SlotRect, Tracked>,
    rejections: Vec<CountRejection>,
    last: Option<SystemTime>,
}

impl Default for SlotHistory {
    fn default() -> Self {
        Self {
            confirm_after: 2,
            clear_after: 2,
            max_gap: Duration::from_secs(4),
            slots: HashMap::new(),
            rejections: Vec::new(),
            last: None,
        }
    }
}

#[derive(Default)]
struct Tracked {
    hash: u64,
    seen: bool,
    confirmed: Option<IconMatch>,
    pending: Option<IconMatch>,
    agreements: u32,
    disagreements: u32,
    confirmed_count: Option<SlotCount>,
    pending_count: Option<SlotCount>,
    count_agreements: u32,
    count_disagreements: u32,
}

impl SlotHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts contradicting a confirmed value during the most recent [`Self::confirm`].
    /// Diagnostic only: a stack that genuinely changed contradicts once before it is adopted.
    pub fn last_count_rejections(&self) -> &[CountRejection] {
        &self.rejections
    }

    /// Replaces each belt slot's identity and count with the ones confirmed across frames. Other
    /// containers and the slot rects pass through untouched.
    ///
    /// Counts are filtered on the same rule as identity because they fail the same way: a live
    /// capture read a confirmed `x30` as `x9` for one frame. A count one frame late is honest; a
    /// count that is wrong is not.
    pub fn confirm<F: FrameBuffer + ?Sized>(
        &mut self,
        frame: &F,
        reading: &InventoryReading,
        inventory_open: bool,
        read_at: SystemTime,
    ) -> InventoryReading {
        if let Some(previous) = self.last
            && read_at.duration_since(previous).is_ok_and(|gap| gap > self.max_gap)
        {
            for state in self.slots.values_mut() {
                demote(state, self.confirm_after);
            }
        }
        self.last = Some(read_at);
        self.rejections.clear();

        let mut belt = Vec::with_capacity(reading.belt.len());

        for (i, slot) in reading.belt.iter().enumerate() {
            let hash = slot_hash(frame, slot.slot);
            let mut state = self.slots.remove(&slot.slot).unwrap_or_default();

            // A panel-backed read is trusted and seeds confirmation, so shutting the panel costs no frames.
            if inventory_open {
                seed(&mut state, slot, hash);
                belt.push(slot.clone());
            } else {
                // Identical pixels are one observation, else a stationary player confirms an invention.
                if !state.seen || state.hash != hash {
                    self.advance(&mut state, slot.item.as_ref());
                    self.advance_count(&mut state, slot.count, i);
                }

                (state.hash, state.seen) = (hash, true);
                belt.push(SlotReading { item: state.confirmed.clone(), count: state.confirmed_count, ..slot.clone() });
            }

            self.slots.insert(slot.slot, state);
        }

        self.prune(&reading.belt);

        InventoryReading { belt, ..reading.clone() }
    }

    fn advance(&self, state: &mut Tracked, seen: Option<&IconMatch>) {
        let Some(fresh) = seen else {
            state.pending = None;
            state.agreements = 0;
            if state.confirmed.is_some() {
                state.disagreements += 1;
                if state.disagreements >= self.clear_after {
                    withdraw(state);
                }
            }
            return;
        };

        if let Some(held) = &state.confirmed {
            if let Some(agreed) = intersect(held, fresh) {
                state.confirmed = Some(agreed);
                state.pending = None;
                (state.agreements, state.disagreements) = (0, 0);
                return;
            }

            state.disagreements += 1;
            if state.disagreements >= self.clear_after {
                withdraw(state);
            }
        }

        // Intersection, not equality: which member of a look-alike set leads is not stable.
        let merged = match &state.pending {
            Some(pending) => intersect(pending, fresh),
            None => Some(fresh.clone()),
        };
        (state.pending, state.agreements) = match merged {
            None => (Some(fresh.clone()), 1),
            Some(merged) => (Some(merged), state.agreements + 1),
        };

        if state.confirmed.is_none() && state.agreements >= self.confirm_after {
            state.confirmed = state.pending.take();
            (state.agreements, state.disagreements) = (0, 0);
        }
    }

    /// The count equivalent of `advance`. Simpler, because a count is either equal or it is not —
    /// there is no ambiguity set to intersect.
    fn advance_count(&mut self, state: &mut Tracked, seen: Option<SlotCount>, slot: usize) {
        let Some(fresh) = seen else {
            state.pending_count = None;
            state.count_agreements = 0;
            if let Some(unread) = state.confirmed_count {
                self.rejections.push(CountRejection { slot, confirmed: unread, seen: None });
                state.count_disagreements += 1;
                if state.count_disagreements >= self.clear_after {
                    withdraw_count(state);
                }
            }
            return;
        };

        if let Some(held) = state.confirmed_count {
            if held == fresh {
                state.pending_count = None;
                (state.count_agreements, state.count_disagreements) = (0, 0);
                return;
            }

            self.rejections.push(CountRejection { slot, confirmed: held, seen: Some(fresh) });
            state.count_disagreements += 1;
            if state.count_disagreements >= self.clear_after {
                withdraw_count(state);
            }
        }

        state.count_agreements = if state.pending_count == Some(fresh) { state.count_agreements + 1 } else { 1 };
        state.pending_count = Some(fresh);

        if state.confirmed_count.is_none() && state.count_agreements >= self.confirm_after {
            state.confirmed_count = Some(fresh);
            state.pending_count = None;
            (state.count_agreements, state.count_disagreements) = (0, 0);
        }
    }

    fn prune(&mut self, belt: &[SlotReading]) {
        if self.slots.len() <= belt.len() {
            return;
        }

        let live: HashSet<SlotRect> = belt.iter().map(|s| s.slot).collect();
        self.slots.retain(|slot, _| live.contains(slot));
    }
}

fn seed(state: &mut Tracked, slot: &SlotReading, hash: u64) {
    state.confirmed = slot.item.clone();
    state.pending = None;
    (state.agreements, state.disagreements) = (0, 0);

    state.confirmed_count = slot.count;
    state.pending_count = None;
    (state.count_agreements, state.count_disagreements) = (0, 0);

    (state.hash, state.seen) = (hash, true);
}

/// What a gap does to a slot: its confirmed item and count become pending one agreement short, so a
/// belt still held re-confirms on the first fresh frame instead of going blank for two. A slot that
/// changed meanwhile still needs the usual agreements, and a stale unconfirmed guess is dropped.
fn demote(state: &mut Tracked, confirm_after: u32) {
    let short = confirm_after.saturating_sub(1);

    state.pending = state.confirmed.take();
    state.agreements = if state.pending.is_some() { short } else { 0 };
    state.pending_count = state.confirmed_count.take();
    state.count_agreements = if state.pending_count.is_some() { short } else { 0 };
    (state.disagreements, state.count_disagreements) = (0, 0);
    state.seen = false;
}

fn withdraw_count(state: &mut Tracked) {
    state.confirmed_count = None;
    state.count_disagreements = 0;
}

fn withdraw(state: &mut Tracked) {
    state.confirmed = None;
    state.disagreements = 0;
}

/// The fresher score, but the held ordering — the leader only changes when it drops out.
fn intersect(held: &IconMatch, fresh: &IconMatch) -> Option<IconMatch> {
    let in_fresh: HashSet<&Arc<ItemDefinition>> = members(fresh).collect();
    let mut kept = members(held).filter(|m| in_fresh.contains(m));
    let first = kept.next()?;

    Some(IconMatch { item: Arc::clone(first), score: fresh.score, indistinguishable: kept.cloned().collect() })
}

fn members(m: &IconMatch) -> impl Iterator<Item = &Arc<ItemDefinition>> {
    std::iter::once(&m.item).chain(&m.indistinguishable)
}
