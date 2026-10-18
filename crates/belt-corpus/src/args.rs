//! The command line, parsed with the C# tool's exact quirks: a flag's value is whatever follows its
//! first occurrence, even another flag.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::PathBuf;

use ro_core::invariant;

pub const USAGE: &str = "usage: BeltCorpus <corpus-dir> [truth,csv] [--from HHmmss] [--to HHmmss] [--exclude HHmmss-fff,...] [--departure <n>] [--confirm] [--skins] [--game-skins item,item] [--skin-categories a,b] [--all-skins] [--dump] [--montage <png>]";

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub directory: PathBuf,
    /// The six belt slots in order; `-` (or `?`) is a slot known to be empty.
    pub truth: Vec<String>,
    pub from: String,
    pub to: String,
    /// Strips whose name contains any of these, e.g. `105609-130` for a frame that is not the game.
    pub exclude: Vec<String>,
    pub montage: Option<PathBuf>,
    pub departure: Option<f64>,
    /// Matched case-insensitively, as the C# `OrdinalIgnoreCase` set was.
    pub categories: Option<Vec<String>>,
    pub skins: bool,
    /// Items whose approved skins are read from the game's bundles, as the agent's skin hunt loads them.
    pub game_skins: Option<Vec<String>>,
    pub skin_categories: Option<Vec<String>>,
    pub all_skins: bool,
    pub flat: bool,
    pub coarse: Option<usize>,
    /// `Some(confirm_after)` when `--confirm` replays through `SlotHistory`.
    pub confirm: Option<u32>,
    pub dump: bool,
}

#[derive(Debug, PartialEq)]
pub enum ArgsError {
    Usage,
    /// C# let `double.Parse`/`int.Parse` throw here; also a value that is not valid Unicode.
    Invalid { flag: &'static str, value: String },
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => f.write_str(USAGE),
            Self::Invalid { flag, value } => write!(f, "{flag}: cannot parse '{value}'"),
        }
    }
}

/// Paths stay `OsString`, so any path Windows accepts works; text values must be Unicode.
pub fn parse(args: &[OsString]) -> Result<Options, ArgsError> {
    let Some(directory) = args.first() else { return Err(ArgsError::Usage) };
    let raw = |name: &str| args.iter().position(|a| a == name).and_then(|at| args.get(at + 1)).map(OsString::as_os_str);
    let flag = |name: &'static str| raw(name).map(|value| unicode(name, value)).transpose();
    let has = |name: &str| args.iter().any(|a| a == name);

    let truth_arg = match args.get(1).filter(|a| !a.as_encoded_bytes().starts_with(b"--")) {
        None => "",
        Some(text) => unicode("truth", text)?,
    };
    let mut truth = split_list(truth_arg);
    if truth.is_empty() {
        truth = vec!["-".to_owned(); 6];
    }

    // TODO(port): C# used current-culture double.Parse (thousands separators, comma decimals); invariant is deliberate.
    let departure = match flag("--departure")? {
        None => None,
        Some(text) => Some(invariant::parse_double(text).ok_or_else(|| invalid("--departure", text))?),
    };
    // A negative shortlist disables the coarse pass exactly as 0 does: C# only tests `> 0`.
    let coarse = match flag("--coarse")? {
        None => None,
        Some(text) => Some(invariant::parse_i32(text).ok_or_else(|| invalid("--coarse", text))?.max(0) as usize),
    };
    // TryParse, falling back to the shipped 2; below 0 behaves as 0, since agreements never are.
    let confirm = has("--confirm").then(|| {
        raw("--confirm").and_then(OsStr::to_str).and_then(invariant::parse_i32).map_or(2, |n| n.max(0) as u32)
    });

    Ok(Options {
        directory: PathBuf::from(directory),
        truth,
        from: flag("--from")?.unwrap_or("000000").to_owned(),
        to: flag("--to")?.unwrap_or("999999").to_owned(),
        exclude: flag("--exclude")?.map(split_list).unwrap_or_default(),
        montage: raw("--montage").map(PathBuf::from),
        departure,
        categories: flag("--categories")?.map(split_list),
        skins: has("--skins"),
        game_skins: flag("--game-skins")?.map(split_list),
        skin_categories: flag("--skin-categories")?.map(split_list),
        all_skins: has("--all-skins"),
        flat: has("--flat"),
        coarse,
        confirm,
        dump: has("--dump"),
    })
}

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`, by simple per-character folding.
pub fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars().map(fold).eq(b.chars().map(fold))
}

fn fold(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

fn split_list(text: &str) -> Vec<String> {
    text.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

fn unicode<'a>(flag: &'static str, value: &'a OsStr) -> Result<&'a str, ArgsError> {
    value.to_str().ok_or_else(|| invalid(flag, &value.to_string_lossy()))
}

fn invalid(flag: &'static str, value: &str) -> ArgsError {
    ArgsError::Invalid { flag, value: value.to_owned() }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
