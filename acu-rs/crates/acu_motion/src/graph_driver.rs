//! Pose and root motion from the native locomotion graph (ACU-RE-0069). Bevy independent.
//!
//! The interpreter (`atom_graph::Player`) chooses weighted clip leaves with their clip times and
//! mirror flags; this module samples them on the rig, blends the local transforms and sums the
//! clips' root motion. Controller input becomes graph parameters through `Controls`.
use crate::atom_graph::{ClipInfo, Graph, Leaf, Player, Value};
use crate::root_delta::{RootDelta, clip_root_delta};
use crate::unity::PoseRig;
use acu_assets::animation::Clip;
use glam::{Mat4, Quat, Vec2, Vec3};
use std::collections::HashMap;

/// Navigation hub of NewDemo (ACU-RE-0066).
pub const HUB_AT: usize = 9_974_731;

/// Forced start: hub walk machine (1), its idle machine (5), state 1 (`xx_Nav_IdleLow_Footr`).
/// Every live capture starts idle in `IdleLow_Footr` (loco-params-1, loco-turns-1,
/// loco-turn-delta-1/2). State 0 of the idle machine (`IdleLow-FullLookDnLedge_Fwd0_Footl`) is
/// its entry pose, left after 0.03 s when the stick is released.
pub const IDLE_START: [u16; 3] = [1, 5, 1];

/// Controller state for one simulation tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    /// Stick in actor-local axes: x right, y forward; length 0..1.
    pub stick: Vec2,
    /// Free-run (high profile) button.
    pub free_run: bool,
}

impl Controls {
    /// ACU-RE-0067 obs. 4: stick 0.25 -> 0 (dead zone), 0.5 -> 0.621, 0.75 -> 1.411,
    /// 1.0 -> 2.2; free run -> 6.277 with HighProfile.
    /// CHOICE: linear between the measured magnitudes, 0.621 from the dead zone to 0.5.
    pub fn movement_speed(&self) -> f32 {
        let m = self.stick.length().min(1.0);
        if m <= 0.25 {
            return 0.0;
        }
        if self.free_run {
            return 6.277;
        }
        const POINTS: [(f32, f32); 3] = [(0.5, 0.621), (0.75, 1.411), (1.0, 2.2)];
        if m <= POINTS[0].0 {
            return POINTS[0].1;
        }
        for w in POINTS.windows(2) {
            let ((a, va), (b, vb)) = (w[0], w[1]);
            if m <= b {
                return va + (vb - va) * (m - a) / (b - a);
            }
        }
        POINTS[2].1
    }

    /// Stick direction relative to the character in degrees, right positive (ACU-RE-0067).
    pub fn target_angle(&self) -> f32 {
        if self.stick.length() <= 1e-6 {
            return 0.0;
        }
        self.stick.x.atan2(self.stick.y).to_degrees()
    }
}

/// Walking facing rate (ACU-RE-0069 obs. 13): while walking, native turns the body toward the
/// stick at a constant 2.15 degrees per 60 Hz frame (about 2.25 rad/s) with an identity root
/// delta rotation (`loco-turn-delta-1`, `loco-turns-1`).
pub const WALK_TURN_RATE: f32 = 2.25;

/// Procedural facing change for one tick, radians (left positive, as the arena yaw).
/// INFERENCE: native applies it while a looping walk graph plays (in starts, stops and plant
/// turns the clip root rotation turns the body). CHOICE: the measured full-stick rate is used at
/// every walking speed, and free run is left to the graph's root motion (its native law is not
/// decoded: an exponential approach plus clip turns, obs. 13).
pub fn walk_turn(controls: &Controls, in_loop: bool, dt: f32) -> f32 {
    if !in_loop || controls.free_run || controls.movement_speed() <= 0.0 {
        return 0.0;
    }
    // TargetAngle is right positive; the arena yaw grows to the left.
    let error = -controls.target_angle().to_radians();
    let max = WALK_TURN_RATE * dt;
    error.clamp(-max, max)
}

