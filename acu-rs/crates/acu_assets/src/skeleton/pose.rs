//! ACU-RE-0016: serialized pose groups; packed channels remain uninterpreted.
use super::{header, humanoid::count};
use crate::{Limits, Result, reader::Reader};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PoseGroup {
    pub file_id: String,
    pub poses: Vec<Pose>,
}

#[derive(Debug, Serialize)]
pub struct Pose {
    pub file_id: String,
    pub pose_id: u32,
    pub bones: Vec<PoseBone>,
}

#[derive(Debug, Serialize)]
pub struct PoseBone {
    pub file_id: String,
    pub bone_id: u32,
    pub packed_position: [i16; 3],
    pub packed_rotation: [i16; 3],
}

pub(super) fn groups(r: &mut Reader<'_>, limits: Limits) -> Result<Vec<PoseGroup>> {
    let n = count(r, limits, 16)?;
    let mut groups = Vec::with_capacity(n);
    for _ in 0..n {
        let file_id = header(r, 0xc8f8_abff)?;
        let n = count(r, limits, 20)?;
        let mut poses = Vec::with_capacity(n);
        for _ in 0..n {
            let file_id = header(r, 0x209c_f072)?;
            let pose_id = r.u32()?;
            let n = count(r, limits, 28)?;
            let mut bones = Vec::with_capacity(n);
            for _ in 0..n {
                bones.push(PoseBone {
                    file_id: header(r, 0x071c_ee50)?,
                    bone_id: r.u32()?,
                    packed_position: [r.i16()?, r.i16()?, r.i16()?],
                    packed_rotation: [r.i16()?, r.i16()?, r.i16()?],
                });
            }
            poses.push(Pose {
                file_id,
                pose_id,
                bones,
            });
        }
        groups.push(PoseGroup { file_id, poses });
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_pose_values_and_invalid_boundaries() -> Result<()> {
        // One group, two poses; asymmetric signed boundary values and an empty
        // second pose provide independent width/order expectations.
        let mut b = 1u32.to_le_bytes().to_vec();
        b.extend(11u64.to_le_bytes());
        b.extend(0xc8f8abffu32.to_le_bytes());
        b.extend(2u32.to_le_bytes());
        b.extend(12u64.to_le_bytes());
        b.extend(0x209cf072u32.to_le_bytes());
        b.extend(99u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(13u64.to_le_bytes());
        b.extend(0x071cee50u32.to_le_bytes());
        b.extend(100u32.to_le_bytes());
        for value in [i16::MIN, -1, 7, i16::MAX, 23, -4] {
            b.extend(value.to_le_bytes());
        }
        b.extend(14u64.to_le_bytes());
        b.extend(0x209cf072u32.to_le_bytes());
        b.extend(98u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        let mut r = Reader::new(&b);
        let g = groups(&mut r, Limits::default())?;
        assert_eq!(r.remaining(), 0);
        assert_eq!(g[0].poses.len(), 2);
        assert_eq!(g[0].poses[0].pose_id, 99);
        let bone = &g[0].poses[0].bones[0];
        assert_eq!(bone.file_id, "000000000000000d");
        assert_eq!(bone.bone_id, 100);
        assert_eq!(bone.packed_position, [i16::MIN, -1, 7]);
        assert_eq!(bone.packed_rotation, [i16::MAX, 23, -4]);
        assert!(g[0].poses[1].bones.is_empty());
        for n in 0..b.len() {
            assert!(groups(&mut Reader::new(&b[..n]), Limits::default()).is_err());
        }
        for offset in [0, 16, 36] {
            // each nested count
            let mut bad = b.clone();
            bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(groups(&mut Reader::new(&bad), Limits::default()).is_err());
        }
        for offset in [12, 28, 48] {
            // each class header
            let mut bad = b.clone();
            bad[offset..offset + 4].fill(0xff);
            assert!(groups(&mut Reader::new(&bad), Limits::default()).is_err());
        }
        Ok(())
    }
}
