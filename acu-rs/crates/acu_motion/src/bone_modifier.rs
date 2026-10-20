//! Skeleton bone-modifier evaluation on the model-space pose, recovered in
//! `docs/findings/ACU-RE-0028-model-pose-bone-modifiers.md`.
//!
//! Implemented: local/model composition (`0x1400988d0`), the model-space joint
//! set with local recompute and descendant recompose (`0x140080fc0`,
//! `0x140097ea0`), the weighted set shared by every modifier apply
//! (`0x140014a60`), quaternion interpolation (`0x1400528e0`) and the MaxLookAt
//! solver/apply (`0x14001c680`, `0x1400494a0`). Not modelled: composition LOD
//! filtering, the native `rsqrtps` approximation (exact normalization is used,
//! expected deviation ~1e-7), and the other modifier classes.
// Negated float comparisons mirror the native NaN behavior of cmpleps/comiss.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use glam::{Mat3, Mat4, Quat, Vec3, Vec4};

/// One 32-byte pose record: position (native w = 0) and xyzw rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub position: Vec3,
    pub rotation: Quat,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModelPoseError {
    LengthMismatch { parents: usize, local: usize },
    Empty,
    InvalidRoot,
    InvalidParent { joint: usize, parent: usize },
    NotDepthFirst { joint: usize },
}

impl std::fmt::Display for ModelPoseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ModelPoseError {}

/// Local and model pose arrays over a depth-first hierarchy (native hierarchy
/// entries store the descendant count directly; it is derived here from parents).
#[derive(Clone, Debug)]
pub struct ModelPose {
    parents: Vec<Option<usize>>,
    descendants: Vec<usize>,
    pub local: Vec<Joint>,
    pub model: Vec<Joint>,
}

/// Native quaternion/vector normalization without the `rsqrtps` estimate.
fn normalize3(v: Vec3) -> Vec3 {
    v * (1.0 / v.dot(v).sqrt())
}

fn normalize_quat(q: Vec4) -> Quat {
    Quat::from_vec4(q * (1.0 / q.dot(q).sqrt()))
}

/// `2 * ((w² - ½) v + (q·v) q + w (q × v))`, the native rotation expression.
pub(crate) fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = Vec3::new(q.x, q.y, q.z);
    ((u.cross(v) * q.w + u * u.dot(v)) + v * (q.w * q.w - 0.5)) * 2.0
}

fn compose(parent: Joint, local: Joint) -> Joint {
    Joint {
        position: parent.position + rotate(parent.rotation, local.position),
        rotation: normalize_quat(Vec4::from(parent.rotation * local.rotation)),
    }
}

impl ModelPose {
    /// Joint 0 must be the only root and every parent must precede its child in
    /// depth-first order, as the native descendant-count ranges require.
    pub fn new(parents: Vec<Option<usize>>, local: Vec<Joint>) -> Result<Self, ModelPoseError> {
        if parents.len() != local.len() {
            return Err(ModelPoseError::LengthMismatch {
                parents: parents.len(),
                local: local.len(),
            });
        }
        if parents.is_empty() {
            return Err(ModelPoseError::Empty);
        }
        if parents[0].is_some() {
            return Err(ModelPoseError::InvalidRoot);
        }
        let n = parents.len();
        let mut end: Vec<usize> = (1..=n).collect();
        for joint in (1..n).rev() {
            let parent = parents[joint].ok_or(ModelPoseError::InvalidRoot)?;
            if parent >= joint {
                return Err(ModelPoseError::InvalidParent { joint, parent });
            }
            end[parent] = end[parent].max(end[joint]);
        }
        for (joint, &range_end) in end.iter().enumerate() {
            let inside = parents[joint + 1..range_end].iter().enumerate();
            if let Some((offset, _)) = inside.clone().find(|(_, p)| p.is_none_or(|p| p < joint)) {
                return Err(ModelPoseError::NotDepthFirst {
                    joint: joint + 1 + offset,
                });
            }
        }
        let descendants = (0..n).map(|j| end[j] - j - 1).collect();
        let mut pose = Self {
            parents,
            descendants,
            model: local.clone(),
            local,
        };
        pose.compose_range(0, n);
        Ok(pose)
    }

    /// Adopt a captured model array as is (no recomposition), so a stage can be
    /// replayed from the exact native state at its entry.
    pub fn from_parts(
        parents: Vec<Option<usize>>,
        local: Vec<Joint>,
        model: Vec<Joint>,
    ) -> Result<Self, ModelPoseError> {
        if model.len() != local.len() {
            return Err(ModelPoseError::LengthMismatch {
                parents: model.len(),
                local: local.len(),
            });
        }
        let mut pose = Self::new(parents, local)?;
        pose.model = model;
        Ok(pose)
    }

    pub fn parent(&self, joint: usize) -> Option<usize> {
        self.parents[joint]
    }

    pub fn descendants(&self, joint: usize) -> usize {
        self.descendants[joint]
    }

    pub fn len(&self) -> usize {
        self.parents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }

    /// `0x1400988d0` without LOD filtering; joint 0 composes with identity.
    fn compose_range(&mut self, first: usize, count: usize) {
        for joint in first..first + count {
            let parent = match self.parents[joint] {
                Some(p) => self.model[p],
                None => Joint {
                    position: Vec3::ZERO,
                    rotation: Quat::IDENTITY,
                },
            };
            self.model[joint] = compose(parent, self.local[joint]);
        }
    }

    /// `0x1400994e0` + `0x140097ea0`: store a model transform, derive its local
    /// transform from the parent model and recompose all descendants.
    pub fn set_model(&mut self, joint: usize, value: Joint) {
        self.model[joint] = value;
        let (parent_p, parent_q) = match self.parents[joint] {
            Some(p) => (self.model[p].position, self.model[p].rotation),
            None => (Vec3::ZERO, Quat::IDENTITY),
        };
        let inverse = parent_q.conjugate();
        self.local[joint] = Joint {
            position: rotate(inverse, value.position - parent_p),
            rotation: normalize_quat(Vec4::from(inverse * value.rotation)),
        };
        self.compose_range(joint + 1, self.descendants[joint]);
    }

