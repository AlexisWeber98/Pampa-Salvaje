//! Los pájaros chicos de suelo: el gorrión (*Passer domesticus*), en bandadas
//! de los pueblos, y el chingolo (*Zonotrichia capensis*), solo o en pareja en
//! el campo. Viven abajo: comen en el suelo a saltitos y picotazos, se posan en
//! los pretiles de las casas, en los postes y en los alambres, y vuelan cuando
//! se les acerca el jugador.
//!
//! Cada grupo ([`SparrowGroup`]) tiene su *comedero* ([`Patch`]: un pedazo de
//! vereda, de calle o del borde de un campo) y sus *posaderos* ([`Spot`]: los
//! pretiles de las casas cercanas, o los postes y el alambre de arriba del
//! alambrado de al lado), sembrados por [`place`]. El grupo alterna entre
//! comer en el suelo y descansar posado; cada pájaro, en el suelo, encadena
//! picotazos ([`SparrowClip::Peck`]), rachas de saltitos
//! ([`SparrowClip::Hop`], que lo mueven solo mientras está en el aire,
//! [`hop_progress`]) y ratos parado mirando alrededor
//! ([`SparrowClip::Idle`]). Cuando el jugador llega a menos de
//! [`SparrowKind::flush`] de uno, vuela todo el grupo: al posadero más cercano
//! que quede lejos del jugador o, si no hay ninguno, lejos, fuera de la vista;
//! después de un rato, con el jugador más allá de [`SparrowKind::calm`],
//! vuelve a bajar a comer. Cada pájaro reproduce sus clips a su propio ritmo
//! ([`Sparrow::pace`]) y empieza en su propio punto, así que la bandada nunca
//! se mueve al unísono.

pub mod place;

use crate::rng::Rng;
use crate::{Aabb, Heading, Vec2};

/// Qué pájaro es.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SparrowKind {
    /// El gorrión, en bandadas en las calles, veredas y techos de los pueblos.
    Gorrion,
    /// El chingolo, solo o en pareja en los alambrados y los bordes del campo.
    Chingolo,
}

impl SparrowKind {
    pub const ALL: [Self; 2] = [Self::Gorrion, Self::Chingolo];

    /// A qué distancia del jugador (en tres dimensiones, desde su cabeza) un
    /// pájaro levanta vuelo, en metros. Estimaciones, no medidas: el gorrión
    /// vive entre la gente y aguanta más cerca; el chingolo del campo abierto
    /// es más arisco.
    pub const fn flush(self) -> f32 {
        match self {
            Self::Gorrion => 4.5,
            Self::Chingolo => 7.0,
        }
    }

    /// Más allá de esta distancia del jugador (desde el comedero) el grupo
    /// vuelve a bajar a comer, en metros.
    pub const fn calm(self) -> f32 {
        3.0 * self.flush()
    }

    /// Cuántos pájaros tiene un grupo, como mínimo y como máximo.
    pub const fn group_size(self) -> (usize, usize) {
        match self {
            Self::Gorrion => (3, 12),
            Self::Chingolo => (1, 2),
        }
    }

    /// Cuánto avanza un saltito, en metros: `tools/sparrow.py`'s `HOP_LENGTH`
    /// (2.6 medios largos del torso, de 32 y 29 mm).
    pub const fn hop_length(self) -> f32 {
        match self {
            Self::Gorrion => 0.083,
            Self::Chingolo => 0.075,
        }
    }

    /// Lo que dura `Flap` (tres aletazos) y `FlapBound` (cuatro y el bote),
    /// en segundos: `tools/sparrow.py`, a 17.0 y 17.9 aletazos por segundo.
    pub const fn flight_seconds(self) -> (f32, f32) {
        match self {
            Self::Gorrion => (0.183, 0.364),
            Self::Chingolo => (0.167, 0.342),
        }
    }
}

/// Los clips de los modelos (`tools/sparrow.py`), en el orden del GLB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SparrowClip {
    /// Aletazos: al despegar y al posarse.
    Flap,
    /// El vuelo ondulante de los pájaros chicos: una racha de aletazos y un
    /// bote con las alas cerradas.
    FlapBound,
    /// Un saltito con las dos patas, en el lugar.
    Hop,
    /// Dos picotazos al suelo.
    Peck,
    /// Parado: giros chicos de la cabeza y de la cola.
    Idle,
}

impl SparrowClip {
    pub const ALL: [Self; 5] = [
        Self::Flap,
        Self::FlapBound,
        Self::Hop,
        Self::Peck,
        Self::Idle,
    ];

    /// Su nombre en el GLB.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Flap => "Flap",
            Self::FlapBound => "FlapBound",
            Self::Hop => "Hop",
            Self::Peck => "Peck",
            Self::Idle => "Idle",
        }
    }

    /// Lo que dura, en segundos, a ritmo normal.
    pub const fn seconds(self, kind: SparrowKind) -> f32 {
        match self {
            Self::Flap => kind.flight_seconds().0,
            Self::FlapBound => kind.flight_seconds().1,
            Self::Hop => HOP_SECONDS,
            Self::Peck => PECK_SECONDS,
            Self::Idle => IDLE_SECONDS,
        }
    }
}

