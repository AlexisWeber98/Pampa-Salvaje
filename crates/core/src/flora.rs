//! Native flora of the open pampa between towns: grass tufts (paja and
//! flechilla) and caldén and chañar trees, scattered and in small groves.
//!
//! Placement is pure and deterministic: each chunk's flora is seeded by its
//! coordinate, so render can rebuild any chunk at any time and logic can
//! list every tree of the world once, at load, for collision.

use crate::rng::Rng;
use std::sync::Arc;

use crate::{
    Aabb, ChunkCoord, FenceLayout, FlatArea, LinePath, RailLine, RoadNetwork, Terrain, Vec2,
    WorldConfig,
};

/// A native tree of the caldenal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Species {
    /// *Prosopis caldenia*: 6–9 m, a dark rough trunk and a broad umbrella
    /// crown.
    Calden,
    /// *Geoffroea decorticans*: 3–5 m. It grows in dense thickets
    /// (chañarales), so it is drawn and collides as one.
    Chanar,
}

/// One tree on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    pub at: Vec2,
    pub species: Species,
    /// Height to the top of the crown, in meters.
    pub height: f32,
    /// Turn about the vertical, in radians.
    pub yaw: f32,
    /// Seeded shape variant (render picks a mesh with it).
    pub variant: u32,
}

/// A kind of grass tuft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuftKind {
    /// Paja: tall coarse bunch grass.
    Paja,
    /// Flechilla: low, fine, pale needle grass.
    Flechilla,
}

/// One grass tuft. Tufts are not obstacles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuft {
    pub at: Vec2,
    pub kind: TuftKind,
    /// Height, in meters.
    pub size: f32,
    pub yaw: f32,
}

/// The flora of one chunk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChunkFlora {
    pub trees: Vec<Tree>,
    pub tufts: Vec<Tuft>,
}

/// Where flora may grow: the world, its towns' leveled ground, the
/// railway line and the roads.
#[derive(Debug, Clone, PartialEq)]
pub struct FloraSite {
    config: WorldConfig,
    flat_areas: Vec<FlatArea>,
    rail: Vec<(Vec2, Vec2)>,
    roads: Option<Arc<RoadNetwork>>,
    /// The surveyed fence lines: `x` of the north–south ones, `y` of the
    /// east–west ones.
    fences: (Vec<f32>, Vec<f32>),
    seed: u32,
}

impl FloraSite {
    /// Most trees one chunk holds.
    pub const TREE_BUDGET: usize = 90;
    /// Most tufts one chunk holds.
    pub const TUFT_BUDGET: usize = 600;
    /// Least distance from a tree's trunk to the track centerline, in
    /// meters: a caldén's crown stays clear of the cars.
    pub const TREE_RAIL_MARGIN: f32 = 12.0;
    /// Least distance from a tuft to the track centerline, in meters: off
    /// the ballast.
    pub const TUFT_RAIL_MARGIN: f32 = 3.5;
    /// Least distance from a tree's trunk to a road's centerline, in
    /// meters: the road, a verge and the crown's overhang stay clear.
    pub const TREE_ROAD_MARGIN: f32 = RoadNetwork::HALF_WIDTH + 6.0;
    /// Least distance from a tuft to a road's centerline, in meters.
    pub const TUFT_ROAD_MARGIN: f32 = RoadNetwork::HALF_WIDTH + 0.5;
    /// Least distance from a tree's trunk to a surveyed fence line, in
    /// meters: no fence runs through a trunk.
    pub const TREE_FENCE_MARGIN: f32 = 1.5;
    /// Least distance from a tree to the world's edge, in meters.
    pub const TREE_EDGE_MARGIN: f32 = 10.0;
    /// Least distance from a tree to a town's leveled band, in meters.
    pub const TREE_TOWN_MARGIN: f32 = 6.0;
    /// Tree candidates tried per chunk of [`Self::REFERENCE_CHUNK`] meters.
    const TREE_CANDIDATES: f32 = 220.0;
    /// Tuft candidates tried per chunk of [`Self::REFERENCE_CHUNK`] meters.
    const TUFT_CANDIDATES: f32 = 600.0;
    /// The chunk side the candidate counts are tuned for, in meters.
    const REFERENCE_CHUNK: f32 = 200.0;
    /// Chance that a candidate on open pampa becomes a lone tree.
    const LONE_TREE: f32 = 0.006;
    /// Extra chance inside a grove (scaled by the mask).
    const GROVE_TREE: f32 = 0.55;

