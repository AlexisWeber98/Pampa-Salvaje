//! Placing many towns in one world: each is generated at its own `origin`,
//! then checked against the world edge and the towns placed before it.

use std::fmt;

use crate::{Aabb, TownConfig, TownConfigError, TownLayout, WorldConfig};

/// Why a town was left out of the world.
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementError {
    /// The town's own config cannot be built.
    Invalid {
        /// Position of the town in the list.
        index: usize,
        name: String,
        error: TownConfigError,
    },
    /// The town, its leveled band or its railway reach past the world edge.
    OutsideWorld {
        index: usize,
        name: String,
        /// Farthest the town reaches from the world center along an axis.
        reach: f32,
        /// The world's half extent.
        limit: f32,
    },
    /// The town (with its leveled band and railway) overlaps a town placed
    /// before it.
    OverlapsTown {
        index: usize,
        name: String,
        /// The town already there.
        other: String,
    },
}

impl PlacementError {
    /// Position of the rejected town in the list.
    pub fn index(&self) -> usize {
        match self {
            Self::Invalid { index, .. }
            | Self::OutsideWorld { index, .. }
            | Self::OverlapsTown { index, .. } => *index,
        }
    }

    /// Name of the rejected town.
    pub fn name(&self) -> &str {
        match self {
            Self::Invalid { name, .. }
            | Self::OutsideWorld { name, .. }
            | Self::OverlapsTown { name, .. } => name,
        }
    }
}

impl fmt::Display for PlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (index, name) = (self.index(), self.name());
        write!(f, "town {index} `{name}`: ")?;
        match self {
            Self::Invalid { error, .. } => write!(f, "{error}"),
            Self::OutsideWorld { reach, limit, .. } => write!(
                f,
                "the town, its leveled band and its railway reach {reach:.0} m from the world \
                 center, past the {limit:.0} m world edge"
            ),
            Self::OverlapsTown { other, .. } => write!(
                f,
                "overlaps `{other}`: towns, their leveled bands and their railways must not touch"
            ),
        }
    }
}

impl std::error::Error for PlacementError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// Generates every town of `configs` in `world`, in order. A town is left
/// out (with an error naming it) when its config is invalid, when it
/// reaches past the world edge, or when it overlaps a town placed before
/// it. The placed towns come back in list order.
pub fn place_towns(
    world: &WorldConfig,
    configs: &[TownConfig],
) -> (Vec<TownLayout>, Vec<PlacementError>) {
    let limit = world.half_extent;
    let mut placed: Vec<TownLayout> = Vec::new();
    let mut errors = Vec::new();
    for (index, config) in configs.iter().enumerate() {
        let name = config.name.clone();
        let town = match TownLayout::generate(config) {
            Ok(town) => town,
            Err(error) => {
                errors.push(PlacementError::Invalid { index, name, error });
                continue;
            }
        };
        let reach = reach(&town);
        if reach > limit {
            errors.push(PlacementError::OutsideWorld {
                index,
                name,
                reach,
                limit,
            });
            continue;
        }
        if let Some(other) = placed.iter().find(|p| clash(p, &town)) {
            errors.push(PlacementError::OverlapsTown {
                index,
                name,
                other: other.name.clone(),
            });
            continue;
        }
        placed.push(town);
    }
    (placed, errors)
}

/// Farthest the town's leveled ground or railway reaches from the world
/// center along either axis, in meters.
fn reach(town: &TownLayout) -> f32 {
    std::iter::once(town.leveled())
        .chain(town.rail_corridor())
        .flat_map(|b| [b.min.x, b.min.y, b.max.x, b.max.y])
        .map(f32::abs)
        .fold(0.0, f32::max)
}

