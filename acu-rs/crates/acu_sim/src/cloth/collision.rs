//! Opcode 7 (ACU-RE-0081). The router `0x1423d5510` takes the swept path when the
//! settings byte S+192 is set, else the discrete path.
//!
//! **Discrete path** (`0x1423d68d0`, capsules `0x1423d6360` → `0x14241d4b0` →
//! `0x142419cc0` / `0x14241b800`): one push per particle against the shape in its
//! current frame, in world space, with friction on the particle's motion relative to the
//! collider (see [`collide_discrete`]). This is the path the flap runs live.
//!
//! **Swept (continuous) path**: native `0x1423d58a0`.
//!
//! Each shape record carries its current and previous frames. For every
//! candidate particle the path runs in shape-local space, from `prev` in the
//! previous frame to `P` in the current one. The motion is sampled on a
//! 1/32 grid (`0x1424389b0`, recursive bisection while the samples are
//! farther apart than a shape-dependent spacing), and each sample is pushed
//! out of the shape. The push carries forward, so a fast particle cannot
//! cross the shape. The result returns to world space with the current frame.
//! Only `P` is written; `prev` is not.
//!
//! Port differences, recorded in the finding:
//! - The particle BVH (record `11.1e`) is replaced by a per-particle test of
//!   the particle's sweep box against the shape's swept box. The native tree
//!   can emit untouched candidates, which only changes rounding.
//! - `sinf`/`cosf`/`acosf` are Rust's, not the MSVC CRT's; `acosf` is
//!   clamped (the native code is not).
//! - The rotation interpolation uses glam's matrix/quaternion conversions
//!   with the native row-vector layout.

use glam::{Mat3, Quat, Vec3, Vec4};

use super::State;

/// 2^-23 (`0x1435f53d0`).
const EPS23: f32 = f32::from_bits(0x3400_0000);
/// 1e-8 (`0x1435f53e0`).
const EPS_POINT: f32 = f32::from_bits(0x322b_cc77);
/// 1e-4 (`0x14361ea50`): boxes snap this far inside the surface.
const BOX_INSET: f32 = f32::from_bits(0x38d1_b717);
/// 2.5e-6 (`0x1435f5400`): degenerate-distance scale of the discrete capsule push.
const EPS_CAPSULE: f32 = f32::from_bits(0x3627_c5ac);
/// Box spacing factor 0.8 (`0x142e77f54`).
const BOX_SPACING: f32 = f32::from_bits(0x3f4c_cccd);

fn dot(a: Vec3, b: Vec3) -> f32 {
    (a.x * b.x + a.y * b.y) + (a.z * b.z + 0.0)
}

/// A shape frame as rows: axes `x`, `y`, `z` and translation `t`
/// (world = `t + x lx + y ly + z lz`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: Vec3,
    pub y: Vec3,
    pub z: Vec3,
    pub t: Vec3,
}

impl Frame {
    pub const IDENTITY: Self = Self {
        x: Vec3::X,
        y: Vec3::Y,
        z: Vec3::Z,
        t: Vec3::ZERO,
    };

    /// `L(M, w)`: `(X·d, Y·d, Z·d)` with `d = w − T` (orthonormal axes).
    pub fn local(&self, w: Vec3) -> Vec3 {
        let d = w - self.t;
        Vec3::new(dot(self.x, d), dot(self.y, d), dot(self.z, d))
    }

    /// `Z z + ((X x + T) + Y y)`.
    pub fn world(&self, l: Vec3) -> Vec3 {
        self.z * l.z + ((self.x * l.x + self.t) + self.y * l.y)
    }

