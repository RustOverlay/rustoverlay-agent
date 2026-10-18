//! .NET `TimeSpan`/`DateTimeOffset` semantics over chrono: the `"c"` JSON format settings files
//! already use, and `AddSeconds`' tick-truncating arithmetic.
//!
//! Use as `#[serde(with = "ro_core::timespan")]` on a `TimeDelta` field.

use chrono::{DateTime, Datelike, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serializer, de};

const TICKS_PER_SECOND: i64 = 10_000_000;
const TICKS_PER_MINUTE: i64 = TICKS_PER_SECOND * 60;
const TICKS_PER_HOUR: i64 = TICKS_PER_MINUTE * 60;
const TICKS_PER_DAY: i64 = TICKS_PER_HOUR * 24;

pub fn to_ticks(span: TimeDelta) -> i64 {
    span.num_nanoseconds().map_or_else(
        || span.num_microseconds().unwrap_or(i64::MAX) * 10,
        |ns| ns / 100,
    )
}

pub fn from_ticks(ticks: i64) -> TimeDelta {
    TimeDelta::microseconds(ticks / 10) + TimeDelta::nanoseconds((ticks % 10) * 100)
}

/// `DateTimeOffset.AddSeconds` as of .NET 7: whole seconds and the fraction are converted to
/// ticks separately, each truncated toward zero — not rounded to the millisecond as before.
/// `None` where .NET threw: a non-finite offset, tick overflow, or a result outside years 1–9999.
pub fn add_seconds(at: DateTime<Utc>, seconds: f64) -> Option<DateTime<Utc>> {
    let integral = seconds.trunc();
    // `as i64` saturates, so bound it here or the overflow check below cannot see it.
    if !integral.is_finite() || integral.abs() >= (i64::MAX / TICKS_PER_SECOND) as f64 {
        return None;
    }
    let fractional = seconds - integral;
    let ticks = (integral as i64)
        .checked_mul(TICKS_PER_SECOND)?
        .checked_add((fractional * TICKS_PER_SECOND as f64) as i64)?;

    at.checked_add_signed(from_ticks(ticks)).filter(|t| (1..=9999).contains(&t.year()))
}

/// The constant (`"c"`) format: `[-][d.]hh:mm:ss[.fffffff]`.
pub fn format(span: TimeDelta) -> String {
    let ticks = to_ticks(span);
    let sign = if ticks < 0 { "-" } else { "" };
    let t = ticks.unsigned_abs() as i64;

    let days = t / TICKS_PER_DAY;
    let hours = t % TICKS_PER_DAY / TICKS_PER_HOUR;
    let minutes = t % TICKS_PER_HOUR / TICKS_PER_MINUTE;
    let seconds = t % TICKS_PER_MINUTE / TICKS_PER_SECOND;
    let fraction = t % TICKS_PER_SECOND;

    let mut out = String::from(sign);
    if days > 0 {
        out.push_str(&format!("{days}."));
    }
    out.push_str(&format!("{hours:02}:{minutes:02}:{seconds:02}"));
    if fraction > 0 {
        out.push_str(&format!(".{fraction:07}"));
    }
    out
}

/// `TimeSpan.Parse`'s invariant forms: `d`, `[d.]hh:mm`, `[d.]hh:mm:ss[.f…]`, `d:hh:mm:ss[.f…]`.
pub fn parse(text: &str) -> Option<TimeDelta> {
    let trimmed = text.trim();
    let (negative, body) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };

    let parts: Vec<&str> = body.split(':').collect();
    let (days, hours, minutes, seconds) = match parts.as_slice() {
        [d] => (number(d)?, "0", "0", "0"),
        [h, m] => split_days(h).map(|(d, h)| (d, h, *m, "0"))?,
        [h, m, s] => split_days(h).map(|(d, h)| (d, h, *m, *s))?,
        [d, h, m, s] => (number(d)?, *h, *m, *s),
        _ => return None,
    };

    let hours = number(hours).filter(|h| *h < 24)?;
    let minutes = number(minutes).filter(|m| *m < 60)?;
    let (whole, fraction) = match seconds.split_once('.') {
        Some((whole, fraction)) => (whole, fraction_ticks(fraction)?),
        None => (seconds, 0),
    };
    let seconds = number(whole).filter(|s| *s < 60)?;

    // Hours, minutes and seconds are bounded above, so only the days term can overflow.
    let ticks = days
        .checked_mul(TICKS_PER_DAY)?
        .checked_add(hours * TICKS_PER_HOUR + minutes * TICKS_PER_MINUTE + seconds * TICKS_PER_SECOND + fraction)?;

    Some(from_ticks(if negative { -ticks } else { ticks }))
}

