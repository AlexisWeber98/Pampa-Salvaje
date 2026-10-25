//! Builds the world and its towns from `assets/world/towns.ron`.
//!
//! The towns are needed before the app starts (the leveled terrain, the
//! crowds and the trains all come from them), when logging is not up yet,
//! so problems are kept in [`TownReport`] and logged at startup.

use std::path::Path;

use bevy::prelude::*;
use pampa_content::load_towns_or_builtin;
use pampa_core::{
    Lagoon, PlaceKind, RailLine, RoadNetwork, Terrain, TownLayout, WorldConfig, place_lagoons,
    place_towns,
};
use pampa_render::font;

/// The world to build, its towns, and what went wrong reading them.
#[derive(Debug, Clone)]
pub struct LoadedWorld {
    pub world: WorldConfig,
    /// Every valid town, in file order; never empty. The player starts at
    /// the first one.
    pub towns: Vec<TownLayout>,
    /// The railway line through the towns, its links checked against them.
    pub line: RailLine,
    /// The dirt roads between neighbouring towns, and each railway town's
    /// rural track across its level crossing.
    pub roads: RoadNetwork,
    /// The lagoons, clear of the towns, the railway and the roads, their
    /// water set against the default terrain.
    pub lagoons: Vec<Lagoon>,
    /// Problems found, ready to log as warnings.
    pub warnings: Vec<String>,
}

/// Loads the towns file at `path` and places every valid town in its world,
/// falling back to the built-in Eduardo Castex when no town can be built,
/// then runs the railway line and the roads through them and lays the
/// lagoons clear of them all.
pub fn load_world(path: &Path) -> LoadedWorld {
    let (content, errors) = load_towns_or_builtin(path);
    let mut warnings: Vec<String> = errors.iter().map(|e| format!("towns: {e}")).collect();
    let mut world = content.world;
    let mut names = content.line;
    let mut branches = content.branches;
    // Content already placed these towns once; this only builds them.
    let (mut towns, rejected) = place_towns(&world, &content.towns);
    warnings.extend(rejected.iter().map(|e| format!("towns: {e}")));
    if towns.is_empty() {
        // The file's line names towns that are gone.
        names = None;
        branches.clear();
        warnings.push(
            "towns: no town fits the world; using the built-in Eduardo Castex in the default world"
                .to_string(),
        );
        world = WorldConfig::default();
        towns = vec![TownLayout::castex_inspired()];
    }
    for town in &towns {
        warnings.extend(unsupported_glyphs(town));
    }
    let (line, broken) =
        RailLine::build_with_branches(names.as_deref(), &branches, &towns, world.bounds());
    warnings.extend(broken.iter().map(|e| format!("towns: {e}")));
    let (roads, left_out) = RoadNetwork::build(&towns, &line, world.bounds());
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    let (lagoons, left_out) = place_lagoons(
        &world,
        &Terrain::default(),
        &towns,
        &line,
        &roads,
        &content.lagoons,
    );
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    // A public road out to each lagoon from the nearest town.
    let (roads, left_out) = roads.with_lagoons(&towns, &line, world.bounds(), &lagoons);
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    LoadedWorld {
        world,
        towns,
        line,
        roads,
        lagoons,
        warnings,
    }
}

/// A warning for each station sign of `town` the font cannot fully draw.
fn unsupported_glyphs(town: &TownLayout) -> Vec<String> {
    let stations = town
        .places()
        .into_iter()
        .filter(|p| p.kind == PlaceKind::Station);
    let mut warnings = Vec::new();
    for place in stations {
        let mut unsupported: Vec<char> = Vec::new();
        for c in place.name.chars().filter(|&c| !font::supports(c)) {
            if !unsupported.contains(&c) {
                unsupported.push(c);
            }
        }
        if !unsupported.is_empty() {
            warnings.push(format!(
                "towns: the station sign \"{}\" uses characters the font cannot draw: {unsupported:?}",
                place.name
            ));
        }
    }
    warnings
}

/// What loading the towns reported, logged once at startup.
#[derive(Resource, Debug, Clone, Default)]
pub struct TownReport {
    pub names: Vec<String>,
    pub warnings: Vec<String>,
}

impl LoadedWorld {
    /// The default terrain with every lagoon sunk into it, before the towns
    /// are leveled.
    pub fn terrain(&self) -> Terrain {
        self.lagoons
            .iter()
            .fold(Terrain::default(), |t, l| t.with_lagoon(l.clone()))
    }
}

