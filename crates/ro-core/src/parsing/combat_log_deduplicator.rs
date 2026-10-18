use chrono::TimeDelta;

use crate::events::CombatEvent;
use crate::invariant::double_equals;

/// Suppresses rows seen in an earlier dump. The combatlog is a ring buffer, so consecutive dumps
/// overlap heavily — the same hit reappears under a larger relative offset.
///
/// Matching cannot key on exact time: relative offsets are logged to 10ms and resolve against
/// different header timestamps, so one real event lands up to a few tens of ms apart between
/// dumps. See docs/log-format.md.
#[derive(Debug)]
pub struct CombatLogDeduplicator {
    tolerance: TimeDelta,
    retention: TimeDelta,
    seen: Vec<CombatEvent>,
}

impl Default for CombatLogDeduplicator {
    fn default() -> Self {
        Self::new(None, None)
    }
}

impl CombatLogDeduplicator {
    pub fn new(tolerance: Option<TimeDelta>, retention: Option<TimeDelta>) -> Self {
        Self {
            tolerance: tolerance.unwrap_or(TimeDelta::seconds(1)),
            retention: retention.unwrap_or(TimeDelta::hours(1)),
            seen: Vec::new(),
        }
    }

    pub fn filter(&mut self, events: impl IntoIterator<Item = CombatEvent>) -> Vec<CombatEvent> {
        let mut fresh = Vec::new();

        for event in events {
            if self.is_duplicate(&event) {
                continue;
            }
            self.seen.push(event.clone());
            fresh.push(event);
        }

        self.prune();
        fresh
    }

    fn is_duplicate(&self, candidate: &CombatEvent) -> bool {
        self.seen.iter().any(|e| {
            e.attacker == candidate.attacker
                && e.target == candidate.target
                && double_equals(e.old_hp, candidate.old_hp)
                && double_equals(e.new_hp, candidate.new_hp)
                && double_equals(e.distance_metres, candidate.distance_metres)
                && (e.timestamp - candidate.timestamp).abs() <= self.tolerance
        })
    }

    fn prune(&mut self) {
        // Cut off from the last row added, not the latest timestamp — as the C# did.
        let Some(last) = self.seen.last() else { return };

        let cutoff = last.timestamp - self.retention;
        self.seen.retain(|e| e.timestamp >= cutoff);
    }
}
