//! Reads a calib-recorder session back as the packets it was captured in (see tools/calib/align.py).
//!
//! Streamed from disk a packet at a time: a 12-minute session is 277 MB of samples, and the replay
//! runs beside the game and its server.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use ro_audio::AudioBlock;

const BYTES_PER_FRAME: u64 = 2 * 4;

pub struct Recording {
    pub rate: u32,
    /// Frames in the WAV's data chunk, which can run past the last header update of a killed recording.
    pub frames: u64,
    /// (first frame, frames, UTC 100 ns) per captured packet or gap fill, in stream order.
    pub packets: Vec<(u64, u64, i64)>,
    wav: PathBuf,
    data_offset: u64,
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
        let wav = dir.join("audio.wav");
        let (data_offset, data_len) = float_wav_data(&wav)?;
        let frames = data_len / BYTES_PER_FRAME;
        let packets = read_timing(&read_text(&dir.join("timing.csv"))?, frames)?;
        Ok(Self { rate, frames, packets, wav, data_offset })
    }

    /// The session as the live capture would have delivered it, one block per packet. Ends early,
    /// with a message on stderr, if the WAV cannot be read.
    pub fn blocks(&self) -> impl Iterator<Item = AudioBlock> + '_ {
        let mut reader = File::open(&self.wav)
            .and_then(|mut f| f.seek(SeekFrom::Start(self.data_offset)).map(|_| BufReader::new(f)))
            .map_err(|e| eprintln!("{}: {e}", self.wav.display()))
            .ok();
        let mut next_frame = 0u64;
        self.packets.iter().map_while(move |&(frame, frames, utc)| {
            let r = reader.as_mut()?;
            // Packets are contiguous in stream order; a gap would mean timing.csv and the WAV disagree.
            if frame != next_frame {
                r.seek_relative(((frame as i64) - next_frame as i64) * BYTES_PER_FRAME as i64).ok()?;
            }
            let mut bytes = vec![0u8; (frames * BYTES_PER_FRAME) as usize];
            r.read_exact(&mut bytes).map_err(|e| eprintln!("{}: {e}", self.wav.display())).ok()?;
            next_frame = frame + frames;
            Some(AudioBlock {
                utc_100ns: utc,
                rate: self.rate,
                samples: bytes.as_chunks::<4>().0.iter().map(|&b| f32::from_le_bytes(b)).collect(),
            })
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

/// (offset, length) of a float32 WAV's `data` chunk, walking the chunks to it.
fn float_wav_data(path: &Path) -> Result<(u64, u64), String> {
    let fail = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut file = File::open(path).map_err(fail)?;
    let file_len = file.metadata().map_err(fail)?.len();
    let mut riff = [0u8; 12];
    file.read_exact(&mut riff).map_err(fail)?;
    if &riff[..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(format!("{}: not a RIFF/WAVE file", path.display()));
    }
    let mut at = 12u64;
    while at + 8 <= file_len {
        let mut chunk = [0u8; 8];
        file.seek(SeekFrom::Start(at)).map_err(fail)?;
        file.read_exact(&mut chunk).map_err(fail)?;
        let size = u64::from(u32::from_le_bytes(chunk[4..8].try_into().expect("4 bytes")));
        if &chunk[..4] == b"data" {
            // The header's size can lag a recording that was killed; trust the file length.
            return Ok((at + 8, size.min(file_len - (at + 8))));
        }
        at += 8 + size + (size & 1);
    }
    Err(format!("{}: no data chunk", path.display()))
}
