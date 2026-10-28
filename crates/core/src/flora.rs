//! Native flora of the open pampa between towns: grass tufts (paja and
//! flechilla), caldén and chañar trees, scattered and in small groves and
//! in the dense monte patches, and the shrubs (piquillín, jarilla and
//! chañar thickets) of the understory.
//!
//! Placement is pure and deterministic: each chunk's flora is seeded by its
//! coordinate, so render can rebuild any chunk at any time and logic can
//! list every tree of the world once, at load, for collision.

use crate::rng::Rng;
use std::sync::Arc;

use crate::{
    Aabb, ChunkCoord, FenceLayout, FlatArea, KeepOut, Lagoon, LinePath, RailLine, RoadNetwork,
    SettlementLayout, Terrain, Vec2, WorldConfig,
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

/// How far along its life a tree is: it decides the size and the girth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Age {
    /// A seedling's first years: a slim stem and a small crown.
    Sapling,
    /// A young tree, still growing.
    Young,
    /// A full-grown tree: most of the stand.
    Mature,
    /// A rare old giant: tall, thick-trunked and broad-crowned.
    Old,
}

/// One tree on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    pub at: Vec2,
    pub species: Species,
    pub age: Age,
    /// Height to the top of the crown, in meters.
    pub height: f32,
    /// How wide the crown and trunk stand against the height's scale
    /// (1 is as modelled): a broad tree or a narrow one.
    pub width: f32,
    /// Turn about the vertical, in radians.
    pub yaw: f32,
    /// Seeded look: 32 random bits (render picks the form, the leaf and
    /// bark tint and whether a caldén is a snag from its bytes).
    pub variant: u32,
}

/// A kind of native shrub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShrubKind {
    /// Piquillín (*Condalia microphylla*): a dense thorny shrub of tiny dark
    /// leaves, the understory of the monte.
    Piquillin,
    /// Jarilla (*Larrea divaricata*): a resinous yellow-green shrub of many
    /// upright stems, on open dry ground.
    Jarilla,
    /// Fachinal: a thicket of chañar suckers, along the groves' edges.
    Fachinal,
}

/// One shrub on the ground plane. Not an obstacle: a rider rides through
/// it (user decision, 2026-10-27).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shrub {
    pub at: Vec2,
    pub kind: ShrubKind,
    /// Height to the top, in meters.
    pub height: f32,
    /// How wide it stands against the height's scale (1 is as modelled).
    pub width: f32,
    /// Turn about the vertical, in radians.
    pub yaw: f32,
    /// Seeded look: 32 random bits (render picks the form and the tints).
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
    pub shrubs: Vec<Shrub>,
}

/// Where flora may grow: the world, its towns' leveled ground, the
/// railway line and the roads.
#[derive(Debug, Clone, PartialEq)]
pub struct FloraSite {
    config: WorldConfig,
    flat_areas: Vec<FlatArea>,
    /// The lagoons: grass stops at their wet mud, trees a little farther.
    lagoons: Vec<Lagoon>,
    rail: Vec<(Vec2, Vec2)>,
    roads: Option<Arc<RoadNetwork>>,
    /// The surveyed fence lines: `x` of the north–south ones, `y` of the
    /// east–west ones.
    fences: (Vec<f32>, Vec<f32>),
    /// The settlements' footprints, patios and tracks.
    settlements: Option<Arc<KeepOut>>,
    /// The hubs of every settlement: the land round a casco, a puesto, a
    /// chacra or a rancho is cleared for orchard, animals and crops, so the
    /// monte keeps a little back from it
    /// ([`Self::rural`]).
    worked: Vec<Vec2>,
    /// Whether [`Self::rural`] scales the stands (always, outside the
    /// tests that look at a stand's own shape).
    redistribute: bool,
    seed: u32,
}

impl FloraSite {
    /// Most trees one chunk plants: the monte's. The groves, as dense as they
    /// always were, stop at [`Self::GROVE_BUDGET`]. The budgets count the
    /// trees planted: [`Self::TREE_KEEP`] of them stay, so a chunk holds at
    /// most about a half of these.
    pub const TREE_BUDGET: usize = 160;
    /// Most trees a chunk's groves and scattered trees add to it.
    pub const GROVE_BUDGET: usize = 90;
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
    /// Least distance from a tree to a lagoon's waterline, in meters.
    pub const TREE_LAGOON_MARGIN: f32 = 12.0;
    /// Least distance from a tree to a town's leveled band, in meters.
    pub const TREE_TOWN_MARGIN: f32 = 6.0;
    /// Least distance from a tree's trunk to a settlement's footprints,
    /// patio and tracks, in meters: crowns stay off roofs and ruts.
    pub const TREE_SETTLEMENT_MARGIN: f32 = 4.0;
    /// Least distance from a tuft, stone or shrub to a settlement's
    /// footprints, patio and tracks, in meters.
    pub const TUFT_SETTLEMENT_MARGIN: f32 = 0.3;
    /// Most shrubs one chunk holds.
    pub const SHRUB_BUDGET: usize = 300;
    /// Least distance from a shrub to the track centerline, in meters.
    pub const SHRUB_RAIL_MARGIN: f32 = 6.0;
    /// Least distance from a shrub to a road's centerline, in meters.
    pub const SHRUB_ROAD_MARGIN: f32 = RoadNetwork::HALF_WIDTH + 1.5;
    /// Least distance from a shrub to a town's leveled band, in meters.
    pub const SHRUB_TOWN_MARGIN: f32 = 2.0;
    /// Least distance from a shrub to a lagoon's waterline, in meters.
    pub const SHRUB_LAGOON_MARGIN: f32 = 4.0;
    /// Least distance from a shrub to a settlement's footprints, patio and
    /// tracks, in meters.
    pub const SHRUB_SETTLEMENT_MARGIN: f32 = 1.5;
    /// Least distance from a shrub to the world's edge, in meters.
    pub const SHRUB_EDGE_MARGIN: f32 = 3.0;
    /// Least distance between two shrubs, in meters: the stands keep lanes
    /// a rider can pick through, wider than the rider's collider.
    pub const SHRUB_SPACING: f32 = 1.9;
    /// Clear ground between a shrub and a tree's trunk, in meters.
    const SHRUB_TRUNK_GAP: f32 = 0.6;
    /// How near a chunk's border a shrub must stand for the trees of the
    /// chunk across it to be checked, in meters. It must cover the farthest
    /// a trunk keeps a shrub off: the stoutest trunk, the stoutest shrub
    /// and [`Self::SHRUB_TRUNK_GAP`] (a test holds it to that).
    const SHRUB_BORDER: f32 = 2.0;
    /// The share of the planted trees that stay (user, 2026-10-27: "sacale
    /// un 20% al total", then "sacale otro 30%": 0.8 x 0.7, then "un 10% más de árboles":
    /// 0.56 x 1.1): monte, groves
    /// and lone trees thinned alike. Shrubs are not thinned.
    pub const TREE_KEEP: f32 = 0.616;
    /// Within this far of a town's leveled ground (meters) the monte and the
    /// groves keep [`Self::TOWN_NEAR_SHARE`] of their density.
    pub const TOWN_NEAR: f32 = 1500.0;
    /// The share of its density the land right round a town keeps.
    pub const TOWN_NEAR_SHARE: f32 = 0.12;
    /// From this far (meters) the density is the plain one; past it it
    /// grows to [`Self::FAR_BOOST`].
    pub const TOWN_FULL: f32 = 3000.0;
    /// How far (meters) the open country is from a town when it is as dense
    /// as it gets.
    pub const FAR_FULL: f32 = 9000.0;
    /// How many times denser than the plain density the far country is.
    pub const FAR_BOOST: f32 = 1.6;
    /// How far (noise units) the far country's monte and grove edges drop
    /// per unit of [`Self::rural`] over 1.
    const FAR_WIDEN: f32 = 0.12;
    /// The share of its density the land at a settlement's hub
    /// keeps.
    pub const WORKED_SHARE: f32 = 0.3;
    /// Within this far of a settlement's hub (meters) the land
    /// is worked.
    pub const WORKED_NEAR: f32 = 120.0;
    /// From this far (meters) the land round a casco is as dense as any.
    pub const WORKED_FULL: f32 = 500.0;
    /// Tree candidates tried per chunk of [`Self::REFERENCE_CHUNK`] meters.
    const TREE_CANDIDATES: f32 = 800.0;
    /// Shrub candidates tried per chunk of [`Self::REFERENCE_CHUNK`] meters.
    const SHRUB_CANDIDATES: f32 = 500.0;
    /// Tuft candidates tried per chunk of [`Self::REFERENCE_CHUNK`] meters.
    const TUFT_CANDIDATES: f32 = 600.0;
    /// The chunk side the candidate counts are tuned for, in meters.
    const REFERENCE_CHUNK: f32 = 200.0;
    /// Chance that a candidate on open pampa becomes a lone tree.
    const LONE_TREE: f32 = 0.0012;
    /// Extra chance inside a grove (scaled by the mask).
    const GROVE_TREE: f32 = 0.4;
    /// The part of a grove's chance that holds between its clumps: its
    /// trees gather in clumps with clearings between them.
    const CLUMP_FLOOR: f32 = 0.02;
    /// How much likelier a candidate is in the middle of a knot (see
    /// [`Self::knot`]) than outside every one: the trees seed their
    /// neighbours, so a stand gathers in clumps with gaps between.
    const KNOT_BOOST: f32 = 6.0;
    /// Chance that a candidate in the heart of a monte patch becomes a tree.
    const MONTE_TREE: f32 = 0.2;
    /// The part of the monte's chance that holds between its knots: the
    /// monte is dense throughout, with glades.
    const MONTE_FLOOR: f32 = 0.4;
    /// Cumulative share of the trees that are saplings, young, and mature
    /// (the rest are old giants), on open ground and in the heart of the
    /// monte: the monte is a young, regrowing stand.
    const AGE_SHARES: [f32; 3] = [0.07, 0.21, 0.965];
    const MONTE_AGE_SHARES: [f32; 3] = [0.22, 0.50, 0.97];

