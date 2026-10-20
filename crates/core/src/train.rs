//! The line train: one passenger train running the whole railway line
//! (Castex → Monte Nievas → General Pico and back) on a timetable driven by
//! the [`crate::GameClock`]. Its run is a pure function of the real seconds
//! since it departed, so clock jumps land it exactly where running it frame
//! by frame would. Its cars are moving obstacles that shove riders off the
//! track.

use crate::{
    Aabb, BASE_RATE, BuildingKind, DAY_SECONDS, Heading, LinePath, RailLine, SideObstacle,
    TimeOfDay, TownLayout, Vec2,
};

/// What a car is, so render can pick its look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CarKind {
    /// Steam locomotive with its tender.
    Locomotive,
    Boxcar,
    /// Open grain hopper.
    Hopper,
    /// Passenger coach.
    Coach,
}

/// One car of the consist, in order from the front.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Car {
    pub kind: CarKind,
    /// Length over the buffers, in meters.
    pub length: f32,
}

/// A car placed on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedCar {
    pub kind: CarKind,
    pub center: Vec2,
    /// Unit direction of travel.
    pub direction: Vec2,
    pub length: f32,
    /// Whether the car is on the drawn railway (the line train always is).
    pub on_rail: bool,
}

/// Which way a run goes along the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Run {
    /// From the line's first town to its last (Castex → Pico).
    Down,
    /// From the line's last town back to its first (Pico → Castex).
    Up,
}

/// What the line train is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinePhase {
    /// Moving between two stations.
    Running,
    /// Standing at a station partway through a run.
    Dwelling,
    /// Laid over at a terminal between runs.
    Parked,
}

/// A station of the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// Index (into the world's towns) of the station's town.
    pub town: usize,
    /// Distance along the line path to the station's projection onto it.
    pub mark: f32,
}

/// Departure times and motion of the line train.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timetable {
    /// When the train leaves the first town (Castex) on its down run.
    pub depart_first: TimeOfDay,
    /// When the train leaves the last town (Pico) on its up run.
    pub depart_last: TimeOfDay,
    /// Real seconds standing at each station partway through a run.
    pub dwell: f32,
    /// Top speed, in m/s.
    pub top_speed: f32,
    /// Pulling-away and braking rate, in m/s².
    pub acceleration: f32,
}

impl Timetable {
    /// 65 km/h, the line's top speed.
    pub const TOP_SPEED: f32 = 65.0 / 3.6;
}

impl Default for Timetable {
    fn default() -> Self {
        Self {
            depart_first: TimeOfDay::hm(7, 0),
            depart_last: TimeOfDay::hm(16, 0),
            dwell: 25.0,
            top_speed: Self::TOP_SPEED,
            acceleration: 0.5,
        }
    }
}

/// Snapshot of the line train.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineTrainState {
    pub phase: LinePhase,
    /// The run underway, or the run the train is facing while parked.
    pub run: Run,
    /// Distance along the line path to the locomotive's front.
    pub front: f32,
    /// Speed along the track, in m/s (never negative).
    pub speed: f32,
    /// Which way the locomotive faces.
    pub heading: Heading,
    /// The station the train stands at, by index into
    /// [`LineTrain::stations`].
    pub station: Option<usize>,
}

/// The one passenger train running the whole line on a timetable.
///
/// It leaves the first town (Castex) at [`Timetable::depart_first`], stops
/// at every station on the way, lays over at the last town (Pico), leaves
/// it at [`Timetable::depart_last`] and runs back, spending the night at
/// the first town. The clock only triggers departures: once running, the
/// train moves physically in real seconds, whatever the clock's rate.
///
/// Its whole run is a pure function of the real seconds since it departed
/// ([`Self::state_at`]), so [`Self::advance`] can jump the clock forward
/// and land the train exactly where running it frame by frame would.
#[derive(Debug, Clone, PartialEq)]
pub struct LineTrain {
    path: LinePath,
    stations: Vec<Station>,
    cars: Vec<Car>,
    timetable: Timetable,
    /// Distance from the locomotive's front to the coaches' middle.
    stop_offset: f32,
    down: RunPlan,
    up: RunPlan,
    motion: Motion,
}

/// Where the train is in its day.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Motion {
    /// Laid over at terminal `station` since game time `since`; `turned`
    /// once it faces its next run (half way through the layover).
    Parked {
        station: usize,
        since: f64,
        turned: bool,
    },
    /// On `run`, `elapsed` real seconds after departing.
    Running { run: Run, elapsed: f64 },
}

/// One run's stops: where the front stands at each, as progress along the
/// run (meters from the run's starting end of the path), and when it
/// arrives and leaves, in real seconds from departure. In run order.
#[derive(Debug, Clone, PartialEq)]
struct RunPlan {
    stops: Vec<f64>,
    times: Vec<(f64, f64)>,
}

/// Arrivals this close (in real seconds) count as at the station, so
/// jumping to a computed arrival never lands a hair short of it.
const ARRIVAL_EPSILON: f64 = 1e-6;
/// Game seconds added to computed waits, for the same reason.
const WAIT_MARGIN: f64 = 1e-3;