    /// The flora of `config`'s world over `terrain` (already leveled for its
    /// towns, with its roads laid), kept off the `line`'s track and the
    /// roads.
    pub fn new(config: WorldConfig, terrain: &Terrain, line: &RailLine) -> Self {
        Self {
            config,
            flat_areas: terrain.flat_areas.clone(),
            rail: line.segments().map(|s| (s.start, s.end)).collect(),
            roads: terrain.roads.clone(),
            fences: (Vec::new(), Vec::new()),
            seed: terrain.seed ^ FLORA_SEED,
        }
    }

    /// This flora, also kept off every piece of the train's whole `path`
    /// ([`crate::RailLine::full_path`]): the links and each town's own
    /// straight track with its run-outs past the town, which
    /// [`Self::new`]'s links alone leave out.
    pub fn with_track(mut self, path: &LinePath) -> Self {
        self.rail
            .extend(path.points().windows(2).map(|w| (w[0], w[1])));
        self
    }

    /// Whether `p` is at least `margin` from every piece of track this
    /// flora keeps off (see [`Self::with_track`]): the one keep-out the
    /// terrain's stones and shrubs share with the trees and tufts.
    pub fn off_track(&self, p: Vec2, margin: f32) -> bool {
        rail_distance(&self.rail, p) >= margin
    }

    /// This flora, with trees kept off the surveyed lines of `fences`.
    pub fn with_fences(mut self, fences: &FenceLayout) -> Self {
        self.fences = fences.surveyed.clone();
        self
    }

    /// The world this flora grows in.
    pub fn config(&self) -> WorldConfig {
        self.config
    }

    /// The trees and tufts of chunk `coord`, seeded by its coordinate.
    /// Chunks outside the world grow nothing.
    pub fn chunk(&self, coord: ChunkCoord) -> ChunkFlora {
        if !self.config.has_chunk(coord) {
            return ChunkFlora::default();
        }
        let rail = self.rail_near(coord);
        let trees = self.chunk_trees(coord, &rail);
        let tufts = self.chunk_tufts(coord, &rail, &trees);
        ChunkFlora { trees, tufts }
    }

    /// The trees of chunk `coord` (cheaper than [`Self::chunk`]).
    pub fn trees(&self, coord: ChunkCoord) -> Vec<Tree> {
        if !self.config.has_chunk(coord) {
            return Vec::new();
        }
        self.chunk_trees(coord, &self.rail_near(coord))
    }

    /// The pieces of track near chunk `coord`.
    fn rail_near(&self, coord: ChunkCoord) -> Vec<(Vec2, Vec2)> {
        let area = coord.bounds(self.config.chunk_size);
        let reach = area.expanded(Self::TREE_RAIL_MARGIN);
        self.rail
            .iter()
            .copied()
            .filter(|&(a, b)| reach.segment_overlap(a, b).is_some())
            .collect()
    }