    /// The flora of `config`'s world over `terrain` (already leveled for its
    /// towns, with its roads laid), kept off the `line`'s track and the
    /// roads.
    pub fn new(config: WorldConfig, terrain: &Terrain, line: &RailLine) -> Self {
        Self {
            config,
            flat_areas: terrain.flat_areas.clone(),
            lagoons: terrain.lagoons.clone(),
            rail: line.segments().map(|s| (s.start, s.end)).collect(),
            roads: terrain.roads.clone(),
            fences: (Vec::new(), Vec::new()),
            settlements: None,
            worked: Vec::new(),
            redistribute: true,
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

    /// Whether a tuft, stone or shrub may grow at `p`: clear of every
    /// lagoon's water and the narrow wet mud along it
    /// ([`Lagoon::MUD_BAND`]). The reeds are drawn on their own.
    pub fn off_lagoons(&self, p: Vec2) -> bool {
        self.lagoons.iter().all(|l| !l.is_bare(p))
    }

    /// Whether a tree may stand at `p`: [`Self::TREE_LAGOON_MARGIN`] past
    /// every lagoon's waterline.
    fn tree_off_lagoons(&self, p: Vec2) -> bool {
        self.lagoons
            .iter()
            .all(|l| l.offshore(p) >= Self::TREE_LAGOON_MARGIN)
    }

    /// This flora, with trees kept off the surveyed lines of `fences`.
    pub fn with_fences(mut self, fences: &FenceLayout) -> Self {
        self.fences = fences.surveyed.clone();
        self
    }

    /// This flora, also kept off every settlement of `settlements`: its
    /// footprints, its patio and its tracks.
    pub fn with_settlements(mut self, settlements: &SettlementLayout) -> Self {
        self.settlements = Some(settlements.keep_out.clone());
        self.worked = settlements.settlements.iter().map(|s| s.hub).collect();
        self
    }

    /// Whether `p` is more than `margin` meters (at most
    /// [`KeepOut::MAX_MARGIN`]) from every settlement this flora keeps off
    /// (see [`Self::with_settlements`]): an indexed, cheap check the
    /// terrain's stones and shrubs share with the trees and tufts.
    pub fn off_settlements(&self, p: Vec2, margin: f32) -> bool {
        self.settlements
            .as_ref()
            .is_none_or(|k| k.is_clear(p, margin))
    }

    /// The world this flora grows in.
    pub fn config(&self) -> WorldConfig {
        self.config
    }

    /// The trees, tufts and shrubs of chunk `coord`, seeded by its
    /// coordinate.
    /// Chunks outside the world grow nothing.
    pub fn chunk(&self, coord: ChunkCoord) -> ChunkFlora {
        if !self.config.has_chunk(coord) {
            return ChunkFlora::default();
        }
        let rail = self.rail_near(coord);
        let trees = self.chunk_trees(coord, &rail);
        let tufts = self.chunk_tufts(coord, &rail, &trees);
        let shrubs = self.chunk_shrubs(coord, &rail, &trees);
        ChunkFlora {
            trees: self.thinned(trees),
            tufts,
            shrubs,
        }
    }

    /// The shrubs of chunk `coord` (cheaper than [`Self::chunk`]).
    pub fn shrubs(&self, coord: ChunkCoord) -> Vec<Shrub> {
        if !self.config.has_chunk(coord) {
            return Vec::new();
        }
        let rail = self.rail_near(coord);
        let trees = self.chunk_trees(coord, &rail);
        self.chunk_shrubs(coord, &rail, &trees)
    }

    /// The trees of chunk `coord` (cheaper than [`Self::chunk`]).
    pub fn trees(&self, coord: ChunkCoord) -> Vec<Tree> {
        if !self.config.has_chunk(coord) {
            return Vec::new();
        }
        self.thinned(self.chunk_trees(coord, &self.rail_near(coord)))
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

    /// Every shrub of the world, chunk by chunk: the same shrubs
    /// [`Self::chunk`] gives.
    pub fn all_shrubs(&self) -> Vec<Shrub> {
        let size = self.config.chunk_size;
        let bounds = self.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        let mut shrubs = Vec::new();
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                shrubs.extend(self.shrubs(ChunkCoord::new(x, y)));
            }
        }
        shrubs
    }

    /// Every tree and every shrub of the world, chunk by chunk: the same
    /// ones [`Self::all_trees`] and [`Self::all_shrubs`] give, each chunk's
    /// trees worked out once for both.
    pub fn all_plants(&self) -> (Vec<Tree>, Vec<Shrub>) {
        let size = self.config.chunk_size;
        let bounds = self.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        let (mut trees, mut shrubs) = (Vec::new(), Vec::new());
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let coord = ChunkCoord::new(x, y);
                let rail = self.rail_near(coord);
                let own = self.chunk_trees(coord, &rail);
                shrubs.extend(self.chunk_shrubs(coord, &rail, &own));
                trees.extend(self.thinned(own));
            }
        }
        (trees, shrubs)
    }

    /// How strongly the grove mask favors trees at `p`, in `[0, 1]`:
    /// low-frequency noise, so trees gather in small groves (isletas).
    pub fn grove(&self, p: Vec2) -> f32 {
        self.grove_wide(p, 0.0)
    }

    /// [`Self::grove`] with its edges `widen` lower, so the groves cover
    /// more ground (the far country, [`Self::rural`]).
    fn grove_wide(&self, p: Vec2, widen: f32) -> f32 {
        let n =
            (value_noise(self.seed, p, 170.0) + 0.3 * value_noise(self.seed ^ 0x51, p, 65.0)) / 1.3;
        smoothstep(0.56 - widen, 0.84 - widen, n)
    }

    /// How strongly trees gather at `p`, in `[0, 1]`: noise over a few
    /// tens of meters, so a grove breaks into clumps with clearings.
    pub fn clump(&self, p: Vec2) -> f32 {
        let n = (value_noise(self.seed ^ 0xc1a7, p, 44.0)
            + 0.5 * value_noise(self.seed ^ 0x9d, p, 20.0))
            / 1.5;
        smoothstep(0.5, 0.6, n)
    }

    /// How strongly trees seed their neighbours at `p`, in `[0, 1]`: noise
    /// over a few meters, so a stand gathers in knots of trees with gaps
    /// between. It is a field of the world, not of a chunk, so the knots
    /// run across the chunk borders.
    pub fn knot(&self, p: Vec2) -> f32 {
        let n = (value_noise(self.seed ^ 0x6b07, p, 17.0)
            + 0.6 * value_noise(self.seed ^ 0x3c1, p, 8.0))
            / 1.6;
        smoothstep(0.42, 0.62, n)
    }

    /// How much of a monte (a dense caldenal, bosque) `p` lies in, in
    /// `[0, 1]`: world-scale noise, so the patches are a few hundred meters
    /// across and run over the chunk borders. Between them the pampa is open
    /// grass with its scattered trees and groves.
    pub fn monte(&self, p: Vec2) -> f32 {
        self.monte_wide(p, 0.0)
    }

    /// [`Self::monte`] with its edges `widen` lower, so the patches cover
    /// more ground (the far country, [`Self::rural`]).
    fn monte_wide(&self, p: Vec2, widen: f32) -> f32 {
        let n = (value_noise(self.seed ^ 0x30e7, p, 480.0)
            + 0.35 * value_noise(self.seed ^ 0x30e8, p, 170.0))
            / 1.35;
        smoothstep(0.585 - widen, 0.685 - widen, n)
    }

    /// How dry the grass is at `p`, in `[0, 1]`: 0 is grey-green, 1 dry
    /// yellow. Low-frequency and continuous.
    pub fn dryness(&self, p: Vec2) -> f32 {
        dryness_of(self.seed, p)
    }

    /// The trees that stay out of `planted`.
    fn thinned(&self, mut planted: Vec<Tree>) -> Vec<Tree> {
        planted.retain(|t| self.kept(t.at));
        planted
    }

    /// How much of its monte, groves and lone trees the land at `p` keeps,
    /// by how far it lies from a town and from a casco (user, 2026-10-27:
    /// "cerca de los poblados no suele haber mucho monte, pero suele haber
    /// en el campo"). A smooth field of the world, so it runs over the chunk
    /// borders like the masks it scales: [`Self::TOWN_NEAR_SHARE`] within
    /// [`Self::TOWN_NEAR`] of a town's leveled ground, easing up to 1 at
    /// [`Self::TOWN_FULL`] and on to [`Self::FAR_BOOST`] at
    /// [`Self::FAR_FULL`] (the share the towns' surroundings lose goes to
    /// the open country); and [`Self::WORKED_SHARE`] at a settlement's
    /// hub, back to 1 at [`Self::WORKED_FULL`].
    pub fn rural(&self, p: Vec2) -> f32 {
        if !self.redistribute {
            return 1.0;
        }
        let town = self
            .flat_areas
            .iter()
            .map(|a| outside(a, p))
            .fold(f32::MAX, f32::min);
        let by_town = Self::TOWN_NEAR_SHARE
            + (1.0 - Self::TOWN_NEAR_SHARE) * smoothstep(Self::TOWN_NEAR, Self::TOWN_FULL, town)
            + (Self::FAR_BOOST - 1.0) * smoothstep(Self::TOWN_FULL, Self::FAR_FULL, town);
        let casco = self
            .worked
            .iter()
            .map(|h| h.distance(p))
            .fold(f32::MAX, f32::min);
        let by_casco = Self::WORKED_SHARE
            + (1.0 - Self::WORKED_SHARE) * smoothstep(Self::WORKED_NEAR, Self::WORKED_FULL, casco);
        by_town * by_casco
    }

    /// Whether the tree planted at `at` stays: [`Self::TREE_KEEP`] of them
    /// do, chosen by the spot alone, so the same trees go whichever chunk
    /// asks and the thinning is even over the monte, the groves and the
    /// lone trees alike. A thinned tree still holds its ground while the
    /// stand is planted (its neighbours keep their distance), so the stand
    /// keeps its clumps and clearings and only has gaps in it.
    fn kept(&self, at: Vec2) -> bool {
        Rng::new(self.seed ^ at.x.to_bits(), at.y.to_bits()).unit() < Self::TREE_KEEP
    }

    /// The trees of chunk `coord` before the thinning, spaced apart within
    /// it and from the trees of its neighbours: where a tree stands too close to one of an
    /// earlier neighbour (the row below, or the chunk to the west), the
    /// later chunk's tree gives way. Each chunk is still decided on its own
    /// (the same trees every time, in any order), from the neighbours'
    /// trees before they gave way to theirs.
    ///
    /// These are what the shrubs and tufts keep clear of, so they stay as
    /// they were when the trees were thinned.
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
            let (pick, species_roll, age_roll, height_roll) =
                (rng.unit(), rng.unit(), rng.unit(), rng.unit());
            let (yaw, width_roll) = (rng.unit(), rng.unit());
            let variant = rng.next();
            if trees.len() >= Self::TREE_BUDGET {
                break;
            }
            // Thin near the towns and the cascos (the chance scales down),
            // wider far out (the patches and groves spread: the stands are
            // as dense as they can be already, so more of them, not
            // denser); the lone trees keep a little more than the stands.
            let rural = self.rural(at);
            let widen = Self::FAR_WIDEN * (rural - 1.0).max(0.0);
            let grove = self.grove_wide(at, widen);
            let monte = self.monte_wide(at, widen);
            // The grove's chance gathers in clumps, and trees seed their
            // neighbours in knots; the monte is dense throughout, with
            // glades. Both are fields of the world, so a clump runs on over
            // a chunk border.
            let knot = self.knot(at);
            let gather = Self::CLUMP_FLOOR + (1.0 - Self::CLUMP_FLOOR) * self.clump(at);
            let seeding = 1.0 + (Self::KNOT_BOOST - 1.0) * knot;
            let grove_chance = Self::GROVE_TREE * grove * gather * seeding;
            let monte_chance =
                Self::MONTE_TREE * monte * (Self::MONTE_FLOOR + (1.0 - Self::MONTE_FLOOR) * knot);
            let thin = rural.min(1.0);
            let (grove_chance, monte_chance) = (grove_chance * thin, monte_chance * thin);
            let chance = Self::LONE_TREE * rural.sqrt() + grove_chance.max(monte_chance);
            if pick >= chance {
                continue;
            }
            // A grove stops at its budget; only the monte goes on past it.
            if trees.len() >= Self::GROVE_BUDGET && monte_chance <= grove_chance {
                continue;
            }
            // Caldenes rule the groves' cores; chañares fill their edges and
            // the open ground, and the understory of the monte.
            let edge = 4.0 * grove * (1.0 - grove);
            let calden = (0.3 + 0.45 * grove - 0.2 * edge + 0.3 * monte).clamp(0.1, 0.9);
            let species = if species_roll < calden {
                Species::Calden
            } else {
                Species::Chanar
            };
            let shares = [0, 1, 2].map(|i| {
                Self::AGE_SHARES[i] + (Self::MONTE_AGE_SHARES[i] - Self::AGE_SHARES[i]) * monte
            });
            let age = match shares.iter().position(|&c| age_roll < c) {
                Some(0) => Age::Sapling,
                Some(1) => Age::Young,
                Some(_) => Age::Mature,
                None => Age::Old,
            };
            let (lo, hi) = age.height_range(species);
            // An old giant stands broad; the rest a little narrow or wide.
            let width = match age {
                Age::Old => 1.15 + 0.25 * width_roll,
                _ => 0.82 + 0.36 * width_roll,
            };
            let tree = Tree {
                at,
                species,
                age,
                height: lo + (hi - lo) * height_roll,
                width,
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
                && self.off_lagoons(at)
                && rail_distance(rail, at) >= Self::TUFT_RAIL_MARGIN
                && self.off_road(at, Self::TUFT_ROAD_MARGIN)
                && self.off_settlements(at, Self::TUFT_SETTLEMENT_MARGIN)
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

    /// How strongly shrubs gather at `p`, in `[0, 1]`: noise over a few
    /// tens of meters, so the brush grows in thickets with open ground
    /// between.
    fn thicket(&self, p: Vec2) -> f32 {
        let n = (value_noise(self.seed ^ 0x7a1c, p, 30.0)
            + 0.5 * value_noise(self.seed ^ 0x7a1d, p, 11.0))
            / 1.5;
        smoothstep(0.38, 0.62, n)
    }

    /// How much `p` lies on the fringe of a grove or of a monte, in
    /// `[0, 1]`: 1 half way out of one, 0 in the open or at a core.
    pub fn fringe(&self, p: Vec2) -> f32 {
        Self::fringe_of(self.grove(p), self.monte(p))
    }

    /// [`Self::fringe`] from the grove and monte masks already worked out.
    fn fringe_of(grove: f32, monte: f32) -> f32 {
        (4.0 * grove * (1.0 - grove)).max(4.0 * monte * (1.0 - monte))
    }

    /// The share of each shrub kind (piquillín, jarilla, fachinal) at `p`,
    /// summing to 1: piquillín makes the monte's understory, jarilla holds
    /// the open dry ground, and the fachinal's chañar suckers fringe the
    /// groves and the monte.
    pub fn shrub_shares(&self, p: Vec2) -> [f32; 3] {
        self.shares_of(p, self.monte(p), self.fringe(p))
    }

    /// [`Self::shrub_shares`] from the monte and fringe masks already
    /// worked out.
    fn shares_of(&self, p: Vec2, monte: f32, fringe: f32) -> [f32; 3] {
        let dry = self.dryness(p);
        let weights = [
            0.3 + 0.55 * monte,
            (1.0 - monte) * (0.15 + 0.7 * dry) * (1.0 - 0.6 * fringe),
            0.1 + 0.9 * fringe * (1.0 - 0.5 * monte),
        ];
        let total: f32 = weights.iter().sum();
        weights.map(|w| w / total)
    }

    /// The shrubs of chunk `coord`, given its `trees`: densest in the monte
    /// and along the fringes of groves and monte, sparse in the open. Kept
    /// off everything the trees keep off (a little closer), off the trunks
    /// of the trees (those of the neighbouring chunks too) and apart from
    /// each other within the chunk.
    fn chunk_shrubs(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)], trees: &[Tree]) -> Vec<Shrub> {
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 2);
        let candidates = self.scaled(Self::SHRUB_CANDIDATES);
        let mut shrubs: Vec<Shrub> = Vec::new();
        // The trees of the chunks around, fetched when a shrub stands near
        // the border they share.
        let mut neighbours: Vec<((i32, i32), Vec<Tree>)> = Vec::new();
        const BORDER: f32 = FloraSite::SHRUB_BORDER;
        for _ in 0..candidates {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            let (pick, kind_roll, height_roll, width_roll, yaw) =
                (rng.unit(), rng.unit(), rng.unit(), rng.unit(), rng.unit());
            let variant = rng.next();
            if shrubs.len() >= Self::SHRUB_BUDGET {
                break;
            }
            let monte = self.monte(at);
            let fringe = Self::fringe_of(self.grove(at), monte);
            let chance = (0.05 + 0.64 * monte + 0.5 * fringe) * (0.3 + 0.7 * self.thicket(at));
            if pick >= chance.min(0.9) || !self.shrub_room(at, rail) {
                continue;
            }
            let [piquillin, jarilla, _] = self.shares_of(at, monte, fringe);
            let kind = if kind_roll < piquillin {
                ShrubKind::Piquillin
            } else if kind_roll < piquillin + jarilla {
                ShrubKind::Jarilla
            } else {
                ShrubKind::Fachinal
            };
            let (lo, hi) = kind.height_range();
            let shrub = Shrub {
                at,
                kind,
                height: lo + (hi - lo) * height_roll,
                width: 0.85 + 0.35 * width_roll,
                yaw: yaw * std::f32::consts::TAU,
                variant,
            };
            // Cheap first: other shrubs, which are few near the candidate.
            if shrubs
                .iter()
                .any(|o| o.at.distance(at) < Self::SHRUB_SPACING)
            {
                continue;
            }
            let clear_of = |t: &Tree| {
                let reach = t.trunk_radius() + shrub.trunk_radius() + Self::SHRUB_TRUNK_GAP;
                let d = t.at - at;
                d.x * d.x + d.y * d.y >= reach * reach
            };
            let sides = |v: f32, lo: f32| {
                if v - lo < BORDER {
                    -1
                } else if lo + size - v < BORDER {
                    1
                } else {
                    0
                }
            };
            let (sx, sy) = (sides(at.x, min.x), sides(at.y, min.y));
            let mut clear = trees.iter().all(clear_of);
            for (dx, dy) in [(sx, 0), (0, sy), (sx, sy)] {
                if !clear || (dx, dy) == (0, 0) {
                    continue;
                }
                let key = (coord.x + dx, coord.y + dy);
                let at_index = match neighbours.iter().position(|(k, _)| *k == key) {
                    Some(i) => i,
                    None => {
                        // Before the neighbour's trees give way to its own
                        // neighbours': a superset of its trees, so a shrub
                        // keeps clear of every one, and a lot cheaper.
                        let other = ChunkCoord::new(key.0, key.1);
                        let trees = if self.config.has_chunk(other) {
                            self.scattered_trees(other, &self.rail_near(other))
                        } else {
                            Vec::new()
                        };
                        neighbours.push((key, trees));
                        neighbours.len() - 1
                    }
                };
                clear = neighbours[at_index].1.iter().all(clear_of);
            }
            if clear {
                shrubs.push(shrub);
            }
        }
        shrubs
    }

    /// Whether a shrub may stand at `p`: inside the world, clear of every
    /// town's band, of the track, of the roads, of the fence lines, of the
    /// lagoons and of the settlements.
    fn shrub_room(&self, p: Vec2, rail: &[(Vec2, Vec2)]) -> bool {
        let bounds = self.config.bounds();
        let m = Self::SHRUB_EDGE_MARGIN;
        let inside = p.x - bounds.min.x >= m
            && bounds.max.x - p.x >= m
            && p.y - bounds.min.y >= m
            && bounds.max.y - p.y >= m;
        inside
            && self
                .flat_areas
                .iter()
                .all(|a| outside(a, p) >= a.falloff + Self::SHRUB_TOWN_MARGIN)
            && self
                .lagoons
                .iter()
                .all(|l| l.offshore(p) >= Self::SHRUB_LAGOON_MARGIN)
            && rail_distance(rail, p) >= Self::SHRUB_RAIL_MARGIN
            && self.off_road(p, Self::SHRUB_ROAD_MARGIN)
            && self.off_fences(p)
            && self.off_settlements(p, Self::SHRUB_SETTLEMENT_MARGIN)
    }

    /// Whether a tree may stand at `p`: inside the world, clear of every
    /// town's band, of the track, of the roads, of the fence lines and of
    /// the settlements.
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
            && self.tree_off_lagoons(p)
            && rail_distance(rail, p) >= Self::TREE_RAIL_MARGIN
            && self.off_road(p, Self::TREE_ROAD_MARGIN)
            && self.off_fences(p)
            && self.off_settlements(p, Self::TREE_SETTLEMENT_MARGIN)
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

