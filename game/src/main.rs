use bevy::prelude::*;
use pampa_render::{AtmospherePlugin, TerrainPlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, TerrainPlugin::default(), AtmospherePlugin))
        .add_systems(Startup, spawn_camera)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 12.0, 25.0).looking_at(Vec3::new(0.0, 0.0, -20.0), Vec3::Y),
    ));
}
