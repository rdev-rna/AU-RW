//! Bounded coat-chain solver, live routine 0x140045510 (ACU-RE-0028).
//! Inputs and persistent state are explicit, independent of Bevy. Collision
//! containers and nonzero wind are rejected by the capture decoder.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::bone_modifier::{normalize_newton, quat_mul_native};
use glam::{Quat, Vec3};

fn rotate(q: Quat, v: Vec3) -> Vec3 {
    let u = Vec3::new(q.x, q.y, q.z);
    let p = u * v;
    ((u.cross(v) * q.w + u * (p.x + (p.y + p.z))) + v * (q.w * q.w - 0.5)) * 2.0
}

pub fn multiply(a: Quat, b: Quat) -> Quat {
    mul(a, b)
}

#[derive(Clone, Debug)]
pub struct CoatNode {
    pub joint: usize,
    pub mass: f32,
    pub damping: f32,
    pub stiffness: f32,
    pub length: f32,
    pub gravity: Vec3,
    pub force: Vec3,
    pub force_mode: u32,
    pub limit_mode: u32,
    pub limit_degrees: [f32; 2],
    pub ellipse: Vec3,
    pub anchor: Vec3,
    pub preceding: Vec3,
    pub current: Vec3,
    pub previous: Vec3,
    pub model_rotation: Quat,
    pub rest_rotation: Quat,
    pub output_rotation: Quat,
}

#[derive(Clone, Debug)]
pub struct CoatChain {
    pub nodes: Vec<CoatNode>,
    pub done: bool,
    pub reset: bool,
    pub previous_dt: f32,
    pub gravity: Vec3,
}

fn small(v: Vec3) -> bool {
    v.abs().max_element() <= 1e-5
}

// Native packed reduction: (x*x + z*z) + y*y (w is zero).
fn norm2(v: Vec3) -> f32 {
    (v.x * v.x + v.z * v.z) + v.y * v.y
}
fn norm(v: Vec3) -> f32 {
    norm2(v).sqrt()
}
fn unit(v: Vec3) -> Vec3 {
    if norm2(v).abs() <= f32::from_bits(0x358637bd) {
        v
    } else {
        normalize_newton(v.extend(0.0)).truncate()
    }
}
fn mul(a: Quat, b: Quat) -> Quat {
    Quat::from_vec4(quat_mul_native(a.into(), b.into()))
}
fn inverse(q: Quat) -> Quat {
    Quat::from_vec4(
        glam::Vec4::new(-q.x, -q.y, -q.z, q.w)
            / crate::bone_modifier::dot4_native(q.into(), q.into()),
    )
}

/// Native shortest arc including its deterministic antiparallel fallback.
fn arc(from: Vec3, to: Vec3) -> Quat {
    let dot = (from.x * to.x + from.y * to.y) + from.z * to.z;
    let mut xyz = from.cross(to);
    let w = if dot >= 0.0 || xyz.abs().max_element() > 2e-5 {
        dot + 1.0
    } else {
        xyz = from.cross(Vec3::X);
        if xyz.abs().max_element() <= 2e-5 {
            xyz = from.cross(Vec3::Y);
        }
        xyz /= norm(xyz);
        0.0
    };
    Quat::from_vec4(normalize_newton(xyz.extend(w)))
}

