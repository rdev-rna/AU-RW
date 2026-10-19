//! Archive-backed diagnostic rig assembly. Solver and pose choices: finding0017.
use crate::load::LoadedPiece;
use acu_assets::{Limits, Result, forge::Forge, package, sha256, skeleton};
use acu_sim::skin::{Influence, Rig};
use bevy::prelude::*;
use serde_json::json;
use std::{collections::HashMap, path::Path};

struct Joint {
    hash: u32,
    parent: Option<usize>,
    bind: Mat4,
}
pub struct Library {
    joints: Vec<Joint>,
    by_hash: HashMap<u32, usize>,
    pub report: serde_json::Value,
}

fn matrix(p: [f32; 4], q: [f32; 4]) -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_array(q).normalize(), Vec3::new(p[0], p[1], p[2]))
}
fn pointer(p: &skeleton::Pointer) -> Option<&str> {
    match p {
        skeleton::Pointer::Id(i) | skeleton::Pointer::Link(i) => Some(i),
        _ => None,
    }
}

pub fn load(game_dir: &Path) -> Result<Library> {
    let mut sources = Vec::new();
    let mut records = Vec::new();
    let mut rejected = Vec::new();
    for (archive, entries) in [
        ("DataPC.forge", vec![110, 968, 1006]),
        ("DataPC_ACU_LGS_BelleEpoque.forge", vec![334]),
    ] {
        let mut forge = Forge::open(game_dir.join(archive), Limits::default())?;
        for entry in entries {
            let record = forge.entries[entry].clone();
            let raw = forge.raw(entry)?;
            let stored = sha256(&raw);
            let p = package::decode(&raw, Limits::default())?;
            for (index, r) in p.resources.iter().enumerate() {
                if r.type_id != skeleton::TYPE_SKELETON {
                    continue;
                }
                // Exclude alternative heads and unrelated effects/props.
                if archive.contains("BelleEpoque")
                    && !(index == 66
                        || (116..=121).contains(&index)
                        || (159..=183).contains(&index))
                {
                    continue;
                }
                if archive == "DataPC.forge"
                    && entry == 110
                    && r.name != "ACU_BaseSkeleton_Male"
                    && r.name != "ACU_Base"
                    && r.name != "ACU_Unisex_ArmsSimple"
                {
                    continue;
                }
                if archive == "DataPC.forge" && entry == 968 && r.file_id != "000000070d1bd4fc" {
                    continue;
                }
                if archive == "DataPC.forge"
                    && entry == 1006
                    && !["0000000c94a16e93", "0000001240bfbded"].contains(&r.file_id.as_str())
                {
                    continue;
                }
                let payload = &p.bytes[r.payload.clone()];
                match skeleton::parse(payload,&r.file_id,Limits::default()){
                    Ok(s)=>{
                        records.push(json!({"archive":archive,"entry":record,"resource_index":index,"resource":r,
                            "stored_sha256":stored,"payload_sha256":sha256(payload),"bones":s.bones.len()}));
                        sources.push(s);
                    }
                    Err(e)=>rejected.push(json!({"archive":archive,"entry":entry,"resource_index":index,"file_id":r.file_id,"error":e.to_string()})),
                }
            }
        }
    }
    let male = sources
        .iter()
        .position(|s| s.file_id == "00000006a522da4d")
        .ok_or("male rig absent")?;
    sources.swap(0, male);
    let mut library = Library {
        joints: Vec::new(),
        by_hash: HashMap::new(),
        report: json!({}),
    };
    let mut pending = sources;
    let mut registrations = Vec::new();
    for pass in 0..8 {
        let mut remaining = Vec::new();
        for s in pending {
            let alignment = if library.joints.is_empty() {
                Some(Mat4::IDENTITY)
            } else {
                s.bones.iter().find_map(|b| {
                    library.by_hash.get(&b.name_hash).map(|&i| {
                        library.joints[i].bind
                            * matrix(b.global_position, b.global_rotation).inverse()
                    })
                })
            };
            let Some(alignment) = alignment else {
                remaining.push(s);
                continue;
            };
            let id_hash: HashMap<_, _> = s
                .bones
                .iter()
                .map(|b| (b.file_id.as_str(), b.name_hash))
                .collect();
            // Allocate all slots first; source array order need not be hierarchical.
            let added: Vec<_> = s
                .bones
                .iter()
                .filter(|b| !library.by_hash.contains_key(&b.name_hash))
                .map(|b| b.name_hash)
                .collect();
            for b in &s.bones {
                if library.by_hash.contains_key(&b.name_hash) {
                    continue;
                }
                let i = library.joints.len();
                library.by_hash.insert(b.name_hash, i);
                library.joints.push(Joint {
                    hash: b.name_hash,
                    parent: None,
                    bind: alignment * matrix(b.global_position, b.global_rotation),
                });
            }
            for b in &s.bones {
                if !added.contains(&b.name_hash) {
                    continue;
                }
                let i = library.by_hash[&b.name_hash];
                library.joints[i].parent = pointer(&b.parent)
                    .and_then(|p| id_hash.get(p))
                    .and_then(|h| library.by_hash.get(h))
                    .copied();
            }
            registrations.push(json!({"file_id":s.file_id,"added_hashes":added.iter().map(|h|format!("{h:08x}")).collect::<Vec<_>>(),"alignment":alignment.to_cols_array()}));
        }
        pending = remaining;
        if pending.is_empty() {
            break;
        }
        if pass == 7 {
            return Err(format!(
                "rig registration did not converge: {:?}",
                pending.iter().map(|s| &s.file_id).collect::<Vec<_>>()
            )
            .into());
        }
    }
    library.report = json!({"finding":"ACU-RE-0017","sources":records,"unsupported":rejected,
        "registration":registrations,"joint_count":library.joints.len(),"pieces":[],
        "limits":"Diagnostic name-hash/root registration; source candidates are not native row selection. No modifier solvers or native animation playback."});
    Ok(library)
}

