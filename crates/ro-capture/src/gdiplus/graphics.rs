use std::marker::PhantomData;
use std::ptr;

use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::GdiPlus::{
    CompositingMode, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDrawImageRectI,
    GdipDrawImageRectRectI, GdipDrawString, GdipGetDC, GdipGetImageGraphicsContext, GdipGraphicsClear,
    GdipMeasureString, GdipReleaseDC, GdipSetCompositingMode, GdipSetInterpolationMode, GdipSetPixelOffsetMode,
    GdipSetTextRenderingHint, GdipStringFormatGetGenericTypographic, GpGraphics, GpSolidFill, InterpolationMode,
    PixelOffsetMode, RectF, TextRenderingHint, UnitPixel,
};

use super::{check, Bitmap, Font, GdiplusError};

/// `Graphics.FromImage(bitmap)`, tied to the bitmap's lifetime.
pub(crate) struct Graphics<'a> {
    raw: *mut GpGraphics,
    _target: PhantomData<&'a Bitmap>,
}

impl<'a> Graphics<'a> {
    pub fn from_image(target: &'a Bitmap) -> Result<Self, GdiplusError> {
        let mut raw = ptr::null_mut();
        // SAFETY: the image is valid for 'a.
        check(unsafe { GdipGetImageGraphicsContext(target.image(), &mut raw) }, "GdipGetImageGraphicsContext")?;
        Ok(Self { raw, _target: PhantomData })
    }

    pub fn set_text_rendering_hint(&self, hint: TextRenderingHint) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipSetTextRenderingHint(self.raw, hint) }, "GdipSetTextRenderingHint")
    }

    pub fn set_interpolation_mode(&self, mode: InterpolationMode) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipSetInterpolationMode(self.raw, mode) }, "GdipSetInterpolationMode")
    }

    pub fn set_pixel_offset_mode(&self, mode: PixelOffsetMode) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipSetPixelOffsetMode(self.raw, mode) }, "GdipSetPixelOffsetMode")
    }

    pub fn set_compositing_mode(&self, mode: CompositingMode) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipSetCompositingMode(self.raw, mode) }, "GdipSetCompositingMode")
    }

    pub fn clear(&self, argb: u32) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipGraphicsClear(self.raw, argb) }, "GdipGraphicsClear")
    }

    /// `DrawString(s, font, new SolidBrush(argb), x, y, StringFormat.GenericTypographic)`: a point
    /// origin becomes a zero-sized layout rectangle, as System.Drawing does it.
    pub fn draw_string_typographic(&self, text: &str, font: &Font, argb: u32, x: f32, y: f32) -> Result<(), GdiplusError> {
        let chars: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF { X: x, Y: y, Width: 0.0, Height: 0.0 };
        let format = generic_typographic()?;
        let brush = SolidBrush::new(argb)?;
        // SAFETY: every pointer is valid for the call; the length is explicit, so no terminator.
        check(
            unsafe {
                GdipDrawString(self.raw, PCWSTR(chars.as_ptr()), chars.len() as i32, font.raw(), &layout, format, brush.raw.cast())
            },
            "GdipDrawString",
        )
    }

    /// `MeasureString(s, font, PointF.Empty, StringFormat.GenericTypographic).Width`.
    pub fn measure_width_typographic(&self, text: &str, font: &Font) -> Result<f32, GdiplusError> {
        let chars: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF::default();
        let mut bounds = RectF::default();
        let (mut fitted, mut lines) = (0, 0);
        let format = generic_typographic()?;
        // SAFETY: every pointer is valid for the call.
        check(
            unsafe {
                GdipMeasureString(
                    self.raw, PCWSTR(chars.as_ptr()), chars.len() as i32, font.raw(), &layout, format,
                    &mut bounds, &mut fitted, &mut lines,
                )
            },
            "GdipMeasureString",
        )?;
        Ok(bounds.Width)
    }

    /// `DrawImage(image, destRect, srcRect, GraphicsUnit.Pixel)`.
    pub fn draw_image_rect_rect(&self, image: &Bitmap, dest: [i32; 4], src: [i32; 4]) -> Result<(), GdiplusError> {
        // SAFETY: both objects are valid; no attributes or abort callback, as in C#.
        check(
            unsafe {
                GdipDrawImageRectRectI(
                    self.raw, image.image(), dest[0], dest[1], dest[2], dest[3], src[0], src[1], src[2], src[3],
                    UnitPixel, ptr::null(), 0, ptr::null_mut(),
                )
            },
            "GdipDrawImageRectRectI",
        )
    }

    /// `DrawImage(image, new Rectangle(x, y, width, height))`.
    pub fn draw_image_rect(&self, image: &Bitmap, dest: [i32; 4]) -> Result<(), GdiplusError> {
        // SAFETY: both objects are valid.
        check(
            unsafe { GdipDrawImageRectI(self.raw, image.image(), dest[0], dest[1], dest[2], dest[3]) },
            "GdipDrawImageRectI",
        )
    }

    /// `GetHdc()` for the duration of `use_dc`, then `ReleaseHdc()` whatever it returns.
    pub fn with_hdc<T>(&self, use_dc: impl FnOnce(HDC) -> T) -> Result<T, GdiplusError> {
        let mut hdc = HDC::default();
        // SAFETY: valid graphics; the DC is released below before any other GDI+ call on it.
        check(unsafe { GdipGetDC(self.raw, &mut hdc) }, "GdipGetDC")?;
        let result = use_dc(hdc);
        // SAFETY: the DC came from GdipGetDC on this graphics.
        check(unsafe { GdipReleaseDC(self.raw, hdc) }, "GdipReleaseDC")?;
        Ok(result)
    }
}

impl Drop for Graphics<'_> {
    fn drop(&mut self) {
        // SAFETY: owned and not yet deleted.
        unsafe { GdipDeleteGraphics(self.raw) };
    }
}

/// GDI+'s shared generic-typographic format. Owned by GDI+, so never deleted.
fn generic_typographic() -> Result<*mut windows::Win32::Graphics::GdiPlus::GpStringFormat, GdiplusError> {
    let mut format = ptr::null_mut();
    // SAFETY: returns a pointer to a GDI+ static.
    check(unsafe { GdipStringFormatGetGenericTypographic(&mut format) }, "GdipStringFormatGetGenericTypographic")?;
    Ok(format)
}

struct SolidBrush {
    raw: *mut GpSolidFill,
}

impl SolidBrush {
    fn new(argb: u32) -> Result<Self, GdiplusError> {
        let mut raw = ptr::null_mut();
        // SAFETY: GDI+ allocates the brush.
        check(unsafe { GdipCreateSolidFill(argb, &mut raw) }, "GdipCreateSolidFill")?;
        Ok(Self { raw })
    }
}

impl Drop for SolidBrush {
    fn drop(&mut self) {
        // SAFETY: owned and not yet deleted.
        unsafe { GdipDeleteBrush(self.raw.cast()) };
    }
}
