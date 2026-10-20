//! ACU-RE-0031: translation correction from VA 0x140188f00.
//! Inputs are already in the correction frame established by the native caller.
//! This excludes rotation, event lookup, collision and movement application.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationMode {
    /// Native controller +448 == 0: remaining-motion shear and axis scaling.
    RemainingMotion,
    /// Native controller +448 != 0: distribute correction along this step.
    CurrentStep,
}

#[derive(Clone, Copy, Debug)]
pub struct TranslationInput {
    pub step: [f32; 3],
    pub remaining: [f32; 3],
    pub error: [f32; 3],
    pub dt: f32,
    pub time_remaining: f32,
    pub mode: TranslationMode,
}

#[derive(Clone, Copy, Debug)]
pub struct TranslationCorrection {
    /// Added to the animation step after the upstream frame composition.
    pub delta: [f32; 3],
    /// Native tests X/Y residual only, even when Z still needs correction.
    pub horizontal_complete: bool,
}

/// One child target and its translation-event binding in the ordinary branch
/// of 0x141a94ec0 (analysis bytes +366 and +367 both zero).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointTarget {
    /// Resolved point from analysis +384, +400 or +416, in that order.
    pub position: [f32; 3],
    pub translation_event: u32,
    /// Native +68 = 2; preserve the raw value without naming its semantics.
    pub native_kind: u32,
    pub retain_after_event: bool,
}

/// Connect already resolved scanner/analysis points to the native target list.
/// `points` are in world space: resolving +436 and optional +632/+640/+648
/// overrides belongs to the caller. Special +366/+367 branches are unsupported.
/// The native branch emits at most three children; this API rejects larger lists.
pub fn ordinary_point_targets(
    points: &[[f32; 3]],
    relative_frame: bool,
) -> Option<Vec<PointTarget>> {
    if points.len() > 3 || !points.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let events = [8_388_617, 8_388_635, 8_388_636];
    Some(
        points
            .iter()
            .enumerate()
            .map(|(i, &position)| PointTarget {
                position,
                translation_event: events[i],
                native_kind: 2,
                retain_after_event: relative_frame,
            })
            .collect(),
    )
}

/// Timing arithmetic from 0x1401886e0. These are raw event record +20/+24
/// pairs, not guessed clip durations or a fixed prediction horizon.
pub fn event_time_remaining(windows: &[[f32; 2]], offset: f32) -> Option<(f32, bool)> {
    if !offset.is_finite() || offset < 0.0 || !windows.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let mut sum = 0.0;
    let mut active = false;
    for &[start, end] in windows {
        if end > 0.0 || end.abs() <= offset {
            let skip = if start <= 0.0 || end - start <= offset {
                active = true;
                0.0
            } else {
                start
            };
            sum += end.max(offset) - skip;
        }
    }
    sum.is_finite().then_some((sum, active))
}

