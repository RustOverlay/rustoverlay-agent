use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, IMMDevice, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};
use windows::Win32::System::Com::{CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use super::clock::Clock;
use super::{process_loopback, FLAG_QPC_ESTIMATED, FLAG_SILENT, FLAG_TIMESTAMP_ERROR};

/// 500 ms of engine buffering: far more than the poll interval, so a stalled poll never drops audio.
const BUFFER_100NS: i64 = 5_000_000;

/// A device QPC further than this from the retrieval-time estimate is treated as garbage.
const QPC_SANITY_100NS: i64 = 2_000_000;

pub const FALLBACK_FORMAT: MixFormat = MixFormat { rate: 48_000, channels: 2, bits: 32, channel_mask: 0x3, float32: true };

#[derive(Clone, Copy, Debug)]
pub struct MixFormat {
    pub rate: u32,
    pub channels: u16,
    pub bits: u16,
    pub channel_mask: u32,
    pub float32: bool,
}

/// One engine packet. `samples` is empty when the engine flagged it silent. `qpc_100ns` is the
/// timestamp to use (see `LoopbackCapture::drain`); the other two are kept for accuracy stats.
pub struct Packet<'a> {
    pub samples: &'a [f32],
    pub frames: u32,
    pub flags: u32,
    pub device_position: u64,
    pub qpc_100ns: u64,
    pub device_qpc_100ns: u64,
    pub estimate_100ns: i64,
}

pub struct LoopbackCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: Option<HANDLE>,
    clock: Clock,
    pub format: MixFormat,
}

impl LoopbackCapture {
    /// Opens a shared-mode loopback stream on `device` in its own mix format. Only float32 mixes
    /// are accepted, which is what every shared-mode engine since Vista hands out.
    pub fn open(device: &IMMDevice) -> Result<Self, String> {
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| format!("activate IAudioClient: {e}"))?;
            let mix = client.GetMixFormat().map_err(|e| format!("GetMixFormat: {e}"))?;
            let format = parse_mix_format(mix);

