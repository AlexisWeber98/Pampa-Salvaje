//! Chunks: las baldosas cuadradas en las que se carga por streaming el mundo
//! abierto.
//!
//! El chunk `(0, 0)` cubre `[0, size) x [0, size)` en el plano del suelo; el
//! chunk `(x, y)` está desplazado `x * size` hacia el este e `y * size` hacia
//! el norte. La lógica nunca necesita que existan los chunks:
//! [`crate::Terrain::height`] está definida en todas partes, así que el render
//! puede construir cualquier chunk cuando lo necesita y los vecinos siempre
//! coinciden en el borde.

use std::fmt;

use crate::{Aabb, Vec2, WorldBounds};

/// Coordenada entera de un chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl ChunkCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// El chunk de lado `size` que contiene el punto `p`.
    pub fn containing(p: Vec2, size: f32) -> Self {
        Self::new((p.x / size).floor() as i32, (p.y / size).floor() as i32)
    }

    /// Esquina sudoeste de este chunk.
    pub fn min(self, size: f32) -> Vec2 {
        Vec2::new(self.x as f32 * size, self.y as f32 * size)
    }

    /// Centro de este chunk.
    pub fn center(self, size: f32) -> Vec2 {
        self.min(size) + Vec2::new(size, size) * 0.5
    }

    /// El cuadrado que cubre este chunk.
    pub fn bounds(self, size: f32) -> Aabb {
        let min = self.min(size);
        Aabb::new(min, min + Vec2::new(size, size))
    }

    /// Distancia al cuadrado hasta `other`, en chunks.
    pub fn distance_squared(self, other: Self) -> i64 {
        let dx = i64::from(self.x) - i64::from(other.x);
        let dy = i64::from(self.y) - i64::from(other.y);
        dx * dx + dy * dy
    }

    /// Todos los chunks a `radius` chunks o menos de este (un disco, este
    /// incluido), del más cercano al más lejano; los empates se ordenan de sur
    /// a norte y después de oeste a este.
    pub fn within(self, radius: i32) -> Vec<Self> {
        if radius < 0 {
            return Vec::new();
        }
        let r2 = i64::from(radius) * i64::from(radius);
        let mut chunks: Vec<Self> = (-radius..=radius)
            .flat_map(|dy| (-radius..=radius).map(move |dx| (dx, dy)))
            .map(|(dx, dy)| Self::new(self.x + dx, self.y + dy))
            .filter(|c| c.distance_squared(self) <= r2)
            .collect();
        chunks.sort_by_key(|c| (c.distance_squared(self), c.y, c.x));
        chunks
    }

    /// Una semilla para lo que se distribuya sobre este chunk, mezclada a
    /// partir de `seed` y de la coordenada para que los vecinos sean distintos.
    pub fn seed(self, seed: u32) -> u32 {
        let mixed = seed
            ^ (self.x as u32).wrapping_mul(0x9e37_79b9)
            ^ (self.y as u32).wrapping_mul(0x85eb_ca6b).rotate_left(16);
        crate::rng::Rng::new(mixed, 0).next()
    }
}

/// Tamaño del mundo abierto y de los chunks en los que se carga por streaming.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldConfig {
    /// La mitad del lado del mundo cuadrado, centrado en el origen, en metros.
    pub half_extent: f32,
    /// Lado de un chunk cuadrado, en metros.
    pub chunk_size: f32,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            half_extent: Self::DEFAULT_HALF_EXTENT,
            chunk_size: Self::DEFAULT_CHUNK_SIZE,
        }
    }
}

/// Por qué no se puede usar una [`WorldConfig`].
#[derive(Debug, Clone, PartialEq)]
pub enum WorldConfigError {
    /// Un número es NaN o infinito.
    NotFinite { field: &'static str },
    /// El mundo está vacío o es más ancho de lo que las posiciones f32 permiten
    /// con precisión.
    HalfExtentOutOfRange { half_extent: f32, max: f32 },
    /// Chunks demasiado chicos para cargarlos por streaming de forma barata, o
    /// más grandes que el mundo.
    ChunkSizeOutOfRange { chunk_size: f32, min: f32, max: f32 },
}

impl fmt::Display for WorldConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite { field } => write!(f, "world `{field}` is not a finite number"),
            Self::HalfExtentOutOfRange { half_extent, max } => write!(
                f,
                "world half_extent {half_extent} m must be above 0 and at most {max} m"
            ),
            Self::ChunkSizeOutOfRange {
                chunk_size,
                min,
                max,
            } => write!(
                f,
                "world chunk_size {chunk_size} m must be between {min} m and {max} m"
            ),
        }
    }
}

