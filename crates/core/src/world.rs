//! The static world the rider moves through: terrain, raised floors and
//! playable bounds.

use std::sync::Arc;

use crate::obstacles::around;
use crate::{
    Body, ChunkCoord, Circle, FenceLayout, Floor, FloraSite, RailLine, RoadNetwork,
    SettlementLayout, SideObstacle, StaticObstacles, Terrain, TownLayout, Vec2, WorldConfig,
};

/// Axis-aligned playable area on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min: Vec2,
    pub max: Vec2,
}

/// Result of keeping a circle inside [`WorldBounds`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contained {
    pub position: Vec2,
    /// Unit direction the circle was pushed back in along `x`, if it was.
    pub push_x: Option<Vec2>,
    /// Unit direction the circle was pushed back in along `y`, if it was.
    pub push_y: Option<Vec2>,
}

impl WorldBounds {
    /// A square of side `2 * half_extent` centered on the origin.
    pub const fn centered(half_extent: f32) -> Self {
        Self {
            min: Vec2::new(-half_extent, -half_extent),
            max: Vec2::new(half_extent, half_extent),
        }
    }

    /// Side lengths along `x` and `y`.
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Clamps a circle of `radius` at `center` so it lies fully inside.
    pub fn contain(&self, center: Vec2, radius: f32) -> Contained {
        let (x, push_x) = clamp_axis(center.x, self.min.x + radius, self.max.x - radius);
        let (y, push_y) = clamp_axis(center.y, self.min.y + radius, self.max.y - radius);
        Contained {
            position: Vec2::new(x, y),
            push_x: push_x.map(|sign| Vec2::new(sign, 0.0)),
            push_y: push_y.map(|sign| Vec2::new(0.0, sign)),
        }
    }
}

/// Clamps `value` into `[lo, hi]`, reporting the sign of the push, if any.
fn clamp_axis(value: f32, lo: f32, hi: f32) -> (f32, Option<f32>) {
    if value < lo {
        (lo, Some(1.0))
    } else if value > hi {
        (hi, Some(-1.0))
    } else {
        (value, None)
    }
}

/// Everything static the rider collides with or stands on.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub config: WorldConfig,
    pub terrain: Terrain,
    pub bounds: WorldBounds,
    /// Static footprints the rider cannot pass through, indexed so
    /// collision only looks at the nearby ones.
    pub obstacles: StaticObstacles,
    /// Moving footprints (the train), refreshed by the game every frame.
    pub moving: Vec<SideObstacle>,
    /// Raised floors people walk on (station platforms), part of
    /// [`Self::ground_height`].
    pub floors: Vec<Floor>,
}

/// Something that knows how high the ground is where bodies stand.
pub trait Ground {
    /// Height of the walkable surface at `p`, in meters.
    fn ground_height(&self, p: Vec2) -> f32;
}

/// Bare terrain: no floors on it.
impl Ground for Terrain {
    fn ground_height(&self, p: Vec2) -> f32 {
        self.height(p.x, p.y)
    }
}

impl Ground for World {
    fn ground_height(&self, p: Vec2) -> f32 {
        World::ground_height(self, p)
    }
}

impl World {
    /// Open `terrain` inside the world `config` describes, with nothing
    /// built on it.
    pub fn new(config: WorldConfig, terrain: Terrain) -> Self {
        Self {
            config,
            terrain,
            bounds: config.bounds(),
            obstacles: StaticObstacles::default(),
            moving: Vec::new(),
            floors: Vec::new(),
        }
    }

    /// Height of the surface bodies stand on at `p`: the terrain, raised by
    /// the highest floor (a platform or its ramp) there, or by a level
    /// crossing's approach and deck
    /// ([`RoadNetwork::crossing_rise`]).
    /// Everything placed on the ground (riders, people, horses, the camera
    /// floor) uses this.
    pub fn ground_height(&self, p: Vec2) -> f32 {
        let crossing = self
            .terrain
            .roads
            .as_ref()
            .map_or(0.0, |roads| roads.crossing_rise(p));
        let rise = self
            .floors
            .iter()
            .filter_map(|f| f.rise_at(p))
            .fold(crossing, f32::max);
        self.terrain.height(p.x, p.y) + rise
    }

    /// Open terrain in the default world, with nothing built on it.
    pub fn open(terrain: Terrain) -> Self {
        Self::new(WorldConfig::default(), terrain)
    }

    /// `terrain` leveled under `town`, with the town's footprints as
    /// obstacles, in the default world.
    pub fn with_town(terrain: Terrain, town: &TownLayout) -> Self {
        Self::with_towns(WorldConfig::default(), terrain, std::slice::from_ref(town))
    }

