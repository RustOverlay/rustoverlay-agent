//! Builds [`IconSignature`]s from catalogue PNGs.
//!
//! Captured slots are cropped by `FrameSlotSampler` instead, which reads a `FrameBuffer`: GDI+
//! cannot be driven from several threads over one bitmap, and the area filter it uses beat
//! `HighQualityBicubic` on the labelled corpus anyway.

use std::path::Path;

use rayon::prelude::*;
use ro_vision::items::{Catalogue, IconFace, IconSignature};
use windows::Win32::Graphics::GdiPlus::{
    CompositingModeSourceCopy, InterpolationModeHighQualityBicubic, PixelOffsetModeHighQuality,
};

use crate::gdiplus::{Bitmap, GdiplusError, Graphics};
use crate::signature_cache::{SignatureCache, Stamp, DOWNSCALE_BYTES};
use crate::{StopToken, Stopped};

const N: i32 = IconSignature::SIZE as i32;

/// Reads a catalogue icon. Its alpha channel becomes the mask.
pub fn from_file(path: &Path) -> Result<IconSignature, GdiplusError> {
    Ok(signature_from_argb(&downscale_file(path)?, N as usize * 4, true))
}

/// As [`from_file`], but answered from `cache` when the file is unchanged since it was cached.
fn from_file_cached(path: &Path, cache: &SignatureCache) -> Result<IconSignature, GdiplusError> {
    let stamp = Stamp::of(path);
    let pixels = match stamp.and_then(|stamp| cache.get(path, stamp)) {
        Some(pixels) => pixels,
        None => {
            let pixels = downscale_file(path)?;
            if let Some(stamp) = stamp {
                cache.put(path.to_owned(), stamp, pixels.clone());
            }
            pixels
        }
    };
    Ok(signature_from_argb(&pixels, N as usize * 4, true))
}

/// Builds signatures for many faces at once, returning the built ones and the faces whose icon
/// would not decode.
///
/// Decoding dominates startup — 637 MB of PNG across the catalogue and the skin cache — and it is
/// one bitmap per file, so GDI+'s one-thread-per-bitmap rule does not stop it running in parallel.
/// A skin icon caught mid-download will not decode; that is expected (see `SkinCatalog::rescan`),
/// so it is reported for the caller to retry rather than raised.
///
/// Both lists keep the order of `faces`.
// TODO(port): C# collected through ConcurrentDictionary/ConcurrentBag, whose order is arbitrary;
// C# also let GDI+ statuses other than InvalidParameter/OutOfMemory/FileNotFound escape.
pub fn build(faces: impl IntoIterator<Item = IconFace>) -> (Catalogue, Vec<IconFace>) {
    unstoppable(build_with(faces, None, None))
}

/// As [`build`], decoding only icons `cache` has not seen at their current revision. The caller
/// saves the cache.
pub fn build_cached(faces: impl IntoIterator<Item = IconFace>, cache: &SignatureCache) -> (Catalogue, Vec<IconFace>) {
    unstoppable(build_with(faces, Some(cache), None))
}

/// As [`build_cached`], but gives up between icons once `stop` is set. Icons decoded by then are
/// already in `cache`, so saving it spares the next launch their decode.
pub fn build_cached_until(
    faces: impl IntoIterator<Item = IconFace>,
    cache: &SignatureCache,
    stop: &StopToken,
) -> Result<(Catalogue, Vec<IconFace>), Stopped> {
    build_with(faces, Some(cache), Some(stop))
}

fn unstoppable(built: Result<(Catalogue, Vec<IconFace>), Stopped>) -> (Catalogue, Vec<IconFace>) {
    built.unwrap_or_else(|Stopped| unreachable!("built without a stop token"))
}

fn build_with(
    faces: impl IntoIterator<Item = IconFace>,
    cache: Option<&SignatureCache>,
    stop: Option<&StopToken>,
) -> Result<(Catalogue, Vec<IconFace>), Stopped> {
    let faces: Vec<IconFace> = faces.into_iter().filter(|f| f.icon_path.is_some()).collect();

    // Collecting Options short-circuits: once one face sees the stop, rayon hands out no more.
    let results: Option<Vec<(IconFace, Option<IconSignature>)>> = faces
        .into_par_iter()
        .map(|face| {
            if stop.is_some_and(StopToken::is_stopped) {
                return None;
            }
            let signature = face.icon_path.as_deref().and_then(|path| match cache {
                Some(cache) => from_file_cached(path, cache).ok(),
                None => from_file(path).ok(),
            });
            Some((face, signature))
        })
        .collect();
    let results = results.ok_or(Stopped)?;

    let mut built = Catalogue::new();
    let mut unreadable = Vec::new();

    for (face, signature) in results {
        match signature {
            Some(signature) => {
                built.insert(face, signature);
            }
            None => unreadable.push(face),
        }
    }

    Ok((built, unreadable))
}

