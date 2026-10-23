//! Wild animals: hares, tero flocks, soaring raptors and ducks (see
//! [`super`]). None of them is an obstacle.

use std::f32::consts::{PI, TAU};

use super::turn_toward;
use crate::rng::Rng;
use crate::{Aabb, Heading, Lagoon, Vec2};

/// What a hare is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HareState {
    /// Sitting up in the grass.
    Sit,
    /// A few slow hops on.
    Hop,
    /// Bolting from a rider.
    Flee,
}

/// A liebre in the open grass.
#[derive(Debug, Clone, PartialEq)]
pub struct Hare {
    pub position: Vec2,
    pub heading: Heading,
    pub state: HareState,
    pub speed: f32,
    /// Meters run since it was seeded: render paces its bounds with it.
    pub travel: f32,
    /// The chunk it was seeded in: it never leaves it, so it streams with it.
    pub home: Aabb,
    timer: f32,
    rng: Rng,
}

impl Hare {
    /// A rider closer than this sets it running, meters.
    pub const ALARM: f32 = 22.0;
    /// It calms down this far from the rider, meters.
    pub const SAFE: f32 = 50.0;
    /// Bolting speed, m/s.
    pub const RUN: f32 = 11.0;
    /// Hopping speed, m/s.
    pub const HOP: f32 = 1.2;

    /// Headings tried, in order, when the way ahead is closed: ever wider
    /// turns either way, then straight back.
    const DETOURS: [f32; 8] = [0.0, 0.8, -0.8, 1.6, -1.6, 2.4, -2.4, PI];

    /// A hare sitting at `position`, kept inside `home`.
    pub fn new(position: Vec2, home: Aabb, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x11a5);
        Self {
            position,
            heading: Heading::from_radians(rng.unit() * TAU),
            state: HareState::Sit,
            speed: 0.0,
            travel: 0.0,
            home,
            timer: rng.range(1.0, 6.0),
            rng,
        }
    }

    /// How high it is off the ground mid-bound, meters.
    pub fn lift(&self) -> f32 {
        let stride = if self.state == HareState::Flee {
            2.2
        } else {
            0.5
        };
        let height = if self.state == HareState::Flee {
            0.35
        } else {
            0.12
        };
        match self.state {
            HareState::Sit => 0.0,
            _ => (self.travel / stride * PI).sin().abs() * height,
        }
    }

    /// Advances `dt` seconds with a rider at `threat`, only ever onto
    /// ground `open` allows inside its home: where the way ahead is closed
    /// it swerves, or turns back.
    pub fn step(&mut self, dt: f32, threat: Option<Vec2>, open: impl Fn(Vec2) -> bool) {
        self.timer -= dt;
        let away = threat.map(|t| (self.position - t, self.position.distance(t)));
        match (self.state, away) {
            (HareState::Flee, Some((_, d))) if d < Self::SAFE || self.timer > 0.0 => {}
            (HareState::Flee, _) => self.settle(),
            (_, Some((dir, d))) if d < Self::ALARM => {
                self.state = HareState::Flee;
                self.heading = Heading::from_radians(dir.y.atan2(dir.x));
                self.speed = Self::RUN;
                self.timer = 2.0;
            }
            (HareState::Hop, _) if self.timer <= 0.0 => self.settle(),
            (HareState::Sit, _) if self.timer <= 0.0 => {
                self.state = HareState::Hop;
                self.heading = self.heading.rotated(self.rng.range(-1.2, 1.2));
                self.speed = Self::HOP;
                self.timer = self.rng.range(0.8, 2.0);
            }
            _ => {}
        }
        if self.state == HareState::Flee {
            // Zigzags as it runs: a jink every so often, always away.
            if let Some((dir, _)) = away {
                let jink = (self.travel * 0.35).sin() * 0.6;
                let off = turn_toward(self.heading, dir) + jink;
                self.heading = self.heading.rotated(off.clamp(-4.0 * dt, 4.0 * dt));
            }
        }
        let step = self.speed * dt;
        if step <= 0.0 {
            return;
        }
        let way = Self::DETOURS.iter().find_map(|&turn| {
            let heading = self.heading.rotated(turn);
            let next = self.position + heading.forward() * step;
            (self.home.contains_point(next) && open(next)).then_some((heading, next))
        });
        match way {
            Some((heading, next)) => {
                self.heading = heading;
                self.position = next;
                self.travel += step;
            }
            // Cornered: it crouches where it is.
            None => self.settle(),
        }
    }

    fn settle(&mut self) {
        self.state = HareState::Sit;
        self.speed = 0.0;
        self.timer = self.rng.range(3.0, 10.0);
    }
}

