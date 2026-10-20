//! Archive-backed diagnostic rig assembly. Solver and pose choices: finding0017.
use crate::load::LoadedPiece;
use acu_assets::{Limits, Result, forge::Forge, sha256, skeleton};
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
    animation_root: (u32, Mat4),
    leg_helpers: Vec<(u32, LegHelper)>,
}
enum LegHelper {
    Aim(u32),
    Orientation(Vec<(u32, f32)>),
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
    let mut leg_helpers = Vec::new();
    for (archive, entries) in [
        ("DataPC.forge", vec![110, 968, 1006]),
        ("DataPC_ACU_LGS_BelleEpoque.forge", vec![334]),
    ] {
        let mut forge = Forge::open(game_dir.join(archive), Limits::default())?;
        for entry in entries {
            let record = forge.entries[entry].clone();
            let raw = forge.raw(entry)?;
            let stored = sha256(&raw);
            let p = crate::io::decode_package(raw, Limits::default())?;
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
                        if r.name == "ACU_Unisex_LegsRollbones" {
                            use skeleton::humanoid::{ModifierData, ObjectData};
                            let ids: HashMap<_, _> = s.bones.iter().map(|b|(b.file_id.as_str(),b.name_hash)).collect();
                            let resolve = |p: &skeleton::Pointer| pointer(p).and_then(|id|ids.get(id)).copied();
                            for b in &s.bones {
                                for m in &b.modifiers {
                                    if m.blend_weight != 1.0 || m.use_modified_bone_matrix { continue; }
                                    let helper = match &m.data {
                                        ModifierData::MaxLookAt { look_at_bone, position_offset, .. }
                                            if *position_offset == [0.0;4] => resolve(look_at_bone).map(LegHelper::Aim),
                                        ModifierData::OrientationConstraint(c)
                                            if !c.keep_initial_offset && c.offset == [0.0,0.0,0.0,1.0] => {
                                                let targets: Option<Vec<_>> = c.targets.iter().map(|p| {
                                                    let skeleton::Pointer::Inline { object, .. } = p else {return None;};
                                                    let ObjectData::ConstraintTarget {bone,weight} = &object.data else {return None;};
                                                    Some((resolve(bone)?, *weight))
                                                }).collect();
                                                targets.map(LegHelper::Orientation)
                                            }
                                        _ => None,
                                    };
                                    if let Some(h) = helper { leg_helpers.push((b.name_hash,h)); }
                                }
                            }
                        }
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
    let animation_root = (
        sources[0].bones[0].name_hash,
        matrix(
            sources[0].root_local_position,
            sources[0].root_local_rotation,
        ),
    );
    let mut library = Library {
        joints: Vec::new(),
        by_hash: HashMap::new(),
        report: json!({}),
        animation_root,
        leg_helpers,
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
        "leg_helper_adapter":"ACU-RE-0020: decoded LegsRollbones target references; bind-relative aim swing and weighted target rotation deltas. Native axes/up-node/reflex/twist solver is NOT recovered.",
        "leg_helper_count":library.leg_helpers.len(),
        "limits":"Diagnostic name-hash/root registration; source candidates are not native row selection. Authored leg helper adapter; no native modifier parity."});
    Ok(library)
}

pub struct Skin {
    rig: Rig,
    hashes: Vec<u32>,
    vertices: Vec<(Vec3, Vec3)>,
    tangents: Vec<(Vec3, f32)>,
    weights: Vec<Influence>,
    pub handles: Vec<Handle<Mesh>>,
    motion_frames: Vec<(usize, Mat4)>,
    common: Mat4,
}

impl Library {
    pub fn motion_rig(&self) -> Result<acu_motion::unity::PoseRig> {
        let mut rig = acu_motion::unity::PoseRig::new(
            self.joints.iter().map(|j| j.hash).collect(),
            self.joints.iter().map(|j| j.bind).collect(),
            self.joints.iter().map(|j| j.parent).collect(),
        )?;
        rig.set_root_frame(self.animation_root.0, self.animation_root.1)?;
        use acu_motion::unity::Helper;
        let helpers = self
            .leg_helpers
            .iter()
            .map(|(hash, h)| -> Result<_> {
                let index = |hash: &u32| {
                    self.by_hash
                        .get(hash)
                        .copied()
                        .ok_or("leg helper hash absent")
                };
                let h = match h {
                    LegHelper::Aim(target) => Helper::Aim {
                        target: index(target)?,
                    },
                    LegHelper::Orientation(targets) => Helper::Orientation {
                        targets: targets
                            .iter()
                            .map(|(t, w)| Ok((index(t)?, *w)))
                            .collect::<Result<Vec<_>>>()?,
                    },
                };
                Ok((index(hash)?, h))
            })
            .collect::<Result<Vec<_>>>()?;
        rig.set_helpers(helpers)?;
        Ok(rig)
    }
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
            motion_frames: self
                .joints
                .iter()
                .enumerate()
                .map(|(i, j)| (i, j.bind.inverse()))
                .collect(),
            common,
        })
    }
}

