//! Pampa Seca: wires player input, `pampa-core` logic and `pampa-render` drawing.

mod abilities;
mod camera;
mod complaints;
// The cheat console: debug builds only, never in `--release`.
#[cfg(debug_assertions)]
mod console;
mod errand;
mod fauna;
mod first_person;
mod life;
mod orbit;
mod player;
mod ride;
#[cfg(test)]
mod settlement_tests;
mod towns;

use bevy::prelude::*;
use pampa_content::{SLOTS_FILE, TOWNS_FILE, asset_path, load_model_slots};
use pampa_core::{
    FaunaSite, FenceLayout, FloraSite, SettlementKind, SettlementLayout, StreetLamps, World,
};
use pampa_render::{
    AssetRoot, AtmospherePlugin, CharacterVisualPlugin, FadePlugin, FencePlugin, FloraPlugin,
    HudPlugin, LagoonPlugin, LampPlugin, MinimapPlugin, RailPlugin, RoadPlugin, SettlementPlugin,
    SignPlugin, SpeechBubblePlugin, StreamFocus, TerrainPlugin, ThinFadePlugin, TownPlugin,
    TrainPlugin, WorldChunks, WorldClock, WorldFences, WorldFlora, WorldLamps, WorldRail,
    WorldSettlements, WorldTerrain, WorldTowns, WorldTrains,
};

/// The fits of the third-party models in the model slots
/// (`assets/models/third-party/slots.ron`), for the renderer: yaw in
/// radians, rotor axes as directions. An invalid file fits nothing (every
/// model as it comes) and is reported.
fn model_slots(path: &std::path::Path) -> (pampa_render::ModelSlots, Option<String>) {
    let file = match load_model_slots(path) {
        Ok(file) => file,
        Err(error) => {
            return (
                pampa_render::ModelSlots::default(),
                Some(format!("{}: {error}; model slots unfitted", path.display())),
            );
        }
    };
    let fits = file
        .slots
        .into_iter()
        .map(|(slot, fit)| {
            let (x, y, z) = fit.offset;
            let rotor = fit.rotor.and_then(|r| {
                let (x, y, z) = r.axis;
                Some(pampa_render::SlotRotor {
                    node: r.node,
                    axis: Dir3::new(Vec3::new(x, y, z)).ok()?,
                })
            });
            let fit = pampa_render::SlotFit {
                scale: fit.scale,
                offset: Vec3::new(x, y, z),
                yaw: fit.yaw.to_radians(),
                rotor,
            };
            (slot, fit)
        })
        .collect();
    (pampa_render::ModelSlots(fits), None)
}

/// The towns that each get an estancia nearby (user, 2026-10-22), by
/// name in `assets/world/towns.ron`.
const RANCH_TOWNS: [&str; 2] = ["Eduardo Castex", "General Pico"];

/// The logic world and everything laid on it before the app starts: the
/// roads, the fences, the rural settlements and the flora, with what laying
/// them took for the startup log.
struct Laid {
    world: World,
    fences: FenceLayout,
    settlements: SettlementLayout,
    flora: FloraSite,
    lamps: StreetLamps,
    fauna: FaunaSite,
    report: LaidReport,
}

/// Counts and timings of [`Laid`], logged at startup.
#[derive(Debug, Clone)]
struct LaidReport {
    spans: usize,
    fenced: std::time::Duration,
    /// Huts, outposts, farms and ranches.
    kinds: [usize; 4],
    obstacles: usize,
    settled: std::time::Duration,
    /// Herds, head of cattle, corral horses, and how long seeding took.
    fauna: (usize, usize, usize, std::time::Duration),
    /// Towns of [`RANCH_TOWNS`] missing from the world or left without
    /// their estancia, ready to log as warnings.
    warnings: Vec<String>,
}

