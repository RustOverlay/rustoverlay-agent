//! Domain events the log pipeline produces.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum GameEvent {
    Combat(CombatEvent),
    Triple(TransformTriple),
    Transform(CameraTransform),
    Identity(IdentityEvent),
    PlayerState(PlayerStateEvent),
}

impl GameEvent {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Self::Combat(e) => e.timestamp,
            Self::Triple(e) => e.timestamp,
            Self::Transform(e) => e.timestamp,
            Self::Identity(e) => e.timestamp,
            Self::PlayerState(e) => e.timestamp,
        }
    }
}

/// One combatlog row, with `timestamp` already resolved to absolute time.
#[derive(Debug, Clone, PartialEq)]
pub struct CombatEvent {
    pub timestamp: DateTime<Utc>,
    pub attacker: String,
    pub target: String,
    pub weapon: String,
    pub ammo: String,
    pub area: String,
    pub distance_metres: f64,
    pub old_hp: f64,
    pub new_hp: f64,
    pub info: Option<String>,
    pub hits: i32,
    pub integrity: f64,
    pub travel_seconds: f64,
    pub mismatch_metres: f64,
    pub desync: i32,
    pub names_ambiguous: bool,
}

impl CombatEvent {
    pub fn is_kill(&self) -> bool {
        self.info_is("killed")
    }

    pub fn is_wound(&self) -> bool {
        self.info_is("wounded")
    }

    pub fn damage(&self) -> f64 {
        self.old_hp - self.new_hp
    }

    fn info_is(&self, value: &str) -> bool {
        self.info.as_deref().is_some_and(|info| info.eq_ignore_ascii_case(value))
    }
}

/// A bare numeric triple emitted by `printpos` or `printeyes`. Which one it is cannot be told from
/// the line alone; see [`crate::correlation::TransformCorrelator`].
#[derive(Debug, Clone, PartialEq)]
pub struct TransformTriple {
    pub timestamp: DateTime<Utc>,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub label: Option<String>,
}

/// A full 6DoF camera transform, correlated from a `printpos;printeyes` pair. Rust is Y-up and
/// north is +Z.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraTransform {
    pub timestamp: DateTime<Utc>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub raw_pitch: f64,
    pub yaw: f64,
    pub roll: f64,
}

impl CameraTransform {
    /// Signed pitch, positive = looking up. Eulers are logged 0–360 with positive pitch
    /// *downward*, so a raw 351.98 is 8.02 degrees up.
    pub fn pitch_degrees(&self) -> f64 {
        -(if self.raw_pitch > 180.0 { self.raw_pitch - 360.0 } else { self.raw_pitch })
    }

    /// Compass bearing. UNVERIFIED that yaw maps 1:1 — confirm against an equipped compass.
    pub fn bearing_degrees(&self) -> f64 {
        ((self.yaw % 360.0) + 360.0) % 360.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdentityEvent {
    pub timestamp: DateTime<Utc>,
    pub steam_id: u64,
    pub name: Option<String>,
}

/// The `Entity: player` dump — name, SteamID and the boolean state flags.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerStateEvent {
    pub timestamp: DateTime<Utc>,
    pub steam_id: u64,
    pub name: String,
    pub flags: HashMap<String, bool>,
}

impl PlayerStateEvent {
    pub fn wounded(&self) -> bool {
        self.flag("Wounded")
    }

    pub fn sleeping(&self) -> bool {
        self.flag("Sleeping")
    }

    pub fn incapacitated(&self) -> bool {
        self.flag("Incapacitated")
    }

    pub fn safe_zone(&self) -> bool {
        self.flag("SafeZone")
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.get(name).copied().unwrap_or(false)
    }
}
