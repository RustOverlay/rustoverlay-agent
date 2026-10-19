//! Features → bearing pair and distance, by inverting the profile's lookup tables.

use crate::profile::AudioProfile;
use crate::{Detection, ShotFeatures, Table};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotEstimate {
    pub utc_ms: i64,
    pub ild_db: f32,
    pub level_db: f32,
    /// Bearing relative to the view, degrees, + = shooter to the right: θ in [−90, 90] and its
    /// front/back mirror. Stereo cannot tell them apart.
    pub rel_bearing_deg: [f32; 2],
    /// None when the report is not one the level tables were measured on (see `Locator`'s bright
    /// reports): a silenced shot at 25 m reads like an AK at 300 m.
    pub distance: Option<Distance>,
    /// hi_lo says near clip but early/late says distant clip: anywhere from ~100 to ~200 m.
    pub near_switch: bool,
    /// 0–1: how far the onset rose over the background, discounted by the width of the range.
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Distance {
    pub center_m: f32,
    pub range_m: (f32, f32),
}

/// For a report the level tables were not measured on, which `Locator` only accepts close in (a
/// silenced report is inaudible past ~25 m): the bearing from the near ILD table, and no distance.
pub fn estimate_close_without_distance(detection: &Detection, profile: &AudioProfile) -> ShotEstimate {
    let f = &detection.features;
    let sin = lateral(&profile.ild.near, f.ild20_db);
    let (front, back) = bearing_pair(f.ild20_db, sin);
    ShotEstimate {
        utc_ms: detection.utc_100ns.div_euclid(10_000),
        ild_db: f.ild20_db,
        level_db: f.level100_db,
        rel_bearing_deg: [front as f32, back as f32],
        distance: None,
        near_switch: false,
        confidence: rise_confidence(f) as f32,
    }
}

/// ILD positive means the shooter is on the left, which is a negative bearing.
fn bearing_pair(ild_db: f32, sin: f64) -> (f64, f64) {
    let front = -f64::from(ild_db.signum()) * sin.asin().to_degrees();
    let back = if front >= 0.0 { 180.0 - front } else { -180.0 - front };
    (front, back)
}

fn rise_confidence(f: &ShotFeatures) -> f64 {
    ((f64::from(f.rise_db) - 12.0) / 12.0).clamp(0.25, 1.0)
}

#[derive(Clone, Copy, PartialEq)]
enum Zone {
    Inside,
    Beyond,
    Ambiguous,
}

pub fn estimate(detection: &Detection, profile: &AudioProfile) -> ShotEstimate {
    let f = &detection.features;
    let zone = zone(f, profile);

    let mut sin = lateral(&profile.ild.far, f.ild20_db);
    let mut distance = distances(f.level100_db, sin, zone, profile);
    if distance.0 < profile.ild.near_max_m {
        sin = lateral(&profile.ild.near, f.ild20_db);
        distance = distances(f.level100_db, sin, zone, profile);
    }
    let (center, lo, hi) = distance;
    let (front, back) = bearing_pair(f.ild20_db, sin);
    let rise = rise_confidence(f);
    let spread = (hi - lo) / center.max(1.0);

    ShotEstimate {
        utc_ms: detection.utc_100ns.div_euclid(10_000),
        ild_db: f.ild20_db,
        level_db: f.level100_db,
        rel_bearing_deg: [front as f32, back as f32],
        distance: Some(Distance { center_m: center as f32, range_m: (lo as f32, hi as f32) }),
        near_switch: zone == Zone::Ambiguous,
        confidence: (rise / (1.0 + spread)) as f32,
    }
}

fn zone(f: &ShotFeatures, profile: &AudioProfile) -> Zone {
    let [lo, hi] = profile.switch.hi_lo_band_db;
    let hi_lo = f64::from(f.hi_lo_db);
    if hi_lo < lo || hi_lo > hi {
        Zone::Beyond
    } else if f64::from(f.early_late_db) >= profile.switch.early_late_far_db {
        Zone::Ambiguous
    } else {
        Zone::Inside
    }
}

/// |sin(rel)| from the ILD magnitude. At or past the table's last ILD the shot is hard-panned.
fn lateral(table: &Table, ild_db: f32) -> f64 {
    let magnitude = f64::from(ild_db.abs());
    let last = table.0[table.0.len() - 1];
    if magnitude >= last[1] { 1.0 } else { table.invert_increasing(magnitude).clamp(0.0, 1.0) }
}

/// (center, low, high) metres. Every distance whose table level matches within the tolerance is
/// kept, so the beyond-switch dip and the ambiguous zone come back as their full range.
fn distances(level_db: f32, sin: f64, zone: Zone, profile: &AudioProfile) -> (f64, f64, f64) {
    let level = f64::from(level_db) - profile.lateral_level_db * sin * sin;
    let tol = profile.level_tolerance_db;
    let tables: &[&Table] = match zone {
        Zone::Inside => &[&profile.inside_table],
        Zone::Beyond => &[&profile.beyond_table],
        Zone::Ambiguous => &[&profile.inside_table, &profile.beyond_table],
    };

    let centers: Vec<f64> = tables.iter().flat_map(|t| t.solutions(level)).collect();
    let edges: Vec<f64> = tables
        .iter()
        .flat_map(|t| [t.solutions(level - tol), t.solutions(level + tol)])
        .flatten()
        .chain(centers.iter().copied())
        .collect();

    let (cmin, cmax) = min_max(&centers);
    let (lo, hi) = min_max(&edges);
    ((cmin + cmax) / 2.0, lo, hi)
}

fn min_max(values: &[f64]) -> (f64, f64) {
    values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection(ild: f32, level: f32, hi_lo: f32, early_late: f32) -> Detection {
        Detection {
            onset_frame: 0,
            utc_100ns: 0,
            background_db: -70.0,
            features: ShotFeatures {
                ild20_db: ild,
                level100_db: level,
                hi_lo_db: hi_lo,
                early_late_db: early_late,
                rise_db: 30.0,
                ..ShotFeatures::default()
            },
        }
    }

    #[test]
    fn a_hard_left_shot_inside_the_switch() {
        let e = estimate(&detection(38.0, -33.8 - 2.85, -12.0, 0.0), &AudioProfile::builtin());
        assert!((e.rel_bearing_deg[0] + 90.0).abs() < 8.0, "{e:?}");
        assert!((e.rel_bearing_deg[1] + 90.0).abs() < 8.0, "{e:?}");
        assert!((e.distance.unwrap().center_m - 60.0).abs() < 5.0, "{e:?}");
        assert!(!e.near_switch);
    }

    #[test]
    fn a_dark_shot_beyond_the_switch_returns_the_dip_as_a_range() {
        let e = estimate(&detection(-5.6, -42.0, -19.0, 7.0), &AudioProfile::builtin());
        assert!(e.rel_bearing_deg[0] > 0.0 && e.rel_bearing_deg[1] > 90.0, "{e:?}");
        let (lo, hi) = e.distance.unwrap().range_m;
        assert!(lo < 142.0 && hi > 185.0, "{e:?}");
    }
}
