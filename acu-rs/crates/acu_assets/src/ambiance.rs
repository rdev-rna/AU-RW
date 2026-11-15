//! `WorldAmbiance` (`21c2d472`) partial decode (ACU-RE-0073).
//!
//! The native loader `0x1421D03D0` (runtime image `ACU.runtime-analysis.pe`, base
//! `0x140000000`) reads, in order: `LayeredSky` handle, embedded `PostEffects`, a byte,
//! `Climate` handle, two f32, `TextureBase` handle, f32, byte, embedded `Wind`,
//! embedded `WorldFog`, then a fixed 75-byte tail (u32 `+3356`, nine f32 `+3360..+3392`,
//! byte `+3430`, four f32 `+3396`, bytes `+3431`/`+3432`, four f32 `+3412`).
//!
//! Decoded here:
//! - forward: the `LayeredSky` handle, and inside `PostEffects` (`0x14225A1D0`, 26
//!   embedded effects) only the first two: `SSAOSettings` (`0x14225A600`, its
//!   `SSBCSettings` `0x14225A6E0`) and `HDRLightingSettings` (`0x142258830`, two
//!   `TextureSelector`s `0x142205470`). The 24 later effects, `Wind` and `WorldFog`
//!   are not decoded, so this is NOT an exact-end decode of the whole payload;
//! - backward: the fixed tail, read from the payload end. Its position is exact only
//!   if nothing follows the last loader read; the reflected names of the tail fields
//!   (`PSSMShadowEndDistance`, `PSSMSplitWeight`, `PSSMShadowNoiseSize`) give the check.
//!
//! Field meanings beyond reflected names are inferences and are labelled as such.

use serde::Serialize;

use crate::gi::{expect_type, f32s, hex16, root_header};
use crate::reader::Reader;
use crate::{Result, invalid};

pub const CLASS_WORLD_AMBIANCE: u32 = 0x21c2_d472;
const CLASS_POST_EFFECTS: u32 = 0xc83b_8907;
const CLASS_SSAO: u32 = 0xd82e_4804;
const CLASS_SSBC: u32 = 0xd52d_0013;
const CLASS_HDR: u32 = 0x33b7_7fec;
const CLASS_TEXTURE_SELECTOR: u32 = 0x7d08_460d;
/// Bytes of the loader's fixed tail.
pub const TAIL_BYTES: usize = 75;

/// `0x14270E560` handle as a reference (tag 1 or 2..): u8 tag, u8, u64 id.
fn reference(r: &mut Reader) -> Result<(u8, String)> {
    let at = r.position;
    let tag = r.u8()?;
    r.u8()?;
    let id = r.u64()?;
    if tag == 0 {
        return invalid(format!(
            "inline handle object at {at} is not supported here"
        ));
    }
    Ok((tag, hex16(id)))
}

