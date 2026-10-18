//! Locates the Rust client window and its client-area rectangle in screen coordinates.

use std::sync::atomic::{AtomicI64, AtomicIsize, Ordering};

use ro_core::capture::ScreenRect;
use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetForegroundWindow, GetWindow, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
    GW_OWNER,
};

const PROCESS_NAME: &str = "RustClient";

/// How long a failed search is trusted before the process table is walked again, in ms.
const REDISCOVER_AFTER: i64 = 1000;

static HANDLE: AtomicIsize = AtomicIsize::new(0);
static SEARCHED_AT: AtomicI64 = AtomicI64::new(i64::MIN);

/// The Rust client's main window, or a null handle when it is not running.
///
/// Cached, because the only way to find it is to walk the process table, and the callers are
/// loops: the overlay polls for keypresses and the capture loop asks per frame. Validated with
/// `IsWindow` rather than trusted, so a Rust that closed or restarted is noticed on the next call;
/// a failed lookup is not retried for [`REDISCOVER_AFTER`], or a closed Rust would cost the
/// enumeration every time.
pub fn find_handle() -> HWND {
    let cached = HANDLE.load(Ordering::Relaxed);
    // SAFETY: IsWindow accepts any value.
    if cached != 0 && unsafe { IsWindow(Some(hwnd(cached))) }.as_bool() {
        return hwnd(cached);
    }

    // The worst a race can do is enumerate twice, so the lookup is deliberately not locked.
    // SAFETY: no preconditions.
    let now = unsafe { GetTickCount64() } as i64;
    if !search_due(cached, now, SEARCHED_AT.load(Ordering::Relaxed)) {
        return HWND::default();
    }

    SEARCHED_AT.store(now, Ordering::Relaxed);
    let found = locate();
    HANDLE.store(found.0 as isize, Ordering::Relaxed);

    found
}

/// A cached handle that failed validation is searched for at once; no handle at all waits out
/// [`REDISCOVER_AFTER`] since the last search.
fn search_due(cached: isize, now: i64, searched_at: i64) -> bool {
    cached != 0 || now >= searched_at.saturating_add(REDISCOVER_AFTER)
}

/// Whether Rust is the window the user is actually looking at.
///
/// Gates capture, and not only to save work. Every backend reads a whole desktop or display before
/// cropping, so grabbing while the player has alt-tabbed captures whatever they switched to — a
/// browser, a chat window — which a tool that shares data must not do.
pub fn is_foreground() -> bool {
    let handle = find_handle();
    // SAFETY: no preconditions.
    !handle.is_invalid() && unsafe { GetForegroundWindow() } == handle
}

/// The client area in virtual-desktop coordinates, or None when Rust is not running or has no
/// client area yet. Excludes border and title bar, so HUD offsets are measured against the
/// rendered surface rather than the window frame.
pub fn client_bounds() -> Option<ScreenRect> {
    let handle = find_handle();
    if handle.is_invalid() {
        return None;
    }

    let mut rect = RECT::default();
    // SAFETY: a stale handle just fails the call.
    unsafe { GetClientRect(handle, &mut rect) }.ok()?;

    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: as above.
    if !unsafe { ClientToScreen(handle, &mut origin) }.as_bool() {
        return None;
    }

    let bounds = ScreenRect::new(origin.x, origin.y, rect.right - rect.left, rect.bottom - rect.top);
    (!bounds.is_empty()).then_some(bounds)
}

fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut _)
}

/// The Rust client's main window: the first visible, unowned top-level window whose process is
/// `RustClient.exe`.
///
/// Walks top-level windows rather than the process table. A Toolhelp snapshot copies every process
/// and thread on the machine and cost ~15 ms, which at one search a second while Rust is closed was
/// half the agent's idle CPU. Only a process that refuses even a limited query (anti-cheat may)
/// sends it back to the snapshot, so nothing that search would find is missed.
fn locate() -> HWND {
    match locate_by_windows() {
        Ok(found) => found,
        Err(Unanswered) => locate_by_processes(),
    }
}

/// Some visible window's process could not be identified.
struct Unanswered;

