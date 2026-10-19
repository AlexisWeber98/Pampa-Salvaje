//! Settlement layouts: street grid, plaza, railway and building footprints.
//!
//! Pure data. Render decides walls, roofs and colors per [`BuildingKind`];
//! logic only needs footprints for collision and the leveled ground.

use crate::{Aabb, FlatArea, Heading, PATCH_HALF_EXTENT, Terrain, Vec2};

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
}

/// A small railway town laid out on an orthogonal grid.
#[derive(Debug, Clone, PartialEq)]
pub struct TownLayout {
    /// Street surfaces, including the ring street around the grid.
    pub streets: Vec<Aabb>,
    /// The central square.
    pub plaza: Aabb,
    /// Trunk positions of the plaza trees.
    pub trees: Vec<Vec2>,
    pub railway: Railway,
    pub buildings: Vec<Building>,
    /// Hitching rail in front of the general store.
    pub hitching_post: HitchingPost,
}

/// Blocks per side of the grid.
const BLOCKS: usize = 5;
/// Side of one square block, in meters.
const BLOCK: f32 = 40.0;
/// Street width, in meters.
const STREET: f32 = 12.0;
/// Half side of the whole grid, ring street included.
const GRID_HALF: f32 = (BLOCKS as f32 * BLOCK + (BLOCKS + 1) as f32 * STREET) / 2.0;
/// Trunk radius used for tree collision.
pub const TREE_TRUNK_RADIUS: f32 = 0.35;

/// Distance from the grid's north edge to the railway centerline.
const RAIL_OFFSET: f32 = 24.0;
/// Broad track gauge, in meters.
const RAIL_GAUGE: f32 = 1.676;
/// Leveled band around the town, in meters.
const FLAT_FALLOFF: f32 = 60.0;

impl TownLayout {
    /// A small pampa railway town inspired by 1920s–1930s settlements of La
    /// Pampa such as Eduardo Castex. It borrows the general pattern (grid,
    /// central plaza, railway at the edge, general store, grain sheds,
    /// windmills) and is not a reconstruction of any real town.
    pub fn castex_inspired() -> Self {
        let mut buildings = Vec::new();
        for j in 0..BLOCKS {
            for i in 0..BLOCKS {
                if (i, j) != PLAZA {
                    fill_block(i, j, &mut buildings);
                }
            }
        }
        buildings.extend(station_yard());

        let rail_y = GRID_HALF + RAIL_OFFSET;
        let store = buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .expect("the town has a general store");
        let hitching_post = hitching_post(store);
        Self {
            streets: streets(),
            plaza: block(PLAZA.0, PLAZA.1),
            trees: plaza_trees(block(PLAZA.0, PLAZA.1)),
            railway: Railway {
                start: Vec2::new(-PATCH_HALF_EXTENT, rail_y),
                end: Vec2::new(PATCH_HALF_EXTENT, rail_y),
                gauge: RAIL_GAUGE,
            },
            buildings,
            hitching_post,
        }
    }

    /// Bounding box of the street grid, ring street included.
    pub fn grid(&self) -> Aabb {
        Aabb::new(
            Vec2::new(-GRID_HALF, -GRID_HALF),
            Vec2::new(GRID_HALF, GRID_HALF),
        )
    }

    /// Bounding box of everything built: grid, station yard and railway corridor
    /// within the town.
    pub fn extent(&self) -> Aabb {
        let grid = self.grid();
        Aabb::new(grid.min, Vec2::new(grid.max.x, self.railway.start.y + 4.0))
    }

