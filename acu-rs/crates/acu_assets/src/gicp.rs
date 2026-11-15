//! GICP octree-delta volumes and the `GIDecodeYCbCr` combination (ACU-RE-0073).
//!
//! Observed behaviour (ACU.exe 748abdc6...7e40, base 0x140000000; corpus pseudocode), re-derived
//! and implemented independently:
//! - `sub_1421C4040` checks the `GICP` magic (`dword_144FB3C5C`), reads the cube edge at `+8`, the
//!   per-channel offset bytes at `+20`, the per-channel root samples at `+24`, the literal count at
//!   `+28`, the 12-byte literals at `+32` (u32 node number, 8 corner bytes), then a u32 bitstream
//!   byte count followed by the bitstream.
//! - `sub_1421E6220` walks an octree in pre-order. Every node gets the next node number; a node
//!   whose number matches the next literal writes the literal's 8 bytes to its corners, otherwise it
//!   reads `w` (3 bits), then the minimum-corner index (3 bits, only when `w > 0`) and 7 values of
//!   `w` bits, inserts a zero at the minimum index, and sets corners 1..7 to
//!   `corner0 - d[0] + d[k]` (wrapping bytes). Nodes larger than 2 recurse into 8 children.
//! - `sub_1421E6070` then adds `offset + 0x80` to slice z = 0 and `previous slice + 0x80` to every
//!   later slice (DPCM along z). `sub_1421E61B0` interleaves channels into RGBA bytes.
//! - The YCbCr path (`GIData +106` set, `sub_1421C4310`) decodes each of the four blobs as one
//!   channel; `sub_1421C5890` routes GIData `k` blob `j` to texture `4k + j`, which
//!   `GIDecodeYCbCr` (cs_5_0 at 0x1433200B0) combines; see [`combine_ycbcr`].
//!
//! Observed in the Paris Noon datablock: 192 of 192 blobs consume exactly the stored bitstream
//! byte count and every literal, followed by 8 bytes of padding.

use crate::{Result, invalid};

pub const MAGIC: &[u8; 4] = b"GICP";
const HEADER: usize = 32;
const LITERAL: usize = 12;
/// Trailing bytes after the bitstream in every observed blob (the native reader fetches 8 bytes at
/// a time). Other paddings are rejected as unsupported.
const PADDING: usize = 8;
const MAX_DIM: u32 = 64;
const CORNERS: [[u32; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

/// One decoded GICP blob: `channels` cubes of `dim`^3 bytes, x fastest, then y, then z.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub dim: u32,
    pub channels: u32,
    pub values: Vec<u8>,
    pub literals: usize,
    pub bitstream_bits: usize,
}

impl Volume {
    pub fn get(&self, channel: u32, x: u32, y: u32, z: u32) -> u8 {
        let d = self.dim as usize;
        self.values[x as usize + d * (y as usize + d * (z as usize + d * channel as usize))]
    }
}

struct Bits<'a> {
    data: &'a [u8],
    position: usize,
}

impl Bits<'_> {
    fn get(&mut self, n: u32) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if self.position + n as usize > self.data.len() * 8 {
            return invalid("GICP bitstream overrun");
        }
        let mut v = 0;
        for _ in 0..n {
            let byte = self.data[self.position >> 3];
            v = (v << 1) | u32::from((byte >> (7 - (self.position & 7))) & 1);
            self.position += 1;
        }
        Ok(v)
    }
}

struct Decoder<'a> {
    dim: u32,
    values: Vec<u8>,
    literals: Vec<(u32, [u8; 8])>,
    next_literal: usize,
    node: u32,
    bits: Bits<'a>,
}

