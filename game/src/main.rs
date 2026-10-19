//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod camera;
mod life;
mod orbit;
mod player;

use bevy::prelude::*;
use pampa_core::townsfolk::TOWNSFOLK_COUNT;
use pampa_core::{Terrain, TownLayout, Townsfolk, Train, TrainSchedule, World};
use pampa_render::{
    AtmospherePlugin, CharacterVisualPlugin, SignPlugin, TerrainPlugin, TownPlugin, TrainPlugin,
    WorldTerrain, WorldTown, WorldTrain,
};

/// Seed for where the townsfolk start and wander.
const TOWNSFOLK_SEED: u32 = 1927;

fn main() {
    // One logic world feeds both the simulation and the drawing: the leveled
    // terrain, the town footprints and the bounds all come from `world`.
    let town = TownLayout::castex_inspired();
    let world = World::with_town(Terrain::default(), &town);
    let (position, heading) = town.arrival();
    let crowd = Townsfolk::new(&town, TOWNSFOLK_COUNT, TOWNSFOLK_SEED);
    let train = Train::for_town(&town, TrainSchedule::default());
    App::new()
        .insert_resource(camera::CameraOccluders::from_town(&town, &world.terrain))
        .insert_resource(WorldTerrain(world.terrain))
        .insert_resource(life::Crowd(crowd))
        .insert_resource(WorldTrain(train))
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
            SignPlugin,
            AtmospherePlugin,
            TrainPlugin,
            CharacterVisualPlugin,
            player::PlayerPlugin,
            life::TownLifePlugin,
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