impl LineTrain {
    /// Width of every car, in meters.
    pub const WIDTH: f32 = 3.0;
    /// Gap between coupled cars, in meters.
    pub const COUPLING: f32 = 1.0;

    /// The train of `line` through `towns`, parked overnight at the first
    /// town at midnight of day 0, facing its morning run. `None` when the
    /// line serves fewer than two stations or is too short for the train.
    pub fn new(line: &RailLine, towns: &[TownLayout], timetable: Timetable) -> Option<Self> {
        let path = line.full_path(towns);
        let stations: Vec<Station> = line
            .connected_towns()
            .into_iter()
            .filter_map(|town| {
                let layout = towns.get(town)?;
                let rail = layout.railway?;
                let at = layout
                    .buildings
                    .iter()
                    .find(|b| b.kind == BuildingKind::Station)
                    .map_or((rail.start + rail.end) * 0.5, |b| b.footprint.center());
                Some(Station {
                    town,
                    mark: path.project(at),
                })
            })
            .collect();
        // A rural mixed train: two passenger coaches behind the locomotive,
        // then freight.
        let cars: Vec<Car> = [
            (CarKind::Locomotive, 14.0),
            (CarKind::Coach, 12.0),
            (CarKind::Coach, 12.0),
            (CarKind::Boxcar, 10.0),
            (CarKind::Boxcar, 10.0),
            (CarKind::Hopper, 9.0),
            (CarKind::Hopper, 9.0),
            (CarKind::Boxcar, 10.0),
        ]
        .into_iter()
        .map(|(kind, length)| Car { kind, length })
        .collect();
        let length = consist_length(&cars);
        if stations.len() < 2 || path.length() < length + 1.0 {
            return None;
        }
        let stop_offset = coaches_middle(&cars).unwrap_or(length / 2.0);
        let total = path.length();
        let down: Vec<f32> = stations
            .iter()
            .map(|s| (s.mark + stop_offset).clamp(length, total))
            .collect();
        let up: Vec<f32> = stations
            .iter()
            .rev()
            .map(|s| total - (s.mark - stop_offset).clamp(0.0, total - length))
            .collect();
        let mut train = Self {
            path,
            stations,
            cars,
            timetable,
            stop_offset,
            down: RunPlan {
                stops: Vec::new(),
                times: Vec::new(),
            },
            up: RunPlan {
                stops: Vec::new(),
                times: Vec::new(),
            },
            motion: Motion::Parked {
                station: 0,
                since: 0.0,
                turned: true,
            },
        };
        train.down = train.plan(&down);
        train.up = train.plan(&up);
        Some(train)
    }

    /// The line the train runs, Castex to Pico.
    pub fn path(&self) -> &LinePath {
        &self.path
    }

    /// The stations, in line order.
    pub fn stations(&self) -> &[Station] {
        &self.stations
    }

    /// The consist, front first.
    pub fn cars(&self) -> &[Car] {
        &self.cars
    }

    pub fn timetable(&self) -> &Timetable {
        &self.timetable
    }

    /// Length from the locomotive's front to the last car's end, in meters.
    pub fn length(&self) -> f32 {
        consist_length(&self.cars)
    }

    /// Distance from the locomotive's front to the middle of the coaches,
    /// which stop alongside each station.
    pub fn stop_offset(&self) -> f32 {
        self.stop_offset
    }

    /// The station (index into [`Self::stations`]) of town `town`.
    pub fn station_of_town(&self, town: usize) -> Option<usize> {
        self.stations.iter().position(|s| s.town == town)
    }

    /// Real seconds from departing one terminal to arriving at the other.
    pub fn run_duration(&self, run: Run) -> f64 {
        self.plan_of(run).times.last().map_or(0.0, |t| t.1)
    }

    /// Real seconds from the departure of `run` to its arrival at, and
    /// departure from, `station`.
    pub fn stop_times(&self, run: Run, station: usize) -> Option<(f64, f64)> {
        let k = self.stop_index(run, station)?;
        self.plan_of(run).times.get(k).copied()
    }

    /// The train `run_seconds` real seconds after departing on `run`
    /// (clamped to the run). Pure: the same input always gives the same
    /// state.
    pub fn state_at(&self, run: Run, run_seconds: f64) -> LineTrainState {
        let plan = self.plan_of(run);
        let t = if run_seconds.is_nan() {
            0.0
        } else {
            run_seconds.clamp(0.0, self.run_duration(run))
        };
        let (a, v) = self.kinematics();
        let n = plan.stops.len();
        let (mut progress, mut speed, mut stopped) = (plan.stops[n - 1], 0.0, Some(n - 1));
        for k in 0..n {
            let (arrive, depart) = plan.times[k];
            if t >= arrive - ARRIVAL_EPSILON && t <= depart {
                (progress, speed, stopped) = (plan.stops[k], 0.0, Some(k));
                break;
            }
            if k + 1 < n && t < plan.times[k + 1].0 - ARRIVAL_EPSILON {
                let d = plan.stops[k + 1] - plan.stops[k];
                let (moved, s) = leg_at(d, v, a, t - depart);
                (progress, speed, stopped) = (plan.stops[k] + moved, s, None);
                break;
            }
        }
        self.snapshot(
            run,
            progress,
            speed,
            stopped.map(|k| {
                // At either end of the run the train stands at a terminal.
                let phase = if k == 0 || k + 1 == n {
                    LinePhase::Parked
                } else {
                    LinePhase::Dwelling
                };
                (phase, self.station_at(run, k))
            }),
        )
    }

