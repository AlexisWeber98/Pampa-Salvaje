//! Builds the world's town from `assets/world/towns.ron`.
//!
//! The town is needed before the app starts (the leveled terrain, the
//! crowd and the train all come from it), when logging is not up yet, so
//! problems are kept in [`TownReport`] and logged at startup.

use std::path::Path;

use bevy::prelude::*;
use pampa_content::load_towns_or_builtin;
use pampa_core::{PlaceKind, TownLayout};
use pampa_render::font;

/// The town to build, and what went wrong reading it.
#[derive(Debug, Clone)]
pub struct LoadedTown {
    pub town: TownLayout,
    /// Problems found, ready to log as warnings.
    pub warnings: Vec<String>,
    /// How many towns the file lists (only the first is built for now).
    pub listed: usize,
}

/// Loads the towns file at `path` and generates its first town, falling
/// back to the built-in Eduardo Castex when the file cannot be used.
pub fn load_town(path: &Path) -> LoadedTown {
    let (configs, errors) = load_towns_or_builtin(path);
    let mut warnings: Vec<String> = errors.iter().map(|e| format!("towns: {e}")).collect();
    // Every returned config was validated by generating it.
    let town = TownLayout::generate(&configs[0]).unwrap_or_else(|error| {
        warnings.push(format!("towns: {error}; using the built-in Eduardo Castex"));
        TownLayout::castex_inspired()
    });
    let names = town
        .places()
        .into_iter()
        .filter(|p| p.kind == PlaceKind::Station);
    for place in names {
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
    LoadedTown {
        town,
        warnings,
        listed: configs.len(),
    }
}

/// What loading the town reported, logged once at startup.
#[derive(Resource, Debug, Clone, Default)]
pub struct TownReport {
    pub name: String,
    pub warnings: Vec<String>,
    pub listed: usize,
}

impl TownReport {
    pub fn new(loaded: &LoadedTown) -> Self {
        Self {
            name: loaded.town.name.clone(),
            warnings: loaded.warnings.clone(),
            listed: loaded.listed,
        }
    }
}

/// Logs the town that was built and any problems loading it.
pub fn log_town_report(report: Res<TownReport>) {
    for warning in &report.warnings {
        warn!("{warning}");
    }
    info!("town: {}", report.name);
    if report.listed > 1 {
        info!(
            "towns: {} listed, only the first is built for now",
            report.listed
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_content::{TOWNS_FILE, asset_path};
    use pampa_core::{Terrain, Townsfolk, Train, TrainSchedule, World};

    fn temp_towns(name: &str, text: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pampa-game-towns-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("towns.ron");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_shipped_file_builds_castex_without_warnings() {
        let loaded = load_town(&asset_path(TOWNS_FILE));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.town, TownLayout::castex_inspired());
    }

    #[test]
    fn the_world_follows_the_loaded_town() {
        let path = temp_towns(
            "small",
            r#"(towns: [(name: "General Alvear", blocks: (3, 4), block_size: 34.0,
                street_width: 9.0, railway: Some((side: South, offset: 26.0)), seed: 3)])"#,
        );
        let loaded = load_town(&path);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let town = loaded.town;
        assert_eq!(town.name, "General Alvear");
        assert_eq!(town.streets.len(), 4 + 5);
        let world = World::with_town(Terrain::default(), &town);
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
    fn a_bad_file_falls_back_to_castex_with_warnings() {
        let path = temp_towns(
            "bad",
            r#"(towns: [(name: "Ciudad #1", blocks: (3, 3), block_size: 40.0, street_width: 2.0)])"#,
        );
        let loaded = load_town(&path);
        assert_eq!(loaded.town, TownLayout::castex_inspired());
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
        let loaded = load_town(&path);
        assert_eq!(loaded.town.name, "Ciudad #1");
        assert!(
            loaded.warnings.iter().any(|w| w.contains("'#'")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
