//! Viajar en el tren de la línea: subir en una estación, ir sentado en un coche
//! mientras el tren avanza, bajar a un andén, saltear hasta la próxima parada y
//! esperar en un andén el próximo tren.
//!
//! Todas las reglas de este módulo son puras. El juego lee las teclas,
//! reproduce el fundido y mueve al jugador; el reloj y el tren saltan mediante
//! [`skip_ride`] y [`wait_for_train`], que dejan el tren exactamente donde lo
//! dejaría seguir el reloj ([`LineTrain::state_at`]).

use crate::{
    BASE_RATE, CarKind, Circle, Floor, GameClock, Heading, LinePhase, LineTrain, LineTrainState,
    PlacedCar, RailBay, Rider, Run, TimeOfDay, TownLayout, Travel, Vec2, World,
};

/// El multiplicador del reloj mientras el jugador viaja en el tren: 12× en
/// lugar de los 30× de base, así que la línea entera (unos 11 minutos reales)
/// equivale a unas dos horas de juego.
pub const ONBOARD_MULTIPLIER: f64 = 0.4;

/// Distancia máxima a la que puede estar una persona de la puerta de un coche
/// para subir, en metros. Un coche solo (el de la lanzadera) se detiene con sus
/// puertas a 3,4 m del lugar central a lo largo del andén y a 3,5 m a lo ancho,
/// así que 5 las alcanza desde ahí.
pub const BOARD_RANGE: f32 = 5.0;

/// Distancia máxima desde el centro de un andén ([`TownLayout::platform_spot`])
/// a la que puede estar una persona para esperar ahí el tren, en metros.
pub const WAIT_RANGE: f32 = 8.0;

/// Distancia máxima desde el lugar de parada de un andén a la que puede estar
/// una persona y seguir contando como parada en él, para [`boarding_at`], en
/// metros.
pub const STOP_SPOT_RADIUS: f32 = 1.5;

/// Altura de los pies de un pasajero sentado sobre la parte superior de los
/// rieles, en metros.
pub const SEAT_HEIGHT: f32 = 1.0;

/// Segundos reales que pasan desde que empieza el juego hasta que el tren de la
/// mañana entra en el primer pueblo: lo bastante para verlo llegar, y lo
/// bastante poco para no aburrir.
pub const OPENING_LEAD: f64 = 50.0;

/// Segundos reales antes de la próxima estación en los que termina un viaje
/// salteado, para que el tren igual entre y frene hasta detenerse ahí.
pub const APPROACH_LEAD: f64 = 3.0;

/// A qué distancia hacia adentro de cada extremo del andén baja un pasajero,
/// como mucho, en metros.
const PLATFORM_END_MARGIN: f32 = 2.0;

/// A qué distancia hacia adentro de cada extremo de un coche están sus puertas
/// de los extremos, en metros.
const DOOR_INSET: f32 = 0.6;

/// El jugador que viaja en un tren.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ride {
    /// El tren, como índice dentro de los trenes del mundo (el tren de la línea
    /// es el 0). [`Self::new`] indica 0; [`boarding`] y [`Self::on_train`] lo
    /// asignan.
    pub train: usize,
    /// El coche en el que va sentado el jugador, como índice dentro de
    /// [`LineTrain::cars`].
    pub car: usize,
    /// La estación (índice dentro de [`LineTrain::stations`]) en la que se baja
    /// al jugador apenas el tren se detiene ahí, si la hay (la apertura).
    pub alight_at: Option<usize>,
    /// De qué lado del coche se sienta el jugador junto a la ventanilla: `1.0`
    /// a la derecha del vagón (mirando hacia donde avanza), `-1.0` a su
    /// izquierda. Se elige mirando hacia el andén de la próxima parada
    /// ([`Self::facing_platform`]).
    pub side: f32,
    /// Si el tren avanzó desde que subió el jugador: llegar a una terminal
    /// termina entonces el viaje, mientras que subir a un tren que espera en
    /// una terminal no lo termina.
    under_way: bool,
}

