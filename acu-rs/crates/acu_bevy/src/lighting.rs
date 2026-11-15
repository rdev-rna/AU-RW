//! Scene lighting (ACU-RE-0073): native Paris `WorldAmbiance` settings where decoded,
//! synthetic stand-ins elsewhere.
//!
//! Native (decoded by `acu_assets::ambiance`, Paris entries 22301-22304):
//! - camera exposure: `HDRLightingSettings` aperture / shutter / `ISO` -> EV100
//!   (the field roles of aperture and shutter are INFERRED, `ISO` is a reflected name);
//! - directional shadows: map size `+3356` (INFERRED) and the four PSSM split distances
//!   `+3396` (native consumer `0x1421F9030`; named by the user's HardShadowPatch plugin).
//!   `PSSMShadowEndDistance` (128 m), the split blend regions `+3412` and the per-cascade
//!   biases `+3380..` are reported, not applied: Bevy has one overlap proportion and one
//!   depth bias per light, and ACU's bias units are not established.
//!
//! - baked GI (bounce): native Paris irradiance cells around Arno (`gi_volume`);
//! - sun colour and direction: a live `g_Frame` sample; the sun / GI ratio is native.
//!
//! Still synthetic: the lux-per-ACU-unit scale, the sky fill (ACU's sky SH is a runtime
//! constant not captured), the clear colour, SSAO (decoded, not applied).

use acu_assets::{Limits, ambiance, forge::Forge, package};
use bevy::camera::Exposure;
use bevy::light::{CascadeShadowConfig, DirectionalLightShadowMap, light_consts::lux};
use bevy::prelude::*;
use std::path::Path;

/// `PAR_WorldAmbiance_*` resources in `DataPC_ACU_Paris.forge`: (name, entry, file id).
const PARIS: [(&str, usize, &str); 4] = [
    ("paris-dawn", 22301, "00000007f5f1bb7b"),
    ("paris-night", 22302, "00000007f5f1bb9d"),
    ("paris-sunset", 22303, "00000007f5f1bbbf"),
    ("paris-noon", 22304, "00000007f5f1bbe1"),
];

pub const PRESETS: &str = "synthetic|paris-dawn|paris-noon|paris-sunset|paris-night";

pub struct Native {
    pub preset: &'static str,
    pub entry: usize,
    pub file_id: &'static str,
    pub sha256: String,
    pub ambiance: ambiance::WorldAmbiance,
}

/// Decode the named Paris preset; `Ok(None)` for `synthetic`.
pub fn load(game_dir: &Path, preset: &str) -> acu_assets::Result<Option<Native>> {
    if preset == "synthetic" {
        return Ok(None);
    }
    let &(name, entry, file_id) = PARIS
        .iter()
        .find(|p| p.0 == preset)
        .ok_or_else(|| format!("unknown lighting preset {preset:?}; expected {PRESETS}"))?;
    let mut forge = Forge::open(game_dir.join("DataPC_ACU_Paris.forge"), Limits::default())?;
    let p = package::decode(&forge.raw(entry)?, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == file_id)
        .ok_or_else(|| format!("{file_id} not in Paris entry {entry}"))?;
    let bytes = &p.bytes[r.payload.clone()];
    Ok(Some(Native {
        preset: name,
        entry,
        file_id,
        sha256: acu_assets::sha256(bytes),
        ambiance: ambiance::decode(bytes)?,
    }))
}

/// JSON provenance for the scene report.
pub fn report(native: Option<&Native>) -> serde_json::Value {
    match native {
        None => serde_json::json!({
            "preset": "synthetic",
            "note": "Synthetic daylight/PBR, not native Day Paris",
        }),
        Some(n) => serde_json::json!({
            "preset": n.preset,
            "source": {"archive": "DataPC_ACU_Paris.forge", "entry": n.entry,
                       "file_id": n.file_id, "payload_sha256": n.sha256},
            "applied": {
                "ev100": n.ambiance.hdr.ev100(),
                "shadow_map_size": n.ambiance.shadow.value3356,
                "pssm_split_distances_m": n.ambiance.shadow.cascades3396,
            },
            "decoded_not_applied": {
                "pssm_shadow_end_distance": n.ambiance.shadow.pssm_shadow_end_distance,
                "pssm_split_weight": n.ambiance.shadow.pssm_split_weight,
                "pssm_split_blend_regions_m": n.ambiance.shadow.cascades3412,
                "pssm_cascade_biases": &n.ambiance.shadow.values3376[1..],
                "hdr_value132": n.ambiance.hdr.value132,
                "ssao": &n.ambiance.ssao,
            },
            "live_sun": {"direction_native": LIVE_SUN_DIRECTION, "sun_color_acu": LIVE_SUN_COLOR,
                         "source": "g_Frame read live 2026-11-15 (pinned save's time of day)"},
            "lux_per_acu_unit_synthetic": lux_per_acu_unit(),
            "synthetic": ["lux per ACU unit (sun luminance = Bevy DIRECT_SUNLIGHT)",
                          "sky fill (sky SH not captured)", "clear colour"],
        }),
    }
}

