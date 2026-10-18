//! The few GDI+ calls the contact sheet makes that ro-capture keeps private: a graphics context,
//! a system font and `DrawImage` between two arbitrary bitmaps. Called as `System.Drawing` does.

use std::path::Path;
use std::ptr;
use std::sync::OnceLock;

use ro_capture::gdiplus::GdiplusError;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::GdiPlus::{
    self as gp, GdipCreateBitmapFromFile, GdipCreateBitmapFromScan0, GdipCreateFont, GdipCreateFontFamilyFromName,
    GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteFont, GdipDeleteFontFamily, GdipDeleteGraphics, GdipDisposeImage,
    GdipDrawImageRectRectI, GdipDrawString, GdipGetGenericFontFamilySansSerif, GdipGetImageGraphicsContext, GdipGraphicsClear, GdipImageForceValidation,
    GdipSaveImageToFile, GdipSetInterpolationMode, GdipSetPixelOffsetMode, GdiplusStartup, GdiplusStartupOutput, GpBitmap,
    GpFont, GpGraphics, GpImage, GpSolidFill, InterpolationMode, PixelOffsetMode, RectF, Status, FontStyleRegular,
    UnitPixel, UnitPoint,
};

const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;
/// The built-in PNG encoder, `ImageFormat.Png`'s codec.
const PNG_ENCODER: windows::core::GUID = windows::core::GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);

fn check(status: Status, call: &'static str) -> Result<(), GdiplusError> {
    if status == gp::Ok { Ok(()) } else { Err(GdiplusError { status: status.0, call }) }
}

/// `GdiplusStartupInputEx`; version 2 is what `System.Drawing` requests. Startup is ref-counted, so
/// sharing the process with ro-capture's own startup is fine.
#[repr(C)]
struct StartupInputEx {
    version: u32,
    debug_event_callback: isize,
    suppress_background_thread: windows::core::BOOL,
    suppress_external_codecs: windows::core::BOOL,
    startup_parameters: i32,
}

fn ensure_started() -> Result<(), GdiplusError> {
    static STARTED: OnceLock<Result<(), GdiplusError>> = OnceLock::new();

    *STARTED.get_or_init(|| {
        let input = StartupInputEx {
            version: 2,
            debug_event_callback: 0,
            suppress_background_thread: false.into(),
            suppress_external_codecs: false.into(),
            startup_parameters: 0,
        };
        let (mut token, mut output) = (0usize, GdiplusStartupOutput::default());
        // SAFETY: the Ex struct extends GdiplusStartupInput, which GDI+ distinguishes by version.
        check(unsafe { GdiplusStartup(&mut token, (&raw const input).cast(), &mut output) }, "GdiplusStartup")
    })
}

pub struct Image(*mut GpBitmap);

impl Image {
    /// `new Bitmap(width, height)`, which is 32bpp ARGB.
    pub fn new(width: i32, height: i32) -> Result<Self, GdiplusError> {
        ensure_started()?;
        let mut raw = ptr::null_mut();
        // SAFETY: GDI+ allocates and owns the pixels when scan0 is null.
        check(
            unsafe { GdipCreateBitmapFromScan0(width, height, 0, PIXEL_FORMAT_32BPP_ARGB, None, &mut raw) },
            "GdipCreateBitmapFromScan0",
        )?;
        Ok(Self(raw))
    }

    /// `new Bitmap(path)`.
    pub fn from_file(path: &Path) -> Result<Self, GdiplusError> {
        ensure_started()?;
        let name = HSTRING::from(path.as_os_str());
        let mut raw = ptr::null_mut();
        // SAFETY: the name outlives the call; GDI+ hands back an owned bitmap on success.
        check(unsafe { GdipCreateBitmapFromFile(&name, &mut raw) }, "GdipCreateBitmapFromFile")?;
        let image = Self(raw);
        // SAFETY: valid bitmap; Drop disposes it on failure.
        check(unsafe { GdipImageForceValidation(image.image()) }, "GdipImageForceValidation")?;
        Ok(image)
    }

    fn image(&self) -> *mut GpImage {
        self.0.cast()
    }

    pub fn save_png(&self, path: &Path) -> Result<(), GdiplusError> {
        let name = HSTRING::from(path.as_os_str());
        // SAFETY: the encoder id and name outlive the call.
        check(unsafe { GdipSaveImageToFile(self.image(), &name, &PNG_ENCODER, ptr::null()) }, "GdipSaveImageToFile")
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: owned and not yet disposed.
        unsafe { GdipDisposeImage(self.image()) };
    }
}

