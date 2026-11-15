//! Cloth step captures (`tools/native_tap/cloth_capture.py`, ACU-RE-0081).
//!
//! One entry/exit pair of the step `0x1423d4b60(solver, body, dt, wind, shapes)` for one body.
//! Entry snapshots: 1 body (736 bytes), 2 positions, 3 previous, 4 particle normals (body
//! +0x30), 5 targets, 6 target normals, 7 dt, 8 wind, 9 shapes `{records, count}`, 10 records
//! (160 bytes each). Exit snapshots: 12 positions, 13 previous, 14 normals.

use crate::{BridgeError, TapEvent, f32_at, u32_at};
use acu_sim::cloth::collision::{Frame, Shape, ShapeKind};
use glam::{Vec3, Vec4};

pub const RECORD: usize = 160;

fn snap(e: &TapEvent, id: u16) -> Result<&[u8], BridgeError> {
    e.snaps
        .iter()
        .find(|(s, _)| *s == id)
        .and_then(|(_, b)| b.as_deref())
        .ok_or(BridgeError::Layout("cloth snapshot missing or unreadable"))
}

pub fn vec4s(b: &[u8], n: usize) -> Result<Vec<Vec4>, BridgeError> {
    if b.len() < n * 16 {
        return Err(BridgeError::Truncated {
            offset: 0,
            need: n * 16,
        });
    }
    Ok((0..n)
        .map(|i| {
            let o = 16 * i;
            Vec4::new(
                f32_at(b, o),
                f32_at(b, o + 4),
                f32_at(b, o + 8),
                f32_at(b, o + 12),
            )
        })
        .collect())
}

fn vec3(b: &[u8], o: usize) -> Vec3 {
    Vec3::new(f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8))
}

/// A frame stored as four 16-byte rows (x, y, z, t; the w lanes are not read).
pub fn frame(b: &[u8], o: usize) -> Frame {
    Frame {
        x: vec3(b, o),
        y: vec3(b, o + 16),
        z: vec3(b, o + 32),
        t: vec3(b, o + 48),
    }
}

/// One 160-byte shape record: type `+8` (1 box, 2 sphere, 3 capsule), current frame `+16`,
/// previous frame `+80`, sizes from `+144` (box half-extents; sphere radius; capsule radius,
/// half-length). Current/previous: observed live, the next step's `+80` rotation equals this
/// step's `+16` (ACU-RE-0081).
pub fn decode_shape(r: &[u8]) -> Result<Shape, BridgeError> {
    if r.len() < RECORD {
        return Err(BridgeError::Truncated {
            offset: 0,
            need: RECORD,
        });
    }
    let kind = match u32_at(r, 8) {
        1 => ShapeKind::Box(vec3(r, 144)),
        2 => ShapeKind::Sphere(f32_at(r, 144)),
        3 => ShapeKind::Capsule {
            radius: f32_at(r, 144),
            half_length: f32_at(r, 148),
        },
        _ => return Err(BridgeError::Layout("cloth shape type not ported")),
    };
    Ok(Shape {
        kind,
        current: frame(r, 16),
        previous: frame(r, 80),
    })
}

/// The solver inputs of one captured step and the game's results.
#[derive(Clone, Debug)]
pub struct Step {
    pub qpc: u64,
    pub body: Vec<u8>,
    pub dt: f32,
    pub wind: Vec3,
    pub pos: Vec<Vec4>,
    pub prev: Vec<Vec4>,
    pub normals: Vec<Vec4>,
    pub target: Vec<Vec4>,
    pub target_normals: Vec<Vec4>,
    pub shapes: Vec<Shape>,
    pub out_pos: Vec<Vec4>,
    pub out_prev: Vec<Vec4>,
    pub out_normals: Vec<Vec4>,
}

pub fn decode_step(e: &TapEvent, x: &TapEvent, n: usize) -> Result<Step, BridgeError> {
    let cap = (n + 15) & !15;
    let body = snap(e, 1)?.to_vec();
    if body.len() < 736 || u32_at(&body, 12) as usize != n {
        return Err(BridgeError::Layout("cloth body particle count differs"));
    }
    let list = snap(e, 9)?;
    let count = u32_at(list, 8) as usize;
    let records = snap(e, 10)?;
    if records.len() < count * RECORD {
        return Err(BridgeError::Layout("more shape records than captured"));
    }
    let shapes = (0..count)
        .map(|i| decode_shape(&records[i * RECORD..(i + 1) * RECORD]))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Step {
        qpc: e.qpc,
        dt: f32_at(snap(e, 7)?, 0),
        wind: vec3(snap(e, 8)?, 0),
        pos: vec4s(snap(e, 2)?, n)?,
        prev: vec4s(snap(e, 3)?, n)?,
        normals: vec4s(snap(e, 4)?, cap)?,
        target: vec4s(snap(e, 5)?, n)?,
        target_normals: vec4s(snap(e, 6)?, n)?,
        shapes,
        out_pos: vec4s(snap(x, 12)?, n)?,
        out_prev: vec4s(snap(x, 13)?, n)?,
        out_normals: vec4s(snap(x, 14)?, cap)?,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_record_decodes_current_then_previous() {
        let mut r = vec![0u8; RECORD];
        r[8..12].copy_from_slice(&3u32.to_le_bytes());
        let put = |r: &mut Vec<u8>, o: usize, v: f32| r[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut r, 16, 1.0); // current x
        put(&mut r, 16 + 48 + 8, 0.7); // current t.z
        put(&mut r, 80, -1.0); // previous x
        put(&mut r, 144, 0.12);
        put(&mut r, 148, 0.16);
        let s = decode_shape(&r).unwrap();
        assert_eq!(
            s.kind,
            ShapeKind::Capsule {
                radius: 0.12,
                half_length: 0.16
            }
        );
        assert_eq!(
            (s.current.x.x, s.current.t.z, s.previous.x.x),
            (1.0, 0.7, -1.0)
        );
        r[8] = 9;
        assert!(decode_shape(&r).is_err());
        assert!(decode_shape(&r[..100]).is_err());
    }
}
