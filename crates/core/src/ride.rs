//! Riding the line train: boarding at a station, sitting in a coach while
//! the train runs, getting off onto a platform, skipping to the next stop
//! and waiting on a platform for the next train.
//!
//! Every rule here is pure. The game reads the keys, plays the fade and
//! moves the player; the clock and the train jump through
//! [`skip_ride`] and [`wait_for_train`], which land the train exactly where
//! following the clock would ([`LineTrain::state_at`]).

use crate::{
    BASE_RATE, CarKind, Circle, Floor, GameClock, Heading, LinePhase, LineTrain, LineTrainState,
    PlacedCar, RailBay, Rider, Run, TimeOfDay, TownLayout, Travel, Vec2, World,
};

/// The clock's multiplier while the player rides the train: 12× instead of
/// the base 30×, so the whole line (about 11 real minutes) reads as about
/// two game hours.
pub const ONBOARD_MULTIPLIER: f64 = 0.4;

/// Farthest a person may stand from a coach door to board, in meters. A
/// lone coach (the shuttle's) stops with its doors 3.4 m along the platform
/// from the middle spot and 3.5 m across, so 5 reaches them from there.
pub const BOARD_RANGE: f32 = 5.0;

/// Farthest from the middle of a platform ([`TownLayout::platform_spot`]) a
/// person may stand to wait for the train there, in meters.
pub const WAIT_RANGE: f32 = 8.0;

/// Height of a seated passenger's feet above the rail tops, in meters.
pub const SEAT_HEIGHT: f32 = 1.0;

/// Real seconds the game starts before the morning train pulls into the
/// first town: long enough to watch it come in, short enough not to bore.
pub const OPENING_LEAD: f64 = 50.0;

/// Real seconds before the next station a skipped ride lands, so the
/// train still comes in and brakes to a stop there.
pub const APPROACH_LEAD: f64 = 3.0;

/// How far in from each end of the platform a passenger gets off, at most,
/// in meters.
const PLATFORM_END_MARGIN: f32 = 2.0;

/// How far in from each end of a coach its end doors are, in meters.
const DOOR_INSET: f32 = 0.6;

/// The player riding a train.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ride {
    /// The train, by index into the world's trains (the line train is 0).
    /// [`Self::new`] says 0; [`boarding`] and [`Self::on_train`] set it.
    pub train: usize,
    /// The coach the player sits in, by index into [`LineTrain::cars`].
    pub car: usize,
    /// The station (index into [`LineTrain::stations`]) where the player is
    /// put off as soon as the train stands there, if any (the opening).
    pub alight_at: Option<usize>,
    /// Which side of the coach the player sits at the window: `1.0` on the
    /// car's right (looking the way it runs), `-1.0` on its left. Chosen
    /// facing the next stop's platform ([`Self::facing_platform`]).
    pub side: f32,
    /// Whether the train has run since the player boarded: arriving at a
    /// terminal then ends the ride, while boarding a train laid over at one
    /// does not.
    under_way: bool,
}

/// What the interact key does beside the line train.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Boarding {
    /// Get on: the ride in that coach.
    Board(Ride),
    /// Mounted at a coach door: the horse stays on the platform, so the
    /// player has to get off it first.
    DismountFirst,
    /// No train to board here.
    Nothing,
}

impl Ride {
    /// A ride in coach `car` of `train`, from where it stands now.
    pub fn new(train: &LineTrain, car: usize) -> Self {
        Self {
            train: 0,
            car,
            alight_at: None,
            side: 1.0,
            under_way: train.state().phase != LinePhase::Parked,
        }
    }

    /// This ride on the train at index `train` of the world's trains.
    pub fn on_train(self, train: usize) -> Self {
        Self { train, ..self }
    }

    /// This ride seated on the side of the coach that faces the platform
    /// of the train's next stop (from a station, the one after it), so the
    /// arrival is seen through the window (a stop in a bay has the bay's
    /// platform, one of `floors`). Unchanged without one.
    pub fn facing_platform(
        self,
        train: &LineTrain,
        towns: &[TownLayout],
        floors: &[Floor],
    ) -> Self {
        let mut there = train.clone();
        let Some((station, _)) = there.skip_to_next_stop(0.0, BASE_RATE) else {
            return self;
        };
        let Some(car) = there.placed_cars().get(self.car).copied() else {
            return self;
        };
        let stop = train.stations().get(station);
        let platform = if stop.is_some_and(|s| s.bay) {
            nearest_floor(floors, car.center).map(|f| f.spot_beside(car.center).0)
        } else {
            stop.and_then(|s| towns.get(s.town))
                .and_then(|t| t.platform_spot())
                .map(|(at, _)| at)
        };
        let Some(platform) = platform else {
            return self;
        };
        let right = Vec2::new(car.direction.y, -car.direction.x);
        let side = if (platform - car.center).dot(right) < 0.0 {
            -1.0
        } else {
            1.0
        };
        Self { side, ..self }
    }

    /// A ride in coach `car` that ends at `station`: the player is put off
    /// there as soon as the train stands at it.
    pub fn to_station(train: &LineTrain, car: usize, station: usize) -> Self {
        Self {
            alight_at: Some(station),
            ..Self::new(train, car)
        }
    }

    /// Where the seated passenger is: the middle of the coach, facing the
    /// way the train runs. `None` for a car the train lacks.
    pub fn seat(&self, train: &LineTrain) -> Option<(Vec2, Heading)> {
        let coach = *train.placed_cars().get(self.car)?;
        let d = coach.direction;
        Some((coach.center, Heading::from_radians(d.y.atan2(d.x))))
    }

    /// Follows the train to its state `now`; `true` when the ride ends here
    /// and the player must get off: at the station the ride ends at, or at
    /// a terminal the train arrived at with the player aboard.
    pub fn put_off(&mut self, now: &LineTrainState) -> bool {
        if now.phase != LinePhase::Parked {
            self.under_way = true;
        }
        let arrived = self
            .alight_at
            .is_some_and(|station| stands_at(now, station));
        arrived || (now.phase == LinePhase::Parked && self.under_way)
    }
}

/// Whether the train stands (dwells, or lays over) at `station`.
pub fn stands_at(state: &LineTrainState, station: usize) -> bool {
    state.phase != LinePhase::Running && state.station == Some(station)
}

/// The two end doors on each side of a coach, on its sides.
pub fn coach_doors(car: &PlacedCar) -> [Vec2; 4] {
    let d = car.direction;
    let along = d * (car.length / 2.0 - DOOR_INSET);
    let across = Vec2::new(-d.y, d.x) * (LineTrain::WIDTH / 2.0);
    [
        car.center + along + across,
        car.center + along - across,
        car.center - along + across,
        car.center - along - across,
    ]
}

