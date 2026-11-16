//! `Cloth` (`e33044ba`) soft-body resources (ACU-RE-0081).
//!
//! Two layers:
//! - The Anvil-serialized object prefix: `Cloth : SoftBody` holding one
//!   `MotionClothState`, which holds `MotionClothLOD` objects. Class and
//!   field names come from ACU's reflection tables. Only the first LOD's
//!   fields are decoded: up to its simulation blob by `parse`, and the rest
//!   (particle skinning, render-vertex mapping, bones) by `lod0_mesh`.
//! - The simulation blob (`SoftBodyLOD` field `64f250cd`): a self-describing
//!   tagged container read by the native cloth library. Each record is
//!   `u32 tag, u32 total_length, data`. The tag's high half must be `0xecd7`.
//!   This mirrors the native record search at `0x1423aee50`.
//!
//! The solver that consumes these records is ported in `acu_sim::cloth`.

use crate::{Result, invalid};
use std::ops::Range;

pub const TYPE_CLOTH: u32 = 0xe330_44ba;
pub const CLASS_MOTION_CLOTH_STATE: u32 = 0x6119_d4ce;
pub const CLASS_MOTION_CLOTH_LOD: u32 = 0x2955_83ef;
pub const CLASS_SOFT_BODY_VERTEX_MAPPING: u32 = 0x62ba_80e1;
pub const CLASS_DUPLICATE_VERTEX: u32 = 0x9a6e_3e7d;
pub const CLASS_MESH_BONE: u32 = 0x9ef0_e7a1;
const TAG_MAGIC: u32 = 0xecd7_0000;
const MAX_ITEMS: usize = 1 << 20;

/// One tagged record in the simulation blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Low 16 bits of the tag (`group << 8 | field`), as compared natively.
    pub tag: u16,
    /// Data range within the blob (after the 8-byte record header).
    pub data: Range<usize>,
}

impl Record {
    pub fn group(&self) -> u8 {
        (self.tag >> 8) as u8
    }
    pub fn field(&self) -> u8 {
        self.tag as u8
    }
}

/// The simulation blob: `u32 version, u32 body_length, records...`.
#[derive(Clone, Debug)]
pub struct Blob<'a> {
    pub bytes: &'a [u8],
    pub version: u32,
    pub records: Vec<Record>,
}

/// Walk the blob's records to its exact end.
pub fn blob(bytes: &[u8]) -> Result<Blob<'_>> {
    if bytes.len() < 8 {
        return invalid("cloth blob: shorter than its header");
    }
    let version = u32_at(bytes, 0)?;
    let body = u32_at(bytes, 4)? as usize;
    if version != 1 {
        return invalid(format!("cloth blob: unsupported version {version}"));
    }
    if body != bytes.len() - 8 {
        return invalid(format!(
            "cloth blob: body length {body} != {}",
            bytes.len() - 8
        ));
    }
    let mut records = Vec::new();
    let mut at = 8usize;
    while at < bytes.len() {
        if bytes.len() - at < 8 {
            return invalid(format!("cloth blob: truncated record header at {at}"));
        }
        let tag = u32_at(bytes, at)?;
        let len = u32_at(bytes, at + 4)? as usize;
        if tag & 0xffff_0000 != TAG_MAGIC {
            return invalid(format!("cloth blob: bad tag {tag:08x} at {at}"));
        }
        if len < 8 || len > bytes.len() - at {
            return invalid(format!("cloth blob: record length {len} at {at}"));
        }
        if records.len() == MAX_ITEMS {
            return invalid("cloth blob: too many records");
        }
        records.push(Record {
            tag: tag as u16,
            data: at + 8..at + len,
        });
        at += len;
    }
    Ok(Blob {
        bytes,
        version,
        records,
    })
}

