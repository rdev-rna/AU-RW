//! Skin (`CN_Skin_Wrinkle`) and hair (`CN_Hair`) material templates: the
//! parameters their G-buffer pixel shaders read, from the templates' records
//! of class `92d220dd` (ACU-RE-0084). Register numbers are `cb3` constants.
//!
//! Both templates also sample textures the material does not name (records
//! with the shared hash `4ddb34ee`): skin t34 / t35 / t40 and hair t33..t36.
//! Those are bound at run time and are not modelled here.

use serde::Serialize;

use crate::material::{Material, ParameterValue};

pub const TEMPLATE_SKIN_WRINKLE: &str = "00000006dc67dd6d";
pub const TEMPLATE_HAIR: &str = "00000011565b044b";

/// Shared by both templates (and the cloth template's cb3[10]).
pub const DESATURATE: u32 = 0xfc59_4e9e; // skin cb3[1], hair cb3[6]
pub const WET_BIAS: u32 = 0x99f3_1a87; // skin and hair cb3[7]

pub const SKIN_DIFFUSE: u32 = 0x5d09_d951; // t32
pub const SKIN_NORMAL: u32 = 0x30fb_00f6; // t33
pub const SKIN_WRINKLE_A: u32 = 0x9cc3_55fb; // t36, used when the vertex weight < cb3[8]
pub const SKIN_WRINKLE_B: u32 = 0xa0ce_6aa2; // t37, used when the vertex weight > cb3[9]
pub const SKIN_MASK: u32 = 0xf86d_bbcf; // t38: B = tint weight and specular occlusion
pub const SKIN_GLOSS_SCALE: u32 = 0x028a_dbcf; // cb3[2]
pub const SKIN_SCATTER: u32 = 0x5a74_9bb1; // cb3[5], written to t_SpecularReflectance.w
pub const SKIN_WRINKLE_LOW: u32 = 0xfb0f_4871; // cb3[8]
pub const SKIN_WRINKLE_HIGH: u32 = 0xcb9d_1dbf; // cb3[9]
pub const SKIN_TINT: u32 = 0xa4ed_3f29; // cb3[11]: rgb tint, w weight bias

pub const HAIR_DIFFUSE: u32 = 0x94d1_e376; // t32
pub const HAIR_UV_SCALE_U: u32 = 0x2af4_8215; // cb3[2]
pub const HAIR_UV_SCALE_V: u32 = 0xb3fd_d3af; // cb3[3]
pub const HAIR_TINT: u32 = 0x37f3_b8db; // cb3[8]
pub const HAIR_NOISE_MIX: u32 = 0x7c45_4238; // cb3[10]: weights of the two noise UV sets

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

