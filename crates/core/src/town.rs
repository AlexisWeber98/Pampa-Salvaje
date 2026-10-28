//! Settlement layouts: street grid, plaza, railway and building footprints.
//!
//! Pure data. A [`TownConfig`] describes a town (grid size, block and street
//! widths, railway side, plaza placement); [`TownLayout::generate`] turns it
//! into footprints. Render decides walls, roofs and colors per
//! [`BuildingKind`]; logic only needs footprints for collision and the
//! leveled ground.
//!
//! Sidewalks are [`Sidewalk::WIDTH`] (3.5 m) strips along the block fronts
//! of a city and round the plaza of a town, inside the streets, so a street
//! of `street_width` leaves a carriageway of `street_width - 7` where both
//! its sides are paved (at least [`TownConfig::MIN_STREET_WIDTH`] - 7 m).
//! Trees are core data and obstacles: the plazas' rings
//! ([`TownLayout::trees`]) and the [`ShadeTree`]s (sidewalk trees in their
//! cazuelas, groves between the plaza paths, front trees on the dirt, yard
//! and landmark trees), with the [`YardPlant`]s (shrubs and flower beds) of
//! the yards as scenery.

use std::fmt;

use crate::lamps::StreetLamps;
use crate::places::PLATFORM_SPOT_FROM_EDGE;
use crate::rng::Rng;
use crate::{Aabb, FlatArea, Heading, Terrain, Vec2};

/// What a building is, so render can pick its look and logic its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    House,
    /// "Almacén de ramos generales": the town's general store.
    GeneralStore,
    PoliceStation,
    Church,
    Station,
    /// Raised platform between the station and the track.
    Platform,
    /// Corrugated-metal shed.
    Shed,
    /// Long grain storage shed by the railway.
    GrainStore,
    /// Round, low "australian" water tank.
    WaterTank,
    /// Windmill tower pumping into a water tank.
    Windmill,
}

/// Which side of a footprint faces the street (where a facade goes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    North,
    South,
    East,
    West,
}

impl Side {
    /// The side facing the other way.
    pub fn opposite(self) -> Self {
        match self {
            Side::North => Side::South,
            Side::South => Side::North,
            Side::East => Side::West,
            Side::West => Side::East,
        }
    }
}

/// One static building: a box footprint, a wall height and its street side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Building {
    pub kind: BuildingKind,
    pub footprint: Aabb,
    /// Wall height in meters (roofs and parapets are a render choice).
    pub height: f32,
    pub front: Side,
}

impl Building {
    /// How far a galpón's (shed's or grain store's) roof reaches past its
    /// walls, in meters: its eaves and verges end on the footprint, and its
    /// walls stand this far in from it (render's `facade::walls::barn`).
    pub const BARN_EAVES: f32 = 0.45;

    /// What people and horses bump into: the footprint, or a galpón's
    /// walls, [`Self::BARN_EAVES`] in from it (only its roof reaches out).
    pub fn collider(&self) -> Aabb {
        match self.kind {
            BuildingKind::Shed | BuildingKind::GrainStore => {
                self.footprint.expanded(-Self::BARN_EAVES)
            }
            _ => self.footprint,
        }
    }
}

/// Which of a town's streets get a brick sidewalk (vereda), by its blocks
/// (user, 2026-10-25). The rest stay dirt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paving {
    /// A village of at most [`Paving::VILLAGE_MAX`] blocks (Metileo):
    /// dirt everywhere.
    None,
    /// A small town (Castex, Monte Nievas, Caleufú): only the plaza's own
    /// edge, a ring round it (user, 2026-10-25).
    Plaza,
    /// A big town (General Pico, [`TownSize::Big`] or bigger): along every
    /// block front and round every plaza.
    Everywhere,
}

impl Paving {
    /// Most blocks of a village with no paving at all (3 x 3).
    pub const VILLAGE_MAX: usize = 9;

    /// The paving of a town of `blocks` (columns, rows).
    pub fn of(blocks: (usize, usize)) -> Self {
        match TownSize::of(blocks) {
            _ if blocks.0 * blocks.1 <= Self::VILLAGE_MAX => Self::None,
            TownSize::Small => Self::Plaza,
            TownSize::Big | TownSize::VeryBig => Self::Everywhere,
        }
    }
}

/// What a sidewalk is made of: brick in a city, and round a town's plaza;
/// a beaten-earth walk that blends into the grass elsewhere, with its curbs
/// all the same (user, 2026-10-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Walk {
    Brick,
    Earth,
}

/// One straight strip of sidewalk (brick or beaten earth, [`Walk`]), lying
/// on the street along a block or plaza edge. It is drawn on the ground like the streets and the plaza
/// paths, a few centimeters proud: people walk over it at the ground's
/// height, so it changes nothing in how anyone moves.
#[derive(Debug, Clone, PartialEq)]
pub struct Sidewalk {
    pub area: Aabb,
    /// Its edges on the roadway, each with a curb.
    pub curbs: Vec<Side>,
    pub walk: Walk,
}

impl Sidewalk {
    /// Width of a sidewalk, curb included, in meters.
    pub const WIDTH: f32 = 3.5;

    /// The strips of a sidewalk all round `area`: the north and south ones
    /// reach round the corners, curbed at their ends; the east and west
    /// ones run between them.
    fn ring(area: Aabb, walk: Walk) -> [Sidewalk; 4] {
        let w = Self::WIDTH;
        let (min, max) = (area.min, area.max);
        let ends = |side| vec![side, Side::East, Side::West];
        [
            Sidewalk {
                area: Aabb::new(Vec2::new(min.x - w, min.y - w), Vec2::new(max.x + w, min.y)),
                curbs: ends(Side::South),
                walk,
            },
            Sidewalk {
                area: Aabb::new(Vec2::new(min.x - w, max.y), Vec2::new(max.x + w, max.y + w)),
                curbs: ends(Side::North),
                walk,
            },
            Sidewalk {
                area: Aabb::new(Vec2::new(min.x - w, min.y), Vec2::new(min.x, max.y)),
                curbs: vec![Side::West],
                walk,
            },
            Sidewalk {
                area: Aabb::new(Vec2::new(max.x, min.y), Vec2::new(max.x + w, max.y)),
                curbs: vec![Side::East],
                walk,
            },
        ]
    }
}

/// The sidewalks of a town paved as `paving`, on `grid` round its main
/// `plaza` (the blocks it takes): a city's are all brick; a town's plaza
/// ring is brick and its other blocks have a walk of beaten earth; a
/// village's are all beaten earth. Every one has its curbs.
fn sidewalks(grid: &Grid, plaza: PlazaBlocks, paving: Paving) -> Vec<Sidewalk> {
    let area = grid.span(plaza);
    let blocks = (0..grid.rows).flat_map(|j| (0..grid.cols).map(move |i| (i, j)));
    let walk = match paving {
        Paving::Everywhere => Walk::Brick,
        Paving::Plaza | Paving::None => Walk::Earth,
    };
    // A village is too small for brick, even round its plaza.
    let plaza_walk = match paving {
        Paving::None => Walk::Earth,
        Paving::Plaza | Paving::Everywhere => Walk::Brick,
    };
    blocks
        .filter(|&b| !plaza.contains(b))
        .flat_map(|(i, j)| Sidewalk::ring(grid.block(i, j), walk))
        .chain(Sidewalk::ring(area, plaza_walk))
        .collect()
}

/// Whether `a` and `b` share an area, not just an edge or a corner.
#[cfg(test)]
fn overlap(a: &Aabb, b: &Aabb) -> bool {
    a.min.x.max(b.min.x) < a.max.x.min(b.max.x) && a.min.y.max(b.min.y) < a.max.y.min(b.max.y)
}

/// A raised floor people walk on: the station platform. Its top is part of
/// the ground height ([`crate::World::ground_height`]), not an obstacle.
///
/// A short ramp at each end leads down to the ground along the floor. Its
/// long sides are plain steps: people walk up onto it from the town side or
/// off it toward the track (where a standing train blocks them, its cars
/// being moving obstacles).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floor {
    pub footprint: Aabb,
    /// Height of the top above the ground, in meters.
    pub height: f32,
    /// Unit world axis along the floor (`+x` or `+y`), the way its ramps run.
    pub along: Vec2,
}

impl Floor {
    /// Height of a station platform's top above the ground, in meters.
    pub const PLATFORM_HEIGHT: f32 = 0.9;
    /// Distance from a track's centerline to the track edge of its
    /// platform, in meters: the cars' side is 1.5 m out, so a hand's
    /// breadth of 0.15 m is left, as at a broad-gauge platform.
    pub const PLATFORM_EDGE: f32 = 1.65;
    /// How far in from a platform's far edge a walk to it runs, in meters.
    pub(crate) const WALK_LINE_IN: f32 = 3.5;
    /// Length of the ramp past each end, in meters (a gentle 1:3 slope for
    /// a 0.9 m platform).
    pub const RAMP_LENGTH: f32 = 3.0;

    /// Unit world axis across the floor.
    pub fn across(&self) -> Vec2 {
        Vec2::new(self.along.y, self.along.x)
    }

    /// The floor's extent along `axis`, as `(low, high)`.
    fn span(&self, axis: Vec2) -> (f32, f32) {
        (self.footprint.min.dot(axis), self.footprint.max.dot(axis))
    }

    /// Height above the ground at `p`: the floor's top on it, falling
    /// linearly to zero down either ramp. `None` off the floor and ramps.
    pub fn rise_at(&self, p: Vec2) -> Option<f32> {
        let (lo, hi) = self.span(self.across());
        let c = p.dot(self.across());
        if c < lo || c > hi {
            return None;
        }
        let (lo, hi) = self.span(self.along);
        let a = p.dot(self.along);
        let past = (lo - a).max(a - hi).max(0.0);
        (past <= Self::RAMP_LENGTH).then(|| self.height * (1.0 - past / Self::RAMP_LENGTH))
    }

    /// Where a passenger steps off a train standing at the floor's side
    /// nearest `train`: half way along, a step in from that long edge (as
    /// [`TownLayout::platform_spot`]), and the heading away from the edge.
    pub fn spot_beside(&self, train: Vec2) -> (Vec2, Heading) {
        let across = self.across();
        let (lo, hi) = self.span(across);
        let c = train.dot(across);
        let (edge, inward) = if (c - lo).abs() < (hi - c).abs() {
            (lo, 1.0)
        } else {
            (hi, -1.0)
        };
        let at = across * (edge + inward * PLATFORM_SPOT_FROM_EDGE)
            + self.along * self.footprint.center().dot(self.along);
        let out = across * inward;
        (at, Heading::from_radians(out.y.atan2(out.x)))
    }

    /// The two ramps' footprints, low end first along [`Self::along`].
    pub fn ramps(&self) -> [Aabb; 2] {
        let reach = self.along * Self::RAMP_LENGTH;
        let f = self.footprint;
        let low_end = f.max - self.along * f.size().dot(self.along);
        let high_end = f.min + self.along * f.size().dot(self.along);
        [
            Aabb::new(f.min - reach, low_end),
            Aabb::new(high_end, f.max + reach),
        ]
    }
}

/// The platform canopy: a flat roof slab hung from the station's track-side
/// wall over the platform, on thin posts. Render draws it from this, and the
/// camera treats it as a roof it cannot see through (a slab, not a column:
/// the camera may still sit under it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canopy {
    /// The roof's footprint on the ground plane.
    pub footprint: Aabb,
    /// Height of the roof slab's middle above the ground under the station.
    pub height: f32,
}

impl Canopy {
    /// How far the roof reaches out from the station wall, in meters.
    pub const DEPTH: f32 = 5.5;
    /// Height of the roof above the ground under the station, in meters.
    pub const HEIGHT: f32 = 3.6;
    /// Thickness of the roof slab, in meters.
    pub const THICKNESS: f32 = 0.12;

    /// Height of the slab's underside above the ground under the station.
    pub fn bottom(&self) -> f32 {
        self.height - Self::THICKNESS / 2.0
    }

    /// Height of the slab's top above the ground under the station.
    pub fn top(&self) -> f32 {
        self.height + Self::THICKNESS / 2.0
    }
}

/// A straight railway line on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Railway {
    pub start: Vec2,
    pub end: Vec2,
    /// Distance between the rails, in meters.
    pub gauge: f32,
}

/// A saddled horse tied in front of a building: it never moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TetheredHorse {
    pub position: Vec2,
    pub heading: Heading,
    pub footprint: Aabb,
}

impl TetheredHorse {
    /// Length of a standing horse's footprint, nose to tail, in meters.
    pub const LENGTH: f32 = 2.4;
    /// Width of a standing horse's footprint, in meters.
    pub const WIDTH: f32 = 1.0;

    /// A horse standing tied at `position`, facing `heading`: its
    /// footprint is the axis-aligned box around its turned body.
    pub fn standing(position: Vec2, heading: Heading) -> Self {
        let along = heading.forward();
        let half = Vec2::new(
            along.x.abs() * Self::LENGTH + along.y.abs() * Self::WIDTH,
            along.y.abs() * Self::LENGTH + along.x.abs() * Self::WIDTH,
        ) * 0.5;
        Self {
            position,
            heading,
            footprint: Aabb::new(position - half, position + half),
        }
    }
}

/// A timber hitching rail with a horse tied alongside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitchingPost {
    pub rail: Aabb,
    pub horse: TetheredHorse,
    /// Depth of the store's veranda, in meters: the rail stands just past
    /// it, so narrow streets get a shallower veranda.
    pub veranda: f32,
}

/// A hitching rail (palenque) in front of a landmark besides the general
/// store's own [`HitchingPost`]: the other general stores, the comisarías,
/// the church and the station. A city ties a horse to a few of them
/// ([`Palenque::MAX_HORSES`]); a town or village leaves them empty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palenque {
    /// The building it stands before: an index into [`TownLayout::buildings`].
    pub building: usize,
    pub rail: Aabb,
    /// The horse tied alongside it, if any.
    pub horse: Option<TetheredHorse>,
}

impl Palenque {
    /// Most horses a city ties at its palenques, so its animated figures
    /// stay few.
    pub const MAX_HORSES: usize = 3;
    /// Length of the rail along the street, in meters.
    pub const LENGTH: f32 = 2.8;
    /// Width of the rail across the street, in meters.
    pub const WIDTH: f32 = 0.2;
    /// Gap kept between a palenque (and its horse's spot) and any building,
    /// tree, sign, post or other obstacle, in meters.
    pub const CLEARANCE: f32 = 0.5;
    /// Gap kept round a street crossing, where the corner lamps stand.
    pub const CROSSING_MARGIN: f32 = 1.0;
    /// How far from the street's middle line a palenque stays: the lane the
    /// walkers use.
    pub const LANE: f32 = 1.0;
    /// How far out of its facade the station's palenque stands on the yard,
    /// meters.
    pub const YARD_OUT: f32 = 2.7;

    /// How many of `posts` palenques tie a horse in a city: half of them, at
    /// most [`Self::MAX_HORSES`].
    pub fn horses_for(posts: usize) -> usize {
        (posts / 2).min(Self::MAX_HORSES)
    }

    /// Its static obstacles: the rail, and the horse when there is one.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        std::iter::once(self.rail).chain(self.horse.map(|h| h.footprint))
    }
}

/// Where a [`ShadeTree`] stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShadeKind {
    /// A street tree on a city's sidewalk, in its cazuela (a small earth
    /// square rimmed in the paving), with a trimmed crown.
    Pit,
    /// A tree on the dirt in front of a house of a town or village, with a
    /// trimmed crown.
    Front,
    /// A tree in a city's plaza, in the green between its paths.
    Grove,
    /// A tree in a yard or beside a landmark, with a free crown.
    Yard,
}

/// A tree outside the ring of [`TownLayout::trees`]: the street trees, the
/// plaza groves and the yard trees. Its trunk is an obstacle like a plaza
/// tree's ([`TownLayout::obstacles`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadeTree {
    /// The middle of the trunk.
    pub at: Vec2,
    pub kind: ShadeKind,
    /// The widest its crown may reach from the trunk, in meters: render
    /// narrows the tree to fit it, and core keeps every building that far
    /// (and a margin) from the trunk.
    pub crown: f32,
    /// The unit ground vector the trunk and crown lean along, toward the
    /// street a trimmed tree stands beside; zero for the others.
    pub toward: Vec2,
    /// The sidewalk strip of a [`ShadeKind::Pit`] tree: a row shares a species.
    pub row: u32,
}

impl ShadeTree {
    /// Side of a cazuela, rim included, in meters.
    pub const PIT_SIDE: f32 = 0.8;
    /// Gap between a cazuela and the curb, in meters.
    pub const PIT_FROM_CURB: f32 = 0.4;
    /// Crown of a pit tree: a 2.5 m sidewalk leaves its trunk 1.7 m off the
    /// facade, so the crown stays narrow (trimmed) and leans over the street.
    pub const PIT_CROWN: f32 = 2.3;
    /// How far a front tree stands from the facade, in meters.
    pub const FRONT_OUT: f32 = 2.2;
    /// Crown of a tree on the dirt in front of a house.
    pub const FRONT_CROWN: f32 = 1.9;
    /// Least gap between two trees on the dirt in front of houses, meters.
    pub const FRONT_APART: f32 = 13.0;
    /// A town has no rows of front trees: within this reach of any one
    /// stand at most two others, meters.
    pub const FRONT_RUN: f32 = 30.0;
    /// Crown of a yard tree.
    pub const YARD_CROWN: f32 = 3.0;
    /// Gap kept between a yard crown and any building, meters.
    pub const BUILDING_GAP: f32 = 0.6;
    /// Gap between a street tree and the next one along its sidewalk, meters
    /// (min, max). 15% more trees than the first planting (user, 2026-10-27:
    /// "subile otro 15% a pueblos y aldeas"): (17, 21.2) / 1.15.
    pub const SPACING: (f32, f32) = (14.8, 18.4);
    /// Least gap between two trunks, whatever they are, in meters.
    pub const APART: f32 = 4.0;
    /// How far a trimmed tree leans toward the street, in degrees.
    pub const LEAN: f32 = 5.0;
    /// Crown of a plaza grove tree: the species' own, uncapped.
    pub const GROVE_CROWN: f32 = 6.0;
    /// How far a grove stays in from its plaza's edge, meters.
    const GROVE_INSET: f32 = 4.0;
    /// Gap between the spots a grove is laid out on, meters (was 10 m; the
    /// plaza ring grew too, and a very big town's cut takes its share of
    /// the groves).
    const GROVE_STEP: f32 = 8.8;
    /// How far each grove spot strays, meters.
    const GROVE_JITTER: f32 = 2.5;
    /// How far from a block's edge a yard tree stands at least, meters.
    const YARD_INSET: f32 = 3.0;
    /// How far past a landmark's walls its trees may stand, meters.
    const LANDMARK_REACH: f32 = 7.0;

    /// The ground a trunk at `at` takes.
    fn trunk(at: Vec2) -> Aabb {
        let side = 2.0 * TREE_TRUNK_RADIUS;
        Aabb::from_center_size(at, Vec2::new(side, side))
    }

    /// The tree's trunk as the obstacle people and horses bump into.
    pub fn obstacle(&self) -> Aabb {
        Self::trunk(self.at)
    }
}

/// What a [`YardPlant`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlantKind {
    /// A low shrub against a wall.
    Shrub,
    /// A small flower bed.
    Flowers,
}

impl PlantKind {
    /// Half the side of the ground it takes, in meters.
    pub fn radius(self) -> f32 {
        match self {
            Self::Shrub => 0.6,
            Self::Flowers => 0.7,
        }
    }
}

/// A shrub or a flower bed in a house's yard: scenery with no obstacle,
/// kept off the streets, doors, other buildings and trunks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YardPlant {
    pub at: Vec2,
    pub kind: PlantKind,
}

impl YardPlant {
    /// The ground it covers.
    pub fn area(&self) -> Aabb {
        let side = 2.0 * self.kind.radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
    }
}

/// Where the plaza goes in the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlazaPlacement {
    /// The central block when there is one; otherwise the block facing the
    /// station; otherwise (no railway) the block nearest the center.
    #[default]
    Auto,
    /// The central block (both block counts must be odd).
    Center,
    /// The block of the row (or column) next to the railway nearest the
    /// grid center; the station is aligned to face it across the street.
    FacingStation,
    /// Block `(i, j)`, counted from the south-west corner.
    Block(usize, usize),
}

/// Which side of the grid the railway runs along, and how far out; or, for
/// a town grown on both sides of its line, which half of the town its
/// station stands in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailwayConfig {
    pub side: Side,
    /// Distance from the grid edge (outer edge of the ring street) to the
    /// track centerline, in meters. For a `through` line, from the edge of
    /// the station's half of the town.
    pub offset: f32,
    /// The track runs east-west through the middle of the town, between its
    /// two middle rows of blocks, instead of along one side: the town grows
    /// on both sides of the line, like the big railway towns. `side`
    /// (`North` or `South`) is then the half the station stands in, facing
    /// it across the street. The streets running north-south cross the
    /// track at level crossings, except where the station yard stands.
    pub through: bool,
}

impl RailwayConfig {
    /// Least rows of blocks a town with a `through` line needs: two on each
    /// side of the track.
    pub const MIN_THROUGH_ROWS: usize = 4;
    /// For a `through` line, how far the track runs from the edge of the
    /// half of the town across from the station, in meters.
    pub const THROUGH_FAR_SIDE: f32 = 16.0;
}

/// Everything needed to generate a town.
#[derive(Debug, Clone, PartialEq)]
pub struct TownConfig {
    /// The town's name (painted on its station).
    pub name: String,
    /// Center of the street grid in world coordinates.
    pub origin: Vec2,
    /// Blocks `(east_west, north_south)`: columns and rows of the grid.
    pub blocks: (usize, usize),
    /// Side of one square block, in meters.
    pub block_size: f32,
    /// Width of every street, ring street included, in meters.
    pub street_width: f32,
    pub railway: Option<RailwayConfig>,
    pub plaza: PlazaPlacement,
    /// Varies house and shed lots.
    pub seed: u32,
    /// How many police stations the town has, overriding its size tier's
    /// count ([`TownSize`]); `None` keeps the tier's. At most the tier's
    /// count (a town without a comisaría is `Some(0)`).
    pub police_stations: Option<usize>,
    /// Share of the house and shed lots that are built, from 0 to 1; `None`
    /// builds them all as usual. A village or a railway halt has few.
    pub density: Option<f32>,
    /// How many people walk the town, overriding its size tier's crowd
    /// ([`TownSize`]); `None` keeps the tier's. At most
    /// [`TownConfig::MAX_TOWNSFOLK`] (an empty halt is `Some(0)`).
    pub townsfolk: Option<usize>,
}

impl TownConfig {
    /// Fewest blocks along either axis.
    pub const MIN_BLOCKS: usize = 3;
    /// Smallest block that still fits the landmarks and lots, in meters.
    pub const MIN_BLOCK_SIZE: f32 = 30.0;
    /// Narrowest street that leaves a 3 m carriageway between two 3.5 m
    /// sidewalks, room for passers-by (the shipped towns are 12 m or wider,
    /// a 5 m carriageway; a tied horse needs 12 m).
    pub const MIN_STREET_WIDTH: f32 = 10.0;
    /// Closest the track may run to the grid: the station yard needs it.
    pub const MIN_RAILWAY_OFFSET: f32 = 24.0;
    /// Most people a town may ask for in `townsfolk`.
    pub const MAX_TOWNSFOLK: usize = 200;

    /// Eduardo Castex-inspired town: 4 x 5 blocks of 40 m (4 east-west, 5
    /// north-south), 12 m streets, railway 24 m north of the grid, at the
    /// world center. With no central block, `Auto` puts the plaza in the top
    /// row facing the station. The built-in fallback when
    /// `assets/world/towns.ron` cannot be used; the shipped Eduardo Castex
    /// there is laid out differently (5 x 4 blocks, at its place on the line).
    pub fn castex() -> Self {
        Self {
            name: "Eduardo Castex".to_string(),
            origin: Vec2::ZERO,
            blocks: (4, 5),
            block_size: 40.0,
            street_width: 12.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 7,
            police_stations: None,
            density: None,
            townsfolk: None,
        }
    }

    /// How big the town is, from its block count.
    pub fn size(&self) -> TownSize {
        TownSize::of(self.blocks)
    }

    /// How many police stations the town gets: the override, or its size
    /// tier's count.
    pub fn police_count(&self) -> usize {
        self.police_stations
            .unwrap_or_else(|| self.size().police_stations())
    }

    /// How many people walk the town: the override, or its size tier's
    /// crowd.
    pub fn townsfolk_count(&self) -> usize {
        self.townsfolk
            .unwrap_or_else(|| self.size().townsfolk(self.blocks))
    }
}

/// A town's size tier, from its block count `n = columns * rows`. It sets
/// the main plaza, the station plaza, the landmarks and the crowd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TownSize {
    /// Up to [`TownSize::SMALL_MAX`] blocks: a one-block plaza, one general
    /// store, one police station and [`crate::townsfolk::TOWNSFOLK_COUNT`]
    /// people.
    Small,
    /// Up to [`TownSize::BIG_MAX`] blocks: a central plaza of two blocks, a
    /// station plaza, two general stores and one police station.
    Big,
    /// More blocks: a central plaza of 2 x 2 blocks, a station plaza, two
    /// general stores and two police stations.
    VeryBig,
}

impl TownSize {
    /// Most blocks of a small town.
    pub const SMALL_MAX: usize = 36;
    /// Most blocks of a big town.
    pub const BIG_MAX: usize = 63;

    /// The tier of a grid of `blocks` (columns, rows).
    pub fn of(blocks: (usize, usize)) -> Self {
        match blocks.0 * blocks.1 {
            n if n <= Self::SMALL_MAX => Self::Small,
            n if n <= Self::BIG_MAX => Self::Big,
            _ => Self::VeryBig,
        }
    }

    /// Blocks the main plaza spans `(east-west, north-south)` on a grid of
    /// `blocks`: a big town merges two along its longer side (east-west on
    /// a square grid).
    /// Never more blocks than the grid has, should a grid ever be narrower
    /// than [`TownConfig::MIN_BLOCKS`] allows today.
    pub(crate) fn plaza_span(self, blocks: (usize, usize)) -> (usize, usize) {
        let (w, h) = match self {
            Self::Small => (1, 1),
            Self::Big if blocks.0 >= blocks.1 => (2, 1),
            Self::Big => (1, 2),
            Self::VeryBig => (2, 2),
        };
        (w.min(blocks.0.max(1)), h.min(blocks.1.max(1)))
    }

    /// General stores in a town of this size: one per half of a town
    /// split by its line, or one by each plaza (user, 2026-10-23).
    fn general_stores(self) -> usize {
        match self {
            Self::Small => 1,
            Self::Big | Self::VeryBig => 2,
        }
    }

    /// Police stations, unless a town overrides it.
    pub fn police_stations(self) -> usize {
        match self {
            Self::Small | Self::Big => 1,
            Self::VeryBig => 2,
        }
    }

    /// People walking a town of `blocks`: the small-town crowd, or four for
    /// every five blocks in bigger towns, never fewer.
    fn townsfolk(self, blocks: (usize, usize)) -> usize {
        let small = crate::townsfolk::TOWNSFOLK_COUNT;
        match self {
            Self::Small => small,
            Self::Big | Self::VeryBig => {
                let n = (blocks.0 * blocks.1) as f32;
                small.max((0.8 * n).round() as usize)
            }
        }
    }
}