/// Lo que hace la tecla de interacción al lado del tren de la línea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Boarding {
    /// Subir: el viaje en ese coche.
    Board(Ride),
    /// Montado junto a la puerta de un coche: el caballo se queda en el andén,
    /// así que el jugador primero tiene que bajarse.
    DismountFirst,
    /// No hay ningún tren al que subir aquí.
    Nothing,
}

impl Ride {
    /// Un viaje en el coche `car` de `train`, desde donde está ahora.
    pub fn new(train: &LineTrain, car: usize) -> Self {
        Self {
            train: 0,
            car,
            alight_at: None,
            side: 1.0,
            under_way: train.state().phase != LinePhase::Parked,
        }
    }

    /// Este viaje en el tren de índice `train` entre los trenes del mundo.
    pub fn on_train(self, train: usize) -> Self {
        Self { train, ..self }
    }

    /// Este viaje sentado del lado del coche que mira hacia el andén de la
    /// próxima parada del tren (desde una estación, la siguiente), para que la
    /// llegada se vea por la ventanilla (una parada en una vía muerta tiene el
    /// andén de la vía muerta, uno de `floors`). Sin cambios si no hay ninguna.
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

    /// Un viaje en el coche `car` que termina en `station`: se baja al jugador
    /// ahí apenas el tren se detiene.
    pub fn to_station(train: &LineTrain, car: usize, station: usize) -> Self {
        Self {
            alight_at: Some(station),
            ..Self::new(train, car)
        }
    }

    /// Dónde está el pasajero sentado: en el centro del coche, mirando hacia
    /// donde avanza el tren. `None` para un vagón que el tren no tiene.
    pub fn seat(&self, train: &LineTrain) -> Option<(Vec2, Heading)> {
        let coach = *train.placed_cars().get(self.car)?;
        let d = coach.direction;
        Some((coach.center, Heading::from_radians(d.y.atan2(d.x))))
    }

