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
    /// Small steam tank engine (a 2-6-2T): side tanks and a coal bunker on
    /// the engine itself, no tender. Hauls the branch shuttle.
    TankEngine,
    Boxcar,
    /// Open grain hopper.
    Hopper,
    /// Passenger coach.
    Coach,
}

impl CarKind {
    /// Height of the car's highest point (roof, or the locomotive's
    /// chimney) above the rail tops, in meters, as render draws it.
    pub fn height(self) -> f32 {
        match self {
            // The chimney's flared cap.
            CarKind::Locomotive => 4.55,
            // The shorter chimney of a branch engine.
            CarKind::TankEngine => 4.2,
            CarKind::Coach => 4.16,
            CarKind::Boxcar => 4.06,
            // The heaped grain in the open bin.
            CarKind::Hopper => 3.56,
        }
    }
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

impl PlacedCar {
    /// The car's footprint as axis-aligned boxes: one box for a car along
    /// an axis, a row of short boxes for a car on a slant, so the boxes hug
    /// its sides.
    pub fn boxes(&self) -> Vec<Aabb> {
        let d = self.direction;
        let pieces = if d.x.abs().max(d.y.abs()) > 0.999 {
            1
        } else {
            (self.length / LineTrain::WIDTH).ceil().max(1.0) as usize
        };
        let piece = self.length / pieces as f32;
        let size = Vec2::new(
            d.x.abs() * piece + d.y.abs() * LineTrain::WIDTH,
            d.y.abs() * piece + d.x.abs() * LineTrain::WIDTH,
        );
        (0..pieces)
            .map(|i| {
                let offset = (i as f32 + 0.5) * piece - self.length / 2.0;
                Aabb::from_center_size(self.center + d * offset, size)
            })
            .collect()
    }
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
    /// Whether the train stands on a bay track here (see
    /// [`LineTrain::on_path_stopping`]) instead of beside the platform.
    pub bay: bool,
}

/// Departure times and motion of a train.
///
/// Each terminal has its departure times, one or more per day. The earliest
/// departure of the day belongs to the morning run ([`Self::morning_run`]);
/// the train spends the night at the terminal it leaves from. A train that
/// is in at a terminal leaves at that terminal's next departure.
#[derive(Debug, Clone, PartialEq)]
pub struct Timetable {
    /// When the train leaves the path's first station on a down run (for
    /// the line, Castex).
    pub down: Vec<TimeOfDay>,
    /// When the train leaves the path's last station on an up run (for the
    /// line, Pico).
    pub up: Vec<TimeOfDay>,
    /// Real seconds standing at each station partway through a run.
    pub dwell: f32,
    /// Top speed, in m/s.
    pub top_speed: f32,
    /// Pulling-away and braking rate, in m/s².
    pub acceleration: f32,
}

/// Why a [`Timetable`] cannot be used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimetableError {
    /// Two departures, from either terminal, are at the same time of day,
    /// so the train could keep only one of them.
    SameDeparture(TimeOfDay),
    /// A terminal has no departure at all, so the train could never leave
    /// it.
    NoDeparture(Run),
}

impl std::fmt::Display for TimetableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SameDeparture(at) => write!(
                f,
                "timetable: two departures are at {at}; give each its own time"
            ),
            Self::NoDeparture(run) => write!(
                f,
                "timetable: no departure for the {run:?} run; give each terminal at least one"
            ),
        }
    }
}

impl std::error::Error for TimetableError {}

impl Timetable {
    /// 65 km/h, the line's top speed.
    pub const TOP_SPEED: f32 = 65.0 / 3.6;

    /// Whether a train can run on this timetable: each terminal needs a
    /// departure, and no two departures share a time of day.
    pub fn check(&self) -> Result<(), TimetableError> {
        for run in [Run::Down, Run::Up] {
            if self.departures(run).is_empty() {
                return Err(TimetableError::NoDeparture(run));
            }
        }
        let all: Vec<TimeOfDay> = self.down.iter().chain(&self.up).copied().collect();
        for (i, at) in all.iter().enumerate() {
            if all[..i].contains(at) {
                return Err(TimetableError::SameDeparture(*at));
            }
        }
        Ok(())
    }

    /// Every time of day `run` leaves its terminal, as listed.
    pub fn departures(&self, run: Run) -> &[TimeOfDay] {
        match run {
            Run::Down => &self.down,
            Run::Up => &self.up,
        }
    }

    /// The earliest time of day `run` leaves its terminal (midnight when it
    /// never does).
    pub fn departure(&self, run: Run) -> TimeOfDay {
        self.departures(run)
            .iter()
            .copied()
            .min_by(|a, b| a.seconds().total_cmp(&b.seconds()))
            .unwrap_or_default()
    }

    /// The run that leaves first each day (the down run on a tie). The
    /// train spends the night at the terminal it leaves from.
    pub fn morning_run(&self) -> Run {
        if self.departure(Run::Up).seconds() < self.departure(Run::Down).seconds() {
            Run::Up
        } else {
            Run::Down
        }
    }
}

impl Default for Timetable {
    fn default() -> Self {
        Self {
            // Overnight at Pico: the morning run leaves Pico, the
            // afternoon run leaves Castex (user, 2026-10-21).
            up: vec![TimeOfDay::hm(7, 0)],
            down: vec![TimeOfDay::hm(16, 0)],
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
/// Its morning run ([`Timetable::morning_run`]) leaves the terminal where it
/// spent the night (Pico, at [`Timetable::depart_pico`], by default), stops
/// at every station on the way, lays over at the other terminal (Castex),
/// leaves it at its own departure time ([`Timetable::depart_castex`]) and
/// runs back for the night. The clock only triggers departures: once running, the
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
    /// Height of the rail tops above the ground: where the cars stand.
    pub const RAIL_TOP: f32 = 0.5;

    /// The train of `line` through `towns`, parked overnight at midnight of
    /// day 0 at the terminal its morning run leaves from (Pico by default),
    /// facing that run. A `timetable` that fails [`Timetable::check`] is
    /// replaced by the default one. `None` when the line serves fewer than
    /// two stations or is too short for the train.
    pub fn new(line: &RailLine, towns: &[TownLayout], timetable: Timetable) -> Option<Self> {
        let timetable = match timetable.check() {
            Ok(()) => timetable,
            Err(_) => Timetable::default(),
        };
        Self::on_path(
            line.full_path(towns),
            &line.connected_towns(),
            towns,
            line_consist(),
            timetable,
        )
    }

    /// A train of `cars` on any `path`, serving `served` (indices into
    /// `towns`, in the order the path reaches them) on `timetable`, parked
    /// overnight at the terminal its morning run leaves from, facing that
    /// run. A town without a railway has no station. `None` when fewer than
    /// two stations remain, the path is too short for the consist, or the
    /// `timetable` fails [`Timetable::check`].
    pub fn on_path(
        path: LinePath,
        served: &[usize],
        towns: &[TownLayout],
        cars: Vec<Car>,
        timetable: Timetable,
    ) -> Option<Self> {
        Self::on_path_stopping(path, served, &[], towns, cars, timetable)
    }

    /// [`Self::on_path`], with `stops` naming where the train stands at a
    /// town instead of beside its station building: pairs of a town index
    /// and a ground point, whose nearest point on the path is the stop
    /// (the middle of the consist at a terminal). A bay track gives the
    /// shuttle's stop at the junction so.
    pub fn on_path_stopping(
        path: LinePath,
        served: &[usize],
        stops: &[(usize, Vec2)],
        towns: &[TownLayout],
        cars: Vec<Car>,
        timetable: Timetable,
    ) -> Option<Self> {
        timetable.check().ok()?;
        let stations: Vec<Station> = served
            .iter()
            .copied()
            .filter_map(|town| {
                let layout = towns.get(town)?;
                let rail = layout.railway?;
                let bay = stops.iter().find(|(t, _)| *t == town).map(|&(_, at)| at);
                let at = bay
                    .or_else(|| {
                        layout
                            .buildings
                            .iter()
                            .find(|b| b.kind == BuildingKind::Station)
                            .map(|b| b.footprint.center())
                    })
                    .unwrap_or((rail.start + rail.end) * 0.5);
                Some(Station {
                    town,
                    mark: path.project(at),
                    bay: bay.is_some(),
                })
            })
            .collect();
        let length = consist_length(&cars);
        if stations.len() < 2 || path.length() < length + 1.0 {
            return None;
        }
        let stop_offset = coaches_middle(&cars).unwrap_or(length / 2.0);
        let marks: Vec<f32> = stations.iter().map(|s| s.mark).collect();
        let (down, up) = stop_progress(&marks, path.length(), length, stop_offset);
        // Overnight at the terminal the morning run leaves from.
        let overnight = match timetable.morning_run() {
            Run::Down => 0,
            Run::Up => stations.len() - 1,
        };
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
                station: overnight,
                since: 0.0,
                turned: true,
            },
        };
        train.down = train.plan(&down);
        train.up = train.plan(&up);
        Some(train)
    }

