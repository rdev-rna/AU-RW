//! Authored traversal over a decoded collision block (see `block.rs`).
//!
//! Synthetic test rules, not recovered native behaviour: the body is three spheres, wall
//! climbing is a free vertical slide, and a mantle is a timed interpolation onto the ledge
//! top. What is taken from findings: the ground support rule (ACU-RE-0050), ledge edges
//! (ACU-RE-0055 recipe) and fall acceleration (−9.55 m/s², ACU-RE-0059). Native reach,
//! candidate selection (types, windows), climb clips and root-motion warping are not used.

use crate::block::{Block, LedgeEdge};
use crate::{DT, Input, Player, RADIUS, Traversal};
use glam::{Vec2, Vec3};

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Mode {
    #[default]
    Ground,
    Air,
    /// On a wall; `normal` is the horizontal unit normal pointing away from the wall.
    Climb {
        normal: Vec2,
    },
    /// Pulling up over a ledge, from `from` to `to` over `duration` seconds.
    Mantle {
        from: Vec3,
        to: Vec3,
        t: f32,
        duration: f32,
    },
}

/// Authored arena constants for block traversal (all metres / seconds).
#[derive(Debug, Clone, Copy)]
pub struct BlockTuning {
    /// Highest rise walked onto without traversal.
    pub step: f32,
    /// Sphere centres above the feet that make up the body.
    pub body: [f32; 3],
    /// Fall acceleration, ACU-RE-0059 (median of observed fall cycles).
    pub gravity: f32,
    pub jump_speed: f32,
    pub climb_up: f32,
    pub climb_side: f32,
    /// Ledge height above the feet accepted for a mantle from the ground.
    pub mantle_from_ground: (f32, f32),
    /// Ledge height above the feet at which a climb tops out.
    pub top_out: f32,
    /// Horizontal reach from the body surface to a ledge.
    pub reach: f32,
    /// How far behind the ledge lip the body lands.
    pub landing_inset: f32,
}

impl Default for BlockTuning {
    fn default() -> Self {
        Self {
            step: 0.45,
            body: [0.45 + RADIUS, 1.12, 1.80 - RADIUS],
            gravity: 9.55,
            jump_speed: 4.0,
            climb_up: 1.4,
            climb_side: 0.9,
            mantle_from_ground: (0.5, 2.4),
            top_out: 1.9,
            reach: 0.7,
            landing_inset: 0.45,
        }
    }
}

fn flat(v: Vec2) -> Vec3 {
    Vec3::new(v.x, 0.0, v.y)
}