    /// Sigue al tren hasta su estado en `now`; `true` cuando el viaje termina
    /// aquí y el jugador tiene que bajarse: en la estación donde termina el
    /// viaje, o en una terminal a la que llegó el tren con el jugador a bordo.
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

/// Si el tren está detenido (en una parada o esperando en la terminal) en
/// `station`.
pub fn stands_at(state: &LineTrainState, station: usize) -> bool {
    state.phase != LinePhase::Running && state.station == Some(station)
}

/// Las dos puertas de los extremos de cada lado de un coche, sobre sus
/// costados.
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

/// El coche de `train` cuya puerta está más cerca de `rider`, y la distancia
/// hasta ella, mientras el tren está detenido en una estación y el jinete está
/// en un andén (uno de `floors`).
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

/// Lo que hace la tecla de interacción para `rider` al lado de los `trains`:
/// subir al coche cuya puerta está a [`BOARD_RANGE`] o menos mientras su tren
/// está detenido en una estación y el jinete está a pie en un andén (uno de
/// `floors`: el de una estación o el de una vía muerta). Si hay varios trenes
/// al alcance, gana el de la puerta más cercana. Ver [`boarding_at`] para el
/// alcance mayor en un lugar de parada.
pub fn boarding(trains: &[LineTrain], rider: &Rider, floors: &[Floor]) -> Boarding {
    boarding_at(trains, &[], rider, floors)
}

/// [`boarding`], pero un jinete que espera en el lugar de parada del andén de
/// una estación ([`TownLayout::platform_spot`], a [`STOP_SPOT_RADIUS`] o menos)
/// alcanza la puerta más cercana del tren detenido ahí hasta a [`WAIT_RANGE`].
/// En las terminales de la línea la formación se detiene centrada en la
/// estación, así que la puerta más cercana de sus coches queda a unos 7 m del
/// lugar, de un lado o del otro según cómo la haya dado vuelta la espera en la
/// terminal. Todos los demás casos conservan [`BOARD_RANGE`].
pub fn boarding_at(
    trains: &[LineTrain],
    towns: &[TownLayout],
    rider: &Rider,
    floors: &[Floor],
) -> Boarding {
    let nearest = trains
        .iter()
        .enumerate()
        .filter_map(|(t, train)| {
            nearest_coach_door(train, rider, floors).map(|(c, gap)| (t, c, gap))
        })
        .filter(|&(t, _, gap)| gap <= reach(&trains[t], towns, rider))
        .min_by(|a, b| a.2.total_cmp(&b.2));
    match nearest {
        Some((t, car, _)) => match rider.travel() {
            Travel::OnFoot => Boarding::Board(Ride::new(&trains[t], car).on_train(t)),
            Travel::Riding => Boarding::DismountFirst,
        },
        None => Boarding::Nothing,
    }
}

/// Desde qué distancia de la puerta de un coche puede `rider` subir a `train`:
/// [`WAIT_RANGE`] en el lugar de parada del andén de la estación donde está
/// detenido el tren y, si no, [`BOARD_RANGE`].
fn reach(train: &LineTrain, towns: &[TownLayout], rider: &Rider) -> f32 {
    let state = train.state();
    let at_stop_spot = state
        .station
        .and_then(|s| train.stations().get(s))
        .filter(|station| !station.bay && state.phase != LinePhase::Running)
        .and_then(|station| towns.get(station.town)?.platform_spot())
        .is_some_and(|(spot, _)| spot.distance(rider.position) <= STOP_SPOT_RADIUS);
    if at_stop_spot {
        WAIT_RANGE
    } else {
        BOARD_RANGE
    }
}

/// Dónde baja un pasajero del coche `car` mientras el tren está detenido en una
/// estación: sobre el andén de ese pueblo (para un tren de vía muerta, el andén
/// de la vía muerta entre los pisos de `world`) al lado de la puerta del coche
/// más cercana al centro del andén (para que F vuelva a subir desde ahí), un
/// paso hacia adentro desde su borde junto a la vía, de espaldas a la vía,
/// apartado de todo lo que hay en `world` (cuyos obstáculos en movimiento
/// deberían ser los vagones del tren). `None` mientras el tren avanza o cuando
/// el pueblo de la estación no tiene andén.
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

/// El piso de `floors` más cercano a `at`.
fn nearest_floor(floors: &[Floor], at: Vec2) -> Option<Floor> {
    let gap = |f: &&Floor| f.footprint.closest_point(at).distance(at);
    floors
        .iter()
        .min_by(|a, b| gap(a).total_cmp(&gap(b)))
        .copied()
}

/// El lugar `middle` del andén desplazado a lo largo de `floor` hasta quedar al
/// lado de `door`, fuera de sus rampas, apunte hacia donde apunte
/// `floor.along`.
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

/// Un andén en el que esperar un tren: el andén de la estación de un pueblo, o
/// el andén de la vía muerta que está al lado (el de la lanzadera del ramal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitingPlace {
    /// Índice (dentro de los pueblos del mundo) del pueblo del andén.
    pub town: usize,
    /// Si es el andén de la vía muerta del pueblo en lugar del de su estación.
    pub bay: bool,
}

impl WaitingPlace {
    /// La estación de `train` que para en este andén: la de su pueblo, en su
    /// vía muerta o no, según sea el andén.
    fn station_of(self, train: &LineTrain) -> Option<usize> {
        train
            .stations()
            .iter()
            .position(|s| s.town == self.town && s.bay == self.bay)
    }
}

/// El andén en el que espera `rider`: a pie, a [`WAIT_RANGE`] o menos del
/// centro del andén de la estación de un pueblo, o en un andén de vía muerta
/// (uno de `bays`), en el que para algún tren, mientras no haya ninguno
/// detenido al lado (en ese caso el jinete sube). Los trenes que paran en el
/// otro andén del pueblo no cuentan: desde el andén de la estación el jinete
/// espera los trenes de ahí, no la lanzadera en su vía muerta.
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

/// Adelanta el reloj (a la velocidad base) y todos los trenes hasta que alguno
/// de ellos llegue a `place` (el que llegue primero), y devuelve los segundos
/// de juego salteados (cero cuando ya hay uno detenido ahí). Si no llega
/// ninguno antes, el tren del día siguiente.
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

