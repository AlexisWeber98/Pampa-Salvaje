//! Kinematic rider (horse + outlaw) model.

use crate::{AnimationState, Circle, Heading, Terrain, Vec2, World};

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
            walk_max: 2.5,
            gallop_max: 11.0,
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
    /// Ground height under the rider, kept in sync with the terrain.
    pub height: f32,
    pub heading: Heading,
    /// Signed speed along the heading, in m/s.
    pub speed: f32,
    pub collider: Circle,
}

impl Rider {
    /// Collision radius of a horse and rider seen from above, in meters.
    pub const COLLIDER_RADIUS: f32 = 0.9;

    pub fn new(position: Vec2, heading: Heading, terrain: &Terrain) -> Self {
        Self {
            position,
            height: terrain.height(position.x, position.y),
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
        let throttle = finite_or_zero(input.throttle).clamp(-1.0, 1.0);
        let turn = finite_or_zero(input.turn).clamp(-1.0, 1.0);

        self.heading = self.heading.rotated(turn * params.turn_rate * dt);
        self.speed = next_speed(self.speed, throttle, input.gallop, params, dt);
        self.position += self.heading.forward() * (self.speed * dt);

        self.resolve_obstacles(world);
        let contained = world.bounds.contain(self.position, self.collider.radius);
        self.position = contained.position;
        for push in [contained.push_x, contained.push_y].into_iter().flatten() {
            self.block(push);
        }

        self.height = world.terrain.height(self.position.x, self.position.y);
    }

    /// Pushes the rider out of every overlapping obstacle. A few passes settle
    /// corners where two footprints meet.
    fn resolve_obstacles(&mut self, world: &World) {
        const PASSES: usize = 4;
        for _ in 0..PASSES {
            let mut moved = false;
            for obstacle in &world.obstacles {
                if let Some(push) = self.collider.push_out(self.position, obstacle) {
                    self.position += push;
                    let length = push.length();
                    if length > 0.0 {
                        self.block(push * (1.0 / length));
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
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
    use crate::Aabb;
    use std::f32::consts::FRAC_PI_4;

    const DT: f32 = 1.0 / 60.0;

    fn rider() -> (Rider, RiderParams, World) {
        let world = World::default();
        let r = Rider::new(
            Vec2::new(3.0, -2.0),
            Heading::from_radians(0.0),
            &world.terrain,
        );
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

    #[test]
    fn never_ends_a_step_inside_town_buildings() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let p = RiderParams::default();
        let (start, heading) = town.arrival();
        let mut r = Rider::new(start, heading, &world.terrain);
        let mut contacts = 0;
        for frame in 0..(240.0 / DT) as usize {
            // Wander: gallop with a slowly changing turn.
            let turn = ((frame as f32) * 0.004).sin() * 0.6;
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
                    "frame {frame}: inside {o:?} at {:?}",
                    r.position
                );
                if gap < Rider::COLLIDER_RADIUS + 1e-2 {
                    contacts += 1;
                }
            }
            assert!(r.position.x.is_finite() && r.position.y.is_finite());
        }
        assert!(contacts > 0, "the wander path must actually hit buildings");
    }
}