impl std::error::Error for WorldConfigError {}

impl WorldConfig {
    /// 4 km desde el centro hasta cada borde: un mundo cuadrado de 8 km.
    pub const DEFAULT_HALF_EXTENT: f32 = 4000.0;
    /// Chunks de 200 m.
    pub const DEFAULT_CHUNK_SIZE: f32 = 200.0;
    /// Lo más lejos que puede llegar un mundo desde el origen: 8,4 km, lugar
    /// para la línea a escala 1:7 más un 5 %. Las posiciones son `f32`, que
    /// resuelven alrededor de un milímetro en cualquier punto por debajo de
    /// 16 km (2^14 m), así que 8,4 km las mantiene precisas a alrededor de un
    /// milímetro con margen de sobra.
    pub const MAX_HALF_EXTENT: f32 = 8400.0;
    /// El chunk más chico que vale la pena cargar por streaming por separado.
    pub const MIN_CHUNK_SIZE: f32 = 50.0;

    /// Esta configuración, después de comprobar cada uno de sus números.
    pub fn validated(self) -> Result<Self, WorldConfigError> {
        if !self.half_extent.is_finite() {
            return Err(WorldConfigError::NotFinite {
                field: "half_extent",
            });
        }
        if !self.chunk_size.is_finite() {
            return Err(WorldConfigError::NotFinite {
                field: "chunk_size",
            });
        }
        if !(self.half_extent > 0.0 && self.half_extent <= Self::MAX_HALF_EXTENT) {
            return Err(WorldConfigError::HalfExtentOutOfRange {
                half_extent: self.half_extent,
                max: Self::MAX_HALF_EXTENT,
            });
        }
        let max = 2.0 * self.half_extent;
        if !(Self::MIN_CHUNK_SIZE..=max).contains(&self.chunk_size) {
            return Err(WorldConfigError::ChunkSizeOutOfRange {
                chunk_size: self.chunk_size,
                min: Self::MIN_CHUNK_SIZE,
                max,
            });
        }
        Ok(self)
    }

    /// El cuadrado jugable.
    pub fn bounds(&self) -> WorldBounds {
        WorldBounds::centered(self.half_extent)
    }

    /// El chunk que contiene `p`.
    pub fn chunk_of(&self, p: Vec2) -> ChunkCoord {
        ChunkCoord::containing(p, self.chunk_size)
    }

    /// Si alguna parte del chunk `c` está dentro del mundo.
    pub fn has_chunk(&self, c: ChunkCoord) -> bool {
        let b = c.bounds(self.chunk_size);
        let world = self.bounds();
        b.min.x < world.max.x
            && b.max.x > world.min.x
            && b.min.y < world.max.y
            && b.max.y > world.min.y
    }

