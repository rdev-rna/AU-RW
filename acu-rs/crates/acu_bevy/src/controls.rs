//! Physical controller layout requested by the user; no native tuning claim.
use acu_sim::Traversal;
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct Controller {
    selected: Option<Entity>,
    pub movement: Vec2,
    pub look: Vec2,
    pub high_profile: bool,
    pub traversal: Traversal,
}

fn stick(raw: Vec2) -> Vec2 {
    let length = raw.length();
    if !length.is_finite() || length <= 0.16 {
        Vec2::ZERO
    } else {
        raw / length * ((length - 0.16) / 0.84).min(1.0)
    }
}

pub fn sample(
    mut controller: ResMut<Controller>,
    pads: Query<(Entity, &Gamepad)>,
    windows: Query<&Window>,
) {
    if !windows.iter().any(|w| w.focused) {
        *controller = Controller::default();
        return;
    }
    let selected = controller
        .selected
        .filter(|e| pads.contains(*e))
        .or_else(|| {
            pads.iter()
                .map(|(entity, _)| entity)
                .min_by_key(|e| e.to_bits())
        });
    *controller = Controller {
        selected,
        ..default()
    };
    let Some((_, pad)) = selected.and_then(|e| pads.get(e).ok()) else {
        return;
    };
    let movement = stick(pad.left_stick());
    controller.movement = Vec2::new(movement.x, -movement.y);
    controller.look = stick(pad.right_stick());
    controller.high_profile = pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0) > 0.25;
    controller.traversal = Traversal::from_buttons(
        controller.high_profile,
        pad.pressed(GamepadButton::South), // PlayStation X / Xbox A, not West.
        pad.pressed(GamepadButton::East),  // PlayStation Circle / Xbox B.
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analog_deadzone_preserves_direction_and_full_scale() {
        assert_eq!(stick(Vec2::splat(0.1)), Vec2::ZERO);
        assert_eq!(stick(Vec2::X), Vec2::X);
        assert!((stick(Vec2::new(0.0, -0.58)).y + 0.5).abs() < 1e-6);
        assert!((stick(Vec2::ONE).length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn controller_hold_release_camera_and_disconnect() {
        let mut app = App::new();
        app.init_resource::<Controller>()
            .add_systems(Update, sample);
        let window = app
            .world_mut()
            .spawn(Window {
                focused: true,
                ..default()
            })
            .id();
        let mut pad = Gamepad::default();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.8);
        pad.analog_mut().set(GamepadAxis::LeftStickY, 1.0);
        pad.analog_mut().set(GamepadAxis::RightStickX, -1.0);
        pad.analog_mut().set(GamepadAxis::RightStickY, 0.58);
        pad.digital_mut().press(GamepadButton::South);
        let entity = app.world_mut().spawn(pad).id();
        app.update();
        let state = app.world().resource::<Controller>();
        assert!(state.high_profile);
        assert_eq!(state.traversal, Traversal::Up);
        assert_eq!(state.movement, -Vec2::Y);
        assert!(state.look.x < 0.0 && state.look.y > 0.0);
        app.world_mut()
            .get_mut::<Gamepad>(entity)
            .unwrap()
            .digital_mut()
            .press(GamepadButton::East);
        app.update();
        assert_eq!(
            app.world().resource::<Controller>().traversal,
            Traversal::Down
        );
        app.world_mut()
            .get_mut::<Gamepad>(entity)
            .unwrap()
            .analog_mut()
            .set(GamepadButton::RightTrigger2, 0.0);
        app.update();
        let state = app.world().resource::<Controller>();
        assert!(!state.high_profile);
        assert_eq!(state.traversal, Traversal::Neutral);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert_eq!(app.world().resource::<Controller>().movement, Vec2::ZERO);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.world_mut().despawn(entity);
        app.update();
        let state = app.world().resource::<Controller>();
        assert_eq!(state.movement, Vec2::ZERO);
        assert_eq!(state.look, Vec2::ZERO);
        assert!(!state.high_profile);
    }
}
