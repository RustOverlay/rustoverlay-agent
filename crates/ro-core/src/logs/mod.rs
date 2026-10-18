//! Raw `output_log.txt` handling: tailing the file and grouping its lines into blocks.

mod log_block;
mod log_block_assembler;
mod log_tailer;

pub use log_block::LogBlock;
pub use log_block_assembler::{LogBlockAssembler, try_parse_prefix};
pub use log_tailer::{DEFAULT_POLL_INTERVAL, LogTailer, read_all_lines};
