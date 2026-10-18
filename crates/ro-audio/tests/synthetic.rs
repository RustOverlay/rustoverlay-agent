use ro_audio::{AudioBlock, AudioProfile, Locator};

const RATE: u32 = 48_000;
const START_UTC_100NS: i64 = 17_922_366_000_000_000;

/// Deterministic white noise in [-1, 1].
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

/// 2 s of −60 dBFS noise with a decaying burst at 1.0 s, panned so L is 20 dB over R. The burst is
/// low-passed: white noise is brighter than any report and the locator drops it as a flyby.
fn panned_click() -> Vec<f32> {
    let mut noise = Noise(7);
    let total = 2 * RATE as usize;
    let at = RATE as usize;
    let mut samples = Vec::with_capacity(total * 2);
    let mut low = 0.0;
    for i in 0..total {
        let floor = 0.001 * noise.next();
        low = 0.95 * low + 0.05 * noise.next();
        let burst = if i >= at { 2.0 * (-((i - at) as f32) / 2400.0).exp() * low } else { 0.0 };
        samples.push(floor + burst);
        samples.push(floor * 0.9 + burst * 0.1);
    }
    samples
}

#[test]
fn a_panned_click_over_noise_is_detected_once_on_the_left() {
    let samples = panned_click();
    let mut locator = Locator::new(AudioProfile::builtin());
    let block_frames = 480;

    let found: Vec<_> = samples
        .chunks(block_frames * 2)
        .enumerate()
        .flat_map(|(i, chunk)| {
            let utc_100ns = START_UTC_100NS + (i * block_frames) as i64 * 10_000_000 / i64::from(RATE);
            locator.push(&AudioBlock { utc_100ns, rate: RATE, samples: chunk.to_vec() })
        })
        .collect();

    assert_eq!(found.len(), 1, "one onset, no retriggers in the decay");
    let shot = &found[0];
    assert!(shot.detection.onset_frame.abs_diff(u64::from(RATE)) <= 48, "onset at {}", shot.detection.onset_frame);
    assert_eq!(shot.estimate.utc_ms, START_UTC_100NS / 10_000 + 1_000);
    assert!((shot.estimate.ild_db - 20.0).abs() < 1.5, "ILD {}", shot.estimate.ild_db);
    assert!(shot.estimate.rel_bearing_deg[0] < 0.0, "left is negative: {:?}", shot.estimate.rel_bearing_deg);
    assert!(shot.detection.features.rise_db > 30.0);
}