    /// Ground to level under the town, at the natural height of its center.
    pub fn flat_area(&self, terrain: &Terrain) -> FlatArea {
        let extent = self.extent();
        let center = extent.center();
        FlatArea {
            center,
            half_extents: extent.size() * 0.5,
            falloff: FLAT_FALLOFF,
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
    /// south of the town, facing north into it.
    pub fn arrival(&self) -> (Vec2, Heading) {
        // The north-south street just east of the plaza is the main road.
        let road_x = lane(PLAZA.0 + 1) + STREET / 2.0;
        (
            Vec2::new(road_x, -GRID_HALF - 25.0),
            Heading::from_radians(std::f32::consts::FRAC_PI_2),
        )
    }
}

/// A rail on the street side of `store`, past its veranda, with a horse tied
/// alongside it facing along the street.
fn hitching_post(store: &Building) -> HitchingPost {
    // Outward facade normal and the direction along the street.
    let (normal, along) = match store.front {
        Side::East => (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)),
        Side::West => (Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0)),
        Side::North => (Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)),
        Side::South => (Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0)),
    };
    let half = store.footprint.size() * 0.5;
    let face =
        store.footprint.center() + normal * (normal.x.abs() * half.x + normal.y.abs() * half.y);
    // Axis-aligned box of `length` along the street and `width` across it.
    let boxed = |center: Vec2, length: f32, width: f32| {
        let size = along * length + normal * width;
        Aabb::from_center_size(center, Vec2::new(size.x.abs(), size.y.abs()))
    };
    // Just past the store's 3 m veranda.
    let rail_offset = 3.2;
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
    }
}

/// Grid cell of the plaza: the central block.
const PLAZA: (usize, usize) = (BLOCKS / 2, BLOCKS / 2);

/// Lower coordinate of street number `k` (0 = the west or south ring street).
fn lane(k: usize) -> f32 {
    -GRID_HALF + k as f32 * (BLOCK + STREET)
}

/// Footprint of block `(i, j)`, counted from the south-west corner.
fn block(i: usize, j: usize) -> Aabb {
    let min = Vec2::new(lane(i) + STREET, lane(j) + STREET);
    Aabb::new(min, min + Vec2::new(BLOCK, BLOCK))
}

