//! Native Paris baked GI around Arno (ACU-RE-0073): the `GIResult` cells of the time-of-day
//! `GILoader` that surround a real Paris spot, decoded with `acu_assets::gicp`, resampled into one
//! Bevy `IrradianceVolume` (1 m voxels) placed so the spot lands on Arno at the arena origin.
//!
//! Native: which cells, their bounds and levels, and every cell voxel's L0 / L1 colour (GICP +
//! `GIDecodeYCbCr`). Observed in the deferred lighting shader (`0x143211F50`): irradiance per
//! colour is `dot(L1, n) + L0` with `n` the world normal and atlas u, v, w = world x, y, z; the
//! volumes hold bounce light only (sky light = an L2 sky SH times GIData 3's visibility, not
//! ported: the sky SH is a runtime constant).
//! Inferred: texel centres at `min + (i + 0.5) * size / 32` (the native `UnitToVolume` inset is a
//! runtime value), and "finest containing leaf wins" (the native indirection grid is built at
//! runtime).
//! Synthetic: the arena, the resampling grid, and the `intensity` calibration (ACU's GI scale
//! relative to exposure is unknown).

use std::collections::BTreeMap;
use std::path::Path;

use acu_assets::gi::{self, GiLoader, TreeNode};
use acu_assets::{Limits, forge::Forge, gicp, package};
use bevy::asset::RenderAssetUsages;
use bevy::light::{IrradianceVolume, LightProbe};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Arno's live position in Paris (ACU pid 126620, `teleport.py where`, 2026-11-15): native metres.
pub const ANCHOR: [f32; 3] = [496.664, -7.308, -0.252];
/// Resampled region relative to the anchor (native metres) and its voxel counts (1 m voxels).
const REGION_MIN: [f32; 3] = [-12.0, -12.0, -2.0];
const REGION_VOXELS: [usize; 3] = [24, 24, 8];
/// Coarsest tree level gathered (level 0 = 8 m leaves, each level doubles; Paris goes to 9).
/// Fine leaves stop at the street surface, so the air above is only covered by coarse levels.
const MAX_LEVEL: u32 = 9;
/// GI output grid per cell: 32, the live `g_Frame.m_GIConsts.InvVolumeSize` (1/32).
const GRID: u32 = 32;
/// `GILoader` resources in `DataPC_ACU_Paris.forge` entry 21267, by lighting preset.
const LOADER_ENTRY: usize = 21267;
const LOADERS: [(&str, &str); 4] = [
    ("paris-night", "0000003a66d8e3b4"),
    ("paris-dawn", "0000003a66d8e3b5"),
    ("paris-noon", "0000003a66d8e3b6"),
    ("paris-sunset", "0000003a66d8e3b7"),
];

pub struct Cell {
    pub min: [f32; 3],
    pub size: f32,
    pub level: u32,
    pub datablock: u64,
    pub result: u64,
    pub texels: Vec<gicp::DecodedTexel>,
}

impl Cell {
    fn contains(&self, p: [f32; 3]) -> bool {
        (0..3).all(|i| self.min[i] <= p[i] && p[i] < self.min[i] + self.size)
    }

    /// Trilinear L1/L0 per colour at native point `p` inside the cell.
    fn sample(&self, p: [f32; 3]) -> [[f32; 4]; 3] {
        let g = GRID as usize;
        let f = |i: usize| {
            ((p[i] - self.min[i]) / self.size * GRID as f32 - 0.5).clamp(0.0, (GRID - 1) as f32)
        };
        let t = [f(0), f(1), f(2)];
        let lo = t.map(|v| v.floor() as usize);
        let w = [
            t[0] - lo[0] as f32,
            t[1] - lo[1] as f32,
            t[2] - lo[2] as f32,
        ];
        let mut out = [[0f32; 4]; 3];
        for corner in 0..8 {
            let k = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
            let weight: f32 = (0..3)
                .map(|a| if k[a] == 1 { w[a] } else { 1.0 - w[a] })
                .product();
            if weight == 0.0 {
                continue;
            }
            let at = |a: usize| (lo[a] + k[a]).min(g - 1);
            let texel = &self.texels[at(0) + g * (at(1) + g * at(2))];
            for (row, src) in out.iter_mut().zip(&texel.rgb) {
                for (o, s) in row.iter_mut().zip(src) {
                    *o += weight * s;
                }
            }
        }
        out
    }
}