    /// World box of a local box centred at the origin with half-size `h`.
    fn bounds(&self, h: Vec3) -> (Vec3, Vec3) {
        let e = self.x.abs() * h.x + self.y.abs() * h.y + self.z.abs() * h.z;
        (self.t - e, self.t + e)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeKind {
    /// Type 1: oriented box with half-extents.
    Box(Vec3),
    /// Type 2: sphere at the frame origin.
    Sphere(f32),
    /// Type 3: capsule along local Y, segment `±half_length`.
    Capsule { radius: f32, half_length: f32 },
}

/// One 160-byte collision record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub current: Frame,
    pub previous: Frame,
}

/// Body-side collision inputs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params {
    /// S+48 (body +180).
    pub thickness: f32,
    /// S+196 (body +328).
    pub friction: f32,
    /// Per-particle thickness factor (body +126, table at body +384+80):
    /// `r_i = factor_i · thickness + r`.
    pub thickness_factor: Option<Vec<f32>>,
    /// Per-particle friction replacing `friction` (body +127, body +384+112).
    pub friction_table: Option<Vec<f32>>,
    /// S+192 (body +324): the swept path; otherwise the discrete path.
    pub swept: bool,
}

/// Motion between the previous and current frames (`0x1424382b0`).
struct Interp {
    m0: Frame,
    m1: Frame,
    axis: Vec3,
    angle: f32,
    rel_t: Vec3,
    cache: [Option<Frame>; 33],
}

impl Interp {
    fn new(m0: Frame, m1: Frame) -> Self {
        // Rel[i][j] = C_i · P_j: the current axes in the previous frame.
        let c = [m1.x, m1.y, m1.z];
        let p = [m0.x, m0.y, m0.z];
        let row = |i: usize| Vec3::new(dot(c[i], p[0]), dot(c[i], p[1]), dot(c[i], p[2]));
        // Row-vector matrix -> column-vector matrix: its columns are the rows.
        let q = Quat::from_mat3(&Mat3::from_cols(row(0), row(1), row(2))).normalize();
        let v = Vec3::new(q.x, q.y, q.z);
        let len = v.length();
        let axis = if len.abs() <= 1e-4 { Vec3::X } else { v / len };
        let mut angle = 2.0 * q.w.clamp(-1.0, 1.0).acos();
        if angle > std::f32::consts::PI {
            angle -= 2.0 * std::f32::consts::PI;
        }
        let rel_t = Vec3::new(
            dot(m1.t, m0.x) - dot(m0.t, m0.x),
            dot(m1.t, m0.y) - dot(m0.t, m0.y),
            dot(m1.t, m0.z) - dot(m0.t, m0.z),
        );
        let mut cache = [None; 33];
        cache[0] = Some(m0);
        cache[32] = Some(m1);
        Self {
            m0,
            m1,
            axis,
            angle,
            rel_t,
            cache,
        }
    }

    /// Pose at step `k` of 32 (`0x1424386b0`): rotation by `k/32` of the
    /// angle, translation linear.
    fn pose(&mut self, k: usize) -> Frame {
        if let Some(f) = self.cache[k] {
            return f;
        }
        let t = k as f32 * 0.03125;
        let h = (t * self.angle) * 0.5;
        let q = Quat::from_xyzw(
            self.axis.x * h.sin(),
            self.axis.y * h.sin(),
            self.axis.z * h.sin(),
            h.cos(),
        );
        let r = Mat3::from_quat(q);
        let m0 = self.m0;
        let axis = |i: usize| {
            let w = r.col(i);
            m0.z * w.z + (m0.y * w.y + m0.x * w.x)
        };
        let tt = self.rel_t * t;
        let f = Frame {
            x: axis(0),
            y: axis(1),
            z: axis(2),
            t: ((m0.x * tt.x + m0.t) + m0.y * tt.y) + m0.z * tt.z,
        };
        self.cache[k] = Some(f);
        f
    }
}

struct End {
    w: Vec3,
    s: Vec3,
    k: usize,
}

/// `0x1424384e0`: bisect the step grid while samples are farther apart than
/// the spacing.
fn bisect(ip: &mut Interp, a: &End, b: &End, s2: f32, out: &mut Vec<Vec3>) {
    if b.k - a.k <= 1 {
        return;
    }
    let k = (a.k + b.k) >> 1;
    let w = (a.w + b.w) * 0.5;
    let m = End {
        w,
        s: ip.pose(k).local(w),
        k,
    };
    if s2 < (a.s - m.s).length_squared() {
        bisect(ip, a, &m, s2, out);
    }
    out.push(m.s);
    if s2 < (m.s - b.s).length_squared() {
        bisect(ip, &m, b, s2, out);
    }
}

