use ro_core::events::CombatEvent;

use crate::proto;

pub fn combat_to_proto(evt: &CombatEvent) -> proto::CombatEvent {
    proto::CombatEvent {
        occurred_at_ms: evt.timestamp.timestamp_millis(),
        attacker: evt.attacker.clone(),
        target: evt.target.clone(),
        weapon: evt.weapon.clone(),
        ammo: evt.ammo.clone(),
        area: evt.area.clone(),
        distance_metres: evt.distance_metres,
        old_hp: evt.old_hp,
        new_hp: evt.new_hp,
        info: evt.info.clone().unwrap_or_default(),
        hits: evt.hits,
        integrity: evt.integrity,
        travel_seconds: evt.travel_seconds,
        mismatch_metres: evt.mismatch_metres,
        desync: evt.desync,
        // Carried, never resolved: the combatlog has names, not SteamIDs, and the split is a guess.
        names_ambiguous: evt.names_ambiguous,
    }
}
