//! The world and its towns, read from `assets/world/towns.ron`.
//!
//! The file mirrors [`pampa_core::WorldConfig`] and
//! [`pampa_core::TownConfig`] through serde DTOs, so the core crate stays
//! free of dependencies. The `world` section is optional (older files only
//! list towns). Every town is validated by placing it in the world with
//! [`pampa_core::place_towns`]: invalid towns, towns past the world edge
//! and towns overlapping an earlier one are reported and skipped, and when
//! no valid town is left the built-in Eduardo Castex config takes over.
//!
//! The optional `line` lists the railway towns in the order the railway
//! line joins them; the game checks it with [`pampa_core::RailLine::build`]
//! once the towns are built.

use std::path::Path;

use pampa_core::{PlazaPlacement, RailwayConfig, Side, TownConfig, Vec2, WorldConfig};
use serde::Deserialize;

use crate::ContentError;

/// Where the world and its towns live, relative to the assets folder.
pub const TOWNS_FILE: &str = "world/towns.ron";

/// The world to build: its size and its valid towns, in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldContent {
    pub world: WorldConfig,
    pub towns: Vec<TownConfig>,
    /// Town names in railway line order, as written; `None` lets the line
    /// run through every railway town west to east.
    pub line: Option<Vec<String>>,
}

impl WorldContent {
    /// The default world with only the built-in Eduardo Castex.
    pub fn builtin() -> Self {
        Self {
            world: WorldConfig::default(),
            towns: vec![TownConfig::castex()],
            line: None,
        }
    }
}

/// The towns file: an optional world section, a list of towns and an
/// optional railway line. Unknown fields are ignored, so newer files still
/// load in older builds.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct TownsFile {
    #[serde(default)]
    world: WorldDto,
    towns: Vec<TownDto>,
    #[serde(default)]
    line: Option<Vec<String>>,
}

/// The `world` section (see [`WorldConfig`]); each field is optional.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
struct WorldDto {
    #[serde(default = "default_half_extent")]
    half_extent: f32,
    #[serde(default = "default_chunk_size")]
    chunk_size: f32,
}

impl Default for WorldDto {
    fn default() -> Self {
        Self {
            half_extent: default_half_extent(),
            chunk_size: default_chunk_size(),
        }
    }
}

fn default_half_extent() -> f32 {
    WorldConfig::DEFAULT_HALF_EXTENT
}

fn default_chunk_size() -> f32 {
    WorldConfig::DEFAULT_CHUNK_SIZE
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
    /// Overrides the size tier's police station count (`Some(0)`: none).
    #[serde(default)]
    police_stations: Option<usize>,
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
            police_stations: town.police_stations,
        }
    }
}

/// Parses the RON text of a towns file. An unusable `world` section is
/// reported and replaced by the default world. Every town is checked by
/// placing it in the world: the valid ones are returned, in file order,
/// with an error for each one left out.
pub fn parse_towns(text: &str) -> Result<(WorldContent, Vec<ContentError>), ContentError> {
    let file: TownsFile = ron::from_str(text).map_err(ContentError::Parse)?;
    let mut errors = Vec::new();
    let world = WorldConfig {
        half_extent: file.world.half_extent,
        chunk_size: file.world.chunk_size,
    };
    let world = world.validated().unwrap_or_else(|error| {
        errors.push(ContentError::World(error));
        WorldConfig::default()
    });
    let configs: Vec<TownConfig> = file.towns.into_iter().map(TownConfig::from).collect();
    let (_, rejected) = pampa_core::place_towns(&world, &configs);
    let left_out: Vec<usize> = rejected.iter().map(|e| e.index()).collect();
    let towns = configs
        .into_iter()
        .enumerate()
        .filter(|(index, _)| !left_out.contains(index))
        .map(|(_, config)| config)
        .collect();
    errors.extend(rejected.into_iter().map(ContentError::Town));
    let line = file.line;
    Ok((WorldContent { world, towns, line }, errors))
}

/// Reads and parses the towns file at `path` (see [`parse_towns`]).
pub fn load_towns(path: &Path) -> Result<(WorldContent, Vec<ContentError>), ContentError> {
    let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_towns(&text)
}

