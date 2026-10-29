//! Animales silvestres: liebres, bandadas de teros, aves rapaces que planean y
//! patos (ver [`super`]). Ninguno de ellos es un obstáculo.

use std::f32::consts::{PI, TAU};

use super::turn_toward;
use crate::rng::Rng;
use crate::{Aabb, Heading, Lagoon, Vec2};

/// Lo que está haciendo una liebre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HareState {
    /// Sentada en el pasto.
    Sit,
    /// Unos pocos saltos lentos.
    Hop,
    /// Huyendo a toda velocidad de un jinete.
    Flee,
}

/// Una liebre en el pasto abierto.
#[derive(Debug, Clone, PartialEq)]
pub struct Hare {
    pub position: Vec2,
    pub heading: Heading,
    pub state: HareState,
    pub speed: f32,
    /// Metros recorridos desde que se sembró: el render marca el ritmo de sus
    /// saltos con esto.
    pub travel: f32,
    /// El chunk en el que se sembró: nunca sale de él, así que se carga por
    /// streaming junto con él.
    pub home: Aabb,
    timer: f32,
    rng: Rng,
}

impl Hare {
    /// Un jinete más cerca que esto la hace salir corriendo, en metros.
    pub const ALARM: f32 = 22.0;
    /// Se calma a esta distancia del jinete, en metros.
    pub const SAFE: f32 = 50.0;
    /// Velocidad de huida, en m/s.
    pub const RUN: f32 = 11.0;
    /// Velocidad a los saltos, en m/s.
    pub const HOP: f32 = 1.2;

    /// Orientaciones que se prueban, en orden, cuando el camino hacia adelante
    /// está cerrado: giros cada vez más amplios hacia uno y otro lado y,
    /// después, media vuelta.
    const DETOURS: [f32; 8] = [0.0, 0.8, -0.8, 1.6, -1.6, 2.4, -2.4, PI];

    /// Una liebre sentada en `position`, que se mantiene dentro de `home`.
    pub fn new(position: Vec2, home: Aabb, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x11a5);
        Self {
            position,
            heading: Heading::from_radians(rng.unit() * TAU),
            state: HareState::Sit,
            speed: 0.0,
            travel: 0.0,
            home,
            timer: rng.range(1.0, 6.0),
            rng,
        }
    }

    /// A qué altura del suelo está en medio de un salto, en metros.
    pub fn lift(&self) -> f32 {
        let stride = if self.state == HareState::Flee {
            2.2
        } else {
            0.5
        };
        let height = if self.state == HareState::Flee {
            0.35
        } else {
            0.12
        };
        match self.state {
            HareState::Sit => 0.0,
            _ => (self.travel / stride * PI).sin().abs() * height,
        }
    }

    /// Avanza `dt` segundos con un jinete en `threat`, pisando solo terreno que
    /// `open` permite dentro de su hogar: donde el camino hacia adelante está
    /// cerrado, se desvía o da media vuelta.
    pub fn step(&mut self, dt: f32, threat: Option<Vec2>, open: impl Fn(Vec2) -> bool) {
        self.timer -= dt;
        let away = threat.map(|t| (self.position - t, self.position.distance(t)));
        match (self.state, away) {
            (HareState::Flee, Some((_, d))) if d < Self::SAFE || self.timer > 0.0 => {}
            (HareState::Flee, _) => self.settle(),
            (_, Some((dir, d))) if d < Self::ALARM => {
                self.state = HareState::Flee;
                self.heading = Heading::from_radians(dir.y.atan2(dir.x));
                self.speed = Self::RUN;
                self.timer = 2.0;
            }
            (HareState::Hop, _) if self.timer <= 0.0 => self.settle(),
            (HareState::Sit, _) if self.timer <= 0.0 => {
                self.state = HareState::Hop;
                self.heading = self.heading.rotated(self.rng.range(-1.2, 1.2));
                self.speed = Self::HOP;
                self.timer = self.rng.range(0.8, 2.0);
            }
            _ => {}
        }
        if self.state == HareState::Flee {
            // Zigzaguea mientras corre: un quiebre cada tanto, siempre
            // alejándose.
            if let Some((dir, _)) = away {
                let jink = (self.travel * 0.35).sin() * 0.6;
                let off = turn_toward(self.heading, dir) + jink;
                self.heading = self.heading.rotated(off.clamp(-4.0 * dt, 4.0 * dt));
            }
        }
        let step = self.speed * dt;
        if step <= 0.0 {
            return;
        }
        let way = Self::DETOURS.iter().find_map(|&turn| {
            let heading = self.heading.rotated(turn);
            let next = self.position + heading.forward() * step;
            (self.home.contains_point(next) && open(next)).then_some((heading, next))
        });
        match way {
            Some((heading, next)) => {
                self.heading = heading;
                self.position = next;
                self.travel += step;
            }
            // Acorralada: se agazapa donde está.
            None => self.settle(),
        }
    }

    fn settle(&mut self) {
        self.state = HareState::Sit;
        self.speed = 0.0;
        self.timer = self.rng.range(3.0, 10.0);
    }
}

