//! Unity adapter over the imported primary pose objective. ACU-RE-0018.
//! Profile eligibility/search weights/blending are reconstruction choices.
use crate::amo::{LocalPose, PoseWeights, primary_pose_cost};
use crate::amo_prediction::{
    PredictionFrame, PredictionWeights, RequestedFrame, prediction_geometry_cost,
};
use acu_assets::animation::{Clip, Track};
use glam::{Mat4, Quat, Vec2, Vec3};

pub struct PoseRig {
    pub hashes: Vec<u32>,
    pub bind: Vec<Mat4>,
    pub parents: Vec<Option<usize>>,
    order: Vec<usize>,
    local: Vec<Mat4>,
    root_frame: Option<(usize, Mat4)>,
    helpers: Vec<(usize, Helper)>,
    helper_order: Vec<usize>,
}
/// Bind-relative reconstruction of serialized modifier dependencies (finding0020).
/// Aim retains bind twist; weighted orientation blends target rotation deltas.
/// These policies are not a recovered implementation of Unity's modifier solver.
pub enum Helper {
    Aim { target: usize },
    Orientation { targets: Vec<(usize, f32)> },
}

/// Native animation evaluator row for one target joint rotation.
/// The mode bits are preserved from the live playback binding table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct NativeTrackBinding {
    pub target_hash: u32,
    pub source_track: usize,
    pub mode: u8,
    /// Low four bits of the native target channel type.
    pub channel_type: u8,
}

/// Quaternion permutations and sign flips recovered from the live ACU image
/// (finding0027). Quaternion lanes are xyzw.
fn native_binding_quaternion(q: Quat, mode: u8) -> Result<Quat, String> {
    let [x, y, z, w] = q.to_array();
    let transformed = match mode {
        0 | 7 => q,
        1 => Quat::from_xyzw(x, -y, -z, w),
        2 => Quat::from_xyzw(y, x, -w, -z),
        3 => Quat::from_xyzw(-x, y, -z, w),
        4 => Quat::from_xyzw(-w, z, y, -x),
        5 => Quat::from_xyzw(-x, -y, z, w),
        6 => Quat::from_xyzw(-z, w, -x, y),
        _ => return Err(format!("unsupported native binding mode {mode}")),
    };
    Ok(transformed.normalize())
}

