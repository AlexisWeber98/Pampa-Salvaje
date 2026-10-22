//! Rural settlements behind the field gates (user, 2026-10-22): the
//! ranchos, puestos, chacras and estancias of 1919 La Pampa.
//!
//! Some open roadside gates ([`crate::Gate`]) lead into a settled field.
//! Behind such a gate, inside its survey cell and clear of the roads, the
//! railway, the towns and the fences, stands a seeded compound: the houses
//! round a bare patio (the hub), with a galpón, corrals, a molino with its
//! tank and trough, haystacks and the spots where its trees will grow.
//!
//! Dirt huellas join it to the gate: a trunk from just inside the gate to
//! the hub, and a branch from the hub to the door of every house, galpón
//! and corral. Huellas are two wheel ruts, not roads: they live outside
//! [`crate::RoadNetwork`], with no fences, signposts or terrain cut, and
//! cross no fence but at their own gate.
//!
//! Everything is axis-aligned, like the survey grid. Houses, galpones,
//! tanks and props are box obstacles; a corral is its rail spans with one
//! opening, so a rider can ride in; trees are left to flora and render.
//! Wild flora keeps off the compounds and the huellas through an indexed
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
    /// A small adobe house, maybe a corral, and a well.
    Rancho,
    /// A herdsman's hut out on an estancia's land, with a corral and an
    /// aguada (a trough, sometimes a molino).
    Puesto,
    /// A farm: a brick or adobe house, a galpón, a molino with its tank and
    /// trough, a corral and haystacks.
    Chacra,
    /// An estancia's casco with its gallery, the houses of its peones,
    /// galpones, corrals, the aguada and a grove, spread over hectares.
    Estancia,
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
    CascoGallery,
}

/// One building or prop of a settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructureKind {
    MainHouse(HouseStyle),
    /// A casa de peones, or the caretaker's house.
    WorkerHouse(HouseStyle),
    /// A sheet-iron shed for tools, carts and grain.
    Galpon,
    /// Post-and-rail corral, open on its front side.
    Corral,
    /// A windpump: its footprint is the tower's base.
    Molino,
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
    /// A two-wheeled ox cart left in the yard.
    Carreta,
    /// Where a eucalyptus of the grove grows (not an obstacle here).
    Eucalyptus,
    /// Where a poplar of the entrance alameda grows (not an obstacle here).
    Poplar,
}

impl StructureKind {
    /// A house people live in.
    pub fn is_house(self) -> bool {
        matches!(self, Self::MainHouse(_) | Self::WorkerHouse(_))
    }

    /// Whether a huella leads to its door.
    pub fn is_reached(self) -> bool {
        self.is_house() || matches!(self, Self::Galpon | Self::Corral)
    }

    /// A tree spot rather than a structure.
    pub fn is_tree(self) -> bool {
        matches!(self, Self::Eucalyptus | Self::Poplar)
    }