/// Lo que está haciendo una bandada de teros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlockState {
    /// En el suelo, picoteando.
    Ground,
    /// En el aire de `from` a `to`, a `t` del recorrido (`0..1`).
    Flight { from: Vec2, to: Vec2, t: f32 },
}

/// Unos pocos teros en el suelo que levantan vuelo juntos cuando alguien se
/// acerca.
#[derive(Debug, Clone, PartialEq)]
pub struct Flock {
    /// Dónde está parada la bandada (o sobre dónde vuela).
    pub center: Vec2,
    pub state: FlockState,
    /// El lugar de cada pájaro alrededor del centro.
    offsets: Vec<Vec2>,
    /// El chunk en el que se sembró: siempre aterriza dentro de él.
    pub home: Aabb,
    /// Segundos desde que se sembró, para picotear y aletear.
    pub clock: f32,
    rng: Rng,
}

impl Flock {
    /// Un jinete más cerca que esto los hace levantar vuelo, en metros.
    pub const ALARM: f32 = 18.0;
    /// Velocidad de vuelo, en m/s.
    pub const SPEED: f32 = 9.0;
    /// Altura del punto más alto del arco, en metros.
    pub const ARC: f32 = 7.0;
    /// Vuelo más corto y más largo, en metros.
    pub const FLIGHT: (f32, f32) = (40.0, 70.0);

    /// Lugares de aterrizaje que se prueban antes de que la bandada vuelva en
    /// círculo al lugar donde levantó vuelo.
    const LANDING_TRIES: usize = 8;