    /// The world `config` describes, with `terrain` leveled under every
    /// town, every town's footprints as obstacles and its platforms as
    /// floors. The towns should come
    /// from [`crate::place_towns`], so they stay apart and inside.
    pub fn with_towns(config: WorldConfig, terrain: Terrain, towns: &[TownLayout]) -> Self {
        let mut terrain = terrain;
        for town in towns {
            let area = town.flat_area(&terrain);
            terrain = terrain.with_flat_area(area);
        }
        let mut world = Self::new(config, terrain);
        for town in towns {
            world.obstacles.extend(town.obstacles());
            world.floors.extend(town.floors());
        }
        world
    }

    /// Adds the bays' platforms as floors ([`RailLine::floors`]).
    pub fn lay_rail(&mut self, line: &RailLine) {
        self.floors.extend(line.floors());
    }

    /// Cuts `roads` into the terrain and adds their signposts and level
    /// crossing signs as obstacles. Lay roads before growing flora, which
    /// keeps off them.
    pub fn lay_roads(&mut self, roads: RoadNetwork) {
        self.obstacles.extend(roads.obstacles());
        let terrain = std::mem::take(&mut self.terrain);
        self.terrain = terrain.with_roads(Arc::new(roads));
    }

    /// Adds every span and every shut gate of `fences` as thin static
    /// obstacles [`FenceLayout::HEIGHT`] tall, which a jumping horse
    /// clears; open gates and broken spans are gaps. Returns how many
    /// boxes were added.
    pub fn fence(&mut self, fences: &FenceLayout) -> usize {
        let spans = fences.obstacles();
        let count = spans.len();
        for span in spans {
            self.obstacles.push_low(span, FenceLayout::HEIGHT);
        }
        count
    }

    /// Adds every house, galpón, tank and prop of `settlements` as a static
    /// obstacle, every corral as its rail spans (open at its gate) and
    /// every tree spot as its trunk, each as tall as its structure
    /// ([`SettlementLayout::obstacle_heights`]). Tracks are no obstacle.
    /// Returns how many boxes were added.
    pub fn settle(&mut self, settlements: &SettlementLayout) -> usize {
        let before = self.obstacles.len();
        for (obstacle, height) in settlements.obstacle_heights() {
            self.obstacles.push_low(obstacle, height);
        }
        self.obstacles.len() - before
    }

    /// Adds every tree trunk `flora` grows anywhere in the world as a
    /// static obstacle, so collision holds in chunks that are not drawn.
    /// Returns how many trees were planted. Shrubs and grass are not
    /// obstacles: a rider rides through them.
    pub fn plant(&mut self, flora: &FloraSite) -> usize {
        let trees = flora.all_trees();
        self.obstacles.extend(trees.iter().map(|t| t.obstacle()));
        trees.len()
    }

    /// Streams the trunks of chunk `coord`'s trees in as obstacles,
    /// replacing any it held, so the chunk collides without the whole map
    /// being planted. The same trees every time. Returns how many were
    /// planted. See [`Self::unplant_chunk`].
    pub fn plant_chunk(&mut self, flora: &FloraSite, coord: ChunkCoord) -> usize {
        let trees = flora.trees(coord);
        let count = trees.len();
        self.obstacles
            .set_chunk(coord, trees.iter().map(|t| t.obstacle()).collect());
        count
    }

    /// Drops what [`Self::plant_chunk`] planted in `coord`. Returns whether
    /// the chunk was planted.
    pub fn unplant_chunk(&mut self, coord: ChunkCoord) -> bool {
        self.obstacles.clear_chunk(coord)
    }
}

/// Overlap (in meters) below which a circle counts as clear of an obstacle.
const CLEAR: f32 = 1e-3;
/// Push-out passes before a circle counts as pinched.
const PASSES: usize = 8;
/// Sample spacing of the free-spot search, in meters.
const SEARCH_STEP: f32 = 0.25;
/// Farthest the free-spot search looks, in meters: past either end of the
/// longest train.
const SEARCH_RANGE: f32 = 60.0;
/// Extra reach of the static-obstacle lookup around a circle, in meters.
const LOOKUP_SLACK: f32 = 0.5;

impl World {
    /// Pushes a circle at `center` out of every overlapping obstacle, static
    /// or moving, and returns where it ends up. `on_push` receives the unit
    /// direction of each push (to cancel velocity into walls).
    ///
    /// A few passes settle corners where two footprints meet. When the
    /// pushes fight each other (a train car shoving the circle into a wall),
    /// the circle is pinched: it moves to the nearest spot clear of
    /// everything instead, searching across and along the moving obstacles
    /// and along the axes. If no such spot exists nearby, static obstacles
    /// win and the circle is only pushed out of those.
    pub fn resolve(&self, collider: Circle, center: Vec2, on_push: impl FnMut(Vec2)) -> Vec2 {
        self.resolve_among(collider, center, &[], on_push)
    }

