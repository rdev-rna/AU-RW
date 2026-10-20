//! Live locomotion motion matching (reconstructed, approximate — not yet native parity).
//!
//! One decision per sim tick — `{take, time}` — drives both the root (the take's own ROOT0/ROOT1 tracks)
//! and the pose. Foot planting remains a separate visual gate. Uses native kernels (image `7ca57147…6916`):
//! primary pose cost `0x1631374` (`amo::primary_pose_cost`), candidate futures `0x1f36eb0` and prediction
//! geometry `0x1631ba0` (`amo_prediction`), range scan `0x163526c` (`amo_search::range_search`), query
//! sampling `0x1f369b4` (`amo_samples::query_pose`). Generic flow `0x1635658` (`scratch_amo/AMO_REPORT.md`):
//! continuation with the settings `+6c` bonus, then global search, then refinement around the best.
//! The global search covers the current take too (no adapter exclusion); a future past the take's
//! Animation duration costs 1000 (`0x1632154`). Native high-level dispatch parity remains open.
//!
//! UNVERIFIED inputs (native writers not traced, `docs/re/LIVE_ANIMATION_BANK_BINDINGS.md`):
//! - requested trajectory: [`spring_request`] predicts it like dense generator `0x162e818` (controller
//!   spring, `0x162f778` / `0x162bc1c` resampling); its open inputs are listed there. The 3-horizon
//!   endpoints 0.05 / 1.1 s are documented, the middle is not;
//! - requested heading: the stick direction; the auxiliary `+40` term is off (its requested value is unknown);
//! - search cadence every tick, global step 0.2 s, refinement centred on the winning time;
//!   native `0x163526c` seeds its refiner from range start + resolution, a wrapper not yet reproduced;
//! - exclusion context `value880 = 1` (no tag filter), history terms off (decoded settings `+44/+48 = 0`).
//!
//! Frames: sample-local = take position minus root, rotated by `-yaw(+54)`: x right, y forward (OBSERVED on
//! jog samples). Root tracks: x right, y forward, yaw counter-clockwise about +z, i.e. a left turn.
//! The sim's facing also increases toward the fighter's left: `motion::to_world` has forward
//! `(sin f, cos f)` and right `(-cos f, sin f)`, so `d forward/df = -right`. Take yaw therefore adds to
//! facing unchanged; the earlier negation turned the fighter against the take's authored turn (measured
//! trunk roll away from every turn, `docs/checkpoints/LOCO_TURN_CHIRALITY_20261017.md`).

use crate::amo::{PoseWeights, primary_pose_cost, wrap_angle};
use crate::amo_prediction::{
    PredictionWeights, RequestedFrame, candidate_future, future_is_valid, prediction_geometry_cost,
};
use crate::amo_request::{DenseRequest, differentiate, resample};
use crate::amo_samples::{SampleBank, SampleCursor};
use crate::amo_search::{Candidate, ExclusionContext, SearchClip, SearchRange, range_search};
use crate::amo_velocity_smoother::{SmootherState, SmootherTuning, smooth_velocity};
use serde::{Deserialize, Serialize};

/// Requested-trajectory horizons, seconds. UNVERIFIED middle value.
pub const HORIZONS: [f32; 3] = [0.05, 0.5, 1.1];
/// Global search step, seconds (UNVERIFIED; native LOD tables give 0.2–0.5).
pub const GLOBAL_STEP: f32 = 0.2;
/// Refinement half-window, seconds (RVA 0x1635f8b: width 0.3 divided by two).
pub const REFINE_WINDOW: f32 = 0.15;
/// Refinement resolution, seconds (`.rdata` 0x357670c, loaded at 0x1635f53).
pub const REFINE_RESOLUTION: f32 = 0.02;

/// One take of a locomotion group: its pose database and root tracks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Take {
    pub name: String,
    /// objectId of the take's Animation (the SubGroupAnimDesc member). Names are not unique: Orochi ships
    /// two `20_009_FPP_Katana_jog_velaligned_plants90_180_01` objects (`42efb742`, `44b145ac`) with
    /// different motion, so presentation binds the clip by this id. Zero: unknown (synthetic takes).
    #[serde(default)]
    pub object_id: u64,
    /// Raw 0x150-byte samples (`fh_assets::amo_db::PoseDb::samples`).
    pub samples: Vec<u8>,
    /// Packed logical→physical map (P+0x78).
    pub table: Vec<u32>,
    pub dt: f32,
    /// P+0x2c removed-frame count.
    pub removed: u32,
    pub tags58: Vec<i32>,
    pub byte4d: bool,
    /// Root per 60 Hz frame: x, y (m), yaw (rad, counter-clockwise), from tracks 0/1.
    pub root: Vec<[f32; 3]>,
    /// Animation duration (s), `AnimationObject` +0x18 (serializer 0x4d3614). Zero: use the root length.
    #[serde(default)]
    pub duration: f32,
}

/// A locomotion group: its takes and matcher settings record (`fh_assets::welder::SettingsRecord`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocoDb {
    pub takes: Vec<Take>,
    pub settings: Vec<u8>,
}

/// The live decision state (serializable, deterministic).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchState {
    pub take: usize,
    /// Seconds into the take.
    pub time: f32,
    /// Search cadence and recent selections (`0x164c4ac`, `0x162e5b8`).
    #[serde(default)]
    pub schedule: Schedule,
}

/// Search interval at search LOD 4 (`ctx+0x2e0`): `0x164c4ac` indexes {_, 0.5, 0.2, 0.15, 0.08, 0.04}
/// (`0x164c559`) and adds `random * 0.05` only below LOD 3. LOD 4 is assigned at `0x162d632` when owner
/// vfunc `+0x2a0` holds; it also gives the global step [`GLOBAL_STEP`] (`minDt[4]` = 0.2).
/// UNVERIFIED: that this vfunc selects the controlled fighter.
pub const SEARCH_INTERVAL: f32 = 0.08;
/// `0x164c4ac`: no search request while `ctx+0x320` (time since the last new selection) is <= 0.1.
pub const SELECT_HOLD: f32 = 0.1;
/// `0x162e5b8`: earlier selections younger than 0.4 s (`0x353856c`) block a candidate within 0.2 s
/// (`0x3538840`) of their time.
const RECENT_AGE: f32 = 0.4;
const RECENT_WINDOW: f32 = 0.2;
/// `0x162e653`: that check is skipped in the last 0.3 s (`0x3546580`) of the playing take.
const RECENT_END_MARGIN: f32 = 0.3;
const RECENT_LEN: usize = 8;

/// One entry of the selection ring `ctx+0x300` (stride 0x28: clock seconds at `+0`, selection at `+8`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Selected {
    pub take: usize,
    pub time: f32,
    /// Seconds since the selection (native: the millisecond clock minus the entry's `+0`).
    pub age: f32,
}

/// Matcher search cadence (`0x164c4ac`) and selection history (`0x162e5b8`). Native accumulates both
/// timers by the update step `ctx+0x410` at `0x1649887`/`0x164989a`; a new selection zeroes `+0x320`
/// first (`0x16497c4`), a search request zeroes `+0x324` (`0x164c8d2`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    /// `ctx+0x324`: seconds since the last search request.
    pub since_search: f32,
    /// `ctx+0x320`: seconds since the last new selection.
    pub since_select: f32,
    /// Newest first. UNVERIFIED: native ring capacity (`0x43494a0`) is not read; only 0.4 s is used.
    pub recent: [Option<Selected>; RECENT_LEN],
}

impl Default for Schedule {
    /// A state with no history searches on its next tick.
    fn default() -> Self {
        Self {
            since_search: SEARCH_INTERVAL,
            since_select: 1.0,
            recent: [None; RECENT_LEN],
        }
    }
}

