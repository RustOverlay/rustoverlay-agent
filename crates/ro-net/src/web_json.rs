//! What `JsonSerializerDefaults.Web` (HttpClient's `ReadFromJsonAsync`) accepted on the way in:
//! property names matched case-insensitively, and numbers allowed to arrive as strings.

use std::fmt::Display;
use std::str::FromStr;

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Parses an API response body. Every wire name is lower snake case, so folding the incoming keys
/// to lowercase is the case-insensitive match.
pub(crate) fn from_slice<T: DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    let mut value: Value = serde_json::from_slice(bytes)?;
    fold_keys(&mut value);
    serde_json::from_value(value)
}

fn fold_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let entries = std::mem::take(map);
            for (key, mut child) in entries {
                fold_keys(&mut child);
                map.insert(key.to_lowercase(), child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(fold_keys),
        _ => {}
    }
}

/// `JsonNumberHandling.AllowReadingFromString`, for a numeric field: `42` or `"42"`.
pub(crate) fn number<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + FromStr,
    T::Err: Display,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient<T> {
        Number(T),
        Text(String),
    }

    match Lenient::<T>::deserialize(deserializer)? {
        Lenient::Number(n) => Ok(n),
        Lenient::Text(text) => text.parse().map_err(D::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Default, PartialEq, Deserialize)]
    #[serde(default)]
    struct Probe {
        #[serde(deserialize_with = "number")]
        steam_id: u64,
        display_name: String,
        nested: Vec<Inner>,
    }

    #[derive(Debug, Default, PartialEq, Deserialize)]
    #[serde(default)]
    struct Inner {
        #[serde(deserialize_with = "number")]
        member_count: i32,
    }

    #[test]
    fn matches_names_case_insensitively_at_every_depth() {
        let probe: Probe =
            from_slice(br#"{"Steam_ID":7,"DISPLAY_NAME":"x","Nested":[{"Member_Count":3}]}"#).unwrap();

        assert_eq!(probe, Probe { steam_id: 7, display_name: "x".into(), nested: vec![Inner { member_count: 3 }] });
    }

    #[test]
    fn reads_numbers_from_strings() {
        let probe: Probe = from_slice(br#"{"steam_id":"76561198000000001"}"#).unwrap();

        assert_eq!(probe.steam_id, 76561198000000001);
    }

    #[test]
    fn rejects_a_string_that_is_not_a_number() {
        assert!(from_slice::<Probe>(br#"{"steam_id":"abc"}"#).is_err());
    }
}