/// Loads the towns file at `path`, skipping invalid towns and falling back
/// to [`WorldContent::builtin`] when the file is missing or broken, and to
/// [`TownConfig::castex`] (in the file's world) when it has no valid town.
/// Every problem is returned so the caller can log it.
pub fn load_towns_or_builtin(path: &Path) -> (WorldContent, Vec<ContentError>) {
    let (mut content, mut errors) = match load_towns(path) {
        Ok(loaded) => loaded,
        Err(error) => return (WorldContent::builtin(), vec![error]),
    };
    if content.towns.is_empty() {
        errors.push(ContentError::NoTowns);
        content.towns = vec![TownConfig::castex()];
    }
    (content, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_path;
    use pampa_core::TownSize;

    const SHIPPED: &str = include_str!("../../../assets/world/towns.ron");

    fn temp_file(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pampa-towns-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("towns.ron");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_shipped_file_holds_the_three_towns_of_the_line_in_the_largest_world() {
        let (content, errors) = parse_towns(SHIPPED).expect("shipped towns.ron is valid");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            content.world,
            WorldConfig {
                half_extent: WorldConfig::MAX_HALF_EXTENT,
                chunk_size: WorldConfig::DEFAULT_CHUNK_SIZE,
            }
        );
        assert_eq!(content.world.validated(), Ok(content.world));
        let names: Vec<&str> = content.towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Monte Nievas", "Eduardo Castex", "General Pico"]);
        let north = Some(RailwayConfig {
            side: Side::North,
            offset: 24.0,
        });
        // Real offsets in meters, scaled 1:7 from Monte Nievas, which keeps
        // its origin, and rounded to the meter (see the README).
        let nievas = Vec2::new(-2450.0, -1950.0);
        let scaled = |east: f32, north: f32| {
            Vec2::new(
                (nievas.x + east / 7.0).round(),
                (nievas.y + north / 7.0).round(),
            )
        };
        for (town, origin, blocks) in [
            (&content.towns[0], nievas, (4, 3)),
            (&content.towns[1], scaled(-12_600.0, -5_400.0), (5, 4)),
            (&content.towns[2], scaled(35_800.0, 22_400.0), (8, 8)),
        ] {
            assert_eq!(town.origin, origin, "{}", town.name);
            assert_eq!(town.blocks, blocks, "{}", town.name);
            assert_eq!((town.block_size, town.street_width), (40.0, 10.0));
            assert_eq!(town.railway, north, "{}", town.name);
            assert_eq!(town.plaza, PlazaPlacement::Auto, "{}", town.name);
        }
        // Monte Nievas and Castex are small, General Pico very big.
        let sizes: Vec<TownSize> = content.towns.iter().map(TownConfig::size).collect();
        assert_eq!(sizes, [TownSize::Small, TownSize::Small, TownSize::VeryBig]);
        let (placed, errors) = pampa_core::place_towns(&content.world, &content.towns);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(placed.len(), content.towns.len());
        // The railway line runs Castex, Monte Nievas, General Pico.
        let line = content.line.as_deref().expect("an explicit line");
        assert_eq!(line, ["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (rail, errors) =
            pampa_core::RailLine::build(Some(line), &placed, content.world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(rail.links.len(), 2);
    }

    #[test]
    fn the_line_is_read_and_optional() {
        let town = r#"(name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0)"#;
        let (content, _) = parse_towns(&format!("(towns: [{town}])")).unwrap();
        assert_eq!(content.line, None);
        let (content, errors) =
            parse_towns(&format!(r#"(towns: [{town}], line: Some(["A", "B"]))"#)).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        // Names are only read here; the game checks them against the towns.
        assert_eq!(content.line, Some(vec!["A".to_string(), "B".to_string()]));
    }

    #[test]
    fn the_readme_town_example_fits_next_to_the_shipped_towns() {
        const README: &str = include_str!("../../../assets/world/README.md");
        let section = &README[README.find("### Adding another town").expect("section")..];
        let start = section.find("```ron").expect("example") + "```ron".len();
        let example = &section[start..start + section[start..].find("```").unwrap()];
        let shipped = SHIPPED.rfind("],").expect("end of the town list");
        let text = format!("{}{example}{}", &SHIPPED[..shipped], &SHIPPED[shipped..]);
        let (content, errors) = parse_towns(&text).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let last = content.towns.last().unwrap();
        assert_eq!(last.name, "Winifreda");
        assert_eq!(last.origin, Vec2::new(3000.0, -2000.0));
    }

    #[test]
    fn the_world_section_is_read_and_optional() {
        let town = r#"(name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0)"#;
        let (content, errors) = parse_towns(&format!(
            "(world: (half_extent: 2000.0, chunk_size: 100.0), towns: [{town}])"
        ))
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            content.world,
            WorldConfig {
                half_extent: 2000.0,
                chunk_size: 100.0,
            }
        );
        // Older files: no world section at all, or only some fields.
        let (content, _) = parse_towns(&format!("(towns: [{town}])")).unwrap();
        assert_eq!(content.world, WorldConfig::default());
        let (content, _) =
            parse_towns(&format!("(world: (chunk_size: 250.0), towns: [{town}])")).unwrap();
        assert_eq!(content.world.half_extent, WorldConfig::DEFAULT_HALF_EXTENT);
        assert_eq!(content.world.chunk_size, 250.0);
        assert_eq!(content.towns.len(), 1);
    }

    #[test]
    fn a_bad_world_section_falls_back_to_the_default_world() {
        let text = r#"(world: (half_extent: 20000.0), towns: [
            (name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0),
        ])"#;
        let (content, errors) = parse_towns(text).unwrap();
        assert_eq!(content.world, WorldConfig::default());
        assert_eq!(content.towns.len(), 1);
        assert!(matches!(errors[..], [ContentError::World(_)]), "{errors:?}");
        let message = errors[0].to_string();
        assert!(
            message.contains("half_extent") && message.contains("default world"),
            "{message}"
        );
    }

    #[test]
    fn overlapping_and_far_out_towns_are_skipped_naming_them() {
        let text = r#"(towns: [
            (name: "Eduardo Castex", blocks: (4, 5), block_size: 40.0, street_width: 10.0,
             railway: Some((side: North, offset: 24.0)), seed: 7),
            (name: "Florencio Varela", blocks: (3, 3), block_size: 40.0, street_width: 10.0),
            (name: "Afuera", origin: (5000.0, 0.0), blocks: (3, 3), block_size: 40.0,
             street_width: 10.0),
            (name: "Lejos", origin: (3000.0, -2000.0), blocks: (3, 3), block_size: 40.0,
             street_width: 10.0, railway: Some((side: South, offset: 30.0))),
        ])"#;
        let (content, errors) = parse_towns(text).unwrap();
        let names: Vec<&str> = content.towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Eduardo Castex", "Lejos"]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert!(
            messages[0].contains("Florencio Varela") && messages[0].contains("Eduardo Castex"),
            "{}",
            messages[0]
        );
        assert!(
            messages[1].contains("Afuera") && messages[1].contains("world edge"),
            "{}",
            messages[1]
        );
    }

    #[test]
    fn the_shipped_file_is_found_and_loaded_from_disk() {
        let path = asset_path(TOWNS_FILE);
        assert!(path.is_file(), "{path:?}");
        let (content, errors) = load_towns_or_builtin(&path);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(content, parse_towns(SHIPPED).unwrap().0);
    }

    #[test]
    fn every_field_maps_onto_the_core_config() {
        let text = r#"
            // Comments and unknown fields are fine.
            (
                towns: [
                    (
                        name: "General Alvear",
                        origin: (1500.0, -2000.0),
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
        let (content, errors) = parse_towns(text).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            content.towns,
            [
                TownConfig {
                    name: "General Alvear".into(),
                    origin: Vec2::new(1500.0, -2000.0),
                    blocks: (3, 4),
                    block_size: 36.0,
                    street_width: 9.0,
                    railway: Some(RailwayConfig {
                        side: Side::West,
                        offset: 30.0,
                    }),
                    plaza: PlazaPlacement::Block(1, 2),
                    seed: 11,
                    police_stations: None,
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
                    police_stations: None,
                },
            ]
        );
        for plaza in ["Auto", "Center", "FacingStation"] {
            let text = format!(
                r#"(towns: [(name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0,
                    railway: Some((side: North, offset: 24.0)), plaza: {plaza})])"#
            );
            let (content, _) = parse_towns(&text).unwrap();
            assert_eq!(content.towns.len(), 1, "{plaza}");
        }
    }

    #[test]
    fn the_shipped_monte_nievas_has_no_comisaria_and_castex_has_one() {
        use pampa_core::{BuildingKind, TownLayout};
        let (content, _) = parse_towns(SHIPPED).unwrap();
        let police = |name: &str| {
            let config = content.towns.iter().find(|t| t.name == name).unwrap();
            let town = TownLayout::generate(config).unwrap();
            let n = town
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::PoliceStation)
                .count();
            (config.police_stations, n)
        };
        assert_eq!(police("Monte Nievas"), (Some(0), 0));
        assert_eq!(police("Eduardo Castex"), (None, 1));
        assert_eq!(police("General Pico"), (None, 2));
    }

    #[test]
    fn the_police_station_count_is_read_and_checked_against_the_town_size() {
        let text = r#"(towns: [
            (name: "Sin", blocks: (3, 3), block_size: 40.0, street_width: 10.0,
             police_stations: Some(0)),
            (name: "Dos", origin: (2000.0, 0.0), blocks: (3, 3), block_size: 40.0,
             street_width: 10.0, police_stations: Some(2)),
        ])"#;
        let (content, errors) = parse_towns(text).unwrap();
        assert_eq!(content.towns.len(), 1);
        assert_eq!(content.towns[0].police_stations, Some(0));
        assert_eq!(errors.len(), 1);
        let message = errors[0].to_string();
        assert!(
            message.contains("Dos") && message.contains("police_stations 2"),
            "{message}"
        );
    }

    #[test]
    fn invalid_towns_are_skipped_with_a_clear_error() {
        let text = r#"(towns: [
            (name: "Chica", blocks: (2, 3), block_size: 40.0, street_width: 10.0),
            (name: "Buena", blocks: (3, 3), block_size: 40.0, street_width: 10.0),
            (name: "Par", origin: (2000.0, 0.0), blocks: (4, 4), block_size: 40.0,
             street_width: 10.0, plaza: Center),
        ])"#;
        let (content, errors) = parse_towns(text).unwrap();
        assert_eq!(content.towns.len(), 1);
        assert_eq!(content.towns[0].name, "Buena");
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
        assert!(matches!(&errors[0], ContentError::Town(e) if e.index() == 0));
    }

    #[test]
    fn missing_broken_or_empty_files_fall_back_to_castex() {
        let castex = vec![TownConfig::castex()];
        let (content, errors) = load_towns_or_builtin(Path::new("no/such/dir/towns.ron"));
        assert_eq!(content, WorldContent::builtin());
        assert!(matches!(errors[..], [ContentError::Io { .. }]));

        let broken = temp_file("broken", "(towns: [");
        let (content, errors) = load_towns_or_builtin(&broken);
        assert_eq!(content, WorldContent::builtin());
        assert!(matches!(errors[..], [ContentError::Parse(_)]));

        // An empty town list keeps the file's world.
        let empty = temp_file("empty", "(world: (half_extent: 1000.0), towns: [])");
        let (content, errors) = load_towns_or_builtin(&empty);
        assert_eq!(content.towns, castex);
        assert_eq!(content.world.half_extent, 1000.0);
        assert!(errors[0].to_string().contains("no towns"), "{}", errors[0]);

        let all_bad = temp_file(
            "bad",
            r#"(towns: [(name: "X", blocks: (3, 3), block_size: 10.0, street_width: 10.0)])"#,
        );
        let (content, errors) = load_towns_or_builtin(&all_bad);
        assert_eq!(content.towns, castex);
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
