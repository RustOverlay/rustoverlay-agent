//! Matches located shots to CalibMod's ground truth and summarises the errors.

use std::fmt::Write as _;
use std::path::Path;

use ro_audio::{Detection, Located, Rejection};

/// A detection belongs to a shot when its onset lands this long after the server's fire time:
/// network plus pipeline latency is 105–116 ms, and distant clips lead in up to ~50 ms later.
const MATCH_FROM_MS: f64 = 60.0;
const MATCH_TO_MS: f64 = 260.0;
/// The player's own gun plays on their client at once; the server logs it a half-RTT later.
const OWN_FROM_MS: f64 = -200.0;
const OWN_TO_MS: f64 = 100.0;
/// Bot shots this close in fire time from different shooters count as overlapping fire.
const OVERLAP_MS: f64 = 150.0;
const BANDS: [(f64, f64, &str); 4] = [(0.0, 40.0, "<40 m"), (40.0, 125.0, "40-125 m"), (125.0, 250.0, "125-250 m"), (250.0, f64::INFINITY, ">=250 m")];

pub struct Shot {
    pub id: String,
    pub utc_ms: f64,
    pub distance_m: f64,
    pub rel_deg: f64,
    /// The listener's own gun (CalibMod `source=own`); scored apart from the bots' shots.
    pub own: bool,
    pub shooter_id: u32,
    /// 0 for a single shot or a burst's first round.
    pub burst_index: u32,
    pub overlapped: bool,
}

/// What the matcher made of one session.
pub struct Matched<'a> {
    pub outcomes: Vec<Outcome<'a>>,
    pub own_total: usize,
    /// Own shots the locator surfaced as gunshots; it should report none.
    pub own_surfaced: usize,
    pub false_positives: usize,
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
        Some(f64::from(self.found?.estimate.distance?.center_m) - self.shot.distance_m)
    }

    fn in_range(&self) -> Option<bool> {
        let (lo, hi) = self.found?.estimate.distance?.range_m;
        Some(f64::from(lo) - 1.0 <= self.shot.distance_m && self.shot.distance_m <= f64::from(hi) + 1.0)
    }
}