impl Schedule {
    fn due(&self) -> bool {
        self.since_search >= SEARCH_INTERVAL && self.since_select > SELECT_HOLD
    }

    fn select(&mut self, s: MatchState) {
        self.since_select = 0.0;
        self.recent.rotate_right(1);
        self.recent[0] = Some(Selected {
            take: s.take,
            time: s.time,
            age: 0.0,
        });
    }

    fn tick(mut self, dt: f32) -> Self {
        self.since_search += dt;
        self.since_select += dt;
        for e in self.recent.iter_mut().flatten() {
            e.age += dt;
        }
        self
    }
}

/// What the character wants this tick, in its own frame (x right, y forward, m/s).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Request {
    /// Target (stick) velocity; also the stop gate's requested speed.
    pub velocity: [f32; 2],
    /// Desired heading relative to the current facing, counter-clockwise radians.
    pub heading: f32,
    /// Requested `(position m, velocity m/s)` at each of [`HORIZONS`], local frame. `None` asks for
    /// constant `velocity` (position `velocity * h`), the pre-spring adapter.
    pub frames: Option<[([f32; 2], [f32; 2]); 3]>,
    /// Matcher flag bytes ctx `+0xa18`, `+0xa19`, `+0xa1a`, the inputs of [`objective_enables`]
    /// (built by `0x164b300` and the actor fill `0x1362130`). The controller drive sets `a1a` bit 0
    /// through [`steady_move`]; the other producers are UNVERIFIED and left clear.
    pub flags: [u8; 3],
}

/// Inputs of the objective enables `0x162fd68` (called from the matcher `0x162b10c` at `0x162b716`)
/// besides the flag bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnableInputs {
    /// Flag bytes ctx `+0xa18`, `+0xa19`, `+0xa1a`.
    pub flags: [u8; 3],
    /// ctx `+0x80` is zero.
    pub ctx80_clear: bool,
    /// The argument `[rbp+0xc0]` (`xmm8`), stored to ctx `+0x8c0`. UNVERIFIED meaning.
    pub arg_c0: f32,
    /// Matcher settings byte `+0xde` (`settings(ctx+0x2c0)`).
    pub settings_de: u8,
}

/// Scalar boundary of the steady-move section `0x164bbfd..0x164be45` of `0x164b300`.
/// Heading wrapping and rotated/normalized direction construction are explicit caller inputs;
/// this does not identify their runtime producers or replace the remaining flag sections.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SteadyInputs {
    pub flags: [u8; 3],
    /// Length of ctx `+0x50` (xmm9, produced earlier in `0x164b300`).
    pub requested_speed: f32,
    /// Wrapped ctx `+0x40` minus heading of ctx `+0x7c0` (`0x164bc49`).
    pub wrapped_heading_error: f32,
    /// Dot after query/actor frame rotations and normalizations (`0x164bd85..bda6`).
    pub direction_dot: f32,
    /// XY length of ctx `+0x140` times actor `+0x10c` (`0x164bd99..bdc5`).
    pub scaled_query_speed: f32,
    /// ctx `+0xc8`, inferred support-normal Z.
    pub support_normal_z: f32,
    /// XY of ctx `+0x50` minus ctx `+0x20` (`0x164be21..be25`).
    pub velocity_error_xy: [f32; 2],
}

/// Returns a1a after the steady-move section: bits 1..3 clear, bit 0 recomputed, others preserved.
/// Does not apply subsequent stop prediction, which can set bits 1..3 again.
pub fn steady_flags(i: SteadyInputs) -> Result<u8, &'static str> {
    if ![
        i.requested_speed,
        i.wrapped_heading_error,
        i.direction_dot,
        i.scaled_query_speed,
        i.support_normal_z,
        i.velocity_error_xy[0],
        i.velocity_error_xy[1],
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err("non-finite steady-move input");
    }
    // Constants: 0.3 at 0x3546580; 0.5 at 0x3538844; dot at 0x36cc864;
    // speed factors at 0x36cc87c / 0x353856c; normal Z at 0x3696dbc.
    let rejected = i.flags[0] & 4 != 0
        || i.requested_speed < 0.3
        || i.wrapped_heading_error.abs() > 0.5
        || i.direction_dot < 0.766_044_44
        || i.requested_speed > i.scaled_query_speed * 1.65
        || i.requested_speed < i.scaled_query_speed * 0.4
        || i.support_normal_z < 0.85;
    let [x, y] = i.velocity_error_xy;
    // SSE mulps + shufps 0x55 + addps, then comiss against 0x3546578 (0.25).
    Ok((i.flags[2] & 0xf0) | u8::from(!rejected && x * x + y * y <= 0.25))
}

/// a1a bit 0 for the next update, from the live free-roam inputs, through [`steady_flags`].
/// `requested` is ctx `+0x50` and `actual` ctx `+0x20` (`0x387be4`), both in the actor frame (x right,
/// y forward, m/s). The query is the retained one: the playing take at `state.time` (`0x162b425` /
/// `0x162b7ea` refresh it after the flag producer, so it is the previous update's selection).
/// Its local root velocity (`+0x10` rotated by `-yaw(+0x54)`, `0x164bd20`) is dotted with `actual`,
/// both normalised. No state or no valid query clears the bit: the fallback query `0x163114c` copies
/// `0x45cb110`, which is not established here.
/// UNVERIFIED: ctx `+0x50` as the controller target (upstream writer of ctx `+0x70` not traced); the
/// heading error as 0 (the controller turns the actor, `free_roam::step`); actor `+0x10c` as 1.0;
/// normal Z 1.0 (flat ground); `a18` clear. The stop prediction (`a1a` bits 1..3) is not ported.
pub fn steady_move(
    db: &LocoDb,
    state: Option<MatchState>,
    requested: [f32; 2],
    actual: [f32; 2],
) -> Result<u8, &'static str> {
    let Some(s) = state else { return Ok(0) };
    let clips = db.clips()?;
    let c = clips.get(s.take).ok_or("invalid locomotion state")?;
    let Some(q) = c.bank.query_pose(s.time, c.dt, c.table)? else {
        return Ok(0);
    };
    let [qx, qy, _] = q.root_velocity;
    let (query_speed, actual_speed) = (qx.hypot(qy), actual[0].hypot(actual[1]));
    if query_speed == 0.0 || actual_speed == 0.0 {
        return Ok(0);
    }
    steady_flags(SteadyInputs {
        flags: [0; 3],
        requested_speed: requested[0].hypot(requested[1]),
        wrapped_heading_error: 0.0,
        direction_dot: (qx * actual[0] + qy * actual[1]) / (query_speed * actual_speed),
        scaled_query_speed: query_speed,
        support_normal_z: 1.0,
        velocity_error_xy: [requested[0] - actual[0], requested[1] - actual[1]],
    })
}

/// Objective term enables ctx `+0x8e0..+0x8e5`, `0x162fd68`. Only `+0x8e4` (term `0x1632fec`) and
/// `+0x8e5` (pruning `0x1632c38`) are consumed by [`step`]; the others are derived for completeness.
pub fn objective_enables(i: EnableInputs) -> [u8; 6] {
    let [a18, a19, a1a] = i.flags;
    let free = a18 & 0x80 == 0 && a18 & 2 == 0 && i.ctx80_clear;
    // 0.7: .data 0x434949c.
    let turn = a18 & 0x68 == 0x68 && free && i.arg_c0.abs() < 0.7;
    [
        u8::from(a18 & 4 != 0 && a18 & 9 == 0),
        (a18 >> 3) & 1,
        u8::from(turn || a18 & 1 != 0 || a19 & 0x80 != 0),
        u8::from(a1a & 0xa == 0xa && free),
        a1a & 1,
        u8::from(i.settings_de != 0 && a19 & 4 == 0),
    ]
}