/// Why a [`TownConfig`] cannot be built.
#[derive(Debug, Clone, PartialEq)]
pub enum TownConfigError {
    /// The name is empty or blank.
    EmptyName,
    /// A number is NaN or infinite.
    NotFinite { field: &'static str },
    /// Fewer than [`TownConfig::MIN_BLOCKS`] blocks along an axis.
    TooFewBlocks { blocks: (usize, usize), min: usize },
    /// Blocks smaller than [`TownConfig::MIN_BLOCK_SIZE`].
    BlockTooSmall { block_size: f32, min: f32 },
    /// Streets narrower than [`TownConfig::MIN_STREET_WIDTH`].
    StreetTooNarrow { street_width: f32, min: f32 },
    /// Track closer than [`TownConfig::MIN_RAILWAY_OFFSET`] to the grid.
    RailwayTooClose { offset: f32, min: f32 },
    /// `PlazaPlacement::Block` outside the grid.
    PlazaOutOfRange {
        block: (usize, usize),
        blocks: (usize, usize),
    },
    /// `PlazaPlacement::Center` on a grid with an even block count.
    NoCentralBlock { blocks: (usize, usize) },
    /// `PlazaPlacement::FacingStation` in a town without a railway.
    NoStationToFace,
    /// `police_stations` asks for more than the size tier places.
    TooManyPoliceStations { wanted: usize, max: usize },
    /// `density` outside 0 to 1.
    BadDensity { density: f32 },
    /// `townsfolk` asks for more than [`TownConfig::MAX_TOWNSFOLK`].
    TooManyTownsfolk { wanted: usize, max: usize },
    /// A `through` railway on the east or west side: it only runs east-west.
    ThroughNotEastWest { side: Side },
    /// A `through` railway in a town with too few rows to grow on both
    /// sides of it ([`RailwayConfig::MIN_THROUGH_ROWS`]).
    ThroughTooFewRows { rows: usize, min: usize },
    /// The plaza spans both sides of a `through` railway.
    PlazaAcrossRailway,
}

impl fmt::Display for TownConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "the town has no name"),
            Self::NotFinite { field } => write!(f, "`{field}` is not a finite number"),
            Self::TooFewBlocks { blocks, min } => write!(
                f,
                "blocks {blocks:?} are too few: at least {min} along each axis"
            ),
            Self::BlockTooSmall { block_size, min } => write!(
                f,
                "block_size {block_size} m is too small for the landmarks (min {min} m)"
            ),
            Self::StreetTooNarrow { street_width, min } => write!(
                f,
                "street_width {street_width} m is too narrow (min {min} m)"
            ),
            Self::RailwayTooClose { offset, min } => write!(
                f,
                "railway offset {offset} m leaves no room for the station yard (min {min} m)"
            ),
            Self::PlazaOutOfRange { block, blocks } => write!(
                f,
                "plaza block {block:?} is outside the {}x{} grid",
                blocks.0, blocks.1
            ),
            Self::NoCentralBlock { blocks } => write!(
                f,
                "plaza `Center` needs a central block, but the {}x{} grid has an even side",
                blocks.0, blocks.1
            ),
            Self::NoStationToFace => {
                write!(
                    f,
                    "plaza `FacingStation` needs a railway, but there is none"
                )
            }
            Self::TooManyPoliceStations { wanted, max } => write!(
                f,
                "police_stations {wanted} is more than a town of this size has (max {max})"
            ),
            Self::BadDensity { density } => {
                write!(f, "density {density} is outside 0 to 1")
            }
            Self::TooManyTownsfolk { wanted, max } => {
                write!(
                    f,
                    "townsfolk {wanted} is more than a town holds (max {max})"
                )
            }
            Self::ThroughNotEastWest { side } => write!(
                f,
                "a `through` railway runs east-west: its side must be North or South, not {side:?}"
            ),
            Self::ThroughTooFewRows { rows, min } => write!(
                f,
                "a `through` railway needs at least {min} rows of blocks, the town has {rows}"
            ),
            Self::PlazaAcrossRailway => {
                write!(f, "the plaza spans both sides of the `through` railway")
            }
        }
    }
}

impl std::error::Error for TownConfigError {}

/// A small railway town laid out on an orthogonal grid.
#[derive(Debug, Clone, PartialEq)]
pub struct TownLayout {
    /// The town's name, from its config.
    pub name: String,
    /// Street surfaces, including the ring street around the grid.
    pub streets: Vec<Aabb>,
    /// The main square: one block, or several merged with the streets
    /// between them in bigger towns.
    pub plaza: Aabb,
    /// A bigger town's second, smaller square by the station: half a block.
    pub station_plaza: Option<Aabb>,
    /// The town's size tier.
    pub size: TownSize,
    /// Trunk positions of every plaza's ring trees.
    pub trees: Vec<Vec2>,
    /// The other trees: sidewalk and dirt street trees, plaza groves, yard
    /// and landmark trees ([`ShadeTree`]).
    pub shade_trees: Vec<ShadeTree>,
    /// The shrubs and flower beds of the yards behind the houses
    /// ([`YardPlant`]).
    pub yard_plants: Vec<YardPlant>,
    pub railway: Option<Railway>,
    pub buildings: Vec<Building>,
    /// Hitching rail in front of the (first) general store.
    pub hitching_post: HitchingPost,
    /// The other hitching rails: before the remaining general stores, the
    /// comisarías, the church and the station ([`Palenque`]).
    pub palenques: Vec<Palenque>,
    /// The config's seed, also used for the town's crowd.
    pub seed: u32,
    /// People walking the streets ([`TownConfig::townsfolk_count`]).
    townsfolk: usize,
    blocks: (usize, usize),
    grid: Aabb,
    extent: Aabb,
    arrival: (Vec2, Heading),
    /// The strip between the two halves of a town grown on both sides of
    /// its line ([`RailwayConfig::through`]): the track and station yard.
    band: Option<Aabb>,
    /// Its brick sidewalks ([`Paving`]).
    sidewalks: Vec<Sidewalk>,
}

/// Trunk radius used for tree collision.
pub const TREE_TRUNK_RADIUS: f32 = 0.35;

/// Width of the two paths crossing every plaza, in meters.
pub const PLAZA_PATH_WIDTH: f32 = 3.0;
/// Side of the square monument plinth where a plaza's paths meet, meters.
pub const PLAZA_MONUMENT: f32 = 3.0;
/// Least gap between a plaza tree's trunk and a path or the monument, in
/// meters: trees stand in the green beds between the paths.
pub const PLAZA_PATH_CLEARANCE: f32 = 1.5;

/// The two paths crossing `plaza` through its center, edge to edge: the
/// east–west one, then the north–south one.
pub fn plaza_paths(plaza: Aabb) -> [Aabb; 2] {
    let (c, s) = (plaza.center(), plaza.size());
    [
        Aabb::from_center_size(c, Vec2::new(s.x, PLAZA_PATH_WIDTH)),
        Aabb::from_center_size(c, Vec2::new(PLAZA_PATH_WIDTH, s.y)),
    ]
}

/// The monument's plinth at the center of `plaza`, where its paths meet.
pub fn plaza_monument(plaza: Aabb) -> Aabb {
    Aabb::from_center_size(plaza.center(), Vec2::new(PLAZA_MONUMENT, PLAZA_MONUMENT))
}

/// Side of the paved square where a plaza's paths cross, meters: a path's
/// width all round the monument's plinth, so people and riders walk round
/// the monument.
pub const PLAZA_CROSSING: f32 = PLAZA_MONUMENT + 2.0 * PLAZA_PATH_WIDTH;

/// The paved square round the monument of `plaza`, where its paths cross.
pub fn plaza_crossing(plaza: Aabb) -> Aabb {
    Aabb::from_center_size(plaza.center(), Vec2::new(PLAZA_CROSSING, PLAZA_CROSSING))
}

/// A slatted plaza bench (banco de plaza) on cast-iron ends, in a green
/// bed beside a path, facing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bench {
    /// The middle of its footprint.
    pub at: Vec2,
    /// Where someone sitting on it looks: toward the path.
    pub facing: Side,
}

impl Bench {
    /// Length of a bench along its path, meters.
    pub const LENGTH: f32 = 1.8;
    /// Depth from the front of its seat to the back of its feet, meters.
    pub const DEPTH: f32 = 0.6;
    /// Gap between the path's edge and the front of the bench, meters:
    /// room for the path's curb and the sitter's feet.
    pub const PATH_GAP: f32 = 0.3;
    /// Least gap between two benches along a path, meters.
    pub const SPACING: f32 = 1.0;

    /// Its static obstacle: the whole bench.
    pub fn footprint(&self) -> Aabb {
        let size = match self.facing {
            Side::North | Side::South => Vec2::new(Self::LENGTH, Self::DEPTH),
            Side::East | Side::West => Vec2::new(Self::DEPTH, Self::LENGTH),
        };
        Aabb::from_center_size(self.at, size)
    }
}

/// What stands in one plaza besides its trees, by its town's [`Paving`]
/// (phase 3 of the town facades): the monument on its plinth, the paved
/// crossing round it, benches beside the paths and, in a city only, a low
/// iron railing round the edge, open at each path's mouth. The
/// monument, the benches and the railing are static obstacles; the paths
/// and the crossing stay clear, so the streets, the townsfolk's graph and
/// every lamp keep where they were.
#[derive(Debug, Clone, PartialEq)]
pub struct PlazaFurniture {
    /// The plaza.
    pub area: Aabb,
    /// Its town's paving, which also sets how grand the plaza is.
    pub paving: Paving,
    /// The monument's collider: its lowest step, inside [`plaza_monument`].
    pub monument: Aabb,
    /// The paved square round the monument ([`plaza_crossing`]).
    pub crossing: Aabb,
    pub benches: Vec<Bench>,
    /// The railing's pieces along the edge, each a thin box.
    pub railing: Vec<Aabb>,
}

impl PlazaFurniture {
    /// How far in from the plaza's edge the railing starts, meters.
    pub const RAILING_INSET: f32 = 0.05;
    /// Depth of the railing, its piers included, meters: it stays out of
    /// the half meter round the lamps at the path mouths, a meter in from
    /// the edge.
    pub const RAILING_DEPTH: f32 = 0.3;
    /// How far to either side of a path's middle the railing opens: the
    /// path and its curbs.
    pub const MOUTH_HALF: f32 = PLAZA_PATH_WIDTH / 2.0 + 0.15;
    /// Least gap from the crossing to the first bench along a path, meters.
    pub const CROSSING_GAP: f32 = 0.5;
    /// Least gap from the plaza's edge to the last bench along a path,
    /// meters: past the lamps at the path mouths.
    pub const EDGE_GAP: f32 = 2.6;

    /// Side of the monument's lowest step: smaller in a village.
    pub fn monument_side(paving: Paving) -> f32 {
        match paving {
            Paving::None => 2.4,
            Paving::Plaza | Paving::Everywhere => PLAZA_MONUMENT,
        }
    }

    /// How the benches are spread (user, 2026-10-26: small plazas stay
    /// sparse): the paths out of the crossing that get any (east, north,
    /// west, south), how many each, and whether on both sides of the path
    /// or only on its left, one per quadrant like a pinwheel. A city has
    /// three on each side of every path (one in its station plaza), a town
    /// one by each path, a village one by the east and one by the west
    /// path.
    pub fn bench_plan(paving: Paving, main: bool) -> (&'static [usize], usize, bool) {
        match paving {
            Paving::Everywhere => (&[0, 1, 2, 3], if main { 3 } else { 1 }, true),
            Paving::Plaza => (&[0, 1, 2, 3], 1, false),
            Paving::None => (&[0, 2], 1, false),
        }
    }

    /// How many benches a plaza gets when they all fit ([`Self::bench_plan`]).
    pub fn bench_count(paving: Paving, main: bool) -> usize {
        let (arms, per, both) = Self::bench_plan(paving, main);
        arms.len() * per * if both { 2 } else { 1 }
    }

    /// The furniture of `area`, the town's main plaza when `main`, in a
    /// town paved as `paving`. A bench that would stand on anything of
    /// `keep_off` (tree trunks, signs) is left out.
    pub fn of(area: Aabb, paving: Paving, main: bool, keep_off: &[Aabb]) -> Self {
        let c = area.center();
        let side = Self::monument_side(paving);
        Self {
            area,
            paving,
            monument: Aabb::from_center_size(c, Vec2::new(side, side)),
            crossing: plaza_crossing(area),
            benches: benches(area, Self::bench_plan(paving, main), keep_off),
            // Only a city fences its plaza (user, 2026-10-26).
            railing: match paving {
                Paving::None | Paving::Plaza => Vec::new(),
                Paving::Everywhere => railing(area),
            },
        }
    }

    /// Its static obstacles: the monument, the benches and the railing.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        std::iter::once(self.monument)
            .chain(self.benches.iter().map(Bench::footprint))
            .chain(self.railing.iter().copied())
    }
}

/// The benches of `area` as `plan` spreads them ([`PlazaFurniture::bench_plan`]),
/// each path's spread evenly between the crossing and the lamps at its
/// mouth, clear of `keep_off`.
fn benches(area: Aabb, plan: (&[usize], usize, bool), keep_off: &[Aabb]) -> Vec<Bench> {
    let (arms, per_side, both) = plan;
    let (c, half) = (area.center(), area.size() * 0.5);
    let lateral = PLAZA_PATH_WIDTH / 2.0 + Bench::PATH_GAP + Bench::DEPTH / 2.0;
    let start = PLAZA_CROSSING / 2.0 + PlazaFurniture::CROSSING_GAP + Bench::LENGTH / 2.0;
    let mut out = Vec::new();
    let paths = [
        (Side::East, Side::North),
        (Side::North, Side::West),
        (Side::West, Side::South),
        (Side::South, Side::East),
    ];
    for (n, across) in arms.iter().map(|&k| paths[k]) {
        let (n, left) = (
            crate::places::side_normal(n),
            crate::places::side_normal(across),
        );
        let reach = n.x.abs() * half.x + n.y.abs() * half.y;
        let end = reach - PlazaFurniture::EDGE_GAP - Bench::LENGTH / 2.0;
        if end < start {
            continue;
        }
        let pitch = Bench::LENGTH + Bench::SPACING;
        let fit = ((end - start) / pitch) as usize + 1;
        let count = per_side.min(fit);
        for k in 0..count {
            let t = if count == 1 {
                (start + end) / 2.0
            } else {
                start + (end - start) * k as f32 / (count - 1) as f32
            };
            for s in if both { &[1.0, -1.0][..] } else { &[1.0][..] } {
                let s = *s;
                let at = c + n * t + left * (s * lateral);
                let facing = if s > 0.0 { across.opposite() } else { across };
                let bench = Bench { at, facing };
                let room = bench.footprint().expanded(0.1);
                if keep_off.iter().all(|o| !o.intersects(&room)) {
                    out.push(bench);
                }
            }
        }
    }
    out
}

/// The railing round `area`: along each edge, just inside it, open at the
/// path's mouth. The north and south runs reach the corners; the east and
/// west ones run between them.
fn railing(area: Aabb) -> Vec<Aabb> {
    let (i, d) = (PlazaFurniture::RAILING_INSET, PlazaFurniture::RAILING_DEPTH);
    let (min, max, c) = (area.min, area.max, area.center());
    let m = PlazaFurniture::MOUTH_HALF;
    let mut out = Vec::new();
    for (lo, hi) in [(min.y + i, min.y + i + d), (max.y - i - d, max.y - i)] {
        out.push(Aabb::new(Vec2::new(min.x + i, lo), Vec2::new(c.x - m, hi)));
        out.push(Aabb::new(Vec2::new(c.x + m, lo), Vec2::new(max.x - i, hi)));
    }
    let (y0, y1) = (min.y + i + d, max.y - i - d);
    for (lo, hi) in [(min.x + i, min.x + i + d), (max.x - i - d, max.x - i)] {
        out.push(Aabb::new(Vec2::new(lo, y0), Vec2::new(hi, c.y - m)));
        out.push(Aabb::new(Vec2::new(lo, c.y + m), Vec2::new(hi, y1)));
    }
    out
}

impl TownLayout {
    /// Builds the town `config` describes, or explains why it cannot.
    pub fn generate(config: &TownConfig) -> Result<Self, TownConfigError> {
        let grid = Grid::validated(config)?;
        let size = config.size();
        let main = main_plaza(config, size)?;
        let plaza = grid.span(main);
        let bounds = grid.bounds();
        let extent = grid.extent(config.railway);

        let police = config.police_count();
        let (landmarks, station) = place_landmarks(&grid, main, size, police, config.railway);
        let mut reserved: Vec<(usize, usize)> = landmarks.iter().map(|l| l.block).collect();
        reserved.extend(main.blocks());
        reserved.extend(station.map(|s| s.block));
        let windmills = windmill_blocks(&grid, &reserved);
        let mut buildings = Vec::new();
        for j in 0..grid.rows {
            for i in 0..grid.cols {
                if main.contains((i, j)) {
                    continue;
                }
                let landmark = landmarks.iter().find(|c| c.block == (i, j));
                fill_block(
                    &grid,
                    (i, j),
                    landmark,
                    windmills.contains(&(i, j)),
                    station.filter(|s| s.block == (i, j)).map(|s| s.area),
                    config,
                    &mut buildings,
                );
            }
        }
        // The first store, church and police station (if the town has one)
        // come first, in block order; extra landmarks follow in the order
        // they were placed, so the first general store found is the one the
        // horse is tied at.
        let primary = LANDMARK_PREFERENCES.len() - usize::from(police == 0);
        for extra in &landmarks[primary..] {
            let inside = grid.block(extra.block.0, extra.block.1);
            if let Some(k) = buildings
                .iter()
                .position(|b| b.kind == extra.kind && inside.contains(&b.footprint))
            {
                let landmark = buildings.remove(k);
                buildings.push(landmark);
            }
        }
        let yard_start = buildings.len();
        let railway = config.railway.map(|r| {
            let (half, track) = grid.yard_half(r);
            let along = grid.along_half(track);
            // A through line's station stands in the middle of the town;
            // one beside the town faces the plaza.
            let wanted = if r.through {
                0.0
            } else {
                (plaza.center() - bounds.center()).dot(axis_along(track))
            };
            buildings.extend(station_yard(half, track, r.offset, along, wanted));
            railway_line(half, track, r.offset, along)
        });
        let yard: Vec<Aabb> = buildings[yard_start..]
            .iter()
            .map(|b| b.footprint)
            .collect();

        let store = buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .expect("every town has a general store");
        let hitching_post = hitching_post(store, veranda_depth(grid.street));
        let (w, h) = main.span();
        let mut town = Self {
            name: config.name.clone(),
            streets: grid.streets(main, &yard),
            plaza,
            station_plaza: station.map(|s| s.area),
            size,
            trees: plaza_trees(plaza, (more_steps(4 * w), more_steps(4 * h)))
                .into_iter()
                .chain(station.into_iter().flat_map(|s| {
                    plaza_trees(
                        s.area,
                        (more_steps(s.tree_steps.0), more_steps(s.tree_steps.1)),
                    )
                }))
                .collect(),
            shade_trees: Vec::new(),
            yard_plants: Vec::new(),
            railway,
            buildings,
            hitching_post,
            palenques: Vec::new(),
            seed: config.seed,
            townsfolk: config.townsfolk_count(),
            blocks: config.blocks,
            grid: bounds,
            extent,
            arrival: arrival(&grid, main, config.railway),
            band: grid.band(),
            sidewalks: sidewalks(&grid, main, Paving::of(config.blocks)),
        };
        town.palenques = town.place_palenques();
        town.shade_trees = town.place_shade_trees(&grid, main);
        town.yard_plants = town.place_yard_plants();
        Ok(town)
    }

    /// A small pampa railway town inspired by 1920s–1930s settlements of La
    /// Pampa such as Eduardo Castex, generated from [`TownConfig::castex`].
    /// It borrows the general pattern (grid, plaza, railway at the edge,
    /// general store, grain sheds, windmills) and is not a reconstruction of
    /// any real town.
    pub fn castex_inspired() -> Self {
        Self::generate(&TownConfig::castex()).expect("the built-in Castex config is valid")
    }

    /// Its brick sidewalks: along every block front in a big town, round
    /// the plaza in a small one, none in a village ([`Paving`]).
    pub fn sidewalks(&self) -> &[Sidewalk] {
        &self.sidewalks
    }

    /// Every plaza: the main one, then the station plaza of a bigger town.
    pub fn plazas(&self) -> impl Iterator<Item = Aabb> + '_ {
        std::iter::once(self.plaza).chain(self.station_plaza)
    }

    /// What stands in each of [`Self::plazas`], in the same order: its
    /// monument, crossing, benches and railing ([`PlazaFurniture`]), clear
    /// of its trees and signs.
    pub fn plaza_furniture(&self) -> Vec<PlazaFurniture> {
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        let keep_off: Vec<Aabb> = self
            .trees
            .iter()
            .map(|&p| Aabb::from_center_size(p, trunk))
            .chain(self.sign_obstacles())
            .collect();
        let paving = Paving::of(self.blocks);
        self.plazas()
            .enumerate()
            .map(|(k, p)| PlazaFurniture::of(p, paving, k == 0, &keep_off))
            .collect()
    }

    /// How many people walk the town's streets: its config's `townsfolk`,
    /// or else 16 in a small town and four for every five blocks (at least
    /// 16) in bigger ones.
    pub fn townsfolk_count(&self) -> usize {
        self.townsfolk
    }

    /// Blocks `(east_west, north_south)`, from its config.
    pub fn blocks(&self) -> (usize, usize) {
        self.blocks
    }

    /// Bounding box of the street grid, ring street included.
    pub fn grid(&self) -> Aabb {
        self.grid
    }

    /// Bounding box of everything built: grid, station yard and railway corridor
    /// within the town.
    pub fn extent(&self) -> Aabb {
        self.extent
    }

    /// The extent plus the band where the leveled ground blends back into
    /// the pampa: everything the town changes on the ground.
    pub fn leveled(&self) -> Aabb {
        self.extent.expanded(Self::LEVELED_BAND)
    }

    /// The strip the railway runs along, a few meters to either side of the
    /// track, from one end of the line to the other.
    pub fn rail_corridor(&self) -> Option<Aabb> {
        let r = self.railway?;
        let min = Vec2::new(r.start.x.min(r.end.x), r.start.y.min(r.end.y));
        let max = Vec2::new(r.start.x.max(r.end.x), r.start.y.max(r.end.y));
        Some(Aabb::new(min, max).expanded(Self::RAIL_CORRIDOR_HALF))
    }

    /// The strip between the two halves of a town grown on both sides of
    /// its line ([`RailwayConfig::through`]), from one end of the grid to the
    /// other: the track, the station yard and the level crossings. `None`
    /// for a town beside its line or without one.
    pub fn rail_band(&self) -> Option<Aabb> {
        self.band
    }

    /// The streets that cross a [`Self::rail_band`] at a level crossing:
    /// each one's centerline across the band, from the middle of the ring
    /// street on one side to the middle of the one on the other, south to
    /// north, west to east.
    pub fn crossing_streets(&self) -> Vec<(Vec2, Vec2)> {
        let Some(band) = self.band else {
            return Vec::new();
        };
        let mut out: Vec<(Vec2, Vec2)> = self
            .streets
            .iter()
            .filter(|s| s.size().y > s.size().x)
            .filter(|s| s.min.y < band.min.y && s.max.y > band.max.y)
            .map(|s| {
                let (x, half) = (s.center().x, s.size().x / 2.0);
                (
                    Vec2::new(x, band.min.y - half),
                    Vec2::new(x, band.max.y + half),
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        out
    }

    /// Ground to level under the town, at the natural height of its center.
    pub fn flat_area(&self, terrain: &Terrain) -> FlatArea {
        let extent = self.extent();
        let center = extent.center();
        FlatArea {
            center,
            half_extents: extent.size() * 0.5,
            falloff: Self::LEVELED_BAND,
            height: terrain.natural_height(center.x, center.y),
        }
    }

    /// Static collision footprints: every building's [`Building::collider`]
    /// but the platform's (a floor people walk on, see [`Self::floors`]),
    /// every tree trunk (the plaza rings' and the [`ShadeTree`]s'), the
    /// hitching rail with its horse, the palenques with their horses,
    /// free-standing signs, and each plaza's monument, benches and railing
    /// ([`PlazaFurniture::obstacles`]).
    pub fn obstacles(&self) -> Vec<Aabb> {
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        self.buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(Building::collider)
            .chain(self.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)))
            .chain(self.shade_trees.iter().map(ShadeTree::obstacle))
            .chain([self.hitching_post.rail, self.hitching_post.horse.footprint])
            .chain(self.palenques.iter().flat_map(Palenque::obstacles))
            .chain(self.sign_obstacles())
            .chain(
                self.plaza_furniture()
                    .iter()
                    .flat_map(PlazaFurniture::obstacles)
                    .collect::<Vec<_>>(),
            )
            .collect()
    }

    /// Where a rider arriving from the open pampa starts: on the main road
    /// outside the grid, on the side away from the railway, facing into town.
    pub fn arrival(&self) -> (Vec2, Heading) {
        self.arrival
    }

    /// Raised floors people walk on: the station platform, running along
    /// the track.
    pub fn floors(&self) -> Vec<Floor> {
        self.buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Platform)
            .map(|b| Floor {
                footprint: b.footprint,
                height: b.height,
                along: match b.front {
                    Side::North | Side::South => Vec2::new(1.0, 0.0),
                    Side::East | Side::West => Vec2::new(0.0, 1.0),
                },
            })
            .collect()
    }

    /// Roofs over the platform: each station's canopy, reaching
    /// [`Canopy::DEPTH`] out from its track-side wall (the back, away from
    /// its street front) along the whole wall.
    pub fn canopies(&self) -> Vec<Canopy> {
        self.buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Station)
            .map(|b| {
                let (f, d) = (b.footprint, Canopy::DEPTH);
                let (min, max) = match b.front.opposite() {
                    Side::North => (Vec2::new(f.min.x, f.max.y), Vec2::new(f.max.x, f.max.y + d)),
                    Side::South => (Vec2::new(f.min.x, f.min.y - d), Vec2::new(f.max.x, f.min.y)),
                    Side::East => (Vec2::new(f.max.x, f.min.y), Vec2::new(f.max.x + d, f.max.y)),
                    Side::West => (Vec2::new(f.min.x - d, f.min.y), Vec2::new(f.min.x, f.max.y)),
                };
                Canopy {
                    footprint: Aabb::new(min, max),
                    height: Canopy::HEIGHT,
                }
            })
            .collect()
    }
}

impl TownLayout {
    /// Width of the band around the town where leveled ground blends back
    /// into the pampa, in meters.
    pub const LEVELED_BAND: f32 = 60.0;
    /// How far the track runs on past either end of the grid, in meters: the
    /// train enters and leaves the town's line out there, in the fog.
    pub const RAIL_RUN_OUT: f32 = 400.0;
    /// Half the width of [`Self::rail_corridor`], in meters.
    pub const RAIL_CORRIDOR_HALF: f32 = 4.0;
}

/// Broad track gauge, in meters.
const RAIL_GAUGE: f32 = 1.676;
/// Block size the landmark and lot proportions were drawn for, in meters.
const REFERENCE_BLOCK: f32 = 40.0;
/// How far outside the grid a rider arrives, in meters.
const ARRIVAL_DISTANCE: f32 = 25.0;

/// The street grid's geometry, derived from a validated config.
#[derive(Debug, Clone, Copy)]
struct Grid {
    origin: Vec2,
    cols: usize,
    rows: usize,
    block: f32,
    street: f32,
    /// A through line between two halves of the town.
    split: Option<Split>,
}

/// Where a through line splits the grid in two halves.
#[derive(Debug, Clone, Copy)]
struct Split {
    /// The first row of blocks north of the line.
    row: usize,
    /// Width of the band between the two halves' ring streets, in meters.
    gap: f32,
}

/// The first row of blocks north of a through line in a town of `rows`.
fn split_row(rows: usize) -> usize {
    rows / 2
}

impl Grid {
    /// The grid of `config`, after checking every number in it.
    fn validated(config: &TownConfig) -> Result<Self, TownConfigError> {
        if config.name.trim().is_empty() {
            return Err(TownConfigError::EmptyName);
        }
        let finite = [
            (
                "origin",
                config.origin.x.is_finite() && config.origin.y.is_finite(),
            ),
            ("block_size", config.block_size.is_finite()),
            ("street_width", config.street_width.is_finite()),
            (
                "railway.offset",
                config.railway.is_none_or(|r| r.offset.is_finite()),
            ),
        ];
        if let Some((field, _)) = finite.into_iter().find(|(_, ok)| !ok) {
            return Err(TownConfigError::NotFinite { field });
        }
        let (cols, rows) = config.blocks;
        let min = TownConfig::MIN_BLOCKS;
        if cols < min || rows < min {
            return Err(TownConfigError::TooFewBlocks {
                blocks: config.blocks,
                min,
            });
        }
        if config.block_size < TownConfig::MIN_BLOCK_SIZE {
            return Err(TownConfigError::BlockTooSmall {
                block_size: config.block_size,
                min: TownConfig::MIN_BLOCK_SIZE,
            });
        }
        if config.street_width < TownConfig::MIN_STREET_WIDTH {
            return Err(TownConfigError::StreetTooNarrow {
                street_width: config.street_width,
                min: TownConfig::MIN_STREET_WIDTH,
            });
        }
        if let Some(r) = config.railway
            && r.offset < TownConfig::MIN_RAILWAY_OFFSET
        {
            return Err(TownConfigError::RailwayTooClose {
                offset: r.offset,
                min: TownConfig::MIN_RAILWAY_OFFSET,
            });
        }
        if let Some(density) = config.density
            && !(0.0..=1.0).contains(&density)
        {
            return Err(TownConfigError::BadDensity { density });
        }
        let through = config.railway.filter(|r| r.through);
        if let Some(r) = through {
            if matches!(r.side, Side::East | Side::West) {
                return Err(TownConfigError::ThroughNotEastWest { side: r.side });
            }
            if rows < RailwayConfig::MIN_THROUGH_ROWS {
                return Err(TownConfigError::ThroughTooFewRows {
                    rows,
                    min: RailwayConfig::MIN_THROUGH_ROWS,
                });
            }
        }
        // Only the tier's landmark blocks are set aside: fewer police
        // stations fit, more do not.
        let max = config.size().police_stations();
        if let Some(wanted) = config.police_stations
            && wanted > max
        {
            return Err(TownConfigError::TooManyPoliceStations { wanted, max });
        }
        if let Some(wanted) = config.townsfolk
            && wanted > TownConfig::MAX_TOWNSFOLK
        {
            return Err(TownConfigError::TooManyTownsfolk {
                wanted,
                max: TownConfig::MAX_TOWNSFOLK,
            });
        }
        Ok(Self {
            origin: config.origin,
            cols,
            rows,
            block: config.block_size,
            street: config.street_width,
            split: through.map(|r| Split {
                row: split_row(rows),
                gap: r.offset + RailwayConfig::THROUGH_FAR_SIDE,
            }),
        })
    }

