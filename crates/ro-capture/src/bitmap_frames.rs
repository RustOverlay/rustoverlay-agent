//! Copies a GDI+ bitmap into a frame buffer the readers can use.

use std::path::Path;

use ro_vision::capture::ScreenRect;
use ro_vision::hud::ArrayFrameBuffer;

use crate::gdiplus::{Bitmap, GdiplusError};

/// One `LockBits` copy rather than `GetPixel` per pixel, which is orders of magnitude slower and
/// would dominate the capture loop.
pub fn read(bitmap: &mut Bitmap) -> Result<ArrayFrameBuffer, GdiplusError> {
    let (width, height) = (bitmap.width()?, bitmap.height()?);
    let (pixels, stride) = bitmap.read_argb(width, height)?;

    Ok(ArrayFrameBuffer::new(pixels, stride, ScreenRect::new(0, 0, width, height)))
}

/// `read(new Bitmap(path))`, decoding the file through GDI+ as the corpus tools always have.
pub fn read_file(path: &Path) -> Result<ArrayFrameBuffer, GdiplusError> {
    read(&mut Bitmap::from_file(path)?)
}