/// `0x1424389b0`: the start point, then deltas between consecutive samples
/// (2 to 33 entries), in shape-local space.
fn samples(ip: &mut Interp, prev: Vec3, pos: Vec3, spacing: f32, out: &mut Vec<Vec3>) {
    out.clear();
    let a = End {
        w: prev,
        s: ip.m0.local(prev),
        k: 0,
    };
    let b = End {
        w: pos,
        s: ip.m1.local(pos),
        k: 32,
    };
    out.push(a.s);
    let s2 = spacing * spacing;
    if s2 < (a.s - b.s).length_squared() {
        bisect(ip, &a, &b, s2, out);
    }
    out.push(b.s);
    for j in (1..out.len()).rev() {
        let previous = out[j - 1];
        out[j] -= previous;
    }
}

/// Push a local point out of the shape: `(pushed, penetration)`.
fn resolve(kind: &ShapeKind, r: f32, x: Vec3) -> (Vec3, f32) {
    match *kind {
        ShapeKind::Sphere(_) => {
            let d2 = x.length_squared();
            if d2 < EPS_POINT {
                (x + Vec3::new(r, 0.0, 0.0), r)
            } else if d2 < r * r {
                let len = d2.sqrt();
                (x * (r / len), r - len)
            } else {
                (x, 0.0)
            }
        }
        ShapeKind::Capsule { half_length: h, .. } => {
            let q = Vec3::new(0.0, x.y.min(h).max(-h), 0.0);
            let d = x - q;
            let d2 = d.length_squared();
            if d2 < EPS_POINT {
                (x + Vec3::new(r, 0.0, 0.0), r)
            } else if d2 < r * r {
                let len = d2.sqrt();
                (d * (r / len) + q, r - len)
            } else {
                (x, 0.0)
            }
        }
        ShapeKind::Box(_) => unreachable!("boxes resolve with resolve_box"),
    }
}

/// Box: `c` is the inset half-extent; one axis snaps (priority x, y, z).
fn resolve_box(c: Vec3, x: Vec3) -> (Vec3, f32) {
    let dd = x.abs() - c;
    let m = dd.x.max(dd.y.max(dd.z));
    if m >= 0.0 {
        return (x, 0.0);
    }
    let pen = -m;
    let mut out = x;
    let snap = |v: f32, c: f32| if v < 0.0 { -c } else { c };
    if -dd.x == pen {
        out.x = snap(x.x, c.x);
    } else if -dd.y == pen {
        out.y = snap(x.y, c.y);
    } else if -dd.z == pen {
        out.z = snap(x.z, c.z);
    }
    (out, pen)
}

