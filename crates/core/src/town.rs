//! Settlement layouts: street grid, plaza, railway and building footprints.
//!
//! Pure data. A [`TownConfig`] describes a town (grid size, block and street
//! widths, railway side, plaza placement); [`TownLayout::generate`] turns it
//! into footprints. Render decides walls, roofs and colors per
//! [`BuildingKind`]; logic only needs footprints for collision and the
//! leveled ground.

use std::fmt;

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

/// A timber hitching rail with a horse tied alongside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitchingPost {
    pub rail: Aabb,
    pub horse: TetheredHorse,
    /// Depth of the store's veranda, in meters: the rail stands just past
    /// it, so narrow streets get a shallower veranda.
    pub veranda: f32,
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

/// Which side of the grid the railway runs along, and how far out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailwayConfig {
    pub side: Side,
    /// Distance from the grid edge (outer edge of the ring street) to the
    /// track centerline, in meters.
    pub offset: f32,
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
}

impl TownConfig {
    /// Fewest blocks along either axis.
    pub const MIN_BLOCKS: usize = 3;
    /// Smallest block that still fits the landmarks and lots, in meters.
    pub const MIN_BLOCK_SIZE: f32 = 30.0;
    /// Narrowest street that leaves room for a tied horse and passers-by.
    pub const MIN_STREET_WIDTH: f32 = 8.0;
    /// Closest the track may run to the grid: the station yard needs it.
    pub const MIN_RAILWAY_OFFSET: f32 = 24.0;

    /// Eduardo Castex-inspired town: 4 x 5 blocks of 40 m (4 east-west, 5
    /// north-south), 10 m streets, railway 24 m north of the grid. With no
    /// central block, `Auto` puts the plaza in the top row facing the
    /// station. Matches the first town of `assets/world/towns.ron`, and is
    /// the fallback when that file cannot be used.
    pub fn castex() -> Self {
        Self {
            name: "Eduardo Castex".to_string(),
            origin: Vec2::ZERO,
            blocks: (4, 5),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 7,
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
    /// The square.
    pub plaza: Aabb,
    /// Trunk positions of the plaza trees.
    pub trees: Vec<Vec2>,
    pub railway: Option<Railway>,
    pub buildings: Vec<Building>,
    /// Hitching rail in front of the general store.
    pub hitching_post: HitchingPost,
    /// The config's seed, also used for the town's crowd.
    pub seed: u32,
    grid: Aabb,
    extent: Aabb,
    arrival: (Vec2, Heading),
}

/// Trunk radius used for tree collision.
pub const TREE_TRUNK_RADIUS: f32 = 0.35;

impl TownLayout {
    /// Builds the town `config` describes, or explains why it cannot.
    pub fn generate(config: &TownConfig) -> Result<Self, TownConfigError> {
        let grid = Grid::validated(config)?;
        let plaza_block = plaza_block(config)?;
        let plaza = grid.block(plaza_block.0, plaza_block.1);
        let bounds = grid.bounds();
        let extent = grid.extent(config.railway);

        let civic = civic_blocks(&grid, plaza_block);
        let windmills = windmill_blocks(&grid, plaza_block, &civic);
        let mut buildings = Vec::new();
        for j in 0..grid.rows {
            for i in 0..grid.cols {
                if (i, j) == plaza_block {
                    continue;
                }
                let landmark = civic.iter().find(|c| c.block == (i, j));
                fill_block(
                    &grid,
                    (i, j),
                    landmark,
                    windmills.contains(&(i, j)),
                    config.seed,
                    &mut buildings,
                );
            }
        }
        let railway = config.railway.map(|r| {
            buildings.extend(station_yard(&grid, r, plaza.center()));
            railway_line(bounds, r, grid.along_half(r.side))
        });

        let store = buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .expect("every town has a general store");
        let hitching_post = hitching_post(store, veranda_depth(grid.street));
        Ok(Self {
            name: config.name.clone(),
            streets: grid.streets(),
            plaza,
            trees: plaza_trees(plaza),
            railway,
            buildings,
            hitching_post,
            seed: config.seed,
            grid: bounds,
            extent,
            arrival: arrival(&grid, plaza_block, config.railway),
        })
    }

    /// A small pampa railway town inspired by 1920s–1930s settlements of La
    /// Pampa such as Eduardo Castex, generated from [`TownConfig::castex`].
    /// It borrows the general pattern (grid, plaza, railway at the edge,
    /// general store, grain sheds, windmills) and is not a reconstruction of
    /// any real town.
    pub fn castex_inspired() -> Self {
        Self::generate(&TownConfig::castex()).expect("the built-in Castex config is valid")
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

    /// Static collision footprints: every building, every tree trunk, the
    /// hitching rail with its horse, and free-standing signs.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        self.buildings
            .iter()
            .map(|b| b.footprint)
            .chain(self.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)))
            .chain([self.hitching_post.rail, self.hitching_post.horse.footprint])
            .chain(self.sign_obstacles())
            .collect()
    }