impl Skin {
    pub fn update(&self, meshes: &mut Assets<Mesh>, pose: &[Mat4], bind: bool, deform: bool) {
        let mut rotations = vec![Quat::IDENTITY; self.hashes.len()];
        for (i, &h) in self.hashes.iter().enumerate() {
            let angle = if deform {
                if h == 0x176183f0 { 0.5 } else { 0.0 }
            } else {
                0.0
            };
            rotations[i] = Quat::from_rotation_x(angle);
        }
        let mut matrices = self.rig.matrices(&rotations);
        if !bind && !deform {
            let inverse = self.common.inverse();
            for &(i, bind_inverse) in &self.motion_frames {
                matrices[i] = inverse * pose[i] * bind_inverse * self.common;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// ACU-RE-0025: compare the actual Rust pose evaluator to externally read
    /// player joints at a recorded clip/time, without phase or speed fitting.
    #[test]
    #[ignore = "requires ACU_GAME_DIR and ACU_NATIVE_IDLE_FIXTURE; native capture is machine data"]
    fn installed_idle_matches_recorded_player_primary_rotations() {
        let dir = std::path::PathBuf::from(
            std::env::var("ACU_GAME_DIR").expect("unavailable: ACU_GAME_DIR required"),
        );
        let fixture_path = std::env::var("ACU_NATIVE_IDLE_FIXTURE")
            .expect("unavailable: ACU_NATIVE_IDLE_FIXTURE required");
        let bytes = std::fs::read(fixture_path).expect("unavailable: native idle fixture");
        assert!(bytes.len() <= 4 * 1024 * 1024, "fixture size bound");
        let fixture: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(fixture["schema_version"], 1);
        assert_eq!(fixture["finding"], "ACU-RE-0025");
        assert_eq!(fixture["local_context_flags"], "0c000000");
        let executable = std::fs::read(dir.join("ACU.exe")).expect("unavailable: ACU.exe");
        assert_eq!(
            acu_assets::sha256(&executable),
            fixture["executable_sha256"].as_str().unwrap()
        );
        let library = load(&dir).unwrap();
        let (matcher, sources) = crate::motion::load(&dir, &library).unwrap();
        let id = fixture["clip_file_id"].as_str().unwrap();
        let take = matcher.takes.iter().find(|t| t.file_id == id).unwrap();
        let source = sources["clips"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["resource"]["file_id"] == id)
            .unwrap();
        assert_eq!(source["payload_sha256"], fixture["clip_payload_sha256"]);
        let joints = fixture["joints"].as_array().unwrap();
        let samples = fixture["samples"].as_array().unwrap();
        assert_eq!(joints.len(), 23, "declared primary subset only");
        assert!(
            (20..=4096).contains(&samples.len()),
            "sample coverage bound"
        );
        let mut mapped = Vec::new();
        for joint in joints {
            let hash = u32::from_str_radix(joint["hash"].as_str().unwrap(), 16).unwrap();
            let index = matcher.rig.hashes.iter().position(|h| *h == hash).unwrap();
            assert!(!mapped.contains(&index), "duplicate mapped joint");
            let parent = matcher.rig.parents[index].expect("nonroot subset");
            assert_eq!(
                format!("{:08x}", matcher.rig.hashes[parent]),
                joint["parent_hash"].as_str().unwrap()
            );
            mapped.push(index);
        }
        let angular_error = |a: Quat, b: Quat| {
            // Compute the metric in f64 so near-unit f32 values do not impose
            // a large acos floor on this small recorded residual.
            let a = a.to_array().map(f64::from);
            let b = b.to_array().map(f64::from);
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let aa: f64 = a.iter().map(|v| v * v).sum();
            let bb: f64 = b.iter().map(|v| v * v).sum();
            assert!(aa.is_finite() && bb.is_finite() && aa > 0.5 && bb > 0.5);
            (2.0 * (dot.abs() / (aa * bb).sqrt()).clamp(0.0, 1.0).acos()).to_degrees()
        };
        let mut worst_mean = 0.0_f64;
        let mut negative_mean = 0.0_f64;
        for sample in samples {
            let time = sample["time900"].as_f64().unwrap() + sample["offset876"].as_f64().unwrap();
            assert!(time.is_finite());
            let time = (time as f32).rem_euclid(take.clip.duration);
            let expected = sample["local_rotation_xyzw"].as_array().unwrap();
            assert_eq!(expected.len(), mapped.len());
            let mut means = [0.0_f64; 2];
            for (control, offset) in [0.0, 0.3].into_iter().enumerate() {
                let pose = matcher
                    .rig
                    .pose(&take.clip, (time + offset).rem_euclid(take.clip.duration));
                for (row, &index) in mapped.iter().enumerate() {
                    let parent = matcher.rig.parents[index].unwrap();
                    let local = pose[parent].inverse() * pose[index];
                    let (_, rotation, _) = local.to_scale_rotation_translation();
                    let values = expected[row].as_array().unwrap();
                    assert_eq!(values.len(), 4);
                    let recorded = Quat::from_array(std::array::from_fn(|i| {
                        values[i].as_f64().unwrap() as f32
                    }));
                    means[control] += angular_error(rotation, recorded) / mapped.len() as f64;
                }
            }
            worst_mean = worst_mean.max(means[0]);
            negative_mean += means[1] / samples.len() as f64;
        }
        println!(
            "native idle: {} frames x {} joints; worst frame mean={worst_mean:.6} deg; shifted-time control mean={negative_mean:.6} deg",
            samples.len(),
            mapped.len()
        );
        assert!(
            worst_mean < 0.05,
            "recorded direct-time primary idle mismatch"
        );
        assert!(negative_mean > 0.1 && negative_mean > worst_mean * 5.0);
    }

    /// ACU-RE-0027: verify native target/source rows and mode transforms at
    /// independently captured local-pose evaluation stages.
    #[test]
    #[ignore = "requires ACU_GAME_DIR and ACU_NATIVE_BINDING_FIXTURE; native capture is machine data"]
    fn installed_binding_modes_match_flagged_player_rotations() {
        use acu_motion::unity::NativeTrackBinding;

        let dir = std::path::PathBuf::from(
            std::env::var("ACU_GAME_DIR").expect("unavailable: ACU_GAME_DIR required"),
        );
        let fixture_path = std::env::var("ACU_NATIVE_BINDING_FIXTURE")
            .expect("unavailable: ACU_NATIVE_BINDING_FIXTURE required");
        let bytes = std::fs::read(fixture_path).expect("unavailable: native binding fixture");
        assert!(bytes.len() <= 4 * 1024 * 1024, "fixture size bound");
        let fixture: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(fixture["schema_version"], 2);
        assert_eq!(fixture["finding"], "ACU-RE-0027");
        assert_eq!(fixture["local_context_flags"], "0c000000");
        let executable = std::fs::read(dir.join("ACU.exe")).expect("unavailable: ACU.exe");
        assert_eq!(
            acu_assets::sha256(&executable),
            fixture["executable_sha256"].as_str().unwrap()
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2);
        let library = load(&dir).unwrap();
        let rig = library.motion_rig().unwrap();
        let angular_error = |a: Quat, b: Quat| {
            let a = a.to_array().map(f64::from);
            let b = b.to_array().map(f64::from);
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let aa: f64 = a.iter().map(|v| v * v).sum();
            let bb: f64 = b.iter().map(|v| v * v).sum();
            assert!(aa.is_finite() && bb.is_finite() && aa > 0.5 && bb > 0.5);
            (2.0 * (dot.abs() / (aa * bb).sqrt()).clamp(0.0, 1.0).acos()).to_degrees()
        };
        let mut worst_mean = 0.0_f64;
        let mut direct_mean = 0.0_f64;
        let mut compared_frames = 0;
        for (case_index, case) in cases.iter().enumerate() {
            let id = case["clip_file_id"].as_str().unwrap();
            let expected_joint_count = if case_index == 0 { 21 } else { 23 };
            let (clip, payload_sha256) = crate::motion::load_clip(&dir, id).unwrap();
            assert_eq!(
                payload_sha256,
                case["clip_payload_sha256"].as_str().unwrap()
            );
            assert_eq!(
                clip.duration,
                case["clip_duration"].as_f64().unwrap() as f32
            );
            let bindings: Vec<NativeTrackBinding> =
                serde_json::from_value(case["bindings"].clone()).unwrap();
            assert_eq!(bindings.len(), expected_joint_count);
            assert_eq!(
                case["mode_counts"]["5"].as_u64().unwrap() as usize,
                expected_joint_count - 1
            );
            assert_eq!(case["mode_counts"]["4"], 1);
            let mut direct_control = bindings.clone();
            for binding in &mut direct_control {
                binding.mode = 0;
            }
            let samples = case["samples"].as_array().unwrap();
            assert!(!samples.is_empty(), "independent capture coverage");
            let mut mapped = Vec::with_capacity(bindings.len());
            for binding in &bindings {
                let index = rig
                    .hashes
                    .iter()
                    .position(|hash| *hash == binding.target_hash)
                    .expect("native target hash absent from Rust rig");
                assert!(!mapped.contains(&index), "duplicate native target");
                assert_eq!(binding.channel_type, 6, "rotation channel only");
                mapped.push(index);
            }
            let mut case_worst = 0.0_f64;
            let mut case_direct_mean = 0.0_f64;
            for sample in samples {
                assert!(sample["weight"].as_f64().unwrap() > 0.99);
                assert!(sample["sample_span_ms"].as_f64().unwrap() < 32.0);
                let time = (sample["time900"].as_f64().unwrap()
                    + sample["offset876"].as_f64().unwrap()) as f32;
                let time = time.rem_euclid(clip.duration);
                let expected = sample["local_rotations"].as_array().unwrap();
                assert_eq!(expected.len(), bindings.len());
                let poses = [
                    rig.pose_with_bindings(&clip, time, &bindings).unwrap(),
                    rig.pose_with_bindings(&clip, time, &direct_control)
                        .unwrap(),
                ];
                let mut means = [0.0_f64; 2];
                for (row, (&index, binding)) in mapped.iter().zip(&bindings).enumerate() {
                    let parent = rig.parents[index].expect("primary target must have a parent");
                    let values = expected[row]["xyzw"].as_array().unwrap();
                    assert_eq!(values.len(), 4);
                    let recorded = Quat::from_array(std::array::from_fn(|i| {
                        values[i].as_f64().unwrap() as f32
                    }));
                    assert_eq!(
                        expected[row]["target_hash"].as_u64().unwrap() as u32,
                        binding.target_hash
                    );
                    for (control, pose) in poses.iter().enumerate() {
                        let local = pose[parent].inverse() * pose[index];
                        let rotation = local.to_scale_rotation_translation().1;
                        means[control] += angular_error(rotation, recorded) / bindings.len() as f64;
                    }
                }
                case_worst = case_worst.max(means[0]);
                case_direct_mean += means[1] / samples.len() as f64;
                compared_frames += 1;
            }
            println!(
                "native binding case {id}: {} flagged frames x {} joints; worst mean={case_worst:.6} deg; mode-zero control mean={case_direct_mean:.6} deg",
                samples.len(),
                bindings.len()
            );
            assert!(case_worst < 0.05, "runtime-bound direct-time pose mismatch");
            assert!(case_direct_mean > 0.1 && case_direct_mean > case_worst * 5.0);
            worst_mean = worst_mean.max(case_worst);
            direct_mean += case_direct_mean * samples.len() as f64;
        }
        direct_mean /= compared_frames as f64;
        println!(
            "native binding total: {compared_frames} flagged frames; worst mean={worst_mean:.6} deg; mode-zero control mean={direct_mean:.6} deg"
        );
    }

    /// ACU-RE-0025: verify weighted playback against the additional owner-linked
    /// 459-row context discovered at skeleton-component slot +0x280.
    #[test]
    #[ignore = "requires ACU_GAME_DIR and ACU_NATIVE_BLEND_FIXTURE; capture is machine data"]
    fn installed_weighted_blend_matches_coherent_459_context() {
        use acu_assets::animation::Clip;
        use acu_motion::unity::NativeTrackBinding;
        use std::collections::HashSet;

        let dir = std::path::PathBuf::from(
            std::env::var("ACU_GAME_DIR").expect("unavailable: ACU_GAME_DIR required"),
        );
        let fixture_path = std::env::var("ACU_NATIVE_BLEND_FIXTURE")
            .expect("unavailable: ACU_NATIVE_BLEND_FIXTURE required");
        let bytes = std::fs::read(fixture_path).expect("unavailable: native blend fixture");
        assert!(bytes.len() <= 4 * 1024 * 1024, "fixture size bound");
        let fixture: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(fixture["schema_version"], 1);
        assert_eq!(fixture["finding"], "ACU-RE-0025");
        assert_eq!(fixture["pose_slot_offset"], "0x280");
        assert_eq!(fixture["context_flags"][0], "0x07000000");
        assert_eq!(fixture["context_flags"][1], "0x0f000000");
        let executable = std::fs::read(dir.join("ACU.exe")).expect("unavailable: ACU.exe");
        assert_eq!(
            acu_assets::sha256(&executable),
            fixture["executable_sha256"].as_str().unwrap()
        );
        let samples = fixture["samples"].as_array().unwrap();
        assert!(samples.len() >= 2, "independent coherent blend samples");
        let library = load(&dir).unwrap();
        let rig = library.motion_rig().unwrap();
        let mut clips: HashMap<String, (Clip, String)> = HashMap::new();
        let angular_error = |a: Quat, b: Quat| {
            let a = a.to_array().map(f64::from);
            let b = b.to_array().map(f64::from);
            let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let aa: f64 = a.iter().map(|v| v * v).sum();
            let bb: f64 = b.iter().map(|v| v * v).sum();
            assert!(aa.is_finite() && bb.is_finite() && aa > 0.5 && bb > 0.5);
            (2.0 * (dot.abs() / (aa * bb).sqrt()).clamp(0.0, 1.0).acos()).to_degrees()
        };
        let mut target_means = Vec::new();
        let mut bound_target_means = Vec::new();
        let mut bound_direct_means = Vec::new();
        let mut bound_shifted_means = Vec::new();

        for sample in samples {
            assert!(matches!(
                sample["pose_flags"].as_u64().unwrap(),
                0x0700_0000 | 0x0f00_0000
            ));
            assert!(sample["sample_span_ms"].as_f64().unwrap() < 40.0);
            let leaves = sample["leaves"].as_array().unwrap();
            assert!(leaves.len() >= 2 && leaves.len() <= 32);
            let weight_sum: f64 = leaves
                .iter()
                .map(|leaf| leaf["weight"].as_f64().unwrap())
                .sum();
            assert!(
                (weight_sum - 1.0).abs() < 1e-4,
                "captured weights sum to one"
            );
            let mut target_maps = Vec::new();
            let mut direct_maps = Vec::new();
            let mut shifted_maps = Vec::new();
            let mut weights = Vec::new();

            for leaf in leaves {
                let id = leaf["file_id"].as_str().unwrap().to_owned();
                if !clips.contains_key(&id) {
                    let (clip, digest) = crate::motion::load_clip(&dir, &id).unwrap();
                    assert_eq!(digest, leaf["clip_payload_sha256"].as_str().unwrap());
                    assert_eq!(
                        clip.duration,
                        leaf["clip_duration"].as_f64().unwrap() as f32
                    );
                    clips.insert(id.clone(), (clip, digest));
                }
                let clip = &clips[&id].0;
                let bindings: Vec<NativeTrackBinding> =
                    serde_json::from_value(leaf["bindings"].clone()).unwrap();
                assert!(!bindings.is_empty());
                let time = (leaf["time900"].as_f64().unwrap() + leaf["offset876"].as_f64().unwrap())
                    as f32;
                let time = time.rem_euclid(clip.duration);

                let evaluate = |mode_zero: bool, phase_shift: f32| {
                    let mut effective = bindings.clone();
                    if mode_zero {
                        for binding in &mut effective {
                            binding.mode = 0;
                        }
                    }
                    let pose = rig
                        .local_pose_with_bindings(
                            clip,
                            (time + phase_shift).rem_euclid(clip.duration),
                            &effective,
                        )
                        .unwrap();
                    let mut rotations = HashMap::new();
                    for (index, &hash) in rig.hashes.iter().enumerate() {
                        rotations.insert(hash, pose[index].to_scale_rotation_translation().1);
                    }
                    rotations
                };
                target_maps.push(evaluate(false, 0.0));
                direct_maps.push(evaluate(true, 0.0));
                shifted_maps.push(evaluate(false, 0.25));
                weights.push(leaf["weight"].as_f64().unwrap());
            }

            let expected: HashMap<u32, Quat> = sample["native_rotations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    let hash = row["target_hash"].as_u64().unwrap() as u32;
                    let values = row["xyzw"].as_array().unwrap();
                    let quat = Quat::from_array(std::array::from_fn(|i| {
                        values[i].as_f64().unwrap() as f32
                    }));
                    (hash, quat)
                })
                .collect();
            assert!(expected.len() >= 440, "mapped rotation coverage");
            let common_bound: HashSet<u32> = sample["common_bound_hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|hash| hash.as_u64().unwrap() as u32)
                .collect();
            assert!(common_bound.len() >= 50, "bound rotation coverage");

            let blend_error = |maps: &[HashMap<u32, Quat>]| {
                let mut all_total = 0.0;
                let mut all_count = 0;
                let mut bound_total = 0.0;
                let mut bound_count = 0;
                let mut unbound_total = 0.0;
                let mut unbound_count = 0;
                let mut per_joint = Vec::with_capacity(expected.len());
                for (&hash, &recorded) in &expected {
                    let Some(first) = maps[0].get(&hash).copied() else {
                        continue;
                    };
                    if maps.iter().any(|map| !map.contains_key(&hash)) {
                        continue;
                    }
                    let reference = first.to_array().map(f64::from);
                    let mut sum = [0.0_f64; 4];
                    for (map, &weight) in maps.iter().zip(&weights) {
                        let q = map[&hash].to_array().map(f64::from);
                        let dot: f64 = reference.iter().zip(q).map(|(a, b)| a * b).sum();
                        let sign = if dot < 0.0 { -1.0 } else { 1.0 };
                        for lane in 0..4 {
                            sum[lane] += q[lane] * sign * weight;
                        }
                    }
                    let norm = sum.iter().map(|x| x * x).sum::<f64>().sqrt();
                    let predicted = Quat::from_array(sum.map(|x| (x / norm) as f32));
                    let error = angular_error(predicted, recorded);
                    all_total += error;
                    all_count += 1;
                    per_joint.push((hash, error));
                    if common_bound.contains(&hash) {
                        bound_total += error;
                        bound_count += 1;
                    } else {
                        unbound_total += error;
                        unbound_count += 1;
                    }
                }
                assert!(all_count >= 440, "all uniquely mapped rotations");
                assert!(bound_count >= 50, "common bound rotations");
                (
                    (
                        all_total / all_count as f64,
                        bound_total / bound_count as f64,
                        unbound_total / unbound_count as f64,
                    ),
                    per_joint,
                )
            };

            let (target_stats, mut per_joint) = blend_error(&target_maps);
            let (target, bound_target, unbound_target) = target_stats;
            let (_, bound_direct, _) = blend_error(&direct_maps).0;
            let (_, bound_shifted, _) = blend_error(&shifted_maps).0;
            let worst_all = per_joint
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(hash, error)| format!("{hash:08x}:{error:.2}"))
                .unwrap_or_default();
            per_joint.retain(|(hash, _)| !common_bound.contains(hash));
            per_joint.sort_by(|a, b| b.1.total_cmp(&a.1));
            let worst_unbound = per_joint
                .iter()
                .take(5)
                .map(|(hash, error)| format!("{hash:08x}:{error:.2}"))
                .collect::<Vec<_>>();
            println!(
                "run64 frame {}: {} leaves x {} mapped rotations; all={target:.6} deg; bound={bound_target:.6} deg; unbound={unbound_target:.6} deg; bound mode-zero={bound_direct:.6} deg; bound +250ms={bound_shifted:.6} deg; worst all={worst_all}; worst unbound={worst_unbound:?}",
                sample["frame"],
                leaves.len(),
                expected.len()
            );
            if sample["pose_flags"] == 0x0f00_0000_u64 {
                assert!(target < 0.6, "all mapped joints in final blend stage");
            }
            target_means.push(target);
            bound_target_means.push(bound_target);
            bound_direct_means.push(bound_direct);
            bound_shifted_means.push(bound_shifted);
        }

        let target_mean = target_means.iter().sum::<f64>() / target_means.len() as f64;
        let bound_target_mean =
            bound_target_means.iter().sum::<f64>() / bound_target_means.len() as f64;
        let bound_direct_mean =
            bound_direct_means.iter().sum::<f64>() / bound_direct_means.len() as f64;
        let bound_shifted_mean =
            bound_shifted_means.iter().sum::<f64>() / bound_shifted_means.len() as f64;
        println!(
            "run64 slot+0x280 weighted blend: {} samples x 444 mapped joints; all={target_mean:.6} deg; bound={bound_target_mean:.6} deg; bound mode-zero={bound_direct_mean:.6} deg; bound +250ms={bound_shifted_mean:.6} deg",
            target_means.len()
        );
        assert!(bound_target_means.iter().all(|error| *error < 0.25));
        assert!(bound_direct_mean > bound_target_mean * 5.0 && bound_direct_mean > 0.5);
        assert!(bound_shifted_mean > bound_target_mean * 5.0 && bound_shifted_mean > 0.5);
    }

    #[test]
    #[ignore = "requires original ACU install; measures actual mesh vertices, helper bones and idle playback"]
    fn installed_leg_helpers_and_idle_deform_render_mesh() {
        use crate::options::{Options, Piece};
        use acu_motion::unity::Profile;
        use bevy::mesh::VertexAttributeValues;
        let dir = std::env::var("ACU_GAME_DIR").expect("ACU_GAME_DIR required");
        let dir = std::path::PathBuf::from(dir);
        let options = Options {
            rigged: true,
            scenario: "idle".into(),
            ticks: 0,
            game_dir: dir.clone(),
            pieces: vec![Piece {
                archive: dir.join(crate::options::DEFAULT_ARCHIVE),
                entry: 679,
                resource: 23,
                label: "legs".into(),
            }],
            capture: None,
            frames: 60,
            orbit_deg: 35.0,
            size: (1280, 720),
            texture_archives: vec![],
            no_textures: true,
            report: None,
        };
        let loaded = crate::load::load_all(&options).unwrap();
        let mut library = load(&dir).unwrap();
        assert_eq!(library.leg_helpers.len(), 8);
        let mut skin = library.skin(&loaded.pieces[0]).unwrap();
        let mut meshes = Assets::<Mesh>::default();
        let handle = meshes.add(Mesh::new(
            bevy::render::render_resource::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::MAIN_WORLD,
        ));
        skin.handles.push(handle.clone());
        let (mut matcher, _) = crate::motion::load(&dir, &library).unwrap();
        let positions = |meshes: &Assets<Mesh>| {
            let VertexAttributeValues::Float32x3(p) = meshes
                .get(&handle)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
            else {
                panic!("positions");
            };
            p.clone()
        };
        let thigh_vertices: Vec<_> = skin
            .weights
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                w.joints
                    .iter()
                    .zip(w.weights)
                    .any(|(&i, w)| w > 128 && matches!(skin.hashes[i], 0x6f69051f | 0x0b7ad5a4))
            })
            .map(|(i, _)| i)
            .collect();
        assert!(
            thigh_vertices.len() > 100,
            "must measure vertices actually weighted to thigh helpers"
        );
        for profile in [Profile::Idle, Profile::Walk, Profile::Sprint] {
            matcher.reset();
            matcher.step(profile, 1.0, acu_sim::DT);
            skin.update(&mut meshes, &matcher.pose, false, false);
            let initial = positions(&meshes);
            let mut max_delta = 0.0_f32;
            let mut max_thigh = 0.0_f32;
            for _ in 0..120 {
                matcher.step(profile, 1.0, acu_sim::DT);
                skin.update(&mut meshes, &matcher.pose, false, false);
                let p = positions(&meshes);
                for i in 0..p.len() {
                    let delta = Vec3::from(p[i]).distance(Vec3::from(initial[i]));
                    max_delta = max_delta.max(delta);
                    if thigh_vertices.contains(&i) {
                        max_thigh = max_thigh.max(delta);
                    }
                }
            }
            println!(
                "{profile:?}: rendered leg vertex max={max_delta}; thigh-helper vertices max={max_thigh}"
            );
            assert!(
                max_delta > 0.005,
                "idle and locomotion must deform uploaded mesh vertices"
            );
            if profile != Profile::Idle {
                assert!(
                    max_thigh > 0.1,
                    "thigh geometry must swing, not just shin geometry"
                );
            }
        }
        // Negative control proves this regression detects the original frozen-helper bug.
        matcher.rig.set_helpers(vec![]).unwrap();
        let c = &matcher
            .takes
            .iter()
            .find(|t| t.profile == Profile::Walk)
            .unwrap()
            .clip;
        let i = matcher
            .rig
            .hashes
            .iter()
            .position(|h| *h == 0x6f69051f)
            .unwrap();
        let a = matcher.rig.pose(c, 0.0);
        let mut helper_delta = 0.0_f32;
        for tick in 1..120 {
            let b = matcher.rig.pose(c, tick as f32 / 60.0);
            // Remove moving pelvis; old helper remains rigid in that frame.
            let parent = matcher.rig.parents[i].unwrap();
            let da = a[parent].inverse() * a[i];
            let db = b[parent].inverse() * b[i];
            helper_delta = helper_delta.max(
                da.transform_vector3(Vec3::X)
                    .distance(db.transform_vector3(Vec3::X))
                    .max(
                        da.transform_vector3(Vec3::Y)
                            .distance(db.transform_vector3(Vec3::Y)),
                    ),
            );
        }
        assert!(
            helper_delta < 1e-4,
            "negative control must reproduce a frozen thigh helper"
        );
    }
}