/// Builds the world of `loaded`: leveled for its towns, its roads cut in,
/// fenced, settled behind some open gates and planted around it all.
fn lay_out(loaded: &towns::LoadedWorld) -> Laid {
    // The lagoons are sunk first: the towns level their ground on top.
    let mut world = World::with_towns(loaded.world, loaded.terrain(), &loaded.towns);
    // The dirt roads, cut into the ground, with their level crossings and
    // signposts (thin obstacles).
    world.lay_roads(loaded.roads.clone());
    // The bays' platforms and foot crossings.
    world.lay_rail(&loaded.line);
    // Wire fences on the surveyed grid and along both sides of every road,
    // with gates into the fields; every span and shut gate is a thin
    // obstacle.
    let started = std::time::Instant::now();
    let fences = FenceLayout::build(world.config, &world.terrain, &loaded.towns, &loaded.line);
    let spans = world.fence(&fences);
    let fenced = started.elapsed();
    // Ranchos, puestos, chacras and estancias behind some open gates, with
    // their tracks from the gate; every building, prop, corral rail and tree
    // trunk of theirs is an obstacle.
    let started = std::time::Instant::now();
    // One estancia is always laid near each of the RANCH_TOWNS.
    let mut warnings = Vec::new();
    let ranch_towns: Vec<usize> = RANCH_TOWNS
        .iter()
        .filter_map(|name| {
            let found = loaded.towns.iter().position(|t| t.name == *name);
            if found.is_none() {
                warnings.push(format!("settlements: no town {name} for its estancia"));
            }
            found
        })
        .collect();
    let settlements = SettlementLayout::build_with_ranches(
        &world.terrain,
        &loaded.towns,
        &loaded.line,
        &fences,
        &ranch_towns,
    );
    warnings.extend(settlements.unplaced_ranches.iter().map(|&t| {
        format!(
            "settlements: no field near {} holds an estancia",
            loaded.towns[t].name
        )
    }));
    let obstacles = world.settle(&settlements);
    let settled = started.elapsed();
    let kinds = [
        SettlementKind::Hut,
        SettlementKind::Outpost,
        SettlementKind::Farm,
        SettlementKind::Ranch,
    ]
    .map(|k| settlements.count(k));
    // Caldenes, chañares and grass between the towns, off the track, the
    // roads, the fence lines and the settlements. Every tree trunk of the
    // world is an obstacle from the start, drawn or not.
    // Kept off the whole train path too: each town's own track and its
    // run-outs, not only the links between towns.
    // The branches' towns and run-outs too.
    let flora = loaded
        .line
        .track_paths(&loaded.towns)
        .iter()
        .fold(
            FloraSite::new(world.config, &world.terrain, &loaded.line),
            |flora, path| flora.with_track(path),
        )
        .with_fences(&fences)
        .with_settlements(&settlements);
    world.plant(&flora);
    // Kerosene street lamps round the plazas, at the town exits and street
    // corners and at the estancias, clear of everything above; each post
    // is a thin obstacle.
    let lamps = StreetLamps::place(&loaded.towns, &loaded.roads, &settlements, &world.obstacles);
    world.obstacles.extend(lamps.obstacles());
    // Cattle in some fenced fields, horses in the corrals, and the wild
    // animals seeded per chunk and lagoon, off everything above.
    let started = std::time::Instant::now();
    let path = loaded.line.full_path(&loaded.towns);
    let fauna = FaunaSite::new(
        world.config,
        &world.terrain,
        &fences,
        &settlements,
        path.points(),
    );
    let herds = fauna.herds();
    let fauna_counts = (
        herds.len(),
        herds.iter().map(|h| h.cows.len()).sum(),
        fauna.all_horses().count(),
        started.elapsed(),
    );
    Laid {
        world,
        fences,
        settlements,
        flora,
        lamps,
        fauna,
        report: LaidReport {
            spans,
            fenced,
            kinds,
            obstacles,
            settled,
            fauna: fauna_counts,
            warnings,
        },
    }
}

