//! Per-frame evaluation order of skeleton bone modifiers, recovered in
//! `docs/findings/ACU-RE-0028-model-pose-bone-modifiers.md` ("Evaluation order").
//!
//! Implemented: the per-LOD list build (`0x1400210a0`: depth buckets from an
//! ancestor stack, LOD filter, stable sort by the u16 at modifier `+36`), the
//! frame walk with its bone skip list (`0x1400377e0`) and the per-bone run with
//! its channel-weighted post-blend (`0x1400379e0`). Not modelled: the bone-ref
//! refresh (`0x14002baa0`), the side effect after a set (`0x14003adc0`) and the
//! pose object's reference transform in the native getters/setters (rotation
//! interpolation and lerp commute with it; rounding may differ).

use std::ops::Range;

use crate::bone_modifier::{Joint, ModelPose, quat_interp};

/// Native hierarchy root marker (u16 parent `0xFFFF`).
const NO_PARENT: u16 = 0xFFFF;

/// Number of per-LOD lists the builder fills (`component + 720 + 12 * m`, m = 0..5).
pub const LOD_LISTS: usize = 6;

/// Native `1/255` factor applied to the run channel byte (`0.0039215689f`,
/// bits `0x3b808081`).
const CHANNEL_SCALE: f32 = f32::from_bits(0x3b80_8081);

#[derive(Clone, Debug, PartialEq)]
pub enum OrderError {
    /// The ancestor stack emptied: the hierarchy is not depth-first.
    NotDepthFirst { joint: usize },
    /// A modifier's host or bone index is outside the hierarchy.
    JointOutOfRange { modifier: usize, joint: usize },
    /// The frame LOD selector indexes past the built lists (native 6 and 7).
    UnbuiltLod { level: u8 },
}

impl std::fmt::Display for OrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for OrderError {}

/// One hierarchy entry as the builder reads it: parent (`+0`, `0xFFFF` = root)
/// and the LOD bits 4..6 of byte `+10`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HierarchyEntry {
    pub parent: u16,
    pub lod: u8,
}

/// One modifier as listed under its host joint's bone object, in that bone's
/// array order. `bone` is the resolved index of the modifier's bone ref
/// (`+16`; `None` when the ref is invalid, `0x14007c170`), `key` the u16 at
/// `+36`, `enabled` bit 0 of byte `+38`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModifierSlot {
    pub host: usize,
    pub bone: Option<usize>,
    pub key: u16,
    pub enabled: bool,
}

/// The builder's bucket index per joint: the position of the joint's parent in
/// an ancestor stack seeded with `0xFFFF`. A parent equal to the previous joint
/// is pushed; any other parent pops the stack down to it. For a depth-first
/// hierarchy this is the joint depth (root = 0).
pub fn depth_buckets(hierarchy: &[HierarchyEntry]) -> Result<Vec<usize>, OrderError> {
    let mut stack = vec![NO_PARENT];
    let mut buckets = Vec::with_capacity(hierarchy.len());
    for (joint, entry) in hierarchy.iter().enumerate() {
        let top = stack.len() - 1;
        if stack[top] != entry.parent {
            if joint > 0 && usize::from(entry.parent) == joint - 1 {
                stack.push(entry.parent);
            } else {
                while stack.last().is_some_and(|&p| p != entry.parent) {
                    stack.pop();
                }
                if stack.is_empty() {
                    return Err(OrderError::NotDepthFirst { joint });
                }
            }
        }
        buckets.push(stack.len() - 1);
    }
    Ok(buckets)
}

/// `0x1400210a0`: list `m` holds, in bucket order and then host/array order,
/// every modifier with a valid bone ref whose bone LOD is at most `m`; the
/// list is then stable-sorted by `key` (`0x14000e3c0`, insertion sort on u16
/// `+36` under a merge sort). Returns indices into `slots`.
pub fn build_lod_lists(
    hierarchy: &[HierarchyEntry],
    slots: &[ModifierSlot],
) -> Result<[Vec<usize>; LOD_LISTS], OrderError> {
    let buckets = depth_buckets(hierarchy)?;
    let mut by_bucket: Vec<Vec<usize>> = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        for joint in [Some(slot.host), slot.bone].into_iter().flatten() {
            if joint >= hierarchy.len() {
                return Err(OrderError::JointOutOfRange {
                    modifier: index,
                    joint,
                });
            }
        }
        let bucket = buckets[slot.host];
        if by_bucket.len() <= bucket {
            by_bucket.resize_with(bucket + 1, Vec::new);
        }
        by_bucket[bucket].push(index);
    }
    // Native buckets keep host order because hosts are visited in hierarchy order.
    for bucket in &mut by_bucket {
        bucket.sort_by_key(|&i| slots[i].host);
    }
    let mut lists: [Vec<usize>; LOD_LISTS] = Default::default();
    for (level, list) in lists.iter_mut().enumerate() {
        for &index in by_bucket.iter().flatten() {
            if let Some(bone) = slots[index].bone
                && usize::from(hierarchy[bone].lod) <= level
            {
                list.push(index);
            }
        }
        list.sort_by_key(|&i| slots[i].key);
    }
    Ok(lists)
}

