//! One teammate's row on the overlay panel (`OverlayMemberViewModel`).

use std::time::SystemTime;

use ro_net::relay::TeamMemberView;
use ro_vision::items::LoadoutCounts;
use ro_vision::items::wire::Loadout;

use super::team_member::display_name;
use crate::format::fixed;
use crate::icons::{Icon, ItemIcons};

/// Banded rather than continuous, so the bar's colour reads as one of three states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthState {
    #[default]
    Full,
    Hurt,
    Critical,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverlayMemberRow {
    pub steam_id: u64,
    pub display_name: String,
    /// Health as 0-1, for a bar width. Rust's scale is 0-100.
    pub health_fraction: f64,
    pub has_health: bool,
    pub health: String,
    pub health_state: HealthState,
    /// The weapon's name, shown only when its icon could not be loaded.
    pub held: String,
    pub weapon_icon: Option<Icon>,
    /// Rounds loaded, then spare for that weapon when both are known.
    pub ammo: String,
    pub med_icon: Option<Icon>,
    pub meds: String,
    /// The count leans on a main grid that has not been seen since the panel shut.
    pub meds_stale: bool,
    pub wall_icon: Option<Icon>,
    pub walls: String,
    pub walls_stale: bool,
    /// Whose inventory the inspector is showing. Marked even while it is closed.
    pub is_selected: bool,
}

impl OverlayMemberRow {
    pub fn of(member: &TeamMemberView, counts: &LoadoutCounts, icons: &ItemIcons, now: SystemTime) -> Self {
        let loadout: Option<Loadout> = member.loadout.clone().map(Into::into);
        let loadout = loadout.as_ref();

        // A null reading is written to the wire as zero, so an unread bar and a corpse look alike.
        // Rendering an empty bar is honest about both; inventing a number for either is not.
        let has_health = member.vitals.as_ref().is_some_and(|v| v.sampled_at_ms > 0);
        let health_fraction = member.vitals.as_ref().map_or(0.0, |v| (v.health / 100.0).clamp(0.0, 1.0));

        let weapon = counts.held_item(loadout);
        let weapon_icon = icons.load(weapon.and_then(|w| w.icon_path.as_deref()));
        // The icon says which gun; the name is only needed when there is no icon to say it.
        let held = if weapon_icon.is_some() { String::new() } else { weapon.map(|w| w.name.clone()).unwrap_or_default() };

        let meds = counts.meds(loadout, now);
        let walls = counts.walls(loadout, now);

        Self {
            steam_id: member.steam_id,
            display_name: display_name(member),
            health_fraction,
            has_health,
            health: match (&member.vitals, has_health) {
                (Some(v), true) => fixed(v.health, 0),
                _ => String::new(),
            },
            health_state: band(health_fraction),
            held,
            weapon_icon,
            ammo: rounds(loadout, counts, now),
            med_icon: icons.load(counts.med_item().and_then(|i| i.icon_path.as_deref())),
            meds: meds.map(|m| m.value.to_string()).unwrap_or_default(),
            meds_stale: meds.is_some_and(|m| m.stale),
            wall_icon: icons.load(counts.wall_item().and_then(|i| i.icon_path.as_deref())),
            walls: walls.map(|w| w.value.to_string()).unwrap_or_default(),
            walls_stale: walls.is_some_and(|w| w.stale),
            is_selected: false,
        }
    }
}

fn band(fraction: f64) -> HealthState {
    if fraction < 0.25 {
        HealthState::Critical
    } else if fraction < 0.6 {
        HealthState::Hurt
    } else {
        HealthState::Full
    }
}

fn rounds(loadout: Option<&Loadout>, counts: &LoadoutCounts, now: SystemTime) -> String {
    match (LoadoutCounts::magazine_rounds(loadout), counts.spare_ammo(loadout, now)) {
        (Some(loaded), Some(carried)) => format!("{loaded} / {}", carried.value),
        (Some(loaded), None) => loaded.to_string(),
        (None, Some(carried)) => format!("— / {}", carried.value),
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ro_net::proto;

    use super::*;

    fn member(vitals: Option<proto::Vitals>, loadout: Option<proto::Loadout>) -> TeamMemberView {
        TeamMemberView { steam_id: 9, display_name: "Ana".into(), online: true, vitals, loadout }
    }

    fn row(m: &TeamMemberView) -> OverlayMemberRow {
        OverlayMemberRow::of(m, &LoadoutCounts::new(None, Duration::from_secs(5)), &ItemIcons::new(), SystemTime::now())
    }

    #[test]
    fn an_unsampled_reading_draws_an_empty_bar_with_no_number() {
        let r = row(&member(Some(proto::Vitals { health: 0.0, sampled_at_ms: 0, ..Default::default() }), None));

        assert!(!r.has_health);
        assert_eq!(r.health, "");
        assert_eq!(r.health_state, HealthState::Critical);
    }

    #[test]
    fn health_bands_and_rounds() {
        let at = |health| row(&member(Some(proto::Vitals { health, sampled_at_ms: 1, ..Default::default() }), None));

        assert_eq!(at(80.5).health, "81");
        assert_eq!(at(80.5).health_state, HealthState::Full);
        assert_eq!(at(59.9).health_state, HealthState::Hurt);
        assert_eq!(at(24.0).health_state, HealthState::Critical);
        assert_eq!(at(140.0).health_fraction, 1.0);
    }

    #[test]
    fn a_magazine_with_no_catalogue_shows_loaded_rounds_only() {
        let loadout = proto::Loadout {
            belt: Some(proto::Container {
                read_at_ms: 1,
                slots: vec![proto::Slot { index: 0, item_id: vec![1], count: 24, is_stack: false }],
            }),
            held_slot: Some(0),
            ..Default::default()
        };
        let r = row(&member(None, Some(loadout)));

        assert_eq!(r.ammo, "24");
        assert_eq!(r.meds, "");
        assert_eq!(r.held, "");
        assert_eq!(r.display_name, "Ana");
    }
}
