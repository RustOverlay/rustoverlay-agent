use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ro_vision::hud::{GlyphAtlas, GlyphSet, GlyphTemplate};
use windows::Win32::Graphics::GdiPlus::{
    FontStyleBold, InterpolationModeHighQualityBicubic, PixelOffsetModeHighQuality, TextRenderingHintAntiAlias,
};

use crate::gdiplus::{Bitmap, Font, GdiplusError, Graphics, PrivateFonts};
use crate::retry_cache::RetryCache;

/// Roboto Condensed Bold, Apache-2.0. Google's, so it may ship; Facepunch's icons may not.
// TODO(port): move the font under crates/ when the C# tree is deleted.
static FONT: &[u8] = include_bytes!("../assets/RobotoCondensed-Bold.ttf");

/// Every character a count can contain.
const CHARACTERS: &str = "0123456789x";

/// Rendered large, then shrunk. Rasterising straight at 12px loses the thin strokes.
const CANVAS: i32 = 256;
const FONT_SIZE: f32 = 96.0;
const ORIGIN: i32 = 20;

const BLACK: u32 = 0xFF00_0000;
const WHITE: u32 = 0xFFFF_FFFF;

/// Renders count glyphs from Roboto Condensed Bold, the typeface Rust draws its HUD in.
///
/// Identified from 191 references in the game's bundles plus an asset named
/// `pickupcondensedamount`, then confirmed by measurement: against 24 labelled glyphs Roboto scored
/// 20 while every substitute collapsed to 12-14, which is the signature of a correct identification
/// rather than a lucky one.
pub struct FontGlyphAtlas {
    /// Locked because `InventoryReader` reads slots in parallel and every one of them asks for an
    /// atlas. Gotcha: the lock also keeps GDI+ from being driven concurrently over one collection.
    inner: Mutex<Inner>,
}

struct Inner {
    fonts: PrivateFonts,
    cache: RetryCache<i32, GlyphSet>,
    empty: Arc<GlyphSet>,
}

/// How long a height whose render failed stays without glyphs before it is rendered again.
const RETRY_AFTER: Duration = Duration::from_secs(30);

impl FontGlyphAtlas {
    pub fn new() -> Result<Self, GdiplusError> {
        let fonts = PrivateFonts::from_memory(FONT)?;
        let inner = Inner { fonts, cache: RetryCache::new(RETRY_AFTER), empty: Arc::new(GlyphSet::new()) };
        Ok(Self { inner: Mutex::new(inner) })
    }
}

impl GlyphAtlas for FontGlyphAtlas {
    fn at(&self, digit_height: i32) -> Arc<GlyphSet> {
        let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let Inner { fonts, cache, empty } = &mut *inner;

        // TODO(port): C# threw here, failing the frame; an empty set makes CountReader abstain.
        cache.get_or_try(digit_height, Instant::now(), || render(fonts, digit_height)).unwrap_or_else(|first| {
            if let Some(error) = first {
                tracing::error!(%error, digit_height, "Could not render count glyphs; retrying every {RETRY_AFTER:?}");
            }
            Arc::clone(empty)
        })
    }
}

fn render(fonts: &PrivateFonts, digit_height: i32) -> Result<GlyphSet, GdiplusError> {
    let font = Font::new(fonts, FONT_SIZE, FontStyleBold)?;
    let canvas = Bitmap::new_argb(CANVAS, CANVAS)?;
    let g = Graphics::from_image(&canvas)?;
    g.set_text_rendering_hint(TextRenderingHintAntiAlias)?;

    // Cap top to baseline, taken from '0'. Every glyph is cut to this same box, so 'x' arrives
    // short with blank above it rather than stretched to digit height.
    draw(&g, &font, '0')?;

    let (mut cap_top, mut cap_bottom) = (CANVAS, -1);
    for y in 0..CANVAS {
        for x in 0..CANVAS {
            if red(canvas.pixel(x, y)?) > 128 {
                cap_top = cap_top.min(y);
                cap_bottom = cap_bottom.max(y);
                break;
            }
        }
    }

    let mut glyphs = GlyphSet::new();
    if cap_bottom < 0 {
        return Ok(glyphs);
    }

    let box_height = cap_bottom - cap_top + 1;
    let scale = f64::from(digit_height) / f64::from(box_height);

    for ch in CHARACTERS.chars() {
        let advance = f64::from(g.measure_width_typographic(&ch.to_string(), &font)?);
        let width = ((advance * scale).round_ties_even() as i32).max(1);

        draw(&g, &font, ch)?;

        let small = Bitmap::new_argb(width, digit_height)?;
        {
            let shrink = Graphics::from_image(&small)?;
            shrink.set_interpolation_mode(InterpolationModeHighQualityBicubic)?;
            shrink.set_pixel_offset_mode(PixelOffsetModeHighQuality)?;
            shrink.draw_image_rect_rect(
                &canvas,
                [0, 0, width, digit_height],
                [ORIGIN, cap_top, advance.round_ties_even() as i32, box_height],
            )?;
        }

        let mut cell = Vec::with_capacity((width * digit_height) as usize);
        for y in 0..digit_height {
            for x in 0..width {
                cell.push(f64::from(red(small.pixel(x, y)?)));
            }
        }

        glyphs.insert(ch, GlyphTemplate { cell, width, height: digit_height });
    }

    Ok(glyphs)
}

/// Typographic layout so the origin is the advance box, not a padded rendering box.
fn draw(g: &Graphics, font: &Font, ch: char) -> Result<(), GdiplusError> {
    g.clear(BLACK)?;
    g.draw_string_typographic(&ch.to_string(), font, WHITE, ORIGIN as f32, ORIGIN as f32)
}

fn red(argb: u32) -> u8 {
    (argb >> 16) as u8
}
