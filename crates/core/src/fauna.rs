//! Los animales de la pampa de 1919: ganado que pasta dentro de los campos
//! alambrados, caballos sueltos en los corrales, liebres en el pasto abierto,
//! teros en el suelo, chimangos y caranchos planeando en lo alto, patos y
//! nutrias en las lagunas, y la fauna silvestre de suelo: zorros, ñandúes,
//! mulitas, peludos, vizcachas y, en el monte, gatos monteses, pumas y muy rara
//! vez un yaguareté.
//!
//! [`FaunaSite`] siembra dónde viven, de forma determinista a partir de la
//! semilla del terreno: un rodeo en algunos de los campos alambrados
//! ([`Herd`]), caballos en cada corral ([`TetheredHorse`]s parados adentro), y
//! liebres, bandadas de teros, aves rapaces y animales de suelo por chunk
//! ([`Hare`], [`Flock`], [`Raptor`], [`Critter`]), y grupos de patos y nutrias
//! por laguna ([`Duck`]). El juego hace aparecer lo que hay en los chunks
//! alrededor del jugador, del más cercano al más lejano, hasta [`caps`], y hace
//! avanzar a cada animal; el render los dibuja.
//!
//! Un rodeo vive en un *potrero*: un rectángulo dentro de una celda alambrada
//! del relevamiento, a [`FaunaSite::FENCE_MARGIN`] de su alambre, con cada uno
//! de sus puntos lejos de los caminos (y de sus alambrados al costado del
//! camino), los pueblos, las lagunas, la vía férrea y los asentamientos. Un
//! potrero es convexo, así que una vaca que deambula por cualquier parte de él
//! nunca cruza un camino ni un alambrado.

mod critter;
mod herd;
pub mod sparrow;
mod wild;

pub use critter::{Critter, CritterKind, CritterState, Land, Leap, Profile};
pub use herd::{Cow, CowCoat, CowState, Herd};
pub use sparrow::{Patch, Perch, Sparrow, SparrowClip, SparrowGroup, SparrowKind, Spot};
pub use wild::{Duck, Flock, FlockState, Hare, HareState, Raptor, RaptorKind};

use std::f32::consts::TAU;
use std::sync::Arc;

use crate::rng::Rng;
use crate::road::segment_distance;
use crate::settlement::KeepOut;
use crate::{
    Aabb, ChunkCoord, FenceLayout, FlatArea, Heading, Lagoon, RoadNetwork, SettlementKind,
    SettlementLayout, StructureKind, Terrain, TetheredHorse, Vec2, WorldConfig,
};

/// Máximo de animales de cada tipo que se simulan y dibujan a la vez cerca del
/// jugador.
pub mod caps {
    pub const COWS: usize = 160;
    pub const HORSES: usize = 16;
    pub const HARES: usize = 16;
    pub const TEROS: usize = 36;
    pub const RAPTORS: usize = 4;
    pub const DUCKS: usize = 32;
    /// Los zorros (modelos animados).
    pub const ZORROS: usize = 3;
    pub const NANDUS: usize = 10;
    /// Mulitas y peludos juntos.
    pub const ARMADILLOS: usize = 4;
    pub const VIZCACHAS: usize = 12;
    /// Gatos monteses, pumas y yaguaretés juntos (modelos animados).
    pub const FELINES: usize = 3;
    pub const NUTRIAS: usize = 6;
}

/// Dónde viven los animales (ver la documentación del módulo).
#[derive(Debug, Clone, PartialEq)]
pub struct FaunaSite {
    config: WorldConfig,
    flat_areas: Vec<FlatArea>,
    lagoons: Vec<Lagoon>,
    roads: Option<Arc<RoadNetwork>>,
    rail: Vec<(Vec2, Vec2)>,
    keep_out: Arc<KeepOut>,
    herds: Vec<Herd>,
    /// Según el centro del asentamiento: los caballos parados en sus corrales.
    horses: Vec<(Vec2, TetheredHorse)>,
    ducks: Vec<Duck>,
    nutrias: Vec<Duck>,
    seed: u32,
}

/// Se mezcla con la semilla del terreno para la fauna.
const FAUNA_SEED: u32 = 0xfa0a_1919;

