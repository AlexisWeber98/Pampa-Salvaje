//! Ganado: un rodeo que pasta en su potrero (ver [`super`]).

use std::f32::consts::TAU;

use super::turn_toward;
use crate::rng::Rng;
use crate::{Aabb, Body, Circle, Heading, StaticObstacles, Vec2};

/// El pelaje de una vaca, según la raza.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CowCoat {
    /// Hereford: colorada con la cara, el pecho y la panza blancos.
    Hereford,
    /// Aberdeen Angus: negra entera.
    Angus,
    /// Overo: blanco con manchas negras.
    Overo,
}

impl CowCoat {
    pub const ALL: [Self; 3] = [Self::Hereford, Self::Angus, Self::Overo];

    /// La raza de un rodeo: sobre todo Hereford, después Angus y algunos
    /// overos.
    fn seeded(rng: &mut Rng) -> Self {
        match rng.unit() {
            u if u < 0.45 => Self::Hereford,
            u if u < 0.8 => Self::Angus,
            _ => Self::Overo,
        }
    }
}

/// Lo que está haciendo una vaca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CowState {
    /// Con la cabeza agachada en el pasto.
    Graze,
    /// Parada, con la cabeza levantada, rumiando.
    Stand,
    /// Caminando despacio hacia algún lado.
    Walk,
}

/// Una cabeza de ganado.
#[derive(Debug, Clone, PartialEq)]
pub struct Cow {
    pub position: Vec2,
    pub heading: Heading,
    pub state: CowState,
    /// Velocidad sobre el suelo, en m/s.
    pub speed: f32,
    pub coat: CowCoat,
    /// El toro del rodeo: se dibuja más grande.
    pub bull: bool,
    target: Vec2,
    /// Segundos que le quedan en este estado.
    timer: f32,
    rng: Rng,
}

/// Un rodeo y el potrero en el que se queda.
#[derive(Debug, Clone, PartialEq)]
pub struct Herd {
    /// La celda del relevamiento de su campo, como índice: estable entre
    /// ejecuciones.
    pub id: u32,
    pub pasture: Aabb,
    pub cows: Vec<Cow>,
}

impl Herd {
    /// El radio del cuerpo de una vaca, en metros.
    pub const RADIUS: f32 = 0.8;
    /// Caminata mientras pasta, en m/s.
    pub const WALK: f32 = 0.5;
    /// Caminata para alejarse de un jinete, en m/s.
    pub const SHY_WALK: f32 = 1.3;
    /// Un jinete más cerca que esto hace que una vaca parada se aleje
    /// caminando, en metros.
    pub const SHY: f32 = 7.0;
    /// Giro más rápido, en radianes por segundo.
    pub const TURN: f32 = 0.9;
    /// Lo más lejos que se aleja una vaca del centro del rodeo, en metros.
    pub const SPREAD: f32 = 22.0;
    /// Mínimo de cabezas que debe tener un rodeo para llevar un toro.
    pub const BULL_HERD: usize = 10;

    /// Un rodeo de `count` cabezas agrupadas en `pasture`, con semilla.
    pub fn new(id: u32, pasture: Aabb, count: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed, id);
        let breed = CowCoat::seeded(&mut rng);
        let inner = inner(pasture);
        let middle = Vec2::new(
            rng.range(inner.min.x, inner.max.x),
            rng.range(inner.min.y, inner.max.y),
        );
        let mut cows: Vec<Cow> = (0..count)
            .map(|k| {
                let mut rng = Rng::new(seed ^ 0xc0e5, k as u32);
                let at = inner.closest_point(
                    middle + Vec2::new(rng.range(-15.0, 15.0), rng.range(-15.0, 15.0)),
                );
                // Una de cada ocho es de otra raza: los rodeos eran mezclados.
                let coat = if rng.unit() < 0.125 {
                    CowCoat::seeded(&mut rng)
                } else {
                    breed
                };
                let state = [CowState::Graze, CowState::Stand][rng.below(2)];
                Cow {
                    position: at,
                    heading: Heading::from_radians(rng.unit() * TAU),
                    state,
                    speed: 0.0,
                    coat,
                    bull: false,
                    target: at,
                    timer: rng.range(1.0, 20.0),
                    rng,
                }
            })
            .collect();
        // Un rodeo lo bastante grande lleva un toro de su raza.
        if count >= Self::BULL_HERD {
            let bull = &mut cows[rng.below(count)];
            bull.bull = true;
            bull.coat = breed;
        }
        Self { id, pasture, cows }
    }

    /// El centro del rodeo.
    pub fn center(&self) -> Vec2 {
        let n = self.cows.len().max(1) as f32;
        self.cows.iter().fold(Vec2::ZERO, |a, c| a + c.position) * (1.0 / n)
    }

    /// Cada vaca como un cuerpo blando que bloquea al jinete.
    pub fn bodies(&self) -> impl Iterator<Item = Body> + '_ {
        self.cows.iter().map(|c| Body {
            position: c.position,
            collider: Circle::new(Self::RADIUS),
        })
    }

    /// Hace avanzar el rodeo `dt` segundos: cada vaca pasta, camina o se queda
    /// parada; un jinete en `threat` hace que las vacas se alejen caminando.
    /// Las vacas se mantienen separadas, fuera de `obstacles` y dentro del
    /// potrero.
    pub fn step(&mut self, dt: f32, threat: Option<Vec2>, obstacles: &StaticObstacles) {
        let center = self.center();
        let inner = inner(self.pasture);
        let before: Vec<Vec2> = self.cows.iter().map(|c| c.position).collect();
        for cow in &mut self.cows {
            cow.step(dt, center, inner, threat);
        }
        self.keep_apart();
        for (cow, before) in self.cows.iter_mut().zip(before) {
            cow.position = settle(cow.position, inner, obstacles).unwrap_or(before);
        }
    }

    /// Separa las vacas que se superponen, la mitad cada una.
    fn keep_apart(&mut self) {
        let gap = 2.0 * Self::RADIUS;
        for i in 0..self.cows.len() {
            for j in i + 1..self.cows.len() {
                let d = self.cows[j].position - self.cows[i].position;
                let len = d.length();
                if len < gap && len > 1e-4 {
                    let push = d * ((gap - len) * 0.5 / len);
                    self.cows[i].position = self.cows[i].position - push;
                    self.cows[j].position += push;
                }
            }
        }
    }
}