    /// The train now.
    pub fn state(&self) -> LineTrainState {
        match self.motion {
            Motion::Running { run, elapsed } => self.state_at(run, elapsed),
            Motion::Parked {
                station, turned, ..
            } => {
                let run = if turned {
                    self.departing(station)
                } else {
                    self.departing(station).back()
                };
                let k = self.stop_index(run, station).unwrap_or(0);
                let progress = self.plan_of(run).stops[k];
                self.snapshot(run, progress, 0.0, Some((LinePhase::Parked, station)))
            }
        }
    }

    /// Follows the clock from game time `from` (seconds since midnight of
    /// day 0, see [`crate::GameClock::elapsed`]) for `game_seconds`, at
    /// `rate` game seconds per real second: the clock's rate while ticking,
    /// [`crate::BASE_RATE`] for a jump. Departures happen when the clock
    /// reaches them; running is in real seconds. Bad input changes nothing.
    pub fn advance(&mut self, from: f64, game_seconds: f64, rate: f64) {
        let valid = |x: f64| x.is_finite() && x > 0.0;
        if !(from.is_finite() && valid(game_seconds) && valid(rate)) {
            return;
        }
        let end = from + game_seconds;
        let mut now = from;
        loop {
            match self.motion {
                Motion::Parked {
                    station,
                    since,
                    turned,
                } => {
                    let depart = self.departure_from(station, since);
                    let turned = turned || end >= (since + depart) / 2.0;
                    if depart > end {
                        self.motion = Motion::Parked {
                            station,
                            since,
                            turned,
                        };
                        return;
                    }
                    now = now.max(depart);
                    self.motion = Motion::Running {
                        run: self.departing(station),
                        elapsed: 0.0,
                    };
                }
                Motion::Running { run, elapsed } => {
                    let left = self.run_duration(run) - elapsed;
                    let real = (end - now) / rate;
                    if real < left {
                        self.motion = Motion::Running {
                            run,
                            elapsed: elapsed + real,
                        };
                        return;
                    }
                    now += left * rate;
                    self.motion = Motion::Parked {
                        station: self.terminal(run),
                        since: now,
                        turned: false,
                    };
                }
            }
        }
    }

    /// The first scheduled departure from either terminal at or after game
    /// time `at`, and the run it starts.
    pub fn next_departure_after(&self, at: f64) -> (f64, Run) {
        let last = self.stations.len() - 1;
        let down = self.departure_from(0, at);
        let up = self.departure_from(last, at);
        if down <= up {
            (down, Run::Down)
        } else {
            (up, Run::Up)
        }
    }

    /// When `run` reaches `station` by the timetable, with the clock at the
    /// base rate the whole way.
    pub fn arrival_time_at(&self, run: Run, station: usize) -> Option<TimeOfDay> {
        let (arrive, _) = self.stop_times(run, station)?;
        let depart = self.scheduled(run).seconds();
        Some(TimeOfDay::from_seconds(depart + arrive * BASE_RATE))
    }

    /// The time of day half way through the morning (down) run's stop at
    /// `town`, with the clock at the base rate: start the clock there and
    /// the train stands at that town's station.
    pub fn first_stop_at(&self, town: usize) -> Option<TimeOfDay> {
        let (arrive, depart) = self.stop_times(Run::Down, self.station_of_town(town)?)?;
        let start = self.timetable.depart_first.seconds();
        Some(TimeOfDay::from_seconds(
            start + (arrive + depart) / 2.0 * BASE_RATE,
        ))
    }

    /// Game seconds, at the base rate, from game time `now` until the train
    /// next stands at `station`: `0` if it already does.
    pub fn time_until_at(&self, station: usize, now: f64) -> Option<f64> {
        if station >= self.stations.len() {
            return None;
        }
        if self.state().station == Some(station) {
            return Some(0.0);
        }
        let (terminal, since) = match self.motion {
            Motion::Running { run, elapsed } => {
                let (arrive, _) = self.stop_times(run, station)?;
                if arrive > elapsed {
                    return Some((arrive - elapsed) * BASE_RATE + WAIT_MARGIN);
                }
                let left = self.run_duration(run) - elapsed;
                (self.terminal(run), now + left * BASE_RATE)
            }
            Motion::Parked { station, since, .. } => (station, since.max(now)),
        };
        let depart = self.departure_from(terminal, since).max(since);
        let (arrive, _) = self.stop_times(self.departing(terminal), station)?;
        Some(depart + arrive * BASE_RATE - now + WAIT_MARGIN)
    }

