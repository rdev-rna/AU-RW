//! Test movement and authored pose driver; docs/mechanics/test-world-movement.md.
use crate::{InspectionCamera, controls::Controller, options::Options, rig::Skin};
use acu_sim::{Input, Player, Traversal};
use bevy::prelude::*;

#[derive(Component)]
pub struct Character;

#[derive(Resource)]
pub struct State {
    pub player: Player,
    previous: Player,
    pub skins: Vec<Skin>,
    pub bind: bool,
    pub deform: bool,
    pub frozen: bool,
    pub capture: bool,
    jump_requested: bool,
    frames: u32,
    skin_seconds: f64,
}

pub fn replay(scenario: &str, tick: u32) -> Input {
    let direction = match scenario {
        "walk" | "run" | "jump" => Vec2::new(0.0, -1.0),
        "turn" => {
            if tick < 90 {
                Vec2::new(0.0, -1.0)
            } else {
                Vec2::X
            }
        }
        "contact" | "profile-up" | "profile-down" => Vec2::new(0.0, -1.0),
        _ => Vec2::ZERO,
    };
    Input {
        direction,
        run: matches!(scenario, "run" | "contact" | "profile-up" | "profile-down"),
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
    pub fn new(options: &Options) -> Self {
        let mut player = Player::default();
        for tick in 0..options.ticks {
            player.step(replay(&options.scenario, tick));
        }
        Self {
            player,
            previous: player,
            skins: Vec::new(),
            bind: options.scenario == "bind",
            deform: options.scenario == "deform",
            frozen: false,
            capture: options.capture.is_some(),
            jump_requested: false,
            frames: 0,
            skin_seconds: 0.0,
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
    state.player.step(input);
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
    let speed = Vec2::new(state.player.velocity.x, state.player.velocity.z).length();
    if !state.frozen {
        let start = std::time::Instant::now();
        for skin in &state.skins {
            skin.update(
                &mut meshes,
                state.player.distance,
                speed,
                state.player.tick,
                state.bind,
                state.deform,
            );
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