pub struct NativeGi {
    pub loader: &'static str,
    pub season: u64,
    pub cells: Vec<Cell>,
    /// Resampled region, x fastest then y then z (native axes): L1 xyz + L0 per colour.
    pub voxels: Vec<[[f32; 4]; 3]>,
    /// Region voxels outside every gathered cell.
    pub uncovered: usize,
    /// Median L0 luminance over region voxels with positive L0 (native GI units).
    pub median_luminance: f32,
}

fn payload(forge: &mut Forge, entry: usize, file_id: &str) -> acu_assets::Result<Vec<u8>> {
    let p = package::decode(&forge.raw(entry)?, Limits::default())?;
    let r = p
        .resources
        .iter()
        .find(|r| r.file_id == file_id)
        .ok_or_else(|| format!("{file_id} not in entry {entry}"))?;
    Ok(p.bytes[r.payload.clone()].to_vec())
}

fn region_bounds() -> ([f32; 3], [f32; 3]) {
    let lo: [f32; 3] = std::array::from_fn(|i| ANCHOR[i] + REGION_MIN[i]);
    let hi: [f32; 3] = std::array::from_fn(|i| lo[i] + REGION_VOXELS[i] as f32);
    (lo, hi)
}

fn touches_region(n: &TreeNode) -> bool {
    let (lo, hi) = region_bounds();
    (0..3).all(|i| n.min[i] < hi[i] && n.min[i] + n.size[i] > lo[i])
}

fn luminance(v: &[[f32; 4]; 3]) -> f32 {
    0.2126 * v[0][3] + 0.7152 * v[1][3] + 0.0722 * v[2][3]
}

/// Decode and resample the cells for `preset` (`Ok(None)` for presets without a Paris loader).
pub fn load(game_dir: &Path, preset: &str) -> acu_assets::Result<Option<NativeGi>> {
    let Some(&(_, loader_id)) = LOADERS.iter().find(|l| l.0 == preset) else {
        return Ok(None);
    };
    let mut paris = Forge::open(game_dir.join("DataPC_ACU_Paris.forge"), Limits::default())?;
    let loader: GiLoader = gi::decode_loader(&payload(&mut paris, LOADER_ENTRY, loader_id)?)?;
    let leaves: Vec<&TreeNode> = loader
        .leaves()
        .into_iter()
        .filter(|n| {
            n.level <= MAX_LEVEL
                && n.size == [8.0 * (1u32 << n.level) as f32; 3]
                && n.result.id != 0
                && touches_region(n)
        })
        .collect();
    let mut by_block: BTreeMap<u64, Vec<&TreeNode>> = BTreeMap::new();
    for n in leaves {
        by_block.entry(n.datablock.id).or_default().push(n);
    }
    let mut gi_forge = Forge::open(
        game_dir.join("DataPC_ACU_Paris_gi.forge"),
        Limits::default(),
    )?;
    let mut cells = Vec::new();
    for (block, nodes) in by_block {
        let fid = gi::hex16(block);
        let entry = gi_forge
            .entries
            .iter()
            .find(|e| e.file_id == fid)
            .map(|e| e.index)
            .ok_or_else(|| format!("GIDatablock {fid} not in DataPC_ACU_Paris_gi.forge"))?;
        let d = gi::decode_datablock(&payload(&mut gi_forge, entry, &fid)?)?;
        for n in nodes {
            let r = d
                .results
                .iter()
                .find(|r| r.object_id == n.result.id)
                .ok_or_else(|| format!("GIResult {} not in {fid}", gi::hex16(n.result.id)))?;
            if r.center[..3] != n.min {
                return Err(
                    format!("GIResult {} Center != tree min", gi::hex16(r.object_id)).into(),
                );
            }
            cells.push(Cell {
                min: n.min,
                size: n.size[0],
                level: n.level,
                datablock: block,
                result: n.result.id,
                texels: gicp::decode_cell(&r.data, GRID)?,
            });
        }
    }
    cells.sort_by_key(|c| c.level);
    let (lo, _) = region_bounds();
    let [nx, ny, nz] = REGION_VOXELS;
    let mut voxels = Vec::with_capacity(nx * ny * nz);
    let mut uncovered = 0;
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let p = [
                    lo[0] + x as f32 + 0.5,
                    lo[1] + y as f32 + 0.5,
                    lo[2] + z as f32 + 0.5,
                ];
                match cells.iter().find(|c| c.contains(p)) {
                    Some(c) => voxels.push(c.sample(p)),
                    None => {
                        uncovered += 1;
                        voxels.push([[0.0; 4]; 3]);
                    }
                }
            }
        }
    }
    let mut lit: Vec<f32> = voxels.iter().map(luminance).filter(|&l| l > 0.0).collect();
    lit.sort_by(f32::total_cmp);
    let median_luminance = lit.get(lit.len() / 2).copied().unwrap_or(0.0);
    Ok(Some(NativeGi {
        loader: loader_id,
        season: loader.season,
        cells,
        voxels,
        uncovered,
        median_luminance,
    }))
}

