use std::fmt;

use ro_vision::hud::ArrayFrameBuffer;

use crate::gdiplus::GdiplusError;

/// One frame of the Rust client area: a window onto a backend's reused BGRA buffer.
pub type Frame<'a> = ArrayFrameBuffer<&'a [u8]>;

/// A backend call that failed outright, as opposed to having nothing to hand back.
#[derive(Debug)]
pub enum CaptureError {
    Windows(windows::core::Error),
    Gdiplus(GdiplusError),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Windows(e) => e.fmt(f),
            Self::Gdiplus(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<windows::core::Error> for CaptureError {
    fn from(e: windows::core::Error) -> Self {
        Self::Windows(e)
    }
}

impl From<GdiplusError> for CaptureError {
    fn from(e: GdiplusError) -> Self {
        Self::Gdiplus(e)
    }
}

/// Supplies frames of the Rust client area to the HUD readers.
///
/// The seam between how a frame is obtained and what is read out of it. Desktop capture, Desktop
/// Duplication and Windows Graphics Capture differ enormously in setup and lifecycle but agree on
/// the only thing the readers want: a rectangle of pixels. See docs/capture-loop.md.
///
/// Gotcha: implementations must be safe to call from more than one thread. The capture loop and
/// the debug frame-grab button share one instance, and a GPU-backed implementation holds device
/// state that a concurrent call would corrupt.
pub trait FrameSource: Send + Sync {
    /// Reads the Rust client area, passing it to `use_frame`. `Ok(false)` when Rust is not running,
    /// has no client area yet, or the backend has nothing to hand back — in which case `use_frame`
    /// is not called.
    ///
    /// Gotcha: the frame is valid only for the duration of the call, which is why this is a
    /// callback rather than a return value. Backends reuse one pixel buffer between grabs, so a
    /// frame kept past the call would be read while it is overwritten — half of one frame and half
    /// of the next, exactly the input that makes the matcher answer confidently and wrongly.
    fn read(&self, use_frame: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError>;
}