impl Request {
    /// Move at `speed` m/s toward `rel` radians from the current facing, in the sim's sense
    /// (positive = toward the fighter's left, as facing increases). Sample frame x is right, so a
    /// left request has negative lateral velocity: `velocity[0] == -speed * sin(heading)`.
    /// UNVERIFIED: native request producer (`0x15ab74c`) not traced; constant velocity is ours.
    pub fn toward(speed: f32, rel: f32) -> Self {
        Self {
            velocity: [-speed * rel.sin(), speed * rel.cos()],
            heading: rel,
            frames: None,
            flags: [0; 3],
        }
    }
}

/// Spring stiffness (MovementSettings `+0x88`, copied into request `+0x4c`, the smoother rate of
/// `0xceac44` -> `0xcec2c0`). 2.7 in both pinned free-navigation `Walk` settings
/// (`scripts/navigation_settings.py`: Warden owner `1471420189`, Orochi `1471420110`) and their free
/// idles; the duel idles use 3.0. UNVERIFIED: read from the install per active entry, not per hero.
pub const FREE_ROAM_STIFFNESS: f32 = 2.7;
/// Dense prediction records and span, seconds (`0x162e818`: 8 records over horizon 1.1 plus the
/// constant 0.1 at `0x3538568`, read at `0x162e8e0`). UNVERIFIED: the LOD selection of the count
/// (`ctx+0x2e0`, `ctx+0xa1a & 2`, `ctx+0xa18 & 8`) and any millisecond rounding of the step.
const DENSE_COUNT: usize = 8;
const DENSE_SPAN: f32 = 1.1 + 0.1;

/// Advance the controller spring one tick toward `target` (world m/s), as movement step
/// `0xceac44` does through `0xcec2c0` when request `+0x4c > 0`; the state is the persisted support
/// (`0xcfa6ac/0xcfa6b3`). UNVERIFIED: that controller wrapper `0xcfa574` steps this actor every tick.
pub fn spring_tick(support: &mut SmootherState, target: [f32; 2], dt: f32) {
    smooth_velocity(
        support,
        [target[0], target[1], 0.0, 0.0],
        FREE_ROAM_STIFFNESS,
        dt,
        &SmootherTuning::IMAGE,
        [0.0; 2],
    );
}

/// Requested trajectory predicted from the controller spring, as `0x162e818`: copy the persisted
/// support (`+60..7f`), store each record before stepping, step the spring, then integrate
/// `pos += step * vel` (`0x162eea5..0x162eeb1`). The records are differentiated (`0x162f778`),
/// resampled at [`HORIZONS`] (`0x162bc1c`) and turned into the frame of `facing` (radians, the sim's
/// `motion::to_world` convention); positions are relative to the current one, as `0x162fd68`.
/// `target` is world m/s; `heading` is the desired heading relative to the facing.
/// UNVERIFIED: the collider (`0xceb715`) and settings `+0xe6` angular policy are not applied
/// (open ground, fixed heading); the resample offset setting `+0xa0` is taken as 0; the target is
/// held constant over the prediction (the stick, scaled by the provisional segment speed).
pub fn spring_request(
    support: &SmootherState,
    target: [f32; 2],
    facing: f32,
    heading: f32,
) -> Result<Request, &'static str> {
    let step = DENSE_SPAN / (DENSE_COUNT - 1) as f32;
    let mut s = *support;
    let mut pos = [0.0f32; 2];
    let mut rows = Vec::with_capacity(DENSE_COUNT);
    for i in 0..DENSE_COUNT {
        rows.push(DenseRequest {
            position: [pos[0], pos[1], 0.0, 0.0],
            velocity: [s[0], s[1], 0.0, 0.0],
            auxiliary: [0.0; 4],
            heading,
            time: i as f32 * step,
        });
        spring_tick(&mut s, target, step);
        pos = [pos[0] + step * s[0], pos[1] + step * s[1]];
    }
    let rows = differentiate(&rows)?;
    let (sin, cos) = facing.sin_cos();
    // Inverse of motion::to_world: right (-cos f, sin f), forward (sin f, cos f).
    let local = |v: [f32; 4]| [-cos * v[0] + sin * v[1], sin * v[0] + cos * v[1]];
    let mut frames = [([0.0; 2], [0.0; 2]); 3];
    for (f, h) in frames.iter_mut().zip(HORIZONS) {
        let r = resample(&rows, h, 0.0)?.ok_or("short requested trajectory")?;
        *f = (local(r.position), local(r.velocity));
    }
    Ok(Request {
        velocity: local([target[0], target[1], 0.0, 0.0]),
        heading,
        frames: Some(frames),
        flags: [0; 3],
    })
}

/// Result of one tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    pub state: MatchState,
    /// Root displacement over the tick: right mm, forward mm, yaw millidegrees toward the left (sim facing).
    pub root_delta: (i32, i32, i32),
    pub cost: f32,
    pub switched: bool,
    /// Crossfade seconds the renderer uses for this switch (`LocoDb::switch_blend`); 0 when not switched.
    pub blend: f32,
}

