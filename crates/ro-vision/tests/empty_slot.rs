//! An empty belt slot drawn over the world, which is a rectangle of landscape the matcher will
//! otherwise name. See `FrameSlotSampler::departure_from`.

mod common;

use std::sync::Arc;

use common::{opaque, N};
use ro_vision::hud::{FrameBuffer, Rgb, SlotRect};
use ro_vision::items::{Catalogue, FixedSlotSampler, FrameSlotSampler, IconMatcher, IconSignature, SlotBackground, SlotSampler};

const SLOT: SlotRect = SlotRect::new(20, 20, 68);

struct Canvas<P: Fn(i32, i32) -> Rgb>(P);

impl<P: Fn(i32, i32) -> Rgb> FrameBuffer for Canvas<P> {
    fn width(&self) -> i32 {
        200
    }

    fn height(&self) -> i32 {
        200
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        (self.0)(x, y)
    }
}

/// Terrain the gate is meant for: shaded and coloured, but smooth at slot scale — snow, sky, open
/// water. Busy terrain departs from its own edges honestly and is not what this catches.
fn scene(x: i32, y: i32) -> Rgb {
    let shade = 110.0 + 30.0 * (f64::from(x) / 70.0).sin() + 20.0 * (f64::from(y) / 55.0).cos() + f64::from(x) * 0.15;

    Rgb::new(clamp(shade), clamp(shade * 1.1), clamp(shade * 0.7))
}

fn clamp(v: f64) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

#[test]
fn a_slot_showing_only_smooth_scene_behind_it_barely_departs_from_it() {
    let departure = FrameSlotSampler::departure_from(&Canvas(scene), SLOT, SLOT.icon_area());

    assert!(departure < 0.039, "departure {departure:.4}");
}

#[test]
fn a_slot_holding_an_item_departs_from_the_scene_around_it() {
    let icon = SLOT.icon_area();

    let with_item = Canvas(|x, y| {
        if x >= icon.x + 8 && x <= icon.right() - 8 && y >= icon.y + 8 && y <= icon.bottom() - 8 {
            Rgb::new(220, 40, 30)
        } else {
            scene(x, y)
        }
    });

    let departure = FrameSlotSampler::departure_from(&with_item, SLOT, icon);

    assert!(departure >= 0.039, "departure {departure:.4}");
}

#[test]
fn an_icon_area_off_the_frame_never_gates_and_is_never_read() {
    let slot = SlotRect::new(160, 160, 68);
    let bounded = Canvas(|x, y| {
        assert!((0..200).contains(&x) && (0..200).contains(&y), "read ({x}, {y}) off the frame");
        scene(x, y)
    });

    assert_eq!(FrameSlotSampler::departure_from(&bounded, slot, slot.icon_area()), f64::INFINITY);
}

#[test]
fn the_gate_only_applies_to_slots_drawn_over_the_world() {
    let matcher = IconMatcher::new(Catalogue::new());
    let canvas = Canvas(scene);

    // Nothing in the catalogue, so both abstain — the point is that neither panics and the flag is
    // what selects the behaviour. The measured effect is in docs/item-matching.md.
    assert!(matcher.identify_sampler(&FrameSlotSampler::new(&canvas, SLOT), None, true).is_none());
    assert!(matcher.identify_sampler(&FrameSlotSampler::new(&canvas, SLOT), None, false).is_none());
}

#[test]
fn a_sampler_with_no_surroundings_to_measure_never_gates() {
    let blank = IconSignature::new(vec![0.0; N * N * 3], opaque());

    assert_eq!(FixedSlotSampler::new(Arc::new(blank), SlotBackground::default()).departure(), f64::INFINITY);
}

#[test]
fn scoring_against_the_predicted_scene_reads_what_the_item_adds_to_it() {
    // An empty slot adds nothing to its own backdrop, so it has no structure to correlate however
    // busy the backdrop is, while an item still does.
    let scene: Vec<f32> = (0..N * N * 3).map(|i| 0.2 + 0.6 * ((i / 3 % N) % 5) as f32 / 4.0).collect();

    let (candidate, at) = barred();
    let empty = IconSignature::new(scene.clone(), opaque());

    let mut composited = scene.clone();
    for cell in 0..N * N {
        for channel in 0..3 {
            composited[cell * 3 + channel] = at[cell * 3 + channel];
        }
    }

    let holding = IconSignature::new(composited, opaque());

    assert_eq!(empty.similarity_to_scene(&candidate, SlotBackground::default(), &scene), 0.0);
    assert!(holding.similarity_to_scene(&candidate, SlotBackground::default(), &scene) > 0.9);
}

/// An opaque icon of vertical bars, and the composite it would produce over any scene.
fn barred() -> (IconSignature, Vec<f32>) {
    let rgb: Vec<f32> = (0..N * N * 3).map(|i| if (i / 3) % N < N / 2 { 0.85 } else { 0.15 }).collect();

    (IconSignature::new(rgb.clone(), opaque()), rgb)
}
