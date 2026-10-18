//! Writes a captured frame to disk for HUD calibration work.
//!
//! Sits outside [`crate::FrameSource`] deliberately: saving a PNG is a debug affordance, not
//! something every backend should have to reimplement. Reading back through `FrameBuffer` costs a
//! couple of million calls, which is irrelevant for a button pressed by hand.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use ro_vision::hud::FrameBuffer;

use crate::gdiplus::{Bitmap, GdiplusError};

/// Subdirectory of the agent's data directory that saved frames live in.
pub const DIRECTORY_NAME: &str = "frames";

#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Gdiplus(GdiplusError),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Gdiplus(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SaveError {}

pub fn save<F: FrameBuffer + ?Sized>(frame: &F, directory: &Path) -> Result<PathBuf, SaveError> {
    std::fs::create_dir_all(directory).map_err(SaveError::Io)?;

    // Sortable, and unique per second without needing a counter.
    let path = directory.join(file_name(chrono::Local::now().naive_local(), frame.width(), frame.height()));

    to_bitmap(frame).and_then(|bitmap| bitmap.save_png(&path)).map_err(SaveError::Gdiplus)?;

    Ok(path)
}

fn file_name(now: chrono::NaiveDateTime, width: i32, height: i32) -> String {
    format!("frame-{}-{width}x{height}.png", now.format("%Y%m%d-%H%M%S"))
}

fn to_bitmap<F: FrameBuffer + ?Sized>(frame: &F) -> Result<Bitmap, GdiplusError> {
    let (width, height) = (frame.width(), frame.height());
    let mut bitmap = Bitmap::new_argb(width, height)?;

    bitmap.write_argb(width, height, |y, row| {
        for x in 0..width {
            let pixel = frame.pixel(x, y);
            let i = x as usize * 4;

            // Format32bppArgb is BGRA in memory.
            row[i..i + 4].copy_from_slice(&[pixel.b, pixel.g, pixel.r, 255]);
        }
    })?;

    Ok(bitmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_sort_by_time_and_carry_the_size() {
        let at = chrono::NaiveDate::from_ymd_opt(2026, 3, 7).unwrap().and_hms_opt(9, 5, 2).unwrap();

        assert_eq!(file_name(at, 1920, 1080), "frame-20260307-090502-1920x1080.png");
    }
}