/// `Hop`, `Peck` e `Idle`, en segundos (`tools/sparrow.py`).
pub const HOP_SECONDS: f32 = 0.32;
pub const PECK_SECONDS: f32 = 1.4;
pub const IDLE_SECONDS: f32 = 3.0;
/// La parte de `Hop` (en fracciones del clip) con las patas en el aire:
/// `tools/sparrow.py`'s `hop_airborne()`.
pub const HOP_AIRBORNE: (f32, f32) = (0.342, 0.698);

/// Cuánto de un saltito avanzó el pájaro en la fracción `phase` del clip
/// (`0..=1`): nada mientras se agacha y empuja, todo una vez que aterrizó, y
/// en el aire, parejo.
pub fn hop_progress(phase: f32) -> f32 {
    let (a, b) = HOP_AIRBORNE;
    ((phase - a) / (b - a)).clamp(0.0, 1.0)
}

/// Sobre qué está un pájaro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perch {
    /// En el suelo de una calle.
    Street,
    /// En el suelo de una vereda.
    Sidewalk,
    /// En el suelo del campo: el borde de un potrero o la banquina.
    Verge,
    /// En el pretil del frente de una casa.
    Roof,
    /// En la punta de un poste de alambrado.
    Post,
    /// En el alambre de arriba.
    Wire,
}

impl Perch {
    /// Si es el suelo (donde se come), y no un posadero.
    pub const fn ground(self) -> bool {
        matches!(self, Self::Street | Self::Sidewalk | Self::Verge)
    }
}

/// Un posadero: un tramo recto de pretil o de alambre, o la punta de un poste.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    /// Su medio, en el suelo.
    pub at: Vec2,
    /// Su altura en `at` (donde apoyan las patas), en metros.
    pub y: f32,
    pub perch: Perch,
    /// Hacia dónde corre (unitario).
    pub along: Vec2,
    /// Cuánto sube por metro a lo largo de `along` (un alambre en una cuesta).
    pub slope: f32,
    /// Hasta cuánto de `at` a lo largo de `along` se posan, en metros (0: un
    /// solo lugar, la punta de un poste).
    pub reach: f32,
    /// Hacia dónde miran los que se posan (en un pretil, hacia la calle).
    pub facing: Heading,
    /// Hasta cuánto de su línea, de través, se posan, en metros (en un
    /// pretil, más adelante o más atrás sobre la albardilla; 0 en un alambre
    /// o un poste).
    pub depth: f32,
}

impl Spot {
    /// El punto a `offset` metros de `at` a lo largo de él, y su altura.
    pub fn place(&self, offset: f32) -> (Vec2, f32) {
        (self.at + self.along * offset, self.y + self.slope * offset)
    }
}

/// Un comedero: dónde comen en el suelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Patch {
    pub area: Aabb,
    /// Qué suelo es ([`Perch::ground`]).
    pub perch: Perch,
}

/// Velocidad de vuelo, en m/s (unos 25 km/h: vuelos cortos de un pájaro
/// chico).
pub const FLY_SPEED: f32 = 7.0;
/// Lo que aletea sin botes al despegar y al posarse, en segundos.
pub const TAKEOFF: f32 = 0.3;
/// Hasta dónde se van, fuera de la vista, cuando no queda ningún posadero
/// lejos del jugador, en metros, y a qué altura sobre el comedero.
pub const AWAY: (f32, f32) = (45.0, 9.0);
/// La separación mínima entre los medios de dos pájaros posados en un mismo
/// pretil o alambre, en metros: un cuerpo de ancho (de costado, más o menos
/// lo que mide de ancho con las alas plegadas y un poco más).
pub const PERCH_SPACING: f32 = 0.08;

/// Cuánto descansan posados después de un susto o por gusto, y cuánto comen
/// antes de subir un rato, en segundos.
const REST: (f32, f32) = (12.0, 30.0);
const FEED: (f32, f32) = (40.0, 100.0);
/// La cabeza del jugador sobre el suelo, en metros.
pub const THREAT_HEIGHT: f32 = 1.6;

/// Un vuelo de un lugar a otro.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Flight {
    from: (Vec2, f32),
    to: (Vec2, f32),
    /// Dónde se posa al llegar (`None`: se va, fuera de la vista).
    lands: Option<Perch>,
    facing: Heading,
    /// Lo que espera antes de despegar, en segundos.
    delay: f32,
    seconds: f32,
}

/// Lo que está haciendo un pájaro.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Act {
    Stand {
        until: f32,
    },
    Peck {
        until: f32,
    },
    Hops {
        from: Vec2,
        dir: Vec2,
        hops: u32,
    },
    Fly(Flight),
    /// Se fue lejos, fuera de la vista.
    Gone,
}

