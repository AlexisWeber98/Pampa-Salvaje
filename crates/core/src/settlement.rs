//! Rural settlements behind the field gates (user, 2026-10-22): the
//! ranchos, puestos, chacras and estancias of 1919 La Pampa.
//!
//! Some open roadside gates ([`crate::Gate`]) lead into a settled field.
//! Behind such a gate, inside its survey cell and clear of the roads, the
//! railway, the towns and the fences, stands a seeded compound: the houses
//! round a bare patio (the hub), with a galpón, corrals, a molino with its
//! tank and trough, haystacks and the spots where its trees will grow (a
//! compound plants a third more of them than its kind lays out, on a random
//! stream of its own: [`Settlement::MORE_TREES`]; then each kind keeps its
//! share, [`Settlement::tree_keep`]).
//!
//! Dirt tracks (huellas) join it to the gate: a trunk from just inside the gate to
//! the hub, and a branch from the hub to the door of every house, galpón
//! and corral. Tracks are two wheel ruts, not roads: they live outside
//! [`crate::RoadNetwork`], with no fences, signposts or terrain cut, and
//! cross no fence but at their own gate.
//!
//! Everything is axis-aligned, like the survey grid. Houses, galpones,
//! tanks and props are box obstacles; a corral is its rail spans with one
//! opening, so a rider can ride in; a tree spot blocks with its trunk only,
//! and render grows the tree on it.
//! Wild flora keeps off the compounds and the tracks through an indexed
//! [`KeepOut`].

use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use crate::rng::Rng;
use crate::road::segment_distance;
use crate::{Aabb, FenceLayout, RailLine, RoadNetwork, Side, Terrain, TownLayout, Vec2};

/// What a settlement is, from poorest to richest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementKind {
    /// A rancho: a small adobe house, maybe a corral, and a well.
    Hut,
    /// A puesto: a herdsman's hut out on an estancia's land, with a corral and an
    /// aguada (a trough, sometimes a molino).
    Outpost,
    /// A chacra: a farm with a brick or adobe house, a galpón, a molino with its tank and
    /// trough, a corral and haystacks.
    Farm,
    /// An estancia: its casco (the homestead) with its gallery, the houses of its peones,
    /// galpones, corrals, the aguada and a grove, spread over hectares.
    Ranch,
}

/// How a house is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HouseStyle {
    /// Adobe walls under a thatched (paja) roof.
    AdobeThatch,
    /// Adobe walls under a corrugated-iron roof.
    AdobeIron,
    /// Whitewashed brick with a flat roof behind a parapet.
    BrickParapet,
    /// An estancia's Italianate casco with a gallery along its front.
    GalleryHouse,
}

/// One building or prop of a settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructureKind {
    MainHouse(HouseStyle),
    /// A casa de peones, or the caretaker's house.
    WorkerHouse(HouseStyle),
    /// A galpón: a sheet-iron shed for tools, carts and grain.
    Barn,
    /// Post-and-rail corral, open on its front side.
    Corral,
    /// A molino (windpump): its footprint is the tower's base.
    Windpump,
    /// The round corrugated "australian" tank the molino fills.
    Tank,
    /// The bebedero the stock drinks from.
    Trough,
    /// A brick well curb (brocal) with its frame.
    Well,
    /// A parva of hay.
    Haystack,
    /// A palenque to tie horses to.
    HitchingPost,
    /// A carreta: a two-wheeled ox cart left in the yard.
    Cart,
    /// An abandoned four-wheeled wagon (a broken-down chata) rotting at a
    /// yard edge or on a road verge: scenery, a small obstacle.
    WreckedWagon,
    /// Where a eucalyptus of the grove grows; only its trunk blocks.
    Eucalyptus,
    /// Where a poplar of the entrance alameda grows; only its trunk blocks.
    Poplar,
}

impl StructureKind {
    /// A house people live in.
    pub fn is_house(self) -> bool {
        matches!(self, Self::MainHouse(_) | Self::WorkerHouse(_))
    }

    /// Whether a track leads to its door.
    pub fn is_reached(self) -> bool {
        self.is_house() || matches!(self, Self::Barn | Self::Corral)
    }

    /// A tree spot rather than a structure.
    pub fn is_tree(self) -> bool {
        matches!(self, Self::Eucalyptus | Self::Poplar)
    }

    /// Every model slot name [`Self::slot`] gives: lowercase folder names
    /// under `assets/models/third-party/`.
    pub const SLOTS: [&'static str; 9] = [
        "windpump",
        "eucalyptus",
        "poplar",
        "well",
        "trough",
        "cart",
        "haystack",
        "hitching_post",
        "wrecked_wagon",
    ];

    /// The model slot a downloaded glTF replaces the procedural mesh from
    /// (`assets/models/third-party/<slot>/`), for the props that have one.
    pub fn slot(self) -> Option<&'static str> {
        Some(match self {
            Self::Windpump => "windpump",
            Self::Eucalyptus => "eucalyptus",
            Self::Poplar => "poplar",
            Self::Well => "well",
            Self::Trough => "trough",
            Self::Cart => "cart",
            Self::Haystack => "haystack",
            Self::HitchingPost => "hitching_post",
            Self::WreckedWagon => "wrecked_wagon",
            _ => return None,
        })
    }
}

/// One building or prop: an axis-aligned footprint, its height and the
/// side its front (door, gallery, corral opening) faces: toward the hub.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Structure {
    pub kind: StructureKind,
    pub footprint: Aabb,
    /// Wall height, or the prop's or tree's height, in meters.
    pub height: f32,
    pub front: Side,
    /// Seeded variant (render picks a shape or a texture with it).
    pub variant: u32,
}

impl Structure {
    /// The point [`Settlement::DOOR_REACH`] meters out of the middle of
    /// its front: where its track ends.
    pub fn door(&self) -> Vec2 {
        let (fp, c) = (self.footprint, self.footprint.center());
        let reach = Settlement::DOOR_REACH;
        match self.front {
            Side::North => Vec2::new(c.x, fp.max.y + reach),
            Side::South => Vec2::new(c.x, fp.min.y - reach),
            Side::East => Vec2::new(fp.max.x + reach, c.y),
            Side::West => Vec2::new(fp.min.x - reach, c.y),
        }
    }

    /// A tree spot's trunk radius at the ground, in meters: a eucalyptus's
    /// thick pale bole, a poplar's slender one. 0 for anything else.
    pub fn trunk_radius(&self) -> f32 {
        match self.kind {
            StructureKind::Eucalyptus => (0.016 * self.height).clamp(0.2, 0.45),
            StructureKind::Poplar => (0.012 * self.height).clamp(0.12, 0.25),
            _ => 0.0,
        }
    }

    /// Its static obstacles: the footprint; a corral's rail spans with
    /// [`Settlement::CORRAL_GATE`] meters open in the middle of its front;
    /// a tree spot's trunk, a square [`Self::trunk_radius`] to either side.
    pub fn obstacles(&self) -> Vec<Aabb> {
        match self.kind {
            k if k.is_tree() => {
                let side = 2.0 * self.trunk_radius();
                vec![Aabb::from_center_size(
                    self.footprint.center(),
                    Vec2::new(side, side),
                )]
            }
            StructureKind::Corral => corral_rails(self.footprint, self.front),
            _ => vec![self.footprint],
        }
    }
}

/// An inner track (a huella): two wheel ruts along `points`,
/// [`Track::HALF_WIDTH`] meters to either side.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub points: Vec<Vec2>,
}

impl Track {
    /// Half the worn width, in meters (2.6 m in all).
    pub const HALF_WIDTH: f32 = 1.3;

    /// The straight pieces, in order.
    pub fn segments(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.points.windows(2).map(|w| (w[0], w[1]))
    }
}

/// One settlement behind a gate.
#[derive(Debug, Clone, PartialEq)]
pub struct Settlement {
    pub kind: SettlementKind,
    /// The open gate it lies behind (an index into [`FenceLayout::gates`]).
    pub gate: u32,
    /// The middle of the patio, where the tracks meet.
    pub hub: Vec2,
    /// The bare, trodden yard round the hub.
    pub patio: Aabb,
    pub structures: Vec<Structure>,
    /// The trunk from the gate to the hub first, then one branch from the
    /// hub to each door.
    pub tracks: Vec<Track>,
    /// Everything it covers: footprints, patio and tracks.
    pub bounds: Aabb,
    /// Seed for render's own variety.
    pub seed: u32,
}

impl Settlement {
    /// How far out of a front its track ends, in meters.
    pub const DOOR_REACH: f32 = 3.5;
    /// Width of a corral's opening, in meters.
    pub const CORRAL_GATE: f32 = 4.0;
    /// Thickness of a corral rail's obstacle, in meters.
    pub const CORRAL_RAIL: f32 = 0.2;
    /// Least distance from a footprint to a road's centerline, in meters:
    /// well behind the roadside fence.
    pub const ROAD_CLEARANCE: f32 = 25.0;
    /// Least distance from a barn to a road, in meters: well past the
    /// common clearance, so a visitor reaches the casco first and the
    /// galpones stay off the roadside. The road index sees 64 m
    /// ([`RoadNetwork::INDEX_REACH`]) and `off_roads_by` adds 2.5 m, so this
    /// is as far as it can be checked.
    pub const BARN_ROAD_CLEARANCE: f32 = 60.0;
    /// Least distance from a footprint to the track's centerline, meters.
    pub const RAIL_CLEARANCE: f32 = 40.0;
    /// Least distance from a footprint or track to a town's leveled band.
    pub const TOWN_CLEARANCE: f32 = 60.0;
    /// Least distance from a footprint to its cell's survey lines (the
    /// grid fences), in meters.
    pub const FENCE_CLEARANCE: f32 = 12.0;
    /// Least distance from a track's centerline to the survey lines.
    pub const TRACK_FENCE_CLEARANCE: f32 = 4.0;
    /// Least distance from a track's centerline to a road's, past its
    /// gate: its ruts stay off the roadside fence.
    pub const TRACK_ROAD_CLEARANCE: f32 = FenceLayout::ROADSIDE + Track::HALF_WIDTH + 1.0;
    /// How far from its gate a track's trunk may still be near the road.
    pub const GATE_APPROACH: f32 = 6.0;
    /// Least clear ground between two footprints, in meters: a rider fits.
    pub const GAP: f32 = 3.0;
    /// Least distance from a track's centerline to a footprint, meters.
    pub const CORRIDOR: f32 = Track::HALF_WIDTH + 1.0;
    /// Least distance between two settlements' hubs, in meters.
    pub const MIN_SPACING: f32 = 300.0;
    /// Extra eucalyptus a compound plants, as a share of the trees its kind
    /// lays out (the count rounds at random).
    pub const MORE_TREES: f32 = 0.35;
    /// The share of its trees a compound of `kind` keeps (user,
    /// 2026-10-27: 30% fewer, then "chacras y estancias sacale del total
    /// que tiene ahora, un 30%", then puestos and ranchos too: "se desmonta
    /// para huerto, animales, plantaciones"): ranchos, puestos and chacras
    /// 0.7, estancias 0.49 (0.7 x 0.7).
    pub fn tree_keep(kind: SettlementKind) -> f32 {
        match kind {
            SettlementKind::Hut | SettlementKind::Outpost | SettlementKind::Farm => 0.7,
            SettlementKind::Ranch => 0.49,
        }
    }
    /// Meters between the pairs of an estancia's alameda, planted as a
    /// windbreak (4 to 6 m).
    pub const POPLAR_SPACING: f32 = 6.0;
    /// How far each row of the alameda stands from the trunk's centerline.
    pub const POPLAR_OFFSET: f32 = 5.0;
    /// How far past the trunk's first point (half a meter inside the gate)
    /// the alameda's last pair stands: about 4 m inside the tranquera.
    pub const POPLAR_GATE_INSET: f32 = 3.5;
    /// Least ground an estancia's compound spreads over, in square meters.
    pub const RANCH_AREA: f32 = 20_000.0;
    /// Chance of an abandoned wagon at a chacra, and at an estancia.
    pub const WRECKS: (f32, f32) = (0.3, 0.5);
    /// An abandoned wagon's footprint: wide along its front, deep out of
    /// it, in meters.
    pub const WRECK_SIZE: (f32, f32) = (4.8, 3.6);

    /// Every structure of `kind`.
    pub fn of(&self, kind: StructureKind) -> impl Iterator<Item = &Structure> + '_ {
        self.structures.iter().filter(move |s| s.kind == kind)
    }

    /// The trunk track, from just inside the gate to the hub.
    pub fn trunk(&self) -> &Track {
        &self.tracks[0]
    }
}

