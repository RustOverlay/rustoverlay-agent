//! Synthetic signatures covering the matcher's scoring and ranking. Corpus-wide accuracy is
//! measured against the real install separately — the icons are Facepunch's and are not vendored.
//! The gate and alignment cases are in icon_matcher_decisions.rs.

mod common;

use std::path::PathBuf;

use common::{item, signature, N};
use ro_vision::items::{Catalogue, IconFace, IconMatcher, IconSignature, ItemDefinition, SlotBackground};

/// Black: compositing a fully opaque icon over it is a no-op, so it isolates the metric.
const DARK: SlotBackground = SlotBackground::new(0.0, 0.0, 0.0);

const NF: f32 = N as f32;

fn gradient(shift: f32) -> IconSignature {
    signature(|x, y| ((x as f32 + shift) / NF, y as f32 / NF, (x + y) as f32 / (2.0 * NF)), 1.0)
}

fn bandage_icon() -> IconSignature {
    signature(|x, y| (y as f32 / NF, 1.0 - x as f32 / NF, 0.2), 1.0)
}

fn face(short_name: &str, path: &str) -> IconFace {
    IconFace::new(item(short_name), Some(PathBuf::from(path)))
}

#[test]
fn an_icon_matches_itself_perfectly() {
    assert!((gradient(0.0).similarity_to(&gradient(0.0)) - 1.0).abs() < 5e-7);
}

#[test]
fn uniform_brightness_changes_do_not_sink_the_score() {
    // The rendered icon can be dimmed or tinted by the slot; correlation is mean and scale
    // invariant so that must not matter.
    let dimmed = signature(
        |x, y| {
            let (r, g, b) = (x as f32 / NF, y as f32 / NF, (x + y) as f32 / (2.0 * NF));
            (r * 0.6 + 0.1, g * 0.6 + 0.1, b * 0.6 + 0.1)
        },
        1.0,
    );

    assert!(gradient(0.0).similarity_to(&dimmed) > 0.99);
}

#[test]
fn a_flat_patch_scores_zero_rather_than_matching_everything() {
    let flat = signature(|_, _| (0.5, 0.5, 0.5), 1.0);

    assert_eq!(flat.similarity_to(&gradient(0.0)), 0.0);
}

#[test]
fn transparent_cells_are_excluded_from_comparison() {
    let transparent = signature(|x, y| (x as f32 / NF, y as f32 / NF, 0.0), 0.1);

    // Nothing opaque on one side means nothing to compare, so no score rather than a lucky one.
    assert_eq!(gradient(0.0).similarity_to(&transparent), 0.0);
}

#[test]
fn identifies_the_best_candidate() {
    let ak = item("rifle.ak");
    let matcher = IconMatcher::new(Catalogue::from([(ak.clone().into(), gradient(0.0)), (item("bandage").into(), bandage_icon())]));

    assert_eq!(matcher.identify(&gradient(0.0), DARK, None).unwrap().item, ak);
}

#[test]
fn a_winning_skin_face_must_clear_the_skin_gate() {
    let ak = item("rifle.ak");
    let skin = face("rifle.ak", "workshop/123/icon.png");
    assert!(skin.is_skin());
    let mut matcher = IconMatcher::new(Catalogue::from([(skin, gradient(0.0)), (item("bandage").into(), bandage_icon())]));

    assert_eq!(matcher.identify(&gradient(0.0), DARK, None).unwrap().item, ak);
    matcher.minimum_skin_score = 1.5;
    assert!(matcher.identify(&gradient(0.0), DARK, None).is_none());
}

#[test]
fn the_skin_gate_leaves_an_items_own_icon_alone() {
    let mut matcher =
        IconMatcher::new(Catalogue::from([(item("rifle.ak").into(), gradient(0.0)), (item("bandage").into(), bandage_icon())]));
    matcher.minimum_skin_score = 1.5;

    assert!(matcher.identify(&gradient(0.0), DARK, None).is_some());
}

#[test]
fn the_coarse_shortlist_grows_with_the_catalogue() {
    // A flat count makes the shortlist shallower exactly as the catalogue grows, which is how adding
    // skin faces cost 90 correct reads over 1500 recorded slots.
    let small = IconMatcher::new(Catalogue::from([(item("rifle.ak").into(), gradient(0.0))]));
    let large = IconMatcher::new((0..4000).map(|i| (item(&format!("item{i}")).into(), gradient(0.0))).collect());

    assert_eq!(small.coarse_shortlist, 64);
    assert_eq!(large.coarse_shortlist, 1600);
}

#[test]
fn two_faces_of_one_item_are_one_answer_rather_than_an_ambiguity() {
    // A skinned AK and a vanilla AK are the same item repainted. Treated as rivals they take the
    // margin from each other and the slot abstains.
    let matcher = IconMatcher::new(Catalogue::from([
        (face("rifle.ak", "rifle.ak.png"), gradient(0.0)),
        (face("rifle.ak", "skins/1199995954/icon.png"), gradient(0.001)),
    ]));

    let found = matcher.identify(&gradient(0.0), DARK, None).expect("the AK is read");

    assert_eq!(found.item, item("rifle.ak"));
    assert!(found.indistinguishable.is_empty());
}