    /// `0x140014a60`: weight < 1e-5 leaves the pose untouched; weight < 0.99999
    /// interpolates from the joint's current model transform.
    pub fn set_model_weighted(&mut self, joint: usize, target: Joint, weight: f32) {
        if !(weight >= 1e-5) {
            return;
        }
        let mut value = target;
        if weight < 0.99999 {
            let current = self.model[joint];
            value.rotation = quat_interp(current.rotation, target.rotation, weight);
            value.position = (target.position - current.position) * weight + current.position;
        }
        self.set_model(joint, value);
    }

    /// `0x140099950` + `0x1400980c0`: store a local transform and recompose the
    /// joint and its descendants from the parent model transform.
    pub fn set_local(&mut self, joint: usize, value: Joint) {
        self.local[joint] = value;
        self.compose_range(joint, self.descendants[joint] + 1);
    }

    /// `0x140014b50`, the local-space counterpart of `set_model_weighted`. The
    /// current local transform is read directly (the native reads go through
    /// the local pose object, whose model-space flag was set in every capture).
    pub fn set_local_weighted(&mut self, joint: usize, target: Joint, weight: f32) {
        if !(weight >= 1e-5) {
            return;
        }
        let mut value = target;
        if weight < 0.99999 {
            let current = self.local[joint];
            value.rotation = quat_interp(current.rotation, target.rotation, weight);
            value.position = (target.position - current.position) * weight + current.position;
        }
        self.set_local(joint, value);
    }
}

/// `0x1400528e0`: normalized lerp when dot >= 0.98935574 (bit pattern
/// 1065174635, compared as a signed integer), otherwise shortest-path slerp.
pub fn quat_interp(a: Quat, b: Quat, t: f32) -> Quat {
    let (va, vb) = (Vec4::from(a), Vec4::from(b));
    let dot = va.dot(vb);
    let lerp = normalize_quat(vb * t + va * (1.0 - t));
    if dot.to_bits() as i32 >= 1_065_174_635 {
        return lerp;
    }
    if dot <= -1.0 || dot >= 1.0 {
        return b;
    }
    let (theta, sign) = if dot >= 0.0 {
        (dot.acos(), 1.0)
    } else {
        ((-dot).acos(), -1.0)
    };
    if !(theta.abs() > 1e-5) {
        return b;
    }
    let inverse_sin = 1.0 / theta.sin();
    let wa = ((1.0 - t) * theta).sin() * inverse_sin;
    let wb = sign * (t * theta).sin() * inverse_sin;
    normalize_quat(vb * wb + va * wa)
}

/// Runtime MaxLookAt fields (property offsets in ACU-RE-0028). Axis values
/// 0..5 = X, Y, Z, -X, -Y, -Z.
#[derive(Clone, Debug, PartialEq)]
pub struct MaxLookAt {
    pub owner: usize,
    pub up_node: Option<usize>,
    pub look_at_bone: Option<usize>,
    pub look_at_used_axis: i32,
    pub up_used_axis: i32,
    pub up_node_control: i32,
    pub source_axis: i32,
    pub weight: f32,
}

const UNIT: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];

/// Every |component| <= 1e-5 (false for NaN, as with `cmpleps`).
fn tiny(v: Vec3) -> bool {
    v.x.abs() <= 1e-5 && v.y.abs() <= 1e-5 && v.z.abs() <= 1e-5
}

fn axis_index(value: i32) -> Option<usize> {
    match value {
        0 | 3 => Some(0),
        1 | 4 => Some(1),
        2 | 5 => Some(2),
        _ => None,
    }
}

/// Columns X, Y, Z to a quaternion, as compiled at `0x14001d6ec..`.
fn quat_from_columns(c: [Vec3; 3]) -> Quat {
    let q = quat_from_columns_raw(c);
    let q = if q.w < 0.0 { -q } else { q };
    normalize_quat(q)
}

/// Shared trace/largest-diagonal extraction (next table {1, 2, 0}); `0x14002c1b0`
/// returns this directly, without the sign flip and normalization of MaxLookAt.
fn quat_from_columns_raw(c: [Vec3; 3]) -> Vec4 {
    let m = |r: usize, k: usize| c[k][r];
    let trace = ((m(1, 1) + m(0, 0)) + m(2, 2)) + 1.0;
    if trace > 1.0 {
        let s = trace.sqrt() * 2.0;
        Vec4::new(
            m(2, 1) - m(1, 2),
            m(0, 2) - m(2, 0),
            m(1, 0) - m(0, 1),
            trace,
        ) / s
    } else {
        const NEXT: [usize; 3] = [1, 2, 0];
        let mut i = usize::from(m(1, 1) > m(0, 0));
        if m(2, 2) > m(i, i) {
            i = 2;
        }
        let (j, k) = (NEXT[i], NEXT[NEXT[i]]);
        let s2 = ((m(i, i) - m(j, j)) - m(k, k)) + 1.0;
        let s = s2.sqrt() * 2.0;
        let mut q = [0.0_f32; 4];
        q[j] = m(i, j) + m(j, i);
        q[k] = m(i, k) + m(k, i);
        q[3] = m(k, j) - m(j, k);
        q[i] = s2;
        Vec4::from_array(q) / s
    }
}

impl MaxLookAt {
    /// Build from serialized fields; `axes_and_control` is in serialized order
    /// LookAtUsedAxis, UpUsedAxis, UpNodeControl, SourceAxis.
    pub fn from_serialized(
        owner: usize,
        up_node: Option<usize>,
        look_at_bone: Option<usize>,
        axes_and_control: [i32; 4],
        weight: f32,
    ) -> Self {
        let [
            look_at_used_axis,
            up_used_axis,
            up_node_control,
            source_axis,
        ] = axes_and_control;
        Self {
            owner,
            up_node,
            look_at_bone,
            look_at_used_axis,
            up_used_axis,
            up_node_control,
            source_axis,
            weight,
        }
    }

