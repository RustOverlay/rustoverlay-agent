use std::ptr;

use windows::Win32::Graphics::GdiPlus::{
    FontStyle, GdipCreateFont, GdipDeleteFont, GdipDeletePrivateFontCollection, GdipGetFontCollectionFamilyCount,
    GdipGetFontCollectionFamilyList, GdipNewPrivateFontCollection, GdipPrivateAddMemoryFont, GpFont, GpFontCollection,
    GpFontFamily, UnitPixel,
};

use super::{check, ensure_started, GdiplusError};

/// `PrivateFontCollection` over font bytes that live for the whole process.
pub(crate) struct PrivateFonts {
    raw: *mut GpFontCollection,
}

// SAFETY: only ever used under the owner's lock; GDI+ objects may move between threads.
unsafe impl Send for PrivateFonts {}

impl PrivateFonts {
    /// Gotcha: GDI+ keeps reading the memory for the collection's lifetime, hence `'static`.
    pub fn from_memory(font: &'static [u8]) -> Result<Self, GdiplusError> {
        ensure_started()?;
        let mut raw = ptr::null_mut();
        // SAFETY: GDI+ allocates the collection.
        check(unsafe { GdipNewPrivateFontCollection(&mut raw) }, "GdipNewPrivateFontCollection")?;
        let fonts = Self { raw };
        // SAFETY: the bytes are 'static, so they outlive the collection.
        check(
            unsafe { GdipPrivateAddMemoryFont(fonts.raw, font.as_ptr().cast(), font.len() as i32) },
            "GdipPrivateAddMemoryFont",
        )?;
        Ok(fonts)
    }

    /// `Families[0]`. The family belongs to the collection, so it is borrowed from it.
    pub fn first_family(&self) -> Result<*mut GpFontFamily, GdiplusError> {
        let mut count = 0;
        // SAFETY: valid collection.
        check(unsafe { GdipGetFontCollectionFamilyCount(self.raw, &mut count) }, "GdipGetFontCollectionFamilyCount")?;
        let mut families = vec![ptr::null_mut(); count.max(0) as usize];
        let mut found = 0;
        // SAFETY: the slice is sized to the count just read.
        check(
            unsafe { GdipGetFontCollectionFamilyList(self.raw, &mut families, &mut found) },
            "GdipGetFontCollectionFamilyList",
        )?;
        families.first().copied().filter(|f| !f.is_null()).ok_or(GdiplusError {
            status: windows::Win32::Graphics::GdiPlus::FontFamilyNotFound.0,
            call: "PrivateFontCollection.Families[0]",
        })
    }
}

impl Drop for PrivateFonts {
    fn drop(&mut self) {
        // SAFETY: owned and not yet deleted.
        unsafe { GdipDeletePrivateFontCollection(&mut self.raw) };
    }
}

/// `new Font(family, size, style, GraphicsUnit.Pixel)`.
pub(crate) struct Font {
    raw: *mut GpFont,
}

impl Font {
    pub fn new(fonts: &PrivateFonts, size: f32, style: FontStyle) -> Result<Self, GdiplusError> {
        let family = fonts.first_family()?;
        let mut raw = ptr::null_mut();
        // SAFETY: the family is owned by `fonts`, which outlives this call.
        check(unsafe { GdipCreateFont(family, size, style.0, UnitPixel, &mut raw) }, "GdipCreateFont")?;
        Ok(Self { raw })
    }

    pub(crate) fn raw(&self) -> *const GpFont {
        self.raw
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: owned and not yet deleted.
        unsafe { GdipDeleteFont(self.raw) };
    }
}
