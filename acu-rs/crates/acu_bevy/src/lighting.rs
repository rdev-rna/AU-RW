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
//! Still synthetic: sun direction, sun / sky illuminance and colour (the time-of-day
//! manager computes them; not decoded), ambient fill (baked GI is not decoded: the GICP
//! blob codec is unknown), the clear colour, SSAO (decoded, not applied).

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
            "synthetic": ["sun direction", "sun illuminance/colour", "ambient fill (GI undecoded)",
                          "clear colour"],
        }),
    }
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
    // SYNTHETIC magnitudes on a physical scale, so the native exposure has something
    // sensible to expose: Bevy's direct-sunlight constant and a sky-like fill.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.70, 0.81, 1.0),
        brightness: 4_000.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.88),
            illuminance: lux::DIRECT_SUNLIGHT,
            shadow_maps_enabled: true,
            ..default()
        },
        cascades,
        Transform::from_rotation(sun_rotation),
    ));
    n.ambiance.hdr.ev100().map(|ev100| Exposure { ev100 })
}