    /// The model slot a downloaded glTF replaces the procedural mesh from
    /// (`assets/models/third-party/<slot>/`), for the props that have one.
    pub fn slot(self) -> Option<&'static str> {
        Some(match self {
            Self::Molino => "windpump",
            Self::Eucalyptus => "eucalyptus",
            Self::Poplar => "poplar",
            Self::Well => "well",
            Self::Trough => "trough",
            Self::Carreta => "carreta",
            Self::Haystack => "haystack",
            Self::HitchingPost => "hitching_post",
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
    /// its front: where its huella ends.
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

    /// Its static obstacles: the footprint; a corral's rail spans with
    /// [`Settlement::CORRAL_GATE`] meters open in the middle of its front;
    /// none for a tree spot.
    pub fn obstacles(&self) -> Vec<Aabb> {
        match self.kind {
            k if k.is_tree() => Vec::new(),
            StructureKind::Corral => corral_rails(self.footprint, self.front),
            _ => vec![self.footprint],
        }
    }
}

/// An inner track: two wheel ruts along `points`,
/// [`Huella::HALF_WIDTH`] meters to either side.
#[derive(Debug, Clone, PartialEq)]
pub struct Huella {
    pub points: Vec<Vec2>,
}

impl Huella {
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
    /// The middle of the patio, where the huellas meet.
    pub hub: Vec2,
    /// The bare, trodden yard round the hub.
    pub patio: Aabb,
    pub structures: Vec<Structure>,
    /// The trunk from the gate to the hub first, then one branch from the
    /// hub to each door.
    pub huellas: Vec<Huella>,
    /// Everything it covers: footprints, patio and huellas.
    pub bounds: Aabb,
    /// Seed for render's own variety.
    pub seed: u32,
}

impl Settlement {
    /// How far out of a front its huella ends, in meters.
    pub const DOOR_REACH: f32 = 3.5;
    /// Width of a corral's opening, in meters.
    pub const CORRAL_GATE: f32 = 4.0;
    /// Thickness of a corral rail's obstacle, in meters.
    pub const CORRAL_RAIL: f32 = 0.2;
    /// Least distance from a footprint to a road's centerline, in meters:
    /// well behind the roadside fence.
    pub const ROAD_CLEARANCE: f32 = 25.0;
    /// Least distance from a footprint to the track's centerline, meters.
    pub const RAIL_CLEARANCE: f32 = 40.0;
    /// Least distance from a footprint or huella to a town's leveled band.
    pub const TOWN_CLEARANCE: f32 = 60.0;
    /// Least distance from a footprint to its cell's survey lines (the
    /// grid fences), in meters.
    pub const FENCE_CLEARANCE: f32 = 12.0;
    /// Least distance from a huella's centerline to the survey lines.
    pub const HUELLA_FENCE_CLEARANCE: f32 = 4.0;
    /// Least distance from a huella's centerline to a road's, past its
    /// gate: its ruts stay off the roadside fence.
    pub const HUELLA_ROAD_CLEARANCE: f32 = FenceLayout::ROADSIDE + Huella::HALF_WIDTH + 1.0;
    /// How far from its gate a huella's trunk may still be near the road.
    pub const GATE_APPROACH: f32 = 6.0;
    /// Least clear ground between two footprints, in meters: a rider fits.
    pub const GAP: f32 = 3.0;
    /// Least distance from a huella's centerline to a footprint, meters.
    pub const CORRIDOR: f32 = Huella::HALF_WIDTH + 1.0;
    /// Least distance between two settlements' hubs, in meters.
    pub const MIN_SPACING: f32 = 300.0;
    /// Least ground an estancia's compound spreads over, in square meters.
    pub const ESTANCIA_AREA: f32 = 20_000.0;

    /// Every structure of `kind`.
    pub fn of(&self, kind: StructureKind) -> impl Iterator<Item = &Structure> + '_ {
        self.structures.iter().filter(move |s| s.kind == kind)
    }

    /// The trunk huella, from just inside the gate to the hub.
    pub fn trunk(&self) -> &Huella {
        &self.huellas[0]
    }
}

/// Every settlement of the world, with the keep-out wild flora respects.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettlementLayout {
    pub settlements: Vec<Settlement>,
    pub(crate) keep_out: Arc<KeepOut>,
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
    /// Distance from the gate to the end of the trunk's first straight
    /// piece, in meters.
    const ENTRY: f32 = 12.0;

