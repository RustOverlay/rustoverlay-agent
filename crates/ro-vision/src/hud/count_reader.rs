use std::collections::HashMap;
use std::sync::Arc;

use super::{luminance, FrameBuffer, GlyphAtlas, GlyphSet, GlyphTemplate, SlotRect};

/// The number drawn in a slot's lower edge. `is_stack` separates the two kinds: a stack is written
/// `x104`, a magazine's remaining ammo is written `24`, and the prefix is the only thing that tells
/// them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SlotCount {
    pub value: i32,
    pub is_stack: bool,
}

impl SlotCount {
    pub const fn new(value: i32, is_stack: bool) -> Self {
        Self { value, is_stack }
    }
}

/// Fractions of slot size bounding the text; see docs/hud-layout.md.
const LEFT_INSET: f64 = 0.08;
const RIGHT_INSET: f64 = 0.04;
const TOP_INSET: f64 = 0.26;
const BOTTOM_INSET: f64 = 0.06;

/// Digit height as a fraction of slot size. Searched by +/-1px around this.
const DIGIT_HEIGHT: f64 = 0.176;

/// Columns of the right margin the last glyph may end in.
const RIGHT_MARGIN: i32 = 6;

/// Longest count expected, as a guard against walking off into the icon.
const MAX_LENGTH: usize = 5;

/// Reads a slot's count by sliding rendered glyphs along the text, right to left.
///
/// Never segments. Splitting the text into glyphs first and classifying them second returned the
/// wrong glyph count on 11 of 24 labelled strips, and no classifier recovers from that.
/// Correlating the font against raw luminance instead reads 24 of 24 while staying silent on all
/// 74 empty slots; see docs/item-matching.md, which also records how each gate below was measured.
pub struct CountReader {
    atlas: Arc<dyn GlyphAtlas>,
    /// How well the last glyph must read before a count is reported at all — the gate deciding
    /// whether the slot has text, so it sits where an empty slot never clears it.
    pub minimum_score: f64,
    /// How well each further glyph must read once the text is known to exist. Deliberately below
    /// `minimum_score`: reusing that gate turns `x16` into `6`.
    pub continuation_score: f64,
    /// How well the leading 'x' must read. Below 0.46 the crossbow's empty magazine reads `x0`.
    pub prefix_score: f64,
}

impl CountReader {
    pub fn new(atlas: Arc<dyn GlyphAtlas>) -> Self {
        Self { atlas, minimum_score: 0.64, continuation_score: 0.50, prefix_score: 0.66 }
    }

    /// None when the slot carries no count, or when it cannot be read confidently.
    pub fn read<F: FrameBuffer + ?Sized>(&self, frame: &F, slot: SlotRect) -> Option<SlotCount> {
        let size = f64::from(slot.size);
        let x0 = 0.max(slot.x + (size * LEFT_INSET) as i32);
        let x1 = (frame.width() - 1).min(slot.right() - (size * RIGHT_INSET) as i32);
        let y0 = 0.max(slot.bottom() - (size * TOP_INSET) as i32);
        let y1 = (frame.height() - 1).min(slot.bottom() - (size * BOTTOM_INSET) as i32);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }

