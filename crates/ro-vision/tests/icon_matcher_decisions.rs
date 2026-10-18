//! The matcher's gates, look-alike grouping and alignment search, on synthetic signatures.

mod common;

use std::cell::Cell;
use std::sync::Arc;

use common::{item, signature, N};
use ro_vision::items::{Catalogue, FixedSlotSampler, IconMatcher, IconSignature, SlotBackground, SlotSampler};

const DARK: SlotBackground = SlotBackground::new(0.0, 0.0, 0.0);

const NF: f32 = N as f32;

fn gradient() -> IconSignature {
    signature(|x, y| (x as f32 / NF, y as f32 / NF, (x + y) as f32 / (2.0 * NF)), 1.0)
}

fn bandage_icon() -> IconSignature {
    signature(|x, y| (y as f32 / NF, 1.0 - x as f32 / NF, 0.2), 1.0)
}

fn rifle_or_bandage() -> IconMatcher {
    IconMatcher::new(Catalogue::from([(item("rifle.ak").into(), gradient()), (item("bandage").into(), bandage_icon())]))
}

/// Four mutually orthogonal +/-1 patterns, so a blend's correlations are exactly its weights:
/// Walsh functions over the bits of the sample index, exactly balanced over the 3072 samples.
fn blend(weights: &[f64]) -> IconSignature {
    let rgb = (0..N * N * 3)
        .map(|i| {
            let sum: f64 = weights.iter().enumerate().map(|(bit, w)| if (i >> bit) & 1 == 0 { *w } else { -w }).sum();
            (0.5 + 0.3 * sum) as f32
        })
        .collect();

    IconSignature::new(rgb, vec![1.0; N * N])
}

#[test]
fn a_loose_look_alike_rescues_a_slot_that_would_otherwise_abstain() {
    // The slot leans 0.95 toward `winner`, and `cousin` correlates 0.80 with it — too far apart to
    // group strictly, close enough that the margin gate rejects the read outright.
    let catalogue = || {
        Catalogue::from([
            (item("ammo.shotgun.slug").into(), blend(&[1.0])),
            (item("ammo.shotgun.fire").into(), blend(&[0.8, 0.6])),
            (item("rock").into(), blend(&[0.0, 0.0, 1.0])),
        ])
    };
    let slot = blend(&[0.95, 0.3122]);

    let mut strict = IconMatcher::new(catalogue());
    strict.ambiguous_at = None;
    assert!(strict.identify(&slot, DARK, None).is_none());

    let found = IconMatcher::new(catalogue()).identify(&slot, DARK, None).expect("the loose pass answers");
    let named: Vec<&str> = found.indistinguishable.iter().chain([&found.item]).map(|i| i.short_name.as_str()).collect();

    assert!(named.contains(&"ammo.shotgun.slug"));
    assert!(named.contains(&"ammo.shotgun.fire"));
}

#[test]
fn a_look_alike_set_whose_members_resemble_only_the_winner_is_not_an_answer() {
    // A degenerate leader resembles several unrelated items without any of them resembling each
    // other. `cousin` and `stranger` correlate 0.704, so mutual grouping keeps `stranger` as the
    // runner-up and the slot stays honest.
    let catalogue = || {
        Catalogue::from([
            (item("water").into(), blend(&[1.0])),
            (item("tshirt.long").into(), blend(&[0.8, 0.6])),
            (item("bucket.water").into(), blend(&[0.88, 0.0, 0.475])),
            (item("rock").into(), blend(&[0.0, 0.0, 0.0, 1.0])),
        ])
    };
    let slot = blend(&[0.95, 0.3122]);

    let mut mutual = IconMatcher::new(catalogue());
    mutual.minimum_margin = 0.20;
    assert!(mutual.identify(&slot, DARK, None).is_none());

    let mut ungrouped = IconMatcher::new(catalogue());
    ungrouped.minimum_margin = 0.20;
    ungrouped.mutual_look_alikes = false;
    assert!(ungrouped.identify(&slot, DARK, None).is_some());
}

#[test]
fn a_close_runner_up_that_is_not_a_look_alike_still_abstains() {
    // The two candidates ramp along opposite axes, so they are uncorrelated and cannot be a
    // look-alike pair, while a diagonal ramp sits equidistant from both.
    let along_x = signature(|x, _| (x as f32 / NF, x as f32 / NF, x as f32 / NF), 1.0);
    let along_y = signature(|_, y| (y as f32 / NF, y as f32 / NF, y as f32 / NF), 1.0);
    let diagonal = signature(|x, y| {
        let v = (x + y) as f32 / (2.0 * NF);
        (v, v, v)
    }, 1.0);

    assert!(!along_x.is_indistinguishable_from(&along_y, IconSignature::INDISTINGUISHABLE_AT), "test needs two candidates that are not look-alikes");

    let matcher = IconMatcher::new(Catalogue::from([(item("rifle.ak").into(), along_x), (item("bandage").into(), along_y)]));
    let ranked = matcher.rank(&diagonal, DARK, None, None);

    assert!(ranked[0].score >= matcher.minimum_score, "test needs both candidates to clear the score gate");
    assert!(ranked[0].score - ranked[1].score < matcher.minimum_margin, "test needs two close candidates");
    assert!(matcher.identify(&diagonal, DARK, None).is_none());
}

