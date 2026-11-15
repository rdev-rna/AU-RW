//! Cloth-driven pieces (ACU-RE-0081): the Tailored flap follows ACU's cloth
//! solver (`acu_sim::cloth`) instead of its mesh skin.
//!
//! Per fixed tick, mirroring the native chain:
//! 1. particle targets and normals by particle skinning (`0x141d7a4a0`) with
//!    the character pose;
//! 2. the frame carry (`0x1423b10d0`) for the character's translation;
//! 3. one solver step (`0x1423d4b60`) at the tick's dt;
//! 4. render vertices from particles (`0x141d70840`) and their normal frames
//!    (`0x141d98430`).
//!
//! The simulation frame has native world axes (Z up, so gravity is the
//! record's `(0, 0, -10)`) with the character's position subtracted, as the
//! native instance stores particles relative to the entity translation.
//!
//! Implementation choices, not native evidence: one solver step per 60 Hz
//! tick (the native substep mode and rate are unverified); the full repeat
//! count (no LOD reduction); no ambient wind; scale 1. Collision (opcode 7)
//! is not ported, so the flap can pass through the legs.

use crate::load::LoadedPiece;
use crate::options::ARNO_CLOTHS;
use crate::rig::Skin;
use acu_assets::{Limits, cloth, forge::Forge, sha256};
use acu_sim::cloth as sim;
use bevy::prelude::*;
use serde_json::json;
use std::path::{Path, PathBuf};

use acu_assets::Result;

pub struct ClothDriver {
    /// Index of the piece's skin in `player::State::skins`.
    pub skin: usize,
    pub handles: Vec<Handle<Mesh>>,
    pub report: serde_json::Value,
    pub last_tick: Option<u64>,
    solver: sim::Cloth,
    rest: Vec<Vec3>,
    rest_normals: Vec<Vec3>,
    particle_bones: Vec<u16>,
    particle_weights: Vec<f32>,
    influences: usize,
    /// Cloth bone -> skin joint slot.
    joints: Vec<usize>,
    triangles: Vec<[u16; 3]>,
    mappings: Vec<sim::VertexMapping>,
    skinned: sim::SkinnedVertices,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u16>,
    canonical: Vec<u16>,
    duplicates: Vec<(u16, u16)>,
    /// Tangent handedness rule (see `load`).
    flip_sign: bool,
    translation: Option<Vec3>,
    pub steps: u64,
}

/// `DataPC_X.forge` -> `DataPC_X_assets.forge`, where the soft bodies live.
fn assets_sibling(archive: &Path) -> Option<PathBuf> {
    let stem = archive.file_stem()?.to_str()?;
    Some(archive.with_file_name(format!("{stem}_assets.forge")))
}

/// Find and bind a `Cloth` resource whose LOD0 names this piece's mesh.
pub fn load(piece: &LoadedPiece, skin: &Skin, skin_index: usize) -> Result<Option<ClothDriver>> {
    let candidates: Vec<PathBuf> = [Some(piece.archive.clone()), assets_sibling(&piece.archive)]
        .into_iter()
        .flatten()
        .filter(|p| p.is_file())
        .collect();
    for &(id, name) in ARNO_CLOTHS {
        let id_text = format!("{id:016x}");
        for archive in &candidates {
            let mut forge = Forge::open(archive, Limits::default())?;
            let Some(entry) = forge
                .entries
                .iter()
                .find(|e| e.file_id == id_text)
                .map(|e| e.index)
            else {
                continue;
            };
            let package = crate::io::decode_package(forge.raw(entry)?, Limits::default())?;
            let Some(r) = package
                .resources
                .iter()
                .find(|r| r.file_id == id_text && r.type_id == "e33044ba")
            else {
                continue;
            };
            let payload = &package.bytes[r.payload.clone()];
            let parsed = cloth::parse(payload, &id_text)?;
            let lod = cloth::lod0_mesh(payload, &parsed)?;
            if lod.mesh_id != piece.file_id {
                continue;
            }
            let mut driver = build(piece, skin, &parsed.lod0_blob, &lod)?;
            driver.skin = skin_index;
            driver.report["resource"] = json!({"name":name,"file_id":id_text,
                "archive":archive.display().to_string(),"entry":entry,
                "payload_sha256":sha256(payload),"mesh":lod.mesh_id});
            return Ok(Some(driver));
        }
    }
    Ok(None)
}

fn values(m: &cloth::ByteMap) -> sim::ParticleValues {
    sim::ParticleValues::from_bytes(&m.bytes, m.scale, m.bias, m.threshold)
}

