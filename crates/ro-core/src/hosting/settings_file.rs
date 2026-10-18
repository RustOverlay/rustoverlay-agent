//! The layered configuration the C# host built with `IConfiguration`: appsettings.json, then the
//! per-user settings.json, then `RUSTOVERLAY_` environment variables, merged by key,
//! case-insensitively.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

pub use super::environment_layer::{ENV_PREFIX, environment_layer};
use crate::invariant::{parse_bool, parse_double};

/// `%LOCALAPPDATA%/RustOverlay` — per-user, because the install directory is not writable for a
/// standard user.
pub fn data_directory() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))
        .unwrap_or_default();

    base.join("RustOverlay")
}

#[derive(Debug)]
pub enum SettingsError {
    Io(PathBuf, std::io::Error),
    Json(PathBuf, serde_json::Error),
    Section(&'static str, serde_json::Error),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, e) => write!(f, "could not read {}: {e}", path.display()),
            Self::Json(path, e) => write!(f, "{} is not valid JSON: {e}", path.display()),
            Self::Section(section, e) => write!(f, "configuration section '{section}' is invalid: {e}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// Merges the files in order, later ones winning per key. A missing file is skipped, as
/// `AddJsonFile(optional: true)` did; an unreadable or invalid one is an error, as it was at
/// startup in C#.
pub fn load_layered(paths: &[&Path]) -> Result<Value, SettingsError> {
    let mut merged = Value::Object(Map::new());

    for path in paths {
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(path).map_err(|e| SettingsError::Io(path.to_path_buf(), e))?;
        let json_error = |e| SettingsError::Json(path.to_path_buf(), e);
        let layer: Value = serde_json::from_slice(&super::json_text::relax(&bytes)).map_err(json_error)?;
        if !layer.is_object() {
            return Err(json_error(serde::de::Error::custom("the top-level JSON element must be an object")));
        }
        merge(&mut merged, layer);
    }

    Ok(merged)
}

/// [`load_layered`], then the process's `RUSTOVERLAY_` environment variables on top.
pub fn load_with_environment(paths: &[&Path]) -> Result<Value, SettingsError> {
    let mut merged = load_layered(paths)?;
    merge(&mut merged, super::environment_layer::from_process());
    Ok(merged)
}

/// Binds one section, field names matched case-insensitively and absent fields left at default.
/// Strings are converted to the bool or number the field expects, as the configuration binder did.
pub fn section<T: DeserializeOwned + Serialize + Default>(
    merged: &Value,
    name: &'static str,
) -> Result<T, SettingsError> {
    let Some(found) = find_key(merged, name) else { return Ok(T::default()) };

    let template = serde_json::to_value(T::default()).map_err(|e| SettingsError::Section(name, e))?;
    let canonical = canonicalize(found.clone(), &template);

    serde_json::from_value(canonical).map_err(|e| SettingsError::Section(name, e))
}

fn find_key<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    value.as_object()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
}

/// Lays `layer` over `target`, later winning per key; nulls never overwrite. Arrays merge index by
/// index, as `IConfiguration` flattened them to `Key:0`, `Key:1`: a shorter later array leaves the
/// earlier tail in place, and an index-keyed object (`SelectKeys__0` from the environment)
/// overlays single entries.
pub fn merge(target: &mut Value, layer: Value) {
    if layer.is_null() {
        return;
    }
    if let (Value::Array(items), Value::Object(_)) = (&*target, &layer) {
        *target = Value::Object(items.iter().enumerate().map(|(i, v)| (i.to_string(), v.clone())).collect());
    }

    match (target, layer) {
        (Value::Array(target), Value::Array(layer)) => {
            for (i, value) in layer.into_iter().enumerate() {
                match target.get_mut(i) {
                    Some(slot) => merge(slot, value),
                    None => target.push(value),
                }
            }
        }
        (Value::Object(target), Value::Object(layer)) => {
            for (key, value) in layer.into_iter().filter(|(_, v)| !v.is_null()) {
                let existing = target.keys().find(|k| k.eq_ignore_ascii_case(&key)).cloned();
                match existing.and_then(|k| target.get_mut(&k)) {
                    Some(slot) => merge(slot, value),
                    None => {
                        target.insert(key, value);
                    }
                }
            }
        }
        (target, layer) => *target = layer,
    }
}

/// Reshapes `value` to the template (the type's own serialisation): keys renamed to its casing,
/// nulls dropped so the field keeps its default, and scalars converted to its types the way the
/// configuration binder did — every value was a string to it, JSON `true` was "True".
fn canonicalize(value: Value, template: &Value) -> Value {
    // An empty default list carries no element type; every list option is a list of strings.
    let element_of = |shape: &[Value]| shape.first().cloned().unwrap_or_else(|| Value::String(String::new()));

    match (value, template) {
        (Value::Object(map), Value::Object(shape)) => {
            let mut out = Map::new();
            for (key, child) in map {
                if child.is_null() {
                    continue;
                }
                match shape.iter().find(|(k, _)| k.eq_ignore_ascii_case(&key)) {
                    Some((canonical, child_shape)) => {
                        out.insert(canonical.clone(), canonicalize(child, child_shape));
                    }
                    None => {
                        out.insert(key, child);
                    }
                }
            }
            Value::Object(out)
        }
        (Value::Object(map), Value::Array(shape)) => {
            let element = element_of(shape);
            let mut indexed: Vec<(usize, Value)> =
                map.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect();
            indexed.sort_by_key(|(i, _)| *i);
            Value::Array(indexed.into_iter().map(|(_, v)| canonicalize(v, &element)).collect())
        }
        (Value::Array(items), Value::Array(shape)) => {
            let element = element_of(shape);
            Value::Array(items.into_iter().map(|v| canonicalize(v, &element)).collect())
        }
        (Value::String(text), Value::Bool(_)) => parse_bool(&text).map_or(Value::String(text), Value::Bool),
        (Value::String(text), Value::Number(n)) => {
            let parsed = if n.is_f64() {
                parse_double(&text).and_then(serde_json::Number::from_f64)
            } else {
                text.trim().parse::<i64>().ok().map(serde_json::Number::from)
            };
            parsed.map_or(Value::String(text), Value::Number)
        }
        (Value::Number(n), Value::String(_)) => Value::String(n.to_string()),
        (Value::Bool(b), Value::String(_)) => Value::String(if b { "True" } else { "False" }.to_owned()),
        (value, _) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosting::{AgentOptions, OverlayOptions};
    use chrono::TimeDelta;

    #[test]
    fn later_layers_win_per_key_and_casing_is_forgiven() {
        let dir = tempfile::tempdir().unwrap();
        let shipped = dir.path().join("appsettings.json");
        let user = dir.path().join("settings.json");
        std::fs::write(&shipped, r#"{"Agent": {"RustPath": null, "PollInterval": "00:00:00.150"},
            "Overlay": {"OffsetX": 16, "Inspect": {"HoldKey": "leftalt"}}}"#)
        .unwrap();
        std::fs::write(&user, r#"{"overlay": {"offsetx": 40, "inspect": {"cyclekey": "v"}}}"#).unwrap();

        let merged = load_layered(&[&shipped, &user, &dir.path().join("absent.json")]).unwrap();
        let overlay: OverlayOptions = section(&merged, OverlayOptions::SECTION).unwrap();
        let agent: AgentOptions = section(&merged, AgentOptions::SECTION).unwrap();

        assert_eq!(overlay.offset_x, 40);
        assert_eq!(overlay.inspect.hold_key, "leftalt");
        assert_eq!(overlay.inspect.cycle_key, "v");
        assert_eq!(overlay.offset_y, 16);
        assert_eq!(agent.rust_path, None);
        assert_eq!(agent.poll_interval, TimeDelta::milliseconds(150));
    }

    #[test]
    fn files_are_read_the_way_add_json_file_read_them() {
        let dir = tempfile::tempdir().unwrap();
        let shipped = dir.path().join("appsettings.json");
        let user = dir.path().join("settings.json");
        std::fs::write(&shipped, r#"{"Overlay": {"Inspect": {"SelectKeys": ["1", "2", "3"]}}}"#).unwrap();
        std::fs::write(
            &user,
            "\u{FEFF}{\n  // hand-edited\n  \"Overlay\": {\"ToggleKey\": 4, \"Inspect\": {\"SelectKeys\": [9,],},},\n}",
        )
        .unwrap();

        let merged = load_layered(&[&shipped, &user]).unwrap();
        let overlay: OverlayOptions = section(&merged, OverlayOptions::SECTION).unwrap();

        assert_eq!(overlay.toggle_key, "4");
        assert_eq!(overlay.inspect.select_keys, ["9", "2", "3"]);

        std::fs::write(&user, "[1]").unwrap();
        assert!(matches!(load_layered(&[&user]), Err(SettingsError::Json(..))));
    }

    #[test]
    fn environment_variables_override_files_and_bind_as_typed_values() {
        let mut merged = serde_json::json!({
            "Agent": {"ReplayFromStart": false},
            "Overlay": {"OffsetX": 16, "Scale": 1.0, "Inspect": {"HoldKey": "leftalt", "SelectKeys": ["1", "2", "3"]}},
        });
        merge(
            &mut merged,
            environment_layer([
                ("RUSTOVERLAY_Overlay__OffsetX", "40"),
                ("rustoverlay_overlay__inspect__holdkey", "leftshift"),
                ("RUSTOVERLAY_Overlay__Inspect__SelectKeys__1", "f2"),
                ("RUSTOVERLAY_Agent__ReplayFromStart", "True"),
                ("RUSTOVERLAY_Agent__PollInterval", "00:00:01"),
                ("RUSTOVERLAY_Overlay__Scale", "1.25"),
                ("UNRELATED", "x"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))),
        );

        let overlay: OverlayOptions = section(&merged, OverlayOptions::SECTION).unwrap();
        let agent: AgentOptions = section(&merged, AgentOptions::SECTION).unwrap();

        assert_eq!(overlay.offset_x, 40);
        assert_eq!(overlay.scale, 1.25);
        assert_eq!(overlay.inspect.hold_key, "leftshift");
        assert_eq!(overlay.inspect.select_keys, ["1", "f2", "3"]);
        assert!(agent.replay_from_start);
        assert_eq!(agent.poll_interval, TimeDelta::seconds(1));
    }
}
