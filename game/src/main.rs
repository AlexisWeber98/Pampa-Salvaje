//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod camera;
mod player;

use bevy::prelude::*;
use pampa_render::{AtmospherePlugin, RiderVisualPlugin, TerrainPlugin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Pampa Seca".into(),
                    ..default()
                }),
                ..default()
            }),
            TerrainPlugin::default(),
            AtmospherePlugin,
            RiderVisualPlugin,
            player::PlayerPlugin,
            camera::FollowCameraPlugin,
        ))
        .add_systems(Update, quit_on_escape)
        .run();
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