fn split_days(hours: &str) -> Option<(i64, &str)> {
    match hours.split_once('.') {
        Some((days, hours)) => Some((number(days)?, hours)),
        None => Some((0, hours)),
    }
}

fn number(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn fraction_ticks(digits: &str) -> Option<i64> {
    if digits.is_empty() || digits.len() > 7 {
        return None;
    }
    Some(number(digits)? * 10_i64.pow(7 - digits.len() as u32))
}

pub fn serialize<S: Serializer>(span: &TimeDelta, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format(*span))
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<TimeDelta, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse(&text).ok_or_else(|| de::Error::custom(format!("'{text}' is not a TimeSpan")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_constant_specifier() {
        assert_eq!(format(TimeDelta::milliseconds(15)), "00:00:00.0150000");
        assert_eq!(format(TimeDelta::seconds(5)), "00:00:05");
        assert_eq!(format(TimeDelta::hours(25)), "1.01:00:00");
        assert_eq!(format(-TimeDelta::seconds(1)), "-00:00:01");
    }

    #[test]
    fn parses_what_appsettings_and_a_save_write() {
        assert_eq!(parse("00:00:00.150"), Some(TimeDelta::milliseconds(150)));
        assert_eq!(parse("00:00:00.0150000"), Some(TimeDelta::milliseconds(15)));
        assert_eq!(parse("01:00:00"), Some(TimeDelta::hours(1)));
        assert_eq!(parse("00:05"), Some(TimeDelta::minutes(5)));
        assert_eq!(parse("2"), Some(TimeDelta::days(2)));
        assert_eq!(parse("1.02:00:00"), Some(TimeDelta::hours(26)));
        assert_eq!(parse("00:60:00"), None);
        assert_eq!(parse("soon"), None);
    }

    #[test]
    fn add_seconds_truncates_to_ticks() {
        let at = DateTime::parse_from_rfc3339("2026-08-02T22:26:57.337Z").unwrap().to_utc();
        let expected = DateTime::parse_from_rfc3339("2026-08-02T22:21:12.357Z").unwrap().to_utc();

        assert_eq!(add_seconds(at, -344.98), Some(expected));
    }

    #[test]
    fn add_seconds_rejects_what_dotnet_threw_on() {
        let at = DateTime::parse_from_rfc3339("2026-08-02T22:26:57.337Z").unwrap().to_utc();

        assert_eq!(add_seconds(at, 1e20), None);
        assert_eq!(add_seconds(at, -1e20), None);
        assert_eq!(add_seconds(at, f64::INFINITY), None);
        assert_eq!(add_seconds(at, f64::NAN), None);
        assert_eq!(add_seconds(at, 1e12), None);
        assert_eq!(add_seconds(at, 3e11), None);
        assert!(add_seconds(at, 86_400.0).is_some());
    }

    #[test]
    fn a_span_too_large_for_ticks_is_rejected_rather_than_wrapping() {
        assert_eq!(parse("99999999"), None);
        assert_eq!(parse("20000000.00:00:00"), None);
        assert_eq!(parse("-20000000.00:00:00"), None);
        assert_eq!(parse("10675199.02:48:05.4775807"), Some(from_ticks(i64::MAX)));
        assert_eq!(parse("10675199.02:48:05.4775808"), None);
    }
}
