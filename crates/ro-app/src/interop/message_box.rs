use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::HSTRING;

/// A modal error box, for failures that happen before or outside any egui window.
pub fn show_error(title: &str, message: &str) {
    // SAFETY: both strings outlive the call.
    unsafe { MessageBoxW(None, &HSTRING::from(message), &HSTRING::from(title), MB_OK | MB_ICONERROR) };
}