    /// Like [`Self::resolve`] for a body `clearance` meters above the
    /// ground (a jumping horse): low obstacles no taller than that
    /// ([`StaticObstacles::near_above`]) let it pass.
    pub fn resolve_above(
        &self,
        collider: Circle,
        center: Vec2,
        clearance: f32,
        on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        self.resolve_among_above(collider, center, &[], clearance, on_push)
    }

    /// Like [`Self::resolve`], also pushing the circle clear of other
    /// characters' `bodies`. Bodies give way to the world: when a body and
    /// a wall pinch the circle, the wall wins.
    pub fn resolve_among(
        &self,
        collider: Circle,
        center: Vec2,
        bodies: &[Body],
        on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        self.resolve_among_above(
            collider,
            center,
            bodies,
            StaticObstacles::ON_GROUND,
            on_push,
        )
    }

    /// [`Self::resolve_among`] for a body `clearance` meters above the
    /// ground, like [`Self::resolve_above`].
    pub fn resolve_among_above(
        &self,
        collider: Circle,
        center: Vec2,
        bodies: &[Body],
        clearance: f32,
        mut on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        let start = center;
        let resolved = self.push_passes(collider, center, bodies, clearance, true, &mut on_push);
        if self.overlap(collider, resolved, clearance) < CLEAR {
            return resolved;
        }
        let spot = match self.nearest_clear(collider, start, clearance) {
            Some(spot) => spot,
            None => self.push_passes(collider, resolved, &[], clearance, false, &mut |_| ()),
        };
        let push = spot - resolved;
        let length = push.length();
        if length > 0.0 {
            on_push(push * (1.0 / length));
        }
        spot
    }

    /// Up to [`PASSES`] rounds of pushing the circle out of each body, then
    /// each obstacle in turn (moving ones only when `with_moving`), so the
    /// world gets the last word in every round. Low obstacles no taller
    /// than `clearance` are passed over.
    fn push_passes(
        &self,
        collider: Circle,
        mut center: Vec2,
        bodies: &[Body],
        clearance: f32,
        with_moving: bool,
        on_push: &mut impl FnMut(Vec2),
    ) -> Vec2 {
        for _ in 0..PASSES {
            let mut moved = false;
            let mut apply = |push: Option<Vec2>, center: &mut Vec2| {
                let Some(push) = push else { return };
                *center += push;
                let length = push.length();
                if length > 0.0 {
                    on_push(push * (1.0 / length));
                }
                moved = true;
            };
            for body in bodies {
                apply(collider.push_out_of_body(center, body), &mut center);
            }
            let nearby = self
                .obstacles
                .near_above(around(center, collider.radius + LOOKUP_SLACK), clearance);
            for obstacle in nearby {
                apply(collider.push_out(center, obstacle), &mut center);
            }
            // Deep water turns bodies back to the shallows.
            for lagoon in &self.terrain.lagoons {
                apply(lagoon.push_out(center), &mut center);
            }
            if with_moving {
                for obstacle in &self.moving {
                    apply(obstacle.push_out(collider, center), &mut center);
                }
            }
            if !moved {
                break;
            }
        }
        center
    }

    /// Deepest overlap of a circle at `center` with any obstacle taller
    /// than `clearance`, in meters.
    fn overlap(&self, collider: Circle, center: Vec2, clearance: f32) -> f32 {
        let footprints = self
            .obstacles
            .near_above(around(center, collider.radius + LOOKUP_SLACK), clearance)
            .into_iter()
            .chain(self.moving.iter().map(|m| &m.footprint));
        footprints
            .filter_map(|o| collider.push_out(center, o))
            .chain(
                self.terrain
                    .lagoons
                    .iter()
                    .filter_map(|l| l.push_out(center)),
            )
            .map(Vec2::length)
            .fold(0.0, f32::max)
    }