/// Lleva a todos los trenes menos `ridden` junto con un salto de reloj de
/// `game` segundos desde la hora de juego `from` a `rate` segundos de juego por
/// segundo real, después de que [`skip_ride`], [`depart_now`] o
/// [`skip_to_approach`] movieran por su cuenta el tren en el que se viaja.
pub fn follow_jump(trains: &mut [LineTrain], ridden: usize, from: f64, game: f64, rate: f64) {
    for (i, train) in trains.iter_mut().enumerate() {
        if i != ridden {
            train.advance(from, game, rate);
        }
    }
}

/// Saltea el resto del viaje hasta la próxima parada del tren: el reloj salta
/// el tiempo de juego que tardaría el viaje a su velocidad actual (a bordo), y
/// el tren hasta su llegada ahí. Devuelve esa estación.
pub fn skip_ride(train: &mut LineTrain, clock: &mut GameClock) -> Option<usize> {
    let (station, game) = train.skip_to_next_stop(clock.elapsed(), clock.rate())?;
    clock.advance_by(game);
    Some(station)
}

/// Hace salir al tren de la estación en la que está detenido ahora (el resto de
/// la parada, o la espera para la salida desde una terminal), con el reloj
/// saltando el mismo tiempo de juego a su velocidad actual. Devuelve los
/// segundos de juego saltados: cero para un tren que ya está en marcha.
pub fn depart_now(train: &mut LineTrain, clock: &mut GameClock) -> Option<f64> {
    let game = train.skip_to_departure(clock.elapsed(), clock.rate())?;
    clock.advance_by(game);
    Some(game)
}

/// Saltea el viaje hasta [`APPROACH_LEAD`] segundos reales antes de la próxima
/// parada, con el reloj saltando el tiempo de juego que eso lleva a su
/// velocidad actual, para que el tren entre de verdad desde ahí. Devuelve esa
/// estación.
pub fn skip_to_approach(train: &mut LineTrain, clock: &mut GameClock) -> Option<usize> {
    let (station, game) =
        train.skip_toward_next_stop(clock.elapsed(), clock.rate(), APPROACH_LEAD)?;
    clock.advance_by(game);
    Some(station)
}

/// La próxima estación a la que llega el tren y la hora del día en que lo hace,
/// con el reloj corriendo a su velocidad actual.
pub fn next_arrival(train: &LineTrain, clock: &GameClock) -> Option<(usize, TimeOfDay)> {
    let (station, game) = train.next_arrival(clock.elapsed(), clock.rate())?;
    Some((station, TimeOfDay::from_seconds(clock.elapsed() + game)))
}

