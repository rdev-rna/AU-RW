//! Recovered AMO primary pose cost, RVA 0x1631374 (image 7ca57147...6916).
//!
//! This is ONE term of the native candidate objective, not a complete matcher.
//! Inputs are already in their respective root-local, inverse-yaw frames. Query
//! construction, prediction cost 0x1631ba0, eligibility and dispatch are not ported.
//! Tracked/paired points deliberately have no guessed bone names.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalPose {
    pub root_velocity: [f32; 3],
    pub tracked_positions: [[f32; 3]; 2],
    pub tracked_velocities: [[f32; 3]; 2],
    pub paired_positions: [[f32; 3]; 2],
    /// wrap(sample+0x58 - sample+0x54); feature column not yet established.
    pub heading_delta: Option<f32>,
    /// sample+0x5c (feature column 21). Native accessor wraps interpolation.
    pub angular_scalar: Option<f32>,
    /// sample+0x68 (accessor RVA 0x1f372cc); feature column not established.
    pub scalar68: Option<f32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PoseWeights {
    /// Runtime settings offsets +20,+28,+2c,+30,+34,+38,+40, in that order.
    pub values: [f32; 7],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PoseCost {
    /// Native diagnostic IDs 2,4,5,6,7,9,0xb, in evaluation order.
    pub terms: [f32; 7],
    pub total: f32,
    /// True means remaining terms were not evaluated, not that they cost zero.
    pub pruned: bool,
}

/// Native angle helper RVA 0x484fb0, finite ordinary-angle domain.
/// The native implementation truncates the absolute number of full turns, then
/// folds into [-pi, pi); preserve +pi -> -pi rather than using a signed remainder.
pub fn wrap_angle(x: f32) -> f32 {
    let turns = (x.abs() / std::f32::consts::TAU).trunc();
    let mut y = x - turns * x.signum() * std::f32::consts::TAU;
    if y < -std::f32::consts::PI {
        y += std::f32::consts::TAU;
    }
    if y >= std::f32::consts::PI {
        y -= std::f32::consts::TAU;
    }
    y
}

fn difference(a: [f32; 3], b: [f32; 3], scale: f32) -> Result<f32, &'static str> {
    if !a.iter().chain(b.iter()).all(|x| x.is_finite()) {
        return Err("nonfinite pose");
    }
    let x = (a[0] - b[0]) * scale;
    let y = (a[1] - b[1]) * scale;
    let z = (a[2] - b[2]) * scale;
    Ok((x * x + z * z) + y * y)
}

fn scalar(a: Option<f32>, b: Option<f32>) -> Result<(f32, f32), &'static str> {
    match (a, b) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Ok((a, b)),
        _ => Err("enabled cost requires unresolved or nonfinite scalar"),
    }
}

/// Evaluate the complete recovered primary term, or a pruning bound.
/// Nonnegative weights are required for sound early pruning. Unsupported signed
/// settings are rejected explicitly; missing enabled fields never become zero.
pub fn primary_pose_cost(
    q: &LocalPose,
    c: &LocalPose,
    w: PoseWeights,
    prune_at: Option<f32>,
) -> Result<PoseCost, &'static str> {
    if !w.values.iter().all(|x| x.is_finite() && *x >= 0.0) {
        return Err("invalid pose weights");
    }
    if prune_at.is_some_and(|x| !x.is_finite()) {
        return Err("nonfinite pruning bound");
    }
    let mut result = PoseCost {
        terms: [0.0; 7],
        total: 0.0,
        pruned: false,
    };
    for (i, weight) in w.values.into_iter().enumerate() {
        let term = if weight == 0.0 {
            0.0
        } else {
            match i {
                0 => difference(q.root_velocity, c.root_velocity, 1.0)? * weight,
                1 | 2 => {
                    // Native scales position error by 2, derivative error by 1/3,
                    // then each of the two point contributions by 0.5*weight.
                    let (a, b, scale) = if i == 1 {
                        (q.tracked_positions, c.tracked_positions, 2.0)
                    } else {
                        (q.tracked_velocities, c.tracked_velocities, 0.33333334_f32)
                    };
                    let mut sum = 0.0;
                    for j in 0..2 {
                        sum += difference(a[j], b[j], scale)? * 0.5 * weight;
                    }
                    sum
                }
                3 => {
                    let (a, b) = scalar(q.heading_delta, c.heading_delta)?;
                    let d = wrap_angle(b - a);
                    d * d * weight
                }
                4 => {
                    let (a, b) = scalar(q.angular_scalar, c.angular_scalar)?;
                    let d = (b - a) * 0.16666667_f32;
                    d * d * weight
                }
                5 => {
                    difference(q.paired_positions[0], c.paired_positions[0], 1.0)? * weight
                        + difference(q.paired_positions[1], c.paired_positions[1], 1.0)? * weight
                }
                6 => {
                    let (a, b) = scalar(q.scalar68, c.scalar68)?;
                    (a - b) * (a - b) * weight
                }
                _ => unreachable!(),
            }
        };
        result.terms[i] = term;
        result.total += term;
        if !result.total.is_finite() {
            return Err("pose cost overflow");
        }
        // Native stops whenever partial >= bound, including equal scores.
        if i < 6 && prune_at.is_some_and(|bound| result.total >= bound) {
            result.pruned = true;
            return Ok(result);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn neutral() -> LocalPose {
        LocalPose {
            root_velocity: [0.0; 3],
            tracked_positions: [[0.0; 3]; 2],
            tracked_velocities: [[0.0; 3]; 2],
            paired_positions: [[0.0; 3]; 2],
            heading_delta: None,
            angular_scalar: None,
            scalar68: None,
        }
    }
    #[test]
    fn native_scaling_is_not_plain_euclidean_distance() {
        let q = neutral();
        let mut c = neutral();
        c.tracked_positions[0][0] = 1.0;
        c.tracked_velocities[1][1] = 3.0;
        let score = primary_pose_cost(
            &q,
            &c,
            PoseWeights {
                values: [0.0, 5.0, 5.0, 0.0, 0.0, 0.0, 0.0],
            },
            None,
        )
        .unwrap();
        assert_eq!(score.terms[1], 10.0);
        assert_eq!(score.terms[2], 2.5);
        assert_eq!(score.total, 12.5); // Unscaled Euclidean would give 50.
    }
    #[test]
    fn absent_enabled_fields_fail_and_equal_bound_prunes_before_them() {
        let q = neutral();
        let w = PoseWeights {
            values: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
        };
        assert!(primary_pose_cost(&q, &q, w, None).is_err());
        let s = primary_pose_cost(&q, &q, w, Some(0.0)).unwrap();
        assert!(s.pruned);
        assert_eq!(s.total, 0.0);
    }
    #[test]
    fn angular_difference_crosses_seam_without_full_turn_penalty() {
        let mut q = neutral();
        let mut c = neutral();
        q.heading_delta = Some(3.13);
        c.heading_delta = Some(-3.13);
        let s = primary_pose_cost(
            &q,
            &c,
            PoseWeights {
                values: [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
            },
            None,
        )
        .unwrap();
        assert!(s.total < 0.001);
        assert_eq!(wrap_angle(std::f32::consts::PI), -std::f32::consts::PI);
    }
}
