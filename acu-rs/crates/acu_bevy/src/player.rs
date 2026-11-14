//! Test movement and authored pose driver; docs/mechanics/test-world-movement.md.
use crate::{InspectionCamera, controls::Controller, options::Options, rig::Skin};
use acu_motion::graph_driver::{Controls, GraphDriver};
use acu_sim::block::Block;
use acu_sim::{BlockTuning, Input, Mode, Player, Traversal};
use bevy::prelude::*;
use std::sync::Arc;

#[derive(Component)]
pub struct Character;

/// The character's native right (+X) and forward (+Y) as arena (x, z) directions at `yaw`.
/// Follows the presentation rotation `rotation_y(yaw) * native_z_up_to_bevy_y_up()` (ADR-0003;
/// the arena itself is synthetic and Y-up).
fn actor_axes(yaw: f32) -> (Vec2, Vec2) {
    let (s, c) = yaw.sin_cos();
    (Vec2::new(c, -s), Vec2::new(-s, -c))
}

/// Yaw of a native rotation about +Z (left turn positive), radians.
fn native_yaw(q: Quat) -> f32 {
    acu_motion::amo::wrap_angle(2.0 * q.z.atan2(q.w))
}

/// The decoded collision block the player traverses, when the arena uses one.
pub struct ArenaBlock {
    pub block: Arc<Block>,
    pub tuning: BlockTuning,
    pub spawn: Vec3,
}

/// One fixed tick. Returns the root-motion displacement requested from the body (before the
/// arena's authored collision).
fn advance(
    player: &mut Player,
    driver: &mut GraphDriver,
    input: Input,
    block: Option<&ArenaBlock>,
) -> Vec2 {
    if input.reset {
        driver.reset();
        *player = spawn_player(block);
    }
    // Climbing and mantling have no graph wired yet: the locomotion graph idles while the
    // authored traversal moves the body (presentation gap, stated).
    let traversing = matches!(player.mode, Mode::Climb { .. } | Mode::Mantle { .. });
    let (right, forward) = actor_axes(player.yaw);
    let d = input.direction.clamp_length_max(1.0);
    let stick = if traversing {
        Vec2::ZERO
    } else {
        Vec2::new(d.dot(right), d.dot(forward))
    };
    let controls = Controls {
        stick,
        free_run: input.run,
    };
    driver.apply(controls);
    let delta = driver.step(acu_sim::DT);
    let steer = acu_motion::graph_driver::walk_turn(&controls, driver.in_loop(), acu_sim::DT);
    // ACU-RE-0057: the graph's root delta drives the body. Its translation is in the actor
    // frame at the step's start; the body orientation composes with its rotation. Only the
    // yaw (native Z) part is applied in this flat arena.
    // While walking, native also turns the body toward the stick at a measured rate
    // (ACU-RE-0069 obs. 13; `walk_turn`).
    let (displacement, yaw) = if traversing {
        (Vec2::ZERO, player.yaw)
    } else {
        (
            right * delta.translation.x + forward * delta.translation.y,
            acu_motion::amo::wrap_angle(player.yaw + native_yaw(delta.rotation) + steer),
        )
    };
    match block {
        Some(arena) => {
            let input = Input {
                reset: false,
                ..input
            };
            if player.step_in_block(&arena.block, &arena.tuning, input, displacement, yaw) {
                *player = Player {
                    tick: player.tick,
                    ..spawn_player(Some(arena))
                };
            }
        }
        None => player.step_with_root(input, displacement, yaw),
    }
    displacement
}

/// Trace entry for the dominant graph leaf.
fn leaf_json(driver: &GraphDriver) -> serde_json::Value {
    match driver.dominant() {
        Some(l) => serde_json::json!({"clip":format!("{:016x}",l.clip),"name":driver.name(l.clip),
            "time":l.time,"weight":l.weight,"mirror":l.mirror,"leaves":driver.player.leaves.len()}),
        None => serde_json::Value::Null,
    }
}

fn spawn_player(block: Option<&ArenaBlock>) -> Player {
    Player {
        position: block.map(|b| b.spawn).unwrap_or(Vec3::ZERO),
        mode: if block.is_some() {
            Mode::Air
        } else {
            Mode::Ground
        },
        ..Player::default()
    }
}