impl Age {
    /// Every age, youngest first.
    pub const ALL: [Age; 4] = [Self::Sapling, Self::Young, Self::Mature, Self::Old];

    /// How much thicker than its height says the trunk of a caldén grows:
    /// a sapling's stem and a mature trunk are stouter by a tenth or so, an
    /// old giant's by four tenths.
    pub fn girth(self) -> f32 {
        match self {
            Self::Sapling => 1.3,
            Self::Young | Self::Mature => 1.1,
            Self::Old => 1.4,
        }
    }

    /// Lowest and highest height of a `species` of this age, in meters.
    pub fn height_range(self, species: Species) -> (f32, f32) {
        match (species, self) {
            (Species::Calden, Self::Sapling) => (2.0, 3.5),
            (Species::Calden, Self::Young) => (3.5, 6.0),
            (Species::Calden, Self::Mature) => (6.0, 9.0),
            (Species::Calden, Self::Old) => (10.0, 12.0),
            (Species::Chanar, Self::Sapling) => (1.5, 2.8),
            (Species::Chanar, Self::Young) => (2.8, 3.8),
            (Species::Chanar, Self::Mature) => (3.8, 5.0),
            (Species::Chanar, Self::Old) => (5.0, 6.5),
        }
    }
}

impl Species {
    /// Lowest and highest tree of any age, in meters.
    pub fn height_range(self) -> (f32, f32) {
        let (lo, _) = Age::Sapling.height_range(self);
        let (_, hi) = Age::Old.height_range(self);
        (lo, hi)
    }
}