impl Decoder<'_> {
    fn index(&self, c: u32, x: u32, y: u32, z: u32) -> usize {
        let d = self.dim as usize;
        x as usize + d * (y as usize + d * (z as usize + d * c as usize))
    }

    fn node(&mut self, c: u32, size: u32, o: [u32; 3]) -> Result<()> {
        self.node += 1;
        let h = size / 2;
        let at: [usize; 8] =
            CORNERS.map(|k| self.index(c, o[0] + k[0] * h, o[1] + k[1] * h, o[2] + k[2] * h));
        match self.literals.get(self.next_literal) {
            Some(&(number, bytes)) if number == self.node => {
                self.next_literal += 1;
                for (i, &b) in at.iter().zip(bytes.iter()) {
                    self.values[*i] = b;
                }
            }
            _ => {
                let width = self.bits.get(3)?;
                let minimum = if width > 0 {
                    self.bits.get(3)? as usize
                } else {
                    0
                };
                let mut read = [0u8; 7];
                for v in &mut read {
                    *v = self.bits.get(width)? as u8;
                }
                let mut d = [0u8; 8];
                d[..minimum].copy_from_slice(&read[..minimum]);
                d[minimum + 1..].copy_from_slice(&read[minimum..]);
                let base = self.values[at[0]].wrapping_sub(d[0]);
                for k in 1..8 {
                    self.values[at[k]] = base.wrapping_add(d[k]);
                }
            }
        }
        if size > 2 {
            for k in CORNERS {
                self.node(c, h, [o[0] + k[0] * h, o[1] + k[1] * h, o[2] + k[2] * h])?;
            }
        }
        Ok(())
    }
}

fn u32_at(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes(s.try_into().expect("4 bytes")))
        .ok_or_else(|| "GICP header truncated".into())
}

/// Decode one blob as `channels` (1..=4) interleaved-in-stream cubes. Fails on any trailing or
/// missing byte, an unconsumed literal, or an unobserved header shape.
pub fn decode(blob: &[u8], channels: u32) -> Result<Volume> {
    if !(1..=4).contains(&channels) {
        return invalid(format!("GICP channel count {channels} unsupported"));
    }
    if blob.len() < HEADER || &blob[..4] != MAGIC {
        return invalid("not a GICP blob");
    }
    let dim = u32_at(blob, 8)?;
    if [u32_at(blob, 12)?, u32_at(blob, 16)?] != [dim, dim] {
        return invalid("GICP non-cubic volume unsupported");
    }
    if !dim.is_power_of_two() || !(2..=MAX_DIM).contains(&dim) {
        return invalid(format!("GICP edge {dim} unsupported"));
    }
    let count = u32_at(blob, 28)? as usize;
    let table_end = count
        .checked_mul(LITERAL)
        .and_then(|n| n.checked_add(HEADER))
        .filter(|&e| e + 4 <= blob.len())
        .ok_or("GICP literal table overruns the blob")?;
    let mut literals = Vec::with_capacity(count);
    for i in 0..count {
        let o = HEADER + i * LITERAL;
        let number = u32_at(blob, o)?;
        if literals.last().is_some_and(|&(n, _)| n >= number) {
            return invalid("GICP literal node numbers not increasing");
        }
        literals.push((number, blob[o + 4..o + 12].try_into().expect("8 bytes")));
    }
    let stream_bytes = u32_at(blob, table_end)? as usize;
    let stream_start = table_end + 4;
    if blob.len().checked_sub(stream_start) != stream_bytes.checked_add(PADDING) {
        return invalid(format!(
            "GICP length {} != header + {stream_bytes} stream bytes + {PADDING} padding",
            blob.len()
        ));
    }
    let cube = (dim as usize).pow(3);
    let mut d = Decoder {
        dim,
        values: vec![0; cube * channels as usize],
        literals,
        next_literal: 0,
        node: 0,
        bits: Bits {
            data: &blob[stream_start..stream_start + stream_bytes],
            position: 0,
        },
    };
    for c in 0..channels {
        let root = d.index(c, 0, 0, 0);
        d.values[root] = blob[24 + c as usize];
        d.node(c, dim, [0, 0, 0])?;
    }
    if d.next_literal != d.literals.len() {
        return invalid("GICP literals left unused");
    }
    if d.bits.position.div_ceil(8) != stream_bytes {
        return invalid(format!(
            "GICP bitstream ends at bit {} of {stream_bytes} bytes",
            d.bits.position
        ));
    }
    let slice = (dim as usize).pow(2);
    for c in 0..channels as usize {
        let base = c * cube;
        let add = blob[20 + c].wrapping_add(0x80);
        for v in &mut d.values[base..base + slice] {
            *v = v.wrapping_add(add);
        }
        for i in base + slice..base + cube {
            d.values[i] = d.values[i].wrapping_add(d.values[i - slice].wrapping_add(0x80));
        }
    }
    Ok(Volume {
        dim,
        channels,
        values: d.values,
        literals: count,
        bitstream_bits: d.bits.position,
    })
}

