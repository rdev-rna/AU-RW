//! Test movement and authored pose driver; docs/mechanics/test-world-movement.md.
use crate::{InspectionCamera, controls::Controller, options::Options, rig::Skin};
use acu_sim::{Input, Player, Traversal};
use bevy::prelude::*;

#[derive(Component)]
pub struct Character;

fn turn_facing(yaw: f32, heading: f32, moving: bool, run: bool) -> f32 {
    if !moving {
        return yaw;
    }
    let turn = acu_motion::amo::wrap_angle(heading - yaw);
    // Authored arena tuning, adjusted after visible review on 2026-10-20.
    // Native steering recovery remains separate from this responsiveness setting.
    let max_turn = (if run { 540.0_f32 } else { 360.0_f32 }).to_radians() * acu_sim::DT;
    yaw + turn.clamp(-max_turn, max_turn)
}

fn advance(
    player: &mut Player,
    matcher: &mut acu_motion::unity::Matcher,
    input: Input,
    bounds: f32,
) {
    player.bounds = bounds;
    if input.reset {
        matcher.reset();
        *player = Player::default();
    }
    use acu_motion::unity::Profile;
    let amount = input.direction.length().min(1.0);
    let profile = if amount < 0.08 {
        Profile::Idle
    } else if input.run {
        Profile::Sprint
    } else {
        Profile::Walk
    };
    let desired = input.direction.normalize_or_zero();
    let heading = desired.x.atan2(desired.y);
    // Facing turns independently from travel. In particular, starting from
    // idle must not snap to the first requested direction.
    let (s, c) = player.yaw.sin_cos();
    let local = Vec2::new(c * desired.x - s * desired.y, s * desired.x + c * desired.y);
    let delta = matcher.step_direction(profile, amount, local, acu_sim::DT);
    let native = delta.truncate();
    let world = Vec2::new(c * native.x + s * native.y, -s * native.x + c * native.y);
    // Original cardinal takes do not cover every stick angle. Warp the remaining
    // direction while retaining the selected clip's root distance and pose time.
    // Both this warp and facing speed are authored arena policies, finding0019.
    let displacement = if profile == Profile::Idle {
        Vec2::ZERO
    } else {
        let angle = acu_motion::amo::wrap_angle(heading - world.x.atan2(world.y));
        let (s, c) = angle.sin_cos();
        Vec2::new(c * world.x + s * world.y, -s * world.x + c * world.y)
    };
    let yaw = turn_facing(player.yaw, heading, profile != Profile::Idle, input.run);
    player.step_with_root(input, displacement, yaw);
}

#[derive(Resource)]
pub struct State {
    pub matcher: acu_motion::unity::Matcher,
    pub motion_report: serde_json::Value,
    pub motion_trace: Vec<serde_json::Value>,
    pub player: Player,
    previous: Player,
    pub skins: Vec<Skin>,
    pub bind: bool,
    pub deform: bool,
    pub frozen: bool,
    pub capture: bool,
    /// Walkable half-extent. The synthetic arena uses `acu_sim::ARENA_BOUNDS`; a world map
    /// scene sets it large because there is no runtime collision yet.
    pub bounds: f32,
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
        "contact" | "profile-up" | "profile-down" => Vec2::new(0.0, -1.0),
        _ => Vec2::ZERO,
    };
    Input {
        direction,
        run: scenario.starts_with("run")
            || matches!(scenario, "contact" | "profile-up" | "profile-down"),
        jump: scenario == "jump" && tick == 90,
        reset: false,
        traversal: match scenario {
            "profile-up" => Traversal::Up,
            "profile-down" => Traversal::Down,
            _ => Traversal::Neutral,
        },
    }
}

