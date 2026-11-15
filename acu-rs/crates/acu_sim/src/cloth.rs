//! ACU cloth solver port (ACU-RE-0081). Bevy-independent; native-order arithmetic.
//!
//! The native step (`0x1423d4b60`) runs a per-LOD schedule of opcodes. This module
//! ports the operations one at a time; each documents its native routine.
//! The reciprocal square root defaults to the exact value; the native code uses
//! the `rsqrtps` estimate (feature `hw-rsqrt` here uses this CPU's estimate).

pub mod asset;
pub mod collision;

use glam::{Mat4, Vec3, Vec4};

/// Particle state. `w` of `pos` is the pin flag: negative = pinned to `target`.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub pos: Vec<Vec4>,
    pub prev: Vec<Vec4>,
    /// Pinned/animated target positions (native body `+64`; falls back to `pos`).
    pub target: Vec<Vec4>,
}

/// Integration settings (native settings struct at body `+132`).
#[derive(Clone, Copy, Debug)]
pub struct Integration {
    /// Gravity (body `+164`, record `11.2e`, else `11.05` halved).
    pub gravity: Vec4,
    /// Damping (body `+176`).
    pub damping: f32,
    /// Max speed (body `+184`, default 1000). Zero disables integration.
    pub max_speed: f32,
    /// Extra damping multiplier `1 - lod[20]` when body `+118` is set (native branch).
    pub lod_damping: Option<f32>,
}

const EPS_LEN2: f32 = 1e-14; // 0x1435f4af0 (0x283424dc)

pub(crate) fn rsqrt(n: f32) -> f32 {
    #[cfg(all(feature = "hw-rsqrt", target_arch = "x86_64"))]
    {
        use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
        // SAFETY: SSE is part of the x86_64 baseline; no memory access.
        unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(n))) }
    }
    #[cfg(not(all(feature = "hw-rsqrt", target_arch = "x86_64")))]
    {
        1.0 / n.sqrt()
    }
}

/// `hadd(hadd(v*v & xyz))`: `(x*x + y*y) + (z*z + 0)`.
fn len2_xyz(v: Vec4) -> f32 {
    (v.x * v.x + v.y * v.y) + (v.z * v.z + 0.0)
}

/// Opcode 6, native `0x1423d7720`: Verlet step with damping, gravity and a
/// per-particle displacement cap; pinned particles (`w < 0`) take `target`.
/// `w` of each particle is preserved; `prev` receives the old positions.
///
/// `gravity_scale` is the per-particle map used when body `+124` is set
/// (native path `0x14241dfb0`: `v = keep * (g_i * g * dt^2 + (P - prev))`).
pub fn integrate(s: &mut State, k: &Integration, dt: f32, gravity_scale: Option<&[f32]>) {
    let n = s.pos.len();
    if k.max_speed == 0.0 {
        // memcpy(prev, pos, 16 * count)
        s.prev[..n].copy_from_slice(&s.pos[..n]);
        return;
    }
    let mut keep = 1.0 - k.damping;
    if let Some(f) = k.lod_damping {
        keep *= f;
    }
    let keep = Vec4::splat(keep);
    let g = k.gravity * Vec4::new(1.0, 1.0, 1.0, 0.0);
    let accel = keep * (g * (dt * dt));
    let cap = k.max_speed * dt;
    for i in 0..n {
        let p = s.pos[i];
        let d = match gravity_scale {
            Some(gs) => keep * (g * (dt * dt) * gs[i] + (p - s.prev[i])),
            None => keep * (p - s.prev[i]) + accel,
        };
        let scale = (rsqrt(len2_xyz(d).max(EPS_LEN2)) * cap).min(1.0);
        let moved = scale * d + p;
        let out = if p.w < 0.0 {
            s.target.get(i).copied().unwrap_or(p)
        } else {
            moved
        };
        s.prev[i] = p;
        s.pos[i] = out.truncate().extend(p.w);
    }
}

/// One distance constraint set (sets 0–2; structural, shear, bend on the flap).
#[derive(Clone, Debug, Default)]
pub struct DistanceSet {
    pub pairs: Vec<(u16, u16)>,
    /// Dequantised rest length per pair (`q * scale + offset`).
    pub rest: Vec<f32>,
    /// `rest - threshold < 0` (native SIMD test; equals `q != 0xff` on the data).
    pub active: Vec<bool>,
    /// Pairs below this run in independent groups of four.
    pub simd_count: usize,
    pub k_min: f32,
    pub k_max: f32,
    pub stiffness: f32,
}

const LEN_FLOOR: f32 = 1e-10; // 0x1435f4740 (0x2edbe6ff)

