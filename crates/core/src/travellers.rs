//! Viajeros: jinetes y carros que van de pueblo a pueblo por los caminos.
//!
//! Deterministas para una semilla dada. Cada camino entre dos pueblos
//! ([`RoadKind::Between`]) tiene dos jinetes, uno que sale de cada punta, y un
//! vehículo (un sulky, un carretón o una carreta, [`Vehicle::DRAWN`]) que va al
//! paso, con su propia semilla, de viaje ([`Route`]): de una estancia, una
//! pulpería o un almacén de ramos generales a otro (usuario, 2026-11-03: "de
//! alguna estancia a alguna pulpería, de algun almacen de ramos generales a la
//! pulpería del otro pueblo"), por los caminos. Al llegar a un pueblo de paso
//! se detiene un rato y sigue por el camino siguiente; al llegar a su destino
//! descansa y emprende la vuelta. Pasa de la salida de un camino a la del
//! siguiente a través del pueblo solo cuando el jugador no lo ve
//! ([`HOP_CLEAR`]). Los jinetes van
//! por la mano izquierda del camino, al paso o al trote; al llegar descansan un
//! rato en la salida de calle del pueblo y emprenden la vuelta. Frenan ante
//! alguien que tienen adelante y, si no se corre, lo pasan por el costado. Antes
//! de un paso a nivel esperan a que pase el tren.

use crate::rng::Rng;
use crate::{
    AnimationState, Body, Circle, Heading, Rider, RiderParams, RoadKind, RoadNetwork, SideObstacle,
    Vec2, Vehicle,
};

/// Un jinete de viaje por un camino.
#[derive(Debug, Clone, PartialEq)]
pub struct Traveller {
    pub position: Vec2,
    pub heading: Heading,
    /// Velocidad actual sobre el suelo, en m/s.
    pub speed: f32,
    pub collider: Circle,
    /// Variación de apariencia del jinete; el render decide cómo se ve.
    pub look: u8,
    /// Semilla del pelaje del caballo; el render decide cómo se ve.
    pub coat: u32,
    /// Índice del camino en [`RoadNetwork::roads`].
    pub road: usize,
    /// Distancia recorrida desde el principio del camino, en metros.
    pub along: f32,
    /// Si va en el sentido del camino (de `from` a `to`).
    pub forward: bool,
    /// El vehículo que lleva (un carro), o nada si va montado.
    pub vehicle: Option<Vehicle>,
    /// Metros que lleva recorridos: las ruedas de un carro giran con ellos.
    pub travelled: f32,
    /// El viaje de un vehículo, de parada en parada; un jinete va y viene por
    /// su camino.
    pub route: Option<Route>,
    /// El tramo de [`Self::route`] que está recorriendo.
    pub leg: usize,
    /// Ritmo de marcha propio, en m/s.
    pace: f32,
    /// Segundos de descanso que le quedan en la punta del camino.
    rest: f32,
    /// Segundos que lleva frenado ante alguien.
    waiting: f32,
    /// Cuánto se corre hacia afuera de su mano para pasar a alguien, en metros.
    dodge: f32,
    rng: Rng,
}

impl Traveller {
    /// Lo que está haciendo el caballo, con los mismos umbrales que el del
    /// jugador.
    pub fn animation(&self) -> AnimationState {
        AnimationState::from_speed(self.speed, RiderParams::default().walk_max)
    }

    /// Dónde está el caballo, como obstáculo para los demás.
    pub fn body(&self) -> Body {
        Body {
            position: self.position,
            collider: self.collider,
        }
    }

    /// Todo lo que ocupa: el caballo, o los caballos y la caja de un carro.
    pub fn bodies(&self) -> Vec<Body> {
        match self.vehicle {
            Some(vehicle) => vehicle.bodies(self.position, self.heading),
            None => vec![self.body()],
        }
    }
}

/// Todos los viajeros de los caminos.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Travellers {
    pub riders: Vec<Traveller>,
}

/// Una parada de un viaje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// Una estancia: un índice dentro de las entradas que recibe
    /// [`Travellers::on`].
    Estancia(usize),
    /// El almacén de ramos generales (la pulpería de un pueblo chico) de un
    /// pueblo: su índice.
    Store(usize),
}

/// Un tramo de un viaje: por el camino `road`, de `from` a `to` metros desde su
/// principio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leg {
    pub road: usize,
    pub from: f32,
    pub to: f32,
}

impl Leg {
    fn forward(&self) -> bool {
        self.to >= self.from
    }

    fn reversed(self) -> Self {
        Self {
            road: self.road,
            from: self.to,
            to: self.from,
        }
    }
}

/// Un viaje por los caminos, de una parada a otra.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub origin: Stop,
    pub destination: Stop,
    /// Los tramos, en orden: cada uno termina en la salida de un pueblo, y el
    /// siguiente sale del mismo pueblo por otro camino.
    pub legs: Vec<Leg>,
}

/// Dónde se sube o se baja a la red de caminos una parada: a un pueblo (por
/// cualquiera de sus caminos) o a un punto de un camino.
#[derive(Debug, Clone, Copy)]
enum Access {
    Town(usize),
    On { road: usize, along: f32 },
}

/// Lo más lejos que puede estar la entrada de una estancia de un camino para
/// tomarla como parada, en metros.
pub const ESTANCIA_REACH: f32 = 120.0;