impl LocoDb {
    fn f32(&self, off: usize) -> f32 {
        self.settings
            .get(off..off + 4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .unwrap_or(0.0)
    }
    /// Crossfade seconds for a new matcher selection, chosen as AMO node update `0x1648e04` does
    /// (image sha256 `7ca57147…2316916`). On a new selection (`ctx+0xa19 & 2`) the blend is settings
    /// `+0x94` (`0x16496a1`); with the stop flag `ctx+0xa18 & 4` set, `ctx+0xa19 & 4` clear and
    /// `|ctx+0x140|² < 1.0` (constant `0x35384a0`) it is settings `+0x98` (`0x16496db`). The result is
    /// stored at node `+0x26c` (`0x1649704`) and becomes the transition duration in `0x1f57568`.
    /// UNVERIFIED: `ctx+0x140` is not identified (zero-initialised at `0x164c9e3`, no matcher-side
    /// writer); it is taken as zero. The LOD clamp `min(blend, 0.3)` when `ctx+0x2e0 <= 1`
    /// (`0x3546580`) and the reset case (`a19 & 0x80`, blend 0) are not modelled. The transition curve
    /// (descriptor kind 1, `0x4bda54`) is not decoded; the renderer keeps its linear crossfade.
    pub fn switch_blend(&self, stopping: bool) -> f32 {
        self.f32(if stopping { 0x98 } else { 0x94 }).max(0.0)
    }
    fn pose_weights(&self) -> PoseWeights {
        PoseWeights {
            values: [0x20, 0x28, 0x2c, 0x30, 0x34, 0x38, 0x40].map(|o| self.f32(o)),
        }
    }
    /// `last_velocity` is the final requested frame's velocity (`req+0x80` record `n-1`, `+0x10`).
    fn prediction_weights(&self, last_velocity: [f32; 2]) -> PredictionWeights {
        // Heading weight (0x1631ea7..0x1631edc): +5c, replaced by +60 when the final requested
        // velocity's squared norm exceeds 0.01 (0x361bd4c); requested z/w are zero here.
        let [vx, vy] = last_velocity;
        let heading = if vx * vx + vy * vy > 0.010000001_f32 {
            self.f32(0x60)
        } else {
            self.f32(0x5c)
        };
        PredictionWeights {
            // position +4c, derivative +50, auxiliary +40 off (UNVERIFIED).
            values: [self.f32(0x4c), self.f32(0x50), 0.0, heading],
            heading_deadzone: 0.0,
        }
    }
    fn clips(&self) -> Result<Vec<SearchClip<'_>>, &'static str> {
        self.takes
            .iter()
            .map(|t| {
                let bank = SampleBank::new(&t.samples)?;
                // The native scan stops 0.5 s before the last sample; shorter takes have no range.
                let long_enough =
                    bank.len() >= 2 && bank.sample_time(bank.len() - 1).is_ok_and(|e| e >= 0.5);
                Ok(SearchClip {
                    bank,
                    table: &t.table,
                    dt: t.dt,
                    removed_count: t.removed,
                    eligible: long_enough,
                    tags58: &t.tags58,
                    byte4d: t.byte4d,
                })
            })
            .collect()
    }
}

fn duration(t: &Take) -> f32 {
    if t.duration > 0.0 {
        t.duration
    } else {
        t.root.len().saturating_sub(1) as f32 / 60.0
    }
}

/// Root sample at `time` (linear between 60 Hz frames).
fn root_at(t: &Take, time: f32) -> [f32; 3] {
    let f = (time * 60.0).clamp(0.0, t.root.len().saturating_sub(1) as f32);
    let i = f.floor() as usize;
    let j = (i + 1).min(t.root.len() - 1);
    let a = f - i as f32;
    let (p, q) = (t.root[i], t.root[j]);
    [
        p[0] + (q[0] - p[0]) * a,
        p[1] + (q[1] - p[1]) * a,
        p[2] + wrap_angle(q[2] - p[2]) * a,
    ]
}

/// Root displacement of take `t` from `a` to `b` seconds, in the root frame at `a`.
fn root_delta(t: &Take, a: f32, b: f32) -> (i32, i32, i32) {
    let (p, q) = (root_at(t, a), root_at(t, b));
    let (s, c) = p[2].sin_cos();
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    // Rotate by -yaw: local x right, y forward.
    let (right, fwd) = (c * dx + s * dy, -s * dx + c * dy);
    let yaw_ccw = wrap_angle(q[2] - p[2]);
    (
        (right * 1000.0).round() as i32,
        (fwd * 1000.0).round() as i32,
        (yaw_ccw.to_degrees() * 1000.0).round() as i32,
    )
}

/// Objective of a candidate: primary pose cost against `query` plus prediction cost against `req`.
#[allow(clippy::too_many_arguments)]
fn objective(
    db: &LocoDb,
    clip: &SearchClip<'_>,
    cursor: SampleCursor,
    time: f32,
    query: Option<&crate::amo::LocalPose>,
    req: &Request,
    enables: [u8; 6],
    prune: f32,
    duration: f32,
) -> Result<f32, &'static str> {
    // Native objective prerequisite 0x1632c38 -> marker accessor 0x1f37684:
    // sample +0x140 bit 4 rejects the ENTRY pose, independently of future validity.
    // The accessor uses the nearest physical record; a half fraction selects the next one.
    if cursor.index < 0
        || clip
            .bank
            .sample_flags(cursor.index as usize + usize::from(cursor.fraction >= 0.5))?
            & 0x10
            != 0
    {
        return Ok(f32::MAX);
    }
    let nearest = cursor.index as usize + usize::from(cursor.fraction >= 0.5);
    let flags = clip.bank.sample_flags(nearest)?;
    // Pruning predicate 0x1632c38: with ctx+0x8e5 a flag-6 sample is rejected.
    if enables[5] != 0 && flags & 0x40 != 0 {
        return Ok(f32::MAX);
    }
    let mut total = 0.0;
    if let Some(q) = query {
        let c = clip.bank.candidate_pose(cursor)?;
        let p = primary_pose_cost(q, &c, db.pose_weights(), Some(prune))?;
        if p.pruned {
            return Ok(f32::MAX);
        }
        total += p.total;
    }
    let mut cand = Vec::with_capacity(HORIZONS.len());
    let mut wanted = Vec::with_capacity(HORIZONS.len());
    for (n, h) in HORIZONS.into_iter().enumerate() {
        let mut future = candidate_future(
            &clip.bank,
            cursor,
            time + h,
            clip.dt,
            clip.removed_count,
            clip.table,
            1.0,
        )?;
        // Clip end, RVA 0x1632154..0x16321a4: a future past the Animation duration
        // ([[P+0x18]]+0x18, UNVERIFIED owner link) is invalid and costs 1000 unaveraged.
        future.valid = future_is_valid(time + h, duration, future.valid, future.flags140)?;
        cand.push(future);
        let [vx, vy] = req.velocity;
        let ([px, py], [vx, vy]) = req.frames.map_or(([vx * h, vy * h], [vx, vy]), |f| f[n]);
        wanted.push(RequestedFrame {
            position: [px, py, 0.0, 0.0],
            velocity: [vx, vy, 0.0, 0.0],
            auxiliary40: [0.0; 4],
        });
    }
    let pc = prediction_geometry_cost(
        &wanted,
        &cand,
        req.heading,
        db.prediction_weights(
            wanted
                .last()
                .map_or([0.0; 2], |f| [f.velocity[0], f.velocity[1]]),
        ),
        total,
        Some(prune),
    )?;
    if pc.pruned {
        return Ok(f32::MAX);
    }
    let mut total = total + pc.total;
    // Term 0x1632fec (debug id 0x18), after the prediction: with ctx+0x8e4, settings +0x78
    // unless the sample has flag bit 3 and not marker bit 2.
    if enables[4] != 0 {
        let w = db.f32(0x78);
        total += w - if flags & 4 == 0 && flags & 8 != 0 {
            w
        } else {
            0.0
        };
    }
    Ok(total)
}

/// Interpolating hierarchical refinement, RVA 0x1634de4. Endpoints precede the centre;
/// a bound below 10000 omits the centre. Recurse around the strict best until half
/// the window is smaller than the requested resolution. Invalid cursors are skipped.
fn refine(
    center: f32,
    mut window: f32,
    resolution: f32,
    last: f32,
    mut bound: f32,
    mut score: impl FnMut(f32, f32) -> f32,
) -> Option<(f32, f32)> {
    let mut center = center;
    let mut best = None;
    loop {
        let times = [
            (center - window).max(0.0),
            (center + window).min(last.max(0.0)),
            center,
        ];
        let count = if bound >= 10000.0 { 3 } else { 2 };
        for time in times.into_iter().take(count) {
            let cost = score(time, bound);
            if cost < bound {
                bound = cost;
                best = Some((time, cost));
            }
        }
        window *= 0.5;
        if window < resolution {
            return best;
        }
        if let Some((time, _)) = best {
            center = time;
        }
    }
}

/// Final refinement `0x1635f05..0x1636071` around the global winner `t`: range
/// `[max(0, t - 0.15), t + 0.15]` (`xmm9` zeroed at `0x163576f`), resolution 0.02, bound `cost + 10`
/// (`.rdata` `0x3546624`), through `0x163526c` with `param_5 = 1`. Its interpolating branch
/// (`0x1635555..0x16355ef`) seeds [`refine`] at range start + resolution with half-window
/// `(end - start) * 0.5`, not at `t`; below the 10000 bound the seed itself is never scored.
fn final_refine(
    t: f32,
    last: f32,
    cost: f32,
    score: impl FnMut(f32, f32) -> f32,
) -> Option<(f32, f32)> {
    let (start, end) = ((t - REFINE_WINDOW).max(0.0), t + REFINE_WINDOW);
    refine(
        start + REFINE_RESOLUTION,
        (end - start) * 0.5,
        REFINE_RESOLUTION,
        last,
        cost + 10.0,
        score,
    )
}

