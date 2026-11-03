//! La fauna silvestre de suelo de la pampa de 1919 (ver [`super`]): zorros,
//! ñandúes, mulitas, peludos, vizcachas, gatos monteses, pumas y, muy rara vez,
//! un yaguareté. Cada uno descansa, deambula un poco y huye de un jinete según
//! su especie ([`CritterKind::profile`]); la vizcacha corre a su vizcachera y
//! se mete adentro. Ninguno es un obstáculo. Los que saltan ([`Leap`]: el
//! zorro y los felinos) pasan los alambrados por arriba en vez de esquivarlos
//! y, huyendo, cruzan un camino alambrado ([`Land`]).

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

    /// Cómo salta un alambrado, si lo salta: el zorro y los felinos, que
    /// pasan por encima de uno de 1.3 m sin esfuerzo (un puma salta varios
    /// metros de alto). El arco sube lo justo para que ninguna parte de su
    /// cuerpo pase el alambre a menos de [`Leap::SPARE`] ([`Leap::tuck`],
    /// medido en Blender sobre su clip `Fence_Leap`, un bote largo y chato:
    /// -0.015 m el zorro, 0.011 el gato montés, 0.11 el puma y 0.074 el
    /// yaguareté; arcos de 1.47, 1.45, 1.34 y 1.38 m). El largo es lo que
    /// recorre en el aire a su velocidad, entre un salto casi parado y uno
    /// en carrera, largo y bajo, de unos 5.5 m el zorro y el gato montés, 8 m
    /// el yaguareté y 11 m el puma (lo que avanzan en el aire a su crucero y
    /// a su pique: con el vuelo más largo de este clip, el zorro a 5 m perdía
    /// más del 30 % de su crucero y el puma a 10 m caía, en una esquina, más
    /// allá de donde había mirado). Los armadillos, las vizcachas y los
    /// ñandúes pasan entre los hilos o los esquivan.
    pub const fn leap(self) -> Option<Leap> {
        let (length, tuck) = match self {
            Self::Zorro => ((2.0, 5.5), -0.015),
            Self::GatoMontes => ((2.0, 5.5), 0.011),
            Self::Puma => ((3.0, 11.0), 0.11),
            Self::Yaguarete => ((3.0, 8.0), 0.074),
            _ => return None,
        };
        // El alambre pasa, como muy temprano, en `CLEAR` del arco.
        let rise = 4.0 * Leap::CLEAR * (1.0 - Leap::CLEAR);
        let height = (crate::FenceLayout::HEIGHT + Leap::SPARE - tuck) / rise;
        Some(Leap {
            height,
            length,
            tuck,
        })
    }

    /// Si es un felino del monte.
    pub const fn feline(self) -> bool {
        matches!(self, Self::GatoMontes | Self::Puma | Self::Yaguarete)
    }
}

/// Cómo salta una especie ([`CritterKind::leap`]): un arco de tiro
/// parabólico, con el alambre siempre debajo de su punto más alto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leap {
    /// Lo más alto que suben sus pies, en metros sobre el suelo.
    pub height: f32,
    /// El salto más corto (parado) y el más largo (en carrera), en metros.
    pub length: (f32, f32),
    /// Lo más bajo que pasa su cuerpo sobre el alambre respecto de la altura
    /// de sus pies (el origen del modelo) cuando el animal cruza el alambre,
    /// en metros, resumido para la regla del arco: si `lift() + tuck` pasa
    /// el alambre por [`Leap::SPARE`] con el alambre en [`Leap::CLEAR`] del
    /// salto, la parte más baja del cuerpo lo pasa por lo menos por eso en
    /// todos sus saltos. No es una medida de una pose sino de todo el cruce: reproduciendo el
    /// clip `Fence_Leap` en Blender sobre el arco del juego, cada parte del
    /// cuerpo (las manos estiradas adelante, la cabeza, la panza, las patas
    /// traseras y la cola) pasa sobre el alambre en su momento, más arriba o
    /// más abajo en el arco; la peor, en los saltos que da (de
    /// `length.0` a `length.1`, el alambre en la mitad del salto, o desde
    /// `CLEAR` en los saltos de la mitad más larga), es la que vale. Puede
    /// ser negativa: lo que pasa más bajo lo hace antes o después de lo más
    /// alto del arco.
    pub tuck: f32,
}

