//! Albedo recolouring of the `CN_Cloth_Tile_ColorMasked_AlphaTest` material
//! template (`000000358c3a2f08`), re-derived from its embedded ps_5_0 code
//! (ACU-RE-0071). All 20 colour-writing pixel variants share this path.
//!
//! The mask texture (map type 7, linear) selects one HSB vector per texel from
//! its red channel; the diffuse colour is rotated in hue and scaled in
//! saturation and brightness with the shader's YIQ-style matrix. Values are
//! (hue in turns, saturation, brightness); (1, 1, 1) is the identity.
//! [`Surface`] holds the template's normal, gloss and specular parameters
//! (ACU-RE-0084); the Bevy character shader evaluates them per pixel.
//! Not modelled: the distance dissolve factor (1 away from its centre).

use serde::Serialize;

use crate::material::{Material, ParameterValue};

/// The shader's own literal for one hue turn (`mul r2.x, r3.y, l(6.280000)`);
/// deliberately not `TAU`.
#[allow(clippy::approx_constant)]
const SHADER_TURN: f32 = 6.28;

pub const TEMPLATE_CLOTH_COLOR_MASKED_ALPHA_TEST: &str = "000000358c3a2f08";

/// Material parameter hashes and the constant register each feeds (template
/// records of class `92d220dd`).
pub const HSB_ZONE_R1: u32 = 0xb000_af3e; // cb3[19], mask R >= 0.9
pub const HSB_BASE: u32 = 0x8ca9_8c4d; // cb3[20], mask R <= 0.1
pub const HSB_ZONE_R2: u32 = 0xdddd_4bd5; // cb3[21], 0.6 <= R < 0.9
pub const HSB_ZONE_R3: u32 = 0xadb7_bf5a; // cb3[22], 0.4 < R < 0.6
pub const TINT_A: u32 = 0xe936_7dfe; // cb3[24]
pub const HSB_ZONE_R4: u32 = 0x34be_eee0; // cb3[25], 0.1 < R <= 0.4
pub const TINT_B: u32 = 0x227c_0a72; // cb3[26]
pub const DESATURATE: u32 = 0xfc59_4e9e; // cb3[10]
pub const TINT_SELECTOR: u32 = 0xa64e_88a4; // t35
pub const MASK_SELECTOR: u32 = 0x9ae6_abcd; // t34

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Params {
    pub base: [f32; 3],
    /// R >= 0.9, 0.6..0.9, 0.4..0.6, 0.1..0.4.
    pub zones: [[f32; 3]; 4],
    pub desaturate: f32,
    pub tint_a: [f32; 3],
    pub tint_b: [f32; 3],
}

fn vec3(m: &Material, hash: u32) -> Option<[f32; 3]> {
    let name = format!("{hash:08x}");
    m.parameters
        .iter()
        .find(|p| p.name_hash == name)
        .and_then(|p| match p.value {
            ParameterValue::Vector4(v) => Some([v[0], v[1], v[2]]),
            _ => None,
        })
}

impl Params {
    /// Read the material's own values; `None` when a required parameter is absent.
    pub fn from_material(m: &Material) -> Option<Self> {
        let desaturate = m
            .parameters
            .iter()
            .find(|p| p.name_hash == format!("{DESATURATE:08x}"))
            .and_then(|p| match p.value {
                ParameterValue::Float(v) => Some(v),
                _ => None,
            })?;
        Some(Self {
            base: vec3(m, HSB_BASE)?,
            zones: [
                vec3(m, HSB_ZONE_R1)?,
                vec3(m, HSB_ZONE_R2)?,
                vec3(m, HSB_ZONE_R3)?,
                vec3(m, HSB_ZONE_R4)?,
            ],
            desaturate,
            tint_a: vec3(m, TINT_A)?,
            tint_b: vec3(m, TINT_B)?,
        })
    }

