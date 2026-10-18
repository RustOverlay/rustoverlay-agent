//! Matches located shots to CalibMod's ground truth and summarises the errors.

use std::fmt::Write as _;
use std::path::Path;

use ro_audio::Located;

/// A detection belongs to a shot when its onset lands this long after the server's fire time:
/// network plus pipeline latency is 105–116 ms, and distant clips lead in up to ~50 ms later.
const MATCH_FROM_MS: f64 = 60.0;
const MATCH_TO_MS: f64 = 260.0;
const BANDS: [(f64, f64, &str); 4] = [(0.0, 40.0, "<40 m"), (40.0, 125.0, "40-125 m"), (125.0, 250.0, "125-250 m"), (250.0, f64::INFINITY, ">=250 m")];

pub struct Shot {
    pub id: String,
    pub utc_ms: f64,
    pub distance_m: f64,
    pub rel_deg: f64,
}

pub struct Outcome<'a> {
    pub shot: &'a Shot,
    pub found: Option<&'a Located>,
}

impl Outcome<'_> {
    fn bearing_error(&self) -> Option<f64> {
        let e = &self.found?.estimate;
        e.rel_bearing_deg.iter().map(|&b| angle_between(f64::from(b), self.shot.rel_deg)).reduce(f64::min)
    }

    fn distance_error(&self) -> Option<f64> {
        Some(f64::from(self.found?.estimate.distance_m) - self.shot.distance_m)
    }

    fn in_range(&self) -> Option<bool> {
        let (lo, hi) = self.found?.estimate.distance_range_m;
        Some(f64::from(lo) - 1.0 <= self.shot.distance_m && self.shot.distance_m <= f64::from(hi) + 1.0)
    }
}

pub fn load_shots(path: &Path) -> Result<Vec<Shot>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().ok_or("shots csv: empty")?.split(',').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).ok_or(format!("shots csv: no {name} column"));
    let (id, utc, dist, rel) = (col("shot_id")?, col("utc_ms")?, col("distance_m")?, col("bearing_rel_deg")?);
    lines
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split(',').collect();
            let num = |i: usize| cells.get(i).and_then(|c| c.parse::<f64>().ok()).ok_or(format!("shots csv: bad row '{line}'"));
            Ok(Shot { id: cells[id].to_owned(), utc_ms: num(utc)?, distance_m: num(dist)?, rel_deg: num(rel)? })
        })
        .collect()
}

/// Where unmatched detections fall after the latest shot fired before them, as "label count" pairs.
pub fn false_positive_delays(shots: &[Shot], located: &[Located], outcomes: &[Outcome<'_>]) -> String {
    let matched: Vec<i64> = outcomes.iter().filter_map(|o| o.found.map(|l| l.detection.utc_100ns)).collect();
    let mut buckets = [0usize; 5];
    for l in located.iter().filter(|l| !matched.contains(&l.detection.utc_100ns)) {
        let t = l.detection.utc_100ns as f64 / 10_000.0;
        let after = shots.iter().filter(|s| s.utc_ms <= t).map(|s| t - s.utc_ms).reduce(f64::min);
        let i = match after {
            Some(ms) if ms < 600.0 => 0,
            Some(ms) if ms < 1200.0 => 1,
            Some(ms) if ms < 3000.0 => 2,
            Some(_) => 3,
            None => 4,
        };
        buckets[i] += 1;
    }
    format!(
        "false positives after the last shot: <0.6 s {}, 0.6-1.2 s {}, 1.2-3 s {}, later {}, before any {}\n",
        buckets[0], buckets[1], buckets[2], buckets[3], buckets[4]
    )
}

/// Each shot takes the earliest unclaimed detection in its window.
pub fn match_shots<'a>(shots: &'a [Shot], located: &'a [Located]) -> (Vec<Outcome<'a>>, usize) {
    let mut claimed = vec![false; located.len()];
    let outcomes = shots
        .iter()
        .map(|shot| {
            let hit = located.iter().enumerate().find(|(i, l)| {
                let delay = l.detection.utc_100ns as f64 / 10_000.0 - shot.utc_ms;
                !claimed[*i] && (MATCH_FROM_MS..=MATCH_TO_MS).contains(&delay)
            });
            let found = hit.map(|(i, l)| {
                claimed[i] = true;
                l
            });
            Outcome { shot, found }
        })
        .collect();
    (outcomes, claimed.iter().filter(|c| !**c).count())
}

