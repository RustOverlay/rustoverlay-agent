//! The slice of GDI+ the capture layer drives, called through the flat API in the order and with the
//! arguments `System.Drawing` used, which is what the matcher's thresholds were tuned against.
//! Replaceable if the corpus score holds; see docs/architecture.md.

mod bitmap;
mod font;
mod graphics;

use std::fmt;
use std::sync::OnceLock;

use windows::Win32::Graphics::GdiPlus::{self as gp, GdiplusStartup, GdiplusStartupOutput, Status};

pub use bitmap::Bitmap;
pub(crate) use font::{Font, PrivateFonts};
pub(crate) use graphics::Graphics;

/// `PixelFormat32bppARGB`, which windows-rs does not define: BGRA in memory, not premultiplied.
pub(crate) const PIXEL_FORMAT_32BPP_ARGB: i32 = 0x0026_200A;

/// A failed GDI+ call, carrying the raw `Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GdiplusError {
    pub status: i32,
    pub call: &'static str,
}

impl fmt::Display for GdiplusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed with GDI+ status {}", self.call, self.status)
    }
}

impl std::error::Error for GdiplusError {}

pub(crate) fn check(status: Status, call: &'static str) -> Result<(), GdiplusError> {
    if status == gp::Ok { Ok(()) } else { Err(GdiplusError { status: status.0, call }) }
}

/// `GdiplusStartupInputEx`, which windows-rs leaves out; layout per gdiplusinit.h.
#[repr(C)]
struct StartupInputEx {
    gdiplus_version: u32,
    debug_event_callback: isize,
    suppress_background_thread: windows::core::BOOL,
    suppress_external_codecs: windows::core::BOOL,
    startup_parameters: i32,
}

/// Starts GDI+ once per process and never shuts it down, as `System.Drawing` does.
///
/// Gotcha: version 2 (GDI+ 1.1) is what `System.Drawing` requests on anything newer than Windows 7,
/// and the version selects code paths inside gdiplus.dll, so it is matched rather than defaulted.
pub(crate) fn ensure_started() -> Result<(), GdiplusError> {
    static STARTED: OnceLock<Result<(), GdiplusError>> = OnceLock::new();

    *STARTED.get_or_init(|| {
        let input = StartupInputEx {
            gdiplus_version: 2,
            debug_event_callback: 0,
            suppress_background_thread: false.into(),
            suppress_external_codecs: false.into(),
            startup_parameters: 0,
        };
        let mut token = 0usize;
        let mut output = GdiplusStartupOutput::default();

        // SAFETY: the Ex struct extends GdiplusStartupInput, which GDI+ distinguishes by version.
        let status = unsafe { GdiplusStartup(&mut token, (&raw const input).cast(), &mut output) };
        check(status, "GdiplusStartup")
    })
}