impl FaunaSite {
    /// Proporción de los campos alambrados que tienen un rodeo.
    pub const HERD_CHANCE: f32 = 0.55;
    /// Mínimo y máximo de cabezas en un rodeo.
    pub const HERD_SIZE: (usize, usize) = (6, 20);
    /// Lado más chico y más grande de un potrero, en metros.
    pub const PASTURE: (f32, f32) = (70.0, 150.0);
    /// Distancia mínima desde un potrero hasta las líneas del relevamiento de
    /// su campo, en metros.
    pub const FENCE_MARGIN: f32 = 4.0;
    /// Separación de las muestras de despeje sobre un potrero, en metros.
    pub const SAMPLE: f32 = 6.0;
    /// Distancia mínima desde un potrero hasta la línea central de un camino:
    /// más allá de su alambrado al costado del camino, con el alcance de una
    /// muestra de margen.
    pub const ROAD_MARGIN: f32 = FenceLayout::ROADSIDE + 6.0;
    /// Distancia mínima desde el terreno de pastoreo hasta la vía férrea, en
    /// metros.
    pub const RAIL_MARGIN: f32 = 20.0;
    /// Distancia mínima desde el terreno de pastoreo más allá de la franja
    /// nivelada de un pueblo.
    pub const TOWN_MARGIN: f32 = 30.0;
    /// Probabilidad de que haya una liebre en un chunk, con dos intentos.
    pub const HARE_CHANCE: f32 = 0.35;
    /// Probabilidad de que haya una bandada de teros en un chunk.
    pub const FLOCK_CHANCE: f32 = 0.3;
    /// Probabilidad de que un chimango o un carancho vuele en círculos sobre un
    /// chunk.
    pub const RAPTOR_CHANCE: f32 = 0.12;
    /// Desde cuánto monte (`FloraSite::monte`) un lugar es monte para los
    /// felinos.
    pub const MONTE: f32 = 0.5;
    /// Por debajo de cuánto monte un lugar es campo abierto para los ñandúes y
    /// las vizcachas.
    pub const OPEN: f32 = 0.2;