pub struct Skin {
    rig: Rig,
    hashes: Vec<u32>,
    vertices: Vec<(Vec3, Vec3)>,
    tangents: Vec<(Vec3, f32)>,
    weights: Vec<Influence>,
    pub handles: Vec<Handle<Mesh>>,
}

impl Library {
    pub fn skin(&mut self, piece: &LoadedPiece) -> Result<Skin> {
        let mut hashes: Vec<_> = self.joints.iter().map(|j| j.hash).collect();
        let mut lookup = self.by_hash.clone();
        let mut parents: Vec<_> = self.joints.iter().map(|j| j.parent).collect();
        // Find the mesh-to-skeleton frame from an exact common base bone.
        let common = piece
            .bones
            .iter()
            .find_map(|b| {
                let hash = u32::from_str_radix(&b.name_hash, 16).ok()?;
                let &i = self.by_hash.get(&hash)?;
                // Base hierarchy slots are first and avoid registering from a face/clothing bone.
                (i < 109).then_some(self.joints[i].bind * Mat4::from_cols_array(&b.matrix))
            })
            .ok_or("mesh has no base-rig frame anchor")?;
        let mut bind: Vec<_> = self
            .joints
            .iter()
            .map(|j| common.inverse() * j.bind)
            .collect();
        let mut unresolved = Vec::new();
        for bone in &piece.bones {
            let hash = u32::from_str_radix(&bone.name_hash, 16)?;
            let inverse_bind = Mat4::from_cols_array(&bone.matrix);
            if !inverse_bind.is_finite() || inverse_bind.determinant().abs() < 1e-8 {
                return Err("invalid mesh bind matrix".into());
            }
            let i = if let Some(&i) = lookup.get(&hash) {
                i
            } else {
                let i = bind.len();
                lookup.insert(hash, i);
                hashes.push(hash);
                bind.push(Mat4::IDENTITY);
                parents.push(None);
                unresolved.push(bone.name_hash.clone());
                i
            };
            bind[i] = inverse_bind.inverse();
        }
        let rig = Rig::new(bind, parents)?;
        let mut weights = vec![
            Influence {
                joints: [0; 8],
                weights: [0; 8]
            };
            piece.vertices.len()
        ];
        let mut covered = vec![false; piece.vertices.len()];
        for part in &piece.parts {
            for i in part.vertex_base..part.vertex_base + part.vertex_count {
                if i >= weights.len() || covered[i] {
                    return Err("overlapping/outside skin spans".into());
                }
                covered[i] = true;
                let v = &piece.vertices[i];
                let j = v.joints.as_ref().ok_or("missing joint slots")?;
                let w = v.joint_weights.as_ref().ok_or("missing joint weights")?;
                if j.len() != w.len() || !matches!(j.len(), 4 | 8) {
                    return Err("invalid influence count".into());
                }
                for slot in 0..j.len() {
                    weights[i].weights[slot] = w[slot];
                    if w[slot] == 0 {
                        continue;
                    }
                    let mesh_index = *part
                        .palette
                        .get(j[slot] as usize)
                        .ok_or("weighted slot outside hardware palette")?
                        as usize;
                    let bone = piece
                        .bones
                        .get(mesh_index)
                        .ok_or("hardware palette outside mesh bones")?;
                    let hash = u32::from_str_radix(&bone.name_hash, 16)?;
                    weights[i].joints[slot] = lookup[&hash];
                }
                weights[i].validate(rig.bind.len())?;
            }
        }
        if covered.iter().any(|v| !*v) {
            return Err("uncovered skin vertices".into());
        }
        let unresolved_weighted: std::collections::BTreeSet<_> = weights
            .iter()
            .flat_map(|w| w.joints.iter().zip(w.weights))
            .filter(|&(_, w)| w != 0)
            .filter(|&(&i, _)| i >= self.joints.len())
            .map(|(&i, _)| format!("{:08x}", hashes[i]))
            .collect();
        let vertices: Vec<_> = piece
            .vertices
            .iter()
            .map(|v| (Vec3::from_array(v.position), Vec3::from_array(v.normal)))
            .collect();
        let matrices = rig.matrices(&vec![Quat::IDENTITY; rig.bind.len()]);
        let max_error = vertices
            .iter()
            .zip(&weights)
            .map(|(&(p, n), w)| (w.deform(p, n, &matrices).0 - p).length())
            .fold(0.0f32, f32::max);
        if max_error > 1e-4 {
            return Err(format!("bind identity error {max_error}").into());
        }
        self.report["pieces"].as_array_mut().unwrap().push(json!({"file_id":piece.file_id,"name":piece.label,"payload_sha256":piece.payload_sha256,
            "vertices":vertices.len(),"joints":rig.bind.len(),"mesh_to_skeleton":common.to_cols_array(),"max_bind_error":max_error,
            "unresolved_parent_hashes":unresolved,"unresolved_weighted_parent_hashes":unresolved_weighted,
            "unresolved_policy":"Preserve original bind joint and weights; parentless and frozen. No guessed attachment."}));
        let tangents = piece
            .vertices
            .iter()
            .map(|v| {
                let n = Vec3::from_array(v.normal).normalize_or_zero();
                let t = Vec3::from_array(v.tangent.unwrap_or([1.0, 0.0, 0.0])).normalize_or_zero();
                let b = Vec3::from_array(v.binormal.unwrap_or([0.0, 1.0, 0.0]));
                (t, if n.cross(t).dot(b) < 0.0 { -1.0 } else { 1.0 })
            })
            .collect();
        Ok(Skin {
            rig,
            hashes,
            vertices,
            tangents,
            weights,
            handles: Vec::new(),
        })
    }
}

