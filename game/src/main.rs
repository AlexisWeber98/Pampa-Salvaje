//! Pampa Salvaje: conecta la entrada del jugador, la lógica de `pampa-core` y
//! el dibujo de `pampa-render`.

mod abilities;
mod camera;
mod complaints;
mod shots;
mod sound;
// La consola de trucos: solo en compilaciones de depuración, nunca en
// `--release`.
#[cfg(debug_assertions)]
mod console;
mod errand;
mod fauna;
mod first_person;
mod life;
mod menu;
mod orbit;
mod plant_collision;
mod player;
mod ride;
#[cfg(test)]
mod settlement_tests;
mod timetable;
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

/// Los ajustes de los modelos de terceros en las ranuras de modelo
/// (`assets/models/third-party/slots.ron`), para el renderer: yaw en radianes,
/// ejes de rotor como direcciones. Un archivo no válido no ajusta nada (cada
/// modelo tal como viene) y se informa.
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

/// Los pueblos que reciben cada uno una estancia cerca (usuario, 2026-10-22),
/// por nombre en `assets/world/towns.ron`.
const RANCH_TOWNS: [&str; 2] = ["Eduardo Castex", "General Pico"];

/// El mundo lógico y todo lo que se coloca sobre él antes de que arranque la
/// app: los caminos, los alambrados, los asentamientos rurales y la flora, con
/// lo que llevó colocarlos para el log de inicio.
struct Laid {
    world: World,
    fences: FenceLayout,
    settlements: SettlementLayout,
    flora: FloraSite,
    lamps: StreetLamps,
    fauna: FaunaSite,
    report: LaidReport,
}

/// Cantidades y tiempos de [`Laid`], registrados en el log al inicio.
#[derive(Debug, Clone)]
struct LaidReport {
    spans: usize,
    fenced: std::time::Duration,
    /// Ranchos, puestos, chacras y estancias.
    kinds: [usize; 4],
    obstacles: usize,
    settled: std::time::Duration,
    /// Manadas, cabezas de ganado, caballos de corral, y cuánto llevó
    /// sembrarlos.
    fauna: (usize, usize, usize, std::time::Duration),
    /// Pueblos de [`RANCH_TOWNS`] que faltan en el mundo o quedaron sin su
    /// estancia, listos para registrarse como avisos.
    warnings: Vec<String>,
}

