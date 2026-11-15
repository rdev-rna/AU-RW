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

    /// One tick of a scripted flat-ground run: time, root-motion position (arena x, z), yaw
    /// (left positive) and the dominant leaf.
    struct Sample {
        t: f32,
        pos: Vec2,
        yaw: f32,
        clip: String,
    }

    /// Conditions as text: parameter conditions by name, event conditions by id.
    fn conds_text(g: &acu_motion::atom_graph::Graph, c: &[acu_motion::atom_graph::Cond]) -> String {
        c.iter()
            .map(|c| {
                let name = if c.kind == 0 {
                    g.params
                        .get(c.id as usize)
                        .map(|p| p.name.clone())
                        .unwrap_or_default()
                } else {
                    format!("{:#010x}", c.id)
                };
                let join = if c.join == 1 { " or" } else { "" };
                let value = match c.value.ty {
                    0 => format!("{}", c.value.j != 0),
                    1 => format!("{}", f32::from_bits(c.value.i)),
                    2 => format!("{}", c.value.j as i32),
                    _ => String::new(),
                };
                if c.children.is_empty() {
                    format!(
                        "[k{} {name} op{} mode{} {value}]{join}",
                        c.kind, c.op, c.mode
                    )
                } else {
                    format!("({}){join}", conds_text(g, &c.children))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Print the transitions of the last step with the rule that fired and the entered
    /// machine's entry rules (`ACU_LOCO_TRACE`).
    fn trace_transitions(driver: &GraphDriver, tick: u32) {
        use acu_motion::atom_graph::Body;
        let g = &driver.player.graph;
        for &(at, from, to, rule) in &driver.player.diag.transitions {
            let Some(s) = g.state_at(at) else { continue };
            let Body::Machine {
                states, machine, ..
            } = &g.states[s].body
            else {
                continue;
            };
            let own = &g.states[states[from as usize]].transitions;
            let all: Vec<_> = own
                .iter()
                .chain(machine.iter().filter(|t| t.target != from))
                .collect();
            let text = all
                .get(rule)
                .and_then(|t| t.expr.as_ref())
                .map(|e| conds_text(g, e))
                .unwrap_or_else(|| "always".into());
            println!("  t{tick} machine {s}: {from} -> {to} (rule {rule}): {text}");
            // Entry rules of every machine entered below the transition.
            let path = driver.player.path();
            let below = path.iter().skip_while(|&&p| p != states[to as usize]);
            for &m in below {
                if let Body::Machine { entries, .. } = &g.states[m].body {
                    println!("      machine {m} entries:");
                    for (e, t) in entries {
                        let text = e.as_ref().map(|e| conds_text(g, e));
                        println!(
                            "        -> {t}: {}",
                            text.unwrap_or_else(|| "always".into())
                        );
                    }
                }
            }
        }
        if !driver.player.diag.transitions.is_empty() {
            let ev: Vec<String> = driver
                .player
                .events()
                .iter()
                .map(|(s, id, a, b)| format!("{s}:{id:#x}[{a:.2},{b:.2}]"))
                .collect();
            println!(
                "      path {:?} events {}",
                driver.player.path(),
                ev.join(" ")
            );
        }
    }

    /// Diagnostic: `ACU_DUMP=3348,3677` prints those states' subtrees with their clips;
    /// `ACU_FIND=<clip name part>` prints every graph state holding a matching clip with its
    /// machine ancestry (a state with no parent is outside every machine).
    #[test]
    #[ignore = "requires original ACU install; diagnostic dump of NewDemo states"]
    fn installed_graph_dump() {
        use acu_motion::atom_graph::Body;
        let dir = std::env::var("ACU_GAME_DIR").expect("ACU_GAME_DIR required");
        let dir = std::path::Path::new(&dir);
        let library = crate::rig::load(dir).unwrap();
        let (driver, _) = crate::motion::load_graph(dir, &library).unwrap();
        let g = &driver.player.graph;
        fn dump(d: &GraphDriver, s: usize, depth: usize) {
            let g = &d.player.graph;
            let pad = "  ".repeat(depth);
            match &g.states[s].body {
                Body::Machine { states, .. } => {
                    println!("{pad}{s} machine ev{:x?}", g.states[s].events);
                    for (i, &c) in states.iter().enumerate() {
                        print!("{pad} [{i}] ");
                        dump(d, c, depth + 1);
                    }
                }
                Body::Graph { nodes, .. } => {
                    let clips: Vec<String> = nodes
                        .iter()
                        .filter_map(|n| n.clip)
                        .map(|c| d.name(c).replace("xx_Nav_", ""))
                        .collect();
                    println!("{s} graph ev{:x?} {}", g.states[s].events, clips.join(", "));
                }
                Body::Layering { base, .. } => {
                    println!("{s} layering");
                    dump(d, *base, depth + 1);
                }
                Body::Other => println!("{s} other"),
            }
        }
        let mut parent = vec![None; g.states.len()];
        for (i, st) in g.states.iter().enumerate() {
            if let Body::Machine { states, .. } = &st.body {
                for (k, &c) in states.iter().enumerate() {
                    parent[c] = Some((i, k));
                }
            }
            if let Body::Layering { base, .. } = &st.body {
                parent[*base] = Some((i, 0));
            }
        }
        if let Ok(f) = std::env::var("ACU_FIND") {
            for (i, st) in g.states.iter().enumerate() {
                if let Body::Graph { nodes, .. } = &st.body
                    && nodes
                        .iter()
                        .filter_map(|n| n.clip)
                        .any(|c| driver.name(c).contains(&f))
                {
                    let mut chain = vec![format!("{i}")];
                    let mut s = i;
                    while let Some((p, k)) = parent[s] {
                        chain.push(format!("{p}[{k}]"));
                        s = p;
                    }
                    println!("found {f} in {}", chain.join(" < "));
                }
            }
        }
        for s in std::env::var("ACU_DUMP")
            .unwrap_or_default()
            .split(',')
            .filter_map(|x| x.parse().ok())
        {
            dump(&driver, s, 0);
        }
    }

    /// Drive the graph from the captured idle start through `steps` of (seconds, arena
    /// direction, free run). The body follows the requested root displacement alone, so the
    /// synthetic arena's obstacles cannot change the result.
    fn script(driver: &mut GraphDriver, steps: &[(f32, Vec2, bool)]) -> Vec<Sample> {
        driver.reset();
        let mut player = Player {
            position: Vec3::new(2.0, 0.0, 0.0),
            ..Player::default()
        };
        let mut pos = Vec2::ZERO;
        let mut out = Vec::new();
        let mut tick = 0u32;
        for &(secs, direction, run) in steps {
            for _ in 0..(secs / acu_sim::DT).round() as u32 {
                let input = Input {
                    direction,
                    run,
                    ..Default::default()
                };
                driver.player.diag.transitions.clear();
                pos += advance(&mut player, driver, input, None);
                if std::env::var_os("ACU_LOCO_TRACE").is_some() {
                    trace_transitions(driver, tick);
                }
                // Keep the arena body away from synthetic obstacles; only `pos` is measured.
                player.position = Vec3::new(2.0, 0.0, 0.0);
                tick += 1;
                let clip = driver
                    .dominant()
                    .map(|l| {
                        let n = driver.name(l.clip).replace("xx_Nav_", "");
                        if l.mirror { format!("{n} (m)") } else { n }
                    })
                    .unwrap_or_else(|| "-".into());
                out.push(Sample {
                    t: tick as f32 * acu_sim::DT,
                    pos,
                    yaw: player.yaw,
                    clip,
                });
            }
        }
        out
    }

    /// Run-length segments by dominant clip: (start s, end s, clip, mean ground speed m/s,
    /// yaw change degrees).
    fn segments(s: &[Sample]) -> Vec<(f32, f32, String, f32, f32)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for (i, x) in s.iter().enumerate() {
            match out.last_mut() {
                Some((_, e)) if s[*e].clip == x.clip => *e = i,
                _ => out.push((i, i)),
            }
        }
        out.iter()
            .map(|&(a, b)| {
                let a0 = a.saturating_sub(1);
                let dt = s[b].t - s[a0].t;
                let d = (s[b].pos - s[a0].pos).length();
                let dyaw = acu_motion::amo::wrap_angle(s[b].yaw - s[a0].yaw).to_degrees();
                (
                    s[a].t - acu_sim::DT,
                    s[b].t,
                    s[a].clip.clone(),
                    if dt > 0.0 { d / dt } else { 0.0 },
                    dyaw,
                )
            })
            .collect()
    }

    fn print_segments(label: &str, s: &[Sample]) {
        println!("== {label}");
        for (a, b, clip, v, dyaw) in segments(s) {
            println!("  {a:6.2}-{b:6.2} {clip:44} v={v:5.2} m/s dyaw {dyaw:7.1}");
        }
    }

    type Script = Vec<(f32, Vec2, bool)>;

    /// The segment of `s` dominant at `t` seconds.
    fn segment_at(s: &[Sample], t: f32) -> (f32, f32, String, f32, f32) {
        segments(s)
            .into_iter()
            .find(|g| g.0 <= t && t < g.1 + 1e-4)
            .expect("segment")
    }

    /// Ground speed over [a, b] seconds of a script, m/s.
    fn speed(s: &[Sample], a: f32, b: f32) -> f32 {
        let at = |t: f32| s[((t / acu_sim::DT).round() as usize).clamp(1, s.len()) - 1].pos;
        (at(b) - at(a)).length() / (b - a)
    }

    /// Flat-ground locomotion against the live captures (docs/LOCOMOTION_PARITY.md). Native
    /// bands come from `local/native-probe/runs/`:
    /// - loco-params-1 `live-state2.jsonl` + `params2.jsonl` (30 Hz, aligned by QPC);
    /// - loco-turn-delta-1 / -2 and loco-turns-1 (`live-state.jsonl`, 30 / 60 Hz).
    ///
    /// Only open-ground segments are used (no wall-brace, crowd-avoid or slope clip dominant).
    /// Known differences are printed and listed; any other difference fails.
    #[test]
    #[ignore = "requires original ACU install; compares flat-ground locomotion with live captures"]
    fn installed_locomotion_segments() {
        let dir = std::env::var("ACU_GAME_DIR").expect("ACU_GAME_DIR required");
        let dir = std::path::Path::new(&dir);
        let library = crate::rig::load(dir).unwrap();
        let (mut driver, _) = crate::motion::load_graph(dir, &library).unwrap();
        let fwd = -Vec2::Y;
        let none = Vec2::ZERO;
        let mut failures = Vec::new();
        let mut check = |ok: bool, what: String| {
            println!("  {} {what}", if ok { "MATCH" } else { "DIFF " });
            if !ok {
                failures.push(what);
            }
        };

        // Idle: native IdleLow_Footr/Footl at 0 m/s in every capture's first seconds.
        let s = script(&mut driver, &[(3.0, none, false)]);
        print_segments("idle", &s);
        let still = s.iter().all(|x| x.pos.length() < 1e-4);
        let idle = segments(&s).iter().all(|g| g.2.starts_with("IdleLow_Foot"));
        check(
            still && idle,
            format!("idle: IdleLow_Foot*, no travel ({still}, {idle})"),
        );

        // Full-stick walk from idle, then release.
        let walk: Script = vec![(1.0, none, false), (4.0, fwd, false), (3.0, none, false)];
        let s = script(&mut driver, &walk);
        print_segments("walk full stick, stop", &s);
        let start = segment_at(&s, 1.05);
        // Native PreNavWalkMid_Lt0/Rt0_tr_Walk dominant 0.60-1.12 s (7 starts).
        check(
            start.2.starts_with("PreNavWalkMid_") && start.2.contains("t0_tr_Walk"),
            format!(
                "walk start clip {} (native PreNavWalkMid_Lt0/Rt0_tr_Walk)",
                start.2
            ),
        );
        let len = start.1 - start.0;
        check(
            (0.55..=1.15).contains(&len),
            format!("walk start {len:.2} s (native 0.60-1.12 s)"),
        );
        // Native first dominant frame within one 30 Hz sample of MovementSpeed > 0.
        check(
            start.0 - 1.0 <= 0.05,
            format!("start latency {:.3} s (native <= 0.033 s)", start.0 - 1.0),
        );
        check(
            segment_at(&s, 4.0).2 == "Walk_3-Loop",
            format!("walk loop {} (native Walk_3-Loop)", segment_at(&s, 4.0).2),
        );
        // Native open-ground Walk_3-Loop over 0.5 s windows: 1.75-1.99 m/s (loco-turn-delta-1),
        // 2.22-2.31 m/s (loco-params-1 fwd_again and turns); TimeScale varies 0.9-1.3 there.
        let v = speed(&s, 4.0, 5.0);
        check(
            (1.7..=2.4).contains(&v),
            format!("walk speed {v:.2} m/s (native 1.75-2.31)"),
        );
        let stop = segment_at(&s, 5.3);
        check(
            stop.2.starts_with("WalkStop_RC"),
            format!(
                "walk stop clip {} (native WalkStop_RC in 4/4 stops)",
                stop.2
            ),
        );
        // Native stop travel 0.39-0.45 m from the stop clip's first frame (4 stops).
        let first = (stop.0 / acu_sim::DT) as usize;
        let travel = (s[s.len() - 1].pos - s[first].pos).length();
        check(
            (0.3..=0.55).contains(&travel),
            format!("walk stop travel {travel:.2} m (native 0.39-0.45)"),
        );
        check(
            segment_at(&s, 7.9).2.starts_with("IdleLow_Foot"),
            "back to IdleLow after the walk stop".into(),
        );

        // Half stick (MovementSpeed 0.621).
        let half: Script = vec![
            (1.0, none, false),
            (4.0, fwd * 0.5, false),
            (3.0, none, false),
        ];
        let s = script(&mut driver, &half);
        print_segments("walk half stick, stop", &s);
        check(
            segment_at(&s, 4.0).2 == "WalkSlow_3-Loop",
            format!(
                "half-stick loop {} (native WalkSlow_3-Loop)",
                segment_at(&s, 4.0).2
            ),
        );
        // Native WalkSlow_3-Loop at MovementSpeed 0.621: 0.76-0.87 m/s (TimeScale 0.7-0.84).
        let v = speed(&s, 4.0, 5.0);
        check(
            (0.7..=0.95).contains(&v),
            format!("half-stick speed {v:.2} m/s (native 0.76-0.87)"),
        );
        let stop = segment_at(&s, 5.5);
        check(
            stop.2.starts_with("WalkSlow_Stop"),
            format!(
                "half-stick stop {} (native WalkSlow_Stop_LP, 1 stop)",
                stop.2
            ),
        );

        // Free run (high profile) from idle, then release.
        let run: Script = vec![(1.0, none, false), (4.0, fwd, true), (3.0, none, false)];
        let s = script(&mut driver, &run);
        print_segments("free run, stop", &s);
        let first_sprint = s
            .iter()
            .find(|x| x.clip.starts_with("Sprint_3-Loop"))
            .map(|x| x.t - 1.0);
        // Native: input to Sprint_3-Loop_V02 dominant 1.00-1.82 s (loco-params-1 twice,
        // loco-turn-delta-2), through Run_3-Loop or acu_nav_run_impultion.
        check(
            first_sprint.is_some_and(|t| (0.9..=1.9).contains(&t)),
            format!("time to sprint {first_sprint:?} s (native 1.00-1.82)"),
        );
        // Native Sprint_3-Loop_V02: 5.26 (60 Hz, loco-turn-delta-2), 5.79-5.95 m/s (30 Hz
        // windows, partly blended with avoid clips).
        let v = speed(&s, 4.0, 5.0);
        check(
            (5.2..=6.0).contains(&v),
            format!("sprint speed {v:.2} m/s (native 5.26-5.95)"),
        );
        let stop = segment_at(&s, 5.4);
        check(
            stop.2.starts_with("SprintStop_Fd100cm"),
            format!("sprint stop {} (native SprintStop_Fd100cm)", stop.2),
        );
        let len = stop.1 - stop.0;
        check(
            (0.65..=0.9).contains(&len),
            format!("sprint stop {len:.2} s (native 0.74-0.78)"),
        );
        let (a, b) = (
            (stop.0 / acu_sim::DT) as usize,
            (stop.1 / acu_sim::DT) as usize,
        );
        let travel = (s[b.min(s.len() - 1)].pos - s[a].pos).length();
        check(
            (0.5..=0.9).contains(&travel),
            format!("sprint stop travel {travel:.2} m (native 0.60-0.81)"),
        );
        let after = segment_at(&s, stop.1 + 0.05).2;
        check(
            after.starts_with("IdleHigh_tr_IdleLow"),
            format!("after the sprint stop {after} (native IdleHigh_tr_IdleLow_footl, 2/2)"),
        );

        // Walking turn: stick 90 degrees right after 2.5 s of walking.
        let turn: Script = vec![
            (1.0, none, false),
            (2.5, fwd, false),
            (2.5, Vec2::X, false),
            (3.0, none, false),
        ];
        let s = script(&mut driver, &turn);
        print_segments("walk, stick right 90, stop", &s);
        let tick = |t: f32| (t / acu_sim::DT).round() as usize - 1;
        let yaw = |a: f32, b: f32| acu_motion::amo::wrap_angle(s[tick(b)].yaw - s[tick(a)].yaw);
        let rate = yaw(3.55, 3.75).to_degrees() / 12.0;
        // Native loco-turn-delta-1: 2.14-2.15 degrees per 60 Hz frame, identity root rotation.
        check(
            (rate + 2.15).abs() < 0.1,
            format!("walking turn {rate:.2} deg/frame (native -2.15, right)"),
        );
        let total = yaw(3.5, 5.5).to_degrees();
        check(
            (total + 90.0).abs() < 2.0 && segment_at(&s, 4.0).2 == "Walk_3-Loop",
            format!(
                "walking turn total {total:.1} deg in Walk_3-Loop (native about 90, no turn clip)"
            ),
        );

        // Stick reversed while walking.
        let reverse: Script = vec![
            (1.0, none, false),
            (2.5, fwd, false),
            (2.0, -fwd, false),
            (3.0, none, false),
        ];
        let s = script(&mut driver, &reverse);
        print_segments("walk, stick reversed, stop", &s);
        let plant = segments(&s)
            .into_iter()
            .find(|g| g.2.contains("PlantAndTurnLt180"));
        // Native WalkFwd_PlantAndTurnLt180_ProperRP 0.52 s / 175.5 deg, _Improper 0.78 s /
        // 177.5 deg (loco-params-1, two runs).
        check(
            plant.as_ref().is_some_and(|g| {
                (0.45..=0.85).contains(&(g.1 - g.0)) && (g.4.abs() - 177.0).abs() < 6.0
            }),
            format!("plant and turn {plant:?} (native 0.52-0.78 s, 175-178 deg)"),
        );

        println!("differences from native: {failures:#?}");
        // Known differences, reported in docs/LOCOMOTION_PARITY.md; any other is a regression.
        let known = [
            "walk stop clip",
            "walk stop travel",
            "half-stick stop",
            "sprint speed",
            "after the sprint stop",
        ];
        let unexpected: Vec<_> = failures
            .iter()
            .filter(|f| !known.iter().any(|k| f.starts_with(k)))
            .collect();
        assert!(unexpected.is_empty(), "{unexpected:#?}");
    }
}
