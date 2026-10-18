use std::path::Path;
use std::ptr;

use windows::core::{GUID, HSTRING};
use windows::Win32::Graphics::GdiPlus::{
    BitmapData, GdipBitmapGetPixel, GdipBitmapLockBits, GdipBitmapUnlockBits, GdipCreateBitmapFromFile,
    GdipCreateBitmapFromScan0, GdipDisposeImage, GdipGetImageHeight, GdipGetImageWidth, GdipImageForceValidation,
    GdipSaveImageToFile, GpBitmap, GpImage, ImageLockModeRead, ImageLockModeWrite, Rect,
};

use super::{check, ensure_started, GdiplusError, PIXEL_FORMAT_32BPP_ARGB};

/// The built-in PNG encoder, `ImageFormat.Png`'s codec.
const PNG_ENCODER: GUID = GUID::from_u128(0x557cf406_1a04_11d3_9a73_0000f81ef32e);

/// An owned GDI+ bitmap. GDI+ allows one thread per bitmap, which `&mut` access enforces.
pub struct Bitmap {
    raw: *mut GpBitmap,
}

// SAFETY: a GDI+ bitmap may move between threads; it may not be used from two at once, which
// leaving it !Sync guarantees.
unsafe impl Send for Bitmap {}

impl Bitmap {
    /// `new Bitmap(width, height, PixelFormat.Format32bppArgb)`.
    pub fn new_argb(width: i32, height: i32) -> Result<Self, GdiplusError> {
        ensure_started()?;
        let mut raw = ptr::null_mut();
        // SAFETY: GDI+ allocates and owns the pixels when scan0 is null.
        check(
            unsafe { GdipCreateBitmapFromScan0(width, height, 0, PIXEL_FORMAT_32BPP_ARGB, None, &mut raw) },
            "GdipCreateBitmapFromScan0",
        )?;
        Ok(Self { raw })
    }

    /// `new Bitmap(path)`: decodes without colour management, then forces a full decode so a
    /// truncated file fails here rather than on first use.
    pub fn from_file(path: &Path) -> Result<Self, GdiplusError> {
        ensure_started()?;
        let name = HSTRING::from(path.as_os_str());
        let mut raw = ptr::null_mut();
        // SAFETY: the name outlives the call; GDI+ hands back an owned bitmap on success.
        check(unsafe { GdipCreateBitmapFromFile(&name, &mut raw) }, "GdipCreateBitmapFromFile")?;
        let bitmap = Self { raw };
        // SAFETY: the bitmap is valid; on failure Drop disposes it, as System.Drawing does.
        check(unsafe { GdipImageForceValidation(bitmap.image()) }, "GdipImageForceValidation")?;
        Ok(bitmap)
    }

    pub(crate) fn image(&self) -> *mut GpImage {
        self.raw.cast()
    }

    pub fn width(&self) -> Result<i32, GdiplusError> {
        let mut width = 0;
        // SAFETY: the bitmap is valid for the life of self.
        check(unsafe { GdipGetImageWidth(self.image(), &mut width) }, "GdipGetImageWidth")?;
        Ok(width as i32)
    }

    pub fn height(&self) -> Result<i32, GdiplusError> {
        let mut height = 0;
        // SAFETY: the bitmap is valid for the life of self.
        check(unsafe { GdipGetImageHeight(self.image(), &mut height) }, "GdipGetImageHeight")?;
        Ok(height as i32)
    }

    /// `Bitmap.GetPixel(x, y)` as a packed ARGB value.
    pub fn pixel(&self, x: i32, y: i32) -> Result<u32, GdiplusError> {
        let mut argb = 0;
        // SAFETY: the bitmap is valid; GDI+ bounds-checks the coordinates.
        check(unsafe { GdipBitmapGetPixel(self.raw, x, y, &mut argb) }, "GdipBitmapGetPixel")?;
        Ok(argb)
    }

    /// `LockBits(rect, ReadOnly, Format32bppArgb)`, copying `stride * height` bytes out. Returns the
    /// bytes and the stride.
    pub fn read_argb(&mut self, width: i32, height: i32) -> Result<(Vec<u8>, i32), GdiplusError> {
        let mut data = self.lock(width, height, ImageLockModeRead.0)?;
        let length = data.Stride as usize * height as usize;
        // SAFETY: a read lock maps `Stride * Height` readable bytes at Scan0 until unlocked.
        let pixels = unsafe { std::slice::from_raw_parts(data.Scan0.cast::<u8>(), length) }.to_vec();
        self.unlock(&mut data)?;
        Ok((pixels, data.Stride))
    }

    /// `LockBits(rect, WriteOnly, Format32bppArgb)`, filling each row through `write_row`.
    pub fn write_argb(
        &mut self,
        width: i32,
        height: i32,
        mut write_row: impl FnMut(i32, &mut [u8]),
    ) -> Result<(), GdiplusError> {
        let mut data = self.lock(width, height, ImageLockModeWrite.0)?;
        for y in 0..height {
            // SAFETY: a write lock maps `Stride` writable bytes per row until unlocked.
            let row = unsafe {
                std::slice::from_raw_parts_mut(
                    data.Scan0.cast::<u8>().add(y as usize * data.Stride as usize),
                    data.Stride as usize,
                )
            };
            write_row(y, row);
        }
        self.unlock(&mut data)
    }

    /// `Image.Save(path, ImageFormat.Png)`.
    pub fn save_png(&self, path: &Path) -> Result<(), GdiplusError> {
        let name = HSTRING::from(path.as_os_str());
        // SAFETY: the encoder id and name outlive the call; no encoder parameters, as in C#.
        check(
            unsafe { GdipSaveImageToFile(self.image(), &name, &PNG_ENCODER, ptr::null()) },
            "GdipSaveImageToFile",
        )
    }

    fn lock(&mut self, width: i32, height: i32, mode: i32) -> Result<BitmapData, GdiplusError> {
        let rect = Rect { X: 0, Y: 0, Width: width, Height: height };
        let mut data = BitmapData::default();
        // SAFETY: the rect lies within the bitmap; GDI+ fills `data`.
        check(
            unsafe { GdipBitmapLockBits(self.raw, &rect, mode as u32, PIXEL_FORMAT_32BPP_ARGB, &mut data) },
            "GdipBitmapLockBits",
        )?;
        Ok(data)
    }

    fn unlock(&mut self, data: &mut BitmapData) -> Result<(), GdiplusError> {
        // SAFETY: `data` came from the matching lock.
        check(unsafe { GdipBitmapUnlockBits(self.raw, data) }, "GdipBitmapUnlockBits")
    }
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: owned and not yet disposed.
        unsafe { GdipDisposeImage(self.image()) };
    }
}