    /// `0x14001c680`. Takes `&mut self` because the native solver permanently
    /// clears UpNode when it equals LookAtBone and UpNodeControl is 0.
    pub fn solve(&mut self, pose: &ModelPose) -> Option<Quat> {
        let owner_p = pose.model[self.owner].position;
        let look_p = self
            .look_at_bone
            .map_or(Vec3::ZERO, |b| pose.model[b].position);
        let mut dir = look_p - owner_p;
        if !(dir.dot(dir).abs() <= 1e-6) {
            dir = normalize3(dir);
        }
        if self.look_at_used_axis >= 3 {
            dir = -dir;
        }

        let mut up_axes = UNIT;
        let mut up_dir = normalize3(-owner_p);
        if let Some(up) = self.up_node {
            let joint = pose.model[up];
            up_axes = UNIT.map(|axis| rotate(joint.rotation, axis));
            up_dir = joint.position - owner_p;
            if !tiny(up_dir) {
                up_dir = normalize3(up_dir);
            }
            if self.up_node == self.look_at_bone && self.up_node_control == 0 {
                self.up_node = None;
                up_axes = UNIT;
                up_dir = normalize3(-owner_p);
            }
        }
        let up = if self.up_node_control == 1 {
            up_axes[axis_index(self.up_used_axis).unwrap_or(0)]
        } else {
            up_dir
        };

        let aim = axis_index(self.look_at_used_axis)?;
        let mut columns = UNIT;
        columns[aim] = if tiny(dir) { UNIT[aim] } else { dir };
        if let Some(second) = axis_index(self.source_axis) {
            if second == aim {
                return None;
            }
            let a = columns[aim];
            let v = a.cross(up.cross(a));
            let mut s = normalize3(if tiny(v) { UNIT[second] } else { v });
            let negate = match (aim, second) {
                (0, 1) => self.source_axis == 4,
                (0, 2) | (1, 2) => self.source_axis == 5,
                (1, 0) | (2, 0) => self.up_used_axis == 3,
                (2, 1) => self.up_used_axis == 4,
                _ => unreachable!(),
            };
            if negate {
                s = -s;
            }
            columns[second] = s;
            let third = 3 - aim - second;
            let c = columns[(third + 1) % 3].cross(columns[(third + 2) % 3]);
            columns[third] = if tiny(c) { UNIT[third] } else { normalize3(c) };
        }
        Some(quat_from_columns(columns))
    }

    /// `0x1400494a0`: requires a LookAtBone; keeps the owner's model position.
    pub fn apply(&mut self, pose: &mut ModelPose) {
        if self.look_at_bone.is_none() {
            return;
        }
        if let Some(rotation) = self.solve(pose) {
            let target = Joint {
                position: pose.model[self.owner].position,
                rotation,
            };
            pose.set_model_weighted(self.owner, target, self.weight);
        }
    }
}

/// One PositionConstraint target (pointed to from the constraint's target
/// array): `present` is the non-null node pointer at +0, `weight` the raw f32
/// at +8, `joint` the cached u16 index at +12 (`None` for 0xFFFF).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConstraintTarget {
    pub present: bool,
    pub weight: f32,
    pub joint: Option<usize>,
}

/// `0x14004b490`. The weight sum counts only present targets while the
/// position sum uses every target with a cached index, as compiled.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionConstraint {
    pub owner: usize,
    pub weight: f32,
    pub targets: Vec<ConstraintTarget>,
    /// Flag bit 0 at +0x34: offset (+0x40) rotated by the owner's parent model
    /// rotation and scaled by the entity scale.
    pub offset: Option<Vec3>,
    pub scale: f32,
}

impl PositionConstraint {
    pub fn apply(&self, pose: &mut ModelPose) {
        if !self.targets.iter().any(|t| t.present) {
            return;
        }
        let total = self
            .targets
            .iter()
            .filter(|t| t.present)
            .fold(0.0_f32, |sum, t| sum + t.weight);
        if total == 0.0 {
            return;
        }
        let mut position = Vec3::ZERO;
        for target in &self.targets {
            if let Some(joint) = target.joint {
                position += (pose.model[joint].position * target.weight) / total;
            }
        }
        if let Some(offset) = self.offset {
            let parent = self.owner_parent(pose);
            position += rotate(parent.rotation, offset) * self.scale;
        }
        let rotation = pose.model[self.owner].rotation;
        pose.set_model_weighted(self.owner, Joint { position, rotation }, self.weight);
    }

    /// `0x140073190`: the owner's parent model transform (hierarchy parent).
    fn owner_parent(&self, pose: &ModelPose) -> Joint {
        match pose.parent(self.owner) {
            Some(p) => pose.model[p],
            None => Joint {
                position: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            },
        }
    }
}

/// Lane sum as compiled (`shufps 0x4e` then `0x11`): `(y + w) + (x + z)`.
pub(crate) fn dot4_native(a: Vec4, b: Vec4) -> f32 {
    let p = a * b;
    (p.y + p.w) + (p.x + p.z)
}

/// Starting estimate for the native `rsqrtps` + Newton normalizations: the exact
/// reciprocal square root by default. Feature `hw-rsqrt` uses this CPU's
/// `rsqrtss` instead; that estimate is vendor-specific, so it only serves to
/// test, on the capture machine, whether a gate residual comes from it.
pub(crate) fn rsqrt_estimate(n: f32) -> f32 {
    #[cfg(all(feature = "hw-rsqrt", target_arch = "x86_64"))]
    {
        use std::arch::x86_64::{_mm_cvtss_f32, _mm_rsqrt_ss, _mm_set_ss};
        // SAFETY: SSE is part of the x86_64 baseline; the call has no memory access.
        unsafe { _mm_cvtss_f32(_mm_rsqrt_ss(_mm_set_ss(n))) }
    }
    #[cfg(not(all(feature = "hw-rsqrt", target_arch = "x86_64")))]
    {
        1.0 / n.sqrt()
    }
}

/// `rsqrtps` + one Newton step: `((3 - n·r·r) · (0.5·r)) · v`, with the
/// estimate from `rsqrt_estimate`.
pub(crate) fn normalize_newton(v: Vec4) -> Vec4 {
    let n = dot4_native(v, v);
    let r = rsqrt_estimate(n);
    v * ((3.0 - (n * r) * r) * (0.5 * r))
}

/// Hamilton product `a * b` in the lane order of `0x140049db0`.
pub(crate) fn quat_mul_native(a: Vec4, b: Vec4) -> Vec4 {
    let s1 = Vec4::new(a.x, -a.x, a.x, -a.x);
    let s2 = Vec4::new(a.y, a.y, -a.y, -a.y);
    let s3 = Vec4::new(-a.z, a.z, a.z, -a.z);
    ((b * a.w + Vec4::new(b.w, b.z, b.y, b.x) * s1) + s2 * Vec4::new(b.z, b.w, b.x, b.y))
        + Vec4::new(b.y, b.x, b.w, b.z) * s3
}