fn limit(node: &CoatNode, parent: Quat, delta: Vec3) -> Option<Vec3> {
    let active = (node.limit_mode == 0 && node.limit_degrees[0].abs() > 1e-5)
        || (node.limit_mode == 1 && node.limit_degrees.iter().any(|a| a.abs() > 1e-5));
    if !active {
        return None;
    }
    let basis = mul(parent, node.rest_rotation);
    let local = rotate(inverse(basis), delta);
    let scale = if node.limit_mode == 0 {
        node.ellipse
    } else {
        Vec3::ONE
    };
    let scaled = local / scale;
    let len = norm(scaled);
    if !(len > 1e-5) || (scaled - Vec3::X * len).abs().max_element() <= 2e-5 {
        return None;
    }
    // The arc's scalar normalization uses (y² + x²) + z², whereas
    // the preceding packed length uses y² + (x² + z²).
    let scalar_len = ((scaled.y * scaled.y + scaled.x * scaled.x) + scaled.z * scaled.z).sqrt();
    let q = arc(Vec3::X, scaled / scalar_len);
    let constrained = if node.limit_mode == 0 {
        let angle = q.w.acos() * 2.0;
        let max = (node.limit_degrees[0] * std::f32::consts::PI) * 0.0055555557;
        let half = angle.clamp(-max, max) * 0.5;
        let mut axis = unit(Vec3::new(q.x, q.y, q.z));
        axis /= (axis.x * axis.x + (axis.y * axis.y + axis.z * axis.z)).sqrt();
        Quat::from_xyzw(
            axis.x * half.sin(),
            axis.y * half.sin(),
            axis.z * half.sin(),
            half.cos(),
        )
    } else {
        // Native YXZ decomposition; the middle angle is retained.
        let dy = 2.0 * (q.w * q.y + q.z * q.x);
        let dx = 1.0 - 2.0 * (q.y * q.y + q.x * q.x);
        let denom = (dy * dy + dx * dx).sqrt();
        let pitch = if denom == 0.0 { 0.0 } else { dy.atan2(dx) };
        let middle = (2.0 * (q.w * q.x - q.y * q.z)).atan2(denom);
        let yaw = (2.0 * (q.w * q.z + q.x * q.y)).atan2(1.0 - 2.0 * (q.z * q.z + q.x * q.x));
        let a = (node.limit_degrees[0] * std::f32::consts::PI) * 0.0055555557;
        let b = (node.limit_degrees[1] * std::f32::consts::PI) * 0.0055555557;
        if (pitch - pitch.clamp(-a, a)).abs() <= 1e-5 && (yaw - yaw.clamp(-b, b)).abs() <= 1e-5 {
            return None;
        }
        mul(
            mul(
                Quat::from_rotation_y(pitch.clamp(-a, a)),
                Quat::from_rotation_x(middle),
            ),
            Quat::from_rotation_z(yaw.clamp(-b, b)),
        )
    };
    let result = rotate(constrained, Vec3::X * len) * scale;
    if result.is_finite() {
        Some(rotate(basis, result))
    } else {
        None
    }
}

impl CoatChain {
    /// One native subsolver call. The first node's cached model rotation seeds
    /// the chain; all node positions are world space. A shared chain runs once
    /// per frame (`done`), even when more than one modifier dispatches to it.
    pub fn solve(&mut self, dt: f32, acceleration: Vec3) {
        self.solve_observed(dt, acceleration, |_| {});
    }

