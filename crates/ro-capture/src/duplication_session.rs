//! The D3D half of [`crate::DuplicationFrameSource`]: one display's duplication and the copy out of
//! it.

use ro_core::capture::ScreenRect;
use windows::core::{Error, Interface};
use windows::Win32::Foundation::{E_POINTER, HMODULE};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIOutputDuplication, DXGI_OUTDUPL_FRAME_INFO};

/// Field order is drop order, which releases the staging texture and duplication before the device.
pub(crate) struct Session {
    /// Created on first use, since its size comes from the frame rather than the output.
    staging: Option<ID3D11Texture2D>,
    pub duplication: IDXGIOutputDuplication,
    context: ID3D11DeviceContext,
    device: ID3D11Device,
    pub bounds: ScreenRect,
}

/// What one acquisition produced.
pub(crate) enum Acquired {
    /// A desktop image, copied out with this row pitch.
    Image { stride: i32 },
    /// Only the cursor moved; there is no image attached.
    NoImage,
}

impl Session {
    pub fn new(device: ID3D11Device, context: ID3D11DeviceContext, duplication: IDXGIOutputDuplication, bounds: ScreenRect) -> Self {
        Self { staging: None, duplication, context, device, bounds }
    }

    /// Copies an acquired frame out, if it has an image. The caller has already acquired it and
    /// releases it afterwards.
    pub fn take(
        &mut self,
        info: &DXGI_OUTDUPL_FRAME_INFO,
        resource: Option<windows::Win32::Graphics::Dxgi::IDXGIResource>,
        pixels: &mut Vec<u8>,
    ) -> Result<Acquired, Error> {
        // Zero means nothing was presented and only the cursor moved; there is no image attached to
        // this frame, so releasing and asking again is the way to a real one.
        if info.LastPresentTime == 0 {
            return Ok(Acquired::NoImage);
        }

        let texture: ID3D11Texture2D = resource.ok_or_else(|| Error::from(E_POINTER))?.cast()?;
        let stride = self.copy_out(&texture, pixels)?;

        Ok(Acquired::Image { stride })
    }

    /// Copies the GPU frame through a staging texture into `pixels`, returning the row pitch.
    ///
    /// Gotcha: the mapping is opened and closed inside this call rather than held for the caller.
    /// A frame is read by the whole recognition pass, and keeping a staging resource mapped across
    /// all of it stalls the GPU for far longer than the copy costs.
    fn copy_out(&mut self, frame: &ID3D11Texture2D, pixels: &mut Vec<u8>) -> Result<i32, Error> {
        let mut description = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: plain descriptor query on a live texture.
        unsafe { frame.GetDesc(&mut description) };

        // Size is rechecked rather than assumed: a mode change normally arrives as ACCESS_LOST and
        // rebuilds the session, but a staging texture that outlived one would fail the copy below.
        let reusable = self.staging.as_ref().is_some_and(|staging| {
            let mut current = D3D11_TEXTURE2D_DESC::default();
            // SAFETY: as above.
            unsafe { staging.GetDesc(&mut current) };
            current.Width == description.Width && current.Height == description.Height
        });

        if !reusable {
            self.staging = None;

            description.Usage = D3D11_USAGE_STAGING;
            description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            description.BindFlags = 0;
            description.MiscFlags = 0;

            let mut staging = None;
            // SAFETY: the descriptor is fully initialised; no initial data.
            unsafe { self.device.CreateTexture2D(&description, None, Some(&mut staging)) }?;
            self.staging = staging;
        }

        let staging = self.staging.as_ref().ok_or_else(|| Error::from(E_POINTER))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();

        // SAFETY: same-sized textures on one device; the map is released before return.
        unsafe {
            self.context.CopyResource(staging, frame);
            self.context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;

            let stride = mapped.RowPitch as i32;
            let length = stride as usize * description.Height as usize;
            if pixels.len() < length {
                *pixels = vec![0; length];
            }
            std::ptr::copy_nonoverlapping(mapped.pData.cast::<u8>(), pixels.as_mut_ptr(), length);

            self.context.Unmap(staging, 0);

            Ok(stride)
        }
    }
}

/// Creates a device on the GPU driving a specific display.
///
/// Gotcha: naming the adapter is the whole point. Duplication cannot cross adapters, which is how it
/// fails on hybrid-graphics laptops when the device lands on the integrated GPU while the panel is
/// driven by the discrete one. The driver type must be UNKNOWN whenever an adapter is given.
pub(crate) fn create_device(adapter: &IDXGIAdapter) -> Result<(ID3D11Device, ID3D11DeviceContext), Error> {
    let (mut device, mut context) = (None, None);

    // SAFETY: out-pointers are valid locals.
    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }

    match (device, context) {
        (Some(device), Some(context)) => Ok((device, context)),
        _ => Err(Error::from(E_POINTER)),
    }
}