/// `0x140049db0` with the target blend `0x14001d980`.
#[derive(Clone, Debug, PartialEq)]
pub struct OrientationConstraint {
    pub owner: usize,
    pub weight: f32,
    pub targets: Vec<ConstraintTarget>,
    /// Flag bit 0 at +0x34: quaternion at +0x40, applied as `blend * offset`.
    pub offset: Option<Quat>,
}

impl OrientationConstraint {
    /// `0x14001d980`: sequential shortest-path slerp of the targets' model
    /// rotations, `t = 1 - old_total / new_total`. Only targets with a cached
    /// index take part (the present flag is not consulted here). `None` when no
    /// target has an index: the native output is then left uninitialized.
    pub fn blend(&self, pose: &ModelPose) -> Option<Quat> {
        let mut acc = Vec4::new(0.0, 0.0, 0.0, 1.0);
        let mut total = 0.0_f32;
        let mut written = false;
        for target in &self.targets {
            let Some(joint) = target.joint else { continue };
            let q = Vec4::from(pose.model[joint].rotation);
            written = true;
            // ucomiss + jne: an unordered total takes the "zero" path too.
            if !(total != 0.0) {
                acc = q;
                total = target.weight;
                continue;
            }
            let sum = target.weight + total;
            let ratio = if sum > 0.0 { total / sum } else { 1.0 };
            total = sum;
            let t = 1.0 - ratio;
            let dot = dot4_native(q, acc);
            acc = 'slerp: {
                if !(dot > -1.0) || dot >= 1.0 {
                    break 'slerp q;
                }
                let (theta, a, b) = if dot >= 0.0 {
                    let theta = dot.acos();
                    (theta, ((1.0 - t) * theta).sin(), (t * theta).sin())
                } else {
                    let theta = (-dot).acos();
                    (theta, ((1.0 - t) * theta).sin(), -(t * theta).sin())
                };
                if !(theta.abs() > 1e-5) {
                    break 'slerp q;
                }
                let inverse = 1.0 / theta.sin();
                normalize_newton(q * (inverse * b) + acc * (a * inverse))
            };
        }
        written.then(|| Quat::from_vec4(acc))
    }

    pub fn apply(&self, pose: &mut ModelPose) {
        if !self.targets.iter().any(|t| t.present) {
            return;
        }
        let Some(blend) = self.blend(pose) else {
            return;
        };
        let mut rotation = Vec4::from(blend);
        if let Some(offset) = self.offset {
            rotation = quat_mul_native(rotation, Vec4::from(offset));
        }
        let target = Joint {
            position: pose.model[self.owner].position,
            rotation: Quat::from_vec4(normalize_newton(rotation)),
        };
        pose.set_model_weighted(self.owner, target, self.weight);
    }
}

/// One Hinge sphere (32-byte entry of the array at `+0x70`): radius f32 at +8,
/// bone ref at +0x10 with the cached index at +0x1c (`None` for 0xFFFF; the
/// native lookup then yields the zero position at `0x14525fbc0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HingeSphere {
    pub radius: f32,
    pub joint: Option<usize>,
}

/// Hinge (class hash `324796c7`, vtable `0x142e77940`): apply `0x1400492a0`,
/// solver `0x140129a30`, angle `0x1401293b0`. Turns the owner about `axis`
/// through the frame (`pivot`, `frame`) by the largest sphere penetration angle.
#[derive(Clone, Debug, PartialEq)]
pub struct Hinge {
    pub owner: usize,
    pub weight: f32,
    /// `+0x30`: scaled by `scale` for the angle only, unscaled in the rotation.
    pub pivot: Vec3,
    /// `+0x40`, used without normalization.
    pub frame: Vec4,
    /// `+0x50`: reference direction (A) in the hinge frame.
    pub reference_axis: Vec4,
    /// `+0x60`: rotation axis (B) in the hinge frame, used without normalization.
    pub axis: Vec4,
    pub spheres: Vec<HingeSphere>,
    /// `+0xa0/+0xa4`: falloff over the probe angle (radians).
    pub angle_ramp: [f32; 2],
    /// `+0xa8/+0xac`: falloff over the sphere distance, scaled; (0, 0) = off.
    pub distance_ramp: [f32; 2],
    /// Entity scale from `0x140030b00` (1.0 when the owner handle is unresolved).
    pub scale: f32,
}

/// `maxps(v, -v) <= 1e-5` on all four lanes (false for NaN).
fn tiny4(v: Vec4) -> bool {
    v.x.abs() <= 1e-5 && v.y.abs() <= 1e-5 && v.z.abs() <= 1e-5 && v.w.abs() <= 1e-5
}

/// `ucomiss` equality: unordered compares equal.
fn ucomiss_eq(a: f32, b: f32) -> bool {
    a == b || a.is_nan() || b.is_nan()
}

/// Native quaternion-to-rotation expansion (column form; the image stores rows).
fn quat_matrix(q: Vec4) -> Mat3 {
    let (x, y, z, w) = (q.x, q.y, q.z, q.w);
    Mat3::from_cols(
        Vec3::new(
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ),
        Vec3::new(
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ),
        Vec3::new(
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ),
    )
}

/// Rodrigues rotation about an unnormalized axis, as built at `0x140129e9b..`.
fn axis_angle_matrix(a: Vec3, theta: f32) -> Mat3 {
    let (s, c) = (theta.sin(), theta.cos());
    let k = 1.0 - c;
    Mat3::from_cols(
        Vec3::new(
            k * a.x * a.x + c,
            k * a.x * a.y + s * a.z,
            k * a.x * a.z - s * a.y,
        ),
        Vec3::new(
            k * a.x * a.y - s * a.z,
            k * a.y * a.y + c,
            k * a.y * a.z + s * a.x,
        ),
        Vec3::new(
            k * a.x * a.z + s * a.y,
            k * a.y * a.z - s * a.x,
            k * a.z * a.z + c,
        ),
    )
}

