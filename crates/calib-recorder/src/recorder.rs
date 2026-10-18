use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use ro_audio::capture::clock::{Clock, ClockPair};
use ro_audio::capture::{
    MixFormat, Packet, Timeline, FLAG_DISCONTINUITY, FLAG_QPC_ESTIMATED, FLAG_SILENT, FLAG_TIMESTAMP_ERROR,
};
use serde_json::{json, Value};

use crate::wav::WavWriter;

const CLOCK_RESAMPLE_100NS: i64 = 100_000_000;

#[derive(Default)]
struct Counts {
    packets: u64,
    silent: u64,
    discontinuities: u64,
    timestamp_errors: u64,
    qpc_estimated: u64,
    gap_fills: u64,
    gap_frames: u64,
}

/// Retrieval-time estimate minus device QPC, over packets that have both.
#[derive(Default)]
struct EstimateError {
    n: u64,
    sum: f64,
    min: f64,
    max: f64,
}

impl EstimateError {
    fn add(&mut self, delta_100ns: f64) {
        let ms = delta_100ns / 10_000.0;
        (self.min, self.max) = if self.n == 0 { (ms, ms) } else { (self.min.min(ms), self.max.max(ms)) };
        self.n += 1;
        self.sum += ms;
    }

    fn to_json(&self) -> Value {
        if self.n == 0 {
            return Value::Null;
        }
        json!({ "packets": self.n, "mean": self.sum / self.n as f64, "min": self.min, "max": self.max })
    }
}

/// Writes audio.wav, timing.csv and clock.csv for one session. Gaps the [`Timeline`] finds are
/// padded with zeros and logged as `gap_fill`, keeping the WAV timeline continuous.
pub struct Recorder {
    wav: WavWriter,
    timing: BufWriter<File>,
    clock_log: BufWriter<File>,
    clock: Clock,
    pair: ClockPair,
    format: MixFormat,
    timeline: Timeline,
    peaks: Vec<f32>,
    counts: Counts,
    estimate_error: EstimateError,
}

impl Recorder {
    pub fn create(dir: &Path, format: MixFormat) -> Result<Self, String> {
        let open = |name: &str| File::create(dir.join(name)).map(BufWriter::new).map_err(|e| format!("{name}: {e}"));
        let wav = WavWriter::create(&dir.join("audio.wav"), format.rate, format.channels, format.channel_mask)
            .map_err(|e| format!("audio.wav: {e}"))?;
        let mut timing = open("timing.csv")?;
        let mut clock_log = open("clock.csv")?;
        writeln!(timing, "frame_index,frames,device_position,qpc_100ns,utc_100ns,flags").map_err(|e| e.to_string())?;
        writeln!(clock_log, "qpc_100ns,utc_100ns,bracket_100ns").map_err(|e| e.to_string())?;

        let clock = Clock::new();
        let pair = clock.sample();
        let mut recorder = Self {
            wav, timing, clock_log, clock, pair, format,
            timeline: Timeline::new(format.rate),
            peaks: vec![0.0; usize::from(format.channels)],
            counts: Counts::default(),
            estimate_error: EstimateError::default(),
        };
        recorder.log_pair()?;
        Ok(recorder)
    }

    pub fn now_utc_100ns(&self) -> i64 {
        self.clock.sample().utc_100ns
    }

    pub fn seconds_recorded(&self) -> f64 {
        self.wav.frames() as f64 / f64::from(self.format.rate)
    }