    /// While running, the next station the train will stop at and the real
    /// seconds until it does: the rest of a ride.
    pub fn next_stop(&self) -> Option<(usize, f64)> {
        let Motion::Running { run, elapsed } = self.motion else {
            return None;
        };
        let plan = self.plan_of(run);
        let k = plan
            .times
            .iter()
            .position(|&(arrive, _)| arrive > elapsed + ARRIVAL_EPSILON)?;
        Some((self.station_at(run, k), plan.times[k].0 - elapsed))
    }

    /// Every car on the ground plane, front first. Each car's ends sit on
    /// the line at their own distance along it, so the consist follows the
    /// curves.
    pub fn placed_cars(&self) -> Vec<PlacedCar> {
        let state = self.state();
        let sign = run_sign(state.run);
        let mut behind = 0.0;
        self.cars
            .iter()
            .map(|car| {
                let (center, direction) = self.car_pose(state.front, sign, behind, car.length);
                behind += car.length + Self::COUPLING;
                PlacedCar {
                    kind: car.kind,
                    center,
                    direction,
                    length: car.length,
                    on_rail: true,
                }
            })
            .collect()
    }

    /// Footprints of the cars, as obstacles that push riders off the track.
    /// A car along an axis is one box; a car on a slant is a row of short
    /// boxes along it, so the boxes hug its sides.
    pub fn obstacles(&self) -> Vec<SideObstacle> {
        let mut out = Vec::new();
        for car in self.placed_cars() {
            let d = car.direction;
            let across = Vec2::new(-d.y, d.x);
            let pieces = if d.x.abs().max(d.y.abs()) > 0.999 {
                1
            } else {
                (car.length / Self::WIDTH).ceil().max(1.0) as usize
            };
            let piece = car.length / pieces as f32;
            let size = Vec2::new(
                d.x.abs() * piece + d.y.abs() * Self::WIDTH,
                d.y.abs() * piece + d.x.abs() * Self::WIDTH,
            );
            for i in 0..pieces {
                let offset = (i as f32 + 0.5) * piece - car.length / 2.0;
                out.push(SideObstacle {
                    footprint: Aabb::from_center_size(car.center + d * offset, size),
                    across,
                });
            }
        }
        out
    }

    /// Stop timings of a run through `stops` (front progress, run order).
    fn plan(&self, stops: &[f32]) -> RunPlan {
        let (a, v) = self.kinematics();
        let dwell = f64::from(self.timetable.dwell.max(0.0));
        let stops: Vec<f64> = stops.iter().map(|&p| f64::from(p)).collect();
        let mut times = vec![(0.0, 0.0)];
        let mut t = 0.0;
        for k in 1..stops.len() {
            t += leg_duration((stops[k] - stops[k - 1]).max(0.0), v, a);
            let arrive = t;
            if k + 1 < stops.len() {
                t += dwell;
            }
            times.push((arrive, t));
        }
        RunPlan { stops, times }
    }

    /// Acceleration and top speed, kept positive.
    fn kinematics(&self) -> (f64, f64) {
        (
            f64::from(self.timetable.acceleration.max(0.01)),
            f64::from(self.timetable.top_speed.max(0.1)),
        )
    }

    fn plan_of(&self, run: Run) -> &RunPlan {
        match run {
            Run::Down => &self.down,
            Run::Up => &self.up,
        }
    }

    /// Index of `station` in `run`'s stop order.
    fn stop_index(&self, run: Run, station: usize) -> Option<usize> {
        let n = self.stations.len();
        (station < n).then(|| match run {
            Run::Down => station,
            Run::Up => n - 1 - station,
        })
    }

    /// The station of `run`'s `k`-th stop.
    fn station_at(&self, run: Run, k: usize) -> usize {
        match run {
            Run::Down => k,
            Run::Up => self.stations.len() - 1 - k,
        }
    }

    /// The run that leaves terminal `station`.
    fn departing(&self, station: usize) -> Run {
        if station == 0 { Run::Down } else { Run::Up }
    }

    /// The terminal `run` ends at.
    fn terminal(&self, run: Run) -> usize {
        match run {
            Run::Down => self.stations.len() - 1,
            Run::Up => 0,
        }
    }

    /// The scheduled time of day `run` departs.
    fn scheduled(&self, run: Run) -> TimeOfDay {
        match run {
            Run::Down => self.timetable.depart_first,
            Run::Up => self.timetable.depart_last,
        }
    }

    /// The first scheduled departure from terminal `station` at or after
    /// game time `since`.
    fn departure_from(&self, station: usize, since: f64) -> f64 {
        let time = self.scheduled(self.departing(station)).seconds();
        let days = ((since - time) / DAY_SECONDS).ceil();
        time + days * DAY_SECONDS
    }