impl Route {
    /// El viaje más corto de `origin` a `destination` por `roads` (los pueblos
    /// unidos por los caminos entre pueblos), o `None` si no hay o son la
    /// misma parada. `estancias` son las entradas de las estancias.
    pub fn plan(
        roads: &RoadNetwork,
        estancias: &[Vec2],
        origin: Stop,
        destination: Stop,
    ) -> Option<Self> {
        if origin == destination {
            return None;
        }
        let access = |stop: Stop| -> Option<Access> {
            match stop {
                Stop::Store(town) => Some(Access::Town(town)),
                Stop::Estancia(i) => {
                    let (road, along) = nearest_on(roads, *estancias.get(i)?)?;
                    Some(Access::On { road, along })
                }
            }
        };
        let (a, b) = (access(origin)?, access(destination)?);
        // Las puntas de cada acceso: (pueblo, tramo hasta o desde él, metros).
        let ends = |acc: Access, leaving: bool| -> Vec<(usize, Option<Leg>, f32)> {
            match acc {
                Access::Town(t) => vec![(t, None, 0.0)],
                Access::On { road, along } => {
                    let r = &roads.roads[road];
                    let length = roads.length(road);
                    let mut out = vec![(r.from, 0.0, along)];
                    if r.kind == RoadKind::Between {
                        out.push((r.to, length, length - along));
                    }
                    out.into_iter()
                        .map(|(town, end, cost)| {
                            let leg = if leaving {
                                Leg {
                                    road,
                                    from: along,
                                    to: end,
                                }
                            } else {
                                Leg {
                                    road,
                                    from: end,
                                    to: along,
                                }
                            };
                            (town, Some(leg), cost)
                        })
                        .collect()
                }
            }
        };
        let mut best: Option<(f32, Vec<Leg>)> = None;
        if let (Access::On { road: ra, along: x }, Access::On { road: rb, along: y }) = (a, b)
            && ra == rb
        {
            best = Some((
                (x - y).abs(),
                vec![Leg {
                    road: ra,
                    from: x,
                    to: y,
                }],
            ));
        }
        for (start, first, c0) in ends(a, true) {
            for (end, last, c1) in ends(b, false) {
                let Some((c, middle)) = town_path(roads, start, end) else {
                    continue;
                };
                let legs: Vec<Leg> = first.into_iter().chain(middle).chain(last).collect();
                let cost = c0 + c + c1;
                if !legs.is_empty() && best.as_ref().is_none_or(|(b, _)| cost < *b) {
                    best = Some((cost, legs));
                }
            }
        }
        let legs: Vec<Leg> = best?
            .1
            .into_iter()
            .filter(|l| (l.to - l.from).abs() > 1.0)
            .collect();
        (!legs.is_empty()).then_some(Self {
            origin,
            destination,
            legs,
        })
    }

    /// El viaje de vuelta.
    fn reversed(&self) -> Self {
        Self {
            origin: self.destination,
            destination: self.origin,
            legs: self.legs.iter().rev().map(|l| l.reversed()).collect(),
        }
    }
}

/// El punto de un camino más cercano a `p`, como `(camino, metros desde su
/// principio)`, si está a menos de [`ESTANCIA_REACH`].
fn nearest_on(roads: &RoadNetwork, p: Vec2) -> Option<(usize, f32)> {
    let mut best: Option<(f32, usize, f32)> = None;
    for (index, road) in roads.roads.iter().enumerate() {
        if !matches!(road.kind, RoadKind::Between | RoadKind::Rural) {
            continue;
        }
        let mut walked = 0.0;
        for pair in road.path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let length = a.distance(b);
            if length < 1e-4 {
                continue;
            }
            let t = ((p - a).dot(b - a) / (length * length)).clamp(0.0, 1.0);
            let d = p.distance(a + (b - a) * t);
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, index, walked + t * length));
            }
            walked += length;
        }
    }
    best.filter(|(d, _, _)| *d < ESTANCIA_REACH)
        .map(|(_, r, a)| (r, a))
}

/// El camino más corto entre los pueblos `from` y `to` por los caminos entre
/// pueblos, como tramos de punta a punta (ninguno si son el mismo), y su largo.
fn town_path(roads: &RoadNetwork, from: usize, to: usize) -> Option<(f32, Vec<Leg>)> {
    let towns = roads
        .roads
        .iter()
        .map(|r| r.from.max(r.to) + 1)
        .max()
        .unwrap_or(0);
    if from >= towns || to >= towns {
        return None;
    }
    let mut dist = vec![f32::INFINITY; towns];
    let mut via: Vec<Option<Leg>> = vec![None; towns];
    let mut done = vec![false; towns];
    dist[from] = 0.0;
    while let Some(u) = (0..towns)
        .filter(|&t| !done[t] && dist[t].is_finite())
        .min_by(|&a, &b| dist[a].total_cmp(&dist[b]))
    {
        done[u] = true;
        for (index, road) in roads.roads.iter().enumerate() {
            if road.kind != RoadKind::Between {
                continue;
            }
            let length = roads.length(index);
            let (next, leg) = if road.from == u {
                (
                    road.to,
                    Leg {
                        road: index,
                        from: 0.0,
                        to: length,
                    },
                )
            } else if road.to == u {
                (
                    road.from,
                    Leg {
                        road: index,
                        from: length,
                        to: 0.0,
                    },
                )
            } else {
                continue;
            };
            if dist[u] + length < dist[next] {
                dist[next] = dist[u] + length;
                via[next] = Some(leg);
            }
        }
    }
    if !dist[to].is_finite() {
        return None;
    }
    let mut legs = Vec::new();
    let mut at = to;
    while at != from {
        let leg = via[at]?;
        let r = &roads.roads[leg.road];
        at = if leg.to > leg.from { r.from } else { r.to };
        legs.push(leg);
    }
    legs.reverse();
    Some((dist[to], legs))
}

