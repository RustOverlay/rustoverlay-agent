//! Answers whether screen coordinates come back in real pixels.
//!
//! A process that is not per-monitor DPI aware is lied to: Windows scales the rectangles it reports
//! from `GetClientRect` and `ClientToScreen` to match what the process expects, while display
//! duplication goes on reporting true physical pixels. Mixing the two crops the wrong region, which
//! moves every HUD offset and makes the matcher answer from the wrong pixels.
//!
//! The shipped agent declares PerMonitorV2 in its manifest, so this should never fire. It exists
//! because the manifest can be bypassed and the failure is otherwise silent.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::{
    GetAwarenessFromDpiAwarenessContext, GetDpiForMonitor, GetThreadDpiAwarenessContext, DPI_AWARENESS,
    DPI_AWARENESS_PER_MONITOR_AWARE, MDT_EFFECTIVE_DPI,
};

/// The DPI of a display at 100%.
pub const UNSCALED: u32 = 96;

/// The display's real DPI when the coordinates reported for this window would be virtualised
/// rather than physical; None when they are real.
pub fn virtualises_coordinates(window: HWND) -> Option<u32> {
    let dpi = dpi_of(window);
    // SAFETY: no preconditions.
    let awareness = unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };

    virtualises(dpi, awareness).then_some(dpi)
}

/// Only a scaled display can be misreported, so an unaware process on a 100% display is fine.
fn virtualises(dpi: u32, awareness: DPI_AWARENESS) -> bool {
    dpi != UNSCALED && awareness != DPI_AWARENESS_PER_MONITOR_AWARE
}

/// Effective DPI is reported truthfully whatever the caller's own awareness, which is exactly what
/// makes it usable as a check on that awareness.
fn dpi_of(window: HWND) -> u32 {
    let (mut x, mut y) = (0, 0);
    // SAFETY: MonitorFromWindow tolerates a null or stale handle by returning the nearest monitor.
    let read = unsafe { GetDpiForMonitor(MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST), MDT_EFFECTIVE_DPI, &mut x, &mut y) };

    if read.is_ok() { x } else { UNSCALED }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::HiDpi::{DPI_AWARENESS_SYSTEM_AWARE, DPI_AWARENESS_UNAWARE};

    use super::*;

    #[test]
    fn an_unscaled_display_is_never_virtualised() {
        assert!(!virtualises(UNSCALED, DPI_AWARENESS_UNAWARE));
        assert!(!virtualises(UNSCALED, DPI_AWARENESS_PER_MONITOR_AWARE));
    }

    #[test]
    fn a_scaled_display_is_virtualised_unless_per_monitor_aware() {
        assert!(virtualises(120, DPI_AWARENESS_UNAWARE));
        assert!(virtualises(144, DPI_AWARENESS_SYSTEM_AWARE));
        assert!(!virtualises(120, DPI_AWARENESS_PER_MONITOR_AWARE));
    }
}