    /// The shuttle on the branch `path` (as [`RailLine::shuttle_paths`]
    /// gives it), serving `served` (the junction town first, then the
    /// branch's) and standing at `stops` (see [`Self::on_path_stopping`];
    /// the junction bay's [`crate::RailBay::stop`]), that meets `line` at
    /// the junction: its timetable comes
    /// from [`shuttle_timetable`]. `None` when the branch cannot hold the
    /// shuttle, `line` does not stop at the junction, or the derived
    /// timetable does not fit ([`Self::schedule_fits`]).
    pub fn branch_shuttle(
        line: &LineTrain,
        path: LinePath,
        served: &[usize],
        stops: &[(usize, Vec2)],
        towns: &[TownLayout],
    ) -> Option<Self> {
        let junction = *served.first()?;
        // Run times do not depend on the departures, so a placeholder
        // timetable on the same motion gives the run from the far terminal
        // to the junction.
        let motion = line.timetable();
        let placeholder = Timetable {
            down: vec![TimeOfDay::hm(12, 0)],
            up: vec![TimeOfDay::hm(13, 0)],
            ..motion.clone()
        };
        let probe = Self::on_path_stopping(
            path.clone(),
            served,
            stops,
            towns,
            branch_consist(),
            placeholder,
        )?;
        let timetable = shuttle_timetable(line, junction, probe.run_duration(Run::Up))?;
        let shuttle =
            Self::on_path_stopping(path, served, stops, towns, branch_consist(), timetable)?;
        shuttle.schedule_fits().then_some(shuttle)
    }

    /// Every train of `line` through `towns`: the line train on `timetable`
    /// first (none when the line cannot hold one), then the shuttle of each
    /// branch that has a bay, meeting the line train at the junction
    /// ([`Self::branch_shuttle`]). A branch with no bay, or whose shuttle
    /// does not fit, has no shuttle.
    pub fn all_of(line: &RailLine, towns: &[TownLayout], timetable: Timetable) -> Vec<Self> {
        let Some(train) = Self::new(line, towns, timetable) else {
            return Vec::new();
        };
        let paths = line.shuttle_paths(towns);
        let shuttles: Vec<Self> = line
            .branches
            .iter()
            .zip(paths)
            .enumerate()
            .filter_map(|(index, (branch, path))| {
                let bay = line.bay_of(index)?;
                Self::branch_shuttle(&train, path, &branch.towns, &[(bay.town, bay.stop)], towns)
            })
            .collect();
        std::iter::once(train).chain(shuttles).collect()
    }

