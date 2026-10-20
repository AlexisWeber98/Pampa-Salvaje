//! Kinematic rider (horse + outlaw) model.

use crate::{AnimationState, Body, Circle, Ground, Heading, Vec2, World};

/// Longest step the game feeds the simulation, in seconds, so a hitch never
/// teleports anything. [`Rider::step`] stays safe for longer steps anyway.
pub const MAX_STEP: f32 = 1.0 / 20.0;

/// Cap on collision sub-steps per step, so a huge `dt` cannot stall a frame.
const MAX_SUBSTEPS: usize = 32;

/// Tuning for the kinematic rider model. Speeds in m/s, rates per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiderParams {
    /// Turn rate in radians per second at full turn input.
    pub turn_rate: f32,
    /// Forward acceleration at full throttle.
    pub acceleration: f32,
    /// Deceleration when throttling against the current motion.
    pub braking: f32,
    /// Passive deceleration with no throttle, or above the gait's top speed.
    pub friction: f32,
    pub walk_max: f32,
    pub gallop_max: f32,
    pub reverse_max: f32,
}

impl Default for RiderParams {
    fn default() -> Self {
        Self {
            turn_rate: 1.8,
            acceleration: 4.0,
            braking: 8.0,
            friction: 3.0,
            walk_max: 3.0,
            // 44 km/h.
            gallop_max: 44.0 / 3.6,
            reverse_max: 1.2,
        }
    }
}

/// Player or AI intent for one simulation step.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RiderInput {
    /// `-1.0..=1.0`: forward throttle, negative brakes then backs up.
    pub throttle: f32,
    /// `-1.0..=1.0`: positive turns left (counter-clockwise).
    pub turn: f32,
    /// Hold to allow galloping speeds.
    pub gallop: bool,
}

/// Logical rider state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rider {
    pub position: Vec2,
    /// Ground height under the rider ([`World::ground_height`]: the terrain
    /// or a platform), kept in sync as it moves.
    pub height: f32,
    pub heading: Heading,
    /// Signed speed along the heading, in m/s.
    pub speed: f32,
    pub collider: Circle,
}

impl Rider {
    /// Collision radius of a horse and rider seen from above, in meters.
    pub const COLLIDER_RADIUS: f32 = 0.9;

    /// Farthest the rider moves in one collision sub-step, in meters.
    ///
    /// A circle cannot tunnel through a box while each move is shorter than
    /// its radius plus the box half-width; half the rider's radius keeps a
    /// wide margin even for the thinnest obstacle (a 0.7 m tree trunk).
    pub const MAX_SUBSTEP_DISTANCE: f32 = Self::COLLIDER_RADIUS * 0.5;

    /// How many collision sub-steps a step of `dt` seconds needs so that no
    /// sub-step moves farther than [`Self::MAX_SUBSTEP_DISTANCE`].
    pub fn substeps(&self, params: &RiderParams, dt: f32) -> usize {
        // Upper bound on the speed reached during the step.
        let fastest = self.speed.abs() + params.acceleration.max(params.braking) * dt;
        let reach = fastest * dt;
        let n = (reach / Self::MAX_SUBSTEP_DISTANCE).ceil();
        if n.is_finite() {
            (n as usize).clamp(1, MAX_SUBSTEPS)
        } else {
            1
        }
    }

    /// A horseback rider at rest at `position`, standing on `ground`
    /// (pass the [`World`] so platforms count).
    pub fn new(position: Vec2, heading: Heading, ground: &(impl Ground + ?Sized)) -> Self {
        Self {
            position,
            height: ground.ground_height(position),
            heading,
            speed: 0.0,
            collider: Circle::new(Self::COLLIDER_RADIUS),
        }
    }

    /// Advances the rider by `dt` seconds inside `world`.
    ///
    /// Non-finite turn or throttle input counts as no input, and a
    /// non-finite or non-positive `dt` leaves the rider untouched.
    pub fn step(&mut self, input: RiderInput, params: &RiderParams, world: &World, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let input = RiderInput {
            throttle: finite_or_zero(input.throttle).clamp(-1.0, 1.0),
            turn: finite_or_zero(input.turn).clamp(-1.0, 1.0),
            gallop: input.gallop,
        };
        // Short sub-steps so fast moves cannot skip over thin obstacles.
        let n = self.substeps(params, dt);
        let sub_dt = dt / n as f32;
        for _ in 0..n {
            self.substep(input, params, world, sub_dt);
        }
        self.height = world.ground_height(self.position);
    }