impl PoseRig {
    pub fn new(
        hashes: Vec<u32>,
        bind: Vec<Mat4>,
        parents: Vec<Option<usize>>,
    ) -> Result<Self, String> {
        validate_rig(&bind, &parents)?;
        let mut order = Vec::new();
        let mut pending: Vec<_> = (0..bind.len()).collect();
        while !pending.is_empty() {
            let old = pending.len();
            pending.retain(|&i| {
                if parents[i].is_none_or(|p| order.contains(&p)) {
                    order.push(i);
                    false
                } else {
                    true
                }
            });
            if pending.len() == old {
                return Err("cyclic motion rig".into());
            }
        }
        let local = bind
            .iter()
            .enumerate()
            .map(|(i, m)| parents[i].map(|p| bind[p].inverse() * *m).unwrap_or(*m))
            .collect();
        if hashes.len() != bind.len() {
            return Err("motion rig hashes".into());
        }
        Ok(Self {
            hashes,
            bind,
            parents,
            order,
            local,
            root_frame: None,
            helpers: Vec::new(),
            helper_order: Vec::new(),
        })
    }
    pub fn set_helpers(&mut self, helpers: Vec<(usize, Helper)>) -> Result<(), String> {
        let dependencies = |h: &Helper| match h {
            Helper::Aim { target } => vec![*target],
            Helper::Orientation { targets } => targets.iter().map(|t| t.0).collect(),
        };
        for (i, h) in &helpers {
            if *i >= self.bind.len() || dependencies(h).iter().any(|&d| d >= self.bind.len()) {
                return Err("helper dependency outside rig".into());
            }
            if let Helper::Orientation { targets } = h
                && (targets.is_empty() || targets.iter().any(|t| !t.1.is_finite() || t.1 <= 0.0))
            {
                return Err("invalid helper weights".into());
            }
        }
        let mut order = Vec::new();
        let mut pending = self.order.clone();
        while !pending.is_empty() {
            let old = pending.len();
            pending.retain(|&i| {
                let ready = self.parents[i].is_none_or(|p| order.contains(&p))
                    && helpers
                        .iter()
                        .filter(|h| h.0 == i)
                        .all(|h| dependencies(&h.1).iter().all(|d| order.contains(d)));
                if ready {
                    order.push(i);
                }
                !ready
            });
            if old == pending.len() {
                return Err("cyclic helper dependencies".into());
            }
        }
        self.helpers = helpers;
        self.helper_order = order;
        Ok(())
    }
    fn apply_helpers(&self, pose: &mut [Mat4]) {
        if self.helpers.is_empty() {
            return;
        }
        let local: Vec<_> = pose
            .iter()
            .enumerate()
            .map(|(i, m)| {
                self.parents[i]
                    .map(|p| pose[p].inverse() * *m)
                    .unwrap_or(*m)
            })
            .collect();
        for &i in &self.helper_order {
            pose[i] = self.parents[i]
                .map(|p| pose[p] * local[i])
                .unwrap_or(local[i]);
            let Some((_, helper)) = self.helpers.iter().find(|h| h.0 == i) else {
                continue;
            };
            let (scale, _, position) = pose[i].to_scale_rotation_translation();
            let bind_q = self.bind[i].to_scale_rotation_translation().1;
            let q = match helper {
                Helper::Aim { target } => {
                    // Preserve the authored bind relation to the parent's frame.
                    let parent_delta = self.parents[i]
                        .map(|p| pose[p] * self.bind[p].inverse())
                        .unwrap_or(Mat4::IDENTITY);
                    let from = parent_delta
                        .transform_vector3(
                            self.bind[*target].w_axis.truncate() - self.bind[i].w_axis.truncate(),
                        )
                        .normalize_or_zero();
                    let to = (pose[*target].w_axis.truncate() - position).normalize_or_zero();
                    if from.length_squared() < 0.5 || to.length_squared() < 0.5 {
                        continue;
                    }
                    Quat::from_rotation_arc(from, to)
                        * parent_delta.to_scale_rotation_translation().1
                        * bind_q
                }
                Helper::Orientation { targets } => {
                    let mut sum = glam::Vec4::ZERO;
                    let mut reference = None;
                    for &(t, weight) in targets {
                        let delta = pose[t].to_scale_rotation_translation().1
                            * self.bind[t].to_scale_rotation_translation().1.inverse();
                        let first = *reference.get_or_insert(delta);
                        let sign = if first.dot(delta) < 0.0 { -1.0 } else { 1.0 };
                        sum += glam::Vec4::from_array(delta.to_array()) * weight * sign;
                    }
                    Quat::from_array(sum.normalize().to_array()) * bind_q
                }
            };
            pose[i] = Mat4::from_scale_rotation_translation(scale, q.normalize(), position);
        }
    }
    /// Serialized skeleton root placement is outside its bone-global array.
    pub fn set_root_frame(&mut self, hash: u32, frame: Mat4) -> Result<(), String> {
        let i = self
            .hashes
            .iter()
            .position(|&h| h == hash)
            .ok_or("animation root hash absent")?;
        if self.parents[i].is_some() || !frame.is_finite() || frame.determinant().abs() < 1e-8 {
            return Err("invalid animation root frame".into());
        }
        self.root_frame = Some((i, frame.inverse()));
        Ok(())
    }
    pub fn pose(&self, clip: &Clip, time: f32) -> Vec<Mat4> {
        let mut local = self.local.clone();
        for track in clip.tracks.iter().skip(2) {
            let Some(i) = self.hashes.iter().position(|&h| h == track.hash) else {
                continue;
            };
            let (scale, mut q, mut p) = local[i].to_scale_rotation_translation();
            let v = sample(track, time);
            match track.codec {
                0..=8 => q = Quat::from_array(v).normalize(),
                9 | 11 => p = Vec3::from_slice(&v[..3]),
                _ => continue,
            }
            local[i] = Mat4::from_scale_rotation_translation(scale, q, p);
        }
        if let Some((i, inverse)) = self.root_frame
            && clip.tracks.iter().skip(2).any(|t| t.hash == self.hashes[i])
        {
            local[i] = inverse * local[i];
        }
        for &i in &self.order {
            if let Some(p) = self.parents[i] {
                local[i] = local[p] * local[i];
            }
        }
        self.apply_helpers(&mut local);
        local
    }