    /// Where a rider arriving from the open pampa starts: on the main road
    /// outside the grid, on the side away from the railway, facing into town.
    pub fn arrival(&self) -> (Vec2, Heading) {
        self.arrival
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
        Ok(Self {
            origin: config.origin,
            cols,
            rows,
            block: config.block_size,
            street: config.street_width,
        })
    }

    /// Half the grid's size, ring street included.
    fn half(&self) -> Vec2 {
        let span = |n: usize| n as f32 * self.block + (n + 1) as f32 * self.street;
        Vec2::new(span(self.cols), span(self.rows)) * 0.5
    }

    fn bounds(&self) -> Aabb {
        Aabb::from_center_size(self.origin, self.half() * 2.0)
    }

    /// Lower `x` of north-south street `k` (0 = the west ring street).
    fn lane_x(&self, k: usize) -> f32 {
        self.origin.x - self.half().x + k as f32 * (self.block + self.street)
    }

    /// Lower `y` of east-west street `k` (0 = the south ring street).
    fn lane_y(&self, k: usize) -> f32 {
        self.origin.y - self.half().y + k as f32 * (self.block + self.street)
    }

    /// Footprint of block `(i, j)`, counted from the south-west corner.
    fn block(&self, i: usize, j: usize) -> Aabb {
        let min = Vec2::new(self.lane_x(i) + self.street, self.lane_y(j) + self.street);
        Aabb::new(min, min + Vec2::new(self.block, self.block))
    }

    /// Every north-south street, west to east, then every east-west street,
    /// south to north.
    fn streets(&self) -> Vec<Aabb> {
        let b = self.bounds();
        let north_south = (0..=self.cols).map(|k| {
            let x = self.lane_x(k);
            Aabb::new(Vec2::new(x, b.min.y), Vec2::new(x + self.street, b.max.y))
        });
        let east_west = (0..=self.rows).map(|k| {
            let y = self.lane_y(k);
            Aabb::new(Vec2::new(b.min.x, y), Vec2::new(b.max.x, y + self.street))
        });
        north_south.chain(east_west).collect()
    }

