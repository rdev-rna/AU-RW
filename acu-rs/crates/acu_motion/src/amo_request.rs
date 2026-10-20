//! Dense requested-trajectory stages recovered from 162f778 and 162bc1c.
//! Upstream movement/collision predictor and controller/context resolution remain open.
use crate::amo::wrap_angle;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DenseRequest {
    pub position: [f32; 4],
    pub velocity: [f32; 4],
    pub auxiliary: [f32; 4],
    pub heading: f32,
    pub time: f32,
}
fn finite(rows: &[DenseRequest]) -> Result<(), &'static str> {
    if rows.len() > 0x3fff
        || rows.iter().any(|r| {
            !r.position
                .iter()
                .chain(r.velocity.iter())
                .chain(r.auxiliary.iter())
                .chain([r.heading, r.time].iter())
                .all(|v| v.is_finite())
        })
    {
        return Err("invalid dense requested records");
    }
    Ok(())
}
/// Two passes over all FOUR vector components. Endpoints are one-sided;
/// interiors compare next/previous, divided by their actual time difference.
/// Native count1 divides by zero; reject it instead of inventing zero derivatives.
pub fn differentiate(rows: &[DenseRequest]) -> Result<Vec<DenseRequest>, &'static str> {
    finite(rows)?;
    if rows.len() < 2 || rows.windows(2).any(|w| w[1].time <= w[0].time) {
        return Err("invalid derivative time grid");
    }
    let mut out = rows.to_vec();
    for i in 0..rows.len() {
        let a = i.saturating_sub(1);
        let b = (i + 1).min(rows.len() - 1);
        let dt = rows[b].time - rows[a].time;
        for j in 0..4 {
            out[i].velocity[j] = (rows[b].position[j] - rows[a].position[j]) / dt;
        }
    }
    for i in 0..rows.len() {
        let a = i.saturating_sub(1);
        let b = (i + 1).min(rows.len() - 1);
        let dt = rows[b].time - rows[a].time;
        for j in 0..4 {
            out[i].auxiliary[j] = (out[b].velocity[j] - out[a].velocity[j]) / dt;
        }
    }
    finite(&out)?;
    Ok(out)
}
/// 162bc1c returns without writing when count<=1; None preserves that branch.
/// Native first-step index arithmetic assumes a zero-origin uniform dense grid.
/// Reject unsupported grids instead of replacing it with generic time search.
pub fn resample(
    rows: &[DenseRequest],
    query_time: f32,
    setting_a0: f32,
) -> Result<Option<DenseRequest>, &'static str> {
    finite(rows)?;
    if !query_time.is_finite() || !setting_a0.is_finite() {
        return Err("invalid request sample time");
    }
    if rows.len() < 2 {
        return Ok(None);
    }
    let dt = rows[1].time - rows[0].time;
    if dt <= 0.
        || rows[0].time != 0.
        || rows
            .iter()
            .enumerate()
            .any(|(i, r)| (r.time - i as f32 * dt).abs() > 1e-5_f32.max(r.time.abs() * 1e-5))
    {
        return Err("unsupported requested resampling grid");
    }
    let time = (query_time - setting_a0)
        .max(0.)
        .min(rows.last().unwrap().time);
    let index = ((time / dt).trunc() as usize).min(rows.len() - 2);
    let fraction = (time - index as f32 * dt) / dt;
    let inverse = 1. - fraction;
    let a = &rows[index];
    let b = &rows[index + 1];
    let mix = |x: [f32; 4], y: [f32; 4]| std::array::from_fn(|i| y[i] * fraction + x[i] * inverse);
    let out = DenseRequest {
        position: mix(a.position, b.position),
        velocity: mix(a.velocity, b.velocity),
        auxiliary: mix(a.auxiliary, b.auxiliary),
        heading: wrap_angle(wrap_angle(b.heading - a.heading) * fraction + a.heading),
        time,
    };
    finite(std::slice::from_ref(&out))?;
    Ok(Some(out))
}