    /// La fauna del mundo de `config`: `terrain` (nivelado, con sus caminos y
    /// lagunas), sus `fences` y `settlements`, y el recorrido completo de la
    /// vía férrea como puntos `rail`.
    pub fn new(
        config: WorldConfig,
        terrain: &Terrain,
        fences: &FenceLayout,
        settlements: &SettlementLayout,
        rail: &[Vec2],
    ) -> Self {
        let mut site = Self {
            config,
            flat_areas: terrain.flat_areas.clone(),
            lagoons: terrain.lagoons.clone(),
            roads: terrain.roads.clone(),
            rail: rail.windows(2).map(|w| (w[0], w[1])).collect(),
            keep_out: settlements.keep_out.clone(),
            herds: Vec::new(),
            horses: Vec::new(),
            ducks: Vec::new(),
            nutrias: Vec::new(),
            seed: terrain.seed ^ FAUNA_SEED,
        };
        site.herds = site.seed_herds(fences);
        site.horses = settlements
            .settlements
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                corral_horses(s, site.seed ^ i as u32)
                    .into_iter()
                    .map(|h| (s.hub, h))
            })
            .collect();
        site.ducks = site.seed_ducks();
        site.nutrias = site.seed_nutrias();
        site
    }

    /// El mundo para el que se sembró.
    pub fn config(&self) -> WorldConfig {
        self.config
    }

    /// Todos los rodeos del mundo.
    pub fn herds(&self) -> &[Herd] {
        &self.herds
    }

    /// Los rodeos cuyo potrero tiene el centro en el chunk `coord`.
    pub fn herds_in(&self, coord: ChunkCoord) -> Vec<Herd> {
        let size = self.config.chunk_size;
        self.herds
            .iter()
            .filter(|h| ChunkCoord::containing(h.pasture.center(), size) == coord)
            .cloned()
            .collect()
    }

    /// Los caballos parados en los corrales de los asentamientos cuyo centro
    /// está en el chunk `coord`.
    pub fn horses_in(&self, coord: ChunkCoord) -> Vec<TetheredHorse> {
        let size = self.config.chunk_size;
        self.horses
            .iter()
            .filter(|(hub, _)| ChunkCoord::containing(*hub, size) == coord)
            .map(|&(_, h)| h)
            .collect()
    }

    /// Todos los caballos de corral del mundo.
    pub fn all_horses(&self) -> impl Iterator<Item = &TetheredHorse> + '_ {
        self.horses.iter().map(|(_, h)| h)
    }

    /// Los patos cuyo grupo tiene su lugar de origen en el chunk `coord`.
    pub fn ducks_in(&self, coord: ChunkCoord) -> Vec<Duck> {
        let size = self.config.chunk_size;
        self.ducks
            .iter()
            .filter(|d| ChunkCoord::containing(d.home, size) == coord)
            .cloned()
            .collect()
    }

    /// Las liebres del chunk `coord`, echadas en el pasto abierto.
    pub fn hares_in(&self, coord: ChunkCoord) -> Vec<Hare> {
        let mut rng = self.chunk_rng(coord, 1);
        (0..2)
            .filter_map(|k| {
                if rng.unit() >= Self::HARE_CHANCE {
                    return None;
                }
                let at = self.open_spot(coord, &mut rng)?;
                Some(Hare::new(at, self.home(coord), rng.next() ^ k))
            })
            .collect()
    }

    /// La bandada de teros del chunk `coord`, si tiene una.
    pub fn flocks_in(&self, coord: ChunkCoord) -> Vec<Flock> {
        let mut rng = self.chunk_rng(coord, 2);
        if rng.unit() >= Self::FLOCK_CHANCE {
            return Vec::new();
        }
        let count = 3 + rng.below(4);
        self.open_spot(coord, &mut rng)
            .map(|at| Flock::new(at, self.home(coord), count, rng.next()))
            .into_iter()
            .collect()
    }

    /// El ave rapaz que vuela en círculos sobre el chunk `coord`, si hay
    /// alguna.
    pub fn raptors_in(&self, coord: ChunkCoord) -> Vec<Raptor> {
        let mut rng = self.chunk_rng(coord, 3);
        if !self.config.has_chunk(coord) || rng.unit() >= Self::RAPTOR_CHANCE {
            return Vec::new();
        }
        let size = self.config.chunk_size;
        let center =
            coord.center(size) + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3)) * size;
        vec![Raptor::new(center, rng.next())]
    }

    /// Las nutrias cuyo lugar de origen está en el chunk `coord`: nadan en las
    /// lagunas igual que los patos (la misma simulación, [`Duck`]).
    pub fn nutrias_in(&self, coord: ChunkCoord) -> Vec<Duck> {
        let size = self.config.chunk_size;
        self.nutrias
            .iter()
            .filter(|d| ChunkCoord::containing(d.home, size) == coord)
            .cloned()
            .collect()
    }

    /// Los animales silvestres de suelo del chunk `coord` ([`CritterKind`]),
    /// cada especie con su propia semilla, sobre pasto abierto
    /// ([`Self::grazeable`]) y en su hábitat según `monte`, qué parte de un
    /// monte hay en un punto (`0..=1`, `FloraSite::monte`): los felinos solo
    /// en el monte, los ñandúes y las vizcachas solo fuera de él, el resto en
    /// cualquier parte. Los ñandúes andan en grupos y las vizcachas en
    /// colonias alrededor de su vizcachera.
    pub fn critters_in(&self, coord: ChunkCoord, monte: impl Fn(Vec2) -> f32) -> Vec<Critter> {
        let mut out = Vec::new();
        if !self.config.has_chunk(coord) {
            return out;
        }
        let home = self.home(coord);
        for kind in CritterKind::ALL {
            let mut rng = self.chunk_rng(coord, critter_seed_kind(kind));
            if rng.unit() >= critter_chance(kind) {
                continue;
            }
            let fits = |p: Vec2| {
                let m = monte(p);
                match kind {
                    k if k.feline() => m >= Self::MONTE,
                    CritterKind::Nandu | CritterKind::Vizcacha => m < Self::OPEN,
                    _ => true,
                }
            };
            let spot = (0..8)
                .map(|_| {
                    Vec2::new(
                        rng.range(home.min.x, home.max.x),
                        rng.range(home.min.y, home.max.y),
                    )
                })
                .find(|&p| self.grazeable(p) && fits(p));
            let Some(at) = spot else { continue };
            let (count, den) = match kind {
                CritterKind::Nandu => (3 + rng.below(4), None),
                CritterKind::Vizcacha => (3 + rng.below(4), Some(at)),
                _ => (1, None),
            };
            for k in 0..count {
                let p = if k == 0 {
                    at
                } else {
                    at + Vec2::new(rng.range(-6.0, 6.0), rng.range(-6.0, 6.0))
                };
                let seed = rng.next();
                if home.contains_point(p) && self.grazeable(p) && fits(p) {
                    // Su territorio se centra donde se sembró el grupo.
                    let roam = 2.0 * kind.profile().roam;
                    let range = Aabb::from_center_size(at, Vec2::new(roam, roam));
                    out.push(Critter::new(kind, p, range, den, seed));
                }
            }
        }
        out
    }

    /// El terreno del chunk `coord`: el hogar del que no se alejan sus liebres
    /// y sus teros (los animales de suelo tienen su propio territorio,
    /// [`Profile::roam`]).
    fn home(&self, coord: ChunkCoord) -> Aabb {
        coord.bounds(self.config.chunk_size)
    }

    fn chunk_rng(&self, coord: ChunkCoord, kind: u32) -> Rng {
        Rng::new(
            self.seed ^ kind.wrapping_mul(0x51ed_270b),
            coord.seed(self.seed),
        )
    }

    /// Un lugar despejado de pasto abierto en el chunk `coord`: lejos de
    /// caminos, pueblos, lagunas, la vía férrea y los asentamientos. Hace unos
    /// pocos intentos.
    fn open_spot(&self, coord: ChunkCoord, rng: &mut Rng) -> Option<Vec2> {
        if !self.config.has_chunk(coord) {
            return None;
        }
        let area = coord.bounds(self.config.chunk_size);
        (0..4)
            .map(|_| {
                Vec2::new(
                    rng.range(area.min.x, area.max.x),
                    rng.range(area.min.y, area.max.y),
                )
            })
            .find(|&p| self.grazeable(p))
    }

    /// Si los animales pueden pastar en `p`: dentro del mundo, lejos de todo
    /// camino (más allá de su alambrado al costado del camino), pueblo, orilla
    /// de laguna, la vía férrea y todo asentamiento.
    pub fn grazeable(&self, p: Vec2) -> bool {
        !self.near_road(p) && self.wild(p)
    }

    /// Si `p` está en la franja de un camino ([`Self::ROAD_MARGIN`]: el
    /// camino, sus banquinas y sus alambrados) y lejos de todo lo demás que
    /// [`Self::grazeable`] deja afuera: por ahí cruzan huyendo los animales
    /// que saltan alambrados ([`Land::road`]).
    pub fn road_strip(&self, p: Vec2) -> bool {
        self.near_road(p) && self.wild(p)
    }

    fn near_road(&self, p: Vec2) -> bool {
        // El margen queda bien dentro del alcance del índice de caminos.
        self.roads
            .as_ref()
            .is_some_and(|r| r.near(p, Self::ROAD_MARGIN).is_some())
    }

    /// Si `p` está dentro del mundo y lejos de pueblos, lagunas, la vía
    /// férrea y asentamientos (todo lo de [`Self::grazeable`] salvo los
    /// caminos).
    fn wild(&self, p: Vec2) -> bool {
        let inner = self.config.bounds();
        if p.x.abs() > inner.max.x - 20.0 || p.y.abs() > inner.max.y - 20.0 {
            return false;
        }
        self.flat_areas.iter().all(|a| {
            let dx = ((p.x - a.center.x).abs() - a.half_extents.x).max(0.0);
            let dy = ((p.y - a.center.y).abs() - a.half_extents.y).max(0.0);
            dx.hypot(dy) >= a.falloff + Self::TOWN_MARGIN
        }) && self
            .lagoons
            .iter()
            .all(|l| l.offshore(p) >= Lagoon::BANK + Lagoon::MARGIN)
            && self
                .rail
                .iter()
                .all(|&(a, b)| segment_distance(p, a, b) >= Self::RAIL_MARGIN)
            && self.keep_out.is_clear(p, KeepOut::MAX_MARGIN)
    }

    /// Si todas las muestras sobre `pasture` sirven para pastar.
    fn clear_pasture(&self, pasture: Aabb) -> bool {
        let size = pasture.size();
        let (nx, ny) = (
            (size.x / Self::SAMPLE).ceil() as usize,
            (size.y / Self::SAMPLE).ceil() as usize,
        );
        (0..=ny).all(|j| {
            (0..=nx).all(|i| {
                let t = Vec2::new(i as f32 / nx as f32, j as f32 / ny as f32);
                self.grazeable(pasture.min + Vec2::new(size.x * t.x, size.y * t.y))
            })
        })
    }

    /// Un rodeo para algunos campos alambrados, en un potrero despejado propio.
    fn seed_herds(&self, fences: &FenceLayout) -> Vec<Herd> {
        let (xs, ys) = &fences.surveyed;
        let mut herds = Vec::new();
        for i in 0..xs.len().saturating_sub(1) {
            for j in 0..ys.len().saturating_sub(1) {
                let id = (i * ys.len() + j) as u32;
                let mut rng = Rng::new(self.seed ^ 0x4e7d, id);
                if rng.unit() >= Self::HERD_CHANCE || fences.descampados.contains(&(i, j)) {
                    continue;
                }
                let field = Aabb::new(Vec2::new(xs[i], ys[j]), Vec2::new(xs[i + 1], ys[j + 1]))
                    .expanded(-Self::FENCE_MARGIN);
                if let Some(pasture) = self.pasture(field, &mut rng) {
                    let (lo, hi) = Self::HERD_SIZE;
                    let count = lo + rng.below(hi - lo + 1);
                    herds.push(Herd::new(id, pasture, count, rng.next()));
                }
            }
        }
        herds
    }

    /// Un potrero despejado dentro de `field`, con unos pocos intentos.
    fn pasture(&self, field: Aabb, rng: &mut Rng) -> Option<Aabb> {
        let room = field.size();
        (0..6).find_map(|_| {
            let (lo, hi) = Self::PASTURE;
            let size = Vec2::new(rng.range(lo, hi).min(room.x), rng.range(lo, hi).min(room.y));
            if size.x < lo * 0.5 || size.y < lo * 0.5 {
                return None;
            }
            let min = Vec2::new(
                rng.range(field.min.x, field.max.x - size.x),
                rng.range(field.min.y, field.max.y - size.y),
            );
            let pasture = Aabb::new(min, min + size);
            self.clear_pasture(pasture).then_some(pasture)
        })
    }

    /// Unos pocos grupos de patos en cada laguna, en aguas abiertas.
    fn seed_ducks(&self) -> Vec<Duck> {
        let mut ducks = Vec::new();
        for (l, lagoon) in self.lagoons.iter().enumerate() {
            let mut rng = Rng::new(self.seed ^ 0xd0c5, l as u32);
            let rafts = ((lagoon.radii.x * lagoon.radii.y) / 15_000.0).clamp(2.0, 6.0) as usize;
            for _ in 0..rafts {
                let angle = rng.unit() * TAU;
                let r = lagoon.shore_radius(angle) * rng.range(0.2, 0.7);
                let home = lagoon.center + Vec2::new(angle.cos(), angle.sin()) * r;
                if !lagoon.is_deep(home) {
                    continue;
                }
                for _ in 0..3 + rng.below(4) {
                    let at = home + Vec2::new(rng.range(-6.0, 6.0), rng.range(-6.0, 6.0));
                    if lagoon.is_deep(at) {
                        ducks.push(Duck::new(home, at, l, rng.next()));
                    }
                }
            }
        }
        ducks
    }

    /// Una o dos nutrias por laguna (tres en las grandes), cerca de la orilla
    /// pero en agua profunda.
    fn seed_nutrias(&self) -> Vec<Duck> {
        let mut nutrias = Vec::new();
        for (l, lagoon) in self.lagoons.iter().enumerate() {
            let mut rng = Rng::new(self.seed ^ 0x0b17, l as u32);
            let count = ((lagoon.radii.x * lagoon.radii.y) / 20_000.0).clamp(1.0, 3.0) as usize;
            for _ in 0..count + rng.below(2) {
                let angle = rng.unit() * TAU;
                let r = lagoon.shore_radius(angle) * rng.range(0.55, 0.8);
                let at = lagoon.center + Vec2::new(angle.cos(), angle.sin()) * r;
                if lagoon.is_deep(at) {
                    nutrias.push(Duck::new(at, at, l, rng.next()));
                }
            }
        }
        nutrias
    }

    /// Las lagunas en las que nadan los patos, por índice.
    pub fn lagoons(&self) -> &[Lagoon] {
        &self.lagoons
    }
}

