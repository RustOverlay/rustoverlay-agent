use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// KSDATAFORMAT_SUBTYPE_IEEE_FLOAT in on-disk byte order.
const SUBTYPE_IEEE_FLOAT: [u8; 16] = [3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71];

/// Streams interleaved float32 frames to a WAV file. Sizes in the header are patched by
/// `update_header`, so a file killed between updates is still readable up to the last one.
pub struct WavWriter {
    file: BufWriter<File>,
    channels: u16,
    frames: u64,
    fact_offset: u64,
    data_offset: u64,
}

impl WavWriter {
    pub fn create(path: &Path, rate: u32, channels: u16, channel_mask: u32) -> io::Result<Self> {
        let mut file = BufWriter::new(File::create(path)?);
        // Plain IEEE float is the most widely readable; anything beyond stereo needs the mask.
        let extensible = channels > 2;
        let block_align = channels * 4;

        file.write_all(b"RIFF\0\0\0\0WAVEfmt ")?;
        file.write_all(&(if extensible { 40u32 } else { 18 }).to_le_bytes())?;
        file.write_all(&(if extensible { 0xFFFEu16 } else { 3 }).to_le_bytes())?;
        file.write_all(&channels.to_le_bytes())?;
        file.write_all(&rate.to_le_bytes())?;
        file.write_all(&(rate * u32::from(block_align)).to_le_bytes())?;
        file.write_all(&block_align.to_le_bytes())?;
        file.write_all(&32u16.to_le_bytes())?;
        if extensible {
            file.write_all(&22u16.to_le_bytes())?;
            file.write_all(&32u16.to_le_bytes())?;
            file.write_all(&channel_mask.to_le_bytes())?;
            file.write_all(&SUBTYPE_IEEE_FLOAT)?;
        } else {
            file.write_all(&0u16.to_le_bytes())?;
        }

        file.write_all(b"fact")?;
        file.write_all(&4u32.to_le_bytes())?;
        let fact_offset = file.stream_position()?;
        file.write_all(&0u32.to_le_bytes())?;
        file.write_all(b"data\0\0\0\0")?;
        let data_offset = file.stream_position()?;

        Ok(Self { file, channels, frames: 0, fact_offset, data_offset })
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn write_samples(&mut self, samples: &[f32]) -> io::Result<()> {
        for sample in samples {
            self.file.write_all(&sample.to_le_bytes())?;
        }
        self.frames += (samples.len() / usize::from(self.channels)) as u64;
        Ok(())
    }

    pub fn write_silence(&mut self, frames: u64) -> io::Result<()> {
        let zeros = vec![0u8; usize::from(self.channels) * 4];
        for _ in 0..frames {
            self.file.write_all(&zeros)?;
        }
        self.frames += frames;
        Ok(())
    }

    pub fn update_header(&mut self) -> io::Result<()> {
        let data_bytes = self.frames * u64::from(self.channels) * 4;
        let data = u32::try_from(data_bytes).unwrap_or(u32::MAX);
        let riff = u32::try_from(data_bytes + self.data_offset - 8).unwrap_or(u32::MAX);
        let frames = u32::try_from(self.frames).unwrap_or(u32::MAX);

        self.file.flush()?;
        let file = self.file.get_mut();
        for (offset, value) in [(4, riff), (self.fact_offset, frames), (self.data_offset - 4, data)] {
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&value.to_le_bytes())?;
        }
        file.seek(SeekFrom::End(0))?;
        file.flush()
    }
}