    /// Los chunks a `radius` chunks o menos del que contiene `p` que tocan el
    /// mundo, del más cercano al más lejano (ver [`ChunkCoord::within`]).
    pub fn chunks_near(&self, p: Vec2, radius: i32) -> Vec<ChunkCoord> {
        self.chunk_of(p)
            .within(radius)
            .into_iter()
            .filter(|&c| self.has_chunk(c))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_map_to_the_chunk_that_holds_them() {
        let size = 200.0;
        assert_eq!(
            ChunkCoord::containing(Vec2::new(0.0, 0.0), size),
            ChunkCoord::new(0, 0)
        );
        assert_eq!(
            ChunkCoord::containing(Vec2::new(199.9, 0.1), size),
            ChunkCoord::new(0, 0)
        );
        assert_eq!(
            ChunkCoord::containing(Vec2::new(200.0, -0.1), size),
            ChunkCoord::new(1, -1)
        );
        assert_eq!(
            ChunkCoord::containing(Vec2::new(-400.0, -401.0), size),
            ChunkCoord::new(-2, -3)
        );
    }

    #[test]
    fn chunks_tile_the_plane_and_round_trip() {
        let size = 200.0;
        for x in -5..5 {
            for y in -5..5 {
                let c = ChunkCoord::new(x, y);
                let b = c.bounds(size);
                assert_eq!(b.min, c.min(size));
                assert_eq!(b.size(), Vec2::new(size, size));
                assert_eq!(b.center(), c.center(size));
                assert_eq!(ChunkCoord::containing(c.center(size), size), c);
                assert_eq!(ChunkCoord::containing(b.min, size), c);
                // El vecino del este empieza donde termina este.
                assert_eq!(ChunkCoord::new(x + 1, y).min(size).x, b.max.x);
            }
        }
    }

    #[test]
    fn within_is_a_disc_nearest_first_and_deterministic() {
        let c = ChunkCoord::new(3, -2);
        assert_eq!(c.within(0), vec![c]);
        let ring = c.within(1);
        assert_eq!(ring.len(), 5);
        assert_eq!(ring[0], c);
        let disc = c.within(3);
        // Disco de 3^2: 29 puntos de la grilla.
        assert_eq!(disc.len(), 29);
        assert!(disc.iter().all(|o| o.distance_squared(c) <= 9));
        assert!(disc.windows(2).all(|w| {
            let (a, b) = (w[0].distance_squared(c), w[1].distance_squared(c));
            a < b || (a == b && (w[0].y, w[0].x) < (w[1].y, w[1].x))
        }));
        assert_eq!(disc, c.within(3));
        assert!(c.within(-1).is_empty());
    }

    #[test]
    fn chunk_seeds_differ_between_neighbors_and_are_stable() {
        let a = ChunkCoord::new(0, 0);
        let mut seen = std::collections::HashSet::new();
        for c in a.within(4) {
            assert_eq!(c.seed(9), c.seed(9));
            assert!(seen.insert(c.seed(9)), "{c:?} repeats a seed");
        }
        assert_ne!(a.seed(1), a.seed(2));
    }

    #[test]
    fn the_default_world_is_8_km_wide_in_200_m_chunks() {
        let w = WorldConfig::default();
        assert_eq!(w.validated(), Ok(w));
        assert_eq!(w.bounds(), WorldBounds::centered(4000.0));
        assert_eq!(w.bounds().size(), Vec2::new(8000.0, 8000.0));
        assert_eq!(w.chunk_size, 200.0);
        assert!(w.has_chunk(ChunkCoord::new(-20, -20)));
        assert!(w.has_chunk(ChunkCoord::new(19, 19)));
        assert!(!w.has_chunk(ChunkCoord::new(20, 0)));
        assert!(!w.has_chunk(ChunkCoord::new(0, -21)));
    }

    #[test]
    fn chunks_near_a_point_stay_inside_the_world() {
        let w = WorldConfig::default();
        let near = w.chunks_near(Vec2::new(10.0, 10.0), 2);
        assert_eq!(near, ChunkCoord::new(0, 0).within(2));
        // En la esquina noreste solo se conservan los chunks que están adentro.
        let corner = w.chunks_near(Vec2::new(3990.0, 3990.0), 2);
        assert!(!corner.is_empty());
        assert!(corner.iter().all(|&c| w.has_chunk(c)));
        assert!(corner.len() < near.len());
        assert_eq!(corner[0], ChunkCoord::new(19, 19));
    }

    #[test]
    fn bad_world_configs_are_rejected() {
        let ok = WorldConfig::default();
        let cases = [
            WorldConfig {
                half_extent: f32::NAN,
                ..ok
            },
            WorldConfig {
                chunk_size: f32::INFINITY,
                ..ok
            },
            WorldConfig {
                half_extent: 0.0,
                ..ok
            },
            WorldConfig {
                half_extent: 9000.0,
                ..ok
            },
            WorldConfig {
                chunk_size: 10.0,
                ..ok
            },
            WorldConfig {
                half_extent: 100.0,
                chunk_size: 400.0,
            },
        ];
        for config in cases {
            let error = config.validated().unwrap_err();
            assert!(!error.to_string().is_empty(), "{config:?}");
        }
        let error = WorldConfig {
            half_extent: 9000.0,
            ..ok
        }
        .validated()
        .unwrap_err();
        let max = WorldConfig::MAX_HALF_EXTENT.to_string();
        assert!(error.to_string().contains(&max), "{error}");
    }
}
