//! ACU runtime pose records recovered from the live transform routine documented
//! in `docs/findings/ACU-RE-0022-live-bone-pose-buffer.md`.
//!
//! This models the observed 32-byte local record and nonroot hierarchy
//! composition only. The native root branch and coordinate conversion remain
//! caller responsibilities.

use glam::{Quat, Vec3};
use std::collections::HashMap;

pub const ACU_POSE_RECORD_SIZE: usize = 32;
pub const ACU_NAME_HASH_INDEX_ENTRY_SIZE: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcuPoseRecord {
    /// Native bytes 0..16, observed as `(x, y, z, 0)`.
    pub translation: [f32; 4],
    /// Native bytes 16..32, observed in XYZW order.
    pub rotation_xyzw: [f32; 4],
}

/// Hash-to-runtime-index entries read from the ACU skeleton metadata table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcuNameHashIndex {
    pub name_hash: u32,
    pub runtime_index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AcuPoseError {
    InvalidByteLength(usize),
    InvalidHashIndexTableLength(usize),
    DuplicateNativeNameHash(u32),
    DuplicateRuntimeIndex(u32),
    RuntimeIndexOutOfRange(u32),
    MissingRuntimeIndex(usize),
    DuplicatePoseNameHash(u32),
    MissingPoseNameHash(u32),
    NonFiniteRecord(usize),
    InvalidTranslationW(usize),
    ParentCountMismatch { records: usize, parents: usize },
    InvalidRootCount(usize),
    InvalidParent { bone: usize, parent: usize },
    DegenerateRotation(usize),
}

/// Decode the observed ACU 32-byte record sequence without applying coordinate
/// conversion or normalizing the sampled quaternion values.
pub fn decode_records(bytes: &[u8]) -> Result<Vec<AcuPoseRecord>, AcuPoseError> {
    let (chunks, remainder) = bytes.as_chunks::<ACU_POSE_RECORD_SIZE>();
    if !remainder.is_empty() {
        return Err(AcuPoseError::InvalidByteLength(bytes.len()));
    }

    let mut records = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        let mut values = [0.0_f32; 8];
        for (component, value) in values.iter_mut().enumerate() {
            let start = component * 4;
            let raw = &chunk[start..start + 4];
            *value = f32::from_le_bytes(raw.try_into().expect("four-byte float chunk"));
        }
        if !values.iter().all(|value| value.is_finite()) {
            return Err(AcuPoseError::NonFiniteRecord(index));
        }
        if values[3] != 0.0 {
            return Err(AcuPoseError::InvalidTranslationW(index));
        }
        records.push(AcuPoseRecord {
            translation: values[..4].try_into().expect("four translation floats"),
            rotation_xyzw: values[4..].try_into().expect("four quaternion floats"),
        });
    }
    Ok(records)
}

/// Decode little-endian `(name_hash, runtime_index)` pairs.
pub fn decode_name_hash_index_table(bytes: &[u8]) -> Result<Vec<AcuNameHashIndex>, AcuPoseError> {
    let (entries, remainder) = bytes.as_chunks::<ACU_NAME_HASH_INDEX_ENTRY_SIZE>();
    if !remainder.is_empty() {
        return Err(AcuPoseError::InvalidHashIndexTableLength(bytes.len()));
    }
    Ok(entries
        .iter()
        .map(|entry| AcuNameHashIndex {
            name_hash: u32::from_le_bytes(entry[..4].try_into().expect("four-byte hash")),
            runtime_index: u32::from_le_bytes(entry[4..].try_into().expect("four-byte index")),
        })
        .collect())
}