    /// Nearest spot to `from` where the circle overlaps nothing, along the
    /// directions across and along every moving obstacle it touches and the
    /// world axes; `None` if there is none within [`SEARCH_RANGE`].
    fn nearest_clear(&self, collider: Circle, from: Vec2, clearance: f32) -> Option<Vec2> {
        let axes = [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)];
        let touching = self
            .moving
            .iter()
            .filter(|m| collider.push_out(from, &m.footprint).is_some())
            .flat_map(|m| [m.across, Vec2::new(-m.across.y, m.across.x)]);
        let directions: Vec<Vec2> = touching.chain(axes).flat_map(|d| [d, d * -1.0]).collect();
        // Strictly clear: the result must not count as touching anything.
        let clear = |p: Vec2| self.overlap(collider, p, clearance) == 0.0;
        let mut best: Option<(f32, Vec2)> = None;
        for direction in directions {
            let limit = best.map_or(SEARCH_RANGE, |(t, _)| t);
            let mut blocked = 0.0;
            let mut t = SEARCH_STEP;
            while t <= limit {
                if clear(from + direction * t) {
                    // Tighten onto the edge between the last blocked sample
                    // and this clear one.
                    let mut free = t;
                    for _ in 0..12 {
                        let mid = (blocked + free) / 2.0;
                        if clear(from + direction * mid) {
                            free = mid;
                        } else {
                            blocked = mid;
                        }
                    }
                    if best.is_none_or(|(b, _)| free < b) {
                        best = Some((free, from + direction * free));
                    }
                    break;
                }
                blocked = t;
                t += SEARCH_STEP;
            }
        }
        best.map(|(_, spot)| spot)
    }

    /// Nearest spot to `near`, at most `range` meters away, where a body of
    /// `collider` can stand: clear of every obstacle, inside the bounds and
    /// on dry ground off the lagoons' wet mud. Searched on rings
    /// [`SPOT_STEP`] apart, nearest first; `None` if there is none.
    pub fn clear_spot(&self, collider: Circle, near: Vec2, range: f32) -> Option<Vec2> {
        let fits = |p: Vec2| {
            self.bounds.contain(p, collider.radius).position == p
                && !self.terrain.lagoons.iter().any(|l| l.is_bare(p))
                && self.overlap(collider, p, StaticObstacles::ON_GROUND) == 0.0
        };
        if fits(near) {
            return Some(near);
        }
        let rings = (range / SPOT_STEP).floor() as usize;
        (1..=rings).find_map(|k| {
            let r = k as f32 * SPOT_STEP;
            let n = ((std::f32::consts::TAU * r / SPOT_STEP).ceil() as usize).max(8);
            (0..n)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / n as f32;
                    near + Vec2::new(a.cos(), a.sin()) * r
                })
                .find(|&p| fits(p))
        })
    }
}

/// Spacing of the rings [`World::clear_spot`] searches, in meters.
const SPOT_STEP: f32 = 1.0;

impl Default for World {
    fn default() -> Self {
        Self::open(Terrain::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aabb, SideObstacle};

    #[test]
    fn every_town_levels_its_ground_and_blocks_its_buildings() {
        let configs = [
            crate::TownConfig::castex(),
            crate::TownConfig {
                name: "Lejos".into(),
                origin: Vec2::new(3000.0, -2000.0),
                ..crate::TownConfig::castex()
            },
        ];
        let (towns, errors) = crate::place_towns(&WorldConfig::default(), &configs);
        assert!(errors.is_empty(), "{errors:?}");
        let terrain = Terrain::default();
        let world = World::with_towns(WorldConfig::default(), terrain.clone(), &towns);
        assert_eq!(world.terrain.flat_areas.len(), 2);
        let mut all = Vec::new();
        for t in &towns {
            let area = t.flat_area(&terrain);
            assert!(world.terrain.flat_areas.contains(&area));
            let c = t.grid().center();
            assert_eq!(world.terrain.height(c.x, c.y), area.height);
            all.extend(t.obstacles());
        }
        assert_eq!(world.obstacles, all);
        // Riding into a building of the far town is blocked there too.
        let wall = towns[1].buildings[0].footprint;
        let c = Circle::new(0.9);
        let end = world.resolve(c, wall.center(), |_| ());
        assert!(deepest_overlap(&world, c, end) < 1e-3, "{end:?}");
    }

    #[test]
    fn a_body_higher_than_a_low_footprint_passes_over_it() {
        let mut world = World::default();
        let fence = Aabb::from_center_size(Vec2::ZERO, Vec2::new(10.0, 0.15));
        world.obstacles.push_low(fence, 1.3);
        let c = Circle::new(0.9);
        let on_it = Vec2::new(0.0, 0.3);
        // Above the wires: left where it is, nothing pushed.
        let mut pushes = 0;
        let end = world.resolve_above(c, on_it, 1.4, |_| pushes += 1);
        assert_eq!((end, pushes), (on_it, 0));
        // Lower than the wires, or on the ground: pushed clear as ever.
        for clearance in [1.2, 0.0] {
            let end = world.resolve_above(c, on_it, clearance, |_| ());
            assert!(end.y >= 0.9 + 0.075 - 1e-4, "{clearance}: {end:?}");
        }
        assert_eq!(
            world.resolve(c, on_it, |_| ()),
            world.resolve_above(c, on_it, 0.0, |_| ())
        );
    }

    #[test]
    fn a_clear_spot_is_the_nearest_free_ground_inside_the_world() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let c = Circle::new(0.9);
        // Open ground is its own clear spot.
        let open = world.bounds.min + Vec2::new(50.0, 50.0);
        assert_eq!(world.clear_spot(c, open, 10.0), Some(open));
        // Inside a building: the nearest spot out of it, overlapping nothing.
        let wall = town.buildings[0].footprint;
        let spot = world.clear_spot(c, wall.center(), 100.0).unwrap();
        assert_eq!(deepest_overlap(&world, c, spot), 0.0, "{spot:?}");
        assert_eq!(world.resolve(c, spot, |_| ()), spot);
        // Out of the corner of the world: pulled back inside.
        let corner = world.clear_spot(c, world.bounds.max, 5.0).unwrap();
        assert_eq!(world.bounds.contain(corner, 0.9).position, corner);
        // Nothing free within reach.
        assert_eq!(world.clear_spot(c, wall.center(), 0.0), None);
    }

