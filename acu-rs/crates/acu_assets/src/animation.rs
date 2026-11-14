//! Bounded navigation animation subset, ACU-RE-0018. Event bytes remain opaque.
use crate::{Result, invalid, reader::Reader};
use serde::Serialize;
use std::f32::consts::FRAC_1_SQRT_2;

#[derive(Clone, Debug, Serialize)]
pub struct Track {
    pub hash: u32,
    pub codec: u8,
    pub offset: usize,
    pub times: Vec<f32>,
    /// Quaternion xyzw, vector xyz0, or scalar x000. Non-transform channels retained.
    pub values: Vec<[f32; 4]>,
    pub encoded_values: Vec<u8>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Clip {
    pub duration: f32,
    pub key: u32,
    pub mapping_offset: usize,
    pub tracks: Vec<Track>,
}
fn float(r: &mut Reader<'_>) -> Result<f32> {
    let f = f32::from_bits(r.u32()?);
    if !f.is_finite() {
        return invalid("nonfinite animation scalar");
    }
    Ok(f)
}
fn quaternion(v: [f32; 3], omitted: usize) -> Result<[f32; 4]> {
    let norm: f32 = v.iter().map(|x| x * x).sum();
    if norm > 1.001 {
        return invalid("invalid packed quaternion");
    }
    let mut q = [0.0; 4];
    let mut j = 0;
    for (i, x) in q.iter_mut().enumerate() {
        if i == omitted {
            *x = (1.0 - norm).max(0.0).sqrt();
        } else {
            *x = v[j];
            j += 1;
        }
    }
    Ok(q)
}
fn value(r: &mut Reader<'_>, codec: u8) -> Result<[f32; 4]> {
    let mut v = [0.0; 4];
    match codec {
        0 => {
            for x in &mut v {
                *x = float(r)?;
            }
            let norm: f32 = v.iter().map(|x| x * x).sum();
            if !(0.99..=1.01).contains(&norm) {
                return invalid("invalid uncompressed quaternion");
            }
        }
        2 | 4 => {
            let bits = if codec == 2 {
                [r.u8()? as u16, r.u8()? as u16, r.u8()? as u16]
            } else {
                [r.u16()?, r.u16()?, r.u16()?]
            };
            let (mask, sign, scale, bias) = if codec == 2 {
                (
                    127,
                    128,
                    std::f32::consts::SQRT_2 / 126.0,
                    std::f32::consts::FRAC_1_SQRT_2,
                )
            } else {
                (
                    32767,
                    32768,
                    f32::from_bits(942999495),
                    f32::from_bits(1060439283),
                )
            };
            let index = usize::from(bits[0] & sign != 0) + 2 * usize::from(bits[1] & sign != 0);
            return quaternion(bits.map(|x| (x & mask) as f32 * scale - bias), index);
        }
        // Codecs 3, 7 and 8: ACU-RE-0061. Key decoders 0x140087B60 (3), 0x140087830 (7),
        // 0x140087ED0 (8), reached through the per-type table at 0x145137A40.
        3 => {
            let p = r.u32()?;
            let scale = f32::from_bits(0x3AB5_5FA3); // sqrt(2) / 1022
            let a = [20, 10, 0].map(|s| ((p >> s) & 1023) as f32 * scale - FRAC_1_SQRT_2);
            return quaternion(a, (p >> 30) as usize);
        }
        7 => {
            let raw = r.u16()?;
            let z = f32::from(raw & 0x7fff) * f32::from_bits(0x3835_07C7) - FRAC_1_SQRT_2; // sqrt(2) / 32766
            let w = (1.0 - z * z).max(0.0).sqrt();
            return Ok(if raw & 0x8000 == 0 {
                [0.0, 0.0, z, w]
            } else {
                [0.0, 0.0, w, z]
            });
        }
        8 => {
            // Three stored components; the low mantissa bits of the first two select the
            // omitted one, and the components are used as stored (flag bits included).
            let a = [float(r)?, float(r)?, float(r)?];
            let index = (a[0].to_bits() & 1) as usize + 2 * (a[1].to_bits() & 1) as usize;
            return quaternion(a, index);
        }
        5 => {
            let packed = (u64::from(r.u32()?) << 32) | u64::from(r.u32()?);
            let scale = std::f64::consts::SQRT_2 / 1048574.0;
            let a = [40, 20, 0].map(|s| {
                (((packed >> s) & 0xfffff) as f64 * scale - std::f64::consts::FRAC_1_SQRT_2) as f32
            });
            return quaternion(a, (packed >> 62) as usize);
        }
        6 => {
            let b = r.u8()?;
            let z = f32::from(b & 127) * std::f32::consts::SQRT_2 / 126.0
                - std::f32::consts::FRAC_1_SQRT_2;
            let w = (1.0 - z * z).max(0.0).sqrt();
            return Ok(if b & 128 == 0 {
                [0.0, 0.0, z, w]
            } else {
                [0.0, 0.0, w, z]
            });
        }
        9 => {
            for x in &mut v[..3] {
                *x = float(r)?;
            }
        }
        11 => {
            for x in &mut v[..3] {
                // Millimetres: the type-33 sampler 0x140065450 multiplies each i16 by 0.001
                // (ACU-RE-0061; the earlier /10000 was wrong).
                *x = f32::from(r.i16()?) * 0.001;
            }
        }
        12 => v[0] = float(r)?,
        13 => v[0] = f32::from(r.u8()? as i8) * 0.008,
        14 => v[0] = f32::from(r.i16()?) * 0.008,
        15..=17 => v[0] = f32::from(r.u8()?),
        18 | 19 => {
            let _ = r.u32()?;
        } // opaque integer events, never used as transforms
        _ => return invalid(format!("unsupported animation codec {codec}")),
    }
    Ok(v)
}

/// Unique, fully validated mapping object; no interpretation of opaque event objects.
pub fn parse(bytes: &[u8]) -> Result<Clip> {
    parse_tracks(bytes, false)
}

/// Decode only the two root channels (track 0 translation, track 1 rotation), which come first.
/// Later bone tracks may use codecs this decoder does not support (7, 8, 3); they are not read, so
/// the trailing-bytes check is skipped. ACU-RE-0058 uses this for root motion.
pub fn parse_root(bytes: &[u8]) -> Result<Clip> {
    parse_tracks(bytes, true)
}

fn parse_tracks(bytes: &[u8], root_only: bool) -> Result<Clip> {
    if bytes.len() > 16 * 1024 * 1024 {
        return invalid("animation size limit");
    }
    let mut r = Reader::new(bytes);
    if r.u8()? != 1 {
        return invalid("animation object pointer");
    }
    r.u64()?;
    if r.u32()? != 0x0fa3067f {
        return invalid("animation class");
    }
    let duration = float(&mut r)?;
    if duration <= 0.0 || duration > 600.0 {
        return invalid("animation duration");
    }
    let key = r.u32()?;
    if r.u32()? != 0 || r.take(4)? != [0, 0, 0, 0] {
        return invalid("teleport/additive/streamed animation unsupported");
    }
    let candidates: Vec<_> = bytes
        .windows(8)
        .enumerate()
        .take(65536)
        .filter(|(_, b)| b[..4] == 0x0181efe8u32.to_le_bytes() && b[4..] == key.to_le_bytes())
        .map(|(i, _)| i)
        .collect();
    if candidates.len() != 1 {
        return invalid("missing/ambiguous inline animation mapping");
    }
    let mapping_offset = candidates[0];
    r.position = mapping_offset + 8;
    let n = r.u32()? as usize;
    if !(2..=4096).contains(&n) {
        return invalid("animation mapping count");
    }
    let mut hashes = Vec::with_capacity(n);
    for _ in 0..n {
        if r.u64()? != 0 || r.u32()? != 0x653caa76 {
            return invalid("animation mapping object");
        }
        hashes.push(r.u32()?);
    }
    if r.i16()? != 0 {
        return invalid("external animation references unsupported");
    }
    let _category = r.i32()?;
    if r.u16()? as usize != n || r.u16()? != 0 {
        return invalid("animation track count mismatch");
    }
    let mut tracks = Vec::with_capacity(n);
    let mut total_keys = 0usize;
    let read = if root_only { 2 } else { n };
    for hash in hashes.into_iter().take(read) {
        let offset = r.position;
        let mut t = r.u8()?;
        if t == 255 {
            t = r.u8()?;
        }
        if t >= 254 {
            return invalid("external/builtin animation track unsupported");
        }
        let size = r.u32()? as usize;
        let count = r.u32()? as usize;
        total_keys = total_keys
            .checked_add(count)
            .ok_or("animation key overflow")?;
        if count == 0 || count > 36001 || total_keys > 1_000_000 {
            return invalid("animation key limit");
        }
        let data_start = r.position;
        let mut times = Vec::with_capacity(count);
        match t % 3 {
            0 | 1 => {
                times.push(0.0);
                for _ in 1..count {
                    times.push(
                        if t % 3 == 0 {
                            f32::from(r.u8()?)
                        } else {
                            f32::from(r.u16()?)
                        } / 60.0,
                    );
                }
            }
            _ => {
                let prefix = r.u8()?;
                let bitmap = r.take((prefix & 63) as usize)?;
                for (i, b) in bitmap.iter().enumerate() {
                    for bit in 0..8 {
                        if b & (1 << bit) != 0 {
                            times.push(((i * 8 + bit) << (prefix >> 6)) as f32 / 60.0);
                        }
                    }
                }
                if times.len() != count {
                    return invalid("animation bitmap key count");
                }
            }
        }
        if times
            .windows(2)
            // Codec 13 scalars (sampler 0x140063E60) repeat a key time in shipped clips: a step.
            .any(|w| w[0] > w[1] || (t / 3 <= 14 && t / 3 != 13 && w[0] == w[1]))
            || times.last().copied().unwrap_or(0.0) > duration + 1.0 / 60.0
        {
            return invalid("animation key timing");
        }
        let codec = t / 3;
        let values_start = r.position;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(value(&mut r, codec)?);
        }
        // Serialized allocation size includes padding, unlike tightly stored bytes.
        if size < r.position - data_start || size > r.position - data_start + 32 {
            return invalid("animation allocation size");
        }
        tracks.push(Track {
            hash,
            codec,
            offset,
            times,
            values,
            encoded_values: bytes[values_start..r.position].to_vec(),
        });
    }
    if !root_only && r.remaining() != 0 {
        return invalid("animation trailing bytes");
    }
    if tracks[0].hash != 0 || tracks[0].codec != 9 || tracks[1].hash != 0 || tracks[1].codec > 8 {
        return invalid("unsupported animation root channels");
    }
    Ok(Clip {
        duration,
        key,
        mapping_offset,
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![1];
        b.extend(1u64.to_le_bytes());
        b.extend(0x0fa3067fu32.to_le_bytes());
        b.extend(1.0f32.to_le_bytes());
        b.extend(42u32.to_le_bytes());
        b.extend([0u8; 8]);
        b.extend(0x0181efe8u32.to_le_bytes());
        b.extend(42u32.to_le_bytes());
        b.extend(2u32.to_le_bytes());
        for _ in 0..2 {
            b.extend(0u64.to_le_bytes());
            b.extend(0x653caa76u32.to_le_bytes());
            b.extend(0u32.to_le_bytes());
        }
        b.extend(0i16.to_le_bytes());
        b.extend(60i32.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b.push(27);
        b.extend(26u32.to_le_bytes());
        b.extend(2u32.to_le_bytes());
        b.push(60);
        for f in [0.0f32, 0.0, 0.0, 0.0, 2.0, 0.0] {
            b.extend(f.to_le_bytes());
        }
        b.push(0);
        b.extend(16u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        for f in [0.0f32, 0.0, 0.0, 1.0] {
            b.extend(f.to_le_bytes());
        }
        b
    }
    #[test]
    fn independent_fixture_and_malformed_boundaries() {
        let b = fixture();
        let c = parse(&b).unwrap();
        assert_eq!(c.tracks[0].times, vec![0.0, 1.0]);
        assert_eq!(c.tracks[0].values[1], [0.0, 2.0, 0.0, 0.0]);
        for end in 0..b.len() {
            assert!(parse(&b[..end]).is_err(), "prefix {end}");
        }
        for offset in [0, 9, 21, 25, 29, 37, 49, 83] {
            let mut bad = b.clone();
            bad[offset] = 255;
            assert!(parse(&bad).is_err(), "mutation {offset}");
        }
        let mut bad = b.clone();
        bad[13..17].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse(&bad).is_err());
        let mut bad = b.clone();
        bad.extend(0x0181efe8u32.to_le_bytes());
        bad.extend(42u32.to_le_bytes());
        assert!(parse(&bad).is_err());
    }
    #[test]
    fn root_only_parse_ignores_unsupported_bone_tracks() {
        // The fixture's two root tracks plus a third track in codec 10 (type 30), which the full
        // parser rejects. Header fields that count tracks are patched from 2 to 3.
        let mut b = fixture();
        let mapping = b
            .windows(4)
            .position(|w| w == 0x0181efe8u32.to_le_bytes())
            .unwrap();
        b[mapping + 8..mapping + 12].copy_from_slice(&3u32.to_le_bytes());
        let entries_end = mapping + 12 + 2 * 16;
        let mut entry = 0u64.to_le_bytes().to_vec();
        entry.extend(0x653caa76u32.to_le_bytes());
        entry.extend(0x1234_5678u32.to_le_bytes());
        b.splice(entries_end..entries_end, entry);
        let count_at = entries_end + 16 + 2 + 4;
        b[count_at..count_at + 2].copy_from_slice(&3u16.to_le_bytes());
        b.push(30);
        b.extend(8u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend([0xAA; 8]);
        assert!(parse(&b).is_err());
        let root = parse_root(&b).unwrap();
        let full = parse(&fixture()).unwrap();
        assert_eq!(root.tracks.len(), 2);
        assert_eq!(root.tracks[0].values, full.tracks[0].values);
        assert_eq!(root.tracks[1].values, full.tracks[1].values);
        assert!(parse_root(&b[..b.len() - 30]).is_err());
    }
    #[test]
    fn known_quaternion_axes_and_invalid_sphere() {
        let q = value(&mut Reader::new(&[0xff, 0x3f, 0xff, 0xbf, 0xff, 0x3f]), 4).unwrap();
        assert!(q[2] > 0.9999);
        assert!(q[0].abs() < 0.0001);
        let q = value(
            &mut Reader::new(&[0x7f, 0xff, 0xff, 0xc7, 0xff, 0xff, 0xf7, 0xff]),
            5,
        )
        .unwrap();
        assert!(q[3] > 0.9999);
        assert!(q[0].abs() < 0.00001);
        assert!(quaternion([0.8; 3], 3).is_err());
    }
    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }
    #[test]
    fn codec_7_single_axis_halves() {
        // 0x3FFF is the midpoint of 15 bits: z = 0.5 * sqrt(2) - 1/sqrt(2) = 0, so w = 1.
        let v = |raw: u16| value(&mut Reader::new(&raw.to_le_bytes()), 7).unwrap();
        assert!(close(v(0x3FFF), [0.0, 0.0, 0.0, 1.0]));
        // The sign bit swaps the stored component into w: a half turn about z.
        assert!(close(v(0xBFFF), [0.0, 0.0, 1.0, 0.0]));
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert!(close(v(0x7FFE), [0.0, 0.0, h, h]));
        assert!(close(v(0x0000), [0.0, 0.0, -h, h]));
        assert!(value(&mut Reader::new(&[0xFF]), 7).is_err());
    }
    #[test]
    fn codec_3_ten_bit_smallest_three() {
        // Each 10-bit field at 511 decodes to 0; the top two bits name the omitted component.
        let mid = (511 << 20) | (511 << 10) | 511u32;
        let v = |p: u32| value(&mut Reader::new(&p.to_le_bytes()), 3).unwrap();
        assert!(close(v(mid), [1.0, 0.0, 0.0, 0.0]));
        assert!(close(v(mid | 0x4000_0000), [0.0, 1.0, 0.0, 0.0]));
        assert!(close(v(mid | 0x8000_0000), [0.0, 0.0, 1.0, 0.0]));
        assert!(close(v(mid | 0xC000_0000), [0.0, 0.0, 0.0, 1.0]));
        // Field values 1022 / 1022 / 511 put 0.7071 in two components: norm 1, omitted = 0.
        let q = v((1022 << 20) | (1022 << 10) | 511 | 0xC000_0000);
        assert!(q[3].abs() < 1e-3);
        assert!(value(&mut Reader::new(&[0; 3]), 3).is_err());
    }
    #[test]
    fn codec_8_flag_bits_select_omitted_component() {
        let enc = |a: [f32; 3]| a.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let flag = f32::from_bits(1); // only the flag bit set
        let v = |a| value(&mut Reader::new(&enc(a)), 8).unwrap();
        assert!(close(v([0.0, 0.0, 0.6]), [0.8, 0.0, 0.0, 0.6]));
        assert!(close(v([flag, 0.0, 0.6]), [0.0, 0.8, 0.0, 0.6]));
        assert!(close(v([0.0, flag, 0.6]), [0.0, 0.0, 0.8, 0.6]));
        assert!(close(v([flag, flag, 0.6]), [0.0, 0.0, 0.6, 0.8]));
        assert!(value(&mut Reader::new(&enc([0.9, 0.9, 0.0])), 8).is_err());
    }
    #[test]
    #[ignore = "requires original ACU install"]
    fn installed_navigation_tracks() -> Result<()> {
        use crate::{Limits, forge::Forge, package};
        let dir = std::env::var("ACU_GAME_DIR")?;
        let mut f = Forge::open(
            std::path::Path::new(&dir).join("DataPC.forge"),
            Limits::default(),
        )?;
        let p = package::decode(&f.raw(928)?, Limits::default())?;
        for (index, keys, duration) in [
            (4190, 79, 2.8),
            (3106, 81, 1.4),
            (6339, 101, 0.6),
            (4898, 62, 1.0),
        ] {
            let b = &p.bytes[p.resources[index].payload.clone()];
            let c = parse(b)?;
            assert_eq!(c.tracks.len(), keys);
            assert!((c.duration - duration).abs() < 0.00001);
            assert_eq!(c.tracks[0].values[0], [0.0; 4]);
            if index == 4190 {
                assert!((c.tracks[0].values.last().unwrap()[1] - 6.899_789).abs() < 1e-5);
            }
            if index == 3106 {
                assert!((c.tracks[0].values.last().unwrap()[1] - 9.896048).abs() < 1e-5);
            }
            for end in [0, 13, c.mapping_offset, c.mapping_offset + 20, b.len() - 1] {
                assert!(parse(&b[..end]).is_err());
            }
            let mut bad = b.to_vec();
            bad.push(0);
            assert!(parse(&bad).is_err());
        }
        Ok(())
    }
}
