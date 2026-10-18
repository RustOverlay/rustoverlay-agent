use ro_core::capture::ScreenRect;
use ro_core::capture::output_selection::{crop, display_for};

/// A 4K primary with a 1080p panel to its left, which puts the secondary at negative x.
const DISPLAYS: [ScreenRect; 2] = [ScreenRect::new(0, 0, 3840, 2160), ScreenRect::new(-1920, 0, 1920, 1080)];

#[test]
fn a_window_on_the_primary_display_is_found() {
    assert_eq!(display_for(&DISPLAYS, ScreenRect::new(100, 100, 1600, 900)), Some(0));
}

#[test]
fn a_window_at_negative_coordinates_is_found() {
    assert_eq!(display_for(&DISPLAYS, ScreenRect::new(-1800, 40, 1280, 720)), Some(1));
}

#[test]
fn a_window_filling_a_display_exactly_is_found() {
    assert_eq!(display_for(&DISPLAYS, ScreenRect::new(-1920, 0, 1920, 1080)), Some(1));
}

#[test]
fn a_window_straddling_two_displays_is_refused() {
    // Rather than cropping the visible part: a short frame would put every HUD offset in the
    // wrong place and the matcher would answer confidently from the wrong pixels.
    assert_eq!(display_for(&DISPLAYS, ScreenRect::new(-200, 100, 800, 600)), None);
}

#[test]
fn a_window_hanging_off_the_bottom_is_refused() {
    assert_eq!(display_for(&DISPLAYS, ScreenRect::new(-1900, 800, 400, 400)), None);
}

#[test]
fn a_collapsed_window_is_refused() {
    assert_eq!(display_for(&DISPLAYS, ScreenRect::new(100, 100, 0, 0)), None);
}

#[test]
fn no_displays_means_no_answer() {
    assert_eq!(display_for(&[], ScreenRect::new(0, 0, 800, 600)), None);
}

#[test]
fn the_crop_is_relative_to_the_display_origin() {
    assert_eq!(crop(DISPLAYS[1], ScreenRect::new(-1800, 40, 1280, 720)), ScreenRect::new(120, 40, 1280, 720));
}

#[test]
fn the_crop_of_a_window_at_the_display_origin_is_zero() {
    assert_eq!(crop(DISPLAYS[1], ScreenRect::new(-1920, 0, 1920, 1080)), ScreenRect::new(0, 0, 1920, 1080));
}