pub fn load_shots(path: &Path) -> Result<Vec<Shot>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().ok_or("shots csv: empty")?.split(',').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).ok_or(format!("shots csv: no {name} column"));
    let (id, utc, dist, rel) = (col("shot_id")?, col("utc_ms")?, col("distance_m")?, col("bearing_rel_deg")?);
    // Absent from CSVs written before ambush mode, which held only single bot shots.
    let (source, shooter, burst) = (col("source").ok(), col("shooter_id").ok(), col("burst_index").ok());
    let mut shots = lines
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split(',').collect();
            let num = |i: usize| cells.get(i).and_then(|c| c.parse::<f64>().ok()).ok_or(format!("shots csv: bad row '{line}'"));
            let int = |i: Option<usize>| i.and_then(|i| cells.get(i)).and_then(|c| c.parse::<u32>().ok()).unwrap_or(0);
            Ok(Shot {
                id: cells[id].to_owned(),
                utc_ms: num(utc)?,
                distance_m: num(dist)?,
                rel_deg: num(rel)?,
                own: source.and_then(|i| cells.get(i)).is_some_and(|s| *s == "own"),
                shooter_id: int(shooter),
                burst_index: int(burst),
                overlapped: false,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    mark_overlaps(&mut shots);
    Ok(shots)
}

fn mark_overlaps(shots: &mut [Shot]) {
    let bots: Vec<(f64, u32)> = shots.iter().filter(|s| !s.own).map(|s| (s.utc_ms, s.shooter_id)).collect();
    for shot in shots.iter_mut().filter(|s| !s.own) {
        shot.overlapped =
            bots.iter().any(|&(t, by)| by != shot.shooter_id && (t - shot.utc_ms).abs() <= OVERLAP_MS);
    }
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

/// Each bot shot takes the earliest unclaimed detection in its window; own shots then take theirs
/// from what is left, so a missed bot shot is never excused by a nearby own shot.
pub fn match_shots<'a>(shots: &'a [Shot], located: &'a [Located]) -> Matched<'a> {
    let mut claimed = vec![false; located.len()];
    let mut claim = |shot: &Shot, window: std::ops::RangeInclusive<f64>| {
        let hit = located.iter().enumerate().find(|(i, l)| {
            let delay = l.detection.utc_100ns as f64 / 10_000.0 - shot.utc_ms;
            !claimed[*i] && window.contains(&delay)
        });
        hit.map(|(i, l)| {
            claimed[i] = true;
            l
        })
    };
    let outcomes: Vec<Outcome> =
        shots.iter().filter(|s| !s.own).map(|shot| Outcome { shot, found: claim(shot, MATCH_FROM_MS..=MATCH_TO_MS) }).collect();
    let own: Vec<&Shot> = shots.iter().filter(|s| s.own).collect();
    let own_surfaced = own.iter().filter(|shot| claim(shot, OWN_FROM_MS..=OWN_TO_MS).is_some()).count();
    Matched { outcomes, own_total: own.len(), own_surfaced, false_positives: claimed.iter().filter(|c| !**c).count() }
}

pub fn report(matched: &Matched<'_>, minutes: f64) -> String {
    let outcomes = &matched.outcomes;
    let hits = outcomes.iter().filter(|o| o.found.is_some()).count();
    let mut out = format!(
        "recall {hits}/{} ({:.1}%), false positives {} ({:.1}/min)\n",
        outcomes.len(),
        100.0 * hits as f64 / outcomes.len().max(1) as f64,
        matched.false_positives,
        matched.false_positives as f64 / minutes.max(1e-9)
    );
    let recall = |pick: &dyn Fn(&Shot) -> bool| {
        let subset: Vec<&Outcome> = outcomes.iter().filter(|o| pick(o.shot)).collect();
        (subset.iter().filter(|o| o.found.is_some()).count(), subset.len())
    };
    let (single, single_n) = recall(&|s| s.burst_index == 0 && !s.overlapped);
    let (follow, follow_n) = recall(&|s| s.burst_index > 0);
    let (over, over_n) = recall(&|s| s.overlapped);
    if follow_n + over_n + matched.own_total > 0 {
        let _ = writeln!(
            out,
            "recall: single {single}/{single_n}, burst follow-ups {follow}/{follow_n}, overlapping {over}/{over_n}; own gun surfaced {}/{}",
            matched.own_surfaced, matched.own_total
        );
    }
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

/// Why each missed shot was missed: the detector never fired in its window, or it did and the
/// locator's filter turned the onset down.
pub fn miss_reasons(outcomes: &[Outcome<'_>], rejected: &[(Detection, Rejection)]) -> String {
    let (mut silent, mut bright, mut echo) = (0, 0, 0);
    for o in outcomes.iter().filter(|o| o.found.is_none()) {
        let why = rejected.iter().find(|(d, _)| {
            (MATCH_FROM_MS..=MATCH_TO_MS).contains(&(d.utc_100ns as f64 / 10_000.0 - o.shot.utc_ms))
        });
        match why.map(|(_, r)| *r) {
            None => silent += 1,
            Some(Rejection::Bright) => bright += 1,
            Some(Rejection::Echo) => echo += 1,
        }
    }
    format!("missed: no onset {silent}, rejected as bright {bright}, rejected as echo {echo}\n")
}

/// Every onset the filter turned down, beside the latest shot fired before it.
pub fn rejected_csv(shots: &[Shot], rejected: &[(Detection, Rejection)]) -> String {
    let mut out = String::from("utc_ms,reason,after_shot_ms,shot_id,shot_distance_m,level100_db,ild20_db,hi_lo_db,early_late_db,rise_db\n");
    for (d, why) in rejected {
        let t = d.utc_100ns as f64 / 10_000.0;
        let shot = shots.iter().filter(|s| !s.own && s.utc_ms <= t).max_by(|a, b| a.utc_ms.total_cmp(&b.utc_ms));
        let f = &d.features;
        let _ = writeln!(
            out,
            "{t:.1},{why:?},{:.1},{},{},{:.2},{:.2},{:.2},{:.2},{:.1}",
            shot.map_or(f64::NAN, |s| t - s.utc_ms),
            shot.map_or("", |s| s.id.as_str()),
            shot.map_or(f64::NAN, |s| s.distance_m),
            f.level100_db,
            f.ild20_db,
            f.hi_lo_db,
            f.early_late_db,
            f.rise_db
        );
    }
    out
}

/// One `key=value` line for score/run.sh to compare against its baseline.
pub fn summary(matched: &Matched<'_>) -> String {
    let found: Vec<&Outcome> = matched.outcomes.iter().filter(|o| o.found.is_some()).collect();
    let bearing: Vec<f64> = found.iter().filter_map(|o| o.bearing_error()).collect();
    let distance: Vec<f64> = found.iter().filter_map(|o| o.distance_error().map(f64::abs)).collect();
    format!(
        "SUMMARY recall={} shots={} fp={} own={}/{} bearing_p50={:.1} bearing_p90={:.1} dist_p50={:.1} dist_p90={:.1} dist_unknown={}\n",
        found.len(),
        matched.outcomes.len(),
        matched.false_positives,
        matched.own_surfaced,
        matched.own_total,
        percentile(&bearing, 50.0),
        percentile(&bearing, 90.0),
        percentile(&distance, 50.0),
        percentile(&distance, 90.0),
        found.len() - distance.len()
    )
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
                let (center, (lo, hi)) = e.distance.map_or((f32::NAN, (f32::NAN, f32::NAN)), |d| (d.center_m, d.range_m));
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
                    center,
                    lo,
                    hi,
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
