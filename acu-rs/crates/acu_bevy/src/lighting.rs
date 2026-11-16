//! Scene lighting (ACU-RE-0073): every native `WorldAmbiance` set in the install where decoded,
//! synthetic stand-ins elsewhere. Default: `paris-day` (`PAR_WorldAmbiance_Noon`, "Day, Paris").
//! `L` / `Shift+L` cycle the sets at runtime.
//!
//! Native (decoded by `acu_assets::ambiance`, sets in `SETS`):
//! - camera exposure: `HDRLightingSettings` aperture / shutter / `ISO` -> EV100
//!   (the field roles of aperture and shutter are INFERRED, `ISO` is a reflected name);
//! - directional shadows: map size `+3356` (INFERRED) and the four PSSM split distances
//!   `+3396` (native consumer `0x1421F9030`; named by the user's HardShadowPatch plugin).
//!   `PSSMShadowEndDistance`, the split blend regions `+3412` and the per-cascade
//!   biases `+3380..` are reported, not applied: Bevy has one overlap proportion and one
//!   depth bias per light, and ACU's bias units are not established.
//!
//! - baked GI (bounce): native Paris irradiance cells around Arno (`gi_volume`), Paris sets only
//!   (the other levels' GI loaders are not wired; Arno's anchor is a Paris street);
//! - sun colour and direction: live `g_Frame` per Paris Season (`PARIS_SUNS`: dawn, day, sunset,
//!   night, read after switching the Season in game; `WorldAmbiance` holds no sun). The Paris
//!   sets use their own; every other set borrows a Paris sun (`sun_source`: Versailles by time of
//!   day, the rest Paris day), labelled SYNTHETIC and scaled by `sun_scale` to its exposure.
//!
//! Still synthetic: the lux-per-ACU-unit scale, the sky fill (ACU's sky SH is a runtime
//! constant not captured), the clear colour, SSAO (decoded, not applied).

use acu_assets::{Limits, ambiance, forge::Forge, package};
use bevy::camera::Exposure;
use bevy::light::{CascadeShadowConfig, DirectionalLightShadowMap, light_consts::lux};
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A `WorldAmbiance` resource: preset name, archive (relative to the install), entry, file id,
/// resource name (asset index, `crc-name`). Patch copies (`*_patch_03`) of the Prologue, Rebirth,
/// Belle Epoque and Medieval sets have byte-identical payloads, so precedence does not matter.
pub struct Set {
    pub preset: &'static str,
    pub archive: &'static str,
    pub entry: usize,
    pub file_id: &'static str,
    pub name: &'static str,
}

const fn set(
    preset: &'static str,
    archive: &'static str,
    entry: usize,
    file_id: &'static str,
    name: &'static str,
) -> Set {
    Set {
        preset,
        archive,
        entry,
        file_id,
        name,
    }
}

const PARIS: &str = "DataPC_ACU_Paris.forge";
const VERSAILLES: &str = "DataPC_ACU_Versailles.forge";
const SAINT_DENIS: &str = "dlc_11/DataPC_ACU-DLC_Saint-Denis.forge";

