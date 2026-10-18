//! The second-order statistics every `IconSignature` gate and the correlation itself derive from.

/// Below this spread across the compared cells there is no picture to recognise, and correlation —
/// being scale invariant — would happily match the faintest gradient to anything.
///
/// A backstop against a literally flat patch, and nothing more; it is not the empty-slot detector.
/// On a wide corpus empty slots reach 0.044 while filled ones start at 0.022, so no absolute floor
/// separates them. See `MINIMUM_STRUCTURE_RATIO` in icon_signature.rs, which does.
const MINIMUM_CONTRAST: f64 = 0.03;

/// Two passes, not one: accumulating raw sums and taking `Sx2 - Sx*Sx/n` subtracts two close large
/// numbers, and the structure ratio compares two of those results against each other.
#[derive(Clone, Copy, Debug)]
pub(super) struct Moments {
    pub covariance: f64,
    pub variance_a: f64,
    pub variance_b: f64,
    pub count: usize,
}

impl Moments {
    pub fn spread_a(&self) -> f64 {
        (self.variance_a / self.count as f64).sqrt()
    }

    pub fn spread_b(&self) -> f64 {
        (self.variance_b / self.count as f64).sqrt()
    }

    pub fn correlation(&self) -> f64 {
        if self.variance_a <= 1e-12 || self.variance_b <= 1e-12 {
            0.0
        } else {
            self.covariance / (self.variance_a * self.variance_b).sqrt()
        }
    }

    /// A flat patch has no structure to correlate; anything would score 1.
    pub fn has_contrast(&self) -> bool {
        self.spread_a() >= MINIMUM_CONTRAST && self.spread_b() >= MINIMUM_CONTRAST
    }
}

/// Second-order statistics, optionally centring each channel on its own mean rather than on one
/// mean across all three.
///
/// Samples are interleaved r,g,b, so a coloured background gives both vectors the same large
/// per-channel offset — covariance that exists whichever candidate is scored. Behind the selected
/// belt slot that drove every candidate to 0.65-0.83 and a held rifle lost to `water`. Applied
/// only to chromatic backgrounds because per-channel centring discards each channel's DC, which is
/// part of how same-silhouette icons in different colours are told apart. See docs/item-matching.md.
pub(super) fn measure(a: &[f32], b: &[f32], per_channel: bool) -> Moments {
    let len = a.len();
    let (mut sum_a, mut sum_b) = ([0.0f32; LANES], [0.0f32; LANES]);
    for_lanes(a, b, |lane, x, y| {
        sum_a[lane] += x;
        sum_b[lane] += y;
    });

    let (mut mean_a, mut mean_b) = ([0.0f32; 3], [0.0f32; 3]);
    for channel in 0..3 {
        let (of_a, of_b, count) = if per_channel {
            let lanes = (channel..LANES).step_by(3);
            (total(lanes.clone().map(|l| sum_a[l])), total(lanes.map(|l| sum_b[l])), (len + 2 - channel) / 3)
        } else {
            (total(sum_a), total(sum_b), len)
        };
        if count > 0 {
            (mean_a[channel], mean_b[channel]) = ((of_a / count as f64) as f32, (of_b / count as f64) as f32);
        }
    }

    let (mut covariance, mut variance_a, mut variance_b) = ([0.0f32; LANES], [0.0f32; LANES], [0.0f32; LANES]);
    for_lanes(a, b, |lane, x, y| {
        let da = x - mean_a[lane % 3];
        let db = y - mean_b[lane % 3];
        covariance[lane] += da * db;
        variance_a[lane] += da * da;
        variance_b[lane] += db * db;
    });

    Moments { covariance: total(covariance), variance_a: total(variance_a), variance_b: total(variance_b), count: len }
}

/// Accumulators per statistic: four cells of r,g,b. One running sum made every add wait on the
/// last; twelve independent ones overlap and vectorise. Chunks start on a cell, so lane `l`
/// always holds channel `l % 3`, the remainder included.
///
/// f32 in the lanes, f64 to combine them. Two passes keep every lane's sum small (deviations from
/// the mean, at most a few thousand samples), which is where f32 would lose precision; the
/// corpus score is unchanged by it.
const LANES: usize = 12;

fn for_lanes(a: &[f32], b: &[f32], mut visit: impl FnMut(usize, f32, f32)) {
    let ((chunks_a, rest_a), (chunks_b, rest_b)) = (a.as_chunks::<LANES>(), b.as_chunks::<LANES>());
    for (x, y) in chunks_a.iter().zip(chunks_b) {
        for lane in 0..LANES {
            visit(lane, x[lane], y[lane]);
        }
    }
    for (lane, (&x, &y)) in rest_a.iter().zip(rest_b).enumerate() {
        visit(lane, x, y);
    }
}

fn total(lanes: impl IntoIterator<Item = f32>) -> f64 {
    lanes.into_iter().map(f64::from).sum()
}

pub(super) fn correlation(a: &[f32], b: &[f32], gate: bool) -> f64 {
    let m = measure(a, b, false);

    if gate && !m.has_contrast() { 0.0 } else { m.correlation() }
}
