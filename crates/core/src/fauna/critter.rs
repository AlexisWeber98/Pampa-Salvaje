//! La fauna silvestre de suelo de la pampa de 1919 (ver [`super`]): zorros,
//! ñandúes, mulitas, peludos, vizcachas, gatos monteses, pumas y, muy rara vez,
//! un yaguareté. Cada uno descansa, deambula un poco y huye de un jinete según
//! su especie ([`CritterKind::profile`]); la vizcacha corre a su vizcachera y
//! se mete adentro. Ninguno es un obstáculo.

use std::f32::consts::TAU;

use super::turn_toward;
use super::wild::detour;
use crate::rng::Rng;
use crate::{Aabb, Heading, Vec2};

/// Qué animal es.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CritterKind {
    /// El zorro gris pampeano.
    Zorro,
    /// El ñandú, en grupos pequeños en el campo abierto.
    Nandu,
    /// La mulita, un armadillo chico.
    Mulita,
    /// El peludo, un armadillo más grande y peludo.
    Peludo,
    /// La vizcacha, en colonias alrededor de su vizcachera.
    Vizcacha,
    /// El gato montés, en el monte.
    GatoMontes,
    /// El puma, en el monte.
    Puma,
    /// El yaguareté: casi extinguido en la región entre 1903 y 1925, un
    /// avistamiento excepcional en el monte.
    Yaguarete,
}

/// Cómo se mueve una especie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// Un jinete más cerca que esto lo hace huir, en metros.
    pub alarm: f32,
    /// Se calma a esta distancia del jinete, en metros.
    pub safe: f32,
    /// Velocidad de huida, en m/s.
    pub run: f32,
    /// Velocidad al deambular, en m/s.
    pub walk: f32,
    /// Cuánto zigzaguea al huir, en radianes (0: en línea recta).
    pub jink: f32,
}

impl CritterKind {
    pub const ALL: [Self; 8] = [
        Self::Zorro,
        Self::Nandu,
        Self::Mulita,
        Self::Peludo,
        Self::Vizcacha,
        Self::GatoMontes,
        Self::Puma,
        Self::Yaguarete,
    ];

    /// Cómo se mueve. Los felinos se alejan al trote más que a la carrera
    /// (el modelo del gato solo trae una caminata); los armadillos son lentos.
    pub const fn profile(self) -> Profile {
        let (alarm, safe, run, walk, jink) = match self {
            Self::Zorro => (30.0, 70.0, 7.0, 0.5, 0.25),
            Self::Nandu => (45.0, 90.0, 13.0, 0.8, 0.0),
            Self::Mulita => (12.0, 30.0, 2.5, 0.3, 0.3),
            Self::Peludo => (12.0, 30.0, 3.0, 0.35, 0.3),
            Self::Vizcacha => (25.0, 45.0, 5.0, 0.4, 0.0),
            Self::GatoMontes => (20.0, 50.0, 3.0, 0.3, 0.2),
            Self::Puma => (40.0, 90.0, 5.0, 0.6, 0.1),
            Self::Yaguarete => (40.0, 90.0, 4.5, 0.6, 0.1),
        };
        Profile {
            alarm,
            safe,
            run,
            walk,
            jink,
        }
    }

    /// Si es un felino del monte.
    pub const fn feline(self) -> bool {
        matches!(self, Self::GatoMontes | Self::Puma | Self::Yaguarete)
    }
}

/// Lo que está haciendo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CritterState {
    /// Quieto (echado, olfateando, picoteando).
    Rest,
    /// Deambulando despacio.
    Walk,
    /// Huyendo de un jinete.
    Flee,
    /// Metido en su cueva (solo la vizcacha).
    Den,
}

/// Un animal silvestre de suelo.
#[derive(Debug, Clone, PartialEq)]
pub struct Critter {
    pub kind: CritterKind,
    pub position: Vec2,
    pub heading: Heading,
    pub state: CritterState,
    pub speed: f32,
    /// El chunk en el que se sembró: nunca sale de él, así que se carga por
    /// streaming junto con él.
    pub home: Aabb,
    /// La boca de su vizcachera (solo la vizcacha).
    pub den: Option<Vec2>,
    timer: f32,
    rng: Rng,
}

