//! Jinetes del pueblo: vecinos a caballo que recorren las calles al paso, de
//! intersección en intersección.
//!
//! Deterministas para una semilla dada. Usan el mismo [`StreetGraph`] que los
//! peatones, pero por la calzada, y no se detienen en las esquinas: al llegar a
//! una eligen la siguiente calle (evitando volver por donde vinieron cuando
//! pueden). En el pueblo van siempre al paso (usuario, 2026-10-29). Van por
//! la mano izquierda de la calzada, como se circulaba en la Argentina hasta
//! 1945, y frenan antes que atropellar a alguien que tienen adelante. Los
//! sulkies ([`Vehicle::Sulky`], [`TownRiders::with_sulkies`]) andan igual, con
//! la caja detrás del caballo.

use crate::rng::Rng;
use crate::townsfolk::StreetGraph;
use crate::{
    AnimationState, Body, Circle, Heading, Rider, RiderParams, TownLayout, Vec2, Vehicle, World,
};

/// Un vecino a caballo.
#[derive(Debug, Clone, PartialEq)]
pub struct TownRider {
    pub position: Vec2,
    pub heading: Heading,
    /// Velocidad actual sobre el suelo, en m/s.
    pub speed: f32,
    pub collider: Circle,
    /// Variación de apariencia del jinete; el render decide cómo se ve.
    pub look: u8,
    /// Semilla del pelaje del caballo; el render decide cómo se ve.
    pub coat: u32,
    /// El vehículo que lleva (un sulky), o nada si va montado.
    pub vehicle: Option<Vehicle>,
    /// Metros que lleva recorridos: las ruedas de un sulky giran con ellos.
    pub travelled: f32,
    /// Ritmo de marcha propio, al paso, en m/s.
    pace: f32,
    from: usize,
    to: usize,
    /// Segundos que lleva frenado o casi sin avanzar.
    stuck: f32,
    rng: Rng,
}

impl TownRider {
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

    /// Todo lo que ocupa: el caballo, o el caballo y el sulky.
    pub fn bodies(&self) -> Vec<Body> {
        match self.vehicle {
            Some(vehicle) => vehicle.bodies(self.position, self.heading),
            None => vec![self.body()],
        }
    }

    /// Índice de la intersección hacia la que va.
    pub fn next_waypoint(&self) -> usize {
        self.to
    }
}

/// Todos los jinetes de un pueblo, más el grafo de calles que siguen.
#[derive(Debug, Clone, PartialEq)]
pub struct TownRiders {
    pub graph: StreetGraph,
    pub riders: Vec<TownRider>,
}

/// Distancia de la línea central de la calle a la que va un jinete, a su
/// izquierda, en metros: dentro de la calzada de 5 m de las calles de 12 m.
pub const LANE: f32 = 1.2;
/// Ritmos al paso (un caballo camina a unos 1,7 m/s), en m/s.
pub const WALK: (f32, f32) = (1.4, 1.9);
/// Aceleración y frenada, en m/s².
const ACCEL: f32 = 2.0;
/// Cuánto mira hacia adelante un jinete para frenar, más allá de los cuerpos
/// que se tocarían, en metros.
const LOOK_AHEAD: f32 = 3.0;
/// Giro máximo de la orientación, en radianes por segundo.
const TURN_RATE: f32 = 2.5;
/// Segundos frenado o trabado antes de dar media vuelta.
const GIVE_UP: f32 = 6.0;
/// Lo que se mezcla con la semilla de los jinetes para la de los sulkies.
const SULKY_SEED: u32 = 0x7375_6c6b;

impl TownRiders {
    /// `count` jinetes repartidos por las calles de `town`, ubicados a partir
    /// de `seed`.
    pub fn new(town: &TownLayout, count: usize, seed: u32) -> Self {
        Self::from_graph(StreetGraph::from_town(town), count, seed)
    }

    /// `count` jinetes repartidos por `graph`, ubicados a partir de `seed`. Sin
    /// intersecciones no aparece nadie; uno ubicado en una intersección aislada
    /// se queda ahí.
    pub fn from_graph(graph: StreetGraph, count: usize, seed: u32) -> Self {
        let riders = if graph.nodes.is_empty() {
            Vec::new()
        } else {
            (0..count)
                .map(|i| TownRider::spawn(&graph, Rng::new(seed, i as u32)))
                .collect()
        };
        Self { graph, riders }
    }