/// `Graphics.FromImage`, borrowing its target.
pub struct Canvas<'a>(*mut GpGraphics, std::marker::PhantomData<&'a Image>);

impl<'a> Canvas<'a> {
    pub fn on(target: &'a Image) -> Result<Self, GdiplusError> {
        let mut raw = ptr::null_mut();
        // SAFETY: the image is valid for 'a.
        check(unsafe { GdipGetImageGraphicsContext(target.image(), &mut raw) }, "GdipGetImageGraphicsContext")?;
        Ok(Self(raw, std::marker::PhantomData))
    }

    pub fn clear(&self, argb: u32) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipGraphicsClear(self.0, argb) }, "GdipGraphicsClear")
    }

    pub fn set_modes(&self, interpolation: InterpolationMode, offset: PixelOffsetMode) -> Result<(), GdiplusError> {
        // SAFETY: valid graphics.
        check(unsafe { GdipSetInterpolationMode(self.0, interpolation) }, "GdipSetInterpolationMode")?;
        // SAFETY: valid graphics.
        check(unsafe { GdipSetPixelOffsetMode(self.0, offset) }, "GdipSetPixelOffsetMode")
    }

    /// `DrawImage(image, destRect, srcRect, GraphicsUnit.Pixel)`.
    pub fn draw_image(&self, image: &Image, dest: [i32; 4], src: [i32; 4]) -> Result<(), GdiplusError> {
        // SAFETY: both objects are valid; no attributes or abort callback, as in C#.
        check(
            unsafe {
                GdipDrawImageRectRectI(
                    self.0, image.image(), dest[0], dest[1], dest[2], dest[3], src[0], src[1], src[2], src[3],
                    UnitPixel, ptr::null(), 0, ptr::null_mut(),
                )
            },
            "GdipDrawImageRectRectI",
        )
    }

    /// `DrawString(s, font, new SolidBrush(argb), x, y)`: a point origin is a zero-sized layout
    /// rectangle and no string format.
    pub fn draw_string(&self, text: &str, font: &Font, argb: u32, x: f32, y: f32) -> Result<(), GdiplusError> {
        let chars: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF { X: x, Y: y, Width: 0.0, Height: 0.0 };
        let mut brush: *mut GpSolidFill = ptr::null_mut();
        // SAFETY: GDI+ allocates the brush; it is deleted below.
        check(unsafe { GdipCreateSolidFill(argb, &mut brush) }, "GdipCreateSolidFill")?;
        // SAFETY: every pointer is valid for the call; the length is explicit.
        let status = unsafe {
            GdipDrawString(self.0, PCWSTR(chars.as_ptr()), chars.len() as i32, font.0, &layout, ptr::null(), brush.cast())
        };
        // SAFETY: owned brush.
        unsafe { GdipDeleteBrush(brush.cast()) };
        check(status, "GdipDrawString")
    }
}

impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        // SAFETY: owned and not yet deleted.
        unsafe { GdipDeleteGraphics(self.0) };
    }
}

/// `new Font(familyName, emSize)`: regular, measured in points.
pub struct Font(*mut GpFont);

impl Font {
    pub fn system(family: &str, points: f32) -> Result<Self, GdiplusError> {
        ensure_started()?;
        let name = HSTRING::from(family);
        let mut family = ptr::null_mut();
        // SAFETY: a null collection means the installed fonts.
        let status = unsafe { GdipCreateFontFamilyFromName(&name, ptr::null_mut(), &mut family) };
        // System.Drawing's `new Font` falls back to the generic sans-serif when the family is missing.
        if status == gp::FontFamilyNotFound {
            // SAFETY: GDI+ hands back a family that is deleted below, as System.Drawing deletes it.
            check(unsafe { GdipGetGenericFontFamilySansSerif(&mut family) }, "GdipGetGenericFontFamilySansSerif")?;
        } else {
            check(status, "GdipCreateFontFamilyFromName")?;
        }
        let mut raw = ptr::null_mut();
        // SAFETY: the family is valid until deleted below; the font keeps its own reference.
        let status = unsafe { GdipCreateFont(family, points, FontStyleRegular.0, UnitPoint, &mut raw) };
        // SAFETY: owned family.
        unsafe { GdipDeleteFontFamily(family) };
        check(status, "GdipCreateFont")?;
        Ok(Self(raw))
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: owned and not yet deleted.
        unsafe { GdipDeleteFont(self.0) };
    }
}
