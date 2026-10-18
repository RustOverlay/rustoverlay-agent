use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::clock::Clock;
use super::endpoint::default_render_device;
use super::process::{self, GAME_CLIENT};
use super::{device_mix_format, LoopbackCapture, MixFormat, Timeline, FALLBACK_FORMAT};
use crate::AudioBlock;

const POLL: Duration = Duration::from_millis(20);
const RETRY: Duration = Duration::from_secs(2);
/// How often the process table is walked for a game that is not running. Each walk is ~25 ms of
/// kernel time; a few seconds' delay before the first shot after launch costs nothing.
const WAIT_FOR_GAME: Duration = Duration::from_secs(10);
/// How often the game is checked for having exited, and the QPC→UTC pair refreshed.
const CHECK_EVERY: Duration = Duration::from_secs(2);
/// Longer silences are not padded in full; the next block's timestamp carries the jump.
const MAX_GAP_FRAMES: u64 = 48_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureStatus {
    WaitingForGame,
    /// A new stream started: whatever was mid-detection belongs to the old one.
    Capturing { pid: u32, rate: u32 },
    Failed(String),
}

impl std::fmt::Display for CaptureStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WaitingForGame => write!(f, "Waiting for {GAME_CLIENT}"),
            Self::Capturing { pid, rate } => write!(f, "Capturing {GAME_CLIENT} (pid {pid}) at {rate} Hz"),
            Self::Failed(e) => write!(f, "Audio capture failed: {e}"),
        }
    }
}

/// Captures what RustClient.exe renders until `stop` is set, reconnecting whenever the game
/// restarts. Blocks are stereo, gap-padded, and stamped with UTC from the QPC mapping.
pub fn run_game_capture(stop: &AtomicBool, on_block: &mut dyn FnMut(AudioBlock), on_status: &mut dyn FnMut(CaptureStatus)) {
    while !stop.load(Ordering::SeqCst) {
        let Ok(target) = process::find(GAME_CLIENT) else {
            on_status(CaptureStatus::WaitingForGame);
            pause(stop, WAIT_FOR_GAME);
            continue;
        };
        let opened = open(target.pid);
        let result = opened.and_then(|capture| {
            on_status(CaptureStatus::Capturing { pid: target.pid, rate: capture.format.rate });
            stream(&capture, target.pid, stop, on_block)
        });
        if let Err(e) = result {
            tracing::warn!(error = %e, "Game audio capture failed; retrying");
            on_status(CaptureStatus::Failed(e));
            pause(stop, RETRY);
        }
    }
}

/// Process loopback in the endpoint's mix format, as calib-recorder records it: the profile was
/// measured on a stereo mix, so any other layout is refused rather than silently mis-read.
fn open(pid: u32) -> Result<LoopbackCapture, String> {
    let device = default_render_device()?;
    let format: MixFormat = device_mix_format(&device).ok().filter(|f| f.float32 && f.channels > 0).unwrap_or(FALLBACK_FORMAT);
    if format.channels != 2 {
        return Err(format!(
            "the default output mixes {} channels; the locator is calibrated for stereo only (set the Windows output to stereo)",
            format.channels
        ));
    }
    LoopbackCapture::open_process(pid, format)
}

/// Returns Ok when the game has gone or restarted, so the caller reconnects.
fn stream(capture: &LoopbackCapture, pid: u32, stop: &AtomicBool, on_block: &mut dyn FnMut(AudioBlock)) -> Result<(), String> {
    let rate = capture.format.rate;
    let game = process::Watched::open(pid)?;
    let clock = Clock::new();
    let mut pair = clock.sample();
    let mut timeline = Timeline::new(rate);
    let mut next_check = Instant::now() + CHECK_EVERY;
    capture.start()?;

    while !stop.load(Ordering::SeqCst) {
        capture.wait(POLL);
        capture.drain(|packet| {
            let placed = timeline.place(&packet);
            if let Some(gap) = placed.gap {
                let frames = gap.frames.min(MAX_GAP_FRAMES) as usize;
                on_block(AudioBlock { utc_100ns: pair.utc_of(gap.qpc_100ns), rate, samples: vec![0.0; frames * 2] });
            }
            let samples = if packet.samples.is_empty() { vec![0.0; packet.frames as usize * 2] } else { packet.samples.to_vec() };
            on_block(AudioBlock { utc_100ns: pair.utc_of(placed.start_qpc_100ns), rate, samples });
            Ok(())
        })?;

        if Instant::now() >= next_check {
            next_check += CHECK_EVERY;
            pair = clock.sample();
            if game.has_exited() {
                tracing::info!(pid, "Rust client exited; reconnecting audio capture");
                return Ok(());
            }
        }
    }
    Ok(())
}

fn pause(stop: &AtomicBool, total: Duration) {
    let until = Instant::now() + total;
    while !stop.load(Ordering::SeqCst) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
}