/// One consecutive group of list entries sharing a bone (`0x1400379e0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoneRun {
    pub bone: usize,
    /// Range into the LOD list.
    pub entries: Range<usize>,
}

/// `0x1400377e0` grouping: the list is cut into runs of equal bone index; a
/// run whose bone is in `skip` (the optional u16 list argument) is passed over.
/// Every listed slot has a valid bone (the builder drops invalid refs).
pub fn bone_runs(list: &[usize], slots: &[ModifierSlot], skip: &[u16]) -> Vec<BoneRun> {
    let bone_of = |position: usize| {
        slots[list[position]]
            .bone
            .expect("listed slots have a bone")
    };
    let mut runs = Vec::new();
    let mut start = 0;
    while start < list.len() {
        let bone = bone_of(start);
        let mut end = start + 1;
        while end < list.len() && bone_of(end) == bone {
            end += 1;
        }
        if !skip.iter().any(|&s| usize::from(s) == bone) {
            runs.push(BoneRun {
                bone,
                entries: start..end,
            });
        }
        start = end;
    }
    runs
}

/// `0x1400379e0` tail: unless `channel` is `0xFF`, pull the run's bone back
/// toward its transform at run start by `channel / 255`. Rotation is set first
/// (`0x140081a50`: `0x140099880` + `0x140097ea0`), then position
/// (`0x1400818f0`: `0x140099750` + `0x140097ea0`); each set recomposes descendants.
pub fn run_post_blend(pose: &mut ModelPose, bone: usize, start: Joint, channel: u8) {
    if channel == 0xFF {
        return;
    }
    let weight = f32::from(channel) * CHANNEL_SCALE;
    let end = pose.model[bone];
    let rotation = quat_interp(start.rotation, end.rotation, weight);
    let position = (end.position - start.position) * weight + start.position;
    pose.set_model(
        bone,
        Joint {
            position: end.position,
            rotation,
        },
    );
    let current = pose.model[bone].rotation;
    pose.set_model(
        bone,
        Joint {
            position,
            rotation: current,
        },
    );
}

