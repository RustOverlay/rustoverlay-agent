//! Per-shot features, computed exactly as tools/calib/shots_report.py (ild20, level100) and
//! blend_features.py (hi_lo, early_late) do, from the onset sample onward.

use std::f64::consts::PI;

/// Everything the estimator needs from one onset, plus its rise for confidence.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShotFeatures {
    /// L − R RMS over 0–20 ms, dB. Positive means the shooter is on the left.
    pub ild20_db: f32,
    /// RMS of the mono mean over 0–100 ms, dBFS.
    pub level100_db: f32,
    /// 2–8 kHz vs 0.1–0.8 kHz energy over the Hann-windowed first 50 ms, dB.
    pub hi_lo_db: f32,
    /// 0–20 ms vs 20–100 ms energy of the mono mean, dB.
    pub early_late_db: f32,
    /// Loudest 1 ms envelope block in the 100 ms window over the background, dB.
    pub rise_db: f32,
}

/// Precomputed windows and twiddles for one sample rate.
pub struct FeatureExtractor {
    rate: u32,
    n20: usize,
    n50: usize,
    n100: usize,
    hop: usize,
    hann: Vec<f64>,
    cos: Vec<f64>,
    sin: Vec<f64>,
    lo_bins: Vec<usize>,
    hi_bins: Vec<usize>,
}

impl FeatureExtractor {
    pub fn new(rate: u32) -> Self {
        let samples = |ms: u32| (u64::from(rate) * u64::from(ms) / 1000) as usize;
        let (n20, n50, n100) = (samples(20), samples(50), samples(100));
        let hann = (0..n50).map(|n| 0.5 - 0.5 * (2.0 * PI * n as f64 / (n50 - 1) as f64).cos()).collect();
        let cos = (0..n50).map(|j| (2.0 * PI * j as f64 / n50 as f64).cos()).collect();
        let sin = (0..n50).map(|j| (2.0 * PI * j as f64 / n50 as f64).sin()).collect();
        // numpy.fft.rfftfreq: k / (n * (1 / rate)).
        let freq = |k: usize| k as f64 / (n50 as f64 * (1.0 / f64::from(rate)));
        let bins = |lo: f64, hi: f64| (0..=n50 / 2).filter(|&k| freq(k) >= lo && freq(k) < hi).collect();
        Self { rate, n20, n50, n100, hop: samples(1), hann, cos, sin, lo_bins: bins(100.0, 800.0), hi_bins: bins(2000.0, 8000.0) }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Frames of audio needed after the onset.
    pub fn window(&self) -> usize {
        self.n100
    }

    /// `left`/`right` start at the onset and hold at least [`Self::window`] frames.
    pub fn compute(&self, left: &[f32], right: &[f32], background_power: f64) -> ShotFeatures {
        let (l20, r20) = (&left[..self.n20], &right[..self.n20]);
        let ild = db_amplitude(rms(l20.iter().map(|&v| f64::from(v)))) - db_amplitude(rms(r20.iter().map(|&v| f64::from(v))));

        // numpy's block.mean(axis=1) averages in float32 before blend_features casts to float64.
        let mono: Vec<f64> = left[..self.n100].iter().zip(&right[..self.n100]).map(|(&l, &r)| f64::from((l + r) / 2.0)).collect();
        let level = db_amplitude(rms(mono.iter().copied()));

        let early: f64 = mono[..self.n20].iter().map(|v| v * v).sum();
        let late: f64 = mono[self.n20..].iter().map(|v| v * v).sum();

        let windowed: Vec<f64> = mono[..self.n50].iter().zip(&self.hann).map(|(v, w)| v * w).collect();
        let band = |bins: &[usize]| 10.0 * (bins.iter().map(|&k| self.power(&windowed, k)).sum::<f64>() + 1e-20).log10();

        let peak = left[..self.n100]
            .chunks_exact(self.hop)
            .zip(right[..self.n100].chunks_exact(self.hop))
            .map(|(l, r)| block_power(l, r))
            .fold(0.0, f64::max);

        ShotFeatures {
            ild20_db: ild as f32,
            level100_db: level as f32,
            hi_lo_db: (band(&self.hi_bins) - band(&self.lo_bins)) as f32,
            early_late_db: (10.0 * ((early + 1e-20) / (late + 1e-20)).log10()) as f32,
            rise_db: (db_power(peak) - db_power(background_power)) as f32,
        }
    }

    /// |rfft(x)[k]|², by direct DFT: only ~335 bins of one 50 ms window are ever needed.
    fn power(&self, x: &[f64], k: usize) -> f64 {
        let n = x.len();
        let (mut re, mut im) = (0.0, 0.0);
        for (i, v) in x.iter().enumerate() {
            let j = (k * i) % n;
            re += v * self.cos[j];
            im -= v * self.sin[j];
        }
        re * re + im * im
    }
}

/// Mean square of the channel sum (L + R), the quantity the Python envelope takes the root of.
pub fn block_power(left: &[f32], right: &[f32]) -> f64 {
    let sum: f64 = left.iter().zip(right).map(|(&l, &r)| f64::from(l + r).powi(2)).sum();
    sum / left.len().max(1) as f64
}

fn rms(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v * v, n + 1));
    (sum / n.max(1) as f64).sqrt()
}

/// shots_report.db: 20·log10 with amplitudes floored at 1e-12.
fn db_amplitude(x: f64) -> f64 {
    20.0 * x.max(1e-12).log10()
}

pub fn db_power(p: f64) -> f64 {
    10.0 * p.max(1e-24).log10()
}