        let w = x1 - x0 + 1;
        let h = y1 - y0 + 1;
        let mut band = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            band.extend((0..w).map(|x| luminance(frame.pixel(x0 + x, y0 + y))));
        }

        let text = self.read_band(&band, w, h, (size * DIGIT_HEIGHT).round_ties_even() as i32)?;
        let digits = text.strip_prefix('x');

        digits
            .unwrap_or(&text)
            .parse()
            .ok()
            .map(|value| SlotCount::new(value, digits.is_some()))
    }

    fn read_band(&self, band: &[f64], w: i32, h: i32, nominal: i32) -> Option<String> {
        let mut atlases: HashMap<i32, Arc<GlyphSet>> = HashMap::new();

        // Height and baseline are searched, not measured (a mask read 12px text as 11-14); the anchor fixes both.
        let mut anchor = Anchor { ch: '\0', score: f64::NEG_INFINITY, start: 0, height: 0, baseline: 0 };

        for edge in (0.max(w - RIGHT_MARGIN)..w).rev() {
            for height in nominal - 1..=nominal + 1 {
                if height < 5 || height > h {
                    continue;
                }
                let glyphs = atlases.entry(height).or_insert_with(|| self.atlas.at(height)).clone();

                for baseline in height - 1..h {
                    // A count never ends in 'x', so the anchor is always a digit.
                    for (ch, glyph) in glyphs.iter().filter(|(ch, _)| *ch != 'x') {
                        let start = edge - glyph.width + 1;
                        let score = correlate(band, w, start, baseline - glyph.height + 1, glyph);
                        if score > anchor.score {
                            anchor = Anchor { ch, score, start, height, baseline };
                        }
                    }
                }
            }
        }

        if anchor.score < self.minimum_score {
            return None;
        }

        let atlas_at = &atlases[&anchor.height];
        let mut text = anchor.ch.to_string();
        let mut leftmost = anchor.start;

        while text.len() < MAX_LENGTH && leftmost >= 1 {
            let (mut best_ch, mut best_score, mut best_start) = ('\0', f64::NEG_INFINITY, 0);

            for edge in [leftmost - 1, leftmost - 2] {
                for (ch, glyph) in atlas_at.iter().filter(|(ch, _)| *ch != 'x') {
                    let start = edge - glyph.width + 1;
                    let score = correlate(band, w, start, anchor.baseline - glyph.height + 1, glyph);
                    if score > best_score {
                        (best_ch, best_score, best_start) = (ch, score, start);
                    }
                }
            }

            if best_score < self.continuation_score {
                break;
            }

            text.insert(0, best_ch);
            leftmost = best_start;
        }

        let Some(prefix) = atlas_at.get('x') else { return Some(text) };

        let prefix_score = [leftmost - 1, leftmost - 2]
            .into_iter()
            .map(|edge| correlate(band, w, edge - prefix.width + 1, anchor.baseline - prefix.height + 1, prefix))
            .fold(f64::NEG_INFINITY, f64::max);

        Some(if prefix_score >= self.prefix_score { format!("x{text}") } else { text })
    }
}

struct Anchor {
    ch: char,
    score: f64,
    start: i32,
    height: i32,
    baseline: i32,
}

/// Correlation of a template against the band at one position. Zero if it does not fit.
fn correlate(band: &[f64], w: i32, x: i32, y: i32, glyph: &GlyphTemplate) -> f64 {
    let (cell, gw, gh) = (&glyph.cell, glyph.width, glyph.height);
    if x < 0 || y < 0 || x + gw > w || ((y + gh) * w) as usize > band.len() {
        return 0.0;
    }

    let (w, x, y, gw, gh) = (w as usize, x as usize, y as usize, gw as usize, gh as usize);
    let rows = || (0..gh).map(move |j| (&band[(y + j) * w + x..][..gw], &cell[j * gw..][..gw]));

    let (mut mean_band, mut mean_cell) = (0.0, 0.0);
    for (b, c) in rows() {
        for (bv, cv) in b.iter().zip(c) {
            mean_band += bv;
            mean_cell += cv;
        }
    }

    let n = (gw * gh) as f64;
    mean_band /= n;
    mean_cell /= n;

    let (mut cov, mut var_band, mut var_cell) = (0.0, 0.0, 0.0);
    for (b, c) in rows() {
        for (bv, cv) in b.iter().zip(c) {
            let a = bv - mean_band;
            let b = cv - mean_cell;
            cov += a * b;
            var_band += a * a;
            var_cell += b * b;
        }
    }

    if var_band <= 1e-9 || var_cell <= 1e-9 { 0.0 } else { cov / (var_band * var_cell).sqrt() }
}
