use bevy::prelude::*;
use pampa_core::{AnimationState, Heading, Vec2 as GroundVec2};
use pampa_render::{
    AtmospherePlugin, RiderVisual, RiderVisualPlugin, TerrainPlugin, VisualPose, WorldTerrain,
    world_from_ground,
};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            TerrainPlugin::default(),
            AtmospherePlugin,
            RiderVisualPlugin,
        ))
        .add_systems(Startup, (spawn_camera, spawn_rider_lineup))
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 4.0, 14.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
}

/// Temporary lineup showing all eight headings until input lands.
fn spawn_rider_lineup(mut commands: Commands, terrain: Res<WorldTerrain>) {
    for k in 0..8 {
        let ground = GroundVec2::new(-10.5 + k as f32 * 3.0, 0.0);
        let height = terrain.0.height(ground.x, ground.y);
        commands.spawn((
            RiderVisual::Billboard,
            VisualPose {
                heading: Heading::from_radians(std::f32::consts::FRAC_PI_2 + k as f32 * 0.785),
                animation: AnimationState::Idle,
            },
            Transform::from_translation(world_from_ground(ground, height)),
        ));
    }
}
