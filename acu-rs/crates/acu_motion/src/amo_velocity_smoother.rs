//! Velocity smoother RVA 0xcec2c0 and its critically damped spring 0x522a60,
//! called by the movement step 0xceac44 when request +4c > 0. Image SHA-256
//! 7ca57147...6916. Verified against work/amo_movement_step_native_emu.py.

/// .data tuning read by 0xcec2c0 (image values in parentheses). The section is
/// writable; runtime values are UNVERIFIED, so every field is an input.
#[derive(Clone, Copy, Debug)]
pub struct SmootherTuning {
    pub damp_enabled: bool,     // byte 41ef898 (1)
    pub damp_decay: f32,        // 41ef89c (1.0)
    pub damp_min_speed: f32,    // 41ef8a0 (5.0)
    pub damp_gain: f32,         // 41ef8a4 (0.3)
    pub damp_full: f32,         // 41ef8a8 (1.0)
    pub damp_scale: f32,        // 41ef8ac (0.8)
    pub carry_enabled: bool,    // byte 41ef8b0 (1)
    pub carry_speed: f32,       // 41ef8b4 (10.0)
    pub cap_enabled: bool,      // byte 41ef8b8 (1)
    pub cap_speed: f32,         // 41ef8bc (15.0)
    pub cap_ramp_enabled: bool, // byte 41ef8c0 (1)
    pub cap_ramp_lo: f32,       // 41ef8c4 (7.0)
    pub cap_ramp_hi: f32,       // 41ef8c8 (12.0)
    pub cap_ramp_gain: f32,     // 41ef8cc (10.0)
    pub damp_zero: f32,         // 478d574 (0.0 in the image; runtime-initialised)
}

impl SmootherTuning {
    /// The `.data` values of the identified image (field comments above). Runtime overrides of the
    /// writable section are not traced (UNVERIFIED).
    pub const IMAGE: Self = Self {
        damp_enabled: true,
        damp_decay: 1.0,
        damp_min_speed: 5.0,
        damp_gain: 0.3,
        damp_full: 1.0,
        damp_scale: 0.8,
        carry_enabled: true,
        carry_speed: 10.0,
        cap_enabled: true,
        cap_speed: 15.0,
        cap_ramp_enabled: true,
        cap_ramp_lo: 7.0,
        cap_ramp_hi: 12.0,
        cap_ramp_gain: 10.0,
        damp_zero: 0.0,
    };
}

type V4 = [f32; 4];

fn lanes(p: V4) -> f32 {
    (p[1] + p[3]) + (p[0] + p[2])
}

fn mul(a: V4, b: V4) -> V4 {
    std::array::from_fn(|i| a[i] * b[i])
}

fn splat(v: f32) -> V4 {
    [v; 4]
}

/// minss 1, maxss 0: clamp with the unordered operand passed through.
fn clamp01(x: f32) -> f32 {
    let x = if 0.0 > x { 0.0 } else { x };
    if 1.0 < x { 1.0 } else { x }
}

/// Inverse-length by rsqrt-free reciprocal sqrt plus two Newton steps, applied
/// to `v` (shared shape of the inline normalisers in 0xceac44 / 0xcec2c0).
pub(crate) fn newton_scale(n: f32, v: V4) -> V4 {
    let r = 1.0 / n.sqrt();
    let y1 = (3.0 - (r * n) * r) * (r * 0.5);
    let y2 = (3.0 - (y1 * n) * y1) * (y1 * 0.5);
    std::array::from_fn(|i| y2 * v[i])
}

/// Ratio ramp shared by 0xcec2c0's damping and cap blocks.
fn ramp(x: f32, lo: f32, hi: f32) -> f32 {
    if lo == hi {
        return 0.0;
    }
    let r = (x - lo) / (hi - lo);
    if 0.0 > r {
        0.0
    } else if r.partial_cmp(&1.0) == Some(std::cmp::Ordering::Greater) {
        1.0
    } else {
        r
    }
}

