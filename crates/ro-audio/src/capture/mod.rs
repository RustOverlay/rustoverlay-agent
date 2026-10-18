//! WASAPI loopback capture: whole-endpoint, or one process tree (Windows 10 2004+), with packets
//! timestamped on QPC and mapped to UTC. Shared by calib-recorder and the live locator.

pub mod clock;
pub mod endpoint;
mod game;
mod loopback;
pub mod process;
mod process_loopback;
mod timeline;

pub use game::{run_game_capture, CaptureStatus};
pub use loopback::{device_mix_format, LoopbackCapture, MixFormat, Packet, FALLBACK_FORMAT};
pub use timeline::{Gap, Placement, Timeline};

pub const FLAG_DISCONTINUITY: u32 = windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32;
pub const FLAG_SILENT: u32 = windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_SILENT.0 as u32;
pub const FLAG_TIMESTAMP_ERROR: u32 = windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32;
/// Ours, not WASAPI's: the packet's QPC is a retrieval-time estimate (see LoopbackCapture::drain).
pub const FLAG_QPC_ESTIMATED: u32 = 0x8000_0000;
