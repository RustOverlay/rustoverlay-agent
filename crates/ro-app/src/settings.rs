//! Configuration layering, as `AgentHost` built it: the shipped appsettings.json, the per-user
//! settings.json on top, then `RUSTOVERLAY_` environment variables. Later layers win per key, and
//! keys match case-insensitively.

use std::path::{Path, PathBuf};

use ro_core::hosting::settings_file::{SettingsError, load_with_environment, merge, section};
use ro_core::hosting::{AgentOptions, OverlayOptions};
use ro_net::enrollment::BackendOptions;
use ro_net::relay::RelayOptions;
use serde_json::Value;

/// The appsettings.json the C# agent shipped beside its exe. Embedded as the lowest layer, so a
/// missing file beside the binary still starts with the shipped values.
const SHIPPED: &str = include_str!("../appsettings.json");

#[derive(Debug, Clone)]
pub struct Options {
    pub agent: AgentOptions,
    pub backend: BackendOptions,
    pub relay: RelayOptions,
    pub overlay: OverlayOptions,
    /// An `EnvFilter` directive built from the `Serilog:MinimumLevel` section.
    pub log_filter: String,
}

pub fn load(data_directory: &Path) -> Result<Options, SettingsError> {
    let mut merged: Value = serde_json::from_str(SHIPPED).expect("the embedded appsettings.json is valid JSON");

    let beside_exe = exe_directory().join("appsettings.json");
    let user = data_directory.join("settings.json");
    merge(&mut merged, load_with_environment(&[&beside_exe, &user])?);

    bind_all(&merged)
}

fn bind_all(merged: &Value) -> Result<Options, SettingsError> {
    Ok(Options {
        agent: section(merged, AgentOptions::SECTION)?,
        backend: section(merged, BackendOptions::SECTION)?,
        relay: section(merged, RelayOptions::SECTION)?,
        overlay: section(merged, OverlayOptions::SECTION)?,
        log_filter: log_filter(merged),
    })
}

fn exe_directory() -> PathBuf {
    std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)).unwrap_or_default()
}

/// `Serilog:MinimumLevel` — `Default` plus per-source `Override`s — as an `EnvFilter` directive.
fn log_filter(merged: &Value) -> String {
    let find = |v: &Value, key: &str| -> Option<Value> {
        v.as_object()?.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.clone())
    };
    let level = find(merged, "Serilog").and_then(|s| find(&s, "MinimumLevel"));

    let default = match &level {
        Some(Value::String(s)) => Some(s.clone()),
        Some(v) => find(v, "Default").and_then(|d| d.as_str().map(str::to_owned)),
        None => None,
    };
    let mut directives = vec![tracing_level(default.as_deref().unwrap_or("Information")).to_owned()];

    if let Some(Value::Object(overrides)) = level.as_ref().and_then(|v| find(v, "Override")) {
        for (source, value) in overrides {
            if let Some(name) = value.as_str() {
                directives.push(format!("{source}={}", tracing_level(name)));
            }
        }
    }

    directives.join(",")
}

fn tracing_level(serilog: &str) -> &'static str {
    match serilog.to_ascii_lowercase().as_str() {
        "verbose" => "trace",
        "debug" => "debug",
        "warning" => "warn",
        "error" | "fatal" => "error",
        _ => "info",
    }
}

#[cfg(test)]
mod tests {
    use ro_core::hosting::settings_file::environment_layer;

    use super::*;

    #[test]
    fn environment_variables_override_the_shipped_file_and_bind_as_typed_values() {
        let mut merged: Value = serde_json::from_str(SHIPPED).unwrap();
        let vars = [
            ("RUSTOVERLAY_Overlay__OffsetX", "40"),
            ("rustoverlay_overlay__inspect__holdkey", "leftshift"),
            ("RUSTOVERLAY_Agent__ReplayFromStart", "True"),
            ("RUSTOVERLAY_Overlay__Scale", "1.25"),
            ("UNRELATED", "x"),
        ];
        merge(&mut merged, environment_layer(vars.map(|(k, v)| (k.to_owned(), v.to_owned()))));

        let options = bind_all(&merged).unwrap();

        assert_eq!(options.overlay.offset_x, 40);
        assert_eq!(options.overlay.offset_y, 16);
        assert_eq!(options.overlay.scale, 1.25);
        assert_eq!(options.overlay.inspect.hold_key, "leftshift");
        assert!(options.agent.replay_from_start);
    }

    #[test]
    fn the_shipped_file_binds_to_the_csharp_defaults() {
        let merged: Value = serde_json::from_str(SHIPPED).unwrap();
        let options = bind_all(&merged).unwrap();

        assert_eq!(options.agent, AgentOptions::default());
        assert_eq!(options.relay, RelayOptions::default());
        assert_eq!(options.backend, BackendOptions::default());
        assert_eq!(options.overlay, OverlayOptions::default());
        assert_eq!(options.log_filter, "info,Microsoft=warn");
    }
}
