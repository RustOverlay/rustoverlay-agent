//! What the agent currently sees on screen, formatted for the Debug tab (`LoadoutViewModel`).

use std::time::SystemTime;

use ro_vision::hosting::LoadoutState;
use ro_vision::hud::SlotReading;

use crate::format::fixed;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadoutView {
    pub summary: String,
    /// Says outright that the panel is shut, since the slots below it are no longer live.
    pub main_state: String,
    pub held: String,
    pub vitals: String,
    pub belt: Vec<String>,
    pub main: Vec<String>,
    pub wear: Vec<String>,
}

impl LoadoutView {
    /// The belt is drawn in world view and the rest is not, so the two are on different clocks: the
    /// belt from the latest frame, the containers from the last frame that showed them.
    pub fn of(state: &LoadoutState, now: SystemTime) -> Self {
        let seconds_since = |at: SystemTime| now.duration_since(at).map_or(0.0, |d| d.as_secs_f64());

        let summary = match state.read_at() {
            None => "Waiting for Rust — capture runs only while the game window is focused".into(),
            Some(at) => format!(
                "Read {}s ago — {}",
                fixed(seconds_since(at), 1),
                if state.inventory_open() { "inventory open" } else { "world view, belt only" }
            ),
        };

        let panel = state.last_open_inventory();

        Self {
            summary,
            main_state: state
                .hidden_since()
                .map(|hidden| format!("Inventory shut — as last seen {}s ago", fixed(seconds_since(hidden), 0)))
                .unwrap_or_default(),
            held: state.held_slot().map_or_else(|| "Nothing held".into(), |slot| format!("Holding slot {}", slot + 1)),
            vitals: state.vitals().map_or_else(
                || "Vitals unreadable".into(),
                |v| format!("Health {} · Hunger {} · Thirst {}", bar(v.health), bar(v.calories), bar(v.hydration)),
            ),
            belt: state.inventory().map(|r| describe_all(&r.belt)).unwrap_or_default(),
            main: panel.map(|r| describe_all(&r.main)).unwrap_or_default(),
            wear: panel.map(|r| describe_all(&r.wear)).unwrap_or_default(),
        }
    }
}

/// Each bar reads independently, so one being unreadable must not blank the others.
fn bar(value: Option<f64>) -> String {
    value.map_or_else(|| "?".into(), |v| fixed(v, 0))
}

fn describe_all(slots: &[SlotReading]) -> Vec<String> {
    slots.iter().enumerate().map(|(i, slot)| describe(i, slot)).collect()
}

/// A look-alike set is shown as all its members, because that is what was actually established —
/// collapsing it to the best-scoring one would present a coin flip as a fact. An unread slot and
/// an empty one both show a dash: the matcher abstains rather than distinguishing them.
fn describe(index: usize, slot: &SlotReading) -> String {
    let name = slot.item.as_ref().map_or_else(
        || "—".to_owned(),
        |m| std::iter::once(&m.item).chain(&m.indistinguishable).map(|i| i.short_name.as_str()).collect::<Vec<_>>().join(" | "),
    );

    let count = slot
        .count
        .map(|c| if c.is_stack { format!("  x{}", c.value) } else { format!("  [{}]", c.value) })
        .unwrap_or_default();

    format!("{}. {name}{count}", index + 1)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use ro_vision::hud::{InventoryReading, SlotCount, SlotRect, VitalsReading};
    use ro_vision::items::{IconMatch, ItemDefinition};

    use super::*;

    fn item(short_name: &str) -> Arc<ItemDefinition> {
        Arc::new(ItemDefinition {
            item_id: 1,
            short_name: short_name.into(),
            name: short_name.into(),
            category: String::new(),
            is_wearable: false,
            is_holdable: false,
            has_condition_bar: false,
            stack_size: 1,
            icon_path: None,
        })
    }

    fn slot(item: Option<IconMatch>, count: Option<SlotCount>) -> SlotReading {
        SlotReading { slot: SlotRect { x: 0, y: 0, size: 10 }, item, count }
    }

    #[test]
    fn nothing_read_yet_says_so() {
        let view = LoadoutView::of(&LoadoutState::new(), SystemTime::now());

        assert!(view.summary.starts_with("Waiting for Rust"));
        assert_eq!(view.held, "Nothing held");
        assert_eq!(view.vitals, "Vitals unreadable");
        assert!(view.belt.is_empty());
    }

    #[test]
    fn slots_list_every_look_alike_and_their_counts() {
        let mut ambiguous = IconMatch::new(item("ammo.rifle"), 0.9);
        ambiguous.indistinguishable.push(item("ammo.rifle.hv"));
        let reading = InventoryReading {
            belt: vec![
                slot(Some(ambiguous), Some(SlotCount::new(104, true))),
                slot(Some(IconMatch::new(item("rifle.ak"), 0.9)), Some(SlotCount::new(30, false))),
                slot(None, None),
            ],
            ..Default::default()
        };
        let at = SystemTime::now();
        let mut state = LoadoutState::new();
        let vitals = VitalsReading { health: Some(87.5), hydration: None, calories: Some(140.2), scale: 1.0, confidence: 1.0 };
        state.set(reading, Some(1), Some(vitals), false, at);

        let view = LoadoutView::of(&state, at + Duration::from_millis(1250));

        assert_eq!(view.belt, ["1. ammo.rifle | ammo.rifle.hv  x104", "2. rifle.ak  [30]", "3. —"]);
        assert_eq!(view.summary, "Read 1.3s ago — world view, belt only");
        assert_eq!(view.held, "Holding slot 2");
        assert_eq!(view.vitals, "Health 88 · Hunger 140 · Thirst ?");
        assert_eq!(view.main_state, "");
    }

    #[test]
    fn a_shut_panel_keeps_its_last_containers_and_says_how_old_they_are() {
        let at = SystemTime::now();
        let open = InventoryReading { main: vec![slot(None, None)], ..Default::default() };
        let mut state = LoadoutState::new();
        state.set(open, None, None, true, at);
        state.set(InventoryReading::default(), None, None, false, at + Duration::from_secs(1));

        let view = LoadoutView::of(&state, at + Duration::from_secs(4));

        assert_eq!(view.main, ["1. —"]);
        assert_eq!(view.main_state, "Inventory shut — as last seen 3s ago");
    }
}
