// Records what the player hears with a QPC→UTC timeline, for aligning against server-side gunshot
// ground truth: WASAPI loopback of the default render endpoint, or with --process/--game only what
// one process tree renders (process loopback, Windows 10 2004+). See docs/research/locate/README.md §4
// and tools/calib/align.py.

#[cfg(windows)]
mod endpoint;
#[cfg(windows)]
mod recorder;
#[cfg(windows)]
mod session;
#[cfg(windows)]
mod wav;

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("calib-recorder uses WASAPI loopback and runs on Windows only.");
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use ro_audio::capture::endpoint::default_render_device;
    use ro_audio::capture::process::{self, TargetProcess};
    use ro_audio::capture::{self, LoopbackCapture};
    use ro_core::hosting::settings_file::data_directory;
    use serde_json::json;
    use windows::core::BOOL;
    use windows::Win32::System::Console::SetConsoleCtrlHandler;

    use crate::recorder::Recorder;
    use crate::session::utc_iso;
    use crate::{endpoint, session};

    const USAGE: &str = "usage: calib-recorder [--out DIR] [--seconds N] [--process [NAME|PID] | --game]\n  \
        --process/--game capture only that process tree (default RustClient.exe); without them, the whole default endpoint";
    const POLL: Duration = Duration::from_millis(5);
    const STATUS_EVERY: Duration = Duration::from_secs(5);

    static STOP: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn on_console_ctrl(_kind: u32) -> BOOL {
        STOP.store(true, Ordering::SeqCst);
        BOOL::from(true)
    }

    pub fn run(args: &[String]) -> Result<(), String> {
        let Args { out, seconds, process } = parse_args(args)?;
        let target = process.as_deref().map(process::find).transpose()?;
        let dir = out.unwrap_or_else(|| {
            data_directory().join("calib").join(chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string())
        });
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        unsafe { SetConsoleCtrlHandler(Some(on_console_ctrl), true) }.map_err(|e| format!("SetConsoleCtrlHandler: {e}"))?;

        let device = default_render_device()?;
        let capture = open_capture(&device, target.as_ref())?;
        let mut recorder = Recorder::create(&dir, capture.format)?;

        let started = recorder.now_utc_100ns();
        let mut info = json!({
            "capture_mode": if target.is_some() { "process" } else { "endpoint" },
            "target_process": target.as_ref().map(|t: &TargetProcess| json!({ "name": t.name, "pid": t.pid })),
            "endpoint": endpoint::describe(&device),
            "mix_format": session::mix_format_json(&capture.format),
            "rust_client_cfg": session::rust_audio_convars(),
            "start_utc": utc_iso(started),
            "start_utc_100ns": started,
            "stop_utc": null,
            "utc_100ns_epoch": "unix",
        });
        let session_path = dir.join("session.json");
        session::write(&session_path, &info)?;

        let f = &capture.format;
        let source = match &target {
            Some(t) => format!("{} (pid {}) process tree", t.name, t.pid),
            None => info["endpoint"]["name"].to_string(),
        };
        println!("Recording {} Hz x{} from {source} to {}", f.rate, f.channels, dir.display());
        println!("{}", match seconds {
            Some(n) => format!("Stops after {n} s or on Ctrl+C."),
            None => "Stops on Ctrl+C.".to_owned(),
        });

        capture.start()?;
        let outcome = capture_loop(&capture, &mut recorder, seconds);
        drop(capture);

        let stopped = recorder.now_utc_100ns();
        info["stats"] = recorder.finish()?;
        info["stop_utc"] = json!(utc_iso(stopped));
        info["stop_utc_100ns"] = json!(stopped);
        if let Err(e) = &outcome {
            info["error"] = json!(e);
        }
        session::write(&session_path, &info)?;

        println!("done — {:.1} s in {}", info["stats"]["seconds"].as_f64().unwrap_or(0.0), dir.display());
        outcome
    }

    fn capture_loop(capture: &LoopbackCapture, recorder: &mut Recorder, seconds: Option<u64>) -> Result<(), String> {
        let started = Instant::now();
        let deadline = seconds.map(|n| started + Duration::from_secs(n));
        let mut next_status = started + STATUS_EVERY;

        while !STOP.load(Ordering::SeqCst) && deadline.is_none_or(|d| Instant::now() < d) {
            capture.drain(|packet| recorder.packet(packet))?;
            recorder.refresh_clock()?;

            if Instant::now() >= next_status {
                next_status += STATUS_EVERY;
                recorder.flush()?;
                let peaks: Vec<String> = recorder.take_peaks_dbfs().iter().map(|db| format!("{db:6.1}")).collect();
                println!(
                    "{:6.1} s wall  {:6.1} s audio  peak dBFS [{}]",
                    started.elapsed().as_secs_f64(),
                    recorder.seconds_recorded(),
                    peaks.join(" ")
                );
            }
            capture.wait(POLL);
        }
        capture.drain(|packet| recorder.packet(packet))
    }

    /// Process mode asks for the endpoint's own mix format, so both modes produce comparable files.
    fn open_capture(device: &windows::Win32::Media::Audio::IMMDevice, target: Option<&TargetProcess>) -> Result<LoopbackCapture, String> {
        let Some(target) = target else {
            return LoopbackCapture::open(device);
        };
        let format = capture::device_mix_format(device)
            .ok()
            .filter(|f| f.float32 && f.channels > 0)
            .unwrap_or(capture::FALLBACK_FORMAT);
        LoopbackCapture::open_process(target.pid, format)
    }

    struct Args {
        out: Option<PathBuf>,
        seconds: Option<u64>,
        process: Option<String>,
    }

    fn parse_args(args: &[String]) -> Result<Args, String> {
        let mut parsed = Args { out: None, seconds: None, process: None };
        let mut rest = args.iter().peekable();
        while let Some(flag) = rest.next() {
            let mut value = || rest.next().ok_or_else(|| format!("{flag} needs a value\n{USAGE}"));
            match flag.as_str() {
                "--out" => parsed.out = Some(PathBuf::from(value()?)),
                "--seconds" => {
                    let text = value()?;
                    parsed.seconds = Some(text.parse().map_err(|_| format!("--seconds: '{text}' is not a whole number"))?);
                }
                "--game" => parsed.process = Some(process::GAME_CLIENT.to_owned()),
                "--process" => {
                    let name = rest.next_if(|next| !next.starts_with("--"));
                    parsed.process = Some(name.map_or(process::GAME_CLIENT.to_owned(), Clone::clone));
                }
                _ => return Err(format!("unknown argument '{flag}'\n{USAGE}")),
            }
        }
        Ok(parsed)
    }
}