/// A slot whose icon is drawn off from where the geometry looked: `at` returns the aligned view
/// only at the offset that undoes the error.
struct ShiftedSampler {
    misaligned: Arc<IconSignature>,
    aligned: Arc<IconSignature>,
    at_x: i32,
    at_y: i32,
    asked: Cell<u32>,
}

impl ShiftedSampler {
    fn new(misaligned: IconSignature, aligned: IconSignature, at_x: i32, at_y: i32) -> Self {
        Self { misaligned: Arc::new(misaligned), aligned: Arc::new(aligned), at_x, at_y, asked: Cell::new(0) }
    }
}

impl SlotSampler for ShiftedSampler {
    fn background(&self) -> SlotBackground {
        DARK
    }

    /// No surroundings to measure; these tests are about alignment, not emptiness.
    fn departure(&self) -> f64 {
        f64::INFINITY
    }

    fn scene(&self) -> Option<&[f32]> {
        None
    }

    fn at(&self, dx: i32, dy: i32, _d_size: i32) -> Arc<IconSignature> {
        self.asked.set(self.asked.get() + 1);
        Arc::clone(if dx == self.at_x && dy == self.at_y { &self.aligned } else { &self.misaligned })
    }
}

#[test]
fn alignment_is_searched_so_a_slightly_misplaced_icon_still_reads() {
    // Thin items are the reason this exists: a rifle a couple of pixels wide moves clean off
    // itself under a two pixel error, while a compact item barely notices.
    let matcher = rifle_or_bandage();

    // Mostly noise over a trace of the rifle: enough to stay the nominal leader — which steers the
    // alignment search — and far too little to clear the score gate.
    let misaligned = || {
        signature(
            |x, y| {
                let (r, g, b) = (x as f32 / NF, y as f32 / NF, (x + y) as f32 / (2.0 * NF));
                let nr = ((x * 7 + y * 3) % N) as f32 / NF;
                let ng = ((x * 3 + y * 7) % N) as f32 / NF;
                let nb = ((x + y) % 2) as f32 * 0.8;
                (0.4 * r + 0.6 * nr, 0.4 * g + 0.6 * ng, 0.4 * b + 0.6 * nb)
            },
            1.0,
        )
    };

    let sampler = ShiftedSampler::new(misaligned(), gradient(), 1, -2);

    assert!(matcher.identify(&misaligned(), DARK, None).is_none());
    assert_eq!(matcher.identify_sampler(&sampler, None, false).unwrap().item, item("rifle.ak"));
    assert!(sampler.asked.get() > 1);
}

#[test]
fn a_sampler_that_cannot_move_matches_the_unrefined_result() {
    let matcher = rifle_or_bandage();

    let direct = matcher.identify(&gradient(), DARK, None).unwrap();
    let sampled = matcher.identify_sampler(&FixedSlotSampler::new(Arc::new(gradient()), DARK), None, false).unwrap();

    assert_eq!(direct.item, sampled.item);
    assert!((direct.score - sampled.score).abs() < 5e-10);
    assert_eq!(direct.indistinguishable, sampled.indistinguishable);
}

#[test]
fn the_alignment_is_chosen_by_the_leader_alone() {
    // The runner-up must not be able to buy the alignment. Measured on real captures:
    // microphonestand overtook rf.detonator that way and abstained only by luck.
    let matcher = rifle_or_bandage();

    // Nominal favours the rifle; one offset would hand the slot to the bandage outright.
    let sampler = ShiftedSampler::new(gradient(), bandage_icon(), 2, 2);

    assert_eq!(matcher.identify_sampler(&sampler, None, false).unwrap().item, item("rifle.ak"));
}

#[test]
fn the_coarse_shortlist_does_not_change_the_answer() {
    // The 8x8 pre-pass may reorder the shortlist; it may not change what comes out.
    let catalogue = || {
        let mut catalogue = Catalogue::new();
        for i in 0..40 {
            let shift = i as f32 * 0.05;
            let icon = signature(move |x, y| ((x as f32 + shift) / NF, (y as f32 + 2.0 * shift) / NF, (x + y) as f32 / (2.0 * NF)), 1.0);
            catalogue.insert(item(&format!("item{i}")).into(), icon);
        }
        catalogue.insert(item("target").into(), bandage_icon());
        catalogue
    };

    let observed = bandage_icon();

    let mut full = IconMatcher::new(catalogue());
    full.coarse_shortlist = 0;
    let mut coarse = IconMatcher::new(catalogue());
    coarse.coarse_shortlist = 8;

    let from_full = full.identify(&observed, DARK, None).unwrap();
    let from_coarse = coarse.identify(&observed, DARK, None).unwrap();

    assert_eq!(from_full.item, item("target"));
    assert_eq!(from_coarse.item, item("target"));
    assert!((from_full.score - from_coarse.score).abs() < 5e-10);
}

#[test]
fn abstains_when_nothing_scores_well_enough() {
    let matcher = IconMatcher::new(Catalogue::from([(item("bandage").into(), bandage_icon())]));
    let unrelated = signature(|x, y| (1.0 - y as f32 / NF, x as f32 / NF, 0.9), 1.0);

    assert!(matcher.identify(&unrelated, DARK, None).is_none());
}