impl<'a> Blob<'a> {
    /// First record with this tag. Native readers search forward a bounded
    /// number of records from a cursor; first-match is equivalent for the
    /// unique tags used here.
    pub fn find(&self, tag: u16) -> Option<&'a [u8]> {
        self.records
            .iter()
            .find(|r| r.tag == tag)
            .map(|r| &self.bytes[r.data.clone()])
    }

    /// Settings name (record `02.01`), NUL-terminated with an exact length,
    /// as checked by the native string reader `0x1423aef00`.
    pub fn settings_name(&self) -> Result<Option<&'a str>> {
        let Some(d) = self.find(0x0201) else {
            return Ok(None);
        };
        match d.split_last() {
            Some((0, s)) if !s.contains(&0) => Ok(Some(std::str::from_utf8(s)?)),
            _ => invalid("cloth blob: settings name is not exactly NUL-terminated"),
        }
    }

    /// Body class selector (record `11.01`): 0 or 1 natively (`0x1423cda50`).
    pub fn body_kind(&self) -> Result<u32> {
        let v = self.u32_record(0x1101)?;
        if v > 1 {
            return invalid(format!("cloth blob: unsupported body kind {v}"));
        }
        Ok(v)
    }

    /// Particle count (record `11.02`, `0x1423b0860`).
    pub fn particle_count(&self) -> Result<usize> {
        Ok(self.u32_record(0x1102)? as usize)
    }

    /// Particle positions (record `11.0b`): float4 per particle, stored for
    /// the count rounded up to 16 (`0x1423b0860`). Returns `count` entries.
    pub fn particles(&self) -> Result<Vec<[f32; 4]>> {
        let count = self.particle_count()?;
        if count > MAX_ITEMS {
            return invalid("cloth blob: particle count exceeds bounds");
        }
        let padded = (count + 15) & !15;
        let d = self
            .find(0x110b)
            .ok_or("cloth blob: positions record absent")?;
        if d.len() != padded * 16 {
            return invalid(format!(
                "cloth blob: positions are {} bytes, expected {}",
                d.len(),
                padded * 16
            ));
        }
        let (quads, _) = d.as_chunks::<16>();
        let out: Vec<[f32; 4]> = quads[..count]
            .iter()
            .map(|q| {
                let f = |i: usize| f32::from_le_bytes([q[i], q[i + 1], q[i + 2], q[i + 3]]);
                [f(0), f(4), f(8), f(12)]
            })
            .collect();
        if out.iter().flatten().any(|v| !v.is_finite()) {
            return invalid("cloth blob: nonfinite particle position");
        }
        Ok(out)
    }

    /// Distance constraint sets 0–2 (opcodes 0/1/2, kernel `0x1423e51f0`).
    /// Sources (loaders `0x1423bb3c0`, `0x14242f880`, `0x1423ce370`):
    /// `11.0c` six block counts, `11.1d` N, `11.0d` u8 rest arrays (entries
    /// 0–2), `11.0e` (scale, offset, threshold) per entry, `11.23` pair counts,
    /// `11.24` u16 pairs, `11.11` SIMD counts, `11.1a` clamp (min, max) per set
    /// (default −2/2, 0/2, −2/0), `11.08` stiffness per set.
    pub fn distance_sets(&self) -> Result<[DistanceSet; 3]> {
        let blocks = self.u32s(0x110c, 6)?;
        let n_total = self.u32_record(0x111d)? as u64;
        let packed = self.find(0x110d).ok_or("cloth blob: record 110d absent")?;
        let sum: u64 = blocks.iter().map(|&c| c as u64).sum();
        if packed.len() as u64 != 16 * (sum + 2 * n_total / 16) {
            return invalid("cloth blob: 110d size disagrees with 110c/111d");
        }
        let scales = self.f32s(0x110e, 18)?;
        let counts = self.u32s(0x1123, 3)?;
        let simd = self.u32s(0x1111, 3)?;
        let pairs_raw = self.find(0x1124).ok_or("cloth blob: record 1124 absent")?;
        let total_pairs: usize = counts.iter().map(|&c| c as usize).sum();
        if pairs_raw.len() != 4 * total_pairs {
            return invalid("cloth blob: 1124 size disagrees with 1123");
        }
        let clamp = match self.find(0x111a) {
            Some(_) => self.f32s(0x111a, 6)?,
            None => vec![-2.0, 2.0, 0.0, 2.0, -2.0, 0.0],
        };
        let stiffness = self.f32s(0x1108, 3)?;
        let particles = self.particle_count()?;
        let mut first_pair = 0usize;
        let mut first_block = 0usize;
        let mut out: Vec<DistanceSet> = Vec::with_capacity(3);
        for s in 0..3 {
            let n = counts[s] as usize;
            let bytes = &packed[16 * first_block..16 * (first_block + blocks[s] as usize)];
            first_block += blocks[s] as usize;
            if bytes.len() < n || simd[s] as usize > n || simd[s] % 8 != 0 {
                return invalid(format!("cloth blob: distance set {s} counts inconsistent"));
            }
            let pairs: Vec<(u16, u16)> = (first_pair..first_pair + n)
                .map(|i| {
                    let p = &pairs_raw[4 * i..4 * i + 4];
                    (
                        u16::from_le_bytes([p[0], p[1]]),
                        u16::from_le_bytes([p[2], p[3]]),
                    )
                })
                .collect();
            first_pair += n;
            if pairs
                .iter()
                .any(|&(a, b)| a as usize >= particles || b as usize >= particles)
            {
                return invalid(format!("cloth blob: distance set {s} index out of range"));
            }
            out.push(DistanceSet {
                pairs,
                rest_q: bytes[..n].to_vec(),
                scale: scales[3 * s],
                offset: scales[3 * s + 1],
                threshold: scales[3 * s + 2],
                simd_count: simd[s] as usize,
                k_min: clamp[2 * s],
                k_max: clamp[2 * s + 1],
                stiffness: stiffness[s],
            });
        }
        out.try_into()
            .map_err(|_| "cloth blob: distance set count".into())
    }

    /// Per-particle constraint arrays for sets 3 (backstop offset), 4 (max
    /// distance radius) and 5 (tether length): entries 3–5 of `11.0c`/`11.0d`/
    /// `11.0e`, one u8 per particle over the capacity. Tether anchors are the
    /// u16 region after them (`11.2a`, same bytes), one per particle.
    pub fn particle_constraints(&self) -> Result<ParticleConstraints> {
        let blocks = self.u32s(0x110c, 6)?;
        let cap = self.u32_record(0x111d)? as usize;
        let packed = self.find(0x110d).ok_or("cloth blob: record 110d absent")?;
        let scales = self.f32s(0x110e, 18)?;
        let mut at: usize = blocks[..3].iter().map(|&c| 16 * c as usize).sum();
        let mut maps = Vec::new();
        for k in 3..6 {
            let len = 16 * blocks[k] as usize;
            if len != cap || at + len > packed.len() {
                return invalid(format!(
                    "cloth blob: constraint array {k} is not one byte per particle"
                ));
            }
            maps.push(ByteMap {
                bytes: packed[at..at + len].to_vec(),
                scale: scales[3 * k],
                bias: scales[3 * k + 1],
                threshold: scales[3 * k + 2],
            });
            at += len;
        }
        if packed.len() != at + 2 * cap {
            return invalid("cloth blob: tether index region size");
        }
        let anchors: Vec<u16> = packed[at..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        if anchors.iter().any(|&a| a as usize >= cap) {
            return invalid("cloth blob: tether anchor out of range");
        }
        let mut it = maps.into_iter();
        let (backstop, max_distance, tether) = (
            it.next().ok_or("maps")?,
            it.next().ok_or("maps")?,
            it.next().ok_or("maps")?,
        );
        Ok(ParticleConstraints {
            backstop,
            max_distance,
            tether,
            anchors,
        })
    }

    /// Per-particle maps at body `+384` (loader `0x1423cf890`): `11.b2` (rows,
    /// slots), `11.b3` raw rows, `11.b4` per slot (u32 rows, scale, bias,
    /// threshold). Slot k is used when flag body `+124+k` (record `11.b1`) is set.
    pub fn particle_maps(&self) -> Result<[Option<ByteMap>; 6]> {
        let mut out: [Option<ByteMap>; 6] = Default::default();
        let Some(head) = self.find(0x11b2) else {
            return Ok(out);
        };
        if head.len() != 8 {
            return invalid("cloth blob: 11b2 size");
        }
        let (rows, slots) = (u32_at(head, 0)? as usize, u32_at(head, 4)? as usize);
        let raw = self.find(0x11b3).ok_or("cloth blob: 11b3 absent")?;
        let desc = self.find(0x11b4).ok_or("cloth blob: 11b4 absent")?;
        if raw.len() != 16 * rows || desc.len() != 16 * slots {
            return invalid("cloth blob: 11b3/11b4 sizes");
        }
        let mut at = 0usize;
        for (k, slot) in out.iter_mut().enumerate().take(slots.min(6)) {
            let n = 16 * u32_at(desc, 16 * k)? as usize;
            let f = |o: usize| u32_at(desc, 16 * k + o).map(f32::from_bits);
            if at + n > raw.len() {
                return invalid("cloth blob: 11b4 rows exceed 11b3");
            }
            if n > 0 {
                *slot = Some(ByteMap {
                    bytes: raw[at..at + n].to_vec(),
                    scale: f(4)?,
                    bias: f(8)?,
                    threshold: f(12)?,
                });
            }
            at += n;
        }
        Ok(out)
    }

    /// Body flags: record `11.04` mapped onto body `+96` (loader
    /// `0x1423cdc40`) and `11.b1` onto `+124..129` (`0x1423cdeb0`).
    /// Returned indexed by body offset − 96 (length 34).
    /// The opcode enable bytes of the settings block S (body +132) that the interpreter's gate
    /// `0x14240bf70` reads, as the blob's loader `0x1423ce370` writes them: S+0..S+6 from record
    /// `1107` (absent: `None`), S+7/S+10 from `1105` bytes 0/1, S+8/S+9 from bytes 48/49, and
    /// S+64 (drag) from the first byte of `112d` (loader `0x1423ce820`).
    pub fn op_gate(&self) -> Result<OpGate> {
        let s = self.find(0x1105).ok_or("cloth blob: 1105 absent")?;
        if s.len() < 50 {
            return invalid("cloth blob: 1105 short");
        }
        let ops = match self.find(0x1107) {
            Some(d) if d.len() >= 7 => {
                let mut v = [false; 7];
                for (i, b) in v.iter_mut().enumerate() {
                    *b = d[i] != 0;
                }
                Some(v)
            }
            Some(_) => return invalid("cloth blob: 1107 short"),
            None => None,
        };
        let drag = self.find(0x112d).ok_or("cloth blob: 112d absent")?;
        Ok(OpGate {
            s0_6: ops,
            s7: s[0] != 0,
            s8: s[48] != 0,
            s9: s[49] != 0,
            s10: s[1] != 0,
            s64: drag.first().is_some_and(|&b| b != 0),
        })
    }

    pub fn flags(&self) -> Result<[u8; 34]> {
        let mut f = [0u8; 34];
        let d = self.find(0x1104).ok_or("cloth blob: 1104 absent")?;
        if d.len() < 18 {
            return invalid("cloth blob: 1104 short");
        }
        let b = |i: usize| d.get(i).map(|&v| (v != 0) as u8);
        for (i, off) in [0usize, 1, 2, 3, 4, 5, 8, 20, 21].iter().enumerate() {
            f[*off] = b(i).unwrap_or(0);
        }
        let v5 = b(9).unwrap_or(0);
        f[6] = b(11).unwrap_or(0);
        for (i, off) in [(13usize, 24usize), (14, 11), (15, 9), (16, 10), (17, 25)] {
            f[off] = b(i).unwrap_or(0);
        }
        f[12] = b(18).unwrap_or(0);
        f[27] = b(19).unwrap_or(0);
        f[23] = d.get(24).copied().unwrap_or(0xff);
        if f[23] == 0xff {
            f[23] = v5;
        }
        f[22] = b(26).unwrap_or(0);
        f[7] = d.get(27).copied().unwrap_or(1);
        f[26] = b(28).unwrap_or(0);
        if let Some(m) = self.find(0x11b1) {
            for (i, v) in m.iter().take(6).enumerate() {
                f[28 + i] = (*v != 0) as u8;
            }
        }
        Ok(f)
    }

    /// Simulation settings from `11.05`, `11.2e` and `11.2d` (loaders
    /// `0x1423ce370`, `0x1423ce820`).
    pub fn settings(&self) -> Result<Settings> {
        let d = self.find(0x1105).ok_or("cloth blob: 1105 absent")?;
        if d.len() < 56 {
            return invalid("cloth blob: 1105 short");
        }
        let f = |o: usize| u32_at(d, o).map(f32::from_bits);
        // Offsets observed in 0x1423ce370: bools at 0,1; S+32..40 at 2..14;
        // S+44 at 14; 8 skipped; S+48 at 26; S+56 at 30; bool 34; S+60 at 35.
        let mut gravity = [f(2)?, f(6)?, f(10)?];
        let damping = f(14)?;
        let thickness = f(26)?;
        let blend = f(30)?;
        let s60 = f(35)?;
        // Optional tail: S+52 (max speed) defaults to 1000.
        let max_speed = if d.len() >= 65 { f(61)? } else { 1000.0 };
        match self.find(0x112e) {
            Some(_) => {
                let g = self.f32s(0x112e, 3)?;
                gravity = [g[0], g[1], g[2]];
            }
            None => gravity = gravity.map(|v| v * 0.5),
        }
        let drag = match (self.find(0x112d), self.find(0x1106)) {
            (Some(x), _) if x.len() >= 21 => {
                let g = |o: usize| u32_at(x, o).map(f32::from_bits);
                Drag {
                    wind_enabled: x[0] != 0,
                    wind: [g(1)?, g(5)?, g(9)?],
                    a: g(13)?,
                    b: g(17)?,
                }
            }
            (None, Some(_)) => return invalid("cloth blob: legacy drag record 1106 unsupported"),
            _ => return invalid("cloth blob: drag record 112d absent or short"),
        };
        let all = [
            gravity[0], gravity[1], gravity[2], damping, thickness, blend, max_speed, drag.a,
            drag.b,
        ];
        if all.iter().any(|v| !v.is_finite()) || drag.b == 0.0 {
            return invalid("cloth blob: nonfinite or zero settings");
        }
        // Serialized bytes 0 (S+7), 43 (S+192), 44 (S+196), 48 (S+8),
        // 49 (S+9), 50 (S+195).
        let collision = CollisionSettings {
            rigid: d[0] != 0,
            swept: d[43] != 0,
            friction: f(44)?,
            mesh: d[48] != 0,
            cloth: d[49] != 0,
            priority: d[50],
        };
        if !collision.friction.is_finite() {
            return invalid("cloth blob: nonfinite friction");
        }
        Ok(Settings {
            gravity,
            damping,
            max_speed,
            thickness,
            blend,
            s60,
            drag,
            collision,
        })
    }

    /// Triangles for normals (opcode 8): `11.12` count, `11.13` u16 triples.
    pub fn triangles(&self) -> Result<Vec<[u16; 3]>> {
        let n = self.u32_record(0x1112)? as usize;
        let d = self.find(0x1113).ok_or("cloth blob: 1113 absent")?;
        if d.len() != 6 * n {
            return invalid("cloth blob: 1113 size");
        }
        let p = self.particle_count()?;
        let t: Vec<[u16; 3]> = d
            .as_chunks::<6>()
            .0
            .iter()
            .map(|c| {
                [
                    u16::from_le_bytes([c[0], c[1]]),
                    u16::from_le_bytes([c[2], c[3]]),
                    u16::from_le_bytes([c[4], c[5]]),
                ]
            })
            .collect();
        if t.iter().flatten().any(|&i| i as usize >= p) {
            return invalid("cloth blob: triangle index out of range");
        }
        Ok(t)
    }

    /// Solver program (`11.09` count, `11.0a` (opcode, count) pairs).
    pub fn schedule(&self) -> Result<Vec<(u32, u32)>> {
        let n = self.u32_record(0x1109)? as usize;
        let v = self.u32s(0x110a, 2 * n)?;
        Ok(v.as_chunks::<2>().0.iter().map(|c| (c[0], c[1])).collect())
    }

    fn u32s(&self, tag: u16, n: usize) -> Result<Vec<u32>> {
        let d = self
            .find(tag)
            .ok_or_else(|| format!("cloth blob: record {tag:04x} absent"))?;
        if d.len() != 4 * n {
            return invalid(format!("cloth blob: record {tag:04x} is {} bytes", d.len()));
        }
        (0..n).map(|i| u32_at(d, 4 * i)).collect()
    }

    fn f32s(&self, tag: u16, n: usize) -> Result<Vec<f32>> {
        let v: Vec<f32> = self.u32s(tag, n)?.into_iter().map(f32::from_bits).collect();
        if v.iter().any(|x| !x.is_finite()) {
            return invalid(format!("cloth blob: record {tag:04x} has nonfinite values"));
        }
        Ok(v)
    }

    fn u32_record(&self, tag: u16) -> Result<u32> {
        let d = self
            .find(tag)
            .ok_or_else(|| format!("cloth blob: record {tag:04x} absent"))?;
        if d.len() != 4 {
            return invalid(format!("cloth blob: record {tag:04x} is {} bytes", d.len()));
        }
        u32_at(d, 0)
    }
}

