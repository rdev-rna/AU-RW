//! Serialized humanoid inputs, ACU-RE-0014. No runtime modifier evaluation.
use super::{Pointer, float, header, pointer, pointer_object, vector};
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;

pub const CONSTRAINT_TARGET: u32 = 0xdf63_8110;
pub const IK_DATA: u32 = 0x9a33_101e;
pub const COM_DATA: u32 = 0x9a67_179a;
pub const REFLEX_SYSTEM: u32 = 0xd417_bb8c;

#[derive(Debug, Serialize)]
pub struct Modifier {
    pub file_id: String,
    pub class_hash: String,
    pub owner: Pointer,
    pub blend_weight: f32,
    pub use_modified_bone_matrix: bool,
    pub data: ModifierData,
}

#[derive(Debug, Serialize)]
pub enum ModifierData {
    MaxLookAt {
        up_node: Pointer,
        look_at_bone: Pointer,
        position_offset: [f32; 4],
        axes_and_control: [i32; 4],
    },
    OrientationConstraint(Constraint),
    PositionConstraint(Constraint),
    /// Serialized inputs only; ACU-RE-0016 does not establish solver behavior.
    Wrinkle {
        neighbor_bones: Vec<Pointer>,
        center_bone: Pointer,
        lengths: [f32; 3],
        flip: bool,
    },
    Reflex {
        connectors: Vec<ConnectorInfo>,
        ball_joints: Vec<BallJointInfo>,
    },
}

#[derive(Debug, Serialize)]
pub struct Constraint {
    pub targets: Vec<Pointer>,
    pub keep_initial_offset: bool,
    pub offset: [f32; 4],
}

#[derive(Debug, Serialize)]
pub struct ConnectorInfo {
    pub file_id: String,
    pub connector_index: u32,
    pub position_offset: [f32; 4],
    pub rotation_offset: [f32; 4],
}

#[derive(Debug, Serialize)]
pub struct BallJointInfo {
    pub file_id: String,
    pub is_double_ball_joint: bool,
    pub ball_joint_index: u8,
    pub bone_in_pose_index: i16,
}

#[derive(Debug, Serialize)]
pub struct Object {
    pub file_id: String,
    pub data: ObjectData,
}

#[derive(Debug, Serialize)]
pub enum ObjectData {
    IkData,
    ComData {
        default_com: [f32; 4],
        default_com_high_body: [f32; 4],
    },
    ConstraintTarget {
        bone: Pointer,
        weight: f32,
    },
    ReflexSystem(ReflexSystem),
}

#[derive(Debug, Serialize)]
pub struct ReflexSystem {
    pub name: String,
    // Nonempty measurements/elements/connectors/double-joints unsupported.
    pub ball_joints: Vec<ReflexBallJoint>,
    pub bone_name_hashes: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub struct ReflexBallJoint {
    pub file_id: String,
    pub name: String,
    pub socket_rotation_offset: [f32; 4],
    pub parent_bone_slot: i16,
    pub socket_bone_slot: i16,
    pub target_bone_slot: i16,
    pub hull: PoseHull,
    pub pose_set: PoseSet,
}

#[derive(Debug, Serialize)]
pub struct PoseHull {
    pub file_id: String,
    pub data: Vec<[f32; 4]>,
}

#[derive(Debug, Serialize)]
pub struct PackedPoseBone {
    pub file_id: String,
    /// Eight serialized i16 fields; unpacking/solver semantics unresolved.
    pub packed: [i16; 8],
}

#[derive(Debug, Serialize)]
pub struct PoseSet {
    pub file_id: String,
    pub bone_name_hashes: Vec<u32>,
    pub data: Vec<PackedPoseBone>,
    pub pose_count: u32,
    pub position_unpack_scale: [f32; 4],
}

pub(super) fn boolean(r: &mut Reader<'_>) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => invalid("unsupported skeleton boolean value"),
    }
}

pub(super) fn count(r: &mut Reader<'_>, limits: Limits, width: usize) -> Result<usize> {
    let count = r.u32()? as usize;
    if count > limits.max_resources || count > r.remaining() / width {
        return invalid("skeleton nested count exceeds bounds");
    }
    Ok(count)
}