/// The call-free native excerpt 0x14018979e..0x140189b81.
/// Supported inputs are finite, dt >= 0 and time_remaining > 0. Event expiry
/// and snap branches are selected upstream and must not enter this function.
pub fn translation_correction(input: TranslationInput) -> Option<TranslationCorrection> {
    let TranslationInput {
        step,
        remaining,
        error,
        dt,
        time_remaining,
        mode,
    } = input;
    if !step
        .into_iter()
        .chain(remaining)
        .chain(error)
        .chain([dt, time_remaining])
        .all(f32::is_finite)
        || dt < 0.0
        || time_remaining <= 0.0
    {
        return None;
    }
    let mut shear = [[0.0_f32; 3]; 3];
    for (i, column) in shear.iter_mut().enumerate() {
        column[i] = 1.0;
    }
    let mut scale = [1.0_f32; 3];
    let mut translation = [0.0_f32; 3];
    // Native strictness and tie ordering differ between the two modes.
    let dominant = match mode {
        TranslationMode::RemainingMotion => {
            let a = remaining.map(f32::abs);
            let axis = if a[1] > a[0] && a[1] > a[2] {
                1
            } else if a[0] > a[2] {
                0
            } else {
                2
            };
            (a[axis] > 0.01).then_some(axis)
        }
        TranslationMode::CurrentStep => {
            let a = step.map(f32::abs);
            (0..3).find(|&i| a[i] > 0.01 && a[i] >= a[(i + 1) % 3] && a[i] >= a[(i + 2) % 3])
        }
    };
    match mode {
        TranslationMode::RemainingMotion => {
            if let Some(axis) = dominant {
                let inverse = 1.0 / remaining[axis];
                for (i, value) in shear[axis].iter_mut().enumerate() {
                    if i != axis {
                        *value = inverse * error[i];
                    }
                }
            }
            for i in 0..3 {
                if dominant.is_some_and(|axis| i != axis) {
                    continue;
                }
                if remaining[i].abs() > 0.01 {
                    scale[i] = error[i] / remaining[i] + 1.0;
                } else {
                    translation[i] = (error[i] * dt) / time_remaining;
                }
            }
        }
        TranslationMode::CurrentStep => {
            let inverse_time = 1.0 / time_remaining;
            let correction = error.map(|e| (e * dt) * inverse_time);
            if let Some(axis) = dominant {
                let inverse = 1.0 / step[axis];
                for (i, value) in shear[axis].iter_mut().enumerate() {
                    if i != axis {
                        *value = inverse * correction[i];
                    }
                }
                translation[axis] = correction[axis];
            } else {
                translation = correction;
            }
        }
    }
    let delta = std::array::from_fn(|i| {
        let value = ((translation[i] + step[2] * shear[2][i]) + step[1] * shear[1][i])
            + step[0] * shear[0][i];
        // Preserve the native second matrix multiply, including sum order.
        let after_scale = ((0.0 + value * if i == 2 { scale[2] } else { 0.0 })
            + value * if i == 1 { scale[1] } else { 0.0 })
            + value * if i == 0 { scale[0] } else { 0.0 };
        after_scale - step[i]
    });
    delta
        .into_iter()
        .all(f32::is_finite)
        .then_some(TranslationCorrection {
            delta,
            horizontal_complete: error[0].abs() <= 0.01 && error[1].abs() <= 0.01,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> TranslationInput {
        TranslationInput {
            step: [0.1, 0.0, 0.0],
            remaining: [2.0, 0.0, 0.0],
            error: [0.0, 1.0, 0.5],
            dt: 0.1,
            time_remaining: 2.0,
            mode: TranslationMode::RemainingMotion,
        }
    }
    #[test]
    fn remaining_motion_shears_toward_target() {
        let got = translation_correction(input()).unwrap();
        assert!((got.delta[1] - 0.05).abs() < 1e-6);
        assert!((got.delta[2] - 0.025).abs() < 1e-6);
        assert!(!got.horizontal_complete);
    }
    #[test]
    fn current_step_distributes_error_by_time() {
        let mut x = input();
        x.mode = TranslationMode::CurrentStep;
        let got = translation_correction(x).unwrap();
        assert!((got.delta[1] - 0.05).abs() < 1e-6);
        assert!((got.delta[2] - 0.025).abs() < 1e-6);
    }
    #[test]
    fn vertical_error_does_not_clear_horizontal_completion() {
        let mut x = input();
        x.error = [0.0, 0.0, 10.0];
        assert!(translation_correction(x).unwrap().horizontal_complete);
        x.time_remaining = 0.0;
        assert!(translation_correction(x).is_none());
        x.time_remaining = 1.0;
        x.step[0] = f32::NAN;
        assert!(translation_correction(x).is_none());
    }
    #[test]
    fn selected_landing_is_the_third_event_target() {
        let points = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [4.0, 1.0, -2.0]];
        let targets = ordinary_point_targets(&points, true).unwrap();
        assert_eq!(targets[2].position, points[2]);
        assert_eq!(targets[2].translation_event, 0x80001c);
        assert!(targets.iter().all(|t| t.retain_after_event));
        assert!(ordinary_point_targets(&[[0.0; 3]; 4], false).is_none());
    }
    #[test]
    fn event_horizon_comes_from_windows() {
        assert_eq!(
            event_time_remaining(&[[0.0, 0.4], [0.2, 0.8], [-0.2, -0.1]], 0.0),
            Some((1.0, true))
        );
        assert_eq!(event_time_remaining(&[[0.2, 0.8]], 0.0), Some((0.6, false)));
    }
}
