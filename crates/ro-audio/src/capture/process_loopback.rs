use std::mem::ManuallyDrop;

use windows::core::{implement, Interface, Ref, HRESULT, IUnknown};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioClient, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{IAgileObject, IAgileObject_Impl, BLOB};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use super::MixFormat;

const ACTIVATE_TIMEOUT_MS: u32 = 5_000;

/// ActivateAudioInterfaceAsync calls back on a worker thread, so the handler must be agile.
#[implement(IActivateAudioInterfaceCompletionHandler, IAgileObject)]
struct Completion {
    done: HANDLE,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(&self, _operation: Ref<IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        unsafe { SetEvent(self.done) }
    }
}

impl IAgileObject_Impl for Completion_Impl {}

/// An IAudioClient that sees only what `pid` and its descendants render (Windows 10 2004+).
/// It has no mix format of its own; the caller picks one in `Initialize` and the engine converts.
pub fn activate(pid: u32) -> Result<IAudioClient, String> {
    let done = unsafe { CreateEventW(None, true, false, None) }.map_err(|e| format!("CreateEventW: {e}"))?;
    let result = activate_with(pid, done);
    let _ = unsafe { CloseHandle(done) };
    result
}

fn activate_with(pid: u32, done: HANDLE) -> Result<IAudioClient, String> {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    // Gotcha: windows-rs PROPVARIANT's Drop calls PropVariantClear, which would CoTaskMemFree this
    // borrowed stack blob and corrupt the heap (0xC0000374). Never let it drop.
    let mut variant = ManuallyDrop::new(PROPVARIANT::default());
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_BLOB;
        inner.Anonymous.blob = BLOB {
            cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: (&raw mut params).cast(),
        };
    }

    let handler: IActivateAudioInterfaceCompletionHandler = Completion { done }.into();
    let operation = unsafe {
        ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*variant), &handler)
    }
    .map_err(|e| format!("ActivateAudioInterfaceAsync (process loopback, pid {pid}): {e}"))?;

    if unsafe { WaitForSingleObject(done, ACTIVATE_TIMEOUT_MS) } != WAIT_OBJECT_0 {
        return Err(format!("process loopback activation for pid {pid} did not complete in {ACTIVATE_TIMEOUT_MS} ms"));
    }
    let (mut status, mut unknown) = (HRESULT(0), None::<IUnknown>);
    unsafe { operation.GetActivateResult(&mut status, &mut unknown) }.map_err(|e| format!("GetActivateResult: {e}"))?;
    status.ok().map_err(|e| format!("process loopback activation for pid {pid}: {e}"))?;
    unknown
        .ok_or("process loopback activation returned no interface")?
        .cast()
        .map_err(|e| format!("process loopback IAudioClient: {e}"))
}

pub fn float_format(format: &MixFormat) -> WAVEFORMATEXTENSIBLE {
    let block_align = format.channels * 4;
    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: format.channels,
            nSamplesPerSec: format.rate,
            nAvgBytesPerSec: format.rate * u32::from(block_align),
            nBlockAlign: block_align,
            wBitsPerSample: 32,
            cbSize: (size_of::<WAVEFORMATEXTENSIBLE>() - size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 { wValidBitsPerSample: 32 },
        dwChannelMask: format.channel_mask,
        SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    }
}