    /// Evaluate the native bound rotation writes in local joint space, before
    /// root-frame removal, hierarchy composition, or reconstructed helpers.
    /// This is the comparison stage exposed by the live ACU context at slot
    /// +0x280. Missing target writes preserve the bind pose.
    pub fn local_pose_with_bindings(
        &self,
        clip: &Clip,
        time: f32,
        bindings: &[NativeTrackBinding],
    ) -> Result<Vec<Mat4>, String> {
        if !time.is_finite() || !clip.duration.is_finite() || clip.duration <= 0.0 {
            return Err("invalid native clip time".into());
        }
        if bindings.is_empty() || bindings.len() > 4096 {
            return Err("native binding count outside bounds".into());
        }
        let mut local = self.local.clone();
        let mut written = std::collections::HashSet::new();
        for binding in bindings {
            if binding.channel_type != 6 {
                return Err(format!(
                    "unsupported native target channel type {}",
                    binding.channel_type
                ));
            }
            let index = self
                .hashes
                .iter()
                .position(|&hash| hash == binding.target_hash)
                .ok_or_else(|| format!("native target hash {:08x} absent", binding.target_hash))?;
            if !written.insert(index) {
                return Err(format!(
                    "duplicate native target hash {:08x}",
                    binding.target_hash
                ));
            }
            let track = clip
                .tracks
                .get(binding.source_track)
                .ok_or("native source track outside clip")?;
            if binding.source_track < 2 || track.codec > 8 || track.times.is_empty() {
                return Err("native rotation binding references a non-quaternion track".into());
            }
            let q = native_binding_quaternion(Quat::from_array(sample(track, time)), binding.mode)?;
            let (scale, _, position) = local[index].to_scale_rotation_translation();
            local[index] = Mat4::from_scale_rotation_translation(scale, q, position);
        }
        Ok(local)
    }