/// Un pájaro.
#[derive(Debug, Clone, PartialEq)]
pub struct Sparrow {
    pub kind: SparrowKind,
    pub at: Vec2,
    /// La altura de sus patas (en vuelo, de su recorrido), en metros.
    pub y: f32,
    pub heading: Heading,
    /// Sobre qué está; `None` en el aire.
    pub perch: Option<Perch>,
    /// A qué ritmo reproduce sus clips (cerca de 1).
    pub pace: f32,
    act: Act,
    /// Segundos desde que empezó `act`.
    clock: f32,
    rng: Rng,
}

impl Sparrow {
    fn new(kind: SparrowKind, at: Vec2, y: f32, perch: Perch, heading: Heading, seed: u32) -> Self {
        let mut rng = Rng::new(seed, 0x5a77);
        let pace = rng.range(0.9, 1.1);
        let mut bird = Self {
            kind,
            at,
            y,
            heading,
            perch: Some(perch),
            pace,
            act: Act::Stand { until: 0.0 },
            clock: 0.0,
            rng,
        };
        bird.act = bird.rest_act();
        // Cada uno empieza en su propio punto de lo que hace.
        bird.clock = match bird.act {
            Act::Stand { until } | Act::Peck { until } => bird.rng.range(0.0, until),
            _ => 0.0,
        };
        bird
    }

    /// El clip que reproduce ahora.
    pub fn clip(&self) -> SparrowClip {
        match self.act {
            Act::Stand { .. } | Act::Gone => SparrowClip::Idle,
            Act::Peck { .. } => SparrowClip::Peck,
            Act::Hops { .. } => SparrowClip::Hop,
            Act::Fly(f) => match self.flight_time(&f) {
                t if t < 0.0 => SparrowClip::Idle,
                t if bounding(&f, t) => SparrowClip::FlapBound,
                _ => SparrowClip::Flap,
            },
        }
    }

    /// Por dónde va del clip que reproduce desde que lo empezó, en segundos
    /// del clip (a su ritmo): el dibujo lo arranca desde ahí cuando cambia.
    pub fn clip_time(&self) -> f32 {
        let elapsed = match self.act {
            Act::Fly(f) => {
                let t = self.flight_time(&f);
                if t < 0.0 {
                    self.clock
                } else if bounding(&f, t) {
                    t - TAKEOFF
                } else if t >= f.seconds - TAKEOFF {
                    t - (f.seconds - TAKEOFF)
                } else {
                    t
                }
            }
            _ => self.clock,
        };
        elapsed * self.pace
    }

    /// Si está en el aire (volando o saltando).
    pub fn airborne(&self) -> bool {
        self.perch.is_none()
            || matches!(self.act, Act::Hops { .. })
                && hop_progress(self.hop_phase()) > 0.0
                && hop_progress(self.hop_phase()) < 1.0
    }

    /// Si se fue lejos y no se dibuja.
    pub fn gone(&self) -> bool {
        self.act == Act::Gone
    }

    /// Si está volando, o por despegar (ya asustado).
    pub fn flying(&self) -> bool {
        matches!(self.act, Act::Fly(_) | Act::Gone)
    }

    fn flight_time(&self, f: &Flight) -> f32 {
        self.clock - f.delay
    }

    /// Lo que dura un saltito a su ritmo, en segundos.
    fn hop_seconds(&self) -> f32 {
        HOP_SECONDS / self.pace
    }

    /// La fracción del saltito en curso.
    fn hop_phase(&self) -> f32 {
        (self.clock / self.hop_seconds()).fract()
    }

    /// Algo para hacer parado donde está: en un posadero, mirar alrededor; en
    /// el suelo, picotear o mirar alrededor (los saltitos los elige
    /// [`Self::ground_act`]).
    fn rest_act(&mut self) -> Act {
        let clips = 1 + self.rng.below(2);
        let idle = Act::Stand {
            until: clips as f32 * IDLE_SECONDS / self.pace,
        };
        if self.perch.is_some_and(Perch::ground) && self.rng.unit() < 0.6 {
            let clips = 1 + self.rng.below(3);
            return Act::Peck {
                until: clips as f32 * PECK_SECONDS / self.pace,
            };
        }
        idle
    }

    /// Lo siguiente en el suelo del comedero `area`: picotear, mirar o una
    /// racha de saltitos que no salga de él.
    fn ground_act(&mut self, area: &Aabb) -> Act {
        if self.rng.unit() >= 0.4 {
            return self.rest_act();
        }
        let hops = 1 + self.rng.below(5) as u32;
        let length = hops as f32 * self.kind.hop_length();
        let inner = area.expanded(-0.1);
        let mut dir = self.heading.rotated(self.rng.range(-0.9, 0.9)).forward();
        if !inner.contains_point(self.at + dir * length) {
            let to = inner.center() - self.at;
            let len = to.length();
            if len > 1e-3 {
                let toward = Heading::from_radians(to.y.atan2(to.x));
                dir = toward.rotated(self.rng.range(-0.4, 0.4)).forward();
            }
        }
        self.heading = Heading::from_radians(dir.y.atan2(dir.x));
        Act::Hops {
            from: self.at,
            dir,
            hops,
        }
    }