            let initialized = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, BUFFER_100NS, 0, mix, None);
            CoTaskMemFree(Some(mix as *const _));
            initialized.map_err(|e| format!("IAudioClient::Initialize (loopback): {e}"))?;

            if !format.float32 {
                return Err(format!("mix format is {}-bit integer, expected float32: {format:?}", format.bits));
            }
            Self::with_client(client, format, None)
        }
    }

    /// Opens a loopback stream of only what `pid` and its child processes render, in `format`.
    pub fn open_process(pid: u32, format: MixFormat) -> Result<Self, String> {
        let client = process_loopback::activate(pid)?;
        let wanted = process_loopback::float_format(&format);
        let flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM;
        unsafe {
            client
                .Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_100NS, 0, (&raw const wanted).cast(), None)
                .map_err(|e| format!("IAudioClient::Initialize (process loopback): {e}"))?;
            let event = CreateEventW(None, false, false, None).map_err(|e| format!("CreateEventW: {e}"))?;
            if let Err(e) = client.SetEventHandle(event) {
                let _ = CloseHandle(event);
                return Err(format!("SetEventHandle: {e}"));
            }
            Self::with_client(client, format, Some(event))
        }
    }

    unsafe fn with_client(client: IAudioClient, format: MixFormat, event: Option<HANDLE>) -> Result<Self, String> {
        let capture: IAudioCaptureClient = unsafe { client.GetService() }.map_err(|e| format!("GetService(IAudioCaptureClient): {e}"))?;
        Ok(Self { client, capture, event, clock: Clock::new(), format })
    }

    pub fn start(&self) -> Result<(), String> {
        unsafe { self.client.Start() }.map_err(|e| format!("IAudioClient::Start: {e}"))
    }

    /// Blocks until the engine signals a packet (event-driven streams) or `timeout` passes.
    pub fn wait(&self, timeout: Duration) {
        match self.event {
            Some(event) => {
                let _ = unsafe { WaitForSingleObject(event, timeout.as_millis() as u32) };
            }
            None => std::thread::sleep(timeout),
        }
    }

    /// Hands every packet queued since the last call to `sink`, oldest first.
    ///
    /// Each packet also gets a retrieval-time estimate of its first frame's QPC: now minus every
    /// frame still queued (GetCurrentPadding, this packet included). Process loopback streams use it
    /// whenever GetBuffer's QPC is zero, flagged, or implausible, and mark the packet FLAG_QPC_ESTIMATED.
    pub fn drain(&self, mut sink: impl FnMut(Packet<'_>) -> Result<(), String>) -> Result<(), String> {
        let channels = usize::from(self.format.channels);
        let rate = i64::from(self.format.rate);
        loop {
            let pending = unsafe { self.capture.GetNextPacketSize() }.map_err(|e| format!("GetNextPacketSize: {e}"))?;
            if pending == 0 {
                return Ok(());
            }
            let queued = unsafe { self.client.GetCurrentPadding() }.unwrap_or(pending);

            let (mut data, mut frames, mut flags, mut device_position, mut qpc) = (std::ptr::null_mut(), 0u32, 0u32, 0u64, 0u64);
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, Some(&mut device_position), Some(&mut qpc))
                    .map_err(|e| format!("GetBuffer: {e}"))?;
            }
            let estimate = self.clock.qpc_100ns() - i64::from(queued.max(frames)) * 10_000_000 / rate;

            let silent = flags & FLAG_SILENT != 0;
            let samples: &[f32] = if silent || data.is_null() {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(data.cast::<f32>(), frames as usize * channels) }
            };

            let device_ok = qpc != 0 && flags & FLAG_TIMESTAMP_ERROR == 0 && (qpc as i64 - estimate).abs() < QPC_SANITY_100NS;
            let (chosen, flags) = if self.event.is_some() && !device_ok {
                (estimate as u64, (flags & !FLAG_TIMESTAMP_ERROR) | FLAG_QPC_ESTIMATED)
            } else {
                (qpc, flags)
            };
            let device_qpc = if device_ok { qpc } else { 0 };

            let packet = Packet { samples, frames, flags, device_position, qpc_100ns: chosen, device_qpc_100ns: device_qpc, estimate_100ns: estimate };
            let result = sink(packet);
            unsafe { self.capture.ReleaseBuffer(frames) }.map_err(|e| format!("ReleaseBuffer: {e}"))?;
            result?;
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
        if let Some(event) = self.event {
            let _ = unsafe { CloseHandle(event) };
        }
    }
}

/// The shared-mode mix format of `device`, which process loopback is asked to match.
pub fn device_mix_format(device: &IMMDevice) -> Result<MixFormat, String> {
    unsafe {
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| format!("activate IAudioClient: {e}"))?;
        let mix = client.GetMixFormat().map_err(|e| format!("GetMixFormat: {e}"))?;
        let format = parse_mix_format(mix);
        CoTaskMemFree(Some(mix as *const _));
        Ok(format)
    }
}

/// WAVEFORMATEX is packed, hence the unaligned reads.
unsafe fn parse_mix_format(mix: *const WAVEFORMATEX) -> MixFormat {
    let base = unsafe { std::ptr::read_unaligned(mix) };
    let (channel_mask, float32) = if u32::from(base.wFormatTag) == WAVE_FORMAT_EXTENSIBLE {
        let ext = unsafe { std::ptr::read_unaligned(mix.cast::<WAVEFORMATEXTENSIBLE>()) };
        let sub = ext.SubFormat;
        (ext.dwChannelMask, sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT && base.wBitsPerSample == 32)
    } else {
        (0, u32::from(base.wFormatTag) == WAVE_FORMAT_IEEE_FLOAT && base.wBitsPerSample == 32)
    };

    MixFormat { rate: base.nSamplesPerSec, channels: base.nChannels, bits: base.wBitsPerSample, channel_mask, float32 }
}