/// Every settlement of the world, with the keep-out wild flora respects.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettlementLayout {
    pub settlements: Vec<Settlement>,
    pub(crate) keep_out: Arc<KeepOut>,
    /// The towns asked for an estancia that got none (see
    /// [`Self::build_with_ranches`]): the caller warns about them.
    pub unplaced_ranches: Vec<usize>,
    /// Abandoned wagons on road verges, between the road's edge and its
    /// fence: a handful on the whole map (see [`Self::ROADSIDE_WRECKS`]).
    pub roadside: Vec<RoadsideWreck>,
}

/// An abandoned wagon on a road verge, lying along the road.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadsideWreck {
    /// The middle of its footprint.
    pub at: Vec2,
    /// Unit direction of the road beside it: its length runs this way.
    pub along: Vec2,
    /// Seeded variant (render tilts it with it).
    pub variant: u32,
}

impl RoadsideWreck {
    /// Side of each of the boxes that block along its body, in meters.
    pub const BLOCK: f32 = 2.0;

    /// The corners of its turned footprint, [`Settlement::WRECK_SIZE`]:
    /// long along the road, deep across it.
    pub fn corners(&self) -> [Vec2; 4] {
        let (wide, deep) = Settlement::WRECK_SIZE;
        let (a, c) = (self.along * (wide / 2.0), self.across() * (deep / 2.0));
        [
            self.at - a - c,
            self.at + a - c,
            self.at + a + c,
            self.at - a + c,
        ]
    }

    /// Unit direction across the road (a quarter turn from `along`).
    pub fn across(&self) -> Vec2 {
        Vec2::new(-self.along.y, self.along.x)
    }

    /// The box round its turned footprint (the flora keeps off it).
    pub fn footprint(&self) -> Aabb {
        let [a, b, c, d] = self.corners();
        Aabb::new(
            Vec2::new(
                a.x.min(b.x).min(c.x).min(d.x),
                a.y.min(b.y).min(c.y).min(d.y),
            ),
            Vec2::new(
                a.x.max(b.x).max(c.x).max(d.x),
                a.y.max(b.y).max(c.y).max(d.y),
            ),
        )
    }

    /// What blocks the rider: three [`Self::BLOCK`] boxes along its body.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let reach = Settlement::WRECK_SIZE.0 / 2.0 - Self::BLOCK / 2.0;
        [-reach, 0.0, reach]
            .into_iter()
            .map(|t| {
                Aabb::from_center_size(
                    self.at + self.along * t,
                    Vec2::new(Self::BLOCK, Self::BLOCK),
                )
            })
            .collect()
    }
}

impl SettlementLayout {
    /// Share of open gates (into an unsettled field) that lead to a
    /// settlement near a town, and far from one.
    pub const SETTLED: (f32, f32) = (0.6, 0.45);
    /// Distance from a town's leveled band within which a gate counts as
    /// near the town (chacras gather there), in meters.
    pub const NEAR_TOWN: f32 = 3000.0;
    /// Tries at a hub per gate.
    const HUB_TRIES: usize = 16;
    /// Tries at a hub per gate for a forced estancia: it needs a big field.
    pub const RANCH_TRIES: usize = 64;
    /// How far from a town's leveled band a forced estancia's gate may be,
    /// in meters: within 3 km, then wider steps when nothing fits.
    pub const RANCH_REACH: [f32; 4] = [3000.0, 6000.0, 12_000.0, f32::INFINITY];
    /// Distance from the gate to the end of the trunk's first straight
    /// piece, in meters.
    const ENTRY: f32 = 12.0;
    /// Most abandoned wagons on the road verges, map-wide.
    pub const ROADSIDE_WRECKS: usize = 5;
    /// Chance that a roadside fence run gets a wreck.
    const ROADSIDE_CHANCE: f32 = 0.15;
    /// Least distance between two roadside wrecks, in meters.
    const ROADSIDE_SPACING: f32 = 1500.0;
    /// Gap between a wreck and the road's edge, in meters.
    const VERGE_GAP: f32 = 0.4;

    /// The settlements behind the open gates of `fences`, over `terrain`
    /// (with its roads laid), clear of the towns of `towns` and of `line`'s
    /// whole track. Seeded per gate: the same every time.
    pub fn build(
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
        fences: &FenceLayout,
    ) -> Self {
        Self::build_with_ranches(terrain, towns, line, fences, &[])
    }

    /// Like [`Self::build`], with one estancia forced near each town of
    /// `ranch_towns` (indices into `towns`), laid before the seeded mix so
    /// the other settlements keep their spacing from it.
    ///
    /// For each such town the open gates within [`Self::RANCH_REACH`] of its
    /// leveled band are tried nearest first, each with up to
    /// [`Self::RANCH_TRIES`] hubs; the first whose field holds a whole
    /// estancia (its casco, at least two worker houses, a barn, a corral,
    /// the water point, over [`Settlement::RANCH_AREA`]) wins. When none
    /// fits, the reach widens step by step; a town that still gets none is
    /// listed in [`Self::unplaced_ranches`].
    pub fn build_with_ranches(
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
        fences: &FenceLayout,
        ranch_towns: &[usize],
    ) -> Self {
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(line.track_paths(towns).iter().flat_map(|path| {
                path.points()
                    .windows(2)
                    .map(|w| (w[0], w[1]))
                    .collect::<Vec<_>>()
            }))
            .collect();
        // Towns and lagoons with their banks.
        let bands: Vec<Aabb> = towns
            .iter()
            .map(|t| t.leveled().expanded(Settlement::TOWN_CLEARANCE))
            .chain(terrain.lagoons.iter().map(|l| l.keep_out()))
            .collect();
        let site = Site {
            terrain,
            fences,
            rails,
            bands,
        };
        let town_gap = |p: Vec2| {
            towns
                .iter()
                .map(|t| t.leveled().closest_point(p).distance(p))
                .fold(f32::INFINITY, f32::min)
        };
        let mut placed = Placed::default();
        let mut unplaced_ranches = Vec::new();
        for &town in ranch_towns {
            let Some(band) = towns.get(town).map(TownLayout::leveled) else {
                unplaced_ranches.push(town);
                continue;
            };
            let gap = |p: Vec2| band.closest_point(p).distance(p);
            let mut open: Vec<(f32, usize)> = fences
                .gates
                .iter()
                .enumerate()
                .filter(|(_, g)| g.open)
                .map(|(i, g)| (gap(g.at), i))
                .collect();
            open.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut done = false;
            let mut tried = 0;
            for reach in Self::RANCH_REACH {
                for &(_, index) in open.iter().filter(|(d, _)| *d <= reach).skip(tried) {
                    tried += 1;
                    let rng = Rng::new(terrain.seed ^ RANCH_SEED, index as u32);
                    let forced = Some((SettlementKind::Ranch, Self::RANCH_TRIES));
                    if site.settle_gate(index, rng, forced, &mut placed, &town_gap) {
                        done = true;
                        break;
                    }
                }
                if done {
                    break;
                }
            }
            if !done {
                unplaced_ranches.push(town);
            }
        }
        for (index, gate) in fences.gates.iter().enumerate() {
            if !gate.open {
                continue;
            }
            let rng = Rng::new(terrain.seed ^ SETTLEMENT_SEED, index as u32);
            site.settle_gate(index, rng, None, &mut placed, &town_gap);
        }
        let settlements = placed.settlements;
        let roadside = site.roadside_wrecks(terrain.seed, &settlements);
        let mut keep_out = KeepOut::default();
        for wreck in &roadside {
            keep_out.push(Shape::Area(wreck.footprint()));
        }
        for s in &settlements {
            keep_out.push(Shape::Area(s.patio));
            for st in &s.structures {
                keep_out.push(Shape::Area(st.footprint));
            }
            for h in &s.tracks {
                for (a, b) in h.segments() {
                    keep_out.push(Shape::Strip(a, b, Track::HALF_WIDTH));
                }
            }
        }
        Self {
            settlements,
            keep_out: Arc::new(keep_out),
            unplaced_ranches,
            roadside,
        }
    }

    /// How many settlements are of `kind`.
    pub fn count(&self, kind: SettlementKind) -> usize {
        self.settlements.iter().filter(|s| s.kind == kind).count()
    }

    /// The static obstacles of every settlement and roadside wreck.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .flat_map(Structure::obstacles)
            .chain(self.roadside.iter().flat_map(RoadsideWreck::obstacles))
    }

    /// [`Self::obstacles`] in the same order, each with how tall it is in
    /// meters: a structure's own height (a jumping horse clears a low
    /// trough or palenque), a roadside wreck's unbounded.
    pub fn obstacle_heights(&self) -> impl Iterator<Item = (Aabb, f32)> + '_ {
        self.settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .flat_map(|s| s.obstacles().into_iter().map(|o| (o, s.height)))
            .chain(
                self.roadside
                    .iter()
                    .flat_map(RoadsideWreck::obstacles)
                    .map(|o| (o, f32::INFINITY)),
            )
    }

    /// The settlements whose hub lies in `area` (a terrain chunk; west and
    /// south edges in, east and north out), so each has one chunk.
    pub fn in_area(&self, area: Aabb) -> impl Iterator<Item = &Settlement> + '_ {
        self.settlements.iter().filter(move |s| {
            let p = s.hub;
            p.x >= area.min.x && p.x < area.max.x && p.y >= area.min.y && p.y < area.max.y
        })
    }

    /// The settlements reaching into `area`.
    pub fn touching(&self, area: Aabb) -> impl Iterator<Item = &Settlement> + '_ {
        self.settlements
            .iter()
            .filter(move |s| s.bounds.intersects(&area))
    }

    /// The tracks of the settlements [`Self::in_area`] holds.
    pub fn tracks_in(&self, area: Aabb) -> impl Iterator<Item = &Track> + '_ {
        self.in_area(area).flat_map(|s| s.tracks.iter())
    }

    /// Whether `p` is more than `margin` meters (at most
    /// [`KeepOut::MAX_MARGIN`]) from every footprint, patio and track.
    pub fn is_clear(&self, p: Vec2, margin: f32) -> bool {
        self.keep_out.is_clear(p, margin)
    }

    /// The indexed keep-out of every settlement.
    pub fn keep_out(&self) -> &KeepOut {
        &self.keep_out
    }
}

/// What every settlement keeps clear of, world-wide.
struct Site<'a> {
    terrain: &'a Terrain,
    fences: &'a FenceLayout,
    rails: Vec<(Vec2, Vec2)>,
    /// Town bands, already grown by [`Settlement::TOWN_CLEARANCE`].
    bands: Vec<Aabb>,
}

/// The settlements laid so far and the survey cells they hold.
#[derive(Default)]
struct Placed {
    settlements: Vec<Settlement>,
    used: HashSet<(u32, u32)>,
}

impl Site<'_> {
    /// Tries to settle the field behind gate `index`: by its seeded chance
    /// and kind, or as `forced` (a kind and its hub tries) whatever the
    /// chance. Whether a settlement was laid.
    fn settle_gate(
        &self,
        index: usize,
        mut rng: Rng,
        forced: Option<(SettlementKind, usize)>,
        placed: &mut Placed,
        town_gap: &impl Fn(Vec2) -> f32,
    ) -> bool {
        let gate = &self.fences.gates[index];
        let roads = self.terrain.roads.as_deref();
        let into = field_side(roads, gate.at, gate.along);
        let probe = gate.at + into * 3.0;
        if self.fences.is_descampado(probe) {
            return false;
        }
        let Some(cell) = self.fences.field_of(probe) else {
            return false;
        };
        let key = (cell.min.x.to_bits(), cell.min.y.to_bits());
        if placed.used.contains(&key) {
            return false;
        }
        let (kind, tries) = match forced {
            Some(forced) => forced,
            None => {
                let near_town = town_gap(gate.at) < SettlementLayout::NEAR_TOWN;
                let chance = if near_town {
                    SettlementLayout::SETTLED.0
                } else {
                    SettlementLayout::SETTLED.1
                };
                if rng.unit() >= chance {
                    return false;
                }
                (pick_kind(&mut rng, near_town), SettlementLayout::HUB_TRIES)
            }
        };
        let reach = cell.expanded(Settlement::RAIL_CLEARANCE + 1.0);
        let ground = Ground {
            roads,
            rails: self
                .rails
                .iter()
                .copied()
                .filter(|&(a, b)| reach.segment_overlap(a, b).is_some())
                .collect(),
            bands: self
                .bands
                .iter()
                .copied()
                .filter(|b| b.intersects(&reach))
                .collect(),
            cell,
        };
        let start = gate.at + into * 0.5;
        let entry = gate.at + into * SettlementLayout::ENTRY;
        if !ground.track_ok(start, entry, Some(gate.at)) {
            return false;
        }
        let (depth, room) = kind.hub_reach();
        let half = cell.size().x.min(cell.size().y) / 2.0 - 1.0;
        let inner = cell.expanded(-room.min(half));
        for _ in 0..tries {
            // Somewhere ahead of the gate, kept inside the cell.
            let ahead = gate.at
                + into * rng.range(depth.0, depth.1)
                + gate.along * rng.range(-depth.1, depth.1) * 0.5;
            let hub = inner.closest_point(ahead);
            let spaced = placed
                .settlements
                .iter()
                .all(|s| s.hub.distance(hub) >= Settlement::MIN_SPACING);
            if (hub - gate.at).dot(into) < depth.0
                || hub.distance(gate.at) > depth.1
                || !spaced
                || !ground.track_ok(entry, hub, None)
            {
                continue;
            }
            let mut compound = Compound::new(&ground, rng, kind, hub, into, [start, entry, hub]);
            let built = compound.raise() && {
                compound.plant_more();
                true
            };
            rng = compound.rng;
            if built {
                placed.used.insert(key);
                let seed = rng.next();
                placed.settlements.push(compound.finish(index as u32, seed));
                return true;
            }
        }
        false
    }
}

