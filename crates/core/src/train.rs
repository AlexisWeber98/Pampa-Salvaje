//! El tren de la línea: un tren de pasajeros que recorre toda la línea del
//! ferrocarril (Castex → Monte Nievas → General Pico y vuelta) según un horario
//! que maneja el [`crate::GameClock`]. Su recorrido es una función pura de los
//! segundos reales desde que salió, así que los saltos del reloj lo dejan
//! exactamente donde lo dejaría avanzar frame por frame. Sus vagones son
//! obstáculos en movimiento que sacan a los jinetes de la vía.

use crate::{
    Aabb, BASE_RATE, BuildingKind, DAY_SECONDS, Heading, LinePath, RailLine, SideObstacle,
    TimeOfDay, TownLayout, Vec2,
};

/// Qué es un vagón, para que el render pueda elegir su aspecto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CarKind {
    /// Locomotora a vapor con su ténder.
    Locomotive,
    /// Pequeña locomotora tanque a vapor (una 2-6-2T): tanques laterales y una
    /// carbonera sobre la propia locomotora, sin ténder. Arrastra la lanzadera
    /// del ramal.
    TankEngine,
    Boxcar,
    /// Tolva abierta para cereal.
    Hopper,
    /// Coche de pasajeros.
    Coach,
}

impl CarKind {
    /// Altura del punto más alto del vagón (el techo, o la chimenea de la
    /// locomotora) sobre la parte superior de los rieles, en metros, tal como
    /// lo dibuja el render.
    pub fn height(self) -> f32 {
        match self {
            // La boca acampanada de la chimenea.
            CarKind::Locomotive => 4.55,
            // La chimenea más corta de una locomotora de ramal.
            CarKind::TankEngine => 4.2,
            CarKind::Coach => 4.16,
            CarKind::Boxcar => 4.06,
            // El cereal amontonado en la caja abierta.
            CarKind::Hopper => 3.56,
        }
    }
}

/// Un vagón de la formación, en orden desde el frente.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Car {
    pub kind: CarKind,
    /// Longitud entre paragolpes, en metros.
    pub length: f32,
}

/// Un vagón ubicado en el plano del suelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedCar {
    pub kind: CarKind,
    pub center: Vec2,
    /// Dirección unitaria de avance.
    pub direction: Vec2,
    pub length: f32,
    /// Si el vagón está sobre la vía férrea dibujada (el tren de la línea
    /// siempre lo está).
    pub on_rail: bool,
}

impl PlacedCar {
    /// La huella del vagón como rectángulos alineados con los ejes: un
    /// rectángulo para un vagón alineado con un eje, y una fila de rectángulos
    /// cortos para un vagón en diagonal, para que los rectángulos se ajusten a
    /// sus costados.
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

/// Hacia dónde va un recorrido a lo largo de la línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Run {
    /// Desde el primer pueblo de la línea hasta el último (Castex → Pico).
    Down,
    /// Desde el último pueblo de la línea de vuelta al primero (Pico → Castex).
    Up,
}

/// Lo que está haciendo el tren de la línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinePhase {
    /// Moviéndose entre dos estaciones.
    Running,
    /// Detenido en una estación a mitad de un recorrido.
    Dwelling,
    /// Esperando en una terminal entre recorridos.
    Parked,
}

/// Una estación de la línea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    /// Índice (dentro de los pueblos del mundo) del pueblo de la estación.
    pub town: usize,
    /// Distancia a lo largo del recorrido de la línea hasta la proyección de la
    /// estación sobre él.
    pub mark: f32,
    /// Si el tren se detiene aquí en una vía muerta (ver
    /// [`LineTrain::on_path_stopping`]) en lugar de junto al andén.
    pub bay: bool,
}

/// Horarios de salida y movimiento de un tren.
///
/// Cada terminal tiene sus horarios de salida, uno o más por día. La salida más
/// temprana del día corresponde al recorrido de la mañana
/// ([`Self::morning_run`]); el tren pasa la noche en la terminal de la que
/// sale. Un tren que está en una terminal sale en la próxima salida de esa
/// terminal.
#[derive(Debug, Clone, PartialEq)]
pub struct Timetable {
    /// Cuándo sale el tren de la primera estación del recorrido en un recorrido
    /// de bajada (para la línea, Castex).
    pub down: Vec<TimeOfDay>,
    /// Cuándo sale el tren de la última estación del recorrido en un recorrido
    /// de subida (para la línea, Pico).
    pub up: Vec<TimeOfDay>,
    /// Segundos reales detenido en cada estación a mitad de un recorrido.
    pub dwell: f32,
    /// Velocidad máxima, en m/s.
    pub top_speed: f32,
    /// Aceleración al arrancar y desaceleración al frenar, en m/s².
    pub acceleration: f32,
}

/// Por qué no se puede usar un [`Timetable`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimetableError {
    /// Dos salidas, desde cualquiera de las terminales, son a la misma hora del
    /// día, así que el tren solo podría conservar una de ellas.
    SameDeparture(TimeOfDay),
    /// Una terminal no tiene ninguna salida, así que el tren nunca podría salir
    /// de ella.
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
    /// 65 km/h, la velocidad máxima de la línea.
    pub const TOP_SPEED: f32 = 65.0 / 3.6;

    /// Si un tren puede funcionar con este horario: cada terminal necesita una
    /// salida, y no puede haber dos salidas a la misma hora del día.
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

    /// Cada hora del día en que `run` sale de su terminal, tal como se listan.
    pub fn departures(&self, run: Run) -> &[TimeOfDay] {
        match run {
            Run::Down => &self.down,
            Run::Up => &self.up,
        }
    }

    /// La hora del día más temprana en que `run` sale de su terminal
    /// (medianoche si nunca sale).
    pub fn departure(&self, run: Run) -> TimeOfDay {
        self.departures(run)
            .iter()
            .copied()
            .min_by(|a, b| a.seconds().total_cmp(&b.seconds()))
            .unwrap_or_default()
    }

    /// El recorrido que sale primero cada día (el de bajada en caso de empate).
    /// El tren pasa la noche en la terminal de la que sale.
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
            // Noche en Pico: el recorrido de la mañana sale de Pico, el de la
            // tarde sale de Castex (usuario, 2026-10-21).
            up: vec![TimeOfDay::hm(7, 0)],
            down: vec![TimeOfDay::hm(16, 0)],
            dwell: 25.0,
            top_speed: Self::TOP_SPEED,
            acceleration: 0.5,
        }
    }
}