/// Las paradas posibles: cada estancia de `estancias` cerca de un camino y el
/// almacén de cada pueblo con caminos.
fn stops(roads: &RoadNetwork, estancias: &[Vec2]) -> Vec<Stop> {
    let towns = roads
        .roads
        .iter()
        .map(|r| r.from.max(r.to) + 1)
        .max()
        .unwrap_or(0);
    (0..towns)
        .map(Stop::Store)
        .chain(
            (0..estancias.len())
                .filter(|&i| nearest_on(roads, estancias[i]).is_some())
                .map(Stop::Estancia),
        )
        .collect()
}

/// Lo que se detiene un vehículo en un pueblo de paso, en segundos.
const STOPOVER: (f32, f32) = (8.0, 20.0);
/// Lo cerca del jugador que un vehículo no pasa de la salida de un camino a la
/// del siguiente (a través del pueblo, sin dibujarse en el medio), en metros.
pub const HOP_CLEAR: f32 = 250.0;

/// Jinetes por camino entre pueblos: uno que sale de cada punta.
pub const PER_ROAD: usize = 2;
/// Carros por camino entre pueblos.
pub const CARTS_PER_ROAD: usize = 1;
/// Lo que se mezcla con la semilla de los viajeros para la de los carros: los
/// jinetes quedan como sin carros.
const CART_SEED: u32 = 0x6361_7272;
/// Semilla propia de la carreta de bueyes.
const CARRETA_SEED: u32 = 0x6275_6579;
/// Qué parte de los vehículos de los caminos son carretones (el resto,
/// sulkies): en 1919 el grano iba de la chacra a la estación en carros de
/// caballos (usuario, 2026-11-03).
pub const CART_SHARE: f32 = 0.6;
/// Distancia de la línea central del camino a la que va un viajero, a su
/// izquierda, en metros: dentro de los 6,5 m del camino.
pub const LANE: f32 = 1.4;
/// Cuánto más se corre un viajero para pasar a alguien, en metros.
const DODGE: f32 = 2.5;
/// Ritmos al paso, en m/s.
const WALK: (f32, f32) = (1.5, 1.9);
/// Ritmos al trote, en m/s.
const TROT: (f32, f32) = (3.5, 4.5);
/// El paso de una yunta de bueyes, en m/s.
const OX_WALK: (f32, f32) = (0.9, 1.2);
/// Proporción de viajeros que van al trote.
const TROTTERS: f32 = 0.5;
/// Descanso al llegar a un pueblo, en segundos.
const REST: (f32, f32) = (20.0, 60.0);
/// Aceleración y frenada, en m/s².
const ACCEL: f32 = 2.0;
/// Cuánto mira hacia adelante un viajero para frenar, más allá de los cuerpos
/// que se tocarían, en metros.
const LOOK_AHEAD: f32 = 3.0;
/// Segundos frenado antes de pasar a alguien por el costado.
const PATIENCE: f32 = 2.0;
/// Velocidad lateral al correrse, en m/s.
const SIDESTEP: f32 = 1.0;
/// Giro máximo de la orientación, en radianes por segundo.
const TURN_RATE: f32 = 2.5;
/// Dónde espera un viajero a que pase el tren: a esta distancia del centro del
/// paso a nivel, a lo largo del camino, en metros (pasada la rampa del paso).
pub const CROSSING_HOLD: f32 = 12.0;
/// Un vagón a menos de esta distancia del paso a nivel hace esperar a los
/// viajeros, en metros.
pub const TRAIN_NEAR: f32 = 150.0;