    /// Evaluate bound rotations and reconstruct the composed rig pose.
    /// Translation and root-motion bindings are not yet part of this slice.
    pub fn pose_with_bindings(
        &self,
        clip: &Clip,
        time: f32,
        bindings: &[NativeTrackBinding],
    ) -> Result<Vec<Mat4>, String> {
        let mut local = self.local_pose_with_bindings(clip, time, bindings)?;
        if let Some((i, inverse)) = self.root_frame
            && bindings
                .iter()
                .any(|binding| binding.target_hash == self.hashes[i])
        {
            local[i] = inverse * local[i];
        }
        for &i in &self.order {
            if let Some(parent) = self.parents[i] {
                local[i] = local[parent] * local[i];
            }
        }
        self.apply_helpers(&mut local);
        Ok(local)
    }
    fn features(&self, pose: &[Mat4], velocity: Vec3) -> LocalPose {
        // Points use actual decoded joint globals; zero only if the rig lacks that hash.
        let point = |hash| {
            self.hashes
                .iter()
                .position(|&h| h == hash)
                .map(|i| pose[i].transform_point3(Vec3::ZERO).to_array())
                .unwrap_or([0.0; 3])
        };
        LocalPose {
            root_velocity: velocity.to_array(),
            tracked_positions: [point(0x176183f0), point(0x757f1291)],
            tracked_velocities: [[0.0; 3]; 2],
            paired_positions: [point(0x060df401), point(0x863d09fc)],
            heading_delta: None,
            angular_scalar: None,
            scalar68: None,
        }
    }
}
fn validate_rig(bind: &[Mat4], parents: &[Option<usize>]) -> Result<(), String> {
    if bind.is_empty()
        || bind.len() != parents.len()
        || bind.len() > 4096
        || bind
            .iter()
            .any(|m| !m.is_finite() || m.determinant().abs() < 1e-8)
        || parents.iter().flatten().any(|&p| p >= bind.len())
    {
        return Err("invalid motion rig".into());
    }
    Ok(())
}
pub fn sample(t: &Track, time: f32) -> [f32; 4] {
    let end = t.times.partition_point(|&s| s <= time);
    if end == 0 {
        return t.values[0];
    }
    if end == t.times.len() {
        return *t.values.last().unwrap();
    }
    let i = end - 1;
    let a = ((time - t.times[i]) / (t.times[end] - t.times[i])).clamp(0.0, 1.0);
    if t.codec <= 8 {
        return Quat::from_array(t.values[i])
            .normalize()
            .slerp(Quat::from_array(t.values[end]).normalize(), a)
            .to_array();
    }
    std::array::from_fn(|j| t.values[i][j] + (t.values[end][j] - t.values[i][j]) * a)
}
fn root(c: &Clip, time: f32) -> Vec3 {
    Vec3::from_slice(&sample(&c.tracks[0], time)[..3])
}
fn velocity(c: &Clip, time: f32) -> Vec3 {
    let a = time.clamp(0.0, (c.duration - 1.0 / 60.0).max(0.0));
    (root(c, a + 1.0 / 60.0) - root(c, a)) * 60.0
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Idle,
    Walk,
    Sprint,
}
pub struct Take {
    pub name: String,
    pub file_id: String,
    pub profile: Profile,
    pub clip: Clip,
}
pub struct Matcher {
    pub rig: PoseRig,
    pub takes: Vec<Take>,
    pub selected: usize,
    pub time: f32,
    pub pose: Vec<Mat4>,
    previous: Vec<Mat4>,
    blend: f32,
    pub cost: f32,
    candidates: Vec<Candidate>,
    search_in: f32,
    last_direction: Vec2,
}
struct Candidate {
    take: usize,
    time: f32,
    pose: LocalPose,
    future: Vec<PredictionFrame>,
}
// Selected navigation clips have identity root rotation (finding0019).
// These are explicit adapter horizons, not a Unity packed FH sample bank.
const HORIZONS: [f32; 3] = [0.05, 0.5, 1.1];
fn extended_root(c: &Clip, time: f32) -> Vec3 {
    let loops = (time / c.duration).floor();
    root(c, time.rem_euclid(c.duration)) + (root(c, c.duration) - root(c, 0.0)) * loops
}
fn future(c: &Clip, time: f32) -> Vec<PredictionFrame> {
    HORIZONS
        .into_iter()
        .map(|h| PredictionFrame {
            position: (extended_root(c, time + h) - root(c, time))
                .extend(0.0)
                .to_array(),
            velocity: velocity(c, (time + h).rem_euclid(c.duration))
                .extend(0.0)
                .to_array(),
            auxiliary40: [0.0; 4],
            relative_yaw: 0.0,
            valid: true,
            flags140: 0,
        })
        .collect()
}
impl Matcher {
    pub fn reset(&mut self) {
        self.selected = self
            .takes
            .iter()
            .position(|t| t.profile == Profile::Idle)
            .unwrap();
        self.time = 0.0;
        self.blend = 1.0;
        self.cost = 0.0;
        self.search_in = 0.0;
        self.last_direction = Vec2::ZERO;
        self.pose = self.rig.pose(&self.takes[self.selected].clip, 0.0);
        self.previous = self.pose.clone();
    }
    pub fn new(rig: PoseRig, takes: Vec<Take>) -> Result<Self, String> {
        if takes.is_empty()
            || [Profile::Idle, Profile::Walk, Profile::Sprint]
                .iter()
                .any(|p| !takes.iter().any(|t| t.profile == *p))
        {
            return Err("idle/walk/sprint required".into());
        }
        let selected = takes
            .iter()
            .position(|t| t.profile == Profile::Idle)
            .unwrap();
        for t in &takes {
            // Translation-only loop extension must never silently ignore a turning root.
            if t.clip.tracks.len() < 2
                || !t.clip.duration.is_finite()
                || t.clip.duration <= 0.0
                || t.clip.duration > 120.0
                || t.clip.tracks[1]
                    .values
                    .iter()
                    .any(|q| Quat::from_array(*q).angle_between(Quat::IDENTITY).abs() > 0.001)
            {
                return Err("directional adapter requires positive-duration translation loops with identity navigation rotation".into());
            }
        }
        let pose = rig.pose(&takes[selected].clip, 0.0);
        let mut candidates = Vec::new();
        for (take, t) in takes.iter().enumerate() {
            for frame in 0..(t.clip.duration * 30.0).ceil() as usize {
                let time = frame as f32 / 30.0;
                candidates.push(Candidate {
                    take,
                    time,
                    pose: rig.features(&rig.pose(&t.clip, time), Vec3::ZERO),
                    future: future(&t.clip, time),
                });
            }
        }
        Ok(Self {
            rig,
            takes,
            selected,
            time: 0.0,
            previous: pose.clone(),
            pose,
            blend: 1.0,
            cost: 0.0,
            candidates,
            search_in: 0.0,
            last_direction: Vec2::ZERO,
        })
    }
    /// Return native local root delta for this same selected pose interval.
    pub fn step(&mut self, profile: Profile, amount: f32, dt: f32) -> Vec3 {
        self.step_direction(profile, amount, Vec2::Y, dt)
    }
    /// Direction is actor-local right/forward, independent of actor facing.
    /// ACU-RE-0019: trajectory selection also runs within an unchanged profile.
    pub fn step_direction(
        &mut self,
        profile: Profile,
        amount: f32,
        direction: Vec2,
        dt: f32,
    ) -> Vec3 {
        assert!(dt.is_finite() && dt > 0.0 && amount.is_finite());
        assert!(direction.is_finite());
        let amount = amount.clamp(0.0, 1.0);
        let direction = direction.normalize_or_zero();
        self.search_in -= dt;
        let profile_changed = self.takes[self.selected].profile != profile;
        if profile_changed || self.search_in <= 0.0 || direction.dot(self.last_direction) < 0.966 {
            self.search_in = 0.1;
            self.last_direction = direction;
            let query = self.rig.features(&self.pose, Vec3::ZERO);
            let reference = &self
                .takes
                .iter()
                .find(|t| t.profile == profile)
                .unwrap()
                .clip;
            let speed = (root(reference, reference.duration) - root(reference, 0.0))
                .truncate()
                .length()
                / reference.duration;
            let v = (direction * speed).extend(0.0).extend(0.0).to_array();
            let requested: Vec<_> = HORIZONS
                .into_iter()
                .map(|h| RequestedFrame {
                    position: v.map(|x| x * h),
                    velocity: v,
                    auxiliary40: [0.0; 4],
                })
                .collect();
            let score = |pose: &LocalPose, frames: &[PredictionFrame]| {
                let p = primary_pose_cost(
                    &query,
                    pose,
                    PoseWeights {
                        values: [0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                    },
                    None,
                )
                .unwrap()
                .total;
                p + prediction_geometry_cost(
                    &requested,
                    frames,
                    0.0,
                    PredictionWeights {
                        values: [1.0, 0.5, 0.0, 0.0],
                        heading_deadzone: 0.0,
                    },
                    0.0,
                    None,
                )
                .unwrap()
                .total
            };
            // Score continuation at its actual phase. Never rewind the same loop
            // every search, and require an improvement to pay for a take switch.
            let mut best = (
                if profile_changed {
                    f32::MAX
                } else {
                    score(&query, &future(&self.takes[self.selected].clip, self.time))
                },
                self.selected,
                self.time,
            );
            for c in self.candidates.iter().filter(|c| {
                self.takes[c.take].profile == profile
                    && (profile_changed || c.take != self.selected)
            }) {
                let cost = score(&c.pose, &c.future) + if profile_changed { 0.0 } else { 0.12 };
                if cost < best.0 {
                    best = (cost, c.take, c.time);
                }
            }
            self.cost = best.0;
            if best.1 != self.selected {
                self.previous = self.pose.clone();
                self.blend = 0.0;
                self.selected = best.1;
                self.time = best.2;
            }
        }
        let c = &self.takes[self.selected].clip;
        let advance = dt
            * if profile == Profile::Idle {
                1.0
            } else {
                amount
            };
        let next = self.time + advance;
        let delta = extended_root(c, next) - root(c, self.time);
        self.time = next % c.duration;
        let target = self.rig.pose(c, self.time);
        self.blend = (self.blend + dt / 0.16).min(1.0);
        // Blend local transforms, then rebuild hierarchy to preserve limb lengths.
        // For Honor's matcher uses selector 3: (t*t)*3 - ((t+t)*(t*t)).
        // Preserve its f32 operation order instead of the algebraically
        // equivalent  t*t*(3-2*t), whose rounding differs at each tick.
        let t = self.blend;
        let tt = t * t;
        let blend = tt * 3.0 - (t + t) * tt;
        for &i in &self.rig.order {
            let p = self.rig.parents[i];
            let a = p
                .map(|p| self.previous[p].inverse() * self.previous[i])
                .unwrap_or(self.previous[i]);
            let b = p
                .map(|p| target[p].inverse() * target[i])
                .unwrap_or(target[i]);
            let (sa, qa, pa) = a.to_scale_rotation_translation();
            let (sb, qb, pb) = b.to_scale_rotation_translation();
            let local = Mat4::from_scale_rotation_translation(
                sa.lerp(sb, blend),
                qa.slerp(qb, blend),
                pa.lerp(pb, blend),
            );
            self.pose[i] = p.map(|p| self.pose[p] * local).unwrap_or(local);
        }
        self.rig.apply_helpers(&mut self.pose);
        delta
    }
    pub fn speed(&self) -> f32 {
        velocity(&self.takes[self.selected].clip, self.time)
            .truncate()
            .length()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track(hash: u32, codec: u8, values: Vec<[f32; 4]>) -> Track {
        Track {
            hash,
            codec,
            offset: 0,
            times: vec![0.0, 0.5, 1.0],
            values,
            encoded_values: Vec::new(),
        }
    }
    fn clip(speed: f32) -> Clip {
        Clip {
            duration: 1.0,
            key: 0,
            mapping_offset: 0,
            tracks: vec![
                track(
                    0,
                    9,
                    vec![
                        [0.0; 4],
                        [0.0, speed * 0.5, 0.0, 0.0],
                        [0.0, speed, 0.0, 0.0],
                    ],
                ),
                track(0, 0, vec![Quat::IDENTITY.to_array(); 3]),
                track(
                    0x176183f0,
                    0,
                    vec![
                        Quat::IDENTITY.to_array(),
                        Quat::from_rotation_z(1.0).to_array(),
                        Quat::IDENTITY.to_array(),
                    ],
                ),
            ],
        }
    }
    fn matcher() -> Matcher {
        let rig = PoseRig::new(
            vec![0x176183f0, 0x060df401],
            vec![Mat4::IDENTITY, Mat4::from_translation(Vec3::X)],
            vec![None, Some(0)],
        )
        .unwrap();
        Matcher::new(
            rig,
            [
                (Profile::Idle, 0.0),
                (Profile::Walk, 2.0),
                (Profile::Sprint, 7.0),
            ]
            .into_iter()
            .map(|(profile, speed)| Take {
                name: format!("{profile:?}"),
                file_id: String::new(),
                profile,
                clip: clip(speed),
            })
            .collect(),
        )
        .unwrap()
    }

    #[test]
    fn native_binding_modes_match_runtime_xyzw_lane_operations() {
        let q = Quat::from_xyzw(0.1, -0.2, 0.3, 0.9).normalize();
        let [x, y, z, w] = q.to_array();
        let expected = [
            [x, y, z, w],
            [x, -y, -z, w],
            [y, x, -w, -z],
            [-x, y, -z, w],
            [-w, z, y, -x],
            [-x, -y, z, w],
            [-z, w, -x, y],
            [x, y, z, w],
        ];
        for (mode, value) in expected.into_iter().enumerate() {
            let actual = native_binding_quaternion(q, mode as u8).unwrap().to_array();
            assert!(
                actual.iter().zip(value).all(|(a, b)| (*a - b).abs() < 1e-6),
                "mode {mode}: {actual:?} != {value:?}"
            );
        }
        assert!(native_binding_quaternion(q, 8).is_err());
    }

    #[test]
    fn pose_binding_uses_source_index_and_target_hash() {
        let rig = PoseRig::new(
            vec![0x1761_83f0, 0x060d_f401],
            vec![Mat4::IDENTITY, Mat4::from_translation(Vec3::X)],
            vec![None, Some(0)],
        )
        .unwrap();
        let mut clip = clip(1.0);
        clip.tracks[2].hash = 0xdead_beef;
        let source = Quat::from_xyzw(0.1, -0.2, 0.3, 0.9).normalize();
        clip.tracks[2].values = vec![source.to_array(); clip.tracks[2].times.len()];
        let binding = NativeTrackBinding {
            target_hash: 0x060d_f401,
            source_track: 2,
            mode: 5,
            channel_type: 6,
        };
        let pose = rig.pose_with_bindings(&clip, 0.5, &[binding]).unwrap();
        let local = pose[0].inverse() * pose[1];
        let actual = local.to_scale_rotation_translation().1;
        let expected = native_binding_quaternion(source, 5).unwrap();
        assert!(actual.dot(expected).abs() > 1.0 - 1e-6);
        assert!(
            rig.pose_with_bindings(&clip, 0.5, &[NativeTrackBinding { mode: 8, ..binding }])
                .is_err()
        );
    }

    #[test]
    fn profile_root_analog_wrap_and_reset() {
        let mut m = matcher();
        let d = m.step(Profile::Walk, 1.0, 1.0 / 60.0);
        assert!((d.y - 2.0 / 60.0).abs() < 1e-5);
        assert_eq!(m.selected, 1);
        let d = m.step(Profile::Sprint, 0.5, 1.0 / 60.0);
        assert!((d.y - 7.0 / 120.0).abs() < 1e-5);
        assert_eq!(m.selected, 2);
        m.time = 0.99;
        let d = m.step(Profile::Sprint, 1.0, 0.02);
        assert!((d.y - 0.14).abs() < 1e-5);
        assert!((m.time - 0.01).abs() < 1e-5);
        let d = m.step(Profile::Idle, 0.0, 1.0 / 60.0);
        assert_eq!(m.selected, 0);
        assert_eq!(d, Vec3::ZERO);
        m.reset();
        assert_eq!(m.time, 0.0);
        assert_eq!(m.selected, 0);
    }
    #[test]
    fn phase_search_uses_joint_pose_and_local_blend_preserves_lengths() {
        let mut m = matcher();
        m.pose = m.rig.pose(&m.takes[0].clip, 0.5);
        m.previous = m.pose.clone();
        m.step(Profile::Walk, 1.0, 1.0 / 60.0);
        assert!(
            m.time > 0.4 && m.time < 0.6,
            "phase search must prefer bent pose over zero"
        );
        for _ in 0..20 {
            m.step(Profile::Walk, 1.0, 1.0 / 60.0);
            assert!(
                (m.pose[1]
                    .transform_point3(Vec3::ZERO)
                    .distance(m.pose[0].transform_point3(Vec3::ZERO))
                    - 1.0)
                    .abs()
                    < 1e-5
            );
        }
    }
    #[test]
    fn cycles_and_invalid_parents_are_rejected() {
        assert!(PoseRig::new(vec![1, 2], vec![Mat4::IDENTITY; 2], vec![Some(1), Some(0)]).is_err());
        assert!(PoseRig::new(vec![1], vec![Mat4::IDENTITY], vec![Some(2)]).is_err());
    }
    #[test]
    fn helper_dependencies_move_weighted_thigh_and_descendants() {
        // A hip-driven knee at (1,0,0); separate roll bone constrained through
        // an aim helper. Both helpers are siblings of the animated hip.
        let mut rig = PoseRig::new(
            vec![10, 11, 12, 13, 14],
            vec![
                Mat4::IDENTITY,
                Mat4::from_translation(Vec3::X),
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                Mat4::from_translation(Vec3::X * 0.5),
            ],
            vec![None, Some(0), None, None, Some(3)],
        )
        .unwrap();
        rig.set_helpers(vec![
            (2, Helper::Aim { target: 1 }),
            (
                3,
                Helper::Orientation {
                    targets: vec![(2, 1.0), (0, 0.1)],
                },
            ),
        ])
        .unwrap();
        let mut pose = rig.bind.clone();
        rig.apply_helpers(&mut pose);
        assert!(
            pose.iter()
                .zip(&rig.bind)
                .all(|(a, b)| a.abs_diff_eq(*b, 1e-6)),
            "bind must stay unchanged"
        );
        pose[0] = Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        pose[1] = pose[0] * Mat4::from_translation(Vec3::X);
        rig.apply_helpers(&mut pose);
        assert!(
            pose[3]
                .transform_vector3(Vec3::X)
                .abs_diff_eq(Vec3::Y, 1e-5)
        );
        assert!(pose[4].w_axis.truncate().abs_diff_eq(Vec3::Y * 0.5, 1e-5));
        assert!(
            rig.set_helpers(vec![(0, Helper::Aim { target: 1 })])
                .is_err()
        );
        assert!(
            rig.set_helpers(vec![(2, Helper::Aim { target: 99 })])
                .is_err()
        );
        assert!(
            rig.set_helpers(vec![(
                2,
                Helper::Orientation {
                    targets: vec![(0, f32::NAN)]
                }
            )])
            .is_err()
        );
    }
    #[test]
    fn direction_changes_select_opposite_takes_without_changing_profile() {
        let base = matcher();
        let mut takes = base.takes;
        for profile in [Profile::Walk, Profile::Sprint] {
            for direction in [Vec2::X, -Vec2::X, -Vec2::Y] {
                let mut c = clip(if profile == Profile::Walk { 2.0 } else { 7.0 });
                for v in &mut c.tracks[0].values {
                    let distance = v[1];
                    v[0] = direction.x * distance;
                    v[1] = direction.y * distance;
                }
                takes.push(Take {
                    name: format!("{profile:?}-{direction}"),
                    file_id: String::new(),
                    profile,
                    clip: c,
                });
            }
        }
        let mut m = Matcher::new(base.rig, takes).unwrap();
        for profile in [Profile::Walk, Profile::Sprint] {
            for direction in [Vec2::Y, Vec2::X, -Vec2::X, -Vec2::Y, Vec2::Y] {
                let d = m.step_direction(profile, 1.0, direction, 1.0 / 60.0);
                assert!(d.truncate().normalize().dot(direction) > 0.999);
                assert_eq!(m.takes[m.selected].profile, profile);
                let selected = m.selected;
                let start = m.time;
                for _ in 0..30 {
                    m.step_direction(profile, 1.0, direction, 1.0 / 60.0);
                    assert_eq!(m.selected, selected, "steady requests must not churn takes");
                }
                assert!(
                    (m.time - (start + 0.5) % 1.0).abs() < 1e-4,
                    "search must preserve the playback phase"
                );
            }
        }
        // Negative control: a forward-only bank cannot provide lateral root travel.
        let mut forward_only = matcher();
        let d = forward_only.step_direction(Profile::Sprint, 1.0, Vec2::X, 1.0 / 60.0);
        assert!(d.truncate().normalize().dot(Vec2::X) < 0.01);
    }
    #[test]
    fn nonidentity_navigation_rotation_is_explicitly_unsupported() {
        let base = matcher();
        let mut takes = base.takes;
        takes[1].clip.tracks[1].values[1] = Quat::from_rotation_z(0.5).to_array();
        assert!(Matcher::new(base.rig, takes).is_err());
    }
    #[test]
    fn external_skeleton_root_placement_is_removed_once() {
        let mut m = matcher();
        let frame = Mat4::from_rotation_translation(
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::Z * 0.96,
        );
        m.rig.set_root_frame(0x176183f0, frame).unwrap();
        let mut c = clip(0.0);
        c.tracks[2].values = vec![Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array(); 3];
        c.tracks
            .push(track(0x176183f0, 9, vec![[0.0, 0.0, 0.96, 0.0]; 3]));
        let pose = m.rig.pose(&c, 0.5);
        assert!(pose[0].abs_diff_eq(Mat4::IDENTITY, 1e-5));
        assert!(
            pose[1]
                .transform_point3(Vec3::ZERO)
                .abs_diff_eq(Vec3::X, 1e-5)
        );
    }
}