/// The icon downscaled to `N`x`N` BGRA, packed at stride `N * 4`: the input to
/// [`signature_from_argb`], and what the signature cache stores.
///
/// Stays GDI+ on measurement, not parity (2026-10-17). A pure-Rust png decode plus downscale
/// finishes ~4x sooner (GDI+ serialises internally) at the same CPU, but every filter tried —
/// box, scaled bicubic, box-prefiltered bicubic — raised single-frame wrong reads on the skins
/// corpus; the box filter gave four confirmed ones (a vanilla `50cal.mounted` over an uncached
/// green door skin). The cache makes it a first-launch cost only.
fn downscale_file(path: &Path) -> Result<Box<[u8]>, GdiplusError> {
    let source = Bitmap::from_file(path)?;

    // Box-filtered downscale: at 512 -> 32 a sampling resize would alias badly and two similar
    // icons could differ only in the pixels it happened to miss.
    let mut small = Bitmap::new_argb(N, N)?;
    {
        let g = Graphics::from_image(&small)?;
        g.set_interpolation_mode(InterpolationModeHighQualityBicubic)?;
        g.set_pixel_offset_mode(PixelOffsetModeHighQuality)?;
        g.set_compositing_mode(CompositingModeSourceCopy)?;
        g.draw_image_rect(&source, [0, 0, N, N])?;
    }

    let (bytes, stride) = small.read_argb(N, N)?;
    let row = N as usize * 4;
    let packed: Box<[u8]> = bytes.chunks(stride as usize).take(N as usize).flat_map(|r| &r[..row]).copied().collect();
    debug_assert_eq!(packed.len(), DOWNSCALE_BYTES);
    Ok(packed)
}

/// Splits a 32x32 BGRA block into the signature's planar RGB and alpha, each scaled to 0..1.
pub(crate) fn signature_from_argb(bytes: &[u8], stride: usize, use_alpha: bool) -> IconSignature {
    let n = IconSignature::SIZE;
    let mut rgb = vec![0f32; n * n * 3];
    let mut alpha = vec![0f32; n * n];

    for y in 0..n {
        for x in 0..n {
            let i = y * stride + x * 4;
            let cell = y * n + x;

            rgb[cell * 3] = f32::from(bytes[i + 2]) / 255f32;
            rgb[cell * 3 + 1] = f32::from(bytes[i + 1]) / 255f32;
            rgb[cell * 3 + 2] = f32::from(bytes[i]) / 255f32;
            alpha[cell] = if use_alpha { f32::from(bytes[i + 3]) / 255f32 } else { 1f32 };
        }
    }

    IconSignature::new(rgb, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_is_split_into_rgb_and_alpha_planes() {
        let n = IconSignature::SIZE;
        let stride = n * 4 + 8;
        let mut bytes = vec![0u8; stride * n];
        bytes[0..4].copy_from_slice(&[10, 20, 30, 51]);
        let last = (n - 1) * stride + (n - 1) * 4;
        bytes[last..last + 4].copy_from_slice(&[255, 0, 0, 255]);

        let signature = signature_from_argb(&bytes, stride, true);

        assert_eq!(&signature.rgb()[0..3], &[30f32 / 255f32, 20f32 / 255f32, 10f32 / 255f32]);
        assert_eq!(signature.alpha()[0], 0.2);
        let cell = n * n - 1;
        assert_eq!(&signature.rgb()[cell * 3..cell * 3 + 3], &[0.0, 0.0, 1.0]);
    }

    #[test]
    fn alpha_is_opaque_when_not_used() {
        let n = IconSignature::SIZE;
        let signature = signature_from_argb(&vec![0u8; n * n * 4], n * 4, false);

        assert!(signature.alpha().iter().all(|&a| a == 1.0));
    }
}