fn build(
    piece: &LoadedPiece,
    skin: &Skin,
    blob: &cloth::Blob,
    lod: &cloth::LodMesh,
) -> Result<ClothDriver> {
    let n = blob.particle_count()?;
    let cap = (n + 15) & !15;
    let flags = blob.flags()?;
    // Native branches this port does not cover (offsets from body +96).
    if flags[22] != 0 {
        return Err("cloth: LOD damping branch (body +118) is not ported".into());
    }
    if flags[29] != 0 {
        return Err("cloth: damping-map integrator (body +125) is not ported".into());
    }
    if flags[7] != 1 {
        return Err("cloth: 4-wide tether mode (body +103 != 1) is not ported".into());
    }
    let settings = blob.settings()?;
    let maps = blob.particle_maps()?;
    let gravity_scale = if flags[28] != 0 {
        let m = maps[0]
            .as_ref()
            .ok_or("cloth: body +124 set without map 0")?;
        Some(values(m).value[..n].to_vec())
    } else {
        None
    };
    let sets = blob.distance_sets()?;
    let distance = sets.map(|s| {
        let v = sim::ParticleValues::from_bytes(&s.rest_q, s.scale, s.offset, s.threshold);
        sim::DistanceSet {
            pairs: s.pairs,
            rest: v.value,
            active: v.enabled,
            simd_count: s.simd_count,
            k_min: s.k_min,
            k_max: s.k_max,
            stiffness: s.stiffness,
        }
    });
    let pc = blob.particle_constraints()?;
    let particles: Vec<Vec4> = blob
        .particles()?
        .iter()
        .map(|p| Vec4::from_array(*p))
        .collect();
    let solver = sim::Cloth {
        state: sim::State {
            pos: particles.clone(),
            prev: particles.clone(),
            target: particles,
        },
        normals: vec![Vec4::X; cap],
        integration: sim::Integration {
            gravity: Vec3::from_array(settings.gravity).extend(0.0),
            damping: settings.damping,
            max_speed: settings.max_speed,
            lod_damping: None,
        },
        gravity_scale,
        distance,
        backstop: values(&pc.backstop),
        max_distance: values(&pc.max_distance),
        tether: values(&pc.tether),
        anchors: pc.anchors,
        triangles: blob.triangles()?,
        schedule: blob.schedule()?,
        blend: settings.blend,
        drag: (Vec4::ZERO, settings.drag.a, settings.drag.b),
        skipped_collision: 0,
    };
    solver.validate()?;
    if lod.mappings.len() != piece.vertices.len() {
        return Err("cloth: render vertex count differs from the mesh".into());
    }
    // Cloth bones bind to the piece's skin joints by name hash; their inverse
    // binds must agree with the mesh's.
    let mut joints = Vec::new();
    let mut bind_error = 0.0f32;
    for b in &lod.bones {
        let j = skin
            .joint(b.name_hash)
            .ok_or_else(|| format!("cloth bone {:08x} is not a skin joint", b.name_hash))?;
        let d = Mat4::from_cols_array(&b.matrix) - skin.bind(j).inverse();
        bind_error = bind_error.max(d.to_cols_array().iter().fold(0.0, |m, v| m.max(v.abs())));
        joints.push(j);
    }
    if bind_error > 1e-3 {
        return Err(format!("cloth bone binds differ from the mesh by {bind_error}").into());
    }
    let v3 = |a: &[[f32; 3]]| a.iter().map(|p| Vec3::from_array(*p)).collect::<Vec<_>>();
    let mut driver = ClothDriver {
        skin: 0,
        handles: Vec::new(),
        report: json!({}),
        last_tick: None,
        solver,
        rest: v3(&lod.rest_positions),
        rest_normals: v3(&lod.rest_normals),
        particle_bones: lod.particle_bones.clone(),
        particle_weights: lod.particle_weights.clone(),
        influences: lod.particle_influences,
        joints,
        triangles: lod.triangles.clone(),
        mappings: lod
            .mappings
            .iter()
            .map(|m| sim::VertexMapping {
                skinned: m.skinned,
                triangle: m.triangle,
                weights: m.weights,
            })
            .collect(),
        skinned: sim::SkinnedVertices {
            rest: v3(&lod.skinned_rest),
            bones: lod.skinned_bones.clone(),
            weights: lod.skinned_weights.clone(),
            k: lod.vertex_influences,
        },
        uvs: lod.uvs.clone(),
        indices: lod.indices.clone(),
        canonical: lod.canonical.clone(),
        duplicates: lod.duplicates.clone(),
        flip_sign: false,
        translation: None,
        steps: 0,
    };
    // Rest check: mapping the rest particles must reproduce the mesh's
    // vertices in order, and the native frames give the mesh's handedness.
    let q = driver.rest.clone();
    let identity = vec![Mat4::IDENTITY; lod.bones.len()];
    let mut x = Vec::new();
    sim::render_positions(
        &q,
        &driver.triangles,
        &driver.mappings,
        &driver.skinned,
        &identity,
        Mat4::IDENTITY,
        &mut x,
    );
    let rest_error = x
        .iter()
        .zip(&piece.vertices)
        .map(|(a, v)| (*a - Vec3::from_array(v.position)).length())
        .fold(0.0f32, f32::max);
    if rest_error > 1e-3 {
        return Err(format!("cloth rest mapping misses the mesh by {rest_error} m").into());
    }
    let (n_r, t_r, b_r) = sim::surface_frames(
        &x,
        &driver.uvs,
        &driver.indices,
        &driver.canonical,
        &driver.duplicates,
    );
    let mesh_sign = |v: &acu_assets::vertex::Vertex| {
        let n = Vec3::from_array(v.normal).normalize_or_zero();
        let t = Vec3::from_array(v.tangent.unwrap_or([1.0, 0.0, 0.0])).normalize_or_zero();
        let b = Vec3::from_array(v.binormal.unwrap_or([0.0, 1.0, 0.0]));
        n.cross(t).dot(b) < 0.0
    };
    let agree = (0..x.len())
        .filter(|&i| (n_r[i].cross(t_r[i]).dot(b_r[i]) < 0.0) == mesh_sign(&piece.vertices[i]))
        .count();
    // The stored bitangent is negated natively; pick the rule that matches
    // the mesh's own frames on most vertices and report the agreement.
    driver.flip_sign = agree * 2 < x.len();
    let normal_dot = (0..x.len())
        .map(|i| n_r[i].dot(Vec3::from_array(piece.vertices[i].normal).normalize_or_zero()))
        .fold(1.0f32, f32::min);
    driver.report = json!({
        "finding":"ACU-RE-0081",
        "particles":n,"render_vertices":x.len(),"bones":lod.bones.len(),
        "schedule":driver.solver.schedule,
        "gravity":settings.gravity,"max_speed":settings.max_speed,"drag":[settings.drag.a,settings.drag.b],
        "authored_wind":{"enabled":settings.drag.wind_enabled,"vector":settings.drag.wind},
        "max_bone_bind_error":bind_error,"max_rest_mapping_error_m":rest_error,
        "rest_normal_min_dot_mesh":normal_dot,
        "tangent_sign_agreement":[agree, x.len()],"tangent_sign_flipped":driver.flip_sign,
        "choices":"one solver step per 60 Hz tick; full repeat count; no ambient wind; scale 1",
        "not_ported":"collision (opcode 7): the flap can pass through the legs"});
    Ok(driver)
}