/// SYNTHETIC sky fill: ACU adds an L2 sky SH times the GI sky visibility; the sky SH is a
/// runtime constant not captured yet, so a sky-like ambient stands in for it.
const SYNTHETIC_SKY_FILL: f32 = 4_000.0;

/// Live `g_Frame` sample (ACU-RE-0073): read-only, ACU pid 101640, 2026-11-15, Paris at
/// (496.663, -7.305, -0.253) from the pinned save `capture-start-1` (its time of day; the Season
/// was not identified). `m_SunLight.m_Direction` (towards the sun, native Z-up) and
/// `m_VolumetricFog.SunColor` (`m_SunLight.m_Color` = SunColor / pi). `m_GlobalLightingScale` =
/// (1, 1, 0, 0). Local copy: `local/lighting/live/g_frame-live-1.json`.
pub const LIVE_SUN_DIRECTION: [f32; 3] = [0.700_562, 0.509_13, 0.5];
pub const LIVE_SUN_COLOR: [f32; 3] = [44.624_485, 20.215_647, 3.637_588];

fn live_sun_luminance() -> f32 {
    0.2126 * LIVE_SUN_COLOR[0] + 0.7152 * LIVE_SUN_COLOR[1] + 0.0722 * LIVE_SUN_COLOR[2]
}

/// SYNTHETIC: Bevy lux per ACU lighting unit, chosen so the live sun has Bevy's
/// `DIRECT_SUNLIGHT` luminance. Native: the sun / GI ratio. ACU's deferred pass shades the sun as
/// `albedo * SunColor/pi * NdotL` and GI as `albedo * (dot(L1, n) + L0)`, so the same factor maps
/// SunColor to lux and GI texels to Bevy irradiance-volume units.
pub fn lux_per_acu_unit() -> f32 {
    lux::DIRECT_SUNLIGHT / live_sun_luminance()
}

/// GI irradiance-volume intensity on the native sun / GI ratio.
pub fn gi_intensity() -> f32 {
    lux_per_acu_unit()
}

/// Spawn the lights and return the camera exposure to attach.
pub fn spawn(commands: &mut Commands, native: Option<&Native>) -> Option<Exposure> {
    let sun_rotation = Quat::from_euler(EulerRot::YXZ, 0.6, -0.9, 0.0);
    let fill_rotation = Quat::from_euler(EulerRot::YXZ, -2.4, -0.4, 0.0);
    let Some(n) = native else {
        // Synthetic: overcast sun plus a soft ambient fill so grey reads clearly.
        commands.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.70, 0.81, 1.0),
            brightness: 550.0,
            ..default()
        });
        commands.spawn((
            DirectionalLight {
                color: Color::srgb(1.0, 0.91, 0.78),
                illuminance: 18_000.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_rotation(sun_rotation),
        ));
        commands.spawn((
            DirectionalLight {
                illuminance: 2_000.0,
                ..default()
            },
            Transform::from_rotation(fill_rotation),
        ));
        return None;
    };
    let shadow = &n.ambiance.shadow;
    if shadow.value3356.is_power_of_two() {
        commands.insert_resource(DirectionalLightShadowMap {
            size: shadow.value3356 as usize,
        });
    }
    let bounds = shadow.cascades3396;
    let cascades = if bounds.windows(2).all(|w| w[0] > 0.0 && w[1] > w[0]) {
        CascadeShadowConfig {
            bounds: bounds.to_vec(),
            ..default()
        }
    } else {
        CascadeShadowConfig::default()
    };
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.70, 0.81, 1.0),
        brightness: SYNTHETIC_SKY_FILL,
        ..default()
    });
    // Live native sun: colour and direction; magnitude on the synthetic lux scale.
    let max = LIVE_SUN_COLOR.iter().copied().fold(f32::MIN, f32::max);
    let [x, y, z] = LIVE_SUN_DIRECTION;
    let towards_sun = crate::native_z_up_to_bevy_y_up() * Vec3::new(x, y, z).normalize();
    commands.spawn((
        DirectionalLight {
            color: Color::linear_rgb(
                LIVE_SUN_COLOR[0] / max,
                LIVE_SUN_COLOR[1] / max,
                LIVE_SUN_COLOR[2] / max,
            ),
            illuminance: lux_per_acu_unit() * max,
            shadow_maps_enabled: true,
            ..default()
        },
        cascades,
        Transform::default().looking_to(-towards_sun, Vec3::Y),
    ));
    n.ambiance.hdr.ev100().map(|ev100| Exposure { ev100 })
}