/// Every unique `WorldAmbiance` in the install (15), cycle order.
pub const SETS: [Set; 15] = [
    set(
        "paris-day",
        PARIS,
        22304,
        "00000007f5f1bbe1",
        "PAR_WorldAmbiance_Noon",
    ),
    set(
        "paris-sunset",
        PARIS,
        22303,
        "00000007f5f1bbbf",
        "PAR_WorldAmbiance_Sunset",
    ),
    set(
        "paris-night",
        PARIS,
        22302,
        "00000007f5f1bb9d",
        "PAR_WorldAmbiance_Night",
    ),
    set(
        "paris-dawn",
        PARIS,
        22301,
        "00000007f5f1bb7b",
        "PAR_WorldAmbiance_Dawn",
    ),
    set(
        "versailles-day",
        VERSAILLES,
        5437,
        "00000010cde129ee",
        "VERS_WorldAmbiance_Noon",
    ),
    set(
        "versailles-sunset",
        VERSAILLES,
        5438,
        "00000010cde12a11",
        "VERS_WorldAmbiance_Sunset",
    ),
    set(
        "versailles-night",
        VERSAILLES,
        5436,
        "00000010cde129cb",
        "VERS_WorldAmbiance_Night",
    ),
    set(
        "versailles-dawn",
        VERSAILLES,
        5435,
        "00000010cde129a8",
        "VERS_WorldAmbiance_Dawn",
    ),
    set(
        "saint-denis",
        SAINT_DENIS,
        1238,
        "00000012f8374555",
        "SDN_WorldAmbiance",
    ),
    set(
        "saint-denis-moon",
        SAINT_DENIS,
        1232,
        "00000013786fcca2",
        "SDN_moon_WorldAmbiance",
    ),
    set(
        "prologue-night",
        "DataPC_ACU_Prologue.forge",
        2463,
        "0000001347aeb35f",
        "PRO_WorldAmbiance_Night",
    ),
    set(
        "rebirth",
        "DataPC_ACU_RebirthRitual.forge",
        501,
        "0000001095d6a63f",
        "REBIRTH_WorldAmbiance_01",
    ),
    set(
        "rift-belle-epoque-sunset",
        "DataPC_ACU_LGS_BelleEpoque.forge",
        348,
        "000000104339fe67",
        "LGS_BEP_WorldAmbiance_Sunset",
    ),
    set(
        "rift-medieval-dawn",
        "DataPC_ACU_LGS_Medieval.forge",
        513,
        "000000144de08bd3",
        "LGS_MDA_WorldAmbiance_Dawn_V2",
    ),
    set(
        "rift-ww2-night",
        "DataPC_ACU_LGS_WorldWar2.forge",
        413,
        "000000110e816c9e",
        "LGS_WWR_WorldAmbiance_Night",
    ),
];

pub const DEFAULT: &str = "paris-day";

/// `--lighting` help: `synthetic`, every set, and the `paris-noon` alias.
pub fn presets() -> String {
    let mut names = vec!["synthetic"];
    names.extend(SETS.iter().map(|s| s.preset));
    format!("{} (alias paris-noon = paris-day)", names.join("|"))
}

/// Canonical preset name (`paris-noon` is the earlier name of `paris-day`).
pub fn canonical(preset: &str) -> &str {
    if preset == "paris-noon" {
        DEFAULT
    } else {
        preset
    }
}

/// Cycle order: every set, then `synthetic`.
fn cycle() -> impl Iterator<Item = &'static str> {
    SETS.iter()
        .map(|s| s.preset)
        .chain(std::iter::once("synthetic"))
}

pub struct Native {
    pub set: &'static Set,
    pub sha256: String,
    pub ambiance: ambiance::WorldAmbiance,
}

/// Decode the named preset; `Ok(None)` for `synthetic`.
pub fn load(game_dir: &Path, preset: &str) -> acu_assets::Result<Option<Native>> {
    let preset = canonical(preset);
    if preset == "synthetic" {
        return Ok(None);
    }
    let set = SETS
        .iter()
        .find(|s| s.preset == preset)
        .ok_or_else(|| format!("unknown lighting preset {preset:?}; expected {}", presets()))?;
    let mut forge = Forge::open(game_dir.join(set.archive), Limits::default())?;
    let p = package::decode(&forge.raw(set.entry)?, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == set.file_id)
        .ok_or_else(|| format!("{} not in {} entry {}", set.file_id, set.archive, set.entry))?;
    let bytes = &p.bytes[r.payload.clone()];
    Ok(Some(Native {
        set,
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
            "preset": n.set.preset,
            "source": {"archive": n.set.archive, "entry": n.set.entry, "name": n.set.name,
                       "file_id": n.set.file_id, "payload_sha256": n.sha256},
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
            "sun": sun_report(n),
            "lux_per_acu_unit_synthetic": lux_per_acu_unit(),
            "synthetic": ["lux per ACU unit (Paris day sun luminance = Bevy DIRECT_SUNLIGHT)",
                          "sun of non-Paris sets (a Paris sun, scaled by 2^(EV100 - 12.94))",
                          "sky fill (sky SH not captured)", "clear colour"],
        }),
    }
}

