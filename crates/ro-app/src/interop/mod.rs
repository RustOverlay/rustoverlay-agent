//! Win32 the overlay needs beyond what winit offers: registered hotkeys, the overlay window's
//! extended styles and placement, and key capture for the bind fields.

pub mod hotkey_binder;
pub mod key_capture;
pub mod message_box;
pub mod overlay_input;
mod own_window;
mod tap_key;
pub mod thread_priority;
mod window_styles;
