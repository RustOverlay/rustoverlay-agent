//! One teammate on the Team tab, formatted for display (`TeamMemberViewModel`).
//!
//! Presence and vitals only — the containers themselves are the overlay's job, where they are drawn
//! as squares rather than listed as text and can be read without leaving the game.

use ro_net::proto;
use ro_net::relay::TeamMemberView;
use ro_vision::items::ItemCatalog;

use crate::format::fixed;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamMemberCard {
    pub steam_id: u64,
    pub display_name: String,
    pub online: bool,
    pub presence: &'static str,
    pub vitals: String,
    pub held: String,
}

impl TeamMemberCard {
    pub fn of(member: &TeamMemberView, catalog: Option<&ItemCatalog>) -> Self {
        Self {
            steam_id: member.steam_id,
            display_name: display_name(member),
            online: member.online,
            presence: if member.online { "online" } else { "offline" },
            vitals: member.vitals.as_ref().map_or_else(
                || "Vitals unread".into(),
                |v| {
                    format!("Health {} · Hunger {} · Thirst {}", fixed(v.health, 0), fixed(v.calories, 0), fixed(v.hydration, 0))
                },
            ),
            held: held_slot(member.loadout.as_ref())
                .map_or_else(|| "Nothing held".into(), |slot| format!("Holding {}", describe(slot, catalog))),
        }
    }
}

/// The SteamID stands in until the periodic snapshot names the member.
pub fn display_name(member: &TeamMemberView) -> String {
    if member.display_name.is_empty() { member.steam_id.to_string() } else { member.display_name.clone() }
}

fn held_slot(loadout: Option<&proto::Loadout>) -> Option<&proto::Slot> {
    let loadout = loadout?;
    let held = loadout.held_slot?;
    loadout.belt.as_ref()?.slots.iter().find(|s| s.index == held)
}

/// A slot's every candidate, because that is what was actually established. The matcher sends
/// several ids when the icon cannot separate look-alikes; picking one here would be a coin flip.
fn describe(slot: &proto::Slot, catalog: Option<&ItemCatalog>) -> String {
    let name = |id: i32| catalog.and_then(|c| c.by_id(id)).map_or_else(|| id.to_string(), |i| i.short_name.clone());
    let names = slot.item_id.iter().map(|&id| name(id)).collect::<Vec<_>>().join(" | ");

    let count = match (slot.count, slot.is_stack) {
        (0, _) => String::new(),
        (n, true) => format!(" x{n}"),
        (n, false) => format!(" [{n}]"),
    };
    format!("{names}{count}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(loadout: Option<proto::Loadout>) -> TeamMemberView {
        TeamMemberView { steam_id: 76561, display_name: String::new(), online: true, vitals: None, loadout }
    }

    fn belt(slots: Vec<proto::Slot>, held: Option<u32>) -> proto::Loadout {
        proto::Loadout {
            belt: Some(proto::Container { read_at_ms: 1, slots }),
            held_slot: held,
            ..Default::default()
        }
    }

    #[test]
    fn a_nameless_member_shows_the_steam_id_and_unread_vitals() {
        let card = TeamMemberCard::of(&member(None), None);

        assert_eq!(card.display_name, "76561");
        assert_eq!(card.presence, "online");
        assert_eq!(card.vitals, "Vitals unread");
        assert_eq!(card.held, "Nothing held");
    }

    #[test]
    fn the_held_slot_lists_every_candidate_with_its_count() {
        let slot = proto::Slot { index: 2, item_id: vec![10, 11], count: 30, is_stack: false };
        let card = TeamMemberCard::of(&member(Some(belt(vec![slot], Some(2)))), None);

        assert_eq!(card.held, "Holding 10 | 11 [30]");
    }

    #[test]
    fn vitals_round_to_whole_numbers() {
        let mut m = member(None);
        m.vitals = Some(proto::Vitals { health: 54.5, calories: 200.4, hydration: 99.9, ..Default::default() });

        assert_eq!(TeamMemberCard::of(&m, None).vitals, "Health 55 · Hunger 200 · Thirst 100");
    }

    #[test]
    fn a_stack_reads_as_a_quantity_and_an_absent_held_slot_as_nothing() {
        let slot = proto::Slot { index: 0, item_id: vec![5], count: 3, is_stack: true };
        assert_eq!(TeamMemberCard::of(&member(Some(belt(vec![slot.clone()], Some(0)))), None).held, "Holding 5 x3");
        assert_eq!(TeamMemberCard::of(&member(Some(belt(vec![slot], Some(4)))), None).held, "Nothing held");
    }
}