/// One distance constraint set. Rest length of pair `j` is
/// `f32(rest_q[j]) * scale + offset`; `rest_q == 0xff` is disabled.
/// Pairs below `simd_count` run natively in independent groups of four.
#[derive(Clone, Debug, PartialEq)]
pub struct DistanceSet {
    pub pairs: Vec<(u16, u16)>,
    pub rest_q: Vec<u8>,
    pub scale: f32,
    pub offset: f32,
    pub threshold: f32,
    pub simd_count: usize,
    pub k_min: f32,
    pub k_max: f32,
    pub stiffness: f32,
}

/// Quantised per-element values: `f32(byte) * scale + bias`; enabled when
/// `value - threshold < 0` (byte 0xff disables on the observed data).
#[derive(Clone, Debug, PartialEq)]
pub struct ByteMap {
    pub bytes: Vec<u8>,
    pub scale: f32,
    pub bias: f32,
    pub threshold: f32,
}

/// Sets 3–5 inputs (one entry per particle, over the capacity).
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleConstraints {
    pub backstop: ByteMap,
    pub max_distance: ByteMap,
    pub tether: ByteMap,
    pub anchors: Vec<u16>,
}

/// Aerodynamic term (opcode 9): `c = (a * 0.5 * dt) / b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub wind_enabled: bool,
    pub wind: [f32; 3],
    pub a: f32,
    pub b: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub gravity: [f32; 3],
    pub damping: f32,
    pub max_speed: f32,
    /// S+48 (collision thickness, AABB margin).
    pub thickness: f32,
    /// S+56 (opcode 5 blend toward targets).
    pub blend: f32,
    /// S+60 (serialized byte 35). Live, the flap's particles sit at up to this multiple of
    /// their tether length (ACU-RE-0081: 1.1 in the game, inference: a tether stretch scale).
    pub s60: f32,
    pub drag: Drag,
    /// Opcode-7 inputs (loader `0x1423ce370`): S+7 rigid/analytic colliders,
    /// S+192 swept path, S+196 friction, S+8 mesh colliders, S+9 cloth
    /// colliders, S+195 cloth-vs-cloth priority.
    pub collision: CollisionSettings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionSettings {
    pub rigid: bool,
    pub swept: bool,
    pub friction: f32,
    pub mesh: bool,
    pub cloth: bool,
    pub priority: u8,
}

