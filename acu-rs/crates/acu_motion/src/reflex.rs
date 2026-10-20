//! Reflex bone modifier (class `057daa86`, vtable `0x142e768c0`), recovered in
//! `docs/findings/ACU-RE-0028-model-pose-bone-modifiers.md`.
//!
//! Apply `0x14004b640`: every connector turns the direction between two joints,
//! seen from a third joint's frame (`0x14005b0e0`), into three sample weights
//! by walking a plane tree to a spherical triangle (`0x14006cf00`). The weighted
//! int16 samples are added to a base local transform with an approximate slerp
//! from identity, and the result is stored in local space (`0x140014b50`).
//! Only the three-sample connector kind is implemented; the nine-sample kind and
//! the second object list (`+0x28`, weights from `0x140028110`) are rejected by
//! the decoder because no live player object uses them.
// Negated float comparisons mirror the native NaN behavior of comiss/cmpps.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use glam::{Quat, Vec3, Vec4};

use crate::bone_modifier::{
    Joint, ModelPose, dot4_native, normalize_newton, quat_mul_native, rsqrt_estimate,
};

/// World transform of a pose object (`+0x00` position, `+0x10` rotation) and
/// its flag bit 26 (`+0x8c & 0x4000000`). When the flag is clear, joint reads
/// through `0x140093210`/`0x140093260` return world-space transforms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseFrame {
    pub position: Vec3,
    pub rotation: Vec4,
    pub model_space: bool,
}

impl PoseFrame {
    pub const MODEL: Self = Self {
        position: Vec3::ZERO,
        rotation: Vec4::W,
        model_space: true,
    };

    /// `0x140093210` (`0x142771d40` when not model space).
    fn position(&self, p: Vec3) -> Vec3 {
        if self.model_space {
            p
        } else {
            rotate_scalar_dot(self.rotation, p) + self.position
        }
    }

