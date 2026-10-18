use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(150);

/// Tails `output_log.txt`, yielding complete lines as they are written.
///
/// The game holds the file open for writing; Rust's `File::open` on Windows already shares
/// read/write/delete, so it is never locked.
#[derive(Debug)]
pub struct LogTailer {
    path: PathBuf,
    poll: Duration,
    from_start: bool,
    position: u64,
    /// Bytes of a UTF-8 character the last read ended inside of.
    pending: Vec<u8>,
    partial: String,
    started: bool,
    delay_due: bool,
}

/// What one poll read: the bytes from `start` to the end of the file, or nothing if it is missing.
struct Chunk {
    start: u64,
    bytes: Vec<u8>,
}

impl LogTailer {
    /// `from_start` reads the existing file before following. Use for backfill and tests; live
    /// agents start at the end so a launch does not replay hours of history.
    pub fn new(path: impl Into<PathBuf>, poll: Option<Duration>, from_start: bool) -> Self {
        Self {
            path: path.into(),
            poll: poll.unwrap_or(DEFAULT_POLL_INTERVAL),
            from_start,
            position: 0,
            pending: Vec::new(),
            partial: String::new(),
            started: false,
            delay_due: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Waits until at least one complete line is available and returns every one read. Each poll
    /// is followed by the poll interval, as in the C# loop; drop the future to cancel.
    pub async fn next_lines(&mut self) -> io::Result<Vec<String>> {
        loop {
            if self.delay_due {
                tokio::time::sleep(self.poll).await;
            }
            self.delay_due = true;

            let (path, position, skip_to_end) = (self.path.clone(), self.position, self.skips_to_end());
            // Off the async worker: a replay reads the whole log. State advances only once it lands.
            let chunk = tokio::task::spawn_blocking(move || read_chunk(&path, position, skip_to_end))
                .await
                .map_err(io::Error::other)??;

            let lines = self.accept(chunk);
            if !lines.is_empty() {
                return Ok(lines);
            }
        }
    }

    /// One poll: reads whatever has been appended since the last call. A missing file yields
    /// nothing rather than an error, so the tail waits for the game to create it.
    pub fn read_available(&mut self) -> io::Result<Vec<String>> {
        let chunk = read_chunk(&self.path, self.position, self.skips_to_end())?;
        Ok(self.accept(chunk))
    }

    fn skips_to_end(&self) -> bool {
        !self.started && !self.from_start
    }

    fn accept(&mut self, chunk: Option<Chunk>) -> Vec<String> {
        let Some(Chunk { start, bytes }) = chunk else { return Vec::new() };

        if start < self.position {
            self.pending.clear();
            self.partial.clear();
        }
        self.started = true;
        self.position = start + bytes.len() as u64;

        // StreamReader skips the encoding preamble at the head of whatever it reads.
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        self.pending.extend_from_slice(bytes);
        let complete = self.pending.len() - incomplete_tail(&self.pending);
        let chunk = String::from_utf8_lossy(&self.pending[..complete]).into_owned();
        self.pending.drain(..complete);

        split_complete_lines(&mut self.partial, &chunk)
    }
}

fn read_chunk(path: &Path, position: u64, skip_to_end: bool) -> io::Result<Option<Chunk>> {
    if !path.is_file() {
        return Ok(None);
    }

    let mut file = File::open(path)?;
    let length = file.metadata()?.len();

    // Rust truncates and rewrites the log on launch. A shorter file is a new session, so
    // rewind rather than waiting forever for the old offset to be reached again.
    let start = if length < position {
        0
    } else if skip_to_end {
        length
    } else {
        position
    };

    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(Some(Chunk { start, bytes }))
}

/// How many trailing bytes begin a UTF-8 character the read ended inside of, to decode next poll.
fn incomplete_tail(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let byte = bytes[bytes.len() - back];
        if byte & 0xC0 != 0x80 {
            let width = match byte {
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF7 => 4,
                _ => 1,
            };
            return if width > back { back } else { 0 };
        }
    }
    0
}

/// A read can land mid-line; the remainder is carried into the next poll so a line is never
/// emitted twice or split in half.
fn split_complete_lines(partial: &mut String, chunk: &str) -> Vec<String> {
    if chunk.is_empty() {
        return Vec::new();
    }

    partial.push_str(chunk);

    let mut lines = Vec::new();
    let mut start = 0;
    while let Some(offset) = partial[start..].find('\n') {
        let newline = start + offset;
        lines.push(partial[start..newline].trim_end_matches('\r').to_owned());
        start = newline + 1;
    }

    partial.drain(..start);
    lines
}

/// Reads a whole log in one pass. Shares the file the same way the tail does, because the game
/// holds it open for writing.
pub fn read_all_lines(path: &Path) -> io::Result<Vec<String>> {
    crate::invariant::read_all_lines(path)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn append(path: &Path, bytes: &[u8]) {
        std::fs::OpenOptions::new().append(true).create(true).open(path).unwrap().write_all(bytes).unwrap();
    }

    #[test]
    fn carries_a_partial_line_into_the_next_read() {
        let mut partial = String::new();

        assert_eq!(split_complete_lines(&mut partial, "one\r\ntw"), ["one"]);
        assert_eq!(split_complete_lines(&mut partial, "o\nthree"), ["two"]);
        assert_eq!(partial, "three");
    }

    #[test]
    fn a_truncated_file_is_reread_from_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output_log.txt");
        std::fs::write(&path, "old session line\n").unwrap();

        let mut tailer = LogTailer::new(&path, None, false);
        assert!(tailer.read_available().unwrap().is_empty());

        std::fs::write(&path, "new\n").unwrap();
        assert_eq!(tailer.read_available().unwrap(), ["new"]);
    }

    #[test]
    fn a_character_split_across_two_reads_is_decoded_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output_log.txt");
        let line = "Жора 你好 😀\n".as_bytes();

        for split in 1..line.len() {
            std::fs::write(&path, b"").unwrap();
            let mut tailer = LogTailer::new(&path, None, true);
            append(&path, &line[..split]);
            let first = tailer.read_available().unwrap();
            append(&path, &line[split..]);
            let second = tailer.read_available().unwrap();

            assert!(first.is_empty(), "split at {split}");
            assert_eq!(second, ["Жора 你好 😀"], "split at {split}");
            assert!(tailer.pending.is_empty());
        }
    }

    #[test]
    fn incomplete_tail_finds_only_an_unfinished_character() {
        assert_eq!(incomplete_tail(b"abc"), 0);
        assert_eq!(incomplete_tail("é".as_bytes()), 0);
        assert_eq!(incomplete_tail(&"é".as_bytes()[..1]), 1);
        assert_eq!(incomplete_tail(&"😀".as_bytes()[..3]), 3);
        assert_eq!(incomplete_tail(b"\x80\x80\x80"), 0);
    }

    #[tokio::test]
    async fn next_lines_reads_through_the_blocking_pool() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output_log.txt");
        std::fs::write(&path, "one\ntwo\n").unwrap();

        let mut tailer = LogTailer::new(&path, Some(Duration::from_millis(1)), true);
        assert_eq!(tailer.next_lines().await.unwrap(), ["one", "two"]);

        append(&path, b"three\n");
        assert_eq!(tailer.next_lines().await.unwrap(), ["three"]);
    }
}