    /// One collision sub-step with already sanitized input.
    fn substep(&mut self, input: RiderInput, params: &RiderParams, world: &World, dt: f32) {
        self.heading = self.heading.rotated(input.turn * params.turn_rate * dt);
        self.speed = next_speed(self.speed, input.throttle, input.gallop, params, dt);
        self.position += self.heading.forward() * (self.speed * dt);

        self.resolve_obstacles(world);
        let contained = world.bounds.contain(self.position, self.collider.radius);
        self.position = contained.position;
        for push in [contained.push_x, contained.push_y].into_iter().flatten() {
            self.block(push);
        }
    }

    /// Pushes the rider out of every overlapping obstacle, sliding along walls.
    fn resolve_obstacles(&mut self, world: &World) {
        let mut normals = Vec::new();
        self.position = world.resolve(self.collider, self.position, |n| normals.push(n));
        for normal in normals {
            self.block(normal);
        }
    }

    /// Removes the part of the rider's velocity that points against `normal`
    /// (the unit direction a wall pushed the rider in), so it slides along
    /// walls instead of sticking.
    fn block(&mut self, normal: Vec2) {
        let along_normal = self.heading.forward().dot(normal);
        if self.speed * along_normal < 0.0 {
            // v' = v - (v.n) n, projected back onto the (unchanged) heading.
            self.speed -= self.speed * along_normal * along_normal;
        }
    }

    /// Where this rider stands, as an obstacle for others.
    pub fn body(&self) -> Body {
        Body {
            position: self.position,
            collider: self.collider,
        }
    }

    /// Moves the rider out of any of `bodies` it still overlaps (people
    /// pinned against a wall that could not make way), stopping its motion
    /// into them.
    pub fn keep_clear_of(&mut self, world: &World, bodies: &[Body]) {
        let mut normals = Vec::new();
        self.position = world.resolve_among(self.collider, self.position, bodies, |n| {
            normals.push(n);
        });
        for normal in normals {
            self.block(normal);
        }
        self.height = world.ground_height(self.position);
    }

