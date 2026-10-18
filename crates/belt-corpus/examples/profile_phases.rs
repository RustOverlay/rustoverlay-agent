//! Wall and process-CPU time per phase of a corpus run, to find where the time goes without an
//! elevated sampling profiler.
//!
//! `cargo run -p belt-corpus --release --example profile_phases -- <corpus-dir> [--skins] [--flat]`

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use belt_corpus::{args, session, strip_frame::StripFrame, strip_name};
    use ro_capture::{bitmap_frames, icon_signatures};
    use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
    use ro_vision::hud::InventoryGeometry;
    use ro_vision::items::{FrameSlotSampler, IconFace, ItemCatalog};
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    fn cpu() -> Duration {
        let (mut a, mut b, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut kernel, &mut user) }.unwrap();
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
    }

    struct Phase { name: &'static str, wall: Duration, cpu: Duration, n: usize }
    let mut phases: Vec<Phase> = Vec::new();
    let mut time = |name: &'static str, n: usize, f: &mut dyn FnMut()| {
        let (w0, c0) = (Instant::now(), cpu());
        f();
        let (wall, cpu) = (w0.elapsed(), cpu() - c0);
        match phases.iter_mut().find(|p| p.name == name) {
            Some(p) => { p.wall += wall; p.cpu += cpu; p.n += n }
            None => phases.push(Phase { name, wall, cpu, n }),
        }
    };

    let raw_args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let options = match args::parse(&raw_args) {
        Err(invalid @ args::ArgsError::Invalid { .. }) => {
            eprintln!("{invalid}");
            std::process::exit(1);
        }
        parsed => parsed.expect("usage: profile_phases <corpus-dir> [--skins] [--flat]"),
    };
    let install = SteamRustInstallLocator::new(None).locate().expect("a Rust install");

    let mut catalog = None;
    time("catalog load (json)", 1, &mut || catalog = Some(Arc::new(ItemCatalog::load(&install.items_directory()).unwrap())));
    let catalog = catalog.unwrap();
    let mut signatures = None;
    let faces: Vec<IconFace> = catalog.with_icons().cloned().map(IconFace::from).collect();
    let n = faces.len();
    time("icon signatures (gdi+)", n, &mut || signatures = Some(icon_signatures::build(faces.clone()).0));
    let signatures = signatures.unwrap();

    let cache_file = std::env::temp_dir().join("ro-profile-signatures.bin");
    let _ = std::fs::remove_file(&cache_file);
    let cold = ro_capture::SignatureCache::open(&cache_file);
    time("signatures, cache cold", n, &mut || { icon_signatures::build_cached(faces.clone(), &cold); cold.save().unwrap(); });
    let mut cached = None;
    time("signatures, cache warm", n, &mut || cached = Some(icon_signatures::build_cached(faces.clone(), &ro_capture::SignatureCache::open(&cache_file)).0));
    let cached = cached.unwrap();
    assert!(
        cached.len() == signatures.len()
            && cached.iter().zip(&signatures).all(|((fa, a), (fb, b))| fa == fb && a.rgb() == b.rgb() && a.alpha() == b.alpha()),
        "cached signatures differ from a fresh decode"
    );
    println!("cache: {} signatures bit-identical to a fresh decode", cached.len());

    // Matched against the catalogue as the agent ships it: shipped skins, and workshop skins with --skins.
    let mut shipped = None;
    time("shipped catalogue (gdi+)", 1, &mut || shipped = Some(session::build_signatures(&install, &options).unwrap().0));
    let shipped = shipped.unwrap();
    println!("matcher catalogue: {} faces", shipped.len());
    let matcher = session::build_matcher(shipped, &options);

    let paths = strip_name::list(&options.directory).unwrap();
    for path in &paths {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Some(strip) = strip_name::select(&name, "000000", "999999").unwrap() else { continue };
        let mut pixels = None;
        time("strip decode (gdi+)", 1, &mut || pixels = Some(bitmap_frames::read_file(path).unwrap()));
        let frame = StripFrame::new(pixels.unwrap(), strip.frame_width, strip.frame_height, strip.origin_x, strip.origin_y);
        let geometry = InventoryGeometry::new(strip.frame_width, strip.frame_height, strip.ui_scale);
        for &slot in geometry.belt() {
            time("identify (per slot)", 1, &mut || { std::hint::black_box(matcher.identify_sampler(&FrameSlotSampler::new(&frame, slot), None, true)); });
        }
    }

    let total_wall: Duration = phases.iter().map(|p| p.wall).sum();
    println!("{:<26} {:>9} {:>9} {:>6} {:>7} {:>11}", "phase", "wall s", "cpu s", "cpu/w", "wall %", "per item ms");
    for p in &phases {
        println!(
            "{:<26} {:>9.2} {:>9.2} {:>6.2} {:>6.1}% {:>11.3}  (n={})",
            p.name, p.wall.as_secs_f64(), p.cpu.as_secs_f64(), p.cpu.as_secs_f64() / p.wall.as_secs_f64().max(1e-9),
            100.0 * p.wall.as_secs_f64() / total_wall.as_secs_f64(), 1000.0 * p.wall.as_secs_f64() / p.n as f64, p.n
        );
    }
}
