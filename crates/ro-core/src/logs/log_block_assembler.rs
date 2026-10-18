use chrono::{DateTime, NaiveDate, Utc};

use super::LogBlock;

const TIMESTAMP_LENGTH: usize = 24; // 2026-08-02T21:30:17.120Z

/// Turns a stream of raw log lines into [`LogBlock`]s. Feed lines in order; blocks are emitted once
/// their successor arrives, so call [`flush`](Self::flush) when the stream goes idle to release the
/// block in progress.
#[derive(Debug, Default)]
pub struct LogBlockAssembler {
    open: Option<LogBlock>,
}

impl LogBlockAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, line: &str) -> Option<LogBlock> {
        if let Some((timestamp, thread_id, message)) = try_parse_prefix(line) {
            return self.open.replace(LogBlock::create(timestamp, thread_id, message));
        }

        // Unprefixed line: continuation of the block in progress. Unity stack frames land here
        // too; they are left in and filtered by the parsers rather than guessed at here, because
        // command output and stack traces are not reliably distinguishable by shape.
        if let Some(block) = &mut self.open {
            block.body.push(line.to_owned());
        }
        None
    }

    pub fn flush(&mut self) -> Option<LogBlock> {
        self.open.take()
    }
}

/// Splits `ISO8601|threadid|message`. The timestamp must be exactly
/// `yyyy-MM-dd'T'HH:mm:ss.fff'Z'`, as `DateTimeOffset.TryParseExact` demanded.
pub fn try_parse_prefix(line: &str) -> Option<(DateTime<Utc>, &str, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() < TIMESTAMP_LENGTH + 2 || bytes[TIMESTAMP_LENGTH] != b'|' {
        return None;
    }

    let timestamp = parse_timestamp(&bytes[..TIMESTAMP_LENGTH])?;

    let rest = &line[TIMESTAMP_LENGTH + 1..];
    let (thread_id, message) = rest.split_once('|')?;

    Some((timestamp, thread_id, message))
}

fn parse_timestamp(text: &[u8]) -> Option<DateTime<Utc>> {
    let separators = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':'), (19, b'.'), (23, b'Z')];
    if separators.iter().any(|&(at, expected)| text[at] != expected) {
        return None;
    }

    let field = |from: usize, to: usize| -> Option<u32> {
        let digits = &text[from..to];
        if !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(digits.iter().fold(0, |acc, d| acc * 10 + u32::from(d - b'0')))
    };

    let year = field(0, 4).filter(|y| *y >= 1)?;
    let date = NaiveDate::from_ymd_opt(year as i32, field(5, 7)?, field(8, 10)?)?;
    let time = date.and_hms_milli_opt(field(11, 13)?, field(14, 16)?, field(17, 19)?, field(20, 23)?)?;

    Some(time.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_unprefixed_lines_under_the_preceding_entry() {
        let mut assembler = LogBlockAssembler::new();

        assert!(assembler.add("orphan before any prefix").is_none());
        assert!(assembler.add("2026-08-02T21:30:39.194Z|0x6e08|SteamID: 1 (a)").is_none());
        assert!(assembler.add("stack frame").is_none());

        let block = assembler.add("2026-08-02T21:30:40.000Z|0x6e08|next").unwrap();
        assert_eq!(block.message, "SteamID: 1 (a)");
        assert_eq!(block.thread_id, "0x6e08");
        assert_eq!(block.body, ["stack frame"]);

        assert_eq!(assembler.flush().unwrap().message, "next");
        assert!(assembler.flush().is_none());
    }

    #[test]
    fn rejects_malformed_prefixes() {
        assert!(try_parse_prefix("2026-08-02T21:30:39.194Z|0x6e08").is_none());
        assert!(try_parse_prefix("2026-08-02 21:30:39.194Z|0x6e08|m").is_none());
        assert!(try_parse_prefix("2026-13-02T21:30:39.194Z|0x6e08|m").is_none());
        assert!(try_parse_prefix("UnityEngine.Debug:Log(Object)").is_none());
    }
}
