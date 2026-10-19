//! Streaming onset detector, the live counterpart of tools/calib/shots_report.py: a 1 ms envelope of
//! the channel sum, triggered at ≥ 12 dB over the background, with features taken once 100 ms of
//! audio past the onset has arrived.

mod features;

use std::collections::VecDeque;

pub use features::{FeatureExtractor, ShotFeatures};
use features::{block_power, db_power};

use crate::AudioBlock;

pub const ONSET_RISE_DB: f64 = 12.0;
/// Background window: [onset − 300 ms, onset − 50 ms), as shots_report's [fire − 300, fire − 50).
const BACKGROUND_FROM_MS: usize = 300;
const BACKGROUND_TO_MS: usize = 50;
/// Under every automatic weapon's round spacing (MP5 ~100 ms, AK ~133 ms): longer merged a burst into
/// its first round. A report's own later layers are rejected as echoes by `Locator`, not here.
const REFRACTORY_MS: u64 = 60;
/// The onset block must also jump this far over the preceding `RECENT_MS`: a shot's own decaying
/// tail stays 12 dB over the pre-shot background for ~0.5 s and would otherwise retrigger.
const LOCAL_RISE_DB: f64 = 6.0;
const RECENT_MS: usize = 20;
/// Keeps digital silence (nothing rendering) from making any sound at all an onset.
const BACKGROUND_FLOOR_POWER: f64 = 1e-10;

#[derive(Clone, Copy, Debug)]
pub struct Detection {
    /// Frame index of the onset in the stream fed to this detector.
    pub onset_frame: u64,
    /// UTC of the onset, Unix epoch 100-ns ticks.
    pub utc_100ns: i64,
    pub background_db: f32,
    pub features: ShotFeatures,
}

struct Pending {
    onset_frame: u64,
    background_power: f64,
}

pub struct Detector {
    extractor: FeatureExtractor,
    hop: usize,
    left: VecDeque<f32>,
    right: VecDeque<f32>,
    /// Frame index of `left[0]`.
    base: u64,
    /// Frames pushed so far.
    total: u64,
    /// Envelope power per completed 1 ms block; `envelope[i]` covers block `env_base + i`.
    envelope: VecDeque<f64>,
    env_base: u64,
    /// (first frame, UTC) of each pushed block, for timestamping onsets.
    anchors: VecDeque<(u64, i64)>,
    refractory_until: u64,
    pending: Vec<Pending>,
}

impl Detector {
    pub fn new(rate: u32) -> Self {
        let extractor = FeatureExtractor::new(rate);
        let hop = (rate / 1000) as usize;
        Self {
            extractor,
            hop,
            left: VecDeque::new(),
            right: VecDeque::new(),
            base: 0,
            total: 0,
            envelope: VecDeque::new(),
            env_base: 0,
            anchors: VecDeque::new(),
            refractory_until: 0,
            pending: Vec::new(),
        }
    }

    pub fn rate(&self) -> u32 {
        self.extractor.rate()
    }

    /// Appends one stereo block and returns every onset whose features became computable.
    pub fn push(&mut self, block: &AudioBlock) -> Vec<Detection> {
        self.anchors.push_back((self.total, block.utc_100ns));
        let mut found = Vec::new();
        for &[l, r] in block.samples.as_chunks::<2>().0 {
            self.left.push_back(l);
            self.right.push_back(r);
            self.total += 1;
            if self.total.is_multiple_of(self.hop as u64) {
                self.close_envelope_block();
            }
        }
        self.finish_pending(&mut found);
        self.trim();
        found
    }

    fn close_envelope_block(&mut self) {
        let start = (self.total - self.hop as u64 - self.base) as usize;
        let (l, r) = (self.slice(&self.left, start, self.hop), self.slice(&self.right, start, self.hop));
        let power = block_power(&l, &r);
        self.envelope.push_back(power);

        let block = self.env_base + self.envelope.len() as u64 - 1;
        if (block as usize) < BACKGROUND_FROM_MS || block < self.refractory_until {
            return;
        }
        let background = self.background(block).max(BACKGROUND_FLOOR_POWER);
        if db_power(power) - db_power(background) >= ONSET_RISE_DB && db_power(power) - db_power(self.recent(block)) >= LOCAL_RISE_DB {
            self.refractory_until = block + REFRACTORY_MS;
            self.pending.push(Pending { onset_frame: block * self.hop as u64, background_power: background });
        }
    }

    /// Median envelope power over the background window: robust to an earlier shot's tail.
    fn background(&self, block: u64) -> f64 {
        let from = (block - BACKGROUND_FROM_MS as u64 - self.env_base) as usize;
        let to = (block - BACKGROUND_TO_MS as u64 - self.env_base) as usize;
        let mut window: Vec<f64> = self.envelope.range(from..to).copied().collect();
        let mid = window.len() / 2;
        *window.select_nth_unstable_by(mid, f64::total_cmp).1
    }

    /// Mean envelope power over the `RECENT_MS` blocks before `block`.
    fn recent(&self, block: u64) -> f64 {
        let to = (block - self.env_base) as usize;
        self.envelope.range(to - RECENT_MS..to).sum::<f64>() / RECENT_MS as f64
    }

    fn finish_pending(&mut self, found: &mut Vec<Detection>) {
        let need = self.extractor.window() as u64;
        let ready: Vec<Pending> = {
            let total = self.total;
            let (ready, waiting) = std::mem::take(&mut self.pending).into_iter().partition(|p| p.onset_frame + need <= total);
            self.pending = waiting;
            ready
        };
        for p in ready {
            let start = (p.onset_frame - self.base) as usize;
            let n = self.extractor.window();
            let (l, r) = (self.slice(&self.left, start, n), self.slice(&self.right, start, n));
            found.push(Detection {
                onset_frame: p.onset_frame,
                utc_100ns: self.utc_of(p.onset_frame),
                background_db: db_power(p.background_power) as f32,
                features: self.extractor.compute(&l, &r, p.background_power),
            });
        }
    }

    fn utc_of(&self, frame: u64) -> i64 {
        let (start, utc) = self.anchors.iter().rev().find(|(start, _)| *start <= frame).copied().unwrap_or((0, 0));
        utc + ((frame - start) as i128 * 10_000_000 / i128::from(self.rate())) as i64
    }

    fn slice(&self, channel: &VecDeque<f32>, start: usize, len: usize) -> Vec<f32> {
        channel.range(start..start + len).copied().collect()
    }

    /// Keeps the background window of envelope, and the frames of any pending onset or partial block.
    fn trim(&mut self) {
        let env_from = (self.total / self.hop as u64).saturating_sub(BACKGROUND_FROM_MS as u64 + 1);
        while self.env_base < env_from && !self.envelope.is_empty() {
            self.envelope.pop_front();
            self.env_base += 1;
        }

        let oldest_pending = self.pending.iter().map(|p| p.onset_frame).min().unwrap_or(u64::MAX);
        let keep_from = oldest_pending.min(self.total - self.total % self.hop as u64);
        while self.base < keep_from {
            self.left.pop_front();
            self.right.pop_front();
            self.base += 1;
        }
        while self.anchors.len() > 1 && self.anchors[1].0 <= self.base {
            self.anchors.pop_front();
        }
    }
}