impl Leap {
    /// Lo más temprano del salto en que puede pasar el alambre, como fracción
    /// de su largo: ahí el arco ya subió el 96 % de su altura.
    pub const CLEAR: f32 = 0.4;
    /// Lo que ocupa un alambrado a lo ancho, con margen (un tramo en diagonal
    /// ocupa más que su espesor), en metros: más allá, otro alambre es otro
    /// alambrado, y no cae encima de uno.
    pub const WIRE: f32 = 0.5;
    /// Lo que le sobra por encima del alambre a lo más bajo de su cuerpo, en
    /// metros.
    pub const SPARE: f32 = 0.1;

    /// Cuántos segundos pasa en el aire: lo que tarda en subir a
    /// [`Self::height`] y volver a caer.
    pub fn flight(&self) -> f32 {
        2.0 * (2.0 * self.height / crate::Rider::GRAVITY).sqrt()
    }

    /// Cuánto salta corriendo a `speed` m/s, en metros: lo que avanza en el
    /// aire, entre [`Self::length`].
    pub fn reach(&self, speed: f32) -> f32 {
        (speed * self.flight()).clamp(self.length.0, self.length.1)
    }

    /// Lo más rápido que avanza en el aire, en m/s: llega al alambre frenando
    /// hasta acá.
    pub fn top(&self) -> f32 {
        self.length.1 / self.flight()
    }
}

/// Lo que un animal de suelo sabe del terreno que pisa ([`Critter::step_on`]).
pub trait Land {
    /// Si puede pisar `p` (pasto abierto).
    fn open(&self, p: Vec2) -> bool;

    /// Si `p` es la franja de un camino, entre sus alambrados: cerrada para
    /// el que deambula, pero el que salta alambrados la cruza huyendo.
    fn road(&self, p: Vec2) -> bool {
        let _ = p;
        false
    }

    /// Dónde cruza el tramo de `from` a `to` el primer alambre (o tranquera
    /// cerrada), como fracción del tramo; `None` si no cruza ninguno.
    fn wire(&self, from: Vec2, to: Vec2) -> Option<f32> {
        let _ = (from, to);
        None
    }
}

/// Un terreno sin caminos ni alambrados: abierto donde lo dice su función.
struct Open<F>(F);

impl<F: Fn(Vec2) -> bool> Land for Open<F> {
    fn open(&self, p: Vec2) -> bool {
        (self.0)(p)
    }
}

/// Un salto en curso.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flight {
    from: Vec2,
    to: Vec2,
    height: f32,
    /// Los segundos que dura y los que lleva en el aire.
    time: f32,
    spent: f32,
}

