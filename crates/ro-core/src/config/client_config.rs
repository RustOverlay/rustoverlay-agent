use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::invariant::{parse_bool, parse_double, parse_i32, read_all_lines};

/// Unity's FullScreenMode, as stored in `graphics.screenmode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenMode {
    ExclusiveFullScreen,
    Borderless,
    Maximized,
    Windowed,
    /// A value outside Unity's enum, kept as written — the C# cast did not validate either.
    Other(i32),
}

impl From<i32> for ScreenMode {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::ExclusiveFullScreen,
            1 => Self::Borderless,
            2 => Self::Maximized,
            3 => Self::Windowed,
            other => Self::Other(other),
        }
    }
}

/// Reads `cfg/client.cfg`, a flat dump of saved convars in `key "value"` form. Used to calibrate
/// the overlay instead of assuming defaults.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    /// Keyed by lowercased name: convar lookup is case-insensitive.
    values: HashMap<String, String>,
}

impl ClientConfig {
    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self::parse(read_all_lines(path)?))
    }

    pub fn parse<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> Self {
        let mut values = HashMap::new();

        for line in lines {
            let line = line.as_ref();
            let Some(space) = line.find(' ').filter(|&s| s > 0) else { continue };

            let value = line[space + 1..].trim().trim_matches('"');
            values.insert(line[..space].to_lowercase(), value.to_owned());
        }

        Self { values }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn screen_mode(&self) -> ScreenMode {
        self.get_int("graphics.screenmode", 1).into()
    }

    /// A topmost overlay cannot composite over true exclusive fullscreen.
    pub fn overlay_will_be_occluded(&self) -> bool {
        self.screen_mode() == ScreenMode::ExclusiveFullScreen
    }

    pub fn field_of_view(&self) -> f64 {
        self.get_double("graphics.fov", 90.0)
    }

    pub fn ui_scale(&self) -> f64 {
        self.get_double("graphics.uiscale", 1.0)
    }

    /// When set, displayed names are local-only Steam nicknames, so any name read from the log or
    /// the screen may not match the profile name and name→SteamID resolution silently fails.
    pub fn uses_steam_nicknames(&self) -> bool {
        self.get_bool("steam.use_steam_nicknames", false)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(&key.to_lowercase()).map(String::as_str)
    }

    fn get_int(&self, key: &str, fallback: i32) -> i32 {
        self.get(key).and_then(parse_i32).unwrap_or(fallback)
    }

    fn get_double(&self, key: &str, fallback: f64) -> f64 {
        self.get(key).and_then(parse_double).unwrap_or(fallback)
    }

    fn get_bool(&self, key: &str, fallback: bool) -> bool {
        self.get(key).and_then(parse_bool).unwrap_or(fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_convars_case_insensitively() {
        let config = ClientConfig::parse([
            "graphics.screenmode \"0\"",
            "Graphics.FOV \"75.5\"",
            "steam.use_steam_nicknames \"True\"",
            "noseparator",
        ]);

        assert_eq!(config.screen_mode(), ScreenMode::ExclusiveFullScreen);
        assert!(config.overlay_will_be_occluded());
        assert_eq!(config.field_of_view(), 75.5);
        assert!(config.uses_steam_nicknames());
        assert_eq!(config.ui_scale(), 1.0);
    }

    #[test]
    fn an_empty_config_answers_with_the_games_defaults() {
        let config = ClientConfig::empty();

        assert_eq!(config.screen_mode(), ScreenMode::Borderless);
        assert_eq!(config.field_of_view(), 90.0);
        assert!(!config.uses_steam_nicknames());
    }
}