/// SYNTHETIC sky fill: ACU adds an L2 sky SH times the GI sky visibility; the sky SH is a
/// runtime constant not captured yet, so a sky-like ambient stands in for it.
const SYNTHETIC_SKY_FILL: f32 = 4_000.0;

/// A live `g_Frame` sun: `m_SunLight.m_Direction` (towards the sun, native Z up) and
/// `m_VolumetricFog.SunColor` (`m_SunLight.m_Color` = SunColor / pi).
pub struct LiveSun {
    pub season: &'static str,
    pub season_id: u32,
    pub direction: [f32; 3],
    pub color: [f32; 3],
}

/// Paris suns per Season (ACU-RE-0073): `tools/native_probe/tod_capture.py`, ACU pid 64912,
/// 2026-11-16, pinned save `capture-start-1` (whose own Season is 4, dawn), after
/// `TimeOfDayManager::SetCurrentSeason` (0x141E2F660; lead ACUFixes) and a 20 s settle; nine
/// `g_Frame` copies agree per Season. Dawn equals the 2026-11-15 sample (pid 101640), so the sun is
/// fixed per Season. Night's SunColor is about 6e-6: the night sun is effectively off.
/// Local copies: `local/native-probe/tod-sun-20261116b/tod-sun*.json`.
pub const PARIS_SUNS: [LiveSun; 4] = [
    LiveSun {
        season: "PAR_TOD_Paris_Dawn",
        season_id: 4,
        direction: [0.700_562, 0.509_13, 0.5],
        color: [44.624_485, 20.215_647, 3.637_588],
    },
    LiveSun {
        season: "PAR_TOD_Paris_Noon",
        season_id: 5,
        direction: [0.117_286, -0.470_409, 0.874_62],
        color: [193.858_38, 171.389_68, 152.644_36],
    },
    LiveSun {
        season: "PAR_TOD_Paris_Sunset",
        season_id: 6,
        direction: [-0.601_766, -0.622_798, 0.5],
        color: [40.205_03, 19.384_531, 4.575_61],
    },
    LiveSun {
        season: "PAR_TOD_Paris_Night",
        season_id: 7,
        direction: [0.094_27, 0.581_659, 0.807_952],
        color: [6e-6, 4e-6, 3e-6],
    },
];

const DAWN: usize = 0;
const DAY: usize = 1;
const SUNSET: usize = 2;
const NIGHT: usize = 3;

/// The sun a set uses and whether it is that set's own (native) sun. Versailles borrows the Paris
/// sun of the same time of day; Saint-Denis, the Prologue, Rebirth and the rifts borrow Paris day
/// (SYNTHETIC stand-ins: their Seasons live in other worlds, not captured).
pub fn sun_source(preset: &str) -> (&'static LiveSun, bool) {
    let paris = |i: usize| &PARIS_SUNS[i];
    match preset {
        "paris-dawn" => (paris(DAWN), true),
        "paris-day" => (paris(DAY), true),
        "paris-sunset" => (paris(SUNSET), true),
        "paris-night" => (paris(NIGHT), true),
        p if p.ends_with("-dawn") => (paris(DAWN), false),
        p if p.ends_with("-sunset") => (paris(SUNSET), false),
        p if p.ends_with("-night") => (paris(NIGHT), false),
        _ => (paris(DAY), false),
    }
}

fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// SYNTHETIC: Bevy lux per ACU lighting unit, chosen so the Paris day sun has Bevy's
/// `DIRECT_SUNLIGHT` luminance. Native: the ratios between suns and the sun / GI ratio. ACU's
/// deferred pass shades the sun as `albedo * SunColor/pi * NdotL` and GI as
/// `albedo * (dot(L1, n) + L0)`, so the same factor maps SunColor to lux and GI texels to Bevy
/// irradiance-volume units.
pub fn lux_per_acu_unit() -> f32 {
    lux::DIRECT_SUNLIGHT / luminance(PARIS_SUNS[DAY].color)
}

