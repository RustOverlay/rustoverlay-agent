//! PNG, never JPEG — a lossy corpus would corrupt the very measurements it exists for.
//!
//! Written through GDI+ as a 24bpp RGB bitmap, as the C# recorder did, so the files are the same
//! format the corpus has always held. ro-capture's bitmap is 32bpp ARGB only.

use std::path::Path;
use std::ptr;
use std::sync::OnceLock;

use ro_capture::gdiplus::GdiplusError;
use ro_vision::capture::ScreenRect;
use ro_vision::hud::FrameBuffer;
use windows::core::{GUID, HSTRING};
use windows::Win32::Graphics::GdiPlus::{
    self as gp, GdipCreateBitmapFromScan0, GdipDisposeImage, GdipSaveImageToFile, GdiplusStartup, GdiplusStartupOutput,
    Status,
};

const PIXEL_FORMAT_24BPP_RGB: i32 = 0x0002_1808;
/// The built-in PNG encoder, `ImageFormat.Png`'s codec.
const PNG_ENCODER: GUID = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);

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

/// BGR rows padded to a 4-byte stride, padding zeroed: the buffer `LockBits` filled in C#.
pub fn bgr_rows<F: FrameBuffer + ?Sized>(frame: &F, strip: ScreenRect) -> (Vec<u8>, usize) {
    let stride = (strip.width as usize * 3 + 3) & !3;
    let mut bytes = vec![0u8; stride * strip.height as usize];

    for y in 0..strip.height {
        let row = &mut bytes[y as usize * stride..];
        for x in 0..strip.width {
            let c = frame.pixel(strip.x + x, strip.y + y);
            row[x as usize * 3..x as usize * 3 + 3].copy_from_slice(&[c.b, c.g, c.r]);
        }
    }

    (bytes, stride)
}

pub fn save<F: FrameBuffer + ?Sized>(frame: &F, strip: ScreenRect, path: &Path) -> Result<(), GdiplusError> {
    ensure_started()?;
    let (bytes, stride) = bgr_rows(frame, strip);
    let mut bitmap = ptr::null_mut();

    // SAFETY: `bytes` holds `stride * height` bytes and outlives the bitmap, disposed below.
    check(
        unsafe {
            GdipCreateBitmapFromScan0(
                strip.width, strip.height, stride as i32, PIXEL_FORMAT_24BPP_RGB, Some(bytes.as_ptr()), &mut bitmap,
            )
        },
        "GdipCreateBitmapFromScan0",
    )?;

    let name = HSTRING::from(path.as_os_str());
    // SAFETY: the bitmap is valid; the encoder id and name outlive the call.
    let saved = unsafe { GdipSaveImageToFile(bitmap.cast(), &name, &PNG_ENCODER, ptr::null()) };
    // SAFETY: owned and not yet disposed.
    unsafe { GdipDisposeImage(bitmap.cast()) };
    check(saved, "GdipSaveImageToFile")
}

#[cfg(test)]
mod tests {
    use ro_vision::hud::ArrayFrameBuffer;

    use super::*;

    #[test]
    fn a_saved_strip_reads_back_pixel_for_pixel() {
        let (width, height) = (7, 3);
        let pixels: Vec<u8> = (0..width * height).flat_map(|i| [(i * 9) as u8, (i * 5) as u8, (i * 3) as u8, 255]).collect();
        let frame = ArrayFrameBuffer::new(pixels, width * 4, ScreenRect::new(0, 0, width, height));
        let strip = ScreenRect::new(2, 1, 4, 2);
        let path = std::env::temp_dir().join(format!("belt-recorder-test-{}.png", std::process::id()));

        save(&frame, strip, &path).unwrap();
        let back = ro_capture::bitmap_frames::read_file(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!((back.width(), back.height()), (4, 2));
        for y in 0..2 {
            for x in 0..4 {
                assert_eq!(back.pixel(x, y), frame.pixel(strip.x + x, strip.y + y));
            }
        }
    }

    #[test]
    fn rows_are_padded_to_four_bytes() {
        let frame = ArrayFrameBuffer::new(vec![1, 2, 3, 255], 4, ScreenRect::new(0, 0, 1, 1));
        let (bytes, stride) = bgr_rows(&frame, ScreenRect::new(0, 0, 1, 1));

        assert_eq!(stride, 4);
        assert_eq!(bytes, vec![1, 2, 3, 0]);
    }
}
