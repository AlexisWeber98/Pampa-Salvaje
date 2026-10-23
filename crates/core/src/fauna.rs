//! The pampa's animals (first pass): cattle grazing inside the fenced
//! fields, loose horses in the corrals, hares in the open grass, teros on
//! the ground, chimangos soaring overhead and ducks on the lagoons.
//!
//! [`FaunaSite`] seeds where they live, deterministically from the terrain
//! seed: a herd per some fenced fields ([`Herd`]), horses per corral
//! ([`TetheredHorse`]s standing in it), and hares, tero flocks and raptors
//! per chunk ([`Hare`], [`Flock`], [`Raptor`]), duck rafts per lagoon
//! ([`Duck`]). The game spawns what lies in the chunks round the player,
//! nearest first, up to [`caps`], and steps each animal; render draws them.
//!
//! A herd lives in a *pasture*: a box inside one fenced survey cell,
//! [`FaunaSite::FENCE_MARGIN`] off its wire, every point of it clear of
//! the roads (and their roadside fences), the towns, the lagoons, the
//! railway and the settlements. A pasture is convex, so a cow wandering
//! anywhere inside it never crosses a road or a fence.

mod herd;
mod wild;

pub use herd::{Cow, CowCoat, CowState, Herd};
pub use wild::{Duck, Flock, FlockState, Hare, HareState, Raptor};

use std::f32::consts::TAU;
use std::sync::Arc;

use crate::rng::Rng;
use crate::road::segment_distance;
use crate::settlement::KeepOut;
use crate::{
    Aabb, ChunkCoord, FenceLayout, FlatArea, Heading, Lagoon, RoadNetwork, SettlementKind,
    SettlementLayout, StructureKind, Terrain, TetheredHorse, Vec2, WorldConfig,
};

/// Most animals of each kind simulated and drawn near the player at once.
pub mod caps {
    pub const COWS: usize = 160;
    pub const HORSES: usize = 16;
    pub const HARES: usize = 16;
    pub const TEROS: usize = 36;
    pub const RAPTORS: usize = 4;
    pub const DUCKS: usize = 32;
}

/// Where the animals live (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct FaunaSite {
    config: WorldConfig,
    flat_areas: Vec<FlatArea>,
    lagoons: Vec<Lagoon>,
    roads: Option<Arc<RoadNetwork>>,
    rail: Vec<(Vec2, Vec2)>,
    keep_out: Arc<KeepOut>,
    herds: Vec<Herd>,
    /// By the settlement's hub: the horses standing in its corrals.
    horses: Vec<(Vec2, TetheredHorse)>,
    ducks: Vec<Duck>,
    seed: u32,
}

/// Mixed into the terrain seed for the fauna.
const FAUNA_SEED: u32 = 0xfa0a_1919;

impl FaunaSite {
    /// Share of the fenced fields that hold a herd.
    pub const HERD_CHANCE: f32 = 0.55;
    /// Fewest and most head in a herd.
    pub const HERD_SIZE: (usize, usize) = (6, 20);
    /// Smallest and largest side of a pasture, in meters.
    pub const PASTURE: (f32, f32) = (70.0, 150.0);
    /// Least distance from a pasture to its field's survey lines, meters.
    pub const FENCE_MARGIN: f32 = 4.0;
    /// Spacing of the clearance samples over a pasture, in meters.
    pub const SAMPLE: f32 = 6.0;
    /// Least distance from a pasture to a road's centerline: past its
    /// roadside fence, with a sample's reach to spare.
    pub const ROAD_MARGIN: f32 = FenceLayout::ROADSIDE + 6.0;
    /// Least distance from grazing ground to the railway, in meters.
    pub const RAIL_MARGIN: f32 = 20.0;
    /// Least distance from grazing ground past a town's leveled band.
    pub const TOWN_MARGIN: f32 = 30.0;
    /// Chance of a hare in a chunk, tried twice.
    pub const HARE_CHANCE: f32 = 0.35;
    /// Chance of a tero flock in a chunk.
    pub const FLOCK_CHANCE: f32 = 0.3;
    /// Chance of a chimango or carancho circling over a chunk.
    pub const RAPTOR_CHANCE: f32 = 0.12;