impl SettlementKind {
    /// How far into the field (from the gate's road) the hub lies at
    /// least, how far from the gate at most, and the room it keeps from its
    /// cell's edges, in meters.
    fn hub_reach(self) -> ((f32, f32), f32) {
        match self {
            Self::Hut => ((35.0, 250.0), 35.0),
            Self::Outpost => ((50.0, 450.0), 45.0),
            Self::Farm => ((60.0, 350.0), 55.0),
            Self::Ranch => ((140.0, 600.0), 120.0),
        }
    }

    /// Side of the patio, in meters.
    fn patio(self) -> f32 {
        match self {
            Self::Hut | Self::Outpost => 10.0,
            Self::Farm => 16.0,
            Self::Ranch => 24.0,
        }
    }
}

/// The kind of a new settlement: chacras near the towns, puestos and
/// ranchos scattered, estancias rare.
fn pick_kind(rng: &mut Rng, near_town: bool) -> SettlementKind {
    use SettlementKind::*;
    let roll = rng.unit();
    let odds: [(SettlementKind, f32); 4] = if near_town {
        [(Farm, 0.55), (Hut, 0.3), (Outpost, 0.1), (Ranch, 0.05)]
    } else {
        [(Hut, 0.35), (Outpost, 0.35), (Farm, 0.2), (Ranch, 0.1)]
    };
    let mut sum = 0.0;
    for (kind, share) in odds {
        sum += share;
        if roll < sum {
            return kind;
        }
    }
    odds[0].0
}

/// Unit direction from the road through a gate at `at` (its fence along
/// `along`) into the field.
fn field_side(roads: Option<&RoadNetwork>, at: Vec2, along: Vec2) -> Vec2 {
    let across = Vec2::new(-along.y, along.x);
    let gap = |p: Vec2| {
        roads
            .and_then(|r| r.near(p, RoadNetwork::INDEX_REACH))
            .unwrap_or(f32::INFINITY)
    };
    if gap(at + across * 4.0) >= gap(at - across * 4.0) {
        across
    } else {
        across * -1.0
    }
}

/// What one settlement keeps clear of: the roads, the railway and the
/// towns near its cell, and the cell's fences.
struct Ground<'a> {
    roads: Option<&'a RoadNetwork>,
    rails: Vec<(Vec2, Vec2)>,
    /// Town bands, already grown by [`Settlement::TOWN_CLEARANCE`].
    bands: Vec<Aabb>,
    cell: Aabb,
}

impl Ground<'_> {
    /// Whether a footprint may stand on `area`.
    fn area_ok(&self, area: Aabb) -> bool {
        self.area_ok_by(area, Settlement::FENCE_CLEARANCE, true)
    }

    /// [`Self::area_ok`] with its own `fence` clearance, and the roads' only
    /// when `roads`: the alameda runs up to the gate, on the road's verge.
    fn area_ok_by(&self, area: Aabb, fence: f32, roads: bool) -> bool {
        self.cell.expanded(-fence).contains(&area)
            && self.bands.iter().all(|b| !b.intersects(&area))
            && self.rails.iter().all(|&(a, b)| {
                area.expanded(Settlement::RAIL_CLEARANCE)
                    .segment_overlap(a, b)
                    .is_none()
            })
            && (!roads || self.off_roads(area))
    }

    /// Whether `area` is [`Settlement::ROAD_CLEARANCE`] from every road:
    /// samples along its edges at most 5 m apart, each held 2.5 m farther
    /// off, and its middle.
    fn off_roads(&self, area: Aabb) -> bool {
        self.off_roads_by(area, Settlement::ROAD_CLEARANCE)
    }

    /// [`Self::off_roads`] for any `clearance`.
    fn off_roads_by(&self, area: Aabb, clearance: f32) -> bool {
        let Some(roads) = self.roads else {
            return true;
        };
        let reach = clearance + 2.5;
        let size = area.size();
        let (nx, ny) = (
            (size.x / 5.0).ceil().max(1.0) as usize,
            (size.y / 5.0).ceil().max(1.0) as usize,
        );
        let clear = |p: Vec2| roads.near(p, reach).is_none();
        clear(area.center())
            && (0..=nx).all(|k| {
                let x = area.min.x + size.x * (k as f32 / nx as f32);
                clear(Vec2::new(x, area.min.y)) && clear(Vec2::new(x, area.max.y))
            })
            && (0..=ny).all(|k| {
                let y = area.min.y + size.y * (k as f32 / ny as f32);
                clear(Vec2::new(area.min.x, y)) && clear(Vec2::new(area.max.x, y))
            })
    }

    /// Whether a track may run straight from `a` to `b`: inside the cell
    /// off its fences, off the roads but within
    /// [`Settlement::GATE_APPROACH`] of its `gate`, off the rail and the
    /// towns. Sampled every meter.
    fn track_ok(&self, a: Vec2, b: Vec2, gate: Option<Vec2>) -> bool {
        let inner = self.cell.expanded(-Settlement::TRACK_FENCE_CLEARANCE);
        let n = (a.distance(b).ceil() as usize).max(1);
        (0..=n).all(|k| {
            let p = a + (b - a) * (k as f32 / n as f32);
            let approach = gate.is_some_and(|g| g.distance(p) <= Settlement::GATE_APPROACH);
            let placed = if approach {
                self.cell.contains_point(p)
            } else {
                inner.contains_point(p)
                    && self
                        .roads
                        .is_none_or(|r| r.near(p, Settlement::TRACK_ROAD_CLEARANCE).is_none())
            };
            placed
                && self.bands.iter().all(|band| !band.contains_point(p))
                && self
                    .rails
                    .iter()
                    .all(|&(r0, r1)| segment_distance(p, r0, r1) >= Settlement::RAIL_CLEARANCE)
        })
    }
}

/// A building or prop to place: its kind, its size (along its front, then
/// deep) and its height.
#[derive(Clone, Copy)]
struct Part {
    kind: StructureKind,
    size: Vec2,
    height: f32,
}

/// Where to look for a part's middle: `radius` meters (least, most) from
/// `around`, in a direction within `spread` radians of `bearing`.
#[derive(Clone, Copy)]
struct Spot {
    around: Vec2,
    radius: (f32, f32),
    bearing: f32,
    spread: f32,
}

/// One settlement being raised.
struct Compound<'a> {
    ground: &'a Ground<'a>,
    rng: Rng,
    kind: SettlementKind,
    hub: Vec2,
    /// Unit direction from the gate into the field.
    into: Vec2,
    patio: Aabb,
    structures: Vec<Structure>,
    tracks: Vec<Track>,
}

impl<'a> Compound<'a> {
    /// Tries at placing one part.
    const TRIES: usize = 24;

    fn new(
        ground: &'a Ground<'a>,
        rng: Rng,
        kind: SettlementKind,
        hub: Vec2,
        into: Vec2,
        trunk: [Vec2; 3],
    ) -> Self {
        let side = kind.patio();
        Self {
            ground,
            rng,
            kind,
            hub,
            into,
            patio: Aabb::from_center_size(hub, Vec2::new(side, side)),
            structures: Vec::new(),
            tracks: vec![Track {
                points: trunk.to_vec(),
            }],
        }
    }

    /// Raises the compound of its kind. `false` when a part it must have
    /// does not fit.
    fn raise(&mut self) -> bool {
        if !self.ground.area_ok(self.patio) {
            return false;
        }
        match self.kind {
            SettlementKind::Hut => self.hut(),
            SettlementKind::Outpost => self.outpost(),
            SettlementKind::Farm => self.farm(),
            SettlementKind::Ranch => self.ranch(),
        }
    }

    fn hut(&mut self) -> bool {
        let style = if self.chance(0.65) {
            HouseStyle::AdobeThatch
        } else {
            HouseStyle::AdobeIron
        };
        let house = self.part(
            StructureKind::MainHouse(style),
            (8.0, 10.0),
            (4.5, 5.5),
            2.6,
        );
        if self.place(house, self.ahead(10.0, 16.0, 0.6)).is_none() {
            return false;
        }
        if self.chance(0.7) {
            let well = self.part(StructureKind::Well, (1.6, 1.6), (1.6, 1.6), 0.9);
            self.place(well, self.round(8.0, 16.0));
        }
        if self.chance(0.5) {
            self.corral((12.0, 16.0), self.round(18.0, 30.0));
        }
        self.extras(0.6, 0.25, (7.0, 10.0), (10.0, 20.0));
        true
    }

    fn outpost(&mut self) -> bool {
        let style = if self.chance(0.5) {
            HouseStyle::AdobeThatch
        } else {
            HouseStyle::AdobeIron
        };
        let hut = self.part(StructureKind::MainHouse(style), (6.0, 8.0), (4.0, 5.0), 2.5);
        if self.place(hut, self.ahead(9.0, 14.0, 0.8)).is_none() {
            return false;
        }
        if !self.corral((18.0, 24.0), self.round(20.0, 34.0)) {
            return false;
        }
        let windpump = self.chance(0.5);
        let tank = windpump && self.chance(0.5);
        self.water_point(windpump, tank, (14.0, 26.0));
        self.extras(0.7, 0.0, (7.0, 10.0), (10.0, 20.0));
        true
    }

    fn farm(&mut self) -> bool {
        let style = if self.chance(0.6) {
            HouseStyle::BrickParapet
        } else {
            HouseStyle::AdobeIron
        };
        let house = self.part(
            StructureKind::MainHouse(style),
            (12.0, 16.0),
            (7.0, 9.0),
            3.4,
        );
        let Some(casco) = self.place(house, self.ahead(12.0, 20.0, 0.5)) else {
            return false;
        };
        let barn = self.part(StructureKind::Barn, (14.0, 20.0), (8.0, 10.0), 4.5);
        if self.place_behind(barn, casco, (22.0, 40.0)).is_none() {
            return false;
        }
        if !self.water_point(true, true, (20.0, 36.0)) {
            return false;
        }
        self.corral((18.0, 26.0), self.round(30.0, 50.0));
        if self.chance(0.35) {
            let style = if self.chance(0.5) {
                HouseStyle::AdobeIron
            } else {
                HouseStyle::AdobeThatch
            };
            let worker = self.part(
                StructureKind::WorkerHouse(style),
                (8.0, 10.0),
                (4.5, 5.5),
                2.7,
            );
            self.place(worker, self.round(25.0, 45.0));
        }
        for _ in 0..1 + self.rng.below(3) {
            let hay = self.part(StructureKind::Haystack, (5.0, 7.0), (5.0, 7.0), 4.0);
            self.place(hay, self.round(30.0, 60.0));
        }
        self.extras(0.8, 0.5, (8.0, 12.0), (14.0, 30.0));
        for _ in 0..self.rng.below(5) {
            let tree = self.tree(StructureKind::Eucalyptus);
            self.place(tree, self.round(16.0, 34.0));
        }
        if self.chance(Settlement::WRECKS.0) {
            self.wreck((40.0, 70.0));
        }
        true
    }

