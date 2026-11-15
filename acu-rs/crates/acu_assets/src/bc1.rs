//! CPU BC1 (DXT1) decode of one mip level to RGBA8, for material baking.
//! Standard block layout: two RGB565 endpoints and 2-bit indices. When
//! `c0 <= c1` the block uses three colours plus transparent black. Rounding of
//! the interpolated colours can differ from a given GPU by one unit.

use crate::{Result, invalid};

fn expand(c: u16) -> [u16; 3] {
    let r = (c >> 11) & 31;
    let g = (c >> 5) & 63;
    let b = c & 31;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// Bytes needed for one `width` x `height` BC1 level.
pub fn level_len(width: u32, height: u32) -> usize {
    width.div_ceil(4) as usize * height.div_ceil(4) as usize * 8
}

/// Decode the first level stored at the start of `data`.
pub fn decode(width: u32, height: u32, data: &[u8]) -> Result<Vec<[u8; 4]>> {
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return invalid("bc1: unsupported dimensions");
    }
    let need = level_len(width, height);
    if data.len() < need {
        return invalid(format!(
            "bc1: {} bytes for a {width}x{height} level needing {need}",
            data.len()
        ));
    }
    let (w, h) = (width as usize, height as usize);
    let mut out = vec![[0u8; 4]; w * h];
    let (blocks, _) = data[..need].as_chunks::<8>();
    for (block, chunk) in blocks.iter().enumerate() {
        let bx = block % w.div_ceil(4);
        let by = block / w.div_ceil(4);
        let c0 = u16::from_le_bytes([chunk[0], chunk[1]]);
        let c1 = u16::from_le_bytes([chunk[2], chunk[3]]);
        let (a, b) = (expand(c0), expand(c1));
        let mix = |p: u16, q: u16, wp: u16, wq: u16, d: u16| ((p * wp + q * wq + d / 2) / d) as u8;
        let mut palette = [[0u8; 4]; 4];
        palette[0] = [a[0] as u8, a[1] as u8, a[2] as u8, 255];
        palette[1] = [b[0] as u8, b[1] as u8, b[2] as u8, 255];
        if c0 > c1 {
            palette[2] = [
                mix(a[0], b[0], 2, 1, 3),
                mix(a[1], b[1], 2, 1, 3),
                mix(a[2], b[2], 2, 1, 3),
                255,
            ];
            palette[3] = [
                mix(a[0], b[0], 1, 2, 3),
                mix(a[1], b[1], 1, 2, 3),
                mix(a[2], b[2], 1, 2, 3),
                255,
            ];
        } else {
            palette[2] = [
                mix(a[0], b[0], 1, 1, 2),
                mix(a[1], b[1], 1, 1, 2),
                mix(a[2], b[2], 1, 1, 2),
                255,
            ];
            palette[3] = [0, 0, 0, 0];
        }
        let bits = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        for i in 0..16 {
            let (x, y) = (bx * 4 + i % 4, by * 4 + i / 4);
            if x < w && y < h {
                out[y * w + x] = palette[((bits >> (2 * i)) & 3) as usize];
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_colour_block_interpolates_endpoints() {
        // c0 = pure red (0xf800), c1 = pure blue (0x001f); indices 0,1,2,3 on row 0.
        let block = [0x00, 0xf8, 0x1f, 0x00, 0b1110_0100, 0, 0, 0];
        let px = decode(4, 4, &block).unwrap();
        assert_eq!(px[0], [255, 0, 0, 255]);
        assert_eq!(px[1], [0, 0, 255, 255]);
        assert_eq!(px[2], [170, 0, 85, 255]);
        assert_eq!(px[3], [85, 0, 170, 255]);
        assert_eq!(px[4], [255, 0, 0, 255]); // index 0 elsewhere
    }

    #[test]
    fn three_colour_block_has_transparent_black() {
        let block = [0x1f, 0x00, 0x00, 0xf8, 0b1110_0100, 0, 0, 0];
        let px = decode(4, 4, &block).unwrap();
        assert_eq!(px[2], [128, 0, 128, 255]);
        assert_eq!(px[3], [0, 0, 0, 0]);
    }

    #[test]
    fn partial_blocks_and_short_input() {
        assert_eq!(decode(2, 2, &[0; 8]).unwrap().len(), 4);
        assert!(decode(8, 8, &[0; 31]).is_err());
        assert!(decode(0, 4, &[0; 8]).is_err());
    }
}