/// One frame of modifier evaluation (`0x1400377e0`). `level` is the selector
/// `(u16 [component + 232] >> 3) & 7`; 0 evaluates nothing. `channels` holds
/// the per-joint run weight bytes of the local pose object (`0x140076310` via
/// `0x140093080`). `apply` runs one enabled modifier (vtable slot 11) and
/// receives its slot index. Returns the runs evaluated.
pub fn evaluate_frame(
    pose: &mut ModelPose,
    lists: &[Vec<usize>; LOD_LISTS],
    slots: &[ModifierSlot],
    level: u8,
    skip: &[u16],
    channels: &[u8],
    mut apply: impl FnMut(usize, &mut ModelPose),
) -> Result<Vec<BoneRun>, OrderError> {
    if level == 0 {
        return Ok(Vec::new());
    }
    let list = lists
        .get(usize::from(level))
        .ok_or(OrderError::UnbuiltLod { level })?;
    let runs = bone_runs(list, slots, skip);
    for run in &runs {
        let start = pose.model[run.bone];
        for &index in &list[run.entries.clone()] {
            if slots[index].enabled {
                apply(index, pose);
            }
        }
        run_post_blend(pose, run.bone, start, channels[run.bone]);
    }
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec3};

    fn entry(parent: u16, lod: u8) -> HierarchyEntry {
        HierarchyEntry { parent, lod }
    }

    // 0 ─┬─ 1 ── 2 ── 3
    //    └─ 4 ── 5
    fn tree() -> Vec<HierarchyEntry> {
        vec![
            entry(NO_PARENT, 0),
            entry(0, 0),
            entry(1, 1),
            entry(2, 2),
            entry(0, 0),
            entry(4, 3),
        ]
    }

    fn slot(host: usize, key: u16) -> ModifierSlot {
        ModifierSlot {
            host,
            bone: Some(host),
            key,
            enabled: true,
        }
    }

    #[test]
    fn buckets_are_depths_for_a_depth_first_tree() {
        assert_eq!(depth_buckets(&tree()).unwrap(), vec![0, 1, 2, 3, 1, 2]);
        // A parent that is neither the stack top nor the previous joint and
        // is not an ancestor empties the stack.
        let bad = vec![entry(NO_PARENT, 0), entry(0, 0), entry(5, 0)];
        assert_eq!(
            depth_buckets(&bad),
            Err(OrderError::NotDepthFirst { joint: 2 })
        );
    }

    #[test]
    fn lists_are_depth_ordered_then_stable_sorted_by_key() {
        // Hosts in deliberately non-depth order of creation; equal keys keep
        // depth order (5 at depth 2 before 3 at depth 3), smaller keys lead.
        let slots = vec![slot(3, 7), slot(5, 7), slot(1, 9), slot(2, 1), slot(4, 7)];
        let lists = build_lod_lists(&tree(), &slots).unwrap();
        // Independent expectation for level 5 (all bones): key 1 {2}, key 7
        // {4 (d1), 5 (d2), 3 (d3)}, key 9 {1}.
        let hosts: Vec<usize> = lists[5].iter().map(|&i| slots[i].host).collect();
        assert_eq!(hosts, vec![2, 4, 5, 3, 1]);
        // LOD filter: level 1 keeps bones with lod <= 1 (1, 2, 4).
        let hosts: Vec<usize> = lists[1].iter().map(|&i| slots[i].host).collect();
        assert_eq!(hosts, vec![2, 4, 1]);
        assert!(lists[0].iter().all(|&i| tree()[slots[i].host].lod == 0));
    }

    #[test]
    fn invalid_refs_are_dropped_and_out_of_range_rejected() {
        let mut slots = vec![slot(1, 0), slot(2, 0)];
        slots[1].bone = None;
        let lists = build_lod_lists(&tree(), &slots).unwrap();
        assert_eq!(lists[5], vec![0]);
        slots[0].bone = Some(9);
        assert_eq!(
            build_lod_lists(&tree(), &slots),
            Err(OrderError::JointOutOfRange {
                modifier: 0,
                joint: 9
            })
        );
    }

    #[test]
    fn runs_group_equal_bones_and_honour_the_skip_list() {
        let slots = vec![slot(1, 0), slot(1, 0), slot(2, 0), slot(1, 0)];
        let list = vec![0, 1, 2, 3];
        let runs = bone_runs(&list, &slots, &[]);
        assert_eq!(
            runs,
            vec![
                BoneRun {
                    bone: 1,
                    entries: 0..2
                },
                BoneRun {
                    bone: 2,
                    entries: 2..3
                },
                BoneRun {
                    bone: 1,
                    entries: 3..4
                },
            ]
        );
        let runs = bone_runs(&list, &slots, &[1]);
        assert_eq!(
            runs,
            vec![BoneRun {
                bone: 2,
                entries: 2..3
            }]
        );
    }

    fn chain_pose() -> ModelPose {
        let joint = |x: f32| Joint {
            position: Vec3::new(x, 0.0, 0.0),
            rotation: Quat::IDENTITY,
        };
        ModelPose::new(
            vec![None, Some(0), Some(1)],
            vec![joint(0.0), joint(1.0), joint(1.0)],
        )
        .unwrap()
    }

    #[test]
    fn post_blend_scales_the_run_edit_by_the_channel() {
        let target = Joint {
            position: Vec3::new(1.0, 2.0, 0.0),
            rotation: Quat::from_rotation_z(1.0),
        };
        let mut edited = chain_pose();
        let start = edited.model[1];
        edited.set_model(1, target);

        // 0xFF keeps the full edit.
        let mut pose = edited.clone();
        run_post_blend(&mut pose, 1, start, 0xFF);
        assert_eq!(pose.model, edited.model);

        // 0 restores the run-start transform and the child follows.
        let mut pose = edited.clone();
        run_post_blend(&mut pose, 1, start, 0);
        let original = chain_pose();
        for (a, b) in pose.model.iter().zip(&original.model) {
            assert!(a.position.distance(b.position) < 1e-6);
            assert!(a.rotation.angle_between(b.rotation) < 1e-6);
        }

        // 51/255 = 0.2: independent slerp/lerp expectation.
        let mut pose = edited.clone();
        run_post_blend(&mut pose, 1, start, 51);
        let expected_q = start.rotation.slerp(target.rotation, 0.2);
        let expected_p = start.position.lerp(target.position, 0.2);
        assert!(pose.model[1].rotation.angle_between(expected_q) < 1e-5);
        assert!(pose.model[1].position.distance(expected_p) < 1e-6);
        let child = pose.model[1].position + pose.model[1].rotation * Vec3::X;
        assert!(pose.model[2].position.distance(child) < 1e-5);
    }

    #[test]
    fn frame_applies_enabled_slots_in_list_order() {
        let hierarchy = vec![entry(NO_PARENT, 0), entry(0, 0), entry(1, 0)];
        let mut slots = vec![slot(2, 5), slot(1, 5), slot(1, 0)];
        slots[1].enabled = false;
        let lists = build_lod_lists(&hierarchy, &slots).unwrap();
        let mut pose = chain_pose();
        let mut seen = Vec::new();
        let runs = evaluate_frame(&mut pose, &lists, &slots, 1, &[], &[0xFF; 3], |i, _| {
            seen.push(i)
        })
        .unwrap();
        // key 0 {slot 2}, then key 5 in depth order {slot 1 (d1, disabled), slot 0 (d2)}.
        assert_eq!(seen, vec![2, 0]);
        assert_eq!(runs.iter().map(|r| r.bone).collect::<Vec<_>>(), vec![1, 2]);
        assert!(
            evaluate_frame(
                &mut pose,
                &lists,
                &slots,
                0,
                &[],
                &[0xFF; 3],
                |_, _| panic!()
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            evaluate_frame(&mut pose, &lists, &slots, 6, &[], &[0xFF; 3], |_, _| ()),
            Err(OrderError::UnbuiltLod { level: 6 })
        );
    }
}