/// f32 -> IEEE half bits, round to nearest even.
fn f16_bits(v: f32) -> u16 {
    let sign = ((v.to_bits() >> 16) & 0x8000) as u16;
    let a = v.abs();
    if a.is_nan() {
        return sign | 0x7e00;
    }
    if a >= 65520.0 {
        return sign | 0x7c00;
    }
    if a < 6.103_515_6e-5 {
        return sign | (a / 5.960_464_5e-8).round() as u16;
    }
    let bits = a.to_bits();
    let exponent = ((bits >> 23) as i32 - 127 + 15) as u32;
    let mantissa = bits & 0x7f_ffff;
    let mut h = (exponent << 10) | (mantissa >> 13);
    let rest = mantissa & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && h & 1 == 1) {
        h += 1;
    }
    sign | h as u16
}

/// Bevy ambient-cube texture `(Rx, 2Ry, 3Rz)` in Bevy axes. The presentation rotation maps native
/// x -> Bevy X, native z -> Bevy Y, native -y -> Bevy Z.
fn ambient_cube(gi: &NativeGi) -> Image {
    let [nx, ny, nz] = REGION_VOXELS;
    let (rx, ry, rz) = (nx, nz, ny);
    let mut data = vec![0u8; rx * 2 * ry * 3 * rz * 8];
    for bz in 0..rz {
        for by in 0..ry {
            for bx in 0..rx {
                let (x, y, z) = (bx, ny - 1 - bz, by);
                let v = &gi.voxels[x + nx * (y + ny * z)];
                // (Bevy side, positive?, native axis, sign along that native axis)
                for (side, positive, axis, sign) in [
                    (0, false, 0, -1.0),
                    (0, true, 0, 1.0),
                    (1, false, 2, -1.0),
                    (1, true, 2, 1.0),
                    (2, false, 1, 1.0),
                    (2, true, 1, -1.0),
                ] {
                    let t = by + if positive { ry } else { 0 };
                    let p = bz + side * rz;
                    let at = ((p * 2 * ry + t) * rx + bx) * 8;
                    for c in 0..3 {
                        let e = (v[c][3] + sign * v[c][axis]).max(0.0);
                        data[at + 2 * c..at + 2 * c + 2]
                            .copy_from_slice(&f16_bits(e).to_le_bytes());
                    }
                    data[at + 6..at + 8].copy_from_slice(&f16_bits(1.0).to_le_bytes());
                }
            }
        }
    }
    Image::new(
        Extent3d {
            width: rx as u32,
            height: 2 * ry as u32,
            depth_or_array_layers: 3 * rz as u32,
        },
        TextureDimension::D3,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Spawn the irradiance volume, anchored so `ANCHOR` sits at the arena origin.
pub fn spawn(commands: &mut Commands, images: &mut Assets<Image>, gi: &NativeGi, intensity: f32) {
    let size = Vec3::from_array(REGION_VOXELS.map(|v| v as f32));
    let center = Vec3::from_array(REGION_MIN) + size / 2.0;
    let rotation = crate::native_z_up_to_bevy_y_up();
    commands.spawn((
        LightProbe::default(),
        IrradianceVolume {
            voxels: images.add(ambient_cube(gi)),
            intensity,
            affects_lightmapped_meshes: true,
        },
        Transform::from_translation(rotation * center)
            .with_scale(Vec3::new(size.x, size.z, size.y)),
    ));
}

pub fn report(gi: Option<&NativeGi>, intensity: f32) -> serde_json::Value {
    match gi {
        None => serde_json::json!({"applied": false}),
        Some(g) => serde_json::json!({
            "applied": true,
            "loader": {"archive": "DataPC_ACU_Paris.forge", "entry": LOADER_ENTRY,
                       "file_id": g.loader, "season": gi::hex16(g.season)},
            "anchor_native_m": ANCHOR,
            "region_native_m": {"min_rel": REGION_MIN, "voxels_1m": REGION_VOXELS},
            "cells": g.cells.iter().map(|c| serde_json::json!({
                "min": c.min, "size": c.size, "level": c.level,
                "datablock": gi::hex16(c.datablock), "result": gi::hex16(c.result)}))
                .collect::<Vec<_>>(),
            "uncovered_voxels": g.uncovered,
            "median_l0_luminance": g.median_luminance,
            "intensity_synthetic": intensity,
            "observed": ["bounce irradiance dot(L1,n)+L0 per colour, world axes (0x143211F50)"],
            "inferred": ["texel centres from the cell min corner", "finest containing leaf wins"],
            "missing": ["sky SH x GIData 3 visibility (runtime constant)"],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, GRID, f16_bits};
    use acu_assets::gicp::DecodedTexel;

    #[test]
    fn half_conversion() {
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(-2.0), 0xc000);
        assert_eq!(f16_bits(65504.0), 0x7bff);
        assert_eq!(f16_bits(1e6), 0x7c00);
        assert_eq!(f16_bits(5.960_464_5e-8), 1);
        assert_eq!(f16_bits(0.333_251_95), 0x3555);
    }

    #[test]
    fn cell_sampling_hits_texel_centres_and_interpolates() {
        let g = GRID as usize;
        let texels = (0..g * g * g)
            .map(|i| {
                let x = (i % g) as f32;
                DecodedTexel {
                    rgb: [[x, 0.0, 0.0, 1.0]; 3],
                    extra: [0.0; 4],
                }
            })
            .collect();
        let c = Cell {
            min: [0.0; 3],
            size: GRID as f32,
            level: 1,
            datablock: 0,
            result: 0,
            texels,
        };
        assert_eq!(c.sample([3.5, 8.5, 8.5])[0][0], 3.0);
        assert!((c.sample([4.0, 8.5, 8.5])[1][0] - 3.5).abs() < 1e-6);
        assert_eq!(c.sample([0.1, 0.1, 0.1])[2][0], 0.0, "clamped at the edge");
        let edge = GRID as f32;
        assert!(c.contains([edge - 0.1, 0.0, 0.0]) && !c.contains([edge, 0.0, 0.0]));
    }
}
