//! The C# recorder's `ConsoleLogger`: warnings and errors from the capture backends, on their own
//! line so they do not vanish under the `\r` progress counter.

use std::fmt::{self, Write as _};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

pub struct ConsoleLog;

impl ConsoleLog {
    pub fn install() {
        // Only fails when a subscriber is already set, which leaves logging working anyway.
        let _ = tracing::subscriber::set_global_default(ConsoleLog);
    }
}

/// `LogLevel` as .NET prints it.
fn level_name(level: Level) -> &'static str {
    match level {
        Level::ERROR => "Error",
        Level::WARN => "Warning",
        Level::INFO => "Information",
        Level::DEBUG => "Debug",
        Level::TRACE => "Trace",
    }
}

#[derive(Default)]
struct Message(String);

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.0, "{value:?}");
        } else {
            let _ = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

pub fn format_line(level: Level, message: &str) -> String {
    format!("\n  [{}] {message}", level_name(level))
}

impl Subscriber for ConsoleLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        *metadata.level() <= Level::WARN
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut message = Message::default();
        event.record(&mut message);
        println!("{}", format_line(*event.metadata().level(), &message.0));
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_read_like_the_csharp_logger() {
        assert_eq!(format_line(Level::WARN, "Duplication lost"), "\n  [Warning] Duplication lost");
        assert_eq!(format_line(Level::ERROR, "x"), "\n  [Error] x");
    }
}
