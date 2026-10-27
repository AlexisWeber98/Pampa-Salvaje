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
    let (mut line, broken) =
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
    // The walks from the bays' platforms to the road crossing by the station.
    line.lay_walks(&towns, &roads);
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
    use pampa_core::town::Walk;
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
    fn general_picos_sidewalks_keep_off_the_railway_band_and_the_platform() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        let pico = &loaded.towns[3];
        assert_eq!(
            pampa_core::town::Paving::of(pico.blocks()),
            pampa_core::town::Paving::Everywhere
        );
        let band = pico.rail_band().expect("the line runs through Pico");
        let corridor = pico.rail_corridor().unwrap();
        let floors = pico.floors();
        assert!(!floors.is_empty(), "Pico's platform");
        let walks = pico.sidewalks();
        assert!(walks.len() > 200, "{} strips", walks.len());
        for walk in walks {
            let a = walk.area;
            assert!(!a.intersects(&band), "{a:?} on the railway band {band:?}");
            assert!(!a.intersects(&corridor), "{a:?} on the track");
            for floor in &floors {
                for area in floor.ramps().into_iter().chain([floor.footprint]) {
                    assert!(!a.intersects(&area), "{a:?} on the platform {area:?}");
                }
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
        // The church front is 12 m, as in every other town: beside its 4 m
        // tower that leaves 3.9 m of nave wall on either side (inside its
        // 0.1 m insets), past the 2.4 m an arched window needs.
        let church = of(BuildingKind::Church).map(frontage).collect::<Vec<_>>();
        assert_eq!(church.len(), 1);
        assert!(church[0] >= 12.0 - 1e-3, "{church:?}");
    }

    #[test]
    fn the_shipped_streets_are_two_meters_wider_and_picos_three_and_every_town_paves_by_its_tier() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        // (name, street width, carriageway left between the sidewalks), all
        // literal: the carriageway is not derived from the width here.
        let wanted: [(&str, f32, f32); 5] = [
            ("Monte Nievas", 12.0, 5.0),
            ("Eduardo Castex", 12.0, 5.0),
            ("Metileo", 12.0, 5.0),
            ("General Pico", 13.0, 6.0),
            ("Caleufú", 12.0, 5.0),
        ];
        assert_eq!(loaded.towns.len(), wanted.len());
        for (town, &(name, width, carriageway)) in loaded.towns.iter().zip(&wanted) {
            assert_eq!(town.name, name);
            // The ring street round the grid is as wide as the streets.
            let first = town
                .streets
                .iter()
                .map(|s| s.size().x.min(s.size().y))
                .fold(f32::INFINITY, f32::min);
            assert!((first - width).abs() < 1e-3, "{name}: streets {first} m");
            // Every town has sidewalks (3.5 m a side), village included, and
            // they leave the carriageway 5 m, Pico 6 m.
            assert!(!town.sidewalks().is_empty(), "{name}");
            assert_eq!(pampa_core::town::Sidewalk::WIDTH, 3.5);
            assert_sidewalks_line_every_cross_section(town, carriageway);
            // Brick: all of Pico's, the plaza's ring of a town (4 strips), none
            // of a village's; every strip has its curbs.
            let brick = town
                .sidewalks()
                .iter()
                .filter(|w| w.walk == Walk::Brick)
                .count();
            let expected = match name {
                "General Pico" => town.sidewalks().len(),
                "Metileo" => 0,
                _ => 4,
            };
            assert_eq!(brick, expected, "{name}");
            assert!(
                town.sidewalks().iter().all(|w| !w.curbs.is_empty()),
                "{name}"
            );
        }
    }

    /// Checks every cross-section along the middle half of each of the town's
    /// streets (at its ends a crossing street's sidewalk may run right across
    /// it, and an intersection has no frontage), one street edge at a time. A
    /// sidewalk belongs on an edge exactly where the land beyond it is a block
    /// or plaza: inside the grid, not on another street and not in the rail
    /// band (where the track and the station yard lie; the ring street's outer
    /// edge faces the pampa). Where one belongs it must be `Sidewalk::WIDTH`
    /// deep from the edge; where none does, nothing may cover that edge;
    /// nothing covers the middle of the street; and where both belong
    /// `carriageway` is what is left.
    fn assert_sidewalks_line_every_cross_section(
        town: &pampa_core::town::TownLayout,
        carriageway: f32,
    ) {
        const SAMPLES: usize = 16;
        const EPS: f32 = 1e-3;
        let width = pampa_core::town::Sidewalk::WIDTH;
        let name = &town.name;
        let axis = |v: Vec2, x: bool| if x { v.x } else { v.y };
        let band = town.rail_band();
        let block_land = |p: Vec2| {
            town.grid().contains_point(p)
                && !band.is_some_and(|b| b.contains_point(p))
                && !town.streets.iter().any(|s| s.contains_point(p))
        };
        for street in &town.streets {
            let size = street.size();
            // Whether the street runs along x; across it is the other axis.
            let x = size.x >= size.y;
            let (lo, hi) = (axis(street.min, !x), axis(street.max, !x));
            for k in 0..SAMPLES {
                let t = 0.25 + 0.5 * (k as f32 + 0.5) / SAMPLES as f32;
                let at = axis(street.min, x) + t * axis(size, x);
                let point = |across: f32| {
                    if x {
                        Vec2::new(at, across)
                    } else {
                        Vec2::new(across, at)
                    }
                };
                // An intersection has no frontage: the crossing street's
                // sidewalks run round its corners.
                let middle = point((lo + hi) / 2.0);
                if town
                    .streets
                    .iter()
                    .any(|o| o != street && o.contains_point(middle))
                {
                    continue;
                }
                let mut covered: Vec<(f32, f32)> = town
                    .sidewalks()
                    .iter()
                    .map(|w| w.area)
                    .filter(|a| axis(a.min, x) < at && at < axis(a.max, x))
                    .map(|a| (axis(a.min, !x).max(lo), axis(a.max, !x).min(hi)))
                    .filter(|(a, z)| a < z)
                    .collect();
                covered.sort_by(|a, b| a.0.total_cmp(&b.0));
                // The runs the sidewalks leave touching each edge, and what
                // they cover in all.
                let (mut low, mut high, mut total) = (0.0f32, 0.0f32, 0.0f32);
                let mut edge = lo;
                for &(a, z) in &covered {
                    if a <= lo + EPS {
                        low = low.max(z - lo);
                    }
                    if z >= hi - EPS {
                        high = high.max(hi - a);
                    }
                    total += (z - a.max(edge)).max(0.0);
                    edge = edge.max(z);
                }
                let (low_belongs, high_belongs) =
                    (block_land(point(lo - 0.25)), block_land(point(hi + 0.25)));
                let here = format!("{name}: street {street:?} across {at}");
                let depth = |belongs: bool| if belongs { width } else { 0.0 };
                assert!(
                    (low - depth(low_belongs)).abs() < EPS,
                    "{here}: low {low} m"
                );
                assert!(
                    (high - depth(high_belongs)).abs() < EPS,
                    "{here}: high {high} m"
                );
                assert!(
                    (total - low - high).abs() < EPS,
                    "{here}: a sidewalk off the edges, {total} m covered"
                );
                if low_belongs && high_belongs {
                    let left = hi - lo - total;
                    assert!((left - carriageway).abs() < EPS, "{here}: {left} m");
                }
            }
        }
    }

    #[test]
    fn metileo_on_40_m_blocks_keeps_clear_of_its_neighbours_roads_and_settlements() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        const METILEO: usize = 2;
        let metileo = &loaded.towns[METILEO];
        // Still centered on its origin, now 168 m across: three 40 m blocks
        // and four 12 m streets.
        let grid = metileo.grid();
        assert_eq!(grid.center(), Vec2::new(288.0, -544.0));
        assert_eq!(grid.size(), Vec2::new(168.0, 168.0));
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
            // Its rural track crosses its own line by the station, once: the
            // bay beside it (Metileo's) rejoins the line east of the road
            // (the road out of Castex to Caleufú crosses the line's run-out
            // too; see the Caleufú tests).
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
            for crossing in &crossings {
                assert!(crossing.at.distance(station.footprint.center()) < 160.0);
            }
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
                street_width: 11.0, railway: Some((side: South, offset: 26.0)), seed: 3)])"#,
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
                 street_width: 11.0, railway: Some((side: South, offset: 26.0)), seed: 11),
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
        use pampa_core::{
            Aabb, BASE_RATE, CarKind, Floor, LineTrain, MAX_STOP_MISS, RoadKind, RoadNetwork, Run,
            Terrain, Timetable,
        };

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

        /// The line train and the shuttle of the shipped world.
        fn trains(loaded: &LoadedWorld) -> (LineTrain, LineTrain) {
            let line = LineTrain::new(&loaded.line, &loaded.towns, Timetable::default())
                .expect("a line train");
            let path = loaded.line.shuttle_paths(&loaded.towns).remove(0);
            let bay = loaded.line.bay_of(0).expect("a bay");
            let stops = [(METILEO, bay.stop)];
            let shuttle =
                LineTrain::branch_shuttle(&line, path, &[METILEO, CALEUFU], &stops, &loaded.towns)
                    .expect("a shuttle");
            (line, shuttle)
        }

        fn jumped(train: &LineTrain, at: f64) -> LineTrain {
            let mut train = train.clone();
            train.advance(0.0, at, BASE_RATE);
            train
        }

        #[test]
        fn metileo_has_a_bay_on_the_far_side_of_its_track_clear_of_the_town() {
            let loaded = loaded();
            let metileo = &loaded.towns[METILEO];
            let track = metileo.railway.unwrap();
            let bay = loaded.line.bay_of(0).expect("Metileo's bay");
            assert_eq!((bay.town, bay.branch), (METILEO, 0));
            // North of the station track (away from the town), 4.5 m out,
            // its dead end at the platform's east end, running west.
            assert!(bay.buffer().y > track.start.y);
            assert!((bay.buffer().y - track.start.y - 4.5).abs() < 1e-3);
            assert!(bay.buffer().x < track.end.x && bay.buffer().x > track.start.x);
            assert!(bay.heading().x < -0.99);
            // Nothing built, planted or fenced stands on it, nor a lagoon,
            // a settlement or a road; the 40 m road clearance keeps the
            // roads off it too.
            let laid = crate::lay_out(&loaded);
            let car = 3.0 / 2.0 + 0.2;
            let around = |a: Vec2, b: Vec2| {
                let (lo, hi) = (
                    Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                    Vec2::new(a.x.max(b.x), a.y.max(b.y)),
                );
                Aabb::new(lo, hi).expanded(car)
            };
            for w in bay.link.path.windows(2) {
                let reach = around(w[0], w[1]);
                // Every obstacle keeps the cars' half width off the track.
                for o in laid.world.obstacles.near(reach) {
                    for k in 0..=8 {
                        let p = w[0] + (w[1] - w[0]) * (k as f32 / 8.0);
                        let off = o.closest_point(p).distance(p);
                        assert!(off >= 1.5, "{o:?} stands {off} m from the bay at {p:?}");
                    }
                }
                for lagoon in &loaded.lagoons {
                    assert!(lagoon.keep_out().segment_overlap(w[0], w[1]).is_none());
                }
                for settlement in &laid.settlements.settlements {
                    assert!(!settlement.bounds.intersects(&reach));
                }
            }
            // No road meets the bay: Metileo's rural track crosses the
            // station track alone, west of where the bay joins it.
            let mut met = 0;
            for road in &loaded.roads.roads {
                for r in road.path.windows(2) {
                    for w in bay.link.path.windows(2) {
                        let Some(at) = crossing(r[0], r[1], w[0], w[1]) else {
                            continue;
                        };
                        met += 1;
                        assert_eq!((road.kind, road.from), (RoadKind::Rural, METILEO));
                        let level = road.crossings.iter().any(|c| c.at.distance(at) < 1e-2);
                        assert!(level, "{at:?}");
                    }
                }
            }
            assert_eq!(met, 0);
            // The flora keeps off it as it keeps off any track: a spot 8 m
            // north of the bay is 12.5 m from the station track, so only
            // the bay itself rules it out.
            let beside = bay.stop + Vec2::new(0.0, 8.0);
            assert!(!laid.flora.off_track(beside, 10.0));
            assert!(laid.flora.off_track(beside, 7.0));
            // Nor does anything the town itself planted or tied up.
            for w in bay.link.path.windows(2) {
                let keep = around(w[0], w[1]);
                let trees = metileo.trees.iter().copied();
                let shade = metileo.shade_trees.iter().map(|t| t.at);
                let plants = metileo.yard_plants.iter().map(|p| p.at);
                for p in trees.chain(shade).chain(plants) {
                    assert!(!keep.contains_point(p), "{p:?}");
                }
                for palenque in &metileo.palenques {
                    assert!(!keep.intersects(&palenque.rail));
                }
            }
        }

        #[test]
        fn the_shuttle_stands_in_the_bay_and_never_overlaps_the_line_train_all_day() {
            let loaded = loaded();
            let (line, shuttle) = trains(&loaded);
            let bay = loaded.line.bay_of(0).unwrap();
            let track = loaded.towns[METILEO].railway.unwrap();
            let (platform, _) = loaded.towns[CALEUFU].platform_spot().expect("a platform");
            let (mut meetings, mut at_caleufu) = (0, 0);
            // Two game days, every 15 game seconds: both line runs, both
            // shuttle round trips, every layover and turnout.
            for k in 0..(2 * 86_400 / 15) {
                let t = f64::from(k) * 15.0;
                let (l, s) = (jumped(&line, t), jumped(&shuttle, t));
                let (lc, sc) = (l.obstacles(), s.obstacles());
                for a in &lc {
                    for b in &sc {
                        assert!(
                            !a.footprint.expanded(0.4).intersects(&b.footprint),
                            "t = {t}: {:?} {:?}",
                            a.footprint,
                            b.footprint
                        );
                    }
                }
                let state = s.state();
                if state.station == Some(0) {
                    // Standing at Metileo: in the bay, beside the station
                    // track, never on it.
                    for car in s.placed_cars() {
                        let off = car.center.y - track.start.y;
                        assert!((off - 4.5).abs() < 0.2, "{off}");
                        // Short of the buffer stop.
                        assert!(car.center.x + car.length / 2.0 < bay.buffer().x - 1.0);
                    }
                    if l.state().station == Some(2) {
                        meetings += 1;
                    }
                }
                if state.station == Some(1) {
                    // Standing at Caleufú: coaches by the platform.
                    let cars = s.placed_cars();
                    let coach = cars.iter().find(|c| c.kind == CarKind::Coach).unwrap();
                    assert!(coach.center.distance(platform) < MAX_STOP_MISS);
                    at_caleufu += 1;
                }
            }
            // The trains were there together at Metileo, and it was tested.
            assert!(meetings > 100, "{meetings}");
            assert!(at_caleufu > 100, "{at_caleufu}");
        }

        /// Footprints of the platform with its ramps.
        fn platform_works(bay: &pampa_core::RailBay) -> Vec<Aabb> {
            let f = bay.platform;
            f.ramps().into_iter().chain([f.footprint]).collect()
        }

        /// Metileo's rural road and the level crossing it makes with the
        /// station track.
        fn metileos_road(loaded: &LoadedWorld) -> (&pampa_core::Road, pampa_core::LevelCrossing) {
            let road = loaded
                .roads
                .roads
                .iter()
                .find(|r| r.kind == RoadKind::Rural && r.from == METILEO)
                .expect("Metileo's rural road");
            let track_y = loaded.towns[METILEO].railway.unwrap().start.y;
            let crossing = road
                .crossings
                .iter()
                .copied()
                .find(|c| (c.at.y - track_y).abs() < 1e-2)
                .expect("a crossing of the station track");
            (road, crossing)
        }

        #[test]
        fn the_bay_platform_and_walks_keep_clear_of_everything_built_planted_or_driven() {
            let loaded = loaded();
            let bay = loaded.line.bay_of(0).unwrap();
            let laid = crate::lay_out(&loaded);
            // The built world walks and boards on the platform.
            assert!(laid.world.floors.contains(&bay.platform));
            let (road, crossing) = metileos_road(&loaded);
            for area in platform_works(bay) {
                let keep = area.expanded(1.0);
                // No building, sign, fence span, tree trunk or prop.
                assert!(
                    laid.world
                        .obstacles
                        .near(keep)
                        .iter()
                        .all(|o| !o.intersects(&keep)),
                    "{area:?}"
                );
                // No road, nor the level crossing's signs and planks.
                for w in road.path.windows(2) {
                    let reach = keep.expanded(RoadNetwork::HALF_WIDTH);
                    assert!(reach.segment_overlap(w[0], w[1]).is_none(), "{area:?}");
                }
                assert!(crossing.at.distance(area.center()) > 15.0);
                // No flora where the platform stands: tufts would poke
                // through it, trees would grow in it.
                let chunk = laid.world.config.chunk_of(area.center());
                let flora = laid.flora.chunk(chunk);
                assert!(flora.tufts.iter().all(|t| !keep.contains_point(t.at)));
                assert!(
                    flora
                        .trees
                        .iter()
                        .all(|t| !keep.expanded(2.0).contains_point(t.at))
                );
            }
            // The walks, for their part, keep half a meter off whatever
            // is built, planted or driven, and off the lagoons and the
            // settlements; they end on the road.
            assert_eq!(bay.walks.len(), 2);
            // Each is 2 m wide along its platform's middle, from the foot of
            // the platform's west ramp.
            let station = loaded.towns[METILEO].floors()[0];
            for (walk, floor) in bay.walks.iter().zip([station, bay.platform]) {
                assert!((walk.size().y - 2.0).abs() < 1e-3, "{walk:?}");
                assert!((walk.center().y - floor.footprint.center().y).abs() < 1e-3);
                assert!((walk.max.x - (floor.footprint.min.x - 3.0)).abs() < 1e-3);
            }
            for walk in &bay.walks {
                let keep = walk.expanded(0.5);
                assert!(
                    laid.world
                        .obstacles
                        .near(keep)
                        .iter()
                        .all(|o| !o.intersects(&keep)),
                    "{walk:?}"
                );
                for lagoon in &loaded.lagoons {
                    assert!(!lagoon.keep_out().intersects(walk));
                }
                for settlement in &laid.settlements.settlements {
                    assert!(!settlement.bounds.intersects(walk));
                }
                let chunk = laid.world.config.chunk_of(walk.center());
                let flora = laid.flora.chunk(chunk);
                assert!(flora.tufts.iter().all(|t| !keep.contains_point(t.at)));
                assert!(flora.trees.iter().all(|t| !keep.contains_point(t.at)));
                let ends = [
                    Vec2::new(walk.min.x, walk.center().y),
                    Vec2::new(walk.max.x, walk.center().y),
                ];
                let by_road = ends.iter().filter(|p| road_distance(&loaded, **p) < 3.0);
                assert_eq!(by_road.count(), 1, "{walk:?}");
            }
            // The ground under the platform is level to within the 0.2 m the
            // slab reaches below it, so it sits flat.
            let ground = |p: Vec2| laid.world.terrain.height(p.x, p.y);
            let f = bay.platform.footprint;
            let heights: Vec<f32> = [
                f.min,
                f.max,
                Vec2::new(f.min.x, f.max.y),
                Vec2::new(f.max.x, f.min.y),
                f.center(),
            ]
            .into_iter()
            .map(ground)
            .collect();
            let (lo, hi) = heights
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
            assert!(hi - lo < 0.2, "{heights:?}");
        }

        /// Distance from `p` to the nearest road.
        fn road_distance(loaded: &LoadedWorld, p: Vec2) -> f32 {
            loaded.roads.distance(p)
        }

        #[test]
        fn the_roads_and_their_level_crossings_by_metileo_are_where_they_were() {
            // The bay moved opposite the platform; the road keeps its one
            // crossing of the station track (about x 189.7) and has none of
            // the bay, which now joins the run-out east of it.
            let loaded = loaded();
            let (road, crossing) = metileos_road(&loaded);
            assert!((crossing.at.x - 189.75).abs() < 0.1, "{:?}", crossing.at);
            assert!((crossing.at.y + 436.0).abs() < 1e-2);
            assert_eq!(road.crossings.len(), 1);
            let bay = loaded.line.bay_of(0).unwrap();
            let join = *bay.link.path.last().unwrap();
            assert!(join.x > crossing.at.x + 10.0, "{join:?}");
            assert_eq!(road.path[3], Vec2::new(189.93747, -440.12494));
        }

        #[test]
        fn the_station_and_bay_platforms_face_each_other_across_the_tracks() {
            let loaded = loaded();
            let bay = loaded.line.bay_of(0).unwrap();
            let station = loaded.towns[METILEO].floors()[0];
            // The station platform is x 258..318 south of the track; the
            // bay platform is the same stretch, north of the bay.
            assert_eq!(station.footprint.min.x, 258.0);
            assert_eq!(station.footprint.max.x, 318.0);
            assert!((bay.platform.footprint.min.x - 258.0).abs() < 1e-3);
            assert!((bay.platform.footprint.max.x - 318.0).abs() < 1e-3);
            assert!((bay.platform.footprint.min.y - (-436.0 + 4.5 + 4.0)).abs() < 1e-3);
            assert!(station.footprint.max.y < -436.0);
            assert_eq!(bay.buffer().x.round(), 318.0);
            assert!((bay.stop.x - 288.0).abs() < 1e-3);
        }

        #[test]
        fn a_walker_goes_from_the_station_platform_to_the_bay_platform_over_the_road_crossing() {
            let loaded = loaded();
            let (line, shuttle) = trains(&loaded);
            let bay = loaded.line.bay_of(0).unwrap();
            let (road, level) = metileos_road(&loaded);
            let (start, _) = loaded.towns[METILEO].platform_spot().unwrap();
            let mut world = crate::lay_out(&loaded).world;
            let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
            let (w0, w1) = (bay.walks[0], bay.walks[1]);
            // West along the station platform's walk to the road, along the
            // road over the level crossing, east along the bay platform's
            // walk, up its ramp.
            let end0 = Vec2::new(w0.min.x, w0.center().y);
            let end1 = Vec2::new(w1.min.x, w1.center().y);
            let near = |p: Vec2| {
                road.path
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
                    .map(|(i, _)| i)
                    .unwrap()
            };
            let (a, b) = (near(end0), near(end1));
            assert!(a < b, "the road runs north past the crossing");
            let mut route = vec![start, Vec2::new(start.x, end0.y), end0];
            route.extend(road.path[a..=b].iter().copied());
            route.extend([end1, Vec2::new(bay.platform.footprint.center().x, end1.y)]);
            // The crossing is on the way.
            assert!(route.windows(2).any(|w| {
                crossing(
                    w[0],
                    w[1],
                    crossing_rail(&loaded).0,
                    crossing_rail(&loaded).1,
                )
                .is_some()
            }));
            let mut checked = 0;
            let mut nearest = f32::MAX;
            for run in [Run::Up, Run::Down] {
                for at in line.stops_at(METILEO, run) {
                    let (l, s) = (
                        jumped(&line, at.seconds() + 30.0),
                        jumped(&shuttle, at.seconds() + 30.0),
                    );
                    if !(l.state().station == Some(2) && s.state().station == Some(0)) {
                        continue;
                    }
                    world.moving = l.obstacles().into_iter().chain(s.obstacles()).collect();
                    let mut last = world.ground_height(start);
                    for w in route.windows(2) {
                        let steps = (w[0].distance(w[1]) / 0.25).ceil() as usize;
                        for k in 0..=steps {
                            let p = w[0] + (w[1] - w[0]) * (k as f32 / steps as f32);
                            assert_eq!(world.resolve(collider, p, |_| ()), p, "blocked at {p:?}");
                            let h = world.ground_height(p);
                            assert!((h - last).abs() < 0.5, "a {} m step at {p:?}", h - last);
                            last = h;
                        }
                    }
                    // Ends on the bay platform, at its height.
                    let end = *route.last().unwrap();
                    assert!(bay.platform.footprint.contains_point(end), "{end:?}");
                    let rise = world.ground_height(end) - world.terrain.height(end.x, end.y);
                    assert!((rise - Floor::PLATFORM_HEIGHT).abs() < 1e-3, "{rise}");
                    // And back, the other way, as open.
                    for p in route.iter().rev() {
                        assert_eq!(world.resolve(collider, *p, |_| ()), *p, "{p:?}");
                    }
                    for car in world.moving.iter().map(|o| o.footprint) {
                        nearest = nearest.min(car.closest_point(level.at).distance(level.at));
                    }
                    checked += 1;
                }
            }
            assert!(checked >= 2, "{checked} moments with both trains in");
            // Neither standing train comes within 25 m of the crossing.
            assert!(nearest > 25.0, "{nearest}");
        }

        /// The station track's two points around the crossing, for a
        /// segment test.
        fn crossing_rail(loaded: &LoadedWorld) -> (Vec2, Vec2) {
            let y = loaded.towns[METILEO].railway.unwrap().start.y;
            (Vec2::new(0.0, y), Vec2::new(500.0, y))
        }

        #[test]
        fn a_standing_train_bars_the_track_by_the_platform_but_never_the_crossing_or_the_walks() {
            let loaded = loaded();
            let (line, shuttle) = trains(&loaded);
            let bay = loaded.line.bay_of(0).unwrap();
            let (_, crossing) = metileos_road(&loaded);
            let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
            world.lay_rail(&loaded.line);
            let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
            let track_y = loaded.towns[METILEO].railway.unwrap().start.y;
            let mut barred = 0;
            for run in [Run::Up, Run::Down] {
                for at in line.stops_at(METILEO, run) {
                    let l = jumped(&line, at.seconds() + 30.0);
                    if l.state().station != Some(2) {
                        continue;
                    }
                    let s = jumped(&shuttle, at.seconds() + 30.0);
                    world.moving = l.obstacles().into_iter().chain(s.obstacles()).collect();
                    // Open on the crossing, whatever stands.
                    let on = Vec2::new(crossing.at.x, track_y);
                    assert_eq!(world.resolve(collider, on, |_| ()), on);
                    // Open along both walks.
                    for walk in &bay.walks {
                        let mut x = walk.min.x;
                        while x <= walk.max.x {
                            let p = Vec2::new(x, walk.center().y);
                            assert_eq!(world.resolve(collider, p, |_| ()), p, "{p:?}");
                            x += 0.5;
                        }
                    }
                    // A standing line train bars the track at the platform.
                    let (platform, _) = loaded.towns[METILEO].platform_spot().unwrap();
                    let at_platform = Vec2::new(platform.x, track_y);
                    if world.resolve(collider, at_platform, |_| ()) != at_platform {
                        barred += 1;
                    }
                }
            }
            assert!(barred >= 2, "{barred}");
        }

        #[test]
        fn no_standing_train_covers_the_crossing_or_the_walks_and_the_trains_keep_apart() {
            let loaded = loaded();
            let (line, shuttle) = trains(&loaded);
            let bay = loaded.line.bay_of(0).unwrap();
            let (_, crossing) = metileos_road(&loaded);
            // The crossing's deck and approaches, and each walk's strip.
            let deck = Aabb::from_center_size(
                crossing.at,
                Vec2::new(2.0 * (RoadNetwork::HALF_WIDTH + 1.0), 2.0 * 10.0),
            );
            let mut keep = vec![deck];
            keep.extend(bay.walks.iter().copied());
            let (mut standing, mut tightest, mut nearest) = (0, f32::MAX, f32::MAX);
            // The moments the shuttle runs the turnout (the line train runs, or
            // is out of the way), and how near the two cars come then.
            let (mut on_turnout, mut turnout_gap) = (0, f32::MAX);
            let join = bay.link.path.last().unwrap().x;
            // One game day: every 2 s from before the line train comes in
            // to after the shuttle has left, around each of the stops.
            let mut times = Vec::new();
            for run in [Run::Up, Run::Down] {
                for at in line.stops_at(METILEO, run) {
                    let from = at.seconds() - 1800.0;
                    times.extend((0..1800).map(|k| from + 2.0 * f64::from(k)));
                }
            }
            for t in times {
                let (l, s) = (jumped(&line, t), jumped(&shuttle, t));
                let (lc, sc) = (l.obstacles(), s.obstacles());
                for a in &lc {
                    for b in &sc {
                        let gap = |x: (f32, f32), y: (f32, f32)| (x.0 - y.1).max(y.0 - x.1);
                        let (f, g) = (a.footprint, b.footprint);
                        let clear = gap((f.min.x, f.max.x), (g.min.x, g.max.x))
                            .max(gap((f.min.y, f.max.y), (g.min.y, g.max.y)));
                        tightest = tightest.min(clear);
                        let turning = s.state().phase == pampa_core::LinePhase::Running
                            && b.footprint.min.x < bay.platform.footprint.min.x
                            && b.footprint.max.x > join;
                        if turning {
                            turnout_gap = turnout_gap.min(clear);
                        }
                    }
                }
                if s.state().phase == pampa_core::LinePhase::Running
                    && sc.iter().any(|c| {
                        c.footprint.min.x < bay.platform.footprint.min.x && c.footprint.max.x > join
                    })
                {
                    on_turnout += 1;
                    // Not with the line train standing in the station: the
                    // timetable's margin keeps the shuttle off the turnout
                    // then.
                    assert!(!pampa_core::stands_at(&l.state(), 2), "t = {t}");
                }
                for (train, cars) in [(&l, &lc), (&s, &sc)] {
                    let state = train.state();
                    let here = state.station.is_some_and(|k| {
                        let town = train.stations().get(k).map(|st| st.town);
                        town == Some(METILEO) && pampa_core::stands_at(&state, k)
                    });
                    if !here {
                        continue;
                    }
                    standing += 1;
                    for car in cars {
                        for k in &keep {
                            assert!(
                                !car.footprint.intersects(k),
                                "t = {t}: {:?} on {k:?}",
                                car.footprint
                            );
                        }
                        nearest = nearest.min(
                            car.footprint
                                .closest_point(crossing.at)
                                .distance(crossing.at),
                        );
                    }
                }
            }
            assert!(standing > 1000, "{standing}");
            // The shuttle runs the turnout with the line train in, and the
            // trains never come closer than the 0.4 m the all-day test keeps.
            assert!(on_turnout > 50, "{on_turnout} samples on the turnout");
            assert!(turnout_gap > 0.4, "{turnout_gap}");
            assert!(tightest > 0.4, "{tightest}");
            assert!(nearest > 25.0, "{nearest}");
            eprintln!(
                "T3d: tightest {tightest} m, on turnout {on_turnout} samples, gap there {turnout_gap} m, nearest standing car {nearest} m"
            );
        }

        #[test]
        fn the_shuttle_is_boarded_from_caleufus_middle_spot_all_through_its_layover() {
            let loaded = loaded();
            let (_, shuttle) = trains(&loaded);
            let (middle, _) = loaded.towns[CALEUFU].platform_spot().unwrap();
            let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
            world.lay_rail(&loaded.line);
            let rider =
                pampa_core::Rider::on_foot(middle, pampa_core::Heading::from_radians(0.0), &world);
            let station = shuttle.station_of_town(CALEUFU).unwrap();
            let mut boarded = 0;
            for arrive in shuttle.arrivals_at(Run::Down, station) {
                let (leave, _) = shuttle.next_departure_after(arrive.seconds() + 1.0);
                // Arrival to departure: before and after it is turned round.
                for k in 0..=10 {
                    let t = arrive.seconds() + (leave - arrive.seconds()) * f64::from(k) / 10.0;
                    let now = jumped(&shuttle, t.clamp(arrive.seconds() + 1.0, leave - 1.0));
                    assert!(pampa_core::stands_at(&now.state(), station));
                    let got =
                        pampa_core::boarding(std::slice::from_ref(&now), &rider, &world.floors);
                    assert!(
                        matches!(got, pampa_core::Boarding::Board(_)),
                        "{t}: {got:?}"
                    );
                    boarded += 1;
                }
            }
            assert!(boarded >= 22, "{boarded}");
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
