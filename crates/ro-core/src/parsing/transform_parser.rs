//! Recognises the numeric triples emitted by `printpos` and `printeyes`.
//!
//! Verified against a live log: the console does not echo the command, only a bare parenthesised
//! `(a, b, c)` on its own line. The bare and labelled forms are still accepted in case a future
//! build decorates the output. Nothing downstream depends on the label — see
//! [`crate::correlation::TransformCorrelator`] for how meaning is assigned.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::{Captures, Regex};

use crate::events::TransformTriple;
use crate::logs::LogBlock;

// Anchored to the whole line on purpose. `find` prints convar defaults inside their help text
// — "...relative to its target; saved across sessions ((0.00, 0.00, 0.00))" — and an
// unanchored match turns those into phantom transforms.
static BRACKETED_TRIPLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:(?<label>[A-Za-z_.]+)\s*[:=]?\s*)?[\(\[]\s*(?<a>-?\d+(?:\.\d+)?)\s*,\s*(?<b>-?\d+(?:\.\d+)?)\s*,\s*(?<c>-?\d+(?:\.\d+)?)\s*[\)\]]\s*$",
    )
    .expect("valid regex")
});

static BARE_TRIPLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:(?<label>[A-Za-z_.]+)\s*[:=]\s*)?(?<a>-?\d+(?:\.\d+)?)\s*,\s*(?<b>-?\d+(?:\.\d+)?)\s*,\s*(?<c>-?\d+(?:\.\d+)?)\s*$",
    )
    .expect("valid regex")
});

pub fn parse(block: &LogBlock) -> Vec<TransformTriple> {
    std::iter::once(&block.message)
        .chain(&block.body)
        .filter_map(|line| try_match(line, block.timestamp))
        .collect()
}

fn try_match(line: &str, timestamp: DateTime<Utc>) -> Option<TransformTriple> {
    let captures = BRACKETED_TRIPLE.captures(line).or_else(|| BARE_TRIPLE.captures(line))?;

    // TODO(port): a non-ASCII digit matched `\d` and threw from double.Parse in .NET; skipped here.
    Some(TransformTriple {
        timestamp,
        a: number(&captures, "a")?,
        b: number(&captures, "b")?,
        c: number(&captures, "c")?,
        label: captures.name("label").map(|m| m.as_str()).filter(|l| !l.is_empty()).map(str::to_owned),
    })
}

fn number(captures: &Captures, group: &str) -> Option<f64> {
    captures.name(group)?.as_str().parse().ok()
}
