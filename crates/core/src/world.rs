//! The static world the rider moves through: terrain and playable bounds.

use crate::{Aabb, Body, Circle, SideObstacle, Terrain, TownLayout, Vec2};

/// Half the side length of the square terrain patch, in meters.
///
/// Render draws exactly this patch, so the playable bounds and the drawn
/// ground always agree.
pub const PATCH_HALF_EXTENT: f32 = 300.0;

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
    /// The drawn terrain patch, centered on the origin.
    pub const PATCH: Self = Self::centered(PATCH_HALF_EXTENT);

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
    pub terrain: Terrain,
    pub bounds: WorldBounds,
    /// Static footprints the rider cannot pass through.
    pub obstacles: Vec<Aabb>,
    /// Moving footprints (the train), refreshed by the game every frame.
    pub moving: Vec<SideObstacle>,
}

impl World {
    /// Open terrain inside the drawn patch, with nothing built on it.
    pub fn open(terrain: Terrain) -> Self {
        Self {
            terrain,
            bounds: WorldBounds::PATCH,
            obstacles: Vec::new(),
            moving: Vec::new(),
        }
    }

    /// `terrain` leveled under `town`, with the town's footprints as obstacles.
    pub fn with_town(terrain: Terrain, town: &TownLayout) -> Self {
        Self {
            terrain: terrain.with_flat_area(town.flat_area(&terrain)),
            bounds: WorldBounds::PATCH,
            obstacles: town.obstacles(),
            moving: Vec::new(),
        }
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

    /// Like [`Self::resolve`], also pushing the circle clear of other
    /// characters' `bodies`. Bodies give way to the world: when a body and
    /// a wall pinch the circle, the wall wins.
    pub fn resolve_among(
        &self,
        collider: Circle,
        center: Vec2,
        bodies: &[Body],
        mut on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        let start = center;
        let resolved = self.push_passes(collider, center, bodies, true, &mut on_push);
        if self.overlap(collider, resolved) < CLEAR {
            return resolved;
        }
        let spot = match self.nearest_clear(collider, start) {
            Some(spot) => spot,
            None => self.push_passes(collider, resolved, &[], false, &mut |_| ()),
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
    /// world gets the last word in every round.
    fn push_passes(
        &self,
        collider: Circle,
        mut center: Vec2,
        bodies: &[Body],
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
            for obstacle in &self.obstacles {
                apply(collider.push_out(center, obstacle), &mut center);
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

    /// Deepest overlap of a circle at `center` with any obstacle, in meters.
    fn overlap(&self, collider: Circle, center: Vec2) -> f32 {
        let footprints = self
            .obstacles
            .iter()
            .chain(self.moving.iter().map(|m| &m.footprint));
        footprints
            .filter_map(|o| collider.push_out(center, o))
            .map(Vec2::length)
            .fold(0.0, f32::max)
    }

    /// Nearest spot to `from` where the circle overlaps nothing, along the
    /// directions across and along every moving obstacle it touches and the
    /// world axes; `None` if there is none within [`SEARCH_RANGE`].
    fn nearest_clear(&self, collider: Circle, from: Vec2) -> Option<Vec2> {
        let axes = [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)];
        let touching = self
            .moving
            .iter()
            .filter(|m| collider.push_out(from, &m.footprint).is_some())
            .flat_map(|m| [m.across, Vec2::new(-m.across.y, m.across.x)]);
        let directions: Vec<Vec2> = touching.chain(axes).flat_map(|d| [d, d * -1.0]).collect();
        // Strictly clear: the result must not count as touching anything.
        let clear = |p: Vec2| self.overlap(collider, p) == 0.0;
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
}

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
    fn patch_bounds_match_the_drawn_patch() {
        assert_eq!(WorldBounds::PATCH.size(), Vec2::new(600.0, 600.0));
        assert_eq!(WorldBounds::PATCH.min, Vec2::new(-300.0, -300.0));
    }

    #[test]
    fn inside_points_are_untouched() {
        let c = WorldBounds::PATCH.contain(Vec2::new(10.0, -20.0), 1.0);
        assert_eq!(c.position, Vec2::new(10.0, -20.0));
        assert_eq!((c.push_x, c.push_y), (None, None));
    }

    #[test]
    fn keeps_the_whole_circle_inside_and_reports_push_directions() {
        let c = WorldBounds::PATCH.contain(Vec2::new(400.0, -299.5), 1.0);
        assert_eq!(c.position, Vec2::new(299.0, -299.0));
        assert_eq!(c.push_x, Some(Vec2::new(-1.0, 0.0)));
        assert_eq!(c.push_y, Some(Vec2::new(0.0, 1.0)));
    }

    #[test]
    fn town_world_levels_ground_and_blocks_buildings() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let area = town.flat_area(&Terrain::default());
        assert_eq!(world.terrain.flat_area, Some(area));
        assert_eq!(world.obstacles, town.obstacles());
        assert_eq!(world.bounds, WorldBounds::PATCH);
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
        world.obstacles[0] = Aabb::new(Vec2::new(3.0, 2.0), Vec2::new(4.6, 6.0));
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
}
