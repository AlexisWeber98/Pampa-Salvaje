//! The player-controlled rider: keyboard -> `RiderInput` -> core step -> visuals.

use bevy::prelude::*;
use pampa_core::{Heading, MAX_STEP, Rider, RiderInput, RiderParams, Vec2 as GroundVec2, World};
use pampa_render::{CharacterVisual, SheetKind, VisualPose, world_from_ground};

/// The logic rider this entity represents. Logic state lives here; the
/// entity's `Transform` and `VisualPose` are derived from it every frame.
#[derive(Component, Debug)]
pub struct Player {
    pub rider: Rider,
}

/// The logic world the player moves through. Render draws from the same
/// value (see `main`), so what collides is what is drawn.
#[derive(Resource, Debug, Clone, Default)]
pub struct GameWorld(pub World);

/// Where and facing which way the player starts.
#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerSpawn {
    pub position: GroundVec2,
    pub heading: Heading,
}

impl Default for PlayerSpawn {
    /// The origin, facing north (where the camera initially looks).
    fn default() -> Self {
        Self {
            position: GroundVec2::ZERO,
            heading: Heading::from_radians(std::f32::consts::FRAC_PI_2),
        }
    }
}

/// Movement tuning for the player's horse.
#[derive(Resource, Debug, Default)]
pub struct PlayerTuning(pub RiderParams);

/// Systems that advance the player's logic; run cameras after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivePlayer;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerTuning>()
            .init_resource::<GameWorld>()
            .init_resource::<PlayerSpawn>()
            .add_systems(Startup, spawn_player)
            .add_systems(Update, drive_player.in_set(DrivePlayer));
    }
}

fn spawn_player(mut commands: Commands, world: Res<GameWorld>, spawn: Res<PlayerSpawn>) {
    let rider = Rider::new(spawn.position, spawn.heading, &world.0.terrain);
    commands.spawn((
        Name::new("Player"),
        CharacterVisual::Billboard(SheetKind::Rider),
        VisualPose {
            heading: rider.heading,
            animation: Default::default(),
        },
        Transform::from_translation(world_from_ground(rider.position, rider.height)),
        Player { rider },
    ));
}

/// Maps held keys to logic intent: W/S throttle, A/D turn, Shift gallop.
pub fn read_input(keys: &ButtonInput<KeyCode>) -> RiderInput {
    let axis = |positive: KeyCode, negative: KeyCode| {
        f32::from(u8::from(keys.pressed(positive))) - f32::from(u8::from(keys.pressed(negative)))
    };
    RiderInput {
        throttle: axis(KeyCode::KeyW, KeyCode::KeyS),
        turn: axis(KeyCode::KeyA, KeyCode::KeyD),
        gallop: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    }
}

fn drive_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    tuning: Res<PlayerTuning>,
    world: Res<GameWorld>,
    mut players: Query<(&mut Player, &mut Transform, &mut VisualPose)>,
) {
    let input = read_input(&keys);
    let dt = time.delta_secs().min(MAX_STEP);
    for (mut player, mut transform, mut pose) in &mut players {
        let rider = &mut player.rider;
        rider.step(input, &tuning.0, &world.0, dt);
        transform.translation = world_from_ground(rider.position, rider.height);
        *pose = VisualPose {
            heading: rider.heading,
            animation: rider.animation(&tuning.0),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Headless app: no window, no renderer, time advanced by hand.
    fn headless_app(frame: Duration) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(frame))
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(PlayerPlugin);
        app
    }

    fn player_rider(app: &mut App) -> Rider {
        let mut query = app.world_mut().query::<&Player>();
        query.single(app.world()).expect("one player").rider
    }

    #[test]
    fn long_frames_are_capped_to_max_step() {
        // Bevy's virtual clock already clamps at 0.25 s; stay below that so
        // this exercises our own cap.
        let mut app = headless_app(Duration::from_millis(200));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.update();
        app.update();

        let before = player_rider(&mut app);
        app.update();
        assert!((app.world().resource::<Time>().delta_secs() - 0.2).abs() < 1e-6);
        let after = player_rider(&mut app);

        let accel = app.world().resource::<PlayerTuning>().0.acceleration;
        let expected_speed = before.speed + accel * MAX_STEP;
        assert!(
            (after.speed - expected_speed).abs() < 1e-5,
            "a 0.2 s hitch must advance only {MAX_STEP} s: {} vs {expected_speed}",
            after.speed
        );
        let moved = after.position.distance(before.position);
        assert!((moved - after.speed * MAX_STEP).abs() < 1e-5);
    }

    #[test]
    fn spawns_at_the_configured_spawn_point() {
        let spawn = PlayerSpawn {
            position: GroundVec2::new(26.0, -161.0),
            heading: Heading::from_radians(1.0),
        };
        let mut app = headless_app(Duration::from_millis(16));
        app.insert_resource(spawn);
        app.update();
        let rider = player_rider(&mut app);
        assert_eq!(
            (rider.position, rider.heading),
            (spawn.position, spawn.heading)
        );
    }

    #[test]
    fn player_transform_follows_logic() {
        let mut app = headless_app(Duration::from_millis(16));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..30 {
            app.update();
        }
        let rider = player_rider(&mut app);
        assert!(rider.speed > 0.0);
        let mut query = app.world_mut().query_filtered::<&Transform, With<Player>>();
        let transform = query.single(app.world()).unwrap();
        assert_eq!(
            transform.translation,
            world_from_ground(rider.position, rider.height)
        );
    }

    #[test]
    fn maps_keys_to_rider_input() {
        let mut keys = ButtonInput::<KeyCode>::default();
        assert_eq!(read_input(&keys), RiderInput::default());

        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        keys.press(KeyCode::ShiftLeft);
        let input = read_input(&keys);
        assert_eq!(input.throttle, 1.0);
        assert_eq!(input.turn, -1.0);
        assert!(input.gallop);

        keys.press(KeyCode::KeyS);
        keys.press(KeyCode::KeyA);
        let opposed = read_input(&keys);
        assert_eq!((opposed.throttle, opposed.turn), (0.0, 0.0));
    }
}
