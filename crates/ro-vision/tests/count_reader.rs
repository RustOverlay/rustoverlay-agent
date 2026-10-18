//! Synthetic glyphs painted into a synthetic frame. Accuracy against the real font and real
//! captures is measured separately — that needs a Rust install and cannot be a committed test.

use std::sync::Arc;

use ro_vision::hud::{CountReader, FrameBuffer, GlyphAtlas, GlyphSet, GlyphTemplate, Rgb, SlotCount, SlotRect};

/// 1440x1080 belt dimensions, where the corpus was labelled.
const SLOT_SIZE: i32 = 68;
const DIGIT_HEIGHT: i32 = 12;

/// An alphabet of uncorrelated noise patterns. Each character matches itself exactly and nothing
/// else, which isolates the reader's anchoring and walking from any question of glyph shape.
struct FakeAtlas;

impl GlyphAtlas for FakeAtlas {
    fn at(&self, digit_height: i32) -> Arc<GlyphSet> {
        let mut glyphs = GlyphSet::new();

        for ch in "0123456789x".chars() {
            // 'x' is lower case, so it is shorter and narrower — the same relationship the real
            // font has, and the reason the reader tests for it separately.
            let (w, h) = if ch == 'x' { (4, digit_height * 2 / 3) } else { (5, digit_height) };
            glyphs.insert(ch, GlyphTemplate { cell: pattern(ch, w, h), width: w, height: h });
        }

        Arc::new(glyphs)
    }
}

fn pattern(ch: char, w: i32, h: i32) -> Vec<f64> {
    let mut seed = (ch as u32).wrapping_mul(2654435761);

    (0..w * h)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            if (seed >> 24).is_multiple_of(2) { 0.0 } else { 255.0 }
        })
        .collect()
}

struct Canvas {
    width: i32,
    height: i32,
    grey: Vec<u8>,
}

impl FrameBuffer for Canvas {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        let v = self.grey[(y * self.width + x) as usize];
        Rgb::new(v, v, v)
    }
}

impl Canvas {
    fn new(width: i32, height: i32, fill: u8) -> Self {
        Self { width, height, grey: vec![fill; (width * height) as usize] }
    }

    /// Paints a glyph with its bottom-right corner at (right, baseline), degraded towards noise.
    ///
    /// The noise is made orthogonal to the glyph and matched to its spread first, so the painted
    /// patch correlates with the template at exactly c/sqrt(c^2 + (1-c)^2).
    fn paint(&mut self, ch: char, right: i32, baseline: i32, clarity: f64) {
        let (w, h) = if ch == 'x' { (4, DIGIT_HEIGHT * 2 / 3) } else { (5, DIGIT_HEIGHT) };
        let cell = centred(pattern(ch, w, h));
        let mut noise = centred(pattern(char::from_u32(ch as u32 + 100).unwrap(), w, h));

        let along = dot(&noise, &cell) / dot(&cell, &cell);
        for (n, c) in noise.iter_mut().zip(&cell) {
            *n -= along * c;
        }

        let rescale = (dot(&cell, &cell) / dot(&noise, &noise)).sqrt();

        for j in 0..h {
            for i in 0..w {
                let k = (j * w + i) as usize;
                let value = clarity * cell[k] + (1.0 - clarity) * rescale * noise[k];
                let at = (baseline - h + 1 + j) * self.width + right - w + 1 + i;
                self.grey[at as usize] = (128.0 + 0.5 * value).clamp(0.0, 255.0) as u8;
            }
        }
    }

    fn scatter(&mut self, fill: u8) {
        let mut seed = 12345u32;
        for v in &mut self.grey {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *v = fill + ((seed >> 24) % 40) as u8;
        }
    }
}

fn centred(values: Vec<f64>) -> Vec<f64> {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values.into_iter().map(|v| v - mean).collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A slot whose text band is entirely inside the canvas, at 1440x1080 belt geometry.
fn blank() -> (Canvas, SlotRect) {
    (Canvas::new(200, 200, 60), SlotRect::new(20, 100, SLOT_SIZE))
}

/// Right edge of the count text, two columns inside the band's right edge.
fn text_right(slot: SlotRect) -> i32 {
    slot.right() - (f64::from(slot.size) * 0.04) as i32 - 2
}

fn baseline(slot: SlotRect) -> i32 {
    slot.bottom() - (f64::from(slot.size) * 0.06) as i32 - 1
}

fn reader() -> CountReader {
    CountReader::new(Arc::new(FakeAtlas))
}

#[test]
fn reads_a_multi_digit_ammo_count() {
    let (mut frame, slot) = blank();
    let right = text_right(slot);

    frame.paint('4', right, baseline(slot), 1.0);
    frame.paint('2', right - 5, baseline(slot), 1.0);

    assert_eq!(reader().read(&frame, slot), Some(SlotCount::new(24, false)));
}

#[test]
fn a_leading_x_makes_it_a_stack_rather_than_an_ammo_count() {
    // The prefix is the only thing separating the two: 24 rounds loaded versus 24 items held.
    let (mut frame, slot) = blank();
    let right = text_right(slot);

    frame.paint('4', right, baseline(slot), 1.0);
    frame.paint('2', right - 5, baseline(slot), 1.0);
    frame.paint('x', right - 10, baseline(slot), 1.0);

    assert_eq!(reader().read(&frame, slot), Some(SlotCount::new(24, true)));
}

#[test]
fn a_slot_with_no_count_reads_nothing() {
    let (mut frame, slot) = blank();
    frame.scatter(60);

    assert_eq!(reader().read(&frame, slot), None);
}

#[test]
fn a_faint_leading_digit_is_kept_rather_than_truncating_the_number() {
    // Why the continuation gate sits below the anchor gate: reusing the anchor's threshold stops
    // the walk on a faint digit and turns 16 into 6. Measured on real captures: x16 read as 6.
    let (mut frame, slot) = blank();
    let right = text_right(slot);

    // At clarity c a blend correlates at c/sqrt(c^2 + (1-c)^2), so 0.40 sits at 0.55 — between
    // the two gates under test.
    frame.paint('6', right, baseline(slot), 1.0);
    frame.paint('1', right - 5, baseline(slot), 0.40);

    let mut lenient = reader();
    lenient.continuation_score = 0.50;
    let mut strict = reader();
    strict.continuation_score = 0.64;

    assert_eq!(lenient.read(&frame, slot), Some(SlotCount::new(16, false)));
    assert_eq!(strict.read(&frame, slot), Some(SlotCount::new(6, false)));
}
