//! Time per stage of one `identify_sampler` call, over every belt slot of a corpus, to see which
//! part of matching a slot costs what.
//!
//! `cargo run -p belt-corpus --release --example profile_identify -- <corpus-dir> [--skins]`

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};

    use belt_corpus::{args, session, strip_frame::StripFrame, strip_name};
    use ro_capture::bitmap_frames;
    use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
    use ro_vision::hud::InventoryGeometry;
    use ro_vision::items::{FrameSlotSampler, SlotSampler};

    let raw_args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let options = args::parse(&raw_args).expect("usage: profile_identify <corpus-dir> [--skins]");
    let install = SteamRustInstallLocator::new(None).locate().expect("a Rust install");
    let (signatures, _) = session::build_signatures(&install, &options).unwrap();
    let matcher = session::build_matcher(signatures, &options);
    let faces: Vec<_> = matcher.catalogue().values().collect();

    let mut stages = [("sampler (background, departure, scene)", Duration::ZERO), ("27 alignment crops", Duration::ZERO),
        ("coarse pass, every face", Duration::ZERO), ("full score, every face", Duration::ZERO), ("identify_sampler", Duration::ZERO)];
    let mut timed = |stage: usize, f: &mut dyn FnMut()| {
        let start = Instant::now();
        f();
        stages[stage].1 += start.elapsed();
    };

    let mut slots = 0usize;
    for path in strip_name::list(&options.directory).unwrap() {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Some(strip) = strip_name::select(&name, "000000", "999999").unwrap() else { continue };
        let frame = StripFrame::new(bitmap_frames::read_file(&path).unwrap(), strip.frame_width, strip.frame_height, strip.origin_x, strip.origin_y);

        for &slot in InventoryGeometry::new(strip.frame_width, strip.frame_height, strip.ui_scale).belt() {
            slots += 1;
            let mut sampler = None;
            timed(0, &mut || sampler = Some(FrameSlotSampler::new(&frame, slot)));
            let sampler = sampler.unwrap();
            timed(1, &mut || {
                for (dx, dy) in (-2..=2).flat_map(|dy| (-2..=2).map(move |dx| (dx, dy))) {
                    black_box(sampler.at(dx, dy, 0));
                }
                black_box(sampler.at(0, 0, 2));
                black_box(sampler.at(0, 0, 4));
            });

            let (nominal, background) = (sampler.at(0, 0, 0), sampler.background());
            timed(2, &mut || faces.iter().for_each(|face| { black_box(nominal.coarse_similarity_to(face, background)); }));
            timed(3, &mut || match sampler.scene() {
                Some(scene) => faces.iter().for_each(|face| { black_box(nominal.similarity_to_scene(face, background, scene)); }),
                None => faces.iter().for_each(|face| { black_box(nominal.similarity_to_composited(face, background)); }),
            });
            timed(4, &mut || { black_box(matcher.identify_sampler(&FrameSlotSampler::new(&frame, slot), None, true)); });
        }
    }

    println!("{} faces, {} slots, coarse shortlist {}", faces.len(), slots, matcher.coarse_shortlist);
    for (name, total) in stages {
        println!("{name:<40} {:>9.3} ms/slot", 1000.0 * total.as_secs_f64() / slots as f64);
    }
}
