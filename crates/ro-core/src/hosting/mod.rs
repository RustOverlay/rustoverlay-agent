//! State and options shared by the agent's long-running services. The services themselves live in
//! ro-app; `LoadoutState` lives in ro-vision.

mod agent_options;
mod agent_status;
mod environment_layer;
mod event_queue;
mod game_event_bus;
mod json_text;
pub mod data_migration;
pub mod log_tail;
mod overlay_options;
mod overlay_settings_store;
pub mod settings_file;

pub use agent_options::AgentOptions;
pub use agent_status::{AgentStatus, TailState};
pub use game_event_bus::{ChannelClosed, DeliveryMode, GameEventBus, Subscription};
pub use overlay_options::{InspectOptions, OverlayOptions};
pub use overlay_settings_store::OverlaySettingsStore;
