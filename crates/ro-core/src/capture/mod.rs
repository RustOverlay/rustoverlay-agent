//! GPU-free capture bookkeeping, kept apart from the D3D code so it can be tested.

mod backend_health;
pub mod output_selection;

pub use backend_health::BackendHealth;
pub use output_selection::ScreenRect;