    /// Whether every run is over before the next departure, whichever end
    /// it leaves from: departures, taken over a day, alternate between the
    /// terminals and each leaves after the previous run has arrived.
    pub fn schedule_fits(&self) -> bool {
        let mut events: Vec<(f64, Run)> = [Run::Down, Run::Up]
            .into_iter()
            .flat_map(|run| {
                self.timetable
                    .departures(run)
                    .iter()
                    .map(move |at| (at.seconds(), run))
            })
            .collect();
        events.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = events.len();
        (0..n).all(|i| {
            let (at, run) = events[i];
            let (next, other) = events[(i + 1) % n];
            let gap = (next - at).rem_euclid(DAY_SECONDS);
            let gap = if gap == 0.0 { DAY_SECONDS } else { gap };
            other == run.back() && gap >= self.run_duration(run) * BASE_RATE
        })
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

    /// When the earliest departure of `run` reaches `station` by the
    /// timetable, with the clock at the base rate the whole way. See
    /// [`Self::arrivals_at`] for a train with several departures.
    pub fn arrival_time_at(&self, run: Run, station: usize) -> Option<TimeOfDay> {
        self.arrivals_at(run, station).into_iter().next()
    }

    /// When each departure of `run` (earliest first) reaches `station` by
    /// the timetable, with the clock at the base rate the whole way. Empty
    /// when the run does not stop there.
    pub fn arrivals_at(&self, run: Run, station: usize) -> Vec<TimeOfDay> {
        let Some((arrive, _)) = self.stop_times(run, station) else {
            return Vec::new();
        };
        self.departures_sorted(run)
            .into_iter()
            .map(|depart| TimeOfDay::from_seconds(depart + arrive * BASE_RATE))
            .collect()
    }

    /// The time of day half way through the earliest departure of `run`'s
    /// stop at `town` (by default the morning up run from Pico, or the
    /// afternoon down run from Castex), with the clock at the base rate:
    /// start the clock there and the train stands at that town's station.
    /// See [`Self::stops_at`] for a train with several departures.
    pub fn stop_at(&self, town: usize, run: Run) -> Option<TimeOfDay> {
        self.stops_at(town, run).into_iter().next()
    }

    /// [`Self::stop_at`] for every departure of `run`, earliest first.
    pub fn stops_at(&self, town: usize, run: Run) -> Vec<TimeOfDay> {
        let Some(station) = self.station_of_town(town) else {
            return Vec::new();
        };
        let Some((arrive, depart)) = self.stop_times(run, station) else {
            return Vec::new();
        };
        self.departures_sorted(run)
            .into_iter()
            .map(|start| TimeOfDay::from_seconds(start + (arrive + depart) / 2.0 * BASE_RATE))
            .collect()
    }

    /// The seconds of day of each departure of `run`, earliest first.
    fn departures_sorted(&self, run: Run) -> Vec<f64> {
        let mut all: Vec<f64> = self
            .timetable
            .departures(run)
            .iter()
            .map(|at| at.seconds())
            .collect();
        all.sort_by(f64::total_cmp);
        all
    }

    /// Whether the train stands on a bay track at the station it is at.
    pub fn stands_in_bay(&self) -> bool {
        self.state()
            .station
            .and_then(|s| self.stations.get(s))
            .is_some_and(|s| s.bay)
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

    /// Puts the train `elapsed` real seconds into `run` (clamped to the
    /// run), as if it had left its terminal on time and run since: how the
    /// game starts with the train already under way. Bad input starts the
    /// run.
    pub fn place_on_run(&mut self, run: Run, elapsed: f64) {
        let elapsed = if elapsed.is_finite() {
            elapsed.clamp(0.0, self.run_duration(run))
        } else {
            0.0
        };
        self.motion = Motion::Running { run, elapsed };
    }

    /// The next station the train arrives at (never one it already stands
    /// at) and the game seconds until it does, with the clock running from
    /// game time `now` at `rate` game seconds per real second: the rest of
    /// a ride, aboard. From a terminal it waits for its departure first.
    /// `None` on bad input.
    pub fn next_arrival(&self, now: f64, rate: f64) -> Option<(usize, f64)> {
        self.arrival_ahead(now, rate)
            .map(|(station, game, _, _)| (station, game))
    }

    /// Jumps the train to its next arrival ([`Self::next_arrival`]) and
    /// returns that station and the game seconds the clock must jump. The
    /// train lands exactly where following the clock at `rate` would, as
    /// the pure [`Self::state_at`] of its run at that arrival.
    pub fn skip_to_next_stop(&mut self, now: f64, rate: f64) -> Option<(usize, f64)> {
        self.skip_toward_next_stop(now, rate, 0.0)
    }

    /// Like [`Self::skip_to_next_stop`], but lands `lead` real seconds
    /// before the arrival (never behind where the train already is), so it
    /// still runs in and brakes to a stop. Returns the station and the game
    /// seconds the clock must jump.
    pub fn skip_toward_next_stop(
        &mut self,
        now: f64,
        rate: f64,
        lead: f64,
    ) -> Option<(usize, f64)> {
        let (station, game, run, arrive) = self.arrival_ahead(now, rate)?;
        let lead = if lead.is_finite() { lead.max(0.0) } else { 0.0 };
        // Where the train is on that run now: a laid-over train has not
        // started it yet.
        let (elapsed, wait) = match self.motion {
            Motion::Running { run: on, elapsed } if on == run => (elapsed, 0.0),
            _ => (0.0, game - arrive * rate),
        };
        let land = (arrive - lead).max(elapsed);
        if land > elapsed || wait > 0.0 {
            self.motion = Motion::Running { run, elapsed: land };
        }
        Some((station, wait + (land - elapsed) * rate))
    }

    /// Jumps a train standing at a station (dwelling, or laid over at a
    /// terminal) to the moment it pulls out, and returns the game seconds
    /// the clock must jump with it at `rate`: the rest of the dwell, or the
    /// wait for the departure from a terminal. A train already running
    /// between stations stays as it is (zero). `None` on bad input.
    pub fn skip_to_departure(&mut self, now: f64, rate: f64) -> Option<f64> {
        if !(now.is_finite() && rate.is_finite() && rate > 0.0) {
            return None;
        }
        let (terminal, since) = match self.motion {
            Motion::Running { run, elapsed }
                if elapsed < self.run_duration(run) - ARRIVAL_EPSILON =>
            {
                let plan = self.plan_of(run);
                let standing = plan.times.iter().find(|&&(arrive, depart)| {
                    elapsed >= arrive - ARRIVAL_EPSILON && elapsed <= depart
                });
                let Some(&(_, depart)) = standing else {
                    return Some(0.0);
                };
                self.motion = Motion::Running {
                    run,
                    elapsed: depart.max(elapsed),
                };
                return Some((depart - elapsed).max(0.0) * rate);
            }
            // Just in at its terminal: laid over from now.
            Motion::Running { run, .. } => (self.terminal(run), now),
            Motion::Parked { station, since, .. } => (station, since),
        };
        let depart = self.departure_from(terminal, since).max(now);
        self.motion = Motion::Running {
            run: self.departing(terminal),
            elapsed: 0.0,
        };
        Some(depart - now)
    }

    /// The next arrival from game time `now` with the clock at `rate`: the
    /// station, the game seconds until it, and the run and real seconds
    /// into it at which it happens.
    fn arrival_ahead(&self, now: f64, rate: f64) -> Option<(usize, f64, Run, f64)> {
        if !(now.is_finite() && rate.is_finite() && rate > 0.0) {
            return None;
        }
        // The run that brings the next arrival, how far into it the train
        // is, and the game seconds before it departs (laid over).
        let (run, elapsed, wait) = match self.motion {
            Motion::Running { run, elapsed }
                if elapsed < self.run_duration(run) - ARRIVAL_EPSILON =>
            {
                (run, elapsed, 0.0)
            }
            // Just in at its terminal: laid over from now.
            Motion::Running { run, .. } => {
                let terminal = self.terminal(run);
                let depart = self.departure_from(terminal, now);
                (self.departing(terminal), 0.0, depart - now)
            }
            Motion::Parked { station, since, .. } => {
                let depart = self.departure_from(station, since).max(now);
                (self.departing(station), 0.0, depart - now)
            }
        };
        let plan = self.plan_of(run);
        let k = plan
            .times
            .iter()
            .position(|&(arrive, _)| arrive > elapsed + ARRIVAL_EPSILON)?;
        let arrive = plan.times[k].0;
        Some((
            self.station_at(run, k),
            wait + (arrive - elapsed) * rate,
            run,
            arrive,
        ))
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
        self.placed_cars()
            .iter()
            .flat_map(|car| {
                let across = Vec2::new(-car.direction.y, car.direction.x);
                car.boxes()
                    .into_iter()
                    .map(move |footprint| SideObstacle { footprint, across })
            })
            .collect()
    }

    /// Stations (indices into [`Self::stations`]) where a stop leaves the
    /// coaches more than [`MAX_STOP_MISS`] from the station: too near an
    /// end of the line for the consist to stand there. Empty on a sound
    /// line; the game warns about any.
    pub fn misplaced_stops(&self) -> Vec<usize> {
        let marks: Vec<f32> = self.stations.iter().map(|s| s.mark).collect();
        let stops = |plan: &RunPlan| plan.stops.iter().map(|&p| p as f32).collect::<Vec<_>>();
        misplaced_stops(
            &marks,
            (&stops(&self.down), &stops(&self.up)),
            self.path.length(),
            self.stop_offset,
        )
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

    /// The first scheduled departure from terminal `station` at or after
    /// game time `since`.
    fn departure_from(&self, station: usize, since: f64) -> f64 {
        self.timetable
            .departures(self.departing(station))
            .iter()
            .map(|at| {
                let time = at.seconds();
                time + ((since - time) / DAY_SECONDS).ceil() * DAY_SECONDS
            })
            .fold(f64::INFINITY, f64::min)
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

/// The body of a passenger coach, as its drawing and the seat view share
/// it, in the car's own frame: heights above the rail tops, distances
/// across from its centreline and along from its middle, in meters.
pub struct CoachBody;

impl CoachBody {
    /// Top of the floor.
    pub const FLOOR: f32 = 1.3;
    /// Underside of the ceiling.
    pub const CEILING: f32 = 3.8;
    /// Outer face of each side wall from the centreline.
    pub const HALF_WIDTH: f32 = 1.4;
    /// Thickness of the walls.
    pub const WALL: f32 = 0.08;
    /// Bottom of the window openings: 0.8 m above the floor, low enough
    /// for a seated passenger to see the ground beside the track.
    pub const SILL: f32 = 2.1;
    /// Top of the window openings: 1.8 m above the floor, above the
    /// horizon from a seat.
    pub const LINTEL: f32 = 3.1;
    /// How far in from each end of the car its end walls stand.
    pub const END_INSET: f32 = 0.35;
    /// Height of a seated passenger's eyes above the floor.
    pub const EYE: f32 = 1.2;

    /// The window openings along each side of a coach `length` long: their
    /// centres from the car's middle, and their width.
    pub fn windows(length: f32) -> (Vec<f32>, f32) {
        let count = ((length - 3.0) / 1.6).floor().max(1.0) as usize;
        let pitch = (length - 3.0) / count as f32;
        let centres = (0..count)
            .map(|i| -length / 2.0 + 1.5 + pitch * (i as f32 + 0.5))
            .collect();
        (centres, pitch * 0.6)
    }

    /// Where a passenger sits by a window in a coach `length` long, as
    /// `(along, across, height)`: at the middle window on the car's right
    /// (`side` +1) or left (-1), half a meter in from the wall, eyes at
    /// seated height, facing forward along the car with that window beside.
    pub fn window_seat(length: f32, side: f32) -> (f32, f32, f32) {
        let (centres, _) = Self::windows(length);
        let along = centres[centres.len() / 2];
        let side = if side < 0.0 { -1.0 } else { 1.0 };
        (
            along,
            side * (Self::HALF_WIDTH - Self::WALL - 0.5),
            Self::FLOOR + Self::EYE,
        )
    }
}

/// Farthest the coaches' middle may stand from a station at a stop, in
/// meters, with the coaches still (nearly) alongside its 60 m platform: at a
/// terminal they stand about 19 m to one side.
pub const MAX_STOP_MISS: f32 = 20.0;

/// Where the front stops at each station (`marks`, in line order, on a
/// path `total` meters long), as progress along each run, for a consist
/// `length` meters long whose coaches' middle is `offset` behind its front:
/// the down run's stops in line order, then the up run's in its own order.
///
/// The coaches stop alongside each station on the way. At the two
/// terminals the whole consist stands centered on the station instead
/// (its coaches a little to one side), so it arrives and leaves on the
/// same stretch of track and turns around in place during the layover.
/// Every stop keeps the consist on the path, and a run never stops behind
/// its previous stop, so a station near a tip only stops the train short.
fn stop_progress(marks: &[f32], total: f32, length: f32, offset: f32) -> (Vec<f32>, Vec<f32>) {
    let onward = |stops: Vec<f32>| {
        let mut farthest = f32::MIN;
        stops
            .into_iter()
            .map(|p| {
                farthest = farthest.max(p);
                farthest
            })
            .collect::<Vec<f32>>()
    };
    let total = total.max(length);
    let last = marks.len().saturating_sub(1);
    let ahead = |i: usize| {
        if i == 0 || i == last {
            length / 2.0
        } else {
            offset
        }
    };
    let down = marks
        .iter()
        .enumerate()
        .map(|(i, &mark)| (mark + ahead(i)).clamp(length, total))
        .collect();
    let up = marks
        .iter()
        .enumerate()
        .rev()
        .map(|(i, &mark)| total - (mark - ahead(i)).clamp(0.0, total - length))
        .collect();
    (onward(down), onward(up))
}

/// Stations whose coaches' middle stands more than [`MAX_STOP_MISS`] from
/// the station at either run's stop (see [`stop_progress`]).
fn misplaced_stops(
    marks: &[f32],
    (down, up): (&[f32], &[f32]),
    total: f32,
    offset: f32,
) -> Vec<usize> {
    let n = marks.len();
    (0..n)
        .filter(|&i| {
            let down_coaches = down[i] - offset;
            let up_coaches = total - up[n - 1 - i] + offset;
            (down_coaches - marks[i]).abs() > MAX_STOP_MISS
                || (up_coaches - marks[i]).abs() > MAX_STOP_MISS
        })
        .collect()
}

/// Length of `cars` coupled together, in meters.
pub(crate) fn consist_length(cars: &[Car]) -> f32 {
    let total: f32 = cars.iter().map(|c| c.length).sum();
    total + LineTrain::COUPLING * cars.len().saturating_sub(1) as f32
}

/// The line train's consist, from the front: a rural mixed train, two
/// passenger coaches behind the locomotive, then freight.
pub(crate) fn line_consist() -> Vec<Car> {
    [
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
    .collect()
}

/// Game seconds the shuttle is in at the junction before `line` arrives
/// there, and stays after it leaves. Long enough for the whole line train
/// (93 m, pulling out at 0.5 m/s²) to clear the bay's turnout before the
/// shuttle follows it onto the shared run-out: 5 minutes left the two
/// trains within a car's width of each other there.
pub const SHUTTLE_MARGIN: f64 = 10.0 * 60.0;

/// The shuttle's timetable, derived from `line`'s stops at `junction`
/// (a town index): for each time `line` stops there, on either run, the
/// shuttle leaves its far terminal (the branch's last station, where it
/// spends the night) so that it is in at the junction [`SHUTTLE_MARGIN`]
/// before `line` arrives, and leaves the junction [`SHUTTLE_MARGIN`] after
/// `line` leaves. `to_junction` is the real seconds the shuttle's up run
/// takes; it moves like `line` (dwell, top speed, acceleration). Times
/// follow the base clock rate. `None` when `line` does not stop at the
/// junction.
pub fn shuttle_timetable(line: &LineTrain, junction: usize, to_junction: f64) -> Option<Timetable> {
    let station = line.station_of_town(junction)?;
    let mut stops: Vec<(f64, f64)> = Vec::new();
    for run in [Run::Down, Run::Up] {
        let (arrive, leave) = line.stop_times(run, station)?;
        for at in line.timetable().departures(run) {
            let from = at.seconds();
            stops.push((from + arrive * BASE_RATE, from + leave * BASE_RATE));
        }
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let up = stops
        .iter()
        .map(|&(arrive, _)| {
            TimeOfDay::from_seconds(arrive - SHUTTLE_MARGIN - to_junction * BASE_RATE)
        })
        .collect();
    let down = stops
        .iter()
        .map(|&(_, leave)| TimeOfDay::from_seconds(leave + SHUTTLE_MARGIN))
        .collect();
    Some(Timetable {
        down,
        up,
        ..line.timetable().clone()
    })
}

/// The branch shuttle's consist, from the front: a tank engine, one
/// passenger coach and a boxcar.
pub(crate) fn branch_consist() -> Vec<Car> {
    [
        // 12 m over the buffers: the coach then stands a meter off the middle
        // of the terminal, so a door is within boarding reach of the stop spot.
        (CarKind::TankEngine, 12.0),
        (CarKind::Coach, 12.0),
        (CarKind::Boxcar, 10.0),
    ]
    .into_iter()
    .map(|(kind, length)| Car { kind, length })
    .collect()
}

/// Distances from the front of the consist to the front of its first coach
/// and the back of its last one. `None` without coaches.
pub(crate) fn coaches_span(cars: &[Car]) -> Option<(f32, f32)> {
    let mut at = 0.0;
    let (mut first, mut last) = (None, None);
    for car in cars {
        if car.kind == CarKind::Coach {
            first.get_or_insert(at);
            last = Some(at + car.length);
        }
        at += car.length + LineTrain::COUPLING;
    }
    Some((first?, last?))
}

/// Distance from the front of the consist to the middle of its coaches.
fn coaches_middle(cars: &[Car]) -> Option<f32> {
    let (first, last) = coaches_span(cars)?;
    Some((first + last) / 2.0)
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
pub(crate) mod line_tests {
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
        // About 9.5 real minutes from end to end at 1:7.
        assert!((500.0..650.0).contains(&end), "{end}");
    }

    #[test]
    fn the_timetable_runs_pico_castex_and_back_each_day() {
        let (_, mut train) = shipped();
        // Day 0, midnight: overnight at Pico, facing Castex.
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(PICO), Run::Up)
        );
        // The morning run leaves Pico at 07:00, the afternoon run leaves
        // Castex at 16:00.
        assert_eq!(train.next_departure_after(0.0), (hm(7, 0), Run::Up));
        assert_eq!(train.next_departure_after(hm(8, 0)), (hm(16, 0), Run::Down));
        assert_eq!(
            train.next_departure_after(hm(17, 0)),
            (DAY_SECONDS + hm(7, 0), Run::Up)
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
                (Parked, Some(PICO)),
                (Running, None),
                (Dwelling, Some(NIEVAS)),
                (Running, None),
                (Parked, Some(CASTEX)),
                (Running, None),
                (Dwelling, Some(NIEVAS)),
                (Running, None),
                (Parked, Some(PICO)),
            ]
        );
        // The next day runs the same.
        let castex = train.arrival_time_at(Run::Up, CASTEX).unwrap().seconds();
        train.advance(now, hm(7, 0) + 1.0, BASE_RATE);
        assert_eq!(train.state().phase, Running);
        assert_eq!(train.state().run, Run::Up);
        train.advance(DAY_SECONDS + hm(7, 0) + 1.0, castex - hm(7, 0), BASE_RATE);
        assert_eq!(train.state().station, Some(CASTEX));
        assert_eq!(train.state().phase, Parked);
    }

    #[test]
    fn the_timetable_names_each_departure_by_its_terminal() {
        let t = Timetable::default();
        assert_eq!(t.up, [TimeOfDay::hm(7, 0)]);
        assert_eq!(t.down, [TimeOfDay::hm(16, 0)]);
        assert_eq!(t.departure(Run::Up), t.up[0]);
        assert_eq!(t.departure(Run::Down), t.down[0]);
        assert_eq!(t.morning_run(), Run::Up);
        // Swapped times put the night at Castex instead.
        let swapped = Timetable {
            down: vec![TimeOfDay::hm(7, 0)],
            up: vec![TimeOfDay::hm(16, 0)],
            ..t
        };
        assert_eq!(swapped.morning_run(), Run::Down);
        let (towns, line) = crate::rail::tests::shipped();
        let train = LineTrain::new(&line, &towns, swapped).unwrap();
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(CASTEX), Run::Down)
        );
        assert_eq!(train.next_departure_after(0.0), (hm(7, 0), Run::Down));
    }

    #[test]
    fn equal_departure_times_are_rejected_and_the_default_is_used() {
        let same = Timetable {
            down: vec![TimeOfDay::hm(9, 0)],
            up: vec![TimeOfDay::hm(9, 0)],
            ..Timetable::default()
        };
        let error = same.check().expect_err("one time for both runs");
        assert_eq!(error, TimetableError::SameDeparture(TimeOfDay::hm(9, 0)));
        assert!(error.to_string().contains("09:00"), "{error}");
        assert_eq!(Timetable::default().check(), Ok(()));
        let (towns, line) = crate::rail::tests::shipped();
        let train = LineTrain::new(&line, &towns, same).unwrap();
        assert_eq!(*train.timetable(), Timetable::default());
    }

    /// The stretch of path the whole consist covers, as `(low, high)`
    /// distances along it.
    fn footprint(train: &LineTrain) -> (f32, f32) {
        let s = train.state();
        let l = train.length();
        match s.run {
            Run::Down => (s.front - l, s.front),
            Run::Up => (s.front, s.front + l),
        }
    }

    #[test]
    fn the_train_turns_around_in_place_on_the_same_stretch_of_track() {
        let (_, mut train) = shipped();
        let mut now = 0.0;
        // Through a whole day: every layover keeps one footprint, centered
        // on its terminal's station.
        let mut layovers = 0;
        let mut last: Option<(Option<usize>, (f32, f32))> = None;
        while now < DAY_SECONDS {
            train.advance(now, 30.0, BASE_RATE);
            now += 30.0;
            let s = train.state();
            if s.phase != LinePhase::Parked {
                last = None;
                continue;
            }
            let span = footprint(&train);
            let mark = train.stations()[s.station.unwrap()].mark;
            assert!(
                ((span.0 + span.1) / 2.0 - mark).abs() < 1e-3,
                "{span:?} vs {mark}"
            );
            match last {
                Some((station, before)) if station == s.station => {
                    assert!(
                        (span.0 - before.0).abs() < 1e-3,
                        "jumped at {now}: {before:?} -> {span:?}"
                    );
                }
                _ => layovers += 1,
            }
            last = Some((s.station, span));
        }
        assert!(layovers >= 2, "{layovers}");
    }

    #[test]
    fn a_station_near_a_line_tip_never_sends_the_train_backwards_and_is_reported() {
        let (length, offset, total) = (93.0, 27.5, 2000.0);
        // A terminal 10 m from the path's start, a station 15 m on, then
        // one well inside, and a terminal 5 m from the far tip.
        let marks = [10.0, 25.0, 900.0, 1995.0];
        let (down, up) = stop_progress(&marks, total, length, offset);
        for stops in [&down, &up] {
            assert!(stops.windows(2).all(|w| w[1] >= w[0]), "{stops:?}");
            assert!(
                stops.iter().all(|&p| (length..=total).contains(&p)),
                "{stops:?}"
            );
        }
        let missed = misplaced_stops(&marks, (&down, &up), total, offset);
        assert_eq!(missed, vec![0, 1, 3]);
        // Shipped, every stop is fine.
        let (_, train) = shipped();
        assert_eq!(train.misplaced_stops(), Vec::<usize>::new());
    }

    #[test]
    fn the_train_turns_around_during_the_layover() {
        let (_, mut train) = shipped();
        let arrive = train.arrival_time_at(Run::Up, CASTEX).unwrap().seconds();
        train.advance(0.0, arrive + 60.0, BASE_RATE);
        assert_eq!(train.state().run, Run::Up, "arrives facing west");
        train.advance(arrive + 60.0, hm(16, 0) - 120.0 - arrive, BASE_RATE);
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(CASTEX), Run::Down),
            "ready to leave east"
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
    fn stops_put_the_coaches_at_each_station_and_center_the_train_at_terminals() {
        let (towns, train) = shipped();
        for run in [Run::Down, Run::Up] {
            for (index, station) in train.stations().iter().enumerate() {
                let (arrive, _) = train.stop_times(run, index).unwrap();
                let s = train.state_at(run, arrive);
                assert_eq!(s.station, Some(index));
                assert_eq!(s.speed, 0.0);
                let sign = if run == Run::Down { 1.0 } else { -1.0 };
                let terminal = index == 0 || index + 1 == train.stations().len();
                // Through stations: the coaches alongside. Terminals: the
                // whole consist centered, to turn around in place.
                let ahead = if terminal {
                    train.length() / 2.0
                } else {
                    train.stop_offset()
                };
                let middle = s.front - sign * ahead;
                assert!((middle - station.mark).abs() < 1e-3, "{run:?} {index}");
                let coaches = s.front - sign * train.stop_offset();
                assert!((coaches - station.mark).abs() <= MAX_STOP_MISS);
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

    /// `train` followed frame by frame (1/60 s) from game time `now` at
    /// `rate` for `game` game seconds.
    fn ticked(train: &LineTrain, now: f64, game: f64, rate: f64) -> LineTrain {
        let mut train = train.clone();
        let step = rate / 60.0;
        let mut t = 0.0;
        while t + step < game {
            train.advance(now + t, step, rate);
            t += step;
        }
        train.advance(now + t, game - t, rate);
        train
    }

    #[test]
    fn skipping_lands_on_the_next_dwell_exactly_as_riding_on_would() {
        let (_, start) = shipped();
        let onboard = 12.0;
        // Running up from Pico, at Monte Nievas, parked at Castex, and
        // parked overnight at Pico before the morning run.
        let cases = [
            (hm(7, 0) + 100.0 * BASE_RATE, NIEVAS),
            (hm(10, 50), CASTEX),
            (hm(13, 0), NIEVAS),
            (hm(5, 0), NIEVAS),
        ];
        for (at, expected) in cases {
            let mut train = start.clone();
            train.advance(0.0, at, BASE_RATE);
            let (station, game) = train.next_arrival(at, onboard).expect("a next stop");
            assert_eq!(station, expected, "from {at}");
            let ridden = ticked(&train, at, game, onboard);
            let mut skipped = train.clone();
            assert_eq!(
                skipped.skip_to_next_stop(at, onboard),
                Some((station, game))
            );
            let (a, b) = (ridden.state(), skipped.state());
            assert_eq!((a.phase, a.station, a.run), (b.phase, b.station, b.run));
            assert_ne!(b.phase, LinePhase::Running, "from {at}");
            assert_eq!(b.station, Some(station));
            assert!((a.front - b.front).abs() < 1e-2, "{a:?} vs {b:?}");
            // The pure state of the run at the arrival.
            let (arrive, _) = skipped.stop_times(b.run, station).unwrap();
            assert_eq!(b, skipped.state_at(b.run, arrive));
        }
        // From the overnight layover the wait for the 07:00 departure
        // counts: game time to the departure, then the ride at 12×.
        let mut night = start.clone();
        night.advance(0.0, hm(5, 0), BASE_RATE);
        let (_, game) = night.next_arrival(hm(5, 0), onboard).unwrap();
        let (arrive, _) = night.stop_times(Run::Up, NIEVAS).unwrap();
        assert!(
            (game - (hm(2, 0) + arrive * onboard)).abs() < 1e-6,
            "{game}"
        );
        assert_eq!(night.next_arrival(hm(5, 0), 0.0), None);
        assert_eq!(night.next_arrival(f64::NAN, onboard), None);
    }

    #[test]
    fn pulling_out_now_and_coming_in_three_seconds_out_land_where_riding_would() {
        let (_, start) = shipped();
        let onboard = 12.0;
        // Dwelling at Monte Nievas on the morning run, half way through.
        let (arrive, depart) = start.stop_times(Run::Up, NIEVAS).unwrap();
        let at = hm(7, 0) + (arrive + depart) / 2.0 * BASE_RATE;
        let mut train = start.clone();
        train.advance(0.0, at, BASE_RATE);
        let mut left = train.clone();
        let game = left.skip_to_departure(at, onboard).unwrap();
        let ridden = ticked(&train, at, game, onboard);
        assert_eq!(left.state(), left.state_at(Run::Up, depart));
        assert!((ridden.state().front - left.state().front).abs() < 1e-3);
        // At the departure: a moment later it is running.
        let mut moving = left.clone();
        moving.advance(at + game, 0.05 * onboard, onboard);
        assert_eq!(moving.state().phase, LinePhase::Running);
        assert!(moving.state().speed > 0.0);
        // The rest of the dwell, at 12×.
        let elapsed = (at - hm(7, 0)) / BASE_RATE;
        assert!(elapsed > arrive && elapsed < depart);
        assert!((game - (depart - elapsed) * onboard).abs() < 1e-6);
        // Already running: nothing to skip.
        assert_eq!(
            moving.clone().skip_to_departure(at + game, onboard),
            Some(0.0)
        );
        assert_eq!(left.skip_to_departure(f64::NAN, onboard), None);

        // Then on to 3 s before Castex: running in, and riding the last
        // 3 s brakes it to a stop there.
        let now = at + game;
        let (station, jump) = left.skip_toward_next_stop(now, onboard, 3.0).unwrap();
        assert_eq!(station, CASTEX);
        let (castex, _) = left.stop_times(Run::Up, CASTEX).unwrap();
        assert_eq!(left.state(), left.state_at(Run::Up, castex - 3.0));
        assert!((jump - (castex - 3.0 - depart) * onboard).abs() < 1e-6);
        assert_eq!(left.state().phase, LinePhase::Running);
        let rolled = ticked(&left, now + jump, 3.0 * onboard, onboard);
        let s = rolled.state();
        assert_eq!((s.phase, s.station), (LinePhase::Parked, Some(CASTEX)));
        // Never backwards: less than the lead away, it stays put.
        let mut close = rolled.clone();
        close.place_on_run(Run::Down, 1.0);
        let before = close.state();
        let (_, none) = close.skip_toward_next_stop(now, onboard, 1e6).unwrap();
        assert_eq!((none, close.state()), (0.0, before));

        // Laid over at Castex: the wait for the 16:00 departure.
        let mut parked = start.clone();
        parked.advance(0.0, hm(13, 0), BASE_RATE);
        let wait = parked.skip_to_departure(hm(13, 0), onboard).unwrap();
        assert!((wait - (hm(16, 0) - hm(13, 0))).abs() < 1e-6);
        assert_eq!(parked.state(), parked.state_at(Run::Down, 0.0));
    }

    #[test]
    fn a_seated_passenger_looks_out_through_the_middle_of_the_window() {
        let floor = CoachBody::FLOOR;
        // A sill low enough to see the ground by the track, a top high
        // enough for the sky, and the eyes in between.
        let sill = CoachBody::SILL - floor;
        let top = CoachBody::LINTEL - floor;
        assert!((0.75..=0.85).contains(&sill), "sill {sill}");
        assert!(top >= 1.7, "top {top}");
        let (along, across, eye) = CoachBody::window_seat(12.0, 1.0);
        assert!((eye - floor - 1.2).abs() < 1e-5);
        let middle = (CoachBody::SILL + CoachBody::LINTEL) / 2.0;
        assert!(
            (eye - middle).abs() < 0.2,
            "eye {eye}, window middle {middle}"
        );
        let (windows, open) = CoachBody::windows(12.0);
        assert!(windows.iter().any(|&x| (along - x).abs() < open / 2.0));
        // On the side asked for.
        assert!(across > 0.0);
        let (_, left, _) = CoachBody::window_seat(12.0, -1.0);
        assert_eq!(left, -across);
    }

    #[test]
    fn a_train_placed_mid_run_stands_where_its_run_says() {
        let (_, mut train) = shipped();
        let (arrive, _) = train.stop_times(Run::Up, NIEVAS).unwrap();
        train.place_on_run(Run::Up, arrive - 50.0);
        assert_eq!(train.state(), train.state_at(Run::Up, arrive - 50.0));
        assert_eq!(train.state().phase, LinePhase::Running);
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, NIEVAS);
        assert!((left - 50.0).abs() < 1e-9);
        train.place_on_run(Run::Down, f64::NAN);
        assert_eq!(train.state(), train.state_at(Run::Down, 0.0));
    }

    #[test]
    fn helpers_tell_when_the_train_reaches_a_station() {
        let (_, mut train) = shipped();
        // The morning stop is half way through the up run's dwell, counted
        // from the 07:00 departure from Pico.
        let morning = train.stop_at(0, Run::Up).unwrap();
        let (arrive, depart) = train.stop_times(Run::Up, NIEVAS).unwrap();
        let expected = hm(7, 0) + (arrive + depart) / 2.0 * BASE_RATE;
        assert!((morning.seconds() - expected).abs() < 1e-6);
        assert_eq!(train.stop_at(99, Run::Down), None);
        assert_eq!(train.stop_at(99, Run::Up), None);
        let mut there = train.clone();
        there.advance(0.0, morning.seconds(), BASE_RATE);
        let s = there.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Dwelling, Some(NIEVAS), Run::Up)
        );

        // The afternoon stop counts from the 16:00 departure from Castex.
        let afternoon = train.stop_at(0, Run::Down).unwrap();
        let (down_arrive, down_depart) = train.stop_times(Run::Down, NIEVAS).unwrap();
        let expected = hm(16, 0) + (down_arrive + down_depart) / 2.0 * BASE_RATE;
        assert!((afternoon.seconds() - expected).abs() < 1e-6);

        // From midnight, waiting at Monte Nievas lands on the morning stop.
        let wait = train.time_until_at(NIEVAS, 0.0).unwrap();
        train.advance(0.0, wait, BASE_RATE);
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Dwelling, Some(NIEVAS), Run::Up)
        );
        assert_eq!(train.time_until_at(NIEVAS, wait), Some(0.0));
        // Ride on: the next stop is Castex, the rest of the run away.
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, CASTEX);
        let total = train.run_duration(Run::Up);
        assert!((left - (total - arrive)).abs() < 0.1, "{left}");
        // Waiting at Pico now means the evening arrival.
        let pico = train.time_until_at(PICO, wait).unwrap();
        let evening = train.arrival_time_at(Run::Down, PICO).unwrap().seconds();
        assert!((wait + pico - evening).abs() < 0.1);
        train.advance(wait, pico, BASE_RATE);
        assert_eq!(train.state().station, Some(PICO));
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
    fn a_car_on_a_slanted_link_is_covered_by_short_boxes_along_it() {
        let (_, mut train) = shipped();
        let (_, depart) = train.stop_times(Run::Down, NIEVAS).unwrap();
        let at = hm(16, 0) + (depart + 200.0) * BASE_RATE;
        train.advance(0.0, at, BASE_RATE);
        let car = train.placed_cars()[2];
        assert!(car.direction.x.abs().max(car.direction.y.abs()) < 0.999);
        let boxes = car.boxes();
        assert!(boxes.len() > 1, "{boxes:?}");
        let across = Vec2::new(-car.direction.y, car.direction.x);
        for k in 0..=20 {
            let along = (k as f32 / 20.0 - 0.5) * car.length * 0.98;
            for off in [-0.5, 0.0, 0.5] {
                let p = car.center + car.direction * along + across * off;
                assert!(boxes.iter().any(|b| b.contains_point(p)), "{p:?}");
            }
        }
        // The obstacles are these very boxes.
        let all: Vec<Aabb> = train
            .placed_cars()
            .iter()
            .flat_map(PlacedCar::boxes)
            .collect();
        let obstacles: Vec<Aabb> = train.obstacles().iter().map(|o| o.footprint).collect();
        assert_eq!(all, obstacles);
    }