fn string(r: &mut Reader<'_>, limits: Limits) -> Result<String> {
    let length = count(r, limits, 1)?;
    let name = std::str::from_utf8(r.take(length)?)?.to_owned();
    if length != 0 && r.u8()? != 0 {
        return invalid("unsupported skeleton string terminator");
    }
    Ok(name)
}

pub(super) fn modifier(r: &mut Reader<'_>, limits: Limits) -> Result<Modifier> {
    if r.u8()? != 0 {
        return invalid("unsupported bone modifier wrapper");
    }
    let file_id = format!("{:016x}", r.u64()?);
    let class = r.u32()?;
    let owner = pointer(r)?;
    let blend_weight = float(r)?;
    let use_modified_bone_matrix = boolean(r)?;
    let data = match class {
        0x49f4_ca3e => ModifierData::MaxLookAt {
            up_node: pointer(r)?,
            look_at_bone: pointer(r)?,
            position_offset: vector(r)?,
            axes_and_control: [r.i32()?, r.i32()?, r.i32()?, r.i32()?],
        },
        0x7511_6750 | 0xd0c3_4a81 => {
            let n = count(r, limits, 1)?;
            let mut targets = Vec::with_capacity(n);
            for _ in 0..n {
                targets.push(pointer_object(r, CONSTRAINT_TARGET, false, limits)?);
            }
            let constraint = Constraint {
                targets,
                keep_initial_offset: boolean(r)?,
                offset: vector(r)?,
            };
            if class == 0x7511_6750 {
                ModifierData::OrientationConstraint(constraint)
            } else {
                ModifierData::PositionConstraint(constraint)
            }
        }
        0xd12b_891d => {
            let n = count(r, limits, 1)?;
            let mut neighbor_bones = Vec::with_capacity(n);
            for _ in 0..n {
                neighbor_bones.push(pointer(r)?);
            }
            ModifierData::Wrinkle {
                neighbor_bones,
                center_bone: pointer(r)?,
                lengths: [float(r)?, float(r)?, float(r)?],
                flip: boolean(r)?,
            }
        }
        0x057d_aa86 => {
            let n = count(r, limits, 48)?;
            let mut connectors = Vec::with_capacity(n);
            for _ in 0..n {
                connectors.push(ConnectorInfo {
                    file_id: header(r, 0xeb38_4b0a)?,
                    connector_index: r.u32()?,
                    position_offset: vector(r)?,
                    rotation_offset: vector(r)?,
                });
            }
            let n = count(r, limits, 16)?;
            let mut ball_joints = Vec::with_capacity(n);
            for _ in 0..n {
                ball_joints.push(BallJointInfo {
                    file_id: header(r, 0xde5f_5e63)?,
                    is_double_ball_joint: boolean(r)?,
                    ball_joint_index: r.u8()?,
                    bone_in_pose_index: r.i16()?,
                });
            }
            ModifierData::Reflex {
                connectors,
                ball_joints,
            }
        }
        _ => {
            return invalid(format!(
                "unsupported bone modifier {class:08x} at {}",
                r.position
            ));
        }
    };
    Ok(Modifier {
        file_id,
        class_hash: format!("{class:08x}"),
        owner,
        blend_weight,
        use_modified_bone_matrix,
        data,
    })
}

pub(super) fn object(r: &mut Reader<'_>, class: u32, limits: Limits) -> Result<Object> {
    let file_id = header(r, class)?;
    let data = match class {
        IK_DATA => ObjectData::IkData,
        COM_DATA => ObjectData::ComData {
            default_com: vector(r)?,
            default_com_high_body: vector(r)?,
        },
        CONSTRAINT_TARGET => ObjectData::ConstraintTarget {
            bone: pointer(r)?,
            weight: float(r)?,
        },
        REFLEX_SYSTEM => {
            let name = string(r, limits)?;
            for field in ["measurements", "connector elements", "connectors"] {
                if r.u32()? != 0 {
                    return invalid(format!("unsupported Reflex {field}"));
                }
            }
            let n = count(r, limits, 42)?;
            let mut ball_joints = Vec::with_capacity(n);
            for _ in 0..n {
                ball_joints.push(ball_joint(r, limits)?);
            }
            if r.u32()? != 0 {
                return invalid("unsupported Reflex double ball joints");
            }
            let n = count(r, limits, 4)?;
            let mut bone_name_hashes = Vec::with_capacity(n);
            for _ in 0..n {
                bone_name_hashes.push(r.u32()?);
            }
            ObjectData::ReflexSystem(ReflexSystem {
                name,
                ball_joints,
                bone_name_hashes,
            })
        }
        _ => return invalid(format!("unsupported skeleton inline class {class:08x}")),
    };
    Ok(Object { file_id, data })
}

