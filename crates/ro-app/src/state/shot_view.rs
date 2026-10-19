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
    let distance = match shot.distance {
        Some(d) => format!(
            "{} m ({}–{})",
            fixed(f64::from(d.center_m), 0),
            fixed(f64::from(d.range_m.0), 0),
            fixed(f64::from(d.range_m.1), 0)
        ),
        None => "distance unknown".to_owned(),
    };
    let switch = if shot.near_switch { "  ~switch" } else { "" };
    format!(
        "{time}  {front}° / {back}°  {distance}  ILD {} dB  level {} dB{switch}",
        signed_trimmed(f64::from(shot.ild_db), 1),
        fixed(f64::from(shot.level_db), 1),
    )
}

#[cfg(test)]
mod tests {
    use ro_audio::Distance;

    use super::*;

    #[test]
    fn a_shot_reads_as_bearing_pair_distance_range_and_features() {
        let mut feed = ShotFeed::new();
        feed.push(ShotEstimate {
            utc_ms: 0,
            ild_db: 8.04,
            level_db: -34.26,
            rel_bearing_deg: [-35.2, -144.8],
            distance: Some(Distance { center_m: 61.6, range_m: (58.9, 64.2) }),
            near_switch: false,
            confidence: 0.9,
        });

        let view = ShotView::of(&feed);

        assert_eq!(view.status, "Gunshot locator starting");
        assert!(view.shots[0].ends_with("  -35° / -145°  62 m (59–64)  ILD +8 dB  level -34.3 dB"), "{}", view.shots[0]);
    }

    #[test]
    fn a_shot_outside_the_level_tables_says_so() {
        let mut feed = ShotFeed::new();
        feed.push(ShotEstimate {
            utc_ms: 0,
            ild_db: 0.0,
            level_db: -58.0,
            rel_bearing_deg: [0.0, 180.0],
            distance: None,
            near_switch: false,
            confidence: 0.5,
        });

        assert!(ShotView::of(&feed).shots[0].contains("distance unknown"));
    }
}