    /// Hace avanzar a todos `dt` segundos dentro de `world`, frenando ante
    /// `obstacles` (el jugador, los peatones) y ante los otros jinetes. Un `dt`
    /// no finito o no positivo no cambia nada.
    pub fn step_around(&mut self, world: &World, obstacles: &[Body], dt: f32) {
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
            self.riders[index].step(&self.graph, world, &others, dt);
        }
    }

    /// Agrega `count` sulkies, ubicados a partir de `seed` (con su propia
    /// semilla: los jinetes no cambian), detrás de los jinetes.
    pub fn with_sulkies(mut self, count: usize, seed: u32) -> Self {
        if !self.graph.nodes.is_empty() {
            for i in 0..count {
                let mut sulky =
                    TownRider::spawn(&self.graph, Rng::new(seed ^ SULKY_SEED, i as u32));
                sulky.vehicle = Some(Vehicle::Sulky);
                sulky.collider = Vehicle::Sulky.collider();
                self.riders.push(sulky);
            }
        }
        self
    }

    /// Dónde está cada caballo y cada sulky, como obstáculo para los demás.
    pub fn bodies(&self) -> Vec<Body> {
        self.riders.iter().flat_map(TownRider::bodies).collect()
    }
}

/// El punto de la mano izquierda de la calle `from`→`to` a la altura de `to`.
fn lane_target(graph: &StreetGraph, from: usize, to: usize) -> Vec2 {
    let (a, b) = (graph.nodes[from], graph.nodes[to]);
    let along = b - a;
    let length = along.length();
    if length < 1e-3 {
        return b;
    }
    let left = Vec2::new(-along.y, along.x) * (LANE / length);
    b + left
}

/// El ángulo más corto de `from` a `to`, en `(-PI, PI]`.
fn turn_between(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (to - from).rem_euclid(TAU);
    if d > PI { d - TAU } else { d }
}

impl TownRider {
    fn spawn(graph: &StreetGraph, mut rng: Rng) -> Self {
        let from = rng.below(graph.nodes.len());
        let links = &graph.links[from];
        let to = if links.is_empty() {
            from
        } else {
            links[rng.below(links.len())]
        };
        let (a, b) = (graph.nodes[from], graph.nodes[to]);
        let left = lane_target(graph, from, to) - b;
        let position = a + (b - a) * rng.range(0.1, 0.9) + left;
        let pace = rng.range(WALK.0, WALK.1);
        Self {
            position,
            heading: Heading::from_radians((b.y - a.y).atan2(b.x - a.x)),
            speed: 0.0,
            collider: Circle::new(Rider::COLLIDER_RADIUS),
            look: (rng.next() >> 24) as u8,
            coat: rng.next(),
            vehicle: None,
            travelled: 0.0,
            pace,
            from,
            to,
            stuck: 0.0,
            rng,
        }
    }

