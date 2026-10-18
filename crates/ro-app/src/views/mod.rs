//! Immediate-mode replacements for the WPF views. Each draws from the state modules and calls into
//! them on input; none computes anything of its own.

mod account;
mod debug_tab;
mod key_bind_box;
pub mod main_window;
pub mod overlay;
mod settings_tab;
pub mod sign_in;
mod team_member_card;
mod team_tab;