    /// Apply one written material property (from a colour-set row). Returns
    /// whether the hash is a parameter this path consumes.
    pub fn set(&mut self, hash: u32, v: [f32; 4]) -> bool {
        let v3 = [v[0], v[1], v[2]];
        match hash {
            HSB_BASE => self.base = v3,
            HSB_ZONE_R1 => self.zones[0] = v3,
            HSB_ZONE_R2 => self.zones[1] = v3,
            HSB_ZONE_R3 => self.zones[2] = v3,
            HSB_ZONE_R4 => self.zones[3] = v3,
            TINT_A => self.tint_a = v3,
            TINT_B => self.tint_b = v3,
            _ => return false,
        }
        true
    }

    /// Shader albedo for one texel. `diffuse` is linear RGB (sRGB-decoded),
    /// `mask` is the linear mask RGB, `tint` the red channel of the t35 map.
    pub fn albedo(&self, diffuse: [f32; 3], mask: [f32; 3], tint: f32) -> [f32; 3] {
        let (r, g) = (mask[0], mask[1]);
        let fl = |x: f32| x.floor();
        let z1 = fl(r + 0.1).max(0.0); // R >= 0.9
        let z2 = (fl(r + 0.4) - fl(r + 0.1)).clamp(0.0, 1.0); // 0.6 <= R < 0.9
        let band = (fl(1.4 - r) - fl(1.1 - r)).clamp(0.0, 1.0); // 0.1 < R <= 0.4
        let z3 = (1.0 - (fl(1.1 - r) + fl(r + 0.4) + band).clamp(0.0, 1.0)).max(0.0); // 0.4 < R < 0.6
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| {
            std::array::from_fn::<f32, 3, _>(|i| a[i] + (b[i] - a[i]) * t)
        };
        let mut hsb = lerp(self.base, self.zones[0], z1);
        hsb = lerp(hsb, self.zones[1], z2);
        hsb = lerp(hsb, self.zones[2], z3);
        hsb = lerp(hsb, self.zones[3], band);
        let [h, s, v] = hsb;
        let vs = (1.0 - self.desaturate) * s * v;
        let (sin, cos) = (h * SHADER_TURN).sin_cos();
        let (u, w) = (vs * cos, vs * sin);
        let [cr, cg, cb] = diffuse;
        let out = [
            (0.299 * v + 0.701 * u + 0.168 * w) * cr
                + (0.587 * v - 0.587 * u + 0.330 * w) * cg
                + (0.114 * v - 0.114 * u - 0.497 * w) * cb,
            (0.299 * v - 0.299 * u - 0.328 * w) * cr
                + (0.587 * v + 0.413 * u + 0.035 * w) * cg
                + (0.114 * v - 0.114 * u + 0.292 * w) * cb,
            (0.299 * v - 0.300 * u + 1.25 * w) * cr
                + (0.587 * v - 0.588 * u - 1.05 * w) * cg
                + (0.114 * v + 0.886 * u - 0.203 * w) * cb,
        ];
        finish(out, g, lerp(self.tint_b, self.tint_a, tint))
    }

    /// Precompute the per-zone colour matrices. Zone weights are exactly 0 or
    /// 1 in the shader, so selecting a matrix equals the lerp chain above.
    pub fn prepare(&self) -> Prepared {
        let hsbs = [
            self.base,
            self.zones[0],
            self.zones[1],
            self.zones[2],
            self.zones[3],
        ];
        let matrices = hsbs.map(|[h, s, v]| {
            let vs = (1.0 - self.desaturate) * s * v;
            let (sin, cos) = (h * SHADER_TURN).sin_cos();
            let (u, w) = (vs * cos, vs * sin);
            [
                [
                    0.299 * v + 0.701 * u + 0.168 * w,
                    0.587 * v - 0.587 * u + 0.330 * w,
                    0.114 * v - 0.114 * u - 0.497 * w,
                ],
                [
                    0.299 * v - 0.299 * u - 0.328 * w,
                    0.587 * v + 0.413 * u + 0.035 * w,
                    0.114 * v - 0.114 * u + 0.292 * w,
                ],
                [
                    0.299 * v - 0.300 * u + 1.25 * w,
                    0.587 * v - 0.588 * u - 1.05 * w,
                    0.114 * v + 0.886 * u - 0.203 * w,
                ],
            ]
        });
        Prepared {
            matrices,
            tint_a: self.tint_a,
            tint_b: self.tint_b,
        }
    }
}