    /// `count` teros en el suelo en `center`, que se mantienen dentro de
    /// `home`.
    pub fn new(center: Vec2, home: Aabb, count: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x7e40);
        let offsets = (0..count)
            .map(|_| Vec2::new(rng.range(-3.0, 3.0), rng.range(-3.0, 3.0)))
            .collect();
        Self {
            center,
            state: FlockState::Ground,
            offsets,
            home,
            clock: 0.0,
            rng,
        }
    }

    /// Si está en el aire.
    pub fn flying(&self) -> bool {
        matches!(self.state, FlockState::Flight { .. })
    }

    /// La posición sobre el suelo de cada pájaro y su altura sobre el suelo.
    pub fn birds(&self) -> impl Iterator<Item = (Vec2, f32)> + '_ {
        let height = match self.state {
            FlockState::Ground => 0.0,
            FlockState::Flight { t, .. } => Self::ARC * (PI * t).sin(),
        };
        self.offsets.iter().map(move |&o| (self.center + o, height))
    }

    /// Hacia dónde miran los pájaros: en la dirección del vuelo, o en una
    /// dirección sembrada.
    pub fn heading(&self) -> Heading {
        match self.state {
            FlockState::Flight { from, to, .. } => {
                let d = to - from;
                Heading::from_radians(d.y.atan2(d.x))
            }
            FlockState::Ground => Heading::from_radians(self.offsets.len() as f32),
        }
    }

    /// Avanza `dt` segundos con un jinete en `threat`: levanta vuelo alejándose
    /// de él, vuela en un arco bajo y aterriza en terreno que `open` permite
    /// dentro de su hogar (o vuelve en círculo al lugar donde levantó vuelo
    /// cuando ningún lugar más adelante sirve).
    pub fn step(&mut self, dt: f32, threat: Option<Vec2>, open: impl Fn(Vec2) -> bool) {
        self.clock = crate::tick_clock(self.clock, dt);
        match self.state {
            FlockState::Ground => {
                let Some(rider) = threat else { return };
                let away = self.center - rider;
                if away.length() >= Self::ALARM {
                    return;
                }
                let from = self.center;
                let ahead = away.y.atan2(away.x);
                let to = (0..Self::LANDING_TRIES)
                    .map(|k| {
                        // Cada vez más desviado de la dirección opuesta al
                        // jinete.
                        let spread = 0.6 + 0.3 * k as f32;
                        let angle = ahead + self.rng.range(-spread, spread);
                        let reach = self.rng.range(Self::FLIGHT.0, Self::FLIGHT.1);
                        from + Vec2::new(angle.cos(), angle.sin()) * reach
                    })
                    .find(|&to| self.home.contains_point(to) && open(to))
                    .unwrap_or(from);
                self.state = FlockState::Flight { from, to, t: 0.0 };
            }
            FlockState::Flight { from, to, t } => {
                // Al menos el tiempo del vuelo más corto, aunque vuelva en
                // círculo.
                let t = t + dt * Self::SPEED / from.distance(to).max(Self::FLIGHT.0);
                if t >= 1.0 {
                    self.center = to;
                    self.state = FlockState::Ground;
                } else {
                    self.center = from + (to - from) * t;
                    self.state = FlockState::Flight { from, to, t };
                }
            }
        }
    }
}

/// Un chimango o un carancho que vuela en círculos lentos en lo alto.
#[derive(Debug, Clone, PartialEq)]
pub struct Raptor {
    /// El centro de su círculo.
    pub center: Vec2,
    pub radius: f32,
    pub altitude: f32,
    /// Radianes recorridos en el círculo.
    pub angle: f32,
    /// Radianes por segundo (negativo: sentido horario).
    pub rate: f32,
}

impl Raptor {
    pub fn new(center: Vec2, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0xca7a);
        let radius = rng.range(30.0, 60.0);
        // Alrededor de 7 m/s por el círculo, en cualquiera de los dos sentidos.
        let rate = 7.0 / radius * if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        Self {
            center,
            radius,
            altitude: rng.range(30.0, 55.0),
            angle: rng.unit() * TAU,
            rate,
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.angle = (self.angle + self.rate * dt).rem_euclid(TAU);
    }

    /// Dónde está sobre el suelo.
    pub fn position(&self) -> Vec2 {
        self.center + Vec2::new(self.angle.cos(), self.angle.sin()) * self.radius
    }

    /// Mirando a lo largo del círculo.
    pub fn heading(&self) -> Heading {
        Heading::from_radians(self.angle + PI / 2.0 * self.rate.signum())
    }

    /// Su inclinación hacia el giro, en radianes (positivo: ala izquierda hacia
    /// abajo).
    pub fn bank(&self) -> f32 {
        0.35 * self.rate.signum()
    }
}

/// Un pato que nada en una laguna.
#[derive(Debug, Clone, PartialEq)]
pub struct Duck {
    /// El centro de su grupo, desde donde se carga por streaming.
    pub home: Vec2,
    pub position: Vec2,
    pub heading: Heading,
    /// Índice dentro de las lagunas del sitio.
    pub lagoon: usize,
    target: Vec2,
    rng: Rng,
}

