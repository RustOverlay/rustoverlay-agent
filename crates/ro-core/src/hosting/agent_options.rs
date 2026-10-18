use chrono::TimeDelta;
use serde::{Deserialize, Serialize};

/// The `Agent` section of appsettings.json / settings.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AgentOptions {
    /// Overrides Steam auto-detection when set.
    pub rust_path: Option<String>,

    /// Parse the existing log before following it. Off by default so a launch does not replay
    /// hours of history.
    pub replay_from_start: bool,

    #[serde(with = "crate::timespan")]
    pub poll_interval: TimeDelta,

    /// How long a position triple waits for its orientation partner.
    #[serde(with = "crate::timespan")]
    pub correlation_window: TimeDelta,

    #[serde(with = "crate::timespan")]
    pub dedupe_tolerance: TimeDelta,

    #[serde(with = "crate::timespan")]
    pub dedupe_retention: TimeDelta,

    /// Rows kept in the debug feed before the oldest are discarded.
    pub max_displayed_events: i32,
}

impl AgentOptions {
    pub const SECTION: &str = "Agent";
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            rust_path: None,
            replay_from_start: false,
            poll_interval: TimeDelta::milliseconds(150),
            correlation_window: TimeDelta::milliseconds(500),
            dedupe_tolerance: TimeDelta::seconds(1),
            dedupe_retention: TimeDelta::hours(1),
            max_displayed_events: 500,
        }
    }
}