/// The coach of `train` whose door is nearest `rider`, and the distance to
/// it, while the train stands at a station and the rider is on a platform
/// (one of `floors`).
fn nearest_coach_door(train: &LineTrain, rider: &Rider, floors: &[Floor]) -> Option<(usize, f32)> {
    let state = train.state();
    if state.phase == LinePhase::Running || state.station.is_none() {
        return None;
    }
    let at = rider.position;
    if !floors.iter().any(|f| f.footprint.contains_point(at)) {
        return None;
    }
    train
        .placed_cars()
        .iter()
        .enumerate()
        .filter(|(_, car)| car.kind == CarKind::Coach)
        .flat_map(|(i, car)| coach_doors(car).map(|door| (i, door.distance(at))))
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// What the interact key does for `rider` beside the `trains`: board the
/// coach whose door is within [`BOARD_RANGE`] while its train stands at a
/// station and the rider is on foot on a platform (one of `floors`: a
/// station's or a bay's). Of several trains in reach, the one with the
/// nearest door wins.
pub fn boarding(trains: &[LineTrain], rider: &Rider, floors: &[Floor]) -> Boarding {
    let nearest = trains
        .iter()
        .enumerate()
        .filter_map(|(t, train)| {
            nearest_coach_door(train, rider, floors).map(|(c, gap)| (t, c, gap))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2));
    match nearest {
        Some((t, car, gap)) if gap <= BOARD_RANGE => match rider.travel() {
            Travel::OnFoot => Boarding::Board(Ride::new(&trains[t], car).on_train(t)),
            Travel::Riding => Boarding::DismountFirst,
        },
        _ => Boarding::Nothing,
    }
}

/// Where a passenger in coach `car` gets off while the train stands at a
/// station: on top of that town's platform (a bay train's, the bay's
/// platform among `world`'s floors) beside the coach's door nearest the
/// platform's middle (so F boards again from there), a step in from its
/// track edge, facing away from the track, pushed clear of everything in
/// `world` (whose moving obstacles should be the train's cars). `None` while
/// the train runs or when the station's town has no platform.
pub fn alight_spot(
    train: &LineTrain,
    car: usize,
    towns: &[TownLayout],
    world: &World,
) -> Option<(Vec2, Heading)> {
    let state = train.state();
    if state.phase == LinePhase::Running {
        return None;
    }
    let station = *train.stations().get(state.station?)?;
    let coach = *train.placed_cars().get(car)?;
    let (floor, middle, facing) = if station.bay {
        let floor = *world.floors.iter().min_by(|a, b| {
            let gap = |f: &Floor| {
                f.footprint
                    .closest_point(coach.center)
                    .distance(coach.center)
            };
            gap(a).total_cmp(&gap(b))
        })?;
        let (middle, facing) = floor.spot_beside(coach.center);
        (floor, middle, facing)
    } else {
        let town = towns.get(station.town)?;
        let (middle, facing) = town.platform_spot()?;
        (*town.floors().first()?, middle, facing)
    };
    let door = coach_doors(&coach)
        .into_iter()
        .min_by(|a, b| a.distance(middle).total_cmp(&b.distance(middle)))?;
    let wanted = beside_door(&floor, middle, door);
    let person = Circle::new(Rider::PERSON_RADIUS);
    let settled = world.resolve(person, wanted, |_| ());
    let spot = if floor.footprint.contains_point(settled) {
        settled
    } else {
        world.resolve(person, middle, |_| ())
    };
    Some((spot, facing))
}

/// The floor of `floors` nearest `at`.
fn nearest_floor(floors: &[Floor], at: Vec2) -> Option<Floor> {
    let gap = |f: &&Floor| f.footprint.closest_point(at).distance(at);
    floors
        .iter()
        .min_by(|a, b| gap(a).total_cmp(&gap(b)))
        .copied()
}

/// The platform's `middle` spot slid along `floor` to beside `door`,
/// staying off its ramps, whichever way `floor.along` points.
fn beside_door(floor: &Floor, middle: Vec2, door: Vec2) -> Vec2 {
    let along = floor.along;
    let (min, max) = (floor.footprint.min, floor.footprint.max);
    let (lo, hi) = [min, max, Vec2::new(min.x, max.y), Vec2::new(max.x, min.y)]
        .into_iter()
        .map(|corner| corner.dot(along))
        .fold((f32::MAX, f32::MIN), |(lo, hi), a| (lo.min(a), hi.max(a)));
    let (lo, hi) = (lo + PLATFORM_END_MARGIN, hi - PLATFORM_END_MARGIN);
    let a = door.dot(along);
    let a = if lo <= hi {
        a.clamp(lo, hi)
    } else {
        middle.dot(along)
    };
    middle + along * (a - middle.dot(along))
}

/// A platform to wait on for a train: a town's station platform, or the
/// bay platform beside it (the branch shuttle's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitingPlace {
    /// Index (into the world's towns) of the platform's town.
    pub town: usize,
    /// Whether it is the town's bay platform instead of its station's.
    pub bay: bool,
}

impl WaitingPlace {
    /// The station of `train` that stops at this platform: the one at its
    /// town on its bay track or not, as the platform is.
    fn station_of(self, train: &LineTrain) -> Option<usize> {
        train
            .stations()
            .iter()
            .position(|s| s.town == self.town && s.bay == self.bay)
    }
}

/// The platform `rider` waits on: on foot within [`WAIT_RANGE`] of the
/// middle of a town's station platform, or on a bay platform (one of
/// `bays`), that some train stops at, while none stands beside it (then the
/// rider boards instead). Trains stopping at the town's other platform do
/// not count: from the station platform the rider waits for the trains
/// there, not the shuttle in its bay.
pub fn waiting_place(
    trains: &[LineTrain],
    towns: &[TownLayout],
    bays: &[RailBay],
    rider: &Rider,
) -> Option<WaitingPlace> {
    if rider.travel() != Travel::OnFoot {
        return None;
    }
    let at_station = trains
        .iter()
        .flat_map(|train| train.stations())
        .map(|station| WaitingPlace {
            town: station.town,
            bay: false,
        })
        .find(|place| {
            towns
                .get(place.town)
                .and_then(|t| t.platform_spot())
                .is_some_and(|(middle, _)| middle.distance(rider.position) <= WAIT_RANGE)
        });
    let at_bay = bays
        .iter()
        .find(|bay| bay.platform.footprint.contains_point(rider.position))
        .map(|bay| WaitingPlace {
            town: bay.town,
            bay: true,
        });
    at_station.or(at_bay).filter(|place| {
        let serving = || {
            trains
                .iter()
                .filter_map(|t| Some((t, place.station_of(t)?)))
        };
        serving().next().is_some()
            && !serving().any(|(train, station)| stands_at(&train.state(), station))
    })
}

/// Fast-forwards the clock (at the base rate) and every train until one of
/// them next arrives at `place` (whichever comes first), and returns the
/// game seconds skipped (zero when one already stands there). The next
/// day's train if none comes sooner.
pub fn wait_for_train(
    trains: &mut [LineTrain],
    clock: &mut GameClock,
    place: WaitingPlace,
) -> Option<f64> {
    let now = clock.elapsed();
    let wait = trains
        .iter()
        .filter_map(|train| train.time_until_at(place.station_of(train)?, now))
        .min_by(f64::total_cmp)?;
    if wait <= 0.0 {
        return Some(0.0);
    }
    for train in trains {
        train.advance(now, wait, BASE_RATE);
    }
    Some(clock.advance_by(wait))
}

