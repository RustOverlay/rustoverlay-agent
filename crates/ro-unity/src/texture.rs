//! Texture2D fields needed to locate and decode streamed pixel data.

use std::io;

use crate::RgbaImage;
use crate::bc3;
use crate::cursor::invalid;
use crate::type_tree::Value;

const DXT5: i32 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextureInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: i32,
    /// Offset into the archive node named by `stream_path`'s file name.
    pub(crate) stream_offset: u64,
    pub(crate) stream_size: u32,
    pub(crate) stream_path: String,
}

impl TextureInfo {
    pub(crate) fn from_value(v: &Value) -> io::Result<Self> {
        let dim =
            |name| u32::try_from(v.field(name)?.int()?).map_err(|_| invalid(format!("bad {name}")));
        let stream = v.field("m_StreamData")?;
        Ok(Self {
            width: dim("m_Width")?,
            height: dim("m_Height")?,
            format: i32::try_from(v.field("m_TextureFormat")?.int()?)
                .map_err(|_| invalid("bad format"))?,
            stream_offset: u64::try_from(stream.field("offset")?.int()?)
                .map_err(|_| invalid("bad offset"))?,
            stream_size: u32::try_from(stream.field("size")?.int()?)
                .map_err(|_| invalid("bad size"))?,
            stream_path: stream.field("path")?.str()?.to_owned(),
        })
    }

    /// File name of the archive node holding the stream (`archive:/CAB-x/CAB-x.resS` -> `CAB-x.resS`).
    pub(crate) fn stream_node_name(&self) -> &str {
        self.stream_path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.stream_path)
    }
}

/// Bytes of mip 0 for a supported format.
pub(crate) fn mip0_len(format: i32, width: u32, height: u32) -> io::Result<usize> {
    match format {
        DXT5 => Ok(bc3::mip0_len(width, height)),
        f => Err(unsupported(f)),
    }
}

/// Decodes mip 0 and flips it top row first (Unity stores the bottom row first).
pub(crate) fn decode(format: i32, data: &[u8], width: u32, height: u32) -> io::Result<RgbaImage> {
    let mut pixels = match format {
        DXT5 => bc3::decode(data, width, height)?,
        f => return Err(unsupported(f)),
    };
    let w = width as usize;
    let rows = pixels.len() / w.max(1);
    for y in 0..rows / 2 {
        let (top, bottom) = pixels.split_at_mut((rows - 1 - y) * w);
        top[y * w..(y + 1) * w].swap_with_slice(&mut bottom[..w]);
    }
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

fn unsupported(format: i32) -> io::Error {
    let name = match format {
        1 => "Alpha8",
        3 => "RGB24",
        4 => "RGBA32",
        5 => "ARGB32",
        10 => "DXT1",
        12 => "DXT5",
        24 => "BC6H",
        25 => "BC7",
        28 => "DXT1Crunched",
        29 => "DXT5Crunched",
        _ => "unknown",
    };
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("unsupported texture format {name} ({format})"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_rows_and_names_unsupported_formats() {
        // 4x8 BC3: first stored block (the image's bottom) opaque white, second transparent black.
        let mut data = vec![0u8; bc3::mip0_len(4, 8)];
        data[0] = 255;
        data[8..10].copy_from_slice(&0xffffu16.to_le_bytes());
        data[10..12].copy_from_slice(&0xffffu16.to_le_bytes());
        let img = decode(DXT5, &data, 4, 8).unwrap();
        assert_eq!(img.pixels[0], [0, 0, 0, 0]);
        assert_eq!(img.pixels[4 * 7], [255, 255, 255, 255]);
        let err = decode(25, &data, 4, 8).unwrap_err();
        assert!(err.to_string().contains("BC7"), "{err}");
        assert!(mip0_len(4, 4, 4).is_err());
    }

    #[test]
    fn node_name_from_stream_path() {
        let t = TextureInfo {
            width: 1,
            height: 1,
            format: DXT5,
            stream_offset: 0,
            stream_size: 0,
            stream_path: "archive:/CAB-ab/CAB-ab.resS".into(),
        };
        assert_eq!(t.stream_node_name(), "CAB-ab.resS");
    }
}
