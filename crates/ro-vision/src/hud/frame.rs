use crate::capture::ScreenRect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Random access to a captured frame's pixels. Exists so the HUD readers stay platform-free and
/// testable while the bitmap handling lives in the capture layer.
///
/// Coordinates are `i32` because the geometry is signed arithmetic; callers only ever ask for
/// pixels inside `0..width` x `0..height`.
pub trait FrameBuffer {
    fn width(&self) -> i32;

    fn height(&self) -> i32;

    fn pixel(&self, x: i32, y: i32) -> Rgb;
}

impl<F: FrameBuffer + ?Sized> FrameBuffer for &F {
    fn width(&self) -> i32 {
        (**self).width()
    }

    fn height(&self) -> i32 {
        (**self).height()
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        (**self).pixel(x, y)
    }
}

/// A window onto a BGRA pixel array — the layout both GDI bitmaps and D3D staging textures hand
/// back.
///
/// The crop is an offset rather than a second copy, so a backend that captures a whole display can
/// hand out just the game's client area without touching the pixels again. Callers index from 0,0
/// and never see the offset.
#[derive(Clone, Debug)]
pub struct ArrayFrameBuffer<P = Vec<u8>> {
    pixels: P,
    stride: i32,
    crop: ScreenRect,
}

impl<P: AsRef<[u8]>> ArrayFrameBuffer<P> {
    pub fn new(pixels: P, stride: i32, crop: ScreenRect) -> Self {
        Self { pixels, stride, crop }
    }

    pub fn into_pixels(self) -> P {
        self.pixels
    }
}

impl<P: AsRef<[u8]>> FrameBuffer for ArrayFrameBuffer<P> {
    fn width(&self) -> i32 {
        self.crop.width
    }

    fn height(&self) -> i32 {
        self.crop.height
    }

    #[inline]
    fn pixel(&self, x: i32, y: i32) -> Rgb {
        let i = ((y + self.crop.y) * self.stride + (x + self.crop.x) * 4) as usize;
        let bgr = &self.pixels.as_ref()[i..i + 3];

        Rgb::new(bgr[2], bgr[1], bgr[0])
    }
}
