//! The player-controlled rider: keyboard -> `RiderInput` -> core step -> visuals.

use bevy::prelude::*;
use pampa_core::{Heading, Rider, RiderInput, RiderParams, Vec2 as GroundVec2};
use pampa_render::{RiderVisual, VisualPose, WorldTerrain, world_from_ground};

/// The logic rider this entity represents. Logic state lives here; the
/// entity's `Transform` and `VisualPose` are derived from it every frame.
#[derive(Component, Debug)]
pub struct Player {
    pub rider: Rider,
}

/// Movement tuning for the player's horse.
#[derive(Resource, Debug, Default)]
pub struct PlayerTuning(pub RiderParams);

/// Longest step fed to the simulation, so a hitch never teleports the rider.
const MAX_STEP: f32 = 1.0 / 20.0;

/// Systems that advance the player's logic; run cameras after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivePlayer;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerTuning>()
            .add_systems(Startup, spawn_player)
            .add_systems(Update, drive_player.in_set(DrivePlayer));
    }
}

fn spawn_player(mut commands: Commands, terrain: Res<WorldTerrain>) {
    // Start facing north, which is where the camera initially looks.
    let rider = Rider::new(
        GroundVec2::ZERO,
        Heading::from_radians(std::f32::consts::FRAC_PI_2),
        &terrain.0,
    );
    commands.spawn((
        Name::new("Player"),
        RiderVisual::Billboard,
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
    terrain: Res<WorldTerrain>,
    mut players: Query<(&mut Player, &mut Transform, &mut VisualPose)>,
) {
    let input = read_input(&keys);
    let dt = time.delta_secs().min(MAX_STEP);
    for (mut player, mut transform, mut pose) in &mut players {
        let rider = &mut player.rider;
        rider.step(input, &tuning.0, &terrain.0, dt);
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
