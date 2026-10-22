//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod camera;
mod complaints;
mod errand;
mod life;
mod orbit;
mod player;
mod towns;

use bevy::prelude::*;
use pampa_content::{TOWNS_FILE, asset_path};
use pampa_core::{FloraSite, Terrain, World};
use pampa_render::{
    AssetRoot, AtmospherePlugin, CharacterVisualPlugin, FloraPlugin, HudPlugin, MinimapPlugin,
    RailPlugin, SignPlugin, SpeechBubblePlugin, StreamFocus, TerrainPlugin, TownPlugin,
    TrainPlugin, WorldChunks, WorldClock, WorldFlora, WorldRail, WorldTerrain, WorldTowns,
    WorldTrain,
};

fn main() {
    // One logic world feeds both the simulation and the drawing: the leveled
    // terrain, the town footprints and the bounds all come from `world`,
    // built from every town of `assets/world/towns.ron`.
    let loaded = towns::load_world(&asset_path(TOWNS_FILE));
    let report = towns::TownReport::new(&loaded);
    let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
    // The dirt roads, cut into the ground, with their level crossings and
    // signposts (thin obstacles).
    world.lay_roads(loaded.roads.clone());
    // Caldenes, chañares and grass between the towns, off the track and the
    // roads. Every
    // tree trunk of the world is an obstacle from the start, drawn or not.
    let flora = FloraSite::new(world.config, &world.terrain, &loaded.line);
    world.plant(&flora);
    // The errand: Don Hidalgo in the first town's plaza sends the player to
    // comisario Farach. Both stand in the way like any obstacle.
    let errand = errand::plan(&loaded.towns, &mut world);
    // The player gets off the morning train from Pico on foot, at the first town's
    // platform.
    let (position, heading) = errand::player_start(&loaded.towns[0]);
    let crowds = life::Crowds::for_towns(&loaded.towns);
    // The game begins in the late morning, with the train from Pico standing at the first town's
    // station; the clock starts there.
    let (clock, train) = life::start_of_day(&loaded.line, &loaded.towns);
    let mut app = App::new();
    app.insert_resource(camera::CameraOccluders::from_towns(
        &loaded.towns,
        &world.terrain,
    ))
    .insert_resource(WorldTerrain(world.terrain.clone()))
    .insert_resource(WorldChunks(world.config))
    .insert_resource(WorldFlora(flora))
    // The CC0 vegetation models and ground textures (see CREDITS.md).
    .insert_resource(AssetRoot(asset_path("")))
    .insert_resource(StreamFocus(position))
    .insert_resource(crowds)
    .insert_resource(WorldClock(clock))
    .insert_resource(WorldTrain(train))
    .insert_resource(WorldTowns(loaded.towns))
    // The railway line between towns, laid chunk by chunk.
    .insert_resource(WorldRail(loaded.line))
    .insert_resource(player::GameWorld(world))
    .insert_resource(player::PlayerSpawn {
        position,
        heading,
        travel: pampa_core::Travel::OnFoot,
    })
    .insert_resource(report);
    if let Some((run, cast)) = errand {
        app.insert_resource(run).insert_resource(cast);
    } else {
        warn!("no errand: the world needs a second town with a police station");
    }
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Pampa Seca".into(),
                ..default()
            }),
            ..default()
        }),
        TerrainPlugin::default(),
        FloraPlugin,
        TownPlugin,
        RailPlugin,
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
    // The errand and the clock and objective in the corner.
    .add_plugins((errand::ErrandPlugin, HudPlugin))
    .add_systems(Startup, towns::log_town_report)
    .add_systems(Update, quit_on_escape)
    .run();
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
