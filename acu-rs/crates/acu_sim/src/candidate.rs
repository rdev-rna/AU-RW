//! Parkour candidate distances (ACU-RE-0055).
//!
//! Fancy slot 1 (`0x1401B1770`) copies a candidate record and fills `+0xC0..+0xC8` from the
//! target `+0x30` minus a reference point passed as an argument. This is that arithmetic only;
//! the generator that chooses targets, and the per-type windows, are not recovered.

use crate::block::LedgeEdge;
use glam::{Vec2, Vec3};

/// The three distances a candidate stores, native Z-up metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateScalars {
    /// `+0xC0`: target height above the reference (signed).
    pub height: f32,
    /// `+0xC4`: horizontal distance.
    pub horizontal: f32,
    /// `+0xC8`: 3D distance.
    pub distance: f32,
}

/// Distances from `reference` to `target`, both native Z-up.
pub fn candidate_scalars(reference: [f32; 3], target: [f32; 3]) -> CandidateScalars {
    let d = [
        target[0] - reference[0],
        target[1] - reference[1],
        target[2] - reference[2],
    ];
    let horizontal = (d[0] * d[0] + d[1] * d[1]).sqrt();
    CandidateScalars {
        height: d[2],
        horizontal,
        distance: (horizontal * horizontal + d[2] * d[2]).sqrt(),
    }
}

/// A ledge edge seen from a reference point, at the edge point horizontally closest to it.
/// Arena frame (Y up, [`crate::block`]). These are analysis features (ACU-RE-0055 per-type
/// windows), not recovered native fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeView {
    /// Edge point height above the reference.
    pub height: f32,
    /// Horizontal distance from the reference to the edge point.
    pub horizontal: f32,
    /// Cosine between the edge's outward normal and the horizontal direction back to the
    /// reference: 1 when the ledge faces the reference.
    pub facing_cos: f32,
}

pub fn edge_view(edge: &LedgeEdge, reference: Vec3) -> EdgeView {
    let ab = Vec2::new(edge.b.x - edge.a.x, edge.b.z - edge.a.z);
    let ra = Vec2::new(reference.x - edge.a.x, reference.z - edge.a.z);
    let t = (ra.dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    let p = edge.a + (edge.b - edge.a) * t;
    let back = Vec2::new(reference.x - p.x, reference.z - p.z);
    let horizontal = back.length();
    EdgeView {
        height: p.y - reference.y,
        horizontal,
        facing_cos: if horizontal > 1e-4 {
            edge.outward.dot(back) / horizontal
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_of_a_3_4_12_offset() {
        let s = candidate_scalars([1.0, 2.0, 3.0], [4.0, 6.0, 15.0]);
        assert_eq!(s.height, 12.0);
        assert_eq!(s.horizontal, 5.0);
        assert_eq!(s.distance, 13.0);
    }

    #[test]
    fn edge_view_of_a_ledge_facing_the_reference() {
        // Edge along x at height 2, top face on -z (outward +z); reference 3 m out on +z.
        let e = LedgeEdge {
            a: Vec3::new(-1.0, 2.0, 0.0),
            b: Vec3::new(1.0, 2.0, 0.0),
            outward: Vec2::new(0.0, 1.0),
            top_ny: 1.0,
            side_ny: 0.0,
            instance: 0,
        };
        let v = edge_view(&e, Vec3::new(0.5, 0.0, 3.0));
        assert_eq!(v.height, 2.0);
        assert_eq!(v.horizontal, 3.0);
        assert_eq!(v.facing_cos, 1.0);
        // Beyond the end: the closest point is the end, seen from behind the ledge.
        let v = edge_view(&e, Vec3::new(4.0, 2.0, -4.0));
        assert_eq!(v.horizontal, 5.0);
        assert!((v.facing_cos + 0.8).abs() < 1e-6);
    }

    #[test]
    fn target_below_gives_negative_height() {
        let s = candidate_scalars([0.0, 0.0, 2.0], [0.0, 0.0, 0.5]);
        assert_eq!(s.height, -1.5);
        assert_eq!(s.horizontal, 0.0);
        assert_eq!(s.distance, 1.5);
    }
}
