//! Kinematic rider (horse + outlaw) model.

use crate::{AnimationState, Circle, Heading, Terrain, Vec2};

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

    /// Advances the rider by `dt` seconds.
    pub fn step(&mut self, input: RiderInput, params: &RiderParams, terrain: &Terrain, dt: f32) {
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let turn = input.turn.clamp(-1.0, 1.0);

        self.heading = self.heading.rotated(turn * params.turn_rate * dt);
        self.speed = next_speed(self.speed, throttle, input.gallop, params, dt);
        self.position += self.heading.forward() * (self.speed * dt);
        self.height = terrain.height(self.position.x, self.position.y);
    }

    pub fn animation(&self, params: &RiderParams) -> AnimationState {
        AnimationState::from_speed(self.speed, params.walk_max)
    }
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
    use std::f32::consts::FRAC_PI_4;

    const DT: f32 = 1.0 / 60.0;

    fn rider() -> (Rider, RiderParams, Terrain) {
        let terrain = Terrain::default();
        let r = Rider::new(Vec2::new(3.0, -2.0), Heading::from_radians(0.0), &terrain);
        (r, RiderParams::default(), terrain)
    }

    fn run(r: &mut Rider, input: RiderInput, p: &RiderParams, t: &Terrain, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            r.step(input, p, t, DT);
        }
    }

    #[test]
    fn new_rider_rests_on_terrain() {
        let (r, p, t) = rider();
        assert_eq!(r.speed, 0.0);
        assert_eq!(r.height, t.height(3.0, -2.0));
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
        assert_eq!(r.height, t.height(r.position.x, r.position.y));
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
}
