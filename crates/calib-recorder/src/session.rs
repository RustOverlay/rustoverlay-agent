use std::path::{Path, PathBuf};

use ro_core::config::ClientConfig;
use ro_core::install::{RustInstallLocator, SteamRustInstallLocator};
use serde_json::{json, Value};

use ro_audio::capture::MixFormat;

const FALLBACK_CLIENT_CFG: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Rust\cfg\client.cfg";
const AUDIO_CONVARS: [&str; 3] = ["audio.master", "audio.game", "audio.speakers"];

pub fn mix_format_json(format: &MixFormat) -> Value {
    json!({
        "rate": format.rate,
        "channels": format.channels,
        "bits": format.bits,
        "channel_mask": format!("0x{:x}", format.channel_mask),
        "sample_format": "float32",
    })
}

/// The client's saved audio convars. client.cfg is written when the game exits, so a value
/// changed in the running session is not reflected until then.
pub fn rust_audio_convars() -> Value {
    let path = SteamRustInstallLocator::new(None)
        .locate()
        .map(|install| install.client_config_path())
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(FALLBACK_CLIENT_CFG));

    let mut convars = serde_json::Map::new();
    convars.insert("path".into(), json!(path.display().to_string()));
    match ClientConfig::load(&path) {
        Ok(config) => {
            for name in AUDIO_CONVARS {
                convars.insert(name.into(), json!(config.get(name)));
            }
        }
        Err(e) => {
            convars.insert("error".into(), json!(e.to_string()));
        }
    }
    Value::Object(convars)
}

pub fn utc_iso(utc_100ns: i64) -> String {
    let seconds = chrono::DateTime::from_timestamp_nanos(utc_100ns.saturating_mul(100)).format("%Y-%m-%dT%H:%M:%S");
    format!("{seconds}.{:07}Z", utc_100ns.rem_euclid(10_000_000))
}

pub fn write(path: &Path, session: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
