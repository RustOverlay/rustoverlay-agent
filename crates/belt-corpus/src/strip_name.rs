//! Recorded strips are named `belt-yyyyMMdd-HHmmss-fff-{w}x{h}-at{x},{y}[-ui{scale}].png`; the name
//! is the only record of the frame they came from. See docs/matcher-tools.md.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;

use chrono::NaiveDateTime;
use regex::Regex;
use ro_core::invariant;

static STAMP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^belt-(\d{8})-(\d{6})-(\d{3})-").unwrap());
// The -ui suffix is optional: frames recorded before it existed are all uiscale 1.0.
static SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-(\d+)x(\d+)-at(\d+),(\d+)(?:-ui([\d.]+))?$").unwrap());

#[derive(Clone, Debug, PartialEq)]
pub struct StripName {
    /// The file name without `.png`.
    pub name: String,
    /// `yyyyMMdd`, which is all the per-frame dump prints.
    pub date: String,
    pub read_at: SystemTime,
    pub frame_width: i32,
    pub frame_height: i32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub ui_scale: f64,
}

/// `Directory.GetFiles(directory, "belt-*.png").Order()`.
// TODO(port): C# ordered with the culture-aware string comparer; this is ordinal. Names differ only
// in fixed-width digits, so the orders agree on every corpus recorded so far.
pub fn list(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("belt-") && name.ends_with(".png") && entry.file_type()?.is_file() {
            paths.push(entry.path());
        }
    }

    paths.sort();
    Ok(paths)
}

/// The strip a file name describes, when it is one and was captured within `from..=to` (`HHmmss`).
/// Err for a name that matches but cannot be read, where C# threw.
pub fn select(name: &str, from: &str, to: &str) -> Result<Option<StripName>, String> {
    let (Some(stamp), Some(shape)) = (STAMP.captures(name), SHAPE.captures(name)) else { return Ok(None) };
    let time = &stamp[2];

    if time < from || time > to {
        return Ok(None);
    }

    let text = format!("{}{}.{}", &stamp[1], time, &stamp[3]);
    let read_at = NaiveDateTime::parse_from_str(&text, "%Y%m%d%H%M%S%.3f")
        .map_err(|e| format!("{name}: bad capture time: {e}"))?
        .and_utc()
        .into();

    let int = |i: usize| shape[i].parse::<i32>().map_err(|e| format!("{name}: {e}"));
    let ui_scale = match shape.get(5) {
        None => 1.0,
        Some(scale) => invariant::parse_double(scale.as_str()).ok_or_else(|| format!("{name}: bad ui scale"))?,
    };

    Ok(Some(StripName {
        name: name.to_owned(),
        date: stamp[1].to_owned(),
        read_at,
        frame_width: int(1)?,
        frame_height: int(2)?,
        origin_x: int(3)?,
        origin_y: int(4)?,
        ui_scale,
    }))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    #[test]
    fn reads_shape_and_capture_time() {
        let strip = select("belt-20260809-105230-801-1440x1080-at481,978", "000000", "999999").unwrap().unwrap();

        assert_eq!(strip.date, "20260809");
        assert_eq!((strip.frame_width, strip.frame_height, strip.origin_x, strip.origin_y), (1440, 1080, 481, 978));
        assert_eq!(strip.ui_scale, 1.0);
        let expected = UNIX_EPOCH + Duration::from_millis(1_786_272_750_801);
        assert_eq!(strip.read_at, expected);
    }

    #[test]
    fn reads_the_ui_scale_suffix() {
        let strip = select("belt-20260809-105230-801-2560x1440-at900,1300-ui1.25", "000000", "999999").unwrap().unwrap();

        assert_eq!(strip.ui_scale, 1.25);
    }

    #[test]
    fn bounds_are_inclusive_capture_times() {
        let name = "belt-20260809-105408-000-1440x1080-at481,978";

        assert!(select(name, "105408", "999999").unwrap().is_some());
        assert!(select(name, "000000", "105408").unwrap().is_some());
        assert!(select(name, "105409", "999999").unwrap().is_none());
        assert!(select(name, "000000", "105407").unwrap().is_none());
    }

    #[test]
    fn other_names_are_skipped_and_bad_dates_fail() {
        assert_eq!(select("belt-notes", "000000", "999999"), Ok(None));
        assert!(select("belt-20261309-105230-801-1440x1080-at481,978", "000000", "999999").is_err());
        // Out of range is skipped before the date is read, as in C#.
        assert_eq!(select("belt-20261309-105230-801-1440x1080-at481,978", "200000", "999999"), Ok(None));
    }
}
