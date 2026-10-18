//! The agent's log file, as Serilog wrote it: `logs/agent-yyyyMMdd.log` under the data directory,
//! a new file each local day, the newest seven kept.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

const PREFIX: &str = "agent-";
const SUFFIX: &str = ".log";
const RETAINED_FILES: usize = 7;

/// Installs the global subscriber. Keep the guard alive for the life of the process, or buffered
/// lines are lost at exit.
pub fn init(data_directory: &Path, filter: &str) -> WorkerGuard {
    let (writer, guard) = tracing_appender::non_blocking(DailyFile::new(data_directory.join("logs")));
    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));

    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_timer(LocalTime).with_ansi(false);
    // Stands in for Serilog's Debug sink.
    if cfg!(debug_assertions) {
        use tracing_subscriber::fmt::writer::MakeWriterExt;
        builder.with_writer(writer.and(io::stderr)).init();
    } else {
        builder.with_writer(writer).init();
    }

    guard
}

struct LocalTime;

impl FormatTime for LocalTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", Local::now().format("%Y-%m-%d %H:%M:%S%.3f %:z"))
    }
}

/// Rolls at local midnight, like Serilog's `RollingInterval.Day`.
struct DailyFile {
    directory: PathBuf,
    open: Option<(NaiveDate, File)>,
}

impl DailyFile {
    fn new(directory: PathBuf) -> Self {
        Self { directory, open: None }
    }

    fn file_for(&mut self, today: NaiveDate) -> io::Result<&mut File> {
        if self.open.as_ref().is_none_or(|(date, _)| *date != today) {
            fs::create_dir_all(&self.directory)?;
            let path = self.directory.join(file_name(today));
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            self.open = Some((today, file));
            prune(&self.directory);
        }
        Ok(&mut self.open.as_mut().expect("opened above").1)
    }
}

impl Write for DailyFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file_for(Local::now().date_naive())?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.open {
            Some((_, file)) => file.flush(),
            None => Ok(()),
        }
    }
}

fn file_name(date: NaiveDate) -> String {
    format!("{PREFIX}{}{SUFFIX}", date.format("%Y%m%d"))
}

/// Deletes all but the newest [`RETAINED_FILES`] logs. The names sort by date, so newest is last.
fn prune(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else { return };
    let mut logs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PREFIX) && n.ends_with(SUFFIX)))
        .collect();
    logs.sort();

    let excess = logs.len().saturating_sub(RETAINED_FILES);
    for old in &logs[..excess] {
        // A log another process still holds open is retried at the next roll.
        let _ = fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_serilogs_day_format() {
        assert_eq!(file_name(NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()), "agent-20260809.log");
    }

    #[test]
    fn rolling_keeps_the_newest_seven() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=9 {
            fs::write(dir.path().join(file_name(NaiveDate::from_ymd_opt(2026, 8, day).unwrap())), "x").unwrap();
        }
        fs::write(dir.path().join("other.txt"), "x").unwrap();

        prune(dir.path());

        let mut left: Vec<String> =
            fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left.len(), 8);
        assert_eq!(left[0], "agent-20260803.log");
        assert!(left.contains(&"other.txt".to_owned()));
    }

    #[test]
    fn writes_append_to_todays_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = DailyFile::new(dir.path().join("logs"));

        file.write_all(b"one\n").unwrap();
        file.write_all(b"two\n").unwrap();
        file.flush().unwrap();

        let today = dir.path().join("logs").join(file_name(Local::now().date_naive()));
        assert_eq!(fs::read_to_string(today).unwrap(), "one\ntwo\n");
    }
}
