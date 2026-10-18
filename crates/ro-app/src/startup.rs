//! Wall-clock milestones from process creation to the first painted frame, logged once as a single
//! line so a startup regression shows up in any user's log.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

static MARKS: Mutex<Vec<(&'static str, Duration)>> = Mutex::new(Vec::new());
static REPORTED: AtomicBool = AtomicBool::new(false);

/// Records `name` at the time since the process was created, which includes the loader's work
/// before `main` ran.
pub fn mark(name: &'static str) {
    if let Some(elapsed) = since_creation() {
        MARKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((name, elapsed));
    }
}

/// Logs every mark once; later calls do nothing.
pub fn report(last: &'static str) {
    if REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    mark(last);
    let marks = std::mem::take(&mut *MARKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    if marks.is_empty() {
        return;
    }

    let line: Vec<String> = marks.iter().map(|(name, at)| format!("{name} {}ms", at.as_millis())).collect();
    tracing::info!("Startup: {}", line.join(", "));
}

fn since_creation() -> Option<Duration> {
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    // SAFETY: four valid out-pointers for the current process's pseudo-handle.
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
    // SAFETY: no preconditions.
    let now = unsafe { GetSystemTimePreciseAsFileTime() };
    Some(Duration::from_nanos(ticks(now).checked_sub(ticks(created))? * 100))
}

fn ticks(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}
