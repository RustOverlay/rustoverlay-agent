//! Contact sheet of individual slots.
//!
//! Not decoration. A score sheet cannot tell a wrong read from a wrong label, and the labels come
//! from a hotbar the player was asked to leave alone — an assumption that breaks the moment they
//! use something. Every finding worth acting on was confirmed by looking at the slots first.

use std::path::Path;

use ro_capture::gdiplus::GdiplusError;
use ro_vision::hud::SlotRect;
use windows::Win32::Graphics::GdiPlus::{InterpolationModeNearestNeighbor, PixelOffsetModeHalf};

use crate::gdiplus::{Canvas, Font, Image};

const CELL: i32 = 190;
const CAPTION: i32 = 24;
const COLUMNS: i32 = 5;

/// One slot of one recorded strip, blown up and labelled.
#[derive(Clone, Debug)]
pub struct Crop {
    pub file: String,
    pub slot: SlotRect,
    pub origin_x: i32,
    pub origin_y: i32,
    pub label: String,
}

/// Writes the sheet and reports it on stdout; nothing at all when there are no crops.
pub fn write_and_report(directory: &Path, crops: &[Crop], path: &Path) -> Result<(), GdiplusError> {
    if crops.is_empty() {
        return Ok(());
    }

    let rows = (crops.len() as i32 + COLUMNS - 1) / COLUMNS;
    let sheet = Image::new(COLUMNS * (CELL + 8), rows * (CELL + CAPTION + 8))?;
    {
        let graphics = Canvas::on(&sheet)?;
        let font = Font::system("Consolas", 10.0)?;

        graphics.clear(0xFF18_181C)?;
        // Nearest neighbour and half-pixel offset: this is evidence, so it must show the captured
        // pixels rather than a smoothed impression of them.
        graphics.set_modes(InterpolationModeNearestNeighbor, PixelOffsetModeHalf)?;

        for (i, crop) in crops.iter().enumerate() {
            let i = i as i32;
            let bitmap = Image::from_file(&directory.join(format!("{}.png", crop.file)))?;
            let x = i % COLUMNS * (CELL + 8);
            let y = i / COLUMNS * (CELL + CAPTION + 8);

            graphics.draw_image(
                &bitmap,
                [x, y + CAPTION, CELL, CELL],
                [crop.slot.x - crop.origin_x, crop.slot.y - crop.origin_y, crop.slot.size, crop.slot.size],
            )?;
            graphics.draw_string(&crop.label, &font, 0xFFFF_FFFF, (x + 2) as f32, (y + 4) as f32)?;
        }
    }

    sheet.save_png(path)?;
    println!("wrote {} ({} slots)", path.display(), crops.len());
    Ok(())
}
