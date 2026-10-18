use windows::Win32::Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

/// The endpoint games render to by default (the console role, which is what Unity opens).
/// Joins the calling thread to the MTA first; repeat calls on one thread are harmless.
pub fn default_render_device() -> Result<IMMDevice, String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|e| format!("CoInitializeEx: {e}"))?;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("MMDeviceEnumerator: {e}"))?;
        enumerator.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|e| format!("default render endpoint: {e}"))
    }
}