impl Flight {
    /// Qué parte del salto lleva, de 0 a 1.
    fn progress(&self) -> f32 {
        (self.spent / self.time).min(1.0)
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
    /// El salto en el que está en el aire.
    flight: Option<Flight>,
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
            flight: None,
            timer: rng.range(1.0, 8.0),
            rng,
        }
    }

    /// A qué altura del suelo está, en metros: bajo tierra dentro de su cueva,
    /// en el aire sobre el arco de un salto.
    pub fn lift(&self) -> f32 {
        if let Some(f) = self.flight {
            let u = f.progress();
            return 4.0 * f.height * u * (1.0 - u);
        }
        if self.state == CritterState::Den {
            -Self::DEN_DEPTH
        } else {
            0.0
        }
    }

    /// Si está en el aire, saltando un alambrado.
    pub fn airborne(&self) -> bool {
        self.flight.is_some()
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
        self.step_on(dt, threat, &Open(open), habitat)
    }

    /// Como [`Self::step_in`], sobre `land`, con sus caminos y alambrados. El
    /// que no salta ([`CritterKind::leap`]) pisa solo lo abierto y no ve los
    /// alambres (pasa entre los hilos). El que salta nunca los atraviesa:
    /// cuando uno le queda a medio salto por delante, despega y lo pasa por
    /// arriba con el alambre bajo lo más alto del arco, si del otro lado
    /// puede pisar; huyendo frena antes hasta su velocidad en el aire
    /// ([`Leap::top`]) y cruza la franja de un camino ([`Land::road`]), por
    /// la que nunca deambula (salvo para salir de ella si quedó adentro).
    pub fn step_on(
        &mut self,
        dt: f32,
        threat: Option<Vec2>,
        land: &impl Land,
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
        // En el aire sigue su arco sin mirar al jinete.
        if self.flight.is_some() {
            self.fly(dt);
            return;
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
        // Lo que puede pisar: el pasto abierto y, si salta alambrados, la
        // franja de un camino cuando huye o para salir de ella.
        let leap = self.kind.leap();
        let crossing =
            leap.is_some() && (self.state == CritterState::Flee || !land.open(self.position));
        let open = |p| land.open(p) || (crossing && land.road(p));
        // Mira un trecho adelante: si por ahí el paso está cerrado, empieza a
        // doblar desde antes en vez de pegar el volantazo contra el borde.
        // Cuanto más rápido corre, más lejos mira: lo que avanza en `LOOK` más
        // lo que le lleva doblar a esa velocidad; parado, un par de metros.
        let reach = (self.speed * Self::LOOK + self.speed * self.speed / Self::GRIP).max(2.0);
        // El que salta no encara un alambre que no puede saltar: tan cerca que
        // no le da para un salto a su velocidad (corriendo pegado a él, por
        // ejemplo) o con la caída del otro lado cerrada o sobre otro alambre
        // (una esquina); lo bordea hasta poder saltarlo o alejarse.
        let jumpable = |h: Heading| {
            let Some(leap) = leap else {
                return true;
            };
            let span = leap.reach(self.speed);
            let ahead = h.forward();
            let far = reach.max(span);
            let Some(at) = land.wire(self.position, self.position + ahead * far) else {
                return true;
            };
            if at * far < Leap::CLEAR * span {
                return false;
            }
            let over = self.position + ahead * (at * far);
            let to = over + ahead * (span * 0.5);
            self.home.contains_point(to)
                && open(to)
                && land.wire(over + ahead * Leap::WIRE, to).is_none()
        };
        let way = [reach, reach * 0.5, reach * 0.25]
            .into_iter()
            .find_map(|far| {
                DETOURS
                    .iter()
                    .map(|&t| goal.rotated(t))
                    .find(|&h| self.clear(h, far, open) && jumpable(h))
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
            let mut brake = if corner < want.min(self.speed) {
                want = corner;
                Self::BRAKE
            } else {
                Self::EASE
            };
            // Frena antes de un alambrado hasta lo que avanza en el aire, para
            // llegar a despegar a esa velocidad.
            // Mira hacia donde va y hacia donde está doblando.
            let wire = [self.heading, way]
                .into_iter()
                .filter_map(|h| land.wire(self.position, self.position + h.forward() * reach))
                .min_by(f32::total_cmp);
            if let Some((leap, at)) = leap.zip(wire) {
                // Con un paso de margen, para no despegar todavía frenando.
                let room = (at * reach - leap.length.1 * 0.5 - self.speed * dt).max(0.0);
                let fence = (leap.top() * leap.top() + 2.0 * Self::BRAKE * room).sqrt();
                if fence < want {
                    want = fence;
                    brake = Self::BRAKE;
                }
            }
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
        let forward = self.heading.forward();
        if let Some(leap) = leap {
            // Un alambre a medio salto por delante: despega con el alambre
            // cerca de la mitad del salto (alargándolo si viene más rápido de
            // lo que avanzaría en el aire, sin dejar el alambre fuera de la
            // parte alta del arco), si del otro lado puede pisar; si no, el
            // alambre lo frena como un borde.
            let far = leap.reach(self.speed) * 0.5 + step;
            if let Some(at) = land.wire(self.position, self.position + forward * far) {
                let near = (at * far).max(0.25);
                let time = leap.flight();
                let length = (self.speed * time).clamp(2.0 * near, near / Leap::CLEAR);
                let to = self.position + forward * length;
                let past = self.position + forward * (at * far + Leap::WIRE).min(length);
                if self.home.contains_point(to) && open(to) && land.wire(past, to).is_none() {
                    self.speed = length / time;
                    self.flight = Some(Flight {
                        from: self.position,
                        to,
                        height: leap.height,
                        time,
                        spent: 0.0,
                    });
                    self.fly(dt);
                    return;
                }
                if at * far <= step {
                    if self.state == CritterState::Flee {
                        self.speed = 0.0;
                    }
                    return;
                }
            }
        }
        let next = self.position + forward * step;
        if self.home.contains_point(next) && open(next) {
            self.fled += step;
            self.position = next;
        } else if self.state == CritterState::Flee {
            // Se topó con el borde sin terminar de doblar: frena en seco y
            // pivotea ahí mismo hasta encarar el paso abierto.
            self.speed = 0.0;
        }
    }

    /// Avanza `dt` segundos por el arco de su salto; al caer sigue andando o
    /// huyendo desde donde cayó.
    fn fly(&mut self, dt: f32) {
        let Some(flight) = self.flight.as_mut() else {
            return;
        };
        flight.spent += dt;
        let u = flight.progress();
        let next = flight.from + (flight.to - flight.from) * u;
        self.fled += next.distance(self.position);
        self.position = next;
        if u >= 1.0 {
            self.flight = None;
            if self.state == CritterState::Walk {
                self.speed = self.kind.profile().walk;
            }
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

    /// Pasto con alambrados en las líneas `x` y `y` de `wires` y, entre
    /// `road.0` y `road.1` de `x`, la franja de un camino.
    struct Fenced {
        wires: Vec<f32>,
        rows: Vec<f32>,
        road: Option<(f32, f32)>,
    }

    impl Land for Fenced {
        fn open(&self, p: Vec2) -> bool {
            !self.road(p)
        }

        fn road(&self, p: Vec2) -> bool {
            self.road.is_some_and(|(a, b)| p.x >= a && p.x <= b)
        }

        fn wire(&self, from: Vec2, to: Vec2) -> Option<f32> {
            let cross = |a: f32, b: f32, w: f32| {
                ((a - w) * (b - w) <= 0.0 && a != b).then(|| (w - a) / (b - a))
            };
            self.wires
                .iter()
                .map(|&w| cross(from.x, to.x, w))
                .chain(self.rows.iter().map(|&w| cross(from.y, to.y, w)))
                .flatten()
                .min_by(f32::total_cmp)
        }
    }

    /// Avanza `animal` un paso y comprueba que, si cruzó un alambre de
    /// `land`, lo hizo en el aire por encima de él.
    fn step_over(animal: &mut Critter, dt: f32, rider: Option<Vec2>, land: &Fenced) -> bool {
        let (at, lift) = (animal.position, animal.lift());
        animal.step_on(dt, rider, land, |_| true);
        let crossed = land.wire(at, animal.position).is_some();
        if crossed {
            // Lo más bajo de su cuerpo, con las patas recogidas.
            let low = lift.min(animal.lift()) + animal.kind.leap().map_or(0.0, |l| l.tuck);
            assert!(
                low > crate::FenceLayout::HEIGHT,
                "{:?} crossed a wire {low} m up",
                animal.kind
            );
        }
        crossed
    }

    #[test]
    fn leapers_jump_wire_fences_in_an_arc_and_never_go_through_them() {
        let leapers = [
            CritterKind::Zorro,
            CritterKind::GatoMontes,
            CritterKind::Puma,
            CritterKind::Yaguarete,
        ];
        for kind in CritterKind::ALL {
            assert_eq!(kind.leap().is_some(), leapers.contains(&kind), "{kind:?}");
        }
        // Una cuadrícula de potreros de 40 m.
        let lines: Vec<f32> = (-50..50).map(|k| k as f32 * 40.0 + 20.0).collect();
        let land = Fenced {
            wires: lines.clone(),
            rows: lines,
            road: None,
        };
        let big = Aabb::from_center_size(Vec2::ZERO, Vec2::new(4000.0, 4000.0));
        let dt = 0.05;
        for kind in leapers {
            let profile = kind.profile();
            let mut animal = Critter::new(kind, Vec2::ZERO, big, None, 3);
            let mut rider = Vec2::new(-profile.alarm * 0.5, 0.0);
            let (mut jumps, mut top, mut stuck) = (0, 0.0_f32, 0.0);
            // Huye de un jinete que lo persigue, y después anda solo.
            for k in 0..4000 {
                let chased = k < 1200;
                let before = (animal.state, animal.speed);
                let (heading, at) = (animal.heading, animal.position);
                jumps += usize::from(step_over(&mut animal, dt, chased.then_some(rider), &land));
                // Ni volantazos ni trabado contra un alambrado.
                let turned = turn_toward(heading, animal.heading.forward()).abs();
                assert!(
                    turned <= Critter::FLEE_TURN * dt + 1e-4,
                    "{kind:?} turned {turned}"
                );
                stuck = if animal.state == CritterState::Flee && animal.position == at {
                    stuck + dt
                } else {
                    0.0
                };
                assert!(stuck < 0.6, "{kind:?} stuck {stuck} s at a fence");
                top = top.max(animal.lift());
                // Llega al alambrado frenando, sin perder velocidad de golpe (a
                // lo sumo lo que frena en dos pasos, si dobla hacia el alambre
                // ya encima de él).
                if before.0 == CritterState::Flee && animal.state == CritterState::Flee {
                    assert!(
                        before.1 - animal.speed <= 2.0 * Critter::BRAKE * dt,
                        "{kind:?} dropped from {} to {} m/s",
                        before.1,
                        animal.speed
                    );
                }
                let to = animal.position - rider;
                if chased && to.length() > 0.0 {
                    rider = animal.position - to * (profile.alarm * 0.5 / to.length());
                }
            }
            assert!(jumps >= 3, "{kind:?} jumped {jumps} fences");
            let leap = kind.leap().unwrap();
            assert!((top - leap.height).abs() < 0.05, "{kind:?} rose {top} m");
            // Pasa el alambre por un palmo, no por un metro: un globo no.
            let spare = top + leap.tuck - crate::FenceLayout::HEIGHT;
            assert!(
                (Leap::SPARE..0.2).contains(&spare),
                "{kind:?} clears the wire by {spare} m"
            );
            assert!(!animal.airborne() || animal.lift() > 0.0);
        }
        // El puma salta más largo que el gato montés, y todos pasan
        // alrededor de un segundo en el aire, lo que tarda en caer de un
        // metro y pico.
        let leap = |k: CritterKind| k.leap().unwrap();
        assert!(leap(CritterKind::Puma).length.1 > leap(CritterKind::GatoMontes).length.1);
        for kind in leapers {
            let t = leap(kind).flight();
            assert!((0.9..1.1).contains(&t), "{kind:?} flies {t} s");
        }
    }

    #[test]
    fn a_fleeing_leaper_keeps_most_of_its_cruise_over_a_fence() {
        let big = Aabb::from_center_size(Vec2::ZERO, Vec2::new(4000.0, 4000.0));
        let dt = 0.02;
        for kind in [
            CritterKind::Zorro,
            CritterKind::GatoMontes,
            CritterKind::Puma,
            CritterKind::Yaguarete,
        ] {
            let profile = kind.profile();
            // Un alambrado bien pasado su pique, cuando ya va a su crucero.
            let wire = if profile.sprint.is_finite() {
                profile.sprint + 80.0
            } else {
                100.0
            };
            let land = Fenced {
                wires: vec![wire],
                rows: Vec::new(),
                road: None,
            };
            let mut animal = Critter::new(kind, Vec2::ZERO, big, None, 2);
            let mut rider = Vec2::new(-profile.alarm * 0.5, 0.0);
            let (mut approach, mut air) = (0.0, None);
            for _ in 0..3000 {
                let was = animal.airborne();
                step_over(&mut animal, dt, Some(rider), &land);
                if animal.position.x < wire - 30.0 {
                    approach = animal.speed;
                }
                if animal.airborne() && !was && air.is_none() {
                    air = Some(animal.speed);
                }
                rider = animal.position - Vec2::new(profile.alarm * 0.5, 0.0);
            }
            let air = air.expect("jumped");
            // Conserva al menos el 70 % de su crucero.
            assert!(
                air >= 0.7 * approach,
                "{kind:?} leapt at {air} m/s after running at {approach}"
            );
        }
    }

    #[test]
    fn fleeing_leapers_cross_a_fenced_road_and_the_others_keep_off_it() {
        // Un camino de norte a sur: su franja de 29 m entre `x` 20 y 49, con
        // un alambrado a cada lado, a 8.5 m de su eje.
        let land = Fenced {
            wires: vec![26.0, 43.0],
            rows: Vec::new(),
            road: Some((20.0, 49.0)),
        };
        let big = Aabb::from_center_size(Vec2::ZERO, Vec2::new(4000.0, 4000.0));
        let dt = 0.05;
        for kind in CritterKind::ALL {
            let profile = kind.profile();
            let mut animal = Critter::new(kind, Vec2::ZERO, big, None, 5);
            // Primero anda solo; después un jinete lo corre hacia el camino.
            for _ in 0..1200 {
                step_over(&mut animal, dt, None, &land);
                assert!(
                    land.open(animal.position),
                    "{kind:?} wandered onto the road"
                );
            }
            let mut rider = animal.position - Vec2::new(profile.alarm * 0.5, 0.0);
            for _ in 0..1200 {
                step_over(&mut animal, dt, Some(rider), &land);
                if kind.leap().is_none() {
                    assert!(land.open(animal.position), "{kind:?} ran onto the road");
                }
                let to = animal.position - rider;
                rider = animal.position - to * (profile.alarm * 0.5 / to.length());
            }
            if kind.leap().is_some() {
                assert!(
                    animal.position.x > 49.0,
                    "{kind:?} stopped at {:?}",
                    animal.position
                );
            }
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