/// Brings every train but `ridden` along with a clock jump of `game`
/// seconds from game time `from` at `rate` game seconds per real second,
/// after [`skip_ride`], [`depart_now`] or [`skip_to_approach`] moved the
/// ridden one by itself.
pub fn follow_jump(trains: &mut [LineTrain], ridden: usize, from: f64, game: f64, rate: f64) {
    for (i, train) in trains.iter_mut().enumerate() {
        if i != ridden {
            train.advance(from, game, rate);
        }
    }
}

/// Skips the rest of the ride to the train's next stop: the clock jumps by
/// the game time the ride would take at its current (onboard) rate, and the
/// train to its arrival there. Returns that station.
pub fn skip_ride(train: &mut LineTrain, clock: &mut GameClock) -> Option<usize> {
    let (station, game) = train.skip_to_next_stop(clock.elapsed(), clock.rate())?;
    clock.advance_by(game);
    Some(station)
}

/// Pulls the train out of the station it stands at now (the rest of the
/// dwell, or the wait for a terminal's departure), the clock jumping by the
/// same game time at its current rate. Returns the game seconds jumped:
/// zero for a train already running.
pub fn depart_now(train: &mut LineTrain, clock: &mut GameClock) -> Option<f64> {
    let game = train.skip_to_departure(clock.elapsed(), clock.rate())?;
    clock.advance_by(game);
    Some(game)
}

/// Skips the ride to [`APPROACH_LEAD`] real seconds before the next stop,
/// the clock jumping by the game time that takes at its current rate, so
/// the train comes in for real from there. Returns that station.
pub fn skip_to_approach(train: &mut LineTrain, clock: &mut GameClock) -> Option<usize> {
    let (station, game) =
        train.skip_toward_next_stop(clock.elapsed(), clock.rate(), APPROACH_LEAD)?;
    clock.advance_by(game);
    Some(station)
}

/// The next station the train arrives at and the time of day it does, with
/// the clock running at its current rate.
pub fn next_arrival(train: &LineTrain, clock: &GameClock) -> Option<(usize, TimeOfDay)> {
    let (station, game) = train.next_arrival(clock.elapsed(), clock.rate())?;
    Some((station, TimeOfDay::from_seconds(clock.elapsed() + game)))
}