impl Skin {
    pub fn update(
        &self,
        meshes: &mut Assets<Mesh>,
        distance: f32,
        speed: f32,
        tick: u64,
        bind: bool,
        deform: bool,
    ) {
        let mut rotations = vec![Quat::IDENTITY; self.hashes.len()];
        let phase = distance * std::f32::consts::TAU / 1.5;
        let blend = (speed / 1.8).clamp(0.0, 1.5);
        for (i, &h) in self.hashes.iter().enumerate() {
            let angle = if deform {
                if h == 0x176183f0 { 0.5 } else { 0.0 }
            } else if bind {
                0.0
            } else {
                match h {
                    0x176183f0 => phase.sin() * 0.38 * blend,
                    0x757f1291 => -phase.sin() * 0.38 * blend,
                    0x060df401 => (-phase.sin()).max(0.0) * 0.6 * blend,
                    0x863d09fc => phase.sin().max(0.0) * 0.6 * blend,
                    0xeb830ada => -phase.sin() * 0.22 * blend,
                    0x6bb3f727 => phase.sin() * 0.22 * blend,
                    0x530ec1cb => (tick as f32 / 60.0 * 1.6).sin() * 0.006,
                    _ => 0.0,
                }
            };
            rotations[i] = Quat::from_rotation_x(angle);
        }
        let matrices = self.rig.matrices(&rotations);
        let surface =
            acu_sim::skin::surface(&self.vertices, &self.tangents, &self.weights, &matrices);
        for h in &self.handles {
            if let Some(mut m) = meshes.get_mut(h) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, surface.positions.clone());
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, surface.normals.clone());
                m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, surface.tangents.clone());
            }
        }
    }
}
