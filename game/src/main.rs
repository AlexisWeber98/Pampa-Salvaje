//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod camera;
mod complaints;
mod errand;
mod life;
mod orbit;
mod player;
mod ride;
mod towns;

use bevy::prelude::*;
use pampa_content::{TOWNS_FILE, asset_path};
use pampa_core::{FenceLayout, FloraSite, Terrain, World};
use pampa_render::{
    AssetRoot, AtmospherePlugin, CharacterVisualPlugin, FadePlugin, FencePlugin, FloraPlugin,
    HudPlugin, MinimapPlugin, RailPlugin, RoadPlugin, SignPlugin, SpeechBubblePlugin, StreamFocus,
    TerrainPlugin, ThinFadePlugin, TownPlugin, TrainPlugin, WorldChunks, WorldClock, WorldFences,
    WorldFlora, WorldRail, WorldTerrain, WorldTowns, WorldTrain,
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
    // Wire fences on the surveyed grid and along both sides of every road,
    // with gates into the fields; every span and shut gate is a thin
    // obstacle.
    let started = std::time::Instant::now();
    let fences = FenceLayout::build(world.config, &world.terrain, &loaded.towns, &loaded.line);
    let spans = world.fence(&fences);
    let fenced = started.elapsed();
    // Caldenes, chañares and grass between the towns, off the track, the
    // roads and the fence lines. Every tree trunk of the world is an
    // obstacle from the start, drawn or not.
    // Kept off the whole train path too: each town's own track and its
    // run-outs, not only the links between towns.
    let flora = FloraSite::new(world.config, &world.terrain, &loaded.line)
        .with_track(&loaded.line.full_path(&loaded.towns))
        .with_fences(&fences);
    world.plant(&flora);
    // The errand: Don Hidalgo in the first town's plaza sends the player to
    // comisario Farach in the town the dialogue names. Both stand in the way
    // like any obstacle.
    let (dialogue, dialogue_report) = errand::read_dialogue();
    let errand = errand::plan(&loaded.towns, &mut world, &dialogue.0.police_town);
    let crowds = life::Crowds::for_towns(&loaded.towns);
    // The opening: the player rides the morning train from Pico, seated in
    // a coach as it pulls into the first town, and gets off there on foot.
    // Without a train they start on foot on the road into the first town.
    let (clock, train, ride) = life::start_of_day(&loaded.line, &loaded.towns);
    let (position, heading) = ride
        .zip(train.as_ref())
        .and_then(|(ride, train)| ride.seat(train))
        .unwrap_or_else(|| loaded.towns[0].arrival());
    let mut app = App::new();
    app.insert_resource(camera::CameraOccluders::from_towns(
        &loaded.towns,
        &world.terrain,
    ))
    .insert_resource(WorldTerrain(world.terrain.clone()))
    .insert_resource(WorldChunks(world.config))
    .insert_resource(WorldFlora(flora))
    .insert_resource(WorldFences(fences))
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
    .insert_resource(ride::StartAboard(ride))
    .insert_resource(report)
    .insert_resource(dialogue)
    .insert_resource(dialogue_report);
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
    // The roads, their level crossings and signposts, and the fences and
    // gates, chunk by chunk; trees, fences and signs fade out of the
    // camera's way.
    .add_plugins((RoadPlugin, FencePlugin, ThinFadePlugin))
    // The errand and the clock and objective in the corner.
    .add_plugins((errand::ErrandPlugin, HudPlugin))
    // Boarding, riding, skipping and waiting for the train, with a fade to
    // cover jumps in time.
    .add_plugins((ride::RidePlugin, FadePlugin))
    .add_systems(Startup, towns::log_town_report)
    .add_systems(Startup, move || {
        info!("fences: {spans} spans as obstacles, laid in {fenced:.1?}");
    })
    .add_systems(Update, quit_on_escape)
    .run();
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
