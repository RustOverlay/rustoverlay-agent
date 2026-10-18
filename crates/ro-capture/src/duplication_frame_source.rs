use std::sync::{Mutex, PoisonError};

use ro_core::capture::{output_selection, ScreenRect};
use ro_vision::hud::ArrayFrameBuffer;
use windows::core::{Error, Interface};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, DXGI_ERROR_WAIT_TIMEOUT,
    DXGI_OUTDUPL_FRAME_INFO,
};

use crate::duplication_session::{create_device, Acquired, Session};
use crate::frame_source::{CaptureError, Frame, FrameSource};
use crate::rust_window;

/// How long to wait for the desktop to change before giving up on an acquisition, in ms.
///
/// Short because a timeout is not a failure — it means the desktop has not changed, so the pixels
/// already held are still current. Only the first frame after a rebuild has nothing to fall back on.
const ACQUIRE_TIMEOUT_MS: u32 = 120;

/// The first acquisitions after `DuplicateOutput` often carry no desktop image — they report
/// cursor movement, with `LastPresentTime` zero. Retrying a few times gets a real one without
/// waiting for the next tick of the capture loop.
const ACQUIRE_ATTEMPTS: usize = 4;

/// Reads the Rust client area with the DXGI Desktop Duplication API.
///
/// Preferred over [`crate::ScreenFrameSource`]: the compositor hands over the display's own
/// surface, so it survives exclusive fullscreen on DirectX 11.1+ and costs a GPU copy rather than a
/// GDI blit. Windows draws no capture indicator — unlike Windows Graphics Capture, whose yellow
/// border can only be removed with a package-manifest capability. See docs/capture-loop.md.
///
/// Passive throughout. Duplication is a display-server facility: nothing is injected, hooked, or
/// read out of the Rust process.
#[derive(Default)]
pub struct DuplicationFrameSource {
    /// Gotcha: held across the caller's read as well as the capture. The pixel buffer is reused
    /// between frames, so a concurrent grab would overwrite the one being read; the device context
    /// is not thread safe either, and the capture loop shares this with the debug grab button.
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    session: Option<Session>,
    pixels: Vec<u8>,
    stride: i32,
    has_pixels: bool,
}

/// Whether this machine can duplicate its primary display, and why not when it cannot.
///
/// Probed once at startup so the choice of backend is made — and logged — before Rust is running
/// rather than silently per frame. Duplication is unavailable in some remote-desktop sessions and
/// on virtualised display drivers, which is why the GDI path is kept.
pub fn is_available() -> Result<(), String> {
    // SAFETY: plain COM factory and enumeration calls; every object is released on drop.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().map_err(|e| e.message())?;
        let Ok(adapter) = factory.EnumAdapters1(0) else { return Err("no display adapter".into()) };
        let Ok(output) = adapter.EnumOutputs(0) else { return Err("no display output".into()) };

        let probe = || -> Result<(), Error> {
            let (device, _context) = create_device(&adapter.cast::<IDXGIAdapter>()?)?;
            output.cast::<IDXGIOutput1>()?.DuplicateOutput(&device)?;
            Ok(())
        };

        probe().map_err(|e| e.message())
    }
}

impl DuplicationFrameSource {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameSource for DuplicationFrameSource {
    fn read(&self, use_frame: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, CaptureError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);

        let Some(client) = rust_window::client_bounds() else {
            // Rust has closed. Nothing will be read until it is back, and the session holds a GPU
            // device and a display's worth of staging memory open meanwhile.
            state.release();
            return Ok(false);
        };

        if !state.session.as_ref().is_some_and(|s| s.bounds.contains(&client)) {
            state.release();
            if !state.start(client) {
                return Ok(false);
            }
        }

        if !state.capture()? && !state.has_pixels {
            return Ok(false);
        }

        let Some(session) = &state.session else { return Ok(false) };
        let crop = output_selection::crop(session.bounds, client);
        use_frame(&ArrayFrameBuffer::new(&state.pixels[..], state.stride, crop));

        Ok(true)
    }
}

impl State {
    fn release(&mut self) {
        self.session = None;
        self.has_pixels = false;
    }

    /// Builds a duplication session for whichever display holds the window.
    fn start(&mut self, client: ScreenRect) -> bool {
        match self.try_start(client) {
            Ok(started) => started,
            Err(error) => {
                // Access is denied while the secure desktop is up, and duplication is unsupported on
                // some hybrid-graphics configurations. Both resolve on their own, so not fatal.
                tracing::warn!(%error, "Could not start display duplication");
                false
            }
        }
    }

    fn try_start(&mut self, client: ScreenRect) -> Result<bool, Error> {
        let mut displays: Vec<(IDXGIAdapter, IDXGIOutput, ScreenRect, i32)> = Vec::new();

        // SAFETY: plain COM factory, enumeration and creation calls; objects release on drop.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;

            let mut a = 0;
            while let Ok(adapter) = factory.EnumAdapters1(a) {
                let mut o = 0;
                while let Ok(output) = adapter.EnumOutputs(o) {
                    let description = output.GetDesc()?;
                    let area = description.DesktopCoordinates;
                    let bounds = ScreenRect::new(area.left, area.top, area.right - area.left, area.bottom - area.top);
                    displays.push((adapter.cast()?, output, bounds, description.Rotation.0));
                    o += 1;
                }
                a += 1;
            }

            let bounds: Vec<ScreenRect> = displays.iter().map(|d| d.2).collect();
            let Some(chosen) = output_selection::display_for(&bounds, client) else {
                tracing::debug!(window = ?client, "Rust window is not wholly on one display; skipping frame");
                return Ok(false);
            };

            let (adapter, output, bounds, rotation) = &displays[chosen];

            // A rotated display duplicates in its unrotated orientation, so every coordinate below
            // would be transposed. Refusing beats reading the wrong pixels confidently.
            if *rotation != DXGI_MODE_ROTATION_IDENTITY.0 && *rotation != DXGI_MODE_ROTATION_UNSPECIFIED.0 {
                tracing::warn!(rotation, "Display is rotated; duplication cannot be used");
                return Ok(false);
            }

            let (device, context) = create_device(adapter)?;
            let duplication = output.cast::<IDXGIOutput1>()?.DuplicateOutput(&device)?;

            self.session = Some(Session::new(device, context, duplication, *bounds));
            self.has_pixels = false;

            tracing::info!(bounds = ?bounds, "Duplicating display");

            Ok(true)
        }
    }

    /// Pulls the newest desktop image into `pixels`. False if there is none.
    fn capture(&mut self) -> Result<bool, CaptureError> {
        for _ in 0..ACQUIRE_ATTEMPTS {
            let Some(session) = self.session.as_mut() else { return Ok(false) };

            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;

            // SAFETY: out-pointers are valid locals.
            match unsafe { session.duplication.AcquireNextFrame(ACQUIRE_TIMEOUT_MS, &mut info, &mut resource) } {
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
                Err(e) => {
                    // Raised on resolution changes, fullscreen transitions, the secure desktop and
                    // driver resets — routine rather than exceptional. Rebuilt on the next call.
                    tracing::debug!(result = %e, "Duplication lost; rebuilding");
                    self.release();
                    return Ok(false);
                }
                Ok(()) => {}
            }

            let taken = session.take(&info, resource, &mut self.pixels);
            // SAFETY: a frame was acquired above and its resource has been dropped.
            let _ = unsafe { session.duplication.ReleaseFrame() };

            match taken? {
                Acquired::NoImage => continue,
                Acquired::Image { stride } => {
                    self.stride = stride;
                    self.has_pixels = true;
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}