    #[test]
    fn a_clear_spot_keeps_out_of_a_lagoon_and_off_its_mud() {
        let terrain = Terrain::default();
        let lagoon = crate::Lagoon::new(&crate::lagoon::tests::arocena_config(), &terrain);
        let world = World::open(terrain.with_lagoon(lagoon.clone()));
        let spot = world
            .clear_spot(Circle::new(0.9), lagoon.center, 2000.0)
            .unwrap();
        assert!(!lagoon.is_bare(spot), "{spot:?}");
        assert!(
            lagoon.offshore(spot) < crate::Lagoon::MUD_BAND + 2.0,
            "nearest"
        );
    }

    #[test]
    fn laying_the_rail_adds_the_bay_platform_as_a_floor_to_walk_and_board_from() {
        let (towns, line) = crate::train::line_tests::branched_line();
        let mut world = World::with_towns(WorldConfig::default(), Terrain::default(), &towns);
        let platforms = world.floors.len();
        world.lay_rail(&line);
        assert_eq!(world.floors.len(), platforms + 1);
        let bay = line.bays[0].platform;
        assert!(world.floors.contains(&bay));
        // The ground on it is its top, 0.9 m up, and level beside it.
        let on = bay.footprint.center();
        let rise = |p: Vec2| world.ground_height(p) - world.terrain.height(p.x, p.y);
        assert!((rise(on) - 0.9).abs() < 1e-5, "{}", rise(on));
        assert_eq!(rise(on + Vec2::new(0.0, 20.0)), 0.0);
    }

