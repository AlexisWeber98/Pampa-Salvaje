//! Builds the world and its towns from `assets/world/towns.ron`.
//!
//! The towns are needed before the app starts (the leveled terrain, the
//! crowds and the trains all come from them), when logging is not up yet,
//! so problems are kept in [`TownReport`] and logged at startup.

use std::path::Path;

use bevy::prelude::*;
use pampa_content::load_towns_or_builtin;
use pampa_core::{PlaceKind, TownLayout, WorldConfig, place_towns};
use pampa_render::font;

/// The world to build, its towns, and what went wrong reading them.
#[derive(Debug, Clone)]
pub struct LoadedWorld {
    pub world: WorldConfig,
    /// Every valid town, in file order; never empty. The player starts at
    /// the first one.
    pub towns: Vec<TownLayout>,
    /// Problems found, ready to log as warnings.
    pub warnings: Vec<String>,
}

/// Loads the towns file at `path` and places every valid town in its world,
/// falling back to the built-in Eduardo Castex when no town can be built.
pub fn load_world(path: &Path) -> LoadedWorld {
    let (content, errors) = load_towns_or_builtin(path);
    let mut warnings: Vec<String> = errors.iter().map(|e| format!("towns: {e}")).collect();
    let mut world = content.world;
    // Content already placed these towns once; this only builds them.
    let (mut towns, rejected) = place_towns(&world, &content.towns);
    warnings.extend(rejected.iter().map(|e| format!("towns: {e}")));
    if towns.is_empty() {
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
    LoadedWorld {
        world,
        towns,
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
    use pampa_core::{Terrain, Townsfolk, Train, TrainSchedule, Vec2, World};

    fn temp_towns(name: &str, text: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pampa-game-towns-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("towns.ron");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_shipped_file_builds_castex_first_without_warnings() {
        let loaded = load_world(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.world, WorldConfig::default());
        assert_eq!(loaded.towns[0], TownLayout::castex_inspired());
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
        let train = Train::for_town(&town, TrainSchedule::default()).expect("a railway");
        assert_eq!(Some(train.railway), town.railway);
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
