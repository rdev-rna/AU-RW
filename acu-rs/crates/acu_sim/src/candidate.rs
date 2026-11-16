//! Parkour candidate distances (ACU-RE-0055).
//!
//! Fancy slot 1 (`0x1401B1770`) copies a candidate record and fills `+0xC0..+0xC8` from the
//! target `+0x30` minus a reference point passed as an argument. This is that arithmetic only;
//! the generator that chooses targets, and the per-type windows, are not recovered.

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
    fn target_below_gives_negative_height() {
        let s = candidate_scalars([0.0, 0.0, 2.0], [0.0, 0.0, 0.5]);
        assert_eq!(s.height, -1.5);
        assert_eq!(s.horizontal, 0.0);
        assert_eq!(s.distance, 1.5);
    }
}