/// Map each runtime index to a matching Rust pose row by exact `name_hash`.
/// Both sides must identify every hash and runtime index uniquely.
pub fn map_runtime_indices_to_pose_rows(
    entries: &[AcuNameHashIndex],
    pose_name_hashes: &[u32],
) -> Result<Vec<usize>, AcuPoseError> {
    let mut pose_rows = HashMap::with_capacity(pose_name_hashes.len());
    for (pose_index, hash) in pose_name_hashes.iter().copied().enumerate() {
        if pose_rows.insert(hash, pose_index).is_some() {
            return Err(AcuPoseError::DuplicatePoseNameHash(hash));
        }
    }

    let mut mapped = vec![None; entries.len()];
    let mut native_hashes = HashMap::with_capacity(entries.len());
    for entry in entries {
        if native_hashes
            .insert(entry.name_hash, entry.runtime_index)
            .is_some()
        {
            return Err(AcuPoseError::DuplicateNativeNameHash(entry.name_hash));
        }
        let runtime_index = usize::try_from(entry.runtime_index)
            .map_err(|_| AcuPoseError::RuntimeIndexOutOfRange(entry.runtime_index))?;
        if runtime_index >= mapped.len() {
            return Err(AcuPoseError::RuntimeIndexOutOfRange(entry.runtime_index));
        }
        if mapped[runtime_index].is_some() {
            return Err(AcuPoseError::DuplicateRuntimeIndex(entry.runtime_index));
        }
        mapped[runtime_index] = Some(
            *pose_rows
                .get(&entry.name_hash)
                .ok_or(AcuPoseError::MissingPoseNameHash(entry.name_hash))?,
        );
    }

    mapped
        .into_iter()
        .enumerate()
        .map(|(index, pose_index)| pose_index.ok_or(AcuPoseError::MissingRuntimeIndex(index)))
        .collect()
}

/// Compose the recovered parent-before-child subset. `root_pose` is supplied
/// by the caller because the native routine has a distinct root branch.
pub fn compose_hierarchy(
    local: &[AcuPoseRecord],
    parents: &[Option<usize>],
    root_pose: AcuPoseRecord,
) -> Result<Vec<AcuPoseRecord>, AcuPoseError> {
    if local.len() != parents.len() {
        return Err(AcuPoseError::ParentCountMismatch {
            records: local.len(),
            parents: parents.len(),
        });
    }

    let roots: Vec<_> = parents
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect();
    if roots != [0] {
        return Err(AcuPoseError::InvalidRootCount(roots.len()));
    }
    if local.is_empty() {
        return Err(AcuPoseError::InvalidRootCount(0));
    }
    for (bone, record) in local.iter().enumerate() {
        if !record
            .translation
            .iter()
            .chain(record.rotation_xyzw.iter())
            .all(|value| value.is_finite())
        {
            return Err(AcuPoseError::NonFiniteRecord(bone));
        }
        if record.translation[3] != 0.0 {
            return Err(AcuPoseError::InvalidTranslationW(bone));
        }
        normalized_rotation(record.rotation_xyzw, bone)?;
    }
    if !root_pose
        .translation
        .iter()
        .chain(root_pose.rotation_xyzw.iter())
        .all(|value| value.is_finite())
    {
        return Err(AcuPoseError::NonFiniteRecord(0));
    }
    normalized_rotation(root_pose.rotation_xyzw, 0)?;

    let mut composed = Vec::with_capacity(local.len());
    composed.push(root_pose);
    for (bone, parent) in parents.iter().copied().enumerate().skip(1) {
        let Some(parent) = parent.filter(|parent| *parent < bone) else {
            return Err(AcuPoseError::InvalidParent {
                bone,
                parent: parent.unwrap_or(usize::MAX),
            });
        };

        let parent_pose = composed[parent];
        let parent_rotation = normalized_rotation(parent_pose.rotation_xyzw, parent)?;
        let local_rotation = normalized_rotation(local[bone].rotation_xyzw, bone)?;
        let rotation = (parent_rotation * local_rotation).normalize();
        let translation = Vec3::from_array(local[bone].translation[..3].try_into().unwrap());
        let world_translation = Vec3::from_array(parent_pose.translation[..3].try_into().unwrap())
            + parent_rotation * translation;
        composed.push(AcuPoseRecord {
            translation: [
                world_translation.x,
                world_translation.y,
                world_translation.z,
                0.0,
            ],
            rotation_xyzw: rotation.to_array(),
        });
    }
    Ok(composed)
}

