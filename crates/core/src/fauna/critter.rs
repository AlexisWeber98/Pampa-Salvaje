//! La fauna silvestre de suelo de la pampa de 1919 (ver [`super`]): zorros,
//! ñandúes, mulitas, peludos, vizcachas, gatos monteses, pumas y, muy rara vez,
//! un yaguareté. Cada uno descansa, deambula un poco y huye de un jinete según
//! su especie ([`CritterKind::profile`]); la vizcacha corre a su vizcachera y
//! se mete adentro. Ninguno es un obstáculo.

use std::f32::consts::TAU;

use super::turn_toward;
use super::wild::DETOURS;
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

/// `speed` km/h en m/s.
pub const fn kmh(speed: f32) -> f32 {
    speed / 3.6
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
    /// Cuántos metros aguanta a `run` antes de aflojar (infinito: no afloja).
    pub sprint: f32,
    /// A qué velocidad sigue huyendo después del pique, en m/s.
    pub cruise: f32,
    /// La mitad del lado de su territorio, en metros.
    pub roam: f32,
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

    /// Cómo se mueve. Los felinos huyen a la carrera, a las velocidades
    /// máximas de un pique corto que se citan para cada especie, en km/h
    /// ([`kmh`]): el gato montés 45, el puma 72 y el yaguareté 65. Son cifras
    /// de divulgación (el puma entre 64 y 80, el yaguareté entre 65 y 80, el
    /// gato de Geoffroy unos 35 y un gato doméstico entre 40 y 48); no hay
    /// mediciones publicadas con revisión de pares. Ese pique dura poco
    /// ([`Profile::sprint`]): el puma aguanta unos cientos de metros, el
    /// yaguareté, que caza al acecho, menos, y el gato montés menos aún;
    /// después sigue a un galope de algo más de la mitad de esa velocidad
    /// ([`Profile::cruise`]: 25, 40 y 36 km/h; distancias y velocidades
    /// estimadas, sin mediciones publicadas). Los demás huyen siempre a su
    /// velocidad. Todos arrancan acelerando ([`Critter::ACCEL`]) y giran de a
    /// poco ([`Critter::WALK_TURN`], [`Critter::FLEE_TURN`]): a toda carrera
    /// la curva es más abierta ([`Critter::GRIP`]), frenan para doblar una
    /// cerrada ([`Critter::BRAKE`]) y miran adelante ([`Critter::LOOK`]) para
    /// esquivar un borde cerrado sin volantazos. Los armadillos son lentos.
    pub const fn profile(self) -> Profile {
        let (alarm, safe, run, walk, jink) = match self {
            Self::Zorro => (30.0, 70.0, 7.0, 0.5, 0.25),
            Self::Nandu => (45.0, 90.0, 13.0, 0.8, 0.0),
            Self::Mulita => (12.0, 30.0, 2.5, 0.3, 0.3),
            Self::Peludo => (12.0, 30.0, 3.0, 0.35, 0.3),
            Self::Vizcacha => (25.0, 45.0, 5.0, 0.4, 0.0),
            Self::GatoMontes => (20.0, 50.0, kmh(45.0), 0.3, 0.2),
            Self::Puma => (40.0, 90.0, kmh(72.0), 0.6, 0.1),
            Self::Yaguarete => (40.0, 90.0, kmh(65.0), 0.6, 0.1),
        };
        // Sus territorios reales van de unas hectáreas (armadillos) a cientos
        // de km² (puma); acá, a escala de la fauna que anda alrededor del
        // jugador, pero en el mismo orden.
        let roam = match self {
            Self::Puma => 500.0,
            Self::Yaguarete => 400.0,
            Self::Nandu => 350.0,
            Self::Zorro => 250.0,
            Self::GatoMontes => 200.0,
            Self::Peludo => 80.0,
            Self::Mulita => 60.0,
            Self::Vizcacha => 40.0,
        };
        let (sprint, cruise) = match self {
            Self::GatoMontes => (100.0, kmh(25.0)),
            Self::Puma => (300.0, kmh(40.0)),
            Self::Yaguarete => (150.0, kmh(36.0)),
            _ => (f32::INFINITY, run),
        };
        Profile {
            alarm,
            safe,
            run,
            walk,
            jink,
            sprint,
            cruise,
            roam,
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
    /// Su territorio ([`Profile::roam`]): nunca sale de él.
    pub home: Aabb,
    /// La boca de su vizcachera (solo la vizcacha).
    pub den: Option<Vec2>,
    /// Hacia dónde quiere ir al deambular: gira hacia ahí de a poco.
    aim: Heading,
    /// Los metros recorridos desde que empezó a huir.
    fled: f32,
    /// Los segundos que lleva fuera de su hábitat.
    out: f32,
    /// Donde estuvo en su hábitat por última vez: ahí vuelve.
    haven: Vec2,
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
    /// Cuánto afloja por segundo al terminar su pique, en m/s².
    pub const EASE: f32 = 2.5;
    /// Cuánto acelera por segundo al arrancar a huir, en m/s².
    pub const ACCEL: f32 = 10.0;
    /// Lo más rápido que gira al deambular, en radianes por segundo.
    pub const WALK_TURN: f32 = 1.5;
    /// Lo más rápido que gira al huir, en radianes por segundo: da media
    /// vuelta en medio segundo.
    pub const FLEE_TURN: f32 = 6.0;
    /// La fuerza lateral que aguantan sus patas al doblar, en m/s²: cuanto más
    /// rápido corre, más abierta la curva.
    pub const GRIP: f32 = 12.0;
    /// Cuánto frena por segundo para doblar una curva cerrada, en m/s².
    pub const BRAKE: f32 = 15.0;
    /// Cuántos segundos de carrera mira adelante para esquivar lo cerrado.
    pub const LOOK: f32 = 1.0;

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
        let heading = Heading::from_radians(rng.unit() * TAU);
        Self {
            kind,
            position,
            heading,
            aim: heading,
            state: CritterState::Rest,
            speed: 0.0,
            home,
            den,
            fled: 0.0,
            out: 0.0,
            haven: position,
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
        self.step_in(dt, threat, open, |_| true)
    }

    /// Lo más que pasa fuera de su hábitat: dos horas de juego, en segundos
    /// reales.
    pub const STRAY: f32 = (2.0 * 3600.0 / crate::BASE_RATE) as f32;

    /// Como [`Self::step`], pero en su `habitat` (el monte, para los
    /// felinos): puede salir a deambular o huyendo, pero pasada la mitad de
    /// [`Self::STRAY`] afuera vuelve sin descansar al último lugar de su
    /// hábitat donde estuvo, apurando el paso si está lejos.
    pub fn step_in(
        &mut self,
        dt: f32,
        threat: Option<Vec2>,
        open: impl Fn(Vec2) -> bool,
        habitat: impl Fn(Vec2) -> bool,
    ) {
        let profile = self.kind.profile();
        self.timer -= dt;
        if habitat(self.position) {
            self.out = 0.0;
            self.haven = self.position;
        } else {
            self.out += dt;
        }
        let homing = self.out >= Self::STRAY * 0.5;
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
            (_, Some((_, d))) if d < profile.alarm => {
                // Gira y acelera desde como estaba (ver abajo).
                self.state = CritterState::Flee;
                self.fled = 0.0;
                self.timer = 2.0;
            }
            (CritterState::Walk, _) if self.timer <= 0.0 && !homing => self.settle(),
            (CritterState::Rest, _) if self.timer <= 0.0 || homing => {
                self.state = CritterState::Walk;
                self.aim = match self.den {
                    _ if homing => self.aim,
                    // Vuelve hacia la vizcachera si se alejó.
                    Some(den) if den.distance(self.position) > Self::DEN_RANGE => {
                        let to = den - self.position;
                        Heading::from_radians(to.y.atan2(to.x))
                    }
                    _ => {
                        // Recorre su territorio: sigue más o menos derecho y,
                        // si se alejó mucho del centro, vuelve hacia él.
                        let to = self.home.center() - self.position;
                        let size = self.home.max - self.home.min;
                        if to.length() > 0.3 * size.x.min(size.y) {
                            Heading::from_radians(to.y.atan2(to.x))
                                .rotated(self.rng.range(-0.6, 0.6))
                        } else {
                            self.heading.rotated(self.rng.range(-1.0, 1.0))
                        }
                    }
                };
                self.speed = profile.walk;
                // Las de vizcachera dan vueltitas cortas cerca de la boca.
                self.timer = match self.den {
                    Some(_) => self.rng.range(1.5, 5.0),
                    None => self.rng.range(5.0, 20.0),
                };
            }
            _ => {}
        }
        // Hacia dónde quiere ir, lo más rápido que puede girar hacia ahí y si
        // ese giro depende de cuán rápido corre.
        let (goal, rate, grip) = match (self.state, self.den, away) {
            (CritterState::Walk, _, _) => {
                if homing {
                    // De vuelta a su hábitat, a tiempo.
                    let to = self.haven - self.position;
                    self.aim = Heading::from_radians(to.y.atan2(to.x));
                    // Con margen: apunta a llegar a las cuatro quintas partes.
                    let left = (Self::STRAY * 0.8 - self.out).max(1.0);
                    self.speed = (to.length() / left).clamp(profile.walk, profile.cruise);
                }
                (self.aim, Self::WALK_TURN, false)
            }
            (CritterState::Flee, Some(den), _) => {
                if den.distance(self.position) <= Self::DEN_REACH.max(self.speed * dt) {
                    self.position = den;
                    self.state = CritterState::Den;
                    self.speed = 0.0;
                    self.timer = self.rng.range(4.0, 10.0);
                    return;
                }
                let to = den - self.position;
                (
                    Heading::from_radians(to.y.atan2(to.x)),
                    Self::FLEE_TURN,
                    false,
                )
            }
            (CritterState::Flee, None, Some((dir, _))) => {
                let jink = (self.timer * 2.3).sin() * profile.jink;
                let goal = self.heading.rotated(turn_toward(self.heading, dir) + jink);
                (goal, Self::FLEE_TURN, true)
            }
            _ => (self.heading, Self::FLEE_TURN, false),
        };
        // Mira un trecho adelante: si por ahí el paso está cerrado, empieza a
        // doblar desde antes en vez de pegar el volantazo contra el borde.
        // Cuanto más rápido corre, más lejos mira: lo que avanza en `LOOK` más
        // lo que le lleva doblar a esa velocidad; parado, un par de metros.
        let reach = (self.speed * Self::LOOK + self.speed * self.speed / Self::GRIP).max(2.0);
        let way = [reach, reach * 0.5, reach * 0.25]
            .into_iter()
            .find_map(|far| {
                DETOURS
                    .iter()
                    .map(|&t| goal.rotated(t))
                    .find(|&h| self.clear(h, far, &open))
            });
        let Some(way) = way else {
            // Acorralado: se queda quieto donde está.
            self.settle();
            return;
        };
        let off = turn_toward(self.heading, way.forward());
        if self.state == CritterState::Walk {
            // Deambulando, el desvío pasa a ser su nuevo rumbo.
            self.aim = way;
        }
        if self.state == CritterState::Flee {
            // Acelera hasta su pique; pasado el pique, afloja hasta su crucero;
            // en una curva cerrada frena para poder doblarla.
            let mut want = if self.fled >= profile.sprint {
                profile.cruise
            } else {
                profile.run
            };
            let corner = (Self::GRIP * Self::LOOK / off.abs().max(1e-3)).max(profile.walk);
            let brake = if corner < want.min(self.speed) {
                want = corner;
                Self::BRAKE
            } else {
                Self::EASE
            };
            self.speed = if self.speed < want {
                (self.speed + Self::ACCEL * dt).min(want)
            } else {
                (self.speed - brake * dt).max(want)
            };
        }
        let rate = if grip {
            rate.min(Self::GRIP / self.speed.max(1e-3))
        } else {
            rate
        };
        let turn = rate * dt;
        self.heading = self.heading.rotated(off.clamp(-turn, turn));
        let step = self.speed * dt;
        if step <= 0.0 {
            return;
        }
        let next = self.position + self.heading.forward() * step;
        if self.home.contains_point(next) && open(next) {
            self.fled += step;
            self.position = next;
        } else if self.state == CritterState::Flee {
            // Se topó con el borde sin terminar de doblar: frena en seco y
            // pivotea ahí mismo hasta encarar el paso abierto.
            self.speed = 0.0;
        }
    }

    /// Si el tramo de `far` metros mirando hacia `heading` está abierto y
    /// dentro de su territorio.
    fn clear(&self, heading: Heading, far: f32, open: impl Fn(Vec2) -> bool) -> bool {
        (1..=4).all(|k| {
            let p = self.position + heading.forward() * (far * k as f32 / 4.0);
            self.home.contains_point(p) && open(p)
        })
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
            // Arranca acelerando: llega a su velocidad de huida en unos
            // segundos, no en un frame.
            assert!(animal.speed < profile.run, "{kind:?}");
            let start = animal.position;
            let mut top = 0.0f32;
            for _ in 0..1000 {
                animal.step(0.1, Some(rider), |_| true);
                top = top.max(animal.speed);
                if animal.state != CritterState::Flee {
                    break;
                }
            }
            assert_eq!(animal.state, CritterState::Rest, "{kind:?} calmed down");
            assert!(
                top >= profile.run.min(profile.cruise),
                "{kind:?} reached {top}"
            );
            assert!(animal.position.distance(rider) >= profile.safe, "{kind:?}");
            assert!(
                animal.position.x < start.x,
                "{kind:?} ran away from the rider"
            );
        }
        // Un ñandú corre mucho más rápido que un peludo, y en línea recta.
        let (nandu, peludo) = (CritterKind::Nandu.profile(), CritterKind::Peludo.profile());
        assert!(nandu.run > 3.0 * peludo.run && nandu.jink == 0.0);
        // Los felinos huyen a sus velocidades de pique, dadas en km/h: el puma
        // a 72 (20 m/s), el más rápido de los tres.
        assert_eq!(kmh(72.0), 20.0);
        let [gato, puma, yagua] = [
            CritterKind::GatoMontes,
            CritterKind::Puma,
            CritterKind::Yaguarete,
        ]
        .map(|k| k.profile().run);
        assert_eq!([gato, puma, yagua], [12.5, 20.0, 65.0 / 3.6]);
    }

    #[test]
    fn a_chased_feline_sprints_its_distance_then_eases_to_its_cruise() {
        for kind in [
            CritterKind::GatoMontes,
            CritterKind::Puma,
            CritterKind::Yaguarete,
            CritterKind::Zorro,
        ] {
            let profile = kind.profile();
            let big = Aabb::from_center_size(Vec2::ZERO, Vec2::new(4000.0, 4000.0));
            let mut animal = Critter::new(kind, Vec2::ZERO, big, None, 5);
            // El jinete lo persigue siempre unos metros detrás.
            let mut rider = Vec2::new(profile.alarm * 0.5, 0.0);
            let start = animal.position;
            let mut slowed_at = None;
            let mut reached = false;
            for _ in 0..1200 {
                animal.step(0.1, Some(rider), |_| true);
                assert_eq!(animal.state, CritterState::Flee, "{kind:?}");
                let gone = animal.position.distance(start);
                if animal.speed == profile.run {
                    reached = true;
                }
                if reached && animal.speed < profile.run && slowed_at.is_none() {
                    slowed_at = Some(gone);
                }
                if reached {
                    assert!(animal.speed >= profile.cruise, "{kind:?}");
                }
                let to = animal.position - rider;
                rider = animal.position - to * (profile.alarm * 0.5 / to.length());
            }
            if kind.feline() {
                let at = slowed_at.expect("slowed down");
                assert!(
                    (at - profile.sprint).abs() < profile.run,
                    "{kind:?} sprinted {at} m"
                );
                assert_eq!(animal.speed, profile.cruise, "{kind:?}");
                assert!(profile.cruise < 0.6 * profile.run, "{kind:?}");
            } else {
                // Los demás huyen siempre a su velocidad.
                assert_eq!(slowed_at, None, "{kind:?}");
                assert_eq!(animal.speed, profile.run, "{kind:?}");
            }
        }
        // El puma pica unos cientos de metros; el yaguareté, que caza al
        // acecho, menos; el gato montés, chico, menos aún.
        let sprint = |k: CritterKind| k.profile().sprint;
        assert!(
            sprint(CritterKind::Puma) > sprint(CritterKind::Yaguarete)
                && sprint(CritterKind::Yaguarete) > sprint(CritterKind::GatoMontes)
        );
    }

    #[test]
    fn every_kind_turns_gradually_walking_and_fleeing() {
        let big = Aabb::from_center_size(Vec2::ZERO, Vec2::new(4000.0, 4000.0));
        let dt = 0.05;
        for kind in CritterKind::ALL {
            let mut animal = Critter::new(kind, Vec2::ZERO, big, None, 11);
            // Deambula un rato, después un jinete aparece justo delante (tiene
            // que dar media vuelta), y luego da vueltas a su alrededor.
            for k in 0..2400 {
                let rider = match k {
                    0..800 => None,
                    800..1400 => Some(animal.position + animal.heading.forward() * 5.0),
                    _ => {
                        let a = k as f32 * 0.01;
                        Some(animal.position + Vec2::new(a.cos(), a.sin()) * 10.0)
                    }
                };
                let before = animal.heading;
                animal.step(dt, rider, |_| true);
                let turned = turn_toward(before, animal.heading.forward()).abs();
                let limit = match animal.state {
                    CritterState::Flee => Critter::FLEE_TURN,
                    _ => Critter::FLEE_TURN.max(Critter::WALK_TURN),
                };
                assert!(
                    turned <= limit * dt + 1e-4,
                    "{kind:?} turned {turned} rad in one step ({:?})",
                    animal.state
                );
            }
        }
    }

    #[test]
    fn every_kind_rounds_closed_ground_without_snapping_its_heading() {
        // Un claro redondo dentro de un chunk chico: el borde le cierra el
        // paso una y otra vez mientras deambula y mientras huye.
        let home = Aabb::from_center_size(Vec2::ZERO, Vec2::new(80.0, 80.0));
        let open = |p: Vec2| p.length() < 30.0;
        let dt = 0.05;
        for kind in CritterKind::ALL {
            let mut animal = Critter::new(kind, Vec2::ZERO, home, None, 5);
            let mut stuck = 0.0;
            for k in 0..3000 {
                let rider = (k >= 600).then(|| {
                    let a = k as f32 * 0.004;
                    Vec2::new(a.cos(), a.sin()) * 12.0
                });
                let (before, before_at) = (animal.heading, animal.position);
                animal.step(dt, rider, open);
                let turned = turn_toward(before, animal.heading.forward()).abs();
                let rate = match animal.state {
                    CritterState::Flee => {
                        Critter::FLEE_TURN.min(Critter::GRIP / animal.speed.max(1e-3))
                    }
                    _ => Critter::WALK_TURN,
                };
                assert!(
                    turned <= rate * dt + 1e-4,
                    "{kind:?} turned {turned} rad in one step at {} m/s ({:?})",
                    animal.speed,
                    animal.state
                );
                assert!(open(animal.position), "{kind:?} left the clearing");
                // Huyendo no se queda trabado contra el borde.
                let moved = animal.position != before_at;
                stuck = if animal.state == CritterState::Flee && !moved {
                    stuck + dt
                } else {
                    0.0
                };
                assert!(stuck < 0.6, "{kind:?} stuck {stuck} s against the edge");
            }
        }
    }

    #[test]
    fn every_kind_roams_its_whole_territory_and_never_beyond() {
        for kind in CritterKind::ALL
            .into_iter()
            .filter(|&k| k != CritterKind::Vizcacha)
        {
            let roam = kind.profile().roam;
            let home = Aabb::from_center_size(Vec2::ZERO, Vec2::new(2.0 * roam, 2.0 * roam));
            let mut animal = Critter::new(kind, Vec2::ZERO, home, None, 7);
            let mut far: f32 = 0.0;
            // Dos horas a solas.
            for _ in 0..72_000 {
                animal.step(0.1, None, |_| true);
                assert!(
                    home.contains_point(animal.position),
                    "{kind:?} left its territory"
                );
                far = far.max(animal.position.length());
            }
            assert!(
                far > 0.6 * roam,
                "{kind:?} only got {far} m out of {roam} m"
            );
        }
        // Los grandes andan más que los chicos.
        let roam = |k: CritterKind| k.profile().roam;
        assert!(roam(CritterKind::Puma) > roam(CritterKind::Yaguarete));
        assert!(roam(CritterKind::Yaguarete) > roam(CritterKind::GatoMontes));
        assert!(roam(CritterKind::Zorro) > roam(CritterKind::Peludo));
    }

    #[test]
    fn a_feline_out_of_the_monte_is_back_within_two_game_hours() {
        assert_eq!(Critter::STRAY, 240.0, "two game hours, in real seconds");
        // El monte es un círculo de 30 m; el territorio, mucho más grande.
        let monte = |p: Vec2| p.length() < 30.0;
        for kind in [
            CritterKind::GatoMontes,
            CritterKind::Puma,
            CritterKind::Yaguarete,
        ] {
            let home = Aabb::from_center_size(Vec2::ZERO, Vec2::new(1000.0, 1000.0));
            let mut cat = Critter::new(kind, Vec2::ZERO, home, None, 9);
            let (dt, mut out, mut left) = (0.1, 0.0_f32, false);
            // Un jinete lo saca corriendo del monte; después lo dejan solo
            // cinco horas de juego.
            for k in 0..6000 {
                let rider = (k < 300).then(|| cat.position - Vec2::new(10.0, 0.0));
                cat.step_in(dt, rider, |_| true, monte);
                out = if monte(cat.position) { 0.0 } else { out + dt };
                left |= out > 0.0;
                assert!(out <= Critter::STRAY, "{kind:?} out of the monte {out} s");
            }
            assert!(left, "{kind:?} never left the monte");
        }
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
