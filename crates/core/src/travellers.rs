//! Viajeros: jinetes y carros que van de pueblo a pueblo por los caminos.
//!
//! Deterministas para una semilla dada. Cada camino entre dos pueblos
//! ([`RoadKind::Between`]) tiene dos jinetes, uno que sale de cada punta, y un
//! carro ([`Vehicle::Cart`]) que va al paso, con su propia semilla. Van
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

/// Jinetes por camino entre pueblos: uno que sale de cada punta.
pub const PER_ROAD: usize = 2;
/// Carros por camino entre pueblos.
pub const CARTS_PER_ROAD: usize = 1;
/// Lo que se mezcla con la semilla de los viajeros para la de los carros: los
/// jinetes quedan como sin carros.
const CART_SEED: u32 = 0x6361_7272;
/// Distancia de la línea central del camino a la que va un viajero, a su
/// izquierda, en metros: dentro de los 6,5 m del camino.
pub const LANE: f32 = 1.4;
/// Cuánto más se corre un viajero para pasar a alguien, en metros.
const DODGE: f32 = 2.5;
/// Ritmos al paso, en m/s.
const WALK: (f32, f32) = (1.5, 1.9);
/// Ritmos al trote, en m/s.
const TROT: (f32, f32) = (3.5, 4.5);
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
    /// carros por camino, en un tramo y un sentido sembrados.
    pub fn on(roads: &RoadNetwork, seed: u32) -> Self {
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
        for &road in &between {
            for k in 0..CARTS_PER_ROAD {
                let mut rng = Rng::new(seed ^ CART_SEED, (road * CARTS_PER_ROAD + k) as u32);
                let forward = rng.unit() < 0.5;
                riders.push(Traveller::spawn(
                    roads,
                    road,
                    forward,
                    Some(Vehicle::Cart),
                    rng,
                ));
            }
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
            self.riders[index].step(roads, &others, trains, dt);
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
        // Los carros van siempre al paso.
        let pace = if vehicle.is_none() && rng.unit() < TROTTERS {
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

    fn step(&mut self, roads: &RoadNetwork, obstacles: &[Body], trains: &[SideObstacle], dt: f32) {
        let length = roads.length(self.road);
        if self.rest > 0.0 {
            self.rest -= dt;
            self.speed = 0.0;
            if self.rest <= 0.0 {
                // Emprende la vuelta. Un carro no da media vuelta de golpe:
                // gira de a poco mientras arranca.
                self.forward = !self.forward;
                if self.vehicle.is_none() {
                    self.heading = self.heading.rotated(std::f32::consts::PI);
                }
                self.place(roads);
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
        let arrived = if self.forward {
            self.along >= length
        } else {
            self.along <= 0.0
        };
        self.along = self.along.clamp(0.0, length);
        self.travelled += (self.along - before).abs();
        let (_, direction) = self.place(roads);
        let turn = turn_between(self.heading.radians(), direction.y.atan2(direction.x));
        let limit = TURN_RATE * dt;
        self.heading = self.heading.rotated(turn.clamp(-limit, limit));
        if arrived {
            self.speed = 0.0;
            self.dodge = 0.0;
            self.rest = self.rng.range(REST.0, REST.1);
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
        let travellers = Travellers::on(&roads, 5);
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
        assert_eq!(travellers, Travellers::on(&roads, 5));
        assert_ne!(travellers, Travellers::on(&roads, 6));
    }

    #[test]
    fn one_cart_walks_each_road_between_towns() {
        let roads = roads();
        let mut travellers = Travellers::on(&roads, 5);
        let carts: Vec<Traveller> = travellers
            .riders
            .iter()
            .filter(|t| t.vehicle == Some(Vehicle::Cart))
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
        let mut travellers = Travellers::on(&roads, 5);
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
        let mut travellers = Travellers::on(&roads, 5);
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
        let mut travellers = Travellers::on(&roads, 5);
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
        let mut travellers = Travellers::on(&roads, 5);
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
}