fn normalized_rotation(rotation: [f32; 4], bone: usize) -> Result<Quat, AcuPoseError> {
    let quaternion = Quat::from_array(rotation);
    let length_squared = quaternion.length_squared();
    if !length_squared.is_finite() || length_squared <= f32::MIN_POSITIVE {
        return Err(AcuPoseError::DegenerateRotation(bone));
    }
    Ok(quaternion.normalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(translation: [f32; 4], rotation_xyzw: [f32; 4]) -> AcuPoseRecord {
        AcuPoseRecord {
            translation,
            rotation_xyzw,
        }
    }

    #[test]
    fn decodes_translation_then_xyzw_quaternion() {
        let values = [1.25_f32, -2.0, 3.5, 0.0, 0.0, 0.0, 0.70710677, 0.70710677];
        let bytes: Vec<_> = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        assert_eq!(
            decode_records(&bytes).unwrap(),
            vec![record(
                [1.25, -2.0, 3.5, 0.0],
                [0.0, 0.0, 0.70710677, 0.70710677]
            )]
        );
    }

    #[test]
    fn rejects_incomplete_and_nonfinite_records() {
        assert_eq!(
            decode_records(&[0; ACU_POSE_RECORD_SIZE - 1]),
            Err(AcuPoseError::InvalidByteLength(31))
        );
        let mut bytes = [0_u8; ACU_POSE_RECORD_SIZE];
        bytes[..4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(
            decode_records(&bytes),
            Err(AcuPoseError::NonFiniteRecord(0))
        );
    }

    #[test]
    fn rejects_nonzero_translation_w() {
        let values = [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let bytes: Vec<_> = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        assert_eq!(
            decode_records(&bytes),
            Err(AcuPoseError::InvalidTranslationW(0))
        );
    }

    #[test]
    fn decodes_and_maps_sorted_hash_indices_to_pose_rows() {
        let entries = [(0x10_u32, 1_u32), (0x20, 2), (0x30, 0)];
        let bytes: Vec<_> = entries
            .iter()
            .flat_map(|(hash, index)| hash.to_le_bytes().into_iter().chain(index.to_le_bytes()))
            .collect();
        let decoded = decode_name_hash_index_table(&bytes).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[0].name_hash, 0x10);
        assert_eq!(decoded[0].runtime_index, 1);
        assert_eq!(
            map_runtime_indices_to_pose_rows(&decoded, &[0x10, 0x20, 0x30]).unwrap(),
            [2, 0, 1]
        );
        assert_eq!(
            decode_name_hash_index_table(&bytes[..bytes.len() - 1]),
            Err(AcuPoseError::InvalidHashIndexTableLength(bytes.len() - 1))
        );
    }

    #[test]
    fn rejects_ambiguous_and_missing_hash_index_mappings() {
        let entry = |name_hash, runtime_index| AcuNameHashIndex {
            name_hash,
            runtime_index,
        };
        assert_eq!(
            map_runtime_indices_to_pose_rows(&[entry(1, 0), entry(2, 0)], &[1, 2]),
            Err(AcuPoseError::DuplicateRuntimeIndex(0))
        );
        assert_eq!(
            map_runtime_indices_to_pose_rows(&[entry(1, 0)], &[1, 1]),
            Err(AcuPoseError::DuplicatePoseNameHash(1))
        );
        assert_eq!(
            map_runtime_indices_to_pose_rows(&[entry(1, 0)], &[2]),
            Err(AcuPoseError::MissingPoseNameHash(1))
        );
        assert_eq!(
            map_runtime_indices_to_pose_rows(&[entry(1, 2)], &[1]),
            Err(AcuPoseError::RuntimeIndexOutOfRange(2))
        );
    }

    #[test]
    fn composes_child_translation_and_rotation_from_parent() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let local = [
            record([0.0; 4], [0.0, 0.0, 0.0, 1.0]),
            record([0.0, 2.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        ];
        let root = record([1.0, 0.0, 0.0, 0.0], [0.0, 0.0, half, half]);
        let result = compose_hierarchy(&local, &[None, Some(0)], root).unwrap();
        assert!((result[1].translation[0] + 1.0).abs() < 1e-6);
        assert!(result[1].translation[1].abs() < 1e-6);
        assert!((result[1].rotation_xyzw[2] - half).abs() < 1e-6);
        assert!((result[1].rotation_xyzw[3] - half).abs() < 1e-6);
    }

    #[test]
    fn rejects_non_topological_parent_indices() {
        let identity = record([0.0; 4], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            compose_hierarchy(&[identity, identity], &[None, Some(1)], identity),
            Err(AcuPoseError::InvalidParent { bone: 1, parent: 1 })
        );
    }
}
