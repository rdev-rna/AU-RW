//! Recovered landing-candidate score, ACU-RE-0030 / VA 0x140177c40.
//! This is not the complete scanner or a root-motion warper.

use crate::bone_modifier::rsqrt_estimate;

/// Bounds and preferred values supplied to the native scorer, in native lanes.
/// X/Y measure horizontal separation; signed Z separation measures height here.
/// No game-wide units, scan radius or prediction horizon are assumed.
#[derive(Clone, Copy, Debug)]
pub struct LandingScoreParams {
    pub preferred_distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub preferred_height: f32,
    pub min_height: f32,
    pub max_height: f32,
}

/// Score one candidate. `None` means invalid API input or outside native bounds.
/// Native returns zero on rejection; `Some(0.0)` preserves a valid zero score.
/// The supplied direction is deliberately not normalized or clamped to unit
/// length: neither operation occurs in this native routine.
///
/// Supported domain: finite vectors/parameters, nonnegative ordered distance
/// bounds and strictly ordered height bounds, with finite squared distance.
/// Native behavior outside this domain is not reproduced. Exact captured-score
/// parity remains ungated; default math uses a portable reciprocal-sqrt estimate.
pub fn landing_score(
    origin: [f32; 3],
    direction: [f32; 3],
    candidate: [f32; 3],
    params: LandingScoreParams,
) -> Option<f32> {
    let p = params;
    if !origin
        .into_iter()
        .chain(direction)
        .chain(candidate)
        .all(f32::is_finite)
        || ![
            p.preferred_distance,
            p.min_distance,
            p.max_distance,
            p.preferred_height,
            p.min_height,
            p.max_height,
        ]
        .into_iter()
        .all(f32::is_finite)
        || p.min_distance < 0.0
        || p.min_distance >= p.max_distance
        || p.min_height >= p.max_height
    {
        return None;
    }
    let dx = candidate[0] - origin[0];
    let dy = candidate[1] - origin[1];
    let dz = candidate[2] - origin[2];
    let squared_distance = dx * dx + dy * dy;
    let distance_span = p.max_distance - p.min_distance;
    let height_span = p.max_height - p.min_height;
    if !squared_distance.is_finite() || !dz.is_finite() || !height_span.is_finite() {
        return None;
    }
    let distance = squared_distance.sqrt();
    if distance < p.min_distance
        || distance > p.max_distance
        || dz < p.min_height
        || dz > p.max_height
    {
        return None;
    }
    // Native mask selects the original horizontal delta at/below this threshold.
    let scale = if squared_distance <= 1e-6 {
        1.0
    } else {
        let r = rsqrt_estimate(squared_distance);
        (3.0 - (r * squared_distance) * r) * (0.5 * r)
    };
    let alignment = ((dx * scale) * direction[0] + (dy * scale) * direction[1]).max(0.0);
    let distance_term =
        (1.0 - (p.preferred_distance - distance).abs() / distance_span).clamp(0.0, 1.0);
    let height_term = (1.0 - (p.preferred_height - dz).abs() / height_span).clamp(0.0, 1.0);
    let score = (distance_term * 0.3 + alignment * 0.5) + height_term * 0.2;
    score.is_finite().then_some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> LandingScoreParams {
        LandingScoreParams {
            preferred_distance: 5.0,
            min_distance: 0.0,
            max_distance: 10.0,
            preferred_height: 0.0,
            min_height: -2.0,
            max_height: 2.0,
        }
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 2e-6, "{actual} != {expected}");
    }

    #[test]
    fn independently_known_geometry_and_weights() {
        let origin = [7.0, -4.0, 10.0];
        // A 3-4-5 triangle, dot with +X = 3/5: .3 + .5*.6 + .2 = .8.
        close(
            landing_score(origin, [1.0, 0.0, 0.0], [10.0, 0.0, 10.0], params()).unwrap(),
            0.8,
        );
        // Same point above by 2: height term drops from 1 to 1/2.
        close(
            landing_score(origin, [1.0, 0.0, 0.0], [10.0, 0.0, 12.0], params()).unwrap(),
            0.7,
        );
        // A backwards candidate loses all of the alignment contribution.
        close(
            landing_score(origin, [-1.0, 0.0, 0.0], [10.0, 0.0, 10.0], params()).unwrap(),
            0.5,
        );
        // Supplied direction is not internally normalized; score can exceed 1.
        close(
            landing_score([0.0; 3], [2.0, 0.0, 0.0], [5.0, 0.0, 0.0], params()).unwrap(),
            1.5,
        );
    }

    #[test]
    fn bounds_are_inclusive_and_invalid_inputs_fail() {
        let run = |candidate, p| landing_score([0.0; 3], [1.0, 0.0, 0.0], candidate, p);
        assert!(run([10.0, 0.0, 2.0], params()).is_some());
        assert!(run([10.001, 0.0, 0.0], params()).is_none());
        assert!(run([5.0, 0.0, -2.001], params()).is_none());
        assert!(run([f32::NAN, 0.0, 0.0], params()).is_none());
        assert!(run([f32::MAX, f32::MAX, 0.0], params()).is_none());
        assert!(
            run(
                [5.0, 0.0, 0.0],
                LandingScoreParams {
                    min_height: 2.0,
                    ..params()
                }
            )
            .is_none()
        );
        assert!(
            run(
                [5.0, 0.0, 0.0],
                LandingScoreParams {
                    max_distance: 0.0,
                    ..params()
                }
            )
            .is_none()
        );
    }

    #[test]
    fn near_zero_native_path_and_valid_zero_score() {
        // Zero separation has no alignment, but distance/height still contribute.
        close(
            landing_score([0.0; 3], [1.0, 0.0, 0.0], [0.0; 3], params()).unwrap(),
            0.35,
        );
        // Tiny deltas stay unnormalized; a unit-normalized alternative adds .5.
        close(
            landing_score([0.0; 3], [1.0, 0.0, 0.0], [0.0005, 0.0, 0.0], params()).unwrap(),
            0.350265,
        );
        let p = LandingScoreParams {
            preferred_distance: 20.0,
            preferred_height: 10.0,
            ..params()
        };
        assert_eq!(
            landing_score([0.0; 3], [1.0, 0.0, 0.0], [-5.0, 0.0, 0.0], p),
            Some(0.0)
        );
    }
}
