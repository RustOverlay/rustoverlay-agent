use ro_vision::capture::ScreenRect;
use ro_vision::hud::ArrayFrameBuffer;
use windows::Win32::Graphics::Gdi::{BitBlt, GetDC, ReleaseDC, SRCCOPY};

use crate::frame_source::{CaptureError, Frame, FrameSource};
use crate::gdiplus::{Bitmap, Graphics};
use crate::rust_window;

/// Grabs the Rust client area off the composited desktop with GDI.
///
/// The fallback backend, not the preferred one. It works while Rust is borderless
/// (`graphics.screenmode 1`) but returns black under exclusive fullscreen and captures whatever
/// overlaps the window. [`crate::DuplicationFrameSource`] supersedes it, but duplication is
/// unavailable in some remote-desktop sessions and on virtualised display drivers, so this stays
/// as the thing to fall back to. Passive throughout: no injection, no hooks, no reads of the game.
#[derive(Debug, Default)]
pub struct ScreenFrameSource;

impl ScreenFrameSource {
    pub fn new() -> Self {
        Self
    }
}

impl FrameSource for ScreenFrameSource {
    fn read(&self, use_frame: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError> {
        let Some(client) = rust_window::client_bounds() else { return Ok(false) };

        let mut frame = Bitmap::new_argb(client.width, client.height)?;
        {
            let graphics = Graphics::from_image(&frame)?;
            copy_from_screen(&graphics, client.x, client.y, client.width, client.height)?;
        }

        let (pixels, stride) = frame.read_argb(client.width, client.height)?;
        use_frame(&ArrayFrameBuffer::new(&pixels[..], stride, ScreenRect::new(0, 0, client.width, client.height)));

        Ok(true)
    }
}

/// `Graphics.CopyFromScreen(x, y, 0, 0, size, CopyPixelOperation.SourceCopy)`, call for call: a
/// screen DC blitted into the bitmap's borrowed HDC.
fn copy_from_screen(graphics: &Graphics, x: i32, y: i32, width: i32, height: i32) -> Result<(), CaptureError> {
    // SAFETY: the screen DC is released before return; the target DC is valid inside with_hdc.
    unsafe {
        let screen = GetDC(None);
        let blitted = graphics.with_hdc(|target| BitBlt(target, 0, 0, width, height, Some(screen), x, y, SRCCOPY));
        ReleaseDC(None, screen);
        blitted??;
    }

    Ok(())
}
