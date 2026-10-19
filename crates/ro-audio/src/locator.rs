use crate::estimate::estimate_close_without_distance;
use crate::{estimate, AudioBlock, AudioProfile, Detection, Detector, ShotEstimate};

/// Within this long after an accepted shot, a quieter onset is that shot's own later layer or echo
/// (replay: a cluster ~170 ms after onset, median 9 dB down). Repeat fire from one spot is level.
const ECHO_WINDOW_100NS: i64 = 10_000_000;
const ECHO_DROP_DB: f32 = 4.0;
/// A report's second layer re-triggers 60–74 ms after its onset, 2.4–4 dB down (replay, single shots);
/// the next round of a burst lands within ±2 dB of the last.
const LAYER_WINDOW_100NS: i64 = 800_000;
const LAYER_DROP_DB: f32 = 2.0;
/// Brighter than an AK report (max −4 dB), which flybys and impacts usually are. Some reports are
/// brighter still, so a bright onset is a report anyway when no shot was accepted just before it and
/// it is either barely bright (a silenced shot, about 0 dB) or louder than any flyby.
const MAX_REPORT_HI_LO_DB: f32 = -2.0;
/// A flyby trails its own report by the bullet's travel time: about 1 s at 250 m (replay).
const FLYBY_TRAIL_100NS: i64 = 20_000_000;
/// Silenced reports at 25 m measured −1 to +2 dB; flybys at 25 m start at about +3.5 dB.
const SILENCED_REPORT_MAX_HI_LO_DB: f32 = 4.0;
/// Flybys play at −35 ± 1 dB whatever the range; an M249 report at 25 m is −27 to −31 dB.
const LOUDER_THAN_FLYBY_DB: f32 = -32.0;
/// Quieter than this, a bright onset is ambience; silenced reports at 25 m are −55 to −61 dB.
const MIN_BRIGHT_REPORT_DB: f32 = -62.0;

pub struct Located {
    pub detection: Detection,
    pub estimate: ShotEstimate,
}

/// Why an onset was not taken for a gunshot report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// Brighter than any report: a flyby or an impact.
    Bright,
    /// Quieter than a shot accepted just before it: that shot's own later layer or echo.
    Echo,
}

/// Detector plus estimator over one continuous stream. A block at a new sample rate starts a fresh
/// detector, since the old one's history no longer lines up.
pub struct Locator {
    profile: AudioProfile,
    detector: Option<Detector>,
    /// UTC and level100 of the last accepted shot.
    last: Option<(i64, f32)>,
    /// Kept only for the replay's diagnostics; the live locator leaves this None.
    rejected: Option<Vec<(Detection, Rejection)>>,
}

impl Locator {
    pub fn new(profile: AudioProfile) -> Self {
        Self { profile, detector: None, last: None, rejected: None }
    }

    /// Also keeps every onset the filter turned down, for [`Self::take_rejected`].
    pub fn keeping_rejections(mut self) -> Self {
        self.rejected = Some(Vec::new());
        self
    }

    pub fn take_rejected(&mut self) -> Vec<(Detection, Rejection)> {
        self.rejected.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// Forgets the stream so far, e.g. when capture reconnects.
    pub fn reset(&mut self) {
        self.detector = None;
        self.last = None;
    }

    pub fn push(&mut self, block: &AudioBlock) -> Vec<Located> {
        let detector = match &mut self.detector {
            Some(d) if d.rate() == block.rate => d,
            slot => slot.insert(Detector::new(block.rate)),
        };
        let mut located = Vec::new();
        for detection in detector.push(block) {
            match self.rejection(&detection) {
                None => {
                    self.last = Some((detection.utc_100ns, detection.features.level100_db));
                    // The level tables were measured on AK reports, which are never this bright.
                    let estimate = if detection.features.hi_lo_db > MAX_REPORT_HI_LO_DB {
                        estimate_close_without_distance(&detection, &self.profile)
                    } else {
                        estimate(&detection, &self.profile)
                    };
                    located.push(Located { estimate, detection });
                }
                Some(why) => {
                    if let Some(rejected) = &mut self.rejected {
                        rejected.push((detection, why));
                    }
                }
            }
        }
        located
    }

    fn rejection(&self, detection: &Detection) -> Option<Rejection> {
        let f = &detection.features;
        if f.hi_lo_db > MAX_REPORT_HI_LO_DB && !self.is_bright_report(detection) {
            return Some(Rejection::Bright);
        }
        let (at, level) = self.last?;
        let since = detection.utc_100ns - at;
        let echo = since < ECHO_WINDOW_100NS && f.level100_db <= level - ECHO_DROP_DB;
        let layer = since < LAYER_WINDOW_100NS && f.level100_db <= level - LAYER_DROP_DB;
        (echo || layer).then_some(Rejection::Echo)
    }

    /// See `MAX_REPORT_HI_LO_DB`.
    fn is_bright_report(&self, detection: &Detection) -> bool {
        let f = &detection.features;
        let trails_a_shot = self.last.is_some_and(|(at, _)| detection.utc_100ns - at < FLYBY_TRAIL_100NS);
        !trails_a_shot
            && f.level100_db >= MIN_BRIGHT_REPORT_DB
            && (f.hi_lo_db <= SILENCED_REPORT_MAX_HI_LO_DB || f.level100_db >= LOUDER_THAN_FLYBY_DB)
    }
}