/// Graph parameters measured in the open street of loco-params-1 (no wall, edge or crowd in
/// range reads the 10.0 cap).
const OPEN_GROUND: [(&str, f32); 3] = [
    ("EdgeDistanceY", 10.0),
    ("WallDistanceY", 10.0),
    ("DenseCrowdDistanceY", 10.0),
];

pub struct GraphDriver {
    pub player: Player,
    pub rig: PoseRig,
    pub clips: HashMap<u64, Clip>,
    /// Per joint: (joint written when mirrored, quaternion mode).
    pub mirror: Vec<(usize, u8)>,
    /// Model-space pose of the last step.
    pub pose: Vec<Mat4>,
    /// Resource names by clip id, for diagnostics.
    pub names: HashMap<u64, String>,
}

impl GraphDriver {
    /// `clips` must hold every clip the graph references that should play; missing clips are
    /// skipped with a diagnostic in the interpreter.
    pub fn new(
        graph: Graph,
        clips: HashMap<u64, Clip>,
        rig: PoseRig,
        mirror: Vec<(usize, u8)>,
    ) -> Result<Self, String> {
        let hub = graph.state_at(HUB_AT).ok_or("navigation hub absent")?;
        let infos = clips
            .iter()
            .map(|(id, c)| (*id, ClipInfo::from_clip(c)))
            .collect();
        let mut player = Player::new(graph, hub, infos)?;
        for (name, v) in OPEN_GROUND {
            player.set(name, Value::Scalar(v));
        }
        player.set("TimeScale", Value::Scalar(1.0));
        player.set("UseBipedFootIK", Value::Bool(true));
        player.start_in(&IDLE_START);
        let pose = rig.compose(rig.local_pose(clips.values().next().ok_or("no clips")?, 0.0, None));
        Ok(Self {
            player,
            rig,
            clips,
            mirror,
            pose,
            names: HashMap::new(),
        })
    }

    /// Back to the captured idle start with neutral controls.
    pub fn reset(&mut self) {
        self.apply(Controls::default());
        self.player.start_in(&IDLE_START);
    }

    /// The heaviest playing leaf.
    pub fn dominant(&self) -> Option<&Leaf> {
        self.player
            .leaves
            .iter()
            .max_by(|a, b| a.weight.total_cmp(&b.weight))
    }

    /// Whether the heaviest leaf plays in a looping graph state (walk / run / sprint loop).
    pub fn in_loop(&self) -> bool {
        self.dominant()
            .is_some_and(|l| self.player.graph.state_loops(l.state))
    }

    /// Resource name of a clip id, or its 16-digit hexadecimal id.
    pub fn name(&self, clip: u64) -> String {
        self.names
            .get(&clip)
            .cloned()
            .unwrap_or_else(|| format!("{clip:016x}"))
    }

    /// Set the controller parameters. Parkour: 0 idle, 1 moving (all 58 real samples of
    /// loco-params-1; 21 beside walls is not modelled in the open arena).
    pub fn apply(&mut self, c: Controls) {
        let speed = c.movement_speed();
        self.player.set("MovementSpeed", Value::Scalar(speed));
        self.player
            .set("HighProfile", Value::Bool(c.free_run && speed > 0.0));
        self.player
            .set("Parkour", Value::Int(i32::from(speed > 0.0)));
        self.player
            .set("TargetAngle", Value::Scalar(c.target_angle()));
    }