/// Run the swept collision for all shapes, in record order.
pub fn collide_swept(state: &mut State, shapes: &[Shape], params: &Params) {
    let t = params.thickness;
    let mut buf = Vec::with_capacity(33);
    for shape in shapes {
        let mut ip = Interp::new(shape.previous, shape.current);
        // Swept world box (radius/extent + thickness; never the table value).
        let (lo, hi) = match shape.kind {
            ShapeKind::Sphere(r) => {
                let (a, b) = (shape.previous.t, shape.current.t);
                (a.min(b) - Vec3::splat(r + t), a.max(b) + Vec3::splat(r + t))
            }
            ShapeKind::Capsule {
                radius: r,
                half_length: h,
            } => {
                let half = Vec3::new(r + t, h + r + t, r + t);
                let (a0, b0) = shape.previous.bounds(half);
                let (a1, b1) = shape.current.bounds(half);
                (a0.min(a1), b0.max(b1))
            }
            ShapeKind::Box(e) => {
                let half = e + Vec3::splat(t);
                let (a0, b0) = shape.previous.bounds(half);
                let (a1, b1) = shape.current.bounds(half);
                (a0.min(a1), b0.max(b1))
            }
        };
        for i in 0..state.pos.len() {
            let (p, q) = (state.pos[i], state.prev[i]);
            // Leaf test of the native query (w lane included: pinned fail).
            let (mn, mx) = (p.min(q), p.max(q));
            let hit = mn.truncate().cmple(hi).all()
                && lo.cmple(mx.truncate()).all()
                && mn.w <= 0.0
                && 0.0 <= mx.w;
            if !hit || p.w < 0.0 {
                continue;
            }
            let factor = params.thickness_factor.as_ref().map(|f| f[i]);
            let mu = params
                .friction_table
                .as_ref()
                .map_or(params.friction, |f| f[i]);
            let (spacing, r_i, c_box) = match shape.kind {
                ShapeKind::Sphere(r) | ShapeKind::Capsule { radius: r, .. } => match factor {
                    None => ((r + t) * 0.5, r + t, Vec3::ZERO),
                    Some(f) => (r * 0.5, f * t + r, Vec3::ZERO),
                },
                ShapeKind::Box(e) => {
                    let e = if factor.is_none() {
                        e + Vec3::splat(t)
                    } else {
                        e
                    };
                    let mut m = e.x;
                    if e.y - m < 0.0 {
                        m = e.y;
                    }
                    if e.z - m < 0.0 {
                        m = e.z;
                    }
                    let c = match factor {
                        None => e - Vec3::splat(BOX_INSET),
                        Some(f) => Vec3::splat(f * t) + (e - Vec3::splat(BOX_INSET)),
                    };
                    (BOX_SPACING * m, 0.0, c)
                }
            };
            samples(&mut ip, q.truncate(), p.truncate(), spacing, &mut buf);
            let push = |x: Vec3| match shape.kind {
                ShapeKind::Box(_) => resolve_box(c_box, x),
                ref k => resolve(k, r_i, x),
            };
            let mut cur = buf[0];
            if matches!(shape.kind, ShapeKind::Capsule { .. }) {
                cur = push(cur).0;
            }
            for &delta in &buf[1..] {
                let (pushed, pen) = push(cur + delta);
                if pen > EPS23 {
                    let d = pushed - cur;
                    let d2 = (d.x * d.x + d.y * d.y) + (d.z * d.z + 0.0);
                    if d2 > EPS23 {
                        let l = d2.sqrt();
                        let mp = mu * pen;
                        let c = if l < mp { l } else { mp };
                        cur = pushed - d * (c / l);
                    }
                } else {
                    cur = pushed;
                }
            }
            state.pos[i] = shape.current.world(cur).extend(0.0);
        }
    }
}

/// Run opcode 7 by the settings' path. Returns the number of shapes the discrete path
/// skipped because their kind is not ported (spheres and boxes).
pub fn collide(state: &mut State, shapes: &[Shape], params: &Params) -> usize {
    if params.swept {
        collide_swept(state, shapes, params);
        0
    } else {
        collide_discrete(state, shapes, params)
    }
}

/// Unit vector perpendicular to the capsule axis `a` (the degenerate-distance
/// normal of `0x14241b800`).
fn perpendicular(a: Vec3) -> Vec3 {
    let (x, y, z) = (a.x.abs(), a.y.abs(), a.z.abs());
    let v = if x.min(y) - z < 0.0 {
        if y - x < 0.0 {
            Vec3::new(a.z, 0.0, -a.x)
        } else {
            Vec3::new(0.0, a.z, -a.y)
        }
    } else {
        Vec3::new(a.y, -a.x, 0.0)
    };
    v * (1.0 / dot(v, v).sqrt())
}