impl ShrubKind {
    /// Every kind of shrub.
    pub const ALL: [ShrubKind; 3] = [Self::Piquillin, Self::Jarilla, Self::Fachinal];

    /// Lowest and highest shrub of this kind, in meters: the piquillín
    /// grows 0.8-2 m, up to 3 m; the jarilla 1-3 m; the fachinal's suckers a
    /// little higher than the chañar's saplings.
    pub fn height_range(self) -> (f32, f32) {
        match self {
            Self::Piquillin => (1.0, 3.0),
            Self::Jarilla => (1.0, 2.5),
            Self::Fachinal => (1.8, 3.2),
        }
    }
}

impl Shrub {
    /// The dense stem core at the ground, in meters.
    /// Thorny piquillín and a thicket's suckers are stouter than the
    /// jarilla's slim stems.
    pub fn trunk_radius(&self) -> f32 {
        self.height
            * match self.kind {
                ShrubKind::Piquillin => 0.1,
                ShrubKind::Jarilla => 0.07,
                ShrubKind::Fachinal => 0.09,
            }
    }

    /// Radius of the shrub seen from above, in meters.
    pub fn crown_radius(&self) -> f32 {
        let base = match self.kind {
            ShrubKind::Piquillin => 0.5,
            ShrubKind::Jarilla => 0.35,
            ShrubKind::Fachinal => 0.4,
        };
        base * self.width * self.height
    }

