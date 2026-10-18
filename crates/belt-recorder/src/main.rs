// Records the belt strip of world-view frames while Rust is focused, for calibrating the matcher
// against backgrounds it has never seen. See docs/matcher-tools.md.

#[cfg(windows)]
mod console_log;
#[cfg(windows)]
mod png24;
mod strip;

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("belt-recorder captures the desktop and runs on Windows only.");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    match record::run(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
mod record {
    use std::ffi::{OsStr, OsString};
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use ro_capture::{rust_window, DuplicationFrameSource, FallbackFrameSource, FrameSource, ScreenFrameSource};
    use ro_core::config::ClientConfig;
    use ro_core::hosting::settings_file::data_directory;
    use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
    use ro_vision::hud::{inventory_is_open, FrameBuffer, InventoryGeometry};

    use crate::console_log::ConsoleLog;
    use crate::{png24, strip};

    pub fn run(args: &[OsString]) -> Result<(), String> {
        let argument = |at: usize| args.get(at).map(OsString::as_os_str);
        let output = argument(0).map_or_else(|| data_directory().join("belt-corpus"), PathBuf::from);
        let wanted: u32 = parse(argument(1).unwrap_or(OsStr::new("250")), "frames")?;
        let minutes: u32 = parse(argument(2).unwrap_or(OsStr::new("25")), "minutes")?;

        std::fs::create_dir_all(&output).map_err(|e| format!("{}: {e}", output.display()))?;
        ConsoleLog::install();

        // Recorded into the filename, because a corpus captured at one UI scale scores as nonsense
        // under another and nothing else in the strip reveals which was in force.
        let ui_scale = match SteamRustInstallLocator::new(None).locate().map(|i| i.client_config_path()) {
            Some(config) if config.is_file() => {
                ClientConfig::load(&config).map_err(|e| format!("{}: {e}", config.display()))?.ui_scale()
            }
            _ => 1.0,
        };

        // Its own instance rather than the running agent's: a process may only duplicate a given
        // output once, but two processes duplicating the same one are fine. The agent can stay open.
        let frames = FallbackFrameSource::new(Box::new(DuplicationFrameSource::new()), Box::new(ScreenFrameSource::new()));

        let deadline = Instant::now() + Duration::from_secs(u64::from(minutes) * 60);
        let mut previous: Option<Vec<u8>> = None;
        let (mut saved, mut skipped, mut unfocused) = (0u32, 0u32, 0u32);

        println!("Recording to {}", output.display());
        println!("Stops after {wanted} frames or {minutes} minutes. Alt-tab into Rust and fly.");

        while saved < wanted && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1200));

            // Privacy control before anything else: capture reads the composited desktop, so an
            // alt-tabbed player would otherwise be recorded browsing.
            if !rust_window::is_foreground() {
                unfocused += 1;
                continue;
            }

            let mut failure = None;
            let read = frames.read(&mut |frame| {
                match record_frame(frame, ui_scale, &output, &mut previous) {
                    Ok(true) => saved += 1,
                    Ok(false) => skipped += 1,
                    Err(e) => failure = Some(e),
                }
            });

            if let Some(message) = read.err().map(|e| e.to_string()).or(failure) {
                println!("  read failed: {message}");
            }

            print!("\rsaved {saved}  skipped {skipped}  unfocused {unfocused}   ");
            let _ = std::io::stdout().flush();
        }

        println!("\ndone — {saved} frames in {}", output.display());
        Ok(())
    }

    /// Saves the frame's belt strip unless the panel is open or the scene has not moved. True when
    /// a strip was written.
    fn record_frame<F: FrameBuffer + ?Sized>(
        frame: &F,
        ui_scale: f64,
        output: &Path,
        previous: &mut Option<Vec<u8>>,
    ) -> Result<bool, String> {
        let geometry = InventoryGeometry::new(frame.width(), frame.height(), ui_scale);

        // World view only. With the panel open every slot sits on the same flat grey, which is the
        // case that already works and would only dilute the corpus.
        if inventory_is_open(frame, &geometry) {
            return Ok(false);
        }

        let strip = strip::strip_of(frame, &geometry);
        let digest = strip::digest(frame, strip);

        if previous.as_deref().is_some_and(|p| strip::similar(p, &digest)) {
            return Ok(false);
        }
        *previous = Some(digest.to_vec());

        let name = format!(
            "belt-{}-{}x{}-at{},{}-ui{ui_scale}.png",
            chrono::Local::now().format("%Y%m%d-%H%M%S-%3f"),
            frame.width(),
            frame.height(),
            strip.x,
            strip.y,
        );

        png24::save(frame, strip, &output.join(name)).map_err(|e| e.to_string())?;
        Ok(true)
    }

    // TODO(port): stricter than C#'s int.Parse (no whitespace, '+' or negatives); fine for a dev tool.
    fn parse(text: &OsStr, what: &str) -> Result<u32, String> {
        text.to_str()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| format!("{what}: '{}' is not a whole number", text.to_string_lossy()))
    }
}