fn streets() -> Vec<Aabb> {
    (0..=BLOCKS)
        .flat_map(|k| {
            let c = lane(k);
            [
                // North-south street.
                Aabb::new(Vec2::new(c, -GRID_HALF), Vec2::new(c + STREET, GRID_HALF)),
                // East-west street.
                Aabb::new(Vec2::new(-GRID_HALF, c), Vec2::new(GRID_HALF, c + STREET)),
            ]
        })
        .collect()
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

/// Box inside block `b` from block-relative `(x0, y0)` to `(x1, y1)`.
fn rel(b: Aabb, x0: f32, y0: f32, x1: f32, y1: f32) -> Aabb {
    Aabb::new(b.min + Vec2::new(x0, y0), b.min + Vec2::new(x1, y1))
}

fn building(kind: BuildingKind, footprint: Aabb, height: f32, front: Side) -> Building {
    Building {
        kind,
        footprint,
        height,
        front,
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

/// Places the civic building of a special block, if any, and returns the
/// street side it takes so houses leave it free.
fn civic_building(i: usize, j: usize, b: Aabb, out: &mut Vec<Building>) -> Option<Side> {
    let (px, py) = PLAZA;
    if (i, j) == (px, py + 1) {
        // Church across the street north of the plaza, facing it.
        out.push(building(
            BuildingKind::Church,
            rel(b, 14.0, 0.0, 26.0, 22.0),
            7.0,
            Side::South,
        ));
        Some(Side::South)
    } else if (i, j) == (px + 1, py) {
        out.push(building(
            BuildingKind::PoliceStation,
            rel(b, 0.0, 13.0, 12.0, 27.0),
            4.5,
            Side::West,
        ));
        Some(Side::West)
    } else if (i, j) == (px - 1, py) {
        out.push(building(
            BuildingKind::GeneralStore,
            rel(b, 26.0, 12.0, 40.0, 28.0),
            5.0,
            Side::East,
        ));
        Some(Side::East)
    } else {
        None
    }
}

/// Blocks with a windmill and its water tank in the back yard.
const WINDMILL_BLOCKS: [(usize, usize); 3] = [(0, 0), (4, 3), (1, 4)];

/// Lines a block's street edges with houses (sheds on the outskirts) and adds
/// any civic building or back-yard windmill it holds.
fn fill_block(i: usize, j: usize, out: &mut Vec<Building>) {
    let b = block(i, j);
    let taken = civic_building(i, j, b, out);
    let outskirts = i == 0 || j == 0 || i == BLOCKS - 1 || j == BLOCKS - 1;
    let density = if outskirts { 0.45 } else { 0.75 };

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
        if Some(side) == taken || lot_noise(i, j, n, 1) > density {
            continue;
        }
        let width = length - 2.0 * lot_noise(i, j, n, 2);
        let depth = 8.0 + 3.0 * lot_noise(i, j, n, 3);
        let (a, w) = (start + (length - width) / 2.0, width);
        let footprint = match side {
            Side::South => rel(b, a, 0.0, a + w, depth),
            Side::North => rel(b, a, BLOCK - depth, a + w, BLOCK),
            Side::West => rel(b, 0.0, a, depth, a + w),
            Side::East => rel(b, BLOCK - depth, a, BLOCK, a + w),
        };
        let shed = outskirts && lot_noise(i, j, n, 4) < 0.25;
        let (kind, height) = if shed {
            (BuildingKind::Shed, 4.0)
        } else {
            (BuildingKind::House, 3.6 + 0.8 * lot_noise(i, j, n, 5))
        };
        out.push(building(kind, footprint, height, side));
    }

    if WINDMILL_BLOCKS.contains(&(i, j)) {
        out.extend(windmill_with_tank(rel(b, 14.5, 18.5, 17.5, 21.5)));
    }
}

/// A windmill tower on `tower` and its round tank just east of it.
fn windmill_with_tank(tower: Aabb) -> [Building; 2] {
    let tank_center = Vec2::new(tower.max.x + 1.0 + 3.5, tower.center().y);
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

/// Station, platform, grain stores, sheds and a windmill between the north
/// ring street and the track.
fn station_yard() -> Vec<Building> {
    let y0 = GRID_HALF + 4.0;
    let yard =
        |x0: f32, x1: f32, depth: f32| Aabb::new(Vec2::new(x0, y0), Vec2::new(x1, y0 + depth));
    let mut out = vec![
        building(
            BuildingKind::Station,
            yard(-12.0, 12.0, 9.0),
            5.0,
            Side::South,
        ),
        building(
            BuildingKind::Platform,
            Aabb::new(Vec2::new(-30.0, y0 + 9.0), Vec2::new(30.0, y0 + 16.0)),
            0.9,
            Side::North,
        ),
        building(
            BuildingKind::GrainStore,
            yard(40.0, 80.0, 12.0),
            6.0,
            Side::North,
        ),
        building(
            BuildingKind::GrainStore,
            yard(90.0, 125.0, 12.0),
            6.0,
            Side::North,
        ),
        building(
            BuildingKind::Shed,
            yard(-75.0, -50.0, 10.0),
            4.5,
            Side::South,
        ),
    ];
    out.extend(windmill_with_tank(Aabb::new(
        Vec2::new(-45.0, y0 + 3.0),
        Vec2::new(-42.0, y0 + 6.0),
    )));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rider, WorldBounds};

    fn town() -> TownLayout {
        TownLayout::castex_inspired()
    }

    fn count(t: &TownLayout, kind: BuildingKind) -> usize {
        t.buildings.iter().filter(|b| b.kind == kind).count()
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(town(), town());
    }

    #[test]
    fn has_the_institutions_of_a_railway_town() {
        let t = town();
        for kind in [
            BuildingKind::GeneralStore,
            BuildingKind::PoliceStation,
            BuildingKind::Church,
            BuildingKind::Station,
            BuildingKind::Platform,
        ] {
            assert_eq!(count(&t, kind), 1, "{kind:?}");
        }
        assert!(count(&t, BuildingKind::House) >= 30);
        assert!(count(&t, BuildingKind::Shed) + count(&t, BuildingKind::GrainStore) >= 3);
        assert!(count(&t, BuildingKind::Windmill) >= 1);
        assert_eq!(
            count(&t, BuildingKind::Windmill),
            count(&t, BuildingKind::WaterTank)
        );
        assert!(t.trees.len() >= 8);
    }

    #[test]
    fn grid_is_orthogonal_around_a_central_plaza() {
        let t = town();
        assert_eq!(t.streets.len(), 2 * (BLOCKS + 1));
        assert!(t.plaza.contains_point(t.grid().center()));
        assert!((t.plaza.size().x - BLOCK).abs() < 1e-4);
        for street in &t.streets {
            let size = street.size();
            assert!(size.x == STREET || size.y == STREET, "{street:?}");
        }
    }

    #[test]
    fn buildings_never_overlap_streets_plaza_or_each_other() {
        let t = town();
        for (i, a) in t.buildings.iter().enumerate() {
            assert!(!a.footprint.intersects(&t.plaza), "{a:?} in plaza");
            for street in &t.streets {
                assert!(!a.footprint.intersects(street), "{a:?} on {street:?}");
            }
            for b in &t.buildings[i + 1..] {
                assert!(!a.footprint.intersects(&b.footprint), "{a:?} vs {b:?}");
            }
            assert!(a.height > 0.0 && a.footprint.size().x > 0.0 && a.footprint.size().y > 0.0);
        }
    }

    #[test]
    fn trees_stand_inside_the_plaza() {
        let t = town();
        let inner = t.plaza.expanded(-TREE_TRUNK_RADIUS);
        assert!(t.trees.iter().all(|&p| inner.contains_point(p)));
    }

    #[test]
    fn railway_runs_past_the_town_edge_by_the_station() {
        let t = town();
        let grid = t.grid();
        let rail = t.railway;
        // A straight east-west line just outside the grid, spanning the patch.
        assert_eq!(rail.start.y, rail.end.y);
        assert!(rail.start.y > grid.max.y);
        assert!(rail.start.x <= -PATCH_HALF_EXTENT && rail.end.x >= PATCH_HALF_EXTENT);
        let station = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .unwrap();
        assert!(station.footprint.min.y > grid.max.y);
        assert!(rail.start.y - station.footprint.max.y < 15.0);
        for b in &t.buildings {
            let corridor = Aabb::new(
                Vec2::new(rail.start.x, rail.start.y - 2.0),
                Vec2::new(rail.end.x, rail.start.y + 2.0),
            );
            assert!(!b.footprint.intersects(&corridor), "{b:?} on the track");
        }
    }

    #[test]
    fn everything_built_sits_on_level_ground_inside_the_world() {
        let t = town();
        let terrain = Terrain::default();
        let area = t.flat_area(&terrain);
        assert_eq!(
            area.height,
            terrain.natural_height(area.center.x, area.center.y)
        );
        let leveled = terrain.with_flat_area(area);
        let bounds = Aabb::new(WorldBounds::PATCH.min, WorldBounds::PATCH.max);
        assert!(bounds.contains(&t.extent().expanded(area.falloff)));
        for b in &t.buildings {
            assert!(t.extent().contains(&b.footprint));
            for corner in [b.footprint.min, b.footprint.max] {
                assert_eq!(leveled.height(corner.x, corner.y), area.height);
            }
        }
    }

    #[test]
    fn obstacles_cover_buildings_and_trees() {
        let t = town();
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

    #[test]
    fn a_horse_is_tied_in_front_of_the_general_store() {
        let t = town();
        let store = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .unwrap();
        let post = t.hitching_post;
        let horse = post.horse;
        // On the street the store faces, just past its 3 m veranda.
        let street = t
            .streets
            .iter()
            .find(|s| s.contains(&horse.footprint))
            .expect("horse stands on a street");
        assert!(street.contains(&post.rail));
        let gap = horse.footprint.min.x - store.footprint.max.x;
        assert!(
            store.front == Side::East && (3.0..6.0).contains(&gap),
            "gap {gap}"
        );
        assert!((horse.footprint.center().y - store.footprint.center().y).abs() < 1e-4);
        assert_eq!(horse.position, horse.footprint.center());
        // Faces along the street, tied alongside the rail without touching it.
        assert!(horse.heading.forward().x.abs() < 1e-5);
        assert!(!horse.footprint.intersects(&post.rail));
        assert!(horse.footprint.min.x - post.rail.max.x < 0.5);
        // Leaves room to pass: a rider fits between the horse and the far side.
        assert!(street.max.x - horse.footprint.max.x > 4.0 * Rider::COLLIDER_RADIUS);
    }

    #[test]
    fn arrival_is_on_open_road_outside_the_grid_facing_in() {
        let t = town();
        let (spawn, heading) = t.arrival();
        assert!(spawn.y < t.grid().min.y);
        assert!(heading.forward().y > 0.99, "faces north into town");
        let road = t
            .streets
            .iter()
            .any(|s| s.min.x < spawn.x && spawn.x < s.max.x && s.size().x == STREET);
        assert!(road, "lines up with a north-south street");
        for o in t.obstacles() {
            assert!(!o.expanded(2.0).contains_point(spawn));
        }
    }
}
