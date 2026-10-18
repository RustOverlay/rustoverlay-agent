use chrono::{DateTime, Utc};

/// One timestamped log entry plus every unprefixed line that followed it.
/// See docs/log-format.md for why multi-line output has to be grouped this way.
#[derive(Debug, Clone, PartialEq)]
pub struct LogBlock {
    pub timestamp: DateTime<Utc>,
    pub thread_id: String,
    pub message: String,
    pub body: Vec<String>,
}

impl LogBlock {
    pub fn new(
        timestamp: DateTime<Utc>,
        thread_id: impl Into<String>,
        message: impl Into<String>,
        body: Vec<String>,
    ) -> Self {
        Self { timestamp, thread_id: thread_id.into(), message: message.into(), body }
    }

    pub fn create(timestamp: DateTime<Utc>, thread_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(timestamp, thread_id, message, Vec::new())
    }
}