    pub fn animation(&self, params: &RiderParams) -> AnimationState {
        AnimationState::from_speed(self.speed, params.walk_max)
    }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Moves `value` toward `target` by at most `max_delta`, never overshooting.
fn approach(value: f32, target: f32, max_delta: f32) -> f32 {
    if value < target {
        (value + max_delta).min(target)
    } else {
        (value - max_delta).max(target)
    }
}

fn next_speed(speed: f32, throttle: f32, gallop: bool, p: &RiderParams, dt: f32) -> f32 {
    let top = if gallop { p.gallop_max } else { p.walk_max };
    if throttle > 0.0 {
        if speed < 0.0 {
            approach(speed, 0.0, p.braking * throttle * dt)
        } else if speed > top {
            // Dropping out of a gallop: ease down instead of snapping.
            approach(speed, top, p.friction * dt)
        } else {
            approach(speed, top, p.acceleration * throttle * dt)
        }
    } else if throttle < 0.0 {
        let amount = -throttle;
        if speed > 0.0 {
            approach(speed, 0.0, p.braking * amount * dt)
        } else {
            approach(speed, -p.reverse_max, p.acceleration * amount * dt)
        }
    } else {
        approach(speed, 0.0, p.friction * dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aabb, Terrain};
    use std::f32::consts::FRAC_PI_4;

    const DT: f32 = 1.0 / 60.0;

    fn rider() -> (Rider, RiderParams, World) {
        let world = World::default();
        let r = Rider::new(Vec2::new(3.0, -2.0), Heading::from_radians(0.0), &world);
        (r, RiderParams::default(), world)
    }

    fn run(r: &mut Rider, input: RiderInput, p: &RiderParams, t: &World, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            r.step(input, p, t, DT);
        }
    }

    #[test]
    fn new_rider_rests_on_terrain() {
        let (r, p, t) = rider();
        assert_eq!(r.speed, 0.0);
        assert_eq!(r.height, t.terrain.height(3.0, -2.0));
        assert_eq!(r.animation(&p), AnimationState::Idle);
    }

    #[test]
    fn idle_rider_stays_put() {
        let (mut r, p, t) = rider();
        run(&mut r, RiderInput::default(), &p, &t, 2.0);
        assert_eq!(r.position, Vec2::new(3.0, -2.0));
        assert_eq!(r.animation(&p), AnimationState::Idle);
    }

    #[test]
    fn throttle_accelerates_to_walk_cap() {
        let (mut r, p, t) = rider();
        let walk = RiderInput {
            throttle: 1.0,
            ..Default::default()
        };
        r.step(walk, &p, &t, DT);
        assert!(r.speed > 0.0);
        run(&mut r, walk, &p, &t, 5.0);
        assert!((r.speed - p.walk_max).abs() < 1e-4);
        assert_eq!(r.animation(&p), AnimationState::Walk);
    }

    #[test]
    fn gallop_reaches_higher_cap_and_drops_back_when_released() {
        let (mut r, p, t) = rider();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        run(&mut r, gallop, &p, &t, 6.0);
        assert!((r.speed - p.gallop_max).abs() < 1e-4);
        assert_eq!(r.animation(&p), AnimationState::Gallop);

        let walk = RiderInput {
            gallop: false,
            ..gallop
        };
        run(&mut r, walk, &p, &t, 6.0);
        assert!((r.speed - p.walk_max).abs() < 1e-4);
    }

    #[test]
    fn friction_stops_without_reversing() {
        let (mut r, p, t) = rider();
        r.speed = 5.0;
        run(&mut r, RiderInput::default(), &p, &t, 5.0);
        assert_eq!(r.speed, 0.0);
    }

    #[test]
    fn negative_throttle_brakes_then_backs_up() {
        let (mut r, p, t) = rider();
        r.speed = 2.0;
        let back = RiderInput {
            throttle: -1.0,
            ..Default::default()
        };
        r.step(back, &p, &t, DT);
        assert!(r.speed < 2.0 && r.speed > 0.0);
        run(&mut r, back, &p, &t, 5.0);
        assert!((r.speed + p.reverse_max).abs() < 1e-4);
    }

    #[test]
    fn moves_along_heading() {
        let (mut r, p, t) = rider();
        r.heading = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        r.speed = 2.0;
        r.step(
            RiderInput {
                throttle: 1.0,
                ..Default::default()
            },
            &p,
            &t,
            DT,
        );
        assert!((r.position.x - 3.0).abs() < 1e-4);
        assert!(r.position.y > -2.0);
    }

    #[test]
    fn turning_is_continuous_not_snapped() {
        let (mut r, p, t) = rider();
        let left = RiderInput {
            turn: 1.0,
            ..Default::default()
        };
        r.step(left, &p, &t, DT);
        let expected = p.turn_rate * DT;
        assert!((r.heading.radians() - expected).abs() < 1e-5);
        assert!((r.heading.radians() % FRAC_PI_4).abs() > 1e-3);

        let right = RiderInput {
            turn: -1.0,
            ..Default::default()
        };
        r.step(right, &p, &t, DT);
        r.step(right, &p, &t, DT);
        assert!(r.heading.radians() > 6.0, "should wrap below zero");
    }

    #[test]
    fn sticks_to_terrain_while_moving() {
        let (mut r, p, t) = rider();
        let gallop = RiderInput {
            throttle: 1.0,
            turn: 0.3,
            gallop: true,
        };
        run(&mut r, gallop, &p, &t, 3.0);
        assert_eq!(r.height, t.terrain.height(r.position.x, r.position.y));
    }

    #[test]
    fn inputs_are_clamped() {
        let (mut r, p, t) = rider();
        let wild = RiderInput {
            throttle: 50.0,
            turn: 50.0,
            gallop: false,
        };
        r.step(wild, &p, &t, DT);
        assert!(r.speed <= p.acceleration * DT + 1e-6);
        assert!(r.heading.radians() <= p.turn_rate * DT + 1e-6);
    }

    #[test]
    fn non_finite_input_counts_as_no_input() {
        let (mut r, p, t) = rider();
        let start = r;
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let input = RiderInput {
                throttle: bad,
                turn: bad,
                gallop: true,
            };
            r.step(input, &p, &t, DT);
            assert_eq!(r, start, "input {bad} must not move the rider");
        }

        r.speed = 2.0;
        let nan_turn = RiderInput {
            throttle: 1.0,
            turn: f32::NAN,
            gallop: false,
        };
        r.step(nan_turn, &p, &t, DT);
        assert_eq!(r.heading, start.heading);
        assert!(r.position.x.is_finite() && r.position.x > start.position.x);
    }

