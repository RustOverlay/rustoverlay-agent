use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::{RustInstall, RustInstallLocator};
use crate::hosting::AgentOptions;

static LIBRARY_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""path"\s*"(?<path>[^"]+)""#).expect("valid regex"));

/// Finds Rust through the Steam registry keys, falling back to a configured path. Returns `None`
/// off Windows rather than failing, so the platform-free test suite exercises the same code path.
#[derive(Debug, Clone, Default)]
pub struct SteamRustInstallLocator {
    configured: Option<String>,
}

impl SteamRustInstallLocator {
    pub fn new(options: Option<&AgentOptions>) -> Self {
        Self { configured: options.and_then(|o| o.rust_path.clone()) }
    }
}

impl RustInstallLocator for SteamRustInstallLocator {
    fn locate(&self) -> Option<RustInstall> {
        if let Some(configured) = self.configured.as_deref().filter(|p| !p.trim().is_empty()) {
            return Path::new(configured).is_dir().then(|| RustInstall::new(configured));
        }

        steam_libraries()
            .into_iter()
            .map(|library| library.join("steamapps").join("common").join("Rust"))
            .find(|candidate| candidate.is_dir())
            .map(RustInstall::new)
    }
}

fn steam_libraries() -> Vec<PathBuf> {
    let Some(steam) = steam_root() else { return Vec::new() };
    let mut libraries = vec![steam.clone()];

    // Games are commonly on a second drive; libraryfolders.vdf lists every registered root.
    let manifest = steam.join("steamapps").join("libraryfolders.vdf");
    if !manifest.is_file() {
        return libraries;
    }

    let Ok(bytes) = std::fs::read(&manifest) else { return libraries };
    let content = String::from_utf8_lossy(&bytes);

    libraries.extend(
        LIBRARY_PATH
            .captures_iter(&content)
            .map(|c| PathBuf::from(c["path"].replace(r"\\", r"\"))),
    );
    libraries
}

/// Steam's own install directory, from the registry; None off Windows or without Steam.
#[cfg(windows)]
pub fn steam_root() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

    const STEAM_KEY: &str = r"SOFTWARE\Valve\Steam";

    for view in [KEY_WOW64_32KEY, KEY_WOW64_64KEY] {
        let path = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(STEAM_KEY, KEY_READ | view)
            .and_then(|key| key.get_value::<String, _>("InstallPath"));

        if let Ok(path) = path
            && Path::new(&path).is_dir()
        {
            return Some(PathBuf::from(path));
        }
    }

    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(STEAM_KEY)
        .and_then(|key| key.get_value::<String, _>("SteamPath"))
        .ok()
        .map(PathBuf::from)
}

#[cfg(not(windows))]
pub fn steam_root() -> Option<PathBuf> {
    None
}