#[derive(Resource)]
pub struct State {
    pub driver: GraphDriver,
    pub motion_report: serde_json::Value,
    pub motion_trace: Vec<serde_json::Value>,
    pub player: Player,
    previous: Player,
    pub skins: Vec<Skin>,
    pub block: Option<ArenaBlock>,
    pub bind: bool,
    pub deform: bool,
    pub frozen: bool,
    pub capture: bool,
    jump_requested: bool,
    frames: u32,
    skin_seconds: f64,
    reversal_review: bool,
    review_tick: u32,
    review_trace_done: bool,
}

pub fn replay(scenario: &str, tick: u32) -> Input {
    let direction = match scenario {
        "reversals" => match tick {
            60..=1019 if ((tick - 60) / 120).is_multiple_of(2) => Vec2::X,
            60..=1019 => -Vec2::X,
            _ => Vec2::ZERO,
        },
        "walk" | "run" | "jump" => Vec2::new(0.0, -1.0),
        "walk-left" | "run-left" => {
            if tick < 90 {
                -Vec2::Y
            } else {
                -Vec2::X
            }
        }
        "walk-right" | "run-right" => {
            if tick < 90 {
                -Vec2::Y
            } else {
                Vec2::X
            }
        }
        "walk-back" | "run-back" => {
            if tick < 90 {
                -Vec2::Y
            } else {
                Vec2::Y
            }
        }
        "walk-circle" | "run-circle" => {
            let angle = tick.saturating_sub(90) as f32 * 360.0_f32.to_radians() * acu_sim::DT;
            Vec2::new(angle.sin(), -angle.cos())
        }
        "turn" => {
            if tick < 90 {
                Vec2::new(0.0, -1.0)
            } else {
                Vec2::X
            }
        }
        "turn-arc" => {
            // Run 37's live 60 Hz input sequence: neutral, forward, eight
            // 300 ms arc steps, a right hold, then the reverse arc. Native
            // XInput uses +Y for forward; this test world uses -Y.
            const ARC: [(i16, i16); 8] = [
                (2731, 13731),
                (5358, 12934),
                (7778, 11641),
                (9899, 9899),
                (11641, 7778),
                (12934, 5358),
                (13731, 2731),
                (14000, 0),
            ];
            const RETURN: [(i16, i16); 8] = [
                (13731, 2731),
                (12934, 5358),
                (11641, 7778),
                (9899, 9899),
                (7778, 11641),
                (5358, 12934),
                (2731, 13731),
                (0, 14000),
            ];
            const AMP: f32 = 1.0 / 32767.0;
            let (x, y) = match tick {
                0..=71 | 486.. => (0, 0),
                72..=161 => (0, 14000),
                162..=305 => ARC[((tick - 162) / 18) as usize],
                306..=341 => (14000, 0),
                342..=485 => RETURN[((tick - 342) / 18) as usize],
            };
            Vec2::new(x as f32 * AMP, -(y as f32) * AMP)
        }
        "contact" | "profile-up" | "profile-down" | "block-climb" => Vec2::new(0.0, -1.0),
        _ => Vec2::ZERO,
    };
    Input {
        direction,
        run: scenario.starts_with("run")
            || matches!(
                scenario,
                "contact" | "profile-up" | "profile-down" | "block-climb"
            ),
        jump: scenario == "jump" && tick == 90,
        reset: false,
        traversal: match scenario {
            "profile-up" | "block-climb" => Traversal::Up,
            "profile-down" => Traversal::Down,
            _ => Traversal::Neutral,
        },
    }
}

