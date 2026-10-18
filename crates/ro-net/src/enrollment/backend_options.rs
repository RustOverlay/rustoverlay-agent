use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use url::Url;

/// The `Backend` section of settings.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct BackendOptions {
    pub base_url: Url,

    /// How long the whole sign-in may take. Bounds a user who opens the browser and wanders off,
    /// which would otherwise leave the listener bound forever.
    #[serde(with = "ro_core::timespan")]
    pub enrollment_timeout: TimeDelta,
}

impl BackendOptions {
    pub const SECTION: &str = "Backend";

    pub fn is_default(&self) -> bool {
        self.base_url.origin() == Self::default().base_url.origin()
    }

    /// `stem.ext` for the default backend, `stem-<host>-<port>.ext` for any other. A token one
    /// backend minted is unknown to another, and one shared file meant each run against the other
    /// backend signed the player out and deleted it.
    pub fn file_name(&self, stem: &str, ext: &str) -> String {
        if self.is_default() {
            return format!("{stem}.{ext}");
        }
        let host: String = self
            .base_url
            .host_str()
            .unwrap_or("unknown")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
            .collect();
        match self.base_url.port_or_known_default() {
            Some(port) => format!("{stem}-{host}-{port}.{ext}"),
            None => format!("{stem}-{host}.{ext}"),
        }
    }
}

impl Default for BackendOptions {
    fn default() -> Self {
        Self {
            base_url: Url::parse("https://api.rustoverlay.com").expect("valid default URL"),
            enrollment_timeout: TimeDelta::minutes(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(url: &str) -> BackendOptions {
        BackendOptions { base_url: Url::parse(url).unwrap(), ..Default::default() }
    }

    #[test]
    fn files_are_scoped_to_their_backend_and_the_default_keeps_its_old_name() {
        assert_eq!(at("https://api.rustoverlay.com/").file_name("install", "token"), "install.token");
        assert_eq!(at("http://localhost:5000").file_name("install", "token"), "install-localhost-5000.token");
        assert_eq!(at("https://staging.rustoverlay.com").file_name("active-team", "json"), "active-team-staging.rustoverlay.com-443.json");
        assert_eq!(at("http://[::1]:8080").file_name("install", "token"), "install-___1_-8080.token");
    }
}