/// La apertura del juego: el tren ubicado [`OPENING_LEAD`] segundos reales
/// antes de que `run` llegue a la estación de `town`, el reloj marcando una
/// hora tal que seguir viajando a la velocidad a bordo lo lleve ahí a su hora
/// de llegada según el horario (a la velocidad base), y el jugador sentado en
/// el primer coche, para bajarlo en esa estación. `None` cuando el pueblo no
/// tiene estación o el tren no tiene coches.
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

    /// Castex, Monte Nievas y Pico tal como se distribuyen, su mundo con el
    /// tren como obstáculos en movimiento, y el tren.
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

    /// El tren saltado desde la medianoche hasta la hora de juego `at`.
    fn at(train: &LineTrain, at: f64) -> LineTrain {
        let mut train = train.clone();
        train.advance(0.0, at, BASE_RATE);
        train
    }

    /// El tren detenido en `station` en su próxima visita después de `from`.
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

    /// La dirección unitaria transversal al coche `car`, hacia el andén.
    fn to_platform(train: &LineTrain, car: usize, towns: &[TownLayout]) -> Vec2 {
        let station = train.state().station.unwrap();
        let (middle, _) = town_of(towns, train, station).platform_spot().unwrap();
        let c = train.placed_cars()[car];
        let across = Vec2::new(-c.direction.y, c.direction.x);
        across * (middle - c.center).dot(across).signum()
    }

    /// Un lugar del andén a 3 m de la puerta del coche de ese lado más cercana
    /// al centro del andén.
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
        // El antiguo punto de partida del andén, al lado del centro de los
        // coches, está al alcance de una puerta.
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
    fn the_line_train_is_boarded_from_each_terminals_middle_spot_all_layover() {
        let (towns, world, train) = shipped();
        let last = train.stations().len() - 1;
        let mut boarded = 0;
        for (station, run) in [(CASTEX, Run::Up), (last, Run::Down)] {
            let (middle, _) = town_of(&towns, &train, station).platform_spot().unwrap();
            let rider = on_foot(middle, &world);
            let (arrive, depart) = train.stop_times(run, station).unwrap();
            let first = train.arrival_time_at(run, station).unwrap().seconds();
            for k in 0..=20 {
                let now = first + (depart - arrive) * f64::from(k) / 20.0;
                let now_train = at(&train, now);
                if !stands_at(&now_train.state(), station) {
                    continue;
                }
                assert!(
                    matches!(
                        boarding_at(
                            std::slice::from_ref(&now_train),
                            &towns,
                            &rider,
                            &world.floors
                        ),
                        Boarding::Board(_)
                    ),
                    "no boarding from the middle spot at station {station}, {now}"
                );
                boarded += 1;
            }
        }
        assert!(boarded >= 20, "{boarded}");
    }

    #[test]
    fn away_from_the_stop_spot_the_terminal_door_stays_beyond_board_range() {
        let (towns, world, train) = shipped();
        let (train, _) = standing(&train, 0.0, CASTEX);
        let (middle, _) = town_of(&towns, &train, CASTEX).platform_spot().unwrap();
        let along = train.placed_cars()[coaches(&train)[0]].direction;
        // 2 m a lo largo del andén hacia la puerta más cercana: fuera del radio
        // del lugar de parada, pero todavía a 5 a 8 m de la puerta, así que
        // solo aplica BOARD_RANGE.
        let (rider, gap) = [-2.0, 2.0]
            .map(|step| on_foot(middle + along * step, &world))
            .map(|r| {
                let gap = nearest_coach_door(&train, &r, &world.floors).unwrap().1;
                (r, gap)
            })
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert!(gap > BOARD_RANGE && gap <= WAIT_RANGE, "{gap}");
        assert_eq!(
            boarding_at(std::slice::from_ref(&train), &towns, &rider, &world.floors),
            Boarding::Nothing
        );
        // Y justo en el lugar aplica el alcance mayor.
        let rider = on_foot(middle, &world);
        let gap = nearest_coach_door(&train, &rider, &world.floors).unwrap().1;
        assert!(gap > BOARD_RANGE && gap <= WAIT_RANGE, "{gap}");
    }

    #[test]
    fn no_boarding_while_running_mounted_far_away_or_off_the_platform() {
        let (towns, world, train) = shipped();
        let floors = &world.floors;
        let (dwelling, _) = standing(&train, 0.0, NIEVAS);
        let car = coaches(&dwelling)[0];
        let spot = by_the_door(&dwelling, car, &towns);
        // Montado: primero hay que bajarse del caballo.
        let mounted = Rider::new(spot, Heading::from_radians(0.0), &world);
        assert_eq!(
            boarding(std::slice::from_ref(&dwelling), &mounted, floors),
            Boarding::DismountFirst
        );
        // Demasiado lejos de todas las puertas a lo largo del andén.
        let far = spot + dwelling.placed_cars()[car].direction * 40.0;
        assert_eq!(
            boarding(
                std::slice::from_ref(&dwelling),
                &on_foot(far, &world),
                floors
            ),
            Boarding::Nothing
        );
        // Del otro lado de la vía: no está en el andén.
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
        // Un momento después de que sale: no hay nada a qué subir.
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
        // Monte Nievas a mitad del recorrido, y las dos terminales con la
        // formación centrada en la estación.
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
                // Al alcance de una puerta, para volver a subir.
                assert!(matches!(
                    boarding(
                        std::slice::from_ref(&train),
                        &on_foot(spot, &world),
                        &world.floors
                    ),
                    Boarding::Board(_)
                ));
                // Sobre el andén.
                assert_eq!(
                    world.ground_height(spot),
                    world.terrain.height(spot.x, spot.y) + floor.height
                );
            }
        }
        // No mientras avanza.
        let running = at(&train, hm(7, 3));
        assert_eq!(alight_spot(&running, 1, &towns, &world), None);
    }

    #[test]
    fn the_alighting_spot_slides_beside_the_door_whichever_way_the_platform_runs() {
        // Un andén de 40 m a lo largo de x y después a lo largo de y, cada uno
        // nombrado en ambos sentidos.
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
                // Una puerta a 9 m del centro, al lado del borde del andén.
                let door = middle + axis * 9.0 - across * 3.0;
                let spot = beside_door(&floor, middle, door);
                assert!(
                    spot.distance(middle + axis * 9.0) < 1e-4,
                    "{along:?}: {spot:?}"
                );
                // Más allá del extremo, retenida a un margen antes de la rampa.
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
        // Subió en Monte Nievas en el recorrido de subida: sigue hasta Castex.
        let (mut train, now) = standing(&train, 0.0, NIEVAS);
        let mut ride = Ride::new(&train, car);
        assert!(!ride.put_off(&train.state()));
        let mut clock = GameClock::at(TimeOfDay::from_seconds(now));
        clock.multiplier = ONBOARD_MULTIPLIER;
        assert_eq!(skip_ride(&mut train, &mut clock), Some(CASTEX));
        assert_eq!(train.state().phase, LinePhase::Parked);
        assert!(ride.put_off(&train.state()), "put off at the terminal");

        // Subió mientras esperaba en Castex: no se lo baja hasta que el tren
        // avanzó.
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

        // Un viaje hasta una estación termina apenas el tren se detiene ahí.
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
        // 12× durante el resto del tramo.
        let expected = from + (arrive - 100.0) * 12.0;
        assert!((eta.seconds() - expected).abs() < 1e-6);
        assert_eq!(skip_ride(&mut train, &mut clock), Some(NIEVAS));
        assert!((clock.elapsed() - expected).abs() < 1e-6);
        let s = train.state();
        assert_eq!((s.phase, s.station), (LinePhase::Dwelling, Some(NIEVAS)));
        assert_eq!(s, train.state_at(Run::Up, arrive));
        // La hora estimada de llegada desde la parada es Castex.
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
        // Tres segundos reales de salida, a 12×.
        let game = clock.tick(3.0);
        train.advance(clock.elapsed() - game, game, clock.rate());
        assert_eq!(train.state().phase, LinePhase::Running);
        assert_eq!(depart_now(&mut train, &mut clock), Some(0.0));
        // El negro: hasta 3 s antes de Castex, el reloj avanza el mismo tiempo
        // de viaje.
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
        // Entrando de verdad: detenido en Castex a horario.
        let game = clock.tick(APPROACH_LEAD as f32);
        train.advance(clock.elapsed() - game, game, clock.rate());
        assert!(stands_at(&train.state(), CASTEX));
        assert!((clock.elapsed() - eta.seconds()).abs() < 1e-3);
    }

    #[test]
    fn waiting_on_a_platform_lands_on_the_next_train_there() {
        let (towns, world, train) = shipped();
        let nievas = town_of(&towns, &train, NIEVAS);
        // La espera es por pueblo: la lanzadera también puede pasar por ahí.
        let (nievas_town, castex_town) =
            (train.stations()[NIEVAS].town, train.stations()[CASTEX].town);
        let (middle, _) = nievas.platform_spot().unwrap();
        // 13:00: el tren espera en Castex; el próximo en Monte Nievas es el
        // recorrido de bajada de la tarde.
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
        // El estado puro en esa llegada.
        let (arrive, _) = train.stop_times(Run::Down, NIEVAS).unwrap();
        assert!((s.front - train.state_at(Run::Down, arrive).front).abs() < 1e-3);
        // Con el tren ahí, no hay nada que esperar.
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

        // Después del tren de la noche, la espera se extiende hasta la mañana
        // siguiente.
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
        // En una terminal el tren espera: también está detenido ahí.
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

    /// Si el asiento de `ride` mira hacia el andén de la estación en la que
    /// está detenido el tren.
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
        // Entrando en el recorrido de la mañana, en el de la tarde y hacia las
        // dos terminales: sentado del lado del andén cuando se detiene.
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
        // Subió durante una parada: el andén de la próxima parada, no el de
        // esta.
        let (dwelling, _) = standing(&train, 0.0, NIEVAS);
        let mut ride = Ride::new(&dwelling, car).facing_platform(&dwelling, &towns, &[]);
        let mut on = dwelling.clone();
        on.skip_to_next_stop(0.0, BASE_RATE);
        assert!(faces_the_platform(&on, &ride, &towns));
        assert!(!ride.put_off(&dwelling.state()));
        // En estos recorridos los andenes quedan a ambos lados, así que el
        // asiento realmente se elige y no es fijo.
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
        // Seguir viajando a 12× llega a la hora del horario.
        let game = clock.tick(OPENING_LEAD as f32);
        train.advance(clock.elapsed() - game, game, ONBOARD_MULTIPLIER * BASE_RATE);
        let arrival = train.arrival_time_at(Run::Up, NIEVAS).unwrap();
        assert!((clock.time_of_day().seconds() - arrival.seconds()).abs() < 0.01);
        assert!(ride.put_off(&train.state()));
        assert!(towns[nievas].platform_spot().is_some());
        // Sentado mirando hacia el andén de Monte Nievas mientras entra.
        let seated = ride.facing_platform(&train, &towns, &[]);
        assert!(faces_the_platform(&train, &seated, &towns));
        assert_eq!(opening(&mut train, 99, Run::Up), None);
    }

    // El mundo con ramal: el tren de la línea pasa por Medio y la lanzadera
    // está en la vía muerta de Medio camino a Norte.

    use crate::train::line_tests::{MEDIO, NORTE, branched_line, branched_trains};

    /// Los pueblos, su mundo (con el andén de la vía muerta colocado), las vías
    /// muertas de la línea y los trenes del mundo: el tren de la línea y
    /// después la lanzadera.
    fn branched() -> (Vec<TownLayout>, World, Vec<RailBay>, Vec<LineTrain>) {
        let (towns, line, shuttle) = branched_trains(crate::Timetable::default());
        let mut world = World::with_towns(WorldConfig::default(), Terrain::default(), &towns);
        let bays = branched_line().1;
        world.lay_rail(&bays);
        (towns, world, bays.bays, vec![line, shuttle])
    }

    /// Todos los trenes saltados desde la medianoche hasta la hora de juego
    /// `now`.
    fn all_at(trains: &[LineTrain], now: f64) -> Vec<LineTrain> {
        trains.iter().map(|t| at(t, now)).collect()
    }

    /// Los trenes detenidos (no en marcha) en Medio.
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

    /// Un lugar 1 m más allá de la puerta del coche de la lanzadera más cercana
    /// a la vía del tren de la línea: el terreno entre las dos vías, fuera de
    /// cualquier andén.
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

    /// Un lugar del andén de la vía muerta al lado de la puerta del coche de la
    /// lanzadera más cercana al lugar central del andén.
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
        // Los dos están detenidos en Medio: el tren de la línea junto al andén
        // y la lanzadera en su vía muerta.
        let now = trains[0].stop_at(MEDIO, Run::Up).unwrap().seconds();
        let trains = all_at(&trains, now);
        assert_eq!(standing_at_medio(&trains), [0, 1]);
        assert!(trains[1].stands_in_bay() && !trains[0].stands_in_bay());
        // En el andén, junto al coche del tren de la línea: el tren de la
        // línea.
        let car = coaches(&trains[0])[0];
        let spot = by_the_door(&trains[0], car, &towns);
        let rider = on_foot(spot, &world);
        let Boarding::Board(ride) = boarding(&trains, &rider, &world.floors) else {
            panic!("boards the line train");
        };
        assert_eq!((ride.train, ride.car), (0, car));
        // En el andén de la vía muerta, junto al coche de la lanzadera: la
        // lanzadera.
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
        // Montado ahí, también hay que bajarse primero.
        let mounted = Rider::new(spot, Heading::from_radians(0.0), &world);
        assert_eq!(
            boarding(&trains, &mounted, &world.floors),
            Boarding::DismountFirst
        );
        // Lejos de los dos, nada.
        let far = on_foot(spot + Vec2::new(0.0, 40.0), &world);
        assert_eq!(boarding(&trains, &far, &world.floors), Boarding::Nothing);
    }

    #[test]
    fn nothing_is_boarded_from_the_ground_not_even_beside_the_bay() {
        let (towns, world, _, trains) = branched();
        let now = trains[0].stop_at(MEDIO, Run::Up).unwrap().seconds();
        let trains = all_at(&trains, now);
        // Al lado del coche del tren de la línea pero fuera del andén (del otro
        // lado de la vía): ahí no se puede subir.
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
        // Tampoco al lado de la lanzadera en su vía muerta, en el terreno entre
        // las vías.
        let ground = on_foot(beside_the_bay(&trains[0], &trains[1]), &world);
        assert!(
            !world
                .floors
                .iter()
                .any(|f| f.footprint.contains_point(ground.position))
        );
        assert_eq!(boarding(&trains, &ground, &world.floors), Boarding::Nothing);
        // Se sube desde el andén de la vía muerta, pero no una vez que salió.
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
            // No hay nada que esperar junto a un andén con su tren detenido.
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
                // El tren de ese andén llegó, en su lugar.
                assert!(standing_at_medio(&waiting).contains(&want), "{now}");
                if want == 1 {
                    shuttle_waits += 1;
                } else if standing.contains(&other) {
                    line_while_shuttle_in += 1;
                }
            }
        }
        // La lanzadera está detenida antes y después del tren de la línea: la
        // espera del tren de la línea continúa aunque ella esté en su vía
        // muerta (no es el tren de ese andén), y la espera en el andén de la
        // vía muerta es por la lanzadera.
        assert!(line_while_shuttle_in > 0 && shuttle_waits > 0);
    }

    #[test]
    fn waiting_at_norte_takes_the_shuttle_which_alone_serves_it() {
        let (towns, world, _, trains) = branched();
        let (middle, _) = towns[NORTE].platform_spot().unwrap();
        let rider = on_foot(middle, &world);
        // 13:00: la lanzadera está en algún punto de su jornada; se la espera
        // en Norte.
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
        // Subir desde el andén y seguir viajando la deja en Medio, mientras el
        // tren de la línea sigue el reloj en el camino.
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
        // Un momento con los dos trenes en marcha.
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
        // El tren en el que se viaja está detenido en su próxima parada.
        assert_eq!(Some(station), now_trains[1].state().station);
        assert!(stands_at(&now_trains[1].state(), station));
        // El tren de la línea está donde lo pone el reloj: no saltó hasta una
        // parada.
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
        // La primera espera en la terminal, de la llegada a la salida: su
        // primera mitad antes de que la lanzadera dé la vuelta y la segunda
        // después.
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
        // En el andén de la vía muerta, al alcance de la puerta del coche, de
        // espaldas a la vía; no en el andén de la estación del otro lado de la
        // línea.
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
        // En camino hacia Medio (el recorrido de subida), un minuto antes de la
        // parada.
        let arrival = shuttle.arrival_time_at(Run::Up, medio).unwrap().seconds();
        let running = at(shuttle, arrival - 60.0);
        assert_eq!(running.state().phase, LinePhase::Running);
        let car = coaches(&running)[0];
        let ride = Ride::new(&running, car).facing_platform(&running, &towns, &world.floors);
        // Donde está detenido, el andén está del lado hacia el que mira el
        // viaje.
        let standing = at(shuttle, arrival + 1.0);
        assert_eq!(standing.state().station, Some(medio));
        let coach = standing.placed_cars()[car];
        let bay = nearest_floor(&world.floors, coach.center).unwrap();
        let (spot, _) = bay.spot_beside(coach.center);
        let right = Vec2::new(coach.direction.y, -coach.direction.x);
        assert_eq!((spot - coach.center).dot(right).signum(), ride.side);
        // No del lado del andén de la estación, del otro lado de la línea.
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
        // Justo después de que el primero vuelve a salir, esperar en Norte
        // termina en el segundo.
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