impl Duck {
    /// Un jinete más cerca que esto lo hace alejarse nadando, en metros.
    pub const ALARM: f32 = 15.0;
    /// Velocidades de nado, tranquila y apurada, en m/s.
    pub const PADDLE: (f32, f32) = (0.3, 1.5);

    pub fn new(home: Vec2, position: Vec2, lagoon: usize, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0xd0c);
        Self {
            home,
            position,
            heading: Heading::from_radians(rng.unit() * TAU),
            lagoon,
            target: position,
            rng,
        }
    }

    /// Avanza `dt` segundos en `lagoon` con un jinete en `threat`: deriva
    /// alrededor de su grupo, se aleja nadando de los jinetes y nunca sale del
    /// agua profunda.
    pub fn step(&mut self, dt: f32, lagoon: &Lagoon, threat: Option<Vec2>) {
        let alarmed = threat.filter(|t| t.distance(self.position) < Self::ALARM);
        if let Some(rider) = alarmed {
            let away = self.position - rider;
            self.target = self.position + away * (8.0 / away.length().max(1e-3));
        } else if self.position.distance(self.target) < 0.5 {
            let r = &mut self.rng;
            self.target = self.home + Vec2::new(r.range(-12.0, 12.0), r.range(-12.0, 12.0));
        }
        let to = self.target - self.position;
        let turn = turn_toward(self.heading, to);
        self.heading = self.heading.rotated(turn.clamp(-1.5 * dt, 1.5 * dt));
        let speed = if alarmed.is_some() {
            Self::PADDLE.1
        } else {
            Self::PADDLE.0
        };
        let next = self.position + self.heading.forward() * (speed * dt);
        if lagoon.is_deep(next) {
            self.position = next;
        } else {
            // Volver hacia el grupo en lugar de salir del agua.
            self.target = self.home;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LagoonConfig, Terrain};

    #[test]
    fn a_hare_bolts_from_a_rider_and_calms_down_once_far() {
        let home = Aabb::from_center_size(Vec2::ZERO, Vec2::new(400.0, 400.0));
        let mut hare = Hare::new(Vec2::new(0.0, 0.0), home, 3);
        let far = Vec2::new(200.0, 0.0);
        for _ in 0..100 {
            hare.step(0.1, Some(far), |_| true);
            assert_ne!(hare.state, HareState::Flee);
        }
        let rider = hare.position + Vec2::new(10.0, 0.0);
        hare.step(0.1, Some(rider), |_| true);
        assert_eq!(hare.state, HareState::Flee);
        let mut fled = 0.0;
        for _ in 0..100 {
            hare.step(0.1, Some(rider), |_| true);
            fled = hare.position.distance(rider);
            if hare.state != HareState::Flee {
                break;
            }
            assert!(hare.lift() >= 0.0 && hare.lift() <= 0.4);
        }
        assert_eq!(hare.state, HareState::Sit, "calmed down");
        assert!(fled >= Hare::SAFE, "{fled}");
        assert!(hare.position.x < 0.0, "ran away from the rider");
    }

    #[test]
    fn teros_take_off_away_from_a_rider_and_land_further_on() {
        let start = Vec2::new(50.0, 50.0);
        let home = Aabb::from_center_size(start, Vec2::new(400.0, 400.0));
        let mut flock = Flock::new(start, home, 4, 9);
        flock.step(0.1, Some(start + Vec2::new(100.0, 0.0)), |_| true);
        assert!(!flock.flying());
        let rider = start + Vec2::new(10.0, 0.0);
        flock.step(0.1, Some(rider), |_| true);
        assert!(flock.flying());
        let mut top: f32 = 0.0;
        for _ in 0..200 {
            flock.step(0.1, None, |_| true);
            top = top.max(flock.birds().map(|b| b.1).fold(0.0, f32::max));
            if !flock.flying() {
                break;
            }
        }
        assert!(!flock.flying(), "landed");
        assert!(top > Flock::ARC * 0.8, "{top}");
        assert!(flock.birds().all(|b| b.1 == 0.0));
        let flown = flock.center.distance(start);
        assert!((Flock::FLIGHT.0..=Flock::FLIGHT.1 + 1.0).contains(&flown));
        assert!(flock.center.distance(rider) > start.distance(rider));
    }

    #[test]
    fn a_hare_bolting_toward_a_lagoon_swerves_and_stays_home() {
        let config = LagoonConfig {
            name: "Prueba".into(),
            center: Vec2::new(0.0, 0.0),
            size: Vec2::new(200.0, 200.0),
            seed: 2,
        };
        let lagoon = Lagoon::new(&config, &Terrain::default());
        let open = |p: Vec2| lagoon.offshore(p) > 10.0;
        let home = Aabb::new(Vec2::new(100.0, -60.0), Vec2::new(200.0, 60.0));
        for seed in 0..20 {
            let mut hare = Hare::new(Vec2::new(125.0, 5.0 * seed as f32 - 50.0), home, seed);
            // El jinete viene del este: alejarse es meterse en el agua.
            let rider = hare.position + Vec2::new(12.0, 0.0);
            for _ in 0..300 {
                hare.step(0.1, Some(rider), open);
                assert!(open(hare.position), "in the lagoon: {:?}", hare.position);
                assert!(home.contains_point(hare.position), "left home");
            }
        }
    }

    #[test]
    fn teros_only_ever_land_on_open_ground_at_home() {
        let start = Vec2::new(0.0, 0.0);
        let home = Aabb::from_center_size(start, Vec2::new(200.0, 200.0));
        // Todo lo que está al este de la bandada está cerrado (un camino, por
        // ejemplo).
        let open = |p: Vec2| p.x <= 5.0;
        for seed in 0..40 {
            let mut flock = Flock::new(start, home, 4, seed);
            // El jinete viene del oeste: alejarse es ir al este, que está
            // cerrado.
            let rider = start - Vec2::new(10.0, 0.0);
            for _ in 0..600 {
                flock.step(0.1, Some(rider), open);
                if !flock.flying() {
                    assert!(open(flock.center) && home.contains_point(flock.center));
                }
            }
        }
        // No hay adónde ir: vuelve en círculo al lugar donde levantó vuelo.
        let mut flock = Flock::new(start, home, 4, 1);
        flock.step(0.1, Some(start + Vec2::new(5.0, 0.0)), |p| p == start);
        assert!(flock.flying());
        for _ in 0..100 {
            flock.step(0.1, None, |p| p == start);
        }
        assert!(!flock.flying() && flock.center == start);
    }

    #[test]
    fn a_raptor_circles_high_and_slowly() {
        let mut bird = Raptor::new(Vec2::new(0.0, 0.0), 4);
        let mut last = bird.position();
        for _ in 0..1000 {
            bird.step(0.1);
            let p = bird.position();
            assert!((p.length() - bird.radius).abs() < 1e-2);
            assert!(p.distance(last) < 1.0, "about 7 m/s");
            last = p;
        }
        assert!((30.0..=55.0).contains(&bird.altitude));
    }

    #[test]
    fn ducks_stay_on_deep_water_and_paddle_off_from_riders() {
        let config = LagoonConfig {
            name: "Prueba".into(),
            center: Vec2::new(0.0, 0.0),
            size: Vec2::new(300.0, 200.0),
            seed: 2,
        };
        let lagoon = Lagoon::new(&config, &Terrain::default());
        let mut duck = Duck::new(Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0), 0, 5);
        for _ in 0..3000 {
            duck.step(0.1, &lagoon, None);
            assert!(lagoon.is_deep(duck.position));
        }
        let rider = duck.position + Vec2::new(5.0, 0.0);
        let before = duck.position.distance(rider);
        for _ in 0..50 {
            duck.step(0.1, &lagoon, Some(rider));
        }
        assert!(duck.position.distance(rider) > before + 2.0);
    }
}