    /// Half the grid's size, ring street included (and, split by a through
    /// line, both halves' ring streets and the band between them).
    fn half(&self) -> Vec2 {
        let span = |n: usize| n as f32 * self.block + (n + 1) as f32 * self.street;
        let split = self.split.map_or(0.0, |s| self.street + s.gap);
        Vec2::new(span(self.cols), span(self.rows) + split) * 0.5
    }

    fn bounds(&self) -> Aabb {
        Aabb::from_center_size(self.origin, self.half() * 2.0)
    }

    /// Lower `x` of north-south street `k` (0 = the west ring street).
    fn lane_x(&self, k: usize) -> f32 {
        self.origin.x - self.half().x + k as f32 * (self.block + self.street)
    }

    /// Lower `y` of east-west street `k` (0 = the south ring street). Past
    /// a through line, `k` from its split row on are the north half's
    /// streets ([`Self::south_lane`] is the south half's last).
    fn lane_y(&self, k: usize) -> f32 {
        let shift = self
            .split
            .filter(|s| k >= s.row)
            .map_or(0.0, |s| self.street + s.gap);
        self.origin.y - self.half().y + k as f32 * (self.block + self.street) + shift
    }

    /// Lower `y` of the south half's north ring street, by a through line.
    fn south_lane(&self) -> Option<f32> {
        let s = self.split?;
        Some(self.origin.y - self.half().y + s.row as f32 * (self.block + self.street))
    }

    /// The band between the two halves of a split grid.
    fn band(&self) -> Option<Aabb> {
        let s = self.split?;
        let b = self.bounds();
        let lo = self.south_lane()? + self.street;
        Some(Aabb::new(
            Vec2::new(b.min.x, lo),
            Vec2::new(b.max.x, lo + s.gap),
        ))
    }

    /// The part of the grid the station yard is laid against, and the side
    /// of it the track runs along: the whole grid and the railway's side,
    /// or, for a through line, the half the station stands in and its side
    /// facing the track.
    fn yard_half(&self, railway: RailwayConfig) -> (Aabb, Side) {
        let b = self.bounds();
        match self.band() {
            Some(band) if railway.side == Side::North => (
                Aabb::new(Vec2::new(b.min.x, band.max.y), b.max),
                Side::South,
            ),
            Some(band) => (
                Aabb::new(b.min, Vec2::new(b.max.x, band.min.y)),
                Side::North,
            ),
            None => (b, railway.side),
        }
    }

    /// Footprint of block `(i, j)`, counted from the south-west corner.
    fn block(&self, i: usize, j: usize) -> Aabb {
        let min = Vec2::new(self.lane_x(i) + self.street, self.lane_y(j) + self.street);
        Aabb::new(min, min + Vec2::new(self.block, self.block))
    }

    /// The blocks of `plaza` merged into one area, streets between included.
    fn span(&self, plaza: PlazaBlocks) -> Aabb {
        let (lo, hi) = (plaza.min, plaza.max);
        Aabb::new(self.block(lo.0, lo.1).min, self.block(hi.0, hi.1).max)
    }

    /// Every north-south street, west to east, then every east-west street,
    /// south to north. A street running between the blocks of a merged
    /// `plaza` stops at its edges: it becomes two pieces, one on either side.
    /// A north-south street that would run into the station `yard` of a
    /// through line stops at the band on either side; the others cross it.
    fn streets(&self, plaza: PlazaBlocks, yard: &[Aabb]) -> Vec<Aabb> {
        let b = self.bounds();
        let area = self.span(plaza);
        let band = self.band();
        // `lane` from `lo` to `hi` along it, minus every one of `cuts`.
        let pieces = |lo: f32, hi: f32, cuts: &[(f32, f32)]| {
            let mut out = vec![(lo, hi)];
            for &(c0, c1) in cuts {
                out = out
                    .into_iter()
                    .flat_map(|(a, z)| {
                        if c1 <= a || c0 >= z {
                            vec![(a, z)]
                        } else {
                            vec![(a, c0), (c1, z)]
                        }
                    })
                    .filter(|(a, z)| z > a)
                    .collect();
            }
            out
        };
        let mut out = Vec::new();
        for k in 0..=self.cols {
            let x = self.lane_x(k);
            let inner = plaza.min.0 < k && k <= plaza.max.0;
            let mut cuts: Vec<(f32, f32)> = inner
                .then_some((area.min.y, area.max.y))
                .into_iter()
                .collect();
            if let Some(band) = band {
                let lane = Aabb::new(
                    Vec2::new(x, band.min.y),
                    Vec2::new(x + self.street, band.max.y),
                )
                .expanded(YARD_CLEARANCE);
                if yard.iter().any(|y| y.intersects(&lane)) {
                    cuts.push((band.min.y, band.max.y));
                }
            }
            for (y0, y1) in pieces(b.min.y, b.max.y, &cuts) {
                out.push(Aabb::new(Vec2::new(x, y0), Vec2::new(x + self.street, y1)));
            }
        }
        for k in 0..=self.rows {
            // A through line's band: the south half's ring street first.
            if let (Some(split), Some(y)) = (self.split, self.south_lane())
                && split.row == k
            {
                out.push(Aabb::new(
                    Vec2::new(b.min.x, y),
                    Vec2::new(b.max.x, y + self.street),
                ));
            }
            let y = self.lane_y(k);
            let inner = plaza.min.1 < k && k <= plaza.max.1;
            let cut = inner.then_some((area.min.x, area.max.x));
            let cuts: Vec<(f32, f32)> = cut.into_iter().collect();
            for (x0, x1) in pieces(b.min.x, b.max.x, &cuts) {
                out.push(Aabb::new(Vec2::new(x0, y), Vec2::new(x1, y + self.street)));
            }
        }
        out
    }

    /// The grid plus the station yard and railway corridor beside it (a
    /// through line's lie inside it).
    fn extent(&self, railway: Option<RailwayConfig>) -> Aabb {
        let grid = self.bounds();
        let Some(r) = railway.filter(|r| !r.through) else {
            return grid;
        };
        let along = self.along_half(r.side);
        let yard = yard_box(grid, r.side, (-along, along), (0.0, r.offset + 4.0));
        Aabb::new(
            Vec2::new(grid.min.x.min(yard.min.x), grid.min.y.min(yard.min.y)),
            Vec2::new(grid.max.x.max(yard.max.x), grid.max.y.max(yard.max.y)),
        )
    }

    /// Half the grid's length along a track on `side`.
    fn along_half(&self, side: Side) -> f32 {
        match side {
            Side::North | Side::South => self.half().x,
            Side::East | Side::West => self.half().y,
        }
    }

    /// Scale of block-relative proportions.
    fn scale(&self) -> f32 {
        self.block / REFERENCE_BLOCK
    }
}

/// Index nearest the middle of `n` blocks; ties go to the lower index.
fn middle(n: usize) -> usize {
    (n - 1) / 2
}

/// The block that holds the plaza.
///
/// With a through line ([`RailwayConfig::through`]), facing the station is
/// the row next to the track in the station's half; and `Auto` takes, in
/// that half, the block one column west of the middle and two rows back
/// from the track (the last row if there are fewer), so the main plaza
/// stands a few blocks from the station, as in the big railway towns.
fn plaza_block(config: &TownConfig) -> Result<(usize, usize), TownConfigError> {
    let (cols, rows) = config.blocks;
    let through = config.railway.filter(|r| r.through);
    let central = cols % 2 == 1 && rows % 2 == 1 && through.is_none();
    let row = split_row(rows);
    let facing = |side: Side| match (side, through.is_some()) {
        (Side::North, true) => (middle(cols), row),
        (_, true) => (middle(cols), row - 1),
        (Side::North, false) => (middle(cols), rows - 1),
        (Side::South, false) => (middle(cols), 0),
        (Side::East, false) => (cols - 1, middle(rows)),
        (Side::West, false) => (0, middle(rows)),
    };
    if let (Some(r), PlazaPlacement::Auto) = (through, config.plaza) {
        let i = middle(cols).saturating_sub(1);
        let j = match r.side {
            Side::North => (row + 2).min(rows - 1),
            _ => (row - 1).saturating_sub(2),
        };
        return Ok((i, j));
    }
    match config.plaza {
        PlazaPlacement::Center if central => Ok((cols / 2, rows / 2)),
        PlazaPlacement::Center => Err(TownConfigError::NoCentralBlock {
            blocks: config.blocks,
        }),
        PlazaPlacement::Block(i, j) if i < cols && j < rows => Ok((i, j)),
        PlazaPlacement::Block(i, j) => Err(TownConfigError::PlazaOutOfRange {
            block: (i, j),
            blocks: config.blocks,
        }),
        PlazaPlacement::FacingStation => config
            .railway
            .map(|r| facing(r.side))
            .ok_or(TownConfigError::NoStationToFace),
        PlazaPlacement::Auto if central => Ok((cols / 2, rows / 2)),
        PlazaPlacement::Auto => Ok(config
            .railway
            .map_or((middle(cols), middle(rows)), |r| facing(r.side))),
    }
}

/// The blocks of the main plaza: columns `min.0..=max.0`, rows
/// `min.1..=max.1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlazaBlocks {
    min: (usize, usize),
    max: (usize, usize),
}

impl PlazaBlocks {
    fn contains(&self, (i, j): (usize, usize)) -> bool {
        (self.min.0..=self.max.0).contains(&i) && (self.min.1..=self.max.1).contains(&j)
    }

    /// Blocks `(east-west, north-south)`.
    fn span(&self) -> (usize, usize) {
        (self.max.0 - self.min.0 + 1, self.max.1 - self.min.1 + 1)
    }

    fn blocks(&self) -> impl Iterator<Item = (usize, usize)> {
        let (lo, hi) = (self.min, self.max);
        (lo.1..=hi.1).flat_map(move |j| (lo.0..=hi.0).map(move |i| (i, j)))
    }

    /// The blocks just past the plaza's `side` (west to east, or south to
    /// north), possibly outside the grid.
    fn beside(&self, side: Side) -> Vec<(isize, isize)> {
        let (lo, hi) = (
            (self.min.0 as isize, self.min.1 as isize),
            (self.max.0 as isize, self.max.1 as isize),
        );
        match side {
            Side::West => (lo.1..=hi.1).map(|j| (lo.0 - 1, j)).collect(),
            Side::East => (lo.1..=hi.1).map(|j| (hi.0 + 1, j)).collect(),
            Side::South => (lo.0..=hi.0).map(|i| (i, lo.1 - 1)).collect(),
            Side::North => (lo.0..=hi.0).map(|i| (i, hi.1 + 1)).collect(),
        }
    }

    /// Blocks from `b` to the plaza along each axis: positive when the
    /// plaza lies east (north), zero when `b` is level with it.
    fn offset(&self, (i, j): (usize, usize)) -> (isize, isize) {
        let toward = |v: usize, lo: usize, hi: usize| {
            if v < lo {
                (lo - v) as isize
            } else if v > hi {
                -((v - hi) as isize)
            } else {
                0
            }
        };
        (
            toward(i, self.min.0, self.max.0),
            toward(j, self.min.1, self.max.1),
        )
    }

    /// The side of a block at `offset` (from [`Self::offset`]) that faces
    /// the plaza, along the axis with the larger offset.
    fn facing((di, dj): (isize, isize)) -> Side {
        if di.abs() >= dj.abs() {
            if di > 0 { Side::East } else { Side::West }
        } else if dj > 0 {
            Side::North
        } else {
            Side::South
        }
    }
}

/// The main plaza's blocks. A small town's plaza is the block
/// [`PlazaPlacement`] picks. A bigger town's spans several
/// ([`TownSize::plaza_span`]): centered on the grid for `Auto` and `Center`
/// (ties go west or south), otherwise covering the picked block, as close
/// to the center as it allows.
fn main_plaza(config: &TownConfig, size: TownSize) -> Result<PlazaBlocks, TownConfigError> {
    let anchor = plaza_block(config)?;
    let (w, h) = size.plaza_span(config.blocks);
    let through = config.railway.is_some_and(|r| r.through);
    let centered =
        matches!(config.plaza, PlazaPlacement::Auto | PlazaPlacement::Center) && !through;
    let start = |n: usize, len: usize, a: usize| {
        let mid = (n - len) / 2;
        if size == TownSize::Small {
            a
        } else if centered {
            mid
        } else {
            mid.clamp(a.saturating_sub(len - 1), a.min(n - len))
        }
    };
    let min = (
        start(config.blocks.0, w, anchor.0),
        start(config.blocks.1, h, anchor.1),
    );
    let plaza = PlazaBlocks {
        min,
        max: (min.0 + w - 1, min.1 + h - 1),
    };
    let row = split_row(config.blocks.1);
    if through && plaza.min.1 < row && plaza.max.1 >= row {
        return Err(TownConfigError::PlazaAcrossRailway);
    }
    Ok(plaza)
}

/// A landmark building facing the plaza from a nearby block.
#[derive(Debug, Clone, Copy)]
struct Landmark {
    kind: BuildingKind,
    block: (usize, usize),
    /// The block side it stands on (its facade side).
    front: Side,
}

/// Where each landmark prefers to stand, seen from the plaza, in order.
/// A central plaza gets the store to the west, the church to the north and
/// the police station to the east.
const LANDMARK_PREFERENCES: [(BuildingKind, [Side; 4]); 3] = [
    (
        BuildingKind::GeneralStore,
        [Side::West, Side::East, Side::South, Side::North],
    ),
    (
        BuildingKind::Church,
        [Side::North, Side::South, Side::East, Side::West],
    ),
    (
        BuildingKind::PoliceStation,
        [Side::East, Side::West, Side::South, Side::North],
    ),
];

/// Places the town's landmarks, spread over the town so it has life all
/// over (user, 2026-10-23), and a bigger town's station plaza. Returns them
/// as the main general store, the church and the first police station
/// (unless `police_count` is zero), then the rest: a bigger town's station
/// store, the other police stations and any extra stores.
///
/// The church faces the main plaza from the first free neighbouring block
/// of its preferences (see [`LANDMARK_PREFERENCES`]). A bigger town's station
/// plaza and its store come next (see [`station_plaza`] and
/// [`station_store`]). Every other landmark is dispersed ([`disperse`]).
fn place_landmarks(
    grid: &Grid,
    plaza: PlazaBlocks,
    size: TownSize,
    police_count: usize,
    railway: Option<RailwayConfig>,
) -> (Vec<Landmark>, Option<StationPlaza>) {
    let [store, church, police] = LANDMARK_PREFERENCES;
    let none = |_: (usize, usize)| false;
    let church = place_landmark(grid, plaza, church.0, &church.1, &none);
    let mut placed = vec![church];
    let station = (size != TownSize::Small).then(|| station_plaza(grid, plaza, &placed, railway));
    let mut rest = Vec::new();
    if let Some(station) = &station {
        let s = station_store(grid, plaza, station, &placed);
        placed.push(s);
        rest.push(s);
    }
    let reserved: Vec<(usize, usize)> = station.iter().map(|s| s.block).collect();
    let main_store = disperse(grid, plaza, store.0, &placed, &reserved);
    placed.push(main_store);
    let mut first = vec![main_store, church];
    let extra_stores = size.general_stores() - 1 - usize::from(size != TownSize::Small);
    let kinds = std::iter::repeat_n(police.0, police_count)
        .chain(std::iter::repeat_n(store.0, extra_stores));
    for (k, kind) in kinds.enumerate() {
        let landmark = disperse(grid, plaza, kind, &placed, &reserved);
        placed.push(landmark);
        if k == 0 && kind == BuildingKind::PoliceStation {
            first.push(landmark);
        } else {
            rest.push(landmark);
        }
    }
    first.extend(rest);
    (first, station)
}

/// Blocks between two civic buildings, at least: never side by side.
const CIVIC_SPACING: usize = 2;

/// Least blocks between two landmarks of the same kind in a town of
/// `cols` x `rows`: about half the town apart.
fn same_kind_spacing(cols: usize, rows: usize) -> usize {
    CIVIC_SPACING.max(cols.max(rows) / 2)
}

/// Blocks apart, the most along either axis.
fn blocks_apart(a: (usize, usize), b: (usize, usize)) -> usize {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

/// A `kind` landmark dispersed over the town: on a block that is not the
/// plaza, `reserved` (the station plaza's) nor `placed`, at least
/// [`CIVIC_SPACING`] blocks from every `placed` landmark and
/// [`same_kind_spacing`] from those of its kind, as near the plaza as that
/// allows (fewest blocks along either axis, then the lowest column, then
/// row). In a town split by a through
/// line, it goes to the half with fewer of its kind (the plaza's on a tie),
/// so each half has its own. When no block keeps the spacing, it takes the
/// one farthest from the others. It faces the plaza along the axis with the
/// larger offset.
fn disperse(
    grid: &Grid,
    plaza: PlazaBlocks,
    kind: BuildingKind,
    placed: &[Landmark],
    reserved: &[(usize, usize)],
) -> Landmark {
    let half = |b: (usize, usize)| grid.split.map(|s| b.1 >= s.row);
    let wanted = grid.split.map(|_| {
        let count = |north: bool| {
            placed
                .iter()
                .filter(|l| l.kind == kind && half(l.block) == Some(north))
                .count()
        };
        let plaza_north = half(plaza.min) == Some(true);
        match count(true).cmp(&count(false)) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => plaza_north,
        }
    });
    let same = same_kind_spacing(grid.cols, grid.rows);
    let free: Vec<(usize, usize)> = (0..grid.cols)
        .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
        .filter(|&b| !plaza.contains(b) && !reserved.contains(&b))
        .filter(|&b| placed.iter().all(|l| l.block != b))
        .filter(|&b| wanted.is_none() || half(b) == wanted)
        .collect();
    // Blocks to the plaza, the most along either axis: a block across a
    // corner counts as near as one beside it, so the landmarks can still
    // fan out round a small plaza.
    let to_plaza = |b: (usize, usize)| {
        let (di, dj) = plaza.offset(b);
        di.unsigned_abs().max(dj.unsigned_abs())
    };
    let nearest_civic = |b: (usize, usize)| {
        placed
            .iter()
            .map(|l| blocks_apart(l.block, b))
            .min()
            .unwrap_or(usize::MAX)
    };
    let spaced = |b: (usize, usize)| {
        placed.iter().all(|l| {
            let apart = blocks_apart(l.block, b);
            apart >= CIVIC_SPACING && (l.kind != kind || apart >= same)
        })
    };
    let block = free
        .iter()
        .copied()
        .filter(|&b| spaced(b))
        .min_by_key(|&b| (to_plaza(b), b.0, b.1))
        .or_else(|| {
            free.iter()
                .copied()
                .min_by_key(|&b| (std::cmp::Reverse(nearest_civic(b)), to_plaza(b), b.0, b.1))
        })
        .expect("a town has a free block for every landmark");
    Landmark {
        kind,
        block,
        front: PlazaBlocks::facing(plaza.offset(block)),
    }
}

/// A `kind` landmark facing `plaza`: the first block next to it, in
/// `preferences` order (west to east, or south to north, along a side),
/// that is not the plaza nor `taken`, facing it across the street;
/// otherwise the nearest such block (fewest blocks away, then lowest column,
/// then lowest row), facing the plaza along the axis with the larger offset.
fn place_landmark(
    grid: &Grid,
    plaza: PlazaBlocks,
    kind: BuildingKind,
    preferences: &[Side],
    taken: &dyn Fn((usize, usize)) -> bool,
) -> Landmark {
    let inside = |i: isize, j: isize| {
        (0..grid.cols as isize).contains(&i) && (0..grid.rows as isize).contains(&j)
    };
    let free = |b: (usize, usize)| !plaza.contains(b) && !taken(b);
    let neighbor = preferences.iter().find_map(|&toward| {
        plaza.beside(toward).into_iter().find_map(|(i, j)| {
            let block = (i as usize, j as usize);
            (inside(i, j) && free(block)).then_some(Landmark {
                kind,
                block,
                front: toward.opposite(),
            })
        })
    });
    neighbor.unwrap_or_else(|| {
        let mut blocks: Vec<(usize, usize)> = (0..grid.cols)
            .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
            .filter(|&b| free(b))
            .collect();
        blocks.sort_by_key(|&b| {
            let (di, dj) = plaza.offset(b);
            (di.unsigned_abs() + dj.unsigned_abs(), b.0, b.1)
        });
        let block = blocks[0];
        let front = PlazaBlocks::facing(plaza.offset(block));
        Landmark { kind, block, front }
    })
}

/// Blocks with a windmill and its water tank in the back yard: the
/// south-west corner, the east column one row below the top, and the second
/// column of the top row, skipping `reserved` blocks (plazas and landmarks).
fn windmill_blocks(grid: &Grid, reserved: &[(usize, usize)]) -> Vec<(usize, usize)> {
    [(0, 0), (grid.cols - 1, grid.rows - 2), (1, grid.rows - 1)]
        .into_iter()
        .filter(|b| !reserved.contains(b))
        .collect()
}

/// A bigger town's station plaza: half of `block`, on its `toward` side.
#[derive(Debug, Clone, Copy)]
struct StationPlaza {
    block: (usize, usize),
    toward: Side,
    area: Aabb,
    /// Tree ring steps along x and y: half as many across the half block.
    tree_steps: (usize, usize),
}

/// Picks a bigger town's station plaza.
///
/// With a railway: the block of the railway-side row facing the station
/// (which is aligned to the main plaza; its west or south block when it
/// spans two), or, when that block holds the main plaza or a landmark, the
/// nearest free block along that row (ties go to the lower index); the
/// plaza takes the half of it nearest the railway. Without a railway (or
/// with no free block on that row): the free block farthest from the main
/// plaza (ties go to the lowest column, then row); the plaza takes the half
/// facing the main plaza.
fn station_plaza(
    grid: &Grid,
    plaza: PlazaBlocks,
    landmarks: &[Landmark],
    railway: Option<RailwayConfig>,
) -> StationPlaza {
    let free = |b: (usize, usize)| !plaza.contains(b) && landmarks.iter().all(|l| l.block != b);
    let on_row = railway.and_then(|r| {
        let (cols, rows) = (grid.cols, grid.rows);
        // A through line's station stands in the middle of the town, in
        // the row of its half next to the track.
        let split = grid.split.map(|s| s.row);
        let (row, toward): (Vec<(usize, usize)>, Side) = match (r.side, split) {
            (Side::North, Some(s)) => ((0..cols).map(|i| (i, s)).collect(), Side::South),
            (_, Some(s)) => ((0..cols).map(|i| (i, s - 1)).collect(), Side::North),
            (Side::North, None) => ((0..cols).map(|i| (i, rows - 1)).collect(), r.side),
            (Side::South, None) => ((0..cols).map(|i| (i, 0)).collect(), r.side),
            (Side::East, None) => ((0..rows).map(|j| (cols - 1, j)).collect(), r.side),
            (Side::West, None) => ((0..rows).map(|j| (0, j)).collect(), r.side),
        };
        // Index along the row of the block facing the station.
        let along = |b: (usize, usize)| match r.side {
            Side::North | Side::South => b.0,
            Side::East | Side::West => b.1,
        };
        let facing = if split.is_some() {
            middle(cols)
        } else {
            along(plaza.min)
        };
        row.into_iter()
            .filter(|&b| free(b))
            .min_by_key(|&b| (along(b).abs_diff(facing), along(b)))
            .map(|b| (b, toward))
    });
    let (block, toward) = on_row.unwrap_or_else(|| {
        let block = (0..grid.cols)
            .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
            .filter(|&b| free(b))
            .min_by_key(|&b| {
                let (di, dj) = plaza.offset(b);
                let d = di.unsigned_abs() + dj.unsigned_abs();
                (std::cmp::Reverse(d), b.0, b.1)
            })
            .expect("a bigger town has free blocks");
        (block, PlazaBlocks::facing(plaza.offset(block)))
    });
    let b = grid.block(block.0, block.1);
    let area = side_box(b, toward, 0.0, grid.block, grid.block / 2.0);
    let tree_steps = match toward {
        Side::North | Side::South => (4, 2),
        Side::East | Side::West => (2, 4),
    };
    StationPlaza {
        block,
        toward,
        area,
        tree_steps,
    }
}

/// The station plaza's general store: on a free block beside it, facing it
/// across the street, never on the side of its house half.
fn station_store(
    grid: &Grid,
    plaza: PlazaBlocks,
    station: &StationPlaza,
    landmarks: &[Landmark],
) -> Landmark {
    let preferences: Vec<Side> = [Side::West, Side::East, Side::South, Side::North]
        .into_iter()
        .filter(|&s| s != station.toward.opposite())
        .collect();
    let taken = |b: (usize, usize)| plaza.contains(b) || landmarks.iter().any(|l| l.block == b);
    let single = PlazaBlocks {
        min: station.block,
        max: station.block,
    };
    place_landmark(
        grid,
        single,
        BuildingKind::GeneralStore,
        &preferences,
        &taken,
    )
}

/// Box against the `side` edge of block `b`, from `a0` to `a1` along that
/// edge (west to east, or south to north) and `depth` deep into the block.
fn side_box(b: Aabb, side: Side, a0: f32, a1: f32, depth: f32) -> Aabb {
    match side {
        Side::South => Aabb::new(
            Vec2::new(b.min.x + a0, b.min.y),
            Vec2::new(b.min.x + a1, b.min.y + depth),
        ),
        Side::North => Aabb::new(
            Vec2::new(b.min.x + a0, b.max.y - depth),
            Vec2::new(b.min.x + a1, b.max.y),
        ),
        Side::West => Aabb::new(
            Vec2::new(b.min.x, b.min.y + a0),
            Vec2::new(b.min.x + depth, b.min.y + a1),
        ),
        Side::East => Aabb::new(
            Vec2::new(b.max.x - depth, b.min.y + a0),
            Vec2::new(b.max.x, b.min.y + a1),
        ),
    }
}

fn building(kind: BuildingKind, footprint: Aabb, height: f32, front: Side) -> Building {
    Building {
        kind,
        footprint,
        height,
        front,
    }
}

/// Footprint proportions of a landmark on a reference 40 m block: start and
/// end along its street edge, depth into the block, and wall height.
fn landmark_shape(kind: BuildingKind) -> (f32, f32, f32, f32) {
    match kind {
        BuildingKind::GeneralStore => (12.0, 28.0, 14.0, 5.0),
        BuildingKind::PoliceStation => (13.0, 27.0, 12.0, 4.5),
        // Church: deep nave behind a narrow front.
        _ => (14.0, 26.0, 22.0, 7.0),
    }
}

/// Small deterministic hash of lot coordinates into `[0, 1)`.
fn lot_noise(i: usize, j: usize, lot: usize, salt: u32) -> f32 {
    let mut v = (i as u32).wrapping_mul(73_856_093)
        ^ (j as u32).wrapping_mul(19_349_663)
        ^ (lot as u32).wrapping_mul(83_492_791)
        ^ salt.wrapping_mul(2_654_435_761);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v & 0xffff) as f32 / 65_536.0
}