impl TownReport {
    pub fn new(loaded: &LoadedWorld) -> Self {
        Self {
            names: loaded.towns.iter().map(|t| t.name.clone()).collect(),
            warnings: loaded.warnings.clone(),
        }
    }
}

/// Logs the towns that were built and any problems loading them.
pub fn log_town_report(report: Res<TownReport>) {
    for warning in &report.warnings {
        warn!("{warning}");
    }
    info!(
        "towns: {} built: {}",
        report.names.len(),
        report.names.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_content::{TOWNS_FILE, asset_path};
    use pampa_core::{BuildingKind, Terrain, TownSize, Townsfolk, Vec2, World};

    fn temp_towns(name: &str, text: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pampa-game-towns-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("towns.ron");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_shipped_file_builds_monte_nievas_first_without_warnings() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.world.half_extent, WorldConfig::MAX_HALF_EXTENT);
        let names: Vec<&str> = loaded.towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Monte Nievas",
                "Eduardo Castex",
                "Metileo",
                "General Pico",
                "Caleufú"
            ]
        );
        // General Pico is very big: a 2 x 2 plaza, a station plaza, two
        // general stores, two police stations and the 31 people its town
        // file sets (its size alone would get 51).
        let pico = &loaded.towns[3];
        assert_eq!(pico.size, TownSize::VeryBig);
        assert!(pico.station_plaza.is_some());
        let count = |kind: PlaceKind| pico.places().iter().filter(|p| p.kind == kind).count();
        assert_eq!(count(PlaceKind::GeneralStore), 2);
        assert_eq!(count(PlaceKind::PoliceStation), 2);
        assert_eq!(count(PlaceKind::Plaza), 2);
        assert_eq!(pico.townsfolk_count(), 31);
        for small in loaded.towns.iter().filter(|t| t.name != "General Pico") {
            assert_eq!(small.size, TownSize::Small, "{}", small.name);
            assert_eq!(small.station_plaza, None, "{}", small.name);
            assert_eq!(small.townsfolk_count(), 10, "{}", small.name);
        }
        // One railway line runs Castex, Monte Nievas, Metileo, General Pico,
        // and a branch leaves it at Metileo for Caleufú.
        assert_eq!(loaded.line.towns, [1, 0, 2, 3]);
        let links: Vec<(usize, usize)> = loaded.line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(links, [(1, 0), (0, 2), (2, 3), (2, 4)]);
        assert_eq!(loaded.line.branches.len(), 1);
        assert_eq!(loaded.line.branches[0].towns, [2, 4]);
        // The small towns' stores are pulperías; Pico's, almacenes de ramos
        // generales.
        for town in &loaded.towns {
            let expected = if town.size == TownSize::Small {
                "PULPERÍA"
            } else {
                "ALMACÉN DE RAMOS GENERALES"
            };
            for store in town
                .places()
                .iter()
                .filter(|p| p.kind == PlaceKind::GeneralStore)
            {
                assert_eq!(store.name, expected, "{}", town.name);
            }
        }
        // Laguna La Arocena is laid, clear of everything.
        assert_eq!(loaded.lagoons.len(), 1);
        assert_eq!(loaded.lagoons[0].name, "Laguna La Arocena");
    }

    #[test]
    fn general_pico_grows_on_both_sides_of_the_line_through_its_middle() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        let pico = &loaded.towns[3];
        let rail = pico.railway.expect("a railway");
        let band = pico.rail_band().expect("the line runs through Pico");
        let grid = pico.grid();
        // The track runs east-west through the town's core, not along its
        // edge: within a block of the middle row, inside the band.
        let y = rail.start.y;
        assert_eq!(rail.end.y, y);
        assert!(
            (y - grid.center().y).abs() < 40.0,
            "{y} vs {:?}",
            grid.center()
        );
        assert!(band.min.y < y && y < band.max.y);
        assert!(rail.start.x < grid.min.x && rail.end.x > grid.max.x);
        // Houses on both sides, and nothing built on the track.
        let houses = |north: bool| {
            pico.buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .filter(|b| (b.footprint.center().y > y) == north)
                .count()
        };
        assert!(
            houses(true) > 30 && houses(false) > 30,
            "{} / {}",
            houses(true),
            houses(false)
        );
        let corridor = pico.rail_corridor().unwrap();
        for b in &pico.buildings {
            assert!(!b.footprint.intersects(&corridor), "{b:?} on the track");
        }
        // The station and its platform stand in the middle of town on the
        // north side of the track, the platform along it.
        let of = |kind| pico.buildings.iter().find(|b| b.kind == kind).unwrap();
        let (station, platform) = (of(BuildingKind::Station), of(BuildingKind::Platform));
        assert!(band.contains(&station.footprint) && band.contains(&platform.footprint));
        assert!(station.footprint.min.y > platform.footprint.max.y - 1e-3);
        assert!(platform.footprint.min.y > y);
        assert!((platform.footprint.center().x - grid.center().x).abs() < 1.0);
        // A store and a comisaría on each side of the line, each serving
        // its half of the town.
        for kind in [BuildingKind::GeneralStore, BuildingKind::PoliceStation] {
            let north: Vec<bool> = pico
                .buildings
                .iter()
                .filter(|b| b.kind == kind)
                .map(|b| b.footprint.center().y > y)
                .collect();
            assert_eq!(north.len(), 2, "{kind:?}");
            assert_ne!(north[0], north[1], "{kind:?}");
        }
        // The main plaza lies north-west of the station, a few blocks off.
        let to_plaza = pico.plaza.center() - station.footprint.center();
        assert!(to_plaza.x < -40.0 && to_plaza.y > 60.0, "{to_plaza:?}");
        assert!(pico.plaza.min.y > band.max.y);
        // Every north-south street that crosses the track has its level
        // crossing, on a street road, square to the track; the others stop
        // short of the station yard.
        let streets = pico.crossing_streets();
        assert!(streets.len() >= 3, "{streets:?}");
        let crossings: Vec<_> = loaded
            .roads
            .roads
            .iter()
            .filter(|r| r.kind == pampa_core::RoadKind::Street && r.from == 3)
            .flat_map(|r| r.crossings.iter())
            .collect();
        assert_eq!(crossings.len(), streets.len());
        for (c, (a, _)) in crossings.iter().zip(&streets) {
            assert!((c.at.x - a.x).abs() < 1e-3 && (c.at.y - y).abs() < 1e-3);
            assert!(c.angle() > pampa_core::RoadNetwork::MIN_CROSSING_ANGLE);
        }
        let yard: Vec<_> = pico
            .buildings
            .iter()
            .filter(|b| band.contains(&b.footprint))
            .collect();
        for s in &pico.streets {
            for b in &yard {
                assert!(!s.intersects(&b.footprint), "{s:?} into {b:?}");
            }
        }
        // Walkers cross the line at the level crossings.
        let graph = pampa_core::StreetGraph::from_town(pico);
        let across = graph.links.iter().enumerate().any(|(n, links)| {
            links
                .iter()
                .any(|&m| (graph.nodes[n].y - y) * (graph.nodes[m].y - y) < 0.0)
        });
        assert!(across);
        // The ground rises onto each crossing's deck and the rider rides
        // across, from one half of town to the other.
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        world.lay_roads(loaded.roads.clone());
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        for (a, b) in &streets {
            let deck = world.ground_height(Vec2::new(a.x, y)) - world.terrain.height(a.x, y);
            assert!(
                (deck - pampa_core::RoadNetwork::CROSSING_RISE).abs() < 0.05,
                "{deck}"
            );
            for k in 0..=20 {
                let p = *a + (*b - *a) * (k as f32 / 20.0);
                assert_eq!(world.resolve(collider, p, |_| ()), p, "{p:?}");
            }
        }
    }

    #[test]
    fn the_train_stops_at_metileo_by_its_platform() {
        use pampa_core::{LineTrain, Run, Timetable};
        let loaded = load_world(&asset_path(TOWNS_FILE));
        let metileo = &loaded.towns[2];
        assert_eq!(metileo.name, "Metileo");
        // A halt: a few houses round its store, no comisaría.
        let count = |kind| metileo.buildings.iter().filter(|b| b.kind == kind).count();
        assert_eq!(count(BuildingKind::PoliceStation), 0);
        assert_eq!(count(BuildingKind::GeneralStore), 1);
        assert!((3..=15).contains(&(count(BuildingKind::House) + count(BuildingKind::Shed))));
        let train = LineTrain::new(&loaded.line, &loaded.towns, Timetable::default()).unwrap();
        let station = train.station_of_town(2).expect("Metileo is a station");
        // Both runs stop there: up from Pico and down to it.
        for run in [Run::Up, Run::Down] {
            let (arrive, depart) = train.stop_times(run, station).unwrap();
            assert!(depart > arrive, "{run:?}");
            let mid = train.state_at(run, (arrive + depart) / 2.0);
            assert!(pampa_core::stands_at(&mid, station), "{run:?}");
        }
        assert!(train.misplaced_stops().is_empty());
        // Its platform is reachable on foot from the street.
        let (spot, _) = metileo.platform_spot().expect("a platform");
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        assert_eq!(world.resolve(collider, spot, |_| ()), spot);
        assert!(world.ground_height(spot) > world.terrain.height(spot.x, spot.y) + 0.5);
    }

    /// The narrowest front that takes a three-bay town house: three 2.8 m
    /// bays between walls set 0.1 m in from the lot (`pampa_render`'s
    /// facade kit).
    const THREE_BAYS: f32 = 3.0 * 2.8 + 2.0 * 0.1;

    /// Width of `b` along its front, in meters.
    fn frontage(b: &pampa_core::Building) -> f32 {
        let s = b.footprint.size();
        match b.front {
            pampa_core::Side::North | pampa_core::Side::South => s.x,
            pampa_core::Side::East | pampa_core::Side::West => s.y,
        }
    }

    #[test]
    fn metileo_has_lots_wide_enough_for_three_bay_houses_and_a_windowed_church() {
        // Metileo's blocks are 40 m, as in the other towns. Its 30 m blocks
        // (until 2026-10-25) left 6–7.5 m house lots, two bays each, and a
        // 9 m church front with no room for a window beside its tower.
        let loaded = load_world(&asset_path(TOWNS_FILE));
        let metileo = &loaded.towns[2];
        assert_eq!(metileo.name, "Metileo");
        let of = |kind| metileo.buildings.iter().filter(move |b| b.kind == kind);
        let houses: Vec<f32> = of(BuildingKind::House).map(frontage).collect();
        // No house narrower than the narrowest in the other towns (8 m),
        // and most wide enough for three 2.8 m bays between their walls.
        assert!(houses.iter().all(|&w| w >= 8.0), "{houses:?}");
        let three = houses.iter().filter(|&&w| w >= THREE_BAYS).count();
        assert!(2 * three >= houses.len(), "{houses:?}");
        // The church front fits its 4 m tower and an arched window, 2.4 m
        // of wall or more, on either side: 12 m, as in every other town.
        let church = of(BuildingKind::Church).map(frontage).collect::<Vec<_>>();
        assert_eq!(church.len(), 1);
        assert!(church[0] >= 4.0 + 2.0 * 2.5 + 2.0, "{church:?}");
    }

    #[test]
    fn metileo_on_40_m_blocks_keeps_clear_of_its_neighbours_roads_and_settlements() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        const METILEO: usize = 2;
        let metileo = &loaded.towns[METILEO];
        // Still centered on its origin, now 152 m across: three 40 m blocks
        // and four 8 m streets.
        let grid = metileo.grid();
        assert_eq!(grid.center(), Vec2::new(288.0, -544.0));
        assert_eq!(grid.size(), Vec2::new(152.0, 152.0));
        // Nothing built on another: buildings, streets and the plaza.
        let built = &metileo.buildings;
        for (i, a) in built.iter().enumerate() {
            for b in &built[i + 1..] {
                assert!(!a.footprint.intersects(&b.footprint), "{a:?} on {b:?}");
            }
            assert!(
                !a.footprint.intersects(&metileo.plaza),
                "{a:?} in the plaza"
            );
            for s in &metileo.streets {
                assert!(!a.footprint.intersects(s), "{a:?} in {s:?}");
            }
        }
        // Clear of the other towns, the lagoon, other towns' roads and every
        // settlement.
        let band = metileo.leveled();
        for (i, other) in loaded.towns.iter().enumerate() {
            if i != METILEO {
                assert!(
                    !other.leveled().intersects(&band.expanded(500.0)),
                    "{}",
                    other.name
                );
            }
        }
        for lagoon in &loaded.lagoons {
            assert!(!lagoon.keep_out().intersects(&band.expanded(500.0)));
        }
        let reach = band.expanded(100.0);
        for road in &loaded.roads.roads {
            if road.from != METILEO && road.to != METILEO {
                let on = road.path.iter().any(|p| reach.contains_point(*p));
                assert!(!on, "{:?} {}->{}", road.kind, road.from, road.to);
            }
        }
        let laid = crate::lay_out(&loaded);
        for settlement in &laid.settlements.settlements {
            assert!(
                !settlement.bounds.intersects(&band),
                "{:?}",
                settlement.kind
            );
        }
    }

    #[test]
    fn the_shipped_world_plants_trees_between_towns_and_none_on_them() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let flora = pampa_core::FloraSite::new(world.config, &world.terrain, &loaded.line);
        let before = world.obstacles.len();
        let started = std::time::Instant::now();
        let planted = world.plant(&flora);
        let took = started.elapsed();
        println!("planted {planted} trees in {took:?}");
        assert_eq!(world.obstacles.len(), before + planted);
        assert!(planted > 5_000, "{planted}");
        for tree in world.obstacles[before..].iter() {
            let c = tree.center();
            for area in &world.terrain.flat_areas {
                assert_eq!(area.weight(c), 0.0, "{c:?}");
            }
        }
        // Every town keeps its start spot free.
        let (spawn, _) = loaded.towns[0].arrival();
        assert!(world.obstacles.iter().all(|o| !o.contains_point(spawn)));
    }

    #[test]
    fn the_shipped_world_has_its_roads_and_one_level_crossing_by_each_station() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let roads = &loaded.roads;
        assert!(roads.length_between(0, 1).is_some());
        assert!(roads.length_between(0, 2).is_some());
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        world.lay_roads(roads.clone());
        let fences = pampa_core::FenceLayout::build(
            world.config,
            &world.terrain,
            &loaded.towns,
            &loaded.line,
        );
        world.fence(&fences);
        assert!(!fences.gates.is_empty() && !fences.openings.is_empty());
        let flora = pampa_core::FloraSite::new(world.config, &world.terrain, &loaded.line)
            .with_fences(&fences);
        world.plant(&flora);
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        for (i, town) in loaded.towns.iter().enumerate() {
            // General Pico, on both sides of its line, crosses it by its
            // streets instead (see the test above).
            if town.rail_band().is_some() {
                continue;
            }
            // Its rural track crosses its own line by the station (the
            // road out of Castex to Caleufú crosses the line's run-out too;
            // see the Caleufú tests).
            let crossings: Vec<_> = roads
                .roads
                .iter()
                .filter(|r| r.from == i && r.kind == pampa_core::RoadKind::Rural)
                .flat_map(|r| r.crossings.iter())
                .collect();
            assert_eq!(crossings.len(), 1, "{}", town.name);
            let station = town
                .buildings
                .iter()
                .find(|b| b.kind == BuildingKind::Station)
                .unwrap();
            assert!(crossings[0].at.distance(station.footprint.center()) < 160.0);
            // Its rural track runs from the street over the crossing.
            assert!(
                roads
                    .roads
                    .iter()
                    .any(|r| r.from == i && r.kind == pampa_core::RoadKind::Rural)
            );
        }
        // Every road, fenced on both sides, is open from end to end.
        for road in &roads.roads {
            for w in road.path.windows(2) {
                for k in 0..4 {
                    let p = w[0] + (w[1] - w[0]) * (k as f32 / 4.0);
                    assert_eq!(world.resolve(collider, p, |_| ()), p, "{p:?}");
                }
            }
        }
    }

    #[test]
    fn a_broken_railway_line_warns_naming_the_towns_and_keeps_them() {
        let path = temp_towns(
            "line",
            r#"(towns: [
                (name: "Oeste", origin: (-2000.0, 0.0), blocks: (4, 4), block_size: 40.0,
                 street_width: 10.0, railway: Some((side: North, offset: 24.0))),
                (name: "Medio", blocks: (4, 4), block_size: 40.0, street_width: 10.0),
                (name: "Este", origin: (2000.0, 0.0), blocks: (4, 4), block_size: 40.0,
                 street_width: 10.0, railway: Some((side: North, offset: 24.0))),
            ], line: Some(["Oeste", "Este", "Este", "Rancul"]))"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns.len(), 3);
        assert_eq!(loaded.line.towns, [0, 2]);
        assert!(loaded.line.links.is_empty());
        let [duplicate, unknown, crossing, road] = &loaded.warnings[..] else {
            panic!("{:?}", loaded.warnings)
        };
        assert!(duplicate.contains("`Este`"), "{duplicate}");
        assert!(unknown.contains("`Rancul`"), "{unknown}");
        assert!(crossing.contains("`Medio`"), "{crossing}");
        // The road between them would cross Medio too, and is left out.
        assert!(road.contains("road") && road.contains("`Medio`"), "{road}");
        let report = TownReport::new(&loaded);
        assert_eq!(report.warnings, loaded.warnings);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_world_follows_the_loaded_town() {
        let path = temp_towns(
            "small",
            r#"(towns: [(name: "General Alvear", blocks: (3, 4), block_size: 34.0,
                street_width: 9.0, railway: Some((side: South, offset: 26.0)), seed: 3)])"#,
        );
        let loaded = load_world(&path);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let town = loaded.towns[0].clone();
        assert_eq!(town.name, "General Alvear");
        assert_eq!(town.streets.len(), 4 + 5);
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        assert_eq!(world.obstacles, town.obstacles());
        let (spawn, _) = town.arrival();
        assert!(world.obstacles.iter().all(|o| !o.contains_point(spawn)));
        // The station stands beside the railway, within its span, for the
        // line train to stop at.
        let rail = town.railway.expect("a railway");
        let station = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .expect("a station")
            .footprint
            .center();
        let along = rail.end - rail.start;
        let t = (station - rail.start).dot(along) / along.dot(along);
        assert!(
            (0.0..=1.0).contains(&t),
            "station off the track's span: {t}"
        );
        let crowd = Townsfolk::new(&town, 4, 1);
        assert_eq!(crowd.graph.nodes.len(), 4 * 5);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn every_town_is_built_at_its_origin_and_clashes_are_reported() {
        let path = temp_towns(
            "many",
            r#"(world: (half_extent: 5000.0), towns: [
                (name: "Eduardo Castex", blocks: (4, 5), block_size: 40.0, street_width: 10.0,
                 railway: Some((side: North, offset: 24.0)), seed: 7),
                (name: "Florencio Varela", blocks: (3, 3), block_size: 40.0, street_width: 10.0),
                (name: "Lejos", origin: (3000.0, -2000.0), blocks: (3, 4), block_size: 36.0,
                 street_width: 9.0, railway: Some((side: South, offset: 26.0)), seed: 11),
            ])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.world.half_extent, 5000.0);
        let names: Vec<&str> = loaded.towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Eduardo Castex", "Lejos"]);
        assert_eq!(loaded.towns[1].grid().center(), Vec2::new(3000.0, -2000.0));
        let [warning] = &loaded.warnings[..] else {
            panic!("{:?}", loaded.warnings)
        };
        assert!(
            warning.contains("Florencio Varela") && warning.contains("Eduardo Castex"),
            "{warning}"
        );
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        assert_eq!(world.terrain.flat_areas.len(), 2);
        let report = TownReport::new(&loaded);
        assert_eq!(report.names, ["Eduardo Castex", "Lejos"]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_bad_file_falls_back_to_castex_with_warnings() {
        let path = temp_towns(
            "bad",
            r#"(towns: [(name: "Ciudad #1", blocks: (3, 3), block_size: 40.0, street_width: 2.0)])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns, vec![TownLayout::castex_inspired()]);
        assert!(
            loaded.warnings.iter().any(|w| w.contains("street_width")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();

        let path = temp_towns(
            "glyphs",
            r#"(towns: [(name: "Ciudad #1", blocks: (3, 3), block_size: 40.0, street_width: 10.0,
                railway: Some((side: North, offset: 24.0)))])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns[0].name, "Ciudad #1");
        assert!(
            loaded.warnings.iter().any(|w| w.contains("'#'")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_world_too_small_for_any_town_falls_back_to_castex_in_the_default_world() {
        let path = temp_towns(
            "tiny",
            r#"(world: (half_extent: 100.0, chunk_size: 50.0), towns: [
                (name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0)])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns, vec![TownLayout::castex_inspired()]);
        assert_eq!(loaded.world, WorldConfig::default());
        assert!(
            loaded.warnings.iter().any(|w| w.contains("default world")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// Caleufú, the shipped world's fifth town, and its branch.
    mod caleufu {
        use super::*;
        use pampa_core::{RoadKind, RoadNetwork};

        const CASTEX: usize = 1;
        const METILEO: usize = 2;
        const CALEUFU: usize = 4;

        fn loaded() -> LoadedWorld {
            let loaded = load_world(&asset_path(TOWNS_FILE));
            assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
            assert_eq!(loaded.towns[CALEUFU].name, "Caleufú");
            loaded
        }

        /// Where segments `p`–`q` and `a`–`b` cross, if they do.
        fn crossing(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> Option<Vec2> {
            let (r, s) = (q - p, b - a);
            let denom = r.x * s.y - r.y * s.x;
            if denom.abs() < 1e-9 {
                return None;
            }
            let d = a - p;
            let t = (d.x * s.y - d.y * s.x) / denom;
            let u = (d.x * r.y - d.y * r.x) / denom;
            ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| p + r * t)
        }

        #[test]
        fn caleufu_is_a_small_railway_town_with_its_pulperia_and_comisaria() {
            let loaded = loaded();
            let town = &loaded.towns[CALEUFU];
            assert_eq!(town.size, TownSize::Small);
            let places = town.places();
            let named = |kind: PlaceKind| -> Vec<&str> {
                places
                    .iter()
                    .filter(|p| p.kind == kind)
                    .map(|p| p.name.as_str())
                    .collect()
            };
            assert_eq!(named(PlaceKind::GeneralStore), ["PULPERÍA"]);
            assert_eq!(named(PlaceKind::PoliceStation).len(), 1);
            assert_eq!(named(PlaceKind::Plaza).len(), 1);
            assert_eq!(named(PlaceKind::Station), ["ESTACIÓN CALEUFÚ"]);
            let houses = town
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .count();
            assert!(houses >= 8, "{houses}");
            // The pulpería and the comisaría stand blocks apart.
            let at = |kind: PlaceKind| places.iter().find(|p| p.kind == kind).unwrap().center();
            let apart = at(PlaceKind::GeneralStore) - at(PlaceKind::PoliceStation);
            let pitch = 40.0 + 10.0;
            assert!(apart.x.abs().max(apart.y.abs()) >= 1.9 * pitch, "{apart:?}");
        }

        #[test]
        fn caleufu_lies_inside_the_world_clear_of_towns_lagoons_roads_and_settlements() {
            let loaded = loaded();
            let town = &loaded.towns[CALEUFU];
            let band = town.leveled();
            let bounds = loaded.world.bounds();
            // Inside the west edge, its track's run-outs too.
            let inner = band.expanded(100.0);
            assert!(inner.min.x > bounds.min.x && inner.max.x < bounds.max.x);
            assert!(inner.min.y > bounds.min.y && inner.max.y < bounds.max.y);
            let rail = town.railway.expect("a railway");
            assert!(rail.start.x - TownLayout::RAIL_CORRIDOR_HALF > bounds.min.x + 200.0);
            for (i, other) in loaded.towns.iter().enumerate() {
                if i != CALEUFU {
                    let near = other.leveled().intersects(&band.expanded(500.0));
                    assert!(!near, "{}", other.name);
                }
            }
            for lagoon in &loaded.lagoons {
                assert!(!lagoon.keep_out().intersects(&band.expanded(500.0)));
            }
            // Only its own roads reach its ground.
            let reach = band.expanded(100.0);
            for road in &loaded.roads.roads {
                if road.from != CALEUFU && road.to != CALEUFU {
                    let on = road.path.iter().any(|p| reach.contains_point(*p));
                    assert!(!on, "{:?} {}->{}", road.kind, road.from, road.to);
                }
            }
            let laid = crate::lay_out(&loaded);
            for settlement in &laid.settlements.settlements {
                assert!(
                    !settlement.bounds.intersects(&band),
                    "{:?}",
                    settlement.kind
                );
            }
        }

        #[test]
        fn the_branch_joins_the_line_at_metileo_and_runs_unbroken_to_caleufus_station() {
            let loaded = loaded();
            let line = &loaded.line;
            let metileo = loaded.towns[METILEO].railway.unwrap();
            let caleufu = loaded.towns[CALEUFU].railway.unwrap();
            let branch = line
                .links
                .iter()
                .find(|l| (l.from, l.to) == (METILEO, CALEUFU))
                .expect("the branch link");
            // The junction: Metileo's west tip, where the line from Monte
            // Nievas arrives.
            let from_nievas = line.links.iter().find(|l| l.to == METILEO).unwrap();
            assert_eq!(branch.track.start, metileo.start);
            assert_eq!(from_nievas.track.end, metileo.start);
            assert_eq!(branch.track.end, caleufu.end);
            // One unbroken path, bending gently, through Metileo's station
            // and out to Caleufú's far tip.
            let paths = line.branch_paths(&loaded.towns);
            let points = paths[0].points();
            assert_eq!(points.first(), Some(&metileo.end));
            assert_eq!(points.last(), Some(&caleufu.start));
            let expected = metileo.start.distance(metileo.end)
                + branch.length()
                + caleufu.start.distance(caleufu.end);
            assert!((paths[0].length() - expected).abs() < 0.5);
            let sharpest = points
                .windows(3)
                .map(|w| {
                    let a = (w[1] - w[0]) * (1.0 / w[0].distance(w[1]));
                    let b = (w[2] - w[1]) * (1.0 / w[1].distance(w[2]));
                    (a.x * b.y - a.y * b.x).atan2(a.dot(b)).abs()
                })
                .fold(0.0, f32::max);
            let max = pampa_core::RailLine::MAX_TURN + 1e-4;
            assert!(sharpest <= max, "{}", sharpest.to_degrees());
            let radius = pampa_core::RailLine::CURVE_RADIUS - 0.1;
            assert!(branch.curves.iter().all(|c| c.radius >= radius));
            // Past the junction the branch parts from the line for good.
            let main = line.full_path(&loaded.towns);
            for p in &branch.path {
                if p.distance(metileo.start) > 400.0 {
                    let gap = main.at(main.project(*p)).distance(*p);
                    assert!(gap > 50.0, "{p:?} {gap}");
                }
            }
            // Caleufú's platform stands by the branch.
            let (spot, _) = loaded.towns[CALEUFU].platform_spot().expect("a platform");
            let on = paths[0].at(paths[0].project(spot));
            assert!(on.distance(spot) < 12.0, "{}", on.distance(spot));
            // The line train still runs the line alone.
            let train =
                pampa_core::LineTrain::new(line, &loaded.towns, pampa_core::Timetable::default())
                    .unwrap();
            assert_eq!(train.station_of_town(CALEUFU), None);
            assert_eq!(train.stations().len(), 4);
            assert!(train.misplaced_stops().is_empty());
        }

        #[test]
        fn a_road_joins_caleufu_to_castex_and_every_road_on_the_branch_crosses_it_level() {
            let loaded = loaded();
            let roads = &loaded.roads;
            // Its nearest town is Eduardo Castex, about 5.9 km away.
            let road = roads
                .roads
                .iter()
                .find(|r| r.kind == RoadKind::Between && (r.from, r.to) == (CASTEX, CALEUFU))
                .expect("Castex to Caleufú");
            let length = roads.length_between(CASTEX, CALEUFU).unwrap();
            assert!((5_000.0..7_000.0).contains(&length), "{length}");
            // It crosses Castex's run-out squarely, west of the town.
            assert_eq!(road.crossings.len(), 1);
            let crossing_at = road.crossings[0];
            assert!(crossing_at.angle() > RoadNetwork::MIN_CROSSING_ANGLE);
            assert!(crossing_at.at.x < loaded.towns[CASTEX].grid().min.x);
            // Signposts at either end name the other town.
            let boards: Vec<&str> = roads
                .signposts
                .iter()
                .flat_map(|s| &s.boards)
                .map(|b| b.text.as_str())
                .collect();
            assert!(
                boards.iter().any(|b| b.starts_with("CALEUFÚ ")),
                "{boards:?}"
            );
            let castex = boards.iter().filter(|b| b.starts_with("EDUARDO CASTEX "));
            assert!(castex.count() >= 2, "{boards:?}");
            // Caleufú's rural track crosses the branch by the station.
            let rural = roads
                .roads
                .iter()
                .find(|r| r.kind == RoadKind::Rural && r.from == CALEUFU)
                .expect("Caleufú's rural track");
            assert_eq!(rural.crossings.len(), 1);
            // Wherever a road meets the branch, it has a level crossing.
            let branch = &loaded.line.branch_paths(&loaded.towns)[0];
            let mut met = 0;
            for road in &roads.roads {
                for w in road.path.windows(2) {
                    for r in branch.points().windows(2) {
                        let Some(at) = crossing(w[0], w[1], r[0], r[1]) else {
                            continue;
                        };
                        met += 1;
                        let level = road.crossings.iter().any(|c| c.at.distance(at) < 1e-2);
                        assert!(level, "{:?} meets the branch at {at:?}", road.kind);
                    }
                }
            }
            assert!(met >= 1);
        }

        #[test]
        fn caleufu_and_its_branch_are_laid_the_same_every_time() {
            let (a, b) = (loaded(), loaded());
            assert_eq!(a.towns[CALEUFU], b.towns[CALEUFU]);
            assert_eq!(a.line, b.line);
            assert_eq!(a.roads, b.roads);
        }
    }
}