fn main() {
    // One logic world feeds both the simulation and the drawing: the leveled
    // terrain, the town footprints and the bounds all come from `world`,
    // built from every town of `assets/world/towns.ron`.
    let loaded = towns::load_world(&asset_path(TOWNS_FILE));
    let report = towns::TownReport::new(&loaded);
    let Laid {
        mut world,
        fences,
        settlements,
        flora,
        lamps,
        fauna,
        report: laid,
    } = lay_out(&loaded);
    let lamp_counts: Vec<String> = loaded
        .towns
        .iter()
        .zip(&lamps.towns)
        .map(|(t, l)| format!("{} {}", t.name, l.len()))
        .chain(std::iter::once(format!(
            "estancias {}",
            lamps.settlements.iter().map(Vec::len).sum::<usize>()
        )))
        .collect();
    // The errand: Don Hidalgo in the first town's plaza sends the player to
    // comisario Farach in the town the dialogue names. Both stand in the way
    // like any obstacle.
    let (dialogue, dialogue_report) = errand::read_dialogue();
    let errand = errand::plan(&loaded.towns, &mut world, &dialogue.0.police_town);
    let crowds = life::Crowds::for_towns(&loaded.towns);
    // The opening: the player rides the morning train from Pico, seated in
    // a coach as it pulls into the first town, and gets off there on foot.
    // Without a train they start on foot on the road into the first town.
    let (clock, trains, ride) = life::start_of_day(&loaded.line, &loaded.towns);
    let (position, heading) = ride
        .and_then(|ride| ride.seat(trains.get(ride.train)?))
        .unwrap_or_else(|| loaded.towns[0].arrival());
    let mut app = App::new();
    // The CC0 vegetation models, ground and building textures (see
    // CREDITS.md); the asset server reads glTF models and building textures
    // from the same folder.
    let root = AssetRoot(asset_path(""));
    let (slots, slots_warning) = model_slots(&asset_path(SLOTS_FILE));
    if let Some(warning) = slots_warning {
        warn!("{warning}");
    }
    app.insert_resource(
        camera::CameraOccluders::from_towns(&loaded.towns, &world.terrain)
            .with_settlements(&settlements, &world.terrain),
    )
    .insert_resource(WorldTerrain(world.terrain.clone()))
    .insert_resource(WorldChunks(world.config))
    .insert_resource(WorldFlora(flora))
    .insert_resource(WorldFences(fences))
    .insert_resource(WorldSettlements(settlements))
    .insert_resource(WorldLamps(lamps))
    .insert_resource(fauna::WorldFauna(fauna))
    .insert_resource(root.clone())
    .insert_resource(slots)
    .insert_resource(StreamFocus(position))
    .insert_resource(crowds)
    .insert_resource(WorldClock(clock))
    .insert_resource(WorldTrains(trains))
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
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Pampa Seca".into(),
                    ..default()
                }),
                ..default()
            })
            .set(pampa_render::gltf::asset_plugin(&root)),
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
    // The roads, their level crossings and signposts, the fences and gates,
    // and the rural settlements, chunk by chunk; trees, fences, signs and
    // thin props fade out of the camera's way.
    .add_plugins((RoadPlugin, FencePlugin, SettlementPlugin, ThinFadePlugin))
    // The kerosene street lamps, glowing by night, the nearest lit.
    .add_plugins(LampPlugin)
    // Cattle, corral horses, hares and birds round the player.
    .add_plugins((pampa_render::FaunaPlugin, fauna::FaunaPlugin))
    // The lagoons' water and reeds.
    .add_plugins(LagoonPlugin)
    // The errand and the clock and objective in the corner.
    .add_plugins((errand::ErrandPlugin, HudPlugin))
    // Boarding, riding, skipping and waiting for the train, with a fade to
    // cover jumps in time.
    .add_plugins((ride::RidePlugin, FadePlugin))
    .add_systems(Startup, towns::log_town_report)
    .add_systems(Startup, move || {
        let LaidReport {
            spans,
            fenced,
            kinds: [huts, outposts, farms, ranches],
            obstacles,
            settled,
            fauna: (herds, head, horses, seeded),
            ref warnings,
        } = laid;
        info!("fences: {spans} spans as obstacles, laid in {fenced:.1?}");
        for warning in warnings {
            warn!("{warning}");
        }
        info!(
            "settlements: {huts} ranchos, {outposts} puestos, {farms} chacras, \
             {ranches} estancias; {obstacles} obstacles, laid in {settled:.1?}"
        );
        info!("street lamps: {}", lamp_counts.join(", "));
        info!("fauna: {herds} herds ({head} head), {horses} corral horses; seeded in {seeded:.1?}");
    })
    .add_systems(Update, quit_on_escape);
    // Ctrl+Shift+F12: the cheat console (see `console`), in debug builds.
    #[cfg(debug_assertions)]
    app.add_plugins(console::ConsolePlugin);
    app.run();
}