/// Lines a block's street edges with houses (sheds on the outskirts) and adds
/// its landmark or back-yard windmill, if any. Every proportion is drawn for
/// a 40 m block and scales with the block. When a second plaza takes half
/// the block, the other half gets every lot that stays clear of it.
fn fill_block(
    grid: &Grid,
    (i, j): (usize, usize),
    landmark: Option<&Landmark>,
    windmill: bool,
    plaza: Option<Aabb>,
    config: &TownConfig,
    out: &mut Vec<Building>,
) {
    let seed = config.seed;
    let b = grid.block(i, j);
    let s = grid.scale();
    if let Some(l) = landmark {
        let (a0, a1, depth, height) = landmark_shape(l.kind);
        out.push(building(
            l.kind,
            side_box(b, l.front, a0 * s, a1 * s, depth * s),
            height,
            l.front,
        ));
    }
    let taken = landmark.map(|l| l.front);
    let outskirts = i == 0 || j == 0 || i == grid.cols - 1 || j == grid.rows - 1;
    let density = match (plaza, outskirts) {
        (Some(_), _) => 1.0,
        (None, true) => 0.45,
        (None, false) => 0.75,
    } * config.density.unwrap_or(1.0);
    let noise = |lot: usize, salt: u32| lot_noise(i, j, lot, salt ^ seed.wrapping_mul(0x9e37_79b9));

    // Lots: (street side, start along the street, lot length). North and south
    // sides hold three lots; east and west one in the middle, clear of corners.
    let lots = [
        (Side::South, 1.0, 10.0),
        (Side::South, 15.0, 10.0),
        (Side::South, 29.0, 10.0),
        (Side::North, 1.0, 10.0),
        (Side::North, 15.0, 10.0),
        (Side::North, 29.0, 10.0),
        (Side::West, 14.0, 12.0),
        (Side::East, 14.0, 12.0),
    ];
    for (n, &(side, start, length)) in lots.iter().enumerate() {
        if Some(side) == taken || noise(n, 1) > density {
            continue;
        }
        let width = length - 2.0 * noise(n, 2);
        let depth = 8.0 + 3.0 * noise(n, 3);
        let a = start + (length - width) / 2.0;
        let footprint = side_box(b, side, a * s, (a + width) * s, depth * s);
        if plaza.is_some_and(|p| p.intersects(&footprint)) {
            continue;
        }
        let shed = outskirts && noise(n, 4) < 0.25;
        let (kind, height) = if shed {
            (BuildingKind::Shed, 4.0)
        } else {
            (BuildingKind::House, 3.6 + 0.8 * noise(n, 5))
        };
        out.push(building(kind, footprint, height, side));
    }

    if windmill {
        // Tower and tank centered in the back yard.
        let c = b.center();
        let tower = Aabb::new(
            Vec2::new(c.x - 5.5, c.y - 1.5),
            Vec2::new(c.x - 2.5, c.y + 1.5),
        );
        out.extend(windmill_with_tank(tower, Vec2::new(1.0, 0.0)));
    }
}

/// A 3 m windmill tower on `tower` and its round 7 m tank 1 m past it
/// along `toward`.
fn windmill_with_tank(tower: Aabb, toward: Vec2) -> [Building; 2] {
    let reach = 1.5 + 1.0 + 3.5;
    let tank_center = tower.center() + toward * reach;
    [
        building(BuildingKind::Windmill, tower, 9.0, Side::South),
        building(
            BuildingKind::WaterTank,
            Aabb::from_center_size(tank_center, Vec2::new(7.0, 7.0)),
            1.5,
            Side::South,
        ),
    ]
}

/// Box beside the grid on `side`: `u` along the grid edge (from the grid's
/// center line, west to east or south to north) and `v` outward from the
/// grid edge.
fn yard_box(grid: Aabb, side: Side, u: (f32, f32), v: (f32, f32)) -> Aabb {
    let c = grid.center();
    let (min, max) = match side {
        Side::North => (
            Vec2::new(c.x + u.0, grid.max.y + v.0),
            Vec2::new(c.x + u.1, grid.max.y + v.1),
        ),
        Side::South => (
            Vec2::new(c.x + u.0, grid.min.y - v.1),
            Vec2::new(c.x + u.1, grid.min.y - v.0),
        ),
        Side::East => (
            Vec2::new(grid.max.x + v.0, c.y + u.0),
            Vec2::new(grid.max.x + v.1, c.y + u.1),
        ),
        Side::West => (
            Vec2::new(grid.min.x - v.1, c.y + u.0),
            Vec2::new(grid.min.x - v.0, c.y + u.1),
        ),
    };
    Aabb::new(min, max)
}

/// Half the platform's length, in meters: longer than the train.
const PLATFORM_HALF: f32 = 30.0;

/// Station, platform, grain stores, a shed and a windmill between the ring
/// street and the track, anchored to the track. The station is centered on
/// the plaza along the track (kept far enough from the grid's ends for the
/// whole platform) and faces it across the ring street; grain stores line
/// the track on one side, the shed and windmill on the other, as far as the
/// grid reaches.
///
/// `bounds` is the part of the grid the yard is laid against (the whole
/// grid, or a through line's station half), `side` the side of it the track
/// runs along `offset` meters out, `half` half its length along the track,
/// and `wanted` where along the track (from its middle) the station should
/// stand.
fn station_yard(bounds: Aabb, side: Side, r: f32, half: f32, wanted: f32) -> Vec<Building> {
    let along = axis_along(side);
    let c = wanted.clamp(-half + PLATFORM_HALF, half - PLATFORM_HALF);
    let reach = half - 2.0;
    let yard =
        |u0: f32, u1: f32, v0: f32, v1: f32| yard_box(bounds, side, (c + u0, c + u1), (v0, v1));
    let (to_grid, to_track) = (side.opposite(), side);

    let mut out = vec![
        building(
            BuildingKind::Station,
            yard(-12.0, 12.0, r - 20.0, r - 11.0),
            5.0,
            to_grid,
        ),
        building(
            BuildingKind::Platform,
            yard(
                -PLATFORM_HALF,
                PLATFORM_HALF,
                r - 11.0,
                r - Floor::PLATFORM_EDGE,
            ),
            Floor::PLATFORM_HEIGHT,
            to_track,
        ),
    ];
    // Grain stores past the platform, toward +u.
    let mut start = PLATFORM_HALF + 10.0;
    for length in [40.0, 35.0] {
        let end = (start + length).min(reach - c);
        if end - start < 20.0 {
            break;
        }
        out.push(building(
            BuildingKind::GrainStore,
            yard(start, end, r - 20.0, r - 8.0),
            6.0,
            to_track,
        ));
        start = end + 10.0;
    }
    // A windmill and a shed before the platform, toward -u.
    if c - 45.0 >= -reach {
        let tower = yard(-45.0, -42.0, r - 17.0, r - 14.0);
        out.extend(windmill_with_tank(tower, along));
    }
    if c - 75.0 >= -reach {
        out.push(building(
            BuildingKind::Shed,
            yard(-75.0, -50.0, r - 20.0, r - 10.0),
            4.5,
            to_grid,
        ));
    }
    out
}

/// Unit world axis along a track on `side` of a grid.
fn axis_along(side: Side) -> Vec2 {
    match side {
        Side::North | Side::South => Vec2::new(1.0, 0.0),
        Side::East | Side::West => Vec2::new(0.0, 1.0),
    }
}

/// How far a north-south street keeps from the station yard to cross a
/// through line's band, in meters.
const YARD_CLEARANCE: f32 = 2.0;

/// The track: parallel to the `grid`'s `side` at `offset`, running
/// [`TownLayout::RAIL_RUN_OUT`] past either end of the grid.
fn railway_line(grid: Aabb, side: Side, offset: f32, along_half: f32) -> Railway {
    // A zero-size box on the track centerline.
    let line = yard_box(grid, side, (0.0, 0.0), (offset, offset));
    let reach = along_half + TownLayout::RAIL_RUN_OUT;
    let c = grid.center();
    let (start, end) = match side {
        Side::North | Side::South => (
            Vec2::new(c.x - reach, line.min.y),
            Vec2::new(c.x + reach, line.min.y),
        ),
        Side::East | Side::West => (
            Vec2::new(line.min.x, c.y - reach),
            Vec2::new(line.min.x, c.y + reach),
        ),
    };
    Railway {
        start,
        end,
        gauge: RAIL_GAUGE,
    }
}

/// Depth of the general store's veranda: 3 m on wide streets, shallower on
/// narrow ones so the tied horse stays clear of the walkers down the middle.
fn veranda_depth(street: f32) -> f32 {
    (street / 2.0 - 2.7).clamp(1.2, Sidewalk::WIDTH)
}

/// A rail on the street side of `store`, past its `veranda`, with a horse
/// tied alongside it facing along the street.
fn hitching_post(store: &Building, veranda: f32) -> HitchingPost {
    let (rail, horse) = tied_post(store, veranda + 0.2, 0.0, 2.8);
    HitchingPost {
        rail,
        horse,
        veranda,
    }
}

/// A rail of `length` along the street `rail_offset` out of `building`'s
/// facade and `lateral` meters off its middle (positive turns left of the
/// outward normal), with a horse tied alongside facing along the street.
fn tied_post(
    building: &Building,
    rail_offset: f32,
    lateral: f32,
    length: f32,
) -> (Aabb, TetheredHorse) {
    // Outward facade normal and the direction along the street.
    let normal = crate::places::side_normal(building.front);
    let along = Vec2::new(normal.y.abs(), normal.x.abs());
    let half = building.footprint.size() * 0.5;
    let face = building.footprint.center()
        + normal * (normal.x.abs() * half.x + normal.y.abs() * half.y)
        + Vec2::new(-normal.y, normal.x) * lateral;
    // Axis-aligned box of `length` along the street and `width` across it.
    let boxed = |center: Vec2, length: f32, width: f32| {
        let size = along * length + normal * width;
        Aabb::from_center_size(center, Vec2::new(size.x.abs(), size.y.abs()))
    };
    // Just past the veranda.
    let (rail_width, horse_width, horse_length) = (0.2, 1.0, 2.4);
    let rail = boxed(face + normal * rail_offset, length, rail_width);
    let horse_center = face + normal * (rail_offset + rail_width / 2.0 + 0.1 + horse_width / 2.0);
    (
        rail,
        TetheredHorse {
            position: horse_center,
            heading: Heading::from_radians(along.y.atan2(along.x)),
            footprint: boxed(horse_center, horse_length, horse_width),
        },
    )
}

impl TownLayout {
    /// The palenques of the town: one before each general store but the
    /// first (whose post is [`Self::hitching_post`]), police station, church
    /// and station that has room for one, in building order. A city ties a
    /// horse to a seeded half of them, at most [`Palenque::MAX_HORSES`].
    fn place_palenques(&self) -> Vec<Palenque> {
        let first_store = self
            .buildings
            .iter()
            .position(|b| b.kind == BuildingKind::GeneralStore);
        // Ground a palenque keeps off, with the gap each one needs.
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        let mut solid: Vec<Aabb> = self
            .buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(Building::collider)
            .chain(self.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)))
            .chain([self.hitching_post.rail, self.hitching_post.horse.footprint])
            .chain(self.sign_obstacles())
            .chain(
                self.plaza_furniture()
                    .iter()
                    .flat_map(PlazaFurniture::obstacles)
                    .collect::<Vec<_>>(),
            )
            .collect();
        let mut open: Vec<Aabb> = self.sidewalks.iter().map(|s| s.area).collect();
        open.extend(self.plazas());
        for floor in self.floors() {
            open.push(floor.footprint);
            open.extend(floor.ramps());
        }
        open.extend(self.canopies().iter().map(|c| c.footprint));
        open.extend(self.rail_corridor());
        // The corner lamps stand in the street crossings.
        open.extend(self.crossings(Palenque::CROSSING_MARGIN));
        // The ground in front of every door, as the lamps keep it clear.
        open.extend(self.door_zones(0.0));
        // Past the sidewalk, and past the store's veranda if the street is wide.
        let offset = (self.hitching_post.veranda + 0.2).max(Sidewalk::WIDTH + 0.2);
        let mut posts: Vec<Palenque> = Vec::new();
        for (k, b) in self.buildings.iter().enumerate() {
            let wanted = matches!(
                b.kind,
                BuildingKind::GeneralStore
                    | BuildingKind::PoliceStation
                    | BuildingKind::Church
                    | BuildingKind::Station
            );
            if !wanted || Some(k) == first_store {
                continue;
            }
            // Beside the door, on the seeded side first, then further out.
            let first = if Rng::new(self.seed ^ 0x9a1e, k as u32).below(2) == 0 {
                1.0
            } else {
                -1.0
            };
            let laterals: Vec<f32> = (0..4)
                .flat_map(|step| {
                    let d = DOOR_SIDE + 1.5 * step as f32;
                    [first * d, -first * d]
                })
                .collect();
            // A city looks for room for a horse first; a rail alone is
            // enough anywhere else, or when the street is too narrow.
            let passes: &[bool] = if self.size == TownSize::Small {
                &[false]
            } else {
                &[true, false]
            };
            'found: for &with_horse in passes {
                for &lateral in &laterals {
                    // The station's post stands on its yard, with no sidewalk to clear.
                    let out = if b.kind == BuildingKind::Station {
                        Palenque::YARD_OUT
                    } else {
                        offset
                    };
                    let (rail, horse) = tied_post(b, out, lateral, Palenque::LENGTH);
                    let spot = if with_horse {
                        Aabb::new(
                            Vec2::new(
                                rail.min.x.min(horse.footprint.min.x),
                                rail.min.y.min(horse.footprint.min.y),
                            ),
                            Vec2::new(
                                rail.max.x.max(horse.footprint.max.x),
                                rail.max.y.max(horse.footprint.max.y),
                            ),
                        )
                    } else {
                        rail
                    };
                    if self.palenque_fits(b.kind, &spot, &solid, &open) {
                        solid.push(spot);
                        posts.push(Palenque {
                            building: k,
                            rail,
                            horse: with_horse.then_some(horse),
                        });
                        break 'found;
                    }
                }
            }
        }
        // A city ties a horse to a seeded half of its posts (the ones with
        // the lowest rolls that have room for one).
        let mut order: Vec<usize> = (0..posts.len())
            .filter(|&k| posts[k].horse.is_some())
            .collect();
        order.sort_by_key(|&k| Rng::new(self.seed ^ 0x7e11, k as u32).next());
        order.truncate(Palenque::horses_for(posts.len()));
        for (k, post) in posts.iter_mut().enumerate() {
            if !order.contains(&k) {
                post.horse = None;
            }
        }
        posts
    }

    /// Whether a palenque of a `kind` of building, filling `spot`, fits: on
    /// one street and off its middle lane (the station's, on the yard in
    /// front of it), `Palenque::CLEARANCE` from every `solid` obstacle and
    /// clear of every `open` area.
    fn palenque_fits(
        &self,
        kind: BuildingKind,
        spot: &Aabb,
        solid: &[Aabb],
        open: &[Aabb],
    ) -> bool {
        let on_ground = if kind == BuildingKind::Station {
            self.extent.contains(spot)
        } else {
            self.on_street_off_lane(spot, Palenque::LANE)
        };
        // The station stands in a through line's band; nothing else does.
        let off_band =
            kind == BuildingKind::Station || self.band.is_none_or(|b| !b.intersects(spot));
        on_ground
            && off_band
            && solid
                .iter()
                .all(|o| !o.intersects(&spot.expanded(Palenque::CLEARANCE)))
            && open.iter().all(|o| !o.intersects(spot))
    }
}

impl TownLayout {
    /// Whether `spot` lies inside one street strip and at least `lane` meters
    /// off its middle line, on one side of it.
    fn on_street_off_lane(&self, spot: &Aabb, lane: f32) -> bool {
        self.streets.iter().any(|street| {
            let across = if street.size().x > street.size().y {
                Vec2::new(0.0, 1.0)
            } else {
                Vec2::new(1.0, 0.0)
            };
            let middle = street.center().dot(across);
            let (lo, hi) = (spot.min.dot(across) - middle, spot.max.dot(across) - middle);
            street.contains(spot) && lo * hi > 0.0 && lo.abs().min(hi.abs()) >= lane
        })
    }

    /// The street crossings, where a north-south street meets an east-west
    /// one, each grown by `margin` meters.
    fn crossings(&self, margin: f32) -> Vec<Aabb> {
        let (north_south, east_west): (Vec<&Aabb>, Vec<&Aabb>) =
            self.streets.iter().partition(|s| s.size().y > s.size().x);
        let mut out = Vec::new();
        for x in &north_south {
            for y in &east_west {
                if x.intersects(y) {
                    let crossing =
                        Aabb::new(Vec2::new(x.min.x, y.min.y), Vec2::new(x.max.x, y.max.y));
                    out.push(crossing.expanded(margin));
                }
            }
        }
        out
    }

    /// The ground in front of every door (but the platform's), as the lamps
    /// keep it clear, each strip `extra` meters wider on both sides.
    fn door_zones(&self, extra: f32) -> Vec<Aabb> {
        let reach = StreetLamps::DOOR_HALF_WIDTH + extra;
        self.buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(|b| {
                let n = crate::places::side_normal(b.front);
                let side = Vec2::new(-n.y, n.x);
                let half = b.footprint.size() * 0.5;
                let facade = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                let (a, z) = (
                    facade + side * reach,
                    facade + n * StreetLamps::DOOR_REACH - side * reach,
                );
                Aabb::new(
                    Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                    Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                )
            })
            .collect()
    }
}

/// What the trees of [`TownLayout::place_shade_trees`] keep off, and where
/// they stand so far.
struct ShadeGround {
    /// Obstacles a trunk keeps [`ShadeGround::CLEARANCE`] from.
    solid: Vec<Aabb>,
    /// Ground no trunk stands on: crossings, doors, galerías, the line.
    zones: Vec<Aabb>,
    /// The ground in front of every door, wider: no street tree stands on it.
    pit_doors: Vec<Aabb>,
    /// The ring round every plaza and its sidewalk: no trunk but a grove's.
    plazas: Vec<Aabb>,
    /// Every building footprint (the platform's excluded).
    buildings: Vec<Aabb>,
    /// Every trunk placed so far, the plaza rings' included.
    taken: Vec<Vec2>,
}

impl ShadeGround {
    /// Gap between a trunk and any obstacle, in meters.
    const CLEARANCE: f32 = 1.0;
    /// Gap kept from a crossing, where the corner lamps stand, meters.
    const CROSSING: f32 = 3.5;
    /// Extra width kept beside a door, meters.
    const DOOR: f32 = 1.8;
    /// Extra width kept beside a door by a street tree, whose crown is
    /// wider, meters.
    const PIT_DOOR: f32 = 2.4;
    /// Depth of a store's galería zone, meters: the walk and its veranda.
    const GALLERY: f32 = 3.0;
    /// Extra width kept beside a store's galería, meters.
    const GALLERY_SIDE: f32 = 2.5;
    /// Gap kept between a yard plant and any obstacle, meters.
    const PLANT_GAP: f32 = 0.4;

    fn new(town: &TownLayout) -> Self {
        let mut zones = town.crossings(Self::CROSSING);
        zones.extend(town.door_zones(Self::DOOR));
        for b in town
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::GeneralStore)
        {
            let n = crate::places::side_normal(b.front);
            let side = Vec2::new(-n.y, n.x);
            let half = b.footprint.size() * 0.5;
            let facade = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
            let wide = side.x.abs() * half.x + side.y.abs() * half.y + Self::GALLERY_SIDE;
            let (a, z) = (
                facade + side * wide,
                facade + n * Self::GALLERY - side * wide,
            );
            zones.push(Aabb::new(
                Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                Vec2::new(a.x.max(z.x), a.y.max(z.y)),
            ));
        }
        for floor in town.floors() {
            zones.push(floor.footprint.expanded(Self::CLEARANCE));
            zones.extend(floor.ramps().map(|r| r.expanded(Self::CLEARANCE)));
        }
        zones.extend(
            town.canopies()
                .iter()
                .map(|c| c.footprint.expanded(Self::CLEARANCE)),
        );
        zones.extend(town.rail_corridor().map(|c| c.expanded(Self::CLEARANCE)));
        zones.extend(town.rail_band().map(|b| b.expanded(Self::CLEARANCE)));
        Self {
            solid: town.obstacles(),
            zones,
            pit_doors: town.door_zones(Self::PIT_DOOR),
            plazas: town
                .plazas()
                .map(|p| p.expanded(Sidewalk::WIDTH + 0.3))
                .collect(),
            buildings: town
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect(),
            taken: town.trees.clone(),
        }
    }

    /// Whether a trunk can stand at `p`: clear of the obstacles and the
    /// zones (and the plazas unless `in_plaza`), and [`ShadeTree::APART`]
    /// from every other trunk.
    fn fits(&self, p: Vec2, in_plaza: bool) -> bool {
        let trunk = ShadeTree::trunk(p);
        let spot = trunk.expanded(Self::CLEARANCE);
        self.solid.iter().all(|o| !o.intersects(&spot))
            && self.zones.iter().all(|z| !z.intersects(&trunk))
            && (in_plaza || self.plazas.iter().all(|z| !z.intersects(&trunk)))
            && self.taken.iter().all(|t| t.distance(p) >= ShadeTree::APART)
    }

    /// The distance from `p` to the nearest building.
    fn building_gap(&self, p: Vec2) -> f32 {
        self.buildings
            .iter()
            .map(|b| b.closest_point(p).distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    fn plant(&mut self, out: &mut Vec<ShadeTree>, tree: ShadeTree) {
        self.taken.push(tree.at);
        self.solid.push(tree.obstacle());
        out.push(tree);
    }
}

impl TownLayout {
    /// The trees outside the plaza rings, deterministic from the town's seed:
    ///
    /// - a city's sidewalk trees in their cazuelas, a trimmed crown every 8
    ///   to 10 m, 0.4 m in from the curb, off the doors, galerías, crossings,
    ///   palenques and signs ([`ShadeKind::Pit`]);
    /// - groves between the paths of a city's plazas ([`ShadeKind::Grove`]);
    /// - a city's yard trees and the ones by its church, comisarías and
    ///   station, and a town's sparse ones behind its houses
    ///   ([`ShadeKind::Yard`], never within a crown's reach of a building);
    /// - a town's sparse trees on the dirt in front of some houses
    ///   ([`ShadeKind::Front`]).
    ///
    /// Every trunk keeps [`ShadeTree::APART`] from the others.
    fn place_shade_trees(&self, grid: &Grid, main: PlazaBlocks) -> Vec<ShadeTree> {
        let city = self.size != TownSize::Small;
        let mut ground = ShadeGround::new(self);
        let mut out: Vec<ShadeTree> = Vec::new();
        let trunk = ShadeTree::trunk;

        // Sidewalk trees, along each strip's curb.
        if city {
            let inset = ShadeTree::PIT_FROM_CURB + ShadeTree::PIT_SIDE / 2.0;
            for (row, walk) in self.sidewalks.iter().enumerate() {
                let Some(&curb) = walk.curbs.first() else {
                    continue;
                };
                let out_of = crate::places::side_normal(curb);
                let a = walk.area;
                let level = out_of.x.abs() < 0.5;
                let (lo, hi) = if level {
                    (a.min.x, a.max.x)
                } else {
                    (a.min.y, a.max.y)
                };
                let edge = match curb {
                    Side::North => a.max.y,
                    Side::South => a.min.y,
                    Side::East => a.max.x,
                    Side::West => a.min.x,
                };
                let at = |t: f32| {
                    if level {
                        Vec2::new(t, edge - out_of.y * inset)
                    } else {
                        Vec2::new(edge - out_of.x * inset, t)
                    }
                };
                let mut rng = Rng::new(self.seed ^ 0x57ee, row as u32);
                let (near, far) = ShadeTree::SPACING;
                let mut t = lo + rng.range(0.0, far);
                let mut last = f32::NEG_INFINITY;
                while t < hi {
                    // The seeded spot, or one a little further or nearer (never
                    // nearer than the least spacing to the last tree).
                    let found = [0.0, 1.5, -1.5, 3.0, -3.0]
                        .iter()
                        .map(|d| t + d)
                        .find(|&t| {
                            (lo..=hi).contains(&t)
                                && t - last >= near
                                && ground.fits(at(t), false)
                                && ground
                                    .pit_doors
                                    .iter()
                                    .all(|d| !d.intersects(&ShadeTree::trunk(at(t))))
                        });
                    if let Some(found) = found {
                        t = found;
                        last = t;
                        let tree = ShadeTree {
                            at: at(t),
                            kind: ShadeKind::Pit,
                            crown: ShadeTree::PIT_CROWN,
                            toward: out_of,
                            row: row as u32,
                        };
                        ground.plant(&mut out, tree);
                    }
                    t += rng.range(near, far);
                }
            }
        }

        // Groves in the green between a city's plaza paths.
        if city {
            let reach = TREE_TRUNK_RADIUS + PLAZA_PATH_CLEARANCE + 0.5;
            for (k, plaza) in self.plazas().enumerate() {
                let keep_off: Vec<Aabb> = plaza_paths(plaza)
                    .into_iter()
                    .chain([plaza_monument(plaza), plaza_crossing(plaza)])
                    .collect();
                let bed = plaza.expanded(-ShadeTree::GROVE_INSET);
                let mut rng = Rng::new(self.seed ^ 0x6a0e, k as u32);
                let cols = (bed.size().x / ShadeTree::GROVE_STEP).floor() as usize + 1;
                let rows = (bed.size().y / ShadeTree::GROVE_STEP).floor() as usize + 1;
                for j in 0..rows {
                    for i in 0..cols {
                        let jitter = Vec2::new(
                            rng.range(-ShadeTree::GROVE_JITTER, ShadeTree::GROVE_JITTER),
                            rng.range(-ShadeTree::GROVE_JITTER, ShadeTree::GROVE_JITTER),
                        );
                        let p = bed.min
                            + Vec2::new(i as f32, j as f32) * ShadeTree::GROVE_STEP
                            + jitter;
                        let clear = keep_off
                            .iter()
                            .all(|a| a.closest_point(p).distance(p) >= reach);
                        if bed.contains_point(p) && clear && ground.fits(p, true) {
                            let tree = ShadeTree {
                                at: p,
                                kind: ShadeKind::Grove,
                                crown: ShadeTree::GROVE_CROWN,
                                toward: Vec2::ZERO,
                                row: 0,
                            };
                            ground.plant(&mut out, tree);
                        }
                    }
                }
            }
        }

        // A yard tree at the first of a few seeded spots in `area` that is
        // a crown's reach from every building and off the streets.
        let yard = |ground: &mut ShadeGround,
                    out: &mut Vec<ShadeTree>,
                    area: Aabb,
                    stream: u32,
                    tries: u32| {
            let mut rng = Rng::new(self.seed ^ 0x7a2d, stream);
            for _ in 0..tries {
                let p = Vec2::new(
                    rng.range(area.min.x, area.max.x),
                    rng.range(area.min.y, area.max.y),
                );
                let off_streets = self
                    .streets
                    .iter()
                    .all(|s| !s.intersects(&trunk(p).expanded(0.5)));
                let reach = ShadeTree::YARD_CROWN + ShadeTree::BUILDING_GAP;
                if off_streets && ground.building_gap(p) >= reach && ground.fits(p, false) {
                    let tree = ShadeTree {
                        at: p,
                        kind: ShadeKind::Yard,
                        crown: ShadeTree::YARD_CROWN,
                        toward: Vec2::ZERO,
                        row: 0,
                    };
                    ground.plant(out, tree);
                    return;
                }
            }
        };

        // By the church, the comisarías and the station of a city.
        if city {
            for (k, b) in self.buildings.iter().enumerate() {
                let trees = match b.kind {
                    BuildingKind::Church | BuildingKind::Station => 2,
                    BuildingKind::PoliceStation => 1,
                    _ => continue,
                };
                let around = b.footprint.expanded(ShadeTree::LANDMARK_REACH);
                for n in 0..trees {
                    yard(&mut ground, &mut out, around, 0x1000 + 4 * k as u32 + n, 24);
                }
            }
        }

        // Behind the houses: every block's yard, a city's with a few trees,
        // a town's sparse.
        for (i, j) in (0..grid.rows).flat_map(|j| (0..grid.cols).map(move |i| (i, j))) {
            let block = grid.block(i, j);
            if main.contains((i, j)) || self.plazas().any(|p| p.intersects(&block)) {
                continue;
            }
            let stream = (j * grid.cols + i) as u32;
            let mut rng = Rng::new(self.seed ^ 0x3d1a, stream);
            // 30% more than the first planting, then 15% more (user,
            // 2026-10-27): a city's blocks get up to three trees, a fourth in
            // seven of ten and a fifth in two of ten; a town's nine in ten one.
            let wanted = if city {
                1 + rng.below(3)
                    + usize::from(rng.below(100) < 70)
                    + usize::from(rng.below(100) < 40)
            } else {
                usize::from(rng.below(100) < 92)
            };
            let area = block.expanded(-ShadeTree::YARD_INSET);
            for n in 0..wanted {
                yard(
                    &mut ground,
                    &mut out,
                    area,
                    0x4000 + 4 * stream + n as u32,
                    8,
                );
            }
        }

        // On the dirt in front of some houses of a town.
        if !city {
            for (k, b) in self
                .buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == BuildingKind::House)
            {
                let mut rng = Rng::new(self.seed ^ 0xf20e, k as u32);
                if rng.below(100) >= 63 {
                    continue;
                }
                let n = crate::places::side_normal(b.front);
                let side = Vec2::new(-n.y, n.x);
                let half = b.footprint.size() * 0.5;
                let facade = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                let wide = side.x.abs() * half.x + side.y.abs() * half.y;
                let first = if rng.below(2) == 0 { 1.0 } else { -1.0 };
                let lateral = rng.range(2.6, wide + 1.5);
                let spot = [first, -first].iter().find_map(|d| {
                    let p = facade + n * ShadeTree::FRONT_OUT + side * (d * lateral);
                    let lane = Palenque::LANE + TREE_TRUNK_RADIUS;
                    let on_dirt = self.on_street_off_lane(&trunk(p), lane);
                    // Never a row: with this one, no tree has more than two
                    // others within FRONT_RUN.
                    let fronts = || out.iter().filter(|t| t.kind == ShadeKind::Front);
                    let within = |t: &ShadeTree, q: Vec2| t.at.distance(q) < ShadeTree::FRONT_RUN;
                    let sparse = fronts().all(|t| t.at.distance(p) >= ShadeTree::FRONT_APART)
                        && fronts().filter(|t| within(t, p)).count() < 2
                        && fronts()
                            .filter(|t| within(t, p))
                            .all(|t| fronts().filter(|u| within(u, t.at)).count() <= 2);
                    (on_dirt && sparse && ground.fits(p, false)).then_some(p)
                });
                if let Some(p) = spot {
                    let tree = ShadeTree {
                        at: p,
                        kind: ShadeKind::Front,
                        crown: ShadeTree::FRONT_CROWN,
                        toward: n,
                        row: 0,
                    };
                    ground.plant(&mut out, tree);
                }
            }
        }
        // General Pico's plaza was too leafy (user, 2026-10-26, twice): a very
        // big town loses a fifth of its plaza trees and then a fifth again of
        // what was left (36 in 100 in all), taken from the groves by seeded
        // rolls so the rings along the edges stay.
        if self.size == TownSize::VeryBig {
            let groves: Vec<usize> = (0..out.len())
                .filter(|&k| out[k].kind == ShadeKind::Grove)
                .collect();
            let cut = (self.trees.len() + groves.len()) * 36 / 100;
            let mut order = groves;
            order.sort_by_key(|&k| Rng::new(self.seed ^ 0x71a5, k as u32).next());
            let gone: Vec<usize> = order.into_iter().take(cut).collect();
            let mut k = 0;
            out.retain(|_| {
                k += 1;
                !gone.contains(&(k - 1))
            });
        }
        out
    }
}

