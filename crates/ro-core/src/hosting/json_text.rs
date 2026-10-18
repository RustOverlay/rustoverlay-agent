//! The JSON dialect `AddJsonFile` read — a UTF-8 BOM, `//` and `/* */` comments and trailing commas
//! all accepted — reduced to what `serde_json` takes. Syntax is blanked rather than removed so a
//! parse error still points at the right line.

pub fn relax(bytes: &[u8]) -> Vec<u8> {
    let mut text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes).to_vec();
    blank_comments(&mut text);
    blank_trailing_commas(&mut text);
    text
}

fn blank_comments(text: &mut [u8]) {
    let mut i = 0;
    let mut in_string = false;

    while i < text.len() {
        if in_string {
            match text[i] {
                b'\\' => i += 1,
                b'"' => in_string = false,
                _ => {}
            }
            i += 1;
            continue;
        }

        let end = match (text[i], text.get(i + 1)) {
            (b'/', Some(b'/')) => text[i..].iter().position(|&c| c == b'\n' || c == b'\r').map_or(text.len(), |p| i + p),
            (b'/', Some(b'*')) => match text[i + 2..].windows(2).position(|w| w == b"*/") {
                Some(p) => i + 2 + p + 2,
                // Unterminated: left in place so the parse fails, as it did in .NET.
                None => return,
            },
            (b'"', _) => {
                in_string = true;
                i += 1;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        };

        text[i..end].iter_mut().filter(|c| !matches!(c, b'\n' | b'\r')).for_each(|c| *c = b' ');
        i = end;
    }
}

/// Only a comma that follows a value counts: `[,]` and `[1,,]` stay invalid.
fn blank_trailing_commas(text: &mut [u8]) {
    let mut in_string = false;
    let mut escaped = false;
    let mut last = 0u8;
    let mut pending = None;

    for i in 0..text.len() {
        let c = text[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
                last = c;
            }
            continue;
        }

        match c {
            b' ' | b'\t' | b'\n' | b'\r' => continue,
            b',' => pending = (!matches!(last, b'[' | b'{' | b',')).then_some(i),
            b'}' | b']' => {
                if let Some(comma) = pending.take() {
                    text[comma] = b' ';
                }
            }
            b'"' => {
                in_string = true;
                pending = None;
            }
            _ => pending = None,
        }
        last = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relaxed(text: &str) -> serde_json::Result<serde_json::Value> {
        serde_json::from_slice(&relax(text.as_bytes()))
    }

    #[test]
    fn comments_trailing_commas_and_a_bom_are_accepted() {
        let value = relaxed(
            "\u{FEFF}{\n  // why\n  \"Url\": \"https://a//b\", /* inline */\n  \"Keys\": [\"1\", \"2\",],\n}\n",
        )
        .unwrap();

        assert_eq!(value, serde_json::json!({"Url": "https://a//b", "Keys": ["1", "2"]}));
    }

    #[test]
    fn what_net_rejected_stays_rejected() {
        assert!(relaxed("[,]").is_err());
        assert!(relaxed("[1,,]").is_err());
        assert!(relaxed("{\"a\": 1 /* open").is_err());
        assert_eq!(relaxed(r#"{"a": "x\",}"}"#).unwrap()["a"], r#"x",}"#);
    }
}
