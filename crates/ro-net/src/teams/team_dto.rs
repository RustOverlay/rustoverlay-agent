use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::web_json::number;

// Absent properties take defaults, as System.Text.Json left unmatched constructor parameters.
// TODO(port): a missing DateTimeOffset defaulted to 0001-01-01; chrono's default is the Unix epoch.

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamDto {
    pub id: Uuid,
    pub name: String,
    pub join_code: String,
    #[serde(deserialize_with = "number")]
    pub owner_steam_id: u64,
    pub created_at: DateTime<FixedOffset>,
    pub members: Vec<TeamMemberDto>,
}

impl TeamDto {
    pub fn member_count(&self) -> usize {
        self.members.len()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamMemberDto {
    #[serde(deserialize_with = "number")]
    pub steam_id: u64,
    pub role: String,
    pub joined_at: DateTime<FixedOffset>,
}

/// What a join code buys before you commit to it. No SteamIDs, by design.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TeamPreviewDto {
    pub name: String,
    #[serde(deserialize_with = "number")]
    pub member_count: i32,
}
