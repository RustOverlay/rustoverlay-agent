//! Finds one of this process's own top-level windows by title. egui hands deferred viewports no
//! window handle, so the overlay's HWND is looked up the way any other window's would be.

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, InternalGetWindowText};
use windows::core::BOOL;

struct Search {
    title: Vec<u16>,
    process: u32,
    found: Option<HWND>,
}

pub fn find(title: &str) -> Option<HWND> {
    let mut search = Search {
        title: title.encode_utf16().collect(),
        // SAFETY: no preconditions.
        process: unsafe { GetCurrentProcessId() },
        found: None,
    };

    // SAFETY: `search` outlives the call; the error from a match's early stop is expected.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}

unsafe extern "system" fn visit(window: HWND, param: LPARAM) -> BOOL {
    // SAFETY: `param` is the `Search` that `find` passed, alive for the enumeration.
    let search = unsafe { &mut *(param.0 as *mut Search) };

    let mut process = 0;
    // SAFETY: a valid out-pointer.
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    if process != search.process {
        return true.into();
    }

    let mut buffer = [0u16; 128];
    // Not `GetWindowTextW`: on our own windows it waits on the UI thread, which can be blocked joining us.
    // SAFETY: the buffer outlives the call.
    let length = unsafe { InternalGetWindowText(window, &mut buffer) }.max(0) as usize;
    if buffer[..length] == search.title[..] {
        search.found = Some(window);
        return false.into();
    }

    true.into()
}