/// Dónde termina una vaca que está en `at` de modo que quede a la vez dentro de
/// `inner` y lejos de todo obstáculo: se ajusta a los límites y después se
/// empuja hacia afuera, en algunas pasadas (un árbol en el borde del potrero la
/// empuja fuera del rectángulo, y el rectángulo la devuelve contra el árbol).
/// `None` cuando ninguna pasada cumple ambas condiciones: la vaca se queda
/// entonces donde estaba.
fn settle(at: Vec2, inner: Aabb, obstacles: &StaticObstacles) -> Option<Vec2> {
    let circle = Circle::new(Herd::RADIUS);
    let mut at = inner.closest_point(at);
    for _ in 0..4 {
        let area = Aabb::from_center_size(at, Vec2::new(4.0, 4.0));
        let mut pushed = false;
        for item in obstacles.near(area) {
            if let Some(out) = circle.push_out(at, item) {
                at = out;
                pushed = true;
            }
        }
        if !pushed {
            return Some(at);
        }
        at = inner.closest_point(at);
    }
    let area = Aabb::from_center_size(at, Vec2::new(4.0, 4.0));
    let clear = obstacles
        .near(area)
        .iter()
        .all(|item| circle.push_out(at, item).is_none());
    clear.then_some(at)
}

/// El potrero menos el radio de una vaca: donde puede estar su centro.
fn inner(pasture: Aabb) -> Aabb {
    pasture.expanded(-Herd::RADIUS)
}

impl Cow {
    fn step(&mut self, dt: f32, center: Vec2, inner: Aabb, threat: Option<Vec2>) {
        self.timer -= dt;
        if let Some(rider) = threat {
            let away = self.position - rider;
            let d = away.length();
            if d < Herd::SHY {
                let dir = away * (1.0 / d.max(1e-3));
                self.state = CowState::Walk;
                self.target = inner.closest_point(self.position + dir * 10.0);
                self.timer = 4.0;
                self.speed = Herd::SHY_WALK;
            }
        }
        if self.state == CowState::Walk {
            let to = self.target - self.position;
            let turn = turn_toward(self.heading, to);
            let max = Herd::TURN * dt;
            self.heading = self.heading.rotated(turn.clamp(-max, max));
            // Primero gira en el lugar y después sigue caminando.
            let pace = if turn.abs() < 1.0 { self.speed } else { 0.2 };
            self.position += self.heading.forward() * (pace * dt);
            if to.length() < 1.0 {
                self.timer = 0.0;
            }
        }
        if self.timer <= 0.0 {
            self.choose(center, inner);
        }
    }