/// El tipo con el que [`FaunaSite::critters_in`] siembra a cada especie: el
/// suyo propio, distinto de los de las liebres (1), los teros (2) y las rapaces
/// (3), así que agregar una especie no mueve a ninguna otra.
fn critter_seed_kind(kind: CritterKind) -> u32 {
    4 + CritterKind::ALL
        .iter()
        .position(|&k| k == kind)
        .unwrap_or(0) as u32
}

/// Probabilidad de que una especie (o un grupo, o una colonia) viva en un
/// chunk, antes de buscarle un lugar en su hábitat: los felinos solo
/// aparecen donde el chunk tiene monte, así que en la práctica son más raros.
/// El yaguareté es un avistamiento excepcional.
fn critter_chance(kind: CritterKind) -> f32 {
    match kind {
        CritterKind::Zorro => 0.1,
        CritterKind::Nandu => 0.05,
        CritterKind::Mulita => 0.07,
        CritterKind::Peludo => 0.07,
        CritterKind::Vizcacha => 0.06,
        CritterKind::GatoMontes => 0.25,
        CritterKind::Puma => 0.08,
        CritterKind::Yaguarete => 0.01,
    }
}

/// Los caballos sueltos en los corrales de `settlement`: 1–3 en los de una
/// estancia, 1–2 en los de una chacra y a veces uno en los de un puesto.
fn corral_horses(settlement: &crate::Settlement, seed: u32) -> Vec<TetheredHorse> {
    let mut rng = Rng::new(seed, 0x40e5);
    let mut horses: Vec<TetheredHorse> = Vec::new();
    for corral in settlement.of(StructureKind::Corral) {
        let count = match settlement.kind {
            SettlementKind::Ranch => 1 + rng.below(3),
            SettlementKind::Farm => 1 + rng.below(2),
            SettlementKind::Outpost => usize::from(rng.unit() < 0.6),
            SettlementKind::Hut => 0,
        };
        // Lejos de los rieles, con medio largo de caballo de margen.
        let inner = corral.footprint.expanded(-1.4);
        for _ in 0..count {
            let placed = (0..8).find_map(|_| {
                let at = Vec2::new(
                    rng.range(inner.min.x, inner.max.x),
                    rng.range(inner.min.y, inner.max.y),
                );
                let horse = TetheredHorse::standing(at, Heading::from_radians(rng.unit() * TAU));
                let fits = inner.contains(&horse.footprint)
                    && horses
                        .iter()
                        .all(|h| !h.footprint.expanded(0.4).intersects(&horse.footprint));
                fits.then_some(horse)
            });
            horses.extend(placed);
        }
    }
    horses
}

