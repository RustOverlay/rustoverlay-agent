use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};

pub const GAME_CLIENT: &str = "RustClient.exe";

/// A process that can be asked whether it is still running without walking the process table:
/// a snapshot costs ~25 ms of kernel time on a busy machine, a wait on the handle microseconds,
/// and the handle cannot be fooled by its PID being reused.
pub struct Watched(HANDLE);

impl Watched {
    pub fn open(pid: u32) -> Result<Self, String> {
        // SAFETY: a PID that has gone or is not ours to open just fails the call.
        unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.map(Self).map_err(|e| format!("OpenProcess({pid}): {e}"))
    }

    /// A failed wait counts as exited, which only costs a reconnect.
    pub fn has_exited(&self) -> bool {
        // SAFETY: the handle is open until drop; a zero timeout never blocks.
        let waited = unsafe { WaitForSingleObject(self.0, 0) };
        waited != WAIT_TIMEOUT
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        // SAFETY: opened by `open` and closed only here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[derive(Clone, Debug)]
pub struct TargetProcess {
    pub name: String,
    pub pid: u32,
}

struct Entry {
    name: String,
    pid: u32,
    parent: u32,
}

/// Resolves a PID or an executable name (".exe" optional, case-insensitive). With several
/// same-named processes the root of their tree wins, since capture includes the whole tree anyway.
pub fn find(spec: &str) -> Result<TargetProcess, String> {
    let entries = snapshot()?;
    let found = if let Ok(pid) = spec.parse::<u32>() {
        entries.iter().find(|e| e.pid == pid)
    } else {
        let wanted = spec.to_ascii_lowercase();
        let wanted = if wanted.ends_with(".exe") { wanted } else { format!("{wanted}.exe") };
        let matches: Vec<&Entry> = entries.iter().filter(|e| e.name.to_ascii_lowercase() == wanted).collect();
        matches.iter().copied().find(|e| !matches.iter().any(|m| m.pid == e.parent)).or(matches.first().copied())
    };
    found
        .map(|e| TargetProcess { name: e.name.clone(), pid: e.pid })
        .ok_or_else(|| format!("no running process matches '{spec}'"))
}

fn snapshot() -> Result<Vec<Entry>, String> {
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.map_err(|e| format!("CreateToolhelp32Snapshot: {e}"))?;
    let entries = walk(handle);
    let _ = unsafe { CloseHandle(handle) };
    entries
}

fn walk(handle: HANDLE) -> Result<Vec<Entry>, String> {
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    unsafe { Process32FirstW(handle, &mut entry) }.map_err(|e| format!("Process32FirstW: {e}"))?;
    let mut entries = Vec::new();
    loop {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        entries.push(Entry {
            name: String::from_utf16_lossy(&entry.szExeFile[..len]),
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
        });
        if unsafe { Process32NextW(handle, &mut entry) }.is_err() {
            return Ok(entries);
        }
    }
}