/// SSE `minps(a, b)`: `a < b ? a : b` (returns `b` when either is NaN).
fn sse_min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}
/// SSE `maxps(a, b)`: `a > b ? a : b`.
fn sse_max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// Opcodes 0/1/2, native `0x1423e51f0` (SIMD region) and `0x1423bd180` (tail).
/// The fractional correction `(len - rest) / len` is scaled by `stiffness`,
/// clamped to `[k_min, k_max]`, then applied along `d = b - a`: half to each
/// free end, all of it to the free end when the other is pinned.
/// Native uses `rsqrtps`/`rcpps` estimates in the SIMD region; exact here.
pub fn solve_distance(pos: &mut [Vec4], c: &DistanceSet) {
    let xyz = Vec4::new(1.0, 1.0, 1.0, 0.0);
    let simd = c.simd_count.min(c.pairs.len()) & !7;
    for g in (0..simd).step_by(4) {
        let mut upd = [(0usize, 0usize, Vec4::ZERO, Vec4::ZERO, false, false); 4];
        for (k, u) in upd.iter_mut().enumerate() {
            let j = g + k;
            let (a, b) = (c.pairs[j].0 as usize, c.pairs[j].1 as usize);
            let (pa, pb) = (pos[a], pos[b]);
            let d = pb - pa;
            let r = rsqrt(len2_xyz(d)) * c.rest[j];
            let kf = sse_max(
                sse_min(((1.0 / r) - 1.0) * c.stiffness * r, c.k_max),
                c.k_min,
            );
            let corr = if c.active[j] {
                (d * kf) * xyz
            } else {
                Vec4::ZERO
            };
            *u = (a, b, corr, corr * 0.5, pa.w < 0.0, pb.w < 0.0);
        }
        for &(a, b, corr, half, pin_a, pin_b) in &upd {
            if !pin_a {
                pos[a] += if pin_b { corr } else { half };
            }
            if !pin_b {
                pos[b] -= if pin_a { corr } else { half };
            }
        }
    }
    for j in simd..c.pairs.len() {
        if !c.active[j] {
            continue;
        }
        let (a, b) = (c.pairs[j].0 as usize, c.pairs[j].1 as usize);
        let d = pos[b] - pos[a];
        let len = sse_max(len2_xyz(d).sqrt(), LEN_FLOOR);
        let k = sse_max(
            sse_min(((len - c.rest[j]) / len) * c.stiffness, c.k_max),
            c.k_min,
        );
        let corr = (d * k) * xyz;
        let (free_a, free_b) = (0.0 <= pos[a].w, 0.0 <= pos[b].w);
        match (free_a, free_b) {
            (true, true) => {
                pos[a] += corr * 0.5;
                pos[b] -= corr * 0.5;
            }
            (true, false) => pos[a] += corr,
            (false, true) => pos[b] -= corr,
            (false, false) => {}
        }
    }
}

const LEN_EPS: f32 = f32::from_bits(0x3089_705f); // 0x1435f4350, about 1e-9
const DISABLED_LENGTH: f32 = 1.0e9; // 0x1435f4b00 (0x4e6e6b28)

/// One dequantised per-particle value with its enable test (native
/// `f32(u8) * scale + bias`, enabled when `value - threshold < 0`).
#[derive(Clone, Debug, Default)]
pub struct ParticleValues {
    pub value: Vec<f32>,
    pub enabled: Vec<bool>,
}

impl ParticleValues {
    pub fn from_bytes(bytes: &[u8], scale: f32, bias: f32, threshold: f32) -> Self {
        let value: Vec<f32> = bytes.iter().map(|&b| b as f32 * scale + bias).collect();
        let enabled = value.iter().map(|&v| v - threshold < 0.0).collect();
        Self { value, enabled }
    }
}

fn dot3(a: Vec4, b: Vec4) -> f32 {
    (a.x * b.x + a.y * b.y) + (a.z * b.z + 0.0)
}

/// Set 3, native `0x1423d7f20` (backstop): keep `dot(P - T, N) >= d` by
/// pushing along the target normal. Applies to every particle up to `count`.
pub fn solve_backstop(pos: &mut [Vec4], target: &[Vec4], normal: &[Vec4], d: &ParticleValues) {
    let xyz = Vec4::new(1.0, 1.0, 1.0, 0.0);
    for i in 0..pos.len() {
        let n = normal[i] * xyz;
        let v = d.value[i] - dot3(pos[i] - target[i], n);
        if d.enabled[i] && v >= 0.0 {
            pos[i] += n * v;
        }
    }
}

/// `len_est`: `r = rsqrt(|D|^2 + 1e-9)`, `len = 1/r` (native `rsqrtps`/`rcpps`).
fn len_est(d: Vec4) -> (f32, f32) {
    let r = rsqrt(len2_xyz(d) + LEN_EPS);
    (r, 1.0 / r)
}

/// Set 4, native `0x1423d8f20` (max distance): pull each particle back inside
/// a sphere of radius `rr` around its target. `w` is preserved.
pub fn solve_max_distance(pos: &mut [Vec4], target: &[Vec4], rr: &ParticleValues) {
    for i in 0..pos.len() {
        let d = target[i] - pos[i];
        let (r, len) = len_est(d);
        let e = len - rr.value[i];
        if rr.enabled[i] && e >= 0.0 {
            let w = pos[i].w;
            pos[i] = (pos[i] + d * (r * e)).truncate().extend(w);
        }
    }
}

/// Set 5, native `0x1423da5b0`, 8-wide mode (body `+103 == 1`): one tether per
/// particle (over the capacity) to an anchor particle, one-sided maximum
/// length; only the particle moves. Blocks of 8 read anchors before writing.
pub fn solve_tether(pos: &mut [Vec4], anchors: &[u16], len: &ParticleValues) {
    let n = anchors.len().min(pos.len());
    for base in (0..n).step_by(8) {
        let end = (base + 8).min(n);
        let snap: Vec<Vec4> = (base..end).map(|i| pos[anchors[i] as usize]).collect();
        for (k, i) in (base..end).enumerate() {
            let l = if len.enabled[i] {
                len.value[i]
            } else {
                DISABLED_LENGTH
            };
            let d = snap[k] - pos[i];
            let (r, length) = len_est(d);
            let e = length - l;
            if e >= 0.0 {
                let w = pos[i].w;
                pos[i] = (pos[i] + d * (r * e)).truncate().extend(w);
            }
        }
    }
}