impl TownLayout {
    /// The shrubs and flower beds of the yards: behind every house (a shed
    /// has none), against its back and side walls, seeded. Each stands clear
    /// of every obstacle (the trunks too), the door zones and galerías, the
    /// streets and plazas, and every other plant.
    fn place_yard_plants(&self) -> Vec<YardPlant> {
        let ground = ShadeGround::new(self);
        let mut out: Vec<YardPlant> = Vec::new();
        for (k, b) in self
            .buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == BuildingKind::House)
        {
            let n = crate::places::side_normal(b.front);
            let side = Vec2::new(-n.y, n.x);
            let half = b.footprint.size() * 0.5;
            let deep = n.x.abs() * half.x + n.y.abs() * half.y;
            let wide = side.x.abs() * half.x + side.y.abs() * half.y;
            let mut rng = Rng::new(self.seed ^ 0x91a7, k as u32);
            let mut wanted = Vec::new();
            if rng.below(100) < 40 {
                wanted.push(PlantKind::Shrub);
            }
            if rng.below(100) < 15 {
                wanted.push(PlantKind::Shrub);
            }
            if rng.below(100) < 25 {
                wanted.push(PlantKind::Flowers);
            }
            for kind in wanted {
                for _ in 0..8 {
                    // Behind the house, or beside it toward the back.
                    let along = rng.range(-wide, wide);
                    let out_of = rng.range(1.0, 2.6);
                    let p = b.footprint.center() - n * (deep + out_of) + side * along;
                    let plant = YardPlant { at: p, kind };
                    let spot = plant.area();
                    let free = ground
                        .solid
                        .iter()
                        .all(|o| !o.intersects(&spot.expanded(ShadeGround::PLANT_GAP)))
                        && ground.zones.iter().all(|z| !z.intersects(&spot))
                        && ground.plazas.iter().all(|z| !z.intersects(&spot))
                        && self
                            .streets
                            .iter()
                            .all(|s| !s.intersects(&spot.expanded(0.5)))
                        && out.iter().all(|q| q.at.distance(p) >= 1.6);
                    if free {
                        out.push(plant);
                        break;
                    }
                }
            }
        }
        out
    }
}

/// How far from the middle of a facade the nearest palenque's middle stands:
/// past the door's keep-out and half the rail, in meters.
const DOOR_SIDE: f32 = StreetLamps::DOOR_HALF_WIDTH + 0.3 + Palenque::LENGTH / 2.0;

/// Trees in a ring just inside the plaza edge, leaving the center open:
/// `steps.0` gaps along the x edges and `steps.1` along the y edges. Only
/// the green beds are planted: a ring spot on a path, or within
/// [`PLAZA_PATH_CLEARANCE`] of one or of the monument, stays empty.
fn plaza_trees(plaza: Aabb, steps: (usize, usize)) -> Vec<Vec2> {
    let reach = TREE_TRUNK_RADIUS + PLAZA_PATH_CLEARANCE;
    let keep_off: Vec<Aabb> = plaza_paths(plaza)
        .into_iter()
        .chain([plaza_monument(plaza)])
        .collect();
    let mut trees = plaza_ring(plaza, steps);
    trees.retain(|&t| {
        keep_off
            .iter()
            .all(|a| a.closest_point(t).distance(t) >= reach)
    });
    trees
}

/// A plaza ring's step count with a few more spots (user, 2026-10-27:
/// "subile otro 15% a pueblos y aldeas"); the steps are whole, so the ring
/// grows less than the rest.
fn more_steps(steps: usize) -> usize {
    (steps as f32 * PLAZA_RING_MORE).round() as usize
}

/// How many times the steps of the first planting a plaza ring takes.
const PLAZA_RING_MORE: f32 = 1.08;

/// Spots in a ring just inside the plaza edge (see [`plaza_trees`]).
fn plaza_ring(plaza: Aabb, steps: (usize, usize)) -> Vec<Vec2> {
    let inset = 3.0;
    let (lo, hi) = (
        plaza.min + Vec2::new(inset, inset),
        plaza.max - Vec2::new(inset, inset),
    );
    let mut trees = Vec::new();
    for k in 0..=steps.0.max(steps.1) {
        if k <= steps.0 {
            let x = lo.x + (hi.x - lo.x) * (k as f32 / steps.0 as f32);
            trees.push(Vec2::new(x, lo.y));
            trees.push(Vec2::new(x, hi.y));
        }
        if k != 0 && k < steps.1 {
            let y = lo.y + (hi.y - lo.y) * (k as f32 / steps.1 as f32);
            trees.push(Vec2::new(lo.x, y));
            trees.push(Vec2::new(hi.x, y));
        }
    }
    trees
}

