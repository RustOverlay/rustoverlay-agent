/// What a slot composites its icon over, normalised to 0..1 per channel.
///
/// Distinct from `Rgb`, which is a raw 0..255 pixel. This is an average over a region consumed as
/// a float; rounding it to bytes would throw away precision the compositing arithmetic depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SlotBackground {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl SlotBackground {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// Panics on a channel outside 0..3, as the C# indexer threw.
    #[inline]
    pub fn channels(&self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }

    pub fn channel(&self, channel: usize) -> f32 {
        match channel {
            0 => self.r,
            1 => self.g,
            2 => self.b,
            _ => panic!("channel {channel} out of range"),
        }
    }
}