    fn ranch(&mut self) -> bool {
        let homestead = self.part(
            StructureKind::MainHouse(HouseStyle::GalleryHouse),
            (22.0, 30.0),
            (14.0, 18.0),
            4.5,
        );
        let Some(casco) = self.place(homestead, self.ahead(18.0, 26.0, 0.3)) else {
            return false;
        };
        let homestead = self.structures[casco].footprint;
        // First, so the casas de peones and their tracks keep clear of the
        // alameda instead of taking its place.
        self.poplar_row();
        // The casas de peones, each in its own sector round the patio.
        let houses = 2 + self.rng.below(3);
        let turn = self.rng.range(0.0, TAU);
        let mut built = 0;
        for k in 0..houses {
            let style = if self.chance(0.6) {
                HouseStyle::AdobeIron
            } else {
                HouseStyle::BrickParapet
            };
            let house = self.part(
                StructureKind::WorkerHouse(style),
                (9.0, 12.0),
                (5.0, 6.0),
                2.8,
            );
            let mut spot = self.round(70.0, 130.0);
            spot.bearing = turn + TAU * k as f32 / houses as f32;
            spot.spread = PI / houses as f32;
            if self.place(house, spot).is_some() {
                built += 1;
            }
        }
        if built < 2 {
            return false;
        }
        let mut barns = 0;
        for _ in 0..1 + self.rng.below(2) {
            let barn = self.part(StructureKind::Barn, (20.0, 30.0), (10.0, 14.0), 5.5);
            barns += self.place_behind(barn, casco, (40.0, 80.0)).is_some() as usize;
        }
        // The corrals go with the galpones, behind the casco.
        let mut corrals = 0;
        for _ in 0..1 + self.rng.below(3) {
            let behind = self.behind(casco, (45.0, 100.0), 1.0);
            corrals += self.corral((22.0, 34.0), behind) as usize;
        }
        if barns == 0 || corrals == 0 || !self.water_point(true, true, (35.0, 70.0)) {
            return false;
        }
        for _ in 0..self.rng.below(4) {
            let hay = self.part(StructureKind::Haystack, (5.0, 7.0), (5.0, 7.0), 4.0);
            self.place(hay, self.round(60.0, 120.0));
        }
        for _ in 0..1 + self.rng.below(2) {
            let post = self.part(StructureKind::HitchingPost, (3.0, 3.0), (0.3, 0.3), 1.1);
            self.place(post, self.ahead(13.0, 20.0, 1.0));
        }
        let cart = self.part(StructureKind::Cart, (5.0, 5.0), (2.2, 2.2), 2.5);
        self.place(cart, self.round(25.0, 50.0));
        self.grove(homestead);
        let bounds = self.bounds();
        if bounds.size().x * bounds.size().y < Settlement::RANCH_AREA {
            return false;
        }
        // Drawn only once the estancia stands, so a retried hub draws the
        // same numbers as before.
        if self.chance(Settlement::WRECKS.1) {
            self.wreck((60.0, 110.0));
        }
        true
    }

    /// An abandoned wagon at the yard's edge, `at` meters from the hub:
    /// placed last, so every door and track is already laid and it keeps
    /// clear of them like any footprint.
    fn wreck(&mut self, at: (f32, f32)) {
        let (wide, deep) = Settlement::WRECK_SIZE;
        let part = self.part(StructureKind::WreckedWagon, (wide, wide), (deep, deep), 2.2);
        self.place(part, self.round(at.0, at.1));
    }

    /// The alameda: poplars in pairs facing each other across the trunk,
    /// [`Settlement::POPLAR_SPACING`] apart, from about 4 m inside the gate
    /// back to 20 m from the hub, whatever the lane's length. The rows
    /// follow the drawn trunk (gate, bend, hub), each pair across its own
    /// leg. [`Self::thin_trees`] leaves them alone: it keeps every other
    /// tree in planting order, which would fell one side or stagger the
    /// pairs.
    fn poplar_row(&mut self) {
        let [gate, bend, hub] = [
            self.tracks[0].points[0],
            self.tracks[0].points[1],
            self.tracks[0].points[2],
        ];
        let last = bend.distance(hub);
        let total = last + gate.distance(bend);
        let mut along = total - Settlement::POPLAR_GATE_INSET;
        while along >= 20.0 {
            let (from, to, back) = if along <= last {
                (hub, bend, along)
            } else {
                (bend, gate, along - last)
            };
            let dir = (to - from) * (1.0 / from.distance(to).max(1e-6));
            let across = Vec2::new(-dir.y, dir.x);
            let before = self.structures.len();
            let mut planted = 0;
            for side in [-Settlement::POPLAR_OFFSET, Settlement::POPLAR_OFFSET] {
                let at = from + dir * back + across * side;
                let tree = self.tree(StructureKind::Poplar);
                planted += self.place_at_by(tree, at, true).is_some() as usize;
            }
            // At the bend the inside tree can land on the other leg's
            // track: skip the pair rather than leave one tree alone.
            if planted == 1 {
                self.structures.truncate(before + 1);
                self.structures.pop();
            }
            along -= Settlement::POPLAR_SPACING;
        }
    }

    /// A monte of eucalyptus beside the casco: a jittered grid 6 m apart.
    fn grove(&mut self, homestead: Aabb) {
        let across = Vec2::new(-self.into.y, self.into.x);
        let side = if self.chance(0.5) { 1.0 } else { -1.0 };
        let (rows, cols) = (4 + self.rng.below(3), 5 + self.rng.below(4));
        let corner = homestead.center()
            + self.into * self.rng.range(20.0, 40.0)
            + across * (side * self.rng.range(15.0, 35.0));
        for r in 0..rows {
            for c in 0..cols {
                let jitter = Vec2::new(self.rng.range(-1.5, 1.5), self.rng.range(-1.5, 1.5));
                let at = corner + Vec2::new(c as f32 * 6.0, r as f32 * 6.0) + jitter;
                let tree = self.tree(StructureKind::Eucalyptus);
                self.place_at(tree, at);
            }
        }
    }

    /// A molino (or a well) with its trough, and the tank when `tank`, at
    /// `radius` from the hub. `false` when the molino or well does not fit.
    fn water_point(&mut self, windpump: bool, tank: bool, radius: (f32, f32)) -> bool {
        let source = if windpump {
            self.part(StructureKind::Windpump, (2.4, 2.4), (2.4, 2.4), 10.0)
        } else {
            self.part(StructureKind::Well, (1.6, 1.6), (1.6, 1.6), 0.9)
        };
        let Some(at) = self.place(source, self.round(radius.0, radius.1)) else {
            return false;
        };
        let at = self.structures[at].footprint.center();
        if tank {
            let d = self.rng.range(7.0, 10.0);
            let tank = self.part(StructureKind::Tank, (d, d), (d, d), 1.6);
            self.place(
                tank,
                Spot {
                    around: at,
                    ..self.round(9.0, 12.0)
                },
            );
        }
        let trough = self.part(StructureKind::Trough, (6.0, 8.0), (0.9, 0.9), 0.6);
        self.place(
            trough,
            Spot {
                around: at,
                ..self.round(5.0, 9.0)
            },
        );
        true
    }

    /// A corral `side` meters square or so; whether it fits.
    fn corral(&mut self, side: (f32, f32), spot: Spot) -> bool {
        let corral = self.part(StructureKind::Corral, side, side, 1.6);
        self.place(corral, spot).is_some()
    }

    /// A hitching post by the patio and a carreta in the yard, each by
    /// its chance.
    fn extras(&mut self, post: f32, cart: f32, post_at: (f32, f32), cart_at: (f32, f32)) {
        if self.chance(post) {
            let part = self.part(StructureKind::HitchingPost, (3.0, 3.0), (0.3, 0.3), 1.1);
            self.place(part, self.round(post_at.0, post_at.1));
        }
        if self.chance(cart) {
            let part = self.part(StructureKind::Cart, (5.0, 5.0), (2.2, 2.2), 2.5);
            self.place(part, self.round(cart_at.0, cart_at.1));
        }
    }

    /// Plants 30% more eucalyptus than the compound already has, on a
    /// stream of its own so the rest of the layout does not move.
    fn plant_more(&mut self) {
        let trees = self.structures.iter().filter(|s| s.kind.is_tree()).count();
        let mut own = Rng::new(0x70a1_a2c3 ^ self.hub.x.to_bits() ^ self.hub.y.to_bits(), 1);
        let extra = (Settlement::MORE_TREES * trees as f32 + own.unit()) as usize;
        std::mem::swap(&mut self.rng, &mut own);
        for _ in 0..extra {
            let tree = self.tree(StructureKind::Eucalyptus);
            self.place(tree, self.round(16.0, 34.0));
        }
        std::mem::swap(&mut self.rng, &mut own);
        self.thin_trees(own.unit());
    }

    /// A compound keeps [`Settlement::tree_keep`] of its trees: a tree stays when its share of the running
    /// total crosses a whole number, so the ones that go are spread evenly through the order they were planted in (a grid's rows, an
    /// alameda's pairs) and nothing else moves. `start` in [0, 1) offsets
    /// the total.
    fn thin_trees(&mut self, start: f32) {
        let keep = Settlement::tree_keep(self.kind);
        if keep >= 1.0 {
            return;
        }
        let mut seen = 0.0_f32;
        self.structures.retain(|s| {
            if !s.kind.is_tree() || s.kind == StructureKind::Poplar {
                return true;
            }
            // The epsilon keeps a share that lands on a whole number from
            // rounding down.
            let at = |n: f32| (start + n * keep + 1e-4).floor();
            let stays = at(seen + 1.0) > at(seen);
            seen += 1.0;
            stays
        });
    }

    fn tree(&mut self, kind: StructureKind) -> Part {
        let height = match kind {
            StructureKind::Poplar => self.rng.range(14.0, 20.0),
            _ => self.rng.range(18.0, 28.0),
        };
        Part {
            kind,
            size: Vec2::new(1.0, 1.0),
            height,
        }
    }

    fn part(
        &mut self,
        kind: StructureKind,
        wide: (f32, f32),
        deep: (f32, f32),
        height: f32,
    ) -> Part {
        let size = Vec2::new(
            self.rng.range(wide.0, wide.1),
            self.rng.range(deep.0, deep.1),
        );
        Part { kind, size, height }
    }

    fn chance(&mut self, p: f32) -> bool {
        self.rng.unit() < p
    }

    /// Behind the patio, as seen from the gate.
    fn ahead(&self, near: f32, far: f32, spread: f32) -> Spot {
        Spot {
            around: self.hub,
            radius: (near, far),
            bearing: self.into.y.atan2(self.into.x),
            spread,
        }
    }

    /// Anywhere round the patio.
    fn round(&self, near: f32, far: f32) -> Spot {
        Spot {
            spread: PI,
            ..self.ahead(near, far, 0.0)
        }
    }

    /// The unit direction of the last approach, from the trunk's bend to
    /// the hub: the way a visitor rides toward the casco.
    fn lane(&self) -> Vec2 {
        let entry = self.tracks[0].points[1];
        (self.hub - entry) * (1.0 / self.hub.distance(entry).max(1e-6))
    }

    /// A spot `radius` meters (least, most) from the casco `casco`, on the
    /// far side of it along [`Self::lane`], within `spread` radians.
    fn behind(&self, casco: usize, radius: (f32, f32), spread: f32) -> Spot {
        let lane = self.lane();
        Spot {
            around: self.structures[casco].footprint.center(),
            radius,
            bearing: lane.y.atan2(lane.x),
            spread,
        }
    }

    /// Places a barn behind the casco `casco`: its near edge past the
    /// casco's far edge along the lane, and at least
    /// [`Settlement::BARN_ROAD_CLEARANCE`] from every road. A try that
    /// breaks either rule is taken back, its track too.
    fn place_behind(&mut self, part: Part, casco: usize, radius: (f32, f32)) -> Option<usize> {
        let lane = self.lane();
        let center = self.structures[casco].footprint.center();
        let corners = |fp: Aabb| {
            [
                fp.min,
                fp.max,
                Vec2::new(fp.min.x, fp.max.y),
                Vec2::new(fp.max.x, fp.min.y),
            ]
            .map(|c| (c - center).dot(lane))
        };
        let casco_end = corners(self.structures[casco].footprint)
            .into_iter()
            .fold(f32::MIN, f32::max);
        for _ in 0..Self::TRIES {
            let spot = self.behind(casco, radius, 0.7);
            let angle = spot.bearing + self.rng.range(-spot.spread, spot.spread);
            let r = self.rng.range(radius.0, radius.1);
            let at = center + Vec2::new(angle.cos(), angle.sin()) * r;
            let (structures, tracks) = (self.structures.len(), self.tracks.len());
            let Some(index) = self.place_at(part, at) else {
                continue;
            };
            let fp = self.structures[index].footprint;
            let near = corners(fp).into_iter().fold(f32::MAX, f32::min);
            if near >= casco_end
                && self
                    .ground
                    .off_roads_by(fp, Settlement::BARN_ROAD_CLEARANCE)
            {
                return Some(index);
            }
            self.structures.truncate(structures);
            self.tracks.truncate(tracks);
        }
        None
    }