pub fn report(outcomes: &[Outcome<'_>], false_positives: usize, minutes: f64) -> String {
    let hits = outcomes.iter().filter(|o| o.found.is_some()).count();
    let mut out = format!(
        "recall {hits}/{} ({:.1}%), false positives {false_positives} ({:.1}/min)\n",
        outcomes.len(),
        100.0 * hits as f64 / outcomes.len().max(1) as f64,
        false_positives as f64 / minutes.max(1e-9)
    );
    let _ = writeln!(out, "{:<10} {:>3}  {:>18}  {:>22}  {:>8}", "band", "n", "bearing err med/p90", "distance |err| med/p90", "in range");
    for (lo, hi, name) in BANDS {
        let band: Vec<&Outcome> = outcomes.iter().filter(|o| o.shot.distance_m >= lo && o.shot.distance_m < hi && o.found.is_some()).collect();
        if band.is_empty() {
            continue;
        }
        let bearing: Vec<f64> = band.iter().filter_map(|o| o.bearing_error()).collect();
        let distance: Vec<f64> = band.iter().filter_map(|o| o.distance_error().map(f64::abs)).collect();
        let in_range = band.iter().filter(|o| o.in_range() == Some(true)).count();
        let _ = writeln!(
            out,
            "{name:<10} {:>3}  {:>8.1}° / {:>5.1}°  {:>9.1} m / {:>6.1} m  {:>4}/{}",
            band.len(),
            percentile(&bearing, 50.0),
            percentile(&bearing, 90.0),
            percentile(&distance, 50.0),
            percentile(&distance, 90.0),
            in_range,
            band.len()
        );
    }
    out
}

pub fn per_shot_csv(outcomes: &[Outcome<'_>]) -> String {
    let mut out = String::from(
        "shot_id,true_distance_m,true_rel_deg,onset_delay_ms,ild20_db,level100_db,hi_lo_db,early_late_db,rise_db,\
         rel_a_deg,rel_b_deg,bearing_err_deg,distance_m,range_lo_m,range_hi_m,near_switch,confidence\n",
    );
    for o in outcomes {
        let s = o.shot;
        let _ = write!(out, "{},{},{}", s.id, s.distance_m, s.rel_deg);
        match o.found {
            None => out.push_str(",,,,,,,,,,,,,,\n"),
            Some(l) => {
                let (f, e) = (&l.detection.features, &l.estimate);
                let _ = writeln!(
                    out,
                    ",{:.1},{:.2},{:.2},{:.2},{:.2},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{},{:.2}",
                    l.detection.utc_100ns as f64 / 10_000.0 - s.utc_ms,
                    f.ild20_db,
                    f.level100_db,
                    f.hi_lo_db,
                    f.early_late_db,
                    f.rise_db,
                    e.rel_bearing_deg[0],
                    e.rel_bearing_deg[1],
                    o.bearing_error().unwrap_or(f64::NAN),
                    e.distance_m,
                    e.distance_range_m.0,
                    e.distance_range_m.1,
                    e.near_switch,
                    e.confidence
                );
            }
        }
    }
    out
}

/// Every detection with its delay after the latest shot fired before it and whether it matched one.
pub fn detections_csv(shots: &[Shot], located: &[Located], outcomes: &[Outcome<'_>]) -> String {
    let matched: Vec<i64> = outcomes.iter().filter_map(|o| o.found.map(|l| l.detection.utc_100ns)).collect();
    let mut out = String::from("utc_ms,after_shot_ms,matched,background_db,rise_db,level100_db,ild20_db,hi_lo_db,early_late_db\n");
    for l in located {
        let (t, f) = (l.detection.utc_100ns as f64 / 10_000.0, &l.detection.features);
        let after = shots.iter().filter(|s| s.utc_ms <= t).map(|s| t - s.utc_ms).reduce(f64::min).unwrap_or(f64::NAN);
        let _ = writeln!(
            out,
            "{t:.1},{after:.1},{},{:.1},{:.1},{:.2},{:.2},{:.1},{:.1}",
            matched.contains(&l.detection.utc_100ns),
            l.detection.background_db,
            f.rise_db,
            f.level100_db,
            f.ild20_db,
            f.hi_lo_db,
            f.early_late_db
        );
    }
    out
}

fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = p / 100.0 * (v.len() - 1) as f64;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    v[lo] + (v[hi] - v[lo]) * (rank - lo as f64)
}
