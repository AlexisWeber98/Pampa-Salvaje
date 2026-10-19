//! A freight train running along the railway on a fixed, deterministic
//! schedule: it rolls in from the west edge of the patch, brakes to a stop at
//! the station, waits, pulls out to the east, and comes back a period later.
//!
//! The train is kinematic: its state is a pure function of the schedule
//! clock. Its cars are moving obstacles that shove riders off the track.

use crate::{Aabb, BuildingKind, Railway, SideObstacle, TownLayout, Vec2};

/// What a car is, so render can pick its look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CarKind {
    /// Steam locomotive with its tender.
    Locomotive,
    Boxcar,
    /// Open grain hopper.
    Hopper,
}

/// One car of the consist, in order from the front.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Car {
    pub kind: CarKind,
    /// Length over the buffers, in meters.
    pub length: f32,
}

/// Timing and motion of the train's round trip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainSchedule {
    /// Seconds from one arrival to the next. Never shorter than one pass.
    pub period: f32,
    /// Running speed between the patch edge and the station, in m/s.
    pub cruise_speed: f32,
    /// Braking and pulling-away rate, in m/s².
    pub acceleration: f32,
    /// Seconds standing at the station.
    pub dwell: f32,
}

impl Default for TrainSchedule {
    fn default() -> Self {
        Self {
            period: 150.0,
            cruise_speed: 10.0,
            acceleration: 0.5,
            dwell: 25.0,
        }
    }
}

/// Where the train is in its round trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrainPhase {
    /// Rolling in and braking for the station.
    Arriving,
    /// Standing at the station.
    Dwelling,
    /// Pulling out and running off the patch.
    Departing,
    /// Off the patch, waiting for the next run.
    Away,
}

/// Snapshot of the train at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainState {
    pub phase: TrainPhase,
    /// Distance of the locomotive's front along the railway from its start.
    pub front: f32,
    /// Speed along the railway, in m/s (never negative: it only runs east).
    pub speed: f32,
}

/// A car placed on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedCar {
    pub kind: CarKind,
    pub center: Vec2,
    /// Unit direction of travel.
    pub direction: Vec2,
    pub length: f32,
    /// Whether any part of the car is on the drawn railway.
    pub on_rail: bool,
}

/// The train: its consist, its railway and its schedule clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Train {
    pub railway: Railway,
    pub cars: Vec<Car>,
    pub schedule: TrainSchedule,
    /// Distance along the railway where the train's middle stops.
    pub stop_at: f32,
    /// Seconds into the current period.
    time: f32,
}

impl Train {
    /// Width of every car, in meters.
    pub const WIDTH: f32 = 3.0;
    /// Gap between coupled cars, in meters.
    pub const COUPLING: f32 = 1.0;

    /// A locomotive and four wagons on `railway`, stopping with its middle
    /// at `stop_at` meters along it. The clock starts as it rolls in.
    pub fn new(railway: Railway, schedule: TrainSchedule, stop_at: f32) -> Self {
        let cars = [
            (CarKind::Locomotive, 14.0),
            (CarKind::Boxcar, 10.0),
            (CarKind::Hopper, 9.0),
            (CarKind::Hopper, 9.0),
            (CarKind::Boxcar, 10.0),
        ]
        .into_iter()
        .map(|(kind, length)| Car { kind, length })
        .collect();
        Self {
            railway,
            cars,
            schedule,
            stop_at,
            time: 0.0,
        }
    }

    /// The train serving `town`'s station, if the town has a railway. It
    /// stops with its middle alongside the station.
    pub fn for_town(town: &TownLayout, schedule: TrainSchedule) -> Option<Self> {
        let railway = town.railway?;
        let (along, _) = along(&railway);
        let station = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .map_or(railway.start, |b| b.footprint.center());
        Some(Self::new(
            railway,
            schedule,
            (station - railway.start).dot(along),
        ))
    }

    /// Length from the locomotive's front to the last car's end, in meters.
    pub fn length(&self) -> f32 {
        let cars: f32 = self.cars.iter().map(|c| c.length).sum();
        cars + Self::COUPLING * self.cars.len().saturating_sub(1) as f32
    }

    /// Seconds into the current period.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Effective period: the configured one, or one full pass if longer.
    pub fn period(&self) -> f32 {
        let p = self.profile();
        self.schedule.period.max(p.active())
    }

    /// Advances the schedule clock by `dt` seconds. Non-finite or
    /// non-positive `dt` changes nothing.
    pub fn step(&mut self, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        self.time = (self.time + dt).rem_euclid(self.period());
    }