    /// The state with the front `progress` meters into `run`.
    fn snapshot(
        &self,
        run: Run,
        progress: f64,
        speed: f64,
        stopped: Option<(LinePhase, usize)>,
    ) -> LineTrainState {
        let front = match run {
            Run::Down => progress as f32,
            Run::Up => self.path.length() - progress as f32,
        };
        let length = self.cars.first().map_or(1.0, |c| c.length);
        let (_, d) = self.car_pose(front, run_sign(run), 0.0, length);
        LineTrainState {
            phase: stopped.map_or(LinePhase::Running, |s| s.0),
            run,
            front,
            speed: speed as f32,
            heading: Heading::from_radians(d.y.atan2(d.x)),
            station: stopped.map(|s| s.1),
        }
    }

    /// Center and unit direction of travel of a car `length` long whose
    /// front end is `behind` meters behind the locomotive's front at
    /// `front`, travelling toward `sign` (+1 down the line, -1 up).
    fn car_pose(&self, front: f32, sign: f32, behind: f32, length: f32) -> (Vec2, Vec2) {
        let head = front - sign * behind;
        let tail = head - sign * length;
        let (a, b) = (self.path.at(head), self.path.at(tail));
        let chord = a - b;
        let span = chord.length();
        let direction = if span > 1e-4 {
            chord * (1.0 / span)
        } else {
            self.path.direction_at(head) * sign
        };
        ((a + b) * 0.5, direction)
    }
}

impl Run {
    /// The run going the other way.
    pub fn back(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
        }
    }
}

/// +1 for a run down the line (growing distance along the path), -1 up.
fn run_sign(run: Run) -> f32 {
    match run {
        Run::Down => 1.0,
        Run::Up => -1.0,
    }
}

/// Length of `cars` coupled together, in meters.
fn consist_length(cars: &[Car]) -> f32 {
    let total: f32 = cars.iter().map(|c| c.length).sum();
    total + LineTrain::COUPLING * cars.len().saturating_sub(1) as f32
}

/// Distance from the front of the consist to the middle of its coaches.
fn coaches_middle(cars: &[Car]) -> Option<f32> {
    let mut at = 0.0;
    let (mut first, mut last) = (None, None);
    for car in cars {
        if car.kind == CarKind::Coach {
            first.get_or_insert(at);
            last = Some(at + car.length);
        }
        at += car.length + LineTrain::COUPLING;
    }
    Some((first? + last?) / 2.0)
}

/// Real seconds to run `d` meters from a stop to a stop, accelerating and
/// braking at `a` with a top speed of `v`.
fn leg_duration(d: f64, v: f64, a: f64) -> f64 {
    if d <= 0.0 {
        0.0
    } else if d >= v * v / a {
        d / v + v / a
    } else {
        2.0 * (d / a).sqrt()
    }
}