    /// `0x140093260`: `frame * q` when not model space.
    fn rotation(&self, q: Quat) -> Vec4 {
        if self.model_space {
            Vec4::from(q)
        } else {
            quat_mul_native(self.rotation, Vec4::from(q))
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReflexError {
    /// A tree read outside the captured tree bytes.
    TreeOutOfBounds {
        offset: i64,
    },
    /// More tree steps than the tree has 16-byte rows.
    TreeCycle,
    SampleOutOfRange {
        index: usize,
    },
}

impl std::fmt::Display for ReflexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ReflexError {}

/// Result of `0x14006cf00`, as stored in the 32-byte cache record: leaf id,
/// three sample indices and their weights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReflexLookup {
    pub leaf: u16,
    pub samples: [u16; 3],
    pub weights: [f32; 3],
}

/// One three-sample connector (24-byte entry kind 0 and its 96-byte record).
#[derive(Clone, Debug, PartialEq)]
pub struct ReflexConnector {
    /// Record `+0x18/+0x1a/+0x1c` through the connector's joint-ref table;
    /// `None` when the ref index is past the table (direction then `+X`).
    pub frame_joint: Option<usize>,
    pub from_joint: Option<usize>,
    pub to_joint: Option<usize>,
    /// Record `+0x00`, applied as `joint * offset`.
    pub frame_offset: Vec4,
    /// Bytes from the tree pointer at record `+0x1e`; offsets are relative to it.
    pub tree: Vec<u8>,
    /// 16-byte rows of the table at record `+0x50`: xyzw rotation then xyzw
    /// position, int16 each.
    pub samples: Vec<[i16; 8]>,
    /// Entry `u16 +0` times the record stride at `+0x4c`.
    pub sample_base: usize,
    /// Record `+0x30`, per-lane position dequantization.
    pub position_scale: Vec4,
}

/// Reflex object (0x40 bytes): owner ref `+0x10` (index `+0x1c`), weight
/// `+0x20`; base transform at `*(+0x08) + 0x30/+0x40`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reflex {
    pub owner: usize,
    pub weight: f32,
    pub base_position: Vec4,
    pub base_rotation: Vec4,
    pub connectors: Vec<ReflexConnector>,
    /// Component `+0xca4`, multiplies the summed position.
    pub scale: f32,
    /// Model pose object transform (joint reads for the direction).
    pub frame: PoseFrame,
}

/// `0x38000100`: int16 to float scale (about 1 / 32767).
const SAMPLE_SCALE: f32 = f32::from_bits(0x3800_0100);
/// Weights at or below 0.0005 (bit pattern 973279855, signed compare) are skipped.
const MIN_WEIGHT_BITS: i32 = 973_279_855;

/// `x + (y + z)` (scalar `addss` order).
fn dot3_scalar(a: Vec4, b: Vec4) -> f32 {
    let p = a * b;
    p.x + (p.y + p.z)
}

/// `2 ((w² - ½) v + (q·v) q + w (q × v))` with the dot summed `x + (y + z)`,
/// as in `0x142771d40` and `0x14005b0e0` (the shared `rotate` sums `(x + y) + z`,
/// which differs in the last bit at world-space magnitudes).
fn rotate_scalar_dot(q: Vec4, v: Vec3) -> Vec3 {
    let u = q.truncate();
    let p = u * v;
    let dot = p.x + (p.y + p.z);
    ((u.cross(v) * q.w + u * dot) + v * (q.w * q.w - 0.5)) * 2.0
}

/// `(y + x) + z` (`shufps 0x55`, `addps`, `movhlps`, `addps` order).
fn dot3_shuffled(a: Vec4, b: Vec4) -> f32 {
    let p = a * b;
    (p.y + p.x) + p.z
}

/// `minps(x, 1)` then `maxps(x, -1)`: NaN becomes 1.
fn clamp_unit(x: f32) -> f32 {
    let x = if x < 1.0 { x } else { 1.0 };
    if x > -1.0 { x } else { -1.0 }
}

/// `maxps(a, b)`: `a` only when `a > b`.
fn maxps(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// Polynomial `acos` of `0x14006cf00` (half-angle `asin` series for |x| > 0.5).
fn acos_native(x: f32) -> f32 {
    const C80: f32 = f32::from_bits(0x3cc4_8f19);
    const C90: f32 = f32::from_bits(0x3d2d_bdcf);
    const CA0: f32 = f32::from_bits(0x3d99_7dcc);
    const CB0: f32 = f32::from_bits(0x3d3a_73d8);
    const CC0: f32 = f32::from_bits(0x3e2a_aaf8);
    let sign = x.to_bits() & 0x8000_0000;
    let ax = x.abs();
    let small = ax <= 0.5;
    let s = if small { ax } else { ((1.0 - ax) * 0.5).sqrt() };
    let s2 = s * s;
    let s4 = s2 * s2;
    let p = (((s4 * C90 + CB0) * s4 + CC0) * s2 + ((s4 * C80 + CA0) * s4 + 1.0)) * s;
    if small {
        std::f32::consts::FRAC_PI_2 - f32::from_bits(p.to_bits() | sign)
    } else {
        let doubled = f32::from_bits((p + p).to_bits() | sign);
        doubled + if x < 0.0 { std::f32::consts::PI } else { 0.0 }
    }
}

/// `max(0.005, |a - b|)` over three lanes.
fn chord(a: Vec4, b: Vec4) -> f32 {
    let d = a - b;
    maxps(0.005, dot3_shuffled(d, d).sqrt())
}

/// Approximate slerp from identity to `q` (attenuated cubic correction of the
/// lerp parameter with constants 0.82279688 and 0.58549219), normalized with
/// one Newton step. `q` is negated when its w is negative.
pub fn slerp_from_identity(q: Vec4, t: f32) -> Vec4 {
    let attenuation = f32::from_bits(0x3f52_a2d1);
    let gain = f32::from_bits(0x3f15_e2d1);
    let d = dot4_native(Vec4::W, q);
    let k = {
        let a = 1.0 - attenuation * d;
        (a * a) * gain
    };
    let q = if d < 0.0 { -q } else { q };
    let f = if t <= 0.5 {
        (((((t + t) - 3.0) * k) * t + 1.0) + k) * t
    } else {
        let s = 1.0 - t;
        1.0 - ((((((s + s) - 3.0) * s) * k + 1.0) + k) * s)
    };
    normalize_newton((q - Vec4::W) * f + Vec4::W)
}

impl ReflexConnector {
    fn vec4_at(&self, offset: i64) -> Result<Vec4, ReflexError> {
        let o = usize::try_from(offset)
            .ok()
            .filter(|o| o.checked_add(16).is_some_and(|e| e <= self.tree.len()))
            .ok_or(ReflexError::TreeOutOfBounds { offset })?;
        let f =
            |k: usize| f32::from_le_bytes(self.tree[o + 4 * k..o + 4 * k + 4].try_into().unwrap());
        Ok(Vec4::new(f(0), f(1), f(2), f(3)))
    }

    fn u16_at(&self, offset: i64) -> Result<u16, ReflexError> {
        let o = usize::try_from(offset)
            .ok()
            .filter(|o| o.checked_add(2).is_some_and(|e| e <= self.tree.len()))
            .ok_or(ReflexError::TreeOutOfBounds { offset })?;
        Ok(u16::from_le_bytes([self.tree[o], self.tree[o + 1]]))
    }

    /// `0x14005b0e0`: `to - from` in world space, rotated by the inverse of
    /// `frame_joint * frame_offset` and normalized; `+Y` when the difference is
    /// within 1e-5 on every lane, `+X` when a joint ref is missing.
    pub fn direction(&self, pose: &ModelPose, frame: &PoseFrame) -> Vec4 {
        let (Some(fj), Some(a), Some(b)) = (self.frame_joint, self.from_joint, self.to_joint)
        else {
            return Vec4::X;
        };
        let f = quat_mul_native(frame.rotation(pose.model[fj].rotation), self.frame_offset);
        let from = frame.position(pose.model[a].position);
        let to = frame.position(pose.model[b].position);
        let v = to - from;
        if v.x.abs() <= 1e-5 && v.y.abs() <= 1e-5 && v.z.abs() <= 1e-5 {
            return Vec4::Y;
        }
        let r = rotate_scalar_dot(Vec4::new(-f.x, -f.y, -f.z, f.w), v);
        let n = r.x * r.x + (r.y * r.y + r.z * r.z);
        let inv = rsqrt_estimate(n);
        (r * ((3.0 - (n * inv) * inv) * (0.5 * inv))).extend(0.0)
    }

    /// `0x14006cf00`: plane-tree walk to a leaf triangle (A, B, C, plane N),
    /// then arc-corrected barycentric weights of `d` in that triangle.
    pub fn lookup(&self, d: Vec4) -> Result<ReflexLookup, ReflexError> {
        let mut node = 0i64;
        let mut steps = 0usize;
        let code = loop {
            steps += 1;
            if steps > self.tree.len() / 16 + 1 {
                return Err(ReflexError::TreeCycle);
            }
            let rows = [
                self.vec4_at(node)?,
                self.vec4_at(node + 16)?,
                self.vec4_at(node + 32)?,
            ];
            let s = (rows[1] * d.y + rows[0] * d.x) + rows[2] * d.z;
            let mask = s.is_negative_bitmask() as i64;
            let code = self.u16_at(node + 0x30 + 2 * mask)? as i16 as i64;
            node += 4 * code;
            if code & 3 != 0 {
                break code;
            }
        };
        let units = if code & 1 != 0 {
            node += 4 * code - 8;
            let side = dot3_scalar(self.vec4_at(node)?, d).is_sign_negative() as i64;
            (self.u16_at(node + 0xc + 2 * side)? as i64) << 2
        } else {
            code
        };
        let leaf = ((units as i32 & !3) << 4) as i64;
        let xyz = |v: Vec4| Vec4::new(v.x, v.y, v.z, 0.0);
        let a = xyz(self.vec4_at(leaf)?);
        let b = xyz(self.vec4_at(leaf + 16)?);
        let c = self.vec4_at(leaf + 32)?;
        let normal = self.vec4_at(leaf + 48)?;
        let leaf_id = self.u16_at(leaf + 0x1e)?;
        let (s0, s1, s2) = (
            self.u16_at(leaf + 0x0c)?,
            self.u16_at(leaf + 0x0e)?,
            self.u16_at(leaf + 0x1c)?,
        );

        let inv = 1.0 / dot3_scalar(normal, d);
        let cross = Vec4::new(
            d.y * a.z - d.z * a.y,
            d.z * a.x - d.x * a.z,
            d.x * a.y - d.y * a.x,
            0.0,
        );
        let e_b = dot3_scalar(b - a, cross);
        let e_c = dot3_scalar(c - a, cross);
        let u = inv * e_c;
        let u = if u > 0.0 {
            if u < 1.0 { u } else { 1.0 }
        } else {
            0.0
        };
        let v = inv * -e_b;
        let v = if v > 0.0 {
            if v < 1.0 { v } else { 1.0 }
        } else {
            0.0
        };
        let w = (1.0 - u) - v;

        let eps = f32::from_bits(0x3851_b717);
        let (pc, pa, pb) = (c * (v + eps), a * (w + eps), b * (u + eps));
        let n_cb = normalize_newton(pc + pb);
        let n_ca = normalize_newton(pc + pa);
        let n_ba = normalize_newton(pb + pa);

        let theta = [a, b, c].map(|x| acos_native(clamp_unit(dot3_shuffled(d, x))));
        let phi = [n_cb, n_ca, n_ba].map(|x| acos_native(clamp_unit(dot3_shuffled(d, x))));
        let chord_theta = [a, b, c].map(|x| chord(x, d));
        let chord_phi = [n_cb, n_ca, n_ba].map(|x| chord(x, d));
        let ratio: [f32; 3] = std::array::from_fn(|i| {
            (maxps(phi[i], chord_phi[i]) * chord_theta[i])
                / (maxps(theta[i], chord_theta[i]) * chord_phi[i])
        });
        let (wa, wb, wc) = (w * ratio[0], u * ratio[1], v * ratio[2]);
        let total = (wb + wa) + wc;
        let total = if total > 0.01 { total } else { 0.01 };
        let inv_total = 1.0 / total;
        Ok(ReflexLookup {
            leaf: leaf_id,
            samples: [s0, s1, s2],
            weights: [inv_total * wa, inv_total * wb, inv_total * wc],
        })
    }

    /// Sample row `sample_base + index`: (rotation, position) dequantized.
    pub fn sample(&self, index: u16) -> Result<(Vec4, Vec4), ReflexError> {
        let i = self.sample_base + index as usize;
        let row = self
            .samples
            .get(i)
            .ok_or(ReflexError::SampleOutOfRange { index: i })?;
        let lanes = |k: usize| {
            Vec4::new(
                row[k] as f32,
                row[k + 1] as f32,
                row[k + 2] as f32,
                row[k + 3] as f32,
            )
        };
        Ok((lanes(0) * SAMPLE_SCALE, lanes(4) * self.position_scale))
    }
}

impl Reflex {
    /// Local target before the weighted set: base transform with every
    /// connector's weighted samples (`acc * slerp(q, w)`, `pos += p * w`), then
    /// the position times the entity scale.
    pub fn evaluate(&self, pose: &ModelPose) -> Result<(Joint, Vec<ReflexLookup>), ReflexError> {
        let (mut position, mut rotation) = (self.base_position, self.base_rotation);
        let mut lookups = Vec::with_capacity(self.connectors.len());
        for c in &self.connectors {
            let lookup = c.lookup(c.direction(pose, &self.frame))?;
            for (&index, &weight) in lookup.samples.iter().zip(&lookup.weights) {
                if weight.to_bits() as i32 <= MIN_WEIGHT_BITS {
                    continue;
                }
                let (q, p) = c.sample(index)?;
                rotation = quat_mul_native(rotation, slerp_from_identity(q, weight));
                position += p * weight;
            }
            lookups.push(lookup);
        }
        let position = position * self.scale;
        let target = Joint {
            position: position.truncate(),
            rotation: Quat::from_vec4(rotation),
        };
        Ok((target, lookups))
    }

    /// `0x14004b640`. Returns the per-connector lookups for comparison with the
    /// native cache records.
    pub fn apply(&self, pose: &mut ModelPose) -> Result<Vec<ReflexLookup>, ReflexError> {
        let (target, lookups) = self.evaluate(pose)?;
        pose.set_local_weighted(self.owner, target, self.weight);
        Ok(lookups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(tree: &mut Vec<u8>, offset: usize, v: [f32; 4]) {
        if tree.len() < offset + 16 {
            tree.resize(offset + 16, 0);
        }
        for (k, x) in v.iter().enumerate() {
            tree[offset + 4 * k..offset + 4 * k + 4].copy_from_slice(&x.to_le_bytes());
        }
    }

    fn put_u16(tree: &mut [u8], offset: usize, v: u16) {
        tree[offset..offset + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Writes a leaf triangle at `offset` (multiple of 64): sample ids land at
    /// the A/B rows' lanes as the native reader expects. The normal is
    /// `(C - A) x (B - A)`, the orientation of the live leaves.
    fn leaf(tree: &mut Vec<u8>, offset: usize, a: Vec3, b: Vec3, c: Vec3, ids: [u16; 4]) {
        let n = (c - a).cross(b - a);
        put(tree, offset, a.extend(0.0).to_array());
        put(tree, offset + 16, b.extend(0.0).to_array());
        put(tree, offset + 32, c.extend(0.0).to_array());
        put(tree, offset + 48, n.extend(0.0).to_array());
        put_u16(tree, offset + 0x0c, ids[1]);
        put_u16(tree, offset + 0x0e, ids[2]);
        put_u16(tree, offset + 0x1c, ids[3]);
        put_u16(tree, offset + 0x1e, ids[0]);
    }

    /// Root node splitting on the sign of x (lane 0 plane = +X): masks with bit
    /// 0 set reach the leaf at 0xc0, the others the leaf at 0x80. Leaf codes
    /// have bit 1 set and address `(code & !3) * 16` from the tree start.
    fn two_leaf_connector() -> ReflexConnector {
        let mut tree = vec![0u8; 0x80];
        put(&mut tree, 0, [1.0, 0.0, 0.0, 0.0]);
        put(&mut tree, 16, [0.0, 0.0, 0.0, 0.0]);
        put(&mut tree, 32, [0.0, 0.0, 0.0, 0.0]);
        for mask in 0..16u16 {
            let code = if mask & 1 != 0 {
                (0xc0 / 16) | 2
            } else {
                (0x80 / 16) | 2
            };
            put_u16(&mut tree, 0x30 + 2 * mask as usize, code);
        }
        let s = std::f32::consts::FRAC_1_SQRT_2;
        leaf(&mut tree, 0x80, Vec3::X, Vec3::Y, Vec3::Z, [7, 0, 1, 2]);
        leaf(
            &mut tree,
            0xc0,
            -Vec3::X,
            Vec3::new(0.0, s, s),
            Vec3::new(0.0, -s, s),
            [8, 3, 4, 5],
        );
        ReflexConnector {
            frame_joint: Some(0),
            from_joint: Some(0),
            to_joint: Some(1),
            frame_offset: Vec4::W,
            tree,
            samples: vec![[0; 8]; 6],
            sample_base: 0,
            position_scale: Vec4::new(1.0, 1.0, 1.0, 0.0),
        }
    }

    #[test]
    fn acos_polynomial_matches_libm() {
        for i in -100..=100 {
            let x = i as f32 / 100.0;
            assert!((acos_native(x) - x.acos()).abs() < 2e-6, "{x}");
        }
        assert_eq!(clamp_unit(f32::NAN), 1.0);
        assert_eq!(clamp_unit(-3.0), -1.0);
    }

    #[test]
    fn lookup_routes_by_plane_and_weights_vertices() {
        let c = two_leaf_connector();
        let at_a = c.lookup(Vec4::X).unwrap();
        assert_eq!((at_a.leaf, at_a.samples), (7, [0, 1, 2]));
        assert!((at_a.weights[0] - 1.0).abs() < 1e-3, "{at_a:?}");
        assert!(at_a.weights[1].abs() < 1e-3 && at_a.weights[2].abs() < 1e-3);

        let at_c = c.lookup(Vec4::Z).unwrap();
        assert!((at_c.weights[2] - 1.0).abs() < 1e-3, "{at_c:?}");
        let at_b = c.lookup(Vec4::Y).unwrap();
        assert!((at_b.weights[1] - 1.0).abs() < 1e-3, "{at_b:?}");

        // The octant centre is symmetric in A, B and C.
        let centre = Vec3::ONE.normalize().extend(0.0);
        let w = c.lookup(centre).unwrap().weights;
        assert!(w.iter().all(|x| (x - 1.0 / 3.0).abs() < 1e-4), "{w:?}");
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);

        assert_eq!(c.lookup(-Vec4::X).unwrap().leaf, 8);
    }

    #[test]
    fn lookup_rejects_bad_trees() {
        let mut c = two_leaf_connector();
        c.tree.truncate(0xd0);
        assert!(matches!(
            c.lookup(-Vec4::X),
            Err(ReflexError::TreeOutOfBounds { .. })
        ));
        let mut c = two_leaf_connector();
        for mask in 0..16 {
            put_u16(&mut c.tree, 0x30 + 2 * mask, 0); // jump to itself forever
        }
        assert_eq!(c.lookup(Vec4::X), Err(ReflexError::TreeCycle));
    }

    #[test]
    fn slerp_from_identity_tracks_true_slerp() {
        let q = Quat::from_axis_angle(Vec3::new(1.0, 2.0, -0.5).normalize(), 1.9);
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let approx = Quat::from_vec4(slerp_from_identity(Vec4::from(q), t));
            let exact = Quat::IDENTITY.slerp(q, t);
            assert!(approx.angle_between(exact) < 2e-3, "t={t}");
        }
        let flipped = slerp_from_identity(-Vec4::from(q), 1.0);
        assert!(Quat::from_vec4(flipped).angle_between(q) < 1e-5);
    }

    #[test]
    fn direction_is_independent_of_the_world_frame() {
        let local = vec![
            Joint {
                position: Vec3::new(0.1, 0.2, 0.3),
                rotation: Quat::from_rotation_y(0.7),
            },
            Joint {
                position: Vec3::new(0.0, 0.0, 0.4),
                rotation: Quat::IDENTITY,
            },
        ];
        let pose = ModelPose::new(vec![None, Some(0)], local).unwrap();
        let c = two_leaf_connector();
        let model = c.direction(&pose, &PoseFrame::MODEL);
        let world = PoseFrame {
            position: Vec3::new(267.0, -175.0, -0.5),
            rotation: Vec4::from(Quat::from_rotation_z(1.15)),
            model_space: false,
        };
        let d = c.direction(&pose, &world);
        assert!((d - model).length() < 1e-3, "{d} {model}");
        // Child sits along the parent's own +Z, so the direction is +Z.
        assert!((model - Vec4::Z).length() < 1e-6, "{model}");
        let missing = ReflexConnector {
            to_joint: None,
            ..two_leaf_connector()
        };
        assert_eq!(missing.direction(&pose, &PoseFrame::MODEL), Vec4::X);
    }

    #[test]
    fn apply_sets_local_from_weighted_samples() {
        let local = vec![
            Joint {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            },
            Joint {
                position: Vec3::new(1.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
            },
            Joint {
                position: Vec3::new(0.0, 0.0, 1.0),
                rotation: Quat::IDENTITY,
            },
            Joint {
                position: Vec3::new(0.0, 1.0, 0.0),
                rotation: Quat::IDENTITY,
            },
        ];
        // Joint 2 (child of 1) points along +Z from joint 1's frame, so the
        // lookup hits vertex C (sample 2) of the first leaf with weight ~1.
        let mut pose = ModelPose::new(vec![None, Some(0), Some(1), Some(2)], local).unwrap();
        let mut c = two_leaf_connector();
        c.frame_joint = Some(1);
        c.from_joint = Some(1);
        c.to_joint = Some(2);
        let q = Quat::from_rotation_x(0.5);
        let qi = (Vec4::from(q) * 32767.0).round();
        c.samples[2] = [
            qi.x as i16,
            qi.y as i16,
            qi.z as i16,
            qi.w as i16,
            100,
            -200,
            0,
            0,
        ];
        c.position_scale = Vec4::new(0.001, 0.001, 0.001, 0.0);
        let reflex = Reflex {
            owner: 3,
            weight: 1.0,
            base_position: Vec4::new(0.0, 1.0, 0.0, 0.0),
            base_rotation: Vec4::W,
            connectors: vec![c],
            scale: 2.0,
            frame: PoseFrame::MODEL,
        };
        let lookups = reflex.apply(&mut pose).unwrap();
        assert_eq!(lookups[0].samples, [0, 1, 2]);
        let w = lookups[0].weights[2];
        assert!((w - 1.0).abs() < 1e-3);
        assert!(pose.local[3].rotation.angle_between(q) < 2e-3);
        let expected = (Vec3::new(0.0, 1.0, 0.0) + Vec3::new(0.1, -0.2, 0.0) * w) * 2.0;
        assert!((pose.local[3].position - expected).length() < 1e-5);
        let parent = pose.model[2];
        assert!(
            (pose.model[3].position - (parent.position + parent.rotation * pose.local[3].position))
                .length()
                < 1e-5
        );
        // Zero weight leaves the pose untouched.
        let before = pose.local.clone();
        let idle = Reflex {
            weight: 0.0,
            ..reflex
        };
        idle.apply(&mut pose).unwrap();
        assert_eq!(pose.local, before);
    }
}