    /// Optional observer at the prediction boundary, for instruction-level replay.
    pub fn solve_observed(
        &mut self,
        dt: f32,
        acceleration: Vec3,
        mut predicted: impl FnMut(&[Vec3]),
    ) {
        if self.done || dt.abs() <= 1e-5 || self.nodes.is_empty() {
            return;
        }
        let dt2 = dt * dt;
        let mut positions = Vec::with_capacity(self.nodes.len());
        let mut previous = Vec::with_capacity(self.nodes.len());
        let mut preceding = self.nodes[0].anchor;
        let mut frame = self.nodes[0].model_rotation;
        for node in &mut self.nodes {
            let velocity = if self.previous_dt.abs() > 1e-5 {
                (node.current - node.previous) / self.previous_dt
            } else {
                Vec3::ZERO
            };
            let gravity = match node.force_mode {
                2 | 3 => node.gravity,
                _ => self.gravity,
            };
            let gravity = if matches!(node.force_mode, 1 | 3) {
                rotate(mul(frame, node.rest_rotation), self.gravity)
            } else {
                gravity
            };
            let predicted = node.current
                + (((-velocity * node.damping + acceleration * node.force.x) / node.mass
                    + gravity)
                    * dt2
                    + velocity * dt);
            let old_direction = unit(node.previous - node.preceding);
            let new_direction = unit(predicted - preceding);
            let correction = if self.previous_dt.abs() > 1e-5
                && old_direction.is_finite()
                && new_direction.is_finite()
            {
                ((old_direction - new_direction) / self.previous_dt) * node.stiffness * node.length
            } else {
                Vec3::ZERO
            };
            let predicted = predicted + correction * dt2;
            previous.push(node.current);
            positions.push(predicted);
            node.preceding = preceding;
            if matches!(node.force_mode, 1 | 3) {
                let local = rotate(inverse(frame), predicted - preceding);
                if !small(local) {
                    frame = mul(frame, arc(Vec3::X, unit(local)));
                }
            }
            preceding = predicted;
        }
        for _ in 0..4 {
            predicted(&positions);
            let mut p = self.nodes[0].anchor;
            let mut frame = self.nodes[0].model_rotation;
            let mut prev_inv_mass = 0.0;
            for (i, node) in self.nodes.iter_mut().enumerate() {
                let inv_mass = 1.0 / node.mass;
                let mut point = positions[i];
                let mut delta = point - p;
                if let Some(constrained) = limit(node, frame, delta) {
                    delta = constrained;
                    point = p + delta;
                }
                if !small(delta) {
                    let len = norm(delta);
                    let correction = (len - node.length) / ((inv_mass + prev_inv_mass) * len);
                    p += (delta * prev_inv_mass) * correction;
                    point -= (delta * inv_mass) * correction;
                }
                let reference = rotate(node.rest_rotation, Vec3::X);
                let local = rotate(inverse(frame), delta);
                node.output_rotation = if !small(local) {
                    mul(arc(reference, unit(local)), node.rest_rotation)
                } else {
                    node.rest_rotation
                };
                if i > 0 {
                    positions[i - 1] = p;
                }
                positions[i] = point;
                p = point;
                frame = mul(frame, node.output_rotation);
                prev_inv_mass = inv_mass;
            }
        }
        for ((node, p), old) in self.nodes.iter_mut().zip(positions).zip(previous) {
            node.current = p;
            node.previous = old;
        }
        self.previous_dt = dt;
        self.done = true;
        self.reset = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chain() -> CoatChain {
        CoatChain {
            nodes: vec![CoatNode {
                joint: 1,
                mass: 1.0,
                damping: 0.0,
                stiffness: 0.0,
                length: 1.0,
                gravity: Vec3::ZERO,
                force: Vec3::ZERO,
                force_mode: 0,
                limit_mode: 0,
                limit_degrees: [0.0; 2],
                ellipse: Vec3::ONE,
                anchor: Vec3::ZERO,
                preceding: Vec3::ZERO,
                current: Vec3::X,
                previous: Vec3::X,
                model_rotation: Quat::IDENTITY,
                rest_rotation: Quat::IDENTITY,
                output_rotation: Quat::IDENTITY,
            }],
            done: false,
            reset: false,
            previous_dt: 0.1,
            gravity: Vec3::new(0.0, 0.0, -10.0),
        }
    }
    #[test]
    fn gravity_prediction_projects_to_fixed_length() {
        let mut c = chain();
        c.solve(0.1, Vec3::ZERO);
        let expected = Vec3::new(1.0, 0.0, -0.1).normalize();
        assert!((c.nodes[0].current - expected).length() < 1e-6);
        assert!((rotate(c.nodes[0].output_rotation, Vec3::X) - expected).length() < 1e-6);
        assert_eq!(c.nodes[0].previous, Vec3::X);
        assert!(c.done);
    }
    #[test]
    fn shared_chain_runs_once_and_zero_dt_preserves_state() {
        let mut c = chain();
        c.solve(0.0, Vec3::ZERO);
        assert_eq!(c.nodes[0].current, Vec3::X);
        assert!(!c.done);
        c.solve(0.1, Vec3::ZERO);
        let first = c.nodes[0].current;
        c.solve(0.1, Vec3::ZERO);
        assert_eq!(c.nodes[0].current, first);
    }
    #[test]
    fn shortest_arc_antiparallel_is_finite() {
        let q = arc(Vec3::X, -Vec3::X);
        assert!(q.is_finite());
        assert!((rotate(q, Vec3::X) + Vec3::X).length() < 1e-6);
    }
}
