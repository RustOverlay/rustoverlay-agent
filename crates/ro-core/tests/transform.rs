mod common;

use chrono::{DateTime, TimeDelta, Utc};
use ro_core::correlation::TransformCorrelator;
use ro_core::events::{CameraTransform, TransformTriple};
use ro_core::logs::LogBlock;
use ro_core::parsing::transform_parser;

fn at() -> DateTime<Utc> {
    common::utc(2026, 8, 2, 21, 41, 41, 696)
}

fn block(message: &str, body: &[&str]) -> LogBlock {
    LogBlock::new(at(), "0x6e08", message, body.iter().map(|s| s.to_string()).collect())
}

fn triple(at: DateTime<Utc>, a: f64, b: f64, c: f64) -> TransformTriple {
    TransformTriple { timestamp: at, a, b, c, label: None }
}

#[test]
fn parses_the_bare_parenthesised_triple_the_console_actually_emits() {
    let triples = transform_parser::parse(&block("(-115.65, -1.04, 152.49)", &[]));
    let [triple] = triples.as_slice() else { panic!("expected one triple") };

    assert_eq!(triple.a, -115.65);
    assert_eq!(triple.b, -1.04);
    assert_eq!(triple.c, 152.49);
}

#[test]
fn ignores_convar_defaults_embedded_in_find_help_text() {
    // `find` prints a convar's default inside its description; an unanchored match turned these
    // into phantom transforms.
    let block = block(
        "found 3 matches",
        &[
            "client.camoffset (Generated) Position offset applied to the camera relative to the tracked bone ((0.00, 1.00, 0.00))",
            "debug.debugcamera_offset (Generated) World-space positional offset ((0.00, 0.00, 0.00))",
            "rgbeffects.colorcorrection_razer Controls how RGB values are mapped ((3.00, 3.00, 3.00))",
        ],
    );

    assert!(transform_parser::parse(&block).is_empty());
}

#[test]
fn pairs_a_position_followed_by_an_orientation() {
    let mut correlator = TransformCorrelator::default();

    assert!(correlator.accept(triple(at(), -115.65, -1.04, 152.49)).is_none());

    let transform = correlator.accept(triple(at() + TimeDelta::milliseconds(40), 8.02, 170.41, 0.00)).unwrap();

    assert_eq!(transform.x, -115.65);
    common::assert_close(170.41, transform.bearing_degrees(), 2);
    common::assert_close(-8.02, transform.pitch_degrees(), 2);
}

#[test]
fn does_not_pair_two_positions() {
    let mut correlator = TransformCorrelator::default();

    correlator.accept(triple(at(), 53.57, 15.10, -131.50));
    let result = correlator.accept(triple(at() + TimeDelta::milliseconds(40), 53.57, 15.10, -131.50));

    // A negative component cannot be an Euler angle, so this is a fresh position, not a pair.
    assert!(result.is_none());
}

#[test]
fn does_not_pair_across_a_long_gap() {
    let mut correlator = TransformCorrelator::default();

    correlator.accept(triple(at(), -115.65, -1.04, 152.49));
    let result = correlator.accept(triple(at() + TimeDelta::seconds(36), 8.02, 170.41, 0.00));

    assert!(result.is_none());
}

#[test]
fn raw_pitch_converts_to_signed_degrees_with_up_positive() {
    for (raw, expected) in [(351.98, 8.02), (8.02, -8.02), (0.0, 0.0)] {
        let transform = CameraTransform { timestamp: at(), x: 0.0, y: 0.0, z: 0.0, raw_pitch: raw, yaw: 0.0, roll: 0.0 };

        common::assert_close(expected, transform.pitch_degrees(), 2);
    }
}
