//! `double.ToString("0.000")`, so printed scores match the C# report digit for digit.
//!
//! .NET renders a custom format from the value's first 15 significant digits and rounds those half
//! away from zero; Rust's `{:.3}` rounds the exact binary value instead, so 0.0395 (stored as
//! 0.03949999…) prints 0.040 in C# and 0.039 in Rust.

/// The custom format `0.` followed by `decimals` zeros.
pub fn fixed(value: f64, decimals: usize) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "∞" } else { "-∞" }.to_owned();
    }

    let scientific = format!("{:.14e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("LowerExp always has an exponent");
    let exponent: i64 = exponent.parse().expect("LowerExp exponent is an integer");
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();

    // The value is 0.d0d1d2… x 10^(exponent + 1); keep the digits down to the last decimal shown.
    let keep = exponent + 1 + decimals as i64;
    let mut scaled: Vec<u8> = match usize::try_from(keep) {
        Err(_) => Vec::new(),
        Ok(keep) => (0..keep).map(|i| digits.get(i).copied().unwrap_or(0)).collect(),
    };

    let next = usize::try_from(keep).ok().and_then(|k| digits.get(k)).copied().unwrap_or(0);
    if keep >= 0 && next >= 5 {
        round_up(&mut scaled);
    }

    let all: String = scaled.iter().map(|d| char::from(b'0' + d)).collect();
    let mut text = all.trim_start_matches('0').to_owned();
    while text.len() < decimals + 1 {
        text.insert(0, '0');
    }

    let negative = value < 0.0 && text.bytes().any(|b| b != b'0');
    if decimals > 0 {
        text.insert(text.len() - decimals, '.');
    }

    if negative { format!("-{text}") } else { text }
}

fn round_up(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit == 9 {
            *digit = 0;
        } else {
            *digit += 1;
            return;
        }
    }
    digits.insert(0, 1);
}

#[cfg(test)]
mod tests {
    use super::fixed;

    #[test]
    fn rounds_the_fifteen_digit_decimal_half_away_from_zero() {
        assert_eq!(fixed(0.0395, 3), "0.040");
        assert_eq!(fixed(0.039, 3), "0.039");
        assert_eq!(fixed(0.8125, 3), "0.813");
        assert_eq!(fixed(2.675, 2), "2.68");
    }

    #[test]
    fn pads_and_carries() {
        assert_eq!(fixed(0.0, 3), "0.000");
        assert_eq!(fixed(0.9996, 3), "1.000");
        assert_eq!(fixed(99.96, 1), "100.0");
        assert_eq!(fixed(100.0, 1), "100.0");
        assert_eq!(fixed(0.0004, 3), "0.000");
        assert_eq!(fixed(0.0005, 3), "0.001");
        assert_eq!(fixed(1e-9, 2), "0.00");
        assert_eq!(fixed(1234.5, 0), "1235");
    }

    #[test]
    fn keeps_the_sign_only_when_something_shows() {
        assert_eq!(fixed(-0.25, 2), "-0.25");
        assert_eq!(fixed(-0.0001, 2), "0.00");
    }

    #[test]
    fn percentages_render_like_the_report() {
        assert_eq!(fixed(1016.0 * 100.0 / 1104.0, 1), "92.0");
        assert_eq!(fixed(2.0 * 100.0 / 3.0, 1), "66.7");
    }
}