/// Opcode enables from the blob (see [`Blob::op_gate`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpGate {
    /// S+0..S+6: distance sets 0-2, backstop, max distance, tether (op 13), blend (op 5).
    pub s0_6: Option<[bool; 7]>,
    /// S+7 rigid colliders, S+8 mesh colliders, S+9 cloth colliders (op 7 runs if any is set).
    pub s7: bool,
    pub s8: bool,
    pub s9: bool,
    /// S+10: normals (op 8).
    pub s10: bool,
    /// S+64: drag (op 9).
    pub s64: bool,
}

/// Serialized pointer with a kind byte (fields with reflection flag `0x08`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// Tag 1: file ID with an undecoded kind byte.
    Id { kind: u8, file_id: String },
    /// Tag 3.
    Null,
}

/// The decoded prefix of a `Cloth` resource.
#[derive(Clone, Debug)]
pub struct Cloth<'a> {
    pub file_id: String,
    pub state_id: String,
    pub graphic_object: Pointer,
    /// `(file_id, class)` of each LOD; only the first is decoded further.
    pub lod_count: usize,
    pub lod0_id: String,
    pub lod0_settings: Pointer,
    /// Six per-particle u8 arrays (`SoftBodyLOD` fields `e3135e5c` …
    /// `69c9a576`); meanings undecoded.
    pub lod0_particle_bytes: [&'a [u8]; 6],
    pub lod0_blob: Blob<'a>,
    /// Payload offset just after the blob; later fields are undecoded.
    pub lod0_rest_offset: usize,
}

/// Decode the resource prefix up to the first LOD's simulation blob.
pub fn parse<'a>(payload: &'a [u8], expected_id: &str) -> Result<Cloth<'a>> {
    let mut c = Cursor { b: payload, at: 0 };
    if c.u8()? != 1 {
        return invalid("cloth: unsupported root prefix");
    }
    let file_id = c.id()?;
    if file_id != expected_id {
        return invalid(format!("cloth: file id {file_id} != {expected_id}"));
    }
    if c.u32()? != TYPE_CLOTH {
        return invalid("cloth: unexpected root class");
    }
    // SoftBody field 99ae01fa: array of owned SoftBodyState objects.
    if c.u32()? != 1 {
        return invalid("cloth: only one soft-body state is supported");
    }
    if (c.u8()?, c.u8()?) != (4, 0) {
        return invalid("cloth: unsupported owned-state pointer");
    }
    let state_id = c.id()?;
    if c.u32()? != CLASS_MOTION_CLOTH_STATE {
        return invalid("cloth: state is not a MotionClothState");
    }
    let graphic_object = c.pointer()?;
    let lod_count = c.u32()? as usize;
    if lod_count == 0 || lod_count > 16 {
        return invalid(format!("cloth: unsupported LOD count {lod_count}"));
    }
    if c.u8()? != 0 {
        return invalid("cloth: unsupported LOD wrapper");
    }
    let lod0_id = c.id()?;
    if c.u32()? != CLASS_MOTION_CLOTH_LOD {
        return invalid("cloth: LOD is not a MotionClothLOD");
    }
    let lod0_settings = c.pointer()?;
    let mut arrays: [&[u8]; 6] = [&[]; 6];
    for a in &mut arrays {
        let n = c.u32()? as usize;
        *a = c.take(n)?;
    }
    let n = c.u32()? as usize;
    let lod0_blob = blob(c.take(n)?)?;
    let count = lod0_blob.particle_count()?;
    if arrays.iter().any(|a| a.len() != count) {
        return invalid("cloth: per-particle arrays disagree with the blob count");
    }
    Ok(Cloth {
        file_id,
        state_id,
        graphic_object,
        lod_count,
        lod0_id,
        lod0_settings,
        lod0_particle_bytes: arrays,
        lod0_blob,
        lod0_rest_offset: c.at,
    })
}