/// Distance covered and speed `tau` real seconds into a stop-to-stop leg
/// of `d` meters (see [`leg_duration`]). It ends exactly at `d`, stopped.
fn leg_at(d: f64, v: f64, a: f64, tau: f64) -> (f64, f64) {
    let total = leg_duration(d, v, a);
    let tau = tau.clamp(0.0, total);
    let peak = v.min((a * d.max(0.0)).sqrt());
    let up = peak / a;
    if tau < up {
        (a * tau * tau / 2.0, a * tau)
    } else if tau <= total - up {
        (peak * peak / (2.0 * a) + peak * (tau - up), peak)
    } else {
        let left = total - tau;
        (d - a * left * left / 2.0, a * left)
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;
    use crate::{MAX_STEP, Rider, RiderInput, RiderParams, Terrain, World};

    /// Castex, Monte Nievas and Pico as shipped, and their line train.
    fn shipped() -> (Vec<TownLayout>, LineTrain) {
        let (towns, line) = crate::rail::tests::shipped();
        let train = LineTrain::new(&line, &towns, Timetable::default()).expect("a line train");
        (towns, train)
    }

    const CASTEX: usize = 0;
    const NIEVAS: usize = 1;
    const PICO: usize = 2;

    fn hm(h: u32, m: u32) -> f64 {
        TimeOfDay::hm(h, m).seconds()
    }

    /// `(phase, station, start, end)` of each stretch of a run, sampled.
    fn stretches(train: &LineTrain, run: Run) -> Vec<(LinePhase, Option<usize>, f64, f64)> {
        let mut out: Vec<(LinePhase, Option<usize>, f64, f64)> = Vec::new();
        let end = train.run_duration(run);
        let dt = 0.05;
        for k in 1..(end / dt) as usize {
            let t = k as f64 * dt;
            let s = train.state_at(run, t);
            match out.last_mut() {
                Some(last) if last.0 == s.phase && last.1 == s.station => last.3 = t,
                _ => out.push((s.phase, s.station, t, t)),
            }
        }
        out
    }

    #[test]
    fn stations_are_in_line_order_with_the_coaches_alongside() {
        let (towns, train) = shipped();
        let names: Vec<&str> = train
            .stations()
            .iter()
            .map(|s| towns[s.town].name.as_str())
            .collect();
        assert_eq!(names, ["Eduardo Castex", "Monte Nievas", "General Pico"]);
        assert!(train.stations().windows(2).all(|w| w[0].mark < w[1].mark));
        assert_eq!(train.station_of_town(0), Some(NIEVAS));
        // A rural consist: two passenger coaches behind the locomotive, then
        // freight, eight cars in all.
        assert_eq!(train.cars().len(), 8);
        let kinds: Vec<CarKind> = train.cars().iter().map(|c| c.kind).collect();
        assert_eq!(kinds.iter().filter(|&&k| k == CarKind::Coach).count(), 2);
        assert_eq!(
            &kinds[..3],
            [CarKind::Locomotive, CarKind::Coach, CarKind::Coach]
        );
        assert!(
            kinds[3..]
                .iter()
                .all(|&k| matches!(k, CarKind::Boxcar | CarKind::Hopper))
        );
        // The coaches' middle: past the locomotive (14 m) and its coupling,
        // half way along two 12 m coaches with a coupling between them.
        assert!(
            (train.stop_offset() - 27.5).abs() < 1e-4,
            "{}",
            train.stop_offset()
        );
    }

    #[test]
    fn each_run_stops_at_monte_nievas_for_the_dwell() {
        let (_, train) = shipped();
        use LinePhase::*;
        let down = stretches(&train, Run::Down);
        let order: Vec<_> = down.iter().map(|s| (s.0, s.1)).collect();
        assert_eq!(
            order,
            [(Running, None), (Dwelling, Some(NIEVAS)), (Running, None)]
        );
        let dwell = down[1].3 - down[1].2;
        assert!((dwell - 25.0).abs() < 0.11, "dwell {dwell}");
        let up = stretches(&train, Run::Up);
        let order: Vec<_> = up.iter().map(|s| (s.0, s.1)).collect();
        assert_eq!(
            order,
            [(Running, None), (Dwelling, Some(NIEVAS)), (Running, None)]
        );
        // Both ends of each run stand at the terminals.
        assert_eq!(train.state_at(Run::Down, 0.0).station, Some(CASTEX));
        let end = train.run_duration(Run::Down);
        assert_eq!(train.state_at(Run::Down, end).station, Some(PICO));
        assert_eq!(train.state_at(Run::Up, 0.0).station, Some(PICO));
        let end = train.run_duration(Run::Up);
        assert_eq!(train.state_at(Run::Up, end).station, Some(CASTEX));
        // About 11 real minutes from end to end.
        assert!((600.0..800.0).contains(&end), "{end}");
    }

    #[test]
    fn the_timetable_runs_castex_pico_and_back_each_day() {
        let (_, mut train) = shipped();
        // Day 0, midnight: overnight at Castex, facing Pico.
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(CASTEX), Run::Down)
        );
        assert_eq!(train.next_departure_after(0.0), (hm(7, 0), Run::Down));
        assert_eq!(train.next_departure_after(hm(8, 0)), (hm(16, 0), Run::Up));
        assert_eq!(
            train.next_departure_after(hm(17, 0)),
            (DAY_SECONDS + hm(7, 0), Run::Down)
        );
        // Sample the whole day at the base rate, in clock minutes.
        let mut seen: Vec<(LinePhase, Option<usize>)> = Vec::new();
        let mut now = 0.0;
        while now < DAY_SECONDS {
            train.advance(now, 60.0, BASE_RATE);
            now += 60.0;
            let s = train.state();
            let at = (s.phase, s.station);
            if seen.last() != Some(&at) {
                seen.push(at);
            }
        }
        use LinePhase::*;
        assert_eq!(
            seen,
            [
                (Parked, Some(CASTEX)),
                (Running, None),
                (Dwelling, Some(NIEVAS)),
                (Running, None),
                (Parked, Some(PICO)),
                (Running, None),
                (Dwelling, Some(NIEVAS)),
                (Running, None),
                (Parked, Some(CASTEX)),
            ]
        );
        // The next day runs the same.
        let pico = train.arrival_time_at(Run::Down, PICO).unwrap().seconds();
        train.advance(now, hm(7, 0) + 1.0, BASE_RATE);
        assert_eq!(train.state().phase, Running);
        train.advance(DAY_SECONDS + hm(7, 0) + 1.0, pico - hm(7, 0), BASE_RATE);
        assert_eq!(train.state().station, Some(PICO));
        assert_eq!(train.state().phase, Parked);
    }

    #[test]
    fn the_train_turns_around_during_the_layover() {
        let (_, mut train) = shipped();
        let arrive = train.arrival_time_at(Run::Down, PICO).unwrap().seconds();
        train.advance(0.0, arrive + 60.0, BASE_RATE);
        assert_eq!(train.state().run, Run::Down, "arrives facing east");
        train.advance(arrive + 60.0, hm(16, 0) - 120.0 - arrive, BASE_RATE);
        let s = train.state();
        assert_eq!(
            (s.phase, s.run),
            (LinePhase::Parked, Run::Up),
            "ready to leave west"
        );
    }

    #[test]
    fn never_faster_than_65_kmh_and_never_backs_up() {
        let (_, train) = shipped();
        for run in [Run::Down, Run::Up] {
            let sign = if run == Run::Down { 1.0 } else { -1.0 };
            let mut last = train.state_at(run, 0.0);
            let dt = 0.02;
            let mut top: f32 = 0.0;
            for k in 1..=(train.run_duration(run) / dt) as usize {
                let s = train.state_at(run, k as f64 * dt);
                assert!(s.speed <= 65.0 / 3.6 + 1e-4, "{run:?} at {s:?}");
                let moved = (s.front - last.front) * sign;
                assert!(moved >= -1e-3, "{run:?} backs up at step {k}");
                assert!(
                    moved <= 65.0 / 3.6 * dt as f32 + 2e-2,
                    "{run:?} jumps at {k}"
                );
                top = top.max(s.speed);
                last = s;
            }
            assert!(top > 18.0, "reaches its top speed: {top}");
        }
    }

    #[test]
    fn stops_put_the_coaches_exactly_at_each_station_projection() {
        let (towns, train) = shipped();
        for run in [Run::Down, Run::Up] {
            for (index, station) in train.stations().iter().enumerate() {
                let (arrive, _) = train.stop_times(run, index).unwrap();
                let s = train.state_at(run, arrive);
                assert_eq!(s.station, Some(index));
                assert_eq!(s.speed, 0.0);
                let sign = if run == Run::Down { 1.0 } else { -1.0 };
                let coaches = s.front - sign * train.stop_offset();
                assert!((coaches - station.mark).abs() < 1e-3, "{run:?} {index}");
                // The mark is the station's projection onto its own track.
                let at = towns[station.town]
                    .buildings
                    .iter()
                    .find(|b| b.kind == BuildingKind::Station)
                    .unwrap()
                    .footprint
                    .center();
                let rail = towns[station.town].railway.unwrap();
                let along = train.path().at(station.mark);
                let across = (at - along).dot(train.path().direction_at(station.mark));
                assert!(across.abs() < 1e-2, "{run:?} {index}: {across}");
                assert!((along.y - rail.start.y).abs() < 1e-2);
            }
        }
    }

    #[test]
    fn a_clock_jump_lands_where_ticking_would() {
        let (_, start) = shipped();
        // From 06:50, through the morning run and into the layover.
        let from = hm(6, 50);
        for real in [30.0, 400.0, 720.0, 2000.0] {
            let mut ticked = start.clone();
            let mut jumped = start.clone();
            ticked.advance(0.0, from, BASE_RATE);
            jumped.advance(0.0, from, BASE_RATE);
            let mut now = from;
            let dt = 1.0 / 60.0;
            for _ in 0..(real / dt) as usize {
                ticked.advance(now, dt * BASE_RATE, BASE_RATE);
                now += dt * BASE_RATE;
            }
            jumped.advance(from, now - from, BASE_RATE);
            let (a, b) = (ticked.state(), jumped.state());
            assert_eq!(
                (a.phase, a.station, a.run),
                (b.phase, b.station, b.run),
                "{real}"
            );
            assert!((a.front - b.front).abs() < 1e-2, "{real}: {a:?} vs {b:?}");
            assert!((a.speed - b.speed).abs() < 1e-3);
        }
    }

    #[test]
    fn riding_slows_the_clock_but_not_the_train() {
        let (_, mut fast) = shipped();
        let mut slow = fast.clone();
        fast.advance(0.0, hm(7, 0), BASE_RATE);
        slow.advance(0.0, hm(7, 0), BASE_RATE);
        // 100 real seconds at 30× and at 12×.
        fast.advance(hm(7, 0), 100.0 * BASE_RATE, BASE_RATE);
        slow.advance(hm(7, 0), 100.0 * 12.0, 12.0);
        assert!((fast.state().front - slow.state().front).abs() < 1e-3);
    }

    #[test]
    fn helpers_tell_when_the_train_reaches_a_station() {
        let (_, mut train) = shipped();
        let morning = train.first_stop_at(0).unwrap();
        let (arrive, depart) = train.stop_times(Run::Down, NIEVAS).unwrap();
        let expected = hm(7, 0) + (arrive + depart) / 2.0 * BASE_RATE;
        assert!((morning.seconds() - expected).abs() < 1e-6);
        assert_eq!(train.first_stop_at(99), None);

        // From midnight, waiting at Monte Nievas lands on the morning stop.
        let wait = train.time_until_at(NIEVAS, 0.0).unwrap();
        train.advance(0.0, wait, BASE_RATE);
        let s = train.state();
        assert_eq!((s.phase, s.station), (LinePhase::Dwelling, Some(NIEVAS)));
        assert_eq!(train.time_until_at(NIEVAS, wait), Some(0.0));
        // Ride on: the next stop is Pico, the rest of the run away.
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, PICO);
        let total = train.run_duration(Run::Down);
        assert!((left - (total - arrive)).abs() < 0.1, "{left}");
        // Waiting at Castex now means the evening arrival.
        let castex = train.time_until_at(CASTEX, wait).unwrap();
        let evening = train.arrival_time_at(Run::Up, CASTEX).unwrap().seconds();
        assert!((wait + castex - evening).abs() < 0.1);
        train.advance(wait, castex, BASE_RATE);
        assert_eq!(train.state().station, Some(CASTEX));
        assert_eq!(train.next_stop(), None, "parked");
    }

    #[test]
    fn cars_follow_the_curves_coupled_in_order() {
        let (_, mut train) = shipped();
        // Find a moment with the train on a link's curve.
        let mut now = 0.0;
        train.advance(now, hm(7, 0), BASE_RATE);
        now += hm(7, 0);
        let mut bent = false;
        for _ in 0..2000 {
            train.advance(now, 0.25 * BASE_RATE, BASE_RATE);
            now += 0.25 * BASE_RATE;
            let cars = train.placed_cars();
            assert_eq!(cars.len(), train.cars().len());
            for car in &cars {
                // Both ends of every car sit on the line.
                let half = car.direction * (car.length / 2.0);
                for end in [car.center + half, car.center - half] {
                    let on = train.path().at(train.path().project(end));
                    assert!(on.distance(end) < 0.05, "{car:?}");
                }
            }
            for pair in cars.windows(2) {
                let gap = (pair[0].center - pair[0].direction * (pair[0].length / 2.0))
                    .distance(pair[1].center + pair[1].direction * (pair[1].length / 2.0));
                assert!((gap - LineTrain::COUPLING).abs() < 0.05, "{gap}");
            }
            let first = cars[0].direction;
            let last = cars[cars.len() - 1].direction;
            if first.dot(last) < 0.9999 {
                bent = true;
                break;
            }
        }
        assert!(bent, "the consist bends round a curve");
        assert!(!train.obstacles().is_empty());
    }

    #[test]
    fn the_cars_block_and_shove_a_rider() {
        let (towns, mut train) = shipped();
        // Dwelling at Monte Nievas, on its straight east-west track.
        let wait = train.time_until_at(NIEVAS, 0.0).unwrap();
        train.advance(0.0, wait, BASE_RATE);
        let mut world = World::with_towns(
            crate::WorldConfig {
                half_extent: crate::WorldConfig::MAX_HALF_EXTENT,
                ..Default::default()
            },
            Terrain::default(),
            &towns,
        );
        world.moving = train.obstacles();
        let car = train.placed_cars()[1];
        let rail_y = car.center.y;
        let p = RiderParams::default();
        for (offset, north) in [(0.4, true), (-0.4, false)] {
            let at = Vec2::new(car.center.x, rail_y + offset);
            let mut r = Rider::new(at, Heading::from_radians(0.0), &world.terrain);
            r.step(RiderInput::default(), &p, &world, MAX_STEP);
            let clear = LineTrain::WIDTH / 2.0 + Rider::COLLIDER_RADIUS - 1e-3;
            if north {
                assert!(r.position.y >= rail_y + clear, "{:?}", r.position);
            } else {
                assert!(r.position.y <= rail_y - clear, "{:?}", r.position);
            }
        }
    }

    #[test]
    fn a_rider_never_ends_inside_a_car_on_a_slanted_link() {
        let (towns, mut train) = shipped();
        // Out on the long diagonal link to Pico, mid-run.
        let (_, depart) = train.stop_times(Run::Down, NIEVAS).unwrap();
        let at = hm(7, 0) + (depart + 200.0) * BASE_RATE;
        train.advance(0.0, at, BASE_RATE);
        let car = train.placed_cars()[2];
        assert!(
            car.direction.x.abs().max(car.direction.y.abs()) < 0.999,
            "{car:?}"
        );
        let mut world = World::with_towns(
            crate::WorldConfig {
                half_extent: crate::WorldConfig::MAX_HALF_EXTENT,
                ..Default::default()
            },
            Terrain::default(),
            &towns,
        );
        world.moving = train.obstacles();
        let c = crate::Circle::new(Rider::COLLIDER_RADIUS);
        let across = Vec2::new(-car.direction.y, car.direction.x);
        for k in 0..=20 {
            let along = (k as f32 / 20.0 - 0.5) * car.length;
            for off in [-1.6, -0.5, 0.0, 0.5, 1.6] {
                let start = car.center + car.direction * along + across * off;
                let end = world.resolve(c, start, |_| ());
                let deepest = world
                    .moving
                    .iter()
                    .filter_map(|o| o.push_out(c, end))
                    .map(Vec2::length)
                    .fold(0.0, f32::max);
                assert!(deepest < 1e-3, "{start:?} -> {end:?} overlaps by {deepest}");
                // Shoved off to one side, clear of the car's centerline.
                let side = (end - car.center).dot(across).abs();
                assert!(
                    side >= LineTrain::WIDTH / 2.0 - 1e-3,
                    "{start:?} -> {end:?}"
                );
            }
        }
    }
}