impl Critter {
    /// A esta distancia de la boca de la vizcachera ya se metió adentro, en
    /// metros.
    pub const DEN_REACH: f32 = 0.8;
    /// Cuánto se aleja una vizcacha de su vizcachera al deambular, en metros.
    pub const DEN_RANGE: f32 = 10.0;
    /// A qué profundidad bajo el suelo queda dentro de su cueva, en metros.
    pub const DEN_DEPTH: f32 = 0.6;

    /// Un animal de `kind` quieto en `position`, que se mantiene dentro de
    /// `home` (con la boca de su cueva en `den`, si tiene).
    pub fn new(
        kind: CritterKind,
        position: Vec2,
        home: Aabb,
        den: Option<Vec2>,
        seed: u32,
    ) -> Self {
        let mut rng = Rng::new(seed, 0xc417);
        Self {
            kind,
            position,
            heading: Heading::from_radians(rng.unit() * TAU),
            state: CritterState::Rest,
            speed: 0.0,
            home,
            den,
            timer: rng.range(1.0, 8.0),
            rng,
        }
    }

    /// A qué altura del suelo está, en metros: bajo tierra dentro de su cueva.
    pub fn lift(&self) -> f32 {
        if self.state == CritterState::Den {
            -Self::DEN_DEPTH
        } else {
            0.0
        }
    }

    /// Avanza `dt` segundos con un jinete en `threat`, pisando solo terreno que
    /// `open` permite dentro de su hogar: donde el camino hacia adelante está
    /// cerrado, se desvía o da media vuelta.
    pub fn step(&mut self, dt: f32, threat: Option<Vec2>, open: impl Fn(Vec2) -> bool) {
        let profile = self.kind.profile();
        self.timer -= dt;
        let away = threat.map(|t| (self.position - t, self.position.distance(t)));
        let calm = away.is_none_or(|(_, d)| d >= profile.safe);
        match (self.state, away) {
            (CritterState::Den, _) => {
                if calm && self.timer <= 0.0 {
                    self.settle();
                }
                return;
            }
            (CritterState::Flee, _) if !calm || self.timer > 0.0 => {}
            (CritterState::Flee, _) => self.settle(),
            (_, Some((dir, d))) if d < profile.alarm => {
                self.state = CritterState::Flee;
                let to = self.den.map_or(dir, |den| den - self.position);
                self.heading = Heading::from_radians(to.y.atan2(to.x));
                self.speed = profile.run;
                self.timer = 2.0;
            }
            (CritterState::Walk, _) if self.timer <= 0.0 => self.settle(),
            (CritterState::Rest, _) if self.timer <= 0.0 => {
                self.state = CritterState::Walk;
                self.heading = match self.den {
                    // Vuelve hacia la vizcachera si se alejó.
                    Some(den) if den.distance(self.position) > Self::DEN_RANGE => {
                        let to = den - self.position;
                        Heading::from_radians(to.y.atan2(to.x))
                    }
                    _ => self.heading.rotated(self.rng.range(-1.5, 1.5)),
                };
                self.speed = profile.walk;
                self.timer = self.rng.range(1.5, 5.0);
            }
            _ => {}
        }
        if self.state == CritterState::Flee {
            match (self.den, away) {
                (Some(den), _) => {
                    if den.distance(self.position) <= Self::DEN_REACH.max(self.speed * dt) {
                        self.position = den;
                        self.state = CritterState::Den;
                        self.speed = 0.0;
                        self.timer = self.rng.range(4.0, 10.0);
                        return;
                    }
                    let off = turn_toward(self.heading, den - self.position);
                    self.heading = self.heading.rotated(off.clamp(-6.0 * dt, 6.0 * dt));
                }
                (None, Some((dir, _))) => {
                    let jink = (self.timer * 2.3).sin() * profile.jink;
                    let off = turn_toward(self.heading, dir) + jink;
                    self.heading = self.heading.rotated(off.clamp(-3.0 * dt, 3.0 * dt));
                }
                (None, None) => {}
            }
        }
        let step = self.speed * dt;
        if step <= 0.0 {
            return;
        }
        match detour(self.position, self.heading, step, self.home, open) {
            Some((heading, next)) => {
                self.heading = heading;
                self.position = next;
            }
            // Acorralado: se queda quieto donde está.
            None => self.settle(),
        }
    }