    /// Every tree of the world, chunk by chunk: the same trees
    /// [`Self::chunk`] gives.
    pub fn all_trees(&self) -> Vec<Tree> {
        let size = self.config.chunk_size;
        let bounds = self.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        let mut trees = Vec::new();
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                trees.extend(self.trees(ChunkCoord::new(x, y)));
            }
        }
        trees
    }

    /// How strongly the grove mask favors trees at `p`, in `[0, 1]`:
    /// low-frequency noise, so trees gather in small groves (isletas).
    pub fn grove(&self, p: Vec2) -> f32 {
        let n =
            (value_noise(self.seed, p, 170.0) + 0.3 * value_noise(self.seed ^ 0x51, p, 65.0)) / 1.3;
        smoothstep(0.56, 0.84, n)
    }

    /// How dry the grass is at `p`, in `[0, 1]`: 0 is grey-green, 1 dry
    /// yellow. Low-frequency and continuous.
    pub fn dryness(&self, p: Vec2) -> f32 {
        dryness_of(self.seed, p)
    }

    /// The trees of chunk `coord`, spaced apart within it and from the
    /// trees of its neighbours: where a tree stands too close to one of an
    /// earlier neighbour (the row below, or the chunk to the west), the
    /// later chunk's tree gives way. Each chunk is still decided on its own
    /// (the same trees every time, in any order), from the neighbours'
    /// trees before they gave way to theirs.
    fn chunk_trees(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)]) -> Vec<Tree> {
        let size = self.config.chunk_size;
        let mut trees = self.scattered_trees(coord, rail);
        for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0)] {
            let earlier = ChunkCoord::new(coord.x + dx, coord.y + dy);
            let reach = earlier.bounds(size).expanded(Tree::MAX_SPACING);
            if !self.config.has_chunk(earlier) || !trees.iter().any(|t| reach.contains_point(t.at))
            {
                continue;
            }
            let theirs = self.scattered_trees(earlier, &self.rail_near(earlier));
            trees.retain(|t| {
                theirs
                    .iter()
                    .all(|u| u.at.distance(t.at) >= u.spacing().max(t.spacing()))
            });
        }
        trees
    }

    /// The trees scattered over chunk `coord`, spaced apart within it.
    fn scattered_trees(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)]) -> Vec<Tree> {
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 0);
        let candidates = self.scaled(Self::TREE_CANDIDATES);
        let mut trees: Vec<Tree> = Vec::new();
        for _ in 0..candidates {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            let (pick, species_roll, height_roll, yaw) =
                (rng.unit(), rng.unit(), rng.unit(), rng.unit());
            let variant = rng.next() >> 28;
            if trees.len() >= Self::TREE_BUDGET {
                break;
            }
            let grove = self.grove(at);
            if pick >= Self::LONE_TREE + Self::GROVE_TREE * grove {
                continue;
            }
            // Caldenes rule the groves; chañares fill their edges and the
            // open ground.
            let species = if species_roll < 0.35 + 0.35 * grove {
                Species::Calden
            } else {
                Species::Chanar
            };
            let (lo, hi) = species.height_range();
            let tree = Tree {
                at,
                species,
                height: lo + (hi - lo) * height_roll,
                yaw: yaw * std::f32::consts::TAU,
                variant,
            };
            let spaced = trees
                .iter()
                .all(|t| t.at.distance(at) >= t.spacing().max(tree.spacing()));
            if spaced && self.tree_room(at, rail) {
                trees.push(tree);
            }
        }
        trees
    }

    fn chunk_tufts(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)], trees: &[Tree]) -> Vec<Tuft> {
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 1);
        let candidates = self.scaled(Self::TUFT_CANDIDATES);
        let bounds = self.config.bounds();
        let mut tufts = Vec::new();
        for _ in 0..candidates {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            let (kind_roll, size_roll, yaw) = (rng.unit(), rng.unit(), rng.unit());
            if tufts.len() >= Self::TUFT_BUDGET {
                break;
            }
            let inside = at.x - bounds.min.x >= 0.5
                && bounds.max.x - at.x >= 0.5
                && at.y - bounds.min.y >= 0.5
                && bounds.max.y - at.y >= 0.5;
            let open = inside
                && self.flat_areas.iter().all(|a| a.weight(at) == 0.0)
                && rail_distance(rail, at) >= Self::TUFT_RAIL_MARGIN
                && self.off_road(at, Self::TUFT_ROAD_MARGIN)
                && trees
                    .iter()
                    .all(|t| t.at.distance(at) > t.trunk_radius() + 0.4);
            if !open {
                continue;
            }
            // Paja gathers where the grass is greener; flechilla on dry ground.
            let kind = if kind_roll < 0.25 + 0.4 * (1.0 - self.dryness(at)) {
                TuftKind::Paja
            } else {
                TuftKind::Flechilla
            };
            let (lo, hi) = kind.size_range();
            tufts.push(Tuft {
                at,
                kind,
                size: lo + (hi - lo) * size_roll,
                yaw: yaw * std::f32::consts::TAU,
            });
        }
        tufts
    }

    /// Whether a tree may stand at `p`: inside the world, clear of every
    /// town's band, of the track and of the roads.
    fn tree_room(&self, p: Vec2, rail: &[(Vec2, Vec2)]) -> bool {
        let bounds = self.config.bounds();
        let m = Self::TREE_EDGE_MARGIN;
        let inside = p.x - bounds.min.x >= m
            && bounds.max.x - p.x >= m
            && p.y - bounds.min.y >= m
            && bounds.max.y - p.y >= m;
        inside
            && self
                .flat_areas
                .iter()
                .all(|a| outside(a, p) >= a.falloff + Self::TREE_TOWN_MARGIN)
            && rail_distance(rail, p) >= Self::TREE_RAIL_MARGIN
            && self.off_road(p, Self::TREE_ROAD_MARGIN)
            && self.off_fences(p)
    }

    /// Whether `p` is at least [`Self::TREE_FENCE_MARGIN`] from every
    /// surveyed fence line.
    fn off_fences(&self, p: Vec2) -> bool {
        let clear = |lines: &[f32], v: f32| {
            let i = lines.partition_point(|&l| l < v);
            [i.checked_sub(1), Some(i)]
                .into_iter()
                .flatten()
                .filter_map(|i| lines.get(i))
                .all(|l| (l - v).abs() >= Self::TREE_FENCE_MARGIN)
        };
        clear(&self.fences.0, p.x) && clear(&self.fences.1, p.y)
    }

    /// Whether `p` is at least `margin` meters from every road.
    fn off_road(&self, p: Vec2, margin: f32) -> bool {
        self.roads
            .as_ref()
            .is_none_or(|r| r.near(p, margin).is_none_or(|d| d >= margin))
    }

    /// `per_reference` candidates scaled to this world's chunk area.
    fn scaled(&self, per_reference: f32) -> usize {
        let ratio = self.config.chunk_size / Self::REFERENCE_CHUNK;
        (per_reference * ratio * ratio).round() as usize
    }
}