/// Construye el mundo de `loaded`: nivelado para sus pueblos, con sus caminos
/// trazados, alambrado, poblado detrás de algunas tranqueras abiertas y
/// plantado alrededor de todo.
fn lay_out(loaded: &towns::LoadedWorld) -> Laid {
    // Las lagunas se hunden primero: los pueblos nivelan su suelo encima.
    let mut world = World::with_towns(loaded.world, loaded.terrain(), &loaded.towns);
    // Los caminos de tierra, trazados en el suelo, con sus pasos a nivel y sus
    // carteles indicadores (obstáculos delgados).
    world.lay_roads(loaded.roads.clone());
    // Los andenes de las vías muertas y los cruces peatonales.
    world.lay_rail(&loaded.line);
    // Alambrados sobre la cuadrícula de la mensura y a ambos lados de cada
    // camino, con tranqueras hacia los campos; cada tramo y cada tranquera
    // cerrada es un obstáculo delgado.
    let started = std::time::Instant::now();
    let fences = FenceLayout::build(world.config, &world.terrain, &loaded.towns, &loaded.line);
    let spans = world.fence(&fences);
    let fenced = started.elapsed();
    // Ranchos, puestos, chacras y estancias detrás de algunas tranqueras
    // abiertas, con sus huellas desde la tranquera; cada edificio, objeto,
    // travesaño de corral y tronco de árbol suyo es un obstáculo.
    let started = std::time::Instant::now();
    // Siempre se coloca una estancia cerca de cada uno de los RANCH_TOWNS.
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
    // Caldenes, chañares y pasto entre los pueblos, fuera de la vía, los
    // caminos, las líneas de alambrado y los asentamientos. Los troncos de
    // los árboles son obstáculos solo en los chunks alrededor del jugador
    // (ver `plant_collision`).
    // También fuera de todo el recorrido del tren: la vía propia de cada
    // pueblo y sus prolongaciones, no solo los tramos entre pueblos.
    // También los pueblos y las prolongaciones de los ramales.
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
    // Faroles de kerosén alrededor de las plazas, en las salidas de los
    // pueblos, en las esquinas y en las estancias, apartados de todo lo
    // anterior; cada poste es un obstáculo delgado.
    let lamps = StreetLamps::place(&loaded.towns, &loaded.roads, &settlements, &world.obstacles);
    world.obstacles.extend(lamps.obstacles());
    // Ganado en algunos campos alambrados, caballos en los corrales, y los
    // animales silvestres sembrados por chunk y por laguna, fuera de todo lo
    // anterior.
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
    // Un único mundo lógico alimenta tanto la simulación como el dibujo: el
    // terreno nivelado, las huellas en planta de los pueblos y los límites
    // salen de `world`, construido con todos los pueblos de
    // `assets/world/towns.ron`.
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
    // El encargo: Don Hidalgo, en la plaza del primer pueblo, manda al jugador
    // a ver al comisario Farach en el pueblo que nombra el diálogo. Ambos
    // estorban el paso como cualquier obstáculo.
    let (dialogue, dialogue_report) = errand::read_dialogue();
    let errand = errand::plan(&loaded.towns, &mut world, &dialogue.0.police_town);
    let crowds = life::Crowds::for_towns(&loaded.towns);
    // La apertura: el jugador viaja en el tren de la mañana desde Pico, sentado
    // en un coche mientras entra al primer pueblo, y allí se baja a pie. Sin
    // tren, empieza a pie en el camino de entrada al primer pueblo.
    let (clock, trains, ride) = life::start_of_day(&loaded.line, &loaded.towns);
    let (position, heading) = ride
        .and_then(|ride| ride.seat(trains.get(ride.train)?))
        .unwrap_or_else(|| loaded.towns[0].arrival());
    let mut app = App::new();
    // Los modelos de vegetación CC0 y las texturas del suelo y de los edificios
    // (ver CREDITS.md); el servidor de assets lee los modelos glTF y las
    // texturas de los edificios de la misma carpeta.
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
    // La línea del ferrocarril entre pueblos, colocada chunk por chunk.
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
                    title: "Pampa Salvaje".into(),
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
    // Los caminos, sus pasos a nivel y carteles indicadores, los alambrados y
    // las tranqueras, y los asentamientos rurales, chunk por chunk; los
    // árboles, los alambrados, los carteles y los objetos delgados se
    // desvanecen para no tapar la cámara.
    .add_plugins((RoadPlugin, FencePlugin, SettlementPlugin, ThinFadePlugin))
    // Los faroles de kerosén, que brillan de noche, con los más cercanos
    // encendidos.
    .add_plugins(LampPlugin)
    // Los árboles colisionan en los chunks alrededor del jugador.
    .add_plugins(plant_collision::PlantCollisionPlugin)
    // El material del suelo: el estándar más la hojarasca del monte.
    .add_plugins(pampa_render::ground::GroundMaterialPlugin)
    // La bóveda del cielo (sol, luna, estrellas, nubes) y los pájaros que
    // vuelan en ella.
    .add_plugins(pampa_render::SkyPlugin)
    // Ganado, caballos de corral, liebres y aves alrededor del jugador.
    .add_plugins((pampa_render::FaunaPlugin, fauna::FaunaPlugin))
    // El agua y los juncos de las lagunas.
    .add_plugins(LagoonPlugin)
    // El encargo, y el reloj y el objetivo en la esquina.
    .add_plugins((errand::ErrandPlugin, timetable::TimetablePlugin, HudPlugin))
    // Subir, viajar, omitir y esperar el tren, con un fundido para tapar los
    // saltos en el tiempo.
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
    });
    // PAMPA_SHOTS=<file>: un recorrido de capturas para desarrollo (ver
    // `shots`), que omite la pantalla de título.
    let tour = shots::ShotsPlugin::from_env();
    // La pantalla de título y el menú de pausa de Esc, con su música.
    app.add_plugins((
        pampa_render::TypefacePlugin,
        menu::MenuPlugin {
            title: tour.is_none(),
            settings: sound::settings_path(),
        },
    ));
    // Ctrl+Shift+F12: la consola de trucos (ver `console`), en compilaciones de
    // depuración.
    #[cfg(debug_assertions)]
    app.add_plugins(console::ConsolePlugin);
    if let Some(tour) = tour {
        app.add_plugins(tour);
    }
    app.run();
}