/// How one render vertex follows the particles (`SoftBodyVertexMapping`,
/// render path `0x141d70840`). Cloth-driven vertices sit at
/// `Q[i0] + u (Q[i1] - Q[i0]) + h TN + v (Q[i2] - Q[i0])`, where `TN` is the
/// triangle's unit normal. `skinned` vertices are bone-driven instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexMapping {
    pub skinned: bool,
    pub triangle: u16,
    /// Serialized `Offset`; the render path does not read it.
    pub offset: f32,
    pub particles: [u16; 3],
    /// `(h, u, v)`.
    pub weights: [f32; 3],
}

/// A `MeshBone` element (the same 81-byte layout as in meshes).
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    pub name_hash: u32,
    /// Serialized order (the mesh's inverse bind matrix).
    pub matrix: [f32; 16],
    pub used: bool,
}

/// LOD0 `SoftBodyLOD`/`ClothLOD` fields after the simulation blob
/// (serializer `0x141d83740`, reflection field order).
#[derive(Clone, Debug, PartialEq)]
pub struct LodMesh {
    /// Field `77b91c50`, one u16 per particle triangle; meaning unknown.
    pub unknown_77b91c50: Vec<u16>,
    /// Rest particle positions and normals (`2c820548`, `c8debfc8`).
    pub rest_positions: Vec<[f32; 3]>,
    pub rest_normals: Vec<[f32; 3]>,
    /// Particle triangles (`cf8a22e7`).
    pub triangles: Vec<[u16; 3]>,
    /// One per render vertex (`744516ed`).
    pub mappings: Vec<VertexMapping>,
    /// `(original, duplicate)` render vertices (`f46419d3`).
    pub duplicates: Vec<(u16, u16)>,
    /// Render vertex to canonical vertex (`aa937f60`).
    pub canonical: Vec<u16>,
    /// Render index buffer (`fafa5f13`).
    pub indices: Vec<u16>,
    pub uvs: Vec<[f32; 2]>,
    pub mesh_id: String,
    pub lod_index: u16,
    pub particle_influences: usize,
    pub vertex_influences: usize,
    /// The two bytes after the influence counts; meanings unknown.
    pub unknown_bytes: [u8; 2],
    /// `particle_influences` per particle; 0xffff = none (`9b03474f`, `81e62f6c`).
    pub particle_bones: Vec<u16>,
    pub particle_weights: Vec<f32>,
    /// Rest positions of the skinned render vertices, in mapping order
    /// (`72e61900`), with `vertex_influences` bones and weights each.
    pub skinned_rest: Vec<[f32; 3]>,
    pub skinned_bones: Vec<u16>,
    pub skinned_weights: Vec<f32>,
    pub bones: Vec<Bone>,
    /// Payload offset after the bones; later fields and LODs are undecoded.
    pub end_offset: usize,
}

/// Decode LOD0's fields after its blob (from `cloth.lod0_rest_offset`).
pub fn lod0_mesh(payload: &[u8], cloth: &Cloth) -> Result<LodMesh> {
    let mut c = Cursor {
        b: payload,
        at: cloth.lod0_rest_offset,
    };
    let particles = cloth.lod0_blob.particle_count()?;
    let unknown_77b91c50 = c.array(2, |c| c.u16())?;
    let rest_positions = c.array(12, |c| c.vec3())?;
    let rest_normals = c.array(12, |c| c.vec3())?;
    if rest_positions.len() != particles || rest_normals.len() != particles {
        return invalid("cloth lod: rest arrays disagree with the particle count");
    }
    let flat = c.array(2, |c| c.u16())?;
    if flat.len() % 3 != 0 || flat.iter().any(|&i| i as usize >= particles) {
        return invalid("cloth lod: bad particle triangles");
    }
    let triangles: Vec<[u16; 3]> = flat.as_chunks::<3>().0.to_vec();
    let mappings = c.array(37, |c| {
        c.object(CLASS_SOFT_BODY_VERTEX_MAPPING)?;
        let skinned = match c.u8()? {
            0 => false,
            1 => true,
            _ => return invalid("cloth lod: vertex mapping flag is not 0/1"),
        };
        Ok(VertexMapping {
            skinned,
            triangle: c.u16()?,
            offset: c.f32()?,
            particles: [c.u16()?, c.u16()?, c.u16()?],
            weights: [c.f32()?, c.f32()?, c.f32()?],
        })
    })?;
    let vertices = mappings.len();
    for m in &mappings {
        let t = triangles
            .get(m.triangle as usize)
            .ok_or("cloth lod: mapping triangle out of range")?;
        if *t != m.particles || m.weights.iter().any(|w| !w.is_finite()) {
            return invalid("cloth lod: mapping disagrees with its triangle");
        }
    }
    let duplicates = c.array(16, |c| {
        c.object(CLASS_DUPLICATE_VERTEX)?;
        Ok((c.u16()?, c.u16()?))
    })?;
    let canonical = c.array(2, |c| c.u16())?;
    let indices = c.array(2, |c| c.u16())?;
    let uvs = c.array(8, |c| Ok([c.f32()?, c.f32()?]))?;
    let outside = |v: u16| v as usize >= vertices;
    if canonical.len() != vertices
        || uvs.len() != vertices
        || indices.len() % 3 != 0
        || indices.iter().any(|&i| outside(i))
        || canonical.iter().any(|&i| outside(i))
        || duplicates.iter().any(|&(a, b)| outside(a) || outside(b))
    {
        return invalid("cloth lod: render arrays disagree with the vertex count");
    }
    for _ in 0..3 {
        // `379eb726` and the two LOD-transfer mapping arrays.
        if c.u32()? != 0 {
            return invalid("cloth lod: LOD-transfer arrays are not supported");
        }
    }
    if c.u8()? != 0 {
        return invalid("cloth lod: unsupported mesh pointer");
    }
    let mesh_id = c.id()?;
    let lod_index = c.u16()?;
    let particle_influences = c.u16()? as usize;
    let vertex_influences = c.u16()? as usize;
    let unknown_bytes = [c.u8()?, c.u8()?];
    if !(1..=8).contains(&particle_influences) || !(1..=8).contains(&vertex_influences) {
        return invalid("cloth lod: unsupported influence count");
    }
    let particle_bones = c.array(2, |c| c.u16())?;
    let particle_weights = c.array(4, |c| c.f32())?;
    let skinned_rest = c.array(12, |c| c.vec3())?;
    let skinned_bones = c.array(2, |c| c.u16())?;
    let skinned_weights = c.array(4, |c| c.f32())?;
    let bones = c.array(81, |c| {
        c.object(CLASS_MESH_BONE)?;
        let name_hash = c.u32()?;
        let mut matrix = [0.0; 16];
        for m in &mut matrix {
            *m = c.f32()?;
        }
        let used = c.u8()? != 0;
        Ok(Bone {
            name_hash,
            matrix,
            used,
        })
    })?;
    let flagged = mappings.iter().filter(|m| m.skinned).count();
    let bad_bone = |v: &[u16]| v.iter().any(|&b| b != 0xffff && b as usize >= bones.len());
    if particle_bones.len() != particles * particle_influences
        || particle_weights.len() != particle_bones.len()
        || skinned_rest.len() != flagged
        || skinned_bones.len() != flagged * vertex_influences
        || skinned_weights.len() != skinned_bones.len()
        || bad_bone(&particle_bones)
        || bad_bone(&skinned_bones)
    {
        return invalid("cloth lod: skinning arrays disagree with their counts");
    }
    let nonfinite = rest_positions
        .iter()
        .chain(&rest_normals)
        .chain(&skinned_rest)
        .flatten()
        .chain(uvs.iter().flatten())
        .chain(&particle_weights)
        .chain(&skinned_weights)
        .chain(bones.iter().flat_map(|b| b.matrix.iter()))
        .any(|v| !v.is_finite());
    if nonfinite {
        return invalid("cloth lod: nonfinite values");
    }
    Ok(LodMesh {
        unknown_77b91c50,
        rest_positions,
        rest_normals,
        triangles,
        mappings,
        duplicates,
        canonical,
        indices,
        uvs,
        mesh_id,
        lod_index,
        particle_influences,
        vertex_influences,
        unknown_bytes,
        particle_bones,
        particle_weights,
        skinned_rest,
        skinned_bones,
        skinned_weights,
        bones,
        end_offset: c.at,
    })
}