fn xz(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

impl Player {
    fn body_blocked(block: &Block, tuning: &BlockTuning, feet: Vec3) -> bool {
        tuning
            .body
            .iter()
            .any(|&h| block.sphere_contact(feet + Vec3::Y * h, RADIUS).is_some())
    }

    /// Where the body would stand on top of `edge`, if there is room.
    fn ledge_landing(
        block: &Block,
        tuning: &BlockTuning,
        edge: &LedgeEdge,
        near: Vec3,
    ) -> Option<Vec3> {
        let (lip, _) = edge.closest(near);
        let spot = lip - flat(edge.outward) * tuning.landing_inset;
        let top = block.support(spot.x, spot.z, lip.y + 0.1, tuning.step)?;
        if (top - lip.y).abs() > 0.3 {
            return None;
        }
        let feet = Vec3::new(spot.x, top, spot.z);
        (!Self::body_blocked(block, tuning, feet)).then_some(feet)
    }

    /// Best ledge in front of the body (facing `-wall_normal`) within the height band.
    fn find_ledge(
        &self,
        block: &Block,
        tuning: &BlockTuning,
        wall_normal: Vec2,
        band: (f32, f32),
    ) -> Option<Vec3> {
        let p = self.position;
        let mut best: Option<(f32, Vec3)> = None;
        for edge in block.ledges_near(p + Vec3::Y * band.1 * 0.5, tuning.reach + RADIUS + band.1) {
            if edge.outward.dot(wall_normal) < 0.7 {
                continue;
            }
            let (lip, _) = edge.closest(p);
            let rise = lip.y - p.y;
            if rise < band.0 || rise > band.1 {
                continue;
            }
            let horizontal = (xz(lip) - xz(p)).length();
            if horizontal > RADIUS + tuning.reach {
                continue;
            }
            let Some(landing) = Self::ledge_landing(block, tuning, edge, p) else {
                continue;
            };
            let score = rise + horizontal; // lowest, closest lip first (authored)
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, landing));
            }
        }
        best.map(|(_, l)| l)
    }

    fn start_mantle(&mut self, to: Vec3) {
        let rise = (to.y - self.position.y).max(0.0);
        self.mode = Mode::Mantle {
            from: self.position,
            to,
            t: 0.0,
            duration: 0.35 + 0.2 * rise,
        };
        self.velocity = Vec3::ZERO;
        self.grounded = false;
    }

    /// Wall in front of the chest along `dir`: (distance, horizontal outward normal).
    fn wall_ahead(&self, block: &Block, dir: Vec2, max: f32) -> Option<(f32, Vec2)> {
        let origin = self.position + Vec3::Y * 1.1;
        let (d, n) = block.ray(origin, flat(dir.normalize_or_zero()), max)?;
        if n.y.abs() > 0.5 {
            return None; // not a wall
        }
        Some((d, xz(n).normalize_or_zero()))
    }

    /// One fixed tick on a collision block. `displacement` is the selected clip's world
    /// X/Z root motion for this tick (used on the ground only); `yaw` its facing.
    /// Returns true when the body has fallen out of the block.
    pub fn step_in_block(
        &mut self,
        block: &Block,
        tuning: &BlockTuning,
        input: Input,
        displacement: Vec2,
        yaw: f32,
    ) -> bool {
        assert!(displacement.is_finite() && yaw.is_finite());
        if input.reset {
            *self = Self::default();
        }
        self.tick += 1;
        let before = self.position;
        let d = input.direction.clamp_length_max(1.0);
        match self.mode {
            Mode::Mantle {
                from,
                to,
                t,
                duration,
            } => {
                let t = t + DT;
                let s = (t / duration).min(1.0);
                let smooth = |x: f32| {
                    let x = x.clamp(0.0, 1.0);
                    x * x * (3.0 - 2.0 * x)
                };
                let up = smooth(s / 0.6);
                let across = smooth((s - 0.3) / 0.7);
                self.position = Vec3::new(
                    from.x + (to.x - from.x) * across,
                    from.y + (to.y - from.y) * up,
                    from.z + (to.z - from.z) * across,
                );
                self.mode = if s >= 1.0 {
                    self.position = to;
                    self.grounded = true;
                    Mode::Ground
                } else {
                    Mode::Mantle {
                        from,
                        to,
                        t,
                        duration,
                    }
                };
            }
            Mode::Climb { normal } => self.climb(block, tuning, input, d, normal),
            Mode::Ground | Mode::Air => self.walk(block, tuning, input, d, displacement, yaw),
        }
        self.distance += (xz(self.position) - xz(before)).length();
        // Below the block: the caller respawns rather than this inventing a kill plane.
        self.position.y < block.min_bound.y - 5.0
    }

    fn climb(&mut self, block: &Block, tuning: &BlockTuning, input: Input, d: Vec2, normal: Vec2) {
        self.yaw = (-normal.x).atan2(-normal.y);
        self.velocity = Vec3::ZERO;
        if input.traversal == Traversal::Down {
            // Let go: push off the wall and fall.
            self.mode = Mode::Air;
            self.velocity = flat(normal) * 1.5;
            return;
        }
        if let Some(to) = self.find_ledge(block, tuning, normal, (0.3, tuning.top_out)) {
            self.start_mantle(to);
            return;
        }
        let tangent = Vec2::new(-normal.y, normal.x);
        let up = d.dot(-normal);
        let side = d.dot(tangent);
        let rate = if input.run { 1.0 } else { 0.0 };
        let mut next = self.position
            + Vec3::Y * up * tuning.climb_up * DT * rate
            + flat(tangent) * side * tuning.climb_side * DT * rate;
        // Reached the ground going down: stand.
        if up < 0.0
            && let Some(floor) = block.support(next.x, next.z, next.y, 0.05)
            && next.y <= floor + 0.02
        {
            self.position = Vec3::new(next.x, floor, next.z);
            self.mode = Mode::Ground;
            self.grounded = true;
            return;
        }
        let probe = Player {
            position: next,
            ..*self
        };
        match probe.wall_ahead(block, -normal, RADIUS + 0.6) {
            Some((distance, n)) => {
                let hit = xz(next) + (-normal) * distance;
                let stand = hit + n * (RADIUS + 0.05);
                next.x = stand.x;
                next.z = stand.y;
                if Self::body_blocked(block, tuning, next) && up > 0.0 {
                    next = self.position; // overhang: stay put
                }
                self.position = next;
                self.mode = Mode::Climb { normal: n };
            }
            None => {
                self.mode = Mode::Air; // the wall ended without a ledge in reach
            }
        }
    }

    fn walk(
        &mut self,
        block: &Block,
        tuning: &BlockTuning,
        input: Input,
        d: Vec2,
        displacement: Vec2,
        yaw: f32,
    ) {
        let grounded = self.mode == Mode::Ground;
        if grounded {
            let v = displacement / DT;
            self.velocity.x = v.x;
            self.velocity.z = v.y;
            self.yaw = yaw;
            // Upward traversal: RT + up into a wall mantles a ledge in reach or starts a climb.
            if input.run && input.traversal == Traversal::Up && d.length_squared() > 0.01 {
                let dir = d.normalize();
                if let Some((distance, n)) = self.wall_ahead(block, dir, RADIUS + 0.6)
                    && dir.dot(-n) > 0.5
                {
                    if let Some(to) = self.find_ledge(block, tuning, n, tuning.mantle_from_ground) {
                        self.start_mantle(to);
                        return;
                    }
                    let wall = xz(self.position) + dir * distance;
                    let stand = wall + n * (RADIUS + 0.05);
                    self.position.x = stand.x;
                    self.position.z = stand.y;
                    self.mode = Mode::Climb { normal: n };
                    self.velocity = Vec3::ZERO;
                    self.grounded = false;
                    return;
                }
            }
            if input.jump && input.traversal != Traversal::Down {
                self.velocity.y = tuning.jump_speed;
                self.mode = Mode::Air;
            }
        }
        if self.mode == Mode::Air {
            self.velocity.y -= tuning.gravity * DT;
        }
        // Horizontal move with axis sliding against walls.
        let mut p = self.position;
        let full = p + Vec3::new(self.velocity.x, 0.0, self.velocity.z) * DT;
        if !Self::body_blocked(block, tuning, full) {
            p = full;
        } else {
            let x_only = p + Vec3::X * self.velocity.x * DT;
            if !Self::body_blocked(block, tuning, x_only) {
                p = x_only;
            } else {
                self.velocity.x = 0.0;
            }
            let z_only = p + Vec3::Z * self.velocity.z * DT;
            if !Self::body_blocked(block, tuning, z_only) {
                p = z_only;
            } else {
                self.velocity.z = 0.0;
            }
        }
        // Vertical: settle on the ACU-RE-0050 support, stepping up/down by `step` on foot.
        if self.mode == Mode::Ground {
            match block.support(p.x, p.z, p.y, tuning.step) {
                Some(floor) if floor >= p.y - tuning.step => {
                    p.y = floor;
                    self.velocity.y = 0.0;
                }
                _ => {
                    self.mode = Mode::Air; // walked off an edge
                    self.velocity.y = 0.0;
                }
            }
        } else {
            let next_y = p.y + self.velocity.y * DT;
            let floor = block.support(p.x, p.z, p.y, 0.05);
            match floor {
                Some(floor) if self.velocity.y <= 0.0 && next_y <= floor => {
                    p.y = floor;
                    self.velocity = Vec3::ZERO;
                    self.mode = Mode::Ground;
                }
                _ => p.y = next_y,
            }
        }
        self.grounded = self.mode == Mode::Ground;
        self.position = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::tests::box_block;

    /// Hold RT + stick (and `traversal`) until a mantle completes or `ticks` run out.
    fn run_into(
        block: &Block,
        start: Vec3,
        dir: Vec2,
        traversal: Traversal,
        ticks: u32,
    ) -> (Player, bool) {
        let tuning = BlockTuning::default();
        let mut p = Player {
            position: start,
            ..Default::default()
        };
        let mut climbed = false;
        for _ in 0..ticks {
            let input = Input {
                direction: dir,
                run: true,
                traversal,
                ..Default::default()
            };
            let was_mantle = matches!(p.mode, Mode::Mantle { .. });
            p.step_in_block(block, &tuning, input, dir * 3.0 * DT, dir.x.atan2(dir.y));
            climbed |= matches!(p.mode, Mode::Climb { .. });
            if was_mantle && p.mode == Mode::Ground {
                break;
            }
        }
        (p, climbed)
    }

    #[test]
    fn walks_settles_and_is_stopped_by_a_wall() {
        // Synthetic 2 m box whose near face is at z = -2.
        let b = box_block(Vec3::new(0.0, 1.0, -3.0), Vec3::new(1.0, 1.0, 1.0));
        let (p, _) = run_into(
            &b,
            Vec3::new(0.0, 0.3, 0.0),
            -Vec2::Y,
            Traversal::Neutral,
            120,
        );
        assert_eq!(p.mode, Mode::Ground);
        assert_eq!(p.position.y, 0.0);
        assert!(
            p.position.z > -2.0 + RADIUS - 0.01 && p.position.z < -2.0 + RADIUS + 0.1,
            "{p:?}"
        );
    }

    #[test]
    fn up_mantles_a_reachable_ledge_and_down_does_not() {
        let b = box_block(Vec3::new(0.0, 0.75, -3.0), Vec3::new(1.0, 0.75, 1.0)); // 1.5 m
        let (up, climbed) = run_into(&b, Vec3::ZERO, -Vec2::Y, Traversal::Up, 150);
        assert!(!climbed, "a 1.5 m ledge is mantled from the ground");
        assert_eq!(up.mode, Mode::Ground);
        assert!((up.position.y - 1.5).abs() < 1e-4, "{up:?}");
        let (down, _) = run_into(&b, Vec3::ZERO, -Vec2::Y, Traversal::Down, 150);
        assert_eq!(down.position.y, 0.0);
    }

    #[test]
    fn up_climbs_a_tall_wall_then_tops_out() {
        let b = box_block(Vec3::new(0.0, 3.0, -3.0), Vec3::new(1.0, 3.0, 1.0)); // 6 m
        let (p, climbed) = run_into(&b, Vec3::ZERO, -Vec2::Y, Traversal::Up, 600);
        assert!(climbed);
        assert_eq!(p.mode, Mode::Ground, "{p:?}");
        assert!((p.position.y - 6.0).abs() < 1e-4, "{p:?}");
        assert!(
            p.position.z < -2.0 && p.position.z > -4.0,
            "landed on the top: {p:?}"
        );
        let (q, _) = run_into(&b, Vec3::ZERO, -Vec2::Y, Traversal::Up, 600);
        assert_eq!(p.position, q.position, "deterministic replay");
        assert_eq!(p.tick, q.tick);
    }

    #[test]
    fn letting_go_falls_with_measured_gravity_and_lands() {
        let b = box_block(Vec3::new(0.0, 3.0, -3.0), Vec3::new(1.0, 3.0, 1.0));
        let tuning = BlockTuning::default();
        let mut p = Player {
            position: Vec3::new(0.0, 3.0, -2.0 + RADIUS + 0.05),
            mode: Mode::Climb { normal: Vec2::Y },
            ..Default::default()
        };
        p.step_in_block(
            &b,
            &tuning,
            Input {
                traversal: Traversal::Down,
                ..Default::default()
            },
            Vec2::ZERO,
            0.0,
        );
        assert_eq!(p.mode, Mode::Air);
        for _ in 0..120 {
            p.step_in_block(&b, &tuning, Input::default(), Vec2::ZERO, 0.0);
        }
        assert_eq!(p.mode, Mode::Ground);
        assert_eq!(p.position.y, 0.0);
        assert!(p.position.z > -2.0 + RADIUS);
    }

    #[test]
    fn walking_off_a_roof_falls_to_the_floor() {
        let b = box_block(Vec3::new(0.0, 1.0, -3.0), Vec3::new(1.0, 1.0, 1.0));
        let (p, _) = run_into(
            &b,
            Vec3::new(0.0, 2.0, -3.0),
            Vec2::Y,
            Traversal::Neutral,
            120,
        );
        assert_eq!(p.mode, Mode::Ground);
        assert_eq!(p.position.y, 0.0);
        assert!(p.position.z > -1.0);
    }
}