    /// Lo próximo que va a hacer, al azar.
    fn choose(&mut self, center: Vec2, inner: Aabb) {
        let r = &mut self.rng;
        let (state, timer) = match r.unit() {
            u if u < 0.58 => (CowState::Graze, r.range(8.0, 20.0)),
            u if u < 0.86 => (CowState::Walk, r.range(15.0, 30.0)),
            _ => (CowState::Stand, r.range(3.0, 8.0)),
        };
        if state == CowState::Walk {
            let s = Herd::SPREAD;
            self.target = inner.closest_point(center + Vec2::new(r.range(-s, s), r.range(-s, s)));
            self.speed = Herd::WALK * r.range(0.8, 1.2);
        } else {
            self.speed = 0.0;
        }
        self.state = state;
        self.timer = timer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn herd() -> Herd {
        let pasture = Aabb::new(Vec2::new(100.0, 100.0), Vec2::new(200.0, 180.0));
        Herd::new(7, pasture, 12, 99)
    }

    fn run(herd: &mut Herd, seconds: f32, threat: Option<Vec2>) {
        let none = StaticObstacles::default();
        for _ in 0..(seconds / 0.1) as usize {
            herd.step(0.1, threat, &none);
        }
    }

    #[test]
    fn a_herd_is_seeded_and_steps_deterministically() {
        let (mut a, mut b) = (herd(), herd());
        assert_eq!(a, b);
        run(&mut a, 60.0, None);
        run(&mut b, 60.0, None);
        assert_eq!(a, b);
        assert_ne!(a, herd(), "they moved");
    }

    #[test]
    fn cows_wander_slowly_inside_the_pasture_and_keep_apart() {
        let mut h = herd();
        let mut states = std::collections::HashSet::new();
        for _ in 0..600 {
            let before: Vec<Vec2> = h.cows.iter().map(|c| c.position).collect();
            run(&mut h, 1.0, None);
            for (c, b) in h.cows.iter().zip(before) {
                assert!(h.pasture.contains_point(c.position));
                assert!(c.position.distance(b) <= 2.0 * Herd::SHY_WALK + 0.5);
                states.insert(c.state);
            }
        }
        assert_eq!(states.len(), 3, "graze, stand and walk: {states:?}");
        for (i, a) in h.cows.iter().enumerate() {
            for b in &h.cows[i + 1..] {
                assert!(a.position.distance(b.position) > Herd::RADIUS, "overlap");
            }
        }
    }

    #[test]
    fn a_rider_walks_cows_off() {
        let mut h = herd();
        for cow in &mut h.cows {
            cow.state = CowState::Graze;
            cow.timer = 1000.0;
        }
        let at = h.cows[0].position + Vec2::new(2.0, 0.0);
        let far = h.cows[0].position + Vec2::new(500.0, 0.0);
        run(&mut h, 0.5, Some(far));
        assert_eq!(h.cows[0].state, CowState::Graze, "a far rider leaves it be");
        run(&mut h, 0.5, Some(at));
        assert_eq!(h.cows[0].state, CowState::Walk);
        run(&mut h, 5.0, Some(at));
        assert!(h.cows[0].position.distance(at) > 3.0);
    }

    #[test]
    fn a_tree_on_the_pasture_edge_never_holds_a_cow_inside_it() {
        let mut h = herd();
        let inner = inner(h.pasture);
        // Un tronco a caballo del borde oeste, y todas las vacas caminando
        // hacia él.
        let tree = Aabb::from_center_size(
            Vec2::new(h.pasture.min.x, h.pasture.center().y),
            Vec2::new(1.2, 1.2),
        );
        let trees = StaticObstacles::new(vec![tree]);
        let clear = |c: Vec2| tree.closest_point(c).distance(c) >= Herd::RADIUS - 1e-3;
        for cow in &mut h.cows {
            cow.position = Vec2::new(inner.min.x + 3.0, tree.center().y);
            cow.state = CowState::Walk;
            cow.target = Vec2::new(inner.min.x, tree.center().y);
            cow.speed = Herd::SHY_WALK;
            cow.timer = 1000.0;
        }
        for _ in 0..400 {
            h.step(0.1, None, &trees);
            for c in &h.cows {
                assert!(clear(c.position), "in the tree: {:?}", c.position);
                assert!(h.pasture.contains_point(c.position));
            }
        }
    }

    #[test]
    fn cows_are_pushed_out_of_trees() {
        let mut h = herd();
        let tree = Aabb::from_center_size(h.cows[0].position, Vec2::new(0.8, 0.8));
        let trees = StaticObstacles::new(vec![tree]);
        h.step(0.1, None, &trees);
        let c = h.cows[0].position;
        assert!(tree.closest_point(c).distance(c) >= Herd::RADIUS - 1e-3);
    }

    #[test]
    fn a_herd_big_enough_has_one_bull_of_its_breed() {
        let pasture = Aabb::new(Vec2::new(100.0, 100.0), Vec2::new(200.0, 180.0));
        let small = Herd::new(3, pasture, Herd::BULL_HERD - 1, 5);
        assert!(
            small.cows.iter().all(|c| !c.bull),
            "too few head for a bull"
        );
        let mut bulls = std::collections::HashSet::new();
        for seed in 0..40 {
            let h = Herd::new(seed, pasture, Herd::BULL_HERD + seed as usize % 8, seed * 7);
            let again = Herd::new(seed, pasture, h.cows.len(), seed * 7);
            assert_eq!(h, again, "deterministic");
            let bull: Vec<usize> = (0..h.cows.len()).filter(|&k| h.cows[k].bull).collect();
            assert_eq!(bull.len(), 1, "one bull per herd");
            bulls.insert(bull[0]);
            let breed = h.cows.iter().filter(|c| !c.bull).map(|c| c.coat);
            let most = CowCoat::ALL
                .into_iter()
                .max_by_key(|&coat| breed.clone().filter(|&c| c == coat).count())
                .unwrap();
            assert_eq!(h.cows[bull[0]].coat, most, "the herd's breed");
        }
        assert!(bulls.len() > 3, "not always the same head: {bulls:?}");
    }
}