impl Travellers {
    /// [`PER_ROAD`] jinetes en cada camino entre pueblos de `roads`, ubicados
    /// a partir de `seed`: uno sale de cada punta, cada uno en un tramo
    /// sembrado del camino. Detrás de todos los jinetes, [`CARTS_PER_ROAD`]
    /// vehículos por camino, cada uno de viaje ([`Route::plan`]) entre dos
    /// paradas sembradas (las estancias cuyas entradas son `estancias`, y el
    /// almacén de cada pueblo), en un tramo sembrado de su viaje.
    pub fn on(roads: &RoadNetwork, estancias: &[Vec2], seed: u32) -> Self {
        let between: Vec<usize> = roads
            .roads
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == RoadKind::Between && r.path.len() >= 2)
            .map(|(road, _)| road)
            .collect();
        let mut riders = Vec::new();
        for &road in &between {
            for k in 0..PER_ROAD {
                let index = (road * PER_ROAD + k) as u32;
                riders.push(Traveller::spawn(
                    roads,
                    road,
                    k % 2 == 0,
                    None,
                    Rng::new(seed, index),
                ));
            }
        }
        let stops = stops(roads, estancias);
        let pick = |rng: &mut Rng, from: &[Stop]| from[rng.below(from.len())];
        let ranches: Vec<Stop> = stops
            .iter()
            .copied()
            .filter(|s| matches!(s, Stop::Estancia(_)))
            .collect();
        // Las estancias sobre una huella rural, para la carreta; sin ninguna,
        // cualquier estancia.
        let rural: Vec<Stop> = ranches
            .iter()
            .copied()
            .filter(|s| match s {
                Stop::Estancia(i) => nearest_on(roads, estancias[*i])
                    .is_some_and(|(r, _)| roads.roads[r].kind == RoadKind::Rural),
                Stop::Store(_) => false,
            })
            .collect();
        let ranch_stops = if rural.is_empty() {
            ranches.clone()
        } else {
            rural
        };
        for &road in &between {
            for k in 0..CARTS_PER_ROAD {
                let mut rng = Rng::new(seed ^ CART_SEED, (road * CARTS_PER_ROAD + k) as u32);
                // Los carretones llevan la carga; los sulkies, la gente.
                let vehicle = if rng.unit() < CART_SHARE {
                    Vehicle::Cart
                } else {
                    Vehicle::Sulky
                };
                // Un viaje que pase por este camino, si lo hay entre unas
                // pocas paradas sembradas; si no, va y viene por él.
                let route = (!stops.is_empty())
                    .then(|| {
                        (0..24).find_map(|_| {
                            // La mitad de las veces, desde o hacia una estancia.
                            let a = if !ranches.is_empty() && rng.unit() < 0.5 {
                                pick(&mut rng, &ranches)
                            } else {
                                pick(&mut rng, &stops)
                            };
                            let b = pick(&mut rng, &stops);
                            let (a, b) = if rng.unit() < 0.5 { (a, b) } else { (b, a) };
                            Route::plan(roads, estancias, a, b)
                                .filter(|r| r.legs.iter().any(|l| l.road == road))
                        })
                    })
                    .flatten();
                let forward = rng.unit() < 0.5;
                let mut cart = Traveller::spawn(roads, road, forward, Some(vehicle), rng);
                if let Some(route) = route {
                    cart.set_route(roads, route);
                }
                riders.push(cart);
            }
        }
        // La carreta de bueyes, rara: una sola, de una estancia a un pueblo
        // por su huella rural.
        let mut rng = Rng::new(seed ^ CARRETA_SEED, 0);
        let carreta = (!ranch_stops.is_empty())
            .then(|| {
                (0..24).find_map(|_| {
                    let from = pick(&mut rng, &ranch_stops);
                    let towns = stops.iter().filter(|s| matches!(s, Stop::Store(_))).count();
                    let to = Stop::Store(rng.below(towns));
                    Route::plan(roads, estancias, from, to)
                })
            })
            .flatten();
        if let Some(route) = carreta {
            let road = route.legs[0].road;
            let mut cart = Traveller::spawn(roads, road, true, Some(Vehicle::Carreta), rng);
            cart.set_route(roads, route);
            riders.push(cart);
        }
        Self { riders }
    }

    /// Hace avanzar a todos `dt` segundos por `roads`, frenando ante
    /// `obstacles` (el jugador) y ante los otros viajeros, y esperando antes de
    /// un paso a nivel mientras algún vagón de `trains` anda cerca. Un `dt` no
    /// finito o no positivo no cambia nada.
    pub fn step(
        &mut self,
        roads: &RoadNetwork,
        obstacles: &[Body],
        trains: &[SideObstacle],
        dt: f32,
    ) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        for index in 0..self.riders.len() {
            let others: Vec<Body> = self
                .riders
                .iter()
                .enumerate()
                .filter(|&(k, _)| k != index)
                .flat_map(|(_, r)| r.bodies())
                .chain(obstacles.iter().copied())
                .collect();
            self.riders[index].step(roads, &others, obstacles, trains, dt);
        }
    }

    /// Dónde está cada caballo y cada carro, como obstáculo para los demás.
    pub fn bodies(&self) -> Vec<Body> {
        self.riders.iter().flat_map(Traveller::bodies).collect()
    }
}

/// El punto a `along` metros del principio de `path` y la dirección unitaria
/// del tramo, sujeto a las puntas.
fn point_at(path: &[Vec2], along: f32) -> (Vec2, Vec2) {
    let mut left = along.max(0.0);
    let mut last = (path[path.len() - 1], Vec2::new(1.0, 0.0));
    for pair in path.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = a.distance(b);
        if length < 1e-4 {
            continue;
        }
        let direction = (b - a) * (1.0 / length);
        if left <= length {
            return (a + direction * left, direction);
        }
        left -= length;
        last = (b, direction);
    }
    last
}

/// El ángulo más corto de `from` a `to`, en `(-PI, PI]`.
fn turn_between(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (to - from).rem_euclid(TAU);
    if d > PI { d - TAU } else { d }
}

impl Traveller {
    fn spawn(
        roads: &RoadNetwork,
        road: usize,
        forward: bool,
        vehicle: Option<Vehicle>,
        mut rng: Rng,
    ) -> Self {
        let along = roads.length(road) * rng.range(0.1, 0.9);
        // Los carros van siempre al paso; los bueyes, más despacio.
        let pace = if vehicle == Some(Vehicle::Carreta) {
            rng.range(OX_WALK.0, OX_WALK.1)
        } else if vehicle.is_none() && rng.unit() < TROTTERS {
            rng.range(TROT.0, TROT.1)
        } else {
            rng.range(WALK.0, WALK.1)
        };
        let mut rider = Self {
            position: Vec2::ZERO,
            heading: Heading::default(),
            speed: 0.0,
            collider: vehicle.map_or(Circle::new(Rider::COLLIDER_RADIUS), Vehicle::collider),
            look: (rng.next() >> 24) as u8,
            coat: rng.next(),
            road,
            along,
            forward,
            vehicle,
            travelled: 0.0,
            route: None,
            leg: 0,
            pace,
            rest: 0.0,
            waiting: 0.0,
            dodge: 0.0,
            rng,
        };
        let (_, direction) = rider.place(roads);
        rider.heading = Heading::from_radians(direction.y.atan2(direction.x));
        rider
    }

