//! `StringComparer.OrdinalIgnoreCase`, which the C# lookups were keyed on.

/// Folds a string so equal keys mean equal under ordinal ignore-case: simple per-char upper casing,
/// leaving any char whose upper case is not a single char (such as 'ß') as it is.
pub(crate) fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

pub(crate) fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars().map(fold_char).eq(b.chars().map(fold_char))
}

fn fold_char(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}