fn affine(rotation: Mat3, translation: Vec3) -> Mat4 {
    Mat4::from_cols(
        rotation.x_axis.extend(0.0),
        rotation.y_axis.extend(0.0),
        rotation.z_axis.extend(0.0),
        translation.extend(1.0),
    )
}

/// `x + (y + z)` then square root.
fn length3_native(v: Vec4) -> f32 {
    (v.x * v.x + (v.y * v.y + v.z * v.z)).sqrt()
}

/// `0x1401299d0` (and the inline distance falloff): linear from 1 at `lo` to
/// 0 at `hi`, clamped; a span within 1e-5 becomes a step at `hi`.
fn hinge_ramp(x: f32, [lo, hi]: [f32; 2]) -> f32 {
    let span = hi - lo;
    if span.abs() > 1e-5 {
        let w = (hi - x) / span;
        if w > 0.0 {
            if w >= 1.0 { 1.0 } else { w }
        } else {
            0.0
        }
    } else if x > hi {
        0.0
    } else {
        1.0
    }
}

/// `0x140129100`: angle from `v` to `a` (both normalized with one Newton step),
/// negative when `b · (v × a) < 0`; 0 for a tiny `v`, pi when antiparallel.
fn hinge_signed_angle(v: Vec4, a: Vec4, b: Vec4) -> f32 {
    if tiny4(v) {
        return 0.0;
    }
    let (an, vn) = (normalize_newton(a), normalize_newton(v));
    let d = dot4_native(vn, an);
    if d >= 1.0 {
        return 0.0;
    }
    if !(d > -1.0) {
        return std::f32::consts::PI;
    }
    let cross = vn.truncate().cross(an.truncate()).extend(0.0);
    let triple = dot4_native(normalize_newton(b), cross);
    let length = dot4_native(cross, cross).sqrt();
    let signed = if triple >= 0.0 { length } else { -length };
    if signed >= 0.0 { d.acos() } else { -d.acos() }
}

/// `0x140129300`: cosine between `v` (Newton-normalized, three lanes) and `c`,
/// clamped to [-1, 1]; 1 for a tiny `v`.
fn hinge_clamped_cos(v: Vec4, c: Vec3) -> f32 {
    if tiny4(v) {
        return 1.0;
    }
    let n = v.x * v.x + (v.y * v.y + v.z * v.z);
    let r = rsqrt_estimate(n);
    let p = (v * ((3.0 - (n * r) * r) * (0.5 * r))).truncate() * c;
    let d = p.x + (p.y + p.z);
    if d > -1.0 {
        if d >= 1.0 { 1.0 } else { d }
    } else {
        -1.0
    }
}

impl Hinge {
    /// Owner model transform as the native 4x4 (column form).
    fn owner_matrix(&self, pose: &ModelPose) -> Mat4 {
        let owner = pose.model[self.owner];
        affine(quat_matrix(Vec4::from(owner.rotation)), owner.position)
    }

    /// `0x1401293b0`: the largest positive penetration angle over the spheres
    /// (0 when none penetrates, there are no spheres or an axis is tiny).
    pub fn angle(&self, pose: &ModelPose) -> f32 {
        let m = self.owner_matrix(pose);
        self.angle_with(pose, m, m.inverse())
    }

    fn angle_with(&self, pose: &ModelPose, m: Mat4, m_inv: Mat4) -> f32 {
        if self.spheres.is_empty() || tiny4(self.axis) || tiny4(self.reference_axis) {
            return 0.0;
        }
        let f = quat_matrix(self.frame);
        let a = (f * self.reference_axis.truncate()).extend(0.0);
        let b = (f * self.axis.truncate()).extend(0.0);
        let c = b.truncate().cross(a.truncate());
        let pivot = m.transform_point3(self.pivot * self.scale);
        let c_model = m.transform_vector3(c);
        let local = |p: Vec3| m_inv.transform_vector3(p).extend(0.0);
        let reject = |v: Vec4, axis: Vec4| v - axis * dot4_native(v, axis);
        let distance_ramp = self.distance_ramp.map(|x| x * self.scale);
        let ramp_off =
            ucomiss_eq(self.distance_ramp[0], 0.0) && ucomiss_eq(self.distance_ramp[1], 0.0);

        let mut best = 0.0_f32;
        for sphere in &self.spheres {
            let mut radius = self.scale * sphere.radius;
            let p = sphere.joint.map_or(Vec3::ZERO, |j| pose.model[j].position);
            let v = local(p - pivot);
            let falloff = if ramp_off {
                1.0
            } else {
                hinge_ramp(length3_native(v), distance_ramp)
            };
            let in_plane = reject(v, b);
            let mut angle = hinge_signed_angle(in_plane, a, b);
            if radius > 1e-5 {
                let length = length3_native(in_plane);
                if length >= radius {
                    angle -= (radius / length).asin();
                } else {
                    angle += -std::f32::consts::FRAC_PI_2;
                    radius = length;
                }
            }
            let contribution = if angle >= 0.0 {
                0.0
            } else {
                let probe = if radius > 1e-5 {
                    reject(local((c_model * radius + p) - pivot), a)
                } else {
                    reject(v, a)
                };
                let spread = hinge_clamped_cos(probe, c).acos().abs();
                -((hinge_ramp(spread, self.angle_ramp) * angle) * falloff)
            };
            if !(best >= contribution) {
                best = contribution;
            }
        }
        best
    }