impl Species {
    /// Lowest and highest tree, in meters.
    pub fn height_range(self) -> (f32, f32) {
        match self {
            Self::Calden => (6.0, 9.0),
            Self::Chanar => (3.0, 5.0),
        }
    }
}

impl TuftKind {
    /// Lowest and highest tuft, in meters.
    pub fn size_range(self) -> (f32, f32) {
        match self {
            Self::Paja => (0.5, 0.9),
            Self::Flechilla => (0.25, 0.5),
        }
    }
}

impl Tree {
    /// Trunk radius at the ground, in meters. A chañar's is its thicket's
    /// dense stem core, which no rider pushes through.
    pub fn trunk_radius(&self) -> f32 {
        match self.species {
            Species::Calden => 0.035 * self.height,
            Species::Chanar => 0.09 * self.height,
        }
    }

    /// Radius of the crown seen from above, in meters.
    pub fn crown_radius(&self) -> f32 {
        match self.species {
            Species::Calden => 0.55 * self.height,
            Species::Chanar => 0.35 * self.height,
        }
    }

    /// The trunk's footprint, a square as wide as the trunk: a static
    /// obstacle.
    pub fn obstacle(&self) -> Aabb {
        let side = 2.0 * self.trunk_radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
    }

    /// The largest [`Self::spacing`] of any species, in meters.
    const MAX_SPACING: f32 = 5.0;

