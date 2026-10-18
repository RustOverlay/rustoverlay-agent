//! The selected belt slot, which Rust fills with flat blue behind the icon.
//!
//! Synthetic, and shaped after the real failure: a thin item over a strongly coloured background,
//! against a candidate whose icon fills the slot. Centring on one mean across all three channels
//! counted the shared background as agreement — a held assault rifle came back as `water`.

mod common;

use common::N;
use ro_vision::items::{Catalogue, IconMatcher, IconSignature, SlotBackground};

/// Measured off the selected belt slot of a real capture.
const HIGHLIGHT: SlotBackground = SlotBackground::new(30.0 / 255.0, 94.0 / 255.0, 141.0 / 255.0);

const PANEL: SlotBackground = SlotBackground::new(68.0 / 255.0, 63.0 / 255.0, 58.0 / 255.0);

/// A thin diagonal item covering a small share of the slot, as a rifle does. Shaded along its
/// length: a flat item has no variance to correlate on either metric.
fn thin() -> IconSignature {
    build(|x, y| if x.abs_diff(y) <= 1 { barrel(y) } else { (0.0, 0.0, 0.0, 0.0) })
}

fn barrel(y: usize) -> (f32, f32, f32, f32) {
    let shade = 0.4 + 0.5 * (y % 8) as f32 / 8.0;

    (0.75 * shade, 0.55 * shade, 0.30 * shade, 1.0)
}

/// An icon that fills the slot in a colour close to the highlight, as `water` does.
fn filler() -> IconSignature {
    build(|x, y| (0.15 + 0.1 * (x as f32 / N as f32), 0.40, 0.60 - 0.1 * (y as f32 / N as f32), 1.0))
}

/// What the slot shows: the thin icon already composited over `over`.
fn rendered(over: SlotBackground) -> IconSignature {
    build(|x, y| if x.abs_diff(y) <= 1 { barrel(y) } else { (over.r, over.g, over.b, 1.0) })
}

fn build(paint: impl Fn(usize, usize) -> (f32, f32, f32, f32)) -> IconSignature {
    let mut rgb = vec![0f32; N * N * 3];
    let mut alpha = vec![0f32; N * N];

    for y in 0..N {
        for x in 0..N {
            let (r, g, b, a) = paint(x, y);
            let cell = y * N + x;
            (rgb[cell * 3], rgb[cell * 3 + 1], rgb[cell * 3 + 2]) = (r, g, b);
            alpha[cell] = a;
        }
    }

    IconSignature::new(rgb, alpha)
}

fn the_thin_item_beats_a_slot_filling_lookalike(highlighted: bool) {
    let background = if highlighted { HIGHLIGHT } else { PANEL };
    let slot = rendered(background);

    let correct = slot.similarity_to_composited(&thin(), background);
    let filler = slot.similarity_to_composited(&filler(), background);

    // The margin gate, not the ranking, is what the highlight used to defeat: both candidates
    // scored in the eighties and the winner was rejected for being too close to call.
    assert!(
        correct - filler >= IconMatcher::new(Catalogue::new()).minimum_margin,
        "correct {correct:.3} vs filler {filler:.3} on {}",
        if highlighted { "highlight" } else { "panel" }
    );
}

#[test]
fn the_thin_item_beats_a_slot_filling_lookalike_on_the_panel() {
    the_thin_item_beats_a_slot_filling_lookalike(false);
}

#[test]
fn the_thin_item_beats_a_slot_filling_lookalike_on_the_highlight() {
    the_thin_item_beats_a_slot_filling_lookalike(true);
}

#[test]
fn the_highlight_does_not_flatten_the_spread_of_scores() {
    let lit = rendered(HIGHLIGHT);
    let plain = rendered(PANEL);

    let lit_spread = lit.similarity_to_composited(&thin(), HIGHLIGHT) - lit.similarity_to_composited(&filler(), HIGHLIGHT);
    let plain_spread = plain.similarity_to_composited(&thin(), PANEL) - plain.similarity_to_composited(&filler(), PANEL);

    // Half is generous; the point is that the coloured background no longer compresses the gap.
    assert!(lit_spread > plain_spread / 2.0, "highlight {lit_spread:.3} vs panel {plain_spread:.3}");
}