/// `0x14270BB60` (u8, u64, u32) or `0x14270BC80` (u64, u32) embedded object header.
fn embedded(r: &mut Reader, class: u32, with_flag: bool) -> Result<()> {
    if with_flag {
        r.u8()?;
    }
    r.u64()?;
    expect_type(r, class)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ssao {
    pub enabled: u8,
    /// Field hashes `F70B4DE9` (`+96`), `393E26EB` (`+100`), `7DAFEB7B` (`+104`, u32) and
    /// `3B65018F` (`+16`); names unresolved.
    pub value96: f32,
    pub value100: f32,
    pub value104: u32,
    pub value16: u32,
    pub ssbc_enabled: u8,
    pub ssbc: [f32; 8],
    pub ssbc_byte64: u8,
    pub ssbc_tail: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HdrLighting {
    pub enabled: u8,
    pub byte288: u8,
    /// `+120`, `+124` (hashes `21E0F3A4`, `D8B393E6`): INFERRED aperture f-stop and
    /// reciprocal shutter time (5.6 and 250 in every Paris preset).
    pub aperture: f32,
    pub shutter_reciprocal: f32,
    /// `+128`, reflected name `ISO`.
    pub iso: f32,
    /// `+132` (hash `6D207AC7`): varies by time of day (Noon 3.0, Night -4.8); meaning unknown.
    pub value132: f32,
    /// `+136` (hash `B03F8DEA`): 18.0 in Noon (INFERRED: middle grey percent).
    pub value136: f32,
    pub byte289: u8,
    pub value140: f32,
    pub byte290: u8,
    pub value144: f32,
    pub value16: u32,
    pub selectors: [[u32; 3]; 2],
    pub selector_refs: [[(u8, String); 2]; 2],
    pub value148: [f32; 3],
    /// `+160`: 32 f32, unknown role (a per-bin curve in Noon).
    pub curve160: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShadowTail {
    /// `+3356` (hash `B1D9DB63`, u32). INFERRED: shadow map resolution (1024).
    pub value3356: u32,
    pub pssm_shadow_end_distance: f32,
    pub pssm_split_weight: f32,
    pub value3368: f32,
    pub pssm_shadow_noise_size: f32,
    /// `+3376..+3392` (hashes `B4BA625D`, `D0CB609E`, `976B1A4E`, `AA0B33FE`, `182BEFEE`).
    pub values3376: [f32; 5],
    pub byte3430: u8,
    /// `+3396`: four f32, INFERRED per-cascade far distances (5, 14, 38, 72 m in Paris).
    pub cascades3396: [f32; 4],
    pub byte3431: u8,
    pub byte3432: u8,
    /// `+3412`: four f32, unknown per-cascade values.
    pub cascades3412: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorldAmbiance {
    pub object_id: String,
    pub layered_sky: (u8, String),
    pub post_effects_bytes: [u8; 2],
    pub ssao: Ssao,
    pub hdr: HdrLighting,
    /// Payload offset where forward decoding stopped (start of the third post effect).
    pub forward_end: usize,
    pub shadow: ShadowTail,
}

/// `TextureSelector` values and its two texture references.
type Selector = ([u32; 3], [(u8, String); 2]);

fn texture_selector(r: &mut Reader) -> Result<Selector> {
    embedded(r, CLASS_TEXTURE_SELECTOR, true)?;
    let v = [r.u32()?, r.u32()?, r.u32()?];
    Ok((v, [reference(r)?, reference(r)?]))
}

fn shadow_tail(tail: &[u8]) -> Result<ShadowTail> {
    let mut r = Reader::new(tail);
    let value3356 = r.u32()?;
    let f = f32s::<9>(&mut r)?;
    let byte3430 = r.u8()?;
    let cascades3396 = f32s::<4>(&mut r)?;
    let byte3431 = r.u8()?;
    let byte3432 = r.u8()?;
    let cascades3412 = f32s::<4>(&mut r)?;
    debug_assert_eq!(r.remaining(), 0);
    Ok(ShadowTail {
        value3356,
        pssm_shadow_end_distance: f[0],
        pssm_split_weight: f[1],
        value3368: f[2],
        pssm_shadow_noise_size: f[3],
        values3376: [f[4], f[5], f[6], f[7], f[8]],
        byte3430,
        cascades3396,
        byte3431,
        byte3432,
        cascades3412,
    })
}

/// Decode the supported parts of a `WorldAmbiance` payload (see the module notes).
pub fn decode(payload: &[u8]) -> Result<WorldAmbiance> {
    let mut r = Reader::new(payload);
    let object_id = hex16(root_header(&mut r, CLASS_WORLD_AMBIANCE)?);
    let layered_sky = reference(&mut r)?;
    embedded(&mut r, CLASS_POST_EFFECTS, true)?;
    let post_effects_bytes = [r.u8()?, r.u8()?];

    embedded(&mut r, CLASS_SSAO, false)?;
    let enabled = r.u8()?;
    let value96 = f32::from_bits(r.u32()?);
    let value100 = f32::from_bits(r.u32()?);
    let value104 = r.u32()?;
    let value16 = r.u32()?;
    embedded(&mut r, CLASS_SSBC, false)?;
    let ssbc_enabled = r.u8()?;
    let ssbc = f32s::<8>(&mut r)?;
    let ssbc_byte64 = r.u8()?;
    let ssbc_tail = f32s::<4>(&mut r)?;
    let ssao = Ssao {
        enabled,
        value96,
        value100,
        value104,
        value16,
        ssbc_enabled,
        ssbc,
        ssbc_byte64,
        ssbc_tail,
    };

    embedded(&mut r, CLASS_HDR, false)?;
    let hdr_enabled = r.u8()?;
    let byte288 = r.u8()?;
    let v = f32s::<5>(&mut r)?;
    let byte289 = r.u8()?;
    let value140 = f32::from_bits(r.u32()?);
    let byte290 = r.u8()?;
    let value144 = f32::from_bits(r.u32()?);
    let hdr_value16 = r.u32()?;
    let (s0, r0) = texture_selector(&mut r)?;
    let (s1, r1) = texture_selector(&mut r)?;
    let value148 = f32s::<3>(&mut r)?;
    let curve160 = f32s::<32>(&mut r)?.to_vec();
    let hdr = HdrLighting {
        enabled: hdr_enabled,
        byte288,
        aperture: v[0],
        shutter_reciprocal: v[1],
        iso: v[2],
        value132: v[3],
        value136: v[4],
        byte289,
        value140,
        byte290,
        value144,
        value16: hdr_value16,
        selectors: [s0, s1],
        selector_refs: [r0, r1],
        value148,
        curve160,
    };
    let forward_end = r.position;
    if payload.len() < forward_end + TAIL_BYTES {
        return invalid(format!(
            "WorldAmbiance of {} bytes leaves no room for the {TAIL_BYTES}-byte tail after {forward_end}",
            payload.len()
        ));
    }
    let shadow = shadow_tail(&payload[payload.len() - TAIL_BYTES..])?;
    Ok(WorldAmbiance {
        object_id,
        layered_sky,
        post_effects_bytes,
        ssao,
        hdr,
        forward_end,
        shadow,
    })
}

impl HdrLighting {
    /// EV100 of the stored camera, INFERRED from `aperture`, `1 / shutter_reciprocal`
    /// and `iso` with the standard `log2(N^2 / t * 100 / S)`.
    pub fn ev100(&self) -> Option<f32> {
        (self.aperture > 0.0 && self.shutter_reciprocal > 0.0 && self.iso > 0.0).then(|| {
            (self.aperture * self.aperture * self.shutter_reciprocal * 100.0 / self.iso).log2()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(b: &mut Vec<u8>, v: &[f32]) {
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }

    fn header(b: &mut Vec<u8>, class: u32, flag: bool) {
        if flag {
            b.push(0);
        }
        b.extend_from_slice(&0x7f5f_1bbe2u64.to_le_bytes());
        b.extend_from_slice(&class.to_le_bytes());
    }

    /// Independent synthetic payload with Noon-like values and a filler middle.
    fn synthetic(filler: usize) -> Vec<u8> {
        let mut b = vec![1];
        b.extend_from_slice(&0x7f5f_1bbe1u64.to_le_bytes());
        b.extend_from_slice(&CLASS_WORLD_AMBIANCE.to_le_bytes());
        b.extend_from_slice(&[1, 0]);
        b.extend_from_slice(&0x7f5f_1bb4au64.to_le_bytes());
        header(&mut b, CLASS_POST_EFFECTS, true);
        b.extend_from_slice(&[0, 0]);
        header(&mut b, CLASS_SSAO, false);
        b.push(1);
        f(&mut b, &[7.0, 2.5]);
        b.extend_from_slice(&5u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        header(&mut b, CLASS_SSBC, false);
        b.push(0);
        f(&mut b, &[0.0, 1.7, 0.35, 0.55, 2.0, 0.6, 5.0, 50.0]);
        b.push(0);
        f(&mut b, &[2000.0, 2500.0, 0.33, 2.5]);
        header(&mut b, CLASS_HDR, false);
        b.extend_from_slice(&[1, 1]);
        f(&mut b, &[5.6, 250.0, 100.0, 3.0, 18.0]);
        b.push(1);
        f(&mut b, &[1.5]);
        b.push(1);
        f(&mut b, &[0.5]);
        b.extend_from_slice(&0u32.to_le_bytes());
        for _ in 0..2 {
            header(&mut b, CLASS_TEXTURE_SELECTOR, true);
            for v in [2u32, 11, 0] {
                b.extend_from_slice(&v.to_le_bytes());
            }
            for _ in 0..2 {
                b.extend_from_slice(&[3, 0]);
                b.extend_from_slice(&0u64.to_le_bytes());
            }
        }
        f(&mut b, &[0.0, 1.5, 3.0]);
        f(&mut b, &[0.25; 32]);
        b.extend(std::iter::repeat_n(0xcd, filler));
        b.extend_from_slice(&1024u32.to_le_bytes());
        f(&mut b, &[128.0, 0.9, 0.0, 3.0, 2.0, 1e-4, 2e-4, 3e-4, 5e-4]);
        b.push(1);
        f(&mut b, &[5.0, 14.0, 38.0, 72.0]);
        b.extend_from_slice(&[1, 1]);
        f(&mut b, &[0.65, 1.3, 2.6, 5.2]);
        b
    }

    #[test]
    fn synthetic_ambiance_decodes_prefix_and_tail() -> Result<()> {
        let w = decode(&synthetic(100))?;
        assert_eq!(w.layered_sky.1, "00000007f5f1bb4a");
        assert_eq!(w.ssao.value104, 5);
        assert_eq!(w.hdr.iso, 100.0);
        assert_eq!(w.hdr.aperture, 5.6);
        assert_eq!(w.shadow.value3356, 1024);
        assert_eq!(w.shadow.pssm_shadow_end_distance, 128.0);
        assert_eq!(w.shadow.cascades3396, [5.0, 14.0, 38.0, 72.0]);
        let ev = w.hdr.ev100().expect("ev");
        assert!((ev - 12.936).abs() < 1e-3, "{ev}");
        Ok(())
    }

    #[test]
    fn ambiance_rejects_bad_inputs() {
        let good = synthetic(100);
        let mut b = good.clone();
        b[9] ^= 1;
        assert!(decode(&b).is_err(), "wrong class");
        let mut b = good.clone();
        b[13] = 0;
        assert!(decode(&b).unwrap_err().to_string().contains("inline"));
        // No room for the tail after the forward part.
        let short = synthetic(0);
        assert!(decode(&short[..short.len() - 1]).is_err());
        for cut in [0, 12, 30, 60] {
            assert!(decode(&good[..cut]).is_err(), "cut {cut}");
        }
    }
}