    /// Least distance to another tree's trunk, in meters.
    fn spacing(&self) -> f32 {
        match self.species {
            Species::Calden => Self::MAX_SPACING,
            Species::Chanar => 3.0,
        }
    }
}

/// How dry the grass of `terrain` is at `p`, in `[0, 1]`: the same as
/// [`FloraSite::dryness`] for a site over that terrain. Render tints the
/// ground with it.
pub fn grass_dryness(terrain: &Terrain, p: Vec2) -> f32 {
    dryness_of(terrain.seed ^ FLORA_SEED, p)
}

fn dryness_of(seed: u32, p: Vec2) -> f32 {
    let s = seed ^ 0xd1e5;
    let n = (value_noise(s, p, 420.0) + 0.35 * value_noise(s ^ 0x77, p, 130.0)) / 1.35;
    smoothstep(0.3, 0.7, n)
}

/// Mixed into the terrain seed for every flora stream.
const FLORA_SEED: u32 = 0xf10a_2a11;

/// Distance from `p` to the flat core of `area` (0 inside it).
fn outside(area: &FlatArea, p: Vec2) -> f32 {
    let dx = ((p.x - area.center.x).abs() - area.half_extents.x).max(0.0);
    let dy = ((p.y - area.center.y).abs() - area.half_extents.y).max(0.0);
    dx.hypot(dy)
}

