//! Times `SkinIconIndex::build` and two decodes; prints peak working set.
//! Usage: cargo run --release -p ro-unity --example index_icons [textures_dir]

use std::path::PathBuf;
use std::time::Instant;

use ro_unity::SkinIconIndex;

const DEFAULT_TEXTURES: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Rust\Bundles\textures";

#[cfg(windows)]
fn peak_working_set_mb() -> f64 {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: the pseudo-handle is always valid and `c` is a correctly sized out-parameter.
    unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    c.PeakWorkingSetSize as f64 / 1e6
}

#[cfg(not(windows))]
fn peak_working_set_mb() -> f64 {
    f64::NAN
}

fn main() -> std::io::Result<()> {
    let dir = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from(DEFAULT_TEXTURES), PathBuf::from);
    let base = peak_working_set_mb();
    let t0 = Instant::now();
    let index = SkinIconIndex::build(&dir)?;
    let built = t0.elapsed();
    let after_build = peak_working_set_mb();
    let t1 = Instant::now();
    for wid in [3208884292u64, 3137160380] {
        let img = index.read_icon(wid)?.expect("icon indexed");
        assert_eq!(img.pixels.len(), (img.width * img.height) as usize);
    }
    println!(
        "indexed {} icons in {:.0} ms; 2 decodes {:.1} ms; peak WS {:.1} MB after build, {:.1} MB end (start {:.1} MB)",
        index.len(),
        built.as_secs_f64() * 1e3,
        t1.elapsed().as_secs_f64() * 1e3,
        after_build,
        peak_working_set_mb(),
        base,
    );
    Ok(())
}