    /// Empieza un vuelo hasta `to`, después de `delay` segundos.
    fn fly(&mut self, to: (Vec2, f32), lands: Option<Perch>, facing: Heading, delay: f32) {
        let from = (self.at, self.y);
        let d = to.0 - from.0;
        let span = (d.length().powi(2) + (to.1 - from.1).powi(2)).sqrt();
        self.act = Act::Fly(Flight {
            from,
            to,
            lands,
            facing,
            delay,
            seconds: (span / FLY_SPEED).max(0.8),
        });
        self.clock = 0.0;
    }

    /// Avanza `dt` segundos; `ground` es la altura del suelo.
    fn step(&mut self, dt: f32, area: &Aabb, ground: &impl Fn(Vec2) -> f32) {
        self.clock += dt;
        match self.act {
            Act::Stand { until } | Act::Peck { until } => {
                if self.clock >= until {
                    self.clock -= until;
                    self.act = if self.perch.is_some_and(Perch::ground) {
                        self.ground_act(area)
                    } else {
                        self.rest_act()
                    };
                }
            }
            Act::Hops { from, dir, hops } => {
                let hop = self.hop_seconds();
                let done = (self.clock / hop).min(hops as f32);
                let whole = done.floor();
                let part = if whole >= hops as f32 {
                    0.0
                } else {
                    hop_progress(done - whole)
                };
                self.at = from + dir * ((whole + part) * self.kind.hop_length());
                self.y = ground(self.at);
                if self.clock >= hops as f32 * hop {
                    self.clock -= hops as f32 * hop;
                    self.act = self.rest_act();
                }
            }
            Act::Fly(f) => {
                let t = self.flight_time(&f);
                if t < 0.0 {
                    return;
                }
                self.perch = None;
                let u = (t / f.seconds).min(1.0);
                let s = u * u * (3.0 - 2.0 * u);
                let d = f.to.0 - f.from.0;
                let arc = (0.3 + 0.15 * d.length()).min(3.0);
                self.at = f.from.0 + d * s;
                self.y = f.from.1 + (f.to.1 - f.from.1) * s + arc * 4.0 * u * (1.0 - u);
                if d.length() > 0.05 && u < 0.9 {
                    self.heading = Heading::from_radians(d.y.atan2(d.x));
                }
                if u >= 1.0 {
                    self.clock = 0.0;
                    match f.lands {
                        Some(perch) => {
                            self.perch = Some(perch);
                            self.heading = f.facing;
                            self.act = self.rest_act();
                        }
                        None => self.act = Act::Gone,
                    }
                }
            }
            Act::Gone => {}
        }
    }
}

/// Si en el segundo `t` de su vuelo `f` el pájaro va en vuelo ondulante: entre
/// los aletazos de despegar y de posarse, en un vuelo largo.
fn bounding(f: &Flight, t: f32) -> bool {
    f.seconds >= 3.0 * TAKEOFF && t >= TAKEOFF && t < f.seconds - TAKEOFF
}

/// Qué hace un grupo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Come en el suelo del comedero.
    Feeding,
    /// Descansa posado (o se fue, asustado).
    Resting,
}

/// Una bandada de gorriones, o un chingolo o una pareja.
#[derive(Debug, Clone, PartialEq)]
pub struct SparrowGroup {
    pub kind: SparrowKind,
    /// Su identidad estable en el mundo.
    pub id: u64,
    pub patch: Patch,
    /// Sus posaderos (al menos uno).
    pub perches: Vec<Spot>,
    pub birds: Vec<Sparrow>,
    mode: Mode,
    /// Segundos que le quedan a `mode`.
    timer: f32,
    rng: Rng,
}

impl SparrowGroup {
    /// Un grupo de `kind` con su comedero y sus posaderos, sembrado con
    /// `seed`: unos empiezan comiendo, otros posados; `ground` es la altura
    /// del suelo.
    pub fn new(
        kind: SparrowKind,
        id: u64,
        patch: Patch,
        perches: Vec<Spot>,
        seed: u32,
        ground: &impl Fn(Vec2) -> f32,
    ) -> Self {
        let mut rng = Rng::new(seed, 0x6e57);
        let (lo, hi) = kind.group_size();
        let count = lo + rng.below(hi - lo + 1);
        let resting = rng.unit() < 0.4 && !perches.is_empty();
        let mut group = Self {
            kind,
            id,
            patch,
            perches,
            birds: Vec::with_capacity(count),
            mode: if resting {
                Mode::Resting
            } else {
                Mode::Feeding
            },
            timer: 0.0,
            rng,
        };
        let seats = if resting {
            let order = group.order_from(patch.area.center());
            group.seats(&order, count)
        } else {
            Vec::new()
        };
        for k in 0..count {
            let seed = group.rng.next() ^ k as u32;
            let bird = if let Some(&(at, y, perch, facing)) = seats.get(k) {
                Sparrow::new(kind, at, y, perch, facing, seed)
            } else {
                let at = group.patch_point();
                let heading = Heading::from_radians(group.rng.range(0.0, std::f32::consts::TAU));
                Sparrow::new(kind, at, ground(at), patch.perch, heading, seed)
            };
            group.birds.push(bird);
        }
        group.timer = if resting {
            group.rng.range(REST.0, REST.1)
        } else {
            group.rng.range(FEED.0, FEED.1)
        };
        group
    }

