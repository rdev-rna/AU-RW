//! ACU skeletons and evidenced humanoid inputs. ACU-RE-0010 / ACU-RE-0014.
pub mod humanoid;
pub mod pose;
use crate::{Limits, Result, invalid, reader::Reader};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub const TYPE_SKELETON: &str = "24aecb7c";
pub const CLASS_SKELETON: u32 = 0x24ae_cb7c;
pub const CLASS_BONE: u32 = 0x9574_1049;

#[derive(Debug, Serialize)]
pub enum Pointer {
    Null,
    Id(String),
    Link(String),
    Inline {
        tag: u8,
        managed: Option<bool>,
        object: Box<humanoid::Object>,
    },
}

impl Pointer {
    fn id(&self) -> Option<&str> {
        match self {
            Self::Null => None,
            Self::Id(id) | Self::Link(id) => Some(id),
            Self::Inline { object, .. } => Some(&object.file_id),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileReference {
    pub tag: u8,
    pub global: u8,
    pub file_id: String,
}

#[derive(Debug, Serialize)]
pub struct Bone {
    pub file_id: String,
    pub name_hash: u32,
    pub parent: Pointer,
    pub mirror: Pointer,
    pub global_position: [f32; 4],
    pub global_rotation: [f32; 4],
    pub local_position: [f32; 4],
    pub local_rotation: [f32; 4],
    pub solving_priority: u8,
    pub mirroring_type: u32,
    pub modifiers: Vec<humanoid::Modifier>,
    pub dependency_name_hashes: Vec<u32>,
    pub wrinkle_category: u32,
    pub wrinkle_factor: f32,
    pub index: u16,
    pub children_count: u16,
}

#[derive(Debug, Serialize)]
pub struct Skeleton {
    pub file_id: String,
    pub skeleton_type: u32,
    pub bones: Vec<Bone>,
    pub root: Pointer,
    pub root_local_position: [f32; 4],
    pub root_local_rotation: [f32; 4],
    pub body_part_mapping: FileReference,
    pub pose_groups: Vec<pose::PoseGroup>,
    pub ik_data: Pointer,
    pub com_data: Pointer,
    pub look_at_definition: FileReference,
    pub reflex_system: Pointer,
    pub skeleton_key: u32,
    pub hierarchy_key: u32,
}

fn header(r: &mut Reader<'_>, expected: u32) -> Result<String> {
    let id = format!("{:016x}", r.u64()?);
    if r.u32()? != expected {
        return invalid("unexpected skeleton class hash");
    }
    Ok(id)
}

fn pointer(r: &mut Reader<'_>) -> Result<Pointer> {
    match r.u8()? {
        1 => Ok(Pointer::Id(format!("{:016x}", r.u64()?))),
        2 => Ok(Pointer::Link(format!("{:016x}", r.u64()?))),
        3 => Ok(Pointer::Null),
        _ => invalid("unsupported inline/managed skeleton pointer"),
    }
}

fn pointer_object(
    r: &mut Reader<'_>,
    class: u32,
    managed_class: bool,
    limits: Limits,
) -> Result<Pointer> {
    let tag = r.u8()?;
    match tag {
        1 => Ok(Pointer::Id(format!("{:016x}", r.u64()?))),
        2 => Ok(Pointer::Link(format!("{:016x}", r.u64()?))),
        3 => Ok(Pointer::Null),
        0 | 4 => {
            let managed = if tag == 4 || managed_class {
                Some(humanoid::boolean(r)?)
            } else {
                None
            };
            Ok(Pointer::Inline {
                tag,
                managed,
                object: Box::new(humanoid::object(r, class, limits)?),
            })
        }
        _ => invalid("unsupported skeleton object pointer"),
    }
}

fn reference(r: &mut Reader<'_>) -> Result<FileReference> {
    let tag = r.u8()?;
    let global = r.u8()?;
    if !matches!(tag, 0..=3) {
        return invalid("unsupported skeleton file reference");
    }
    Ok(FileReference {
        tag,
        global,
        file_id: format!("{:016x}", r.u64()?),
    })
}

fn float(r: &mut Reader<'_>) -> Result<f32> {
    let value = f32::from_bits(r.u32()?);
    if !value.is_finite() {
        return invalid("nonfinite skeleton transform/scalar");
    }
    Ok(value)
}

fn vector(r: &mut Reader<'_>) -> Result<[f32; 4]> {
    Ok([float(r)?, float(r)?, float(r)?, float(r)?])
}

/// Read one embedded Bone beginning at its u64 ID/u32 class hash.
/// Unknown modifier schemas fail explicitly; see ACU-RE-0014 for supported classes.
pub fn parse_bone(r: &mut Reader<'_>, limits: Limits) -> Result<Bone> {
    let file_id = header(r, CLASS_BONE)?;
    let name_hash = r.u32()?;
    let parent = pointer(r)?;
    if parent.id() == Some(file_id.as_str()) {
        return invalid("self-parented skeleton bone");
    }
    let mirror = pointer(r)?;
    let global_position = vector(r)?;
    let global_rotation = vector(r)?;
    let local_position = vector(r)?;
    let local_rotation = vector(r)?;
    let solving_priority = r.u8()?;
    let mirroring_type = r.u32()?;
    let modifier_count = humanoid::count(r, limits, 19)?;
    let mut modifiers = Vec::with_capacity(modifier_count);
    for _ in 0..modifier_count {
        modifiers.push(humanoid::modifier(r, limits)?);
    }
    let count = r.u32()? as usize;
    if count > limits.max_resources || count > r.remaining() / 4 {
        return invalid("skeleton dependency count exceeds bounds");
    }
    let mut dependency_name_hashes = Vec::with_capacity(count);
    for _ in 0..count {
        dependency_name_hashes.push(r.u32()?);
    }
    Ok(Bone {
        file_id,
        name_hash,
        parent,
        mirror,
        global_position,
        global_rotation,
        local_position,
        local_rotation,
        solving_priority,
        mirroring_type,
        modifiers,
        dependency_name_hashes,
        wrinkle_category: r.u32()?,
        wrinkle_factor: float(r)?,
        index: r.u16()?,
        children_count: r.u16()?,
    })
}

/// Full resource payload; unsupported nested classes fail rather than becoming opaque successes.
pub fn parse(bytes: &[u8], expected_id: &str, limits: Limits) -> Result<Skeleton> {
    if bytes.len() > limits.max_decoded {
        return invalid("skeleton payload limit exceeded");
    }
    let mut r = Reader::new(bytes);
    if r.u8()? != 1 {
        return invalid("unsupported skeleton resource prefix");
    }
    let file_id = header(&mut r, CLASS_SKELETON)?;
    if file_id != expected_id {
        return invalid("skeleton resource ID mismatch");
    }
    let skeleton_type = r.u32()?;
    let count = r.u32()? as usize;
    if count > limits.max_resources || count > r.remaining() / 110 {
        return invalid("skeleton bone count exceeds bounds");
    }
    let mut bones = Vec::with_capacity(count);
    for _ in 0..count {
        if r.u8()? != 0 {
            return invalid("unsupported skeleton bone prefix");
        }
        bones.push(parse_bone(&mut r, limits)?);
    }
    let skeleton = Skeleton {
        file_id,
        skeleton_type,
        bones,
        root: pointer(&mut r)?,
        root_local_position: vector(&mut r)?,
        root_local_rotation: vector(&mut r)?,
        body_part_mapping: reference(&mut r)?,
        pose_groups: pose::groups(&mut r, limits)?,
        ik_data: pointer_object(&mut r, humanoid::IK_DATA, false, limits)?,
        com_data: pointer_object(&mut r, humanoid::COM_DATA, false, limits)?,
        look_at_definition: reference(&mut r)?,
        reflex_system: pointer_object(&mut r, humanoid::REFLEX_SYSTEM, true, limits)?,
        skeleton_key: r.u32()?,
        hierarchy_key: r.u32()?,
    };
    if r.remaining() != 0 {
        return invalid("unconsumed skeleton bytes");
    }
    let mut ids = HashSet::new();
    let mut indices = HashSet::new();
    for bone in &skeleton.bones {
        if !ids.insert(bone.file_id.as_str()) || !indices.insert(bone.index) {
            return invalid("duplicate skeleton bone ID/index");
        }
    }
    // Additive skeleton parent links may deliberately target external bones.
    let by_id: HashMap<_, _> = skeleton
        .bones
        .iter()
        .enumerate()
        .map(|(i, b)| (b.file_id.as_str(), i))
        .collect();
    let mut state = vec![0u8; skeleton.bones.len()];
    for start in 0..skeleton.bones.len() {
        if state[start] == 2 {
            continue;
        }
        let mut path = Vec::new();
        let mut current = Some(start);
        while let Some(i) = current {
            if state[i] == 2 {
                break;
            }
            if state[i] == 1 {
                return invalid("skeleton parent cycle");
            }
            state[i] = 1;
            path.push(i);
            current = skeleton.bones[i]
                .parent
                .id()
                .and_then(|id| by_id.get(id))
                .copied();
        }
        for i in path {
            state[i] = 2;
        }
    }
    Ok(skeleton)
}