pub const TYPE_LITE_RAGDOLL: u32 = 0x8910_43d5;
pub const CLASS_LITE_RAGDOLL_SHAPE: u32 = 0x2222_5754;
pub const CLASS_CAPSULE_GROUP_FLAGS: u32 = 0x13e0_73f6;

/// One `LiteRagdollShape` (descriptor `0x144e894f0`, deserializer
/// `0x141d82960`). At runtime each becomes a small rigid body whose world
/// frame is `local × bone` (row vectors) and which joins the cloth body's
/// explicit collider list.
#[derive(Clone, Debug, PartialEq)]
pub struct LiteRagdollShape {
    /// `ShapeType`: 0 capsule, 1 sphere, 2 box, 3 mesh.
    pub shape_type: u32,
    /// `BoneID`: bone name hash.
    pub bone: u32,
    /// `HalfExtent` (boxes).
    pub half_extent: [f32; 4],
    /// `Height`: the capsule's half-length.
    pub height: f32,
    pub radius: f32,
    /// `LocalMatrix` rows X, Y, Z, T (serialized order).
    pub local: [[f32; 4]; 4],
    /// `GroupFlags`, one byte per flag; not read by the collision path.
    pub group_flags: [u8; 16],
}

/// `LiteRagdoll` (`891043d5`): `Capsules`, then `ExternalCapsules`
/// (only an empty list is supported).
pub fn lite_ragdoll(payload: &[u8], expected_id: &str) -> Result<Vec<LiteRagdollShape>> {
    let mut c = Cursor { b: payload, at: 0 };
    if c.u8()? != 1 || c.id()? != expected_id || c.u32()? != TYPE_LITE_RAGDOLL {
        return invalid("lite ragdoll: unexpected root");
    }
    let shapes = c.array(136, |c| {
        c.id()?;
        if c.u32()? != CLASS_LITE_RAGDOLL_SHAPE {
            return invalid("lite ragdoll: element is not a LiteRagdollShape");
        }
        let shape_type = c.u32()?;
        let bone = c.u32()?;
        let half_extent = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
        let height = c.f32()?;
        let radius = c.f32()?;
        let mut local = [[0.0; 4]; 4];
        for row in &mut local {
            for v in row.iter_mut() {
                *v = c.f32()?;
            }
        }
        c.object(CLASS_CAPSULE_GROUP_FLAGS)?;
        let mut group_flags = [0u8; 16];
        group_flags.copy_from_slice(c.take(16)?);
        let all = half_extent.iter().chain(local.iter().flatten());
        if all.chain([&height, &radius]).any(|v| !v.is_finite()) {
            return invalid("lite ragdoll: nonfinite shape values");
        }
        Ok(LiteRagdollShape {
            shape_type,
            bone,
            half_extent,
            height,
            radius,
            local,
            group_flags,
        })
    })?;
    if c.u32()? != 0 {
        return invalid("lite ragdoll: external capsules are not supported");
    }
    if c.at != payload.len() {
        return invalid("lite ragdoll: trailing bytes");
    }
    Ok(shapes)
}

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.b.len() - self.at {
            return invalid(format!("cloth: {n} bytes past end at {}", self.at));
        }
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }
    fn f32(&mut self) -> Result<f32> {
        self.u32().map(f32::from_bits)
    }
    fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    /// `u32 count`, then `count` elements of at least `min_size` bytes each.
    fn array<T>(
        &mut self,
        min_size: usize,
        mut f: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let n = self.u32()? as usize;
        if n > MAX_ITEMS || n * min_size > self.b.len() - self.at {
            return invalid(format!("cloth: array of {n} past end at {}", self.at));
        }
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(f(self)?);
        }
        Ok(v)
    }
    /// Inline object header: u64 0, then the class hash.
    fn object(&mut self, class: u32) -> Result<()> {
        if self.id()? != "0000000000000000" || self.u32()? != class {
            return invalid(format!("cloth: expected inline object {class:08x}"));
        }
        Ok(())
    }
    fn u32(&mut self) -> Result<u32> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn id(&mut self) -> Result<String> {
        let s = self.take(8)?;
        let mut v = [0u8; 8];
        v.copy_from_slice(s);
        Ok(format!("{:016x}", u64::from_le_bytes(v)))
    }
    fn pointer(&mut self) -> Result<Pointer> {
        match self.u8()? {
            1 => Ok(Pointer::Id {
                kind: self.u8()?,
                file_id: self.id()?,
            }),
            3 => Ok(Pointer::Null),
            t => invalid(format!("cloth: unsupported pointer tag {t}")),
        }
    }
}

