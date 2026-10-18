//! Saving overlay settings. The file is the player's, shared with hand-written configuration for
//! every other part of the agent, so the invariant that matters is what a save leaves alone.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ro_core::hosting::{OverlayOptions, OverlaySettingsStore};
use serde_json::Value;
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self { dir: tempfile::tempdir().unwrap() }
    }

    fn path(&self) -> PathBuf {
        self.dir.path().join("settings.json")
    }

    fn store(&self, seed: Option<OverlayOptions>) -> OverlaySettingsStore {
        OverlaySettingsStore::new(self.dir.path(), seed.unwrap_or_default())
    }

    fn saved(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.path()).unwrap()).unwrap()
    }
}

#[test]
fn an_edit_is_applied_and_written() {
    let f = Fixture::new();
    let store = f.store(None);

    store.update(|options| options.scale = 1.25);

    assert_eq!(store.current().scale, 1.25);
    assert_eq!(f.saved()["Overlay"]["Scale"].as_f64(), Some(1.25));
}

#[test]
fn configuration_the_settings_screen_does_not_own_survives_a_save() {
    let f = Fixture::new();
    std::fs::write(f.path(), r#"{"Agent": {"RustPath": "D:\\Rust"}, "Overlay": {"OffsetX": 99}}"#).unwrap();

    f.store(None).update(|options| options.offset_x = 40);

    let saved = f.saved();
    assert_eq!(saved["Agent"]["RustPath"].as_str(), Some("D:\\Rust"));
    assert_eq!(saved["Overlay"]["OffsetX"].as_i64(), Some(40));
}

#[test]
fn comments_and_trailing_commas_startup_accepts_do_not_cost_the_other_sections() {
    let f = Fixture::new();
    std::fs::write(
        f.path(),
        "\u{FEFF}{\n  // my path\n  \"Agent\": {\"RustPath\": \"D:\\\\Rust\",},\n  \"Backend\": {\"Url\": \"https://x\"},\n}",
    )
    .unwrap();

    f.store(None).update(|options| options.offset_x = 40);

    let saved = f.saved();
    assert_eq!(saved["Agent"]["RustPath"].as_str(), Some("D:\\Rust"));
    assert_eq!(saved["Backend"]["Url"].as_str(), Some("https://x"));
    assert_eq!(saved["Overlay"]["OffsetX"].as_i64(), Some(40));
}

#[test]
fn a_save_goes_through_a_temporary_file_and_leaves_none_behind() {
    let f = Fixture::new();
    std::fs::write(f.path(), r#"{"Agent": {"RustPath": "D:\\Rust"}}"#).unwrap();

    f.store(None).update(|options| options.offset_y = 3);

    assert_eq!(f.saved()["Overlay"]["OffsetY"].as_i64(), Some(3));
    let names: Vec<_> = std::fs::read_dir(f.dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, ["settings.json"]);
}

#[test]
fn a_file_that_is_not_valid_json_is_replaced_rather_than_failing_the_save() {
    let f = Fixture::new();
    std::fs::write(f.path(), "{ this was hand-edited badly").unwrap();

    f.store(None).update(|options| options.offset_y = 7);

    assert_eq!(f.saved()["Overlay"]["OffsetY"].as_i64(), Some(7));
}

#[test]
fn nested_inspector_settings_are_written_too() {
    let f = Fixture::new();

    f.store(None).update(|options| options.inspect.hold_key = "leftcontrol".into());

    assert_eq!(f.saved()["Overlay"]["Inspect"]["HoldKey"].as_str(), Some("leftcontrol"));
}

#[test]
fn reset_returns_the_shipped_defaults_rather_than_what_this_run_started_with() {
    let f = Fixture::new();
    let store = f.store(Some(OverlayOptions { toggle_key: "f9".into(), scale: 1.8, ..Default::default() }));

    store.reset();

    assert_eq!(store.current().toggle_key, OverlayOptions::default().toggle_key);
    assert_eq!(store.current().scale, 1.0);
}

#[test]
fn every_change_tells_whoever_is_drawing_from_it() {
    let f = Fixture::new();
    let store = f.store(None);
    let raised = Arc::new(AtomicUsize::new(0));
    let counter = raised.clone();
    store.on_changed(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    store.update(|options| options.offset_x = 1);
    store.reset();

    assert_eq!(raised.load(Ordering::SeqCst), 2);
}

#[test]
fn the_written_shape_matches_what_the_csharp_agent_wrote() {
    let f = Fixture::new();
    // Written by System.Text.Json from the C# OverlayOptions; existing users have exactly this.
    std::fs::write(
        f.path(),
        r#"{"Overlay": {"ToggleKey": "f6", "EnabledAtLaunch": false, "OffsetX": 3, "OffsetY": 4, "Scale": 1.5,
            "PollInterval": "00:00:00.0150000", "StaleAfter": "00:00:05",
            "Inspect": {"HoldKey": "leftalt", "CycleKey": "c", "SelectKeys": ["1"], "CellSize": 40, "Scale": 1}}}"#,
    )
    .unwrap();

    let loaded: OverlayOptions = serde_json::from_value(f.saved()["Overlay"].clone()).unwrap();
    assert_eq!(loaded.toggle_key, "f6");
    assert_eq!(loaded.inspect.select_keys, ["1"]);

    f.store(Some(loaded)).update(|_| {});
    let saved = f.saved();
    assert_eq!(saved["Overlay"]["PollInterval"].as_str(), Some("00:00:00.0150000"));
    assert_eq!(saved["Overlay"]["StaleAfter"].as_str(), Some("00:00:05"));
}
