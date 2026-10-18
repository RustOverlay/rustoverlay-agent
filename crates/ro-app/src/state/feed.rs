//! The event feed and the player Rust says is playing, fed by the UI event pump (`AgentViewModel.Append`).

use std::collections::VecDeque;

use chrono::{DateTime, Local, Utc};
use ro_core::events::GameEvent;
use super::account::Player;
use crate::format::{signed_trimmed, trimmed};

/// One line in the debug feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    pub time: String,
    pub kind: String,
    pub detail: String,
}

impl EventRow {
    pub fn from_event(event: &GameEvent) -> Self {
        let row = |at: DateTime<Utc>, kind: &str, detail: String| Self { time: stamp(at), kind: kind.into(), detail };

        match event {
            GameEvent::Combat(c) => row(
                c.timestamp,
                if c.is_kill() {
                    "KILL"
                } else if c.is_wound() {
                    "WOUND"
                } else {
                    "HIT"
                },
                format!(
                    "{} → {}  {} dmg  {}  {}m  {}",
                    c.attacker,
                    c.target,
                    trimmed(c.damage(), 1),
                    c.area,
                    trimmed(c.distance_metres, 1),
                    weapon(&c.weapon)
                ),
            ),
            GameEvent::Transform(t) => row(
                t.timestamp,
                "TRANSFORM",
                format!(
                    "({}, {}, {})  bearing {}°  pitch {}°",
                    trimmed(t.x, 2),
                    trimmed(t.y, 2),
                    trimmed(t.z, 2),
                    trimmed(t.bearing_degrees(), 1),
                    signed_trimmed(t.pitch_degrees(), 1)
                ),
            ),
            GameEvent::Triple(p) => {
                row(p.timestamp, "triple", format!("{}, {}, {}", trimmed(p.a, 2), trimmed(p.b, 2), trimmed(p.c, 2)))
            }
            GameEvent::Identity(i) => {
                row(i.timestamp, "IDENTITY", format!("{}  {}", i.name.as_deref().unwrap_or("?"), i.steam_id))
            }
            GameEvent::PlayerState(s) => {
                // TODO(port): C# kept the log's flag order; ro-core's flags are a HashMap, so sorted.
                let mut set: Vec<&str> = s.flags.iter().filter(|(_, on)| **on).map(|(k, _)| k.as_str()).collect();
                set.sort_unstable();
                row(s.timestamp, "STATE", format!("{}  {}", s.name, set.join(" ")))
            }
        }
    }
}

fn stamp(at: DateTime<Utc>) -> String {
    at.with_timezone(&Local).format("%H:%M:%S%.3f").to_string()
}

/// `Path.GetFileNameWithoutExtension(weapon).Replace(".entity", "")`.
fn weapon(weapon: &str) -> String {
    if weapon == "N/A" {
        return String::new();
    }
    let file = weapon.rsplit(['/', '\\']).next().unwrap_or(weapon);
    let stem = file.rfind('.').map_or(file, |dot| &file[..dot]);
    stem.replace(".entity", "")
}

/// Newest first, capped at `AgentOptions.max_displayed_events`.
pub struct EventFeed {
    rows: VecDeque<EventRow>,
    received: u64,
    in_game: Option<Player>,
    max_rows: usize,
}

impl EventFeed {
    pub fn new(max_rows: usize) -> Self {
        Self { rows: VecDeque::new(), received: 0, in_game: None, max_rows }
    }

    pub fn append(&mut self, events: &[GameEvent]) {
        for event in events {
            if let GameEvent::Identity(id) = event {
                self.in_game = Some(Player::from_identity(id));
            }
            self.rows.push_front(EventRow::from_event(event));
        }

        self.received += events.len() as u64;
        self.rows.truncate(self.max_rows);
    }

    /// Who Rust last logged as playing. None until it does: the line is written once at
    /// server-connect and the tailer starts at EOF, so it is often never seen.
    pub fn in_game(&self) -> Option<&Player> {
        self.in_game.as_ref()
    }

    pub fn received(&self) -> u64 {
        self.received
    }

    // TODO(port): no view binds the rows, as in C#; kept for the debug feed that will.
    #[cfg(test)]
    pub fn rows(&self) -> impl Iterator<Item = &EventRow> {
        self.rows.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeZone;
    use ro_core::events::{CombatEvent, IdentityEvent, PlayerStateEvent};

    use super::*;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap()
    }

    fn combat(info: Option<&str>) -> GameEvent {
        GameEvent::Combat(CombatEvent {
            timestamp: at(),
            attacker: "you".into(),
            target: "bob".into(),
            weapon: "assets/prefabs/weapons/ak47u/ak47u.entity.prefab".into(),
            ammo: "riflebullet".into(),
            area: "head".into(),
            distance_metres: 23.456,
            old_hp: 100.0,
            new_hp: 52.75,
            info: info.map(Into::into),
            hits: 1,
            integrity: 1.0,
            travel_seconds: 0.0,
            mismatch_metres: 0.0,
            desync: 0,
            names_ambiguous: false,
        })
    }

    #[test]
    fn combat_rows_carry_kind_damage_and_the_bare_weapon_name() {
        let row = EventRow::from_event(&combat(Some("killed")));

        assert_eq!(row.kind, "KILL");
        assert_eq!(row.detail, "you → bob  47.3 dmg  head  23.5m  ak47u");
        assert_eq!(EventRow::from_event(&combat(Some("Wounded"))).kind, "WOUND");
        assert_eq!(EventRow::from_event(&combat(None)).kind, "HIT");
    }

    #[test]
    fn an_unknown_weapon_is_blank() {
        assert_eq!(weapon("N/A"), "");
        assert_eq!(weapon(r"a\b\knife.entity.prefab"), "knife");
    }

    #[test]
    fn identity_events_set_the_player_in_game_and_rows_are_capped() {
        let mut feed = EventFeed::new(2);
        assert_eq!(feed.in_game(), None);

        let identity = |name: Option<&str>| {
            GameEvent::Identity(IdentityEvent { timestamp: at(), steam_id: 7656, name: name.map(Into::into) })
        };
        feed.append(&[identity(Some("Hex")), combat(None), identity(None)]);

        assert_eq!(feed.in_game(), Some(&Player { name: String::new(), steam_id: 7656 }));
        assert_eq!(feed.received(), 3);
        assert_eq!(feed.rows().count(), 2);
        assert_eq!(feed.rows().next().unwrap().kind, "IDENTITY");
    }

    #[test]
    fn state_rows_list_only_the_set_flags() {
        let flags = HashMap::from([("Wounded".to_owned(), true), ("Sleeping".to_owned(), false), ("Aiming".to_owned(), true)]);
        let row = EventRow::from_event(&GameEvent::PlayerState(PlayerStateEvent {
            timestamp: at(),
            steam_id: 1,
            name: "Hex".into(),
            flags,
        }));

        assert_eq!(row.detail, "Hex  Aiming Wounded");
    }
}
