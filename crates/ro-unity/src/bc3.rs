//! BC3 (DXT5) decoder, rounding as Pillow's BcnDecode does so output matches UnityPy exports.

use std::io;

use crate::cursor::invalid;

const BLOCK_BYTES: usize = 16;

pub(crate) fn mip0_len(width: u32, height: u32) -> usize {
    width.div_ceil(4) as usize * height.div_ceil(4) as usize * BLOCK_BYTES
}

/// Decodes mip 0 into RGBA rows in storage order.
pub(crate) fn decode(data: &[u8], width: u32, height: u32) -> io::Result<Vec<[u8; 4]>> {
    let need = mip0_len(width, height);
    if data.len() < need {
        return Err(invalid(format!(
            "BC3 {width}x{height} needs {need} bytes, got {}",
            data.len()
        )));
    }
    let (w, h) = (width as usize, height as usize);
    let blocks_x = w.div_ceil(4);
    let mut out = vec![[0u8; 4]; w * h];
    for (i, block) in data[..need].as_chunks::<BLOCK_BYTES>().0.iter().enumerate() {
        let (bx, by) = (i % blocks_x * 4, i / blocks_x * 4);
        let texels = decode_block(block);
        for (n, px) in texels.iter().enumerate() {
            let (x, y) = (bx + n % 4, by + n / 4);
            if x < w && y < h {
                out[y * w + x] = *px;
            }
        }
    }
    Ok(out)
}

fn decode_block(b: &[u8; 16]) -> [[u8; 4]; 16] {
    let alpha = alpha_palette(b[0], b[1]);
    let alpha_bits = u64::from_le_bytes([b[2], b[3], b[4], b[5], b[6], b[7], 0, 0]);
    let colors = color_palette(
        u16::from_le_bytes([b[8], b[9]]),
        u16::from_le_bytes([b[10], b[11]]),
    );
    let color_bits = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
    std::array::from_fn(|n| {
        let [r, g, b] = colors[(color_bits >> (2 * n) & 3) as usize];
        [r, g, b, alpha[(alpha_bits >> (3 * n) & 7) as usize]]
    })
}

fn alpha_palette(a0: u8, a1: u8) -> [u8; 8] {
    let (a0w, a1w) = (u16::from(a0), u16::from(a1));
    let mix = |w0: u16, w1: u16, d: u16| ((w0 * a0w + w1 * a1w) / d) as u8;
    if a0 > a1 {
        [
            a0,
            a1,
            mix(6, 1, 7),
            mix(5, 2, 7),
            mix(4, 3, 7),
            mix(3, 4, 7),
            mix(2, 5, 7),
            mix(1, 6, 7),
        ]
    } else {
        [
            a0,
            a1,
            mix(4, 1, 5),
            mix(3, 2, 5),
            mix(2, 3, 5),
            mix(1, 4, 5),
            0,
            255,
        ]
    }
}

/// BC3 colour blocks always use the four-colour mode, whatever the endpoint order.
fn color_palette(c0: u16, c1: u16) -> [[u8; 3]; 4] {
    let (p0, p1) = (rgb565(c0), rgb565(c1));
    let mix = |w0: u16, w1: u16| {
        std::array::from_fn(|k| ((w0 * u16::from(p0[k]) + w1 * u16::from(p1[k])) / 3) as u8)
    };
    [p0, p1, mix(2, 1), mix(1, 2)]
}

fn rgb565(v: u16) -> [u8; 3] {
    let r = ((v & 0xf800) >> 8) as u8;
    let g = ((v & 0x07e0) >> 3) as u8;
    let b = ((v & 0x001f) << 3) as u8;
    [r | r >> 5, g | g >> 6, b | b >> 5]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_block_and_palette_indices() {
        // Opaque white endpoints, alpha a0=255 a1=0 with texel 0 -> index 1 (0), texel 1 -> index 2.
        let mut blk = [0u8; 16];
        blk[0] = 255;
        blk[2] = 0b010_001;
        blk[8..10].copy_from_slice(&0xffffu16.to_le_bytes());
        blk[10..12].copy_from_slice(&0x0000u16.to_le_bytes());
        blk[12] = 0b10_01;
        let px = decode_block(&blk);
        assert_eq!(px[0], [0, 0, 0, 0]);
        assert_eq!(px[1], [170, 170, 170, (6 * 255 / 7) as u8]);
        assert_eq!(px[2], [255, 255, 255, 255]);
    }

    #[test]
    fn crops_partial_blocks_and_checks_length() {
        let data = vec![0xffu8; mip0_len(5, 3)];
        assert_eq!(decode(&data, 5, 3).unwrap().len(), 15);
        assert!(decode(&data[..16], 5, 3).is_err());
        assert_eq!(alpha_palette(0, 0)[6..], [0, 255]);
    }
}