/// Estado instantáneo del tren de la línea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineTrainState {
    pub phase: LinePhase,
    /// El recorrido en curso, o el recorrido hacia el que mira el tren mientras
    /// está estacionado.
    pub run: Run,
    /// Distancia a lo largo del recorrido de la línea hasta el frente de la
    /// locomotora.
    pub front: f32,
    /// Velocidad a lo largo de la vía, en m/s (nunca negativa).
    pub speed: f32,
    /// Hacia dónde mira la locomotora.
    pub heading: Heading,
    /// La estación en la que está detenido el tren, como índice dentro de
    /// [`LineTrain::stations`].
    pub station: Option<usize>,
}

/// El único tren de pasajeros que recorre toda la línea según un horario.
///
/// Su recorrido de la mañana ([`Timetable::morning_run`]) sale de la terminal
/// donde pasó la noche (por defecto Pico, a la hora [`Timetable::depart_pico`]),
/// para en cada estación del camino, espera en la otra terminal (Castex), sale de
/// ella a su propia hora de salida ([`Timetable::depart_castex`]) y vuelve para
/// pasar la noche. El reloj solo dispara las salidas: una vez en marcha, el tren
/// se mueve físicamente en segundos reales, sea cual sea la velocidad del reloj.
///
/// Todo su recorrido es una función pura de los segundos reales desde que salió
/// ([`Self::state_at`]), así que [`Self::advance`] puede adelantar el reloj de
/// golpe y dejar el tren exactamente donde lo dejaría avanzar frame por frame.
#[derive(Debug, Clone, PartialEq)]
pub struct LineTrain {
    path: LinePath,
    stations: Vec<Station>,
    cars: Vec<Car>,
    timetable: Timetable,
    /// Distancia desde el frente de la locomotora hasta el centro de los
    /// coches.
    stop_offset: f32,
    down: RunPlan,
    up: RunPlan,
    motion: Motion,
}

/// En qué punto de su jornada está el tren.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Motion {
    /// Esperando en la terminal `station` desde la hora de juego `since`;
    /// `turned` una vez que mira hacia su próximo recorrido (a mitad de la
    /// espera).
    Parked {
        station: usize,
        since: f64,
        turned: bool,
    },
    /// En `run`, `elapsed` segundos reales después de salir.
    Running { run: Run, elapsed: f64 },
}

/// Las paradas de un recorrido: dónde queda el frente en cada una, como
/// progreso a lo largo del recorrido (metros desde el extremo inicial del
/// recorrido), y cuándo llega y sale, en segundos reales desde la salida. En el
/// orden del recorrido.
#[derive(Debug, Clone, PartialEq)]
struct RunPlan {
    stops: Vec<f64>,
    times: Vec<(f64, f64)>,
}

/// Las llegadas a esta distancia o menos (en segundos reales) cuentan como en
/// la estación, para que saltar a una llegada calculada nunca termine apenas
/// antes de ella.
const ARRIVAL_EPSILON: f64 = 1e-6;
/// Segundos de juego que se suman a las esperas calculadas, por la misma razón.
const WAIT_MARGIN: f64 = 1e-3;

impl LineTrain {
    /// Ancho de todos los vagones, en metros.
    pub const WIDTH: f32 = 3.0;
    /// Separación entre vagones enganchados, en metros.
    pub const COUPLING: f32 = 1.0;
    /// Altura de la parte superior de los rieles sobre el suelo: donde se
    /// apoyan los vagones.
    pub const RAIL_TOP: f32 = 0.5;

    /// El tren de `line` a través de `towns`, estacionado para pasar la noche a
    /// la medianoche del día 0 en la terminal de la que sale su recorrido de la
    /// mañana (Pico por defecto), mirando hacia ese recorrido. Un `timetable`
    /// que no pasa [`Timetable::check`] se reemplaza por el horario por
    /// defecto. `None` cuando la línea sirve a menos de dos estaciones o es
    /// demasiado corta para el tren.
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

    /// Un tren de `cars` sobre cualquier `path`, que sirve a `served` (índices
    /// dentro de `towns`, en el orden en que el recorrido llega a ellos) según
    /// `timetable`, estacionado para pasar la noche en la terminal de la que
    /// sale su recorrido de la mañana, mirando hacia ese recorrido. Un pueblo
    /// sin ferrocarril no tiene estación. `None` cuando quedan menos de dos
    /// estaciones, el recorrido es demasiado corto para la formación o el
    /// `timetable` no pasa [`Timetable::check`].
    pub fn on_path(
        path: LinePath,
        served: &[usize],
        towns: &[TownLayout],
        cars: Vec<Car>,
        timetable: Timetable,
    ) -> Option<Self> {
        Self::on_path_stopping(path, served, &[], towns, cars, timetable)
    }

    /// [`Self::on_path`], con `stops` indicando dónde se detiene el tren en un
    /// pueblo en lugar de junto al edificio de su estación: pares de un índice
    /// de pueblo y un punto del suelo, cuyo punto más cercano sobre el
    /// recorrido es la parada (el centro de la formación en una terminal). Así
    /// es como una vía muerta da la parada de la lanzadera en el empalme.
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
        // Pasa la noche en la terminal de la que sale el recorrido de la
        // mañana.
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

