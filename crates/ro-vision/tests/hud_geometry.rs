//! Expectations are the measurements from real captures listed in docs/hud-layout.md, expressed as
//! distances from the frame edges. Tolerance is 2px: the model is a search hint, and rounding at
//! three different scales cannot land exactly on every one.

use ro_vision::hud::HudGeometry;

const TOLERANCE: f64 = 2.0;

fn near(expected: f64, actual: f64) {
    assert!((expected - TOLERANCE..=expected + TOLERANCE).contains(&actual), "expected {expected} +/- {TOLERANCE}, got {actual}");
}

#[test]
fn reproduces_the_measured_frames() {
    // frameWidth, frameHeight, uiScale, healthLeftFromRight, healthBottomFromBottom, barHeight
    for (width, height, ui_scale, left_from_right, bottom_from_bottom, bar_height) in
        [(1920, 1080, 1.0, 274, 113, 30), (1920, 1080, 0.5, 136, 56, 15), (1440, 1080, 1.0, 205, 84, 23)]
    {
        let hud = HudGeometry::new(width, height, ui_scale);

        near(f64::from(left_from_right), f64::from(width - 1 - hud.health.left_x));
        near(f64::from(bottom_from_bottom), f64::from(height - 1 - hud.health.bottom()));
        near(f64::from(bar_height), f64::from(hud.health.height));
    }
}

#[test]
fn places_the_hydration_row_one_pitch_below_health() {
    for (width, height, ui_scale, bottom_from_bottom) in [(1920, 1080, 1.0, 71), (1920, 1080, 0.5, 35), (1440, 1080, 1.0, 53)] {
        let hud = HudGeometry::new(width, height, ui_scale);

        near(f64::from(bottom_from_bottom), f64::from(height - 1 - hud.hydration.bottom()));
    }
}

#[test]
fn ui_scale_halves_every_offset_about_the_bottom_right_corner() {
    let full = HudGeometry::new(1920, 1080, 1.0);
    let half = HudGeometry::new(1920, 1080, 0.5);

    near(f64::from(1919 - full.health.left_x) / 2.0, f64::from(1919 - half.health.left_x));
    near(f64::from(1079 - full.health.bottom()) / 2.0, f64::from(1079 - half.health.bottom()));
    near(f64::from(full.health.height) / 2.0, f64::from(half.health.height));
}

#[test]
fn scales_with_frame_width_not_height() {
    let wide = HudGeometry::new(1920, 1080, 1.0);
    let narrow = HudGeometry::new(1440, 1080, 1.0);

    // Same height, so a height-driven model would put these in the same place.
    assert_ne!(1919 - wide.health.left_x, 1439 - narrow.health.left_x);
    assert!((narrow.scale / wide.scale - 0.75).abs() < 0.0005);
}

#[test]
fn rows_are_evenly_spaced() {
    let hud = HudGeometry::new(1920, 1080, 1.0);

    assert_eq!(hud.hydration.bottom() - hud.health.bottom(), hud.calories.bottom() - hud.hydration.bottom());
}
