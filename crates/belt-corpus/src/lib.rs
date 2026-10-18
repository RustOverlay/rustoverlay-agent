//! Scores recorded belt strips against a hotbar the player held fixed.
//! See docs/matcher-tools.md and docs/item-matching.md.

pub mod args;
pub mod dotnet_format;
pub mod score;
pub mod strip_frame;
pub mod strip_name;

#[cfg(windows)]
mod gdiplus;
#[cfg(windows)]
pub mod montage;
#[cfg(windows)]
pub mod session;
