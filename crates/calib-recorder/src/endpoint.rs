use serde_json::{json, Value};
use windows::core::PWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDevice, ISpatialAudioClient};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CoTaskMemFree, CLSCTX_ALL, STGM_READ};

/// Name, id, volume, mute and spatial-audio state of `device`, as session.json reports them.
/// Each probe that fails is reported as null with its error rather than failing the recording.
pub fn describe(device: &IMMDevice) -> Value {
    let id = unsafe { device.GetId() }.map(take_pwstr).map_err(|e| e.to_string());
    let name = friendly_name(device);
    let volume: Result<IAudioEndpointVolume, _> = unsafe { device.Activate(CLSCTX_ALL, None) };
    let (scalar, muted) = match &volume {
        Ok(v) => unsafe { (v.GetMasterVolumeLevelScalar().ok(), v.GetMute().ok().map(|m| m.as_bool())) },
        Err(_) => (None, None),
    };

    json!({
        "name": name.unwrap_or_else(|e| format!("<unknown: {e}>")),
        "id": id.unwrap_or_else(|e| format!("<unknown: {e}>")),
        "volume_scalar": scalar,
        "muted": muted,
        "spatial_audio": spatial_audio(device),
    })
}

fn friendly_name(device: &IMMDevice) -> Result<String, String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).map_err(|e| e.to_string())?;
        let value = store.GetValue(&PKEY_Device_FriendlyName).map_err(|e| e.to_string())?;
        PropVariantToStringAlloc(&value).map(take_pwstr).map_err(|e| e.to_string())
    }
}

/// ISpatialAudioClient activates on any spatial-capable endpoint, but only reports dynamic objects
/// while a spatial format (Windows Sonic, Dolby Atmos, DTS) is switched on for it.
fn spatial_audio(device: &IMMDevice) -> Value {
    let client: Result<ISpatialAudioClient, _> = unsafe { device.Activate(CLSCTX_ALL, None) };
    match client {
        Err(e) => json!({ "active": false, "detail": format!("ISpatialAudioClient unavailable: {e}") }),
        Ok(client) => match unsafe { client.GetMaxDynamicObjectCount() } {
            Ok(objects) => json!({ "active": objects > 0, "max_dynamic_objects": objects }),
            Err(e) => json!({ "active": null, "detail": format!("GetMaxDynamicObjectCount: {e}") }),
        },
    }
}

fn take_pwstr(text: PWSTR) -> String {
    let owned = unsafe { text.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(text.0 as *const _)) };
    owned
}