/// Discrete path for capsules, in record order (`0x1423d68d0`, `0x1423d6360`; kernels
/// `0x142419cc0` without and `0x14241b800` with the thickness table). For each free particle
/// `P` (previous `Q`), with the capsule's current frame (centre `C`, axis = row Y):
/// - `X = C + A·clamp(A·(P − C), ±h)`, `d = P − X`, `L = |d|`;
/// - `n = d / max(L, ε)` with `ε = 2.5e-6·(r + h)`, or a fixed perpendicular when `L < ε`;
/// - `pen = r_i − L` (`r_i = r + factor_i·thickness`, or `r + thickness` without the table);
///   no contact when `pen < 0`;
/// - contact: `out = X + n·r_i`. With `m` = the collider's motion at `Q` (previous frame to
///   current), the motion relative to the collider `rel = out − m − Q` is shortened by
///   `min(|rel|, μ·pen)`; the result `b + m` is then pushed back along `n` if it fell
///   inside the push plane.
///
/// Every particle is tested (the native BVH candidates add only no-contact particles, which
/// are left untouched). Returns the number of non-capsule shapes skipped (not ported).
pub fn collide_discrete(state: &mut State, shapes: &[Shape], params: &Params) -> usize {
    let mut skipped = 0;
    for shape in shapes {
        let ShapeKind::Capsule {
            radius: r,
            half_length: h,
        } = shape.kind
        else {
            skipped += 1;
            continue;
        };
        let (cur, prv) = (shape.current, shape.previous);
        let axis = cur.y;
        let base_r = if params.thickness_factor.is_some() {
            r
        } else {
            r + params.thickness
        };
        let eps = EPS_CAPSULE * (base_r + h);
        let fallback = perpendicular(axis);
        for i in 0..state.pos.len() {
            let p4 = state.pos[i];
            if p4.w < 0.0 {
                continue;
            }
            let p = p4.truncate();
            let ri = match &params.thickness_factor {
                Some(f) => base_r + f[i] * params.thickness,
                None => base_r,
            };
            let mu = params
                .friction_table
                .as_ref()
                .map_or(params.friction, |f| f[i]);
            let t = dot(axis, p - cur.t).min(h).max(-h);
            let x = axis * t + cur.t;
            let d = p - x;
            let l = dot(d, d).sqrt();
            let n = if l - eps < 0.0 {
                fallback
            } else {
                d / l.max(eps)
            };
            let pen = ri - l;
            if pen < 0.0 {
                continue;
            }
            let mut out = n * ri + x;
            let q = state.prev[i].truncate();
            let m = cur.world(prv.local(q)) - q;
            let a = out - m;
            let rel = a - q;
            let len = dot(rel, rel).sqrt();
            let c = len.min(mu * pen);
            let b = if 0.0 < len { a - rel * (c / len) } else { q };
            let b = b + m;
            let s = dot(b - out, n);
            out = if s < 0.0 { b - n * s } else { b };
            state.pos[i] = out.extend(p4.w);
        }
    }
    skipped
}