#[test]
fn a_skinned_catalogue_still_abstains_when_two_items_are_too_close() {
    // The same-item skip must not become a way past the margin gate: a genuine rival is still a
    // rival however many faces the winner has.
    let along_x = || signature(|x, _| (x as f32 / NF, x as f32 / NF, x as f32 / NF), 1.0);
    let matcher = IconMatcher::new(Catalogue::from([
        (face("rifle.ak", "rifle.ak.png"), along_x()),
        (face("rifle.ak", "skins/1/icon.png"), along_x()),
        (item("bandage").into(), signature(|_, y| (y as f32 / NF, y as f32 / NF, y as f32 / NF), 1.0)),
    ]));

    let diagonal = signature(|x, y| {
        let v = (x + y) as f32 / (2.0 * NF);
        (v, v, v)
    }, 1.0);

    assert!(matcher.identify(&diagonal, DARK, None).is_none());
}

#[test]
fn the_ranking_names_each_item_once_however_many_faces_it_has() {
    let matcher = IconMatcher::new(Catalogue::from([
        (face("rifle.ak", "rifle.ak.png"), gradient(0.0)),
        (face("rifle.ak", "skins/1/icon.png"), gradient(0.001)),
        (face("rifle.ak", "skins/2/icon.png"), gradient(0.002)),
    ]));

    let ranked = matcher.rank(&gradient(0.0), DARK, None, None);

    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].item, item("rifle.ak"));
}

#[test]
fn look_alikes_are_reported_together_rather_than_suppressing_the_match() {
    // Near-duplicate icons genuinely exist — door.key and apartment.master_key are identical. The
    // slot really does hold one of them, so say so instead of abstaining.
    let matcher = IconMatcher::new(Catalogue::from([
        (item("door.key").into(), gradient(0.0)),
        (item("apartment.master_key").into(), gradient(0.001)),
    ]));

    let found = matcher.identify(&gradient(0.0), DARK, None).expect("one of the keys is read");
    // Which of two indistinguishable icons leads is rounding, not information.
    let mut both: Vec<&str> = std::iter::once(&found.item).chain(&found.indistinguishable).map(|i| i.short_name.as_str()).collect();
    both.sort_unstable();

    assert_eq!(both, ["apartment.master_key", "door.key"]);
}

#[test]
fn a_near_flat_patch_is_rejected_however_well_it_correlates() {
    // Correlation is scale invariant, so a barely-there gradient matches a real icon perfectly —
    // an empty wear slot was confidently reported as a pumpkin.
    let faint = signature(|x, y| (0.5 + x as f32 / (500.0 * NF), 0.5 + y as f32 / (500.0 * NF), 0.5), 1.0);
    let strong = signature(|x, y| (x as f32 / NF, y as f32 / NF, 0.5), 1.0);

    assert_eq!(faint.similarity_to(&strong), 0.0);
    assert_eq!(faint.similarity_to_composited(&strong, DARK), 0.0);
    assert!(strong.similarity_to(&strong) > 0.99, "a real icon must still match itself");
}

#[test]
fn a_slot_too_faint_to_be_the_icon_is_rejected_even_at_perfect_correlation() {
    // The empty-slot detector that works: the amount a faint echo had to be amplified gives it away.
    let icon = signature(|x, y| (x as f32 / NF, y as f32 / NF, (x + y) as f32 / (2.0 * NF)), 1.0);

    let faded = |by: f32| {
        signature(
            |x, y| {
                let (r, g, b) = (x as f32 / NF, y as f32 / NF, (x + y) as f32 / (2.0 * NF));
                (0.5 + by * (r - 0.5), 0.5 + by * (g - 0.5), 0.5 + by * (b - 0.5))
            },
            1.0,
        )
    };

    let one = |score: f64| (score - 1.0).abs() < 5e-7;
    assert!(one(faded(0.6).similarity_to(&icon)));
    assert!(one(faded(0.6).similarity_to_composited(&icon, DARK)));

    // A fifth of the structure the icon predicts is not that icon, however well it lines up.
    // Still above the absolute contrast floor, so this is the ratio doing the work.
    assert!(one(faded(0.2).similarity_to(&icon)));
    assert_eq!(faded(0.2).similarity_to_composited(&icon, DARK), 0.0);
}

#[test]
fn compositing_over_the_slot_background_beats_comparing_the_raw_icon() {
    // A half-transparent icon over a bright background: the slot shows the blend, so the raw icon
    // mispredicts it and the composited candidate does not.
    let background = SlotBackground::new(0.85, 0.85, 0.85);
    let icon = signature(|x, y| (x as f32 / NF, y as f32 / NF, 0.0), 0.5);
    let observed = signature(|x, y| (0.5 * x as f32 / NF + 0.425, 0.5 * y as f32 / NF + 0.425, 0.425), 1.0);

    assert!(observed.similarity_to_composited(&icon, background) > observed.similarity_to(&icon));
}

#[test]
fn candidate_filter_restricts_the_search() {
    let wearable = std::sync::Arc::new(ItemDefinition { is_wearable: true, ..(*item("metal.facemask")).clone() });
    let matcher = IconMatcher::new(Catalogue::from([(item("rifle.ak").into(), gradient(0.0)), (wearable.clone().into(), bandage_icon())]));

    let ranked = matcher.rank(&gradient(0.0), DARK, Some(&|i: &ItemDefinition| i.is_wearable), None);

    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].item, wearable);
}