/// Trilinear sample of a single-channel cube as UNORM, at the texel position the `GIDecode*`
/// shaders use for output voxel `i` of `n`: `0.5/s + i/(n-1) * (1 - 1/s)` per axis. That the
/// shader's `cb5[0]` is `1/(n-1)` is INFERRED (it is a constant buffer value, not observed).
fn sample(v: &Volume, i: [u32; 3], n: u32) -> f32 {
    let s = v.dim as f32;
    let t = |k: u32| {
        let u = 0.5 / s + k as f32 / (n - 1).max(1) as f32 * (1.0 - 1.0 / s);
        (u * s - 0.5).clamp(0.0, s - 1.0)
    };
    let p = i.map(t);
    let lo = p.map(|x| x.floor() as u32);
    let f = [
        p[0] - lo[0] as f32,
        p[1] - lo[1] as f32,
        p[2] - lo[2] as f32,
    ];
    let hi = lo.map(|x| (x + 1).min(v.dim - 1));
    let mut acc = 0.0;
    for k in CORNERS {
        let w = (if k[0] == 1 { f[0] } else { 1.0 - f[0] })
            * (if k[1] == 1 { f[1] } else { 1.0 - f[1] })
            * (if k[2] == 1 { f[2] } else { 1.0 - f[2] });
        if w > 0.0 {
            let pick = |a: usize| if k[a] == 1 { hi[a] } else { lo[a] };
            acc += w * f32::from(v.get(0, pick(0), pick(1), pick(2))) / 255.0;
        }
    }
    acc
}

/// One output voxel of `GIDecodeYCbCr`: `rgb[c]` is the `u0` / `u1` / `u2` texel (R, G, B) with
/// four coefficients; `extra` is the `u3` texel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedTexel {
    pub rgb: [[f32; 4]; 3],
    pub extra: [f32; 4],
}

