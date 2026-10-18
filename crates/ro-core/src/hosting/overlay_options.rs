use chrono::TimeDelta;
use serde::{Deserialize, Serialize};

/// The `Overlay` section of settings.json. Field order and names are the file's shape — existing
/// files must keep loading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct OverlayOptions {
    /// The key that shows and hides the panel, in Rust's own lowercase naming — see
    /// [`crate::input::key_names`]. A tap, registered while Rust is in front and sampled otherwise
    /// (see `HotkeyBinder`), so default to something the game leaves alone.
    pub toggle_key: String,

    pub enabled_at_launch: bool,

    /// Offset from the top-left of Rust's client area, in physical pixels.
    pub offset_x: i32,

    pub offset_y: i32,

    /// Scales the whole overlay — rows, bars, icons and inspector squares alike. One factor so
    /// nothing ends up out of proportion; icons decode at 48px, so past ~1.4 the squares soften.
    pub scale: f64,

    /// How often the panel checks whether Rust is in front and whether a key was hit.
    ///
    /// Fast because `GetAsyncKeyState` reports whether a key is down *now*, so a tap between two
    /// polls is never seen. 15ms because Windows timers round up to a ~15.6ms tick: 20ms measures
    /// at 31ms, 15ms at 15.6ms, and anything lower lands on the same tick.
    #[serde(with = "crate::timespan")]
    pub poll_interval: TimeDelta,

    /// How old a panel-backed container may be before counts drawn from it are marked stale. The
    /// main grid is retained, not trusted, once the inventory shuts — see `LoadoutState`.
    #[serde(with = "crate::timespan")]
    pub stale_after: TimeDelta,

    pub inspect: InspectOptions,
}

impl OverlayOptions {
    pub const SECTION: &str = "Overlay";
}

impl Default for OverlayOptions {
    fn default() -> Self {
        Self {
            toggle_key: "f4".into(),
            enabled_at_launch: true,
            offset_x: 16,
            offset_y: 16,
            scale: 1.0,
            poll_interval: TimeDelta::milliseconds(15),
            stale_after: TimeDelta::seconds(5),
            inspect: InspectOptions::default(),
        }
    }
}

/// The hold-to-inspect keys, under `Overlay:Inspect`.
///
/// `hold_key` is sampled and never consumed, because a hold needs the key's release and a
/// registered key is only announced on the way down. The taps — `cycle_key` and `select_keys` —
/// are registered while Rust is in front; see `HotkeyBinder`.
///
/// Whether Rust still acts on a registered key is untested, so assume it might: put the number row
/// in `select_keys` and selecting the second teammate may also put belt slot 2 in your hands. It
/// ships empty for that reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct InspectOptions {
    /// Held down for as long as the inventory should be on screen.
    pub hold_key: String,

    /// Tapped while `hold_key` is down to advance to the next teammate.
    pub cycle_key: String,

    /// Optional direct jumps, first entry selecting the first row.
    pub select_keys: Vec<String>,

    pub cell_size: f64,

    /// Sizes the inventory grids on top of [`OverlayOptions::scale`], not instead of it, so the
    /// grids can be made larger without dragging the compact list up with them.
    pub scale: f64,
}

impl Default for InspectOptions {
    fn default() -> Self {
        Self {
            hold_key: "leftalt".into(),
            cycle_key: "c".into(),
            select_keys: Vec::new(),
            cell_size: 40.0,
            scale: 1.0,
        }
    }
}
