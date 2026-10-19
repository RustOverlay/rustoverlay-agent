//! Records the HUD compass strip beside the audio, so a yaw reader can be built and scored later
//! against the same sessions instead of new ones. See docs/research/locate/client-code.md §3.
//!
//! Grayscale PNGs of the strip at 20 Hz, plus `compass/index.csv` mapping each to its UTC capture
//! time. On its own thread: a frame grab must never delay draining the audio buffer.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ro_capture::{DuplicationFrameSource, FallbackFrameSource, Frame, FrameSource, ScreenFrameSource};
use ro_vision::hud::FrameBuffer;

const INTERVAL: Duration = Duration::from_millis(50);
/// The strip's rows at 1080 px height, scaled to the client height.
const STRIP_ROWS_AT_1080: f64 = 34.0;
/// The ±42° visible span is 0.37 of the width (W/225 px per degree); a little margin either side.
const STRIP_WIDTH_FRACTION: f64 = 0.40;

pub struct CompassRecorder {
    thread: JoinHandle<Result<u64, String>>,
}

impl CompassRecorder {
    pub fn start(session_dir: &Path, stop: &'static AtomicBool) -> Result<Self, String> {
        let dir = session_dir.join("compass");
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let thread = thread::Builder::new()
            .name("compass".into())
            .spawn(move || record(&dir, stop))
            .map_err(|e| format!("compass thread: {e}"))?;
        Ok(Self { thread })
    }

    /// Frames written, once the stop flag has ended the loop.
    pub fn finish(self) -> Result<u64, String> {
        self.thread.join().map_err(|_| "compass thread panicked".to_owned())?
    }
}

fn record(dir: &Path, stop: &AtomicBool) -> Result<u64, String> {
    let frames = FallbackFrameSource::new(Box::new(DuplicationFrameSource::new()), Box::new(ScreenFrameSource::new()));
    let index_path = dir.join("index.csv");
    let mut index = BufWriter::new(File::create(&index_path).map_err(|e| format!("{}: {e}", index_path.display()))?);
    writeln!(index, "seq,utc_100ns,client_width,client_height,strip_x,strip_width,strip_height").map_err(|e| e.to_string())?;

    let mut seq = 0u64;
    let mut next = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        let mut saved = Ok(None);
        // A failed grab (Rust minimised, device lost) is a gap in the strip, not a reason to stop recording audio.
        let _ = frames.read(&mut |frame| saved = save_strip(frame, &dir.join(format!("{seq:06}.png"))));
        if let Some((x, width, height, client)) = saved? {
            let utc = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) / 100;
            writeln!(index, "{seq},{utc},{},{},{x},{width},{height}", client.0, client.1).map_err(|e| e.to_string())?;
            seq += 1;
        }
        next += INTERVAL;
        thread::sleep(next.saturating_duration_since(Instant::now()));
    }
    index.flush().map_err(|e| e.to_string())?;
    Ok(seq)
}

type Saved = Option<(i32, i32, i32, (i32, i32))>;

fn save_strip(frame: &Frame<'_>, path: &PathBuf) -> Result<Saved, String> {
    let (w, h) = (frame.width(), frame.height());
    if w <= 0 || h <= 0 {
        return Ok(None);
    }
    let width = ((f64::from(w) * STRIP_WIDTH_FRACTION).round() as i32).min(w);
    let height = ((f64::from(h) * STRIP_ROWS_AT_1080 / 1080.0).ceil() as i32).clamp(1, h);
    let x0 = (w - width) / 2;

    let mut gray = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in x0..x0 + width {
            let p = frame.pixel(x, y);
            gray.push(((299 * u32::from(p.r) + 587 * u32::from(p.g) + 114 * u32::from(p.b)) / 1000) as u8);
        }
    }

    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&gray).map_err(|e| e.to_string())?;
    Ok(Some((x0, width, height, (w, h))))
}