    #[test]
    fn cars_stand_a_little_over_four_meters_above_the_rails() {
        for kind in [
            CarKind::Locomotive,
            CarKind::TankEngine,
            CarKind::Coach,
            CarKind::Boxcar,
            CarKind::Hopper,
        ] {
            let h = kind.height();
            assert!(h > 3.4 && h < 5.0, "{kind:?}: {h}");
        }
        assert!(CarKind::Locomotive.height() > CarKind::Coach.height());
        assert!(CarKind::TankEngine.height() < CarKind::Locomotive.height());
        assert!(CarKind::Hopper.height() < CarKind::Boxcar.height());
        const { assert!(LineTrain::RAIL_TOP > 0.0) };
    }

    #[test]
    fn a_rider_never_ends_inside_a_car_on_a_slanted_link() {
        let (towns, mut train) = shipped();
        // Out on the long diagonal link to Pico, mid-run.
        let (_, depart) = train.stop_times(Run::Down, NIEVAS).unwrap();
        let at = hm(16, 0) + (depart + 200.0) * BASE_RATE;
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

    pub(crate) const MEDIO: usize = 1;
    pub(crate) const NORTE: usize = 3;

    /// Oeste, Medio, Este and Norte, and their line with the branch to
    /// Norte off Medio.
    pub(crate) fn branched_line() -> (Vec<TownLayout>, RailLine) {
        let (towns, list, branches) = crate::rail::tests::branched();
        let bounds = crate::WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line)
    }

