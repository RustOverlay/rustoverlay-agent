#![allow(dead_code)]

use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};

pub fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32) -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_milli_opt(h, mi, s, ms).unwrap().and_utc()
}

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// xUnit's `Assert.Equal(double, double, precision)`, which rounds with `Math.Round`: half to even.
pub fn assert_close(expected: f64, actual: f64, decimals: i32) {
    let scale = 10f64.powi(decimals);
    assert_eq!(
        (expected * scale).round_ties_even(),
        (actual * scale).round_ties_even(),
        "expected {expected}, got {actual}"
    );
}