/// What a tero flock is doing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlockState {
    /// On the ground, pecking.
    Ground,
    /// In the air from `from` to `to`, `t` of the way (`0..1`).
    Flight { from: Vec2, to: Vec2, t: f32 },
}

/// A few teros on the ground that take off together when approached.
#[derive(Debug, Clone, PartialEq)]
pub struct Flock {
    /// Where the flock stands (or flies over).
    pub center: Vec2,
    pub state: FlockState,
    /// Each bird's place round the center.
    offsets: Vec<Vec2>,
    /// The chunk it was seeded in: it always lands inside it.
    pub home: Aabb,
    /// Seconds since seeded, for pecking and flapping.
    pub clock: f32,
    rng: Rng,
}

impl Flock {
    /// A rider closer than this puts them up, meters.
    pub const ALARM: f32 = 18.0;
    /// Flight speed, m/s.
    pub const SPEED: f32 = 9.0;
    /// Height of the arc's top, meters.
    pub const ARC: f32 = 7.0;
    /// Shortest and longest flight, meters.
    pub const FLIGHT: (f32, f32) = (40.0, 70.0);

    /// Landing spots tried before the flock circles back to where it rose.
    const LANDING_TRIES: usize = 8;

    /// `count` teros on the ground at `center`, kept inside `home`.
    pub fn new(center: Vec2, home: Aabb, count: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x7e40);
        let offsets = (0..count)
            .map(|_| Vec2::new(rng.range(-3.0, 3.0), rng.range(-3.0, 3.0)))
            .collect();
        Self {
            center,
            state: FlockState::Ground,
            offsets,
            home,
            clock: 0.0,
            rng,
        }
    }

    /// Whether it is in the air.
    pub fn flying(&self) -> bool {
        matches!(self.state, FlockState::Flight { .. })
    }

    /// Each bird's ground position and height above the ground.
    pub fn birds(&self) -> impl Iterator<Item = (Vec2, f32)> + '_ {
        let height = match self.state {
            FlockState::Ground => 0.0,
            FlockState::Flight { t, .. } => Self::ARC * (PI * t).sin(),
        };
        self.offsets.iter().map(move |&o| (self.center + o, height))
    }

    /// Which way the birds face: along the flight, or a seeded way.
    pub fn heading(&self) -> Heading {
        match self.state {
            FlockState::Flight { from, to, .. } => {
                let d = to - from;
                Heading::from_radians(d.y.atan2(d.x))
            }
            FlockState::Ground => Heading::from_radians(self.offsets.len() as f32),
        }
    }

    /// Advances `dt` seconds with a rider at `threat`: takes off away from
    /// it, flies a low arc and lands on ground `open` allows inside its home
    /// (or circles back to where it rose when no spot ahead will do).
    pub fn step(&mut self, dt: f32, threat: Option<Vec2>, open: impl Fn(Vec2) -> bool) {
        self.clock = crate::tick_clock(self.clock, dt);
        match self.state {
            FlockState::Ground => {
                let Some(rider) = threat else { return };
                let away = self.center - rider;
                if away.length() >= Self::ALARM {
                    return;
                }
                let from = self.center;
                let ahead = away.y.atan2(away.x);
                let to = (0..Self::LANDING_TRIES)
                    .map(|k| {
                        // Ever wider of straight away from the rider.
                        let spread = 0.6 + 0.3 * k as f32;
                        let angle = ahead + self.rng.range(-spread, spread);
                        let reach = self.rng.range(Self::FLIGHT.0, Self::FLIGHT.1);
                        from + Vec2::new(angle.cos(), angle.sin()) * reach
                    })
                    .find(|&to| self.home.contains_point(to) && open(to))
                    .unwrap_or(from);
                self.state = FlockState::Flight { from, to, t: 0.0 };
            }
            FlockState::Flight { from, to, t } => {
                // At least the shortest flight's time, even circling back.
                let t = t + dt * Self::SPEED / from.distance(to).max(Self::FLIGHT.0);
                if t >= 1.0 {
                    self.center = to;
                    self.state = FlockState::Ground;
                } else {
                    self.center = from + (to - from) * t;
                    self.state = FlockState::Flight { from, to, t };
                }
            }
        }
    }
}

/// A chimango or carancho circling slowly overhead.
#[derive(Debug, Clone, PartialEq)]
pub struct Raptor {
    /// The middle of its circle.
    pub center: Vec2,
    pub radius: f32,
    pub altitude: f32,
    /// Radians round the circle.
    pub angle: f32,
    /// Radians per second (negative: clockwise).
    pub rate: f32,
}