    /// The line train over Oeste, Medio and Este with a branch to Norte
    /// off Medio, and the shuttle on that branch.
    pub(crate) fn branched_trains(
        line_timetable: Timetable,
    ) -> (Vec<TownLayout>, LineTrain, LineTrain) {
        let (towns, line) = branched_line();
        let train = LineTrain::new(&line, &towns, line_timetable).expect("a line train");
        let path = line.shuttle_paths(&towns).remove(0);
        // It stands in Medio's bay track, not on the line train's.
        let bay = line.bay_of(0).expect("a bay at Medio");
        let stops = [(MEDIO, bay.stop)];
        let shuttle = LineTrain::branch_shuttle(&train, path, &[MEDIO, NORTE], &stops, &towns)
            .expect("a shuttle on the branch");
        (towns, train, shuttle)
    }

    /// `train` jumped from midnight of day 0 to game time `at`.
    fn jumped(train: &LineTrain, at: f64) -> LineTrain {
        let mut train = train.clone();
        train.advance(0.0, at, BASE_RATE);
        train
    }

    /// Checks that `shuttle` is standing at Medio's station from
    /// `SHUTTLE_MARGIN` before each stop of `line` there until
    /// `SHUTTLE_MARGIN` after it, and away from it just after.
    fn assert_meets_line(line: &LineTrain, shuttle: &LineTrain) {
        let station = line.station_of_town(MEDIO).expect("the line serves Medio");
        let eps = 1.0;
        let mut meetings = 0;
        for run in [Run::Down, Run::Up] {
            let (arrive, leave) = line.stop_times(run, station).unwrap();
            for at in line.timetable().departures(run) {
                let (arrive, leave) = (
                    at.seconds() + arrive * BASE_RATE,
                    at.seconds() + leave * BASE_RATE,
                );
                let standing = |t: f64| jumped(shuttle, t).state().station == Some(0);
                assert!(standing(arrive - SHUTTLE_MARGIN + eps), "in before {run:?}");
                assert!(standing(arrive), "in when {run:?} arrives");
                assert!(standing(leave), "in when {run:?} leaves");
                assert!(
                    standing(leave + SHUTTLE_MARGIN - eps),
                    "stays after {run:?}"
                );
                assert!(
                    !standing(leave + SHUTTLE_MARGIN + 5.0),
                    "gone after {run:?}"
                );
                meetings += 1;
            }
        }
        assert_eq!(meetings, 2);
    }