/// Whether two towns get in each other's way: either one's leveled ground
/// meets the other's, or a railway runs across the other town.
fn clash(a: &TownLayout, b: &TownLayout) -> bool {
    let rails = |t: &TownLayout| t.rail_corridor().into_iter().collect::<Vec<Aabb>>();
    a.leveled().intersects(&b.leveled())
        || rails(a).iter().any(|r| r.intersects(&b.leveled()))
        || rails(b).iter().any(|r| r.intersects(&a.leveled()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    fn at(name: &str, x: f32, y: f32) -> TownConfig {
        TownConfig {
            name: name.into(),
            origin: Vec2::new(x, y),
            ..TownConfig::castex()
        }
    }

    #[test]
    fn distant_towns_are_all_placed_in_order() {
        let configs = [
            at("Castex", 0.0, 0.0),
            at("Lejos", 3000.0, -2000.0),
            at("Norte", -1500.0, 3000.0),
        ];
        let (towns, errors) = place_towns(&WorldConfig::default(), &configs);
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<&str> = towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Castex", "Lejos", "Norte"]);
        for (c, t) in configs.iter().zip(&towns) {
            assert_eq!(t.grid().center(), c.origin);
        }
    }

    #[test]
    fn overlapping_towns_are_rejected_naming_both() {
        let configs = [
            at("Eduardo Castex", 0.0, 0.0),
            at("Florencio Varela", 0.0, 0.0),
            at("Vecina", 300.0, 0.0),
            at("Lejos", 3000.0, -2000.0),
        ];
        let (towns, errors) = place_towns(&WorldConfig::default(), &configs);
        let names: Vec<&str> = towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Eduardo Castex", "Lejos"]);
        assert_eq!(errors.len(), 2);
        for (error, index) in errors.iter().zip([1, 2]) {
            assert!(
                matches!(error, PlacementError::OverlapsTown { other, .. } if other == "Eduardo Castex"),
                "{error}"
            );
            assert_eq!(error.index(), index);
            let message = error.to_string();
            assert!(message.contains(configs[index].name.as_str()), "{message}");
            assert!(message.contains("Eduardo Castex"), "{message}");
        }
    }

    #[test]
    fn a_railway_running_through_another_town_is_an_overlap() {
        // A railway-less town on the line of Castex's track, east of the
        // grid but inside the track's run-out.
        let castex = TownLayout::castex_inspired();
        let rail = castex.railway.expect("a railway");
        let configs = [
            TownConfig::castex(),
            TownConfig {
                name: "Sobre la vía".into(),
                origin: Vec2::new(rail.end.x - 60.0, rail.start.y),
                railway: None,
                blocks: (3, 3),
                ..TownConfig::castex()
            },
        ];
        let (towns, errors) = place_towns(&WorldConfig::default(), &configs);
        assert_eq!(towns.len(), 1);
        assert!(
            matches!(&errors[..], [PlacementError::OverlapsTown { index: 1, .. }]),
            "{errors:?}"
        );
    }

    #[test]
    fn towns_past_the_world_edge_are_rejected() {
        let world = WorldConfig::default();
        let configs = [at("Borde", 3900.0, 0.0), at("Castex", 0.0, 0.0)];
        let (towns, errors) = place_towns(&world, &configs);
        assert_eq!(towns.len(), 1);
        assert_eq!(towns[0].name, "Castex");
        let [error] = &errors[..] else {
            panic!("{errors:?}")
        };
        assert!(
            matches!(error, PlacementError::OutsideWorld { index: 0, limit, .. } if *limit == 4000.0),
            "{error}"
        );
        let message = error.to_string();
        assert!(
            message.contains("Borde") && message.contains("world edge"),
            "{message}"
        );
        // A smaller world rejects even the town at the center.
        let small = WorldConfig {
            half_extent: 300.0,
            chunk_size: 100.0,
        };
        let (towns, errors) = place_towns(&small, &[TownConfig::castex()]);
        assert!(towns.is_empty());
        assert!(matches!(errors[..], [PlacementError::OutsideWorld { .. }]));
    }

    #[test]
    fn placed_towns_reach_only_inside_the_world() {
        let world = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let bounds = world.bounds();
        let inside = Aabb::new(bounds.min, bounds.max);
        let configs: Vec<TownConfig> = crate::town::tests::sample_configs()
            .into_iter()
            .enumerate()
            .map(|(k, c)| TownConfig {
                origin: Vec2::new(-4500.0 + 750.0 * k as f32, 1000.0 * (k % 3) as f32 - 1000.0),
                ..c
            })
            .collect();
        let (towns, errors) = place_towns(&world, &configs);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(towns.len(), configs.len());
        for t in &towns {
            assert!(inside.contains(&t.leveled()), "{}", t.name);
            if let Some(rail) = t.rail_corridor() {
                assert!(inside.contains(&rail), "{}", t.name);
            }
        }
    }

    #[test]
    fn invalid_configs_are_reported_by_name() {
        let configs = [
            TownConfig {
                name: "Chica".into(),
                blocks: (2, 3),
                ..TownConfig::castex()
            },
            TownConfig::castex(),
        ];
        let (towns, errors) = place_towns(&WorldConfig::default(), &configs);
        assert_eq!(towns.len(), 1);
        let [error] = &errors[..] else {
            panic!("{errors:?}")
        };
        assert!(matches!(error, PlacementError::Invalid { index: 0, .. }));
        let message = error.to_string();
        assert!(
            message.contains("Chica") && message.contains("too few"),
            "{message}"
        );
    }
}