    /// Si come en el suelo (y no descansa posado ni se fue).
    pub fn feeding(&self) -> bool {
        self.mode == Mode::Feeding
    }

    /// Un punto al azar del comedero.
    fn patch_point(&mut self) -> Vec2 {
        let a = self.patch.area.expanded(-0.1);
        Vec2::new(
            self.rng.range(a.min.x, a.max.x),
            self.rng.range(a.min.y, a.max.y),
        )
    }

    /// Los posaderos, del más cercano a `p` al más lejano.
    fn order_from(&self, p: Vec2) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.perches.len()).collect();
        order.sort_by(|&a, &b| {
            self.perches[a]
                .at
                .distance(p)
                .total_cmp(&self.perches[b].at.distance(p))
        });
        order
    }

    /// Dónde se posa cada uno de `n` pájaros, llenando los posaderos en el
    /// orden `order` ([`Self::slots`]); si no caben, más apretados, y los que
    /// ni así caben quedan sin lugar (al asustarse, se van lejos).
    fn seats(&mut self, order: &[usize], n: usize) -> Vec<(Vec2, f32, Perch, Heading)> {
        // Con huecos al azar; si no caben todos, más apretados.
        let mut free = Vec::new();
        for loose in [1.0, 0.4, 0.0] {
            free = self.slots(order, n, loose);
            if free.len() >= n {
                break;
            }
        }
        free.truncate(n);
        free
    }

    /// Los lugares en los posaderos `order`, hasta `n`: amontonados sin orden,
    /// separados al menos [`PERCH_SPACING`], a veces tocándose de a dos, a
    /// veces con huecos de un palmo o más (`loose` escala los huecos).
    fn slots(&mut self, order: &[usize], n: usize, loose: f32) -> Vec<(Vec2, f32, Perch, Heading)> {
        let mut free: Vec<(Vec2, f32, Perch, Heading)> = Vec::new();
        for &i in order {
            let spot = self.perches[i];
            let mut slots = Vec::new();
            let mut offset = -spot.reach + (loose * self.rng.range(0.0, 0.25)).min(spot.reach);
            while offset <= spot.reach + 1e-6 {
                slots.push(offset.min(spot.reach));
                let gap = if self.rng.unit() < 0.25 {
                    self.rng.range(0.0, 0.02)
                } else {
                    self.rng.range(0.05, 0.40)
                };
                offset += PERCH_SPACING + loose * gap;
            }
            // Desde un extremo cualquiera, no siempre el mismo.
            if self.rng.unit() < 0.5 {
                slots.reverse();
            }
            let across = Vec2::new(-spot.along.y, spot.along.x);
            for offset in slots {
                let (at, y) = spot.place(offset);
                let at = at + across * self.rng.range(-spot.depth, spot.depth);
                let facing = self.perched_facing(&spot);
                free.push((at, y, spot.perch, facing));
            }
            if free.len() >= n {
                break;
            }
        }
        free
    }

    /// Hacia dónde mira uno que se posa en `spot`: en un pretil, casi la
    /// mitad hacia la calle, un tercio de costado y algunos de espaldas; en
    /// un alambre o un poste, de un lado o del otro, algo torcidos.
    fn perched_facing(&mut self, spot: &Spot) -> Heading {
        use std::f32::consts::{FRAC_PI_2, PI};
        let turn = match spot.perch {
            Perch::Roof => {
                let r = self.rng.unit();
                if r < 0.45 {
                    0.0
                } else if r < 0.8 {
                    if self.rng.unit() < 0.5 {
                        FRAC_PI_2
                    } else {
                        -FRAC_PI_2
                    }
                } else {
                    PI
                }
            }
            _ if self.rng.unit() < 0.5 => PI,
            _ => 0.0,
        };
        spot.facing.rotated(turn + self.rng.range(-0.45, 0.45))
    }

    /// Avanza `dt` segundos con el jugador en `threat` (su posición sobre el
    /// suelo y la altura del suelo ahí), si hay uno; `ground` es la altura
    /// del suelo.
    pub fn step(&mut self, dt: f32, threat: Option<(Vec2, f32)>, ground: &impl Fn(Vec2) -> f32) {
        let head = threat.map(|(p, y)| (p, y + THREAT_HEIGHT));
        let near = |at: Vec2, y: f32| {
            head.map_or(f32::INFINITY, |(p, h)| {
                ((at - p).length().powi(2) + (y - h).powi(2)).sqrt()
            })
        };
        let flush = self.kind.flush();
        let alarmed = self
            .birds
            .iter()
            .any(|b| !b.flying() && near(b.at, b.y) < flush);
        if alarmed {
            self.scatter(&near);
        } else {
            self.timer -= dt;
            if self.timer <= 0.0 {
                let calm = near(self.patch.area.center(), ground(self.patch.area.center()))
                    > self.kind.calm();
                match self.mode {
                    Mode::Resting if calm => self.come_down(ground),
                    Mode::Resting => self.timer = self.rng.range(2.0, 5.0),
                    Mode::Feeding => self.go_up(),
                }
            }
        }
        let area = self.patch.area;
        for bird in &mut self.birds {
            bird.step(dt, &area, ground);
        }
    }

    /// Todos vuelan del jugador: al posadero más cercano que les queda lejos
    /// de él o, si no hay ninguno, lejos, fuera de la vista.
    fn scatter(&mut self, near: &impl Fn(Vec2, f32) -> f32) {
        self.mode = Mode::Resting;
        self.timer = self.rng.range(REST.0, REST.1);
        let safe = 2.0 * self.kind.flush();
        let center = self.center();
        let order: Vec<usize> = self
            .order_from(center)
            .into_iter()
            .filter(|&i| near(self.perches[i].at, self.perches[i].y) > safe)
            .collect();
        let seats = self.seats(&order, self.birds.len());
        let away = self.away(near);
        for k in 0..self.birds.len() {
            let delay = self.rng.range(0.0, 0.35);
            let bird = &mut self.birds[k];
            if bird.gone() {
                continue;
            }
            match seats.get(k) {
                Some(&(at, y, perch, facing)) => bird.fly((at, y), Some(perch), facing, delay),
                None => bird.fly(away, None, bird.heading, delay),
            }
        }
    }

    /// El punto lejos del jugador adonde se van si no hay posadero.
    fn away(&mut self, near: &impl Fn(Vec2, f32) -> f32) -> (Vec2, f32) {
        let center = self.center();
        // La dirección que más los aleja, entre ocho.
        let best = (0..8)
            .map(|k| Heading::from_radians(k as f32 * std::f32::consts::FRAC_PI_4).forward())
            .max_by(|a, b| near(center + *a * 5.0, 0.0).total_cmp(&near(center + *b * 5.0, 0.0)))
            .unwrap_or(Vec2::new(1.0, 0.0));
        let y = self.birds.iter().map(|b| b.y).fold(f32::MIN, f32::max);
        (center + best * AWAY.0, y + AWAY.1)
    }

    /// El medio de los pájaros.
    fn center(&self) -> Vec2 {
        let n = self.birds.len().max(1) as f32;
        self.birds.iter().fold(Vec2::new(0.0, 0.0), |s, b| s + b.at) * (1.0 / n)
    }

    /// Bajan a comer, cada uno a su momento.
    fn come_down(&mut self, ground: &impl Fn(Vec2) -> f32) {
        self.mode = Mode::Feeding;
        self.timer = self.rng.range(FEED.0, FEED.1);
        let perch = self.patch.perch;
        for k in 0..self.birds.len() {
            let to = self.patch_point();
            let delay = self.rng.range(0.0, 2.5);
            let facing = Heading::from_radians(self.rng.range(0.0, std::f32::consts::TAU));
            let y = ground(to);
            self.birds[k].fly((to, y), Some(perch), facing, delay);
        }
    }

    /// Suben un rato a los posaderos más cercanos al comedero.
    fn go_up(&mut self) {
        self.mode = Mode::Resting;
        self.timer = self.rng.range(REST.0 + 8.0, REST.1 + 15.0);
        let order = self.order_from(self.patch.area.center());
        let seats = self.seats(&order, self.birds.len());
        for (k, &(at, y, perch, facing)) in seats.iter().enumerate() {
            let delay = self.rng.range(0.0, 2.0);
            self.birds[k].fly((at, y), Some(perch), facing, delay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(_: Vec2) -> f32 {
        0.0
    }

    /// Un grupo de `kind` comiendo en un comedero de 3 m en el origen, con un
    /// pretil de 2 m a `perch` metros al norte, a 5 m de alto.
    fn group(kind: SparrowKind, perch: f32, seed: u32) -> SparrowGroup {
        let patch = Patch {
            area: Aabb::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(3.0, 3.0)),
            perch: Perch::Sidewalk,
        };
        let spot = Spot {
            at: Vec2::new(0.0, perch),
            y: 5.0,
            perch: Perch::Roof,
            along: Vec2::new(1.0, 0.0),
            slope: 0.0,
            reach: 1.0,
            facing: Heading::from_radians(-std::f32::consts::FRAC_PI_2),
            depth: 0.06,
        };
        let mut g = SparrowGroup::new(kind, 1, patch, vec![spot], seed, &flat);
        // Comiendo, todos en el suelo.
        g.come_down(&flat);
        run(&mut g, 6.0, None);
        assert!(g.birds.iter().all(|b| b.perch == Some(Perch::Sidewalk)));
        g
    }

    fn run(g: &mut SparrowGroup, seconds: f32, threat: Option<Vec2>) {
        for _ in 0..(seconds / 0.02) as usize {
            g.step(0.02, threat.map(|p| (p, 0.0)), &flat);
        }
    }

    #[test]
    fn a_hop_moves_the_bird_only_while_its_feet_are_off_the_ground() {
        assert_eq!(hop_progress(0.0), 0.0);
        assert_eq!(hop_progress(HOP_AIRBORNE.0 - 0.01), 0.0);
        assert_eq!(hop_progress(HOP_AIRBORNE.1 + 0.01), 1.0);
        let mid = hop_progress((HOP_AIRBORNE.0 + HOP_AIRBORNE.1) / 2.0);
        assert!((mid - 0.5).abs() < 1e-3, "{mid}");
        let area = Aabb::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(4.0, 4.0));
        let mut bird = Sparrow::new(
            SparrowKind::Gorrion,
            Vec2::new(0.0, 0.0),
            0.0,
            Perch::Sidewalk,
            Heading::from_radians(0.0),
            3,
        );
        bird.act = Act::Hops {
            from: bird.at,
            dir: Vec2::new(1.0, 0.0),
            hops: 2,
        };
        bird.clock = 0.0;
        let hop = bird.hop_seconds();
        let mut track = Vec::new();
        for _ in 0..(2.0 * hop / 0.002) as usize - 1 {
            bird.step(0.002, &area, &flat);
            let phase = bird.hop_phase();
            track.push((phase, bird.at.x, bird.airborne()));
        }
        for &(phase, x, airborne) in &track {
            let inside = phase > HOP_AIRBORNE.0 + 0.01 && phase < HOP_AIRBORNE.1 - 0.01;
            let outside = phase < HOP_AIRBORNE.0 - 0.01 || phase > HOP_AIRBORNE.1 + 0.01;
            if outside {
                // Quieto en el suelo: en un múltiplo de un saltito.
                let hops = x / SparrowKind::Gorrion.hop_length();
                assert!((hops - hops.round()).abs() < 1e-3, "{phase} {x}");
                assert!(!airborne, "{phase}");
            }
            if inside {
                assert!(airborne, "{phase}");
            }
        }
        let end = track.last().unwrap().1;
        assert!(
            (end - 2.0 * SparrowKind::Gorrion.hop_length()).abs() < 1e-3,
            "{end}"
        );
    }

    #[test]
    fn each_kind_flushes_within_its_own_distance_to_a_perch_away_from_the_player() {
        for kind in SparrowKind::ALL {
            let mut g = group(kind, 15.0, 7);
            // Desde el sur: la distancia es en 3D, hasta la cabeza.
            let head = THREAT_HEIGHT;
            let level = |d: f32| (d * d - head * head).sqrt();
            let south = g.birds.iter().map(|b| b.at.y).fold(f32::MAX, f32::min);
            let far = Vec2::new(0.0, south - level(kind.flush() + 0.3));
            run(&mut g, 1.0, Some(far));
            assert!(
                g.birds.iter().all(|b| !b.flying()),
                "{kind:?} flushed outside its distance"
            );
            let first = g.birds[0].at;
            let near = first - Vec2::new(0.0, level(kind.flush() - 0.3));
            run(&mut g, 0.05, Some(near));
            assert!(g.birds.iter().all(Sparrow::flying), "{kind:?}");
            assert!(!g.feeding());
            // Primero aletean (o esperan su turno), nunca en un bote.
            run(&mut g, 0.1, Some(near));
            assert!(
                g.birds
                    .iter()
                    .all(|b| matches!(b.clip(), SparrowClip::Flap | SparrowClip::Idle))
            );
            let mut bounded = false;
            for _ in 0..200 {
                run(&mut g, 0.02, Some(near));
                bounded |= g.birds.iter().any(|b| b.clip() == SparrowClip::FlapBound);
            }
            assert!(bounded, "{kind:?}: a 15 m flight bounds");
            run(&mut g, 3.0, Some(near));
            for b in &g.birds {
                assert_eq!(b.perch, Some(Perch::Roof), "{kind:?}");
                assert!((b.y - 5.0).abs() < 1e-4);
                assert!(b.at.distance(near) > 2.0 * kind.flush());
                assert!(b.at.x.abs() <= 1.0 + 1e-3, "on the coping: {}", b.at.x);
            }
        }
    }

    #[test]
    fn they_settle_again_on_the_ground_once_the_player_is_gone() {
        for kind in SparrowKind::ALL {
            let mut g = group(kind, 15.0, 11);
            run(&mut g, 0.5, Some(Vec2::new(0.0, -2.0)));
            run(&mut g, 4.0, Some(Vec2::new(0.0, -2.0)));
            assert!(g.birds.iter().all(|b| b.perch == Some(Perch::Roof)));
            // Mientras el jugador está cerca del comedero, no bajan.
            run(&mut g, 60.0, Some(Vec2::new(0.0, -2.0 - kind.calm() / 2.0)));
            assert!(g.birds.iter().all(|b| b.perch == Some(Perch::Roof)));
            run(&mut g, REST.1 + 10.0, Some(Vec2::new(0.0, -200.0)));
            assert!(g.feeding(), "{kind:?}");
            for b in &g.birds {
                assert_eq!(b.perch, Some(Perch::Sidewalk), "{kind:?}");
                assert!(g.patch.area.contains_point(b.at));
            }
        }
    }

    #[test]
    fn with_every_perch_near_the_player_they_fly_off_out_of_sight_and_come_back() {
        let mut g = group(SparrowKind::Gorrion, 3.0, 5);
        let player = Vec2::new(0.0, 1.0);
        run(&mut g, 0.05, Some(player));
        run(&mut g, 12.0, Some(player));
        assert!(g.birds.iter().all(Sparrow::gone));
        assert!(g.birds.iter().all(|b| b.at.distance(player) > AWAY.0 - 5.0));
        run(&mut g, REST.1 + 15.0, Some(Vec2::new(500.0, 0.0)));
        assert!(g.birds.iter().all(|b| b.perch == Some(Perch::Sidewalk)));
    }

    #[test]
    fn a_flock_never_moves_in_sync() {
        let mut g = group(SparrowKind::Gorrion, 15.0, 2);
        g.birds.truncate(12);
        let n = g.birds.len();
        assert!(n >= 3);
        let mut paces: Vec<f32> = g.birds.iter().map(|b| b.pace).collect();
        paces.sort_by(f32::total_cmp);
        paces.dedup();
        assert_eq!(paces.len(), n, "each at its own pace");
        // Sembrados: cada uno en su propio punto de su clip.
        let fresh = SparrowGroup::new(
            SparrowKind::Gorrion,
            1,
            g.patch,
            g.perches.clone(),
            99,
            &flat,
        );
        let mut times: Vec<f32> = fresh.birds.iter().map(Sparrow::clip_time).collect();
        times.sort_by(f32::total_cmp);
        assert!(times[times.len() - 1] - times[0] > 0.5, "{times:?}");
        // Después de un rato comiendo, no todos hacen lo mismo.
        let mut mixed = 0;
        for _ in 0..50 {
            run(&mut g, 0.5, None);
            let first = g.birds[0].clip();
            mixed += usize::from(g.birds.iter().any(|b| b.clip() != first));
        }
        assert!(mixed > 40, "{mixed}");
    }

    #[test]
    fn a_settled_flock_bunches_irregularly_on_its_ledge_facing_every_way() {
        for seed in 0..6 {
            let mut g = group(SparrowKind::Gorrion, 15.0, 40 + seed);
            let near = g.birds[0].at - Vec2::new(0.0, 2.0);
            run(&mut g, 0.05, Some(near));
            run(&mut g, 6.0, Some(near));
            let spot = g.perches[0];
            let mut along: Vec<f32> = g
                .birds
                .iter()
                .map(|b| {
                    assert_eq!(b.perch, Some(Perch::Roof));
                    let d = b.at - spot.at;
                    let across = Vec2::new(-spot.along.y, spot.along.x);
                    // Sobre la albardilla.
                    assert!(d.dot(spot.along).abs() <= spot.reach + 1e-4);
                    assert!(d.dot(across).abs() <= spot.depth + 1e-4);
                    d.dot(spot.along)
                })
                .collect();
            along.sort_by(f32::total_cmp);
            let gaps: Vec<f32> = along.windows(2).map(|w| w[1] - w[0]).collect();
            if gaps.len() < 2 {
                continue;
            }
            // Separados al menos un cuerpo, pero no como cuentas en un hilo.
            assert!(gaps.iter().all(|&d| d >= PERCH_SPACING - 1e-4), "{gaps:?}");
            let (lo, hi) = gaps
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), &d| (a.min(d), b.max(d)));
            assert!(hi - lo > 0.1, "{gaps:?}");
            // Mirando hacia distintos lados.
            let out = spot.facing.forward();
            let ways: Vec<i32> = g
                .birds
                .iter()
                .map(|b| (b.heading.forward().dot(out) * 1.5).round() as i32)
                .collect();
            let mut kinds = ways.clone();
            kinds.sort();
            kinds.dedup();
            assert!(kinds.len() >= 2, "{ways:?}");
        }
    }
}