/// Opcode 5 (set 6), native `0x1423d8990` without a map: blend free
/// particles toward their targets by `a` (record `11.05` S+56).
pub fn blend_to_targets(pos: &mut [Vec4], target: &[Vec4], a: f32) {
    let av = Vec4::new(a, a, a, 0.0);
    let keep = Vec4::new(1.0, 1.0, 1.0, 0.0) - av;
    for i in 0..pos.len() {
        if pos[i].w >= 0.0 || pos[i].w.is_nan() {
            pos[i] = target[i] * av + pos[i] * keep + Vec4::new(0.0, 0.0, 0.0, pos[i].w);
        }
    }
}

/// Opcode 8, native `0x1423e4cf0`: unweighted sum of unit face normals per
/// particle, normalised; degenerate faces/sums use +X.
pub fn compute_normals(pos: &[Vec4], tris: &[[u16; 3]], out: &mut Vec<Vec4>) {
    out.clear();
    out.resize(pos.len(), Vec4::ZERO);
    let unit = |v: Vec4| {
        let l2 = len2_xyz(v);
        let v = if l2 > 0.0 { v } else { Vec4::X };
        v * rsqrt(len2_xyz(v))
    };
    for t in tris {
        let (a, b, c) = (pos[t[0] as usize], pos[t[1] as usize], pos[t[2] as usize]);
        let n = unit((b - a).truncate().cross((c - a).truncate()).extend(0.0));
        for &i in t {
            out[i as usize] += n;
        }
    }
    for n in out.iter_mut() {
        *n = unit(*n);
    }
}

/// Opcode 9, native `0x1423d72b0`: aerodynamic term along the normal with the
/// relative velocity `wind*dt - (P - prev)`; `c = (a * 0.5 * dt) / b`. Free
/// particles only.
pub fn apply_drag(s: &mut State, normal: &[Vec4], wind: Vec4, a: f32, b: f32, dt: f32) {
    let xyz = Vec4::new(1.0, 1.0, 1.0, 0.0);
    let c = ((a * 0.5) * dt) / b;
    let w = (wind * xyz) * dt;
    for ((p, prev), &n) in s.pos.iter_mut().zip(&s.prev).zip(normal) {
        if p.w < 0.0 {
            continue;
        }
        let rel = (w - (*p - *prev)) * xyz;
        let dn = dot3(rel, n);
        *p += ((n * dn) * c) * xyz;
    }
}

/// Frame carry, native `0x1423af520` → `0x1423b10d0`. Rows are the object's
/// rotation (row vectors) and `t` its translation. Free particles keep their
/// world position (`P += old_t - new_t`, same for `prev`); pinned ones rotate
/// with the object and lose velocity.
pub fn carry(s: &mut State, old_rows: [Vec4; 3], old_t: Vec4, new_rows: [Vec4; 3], new_t: Vec4) {
    let xyz = Vec4::new(1.0, 1.0, 1.0, 0.0);
    let d = (old_t - new_t) * xyz;
    // M[j] = sum_k old[k][j] * new[k]
    let m: [Vec4; 3] = std::array::from_fn(|j| {
        (new_rows[0] * old_rows[0][j] + new_rows[1] * old_rows[1][j] + new_rows[2] * old_rows[2][j])
            * xyz
    });
    for i in 0..s.pos.len() {
        let p = s.pos[i];
        if p.w < 0.0 {
            let r = m[0] * p.x + m[1] * p.y + m[2] * p.z;
            let q = r.truncate().extend(-1.0);
            s.pos[i] = q;
            s.prev[i] = q;
        } else {
            s.pos[i] += d;
            s.prev[i] += d;
        }
    }
}

/// Everything one cloth LOD needs to step, decoded from its blob.
#[derive(Clone, Debug, Default)]
pub struct Cloth {
    pub state: State,
    /// Capacity-sized normals (native body `+48`, written by opcode 8).
    pub normals: Vec<Vec4>,
    pub integration: Integration,
    pub gravity_scale: Option<Vec<f32>>,
    pub distance: [DistanceSet; 3],
    pub backstop: ParticleValues,
    pub max_distance: ParticleValues,
    pub tether: ParticleValues,
    pub anchors: Vec<u16>,
    pub triangles: Vec<[u16; 3]>,
    pub schedule: Vec<(u32, u32)>,
    pub blend: f32,
    pub drag: (Vec4, f32, f32),
    /// Collision shapes for opcode 7 (swept path), refreshed per step.
    pub shapes: Vec<collision::Shape>,
    /// Swept-path collision inputs; `None` skips opcode 7.
    pub collision: Option<collision::Params>,
    /// Number of opcode-7 executions skipped (no collision configured).
    pub skipped_collision: u64,
}

impl Default for Integration {
    fn default() -> Self {
        Self {
            gravity: Vec4::ZERO,
            damping: 0.0,
            max_speed: 0.0,
            lod_damping: None,
        }
    }
}

