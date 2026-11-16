//! Ambient occlusion at max settings: NVIDIA HBAO+ as ACU drives it (finding ACU-RE-0105).
//!
//! ACU ships the SDK as `GFSDK_SSAO.win64.dll` (SHA-256 `3194ccc5…b1f3`, image base
//! `0x180000000`, analysed in IDA on a local copy). Its renderer's `RenderAO` (vtable RVA
//! `0x9D4B8` slot 2, `0x180010880`) receives `Parameters` in r9. `acu_hbao_probe.dll` logged
//! them live at max quality (ACU pid 8324, 5,851 calls, all identical): [`HbaoParams::ACU_MAX`].
//!
//! The constants follow the DLL's own `0x1800085A0` (called from `SetAOParameters`
//! `0x18000CA80`); the field names are the public HBAO+ header's and are INFERRED from the
//! clamps applied to each offset, not from symbols. This SDK revision is older than the 3.x one
//! For Honor links: two amounts (`DetailAO`, `CoarseAO`, each `[0, 2]`) instead of
//! `SmallScaleAO` / `LargeScaleAO`, no foreground / background AO, and a radius-8 blur.

/// `GFSDK_SSAO_Parameters` as ACU passes it (offsets in the raw 0x3C bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HbaoParams {
    /// `+0x00` Radius (world units; ACU's `MetersToViewSpaceUnits` reads 1.0, INFERRED).
    pub radius: f32,
    /// `+0x04` Bias, clamped to `[0, 1]`.
    pub bias: f32,
    /// `+0x08` DetailAO, clamped to `[0, 2]`: weights the full-resolution 4-tap term.
    pub detail_ao: f32,
    /// `+0x0C` CoarseAO, clamped to `[0, 2]`: weights the 8 x 4 coarse samples.
    pub coarse_ao: f32,
    /// `+0x10` PowerExponent, floored at 0.
    pub power: f32,
    /// `+0x14..+0x1C` DepthThreshold {Enable, MaxViewDepth, Sharpness}.
    pub depth_threshold: Option<(f32, f32)>,
    /// `+0x20..+0x28` Blur {Enable, Radius enum, Sharpness}.
    pub blur: Option<Blur>,
    /// `+0x2C..+0x38` Blur.SharpnessProfile {Enable, ForegroundSharpnessScale,
    /// ForegroundViewDepth, BackgroundViewDepth} (INFERRED layout; ACU leaves it disabled).
    pub sharpness_profile: Option<[f32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blur {
    /// Kernel radius in pixels (2, 4 or 8).
    pub radius: u32,
    pub sharpness: f32,
}

/// Size of the raw `Parameters` prefix this module reads.
pub const PARAMS_LEN: usize = 0x3C;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Short(usize),
    BlurRadius(u32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Short(n) => write!(f, "HBAO+ parameters need {PARAMS_LEN} bytes, got {n}"),
            Error::BlurRadius(e) => write!(f, "unsupported HBAO+ blur radius enum {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// The blur radius enum: the DLL holds blur pixel shaders for exactly radii 2, 4 and 8 (falloff
/// literals `2 / (r + 1)^2` = 2/9, 2/25, 2/81). Enum order 0, 1, 2 is INFERRED from the header
/// order of `GFSDK_SSAO_BLUR_RADIUS_*`.
pub fn blur_radius(e: u32) -> Result<u32, Error> {
    match e {
        0 => Ok(2),
        1 => Ok(4),
        2 => Ok(8),
        _ => Err(Error::BlurRadius(e)),
    }
}

impl HbaoParams {
    /// The values ACU passed at max quality (`local/native-probe/hbao-max-20261116`).
    pub const ACU_MAX: HbaoParams = HbaoParams {
        radius: 0.8,
        bias: 0.01,
        detail_ao: 1.34,
        coarse_ao: 1.09,
        power: 2.4,
        depth_threshold: None,
        blur: Some(Blur {
            radius: 8,
            sharpness: 8.0,
        }),
        sharpness_profile: None,
    };

    /// Reads the raw `Parameters` bytes (little-endian) as the probe logs them.
    pub fn parse(b: &[u8]) -> Result<Self, Error> {
        if b.len() < PARAMS_LEN {
            return Err(Error::Short(b.len()));
        }
        let f = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        Ok(Self {
            radius: f(0x00),
            bias: f(0x04),
            detail_ao: f(0x08),
            coarse_ao: f(0x0C),
            power: f(0x10),
            depth_threshold: (u(0x14) != 0).then(|| (f(0x18), f(0x1C))),
            blur: if u(0x20) != 0 {
                Some(Blur {
                    radius: blur_radius(u(0x24))?,
                    sharpness: f(0x28),
                })
            } else {
                None
            },
            sharpness_profile: (u(0x2C) != 0).then(|| [f(0x30), f(0x34), f(0x38)]),
        })
    }

    /// The constant buffer `0x1800085A0` derives, for a viewport `height` pixels tall with
    /// vertical field of view `2 atan(tan_half_fovy)`. Which input-struct fields are the
    /// tangent and the height is INFERRED (`radius * 0.5 / a[13] * a[8]`, as For Honor's SDK).
    pub fn constants(&self, tan_half_fovy: f32, height: f32, meters_to_view: f32) -> HbaoConstants {
        let r = self.radius.max(1e-5) * meters_to_view;
        let bias = self.bias.clamp(0.0, 1.0);
        let k = 1.0 / (1.0 - bias);
        let (inv_threshold, threshold_sharpness) = match self.depth_threshold {
            Some((depth, sharpness)) => (-1.0 / depth.max(1e-6), sharpness.max(0.0)),
            None => (0.0, 1.0),
        };
        HbaoConstants {
            radius_to_screen: r * 0.5 / tan_half_fovy * height,
            r2: r * r,
            neg_inv_r2: -1.0 / (r * r),
            ndotv_bias: bias,
            detail_amount: self.detail_ao.clamp(0.0, 2.0) * k,
            coarse_amount: self.coarse_ao.clamp(0.0, 2.0) * k,
            power: self.power.max(0.0),
            depth_threshold: [inv_threshold, threshold_sharpness],
            blur_sharpness: self
                .blur
                .map_or(0.0, |b| b.sharpness.max(0.0) / meters_to_view),
            blur_falloff: self
                .blur
                .map_or(0.0, |b| 2.0 / (b.radius as f32 + 1.0).powi(2)),
        }
    }
}

/// `0x1800085A0`'s outputs (float index into the SDK's global constant buffer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HbaoConstants {
    /// `[8]` (`cb0[2].x` of CoarseAO): radius in pixels at view depth 1. A pixel whose
    /// `radius_to_screen / z` is below one texel gets no occlusion.
    pub radius_to_screen: f32,
    /// `[9]` R^2 and `[10]` -1 / R^2 (falloff `saturate(1 - d^2 / R^2)`).
    pub r2: f32,
    pub neg_inv_r2: f32,
    /// `[11]` the N.V bias.
    pub ndotv_bias: f32,
    /// `[12]` DetailAO / (1 - bias) and `[13]` CoarseAO / (1 - bias).
    pub detail_amount: f32,
    pub coarse_amount: f32,
    /// `[14]` PowerExponent, applied when the 16 layers are reinterleaved.
    pub power: f32,
    /// `[26]`, `[27]`: depth threshold (`0, 1` when disabled, as ACU runs it).
    pub depth_threshold: [f32; 2],
    /// The blur's depth sharpness and its `r^2` coefficient `2 / (radius + 1)^2`.
    pub blur_sharpness: f32,
    pub blur_falloff: f32,
}

/// The coarse kernel (CoarseAO DXBC at DLL file offset `0x7B410` and its 5 siblings): 8
/// directions 45 degrees apart, 4 steps each of `radius_px / 4 / 5` on the quarter-resolution
/// grid, jittered per layer.
pub const DIRECTIONS: u32 = 8;
pub const STEPS: u32 = 4;
/// The detail term (normal pass at DLL file offset `0x717B0`): 4 full-resolution taps at
/// +-3 pixels in x and y, averaged, stored in the normal target's alpha. CoarseAO adds it as
/// `detail * DetailAO * 8` before the shared `1 / 16` normalisation.
pub const DETAIL_TAP: i32 = 3;
pub const DETAIL_WEIGHT: f32 = 8.0;
pub const NORMALISE: f32 = 1.0 / 16.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(p: &HbaoParams, blur_enum: u32) -> Vec<u8> {
        let mut b = Vec::new();
        for v in [p.radius, p.bias, p.detail_ao, p.coarse_ao, p.power] {
            b.extend(v.to_le_bytes());
        }
        b.extend(0u32.to_le_bytes());
        b.extend(60f32.to_le_bytes());
        b.extend(100f32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(blur_enum.to_le_bytes());
        b.extend(8f32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        for v in [4.0f32, 0.5, 1.0] {
            b.extend(v.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_the_live_layout() {
        let b = raw(&HbaoParams::ACU_MAX, 2);
        assert_eq!(b.len(), PARAMS_LEN);
        assert_eq!(HbaoParams::parse(&b).unwrap(), HbaoParams::ACU_MAX);
    }

    #[test]
    fn rejects_short_input_and_unknown_blur() {
        assert_eq!(HbaoParams::parse(&[0; 8]), Err(Error::Short(8)));
        let b = raw(&HbaoParams::ACU_MAX, 3);
        assert_eq!(HbaoParams::parse(&b), Err(Error::BlurRadius(3)));
    }

    #[test]
    fn constants_follow_0x1800085a0() {
        // 60 degree vertical fov at 1080 px: tan(30 deg) = 0.57735.
        let c = HbaoParams::ACU_MAX.constants(0.577_350_3, 1080.0, 1.0);
        assert!((c.radius_to_screen - 0.8 * 0.5 / 0.577_350_3 * 1080.0).abs() < 1e-3);
        assert!((c.r2 - 0.64).abs() < 1e-6);
        assert!((c.neg_inv_r2 + 1.0 / 0.64).abs() < 1e-5);
        let k = 1.0 / 0.99;
        assert!((c.detail_amount - 1.34 * k).abs() < 1e-6);
        assert!((c.coarse_amount - 1.09 * k).abs() < 1e-6);
        assert_eq!((c.power, c.depth_threshold), (2.4, [0.0, 1.0]));
        assert!((c.blur_falloff - 2.0 / 81.0).abs() < 1e-8);
        assert_eq!(c.blur_sharpness, 8.0);
    }

    #[test]
    fn constants_clamp_like_the_dll() {
        let mut p = HbaoParams::ACU_MAX;
        (p.bias, p.detail_ao, p.coarse_ao, p.power) = (-0.5, 3.0, -1.0, -2.0);
        p.depth_threshold = Some((0.0, -4.0));
        let c = p.constants(1.0, 100.0, 1.0);
        assert_eq!(
            (c.ndotv_bias, c.detail_amount, c.coarse_amount, c.power),
            (0.0, 2.0, 0.0, 0.0)
        );
        assert_eq!(c.depth_threshold, [-1.0e6, 0.0]);
    }

    #[test]
    fn blur_radius_enum_matches_the_shader_families() {
        assert_eq!([0, 1, 2].map(|e| blur_radius(e).unwrap()), [2, 4, 8]);
    }
}