/// El mundo incluido, cargado y colocado una sola vez para los tests que solo
/// lo leen; un test que lo cambia clona lo que cambia.
#[cfg(test)]
mod shipped_world {
    use super::*;
    use std::sync::LazyLock;

    static LOADED: LazyLock<towns::LoadedWorld> =
        LazyLock::new(|| towns::load_world(&asset_path(TOWNS_FILE)));
    static LAID: LazyLock<Laid> = LazyLock::new(|| lay_out(loaded()));

    /// El archivo de pueblos incluido, cargado.
    pub(crate) fn loaded() -> &'static towns::LoadedWorld {
        &LOADED
    }

    /// [`loaded`] dispuesto como lo dispone el juego.
    pub(crate) fn laid() -> &'static Laid {
        &LAID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_world_is_settled_solid_and_seen_round_with_a_few_wrecks_off_its_roads() {
        let laid = shipped_world::laid();
        let settlements = &laid.settlements;
        assert!(!settlements.settlements.is_empty());
        assert!(laid.report.kinds.iter().sum::<usize>() == settlements.settlements.len());
        // Cada edificio, objeto, travesaño y tronco es un obstáculo del mundo.
        assert_eq!(laid.report.obstacles, settlements.obstacles().count());
        for o in settlements.obstacles() {
            assert!(laid.world.obstacles.contains(&o), "{o:?}");
        }
        // Ningún árbol silvestre crece sobre un asentamiento: la flora se
        // mantiene fuera de ellos, en cada chunk al que llega cada uno.
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
        // Cada casa y cada galpón detienen la cámara.
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
        // Algunos restos abandonados yacen en los patios y en las banquinas,
        // fuera de los caminos, sólidos y sin ningún árbol silvestre encima.
        let roads = laid.world.terrain.roads.as_deref().expect("roads laid");
        let yards = settlements
            .settlements
            .iter()
            .flat_map(|s| s.of(pampa_core::StructureKind::WreckedWagon))
            .count();
        println!(
            "wrecks: {yards} in yards, {} on verges",
            settlements.roadside.len()
        );
        assert!(settlements.roadside.len() <= pampa_core::SettlementLayout::ROADSIDE_WRECKS);
        assert!(yards + settlements.roadside.len() >= 1, "no wreck anywhere");
        for wreck in &settlements.roadside {
            for c in wreck.corners() {
                assert!(
                    roads.distance(c) > pampa_core::RoadNetwork::HALF_WIDTH,
                    "{wreck:?}"
                );
            }
            for o in wreck.obstacles() {
                assert!(laid.world.obstacles.contains(&o));
            }
            let chunk = laid.world.config.chunk_of(wreck.at);
            for t in laid.flora.trees(chunk) {
                assert!(
                    !wreck.footprint().expanded(0.5).contains_point(t.at),
                    "{t:?}"
                );
            }
        }
    }

    #[test]
    fn every_town_and_estancia_is_lit_by_lamps_standing_clear_and_solid() {
        let loaded = shipped_world::loaded();
        let laid = shipped_world::laid();
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
        // Cada poste es sólido y no toca nada más del mundo.
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
        // Las personas y el caballo del encargo, colocados después, quedan
        // apartados de ellos.
        let (dialogue, _) = errand::read_dialogue();
        let before = laid.world.obstacles.len();
        let mut world = laid.world.clone();
        errand::plan(&loaded.towns, &mut world, &dialogue.0.police_town)
            .expect("the shipped world has its errand");
        let added = world.obstacles.len() - before;
        assert!(added > 0);
        for post in &posts {
            let hits = world
                .obstacles
                .near(*post)
                .into_iter()
                .filter(|o| *o != post && o.intersects(post))
                .count();
            assert_eq!(hits, 0, "the errand stands on {post:?}");
        }
        // Cada farol queda apartado de todos los árboles, para que no necesite
        // ver los árboles al inicio (se plantan a medida que el jugador se
        // acerca).
        let mut trees = std::collections::HashMap::new();
        for lamp in laid.lamps.all() {
            let reach = lamp.obstacle().expanded(pampa_core::StreetLamps::CLEARANCE);
            let here = laid.world.config.chunk_of(lamp.obstacle().center());
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let c = pampa_core::ChunkCoord::new(here.x + dx, here.y + dy);
                    let near = trees.entry(c).or_insert_with(|| laid.flora.trees(c));
                    for t in near.iter() {
                        assert!(!t.obstacle().intersects(&reach), "{lamp:?} on {t:?}");
                    }
                }
            }
        }
        let lamps = laid.lamps.all().count();
        assert!(lamps > 50, "{lamps} lamps");
    }

    #[test]
    fn laguna_la_arocena_keeps_everything_off_and_the_rider_out_of_deep_water() {
        let laid = shipped_world::laid();
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
        // Su agua yace plana en una cuenca por debajo del terreno que la rodea.
        let middle = laid.world.terrain.height(lagoon.center.x, lagoon.center.y);
        assert!(middle < lagoon.level - 1.0);
        for p in lagoon.ring(64, 1.0) {
            assert!(laid.world.terrain.height(p.x, p.y) > lagoon.level, "{p:?}");
        }
        // Ningún alambrado, asentamiento ni camino dentro de su orilla, su
        // ribera y su margen; los árboles se mantienen lejos del agua y el
        // pasto, de su barro.
        for o in laid.fences.obstacles() {
            assert!(!o.intersects(&keep), "fence {o:?}");
        }
        for s in &laid.settlements.settlements {
            assert!(!s.bounds.intersects(&keep), "{:?}", s.bounds);
        }
        // Solo el camino público que llega hasta ella entra en su ribera.
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
                // El pasto crece hasta el barro húmedo angosto, no sobre él.
                for t in &chunk.tufts {
                    assert!(!lagoon.is_bare(t.at), "{t:?}");
                    tufts +=
                        usize::from(lagoon.offshore(t.at) < pampa_core::Lagoon::MUD_BAND + 20.0);
                }
            }
        }
        assert!(tufts > 100, "{tufts} tufts near the shore");
        // Entrando a caballo desde el oeste, el jinete se mete en el agua y se
        // detiene en lo poco profundo.
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
        let loaded = shipped_world::loaded();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let laid = shipped_world::laid();
        let lagoon = &laid.world.terrain.lagoons[0];
        let roads = laid.world.terrain.roads.as_ref().unwrap();
        let to_lagoon: Vec<_> = roads
            .roads
            .iter()
            .filter(|r| r.kind == pampa_core::RoadKind::Lagoon)
            .collect();
        assert_eq!(to_lagoon.len(), 1);
        let road = to_lagoon[0];
        // Desde General Pico, saliendo por su lado este o sur.
        let pico = &loaded.towns[road.from];
        assert_eq!(pico.name, "General Pico");
        let grid = pico.grid();
        let start = road.path[0];
        let east = (start.x - grid.max.x).abs() < 1e-2;
        let south = (start.y - grid.min.y).abs() < 1e-2;
        assert!(east || south, "{start:?} on {grid:?}");
        // Hasta la ribera: nunca dentro del agua ni sobre su barro, y
        // terminando junto a ella.
        let end = road
            .path
            .iter()
            .map(|&p| lagoon.offshore(p))
            .fold(f32::MAX, f32::min);
        assert!(end > pampa_core::Lagoon::MUD_BAND && end < 15.0, "{end}");
        // Un jinete lo recorre de punta a punta: ningún alambrado, tranquera,
        // asentamiento, árbol ni cartel indicador está sobre él.
        // Los árboles se plantan a medida que un jinete se acerca (ver
        // `plant_collision`): se plantan los chunks que cruza el camino y el
        // anillo completo (3x3, diagonales incluidas) que el juego carga
        // alrededor del jinete.
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        let mut planted = laid.world.clone();
        for p in &road.path {
            let here = planted.config.chunk_of(*p);
            let r = plant_collision::LOAD_RADIUS;
            for dy in -r..=r {
                for dx in -r..=r {
                    let c = pampa_core::ChunkCoord::new(here.x + dx, here.y + dy);
                    planted.plant_chunk(&laid.flora, c);
                }
            }
        }
        assert!(planted.obstacles.streamed_len() > 0);
        for w in road.path.windows(2) {
            for k in 0..4 {
                let p = w[0] + (w[1] - w[0]) * (k as f32 / 4.0);
                assert_eq!(planted.resolve(collider, p, |_| ()), p, "{p:?}");
            }
        }
        // Un alambrado a cada lado, como a lo largo de todo camino, fuera del
        // pueblo y fuera de la ribera de la laguna.
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
        let loaded = shipped_world::loaded();
        let laid = shipped_world::laid();
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