/// Recovered leaf 34ad48. Uses conjugate(q)/norm_squared, not normalization.
/// Native SIMD discards W. Finite, nonzero-norm inputs only; no NaN parity claim.
pub fn rotate_native(q: [f32; 4], v: [f32; 4]) -> Result<[f32; 4], &'static str> {
    if !q.iter().chain(v.iter()).all(|x| x.is_finite()) {
        return Err("invalid local transform");
    }
    let sq = q.map(|x| x * x);
    let norm = (sq[0] + sq[2]) + (sq[1] + sq[3]);
    if !norm.is_finite() || norm <= 0. {
        return Err("invalid quaternion norm");
    }
    let r = [-q[0] / norm, -q[1] / norm, -q[2] / norm, q[3] / norm];
    let dot = r[0] * v[0] + (r[1] * v[1] + r[2] * v[2]);
    let cross = [
        r[1] * v[2] - v[1] * r[2],
        r[2] * v[0] - v[2] * r[0],
        r[0] * v[1] - v[0] * r[1],
    ];
    let mut out = [0.; 4];
    for i in 0..3 {
        out[i] = ((cross[i] * r[3] + dot * r[i]) + (r[3] * r[3] - 0.5) * v[i]) * 2.;
    }
    if !out.iter().all(|x| x.is_finite()) {
        return Err("nonfinite local transform result");
    }
    Ok(out)
}
/// Resolved controller840 quaternion and controller7b0/3c0 origins.
/// Caller must resolve the native base-heading (including mode12 override).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalFrame {
    pub rotation: [f32; 4],
    pub origin_7b0: [f32; 4],
    pub origin_3c0: [f32; 4],
    pub base_heading: f32,
}
/// 162fd68 horizon conversion: translate positions only; rotate all vectors.
/// Heading wraps relative to the resolved base; time is copied unchanged.
pub fn assemble_local(
    rows: &[DenseRequest],
    frame: &LocalFrame,
) -> Result<Vec<DenseRequest>, &'static str> {
    finite(rows)?;
    if !frame
        .origin_7b0
        .iter()
        .chain(frame.origin_3c0.iter())
        .chain([frame.base_heading].iter())
        .all(|x| x.is_finite())
    {
        return Err("invalid local query frame");
    }
    rows.iter()
        .map(|r| {
            let position = std::array::from_fn(|i| {
                r.position[i] - (frame.origin_7b0[i] - frame.origin_3c0[i])
            });
            let out = DenseRequest {
                position: rotate_native(frame.rotation, position)?,
                velocity: rotate_native(frame.rotation, r.velocity)?,
                auxiliary: rotate_native(frame.rotation, r.auxiliary)?,
                heading: wrap_angle(r.heading - frame.base_heading),
                time: r.time,
            };
            finite(std::slice::from_ref(&out))?;
            Ok(out)
        })
        .collect()
}
/// Complete recovered numeric chain for caller-supplied dense POSITIONS.
/// Upstream movement/collision generation and horizon policy remain caller work.
pub fn prepare_horizon(
    dense: &[DenseRequest],
    times: &[f32],
    setting_a0: f32,
    frame: &LocalFrame,
) -> Result<Vec<DenseRequest>, &'static str> {
    let derivatives = differentiate(dense)?;
    let sampled = times
        .iter()
        .map(|&time| {
            let mut row = resample(&derivatives, time, setting_a0)?.ok_or("no requested sample")?;
            // Native array560 holds the requested horizon, not clamped/offset sample time.
            row.time = time;
            Ok(row)
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    assemble_local(&sampled, frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(t: f32, x: f32, w: f32) -> DenseRequest {
        DenseRequest {
            position: [x, 0., 0., w],
            velocity: [0.; 4],
            auxiliary: [0.; 4],
            heading: 0.,
            time: t,
        }
    }
    #[test]
    fn two_pass_derivatives_keep_w_and_one_sided_endpoints() {
        let r = differentiate(&[row(0., 0., 0.), row(1., 1., 2.), row(2., 4., 8.)]).unwrap();
        assert_eq!(
            r.iter().map(|x| x.velocity[0]).collect::<Vec<_>>(),
            [1., 2., 3.]
        );
        assert_eq!(
            r.iter().map(|x| x.auxiliary[0]).collect::<Vec<_>>(),
            [1., 1., 1.]
        );
        assert_eq!(r[1].velocity[3], 4.);
        assert_eq!(r[1].auxiliary[3], 2.);
    }
    #[test]
    fn resampler_preserves_short_arc_and_no_write_branch() {
        let mut r = [row(0., 0., 0.), row(1., 2., 0.)];
        r[0].heading = 3.1;
        r[1].heading = -3.1;
        let x = resample(&r, 0.75, 0.25).unwrap().unwrap();
        assert_eq!(x.position[0], 1.);
        assert!(x.heading.abs() > 3.1);
        assert!(resample(&r[..1], 0., 0.).unwrap().is_none());
        assert!(differentiate(&r[..1]).is_err());
    }
    #[test]
    fn local_transform_preserves_native_nonunit_behavior() {
        assert_eq!(
            rotate_native([0., 0., 0., 2.], [2., 4., 6., 8.]).unwrap(),
            [-1., -2., -3., 0.]
        );
        assert_eq!(
            rotate_native([0., 0., 0., 1.], [2., 4., 6., 8.]).unwrap(),
            [2., 4., 6., 0.]
        );
        assert!(rotate_native([0.; 4], [0.; 4]).is_err());
    }
    #[test]
    fn horizon_uses_original_times_and_translates_only_position() {
        let frame = LocalFrame {
            rotation: [0., 0., 0., 1.],
            origin_7b0: [5., 0., 0., 0.],
            origin_3c0: [2., 0., 0., 0.],
            base_heading: 0.,
        };
        let out = prepare_horizon(
            &[row(0., 0., 1.), row(1., 2., 3.)],
            &[0.75, 2.],
            0.25,
            &frame,
        )
        .unwrap();
        assert_eq!(out[0].position, [-2., 0., 0., 0.]);
        assert_eq!(out[0].velocity, [2., 0., 0., 0.]);
        assert_eq!(out[0].auxiliary, [0.; 4]);
        assert_eq!(out[0].time, 0.75);
        assert_eq!(out[1].time, 2.);
        assert_eq!(out[1].position[0], -1.);
    }
}