    /// The train at `time` seconds into a period (wrapped into one period).
    pub fn state_at(&self, time: f32) -> TrainState {
        let p = self.profile();
        let t = time.rem_euclid(self.period());
        let (v, a) = (p.speed, p.acceleration);
        let arrive = p.cruise_in + p.braking;
        let depart = arrive + p.dwell;
        let (phase, front, speed) = if t < p.cruise_in {
            (TrainPhase::Arriving, p.start + v * t, v)
        } else if t < arrive {
            let tau = t - p.cruise_in;
            let front = p.stop - p.brake_distance + v * tau - a * tau * tau / 2.0;
            (
                TrainPhase::Arriving,
                front.min(p.stop),
                (v - a * tau).max(0.0),
            )
        } else if t < depart {
            (TrainPhase::Dwelling, p.stop, 0.0)
        } else if t < depart + p.braking {
            let tau = t - depart;
            (TrainPhase::Departing, p.stop + a * tau * tau / 2.0, a * tau)
        } else if t < p.active() {
            let tau = t - depart - p.braking;
            (
                TrainPhase::Departing,
                p.stop + p.brake_distance + v * tau,
                v,
            )
        } else {
            (TrainPhase::Away, p.gone, 0.0)
        };
        TrainState {
            phase,
            front,
            speed,
        }
    }

    /// The train now.
    pub fn state(&self) -> TrainState {
        self.state_at(self.time)
    }

    /// Every car on the ground plane, front first.
    pub fn placed_cars(&self) -> Vec<PlacedCar> {
        let state = self.state();
        let (direction, rail_length) = along(&self.railway);
        let mut behind = 0.0;
        self.cars
            .iter()
            .map(|car| {
                let middle = state.front - behind - car.length / 2.0;
                behind += car.length + Self::COUPLING;
                let half = car.length / 2.0;
                PlacedCar {
                    kind: car.kind,
                    center: self.railway.start + direction * middle,
                    direction,
                    length: car.length,
                    on_rail: state.phase != TrainPhase::Away
                        && middle + half > 0.0
                        && middle - half < rail_length,
                }
            })
            .collect()
    }

    /// Footprints of the cars currently on the railway, as obstacles that
    /// push riders off the track.
    pub fn obstacles(&self) -> Vec<SideObstacle> {
        self.placed_cars()
            .into_iter()
            .filter(|car| car.on_rail)
            .map(|car| {
                let across = Vec2::new(-car.direction.y, car.direction.x);
                let size = car.direction * car.length + across * Self::WIDTH;
                SideObstacle {
                    footprint: Aabb::from_center_size(
                        car.center,
                        Vec2::new(size.x.abs(), size.y.abs()),
                    ),
                    across,
                }
            })
            .collect()
    }

    fn profile(&self) -> Profile {
        let speed = self.schedule.cruise_speed.max(0.1);
        let acceleration = self.schedule.acceleration.max(0.01);
        let (_, rail_length) = along(&self.railway);
        let length = self.length();
        let stop = self.stop_at + length / 2.0;
        let brake_distance = speed * speed / (2.0 * acceleration);
        // Enter at the patch edge, or already braking if the stop is close.
        let cruise_distance = (stop - brake_distance).max(0.0);
        let start = stop - brake_distance - cruise_distance;
        // Run on until the last car has left the railway.
        let gone = (rail_length + length).max(stop + brake_distance);
        Profile {
            speed,
            acceleration,
            start,
            stop,
            gone,
            brake_distance,
            cruise_in: cruise_distance / speed,
            braking: speed / acceleration,
            dwell: self.schedule.dwell.max(0.0),
            cruise_out: (gone - stop - brake_distance) / speed,
        }
    }
}

/// Derived timings and distances of one pass (front positions along the rail).
struct Profile {
    speed: f32,
    acceleration: f32,
    /// Front position as the pass begins.
    start: f32,
    /// Front position while standing at the station.
    stop: f32,
    /// Front position once every car has left the railway.
    gone: f32,
    brake_distance: f32,
    cruise_in: f32,
    /// Seconds to brake from cruise speed (and to pull away again).
    braking: f32,
    dwell: f32,
    cruise_out: f32,
}

impl Profile {
    /// Seconds from rolling in to leaving the railway.
    fn active(&self) -> f32 {
        self.cruise_in + 2.0 * self.braking + self.dwell + self.cruise_out
    }
}

