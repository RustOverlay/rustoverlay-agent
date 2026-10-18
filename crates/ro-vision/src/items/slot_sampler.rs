use std::sync::Arc;

use super::{IconSignature, SlotBackground};

/// Resamples one slot at small perturbations of where the geometry believes the icon is drawn.
///
/// The geometry places the icon area to within a pixel or two. That is ample for a compact item
/// and not nearly enough for a thin one — a rifle is a few pixels wide at slot scale, so a two
/// pixel error moves it clean off itself. Refining the alignment took the labelled corpus from 194
/// correct to 201 with nothing lost; see docs/item-matching.md.
pub trait SlotSampler {
    /// What the slot composites its icon over. Constant across alignments.
    fn background(&self) -> SlotBackground;

    /// How far the icon area departs from what the margin around it predicts. Near zero when the
    /// slot holds nothing. See [`super::FrameSlotSampler::departure_from`].
    fn departure(&self) -> f64;

    /// Per-cell prediction of the scene behind the icon, or None when there is none to be had.
    fn scene(&self) -> Option<&[f32]>;

    /// The slot with its icon area moved by (dx, dy) and grown by `d_size`.
    fn at(&self, dx: i32, dy: i32, d_size: i32) -> Arc<IconSignature>;
}

/// A sampler that cannot move, for callers holding a signature and no frame to re-crop.
#[derive(Clone, Debug)]
pub struct FixedSlotSampler {
    signature: Arc<IconSignature>,
    background: SlotBackground,
}

impl FixedSlotSampler {
    pub fn new(signature: Arc<IconSignature>, background: SlotBackground) -> Self {
        Self { signature, background }
    }
}

impl SlotSampler for FixedSlotSampler {
    fn background(&self) -> SlotBackground {
        self.background
    }

    /// Nothing to measure it against, so it never gates.
    fn departure(&self) -> f64 {
        f64::INFINITY
    }

    /// No frame to interpolate a scene from, so the flat background is all there is.
    fn scene(&self) -> Option<&[f32]> {
        None
    }

    fn at(&self, _dx: i32, _dy: i32, _d_size: i32) -> Arc<IconSignature> {
        Arc::clone(&self.signature)
    }
}
