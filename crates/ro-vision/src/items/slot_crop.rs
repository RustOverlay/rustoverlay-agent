//! The area-averaged downscale of a captured slot to an [`IconSignature`].

use super::IconSignature;
use crate::hud::{FrameBuffer, SlotRect};

const N: usize = IconSignature::SIZE;

/// Area average down to `IconSignature::SIZE`, alpha left opaque.
///
/// A slot crop has already been composited by the game, so there is no alpha left to mask with;
/// the catalogue side still masks, which is what excludes the background.
///
/// Gotcha: coverage is fractional, not snapped to whole pixels. A slot is only ~2.3px per output
/// cell, so integer boundaries hand some cells 2 source pixels and their neighbours 3 — a 50% swing
/// that cost six reads on the labelled corpus, all of them thin items.
///
/// The matcher re-crops each slot ~27 times while fitting its alignment, so the weights are
/// computed once per axis and the region read once; every weight and every sum is the same
/// expression in the same order as the direct 2-D loop, so the result is bit-identical to it.
pub(super) fn crop<F: FrameBuffer + ?Sized>(frame: &F, area: SlotRect) -> IconSignature {
    // Gotcha: the last clamp only bites on an area hanging off a negative edge, which read past the frame.
    let fits = (frame.width() - area.x).min(frame.height() - area.y).min(frame.width()).min(frame.height());
    let size = 1.max(area.size.min(fits));
    let x0 = 0.max(area.x.min(frame.width() - size));
    let y0 = 0.max(area.y.min(frame.height() - size));
    let scale = f64::from(size) / N as f64;

    let (xs, ys) = (Axis::new(x0, size, scale), Axis::new(y0, size, scale));
    let width = size as usize;
    let region: Vec<_> = (y0..y0 + size).flat_map(|y| (x0..x0 + size).map(move |x| frame.pixel(x, y))).collect();

    let mut rgb = vec![0f32; N * N * 3];
    let alpha = vec![1f32; N * N];

    for cy in 0..N {
        let row0 = (ys.first[cy] - y0) as usize;

        for cx in 0..N {
            let col0 = (xs.first[cx] - x0) as usize;
            let (mut r, mut g, mut b, mut total) = (0.0, 0.0, 0.0, 0.0);

            for (j, &wy) in ys.weights(cy).iter().enumerate() {
                let line = &region[(row0 + j) * width + col0..];

                for (&wx, c) in xs.weights(cx).iter().zip(line) {
                    let w = wy * wx;
                    r += w * f64::from(c.r);
                    g += w * f64::from(c.g);
                    b += w * f64::from(c.b);
                    total += w;
                }
            }

            let cell = (cy * N + cx) * 3;
            rgb[cell] = (r / total / 255.0) as f32;
            rgb[cell + 1] = (g / total / 255.0) as f32;
            rgb[cell + 2] = (b / total / 255.0) as f32;
        }
    }

    IconSignature::new(rgb, alpha)
}

/// For each output cell along one axis: its first source pixel and each covered pixel's share.
struct Axis {
    first: [i32; N],
    offsets: [usize; N + 1],
    weights: Vec<f64>,
}

impl Axis {
    fn new(origin: i32, size: i32, scale: f64) -> Self {
        let mut first = [0; N];
        let mut offsets = [0; N + 1];
        let mut weights = Vec::with_capacity(N * 4);

        for cell in 0..N {
            let low = f64::from(origin) + cell as f64 * scale;
            let high = low + scale;
            // Gotcha: scale * N need not land exactly on `size`; unclamped, it indexes past the crop.
            let last = (high.ceil() as i32 - 1).min(origin + size - 1);

            first[cell] = low as i32;
            for p in low as i32..=last {
                weights.push(high.min(f64::from(p + 1)) - low.max(f64::from(p)));
            }
            offsets[cell + 1] = weights.len();
        }

        Self { first, offsets, weights }
    }

    fn weights(&self, cell: usize) -> &[f64] {
        &self.weights[self.offsets[cell]..self.offsets[cell + 1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::Rgb;

    struct Noise {
        width: i32,
        height: i32,
    }

    impl FrameBuffer for Noise {
        fn width(&self) -> i32 {
            self.width
        }

        fn height(&self) -> i32 {
            self.height
        }

        fn pixel(&self, x: i32, y: i32) -> Rgb {
            assert!((0..self.width).contains(&x) && (0..self.height).contains(&y), "read ({x}, {y}) off the frame");
            let h =(x as u32).wrapping_mul(2_654_435_761) ^ (y as u32).wrapping_mul(40_503);
            Rgb { r: h as u8, g: (h >> 8) as u8, b: (h >> 16) as u8 }
        }
    }

    /// The direct 2-D loop `crop` replaced, kept as the reference it must match bit for bit.
    fn reference<F: FrameBuffer + ?Sized>(frame: &F, area: SlotRect) -> Vec<f32> {
        let size = 1.max(area.size.min((frame.width() - area.x).min(frame.height() - area.y)));
        let x0 = 0.max(area.x.min(frame.width() - size));
        let y0 = 0.max(area.y.min(frame.height() - size));
        let scale = f64::from(size) / N as f64;
        let mut rgb = vec![0f32; N * N * 3];

        for cy in 0..N {
            let top = f64::from(y0) + cy as f64 * scale;
            let bottom = top + scale;
            for cx in 0..N {
                let left = f64::from(x0) + cx as f64 * scale;
                let right = left + scale;
                let (mut r, mut g, mut b, mut total) = (0.0, 0.0, 0.0, 0.0);
                let last_y = (bottom.ceil() as i32 - 1).min(y0 + size - 1);
                let last_x = (right.ceil() as i32 - 1).min(x0 + size - 1);
                for y in top as i32..=last_y {
                    let wy = bottom.min(f64::from(y + 1)) - top.max(f64::from(y));
                    for x in left as i32..=last_x {
                        let w = wy * (right.min(f64::from(x + 1)) - left.max(f64::from(x)));
                        let c = frame.pixel(x, y);
                        r += w * f64::from(c.r);
                        g += w * f64::from(c.g);
                        b += w * f64::from(c.b);
                        total += w;
                    }
                }
                let cell = (cy * N + cx) * 3;
                rgb[cell] = (r / total / 255.0) as f32;
                rgb[cell + 1] = (g / total / 255.0) as f32;
                rgb[cell + 2] = (b / total / 255.0) as f32;
            }
        }
        rgb
    }

    #[test]
    fn matches_the_direct_loop_bit_for_bit() {
        let frame = Noise { width: 1920, height: 1080 };

        for (x, y, size) in [(481, 978, 74), (500, 990, 75), (1_000, 500, 33), (10, 10, 160), (1_900, 1_060, 74), (0, 0, 7)] {
            let area = SlotRect { x, y, size };
            let fast = crop(&frame, area);
            let slow = reference(&frame, area);
            assert!(fast.rgb().iter().zip(&slow).all(|(a, b)| a.to_bits() == b.to_bits()), "differs at {area:?}");
        }
    }

    #[test]
    fn an_area_hanging_off_a_negative_edge_stays_on_the_frame() {
        let frame = Noise { width: 40, height: 60 };

        for (x, y, size) in [(-2, 5, 44), (3, -2, 64), (-3, -3, 80)] {
            crop(&frame, SlotRect { x, y, size });
        }
    }
}
