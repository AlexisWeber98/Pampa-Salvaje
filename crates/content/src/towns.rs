//! Town configs, read from `assets/world/towns.ron`.
//!
//! The file mirrors [`pampa_core::TownConfig`] through serde DTOs, so the
//! core crate stays free of dependencies. Every town is validated by
//! generating it; invalid towns are reported and skipped, and when no valid
//! town is left the built-in Eduardo Castex config takes over.

use std::path::Path;

use pampa_core::{PlazaPlacement, RailwayConfig, Side, TownConfig, TownLayout, Vec2};
use serde::Deserialize;

use crate::ContentError;

/// Where the towns live, relative to the assets folder.
pub const TOWNS_FILE: &str = "world/towns.ron";

/// The towns file: a list of towns. Unknown fields are ignored, so newer
/// files still load in older builds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct TownsFile {
    towns: Vec<TownDto>,
}

/// One town as written in the file (see [`TownConfig`] for each field).
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct TownDto {
    name: String,
    #[serde(default)]
    origin: (f32, f32),
    blocks: (usize, usize),
    block_size: f32,
    street_width: f32,
    #[serde(default)]
    railway: Option<RailwayDto>,
    #[serde(default)]
    plaza: PlazaDto,
    #[serde(default)]
    seed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct RailwayDto {
    side: SideDto,
    offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
enum SideDto {
    North,
    South,
    East,
    West,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
enum PlazaDto {
    #[default]
    Auto,
    Center,
    FacingStation,
    Block(usize, usize),
}

impl From<SideDto> for Side {
    fn from(side: SideDto) -> Self {
        match side {
            SideDto::North => Side::North,
            SideDto::South => Side::South,
            SideDto::East => Side::East,
            SideDto::West => Side::West,
        }
    }
}

impl From<PlazaDto> for PlazaPlacement {
    fn from(plaza: PlazaDto) -> Self {
        match plaza {
            PlazaDto::Auto => PlazaPlacement::Auto,
            PlazaDto::Center => PlazaPlacement::Center,
            PlazaDto::FacingStation => PlazaPlacement::FacingStation,
            PlazaDto::Block(i, j) => PlazaPlacement::Block(i, j),
        }
    }
}

impl From<TownDto> for TownConfig {
    fn from(town: TownDto) -> Self {
        TownConfig {
            name: town.name,
            origin: Vec2::new(town.origin.0, town.origin.1),
            blocks: town.blocks,
            block_size: town.block_size,
            street_width: town.street_width,
            railway: town.railway.map(|r| RailwayConfig {
                side: r.side.into(),
                offset: r.offset,
            }),
            plaza: town.plaza.into(),
            seed: town.seed,
        }
    }
}

/// Parses the RON text of a towns file. Every town is checked by
/// generating it: the valid ones are returned, in file order, with an
/// error for each invalid one.
pub fn parse_towns(text: &str) -> Result<(Vec<TownConfig>, Vec<ContentError>), ContentError> {
    let file: TownsFile = ron::from_str(text).map_err(ContentError::Parse)?;
    let mut towns = Vec::new();
    let mut errors = Vec::new();
    for (index, dto) in file.towns.into_iter().enumerate() {
        let config = TownConfig::from(dto);
        match TownLayout::generate(&config) {
            Ok(_) => towns.push(config),
            Err(error) => errors.push(ContentError::Town {
                index,
                name: config.name,
                error,
            }),
        }
    }
    Ok((towns, errors))
}

/// Reads and parses the towns file at `path` (see [`parse_towns`]).
pub fn load_towns(path: &Path) -> Result<(Vec<TownConfig>, Vec<ContentError>), ContentError> {
    let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_towns(&text)
}

/// Loads the towns file at `path`, skipping invalid towns and falling back
/// to [`TownConfig::castex`] when the file is missing, broken or has no
/// valid town. Every problem is returned so the caller can log it.
pub fn load_towns_or_builtin(path: &Path) -> (Vec<TownConfig>, Vec<ContentError>) {
    let (towns, mut errors) = match load_towns(path) {
        Ok(loaded) => loaded,
        Err(error) => return (vec![TownConfig::castex()], vec![error]),
    };
    if towns.is_empty() {
        errors.push(ContentError::NoTowns);
        return (vec![TownConfig::castex()], errors);
    }
    (towns, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_path;

    const SHIPPED: &str = include_str!("../../../assets/world/towns.ron");

    fn temp_file(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pampa-towns-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("towns.ron");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_shipped_file_holds_the_castex_config() {
        let (towns, errors) = parse_towns(SHIPPED).expect("shipped towns.ron is valid");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(towns.first(), Some(&TownConfig::castex()));
        for town in &towns {
            TownLayout::generate(town).expect("every shipped town is valid");
        }
    }

    #[test]
    fn the_shipped_file_is_found_and_loaded_from_disk() {
        let path = asset_path(TOWNS_FILE);
        assert!(path.is_file(), "{path:?}");
        let (towns, errors) = load_towns_or_builtin(&path);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(towns, parse_towns(SHIPPED).unwrap().0);
    }

    #[test]
    fn every_field_maps_onto_the_core_config() {
        let text = r#"
            // Comments and unknown fields are fine.
            (
                towns: [
                    (
                        name: "General Alvear",
                        origin: (10.0, -20.0),
                        blocks: (3, 4),
                        block_size: 36.0,
                        street_width: 9.0,
                        railway: Some((side: West, offset: 30.0)),
                        plaza: Block(1, 2),
                        seed: 11,
                        population: 900,
                    ),
                    (
                        name: "Carmensa",
                        blocks: (3, 3),
                        block_size: 32.0,
                        street_width: 8.0,
                    ),
                ],
                roads: [],
            )
        "#;
        let (towns, errors) = parse_towns(text).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            towns,
            [
                TownConfig {
                    name: "General Alvear".into(),
                    origin: Vec2::new(10.0, -20.0),
                    blocks: (3, 4),
                    block_size: 36.0,
                    street_width: 9.0,
                    railway: Some(RailwayConfig {
                        side: Side::West,
                        offset: 30.0,
                    }),
                    plaza: PlazaPlacement::Block(1, 2),
                    seed: 11,
                },
                TownConfig {
                    name: "Carmensa".into(),
                    origin: Vec2::ZERO,
                    blocks: (3, 3),
                    block_size: 32.0,
                    street_width: 8.0,
                    railway: None,
                    plaza: PlazaPlacement::Auto,
                    seed: 0,
                },
            ]
        );
        for plaza in ["Auto", "Center", "FacingStation"] {
            let text = format!(
                r#"(towns: [(name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0,
                    railway: Some((side: North, offset: 24.0)), plaza: {plaza})])"#
            );
            let (towns, _) = parse_towns(&text).unwrap();
            assert_eq!(towns.len(), 1, "{plaza}");
        }
    }

    #[test]
    fn invalid_towns_are_skipped_with_a_clear_error() {
        let text = r#"(towns: [
            (name: "Chica", blocks: (2, 3), block_size: 40.0, street_width: 10.0),
            (name: "Buena", blocks: (3, 3), block_size: 40.0, street_width: 10.0),
            (name: "Par", blocks: (4, 4), block_size: 40.0, street_width: 10.0, plaza: Center),
        ])"#;
        let (towns, errors) = parse_towns(text).unwrap();
        assert_eq!(towns.len(), 1);
        assert_eq!(towns[0].name, "Buena");
        assert_eq!(errors.len(), 2);
        let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            messages[0].contains("Chica") && messages[0].contains("too few"),
            "{}",
            messages[0]
        );
        assert!(
            messages[1].contains("Par") && messages[1].contains("central block"),
            "{}",
            messages[1]
        );
        assert!(matches!(errors[0], ContentError::Town { index: 0, .. }));
    }

    #[test]
    fn missing_broken_or_empty_files_fall_back_to_castex() {
        let castex = vec![TownConfig::castex()];
        let (towns, errors) = load_towns_or_builtin(Path::new("no/such/dir/towns.ron"));
        assert_eq!(towns, castex);
        assert!(matches!(errors[..], [ContentError::Io { .. }]));

        let broken = temp_file("broken", "(towns: [");
        let (towns, errors) = load_towns_or_builtin(&broken);
        assert_eq!(towns, castex);
        assert!(matches!(errors[..], [ContentError::Parse(_)]));

        let empty = temp_file("empty", "(towns: [])");
        let (towns, errors) = load_towns_or_builtin(&empty);
        assert_eq!(towns, castex);
        assert!(errors[0].to_string().contains("no towns"), "{}", errors[0]);

        let all_bad = temp_file(
            "bad",
            r#"(towns: [(name: "X", blocks: (3, 3), block_size: 10.0, street_width: 10.0)])"#,
        );
        let (towns, errors) = load_towns_or_builtin(&all_bad);
        assert_eq!(towns, castex);
        assert!(
            errors[0].to_string().contains("block_size 10 m"),
            "{}",
            errors[0]
        );
        // The fallback is announced too.
        assert!(errors.iter().any(|e| matches!(e, ContentError::NoTowns)));

        for path in [broken, empty, all_bad] {
            std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
    }
}
