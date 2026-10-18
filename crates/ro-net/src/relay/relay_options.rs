use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use url::Url;

/// The `Relay` section of settings.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct RelayOptions {
    pub base_url: Url,

    /// Cloudflare closes idle WebSockets at roughly 100s, so this fires regardless of traffic.
    #[serde(with = "ro_core::timespan")]
    pub heartbeat_interval: TimeDelta,

    /// Floor between loadout frames. The screen is read at ~1Hz; the wire need not be faster.
    #[serde(with = "ro_core::timespan")]
    pub min_loadout_interval: TimeDelta,

    /// Unconditional resend, so a frame the relay dropped does not leave teammates stale.
    #[serde(with = "ro_core::timespan")]
    pub loadout_refresh_interval: TimeDelta,

    /// Also the base of the backoff after the relay refuses a session.
    #[serde(with = "ro_core::timespan")]
    pub reconnect_delay: TimeDelta,

    /// The relay sends a heartbeat every 30s and a snapshot every 10s, so silence this long means a
    /// half-open link. Zero disables the bound.
    #[serde(with = "ro_core::timespan")]
    pub idle_timeout: TimeDelta,
}

impl RelayOptions {
    pub const SECTION: &str = "Relay";
}

impl Default for RelayOptions {
    fn default() -> Self {
        Self {
            base_url: Url::parse("wss://relay-eu.rustoverlay.com").expect("valid default URL"),
            heartbeat_interval: TimeDelta::seconds(30),
            min_loadout_interval: TimeDelta::seconds(1),
            loadout_refresh_interval: TimeDelta::seconds(15),
            reconnect_delay: TimeDelta::seconds(5),
            idle_timeout: TimeDelta::seconds(60),
        }
    }
}
