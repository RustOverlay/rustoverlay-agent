use crate::{estimate, AudioBlock, AudioProfile, Detection, Detector, ShotEstimate};

/// Within this long after an accepted shot, a quieter onset is that shot's own later layer or echo
/// (replay: a cluster ~170 ms after onset, median 9 dB down). Repeat fire from one spot is level.
const ECHO_WINDOW_100NS: i64 = 10_000_000;
const ECHO_DROP_DB: f32 = 4.0;
/// Brighter than any gunshot report measured (max −4 dB): flybys and impacts, not a report.
const MAX_REPORT_HI_LO_DB: f32 = -2.0;

pub struct Located {
    pub detection: Detection,
    pub estimate: ShotEstimate,
}

/// Detector plus estimator over one continuous stream. A block at a new sample rate starts a fresh
/// detector, since the old one's history no longer lines up.
pub struct Locator {
    profile: AudioProfile,
    detector: Option<Detector>,
    /// UTC and level100 of the last accepted shot.
    last: Option<(i64, f32)>,
}

impl Locator {
    pub fn new(profile: AudioProfile) -> Self {
        Self { profile, detector: None, last: None }
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
            if self.is_report(&detection) {
                self.last = Some((detection.utc_100ns, detection.features.level100_db));
                located.push(Located { estimate: estimate(&detection, &self.profile), detection });
            }
        }
        located
    }

    fn is_report(&self, detection: &Detection) -> bool {
        let f = &detection.features;
        if f.hi_lo_db > MAX_REPORT_HI_LO_DB {
            return false;
        }
        match self.last {
            Some((at, level)) if detection.utc_100ns - at < ECHO_WINDOW_100NS => f.level100_db > level - ECHO_DROP_DB,
            _ => true,
        }
    }
}