impl Raptor {
    pub fn new(center: Vec2, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0xca7a);
        let radius = rng.range(30.0, 60.0);
        // About 7 m/s round the circle, either way.
        let rate = 7.0 / radius * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        Self {
            center,
            radius,
            altitude: rng.range(30.0, 55.0),
            angle: rng.unit() * TAU,
            rate,
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.angle = (self.angle + self.rate * dt).rem_euclid(TAU);
    }

    /// Where it is over the ground.
    pub fn position(&self) -> Vec2 {
        self.center + Vec2::new(self.angle.cos(), self.angle.sin()) * self.radius
    }

    /// Facing along the circle.
    pub fn heading(&self) -> Heading {
        Heading::from_radians(self.angle + PI / 2.0 * self.rate.signum())
    }

    /// Its bank into the turn, radians (positive: left wing down).
    pub fn bank(&self) -> f32 {
        0.35 * self.rate.signum()
    }
}

/// A duck paddling on a lagoon.
#[derive(Debug, Clone, PartialEq)]
pub struct Duck {
    /// Its raft's middle, where it streams from.
    pub home: Vec2,
    pub position: Vec2,
    pub heading: Heading,
    /// Index into the site's lagoons.
    pub lagoon: usize,
    target: Vec2,
    rng: Rng,
}

impl Duck {
    /// A rider closer than this makes it paddle off, meters.
    pub const ALARM: f32 = 15.0;
    /// Paddling speeds, calm and hurried, m/s.
    pub const PADDLE: (f32, f32) = (0.3, 1.5);