/// RVA 0x522a60: critically damped step of (value, rate) toward `target`.
pub fn smooth_cd(value: &mut V4, rate: &mut V4, target: V4, k: f32, dt: f32) {
    let all_small = |v: V4| v.iter().all(|x| x.abs() <= 0.0005);
    if all_small(std::array::from_fn(|i| target[i] - value[i])) && all_small(*rate) {
        *value = target;
        *rate = mul(splat(0.0), *rate);
        return;
    }
    if 0.0005 >= dt.abs() {
        return;
    }
    let change: V4 = std::array::from_fn(|i| value[i] - target[i]);
    let k2 = k * k;
    let x = k2 * dt;
    let temp: V4 = std::array::from_fn(|i| (k2 * change[i] + rate[i]) * dt);
    let d = ((0.48 * x) * x + (x + 1.0)) + ((0.235 * x) * x) * x;
    let e = 1.0 / d;
    *rate = std::array::from_fn(|i| (rate[i] - k2 * temp[i]) * e);
    *value = std::array::from_fn(|i| (temp[i] + change[i]) * e + target[i]);
}

/// Smoother state (arg8 in, arg9 out of 0xceac44): +0 value xy, +8 rate xy,
/// +10 previous target xy, +18 damping scalar, +1c untouched here.
pub type SmootherState = [f32; 8];

/// RVA 0xcec2c0. `g330` is the runtime pair at 0x45ce330 (zero in the image).
pub fn smooth_velocity(
    s: &mut SmootherState,
    target: V4,
    k: f32,
    dt: f32,
    t: &SmootherTuning,
    g330: [f32; 2],
) {
    if 0.0001 > dt {
        return;
    }
    let mut cur: V4 = [s[0], s[1], 0.0, 0.0];
    let mut rate: V4 = [s[2], s[3], 0.0, 0.0];
    let mut aim = target;
    let tsq = lanes(mul(target, target));
    let ssq = lanes(mul(cur, cur));
    // Native: !(10000 > x) && !(x > -10000), which holds only for NaN.
    if s[4].is_nan() {
        s[5] = g330[1];
        s[4] = g330[0];
    }
    if t.damp_enabled {
        s[6] = clamp01(s[6] - t.damp_decay * dt);
        if tsq > t.damp_min_speed * t.damp_min_speed {
            let dy = target[1] - s[5];
            let dx = target[0] - s[4];
            let dist = (dy * dy + dx * dx).sqrt();
            s[6] = clamp01(t.damp_gain * (dist / dt) * dt + s[6]);
            let w = ramp(s[6], t.damp_zero, t.damp_full);
            let k = (t.damp_scale - 1.0) * w + 1.0;
            aim = mul(target, splat(k));
        }
    }
    s[4] = target[0];
    s[5] = target[1];
    if t.carry_enabled {
        let c2 = t.carry_speed * t.carry_speed;
        if ssq > c2 && c2 > tsq {
            let n = cur[1] * cur[1] + cur[0] * cur[0];
            cur = mul(newton_scale(n, cur), splat(t.carry_speed));
            if cfg!(not(feature = "negative-smoother-carry-keeps-rate")) {
                rate = [0.0; 4];
            }
        }
    }
    smooth_cd(&mut cur, &mut rate, aim, k, dt);
    if t.cap_enabled && cfg!(not(feature = "negative-smoother-cap-off")) {
        let mut cap = t.cap_speed;
        if t.cap_ramp_enabled {
            let mx = if ssq > tsq { ssq } else { tsq };
            let lo = t.cap_ramp_lo * t.cap_ramp_lo;
            if mx > lo {
                cap = t.cap_ramp_gain * ramp(mx, lo, t.cap_ramp_hi * t.cap_ramp_hi) + t.cap_speed;
            }
        }
        let rsq = rate[1] * rate[1] + rate[0] * rate[0];
        if rsq > cap * cap {
            rate = mul(newton_scale(rsq, rate), splat(cap));
            cur = std::array::from_fn(|i| dt * rate[i] + [s[0], s[1], 0.0, 0.0][i]);
        }
    }
    let tiny = |v: V4| 0.010000001 > lanes(mul(v, v));
    if tiny(aim) && tiny(cur) && tiny(rate) {
        cur = [0.0; 4];
        rate = [0.0; 4];
    }
    s[0] = cur[0];
    s[1] = cur[1];
    s[2] = rate[0];
    s[3] = rate[1];
}