    /// The settlements behind the open gates of `fences`, over `terrain`
    /// (with its roads laid), clear of the towns of `towns` and of `line`'s
    /// whole track. Seeded per gate: the same every time.
    pub fn build(
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
        fences: &FenceLayout,
    ) -> Self {
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                line.full_path(towns)
                    .points()
                    .windows(2)
                    .map(|w| (w[0], w[1])),
            )
            .collect();
        let bands: Vec<Aabb> = towns
            .iter()
            .map(|t| t.leveled().expanded(Settlement::TOWN_CLEARANCE))
            .collect();
        let roads = terrain.roads.as_deref();
        let mut settlements: Vec<Settlement> = Vec::new();
        let mut used: HashSet<(u32, u32)> = HashSet::new();
        for (index, gate) in fences.gates.iter().enumerate() {
            if !gate.open {
                continue;
            }
            let mut rng = Rng::new(terrain.seed ^ SETTLEMENT_SEED, index as u32);
            let into = field_side(roads, gate.at, gate.along);
            let probe = gate.at + into * 3.0;
            if fences.is_descampado(probe) {
                continue;
            }
            let Some(cell) = fences.field_of(probe) else {
                continue;
            };
            let key = (cell.min.x.to_bits(), cell.min.y.to_bits());
            let town_gap = towns
                .iter()
                .map(|t| t.leveled().closest_point(gate.at).distance(gate.at))
                .fold(f32::INFINITY, f32::min);
            let near_town = town_gap < Self::NEAR_TOWN;
            let chance = if near_town {
                Self::SETTLED.0
            } else {
                Self::SETTLED.1
            };
            if used.contains(&key) || rng.unit() >= chance {
                continue;
            }
            let kind = pick_kind(&mut rng, near_town);
            let reach = cell.expanded(Settlement::RAIL_CLEARANCE + 1.0);
            let ground = Ground {
                roads,
                rails: rails
                    .iter()
                    .copied()
                    .filter(|&(a, b)| reach.segment_overlap(a, b).is_some())
                    .collect(),
                bands: bands
                    .iter()
                    .copied()
                    .filter(|b| b.intersects(&reach))
                    .collect(),
                cell,
            };
            let start = gate.at + into * 0.5;
            let entry = gate.at + into * Self::ENTRY;
            if !ground.track_ok(start, entry, Some(gate.at)) {
                continue;
            }
            let (depth, room) = kind.hub_reach();
            let half = cell.size().x.min(cell.size().y) / 2.0 - 1.0;
            let inner = cell.expanded(-room.min(half));
            for _ in 0..Self::HUB_TRIES {
                // Somewhere ahead of the gate, kept inside the cell.
                let ahead = gate.at
                    + into * rng.range(depth.0, depth.1)
                    + gate.along * rng.range(-depth.1, depth.1) * 0.5;
                let hub = inner.closest_point(ahead);
                let spaced = settlements
                    .iter()
                    .all(|s| s.hub.distance(hub) >= Settlement::MIN_SPACING);
                if (hub - gate.at).dot(into) < depth.0
                    || hub.distance(gate.at) > depth.1
                    || !spaced
                    || !ground.track_ok(entry, hub, None)
                {
                    continue;
                }
                let mut compound =
                    Compound::new(&ground, rng, kind, hub, into, [start, entry, hub]);
                let built = compound.raise();
                rng = compound.rng;
                if built {
                    used.insert(key);
                    let seed = rng.next();
                    settlements.push(compound.finish(index as u32, seed));
                    break;
                }
            }
        }
        let mut keep_out = KeepOut::default();
        for s in &settlements {
            keep_out.push(Shape::Area(s.patio));
            for st in &s.structures {
                keep_out.push(Shape::Area(st.footprint));
            }
            for h in &s.huellas {
                for (a, b) in h.segments() {
                    keep_out.push(Shape::Strip(a, b, Huella::HALF_WIDTH));
                }
            }
        }
        Self {
            settlements,
            keep_out: Arc::new(keep_out),
        }
    }

    /// How many settlements are of `kind`.
    pub fn count(&self, kind: SettlementKind) -> usize {
        self.settlements.iter().filter(|s| s.kind == kind).count()
    }

    /// The static obstacles of every settlement.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .flat_map(Structure::obstacles)
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

    /// The huellas of the settlements [`Self::in_area`] holds.
    pub fn tracks_in(&self, area: Aabb) -> impl Iterator<Item = &Huella> + '_ {
        self.in_area(area).flat_map(|s| s.huellas.iter())
    }

    /// Whether `p` is more than `margin` meters (at most
    /// [`KeepOut::MAX_MARGIN`]) from every footprint, patio and huella.
    pub fn is_clear(&self, p: Vec2, margin: f32) -> bool {
        self.keep_out.is_clear(p, margin)
    }

    /// The indexed keep-out of every settlement.
    pub fn keep_out(&self) -> &KeepOut {
        &self.keep_out
    }
}

impl SettlementKind {
    /// How far into the field (from the gate's road) the hub lies at
    /// least, how far from the gate at most, and the room it keeps from its
    /// cell's edges, in meters.
    fn hub_reach(self) -> ((f32, f32), f32) {
        match self {
            Self::Rancho => ((35.0, 250.0), 35.0),
            Self::Puesto => ((50.0, 450.0), 45.0),
            Self::Chacra => ((60.0, 350.0), 55.0),
            Self::Estancia => ((140.0, 600.0), 120.0),
        }
    }

    /// Side of the patio, in meters.
    fn patio(self) -> f32 {
        match self {
            Self::Rancho | Self::Puesto => 10.0,
            Self::Chacra => 16.0,
            Self::Estancia => 24.0,
        }
    }
}