fn vec4(m: &Material, hash: u32) -> Option<[f32; 4]> {
    let name = format!("{hash:08x}");
    m.parameters
        .iter()
        .find(|p| p.name_hash == name)
        .and_then(|p| match p.value {
            ParameterValue::Vector4(v) => Some(v),
            _ => None,
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Skin {
    pub desaturate: f32,
    /// The gloss is multiplied by `sat(wetness + gloss_scale)`.
    pub gloss_scale: f32,
    pub scatter: f32,
    pub wet_bias: f32,
    pub wrinkle_thresholds: [f32; 2],
    /// Albedo = lerp(tint.rgb, albedo, sat(mask B + tint.w)).
    pub tint: [f32; 4],
}

impl Skin {
    pub fn from_material(m: &Material) -> Option<Self> {
        Some(Self {
            desaturate: float(m, DESATURATE)?,
            gloss_scale: float(m, SKIN_GLOSS_SCALE)?,
            scatter: float(m, SKIN_SCATTER)?,
            wet_bias: float(m, WET_BIAS)?,
            wrinkle_thresholds: [float(m, SKIN_WRINKLE_LOW)?, float(m, SKIN_WRINKLE_HIGH)?],
            tint: vec4(m, SKIN_TINT)?,
        })
    }

    /// Skin albedo from the (linear) diffuse colour and the mask's B channel.
    pub fn albedo(&self, diffuse: [f32; 3], mask_b: f32) -> [f32; 3] {
        let c = apply(saturation_matrix(self.desaturate), diffuse);
        let w = (mask_b + self.tint[3]).clamp(0.0, 1.0);
        std::array::from_fn(|i| self.tint[i] + (c[i] - self.tint[i]) * w)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Hair {
    pub desaturate: f32,
    pub uv_scale: [f32; 2],
    pub wet_bias: f32,
    pub tint: [f32; 3],
    pub noise_mix: [f32; 2],
}

impl Hair {
    pub fn from_material(m: &Material) -> Option<Self> {
        let tint = vec4(m, HAIR_TINT)?;
        let mix = vec4(m, HAIR_NOISE_MIX)?;
        Some(Self {
            desaturate: float(m, DESATURATE)?,
            uv_scale: [float(m, HAIR_UV_SCALE_U)?, float(m, HAIR_UV_SCALE_V)?],
            wet_bias: float(m, WET_BIAS)?,
            tint: [tint[0], tint[1], tint[2]],
            noise_mix: [mix[0], mix[1]],
        })
    }

    /// Hair albedo: tint, then the saturation matrix (dissolve fade 1).
    pub fn albedo(&self, diffuse: [f32; 3]) -> [f32; 3] {
        let tinted = std::array::from_fn(|i| diffuse[i] * self.tint[i]);
        apply(saturation_matrix(self.desaturate), tinted)
    }
}

/// Primary specular reflectance of the hair path: the template writes
/// `0.8 * t34.x` to the albedo target's w, which the deferred hair path reads
/// as `pow(w, 2.2)`.
pub fn hair_reflectance(mask_x: f32) -> f32 {
    (0.8 * mask_x.clamp(0.0, 1.0)).powf(2.2)
}

/// The templates' saturation matrix (rows = output R, G, B) for
/// `k = 1 - desaturate`: luma rows at k = 0, within 0.5% of identity at k = 1.
/// Coefficients as the shaders use them (YIQ-derived, with their small
/// round-off terms).
pub fn saturation_matrix(desaturate: f32) -> [[f32; 3]; 3] {
    let k = 1.0 - desaturate;
    [
        [
            0.299 + 0.700_996 * k - 0.000_535 * k,
            0.587 - 0.586_997 * k - 0.001_051 * k,
            0.114 - 0.113_999 * k + 0.001_583 * k,
        ],
        [
            0.299 - 0.298_998 * k + 0.001_045 * k,
            0.587 + 0.412_998 * k - 0.000_111 * k,
            0.114 - 0.113_999 * k - 0.000_930 * k,
        ],
        [
            0.299 - 0.299_998 * k - 0.003_981 * k,
            0.587 - 0.587_997 * k + 0.003_344 * k,
            0.114 + 0.885_996 * k + 0.000_647 * k,
        ],
    ]
}

fn apply(m: [[f32; 3]; 3], c: [f32; 3]) -> [f32; 3] {
    m.map(|row| row[0] * c[0] + row[1] * c[1] + row[2] * c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturation_matrix_endpoints() {
        let grey = saturation_matrix(1.0);
        for row in grey {
            assert!((row[0] - 0.299).abs() < 1e-6);
            assert!((row[1] - 0.587).abs() < 1e-6);
            assert!((row[2] - 0.114).abs() < 1e-6);
        }
        let id = saturation_matrix(0.0);
        for (i, row) in id.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!((v - want).abs() < 0.005, "{i} {j} {v}");
            }
        }
    }

    #[test]
    fn skin_tint_weight() {
        let s = Skin {
            desaturate: 1.0,
            gloss_scale: 0.75,
            scatter: 0.75,
            wet_bias: 0.0,
            wrinkle_thresholds: [0.0, 0.0],
            tint: [1.0, 0.0, 0.0, 0.0],
        };
        // mask B 0: full tint; mask B 1: the (grey) albedo
        assert_eq!(s.albedo([0.5, 0.5, 0.5], 0.0), [1.0, 0.0, 0.0]);
        let a = s.albedo([0.5, 0.5, 0.5], 1.0);
        assert!(a.iter().all(|c| (c - 0.5).abs() < 1e-6), "{a:?}");
        // Arno's tint (0, 0, 0, 1) is a no-op whatever the mask
        let arno = Skin {
            desaturate: 0.0,
            tint: [0.0, 0.0, 0.0, 1.0],
            ..s
        };
        let a = arno.albedo([0.6, 0.4, 0.3], 0.0);
        assert!(
            (a[0] - 0.6).abs() < 0.005 && (a[2] - 0.3).abs() < 0.005,
            "{a:?}"
        );
    }

    #[test]
    fn hair_albedo_and_reflectance() {
        let h = Hair {
            desaturate: 0.0,
            uv_scale: [1.0, 1.0],
            wet_bias: 0.0,
            tint: [0.5, 1.0, 1.0],
            noise_mix: [1.0, 0.0],
        };
        let a = h.albedo([0.8, 0.4, 0.2]);
        assert!(
            (a[0] - 0.4).abs() < 0.005 && (a[1] - 0.4).abs() < 0.005,
            "{a:?}"
        );
        assert!((hair_reflectance(1.0) - 0.8f32.powf(2.2)).abs() < 1e-6);
        assert_eq!(hair_reflectance(0.0), 0.0);
    }
}
