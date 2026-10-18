//! Display state for the views, computed here so the views only draw (the C# view models).
//!
//! Models that background threads update hold their state behind a mutex and call their `Notify`
//! afterwards; the app wires that to a repaint of whichever window draws them.

use std::sync::Arc;

pub mod account;
pub mod agent;
pub mod feed;
pub mod inspect;
pub mod loadout_view;
pub mod overlay;
pub mod overlay_member;
pub mod settings_form;
pub mod shot_view;
pub mod sign_in;
pub mod team;
pub mod team_member;
mod team_refresh;

/// Asks for a repaint after a change made off the UI thread.
pub type Notify = Arc<dyn Fn() + Send + Sync>;