    pub fn packet(&mut self, packet: Packet<'_>) -> Result<(), String> {
        let estimated = packet.flags & FLAG_QPC_ESTIMATED != 0;
        let timestamp_ok = packet.flags & FLAG_TIMESTAMP_ERROR == 0;
        if packet.device_qpc_100ns != 0 {
            self.estimate_error.add((packet.estimate_100ns - packet.device_qpc_100ns as i64) as f64);
        }

        let placed = self.timeline.place(&packet);
        if let Some(gap) = placed.gap {
            self.row(self.wav.frames(), gap.frames, None, gap.qpc_100ns, "gap_fill")?;
            self.wav.write_silence(gap.frames).map_err(|e| format!("audio.wav: {e}"))?;
            self.counts.gap_fills += 1;
            self.counts.gap_frames += gap.frames;
        }

        let flags = describe_flags(packet.flags);
        self.row(self.wav.frames(), u64::from(packet.frames), Some(packet.device_position), placed.qpc_100ns, &flags)?;
        if packet.samples.is_empty() {
            self.wav.write_silence(u64::from(packet.frames)).map_err(|e| format!("audio.wav: {e}"))?;
        } else {
            self.wav.write_samples(packet.samples).map_err(|e| format!("audio.wav: {e}"))?;
            for frame in packet.samples.chunks_exact(self.peaks.len()) {
                for (peak, sample) in self.peaks.iter_mut().zip(frame) {
                    *peak = peak.max(sample.abs());
                }
            }
        }

        self.counts.packets += 1;
        self.counts.silent += u64::from(packet.flags & FLAG_SILENT != 0);
        self.counts.discontinuities += u64::from(packet.flags & FLAG_DISCONTINUITY != 0);
        self.counts.timestamp_errors += u64::from(!timestamp_ok);
        self.counts.qpc_estimated += u64::from(estimated);
        Ok(())
    }

    /// Takes a fresh QPC/UTC pair once the current one is ~10 s old.
    pub fn refresh_clock(&mut self) -> Result<(), String> {
        if self.clock.qpc_100ns() - self.pair.qpc_100ns >= CLOCK_RESAMPLE_100NS {
            self.pair = self.clock.sample();
            self.log_pair()?;
        }
        Ok(())
    }

    /// Per-channel peak since the last call, in dBFS.
    pub fn take_peaks_dbfs(&mut self) -> Vec<f64> {
        let db = self.peaks.iter().map(|&p| if p > 0.0 { 20.0 * f64::from(p).log10() } else { f64::NEG_INFINITY }).collect();
        self.peaks.iter_mut().for_each(|p| *p = 0.0);
        db
    }

    pub fn flush(&mut self) -> Result<(), String> {
        self.wav.update_header().map_err(|e| format!("audio.wav: {e}"))?;
        self.timing.flush().and_then(|()| self.clock_log.flush()).map_err(|e| e.to_string())
    }

    /// Finalizes every file and returns the session statistics for session.json.
    pub fn finish(mut self) -> Result<Value, String> {
        self.pair = self.clock.sample();
        self.log_pair()?;
        self.flush()?;
        let c = &self.counts;
        Ok(json!({
            "frames": self.wav.frames(),
            "seconds": self.seconds_recorded(),
            "packets": c.packets,
            "silent_packets": c.silent,
            "discontinuities": c.discontinuities,
            "timestamp_errors": c.timestamp_errors,
            "qpc_estimated_packets": c.qpc_estimated,
            "gap_fills": c.gap_fills,
            "gap_frames": c.gap_frames,
            "qpc_estimate_minus_device_ms": self.estimate_error.to_json(),
        }))
    }

    fn row(&mut self, frame: u64, frames: u64, device_position: Option<u64>, qpc: i64, flags: &str) -> Result<(), String> {
        let position = device_position.map(|p| p.to_string()).unwrap_or_default();
        let utc = self.pair.utc_of(qpc);
        writeln!(self.timing, "{frame},{frames},{position},{qpc},{utc},{flags}").map_err(|e| format!("timing.csv: {e}"))
    }

    fn log_pair(&mut self) -> Result<(), String> {
        let p = self.pair;
        writeln!(self.clock_log, "{},{},{}", p.qpc_100ns, p.utc_100ns, p.bracket_100ns).map_err(|e| format!("clock.csv: {e}"))
    }
}

fn describe_flags(flags: u32) -> String {
    [
        (FLAG_SILENT, "silent"),
        (FLAG_DISCONTINUITY, "discontinuity"),
        (FLAG_TIMESTAMP_ERROR, "timestamp_error"),
        (FLAG_QPC_ESTIMATED, "qpc_estimated"),
    ]
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join("|")
}
