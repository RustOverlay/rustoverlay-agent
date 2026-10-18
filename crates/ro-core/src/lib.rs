//! Platform-neutral core of the agent: log tailing and parsing, game events, install discovery,
//! key tables, capture bookkeeping and the hosting state the services share. See docs/architecture.md.

pub mod capture;
pub mod config;
pub mod correlation;
pub mod events;
pub mod hosting;
pub mod input;
pub mod install;
pub mod invariant;
pub mod log_pipeline;
pub mod logs;
pub mod parsing;
pub mod timespan;

pub use log_pipeline::LogPipeline;