/// Combine four GIData (16 single-channel volumes) at output voxel `i` of an `n`^3 grid, as
/// `GIDecodeYCbCr` does (observed in its DXBC):
/// - GIData `k` gives `a = t0*S.x+B.x`, `b = t1*S.y+B.y`, `c = t2*S.z+B.z`, `w = t3^2.2*S.w+B.w`
///   with `S = scale[k]`, `B = bias[k]` (`cb5[3+k]`, `cb5[7+k]`);
/// - `m = |w0|`; coefficient j < 3 has (Y, Cb, Cr) = `m * (comp_j(GIData0..2))`, coefficient 3
///   has (`w0`, `m*w1`, `m*w2`); BT.601 rows convert to RGB;
/// - `extra = (a3, b3, c3) * |w3|, w3`.
///
/// That `scale` / `bias` are GIData `+0` / `+16` is INFERRED from `sub_1421C5890` copying them to
/// two consecutive 4 x vec4 blocks; the constant-buffer binding itself was not observed.
pub fn combine_ycbcr(
    volumes: &[[Volume; 4]; 4],
    scale: &[[f32; 4]; 4],
    bias: &[[f32; 4]; 4],
    i: [u32; 3],
    n: u32,
) -> DecodedTexel {
    let mut q = [[0f32; 4]; 4];
    for k in 0..4 {
        for j in 0..4 {
            let t = sample(&volumes[k][j], i, n);
            let t = if j == 3 { t.abs().powf(2.2) } else { t };
            q[k][j] = t * scale[k][j] + bias[k][j];
        }
    }
    let m = q[0][3].abs();
    let rgb_of = |y: f32, cb: f32, cr: f32| {
        [
            1.168_95 * y + 1.602_29 * cr,
            1.168_95 * y - 0.393_299 * cb - 0.816_156 * cr,
            1.168_95 * y + 2.025_14 * cb + 0.000_003 * cr,
        ]
    };
    let mut rgb = [[0f32; 4]; 3];
    for j in 0..4 {
        let (y, cb, cr) = if j < 3 {
            (m * q[0][j], m * q[1][j], m * q[2][j])
        } else {
            (q[0][3], m * q[1][3], m * q[2][3])
        };
        let c = rgb_of(y, cb, cr);
        for ch in 0..3 {
            rgb[ch][j] = c[ch];
        }
    }
    let e = q[3][3].abs();
    DecodedTexel {
        rgb,
        extra: [q[3][0] * e, q[3][1] * e, q[3][2] * e, q[3][3]],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Independent encoder for tests: every node except those in `literal_nodes` is coded with
    /// width `w` deltas from `samples` (already in residual space, before the z-DPCM).
    struct Writer {
        bytes: Vec<u8>,
        bit: usize,
    }
    impl Writer {
        fn put(&mut self, v: u32, n: u32) {
            for i in (0..n).rev() {
                if self.bit % 8 == 0 {
                    self.bytes.push(0);
                }
                if (v >> i) & 1 == 1 {
                    *self.bytes.last_mut().unwrap() |= 0x80 >> (self.bit % 8);
                }
                self.bit += 1;
            }
        }
    }

    /// A 2^3 blob: one node. `residual` are the eight corner residuals (corner 0 = root).
    fn blob2(residual: [u8; 8], offset: u8, literal: bool, padding: usize) -> Vec<u8> {
        let mut b = b"GICP".to_vec();
        b.extend_from_slice(&[1, 0, 0, 0]);
        for _ in 0..3 {
            b.extend_from_slice(&2u32.to_le_bytes());
        }
        b.extend_from_slice(&[offset, 0, 0, 0, residual[0], 0, 0, 0]);
        let mut w = Writer {
            bytes: vec![],
            bit: 0,
        };
        if literal {
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&residual);
        } else {
            b.extend_from_slice(&0u32.to_le_bytes());
            let min = (0..8).min_by_key(|&k| residual[k]).unwrap();
            let d: Vec<u8> = residual.iter().map(|&r| r - residual[min]).collect();
            let width = 8 - d.iter().max().unwrap().leading_zeros().min(8);
            w.put(width, 3);
            if width > 0 {
                w.put(min as u32, 3);
                for (k, &v) in d.iter().enumerate() {
                    if k != min {
                        w.put(u32::from(v), width);
                    }
                }
            }
        }
        b.extend_from_slice(&(w.bytes.len() as u32).to_le_bytes());
        b.extend_from_slice(&w.bytes);
        b.extend(std::iter::repeat_n(0, padding));
        b
    }

    /// Expected volume from residuals with the z-DPCM applied by hand.
    fn expect2(residual: [u8; 8], offset: u8) -> Vec<u8> {
        let mut v = residual.to_vec();
        for x in &mut v[..4] {
            *x = x.wrapping_add(offset).wrapping_add(0x80);
        }
        for i in 4..8 {
            v[i] = v[i].wrapping_add(v[i - 4]).wrapping_add(0x80);
        }
        v
    }

    #[test]
    fn delta_node_decodes_to_end() -> Result<()> {
        let r = [10, 13, 9, 12, 20, 11, 9, 15];
        let v = decode(&blob2(r, 5, false, 8), 1)?;
        assert_eq!(v.values, expect2(r, 5));
        assert_eq!(v.literals, 0);
        Ok(())
    }

    #[test]
    fn literal_node_decodes_to_end() -> Result<()> {
        let r = [200, 1, 2, 3, 4, 5, 6, 255];
        let v = decode(&blob2(r, 0, true, 8), 1)?;
        assert_eq!(v.values, expect2(r, 0));
        assert_eq!(v.literals, 1);
        Ok(())
    }

    #[test]
    fn flat_node_uses_three_bits() -> Result<()> {
        let v = decode(&blob2([7; 8], 0, false, 8), 1)?;
        assert_eq!(v.bitstream_bits, 3);
        assert_eq!(v.values, expect2([7; 8], 0));
        Ok(())
    }

    #[test]
    fn rejects_bad_inputs() {
        let good = blob2([10, 13, 9, 12, 20, 11, 9, 15], 5, false, 8);
        assert!(decode(&good, 0).is_err());
        assert!(decode(&good, 5).is_err());
        let mut magic = good.clone();
        magic[0] = b'X';
        assert!(decode(&magic, 1).is_err());
        assert!(decode(&good[..good.len() - 1], 1).is_err(), "short padding");
        assert!(
            decode(&blob2([1; 8], 0, false, 9), 1).is_err(),
            "extra trailing byte"
        );
        assert!(decode(&good[..20], 1).is_err(), "truncated header");
        let mut dims = good.clone();
        dims[12] = 4;
        assert!(decode(&dims, 1).is_err(), "non-cubic");
        let mut odd = good.clone();
        for o in [8, 12, 16] {
            odd[o] = 3;
        }
        assert!(decode(&odd, 1).is_err(), "edge not a power of two");
        let mut count = good.clone();
        count[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&count, 1).is_err(), "literal table overrun");
        // Stream byte count says more than is coded: the walk ends early.
        let mut long = blob2([1; 8], 0, false, 0);
        let at = long.len() - 1 - 4;
        long[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
        long.extend_from_slice(&[0; 9]);
        assert!(decode(&long, 1).is_err(), "unused stream bytes");
        // Unused literal (node 2 never exists in a 2^3 volume).
        let mut lit = blob2([1; 8], 0, true, 8);
        lit[28..32].copy_from_slice(&2u32.to_le_bytes());
        let tail = lit.split_off(44);
        lit.extend_from_slice(&2u32.to_le_bytes());
        lit.extend_from_slice(&[0; 8]);
        lit.extend_from_slice(&tail);
        assert!(decode(&lit, 1).is_err(), "unused literal");
    }

    #[test]
    fn ycbcr_combination_matches_hand_values() {
        // Constant volumes: every texel 255 (t = 1), so t^2.2 = 1.
        let full = || Volume {
            dim: 2,
            channels: 1,
            values: vec![255; 8],
            literals: 0,
            bitstream_bits: 0,
        };
        let vols: [[Volume; 4]; 4] = std::array::from_fn(|_| std::array::from_fn(|_| full()));
        let scale = [
            [0.5, 0.25, 0.125, 2.0],
            [0.0; 4],
            [0.0; 4],
            [1.0, 1.0, 1.0, -0.5],
        ];
        let bias = [[0.0; 4]; 4];
        let t = combine_ycbcr(&vols, &scale, &bias, [0, 0, 0], 2);
        // m = |w0| = 2; coefficient 0: Y = 2*0.5 = 1, Cb = Cr = 0 -> RGB = 1.16895.
        assert!((t.rgb[0][0] - 1.168_95).abs() < 1e-6);
        assert!((t.rgb[2][1] - 0.584_475).abs() < 1e-6);
        // Coefficient 3: Y = w0 = 2.
        assert!((t.rgb[1][3] - 2.337_9).abs() < 1e-5);
        assert_eq!(t.extra, [0.5, 0.5, 0.5, -0.5]);
    }
}
