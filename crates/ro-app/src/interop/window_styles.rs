//! Extended window styles and placement for the overlay window only.
//!
//! A transparent viewport is see-through, not something clicks pass through; that needs
//! `WS_EX_TRANSPARENT`. Placement is in physical pixels through the HWND, because
//! `RustWindow::client_bounds` reports device pixels while egui positions in points.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW, SetWindowPos, WS_CAPTION, WS_EX_APPWINDOW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
    WS_THICKFRAME,
};

/// Makes the window ignore the mouse, stay out of the taskbar and alt-tab, and never take focus.
/// Idempotent and cheap when already applied, so it is re-asserted every tick: winit rewrites
/// `GWL_EXSTYLE` from its own flags whenever any of them change.
///
/// `WS_EX_NOACTIVATE` is load-bearing rather than tidiness. The panel is shown only while Rust is
/// the foreground window, so a panel that could take focus would take it from Rust, hide itself,
/// hand focus back, and show again — a flicker loop driven by its own visibility rule.
///
/// `WS_EX_APPWINDOW` has to go: winit sets it on every unowned window, and it outranks
/// `WS_EX_TOOLWINDOW` for the alt-tab list.
pub fn make_click_through(window: HWND) {
    let added = (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize;
    // SAFETY: a stale handle just fails the calls.
    unsafe {
        let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let wanted = (style & !(WS_EX_APPWINDOW.0 as isize)) | added;
        // Only on a change: setting a style sends WM_STYLECHANGING to the UI thread and waits for it.
        if wanted != style {
            SetWindowLongPtrW(window, GWL_EXSTYLE, wanted);
        }
    }
}

/// Turns the window into a bare popup. winit keeps `WS_CAPTION | WS_SYSMENU` on an undecorated
/// window and hides the frame itself, but with `WS_EX_TOOLWINDOW` set Windows can still draw a tool
/// caption whose close button does nothing, since egui never lets the overlay viewport close.
pub fn strip_frame(window: HWND) {
    let frame = (WS_CAPTION.0 | WS_SYSMENU.0 | WS_THICKFRAME.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as isize;
    // SAFETY: as above.
    unsafe {
        let style = GetWindowLongPtrW(window, GWL_STYLE);
        let wanted = (style & !frame) | WS_POPUP.0 as isize;
        if wanted != style {
            SetWindowLongPtrW(window, GWL_STYLE, wanted);
            let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
            let _ = SetWindowPos(window, None, 0, 0, 0, 0, flags);
        }
    }
}

/// Moves the window to a point in virtual-desktop pixels, leaving size and z-order alone.
/// Asynchronous, so the overlay's input thread never waits on the UI thread.
pub fn move_to(window: HWND, x: i32, y: i32) {
    // SAFETY: as above.
    let _ = unsafe { SetWindowPos(window, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS) };
}

/// Puts the window back on top of the topmost band, which anything made topmost after it sits above.
/// Asynchronous, as `move_to` is.
pub fn raise_topmost(window: HWND) {
    // SAFETY: as above.
    let _ = unsafe {
        SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS)
    };
}