/// GI irradiance-volume intensity on the native sun / GI ratio.
pub fn gi_intensity() -> f32 {
    lux_per_acu_unit()
}

/// EV100 the Paris suns were captured under (all four Paris sets share it).
const PARIS_EV100: f32 = 12.936_638;

/// Sun / sky-fill scale. 1 for a set's own sun. For a borrowed Paris sun, SYNTHETIC
/// `2^(EV100_set - EV100_Paris)`: a set exposed for a dimmer scene (Saint-Denis moon EV -5.3,
/// Medieval dawn 2.5) would otherwise burn out under a Paris sun.
pub fn sun_scale(n: &Native) -> f32 {
    if sun_source(n.set.preset).1 {
        return 1.0;
    }
    n.ambiance
        .hdr
        .ev100()
        .map_or(1.0, |ev| 2f32.powf(ev - PARIS_EV100))
}

fn sun_report(n: &Native) -> serde_json::Value {
    let (sun, native) = sun_source(n.set.preset);
    serde_json::json!({
        "season": sun.season, "season_id": sun.season_id,
        "direction_native": sun.direction, "sun_color_acu": sun.color,
        "native_for_this_set": native, "scale_synthetic": sun_scale(n),
        "source": "g_Frame live per Paris Season, 2026-11-16 (tod_capture.py)",
    })
}

/// Marks the entities `spawn` creates (and the GI volume), so a preset switch replaces them.
#[derive(Component)]
pub struct SceneLight;

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
        commands.insert_resource(DirectionalLightShadowMap::default());
        commands.spawn((
            SceneLight,
            DirectionalLight {
                color: Color::srgb(1.0, 0.91, 0.78),
                illuminance: 18_000.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_rotation(sun_rotation),
        ));
        commands.spawn((
            SceneLight,
            DirectionalLight {
                illuminance: 2_000.0,
                ..default()
            },
            Transform::from_rotation(fill_rotation),
        ));
        return None;
    };
    let shadow = &n.ambiance.shadow;
    commands.insert_resource(if shadow.value3356.is_power_of_two() {
        DirectionalLightShadowMap {
            size: shadow.value3356 as usize,
        }
    } else {
        DirectionalLightShadowMap::default()
    });
    let bounds = shadow.cascades3396;
    let cascades = if bounds.windows(2).all(|w| w[0] > 0.0 && w[1] > w[0]) {
        CascadeShadowConfig {
            bounds: bounds.to_vec(),
            ..default()
        }
    } else {
        CascadeShadowConfig::default()
    };
    let scale = sun_scale(n);
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.70, 0.81, 1.0),
        brightness: SYNTHETIC_SKY_FILL * scale,
        ..default()
    });
    // Live sun of the set's Season: colour and direction; magnitude on the synthetic lux scale.
    let sun = sun_source(n.set.preset).0;
    let max = sun
        .color
        .iter()
        .copied()
        .fold(f32::MIN, f32::max)
        .max(f32::MIN_POSITIVE);
    let [x, y, z] = sun.direction;
    let towards_sun = crate::native_z_up_to_bevy_y_up() * Vec3::new(x, y, z).normalize();
    commands.spawn((
        SceneLight,
        DirectionalLight {
            color: Color::linear_rgb(sun.color[0] / max, sun.color[1] / max, sun.color[2] / max),
            illuminance: lux_per_acu_unit() * max * scale,
            shadow_maps_enabled: true,
            ..default()
        },
        cascades,
        Transform::default().looking_to(-towards_sun, Vec3::Y),
    ));
    n.ambiance.hdr.ev100().map(|ev100| Exposure { ev100 })
}

type Cached = (Option<Native>, Option<crate::gi_volume::NativeGi>);