impl ClothDriver {
    /// One fixed tick: skin the targets, carry, step, and rebuild the mesh.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        skin: &Skin,
        pose: &[Mat4],
        bind: bool,
        deform: bool,
        yaw: f32,
        position: Vec3,
        dt: f32,
    ) {
        let c = crate::native_z_up_to_bevy_y_up();
        let r = c.inverse() * Quat::from_rotation_y(yaw) * c;
        let t = c.inverse() * position;
        let rm = Mat4::from_quat(r);
        let mats = skin.matrices(pose, bind, deform);
        let bones: Vec<Mat4> = self.joints.iter().map(|&j| rm * mats[j]).collect();
        let (targets, target_normals) = sim::skin_particles(
            &self.rest,
            &self.rest_normals,
            &self.particle_bones,
            &self.particle_weights,
            self.influences,
            &bones,
            rm,
        );
        let s = &mut self.solver.state;
        match self.translation {
            None => {
                // Reset (`0x1423b1270`): positions and history take the targets.
                for (i, tg) in targets.iter().enumerate() {
                    let w = s.pos[i].w;
                    s.pos[i] = tg.truncate().extend(w);
                    s.prev[i] = s.pos[i];
                }
            }
            Some(old) => {
                let rows = [Vec4::X, Vec4::Y, Vec4::Z];
                sim::carry(s, rows, old.extend(0.0), rows, t.extend(0.0));
            }
        }
        self.translation = Some(t);
        s.target = targets;
        self.solver.step(dt, &target_normals);
        self.steps += 1;
    }

    /// Write the current particles to the piece's meshes (mesh frame).
    pub fn present(
        &self,
        meshes: &mut Assets<Mesh>,
        skin: &Skin,
        pose: &[Mat4],
        bind: bool,
        deform: bool,
        yaw: f32,
    ) {
        let c = crate::native_z_up_to_bevy_y_up();
        let r = c.inverse() * Quat::from_rotation_y(yaw) * c;
        let rm = Mat4::from_quat(r);
        let mats = skin.matrices(pose, bind, deform);
        let bones: Vec<Mat4> = self.joints.iter().map(|&j| rm * mats[j]).collect();
        let q: Vec<Vec3> = self.solver.state.pos.iter().map(|p| p.truncate()).collect();
        let mut x = Vec::new();
        sim::render_positions(
            &q,
            &self.triangles,
            &self.mappings,
            &self.skinned,
            &bones,
            rm,
            &mut x,
        );
        let inv = r.inverse();
        for p in &mut x {
            *p = inv * *p;
        }
        let (n, t, b) = sim::surface_frames(
            &x,
            &self.uvs,
            &self.indices,
            &self.canonical,
            &self.duplicates,
        );
        let positions: Vec<[f32; 3]> = x.iter().map(|p| p.to_array()).collect();
        let normals: Vec<[f32; 3]> = n.iter().map(|v| v.to_array()).collect();
        let tangents: Vec<[f32; 4]> = (0..x.len())
            .map(|i| {
                let neg = (n[i].cross(t[i]).dot(b[i]) < 0.0) != self.flip_sign;
                [t[i].x, t[i].y, t[i].z, if neg { -1.0 } else { 1.0 }]
            })
            .collect();
        for h in &self.handles {
            if let Some(mut m) = meshes.get_mut(h) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
                m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents.clone());
            }
        }
    }
}