    #[test]
    fn bad_dt_is_a_no_op() {
        let (mut r, p, t) = rider();
        r.speed = 3.0;
        let start = r;
        let input = RiderInput {
            throttle: 1.0,
            turn: 1.0,
            gallop: true,
        };
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -DT, 0.0] {
            r.step(input, &p, &t, dt);
            assert_eq!(r, start, "dt {dt} must not change the rider");
        }
    }

    #[test]
    fn cannot_leave_world_bounds() {
        let (mut r, p, t) = rider();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        // A minute of gallop from 200 m short of the east edge.
        r.position = Vec2::new(t.bounds.max.x - 200.0, 0.0);
        run(&mut r, gallop, &p, &t, 60.0);
        let edge = t.bounds.max.x - Rider::COLLIDER_RADIUS;
        assert!((r.position.x - edge).abs() < 1e-3, "x = {}", r.position.x);
        assert!(r.speed.abs() < 1e-3, "head-on wall stops the rider");
        assert_eq!(r.height, t.terrain.height(r.position.x, r.position.y));
    }

    #[test]
    fn slides_along_world_edge() {
        let (mut r, p, t) = rider();
        r.position = Vec2::new(t.bounds.max.x - Rider::COLLIDER_RADIUS, 0.0);
        // Heading 30 degrees north of east: into the east edge, partly along it.
        r.heading = Heading::from_radians(30f32.to_radians());
        r.speed = 4.0;
        r.step(RiderInput::default(), &p, &t, DT);
        assert!(r.position.x <= t.bounds.max.x - Rider::COLLIDER_RADIUS);
        assert!(r.position.y > 0.0, "keeps moving along the edge");
        assert!(
            r.speed > 0.0 && r.speed < 4.0,
            "loses only the part into the wall"
        );
    }

    fn walled_world() -> World {
        let mut world = World::default();
        world
            .obstacles
            .push(Aabb::new(Vec2::new(10.0, -10.0), Vec2::new(14.0, 10.0)));
        world
    }

    #[test]
    fn cannot_ride_through_a_building() {
        let (mut r, p, _) = rider();
        let world = walled_world();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        run(&mut r, gallop, &p, &world, 10.0);
        let face = 10.0 - Rider::COLLIDER_RADIUS;
        assert!((r.position.x - face).abs() < 1e-3, "x = {}", r.position.x);
        assert!(r.speed.abs() < 1e-3, "head-on stops the rider");
    }

    #[test]
    fn slides_along_a_building() {
        let (mut r, p, _) = rider();
        let world = walled_world();
        r.position = Vec2::new(10.0 - Rider::COLLIDER_RADIUS, 0.0);
        r.heading = Heading::from_radians(-30f32.to_radians());
        r.speed = 4.0;
        r.step(RiderInput::default(), &p, &world, DT);
        assert!(r.position.x <= 10.0 - Rider::COLLIDER_RADIUS + 1e-4);
        assert!(r.position.y < 0.0, "keeps moving along the wall");
        assert!(r.speed > 0.0 && r.speed < 4.0);
    }

    fn galloping_into(trunk: Vec2, params: RiderParams, world: &World) -> Rider {
        let mut r = Rider::new(
            trunk - Vec2::new(10.0, 0.0),
            Heading::from_radians(0.0),
            world,
        );
        r.speed = params.gallop_max;
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let near_face = trunk.x - crate::town::TREE_TRUNK_RADIUS - Rider::COLLIDER_RADIUS;
        for frame in 0..(4.0 / MAX_STEP) as usize {
            r.step(gallop, &params, world, MAX_STEP);
            assert!(
                r.position.x <= near_face + 1e-3,
                "frame {frame}: tunneled to x = {} (face {near_face})",
                r.position.x
            );
        }
        r
    }

    #[test]
    fn galloping_head_on_into_a_plaza_tree_stays_on_the_near_side() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let trunk = town.trees[0];
        let r = galloping_into(trunk, RiderParams::default(), &world);
        assert!(r.speed.abs() < 1e-3, "head-on stops the rider");
        assert!((r.position.y - trunk.y).abs() < 1e-3);
    }

    #[test]
    fn even_a_much_faster_horse_cannot_tunnel_through_a_trunk() {
        let trunk = Vec2::new(20.0, 0.0);
        let mut world = World::default();
        let size = Vec2::new(2.0, 2.0) * crate::town::TREE_TRUNK_RADIUS;
        world.obstacles.push(Aabb::from_center_size(trunk, size));
        // 40 m/s moves 2 m per max step: more than radius + half-width.
        let params = RiderParams {
            gallop_max: 40.0,
            ..RiderParams::default()
        };
        let r = galloping_into(trunk, params, &world);
        assert!(r.speed.abs() < 1e-3);
    }

    #[test]
    fn substeps_keep_every_move_under_the_bound() {
        let (mut r, p, _) = rider();
        r.speed = 0.0;
        assert_eq!(r.substeps(&p, DT), 1);
        for (speed, dt) in [(p.gallop_max, MAX_STEP), (40.0, MAX_STEP), (-3.0, 0.25)] {
            r.speed = speed;
            let n = r.substeps(&p, dt);
            let reach = (speed.abs() + p.acceleration.max(p.braking) * dt) * dt;
            assert!(reach / n as f32 <= Rider::MAX_SUBSTEP_DISTANCE + 1e-6);
            assert!(n < 64, "bounded work: {n}");
        }
    }

    /// Gallops a wandering path from `town`'s arrival point for `seconds`,
    /// checking the rider never ends a step inside anything; returns how
    /// many steps ended touching an obstacle.
    fn wander_through(town: &crate::TownLayout, seconds: f32) -> usize {
        wander_turning(town, seconds, 0.004)
    }

    /// Like [`wander_through`], turning at `rate` radians of phase per step.
    fn wander_turning(town: &crate::TownLayout, seconds: f32, rate: f32) -> usize {
        let world = World::with_town(Terrain::default(), town);
        let p = RiderParams::default();
        let (start, heading) = town.arrival();
        let mut r = Rider::new(start, heading, &world);
        let mut contacts = 0;
        for frame in 0..(seconds / DT) as usize {
            // Wander: gallop with a slowly changing turn.
            let turn = ((frame as f32) * rate).sin() * 0.6;
            let input = RiderInput {
                throttle: 1.0,
                turn,
                gallop: true,
            };
            r.step(input, &p, &world, DT);
            for o in &world.obstacles {
                let gap = r.position.distance(o.closest_point(r.position));
                assert!(
                    gap >= Rider::COLLIDER_RADIUS - 1e-3,
                    "{}, frame {frame}: inside {o:?} at {:?}",
                    town.name,
                    r.position
                );
                if gap < Rider::COLLIDER_RADIUS + 1e-2 {
                    contacts += 1;
                }
            }
            assert!(r.position.x.is_finite() && r.position.y.is_finite());
        }
        contacts
    }

    #[test]
    fn never_ends_a_step_inside_town_buildings() {
        // A turn rate whose path through Castex runs into buildings.
        let contacts = wander_turning(&crate::TownLayout::castex_inspired(), 240.0, 0.003);
        assert!(contacts > 0, "the wander path must actually hit buildings");
    }

    #[test]
    fn never_ends_a_step_inside_the_buildings_of_any_town() {
        let mut contacts = 0;
        for town in crate::town::tests::sample_towns() {
            contacts += wander_through(&town, 90.0);
        }
        assert!(contacts > 0, "the wander paths must actually hit buildings");
    }
}
