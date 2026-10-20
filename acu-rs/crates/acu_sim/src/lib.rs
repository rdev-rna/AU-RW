//! Synthetic deterministic-input test arena. No native gameplay constants.
pub mod skin;
use glam::{Vec2, Vec3};

pub const DT: f32 = 1.0 / 60.0;
pub const RADIUS: f32 = 0.28;
pub const BLOCKS: [(f32, f32, f32); 4] = [
    (-4.2, -3.0, 0.8),
    (4.4, 2.6, 1.2),
    (0.0, -6.0, 0.4),
    (6.0, -4.2, 0.6),
];

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub direction: Vec2,
    pub run: bool,
    pub jump: bool,
    pub reset: bool,
    pub traversal: Traversal,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Traversal {
    #[default]
    Neutral,
    Up,
    Down,
}

impl Traversal {
    pub fn from_buttons(high_profile: bool, up: bool, down: bool) -> Self {
        if !high_profile {
            Self::Neutral
        } else if down {
            Self::Down
        } else if up {
            Self::Up
        } else {
            Self::Neutral
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Player {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub distance: f32,
    pub grounded: bool,
    pub tick: u64,
}

fn blocked(p: Vec3) -> bool {
    if p.x.abs() > 11.5 || p.z.abs() > 11.5 {
        return true;
    }
    BLOCKS.iter().any(|&(x, z, size)| {
        if p.y >= size {
            return false;
        }
        let closest = Vec2::new(
            p.x.clamp(x - size / 2.0, x + size / 2.0),
            p.z.clamp(z - size / 2.0, z + size / 2.0),
        );
        (Vec2::new(p.x, p.z) - closest).length_squared() < RADIUS * RADIUS
    })
}

impl Player {
    pub fn step(&mut self, input: Input) {
        self.step_internal(input, if input.run { 4.5 } else { 1.8 }, false, None);
    }

    /// Clip-derived root speed in the presentation arena; collision remains authored.
    pub fn step_with_speed(&mut self, input: Input, speed: f32) {
        self.step_internal(input, speed, true, None);
    }
    /// World X/Z displacement from selected animation, with independently steered facing.
    pub fn step_with_root(&mut self, input: Input, displacement: Vec2, yaw: f32) {
        assert!(displacement.is_finite() && yaw.is_finite());
        self.step_internal(input, 0.0, true, Some(displacement / DT));
        self.yaw = yaw;
    }
    fn step_internal(&mut self, input: Input, speed: f32, clip_root: bool, root: Option<Vec2>) {
        if input.reset {
            *self = Self::default();
        }
        self.tick += 1;
        let d = input.direction.clamp_length_max(1.0);
        let target = root.unwrap_or(d * speed.max(0.0));
        let old = Vec2::new(self.velocity.x, self.velocity.z);
        let acceleration = if d.length_squared() > 0.0 { 12.0 } else { 18.0 };
        let velocity = if clip_root {
            target
        } else {
            old.move_towards(target, acceleration * DT)
        };
        self.velocity.x = velocity.x;
        self.velocity.z = velocity.y;
        if self.position.y <= 0.0 {
            self.grounded = true;
        }
        // Synthetic upward route: launch before an obstacle instead of guessing
        // native climb/root-motion semantics. Down never selects this launch.
        let ahead = self.position + Vec3::new(d.x, 0.0, d.y) * 0.5;
        let upward_route = input.run
            && input.traversal == Traversal::Up
            && d.length_squared() > 0.01
            && blocked(ahead);
        if (input.jump || upward_route) && input.traversal != Traversal::Down && self.grounded {
            self.velocity.y = 5.5;
            self.grounded = false;
        }
        self.velocity.y -= 18.0 * DT;
        let before = self.position;
        let mut p = self.position;
        p.x += self.velocity.x * DT;
        if blocked(p) {
            self.velocity.x = 0.0;
        } else {
            self.position.x = p.x;
        }
        p = self.position;
        p.z += self.velocity.z * DT;
        if blocked(p) {
            self.velocity.z = 0.0;
        } else {
            self.position.z = p.z;
        }
        let next_y = self.position.y + self.velocity.y * DT;
        let mut floor = 0.0f32;
        if self.velocity.y <= 0.0 {
            for &(x, z, size) in &BLOCKS {
                let closest = Vec2::new(
                    self.position.x.clamp(x - size / 2.0, x + size / 2.0),
                    self.position.z.clamp(z - size / 2.0, z + size / 2.0),
                );
                if before.y >= size
                    && next_y <= size
                    && (Vec2::new(self.position.x, self.position.z) - closest).length_squared()
                        < RADIUS * RADIUS
                {
                    floor = floor.max(size);
                }
            }
        }
        self.position.y = next_y.max(floor);
        self.grounded = next_y <= floor;
        if self.grounded {
            self.velocity.y = 0.0;
            self.grounded = true;
        }
        let travel = Vec2::new(self.position.x - before.x, self.position.z - before.z).length();
        self.distance += travel;
        if velocity.length_squared() > 0.01 {
            // The inspected face points toward presentation +Z at yaw zero.
            // This defines the test controller's forward, not native game axes.
            self.yaw = velocity.x.atan2(velocity.y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lateral_root_displacement_preserves_independent_facing_and_collision() {
        let mut p = Player::default();
        for _ in 0..30 {
            p.step_with_root(
                Input {
                    direction: Vec2::X,
                    ..Default::default()
                },
                Vec2::X * 0.02,
                0.0,
            );
        }
        assert!((p.position.x - 0.6).abs() < 1e-5);
        assert_eq!(p.yaw, 0.0);
        p.position = Vec3::new(11.49, 0.0, 0.0);
        p.step_with_root(Input::default(), Vec2::X * 0.1, 0.3);
        assert!((p.position.x - 11.49).abs() < 1e-5);
        assert_eq!(p.yaw, 0.3);
    }
    #[test]
    fn held_profile_routes_up_and_down_without_latching() {
        assert_eq!(
            Traversal::from_buttons(false, true, false),
            Traversal::Neutral
        );
        assert_eq!(Traversal::from_buttons(true, true, false), Traversal::Up);
        assert_eq!(Traversal::from_buttons(true, false, true), Traversal::Down);
        assert_eq!(Traversal::from_buttons(true, true, true), Traversal::Down);
        assert_eq!(
            Traversal::from_buttons(false, false, false),
            Traversal::Neutral
        );
        let start = Player {
            position: Vec3::new(0.0, 0.0, -5.1),
            grounded: true,
            ..Default::default()
        };
        let mut up = start;
        let mut down = start;
        for _ in 0..24 {
            up.step(Input {
                direction: -Vec2::Y,
                run: true,
                traversal: Traversal::Up,
                ..Default::default()
            });
            down.step(Input {
                direction: -Vec2::Y,
                run: true,
                traversal: Traversal::Down,
                ..Default::default()
            });
        }
        assert!(up.position.y > 0.4 && up.position.z < -5.8);
        assert_eq!(down.position.y, 0.0);
        assert!(down.position.z > -5.53);
        let mut descending = Player {
            position: Vec3::new(0.0, 0.4, -6.0),
            grounded: true,
            ..Default::default()
        };
        for _ in 0..45 {
            descending.step(Input {
                direction: Vec2::X,
                run: true,
                traversal: Traversal::Down,
                ..Default::default()
            });
        }
        assert_eq!(descending.position.y, 0.0);
    }
    #[test]
    fn replay_walk_stop_and_reset() {
        let mut a = Player::default();
        let mut b = a;
        for _ in 0..120 {
            let i = Input {
                direction: Vec2::X,
                ..Default::default()
            };
            a.step(i);
            b.step(i);
        }
        assert_eq!(a.position, b.position);
        // 8-tick acceleration ramp: .2+.4+...+1.6 then 112 ticks at 1.8.
        assert!((a.position.x - 3.48).abs() < 1e-4);
        assert!((a.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        for _ in 0..20 {
            a.step(Input::default());
        }
        assert_eq!(a.velocity, Vec3::ZERO);
        a.step(Input {
            reset: true,
            ..Default::default()
        });
        assert_eq!(a.position, Vec3::ZERO);
    }
    #[test]
    fn jump_lands_and_circle_stops_at_block() {
        let mut p = Player::default();
        p.step(Input {
            jump: true,
            ..Default::default()
        });
        assert!(p.position.y > 0.0 && !p.grounded);
        for _ in 0..60 {
            p.step(Input::default());
        }
        assert!(p.grounded && p.position.y == 0.0);
        for _ in 0..300 {
            p.step(Input {
                direction: -Vec2::Y,
                run: true,
                ..Default::default()
            });
        }
        assert!(p.position.z >= -5.52 && p.position.z < -5.4);
        assert!(!blocked(p.position));
        let mut top = Player {
            position: Vec3::new(0.0, 0.4, -6.0),
            grounded: true,
            ..Default::default()
        };
        top.step(Input::default());
        assert_eq!(top.position.y, 0.4);
        assert!(top.grounded);
        for _ in 0..80 {
            top.step(Input {
                direction: Vec2::X,
                ..Default::default()
            });
        }
        assert_eq!(top.position.y, 0.0);
        assert!(top.grounded);
    }
}