/// Distance from `p` to the nearest of the `rail` segments.
fn rail_distance(rail: &[(Vec2, Vec2)], p: Vec2) -> f32 {
    rail.iter()
        .map(|&(a, b)| segment_distance(p, a, b))
        .fold(f32::INFINITY, f32::min)
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise in `[0, 1]` with lattice cells `cell` meters wide.
fn value_noise(seed: u32, p: Vec2, cell: f32) -> f32 {
    let (x, y) = (p.x / cell, p.y / cell);
    let (i, j) = (x.floor(), y.floor());
    let (fx, fy) = (fade(x - i), fade(y - j));
    let (i, j) = (i as i32, j as i32);
    let at =
        |i: i32, j: i32| Rng::new(seed ^ (i as u32).wrapping_mul(0x85eb_ca6b), j as u32).unit();
    let bottom = at(i, j) + (at(i + 1, j) - at(i, j)) * fx;
    let top = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * fx;
    bottom + (top - bottom) * fy
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Distance from `p` to the segment `a`–`b`.
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let span = d.dot(d);
    if span <= 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(d) / span).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Circle, Heading, Rider, RiderInput, RiderParams, TownConfig, World, place_towns};

    /// Two railway towns 3.6 km apart and the line between them.
    fn site() -> (World, RailLine, FloraSite) {
        let configs = [
            TownConfig::castex(),
            TownConfig {
                name: "Lejos".into(),
                origin: Vec2::new(3000.0, -2000.0),
                ..TownConfig::castex()
            },
        ];
        let config = WorldConfig::default();
        let (towns, errors) = place_towns(&config, &configs);
        assert!(errors.is_empty(), "{errors:?}");
        let world = World::with_towns(config, Terrain::default(), &towns);
        let (line, errors) = RailLine::build(None, &towns, world.bounds);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.links.len(), 1);
        let flora = FloraSite::new(config, &world.terrain, &line);
        (world, line, flora)
    }

    fn rail_distance(line: &RailLine, p: Vec2) -> f32 {
        line.segments()
            .map(|s| segment_distance(p, s.start, s.end))
            .fold(f32::INFINITY, f32::min)
    }

    /// A block of chunks between the two towns, across the line.
    fn between() -> Vec<ChunkCoord> {
        (0..14)
            .flat_map(|x| (-11..1).map(move |y| ChunkCoord::new(x, y)))
            .collect()
    }

    #[test]
    fn scatter_is_deterministic_per_chunk() {
        let (_, _, flora) = site();
        let a = flora.chunk(ChunkCoord::new(5, -4));
        assert!(!a.tufts.is_empty());
        assert_eq!(a, flora.chunk(ChunkCoord::new(5, -4)));
        assert_ne!(a, flora.chunk(ChunkCoord::new(6, -4)));
        let size = WorldConfig::default().chunk_size;
        let bounds = ChunkCoord::new(5, -4).bounds(size);
        assert!(a.tufts.iter().all(|t| bounds.contains_point(t.at)));
        assert!(a.trees.iter().all(|t| bounds.contains_point(t.at)));
        assert!(a.tufts.len() <= FloraSite::TUFT_BUDGET);
        assert!(a.trees.len() <= FloraSite::TREE_BUDGET);
    }

    #[test]
    fn planting_the_whole_world_stays_quick() {
        let (mut world, _, flora) = site();
        let started = std::time::Instant::now();
        let planted = world.plant(&flora);
        let took = started.elapsed();
        assert!(planted > 1000, "{planted}");
        // A generous bound: startup must not stall on the trees.
        assert!(took.as_secs_f32() < 10.0, "planted in {took:?}");
    }

    #[test]
    fn trees_keep_their_spacing_across_chunk_borders() {
        let (_, _, flora) = site();
        let trees: Vec<(ChunkCoord, Tree)> = between()
            .into_iter()
            .flat_map(|c| flora.trees(c).into_iter().map(move |t| (c, t)))
            .collect();
        assert!(trees.len() > 200, "{}", trees.len());
        let mut near_borders = 0;
        for (i, (ca, a)) in trees.iter().enumerate() {
            for (cb, b) in &trees[i + 1..] {
                if ca == cb {
                    continue;
                }
                let gap = a.at.distance(b.at);
                if gap < 10.0 {
                    near_borders += 1;
                }
                let least = a.spacing().max(b.spacing());
                assert!(
                    gap >= least,
                    "{ca:?} {:?} and {cb:?} {:?}: {gap}",
                    a.at,
                    b.at
                );
            }
        }
        assert!(near_borders > 0, "no pair close across a border to check");
        // Still deterministic, chunk by chunk.
        for c in between().into_iter().step_by(7) {
            assert_eq!(flora.trees(c), flora.trees(c));
            assert_eq!(flora.trees(c), flora.chunk(c).trees);
        }
    }

    #[test]
    fn trees_have_their_species_sizes() {
        let (_, _, flora) = site();
        let trees: Vec<Tree> = between()
            .into_iter()
            .flat_map(|c| flora.chunk(c).trees)
            .collect();
        let calden: Vec<&Tree> = trees
            .iter()
            .filter(|t| t.species == Species::Calden)
            .collect();
        let chanar: Vec<&Tree> = trees
            .iter()
            .filter(|t| t.species == Species::Chanar)
            .collect();
        assert!(
            calden.len() > 20 && chanar.len() > 20,
            "{} {}",
            calden.len(),
            chanar.len()
        );
        assert!(calden.iter().all(|t| (6.0..=9.0).contains(&t.height)));
        assert!(chanar.iter().all(|t| (3.0..=5.0).contains(&t.height)));
        // A chañar is drawn as a dense thicket: its obstacle is the thicket's
        // stem core, wider than a single trunk.
        assert!(
            chanar
                .iter()
                .all(|t| t.trunk_radius() >= 0.08 * t.height && t.trunk_radius() < t.crown_radius())
        );
        assert!(
            trees
                .iter()
                .all(|t| t.trunk_radius() > 0.08 && t.trunk_radius() < 0.5)
        );
    }

    #[test]
    fn nothing_grows_on_town_ground_the_rail_corridor_or_off_the_world() {
        let (world, line, flora) = site();
        let bounds = world.bounds;
        let mut chunks = between();
        // Around both towns, and along the world's west and south edges.
        chunks.extend((-4..4).flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y))));
        chunks.extend((11..19).flat_map(|x| (-14..-6).map(move |y| ChunkCoord::new(x, y))));
        chunks.extend((-20..-17).flat_map(|x| (-20..-10).map(move |y| ChunkCoord::new(x, y))));
        let (mut trees, mut tufts) = (0, 0);
        for c in chunks {
            let f = flora.chunk(c);
            for t in &f.trees {
                for area in &world.terrain.flat_areas {
                    assert_eq!(area.weight(t.at), 0.0, "tree on town ground {t:?}");
                }
                assert!(
                    rail_distance(&line, t.at) >= FloraSite::TREE_RAIL_MARGIN,
                    "{t:?}"
                );
                assert!(
                    t.at.x - bounds.min.x >= FloraSite::TREE_EDGE_MARGIN,
                    "{t:?}"
                );
                assert!(
                    t.at.y - bounds.min.y >= FloraSite::TREE_EDGE_MARGIN,
                    "{t:?}"
                );
                trees += 1;
            }
            for t in &f.tufts {
                for area in &world.terrain.flat_areas {
                    assert_eq!(area.weight(t.at), 0.0, "tuft on town ground {t:?}");
                }
                assert!(
                    rail_distance(&line, t.at) >= FloraSite::TUFT_RAIL_MARGIN,
                    "{t:?}"
                );
                assert!(bounds.contain(t.at, 0.5).push_x.is_none(), "{t:?}");
                assert!(bounds.contain(t.at, 0.5).push_y.is_none(), "{t:?}");
                tufts += 1;
            }
        }
        assert!(trees > 50 && tufts > 5000, "{trees} {tufts}");
        // Chunks wholly outside the world grow nothing.
        assert_eq!(flora.chunk(ChunkCoord::new(-21, 0)), ChunkFlora::default());
        assert_eq!(flora.chunk(ChunkCoord::new(0, 20)), ChunkFlora::default());
    }

    #[test]
    fn trees_cluster_in_groves() {
        let (_, _, flora) = site();
        let counts: Vec<f32> = (-19..1)
            .flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y)))
            .map(|c| flora.chunk(c).trees.len() as f32)
            .collect();
        let n = counts.len() as f32;
        let mean = counts.iter().sum::<f32>() / n;
        let variance = counts.iter().map(|c| (c - mean).powi(2)).sum::<f32>() / n;
        // A uniform (Poisson) scatter has variance equal to its mean.
        assert!(mean > 2.0, "mean {mean}");
        assert!(variance > 4.0 * mean, "variance {variance}, mean {mean}");
        // Some chunks are open pampa, some hold a grove.
        assert!(counts.iter().any(|&c| c <= 2.0));
        assert!(counts.iter().any(|&c| c >= 25.0));
        // The mask is smooth: no jump between two points a meter apart.
        for k in 0..500 {
            let p = Vec2::new(k as f32 * 7.3 - 1800.0, k as f32 * -3.1 + 400.0);
            let g = flora.grove(p);
            assert!((0.0..=1.0).contains(&g));
            assert!((g - flora.grove(p + Vec2::new(1.0, 0.0))).abs() < 0.05);
        }
    }

    #[test]
    fn dryness_is_smooth_and_varied() {
        let (_, _, flora) = site();
        let (mut lo, mut hi) = (1.0_f32, 0.0_f32);
        for k in 0..2000 {
            let p = Vec2::new(k as f32 * 3.7 - 3000.0, (k as f32 * 0.37).sin() * 900.0);
            let d = flora.dryness(p);
            assert!((0.0..=1.0).contains(&d));
            assert!((d - flora.dryness(p + Vec2::new(0.5, 0.5))).abs() < 0.02);
            assert_eq!(d, grass_dryness(&Terrain::default(), p));
            lo = lo.min(d);
            hi = hi.max(d);
        }
        assert!(hi - lo > 0.6, "{lo}..{hi}");
    }

    #[test]
    fn every_tree_is_an_obstacle_and_a_rider_is_pushed_off_a_calden_trunk() {
        let (mut world, _, flora) = site();
        let before = world.obstacles.len();
        let planted = world.plant(&flora);
        let all = flora.all_trees();
        assert_eq!(planted, all.len());
        assert_eq!(world.obstacles.len(), before + all.len());
        // Every chunk's trees are in the world's list, chunk or not loaded.
        for c in between() {
            for tree in flora.chunk(c).trees {
                assert!(all.contains(&tree), "{tree:?}");
                assert!(world.obstacles.contains(&tree.obstacle()));
            }
        }
        let calden = *all
            .iter()
            .find(|t| t.species == Species::Calden)
            .expect("a caldén");
        // Ride at the trunk from 10 m east, heading west, at a gallop.
        let start = calden.at + Vec2::new(10.0, 0.0);
        let mut rider = Rider::new(start, Heading::from_radians(std::f32::consts::PI), &world);
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let params = RiderParams::default();
        let radius = Circle::new(Rider::COLLIDER_RADIUS).radius;
        for _ in 0..120 {
            rider.step(input, &params, &world, 1.0 / 60.0);
            let gap = calden
                .obstacle()
                .closest_point(rider.position)
                .distance(rider.position);
            assert!(
                gap >= radius - 1e-3,
                "inside the trunk at {:?}",
                rider.position
            );
        }
        // It stopped against the trunk rather than passing it.
        assert!(rider.position.x > calden.at.x, "{:?}", rider.position);
    }

    #[test]
    fn nothing_grows_on_the_trains_path_anywhere_along_the_line() {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let path = line.full_path(&towns);
        let flora = FloraSite::new(config, &world.terrain, &line).with_track(&path);
        let gap = |p: Vec2| p.distance(path.at(path.project(p)));
        // Every chunk the path crosses, sampled every 20 m.
        let mut chunks: Vec<ChunkCoord> = (0..=(path.length() / 20.0) as usize)
            .map(|k| config.chunk_of(path.at(k as f32 * 20.0)))
            .collect();
        chunks.sort_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let (mut trees, mut tufts) = (0, 0);
        for c in chunks {
            let f = flora.chunk(c);
            for t in &f.trees {
                // The trunk and the crown stay clear of the cars.
                let g = gap(t.at);
                assert!(
                    g >= FloraSite::TREE_RAIL_MARGIN - 1e-3,
                    "{t:?}: {g} m from the track"
                );
                trees += 1;
            }
            for t in &f.tufts {
                assert!(gap(t.at) >= FloraSite::TUFT_RAIL_MARGIN - 1e-3, "{t:?}");
                tufts += 1;
            }
        }
        assert!(trees > 20 && tufts > 1000, "{trees} {tufts}");
        // The margin clears a car's half width with room for a crown.
        const { assert!(FloraSite::TREE_RAIL_MARGIN > crate::LineTrain::WIDTH / 2.0 + 6.0) };
    }

    #[test]
    fn nothing_grows_on_a_road_or_its_verges() {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let roads = world.terrain.roads.clone().expect("roads laid");
        let flora = FloraSite::new(config, &world.terrain, &line);
        let mut chunks: Vec<ChunkCoord> = roads
            .segments()
            .step_by(40)
            .map(|(a, _)| config.chunk_of(a))
            .collect();
        chunks.sort_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let (mut trees, mut tufts) = (0, 0);
        for c in chunks {
            let f = flora.chunk(c);
            for t in &f.trees {
                let gap = roads.distance(t.at);
                assert!(gap >= FloraSite::TREE_ROAD_MARGIN, "{t:?}: {gap}");
                trees += 1;
            }
            for t in &f.tufts {
                assert!(roads.distance(t.at) >= FloraSite::TUFT_ROAD_MARGIN, "{t:?}");
                tufts += 1;
            }
            assert_eq!(f.trees, flora.trees(c));
        }
        assert!(trees > 20 && tufts > 2000, "{trees} {tufts}");
    }
}