/// Convenience for callers holding particle arrays as `Vec4`.
pub fn frame_from_rows(rows: [Vec4; 4]) -> Frame {
    Frame {
        x: rows[0].truncate(),
        y: rows[1].truncate(),
        z: rows[2].truncate(),
        t: rows[3].truncate(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(p: Vec3, prev: Vec3) -> State {
        State {
            pos: vec![p.extend(0.0)],
            prev: vec![prev.extend(0.0)],
            target: vec![Vec4::ZERO],
        }
    }

    fn still(kind: ShapeKind, t: Vec3) -> Shape {
        let f = Frame {
            t,
            ..Frame::IDENTITY
        };
        Shape {
            kind,
            current: f,
            previous: f,
        }
    }

    #[test]
    fn sphere_pushes_a_resting_particle_to_its_surface() {
        let mut s = state(Vec3::new(0.5, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0));
        collide_swept(
            &mut s,
            &[still(ShapeKind::Sphere(1.0), Vec3::ZERO)],
            &Params::default(),
        );
        // Start and end coincide: one delta of zero, pushed from the inside.
        assert!((s.pos[0].truncate() - Vec3::X).length() < 1e-6);
    }

    #[test]
    fn fast_particle_cannot_cross_a_capsule() {
        // Capsule along Y at the origin; the particle moves from x = -2 to
        // x = +2 in one step, straight through it.
        let shape = still(
            ShapeKind::Capsule {
                radius: 0.5,
                half_length: 1.0,
            },
            Vec3::ZERO,
        );
        let mut s = state(Vec3::new(2.0, 0.0, 0.0), Vec3::new(-2.0, 0.0, 0.0));
        collide_swept(&mut s, &[shape], &Params::default());
        let p = s.pos[0].truncate();
        assert!((p - Vec3::new(-0.5, 0.0, 0.0)).length() < 1e-5, "{p}");
        assert_eq!(s.prev[0].truncate(), Vec3::new(-2.0, 0.0, 0.0));
    }

    #[test]
    fn moving_sphere_pushes_a_still_particle_ahead() {
        let shape = Shape {
            kind: ShapeKind::Sphere(1.0),
            previous: Frame {
                t: Vec3::new(-2.0, 0.0, 0.0),
                ..Frame::IDENTITY
            },
            current: Frame::IDENTITY,
        };
        let mut s = state(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.0, 0.1, 0.0));
        collide_swept(&mut s, &[shape], &Params::default());
        let p = s.pos[0].truncate();
        assert!((p.length() - 1.0).abs() < 1e-5 && p.x > 0.9, "{p}");
    }

    #[test]
    fn box_snaps_one_axis_just_inside_the_face() {
        let mut s = state(Vec3::new(0.9, 0.2, 0.0), Vec3::new(0.9, 0.2, 0.0));
        collide_swept(
            &mut s,
            &[still(ShapeKind::Box(Vec3::ONE), Vec3::ZERO)],
            &Params::default(),
        );
        let p = s.pos[0].truncate();
        assert_eq!(p, Vec3::new(1.0 - BOX_INSET, 0.2, 0.0));
    }

    #[test]
    fn pinned_and_distant_particles_are_untouched() {
        let shape = still(ShapeKind::Sphere(1.0), Vec3::ZERO);
        let mut s = State {
            pos: vec![
                Vec4::new(0.5, 0.0, 0.0, -1.0),
                Vec4::new(5.0, 0.0, 0.0, 0.0),
            ],
            prev: vec![
                Vec4::new(0.5, 0.0, 0.0, -1.0),
                Vec4::new(5.0, 0.0, 0.0, 0.0),
            ],
            target: vec![Vec4::ZERO; 2],
        };
        let before = s.pos.clone();
        collide_swept(&mut s, &[shape], &Params::default());
        assert_eq!(s.pos, before);
    }

    #[test]
    fn friction_holds_a_sliding_contact() {
        // A particle sliding along the sphere surface direction while
        // penetrating: with high friction it keeps more of its start.
        let shape = still(ShapeKind::Sphere(1.0), Vec3::ZERO);
        let run = |mu: f32| {
            let mut s = state(Vec3::new(0.3, 0.9, 0.0), Vec3::new(-0.3, 0.9, 0.0));
            let p = Params {
                friction: mu,
                ..Params::default()
            };
            collide_swept(&mut s, &[shape], &p);
            s.pos[0].truncate()
        };
        let free = run(0.0);
        let sticky = run(10.0);
        // Without friction the push carries forward, so the end may lie
        // outside, never inside. With enough friction (`mu * pen >= L`) each
        // step keeps the previous sample: static friction holds the particle.
        assert!(free.length() >= 1.0 - 1e-5, "{free}");
        assert!(sticky.x < free.x, "{sticky} vs {free}");
    }

    #[test]
    fn interpolated_pose_rotates_halfway() {
        let m1 = Frame {
            x: Vec3::Y,
            y: -Vec3::X,
            z: Vec3::Z,
            t: Vec3::new(2.0, 0.0, 0.0),
        };
        let mut ip = Interp::new(Frame::IDENTITY, m1);
        let half = ip.pose(16);
        let c = std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (half.x - Vec3::new(c, c, 0.0)).length() < 1e-5,
            "{:?}",
            half.x
        );
        assert!((half.t - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-6);
        assert_eq!(ip.pose(32), m1);
    }

    fn capsule(t: Vec3) -> Shape {
        still(
            ShapeKind::Capsule {
                radius: 0.1,
                half_length: 0.2,
            },
            t,
        )
    }

    #[test]
    fn discrete_capsule_pushes_to_the_inflated_surface() {
        // Particle 0.05 from the axis (inside r + thickness = 0.15), resting.
        let mut s = state(Vec3::new(0.05, 0.1, 0.0), Vec3::new(0.05, 0.1, 0.0));
        let p = Params {
            thickness: 0.05,
            friction: 0.2,
            ..Params::default()
        };
        assert_eq!(collide_discrete(&mut s, &[capsule(Vec3::ZERO)], &p), 0);
        let q = s.pos[0].truncate();
        assert!((q - Vec3::new(0.15, 0.1, 0.0)).length() < 1e-6, "{q}");
        // Outside: untouched, bit for bit.
        let mut s = state(Vec3::new(0.3, 0.0, 0.0), Vec3::new(0.3, 0.0, 0.0));
        collide_discrete(&mut s, &[capsule(Vec3::ZERO)], &p);
        assert_eq!(s.pos[0], Vec4::new(0.3, 0.0, 0.0, 0.0));
    }

    #[test]
    fn discrete_capsule_uses_the_thickness_table_and_skips_pinned() {
        let mut s = State {
            pos: vec![
                Vec4::new(0.05, 0.0, 0.0, 0.0),
                Vec4::new(0.05, 0.0, 0.0, -1.0),
            ],
            prev: vec![
                Vec4::new(0.05, 0.0, 0.0, 0.0),
                Vec4::new(0.05, 0.0, 0.0, -1.0),
            ],
            target: vec![Vec4::ZERO; 2],
        };
        let p = Params {
            thickness: 0.05,
            thickness_factor: Some(vec![0.5, 0.5]),
            ..Params::default()
        };
        collide_discrete(&mut s, &[capsule(Vec3::ZERO)], &p);
        assert!((s.pos[0].x - 0.125).abs() < 1e-6, "{}", s.pos[0]);
        assert_eq!(s.pos[1], Vec4::new(0.05, 0.0, 0.0, -1.0));
    }

    #[test]
    fn discrete_friction_acts_on_motion_relative_to_the_collider() {
        // The capsule moves +z by 0.04 into a particle resting at x = 0.12, z = 0.02 (prev
        // equal). Frictionless: pushed radially. With friction, the tangential slide relative
        // to the collider is shortened: the particle is carried further along +z.
        let shape = Shape {
            previous: Frame {
                t: Vec3::new(0.0, 0.0, -0.04),
                ..Frame::IDENTITY
            },
            ..capsule(Vec3::ZERO)
        };
        let run = |mu: f32| {
            let mut s = state(Vec3::new(0.12, 0.0, 0.02), Vec3::new(0.12, 0.0, 0.02));
            let p = Params {
                thickness: 0.05,
                friction: mu,
                ..Params::default()
            };
            collide_discrete(&mut s, &[shape], &p);
            s.pos[0].truncate()
        };
        let free = run(0.0);
        assert!((free.length() - 0.15).abs() < 1e-6, "{free}");
        let sticky = run(0.2);
        assert!(sticky.z > free.z, "{sticky} vs {free}");
        // Never left inside the push plane.
        assert!(sticky.length() >= 0.15 - 1e-6, "{sticky}");
    }

    #[test]
    fn discrete_on_axis_particle_uses_the_perpendicular() {
        let mut s = state(Vec3::ZERO, Vec3::ZERO);
        let p = Params {
            thickness: 0.05,
            ..Params::default()
        };
        collide_discrete(&mut s, &[capsule(Vec3::ZERO)], &p);
        // Axis = +Y: min(|x|,|y|) - |z| = 0 is not < 0, so (ay, -ax, 0) = (1, 0, 0).
        assert!((s.pos[0].truncate() - Vec3::new(0.15, 0.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn collide_routes_by_the_swept_flag() {
        let mut s = state(Vec3::new(0.3, 0.0, 0.0), Vec3::new(0.3, 0.0, 0.0));
        let boxed = still(ShapeKind::Box(Vec3::splat(0.1)), Vec3::ZERO);
        // The discrete path reports the unported box; the swept path handles it.
        assert_eq!(collide(&mut s, &[boxed], &Params::default()), 1);
        let p = Params {
            swept: true,
            ..Params::default()
        };
        assert_eq!(collide(&mut s, &[boxed], &p), 0);
    }
}