    fn settle(&mut self) {
        self.state = CritterState::Rest;
        self.speed = 0.0;
        self.timer = self.rng.range(3.0, 12.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> Aabb {
        Aabb::from_center_size(Vec2::ZERO, Vec2::new(400.0, 400.0))
    }

    #[test]
    fn each_kind_flees_a_rider_at_its_own_pace_and_calms_down_far_off() {
        for kind in CritterKind::ALL
            .into_iter()
            .filter(|&k| k != CritterKind::Vizcacha)
        {
            let profile = kind.profile();
            let mut animal = Critter::new(kind, Vec2::ZERO, home(), None, 7);
            let far = Vec2::new(profile.safe + 50.0, 0.0);
            for _ in 0..100 {
                animal.step(0.1, Some(far), |_| true);
                assert_ne!(animal.state, CritterState::Flee, "{kind:?}");
            }
            let rider = animal.position + Vec2::new(profile.alarm * 0.5, 0.0);
            animal.step(0.1, Some(rider), |_| true);
            assert_eq!(animal.state, CritterState::Flee, "{kind:?}");
            assert_eq!(animal.speed, profile.run);
            let start = animal.position;
            for _ in 0..1000 {
                animal.step(0.1, Some(rider), |_| true);
                if animal.state != CritterState::Flee {
                    break;
                }
            }
            assert_eq!(animal.state, CritterState::Rest, "{kind:?} calmed down");
            assert!(animal.position.distance(rider) >= profile.safe, "{kind:?}");
            assert!(
                animal.position.x < start.x,
                "{kind:?} ran away from the rider"
            );
        }
        // Un ñandú corre mucho más rápido que un peludo, y en línea recta.
        let (nandu, peludo) = (CritterKind::Nandu.profile(), CritterKind::Peludo.profile());
        assert!(nandu.run > 3.0 * peludo.run && nandu.jink == 0.0);
    }

    #[test]
    fn a_vizcacha_runs_into_its_burrow_and_comes_out_once_the_rider_is_gone() {
        let den = Vec2::new(-6.0, 0.0);
        let mut vizcacha = Critter::new(CritterKind::Vizcacha, Vec2::ZERO, home(), Some(den), 3);
        // El jinete viene desde el lado de la vizcachera.
        let rider = Vec2::new(-15.0, 3.0);
        for _ in 0..50 {
            vizcacha.step(0.1, Some(rider), |_| true);
        }
        assert_eq!(vizcacha.state, CritterState::Den);
        assert_eq!(vizcacha.position, den);
        assert!(vizcacha.lift() < -0.3, "under ground");
        for _ in 0..300 {
            vizcacha.step(0.1, Some(rider), |_| true);
            assert_eq!(
                vizcacha.state,
                CritterState::Den,
                "stays in while the rider is near"
            );
        }
        let gone = Vec2::new(500.0, 0.0);
        for _ in 0..200 {
            vizcacha.step(0.1, Some(gone), |_| true);
        }
        assert_ne!(vizcacha.state, CritterState::Den);
        assert_eq!(vizcacha.lift(), 0.0);
        // Al deambular no se aleja de la vizcachera.
        for _ in 0..5000 {
            vizcacha.step(0.1, None, |_| true);
            assert!(vizcacha.position.distance(den) < Critter::DEN_RANGE + 5.0);
        }
    }

    #[test]
    fn a_fleeing_animal_never_leaves_open_ground_or_its_home() {
        let home = Aabb::new(Vec2::new(0.0, -50.0), Vec2::new(100.0, 50.0));
        let open = |p: Vec2| p.x < 60.0;
        for kind in CritterKind::ALL {
            for seed in 0..10 {
                let mut animal = Critter::new(
                    kind,
                    Vec2::new(50.0, seed as f32 * 5.0 - 25.0),
                    home,
                    None,
                    seed,
                );
                // El jinete viene del oeste: alejarse es ir hacia lo cerrado.
                let rider = animal.position - Vec2::new(8.0, 0.0);
                for _ in 0..400 {
                    animal.step(0.1, Some(rider), open);
                    assert!(open(animal.position) && home.contains_point(animal.position));
                }
            }
        }
    }
}