/// Advance one sim tick of `dt` seconds: continuation, global search, refinement; returns the decision and
/// the root motion of the played segment.
pub fn step(
    db: &LocoDb,
    state: Option<MatchState>,
    req: &Request,
    dt: f32,
) -> Result<Decision, &'static str> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err("invalid locomotion tick");
    }
    if state.is_some_and(|s| s.take >= db.takes.len() || !s.time.is_finite() || s.time < 0.0)
        || db.takes.iter().any(|t| t.root.is_empty())
    {
        return Err("invalid locomotion state/root tracks");
    }
    let clips = db.clips()?;
    if clips.is_empty() {
        return Err("empty locomotion group");
    }
    // UNVERIFIED: ctx+0x80 and the 0x162fd68 argument are not traced; they gate only +0x8e2/+0x8e3,
    // which no wired term reads.
    let enables = objective_enables(EnableInputs {
        flags: req.flags,
        ctx80_clear: true,
        arg_c0: 0.0,
        settings_de: db.settings.get(0xde).copied().unwrap_or(0),
    });
    let ctx = ExclusionContext {
        value880: 1,
        settings_dc: false,
        // Generic dispatcher 0x16301fc requires this bit clear; 0x1637728 then
        // excludes event-sync-only clips (+4d). Do not admit them as ordinary locomotion.
        flag_a19_bit2: false,
    };
    // Same-take rewind penalty (0x1633228): an earlier time of the current take strictly within the
    // preceding 0.5 s costs 10.0 (exemption helper 0x162aec0 not traced: never exempt here, UNVERIFIED).
    let current = state.map(|s| (s.take, s.time));
    let rewind = |take: usize, time: f32| match current {
        Some((t, now)) if t == take && time < now && time > now - 0.5 => 10.0,
        _ => 0.0,
    };
    let cost_at =
        |take: usize, logical_time: f32, query: Option<&crate::amo::LocalPose>, prune: f32| {
            let c = &clips[take];
            match c.bank.candidate_cursor(logical_time, c.dt, c.table) {
                Ok(Some(cursor)) => Ok::<f32, &'static str>(
                    objective(
                        db,
                        c,
                        cursor,
                        logical_time,
                        query,
                        req,
                        enables,
                        prune,
                        duration(&db.takes[take]),
                    )
                    .unwrap_or(f32::MAX),
                ),
                _ => Ok(f32::MAX),
            }
        };
    // Search cadence (0x164c4ac): between searches the current take plays on.
    let mut schedule = state.map_or_else(Schedule::default, |s| s.schedule);
    if let Some(s) = state {
        // UNVERIFIED stand-in for the forced search of 0x162b10c (`a19 & 0x20` path): search when the
        // take cannot play the whole tick.
        let exhausted = s.time + dt > duration(&db.takes[s.take]);
        if !schedule.due() && !exhausted {
            let s = MatchState {
                schedule: schedule.tick(dt),
                ..s
            };
            return Ok(play(db, state, s, 0.0, dt, req));
        }
    }
    schedule.since_search = 0.0;
    // Query = the current take's sample at the current time (0x162b10c → 0x1f369b4).
    let query = match state {
        Some(s) => {
            let c = &clips[s.take];
            // An invalid query (removed row, unsupported fraction) leaves the primary term out.
            c.bank.query_pose(s.time, c.dt, c.table).ok().flatten()
        }
        None => None,
    };
    // Continuation (0x1635658): the current take at the current time; it plays on when its cost
    // minus the +6c bonus is <= 0. Otherwise the global search runs bounded by the raw cost.
    let mut best: Option<(MatchState, f32)> = None;
    let mut bound = f32::MAX;
    if let Some(s) = state {
        let take = &db.takes[s.take];
        if clips[s.take].eligible
            && !crate::amo_search::clip_excluded(&take.tags58, take.byte4d, ctx)
            && s.time + 0.5 < duration(take)
        {
            if let Ok(cost) = cost_at(s.take, s.time, query.as_ref(), f32::MAX) {
                if cost < f32::MAX {
                    bound = cost;
                    best = Some((s, cost - db.f32(0x6c)));
                }
            }
        }
    }
    if let Some((s, cost)) = best.filter(|b| b.1 <= 0.0) {
        let s = MatchState {
            schedule: schedule.tick(dt),
            ..s
        };
        return Ok(play(db, state, s, cost, dt, req));
    }
    // Global search 0x1636744 -> 0x163526c over the whole group, the current take included,
    // bounded by the continuation cost without the bonus (fStack_820). Re-entering the playing
    // take at its own time is left to acceptance 0x162e5b8 (`accepts`).
    let global = range_search(
        &clips,
        SearchRange {
            first_clip: 0,
            last_clip: clips.len() - 1,
            start_time: 0.0,
            end_time: f32::MAX,
        },
        GLOBAL_STEP,
        ctx,
        bound.min(1.0e30),
        |cand: Candidate, prune| {
            let c = &clips[cand.clip];
            let cursor = SampleCursor {
                index: cand.physical as i32,
                fraction: 0.0,
            };
            // A candidate the kernels reject (e.g. a future past a removed row) is not eligible.
            Ok(objective(
                db,
                c,
                cursor,
                cand.time,
                query.as_ref(),
                req,
                enables,
                prune,
                duration(&db.takes[cand.clip]),
            )
            .map_or(f32::MAX, |v| v + rewind(cand.clip, cand.time)))
        },
    )?;
    if let Some(hit) = global {
        let t = hit.candidate.time;
        let clip = hit.candidate.clip;
        let last = clips[clip].bank.sample_time(clips[clip].bank.len() - 1)? - 0.5;
        let (time, cost) = final_refine(t, last, hit.cost, |time, prune| {
            cost_at(clip, time, query.as_ref(), prune).unwrap_or(f32::MAX) + rewind(clip, time)
        })
        .unwrap_or((t, hit.cost));
        // A global hit (strictly below the bound) replaces the continuation.
        best = Some((
            MatchState {
                take: clip,
                time,
                ..Default::default()
            },
            cost,
        ));
    }
    let (mut s, cost) = best.ok_or("no candidate")?;
    match state {
        Some(current) if !moved(current, s) => s = current,
        Some(current) if !accepts(db, &schedule, current, s) => s = current,
        _ => schedule.select(s),
    }
    s.schedule = schedule.tick(dt);
    Ok(play(db, state, s, cost, dt, req))
}

fn moved(a: MatchState, b: MatchState) -> bool {
    a.take != b.take || (a.time - b.time).abs() > 1e-4
}

/// Switch acceptance `0x162e5b8` for a new selection `cand` while `current` plays. Helper `0x162aec0(a, b, w)`
/// holds for the same group and clip with `w >= |a.t - b.t|`. Rejected:
/// - outside the last 0.3 s of the playing take, a candidate matching (window 0.2) any selection younger than
///   0.4 s except the newest, which is skipped (`0x162e701`);
/// - a candidate matching the playing take within its pose-database step `P+0x20` (`ctx+0x2d0`, `0x162e7a0`).
///
/// UNVERIFIED: the accept-all gates `ctx+0xa18 & 3` and `owner+0x360`, and `ctx+0xa19 & 4` (which skips the
/// history and lets a playing-take match through when `ctx+0xa0 != ctx+0xa4`), are taken as clear.
fn accepts(db: &LocoDb, schedule: &Schedule, current: MatchState, cand: MatchState) -> bool {
    let same = |take: usize, time: f32, window: f32| {
        take == cand.take && window >= (time - cand.time).abs()
    };
    let playing = &db.takes[current.take];
    if current.time <= duration(playing) - RECENT_END_MARGIN {
        for e in schedule.recent.iter().flatten().skip(1) {
            if e.age > RECENT_AGE {
                break;
            }
            if same(e.take, e.time, RECENT_WINDOW) {
                return false;
            }
        }
    }
    !same(current.take, current.time, playing.dt)
}