/// Install-dependent: needs a block exported from the original archives
/// (`tools/world_collision.py block`), named by `ACU_BLOCK`. Missing data is reported as
/// unavailable, never as a pass.
#[cfg(test)]
mod block_file {
    use super::*;

    fn load() -> Option<Block> {
        let Some(path) = std::env::var_os("ACU_BLOCK") else {
            eprintln!("unavailable: set ACU_BLOCK to an ACUBLK01 file");
            return None;
        };
        Some(Block::parse(&std::fs::read(path).expect("read ACU_BLOCK")).expect("parse ACU_BLOCK"))
    }

    /// The exporter's Python column query (over the whole world's instances) against this
    /// crate's grid query over the exported triangles, on the exporter's sample grid.
    #[test]
    #[ignore = "requires a block exported from the original install (ACU_BLOCK)"]
    fn ground_matches_exporter_samples() {
        let Some(block) = load() else { return };
        let path = std::path::PathBuf::from(std::env::var_os("ACU_BLOCK").unwrap())
            .with_extension("samples.txt");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("unavailable: {} (re-export the block)", path.display());
            return;
        };
        let (mut compared, mut worst) = (0, 0.0f32);
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            let (x, y): (f32, f32) = (f[0].parse().unwrap(), f[1].parse().unwrap());
            // native (x, y) -> arena (x, -y); heights are native z = arena y.
            let rust = block.support(x, -y, 0.0, 0.5);
            match f[2] {
                "none" => assert_eq!(rust, None, "at {x} {y}"),
                z => {
                    let z: f32 = z.parse().unwrap();
                    let rust = rust.unwrap_or_else(|| panic!("no support at {x} {y}, Python {z}"));
                    worst = worst.max((rust - z).abs());
                    compared += 1;
                }
            }
        }
        println!("ground samples compared {compared}, worst |dz| {worst:.5} m");
        assert!(compared > 0 && worst < 1e-3, "worst {worst}");
    }

    /// The `block-climb` arena scenario from spawn (12, 12) on the Paris arrival block
    /// (`local/blocks/paris-arrival-40m.bin`, SHA-256 802bdf45…27d8f6): RT + up held
    /// toward -Z mantles, climbs a facade and tops out, the same way on every run.
    #[test]
    #[ignore = "requires a block exported from the original install (ACU_BLOCK)"]
    fn paris_arrival_block_climb_replays() {
        let Some(block) = load() else { return };
        let tuning = BlockTuning::default();
        let run = || {
            let y = block.support(12.0, 12.0, 0.0, 0.5).expect("spawn support");
            let mut p = Player {
                position: Vec3::new(12.0, y, 12.0),
                mode: Mode::Air,
                ..Default::default()
            };
            let (mut climb, mut mantles, mut top) = (0, 0, f32::MIN);
            for _ in 0..300 {
                let input = Input {
                    direction: -Vec2::Y,
                    run: true,
                    traversal: Traversal::Up,
                    ..Default::default()
                };
                let was = p.mode;
                assert!(!p.step_in_block(
                    &block,
                    &tuning,
                    input,
                    -Vec2::Y * 3.0 * DT,
                    std::f32::consts::PI
                ));
                climb += u32::from(matches!(p.mode, Mode::Climb { .. }));
                if matches!(was, Mode::Mantle { .. }) && p.mode == Mode::Ground {
                    mantles += 1;
                    top = top.max(p.position.y);
                }
            }
            (p.position, climb, mantles, top)
        };
        let (a, climb, mantles, top) = run();
        println!("end {a:?}, climb ticks {climb}, mantles {mantles}, highest landing {top:.3} m");
        assert!(climb > 20 && mantles >= 2 && top > 4.0);
        assert_eq!(run().0, a, "deterministic replay");
    }

    #[test]
    #[ignore = "requires a block exported from the original install (ACU_BLOCK)"]
    fn survey_climbs_from_spawn() {
        let Some(block) = load() else { return };
        let tuning = BlockTuning::default();
        for sx in (-16..=16).step_by(4) {
            for sz in (-16..=16).step_by(4) {
                let (x, z) = (sx as f32, sz as f32);
                let Some(y) = block.support(x, z, 0.0, 0.5) else {
                    continue;
                };
                let start = Vec3::new(x, y, z);
                for k in 0..8 {
                    let a = k as f32 * std::f32::consts::FRAC_PI_4;
                    let dir = Vec2::new(a.sin(), -a.cos());
                    let mut p = Player {
                        position: start,
                        mode: Mode::Air,
                        ..Default::default()
                    };
                    let mut climbed = 0u32;
                    let mut mantles = 0;
                    for _ in 0..900 {
                        let input = Input {
                            direction: dir,
                            run: true,
                            traversal: Traversal::Up,
                            ..Default::default()
                        };
                        let was = p.mode;
                        if p.step_in_block(
                            &block,
                            &tuning,
                            input,
                            dir * 3.0 * DT,
                            dir.x.atan2(dir.y),
                        ) {
                            break;
                        }
                        climbed += u32::from(matches!(p.mode, Mode::Climb { .. }));
                        if matches!(was, Mode::Mantle { .. }) && p.mode == Mode::Ground {
                            mantles += 1;
                            if climbed > 60 {
                                break;
                            }
                        }
                    }
                    if climbed > 60 && mantles > 0 {
                        println!(
                            "CLIMB spawn ({x}, {y:.2}, {z}) dir {k} ({:.2},{:.2}) climb ticks {climbed} -> ({:.2}, {:.2}, {:.2}) {:?}",
                            dir.x, dir.y, p.position.x, p.position.y, p.position.z, p.mode
                        );
                    }
                }
            }
        }
    }
}