    /// La lanzadera sobre el `path` del ramal (tal como lo da
    /// [`RailLine::shuttle_paths`]), que sirve a `served` (primero el pueblo de
    /// empalme y después los del ramal) y se detiene en `stops` (ver
    /// [`Self::on_path_stopping`]; el [`crate::RailBay::stop`] de la vía muerta
    /// del empalme), y que se encuentra con `line` en el empalme: su horario
    /// sale de [`shuttle_timetable`]. `None` cuando el ramal no puede contener
    /// la lanzadera, `line` no para en el empalme o el horario derivado no
    /// encaja ([`Self::schedule_fits`]).
    pub fn branch_shuttle(
        line: &LineTrain,
        path: LinePath,
        served: &[usize],
        stops: &[(usize, Vec2)],
        towns: &[TownLayout],
    ) -> Option<Self> {
        let junction = *served.first()?;
        // Los tiempos de recorrido no dependen de las salidas, así que un
        // horario provisorio con el mismo movimiento da el recorrido desde la
        // terminal lejana hasta el empalme.
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

    /// Todos los trenes de `line` a través de `towns`: primero el tren de la
    /// línea según `timetable` (ninguno cuando la línea no puede contener uno)
    /// y después la lanzadera de cada ramal que tiene vía muerta, que se
    /// encuentra con el tren de la línea en el empalme
    /// ([`Self::branch_shuttle`]). Un ramal sin vía muerta, o cuya lanzadera no
    /// encaja, no tiene lanzadera.
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

    /// Si cada recorrido termina antes de la próxima salida, salga de donde
    /// salga: las salidas, tomadas a lo largo de un día, se alternan entre las
    /// terminales y cada una sale después de que llegó el recorrido anterior.
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

    /// La línea que recorre el tren, de Castex a Pico.
    pub fn path(&self) -> &LinePath {
        &self.path
    }

    /// Las estaciones, en el orden de la línea.
    pub fn stations(&self) -> &[Station] {
        &self.stations
    }

    /// La formación, primero el frente.
    pub fn cars(&self) -> &[Car] {
        &self.cars
    }

    pub fn timetable(&self) -> &Timetable {
        &self.timetable
    }

    /// Longitud desde el frente de la locomotora hasta el final del último
    /// vagón, en metros.
    pub fn length(&self) -> f32 {
        consist_length(&self.cars)
    }

    /// Distancia desde el frente de la locomotora hasta el centro de los
    /// coches, que se detienen junto a cada estación.
    pub fn stop_offset(&self) -> f32 {
        self.stop_offset
    }

    /// La estación (índice dentro de [`Self::stations`]) del pueblo `town`.
    pub fn station_of_town(&self, town: usize) -> Option<usize> {
        self.stations.iter().position(|s| s.town == town)
    }

    /// Segundos reales desde que sale de una terminal hasta que llega a la
    /// otra.
    pub fn run_duration(&self, run: Run) -> f64 {
        self.plan_of(run).times.last().map_or(0.0, |t| t.1)
    }

    /// Segundos reales desde la salida de `run` hasta su llegada a `station` y
    /// su salida de ella.
    pub fn stop_times(&self, run: Run, station: usize) -> Option<(f64, f64)> {
        let k = self.stop_index(run, station)?;
        self.plan_of(run).times.get(k).copied()
    }

    /// El tren `run_seconds` segundos reales después de salir en `run`
    /// (limitado al recorrido). Pura: la misma entrada siempre da el mismo
    /// estado.
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
                // En cualquiera de los dos extremos del recorrido el tren está
                // detenido en una terminal.
                let phase = if k == 0 || k + 1 == n {
                    LinePhase::Parked
                } else {
                    LinePhase::Dwelling
                };
                (phase, self.station_at(run, k))
            }),
        )
    }

    /// El tren ahora.
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

    /// Sigue al reloj desde la hora de juego `from` (segundos desde la
    /// medianoche del día 0, ver [`crate::GameClock::elapsed`]) durante
    /// `game_seconds`, a `rate` segundos de juego por segundo real: la
    /// velocidad del reloj mientras corre, [`crate::BASE_RATE`] para un salto.
    /// Las salidas ocurren cuando el reloj llega a ellas; el avance es en
    /// segundos reales. Una entrada incorrecta no cambia nada.
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

    /// La primera salida programada desde cualquiera de las terminales a la
    /// hora de juego `at` o después, y el recorrido que inicia.
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

    /// Cuándo llega a `station` la salida más temprana de `run` según el
    /// horario, con el reloj a la velocidad base todo el tiempo. Ver
    /// [`Self::arrivals_at`] para un tren con varias salidas.
    pub fn arrival_time_at(&self, run: Run, station: usize) -> Option<TimeOfDay> {
        self.arrivals_at(run, station).into_iter().next()
    }

    /// Cuándo llega a `station` cada salida de `run` (primero la más temprana)
    /// según el horario, con el reloj a la velocidad base todo el tiempo. Vacío
    /// cuando el recorrido no para ahí.
    pub fn arrivals_at(&self, run: Run, station: usize) -> Vec<TimeOfDay> {
        let Some((arrive, _)) = self.stop_times(run, station) else {
            return Vec::new();
        };
        self.departures_sorted(run)
            .into_iter()
            .map(|depart| TimeOfDay::from_seconds(depart + arrive * BASE_RATE))
            .collect()
    }

    /// La hora del día a mitad de la parada en `town` de la salida más temprana
    /// de `run` (por defecto el recorrido de subida de la mañana desde Pico, o
    /// el de bajada de la tarde desde Castex), con el reloj a la velocidad
    /// base: si se pone el reloj en esa hora, el tren está detenido en la
    /// estación de ese pueblo. Ver [`Self::stops_at`] para un tren con varias
    /// salidas.
    pub fn stop_at(&self, town: usize, run: Run) -> Option<TimeOfDay> {
        self.stops_at(town, run).into_iter().next()
    }

    /// [`Self::stop_at`] para cada salida de `run`, primero la más temprana.
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

    /// Los segundos del día de cada salida de `run`, primero la más temprana.
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

    /// Si el tren está detenido en una vía muerta en la estación en la que
    /// está.
    pub fn stands_in_bay(&self) -> bool {
        self.state()
            .station
            .and_then(|s| self.stations.get(s))
            .is_some_and(|s| s.bay)
    }

    /// Segundos de juego, a la velocidad base, desde la hora de juego `now`
    /// hasta que el tren vuelva a estar detenido en `station`: `0` si ya lo
    /// está.
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

    /// Mientras está en marcha, la próxima estación en la que va a parar el
    /// tren y los segundos reales que faltan: el resto de un viaje.
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

    /// Pone al tren `elapsed` segundos reales dentro de `run` (limitado al
    /// recorrido), como si hubiera salido de su terminal a horario y hubiera
    /// avanzado desde entonces: así empieza el juego con el tren ya en marcha.
    /// Una entrada incorrecta lo pone al inicio del recorrido.
    pub fn place_on_run(&mut self, run: Run, elapsed: f64) {
        let elapsed = if elapsed.is_finite() {
            elapsed.clamp(0.0, self.run_duration(run))
        } else {
            0.0
        };
        self.motion = Motion::Running { run, elapsed };
    }

    /// La próxima estación a la que llega el tren (nunca una en la que ya está
    /// detenido) y los segundos de juego que faltan, con el reloj corriendo
    /// desde la hora de juego `now` a `rate` segundos de juego por segundo
    /// real: el resto de un viaje, a bordo. Desde una terminal, primero espera
    /// su salida. `None` con una entrada incorrecta.
    pub fn next_arrival(&self, now: f64, rate: f64) -> Option<(usize, f64)> {
        self.arrival_ahead(now, rate)
            .map(|(station, game, _, _)| (station, game))
    }

    /// Salta el tren hasta su próxima llegada ([`Self::next_arrival`]) y
    /// devuelve esa estación y los segundos de juego que debe saltar el reloj.
    /// El tren queda exactamente donde lo dejaría seguir el reloj a `rate`,
    /// como el [`Self::state_at`] puro de su recorrido en esa llegada.
    pub fn skip_to_next_stop(&mut self, now: f64, rate: f64) -> Option<(usize, f64)> {
        self.skip_toward_next_stop(now, rate, 0.0)
    }

    /// Como [`Self::skip_to_next_stop`], pero termina `lead` segundos reales
    /// antes de la llegada (nunca detrás de donde ya está el tren), para que
    /// igual entre y frene hasta detenerse. Devuelve la estación y los segundos
    /// de juego que debe saltar el reloj.
    pub fn skip_toward_next_stop(
        &mut self,
        now: f64,
        rate: f64,
        lead: f64,
    ) -> Option<(usize, f64)> {
        let (station, game, run, arrive) = self.arrival_ahead(now, rate)?;
        let lead = if lead.is_finite() { lead.max(0.0) } else { 0.0 };
        // Dónde está ahora el tren en ese recorrido: un tren que espera en la
        // terminal todavía no lo empezó.
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

    /// Salta un tren detenido en una estación (en una parada, o esperando en
    /// una terminal) hasta el momento en que sale, y devuelve los segundos de
    /// juego que el reloj debe saltar con él a `rate`: el resto de la parada, o
    /// la espera hasta la salida desde una terminal. Un tren que ya está en
    /// marcha entre estaciones queda como está (cero). `None` con una entrada
    /// incorrecta.
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
            // Recién llegado a su terminal: espera desde ahora.
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

    /// La próxima llegada desde la hora de juego `now` con el reloj a `rate`:
    /// la estación, los segundos de juego que faltan, y el recorrido y los
    /// segundos reales dentro de él en los que ocurre.
    fn arrival_ahead(&self, now: f64, rate: f64) -> Option<(usize, f64, Run, f64)> {
        if !(now.is_finite() && rate.is_finite() && rate > 0.0) {
            return None;
        }
        // El recorrido que trae la próxima llegada, qué tan avanzado está el
        // tren en él y los segundos de juego antes de que salga (si está
        // esperando en la terminal).
        let (run, elapsed, wait) = match self.motion {
            Motion::Running { run, elapsed }
                if elapsed < self.run_duration(run) - ARRIVAL_EPSILON =>
            {
                (run, elapsed, 0.0)
            }
            // Recién llegado a su terminal: espera desde ahora.
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

    /// Todos los vagones en el plano del suelo, primero el frente. Los extremos
    /// de cada vagón quedan sobre la línea, cada uno a su propia distancia a lo
    /// largo de ella, así que la formación sigue las curvas.
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

    /// Huellas de los vagones, como obstáculos que sacan a los jinetes de la
    /// vía. Un vagón alineado con un eje es un rectángulo; un vagón en diagonal
    /// es una fila de rectángulos cortos a lo largo de él, para que los
    /// rectángulos se ajusten a sus costados.
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

    /// Estaciones (índices dentro de [`Self::stations`]) donde una parada deja
    /// los coches a más de [`MAX_STOP_MISS`] de la estación: demasiado cerca de
    /// un extremo de la línea para que la formación se detenga ahí. Vacío en
    /// una línea correcta; el juego avisa si hay alguna.
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

    /// Tiempos de parada de un recorrido a través de `stops` (progreso del
    /// frente, en el orden del recorrido).
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

    /// Aceleración y velocidad máxima, siempre positivas.
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

    /// Índice de `station` en el orden de paradas de `run`.
    fn stop_index(&self, run: Run, station: usize) -> Option<usize> {
        let n = self.stations.len();
        (station < n).then(|| match run {
            Run::Down => station,
            Run::Up => n - 1 - station,
        })
    }

    /// La estación de la parada número `k` de `run`.
    fn station_at(&self, run: Run, k: usize) -> usize {
        match run {
            Run::Down => k,
            Run::Up => self.stations.len() - 1 - k,
        }
    }

    /// El recorrido que sale de la terminal `station`.
    fn departing(&self, station: usize) -> Run {
        if station == 0 { Run::Down } else { Run::Up }
    }

    /// La terminal en la que termina `run`.
    fn terminal(&self, run: Run) -> usize {
        match run {
            Run::Down => self.stations.len() - 1,
            Run::Up => 0,
        }
    }

    /// La primera salida programada desde la terminal `station` a la hora de
    /// juego `since` o después.
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

    /// El estado con el frente a `progress` metros dentro de `run`.
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

    /// Centro y dirección unitaria de avance de un vagón de largo `length` cuyo
    /// extremo delantero está `behind` metros detrás del frente de la
    /// locomotora ubicado en `front`, avanzando hacia `sign` (+1 en bajada por
    /// la línea, -1 en subida).
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
    /// El recorrido en el sentido contrario.
    pub fn back(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
        }
    }
}

/// +1 para un recorrido de bajada por la línea (distancia creciente a lo largo
/// del recorrido), -1 para uno de subida.
fn run_sign(run: Run) -> f32 {
    match run {
        Run::Down => 1.0,
        Run::Up => -1.0,
    }
}

/// La caja de un coche de pasajeros, tal como la comparten su dibujo y la vista
/// desde el asiento, en el sistema de referencia propio del vagón: alturas
/// sobre la parte superior de los rieles, distancias a lo ancho desde su línea
/// central y a lo largo desde su centro, en metros.
pub struct CoachBody;

impl CoachBody {
    /// Parte superior del piso.
    pub const FLOOR: f32 = 1.3;
    /// Parte inferior del techo.
    pub const CEILING: f32 = 3.8;
    /// Cara exterior de cada pared lateral desde la línea central.
    pub const HALF_WIDTH: f32 = 1.4;
    /// Espesor de las paredes.
    pub const WALL: f32 = 0.08;
    /// Parte inferior de las aberturas de las ventanillas: 0,8 m sobre el piso,
    /// lo bastante bajo para que un pasajero sentado vea el suelo al lado de la
    /// vía.
    pub const SILL: f32 = 2.1;
    /// Parte superior de las aberturas de las ventanillas: 1,8 m sobre el piso,
    /// por encima del horizonte visto desde un asiento.
    pub const LINTEL: f32 = 3.1;
    /// A qué distancia hacia adentro de cada extremo del vagón están sus
    /// paredes de los extremos.
    pub const END_INSET: f32 = 0.35;
    /// Altura de los ojos de un pasajero sentado sobre el piso.
    pub const EYE: f32 = 1.2;

    /// Las aberturas de las ventanillas a lo largo de cada lado de un coche de
    /// largo `length`: sus centros respecto del centro del vagón, y su ancho.
    pub fn windows(length: f32) -> (Vec<f32>, f32) {
        let count = ((length - 3.0) / 1.6).floor().max(1.0) as usize;
        let pitch = (length - 3.0) / count as f32;
        let centres = (0..count)
            .map(|i| -length / 2.0 + 1.5 + pitch * (i as f32 + 0.5))
            .collect();
        (centres, pitch * 0.6)
    }

    /// Dónde se sienta un pasajero junto a una ventanilla en un coche de largo
    /// `length`, como `(along, across, height)`: en la ventanilla del medio, a
    /// la derecha del vagón (`side` +1) o a la izquierda (-1), medio metro
    /// hacia adentro desde la pared, con los ojos a la altura de una persona
    /// sentada, mirando hacia adelante a lo largo del vagón con esa ventanilla
    /// al lado.
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

/// Lo más lejos que puede quedar el centro de los coches de una estación en una
/// parada, en metros, con los coches todavía (casi) junto a su andén de 60 m:
/// en una terminal quedan a unos 19 m hacia un lado.
pub const MAX_STOP_MISS: f32 = 20.0;

/// Dónde se detiene el frente en cada estación (`marks`, en el orden de la
/// línea, sobre un recorrido de `total` metros), como progreso a lo largo de
/// cada recorrido, para una formación de `length` metros cuyo centro de los
/// coches está `offset` detrás de su frente: primero las paradas del recorrido
/// de bajada en el orden de la línea y después las del de subida en su propio
/// orden.
///
/// Los coches se detienen junto a cada estación del camino. En las dos
/// terminales, en cambio, toda la formación queda centrada en la estación (con
/// sus coches un poco hacia un lado), así que llega y sale por el mismo tramo
/// de vía y da la vuelta en el lugar durante la espera. Cada parada mantiene la
/// formación sobre el recorrido, y un recorrido nunca se detiene detrás de su
/// parada anterior, así que una estación cerca de una punta solo hace que el
/// tren se detenga antes.
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

/// Estaciones cuyo centro de los coches queda a más de [`MAX_STOP_MISS`] de la
/// estación en la parada de cualquiera de los dos recorridos (ver
/// [`stop_progress`]).
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

/// Longitud de `cars` enganchados, en metros.
pub(crate) fn consist_length(cars: &[Car]) -> f32 {
    let total: f32 = cars.iter().map(|c| c.length).sum();
    total + LineTrain::COUPLING * cars.len().saturating_sub(1) as f32
}

/// La formación del tren de la línea, desde el frente: un tren mixto rural, dos
/// coches de pasajeros detrás de la locomotora y después carga.
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

/// Segundos de juego que la lanzadera está en el empalme antes de que llegue
/// `line`, y que se queda después de que sale. Lo bastante para que todo el
/// tren de la línea (93 m, arrancando a 0,5 m/s²) libere el desvío de la vía
/// muerta antes de que la lanzadera lo siga al tramo final compartido: con 5
/// minutos, los dos trenes quedaban ahí a menos del ancho de un vagón entre sí.
pub const SHUTTLE_MARGIN: f64 = 10.0 * 60.0;

/// El horario de la lanzadera, derivado de las paradas de `line` en `junction`
/// (un índice de pueblo): por cada vez que `line` para ahí, en cualquiera de
/// los dos recorridos, la lanzadera sale de su terminal lejana (la última
/// estación del ramal, donde pasa la noche) de modo que esté en el empalme
/// [`SHUTTLE_MARGIN`] antes de que llegue `line`, y sale del empalme
/// [`SHUTTLE_MARGIN`] después de que sale `line`. `to_junction` son los
/// segundos reales que tarda el recorrido de subida de la lanzadera; se mueve
/// como `line` (parada, velocidad máxima, aceleración). Los tiempos siguen la
/// velocidad base del reloj. `None` cuando `line` no para en el empalme.
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

/// La formación de la lanzadera del ramal, desde el frente: una locomotora
/// tanque, un coche de pasajeros y un furgón.
pub(crate) fn branch_consist() -> Vec<Car> {
    [
        // 12 m entre paragolpes: así el coche queda a un metro del centro de la
        // terminal, y una puerta queda al alcance para subir desde el lugar de
        // parada.
        (CarKind::TankEngine, 12.0),
        (CarKind::Coach, 12.0),
        (CarKind::Boxcar, 10.0),
    ]
    .into_iter()
    .map(|(kind, length)| Car { kind, length })
    .collect()
}

/// Distancias desde el frente de la formación hasta el frente de su primer
/// coche y hasta la parte trasera del último. `None` si no hay coches.
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

/// Distancia desde el frente de la formación hasta el centro de sus coches.
fn coaches_middle(cars: &[Car]) -> Option<f32> {
    let (first, last) = coaches_span(cars)?;
    Some((first + last) / 2.0)
}

/// Segundos reales para recorrer `d` metros de una parada a otra, acelerando y
/// frenando a `a` con una velocidad máxima de `v`.
fn leg_duration(d: f64, v: f64, a: f64) -> f64 {
    if d <= 0.0 {
        0.0
    } else if d >= v * v / a {
        d / v + v / a
    } else {
        2.0 * (d / a).sqrt()
    }
}

/// Distancia recorrida y velocidad a los `tau` segundos reales de un tramo de
/// parada a parada de `d` metros (ver [`leg_duration`]). Termina exactamente en
/// `d`, detenido.
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

    /// Castex, Monte Nievas y Pico tal como se distribuyen, y su tren de la
    /// línea.
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

    /// `(phase, station, start, end)` de cada tramo de un recorrido,
    /// muestreado.
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
        // Una formación rural: dos coches de pasajeros detrás de la locomotora
        // y después carga, ocho vagones en total.
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
        // El centro de los coches: pasando la locomotora (14 m) y su enganche,
        // a mitad de dos coches de 12 m con un enganche entre ellos.
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
        // Los dos extremos de cada recorrido quedan en las terminales.
        assert_eq!(train.state_at(Run::Down, 0.0).station, Some(CASTEX));
        let end = train.run_duration(Run::Down);
        assert_eq!(train.state_at(Run::Down, end).station, Some(PICO));
        assert_eq!(train.state_at(Run::Up, 0.0).station, Some(PICO));
        let end = train.run_duration(Run::Up);
        assert_eq!(train.state_at(Run::Up, end).station, Some(CASTEX));
        // Unos 9,5 minutos reales de punta a punta a escala 1:7.
        assert!((500.0..650.0).contains(&end), "{end}");
    }

    #[test]
    fn the_timetable_runs_pico_castex_and_back_each_day() {
        let (_, mut train) = shipped();
        // Día 0, medianoche: pasa la noche en Pico, mirando hacia Castex.
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(PICO), Run::Up)
        );
        // El recorrido de la mañana sale de Pico a las 07:00 y el de la tarde
        // sale de Castex a las 16:00.
        assert_eq!(train.next_departure_after(0.0), (hm(7, 0), Run::Up));
        assert_eq!(train.next_departure_after(hm(8, 0)), (hm(16, 0), Run::Down));
        assert_eq!(
            train.next_departure_after(hm(17, 0)),
            (DAY_SECONDS + hm(7, 0), Run::Up)
        );
        // Muestrear todo el día a la velocidad base, en minutos de reloj.
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
        // El día siguiente funciona igual.
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
        // Con los horarios intercambiados, la noche pasa a ser en Castex.
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

    /// El tramo del recorrido que cubre toda la formación, como distancias
    /// `(low, high)` a lo largo de él.
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
        // Durante todo un día: cada espera en la terminal conserva una sola
        // huella, centrada en la estación de su terminal.
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
        // Una terminal a 10 m del inicio del recorrido, una estación 15 m más
        // adelante, después una bien adentro y una terminal a 5 m de la punta
        // lejana.
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
        // Tal como se distribuye, todas las paradas están bien.
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
                // Estaciones intermedias: los coches al lado. Terminales: toda
                // la formación centrada, para dar la vuelta en el lugar.
                let ahead = if terminal {
                    train.length() / 2.0
                } else {
                    train.stop_offset()
                };
                let middle = s.front - sign * ahead;
                assert!((middle - station.mark).abs() < 1e-3, "{run:?} {index}");
                let coaches = s.front - sign * train.stop_offset();
                assert!((coaches - station.mark).abs() <= MAX_STOP_MISS);
                // La marca es la proyección de la estación sobre su propia vía.
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
        // Desde las 06:50, a lo largo del recorrido de la mañana y hasta la
        // espera en la terminal.
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
        // 100 segundos reales a 30× y a 12×.
        fast.advance(hm(7, 0), 100.0 * BASE_RATE, BASE_RATE);
        slow.advance(hm(7, 0), 100.0 * 12.0, 12.0);
        assert!((fast.state().front - slow.state().front).abs() < 1e-3);
    }

    /// `train` seguido frame por frame (1/60 s) desde la hora de juego `now` a
    /// `rate` durante `game` segundos de juego.
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
        // Subiendo desde Pico, en Monte Nievas, estacionado en Castex, y
        // estacionado de noche en Pico antes del recorrido de la mañana.
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
            // El estado puro del recorrido en la llegada.
            let (arrive, _) = skipped.stop_times(b.run, station).unwrap();
            assert_eq!(b, skipped.state_at(b.run, arrive));
        }
        // Desde la espera nocturna cuenta la espera hasta la salida de las
        // 07:00: tiempo de juego hasta la salida y después el viaje a 12×.
        let mut night = start.clone();
        night.advance(0.0, hm(5, 0), BASE_RATE);
        let (_, game) = night.next_arrival(hm(5, 0), onboard).unwrap();
        let (arrive, _) = night.stop_times(Run::Up, NIEVAS).unwrap();
        assert!(
            (game - (hm(2, 0) + arrive * onboard)).abs() < 1e-6,
            "{game}"
        );
        assert_eq!(night.next_arrival(hm(5, 0), 0.0), None);
    }

    #[test]
    fn pulling_out_now_and_coming_in_three_seconds_out_land_where_riding_would() {
        let (_, start) = shipped();
        let onboard = 12.0;
        // En la parada de Monte Nievas en el recorrido de la mañana, a mitad de
        // ella.
        let (arrive, depart) = start.stop_times(Run::Up, NIEVAS).unwrap();
        let at = hm(7, 0) + (arrive + depart) / 2.0 * BASE_RATE;
        let mut train = start.clone();
        train.advance(0.0, at, BASE_RATE);
        let mut left = train.clone();
        let game = left.skip_to_departure(at, onboard).unwrap();
        let ridden = ticked(&train, at, game, onboard);
        assert_eq!(left.state(), left.state_at(Run::Up, depart));
        assert!((ridden.state().front - left.state().front).abs() < 1e-3);
        // En la salida: un momento después ya está en marcha.
        let mut moving = left.clone();
        moving.advance(at + game, 0.05 * onboard, onboard);
        assert_eq!(moving.state().phase, LinePhase::Running);
        assert!(moving.state().speed > 0.0);
        // El resto de la parada, a 12×.
        let elapsed = (at - hm(7, 0)) / BASE_RATE;
        assert!(elapsed > arrive && elapsed < depart);
        assert!((game - (depart - elapsed) * onboard).abs() < 1e-6);
        // Ya está en marcha: no hay nada que saltear.
        assert_eq!(
            moving.clone().skip_to_departure(at + game, onboard),
            Some(0.0)
        );

        // Después, hasta 3 s antes de Castex: entrando, y viajar los últimos
        // 3 s lo frena hasta detenerse ahí.
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
        // Nunca hacia atrás: a menos de la anticipación, se queda donde está.
        let mut close = rolled.clone();
        close.place_on_run(Run::Down, 1.0);
        let before = close.state();
        let (_, none) = close.skip_toward_next_stop(now, onboard, 1e6).unwrap();
        assert_eq!((none, close.state()), (0.0, before));

        // Esperando en Castex: la espera hasta la salida de las 16:00.
        let mut parked = start.clone();
        parked.advance(0.0, hm(13, 0), BASE_RATE);
        let wait = parked.skip_to_departure(hm(13, 0), onboard).unwrap();
        assert!((wait - (hm(16, 0) - hm(13, 0))).abs() < 1e-6);
        assert_eq!(parked.state(), parked.state_at(Run::Down, 0.0));
    }

    #[test]
    fn a_seated_passenger_looks_out_through_the_middle_of_the_window() {
        let floor = CoachBody::FLOOR;
        // Un antepecho lo bastante bajo para ver el suelo junto a la vía, una
        // parte superior lo bastante alta para ver el cielo, y los ojos en el
        // medio.
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
        // Del lado pedido.
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
    }

    #[test]
    fn helpers_tell_when_the_train_reaches_a_station() {
        let (_, mut train) = shipped();
        // La parada de la mañana es a mitad de la parada del recorrido de
        // subida, contando desde la salida de las 07:00 desde Pico.
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

        // La parada de la tarde cuenta desde la salida de las 16:00 desde
        // Castex.
        let afternoon = train.stop_at(0, Run::Down).unwrap();
        let (down_arrive, down_depart) = train.stop_times(Run::Down, NIEVAS).unwrap();
        let expected = hm(16, 0) + (down_arrive + down_depart) / 2.0 * BASE_RATE;
        assert!((afternoon.seconds() - expected).abs() < 1e-6);

        // Desde la medianoche, esperar en Monte Nievas lleva a la parada de la
        // mañana.
        let wait = train.time_until_at(NIEVAS, 0.0).unwrap();
        train.advance(0.0, wait, BASE_RATE);
        let s = train.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Dwelling, Some(NIEVAS), Run::Up)
        );
        assert_eq!(train.time_until_at(NIEVAS, wait), Some(0.0));
        // Seguir viaje: la próxima parada es Castex, a lo que queda del
        // recorrido.
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, CASTEX);
        let total = train.run_duration(Run::Up);
        assert!((left - (total - arrive)).abs() < 0.1, "{left}");
        // Esperar en Pico ahora significa la llegada de la noche.
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
        // Buscar un momento con el tren en la curva de un enlace.
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
                // Los dos extremos de cada vagón quedan sobre la línea.
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
        // En la parada de Monte Nievas, sobre su vía recta este-oeste.
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
        // Los obstáculos son exactamente estos rectángulos.
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
        // En el enlace diagonal largo hacia Pico, a mitad del recorrido.
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
                // Empujado hacia un costado, lejos de la línea central del
                // vagón.
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

    /// Oeste, Medio, Este y Norte, y su línea con el ramal a Norte que sale de
    /// Medio.
    pub(crate) fn branched_line() -> (Vec<TownLayout>, RailLine) {
        let (towns, list, branches) = crate::rail::tests::branched();
        let bounds = crate::WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line)
    }

    /// El tren de la línea por Oeste, Medio y Este con un ramal a Norte que
    /// sale de Medio, y la lanzadera de ese ramal.
    pub(crate) fn branched_trains(
        line_timetable: Timetable,
    ) -> (Vec<TownLayout>, LineTrain, LineTrain) {
        let (towns, line) = branched_line();
        let train = LineTrain::new(&line, &towns, line_timetable).expect("a line train");
        let path = line.shuttle_paths(&towns).remove(0);
        // Se detiene en la vía muerta de Medio, no en la del tren de la línea.
        let bay = line.bay_of(0).expect("a bay at Medio");
        let stops = [(MEDIO, bay.stop)];
        let shuttle = LineTrain::branch_shuttle(&train, path, &[MEDIO, NORTE], &stops, &towns)
            .expect("a shuttle on the branch");
        (towns, train, shuttle)
    }

    /// `train` saltado desde la medianoche del día 0 hasta la hora de juego
    /// `at`.
    fn jumped(train: &LineTrain, at: f64) -> LineTrain {
        let mut train = train.clone();
        train.advance(0.0, at, BASE_RATE);
        train
    }

    /// Comprueba que `shuttle` está detenido en la estación de Medio desde
    /// `SHUTTLE_MARGIN` antes de cada parada de `line` ahí hasta
    /// `SHUTTLE_MARGIN` después, y que no está ahí justo después.
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
        // Los dos recorridos van de punta a punta entre las dos estaciones.
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
        // Circula por el ramal: la estación de Medio y la de Norte son sus
        // marcas.
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
        // La noche es en Norte, mirando hacia el recorrido de la mañana que
        // sale de ahí.
        let s = shuttle.state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Parked, Some(1), Run::Up)
        );
        // Pura: construirla de nuevo da el mismo tren.
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
        // Llega al final del recorrido de subida de la mañana y sale en el
        // primer recorrido de bajada.
        let (arrive, leave) = (
            timetable.up[0].seconds() + shuttle.run_duration(Run::Up) * BASE_RATE,
            timetable.down[0].seconds(),
        );
        // En la vía muerta durante toda la parada: el centro de la formación en
        // la marca de parada de la vía muerta, cada vagón 4,5 m al norte de la
        // vía de la estación.
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
        // Mañana y tarde: dos salidas desde cada extremo, la noche en la
        // terminal lejana.
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
        // En la terminal propia de la línea hay una parada con la que
        // coincidir.
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
            // En orden, y el tren está detenido ahí en cada una.
            assert!(arrivals.windows(2).all(|w| w[0].seconds() < w[1].seconds()));
            for (arrival, stop) in arrivals.iter().zip(&stops) {
                let standing = |at: TimeOfDay| {
                    let s = jumped(&shuttle, at.seconds()).state();
                    (s.station == Some(station)).then_some(s.phase)
                };
                assert!(standing(*stop).is_some(), "{run:?} stands at {stop}");
                // Justo después de la llegada ya está ahí; la parada no es
                // antes.
                assert!(stop.seconds() >= arrival.seconds());
                let arrived = TimeOfDay::from_seconds(arrival.seconds() + 1.0);
                assert!(standing(arrived).is_some(), "{run:?} in at {arrival}");
            }
            // Las funciones auxiliares de una sola vez son la más temprana de
            // ellas.
            assert_eq!(shuttle.arrival_time_at(run, station), Some(arrivals[0]));
            assert_eq!(shuttle.stop_at(town, run), Some(stops[0]));
        }
        // Un recorrido que no para ahí no tiene horarios.
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
        // Antes de la primera salida: esperando en Norte.
        let s = jumped(&shuttle, night).state();
        assert_eq!((s.phase, s.station), (LinePhase::Parked, Some(1)));
        // Justo después: en el recorrido de subida.
        let s = jumped(&shuttle, running).state();
        assert_eq!((s.phase, s.run), (LinePhase::Running, Run::Up));
        // Entre los viajes de ida y vuelta: de vuelta en Norte después del
        // recorrido de bajada.
        let s = jumped(&shuttle, between).state();
        assert_eq!((s.phase, s.station), (LinePhase::Parked, Some(1)));
        assert!(down[0].seconds() > morning && down[0].seconds() < afternoon);
        // Justo después de la salida de la tarde: en su recorrido de subida, no
        // en el de la mañana.
        let s = jumped(&shuttle, afternoon + 30.0).state();
        assert_eq!((s.phase, s.run), (LinePhase::Running, Run::Up));
        // La próxima salida desde cualquiera de los extremos sigue las listas.
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
        // Después del recorrido de bajada de la tarde espera toda la noche
        // hasta la mañana.
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
        // Cuenta la hora más temprana de la lista, sea cual sea el orden.
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
        // La de bajada sale un minuto antes que la de subida: ningún recorrido
        // dura un minuto.
        let tight = Timetable {
            down: vec![TimeOfDay::hm(12, 0)],
            up: vec![TimeOfDay::hm(12, 1)],
            ..line.timetable().clone()
        };
        let train = LineTrain::on_path(path.clone(), served, &towns, branch_consist(), tight)
            .expect("a train, if not a sound schedule");
        assert!(!train.schedule_fits());
        // Dos salidas de subida seguidas, sin un recorrido de bajada en el
        // medio.
        let doubled = Timetable {
            down: vec![TimeOfDay::hm(12, 0)],
            up: vec![TimeOfDay::hm(6, 0), TimeOfDay::hm(9, 0)],
            ..line.timetable().clone()
        };
        let train = LineTrain::on_path(path, served, &towns, branch_consist(), doubled).unwrap();
        assert!(!train.schedule_fits());
        // El horario propio de la línea, tal como se distribuye, encaja.
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