    pub fn new(home: Vec2, position: Vec2, lagoon: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0xd0c);
        Self {
            home,
            position,
            heading: Heading::from_radians(rng.unit() * TAU),
            lagoon,
            target: position,
            rng,
        }
    }

    /// Advances `dt` seconds on `lagoon` with a rider at `threat`: drifts
    /// round its raft, paddling off from riders, and never leaves deep water.
    pub fn step(&mut self, dt: f32, lagoon: &Lagoon, threat: Option<Vec2>) {
        let alarmed = threat.filter(|t| t.distance(self.position) < Self::ALARM);
        if let Some(rider) = alarmed {
            let away = self.position - rider;
            self.target = self.position + away * (8.0 / away.length().max(1e-3));
        } else if self.position.distance(self.target) < 0.5 {
            let r = &mut self.rng;
            self.target = self.home + Vec2::new(r.range(-12.0, 12.0), r.range(-12.0, 12.0));
        }
        let to = self.target - self.position;
        let turn = turn_toward(self.heading, to);
        self.heading = self.heading.rotated(turn.clamp(-1.5 * dt, 1.5 * dt));
        let speed = if alarmed.is_some() {
            Self::PADDLE.1
        } else {
            Self::PADDLE.0
        };
        let next = self.position + self.heading.forward() * (speed * dt);
        if lagoon.is_deep(next) {
            self.position = next;
        } else {
            // Turn back to the raft rather than climb out.
            self.target = self.home;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LagoonConfig, Terrain};

    #[test]
    fn a_hare_bolts_from_a_rider_and_calms_down_once_far() {
        let home = Aabb::from_center_size(Vec2::ZERO, Vec2::new(400.0, 400.0));
        let mut hare = Hare::new(Vec2::new(0.0, 0.0), home, 3);
        let far = Vec2::new(200.0, 0.0);
        for _ in 0..100 {
            hare.step(0.1, Some(far), |_| true);
            assert_ne!(hare.state, HareState::Flee);
        }
        let rider = hare.position + Vec2::new(10.0, 0.0);
        hare.step(0.1, Some(rider), |_| true);
        assert_eq!(hare.state, HareState::Flee);
        let mut fled = 0.0;
        for _ in 0..100 {
            hare.step(0.1, Some(rider), |_| true);
            fled = hare.position.distance(rider);
            if hare.state != HareState::Flee {
                break;
            }
            assert!(hare.lift() >= 0.0 && hare.lift() <= 0.4);
        }
        assert_eq!(hare.state, HareState::Sit, "calmed down");
        assert!(fled >= Hare::SAFE, "{fled}");
        assert!(hare.position.x < 0.0, "ran away from the rider");
    }

    #[test]
    fn teros_take_off_away_from_a_rider_and_land_further_on() {
        let start = Vec2::new(50.0, 50.0);
        let home = Aabb::from_center_size(start, Vec2::new(400.0, 400.0));
        let mut flock = Flock::new(start, home, 4, 9);
        flock.step(0.1, Some(start + Vec2::new(100.0, 0.0)), |_| true);
        assert!(!flock.flying());
        let rider = start + Vec2::new(10.0, 0.0);
        flock.step(0.1, Some(rider), |_| true);
        assert!(flock.flying());
        let mut top: f32 = 0.0;
        for _ in 0..200 {
            flock.step(0.1, None, |_| true);
            top = top.max(flock.birds().map(|b| b.1).fold(0.0, f32::max));
            if !flock.flying() {
                break;
            }
        }
        assert!(!flock.flying(), "landed");
        assert!(top > Flock::ARC * 0.8, "{top}");
        assert!(flock.birds().all(|b| b.1 == 0.0));
        let flown = flock.center.distance(start);
        assert!((Flock::FLIGHT.0..=Flock::FLIGHT.1 + 1.0).contains(&flown));
        assert!(flock.center.distance(rider) > start.distance(rider));
    }

    #[test]
    fn a_hare_bolting_toward_a_lagoon_swerves_and_stays_home() {
        let config = LagoonConfig {
            name: "Prueba".into(),
            center: Vec2::new(0.0, 0.0),
            size: Vec2::new(200.0, 200.0),
            seed: 2,
        };
        let lagoon = Lagoon::new(&config, &Terrain::default());
        let open = |p: Vec2| lagoon.offshore(p) > 10.0;
        let home = Aabb::new(Vec2::new(100.0, -60.0), Vec2::new(200.0, 60.0));
        for seed in 0..20 {
            let mut hare = Hare::new(Vec2::new(125.0, 5.0 * seed as f32 - 50.0), home, seed);
            // The rider comes from the east: away is into the water.
            let rider = hare.position + Vec2::new(12.0, 0.0);
            for _ in 0..300 {
                hare.step(0.1, Some(rider), open);
                assert!(open(hare.position), "in the lagoon: {:?}", hare.position);
                assert!(home.contains_point(hare.position), "left home");
            }
        }
    }

    #[test]
    fn teros_only_ever_land_on_open_ground_at_home() {
        let start = Vec2::new(0.0, 0.0);
        let home = Aabb::from_center_size(start, Vec2::new(200.0, 200.0));
        // Everything east of the flock is closed (a road, say).
        let open = |p: Vec2| p.x <= 5.0;
        for seed in 0..40 {
            let mut flock = Flock::new(start, home, 4, seed);
            // The rider comes from the west: away is east, closed.
            let rider = start - Vec2::new(10.0, 0.0);
            for _ in 0..600 {
                flock.step(0.1, Some(rider), open);
                if !flock.flying() {
                    assert!(open(flock.center) && home.contains_point(flock.center));
                }
            }
        }
        // Nowhere to go: it circles back to where it rose.
        let mut flock = Flock::new(start, home, 4, 1);
        flock.step(0.1, Some(start + Vec2::new(5.0, 0.0)), |p| p == start);
        assert!(flock.flying());
        for _ in 0..100 {
            flock.step(0.1, None, |p| p == start);
        }
        assert!(!flock.flying() && flock.center == start);
    }

    #[test]
    fn a_raptor_circles_high_and_slowly() {
        let mut bird = Raptor::new(Vec2::new(0.0, 0.0), 4);
        let mut last = bird.position();
        for _ in 0..1000 {
            bird.step(0.1);
            let p = bird.position();
            assert!((p.length() - bird.radius).abs() < 1e-2);
            assert!(p.distance(last) < 1.0, "about 7 m/s");
            last = p;
        }
        assert!((30.0..=55.0).contains(&bird.altitude));
    }

    #[test]
    fn ducks_stay_on_deep_water_and_paddle_off_from_riders() {
        let config = LagoonConfig {
            name: "Prueba".into(),
            center: Vec2::new(0.0, 0.0),
            size: Vec2::new(300.0, 200.0),
            seed: 2,
        };
        let lagoon = Lagoon::new(&config, &Terrain::default());
        let mut duck = Duck::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0), 0, 5);
        for _ in 0..3000 {
            duck.step(0.1, &lagoon, None);
            assert!(lagoon.is_deep(duck.position));
        }
        let rider = duck.position + Vec2::new(5.0, 0.0);
        let before = duck.position.distance(rider);
        for _ in 0..50 {
            duck.step(0.1, &lagoon, Some(rider));
        }
        assert!(duck.position.distance(rider) > before + 2.0);
    }
}
