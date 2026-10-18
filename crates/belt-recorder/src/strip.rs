//! What gets recorded: the belt row plus a margin, and whether it changed since the last save.

use ro_vision::capture::ScreenRect;
use ro_vision::hud::{FrameBuffer, InventoryGeometry};

const MARGIN: i32 = 14;
const CELLS: usize = 24;

/// The belt row plus a margin, which is all any of this measures.
///
/// A full frame is ~3 MB and hundreds of them are pointless when only six slots are scored. The
/// filename carries the frame size and this origin, so `InventoryGeometry` rebuilds the slot rects
/// exactly and the crop can be indexed at its original coordinates.
pub fn strip_of<F: FrameBuffer + ?Sized>(frame: &F, geometry: &InventoryGeometry) -> ScreenRect {
    let belt = geometry.belt();
    let (first, last) = (belt[0], belt[belt.len() - 1]);

    let x = 0.max(first.x - MARGIN);
    let y = 0.max(first.y - MARGIN);

    ScreenRect::new(
        x,
        y,
        (frame.width() - x).min(last.right() + MARGIN - x + 1),
        (frame.height() - y).min(first.bottom() + MARGIN - y + 1),
    )
}

/// A coarse fingerprint of the strip, for telling "the player moved" from "the player parked".
pub fn digest<F: FrameBuffer + ?Sized>(frame: &F, strip: ScreenRect) -> [u8; CELLS * 3] {
    let mut digest = [0u8; CELLS * 3];
    let cell_width = 1.max(strip.width / CELLS as i32);
    let right = strip.x + strip.width;

    for cell in 0..CELLS {
        let (mut r, mut g, mut b, mut n) = (0i64, 0i64, 0i64, 0i64);
        let start = strip.x + cell as i32 * cell_width;
        let end = right.min(strip.x + (cell as i32 + 1) * cell_width);

        for y in (strip.y..strip.y + strip.height).step_by(3) {
            for x in (start..end).step_by(3) {
                let c = frame.pixel(x, y);
                r += i64::from(c.r);
                g += i64::from(c.g);
                b += i64::from(c.b);
                n += 1;
            }
        }

        if n == 0 {
            continue;
        }

        digest[cell * 3] = (r / n) as u8;
        digest[cell * 3 + 1] = (g / n) as u8;
        digest[cell * 3 + 2] = (b / n) as u8;
    }

    digest
}

pub fn similar(a: &[u8], b: &[u8]) -> bool {
    let moved = a.iter().zip(b).filter(|(a, b)| (i32::from(**a) - i32::from(**b)).abs() > 8).count();

    moved < 6
}

#[cfg(test)]
mod tests {
    use ro_vision::hud::{ArrayFrameBuffer, Rgb};

    use super::*;

    fn flat(width: i32, height: i32, bgr: [u8; 3]) -> ArrayFrameBuffer {
        let pixels = (0..width * height).flat_map(|_| [bgr[0], bgr[1], bgr[2], 255]).collect();
        ArrayFrameBuffer::new(pixels, width * 4, ScreenRect::new(0, 0, width, height))
    }

    #[test]
    fn the_strip_wraps_the_belt_with_a_margin() {
        let frame = flat(1, 1, [0, 0, 0]);
        let geometry = InventoryGeometry::new(1440, 1080, 1.0);
        let belt = geometry.belt();
        let strip = strip_of(&ScreenSized(&frame, 1440, 1080), &geometry);

        assert_eq!(strip.x, belt[0].x - MARGIN);
        assert_eq!(strip.y, belt[0].y - MARGIN);
        assert_eq!(strip.x + strip.width, belt[5].right() + MARGIN + 1);
        assert_eq!(strip.y + strip.height, (belt[0].bottom() + MARGIN + 1).min(1080));
    }

    #[test]
    fn a_digest_averages_each_cell() {
        let frame = flat(48, 6, [30, 20, 10]);
        let digest = digest(&frame, ScreenRect::new(0, 0, 48, 6));

        assert!(digest.chunks(3).all(|c| c == [10, 20, 30]));
    }

    #[test]
    fn small_drift_is_similar_and_a_moved_scene_is_not() {
        let a = [100u8; CELLS * 3];
        let mut b = a;
        b[..5].fill(200);
        assert!(similar(&a, &b));

        b[5] = 0;
        assert!(!similar(&a, &b));

        let mut c = a;
        c.fill(108);
        assert!(similar(&a, &c));
    }

    /// Reports a frame size larger than its pixels, for geometry-only checks.
    struct ScreenSized<'a>(&'a ArrayFrameBuffer, i32, i32);

    impl FrameBuffer for ScreenSized<'_> {
        fn width(&self) -> i32 {
            self.1
        }

        fn height(&self) -> i32 {
            self.2
        }

        fn pixel(&self, _: i32, _: i32) -> Rgb {
            self.0.pixel(0, 0)
        }
    }
}