/// Runtime preset switching (`L` next, `Shift+L` previous). Decoded sets and resampled GI are
/// cached, so each set is read from its archive once.
#[derive(Resource)]
pub struct Switcher {
    pub game_dir: PathBuf,
    pub current: &'static str,
    cache: HashMap<&'static str, Cached>,
}

impl Switcher {
    pub fn new(game_dir: PathBuf, current: &str) -> Self {
        let current = cycle()
            .find(|p| *p == canonical(current))
            .unwrap_or(DEFAULT);
        Self {
            game_dir,
            current,
            cache: HashMap::new(),
        }
    }
}

/// The preset after (or before) `current` in the cycle.
fn step(current: &str, back: bool) -> &'static str {
    let names: Vec<&'static str> = cycle().collect();
    let at = names.iter().position(|p| *p == current).unwrap_or(0);
    let offset = if back { names.len() - 1 } else { 1 };
    names[(at + offset) % names.len()]
}

pub fn switch_preset(
    keys: Res<ButtonInput<KeyCode>>,
    mut switcher: ResMut<Switcher>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    lights: Query<Entity, With<SceneLight>>,
    cameras: Query<Entity, With<Camera3d>>,
) {
    if !keys.just_pressed(KeyCode::KeyL) {
        return;
    }
    let back = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let next = step(switcher.current, back);
    if !switcher.cache.contains_key(next) {
        let dir = switcher.game_dir.clone();
        let native = load(&dir, next).unwrap_or_else(|e| {
            eprintln!("lighting preset {next:?} failed: {e}");
            None
        });
        let gi = match native {
            Some(_) => crate::gi_volume::load(&dir, next).unwrap_or_else(|e| {
                eprintln!("native GI for {next:?} failed: {e}");
                None
            }),
            None => None,
        };
        switcher.cache.insert(next, (native, gi));
    }
    for entity in &lights {
        commands.entity(entity).despawn();
    }
    let (native, gi) = &switcher.cache[next];
    let exposure = spawn(&mut commands, native.as_ref()).unwrap_or_default();
    if let Some(gi) = gi {
        let volume = crate::gi_volume::spawn(&mut commands, &mut images, gi, gi_intensity());
        commands.entity(volume).insert(SceneLight);
    }
    for camera in &cameras {
        commands.entity(camera).insert(exposure);
    }
    let mut info = report(native.as_ref());
    info["gi"] = crate::gi_volume::report(gi.as_ref(), gi_intensity());
    println!("lighting: {} -> {next}: {info}", switcher.current);
    switcher.current = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_are_unique_and_default_is_paris_day() {
        let names: Vec<_> = cycle().collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
        assert_eq!(SETS[0].preset, DEFAULT);
        assert_eq!(SETS[0].name, "PAR_WorldAmbiance_Noon");
        assert_eq!(canonical("paris-noon"), "paris-day");
        assert!(SETS.iter().all(|s| s.file_id.len() == 16));
    }

    #[test]
    fn paris_sets_use_their_own_sun_and_others_borrow() {
        assert_eq!(sun_source("paris-day").0.season_id, 5);
        assert!(sun_source("paris-night").1);
        assert_eq!(sun_source("versailles-sunset").0.season_id, 6);
        assert!(!sun_source("versailles-sunset").1);
        assert_eq!(sun_source("saint-denis-moon").0.season_id, 5);
        assert!(
            SETS.iter()
                .all(|s| sun_source(s.preset).1 == s.preset.starts_with("paris-"))
        );
        let day = luminance(PARIS_SUNS[DAY].color) * lux_per_acu_unit();
        assert!((day - lux::DIRECT_SUNLIGHT).abs() < 1.0);
    }

    #[test]
    fn cycle_wraps_both_ways() {
        assert_eq!(step("paris-day", false), "paris-sunset");
        assert_eq!(step("paris-day", true), "synthetic");
        assert_eq!(step("synthetic", false), "paris-day");
        assert_eq!(step("rift-ww2-night", false), "synthetic");
    }
}
