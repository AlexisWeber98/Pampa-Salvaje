//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod camera;
mod player;

use bevy::prelude::*;
use pampa_core::{Terrain, TownLayout, World};
use pampa_render::{
    AtmospherePlugin, RiderVisualPlugin, TerrainPlugin, TownPlugin, WorldTerrain, WorldTown,
};

fn main() {
    // One logic world feeds both the simulation and the drawing: the leveled
    // terrain, the town footprints and the bounds all come from `world`.
    let town = TownLayout::castex_inspired();
    let world = World::with_town(Terrain::default(), &town);
    let (position, heading) = town.arrival();
    App::new()
        .insert_resource(WorldTerrain(world.terrain))
        .insert_resource(WorldTown(town))
        .insert_resource(player::GameWorld(world))
        .insert_resource(player::PlayerSpawn { position, heading })
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Pampa Seca".into(),
                    ..default()
                }),
                ..default()
            }),
            TerrainPlugin::default(),
            TownPlugin,
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