impl State {
    pub fn new(
        options: &Options,
        mut driver: GraphDriver,
        motion_report: serde_json::Value,
        block: Option<ArenaBlock>,
    ) -> Self {
        let mut player = spawn_player(block.as_ref());
        let mut motion_trace = Vec::new();
        for tick in 0..options.ticks {
            let input = replay(&options.scenario, tick);
            let selected = driver.dominant().map(|l| (l.clip, l.mirror));
            let mode = player.mode;
            advance(&mut player, &mut driver, input, block.as_ref());
            if selected != driver.dominant().map(|l| (l.clip, l.mirror))
                || std::mem::discriminant(&mode) != std::mem::discriminant(&player.mode)
                || tick == 0
                || tick + 1 == options.ticks
                || options.scenario == "turn-arc"
            {
                motion_trace.push(
                    serde_json::json!({"tick":tick+1,"direction":input.direction.to_array(),
                    "yaw":player.yaw,"leaf":leaf_json(&driver),
                    "position":player.position.to_array(),"mode":format!("{:?}",player.mode)}),
                );
            }
        }
        Self {
            driver,
            motion_report,
            motion_trace,
            player,
            previous: player,
            skins: Vec::new(),
            block,
            bind: options.scenario == "bind",
            deform: options.scenario == "deform",
            frozen: false,
            capture: options.capture.is_some(),
            jump_requested: false,
            frames: 0,
            skin_seconds: 0.0,
            reversal_review: options.scenario == "reversals",
            review_tick: 0,
            review_trace_done: false,
        }
    }
}

pub fn step(
    keys: Res<ButtonInput<KeyCode>>,
    controller: Res<Controller>,
    mut state: ResMut<State>,
    cameras: Query<&InspectionCamera>,
) {
    if state.capture {
        return;
    }
    if state.reversal_review {
        if state.review_tick == 1080 {
            state.review_trace_done = true;
            state.review_tick = 0;
            state.player = spawn_player(state.block.as_ref());
            state.driver.reset();
        }
        let tick = state.review_tick;
        let input = replay("reversals", tick);
        state.previous = state.player;
        let State {
            player,
            driver,
            block,
            ..
        } = &mut *state;
        advance(player, driver, input, block.as_ref());
        if !state.review_trace_done
            && tick.is_multiple_of(6)
            && let Some(path) = std::env::var_os("ACU_REVERSAL_TRACE")
        {
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(
                    file,
                    "REVERSAL_REVIEW {}",
                    serde_json::json!({"tick":tick,"direction":input.direction.to_array(),"position":state.player.position.to_array(),"yaw":state.player.yaw,"leaf":leaf_json(&state.driver),"pose":state.driver.pose.iter().map(|m|m.w_axis.truncate().to_array()).collect::<Vec<_>>(),"parents":state.driver.rig.parents})
                );
            }
        }
        state.review_tick += 1;
        return;
    }
    let axis = |a, b| f32::from(u8::from(keys.pressed(a))) - f32::from(u8::from(keys.pressed(b)));
    let stick = (Vec2::new(
        axis(KeyCode::KeyD, KeyCode::KeyA),
        axis(KeyCode::KeyS, KeyCode::KeyW),
    ) + controller.movement)
        .clamp_length_max(1.0);
    let yaw = cameras.iter().next().map(|c| c.azimuth).unwrap_or(0.0);
    let (s, c) = yaw.sin_cos();
    let direction = Vec2::new(c * stick.x + s * stick.y, -s * stick.x + c * stick.y);
    let high_profile = controller.high_profile
        || keys.pressed(KeyCode::ShiftLeft)
        || keys.pressed(KeyCode::ShiftRight);
    let traversal = if controller.high_profile {
        controller.traversal
    } else {
        Traversal::from_buttons(
            high_profile,
            keys.pressed(KeyCode::Space),
            keys.pressed(KeyCode::KeyC),
        )
    };
    let input = Input {
        direction,
        run: high_profile,
        jump: std::mem::take(&mut state.jump_requested),
        reset: keys.pressed(KeyCode::KeyR),
        traversal,
    };
    state.previous = state.player;
    let State {
        player,
        driver,
        block,
        ..
    } = &mut *state;
    advance(player, driver, input, block.as_ref());
}