    /// Pone al viajero en su mano del camino a la altura de `along`; devuelve
    /// el punto de la línea central y la dirección en la que viaja.
    fn place(&mut self, roads: &RoadNetwork) -> (Vec2, Vec2) {
        let (center, direction) = point_at(&roads.roads[self.road].path, self.along);
        let direction = if self.forward {
            direction
        } else {
            direction * -1.0
        };
        let left = Vec2::new(-direction.y, direction.x);
        self.position = center + left * (LANE + self.dodge);
        (center, direction)
    }

    /// Cuántos metros puede avanzar antes de detenerse a esperar el tren en el
    /// próximo paso a nivel, o `None` si no tiene que esperar: no hay un paso
    /// adelante, ya está en él, o no anda ningún vagón cerca.
    fn room_before_train(&self, roads: &RoadNetwork, trains: &[SideObstacle]) -> Option<f32> {
        roads.roads[self.road]
            .crossings
            .iter()
            .filter(|c| {
                trains
                    .iter()
                    .any(|t| t.footprint.center().distance(c.at) < TRAIN_NEAR)
            })
            .map(|c| {
                if self.forward {
                    c.along - CROSSING_HOLD - self.along
                } else {
                    self.along - (c.along + CROSSING_HOLD)
                }
            })
            // Detrás de la espera (ya en el paso o pasado): sigue de largo.
            .filter(|&room| room >= -1e-3)
            .min_by(f32::total_cmp)
            .map(|room| room.max(0.0))
    }

    /// Lo sube a su viaje `route`, en un punto sembrado de uno de sus tramos
    /// que pasa por su camino actual.
    fn set_route(&mut self, roads: &RoadNetwork, route: Route) {
        let leg = route
            .legs
            .iter()
            .position(|l| l.road == self.road)
            .unwrap_or(0);
        let l = route.legs[leg];
        self.road = l.road;
        self.leg = leg;
        self.forward = l.forward();
        self.along = l.from + (l.to - l.from) * self.rng.range(0.1, 0.9);
        self.route = Some(route);
        let (_, direction) = self.place(roads);
        self.heading = Heading::from_radians(direction.y.atan2(direction.x));
    }

    /// Hasta dónde va por su camino actual: el final de su tramo o, sin
    /// viaje, la punta del camino.
    fn target(&self, roads: &RoadNetwork) -> f32 {
        match &self.route {
            Some(route) => route.legs[self.leg].to,
            None if self.forward => roads.length(self.road),
            None => 0.0,
        }
    }

    /// Terminó un descanso: sigue su viaje (el tramo siguiente, o la vuelta)
    /// o, sin viaje, emprende la vuelta por su camino. Devuelve `false` si
    /// tiene que seguir esperando: pasar al tramo siguiente lo saca de la
    /// vista, y no lo hace con alguien de `watchers` cerca.
    fn set_off(&mut self, roads: &RoadNetwork, watchers: &[Body]) -> bool {
        let Some(route) = &self.route else {
            // Un carro no da media vuelta de golpe: gira de a poco mientras
            // arranca.
            self.forward = !self.forward;
            if self.vehicle.is_none() {
                self.heading = self.heading.rotated(std::f32::consts::PI);
            }
            self.place(roads);
            return true;
        };
        if self.leg + 1 < route.legs.len() {
            let next = route.legs[self.leg + 1];
            let (from, _) = point_at(&roads.roads[next.road].path, next.from);
            let seen = watchers.iter().any(|w| {
                w.position.distance(self.position) < HOP_CLEAR
                    || w.position.distance(from) < HOP_CLEAR
            });
            if seen {
                return false;
            }
            self.leg += 1;
            self.road = next.road;
            self.along = next.from;
            self.forward = next.forward();
            let (_, direction) = self.place(roads);
            self.heading = Heading::from_radians(direction.y.atan2(direction.x));
        } else {
            let back = route.reversed();
            self.route = Some(back);
            self.leg = 0;
            self.forward = !self.forward;
            self.place(roads);
        }
        true
    }

    fn step(
        &mut self,
        roads: &RoadNetwork,
        obstacles: &[Body],
        watchers: &[Body],
        trains: &[SideObstacle],
        dt: f32,
    ) {
        if self.rest > 0.0 {
            self.rest -= dt;
            self.speed = 0.0;
            if self.rest <= 0.0 && !self.set_off(roads, watchers) {
                self.rest = 1.0;
            }
            return;
        }
        let (_, direction) = self.place(roads);
        let blocked = obstacles.iter().any(|o| self.ahead(direction, o));
        let beside = obstacles.iter().any(|o| {
            o.position.distance(self.position)
                < self.collider.radius + o.collider.radius + LOOK_AHEAD
        });
        if blocked {
            self.waiting += dt;
        } else {
            self.waiting = 0.0;
        }
        // Frenado un rato: se corre hacia afuera para pasar; cuando ya dejó
        // atrás a quien pasaba, vuelve de a poco a su mano.
        self.dodge = if self.waiting > PATIENCE {
            (self.dodge + SIDESTEP * dt).min(DODGE)
        } else if blocked || beside {
            self.dodge
        } else {
            (self.dodge - 0.5 * SIDESTEP * dt).max(0.0)
        };
        let room = self.room_before_train(roads, trains);
        let braking = self.speed * self.speed / (2.0 * ACCEL);
        let hold = room.is_some_and(|room| room <= braking + 0.5);
        let wanted = if blocked || hold { 0.0 } else { self.pace };
        self.speed = if self.speed < wanted {
            (self.speed + ACCEL * dt).min(wanted)
        } else {
            (self.speed - ACCEL * dt).max(wanted)
        };

        // Nunca más allá de la espera, aunque venga frenando.
        let stride = room.map_or(self.speed * dt, |room| (self.speed * dt).min(room));
        if room.is_some_and(|room| room <= 0.0) {
            self.speed = 0.0;
        }
        let before = self.along;
        self.along = if self.forward {
            self.along + stride
        } else {
            self.along - stride
        };
        let target = self.target(roads);
        let arrived = if self.forward {
            self.along >= target
        } else {
            self.along <= target
        };
        self.along = if self.forward {
            self.along.min(target)
        } else {
            self.along.max(target)
        };
        self.travelled += (self.along - before).abs();
        let (_, direction) = self.place(roads);
        let turn = turn_between(self.heading.radians(), direction.y.atan2(direction.x));
        let limit = TURN_RATE * dt;
        self.heading = self.heading.rotated(turn.clamp(-limit, limit));
        if arrived {
            self.speed = 0.0;
            self.dodge = 0.0;
            let stopover = self
                .route
                .as_ref()
                .is_some_and(|r| self.leg + 1 < r.legs.len());
            self.rest = if stopover {
                self.rng.range(STOPOVER.0, STOPOVER.1)
            } else {
                self.rng.range(REST.0, REST.1)
            };
        }
    }

