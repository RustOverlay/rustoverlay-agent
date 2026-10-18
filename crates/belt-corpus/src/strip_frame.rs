use ro_vision::hud::{FrameBuffer, Rgb};

/// A recorded belt strip, presented at the coordinates it was cropped from.
///
/// Reports the size of the frame it came out of rather than its own, so `InventoryGeometry`
/// computes the same slot rects the agent would have. Everything scored lies inside the strip;
/// anything outside reads black and nothing asks for it.
pub struct StripFrame<F> {
    strip: F,
    width: i32,
    height: i32,
    origin_x: i32,
    origin_y: i32,
}

impl<F: FrameBuffer> StripFrame<F> {
    pub fn new(strip: F, width: i32, height: i32, origin_x: i32, origin_y: i32) -> Self {
        Self { strip, width, height, origin_x, origin_y }
    }
}

impl<F: FrameBuffer> FrameBuffer for StripFrame<F> {
    fn width(&self) -> i32 {
        self.width
    }

    fn height(&self) -> i32 {
        self.height
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        let (sx, sy) = (x - self.origin_x, y - self.origin_y);

        if sx < 0 || sy < 0 || sx >= self.strip.width() || sy >= self.strip.height() {
            Rgb::default()
        } else {
            self.strip.pixel(sx, sy)
        }
    }
}

#[cfg(test)]
mod tests {
    use ro_vision::capture::ScreenRect;
    use ro_vision::hud::ArrayFrameBuffer;

    use super::*;

    #[test]
    fn indexes_the_strip_at_its_origin_and_reads_black_outside() {
        // 2x1 strip, BGRA: red then green.
        let strip = ArrayFrameBuffer::new(vec![0, 0, 255, 255, 0, 255, 0, 255], 8, ScreenRect::new(0, 0, 2, 1));
        let frame = StripFrame::new(strip, 1920, 1080, 100, 50);

        assert_eq!((frame.width(), frame.height()), (1920, 1080));
        assert_eq!(frame.pixel(100, 50), Rgb::new(255, 0, 0));
        assert_eq!(frame.pixel(101, 50), Rgb::new(0, 255, 0));
        assert_eq!(frame.pixel(99, 50), Rgb::default());
        assert_eq!(frame.pixel(102, 50), Rgb::default());
        assert_eq!(frame.pixel(100, 51), Rgb::default());
    }
}