fn ball_joint(r: &mut Reader<'_>, limits: Limits) -> Result<ReflexBallJoint> {
    let file_id = header(r, 0xfbf2_aa16)?;
    let name = string(r, limits)?;
    let socket_rotation_offset = vector(r)?;
    let parent_bone_slot = r.i16()?;
    let socket_bone_slot = r.i16()?;
    let target_bone_slot = r.i16()?;
    let hull_id = header(r, 0x7f67_197e)?;
    let n = count(r, limits, 16)?;
    let mut hull_data = Vec::with_capacity(n);
    for _ in 0..n {
        hull_data.push(vector(r)?);
    }
    let hull = PoseHull {
        file_id: hull_id,
        data: hull_data,
    };
    let set_id = header(r, 0x854d_5ad3)?;
    let n = count(r, limits, 4)?;
    let mut bone_name_hashes = Vec::with_capacity(n);
    for _ in 0..n {
        bone_name_hashes.push(r.u32()?);
    }
    let n = count(r, limits, 28)?;
    let mut data = Vec::with_capacity(n);
    for _ in 0..n {
        let file_id = header(r, 0x6e3e_0317)?;
        let mut packed = [0; 8];
        for value in &mut packed {
            *value = r.i16()?;
        }
        data.push(PackedPoseBone { file_id, packed });
    }
    let pose_count = r.u32()?;
    if pose_count as usize > limits.max_resources {
        return invalid("skeleton pose count exceeds bounds");
    }
    let pose_set = PoseSet {
        file_id: set_id,
        bone_name_hashes,
        data,
        pose_count,
        position_unpack_scale: vector(r)?,
    };
    Ok(ReflexBallJoint {
        file_id,
        name,
        socket_rotation_offset,
        parent_bone_slot,
        socket_bone_slot,
        target_bone_slot,
        hull,
        pose_set,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrinkle_values_and_malformed_boundaries() -> Result<()> {
        // Independent vector: two neighbors (ID and null), center link,
        // asymmetric signed lengths and flip. No original bytes embedded.
        let mut b = vec![0];
        b.extend(17u64.to_le_bytes());
        b.extend(0xd12b891du32.to_le_bytes());
        b.push(3); // null owner
        b.extend(0.5f32.to_le_bytes());
        b.push(0);
        let count_offset = b.len();
        b.extend(2u32.to_le_bytes());
        b.push(1);
        b.extend(42u64.to_le_bytes());
        b.push(3);
        b.push(2);
        b.extend(43u64.to_le_bytes());
        let length_offset = b.len();
        for f in [-1.5f32, 2.25, 7.0] {
            b.extend(f.to_le_bytes());
        }
        b.push(1);
        let mut r = Reader::new(&b);
        let m = modifier(&mut r, Limits::default())?;
        assert_eq!(r.remaining(), 0);
        let ModifierData::Wrinkle {
            neighbor_bones,
            center_bone,
            lengths,
            flip,
        } = m.data
        else {
            panic!("wrinkle class")
        };
        assert_eq!(neighbor_bones.len(), 2);
        assert_eq!(neighbor_bones[0].id(), Some("000000000000002a"));
        assert!(matches!(neighbor_bones[1], Pointer::Null));
        assert_eq!(center_bone.id(), Some("000000000000002b"));
        assert_eq!(lengths, [-1.5, 2.25, 7.0]);
        assert!(flip);
        for n in 0..b.len() {
            assert!(modifier(&mut Reader::new(&b[..n]), Limits::default()).is_err());
        }
        let mut bad = b.clone();
        bad[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(modifier(&mut Reader::new(&bad), Limits::default()).is_err());
        let mut bad = b.clone();
        bad[length_offset..length_offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(modifier(&mut Reader::new(&bad), Limits::default()).is_err());
        let mut bad = b;
        *bad.last_mut().unwrap() = 2;
        assert!(modifier(&mut Reader::new(&bad), Limits::default()).is_err());
        Ok(())
    }

    // Authored independently from the layout table in ACU-RE-0014. A constraint
    // contains one inline target and one unresolved external target.
    fn constraint() -> Vec<u8> {
        let mut b = vec![0];
        b.extend(77u64.to_le_bytes());
        b.extend(0x75116750u32.to_le_bytes());
        b.push(2);
        b.extend(100u64.to_le_bytes());
        b.extend(0.75f32.to_le_bytes());
        b.push(1);
        b.extend(2u32.to_le_bytes());
        b.push(0);
        b.extend(78u64.to_le_bytes());
        b.extend(CONSTRAINT_TARGET.to_le_bytes());
        b.push(2);
        b.extend(101u64.to_le_bytes());
        b.extend(0.25f32.to_le_bytes());
        b.push(1);
        b.extend(79u64.to_le_bytes());
        b.push(1);
        for f in [1f32, 2., 3., 0.] {
            b.extend(f.to_le_bytes());
        }
        b
    }

    #[test]
    fn constraint_targets_preserve_values_and_reject_invalid_inputs() -> Result<()> {
        let b = constraint();
        let mut r = Reader::new(&b);
        let m = modifier(&mut r, Limits::default())?;
        assert_eq!(r.remaining(), 0);
        assert_eq!(m.blend_weight, 0.75);
        let ModifierData::OrientationConstraint(c) = m.data else {
            panic!("constraint class")
        };
        assert_eq!(c.offset, [1., 2., 3., 0.]);
        assert!(c.keep_initial_offset);
        assert_eq!(c.targets[1].id(), Some("000000000000004f"));
        let Pointer::Inline { object, .. } = &c.targets[0] else {
            panic!("inline target")
        };
        let ObjectData::ConstraintTarget { bone, weight } = &object.data else {
            panic!("target class")
        };
        assert_eq!(bone.id(), Some("0000000000000065"));
        assert_eq!(*weight, 0.25);
        for n in 0..b.len() {
            assert!(modifier(&mut Reader::new(&b[..n]), Limits::default()).is_err());
        }
        for (offset, value) in [(9, 0xff), (26, 2), (31, 9)] {
            let mut bad = b.clone();
            bad[offset] = value;
            assert!(modifier(&mut Reader::new(&bad), Limits::default()).is_err());
        }
        let mut bad = b.clone();
        bad[27..31].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(modifier(&mut Reader::new(&bad), Limits::default()).is_err());
        let mut bad = b;
        bad[22..26].copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert!(modifier(&mut Reader::new(&bad), Limits::default()).is_err());
        Ok(())
    }

    #[test]
    fn managed_pointer_and_reflex_variant_boundaries() -> Result<()> {
        let mut b = vec![0, 1]; // managed class, inline and managed true
        b.extend(88u64.to_le_bytes());
        b.extend(REFLEX_SYSTEM.to_le_bytes());
        b.extend(0u32.to_le_bytes()); // empty name has no extra terminator
        b.extend([0u8; 24]); // six empty lists
        let mut r = Reader::new(&b);
        let p = pointer_object(&mut r, REFLEX_SYSTEM, true, Limits::default())?;
        assert_eq!(r.remaining(), 0);
        assert!(matches!(
            p,
            Pointer::Inline {
                managed: Some(true),
                ..
            }
        ));
        for n in 0..b.len() {
            assert!(
                pointer_object(
                    &mut Reader::new(&b[..n]),
                    REFLEX_SYSTEM,
                    true,
                    Limits::default()
                )
                .is_err()
            );
        }
        let mut bad = b.clone();
        bad[1] = 2;
        assert!(
            pointer_object(
                &mut Reader::new(&bad),
                REFLEX_SYSTEM,
                true,
                Limits::default()
            )
            .is_err()
        );
        let mut bad = b;
        bad[18] = 1; // nonempty measurements require a new schema
        assert!(
            pointer_object(
                &mut Reader::new(&bad),
                REFLEX_SYSTEM,
                true,
                Limits::default()
            )
            .unwrap_err()
            .to_string()
            .contains("unsupported Reflex measurements")
        );
        Ok(())
    }
}
