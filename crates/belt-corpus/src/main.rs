// Scores recorded belt strips against a hotbar the player held fixed. See docs/matcher-tools.md.

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("belt-corpus decodes through GDI+ and runs on Windows only.");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    std::process::ExitCode::from(run::run(&args))
}

#[cfg(windows)]
mod run {
    use std::ffi::OsString;

    use belt_corpus::args::{self, ArgsError, Options};
    use belt_corpus::dotnet_format::fixed;
    use belt_corpus::montage::{self, Crop};
    use belt_corpus::score::{judge, Tally, Verdict};
    use belt_corpus::session::{build_matcher, build_signatures, named, read_strip};
    use belt_corpus::strip_name;
    use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
    use ro_vision::hud::SlotHistory;

    /// The process exit code: 0 when nothing read wrong, 2 when something did, 1 when nothing ran.
    pub fn run(args: &[OsString]) -> u8 {
        let options = match args::parse(args) {
            Ok(options) => options,
            Err(usage @ ArgsError::Usage) => {
                println!("{usage}");
                return 1;
            }
            Err(invalid) => {
                eprintln!("{invalid}");
                return 1;
            }
        };

        match score(&options) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("{message}");
                1
            }
        }
    }

    fn score(options: &Options) -> Result<u8, String> {
        let install = SteamRustInstallLocator::new(None).locate();
        let Some(install) = install.filter(|i| i.has_item_bundles()) else {
            println!("No Rust install with item bundles found; icons are read from it at runtime.");
            return Ok(1);
        };

        let (signatures, skins) = build_signatures(&install, options).map_err(|e| format!("item catalogue: {e}"))?;
        if let Some(skins) = skins {
            println!("{}", skins.line());
        }

        let matcher = build_matcher(signatures, options);
        // Replays the corpus through the agent's multi-frame filter, in capture order.
        let mut history = options.confirm.map(|n| {
            let mut history = SlotHistory::new();
            history.confirm_after = n;
            history
        });

        println!(
            "catalogue: {} icons   departure {}   confirm {}   scene {}",
            matcher.catalogue().len(),
            fixed(matcher.minimum_departure, 3),
            history.as_ref().map_or("off".to_owned(), |h| h.confirm_after.to_string()),
            if matcher.scene_composited { "on" } else { "off" },
        );

        let truth = &options.truth;
        let mut tally = Tally::new(truth.len());
        let mut evidence = Vec::new();
        let paths = strip_name::list(&options.directory).map_err(|e| format!("{}: {e}", options.directory.display()))?;

        for path in paths {
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if options.exclude.iter().any(|frame| name.contains(frame.as_str())) {
                continue;
            }
            let Some(strip) = strip_name::select(&name, &options.from, &options.to)? else { continue };

            let reading = read_strip(&path, &strip, &matcher, options.categories.as_deref(), history.as_mut(), truth.len())
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let mut marks = vec!['\0'; truth.len()];

            for (slot, read) in reading.confirmed.belt.iter().enumerate() {
                let names = read.item.as_ref().map(named);
                let verdict = judge(&truth[slot], names.as_deref());
                tally.record(slot, &verdict);
                marks[slot] = verdict.mark();

                if let (Verdict::Wrong { says, expected }, Some(found)) = (&verdict, &read.item) {
                    println!("  WRONG {name} slot {slot}: {says} {} (expected {expected})", fixed(found.score, 3));
                    evidence.push(Crop {
                        file: name.clone(),
                        slot: reading.geometry.belt()[slot],
                        origin_x: strip.origin_x,
                        origin_y: strip.origin_y,
                        label: format!("{slot}: {says} {}", fixed(found.score, 2)),
                    });
                }
            }

            tally.record_frame(&strip.date, &marks);
        }

        if tally.total() == 0 {
            println!("No strips matched in {}.", options.directory.display());
            return Ok(1);
        }

        if options.dump {
            println!("\nper frame (. correct, - abstain, X wrong):");
            for line in &tally.per_frame {
                println!("{line}");
            }
        }

        for line in tally.report(truth) {
            println!("{line}");
        }

        if let Some(path) = &options.montage {
            montage::write_and_report(&options.directory, &evidence, path).map_err(|e| format!("montage: {e}"))?;
        }

        Ok(if tally.wrong == 0 { 0 } else { 2 })
    }
}
