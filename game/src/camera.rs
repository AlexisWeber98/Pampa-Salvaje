//! Third-person camera that trails behind and above the player, smoothly.

use bevy::prelude::*;
use pampa_render::{WorldTerrain, world_from_ground};

use crate::player::{DrivePlayer, Player};

/// Where the camera sits relative to the rider, and how quickly it catches up.
#[derive(Component, Debug, Clone, Copy)]
pub struct FollowCamera {
    pub distance: f32,
    pub height: f32,
    /// Point above the rider's feet the camera looks at.
    pub look_height: f32,
    /// Exponential catch-up rate (higher = snappier).
    pub smoothing: f32,
    /// Minimum clearance above the terrain under the camera.
    pub ground_clearance: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            distance: 9.0,
            height: 4.0,
            look_height: 1.6,
            smoothing: 4.0,
            ground_clearance: 1.0,
        }
    }
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, follow_player.after(DrivePlayer));
    }
}

fn spawn_camera(mut commands: Commands) {
    let follow = FollowCamera::default();
    commands.spawn((
        Name::new("Follow camera"),
        Camera3d::default(),
        Transform::from_xyz(0.0, follow.height, follow.distance)
            .looking_at(Vec3::Y * follow.look_height, Vec3::Y),
        follow,
    ));
}

fn follow_player(
    time: Res<Time>,
    terrain: Res<WorldTerrain>,
    player: Single<&Player>,
    camera: Single<(&mut Transform, &FollowCamera)>,
) {
    let rider = &player.rider;
    let (mut transform, follow) = camera.into_inner();

    let feet = world_from_ground(rider.position, rider.height);
    let back = world_from_ground(rider.heading.forward(), 0.0) * -follow.distance;
    let mut desired = feet + back + Vec3::Y * follow.height;

    // Keep the camera from sinking into a rise behind the rider.
    let ground = terrain.0.height(desired.x, -desired.z) + follow.ground_clearance;
    desired.y = desired.y.max(ground);

    let dt = time.delta_secs();
    transform
        .translation
        .smooth_nudge(&desired, follow.smoothing, dt);
    transform.look_at(feet + Vec3::Y * follow.look_height, Vec3::Y);
}