    #[test]
    fn the_shuttle_is_hauled_by_a_tank_engine_and_the_line_train_by_a_tender_engine() {
        let (_, line, shuttle) = branched_trains(Timetable::default());
        assert_eq!(shuttle.cars()[0].kind, CarKind::TankEngine);
        assert_eq!(line.cars()[0].kind, CarKind::Locomotive);
        assert!(
            shuttle
                .cars()
                .iter()
                .skip(1)
                .all(|c| c.kind != CarKind::TankEngine),
            "one engine only"
        );
    }

    #[test]
    fn a_shuttle_on_the_branch_stops_at_both_ends_and_runs_both_ways() {
        let (towns, _, shuttle) = branched_trains(Timetable::default());
        let towns_served: Vec<usize> = shuttle.stations().iter().map(|s| s.town).collect();
        assert_eq!(towns_served, [MEDIO, NORTE]);
        assert_eq!(shuttle.cars().len(), branch_consist().len());
        assert!(shuttle.length() < 50.0);
        // Both runs go the full length between the two stations.
        for run in [Run::Down, Run::Up] {
            assert!(shuttle.run_duration(run) > 60.0, "{run:?}");
            let start = shuttle.state_at(run, 0.0);
            let end = shuttle.state_at(run, shuttle.run_duration(run));
            let (from, to) = match run {
                Run::Down => (0, 1),
                Run::Up => (1, 0),
            };
            assert_eq!((start.station, end.station), (Some(from), Some(to)));
            assert_eq!(end.phase, LinePhase::Parked);
            assert!(end.speed.abs() < 1e-3);
        }
        // It runs on the branch: Medio's station and Norte's are its marks.
        let marks: Vec<f32> = shuttle.stations().iter().map(|s| s.mark).collect();
        assert!(marks[0] < marks[1]);
        let station_of = |town: usize| {
            towns[town]
                .buildings
                .iter()
                .find(|b| b.kind == BuildingKind::Station)
                .map(|b| b.footprint.center())
                .unwrap()
        };
        let reached =
            |k: usize, town: usize| shuttle.path().at(marks[k]).distance(station_of(town));
        assert!(reached(0, MEDIO) < 60.0 && reached(1, NORTE) < 60.0);
        // The night is at Norte, facing the morning run out of it.
        let s = shuttle.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(1), Run::Up)
        );
        // Pure: building it again gives the same train.
        let (_, _, again) = branched_trains(Timetable::default());
        assert_eq!(again, shuttle);
        assert_eq!(
            again.state_at(Run::Up, 90.0),
            shuttle.state_at(Run::Up, 90.0)
        );
    }

    #[test]
    fn the_shuttle_stands_in_medios_bay_beside_the_line_trains_track() {
        let (towns, _, shuttle) = branched_trains(Timetable::default());
        let track = towns[MEDIO].railway.unwrap();
        let timetable = shuttle.timetable();
        // In at the end of the morning up run, out on the first down run.
        let (arrive, leave) = (
            timetable.up[0].seconds() + shuttle.run_duration(Run::Up) * BASE_RATE,
            timetable.down[0].seconds(),
        );
        // In the bay for the whole stop: the consist's middle at the
        // bay's stop mark, every car 4.5 m north of the station track.
        for t in [arrive + 1.0, (arrive + leave) / 2.0, leave - 1.0] {
            let at = jumped(&shuttle, t);
            assert_eq!(at.state().station, Some(0));
            for car in at.placed_cars() {
                assert!((car.center.y - track.start.y - 4.5).abs() < 0.05, "{car:?}");
            }
            let middle = (at.placed_cars()[1].center, at.placed_cars()[2].center);
            let stop = shuttle.path().at(shuttle.stations()[0].mark);
            assert!(stop.distance((middle.0 + middle.1) * 0.5) < 20.0);
        }
    }

    #[test]
    fn the_shuttle_meets_the_line_train_at_the_junction_both_ways() {
        let (_, line, shuttle) = branched_trains(Timetable::default());
        assert!(shuttle.schedule_fits());
        assert_meets_line(&line, &shuttle);
        // Morning and afternoon: two departures from each end, the night at
        // the far terminal.
        assert_eq!(shuttle.timetable().up.len(), 2);
        assert_eq!(shuttle.timetable().down.len(), 2);
        assert_eq!(shuttle.timetable().morning_run(), Run::Up);
    }

    #[test]
    fn the_shuttle_still_meets_the_line_train_when_the_line_timetable_shifts() {
        let (_, default_line, default_shuttle) = branched_trains(Timetable::default());
        let shifted = Timetable {
            up: vec![TimeOfDay::hm(8, 40)],
            down: vec![TimeOfDay::hm(17, 15)],
            ..Timetable::default()
        };
        let (_, line, shuttle) = branched_trains(shifted);
        assert_meets_line(&line, &shuttle);
        assert_ne!(shuttle.timetable(), default_shuttle.timetable());
        assert_ne!(line.timetable(), default_line.timetable());
    }

    #[test]
    fn no_shuttle_is_derived_for_a_town_the_line_does_not_stop_at() {
        let (_, line, shuttle) = branched_trains(Timetable::default());
        assert_eq!(shuttle_timetable(&line, NORTE, 100.0), None);
        // At the line's own terminal there is a stop to meet.
        assert!(shuttle_timetable(&line, 0, 100.0).is_some());
        assert!(shuttle.schedule_fits());
    }

    #[test]
    fn arrival_times_cover_every_departure_of_a_train_with_several() {
        let (_, _, shuttle) = branched_trains(Timetable::default());
        let norte = shuttle.station_of_town(NORTE).unwrap();
        let medio = shuttle.station_of_town(MEDIO).unwrap();
        for (run, station, town) in [(Run::Down, norte, NORTE), (Run::Up, medio, MEDIO)] {
            let arrivals = shuttle.arrivals_at(run, station);
            let stops = shuttle.stops_at(town, run);
            let departures = shuttle.timetable().departures(run).len();
            assert!(departures >= 2, "two round trips");
            assert_eq!((arrivals.len(), stops.len()), (departures, departures));
            // In order, and the train stands there at each one.
            assert!(arrivals.windows(2).all(|w| w[0].seconds() < w[1].seconds()));
            for (arrival, stop) in arrivals.iter().zip(&stops) {
                let standing = |at: TimeOfDay| {
                    let s = jumped(&shuttle, at.seconds()).state();
                    (s.station == Some(station)).then_some(s.phase)
                };
                assert!(standing(*stop).is_some(), "{run:?} stands at {stop}");
                // Just after the arrival it is in; the stop is no earlier.
                assert!(stop.seconds() >= arrival.seconds());
                let arrived = TimeOfDay::from_seconds(arrival.seconds() + 1.0);
                assert!(standing(arrived).is_some(), "{run:?} in at {arrival}");
            }
            // The one-time helpers are the earliest of them.
            assert_eq!(shuttle.arrival_time_at(run, station), Some(arrivals[0]));
            assert_eq!(shuttle.stop_at(town, run), Some(stops[0]));
        }
        // A run that does not stop there has no times.
        assert!(shuttle.arrivals_at(Run::Down, 9).is_empty());
        assert!(shuttle.stops_at(99, Run::Down).is_empty());
    }

    #[test]
    fn a_train_with_two_round_trips_is_where_the_timetable_puts_it() {
        let (_, _, shuttle) = branched_trains(Timetable::default());
        let up = shuttle.timetable().up.clone();
        let down = shuttle.timetable().down.clone();
        let (morning, afternoon) = (up[0].seconds(), up[1].seconds());
        assert!(morning < afternoon);
        let (night, running, between) = (morning - 60.0, morning + 30.0, afternoon - 60.0);
        // Before the first departure: laid over at Norte.
        let s = jumped(&shuttle, night).state();
        assert_eq!((s.phase, s.station), (LinePhase::Parked, Some(1)));
        // Just after it: on the up run.
        let s = jumped(&shuttle, running).state();
        assert_eq!((s.phase, s.run), (LinePhase::Running, Run::Up));
        // Between the round trips: back at Norte after the down run.
        let s = jumped(&shuttle, between).state();
        assert_eq!((s.phase, s.station), (LinePhase::Parked, Some(1)));
        assert!(down[0].seconds() > morning && down[0].seconds() < afternoon);
        // Just after the afternoon departure: on its up run, not the
        // morning's.
        let s = jumped(&shuttle, afternoon + 30.0).state();
        assert_eq!((s.phase, s.run), (LinePhase::Running, Run::Up));
        // The next departure from either end follows the lists.
        assert_eq!(
            shuttle.next_departure_after(morning + 1.0),
            (
                down[0].seconds().min(afternoon),
                if down[0].seconds() < afternoon {
                    Run::Down
                } else {
                    Run::Up
                }
            )
        );
        // After the afternoon down run it waits overnight for the morning.
        let late = down[1].seconds() + shuttle.run_duration(Run::Down) * BASE_RATE + 60.0;
        let s = jumped(&shuttle, late).state();
        assert_eq!((s.phase, s.station), (LinePhase::Parked, Some(1)));
        assert_eq!(
            shuttle.next_departure_after(late),
            (morning + DAY_SECONDS, Run::Up)
        );
    }

    #[test]
    fn a_timetable_needs_a_departure_from_each_end_and_none_at_the_same_time() {
        let default = Timetable::default();
        let none = Timetable {
            up: Vec::new(),
            ..default.clone()
        };
        assert_eq!(none.check(), Err(TimetableError::NoDeparture(Run::Up)));
        assert!(
            none.check().unwrap_err().to_string().contains("Up"),
            "names the run"
        );
        let twice = Timetable {
            up: vec![TimeOfDay::hm(7, 0), TimeOfDay::hm(7, 0)],
            ..default.clone()
        };
        assert_eq!(
            twice.check(),
            Err(TimetableError::SameDeparture(TimeOfDay::hm(7, 0)))
        );
        let across = Timetable {
            down: vec![TimeOfDay::hm(9, 0), TimeOfDay::hm(7, 0)],
            ..default.clone()
        };
        assert_eq!(
            across.check(),
            Err(TimetableError::SameDeparture(TimeOfDay::hm(7, 0)))
        );
        // The earliest listed time counts, whatever the order.
        let listed = Timetable {
            down: vec![TimeOfDay::hm(18, 0), TimeOfDay::hm(10, 0)],
            ..default
        };
        assert_eq!(listed.check(), Ok(()));
        assert_eq!(listed.departure(Run::Down), TimeOfDay::hm(10, 0));
        assert_eq!(listed.morning_run(), Run::Up);
    }

    #[test]
    fn a_schedule_that_leaves_before_the_run_is_over_does_not_fit() {
        let (towns, line, shuttle) = branched_trains(Timetable::default());
        let (served, path) = (&[MEDIO, NORTE], shuttle.path().clone());
        // Down leaves a minute before up: no run takes a minute.
        let tight = Timetable {
            down: vec![TimeOfDay::hm(12, 0)],
            up: vec![TimeOfDay::hm(12, 1)],
            ..line.timetable().clone()
        };
        let train = LineTrain::on_path(path.clone(), served, &towns, branch_consist(), tight)
            .expect("a train, if not a sound schedule");
        assert!(!train.schedule_fits());
        // Two up departures in a row, with no down run between.
        let doubled = Timetable {
            down: vec![TimeOfDay::hm(12, 0)],
            up: vec![TimeOfDay::hm(6, 0), TimeOfDay::hm(9, 0)],
            ..line.timetable().clone()
        };
        let train = LineTrain::on_path(path, served, &towns, branch_consist(), doubled).unwrap();
        assert!(!train.schedule_fits());
        // The line's own schedule, as shipped, fits.
        assert!(line.schedule_fits());
    }

    #[test]
    fn on_path_refuses_a_bad_timetable_where_new_replaces_it() {
        let (towns, line, shuttle) = branched_trains(Timetable::default());
        let bad = Timetable {
            up: Vec::new(),
            ..Timetable::default()
        };
        let on = LineTrain::on_path(
            shuttle.path().clone(),
            &[MEDIO, NORTE],
            &towns,
            branch_consist(),
            bad,
        );
        assert!(on.is_none());
        assert_eq!(line.timetable(), &Timetable::default());
    }
}