    #[test]
    fn the_ground_on_a_platform_is_its_top_and_its_ramps_lead_down() {
        let mut checked = 0;
        for town in crate::town::tests::sample_towns() {
            let world = World::with_town(Terrain::default(), &town);
            assert_eq!(world.floors, town.floors());
            let Some(platform) = town
                .buildings
                .iter()
                .find(|b| b.kind == crate::BuildingKind::Platform)
            else {
                assert!(world.floors.is_empty(), "{}", town.name);
                continue;
            };
            let floor = world.floors[0];
            let f = platform.footprint;
            let terrain = |p: Vec2| world.terrain.height(p.x, p.y);
            // Anywhere on the platform, the ground is its top.
            for (i, j) in [(0.5, 0.5), (0.0, 0.0), (1.0, 1.0), (0.1, 0.9), (0.95, 0.2)] {
                let p = f.min + Vec2::new(f.size().x * i, f.size().y * j);
                assert_eq!(world.ground_height(p), terrain(p) + platform.height);
                assert_eq!(Ground::ground_height(&world, p), world.ground_height(p));
            }
            // Down each ramp, steadily, to the bare ground at its foot.
            let mid = f.center().dot(floor.across());
            let (lo, hi) = (f.min.dot(floor.along), f.max.dot(floor.along));
            for (end, out) in [(hi, 1.0), (lo, -1.0)] {
                let at = |d: f32| floor.along * (end + out * d) + floor.across() * mid;
                let mut last = world.ground_height(at(0.0));
                for k in 1..=30 {
                    let p = at(k as f32 * Floor::RAMP_LENGTH / 30.0);
                    let h = world.ground_height(p);
                    assert!(h < last, "{}: {k}", town.name);
                    last = h;
                }
                assert!((last - terrain(at(Floor::RAMP_LENGTH))).abs() < 1e-4);
                assert_eq!(world.ground_height(at(4.0)), terrain(at(4.0)));
            }
            // Off its long sides, the bare ground: they are steps.
            for side in [
                f.min.dot(floor.across()) - 0.1,
                f.max.dot(floor.across()) + 0.1,
            ] {
                let p = floor.along * f.center().dot(floor.along) + floor.across() * side;
                assert_eq!(world.ground_height(p), terrain(p), "{}", town.name);
            }
            // The bare terrain knows nothing of platforms.
            let c = f.center();
            assert_eq!(Ground::ground_height(&world.terrain, c), terrain(c));
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn a_rider_wades_the_shallows_of_a_lagoon_but_never_its_deep_water() {
        let config = crate::lagoon::tests::arocena_config();
        let lagoon = crate::Lagoon::new(&config, &Terrain::default());
        let world = World::new(
            WorldConfig::default(),
            Terrain::default().with_lagoon(lagoon.clone()),
        );
        let collider = Circle::new(crate::Rider::COLLIDER_RADIUS);
        // Riding straight at the middle from every side: each step is
        // resolved, and the rider ends in the shallows, never deeper.
        for k in 0..16 {
            let angle = k as f32 * std::f32::consts::TAU / 16.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let mut at = lagoon.center + dir * (lagoon.shore_radius(angle) + 20.0);
            let mut pushed = false;
            for _ in 0..200 {
                at = world.resolve(collider, at + dir * -0.5, |_| pushed = true);
                assert!(!lagoon.is_deep(at), "{at:?}");
            }
            assert!(pushed && lagoon.is_water(at), "{at:?}");
            assert!(world.ground_height(at) < lagoon.level);
        }
        // Dry land is left alone.
        let dry = lagoon.center + Vec2::new(lagoon.radii.x * 2.0, 0.0);
        assert_eq!(world.resolve(collider, dry, |_| ()), dry);
    }

    #[test]
    fn world_bounds_follow_the_world_config() {
        assert_eq!(World::default().bounds, WorldConfig::default().bounds());
        let config = WorldConfig {
            half_extent: 1000.0,
            chunk_size: 100.0,
        };
        let world = World::new(config, Terrain::default());
        assert_eq!(world.bounds.min, Vec2::new(-1000.0, -1000.0));
        assert_eq!(world.bounds.size(), Vec2::new(2000.0, 2000.0));
        assert_eq!(world.config, config);
    }

    #[test]
    fn inside_points_are_untouched() {
        let c = WorldBounds::centered(300.0).contain(Vec2::new(10.0, -20.0), 1.0);
        assert_eq!(c.position, Vec2::new(10.0, -20.0));
        assert_eq!((c.push_x, c.push_y), (None, None));
    }

    #[test]
    fn keeps_the_whole_circle_inside_and_reports_push_directions() {
        let c = WorldBounds::centered(300.0).contain(Vec2::new(400.0, -299.5), 1.0);
        assert_eq!(c.position, Vec2::new(299.0, -299.0));
        assert_eq!(c.push_x, Some(Vec2::new(-1.0, 0.0)));
        assert_eq!(c.push_y, Some(Vec2::new(0.0, 1.0)));
    }

    #[test]
    fn town_world_levels_ground_and_blocks_buildings() {
        for town in crate::town::tests::sample_towns() {
            let world = World::with_town(Terrain::default(), &town);
            let area = town.flat_area(&Terrain::default());
            assert_eq!(world.terrain.flat_areas, vec![area]);
            assert_eq!(world.obstacles, town.obstacles());
            assert_eq!(world.bounds, WorldConfig::default().bounds());
        }
    }

    /// Deepest overlap of a circle at `p` with anything in `world`, in meters.
    pub(crate) fn deepest_overlap(world: &World, collider: Circle, p: Vec2) -> f32 {
        let statics = world
            .obstacles
            .iter()
            .filter_map(|o| collider.push_out(p, o));
        let moving = world.moving.iter().filter_map(|o| o.push_out(collider, p));
        statics.chain(moving).map(Vec2::length).fold(0.0, f32::max)
    }

    /// A 10 m car along `x` (3 m wide) with a wall 0.5 m north of it: too
    /// narrow a gap for a 0.9 m circle.
    fn pinch_world() -> World {
        let mut world = World::default();
        world
            .obstacles
            .push(Aabb::new(Vec2::new(-20.0, 2.0), Vec2::new(20.0, 6.0)));
        world.moving.push(SideObstacle {
            footprint: Aabb::from_center_size(Vec2::ZERO, Vec2::new(10.0, 3.0)),
            across: Vec2::new(0.0, 1.0),
        });
        world
    }

    #[test]
    fn a_circle_pinched_between_a_car_and_a_wall_ends_clear_of_both() {
        let world = pinch_world();
        let c = Circle::new(0.9);
        for k in 0..=20 {
            // From inside the car's north half up to touching the wall.
            let start = Vec2::new(-4.0 + 0.4 * k as f32, 0.1 + 0.1 * k as f32);
            let mut pushes = 0;
            let end = world.resolve(c, start, |n| {
                assert!((n.length() - 1.0).abs() < 1e-4, "unit push {n:?}");
                pushes += 1;
            });
            assert!(pushes > 0, "{start:?} needed a push");
            assert!(
                deepest_overlap(&world, c, end) < 1e-3,
                "{start:?} -> {end:?} still overlaps"
            );
            // Nearest free spot: across the car to its far side, or past
            // either end when the wall leaves room there and that is closer.
            let across = start.y + 1.5 + 0.9;
            let past_end = (start.x + 5.9).min(5.9 - start.x);
            let nearest = if start.y + 0.9 <= 2.0 {
                across.min(past_end)
            } else {
                across
            };
            assert!(
                end.distance(start) <= nearest + 0.01,
                "{start:?} -> {end:?}, nearest is {nearest}"
            );
        }
    }

    #[test]
    fn a_pinch_near_the_car_end_escapes_past_the_end_when_closer() {
        // The wall only covers the car's east end, and leaves room past it.
        let mut world = pinch_world();
        world.obstacles =
            StaticObstacles::new(vec![Aabb::new(Vec2::new(3.0, 2.0), Vec2::new(4.6, 6.0))]);
        let c = Circle::new(0.9);
        let start = Vec2::new(4.5, 1.6);
        let end = world.resolve(c, start, |_| ());
        assert!(deepest_overlap(&world, c, end) < 1e-3, "{end:?}");
        assert!(end.distance(start) < 3.0, "nearest free spot: {end:?}");
    }

    #[test]
    fn bodies_push_the_circle_aside_but_walls_win() {
        let world = pinch_world();
        let c = Circle::new(0.3);
        let rider = Body {
            position: Vec2::new(0.0, 10.0),
            collider: Circle::new(0.9),
        };
        // Open ground: straight out of the rider.
        let end = world.resolve_among(c, Vec2::new(0.0, 10.5), &[rider], |_| ());
        assert!(end.distance(Vec2::new(0.0, 11.2)) < 1e-4, "{end:?}");
        assert_eq!(
            world.resolve(c, Vec2::new(0.0, 10.5), |_| ()),
            Vec2::new(0.0, 10.5)
        );
        // A rider against the wall's north face cannot push into the wall.
        let rider = Body {
            position: Vec2::new(0.0, 7.0),
            ..rider
        };
        let end = world.resolve_among(c, Vec2::new(0.0, 6.4), &[rider], |_| ());
        assert!(deepest_overlap(&world, c, end) < 1e-3, "{end:?}");
    }

    #[test]
    fn unpinched_pushes_are_unchanged() {
        let world = pinch_world();
        let c = Circle::new(0.9);
        // South half of the car, open ground beyond: plain push across.
        let end = world.resolve(c, Vec2::new(1.0, -0.5), |_| ());
        assert!(end.distance(Vec2::new(1.0, -2.4)) < 1e-4, "{end:?}");
        // Against the wall from the north: plain push out.
        let end = world.resolve(c, Vec2::new(0.0, 6.5), |_| ());
        assert!(end.distance(Vec2::new(0.0, 6.9)) < 1e-4, "{end:?}");
    }

    /// The shipped towns with their roads laid, unplanted: trees keep off
    /// the roads (`nothing_blocks_a_rider_...` checks it).
    fn roaded_world() -> (World, crate::RoadNetwork, RailLine) {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads.clone());
        (world, roads, line)
    }

    #[test]
    fn nothing_blocks_a_rider_anywhere_on_a_road_or_its_level_crossings() {
        let (mut world, roads, line) = roaded_world();
        world.plant(&FloraSite::new(world.config, &world.terrain, &line));
        let c = Circle::new(crate::Rider::COLLIDER_RADIUS);
        for road in &roads.roads {
            for w in road.path.windows(2) {
                for k in 0..4 {
                    let p = w[0] + (w[1] - w[0]) * (k as f32 / 4.0);
                    assert_eq!(world.resolve(c, p, |_| ()), p, "blocked at {p:?}");
                }
            }
        }
        // The signs stand beside the crossing, the post beside the road.
        assert!(world.obstacles.len() > roads.obstacles().len());
    }

    #[test]
    fn a_rider_rides_out_of_town_across_the_level_crossing_to_the_rural_track() {
        let (world, roads, _) = roaded_world();
        let params = crate::RiderParams::default();
        for road in roads
            .roads
            .iter()
            .filter(|r| r.kind == crate::RoadKind::Rural)
        {
            let crossing = road.crossings[0];
            let start = road.path[0];
            let out = road.ends[0].out;
            let mut rider = crate::Rider::new(
                start,
                crate::Heading::from_radians(out.y.atan2(out.x)),
                &world,
            );
            let mut passed = false;
            for _ in 0..60 * 120 {
                // Steer for a point a little ahead along the road.
                let near = (0..road.path.len())
                    .min_by(|&a, &b| {
                        road.path[a]
                            .distance(rider.position)
                            .total_cmp(&road.path[b].distance(rider.position))
                    })
                    .unwrap();
                let aim = road.path[(near + 3).min(road.path.len() - 1)] - rider.position;
                let want = aim.y.atan2(aim.x);
                let mut turn = want - rider.heading.radians();
                while turn > std::f32::consts::PI {
                    turn -= std::f32::consts::TAU;
                }
                while turn < -std::f32::consts::PI {
                    turn += std::f32::consts::TAU;
                }
                let input = crate::RiderInput {
                    throttle: 1.0,
                    turn: (turn * 3.0).clamp(-1.0, 1.0),
                    gallop: true,
                };
                rider.step(input, &params, &world, 1.0 / 60.0);
                passed |= (rider.position - crossing.at).dot(crossing.road) > 10.0;
                if rider.position.distance(road.ends[1].at) < 8.0 {
                    break;
                }
            }
            assert!(passed, "never crossed at {:?}", crossing.at);
            assert!(
                rider.position.distance(road.ends[1].at) < 8.0,
                "stuck at {:?}",
                rider.position
            );
        }
    }

    /// The deck of a level crossing, above the ground: the planks' top.
    const DECK: f32 = crate::LineTrain::RAIL_TOP - 0.02;

    #[test]
    fn the_road_climbs_gently_onto_each_crossing_deck_and_down_again() {
        let (world, roads, _) = roaded_world();
        let raised = |p: Vec2| world.ground_height(p) - world.terrain.height(p.x, p.y);
        for c in roads.crossings() {
            let mut last: Option<f32> = None;
            for k in -60..=60 {
                let p = c.at + c.road * (k as f32 * 0.25);
                let h = world.ground_height(p);
                if let Some(before) = last {
                    let slope = (h - before).abs() / 0.25;
                    let land = (world.terrain.height(p.x, p.y)
                        - world
                            .terrain
                            .height(p.x - c.road.x * 0.25, p.y - c.road.y * 0.25))
                    .abs()
                        / 0.25;
                    assert!(slope - land <= 0.105, "{slope} at {p:?}");
                }
                last = Some(h);
            }
            assert!((raised(c.at) - DECK).abs() < 0.01, "{}", raised(c.at));
            for s in [-12.0, 12.0] {
                assert!(raised(c.at + c.road * s).abs() < 1e-4);
            }
            // Along the track, past the road's edges, the ground is the
            // terrain's (and the terrain under the track is untouched).
            for s in [-9.0, -6.0, 6.0, 9.0] {
                let p = c.at + c.rail * s;
                assert_eq!(
                    world.ground_height(p),
                    world.terrain.height(p.x, p.y),
                    "{s}"
                );
            }
        }
    }

    #[test]
    fn a_rider_rides_up_onto_the_deck_and_over_the_rails() {
        let (world, roads, _) = roaded_world();
        let params = crate::RiderParams::default();
        for c in roads.crossings() {
            let start = c.at - c.road * 20.0;
            let heading = crate::Heading::from_radians(c.road.y.atan2(c.road.x));
            let mut rider = crate::Rider::new(start, heading, &world);
            let mut on_deck = None;
            for _ in 0..60 * 20 {
                let input = crate::RiderInput {
                    throttle: 1.0,
                    ..Default::default()
                };
                rider.step(input, &params, &world, 1.0 / 60.0);
                let ground = world.terrain.height(rider.position.x, rider.position.y);
                if rider.position.distance(c.at) < 0.5 {
                    on_deck = Some(rider.height - ground);
                }
                if (rider.position - c.at).dot(c.road) > 15.0 {
                    break;
                }
            }
            let lift = on_deck.expect("rode over the crossing");
            assert!((lift - DECK).abs() < 0.01, "{lift}");
            assert!((rider.position - c.at).dot(c.road) > 15.0, "stuck");
        }
    }

    #[test]
    fn a_train_car_on_the_level_crossing_still_blocks_a_rider() {
        let (mut world, roads, _) = roaded_world();
        let crossing = *roads.crossings().next().expect("a level crossing");
        let across = Vec2::new(-crossing.rail.y, crossing.rail.x);
        world.moving.push(SideObstacle {
            footprint: Aabb::from_center_size(crossing.at, Vec2::new(16.0, 3.0)),
            across,
        });
        let c = Circle::new(crate::Rider::COLLIDER_RADIUS);
        let end = world.resolve(c, crossing.at, |_| ());
        assert!(end.distance(crossing.at) > 1.0, "{end:?}");
        assert!(deepest_overlap(&world, c, end) < 1e-3);
    }
}
