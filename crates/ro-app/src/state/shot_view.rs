//! The gunshot locator's recent estimates, formatted for the Debug tab.

use chrono::{DateTime, Local};
use ro_audio::{ShotEstimate, ShotFeed};

use crate::format::{fixed, signed_trimmed};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShotView {
    pub status: String,
    /// Newest first.
    pub shots: Vec<String>,
}

impl ShotView {
    pub fn of(feed: &ShotFeed) -> Self {
        let status = if feed.status().is_empty() { "Gunshot locator starting".to_owned() } else { feed.status().to_owned() };
        Self { status, shots: feed.shots().map(describe).collect() }
    }
}

fn describe(shot: &ShotEstimate) -> String {
    let time = DateTime::from_timestamp_millis(shot.utc_ms)
        .map(|t| t.with_timezone(&Local).format("%H:%M:%S%.3f").to_string())
        .unwrap_or_default();
    let [front, back] = shot.rel_bearing_deg.map(|b| signed_trimmed(f64::from(b), 0));
    let (lo, hi) = shot.distance_range_m;
    let switch = if shot.near_switch { "  ~switch" } else { "" };
    format!(
        "{time}  {front}° / {back}°  {} m ({}–{})  ILD {} dB  level {} dB{switch}",
        fixed(f64::from(shot.distance_m), 0),
        fixed(f64::from(lo), 0),
        fixed(f64::from(hi), 0),
        signed_trimmed(f64::from(shot.ild_db), 1),
        fixed(f64::from(shot.level_db), 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shot_reads_as_bearing_pair_distance_range_and_features() {
        let mut feed = ShotFeed::new();
        feed.push(ShotEstimate {
            utc_ms: 0,
            ild_db: 8.04,
            level_db: -34.26,
            rel_bearing_deg: [-35.2, -144.8],
            distance_m: 61.6,
            distance_range_m: (58.9, 64.2),
            near_switch: false,
            confidence: 0.9,
        });

        let view = ShotView::of(&feed);

        assert_eq!(view.status, "Gunshot locator starting");
        assert!(view.shots[0].ends_with("  -35° / -145°  62 m (59–64)  ILD +8 dB  level -34.3 dB"), "{}", view.shots[0]);
    }
}