    fn step(&mut self, graph: &StreetGraph, world: &World, obstacles: &[Body], dt: f32) {
        if self.from == self.to {
            self.speed = 0.0;
            return;
        }
        let target = lane_target(graph, self.from, self.to);
        let offset = target - self.position;
        let distance = offset.length();
        let direction = if distance > 1e-4 {
            offset * (1.0 / distance)
        } else {
            self.heading.forward()
        };
        let blocked = obstacles.iter().any(|o| self.ahead(direction, o));
        let wanted = if blocked { 0.0 } else { self.pace };
        self.speed = if self.speed < wanted {
            (self.speed + ACCEL * dt).min(wanted)
        } else {
            (self.speed - ACCEL * dt).max(wanted)
        };

        let turn = turn_between(self.heading.radians(), direction.y.atan2(direction.x));
        let limit = TURN_RATE * dt;
        self.heading = self.heading.rotated(turn.clamp(-limit, limit));

        let stride = self.speed * dt;
        let start = self.position;
        if distance <= stride {
            self.position = target;
            self.arrive(graph);
        } else {
            self.position += direction * stride;
        }
        self.position = world.resolve_among(self.collider, self.position, obstacles, |_| ());

        // Frenado ante alguien que no se corre, o trabado contra algo: media
        // vuelta.
        let moved = self.position.distance(start);
        self.travelled += moved;
        if blocked || moved < 0.25 * self.pace * dt {
            self.stuck += dt;
            if self.stuck > GIVE_UP {
                std::mem::swap(&mut self.from, &mut self.to);
                self.stuck = 0.0;
            }
        } else {
            self.stuck = 0.0;
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

    /// Llegó a `to`: sigue por otra calle, sin volver si puede evitarlo.
    fn arrive(&mut self, graph: &StreetGraph) {
        let came_from = self.from;
        self.from = self.to;
        let links = &graph.links[self.from];
        let onward: Vec<usize> = links.iter().copied().filter(|&n| n != came_from).collect();
        let choices = if onward.is_empty() { links } else { &onward };
        self.to = if choices.is_empty() {
            self.from
        } else {
            choices[self.rng.below(choices.len())]
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAX_STEP, Terrain};

    fn town() -> (TownLayout, World) {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        (town, world)
    }

    #[test]
    fn riders_keep_to_the_streets_and_ride_on() {
        let (town, world) = town();
        let mut riders = TownRiders::new(&town, 4, 3);
        let start: Vec<Vec2> = riders.riders.iter().map(|r| r.position).collect();
        let mut moving = false;
        for _ in 0..(90.0 / MAX_STEP) as usize {
            riders.step_around(&world, &[], MAX_STEP);
            for r in &riders.riders {
                assert!(
                    town.streets.iter().any(|s| s.contains_point(r.position)),
                    "off the street at {:?}",
                    r.position
                );
                moving |= r.animation() != AnimationState::Idle;
                // Al paso, nunca al trote ni al galope.
                assert!(r.speed <= WALK.1 + 1e-4);
                assert_ne!(r.animation(), AnimationState::Gallop);
            }
        }
        assert!(moving);
        for (r, s) in riders.riders.iter().zip(start) {
            assert!(r.position.distance(s) > 20.0, "stayed near {s:?}");
        }
    }

    #[test]
    fn a_rider_reins_in_before_someone_in_the_way() {
        let (town, world) = town();
        let mut riders = TownRiders::new(&town, 1, 3);
        // Alguien parado en la mano del jinete, unos metros adelante.
        let rider = &riders.riders[0];
        let target = lane_target(&riders.graph, rider.from, rider.to);
        let ahead =
            rider.position + (target - rider.position) * (3.0 / rider.position.distance(target));
        let person = Body {
            position: ahead,
            collider: Circle::new(crate::Townsperson::COLLIDER_RADIUS),
        };
        for _ in 0..(4.0 / MAX_STEP) as usize {
            riders.step_around(&world, &[person], MAX_STEP);
            let r = &riders.riders[0];
            assert!(
                r.position.distance(ahead) >= r.collider.radius + person.collider.radius - 1e-3
            );
        }
        assert_eq!(riders.riders[0].speed, 0.0);
    }

    #[test]
    fn sulkies_walk_the_streets_after_the_riders() {
        let (town, world) = town();
        let riders = TownRiders::new(&town, 2, 3);
        let mut all = riders.clone().with_sulkies(2, 3);
        assert_eq!(all.riders.len(), 4);
        assert_eq!(
            all.riders[..2],
            riders.riders[..],
            "the riders stay as they were"
        );
        assert!(
            all.riders[2..]
                .iter()
                .all(|r| r.vehicle == Some(Vehicle::Sulky))
        );
        assert_eq!(all, riders.with_sulkies(2, 3));
        // El sulky ocupa su largo: el caballo y el asiento.
        assert_eq!(all.bodies().len(), 2 + 2 * Vehicle::Sulky.footprint().len());
        let start: Vec<Vec2> = all.riders.iter().map(|r| r.position).collect();
        let mut rolled = [0.0f32; 2];
        for _ in 0..(90.0 / MAX_STEP) as usize {
            let before: Vec<Vec2> = all.riders[2..].iter().map(|r| r.position).collect();
            all.step_around(&world, &[], MAX_STEP);
            for (k, s) in all.riders[2..].iter().enumerate() {
                assert!(
                    town.streets.iter().any(|st| st.contains_point(s.position)),
                    "off the street at {:?}",
                    s.position
                );
                assert!(s.speed <= WALK.1 + 1e-4, "only at a walk");
                rolled[k] += s.position.distance(before[k]);
            }
        }
        for (k, s) in all.riders[2..].iter().enumerate() {
            assert!(s.position.distance(start[2 + k]) > 20.0, "stayed put");
            assert!(
                (s.travelled - rolled[k]).abs() < 1e-2,
                "{} {}",
                s.travelled,
                rolled[k]
            );
        }
    }

    #[test]
    fn the_crowd_of_riders_comes_from_the_seed() {
        let (town, _) = town();
        assert_eq!(TownRiders::new(&town, 3, 9), TownRiders::new(&town, 3, 9));
        assert_ne!(TownRiders::new(&town, 3, 9), TownRiders::new(&town, 3, 10));
        assert_eq!(TownRiders::new(&town, 3, 9).riders.len(), 3);
    }
}