/// The game's opening: the train placed [`OPENING_LEAD`] real seconds
/// before `run` arrives at the station of `town`, the clock reading so
/// that riding on at the onboard rate brings it there at its timetabled
/// arrival (base rate), and the player seated in the first coach, to be
/// put off at that station. `None` when the town has no station or the
/// train no coach.
pub fn opening(train: &mut LineTrain, town: usize, run: Run) -> Option<(GameClock, Ride)> {
    let station = train.station_of_town(town)?;
    let car = train.cars().iter().position(|c| c.kind == CarKind::Coach)?;
    let (arrive, _) = train.stop_times(run, station)?;
    let arrival = train.arrival_time_at(run, station)?;
    let lead = OPENING_LEAD.min(arrive);
    train.place_on_run(run, arrive - lead);
    let mut clock = GameClock::at(TimeOfDay::from_seconds(
        arrival.seconds() - lead * ONBOARD_MULTIPLIER * BASE_RATE,
    ));
    clock.multiplier = ONBOARD_MULTIPLIER;
    Some((clock, Ride::to_station(train, car, station)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Terrain, WorldConfig};

    /// Castex, Monte Nievas and Pico as shipped, their world with the train
    /// as moving obstacles, and the train.
    fn shipped() -> (Vec<TownLayout>, World, LineTrain) {
        let (towns, line) = crate::rail::tests::shipped();
        let train = LineTrain::new(&line, &towns, crate::Timetable::default()).unwrap();
        let world = World::with_towns(
            WorldConfig {
                half_extent: WorldConfig::MAX_HALF_EXTENT,
                ..Default::default()
            },
            Terrain::default(),
            &towns,
        );
        (towns, world, train)
    }

    const CASTEX: usize = 0;
    const NIEVAS: usize = 1;
    const PICO: usize = 2;

    fn hm(h: u32, m: u32) -> f64 {
        TimeOfDay::hm(h, m).seconds()
    }

    /// The train jumped from midnight to game time `at`.
    fn at(train: &LineTrain, at: f64) -> LineTrain {
        let mut train = train.clone();
        train.advance(0.0, at, BASE_RATE);
        train
    }

    /// The train standing at `station` on its next visit after `from`.
    fn standing(train: &LineTrain, from: f64, station: usize) -> (LineTrain, f64) {
        let mut train = at(train, from);
        let wait = train.time_until_at(station, from).unwrap();
        train.advance(from, wait, BASE_RATE);
        (train, from + wait)
    }

    fn town_of<'a>(towns: &'a [TownLayout], train: &LineTrain, station: usize) -> &'a TownLayout {
        &towns[train.stations()[station].town]
    }

    fn coaches(train: &LineTrain) -> Vec<usize> {
        (0..train.cars().len())
            .filter(|&i| train.cars()[i].kind == CarKind::Coach)
            .collect()
    }

    fn on_foot(at: Vec2, world: &World) -> Rider {
        Rider::on_foot(at, Heading::from_radians(0.0), world)
    }

    /// The unit direction across coach `car` toward the platform.
    fn to_platform(train: &LineTrain, car: usize, towns: &[TownLayout]) -> Vec2 {
        let station = train.state().station.unwrap();
        let (middle, _) = town_of(towns, train, station).platform_spot().unwrap();
        let c = train.placed_cars()[car];
        let across = Vec2::new(-c.direction.y, c.direction.x);
        across * (middle - c.center).dot(across).signum()
    }

    /// A spot on the platform 3 m out from the coach's door on that side
    /// nearest the platform's middle.
    fn by_the_door(train: &LineTrain, car: usize, towns: &[TownLayout]) -> Vec2 {
        let side = to_platform(train, car, towns);
        let c = train.placed_cars()[car];
        let station = train.state().station.unwrap();
        let (middle, _) = town_of(towns, train, station).platform_spot().unwrap();
        let door = coach_doors(&c)
            .into_iter()
            .filter(|d| (*d - c.center).dot(side) > 0.0)
            .min_by(|a, b| a.distance(middle).total_cmp(&b.distance(middle)))
            .unwrap();
        door + side * 3.0
    }

    #[test]
    fn a_passenger_on_foot_boards_a_standing_train_from_its_platform() {
        let (towns, world, train) = shipped();
        let (train, _) = standing(&train, 0.0, NIEVAS);
        assert_eq!(train.state().phase, LinePhase::Dwelling);
        let floors = &world.floors;
        for car in coaches(&train) {
            let spot = by_the_door(&train, car, &towns);
            assert!(floors.iter().any(|f| f.footprint.contains_point(spot)));
            let rider = on_foot(spot, &world);
            match boarding(std::slice::from_ref(&train), &rider, floors) {
                Boarding::Board(ride) => {
                    assert!(coaches(&train).contains(&ride.car));
                    assert_eq!(ride.alight_at, None);
                }
                other => panic!("car {car}: {other:?}"),
            }
        }
        // The old platform start, beside the middle of the coaches, is in
        // reach of a door.
        let (middle, _) = town_of(&towns, &train, NIEVAS).platform_spot().unwrap();
        assert!(matches!(
            boarding(
                std::slice::from_ref(&train),
                &on_foot(middle, &world),
                floors
            ),
            Boarding::Board(_)
        ));
    }

    #[test]
    fn no_boarding_while_running_mounted_far_away_or_off_the_platform() {
        let (towns, world, train) = shipped();
        let floors = &world.floors;
        let (dwelling, _) = standing(&train, 0.0, NIEVAS);
        let car = coaches(&dwelling)[0];
        let spot = by_the_door(&dwelling, car, &towns);
        // Mounted: get off the horse first.
        let mounted = Rider::new(spot, Heading::from_radians(0.0), &world);
        assert_eq!(
            boarding(std::slice::from_ref(&dwelling), &mounted, floors),
            Boarding::DismountFirst
        );
        // Too far along the platform from every door.
        let far = spot + dwelling.placed_cars()[car].direction * 40.0;
        assert_eq!(
            boarding(
                std::slice::from_ref(&dwelling),
                &on_foot(far, &world),
                floors
            ),
            Boarding::Nothing
        );
        // On the far side of the track: not on the platform.
        let c = dwelling.placed_cars()[car];
        let beyond = c.center - to_platform(&dwelling, car, &towns) * 3.0;
        assert!(!floors.iter().any(|f| f.footprint.contains_point(beyond)));
        assert_eq!(
            boarding(
                std::slice::from_ref(&dwelling),
                &on_foot(beyond, &world),
                floors
            ),
            Boarding::Nothing
        );
        // A moment after it pulls out: nothing to board.
        let mut gone = dwelling.clone();
        let run = gone.state().run;
        let (_, depart) = gone.stop_times(run, NIEVAS).unwrap();
        gone.place_on_run(run, depart + 1.0);
        assert_eq!(gone.state().phase, LinePhase::Running);
        assert_eq!(
            boarding(std::slice::from_ref(&gone), &on_foot(spot, &world), floors),
            Boarding::Nothing
        );
    }

    #[test]
    fn the_seat_follows_the_coach() {
        let (_, _, train) = shipped();
        let train = at(&train, hm(7, 3));
        let car = coaches(&train)[1];
        let ride = Ride::new(&train, car);
        let (seat, heading) = ride.seat(&train).unwrap();
        let coach = train.placed_cars()[car];
        assert_eq!(seat, coach.center);
        assert!(heading.forward().distance(coach.direction) < 1e-4);
        let gone = Ride { car: 99, ..ride };
        assert_eq!(gone.seat(&train), None);
    }

    #[test]
    fn passengers_get_off_on_the_platform_beside_their_coach_clear_of_everything() {
        let (towns, mut world, train) = shipped();
        // Monte Nievas mid-run, and both terminals with the consist
        // centred on the station.
        for (from, station) in [(0.0, NIEVAS), (hm(11, 0), CASTEX), (hm(18, 0), PICO)] {
            let (train, _) = standing(&train, from, station);
            world.moving = train.obstacles();
            let town = town_of(&towns, &train, station);
            let floor = town.floors()[0];
            let (_, facing) = town.platform_spot().unwrap();
            for car in coaches(&train) {
                let (spot, heading) = alight_spot(&train, car, &towns, &world).expect("a spot");
                assert!(floor.footprint.contains_point(spot), "{station}: {spot:?}");
                assert_eq!(heading, facing);
                let person = Circle::new(Rider::PERSON_RADIUS);
                assert_eq!(world.resolve(person, spot, |_| ()), spot, "clear");
                let coach = train.placed_cars()[car];
                let along = (spot - coach.center).dot(floor.along).abs();
                assert!(along <= coach.length / 2.0 + 0.5, "beside it: {along}");
                // Within reach of a door, to board again.
                assert!(matches!(
                    boarding(
                        std::slice::from_ref(&train),
                        &on_foot(spot, &world),
                        &world.floors
                    ),
                    Boarding::Board(_)
                ));
                // On top of the platform.
                assert_eq!(
                    world.ground_height(spot),
                    world.terrain.height(spot.x, spot.y) + floor.height
                );
            }
        }
        // Not while it runs.
        let running = at(&train, hm(7, 3));
        assert_eq!(alight_spot(&running, 1, &towns, &world), None);
    }

    #[test]
    fn the_alighting_spot_slides_beside_the_door_whichever_way_the_platform_runs() {
        // A 40 m platform along x, then along y, each named both ways.
        let x = crate::Aabb::new(Vec2::new(-20.0, 4.0), Vec2::new(20.0, 8.0));
        let y = crate::Aabb::new(Vec2::new(4.0, -20.0), Vec2::new(8.0, 20.0));
        for (footprint, axis) in [(x, Vec2::new(1.0, 0.0)), (y, Vec2::new(0.0, 1.0))] {
            let middle = (footprint.min + footprint.max) * 0.5;
            let across = Vec2::new(axis.y, axis.x);
            for along in [axis, axis * -1.0] {
                let floor = Floor {
                    footprint,
                    height: 0.9,
                    along,
                };
                // A door 9 m from the middle, beside the platform's edge.
                let door = middle + axis * 9.0 - across * 3.0;
                let spot = beside_door(&floor, middle, door);
                assert!(
                    spot.distance(middle + axis * 9.0) < 1e-4,
                    "{along:?}: {spot:?}"
                );
                // Past the end, held a margin short of the ramp.
                let far = beside_door(&floor, middle, middle + axis * 30.0);
                let end = 20.0 - PLATFORM_END_MARGIN;
                assert!(
                    far.distance(middle + axis * end) < 1e-4,
                    "{along:?}: {far:?}"
                );
            }
        }
    }

    #[test]
    fn the_ride_ends_at_its_station_or_on_arriving_at_a_terminal() {
        let (_, _, train) = shipped();
        let car = coaches(&train)[0];
        // Boarded at Monte Nievas on the up run: on to Castex.
        let (mut train, now) = standing(&train, 0.0, NIEVAS);
        let mut ride = Ride::new(&train, car);
        assert!(!ride.put_off(&train.state()));
        let mut clock = GameClock::at(TimeOfDay::from_seconds(now));
        clock.multiplier = ONBOARD_MULTIPLIER;
        assert_eq!(skip_ride(&mut train, &mut clock), Some(CASTEX));
        assert_eq!(train.state().phase, LinePhase::Parked);
        assert!(ride.put_off(&train.state()), "put off at the terminal");

        // Boarded while laid over at Castex: not put off until it ran.
        let mut parked = Ride::new(&train, car);
        assert!(!parked.put_off(&train.state()));
        train.advance(clock.elapsed(), 600.0, BASE_RATE);
        assert!(!parked.put_off(&train.state()), "still laid over");
        assert_eq!(skip_ride(&mut train, &mut clock), Some(NIEVAS));
        assert!(
            !parked.put_off(&train.state()),
            "rides on past Monte Nievas"
        );
        assert_eq!(skip_ride(&mut train, &mut clock), Some(PICO));
        assert!(parked.put_off(&train.state()));

        // A ride to a station ends as soon as the train stands there.
        let (mut opening, _) = standing(&train, 0.0, NIEVAS);
        let run = opening.state().run;
        let (arrive, _) = opening.stop_times(run, NIEVAS).unwrap();
        opening.place_on_run(run, arrive - 5.0);
        let mut ride = Ride::to_station(&opening, car, NIEVAS);
        assert!(!ride.put_off(&opening.state()));
        opening.place_on_run(run, arrive);
        assert!(ride.put_off(&opening.state()));
    }

    #[test]
    fn skipping_jumps_the_clock_by_the_onboard_ride_time() {
        let (_, _, train) = shipped();
        let from = hm(7, 0) + 100.0 * BASE_RATE;
        let mut train = at(&train, from);
        let mut clock = GameClock::at(TimeOfDay::from_seconds(from));
        clock.multiplier = ONBOARD_MULTIPLIER;
        let (_, eta) = next_arrival(&train, &clock).unwrap();
        let (arrive, _) = train.stop_times(Run::Up, NIEVAS).unwrap();
        // 12× for the rest of the leg.
        let expected = from + (arrive - 100.0) * 12.0;
        assert!((eta.seconds() - expected).abs() < 1e-6);
        assert_eq!(skip_ride(&mut train, &mut clock), Some(NIEVAS));
        assert!((clock.elapsed() - expected).abs() < 1e-6);
        let s = train.state();
        assert_eq!((s.phase, s.station), (LinePhase::Dwelling, Some(NIEVAS)));
        assert_eq!(s, train.state_at(Run::Up, arrive));
        // The ETA from the dwell is Castex.
        let (next, _) = next_arrival(&train, &clock).unwrap();
        assert_eq!(next, CASTEX);
    }

    #[test]
    fn the_skip_pulls_out_then_lands_three_seconds_before_the_next_stop() {
        let (_, _, train) = shipped();
        let (mut train, now) = standing(&train, 0.0, NIEVAS);
        let mut clock = GameClock::at(TimeOfDay::from_seconds(now));
        clock.multiplier = ONBOARD_MULTIPLIER;
        let (_, depart) = train.stop_times(Run::Up, NIEVAS).unwrap();
        let jumped = depart_now(&mut train, &mut clock).unwrap();
        assert!(jumped > 0.0);
        assert!((clock.elapsed() - now - jumped).abs() < 1e-6);
        assert_eq!(train.state(), train.state_at(Run::Up, depart));
        // Three real seconds of pulling out, at 12×.
        let game = clock.tick(3.0);
        train.advance(clock.elapsed() - game, game, clock.rate());
        assert_eq!(train.state().phase, LinePhase::Running);
        assert_eq!(depart_now(&mut train, &mut clock), Some(0.0));
        // The black: to 3 s before Castex, the clock by the same ride time.
        let before = clock.elapsed();
        let (_, eta) = next_arrival(&train, &clock).unwrap();
        assert_eq!(skip_to_approach(&mut train, &mut clock), Some(CASTEX));
        let lead = APPROACH_LEAD * ONBOARD_MULTIPLIER * BASE_RATE;
        assert!((clock.elapsed() + lead - eta.seconds()).abs() < 1e-6);
        assert!(clock.elapsed() > before);
        let (castex, _) = train.stop_times(Run::Up, CASTEX).unwrap();
        assert_eq!(
            train.state(),
            train.state_at(Run::Up, castex - APPROACH_LEAD)
        );
        // Coming in for real: stopped at Castex on time.
        let game = clock.tick(APPROACH_LEAD as f32);
        train.advance(clock.elapsed() - game, game, clock.rate());
        assert!(stands_at(&train.state(), CASTEX));
        assert!((clock.elapsed() - eta.seconds()).abs() < 1e-3);
    }

    #[test]
    fn waiting_on_a_platform_lands_on_the_next_train_there() {
        let (towns, world, train) = shipped();
        let nievas = town_of(&towns, &train, NIEVAS);
        // Waiting is by town: the shuttle may serve it too.
        let (nievas_town, castex_town) =
            (train.stations()[NIEVAS].town, train.stations()[CASTEX].town);
        let (middle, _) = nievas.platform_spot().unwrap();
        // 13:00: the train lays over at Castex; the next one at Monte
        // Nievas is the afternoon down run.
        let mut train = at(&train, hm(13, 0));
        let rider = on_foot(middle, &world);
        assert_eq!(
            waiting_place(std::slice::from_ref(&train), &towns, &[], &rider),
            Some(WaitingPlace {
                town: nievas_town,
                bay: false
            })
        );
        let mounted = Rider::new(middle, Heading::from_radians(0.0), &world);
        assert_eq!(
            waiting_place(std::slice::from_ref(&train), &towns, &[], &mounted),
            None
        );
        let far = on_foot(middle + Vec2::new(0.0, 30.0), &world);
        assert_eq!(
            waiting_place(std::slice::from_ref(&train), &towns, &[], &far),
            None
        );
        let mut clock = GameClock::at(TimeOfDay::hm(13, 0));
        let waited = wait_for_train(
            std::slice::from_mut(&mut train),
            &mut clock,
            WaitingPlace {
                town: nievas_town,
                bay: false,
            },
        )
        .unwrap();
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Dwelling, Some(NIEVAS), Run::Down)
        );
        let arrival = train.arrival_time_at(Run::Down, NIEVAS).unwrap().seconds();
        assert!(
            (clock.elapsed() - arrival).abs() < 0.01,
            "{}",
            clock.hh_mm()
        );
        assert!((waited - (arrival - hm(13, 0))).abs() < 0.01);
        // The pure state at that arrival.
        let (arrive, _) = train.stop_times(Run::Down, NIEVAS).unwrap();
        assert!((s.front - train.state_at(Run::Down, arrive).front).abs() < 1e-3);
        // With the train there, nothing to wait for.
        assert_eq!(
            waiting_place(std::slice::from_ref(&train), &towns, &[], &rider),
            None
        );
        assert_eq!(
            wait_for_train(
                std::slice::from_mut(&mut train),
                &mut clock,
                WaitingPlace {
                    town: nievas_town,
                    bay: false
                }
            ),
            Some(0.0)
        );

        // After the evening train, the wait runs into the next morning.
        let mut late = at(&train, hm(22, 0));
        let mut clock = GameClock::at(TimeOfDay::hm(22, 0));
        wait_for_train(
            std::slice::from_mut(&mut late),
            &mut clock,
            WaitingPlace {
                town: nievas_town,
                bay: false,
            },
        )
        .unwrap();
        assert_eq!(clock.day(), 1);
        assert_eq!(late.state().run, Run::Up);
        assert!(stands_at(&late.state(), NIEVAS));
        // At a terminal the train lays over: it stands there too.
        let mut clock = GameClock::at(TimeOfDay::hm(10, 0));
        let mut castex = at(&train, hm(10, 0));
        wait_for_train(
            std::slice::from_mut(&mut castex),
            &mut clock,
            WaitingPlace {
                town: castex_town,
                bay: false,
            },
        )
        .unwrap();
        assert!(stands_at(&castex.state(), CASTEX));
        assert_eq!(castex.state().phase, LinePhase::Parked);
    }

    /// Whether the seat of `ride` looks toward the platform of the station
    /// the train stands at.
    fn faces_the_platform(train: &LineTrain, ride: &Ride, towns: &[TownLayout]) -> bool {
        let station = train.state().station.unwrap();
        let (platform, _) = town_of(towns, train, station).platform_spot().unwrap();
        let car = train.placed_cars()[ride.car];
        let right = Vec2::new(car.direction.y, -car.direction.x);
        (platform - car.center).dot(right) * ride.side > 0.0
    }

    #[test]
    fn the_seat_faces_the_platform_of_the_next_stop() {
        let (towns, _, train) = shipped();
        let car = coaches(&train)[0];
        let mut sides = Vec::new();
        // Running in on the morning run, the afternoon run, and to both
        // terminals: seated on the platform side when it stops.
        for (from, station) in [
            (hm(7, 3), NIEVAS),
            (hm(11, 0), CASTEX),
            (hm(16, 1), NIEVAS),
            (hm(17, 50), PICO),
        ] {
            let mut moving = at(&train, from);
            assert_eq!(moving.state().phase, LinePhase::Running, "{from}");
            let ride = Ride::new(&moving, car).facing_platform(&moving, &towns, &[]);
            moving.skip_to_next_stop(from, BASE_RATE);
            assert_eq!(moving.state().station, Some(station));
            assert!(faces_the_platform(&moving, &ride, &towns), "to {station}");
            sides.push(ride.side);
        }
        // Boarded at a dwell: the next stop's platform, not this one's.
        let (dwelling, _) = standing(&train, 0.0, NIEVAS);
        let mut ride = Ride::new(&dwelling, car).facing_platform(&dwelling, &towns, &[]);
        let mut on = dwelling.clone();
        on.skip_to_next_stop(0.0, BASE_RATE);
        assert!(faces_the_platform(&on, &ride, &towns));
        assert!(!ride.put_off(&dwelling.state()));
        // The platforms lie on both sides across these runs, so the seat
        // really is chosen, not fixed.
        assert!(sides.contains(&1.0) && sides.contains(&-1.0), "{sides:?}");
    }

    #[test]
    fn the_opening_rides_into_the_first_town_and_reaches_it_on_time() {
        let (towns, _, train) = shipped();
        let nievas = train.stations()[NIEVAS].town;
        let mut train = train;
        let (mut clock, mut ride) = opening(&mut train, nievas, Run::Up).unwrap();
        assert_eq!(clock.multiplier, ONBOARD_MULTIPLIER);
        assert_eq!(ride.alight_at, Some(NIEVAS));
        assert_eq!(train.cars()[ride.car].kind, CarKind::Coach);
        let s = train.state();
        assert_eq!((s.phase, s.run), (LinePhase::Running, Run::Up));
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, NIEVAS);
        assert!((left - OPENING_LEAD).abs() < 1e-9);
        assert!(!ride.put_off(&s));
        // Riding on at 12× arrives at the timetabled time.
        let game = clock.tick(OPENING_LEAD as f32);
        train.advance(clock.elapsed() - game, game, ONBOARD_MULTIPLIER * BASE_RATE);
        let arrival = train.arrival_time_at(Run::Up, NIEVAS).unwrap();
        assert!((clock.time_of_day().seconds() - arrival.seconds()).abs() < 0.01);
        assert!(ride.put_off(&train.state()));
        assert!(towns[nievas].platform_spot().is_some());
        // Seated facing the Monte Nievas platform as it comes in.
        let seated = ride.facing_platform(&train, &towns, &[]);
        assert!(faces_the_platform(&train, &seated, &towns));
        assert_eq!(opening(&mut train, 99, Run::Up), None);
    }

    // The branched world: the line train through Medio and the shuttle in
    // Medio's bay on the way to Norte.

    use crate::train::line_tests::{MEDIO, NORTE, branched_line, branched_trains};

    /// The towns, their world (with the bay's platform laid), the line's
    /// bays and the world's trains: the line train, then the shuttle.
    fn branched() -> (Vec<TownLayout>, World, Vec<RailBay>, Vec<LineTrain>) {
        let (towns, line, shuttle) = branched_trains(crate::Timetable::default());
        let mut world = World::with_towns(WorldConfig::default(), Terrain::default(), &towns);
        let bays = branched_line().1;
        world.lay_rail(&bays);
        (towns, world, bays.bays, vec![line, shuttle])
    }

    /// Every train jumped from midnight to game time `now`.
    fn all_at(trains: &[LineTrain], now: f64) -> Vec<LineTrain> {
        trains.iter().map(|t| at(t, now)).collect()
    }

    /// The trains standing (not running) at Medio.
    fn standing_at_medio(trains: &[LineTrain]) -> Vec<usize> {
        (0..trains.len())
            .filter(|&i| {
                let train = &trains[i];
                train
                    .station_of_town(MEDIO)
                    .is_some_and(|s| stands_at(&train.state(), s))
            })
            .collect()
    }

    /// A spot 1 m beyond the door of the shuttle's coach nearest the line
    /// train's track: the ground between the two tracks, off any platform.
    fn beside_the_bay(line: &LineTrain, shuttle: &LineTrain) -> Vec2 {
        let car = coaches(shuttle)[0];
        let placed = shuttle.placed_cars()[car];
        let off_track = |d: &Vec2| d.distance(line.path().at(line.path().project(*d)));
        let door = coach_doors(&placed)
            .into_iter()
            .min_by(|a, b| off_track(a).total_cmp(&off_track(b)))
            .unwrap();
        let from_track = door - line.path().at(line.path().project(door));
        door - from_track * (1.0 / from_track.length())
    }

    /// A spot on the bay platform beside the shuttle's coach door nearest
    /// the platform's middle spot.
    fn on_the_bay_platform(shuttle: &LineTrain, world: &World) -> Vec2 {
        let coach = shuttle.placed_cars()[coaches(shuttle)[0]];
        let floor = nearest_floor(&world.floors, coach.center).unwrap();
        let (middle, _) = floor.spot_beside(coach.center);
        let door = coach_doors(&coach)
            .into_iter()
            .min_by(|a, b| a.distance(middle).total_cmp(&b.distance(middle)))
            .unwrap();
        beside_door(&floor, middle, door)
    }

    #[test]
    fn boarding_takes_the_standing_train_beside_the_rider_among_several() {
        let (towns, world, _, trains) = branched();
        // Both stand at Medio: the line train beside the platform, the
        // shuttle in its bay.
        let now = trains[0].stop_at(MEDIO, Run::Up).unwrap().seconds();
        let trains = all_at(&trains, now);
        assert_eq!(standing_at_medio(&trains), [0, 1]);
        assert!(trains[1].stands_in_bay() && !trains[0].stands_in_bay());
        // On the platform by the line train's coach: the line train.
        let car = coaches(&trains[0])[0];
        let spot = by_the_door(&trains[0], car, &towns);
        let rider = on_foot(spot, &world);
        let Boarding::Board(ride) = boarding(&trains, &rider, &world.floors) else {
            panic!("boards the line train");
        };
        assert_eq!((ride.train, ride.car), (0, car));
        // On the bay platform by the shuttle's coach: the shuttle.
        let spot = on_the_bay_platform(&trains[1], &world);
        assert!(
            world
                .floors
                .iter()
                .any(|f| f.footprint.contains_point(spot))
        );
        let rider = on_foot(spot, &world);
        let Boarding::Board(ride) = boarding(&trains, &rider, &world.floors) else {
            panic!("boards the shuttle");
        };
        assert_eq!(ride.train, 1);
        assert_eq!(trains[1].cars()[ride.car].kind, CarKind::Coach);
        // Mounted there it is the same dismount first.
        let mounted = Rider::new(spot, Heading::from_radians(0.0), &world);
        assert_eq!(
            boarding(&trains, &mounted, &world.floors),
            Boarding::DismountFirst
        );
        // Far from both, nothing.
        let far = on_foot(spot + Vec2::new(0.0, 40.0), &world);
        assert_eq!(boarding(&trains, &far, &world.floors), Boarding::Nothing);
    }

    #[test]
    fn nothing_is_boarded_from_the_ground_not_even_beside_the_bay() {
        let (towns, world, _, trains) = branched();
        let now = trains[0].stop_at(MEDIO, Run::Up).unwrap().seconds();
        let trains = all_at(&trains, now);
        // Beside the line train's coach but off the platform (across the
        // track from it): no step there.
        let car = coaches(&trains[0])[0];
        let on_platform = by_the_door(&trains[0], car, &towns);
        let side = to_platform(&trains[0], car, &towns);
        let off = on_foot(on_platform - side * 5.0, &world);
        assert!(
            !world
                .floors
                .iter()
                .any(|f| f.footprint.contains_point(off.position))
        );
        assert_eq!(boarding(&trains, &off, &world.floors), Boarding::Nothing);
        // Nor beside the shuttle in its bay, on the ground between the tracks.
        let ground = on_foot(beside_the_bay(&trains[0], &trains[1]), &world);
        assert!(
            !world
                .floors
                .iter()
                .any(|f| f.footprint.contains_point(ground.position))
        );
        assert_eq!(boarding(&trains, &ground, &world.floors), Boarding::Nothing);
        // It is boarded from the bay platform, but not once it has left.
        let rider = on_foot(on_the_bay_platform(&trains[1], &world), &world);
        let leaving = trains[1].timetable().departure(Run::Down).seconds() + 120.0;
        let gone = all_at(&trains, leaving);
        assert!(matches!(
            boarding(&trains, &rider, &world.floors),
            Boarding::Board(_)
        ));
        assert_eq!(boarding(&gone, &rider, &world.floors), Boarding::Nothing);
    }

    #[test]
    fn each_platform_at_medio_waits_for_its_own_train() {
        let (towns, world, bays, trains) = branched();
        let (middle, _) = towns[MEDIO].platform_spot().unwrap();
        let on_station = on_foot(middle, &world);
        let in_bay = all_at(
            &trains,
            trains[0].stop_at(MEDIO, Run::Up).unwrap().seconds(),
        );
        let on_bay = on_foot(on_the_bay_platform(&in_bay[1], &world), &world);
        let station_place = WaitingPlace {
            town: MEDIO,
            bay: false,
        };
        let bay_place = WaitingPlace {
            town: MEDIO,
            bay: true,
        };
        let (mut line_while_shuttle_in, mut shuttle_waits) = (0, 0);
        for k in 0..24 * 12 {
            let now = f64::from(k) * 300.0;
            let now_trains = all_at(&trains, now);
            let standing = standing_at_medio(&now_trains);
            // Nothing to wait for beside a platform with its train in.
            let place = |rider: &Rider| waiting_place(&now_trains, &towns, &bays, rider);
            assert_eq!(
                place(&on_station),
                (!standing.contains(&0)).then_some(station_place),
                "{now}"
            );
            assert_eq!(
                place(&on_bay),
                (!standing.contains(&1)).then_some(bay_place),
                "{now}"
            );
            for (rider, want, other) in [(&on_station, 0, 1), (&on_bay, 1, 0)] {
                let Some(wanted) = place(rider) else { continue };
                let mut waiting = now_trains.clone();
                let mut clock = GameClock::at(TimeOfDay::from_seconds(now));
                let waited = wait_for_train(&mut waiting, &mut clock, wanted).unwrap();
                assert!(waited > 0.0, "{now}");
                // The train of that platform has come in, in its place.
                assert!(standing_at_medio(&waiting).contains(&want), "{now}");
                if want == 1 {
                    shuttle_waits += 1;
                } else if standing.contains(&other) {
                    line_while_shuttle_in += 1;
                }
            }
        }
        // The shuttle stands before and after the line train: waiting for
        // the line train goes on past it in its bay (it is not the train
        // of that platform), and the bay platform's wait is for the shuttle.
        assert!(line_while_shuttle_in > 0 && shuttle_waits > 0);
    }

    #[test]
    fn waiting_at_norte_takes_the_shuttle_which_alone_serves_it() {
        let (towns, world, _, trains) = branched();
        let (middle, _) = towns[NORTE].platform_spot().unwrap();
        let rider = on_foot(middle, &world);
        // 13:00: the shuttle is somewhere on its day; wait for it at Norte.
        let mut now_trains = all_at(&trains, hm(13, 0));
        let mut clock = GameClock::at(TimeOfDay::hm(13, 0));
        if let Some(place) = waiting_place(&now_trains, &towns, &[], &rider) {
            assert_eq!(
                place,
                WaitingPlace {
                    town: NORTE,
                    bay: false
                }
            );
            wait_for_train(&mut now_trains, &mut clock, place).unwrap();
        }
        let shuttle = &now_trains[1];
        let at_norte = shuttle.station_of_town(NORTE).unwrap();
        assert!(stands_at(&shuttle.state(), at_norte));
        assert!(now_trains[0].station_of_town(NORTE).is_none());
        // Boarding it from the platform and riding on lands it at Medio,
        // the line train following the clock on the way.
        let car = coaches(shuttle)[0];
        let spot = by_the_door(shuttle, car, &towns);
        let Boarding::Board(ride) = boarding(&now_trains, &on_foot(spot, &world), &world.floors)
        else {
            panic!("boards the shuttle at Norte");
        };
        assert_eq!(ride.train, 1);
    }

    #[test]
    fn a_jump_of_the_ridden_train_leaves_the_others_following_the_clock() {
        let (_, _, _, trains) = branched();
        // A moment with both trains running.
        let moment = (0..24 * 60)
            .map(|k| f64::from(k) * 60.0)
            .find(|&now| {
                all_at(&trains, now)
                    .iter()
                    .all(|t| t.state().phase == LinePhase::Running)
            })
            .expect("both trains run at some minute of the day");
        let mut now_trains = all_at(&trains, moment);
        let mut clock = GameClock::at(TimeOfDay::from_seconds(moment));
        clock.multiplier = ONBOARD_MULTIPLIER;
        let from = clock.elapsed();
        let line_before = now_trains[0].state();
        assert_eq!(line_before.phase, LinePhase::Running, "the line train runs");
        let station = skip_ride(&mut now_trains[1], &mut clock).unwrap();
        follow_jump(
            &mut now_trains,
            1,
            from,
            clock.elapsed() - from,
            clock.rate(),
        );
        // The ridden train stands at its next stop.
        assert_eq!(Some(station), now_trains[1].state().station);
        assert!(stands_at(&now_trains[1].state(), station));
        // The line train is where the clock puts it: not skipped to a stop.
        let mut expected = all_at(&trains, from)[0].clone();
        expected.advance(from, clock.elapsed() - from, clock.rate());
        assert_eq!(now_trains[0], expected);
        assert_ne!(now_trains[0].state(), line_before, "time passed");
        assert!(clock.elapsed() > from);
    }

    #[test]
    fn the_shuttle_is_boarded_from_the_middle_spot_of_its_terminal_platform_all_layover() {
        let (towns, world, _, trains) = branched();
        let shuttle = &trains[1];
        let norte = shuttle.station_of_town(NORTE).unwrap();
        let (middle, _) = towns[NORTE].platform_spot().unwrap();
        let rider = on_foot(middle, &world);
        // The first layover, arrival to departure: its first half before
        // the shuttle is turned round, its second after.
        let first = shuttle.arrival_time_at(Run::Down, norte).unwrap().seconds();
        let (last, run) = shuttle.next_departure_after(first + 1.0);
        assert_eq!(run, Run::Up);
        let mut boarded = 0;
        for k in 0..=20 {
            let now = first + (last - first) * f64::from(k) / 20.0;
            let now_trains = all_at(&trains, now.clamp(first + 1.0, last - 1.0));
            if !stands_at(&now_trains[1].state(), norte) {
                continue;
            }
            let Boarding::Board(ride) = boarding(&now_trains, &rider, &world.floors) else {
                panic!("no boarding from the middle spot at {now}");
            };
            assert_eq!(ride.train, 1);
            boarded += 1;
        }
        assert!(boarded >= 10, "{boarded}");
    }

    #[test]
    fn a_rider_alights_from_the_shuttle_on_the_bay_platform_at_medios_bay() {
        let (towns, mut world, _, trains) = branched();
        let now = trains[0].stop_at(MEDIO, Run::Up).unwrap().seconds();
        let trains = all_at(&trains, now);
        world.moving = trains.iter().flat_map(|t| t.obstacles()).collect();
        let shuttle = &trains[1];
        assert!(shuttle.stands_in_bay());
        let car = coaches(shuttle)[0];
        let (spot, heading) = alight_spot(shuttle, car, &towns, &world).expect("a spot");
        // On the bay's platform, in reach of the coach's door, facing away
        // from the track; not on the station platform across the line.
        let bay = world.floors.last().unwrap();
        assert!(bay.footprint.contains_point(spot), "{spot:?}");
        assert!(!towns[MEDIO].floors()[0].footprint.contains_point(spot));
        let out = Vec2::new(heading.radians().cos(), heading.radians().sin());
        let coach = shuttle.placed_cars()[car];
        assert!((spot - coach.center).dot(out) > 0.0);
        let rider = on_foot(spot, &world);
        assert!(matches!(
            boarding(&trains, &rider, &world.floors),
            Boarding::Board(r) if r.train == 1
        ));
    }

    #[test]
    fn a_passenger_on_the_shuttle_faces_the_bay_platform_it_comes_in_to() {
        let (towns, world, _, trains) = branched();
        let shuttle = &trains[1];
        let medio = shuttle.station_of_town(MEDIO).unwrap();
        // Underway toward Medio (the up run), a minute before the stop.
        let arrival = shuttle.arrival_time_at(Run::Up, medio).unwrap().seconds();
        let running = at(shuttle, arrival - 60.0);
        assert_eq!(running.state().phase, LinePhase::Running);
        let car = coaches(&running)[0];
        let ride = Ride::new(&running, car).facing_platform(&running, &towns, &world.floors);
        // Where it stands, the platform is on the side the ride faces.
        let standing = at(shuttle, arrival + 1.0);
        assert_eq!(standing.state().station, Some(medio));
        let coach = standing.placed_cars()[car];
        let bay = nearest_floor(&world.floors, coach.center).unwrap();
        let (spot, _) = bay.spot_beside(coach.center);
        let right = Vec2::new(coach.direction.y, -coach.direction.x);
        assert_eq!((spot - coach.center).dot(right).signum(), ride.side);
        // Not the station platform's side, across the line.
        let (station, _) = towns[MEDIO].platform_spot().unwrap();
        assert_ne!((station - coach.center).dot(right).signum(), ride.side);
    }

    #[test]
    fn the_shuttles_second_round_trip_is_reached_by_waiting() {
        let (_, _, _, trains) = branched();
        let shuttle = &trains[1];
        let norte = shuttle.station_of_town(NORTE).unwrap();
        let arrivals = shuttle.arrivals_at(Run::Down, norte);
        assert_eq!(arrivals.len(), 2, "one arrival at Norte per down run");
        // Just after the first has left again, waiting at Norte lands on
        // the second.
        let (_, left) = shuttle.stop_times(Run::Down, norte).unwrap();
        let _ = left;
        let after_first = arrivals[0].seconds() + 60.0;
        let up = shuttle.timetable().departures(Run::Up).to_vec();
        let start = up
            .iter()
            .map(|t| t.seconds())
            .find(|&t| t > after_first)
            .expect("an up run after the first arrival")
            + 60.0;
        let mut now_trains = all_at(&trains, start);
        assert!(!stands_at(&now_trains[1].state(), norte));
        let mut clock = GameClock::at(TimeOfDay::from_seconds(start));
        wait_for_train(
            &mut now_trains,
            &mut clock,
            WaitingPlace {
                town: NORTE,
                bay: false,
            },
        )
        .unwrap();
        assert!(stands_at(&now_trains[1].state(), norte));
        assert!(
            (clock.elapsed() - arrivals[1].seconds()).abs() < 0.01,
            "{} for {}",
            clock.hh_mm(),
            arrivals[1]
        );
    }
}
