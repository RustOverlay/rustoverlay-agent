//! Block parsers. See docs/log-format.md for the formats and their traps.

mod combat_log_deduplicator;
pub mod combat_log_parser;
pub mod player_parser;
pub mod transform_parser;

pub use combat_log_deduplicator::CombatLogDeduplicator;