    /// The fauna of `config`'s world: `terrain` (leveled, with its roads and
    /// lagoons), its `fences` and `settlements`, and the railway's whole
    /// path as `rail` points.
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
        site
    }

    /// The world it was seeded for.
    pub fn config(&self) -> WorldConfig {
        self.config
    }

    /// Every herd of the world.
    pub fn herds(&self) -> &[Herd] {
        &self.herds
    }

    /// The herds whose pasture's middle lies in chunk `coord`.
    pub fn herds_in(&self, coord: ChunkCoord) -> Vec<Herd> {
        let size = self.config.chunk_size;
        self.herds
            .iter()
            .filter(|h| ChunkCoord::containing(h.pasture.center(), size) == coord)
            .cloned()
            .collect()
    }

    /// The horses standing in the corrals of the settlements whose hub
    /// lies in chunk `coord`.
    pub fn horses_in(&self, coord: ChunkCoord) -> Vec<TetheredHorse> {
        let size = self.config.chunk_size;
        self.horses
            .iter()
            .filter(|(hub, _)| ChunkCoord::containing(*hub, size) == coord)
            .map(|&(_, h)| h)
            .collect()
    }

    /// Every corral horse of the world.
    pub fn all_horses(&self) -> impl Iterator<Item = &TetheredHorse> + '_ {
        self.horses.iter().map(|(_, h)| h)
    }

    /// The ducks whose raft's home lies in chunk `coord`.
    pub fn ducks_in(&self, coord: ChunkCoord) -> Vec<Duck> {
        let size = self.config.chunk_size;
        self.ducks
            .iter()
            .filter(|d| ChunkCoord::containing(d.home, size) == coord)
            .cloned()
            .collect()
    }

    /// The hares of chunk `coord`, sitting in open grass.
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

    /// The tero flock of chunk `coord`, if it has one.
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

    /// The raptor circling over chunk `coord`, if any.
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

    /// Chunk `coord`'s ground: the home its wild animals keep to.
    fn home(&self, coord: ChunkCoord) -> Aabb {
        coord.bounds(self.config.chunk_size)
    }

    fn chunk_rng(&self, coord: ChunkCoord, kind: u32) -> Rng {
        Rng::new(
            self.seed ^ kind.wrapping_mul(0x51ed_270b),
            coord.seed(self.seed),
        )
    }

    /// A clear spot of open grass in chunk `coord`: clear of roads, towns,
    /// lagoons, the railway and settlements. A few tries.
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

    /// Whether animals may graze at `p`: inside the world, clear of every
    /// road (past its roadside fence), town, lagoon bank, the railway and
    /// every settlement.
    pub fn grazeable(&self, p: Vec2) -> bool {
        let inner = self.config.bounds();
        if p.x.abs() > inner.max.x - 20.0 || p.y.abs() > inner.max.y - 20.0 {
            return false;
        }
        // The margin is well inside the road index's reach.
        let off_roads = self
            .roads
            .as_ref()
            .is_none_or(|r| r.near(p, Self::ROAD_MARGIN).is_none());
        off_roads
            && self.flat_areas.iter().all(|a| {
                let dx = ((p.x - a.center.x).abs() - a.half_extents.x).max(0.0);
                let dy = ((p.y - a.center.y).abs() - a.half_extents.y).max(0.0);
                dx.hypot(dy) >= a.falloff + Self::TOWN_MARGIN
            })
            && self
                .lagoons
                .iter()
                .all(|l| l.offshore(p) >= Lagoon::BANK + Lagoon::MARGIN)
            && self
                .rail
                .iter()
                .all(|&(a, b)| segment_distance(p, a, b) >= Self::RAIL_MARGIN)
            && self.keep_out.is_clear(p, KeepOut::MAX_MARGIN)
    }

    /// Whether every sample over `pasture` is grazeable.
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

    /// One herd for some fenced fields, in a clear pasture of their own.
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

    /// A clear pasture inside `field`, a few tries.
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

    /// A few rafts of ducks on every lagoon, out on open water.
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

    /// The lagoons ducks swim on, by index.
    pub fn lagoons(&self) -> &[Lagoon] {
        &self.lagoons
    }
}

/// The horses standing loose in `settlement`'s corrals: 1–3 at an
/// estancia's, 1–2 at a chacra's, sometimes one at a puesto's.
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
        // Clear of the rails, a horse length's half to spare.
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

/// The shortest turn from `from` to `to`, in radians, in `(-π, π]`.
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

    fn site() -> (FaunaSite, FenceLayout, crate::World, SettlementLayout) {
        let (world, towns, line, fences, layout) = settled();
        let path = line.full_path(&towns);
        let started = std::time::Instant::now();
        let site = FaunaSite::new(
            world.config,
            &world.terrain,
            &fences,
            &layout,
            path.points(),
        );
        println!(
            "fauna: {} herds ({} head), {} corral horses, {} ducks; seeded in {:?}",
            site.herds.len(),
            site.herds.iter().map(|h| h.cows.len()).sum::<usize>(),
            site.horses.len(),
            site.ducks.len(),
            started.elapsed()
        );
        (site, fences, world, layout)
    }

    #[test]
    fn herds_are_seeded_the_same_way_every_time_and_sparsely() {
        let (site, fences, world, layout) = site();
        let (_, towns, line, ..) = settled();
        let again = FaunaSite::new(
            world.config,
            &world.terrain,
            &fences,
            &layout,
            line.full_path(&towns).points(),
        );
        assert_eq!(site, again);
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
        assert_eq!(coats.len(), 3, "every coat grazes somewhere");
    }

    #[test]
    fn herds_graze_only_inside_fenced_fields_off_roads_towns_and_settlements() {
        let (site, fences, world, layout) = site();
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
        let (site, fences, mut world, layout) = site();
        let (_, towns, line, ..) = settled();
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
            &fences,
            &layout,
            line.full_path(&towns).points(),
        );
        // Its pasture moved off the water (or the herd is gone), and none
        // grazes on the bank.
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
    }

    #[test]
    fn corral_horses_stand_inside_the_corrals_of_farms_and_ranches() {
        let (site, _, _, layout) = site();
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
        let (site, ..) = site();
        let mut hares = 0;
        let mut flocks = 0;
        let mut raptors = 0;
        for coord in ChunkCoord::new(0, 0).within(12) {
            let h = site.hares_in(coord);
            assert_eq!(h, site.hares_in(coord), "deterministic");
            let home = coord.bounds(site.config.chunk_size);
            for hare in &h {
                assert!(site.grazeable(hare.position));
                assert_eq!(hare.home, home);
                // Chased about for a while, it stays on open grass at home.
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
    fn herds_stream_with_one_chunk_each() {
        let (site, ..) = site();
        let size = site.config.chunk_size;
        let chunks: std::collections::BTreeSet<_> = site
            .herds
            .iter()
            .map(|h| ChunkCoord::containing(h.pasture.center(), size))
            .collect();
        let streamed: usize = chunks.iter().map(|&c| site.herds_in(c).len()).sum();
        assert_eq!(streamed, site.herds.len());
        // A herd stepped for minutes with a rider about stays in its pasture.
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
