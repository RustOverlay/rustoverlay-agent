//! .NET invariant-culture parsing and text-reading semantics, so the ported parsers accept exactly
//! what `double.TryParse`/`int.TryParse`/`bool.TryParse` and `StreamReader.ReadLine` did.

use std::io;
use std::path::Path;

/// `double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture)`.
pub fn parse_double(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);

    // Rust accepts "inf"; .NET only spells it "Infinity".
    if unsigned.eq_ignore_ascii_case("inf") {
        return None;
    }

    trimmed.parse().ok()
}

/// `int.TryParse(s, NumberStyles.Integer, CultureInfo.InvariantCulture)`.
pub fn parse_i32(text: &str) -> Option<i32> {
    text.trim().parse().ok()
}

/// `ulong.TryParse(s)` — which, unlike Rust, accepts "-0".
pub fn parse_u64(text: &str) -> Option<u64> {
    let trimmed = text.trim();

    match trimmed.strip_prefix('-') {
        Some(rest) => rest.parse::<u64>().ok().filter(|v| *v == 0),
        None => trimmed.parse().ok(),
    }
}

/// `bool.TryParse`: "true"/"false" in any case, surrounding whitespace allowed.
pub fn parse_bool(text: &str) -> Option<bool> {
    let trimmed = text.trim().trim_end_matches('\0');

    if trimmed.eq_ignore_ascii_case("true") {
        Some(true)
    } else if trimmed.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// `double.Equals`, under which NaN equals NaN — unlike `==`.
pub fn double_equals(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// `StreamReader.ReadLine` over a whole buffer: splits on `\r\n`, `\n` or `\r`, drops a leading
/// UTF-8 BOM, and yields no trailing empty line for a final terminator.
pub fn split_lines(bytes: &[u8]) -> Vec<String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = String::from_utf8_lossy(bytes);

    let mut lines = Vec::new();
    let mut rest = text.as_ref();

    while !rest.is_empty() {
        match rest.find(['\r', '\n']) {
            Some(end) => {
                lines.push(rest[..end].to_owned());
                let skip = if rest[end..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[end + skip..];
            }
            None => {
                lines.push(rest.to_owned());
                break;
            }
        }
    }

    lines
}

/// `File.ReadAllLines`, with the same line splitting as [`split_lines`].
pub fn read_all_lines(path: &Path) -> io::Result<Vec<String>> {
    Ok(split_lines(&std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_follow_invariant_culture() {
        assert_eq!(parse_double(" 3.6 "), Some(3.6));
        assert_eq!(parse_double("-.5"), Some(-0.5));
        assert_eq!(parse_double("1e3"), Some(1000.0));
        assert!(parse_double("NaN").unwrap().is_nan());
        assert_eq!(parse_double("Infinity"), Some(f64::INFINITY));
        assert_eq!(parse_double("inf"), None);
        assert_eq!(parse_double("1,5"), None);
        assert_eq!(parse_double("3.6m"), None);
    }

    #[test]
    fn unsigned_accepts_negative_zero_only() {
        assert_eq!(parse_u64("-0"), Some(0));
        assert_eq!(parse_u64("-1"), None);
        assert_eq!(parse_u64("+7"), Some(7));
    }

    #[test]
    fn lines_split_like_read_line() {
        assert_eq!(split_lines(b"\xEF\xBB\xBFa\r\nb\rc\n\nd\n"), ["a", "b", "c", "", "d"]);
        assert!(split_lines(b"").is_empty());
    }
}
