//! Reader for `acu_tap` event records (`tools/native_tap/tap.cpp`) and the
//! native pose/modifier layouts in `docs/findings/ACU-RE-0028-model-pose-bone-modifiers.md`.
//! Mode A replay: the game's own stage inputs are decoded here and run through
//! `acu_motion`; nothing in this crate decides behavior.
#![forbid(unsafe_code)]

use acu_motion::bone_modifier::{
    ConstraintTarget, Hinge, HingeSphere, Joint, MaxLookAt, OrientationConstraint,
    PositionConstraint,
};
use acu_motion::reflex::{PoseFrame, Reflex, ReflexConnector, ReflexLookup};
use glam::{Quat, Vec3, Vec4};

pub mod coat;
pub mod stats;

#[derive(Clone, Debug, PartialEq)]
pub enum BridgeError {
    Truncated { offset: usize, need: usize },
    BadMagic { offset: usize },
    BadSize { offset: usize, size: u32 },
    Layout(&'static str),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BridgeError {}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// `EvHdr` (packed): magic, size, seq, qpc, tid, hook, phase, nsnap, a[4], rsp, ret, rax.
pub const EVENT_MAGIC: u32 = 0x4554_4341;
const EVENT_HEADER: usize = 4 + 4 + 8 + 8 + 4 + 2 + 1 + 1 + 32 + 24;
const SNAP_HEADER: usize = 8;

#[derive(Clone, Debug)]
pub struct TapEvent {
    pub seq: u64,
    pub qpc: u64,
    pub tid: u32,
    pub hook: u16,
    /// 0 = entry, 1 = exit.
    pub phase: u8,
    pub args: [u64; 4],
    pub rsp: u64,
    pub ret: u64,
    pub rax: u64,
    /// `(snap_id, bytes)`; `None` when the in-process copy faulted.
    pub snaps: Vec<(u16, Option<Vec<u8>>)>,
}

impl TapEvent {
    pub fn snap(&self, id: u16) -> Option<&[u8]> {
        self.snaps
            .iter()
            .find(|(sid, _)| *sid == id)
            .and_then(|(_, b)| b.as_deref())
    }
}

/// Parse a `DUMP` file. Stops with an error on the first malformed record;
/// a dump is written by one bump allocator, so records are contiguous.
pub fn parse_events(data: &[u8]) -> Result<Vec<TapEvent>, BridgeError> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < data.len() {
        let need = |n: usize| {
            if off.checked_add(n).is_some_and(|e| e <= data.len()) {
                Ok(())
            } else {
                Err(BridgeError::Truncated {
                    offset: off,
                    need: n,
                })
            }
        };
        need(EVENT_HEADER)?;
        let h = &data[off..];
        if u32_at(h, 0) != EVENT_MAGIC {
            return Err(BridgeError::BadMagic { offset: off });
        }
        let size = u32_at(h, 4);
        if (size as usize) < EVENT_HEADER {
            return Err(BridgeError::BadSize { offset: off, size });
        }
        need(size as usize)?;
        let rec = &data[off..off + size as usize];
        let nsnap = rec[31] as usize;
        let mut p = EVENT_HEADER;
        let mut snaps = Vec::with_capacity(nsnap);
        for _ in 0..nsnap {
            if p + SNAP_HEADER > rec.len() {
                return Err(BridgeError::BadSize { offset: off, size });
            }
            let (id, ok, len) = (
                u16_at(rec, p),
                u16_at(rec, p + 2),
                u32_at(rec, p + 4) as usize,
            );
            p += SNAP_HEADER;
            let padded = len
                .checked_add(7)
                .ok_or(BridgeError::BadSize { offset: off, size })?
                & !7;
            if p + padded > rec.len() {
                return Err(BridgeError::BadSize { offset: off, size });
            }
            snaps.push((id, (ok != 0).then(|| rec[p..p + len].to_vec())));
            p += padded;
        }
        let a = |i: usize| u64_at(rec, 32 + 8 * i);
        out.push(TapEvent {
            seq: u64_at(rec, 8),
            qpc: u64_at(rec, 16),
            tid: u32_at(rec, 24),
            hook: u16_at(rec, 28),
            phase: rec[30],
            args: [a(0), a(1), a(2), a(3)],
            rsp: u64_at(rec, 64),
            ret: u64_at(rec, 72),
            rax: u64_at(rec, 80),
            snaps,
        });
        off += size as usize;
    }
    Ok(out)
}

/// Pair each entry with the next exit of the same hook on the same thread.
/// Probes used here sit on non-reentrant leaf applies; a nested entry is an error.
pub fn pair_entry_exit(events: &[TapEvent]) -> Result<Vec<(usize, usize)>, BridgeError> {
    let mut open: std::collections::HashMap<(u32, u16), usize> = Default::default();
    let mut pairs = Vec::new();
    for (i, e) in events.iter().enumerate() {
        let key = (e.tid, e.hook);
        match e.phase {
            0 => {
                if open.insert(key, i).is_some() {
                    return Err(BridgeError::Layout("nested entry on one thread"));
                }
            }
            _ => {
                if let Some(entry) = open.remove(&key) {
                    pairs.push((entry, i));
                }
            }
        }
    }
    Ok(pairs)
}

/// 32-byte pose records: vec4 position (w = 0) then xyzw rotation.
pub fn decode_joints(bytes: &[u8], count: usize) -> Result<Vec<Joint>, BridgeError> {
    if bytes.len() < count * 32 {
        return Err(BridgeError::Layout("pose array shorter than joint count"));
    }
    Ok((0..count)
        .map(|j| {
            let f = |k: usize| f32_at(bytes, j * 32 + 4 * k);
            Joint {
                position: Vec3::new(f(0), f(1), f(2)),
                rotation: Quat::from_xyzw(f(4), f(5), f(6), f(7)),
            }
        })
        .collect())
}

/// Skeleton hierarchy (skeleton `+0x68`): 12-byte entries, u16 parent (0xFFFF =
/// root) and u16 descendant count. The joint count is the root's descendant
/// count + 1; every native descendant count is checked against the parents.
pub fn decode_hierarchy(bytes: &[u8]) -> Result<Vec<Option<usize>>, BridgeError> {
    if bytes.len() < 12 {
        return Err(BridgeError::Layout("empty hierarchy"));
    }
    let count = u16_at(bytes, 2) as usize + 1;
    if bytes.len() < count * 12 || u16_at(bytes, 0) != 0xFFFF {
        return Err(BridgeError::Layout("hierarchy shorter than root range"));
    }
    let parents: Vec<Option<usize>> = (0..count)
        .map(|j| match u16_at(bytes, j * 12) {
            0xFFFF => None,
            p => Some(p as usize),
        })
        .collect();
    let pose = acu_motion::bone_modifier::ModelPose::new(
        parents.clone(),
        vec![
            Joint {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY
            };
            count
        ],
    )
    .map_err(|_| BridgeError::Layout("hierarchy is not depth-first"))?;
    for j in 0..count {
        if pose.descendants(j) != u16_at(bytes, j * 12 + 2) as usize {
            return Err(BridgeError::Layout("native descendant count disagrees"));
        }
    }
    Ok(parents)
}

/// MaxLookAt vtable in ACU.exe SHA-256 748abdc6...7e40 (ACU-RE-0028).
pub const MAX_LOOK_AT_VTABLE: u64 = 0x1_42e7_79b0;

/// Live MaxLookAt object (first 0x80 bytes): bone refs (+8 hash, +12 cached
/// index) at +0x10 owner, +0x60 up node, +0x70 look bone; weight +0x20; UpNode
/// and LookAtBone pointers +0x40/+0x48; axes and control +0x50..+0x5c.
pub fn decode_max_look_at(bytes: &[u8]) -> Result<MaxLookAt, BridgeError> {
    if bytes.len() < 0x80 {
        return Err(BridgeError::Layout("MaxLookAt record shorter than 0x80"));
    }
    if u64_at(bytes, 0) != MAX_LOOK_AT_VTABLE {
        return Err(BridgeError::Layout("not a MaxLookAt vtable"));
    }
    let index = |o: usize| u16_at(bytes, o + 12) as usize;
    let present = |o: usize| u64_at(bytes, o) != 0;
    let axes = [0x50, 0x54, 0x58, 0x5c].map(|o| u32_at(bytes, o) as i32);
    Ok(MaxLookAt::from_serialized(
        index(0x10),
        present(0x40).then(|| index(0x60)),
        present(0x48).then(|| index(0x70)),
        axes,
        f32_at(bytes, 0x20),
    ))
}

/// PositionConstraint vtable (ACU-RE-0028).
pub const POSITION_CONSTRAINT_VTABLE: u64 = 0x1_42e7_7710;

/// OrientationConstraint vtable (ACU-RE-0028).
pub const ORIENTATION_CONSTRAINT_VTABLE: u64 = 0x1_42e7_7780;

/// Shared constraint layout (0x50 bytes; Position and Orientation): owner ref
/// +0x10, weight +0x20, target pointer array +0x28, u16 count +0x32, flags +0x34
/// (bit 0 = offset), offset +0x40. `targets` are the 16-byte target records.
struct ConstraintFields {
    owner: usize,
    weight: f32,
    targets: Vec<ConstraintTarget>,
    offset: Option<[f32; 4]>,
}

fn decode_constraint(
    bytes: &[u8],
    vtable: u64,
    targets: &[&[u8]],
) -> Result<ConstraintFields, BridgeError> {
    if bytes.len() < 0x50 {
        return Err(BridgeError::Layout("constraint record shorter than 0x50"));
    }
    if u64_at(bytes, 0) != vtable {
        return Err(BridgeError::Layout("unexpected constraint vtable"));
    }
    let count = u16_at(bytes, 0x32) as usize;
    if targets.len() < count {
        return Err(BridgeError::Layout(
            "fewer target records than the constraint count",
        ));
    }
    let targets = targets[..count]
        .iter()
        .map(|t| {
            if t.len() < 16 {
                return Err(BridgeError::Layout("target record shorter than 16"));
            }
            Ok(ConstraintTarget {
                present: u64_at(t, 0) != 0,
                weight: f32_at(t, 8),
                joint: match u16_at(t, 12) {
                    0xFFFF => None,
                    j => Some(j as usize),
                },
            })
        })
        .collect::<Result<_, _>>()?;
    let offset = (bytes[0x34] & 1 != 0).then(|| [0x40, 0x44, 0x48, 0x4c].map(|o| f32_at(bytes, o)));
    Ok(ConstraintFields {
        owner: u16_at(bytes, 0x1c) as usize,
        weight: f32_at(bytes, 0x20),
        targets,
        offset,
    })
}

pub fn decode_position_constraint(
    bytes: &[u8],
    targets: &[&[u8]],
    scale: f32,
) -> Result<PositionConstraint, BridgeError> {
    let c = decode_constraint(bytes, POSITION_CONSTRAINT_VTABLE, targets)?;
    Ok(PositionConstraint {
        owner: c.owner,
        weight: c.weight,
        targets: c.targets,
        offset: c.offset.map(|o| Vec3::new(o[0], o[1], o[2])),
        scale,
    })
}

pub fn decode_orientation_constraint(
    bytes: &[u8],
    targets: &[&[u8]],
) -> Result<OrientationConstraint, BridgeError> {
    let c = decode_constraint(bytes, ORIENTATION_CONSTRAINT_VTABLE, targets)?;
    Ok(OrientationConstraint {
        owner: c.owner,
        weight: c.weight,
        targets: c.targets,
        offset: c.offset.map(Quat::from_array),
    })
}

/// Hinge vtable (class `324796c7`; ACU-RE-0028).
pub const HINGE_VTABLE: u64 = 0x1_42e7_7940;

/// Live Hinge object (0xc0 bytes): owner ref +0x10 (index +0x1c), weight +0x20,
/// pivot +0x30, frame +0x40, reference axis +0x50, axis +0x60, sphere array
/// pointer +0x70 with u16 count +0x7a, angle ramp +0xa0/+0xa4, distance ramp
/// +0xa8/+0xac. `spheres` holds the 32-byte entries the array points to.
pub fn decode_hinge(bytes: &[u8], spheres: &[u8], scale: f32) -> Result<Hinge, BridgeError> {
    if bytes.len() < 0xc0 {
        return Err(BridgeError::Layout("Hinge record shorter than 0xc0"));
    }
    if u64_at(bytes, 0) != HINGE_VTABLE {
        return Err(BridgeError::Layout("not a Hinge vtable"));
    }
    let count = u16_at(bytes, 0x7a) as usize;
    if spheres.len() < count * 32 {
        return Err(BridgeError::Layout(
            "fewer sphere entries than the Hinge count",
        ));
    }
    let vec4 = |o: usize| {
        Vec4::new(
            f32_at(bytes, o),
            f32_at(bytes, o + 4),
            f32_at(bytes, o + 8),
            f32_at(bytes, o + 12),
        )
    };
    Ok(Hinge {
        owner: u16_at(bytes, 0x1c) as usize,
        weight: f32_at(bytes, 0x20),
        pivot: vec4(0x30).truncate(),
        frame: vec4(0x40),
        reference_axis: vec4(0x50),
        axis: vec4(0x60),
        spheres: (0..count)
            .map(|k| HingeSphere {
                radius: f32_at(spheres, k * 32 + 8),
                joint: match u16_at(spheres, k * 32 + 0x1c) {
                    0xFFFF => None,
                    j => Some(j as usize),
                },
            })
            .collect(),
        angle_ramp: [f32_at(bytes, 0xa0), f32_at(bytes, 0xa4)],
        distance_ramp: [f32_at(bytes, 0xa8), f32_at(bytes, 0xac)],
        scale,
    })
}

/// Reflex vtable (class `057daa86`; ACU-RE-0028).
pub const REFLEX_VTABLE: u64 = 0x1_42e7_68c0;

/// Static bytes of one Reflex connector, read once next to a capture
/// (`tools/native_tap/reflex_static.py`): the 24-byte entry, its 96-byte
/// record, the joint-ref table (16-byte refs), tree and int16 sample table.
#[derive(Clone, Debug, Default)]
pub struct ReflexConnectorBytes {
    pub entry: Vec<u8>,
    pub record: Vec<u8>,
    pub refs: Vec<u8>,
    pub tree: Vec<u8>,
    pub samples: Vec<u8>,
}

fn decode_reflex_connector(c: &ReflexConnectorBytes) -> Result<ReflexConnector, BridgeError> {
    if c.entry.len() < 24
        || c.record.len() < 96
        || !c.samples.len().is_multiple_of(16)
        || !c.refs.len().is_multiple_of(16)
    {
        return Err(BridgeError::Layout(
            "Reflex connector bytes have a bad length",
        ));
    }
    if c.entry[2] != 0 {
        return Err(BridgeError::Layout(
            "only the three-sample Reflex connector is supported",
        ));
    }
    let r = &c.record;
    let ref_joint = |o: usize| -> Result<Option<usize>, BridgeError> {
        let k = u16_at(r, o) as usize;
        if k >= c.refs.len() / 16 {
            return Ok(None);
        }
        match u16_at(&c.refs, 16 * k + 12) {
            0xFFFF => Err(BridgeError::Layout(
                "Reflex joint ref without a cached index",
            )),
            j => Ok(Some(j as usize)),
        }
    };
    let vec4 = |o: usize| {
        Vec4::new(
            f32_at(r, o),
            f32_at(r, o + 4),
            f32_at(r, o + 8),
            f32_at(r, o + 12),
        )
    };
    let stride = u32_at(r, 0x4c) as usize;
    Ok(ReflexConnector {
        frame_joint: ref_joint(0x18)?,
        from_joint: ref_joint(0x1a)?,
        to_joint: ref_joint(0x1c)?,
        frame_offset: vec4(0),
        tree: c.tree.clone(),
        samples: c
            .samples
            .as_chunks::<16>()
            .0
            .iter()
            .map(|row| std::array::from_fn(|k| i16::from_le_bytes([row[2 * k], row[2 * k + 1]])))
            .collect(),
        sample_base: u16_at(&c.entry, 0) as usize * stride,
        position_scale: vec4(0x30),
    })
}

/// Live Reflex object (0x40 bytes): owner ref +0x10 (index +0x1c), weight
/// +0x20, second list count +0x32 (must be 0), connector count +0x3e. `base` is
/// the record at `*(+0x08)` (position +0x30, rotation +0x40), `model_header` the
/// model pose object's first 0x90 bytes (world transform, flags +0x8c).
pub fn decode_reflex(
    bytes: &[u8],
    base: &[u8],
    model_header: &[u8],
    scale: f32,
    connectors: &[ReflexConnectorBytes],
) -> Result<Reflex, BridgeError> {
    if bytes.len() < 0x40 || base.len() < 0x50 || model_header.len() < 0x90 {
        return Err(BridgeError::Layout("Reflex snapshot too short"));
    }
    if u64_at(bytes, 0) != REFLEX_VTABLE {
        return Err(BridgeError::Layout("not a Reflex vtable"));
    }
    if u16_at(bytes, 0x32) != 0 {
        return Err(BridgeError::Layout("Reflex second list is not supported"));
    }
    if u16_at(bytes, 0x3e) as usize != connectors.len() {
        return Err(BridgeError::Layout("Reflex connector count disagrees"));
    }
    let vec4 = |b: &[u8], o: usize| {
        Vec4::new(
            f32_at(b, o),
            f32_at(b, o + 4),
            f32_at(b, o + 8),
            f32_at(b, o + 12),
        )
    };
    Ok(Reflex {
        owner: u16_at(bytes, 0x1c) as usize,
        weight: f32_at(bytes, 0x20),
        base_position: vec4(base, 0x30),
        base_rotation: vec4(base, 0x40),
        connectors: connectors
            .iter()
            .map(decode_reflex_connector)
            .collect::<Result<_, _>>()?,
        scale,
        frame: PoseFrame {
            position: vec4(model_header, 0).truncate(),
            rotation: vec4(model_header, 0x10),
            model_space: u32_at(model_header, 0x8c) & 0x0400_0000 != 0,
        },
    })
}

/// `SNAPMODS` blob (`tap.cpp` `capture_mods`), taken at the frame evaluator
/// `0x1400377e0`: the selected LOD list's objects with their pointed-to inputs
/// and the local pose object's per-joint run channel bytes.
const MODS_MAGIC: u32 = 0x5344_4f4d;
const MODS_HEADER: usize = 32;
const MODS_CHANNELS: usize = 512;
const MODS_ENTRY: usize = 544;
const MODS_MAX: usize = 512;

#[derive(Clone, Debug, PartialEq)]
pub struct ModsEntry {
    pub object: u64,
    /// The object's first 0xc0 bytes.
    pub record: Vec<u8>,
    /// Constraint target records `*(*(obj + 0x28) + 8k)`, k = 0..4.
    pub targets: [Option<Vec<u8>>; 4],
    /// Hinge sphere entries `*(obj + 0x70)`, 192 bytes.
    pub spheres: Option<Vec<u8>>,
    /// Reflex base record `*(obj + 8)`, 80 bytes.
    pub base: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModsBlob {
    /// `(u16 [comp + 232] >> 3) & 7`.
    pub level: u8,
    /// The native list count (entries beyond 512 are not captured).
    pub list_count: usize,
    pub channels: Option<Vec<u8>>,
    pub entries: Vec<ModsEntry>,
}

pub fn decode_mods_blob(b: &[u8]) -> Result<ModsBlob, BridgeError> {
    if b.len() < MODS_HEADER + MODS_CHANNELS || u32_at(b, 0) != MODS_MAGIC {
        return Err(BridgeError::Layout("not a SNAPMODS blob"));
    }
    let captured = u32_at(b, 12) as usize;
    if captured > MODS_MAX || b.len() < MODS_HEADER + MODS_CHANNELS + captured * MODS_ENTRY {
        return Err(BridgeError::Layout(
            "SNAPMODS blob shorter than its entry count",
        ));
    }
    let level = u8::try_from(u32_at(b, 4))
        .ok()
        .filter(|&l| l < 8)
        .ok_or(BridgeError::Layout("SNAPMODS level above 7"))?;
    let entries = (0..captured)
        .map(|i| {
            let e = &b[MODS_HEADER + MODS_CHANNELS + i * MODS_ENTRY..][..MODS_ENTRY];
            let bits = u32_at(e, 8);
            let part = |bit: u32, range: std::ops::Range<usize>| {
                (bits & (1 << bit) != 0).then(|| e[range].to_vec())
            };
            ModsEntry {
                object: u64_at(e, 0),
                record: part(0, 16..208).unwrap_or_default(),
                targets: std::array::from_fn(|k| part(1 + k as u32, 208 + 16 * k..224 + 16 * k)),
                spheres: part(5, 272..464),
                base: part(6, 464..544),
            }
        })
        .collect();
    Ok(ModsBlob {
        level,
        list_count: u32_at(b, 8) as usize,
        channels: (u32_at(b, 16) != 0)
            .then(|| b[MODS_HEADER..MODS_HEADER + MODS_CHANNELS].to_vec()),
        entries,
    })
}

/// 32-byte cache record written by `0x14006cf00`: leaf id, three sample
/// indices (u32) and three weights.
pub fn decode_reflex_lookup(bytes: &[u8]) -> Result<ReflexLookup, BridgeError> {
    if bytes.len() < 28 {
        return Err(BridgeError::Layout("Reflex cache record shorter than 28"));
    }
    let index = |o: usize| {
        u16::try_from(u32_at(bytes, o))
            .map_err(|_| BridgeError::Layout("Reflex sample index above u16"))
    };
    Ok(ReflexLookup {
        leaf: index(0)?,
        samples: [index(4)?, index(8)?, index(12)?],
        weights: [f32_at(bytes, 16), f32_at(bytes, 20), f32_at(bytes, 24)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: u64, tid: u32, phase: u8, snaps: &[(u16, Option<&[u8]>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, s) in snaps {
            let len = s.map_or(4, |b| b.len());
            body.extend_from_slice(&id.to_le_bytes());
            body.extend_from_slice(&u16::from(s.is_some()).to_le_bytes());
            body.extend_from_slice(&(len as u32).to_le_bytes());
            let mut d = s.map_or(vec![0; len], |b| b.to_vec());
            d.resize((len + 7) & !7, 0);
            body.extend(d);
        }
        let mut r = Vec::new();
        r.extend_from_slice(&EVENT_MAGIC.to_le_bytes());
        r.extend_from_slice(&((EVENT_HEADER + body.len()) as u32).to_le_bytes());
        r.extend_from_slice(&seq.to_le_bytes());
        r.extend_from_slice(&100u64.to_le_bytes());
        r.extend_from_slice(&tid.to_le_bytes());
        r.extend_from_slice(&0u16.to_le_bytes());
        r.push(phase);
        r.push(snaps.len() as u8);
        for a in [1u64, 2, 3, 4, 5, 6, 7] {
            r.extend_from_slice(&a.to_le_bytes());
        }
        assert_eq!(r.len(), EVENT_HEADER);
        r.extend(body);
        r
    }

    #[test]
    fn mods_blob_decodes_entries_and_presence_bits() {
        let mut b = vec![0u8; MODS_HEADER + MODS_CHANNELS + 2 * MODS_ENTRY];
        b[..4].copy_from_slice(&MODS_MAGIC.to_le_bytes());
        b[4] = 5;
        b[8] = 3; // native count 3, two captured
        b[12] = 2;
        b[16] = 1;
        b[MODS_HEADER + 7] = 0x80;
        let e1 = MODS_HEADER + MODS_CHANNELS + MODS_ENTRY;
        b[e1..e1 + 8].copy_from_slice(&0xdead_beefu64.to_le_bytes());
        b[e1 + 8] = 0b0100_0101; // record, target 1, base
        b[e1 + 16] = 0xaa;
        b[e1 + 240] = 0xbb; // target k = 2 is absent
        b[e1 + 224] = 0xcc;
        let blob = decode_mods_blob(&b).unwrap();
        assert_eq!((blob.level, blob.list_count, blob.entries.len()), (5, 3, 2));
        assert_eq!(blob.channels.as_ref().unwrap()[7], 0x80);
        let e = &blob.entries[1];
        assert_eq!(e.object, 0xdead_beef);
        assert_eq!((e.record.len(), e.record[0]), (0xc0, 0xaa));
        assert_eq!(e.targets[1].as_ref().unwrap()[0], 0xcc);
        assert!(e.targets[0].is_none() && e.targets[2].is_none() && e.spheres.is_none());
        assert_eq!(e.base.as_ref().unwrap().len(), 80);
        assert!(blob.entries[0].record.is_empty());

        // Truncated entry table, bad magic and a level above 7 are rejected.
        assert!(decode_mods_blob(&b[..b.len() - 1]).is_err());
        let mut bad = b.clone();
        bad[0] = 0;
        assert!(decode_mods_blob(&bad).is_err());
        let mut bad = b;
        bad[4] = 8;
        assert!(decode_mods_blob(&bad).is_err());
    }

    #[test]
    fn parses_records_pairs_and_rejects_corruption() {
        let mut data = event(0, 7, 0, &[(1, Some(&[1, 2, 3])), (2, None)]);
        data.extend(event(1, 9, 0, &[]));
        data.extend(event(2, 7, 1, &[(4, Some(&[9; 9]))]));
        let ev = parse_events(&data).unwrap();
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[0].snap(1), Some(&[1u8, 2, 3][..]));
        assert_eq!(ev[0].snap(2), None);
        assert_eq!(ev[2].snap(4).unwrap().len(), 9);
        assert_eq!(ev[0].args, [1, 2, 3, 4]);
        assert_eq!((ev[0].rsp, ev[0].ret, ev[0].rax), (5, 6, 7));
        assert_eq!(pair_entry_exit(&ev).unwrap(), vec![(0, 2)]);

        assert!(matches!(
            parse_events(&data[..data.len() - 1]),
            Err(BridgeError::Truncated { .. })
        ));
        let mut bad = data.clone();
        bad[0] ^= 1;
        assert_eq!(
            parse_events(&bad).unwrap_err(),
            BridgeError::BadMagic { offset: 0 }
        );
        let mut bad = data.clone();
        bad[EVENT_HEADER + 4] = 0xFF; // snapshot length beyond the record
        assert!(matches!(
            parse_events(&bad),
            Err(BridgeError::BadSize { .. })
        ));
        let nested = [event(0, 7, 0, &[]), event(1, 7, 0, &[])].concat();
        assert!(pair_entry_exit(&parse_events(&nested).unwrap()).is_err());
    }

    #[test]
    fn hierarchy_checks_native_descendant_counts() {
        let entry = |parent: u16, desc: u16| {
            let mut e = [0u8; 12];
            e[..2].copy_from_slice(&parent.to_le_bytes());
            e[2..4].copy_from_slice(&desc.to_le_bytes());
            e
        };
        let good = [entry(0xFFFF, 2), entry(0, 1), entry(1, 0), entry(9, 9)].concat();
        assert_eq!(
            decode_hierarchy(&good).unwrap(),
            vec![None, Some(0), Some(1)]
        );
        let wrong = [entry(0xFFFF, 2), entry(0, 0), entry(1, 0)].concat();
        assert!(decode_hierarchy(&wrong).is_err());
        assert!(decode_hierarchy(&good[..24]).is_err());
    }

    #[test]
    fn max_look_at_record_layout() {
        let mut b = vec![0u8; 0x80];
        b[..8].copy_from_slice(&MAX_LOOK_AT_VTABLE.to_le_bytes());
        b[0x1c..0x1e].copy_from_slice(&52u16.to_le_bytes());
        b[0x20..0x24].copy_from_slice(&0.5f32.to_le_bytes());
        b[0x48] = 1;
        b[0x7c..0x7e].copy_from_slice(&55u16.to_le_bytes());
        b[0x6c..0x6e].copy_from_slice(&0xFFFFu16.to_le_bytes());
        for (o, v) in [(0x50, 0), (0x54, 2), (0x58, 1), (0x5c, 2)] {
            b[o..o + 4].copy_from_slice(&(v as u32).to_le_bytes());
        }
        let m = decode_max_look_at(&b).unwrap();
        assert!(decode_position_constraint(&b, &[], 1.0).is_err());
        assert_eq!(
            m,
            MaxLookAt::from_serialized(52, None, Some(55), [0, 2, 1, 2], 0.5)
        );
        b[0] ^= 1;
        assert!(decode_max_look_at(&b).is_err());
        assert!(decode_max_look_at(&b[..0x7f]).is_err());
    }

    #[test]
    fn position_constraint_record_layout() {
        let mut b = vec![0u8; 0x50];
        b[..8].copy_from_slice(&POSITION_CONSTRAINT_VTABLE.to_le_bytes());
        b[0x1c..0x1e].copy_from_slice(&339u16.to_le_bytes());
        b[0x20..0x24].copy_from_slice(&1.0f32.to_le_bytes());
        b[0x32..0x34].copy_from_slice(&2u16.to_le_bytes());
        b[0x34] = 3;
        b[0x40..0x44].copy_from_slice(&0.25f32.to_le_bytes());
        let target = |ptr: u64, w: f32, j: u16| {
            let mut t = [0u8; 16];
            t[..8].copy_from_slice(&ptr.to_le_bytes());
            t[8..12].copy_from_slice(&w.to_le_bytes());
            t[12..14].copy_from_slice(&j.to_le_bytes());
            t
        };
        let (a, z) = (target(1, 0.5, 328), target(0, 0.75, 0xFFFF));
        let c = decode_position_constraint(&b, &[&a, &z, &a], 2.0).unwrap();
        assert_eq!(c.owner, 339);
        assert_eq!(c.offset, Some(Vec3::new(0.25, 0.0, 0.0)));
        assert_eq!(c.targets.len(), 2);
        assert_eq!(
            c.targets[1],
            ConstraintTarget {
                present: false,
                weight: 0.75,
                joint: None
            }
        );
        assert_eq!(c.scale, 2.0);
        assert!(decode_position_constraint(&b, &[&a], 1.0).is_err());
        b[0x34] = 2;
        assert_eq!(
            decode_position_constraint(&b, &[&a, &z], 1.0)
                .unwrap()
                .offset,
            None
        );
        assert!(decode_orientation_constraint(&b, &[&a, &z]).is_err());
        b[..8].copy_from_slice(&ORIENTATION_CONSTRAINT_VTABLE.to_le_bytes());
        b[0x34] = 1;
        b[0x4c..0x50].copy_from_slice(&1.0f32.to_le_bytes());
        let o = decode_orientation_constraint(&b, &[&a, &z]).unwrap();
        assert_eq!(o.offset, Some(Quat::from_xyzw(0.25, 0.0, 0.0, 1.0)));
    }

    #[test]
    fn hinge_record_layout() {
        let mut b = vec![0u8; 0xc0];
        b[..8].copy_from_slice(&HINGE_VTABLE.to_le_bytes());
        b[0x1c..0x1e].copy_from_slice(&30u16.to_le_bytes());
        let put = |b: &mut Vec<u8>, o: usize, v: f32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        for (o, v) in [
            (0x20, 1.0),
            (0x38, 0.05),
            (0x4c, 1.0),
            (0x50, 1.0),
            (0x68, 1.0),
        ] {
            put(&mut b, o, v);
        }
        for (o, v) in [(0xa0, 0.17), (0xa4, 1.57), (0xa8, 0.2), (0xac, 0.4)] {
            put(&mut b, o, v);
        }
        b[0x7a..0x7c].copy_from_slice(&2u16.to_le_bytes());
        let mut spheres = vec![0u8; 64];
        spheres[8..12].copy_from_slice(&0.125f32.to_le_bytes());
        spheres[0x1c..0x1e].copy_from_slice(&7u16.to_le_bytes());
        spheres[0x28..0x2c].copy_from_slice(&0.25f32.to_le_bytes());
        spheres[0x3c..0x3e].copy_from_slice(&0xFFFFu16.to_le_bytes());
        let h = decode_hinge(&b, &spheres, 1.5).unwrap();
        assert_eq!((h.owner, h.weight, h.scale), (30, 1.0, 1.5));
        assert_eq!(h.pivot, Vec3::new(0.0, 0.0, 0.05));
        assert_eq!(h.frame, Vec4::W);
        assert_eq!((h.reference_axis, h.axis), (Vec4::X, Vec4::Z));
        assert_eq!((h.angle_ramp, h.distance_ramp), ([0.17, 1.57], [0.2, 0.4]));
        assert_eq!(
            h.spheres,
            vec![
                HingeSphere {
                    radius: 0.125,
                    joint: Some(7)
                },
                HingeSphere {
                    radius: 0.25,
                    joint: None
                }
            ]
        );
        assert!(decode_hinge(&b, &spheres[..63], 1.0).is_err());
        assert!(decode_reflex(&b[..0x40], &[0; 0x50], &[0; 0x90], 1.0, &[]).is_err());
        assert!(decode_hinge(&b[..0xbf], &spheres, 1.0).is_err());
        b[0] ^= 1;
        assert!(decode_hinge(&b, &spheres, 1.0).is_err());
    }

    #[test]
    fn reflex_record_layout() {
        let put = |b: &mut [u8], o: usize, v: &[u8]| b[o..o + v.len()].copy_from_slice(v);
        let mut obj = vec![0u8; 0x40];
        put(&mut obj, 0, &REFLEX_VTABLE.to_le_bytes());
        put(&mut obj, 0x1c, &71u16.to_le_bytes());
        put(&mut obj, 0x20, &0.5f32.to_le_bytes());
        put(&mut obj, 0x3e, &1u16.to_le_bytes());
        let mut base = vec![0u8; 0x50];
        put(&mut base, 0x34, &0.25f32.to_le_bytes());
        put(&mut base, 0x4c, &1.0f32.to_le_bytes());
        let mut header = vec![0u8; 0x90];
        put(&mut header, 0, &267.0f32.to_le_bytes());
        put(&mut header, 0x1c, &1.0f32.to_le_bytes());
        put(&mut header, 0x8c, &0x8000_0000u32.to_le_bytes());

        let mut entry = vec![0u8; 24];
        put(&mut entry, 0, &2u16.to_le_bytes()); // sample block 2 of stride 3
        entry[3] = 4;
        let mut record = vec![0u8; 96];
        put(&mut record, 0x0c, &1.0f32.to_le_bytes());
        for (o, k) in [(0x18, 1u16), (0x1a, 0), (0x1c, 9)] {
            put(&mut record, o, &k.to_le_bytes());
        }
        put(&mut record, 0x30, &0.001f32.to_le_bytes());
        put(&mut record, 0x4c, &3u32.to_le_bytes());
        let mut refs = vec![0u8; 32];
        put(&mut refs, 12, &87u16.to_le_bytes());
        put(&mut refs, 28, &88u16.to_le_bytes());
        let mut samples = vec![0u8; 16 * 7];
        put(&mut samples, 16 * 6 + 6, &32767i16.to_le_bytes());
        put(&mut samples, 16 * 6 + 8, &(-5i16).to_le_bytes());
        let conn = ReflexConnectorBytes {
            entry,
            record,
            refs,
            tree: vec![0; 64],
            samples,
        };
        let r = decode_reflex(&obj, &base, &header, 2.0, std::slice::from_ref(&conn)).unwrap();
        assert_eq!((r.owner, r.weight, r.scale), (71, 0.5, 2.0));
        assert_eq!(
            (r.base_position, r.base_rotation),
            (Vec4::new(0.0, 0.25, 0.0, 0.0), Vec4::W)
        );
        assert_eq!(r.frame.position, Vec3::new(267.0, 0.0, 0.0));
        assert!(!r.frame.model_space);
        let c = &r.connectors[0];
        // Ref 9 is past the two-entry table: absent, as the native lookup returns null.
        assert_eq!(
            (c.frame_joint, c.from_joint, c.to_joint),
            (Some(88), Some(87), None)
        );
        assert_eq!(
            (c.sample_base, c.samples.len(), c.frame_offset),
            (6, 7, Vec4::W)
        );
        let (q, p) = c.sample(0).unwrap();
        assert!((q.w - 1.0).abs() < 1e-4 && (p.x + 0.005).abs() < 1e-7);
        assert!(c.sample(1).is_err());

        let mut bad = conn.clone();
        bad.entry[2] = 1;
        assert!(decode_reflex(&obj, &base, &header, 1.0, &[bad]).is_err());
        let mut bad = conn.clone();
        put(&mut bad.refs, 12, &0xFFFFu16.to_le_bytes());
        assert!(decode_reflex(&obj, &base, &header, 1.0, &[bad]).is_err());
        assert!(decode_reflex(&obj, &base, &header, 1.0, &[]).is_err());
        let mut two_lists = obj.clone();
        put(&mut two_lists, 0x32, &1u16.to_le_bytes());
        assert!(decode_reflex(&two_lists, &base, &header, 1.0, &[conn]).is_err());

        let mut cache = vec![0u8; 32];
        for (o, v) in [(0, 9u32), (4, 3), (8, 0), (12, 5)] {
            put(&mut cache, o, &v.to_le_bytes());
        }
        put(&mut cache, 20, &0.75f32.to_le_bytes());
        let l = decode_reflex_lookup(&cache).unwrap();
        assert_eq!(
            (l.leaf, l.samples, l.weights),
            (9, [3, 0, 5], [0.0, 0.75, 0.0])
        );
        put(&mut cache, 4, &0x1_0000u32.to_le_bytes());
        assert!(decode_reflex_lookup(&cache).is_err());
        assert!(decode_reflex_lookup(&cache[..27]).is_err());
    }
}