    /// Si `other` está adelante, en el camino del caballo que va hacia
    /// `direction`.
    fn ahead(&self, direction: Vec2, other: &Body) -> bool {
        let rel = other.position - self.position;
        let along = rel.dot(direction);
        let across = (rel.x * direction.y - rel.y * direction.x).abs();
        let reach = self.collider.radius + other.collider.radius;
        // Más la distancia que necesita para frenar a la velocidad que lleva.
        let braking = self.speed * self.speed / (2.0 * ACCEL);
        along > 0.0 && along < reach + LOOK_AHEAD + braking && across < reach + 0.2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MAX_STEP;

    fn roads() -> RoadNetwork {
        crate::road::tests::shipped_roads().2
    }

    fn between(roads: &RoadNetwork) -> usize {
        roads
            .roads
            .iter()
            .filter(|r| r.kind == RoadKind::Between)
            .count()
    }

    fn riders(travellers: &Travellers) -> Vec<Traveller> {
        travellers
            .riders
            .iter()
            .filter(|t| t.vehicle.is_none())
            .cloned()
            .collect()
    }

    #[test]
    fn two_travellers_ride_each_road_between_towns_from_either_end() {
        let roads = roads();
        let travellers = Travellers::on(&roads, &[], 5);
        assert!(between(&roads) > 0);
        let riders = riders(&travellers);
        assert_eq!(riders.len(), PER_ROAD * between(&roads));
        for pair in riders.chunks(PER_ROAD) {
            assert_eq!(roads.roads[pair[0].road].kind, RoadKind::Between);
            assert_eq!(pair[0].road, pair[1].road);
            assert_ne!(pair[0].forward, pair[1].forward);
        }
        for t in &travellers.riders {
            assert!(
                roads.distance(t.position) < RoadNetwork::HALF_WIDTH,
                "off the road"
            );
        }
        assert_eq!(travellers, Travellers::on(&roads, &[], 5));
        assert_ne!(travellers, Travellers::on(&roads, &[], 6));
    }

    #[test]
    fn one_vehicle_sets_off_on_each_road_between_towns() {
        let roads = roads();
        let mut travellers = Travellers::on(&roads, &[], 5);
        let carts: Vec<Traveller> = travellers
            .riders
            .iter()
            .filter(|t| t.vehicle.is_some())
            .cloned()
            .collect();
        assert_eq!(carts.len(), between(&roads));
        let roads_with_carts: std::collections::BTreeSet<usize> =
            carts.iter().map(|c| c.road).collect();
        assert_eq!(roads_with_carts.len(), carts.len(), "one per road");
        for c in &carts {
            assert_eq!(roads.roads[c.road].kind, RoadKind::Between);
            assert!(roads.distance(c.position) < RoadNetwork::HALF_WIDTH);
        }
        // Al paso, nunca al trote.
        for _ in 0..(30.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[], MAX_STEP);
            for c in travellers.riders.iter().filter(|t| t.vehicle.is_some()) {
                assert!(c.speed <= WALK.1 + 1e-4, "{}", c.speed);
                assert_ne!(c.animation(), AnimationState::Gallop);
            }
        }
    }

    #[test]
    fn a_cart_counts_the_distance_it_rolls() {
        let roads = roads();
        let mut travellers = Travellers::on(&roads, &[], 5);
        travellers.riders.retain(|t| t.vehicle.is_some());
        travellers.riders.truncate(1);
        let mut rolled = 0.0;
        for _ in 0..(20.0 / MAX_STEP) as usize {
            let before = travellers.riders[0].along;
            travellers.step(&roads, &[], &[], MAX_STEP);
            rolled += (travellers.riders[0].along - before).abs();
        }
        let cart = &travellers.riders[0];
        assert!(rolled > 10.0, "{rolled}");
        assert!((cart.travelled - rolled).abs() < 1e-3, "{}", cart.travelled);
    }

