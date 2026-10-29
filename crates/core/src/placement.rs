//! Ubicación de muchos pueblos en un mismo mundo: cada uno se genera en su
//! propio `origin` y después se comprueba contra el borde del mundo y contra
//! los pueblos ubicados antes.

use std::fmt;

use crate::{Aabb, TownConfig, TownConfigError, TownLayout, WorldConfig};

/// Por qué un pueblo quedó fuera del mundo.
#[derive(Debug, Clone, PartialEq)]
pub enum PlacementError {
    /// No se puede construir la configuración propia del pueblo.
    Invalid {
        /// Posición del pueblo en la lista.
        index: usize,
        name: String,
        error: TownConfigError,
    },
    /// El pueblo, su franja nivelada o su vía férrea se salen del borde del
    /// mundo.
    OutsideWorld {
        index: usize,
        name: String,
        /// Lo más lejos que llega el pueblo desde el centro del mundo a lo
        /// largo de un eje.
        reach: f32,
        /// La mitad de la extensión del mundo.
        limit: f32,
    },
    /// El pueblo (con su franja nivelada y su vía férrea) se superpone con un
    /// pueblo ubicado antes.
    OverlapsTown {
        index: usize,
        name: String,
        /// El pueblo que ya estaba.
        other: String,
    },
}

impl PlacementError {
    /// Posición del pueblo rechazado en la lista.
    pub fn index(&self) -> usize {
        match self {
            Self::Invalid { index, .. }
            | Self::OutsideWorld { index, .. }
            | Self::OverlapsTown { index, .. } => *index,
        }
    }

    /// Nombre del pueblo rechazado.
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

/// Genera todos los pueblos de `configs` en `world`, en orden. Un pueblo queda
/// fuera (con un error que lo nombra) cuando su configuración es inválida,
/// cuando se sale del borde del mundo o cuando se superpone con un pueblo
/// ubicado antes. Los pueblos ubicados se devuelven en el orden de la lista.
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

/// Lo más lejos que llegan el terreno nivelado o la vía férrea del pueblo desde
/// el centro del mundo, sobre cualquiera de los dos ejes, en metros.
fn reach(town: &TownLayout) -> f32 {
    std::iter::once(town.leveled())
        .chain(town.rail_corridor())
        .flat_map(|b| [b.min.x, b.min.y, b.max.x, b.max.y])
        .map(f32::abs)
        .fold(0.0, f32::max)
}

/// Si dos pueblos se estorban entre sí: el terreno nivelado de uno toca el del
/// otro, o una vía férrea atraviesa el otro pueblo.
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
        // Un pueblo sin vía férrea sobre la línea de la vía de Castex, al este
        // de la cuadrícula pero dentro del tramo final de la vía.
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
        // Un mundo más chico rechaza incluso el pueblo del centro.
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