pub fn present(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time<Fixed>>,
    mut state: ResMut<State>,
    mut roots: Query<&mut Transform, With<Character>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !state.capture {
        state.jump_requested |= keys.just_pressed(KeyCode::Space);
        if keys.just_pressed(KeyCode::F2) {
            state.bind = !state.bind;
        }
        if keys.just_pressed(KeyCode::F3) {
            state.frozen = !state.frozen;
        }
    }
    let alpha = if state.capture {
        1.0
    } else {
        time.overstep_fraction()
    };
    let position = state.previous.position.lerp(state.player.position, alpha);
    for mut t in &mut roots {
        t.translation = position;
        t.rotation = Quat::from_rotation_y(state.player.yaw) * crate::native_z_up_to_bevy_y_up();
    }
    if !state.frozen {
        let start = std::time::Instant::now();
        for skin in &state.skins {
            skin.update(&mut meshes, &state.driver.pose, state.bind, state.deform);
        }
        state.skin_seconds += start.elapsed().as_secs_f64();
        state.frames += 1;
        if state.frames == 60 {
            println!(
                "CPU skin mean over 60 frames: {:.3} ms",
                state.skin_seconds * 1000.0 / 60.0
            );
        }
    }
}

pub fn follow(
    state: Res<State>,
    time: Res<Time<Fixed>>,
    mut cameras: Query<&mut InspectionCamera>,
) {
    for mut camera in &mut cameras {
        let alpha = if state.capture {
            1.0
        } else {
            time.overstep_fraction()
        };
        camera.center = state.previous.position.lerp(state.player.position, alpha) + Vec3::Y * 0.95;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_axes_follow_the_presentation_rotation() {
        for yaw in [0.0_f32, 0.7, -2.0, 3.1] {
            let q = Quat::from_rotation_y(yaw) * crate::native_z_up_to_bevy_y_up();
            let (right, forward) = actor_axes(yaw);
            let r = q * Vec3::X;
            let f = q * Vec3::Y;
            assert!((Vec2::new(r.x, r.z) - right).length() < 1e-5);
            assert!((Vec2::new(f.x, f.z) - forward).length() < 1e-5);
            assert!(r.y.abs() < 1e-5 && f.y.abs() < 1e-5);
        }
    }

    #[test]
    fn native_left_turn_increases_yaw_toward_left() {
        // ACU-RE-0057 obs. 7: rotation deltas are about native +Z; +Z turns forward toward -X.
        let q = Quat::from_rotation_z(0.1);
        assert!((native_yaw(q) - 0.1).abs() < 1e-6);
        let (right, forward) = actor_axes(0.0);
        let (_, turned) = actor_axes(native_yaw(q));
        assert!((turned - forward).dot(-right) > 0.0);
    }

    /// Hold the stick toward `direction` (arena x, z) from idle at yaw 0 for `hold` ticks, then
    /// release for `release` ticks. Returns the player, the dominant leaf names and the
    /// root-motion travel requested over the last held second (independent of arena blocks).
    fn drive(
        driver: &mut GraphDriver,
        direction: Vec2,
        run: bool,
        hold: u32,
        release: u32,
    ) -> (Player, Vec<String>, Vec3) {
        driver.reset();
        // A clear lane of the synthetic arena (no block within reach), 10 m behind the
        // travel direction so a held sprint stays inside the bounds.
        let lane = if direction.x == 0.0 {
            Vec3::new(2.0, 0.0, 0.0)
        } else {
            Vec3::ZERO
        };
        let mut player = Player {
            position: lane - Vec3::new(direction.x, 0.0, direction.y) * 10.0,
            ..Player::default()
        };
        let mut names: Vec<String> = Vec::new();
        let mut late = Vec3::ZERO;
        for tick in 0..hold + release {
            let input = Input {
                direction: if tick < hold { direction } else { Vec2::ZERO },
                run,
                ..Default::default()
            };
            let before = player.position;
            let requested = advance(&mut player, driver, input, None);
            if std::env::var_os("ACU_STEER_DEBUG").is_some() && tick % 6 == 0 {
                let leaves: Vec<String> = driver
                    .player
                    .leaves
                    .iter()
                    .filter(|l| l.weight > 0.01)
                    .map(|l| {
                        format!(
                            "{}{}@{:.2}w{:.2}a{:.3}",
                            driver.name(l.clip),
                            if l.mirror { "(m)" } else { "" },
                            l.time,
                            l.weight,
                            l.advance
                        )
                    })
                    .collect();
                println!(
                    "t{tick} pos {:?} yaw {:.3} step {:.4} | {}",
                    player.position.to_array(),
                    player.yaw,
                    (player.position - before).length(),
                    leaves.join(", ")
                );
            }
            if tick >= hold.saturating_sub(60) && tick < hold {
                late += Vec3::new(requested.x, 0.0, requested.y);
            }
            if let Some(l) = driver.dominant() {
                let n = format!(
                    "{}{}",
                    driver.name(l.clip),
                    if l.mirror { " (m)" } else { "" }
                );
                if names.last() != Some(&n) {
                    names.push(n);
                }
            }
        }
        (player, names, late)
    }

    #[test]
    #[ignore = "requires original ACU install; drives the native NewDemo graph"]
    fn installed_graph_steering() {
        let dir = std::env::var("ACU_GAME_DIR").expect("ACU_GAME_DIR required");
        let dir = std::path::Path::new(&dir);
        let library = crate::rig::load(dir).unwrap();
        let (mut driver, _) = crate::motion::load_graph(dir, &library).unwrap();
        // At yaw 0 the character faces arena -Z; its right is +X.
        let mut failures = Vec::new();
        for (label, direction) in [
            ("forward", -Vec2::Y),
            ("right", Vec2::X),
            ("left", -Vec2::X),
            ("back", Vec2::Y),
        ] {
            for run in [false, true] {
                let (player, names, late) = drive(&mut driver, direction, run, 240, 150);
                let travel = Vec2::new(late.x, late.z);
                let (_, facing) = actor_axes(player.yaw);
                let speed = Vec2::new(player.velocity.x, player.velocity.z).length();
                println!(
                    "{label} run={run}: last held second {:.3} m along {:.3}; final facing.dir {:.3}; final speed {:.3}\n  {}",
                    travel.length(),
                    travel.normalize_or_zero().dot(direction),
                    facing.dot(direction),
                    speed,
                    names.join(" -> ")
                );
                if travel.length() <= 0.5 {
                    failures.push(format!("{label}/{run}: the graph must move the body"));
                }
                if travel.normalize_or_zero().dot(direction) <= 0.9 {
                    failures.push(format!("{label}/{run}: travel follows the stick"));
                }
                if facing.dot(direction) <= 0.9 {
                    failures.push(format!("{label}/{run}: the character turns to the stick"));
                }
                if speed >= 0.05 {
                    failures.push(format!("{label}/{run}: releasing the stick stops"));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    #[ignore = "requires original ACU install; drives the native NewDemo graph"]
    fn installed_walking_turn_matches_native_rate() {
        let dir = std::env::var("ACU_GAME_DIR").expect("ACU_GAME_DIR required");
        let dir = std::path::Path::new(&dir);
        let library = crate::rig::load(dir).unwrap();
        let (mut driver, _) = crate::motion::load_graph(dir, &library).unwrap();
        driver.reset();
        // Walk toward arena -Z for 2 s, then hold the stick toward +X (90 degrees right).
        let mut player = Player {
            position: Vec3::new(2.0, 0.0, 10.0),
            ..Player::default()
        };
        let mut yaws = Vec::new();
        for tick in 0..240 {
            let direction = if tick < 120 { -Vec2::Y } else { Vec2::X };
            advance(
                &mut player,
                &mut driver,
                Input {
                    direction,
                    ..Default::default()
                },
                None,
            );
            yaws.push(player.yaw);
        }
        let turned = |t: usize| -yaws[t].to_degrees();
        println!(
            "yaw after the stick turned: 6 ticks {:.1}, 21 ticks {:.1}, 42 ticks {:.1}, 120 ticks {:.1}",
            turned(125),
            turned(140),
            turned(161),
            turned(239)
        );
        // Native (loco-turn-delta-1): 2.15 degrees per frame, 90 degrees after about 0.7 s.
        let early = turned(140) - turned(125);
        assert!(
            (early / 15.0 - 2.15).abs() < 0.3,
            "rate {:.2}/tick",
            early / 15.0
        );
        assert!((turned(239) - 90.0).abs() < 5.0, "final {:.1}", turned(239));
    }
}