fn quit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_world_is_settled_solid_and_seen_round() {
        let loaded = towns::load_world(&asset_path(TOWNS_FILE));
        let laid = lay_out(&loaded);
        let settlements = &laid.settlements;
        assert!(!settlements.settlements.is_empty());
        assert!(laid.report.kinds.iter().sum::<usize>() == settlements.settlements.len());
        // Every building, prop, rail and trunk is an obstacle of the world.
        assert_eq!(laid.report.obstacles, settlements.obstacles().count());
        for o in settlements.obstacles() {
            assert!(laid.world.obstacles.contains(&o), "{o:?}");
        }
        // No wild tree grows on a settlement: the flora keeps off them,
        // over every chunk each one reaches into.
        let size = laid.world.config.chunk_size;
        for s in &settlements.settlements {
            let (lo, hi) = (
                pampa_core::ChunkCoord::containing(s.bounds.min, size),
                pampa_core::ChunkCoord::containing(s.bounds.max, size),
            );
            for cx in lo.x..=hi.x {
                for cy in lo.y..=hi.y {
                    for t in laid.flora.trees(pampa_core::ChunkCoord { x: cx, y: cy }) {
                        assert!(settlements.is_clear(t.at, 0.0), "{t:?}");
                    }
                }
            }
        }
        // Every house and barn stops the camera.
        let occluders =
            camera::CameraOccluders::default().with_settlements(settlements, &laid.world.terrain);
        let buildings: Vec<_> = settlements
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_house() || s.kind == pampa_core::StructureKind::Barn)
            .collect();
        assert!(!buildings.is_empty());
        assert_eq!(occluders.0.len(), buildings.len());
        for b in buildings {
            assert!(
                occluders
                    .0
                    .iter()
                    .any(|o| o.footprint == b.footprint && o.top > o.bottom + b.height),
                "{b:?}"
            );
        }
        assert!(!laid.fences.gates.is_empty());
    }

    #[test]
    fn every_town_and_estancia_is_lit_by_lamps_standing_clear_and_solid() {
        let loaded = towns::load_world(&asset_path(TOWNS_FILE));
        let mut laid = lay_out(&loaded);
        let lamps = &laid.lamps;
        for (town, placed) in loaded.towns.iter().zip(&lamps.towns) {
            println!("{}: {} street lamps", town.name, placed.len());
            assert!(placed.len() >= 12, "{}: {}", town.name, placed.len());
        }
        for (s, placed) in laid.settlements.settlements.iter().zip(&lamps.settlements) {
            if s.kind == SettlementKind::Ranch {
                println!("estancia at {:?}: {} lamps", s.hub, placed.len());
                assert_eq!(placed.len(), 3, "{:?}", s.hub);
            }
        }
        // Every post is solid, and touches nothing else of the world.
        let posts: Vec<_> = lamps.obstacles().collect();
        for post in &posts {
            let others = laid
                .world
                .obstacles
                .near(*post)
                .into_iter()
                .filter(|o| *o != post && o.intersects(post))
                .count();
            assert_eq!(others, 0, "{post:?}");
        }
        // The errand's people and horse, placed after, stand clear of them.
        let (dialogue, _) = errand::read_dialogue();
        let before = laid.world.obstacles.len();
        errand::plan(&loaded.towns, &mut laid.world, &dialogue.0.police_town)
            .expect("the shipped world has its errand");
        let added = laid.world.obstacles.len() - before;
        assert!(added > 0);
        for post in &posts {
            let hits = laid
                .world
                .obstacles
                .near(*post)
                .into_iter()
                .filter(|o| *o != post && o.intersects(post))
                .count();
            assert_eq!(hits, 0, "the errand stands on {post:?}");
        }
    }

    #[test]
    fn laguna_la_arocena_keeps_everything_off_and_the_rider_out_of_deep_water() {
        let loaded = towns::load_world(&asset_path(TOWNS_FILE));
        let laid = lay_out(&loaded);
        let lagoon = laid
            .world
            .terrain
            .lagoons
            .first()
            .expect("the shipped lagoon");
        assert_eq!(lagoon.name, "Laguna La Arocena");
        let keep = lagoon.keep_out();
        let world = laid.world.config.bounds();
        assert!(keep.min.x > world.min.x && keep.max.x < world.max.x);
        // Its water lies flat in a basin below the land around it.
        let middle = laid.world.terrain.height(lagoon.center.x, lagoon.center.y);
        assert!(middle < lagoon.level - 1.0);
        for p in lagoon.ring(64, 1.0) {
            assert!(laid.world.terrain.height(p.x, p.y) > lagoon.level, "{p:?}");
        }
        // No fence, settlement or road inside its shore, bank and margin;
        // trees keep back from the water and grass from its mud.
        for o in laid.fences.obstacles() {
            assert!(!o.intersects(&keep), "fence {o:?}");
        }
        for s in &laid.settlements.settlements {
            assert!(!s.bounds.intersects(&keep), "{:?}", s.bounds);
        }
        // Only the public road out to it comes onto its bank.
        let roads = laid.world.terrain.roads.as_ref().unwrap();
        for road in roads
            .roads
            .iter()
            .filter(|r| r.kind != pampa_core::RoadKind::Lagoon)
        {
            for w in road.path.windows(2) {
                assert!(
                    keep.segment_overlap(w[0], w[1]).is_none(),
                    "road {:?}",
                    w[0]
                );
            }
        }
        let mut tufts = 0;
        let size = laid.world.config.chunk_size;
        let (lo, hi) = (
            pampa_core::ChunkCoord::containing(keep.min, size),
            pampa_core::ChunkCoord::containing(keep.max, size),
        );
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                let chunk = laid.flora.chunk(pampa_core::ChunkCoord { x: cx, y: cy });
                for t in &chunk.trees {
                    assert!(
                        lagoon.offshore(t.at) >= pampa_core::FloraSite::TREE_LAGOON_MARGIN,
                        "{t:?}"
                    );
                }
                // Grass grows down to the narrow wet mud, not on it.
                for t in &chunk.tufts {
                    assert!(!lagoon.is_bare(t.at), "{t:?}");
                    tufts +=
                        usize::from(lagoon.offshore(t.at) < pampa_core::Lagoon::MUD_BAND + 20.0);
                }
            }
        }
        assert!(tufts > 100, "{tufts} tufts near the shore");
        // Riding in from the west, the rider wades in and stops in the
        // shallows.
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        let mut at = lagoon.center - pampa_core::Vec2::new(lagoon.radii.x + 40.0, 0.0);
        for _ in 0..400 {
            at = laid
                .world
                .resolve(collider, at + pampa_core::Vec2::new(0.5, 0.0), |_| ());
            assert!(!lagoon.is_deep(at), "{at:?}");
        }
        assert!(lagoon.is_water(at));
    }

    #[test]
    fn a_public_road_leads_from_general_pico_to_the_lagoon_and_rides_clear() {
        let loaded = towns::load_world(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let laid = lay_out(&loaded);
        let lagoon = &laid.world.terrain.lagoons[0];
        let roads = laid.world.terrain.roads.as_ref().unwrap();
        let to_lagoon: Vec<_> = roads
            .roads
            .iter()
            .filter(|r| r.kind == pampa_core::RoadKind::Lagoon)
            .collect();
        assert_eq!(to_lagoon.len(), 1);
        let road = to_lagoon[0];
        // From General Pico, out of its east or south side.
        let pico = &loaded.towns[road.from];
        assert_eq!(pico.name, "General Pico");
        let grid = pico.grid();
        let start = road.path[0];
        let east = (start.x - grid.max.x).abs() < 1e-2;
        let south = (start.y - grid.min.y).abs() < 1e-2;
        assert!(east || south, "{start:?} on {grid:?}");
        // To the bank: never into the water or onto its mud, and ending by it.
        let end = road
            .path
            .iter()
            .map(|&p| lagoon.offshore(p))
            .fold(f32::MAX, f32::min);
        assert!(end > pampa_core::Lagoon::MUD_BAND && end < 15.0, "{end}");
        // A rider rides it end to end: no fence, gate, settlement, tree or
        // signpost stands on it.
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        for w in road.path.windows(2) {
            for k in 0..4 {
                let p = w[0] + (w[1] - w[0]) * (k as f32 / 4.0);
                assert_eq!(laid.world.resolve(collider, p, |_| ()), p, "{p:?}");
            }
        }
        // A fence each side, as along every road, out of town and off the
        // lagoon's bank.
        let half = pampa_core::RoadNetwork::HALF_WIDTH;
        let fenced = laid
            .fences
            .obstacles()
            .iter()
            .filter(|o| {
                road.path.windows(2).any(|w| {
                    let mid = (w[0] + w[1]) * 0.5;
                    o.closest_point(mid).distance(mid) < half + 8.0
                })
            })
            .count();
        assert!(fenced > 50, "{fenced} fence spans along the road");
    }

    #[test]
    fn castex_and_pico_each_have_an_estancia_nearby() {
        use pampa_core::{SettlementKind, StructureKind};
        let loaded = towns::load_world(&asset_path(TOWNS_FILE));
        let laid = lay_out(&loaded);
        // Laid the same way every time.
        assert_eq!(lay_out(&loaded).settlements, laid.settlements);
        for name in RANCH_TOWNS {
            let town = loaded
                .towns
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("{name} in the shipped towns"));
            let band = town.leveled();
            let ranches: Vec<_> = laid
                .settlements
                .settlements
                .iter()
                .filter(|s| s.kind == SettlementKind::Ranch)
                .filter(|s| {
                    let gate = laid.fences.gates[s.gate as usize].at;
                    band.closest_point(gate).distance(gate) <= 3000.0
                })
                .collect();
            assert!(!ranches.is_empty(), "no estancia within 3 km of {name}");
            assert!(
                laid.report.warnings.is_empty(),
                "{:?}",
                laid.report.warnings
            );
            assert!(laid.settlements.unplaced_ranches.is_empty());
            for r in ranches {
                let workers = r
                    .structures
                    .iter()
                    .filter(|s| matches!(s.kind, StructureKind::WorkerHouse(_)))
                    .count();
                assert!(workers >= 2, "{name}: {workers} worker houses");
                let area = r.bounds.size().x * r.bounds.size().y;
                assert!(area >= 20_000.0, "{name}: {area} m²");
                assert!(r.of(StructureKind::Barn).count() >= 1, "{name}: no barn");
                for st in r.structures.iter().filter(|s| s.kind.is_reached()) {
                    let door = st.door();
                    assert!(
                        r.tracks.iter().any(|t| t.points.last() == Some(&door)),
                        "{name}: no track to {st:?}"
                    );
                }
            }
        }
    }
}
