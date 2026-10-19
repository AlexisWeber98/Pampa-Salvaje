//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod camera;
mod complaints;
mod life;
mod orbit;
mod player;
mod towns;

use bevy::prelude::*;
use pampa_content::{TOWNS_FILE, asset_path};
use pampa_core::{Terrain, Train, TrainSchedule, World};
use pampa_render::{
    AtmospherePlugin, CharacterVisualPlugin, MinimapPlugin, SignPlugin, SpeechBubblePlugin,
    StreamFocus, TerrainPlugin, TownPlugin, TrainPlugin, WorldChunks, WorldTerrain, WorldTowns,
    WorldTrains,
};

fn main() {
    // One logic world feeds both the simulation and the drawing: the leveled
    // terrain, the town footprints and the bounds all come from `world`,
    // built from every town of `assets/world/towns.ron`.
    let loaded = towns::load_world(&asset_path(TOWNS_FILE));
    let report = towns::TownReport::new(&loaded);
    let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
    // The player starts on the road into the first town.
    let (position, heading) = loaded.towns[0].arrival();
    let crowds = life::Crowds::for_towns(&loaded.towns);
    let trains = loaded
        .towns
        .iter()
        .map(|town| Train::for_town(town, TrainSchedule::default()))
        .collect();
    let mut app = App::new();
    app.insert_resource(camera::CameraOccluders::from_towns(
        &loaded.towns,
        &world.terrain,
    ))
    .insert_resource(WorldTerrain(world.terrain.clone()))
    .insert_resource(WorldChunks(world.config))
    .insert_resource(StreamFocus(position))
    .insert_resource(crowds)
    .insert_resource(WorldTrains(trains))
    .insert_resource(WorldTowns(loaded.towns))
    .insert_resource(player::GameWorld(world))
    .insert_resource(player::PlayerSpawn { position, heading })
    .insert_resource(report)
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
        MinimapPlugin,
        AtmospherePlugin,
        TrainPlugin,
        CharacterVisualPlugin,
        SpeechBubblePlugin,
        player::PlayerPlugin,
        life::TownLifePlugin,
        complaints::ComplaintsPlugin,
        camera::FollowCameraPlugin,
    ))
    .add_systems(Startup, towns::log_town_report)
    .add_systems(Update, quit_on_escape)
    .run();
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
