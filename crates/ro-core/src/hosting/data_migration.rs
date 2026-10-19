//! Moves the agent's own files out of Velopack's install root into the data directory.
//!
//! Until 0.2.1 the data directory was `%LOCALAPPDATA%/RustOverlay`, which is also where Velopack
//! installs. Setup empties that folder before installing, and uninstall deletes it, so a reinstall
//! wiped the token, settings and every recorded corpus. Only names the agent writes are moved;
//! anything else there may be Velopack's, and moving it would break updates.

use std::fs;
use std::path::Path;

/// Files the agent writes at the top of its data directory; `*` is a wildcard.
const FILES: [&str; 6] =
    ["settings.json", "install*.token", "active-team*.json", "icon-signatures.bin", "audio-profile.json", "survivors.txt"];
const DIRECTORIES: [&str; 6] = ["logs", "frames", "calib", "game-skin-icons", "belt-corpus*", "audio-corpus"];

/// Moves what it recognises from `legacy` to `current`, never over something already there.
/// Returns one line per entry moved or left behind, for logging once logging is up.
pub fn migrate_legacy_data(legacy: &Path, current: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(legacy) else { return Vec::new() };
    if let Err(e) = fs::create_dir_all(current) {
        return vec![format!("could not create {}: {e}", current.display())];
    }

    let mut report = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        let patterns: &[&str] = if is_dir { &DIRECTORIES } else { &FILES };
        if !patterns.iter().any(|p| matches(p, &name)) {
            continue;
        }

        let target = current.join(&name);
        if target.exists() {
            report.push(format!("kept {name} in {}: {} already has one", legacy.display(), current.display()));
            continue;
        }
        match fs::rename(entry.path(), &target) {
            Ok(()) => report.push(format!("moved {name} to {}", current.display())),
            Err(e) => report.push(format!("could not move {name}: {e}")),
        }
    }
    report
}

/// Case-insensitive, with at most one `*`, which matches any run of characters.
fn matches(pattern: &str, name: &str) -> bool {
    let (pattern, name) = (pattern.to_ascii_lowercase(), name.to_ascii_lowercase());
    match pattern.split_once('*') {
        Some((head, tail)) => name.len() >= head.len() + tail.len() && name.starts_with(head) && name.ends_with(tail),
        None => pattern == name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    #[test]
    fn moves_the_agents_files_and_leaves_velopacks_alone() {
        let root = tempfile::tempdir().unwrap();
        let (legacy, current) = (root.path().join("RustOverlay"), root.path().join("RustOverlayData"));
        for file in ["settings.json", "install.token", "install-localhost-8000.token", "active-team.json", "Update.exe"] {
            touch(&legacy.join(file));
        }
        touch(&legacy.join("belt-corpus-skins").join("frame.png"));
        touch(&legacy.join("current").join("RustOverlay.Agent.exe"));
        touch(&legacy.join("packages").join("RustOverlay-0.2.0-full.nupkg"));

        let report = migrate_legacy_data(&legacy, &current);

        for moved in ["settings.json", "install.token", "install-localhost-8000.token", "active-team.json"] {
            assert!(current.join(moved).is_file(), "{moved} not moved");
            assert!(!legacy.join(moved).exists());
        }
        assert!(current.join("belt-corpus-skins").join("frame.png").is_file());
        for stays in ["Update.exe", "current", "packages"] {
            assert!(legacy.join(stays).exists(), "{stays} was moved");
        }
        assert_eq!(report.len(), 5);
    }

    #[test]
    fn never_overwrites_what_the_data_directory_already_has() {
        let root = tempfile::tempdir().unwrap();
        let (legacy, current) = (root.path().join("old"), root.path().join("new"));
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(legacy.join("settings.json"), "old").unwrap();
        fs::write(current.join("settings.json"), "new").unwrap();

        migrate_legacy_data(&legacy, &current);

        assert_eq!(fs::read_to_string(current.join("settings.json")).unwrap(), "new");
        assert_eq!(fs::read_to_string(legacy.join("settings.json")).unwrap(), "old");
    }

    #[test]
    fn a_missing_legacy_directory_is_nothing_to_do() {
        let root = tempfile::tempdir().unwrap();
        assert!(migrate_legacy_data(&root.path().join("absent"), &root.path().join("new")).is_empty());
    }
}
