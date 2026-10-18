//! .NET custom numeric format strings the C# views used ("0", "0.0", "0.#", "0.##", "n0").
//!
//! .NET formats a double through its 15-significant-digit decimal form and then rounds half away
//! from zero, so 2.25 is "2.3" where Rust's `{:.1}` gives "2.2". Matching that keeps every label
//! identical to the C# agent's.

/// `value.ToString("0.00")`-style: exactly `decimals` places.
pub fn fixed(value: f64, decimals: usize) -> String {
    render(value, decimals, false)
}

/// `value.ToString("0.##")`-style: up to `decimals` places, trailing zeros dropped.
pub fn trimmed(value: f64, decimals: usize) -> String {
    render(value, decimals, true)
}

/// `"+0.#;-0.#;0"`: signed, and a value that rounds to zero takes the third section.
pub fn signed_trimmed(value: f64, decimals: usize) -> String {
    let magnitude = trimmed(value.abs(), decimals);
    if magnitude == "0" {
        "0".into()
    } else if value > 0.0 {
        format!("+{magnitude}")
    } else {
        format!("-{magnitude}")
    }
}

/// `value.ToString("n0")` under en-US: comma-grouped thousands.
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn render(value: f64, decimals: usize, trim: bool) -> String {
    if !value.is_finite() {
        return value.to_string();
    }

    let (mut digits, mut point) = significant_digits(value.abs());
    let keep = point + decimals as i32;

    if keep < 0 {
        digits.clear();
    } else if (keep as usize) < digits.len() {
        let round_up = digits[keep as usize] >= 5;
        digits.truncate(keep as usize);
        if round_up && carry(&mut digits) {
            point += 1;
        }
    }

    let (integer, mut fraction) = split(&digits, point, decimals);
    if trim {
        while fraction.ends_with('0') {
            fraction.pop();
        }
    }

    let sign = if value < 0.0 && !(integer == "0" && fraction.chars().all(|c| c == '0')) { "-" } else { "" };
    if fraction.is_empty() { format!("{sign}{integer}") } else { format!("{sign}{integer}.{fraction}") }
}

/// The 15 significant digits .NET formats from, and where the decimal point falls among them.
fn significant_digits(value: f64) -> (Vec<u8>, i32) {
    if value == 0.0 {
        return (Vec::new(), 1);
    }
    let sci = format!("{value:.14e}");
    let (mantissa, exponent) = sci.split_once('e').expect("scientific notation has an exponent");
    let digits = mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    (digits, exponent.parse::<i32>().expect("exponent is an integer") + 1)
}

/// Adds one at the last digit. True when that grew the number by a digit (9.96 -> 10.0).
fn carry(digits: &mut Vec<u8>) -> bool {
    for d in digits.iter_mut().rev() {
        if *d == 9 {
            *d = 0;
        } else {
            *d += 1;
            return false;
        }
    }
    digits.insert(0, 1);
    true
}

fn split(digits: &[u8], point: i32, decimals: usize) -> (String, String) {
    let digit = |i: i32| if i >= 0 && (i as usize) < digits.len() { (b'0' + digits[i as usize]) as char } else { '0' };

    let integer: String = if point <= 0 { "0".into() } else { (0..point).map(digit).collect() };
    let fraction: String = (point..point + decimals as i32).map(digit).collect();
    (integer, fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_half_away_from_zero_like_dotnet() {
        assert_eq!(trimmed(2.25, 1), "2.3");
        assert_eq!(trimmed(2.35, 1), "2.4");
        assert_eq!(fixed(0.05, 1), "0.1");
        assert_eq!(fixed(99.5, 0), "100");
        assert_eq!(fixed(-1.5, 0), "-2");
    }

    #[test]
    fn trims_trailing_zeros_only_when_asked() {
        assert_eq!(trimmed(12.0, 2), "12");
        assert_eq!(trimmed(12.10, 2), "12.1");
        assert_eq!(fixed(3.0, 1), "3.0");
        assert_eq!(trimmed(0.004, 2), "0");
        assert_eq!(trimmed(9.996, 2), "10");
    }

    #[test]
    fn signed_format_uses_the_zero_section_for_values_that_round_away() {
        assert_eq!(signed_trimmed(8.02, 1), "+8");
        assert_eq!(signed_trimmed(-12.36, 1), "-12.4");
        assert_eq!(signed_trimmed(0.04, 1), "0");
        assert_eq!(signed_trimmed(-0.04, 1), "0");
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1,000");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