    /// Advance the graph by `dt`, rebuild the pose and return the root motion in the actor's
    /// native frame at the start of the step.
    pub fn step(&mut self, dt: f32) -> RootDelta {
        self.player.step(dt);
        let n = self.rig.hashes.len();
        let mut q_acc = vec![Vec4Acc::default(); n];
        let mut t_acc = vec![Vec3::ZERO; n];
        let mut s_acc = vec![Vec3::ZERO; n];
        let mut total = 0.0f32;
        let mut root_t = Vec3::ZERO;
        let mut root_q = Vec4Acc::default();
        for leaf in &self.player.leaves {
            let w = leaf.weight;
            if w <= 1e-4 {
                continue;
            }
            let Some(clip) = self.clips.get(&leaf.clip) else {
                continue;
            };
            let local = self.rig.local_pose(
                clip,
                leaf.time,
                leaf.mirror.then_some(self.mirror.as_slice()),
            );
            for (i, m) in local.iter().enumerate() {
                let (s, q, t) = m.to_scale_rotation_translation();
                q_acc[i].add(q, w);
                t_acc[i] += t * w;
                s_acc[i] += s * w;
            }
            total += w;
            if leaf.advance > 1e-6 && clip.duration > 0.0 {
                let a = (leaf.time - leaf.advance).rem_euclid(clip.duration);
                if let Ok(d) = clip_root_delta(clip, a, leaf.time, leaf.mirror) {
                    root_t += d.translation * w;
                    root_q.add(d.rotation, w);
                }
            }
        }
        if total > 1e-6 {
            let local: Vec<Mat4> = (0..n)
                .map(|i| {
                    Mat4::from_scale_rotation_translation(
                        s_acc[i] / total,
                        q_acc[i].get(),
                        t_acc[i] / total,
                    )
                })
                .collect();
            self.pose = self.rig.compose(local);
            // CHOICE: root motion is the weight-normalized blend of the leaves' deltas
            // (ACU-RE-0058 observed the incoming clip's delta for one transition type).
            RootDelta {
                translation: root_t / total,
                rotation: root_q.get(),
            }
        } else {
            RootDelta::IDENTITY
        }
    }
}

/// Weighted quaternion sum on one hemisphere.
#[derive(Clone, Copy, Default)]
struct Vec4Acc {
    sum: glam::Vec4,
    first: Option<Quat>,
}

impl Vec4Acc {
    fn add(&mut self, q: Quat, w: f32) {
        let first = *self.first.get_or_insert(q);
        let q = if first.dot(q) < 0.0 { -q } else { q };
        self.sum += glam::Vec4::from(q) * w;
    }
    fn get(&self) -> Quat {
        if self.sum.length_squared() <= 1e-12 {
            return Quat::IDENTITY;
        }
        Quat::from_vec4(self.sum).normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_reproduce_measured_speeds() {
        let at = |m: f32| {
            Controls {
                stick: Vec2::new(0.0, m),
                free_run: false,
            }
            .movement_speed()
        };
        assert_eq!(at(0.25), 0.0);
        assert!((at(0.5) - 0.621).abs() < 1e-6);
        assert!((at(0.75) - 1.411).abs() < 1e-6);
        assert!((at(1.0) - 2.2).abs() < 1e-6);
        let run = Controls {
            stick: Vec2::Y,
            free_run: true,
        };
        assert_eq!(run.movement_speed(), 6.277);
    }

    #[test]
    fn target_angle_is_right_positive_degrees() {
        let a = |x: f32, y: f32| {
            Controls {
                stick: Vec2::new(x, y),
                free_run: false,
            }
            .target_angle()
        };
        assert!((a(1.0, 0.0) - 90.0).abs() < 1e-4);
        assert!((a(-1.0, 0.0) + 90.0).abs() < 1e-4);
        assert!((a(0.0, -1.0).abs() - 180.0).abs() < 1e-4);
    }

    #[test]
    fn walk_turn_is_rate_limited_and_gated() {
        let right = Controls {
            stick: Vec2::X,
            free_run: false,
        };
        let step = walk_turn(&right, true, 1.0 / 60.0);
        assert!(
            (step.to_degrees() + 2.148).abs() < 0.01,
            "{}",
            step.to_degrees()
        );
        assert_eq!(walk_turn(&right, false, 1.0 / 60.0), 0.0);
        let run = Controls {
            free_run: true,
            ..right
        };
        assert_eq!(walk_turn(&run, true, 1.0 / 60.0), 0.0);
        let near = Controls {
            stick: Vec2::new(0.01, 1.0),
            free_run: false,
        };
        let s = walk_turn(&near, true, 1.0 / 60.0);
        assert!((s + 0.01f32.atan2(1.0)).abs() < 1e-6);
    }

    #[test]
    fn quaternion_blend_stays_on_one_hemisphere() {
        let mut acc = Vec4Acc::default();
        let q = Quat::from_rotation_z(0.3);
        acc.add(q, 0.5);
        acc.add(-q, 0.5);
        assert!(acc.get().angle_between(q) < 1e-5);
    }
}