/// The kind of a new settlement: chacras near the towns, puestos and
/// ranchos scattered, estancias rare.
fn pick_kind(rng: &mut Rng, near_town: bool) -> SettlementKind {
    use SettlementKind::*;
    let roll = rng.unit();
    let odds: [(SettlementKind, f32); 4] = if near_town {
        [
            (Chacra, 0.55),
            (Rancho, 0.3),
            (Puesto, 0.1),
            (Estancia, 0.05),
        ]
    } else {
        [
            (Rancho, 0.35),
            (Puesto, 0.35),
            (Chacra, 0.2),
            (Estancia, 0.1),
        ]
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
        self.cell
            .expanded(-Settlement::FENCE_CLEARANCE)
            .contains(&area)
            && self.bands.iter().all(|b| !b.intersects(&area))
            && self.rails.iter().all(|&(a, b)| {
                area.expanded(Settlement::RAIL_CLEARANCE)
                    .segment_overlap(a, b)
                    .is_none()
            })
            && self.off_roads(area)
    }

    /// Whether `area` is [`Settlement::ROAD_CLEARANCE`] from every road:
    /// samples along its edges at most 5 m apart, each held 2.5 m farther
    /// off, and its middle.
    fn off_roads(&self, area: Aabb) -> bool {
        let Some(roads) = self.roads else {
            return true;
        };
        let reach = Settlement::ROAD_CLEARANCE + 2.5;
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

    /// Whether a huella may run straight from `a` to `b`: inside the cell
    /// off its fences, off the roads but within
    /// [`Settlement::GATE_APPROACH`] of its `gate`, off the rail and the
    /// towns. Sampled every meter.
    fn track_ok(&self, a: Vec2, b: Vec2, gate: Option<Vec2>) -> bool {
        let inner = self.cell.expanded(-Settlement::HUELLA_FENCE_CLEARANCE);
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
                        .is_none_or(|r| r.near(p, Settlement::HUELLA_ROAD_CLEARANCE).is_none())
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
    huellas: Vec<Huella>,
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
            huellas: vec![Huella {
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
            SettlementKind::Rancho => self.rancho(),
            SettlementKind::Puesto => self.puesto(),
            SettlementKind::Chacra => self.chacra(),
            SettlementKind::Estancia => self.estancia(),
        }
    }

    fn rancho(&mut self) -> bool {
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

    fn puesto(&mut self) -> bool {
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
        let molino = self.chance(0.5);
        let tank = molino && self.chance(0.5);
        self.aguada(molino, tank, (14.0, 26.0));
        self.extras(0.7, 0.0, (7.0, 10.0), (10.0, 20.0));
        true
    }

    fn chacra(&mut self) -> bool {
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
        if self.place(house, self.ahead(12.0, 20.0, 0.5)).is_none() {
            return false;
        }
        let galpon = self.part(StructureKind::Galpon, (14.0, 20.0), (8.0, 10.0), 4.5);
        if self.place(galpon, self.round(24.0, 40.0)).is_none() {
            return false;
        }
        if !self.aguada(true, true, (20.0, 36.0)) {
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
        true
    }

    fn estancia(&mut self) -> bool {
        let casco = self.part(
            StructureKind::MainHouse(HouseStyle::CascoGallery),
            (22.0, 30.0),
            (14.0, 18.0),
            4.5,
        );
        let Some(casco) = self.place(casco, self.ahead(18.0, 26.0, 0.3)) else {
            return false;
        };
        let casco = self.structures[casco].footprint;
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
        let mut galpones = 0;
        for _ in 0..1 + self.rng.below(2) {
            let galpon = self.part(StructureKind::Galpon, (20.0, 30.0), (10.0, 14.0), 5.5);
            galpones += self.place(galpon, self.round(40.0, 90.0)).is_some() as usize;
        }
        let mut corrals = 0;
        for _ in 0..1 + self.rng.below(3) {
            corrals += self.corral((22.0, 34.0), self.round(60.0, 120.0)) as usize;
        }
        if galpones == 0 || corrals == 0 || !self.aguada(true, true, (35.0, 70.0)) {
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
        let carreta = self.part(StructureKind::Carreta, (5.0, 5.0), (2.2, 2.2), 2.5);
        self.place(carreta, self.round(25.0, 50.0));
        self.alameda();
        self.grove(casco);
        let bounds = self.bounds();
        bounds.size().x * bounds.size().y >= Settlement::ESTANCIA_AREA
    }

    /// Poplars both sides of the trunk's last 90 m, 8 m apart.
    fn alameda(&mut self) {
        let entry = self.huellas[0].points[1];
        let length = entry.distance(self.hub);
        let dir = (entry - self.hub) * (1.0 / length.max(1e-6));
        let across = Vec2::new(-dir.y, dir.x);
        let mut along = 20.0;
        while along < length.min(90.0) {
            for side in [-5.0, 5.0] {
                let at = self.hub + dir * along + across * side;
                let tree = self.tree(StructureKind::Poplar);
                self.place_at(tree, at);
            }
            along += 8.0;
        }
    }

    /// A monte of eucalyptus beside the casco: a jittered grid 6 m apart.
    fn grove(&mut self, casco: Aabb) {
        let across = Vec2::new(-self.into.y, self.into.x);
        let side = if self.chance(0.5) { 1.0 } else { -1.0 };
        let (rows, cols) = (4 + self.rng.below(3), 5 + self.rng.below(4));
        let corner = casco.center()
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
    fn aguada(&mut self, molino: bool, tank: bool, radius: (f32, f32)) -> bool {
        let source = if molino {
            self.part(StructureKind::Molino, (2.4, 2.4), (2.4, 2.4), 10.0)
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
    fn extras(&mut self, post: f32, carreta: f32, post_at: (f32, f32), carreta_at: (f32, f32)) {
        if self.chance(post) {
            let part = self.part(StructureKind::HitchingPost, (3.0, 3.0), (0.3, 0.3), 1.1);
            self.place(part, self.round(post_at.0, post_at.1));
        }
        if self.chance(carreta) {
            let part = self.part(StructureKind::Carreta, (5.0, 5.0), (2.2, 2.2), 2.5);
            self.place(part, self.round(carreta_at.0, carreta_at.1));
        }
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

    /// Places `part` somewhere in `spot`, facing the hub, with its branch
    /// huella when it has a door. Returns its index.
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
        if !self.fits(structure.footprint) {
            return None;
        }
        if part.kind.is_reached() {
            let door = structure.door();
            if !self.branch_ok(self.hub, door, structure.footprint) {
                return None;
            }
            self.huellas.push(Huella {
                points: vec![self.hub, door],
            });
        }
        self.structures.push(structure);
        Some(self.structures.len() - 1)
    }

    /// Whether `footprint` stands clear of the ground's keep-outs, the
    /// patio, every other footprint and every huella.
    fn fits(&self, footprint: Aabb) -> bool {
        let corridor = footprint.expanded(Settlement::CORRIDOR);
        self.ground.area_ok(footprint)
            && !self.patio.intersects(&footprint)
            && self
                .structures
                .iter()
                .all(|s| !s.footprint.expanded(Settlement::GAP).intersects(&footprint))
            && self
                .huellas
                .iter()
                .flat_map(Huella::segments)
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
        for p in self.huellas.iter().flat_map(|h| h.points.iter()) {
            grow(Aabb::from_center_size(
                *p,
                Vec2::new(2.0, 2.0) * Huella::HALF_WIDTH,
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
            huellas: self.huellas,
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
/// and huellas, indexed on a uniform grid so a point's check only looks at
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fence::tests::{fenced, reached};
    use crate::{ChunkCoord, FloraSite, StaticObstacles, World};

    /// The fence fixture's world with its settlements laid and settled.
    fn settled() -> (
        World,
        Vec<TownLayout>,
        RailLine,
        FenceLayout,
        SettlementLayout,
    ) {
        let (mut world, towns, line, fences) = fenced();
        let started = std::time::Instant::now();
        let layout = SettlementLayout::build(&world.terrain, &towns, &line, &fences);
        let built = started.elapsed();
        let count = world.settle(&layout);
        let open = fences.gates.iter().filter(|g| g.open).count();
        println!(
            "settlements: {} behind {open} open gates ({} ranchos, {} puestos, {} chacras, \
             {} estancias), {count} obstacles; laid in {built:?}",
            layout.settlements.len(),
            layout.count(SettlementKind::Rancho),
            layout.count(SettlementKind::Puesto),
            layout.count(SettlementKind::Chacra),
            layout.count(SettlementKind::Estancia),
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

    /// Points along every huella of `s`, `step` meters apart.
    fn huella_points(s: &Settlement, step: f32) -> Vec<Vec2> {
        s.huellas
            .iter()
            .flat_map(Huella::segments)
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
            SettlementLayout::build(&world.terrain, &towns, &line, &fences)
        );
        let open = fences.gates.iter().filter(|g| g.open).count();
        let total = layout.settlements.len();
        // Only some open gates lead to a settlement, and estancias are few.
        assert!(total >= 10, "{total}");
        assert!(total * 2 < open, "{total} of {open}");
        let count = |k| layout.count(k);
        use SettlementKind::*;
        assert!(count(Estancia) >= 1, "{}", count(Estancia));
        assert!(count(Estancia) * 3 <= total, "{}", count(Estancia));
        for k in [Rancho, Puesto, Chacra] {
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
            SettlementLayout::build(&other, &towns, &line, &fences)
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
                assert!(
                    cell.expanded(-Settlement::FENCE_CLEARANCE).contains(&fp),
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
                        roads.near(p, Settlement::ROAD_CLEARANCE - 0.01).is_none(),
                        "{st:?} at {p:?}"
                    );
                    for &(a, b) in &near_rails {
                        assert!(segment_distance(p, a, b) >= Settlement::RAIL_CLEARANCE - 0.01);
                    }
                }
            }
            // The huellas stay in the field, off the road and the rail, and
            // cross no wire: the trunk passes the gate's open leaves.
            let gate = fences.gates[s.gate as usize].at;
            for p in huella_points(s, 0.5) {
                assert!(cell.contains_point(p), "{p:?}");
                assert!(
                    wire.near(Aabb::from_center_size(p, Vec2::new(0.6, 0.6)))
                        .is_empty(),
                    "a huella crosses a fence at {p:?}"
                );
                if p.distance(gate) > Settlement::GATE_APPROACH {
                    assert!(
                        cell.expanded(-Settlement::HUELLA_FENCE_CLEARANCE)
                            .contains_point(p)
                    );
                    assert!(
                        roads
                            .near(p, Settlement::HUELLA_ROAD_CLEARANCE - 0.01)
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
    fn buildings_overlap_neither_each_other_nor_the_huellas() {
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
                for (p, q) in s.huellas.iter().flat_map(Huella::segments) {
                    assert!(
                        a.footprint
                            .expanded(Huella::HALF_WIDTH)
                            .segment_overlap(p, q)
                            .is_none(),
                        "{a:?} on the huella {p:?}-{q:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn huellas_lead_from_the_gate_to_every_door() {
        let (_, _, _, _, layout) = settled();
        for s in &layout.settlements {
            // Every branch leaves from the hub, where the trunk ends.
            for h in &s.huellas[1..] {
                assert_eq!(h.points[0], s.hub);
            }
            for st in s.structures.iter().filter(|st| st.kind.is_reached()) {
                let door = st.door();
                assert!(
                    s.huellas[1..]
                        .iter()
                        .any(|h| h.points.last().unwrap().distance(door) < 1e-3),
                    "no huella to {st:?}"
                );
            }
            assert_eq!(
                s.huellas.len(),
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
                SettlementKind::Rancho => {
                    assert!(matches!(
                        style,
                        HouseStyle::AdobeThatch | HouseStyle::AdobeIron
                    ));
                    assert_eq!(n(Galpon) + n(Molino), 0, "{s:?}");
                }
                SettlementKind::Puesto => {
                    assert!(n(Corral) == 1 && n(Trough) <= 1, "{s:?}");
                    assert!(n(Molino) + n(Well) == 1, "{s:?}");
                }
                SettlementKind::Chacra => {
                    assert!(style != HouseStyle::CascoGallery);
                    assert!(n(Galpon) == 1 && n(Molino) == 1, "{s:?}");
                }
                SettlementKind::Estancia => {
                    assert_eq!(style, HouseStyle::CascoGallery);
                    let workers = s
                        .structures
                        .iter()
                        .filter(|st| matches!(st.kind, WorkerHouse(_)))
                        .count();
                    assert!((2..=4).contains(&workers), "{workers}");
                    assert!((1..=2).contains(&n(Galpon)));
                    assert!(n(Corral) >= 1 && n(Molino) == 1);
                    assert!(n(Eucalyptus) + n(Poplar) >= 10, "{s:?}");
                    let size = s.bounds.size();
                    assert!(
                        size.x * size.y >= Settlement::ESTANCIA_AREA,
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
                        || s.huellas
                            .iter()
                            .flat_map(Huella::segments)
                            .any(|(a, b)| segment_distance(p, a, b) <= Huella::HALF_WIDTH + margin)
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
            // On the huellas and in the buildings nothing grows; far off,
            // anything may.
            for p in huella_points(s, 2.0) {
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
            .flat_map(|s| huella_points(s, 4.0))
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
                .map(|s| s.huellas.len())
                .sum::<usize>()
        );
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
        assert_eq!(StructureKind::Molino.slot(), Some("windpump"));
        assert!(StructureKind::Eucalyptus.is_tree());
        assert!(
            Structure {
                kind: StructureKind::Poplar,
                ..corral
            }
            .obstacles()
            .is_empty()
        );
    }
}