    /// The stem core's footprint, a square as wide as the core. The world
    /// does not plant it: shrubs never block.
    pub fn obstacle(&self) -> Aabb {
        let side = 2.0 * self.trunk_radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
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
            Species::Calden => 0.035 * self.age.girth() * self.height,
            Species::Chanar => 0.09 * self.height,
        }
    }

    /// Radius of the crown seen from above, in meters.
    pub fn crown_radius(&self) -> f32 {
        let base = match self.species {
            Species::Calden => 0.55,
            Species::Chanar => 0.35,
        };
        base * self.width * self.height
    }

    /// The trunk's footprint, a square as wide as the trunk: a static
    /// obstacle.
    pub fn obstacle(&self) -> Aabb {
        let side = 2.0 * self.trunk_radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
    }

    /// The largest [`Self::spacing`] of any tree, in meters.
    const MAX_SPACING: f32 = 8.0;

    /// Least distance to another tree's trunk, in meters: a bigger tree
    /// takes more room, a sapling can stand in the shade of an old one.
    fn spacing(&self) -> f32 {
        match self.species {
            Species::Calden => 1.6 + 0.3 * self.height,
            Species::Chanar => 1.4 + 0.25 * self.height,
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
        // The stands as they are in themselves, not scaled by the distance
        // to the towns (see `redistributed`).
        let mut flora = FloraSite::new(config, &world.terrain, &line);
        flora.redistribute = false;
        (world, line, flora)
    }

    /// [`site`]'s flora with the monte scaled by the distance to the towns.
    fn redistributed() -> (World, RailLine, FloraSite) {
        let (world, line, mut flora) = site();
        flora.redistribute = true;
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

    /// Every tree of a broad block between the towns and of a grove-rich
    /// block to the south-west.
    fn sample(flora: &FloraSite) -> Vec<Tree> {
        between()
            .into_iter()
            .chain((-19..1).flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y))))
            .flat_map(|c| flora.trees(c))
            .collect()
    }

    #[test]
    fn trees_have_their_species_sizes() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let of = |s: Species| trees.iter().filter(|t| t.species == s).collect::<Vec<_>>();
        let (calden, chanar) = (of(Species::Calden), of(Species::Chanar));
        assert!(
            calden.len() > 20 && chanar.len() > 20,
            "{} {}",
            calden.len(),
            chanar.len()
        );
        for t in &trees {
            // Each tree's height belongs to its age class.
            let (lo, hi) = t.age.height_range(t.species);
            assert!((lo..=hi).contains(&t.height), "{t:?}");
            let (lo, hi) = t.species.height_range();
            assert!((lo..=hi).contains(&t.height), "{t:?}");
        }
        // The mature trees keep the documented sizes.
        let mature = |ts: &[&Tree], lo: f32, hi: f32| {
            ts.iter()
                .filter(|t| t.age == Age::Mature)
                .all(|t| (lo..=hi).contains(&t.height))
        };
        assert!(mature(&calden, 6.0, 9.0));
        assert!(mature(&chanar, 3.8, 5.0));
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
                .all(|t| t.trunk_radius() > 0.05 && t.trunk_radius() < 0.7)
        );
    }

    #[test]
    fn trees_come_in_age_classes_and_their_collision_grows_with_them() {
        let (_, _, flora) = site();
        // Off the monte, which grows a younger stand (see
        // `the_monte_is_a_young_stand_with_a_chanar_understory`).
        let trees: Vec<Tree> = sample(&flora)
            .into_iter()
            .filter(|t| flora.monte(t.at) < 0.02)
            .collect();
        for species in [Species::Calden, Species::Chanar] {
            let own: Vec<&Tree> = trees.iter().filter(|t| t.species == species).collect();
            let share =
                |age: Age| own.iter().filter(|t| t.age == age).count() as f32 / own.len() as f32;
            println!(
                "{species:?}: {:.3} sapling {:.3} young {:.3} mature {:.3} old",
                share(Age::Sapling),
                share(Age::Young),
                share(Age::Mature),
                share(Age::Old)
            );
            assert!((0.04..=0.09).contains(&share(Age::Sapling)), "{species:?}");
            assert!((0.10..=0.20).contains(&share(Age::Young)), "{species:?}");
            assert!((0.70..=0.85).contains(&share(Age::Mature)), "{species:?}");
            // Old giants are rare, but there are some.
            assert!((0.025..=0.04).contains(&share(Age::Old)), "{species:?}");
            // A bigger class stands taller, is wider-trunked and takes more
            // room, on average and without overlap of the extremes.
            let mean = |age: Age, f: fn(&Tree) -> f32| {
                let v: Vec<f32> = own.iter().filter(|t| t.age == age).map(|t| f(t)).collect();
                v.iter().sum::<f32>() / v.len() as f32
            };
            for pair in Age::ALL.windows(2) {
                for f in [
                    (|t| t.height) as fn(&Tree) -> f32,
                    |t| t.trunk_radius(),
                    |t| t.spacing(),
                ] {
                    assert!(mean(pair[0], f) < mean(pair[1], f), "{species:?} {pair:?}");
                }
            }
        }
        // A caldén giant is 10-12 m with a trunk thicker than any mature
        // tree's and a crown wider than any mature tree's.
        let calden = |age: Age| {
            trees
                .iter()
                .filter(move |t| t.species == Species::Calden && t.age == age)
        };
        let max = |age: Age, f: fn(&Tree) -> f32| calden(age).map(f).fold(0.0, f32::max);
        let min = |age: Age, f: fn(&Tree) -> f32| calden(age).map(f).fold(f32::MAX, f32::min);
        assert!(min(Age::Old, |t| t.height) >= 10.0);
        assert!(max(Age::Old, |t| t.height) <= 12.0);
        assert!(
            min(Age::Old, |t| t.trunk_radius()) > 1.15 * max(Age::Mature, |t| t.trunk_radius())
        );
        assert!(min(Age::Old, |t| t.crown_radius()) > max(Age::Mature, |t| t.crown_radius()));
        assert!(max(Age::Sapling, |t| t.height) <= 3.5);
        // Each tree's width is its own, modest, and the giants stand broad.
        for t in &trees {
            assert!((0.8..=1.4).contains(&t.width), "{t:?}");
        }
        let widths: std::collections::HashSet<u32> =
            trees.iter().map(|t| t.width.to_bits()).collect();
        assert!(widths.len() > trees.len() / 2, "{}", widths.len());
        // Every tree still has its own yaw and seed.
        let looks: std::collections::HashSet<u32> = trees.iter().map(|t| t.variant).collect();
        assert!(looks.len() > trees.len() * 9 / 10);
    }

    /// Distance from each tree whose grove mask is over `grove` to its
    /// nearest neighbour.
    fn grove_neighbour_gaps(flora: &FloraSite, trees: &[Tree], grove: f32) -> Vec<f32> {
        trees
            .iter()
            .enumerate()
            .filter(|(_, a)| flora.grove(a.at) > grove)
            .map(|(i, a)| {
                trees
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, b)| b.at.distance(a.at))
                    .fold(f32::INFINITY, f32::min)
            })
            .collect()
    }

    #[test]
    fn groves_are_clumps_with_gaps_not_an_even_stand() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let gaps = grove_neighbour_gaps(&flora, &trees, 0.6);
        let n = gaps.len() as f32;
        assert!(n > 1000.0, "{n}");
        let mean = gaps.iter().sum::<f32>() / n;
        let sd = (gaps.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / n).sqrt();
        let close = gaps.iter().filter(|&&d| d < 6.0).count() as f32 / n;
        println!(
            "grove gaps: mean {mean:.1} m, cv {:.2}, {close:.2} close",
            sd / mean
        );
        // Counts in 24 m quadrats over the groves: a random scatter has a
        // variance equal to its mean (an index of 1), an even one far less.
        let mut counts = Vec::new();
        for qx in -158..8 {
            for qy in 25..158 {
                let (x, y) = (qx as f32 * 24.0, qy as f32 * 24.0);
                if flora.grove(Vec2::new(x + 12.0, y + 12.0)) > 0.6 {
                    let n = trees
                        .iter()
                        .filter(|t| {
                            (x..x + 24.0).contains(&t.at.x) && (y..y + 24.0).contains(&t.at.y)
                        })
                        .count();
                    counts.push(n as f32);
                }
            }
        }
        let q = counts.len() as f32;
        let qmean = counts.iter().sum::<f32>() / q;
        let qvar = counts.iter().map(|c| (c - qmean).powi(2)).sum::<f32>() / q;
        println!(
            "quadrats: {q} of mean {qmean:.2}, dispersion {:.2}",
            qvar / qmean
        );
        assert!(q > 100.0, "{q}");
        // The even groves had 0.97; clumps with gaps are well over it.
        assert!(qvar / qmean > 1.6, "dispersion {}", qvar / qmean);
        // The nearest-neighbour spread (cv) of an even stand sits under a
        // random one's 0.52 (the even groves had 0.43).
        assert!(sd / mean > 0.5, "cv {}", sd / mean);
        // Many trees stand close.
        assert!(close > 0.3, "{close}");
    }

    #[test]
    fn chanares_are_a_bigger_share_where_the_groves_thin_out() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let edge: Vec<&Tree> = trees
            .iter()
            .filter(|t| (0.15..0.85).contains(&flora.grove(t.at)))
            .collect();
        let core: Vec<&Tree> = trees.iter().filter(|t| flora.grove(t.at) >= 0.85).collect();
        let share = |ts: &[&Tree]| {
            ts.iter().filter(|t| t.species == Species::Chanar).count() as f32 / ts.len() as f32
        };
        println!(
            "chañar share: edge {:.2} of {}, core {:.2} of {}",
            share(&edge),
            edge.len(),
            share(&core),
            core.len()
        );
        assert!(edge.len() > 500 && core.len() > 500);
        // Old: about 0.48 at the edges. Chañares fill the edges, and the
        // caldenes keep the cores.
        assert!(share(&edge) > 0.6, "{}", share(&edge));
        assert!(share(&edge) > share(&core) + 0.2);
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

    /// Pearson correlation of two equally long series.
    fn correlation(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len() as f32;
        let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
        let cov: f32 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
        let va: f32 = a.iter().map(|x| (x - ma).powi(2)).sum();
        let vb: f32 = b.iter().map(|y| (y - mb).powi(2)).sum();
        cov / (va * vb).sqrt()
    }

    #[test]
    fn clumping_is_continuous_across_chunk_borders() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let size = WorldConfig::default().chunk_size;
        // Trees in the 12 m wide, 20 m long strip from `x` along `y`.
        let strip = |x: f32, y: f32| {
            trees
                .iter()
                .filter(|t| (x..x + 12.0).contains(&t.at.x) && (y..y + 20.0).contains(&t.at.y))
                .count() as f32
        };
        // Down every vertical chunk border of the grove-rich block, pair
        // the strips either side of it, and as a control two neighbouring
        // strips well inside one chunk.
        let (mut across, mut across_to, mut within, mut within_to) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for k in -18..0 {
            let seam = k as f32 * size;
            for j in 15..95 {
                let y = j as f32 * 20.0;
                across.push(strip(seam - 12.0, y));
                across_to.push(strip(seam, y));
                within.push(strip(seam + 38.0, y));
                within_to.push(strip(seam + 50.0, y));
            }
        }
        // The fields themselves run on through a border: no jump between
        // two points a tenth of a meter apart either side of one.
        for k in -18..0 {
            for j in 0..200 {
                let p = Vec2::new(k as f32 * size, -1900.0 + j as f32 * 19.3);
                let q = p - Vec2::new(0.1, 0.0);
                for (a, b) in [
                    (flora.knot(p), flora.knot(q)),
                    (flora.clump(p), flora.clump(q)),
                    (flora.grove(p), flora.grove(q)),
                    (flora.monte(p), flora.monte(q)),
                ] {
                    assert!((a - b).abs() < 0.1, "{a} {b} at {p:?}");
                }
            }
        }
        let cross = correlation(&across, &across_to);
        let inside = correlation(&within, &within_to);
        println!("strip correlation: across a border {cross:.2}, within a chunk {inside:.2}");
        assert!(inside > 0.3, "no clumping to speak of: {inside}");
        // A border does not cut the clumps: the two sides agree about as well
        // as two strips of one chunk do.
        assert!(cross > 0.85 * inside, "across {cross}, within {inside}");
    }

    /// Every chunk of the default world.
    fn world_chunks() -> Vec<ChunkCoord> {
        (-20..20)
            .flat_map(|x| (-20..20).map(move |y| ChunkCoord::new(x, y)))
            .collect()
    }

    /// The lowest, the mean and the highest of `field` over a 5 by 5 grid in
    /// chunk `c`.
    fn chunk_range(c: ChunkCoord, field: impl Fn(Vec2) -> f32) -> (f32, f32, f32) {
        let b = c.bounds(WorldConfig::default().chunk_size);
        let step = (b.max - b.min) * 0.25;
        let all: Vec<f32> = (0..25)
            .map(|k| field(b.min + Vec2::new((k % 5) as f32 * step.x, (k / 5) as f32 * step.y)))
            .collect();
        (
            all.iter().copied().fold(f32::MAX, f32::min),
            all.iter().sum::<f32>() / 25.0,
            all.iter().copied().fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn monte_patches_are_world_scale_smooth_and_a_share_of_the_pampa() {
        let (_, _, flora) = site();
        for k in 0..500 {
            let p = Vec2::new(k as f32 * 7.3 - 1800.0, k as f32 * -3.1 + 400.0);
            let m = flora.monte(p);
            assert!((0.0..=1.0).contains(&m));
            // Continuous across any chunk border too.
            assert!((m - flora.monte(p + Vec2::new(1.0, 0.0))).abs() < 0.05);
            assert!((m - flora.monte(p + Vec2::new(0.0, 1.0))).abs() < 0.05);
        }
        // Walk the world's rows every 100 m, sampling every 20 m.
        let (mut inside, mut total) = (0, 0);
        let mut runs: Vec<f32> = Vec::new();
        for row in -19..19 {
            let mut run = 0;
            for col in -100..100 {
                let p = Vec2::new(col as f32 * 20.0, row as f32 * 100.0 + 50.0);
                total += 1;
                if flora.monte(p) > 0.5 {
                    inside += 1;
                    run += 1;
                } else if run > 0 {
                    runs.push(run as f32 * 20.0);
                    run = 0;
                }
            }
            if run > 0 {
                runs.push(run as f32 * 20.0);
            }
        }
        let share = inside as f32 / total as f32;
        let mean = runs.iter().sum::<f32>() / runs.len() as f32;
        let widest = runs.iter().copied().fold(0.0, f32::max);
        println!(
            "monte: {share:.3} of the world, {} crossings of mean {mean:.0} m, widest {widest:.0} m",
            runs.len()
        );
        // A good part of the pampa, but not most of it.
        assert!((0.21..0.25).contains(&share), "{share}");
        // Patches a few hundred meters across, the biggest a good deal more.
        assert!((150.0..600.0).contains(&mean), "{mean}");
        assert!(widest >= 400.0, "{widest}");
    }

    #[test]
    fn the_monte_is_far_denser_than_the_open_pampa() {
        let (_, _, flora) = site();
        let (mut inside, mut groves, mut open) = (Vec::new(), Vec::new(), Vec::new());
        let (mut peak, mut grove_peak) = (0.0_f32, 0.0_f32);
        for c in world_chunks() {
            let (monte, grove) = (
                chunk_range(c, |p| flora.monte(p)),
                chunk_range(c, |p| flora.grove(p)),
            );
            let n = flora.trees(c).len() as f32;
            if monte.1 > 0.8 {
                peak = peak.max(n);
                // Where a grove does not add to it.
                if grove.2 < 0.3 {
                    inside.push(n);
                }
            } else if monte.2 < 0.02 && grove.1 > 0.6 {
                groves.push(n);
                grove_peak = grove_peak.max(n);
            } else if monte.2 < 0.02 && grove.2 < 0.02 {
                open.push(n);
            }
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        println!(
            "trees per chunk: monte {:.1} of {} chunks (up to {peak}), groves {:.1} of {}, \
             open {:.1} of {}",
            mean(&inside),
            inside.len(),
            mean(&groves),
            groves.len(),
            mean(&open),
            open.len()
        );
        assert!(inside.len() >= 10 && groves.len() >= 10 && open.len() >= 50);
        // Dense, within the per-chunk budget, and many times the open pampa's
        // few scattered trees (the groves, a smaller thing, already fill the
        // budget). The counts are the trees that stay after the thinning
        // ([`FloraSite::TREE_KEEP`]), so the budgets are caps of the planted
        // trees and are not reached.
        assert!(mean(&inside) >= 40.0, "{}", mean(&inside));
        assert!(peak <= FloraSite::TREE_BUDGET as f32);
        // The groves keep their old budget; only the monte goes past what a
        // grove keeps (the budget thinned).
        assert!(grove_peak <= FloraSite::GROVE_BUDGET as f32, "{grove_peak}");
        let grove_kept = FloraSite::GROVE_BUDGET as f32 * FloraSite::TREE_KEEP;
        assert!(peak > grove_kept, "{peak}");
        assert!(mean(&inside) >= 6.0 * mean(&open));
        // Without clearings: even the lightest monte chunk is a stand.
        let lightest = inside.iter().copied().fold(f32::MAX, f32::min);
        assert!(lightest >= 20.0, "{lightest}");
    }

    #[test]
    fn the_monte_is_a_young_stand_with_a_chanar_understory() {
        let (_, _, flora) = site();
        let trees: Vec<Tree> = world_chunks()
            .into_iter()
            .flat_map(|c| flora.trees(c))
            .collect();
        let zone = |lo: f32, hi: f32| -> Vec<&Tree> {
            trees
                .iter()
                .filter(|t| (lo..=hi).contains(&flora.monte(t.at)))
                .collect()
        };
        let (heart, open) = (zone(0.9, 1.0), zone(0.0, 0.01));
        let share = |ts: &[&Tree], f: fn(&Tree) -> bool| {
            ts.iter().filter(|t| f(t)).count() as f32 / ts.len() as f32
        };
        let young = |t: &Tree| t.age <= Age::Young;
        let chanar = |t: &Tree| t.species == Species::Chanar;
        let giant = |t: &Tree| t.age == Age::Old;
        println!(
            "monte {} trees: {:.2} sapling or young, {:.2} chañar, {:.3} giants; open {}: {:.2}, {:.2}",
            heart.len(),
            share(&heart, young),
            share(&heart, chanar),
            share(&heart, giant),
            open.len(),
            share(&open, young),
            share(&open, chanar)
        );
        assert!(heart.len() > 1000 && open.len() > 500);
        // Mixed ages: many saplings and young trees, but mature ones and the
        // odd old giant too.
        assert!(share(&heart, young) > 0.45, "{}", share(&heart, young));
        assert!(share(&heart, young) > share(&open, young) + 0.2);
        assert!(share(&heart, |t| t.age == Age::Sapling) > 0.2);
        assert!(share(&heart, |t| t.age == Age::Mature) > 0.42);
        assert!(share(&heart, giant) > 0.02);
        // Caldenes make the canopy and chañares the understory.
        assert!((0.2..0.5).contains(&share(&heart, chanar)));
    }

    /// Every shrub of the default world, with its chunk.
    fn world_shrubs(flora: &FloraSite) -> Vec<(ChunkCoord, Shrub)> {
        world_chunks()
            .into_iter()
            .flat_map(|c| flora.shrubs(c).into_iter().map(move |s| (c, s)))
            .collect()
    }

    #[test]
    fn shrubs_are_deterministic_per_chunk_and_stay_in_budget() {
        let (_, _, flora) = site();
        let size = WorldConfig::default().chunk_size;
        let mut most = 0;
        for c in world_chunks().into_iter().step_by(3) {
            let own = flora.shrubs(c);
            assert_eq!(own, flora.shrubs(c));
            assert_eq!(own, flora.chunk(c).shrubs);
            assert!(own.len() <= FloraSite::SHRUB_BUDGET);
            let bounds = c.bounds(size);
            assert!(own.iter().all(|s| bounds.contains_point(s.at)));
            most = most.max(own.len());
        }
        // The thickest chunks reach the budget; neighbours differ.
        assert!(most >= 100, "{most}");
        assert_ne!(
            flora.shrubs(ChunkCoord::new(5, -4)),
            flora.shrubs(ChunkCoord::new(6, -4))
        );
        assert_eq!(flora.shrubs(ChunkCoord::new(-21, 0)), Vec::new());
    }

    #[test]
    fn shrubs_are_densest_in_the_monte_and_on_the_fringes_and_sparse_in_the_open() {
        let (_, _, flora) = site();
        let (mut monte, mut fringe, mut open) = (Vec::new(), Vec::new(), Vec::new());
        for c in world_chunks() {
            let m = chunk_range(c, |p| flora.monte(p));
            let f = chunk_range(c, |p| flora.fringe(p));
            let g = chunk_range(c, |p| flora.grove(p));
            let n = flora.shrubs(c).len() as f32;
            if m.1 > 0.8 {
                monte.push(n);
            } else if f.1 > 0.45 && m.1 < 0.5 {
                fringe.push(n);
            } else if m.2 < 0.02 && g.2 < 0.02 {
                open.push(n);
            }
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        println!(
            "shrubs per chunk: monte {:.1} of {}, fringe {:.1} of {}, open {:.1} of {}",
            mean(&monte),
            monte.len(),
            mean(&fringe),
            fringe.len(),
            mean(&open),
            open.len()
        );
        assert!(monte.len() >= 15 && fringe.len() >= 15 && open.len() >= 50);
        assert!(mean(&monte) >= 40.0, "{}", mean(&monte));
        assert!(mean(&monte) >= 10.0 * mean(&open));
        assert!(mean(&fringe) >= 3.0 * mean(&open));
        // The open pampa is not bare of brush: a lone shrub here and there.
        assert!(mean(&open) > 1.0, "{}", mean(&open));
    }

    #[test]
    fn each_zone_has_its_shrub_species() {
        let (_, _, flora) = site();
        let shrubs = world_shrubs(&flora);
        let zone = |f: &dyn Fn(Vec2) -> bool| -> [f32; 3] {
            let own: Vec<&Shrub> = shrubs.iter().map(|(_, s)| s).filter(|s| f(s.at)).collect();
            assert!(own.len() > 1000, "{}", own.len());
            ShrubKind::ALL
                .map(|k| own.iter().filter(|s| s.kind == k).count() as f32 / own.len() as f32)
        };
        let monte = zone(&|p| flora.monte(p) > 0.9);
        let fringe = zone(&|p| flora.fringe(p) > 0.8 && flora.monte(p) < 0.5);
        let open =
            zone(&|p| flora.monte(p) < 0.01 && flora.grove(p) < 0.02 && flora.dryness(p) > 0.6);
        println!(
            "shares (piquillín, jarilla, fachinal): monte {monte:.2?}, fringe {fringe:.2?}, open dry {open:.2?}"
        );
        // Piquillín is the monte's understory, the fachinal rims the groves,
        // jarilla holds the open dry ground.
        assert!(monte[0] > 0.7 && monte[1] < 0.05, "{monte:?}");
        assert!(fringe[2] > 0.5 && fringe[2] > fringe[0], "{fringe:?}");
        assert!(
            open[1] > 0.45 && open[1] > open[0] && open[1] > open[2],
            "{open:?}"
        );
        for kind in 0..3 {
            for zone in [monte, fringe, open] {
                assert!(zone[kind] >= 0.0);
            }
        }
        // The shares sum to one everywhere.
        for k in 0..200 {
            let p = Vec2::new(k as f32 * 17.0 - 1700.0, k as f32 * -11.0 + 900.0);
            let total: f32 = flora.shrub_shares(p).iter().sum();
            assert!((total - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn shrubs_have_their_kind_sizes_and_each_its_own_look() {
        let (_, _, flora) = site();
        let shrubs: Vec<Shrub> = world_shrubs(&flora).into_iter().map(|(_, s)| s).collect();
        for kind in ShrubKind::ALL {
            let (lo, hi) = kind.height_range();
            let own: Vec<&Shrub> = shrubs.iter().filter(|s| s.kind == kind).collect();
            assert!(own.len() > 1000, "{kind:?}");
            for s in &own {
                assert!((lo..=hi).contains(&s.height), "{s:?}");
                assert!((0.85..=1.2).contains(&s.width), "{s:?}");
                assert!((0.07..0.35).contains(&s.trunk_radius()), "{s:?}");
                assert!(s.crown_radius() > s.trunk_radius());
            }
        }
        // Every shrub has its own turn and seed.
        let looks: std::collections::HashSet<u32> = shrubs.iter().map(|s| s.variant).collect();
        assert!(looks.len() > shrubs.len() * 99 / 100);
    }

    #[test]
    fn shrubs_keep_off_town_ground_the_rail_the_world_edge_and_every_trunk() {
        let (world, line, flora) = site();
        let bounds = world.bounds;
        let mut chunks = between();
        chunks.extend((-4..4).flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y))));
        chunks.extend((-20..-17).flat_map(|x| (-20..-10).map(move |y| ChunkCoord::new(x, y))));
        // The monte and the groves of the south-west, thick with both.
        chunks.extend((-19..1).flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y))));
        let trees: Vec<Tree> = chunks.iter().flat_map(|&c| flora.trees(c)).collect();
        let (mut count, mut near_border) = (0, 0);
        let size = WorldConfig::default().chunk_size;
        for c in chunks {
            for s in flora.shrubs(c) {
                for area in &world.terrain.flat_areas {
                    assert!(
                        outside(area, s.at) >= area.falloff + FloraSite::SHRUB_TOWN_MARGIN,
                        "shrub on town ground {s:?}"
                    );
                }
                assert!(
                    rail_distance(&line, s.at) >= FloraSite::SHRUB_RAIL_MARGIN,
                    "{s:?}"
                );
                assert!(
                    bounds
                        .contain(s.at, FloraSite::SHRUB_EDGE_MARGIN)
                        .push_x
                        .is_none()
                );
                assert!(
                    bounds
                        .contain(s.at, FloraSite::SHRUB_EDGE_MARGIN)
                        .push_y
                        .is_none()
                );
                // Off every trunk, those of the neighbouring chunks too, and
                // with room for the rider to pass between.
                for t in &trees {
                    assert!(
                        t.at.distance(s.at)
                            >= t.trunk_radius() + s.trunk_radius() + FloraSite::SHRUB_TRUNK_GAP
                                - 1e-3,
                        "{s:?} too near the trunk of {t:?}"
                    );
                }
                let b = c.bounds(size);
                near_border += usize::from(
                    (s.at.x - b.min.x)
                        .min(b.max.x - s.at.x)
                        .min((s.at.y - b.min.y).min(b.max.y - s.at.y))
                        < 2.0,
                );
                count += 1;
            }
        }
        assert!(count > 5000 && near_border > 100, "{count} {near_border}");
    }

    #[test]
    fn the_shrub_border_covers_the_farthest_a_trunk_keeps_a_shrub_off() {
        // The stoutest tree trunk over every species and age, at its tallest.
        let trunk = [Species::Calden, Species::Chanar]
            .iter()
            .flat_map(|&species| Age::ALL.iter().map(move |&age| (species, age)))
            .map(|(species, age)| {
                Tree {
                    at: Vec2::ZERO,
                    species,
                    age,
                    height: age.height_range(species).1,
                    width: 1.0,
                    yaw: 0.0,
                    variant: 0,
                }
                .trunk_radius()
            })
            .fold(0.0, f32::max);
        let shrub = ShrubKind::ALL
            .iter()
            .map(|&k| {
                let sample = Shrub {
                    at: Vec2::ZERO,
                    kind: k,
                    height: k.height_range().1,
                    width: 1.0,
                    yaw: 0.0,
                    variant: 0,
                };
                sample.trunk_radius()
            })
            .fold(0.0, f32::max);
        let reach = trunk + shrub + FloraSite::SHRUB_TRUNK_GAP;
        assert!(
            FloraSite::SHRUB_BORDER >= reach,
            "a shrub {reach} m from a trunk across the border is missed ({} m)",
            FloraSite::SHRUB_BORDER
        );
    }

    #[test]
    fn shrubs_keep_off_roads_and_lagoons_and_apart() {
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
        let mut shrubs = 0;
        for c in chunks {
            let own = flora.shrubs(c);
            for (i, s) in own.iter().enumerate() {
                assert!(
                    roads.distance(s.at) >= FloraSite::SHRUB_ROAD_MARGIN,
                    "{s:?}"
                );
                // Lanes between the shrubs of a chunk.
                assert!(
                    own[i + 1..]
                        .iter()
                        .all(|o| o.at.distance(s.at) >= FloraSite::SHRUB_SPACING),
                    "{s:?}"
                );
                shrubs += 1;
            }
        }
        assert!(shrubs > 500, "{shrubs}");

        let lagoon_config = crate::lagoon::tests::arocena_config();
        let lagoon = Lagoon::new(&lagoon_config, &Terrain::default());
        let terrain = Terrain::default().with_lagoon(lagoon.clone());
        let world = WorldConfig::default();
        let (line, _) = RailLine::build(None, &[], world.bounds());
        let flora = FloraSite::new(world, &terrain, &line);
        let keep = lagoon.keep_out();
        let (lo, hi) = (
            ChunkCoord::containing(keep.min, world.chunk_size),
            ChunkCoord::containing(keep.max, world.chunk_size),
        );
        let mut near = 0;
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                for s in flora.shrubs(ChunkCoord { x: cx, y: cy }) {
                    let off = lagoon.offshore(s.at);
                    assert!(off >= FloraSite::SHRUB_LAGOON_MARGIN, "{s:?}");
                    near += usize::from(off < 40.0);
                }
            }
        }
        assert!(near > 10, "{near}");
    }

    #[test]
    fn planting_the_world_makes_no_shrub_an_obstacle_and_a_rider_rides_through_one() {
        let (mut world, _, flora) = site();
        let before = world.obstacles.len();
        let planted = world.plant(&flora);
        let shrubs = world_shrubs(&flora);
        // Only the trees are planted: shrubs (and grass) never block.
        assert_eq!(planted, flora.all_trees().len());
        assert_eq!(world.obstacles.len(), before + planted);
        for (_, s) in shrubs.iter().step_by(97) {
            assert!(!world.obstacles.contains(&s.obstacle()), "{s:?}");
        }
        // A tall piquillín with nothing planted in the lane west of it.
        let lane = |s: &Shrub| Aabb::new(s.at - Vec2::new(1.0, 3.0), s.at + Vec2::new(11.0, 3.0));
        let shrub = shrubs
            .iter()
            .map(|(_, s)| *s)
            .find(|s| {
                s.kind == ShrubKind::Piquillin
                    && s.height > 2.5
                    && world.obstacles.near(lane(s)).is_empty()
            })
            .expect("a tall piquillín in the open");
        // Ride at its core from 10 m east, heading west, at a gallop: the
        // rider goes straight through it.
        let mut rider = Rider::new(
            shrub.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let params = RiderParams::default();
        let mut through = false;
        for _ in 0..240 {
            rider.step(input, &params, &world, 1.0 / 60.0);
            through |= shrub.obstacle().contains_point(rider.position);
        }
        assert!(through, "never reached the shrub's core");
        assert!(rider.position.x < shrub.at.x - 1.0, "{:?}", rider.position);
    }

    #[test]
    fn a_planted_chunk_stops_a_rider_at_its_tree_not_its_shrub_and_unplanting_frees_it() {
        let (mut world, _, flora) = site();
        // A chunk with both kinds of plant.
        let coord = between()
            .into_iter()
            .find(|&c| {
                flora.trees(c).iter().any(|t| t.species == Species::Calden)
                    && flora
                        .shrubs(c)
                        .iter()
                        .any(|s| s.kind == ShrubKind::Piquillin)
            })
            .expect("a chunk with a caldén and a piquillín");
        let before = world.obstacles.len();
        let trees = flora.trees(coord);
        assert_eq!(world.plant_chunk(&flora, coord), trees.len());
        // Streamed, not permanent: the permanent list is untouched, and
        // only trees are in the layer.
        assert_eq!(world.obstacles.len(), before);
        assert_eq!(world.obstacles.streamed_len(), trees.len());
        let params = RiderParams::default();
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let radius = Circle::new(Rider::COLLIDER_RADIUS).radius;
        let calden = *trees.iter().find(|t| t.species == Species::Calden).unwrap();
        // Ride at the trunk from 10 m east, heading west, at a gallop.
        let mut rider = Rider::new(
            calden.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        for _ in 0..120 {
            rider.step(input, &params, &world, 1.0 / 60.0);
            let gap = calden
                .obstacle()
                .closest_point(rider.position)
                .distance(rider.position);
            assert!(gap >= radius - 1e-3, "inside the tree");
        }
        assert!(rider.position.x > calden.at.x - 0.5, "{:?}", rider.position);
        // A shrub of the same chunk does not stop a rider.
        let shrub = flora
            .shrubs(coord)
            .into_iter()
            .find(|s| {
                let lane = Aabb::new(s.at - Vec2::new(1.0, 3.0), s.at + Vec2::new(11.0, 3.0));
                s.kind == ShrubKind::Piquillin && world.obstacles.near(lane).is_empty()
            })
            .expect("a lone piquillín");
        let mut rider = Rider::new(
            shrub.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        for _ in 0..240 {
            rider.step(input, &params, &world, 1.0 / 60.0);
        }
        assert!(rider.position.x < shrub.at.x - 1.0, "{:?}", rider.position);
        // Unplanted, the tree's ground is free again.
        assert!(world.unplant_chunk(coord));
        assert!(!world.unplant_chunk(coord));
        assert_eq!(world.obstacles.streamed_len(), 0);
        let mut rider = Rider::new(
            calden.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        for _ in 0..240 {
            rider.step(input, &params, &world, 1.0 / 60.0);
        }
        assert!(rider.position.x < calden.at.x - 1.0, "{:?}", rider.position);
    }

    #[test]
    fn planting_a_chunk_again_gives_the_same_trees_and_every_chunk_adds_up_to_the_world() {
        let (mut world, _, flora) = site();
        let coord = ChunkCoord::new(5, -4);
        let first = world.plant_chunk(&flora, coord);
        let area = coord.bounds(world.config.chunk_size).expanded(20.0);
        let seen: Vec<Aabb> = world.obstacles.near(area).into_iter().copied().collect();
        // Replacing, not stacking: a second plant gives the same set.
        assert_eq!(world.plant_chunk(&flora, coord), first);
        assert_eq!(world.obstacles.streamed_len(), first);
        let again: Vec<Aabb> = world.obstacles.near(area).into_iter().copied().collect();
        assert_eq!(seen, again);
        // Dropped and brought back, the same footprints.
        world.unplant_chunk(coord);
        assert!(world.obstacles.near(area).is_empty());
        world.plant_chunk(&flora, coord);
        let back: Vec<Aabb> = world.obstacles.near(area).into_iter().copied().collect();
        assert_eq!(seen, back);
        // Every chunk of the world, streamed, is what planting the world adds.
        let mut all = World::default();
        all.plant(&flora);
        let mut streamed = World::default();
        let total: usize = world_chunks()
            .into_iter()
            .map(|c| streamed.plant_chunk(&flora, c))
            .sum();
        assert_eq!(total, all.obstacles.len());
        assert_eq!(streamed.obstacles.streamed_len(), all.obstacles.len());
    }

    /// Every chunk of the world: its trees before the thinning, and after.
    fn planted_and_kept(flora: &FloraSite) -> Vec<(ChunkCoord, Vec<Tree>, Vec<Tree>)> {
        let size = flora.config.chunk_size;
        let bounds = flora.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        (lo.y..=hi.y)
            .flat_map(|y| (lo.x..=hi.x).map(move |x| ChunkCoord::new(x, y)))
            .map(|c| {
                let planted = flora.chunk_trees(c, &flora.rail_near(c));
                (c, planted, flora.trees(c))
            })
            .collect()
    }

    #[test]
    fn about_half_the_trees_stay_evenly_over_monte_groves_and_lone_trees() {
        let (_, _, flora) = site();
        let all = planted_and_kept(&flora);
        // The share that stays in each kind of ground: the monte, the
        // groves, the open pampa's lone trees (planted, kept).
        let mut kinds = [(0.0_f32, 0.0_f32); 3];
        let (mut planted, mut kept) = (0.0_f32, 0.0_f32);
        for (c, before, after) in &all {
            // The kept trees are exactly the planted ones that stay.
            assert!(after.iter().all(|t| before.contains(t)));
            assert!(after.len() <= before.len());
            let (monte, grove) = (
                chunk_range(*c, |p| flora.monte(p)),
                chunk_range(*c, |p| flora.grove(p)),
            );
            let kind = if monte.1 > 0.8 {
                0
            } else if monte.2 < 0.02 && grove.1 > 0.6 {
                1
            } else if monte.2 < 0.02 && grove.2 < 0.02 {
                2
            } else {
                continue;
            };
            kinds[kind].0 += before.len() as f32;
            kinds[kind].1 += after.len() as f32;
        }
        for (_, before, after) in &all {
            planted += before.len() as f32;
            kept += after.len() as f32;
        }
        println!("trees: {planted} planted, {kept} kept; per kind {kinds:?}");
        assert!(planted > 50_000.0, "{planted}");
        let share = kept / planted;
        assert!((share - 0.616).abs() < 0.01, "{share}");
        assert_eq!(FloraSite::TREE_KEEP, 0.616);
        for (kind, (before, after)) in kinds.iter().enumerate() {
            // Within three standard deviations of a fair 0.616 share.
            let slack = 3.0 * (0.2365 / before).sqrt();
            assert!(*before > 50.0, "kind {kind}: {before}");
            assert!(
                (after / before - 0.616).abs() < slack,
                "kind {kind}: {after}/{before}"
            );
        }
    }

    #[test]
    fn the_same_trees_go_whichever_chunk_asks_and_shrubs_are_not_thinned() {
        let (_, _, flora) = site();
        for c in world_chunks().into_iter().step_by(37) {
            let chunk = flora.chunk(c);
            assert_eq!(chunk.trees, flora.trees(c));
            assert_eq!(chunk.shrubs, flora.shrubs(c));
        }
        let (trees, shrubs) = flora.all_plants();
        assert_eq!(trees, flora.all_trees());
        assert_eq!(shrubs, flora.all_shrubs());
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
        // Every tree's trunk is an obstacle.
        assert_eq!(planted, all.len());
        assert_eq!(world.obstacles.len(), before + planted);
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
        let (mut trees, mut tufts, mut shrubs) = (0, 0, 0);
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
            // A shrub's stem core never blocks the line either.
            for sh in &f.shrubs {
                assert!(gap(sh.at) >= FloraSite::SHRUB_RAIL_MARGIN - 1e-3, "{sh:?}");
                shrubs += 1;
            }
        }
        assert!(
            trees > 20 && tufts > 1000 && shrubs > 100,
            "{trees} {tufts} {shrubs}"
        );
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

    #[test]
    fn grass_grows_to_the_lagoons_wet_mud_and_trees_keep_back() {
        let config = crate::lagoon::tests::arocena_config();
        let lagoon = Lagoon::new(&config, &Terrain::default());
        let terrain = Terrain::default().with_lagoon(lagoon.clone());
        let world = WorldConfig::default();
        let (line, _) = RailLine::build(None, &[], world.bounds());
        let flora = FloraSite::new(world, &terrain, &line);
        let keep = lagoon.keep_out();
        let (lo, hi) = (
            ChunkCoord::containing(keep.min, world.chunk_size),
            ChunkCoord::containing(keep.max, world.chunk_size),
        );
        let mut near = 0;
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                let chunk = flora.chunk(ChunkCoord { x: cx, y: cy });
                for t in &chunk.tufts {
                    assert!(!lagoon.is_bare(t.at), "{t:?} on the mud");
                    let off = lagoon.offshore(t.at);
                    near += usize::from(off < Lagoon::MUD_BAND + 20.0);
                }
                for t in &chunk.trees {
                    assert!(
                        lagoon.offshore(t.at) >= FloraSite::TREE_LAGOON_MARGIN,
                        "{t:?}"
                    );
                }
            }
        }
        // The bank is grassed right down to the mud.
        assert!(near > 100, "{near}");
    }

    /// Distance from `p` to the nearest town's leveled core, in meters.
    fn town_distance(world: &World, p: Vec2) -> f32 {
        world
            .terrain
            .flat_areas
            .iter()
            .map(|a| outside(a, p))
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn the_monte_keeps_back_from_the_towns_and_the_world_total_holds() {
        // The shipped towns in the biggest world (2026-10-27, before the
        // redistribution, 0.56 kept): 190,919 trees, 28.4 a chunk within
        // 1.5 km of a town and 26.3 beyond 3 km. After "un 10% mas de
        // arboles grandes y zonas boscosas" (0.616 kept, older stands, a
        // wider monte): 218,995 trees, 6.0 a chunk near, 35.4 far. The 2-town `site()` world
        // is too small to give the far country the room to make up for it.
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let flora = FloraSite::new(config, &world.terrain, &line);
        let size = config.chunk_size;
        let (lo, hi) = (
            ChunkCoord::containing(config.bounds().min, size),
            ChunkCoord::containing(config.bounds().max, size),
        );
        let (mut near, mut far, mut total, mut peak) = ((0, 0), (0, 0), 0, 0);
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let c = ChunkCoord::new(x, y);
                let n = flora.trees(c).len();
                peak = peak.max(flora.chunk_trees(c, &flora.rail_near(c)).len());
                total += n;
                match town_distance(&world, c.bounds(size).center()) {
                    d if d < FloraSite::TOWN_NEAR => (near.0, near.1) = (near.0 + n, near.1 + 1),
                    d if d > FloraSite::TOWN_FULL => (far.0, far.1) = (far.0 + n, far.1 + 1),
                    _ => {}
                }
            }
        }
        let density = |(n, chunks): (usize, usize)| n as f32 / chunks as f32;
        println!(
            "trees: {total}, near {:.1} a chunk, far {:.1}, peak planted {peak}",
            density(near),
            density(far)
        );
        assert!(near.1 > 300 && far.1 > 3000, "{near:?} {far:?}");
        assert!(density(near) < 0.4 * density(far), "{near:?} {far:?}");
        let held = total as f32 / 218_995.0;
        assert!((0.95..=1.05).contains(&held), "{total} trees: {held}");
        // The boost spreads the stands wider; a chunk's budget stays far off.
        assert!(peak < FloraSite::TREE_BUDGET, "{peak}");
    }

    #[test]
    fn the_land_round_a_settlement_is_thinner_than_the_open_field_further_out() {
        let (world, towns, line, fences, layout) = crate::settlement::tests::settled();
        let flora = FloraSite::new(world.config, &world.terrain, &line)
            .with_track(&line.full_path(&towns))
            .with_fences(&fences)
            .with_settlements(&layout);
        let hubs: Vec<Vec2> = layout.settlements.iter().map(|s| s.hub).collect();
        assert_eq!(flora.worked.len(), hubs.len());
        let away = |p: Vec2| hubs.iter().map(|h| h.distance(p)).fold(f32::MAX, f32::min);
        // Trees and ground by distance to the nearest hub: the ring a casco
        // clears, and the field round it.
        let (mut ring, mut field) = ((0.0_f32, 0.0_f32), (0.0_f32, 0.0_f32));
        for t in flora.all_trees() {
            match away(t.at) {
                d if (FloraSite::WORKED_NEAR..FloraSite::WORKED_FULL).contains(&d) => ring.0 += 1.0,
                d if (800.0..1500.0).contains(&d) => field.0 += 1.0,
                _ => {}
            }
        }
        let bounds = world.config.bounds();
        let mut at = bounds.min;
        while at.y < bounds.max.y {
            at.x = bounds.min.x;
            while at.x < bounds.max.x {
                match away(at) {
                    d if (FloraSite::WORKED_NEAR..FloraSite::WORKED_FULL).contains(&d) => {
                        ring.1 += 1.0
                    }
                    d if (800.0..1500.0).contains(&d) => field.1 += 1.0,
                    _ => {}
                }
                at.x += 40.0;
            }
            at.y += 40.0;
        }
        let (ring, field) = (ring.0 / ring.1, field.0 / field.1);
        println!("trees per sample: ring {ring:.4}, field {field:.4}");
        assert!(field > 0.0 && ring < 0.7 * field, "{ring} {field}");
    }

    #[test]
    fn the_rural_share_is_smooth_across_chunk_borders_and_follows_the_distance() {
        let (world, _, flora) = redistributed();
        let area = world.terrain.flat_areas[0];
        // West of the town, away from the other.
        let west = area.center.x - area.half_extents.x;
        let at = |d: f32| flora.rural(Vec2::new(west - d, area.center.y));
        // Right by the town, past its near ring, on to the far country.
        assert!(
            (at(10.0) - FloraSite::TOWN_NEAR_SHARE).abs() < 0.02,
            "{}",
            at(10.0)
        );
        assert!(
            (at(FloraSite::TOWN_FULL) - 1.0).abs() < 0.05,
            "{}",
            at(3000.0)
        );
        assert!(at(100.0) < 0.2 && at(2000.0) > at(1000.0) && at(2900.0) > 0.8);
        assert!((at(FloraSite::FAR_FULL) - FloraSite::FAR_BOOST).abs() < 0.01);
        // Continuous: no step in 1 m anywhere along the line, over every
        // chunk border it crosses.
        let mut last = at(0.0);
        for k in 1..8000 {
            let now = at(k as f32);
            assert!((now - last).abs() < 0.01, "{k} m: {last} to {now}");
            last = now;
        }
        // The same share whichever site asks (it is a function of the spot,
        // not of any state): a second site, built apart, agrees.
        let p = Vec2::new(west - 1234.5, area.center.y + 321.0);
        let (_, _, other) = redistributed();
        assert_eq!(flora.rural(p), other.rural(p));
    }

    #[test]
    fn redistributed_trees_keep_their_spacing_across_chunk_borders_and_are_the_same_whichever_chunk_asks()
     {
        let (world, _, flora) = redistributed();
        // Chunks straddling the ramp out of the town (1.5 to 3 km).
        let area = world.terrain.flat_areas[0];
        let size = world.config.chunk_size;
        let from = world.config.chunk_of(Vec2::new(
            area.center.x + area.half_extents.x + 1500.0,
            area.center.y,
        ));
        let block: Vec<ChunkCoord> = (0..6)
            .flat_map(|dx| (-3..3).map(move |dy| ChunkCoord::new(from.x + dx, from.y + dy)))
            .filter(|c| world.config.has_chunk(*c))
            .collect();
        let trees: Vec<(ChunkCoord, Tree)> = block
            .iter()
            .flat_map(|&c| flora.trees(c).into_iter().map(move |t| (c, t)))
            .collect();
        assert!(trees.len() > 100, "{} over {size} m chunks", trees.len());
        for (i, (ca, a)) in trees.iter().enumerate() {
            for (cb, b) in &trees[i + 1..] {
                if ca != cb {
                    assert!(a.at.distance(b.at) >= a.spacing().max(b.spacing()));
                }
            }
        }
        for &c in &block {
            assert_eq!(flora.trees(c), flora.chunk(c).trees);
        }
    }
}