/// El giro más corto de `from` a `to`, en radianes, en `(-π, π]`.
pub(crate) fn turn_toward(from: Heading, to: Vec2) -> f32 {
    let want = to.y.atan2(to.x);
    let d = (want - from.radians()).rem_euclid(TAU);
    if d > std::f32::consts::PI { d - TAU } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settlement::tests::settled;
    use crate::{LagoonConfig, StaticObstacles};

    /// La fauna del mundo compartido [`settled`], sembrada una sola vez.
    fn site() -> &'static FaunaSite {
        static SITE: std::sync::LazyLock<FaunaSite> = std::sync::LazyLock::new(|| {
            let (world, towns, line, fences, layout) = settled();
            let site = FaunaSite::new(
                world.config,
                &world.terrain,
                fences,
                layout,
                line.full_path(towns).points(),
            );
            println!(
                "fauna: {} herds ({} head), {} corral horses, {} ducks",
                site.herds.len(),
                site.herds.iter().map(|h| h.cows.len()).sum::<usize>(),
                site.horses.len(),
                site.ducks.len(),
            );
            site
        });
        &SITE
    }

    #[test]
    fn herds_are_seeded_the_same_way_every_time_and_sparsely() {
        let site = site();
        let (world, towns, line, fences, layout) = settled();
        let again = FaunaSite::new(
            world.config,
            &world.terrain,
            fences,
            layout,
            line.full_path(towns).points(),
        );
        assert_eq!(*site, again);
        let share = site.herds.len() as f32 / fences.cells() as f32;
        assert!((0.05..=FaunaSite::HERD_CHANCE).contains(&share), "{share}");
        for herd in &site.herds {
            let n = herd.cows.len();
            assert!((FaunaSite::HERD_SIZE.0..=FaunaSite::HERD_SIZE.1).contains(&n));
        }
        let coats: std::collections::HashSet<_> = site
            .herds
            .iter()
            .flat_map(|h| h.cows.iter().map(|c| c.coat))
            .collect();
        assert_eq!(
            coats.len(),
            CowCoat::ALL.len(),
            "every coat grazes somewhere"
        );
    }

    #[test]
    fn herds_graze_only_inside_fenced_fields_off_roads_towns_and_settlements() {
        let site = site();
        let (world, _, _, fences, layout) = settled();
        let roads = world.terrain.roads.clone().unwrap();
        assert!(!site.herds.is_empty());
        for herd in &site.herds {
            let field = fences.field_of(herd.pasture.center()).unwrap();
            assert!(field.contains(&herd.pasture), "{herd:?}");
            assert!(!fences.is_descampado(herd.pasture.center()));
            let p = herd.pasture;
            let mut probes = vec![p.min, p.max, Vec2::new(p.min.x, p.max.y), p.center()];
            probes.extend(herd.cows.iter().map(|c| c.position));
            for q in probes {
                assert!(
                    roads.distance(q) > FenceLayout::ROADSIDE + 1.0,
                    "{q:?} on a road"
                );
                assert!(fences.line_distance(q) >= FaunaSite::FENCE_MARGIN - 1e-3);
                assert!(world.terrain.flat_areas.iter().all(|a| a.weight(q) == 0.0));
                assert!(layout.is_clear(q, 1.0), "{q:?} in a settlement");
            }
        }
    }

    #[test]
    fn a_lagoon_over_a_field_moves_its_herd_off() {
        let site = site();
        let (world, towns, line, fences, layout) = settled();
        let mut world = world.clone();
        let herd = &site.herds[site.herds.len() / 2];
        let lagoon = Lagoon::new(
            &LagoonConfig {
                name: "Prueba".into(),
                center: herd.pasture.center(),
                size: Vec2::new(200.0, 200.0),
                seed: 1,
            },
            &world.terrain,
        );
        world.terrain = std::mem::take(&mut world.terrain).with_lagoon(lagoon.clone());
        let wet = FaunaSite::new(
            world.config,
            &world.terrain,
            fences,
            layout,
            line.full_path(towns).points(),
        );
        // Su potrero se alejó del agua (o el rodeo ya no está), y ninguna vaca
        // pasta en la orilla.
        let moved = wet.herds.iter().find(|h| h.id == herd.id);
        assert!(moved.is_none_or(|h| h.pasture != herd.pasture));
        for h in &wet.herds {
            let p = h.pasture;
            for q in [
                p.min,
                p.max,
                Vec2::new(p.min.x, p.max.y),
                Vec2::new(p.max.x, p.min.y),
            ] {
                assert!(lagoon.offshore(q) > Lagoon::BANK, "{h:?}");
            }
        }
        assert!(!wet.ducks.is_empty(), "ducks on the new lagoon");
        assert!(wet.ducks.iter().all(|d| lagoon.is_water(d.position)));
        // Y nutrias, en agua profunda, cada una cargada con un solo chunk.
        assert!(!wet.nutrias.is_empty(), "nutrias on the new lagoon");
        assert!(wet.nutrias.iter().all(|n| lagoon.is_deep(n.position)));
        let size = wet.config.chunk_size;
        let chunks: std::collections::BTreeSet<_> = wet
            .nutrias
            .iter()
            .map(|n| ChunkCoord::containing(n.home, size))
            .collect();
        let streamed: usize = chunks.iter().map(|&c| wet.nutrias_in(c).len()).sum();
        assert_eq!(streamed, wet.nutrias.len());
    }

    #[test]
    fn corral_horses_stand_inside_the_corrals_of_farms_and_ranches() {
        let site = site();
        let (.., layout) = settled();
        assert!(site.horses.len() >= 2, "{}", site.horses.len());
        for (hub, horse) in &site.horses {
            let s = layout.settlements.iter().find(|s| s.hub == *hub).unwrap();
            assert_ne!(s.kind, SettlementKind::Hut);
            assert!(
                s.of(StructureKind::Corral)
                    .any(|c| c.footprint.contains(&horse.footprint))
            );
        }
        let total: usize = site
            .horses
            .iter()
            .map(|(hub, _)| ChunkCoord::containing(*hub, site.config.chunk_size))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|c| site.horses_in(c).len())
            .sum();
        assert_eq!(
            total,
            site.horses.len(),
            "every horse streams with one chunk"
        );
    }

    #[test]
    fn wild_animals_are_seeded_per_chunk_on_open_grass() {
        let site = site();
        let mut hares = 0;
        let mut flocks = 0;
        let mut raptors = 0;
        for coord in ChunkCoord::new(0, 0).within(3) {
            let h = site.hares_in(coord);
            assert_eq!(h, site.hares_in(coord), "deterministic");
            let home = coord.bounds(site.config.chunk_size);
            for hare in &h {
                assert!(site.grazeable(hare.position));
                assert_eq!(hare.home, home);
                // Después de que la persiguen un rato, se queda en el pasto
                // abierto de su hogar.
                let mut hare = hare.clone();
                for k in 0..200 {
                    let rider = hare.position + Vec2::new((k as f32 * 0.1).cos(), 0.6) * 10.0;
                    hare.step(0.1, Some(rider), |p| site.grazeable(p));
                    assert!(site.grazeable(hare.position) && home.contains_point(hare.position));
                }
            }
            hares += h.len();
            for flock in site.flocks_in(coord) {
                assert!(site.grazeable(flock.center));
                assert!((3..=6).contains(&flock.birds().count()));
                let mut flock = flock.clone();
                for k in 0..300 {
                    let rider = flock.center + Vec2::new(5.0, (k % 7) as f32);
                    flock.step(0.1, Some(rider), |p| site.grazeable(p));
                    if !flock.flying() {
                        assert!(site.grazeable(flock.center));
                        assert!(home.contains_point(flock.center));
                    }
                }
                flocks += 1;
            }
            raptors += site.raptors_in(coord).len();
        }
        assert!(
            hares > 0 && flocks > 0 && raptors > 0,
            "{hares} {flocks} {raptors}"
        );
    }

    #[test]
    fn roads_through_open_country_are_a_road_strip_not_pasture() {
        let site = site();
        let roads = site.roads.as_ref().expect("roads");
        let mut strip = 0;
        for (a, b) in roads.segments() {
            let mid = (a + b) * 0.5;
            assert!(!site.grazeable(mid));
            strip += usize::from(site.road_strip(mid));
        }
        assert!(strip > 0, "some road runs through the open pampa");
    }

    #[test]
    fn wild_ground_animals_keep_to_their_habitat_and_the_yaguarete_is_rarest() {
        let site = site();
        let (world, _, line, ..) = settled();
        let flora = crate::FloraSite::new(world.config, &world.terrain, line);
        let config = site.config;
        let reach = (config.half_extent / config.chunk_size).ceil() as i32;
        let mut seen = std::collections::HashMap::<CritterKind, usize>::new();
        for coord in config.chunks_near(Vec2::ZERO, reach) {
            let critters = site.critters_in(coord, |p| flora.monte(p));
            assert_eq!(
                critters,
                site.critters_in(coord, |p| flora.monte(p)),
                "deterministic"
            );
            let home = coord.bounds(config.chunk_size);
            for c in &critters {
                assert!(site.grazeable(c.position) && home.contains_point(c.position));
                // Su territorio es de su especie, no el chunk.
                let roam = c.kind.profile().roam;
                let size = c.home.max - c.home.min;
                assert!(
                    (size - Vec2::new(2.0 * roam, 2.0 * roam)).length() < 1e-2,
                    "{c:?}"
                );
                assert!(c.home.contains_point(c.position));
                let monte = flora.monte(c.position);
                match c.kind {
                    k if k.feline() => assert!(monte >= FaunaSite::MONTE, "{c:?}"),
                    CritterKind::Nandu | CritterKind::Vizcacha => {
                        assert!(monte < FaunaSite::OPEN, "{c:?}")
                    }
                    _ => {}
                }
                assert_eq!(c.den.is_some(), c.kind == CritterKind::Vizcacha);
                *seen.entry(c.kind).or_default() += 1;
            }
            // Sin monte no hay felinos.
            assert!(
                site.critters_in(coord, |_| 0.0)
                    .iter()
                    .all(|c| !c.kind.feline())
            );
        }
        println!("wild ground animals: {seen:?}");
        let n = |k| seen.get(&k).copied().unwrap_or(0);
        for kind in CritterKind::ALL {
            if kind != CritterKind::Yaguarete {
                assert!(n(kind) > 0, "{kind:?} lives somewhere");
            }
        }
        assert!(n(CritterKind::Yaguarete) * 3 <= n(CritterKind::Puma).max(1));
        assert!(n(CritterKind::Puma) < n(CritterKind::GatoMontes));
    }

    #[test]
    fn raptors_are_chimangos_and_caranchos() {
        let site = site();
        let mut kinds = std::collections::HashSet::new();
        for coord in ChunkCoord::new(0, 0).within(12) {
            kinds.extend(site.raptors_in(coord).iter().map(|r| r.kind));
        }
        assert_eq!(kinds.len(), 2, "{kinds:?}");
    }

    #[test]
    fn herds_stream_with_one_chunk_each() {
        let site = site();
        let size = site.config.chunk_size;
        let chunks: std::collections::BTreeSet<_> = site
            .herds
            .iter()
            .map(|h| ChunkCoord::containing(h.pasture.center(), size))
            .collect();
        let streamed: usize = chunks.iter().map(|&c| site.herds_in(c).len()).sum();
        assert_eq!(streamed, site.herds.len());
        // Un rodeo que avanza durante minutos con un jinete cerca se queda en
        // su potrero.
        let mut herd = site.herds[0].clone();
        let none = StaticObstacles::default();
        for k in 0..3000 {
            let rider = herd.pasture.center() + Vec2::new((k as f32 * 0.01).sin() * 60.0, 0.0);
            herd.step(0.1, Some(rider), &none);
        }
        assert!(
            herd.cows
                .iter()
                .all(|c| herd.pasture.contains_point(c.position))
        );
    }
}
