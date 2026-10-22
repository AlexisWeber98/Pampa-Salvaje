//! Builds the world and its towns from `assets/world/towns.ron`.
//!
//! The towns are needed before the app starts (the leveled terrain, the
//! crowds and the trains all come from them), when logging is not up yet,
//! so problems are kept in [`TownReport`] and logged at startup.

use std::path::Path;

use bevy::prelude::*;
use pampa_content::load_towns_or_builtin;
use pampa_core::{PlaceKind, RailLine, RoadNetwork, TownLayout, WorldConfig, place_towns};
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
    /// Problems found, ready to log as warnings.
    pub warnings: Vec<String>,
}

/// Loads the towns file at `path` and places every valid town in its world,
/// falling back to the built-in Eduardo Castex when no town can be built,
/// then runs the railway line and the roads through them.
pub fn load_world(path: &Path) -> LoadedWorld {
    let (content, errors) = load_towns_or_builtin(path);
    let mut warnings: Vec<String> = errors.iter().map(|e| format!("towns: {e}")).collect();
    let mut world = content.world;
    let mut names = content.line;
    // Content already placed these towns once; this only builds them.
    let (mut towns, rejected) = place_towns(&world, &content.towns);
    warnings.extend(rejected.iter().map(|e| format!("towns: {e}")));
    if towns.is_empty() {
        // The file's line names towns that are gone.
        names = None;
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
    let (line, broken) = RailLine::build(names.as_deref(), &towns, world.bounds());
    warnings.extend(broken.iter().map(|e| format!("towns: {e}")));
    let (roads, left_out) = RoadNetwork::build(&towns, &line, world.bounds());
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    LoadedWorld {
        world,
        towns,
        line,
        roads,
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
        assert_eq!(names, ["Monte Nievas", "Eduardo Castex", "General Pico"]);
        // General Pico is very big: a 2 x 2 plaza, a station plaza, four
        // general stores, two police stations and 51 people.
        let pico = &loaded.towns[2];
        assert_eq!(pico.size, TownSize::VeryBig);
        assert!(pico.station_plaza.is_some());
        let count = |kind: PlaceKind| pico.places().iter().filter(|p| p.kind == kind).count();
        assert_eq!(count(PlaceKind::GeneralStore), 4);
        assert_eq!(count(PlaceKind::PoliceStation), 2);
        assert_eq!(count(PlaceKind::Plaza), 2);
        assert_eq!(pico.townsfolk_count(), 51);
        for small in &loaded.towns[..2] {
            assert_eq!(small.size, TownSize::Small, "{}", small.name);
            assert_eq!(small.station_plaza, None, "{}", small.name);
        }
        // One railway line runs Castex, Monte Nievas, General Pico.
        assert_eq!(loaded.line.towns, [1, 0, 2]);
        let links: Vec<(usize, usize)> = loaded.line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(links, [(1, 0), (0, 2)]);
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
        let flora = pampa_core::FloraSite::new(world.config, &world.terrain, &loaded.line);
        world.plant(&flora);
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        for (i, town) in loaded.towns.iter().enumerate() {
            let crossings: Vec<_> = roads
                .roads
                .iter()
                .filter(|r| r.from == i)
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
        // Every road is open from end to end.
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
}
