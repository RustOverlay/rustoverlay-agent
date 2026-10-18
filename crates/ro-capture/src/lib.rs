//! Where frames come from, the GDI+ rendering the matcher's thresholds were tuned against, and the
//! capture loop. Windows-only by necessity. See docs/capture-loop.md and docs/architecture.md.

#![cfg(windows)]

pub mod bitmap_frames;
pub mod display_scaling;
mod duplication_frame_source;
mod duplication_session;
mod fallback_frame_source;
mod fetched_skins;
mod font_glyph_atlas;
pub mod frame_image;
mod frame_source;
mod game_skin_icons;
pub mod gdiplus;
pub mod icon_signatures;
mod key_watcher;
pub mod rust_window;
mod retry_cache;
mod screen_frame_source;
mod signature_cache;
mod skin_hunt;
mod skin_watcher;
mod stop_token;
mod vision_pass;
mod vision_service;

pub use duplication_frame_source::{is_available as duplication_is_available, DuplicationFrameSource};
pub use fallback_frame_source::FallbackFrameSource;
pub use fetched_skins::{skin_signatures, ArrivedIcons, SkinIconSource};
pub use font_glyph_atlas::FontGlyphAtlas;
pub use frame_source::{CaptureError, Frame, FrameSource};
pub use game_skin_icons::GameSkinIcons;
pub use key_watcher::{virtual_key, KeyWatcher};
pub use screen_frame_source::ScreenFrameSource;
pub use signature_cache::SignatureCache;
pub use skin_watcher::SkinWatcher;
pub use stop_token::{StopToken, Stopped};
pub use vision_service::{CaptureDiagnostics, VisionService};
