//! `Cloth` (`e33044ba`) soft-body resources (ACU-RE-0081).
//!
//! Two layers:
//! - The Anvil-serialized object prefix: `Cloth : SoftBody` holding one
//!   `MotionClothState`, which holds `MotionClothLOD` objects. Class and
//!   field names come from ACU's reflection tables. Only the first LOD's
//!   fields up to its simulation blob are decoded; the remaining LOD fields
//!   are not.
//! - The simulation blob (`SoftBodyLOD` field `64f250cd`): a self-describing
//!   tagged container read by the native cloth library. Each record is
//!   `u32 tag, u32 total_length, data`. The tag's high half must be `0xecd7`.
//!   This mirrors the native record search at `0x1423aee50`.
//!
//! The solver that consumes these records is not decoded here.

use crate::{Result, invalid};
use std::ops::Range;

pub const TYPE_CLOTH: u32 = 0xe330_44ba;
pub const CLASS_MOTION_CLOTH_STATE: u32 = 0x6119_d4ce;
pub const CLASS_MOTION_CLOTH_LOD: u32 = 0x2955_83ef;
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
}