/// The main road in: the north-south street just east of the plaza, entered
/// from the south (from the north when the railway runs south of town, or
/// when a through line's station stands in the north half).
fn arrival(grid: &Grid, plaza: PlazaBlocks, railway: Option<RailwayConfig>) -> (Vec2, Heading) {
    let bounds = grid.bounds();
    let road_x = grid.lane_x(plaza.max.0 + 1) + grid.street / 2.0;
    if railway.is_some_and(|r| (r.side == Side::South) != r.through) {
        (
            Vec2::new(road_x, bounds.max.y + ARRIVAL_DISTANCE),
            Heading::from_radians(-std::f32::consts::FRAC_PI_2),
        )
    } else {
        (
            Vec2::new(road_x, bounds.min.y - ARRIVAL_DISTANCE),
            Heading::from_radians(std::f32::consts::FRAC_PI_2),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Rider;

    /// Configs every invariant is checked against: square and oblong grids,
    /// odd and even block counts, blocks of 32–50 m, streets of 8–14 m, the
    /// railway on every side or missing, every plaza placement, and big and
    /// very big towns.
    pub(crate) fn sample_configs() -> Vec<TownConfig> {
        let base = TownConfig::castex();
        let rail = |side, offset| {
            Some(RailwayConfig {
                side,
                offset,
                through: false,
            })
        };
        vec![
            base.clone(),
            five_by_five(),
            TownConfig {
                name: "Tres".into(),
                blocks: (3, 3),
                block_size: 32.0,
                street_width: 12.0,
                seed: 1,
                ..base.clone()
            },
            TownConfig {
                name: "Seis por cuatro".into(),
                origin: Vec2::new(-20.0, 15.0),
                blocks: (6, 4),
                block_size: 50.0,
                street_width: 16.0,
                railway: rail(Side::South, 30.0),
                seed: 99,
                ..base.clone()
            },
            TownConfig {
                name: "Este".into(),
                blocks: (4, 4),
                block_size: 36.0,
                street_width: 12.0,
                railway: rail(Side::East, 26.0),
                ..base.clone()
            },
            TownConfig {
                name: "Oeste".into(),
                blocks: (3, 5),
                block_size: 45.0,
                street_width: 13.0,
                railway: rail(Side::West, 24.0),
                plaza: PlazaPlacement::Block(0, 0),
                ..base.clone()
            },
            TownConfig {
                name: "Sin tren".into(),
                blocks: (5, 3),
                railway: None,
                ..base.clone()
            },
            TownConfig {
                name: "Sin tren par".into(),
                blocks: (4, 6),
                block_size: 34.0,
                railway: None,
                ..base.clone()
            },
            TownConfig {
                name: "Frente".into(),
                blocks: (5, 5),
                block_size: 38.0,
                street_width: 12.0,
                plaza: PlazaPlacement::FacingStation,
                ..base.clone()
            },
            // Big towns (37 to 63 blocks): a two-block plaza and a station
            // plaza.
            TownConfig {
                name: "Siete".into(),
                blocks: (7, 7),
                block_size: 36.0,
                street_width: 12.0,
                railway: rail(Side::East, 26.0),
                seed: 5,
                ..base.clone()
            },
            TownConfig {
                name: "Larga".into(),
                blocks: (10, 4),
                block_size: 34.0,
                street_width: 12.0,
                railway: rail(Side::South, 24.0),
                seed: 21,
                ..base.clone()
            },
            TownConfig {
                name: "Alta".into(),
                blocks: (4, 10),
                block_size: 36.0,
                street_width: 12.0,
                railway: rail(Side::West, 24.0),
                seed: 8,
                ..base.clone()
            },
            TownConfig {
                name: "Grande sin tren".into(),
                blocks: (7, 6),
                railway: None,
                seed: 3,
                ..base.clone()
            },
            // Very big towns (64 blocks or more): a 2 x 2 plaza.
            TownConfig {
                name: "Muy grande".into(),
                blocks: (8, 8),
                seed: 11,
                ..base.clone()
            },
            TownConfig {
                name: "Muy grande sin tren".into(),
                blocks: (9, 8),
                block_size: 36.0,
                railway: None,
                seed: 4,
                ..base.clone()
            },
            // Towns grown on both sides of a line through their middle.
            TownConfig {
                name: "Partida".into(),
                blocks: (8, 8),
                railway: Some(RailwayConfig {
                    side: Side::North,
                    offset: 24.0,
                    through: true,
                }),
                seed: 13,
                ..base.clone()
            },
            TownConfig {
                name: "Partida sur".into(),
                blocks: (5, 4),
                block_size: 34.0,
                street_width: 12.0,
                railway: Some(RailwayConfig {
                    side: Side::South,
                    offset: 28.0,
                    through: true,
                }),
                seed: 21,
                density: Some(0.5),
                ..base
            },
        ]
    }

    /// A 5 x 5 grid of 40 m blocks and 12 m streets with a central plaza.
    fn five_by_five() -> TownConfig {
        TownConfig {
            name: "Cinco por cinco".into(),
            blocks: (5, 5),
            street_width: 14.0,
            ..TownConfig::castex()
        }
    }

    /// Every sample town, generated.
    pub(crate) fn sample_towns() -> Vec<TownLayout> {
        sample_configs()
            .iter()
            .map(|c| TownLayout::generate(c).unwrap_or_else(|e| panic!("{}: {e}", c.name)))
            .collect()
    }

    fn count(t: &TownLayout, kind: BuildingKind) -> usize {
        t.buildings.iter().filter(|b| b.kind == kind).count()
    }

    fn find(t: &TownLayout, kind: BuildingKind) -> Option<&Building> {
        t.buildings.iter().find(|b| b.kind == kind)
    }

    /// Unit vector pointing out of `side`.
    fn normal(side: Side) -> Vec2 {
        crate::places::side_normal(side)
    }

    #[test]
    fn generation_is_deterministic_and_the_seed_varies_lots() {
        for c in sample_configs() {
            assert_eq!(TownLayout::generate(&c), TownLayout::generate(&c));
            let other = TownConfig {
                seed: c.seed.wrapping_add(1),
                ..c.clone()
            };
            let (a, b) = (
                TownLayout::generate(&c).unwrap(),
                TownLayout::generate(&other).unwrap(),
            );
            assert_ne!(a.buildings, b.buildings, "{}", c.name);
            assert_eq!((a.streets, a.plaza), (b.streets, b.plaza));
        }
        assert_eq!(
            TownLayout::castex_inspired(),
            TownLayout::generate(&TownConfig::castex()).unwrap()
        );
    }

    #[test]
    fn has_the_institutions_of_a_railway_town() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let railway = usize::from(c.railway.is_some());
            let (stores, police) = match c.size() {
                TownSize::Small => (1, 1),
                TownSize::Big => (2, 1),
                TownSize::VeryBig => (2, 2),
            };
            assert_eq!(count(&t, BuildingKind::GeneralStore), stores, "{}", c.name);
            assert_eq!(count(&t, BuildingKind::PoliceStation), police, "{}", c.name);
            assert_eq!(count(&t, BuildingKind::Church), 1, "{}", c.name);
            for kind in [BuildingKind::Station, BuildingKind::Platform] {
                assert_eq!(count(&t, kind), railway, "{}: {kind:?}", c.name);
            }
            let blocks = c.blocks.0 * c.blocks.1;
            assert!(count(&t, BuildingKind::House) >= blocks, "{}", c.name);
            assert!(count(&t, BuildingKind::Windmill) >= 1, "{}", c.name);
            assert_eq!(
                count(&t, BuildingKind::Windmill),
                count(&t, BuildingKind::WaterTank)
            );
            assert!(t.trees.len() >= 8);
            assert_eq!(t.name, c.name);
        }
        let castex = TownLayout::castex_inspired();
        assert!(count(&castex, BuildingKind::House) >= 30);
        assert!(count(&castex, BuildingKind::Shed) + count(&castex, BuildingKind::GrainStore) >= 3);
    }

    #[test]
    fn the_grid_follows_the_config() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let (cols, rows) = c.blocks;
            let (b, w) = (c.block_size, c.street_width);
            // A street between the blocks of a merged plaza is cut in two.
            let (pw, ph) = t.size.plaza_span(c.blocks);
            let cut = (pw - 1) + (ph - 1);
            // A line through the town adds a ring street on each side of its
            // band, and cuts the streets that would run into its yard.
            let (split, band) = match c.railway.filter(|r| r.through) {
                Some(r) => (
                    1 + (cols + 1 - t.crossing_streets().len()),
                    w + r.offset + RailwayConfig::THROUGH_FAR_SIDE,
                ),
                None => (0, 0.0),
            };
            assert_eq!(
                t.streets.len(),
                cols + 1 + rows + 1 + cut + split,
                "{}",
                c.name
            );
            let size = t.grid().size();
            let expected = Vec2::new(
                cols as f32 * b + (cols + 1) as f32 * w,
                rows as f32 * b + (rows + 1) as f32 * w + band,
            );
            assert!(size.distance(expected) < 1e-3, "{}: {size:?}", c.name);
            assert!(t.grid().center().distance(c.origin) < 1e-3, "{}", c.name);
            // Whole blocks, with the streets between them.
            let merged = |n: usize| n as f32 * b + (n - 1) as f32 * w;
            let plaza = Vec2::new(merged(pw), merged(ph));
            assert!(t.plaza.size().distance(plaza) < 1e-3, "{}", c.name);
            for street in &t.streets {
                let size = street.size();
                assert!(
                    (size.x - w).abs() < 1e-4 || (size.y - w).abs() < 1e-4,
                    "{street:?}"
                );
                assert!(t.grid().contains(street));
            }
            // The plaza is bordered by streets on all four sides.
            for side in [Side::North, Side::South, Side::East, Side::West] {
                let n = normal(side);
                let reach = t.plaza.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
                let beside = t.plaza.center() + n * (reach + w / 2.0);
                assert!(t.streets.iter().any(|s| s.contains_point(beside)));
            }
        }
    }

    #[test]
    fn the_plaza_takes_the_central_block_when_there_is_one() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let central = c.blocks.0 % 2 == 1 && c.blocks.1 % 2 == 1;
            if c.plaza == PlazaPlacement::Auto && central {
                assert!(t.plaza.contains_point(t.grid().center()), "{}", c.name);
            }
        }
    }

    /// Checks the plaza sits next to the railway, nearest the grid center
    /// along it, with the station aligned to face it across the street.
    fn assert_plaza_faces_the_station(c: &TownConfig, t: &TownLayout) {
        let side = c.railway.unwrap().side;
        let n = normal(side);
        let along = Vec2::new(n.y.abs(), n.x.abs());
        let grid = t.grid();
        // The plaza block borders the ring street on the railway side.
        let edge =
            |b: Aabb| b.center().dot(n) + b.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
        assert!(
            (edge(grid) - edge(t.plaza) - c.street_width).abs() < 1e-3,
            "{}: plaza not on the railway row",
            c.name
        );
        // Nearest the center along the track (ties go to the lower index).
        let offset = (t.plaza.center() - grid.center()).dot(along);
        let pitch = c.block_size + c.street_width;
        assert!(offset.abs() <= pitch / 2.0 + 1e-3, "{}: {offset}", c.name);
        assert!(offset <= 1e-3, "{}: ties go to the lower index", c.name);
        // The station faces the plaza, centered on it, across the street.
        let station = find(t, BuildingKind::Station).expect("a station");
        assert_eq!(station.front, side.opposite(), "{}", c.name);
        let off_axis = (station.footprint.center() - t.plaza.center()).dot(along);
        assert!(
            off_axis.abs() < 1e-3,
            "{}: station off by {off_axis}",
            c.name
        );
        assert!((station.footprint.center() - t.plaza.center()).dot(n) > 0.0);
    }

    #[test]
    fn without_a_central_block_the_plaza_faces_the_station() {
        let mut checked = 0;
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let central = c.blocks.0 % 2 == 1 && c.blocks.1 % 2 == 1;
            let facing = match c.plaza {
                _ if through(c) => false,
                PlazaPlacement::FacingStation => true,
                PlazaPlacement::Auto => {
                    !central && c.railway.is_some() && c.size() == TownSize::Small
                }
                _ => false,
            };
            if facing {
                assert_plaza_faces_the_station(c, &t);
                checked += 1;
            }
        }
        assert!(checked >= 3, "{checked} facing-station towns");
    }

    #[test]
    fn the_station_is_aligned_with_a_central_plaza_too() {
        let t = TownLayout::generate(&five_by_five()).unwrap();
        let station = find(&t, BuildingKind::Station).unwrap();
        assert!((station.footprint.center().x - t.plaza.center().x).abs() < 1e-3);
    }

    #[test]
    fn castex_is_four_by_five_with_the_plaza_facing_the_station() {
        let c = TownConfig::castex();
        assert_eq!(c.name, "Eduardo Castex");
        assert_eq!(
            (c.blocks, c.block_size, c.street_width),
            ((4, 5), 40.0, 12.0)
        );
        assert_eq!(c.plaza, PlazaPlacement::Auto);
        assert_eq!(
            c.railway,
            Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            })
        );
        let t = TownLayout::castex_inspired();
        assert_plaza_faces_the_station(&c, &t);
        // The church stands across the street south of the plaza, facing it.
        let church = find(&t, BuildingKind::Church).unwrap();
        assert_eq!(church.front, Side::North);
        assert!(church.footprint.max.y < t.plaza.min.y);
    }

    #[test]
    fn an_explicit_plaza_block_is_honored() {
        let c = TownConfig {
            plaza: PlazaPlacement::Block(1, 3),
            ..TownConfig::castex()
        };
        let t = TownLayout::generate(&c).unwrap();
        let grid = t.grid();
        let pitch = c.block_size + c.street_width;
        let expected =
            grid.min + Vec2::new(c.street_width, c.street_width) + Vec2::new(pitch, 3.0 * pitch);
        assert!(t.plaza.min.distance(expected) < 1e-3, "{:?}", t.plaza);
        // No railway and no central block: the block nearest the center.
        let even = TownConfig {
            blocks: (4, 6),
            railway: None,
            ..TownConfig::castex()
        };
        let t = TownLayout::generate(&even).unwrap();
        let pitch = even.block_size + even.street_width;
        let grid = t.grid();
        let expected = grid.min
            + Vec2::new(even.street_width, even.street_width)
            + Vec2::new(pitch, 2.0 * pitch);
        assert!(
            t.plaza.min.distance(expected) < 1e-3,
            "nearest center, lower index"
        );
    }

    #[test]
    fn landmarks_face_the_plaza_from_neighboring_blocks() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            for kind in [
                BuildingKind::GeneralStore,
                BuildingKind::PoliceStation,
                BuildingKind::Church,
            ] {
                let b = find(&t, kind).unwrap();
                // Its facade looks toward the plaza.
                let to_plaza = t.plaza.center() - b.footprint.center();
                assert!(to_plaza.dot(normal(b.front)) > 0.0, "{}: {kind:?}", c.name);
                // And a street lies just in front of it.
                let half = b.footprint.size() * 0.5;
                let n = normal(b.front);
                let ahead = b.footprint.center()
                    + n * (n.x.abs() * half.x + n.y.abs() * half.y + c.street_width / 2.0);
                assert!(
                    t.streets.iter().any(|s| s.contains_point(ahead)),
                    "{}: {kind:?}",
                    c.name
                );
            }
        }
        // Around a central plaza: the church north of it, facing it; the
        // store and the police station spread away from it and each other.
        let t = TownLayout::generate(&five_by_five()).unwrap();
        assert_eq!(find(&t, BuildingKind::Church).unwrap().front, Side::South);
    }

    /// The civic buildings of `t`: stores, police stations and the church.
    fn civic(t: &TownLayout) -> Vec<&Building> {
        t.buildings
            .iter()
            .filter(|b| {
                matches!(
                    b.kind,
                    BuildingKind::GeneralStore | BuildingKind::PoliceStation | BuildingKind::Church
                )
            })
            .collect()
    }

    #[test]
    fn civic_buildings_are_spread_over_every_town() {
        // User, 2026-10-23: never side by side, the same kind about half
        // the town apart, so the whole town has life.
        let mut configs = small_configs();
        configs.extend(
            sample_configs()
                .into_iter()
                .filter(|c| c.size() != TownSize::Small),
        );
        for c in configs {
            let t = TownLayout::generate(&c).unwrap();
            let pitch = c.block_size + c.street_width;
            let same = (same_kind_spacing(c.blocks.0, c.blocks.1) - 1) as f32 * pitch;
            let buildings = civic(&t);
            for (k, a) in buildings.iter().enumerate() {
                for b in &buildings[k + 1..] {
                    let d = a.footprint.center() - b.footprint.center();
                    let apart = d.x.abs().max(d.y.abs());
                    // Across at least one whole block, not one street.
                    assert!(apart > pitch, "{}: {a:?} by {b:?}", c.name);
                    if a.kind == b.kind {
                        assert!(apart > same, "{}: two {:?} {apart} m apart", c.name, a.kind);
                    }
                }
            }
        }
        // The dispersal is no accident of the samples: a small town keeps
        // its store, comisaría and church apart too.
        let t = TownLayout::castex_inspired();
        assert_eq!(civic(&t).len(), 3);
    }

    #[test]
    fn a_town_split_by_its_line_has_a_store_and_a_comisaria_on_each_side() {
        let (_, t) = partida();
        let y = t.railway.unwrap().start.y;
        for kind in [BuildingKind::GeneralStore, BuildingKind::PoliceStation] {
            let sides: Vec<bool> = all(&t, kind)
                .iter()
                .map(|b| b.footprint.center().y > y)
                .collect();
            assert_eq!(sides.len(), 2, "{kind:?}");
            assert_ne!(sides[0], sides[1], "{kind:?} both on one side");
        }
        // The church faces the plaza, in the station's half.
        let church = find(&t, BuildingKind::Church).unwrap();
        assert!(church.footprint.center().y > y);
    }

    #[test]
    fn landmarks_scale_with_the_block() {
        let small = TownLayout::generate(&TownConfig {
            block_size: 32.0,
            ..TownConfig::castex()
        })
        .unwrap();
        let big = TownLayout::generate(&TownConfig {
            block_size: 50.0,
            ..TownConfig::castex()
        })
        .unwrap();
        for kind in [BuildingKind::GeneralStore, BuildingKind::Church] {
            let (a, b) = (find(&small, kind).unwrap(), find(&big, kind).unwrap());
            let ratio = b.footprint.size().x / a.footprint.size().x;
            assert!((ratio - 50.0 / 32.0).abs() < 1e-3, "{kind:?} {ratio}");
        }
    }

    #[test]
    fn buildings_never_overlap_streets_plaza_or_each_other() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            for (i, a) in t.buildings.iter().enumerate() {
                for plaza in t.plazas() {
                    assert!(
                        !a.footprint.intersects(&plaza),
                        "{}: {a:?} in plaza {plaza:?}",
                        c.name
                    );
                }
                for street in &t.streets {
                    assert!(
                        !a.footprint.intersects(street),
                        "{}: {a:?} on {street:?}",
                        c.name
                    );
                }
                for b in &t.buildings[i + 1..] {
                    assert!(
                        !a.footprint.intersects(&b.footprint),
                        "{}: {a:?} vs {b:?}",
                        c.name
                    );
                }
                assert!(a.height > 0.0 && a.footprint.size().x > 0.0 && a.footprint.size().y > 0.0);
            }
        }
    }

    #[test]
    fn trees_stand_inside_the_plaza() {
        for t in sample_towns() {
            let inside = |p: Vec2| {
                t.plazas()
                    .any(|plaza| plaza.expanded(-TREE_TRUNK_RADIUS).contains_point(p))
            };
            assert!(t.trees.iter().all(|&p| inside(p)), "{}", t.name);
        }
    }

    #[test]
    fn plaza_trees_stand_in_the_beds_clear_of_the_paths_and_the_monument() {
        let mut checked = 0;
        for t in sample_towns() {
            for &tree in &t.trees {
                let plaza = t
                    .plazas()
                    .find(|p| p.contains_point(tree))
                    .expect("every tree is in a plaza");
                let reach = TREE_TRUNK_RADIUS + PLAZA_PATH_CLEARANCE;
                for keep_off in plaza_paths(plaza)
                    .into_iter()
                    .chain([plaza_monument(plaza)])
                {
                    let gap = keep_off.closest_point(tree).distance(tree);
                    assert!(
                        gap >= reach - 1e-4,
                        "{}: tree {tree:?} {gap} m off a path",
                        t.name
                    );
                }
                checked += 1;
            }
            // Still a shady plaza.
            assert!(t.trees.len() >= 8, "{}", t.name);
        }
        assert!(checked > 100, "{checked}");
    }

    #[test]
    fn railway_runs_past_the_town_edge_by_the_station() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let Some(config) = c.railway else {
                assert_eq!(t.railway, None);
                continue;
            };
            if config.through {
                // Through the middle instead: see its own test.
                continue;
            }
            let rail = t.railway.expect("a railway");
            let n = normal(config.side);
            let grid = t.grid();
            let along = Vec2::new(n.y.abs(), n.x.abs());
            // A straight line parallel to the grid side, running past both
            // ends of the grid by the run-out.
            assert!((rail.end - rail.start).dot(n).abs() < 1e-4);
            let edge =
                grid.center().dot(n) + grid.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
            assert!(
                (rail.start.dot(n) - edge - config.offset).abs() < 1e-3,
                "{}",
                c.name
            );
            let (lo, hi) = (
                grid.min.dot(along) - TownLayout::RAIL_RUN_OUT,
                grid.max.dot(along) + TownLayout::RAIL_RUN_OUT,
            );
            assert!((rail.start.dot(along) - lo).abs() < 1e-3, "{}", c.name);
            assert!((rail.end.dot(along) - hi).abs() < 1e-3, "{}", c.name);
            let corridor = t.rail_corridor().expect("a corridor");
            assert!(corridor.contains_point(rail.start) && corridor.contains_point(rail.end));
            assert!(
                (corridor.size().dot(Vec2::new(n.x.abs(), n.y.abs()))
                    - 2.0 * TownLayout::RAIL_CORRIDOR_HALF)
                    .abs()
                    < 1e-3
            );
            // The station stands between the grid and the track, close to it.
            let station = find(&t, BuildingKind::Station).unwrap();
            let near = station.footprint.center().dot(n)
                - station
                    .footprint
                    .size()
                    .dot(Vec2::new(n.x.abs(), n.y.abs()))
                    / 2.0;
            let far = near
                + station
                    .footprint
                    .size()
                    .dot(Vec2::new(n.x.abs(), n.y.abs()));
            assert!(near > edge, "{}", c.name);
            assert!(rail.start.dot(n) - far < 15.0, "{}", c.name);
            // The width of a train: the platform's edge stands just outside.
            let corridor = Aabb::from_center_size(
                rail.start,
                Vec2::new(n.x.abs(), n.y.abs()) * 3.0 + along * 2000.0,
            );
            for b in &t.buildings {
                assert!(
                    !b.footprint.intersects(&corridor),
                    "{}: {b:?} on the track",
                    c.name
                );
            }
        }
    }

    #[test]
    fn everything_built_sits_on_level_ground() {
        let terrain = Terrain::default();
        for t in sample_towns() {
            let area = t.flat_area(&terrain);
            assert_eq!(
                area.height,
                terrain.natural_height(area.center.x, area.center.y)
            );
            let leveled = terrain.clone().with_flat_area(area);
            assert_eq!(t.leveled(), t.extent().expanded(area.falloff), "{}", t.name);
            assert!(t.extent().contains(&t.grid()));
            for b in &t.buildings {
                assert!(t.extent().contains(&b.footprint), "{}: {b:?}", t.name);
                for corner in [b.footprint.min, b.footprint.max] {
                    assert_eq!(leveled.height(corner.x, corner.y), area.height);
                }
            }
        }
    }

    #[test]
    fn obstacles_cover_buildings_and_trees() {
        for t in sample_towns() {
            let obstacles = t.obstacles();
            let solid = t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform);
            let furniture: Vec<Aabb> = t
                .plaza_furniture()
                .iter()
                .flat_map(PlazaFurniture::obstacles)
                .collect();
            let palenques: usize = t.palenques.iter().map(|p| p.obstacles().count()).sum();
            assert_eq!(
                obstacles.len(),
                solid.clone().count()
                    + t.trees.len()
                    + t.shade_trees.len()
                    + 2
                    + palenques
                    + t.sign_obstacles().len()
                    + furniture.len()
            );
            assert!(
                t.shade_trees
                    .iter()
                    .all(|s| obstacles.contains(&s.obstacle()))
            );
            assert!(furniture.iter().all(|f| obstacles.contains(f)));
            assert!(obstacles.contains(&t.hitching_post.rail));
            assert!(obstacles.contains(&t.hitching_post.horse.footprint));
            for b in solid {
                assert!(obstacles.contains(&b.collider()));
            }
        }
    }

    #[test]
    fn plaza_furniture_follows_the_town_size() {
        let mut seen = std::collections::HashSet::new();
        for t in sample_towns() {
            let paving = Paving::of(t.blocks());
            let all = t.plaza_furniture();
            assert_eq!(all.len(), t.plazas().count(), "{}", t.name);
            for (k, (f, area)) in all.iter().zip(t.plazas()).enumerate() {
                seen.insert(paving);
                assert_eq!((f.area, f.paving), (area, paving));
                // The monument stands on the plinth core always kept for
                // it, smaller in a village.
                assert!(plaza_monument(area).contains(&f.monument));
                assert_eq!(f.monument.center(), area.center());
                let side = if paving == Paving::None { 2.4 } else { 3.0 };
                assert!((f.monument.size().x - side).abs() < 1e-4, "{}", t.name);
                assert_eq!(f.crossing, plaza_crossing(area));
                // A railing in the city only, open at the four mouths.
                let railed = paving == Paving::Everywhere;
                assert_eq!(f.railing.len(), if railed { 8 } else { 0 }, "{}", t.name);
                // Benches by the tier: 24 round a city's main plaza, 4 in
                // a town's, 2 in a village's.
                let want = PlazaFurniture::bench_count(paving, k == 0);
                let main_want = match paving {
                    Paving::Everywhere => 24,
                    Paving::Plaza => 4,
                    Paving::None => 2,
                };
                assert!(!f.benches.is_empty(), "{}", t.name);
                assert!(f.benches.len() <= want, "{}", t.name);
                if k == 0 {
                    assert_eq!(f.benches.len(), main_want, "{}", t.name);
                }
                // A sparse plaza puts one bench to a quadrant at most.
                if paving != Paving::Everywhere {
                    let c = area.center();
                    let quadrants: std::collections::HashSet<(bool, bool)> = f
                        .benches
                        .iter()
                        .map(|b| (b.at.x > c.x, b.at.y > c.y))
                        .collect();
                    assert_eq!(quadrants.len(), f.benches.len(), "{}", t.name);
                }
            }
        }
        assert_eq!(seen.len(), 3, "the samples cover every tier: {seen:?}");
    }

    #[test]
    fn plaza_furniture_keeps_off_the_paths_streets_trees_and_signs() {
        for t in sample_towns() {
            let mut clear: Vec<Aabb> = t.streets.clone();
            clear.extend(t.sidewalks().iter().map(|s| s.area));
            clear.extend(t.sign_obstacles());
            let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
            clear.extend(t.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)));
            for f in t.plaza_furniture() {
                let mut paths = plaza_paths(f.area).to_vec();
                for o in f
                    .benches
                    .iter()
                    .map(Bench::footprint)
                    .chain(f.railing.clone())
                {
                    assert!(f.area.contains(&o), "{}: {o:?} outside", t.name);
                    assert!(!o.intersects(&f.crossing), "{}: {o:?}", t.name);
                    for c in paths.iter().chain(&clear) {
                        assert!(!o.intersects(c), "{}: {o:?} on {c:?}", t.name);
                    }
                }
                // A bench faces its path, its front just off the path.
                for b in &f.benches {
                    let front = b.at + crate::places::side_normal(b.facing) * (Bench::DEPTH / 2.0);
                    let gap = paths
                        .iter()
                        .map(|p| p.closest_point(front).distance(front))
                        .fold(f32::MAX, f32::min);
                    assert!((gap - Bench::PATH_GAP).abs() < 1e-3, "{}: {b:?}", t.name);
                }
                // The paths stay open from edge to edge: the railing only
                // runs beside them.
                paths.push(f.crossing);
                for r in &f.railing {
                    let mouth = Aabb::from_center_size(
                        f.area.center(),
                        Vec2::new(2.0, 2.0) * PlazaFurniture::MOUTH_HALF,
                    );
                    let across_x = Aabb::new(
                        Vec2::new(f.area.min.x, mouth.min.y),
                        Vec2::new(f.area.max.x, mouth.max.y),
                    );
                    let across_y = Aabb::new(
                        Vec2::new(mouth.min.x, f.area.min.y),
                        Vec2::new(mouth.max.x, f.area.max.y),
                    );
                    assert!(
                        !r.intersects(&across_x) && !r.intersects(&across_y),
                        "{r:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_galpon_collides_at_its_walls_not_at_its_eaves() {
        let mut galpones = 0;
        for t in sample_towns() {
            let obstacles = t.obstacles();
            for b in t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
            {
                let barn = matches!(b.kind, BuildingKind::Shed | BuildingKind::GrainStore);
                let inset = if barn { Building::BARN_EAVES } else { 0.0 };
                let want = b.footprint.expanded(-inset);
                assert_eq!(b.collider(), want, "{}: {:?}", t.name, b.kind);
                assert!(obstacles.contains(&want), "{}: {:?}", t.name, b.kind);
                galpones += usize::from(barn);
            }
        }
        assert!(galpones > 10, "{galpones} galpones");
    }

    /// Whether `p` lies on one of `t`'s streets.
    fn on_street(t: &TownLayout, p: Vec2) -> bool {
        t.streets.iter().any(|s| s.contains_point(p))
    }

    #[test]
    fn paving_follows_the_town_size() {
        assert_eq!(Paving::of((3, 3)), Paving::None);
        assert_eq!(Paving::of((4, 3)), Paving::Plaza);
        assert_eq!(Paving::of((5, 4)), Paving::Plaza);
        assert_eq!(Paving::of((6, 6)), Paving::Plaza);
        assert_eq!(Paving::of((7, 6)), Paving::Everywhere);
        assert_eq!(Paving::of((8, 8)), Paving::Everywhere);
    }

    #[test]
    fn sidewalks_lie_on_the_streets_clear_of_buildings_and_plazas() {
        let pico = TownLayout::generate(&TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        let mut paved = 0;
        for t in sample_towns().into_iter().chain([pico]) {
            for walk in t.sidewalks() {
                let a = walk.area;
                assert!(
                    (a.size().x.min(a.size().y) - Sidewalk::WIDTH).abs() < 1e-3,
                    "{}: {a:?}",
                    t.name
                );
                // Corners (just inside) and middle on a street.
                let inner = a.expanded(-0.01);
                for p in [
                    inner.min,
                    inner.max,
                    Vec2::new(inner.min.x, inner.max.y),
                    a.center(),
                ] {
                    assert!(
                        on_street(&t, p),
                        "{}: {a:?} off the street at {p:?}",
                        t.name
                    );
                }
                for b in &t.buildings {
                    assert!(
                        !overlap(&a, &b.footprint),
                        "{}: {a:?} on {:?}",
                        t.name,
                        b.kind
                    );
                }
                for plaza in t.plazas() {
                    assert!(!overlap(&a, &plaza), "{}: {a:?} in a plaza", t.name);
                }
                assert!(!walk.curbs.is_empty());
                paved += 1;
            }
        }
        assert!(paved > 200, "{paved} strips");
    }

    #[test]
    fn a_city_paves_every_block_in_brick_a_town_its_plaza_and_a_village_nothing_but_earth() {
        let towns = sample_towns();
        let count =
            |t: &TownLayout, walk: Walk| t.sidewalks().iter().filter(|w| w.walk == walk).count();
        // Every strip of every town has its curbs.
        for t in &towns {
            assert!(
                t.sidewalks().iter().all(|w| !w.curbs.is_empty()),
                "{}",
                t.name
            );
        }
        // Pico: a ring round every block but the plaza's, and the plaza's, all brick.
        let pico = TownLayout::generate(&TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        assert_eq!(pico.sidewalks().len(), 4 * (64 - 4 + 1));
        assert_eq!(count(&pico, Walk::Brick), 244);
        // Castex (20 blocks): brick round the plaza's one block, four strips,
        // each against the plaza; earth round the other 19 blocks.
        let castex = &towns[0];
        assert_eq!(Paving::of(castex.blocks()), Paving::Plaza);
        assert_eq!(castex.sidewalks().len(), 80);
        assert_eq!(
            (count(castex, Walk::Brick), count(castex, Walk::Earth)),
            (4, 76)
        );
        let ring = castex.plaza.expanded(Sidewalk::WIDTH);
        for walk in castex.sidewalks().iter().filter(|w| w.walk == Walk::Brick) {
            assert!(
                ring.contains(&walk.area),
                "{:?} off the plaza's edge",
                walk.area
            );
            assert!(!overlap(&walk.area, &castex.plaza));
        }
        // A 3 x 3 village: nine rings less the plaza's block plus the plaza's,
        // all beaten earth.
        let village = towns.iter().find(|t| t.blocks() == (3, 3)).unwrap();
        assert_eq!(Paving::of(village.blocks()), Paving::None);
        assert_eq!(village.sidewalks().len(), 36);
        assert_eq!(count(village, Walk::Earth), 36);
    }

    #[test]
    fn a_village_has_no_brick_even_round_its_plaza() {
        let towns = sample_towns();
        let village = towns.iter().find(|t| t.blocks() == (3, 3)).unwrap();
        assert_eq!(Paving::of(village.blocks()), Paving::None);
        assert!(village.sidewalks().iter().all(|w| w.walk == Walk::Earth));
        // The plaza's ring is four strips against the plaza, beaten earth like
        // the rest, never brick.
        let ring = village.plaza.expanded(Sidewalk::WIDTH);
        let round_plaza = village
            .sidewalks()
            .iter()
            .filter(|w| ring.contains(&w.area) && !overlap(&w.area, &village.plaza))
            .filter(|w| overlap(&w.area.expanded(0.01), &village.plaza))
            .count();
        assert_eq!(round_plaza, 4);
    }

    #[test]
    fn the_platform_is_a_floor_with_clear_ramps_not_an_obstacle() {
        let mut checked = 0;
        for t in sample_towns() {
            let obstacles = t.obstacles();
            let platforms: Vec<&Building> = t
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::Platform)
                .collect();
            let floors = t.floors();
            assert_eq!(floors.len(), platforms.len(), "{}", t.name);
            for (floor, platform) in floors.iter().zip(platforms) {
                assert!(!obstacles.contains(&platform.footprint), "{}", t.name);
                assert_eq!(floor.footprint, platform.footprint);
                assert_eq!(floor.height, platform.height);
                // Along the track, so the ramps lead along it.
                let rail = t.railway.unwrap();
                let track = rail.end - rail.start;
                assert!((floor.along.dot(track) - track.length()).abs() < 1e-3);
                // Nothing built stands on the platform or its ramps.
                for area in floor.ramps().into_iter().chain([floor.footprint]) {
                    for o in &obstacles {
                        let inside = Aabb::new(
                            area.min + Vec2::new(0.01, 0.01),
                            area.max - Vec2::new(0.01, 0.01),
                        );
                        assert!(!o.intersects(&inside), "{}: {o:?} on {area:?}", t.name);
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 3);
    }

    #[test]
    fn the_station_canopy_hangs_over_the_platform_from_the_track_side_wall() {
        let mut checked = 0;
        for t in sample_towns() {
            let canopies = t.canopies();
            let Some(station) = find(&t, BuildingKind::Station) else {
                assert!(canopies.is_empty(), "{}", t.name);
                continue;
            };
            let platform = find(&t, BuildingKind::Platform).expect("a platform");
            assert_eq!(canopies.len(), 1, "{}", t.name);
            let canopy = canopies[0];
            // Flush with the station's back wall, on the track side.
            let (s, c) = (station.footprint, canopy.footprint);
            assert!(!s.intersects(&c.expanded(-0.01)), "{}", t.name);
            assert!(
                s.intersects(&c.expanded(0.01)),
                "{}: touches the station",
                t.name
            );
            let back = station.front.opposite();
            let toward = c.center() - s.center();
            let out = match back {
                Side::North => toward.y,
                Side::South => -toward.y,
                Side::East => toward.x,
                Side::West => -toward.x,
            };
            assert!(out > 0.0, "{}: on the track side", t.name);
            // As wide as the station and as deep as the roof render draws.
            let size = c.size();
            let (width, depth, wall) = match back {
                Side::North | Side::South => (size.x, size.y, s.size().x),
                Side::East | Side::West => (size.y, size.x, s.size().y),
            };
            assert!((depth - Canopy::DEPTH).abs() < 1e-4, "{}", t.name);
            assert!((width - wall).abs() < 1e-4, "{}", t.name);
            // Over the platform, with headroom above its top.
            assert!(platform.footprint.contains(&c), "{}", t.name);
            assert!(canopy.bottom() - platform.height > 2.0, "{}", t.name);
            assert!(canopy.top() > canopy.bottom());
            checked += 1;
        }
        assert!(checked > 3);
    }

    /// Distance from the railway's centerline to the near and far sides
    /// of `area`, across the track (the towns' tracks run along an axis).
    fn across_the_track(t: &TownLayout, area: Aabb) -> (f32, f32) {
        let rail = t.railway.unwrap();
        let span = rail.end - rail.start;
        let dir = span * (1.0 / span.length());
        let normal = Vec2::new(-dir.y, dir.x);
        let (lo, hi) = (area.min, area.max);
        [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y)]
            .iter()
            .map(|c| (*c - rail.start).dot(normal).abs())
            .fold((f32::MAX, f32::MIN), |(n, f), d| (n.min(d), f.max(d)))
    }

    #[test]
    fn every_platforms_track_edge_is_1_65_m_from_the_track_and_its_far_edge_11_m() {
        let mut checked = 0;
        for t in sample_towns() {
            let Some(platform) = find(&t, BuildingKind::Platform) else {
                continue;
            };
            let (near, far) = across_the_track(&t, platform.footprint);
            assert!((near - 1.65).abs() < 1e-3, "{}: {near}", t.name);
            assert!((far - 11.0).abs() < 1e-3, "{}: {far}", t.name);
            assert_eq!(Floor::PLATFORM_EDGE, 1.65);
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn the_canopy_edge_and_its_posts_stay_5_5_m_from_the_track_clear_of_a_train() {
        let mut checked = 0;
        for t in sample_towns() {
            let Some(canopy) = t.canopies().first().copied() else {
                continue;
            };
            let (near, far) = across_the_track(&t, canopy.footprint);
            // The roof ends where it did, 5.5 m from the track (its columns
            // stand 0.4 m in from the edge, 5.1 m out): a train's side is
            // 1.5 m out, so neither reaches over the cars.
            assert!((near - 5.5).abs() < 1e-3, "{}: {near}", t.name);
            assert!(far > near);
            assert!(near > crate::LineTrain::WIDTH / 2.0 + 3.0, "{}", t.name);
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn a_horse_is_tied_in_front_of_the_general_store() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let store = find(&t, BuildingKind::GeneralStore).unwrap();
            let post = t.hitching_post;
            let horse = post.horse;
            // On the street the store faces, just past its veranda.
            let street = t
                .streets
                .iter()
                .find(|s| s.contains(&horse.footprint))
                .expect("horse stands on a street");
            assert!(street.contains(&post.rail));
            let n = normal(store.front);
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let half = store.footprint.size() * 0.5;
            let face = store.footprint.center().dot(n) + half.dot(Vec2::new(n.x.abs(), n.y.abs()));
            let horse_half = horse.footprint.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
            let gap = horse.footprint.center().dot(n) - horse_half - face;
            assert!(
                (post.veranda..post.veranda + 3.5).contains(&gap),
                "{}: gap {gap}",
                c.name
            );
            assert!((1.0..=3.5).contains(&post.veranda));
            let off = (horse.footprint.center() - store.footprint.center()).dot(along);
            assert!(off.abs() < 1e-4);
            assert_eq!(horse.position, horse.footprint.center());
            // Faces along the street, tied alongside the rail without touching it.
            assert!(horse.heading.forward().dot(n).abs() < 1e-5);
            assert!(!horse.footprint.intersects(&post.rail));
            // Leaves room to pass: a rider fits between the horse and the far side.
            let far =
                street.center().dot(n) + street.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
            let room = far - (horse.footprint.center().dot(n) + horse_half);
            assert!(
                room > 2.0 * Rider::COLLIDER_RADIUS + 2.0,
                "{}: room {room}",
                c.name
            );
            // And clear of the walkers' lanes down the middle of the street.
            let middle = street.center().dot(n);
            assert!(
                middle - (horse.footprint.center().dot(n) + horse_half) > 1.25,
                "{}",
                c.name
            );
        }
    }

    #[test]
    fn arrival_is_on_open_road_outside_the_grid_facing_in() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let (spawn, heading) = t.arrival();
            let grid = t.grid();
            assert!(!grid.contains_point(spawn), "{}", c.name);
            let inward = heading.forward();
            assert!(
                grid.contains_point(spawn + inward * 40.0),
                "{}: faces into town",
                c.name
            );
            if let Some(r) = c.railway.filter(|r| !r.through) {
                assert!(
                    inward.dot(normal(r.side)) > -1e-4,
                    "{}: comes from away from the track",
                    c.name
                );
            }
            // Through a line in the middle, it comes in on the station's
            // side, from the edge away from the track.
            if let (Some(r), Some(band)) = (c.railway, t.rail_band()) {
                assert!(
                    (spawn - band.center()).dot(normal(r.side)) > 0.0,
                    "{}",
                    c.name
                );
            }
            let road = t.streets.iter().any(|s| {
                s.contains_point(spawn + inward * 30.0)
                    && (s.size().dot(Vec2::new(inward.x.abs(), inward.y.abs())) > c.street_width)
            });
            assert!(road, "{}: lines up with a street into town", c.name);
            for o in t.obstacles() {
                assert!(!o.expanded(2.0).contains_point(spawn));
            }
            assert!(t.leveled().expanded(-5.0).contains_point(spawn));
        }
    }

    #[test]
    fn invalid_configs_are_rejected_with_the_right_error() {
        let base = TownConfig::castex();
        let cases = [
            (
                TownConfig {
                    name: "  ".into(),
                    ..base.clone()
                },
                TownConfigError::EmptyName,
            ),
            (
                TownConfig {
                    block_size: f32::NAN,
                    ..base.clone()
                },
                TownConfigError::NotFinite {
                    field: "block_size",
                },
            ),
            (
                TownConfig {
                    origin: Vec2::new(f32::INFINITY, 0.0),
                    ..base.clone()
                },
                TownConfigError::NotFinite { field: "origin" },
            ),
            (
                TownConfig {
                    blocks: (2, 5),
                    ..base.clone()
                },
                TownConfigError::TooFewBlocks {
                    blocks: (2, 5),
                    min: 3,
                },
            ),
            (
                TownConfig {
                    blocks: (5, 0),
                    ..base.clone()
                },
                TownConfigError::TooFewBlocks {
                    blocks: (5, 0),
                    min: 3,
                },
            ),
            (
                TownConfig {
                    block_size: 20.0,
                    ..base.clone()
                },
                TownConfigError::BlockTooSmall {
                    block_size: 20.0,
                    min: 30.0,
                },
            ),
            (
                TownConfig {
                    street_width: 6.0,
                    ..base.clone()
                },
                TownConfigError::StreetTooNarrow {
                    street_width: 6.0,
                    min: 10.0,
                },
            ),
            (
                TownConfig {
                    railway: Some(RailwayConfig {
                        side: Side::North,
                        offset: 10.0,
                        through: false,
                    }),
                    ..base.clone()
                },
                TownConfigError::RailwayTooClose {
                    offset: 10.0,
                    min: 24.0,
                },
            ),
            (
                TownConfig {
                    plaza: PlazaPlacement::Block(5, 0),
                    ..base.clone()
                },
                TownConfigError::PlazaOutOfRange {
                    block: (5, 0),
                    blocks: (4, 5),
                },
            ),
            (
                TownConfig {
                    blocks: (4, 5),
                    plaza: PlazaPlacement::Center,
                    ..base.clone()
                },
                TownConfigError::NoCentralBlock { blocks: (4, 5) },
            ),
            (
                TownConfig {
                    railway: None,
                    plaza: PlazaPlacement::FacingStation,
                    ..base.clone()
                },
                TownConfigError::NoStationToFace,
            ),
        ];
        for (config, expected) in cases {
            assert_eq!(TownLayout::generate(&config), Err(expected.clone()));
            assert!(!expected.to_string().is_empty());
        }
        // How far a town reaches is the world's business, not the town's:
        // any size and origin that builds is a valid town.
        for config in [
            TownConfig {
                blocks: (12, 5),
                ..base.clone()
            },
            TownConfig {
                origin: Vec2::new(5150.0, -2000.0),
                ..base.clone()
            },
            TownConfig {
                block_size: 90.0,
                ..base.clone()
            },
        ] {
            TownLayout::generate(&config).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn a_town_is_the_same_wherever_it_stands() {
        let offset = Vec2::new(3000.0, -2000.0);
        for c in sample_configs() {
            let here = TownLayout::generate(&c).unwrap();
            let there = TownLayout::generate(&TownConfig {
                origin: c.origin + offset,
                ..c.clone()
            })
            .unwrap();
            let moved = |a: Aabb| Aabb::new(a.min + offset, a.max + offset);
            let near =
                |a: Aabb, b: Aabb| a.min.distance(b.min) < 1e-2 && a.max.distance(b.max) < 1e-2;
            assert!(near(there.grid(), moved(here.grid())), "{}", c.name);
            assert!(near(there.extent(), moved(here.extent())), "{}", c.name);
            assert!(near(there.leveled(), moved(here.leveled())), "{}", c.name);
            assert!(near(there.plaza, moved(here.plaza)), "{}", c.name);
            match (there.station_plaza, here.station_plaza) {
                (Some(a), Some(b)) => assert!(near(a, moved(b)), "{}", c.name),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
            assert_eq!(there.streets.len(), here.streets.len());
            for (a, b) in there.streets.iter().zip(&here.streets) {
                assert!(near(*a, moved(*b)), "{}", c.name);
            }
            assert_eq!(there.buildings.len(), here.buildings.len());
            for (a, b) in there.buildings.iter().zip(&here.buildings) {
                assert_eq!((a.kind, a.front), (b.kind, b.front));
                assert!(near(a.footprint, moved(b.footprint)), "{}", c.name);
            }
            assert!(there.arrival().0.distance(here.arrival().0 + offset) < 1e-2);
            assert_eq!(there.arrival().1, here.arrival().1);
            match (there.railway, here.railway) {
                (Some(a), Some(b)) => {
                    assert!(a.start.distance(b.start + offset) < 1e-2);
                    assert!(a.end.distance(b.end + offset) < 1e-2);
                }
                (None, None) => {}
                other => panic!("{other:?}"),
            }
            let graph = |t: &TownLayout| crate::StreetGraph::from_town(t).nodes;
            for (a, b) in graph(&there).iter().zip(graph(&here)) {
                assert!(a.distance(b + offset) < 1e-2, "{}", c.name);
            }
            for (a, b) in there.places().iter().zip(here.places()) {
                assert_eq!(a.kind, b.kind);
                for (sa, sb) in a.signs.iter().zip(&b.signs) {
                    assert!(sa.anchor.distance(sb.anchor + offset) < 1e-2);
                }
            }
            assert_eq!(there.seed, c.seed);
        }
    }

    /// FNV-1a hash of everything a town builds on the ground, to pin layouts.
    fn fingerprint(t: &TownLayout) -> u64 {
        let text = format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            t.streets,
            t.plaza,
            t.trees,
            t.railway,
            t.buildings,
            t.hitching_post,
            t.arrival(),
            t.extent(),
            t.places()
        );
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Whether `c`'s line runs through the middle of the town: these towns
    /// keep their own tests, not the ones of a town beside its line.
    fn through(c: &TownConfig) -> bool {
        c.railway.is_some_and(|r| r.through)
    }

    /// Small towns beside their line or without one: the sample configs of
    /// 36 blocks or fewer, plus Monte Nievas and the 5 x 4 Castex of
    /// `assets/world/towns.ron`.
    fn small_configs() -> Vec<TownConfig> {
        let shipped = [
            TownConfig {
                name: "Monte Nievas".into(),
                blocks: (4, 3),
                seed: 7,
                // As shipped: no comisaría (the errand's story).
                police_stations: Some(0),
                ..TownConfig::castex()
            },
            TownConfig {
                blocks: (5, 4),
                seed: 2,
                ..TownConfig::castex()
            },
        ];
        shipped
            .into_iter()
            .chain(
                sample_configs()
                    .into_iter()
                    .filter(|c| c.size() == TownSize::Small && !through(c)),
            )
            .collect()
    }

    /// Big and very big sample towns, generated, with their configs.
    fn bigger_towns() -> Vec<(TownConfig, TownLayout)> {
        sample_configs()
            .into_iter()
            .zip(sample_towns())
            .filter(|(c, _)| c.size() != TownSize::Small && !through(c))
            .collect()
    }

    fn all(t: &TownLayout, kind: BuildingKind) -> Vec<&Building> {
        t.buildings.iter().filter(|b| b.kind == kind).collect()
    }

    /// Blocks `(east-west, north-south)` the main plaza spans.
    fn plaza_span(c: &TownConfig, t: &TownLayout) -> (usize, usize) {
        let pitch = c.block_size + c.street_width;
        let blocks = |len: f32| ((len + c.street_width) / pitch).round() as usize;
        (blocks(t.plaza.size().x), blocks(t.plaza.size().y))
    }

    /// Checks `b` looks toward `area` with a street just in front of it.
    fn assert_faces(t: &TownLayout, b: &Building, area: Aabb, street_width: f32) {
        let n = normal(b.front);
        assert!(
            (area.center() - b.footprint.center()).dot(n) > 0.0,
            "{}: {b:?} looks away from {area:?}",
            t.name
        );
        let half = b.footprint.size() * 0.5;
        let ahead = b.footprint.center()
            + n * (n.x.abs() * half.x + n.y.abs() * half.y + street_width / 2.0);
        assert!(
            t.streets.iter().any(|s| s.contains_point(ahead)),
            "{}: no street in front of {b:?}",
            t.name
        );
    }

    #[test]
    fn town_size_follows_the_block_count() {
        for (blocks, size) in [
            ((3, 3), TownSize::Small),
            ((5, 4), TownSize::Small),
            ((6, 6), TownSize::Small),
            ((9, 4), TownSize::Small),
            ((37, 1), TownSize::Big),
            ((7, 6), TownSize::Big),
            ((9, 7), TownSize::Big),
            ((8, 8), TownSize::VeryBig),
            ((16, 4), TownSize::VeryBig),
        ] {
            let c = TownConfig {
                blocks,
                ..TownConfig::castex()
            };
            assert_eq!(c.size(), size, "{blocks:?}");
        }
        assert!(sample_configs().iter().any(|c| c.size() == TownSize::Big));
        assert!(
            sample_configs()
                .iter()
                .any(|c| c.size() == TownSize::VeryBig)
        );
    }

    #[test]
    fn small_towns_keep_one_plaza_store_and_police_station() {
        for c in small_configs() {
            let t = TownLayout::generate(&c).unwrap();
            assert_eq!(t.size, TownSize::Small, "{}", c.name);
            assert_eq!(t.station_plaza, None, "{}", c.name);
            assert_eq!(t.plazas().count(), 1, "{}", c.name);
            assert_eq!(plaza_span(&c, &t), (1, 1), "{}", c.name);
            assert_eq!(count(&t, BuildingKind::GeneralStore), 1, "{}", c.name);
            // One, unless overridden (Monte Nievas has none).
            let police = c.police_stations.unwrap_or(1);
            assert_eq!(count(&t, BuildingKind::PoliceStation), police, "{}", c.name);
            assert_eq!(t.townsfolk_count(), crate::townsfolk::TOWNSFOLK_COUNT);
        }
    }

    #[test]
    fn small_towns_keep_their_layout_byte_for_byte() {
        // Fingerprints of the layouts generated before big towns existed.
        // Every one re-pinned on purpose (2026-10-23): the store and the
        // police station are spread over the town instead of round the
        // plaza, and a small town's store is a pulpería. Before that
        // (PR-F1), plaza trees stopped standing on the plaza paths, so the
        // ring spots on and beside the paths are left empty. Earlier, Monte
        // Nievas was re-pinned when it shipped with `police_stations: Some(0)`.
        // The towns with a platform were re-pinned again (2026-10-26): the
        // station has two name boards on it instead of one (`places()` is
        // part of the fingerprint); the two towns without a train are unchanged.
        // Three were re-pinned for the 2.5 m sidewalks (2026-10-26): the
        // store's veranda stays over the walk, so on a street wider than
        // 10.4 m it is 2.5 m deep, not 3 (Cinco por cinco, Seis por cuatro,
        // Oeste). The new shade trees are not part of the fingerprint.
        // All eleven were re-pinned for S6c (2026-10-26): the sidewalks are
        // 3.5 m and every sample street is 2 m wider, so every grid, and the
        // store's veranda, moved. Two more were re-pinned for S6d (Tres and
        // Este: their 10 and 11 m sample streets are 12 m now, because every
        // town has sidewalks). Nine were re-pinned for T5b (2026-10-27): every
        // platform grew 2.35 m toward the track (edge 1.65 m from it, was
        // 4 m); the two towns without a train are unchanged. All eleven were
        // re-pinned when the church lost its signboard (2026-10-28): the
        // places' signs are part of the fingerprint.
        let pinned = [
            0x724c_91c9_470f_e41f,
            0xddf4_88db_9aca_9d42,
            0xabf1_d06d_e5fe_009f,
            0x1967_ad10_c15d_9323,
            0x4d89_5840_612f_7b5e,
            0x13b1_8853_df0f_ebac,
            0x365f_dd8c_2dfa_515e,
            0x1aae_2a35_d655_4343,
            0x22e7_02d3_ccc3_bbdf,
            0x4d1c_69a6_fa5a_47b4,
            0x8854_fa05_02dd_27b9,
        ];
        let configs = small_configs();
        assert_eq!(configs.len(), pinned.len());
        for (c, expected) in configs.iter().zip(pinned) {
            let t = TownLayout::generate(c).unwrap();
            assert_eq!(fingerprint(&t), expected, "{} changed", c.name);
        }
    }

    #[test]
    fn a_police_override_of_zero_leaves_the_town_without_a_comisaria() {
        let mut configs = small_configs();
        configs.extend(bigger_towns().into_iter().map(|(c, _)| c));
        for c in configs {
            let none = TownConfig {
                police_stations: Some(0),
                ..c.clone()
            };
            let t = TownLayout::generate(&none).unwrap();
            assert_eq!(count(&t, BuildingKind::PoliceStation), 0, "{}", c.name);
            assert!(
                t.places()
                    .iter()
                    .all(|p| p.kind != crate::PlaceKind::PoliceStation),
                "{}",
                c.name
            );
            assert_eq!(t.police_door(), None, "{}", c.name);
            // Everything else a town needs is still there.
            let default = TownLayout::generate(&c).unwrap();
            assert_eq!(count(&t, BuildingKind::Church), 1, "{}", c.name);
            assert_eq!(
                count(&t, BuildingKind::GeneralStore),
                count(&default, BuildingKind::GeneralStore),
                "{}",
                c.name
            );
            assert_eq!(t.plaza, default.plaza, "{}", c.name);
            assert_eq!(t.hitching_post, default.hitching_post, "{}", c.name);
        }
    }

    #[test]
    fn the_police_override_can_lower_but_not_raise_the_tiers_count() {
        for (c, _) in bigger_towns()
            .into_iter()
            .filter(|(c, _)| c.size() == TownSize::VeryBig)
        {
            let one = TownConfig {
                police_stations: Some(1),
                ..c.clone()
            };
            let t = TownLayout::generate(&one).unwrap();
            assert_eq!(count(&t, BuildingKind::PoliceStation), 1, "{}", c.name);
        }
        for c in small_configs() {
            // The tier's own count is the same as no override.
            let same = TownConfig {
                police_stations: Some(1),
                ..c.clone()
            };
            if c.police_stations.is_none() {
                assert_eq!(TownLayout::generate(&same), TownLayout::generate(&c));
            }
            let two = TownConfig {
                police_stations: Some(2),
                ..c.clone()
            };
            let error = TownLayout::generate(&two).unwrap_err();
            assert_eq!(
                error,
                TownConfigError::TooManyPoliceStations { wanted: 2, max: 1 },
                "{}",
                c.name
            );
            assert!(error.to_string().contains("police_stations 2"), "{error}");
        }
    }

    /// Whether `plaza` covers block `(i, j)` of town `c`.
    fn covers(c: &TownConfig, t: &TownLayout, plaza: Aabb, (i, j): (usize, usize)) -> bool {
        let pitch = c.block_size + c.street_width;
        let g = t.grid().min;
        let center = Vec2::new(
            g.x + pitch * i as f32 + c.block_size / 2.0,
            g.y + pitch * j as f32 + c.block_size / 2.0,
        );
        plaza.contains_point(center)
    }

    #[test]
    fn bigger_towns_honor_block_and_facing_station_plazas() {
        let base = |blocks, plaza| TownConfig {
            name: "Grande".into(),
            blocks,
            plaza,
            ..TownConfig::castex()
        };
        for blocks in [(8, 6), (6, 8), (9, 9)] {
            for block in [(0, 0), (blocks.0 - 1, 2), (3, blocks.1 - 1)] {
                let c = base(blocks, PlazaPlacement::Block(block.0, block.1));
                let t = TownLayout::generate(&c).unwrap();
                assert!(covers(&c, &t, t.plaza, block), "{blocks:?} {block:?}");
                let expected = c.size().plaza_span(c.blocks);
                assert_eq!(plaza_span(&c, &t), expected, "{blocks:?} {block:?}");
                assert!(t.grid().contains(&t.plaza), "{blocks:?} {block:?}");
                for b in &t.buildings {
                    assert!(
                        !b.footprint.intersects(&t.plaza.expanded(-0.01)),
                        "{:?}",
                        b.kind
                    );
                }
            }
            // Facing the station: on the row next to the railway (north),
            // as near the middle as whole blocks allow.
            let c = base(blocks, PlazaPlacement::FacingStation);
            let t = TownLayout::generate(&c).unwrap();
            assert!(
                covers(&c, &t, t.plaza, (blocks.0 / 2, blocks.1 - 1))
                    || covers(&c, &t, t.plaza, ((blocks.0 - 1) / 2, blocks.1 - 1)),
                "{blocks:?}: {:?} in {:?}",
                t.plaza,
                t.grid()
            );
            let off = t.plaza.center().x - t.grid().center().x;
            let pitch = c.block_size + c.street_width;
            assert!(off.abs() <= pitch, "{blocks:?}: {off}");
            assert_eq!(plaza_span(&c, &t), c.size().plaza_span(c.blocks));
        }
    }

    #[test]
    fn the_plaza_span_never_outgrows_a_narrow_grid() {
        // The narrowest grid allowed, very big all the same.
        let narrow = (TownConfig::MIN_BLOCKS, 64 / TownConfig::MIN_BLOCKS + 1);
        let size = TownSize::of(narrow);
        assert_ne!(size, TownSize::Small);
        let (w, h) = size.plaza_span(narrow);
        assert!(w <= narrow.0 && h <= narrow.1);
        // Should the least grid ever shrink, the span still fits.
        for blocks in [(1, 64), (64, 1), (1, 40), (2, 70)] {
            for size in [TownSize::Small, TownSize::Big, TownSize::VeryBig] {
                let (w, h) = size.plaza_span(blocks);
                assert!(w <= blocks.0 && h <= blocks.1, "{size:?} {blocks:?}");
            }
        }
        let c = TownConfig {
            blocks: narrow,
            ..TownConfig::castex()
        };
        let t = TownLayout::generate(&c).unwrap();
        assert!(t.grid().contains(&t.plaza));
    }

    #[test]
    fn bigger_towns_merge_blocks_into_a_central_plaza() {
        for (c, t) in bigger_towns() {
            let span = plaza_span(&c, &t);
            let expected = match (c.size(), c.blocks.0 >= c.blocks.1) {
                (TownSize::Big, true) => (2, 1),
                (TownSize::Big, false) => (1, 2),
                _ => (2, 2),
            };
            assert_eq!(span, expected, "{}", c.name);
            // Centered: as close to the grid center as whole blocks allow.
            let pitch = c.block_size + c.street_width;
            let off = t.plaza.center() - t.grid().center();
            assert!(off.x.abs() <= pitch / 2.0 + 1e-3, "{}: {off:?}", c.name);
            assert!(off.y.abs() <= pitch / 2.0 + 1e-3, "{}: {off:?}", c.name);
            assert!(
                off.x <= 1e-3 && off.y <= 1e-3,
                "{}: ties go west and south",
                c.name
            );
            // Its own trees, spread along its longer edges too.
            let inner = t.plaza.expanded(-TREE_TRUNK_RADIUS);
            let trees = t.trees.iter().filter(|&&p| inner.contains_point(p)).count();
            assert!(trees >= 8 * span.0.max(span.1), "{}: {trees}", c.name);
        }
    }

    #[test]
    fn no_street_runs_through_a_merged_plaza() {
        for (c, t) in bigger_towns() {
            for street in &t.streets {
                assert!(!street.intersects(&t.plaza), "{}: {street:?}", c.name);
            }
            // Streets still border it on every side.
            for side in [Side::North, Side::South, Side::East, Side::West] {
                let n = normal(side);
                let reach = t.plaza.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
                let beside = t.plaza.center() + n * (reach + c.street_width / 2.0);
                assert!(
                    t.streets.iter().any(|s| s.contains_point(beside)),
                    "{}",
                    c.name
                );
            }
            // Walkers have no intersection inside it and no way across it.
            let graph = crate::StreetGraph::from_town(&t);
            let (cols, rows) = c.blocks;
            let (w, h) = plaza_span(&c, &t);
            let inner = (w - 1) * (h - 1);
            assert_eq!(
                graph.nodes.len(),
                (cols + 1) * (rows + 1) - inner,
                "{}",
                c.name
            );
            for (a, links) in graph.links.iter().enumerate() {
                assert!(!t.plaza.contains_point(graph.nodes[a]), "{}", c.name);
                for &b in links {
                    let mid = (graph.nodes[a] + graph.nodes[b]) * 0.5;
                    assert!(!t.plaza.contains_point(mid), "{}: across the plaza", c.name);
                }
            }
            // Nothing stands in it but its trees, its sign and its
            // furniture.
            let mut sign: Vec<Aabb> = t.sign_obstacles();
            sign.extend(
                t.plaza_furniture()
                    .iter()
                    .flat_map(PlazaFurniture::obstacles),
            );
            for o in t.obstacles() {
                let tree = t.trees.iter().any(|&p| o.contains_point(p))
                    || t.shade_trees.iter().any(|s| o.contains_point(s.at));
                if !tree && !sign.contains(&o) {
                    assert!(!o.intersects(&t.plaza), "{}: {o:?} in the plaza", c.name);
                }
            }
        }
    }

    #[test]
    fn bigger_towns_scale_stores_police_and_crowds() {
        for (c, t) in bigger_towns() {
            let n = c.blocks.0 * c.blocks.1;
            let (stores, police) = match c.size() {
                TownSize::Big => (2, 1),
                _ => (2, 2),
            };
            assert_eq!(
                all(&t, BuildingKind::GeneralStore).len(),
                stores,
                "{}",
                c.name
            );
            assert_eq!(
                all(&t, BuildingKind::PoliceStation).len(),
                police,
                "{}",
                c.name
            );
            let crowd = (0.8 * n as f32).round() as usize;
            assert_eq!(t.townsfolk_count(), crowd.max(16), "{}", c.name);
        }
        let pico = TownLayout::generate(&TownConfig {
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        assert_eq!(pico.size, TownSize::VeryBig);
        assert_eq!(count(&pico, BuildingKind::GeneralStore), 2);
        assert_eq!(pico.townsfolk_count(), 51);
    }

    #[test]
    fn a_townsfolk_override_wins_over_the_size_formula() {
        for (blocks, formula) in [((4, 5), 16), ((8, 8), 51)] {
            let base = TownConfig {
                blocks,
                ..TownConfig::castex()
            };
            assert_eq!(base.townsfolk, None);
            assert_eq!(base.townsfolk_count(), formula);
            for wanted in [0, 10, TownConfig::MAX_TOWNSFOLK] {
                let c = TownConfig {
                    townsfolk: Some(wanted),
                    ..base.clone()
                };
                assert_eq!(c.townsfolk_count(), wanted);
                assert_eq!(TownLayout::generate(&c).unwrap().townsfolk_count(), wanted);
            }
        }
    }

    #[test]
    fn too_many_townsfolk_are_rejected() {
        let wanted = TownConfig::MAX_TOWNSFOLK + 1;
        let error = TownLayout::generate(&TownConfig {
            townsfolk: Some(wanted),
            ..TownConfig::castex()
        })
        .unwrap_err();
        assert_eq!(
            error,
            TownConfigError::TooManyTownsfolk {
                wanted,
                max: TownConfig::MAX_TOWNSFOLK
            }
        );
        assert!(error.to_string().contains("townsfolk 201"), "{error}");
    }

    #[test]
    fn landmarks_of_bigger_towns_face_their_plaza() {
        for (c, t) in bigger_towns() {
            let stores = all(&t, BuildingKind::GeneralStore);
            let (last, central) = stores.split_last().unwrap();
            // Every store but the last faces the main plaza, the first one
            // with the horse tied in front.
            for store in central {
                assert_faces(&t, store, t.plaza, c.street_width);
            }
            assert_faces(&t, last, t.station_plaza.unwrap(), c.street_width);
            for police in all(&t, BuildingKind::PoliceStation) {
                assert_faces(&t, police, t.plaza, c.street_width);
            }
            let horse = t.hitching_post.horse.position;
            let gap = |s: &Building| s.footprint.closest_point(horse).distance(horse);
            assert!(
                stores[1..].iter().all(|s| gap(stores[0]) < gap(s)),
                "{}",
                c.name
            );
        }
    }

    #[test]
    fn the_station_plaza_is_half_a_block_facing_the_station() {
        let mut checked = 0;
        for (c, t) in bigger_towns() {
            let station_plaza = t.station_plaza.expect("a station plaza");
            assert!(!station_plaza.intersects(&t.plaza), "{}", c.name);
            assert!(t.grid().contains(&station_plaza), "{}", c.name);
            let inner = station_plaza.expanded(-TREE_TRUNK_RADIUS);
            assert!(
                t.trees.iter().filter(|&&p| inner.contains_point(p)).count() >= 8,
                "{}",
                c.name
            );
            let area = |a: Aabb| a.size().x * a.size().y;
            assert!(area(station_plaza) < area(t.plaza), "{}", c.name);
            let Some(r) = c.railway else {
                // Without a railway: far from the main plaza.
                let pitch = c.block_size + c.street_width;
                let d = station_plaza.center() - t.plaza.center();
                assert!(d.x.abs() + d.y.abs() > 2.0 * pitch, "{}", c.name);
                continue;
            };
            let n = normal(r.side);
            let across = Vec2::new(n.x.abs(), n.y.abs());
            let along = Vec2::new(n.y.abs(), n.x.abs());
            // Half a block deep, a whole block long.
            assert!((station_plaza.size().dot(across) - c.block_size / 2.0).abs() < 1e-3);
            assert!((station_plaza.size().dot(along) - c.block_size).abs() < 1e-3);
            // The railway half of a block on the railway row.
            let edge = |b: Aabb| b.center().dot(n) + b.size().dot(across) / 2.0;
            assert!(
                (edge(t.grid()) - edge(station_plaza) - c.street_width).abs() < 1e-3,
                "{}: station plaza not against the railway-side street",
                c.name
            );
            // Within a block and a half of the station along the track.
            let station = find(&t, BuildingKind::Station).unwrap().footprint.center();
            let pitch = c.block_size + c.street_width;
            let distance = (station_plaza.center() - station).dot(along).abs();
            assert!(distance <= 1.5 * pitch + 1e-3, "{}: {distance}", c.name);
            checked += 1;
        }
        assert!(checked >= 3, "{checked} bigger railway towns");
    }

    #[test]
    fn the_rest_of_the_station_plaza_block_keeps_houses() {
        for (c, t) in bigger_towns() {
            let half = t.station_plaza.unwrap();
            // The other half: the side of the station plaza with no street.
            let blocks: Vec<Aabb> = [Side::North, Side::South, Side::East, Side::West]
                .into_iter()
                .map(|side| {
                    let n = normal(side);
                    let grow = n * half.size().dot(Vec2::new(n.x.abs(), n.y.abs()));
                    Aabb::new(
                        Vec2::new(
                            half.min.x.min(half.min.x + grow.x),
                            half.min.y.min(half.min.y + grow.y),
                        ),
                        Vec2::new(
                            half.max.x.max(half.max.x + grow.x),
                            half.max.y.max(half.max.y + grow.y),
                        ),
                    )
                })
                .filter(|b| t.streets.iter().all(|s| !s.intersects(&b.expanded(-0.01))))
                .collect();
            let [block] = blocks[..] else {
                panic!("{}: {blocks:?}", c.name);
            };
            let lots = t
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, BuildingKind::House | BuildingKind::Shed))
                .filter(|b| block.contains(&b.footprint))
                .count();
            assert!(lots >= 1, "{}: no houses beside the station plaza", c.name);
        }
    }

    #[test]
    fn errors_explain_themselves() {
        let error = TownConfigError::BlockTooSmall {
            block_size: 20.0,
            min: 30.0,
        };
        assert!(error.to_string().contains("block_size 20 m"), "{error}");
        let error = TownConfigError::NoCentralBlock { blocks: (4, 5) };
        assert!(error.to_string().contains("4x5"), "{error}");
    }

    /// The very big sample town grown on both sides of its line.
    fn partida() -> (TownConfig, TownLayout) {
        let c = sample_configs()
            .into_iter()
            .find(|c| c.name == "Partida")
            .unwrap();
        let t = TownLayout::generate(&c).unwrap();
        (c, t)
    }

    #[test]
    fn a_through_line_splits_the_town_with_blocks_and_crossings_on_both_sides() {
        let (_, t) = partida();
        let band = t.rail_band().expect("a band");
        let rail = t.railway.unwrap();
        assert!(band.min.y < rail.start.y && rail.start.y < band.max.y);
        assert_eq!(rail.start.y, rail.end.y);
        // The grid holds the band: its extent is the grid itself.
        assert_eq!(t.extent(), t.grid());
        assert!(t.grid().contains(&band));
        // Ring streets on both sides of the band, along its whole length.
        for y in [band.min.y, band.max.y] {
            assert!(
                t.streets.iter().any(|s| s.size().x >= band.size().x - 1e-3
                    && (s.max.y - y).abs() < 1e-3
                    || (s.min.y - y).abs() < 1e-3 && s.size().x >= band.size().x - 1e-3),
                "no ring street at {y}"
            );
        }
        // Crossing streets run square across the band, clear of the yard;
        // the rest stop at its edges.
        let crossings = t.crossing_streets();
        assert!(crossings.len() >= 3 && crossings.len() < 9, "{crossings:?}");
        for (a, b) in &crossings {
            assert_eq!(a.x, b.x);
            assert!(a.y < band.min.y && b.y > band.max.y);
        }
        for s in t.streets.iter().filter(|s| s.size().y > s.size().x) {
            let inside = s.min.y < band.max.y && s.max.y > band.min.y;
            let across = s.min.y < band.min.y && s.max.y > band.max.y;
            assert!(!inside || across, "{s:?} ends in the band");
        }
        for b in t.buildings.iter().filter(|b| band.contains(&b.footprint)) {
            for s in &t.streets {
                assert!(!s.intersects(&b.footprint), "{s:?} through {b:?}");
            }
        }
        // The station faces its half of town across the ring street, with
        // the platform between it and the track.
        let station = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .unwrap();
        assert_eq!(station.front, Side::North);
        let canopy = t.canopies()[0];
        assert!(canopy.footprint.max.y <= station.footprint.min.y + 1e-3);
        // Its 2 x 2 plaza lies whole in the station's half, north-west of the
        // station, with no street through it; the station plaza is the half
        // block across the ring street from the station.
        assert!(t.plaza.min.y > band.max.y);
        let to_plaza = t.plaza.center() - station.footprint.center();
        assert!(to_plaza.x < 0.0 && to_plaza.y > 0.0, "{to_plaza:?}");
        for street in &t.streets {
            assert!(!street.intersects(&t.plaza), "{street:?}");
        }
        let station_plaza = t.station_plaza.expect("a station plaza");
        let ring = t
            .streets
            .iter()
            .find(|s| s.size().x > s.size().y && (s.min.y - band.max.y).abs() < 1e-3)
            .unwrap();
        assert!(
            (station_plaza.min.y - ring.max.y).abs() < 1e-3,
            "{station_plaza:?}"
        );
        assert!((station_plaza.center().x - station.footprint.center().x).abs() < 50.0);
    }

    #[test]
    fn a_through_line_needs_room_and_an_east_west_track() {
        let (c, _) = partida();
        let rail = c.railway.unwrap();
        let east = TownConfig {
            railway: Some(RailwayConfig {
                side: Side::East,
                ..rail
            }),
            ..c.clone()
        };
        assert_eq!(
            TownLayout::generate(&east),
            Err(TownConfigError::ThroughNotEastWest { side: Side::East })
        );
        let low = TownConfig {
            blocks: (6, 3),
            ..c.clone()
        };
        assert!(matches!(
            TownLayout::generate(&low),
            Err(TownConfigError::ThroughTooFewRows { rows: 3, min: 4 })
        ));
        // A plaza picked across the track is refused; its own half is fine.
        let across = TownConfig {
            plaza: PlazaPlacement::Block(2, 3),
            ..c.clone()
        };
        assert_eq!(
            TownLayout::generate(&across),
            Err(TownConfigError::PlazaAcrossRailway)
        );
        let south = TownConfig {
            plaza: PlazaPlacement::Block(2, 1),
            ..c.clone()
        };
        assert!(
            TownLayout::generate(&south).unwrap().plaza.max.y < rail.offset.max(0.0) + c.origin.y
        );
        for e in [
            TownConfigError::ThroughNotEastWest { side: Side::West },
            TownConfigError::ThroughTooFewRows { rows: 3, min: 4 },
            TownConfigError::PlazaAcrossRailway,
            TownConfigError::BadDensity { density: 2.0 },
        ] {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn density_thins_the_lots_and_must_be_a_share() {
        let base = TownConfig::castex();
        let count = |c: &TownConfig| {
            TownLayout::generate(c)
                .unwrap()
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, BuildingKind::House | BuildingKind::Shed))
                .count()
        };
        let full = count(&base);
        let thin = count(&TownConfig {
            density: Some(0.3),
            ..base.clone()
        });
        assert!(thin * 2 < full, "{thin} of {full}");
        let houses = |c: &TownConfig| {
            TownLayout::generate(c)
                .unwrap()
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .count()
        };
        let none = houses(&TownConfig {
            density: Some(0.0),
            ..base.clone()
        });
        assert_eq!(none, 0);
        for density in [-0.1, 1.5, f32::NAN] {
            assert!(matches!(
                TownLayout::generate(&TownConfig {
                    density: Some(density),
                    ..base.clone()
                }),
                Err(TownConfigError::BadDensity { .. })
            ));
        }
    }

    /// The buildings that get a palenque besides the store's post.
    fn wants_palenque(t: &TownLayout, k: usize) -> bool {
        let first_store = t
            .buildings
            .iter()
            .position(|b| b.kind == BuildingKind::GeneralStore);
        matches!(
            t.buildings[k].kind,
            BuildingKind::GeneralStore
                | BuildingKind::PoliceStation
                | BuildingKind::Church
                | BuildingKind::Station
        ) && Some(k) != first_store
    }

    /// Every shipped-like town, with the through-line ones.
    fn towns_with_names() -> Vec<(String, TownLayout)> {
        sample_towns()
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect()
    }

    #[test]
    fn every_landmark_but_the_first_store_has_a_palenque_before_it() {
        for (name, t) in towns_with_names() {
            let mut wanted = 0;
            for k in (0..t.buildings.len()).filter(|&k| wants_palenque(&t, k)) {
                wanted += 1;
                let b = &t.buildings[k];
                let p = t
                    .palenques
                    .iter()
                    .find(|p| p.building == k)
                    .unwrap_or_else(|| panic!("{name}: no palenque before {:?}", b.kind));
                // In front of the facade, on the street side, along the street.
                let n = normal(b.front);
                let ahead = (p.rail.center() - b.footprint.center()).dot(n);
                let half = b.footprint.size() * 0.5;
                assert!(ahead > n.x.abs() * half.x + n.y.abs() * half.y, "{name}");
                let size = p.rail.size();
                let long = if n.x.abs() > 0.5 { size.y } else { size.x };
                assert!((long - Palenque::LENGTH).abs() < 1e-4, "{name}");
            }
            assert_eq!(t.palenques.len(), wanted, "{name}: one per landmark");
            // Never one before the store that already has its post.
            assert!(t.palenques.iter().all(|p| wants_palenque(&t, p.building)));
        }
    }

    #[test]
    fn only_cities_tie_horses_to_their_palenques_and_only_a_few() {
        let mut cities = 0;
        for (name, t) in towns_with_names() {
            let horses = t.palenques.iter().filter(|p| p.horse.is_some()).count();
            if t.size == TownSize::Small {
                assert_eq!(horses, 0, "{name}: a town's palenques stay empty");
            } else {
                cities += 1;
                let posts = t.palenques.len();
                let share = Palenque::horses_for(posts);
                assert!(horses <= share, "{name}");
                // Where the streets are wide enough to leave a horse room.
                let narrowest = t
                    .streets
                    .iter()
                    .map(|s| s.size().x.min(s.size().y))
                    .fold(f32::INFINITY, f32::min);
                if narrowest >= 12.0 {
                    assert_eq!(horses, share, "{name}");
                }
                assert!(horses >= 1, "{name}");
            }
        }
        assert!(cities >= 2);
    }

    #[test]
    fn a_city_ties_half_its_palenques_to_a_horse_up_to_a_cap() {
        let tied: Vec<usize> = [0, 1, 2, 3, 4, 5, 6, 7, 8, 20]
            .into_iter()
            .map(Palenque::horses_for)
            .collect();
        assert_eq!(tied, [0, 0, 1, 1, 2, 2, 3, 3, 3, 3]);
        assert_eq!(Palenque::MAX_HORSES, 3);
    }

    #[test]
    fn the_seed_picks_which_palenques_tie_a_horse() {
        let pico = |seed| {
            TownLayout::generate(&TownConfig {
                name: "Pico".into(),
                blocks: (8, 8),
                seed,
                ..TownConfig::castex()
            })
            .unwrap()
        };
        let tied = |t: &TownLayout| -> Vec<bool> {
            t.palenques.iter().map(|p| p.horse.is_some()).collect()
        };
        assert_eq!(tied(&pico(5)), tied(&pico(5)));
        assert!(
            (0..8).any(|seed| tied(&pico(seed)) != tied(&pico(seed + 1))),
            "eight neighboring seeds all tie the same posts"
        );
        for seed in 0..8 {
            let t = pico(seed);
            let count = t.palenques.iter().filter(|p| p.horse.is_some()).count();
            assert!((1..=Palenque::MAX_HORSES).contains(&count), "seed {seed}");
        }
    }

    #[test]
    fn a_palenque_tethers_its_horse_alongside_without_touching_the_rail() {
        let mut seen = 0;
        for (name, t) in towns_with_names() {
            for p in t.palenques.iter().filter(|p| p.horse.is_some()) {
                let horse = p.horse.unwrap();
                let n = normal(t.buildings[p.building].front);
                assert!(!horse.footprint.intersects(&p.rail), "{name}");
                assert_eq!(horse.position, horse.footprint.center(), "{name}");
                // Along the street, on the street side of the rail.
                assert!(horse.heading.forward().dot(n).abs() < 1e-5, "{name}");
                assert!((horse.footprint.center() - p.rail.center()).dot(n) > 0.0);
                seen += 1;
            }
        }
        assert!(seen > 0);
    }

    #[test]
    fn rails_and_horses_of_the_palenques_are_obstacles() {
        assert!(
            towns_with_names()
                .iter()
                .any(|(_, t)| !t.palenques.is_empty())
        );
        for (name, t) in towns_with_names() {
            let obstacles = t.obstacles();
            let horses = t.palenques.iter().filter(|p| p.horse.is_some()).count();
            let furniture: usize = t
                .plaza_furniture()
                .iter()
                .map(|f| f.obstacles().count())
                .sum();
            assert_eq!(
                obstacles.len(),
                t.buildings
                    .iter()
                    .filter(|b| b.kind != BuildingKind::Platform)
                    .count()
                    + t.trees.len()
                    + t.shade_trees.len()
                    + 2
                    + t.palenques.len()
                    + horses
                    + t.sign_obstacles().len()
                    + furniture,
                "{name}"
            );
            for p in &t.palenques {
                assert!(p.obstacles().all(|o| obstacles.contains(&o)), "{name}");
            }
        }
    }

    #[test]
    fn palenques_stand_clear_of_everything_else() {
        let placed: usize = towns_with_names()
            .iter()
            .map(|(_, t)| t.palenques.len())
            .sum();
        assert!(placed > 40, "{placed} palenques in the sample towns");
        for (name, t) in towns_with_names() {
            let walks: Vec<Aabb> = t.sidewalks().iter().map(|s| s.area).collect();
            let obstacles = t.obstacles();
            for (i, p) in t.palenques.iter().enumerate() {
                let spots: Vec<Aabb> = p.obstacles().collect();
                // On a street, off the walkers' lane down its middle (the
                // station's, on the yard in front of it).
                if t.buildings[p.building].kind == BuildingKind::Station {
                    assert!(spots.iter().all(|o| t.extent().contains(o)), "{name}");
                } else {
                    let street = t
                        .streets
                        .iter()
                        .find(|s| spots.iter().all(|o| s.contains(o)))
                        .unwrap_or_else(|| panic!("{name}: palenque {i} leaves the street"));
                    let across = if street.size().x > street.size().y {
                        Vec2::new(0.0, 1.0)
                    } else {
                        Vec2::new(1.0, 0.0)
                    };
                    let middle = street.center().dot(across);
                    for o in &spots {
                        let (lo, hi) = (o.min.dot(across) - middle, o.max.dot(across) - middle);
                        assert!(
                            lo * hi > 0.0 && lo.abs().min(hi.abs()) >= Palenque::LANE,
                            "{name}: palenque {i} in the walkers' lane"
                        );
                    }
                }
                for o in &spots {
                    // Off the sidewalks.
                    assert!(walks.iter().all(|w| !overlap(w, o)), "{name}: {i}");
                    // And off every other obstacle, with room to walk round.
                    let reach = o.expanded(Palenque::CLEARANCE - 0.01);
                    for other in &obstacles {
                        if spots.contains(other) {
                            continue;
                        }
                        assert!(!overlap(&reach, other), "{name}: palenque {i} by {other:?}");
                    }
                }
                // Clear of the doors, the plazas, the line and the street crossings.
                for (k, other) in t.buildings.iter().enumerate() {
                    if other.kind == BuildingKind::Platform {
                        continue;
                    }
                    let m = normal(other.front);
                    let h = other.footprint.size() * 0.5;
                    let facade = other.footprint.center() + m * (m.x.abs() * h.x + m.y.abs() * h.y);
                    let side = Vec2::new(-m.y, m.x);
                    let (a, z) = (facade + side * 1.8, facade + m * 4.0 - side * 1.8);
                    let door = Aabb::new(
                        Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                        Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                    );
                    assert!(spots.iter().all(|o| !overlap(o, &door)), "{name}: {k} {i}");
                }
                for z in t.plazas().chain(t.floors().iter().map(|f| f.footprint)) {
                    assert!(spots.iter().all(|o| !overlap(o, &z)), "{name}: {i}");
                }
                // The station itself stands in a through line's band.
                let band = t
                    .rail_band()
                    .filter(|_| t.buildings[p.building].kind != BuildingKind::Station);
                for z in t.rail_corridor().into_iter().chain(band) {
                    assert!(spots.iter().all(|o| !overlap(o, &z)), "{name}: {i}");
                }
                for (x, y) in t
                    .streets
                    .iter()
                    .filter(|s| s.size().y > s.size().x)
                    .flat_map(|x| {
                        t.streets
                            .iter()
                            .filter(|s| s.size().x > s.size().y)
                            .map(move |y| (x, y))
                    })
                    .filter(|(x, y)| overlap(x, y))
                {
                    let crossing =
                        Aabb::new(Vec2::new(x.min.x, y.min.y), Vec2::new(x.max.x, y.max.y))
                            .expanded(Palenque::CROSSING_MARGIN - 0.01);
                    assert!(spots.iter().all(|o| !overlap(o, &crossing)), "{name}: {i}");
                }
            }
            // And apart from each other.
            for (i, a) in t.palenques.iter().enumerate() {
                for b in &t.palenques[i + 1..] {
                    assert!(
                        a.obstacles()
                            .all(|x| b.obstacles().all(|y| !overlap(&x, &y)))
                    );
                }
            }
        }
    }

    #[test]
    fn a_palenque_spot_needs_a_street_room_to_pass_and_free_ground() {
        let t = TownLayout::castex_inspired();
        let post = t
            .palenques
            .iter()
            .find(|p| t.buildings[p.building].kind == BuildingKind::Church)
            .unwrap();
        let spot = post.rail;
        let kind = BuildingKind::Church;
        let beside = |gap: f32| {
            let size = spot.size();
            let shift = Vec2::new(size.x + gap, 0.0);
            Aabb::new(spot.min + shift, spot.max + shift)
        };
        assert!(t.palenque_fits(kind, &spot, &[], &[]));
        // Another obstacle within the clearance blocks it, just past it does not.
        let near = beside(Palenque::CLEARANCE - 0.1);
        let far = beside(Palenque::CLEARANCE + 0.1);
        assert!(!t.palenque_fits(kind, &spot, &[near], &[]));
        assert!(t.palenque_fits(kind, &spot, &[far], &[]));
        // A keep-out area blocks it only when it overlaps.
        assert!(!t.palenque_fits(kind, &spot, &[], &[spot.expanded(-0.01)]));
        assert!(t.palenque_fits(kind, &spot, &[], &[beside(0.0)]));
        // Off every street, or in the lane down the middle, it does not fit.
        let off = Aabb::new(
            spot.min + Vec2::new(0.0, 15.0),
            spot.max + Vec2::new(0.0, 15.0),
        );
        assert!(!t.palenque_fits(kind, &off, &[], &[]));
        let street = t.streets.iter().find(|s| s.contains(&spot)).unwrap();
        let lane = Aabb::from_center_size(street.center(), spot.size());
        assert!(!t.palenque_fits(kind, &lane, &[], &[]));
    }

    fn pico_with_streets(street_width: f32) -> TownLayout {
        TownLayout::generate(&TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            street_width,
            ..TownConfig::castex()
        })
        .unwrap()
    }

    #[test]
    fn sidewalks_are_three_and_a_half_meters_and_the_galeria_stays_over_them() {
        assert!((Sidewalk::WIDTH - 3.5).abs() < 1e-6);
        // The store's galería is as deep as the street allows, never past the walk.
        for (street, veranda) in [
            (10.0, 2.3),
            (11.0, 2.8),
            (12.0, 3.3),
            (14.0, 3.5),
            (16.0, 3.5),
        ] {
            let t = pico_with_streets(street);
            assert!(
                (t.hitching_post.veranda - veranda).abs() < 1e-4,
                "{street} m street: veranda {}",
                t.hitching_post.veranda
            );
            for walk in t.sidewalks() {
                let a = walk.area.size();
                assert!((a.x.min(a.y) - 3.5).abs() < 1e-3, "{street} m street");
            }
        }
    }

    #[test]
    fn a_tied_horse_needs_a_twelve_meter_street_and_the_rails_still_fit_on_narrow_ones() {
        for street in [10.0, 11.0, 12.0, 14.0] {
            let t = pico_with_streets(street);
            assert!(
                t.palenques.len() >= 4,
                "{street} m: {} posts",
                t.palenques.len()
            );
            // The station's post stands on its yard, not on a street.
            let on_streets: Vec<&Palenque> = t
                .palenques
                .iter()
                .filter(|p| t.buildings[p.building].kind != BuildingKind::Station)
                .collect();
            let horses: Vec<_> = on_streets.iter().filter_map(|p| p.horse).collect();
            if street < 12.0 {
                assert!(
                    horses.is_empty(),
                    "{street} m street has no room for a horse"
                );
            } else {
                assert!(!horses.is_empty(), "{street} m street should tie a horse");
            }
            // Past the 3.5 m walk, with the lane down the middle (1 m each way) free.
            for p in on_streets {
                let street_strip = t
                    .streets
                    .iter()
                    .find(|s| s.contains(&p.rail))
                    .unwrap_or_else(|| panic!("{street} m: rail off the street"));
                let across = if street_strip.size().x > street_strip.size().y {
                    Vec2::new(0.0, 1.0)
                } else {
                    Vec2::new(1.0, 0.0)
                };
                let middle = street_strip.center().dot(across);
                for o in p.obstacles() {
                    let near = (o.min.dot(across) - middle)
                        .abs()
                        .min((o.max.dot(across) - middle).abs());
                    assert!(near >= 1.0 - 1e-4, "{street} m: {near} m from the middle");
                    assert!(
                        t.sidewalks().iter().all(|w| !overlap(&w.area, &o)),
                        "{street} m: on the walk"
                    );
                }
            }
        }
    }

    fn trees_of(t: &TownLayout, kind: ShadeKind) -> Vec<ShadeTree> {
        t.shade_trees
            .iter()
            .copied()
            .filter(|s| s.kind == kind)
            .collect()
    }

    #[test]
    fn only_cities_get_street_trees_and_groves_and_only_towns_front_trees() {
        let (mut cities, mut towns) = (0, 0);
        for (name, t) in towns_with_names() {
            let (pits, groves) = (trees_of(&t, ShadeKind::Pit), trees_of(&t, ShadeKind::Grove));
            let fronts = trees_of(&t, ShadeKind::Front);
            if t.size == TownSize::Small {
                towns += 1;
                assert!(pits.is_empty() && groves.is_empty(), "{name}");
                assert!(!fronts.is_empty(), "{name}: no tree in front of any house");
            } else {
                cities += 1;
                assert!(fronts.is_empty(), "{name}");
                assert!(pits.len() > 100 && groves.len() >= 4, "{name}");
            }
            // Yards everywhere, but only a few in a village.
            assert!(!trees_of(&t, ShadeKind::Yard).is_empty(), "{name}");
        }
        assert!(cities >= 4 && towns >= 6);
    }

    #[test]
    fn shade_trees_are_seeded_and_the_seed_moves_them() {
        let pico = |seed| {
            TownLayout::generate(&TownConfig {
                name: "Pico".into(),
                blocks: (8, 8),
                seed,
                ..TownConfig::castex()
            })
            .unwrap()
        };
        assert_eq!(pico(7).shade_trees, pico(7).shade_trees);
        // The same sidewalks, a different seed: each row's first tree starts
        // elsewhere along it.
        let first = |t: &TownLayout, row: u32| {
            t.shade_trees
                .iter()
                .find(|s| s.kind == ShadeKind::Pit && s.row == row)
                .map(|s| s.at.x + s.at.y)
        };
        let (a, b) = (pico(3), pico(4));
        assert_eq!(a.sidewalks(), b.sidewalks());
        let rows = a.sidewalks().len() as u32;
        let moved = (0..rows)
            .filter(|&r| match (first(&a, r), first(&b, r)) {
                (Some(x), Some(y)) => (x - y).abs() > 0.5,
                _ => false,
            })
            .count();
        assert!(moved * 2 > rows as usize, "{moved} of {rows} rows moved");
    }

    #[test]
    fn street_trees_stand_in_their_pits_forty_centimeters_from_the_curb() {
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size != TownSize::Small)
        {
            for tree in trees_of(&t, ShadeKind::Pit) {
                let walk = &t.sidewalks()[tree.row as usize];
                let curb = walk.curbs[0];
                let n = normal(curb);
                // The strip's outer (curb) edge along its normal.
                let edge = match curb {
                    Side::North => walk.area.max.y,
                    Side::South => walk.area.min.y,
                    Side::East => walk.area.max.x,
                    Side::West => walk.area.min.x,
                };
                let on_edge = if n.x.abs() > 0.5 {
                    Vec2::new(edge, tree.at.y)
                } else {
                    Vec2::new(tree.at.x, edge)
                };
                let gap = on_edge.distance(tree.at);
                // A 0.8 m cazuela, its edge 0.4 m in from the curb.
                assert!((gap - 0.8).abs() < 1e-3, "{name}: pit center {gap} m in");
                assert!(walk.area.contains_point(tree.at), "{name}");
                assert_eq!(tree.toward, n, "{name}: leans over the street");
                assert!((tree.crown - 2.3).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn a_street_row_keeps_fifteen_to_eighteen_meters_between_its_trees() {
        let mut gaps = Vec::new();
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size != TownSize::Small)
        {
            for row in 0..t.sidewalks().len() {
                let mut at: Vec<Vec2> = trees_of(&t, ShadeKind::Pit)
                    .into_iter()
                    .filter(|s| s.row as usize == row)
                    .map(|s| s.at)
                    .collect();
                at.sort_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)));
                for pair in at.windows(2) {
                    let gap = pair[0].distance(pair[1]);
                    assert!(gap >= 14.8 - 1e-3, "{name}: row {row}, {gap} m apart");
                    gaps.push(gap);
                }
            }
        }
        assert!(gaps.len() > 250, "{} gaps", gaps.len());
        // Most of them are the planned 14.8 to 18.4 m; the rest skip a door
        // or a lamp.
        let planned = gaps.iter().filter(|&&g| g < 24.0).count();
        assert!(planned * 10 > gaps.len() * 8, "{planned} of {}", gaps.len());
    }

    #[test]
    fn the_sample_cities_got_a_tenth_fewer_street_trees_and_the_towns_half_as_many_front_ones_again()
     {
        // Counts before the user's second playtest (2026-10-26), on the
        // 2026-10-26 street widths: 2247 street trees in the cities of these
        // samples; before the first playtest, 119 front trees in the towns
        // (on the old widths); 189 front trees before the 15% more.
        let (mut pits, mut fronts) = (0, 0);
        for (_, t) in towns_with_names() {
            pits += trees_of(&t, ShadeKind::Pit).len();
            fronts += trees_of(&t, ShadeKind::Front).len();
        }
        // Then 15% more (2026-10-27): 2039 before, so 1.12 to 1.18 times.
        assert!(
            (2039 * 112 / 100..=2039 * 118 / 100).contains(&pits),
            "{pits} street trees"
        );
        assert!(
            (189 * 112 / 100..=189 * 123 / 100).contains(&fronts),
            "{fronts} front trees"
        );
    }

    /// Distance from `p` to the nearest of `boxes`.
    fn gap_to(boxes: impl IntoIterator<Item = Aabb>, p: Vec2) -> f32 {
        boxes
            .into_iter()
            .map(|b| b.closest_point(p).distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn shade_trees_keep_a_crown_off_every_building() {
        let mut checked = 0;
        for (name, t) in towns_with_names() {
            let walls: Vec<Aabb> = t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect();
            for tree in &t.shade_trees {
                let gap = gap_to(walls.iter().copied(), tree.at);
                // Literal gaps: a pit tree stands 2.7 m off its facade (3.5 m
                // walk less the pit), a front one 2.2 m, a yard one 3.6 m.
                let least = match tree.kind {
                    ShadeKind::Pit => 2.65,
                    ShadeKind::Front => 2.15,
                    ShadeKind::Yard => 3.5,
                    ShadeKind::Grove => 4.0,
                };
                assert!(gap >= least, "{name}: {:?} {gap} m off a wall", tree.kind);
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    #[test]
    fn shade_trees_stand_clear_of_doors_galerias_crossings_palenques_and_each_other() {
        for (name, t) in towns_with_names() {
            let trunks: Vec<Vec2> = t
                .trees
                .iter()
                .copied()
                .chain(t.shade_trees.iter().map(|s| s.at))
                .collect();
            for (k, tree) in t.shade_trees.iter().enumerate() {
                for (j, other) in trunks.iter().enumerate() {
                    if j != t.trees.len() + k {
                        assert!(
                            tree.at.distance(*other) >= 3.99,
                            "{name}: trunks {k} and {j}"
                        );
                    }
                }
                // Past the corner lamps' crossings.
                for (x, y) in t
                    .streets
                    .iter()
                    .filter(|s| s.size().y > s.size().x)
                    .flat_map(|x| {
                        t.streets
                            .iter()
                            .filter(|y| y.size().x > y.size().y)
                            .map(move |y| (x, y))
                    })
                    .filter(|(x, y)| x.intersects(y))
                {
                    let crossing =
                        Aabb::new(Vec2::new(x.min.x, y.min.y), Vec2::new(x.max.x, y.max.y));
                    assert!(
                        crossing.closest_point(tree.at).distance(tree.at) >= 1.9
                            || !on_street(&t, tree.at),
                        "{name}: tree {k} at a crossing"
                    );
                }
                for b in t
                    .buildings
                    .iter()
                    .filter(|b| b.kind != BuildingKind::Platform)
                {
                    let m = normal(b.front);
                    let h = b.footprint.size() * 0.5;
                    let facade = b.footprint.center() + m * (m.x.abs() * h.x + m.y.abs() * h.y);
                    let side = Vec2::new(-m.y, m.x);
                    let (a, z) = (facade + side * 2.5, facade + m * 4.0 - side * 2.5);
                    let door = Aabb::new(
                        Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                        Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                    );
                    assert!(!door.contains_point(tree.at), "{name}: tree {k} at a door");
                    if b.kind == BuildingKind::GeneralStore {
                        let wide = h.dot(Vec2::new(side.x.abs(), side.y.abs())) + 1.4;
                        let (a, z) = (facade + side * wide, facade + m * 3.0 - side * wide);
                        let gallery = Aabb::new(
                            Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                            Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                        );
                        assert!(
                            !gallery.contains_point(tree.at),
                            "{name}: tree {k} at a galería"
                        );
                    }
                }
                // Off every palenque, its horse, the store's post and every sign.
                let posts = t
                    .palenques
                    .iter()
                    .flat_map(Palenque::obstacles)
                    .chain([t.hitching_post.rail, t.hitching_post.horse.footprint])
                    .chain(t.sign_obstacles());
                assert!(gap_to(posts, tree.at) >= 1.3, "{name}: tree {k} by a post");
            }
        }
    }

    #[test]
    fn groves_stand_between_the_plaza_paths_and_yard_trees_beside_the_landmarks() {
        let mut groves = 0;
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size != TownSize::Small)
        {
            for tree in trees_of(&t, ShadeKind::Grove) {
                let plaza = t
                    .plazas()
                    .find(|p| p.contains_point(tree.at))
                    .unwrap_or_else(|| panic!("{name}: a grove tree off the plazas"));
                for keep_off in plaza_paths(plaza)
                    .into_iter()
                    .chain([plaza_monument(plaza)])
                {
                    assert!(
                        keep_off.closest_point(tree.at).distance(tree.at) >= 1.8,
                        "{name}: a grove tree on a path"
                    );
                }
                // In the middle of the green, not only along its edge.
                groves += usize::from(plaza.expanded(-10.0).contains_point(tree.at));
            }
            // A tree within 8 m of each church, comisaría and station (a
            // through line's station stands in its yard band, with no room).
            let yards = trees_of(&t, ShadeKind::Yard);
            for b in t.buildings.iter().filter(|b| {
                matches!(b.kind, BuildingKind::Church | BuildingKind::PoliceStation)
                    || (b.kind == BuildingKind::Station && t.rail_band().is_none())
            }) {
                let near = yards
                    .iter()
                    .any(|y| b.footprint.closest_point(y.at).distance(y.at) <= 8.0);
                assert!(near, "{name}: no tree by the {:?}", b.kind);
            }
        }
        assert!(
            groves > 20,
            "{groves} grove trees 10 m or more in from a plaza's edge"
        );
    }

    #[test]
    fn front_trees_are_few_and_far_apart_on_the_dirt_beside_the_lane() {
        let mut seen = 0;
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size == TownSize::Small)
        {
            let fronts = trees_of(&t, ShadeKind::Front);
            let houses = t
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .count();
            // Sparse: no more than one house in two, never two within 13 m,
            // never more than two others within 30 m: no continuous row.
            assert!(
                fronts.len() * 2 <= houses,
                "{name}: {} of {houses}",
                fronts.len()
            );
            for (k, a) in fronts.iter().enumerate() {
                let run = fronts.iter().filter(|b| a.at.distance(b.at) < 30.0).count() - 1;
                assert!(run <= 2, "{name}: a row of {} trees", run + 1);
                for b in &fronts[k + 1..] {
                    assert!(a.at.distance(b.at) >= 12.9, "{name}: trees too close");
                }
                // On a street, off its walkers' lane, on a beaten-earth walk (no
                // cazuela) and at least a meter in from its curb.
                assert!(on_street(&t, a.at), "{name}");
                let on: Vec<&Sidewalk> = t
                    .sidewalks()
                    .iter()
                    .filter(|w| w.area.contains_point(a.at))
                    .collect();
                assert!(!on.is_empty(), "{name}: a front tree off the walks");
                assert!(on.iter().all(|w| w.walk == Walk::Earth), "{name}: on brick");
                // In from the curb along the strip it stands on (a tree by a
                // corner also lies on the cross strip, whose curb is near).
                let curb = on
                    .iter()
                    .map(|w| match w.curbs[0] {
                        Side::North => w.area.max.y - a.at.y,
                        Side::South => a.at.y - w.area.min.y,
                        Side::East => w.area.max.x - a.at.x,
                        Side::West => a.at.x - w.area.min.x,
                    })
                    .fold(f32::MIN, f32::max);
                assert!(curb >= 1.0, "{name}: {curb} m from the curb");
                assert!(a.crown <= 1.9 + 1e-6 && a.toward.length() > 0.99, "{name}");
                seen += 1;
            }
        }
        assert!(seen > 50, "{seen}");
    }

    #[test]
    fn yards_have_thirty_percent_more_trees_and_shrubs_and_flower_beds_besides() {
        // Yard trees before the playtest (2026-10-26): 99 in the sample
        // towns, 648 in the sample cities.
        let (mut small, mut city) = (0, 0);
        let (mut shrubs, mut flowers) = (0, 0);
        for (_, t) in towns_with_names() {
            let yards = trees_of(&t, ShadeKind::Yard).len();
            if t.size == TownSize::Small {
                small += yards;
            } else {
                city += yards;
            }
            shrubs += t
                .yard_plants
                .iter()
                .filter(|p| p.kind == PlantKind::Shrub)
                .count();
            flowers += t
                .yard_plants
                .iter()
                .filter(|p| p.kind == PlantKind::Flowers)
                .count();
        }

        // After the 15% more (2026-10-27): the sample towns 178, the cities
        // 968.
        assert!(
            (165..=190).contains(&small),
            "{small} yard trees in the towns"
        );
        assert!(
            (910..=1030).contains(&city),
            "{city} yard trees in the cities"
        );
        // Plants on top: at least a third as many as the trees.
        assert!(
            shrubs * 3 > (small + city) && flowers * 3 > (small + city) / 2,
            "{shrubs} {flowers}"
        );
    }

    #[test]
    fn yard_plants_stay_behind_the_houses_off_everything_else() {
        let mut checked = 0;
        for (name, t) in towns_with_names() {
            let walls: Vec<Aabb> = t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect();
            let obstacles = t.obstacles();
            for (k, plant) in t.yard_plants.iter().enumerate() {
                let spot = plant.area();
                // A shrub is 1.2 m across, a flower bed 1.4 m.
                let side = spot.size().x;
                assert!(
                    (side - 1.2).abs() < 1e-3 || (side - 1.4).abs() < 1e-3,
                    "{name}"
                );
                assert!(
                    walls.iter().all(|w| !overlap(w, &spot)),
                    "{name}: plant {k} in a building"
                );
                assert!(
                    obstacles.iter().all(|o| !overlap(o, &spot.expanded(0.3))),
                    "{name}: plant {k} by an obstacle"
                );
                assert!(
                    t.streets.iter().all(|s| !overlap(s, &spot)),
                    "{name}: plant {k} on a street"
                );
                assert!(t.plazas().all(|p| !overlap(&p, &spot)), "{name}");
                assert!(
                    t.sidewalks().iter().all(|w| !overlap(&w.area, &spot)),
                    "{name}"
                );
                // Off the ground in front of every door (1.8 m each side, 4 m out).
                for b in t
                    .buildings
                    .iter()
                    .filter(|b| b.kind != BuildingKind::Platform)
                {
                    let m = normal(b.front);
                    let h = b.footprint.size() * 0.5;
                    let facade = b.footprint.center() + m * (m.x.abs() * h.x + m.y.abs() * h.y);
                    let across = Vec2::new(-m.y, m.x);
                    let (a, z) = (facade + across * 1.8, facade + m * 4.0 - across * 1.8);
                    let door = Aabb::new(
                        Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                        Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                    );
                    assert!(!overlap(&door, &spot), "{name}: plant {k} at a door");
                }
                for other in &t.yard_plants[k + 1..] {
                    assert!(
                        plant.at.distance(other.at) >= 1.59,
                        "{name}: plants too close"
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    #[test]
    fn a_very_big_town_has_over_a_third_fewer_plaza_trees_taken_from_the_groves() {
        // Before the thinning (user, 2026-10-26) the very big samples had
        // 90, 75 and 90 plaza trees (the rings' 36 and 54, 39 and 54 groves);
        // a fifth fewer after the first playtest (72, 60, 72), 36 in 100
        // fewer after the second (58, 48, 58), all of it from the groves;
        // then 15% more trees in towns (2026-10-27): the rings grew to 44
        // and the cut, a share of the whole, with them (71, 64, 73).
        let very_big: Vec<(String, TownLayout)> = towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size == TownSize::VeryBig)
            .collect();
        assert_eq!(very_big.len(), 3);
        for ((name, t), total) in very_big.iter().zip([71, 64, 73]) {
            let groves = trees_of(t, ShadeKind::Grove).len();
            assert_eq!(t.trees.len(), 44, "{name}: the rings stay whole");
            assert_eq!(t.trees.len() + groves, total, "{name}");
            // Still groves in the middle, not just at the edge.
            let plaza = t.plaza.expanded(-12.0);
            let inside = trees_of(t, ShadeKind::Grove)
                .iter()
                .filter(|g| plaza.contains_point(g.at))
                .count();
            assert!(inside >= 4, "{name}: {inside} grove trees in the middle");
        }
        // The big towns' groves with the denser grid (9, 7, 11, 17 before).
        let big: Vec<(String, TownLayout)> = towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size == TownSize::Big)
            .collect();
        assert_eq!(big.len(), 4);
        for ((name, t), groves) in big.iter().zip([10, 12, 11, 19]) {
            assert_eq!(trees_of(t, ShadeKind::Grove).len(), groves, "{name}");
        }
    }

    #[test]
    fn towns_plant_fifteen_percent_more_trees() {
        // Trees of the sample towns before the extra ones (2026-10-27):
        // plaza rings 340, street trees 2039, front 189, groves 100, yards
        // 1013, 3681 in all. Whole steps and a very big town's grove cut
        // keep the parts apart from a flat 15%.
        let count = |kind| {
            sample_towns()
                .iter()
                .flat_map(|t| t.shade_trees.iter())
                .filter(|s| s.kind == kind)
                .count()
        };
        let ring: usize = sample_towns().iter().map(|t| t.trees.len()).sum();
        let total = ring
            + [
                ShadeKind::Pit,
                ShadeKind::Front,
                ShadeKind::Grove,
                ShadeKind::Yard,
            ]
            .map(count)
            .iter()
            .sum::<usize>();
        let share = total as f32 / 3681.0;
        assert!((1.12..=1.18).contains(&share), "{total} trees: {share}");
        assert!(count(ShadeKind::Pit) as f32 >= 1.12 * 2039.0);
        assert!(count(ShadeKind::Yard) as f32 >= 1.08 * 1013.0);
        assert!(ring as f32 >= 1.08 * 340.0);
    }
}
