//! Recovered prediction geometry, gates and isolated penalties within 0x1631ba0.
//! Ordered prepared-frame scoring is ported; native query production and
//! bit-identical quaternion transforms remain open.
use crate::{
    amo::wrap_angle,
    amo_samples::{SampleBank, SampleCursor, mapped_sample_index},
    amo_search::logical_at_time,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FutureLocation {
    pub physical: usize,
    pub valid: bool,
}
/// RVA 0x1f36eb0. Removed logical frames read sample zero with validity=false.
/// No interpolation or query sampler's half-frame recovery occurs here.
pub fn future_location(
    bank: &SampleBank<'_>,
    time: f32,
    dt: f32,
    removed: u32,
    table: &[u32],
) -> Result<FutureLocation, &'static str> {
    let logical = logical_at_time(time, dt, bank.len(), removed)?;
    let mapped = mapped_sample_index(table, logical)?;
    if mapped < 0 {
        return Ok(FutureLocation {
            physical: 0,
            valid: false,
        });
    }
    if mapped as usize >= bank.len() {
        return Err("invalid future mapped sample");
    }
    Ok(FutureLocation {
        physical: mapped as usize,
        valid: true,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PredictionFrame {
    /// Native transformed displacement +00, derivative +10, auxiliary vector +40.
    /// W is forced to zero by native rotation; requested W remains in cost.
    pub position: [f32; 4],
    pub velocity: [f32; 4],
    pub auxiliary40: [f32; 4],
    /// wrap(future+54 - candidate+54), before comparison to requested final heading.
    pub relative_yaw: f32,
    pub valid: bool,
    pub flags140: u32,
}
/// Resolve one candidate future record and express it in the candidate's frame.
/// Geometry uses ordinary trig, not a bit-certified port of the native quaternion helper.
pub fn candidate_future(
    bank: &SampleBank<'_>,
    cursor: SampleCursor,
    time: f32,
    dt: f32,
    removed: u32,
    table: &[u32],
    scale: f32,
) -> Result<PredictionFrame, &'static str> {
    if !scale.is_finite() || scale < 0.0 {
        return Err("invalid prediction scale");
    }
    let root = bank.vector(cursor, 0)?;
    let yaw = bank.scalar(cursor, 0x54)?;
    let location = future_location(bank, time, dt, removed, table)?;
    let next = SampleCursor {
        index: location.physical as i32,
        fraction: 0.0,
    };
    let mut position = bank.vector(next, 0)?;
    for i in 0..4 {
        position[i] -= root[i];
    }
    let rotate = |v: [f32; 4]| {
        let (s, c) = yaw.sin_cos();
        [
            (c * v[0] + s * v[1]) * scale,
            (-s * v[0] + c * v[1]) * scale,
            v[2] * scale,
            0.0,
        ]
    };
    Ok(PredictionFrame {
        position: rotate(position),
        velocity: rotate(bank.vector(next, 0x10)?),
        auxiliary40: rotate(bank.vector(next, 0x40)?),
        relative_yaw: wrap_angle(bank.scalar(next, 0x54)? - yaw),
        valid: location.valid,
        flags140: bank.sample_flags(location.physical)?,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestedFrame {
    pub position: [f32; 4],
    pub velocity: [f32; 4],
    pub auxiliary40: [f32; 4],
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PredictionWeights {
    /// Already gated weights from +4c,+50,+54,+5c or +60. No implicit defaults.
    pub values: [f32; 4],
    /// +91 supplies a .7 radian heading deadzone; ordinary mode supplies zero.
    pub heading_deadzone: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PredictionCost {
    /// invalid future, position, derivative, auxiliary +40, relative heading.
    pub terms: [f32; 5],
    /// Incremental prediction contribution: native subtracts entry accumulator.
    pub total: f32,
    pub pruned: bool,
}
fn distance(a: [f32; 4], b: [f32; 4]) -> Result<f32, &'static str> {
    if !a.iter().chain(b.iter()).all(|v| v.is_finite()) {
        return Err("nonfinite prediction frame");
    }
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
    Ok((d[0] * d[0] + d[2] * d[2]) + (d[1] * d[1] + d[3] * d[3]))
}
/// Core prediction contributions in native loop order. Not full 0x1631ba0 cost.
/// Native desired yaw is final requested point +30 for EVERY horizon point.
/// Geometry and heading terms are averaged over horizon count; last heading costs 5x.
/// initial_total is the already accumulated candidate cost, used for pruning only.
pub fn prediction_geometry_cost(
    requested: &[RequestedFrame],
    candidate: &[PredictionFrame],
    final_yaw: f32,
    w: PredictionWeights,
    initial_total: f32,
    prune_at: Option<f32>,
) -> Result<PredictionCost, &'static str> {
    if requested.is_empty() || requested.len() != candidate.len() || requested.len() > 0x3fff {
        return Err("invalid prediction horizon");
    }
    if !w.values.iter().all(|v| v.is_finite() && *v >= 0.0)
        || !w.heading_deadzone.is_finite()
        || w.heading_deadzone < 0.0
        || !final_yaw.is_finite()
        || !initial_total.is_finite()
        || prune_at.is_some_and(|b| !b.is_finite())
    {
        return Err("invalid prediction cost settings");
    }
    let mut cost = PredictionCost {
        terms: [0.0; 5],
        total: initial_total,
        pruned: false,
    };
    let average = 1.0 / requested.len() as f32;
    for (i, (q, c)) in requested.iter().zip(candidate).enumerate() {
        // Clip-end invalidity is caller-reflected in valid; flag bit 4 is independent.
        let invalid = if !c.valid || c.flags140 & 0x10 != 0 {
            1000.0
        } else {
            0.0
        };
        for term in 0..5 {
            let value = match term {
                0 => invalid,
                1..=3 => {
                    let weight = w.values[term - 1];
                    if weight == 0.0 {
                        0.0
                    } else {
                        let (a, b) = match term {
                            1 => (q.position, c.position),
                            2 => (q.velocity, c.velocity),
                            _ => (q.auxiliary40, c.auxiliary40),
                        };
                        distance(a, b)? * average * weight
                    }
                }
                _ => {
                    if w.values[3] == 0.0 {
                        0.0
                    } else {
                        if !c.relative_yaw.is_finite() {
                            return Err("nonfinite prediction yaw");
                        }
                        let error = (wrap_angle(c.relative_yaw - final_yaw).abs()
                            - w.heading_deadzone)
                            .max(0.0);
                        error
                            * error
                            * average
                            * w.values[3]
                            * if i + 1 == candidate.len() { 5.0 } else { 1.0 }
                    }
                }
            };
            cost.terms[term] += value;
            cost.total += value;
            if !cost.total.is_finite() {
                return Err("prediction cost overflow");
            }
            if prune_at.is_some_and(|b| cost.total >= b) {
                cost.pruned = true;
                cost.total -= initial_total;
                return Ok(cost);
            }
        }
    }
    cost.total -= initial_total;
    Ok(cost)
}
/// Clip-end validity in RVA 1632154..16321a4: COMISS / CMOVA accepts
/// equality to duration, unlike the misleading >= expression in decompiled C.
/// Sample map validity and raw flag bit4 are independent additional conditions.
pub fn future_is_valid(
    time: f32,
    duration: f32,
    mapped_valid: bool,
    flags140: u32,
) -> Result<bool, &'static str> {
    if !time.is_finite() || !duration.is_finite() || duration < 0.0 {
        return Err("invalid prediction clip time");
    }
    Ok(time <= duration && mapped_valid && flags140 & 0x10 == 0)
}

/// Settings +4c,+50,+54,+58,+5c,+60,+64,+68,+88,+b8,+84, respectively.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GateSettings {
    pub values: [f32; 11],
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PredictionContext {
    pub flag90: bool,
    pub flag91: bool,
    pub flag96: bool,
    pub stage38: i32,
    pub mode30: i32,
    pub plane10: [f32; 4],
    pub point20: [f32; 4],
    pub current50: [f32; 4],
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResolvedWeights {
    pub geometry: PredictionWeights,
    pub radial: f32,
}
fn finite_settings(s: GateSettings, ctx: PredictionContext) -> Result<(), &'static str> {
    if !s.values.iter().all(|v| v.is_finite() && *v >= 0.0)
        || !ctx
            .plane10
            .iter()
            .chain(ctx.point20.iter())
            .chain(ctx.current50.iter())
            .all(|v| v.is_finite())
    {
        return Err("invalid prediction gates");
    }
    Ok(())
}
/// Native prediction setup: derivative/auxiliary gates, moving-heading weight,
/// flag90 position multiplier, flag91 deadzone, flag96 radial-distance weight.
pub fn resolve_weights(
    s: GateSettings,
    ctx: PredictionContext,
    last_velocity: [f32; 4],
) -> Result<ResolvedWeights, &'static str> {
    finite_settings(s, ctx)?;
    if !last_velocity.iter().all(|v| v.is_finite()) {
        return Err("nonfinite requested velocity");
    }
    let special = ctx.flag90 || ctx.flag91;
    let dot = ctx.current50[0] * last_velocity[0] + ctx.current50[1] * last_velocity[1];
    let speed2 = (last_velocity[1] * last_velocity[1] + last_velocity[3] * last_velocity[3])
        + (last_velocity[0] * last_velocity[0] + last_velocity[2] * last_velocity[2]);
    let result = ResolvedWeights {
        geometry: PredictionWeights {
            values: [
                s.values[0] * if ctx.flag90 { s.values[9] } else { 1.0 },
                if special { 0.0 } else { s.values[1] },
                if special || dot <= 0.0 || ctx.flag96 {
                    0.0
                } else {
                    s.values[2]
                },
                if speed2 > 0.010000001_f32 {
                    s.values[5]
                } else {
                    s.values[4]
                },
            ],
            heading_deadzone: if ctx.flag91 { 0.7 } else { 0.0 },
        },
        radial: if ctx.flag96 && !special {
            s.values[3]
        } else {
            0.0
        },
    };
    if !result.geometry.values.iter().all(|v| v.is_finite()) {
        return Err("prediction gate overflow");
    }
    Ok(result)
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PenaltyInput {
    pub candidate_position: [f32; 4],
    pub candidate_velocity: [f32; 4],
    pub current_candidate_velocity: [f32; 4],
    pub desired_position: [f32; 4],
    pub desired_velocity: [f32; 4],
    pub last_position: [f32; 4],
    pub last_velocity: [f32; 4],
    pub desired_heading: f32,
    /// Unscaled WORLD vertical displacement, distinct from scaled local position.z.
    pub raw_vertical: f32,
    pub position_error_squared: f32,
    pub heading_error: f32,
    pub flags140: u32,
    pub is_last: bool,
    pub horizon_count: usize,
}
/// Additional recovered pure contributions. Each is returned separately because
/// native interleaves them with geometry; summing these after geometry is NOT
/// a port of native pruning order. Mode/submode constraints are supplied separately below.
pub fn prediction_penalties(
    s: GateSettings,
    ctx: PredictionContext,
    x: PenaltyInput,
) -> Result<[f32; 8], &'static str> {
    selected_penalties(s, ctx, x, None)
}

fn selected_penalties(
    s: GateSettings,
    ctx: PredictionContext,
    x: PenaltyInput,
    selected: Option<usize>,
) -> Result<[f32; 8], &'static str> {
    let want = |i| selected.is_none_or(|j| i == j);
    finite_settings(s, ctx)?;
    if x.horizon_count == 0
        || x.horizon_count > 0x3fff
        || !x
            .candidate_position
            .iter()
            .chain(x.candidate_velocity.iter())
            .chain(x.current_candidate_velocity.iter())
            .chain(x.desired_position.iter())
            .chain(x.desired_velocity.iter())
            .chain(x.last_position.iter())
            .chain(x.last_velocity.iter())
            .chain(
                [
                    x.desired_heading,
                    x.raw_vertical,
                    x.position_error_squared,
                    x.heading_error,
                ]
                .iter(),
            )
            .all(|v| v.is_finite())
    {
        return Err("invalid prediction penalty inputs");
    }
    let w = resolve_weights(s, ctx, x.last_velocity)?;
    let average = 1.0 / x.horizon_count as f32;
    let special = ctx.flag90 || ctx.flag91;
    let mut terms = [0.0; 8];
    // Radial term uses squared planar radii capped at 4, then blends squared
    // difference toward a radius ratio above the native .1/.01 thresholds.
    if want(0) && w.radial > 0.0 {
        let desired = (x.desired_position[0] * x.desired_position[0]
            + x.desired_position[1] * x.desired_position[1])
            .min(4.0);
        let candidate = (x.candidate_position[0] * x.candidate_position[0]
            + x.candidate_position[1] * x.candidate_position[1])
            .min(4.0);
        let d = candidate - desired;
        let mut value = d * d * average * w.radial;
        let blend = ((desired - 0.1) / 0.9).clamp(0.0, 1.0);
        if blend > 0.0 && desired > 0.1 && candidate > 0.01 {
            let ratio = if d > 0.0 {
                candidate / desired
            } else {
                desired / candidate
            };
            let alternate = (ratio - 1.0).max(0.0) * average * w.radial;
            value += (alternate - value) * blend;
        }
        terms[0] = value;
    }
    if want(1) && !special && ctx.stage38 == 0 && x.flags140 & 4 != 0 {
        terms[1] = s.values[8];
    }
    if want(2)
        && x.is_last
        && ctx.flag90
        && ctx.stage38 == 0
        && (x.heading_error.abs() >= 1.0
            || x.position_error_squared >= 0.040000003_f32
            || x.flags140 & 4 == 0)
    {
        terms[2] = 10.0;
    }
    let plane_active = ctx.plane10.iter().any(|v| v.abs() > 0.0005);
    if want(3) && plane_active {
        let d = -(0..4)
            .map(|i| (x.candidate_position[i] - ctx.point20[i]) * ctx.plane10[i])
            .sum::<f32>();
        if d > 0.0 {
            terms[3] = (d + d) * (d + d) * s.values[7];
        }
    }
    let vertical_gate =
        !special && ctx.stage38 == 0 && ctx.mode30 == 1 && x.desired_heading.abs() < 1.5;
    let small_high = x.last_position[2] > 0.5
        && x.last_position[0] * x.last_position[0] + x.last_position[1] * x.last_position[1] < 0.09;
    let desired_z = x.desired_position[2];
    let error = x.raw_vertical - desired_z;
    if want(4) && x.is_last && vertical_gate && !small_high {
        let mut multiplier = 2.0;
        if desired_z.abs() > 2.0 && x.raw_vertical.abs() < desired_z.abs() {
            multiplier = 6.0;
        }
        if desired_z.abs() < 2.0 && desired_z.abs() < x.raw_vertical.abs() {
            multiplier *= 100.0;
        }
        terms[4] = error * error * multiplier;
    }
    if want(5) && vertical_gate && x.desired_velocity[2].abs() < 0.01 && desired_z < -0.1 {
        terms[5] = error * error * 2000.0;
    }
    let desired_speed2 =
        x.last_velocity[0] * x.last_velocity[0] + x.last_velocity[1] * x.last_velocity[1];
    let dot = ctx.current50[0] * x.last_velocity[0] + ctx.current50[1] * x.last_velocity[1];
    if want(6)
        && x.is_last
        && !special
        && !plane_active
        && ctx.stage38 == 0
        && !ctx.flag96
        && desired_speed2 >= 1.0
        && dot <= 0.0
    {
        let dx = (x.candidate_velocity[0] - x.current_candidate_velocity[0])
            - (x.last_velocity[0] - ctx.current50[0]);
        let dy = (x.candidate_velocity[1] - x.current_candidate_velocity[1])
            - (x.last_velocity[1] - ctx.current50[1]);
        terms[6] = (dx * dx + dy * dy) * s.values[6];
    }
    // Diagnostic 23 is present in assembly (1632b61..1632bc2), although the
    // decompiled C omits the addition: setting+84 when a fast request ends slow.
    let candidate_speed2 = x.candidate_velocity[0] * x.candidate_velocity[0]
        + x.candidate_velocity[1] * x.candidate_velocity[1];
    if want(7)
        && x.is_last
        && !special
        && !plane_active
        && ctx.stage38 == 0
        && desired_speed2 >= 1.0
        && candidate_speed2 < 1.0
    {
        terms[7] = s.values[10];
    }
    if !terms.iter().all(|v| v.is_finite()) {
        return Err("prediction penalty overflow");
    }
    Ok(terms)
}

/// Final-frame constraints from RVA 1631ee2..1632035 and 16326a3..1632759.
/// Raw resource counts/classes must be supplied from their native fields, not
/// substituted with pose-bank length or inferred gameplay labels.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ConstraintContext {
    pub enable_dc: u8,
    pub mode30: u32,
    pub submode34: u32,
    pub stage38: i32,
    pub p_count60: u32,
    pub group_has_class_fb5e7753: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstraintGates {
    pub mode: bool,
    pub submode: bool,
}
pub fn constraint_gates(c: ConstraintContext) -> ConstraintGates {
    let enabled = c.enable_dc != 0 && c.stage38 == 0;
    ConstraintGates {
        mode: enabled && c.mode30 != 1 && c.p_count60 & 0x3fff > 1 && !c.group_has_class_fb5e7753,
        submode: enabled && c.mode30 == 4 && matches!(c.submode34, 1 | 2),
    }
}
/// Raw integer compares, not floating-point conversions. No horizon averaging.
/// These contributions must be interleaved with native per-point pruning.
pub fn prediction_constraints(
    c: ConstraintContext,
    raw_mode60: u32,
    raw_submode6c: u32,
    is_last: bool,
) -> [f32; 2] {
    let g = constraint_gates(c);
    [
        if is_last && g.mode && raw_mode60 != c.mode30 {
            10000.
        } else {
            0.
        },
        if is_last && g.submode && matches!(raw_submode6c, 1 | 2) && raw_submode6c != c.submode34 {
            100.
        } else {
            0.
        },
    ]
}

/// Inputs already expressed in the candidate frame; raw world Z and sample
/// integer words remain separate. This is not a native trajectory producer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderedPoint {
    pub requested: RequestedFrame,
    pub candidate: PredictionFrame,
    pub requested_heading: f32,
    pub raw_vertical: f32,
    pub raw_constraints: [u32; 2],
    pub time: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderedPredictionCost {
    /// geometry[0..5], penalties[5..13], mode/submode[13..15].
    pub terms: [f32; 15],
    pub total: f32,
    pub pruned: bool,
    /// Point index and semantic term index at the native pruning checkpoint.
    pub stopped_at: Option<[usize; 2]>,
}
/// RVA1631ba0 prepared-frame contribution order and cost-bound checkpoints.
/// Returns the prediction increment (subtracts initial_total exactly once).
/// Inputs are finite validated Rust records; native NaN/debug-handle behavior
/// and native quaternion production are outside this interface's scope.
pub fn ordered_prediction_cost(
    points: &[OrderedPoint],
    s: GateSettings,
    ctx: PredictionContext,
    constraints: ConstraintContext,
    current_candidate_velocity: [f32; 4],
    final_yaw: f32,
    duration: f32,
    initial_total: f32,
    prune_at: Option<f32>,
) -> Result<OrderedPredictionCost, &'static str> {
    ordered_prediction_from(
        points.len(),
        points
            .last()
            .ok_or("empty prediction horizon")?
            .requested
            .clone(),
        |i| Ok(points[i].clone()),
        s,
        ctx,
        constraints,
        current_candidate_velocity,
        final_yaw,
        duration,
        initial_total,
        prune_at,
    )
}
fn ordered_prediction_from<F>(
    count: usize,
    last_requested: RequestedFrame,
    mut point_at: F,
    s: GateSettings,
    ctx: PredictionContext,
    constraints: ConstraintContext,
    current_candidate_velocity: [f32; 4],
    final_yaw: f32,
    duration: f32,
    initial_total: f32,
    prune_at: Option<f32>,
) -> Result<OrderedPredictionCost, &'static str>
where
    F: FnMut(usize) -> Result<OrderedPoint, &'static str>,
{
    if count == 0
        || count > 0x3fff
        || !initial_total.is_finite()
        || !final_yaw.is_finite()
        || !duration.is_finite()
        || duration < 0.
        || prune_at.is_some_and(|v| !v.is_finite())
        || constraints.mode30 != ctx.mode30 as u32
        || constraints.stage38 != ctx.stage38
    {
        return Err("invalid ordered prediction context");
    }
    finite_settings(s, ctx)?;
    let weights = resolve_weights(s, ctx, last_requested.velocity)?;
    let mut last_point = None;
    let mut out = OrderedPredictionCost {
        terms: [0.; 15],
        total: initial_total,
        pruned: false,
        stopped_at: None,
    };
    let average = 1. / count as f32;
    let plane = ctx.plane10.iter().any(|v| v.abs() > 0.0005);
    // Native per-point sequence: invalid, position, radial, derivative, aux,
    // raw flag, heading, final stop, mode, submode, plane, vertical, down-Z.
    for i in 0..count {
        let p = point_at(i)?;
        let c = &p.candidate;
        let q = &p.requested;
        let is_last = i + 1 == count;
        let error_squared = distance(q.position, c.position)?;
        if !p.requested_heading.is_finite()
            || !p.raw_vertical.is_finite()
            || !c.relative_yaw.is_finite()
        {
            return Err("nonfinite ordered prediction point");
        }
        let heading = wrap_angle(c.relative_yaw - final_yaw);
        let x = PenaltyInput {
            candidate_position: c.position,
            candidate_velocity: c.velocity,
            current_candidate_velocity,
            desired_position: q.position,
            desired_velocity: q.velocity,
            last_position: last_requested.position,
            last_velocity: last_requested.velocity,
            desired_heading: p.requested_heading,
            raw_vertical: p.raw_vertical,
            position_error_squared: error_squared,
            heading_error: heading,
            flags140: c.flags140,
            is_last,
            horizon_count: count,
        };
        let invalid = !future_is_valid(p.time, duration, c.valid, c.flags140)?;
        let vertical = !ctx.flag90
            && !ctx.flag91
            && ctx.stage38 == 0
            && ctx.mode30 == 1
            && p.requested_heading.abs() < 1.5;
        let small_high = last_requested.position[2] > 0.5
            && last_requested.position[0] * last_requested.position[0]
                + last_requested.position[1] * last_requested.position[1]
                < 0.09;
        for term in [0, 1, 5, 2, 3, 6, 4, 7, 13, 14, 8, 9, 10] {
            let value = match term {
                0 => {
                    if invalid {
                        1000.
                    } else {
                        0.
                    }
                }
                1 => error_squared * average * weights.geometry.values[0],
                2 | 3 => {
                    let weight = weights.geometry.values[term - 1];
                    if weight == 0. {
                        0.
                    } else {
                        let (a, b) = if term == 2 {
                            (q.velocity, c.velocity)
                        } else {
                            (q.auxiliary40, c.auxiliary40)
                        };
                        distance(a, b)? * average * weight
                    }
                }
                4 => {
                    let e = if weights.geometry.values[3] > 0. {
                        (heading.abs() - weights.geometry.heading_deadzone).max(0.)
                    } else {
                        0.
                    };
                    e * e * average * weights.geometry.values[3] * if is_last { 5. } else { 1. }
                }
                13 | 14 => prediction_constraints(
                    constraints,
                    p.raw_constraints[0],
                    p.raw_constraints[1],
                    is_last,
                )[term - 13],
                _ => selected_penalties(s, ctx, x, Some(term - 5))?[term - 5],
            };
            let checkpoint = match term {
                0 => invalid,
                1 | 6 => true,
                2 | 3 => weights.geometry.values[term - 1] > 0.,
                4 => weights.geometry.values[3] > 0.,
                5 => weights.radial > 0.,
                7 => is_last,
                13 | 14 => value != 0.,
                8 => {
                    plane
                        && -(0..4)
                            .map(|j| (c.position[j] - ctx.point20[j]) * ctx.plane10[j])
                            .sum::<f32>()
                            > 0.
                }
                9 => is_last && vertical && !small_high,
                10 => vertical && q.velocity[2].abs() < 0.01 && q.position[2] < -0.1,
                _ => false,
            };
            out.terms[term] += value;
            out.total += value;
            if !out.total.is_finite() {
                return Err("ordered prediction overflow");
            }
            if checkpoint && prune_at.is_some_and(|b| out.total >= b) {
                out.pruned = true;
                out.stopped_at = Some([i, term]);
                out.total -= initial_total;
                return Ok(out);
            }
        }
        last_point = Some(p);
    }
    // Tail velocity-change and assembly-only stop84 are outside the horizon loop.
    let p = last_point.ok_or("empty prediction horizon")?;
    let c = &p.candidate;
    let q = &p.requested;
    let x = PenaltyInput {
        candidate_position: c.position,
        candidate_velocity: c.velocity,
        current_candidate_velocity,
        desired_position: q.position,
        desired_velocity: q.velocity,
        last_position: q.position,
        last_velocity: q.velocity,
        desired_heading: p.requested_heading,
        raw_vertical: p.raw_vertical,
        position_error_squared: 0.,
        heading_error: 0.,
        flags140: c.flags140,
        is_last: true,
        horizon_count: count,
    };
    for term in [11, 12] {
        // A pre-tail bound check is part of native control flow even if the prior
        // per-point term was disabled. Usually the final-stop checkpoint caught it.
        if prune_at.is_some_and(|b| out.total >= b) {
            out.pruned = true;
            out.total -= initial_total;
            return Ok(out);
        }
        let value = selected_penalties(s, ctx, x, Some(term - 5))?[term - 5];
        out.terms[term] += value;
        out.total += value;
        if !out.total.is_finite() {
            return Err("ordered prediction overflow");
        }
        // Native compares after term64 only to decide whether to execute84.
        if term == 11 && prune_at.is_some_and(|b| out.total >= b) {
            out.pruned = true;
            out.stopped_at = Some([count - 1, term]);
            out.total -= initial_total;
            return Ok(out);
        }
    }
    out.total -= initial_total;
    Ok(out)
}

/// Explicit native requested record fields; producer remains caller-supplied.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestedPredictionPoint {
    pub frame: RequestedFrame,
    pub heading: f32,
    pub offset_time: f32,
}
/// Sampling inputs, deliberately distinct from native resource-handle eligibility.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BankPredictionInput {
    pub index: i32,
    pub fraction: f32,
    pub base_time: f32,
    pub dt: f32,
    pub removed: u32,
    pub scale: f32,
    pub duration: f32,
}
/// Raw bank -> lazily sampled prepared point -> recovered ordered score.
/// Candidate root is interpolated; future points use nearest mapped raw records.
/// Only reached future records are read, preserving pruning short-circuiting.
/// Geometry uses the existing inverse-yaw trig transform, not bit-certified
/// native quaternion arithmetic. Group/count facts and requests are explicit.
pub fn bank_prediction_cost(
    bank: &SampleBank<'_>,
    table: &[u32],
    input: BankPredictionInput,
    requested: &[RequestedPredictionPoint],
    s: GateSettings,
    ctx: PredictionContext,
    constraints: ConstraintContext,
    initial_total: f32,
    prune_at: Option<f32>,
) -> Result<OrderedPredictionCost, &'static str> {
    if input.index < 0
        || !input.base_time.is_finite()
        || input.base_time < 0.
        || !input.scale.is_finite()
        || input.scale < 0.
    {
        return Err("invalid bank prediction input");
    }
    let cursor = SampleCursor {
        index: input.index,
        fraction: input.fraction,
    };
    let root = bank.vector(cursor, 0)?;
    let yaw = bank.scalar(cursor, 0x54)?;
    let v = bank.vector(cursor, 0x10)?;
    let (sin, cos) = yaw.sin_cos();
    // Initial candidate derivative is unscaled, unlike scaled future derivative.
    let current = [cos * v[0] + sin * v[1], -sin * v[0] + cos * v[1], v[2], 0.];
    let last = requested.last().ok_or("empty prediction horizon")?;
    ordered_prediction_from(
        requested.len(),
        last.frame.clone(),
        |i| {
            let q = &requested[i];
            let time = input.base_time + q.offset_time;
            let location = future_location(bank, time, input.dt, input.removed, table)?;
            let raw = SampleCursor {
                index: location.physical as i32,
                fraction: 0.,
            };
            let candidate = candidate_future(
                bank,
                cursor,
                time,
                input.dt,
                input.removed,
                table,
                input.scale,
            )?;
            Ok(OrderedPoint {
                requested: q.frame.clone(),
                candidate,
                requested_heading: q.heading,
                raw_vertical: bank.vector(raw, 0)?[2] - root[2],
                raw_constraints: bank.prediction_constraint_words(location.physical)?,
                time,
            })
        },
        s,
        ctx,
        constraints,
        current,
        last.heading,
        input.duration,
        initial_total,
        prune_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ordered_test_point() -> OrderedPoint {
        OrderedPoint {
            requested: RequestedFrame {
                position: [0.; 4],
                velocity: [1., 0., 0., 0.],
                auxiliary40: [0.; 4],
            },
            candidate: PredictionFrame {
                position: [0.; 4],
                velocity: [0.; 4],
                auxiliary40: [0.; 4],
                relative_yaw: 0.,
                valid: true,
                flags140: 0,
            },
            requested_heading: 0.,
            raw_vertical: 0.,
            raw_constraints: [0, 0],
            time: 1.,
        }
    }
    fn ordered_test_context() -> (PredictionContext, ConstraintContext) {
        (
            PredictionContext {
                flag90: false,
                flag91: false,
                flag96: false,
                stage38: 0,
                mode30: 4,
                plane10: [0.; 4],
                point20: [0.; 4],
                current50: [0.; 4],
            },
            ConstraintContext {
                enable_dc: 0,
                mode30: 4,
                submode34: 0,
                stage38: 0,
                p_count60: 0,
                group_has_class_fb5e7753: false,
            },
        )
    }
    #[test]
    fn bank_bridge_does_not_read_later_malformed_raw_record_after_pruning() {
        use crate::amo_samples::SAMPLE_STRIDE;
        let mut bytes = vec![0u8; SAMPLE_STRIDE * 3];
        bytes[0x140] = 0x10;
        bytes[SAMPLE_STRIDE * 2..SAMPLE_STRIDE * 2 + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        let bank = SampleBank::new(&bytes).unwrap();
        let point = ordered_test_point();
        let requested = [
            RequestedPredictionPoint {
                frame: point.requested.clone(),
                heading: 0.,
                offset_time: 0.,
            },
            RequestedPredictionPoint {
                frame: point.requested,
                heading: 0.,
                offset_time: 2.,
            },
        ];
        let input = BankPredictionInput {
            index: 0,
            fraction: 0.,
            base_time: 0.,
            dt: 1.,
            removed: 0,
            scale: 1.,
            duration: 2.,
        };
        let (ctx, cc) = ordered_test_context();
        let settings = GateSettings { values: [0.; 11] };
        let out = bank_prediction_cost(
            &bank,
            &[0, 0, 0],
            input,
            &requested,
            settings,
            ctx,
            cc,
            7.,
            Some(1007.),
        )
        .unwrap();
        assert_eq!(out.total, 1000.);
        assert_eq!(out.stopped_at, Some([0, 0]));
        assert!(
            bank_prediction_cost(
                &bank,
                &[0, 0, 0],
                input,
                &requested,
                settings,
                ctx,
                cc,
                7.,
                None
            )
            .is_err()
        );
    }
    #[test]
    fn ordered_pruning_returns_increment_before_evaluating_later_point() {
        let (ctx, cc) = ordered_test_context();
        let s = GateSettings { values: [0.; 11] };
        let mut first = ordered_test_point();
        first.candidate.valid = false;
        let mut later = ordered_test_point();
        later.candidate.position[0] = f32::NAN;
        let points = [first, later];
        let out =
            ordered_prediction_cost(&points, s, ctx, cc, [0.; 4], 0., 1., 7., Some(1007.)).unwrap();
        assert_eq!(out.total, 1000.);
        assert_eq!(out.stopped_at, Some([0, 0]));
        assert!(out.pruned);
        assert!(ordered_prediction_cost(&points, s, ctx, cc, [0.; 4], 0., 1., 7., None).is_err());
    }
    #[test]
    fn final_stop84_has_no_post_add_pruning_checkpoint() {
        let (ctx, cc) = ordered_test_context();
        let mut s = GateSettings { values: [0.; 11] };
        s.values[10] = 11.;
        let out = ordered_prediction_cost(
            &[ordered_test_point()],
            s,
            ctx,
            cc,
            [0.; 4],
            0.,
            1.,
            7.,
            Some(10.),
        )
        .unwrap();
        assert_eq!(out.total, 11.);
        assert_eq!(out.terms[12], 11.);
        assert!(!out.pruned);
        assert_eq!(out.stopped_at, None);
    }
    #[test]
    fn clip_end_accepts_equality_and_keeps_map_and_flag_checks_independent() {
        let end = 1.0_f32;
        assert!(future_is_valid(end, end, true, 0).unwrap());
        assert!(future_is_valid(f32::from_bits(end.to_bits() - 1), end, true, 0).unwrap());
        assert!(!future_is_valid(f32::from_bits(end.to_bits() + 1), end, true, 0).unwrap());
        assert!(!future_is_valid(end, end, false, 0).unwrap());
        assert!(!future_is_valid(end, end, true, 0x10).unwrap());
        assert!(future_is_valid(end, end, true, 4).unwrap());
        assert!(future_is_valid(f32::NAN, end, true, 0).is_err());
    }
    #[test]
    fn gates_use_four_dimensional_speed_but_planar_velocity_dot() {
        let s = GateSettings {
            values: [2., 3., 4., 5., 6., 7., 8., 9., 10., 1.5, 11.],
        };
        let c = PredictionContext {
            flag90: false,
            flag91: false,
            flag96: false,
            stage38: 0,
            mode30: 1,
            plane10: [0.; 4],
            point20: [0.; 4],
            current50: [1., 0., 0., 0.],
        };
        let w = resolve_weights(s, c, [0., 0., 1., 0.]).unwrap();
        assert_eq!(w.geometry.values, [2., 3., 0., 7.]);
        let special = resolve_weights(
            s,
            PredictionContext {
                flag90: true,
                flag96: true,
                ..c
            },
            [1., 0., 0., 0.],
        )
        .unwrap();
        assert_eq!(special.geometry.values, [3., 0., 0., 7.]);
        assert_eq!(special.radial, 0.);
        let mut huge = s;
        huge.values[0] = f32::MAX;
        huge.values[9] = 2.;
        assert!(
            resolve_weights(
                huge,
                PredictionContext { flag90: true, ..c },
                [1., 0., 0., 0.]
            )
            .is_err()
        );
    }
    #[test]
    fn assembly_only_stop_term_has_strict_candidate_speed_boundary() {
        let s = GateSettings {
            values: [0., 0., 0., 0., 0., 0., 0., 0., 0., 1., 11.],
        };
        let c = PredictionContext {
            flag90: false,
            flag91: false,
            flag96: true,
            stage38: 0,
            mode30: 4,
            plane10: [0.; 4],
            point20: [0.; 4],
            current50: [0.; 4],
        };
        let mut x = PenaltyInput {
            candidate_position: [0.; 4],
            candidate_velocity: [1., 0., 0., 0.],
            current_candidate_velocity: [0.; 4],
            desired_position: [0.; 4],
            desired_velocity: [0.; 4],
            last_position: [0.; 4],
            last_velocity: [1., 0., 0., 0.],
            desired_heading: 0.,
            raw_vertical: 0.,
            position_error_squared: 0.,
            heading_error: 0.,
            flags140: 0,
            is_last: true,
            horizon_count: 1,
        };
        assert_eq!(prediction_penalties(s, c, x).unwrap()[7], 0.);
        x.candidate_velocity[0] = 0.999;
        assert_eq!(prediction_penalties(s, c, x).unwrap()[7], 11.); // flag96 does NOT gate this term.
        assert_eq!(
            prediction_penalties(
                s,
                PredictionContext {
                    plane10: [0.001, 0., 0., 0.],
                    ..c
                },
                x
            )
            .unwrap()[7],
            0.
        );
    }
    #[test]
    fn last_heading_multiplies_by_five_and_requested_w_is_not_discarded() {
        let requested = vec![
            RequestedFrame {
                position: [0., 0., 0., 1.],
                velocity: [0.; 4],
                auxiliary40: [0.; 4]
            };
            2
        ];
        let candidate = vec![
            PredictionFrame {
                position: [0.; 4],
                velocity: [0.; 4],
                auxiliary40: [0.; 4],
                relative_yaw: 1.,
                valid: true,
                flags140: 0
            };
            2
        ];
        let cost = prediction_geometry_cost(
            &requested,
            &candidate,
            0.,
            PredictionWeights {
                values: [2., 0., 0., 4.],
                heading_deadzone: 0.,
            },
            0.,
            None,
        )
        .unwrap();
        assert_eq!(cost.terms, [0., 2., 0., 0., 12.]);
        assert_eq!(cost.total, 14.);
    }
    #[test]
    fn invalid_frame_penalty_is_not_averaged_and_equal_bound_prunes() {
        let requested = vec![
            RequestedFrame {
                position: [0.; 4],
                velocity: [0.; 4],
                auxiliary40: [0.; 4]
            };
            2
        ];
        let candidate = vec![
            PredictionFrame {
                position: [0.; 4],
                velocity: [0.; 4],
                auxiliary40: [0.; 4],
                relative_yaw: 0.,
                valid: false,
                flags140: 0x10
            };
            2
        ];
        let w = PredictionWeights {
            values: [0.; 4],
            heading_deadzone: 0.,
        };
        assert_eq!(
            prediction_geometry_cost(&requested, &candidate, 0., w, 7., None)
                .unwrap()
                .total,
            2000.
        );
        let cost =
            prediction_geometry_cost(&requested, &candidate, 0., w, 7., Some(1007.)).unwrap();
        assert!(cost.pruned);
        assert_eq!(cost.total, 1000.);
    }
}
