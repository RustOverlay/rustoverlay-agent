//! The settings screen, editing `OverlaySettingsStore` in place (`SettingsViewModel`).
//!
//! Every setter writes straight through to the store, which saves and republishes. There is no
//! dirty state and no Save button: scale and offset are tuned by watching the overlay move over the
//! game, which an unapplied form cannot show you. A value equal to the current one is not written,
//! so an idle form never keeps re-saving the file it just saved.

use ro_core::hosting::{OverlayOptions, OverlaySettingsStore};
use ro_core::input::key_names;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Toggle,
    Hold,
    Cycle,
}

impl Bind {
    pub fn get(self, options: &OverlayOptions) -> &str {
        match self {
            Self::Toggle => &options.toggle_key,
            Self::Hold => &options.inspect.hold_key,
            Self::Cycle => &options.inspect.cycle_key,
        }
    }

    fn slot(self, options: &mut OverlayOptions) -> &mut String {
        match self {
            Self::Toggle => &mut options.toggle_key,
            Self::Hold => &mut options.inspect.hold_key,
            Self::Cycle => &mut options.inspect.cycle_key,
        }
    }
}

/// A name the watcher would ignore is refused rather than stored: the settings screen only ever
/// offers keys from the same table `KeyWatcher` resolves against.
pub fn set_bind(store: &OverlaySettingsStore, bind: Bind, key: &str) {
    if key_names::code_of(key) == 0 || bind.get(&store.current()) == key {
        return;
    }
    store.update(|options| *bind.slot(options) = key.to_owned());
}

/// Percent rather than a factor, because that is what the slider reads out.
pub fn scale_percent(options: &OverlayOptions) -> f64 {
    options.scale * 100.0
}

pub fn set_scale_percent(store: &OverlaySettingsStore, percent: f64) {
    let scale = percent.clamp(50.0, 200.0) / 100.0;
    if scale != store.current().scale {
        store.update(|options| options.scale = scale);
    }
}

/// Relative to the overlay's own scale, which it nests inside rather than replaces.
pub fn inventory_scale_percent(options: &OverlayOptions) -> f64 {
    options.inspect.scale * 100.0
}

pub fn set_inventory_scale_percent(store: &OverlaySettingsStore, percent: f64) {
    let scale = percent.clamp(50.0, 200.0) / 100.0;
    if scale != store.current().inspect.scale {
        store.update(|options| options.inspect.scale = scale);
    }
}

pub fn set_offset_x(store: &OverlaySettingsStore, value: i32) {
    if value != store.current().offset_x {
        store.update(|options| options.offset_x = value);
    }
}

pub fn set_offset_y(store: &OverlaySettingsStore, value: i32) {
    if value != store.current().offset_y {
        store.update(|options| options.offset_y = value);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn store() -> (tempfile::TempDir, OverlaySettingsStore, Arc<AtomicUsize>) {
        let dir = tempfile::tempdir().unwrap();
        let store = OverlaySettingsStore::new(dir.path(), OverlayOptions::default());
        let saves = Arc::new(AtomicUsize::new(0));
        let counter = saves.clone();
        store.on_changed(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        (dir, store, saves)
    }

    #[test]
    fn unknown_keys_are_refused_and_known_ones_stored() {
        let (_dir, store, saves) = store();

        set_bind(&store, Bind::Cycle, "nonsense");
        assert_eq!(store.current().inspect.cycle_key, "c");

        set_bind(&store, Bind::Cycle, "v");
        assert_eq!(store.current().inspect.cycle_key, "v");
        assert_eq!(saves.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn scale_is_clamped_and_unchanged_values_are_not_saved() {
        let (_dir, store, saves) = store();

        set_scale_percent(&store, 500.0);
        assert_eq!(store.current().scale, 2.0);
        set_scale_percent(&store, 200.0);
        set_inventory_scale_percent(&store, 10.0);
        assert_eq!(store.current().inspect.scale, 0.5);
        set_offset_x(&store, 16);

        assert_eq!(saves.load(Ordering::Relaxed), 2);
        assert_eq!(scale_percent(&store.current()), 200.0);
    }
}