/// Opcodes this port executes; anything else is rejected at load.
pub const SUPPORTED_OPCODES: [u32; 13] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13];

impl Cloth {
    /// Check the program uses only ported opcodes and nests repeats correctly.
    pub fn validate(&self) -> Result<(), String> {
        let mut depth = 0i32;
        for &(op, _) in &self.schedule {
            if !SUPPORTED_OPCODES.contains(&op) {
                return Err(format!("cloth opcode {op} is not ported"));
            }
            depth += (op == 10) as i32 - (op == 11) as i32;
            if depth < 0 {
                return Err("cloth schedule closes an unopened repeat".into());
            }
        }
        if depth != 0 {
            return Err("cloth schedule leaves a repeat open".into());
        }
        let n = self.state.pos.len();
        let cap = self.normals.len();
        if self.state.prev.len() != n || self.state.target.len() != n || cap < n {
            return Err("cloth state arrays disagree".into());
        }
        if self.anchors.iter().any(|&a| a as usize >= cap) {
            return Err("cloth tether anchor beyond capacity".into());
        }
        Ok(())
    }

    /// One native step (`0x1423d4b60`): run the program with `dt`.
    /// `target_normals` feeds the backstop (native body `+80`).
    /// Positions are stored padded to the capacity for the capacity-wide ops.
    pub fn step(&mut self, dt: f32, target_normals: &[Vec4]) {
        let mut i = 0usize;
        while i < self.schedule.len() {
            let (op, count) = self.schedule[i];
            let mut next = i;
            for _ in 0..count {
                next = self.exec(i, dt, target_normals);
            }
            if op == 10 && count == 0 {
                next = self.matching_end(i);
            }
            i = next + 1;
        }
    }

    fn matching_end(&self, start: usize) -> usize {
        let mut depth = 0;
        for (j, &(op, _)) in self.schedule.iter().enumerate().skip(start) {
            depth += (op == 10) as i32 - (op == 11) as i32;
            if depth == 0 {
                return j;
            }
        }
        self.schedule.len() - 1
    }

    /// Native interpreter `0x1423d47e0`; returns the index it finished on.
    fn exec(&mut self, i: usize, dt: f32, tn: &[Vec4]) -> usize {
        let n = self.state.pos.len();
        match self.schedule[i].0 {
            s @ 0..=2 => solve_distance(&mut self.state.pos, &self.distance[s as usize]),
            3 => solve_backstop(&mut self.state.pos, &self.state.target, tn, &self.backstop),
            4 => solve_max_distance(&mut self.state.pos, &self.state.target, &self.max_distance),
            5 => blend_to_targets(&mut self.state.pos, &self.state.target, self.blend),
            6 => integrate(
                &mut self.state,
                &self.integration,
                dt,
                self.gravity_scale.as_deref(),
            ),
            7 => match &self.collision {
                Some(p) if !self.shapes.is_empty() => {
                    collision::collide_swept(&mut self.state, &self.shapes, p)
                }
                _ => self.skipped_collision += 1,
            },
            8 => {
                let mut normals = std::mem::take(&mut self.normals);
                let cap = normals.len();
                compute_normals(&self.state.pos, &self.triangles, &mut normals);
                normals.resize(cap, Vec4::X);
                self.normals = normals;
            }
            9 => {
                let (w, a, b) = self.drag;
                let normals = std::mem::take(&mut self.normals);
                apply_drag(&mut self.state, &normals[..n], w, a, b, dt);
                self.normals = normals;
            }
            10 => {
                let mut j = i + 1;
                while j < self.schedule.len() && self.schedule[j].0 != 11 {
                    let (op, count) = self.schedule[j];
                    let mut last = j;
                    for _ in 0..count {
                        last = self.exec(j, dt, tn);
                    }
                    if op == 10 && count == 0 {
                        last = self.matching_end(j);
                    }
                    j = last + 1;
                }
                return j;
            }
            13 => solve_tether_padded(&mut self.state.pos, &self.anchors, &self.tether),
            _ => {}
        }
        i
    }
}

/// The tether op runs over the capacity; padding particles anchor to
/// themselves, so only the first `pos.len()` entries can move here.
fn solve_tether_padded(pos: &mut [Vec4], anchors: &[u16], len: &ParticleValues) {
    let n = pos.len();
    let a: Vec<u16> = anchors[..n].to_vec();
    if a.iter().all(|&x| (x as usize) < n) {
        solve_tether(pos, &a, len);
    } else {
        // An anchor in the padding would read a zero particle natively.
        let mut padded = pos.to_vec();
        padded.resize(anchors.len(), Vec4::ZERO);
        solve_tether(&mut padded, anchors, len);
        pos.copy_from_slice(&padded[..n]);
    }
}