impl State {
    pub fn new(
        options: &Options,
        mut matcher: acu_motion::unity::Matcher,
        motion_report: serde_json::Value,
    ) -> Self {
        let mut player = Player::default();
        player.bounds = if options.map_player {
            100_000.0
        } else {
            acu_sim::ARENA_BOUNDS
        };
        let mut motion_trace = Vec::new();
        for tick in 0..options.ticks {
            let input = replay(&options.scenario, tick);
            let selected = matcher.selected;
            let bounds = player.bounds;
            advance(&mut player, &mut matcher, input, bounds);
            if selected != matcher.selected
                || tick == 0
                || tick + 1 == options.ticks
                || options.scenario == "turn-arc"
            {
                motion_trace.push(serde_json::json!({"tick":tick+1,"direction":input.direction.to_array(),
                    "yaw":player.yaw,"clip":matcher.takes[matcher.selected].file_id,
                    "name":matcher.takes[matcher.selected].name,"time":matcher.time,"cost":matcher.cost,
                    "position":player.position.to_array()}));
            }
        }
        Self {
            matcher,
            motion_report,
            motion_trace,
            player,
            previous: player,
            skins: Vec::new(),
            bind: options.scenario == "bind",
            deform: options.scenario == "deform",
            frozen: false,
            capture: options.capture.is_some(),
            bounds: player.bounds,
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
    let bounds = state.bounds;
    if state.reversal_review {
        if state.review_tick == 1080 {
            state.review_trace_done = true;
            state.review_tick = 0;
            state.player = Player::default();
            state.matcher.reset();
        }
        let tick = state.review_tick;
        let input = replay("reversals", tick);
        state.previous = state.player;
        let State {
            player, matcher, ..
        } = &mut *state;
        advance(player, matcher, input, bounds);
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
                    serde_json::json!({"tick":tick,"direction":input.direction.to_array(),"position":state.player.position.to_array(),"yaw":state.player.yaw,"clip":state.matcher.takes[state.matcher.selected].file_id,"name":state.matcher.takes[state.matcher.selected].name,"time":state.matcher.time,"pose":state.matcher.pose.iter().map(|m|m.w_axis.truncate().to_array()).collect::<Vec<_>>(),"parents":state.matcher.rig.parents})
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
    if std::env::var_os("ACU_PLAYER_TRACE").is_some() && stick != Vec2::ZERO {
        eprintln!(
            "player: stick {stick:?} position {:?} yaw {:.2}",
            state.player.position, state.player.yaw
        );
    }
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
        player, matcher, ..
    } = &mut *state;
    advance(player, matcher, input, bounds);
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
            skin.update(&mut meshes, &state.matcher.pose, state.bind, state.deform);
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
    fn turn_in_from_idle_is_gradual() {
        let target = std::f32::consts::FRAC_PI_2;
        let walk_step = turn_facing(0.0, target, true, false);
        let sprint_step = turn_facing(0.0, target, true, true);
        assert!((walk_step.to_degrees() - 6.0).abs() < 1e-5);
        assert!((sprint_step.to_degrees() - 9.0).abs() < 1e-5);
        assert!(walk_step < target && sprint_step < target);
        assert_eq!(turn_facing(0.0, target, false, false), 0.0);
    }

    #[test]
    #[ignore = "requires original ACU install; exercises original clips and composed rig"]
    fn installed_directional_steering() {
        let dir = std::env::var("ACU_GAME_DIR").expect("ACU_GAME_DIR required");
        let library = crate::rig::load(std::path::Path::new(&dir)).unwrap();
        let (mut m, _) = crate::motion::load(std::path::Path::new(&dir), &library).unwrap();
        for (run, right, left, forward) in [
            (
                false,
                "000000137c558075",
                "000000137c55802d",
                "0000000fc08fd67a",
            ),
            (
                true,
                "0000000fed5e3950",
                "000000080594a3c6",
                "000000016737c2c4",
            ),
        ] {
            // Isolate matcher requests from world-space steering. Same profile throughout.
            m.reset();
            for (direction, id) in [
                (Vec2::Y, forward),
                (Vec2::X, right),
                (-Vec2::X, left),
                (Vec2::Y, forward),
            ] {
                let d = m.step_direction(
                    if run {
                        acu_motion::unity::Profile::Sprint
                    } else {
                        acu_motion::unity::Profile::Walk
                    },
                    1.0,
                    direction,
                    acu_sim::DT,
                );
                assert_eq!(m.takes[m.selected].file_id, id);
                assert!(d.truncate().normalize().dot(direction) > 0.99);
            }
            for side in [-1.0, 1.0] {
                m.reset();
                let mut player = Player::default();
                for _ in 0..60 {
                    advance(
                        &mut player,
                        &mut m,
                        Input {
                            direction: -Vec2::Y,
                            run,
                            ..Default::default()
                        },
                    );
                }
                let yaw = player.yaw;
                advance(
                    &mut player,
                    &mut m,
                    Input {
                        direction: Vec2::X * side,
                        run,
                        ..Default::default()
                    },
                );
                // Facing -Z means world +X is actor-local LEFT.
                assert_eq!(
                    m.takes[m.selected].file_id,
                    if side > 0.0 { left } else { right }
                );
                assert!(
                    (player.yaw - yaw).abs() <= 9.01_f32.to_radians(),
                    "facing must not snap to the stick"
                );
                assert!(player.velocity.x * side > 0.0);
                assert!(player.velocity.z.abs() < 1e-4);
                let mut switches = 0;
                for _ in 0..90 {
                    let old = m.selected;
                    advance(
                        &mut player,
                        &mut m,
                        Input {
                            direction: Vec2::X * side,
                            run,
                            ..Default::default()
                        },
                    );
                    switches += usize::from(old != m.selected);
                }
                assert_eq!(
                    m.takes[m.selected].file_id, forward,
                    "held direction must settle to forward"
                );
                assert!(switches <= 3, "corner must not churn takes: {switches}");
            }
        }
    }
}