fn play(
    db: &LocoDb,
    previous: Option<MatchState>,
    s: MatchState,
    cost: f32,
    dt: f32,
    req: &Request,
) -> Decision {
    let take = &db.takes[s.take];
    let switched = previous.is_none_or(|p| moved(p, s));
    // The played segment: from the chosen time over this tick.
    let end = (s.time + dt).min(duration(take));
    let d = root_delta(take, s.time, end);
    let blend = match previous {
        Some(p) if switched => db.switch_blend(stopping(db, p, req)),
        _ => 0.0,
    };
    Decision {
        state: MatchState { time: end, ..s },
        root_delta: d,
        cost,
        switched,
        blend,
    }
}

/// The matcher stop flag `ctx+0xa18 & 4`, set in `0x162b10c` (`0x162b5a3`) when `|ctx+0x20 xy| > 1.5`
/// (`0x3538570`) and the last requested-trajectory point has `|xy| < 1.0` (`0x35384a0`).
/// UNVERIFIED: `ctx+0x20` is read here as the playing take's root speed and the trajectory end as the
/// constant request velocity; the further gates (`owner+0x51d`, `ctx+0xd4`) and the flag's lifetime
/// (it is evaluated per switch here, not latched) are not traced.
fn stopping(db: &LocoDb, p: MatchState, req: &Request) -> bool {
    let take = &db.takes[p.take];
    let a = (p.time - 1.0 / 60.0).max(0.0);
    let b = (a + 1.0 / 60.0).min(duration(take));
    let (x, y, _) = root_delta(take, a, b);
    let current = (x as f32).hypot(y as f32) / 1000.0 / (b - a).max(1e-6);
    current > 1.5 && req.velocity[0].hypot(req.velocity[1]) < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_rejection_is_independent_of_valid_prediction_futures() {
        let mut db = ramp_db(1.0);
        // Current record rejected, every future record clear. Future-only checks miss this.
        db.takes[0].samples[0x140..0x144].copy_from_slice(&0x10u32.to_le_bytes());
        let score = |db: &LocoDb| {
            let clips = db.clips().unwrap();
            objective(
                db,
                &clips[0],
                SampleCursor {
                    index: 0,
                    fraction: 0.0,
                },
                0.0,
                None,
                &Request::default(),
                [0; 6],
                f32::MAX,
                duration(&db.takes[0]),
            )
            .unwrap()
        };
        assert_eq!(score(&db), f32::MAX);
        db.takes[0].samples[0x140..0x144].fill(0);
        assert_eq!(score(&db), 0.0);
    }

    #[test]
    fn objective_enables_follow_0x162fd68() {
        let on = |flags: [u8; 3], settings_de: u8| {
            objective_enables(EnableInputs {
                flags,
                ctx80_clear: true,
                arg_c0: 0.0,
                settings_de,
            })
        };
        assert_eq!(on([0, 0, 0], 0), [0; 6]);
        // +8e5: settings +0xde unless a19 bit 2.
        assert_eq!(on([0, 0, 0], 1), [0, 0, 0, 0, 0, 1]);
        assert_eq!(on([0, 4, 0], 1), [0; 6]);
        // +8e4 = a1a bit 0; +8e3 needs a1a bits 1 and 3 with a18 bits 1 and 7 clear.
        assert_eq!(on([0, 0, 0xb], 0), [0, 0, 0, 1, 1, 0]);
        assert_eq!(on([2, 0, 0xa], 0), [0; 6]);
        // +8e0 = a18 bit 2 without bits 0/3; +8e1 = a18 bit 3; +8e2 by a18 bit 0 or 0x68 turn.
        assert_eq!(on([4, 0, 0], 0), [1, 0, 0, 0, 0, 0]);
        assert_eq!(on([0x6c, 0, 0], 0), [0, 1, 1, 0, 0, 0]);
        assert_eq!(on([1, 0, 0], 0), [0, 0, 1, 0, 0, 0]);
        let mut wide = EnableInputs {
            flags: [0x68, 0, 0],
            ctx80_clear: true,
            arg_c0: 0.7,
            settings_de: 0,
        };
        assert_eq!(objective_enables(wide)[2], 0);
        wide.arg_c0 = -0.69;
        assert_eq!(objective_enables(wide)[2], 1);
    }

    #[test]
    fn steady_move_reads_the_retained_query_against_the_actual_velocity() {
        let mut db = ramp_db(1.0);
        // Query root velocity +0x10 = (0, 4) m/s forward on every sample, no yaw at +0x54.
        for i in 0..31 {
            let at = i * 0x150 + 0x10;
            db.takes[0].samples[at..at + 4].fill(0);
            db.takes[0].samples[at + 4..at + 8].copy_from_slice(&4.0f32.to_le_bytes());
            db.takes[0].samples[i * 0x150 + 0x54..i * 0x150 + 0x58].fill(0);
        }
        let state = Some(MatchState {
            take: 0,
            time: 1.0,
            schedule: Schedule::default(),
        });
        let flag = |state, requested, actual| steady_move(&db, state, requested, actual).unwrap();
        assert_eq!(flag(state, [0.0, 4.0], [0.0, 4.0]), 1);
        // Negative controls: no retained query, sideways actual velocity (dot 0 < 0.766),
        // requested speed below 0.3, and |requested - actual|^2 > 0.25.
        assert_eq!(flag(None, [0.0, 4.0], [0.0, 4.0]), 0);
        assert_eq!(flag(state, [0.0, 4.0], [4.0, 0.0]), 0);
        assert_eq!(flag(state, [0.0, 0.2], [0.0, 0.2]), 0);
        assert_eq!(flag(state, [0.0, 4.0], [0.0, 3.4]), 0);
    }

    #[test]
    fn term_0x18_and_the_flag_6_prune_follow_the_sample_flags() {
        let mut db = ramp_db(1.0);
        db.settings.resize(0xf0, 0);
        db.settings[0x78..0x7c].copy_from_slice(&5.0f32.to_le_bytes());
        let score = |db: &LocoDb, flags: u32, enables: [u8; 6]| {
            let mut db = db.clone();
            db.takes[0].samples[0x140..0x144].copy_from_slice(&flags.to_le_bytes());
            let clips = db.clips().unwrap();
            let cursor = SampleCursor {
                index: 0,
                fraction: 0.0,
            };
            let req = Request::default();
            objective(
                &db,
                &clips[0],
                cursor,
                0.0,
                None,
                &req,
                enables,
                f32::MAX,
                3.0,
            )
            .unwrap()
        };
        let base = score(&db, 0, [0; 6]);
        let term = [0, 0, 0, 0, 1, 0];
        // Settings +0x78 unless loop bit 3 is set without marker bit 2.
        assert_eq!(score(&db, 0, term), base + 5.0);
        assert_eq!(score(&db, 8, term), base);
        assert_eq!(score(&db, 0xc, term), base + 5.0);
        // +8e5 rejects flag-6 samples; without it they score normally.
        assert_eq!(score(&db, 0x40, [0, 0, 0, 0, 0, 1]), f32::MAX);
        assert_eq!(score(&db, 0x40, [0; 6]), base);
    }

    #[test]
    fn final_refinement_seeds_at_the_range_start() {
        // 0x163526c param_5 = 1 seeds 0x1634de4 at start + 0.02 with half-window 0.15; the
        // bound hit + 10 < 10000 skips the seed, so the first pair is (start + 0.02) -/+ 0.15.
        let mut seen = Vec::new();
        let hit = final_refine(1.2, 10.0, 0.0, |time, _| {
            seen.push(time);
            (time - 1.2).abs() * 10.0
        })
        .unwrap();
        let want = [0.92, 1.22, 1.145, 1.295, 1.1825, 1.2575];
        assert_eq!(seen.len(), want.len(), "{seen:?}");
        for (a, b) in seen.iter().zip(want) {
            assert!((a - b).abs() < 1e-4, "{seen:?}");
        }
        assert!((hit.0 - 1.1825).abs() < 1e-4);
        // Negative control: a refine centred on the winner scores t - 0.15 first.
        let mut centred = Vec::new();
        refine(
            1.2,
            REFINE_WINDOW,
            REFINE_RESOLUTION,
            10.0,
            10.0,
            |time, _| {
                centred.push(time);
                0.0
            },
        );
        assert!((centred[0] - 1.05).abs() < 1e-4 && (seen[0] - 1.05).abs() > 0.1);
    }

    #[test]
    fn a_future_past_the_take_end_costs_1000() {
        // 0x1632154: time + horizon > duration clears the future's validity (+1000 each).
        let db = ramp_db(1.0);
        let clips = db.clips().unwrap();
        let score = |time: f32, duration: f32| {
            let cursor = clips[0]
                .bank
                .candidate_cursor(time, clips[0].dt, clips[0].table)
                .unwrap()
                .unwrap();
            let req = Request::default();
            objective(
                &db,
                &clips[0],
                cursor,
                time,
                None,
                &req,
                [0; 6],
                f32::MAX,
                duration,
            )
            .unwrap()
        };
        let end = duration(&db.takes[0]);
        assert_eq!(end, 3.0);
        // 1.9 + 1.1 = 3.0: equality is still valid (COMISS/CMOVA, strictly greater only).
        assert!(score(1.9, end) < 1000.0);
        // 2.0 + 1.1 > 3.0: only the last horizon is invalid.
        let past = score(2.0, end);
        assert!((1000.0..2000.0).contains(&past), "{past}");
        // Negative control: without the clip-end rule (a longer duration) the clamped
        // future at the last sample costs no more than geometry.
        assert!(score(2.0, end + 0.2) < 1000.0);
    }

    /// Spring support settled on `target` (world m/s) from rest, at the 30 Hz sim tick.
    fn settled(target: [f32; 2]) -> SmootherState {
        let mut s = [0., 0., 0., 0., 0., 0., 0., -1.];
        for _ in 0..90 {
            spring_tick(&mut s, target, 1.0 / 30.0);
        }
        s
    }

    /// Requested `(position, velocity)` at horizon `n`, whichever form the request carries.
    fn horizon(req: &Request, n: usize) -> ([f32; 2], [f32; 2]) {
        let [vx, vy] = req.velocity;
        req.frames
            .map_or(([vx * HORIZONS[n], vy * HORIZONS[n]], [vx, vy]), |f| f[n])
    }

    #[test]
    fn a_settled_spring_requests_the_straight_trajectory_in_the_fighter_frame() {
        // Facing 90 deg: forward is world +x (motion::to_world), so world +x is local forward.
        let s = settled([3.4, 0.0]);
        let req = spring_request(&s, [3.4, 0.0], std::f32::consts::FRAC_PI_2, 0.0).unwrap();
        for (n, h) in HORIZONS.into_iter().enumerate() {
            let (p, v) = horizon(&req, n);
            assert!(v[0].abs() < 1e-3 && (v[1] - 3.4).abs() < 1e-3, "{n}: {v:?}");
            assert!((p[1] - 3.4 * h).abs() < 1e-3, "{n}: {p:?}");
        }
        assert!(req.velocity[0].abs() < 1e-5 && (req.velocity[1] - 3.4).abs() < 1e-5);
    }

    #[test]
    fn a_right_angle_stick_turn_keeps_the_near_request_on_the_current_velocity() {
        // Jogging forward (facing 0: world +y), the stick snaps to the fighter's right (world -x).
        // The spring (0x162e818 via 0xcec2c0, k = 2.7) still asks for mostly forward motion at 0.05 s
        // and arrives at the new direction by 1.1 s; the constant request asks for a sideways jump.
        let s = settled([0.0, 3.4]);
        let spring = spring_request(&s, [-3.4, 0.0], 0.0, -std::f32::consts::FRAC_PI_2).unwrap();
        let constant = Request::toward(3.4, -std::f32::consts::FRAC_PI_2);
        let keeps_momentum = |req: &Request| {
            let (_, near) = horizon(req, 0);
            let (_, far) = horizon(req, 2);
            near[1] > 1.5 && far[0] > 3.0 && far[1].abs() < 0.3
        };
        assert!(keeps_momentum(&spring), "{:?}", spring.frames);
        assert!(!keeps_momentum(&constant), "negative control");
        assert!(
            spring.velocity[0] > 3.39,
            "the stop gate still sees the stick target"
        );
    }

    #[test]
    fn an_authored_left_turn_turns_the_fighter_left() {
        // Root on a counter-clockwise circle: heading t, forward (-sin t, cos t), drifting to -x (left).
        let take = Take {
            name: "left_circle".into(),
            object_id: 0,
            samples: Vec::new(),
            table: Vec::new(),
            dt: 1.0 / 30.0,
            removed: 0,
            tags58: Vec::new(),
            byte4d: false,
            duration: 0.0,
            root: (0..=60)
                .map(|i| {
                    let t = i as f32 / 60.0;
                    [t.cos() - 1.0, t.sin(), t]
                })
                .collect(),
        };
        let (right, fwd, yaw) = root_delta(&take, 0.0, 0.5);
        assert!(
            right < 0 && fwd > 0,
            "the take drifts to its left: {right} {fwd}"
        );
        // The fighter must turn toward the side the take carried it: one chirality for both.
        let (dx, _) = crate::motion::to_world(right, fwd, 0);
        let turned = (yaw as f64 / 1000.0).to_radians().sin(); // new forward's world x
        assert!(
            dx.signum() as f64 == turned.signum(),
            "moved {dx} mm in world x but turned toward {turned}"
        );
        // A request toward that side asks for the same authored turn.
        let req = Request::toward(1.0, 0.3);
        assert!(req.heading > 0.0 && req.velocity[0] < 0.0);
    }

    fn ramp_db(bonus: f32) -> LocoDb {
        let mut samples = vec![0; 31 * 0x150];
        for i in 0..31 {
            let time = i as f32 * 0.1;
            for (offset, value) in [(0x10, time), (0x50, time)] {
                samples[i * 0x150 + offset..i * 0x150 + offset + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        let mut settings = vec![0; 0x70];
        settings[0x20..0x24].copy_from_slice(&1000.0f32.to_le_bytes());
        settings[0x6c..0x70].copy_from_slice(&bonus.to_le_bytes());
        LocoDb {
            takes: vec![Take {
                name: "clock regression".into(),
                object_id: 0,
                samples,
                table: vec![],
                dt: 0.1,
                removed: 0,
                tags58: vec![],
                byte4d: false,
                duration: 0.0,
                // Accelerating track distinguishes successive tick intervals.
                root: (0..=180)
                    .map(|i| [0.0, (i as f32 / 60.0).powi(2), 0.0])
                    .collect(),
            }],
            settings,
        }
    }

    #[test]
    fn heading_weight_switches_to_moving_setting() {
        // Warden free-navigation values: +5c = 0.2, +60 = 0 (0x1631ea7..0x1631edc).
        let mut db = ramp_db(1.0);
        db.settings[0x5c..0x60].copy_from_slice(&0.2f32.to_le_bytes());
        db.settings[0x60..0x64].copy_from_slice(&0.0f32.to_le_bytes());
        assert_eq!(db.prediction_weights([0.0, 1.373]).values[3], 0.0);
        assert_eq!(db.prediction_weights([0.05, 0.05]).values[3], 0.2);
        assert_eq!(db.prediction_weights([0.0, 0.0]).values[3], 0.2);
    }

    #[test]
    fn continuation_uses_interpolated_pose_and_consumes_exactly_one_interval() {
        // Negative bonus forces the positive-cost path that formerly advanced twice.
        for bonus in [1.0, -1.0] {
            let db = ramp_db(bonus);
            let previous = MatchState {
                take: 0,
                time: 0.55,
                ..Default::default()
            };
            let dt = 1.0 / 30.0;
            let d = step(&db, Some(previous), &Request::default(), dt).unwrap();
            assert!((d.state.time - (previous.time + dt)).abs() < 1e-5, "{d:?}");
            assert!(
                !d.switched,
                "continuous playback must not trigger a blend: {d:?}"
            );
            assert_eq!(
                d.root_delta,
                root_delta(&db.takes[0], previous.time, previous.time + dt)
            );
        }
    }

    #[test]
    fn equal_cost_phases_do_not_retime_a_viable_playback_clock() {
        let mut db = ramp_db(0.0);
        for sample in db.takes[0].samples.as_chunks_mut::<0x150>().0 {
            sample[0x10..0x20].fill(0); // identical pose/velocity at every phase
        }
        db.settings[0x4c..0x50].copy_from_slice(&1.0f32.to_le_bytes());
        let req = Request {
            velocity: [0.0, 1.0],
            heading: 0.0,
            frames: None,
            flags: [0; 3],
        };
        let mut state = MatchState {
            take: 0,
            time: 0.55,
            ..Default::default()
        };
        for _ in 0..30 {
            let d = step(&db, Some(state), &req, 1.0 / 30.0).unwrap();
            assert!(!d.switched, "equal-cost phase interrupted playback: {d:?}");
            assert!((d.state.time - state.time - 1.0 / 30.0).abs() < 1e-6);
            state = d.state;
        }
    }

    #[test]
    fn playback_continuity_does_not_block_a_better_different_take() {
        let mut db = ramp_db(0.1);
        db.settings[0x20..0x24].fill(0);
        db.settings[0x50..0x54].copy_from_slice(&1.0f32.to_le_bytes());
        let mut better = db.takes[0].clone();
        for sample in better.samples.as_chunks_mut::<0x150>().0 {
            sample[0x10..0x20].fill(0);
            sample[0x14..0x18].copy_from_slice(&2.0f32.to_le_bytes());
        }
        db.takes.push(better);
        let d = step(
            &db,
            Some(MatchState {
                take: 0,
                time: 0.55,
                ..Default::default()
            }),
            &Request {
                velocity: [0.0, 2.0],
                heading: 0.0,
                frames: None,
                flags: [0; 3],
            },
            1.0 / 30.0,
        )
        .unwrap();
        assert_eq!(d.state.take, 1);
        assert!(d.switched);
        // 0x16496a1: an ordinary new selection fades over settings +0x94 (zero in this record).
        assert_eq!(d.blend, 0.0);
    }

    #[test]
    fn a_switch_fades_over_the_settings_selection_blend() {
        let mut db = ramp_db(0.0);
        db.settings.resize(0xf0, 0);
        db.settings[0x94..0x98].copy_from_slice(&0.25f32.to_le_bytes());
        db.settings[0x98..0x9c].copy_from_slice(&0.45f32.to_le_bytes());
        assert_eq!(db.switch_blend(false), 0.25);
        assert_eq!(db.switch_blend(true), 0.45);
        let moving = Request {
            velocity: [0.0, 2.0],
            heading: 0.0,
            frames: None,
            flags: [0; 3],
        };
        // Root y = t², so speed 2t: 3 m/s at 1.5 s, 1.1 m/s at 0.55 s (gate 1.5, 0x3538570).
        let fast = MatchState {
            take: 0,
            time: 1.5,
            ..Default::default()
        };
        let slow = MatchState {
            take: 0,
            time: 0.55,
            ..Default::default()
        };
        assert!(stopping(&db, fast, &Request::default()));
        assert!(!stopping(&db, fast, &moving), "request still moving");
        assert!(!stopping(&db, slow, &Request::default()), "below 1.5 m/s");
        let d = play(&db, Some(slow), fast, 0.0, 1.0 / 30.0, &Request::default());
        assert!(d.switched);
        assert_eq!(d.blend, 0.25);
        let d = play(&db, Some(fast), slow, 0.0, 1.0 / 30.0, &Request::default());
        assert_eq!(d.blend, 0.45, "stop selection uses settings +0x98");
        let d = play(&db, Some(fast), fast, 0.0, 1.0 / 30.0, &Request::default());
        assert!(!d.switched);
        assert_eq!(d.blend, 0.0);
    }

    #[test]
    fn searches_follow_the_native_cadence() {
        let db = ramp_db(1.0);
        let dt = 1.0 / 30.0;
        let at = |schedule| MatchState {
            take: 0,
            time: 0.55,
            schedule,
        };
        // 0x164c4ac: a search needs ctx+0x324 >= 0.08 and ctx+0x320 > 0.1.
        let held = [
            Schedule {
                since_search: 0.07,
                ..Default::default()
            },
            Schedule {
                since_select: 0.1,
                ..Default::default()
            },
        ];
        for schedule in held {
            let d = step(&db, Some(at(schedule)), &Request::default(), dt).unwrap();
            assert_eq!(d.cost, 0.0, "plays on without a search");
            assert_eq!(d.state.schedule.since_search, schedule.since_search + dt);
        }
        let d = step(&db, Some(at(Schedule::default())), &Request::default(), dt).unwrap();
        assert_eq!(
            d.state.schedule.since_search, dt,
            "search request zeroes ctx+0x324"
        );
    }

    #[test]
    fn acceptance_rejects_recent_and_playing_matches() {
        let db = ramp_db(0.0);
        let state = |time| MatchState {
            take: 0,
            time,
            ..Default::default()
        };
        let mut schedule = Schedule::default();
        schedule.select(state(2.0));
        schedule.select(state(0.5));
        schedule = schedule.tick(0.39);
        // 0x162e5b8: older selections within 0.2 s block; the newest entry is skipped.
        assert!(!accepts(&db, &schedule, state(0.5), state(1.85)));
        assert!(accepts(&db, &schedule, state(0.5), state(1.75)));
        assert!(
            accepts(&db, &schedule, state(1.0), state(0.45)),
            "newest skipped"
        );
        // Older than 0.4 s, or within 0.3 s of the playing take's end: no history check.
        assert!(accepts(&db, &schedule.tick(0.02), state(0.5), state(1.85)));
        assert!(accepts(&db, &schedule, state(2.71), state(1.85)));
        // The playing take within its pose-database step P+0x20.
        assert!(!accepts(&db, &schedule, state(0.5), state(0.59)));
        assert!(accepts(&db, &schedule, state(0.5), state(0.62)));
    }

    #[test]
    fn hierarchical_refinement_preserves_endpoint_ties_and_threshold_rules() {
        let mut visited = Vec::new();
        let hit = refine(1.0, 0.04, 0.04, 2.0, 10000.0, |time, _| {
            visited.push(time);
            2.0
        })
        .unwrap();
        assert_eq!(visited, [0.96, 1.04, 1.0]);
        assert_eq!(hit, (0.96, 2.0));
        visited.clear();
        assert!(
            refine(1.0, 0.04, 0.04, 2.0, 2.0, |time, _| {
                visited.push(time);
                2.0
            })
            .is_none()
        );
        assert_eq!(visited, [0.96, 1.04]);
    }

    #[test]
    fn invalid_state_or_tick_returns_an_error() {
        let db = ramp_db(1.0);
        for dt in [0.0, -1.0, f32::NAN] {
            assert!(step(&db, None, &Request::default(), dt).is_err());
        }
        assert!(
            step(
                &db,
                Some(MatchState {
                    take: 99,
                    time: 0.0,
                    ..Default::default()
                }),
                &Request::default(),
                1.0 / 30.0
            )
            .is_err()
        );
    }
}