/// Unit direction along `railway` and its length.
fn along(railway: &Railway) -> (Vec2, f32) {
    let d = railway.end - railway.start;
    let length = d.length();
    if length > 0.0 {
        (d * (1.0 / length), length)
    } else {
        (Vec2::new(1.0, 0.0), 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aabb, Circle, Heading, MAX_STEP, Rider, RiderInput, RiderParams, Terrain, World};

    fn town_train() -> (TownLayout, Train) {
        let town = TownLayout::castex_inspired();
        let train = Train::for_town(&town, TrainSchedule::default()).expect("a railway");
        (town, train)
    }

    /// `(start, end)` of each phase in the default town schedule.
    fn phases(train: &Train) -> Vec<(TrainPhase, f32, f32)> {
        let mut out: Vec<(TrainPhase, f32, f32)> = Vec::new();
        let dt = 0.01;
        for k in 0..(train.period() / dt) as usize {
            let t = k as f32 * dt;
            let phase = train.state_at(t).phase;
            match out.last_mut() {
                Some(last) if last.0 == phase => last.2 = t,
                _ => out.push((phase, t, t)),
            }
        }
        out
    }

    #[test]
    fn runs_arrive_dwell_depart_away_in_order() {
        let (_, train) = town_train();
        let p = phases(&train);
        let order: Vec<_> = p.iter().map(|x| x.0).collect();
        use TrainPhase::*;
        assert_eq!(order, [Arriving, Dwelling, Departing, Away]);
        // Default schedule: 10 m/s, 0.5 m/s² (20 s to stop), 25 s dwell.
        let dwell = p[1].2 - p[1].1;
        assert!((dwell - 25.0).abs() < 0.05, "dwell {dwell}");
        assert!((120.0..=180.0).contains(&train.period()));
        assert!(p[3].2 - p[3].1 > 10.0, "a quiet spell between trains");
    }

    #[test]
    fn rolls_in_from_the_west_edge_and_stops_with_its_middle_at_the_station() {
        let (town, train) = town_train();
        let start = train.state_at(0.0);
        assert_eq!(start.phase, TrainPhase::Arriving);
        assert!(start.front.abs() < 1e-3, "front enters at the patch edge");
        assert!((start.speed - 10.0).abs() < 1e-4);

        let station = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .unwrap()
            .footprint;
        let platform = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .unwrap()
            .footprint;
        let (_, a, _) = phases(&train)[1];
        let stopped = train.state_at(a + 1.0);
        assert_eq!(stopped.speed, 0.0);
        let mut at_station = train.clone();
        at_station.time = a + 1.0;
        let cars = at_station.placed_cars();
        let middle = (cars[0].center + cars[cars.len() - 1].center) * 0.5;
        assert!((middle.x - station.center().x).abs() < 1.5, "{middle:?}");
        for car in &cars {
            // Alongside the platform, on the track.
            let half = car.length / 2.0;
            assert!(car.center.x - half >= platform.min.x && car.center.x + half <= platform.max.x);
            assert!((car.center.y - train.railway.start.y).abs() < 1e-4);
        }
    }

    #[test]
    fn motion_is_continuous_and_never_backs_up() {
        let (_, train) = town_train();
        let mut last = train.state_at(0.0);
        let dt = 0.01;
        for k in 1..(train.period() / dt) as usize {
            let s = train.state_at(k as f32 * dt);
            assert!(s.front >= last.front - 1e-3, "backs up at step {k}");
            assert!(
                s.front - last.front <= 10.0 * dt + 1e-3,
                "jumps at step {k}"
            );
            assert!((0.0..=10.0 + 1e-4).contains(&s.speed));
            last = s;
        }
    }

    #[test]
    fn repeats_every_period_and_leaves_the_patch_while_away() {
        let (_, mut train) = town_train();
        let period = train.period();
        for t in [0.0, 13.7, 61.0, 99.0] {
            let (a, b) = (train.state_at(t), train.state_at(t + period));
            assert_eq!(a.phase, b.phase);
            assert!((a.front - b.front).abs() < 1e-2 && (a.speed - b.speed).abs() < 1e-3);
        }
        // Stepping the clock in game-sized steps matches the pure schedule.
        let steps = (200.0 / MAX_STEP) as usize;
        for _ in 0..steps {
            train.step(MAX_STEP);
        }
        let expected = train.state_at(steps as f32 * MAX_STEP);
        assert!((train.state().front - expected.front).abs() < 0.05);

        let away = phases(&train)[3];
        train.time = (away.1 + away.2) / 2.0;
        assert!(train.placed_cars().iter().all(|c| !c.on_rail));
        assert!(train.obstacles().is_empty());
    }

    #[test]
    fn bad_dt_is_a_no_op() {
        let (_, mut train) = town_train();
        train.step(3.0);
        let before = train.clone();
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            train.step(dt);
            assert_eq!(train, before);
        }
    }

    #[test]
    fn cars_are_coupled_in_order_behind_the_locomotive() {
        let (_, mut train) = town_train();
        train.time = 30.0;
        let cars = train.placed_cars();
        assert_eq!(cars.len(), train.cars.len());
        assert_eq!(cars[0].kind, CarKind::Locomotive);
        let front = train.state().front;
        let start = train.railway.start;
        assert!((cars[0].center.x + cars[0].length / 2.0 - (start.x + front)).abs() < 1e-3);
        for pair in cars.windows(2) {
            let gap = (pair[0].center.x - pair[0].length / 2.0)
                - (pair[1].center.x + pair[1].length / 2.0);
            assert!((gap - Train::COUPLING).abs() < 1e-3);
        }
        assert_eq!(
            train.obstacles().len(),
            cars.iter().filter(|c| c.on_rail).count()
        );
    }

    fn world_with_train_at(time: impl Fn(&Train) -> f32) -> (World, Train) {
        let (town, mut train) = town_train();
        let mut world = World::with_town(Terrain::default(), &town);
        train.time = time(&train);
        world.moving = train.obstacles();
        (world, train)
    }

    fn world_with_standing_train() -> (World, Train) {
        world_with_train_at(|train| phases(train)[1].1 + 1.0)
    }

    #[test]
    fn the_train_blocks_the_rider() {
        // Rolling in, west of the station yard: open ground on both sides.
        let (world, train) = world_with_train_at(|_| 15.0);
        let rail_y = train.railway.start.y;
        let car = train.placed_cars()[1];
        assert!(car.on_rail && car.center.x < -100.0);
        let start = Vec2::new(car.center.x, rail_y - 10.0);
        let north = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        let mut r = Rider::new(start, north, &world.terrain);
        let p = RiderParams::default();
        let gallop = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        for _ in 0..(5.0 / MAX_STEP) as usize {
            r.step(gallop, &p, &world, MAX_STEP);
        }
        let face = rail_y - Train::WIDTH / 2.0 - Rider::COLLIDER_RADIUS;
        assert!(
            r.position.y <= face + 1e-3,
            "rode into the train: {:?}",
            r.position
        );
        assert!(r.speed.abs() < 1e-3);
    }

    #[test]
    fn the_train_shoves_a_rider_off_the_track_on_its_own_side() {
        let (world, train) = world_with_standing_train();
        let rail_y = train.railway.start.y;
        let car = train.placed_cars()[2];
        let p = RiderParams::default();
        for (offset, north) in [(0.4, true), (-0.4, false)] {
            let at = Vec2::new(car.center.x, rail_y + offset);
            let mut r = Rider::new(at, Heading::from_radians(0.0), &world.terrain);
            r.step(RiderInput::default(), &p, &world, MAX_STEP);
            let clear = Train::WIDTH / 2.0 + Rider::COLLIDER_RADIUS - 1e-3;
            if north {
                assert!(r.position.y >= rail_y + clear, "{:?}", r.position);
            } else {
                assert!(r.position.y <= rail_y - clear, "{:?}", r.position);
            }
        }
    }

    #[test]
    fn a_rider_pinched_against_the_platform_ends_clear_of_both() {
        let (mut world, train) = world_with_standing_train();
        let platform = TownLayout::castex_inspired()
            .buildings
            .iter()
            .find(|b| b.kind == crate::BuildingKind::Platform)
            .unwrap()
            .footprint;
        let rail_y = train.railway.start.y;
        let car_south = rail_y - Train::WIDTH / 2.0;
        // Freight stacked along the platform edge narrows the gap to the
        // standing train below a rider's width.
        world.obstacles.push(Aabb::new(
            Vec2::new(platform.min.x, platform.max.y),
            Vec2::new(platform.max.x, car_south - 0.5),
        ));
        let platform = world.obstacles[world.obstacles.len() - 1];
        let c = Circle::new(Rider::COLLIDER_RADIUS);
        let mut k = 0;
        while k <= 40 {
            let x = platform.min.x + 1.0 + (platform.size().x - 2.0) * k as f32 / 40.0;
            for y in [platform.max.y + 0.1, car_south - 0.05, car_south + 0.3] {
                let start = Vec2::new(x, y);
                let end = world.resolve(c, start, |_| ());
                let statics = world.obstacles.iter().filter_map(|o| c.push_out(end, o));
                let moving = world.moving.iter().filter_map(|o| o.push_out(c, end));
                let deepest = statics.chain(moving).map(Vec2::length).fold(0.0, f32::max);
                assert!(deepest < 1e-3, "{start:?} -> {end:?} overlaps by {deepest}");
            }
            k += 1;
        }
    }
}
