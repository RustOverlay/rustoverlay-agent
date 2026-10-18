//! Reads a calib-recorder session back as the packets it was captured in (see tools/calib/align.py).

use std::path::Path;

use ro_audio::AudioBlock;

pub struct Recording {
    pub rate: u32,
    /// Interleaved stereo.
    pub samples: Vec<f32>,
    /// (first frame, frames, UTC 100 ns) per captured packet or gap fill, in stream order.
    pub packets: Vec<(u64, u64, i64)>,
}

impl Recording {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let info: serde_json::Value = serde_json::from_str(&read_text(&dir.join("session.json"))?)
            .map_err(|e| format!("session.json: {e}"))?;
        let rate = info["mix_format"]["rate"].as_u64().ok_or("session.json: no mix_format.rate")? as u32;
        let channels = info["mix_format"]["channels"].as_u64().ok_or("session.json: no mix_format.channels")?;
        if channels != 2 {
            return Err(format!("session is {channels}-channel; the locator handles stereo only"));
        }
        let samples = read_float_wav(&dir.join("audio.wav"))?;
        let packets = read_timing(&read_text(&dir.join("timing.csv"))?, samples.len() as u64 / 2)?;
        Ok(Self { rate, samples, packets })
    }

    /// The session as the live capture would have delivered it, one block per packet.
    pub fn blocks(&self) -> impl Iterator<Item = AudioBlock> + '_ {
        self.packets.iter().map(|&(frame, frames, utc)| AudioBlock {
            utc_100ns: utc,
            rate: self.rate,
            samples: self.samples[frame as usize * 2..(frame + frames) as usize * 2].to_vec(),
        })
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// A live session's last row can be half-written or run past the WAV's last header update.
fn read_timing(text: &str, total_frames: u64) -> Result<Vec<(u64, u64, i64)>, String> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().ok_or("timing.csv: empty")?.split(',').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).ok_or(format!("timing.csv: no {name} column"));
    let (frame_at, frames_at, utc_at) = (col("frame_index")?, col("frames")?, col("utc_100ns")?);
    let mut rows = Vec::new();
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        let parsed = (|| Some((cells.get(frame_at)?.parse().ok()?, cells.get(frames_at)?.parse().ok()?, cells.get(utc_at)?.parse().ok()?)))();
        match parsed {
            Some((frame, frames, utc)) if frame + frames <= total_frames => rows.push((frame, frames, utc)),
            _ => break,
        }
    }
    Ok(rows)
}

/// Float32 WAV data, walking chunks to the `data` one.
fn read_float_wav(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{}: not a RIFF/WAVE file", path.display()));
    }
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().expect("4 bytes")) as usize;
        if &bytes[at..at + 4] == b"data" {
            // The header's size can lag a recording that was killed; trust the file length.
            let data = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
            return Ok(data.as_chunks::<4>().0.iter().map(|&b| f32::from_le_bytes(b)).collect());
        }
        at += 8 + size + (size & 1);
    }
    Err(format!("{}: no data chunk", path.display()))
}
