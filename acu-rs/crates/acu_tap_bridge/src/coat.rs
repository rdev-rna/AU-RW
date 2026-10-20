//! Capture decoder for the bounded coat-chain port (ACU-RE-0028).
use crate::{BridgeError, f32_at, u16_at, u32_at, u64_at};
use acu_motion::coat::{CoatChain, CoatNode};
use glam::{Quat, Vec3};

pub fn vec3(b: &[u8], o: usize) -> Vec3 {
    Vec3::new(f32_at(b, o), f32_at(b, o + 4), f32_at(b, o + 8))
}
pub fn quat(b: &[u8], o: usize) -> Quat {
    Quat::from_xyzw(
        f32_at(b, o),
        f32_at(b, o + 4),
        f32_at(b, o + 8),
        f32_at(b, o + 12),
    )
}

pub fn decode_chain(s: &[u8], records: &[&[u8]], gravity: Vec3) -> Result<CoatChain, BridgeError> {
    if s.len() < 96 {
        return Err(BridgeError::Layout("coat solver truncated"));
    }
    let n = usize::from(u16_at(s, 18));
    if n == 0 || n > 64 || records.len() != n {
        return Err(BridgeError::Layout("coat node count"));
    }
    if u64_at(s, 24) != 0 {
        return Err(BridgeError::Layout("coat collisions unsupported"));
    }
    if s[33] != 0 {
        return Err(BridgeError::Layout("coat reset unsupported"));
    }
    let nodes = records
        .iter()
        .map(|b| {
            if b.len() < 324 {
                return Err(BridgeError::Layout("coat node truncated"));
            }
            if u64_at(b, 104) != 0 {
                return Err(BridgeError::Layout("coat node collisions unsupported"));
            }
            if f32_at(b, 144) != 0.0 {
                return Err(BridgeError::Layout("coat wind unsupported"));
            }
            if u32_at(b, 180) & 0x8000_0000 != 0 {
                return Err(BridgeError::Layout("coat external frame unsupported"));
            }
            if u32_at(b, 64) > 1 || u32_at(b, 68) > 3 {
                return Err(BridgeError::Layout("coat mode unsupported"));
            }
            if !f32_at(b, 112).is_finite() || f32_at(b, 112) <= 0.0 {
                return Err(BridgeError::Layout("coat mass"));
            }
            Ok(CoatNode {
                joint: usize::from(u16_at(b, 28)),
                mass: f32_at(b, 112),
                damping: f32_at(b, 140),
                stiffness: f32_at(b, 136),
                length: f32_at(b, 320),
                gravity: vec3(b, 48),
                force: vec3(b, 144),
                force_mode: u32_at(b, 68),
                limit_mode: u32_at(b, 64),
                limit_degrees: [f32_at(b, 120), f32_at(b, 124)],
                ellipse: Vec3::new(1.0, f32_at(b, 128), f32_at(b, 132)),
                anchor: vec3(b, 192),
                preceding: vec3(b, 208),
                current: vec3(b, 224),
                previous: vec3(b, 240),
                model_rotation: quat(b, 256),
                rest_rotation: quat(b, 272),
                output_rotation: quat(b, 288),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CoatChain {
        nodes,
        done: s[32] != 0,
        reset: false,
        previous_dt: f32_at(s, 48),
        gravity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncation_counts_and_unsupported_state() {
        assert!(decode_chain(&[], &[], Vec3::ZERO).is_err());
        let mut solver = [0u8; 112];
        solver[18] = 65;
        assert!(decode_chain(&solver, &[], Vec3::ZERO).is_err());
        solver[18] = 1;
        assert!(decode_chain(&solver, &[&[0; 32]], Vec3::ZERO).is_err());
        let mut node = [0u8; 336];
        node[112..116].copy_from_slice(&1f32.to_le_bytes());
        assert!(decode_chain(&solver, &[&node], Vec3::ZERO).is_ok());
        node[104] = 1;
        assert!(decode_chain(&solver, &[&node], Vec3::ZERO).is_err());
        node[104] = 0;
        node[144..148].copy_from_slice(&1f32.to_le_bytes());
        assert!(decode_chain(&solver, &[&node], Vec3::ZERO).is_err());
        node[144..148].fill(0);
        solver[33] = 1;
        assert!(decode_chain(&solver, &[&node], Vec3::ZERO).is_err());
    }
}
