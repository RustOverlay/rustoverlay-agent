//! Screen reading: HUD geometry, the vitals and count readers, and the icon matcher.
//!
//! Pure and platform-neutral. Pixels arrive through [`hud::FrameBuffer`] and glyphs through
//! [`hud::GlyphAtlas`]; `ro-capture` implements both. See docs/item-matching.md and
//! docs/hud-layout.md for why every threshold is what it is.

pub mod capture;
pub mod hosting;
pub mod hud;
pub mod items;

mod dotnet_sort;
mod text;
