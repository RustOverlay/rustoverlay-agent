//! Expectations come from the captures listed in docs/hud-layout.md. Tolerance is 2px — the model
//! is a search hint that the readers refine, not an exact placement.

use ro_vision::hud::{InventoryGeometry, SlotRect};

const TOLERANCE: f64 = 2.0;

fn near(expected: f64, actual: i32) {
    let actual = f64::from(actual);
    assert!((expected - TOLERANCE..=expected + TOLERANCE).contains(&actual), "expected {expected} +/- {TOLERANCE}, got {actual}");
}

fn geometry(width: i32, height: i32) -> InventoryGeometry {
    InventoryGeometry::new(width, height, 1.0)
}

#[test]
fn reproduces_the_measured_belt_at_1440x1080() {
    let g = geometry(1440, 1080);

    near(495.0, g.belt()[0].x);
    near(72.0, g.belt()[1].x - g.belt()[0].x);
    near(68.0, g.belt()[0].size);
    near(20.0, 1079 - g.belt()[0].bottom());
}

#[test]
fn reproduces_the_measured_belt_at_1600x900() {
    let g = geometry(1600, 900);

    near(549.0, g.belt()[0].x);
    near(80.0, g.belt()[1].x - g.belt()[0].x);
    near(22.0, 899 - g.belt()[0].bottom());
}

#[test]
fn height_does_not_affect_geometry_below_sixteen_by_nine() {
    // 1600x900 and 1600x1024 were captured back to back and measured identical. Both are 16:9 or
    // narrower, so width is the limiting axis in each and height cannot enter.
    let shorter = geometry(1600, 900);
    let taller = geometry(1600, 1024);

    for i in 0..InventoryGeometry::BELT_SLOTS {
        assert_eq!(shorter.belt()[i].x, taller.belt()[i].x);
        assert_eq!(shorter.belt()[i].size, taller.belt()[i].size);
        assert_eq!(899 - shorter.belt()[i].bottom(), 1023 - taller.belt()[i].bottom());
    }
}

#[test]
fn scales_between_widths() {
    let narrow = geometry(1440, 1080);
    let wide = geometry(1600, 900);

    near(f64::from(narrow.belt()[0].x) * (1600.0 / 1440.0), wide.belt()[0].x);
    near(f64::from(narrow.belt()[1].x - narrow.belt()[0].x) * (1600.0 / 1440.0), wide.belt()[1].x - wide.belt()[0].x);
}

#[test]
fn wear_slots_use_a_smaller_pitch_than_the_other_containers() {
    let g = geometry(1440, 1080);

    let belt_pitch = g.belt()[1].x - g.belt()[0].x;
    let wear_pitch = g.wear()[1].x - g.wear()[0].x;

    near(72.0, belt_pitch);
    near(61.0, wear_pitch);
    assert!(wear_pitch < belt_pitch);
}

#[test]
fn icon_area_is_inset_and_sits_above_the_slot_centre() {
    // A 68px slot is the corpus resolution, where the swept optimum is a 55px icon 3px down.
    let slot = SlotRect::new(495, 992, 68);
    let icon = slot.icon_area();

    assert_eq!(icon.size, 55);
    assert_eq!(icon.x, 501);
    assert_eq!(icon.y, 995);

    // Above centre, which is the part a symmetric inset would miss.
    assert!(icon.y + icon.size / 2 < slot.y + slot.size / 2);
}

#[test]
fn icon_area_scales_with_the_slot() {
    let small = SlotRect::new(0, 0, 68).icon_area();
    let large = SlotRect::new(0, 0, 91).icon_area();

    assert!((f64::from(small.size) - 68.0 * 0.806).abs() <= 1.0);
    assert!((f64::from(large.size) - 91.0 * 0.806).abs() <= 1.0);
}

#[test]
fn pitch_follows_the_smaller_axis_ratio() {
    // CanvasScaler is ScaleWithScreenSize/Expand against 1280x720, so the scale is the smaller of
    // the two ratios. Scaling by width alone overshoots on anything wider than 16:9.
    for (width, height, pitch) in [
        (2560, 1080, 96),  // 21:9 — height limits, width alone would give 128
        (3440, 1440, 128), // 21:9 at a larger size
        (5120, 1440, 128), // 32:9
        (1920, 1080, 96),  // 16:9 — both axes agree
        (1440, 1080, 72),  // 4:3 — width limits, as before
    ] {
        let g = geometry(width, height);

        assert_eq!(g.belt()[1].x - g.belt()[0].x, pitch, "{width}x{height}");
    }
}

#[test]
fn ui_scale_multiplies_the_pitch() {
    let full = InventoryGeometry::new(1920, 1080, 1.0);
    let half = InventoryGeometry::new(1920, 1080, 0.5);

    assert_eq!(full.belt()[1].x - full.belt()[0].x, 96);
    assert_eq!(half.belt()[1].x - half.belt()[0].x, 48);
}

#[test]
fn every_container_is_anchored_to_the_bottom_not_the_top() {
    // Equal height, different width: an offset from the top and one from the bottom disagree,
    // which is the bug this guards — at 1920x1080 the main grid missed every slot.
    let narrow = geometry(1440, 1080);
    let wide = geometry(1920, 1080);

    for (n, w) in [
        (narrow.belt()[0], wide.belt()[0]),
        (narrow.main()[0], wide.main()[0]),
        (narrow.wear()[0], wide.wear()[0]),
    ] {
        near(f64::from(1079 - n.y) / 0.75, 1079 - w.y);
    }
}

#[test]
fn every_slot_lands_inside_the_frame_at_reference_width() {
    // The wear row's offset used to be expressed from the top as 1189, past the bottom edge.
    let g = geometry(1920, 1080);

    for slot in g.belt().iter().chain(g.main()).chain(g.wear()) {
        assert!((0..=1079).contains(&slot.y));
        assert!((0..=1079).contains(&slot.bottom()));
        assert!((0..=1919).contains(&slot.x));
        assert!((0..=1919).contains(&slot.right()));
    }
}

#[test]
fn main_grid_is_six_by_four_in_row_major_order() {
    let g = geometry(1440, 1080);

    assert_eq!(g.main().len(), 24);
    assert_eq!(g.main()[0].y, g.main()[5].y);
    assert!(g.main()[6].y > g.main()[0].y);
    near(72.0, g.main()[6].y - g.main()[0].y);
}