    /// `0x1400492a0`: `M F R(axis, angle) F^-1` (column form), converted by
    /// `0x14002c1b0` and stored through the weighted set even when the angle
    /// is within 1e-5 (the target is then the owner's own matrix).
    pub fn apply(&self, pose: &mut ModelPose) {
        let m = self.owner_matrix(pose);
        let theta = self.angle_with(pose, m, m.inverse());
        let out = if theta.abs() > 1e-5 {
            let f = quat_matrix(self.frame);
            let f_inv = affine(f.transpose(), -(f.transpose() * self.pivot));
            let r = affine(axis_angle_matrix(self.axis.truncate(), theta), Vec3::ZERO);
            m * affine(f, self.pivot) * r * f_inv
        } else {
            m
        };
        let rotation = quat_from_columns_raw([
            out.x_axis.truncate(),
            out.y_axis.truncate(),
            out.z_axis.truncate(),
        ]);
        let target = Joint {
            position: out.w_axis.truncate(),
            rotation: Quat::from_vec4(rotation),
        };
        pose.set_model_weighted(self.owner, target, self.weight);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same_rotation(a: Quat, b: Quat, tolerance: f32) -> bool {
        a.dot(b).abs() > 1.0 - tolerance
    }

    fn chain() -> ModelPose {
        let q = |x: f32, y: f32, z: f32| Quat::from_euler(glam::EulerRot::XYZ, x, y, z);
        let local = vec![
            Joint {
                position: Vec3::new(0.1, 0.2, 1.0),
                rotation: q(0.1, 0.2, 0.3),
            },
            Joint {
                position: Vec3::new(0.0, 0.3, 0.1),
                rotation: q(-0.4, 0.1, 0.2),
            },
            Joint {
                position: Vec3::new(0.2, 0.0, 0.3),
                rotation: q(0.3, -0.2, 0.5),
            },
            Joint {
                position: Vec3::new(0.0, 0.25, 0.0),
                rotation: q(0.0, 0.6, -0.1),
            },
            Joint {
                position: Vec3::new(-0.3, 0.1, 0.4),
                rotation: q(0.2, 0.2, 0.2),
            },
        ];
        ModelPose::new(vec![None, Some(0), Some(1), Some(2), Some(0)], local).unwrap()
    }

    #[test]
    fn hierarchy_validation_rejects_non_depth_first_parents() {
        let joint = Joint {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
        };
        let err =
            ModelPose::new(vec![None, Some(0), Some(0), Some(1)], vec![joint; 4]).unwrap_err();
        // Joint 3 extends joint 1's range over joint 2, whose parent is 0.
        assert_eq!(err, ModelPoseError::NotDepthFirst { joint: 2 });
        assert!(ModelPose::new(vec![None, Some(2), Some(0)], vec![joint; 3]).is_err());
        assert_eq!(
            ModelPose::new(vec![Some(0)], vec![joint]).unwrap_err(),
            ModelPoseError::InvalidRoot
        );
    }

    #[test]
    fn interp_matches_slerp_and_lerp_regions() {
        let a = Quat::from_rotation_y(0.2);
        let b = Quat::from_axis_angle(Vec3::new(1.0, 1.0, 0.0).normalize(), 1.4);
        for t in [0.0, 0.25, 0.5, 0.9] {
            assert!(same_rotation(quat_interp(a, b, t), a.slerp(b, t), 1e-6));
            // Shortest path when the target sign is flipped.
            assert!(same_rotation(quat_interp(a, -b, t), a.slerp(b, t), 1e-6));
        }
        let near = Quat::from_rotation_y(0.21);
        let expected = (Vec4::from(near) * 0.3 + Vec4::from(a) * 0.7).normalize();
        assert!(Vec4::from(quat_interp(a, near, 0.3)).distance(expected) < 1e-7);
    }

    #[test]
    fn weighted_set_keeps_local_and_model_consistent() {
        let mut pose = chain();
        let target = Joint {
            position: Vec3::new(1.0, -0.5, 2.0),
            rotation: Quat::from_rotation_z(1.0),
        };
        pose.set_model_weighted(1, target, 1.0);
        assert_eq!(pose.model[1], target);
        let rebuilt = ModelPose::new(pose.parents.clone(), pose.local.clone()).unwrap();
        for j in 0..5 {
            assert!(
                rebuilt.model[j].position.distance(pose.model[j].position) < 1e-5,
                "joint {j}"
            );
            assert!(same_rotation(
                rebuilt.model[j].rotation,
                pose.model[j].rotation,
                1e-6
            ));
        }
        // Joint 4 is outside joint 1's subtree and must not change.
        assert_eq!(pose.model[4], chain().model[4]);

        let before = pose.model[2];
        pose.set_model_weighted(2, target, 0.5);
        assert!(same_rotation(
            pose.model[2].rotation,
            quat_interp(before.rotation, target.rotation, 0.5),
            1e-6
        ));
        assert!(
            pose.model[2]
                .position
                .distance(before.position.lerp(target.position, 0.5))
                < 1e-6
        );
        let unchanged = pose.clone();
        pose.set_model_weighted(2, target, 0.0);
        assert_eq!(pose.model, unchanged.model);
    }

    /// Independent Gram–Schmidt basis for every valid aim/second axis pair,
    /// with the compiled negation table.
    #[test]
    fn max_look_at_matches_independent_basis_and_negation_table() {
        let pose = chain();
        let (owner, look, up_node) = (2, 4, 0);
        let p = pose.model[owner].position;
        let d = (pose.model[look].position - p).normalize();
        let u = (pose.model[up_node].position - p).normalize();
        for look_axis in 0..6 {
            for source_axis in 0..6 {
                for up_axis in [0, 3, 4] {
                    let mut m = MaxLookAt::from_serialized(
                        owner,
                        Some(up_node),
                        Some(look),
                        [look_axis, up_axis, 0, source_axis],
                        1.0,
                    );
                    let (ai, si) = (look_axis as usize % 3, source_axis as usize % 3);
                    let Some(q) = m.solve(&pose) else {
                        assert_eq!(ai, si);
                        continue;
                    };
                    let aim = if look_axis >= 3 { -d } else { d };
                    let negate = match (ai, si) {
                        (0, 1) => source_axis == 4,
                        (0, 2) | (1, 2) => source_axis == 5,
                        (1, 0) | (2, 0) => up_axis == 3,
                        _ => up_axis == 4,
                    };
                    let second =
                        (u - aim * u.dot(aim)).normalize() * if negate { -1.0 } else { 1.0 };
                    let mut cols = [Vec3::ZERO; 3];
                    cols[ai] = aim;
                    cols[si] = second;
                    let third = 3 - ai - si;
                    cols[third] = cols[(third + 1) % 3].cross(cols[(third + 2) % 3]);
                    let expected = Quat::from_mat3(&Mat3::from_cols(cols[0], cols[1], cols[2]));
                    assert!(
                        same_rotation(q, expected, 1e-6),
                        "axes {look_axis} {source_axis} {up_axis}"
                    );
                    assert!(q.w >= 0.0);
                }
            }
        }
    }

    #[test]
    fn max_look_at_up_node_control_and_reset() {
        let pose = chain();
        // UpNodeControl 1 uses the up node's rotated Z axis (UpUsedAxis 2).
        let mut m = MaxLookAt::from_serialized(2, Some(0), Some(4), [0, 2, 1, 1], 1.0);
        let q = m.solve(&pose).unwrap();
        let up = pose.model[0].rotation * Vec3::Z;
        let x = q * Vec3::X;
        let expected_y = (up - x * up.dot(x)).normalize();
        assert!((q * Vec3::Y).distance(expected_y) < 1e-5);

        // UpNode == LookAtBone with control 0 is cleared and the origin is used.
        let mut m = MaxLookAt::from_serialized(2, Some(4), Some(4), [0, 0, 0, 1], 1.0);
        let q = m.solve(&pose).unwrap();
        assert_eq!(m.up_node, None);
        let mut fresh = MaxLookAt::from_serialized(2, None, Some(4), [0, 0, 0, 1], 1.0);
        assert_eq!(fresh.solve(&pose), Some(q));

        assert_eq!(
            MaxLookAt::from_serialized(2, None, Some(4), [6, 0, 0, 1], 1.0).solve(&pose),
            None
        );
    }

    #[test]
    fn position_constraint_weighted_average_offset_and_quirks() {
        let base = chain();
        let target = |joint, weight| ConstraintTarget {
            present: true,
            weight,
            joint: Some(joint),
        };
        let mut c = PositionConstraint {
            owner: 3,
            weight: 1.0,
            targets: vec![target(0, 0.5), target(4, 0.75)],
            offset: None,
            scale: 1.0,
        };
        let mut pose = base.clone();
        c.apply(&mut pose);
        let expected = (base.model[0].position * 0.5 + base.model[4].position * 0.75) / 1.25;
        assert!(pose.model[3].position.distance(expected) < 1e-6);
        assert!(same_rotation(
            pose.model[3].rotation,
            base.model[3].rotation,
            1e-7
        ));

        // Offset in the parent's model frame, scaled.
        c.offset = Some(Vec3::new(0.1, -0.2, 0.3));
        c.scale = 2.0;
        let mut pose = base.clone();
        c.apply(&mut pose);
        let parent = base.model[2].rotation;
        let with_offset = expected + parent * Vec3::new(0.1, -0.2, 0.3) * 2.0;
        assert!(pose.model[3].position.distance(with_offset) < 1e-6);

        // A target with a null node still contributes its position, but not
        // to the weight sum.
        c.offset = None;
        c.targets[1].present = false;
        let mut pose = base.clone();
        c.apply(&mut pose);
        let quirk = base.model[0].position + base.model[4].position * 1.5;
        assert!(pose.model[3].position.distance(quirk) < 1e-6);

        // No present target, or a zero weight sum: untouched.
        for targets in [vec![], vec![target(0, 0.0)]] {
            let mut pose = base.clone();
            PositionConstraint {
                targets,
                ..c.clone()
            }
            .apply(&mut pose);
            assert_eq!(pose.model, base.model);
        }
    }

    #[test]
    fn native_quaternion_helpers_match_glam() {
        let a = Quat::from_euler(glam::EulerRot::XYZ, 0.3, -1.1, 2.0);
        let b = Quat::from_euler(glam::EulerRot::XYZ, -0.7, 0.4, 0.9);
        let p = quat_mul_native(Vec4::from(a), Vec4::from(b));
        assert!(p.distance(Vec4::from(a * b)) < 1e-6);
        let v = Vec4::new(0.3, -2.0, 0.5, 1.5);
        assert!(normalize_newton(v).distance(v.normalize()) < 1e-7);
    }

    #[test]
    fn orientation_constraint_blends_sequentially_with_offset() {
        let base = chain();
        let target = |joint, weight| ConstraintTarget {
            present: true,
            weight,
            joint: Some(joint),
        };
        let (q0, q1, q4) = (
            base.model[0].rotation,
            base.model[1].rotation,
            base.model[4].rotation,
        );
        let mut c = OrientationConstraint {
            owner: 3,
            weight: 1.0,
            targets: vec![target(0, 1.0), target(4, 1.0)],
            offset: None,
        };
        assert!(same_rotation(
            c.blend(&base).unwrap(),
            q0.slerp(q4, 0.5),
            1e-6
        ));

        // Third target: slerp toward it by w / (running total).
        c.targets.push(target(1, 2.0));
        let expected = q0.slerp(q4, 0.5).slerp(q1, 0.5);
        assert!(same_rotation(c.blend(&base).unwrap(), expected, 1e-6));

        // Offset is applied on the right; position is kept.
        let offset = Quat::from_rotation_z(0.4);
        c.offset = Some(offset);
        let mut pose = base.clone();
        c.apply(&mut pose);
        assert!(same_rotation(
            pose.model[3].rotation,
            expected * offset,
            1e-6
        ));
        assert!(pose.model[3].position.distance(base.model[3].position) < 1e-6);

        // A leading zero weight is replaced by the next target, not blended.
        let z = OrientationConstraint {
            targets: vec![target(0, 0.0), target(4, 1.0)],
            ..c.clone()
        };
        assert!(same_rotation(z.blend(&base).unwrap(), q4, 1e-7));

        // No present target: untouched; no indexed target: no blend.
        let mut absent = c.clone();
        absent.targets.iter_mut().for_each(|t| t.present = false);
        let mut pose = base.clone();
        absent.apply(&mut pose);
        assert_eq!(pose.model, base.model);
        let mut unindexed = c.clone();
        unindexed.targets.iter_mut().for_each(|t| t.joint = None);
        assert_eq!(unindexed.blend(&base), None);
    }

    #[test]
    fn max_look_at_apply_keeps_position_and_weights_rotation() {
        let mut pose = chain();
        let before = pose.model[2];
        let mut m = MaxLookAt::from_serialized(2, Some(0), Some(4), [0, 0, 0, 1], 0.5);
        let solved = m.clone().solve(&pose).unwrap();
        m.apply(&mut pose);
        assert!(pose.model[2].position.distance(before.position) < 1e-6);
        assert!(same_rotation(
            pose.model[2].rotation,
            quat_interp(before.rotation, solved, 0.5),
            1e-6
        ));
        let unchanged = pose.model.clone();
        MaxLookAt {
            look_at_bone: None,
            ..m
        }
        .apply(&mut pose);
        assert_eq!(pose.model, unchanged);
    }

    /// Root, hinge owner at the origin, a limb along +X under the owner and a
    /// sphere joint at polar angle `phi`, distance `length` in the XY plane.
    fn hinge_scene(phi: f32, length: f32) -> (ModelPose, Hinge) {
        let joint = |position: Vec3| Joint {
            position,
            rotation: Quat::IDENTITY,
        };
        let pose = ModelPose::new(
            vec![None, Some(0), Some(1), Some(0)],
            vec![
                joint(Vec3::ZERO),
                joint(Vec3::ZERO),
                joint(Vec3::X),
                joint(Vec3::new(phi.cos(), phi.sin(), 0.0) * length),
            ],
        )
        .unwrap();
        let hinge = Hinge {
            owner: 1,
            weight: 1.0,
            pivot: Vec3::ZERO,
            frame: Vec4::new(0.0, 0.0, 0.0, 1.0),
            reference_axis: Vec4::new(1.0, 0.0, 0.0, 0.0),
            axis: Vec4::new(0.0, 0.0, 1.0, 0.0),
            spheres: vec![HingeSphere {
                radius: 0.5,
                joint: Some(3),
            }],
            angle_ramp: [10f32.to_radians(), 90f32.to_radians()],
            distance_ramp: [0.0, 0.0],
            scale: 1.0,
        };
        (pose, hinge)
    }

    #[test]
    fn hinge_angle_is_sphere_bearing_plus_tangent_angle() {
        let (phi, length) = (0.3_f32, 2.0_f32);
        let (pose, hinge) = hinge_scene(phi, length);
        // v = sphere direction, A = X, B = Z: b·(v×a) = -sin(phi), so the signed
        // angle is -phi; the radius adds asin(r/L) and the result is negated.
        let expected = phi + (0.5_f32 / length).asin();
        assert!((hinge.angle(&pose) - expected).abs() < 1e-5);

        // A sphere on the other side gives a positive signed angle: no edit.
        let (pose, mut hinge) = hinge_scene(-1.2, length);
        assert_eq!(hinge.angle(&pose), 0.0);
        // Inside the radius the tangent term is -pi/2 instead of asin.
        let (pose, _) = hinge_scene(0.3, 0.4);
        assert!((hinge.angle(&pose) - (0.3 + std::f32::consts::FRAC_PI_2)).abs() < 1e-5);
        // Tiny axes and empty sphere lists disable the hinge.
        hinge.axis = Vec4::ZERO;
        assert_eq!(hinge.angle(&pose), 0.0);
    }

    #[test]
    fn hinge_falloffs_scale_the_angle() {
        let (phi, length) = (0.3_f32, 2.0_f32);
        let (pose, mut hinge) = hinge_scene(phi, length);
        let full = hinge.angle(&pose);
        // Distance ramp 1 -> 3 at |v| = 2 halves the contribution.
        hinge.distance_ramp = [1.0, 3.0];
        assert!((hinge.angle(&pose) - full * 0.5).abs() < 1e-5);
        // A step ramp below the distance removes it entirely.
        hinge.distance_ramp = [1.0, 1.0];
        assert_eq!(hinge.angle(&pose), 0.0);
        assert_eq!(hinge_ramp(0.5, [1.0, 1.0]), 1.0);
        assert_eq!(hinge_ramp(2.0, [0.0, 1.0]), 0.0);
        assert_eq!(hinge_ramp(0.25, [0.0, 1.0]), 0.75);
        assert_eq!(hinge_ramp(-1.0, [0.0, 1.0]), 1.0);
        // Antiparallel and tiny inputs of the signed angle.
        let (x, z) = (Vec4::new(1.0, 0.0, 0.0, 0.0), Vec4::new(0.0, 0.0, 1.0, 0.0));
        assert_eq!(hinge_signed_angle(-x, x, z), std::f32::consts::PI);
        assert_eq!(hinge_signed_angle(Vec4::ZERO, x, z), 0.0);
        assert_eq!(hinge_clamped_cos(Vec4::ZERO, Vec3::Y), 1.0);
    }

    #[test]
    fn hinge_apply_rotates_about_the_frame_axis_through_the_pivot() {
        let (mut pose, mut hinge) = hinge_scene(0.3, 2.0);
        hinge.pivot = Vec3::new(0.1, -0.2, 0.05);
        let frame = Quat::from_euler(glam::EulerRot::XYZ, 0.2, -0.1, 0.3);
        hinge.frame = Vec4::from(frame);
        hinge.axis = Vec4::new(0.0, 0.0, 1.0, 0.0);
        let owner = pose.model[1];
        let limb = pose.model[2];
        let theta = hinge.angle(&pose);
        assert!(theta > 0.01);
        hinge.apply(&mut pose);

        // Independent form: rotate by theta about frame*axis through the pivot,
        // all in the owner's space, then map with the owner transform.
        let turn = Quat::from_axis_angle(frame * Vec3::Z, theta);
        let about_pivot = |p: Vec3| hinge.pivot + turn * (p - hinge.pivot);
        let owner_to_model = |p: Vec3| owner.position + owner.rotation * p;
        let expected_rotation = owner.rotation * turn;
        assert!(same_rotation(
            pose.model[1].rotation,
            expected_rotation,
            1e-6
        ));
        assert!(
            pose.model[1]
                .position
                .distance(owner_to_model(about_pivot(Vec3::ZERO)))
                < 1e-5
        );
        let limb_local = owner.rotation.conjugate() * (limb.position - owner.position);
        assert!(
            pose.model[2]
                .position
                .distance(owner_to_model(about_pivot(limb_local)))
                < 1e-5
        );

        // Below 1e-5 the owner's own matrix is stored back (no visible edit).
        let (mut pose, mut hinge) = hinge_scene(-1.2, 2.0);
        hinge.weight = 1.0;
        let before = pose.model.clone();
        hinge.apply(&mut pose);
        for (a, b) in before.iter().zip(&pose.model) {
            assert!(a.position.distance(b.position) < 1e-6);
            assert!(same_rotation(a.rotation, b.rotation, 1e-6));
        }
    }
}
