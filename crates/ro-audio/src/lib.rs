//! Single-player gunshot locator from the game's own audio: process-loopback capture of
//! RustClient.exe, a streaming onset detector, and a calibrated inversion of each shot's features
//! into a bearing pair and a distance. See docs/research/locate/locator.md.

mod block;
#[cfg(windows)]
pub mod capture;
pub mod detect;
pub mod estimate;
mod feed;
#[cfg(windows)]
pub mod live;
mod locator;
pub mod profile;
mod table;

pub use block::AudioBlock;
pub use detect::{Detection, Detector, ShotFeatures};
pub use estimate::{estimate, Distance, ShotEstimate};
pub use feed::ShotFeed;
pub use locator::{Located, Locator, Rejection};
pub use profile::AudioProfile;
pub use table::Table;