    #[test]
    fn a_traveller_reaches_the_town_rests_and_rides_back() {
        let roads = roads();
        let mut travellers = Travellers::on(&roads, &[], 5);
        travellers.riders.truncate(1);
        let start = travellers.riders[0].clone();
        let length = roads.length(start.road);
        // Lo bastante para llegar a la punta al paso más lento.
        let mut arrived = None;
        let mut steps = 0;
        while arrived.is_none() {
            travellers.step(&roads, &[], &[], MAX_STEP);
            let t = &travellers.riders[0];
            assert!(
                roads.distance(t.position) < RoadNetwork::HALF_WIDTH,
                "off the road"
            );
            if t.along == 0.0 || t.along == length {
                arrived = Some(t.along);
            }
            steps += 1;
            assert!(
                (steps as f32) * MAX_STEP < length / WALK.0 + 10.0,
                "never arrived"
            );
        }
        let end = if start.forward { length } else { 0.0 };
        assert_eq!(arrived, Some(end));
        // Descansa quieto en la punta y después vuelve.
        travellers.step(&roads, &[], &[], MAX_STEP);
        assert_eq!(travellers.riders[0].speed, 0.0);
        for _ in 0..((REST.1 + 10.0) / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[], MAX_STEP);
        }
        let t = &travellers.riders[0];
        assert_ne!(t.forward, start.forward);
        assert!((t.along - end).abs() > 5.0, "still at the town");
    }

    #[test]
    fn a_traveller_waits_at_a_level_crossing_until_the_train_has_gone() {
        let roads = crate::road::tests::crossing_world().2;
        let mut travellers = Travellers::on(&roads, &[], 5);
        let (road, crossing) = roads
            .roads
            .iter()
            .enumerate()
            .find_map(|(i, r)| {
                (r.kind == RoadKind::Between)
                    .then(|| r.crossings.first().copied())
                    .flatten()
                    .map(|c| (i, c))
            })
            .expect("a road between towns crosses the line");
        travellers.riders.retain(|t| t.road == road && t.forward);
        travellers.riders.truncate(1);
        // A 60 m del paso, yendo hacia él, con un vagón parado sobre la vía.
        travellers.riders[0].along = crossing.along - 60.0;
        let wagon = SideObstacle {
            footprint: crate::Aabb::from_center_size(
                crossing.at + crossing.rail * 20.0,
                Vec2::new(3.0, 3.0),
            ),
            across: Vec2::new(-crossing.rail.y, crossing.rail.x),
        };
        for _ in 0..(60.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[wagon], MAX_STEP);
            assert!(travellers.riders[0].along <= crossing.along - CROSSING_HOLD + 1e-3);
        }
        assert_eq!(travellers.riders[0].speed, 0.0);
        assert!(
            travellers.riders[0].along > crossing.along - CROSSING_HOLD - 1.0,
            "stopped short"
        );
        // El tren se fue: cruza.
        for _ in 0..(30.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[], MAX_STEP);
        }
        assert!(travellers.riders[0].along > crossing.along + CROSSING_HOLD);
        // Ya sobre el paso cuando llega un tren: sigue de largo, no se queda
        // en la vía.
        travellers.riders[0].along = crossing.along;
        for _ in 0..(20.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[wagon], MAX_STEP);
        }
        assert!(travellers.riders[0].along > crossing.along + CROSSING_HOLD);
    }

    #[test]
    fn a_traveller_reins_in_then_passes_someone_standing_in_the_way() {
        let roads = roads();
        let mut travellers = Travellers::on(&roads, &[], 5);
        travellers.riders.truncate(1);
        let t = travellers.riders[0].clone();
        let toward = t.heading.forward();
        let person = Body {
            position: t.position + toward * 4.0,
            collider: Circle::new(Rider::COLLIDER_RADIUS),
        };
        let reach = t.collider.radius + person.collider.radius - 1e-3;
        let mut stopped = false;
        for _ in 0..(20.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[person], &[], MAX_STEP);
            let t = &travellers.riders[0];
            assert!(t.position.distance(person.position) >= reach);
            stopped |= t.speed == 0.0;
        }
        assert!(stopped, "never reined in");
        let passed = travellers.riders[0].position - person.position;
        assert!(passed.dot(toward) > 0.0, "never got past");
    }

    /// Entradas de estancia: una al costado de cada camino rural.
    fn estancias(roads: &RoadNetwork) -> Vec<Vec2> {
        roads
            .roads
            .iter()
            .filter(|r| r.kind == RoadKind::Rural && r.path.len() >= 2)
            .map(|r| {
                let (a, b) = (r.path[r.path.len() / 2 - 1], r.path[r.path.len() / 2]);
                let d = (b - a) * (1.0 / a.distance(b).max(1e-3));
                (a + b) * 0.5 + Vec2::new(-d.y, d.x) * 30.0
            })
            .collect()
    }

    /// La punta de un camino donde está el punto `along`, si es una: el
    /// pueblo.
    fn town_at(roads: &RoadNetwork, road: usize, along: f32) -> Option<usize> {
        let r = &roads.roads[road];
        if along < 1e-3 {
            Some(r.from)
        } else if (along - roads.length(road)).abs() < 1e-3 && r.kind == RoadKind::Between {
            Some(r.to)
        } else {
            None
        }
    }

    #[test]
    fn a_vehicle_travels_between_estancias_pulperias_and_stores_along_the_roads() {
        let roads = roads();
        let estancias = estancias(&roads);
        assert!(!estancias.is_empty());
        let travellers = Travellers::on(&roads, &estancias, 5);
        let vehicles: Vec<&Traveller> = travellers
            .riders
            .iter()
            .filter(|t| t.vehicle.is_some())
            .collect();
        assert!(
            vehicles.iter().all(|t| t.route.is_some()),
            "every vehicle is on a journey"
        );
        let mut kinds = std::collections::BTreeSet::new();
        for t in &vehicles {
            let route = t.route.as_ref().unwrap();
            assert_ne!(route.origin, route.destination);
            let (first, last) = (route.legs[0], *route.legs.last().unwrap());
            let at = |stop: Stop, leg: Leg, along: f32| match stop {
                Stop::Store(town) => {
                    assert_eq!(town_at(&roads, leg.road, along), Some(town), "{route:?}")
                }
                Stop::Estancia(i) => {
                    let p = point_at(&roads.roads[leg.road].path, along).0;
                    assert!(p.distance(estancias[i]) < ESTANCIA_REACH, "{route:?}");
                }
            };
            at(route.origin, first, first.from);
            at(route.destination, last, last.to);
            for pair in route.legs.windows(2) {
                let end = town_at(&roads, pair[0].road, pair[0].to);
                assert!(
                    end.is_some() && end == town_at(&roads, pair[1].road, pair[1].from),
                    "{route:?}"
                );
            }
            kinds.insert(
                matches!(route.origin, Stop::Estancia(_)) as u8 * 2
                    + matches!(route.destination, Stop::Estancia(_)) as u8,
            );
        }
        assert!(kinds.len() >= 2, "estancias and stores both: {kinds:?}");
    }

    #[test]
    fn a_vehicle_on_the_road_arrives_at_its_destination_and_turns_back() {
        let roads = roads();
        let estancias = estancias(&roads);
        let mut travellers = Travellers::on(&roads, &estancias, 5);
        travellers.riders.retain(|t| t.route.is_some());
        // El que tiene menos camino por delante.
        let left = |t: &Traveller| {
            let r = t.route.as_ref().unwrap();
            (t.along - r.legs[t.leg].to).abs()
                + r.legs[t.leg + 1..]
                    .iter()
                    .map(|l| (l.to - l.from).abs())
                    .sum::<f32>()
        };
        travellers
            .riders
            .sort_by(|a, b| left(a).total_cmp(&left(b)));
        travellers.riders.truncate(1);
        let destination = travellers.riders[0].route.as_ref().unwrap().destination;
        let seconds = left(&travellers.riders[0]) / WALK.0 + STOPOVER.1 * 4.0 + REST.1 + 30.0;
        let mut back = false;
        for _ in 0..(seconds / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[], MAX_STEP);
            let t = &travellers.riders[0];
            assert!(
                roads.distance(t.position) < LANE + DODGE + 0.1,
                "off the road at {:?}",
                t.position
            );
            if t.route.as_ref().unwrap().origin == destination {
                back = true;
                break;
            }
        }
        assert!(back, "it reached {destination:?} and set off back");
    }

    #[test]
    fn a_vehicle_crosses_a_town_to_its_next_road_only_out_of_sight() {
        let roads = roads();
        let estancias = estancias(&roads);
        let mut travellers = Travellers::on(&roads, &estancias, 5);
        let (index, _) = travellers
            .riders
            .iter()
            .enumerate()
            .find(|(_, t)| t.route.as_ref().is_some_and(|r| t.leg + 1 < r.legs.len()))
            .expect("a journey through a town");
        let mut t = travellers.riders.swap_remove(index);
        let leg = t.route.as_ref().unwrap().legs[t.leg];
        t.along = leg.to - if leg.forward() { 0.2 } else { -0.2 };
        t.place(&roads);
        let mut travellers = Travellers { riders: vec![t] };
        let at = travellers.riders[0].leg;
        let player = |p: Vec2| Body {
            position: p + Vec2::new(30.0, 0.0),
            collider: Circle::new(Rider::COLLIDER_RADIUS),
        };
        let watcher = player(travellers.riders[0].position);
        for _ in 0..(STOPOVER.1 * 3.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[watcher], &[], MAX_STEP);
        }
        assert_eq!(
            travellers.riders[0].leg, at,
            "no hop while the player looks on"
        );
        for _ in 0..(2.0 / MAX_STEP) as usize {
            travellers.step(&roads, &[], &[], MAX_STEP);
        }
        assert_eq!(
            travellers.riders[0].leg,
            at + 1,
            "out of sight, on to its next road"
        );
    }

    #[test]
    fn carts_and_sulkies_fill_the_roads_and_one_rare_ox_carreta_leaves_an_estancia() {
        let roads = roads();
        let estancias = estancias(&roads);
        let travellers = Travellers::on(&roads, &estancias, 5);
        let of = |v: Vehicle| {
            travellers
                .riders
                .iter()
                .filter(|t| t.vehicle == Some(v))
                .count()
        };
        assert!(of(Vehicle::Cart) > 0 && of(Vehicle::Sulky) > 0);
        assert_eq!(of(Vehicle::Carreta), 1);
        let carreta = travellers
            .riders
            .iter()
            .find(|t| t.vehicle == Some(Vehicle::Carreta))
            .unwrap();
        let route = carreta.route.as_ref().unwrap();
        assert!(
            matches!(route.origin, Stop::Estancia(_))
                || matches!(route.destination, Stop::Estancia(_))
        );
        assert_eq!(
            roads.roads[route.legs[0].road].kind,
            RoadKind::Rural,
            "on its rural track"
        );
        assert_eq!(
            Travellers::on(&roads, &[], 5)
                .riders
                .iter()
                .filter(|t| t.vehicle == Some(Vehicle::Carreta))
                .count(),
            0
        );
    }
}