/// Particle skinning, native `0x141d7a4a0`. `k` influences per particle;
/// an influence is skipped when its bone is 0xffff or its weight is at most
/// `1e-5`. The result is divided by the kept weight sum; the normal is not
/// renormalised. With nothing kept, the particle takes `fallback`'s
/// translation and Y axis.
pub fn skin_particles(
    rest: &[Vec3],
    rest_normals: &[Vec3],
    bones: &[u16],
    weights: &[f32],
    k: usize,
    matrices: &[Mat4],
    fallback: Mat4,
) -> (Vec<Vec4>, Vec<Vec4>) {
    const MIN_WEIGHT: f32 = f32::from_bits(0x3727_c5ac); // about 1e-5
    let mut pos = Vec::with_capacity(rest.len());
    let mut nrm = Vec::with_capacity(rest.len());
    for (i, (&p, &n)) in rest.iter().zip(rest_normals).enumerate() {
        let (mut sp, mut sn, mut sw) = (Vec3::ZERO, Vec3::ZERO, 0.0f32);
        for s in i * k..(i + 1) * k {
            let (b, w) = (bones[s], weights[s]);
            if b == 0xffff || w <= MIN_WEIGHT {
                continue;
            }
            let m = matrices[b as usize];
            sp += m.transform_point3(p) * w;
            sn += m.transform_vector3(n) * w;
            sw += w;
        }
        if sw > 0.0 {
            pos.push((sp / sw).extend(0.0));
            nrm.push((sn / sw).extend(0.0));
        } else {
            pos.push(fallback.w_axis.truncate().extend(0.0));
            nrm.push(fallback.y_axis.truncate().extend(0.0));
        }
    }
    (pos, nrm)
}

/// One render vertex's attachment (`SoftBodyVertexMapping`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexMapping {
    pub skinned: bool,
    pub triangle: u16,
    /// `(h, u, v)`.
    pub weights: [f32; 3],
}

/// Bone-driven render vertices: rest positions in mapping order, `k`
/// influences each. Natively summed without dividing by the weight sum.
#[derive(Clone, Debug, Default)]
pub struct SkinnedVertices {
    pub rest: Vec<Vec3>,
    pub bones: Vec<u16>,
    pub weights: Vec<f32>,
    pub k: usize,
}

/// `nz()` in `0x141d70840`: normalise unless `|v|^2 < 1e-6` (`0x358637bd`).
fn nz(v: Vec3) -> Vec3 {
    let l2 = v.length_squared();
    if l2 < f32::from_bits(0x3586_37bd) {
        v
    } else {
        v * rsqrt(l2)
    }
}

/// Render positions, native `0x141d70840`. Cloth-driven vertex:
/// `Q[i0] + u (Q[i1] - Q[i0]) + h TN + v (Q[i2] - Q[i0])` with
/// `TN = nz(cross(nz(Q1 - Q0), nz(Q2 - Q1)))`. Skinned vertices take the
/// next entry of `skinned` (`sum w M p`, or `fallback * p` with no bone).
pub fn render_positions(
    q: &[Vec3],
    triangles: &[[u16; 3]],
    mappings: &[VertexMapping],
    skinned: &SkinnedVertices,
    matrices: &[Mat4],
    fallback: Mat4,
    out: &mut Vec<Vec3>,
) {
    let tn: Vec<Vec3> = triangles
        .iter()
        .map(|t| {
            let (a, b, c) = (q[t[0] as usize], q[t[1] as usize], q[t[2] as usize]);
            nz(nz(b - a).cross(nz(c - b)))
        })
        .collect();
    out.clear();
    let mut s = 0usize;
    for m in mappings {
        if m.skinned {
            let p = skinned.rest[s];
            let mut x = Vec3::ZERO;
            let mut any = false;
            for j in s * skinned.k..(s + 1) * skinned.k {
                let b = skinned.bones[j];
                if b == 0xffff {
                    continue;
                }
                any = true;
                x += matrices[b as usize].transform_point3(p) * skinned.weights[j];
            }
            out.push(if any { x } else { fallback.transform_point3(p) });
            s += 1;
        } else {
            let t = triangles[m.triangle as usize];
            let (a, b, c) = (q[t[0] as usize], q[t[1] as usize], q[t[2] as usize]);
            let [h, u, v] = m.weights;
            out.push(a + (b - a) * u + tn[m.triangle as usize] * h + (c - a) * v);
        }
    }
}

