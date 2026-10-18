use std::sync::Arc;

/// One rendered character, as greyscale, occupying the font's full advance width.
///
/// Advance width rather than the ink's bounding box, deliberately. A tight box makes '1' a three
/// pixel stroke that correlates with any vertical edge, and leaves the distance to the next glyph
/// undefined; advances abut, so the next glyph always starts one pixel left of this one.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphTemplate {
    pub cell: Vec<f64>,
    pub width: i32,
    pub height: i32,
}

/// The glyphs of one size, in insertion order.
///
/// Ordered because [`crate::hud::CountReader`] keeps the first of equally scoring glyphs, so the
/// order the atlas rendered them in is part of the answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphSet {
    glyphs: Vec<(char, GlyphTemplate)>,
}

impl GlyphSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces an existing glyph in place, as a dictionary assignment would.
    pub fn insert(&mut self, ch: char, glyph: GlyphTemplate) {
        match self.glyphs.iter_mut().find(|(c, _)| *c == ch) {
            Some((_, existing)) => *existing = glyph,
            None => self.glyphs.push((ch, glyph)),
        }
    }

    pub fn get(&self, ch: char) -> Option<&GlyphTemplate> {
        self.glyphs.iter().find(|(c, _)| *c == ch).map(|(_, g)| g)
    }

    pub fn iter(&self) -> impl Iterator<Item = (char, &GlyphTemplate)> {
        self.glyphs.iter().map(|(c, g)| (*c, g))
    }

    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

/// The characters a slot count can contain, rendered at a given size. Exists so `CountReader`
/// stays platform-free while font rasterising lives in the capture layer.
///
/// Gotcha: must be safe to call concurrently — `InventoryReader` reads slots in parallel and each
/// one asks for an atlas. Implementations that cache have to synchronise that cache.
pub trait GlyphAtlas: Send + Sync {
    /// Every glyph, scaled so a digit stands exactly `digit_height` tall. Each glyph keeps its own
    /// height and sits on the shared baseline — 'x' is lower case and arrives about two thirds of a
    /// digit, not stretched to match one.
    fn at(&self, digit_height: i32) -> Arc<GlyphSet>;
}