fn locate_by_windows() -> Result<HWND, Unanswered> {
    struct Search {
        found: HWND,
        identified: Vec<(u32, Option<bool>)>,
    }

    unsafe extern "system" fn visit(window: HWND, state: LPARAM) -> BOOL {
        // SAFETY: `state` is the &mut Search passed to EnumWindows below, alive for the call.
        let search = unsafe { &mut *(state.0 as *mut Search) };
        let mut process_id = 0;
        // SAFETY: plain queries on a window handle EnumWindows supplied.
        unsafe {
            let unowned = GetWindow(window, GW_OWNER).map_or(true, |owner| owner.is_invalid());
            if !unowned || !IsWindowVisible(window).as_bool() {
                return true.into();
            }
            GetWindowThreadProcessId(window, Some(&mut process_id));
        }

        let is_rust = match search.identified.iter().find(|(id, _)| *id == process_id) {
            Some(&(_, answer)) => answer,
            None => {
                let answer = is_named(process_id, PROCESS_NAME);
                search.identified.push((process_id, answer));
                answer
            }
        };
        if is_rust == Some(true) {
            search.found = window;
            return false.into();
        }
        true.into()
    }

    let mut search = Search { found: HWND::default(), identified: Vec::new() };
    // SAFETY: the callback only touches `search`, which outlives the enumeration. Stopping early
    // reports an error, which is expected.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut search as isize)) };

    if search.found.is_invalid() && search.identified.iter().any(|(_, answer)| answer.is_none()) {
        return Err(Unanswered);
    }
    Ok(search.found)
}

/// Whether the process's image is `<name>.exe`, or None when the process will not say.
fn is_named(process_id: u32, name: &str) -> Option<bool> {
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;

    // SAFETY: the handle is closed below; `buffer` and `length` describe a valid out-buffer.
    let path = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let queried = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length);
        let _ = CloseHandle(process);
        queried.ok()?;
        String::from_utf16_lossy(&buffer[..length as usize])
    };

    let file = path.rsplit(['\\', '/']).next().unwrap_or(&path);
    Some(file.eq_ignore_ascii_case(&format!("{name}.exe")))
}

/// The first `RustClient` process's main window, as `Process.GetProcessesByName(...)` then
/// `MainWindowHandle` would find it.
fn locate_by_processes() -> HWND {
    for process_id in processes_named(PROCESS_NAME) {
        let window = main_window_of(process_id);
        if !window.is_invalid() {
            return window;
        }
    }

    HWND::default()
}

fn processes_named(name: &str) -> Vec<u32> {
    let exe = format!("{name}.exe");
    let mut ids = Vec::new();

    // SAFETY: the snapshot handle is closed below; the entry is sized as the API requires.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return ids };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };

        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let length = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..length]).eq_ignore_ascii_case(&exe) {
                ids.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }

        let _ = CloseHandle(snapshot);
    }

    ids
}

/// .NET's `MainWindowHandle`: the first top-level window of the process, in z-order, that is
/// visible and has no owner.
fn main_window_of(process_id: u32) -> HWND {
    struct Search {
        process_id: u32,
        found: HWND,
    }

    unsafe extern "system" fn visit(window: HWND, state: LPARAM) -> BOOL {
        // SAFETY: `state` is the &mut Search passed to EnumWindows below, alive for the call.
        let search = unsafe { &mut *(state.0 as *mut Search) };
        let mut owner_process = 0;
        // SAFETY: plain queries on a window handle EnumWindows supplied.
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut owner_process));
            let unowned = GetWindow(window, GW_OWNER).map_or(true, |owner| owner.is_invalid());
            if owner_process == search.process_id && unowned && IsWindowVisible(window).as_bool() {
                search.found = window;
                return false.into();
            }
        }
        true.into()
    }

    let mut search = Search { process_id, found: HWND::default() };
    // SAFETY: the callback only touches `search`, which outlives the enumeration. Stopping early
    // reports an error, which is expected.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut search as isize)) };

    search.found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_search_is_trusted_for_a_second() {
        assert!(search_due(0, 5_000, i64::MIN));
        assert!(!search_due(0, 5_999, 5_000));
        assert!(search_due(0, 6_000, 5_000));
    }

    #[test]
    fn a_process_is_identified_by_its_image_name() {
        let exe = std::env::current_exe().unwrap();
        let stem = exe.file_stem().unwrap().to_string_lossy().to_uppercase();

        assert_eq!(is_named(std::process::id(), &stem), Some(true));
        assert_eq!(is_named(std::process::id(), "RustClient"), Some(false));
    }

    #[test]
    fn a_stale_handle_is_searched_for_at_once() {
        assert!(search_due(42, 5_001, 5_000));
    }
}