fn finish(out: [f32; 3], g: f32, tint3: [f32; 3]) -> [f32; 3] {
    let metal = ((0.247_059 - g) * 4.047_619).clamp(0.0, 1.0);
    let cloth = ((g - 0.752_941) * 4.047_619).clamp(0.0, 1.0);
    let scale = 1.217_638 - metal - cloth;
    std::array::from_fn(|i| out[i] * (1.0 + cloth * (tint3[i] - 1.0)) * scale)
}

/// Mask-R zone: 0 base (R <= 0.1), 1 (R >= 0.9), 2 (0.6..0.9), 3 (0.4..0.6), 4 (0.1..0.4).
pub fn zone(r: f32) -> usize {
    let fl = |x: f32| x.floor();
    if fl(r + 0.1) >= 1.0 {
        1
    } else if fl(r + 0.4) - fl(r + 0.1) >= 1.0 {
        2
    } else if fl(1.4 - r) - fl(1.1 - r) >= 1.0 {
        4
    } else if fl(1.1 - r) + fl(r + 0.4) < 1.0 {
        3
    } else {
        0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Prepared {
    matrices: [[[f32; 3]; 3]; 5],
    tint_a: [f32; 3],
    tint_b: [f32; 3],
}

impl Prepared {
    pub fn albedo(&self, diffuse: [f32; 3], mask: [f32; 3], tint: f32) -> [f32; 3] {
        let m = &self.matrices[zone(mask[0])];
        let out = std::array::from_fn(|i| {
            m[i][0] * diffuse[0] + m[i][1] * diffuse[1] + m[i][2] * diffuse[2]
        });
        let tint3 =
            std::array::from_fn(|i| self.tint_b[i] + (self.tint_a[i] - self.tint_b[i]) * tint);
        finish(out, mask[1], tint3)
    }
}

/// Texture parameters of the template (records with registers 160..167 =
/// t32..t39). t37 is a noise map used only by the dissolve.
pub const DIFFUSE_SELECTOR: u32 = 0x1371_c615; // t32
pub const NORMAL_SELECTOR: u32 = 0x4c67_1014; // t33 (RG normal, A gloss)
pub const DETAIL_METAL: u32 = 0x14c6_a86b; // t36, weighted by metal * cb3[11]
pub const NOISE: u32 = 0x4ddb_34ee; // t37
pub const DETAIL_CLOTH: u32 = 0xe306_9623; // t38, cloth * cb3[4] + mid * cb3[3]
pub const DETAIL_BASE: u32 = 0xd7e5_0c4a; // t39, base * cb3[1]
pub const DETAIL_UV_SCALE: u32 = 0x8f6d_6273; // cb3[23]

/// Scalar parameters (`cb3` register in the comment) that drive the
/// template's normal, gloss and specular outputs (G-buffer pixel shaders of
/// the template, ACU-RE-0084). Registers 7-9 and 27 (dissolve) and 13 / 16
/// (translucency) are not modelled here.
pub const DETAIL_BASE_WEIGHT: u32 = 0x4cba_78d8; // cb3[1]
pub const METAL_HUE_SCALE: u32 = 0xc541_6a6d; // cb3[2]
pub const DETAIL_MID_WEIGHT: u32 = 0xd8b0_b4e3; // cb3[3]
pub const DETAIL_CLOTH_WEIGHT: u32 = 0x50bf_8584; // cb3[4]
pub const CLOTH_SPECULAR: u32 = 0x84f7_2d37; // cb3[5]
pub const WET_BIAS: u32 = 0x99f3_1a87; // cb3[6]
pub const DETAIL_METAL_WEIGHT: u32 = 0xccc4_a04b; // cb3[11]
pub const CLOTH_SATURATION: u32 = 0xc9c7_3faf; // cb3[12]
pub const METAL_SATURATION: u32 = 0xd3cd_1895; // cb3[14]
pub const MID_SPECULAR: u32 = 0xebc1_dd6d; // cb3[15]
pub const HUE_SCALE: u32 = 0xe640_aad7; // cb3[17]
pub const MID_THRESHOLD: u32 = 0x5f7a_6196; // cb3[18]

/// Per-texel band weights from mask G. `metal + cloth + mid + base == 1`
/// unless the first three already exceed 1 (then base is 0).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Weights {
    pub metal: f32,
    pub cloth: f32,
    pub mid: f32,
    pub base: f32,
}

pub fn weights(g: f32, mid_threshold: f32) -> Weights {
    let metal = ((0.247_059 - g) * 4.047_619).clamp(0.0, 1.0);
    let cloth = ((g - 0.752_941) * 4.047_619).clamp(0.0, 1.0);
    let mid = (1.0 - cloth) * (g - mid_threshold).clamp(0.0, 1.0) * 8.0;
    let base = 1.0 - (metal + mid + cloth).min(1.0);
    Weights {
        metal,
        cloth,
        mid,
        base,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Surface {
    pub detail_base: f32,
    pub metal_hue: f32,
    pub detail_mid: f32,
    pub detail_cloth: f32,
    pub cloth_specular: f32,
    pub wet_bias: f32,
    pub detail_metal: f32,
    pub cloth_saturation: f32,
    pub metal_saturation: f32,
    pub mid_specular: f32,
    pub hue_scale: f32,
    pub mid_threshold: f32,
    pub detail_uv_scale: [f32; 2],
}

fn float(m: &Material, hash: u32) -> Option<f32> {
    let name = format!("{hash:08x}");
    m.parameters
        .iter()
        .find(|p| p.name_hash == name)
        .and_then(|p| match p.value {
            ParameterValue::Float(v) => Some(v),
            _ => None,
        })
}

impl Surface {
    /// Read the material's own values; `None` when a parameter is absent.
    pub fn from_material(m: &Material) -> Option<Self> {
        let uv = vec3(m, DETAIL_UV_SCALE)?;
        Some(Self {
            detail_base: float(m, DETAIL_BASE_WEIGHT)?,
            metal_hue: float(m, METAL_HUE_SCALE)?,
            detail_mid: float(m, DETAIL_MID_WEIGHT)?,
            detail_cloth: float(m, DETAIL_CLOTH_WEIGHT)?,
            cloth_specular: float(m, CLOTH_SPECULAR)?,
            wet_bias: float(m, WET_BIAS)?,
            detail_metal: float(m, DETAIL_METAL_WEIGHT)?,
            cloth_saturation: float(m, CLOTH_SATURATION)?,
            metal_saturation: float(m, METAL_SATURATION)?,
            mid_specular: float(m, MID_SPECULAR)?,
            hue_scale: float(m, HUE_SCALE)?,
            mid_threshold: float(m, MID_THRESHOLD)?,
            detail_uv_scale: [uv[0], uv[1]],
        })
    }

    /// Specular reflectance (the template's `t_SpecularReflectance` output)
    /// from the tinted albedo before the band scale (only its hue and
    /// saturation matter) and the band weights.
    pub fn specular(&self, albedo: [f32; 3], w: Weights) -> [f32; 3] {
        let (hue, sat) = hue_saturation(albedo);
        let saturation = (w.metal * self.metal_saturation + w.cloth * self.cloth_saturation) * sat;
        let h = hue * ((w.cloth + w.mid) * self.hue_scale + w.metal * self.metal_hue);
        let hue_rgb = [
            ((h * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0),
            (2.0 - (h * 6.0 - 2.0).abs()).clamp(0.0, 1.0),
            (2.0 - (h * 6.0 - 4.0).abs()).clamp(0.0, 1.0),
        ];
        let intensity = w.base * 0.04
            + w.cloth * self.cloth_specular
            + w.mid * self.mid_specular
            + w.metal * 0.9;
        hue_rgb.map(|c| ((c - 1.0) * saturation + 1.0) * intensity)
    }

    /// Velvet-lobe blend (the template's `t_MiscProps.x`): 1 cloth, 0.5 mid,
    /// 0.1 base, 0 metal, reduced by wetness.
    pub fn velvet(&self, w: Weights, wet: f32) -> f32 {
        (w.cloth + 0.5 * w.mid + 0.1 * w.base - wet).clamp(0.0, 1.0)
    }
}

/// RGB -> (hue in turns, saturation) as the template computes it; both are 0
/// for a grey input.
pub fn hue_saturation([r, g, b]: [f32; 3]) -> (f32, f32) {
    let max = r.max(g).max(b);
    let range = max - r.min(g).min(b);
    if range == 0.0 {
        return (0.0, 0.0);
    }
    let d = [(max - b) / range, (max - r) / range, (max - g) / range];
    // each channel equal to the maximum offers a candidate; the largest wins
    let candidate = |is_max: bool, v: f32| if is_max { v } else { 0.0 };
    let h = candidate(r >= max, d[0] - d[2] + 6.0)
        .max(candidate(b >= max, d[2] - d[1] + 4.0))
        .max(candidate(g >= max, d[1] - d[0] + 2.0));
    ((h / 6.0).fract(), range / max)
}

/// Combine the base gloss (normal-map alpha) with the blended detail gloss:
/// the specular powers `2^(19 g)` add as reciprocals, then wetness pushes the
/// result towards `1.5 * max(g, 0.1)`.
pub fn combine_gloss(base: f32, detail: f32, wet: f32) -> f32 {
    let inv = (-19.0 * base).exp2() + (-19.0 * detail).exp2();
    let g = ((1.0 / inv).log2() / 19.0).clamp(0.0, 1.0);
    g + wet * (g.max(0.1) * 1.5 - g)
}

/// ACU's deferred pass evaluates GGX with `alpha^2 = 2 / (2^(19 g) + 2)`;
/// Bevy's perceptual roughness is `sqrt(alpha)`.
pub fn gloss_to_perceptual_roughness(g: f32) -> f32 {
    (2.0 / ((19.0 * g).exp2() + 2.0)).powf(0.25)
}

pub fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Params {
        Params {
            base: [1.0; 3],
            zones: [[1.0; 3]; 4],
            desaturate: 0.0,
            tint_a: [1.0; 3],
            tint_b: [0.0; 3],
        }
    }

    const NEUTRAL_G: f32 = 0.5; // neither metal nor cloth band: scale 1.217638

    #[test]
    fn identity_hsb_only_applies_template_scale() {
        let p = identity();
        let c = [0.2, 0.4, 0.6];
        let out = p.albedo(c, [1.0, NEUTRAL_G, 0.0], 1.0);
        for i in 0..3 {
            // a full turn (6.28 rad, not 2*pi) leaves a ~0.3% chroma residue
            assert!((out[i] - c[i] * 1.217_638).abs() < 0.01, "{out:?}");
        }
    }

    #[test]
    fn grey_is_hue_invariant_and_brightness_scales() {
        let mut p = identity();
        p.zones[1] = [0.37, 1.5, 0.5];
        let out = p.albedo([0.5; 3], [0.7, NEUTRAL_G, 0.0], 1.0);
        // matrix columns sum to v for r = g = b (chroma terms cancel to within the coefficients' rounding)
        for v in out {
            assert!((v - 0.5 * 0.5 * 1.217_638).abs() < 0.004, "{out:?}");
        }
    }

    #[test]
    fn zone_thresholds_follow_mask_red() {
        let mut p = identity();
        p.base = [0.0, 0.0, 0.1];
        p.zones = [
            [0.0, 0.0, 0.2],
            [0.0, 0.0, 0.3],
            [0.0, 0.0, 0.4],
            [0.0, 0.0, 0.5],
        ];
        let level = |r: f32| p.albedo([1.0; 3], [r, NEUTRAL_G, 0.0], 1.0)[0] / 1.217_638;
        for (r, want) in [
            (0.0, 0.1),
            (0.05, 0.1),
            (0.25, 0.5),
            (0.5, 0.4),
            (0.75, 0.3),
            (0.95, 0.2),
            (1.0, 0.2),
        ] {
            assert!(
                (level(r) - want).abs() < 1e-4,
                "R {r}: {} != {want}",
                level(r)
            );
        }
    }

    #[test]
    fn half_turn_rotates_blue_toward_yellow() {
        let mut p = identity();
        p.zones[0] = [0.5, 1.0, 1.0];
        let out = p.albedo([0.1, 0.2, 0.8], [1.0, NEUTRAL_G, 0.0], 1.0);
        assert!(out[0] > out[2] && out[1] > out[2], "{out:?}");
    }

    #[test]
    fn mask_green_bands_scale_albedo() {
        let p = identity();
        let metal = p.albedo([0.5; 3], [1.0, 0.0, 0.0], 1.0)[0];
        let cloth = p.albedo([0.5; 3], [1.0, 1.0, 0.0], 1.0)[0];
        assert!((metal - 0.5 * 0.217_638).abs() < 1e-3);
        assert!((cloth - 0.5 * 0.217_638).abs() < 1e-3);
    }

    #[test]
    fn prepared_matches_reference_path() {
        let mut p = identity();
        p.base = [0.1, 0.9, 0.8];
        p.zones = [
            [1.02, 0.5, 1.0],
            [0.62, 1.2, 1.0],
            [0.0, 0.0, 1.1],
            [1.43, 0.74, 0.36],
        ];
        let q = p.prepare();
        for i in 0..=40 {
            let r = i as f32 / 40.0;
            for g in [0.0, 0.2, 0.5, 0.8, 1.0] {
                let (a, b) = (
                    p.albedo([0.3, 0.5, 0.7], [r, g, 0.0], 1.0),
                    q.albedo([0.3, 0.5, 0.7], [r, g, 0.0], 1.0),
                );
                for k in 0..3 {
                    assert!((a[k] - b[k]).abs() < 1e-5, "r {r} g {g}: {a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn srgb_round_trip() {
        for c in [0u8, 1, 10, 64, 128, 200, 255] {
            assert_eq!(linear_to_srgb(srgb_to_linear(c)), c);
        }
    }

    fn surface() -> Surface {
        // Arno's upper-body values (DataPC_ACU_LGS_BelleEpoque.forge entry 370).
        Surface {
            detail_base: 4.0,
            metal_hue: 1.0,
            detail_mid: 4.0,
            detail_cloth: 4.0,
            cloth_specular: 0.042_278,
            wet_bias: 0.0,
            detail_metal: 1.0,
            cloth_saturation: 1.0,
            metal_saturation: 1.0,
            mid_specular: 0.02,
            hue_scale: 1.0,
            mid_threshold: 0.55,
            detail_uv_scale: [9.0, 18.0],
        }
    }

    #[test]
    fn band_weights_partition_mask_green() {
        let t = 0.55;
        let w = weights(0.0, t);
        assert_eq!((w.metal, w.cloth, w.mid, w.base), (1.0, 0.0, 0.0, 0.0));
        let w = weights(1.0, t);
        assert_eq!((w.metal, w.cloth, w.mid, w.base), (0.0, 1.0, 0.0, 0.0));
        let w = weights(0.5, t);
        assert_eq!((w.metal, w.cloth, w.mid, w.base), (0.0, 0.0, 0.0, 1.0));
        // just above the mid threshold: 8x ramp
        let w = weights(0.6, t);
        assert!(
            (w.mid - 0.4).abs() < 1e-5 && (w.base - 0.6).abs() < 1e-5,
            "{w:?}"
        );
    }

    #[test]
    fn hue_matches_hsv() {
        let close =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        assert!(close(hue_saturation([1.0, 0.0, 0.0]), (0.0, 1.0)));
        assert!(close(hue_saturation([0.0, 1.0, 0.0]), (1.0 / 3.0, 1.0)));
        assert!(close(hue_saturation([0.0, 0.0, 1.0]), (2.0 / 3.0, 1.0)));
        assert!(close(hue_saturation([1.0, 1.0, 0.0]), (1.0 / 6.0, 1.0)));
        assert!(close(hue_saturation([0.5, 0.25, 0.5]), (5.0 / 6.0, 0.5)));
        assert_eq!(hue_saturation([0.3, 0.3, 0.3]), (0.0, 0.0));
    }

    #[test]
    fn specular_bands() {
        let s = surface();
        let red = [0.8, 0.2, 0.2];
        // base band: grey dielectric 0.04 regardless of albedo
        let f = s.specular(red, weights(0.5, s.mid_threshold));
        assert!(f.iter().all(|c| (c - 0.04).abs() < 1e-6), "{f:?}");
        // metal band: 0.9 times the albedo hue at the albedo's saturation (0.75)
        let f = s.specular(red, weights(0.0, s.mid_threshold));
        assert!((f[0] - 0.9).abs() < 1e-5, "{f:?}");
        assert!(
            (f[1] - 0.9 * 0.25).abs() < 1e-5 && (f[2] - f[1]).abs() < 1e-5,
            "{f:?}"
        );
        // cloth band on grey: the velvet intensity, uncoloured
        let f = s.specular([0.5, 0.5, 0.5], weights(1.0, s.mid_threshold));
        assert!(f.iter().all(|c| (c - 0.042_278).abs() < 1e-6), "{f:?}");
        // hue and saturation are scale invariant: the band scale does not matter
        let a = s.specular([0.1, 0.3, 0.6], weights(0.0, 0.55));
        let b = s.specular([0.2, 0.6, 1.2], weights(0.0, 0.55));
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6));
    }

    #[test]
    fn velvet_blend_by_band() {
        let s = surface();
        assert_eq!(s.velvet(weights(1.0, 0.55), 0.0), 1.0);
        assert_eq!(s.velvet(weights(0.0, 0.55), 0.0), 0.0);
        assert!((s.velvet(weights(0.5, 0.55), 0.0) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn gloss_combination_and_roughness() {
        // detail gloss 4 (no detail weight): base gloss passes through
        for g in [0.0, 0.3, 0.7, 1.0] {
            assert!((combine_gloss(g, 4.0, 0.0) - g).abs() < 1e-4, "{g}");
        }
        // equal powers halve: g drops by 1/19
        let g = combine_gloss(0.5, 0.5, 0.0);
        assert!((g - (0.5 - 1.0 / 19.0)).abs() < 1e-5, "{g}");
        // full wetness: 1.5 * max(g, 0.1)
        assert!((combine_gloss(0.0, 4.0, 1.0) - 0.15).abs() < 1e-5);
        assert!((combine_gloss(0.4, 4.0, 1.0) - 0.6).abs() < 1e-4);
        // roughness endpoints: g = 0 gives alpha^2 = 2/3
        let r = gloss_to_perceptual_roughness(0.0);
        assert!((r.powi(4) - 2.0 / 3.0).abs() < 1e-6);
        assert!(gloss_to_perceptual_roughness(1.0) < 0.05);
        assert!(gloss_to_perceptual_roughness(0.3) > gloss_to_perceptual_roughness(0.6));
    }
}