    /// Places `part` somewhere in `spot`, facing the hub, with its branch
    /// track when it has a door. Returns its index.
    fn place(&mut self, part: Part, spot: Spot) -> Option<usize> {
        for _ in 0..Self::TRIES {
            let angle = spot.bearing + self.rng.range(-spot.spread, spot.spread);
            let r = self.rng.range(spot.radius.0, spot.radius.1);
            let at = spot.around + Vec2::new(angle.cos(), angle.sin()) * r;
            if let Some(index) = self.place_at(part, at) {
                return Some(index);
            }
        }
        None
    }

    /// Places `part` centered on `at`, if it fits there.
    fn place_at(&mut self, part: Part, at: Vec2) -> Option<usize> {
        self.place_at_by(part, at, false)
    }

    /// [`Self::place_at`]; `lane` trees of the alameda may stand 2 m from
    /// the cell's fences and on the road's verge.
    fn place_at_by(&mut self, part: Part, at: Vec2, lane: bool) -> Option<usize> {
        let front = facing(self.hub - at);
        let size = match front {
            Side::North | Side::South => part.size,
            Side::East | Side::West => Vec2::new(part.size.y, part.size.x),
        };
        let structure = Structure {
            kind: part.kind,
            footprint: Aabb::from_center_size(at, size),
            height: part.height,
            front,
            variant: self.rng.next() >> 8,
        };
        if !self.fits_by(structure.footprint, lane) {
            return None;
        }
        if part.kind.is_reached() {
            let door = structure.door();
            if !self.branch_ok(self.hub, door, structure.footprint) {
                return None;
            }
            self.tracks.push(Track {
                points: vec![self.hub, door],
            });
        }
        self.structures.push(structure);
        Some(self.structures.len() - 1)
    }

    /// Whether `footprint` stands clear of the ground's keep-outs, the
    /// patio, every other footprint and every track.
    /// The alameda's trees (`lane`) get the looser ground.
    fn fits_by(&self, footprint: Aabb, lane: bool) -> bool {
        let corridor = footprint.expanded(Settlement::CORRIDOR);
        let ground = if lane {
            self.ground.area_ok_by(footprint, 2.0, false)
        } else {
            self.ground.area_ok(footprint)
        };
        ground
            && !self.patio.intersects(&footprint)
            && self
                .structures
                .iter()
                .all(|s| !s.footprint.expanded(Settlement::GAP).intersects(&footprint))
            && self
                .tracks
                .iter()
                .flat_map(Track::segments)
                .all(|(a, b)| corridor.segment_overlap(a, b).is_none())
    }

    /// Whether a branch may run from `a` to `b`, clear of every footprint
    /// and of `own` (the one it leads to).
    fn branch_ok(&self, a: Vec2, b: Vec2, own: Aabb) -> bool {
        self.ground.track_ok(a, b, None)
            && self
                .structures
                .iter()
                .map(|s| s.footprint)
                .chain([own])
                .all(|f| {
                    f.expanded(Settlement::CORRIDOR)
                        .segment_overlap(a, b)
                        .is_none()
                })
    }

    /// Everything the compound covers.
    fn bounds(&self) -> Aabb {
        let mut bounds = self.patio;
        let mut grow = |area: Aabb| {
            bounds = Aabb::new(
                Vec2::new(bounds.min.x.min(area.min.x), bounds.min.y.min(area.min.y)),
                Vec2::new(bounds.max.x.max(area.max.x), bounds.max.y.max(area.max.y)),
            );
        };
        for s in &self.structures {
            grow(s.footprint);
        }
        for p in self.tracks.iter().flat_map(|h| h.points.iter()) {
            grow(Aabb::from_center_size(
                *p,
                Vec2::new(2.0, 2.0) * Track::HALF_WIDTH,
            ));
        }
        bounds
    }

    fn finish(self, gate: u32, seed: u32) -> Settlement {
        Settlement {
            kind: self.kind,
            gate,
            hub: self.hub,
            patio: self.patio,
            bounds: self.bounds(),
            structures: self.structures,
            tracks: self.tracks,
            seed,
        }
    }
}

/// The side of a footprint facing along `toward`.
fn facing(toward: Vec2) -> Side {
    if toward.x.abs() > toward.y.abs() {
        if toward.x > 0.0 {
            Side::East
        } else {
            Side::West
        }
    } else if toward.y > 0.0 {
        Side::North
    } else {
        Side::South
    }
}

/// Thin boxes along the four sides of a corral, with
/// [`Settlement::CORRAL_GATE`] meters left open in the middle of its
/// `front`.
fn corral_rails(fp: Aabb, front: Side) -> Vec<Aabb> {
    let t = Settlement::CORRAL_RAIL / 2.0;
    let (lo, hi) = (fp.min, fp.max);
    let sides = [
        (Side::South, Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y)),
        (Side::North, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, hi.y)),
        (Side::West, Vec2::new(lo.x, lo.y), Vec2::new(lo.x, hi.y)),
        (Side::East, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
    ];
    let rail = |a: Vec2, b: Vec2| {
        Aabb::new(
            Vec2::new(a.x.min(b.x) - t, a.y.min(b.y) - t),
            Vec2::new(a.x.max(b.x) + t, a.y.max(b.y) + t),
        )
    };
    let mut rails = Vec::new();
    for (side, a, b) in sides {
        if side == front {
            let mid = (a + b) * 0.5;
            let half = (b - a) * (Settlement::CORRAL_GATE / 2.0 / a.distance(b));
            rails.push(rail(a, mid - half));
            rails.push(rail(mid + half, b));
        } else {
            rails.push(rail(a, b));
        }
    }
    rails
}

/// One kept-out shape: an area, or a strip `half` meters to either side of
/// a segment.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Area(Aabb),
    Strip(Vec2, Vec2, f32),
}

impl Shape {
    /// How far `p` is outside it (0 inside).
    fn distance(&self, p: Vec2) -> f32 {
        match *self {
            Shape::Area(area) => area.closest_point(p).distance(p),
            Shape::Strip(a, b, half) => (segment_distance(p, a, b) - half).max(0.0),
        }
    }

    fn bounds(&self) -> Aabb {
        match *self {
            Shape::Area(area) => area,
            Shape::Strip(a, b, half) => Aabb::new(
                Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                Vec2::new(a.x.max(b.x), a.y.max(b.y)),
            )
            .expanded(half),
        }
    }
}

/// The ground wild flora keeps off: every settlement's footprints, patios
/// and tracks, indexed on a uniform grid so a point's check only looks at
/// the shapes near it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeepOut {
    shapes: Vec<Shape>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl KeepOut {
    /// Side of one index cell, in meters.
    pub const CELL: f32 = 32.0;
    /// The widest margin a check can ask for, in meters.
    pub const MAX_MARGIN: f32 = 8.0;

    fn push(&mut self, shape: Shape) {
        let index = self.shapes.len() as u32;
        let reach = shape.bounds().expanded(Self::MAX_MARGIN);
        let (x0, y0) = cell_of(reach.min);
        let (x1, y1) = cell_of(reach.max);
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.cells.entry((x, y)).or_default().push(index);
            }
        }
        self.shapes.push(shape);
    }

    /// The shapes indexed in `p`'s cell.
    fn nearby(&self, p: Vec2) -> &[u32] {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return &[];
        }
        self.cells.get(&cell_of(p)).map_or(&[], Vec::as_slice)
    }

    /// Whether `p` is more than `margin` meters (at most
    /// [`Self::MAX_MARGIN`]) from every kept-out shape.
    pub fn is_clear(&self, p: Vec2, margin: f32) -> bool {
        let margin = margin.min(Self::MAX_MARGIN);
        self.nearby(p)
            .iter()
            .all(|&i| self.shapes[i as usize].distance(p) > margin)
    }

    /// How many shapes a check at `p` looks at: what [`Self::is_clear`]
    /// costs.
    pub fn scanned(&self, p: Vec2) -> usize {
        self.nearby(p).len()
    }
}

fn cell_of(p: Vec2) -> (i32, i32) {
    (
        (p.x / KeepOut::CELL).floor() as i32,
        (p.y / KeepOut::CELL).floor() as i32,
    )
}

/// Mixed into the terrain seed for the settlements.
const SETTLEMENT_SEED: u32 = 0x5e77_1e05;
/// Mixed into the terrain seed for the forced estancias.
const RANCH_SEED: u32 = 0x7a2c_4e11;
/// Mixed into the terrain seed for the roadside wrecks.
const WRECK_SEED: u32 = 0x3c4a_7a5e;

