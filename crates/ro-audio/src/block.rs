/// A run of interleaved stereo float32 frames (L, R, L, R, …) and the UTC instant of its first frame.
#[derive(Clone, Debug)]
pub struct AudioBlock {
    /// Unix epoch, 100-ns ticks, mapped from the capture's QPC timestamps.
    pub utc_100ns: i64,
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl AudioBlock {
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }
}
