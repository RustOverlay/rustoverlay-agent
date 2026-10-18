//! Picks which display a window is on, and where it sits within that display's pixels. Desktop
//! Duplication is per-display, so a capture backend has to answer both before it can hand back
//! the client area.

pub use ro_vision::capture::ScreenRect;

/// Index of the display wholly containing `window`, or `None` if none does.
///
/// Containment is deliberately all-or-nothing. A window straddling two displays could be served by
/// cropping the part on one of them, but the readers size their geometry from the frame they are
/// given — a short frame puts every HUD offset in the wrong place and the matcher answers
/// confidently from the wrong pixels. It only happens while a window is mid-drag.
pub fn display_for(outputs: &[ScreenRect], window: ScreenRect) -> Option<usize> {
    if window.is_empty() {
        return None;
    }

    outputs.iter().position(|output| output.contains(&window))
}

/// Where the window sits inside the display's own pixels, with its origin at 0,0.
pub fn crop(output: ScreenRect, window: ScreenRect) -> ScreenRect {
    ScreenRect::new(window.x - output.x, window.y - output.y, window.width, window.height)
}