impl Site<'_> {
    /// A handful of abandoned wagons on road verges, seeded per roadside
    /// fence run: by its chance, the first span from a seeded one whose
    /// verge holds a wagon along the road, between the road's edge and the
    /// fence, clear of gates, openings, signposts and crossings, rails,
    /// towns, settlements and the other wrecks.
    fn roadside_wrecks(&self, seed: u32, settlements: &[Settlement]) -> Vec<RoadsideWreck> {
        let Some(roads) = self.terrain.roads.as_deref() else {
            return Vec::new();
        };
        let road_parts = roads.obstacles();
        let mut wrecks: Vec<RoadsideWreck> = Vec::new();
        for (index, run) in self.fences.runs.iter().enumerate() {
            if wrecks.len() >= SettlementLayout::ROADSIDE_WRECKS {
                break;
            }
            if !run.roadside || run.posts.len() < 4 {
                continue;
            }
            let mut rng = Rng::new(seed ^ WRECK_SEED, index as u32);
            if rng.unit() >= SettlementLayout::ROADSIDE_CHANCE {
                continue;
            }
            // Spans inside the run (not at its ends, by a crossing or a
            // gate), from a seeded one on.
            let spans = run.posts.len() - 3;
            let first = rng.below(spans);
            let variant = rng.next() >> 8;
            let found = (0..spans).find_map(|k| {
                let k = 1 + (first + k) % spans;
                let (a, b) = (run.posts[k], run.posts[k + 1]);
                let wreck = self.on_verge(roads, a, b, variant)?;
                let fp = wreck.footprint();
                let clear = self
                    .fences
                    .gates
                    .iter()
                    .all(|g| g.at.distance(wreck.at) > 25.0)
                    && self
                        .fences
                        .openings
                        .iter()
                        .all(|o| o.at.distance(wreck.at) > 25.0)
                    && road_parts.iter().all(|o| !o.expanded(3.0).intersects(&fp))
                    && self.rails.iter().all(|&(p, q)| {
                        segment_distance(wreck.at, p, q) > RoadNetwork::RAIL_CLEARANCE
                    })
                    && self.bands.iter().all(|band| !band.intersects(&fp))
                    && settlements
                        .iter()
                        .all(|s| !s.bounds.expanded(30.0).intersects(&fp))
                    && wrecks
                        .iter()
                        .all(|w| w.at.distance(wreck.at) >= SettlementLayout::ROADSIDE_SPACING);
                clear.then_some(wreck)
            });
            wrecks.extend(found);
        }
        wrecks
    }

    /// A wagon along the fence span `a`–`b`, on the road's side of it with
    /// its near side a little off the road's edge, if the whole of it lies
    /// in the verge.
    fn on_verge(
        &self,
        roads: &RoadNetwork,
        a: Vec2,
        b: Vec2,
        variant: u32,
    ) -> Option<RoadsideWreck> {
        let length = a.distance(b);
        if length < 1.0 {
            return None;
        }
        let along = (b - a) * (1.0 / length);
        let mid = (a + b) * 0.5;
        let across = Vec2::new(-along.y, along.x);
        let toward = if roads.distance(mid + across) < roads.distance(mid - across) {
            across
        } else {
            across * -1.0
        };
        let deep = Settlement::WRECK_SIZE.1;
        let out = RoadNetwork::HALF_WIDTH + SettlementLayout::VERGE_GAP + deep / 2.0;
        let wreck = RoadsideWreck {
            at: mid + toward * (roads.distance(mid) - out),
            along,
            variant,
        };
        let inside = wreck.corners().iter().all(|&c| {
            let d = roads.distance(c);
            let verge = RoadNetwork::HALF_WIDTH + SettlementLayout::VERGE_GAP * 0.5
                ..=FenceLayout::ROADSIDE - 0.3;
            verge.contains(&d)
        });
        inside.then_some(wreck)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fence::tests::{fenced, reached};
    use crate::{ChunkCoord, FloraSite, StaticObstacles, World};

    /// The towns of the fence fixture that get an estancia nearby, as the
    /// game asks for them: Eduardo Castex and General Pico.
    fn ranch_towns(towns: &[TownLayout]) -> Vec<usize> {
        ["Eduardo Castex", "General Pico"]
            .iter()
            .filter_map(|name| towns.iter().position(|t| t.name == *name))
            .collect()
    }

    /// The fence fixture's world with its settlements laid and settled, with
    /// the game's estancias near Castex and Pico (the seeded mix alone may
    /// draw none).
    pub(crate) fn settled() -> (
        World,
        Vec<TownLayout>,
        RailLine,
        FenceLayout,
        SettlementLayout,
    ) {
        let (mut world, towns, line, fences) = fenced();
        let started = std::time::Instant::now();
        let ranches = ranch_towns(&towns);
        assert_eq!(ranches.len(), 2);
        let layout =
            SettlementLayout::build_with_ranches(&world.terrain, &towns, &line, &fences, &ranches);
        let built = started.elapsed();
        let count = world.settle(&layout);
        let open = fences.gates.iter().filter(|g| g.open).count();
        println!(
            "settlements: {} behind {open} open gates ({} huts, {} outposts, {} farms, \
             {} ranchs), {count} obstacles; laid in {built:?}",
            layout.settlements.len(),
            layout.count(SettlementKind::Hut),
            layout.count(SettlementKind::Outpost),
            layout.count(SettlementKind::Farm),
            layout.count(SettlementKind::Ranch),
        );
        // A generous bound, like the fences': laying them stays cheap at
        // startup.
        assert!(built.as_secs_f32() < 10.0, "laid in {built:?}");
        (world, towns, line, fences, layout)
    }

    /// Unit direction from the road through `gate` into its field, by the
    /// exact road distance.
    fn into_field(roads: &RoadNetwork, gate: &crate::Gate) -> Vec2 {
        let out = Vec2::new(-gate.along.y, gate.along.x);
        if roads.distance(gate.at + out) > roads.distance(gate.at) {
            out
        } else {
            out * -1.0
        }
    }

    /// Points along every track of `s`, `step` meters apart.
    fn track_points(s: &Settlement, step: f32) -> Vec<Vec2> {
        s.tracks
            .iter()
            .flat_map(Track::segments)
            .flat_map(|(a, b)| {
                let n = (a.distance(b) / step).ceil().max(1.0) as usize;
                (0..=n).map(move |k| a + (b - a) * (k as f32 / n as f32))
            })
            .collect()
    }

    /// Points round the edge of `area`, about a meter apart.
    fn edge_points(area: Aabb) -> Vec<Vec2> {
        let size = area.size();
        let (nx, ny) = (size.x.ceil() as usize, size.y.ceil() as usize);
        let mut points = Vec::new();
        for k in 0..=nx {
            let x = area.min.x + size.x * (k as f32 / nx as f32);
            points.extend([Vec2::new(x, area.min.y), Vec2::new(x, area.max.y)]);
        }
        for k in 0..=ny {
            let y = area.min.y + size.y * (k as f32 / ny as f32);
            points.extend([Vec2::new(area.min.x, y), Vec2::new(area.max.x, y)]);
        }
        points
    }

    #[test]
    fn settlements_are_laid_the_same_way_every_time_and_sparsely() {
        let (world, towns, line, fences, layout) = settled();
        assert_eq!(
            layout,
            SettlementLayout::build_with_ranches(
                &world.terrain,
                &towns,
                &line,
                &fences,
                &ranch_towns(&towns)
            )
        );
        // The plain build, without forced estancias, is seeded too.
        let plain = SettlementLayout::build(&world.terrain, &towns, &line, &fences);
        assert_eq!(
            plain,
            SettlementLayout::build(&world.terrain, &towns, &line, &fences)
        );
        assert!(!plain.settlements.is_empty());
        let open = fences.gates.iter().filter(|g| g.open).count();
        let total = layout.settlements.len();
        // Only some open gates lead to a settlement, and estancias are few.
        assert!(total >= 10, "{total}");
        assert!(total * 2 < open, "{total} of {open}");
        let count = |k| layout.count(k);
        use SettlementKind::*;
        assert!(count(Ranch) >= 1, "{}", count(Ranch));
        assert!(count(Ranch) * 3 <= total, "{}", count(Ranch));
        for k in [Hut, Outpost, Farm] {
            assert!(count(k) >= 1, "{k:?}");
        }
        // Another seed lays another countryside.
        let mut other = world.terrain.clone();
        other.seed ^= 0x55;
        assert_ne!(
            layout
                .settlements
                .iter()
                .map(|s| s.gate)
                .collect::<Vec<_>>(),
            SettlementLayout::build_with_ranches(
                &other,
                &towns,
                &line,
                &fences,
                &ranch_towns(&towns)
            )
            .settlements
            .iter()
            .map(|s| s.gate)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_settlement_lies_behind_an_open_gate_on_its_field_side() {
        let (world, _, _, fences, layout) = settled();
        let roads = world.terrain.roads.clone().unwrap();
        let mut cells: Vec<Aabb> = Vec::new();
        for s in &layout.settlements {
            let gate = &fences.gates[s.gate as usize];
            assert!(gate.open, "{gate:?}");
            let into = into_field(&roads, gate);
            let start = s.trunk().points[0];
            assert!(start.distance(gate.at) < 1.0, "{start:?} {gate:?}");
            assert!((start - gate.at).dot(into) > 0.0);
            assert!((s.hub - gate.at).dot(into) > 30.0, "{:?}", s.hub);
            assert_eq!(*s.trunk().points.last().unwrap(), s.hub);
            let cell = fences.field_of(gate.at + into * 3.0).unwrap();
            assert!(cell.contains(&s.bounds), "{cell:?} {:?}", s.bounds);
            assert!(!fences.is_descampado(s.hub));
            // One settlement per field, and well apart.
            assert!(!cells.contains(&cell));
            cells.push(cell);
            for t in &layout.settlements {
                if t != s {
                    assert!(t.hub.distance(s.hub) >= Settlement::MIN_SPACING);
                    assert!(!t.bounds.intersects(&s.bounds));
                }
            }
        }
    }

    #[test]
    fn settlements_keep_clear_of_roads_rails_towns_and_fences() {
        let (world, towns, line, fences, layout) = settled();
        let roads = world.terrain.roads.clone().unwrap();
        let rails: Vec<(Vec2, Vec2)> = line
            .full_path(&towns)
            .points()
            .windows(2)
            .map(|w| (w[0], w[1]))
            .chain(line.segments().map(|s| (s.start, s.end)))
            .collect();
        let wire = StaticObstacles::new(fences.obstacles());
        for s in &layout.settlements {
            let cell = fences.field_of(s.hub).unwrap();
            let near_rails: Vec<(Vec2, Vec2)> = rails
                .iter()
                .copied()
                .filter(|&(a, b)| s.bounds.expanded(100.0).segment_overlap(a, b).is_some())
                .collect();
            for st in &s.structures {
                let fp = st.footprint;
                // The alameda reaches the gate: it keeps 2 m off the fences
                // and may stand on the road's verge.
                let lane = st.kind == StructureKind::Poplar;
                assert!(
                    cell.expanded(if lane {
                        -2.0
                    } else {
                        -Settlement::FENCE_CLEARANCE
                    })
                    .contains(&fp),
                    "{st:?}"
                );
                assert!(wire.near(fp.expanded(1.0)).is_empty(), "{st:?}");
                for t in &towns {
                    assert!(
                        !t.leveled()
                            .expanded(Settlement::TOWN_CLEARANCE)
                            .intersects(&fp)
                    );
                }
                for p in edge_points(fp) {
                    assert!(
                        lane || roads.near(p, Settlement::ROAD_CLEARANCE - 0.01).is_none(),
                        "{st:?} at {p:?}"
                    );
                    for &(a, b) in &near_rails {
                        assert!(segment_distance(p, a, b) >= Settlement::RAIL_CLEARANCE - 0.01);
                    }
                }
            }
            // The tracks stay in the field, off the road and the rail, and
            // cross no wire: the trunk passes the gate's open leaves.
            let gate = fences.gates[s.gate as usize].at;
            for p in track_points(s, 0.5) {
                assert!(cell.contains_point(p), "{p:?}");
                assert!(
                    wire.near(Aabb::from_center_size(p, Vec2::new(0.6, 0.6)))
                        .is_empty(),
                    "a track crosses a fence at {p:?}"
                );
                if p.distance(gate) > Settlement::GATE_APPROACH {
                    assert!(
                        cell.expanded(-Settlement::TRACK_FENCE_CLEARANCE)
                            .contains_point(p)
                    );
                    assert!(
                        roads
                            .near(p, Settlement::TRACK_ROAD_CLEARANCE - 0.01)
                            .is_none(),
                        "{p:?}"
                    );
                }
                for &(a, b) in &near_rails {
                    assert!(segment_distance(p, a, b) >= Settlement::RAIL_CLEARANCE - 0.01);
                }
            }
        }
    }

    #[test]
    fn buildings_overlap_neither_each_other_nor_the_tracks() {
        let (_, _, _, _, layout) = settled();
        for s in &layout.settlements {
            for (i, a) in s.structures.iter().enumerate() {
                assert!(!s.patio.intersects(&a.footprint), "{a:?}");
                for b in &s.structures[i + 1..] {
                    assert!(
                        !a.footprint
                            .expanded(Settlement::GAP - 0.01)
                            .intersects(&b.footprint),
                        "{a:?} {b:?}"
                    );
                }
                for (p, q) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(
                        a.footprint
                            .expanded(Track::HALF_WIDTH)
                            .segment_overlap(p, q)
                            .is_none(),
                        "{a:?} on the track {p:?}-{q:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn tracks_lead_from_the_gate_to_every_door() {
        let (_, _, _, _, layout) = settled();
        for s in &layout.settlements {
            // Every branch leaves from the hub, where the trunk ends.
            for h in &s.tracks[1..] {
                assert_eq!(h.points[0], s.hub);
            }
            for st in s.structures.iter().filter(|st| st.kind.is_reached()) {
                let door = st.door();
                assert!(
                    s.tracks[1..]
                        .iter()
                        .any(|h| h.points.last().unwrap().distance(door) < 1e-3),
                    "no track to {st:?}"
                );
            }
            assert_eq!(
                s.tracks.len(),
                1 + s
                    .structures
                    .iter()
                    .filter(|st| st.kind.is_reached())
                    .count()
            );
            let houses = s.structures.iter().filter(|st| st.kind.is_house()).count();
            let main = s
                .structures
                .iter()
                .filter(|st| matches!(st.kind, StructureKind::MainHouse(_)))
                .count();
            assert!(houses >= 1 && main == 1, "{s:?}");
        }
    }

    #[test]
    fn each_kind_has_the_buildings_of_its_time() {
        let (_, _, _, _, layout) = settled();
        use StructureKind::*;
        for s in &layout.settlements {
            let n = |k| s.of(k).count();
            let style = s
                .structures
                .iter()
                .find_map(|st| match st.kind {
                    MainHouse(style) => Some(style),
                    _ => None,
                })
                .unwrap();
            match s.kind {
                SettlementKind::Hut => {
                    assert!(matches!(
                        style,
                        HouseStyle::AdobeThatch | HouseStyle::AdobeIron
                    ));
                    assert_eq!(n(Barn) + n(Windpump), 0, "{s:?}");
                }
                SettlementKind::Outpost => {
                    assert!(n(Corral) == 1 && n(Trough) <= 1, "{s:?}");
                    assert!(n(Windpump) + n(Well) == 1, "{s:?}");
                }
                SettlementKind::Farm => {
                    assert!(style != HouseStyle::GalleryHouse);
                    assert!(n(Barn) == 1 && n(Windpump) == 1, "{s:?}");
                }
                SettlementKind::Ranch => {
                    assert_eq!(style, HouseStyle::GalleryHouse);
                    let workers = s
                        .structures
                        .iter()
                        .filter(|st| matches!(st.kind, WorkerHouse(_)))
                        .count();
                    assert!((2..=4).contains(&workers), "{workers}");
                    assert!((1..=2).contains(&n(Barn)));
                    assert!(n(Corral) >= 1 && n(Windpump) == 1);
                    assert!(n(Eucalyptus) + n(Poplar) >= 10, "{s:?}");
                    let size = s.bounds.size();
                    assert!(
                        size.x * size.y >= Settlement::RANCH_AREA,
                        "{} m²",
                        size.x * size.y
                    );
                }
            }
        }
    }

    #[test]
    fn a_rider_rides_in_through_the_gate_to_every_door_and_into_the_corrals() {
        let (mut world, towns, line, fences, layout) = settled();
        let flora = FloraSite::new(world.config, &world.terrain, &line)
            .with_track(&line.full_path(&towns))
            .with_fences(&fences)
            .with_settlements(&layout);
        world.plant(&flora);
        let roads = world.terrain.roads.clone().unwrap();
        let mut seen_kinds = HashSet::new();
        let mut checked = 0;
        for s in &layout.settlements {
            if !seen_kinds.insert(s.kind) && checked >= 8 {
                continue;
            }
            let gate = &fences.gates[s.gate as usize];
            let from = gate.at - into_field(&roads, gate) * 4.0;
            let window = Aabb::new(
                Vec2::new(s.bounds.min.x.min(from.x), s.bounds.min.y.min(from.y)),
                Vec2::new(s.bounds.max.x.max(from.x), s.bounds.max.y.max(from.y)),
            )
            .expanded(20.0);
            let window = Aabb::new(
                Vec2::new(window.min.x.floor(), window.min.y.floor()),
                Vec2::new(window.max.x.floor(), window.max.y.floor()),
            );
            let (seen, w) = reached(&world, window, &[from]);
            let at = |p: Vec2| {
                let q = p - window.min;
                seen[q.y as usize * w + q.x as usize]
            };
            for st in &s.structures {
                if st.kind.is_reached() {
                    assert!(at(st.door()), "cannot ride to {st:?} of {:?}", s.kind);
                }
                if st.kind == StructureKind::Corral {
                    assert!(at(st.footprint.center()), "cannot ride into {st:?}");
                    assert_eq!(st.obstacles().len(), 5);
                }
            }
            checked += 1;
        }
        assert!(seen_kinds.len() == 4 && checked >= 8, "{seen_kinds:?}");
    }

    #[test]
    fn flora_keeps_off_the_settlements_through_a_cheap_index() {
        let (world, towns, line, fences, layout) = settled();
        let flora = FloraSite::new(world.config, &world.terrain, &line)
            .with_track(&line.full_path(&towns))
            .with_fences(&fences)
            .with_settlements(&layout);
        let trees = flora.all_trees();
        assert!(trees.len() > 5_000);
        let near = |p: Vec2, margin: f32| {
            layout
                .touching(Aabb::from_center_size(p, Vec2::new(1.0, 1.0)).expanded(margin))
                .any(|s| {
                    s.structures
                        .iter()
                        .any(|st| st.footprint.closest_point(p).distance(p) <= margin)
                        || s.tracks
                            .iter()
                            .flat_map(Track::segments)
                            .any(|(a, b)| segment_distance(p, a, b) <= Track::HALF_WIDTH + margin)
                })
        };
        for t in &trees {
            assert!(!near(t.at, FloraSite::TREE_SETTLEMENT_MARGIN), "{t:?}");
        }
        let mut tufts = 0;
        let size = world.config.chunk_size;
        for s in layout.settlements.iter().take(12) {
            for p in [s.hub, s.bounds.min, s.bounds.max] {
                for t in flora.chunk(ChunkCoord::containing(p, size)).tufts {
                    assert!(!near(t.at, FloraSite::TUFT_SETTLEMENT_MARGIN), "{t:?}");
                    tufts += 1;
                }
            }
            // On the tracks and in the buildings nothing grows; far off,
            // anything may.
            for p in track_points(s, 2.0) {
                assert!(!layout.is_clear(p, 0.0) && !flora.off_settlements(p, 0.0));
            }
            for st in &s.structures {
                assert!(!layout.is_clear(st.footprint.center(), 0.0));
            }
        }
        assert!(tufts > 1_000, "{tufts}");
        // The check only looks at a handful of nearby shapes, wherever.
        let busiest = layout
            .settlements
            .iter()
            .flat_map(|s| track_points(s, 4.0))
            .map(|p| layout.keep_out().scanned(p))
            .max()
            .unwrap();
        assert!(busiest <= 80, "{busiest}");
        let far = Vec2::new(1e6, 1e6);
        assert!(layout.is_clear(far, 4.0) && layout.keep_out().scanned(far) == 0);
    }

    #[test]
    fn chunks_share_out_the_settlements_once_each() {
        let (world, _, _, _, layout) = settled();
        let size = world.config.chunk_size;
        let mut owned = 0;
        let mut tracks = 0;
        let bounds = world.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let area = ChunkCoord::new(x, y).bounds(size);
                owned += layout.in_area(area).count();
                tracks += layout.tracks_in(area).count();
                for s in layout.in_area(area) {
                    assert!(layout.touching(area).any(|t| t == s));
                }
            }
        }
        assert_eq!(owned, layout.settlements.len());
        assert_eq!(
            tracks,
            layout
                .settlements
                .iter()
                .map(|s| s.tracks.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn every_structure_blocks_up_to_its_own_height() {
        let (world, _, _, _, layout) = settled();
        let wreck_parts: Vec<Aabb> = layout
            .roadside
            .iter()
            .flat_map(RoadsideWreck::obstacles)
            .collect();
        let structures = layout.settlements.iter().flat_map(|s| s.structures.iter());
        let mut checked = 0;
        for s in structures {
            for o in s.obstacles() {
                let found = |clearance| world.obstacles.near_above(o, clearance).contains(&&o);
                assert!(found(s.height - 0.01), "{:?} under its top", s.kind);
                // Another footprint may share the box (a wreck's own, say).
                if !wreck_parts.contains(&o) {
                    assert!(!found(s.height), "{:?} at its top", s.kind);
                }
                checked += 1;
            }
        }
        assert!(checked > 0);
        // Roadside wrecks have no height: they are never cleared.
        for o in &wreck_parts {
            assert!(world.obstacles.near_above(*o, 1e6).contains(&o));
        }
    }

    #[test]
    fn a_corral_is_open_only_at_its_gate() {
        let fp = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(20.0, 20.0));
        let corral = Structure {
            kind: StructureKind::Corral,
            footprint: fp,
            height: 1.6,
            front: Side::North,
            variant: 0,
        };
        let rails = corral.obstacles();
        assert_eq!(rails.len(), 5);
        let blocked = |p: Vec2| rails.iter().any(|r| r.contains_point(p));
        assert!(!blocked(Vec2::new(10.0, 20.0)), "the gate is open");
        assert!(blocked(Vec2::new(5.0, 20.0)) && blocked(Vec2::new(10.0, 0.0)));
        assert!(blocked(Vec2::new(0.0, 10.0)) && blocked(Vec2::new(20.0, 10.0)));
        assert_eq!(
            corral.door(),
            Vec2::new(10.0, 20.0 + Settlement::DOOR_REACH)
        );
        assert_eq!(StructureKind::Windpump.slot(), Some("windpump"));
        assert!(StructureKind::Eucalyptus.is_tree());
    }

    #[test]
    fn every_slot_name_is_listed_once_as_a_plain_folder_name() {
        use StructureKind::*;
        let kinds = [
            MainHouse(HouseStyle::AdobeThatch),
            WorkerHouse(HouseStyle::BrickParapet),
            Barn,
            Corral,
            Windpump,
            Tank,
            Trough,
            Well,
            Haystack,
            HitchingPost,
            Cart,
            WreckedWagon,
            Eucalyptus,
            Poplar,
        ];
        let mut slots: Vec<&str> = kinds.iter().filter_map(|k| k.slot()).collect();
        let mut listed = StructureKind::SLOTS.to_vec();
        slots.sort_unstable();
        listed.sort_unstable();
        assert_eq!(slots, listed, "every kind with a slot is listed, once");
        for slot in StructureKind::SLOTS {
            assert!(
                !slot.is_empty()
                    && slot
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{slot}"
            );
        }
        // The buildings and the corral are built procedurally only.
        assert!(Barn.slot().is_none() && Corral.slot().is_none() && Tank.slot().is_none());
    }

    #[test]
    fn tree_spots_block_with_their_trunks_only() {
        let (world, _, _, _, layout) = settled();
        let trees: Vec<&Structure> = layout
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
            .collect();
        assert!(!trees.is_empty());
        let collider = crate::Circle::new(crate::Rider::COLLIDER_RADIUS);
        for tree in trees {
            let at = tree.footprint.center();
            let trunk = tree.obstacles();
            assert_eq!(trunk.len(), 1, "{tree:?}");
            let r = tree.trunk_radius();
            assert!((trunk[0].size().x - 2.0 * r).abs() < 1e-3);
            // A rider walking at the trunk is pushed clear of the trunk box
            // itself, not just off its middle.
            let pushed = world.resolve(collider, at, |_| ());
            let clear = r + crate::Rider::COLLIDER_RADIUS - 1e-3;
            assert!(
                (pushed.x - at.x).abs() >= clear || (pushed.y - at.y).abs() >= clear,
                "{tree:?} pushed to {pushed:?}"
            );
            // No trunk stands on a track: the ruts and the alameda between
            // its poplars stay open to the rider.
            for s in &layout.settlements {
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(
                        trunk[0]
                            .expanded(crate::Rider::COLLIDER_RADIUS)
                            .segment_overlap(a, b)
                            .is_none(),
                        "{tree:?} blocks a track"
                    );
                }
            }
        }
        // `World::settle` counts the trunks with the other obstacles.
        assert_eq!(
            layout.obstacles().count(),
            layout
                .settlements
                .iter()
                .flat_map(|s| s.structures.iter())
                .map(|s| s.obstacles().len())
                .sum::<usize>()
                + 3 * layout.roadside.len()
        );
        let mut bare = World::with_towns(world.config, crate::Terrain::default(), &[]);
        let trunks = layout
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
            .count();
        assert!(bare.settle(&layout) >= trunks + layout.settlements.len());
        // Radii stay in a trunk's range at any height.
        for (kind, lo, hi) in [
            (StructureKind::Eucalyptus, 0.2, 0.45),
            (StructureKind::Poplar, 0.12, 0.25),
        ] {
            for height in [1.0, 20.0, 80.0] {
                let r = Structure {
                    kind,
                    height,
                    ..*layout.settlements[0].structures.first().unwrap()
                }
                .trunk_radius();
                assert!((lo..=hi).contains(&r), "{kind:?} {height}: {r}");
            }
        }
        let r = Structure {
            kind: StructureKind::Eucalyptus,
            height: 25.0,
            ..*layout.settlements[0].structures.first().unwrap()
        }
        .trunk_radius();
        assert!((r - 0.4).abs() < 1e-5, "{r}");
    }

    #[test]
    fn abandoned_wagons_are_few_and_stay_off_roads_tracks_and_doors() {
        let (world, _, _, fences, layout) = settled();
        let roads = world.terrain.roads.as_deref().expect("roads laid");
        // In the yards: at most one per settlement, only at chacras and
        // estancias, clear of every track (so of every door) and patio.
        let mut yard = 0;
        for s in &layout.settlements {
            let wrecks: Vec<&Structure> = s.of(StructureKind::WreckedWagon).collect();
            assert!(wrecks.len() <= 1, "{s:?}");
            if !wrecks.is_empty() {
                assert!(matches!(
                    s.kind,
                    SettlementKind::Farm | SettlementKind::Ranch
                ));
            }
            for w in wrecks {
                yard += 1;
                assert!(!w.footprint.intersects(&s.patio));
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(
                        w.footprint
                            .expanded(Track::HALF_WIDTH)
                            .segment_overlap(a, b)
                            .is_none(),
                        "{w:?} on a track"
                    );
                }
                for other in s.structures.iter().filter(|o| o.kind.is_reached()) {
                    assert!(!w.footprint.contains_point(other.door()), "{w:?} at a door");
                }
            }
        }
        // On the verges: a handful, between the road's edge and the fence,
        // never on the road, a gate or a signpost, far apart.
        let verge = &layout.roadside;
        println!("wrecks: {yard} in yards, {} on verges", verge.len());
        assert!(verge.len() <= SettlementLayout::ROADSIDE_WRECKS);
        assert!(!verge.is_empty(), "the shipped map has long straight roads");
        let signposts = roads.obstacles();
        for w in verge {
            for c in w.corners().into_iter().chain([w.at]) {
                let d = roads.distance(c);
                assert!(d > RoadNetwork::HALF_WIDTH, "{w:?} on the road");
                assert!(d < FenceLayout::ROADSIDE, "{w:?} past the fence");
            }
            let fp = w.footprint();
            assert!(signposts.iter().all(|o| !o.intersects(&fp)), "{w:?}");
            assert!(
                fences
                    .gates
                    .iter()
                    .all(|g| !fp.expanded(10.0).contains_point(g.at))
            );
            // Its body blocks the rider, without reaching onto the road.
            for o in w.obstacles() {
                assert!(world.obstacles.contains(&o), "{w:?} does not block");
                for c in [
                    o.min,
                    o.max,
                    Vec2::new(o.min.x, o.max.y),
                    Vec2::new(o.max.x, o.min.y),
                ] {
                    assert!(
                        roads.distance(c) > RoadNetwork::HALF_WIDTH,
                        "{o:?} on the road"
                    );
                }
            }
            assert!(!layout.is_clear(w.at, 0.0), "flora keeps off it");
            assert!((w.along.length() - 1.0).abs() < 1e-4);
        }
        for (i, a) in verge.iter().enumerate() {
            for b in &verge[i + 1..] {
                assert!(a.at.distance(b.at) >= SettlementLayout::ROADSIDE_SPACING);
            }
        }
    }

    #[test]
    fn a_town_asked_for_an_estancia_gets_one_nearby_or_is_reported() {
        let (world, towns, line, fences) = fenced();
        let plain = SettlementLayout::build(&world.terrain, &towns, &line, &fences);
        let asked: Vec<usize> = (0..towns.len()).chain([towns.len() + 3]).collect();
        let layout =
            SettlementLayout::build_with_ranches(&world.terrain, &towns, &line, &fences, &asked);
        let again =
            SettlementLayout::build_with_ranches(&world.terrain, &towns, &line, &fences, &asked);
        assert_eq!(layout, again);
        // The missing town is reported, never silently dropped.
        assert!(layout.unplaced_ranches.contains(&(towns.len() + 3)));
        // The forced estancias are laid first, in the order asked: one per
        // placed town, ahead of the seeded mix.
        let placed: Vec<usize> = asked
            .iter()
            .copied()
            .filter(|t| !layout.unplaced_ranches.contains(t))
            .collect();
        assert!(!placed.is_empty(), "no town in range got its estancia");
        for (k, &t) in placed.iter().enumerate() {
            let town = &towns[t];
            let forced = &layout.settlements[k];
            assert_eq!(forced.kind, SettlementKind::Ranch, "{}", town.name);
            // It comes from the forced pass (its own seed), not the mix.
            assert!(!plain.settlements.contains(forced), "{}", town.name);
            // Gates are tried nearest first and a step's gates are never
            // tried again, so the step it was found in is the first whose
            // reach holds its gate: never the unbounded last one here.
            let gate = fences.gates[forced.gate as usize].at;
            let gap = town.leveled().closest_point(gate).distance(gate);
            let step = SettlementLayout::RANCH_REACH
                .iter()
                .position(|&reach| gap <= reach)
                .expect("the last step reaches anywhere");
            println!("{}: estancia {gap:.0} m out, step {step}", town.name);
            assert!(
                step < SettlementLayout::RANCH_REACH.len() - 1,
                "{}: {gap} m",
                town.name
            );
        }
        assert!(layout.count(SettlementKind::Ranch) >= towns.len().min(1));
        // Every settlement keeps its spacing, the forced ones too.
        for (i, a) in layout.settlements.iter().enumerate() {
            for b in &layout.settlements[i + 1..] {
                assert!(a.hub.distance(b.hub) >= Settlement::MIN_SPACING);
            }
        }
        // Asking for nothing lays the seeded mix as before.
        assert!(plain.unplaced_ranches.is_empty());
        assert_eq!(
            SettlementLayout::build_with_ranches(&world.terrain, &towns, &line, &fences, &[]),
            plain
        );
    }

    #[test]
    fn settlements_plant_their_trees_clear_of_everything() {
        let (_, _, _, _, layout) = settled();
        let trees: Vec<&Structure> = layout
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
            .collect();
        assert!(!trees.is_empty());
        for s in &layout.settlements {
            for (k, tree) in s
                .structures
                .iter()
                .enumerate()
                .filter(|(_, t)| t.kind.is_tree())
            {
                let at = tree.footprint.center();
                assert!(!s.patio.contains_point(at), "{s:?}");
                for (j, other) in s.structures.iter().enumerate().filter(|(j, _)| *j != k) {
                    assert!(
                        !other.footprint.expanded(2.9).intersects(&tree.footprint),
                        "{k} by {j}"
                    );
                }
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(segment_distance(at, a, b) >= 2.2, "a tree on a huella");
                }
            }
        }
    }

    /// Trees standing in the settlements of `kind` in the sample world,
    /// the alameda's poplars apart: they are never thinned.
    fn trees_of(layout: &SettlementLayout, kind: SettlementKind) -> usize {
        layout
            .settlements
            .iter()
            .filter(|s| s.kind == kind)
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree() && s.kind != StructureKind::Poplar)
            .count()
    }

    #[test]
    fn estancias_keep_half_and_chacras_seven_tenths_of_their_trees() {
        // Trees in the sample world before the thinning (re-counted
        // 2026-10-28, after the barns moved behind the casco and the alameda grew): the chacras
        // 21, the estancias 113, their poplars apart. The chacras' range is
        // wide because 21 trees round coarsely. No puesto or rancho of the sample
        // plants any (`a_compound_keeps_..._by_kind` covers those).
        let (_, _, _, _, layout) = settled();
        let farms = trees_of(&layout, SettlementKind::Farm) as f32 / 21.0;
        let ranches = trees_of(&layout, SettlementKind::Ranch) as f32 / 113.0;
        assert!((0.66..=0.80).contains(&farms), "chacras {farms}");
        assert!((0.45..=0.53).contains(&ranches), "estancias {ranches}");
    }

    #[test]
    fn a_compound_keeps_its_kinds_share_of_its_trees_spread_evenly() {
        let ground = Ground {
            roads: None,
            rails: Vec::new(),
            bands: Vec::new(),
            cell: Aabb::from_center_size(Vec2::ZERO, Vec2::new(2000.0, 2000.0)),
        };
        for (kind, kept, gap) in [
            (SettlementKind::Hut, 14, 2),
            (SettlementKind::Outpost, 14, 2),
            (SettlementKind::Farm, 14, 2),
            (SettlementKind::Ranch, 10, 3),
        ] {
            for start in [0.0, 0.3, 0.99] {
                let hub = Vec2::new(0.0, -300.0);
                let mut c = Compound::new(
                    &ground,
                    Rng::new(7, 9),
                    kind,
                    hub,
                    Vec2::new(0.0, 1.0),
                    [hub, hub, hub],
                );
                let spots: Vec<Vec2> = (0..20)
                    .map(|k| Vec2::new(k as f32 * 12.0 - 120.0, 0.0))
                    .collect();
                for &at in &spots {
                    let tree = c.tree(StructureKind::Eucalyptus);
                    assert!(c.place_at(tree, at).is_some());
                }
                c.thin_trees(start);
                let left: Vec<usize> = spots
                    .iter()
                    .enumerate()
                    .filter(|(_, at)| c.structures.iter().any(|s| s.footprint.center() == **at))
                    .map(|(k, _)| k)
                    .collect();
                // 20 trees at 0.49 keep 9 or 10 by the offset.
                let ok = left.len() == kept || (kept == 10 && left.len() == 9);
                assert!(ok, "{kind:?} {start}: {}", left.len());
                // Evenly: the ones that go are never in a run.
                for w in left.windows(2) {
                    assert!(w[1] - w[0] <= gap, "{kind:?} {left:?}");
                }
            }
        }
    }
    /// The unit direction of an estancia's last approach, from its trunk's
    /// bend toward the hub, and the casco.
    fn approach(s: &Settlement) -> (Vec2, Vec2, &Structure) {
        let entry = s.tracks[0].points[1];
        let lane = (s.hub - entry) * (1.0 / s.hub.distance(entry));
        let casco = s
            .structures
            .iter()
            .find(|st| matches!(st.kind, StructureKind::MainHouse(_)))
            .unwrap();
        (entry, lane, casco)
    }

    /// Where `p` stands against the trunk of `s`: meters along it from the
    /// hub, and meters to its left (positive) or right.
    fn along_the_trunk(s: &Settlement, p: Vec2) -> (f32, f32) {
        let [gate, bend, hub] = [s.tracks[0].points[0], s.tracks[0].points[1], s.hub];
        let mut best = (f32::MAX, 0.0, 0.0);
        let mut start = 0.0;
        for (a, b) in [(hub, bend), (bend, gate)] {
            let len = a.distance(b);
            let dir = (b - a) * (1.0 / len);
            let d = p - a;
            let along = d.dot(dir).clamp(0.0, len);
            let across = dir.x * d.y - dir.y * d.x;
            let off = (d - dir * along).length();
            if off < best.0 {
                best = (off, start + along, across);
            }
            start += len;
        }
        (best.1, best.2)
    }

    #[test]
    fn the_alameda_lines_both_sides_of_the_lane_up_to_the_gate() {
        let (_, _, _, _, layout) = settled();
        let ranches: Vec<_> = layout
            .settlements
            .iter()
            .filter(|s| s.kind == SettlementKind::Ranch)
            .collect();
        assert!(!ranches.is_empty());
        for s in ranches {
            let (mut lefts, mut rights) = (Vec::new(), Vec::new());
            let mut nearest_gate = f32::MAX;
            for p in s.of(StructureKind::Poplar) {
                let at = p.footprint.center();
                let (along, across) = along_the_trunk(s, at);
                if across.abs() > Settlement::POPLAR_OFFSET + 1.0 {
                    continue; // the monte, not the row
                }
                nearest_gate = nearest_gate.min(at.distance(s.tracks[0].points[0]));
                if across > 0.0 {
                    lefts.push(along);
                } else {
                    rights.push(along);
                }
            }
            // The last pair stands inside the tranquera, not at its post.
            assert!(
                nearest_gate <= 8.0,
                "the alameda stops {nearest_gate} m from the gate at {:?}",
                s.hub
            );
            for side in [&mut lefts, &mut rights] {
                side.sort_by(f32::total_cmp);
                assert!(side.len() >= 4, "{} poplars at {:?}", side.len(), s.hub);
                // Dense: a windbreak, with at most one skipped pair (the bend).
                let gaps = side.windows(2).map(|w| w[1] - w[0]);
                let skipped = gaps
                    .clone()
                    .filter(|g| *g > Settlement::POPLAR_SPACING + 1.0)
                    .count();
                assert!(
                    skipped <= 1
                        && gaps.fold(0.0, f32::max) <= 2.0 * Settlement::POPLAR_SPACING + 1.0,
                    "gaps in the alameda at {:?}",
                    s.hub
                );
            }
            // An alameda: every tree faces a partner across the lane.
            for (mine, other) in [(&lefts, &rights), (&rights, &lefts)] {
                for a in mine {
                    assert!(
                        other.iter().any(|b| (a - b).abs() <= 1.0),
                        "a poplar {a} m along the lane has no partner across it at {:?}",
                        s.hub
                    );
                }
            }
            assert_eq!(lefts.len(), rights.len());
        }
    }

    #[test]
    fn barns_stand_behind_the_casco_and_far_from_the_main_road() {
        let (world, _, _, _, layout) = settled();
        let roads = world.terrain.roads.as_deref().unwrap();
        let mut barns = 0;
        for s in &layout.settlements {
            for barn in s.of(StructureKind::Barn) {
                barns += 1;
                let (entry, lane, casco) = approach(s);
                let far = |fp: Aabb| {
                    [
                        fp.min,
                        fp.max,
                        Vec2::new(fp.min.x, fp.max.y),
                        Vec2::new(fp.max.x, fp.min.y),
                    ]
                    .map(|c| (c - casco.footprint.center()).dot(lane))
                };
                let nearest = far(barn.footprint).into_iter().fold(f32::MAX, f32::min);
                let casco_end = far(casco.footprint).into_iter().fold(f32::MIN, f32::max);
                assert!(
                    nearest >= casco_end,
                    "{:?} is not behind {casco:?}",
                    barn.footprint
                );
                assert!(
                    barn.footprint.center().distance(entry)
                        > casco.footprint.center().distance(entry)
                );
                let fp = barn.footprint;
                for p in [
                    fp.center(),
                    fp.min,
                    fp.max,
                    Vec2::new(fp.min.x, fp.max.y),
                    Vec2::new(fp.max.x, fp.min.y),
                ] {
                    assert!(
                        roads.near(p, Settlement::BARN_ROAD_CLEARANCE).is_none(),
                        "a barn within {} m of a road at {p:?}",
                        Settlement::BARN_ROAD_CLEARANCE
                    );
                }
            }
        }
        assert!(barns >= 2, "{barns}");
    }
}