/// `SoftBodySettings` resource type (reflection class `f7a7e4df`, 144-byte runtime object).
pub const TYPE_SOFT_BODY_SETTINGS: u32 = 0xf7a7_e4df;
/// Fixed serialized size of a `SoftBodySettings` payload (all 109 installed variants).
pub const SOFT_BODY_SETTINGS_LEN: usize = 141;

/// One serialized `SoftBodySettings` field (reflection type codes `0a` float, `00`/`03`/`c0`
/// byte, `07` int32, `0c` vec3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingValue {
    F32(f32),
    U8(u8),
    I32(i32),
    Vec3([f32; 3]),
}

/// Reflection field order (`SoftBodySettings`, ACU-RE-0081) with each field's type code.
/// Only `8bb303f6` has a name (`Damping`); the rest are CRC-32 name hashes.
const SOFT_BODY_SETTINGS_FIELDS: [(u32, u8); 45] = [
    (0x8241_9bf7, 0xc0),
    (0x9098_bae0, 0xc0),
    (0x275b_8856, 0xc0),
    (0x531c_fd0e, 0x0a),
    (0xd6f6_d11a, 0x03),
    (0x50c1_fc95, 0x07),
    (0xfac3_f701, 0x0a),
    (0x7ada_8f5d, 0x0a),
    (0xffa9_64ad, 0x0a),
    (0x536b_b0ea, 0x0a),
    (0x8807_8322, 0x0a),
    (0x447b_4a4a, 0x0a),
    (0xe34f_ca33, 0x0a),
    (0x495e_ea23, 0x0a),
    (0x9892_2727, 0x0a),
    (0x3fa6_a75e, 0x0a),
    (0xd47d_fb66, 0x0a),
    (0x4ae3_78a9, 0x0a),
    (0xedd7_f8d0, 0x0a),
    (0x0649_2b52, 0x00),
    (0xee6c_1935, 0x0a),
    (0x95cc_3788, 0x00),
    (0x8347_736e, 0x0a),
    (0x7dd9_c586, 0x00),
    (0x1664_baeb, 0x0a),
    (0x8ca9_117e, 0x00),
    (0x4465_9285, 0x00),
    (0x87b0_8a1b, 0x00),
    (0x1902_337e, 0x00),
    (0x0e02_89e3, 0x00),
    (0xa276_7efb, 0x00),
    (0x00d1_5395, 0x03),
    (0x37e5_8757, 0x00),
    (0xeb7a_7a5d, 0x0a),
    (0xcf50_65b0, 0x0a),
    (0xb1b4_8708, 0x00),
    (0x90f1_fc99, 0x0a),
    (0x8bb3_03f6, 0x0a),
    (0xbb87_efe7, 0x03),
    (0xe087_12ff, 0x0c),
    (0xd2bb_63f9, 0x0a),
    (0x6a99_69e1, 0x0a),
    (0xfd87_0e7b, 0x00),
    (0x5421_c3b0, 0x00),
    (0xeb29_cec2, 0x00),
];

/// Serialized field order: fields 3 and 4, then the three `c0` fields 0–2, then 5–44. The
/// position of fields 0–2 is an inference (they hold 0, 0, 1 in every installed variant).
const SOFT_BODY_SETTINGS_ORDER: [usize; 45] = {
    let mut o = [0usize; 45];
    o[0] = 3;
    o[1] = 4;
    o[2] = 0;
    o[3] = 1;
    o[4] = 2;
    let mut i = 5;
    while i < 45 {
        o[i] = i;
        i += 1;
    }
    o
};

/// The external `SoftBodySettings` a `Cloth` LOD names (`ClothSettings_Standard_Avatars` for the
/// flap). Fields are kept by reflection index; the named accessors are the fields whose live
/// body value was matched (ACU-RE-0081, live comparison 2026-11-16).
#[derive(Clone, Debug, PartialEq)]
pub struct SoftBodySettings {
    pub file_id: String,
    /// Reflection order (index = field number above).
    pub fields: Vec<(u32, SettingValue)>,
}

impl SoftBodySettings {
    fn f(&self, i: usize) -> f32 {
        match self.fields[i].1 {
            SettingValue::F32(v) => v,
            _ => f32::NAN,
        }
    }
    /// Field 39 (`e08712ff`, vec3): live body S+32 (+164), used as is (no halving).
    pub fn gravity(&self) -> [f32; 3] {
        match self.fields[39].1 {
            SettingValue::Vec3(v) => v,
            _ => [f32::NAN; 3],
        }
    }
    /// Field 37 (`Damping`): live body S+44 (+176).
    pub fn damping(&self) -> f32 {
        self.f(37)
    }
    /// Field 34 (`cf5065b0`): collision thickness, live body S+48 (+180).
    pub fn thickness(&self) -> f32 {
        self.f(34)
    }
    /// Field 20 (`ee6c1935`): live body S+60 (+192); the tether stretch scale (inference).
    pub fn tether_scale(&self) -> f32 {
        self.f(20)
    }
    /// Field 24 (`1664baeb`): collision friction, live body S+196 (+328). Inference: fields 33
    /// and 36 hold the same 0.2 in the flap's settings; 24 is chosen because its bool-then-float
    /// pair (fields 23/24) mirrors the blob record's swept/friction bytes 43/44.
    pub fn friction(&self) -> f32 {
        self.f(24)
    }
    /// Fields `10 + 3·set` .. `12 + 3·set`: distance set `set` (0–2) stiffness, `k_min`, `k_max`.
    /// The flap's triples (1, −2, 2), (1, 0, 2), (1.5, −2, 0) carry the blob's clamps, and the
    /// stiffnesses equal the live body's +152/+156/+160, which the distance dispatcher
    /// `0x1423e5180` reads (`*(solver+496) + 152 + 4·set`) (ACU-RE-0081).
    pub fn distance(&self, set: usize) -> (f32, f32, f32) {
        let i = 10 + 3 * set.min(2);
        (self.f(i), self.f(i + 1), self.f(i + 2))
    }
    /// Fields 40 and 41: the drag constants the kernel `0x1423d72b0` reads (body +252, +256).
    pub fn drag(&self) -> (f32, f32) {
        (self.f(40), self.f(41))
    }
}