    /// The grid plus the station yard and railway corridor beside it.
    fn extent(&self, railway: Option<RailwayConfig>) -> Aabb {
        let grid = self.bounds();
        let Some(r) = railway else {
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
fn plaza_block(config: &TownConfig) -> Result<(usize, usize), TownConfigError> {
    let (cols, rows) = config.blocks;
    let central = cols % 2 == 1 && rows % 2 == 1;
    let facing = |side: Side| match side {
        Side::North => (middle(cols), rows - 1),
        Side::South => (middle(cols), 0),
        Side::East => (cols - 1, middle(rows)),
        Side::West => (0, middle(rows)),
    };
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

/// Places the general store, church and police station, in that order.
///
/// Each takes the first free block next to the plaza in its preference
/// order and faces the plaza across the street. When every neighbor is
/// taken (a plaza in a corner), it takes the nearest free block instead
/// (fewest blocks away, then lowest column, then lowest row) and faces the
/// plaza along the axis with the larger offset.
fn civic_blocks(grid: &Grid, plaza: (usize, usize)) -> Vec<Landmark> {
    let (pi, pj) = (plaza.0 as isize, plaza.1 as isize);
    let inside = |i: isize, j: isize| {
        (0..grid.cols as isize).contains(&i) && (0..grid.rows as isize).contains(&j)
    };
    let mut placed: Vec<Landmark> = Vec::new();
    for (kind, preferences) in LANDMARK_PREFERENCES {
        let free = |b: (usize, usize)| b != plaza && placed.iter().all(|l| l.block != b);
        let neighbor = preferences.into_iter().find_map(|toward| {
            let n = crate::places::side_normal(toward);
            let (i, j) = (pi + n.x as isize, pj + n.y as isize);
            let block = (i as usize, j as usize);
            (inside(i, j) && free(block)).then_some(Landmark {
                kind,
                block,
                front: toward.opposite(),
            })
        });
        let landmark = neighbor.unwrap_or_else(|| {
            let mut blocks: Vec<(usize, usize)> = (0..grid.cols)
                .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
                .filter(|&b| free(b))
                .collect();
            blocks.sort_by_key(|&(i, j)| (i.abs_diff(plaza.0) + j.abs_diff(plaza.1), i, j));
            let block = blocks[0];
            let (di, dj) = (pi - block.0 as isize, pj - block.1 as isize);
            let front = if di.abs() >= dj.abs() {
                if di > 0 { Side::East } else { Side::West }
            } else if dj > 0 {
                Side::North
            } else {
                Side::South
            };
            Landmark { kind, block, front }
        });
        placed.push(landmark);
    }
    placed
}

/// Blocks with a windmill and its water tank in the back yard: the
/// south-west corner, the east column one row below the top, and the second
/// column of the top row, skipping the plaza and landmark blocks.
fn windmill_blocks(grid: &Grid, plaza: (usize, usize), civic: &[Landmark]) -> Vec<(usize, usize)> {
    [(0, 0), (grid.cols - 1, grid.rows - 2), (1, grid.rows - 1)]
        .into_iter()
        .filter(|&b| b != plaza && civic.iter().all(|l| l.block != b))
        .collect()
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
/// a 40 m block and scales with the block.
fn fill_block(
    grid: &Grid,
    (i, j): (usize, usize),
    landmark: Option<&Landmark>,
    windmill: bool,
    seed: u32,
    out: &mut Vec<Building>,
) {
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
    let density = if outskirts { 0.45 } else { 0.75 };
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
fn station_yard(grid: &Grid, railway: RailwayConfig, plaza: Vec2) -> Vec<Building> {
    let bounds = grid.bounds();
    let side = railway.side;
    let r = railway.offset;
    let half = grid.along_half(side);
    let along = match side {
        Side::North | Side::South => Vec2::new(1.0, 0.0),
        Side::East | Side::West => Vec2::new(0.0, 1.0),
    };
    let wanted = (plaza - bounds.center()).dot(along);
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
            yard(-PLATFORM_HALF, PLATFORM_HALF, r - 11.0, r - 4.0),
            0.9,
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

/// The track: parallel to the grid's `side` at `offset`, running
/// [`TownLayout::RAIL_RUN_OUT`] past either end of the grid.
fn railway_line(grid: Aabb, r: RailwayConfig, along_half: f32) -> Railway {
    // A zero-size box on the track centerline.
    let line = yard_box(grid, r.side, (0.0, 0.0), (r.offset, r.offset));
    let reach = along_half + TownLayout::RAIL_RUN_OUT;
    let c = grid.center();
    let (start, end) = match r.side {
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
    (street / 2.0 - 2.7).clamp(1.2, 3.0)
}

/// A rail on the street side of `store`, past its `veranda`, with a horse
/// tied alongside it facing along the street.
fn hitching_post(store: &Building, veranda: f32) -> HitchingPost {
    // Outward facade normal and the direction along the street.
    let normal = crate::places::side_normal(store.front);
    let along = Vec2::new(normal.y.abs(), normal.x.abs());
    let half = store.footprint.size() * 0.5;
    let face =
        store.footprint.center() + normal * (normal.x.abs() * half.x + normal.y.abs() * half.y);
    // Axis-aligned box of `length` along the street and `width` across it.
    let boxed = |center: Vec2, length: f32, width: f32| {
        let size = along * length + normal * width;
        Aabb::from_center_size(center, Vec2::new(size.x.abs(), size.y.abs()))
    };
    // Just past the veranda.
    let rail_offset = veranda + 0.2;
    let (rail_width, horse_width, horse_length) = (0.2, 1.0, 2.4);
    let rail = boxed(face + normal * rail_offset, 2.8, rail_width);
    let horse_center = face + normal * (rail_offset + rail_width / 2.0 + 0.1 + horse_width / 2.0);
    HitchingPost {
        rail,
        horse: TetheredHorse {
            position: horse_center,
            heading: Heading::from_radians(along.y.atan2(along.x)),
            footprint: boxed(horse_center, horse_length, horse_width),
        },
        veranda,
    }
}

/// Trees in a ring just inside the plaza edge, leaving the center open.
fn plaza_trees(plaza: Aabb) -> Vec<Vec2> {
    let inset = 3.0;
    let (lo, hi) = (
        plaza.min + Vec2::new(inset, inset),
        plaza.max - Vec2::new(inset, inset),
    );
    let steps = 4;
    let mut trees = Vec::new();
    for k in 0..=steps {
        let t = k as f32 / steps as f32;
        let x = lo.x + (hi.x - lo.x) * t;
        let y = lo.y + (hi.y - lo.y) * t;
        trees.push(Vec2::new(x, lo.y));
        trees.push(Vec2::new(x, hi.y));
        if k != 0 && k != steps {
            trees.push(Vec2::new(lo.x, y));
            trees.push(Vec2::new(hi.x, y));
        }
    }
    trees
}

/// The main road in: the north-south street just east of the plaza, entered
/// from the south (from the north when the railway runs south of town).
fn arrival(grid: &Grid, plaza: (usize, usize), railway: Option<RailwayConfig>) -> (Vec2, Heading) {
    let bounds = grid.bounds();
    let road_x = grid.lane_x(plaza.0 + 1) + grid.street / 2.0;
    if railway.is_some_and(|r| r.side == Side::South) {
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
    /// railway on every side or missing, and every plaza placement.
    pub(crate) fn sample_configs() -> Vec<TownConfig> {
        let base = TownConfig::castex();
        let rail = |side, offset| Some(RailwayConfig { side, offset });
        vec![
            base.clone(),
            five_by_five(),
            TownConfig {
                name: "Tres".into(),
                blocks: (3, 3),
                block_size: 32.0,
                street_width: 8.0,
                seed: 1,
                ..base.clone()
            },
            TownConfig {
                name: "Seis por cuatro".into(),
                origin: Vec2::new(-20.0, 15.0),
                blocks: (6, 4),
                block_size: 50.0,
                street_width: 14.0,
                railway: rail(Side::South, 30.0),
                seed: 99,
                ..base.clone()
            },
            TownConfig {
                name: "Este".into(),
                blocks: (4, 4),
                block_size: 36.0,
                street_width: 9.0,
                railway: rail(Side::East, 26.0),
                ..base.clone()
            },
            TownConfig {
                name: "Oeste".into(),
                blocks: (3, 5),
                block_size: 45.0,
                street_width: 11.0,
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
                street_width: 10.0,
                plaza: PlazaPlacement::FacingStation,
                ..base
            },
        ]
    }

    /// A 5 x 5 grid of 40 m blocks and 12 m streets with a central plaza.
    fn five_by_five() -> TownConfig {
        TownConfig {
            name: "Cinco por cinco".into(),
            blocks: (5, 5),
            street_width: 12.0,
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
            for kind in [
                BuildingKind::GeneralStore,
                BuildingKind::PoliceStation,
                BuildingKind::Church,
            ] {
                assert_eq!(count(&t, kind), 1, "{}: {kind:?}", c.name);
            }
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
            assert_eq!(t.streets.len(), cols + 1 + rows + 1, "{}", c.name);
            let size = t.grid().size();
            let expected = Vec2::new(
                cols as f32 * b + (cols + 1) as f32 * w,
                rows as f32 * b + (rows + 1) as f32 * w,
            );
            assert!(size.distance(expected) < 1e-3, "{}: {size:?}", c.name);
            assert!(t.grid().center().distance(c.origin) < 1e-3, "{}", c.name);
            assert!((t.plaza.size().x - b).abs() < 1e-3 && (t.plaza.size().y - b).abs() < 1e-3);
            for street in &t.streets {
                let size = street.size();
                assert!(
                    (size.x - w).abs() < 1e-4 || (size.y - w).abs() < 1e-4,
                    "{street:?}"
                );
                assert!(t.grid().contains(street));
            }
            // The plaza is a block: bordered by streets on all four sides.
            for side in [Side::North, Side::South, Side::East, Side::West] {
                let beside = t.plaza.center() + normal(side) * (b / 2.0 + w / 2.0);
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
                PlazaPlacement::FacingStation => true,
                PlazaPlacement::Auto => !central && c.railway.is_some(),
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
            ((4, 5), 40.0, 10.0)
        );
        assert_eq!(c.plaza, PlazaPlacement::Auto);
        assert_eq!(
            c.railway,
            Some(RailwayConfig {
                side: Side::North,
                offset: 24.0
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
        // Around a central plaza: store west, police east, church north.
        let t = TownLayout::generate(&five_by_five()).unwrap();
        assert_eq!(
            find(&t, BuildingKind::GeneralStore).unwrap().front,
            Side::East
        );
        assert_eq!(
            find(&t, BuildingKind::PoliceStation).unwrap().front,
            Side::West
        );
        assert_eq!(find(&t, BuildingKind::Church).unwrap().front, Side::South);
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
                assert!(
                    !a.footprint.intersects(&t.plaza),
                    "{}: {a:?} in plaza",
                    c.name
                );
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
            let inner = t.plaza.expanded(-TREE_TRUNK_RADIUS);
            assert!(t.trees.iter().all(|&p| inner.contains_point(p)));
        }
    }

    #[test]
    fn railway_runs_past_the_town_edge_by_the_station() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let Some(config) = c.railway else {
                assert_eq!(t.railway, None);
                continue;
            };
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
            let corridor = Aabb::from_center_size(
                rail.start,
                Vec2::new(n.x.abs(), n.y.abs()) * 4.0 + along * 2000.0,
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
            assert_eq!(
                obstacles.len(),
                t.buildings.len() + t.trees.len() + 2 + t.sign_obstacles().len()
            );
            assert!(obstacles.contains(&t.hitching_post.rail));
            assert!(obstacles.contains(&t.hitching_post.horse.footprint));
            for b in &t.buildings {
                assert!(obstacles.contains(&b.footprint));
            }
        }
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
                (post.veranda..post.veranda + 3.0).contains(&gap),
                "{}: gap {gap}",
                c.name
            );
            assert!((1.0..=3.0).contains(&post.veranda));
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
            if let Some(r) = c.railway {
                assert!(
                    inward.dot(normal(r.side)) > -1e-4,
                    "{}: comes from away from the track",
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
                    min: 8.0,
                },
            ),
            (
                TownConfig {
                    railway: Some(RailwayConfig {
                        side: Side::North,
                        offset: 10.0,
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
}