/// Render normals, tangents and bitangents, native `0x141d98430`. Area-weighted
/// face sums over the index buffer remapped to canonical vertices, copied to
/// duplicates, then Gram-Schmidt with the native degenerate fallbacks. The
/// bitangent is returned negated, as the native code stores it.
pub fn surface_frames(
    x: &[Vec3],
    uv: &[[f32; 2]],
    indices: &[u16],
    canonical: &[u16],
    duplicates: &[(u16, u16)],
) -> (Vec<Vec3>, Vec<Vec3>, Vec<Vec3>) {
    let n_v = x.len();
    let (mut nn, mut tt, mut bb) = (
        vec![Vec3::ZERO; n_v],
        vec![Vec3::ZERO; n_v],
        vec![Vec3::ZERO; n_v],
    );
    for tri in indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| canonical[i as usize] as usize);
        let (e1, e2) = (x[b] - x[a], x[c] - x[a]);
        let (du1, dv1) = (uv[b][0] - uv[a][0], uv[b][1] - uv[a][1]);
        let (du2, dv2) = (uv[c][0] - uv[a][0], uv[c][1] - uv[a][1]);
        let n = e1.cross(e2);
        let det = dv2 * du1 - du2 * dv1;
        let r = if det.abs() > 1e-6 { 1.0 / det } else { 1.0 };
        let t = (e1 * dv2 - e2 * dv1) * r;
        let bt = (e2 * du1 - e1 * du2) * r;
        for v in [a, b, c] {
            nn[v] += n;
            tt[v] += t;
            bb[v] += bt;
        }
    }
    for &(o, d) in duplicates {
        let (o, d) = (o as usize, d as usize);
        nn[d] = nn[o];
        tt[d] = tt[o];
        bb[d] = bb[o];
    }
    let tiny = f32::from_bits(0x2000_0000); // 2^-63
    let unit = |v: Vec3, eps: f32| {
        let l2 = v.length_squared();
        if l2 <= eps { v } else { v * rsqrt(l2) }
    };
    for i in 0..n_v {
        let n = unit(nn[i], tiny);
        let t = unit(tt[i], tiny);
        let b = unit(bb[i], tiny);
        let t1 = unit(t - n * t.dot(n), 1e-6);
        let b1 = unit(b - n * b.dot(n) - t1 * b.dot(t1), 1e-6);
        let ok = |v: Vec3| v.length_squared() > 1e-6;
        let (t2, b2) = if !ok(t1) || !ok(b1) {
            if t1.length_squared() > 0.5 {
                (t1, n.cross(t1))
            } else if b1.length_squared() <= 0.5 {
                let axis = if n.dot(Vec3::X) >= n.dot(Vec3::Y) {
                    Vec3::Y
                } else {
                    Vec3::X
                };
                let t = n.cross(axis);
                (t, n.cross(t))
            } else {
                (b1.cross(n), b1)
            }
        } else if t1.dot(b1) > 0.999 {
            (t1, n.cross(t1))
        } else {
            (t1, b1)
        };
        nn[i] = n;
        tt[i] = t2;
        bb[i] = -b2;
    }
    (nn, tt, bb)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(pairs: &[(u16, u16)], rest: f32, simd: usize, lo: f32, hi: f32) -> DistanceSet {
        DistanceSet {
            pairs: pairs.to_vec(),
            rest: vec![rest; pairs.len()],
            active: vec![true; pairs.len()],
            simd_count: simd,
            k_min: lo,
            k_max: hi,
            stiffness: 1.0,
        }
    }

    #[test]
    fn stretched_pair_meets_rest_length() {
        let mut p = vec![Vec4::ZERO, Vec4::new(0.2, 0.0, 0.0, 0.0)];
        solve_distance(&mut p, &set(&[(0, 1)], 0.1, 0, -2.0, 2.0));
        assert!((p[0].x - 0.05).abs() < 1e-6 && (p[1].x - 0.15).abs() < 1e-6);
        // pinned end stays; the free end takes the whole correction
        let mut p = vec![
            Vec4::new(0.0, 0.0, 0.0, -1.0),
            Vec4::new(0.2, 0.0, 0.0, 0.0),
        ];
        solve_distance(&mut p, &set(&[(0, 1)], 0.1, 0, -2.0, 2.0));
        assert_eq!(p[0].x, 0.0);
        assert!((p[1].x - 0.1).abs() < 1e-6);
    }

    #[test]
    fn one_sided_clamps_and_inactive_pairs() {
        // shear-like set (k in [0, 2]) ignores compression
        let mut p = vec![Vec4::ZERO, Vec4::new(0.05, 0.0, 0.0, 0.0)];
        solve_distance(&mut p, &set(&[(0, 1)], 0.1, 0, 0.0, 2.0));
        assert_eq!(p[1].x, 0.05);
        // bend-like set (k in [-2, 0]) ignores stretch
        let mut p = vec![Vec4::ZERO, Vec4::new(0.3, 0.0, 0.0, 0.0)];
        solve_distance(&mut p, &set(&[(0, 1)], 0.1, 0, -2.0, 0.0));
        assert_eq!(p[1].x, 0.3);
        let mut s = set(&[(0, 1)], 0.1, 0, -2.0, 2.0);
        s.active[0] = false;
        let mut p = vec![Vec4::ZERO, Vec4::new(0.3, 0.0, 0.0, 0.0)];
        solve_distance(&mut p, &s);
        assert_eq!(p[1].x, 0.3);
    }

    #[test]
    fn simd_groups_match_sequential_on_disjoint_pairs() {
        // 8 disjoint pairs in the SIMD region == the same pairs as a scalar tail
        let pairs: Vec<(u16, u16)> = (0..8).map(|i| (2 * i, 2 * i + 1)).collect();
        let mut a: Vec<Vec4> = (0..16)
            .map(|i| Vec4::new(i as f32 * 0.13, (i % 3) as f32 * 0.07, 0.0, 0.0))
            .collect();
        let mut b = a.clone();
        solve_distance(&mut a, &set(&pairs, 0.1, 8, -2.0, 2.0));
        solve_distance(&mut b, &set(&pairs, 0.1, 0, -2.0, 2.0));
        for (x, y) in a.iter().zip(&b) {
            assert!((*x - *y).abs().max_element() < 1e-6);
        }
    }

    fn state(p: &[[f32; 4]], q: &[[f32; 4]]) -> State {
        State {
            pos: p.iter().map(|a| Vec4::from_array(*a)).collect(),
            prev: q.iter().map(|a| Vec4::from_array(*a)).collect(),
            target: vec![Vec4::new(9.0, 9.0, 9.0, 0.0); p.len()],
        }
    }
    const K: Integration = Integration {
        gravity: Vec4::new(0.0, 0.0, -10.0, 0.0),
        damping: 0.0,
        max_speed: 1000.0,
        lod_damping: None,
    };

    #[test]
    fn free_fall_from_rest_is_g_dt2() {
        let mut s = state(&[[0.0, 0.0, 1.0, 0.0]], &[[0.0, 0.0, 1.0, 0.0]]);
        integrate(&mut s, &K, 0.1, None);
        assert!((s.pos[0].z - (1.0 - 0.1)).abs() < 1e-6);
        assert_eq!(s.prev[0], Vec4::new(0.0, 0.0, 1.0, 0.0));
        // second step keeps the velocity: z drops by v*dt + g*dt^2
        integrate(&mut s, &K, 0.1, None);
        assert!((s.pos[0].z - (0.9 - 0.1 - 0.1)).abs() < 1e-6);
    }

    #[test]
    fn pinned_snaps_to_target_and_keeps_w() {
        let mut s = state(&[[1.0, 2.0, 3.0, -1.0]], &[[0.0, 0.0, 0.0, -1.0]]);
        integrate(&mut s, &K, 0.016, None);
        assert_eq!(s.pos[0], Vec4::new(9.0, 9.0, 9.0, -1.0));
        assert_eq!(s.prev[0], Vec4::new(1.0, 2.0, 3.0, -1.0));
    }

    #[test]
    fn displacement_is_capped_and_damped() {
        let k = Integration {
            max_speed: 1.0,
            damping: 0.5,
            gravity: Vec4::ZERO,
            ..K
        };
        let mut s = state(&[[10.0, 0.0, 0.0, 0.0]], &[[0.0, 0.0, 0.0, 0.0]]);
        integrate(&mut s, &k, 0.5, None); // d = 5, cap = 0.5
        assert!((s.pos[0].x - 10.5).abs() < 1e-5);
        let k0 = Integration {
            max_speed: 0.0,
            ..K
        };
        let mut s = state(&[[1.0, 0.0, 0.0, 0.0]], &[[0.0, 0.0, 0.0, 0.0]]);
        integrate(&mut s, &k0, 0.5, None);
        assert_eq!(s.pos[0].x, 1.0);
        assert_eq!(s.prev[0].x, 1.0);
    }

    fn vals(v: &[f32]) -> ParticleValues {
        ParticleValues {
            value: v.to_vec(),
            enabled: vec![true; v.len()],
        }
    }

    #[test]
    fn backstop_pushes_out_along_normal_only_when_behind() {
        let t = [Vec4::ZERO, Vec4::ZERO];
        let n = [Vec4::Z, Vec4::Z];
        let mut p = [
            Vec4::new(0.3, 0.0, -0.2, 0.0),
            Vec4::new(0.0, 0.0, 0.5, 0.0),
        ];
        solve_backstop(&mut p, &t, &n, &vals(&[-0.1, -0.1]));
        assert!((p[0].z + 0.1).abs() < 1e-6 && p[0].x == 0.3);
        assert_eq!(p[1].z, 0.5);
    }

    #[test]
    fn max_distance_and_tether_are_one_sided() {
        let t = [Vec4::ZERO];
        let mut p = [Vec4::new(0.0, 2.0, 0.0, 0.0)];
        solve_max_distance(&mut p, &t, &vals(&[0.5]));
        assert!((p[0].y - 0.5).abs() < 1e-4);
        let mut q = [Vec4::new(0.0, 0.2, 0.0, 0.0)];
        solve_max_distance(&mut q, &t, &vals(&[0.5]));
        assert_eq!(q[0].y, 0.2);
        // tether: particle 1 anchored to pinned particle 0, max length 1
        let mut p = [
            Vec4::new(0.0, 0.0, 0.0, -1.0),
            Vec4::new(3.0, 0.0, 0.0, 0.0),
        ];
        solve_tether(&mut p, &[0, 0], &vals(&[0.0, 1.0]));
        assert_eq!(p[0], Vec4::new(0.0, 0.0, 0.0, -1.0));
        assert!((p[1].x - 1.0).abs() < 1e-4 && p[1].w == 0.0);
    }

    #[test]
    fn normals_of_a_flat_quad_point_up() {
        let p = [
            Vec4::ZERO,
            Vec4::new(1.0, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(1.0, 1.0, 0.0, 0.0),
        ];
        let mut n = Vec::new();
        compute_normals(&p, &[[0, 1, 2], [1, 3, 2]], &mut n);
        for v in &n {
            assert!((*v - Vec4::Z).abs().max_element() < 1e-6);
        }
    }

    #[test]
    fn drag_damps_normal_velocity_of_free_particles() {
        let mut s = State {
            pos: vec![
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(0.0, 0.0, 1.0, -1.0),
            ],
            prev: vec![Vec4::ZERO, Vec4::ZERO],
            target: vec![Vec4::ZERO; 2],
        };
        apply_drag(&mut s, &[Vec4::Z, Vec4::Z], Vec4::ZERO, 1.3, 0.7, 0.1);
        let c = (1.3 * 0.5 * 0.1) / 0.7;
        assert!((s.pos[0].z - (1.0 - c)).abs() < 1e-6);
        assert_eq!(s.pos[1].z, 1.0);
    }

    #[test]
    fn carry_keeps_free_world_position_and_rotates_pinned() {
        let rows = [Vec4::X, Vec4::Y, Vec4::Z];
        let mut s = State {
            pos: vec![
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(1.0, 0.0, 0.0, -1.0),
            ],
            prev: vec![
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(1.0, 0.0, 0.0, -1.0),
            ],
            target: vec![Vec4::ZERO; 2],
        };
        // object moves +2 in x, rotates 90 degrees about z
        let new_rows = [Vec4::Y, -Vec4::X, Vec4::Z];
        carry(
            &mut s,
            rows,
            Vec4::ZERO,
            new_rows,
            Vec4::new(2.0, 0.0, 0.0, 0.0),
        );
        assert_eq!(s.pos[0], Vec4::new(-1.0, 0.0, 0.0, 0.0));
        assert_eq!(s.pos[1], Vec4::new(0.0, 1.0, 0.0, -1.0));
        assert_eq!(s.prev[1], s.pos[1]);
    }

    #[test]
    fn program_repeats_blocks_and_rejects_unported_opcodes() {
        let mut c = Cloth {
            state: State {
                pos: vec![Vec4::new(0.0, 0.0, 1.0, 0.0)],
                prev: vec![Vec4::new(0.0, 0.0, 1.0, 0.0)],
                target: vec![Vec4::ZERO],
            },
            normals: vec![Vec4::Z],
            integration: Integration {
                gravity: Vec4::new(0.0, 0.0, -10.0, 0.0),
                max_speed: 1000.0,
                ..Default::default()
            },
            // integrate twice via a repeat block, then once more
            schedule: vec![(10, 2), (6, 1), (11, 1), (6, 1)],
            ..Default::default()
        };
        assert!(c.validate().is_ok());
        c.step(0.1, &[Vec4::Z]);
        // three integrations from rest: z = 1 - (1+2+3)*0.1
        assert!((c.state.pos[0].z - 0.4).abs() < 1e-5);
        c.schedule = vec![(12, 1)];
        assert!(c.validate().is_err());
        c.schedule = vec![(10, 1), (6, 1)];
        assert!(c.validate().is_err());
    }

    #[test]
    fn particle_skinning_divides_by_kept_weights() {
        let m = [
            Mat4::from_translation(Vec3::X),
            Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2),
        ];
        let fallback = Mat4::from_translation(Vec3::new(0.0, 0.0, 5.0));
        let rest = [Vec3::Y, Vec3::Y, Vec3::Y];
        let normals = [Vec3::Z; 3];
        // Particle 0: bones 0 and 1 at 0.5 each. Particle 1: bone 0 with weight
        // 1e-6 skipped, so only bone 1 counts (weight 0.25, divided out).
        // Particle 2: nothing kept, takes the fallback.
        let bones = [0, 1, 0, 1, 0xffff, 0xffff];
        let weights = [0.5, 0.5, 1e-6, 0.25, 1.0, 1.0];
        let (p, n) = skin_particles(&rest, &normals, &bones, &weights, 2, &m, fallback);
        assert!((p[0].truncate() - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-6);
        assert!((p[1].truncate() - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-6);
        assert_eq!(p[2], Vec4::new(0.0, 0.0, 5.0, 0.0));
        assert_eq!(n[2], Vec4::Y);
        assert!((n[0].truncate() - Vec3::Z).length() < 1e-6);
    }

    #[test]
    fn render_positions_use_barycentrics_and_normal_offset() {
        let q = [Vec3::ZERO, Vec3::X, Vec3::new(1.0, 1.0, 0.0)];
        let tris = [[0, 1, 2]];
        let maps = [
            VertexMapping {
                skinned: false,
                triangle: 0,
                weights: [0.5, 0.25, 0.5],
            },
            VertexMapping {
                skinned: true,
                triangle: 0,
                weights: [0.0; 3],
            },
        ];
        let skinned = SkinnedVertices {
            rest: vec![Vec3::Z],
            bones: vec![0, 0xffff],
            weights: vec![0.5, 0.5],
            k: 2,
        };
        let m = [Mat4::from_translation(Vec3::X)];
        let mut out = Vec::new();
        render_positions(&q, &tris, &maps, &skinned, &m, Mat4::IDENTITY, &mut out);
        // u (Q1 - Q0) = (0.25, 0, 0); v (Q2 - Q0) = (0.5, 0.5, 0); h TN = 0.5 Z.
        assert!((out[0] - Vec3::new(0.75, 0.5, 0.5)).length() < 1e-6);
        // No division by the weight sum: 0.5 * (Z + X).
        assert!((out[1] - Vec3::new(0.5, 0.0, 0.5)).length() < 1e-6);
    }

    #[test]
    fn surface_frames_of_a_flat_quad() {
        // Quad in the XY plane, u along X and v along Y; vertex 3 duplicates 2.
        let x = [
            Vec3::ZERO,
            Vec3::X,
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::Y,
        ];
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [1.0, 1.0], [0.0, 1.0]];
        let indices = [0, 1, 2, 0, 3, 4];
        let canonical = [0, 1, 2, 2, 4];
        let (n, t, b) = surface_frames(&x, &uv, &indices, &canonical, &[(2, 3)]);
        for i in 0..5 {
            assert!((n[i] - Vec3::Z).length() < 1e-6, "n {i}");
            assert!((t[i] - Vec3::X).length() < 1e-6, "t {i}");
            assert!((b[i] + Vec3::Y).length() < 1e-6, "b {i}");
        }
    }
}