/// Decode a `SoftBodySettings` payload: root byte 1, file id, type, then the 45 fields.
pub fn soft_body_settings(payload: &[u8], expected_id: &str) -> Result<SoftBodySettings> {
    if payload.len() != SOFT_BODY_SETTINGS_LEN {
        return invalid(format!(
            "soft body settings: {} bytes, expected {SOFT_BODY_SETTINGS_LEN}",
            payload.len()
        ));
    }
    let mut c = Cursor { b: payload, at: 0 };
    if c.u8()? != 1 {
        return invalid("soft body settings: unexpected root tag");
    }
    let file_id = c.id()?;
    if file_id != expected_id || c.u32()? != TYPE_SOFT_BODY_SETTINGS {
        return invalid("soft body settings: unexpected id or type");
    }
    let mut fields = vec![(0u32, SettingValue::U8(0)); 45];
    for &i in &SOFT_BODY_SETTINGS_ORDER {
        let (hash, code) = SOFT_BODY_SETTINGS_FIELDS[i];
        let v = match code {
            0x0a => SettingValue::F32(c.f32()?),
            0x07 => SettingValue::I32(c.u32()? as i32),
            0x0c => SettingValue::Vec3([c.f32()?, c.f32()?, c.f32()?]),
            0x00 => {
                let b = c.u8()?;
                if b > 1 {
                    return invalid(format!("soft body settings: bool field {i} is {b}"));
                }
                SettingValue::U8(b)
            }
            _ => SettingValue::U8(c.u8()?),
        };
        if let SettingValue::F32(x) = v
            && !x.is_finite()
        {
            return invalid(format!("soft body settings: field {i} is not finite"));
        }
        fields[i] = (hash, v);
    }
    debug_assert_eq!(c.at, SOFT_BODY_SETTINGS_LEN);
    Ok(SoftBodySettings { file_id, fields })
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    let s = b.get(at..at + 4).ok_or("cloth: u32 read past end")?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(tag: u16, data: &[u8]) -> Vec<u8> {
        let mut v = (TAG_MAGIC | tag as u32).to_le_bytes().to_vec();
        v.extend((data.len() as u32 + 8).to_le_bytes());
        v.extend(data);
        v
    }
    fn wrap(records: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = records.concat();
        let mut v = 1u32.to_le_bytes().to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        v
    }
    fn particles_record(count: usize) -> Vec<u8> {
        let padded = (count + 15) & !15;
        let mut d = Vec::new();
        for i in 0..padded {
            for k in 0..4 {
                d.extend((i as f32 + k as f32 * 0.25).to_le_bytes());
            }
        }
        rec(0x110b, &d)
    }

    #[test]
    fn blob_records_and_particles() {
        let b = wrap(&[
            rec(0x0201, b"Std\0"),
            rec(0x1101, &1u32.to_le_bytes()),
            rec(0x1102, &3u32.to_le_bytes()),
            particles_record(3),
        ]);
        let blob = blob(&b).unwrap();
        assert_eq!(blob.records.len(), 4);
        assert_eq!(blob.records[3].group(), 0x11);
        assert_eq!(blob.records[3].field(), 0x0b);
        assert_eq!(blob.settings_name().unwrap(), Some("Std"));
        assert_eq!(blob.body_kind().unwrap(), 1);
        let p = blob.particles().unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[2], [2.0, 2.25, 2.5, 2.75]);
    }

    #[test]
    fn blob_rejects_bad_framing() {
        let good = wrap(&[rec(0x1102, &1u32.to_le_bytes())]);
        let mut bad_magic = good.clone();
        bad_magic[10] ^= 1; // tag high half
        assert!(blob(&bad_magic).is_err());
        let mut bad_len = good.clone();
        bad_len[12] = 200; // record length past end
        assert!(blob(&bad_len).is_err());
        let mut short = good.clone();
        short.push(0); // trailing byte breaks body length
        assert!(blob(&short).is_err());
        let mut version = good.clone();
        version[0] = 2;
        assert!(blob(&version).is_err());
    }

    #[test]
    fn particle_record_must_match_padded_count() {
        let mut p = particles_record(16);
        p.truncate(p.len() - 16);
        let len = (p.len() as u32).to_le_bytes();
        p[4..8].copy_from_slice(&len);
        let b = wrap(&[rec(0x1102, &16u32.to_le_bytes()), p]);
        assert!(blob(&b).unwrap().particles().is_err());
        let b = wrap(&[rec(0x0201, b"x")]);
        assert!(blob(&b).unwrap().settings_name().is_err());
    }

    /// A synthetic settings payload in serialized order (fields 3, 4, 0-2, 5-44).
    fn settings_payload() -> Vec<u8> {
        let mut v = vec![1u8];
        v.extend(0x0000_0016_c61d_53a1u64.to_le_bytes());
        v.extend(TYPE_SOFT_BODY_SETTINGS.to_le_bytes());
        for &i in &SOFT_BODY_SETTINGS_ORDER {
            match SOFT_BODY_SETTINGS_FIELDS[i].1 {
                0x0a => v.extend((i as f32).to_le_bytes()),
                0x07 => v.extend((-1i32).to_le_bytes()),
                0x0c => [0.0f32, 0.0, -20.0]
                    .iter()
                    .for_each(|x| v.extend(x.to_le_bytes())),
                0x00 => v.push(1),
                _ => v.push(i as u8),
            }
        }
        v
    }

    #[test]
    fn soft_body_settings_fields_land_by_reflection_index() {
        let p = settings_payload();
        assert_eq!(p.len(), SOFT_BODY_SETTINGS_LEN);
        let s = soft_body_settings(&p, "00000016c61d53a1").unwrap();
        assert_eq!(s.gravity(), [0.0, 0.0, -20.0]);
        assert_eq!(
            (s.damping(), s.thickness(), s.tether_scale()),
            (37.0, 34.0, 20.0)
        );
        assert_eq!((s.friction(), s.drag()), (24.0, (40.0, 41.0)));
        assert_eq!(
            (s.distance(0), s.distance(2)),
            ((10.0, 11.0, 12.0), (16.0, 17.0, 18.0))
        );
        assert_eq!(s.fields[2], (0x275b_8856, SettingValue::U8(2)));
        assert_eq!(s.fields[5].1, SettingValue::I32(-1));
        assert!(soft_body_settings(&p, "00000016c61d53a2").is_err());
        assert!(soft_body_settings(&p[..140], "00000016c61d53a1").is_err());
        let mut bad = p.clone();
        bad[77] = 2; // field 19, a bool, in serialized position 77
        assert!(soft_body_settings(&bad, "00000016c61d53a1").is_err());
    }
}
