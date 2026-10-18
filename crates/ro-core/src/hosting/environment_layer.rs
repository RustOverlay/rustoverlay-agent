//! `AddEnvironmentVariables("RUSTOVERLAY_")`: variables carrying the prefix (matched
//! case-insensitively) become a configuration layer, with `__` separating sections as .NET's `:`.

use serde_json::{Map, Value};

pub const ENV_PREFIX: &str = "RUSTOVERLAY_";

/// The layer built from the process environment.
pub fn from_process() -> Value {
    environment_layer(std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))))
}

/// Every value is a string, as it was to `IConfiguration`; `section` converts it to the field's type.
pub fn environment_layer(vars: impl IntoIterator<Item = (String, String)>) -> Value {
    let mut root = Value::Object(Map::new());

    for (name, value) in vars {
        let Some(key) = strip_prefix_ignore_case(&name, ENV_PREFIX) else { continue };
        let path: Vec<&str> = key.split("__").filter(|p| !p.is_empty()).collect();
        let Some((last, parents)) = path.split_last() else { continue };

        let mut node = &mut root;
        for parent in parents {
            node = child(node, parent);
        }
        if let Value::Object(map) = node {
            let existing = map.keys().find(|k| k.eq_ignore_ascii_case(last)).cloned();
            map.insert(existing.unwrap_or_else(|| (*last).to_owned()), Value::String(value));
        }
    }

    root
}

fn strip_prefix_ignore_case<'a>(name: &'a str, prefix: &str) -> Option<&'a str> {
    let head = name.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &name[prefix.len()..])
}

// A variable naming a section outright (`RUSTOVERLAY_Overlay=x`) is dropped once a child needs it.
fn child<'a>(node: &'a mut Value, key: &str) -> &'a mut Value {
    if !node.is_object() {
        *node = Value::Object(Map::new());
    }
    let Value::Object(map) = node else { unreachable!("made an object above") };
    let existing = map.keys().find(|k| k.eq_ignore_ascii_case(key)).cloned().unwrap_or_else(|| key.to_owned());
    let slot = map.entry(existing).or_insert_with(|| Value::Object(Map::new()));
    if !slot.is_object() {
        *slot = Value::Object(Map::new());
    }
    slot
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    #[test]
    fn the_prefix_is_dropped_and_double_underscores_nest() {
        let layer = environment_layer(vars(&[
            ("RUSTOVERLAY_Agent__RustPath", r"D:\Games\Rust"),
            ("rustoverlay_overlay__inspect__holdkey", "leftalt"),
            ("RUSTOVERLAY_Overlay__ToggleKey", "f4"),
            ("UNRELATED", "x"),
            ("RUSTOVERLAY_", "ignored"),
        ]));

        assert_eq!(
            layer,
            serde_json::json!({
                "Agent": {"RustPath": r"D:\Games\Rust"},
                "overlay": {"inspect": {"holdkey": "leftalt"}, "ToggleKey": "f4"},
            })
        );
    }
}
