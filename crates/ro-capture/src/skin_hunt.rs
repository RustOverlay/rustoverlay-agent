//! Which item's skins to fetch for a belt slot that nothing local can read.
//!
//! An unread slot that is not empty is usually a skin the catalogue has no picture of; fetching
//! every approved skin of its item fixes it, but its item is unknown. The candidates are what the
//! matcher ranks near the top, and on one frame those are noise: an empty slot over busy ground
//! clears the departure gate often, and fetching for every candidate cost 41-180 item types per
//! corpus. What separates a real item from landscape is stability — the same candidates frame
//! after frame — so only an item in the top of every frame of a streak is fetched, and only the
//! best-ranked one at a time. See research/skin-icon-source.md, "Choosing the trigger".

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How far down each frame's ranking a candidate may sit.
pub(crate) const TOP: usize = 5;
/// Changed frames a streak needs, and the time they must span; matching the corpus's ~1.3 s
/// between recorded frames, so a fast capture cadence cannot make landscape look stable.
const FRAMES: u32 = 5;
const SPAN: Duration = Duration::from_secs(5);
/// Longer than this between unread frames and the streak starts over: the slot may be anything.
const GAP: Duration = Duration::from_secs(4);
/// How long a slot that asked waits for its answer before asking for its next candidate: the
/// largest set (345 skins) takes ~90 s at four requests a second. Without it, a slot still
/// unread mid-fetch asked for its second-best item six seconds later.
const AWAIT_ANSWER: Duration = Duration::from_secs(180);

#[derive(Default)]
pub(crate) struct SkinHunt {
    slots: HashMap<usize, Streak>,
    requested: HashSet<String>,
    asked_at: HashMap<usize, Instant>,
}

struct Streak {
    started: Instant,
    last: Instant,
    hash: u64,
    frames: u32,
    /// Items in the top of every frame so far, in the latest frame's order.
    candidates: Vec<String>,
}

impl SkinHunt {
    /// Feeds one frame of one belt slot: `leaders` is None when the slot was read or is empty,
    /// else the skinnable items in its top [`TOP`], best first. Returns the item to fetch, if any.
    pub fn observe(&mut self, slot: usize, hash: u64, leaders: Option<Vec<String>>, now: Instant) -> Option<String> {
        let Some(leaders) = leaders else {
            self.slots.remove(&slot);
            self.asked_at.remove(&slot);
            return None;
        };
        if self.asked_at.get(&slot).is_some_and(|&at| now.saturating_duration_since(at) < AWAIT_ANSWER) {
            return None;
        }

        let streak = self.slots.entry(slot).or_insert_with(|| Streak { started: now, last: now, hash, frames: 0, candidates: leaders.clone() });
        if now.saturating_duration_since(streak.last) > GAP {
            *streak = Streak { started: now, last: now, hash, frames: 0, candidates: leaders.clone() };
        }
        // Identical pixels are one observation: a player standing still is not evidence of stability.
        if streak.frames > 0 && streak.hash == hash {
            return None;
        }

        streak.candidates = leaders.into_iter().filter(|item| streak.candidates.contains(item)).collect();
        (streak.hash, streak.last, streak.frames) = (hash, now, streak.frames + 1);

        if streak.candidates.is_empty() {
            self.slots.remove(&slot);
            return None;
        }
        if streak.frames < FRAMES || now.saturating_duration_since(streak.started) < SPAN {
            return None;
        }

        let chosen = streak.candidates.iter().find(|item| !self.requested.contains(*item)).cloned();
        self.slots.remove(&slot);
        let chosen = chosen?;
        self.requested.insert(chosen.clone());
        self.asked_at.insert(slot, now);
        Some(chosen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaders(items: &[&str]) -> Option<Vec<String>> {
        Some(items.iter().map(|s| s.to_string()).collect())
    }

    /// Feeds `frames` changed frames 1.3 s apart and returns what was chosen on each.
    fn feed(hunt: &mut SkinHunt, frames: &[Option<Vec<String>>]) -> Vec<Option<String>> {
        let start = Instant::now();
        frames
            .iter()
            .enumerate()
            .map(|(i, l)| hunt.observe(0, i as u64, l.clone(), start + Duration::from_millis(1300 * i as u64)))
            .collect()
    }

    #[test]
    fn the_item_every_frame_agrees_on_is_fetched_once_the_streak_is_long_enough() {
        let mut hunt = SkinHunt::default();
        let frames = [
            leaders(&["locker", "door.hinged.metal", "pants"]),
            leaders(&["door.hinged.metal", "boomerang"]),
            leaders(&["sail", "door.hinged.metal"]),
            leaders(&["door.hinged.metal"]),
            leaders(&["door.hinged.metal", "rifle.ak"]),
        ];
        assert_eq!(feed(&mut hunt, &frames), [None, None, None, None, Some("door.hinged.metal".into())]);
    }

    #[test]
    fn landscape_whose_candidates_keep_changing_fetches_nothing() {
        let mut hunt = SkinHunt::default();
        let frames = [leaders(&["sail"]), leaders(&["cannonball"]), leaders(&["sail"]), leaders(&["shovel"]), leaders(&["sail"])];
        assert!(feed(&mut hunt, &frames).iter().all(Option::is_none));
    }

    #[test]
    fn a_read_or_empty_frame_breaks_the_streak() {
        let mut hunt = SkinHunt::default();
        let door = || leaders(&["door.hinged.metal"]);
        let frames = [door(), door(), door(), None, door(), door()];
        assert!(feed(&mut hunt, &frames).iter().all(Option::is_none));
    }

    #[test]
    fn unchanged_pixels_do_not_count_toward_the_streak() {
        let mut hunt = SkinHunt::default();
        let start = Instant::now();
        let chosen: Vec<_> = (0..10).map(|i| hunt.observe(0, 7, leaders(&["door.hinged.metal"]), start + Duration::from_secs(i))).collect();
        assert!(chosen.iter().all(Option::is_none));
    }

    #[test]
    fn a_slot_waits_for_its_answer_before_offering_its_next_candidate() {
        let mut hunt = SkinHunt::default();
        let both = || leaders(&["door.hinged.metal", "rifle.ak"]);
        let start = Instant::now();
        let at = |s: u64| start + Duration::from_millis(s);
        let mut asked = Vec::new();
        for i in 0..20u64 {
            asked.extend(hunt.observe(0, i, both(), at(1300 * i)));
        }
        assert_eq!(asked, ["door.hinged.metal"], "still waiting on the door set");

        for i in 0..5u64 {
            asked.extend(hunt.observe(0, 100 + i, both(), at(200_000 + 1300 * i)));
        }
        assert_eq!(asked, ["door.hinged.metal", "rifle.ak"], "the door set did not help, so the next candidate");
    }

    #[test]
    fn a_slot_that_reads_is_not_kept_waiting() {
        let mut hunt = SkinHunt::default();
        let door = || leaders(&["door.hinged.metal"]);
        let start = Instant::now();
        let first: Vec<_> = (0..5u64).filter_map(|i| hunt.observe(0, i, door(), start + Duration::from_millis(1300 * i))).collect();
        assert_eq!(first, ["door.hinged.metal"]);

        hunt.observe(0, 50, None, start + Duration::from_secs(8));
        let wood = || leaders(&["door.hinged.wood"]);
        let later: Vec<_> = (0..5u64).filter_map(|i| hunt.observe(0, 60 + i, wood(), start + Duration::from_secs(10) + Duration::from_millis(1300 * i))).collect();
        assert_eq!(later, ["door.hinged.wood"]);
    }
}
