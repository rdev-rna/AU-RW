//! Albedo recolouring of the `CN_Cloth_Tile_ColorMasked_AlphaTest` material
//! template (`000000358c3a2f08`), re-derived from its embedded ps_5_0 code
//! (ACU-RE-0071). All 20 colour-writing pixel variants share this path.
//!
//! The mask texture (map type 7, linear) selects one HSB vector per texel from
//! its red channel; the diffuse colour is rotated in hue and scaled in
//! saturation and brightness with the shader's YIQ-style matrix. Values are
//! (hue in turns, saturation, brightness); (1, 1, 1) is the identity.
//! Not modelled: the distance dissolve factor (1 away from its centre), and
//! the specular/velvet outputs, which do not change albedo.

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
}
