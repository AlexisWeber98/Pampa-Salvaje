//! Caballos criollos ensillados: el caballo de henrysteve973
//! (`tools/henry_horse.py`, el modelo de los caballos de corral, ver
//! [`livestock`](crate::fauna::livestock)) con un recado de 1919 y una brida
//! de cuero crudo (ver [`tack`](super::tack)), atado o montado. Los huesos que
//! usa el juego se piden por su papel ([`bones`](super::bones)).
//!
//! Un caballo ensillado es una entidad de cuerpo ([`HorseBody`], girada hacia
//! el rumbo, +X su frente, su derecha +Z, los vasos en su origen) que contiene
//! el modelo ([`HorseModel`]). El modelo espera su escena ([`HorsePending`]):
//! hasta que termina de cargar no se dibuja nada (las escenas cargan con el
//! juego, mucho antes de que alguien se suba a un caballo). Cuando se crea,
//! [`dress_horse`] les da a sus meshes sus mapas y el rango compartido de los
//! accesorios, como a los caballos de corral, y ubica los huesos que mueven los
//! andares ([`gait`](super::gait): las cadenas de sus patas) y los del salto
//! ([`HorseJoints`]). El mismo evento de escena pone los aperos
//! ([`tack`](super::tack): el recado y la brida con skinning, los estribos y
//! las correas), y una vez puestos los aperos un jinete con modelo se sienta
//! sobre el lomo ([`rider`](super::rider)), así que sube, cabecea y gira con
//! el propio clip del caballo.
//!
//! [`drive_horses`] reproduce los clips: la espera, después la caminata (no
//! hay clip de trote) y después el galope ([`gait_weights`]), con los vasos
//! apoyados quietos sobre el suelo. La caminata, por encima de la velocidad
//! de su clip, reparte lo que falta por igual entre el ritmo y el tranco
//! ([`gait_rate`], [`walk_stride`]); el galope no se estira: cuadro a cuadro
//! va al ritmo de su vaso apoyado mientras apoya y más despacio en la
//! suspensión ([`gallop_rate`]), así que alarga el tranco con la velocidad
//! (S7); al retroceder, la zancada se reproduce hacia atrás
//! al mismo ritmo; al girar en el lugar, camina en el sitio al ritmo corto de
//! [`stepping_speed`]. Un caballo atado cambia de espera de vez en cuando, como
//! los caballos de corral; uno montado se queda quieto.
//!
//! El salto encadena tres clips (S3 de `odd/tasks/henry-horse.md`). En el aire
//! ([`VisualPose::flight`](super::VisualPose)) se ve el despegue y la subida
//! de `Jump`, fundidos en lo alto en la bajada y el apoyo de `S.Jump`, el
//! salto corto ([`JUMP_SEAM`]): el resto de `Jump` y los aterrizajes
//! `Jump_Run` y `Jump_Land` son los de una caída desde un barranco (aterrizan
//! con el lomo a 38-41 grados de cabeza), y `S.Jump` aterriza a 17, lo que
//! baja el arco de la física. Nunca avanzan con el reloj: en cada frame se
//! fijan en el punto del propio vuelo de la física ([`jump_frame`]),
//! demorándose en el impulso ([`JUMP_LINGER`]). Ya en el suelo
//! ([`landing`]), si sigue galopando, `Jump_Run` sigue desde el cuadro más
//! parecido al apoyo de `S.Jump` ([`RUN_FROM`]) al ritmo del suelo
//! ([`JUMP_RUN_SPEED`]) y, cuando
//! termina, el galope retoma desde su primer cuadro, que es el último de
//! `Jump_Run`; si no, sigue el resto de `S.Jump`, que se detiene. Los clips
//! suben el cuerpo dentro del clip; [`ground_horses`] lo baja para que el vaso
//! más bajo siga el arco de la física ([`jump_drop`]) y fija la raíz en su
//! lugar (la física es la que lo mueve). Un
//! caballo fuera de la vista o lejos conserva su última pose, pero nunca el del
//! jugador.
//!
//! Cada pelaje está horneado en su propio mapa de color del cuerpo
//! ([`Coat::map_name`], `tools/henry_coats.py`): el color según dónde está
//! cada texel en el caballo parado, así que las manchas, los rodados y la raya
//! de mula del bayo recorren el cuerpo; el pigmento se sombrea además con la
//! forma del caballo (el lomo y lo alto del cuello más oscuros, el vientre y
//! los ijares más claros, manchones suaves), por el pelo del mapa de
//! henrysteve973 sin sus manchas; el mapa normal agrega los surcos de los
//! músculos.

use bevy::animation::graph::AnimationGraphHandle;
use bevy::ecs::system::SystemParam;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use pampa_core::{AnimationState, Heading, HorseGait};

use super::bones::{HorseJoint, TOE_STANDING};
use super::clips::stepping_speed;
use super::figure::{FigureKind, Rgb, body_rotation};
use super::gait::{HorseBones, fit_horse};
use super::visual::VisualPose;
use crate::fauna::Animal;
use crate::fauna::livestock::{
    self, CLIP_COUNT, Clip, CoatCache, HORSE_GALLOP_SPEED, HORSE_IDLE, HORSE_WALK_SPEED,
    HORSE_WITHERS, LOADED_COUNT, LivestockModels, ModelPart, Species, any_in_view, dress_meshes,
    frozen, mix, start_offset, view_eye,
};
use crate::gltf::{self, ModelState};

/// El pelaje de un caballo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coat {
    /// Marrón oscuro entero, crin y cola negras, sin blanco.
    Zaino,
    /// Gris con rodados más claros.
    Tordillo,
    /// Bayo con grandes manchas blancas.
    Overo,
    /// Alazán, con la crin y la cola más claras, rubias.
    Alazan,
    /// Bayo: amarillo arena con la crin, la cola y la parte baja de las patas
    /// negras, y una raya oscura a lo largo del lomo.
    Bayo,
}

impl Coat {
    pub const ALL: [Self; 5] = [
        Self::Zaino,
        Self::Tordillo,
        Self::Overo,
        Self::Alazan,
        Self::Bayo,
    ];

    /// El caballo de Don Hidalgo, el que monta el jugador.
    pub const HIDALGO: Self = Self::Alazan;

    /// Un pelaje elegido de forma determinista a partir de `seed`.
    pub fn seeded(seed: u32) -> Self {
        let mut h = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        Self::ALL[(h % Self::ALL.len() as u32) as usize]
    }

    /// El nombre del mapa de este pelaje en el conjunto del cuerpo del
    /// caballo (`henry/horse_body/<nombre>.ktx2`): `tools/henry_coats.py`
    /// hornea en él el pelaje entero (las manchas del overo, los rodados del
    /// tordillo, la raya de mula del bayo, los cabos oscuros, el lomo más
    /// oscuro y el vientre más claro) por el pelo del caballo de henrysteve973.
    pub fn map_name(self) -> &'static str {
        match self {
            Self::Zaino => "zaino",
            Self::Tordillo => "tordillo",
            Self::Overo => "overo",
            Self::Alazan => "alazan",
            Self::Bayo => "bayo",
        }
    }

    /// El color del cuerpo, el que se dibuja si falta el mapa del pelaje: el
    /// `body` de `COATS` en `tools/henry_coats.py`.
    pub(crate) fn body(self) -> Rgb {
        match self {
            Self::Zaino => [78, 50, 34],
            Self::Tordillo => [118, 118, 114],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [154, 80, 42],
            Self::Bayo => [198, 162, 104],
        }
    }

    /// La crin y la cola: negras en el zaino y el bayo, blanquecinas en el
    /// tordillo, rubias en el alazán y más oscuras que el pelaje en el overo;
    /// siempre apartadas del pelaje, para que la crin no se pierda contra el
    /// cuello. Tiñe el mapa neutro de las tarjetas de pelo (`henry_hair`).
    pub(crate) fn mane(self) -> Rgb {
        match self {
            Self::Zaino | Self::Bayo => [34, 28, 24],
            Self::Tordillo => [206, 202, 194],
            Self::Overo => [88, 50, 30],
            Self::Alazan => [214, 150, 92],
        }
    }
}

/// A qué distancia (en m) están los vasos de un caballo del centro de un giro
/// en el lugar, para el ritmo de sus pasos: con la velocidad de giro del juego
/// de 1.8 rad/s, 0.40 m/s, unos 0.43 pasos por segundo con la caminata de
/// tranco corto del caballo de henrysteve973 (los mismos que daba el caballo
/// anterior con 0.3 m).
pub const HORSE_TURN_STEP_RADIUS: f32 = 0.22;

/// Las velocidades sobre el suelo (m/s) entre las que la espera se funde con la
/// caminata.
pub const WALK_BAND: (f32, f32) = (0.05, 0.6);

/// Las velocidades sobre el suelo (m/s) a las que un caballo que camina pasa al
/// galope, y uno que galopa vuelve a la caminata: alrededor de
/// [`HorseGait::TROT_MAX`] (6.5 m/s), donde la lógica deja de trotar y galopa
/// (el galope más lento, 1.4 trancos por segundo, [`gallop_rate`]). La lógica
/// camina hasta [`WALK_TOP`] y trota de ahí a 6.5; sin clip de trote todavía, en
/// esa franja la caminata va a su ritmo y su tranco más largos
/// ([`walk_stride`]) y los vasos se deslizan hacia atrás lo que le falta. El andar cambia de golpe (y los pesos se funden rápido,
/// [`HORSE_BLEND_RATE`], en una fracción de tranco, al paso de la mano de
/// galope, [`in_step`]), sin mantener nunca los dos mitad y mitad: sus patas
/// no coinciden.
pub const GALLOP_FROM: (f32, f32) = (6.35, 6.65);

/// Si un caballo que se mueve a `speed` galopa, según si `was` galopando.
pub fn gallops(was: bool, speed: f32) -> bool {
    speed > if was { GALLOP_FROM.0 } else { GALLOP_FROM.1 }
}

/// Lo más lento que se reproduce la caminata, respecto de lo grabado: al
/// retroceder despacio y al girar en el lugar los pasos no se arrastran.
pub const WALK_RATE_FLOOR: f32 = 0.3;

/// Hasta qué velocidad sobre el suelo camina la lógica, en m/s (4,5 km/h,
/// `pampa_core::RiderParams::walk_max`): hasta ella la caminata es el clip de
/// henry tal cual, sin estirar el tranco, a lo sumo 1,35 veces su ritmo (S8).
/// Entre ella y [`HorseGait::TROT_MAX`] la lógica trota.
pub const WALK_TOP: f32 = 4.5 / 3.6;

/// Hasta qué velocidad se estira el tranco de la caminata, en m/s (10,8
/// km/h): en la franja del trote, sin clip de trote todavía, la caminata va a
/// su tranco y su ritmo más largos, y por encima los vasos se deslizan hacia
/// atrás lo que falta.
pub const STRETCH_TOP: f32 = 3.0;

/// Cuántas veces más largo que el del clip es el tranco de la caminata de un
/// caballo que se mueve a `speed` m/s (hacia adelante o hacia atrás): ninguna
/// hasta [`WALK_TOP`]; por encima, lo que falta se reparte por igual entre el
/// ritmo ([`gait_rate`]) y el tranco, la raíz de la velocidad sobre
/// [`WALK_TOP`], hasta [`STRETCH_TOP`] (1.55 veces cada uno). Lo estira el
/// ajuste de los andares ([`gait`](super::gait)).
pub fn walk_stride(speed: f32) -> f32 {
    (speed.abs() / WALK_TOP)
        .clamp(1.0, STRETCH_TOP / WALK_TOP)
        .sqrt()
}

/// La velocidad sobre el suelo que muestra la caminata del caballo en el
/// juego, en m/s: la del clip ([`HORSE_WALK_SPEED`]) a su escala.
pub const WALK_SPEED: f32 = HORSE_WALK_SPEED * Species::Horse.withers() / HORSE_WITHERS;

/// La velocidad sobre el suelo que muestra el galope en el juego reproducido
/// tal como se grabó, en m/s: la del clip ([`HORSE_GALLOP_SPEED`]) a su
/// escala, 4.22. El galope del juego no se reproduce parejo
/// ([`gallop_rate`]).
pub const GALLOP_SPEED: f32 = HORSE_GALLOP_SPEED * Species::Horse.withers() / HORSE_WITHERS;

/// Dónde está la suspensión del galope en su ciclo (`0..1`, el clip de
/// 1.000 s): ningún vaso apoya (cada punta de vaso a más de 3 cm de lo más
/// bajo que llega) desde que despega la mano derecha, la de galope, hasta que
/// apoya la pata izquierda; el 43 % del ciclo. El hueco entre la mano
/// izquierda y la derecha (0.826 a 0.891, 65 ms) cuenta como apoyo.
pub const GALLOP_SUSPENSION: (f32, f32) = (0.001, 0.428);

/// Qué tan rápido se reproduce la suspensión del galope, respecto de lo
/// grabado: con 1.25 el galope tendido (49 km/h) da 2.05 trancos por segundo,
/// como un caballo (Parsons et al. 2008: unos 2.0 a 9.5 m/s y 2.1 a 12.5).
/// Más lento parecería flotar: la pelvis baja 19 cm en la suspensión.
pub const GALLOP_AIR_RATE: f32 = 1.25;

/// En qué fracción del ciclo pasa el galope del ritmo del apoyo al de la
/// suspensión, y de vuelta: dentro de la suspensión, junto a cada borde (40 ms
/// del clip), para que los vasos apoyados no se deslicen.
pub const GALLOP_EASE: f32 = 0.04;

/// Cuántos segundos del clip `Gallop` tarda en pasar un metro del juego bajo
/// el vaso apoyado que marca el ritmo, en 48 puntos de su ciclo (de
/// 0/48 a 47/48, entre ellos en línea recta): el inverso de lo rápido que va
/// hacia atrás el último vaso que apoyó. Medido en `horse.glb` a 960 muestras
/// por ciclo, cada ventana de apoyo limitada a entre 1/1.6 y 1/0.7 veces su
/// velocidad media (en los bordes cae a 1.9 m/s), suavizado a 40 ms y, en la
/// suspensión y en el hueco entre las manos, de un borde al otro en línea
/// recta. En promedio el vaso apoyado va a 4.06 m/s del juego.
#[rustfmt::skip]
#[allow(clippy::approx_constant)] // 0.3010 es una medida, no log10(2).
pub const GALLOP_SWEEP: [f32; 48] = [
    0.2488, 0.2957, 0.3010, 0.3065, 0.3122, 0.3182, 0.3244, 0.3308,
    0.3374, 0.3444, 0.3516, 0.3592, 0.3671, 0.3753, 0.3839, 0.3929,
    0.4024, 0.4123, 0.4227, 0.4337, 0.4448, 0.4528, 0.3535, 0.2969,
    0.2869, 0.2765, 0.2703, 0.2713, 0.2735, 0.2621, 0.2368, 0.2360,
    0.2579, 0.2510, 0.2316, 0.2148, 0.2164, 0.2198, 0.2335, 0.2498,
    0.2402, 0.2185, 0.1989, 0.1891, 0.1742, 0.1610, 0.1742, 0.2013,
];

/// La velocidad sobre el suelo, en m/s del juego, a la que van hacia atrás los
/// vasos apoyados de `Jump_Run` (el aterrizaje que sigue galopando) desde
/// [`RUN_FROM`]: 4.4 a 4.6 por pata, medido igual que el galope.
pub const JUMP_RUN_SPEED: f32 = 4.5;

/// Qué tan rápido se reproduce el galope en la fase `phase` (`0..1`) de su
/// ciclo para un caballo que se mueve a `speed` m/s (negativa al retroceder):
/// mientras apoya, al ritmo con el que el vaso que marca el paso
/// ([`GALLOP_SWEEP`]) va hacia atrás a la velocidad del suelo, así que no se
/// desliza; en la suspensión ([`GALLOP_SUSPENSION`]), a [`GALLOP_AIR_RATE`],
/// pasando de un ritmo al otro en [`GALLOP_EASE`]. Así el galope alarga el
/// tranco con la velocidad, sin estirar las patas. Nunca más lento que a la
/// velocidad a la que un caballo deja de galopar ([`GALLOP_FROM`]).
pub fn gallop_rate(phase: f32, speed: f32) -> f32 {
    let back = if speed < 0.0 { -1.0 } else { 1.0 };
    let speed = speed.abs().max(GALLOP_FROM.0);
    let phase = phase.rem_euclid(1.0);
    let at = phase * GALLOP_SWEEP.len() as f32;
    let i = (at as usize).min(GALLOP_SWEEP.len() - 1);
    let f = at - i as f32;
    let sweep = GALLOP_SWEEP[i] * (1.0 - f) + GALLOP_SWEEP[(i + 1) % GALLOP_SWEEP.len()] * f;
    let (lift, land) = GALLOP_SUSPENSION;
    let ground = if phase > lift && phase < land {
        (1.0 - (phase - lift).min(land - phase) / GALLOP_EASE).max(0.0)
    } else {
        1.0
    };
    back * (ground * speed * sweep + (1.0 - ground) * GALLOP_AIR_RATE)
}

/// Cuántos segundos tarda un tranco de galope de un caballo que se mueve a
/// `speed` m/s ([`gallop_rate`] a lo largo del ciclo del clip de `length`
/// segundos).
pub fn gallop_period(speed: f32, length: f32) -> f32 {
    const STEPS: usize = 192;
    (0..STEPS)
        .map(|k| {
            let phase = (k as f32 + 0.5) / STEPS as f32;
            length / STEPS as f32 / gallop_rate(phase, speed.abs())
        })
        .sum()
}

/// El peso de cada clip, en el orden de [`Clip::ALL`], para un caballo parado
/// con `idle` o que se mueve a `speed` m/s con `stride` (la caminata o el
/// galope): la espera se funde con la zancada. Suman uno.
pub fn gait_weights(idle: Clip, stride: Clip, speed: f32) -> [f32; CLIP_COUNT] {
    let moving = ((speed - WALK_BAND.0) / (WALK_BAND.1 - WALK_BAND.0)).clamp(0.0, 1.0);
    let mut out = [0.0; CLIP_COUNT];
    out[idle.index()] = 1.0 - moving;
    out[stride.index()] += moving;
    out
}

/// Qué tan rápido se reproduce `clip` para un caballo que se mueve a `speed`
/// m/s, negativa al retroceder: la caminata, al ritmo del suelo hasta
/// [`WALK_TOP`] (nunca por debajo de [`WALK_RATE_FLOOR`]) y por encima, además,
/// lo mismo que estira el tranco ([`walk_stride`]); el galope, en promedio a
/// lo largo de un tranco (cuadro a cuadro va a [`gallop_rate`]); `Jump_Run`,
/// al ritmo del suelo sobre su [`JUMP_RUN_SPEED`]; una espera, tal como se
/// grabó.
pub fn gait_rate(clip: Clip, speed: f32) -> f32 {
    let back = if speed < 0.0 { -1.0 } else { 1.0 };
    let speed = speed.abs();
    match clip {
        Clip::Walk if speed < WALK_TOP => back * (speed / WALK_SPEED).max(WALK_RATE_FLOOR),
        Clip::Walk => back * WALK_TOP / WALK_SPEED * walk_stride(speed),
        Clip::Gallop => back * GALLOP_LENGTH / gallop_period(speed, GALLOP_LENGTH),
        Clip::JumpRun => back * speed / JUMP_RUN_SPEED,
        _ => 1.0,
    }
}

/// Cuánto dura el clip `Gallop`, en segundos: un tranco (`tools/henry_horse.py`
/// le agrega el cuadro que le falta al original para cerrar el ciclo).
pub const GALLOP_LENGTH: f32 = 1.0;

/// Las esperas entre las que alterna un caballo atado. Atado corto al poste, no
/// puede pastar (y sus riendas, sobre la cabezada, le atravesarían el cuello si
/// lo hiciera).
pub const TIED_IDLES: [Clip; 3] = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];

/// Los clips que reproduce un caballo ensillado: los de [`Clip::ALL`], el
/// salto corto y el aterrizaje al galope.
pub const HORSE_CLIPS: [Clip; CLIP_COUNT + 2] = [
    Clip::Idle,
    Clip::Idle2,
    Clip::IdleHeadlow,
    Clip::Eating,
    Clip::Walk,
    Clip::Gallop,
    Clip::GallopJump,
    Clip::ShortJump,
    Clip::JumpRun,
];

/// Los huesos con los que [`ground_horses`] baja al caballo en el aire y le
/// quita el movimiento de raíz ([`HorseJoint`]), en la raíz de su modelo.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorseJoints {
    pub root: Entity,
    /// El reposo de la traslación de `root`.
    pub root_rest: Vec3,
    pub toes: [Entity; 4],
}

/// El cuerpo de un caballo ensillado, hijo de su entidad de personaje: girado
/// hacia el rumbo, contiene el modelo.
#[derive(Component, Debug, Clone)]
pub struct HorseBody {
    pub coat: Coat,
    /// Quién lo monta, si alguien lo hace.
    pub rider: Option<FigureKind>,
    /// Si lleva el recado y la brida; un caballo de tiro no.
    pub saddled: bool,
    /// Su andar: lo deciden la animación de la lógica y la velocidad.
    pub gait: HorseGait,
    /// Cuánto galope reproduce ahora (`0..1`): un jinete se inclina con él.
    pub gallop: f32,
    /// Su velocidad sobre el suelo actual, en m/s (ninguna estando parado).
    pub speed: f32,
    /// Qué tan avanzado está el vuelo de un salto (`0..=1`), nada en el suelo:
    /// un jinete mantiene el equilibrio durante él.
    pub flight: Option<f32>,
    /// Su propia semilla: dónde empiezan sus clips y qué espera reproduce un
    /// caballo atado.
    pub seed: u32,
}

impl HorseBody {
    /// La espera con la que está parado a los `clock` segundos: un caballo
    /// montado se queda quieto; uno atado cambia de espera de vez en cuando,
    /// como los caballos de corral ([`TIED_IDLES`]).
    pub fn idle(&self, clock: f32) -> Clip {
        match self.rider {
            Some(_) => Clip::Idle,
            // Enganchado a un vehículo, se queda quieto.
            None if !self.saddled => Clip::Idle,
            None => {
                let turn = ((clock.max(0.0) + (self.seed % 97) as f32) / HORSE_IDLE) as u32;
                TIED_IDLES[mix(self.seed, turn) as usize % TIED_IDLES.len()]
            }
        }
    }

    /// Si sus clips siguen reproduciéndose fuera de la vista o lejos: el
    /// caballo del jugador siempre lo hace.
    pub fn never_freezes(&self) -> bool {
        self.rider == Some(FigureKind::Protagonist)
    }
}

/// La raíz del modelo del caballo, hija de su [`HorseBody`].
#[derive(Component, Debug, Clone, Copy)]
pub struct HorseModel;

/// Un modelo de caballo que espera que cargue su escena.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorsePending;

/// El estado de animación de un modelo de caballo creado, en su raíz.
#[derive(Component, Debug, Clone)]
pub struct HorseRig {
    /// La entidad con su `AnimationPlayer`.
    pub player: Entity,
    /// Sus meshes dibujadas: está a la vista mientras alguna lo esté.
    pub probes: Vec<Entity>,
    /// El peso actual de cada clip, en el orden de [`Clip::LOADED`] (solo los
    /// de [`HORSE_CLIPS`] llegan a tener peso).
    pub weights: [f32; LOADED_COUNT],
    /// Si sus clips se están reproduciendo.
    pub started: bool,
    /// Si sus huesos conservan su última pose.
    pub frozen: bool,
    /// La duración de cada clip, en segundos, una vez que terminó de cargar,
    /// en el orden de [`Clip::LOADED`].
    pub durations: [f32; LOADED_COUNT],
    /// El clip de zancada que reproduce principalmente (la caminata o el
    /// galope): el otro se pone al mismo paso cuando toma el relevo.
    pub lead: Clip,
    /// Qué tan avanzado está el vuelo de un salto (`0..=1`), nada en el suelo.
    pub flight: Option<f32>,
    /// El aterrizaje que se reproduce después de un salto ([`landing`]) y en
    /// qué segundo de su clip va; nada fuera de uno.
    pub landing: Option<(Clip, f32)>,
    /// Cuántas veces más largo que el del clip es ahora el tranco de su
    /// caminata ([`walk_stride`]).
    pub stride: f32,
}

impl HorseRig {
    /// El rig de un modelo, con sus clips reproducidos por `player`, visto a
    /// través de `probes`.
    pub fn new(player: Entity, probes: Vec<Entity>) -> Self {
        Self {
            player,
            probes,
            weights: [0.0; LOADED_COUNT],
            started: false,
            frozen: false,
            durations: [0.0; LOADED_COUNT],
            lead: Clip::Walk,
            flight: None,
            landing: None,
            stride: 1.0,
        }
    }
}

/// Dónde está el ciclo de la caminata al mismo paso que el del galope: cada uno
/// con la mano izquierda apoyando (el de la caminata en [`WALK_PLANT`], el del
/// galope en [`GALLOP_PLANT`]).
pub const WALK_FROM_GALLOP: f32 = WALK_PLANT - GALLOP_PLANT;

/// En qué punto de su ciclo (`0..1`) apoya la mano izquierda en el galope: el
/// centro de su herradura llega a 3 cm de su punto más bajo en 0.656 s de
/// 1.000 (muestreado a 96 por segundo). El galope del caballo de henrysteve973
/// es a mano derecha: apoyan la pata izquierda, la derecha, la mano izquierda
/// y por último la derecha, la de galope.
pub const GALLOP_PLANT: f32 = 0.656;

/// En qué punto de su ciclo (`0..1`) apoya la mano izquierda en la caminata,
/// medido igual: en 0.115 s de 1 s.
pub const WALK_PLANT: f32 = 0.115;

/// La fracción de su ciclo (`0..1`) en la que empieza un clip de zancada que
/// toma el relevo de `from` en la fase `phase` de su ciclo.
pub fn in_step(to: Clip, phase: f32) -> f32 {
    match to {
        Clip::Walk => (phase + WALK_FROM_GALLOP).rem_euclid(1.0),
        _ => (phase - WALK_FROM_GALLOP).rem_euclid(1.0),
    }
}

/// Qué tan rápido se acercan los pesos de los clips a sus objetivos, por
/// segundo: un fundido rápido, para que rara vez se vean dos clips mitad y
/// mitad.
pub const HORSE_BLEND_RATE: f32 = 12.0;

/// Lo mismo, al despegar: más rápido, para que las patas estén recogidas cuando
/// el caballo deja el suelo.
pub const JUMP_BLEND_RATE: f32 = 40.0;

/// En qué segundo del clip `Jump` despega el caballo: cuando los dos vasos
/// traseros, empujando estirados hacia atrás, dejan el suelo (el centro de su
/// herradura a más de 3 cm de su punto más bajo, 0.47 s). Lo que viene antes
/// es el agachado (las manos despegan primero), que el juego no reproduce: la
/// física despega de golpe.
pub const JUMP_TAKEOFF: f32 = 0.47;

/// En qué segundo de `S.Jump` toca el suelo la primera mano (la izquierda, a
/// menos de 3 cm de su punto más bajo; la derecha la trae apenas levantada,
/// también parado): con el cuerpo 17 grados de cabeza, lo que baja un salto
/// de alambrado, y no los 38-41 de `Jump_Run` y `Jump_Land`, que aterrizan de
/// una caída (siguen a `Falling`).
pub const SHORT_TOUCHDOWN: f32 = 26.0 / 24.0;

/// Cuántos segundos va `S.Jump` detrás de `Jump` en el vuelo: de la subida a
/// lo alto los dos clips muestran casi la misma pose (a unos 20 cm de la otra
/// por articulación), la de `S.Jump` unos 7.5 cuadros después.
pub const SHORT_BEHIND: f32 = 7.5 / 24.0;

/// El tramo de `Jump`, en sus segundos, en el que se funde en `S.Jump`: en lo
/// alto, cuando las patas recogidas empiezan a bajar. Antes, `Jump` sigue
/// subiendo el cuerpo y lo cabecea hacia la caída de un barranco (la pelvis a
/// 2.9 m, el lomo a 35 grados de cabeza); fundido a lo largo del tramo, el
/// lomo queda casi horizontal mientras pasa de un clip al otro.
pub const JUMP_SEAM: (f32, f32) = (22.0 / 24.0, 26.0 / 24.0);

/// Desde qué segundo de `Jump_Run` sigue en el suelo un caballo que aterriza
/// galopando: el cuadro más parecido al apoyo de `S.Jump` (las manos
/// empujando y las patas traseras llegando abajo, el lomo casi horizontal);
/// antes, `Jump_Run` es el final de una caída.
pub const RUN_FROM: f32 = 0.40;

/// Cuántos segundos del clip se alarga el impulso del despegue: el clip empieza
/// lento (a alrededor de un tercio de su ritmo) en el impulso de las patas
/// traseras, para que se note cuando el caballo deja el suelo, y se pone al día
/// hacia un tercio del vuelo.
pub const JUMP_LINGER: f32 = 0.15;

/// Cómo sigue en el suelo un caballo que aterriza a `speed` m/s (S3): si
/// sigue galopando ([`gallops`]), con `Jump_Run` desde [`RUN_FROM`], que
/// termina en el primer cuadro del galope; si no, con el resto de `S.Jump`,
/// que se detiene.
pub fn landing(speed: f32) -> Clip {
    if gallops(true, speed.abs()) {
        Clip::JumpRun
    } else {
        Clip::ShortJump
    }
}

/// Desde qué segundo de su clip sigue en el suelo el aterrizaje `land`
/// ([`landing`]).
pub fn landing_from(land: Clip) -> f32 {
    match land {
        Clip::JumpRun => RUN_FROM,
        _ => SHORT_TOUCHDOWN,
    }
}

/// Lo que muestra un caballo en un punto de su salto ([`jump_frame`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpFrame {
    /// En qué segundo de `Jump` está.
    pub jump: f32,
    /// En qué segundo de `S.Jump` está.
    pub short: f32,
    /// El peso de `S.Jump` (`0..=1`); `Jump` tiene el resto.
    pub blend: f32,
}

/// Qué muestra un caballo en la fracción `flight` (`0..=1`) de su salto: el
/// despegue y la subida de `Jump` (desde [`JUMP_TAKEOFF`]) fundidos en lo
/// alto ([`JUMP_SEAM`]) en la bajada de `S.Jump` hasta su
/// [`SHORT_TOUCHDOWN`], repartidos a lo largo del propio vuelo de la física
/// (casi al ritmo con que se crearon), demorándose en el impulso
/// ([`JUMP_LINGER`]). Nunca se reproduce con el reloj.
pub fn jump_frame(flight: f32) -> JumpFrame {
    let f = flight.clamp(0.0, 1.0);
    let span = SHORT_TOUCHDOWN + SHORT_BEHIND - JUMP_TAKEOFF + JUMP_LINGER;
    let jump = JUMP_TAKEOFF + span * f - JUMP_LINGER * (1.0 - (1.0 - f).powi(4));
    let (from, to) = JUMP_SEAM;
    let s = ((jump - from) / (to - from)).clamp(0.0, 1.0);
    JumpFrame {
        jump,
        short: (jump - SHORT_BEHIND).max(0.0),
        blend: s * s * (3.0 - 2.0 * s),
    }
}

/// Cuánto bajar el modelo en el aire, en metros del modelo, con las puntas de
/// los vasos a `toes` de alto ([`HorseJoint::TOES`]): el vuelo de los clips
/// sube el cuerpo dentro del clip (los vasos hasta 1.1-1.5 m en lo alto), y el del
/// juego es el arco de la física, así que en el aire el vaso más bajo queda a
/// la altura de un vaso apoyado ([`TOE_STANDING`]) y la física lo lleva por
/// su arco: los vasos pasan sobre lo que la física dice que pasan. En el
/// suelo, nada; nunca lo sube.
pub fn jump_drop(toes: [f32; 4], flying: bool) -> f32 {
    if !flying {
        return 0.0;
    }
    (toes.into_iter().fold(f32::MAX, f32::min) - TOE_STANDING).max(0.0)
}

/// El transform del modelo bajo su cuerpo: la exportación del caballo girada
/// para mirar hacia +X y escalada a tamaño real (la escala de los caballos de
/// corral).
pub fn model_transform() -> Transform {
    livestock::model_transform(Species::Horse, Heading::default(), 0.0)
}

/// Crea un caballo ensillado con el pelaje `coat` (montado por `rider`) bajo
/// `parent`: su cuerpo y su modelo esperando la escena. Devuelve el cuerpo.
pub(super) fn spawn_horse(
    commands: &mut Commands,
    parent: Entity,
    coat: Coat,
    rider: Option<FigureKind>,
    saddled: bool,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Horse"),
            HorseBody {
                coat,
                rider,
                saddled,
                gait: HorseGait::Idle,
                gallop: 0.0,
                speed: 0.0,
                flight: None,
                seed: parent.index_u32(),
            },
            Transform::default(),
            Visibility::default(),
            ChildOf(parent),
        ))
        .id();
    commands.spawn((
        Name::new("Horse model"),
        HorseModel,
        HorsePending,
        model_transform(),
        Visibility::default(),
        ChildOf(body),
    ));
    body
}

/// Crea la escena de cada modelo de caballo en espera una vez que terminó de
/// cargar. Sin el archivo del caballo no se dibuja nada.
pub(super) fn swap_in_horses(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    models: Option<Res<LivestockModels>>,
    pending: Query<Entity, With<HorsePending>>,
) {
    let Some(models) = models else { return };
    for entity in &pending {
        let Some(model) = models.get(Species::Horse) else {
            commands.entity(entity).remove::<HorsePending>();
            continue;
        };
        let Some(server) = server.as_deref() else {
            return;
        };
        match gltf::model_state(server, &model.scene) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<HorsePending>()
                    .try_insert(WorldAssetRoot(model.scene.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).remove::<HorsePending>();
            }
            ModelState::Loading => {}
        }
    }
}

/// La ubicación de reposo de `entity` en el marco del cuerpo del modelo `root`
/// (su propio transform y los de sus ancestros, hasta el de la raíz inclusive).
pub(super) fn rest_affine(
    entity: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    transforms: &Query<&Transform>,
) -> Affine3A {
    let mut affine = Affine3A::IDENTITY;
    let mut at = entity;
    loop {
        if let Ok(t) = transforms.get(at) {
            affine = t.compute_affine() * affine;
        }
        if at == root {
            break;
        }
        let Ok(child_of) = parents.get(at) else {
            break;
        };
        at = child_of.parent();
    }
    affine
}

/// Un transform a partir de una ubicación afín.
pub(super) fn transform_of(affine: Affine3A) -> Transform {
    Transform::from_matrix(Mat4::from(affine))
}

/// A partir de qué se prepara un modelo de caballo: los nombres, los reposos y
/// las meshes de sus nodos.
#[derive(SystemParam)]
pub(super) struct HorseScene<'w, 's> {
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static Transform>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    parts: Query<'w, 's, ModelPart>,
}

/// Los almacenes a partir de los que se prepara un caballo.
#[derive(SystemParam)]
pub(super) struct HorseStores<'w> {
    cache: Option<ResMut<'w, CoatCache>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    models: Option<Res<'w, LivestockModels>>,
}

/// Prepara un modelo de caballo en el momento en que se crea su escena: su
/// pelaje y su rango, el grafo de sus clips y sus huesos para [`fit_horses`].
/// (Los aperos y el jinete los ponen sus propios módulos.)
pub(super) fn dress_horse(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    mut stores: HorseStores,
    scene: HorseScene,
    roots: Query<&ChildOf, With<HorseModel>>,
    bodies: Query<&HorseBody>,
) {
    let root = ready.entity;
    let Ok(child_of) = roots.get(root) else {
        return;
    };
    let carrier = child_of.parent();
    let (Ok(body), Some(species)) = (
        bodies.get(carrier),
        stores
            .models
            .as_ref()
            .and_then(|m| m.get(Species::Horse))
            .cloned(),
    ) else {
        return;
    };
    let HorseStores {
        cache: Some(cache),
        materials,
        ..
    } = &mut stores
    else {
        return;
    };
    let (player, probes) = dress_meshes(
        &mut commands,
        (cache, materials),
        root,
        (Animal::Horse(body.coat), Species::Horse),
        &scene.children,
        &scene.parts,
    );
    let Some(player) = player else {
        warn!("horse model without an animation player: it stands still");
        return;
    };
    commands
        .entity(player)
        .try_insert(AnimationGraphHandle(species.graph.clone()));
    commands
        .entity(root)
        .try_insert(HorseRig::new(player, probes));
    match find_bones(&scene, root) {
        Some(bones) => {
            commands.entity(root).try_insert(bones);
        }
        None => debug!("horse model without the gait layer's bones: its gallop stays the clip's"),
    }
    match find_joints(&scene, root) {
        Some(joints) => {
            commands.entity(root).try_insert(joints);
        }
        None => warn!("horse model without its root and toe joints: it jumps the clip's own arc"),
    }
}

/// Los huesos de [`HorseJoints`] bajo el modelo `root`, por su nombre
/// ([`HorseJoint::name`]).
fn find_joints(scene: &HorseScene, root: Entity) -> Option<HorseJoints> {
    let named = |joint: HorseJoint| {
        scene.children.iter_descendants(root).find(|e| {
            scene
                .names
                .get(*e)
                .is_ok_and(|n| n.as_str() == joint.name())
        })
    };
    let bone = named(HorseJoint::Root)?;
    let [a, b, c, d] = HorseJoint::TOES.map(named);
    Some(HorseJoints {
        root: bone,
        root_rest: scene.transforms.get(bone).ok()?.translation,
        toes: [a?, b?, c?, d?],
    })
}

/// Después de los clips, baja cada caballo en el aire lo que su clip lo sube
/// ([`jump_drop`]) y fija la raíz del esqueleto en su lugar (ningún clip que
/// reproduce la mueve; el único que lo hace, `Jump_Land`, no se usa: la física
/// es la que lo mueve).
pub(super) fn ground_horses(
    mut rigs: Query<(&HorseRig, &HorseJoints, &mut Transform)>,
    mut transforms: Query<&mut Transform, Without<HorseRig>>,
    parents: Query<&ChildOf>,
) {
    for (rig, joints, mut model) in &mut rigs {
        if rig.frozen || !rig.started {
            continue;
        }
        let Ok(mut bone) = transforms.get_mut(joints.root) else {
            continue;
        };
        bone.translation.x = joints.root_rest.x;
        bone.translation.z = joints.root_rest.z;
        let mut toes = [0.0; 4];
        if rig.flight.is_some() {
            let reader = transforms.as_readonly();
            for (y, &toe) in toes.iter_mut().zip(&joints.toes) {
                *y = up_to(toe, joints.root, &parents, &reader).translation.y;
            }
        }
        let drop = jump_drop(toes, rig.flight.is_some()) * Species::Horse.scale();
        let goal = model_transform().translation - Vec3::Y * drop;
        if model.translation != goal {
            model.translation = goal;
        }
    }
}

/// [`rest_affine`] sobre una consulta de transforms que también se escribe:
/// la ubicación de `entity` en el marco del padre de `root`.
fn up_to(
    entity: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    transforms: &Query<&Transform, Without<HorseRig>>,
) -> Affine3A {
    let mut affine = Affine3A::IDENTITY;
    let mut at = entity;
    loop {
        if let Ok(t) = transforms.get(at) {
            affine = t.compute_affine() * affine;
        }
        if at == root {
            return affine;
        }
        let Ok(child_of) = parents.get(at) else {
            return affine;
        };
        at = child_of.parent();
    }
}

/// Los huesos que mueve [`fit_horses`] bajo el modelo `root`: la cadena de
/// cada pata desde la raíz del esqueleto ([`HorseJoint::Root`]) hasta la punta
/// del vaso, y dónde está el padre de la raíz en el marco del modelo.
fn find_bones(scene: &HorseScene, root: Entity) -> Option<HorseBones> {
    let named = |joint: HorseJoint| {
        scene.children.iter_descendants(root).find(|e| {
            scene
                .names
                .get(*e)
                .is_ok_and(|n| n.as_str() == joint.name())
        })
    };
    let skeleton = named(HorseJoint::Root)?;
    let mut legs: [Vec<(Entity, &str)>; 4] = Default::default();
    for (leg, chain) in legs.iter_mut().enumerate() {
        let mut at = named(HorseJoint::Toe(leg))?;
        loop {
            chain.push((at, scene.names.get(at).ok()?.as_str()));
            if at == skeleton {
                break;
            }
            at = scene.parents.get(at).ok()?.parent();
        }
        chain.reverse();
    }
    let above = scene.parents.get(skeleton).ok()?.parent();
    let model = scene.transforms.get(root).ok()?.compute_affine();
    let base = model.inverse() * rest_affine(above, root, &scene.parents, &scene.transforms);
    HorseBones::new(base, legs)
}

/// Ajusta la pose de cada caballo que se reproduce después de sus clips (ver
/// [`gait`](super::gait)): la caminata estirada a su tranco
/// ([`HorseRig::stride`]) y los vasos por encima del suelo.
pub(super) fn fit_horses(
    rigs: Query<(&HorseRig, &HorseBones)>,
    mut transforms: Query<&mut Transform>,
) {
    for (rig, bones) in &rigs {
        if rig.frozen || !rig.started {
            continue;
        }
        let Some(weights) = rig.weights.first_chunk::<CLIP_COUNT>() else {
            continue;
        };
        fit_horse(bones, &mut transforms, weights, rig.stride);
    }
}

/// Gira cada caballo ensillado hacia su rumbo y reproduce sus clips (ver las
/// notas del módulo): ponderados y al ritmo de la velocidad, empezados en su
/// punto con semilla, congelados lejos o fuera de la vista salvo que sea el del
/// jugador.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_horses(
    mut commands: Commands,
    time: Res<Time>,
    models: Option<Res<LivestockModels>>,
    loaded: Option<Res<Assets<AnimationClip>>>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    poses: Query<&VisualPose>,
    mut bodies: Query<(&ChildOf, &mut HorseBody, &mut Transform)>,
    mut rigs: Query<(&ChildOf, &mut HorseRig, &GlobalTransform)>,
    mut players: Query<&mut AnimationPlayer>,
    seen: Query<&ViewVisibility>,
) {
    let dt = time.delta_secs();
    let clock = time.elapsed_secs();
    for (child_of, mut body, mut transform) in &mut bodies {
        let Ok(pose) = poses.get(child_of.parent()) else {
            continue;
        };
        transform.rotation = body_rotation(pose.heading);
        body.gait = HorseGait::of(pose.animation, pose.speed);
    }
    let eye = view_eye(cameras);
    let species = models.as_ref().and_then(|m| m.get(Species::Horse));
    for (child_of, mut rig, at) in &mut rigs {
        let Ok((parent, mut body, _)) = bodies.get_mut(child_of.parent()) else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        let in_view =
            eye.is_none() || any_in_view(&rig.probes, |p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = !body.never_freezes() && frozen(rig.frozen, distance, in_view);
        let Some(species) = species else {
            rig.frozen = freeze;
            continue;
        };
        if freeze != rig.frozen {
            rig.frozen = freeze;
            let mut player = commands.entity(rig.player);
            if freeze {
                player.try_remove::<AnimationGraphHandle>();
            } else {
                player.try_insert(AnimationGraphHandle(species.graph.clone()));
            }
        }
        if rig.frozen {
            continue;
        }
        let Ok(mut player) = players.get_mut(rig.player) else {
            continue;
        };
        // Negativa al retroceder: la zancada se reproduce hacia atrás,
        // elegida y ponderada según su magnitud.
        // Girar en el lugar da pasos cortos ([`stepping_speed`]).
        let velocity = match poses.get(parent.parent()) {
            Ok(pose) if pose.animation != AnimationState::Idle => {
                stepping_speed(pose.speed, pose.turn, HORSE_TURN_STEP_RADIUS)
            }
            Ok(pose) => stepping_speed(0.0, pose.turn, HORSE_TURN_STEP_RADIUS),
            _ => 0.0,
        };
        let speed = velocity.abs();
        let stride = if gallops(rig.lead == Clip::Gallop, speed) {
            Clip::Gallop
        } else {
            Clip::Walk
        };
        let flight = poses.get(parent.parent()).ok().and_then(|p| p.flight);
        let fresh = !rig.started;
        if fresh {
            let Some(loaded) = loaded.as_deref() else {
                continue;
            };
            let durations: Option<Vec<f32>> = species
                .clips
                .iter()
                .map(|c| loaded.get(c).map(AnimationClip::duration))
                .collect();
            let Some(durations) = durations else {
                continue;
            };
            for (k, duration) in durations.into_iter().enumerate().take(LOADED_COUNT) {
                rig.durations[k] = duration;
            }
            for clip in HORSE_CLIPS {
                player
                    .play(species.nodes[clip.index()])
                    .repeat()
                    .seek_to(start_offset(body.seed, clip) * rig.durations[clip.index()]);
            }
            rig.started = true;
        }
        // El salto (S3): en el aire, `Jump` fundido en `S.Jump`, fijados en el
        // punto del vuelo; en el suelo, el aterrizaje (`Jump_Run` al ritmo del
        // suelo o el resto de `S.Jump`), y después el galope desde su primer
        // cuadro, donde termina `Jump_Run`, o la espera.
        let air = flight.map(jump_frame);
        let mut shown = None;
        if air.is_some() {
            rig.landing = None;
        } else {
            if body.flight.is_some() {
                // Recién aterrizado (también de un vuelo cortado).
                let land = landing(speed);
                rig.landing = Some((land, landing_from(land)));
            }
            if let Some((clip, at)) = rig.landing {
                let rate = match clip {
                    Clip::JumpRun => gait_rate(Clip::JumpRun, speed),
                    _ => 1.0,
                };
                let at = at + rate * dt;
                if at < rig.durations[clip.index()] {
                    rig.landing = Some((clip, at));
                    shown = Some((clip, at));
                } else {
                    rig.landing = None;
                    if clip == Clip::JumpRun
                        && let Some(active) =
                            player.animation_mut(species.nodes[Clip::Gallop.index()])
                    {
                        active.seek_to(0.0);
                        rig.lead = Clip::Gallop;
                    }
                }
            }
        }
        let mut goal = [0.0; LOADED_COUNT];
        match (air, shown) {
            (Some(air), _) => {
                goal[Clip::GallopJump.index()] = 1.0 - air.blend;
                goal[Clip::ShortJump.index()] = air.blend;
            }
            (None, Some((clip, _))) => goal[clip.index()] = 1.0,
            (None, None) => {
                let weights = gait_weights(body.idle(clock), stride, speed);
                goal[..CLIP_COUNT].copy_from_slice(&weights);
            }
        }
        if fresh {
            rig.weights = goal;
        }
        // El clip de zancada que toma el relevo empieza al mismo paso que el
        // que sale.
        let lead = stride;
        if lead != rig.lead && shown.is_none() && air.is_none() {
            let from = rig.lead;
            let phase = player
                .animation(species.nodes[from.index()])
                .map(|a| a.seek_time() / rig.durations[from.index()].max(1e-3));
            if let (Some(phase), Some(active)) =
                (phase, player.animation_mut(species.nodes[lead.index()]))
            {
                active.seek_to(in_step(lead, phase) * rig.durations[lead.index()]);
            }
            rig.lead = lead;
        }
        let rate = if flight.is_some() {
            JUMP_BLEND_RATE
        } else {
            HORSE_BLEND_RATE
        };
        let k = 1.0 - (-rate * dt).exp();
        for (w, t) in rig.weights.iter_mut().zip(goal) {
            *w += (t - *w) * k;
        }
        body.gallop = rig.weights[Clip::Gallop.index()] + rig.weights[Clip::JumpRun.index()];
        body.speed = speed;
        rig.stride = walk_stride(speed);
        body.flight = flight;
        rig.flight = flight;
        for clip in HORSE_CLIPS {
            let Some(active) = player.animation_mut(species.nodes[clip.index()]) else {
                continue;
            };
            active.set_weight(rig.weights[clip.index()]);
            if matches!(clip, Clip::GallopJump | Clip::JumpRun | Clip::ShortJump) {
                // Nunca se reproducen con el reloj (así que nunca hacen
                // bucle): se fijan en su punto, y se dejan ahí mientras se
                // desvanecen.
                let at = match (air, shown) {
                    (Some(air), _) if clip == Clip::GallopJump => Some(air.jump),
                    (Some(air), _) if clip == Clip::ShortJump => Some(air.short),
                    (None, Some((c, at))) if c == clip => Some(at),
                    _ => None,
                };
                if let Some(at) = at {
                    active.seek_to(at);
                }
                active.set_speed(0.0);
            } else if clip == Clip::Gallop {
                // Cuadro a cuadro: más rápido mientras apoya, más despacio en
                // la suspensión.
                let phase = active.seek_time() / rig.durations[clip.index()].max(1e-3);
                active.set_speed(gallop_rate(phase, velocity));
            } else {
                active.set_speed(gait_rate(clip, velocity));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn luma(c: Rgb) -> f32 {
        0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32
    }

    #[test]
    fn every_mane_stands_apart_from_its_coat() {
        // La crin no se pierde contra el cuello: más oscura o más clara que
        // el pelaje, según el pelaje (negra en el zaino y el bayo, blanca en
        // el tordillo, rubia en el alazán, oscura en el overo), en al menos
        // un tercio de la luminancia del más claro de los dos. La crin del
        // alazán (176, 104, 58) se fundía con el cuello al galope (L55).
        for coat in Coat::ALL {
            let (mane, body) = (luma(coat.mane()), luma(coat.body()));
            let apart = (mane - body).abs() / mane.max(body);
            assert!(apart > 1.0 / 3.0, "{coat:?}: mane {mane}, coat {body}");
        }
    }

    #[test]
    fn coats_are_seeded_deterministically_and_hidalgo_rides_an_alazan() {
        assert_eq!(Coat::HIDALGO, Coat::Alazan);
        for seed in 0..50 {
            assert_eq!(Coat::seeded(seed), Coat::seeded(seed));
        }
        let seen: std::collections::HashSet<_> = (0..200).map(Coat::seeded).collect();
        assert_eq!(seen.len(), Coat::ALL.len(), "every coat turns up");
    }

    #[test]
    fn the_idle_fades_into_the_stride_and_the_gallop_takes_over_outright() {
        let only = |w: [f32; CLIP_COUNT], clip: Clip| {
            w.iter()
                .enumerate()
                .all(|(k, w)| *w == if k == clip.index() { 1.0 } else { 0.0 })
        };
        let (walk, gallop) = (Clip::Walk, Clip::Gallop);
        assert!(only(gait_weights(Clip::Idle, walk, 0.0), Clip::Idle));
        assert!(only(gait_weights(Clip::Idle2, gallop, 0.0), Clip::Idle2));
        assert!(only(gait_weights(Clip::Idle, walk, 3.0), walk));
        assert!(only(gait_weights(Clip::Idle, gallop, 12.2), gallop));
        let setting_off = gait_weights(Clip::Idle, walk, 0.3);
        assert!(setting_off[0] > 0.3 && setting_off[walk.index()] > 0.3);
        for speed in [0.0, 0.1, 0.3, 0.6, 1.0, 1.5, 3.0, 4.8, 8.0, 12.2] {
            for stride in [walk, gallop] {
                let w = gait_weights(Clip::Idle, stride, speed);
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{speed}");
                // Nunca la caminata y el galope a la vez.
                assert!(w[walk.index()] * w[gallop.index()] == 0.0);
            }
        }
        // La caminata de la lógica camina, la mitad rápida del trote y el
        // galope galopan, con un margen hacia cada lado para que un ritmo
        // constante nunca parpadee.
        assert!(!gallops(false, 3.0) && !gallops(false, 6.5) && gallops(false, 6.7));
        assert!(gallops(true, 6.5) && !gallops(true, 6.3));
        assert!(gallops(false, 12.2) && gallops(true, 12.2));
    }

    #[test]
    fn the_walk_plays_the_clip_unstretched_up_to_4_5_kmh_and_splits_the_extra_above() {
        // Hasta 4,5 km/h el clip de henry tal cual, sin estirar el tranco, a
        // 1,35 veces su ritmo en el tope (usuario, 2026-11-16).
        assert!((WALK_TOP - 4.5 / 3.6).abs() < 1e-6, "{WALK_TOP}");
        for speed in [WALK_SPEED, 1.1, WALK_TOP] {
            let rate = gait_rate(Clip::Walk, speed);
            assert_eq!(walk_stride(speed), 1.0, "{speed}");
            assert_eq!(walk_stride(-speed), 1.0, "backing {speed}");
            assert!((WALK_SPEED * rate - speed).abs() < 1e-4, "{speed}: {rate}");
        }
        assert!((gait_rate(Clip::Walk, WALK_TOP) - 1.36).abs() < 0.01);
        // Por encima (la franja del trote y los viajeros al trote), lo que
        // falta se reparte por igual entre el ritmo y el tranco, hasta 10,8
        // km/h, y juntos muestran la velocidad.
        for speed in [2.0, STRETCH_TOP] {
            let even = (speed / WALK_TOP).sqrt();
            let (rate, stride) = (gait_rate(Clip::Walk, speed), walk_stride(speed));
            assert!((stride - even).abs() < 1e-5, "{speed}: stride {stride}");
            let top = gait_rate(Clip::Walk, WALK_TOP);
            assert!((rate - top * even).abs() < 1e-5, "{speed}: rate {rate}");
            assert!((WALK_SPEED * rate * stride - speed).abs() < 1e-4, "{speed}");
        }
        assert_eq!(walk_stride(4.8), walk_stride(STRETCH_TOP));
        assert_eq!(
            gait_rate(Clip::Walk, 4.8),
            gait_rate(Clip::Walk, STRETCH_TOP)
        );
        // Por debajo, como antes: sin estirar, al ritmo del suelo con su piso
        // (retroceder y girar en el lugar).
        assert_eq!(walk_stride(0.5), 1.0);
        assert!((gait_rate(Clip::Walk, 0.5) - 0.5 / WALK_SPEED).abs() < 1e-6);
        assert_eq!(gait_rate(Clip::Walk, 0.0), 0.3, "clamped");
        assert_eq!(gait_rate(Clip::Idle, 9.0), 1.0, "an idle as recorded");
        assert_eq!(gait_rate(Clip::Eating, 9.0), 1.0);
    }

    #[test]
    fn backing_up_plays_the_walk_and_the_gallop_backward_and_the_idles_forward() {
        for clip in [Clip::Walk, Clip::Gallop] {
            for speed in [0.5, 1.5, 4.8, 12.2, 19.4] {
                let ahead = gait_rate(clip, speed);
                assert_eq!(gait_rate(clip, -speed), -ahead, "{clip:?} at {speed}");
            }
            assert!(gait_rate(clip, -0.0) > 0.0, "a stop is no reverse");
        }
        assert_eq!(gait_rate(Clip::Idle, -1.5), 1.0);
        assert_eq!(gait_rate(Clip::Eating, -1.5), 1.0);
    }

    #[test]
    fn a_jump_lands_running_while_it_gallops_and_stops_otherwise() {
        assert_eq!(landing(12.2), Clip::JumpRun);
        assert_eq!(landing(GALLOP_FROM.0 + 0.1), Clip::JumpRun);
        assert_eq!(landing(-12.2), Clip::JumpRun, "by its magnitude");
        assert_eq!(landing(3.0), Clip::ShortJump);
        assert_eq!(landing(0.0), Clip::ShortJump);
        // `Jump_Run` desde el cuadro que sigue al apoyo de `S.Jump`; `S.Jump`
        // desde su apoyo.
        assert_eq!(landing_from(Clip::JumpRun), RUN_FROM);
        assert_eq!(landing_from(Clip::ShortJump), SHORT_TOUCHDOWN);
    }

    #[test]
    fn a_jump_spreads_the_clips_flight_over_the_physics_flight() {
        // Del despegue de `Jump` al apoyo de `S.Jump`, fundidos en lo alto.
        let start = jump_frame(0.0);
        assert_eq!((start.jump, start.blend), (JUMP_TAKEOFF, 0.0));
        let end = jump_frame(1.0);
        assert!((end.short - SHORT_TOUCHDOWN).abs() < 1e-4, "{end:?}");
        assert_eq!(end.blend, 1.0);
        // Siempre hacia adelante, de `Jump` a `S.Jump` y nunca de vuelta; los
        // dos clips al mismo paso.
        let mut last = jump_frame(0.0);
        for k in 1..=100 {
            let now = jump_frame(k as f32 / 100.0);
            assert!(
                now.jump >= last.jump && now.blend >= last.blend,
                "{k}: {now:?}"
            );
            assert!(
                (now.jump - now.short - SHORT_BEHIND).abs() < 1e-4,
                "{k}: {now:?}"
            );
            last = now;
        }
        // El fundido, solo en lo alto.
        for k in 0..=100 {
            let now = jump_frame(k as f32 / 100.0);
            let inside = now.jump > JUMP_SEAM.0 && now.jump < JUMP_SEAM.1;
            assert_eq!(now.blend > 0.0 && now.blend < 1.0, inside, "{k}: {now:?}");
        }
        // Se demora en el impulso.
        let early = jump_frame(0.1).jump;
        let span = SHORT_TOUCHDOWN + SHORT_BEHIND - JUMP_TAKEOFF;
        assert!(early - JUMP_TAKEOFF < 0.1 * span, "{early}");
    }

    #[test]
    fn in_the_air_the_lowest_toe_rides_the_physics_arc() {
        let toes = [0.5, 0.6, 1.0, 1.2];
        assert_eq!(jump_drop(toes, false), 0.0, "on the ground");
        assert!((jump_drop(toes, true) - (0.5 - TOE_STANDING)).abs() < 1e-6);
        assert_eq!(jump_drop([0.05, 0.6, 1.0, 1.2], true), 0.0, "never raised");
    }

    #[test]
    fn a_ridden_horse_stands_still_and_a_tied_one_changes_idle() {
        let mut body = HorseBody {
            coat: Coat::Zaino,
            rider: Some(FigureKind::Protagonist),
            saddled: true,
            gait: HorseGait::Idle,
            gallop: 0.0,
            speed: 0.0,
            flight: None,
            seed: 11,
        };
        for k in 0..40 {
            assert_eq!(body.idle(k as f32 * 14.0), Clip::Idle);
        }
        assert!(body.never_freezes());
        body.rider = Some(FigureKind::Hidalgo);
        assert!(!body.never_freezes(), "only the player's");
        body.rider = None;
        assert!(!body.never_freezes());
        let seen: std::collections::HashSet<Clip> =
            (0..40).map(|k| body.idle(k as f32 * 14.0)).collect();
        assert_eq!(seen, TIED_IDLES.into(), "{seen:?}");
        assert_eq!(body.idle(30.0), body.idle(30.0));
    }

    // --- con el caballo, el jinete y los clips incluidos ---------------------

    use crate::character::cpu;
    use crate::character::{CharacterVisual, CharacterVisualPlugin};
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;

    /// Los plugins de fauna y de personajes con los modelos incluidos, y
    /// `visual` en el origen mirando hacia +X, moviéndose a `speed`; vuelve
    /// cuando el caballo reproduce sus clips.
    pub(crate) fn saddled(visual: CharacterVisual, speed: f32) -> (App, Entity) {
        saddled_every(visual, speed, 50)
    }

    /// [`saddled`], con frames de `ms` milisegundos.
    pub(crate) fn saddled_every(visual: CharacterVisual, speed: f32, ms: u64) -> (App, Entity) {
        use crate::gltf::tests::{root, unfinished_app};
        use bevy::image::{CompressedImageFormatSupport, CompressedImageFormats, ImageLoader};
        let mut app = unfinished_app(&root());
        // Una GPU que acepta BC7, como la de escritorio: los mapas UASTC de los
        // aperos se transcodifican a él (sin ningún formato comprimido, el
        // cargador de Bevy 0.20 hace panic en lugar de usar una alternativa).
        let formats = CompressedImageFormats::BC;
        app.register_asset_loader(ImageLoader::new(formats))
            .insert_resource(CompressedImageFormatSupport(formats));
        app.init_asset::<SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(ms),
            ))
            .add_plugins((crate::fauna::FaunaPlugin, CharacterVisualPlugin));
        // Frames desde que cambió por última vez un asset de escena: una escena
        // creada antes de que su asset se estabilice se vuelve a crear, y los
        // tests retienen sus huesos.
        #[derive(Resource, Default)]
        struct Settled(u32);
        app.init_resource::<Settled>().add_systems(
            Update,
            |mut events: MessageReader<AssetEvent<bevy::world_serialization::WorldAsset>>,
             mut settled: ResMut<Settled>| {
                settled.0 = if events.read().count() > 0 {
                    0
                } else {
                    settled.0 + 1
                };
            },
        );
        app.finish();
        app.cleanup();
        let animation = match speed {
            s if s <= 0.0 => AnimationState::Idle,
            s if s <= 3.0 => AnimationState::Walk,
            _ => AnimationState::Gallop,
        };
        let me = app
            .world_mut()
            .spawn((
                visual,
                VisualPose {
                    animation,
                    speed,
                    ..default()
                },
            ))
            .id();
        // Hasta que el caballo reproduce, el mismo modelo durante un rato, con
        // sus assets estabilizados.
        const QUIET: u32 = 40;
        let mut held = (Entity::PLACEHOLDER, 0);
        for _ in 0..3000 {
            app.update();
            let world = app.world_mut();
            let playing = world
                .query::<(Entity, &HorseRig)>()
                .iter(world)
                .find(|(_, r)| r.started)
                .map(|(_, r)| r.player);
            match playing {
                Some(player) if player == held.0 => held.1 += 1,
                Some(player) => held = (player, 0),
                None => held = (Entity::PLACEHOLDER, 0),
            }
            // Quieto durante 40 frames (al menos 200 ms de tiempo real, sin
            // importar qué tan cargada esté la máquina): un archivo que llegara
            // más tarde volvería a crear la escena en medio de una medición.
            // Y sin aperos a medio vestir: el recado modelado puede llegar
            // después de que el caballo reproduce (con muchos mapas cargando a
            // la vez), y los tests de los aperos leerían un recado sin armar.
            // Un caballo que no lleva aperos nunca deja ninguno pendiente.
            let tack_dressed = world
                .resource::<crate::character::tack::PendingTack>()
                .0
                .is_empty();
            if held.1 >= QUIET && world.resource::<Settled>().0 >= QUIET && tack_dressed {
                return (app, me);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the horse never played");
    }

    #[test]
    fn a_draught_horse_wears_no_saddle_and_stands_still() {
        use crate::character::tack::HorsePiece;
        let (mut app, me) = saddled_every(CharacterVisual::Draught(Coat::Zaino), 0.0, 50);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        assert!(
            app.world()
                .get::<crate::character::tack::Tack>(carrier)
                .is_none(),
            "no tack"
        );
        let mut pieces = app.world_mut().query::<&HorsePiece>();
        assert_eq!(pieces.iter(app.world()).count(), 0, "no saddle or bridle");
        let body = app.world().get::<HorseBody>(carrier).unwrap().clone();
        for k in 0..40 {
            assert_eq!(body.idle(k as f32 * 14.0), Clip::Idle, "in harness");
        }
    }

    #[test]
    fn the_galloping_horse_bends_its_back_through_the_stride() {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..80 {
            ride(&mut app, me, 12.2);
        }
        let mut flex = Vec::new();
        for _ in 0..40 {
            ride(&mut app, me, 12.2);
            let [croup, withers] = [HorseJoint::Croup, HorseJoint::Withers]
                .map(|j| named(app.world(), carrier, j.name()));
            let turn = |e: Entity| {
                app.world()
                    .get::<GlobalTransform>(e)
                    .unwrap()
                    .to_scale_rotation_translation()
                    .1
            };
            flex.push(turn(croup).inverse() * turn(withers));
        }
        // No es un bloque rígido: la grupa y la cruz giran por separado y
        // vuelven.
        let bend = flex
            .iter()
            .map(|q| q.angle_between(flex[0]))
            .fold(0.0, f32::max);
        assert!(bend > 0.1, "the horse's back bends over a stride: {bend}");
    }

    /// `visual` cabalgando a `speed` (un galope) en la fracción `flight` de un
    /// salto, durante un frame.
    fn fly(app: &mut App, me: Entity, flight: Option<f32>) {
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation: AnimationState::Gallop,
            speed: 12.2,
            flight,
            ..default()
        });
        app.update();
    }

    /// El rig que se reproduce: una escena creada de nuevo bajo carga deja uno
    /// nuevo, sin empezar, a su lado hasta que se estabiliza.
    fn rig_of(app: &mut App) -> (Entity, [f32; LOADED_COUNT]) {
        let world = app.world_mut();
        let (_, rig) = world
            .query::<(Entity, &HorseRig)>()
            .iter(world)
            .find(|(_, r)| r.started)
            .unwrap();
        (rig.player, rig.weights)
    }

    /// La punta del vaso de cada pata desde su articulación superior (el codo
    /// de la mano, la cadera de la pata), en el marco del cuerpo del caballo,
    /// en el orden de las patas ([`HorseJoint::TOES`]).
    fn hooves_from_joints(app: &App, carrier: Entity) -> [Vec3; 4] {
        std::array::from_fn(|leg| {
            let joint = if leg < 2 {
                HorseJoint::Lower(leg)
            } else {
                HorseJoint::Upper(leg)
            };
            in_body(app, carrier, HorseJoint::Toe(leg).name()) - in_body(app, carrier, joint.name())
        })
    }

    /// La altura de la punta de cada vaso en el marco del cuerpo del caballo.
    fn toe_heights(app: &App, carrier: Entity) -> [f32; 4] {
        HorseJoint::TOES.map(|t| in_body(app, carrier, t.name()).y)
    }

    #[test]
    fn over_a_jump_the_hinds_push_off_stretched_back_then_fold_up_and_the_fores_tuck_at_takeoff() {
        // S3 de `odd/tasks/henry-horse.md`: el despegue de `Jump`. Antes un
        // ajuste propio estiraba las patas traseras al despegar (el clip del
        // caballo anterior las dejaba bajo la panza); el de henrysteve973 ya
        // impulsa con ellas estiradas hacia atrás.
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let standing = TOE_STANDING * Species::Horse.scale();
        let at = |app: &mut App, f: f32| {
            for _ in 0..4 {
                fly(app, me, Some(f));
            }
            (toe_heights(app, carrier), hooves_from_joints(app, carrier))
        };
        // Al dejar el suelo: cada pata trasera estirada hacia atrás desde la
        // cadera, casi de su largo completo, con el vaso todavía abajo; las
        // manos ya recogidas cerca de los codos, arriba (a los 0.56 s de
        // `Jump`, un séptimo del vuelo: el clip va casi a su ritmo).
        let (toes, legs) = at(&mut app, 0.15);
        for leg in 2..4 {
            let hind = legs[leg];
            assert!(hind.x < -0.3 && hind.length() > 0.9, "pushing: {hind}");
            assert!(
                toes[leg] < standing + 0.06,
                "pushing off the ground: {toes:?}"
            );
        }
        for leg in 0..2 {
            let fore = legs[leg];
            assert!(fore.length() < 0.4, "tucked: {fore}");
            assert!(toes[leg] > standing + 0.5, "tucked up: {toes:?}");
        }
        // Pasado lo alto el impulso ya terminó: las traseras plegadas, arriba
        // y por detrás de la cadera (Muybridge, Animal Locomotion, lám. 639).
        // En lo alto mismo `Jump` todavía las lleva estiradas hacia atrás; se
        // pliegan mientras se funde en `S.Jump`, cuyo lomo casi horizontal
        // las deja a poco más alto que las manos.
        let (toes, legs) = at(&mut app, 0.7);
        for leg in 2..4 {
            let hind = legs[leg];
            assert!(hind.x < 0.0 && hind.length() < 0.9, "folded: {hind}");
            assert!(toes[leg] > standing + 0.15, "folded up: {toes:?}");
        }
    }

    #[test]
    fn a_jump_holds_jump_then_s_jump_at_the_flights_point_and_gallops_on_through_jump_run() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let nodes = app
            .world()
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .unwrap()
            .nodes
            .clone();
        let seek = |app: &mut App, clip: Clip| {
            let (player, _) = rig_of(app);
            let player = app.world().get::<AnimationPlayer>(player).unwrap();
            let active = player.animation(nodes[clip.index()]).unwrap();
            (active.seek_time(), active.speed())
        };
        // El vaso más bajo sobre el suelo del cuerpo del caballo.
        let lowest = |app: &App| {
            HorseJoint::TOES
                .map(|t| in_body(app, carrier, t.name()).y)
                .into_iter()
                .fold(f32::MAX, f32::min)
        };
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        // Un vaso apoyado (al galope, a veces los cuatro están en el aire).
        let standing = TOE_STANDING * Species::Horse.scale();
        for flight in [0.0, 0.3, 0.6, 0.9, 0.97, 1.0] {
            for _ in 0..10 {
                fly(&mut app, me, Some(flight));
            }
            let frame = jump_frame(flight);
            let (_, weights) = rig_of(&mut app);
            for (clip, at, weight) in [
                (Clip::GallopJump, frame.jump, 1.0 - frame.blend),
                (Clip::ShortJump, frame.short, frame.blend),
            ] {
                assert!(
                    (weights[clip.index()] - weight).abs() < 0.01,
                    "{flight}: {weights:?}"
                );
                if weight > 0.0 {
                    let (seek_at, speed) = seek(&mut app, clip);
                    assert!((seek_at - at).abs() < 1e-3, "{flight}: {seek_at} for {at}");
                    assert_eq!(speed, 0.0, "never with the clock");
                }
            }
            // El vaso más bajo sigue el arco de la física, no el del clip (que
            // en lo alto lo sube cerca de un metro).
            let low = lowest(&app);
            assert!(
                (low - standing).abs() < 0.06,
                "{flight}: {low} vs {standing}"
            );
        }
        // Aterrizado: `Jump_Run` sigue desde el cuadro que sigue al apoyo de
        // `S.Jump` al ritmo del galope, sin el bajón del aire.
        ride(&mut app, me, 12.2);
        let (at, _) = seek(&mut app, Clip::JumpRun);
        let step = 0.02 * gait_rate(Clip::Gallop, 12.2);
        assert!(at > RUN_FROM && at < RUN_FROM + 2.0 * step, "{at}");
        let model = |app: &mut App| {
            let world = app.world_mut();
            world
                .query_filtered::<&Transform, With<HorseModel>>()
                .single(world)
                .unwrap()
                .translation
        };
        assert_eq!(model(&mut app), model_transform().translation);
        // Cuando termina, el galope sigue desde su primer cuadro, donde
        // termina `Jump_Run`.
        let mut ended = false;
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
            let world = app.world_mut();
            let rig = world.query::<&HorseRig>().single(world).unwrap().clone();
            if rig.landing.is_none() {
                let (gallop, _) = seek(&mut app, Clip::Gallop);
                assert!(gallop < 2.0 * step, "{gallop}");
                assert_eq!(rig.lead, Clip::Gallop);
                ended = true;
                break;
            }
        }
        assert!(ended, "the landing ends");
        for _ in 0..30 {
            ride(&mut app, me, 12.2);
        }
        let (_, weights) = rig_of(&mut app);
        let goal = gait_weights(Clip::Idle, Clip::Gallop, 12.2);
        for (w, g) in weights.iter().zip(goal) {
            assert!((w - g).abs() < 0.05, "{weights:?} vs {goal:?}");
        }
        assert!(
            weights[CLIP_COUNT..].iter().all(|w| *w < 0.05),
            "{weights:?}"
        );
    }

    /// Henry's tail joints, from the dock down, and the end of the last one
    /// (the tip of the tail's bones; the hair cards hang a little past it).
    const TAIL: [&str; 5] = [
        "BN_Tail_00_061_067",
        "BN_Tail_01_062_068",
        "BN_Tail_02_063_069",
        "BN_Tail_03_064_070",
        "BN_Tail_04_065_071",
    ];
    const TAIL_TIP: &str = "BN_Tail_04_065_end_098";

    /// Dónde está `name` en el marco del cuerpo del caballo de `carrier`.
    fn in_body(app: &App, carrier: Entity, name: &str) -> Vec3 {
        let world = app.world();
        let to_body = world
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let e = named(world, carrier, name);
        to_body.transform_point3(world.get::<GlobalTransform>(e).unwrap().translation())
    }

    #[test]
    fn the_tail_streams_back_at_the_gallop_and_over_a_jump_never_between_the_hind_legs() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // La punta de la cola, detrás de las nalgas y de los corvejones: entre
        // las patas quedaría por delante de ellos. Where it is along the
        // body (the most forward it gets), and how far behind the nearer
        // hock (the least).
        let behind = |app: &mut App| {
            let tip = in_body(app, carrier, TAIL_TIP);
            let hocks = [2, 3].map(|leg| in_body(app, carrier, HorseJoint::Cannon(leg).name()).x);
            let hock = hocks.iter().map(|&h| h - tip.x).fold(f32::MAX, f32::min);
            (-tip.x, hock)
        };
        let worst = |a: (f32, f32), b: (f32, f32)| (a.0.min(b.0), a.1.min(b.1));
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let mut galloping = (f32::MAX, f32::MAX);
        for _ in 0..30 {
            ride(&mut app, me, 12.2);
            galloping = worst(galloping, behind(&mut app));
        }
        let mut jumping = (f32::MAX, f32::MAX);
        for f in 0..=20 {
            fly(&mut app, me, Some(f as f32 / 20.0));
            jumping = worst(jumping, behind(&mut app));
        }
        println!(
            "the tail's tip at least this far back and behind the hocks: galloping {galloping:.3?} m, jumping {jumping:.3?} m"
        );
        for (when, (back, hocks)) in [("galloping", galloping), ("over a jump", jumping)] {
            // On henry (user, 2026-11-18: "Ajustarlas a henry"): over a jump
            // the tip gets as far forward as 0.758 m back and 6.3 cm behind
            // the hocks (the old horse: past 0.85 m and 10 cm).
            assert!(back > 0.73, "{when}: the tail's tip only {back} m back");
            assert!(hocks > 0.04, "{when}: {hocks} m behind the hocks");
        }
    }

    #[test]
    fn a_jump_lands_on_the_fores_and_the_hinds_last() {
        // S3: el vuelo termina en el apoyo de `S.Jump`: la mano izquierda en
        // el suelo y la derecha apenas arriba (el clip la trae levantada unos
        // centímetros, también parado), las dos estiradas.
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        for _ in 0..4 {
            fly(&mut app, me, Some(1.0));
        }
        let standing = TOE_STANDING * Species::Horse.scale();
        let (toes, legs) = (
            toe_heights(&app, carrier),
            hooves_from_joints(&app, carrier),
        );
        assert!(
            toes[0] < standing + 0.03,
            "landing on the left fore: {toes:?}"
        );
        for leg in 0..2 {
            assert!(toes[leg] < standing + 0.1, "both fores down: {toes:?}");
            // Casi todo su largo (de pie, unos 0.86 m del codo a la punta).
            assert!(legs[leg].length() > 0.7, "unfolded: {:?}", legs[leg]);
        }
        // Las patas traseras aterrizan después: todavía arriba, bajando bajo
        // las caderas, cuando las manos llegan al suelo.
        for leg in 2..4 {
            assert!(toes[leg] > standing + 0.4, "{toes:?} {:?}", legs[leg]);
        }
    }

    #[test]
    fn over_a_jump_the_body_never_dives_more_steeply_than_the_arc_comes_down() {
        // El usuario (2026-11-14): "la caida es muy vertical". El cuerpo (de
        // la grupa a la cruz) baja de cabeza a lo sumo tan empinado como baja
        // el arco de la física al aterrizar (unos 24 grados a 12.2 m/s): el
        // salto de un alambrado, no la caída de un barranco (`Jump_Run` y
        // `Jump_Land` aterrizan a 38-41 grados).
        use pampa_core::Rider;
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let arc = (Rider::JUMP_SPEED / 12.2).atan().to_degrees();
        for k in 0..=20 {
            let f = k as f32 / 20.0;
            fly(&mut app, me, Some(f));
            let back = in_body(&app, carrier, HorseJoint::Withers.name())
                - in_body(&app, carrier, HorseJoint::Croup.name());
            let pitch = back.y.atan2(back.x).to_degrees();
            assert!(
                pitch > -arc,
                "flight {f}: the back dives {pitch} deg (arc {arc})"
            );
        }
    }

    #[test]
    fn the_galloping_tail_flows_in_a_curve_and_swings_with_the_stride() {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..80 {
            ride(&mut app, me, 12.2);
        }
        let (mut bends, mut tips) = (Vec::new(), Vec::new());
        // Un ciclo de galope y algo más.
        for _ in 0..50 {
            ride(&mut app, me, 12.2);
            let p = |n: &str| in_body(&app, carrier, n);
            let dock = p(TAIL[2]) - p(TAIL[0]);
            let hair = p(TAIL_TIP) - p(TAIL[3]);
            bends.push(dock.angle_between(hair).to_degrees());
            tips.push(p(TAIL_TIP).y);
        }
        let range = |v: &[f32]| {
            v.iter().copied().fold(f32::MIN, f32::max) - v.iter().copied().fold(f32::MAX, f32::min)
        };
        // No es una vara recta: el pelo cae en curva desde el maslo levantado,
        // y la curva y la punta suben y bajan con cada tranco.
        let most = bends.iter().copied().fold(0.0, f32::max);
        assert!(most > 30.0 && range(&bends) > 15.0, "{bends:?}");
        assert!(range(&tips) > 0.15, "{tips:?}");
    }

    #[test]
    fn the_tail_hangs_to_about_the_hocks_and_the_walk_nods_the_head_a_few_centimetres() {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 0.0, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // El pelo de la cola, detrás de las nalgas: hasta la altura de los
        // corvejones, apenas por debajo (un criollo; antes llegaba casi al
        // suelo).
        let drawn = cpu::drawn(app.world_mut(), Some(me));
        let to_body = app
            .world()
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let lowest = drawn
            .iter()
            .filter(|d| d.name == "Hair")
            .flat_map(|d| d.positions.iter().map(|&p| to_body.transform_point3(p)))
            .filter(|p| p.x < -0.8)
            .map(|p| p.y)
            .fold(f32::MAX, f32::min);
        let hocks = in_body(&app, carrier, HorseJoint::Cannon(2).name()).y;
        // Al paso la cabeza sube y baja unos 9 cm, dos veces por tranco
        // (Loscher et al. 2016: 9.1 ± 3.4 cm), no picotea.
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 1.5, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 1.5);
        }
        let heights: Vec<f32> = (0..60)
            .map(|_| {
                ride(&mut app, me, 1.5);
                in_body(&app, carrier, HorseJoint::Poll.name()).y
            })
            .collect();
        let range = heights.iter().copied().fold(f32::MIN, f32::max)
            - heights.iter().copied().fold(f32::MAX, f32::min);
        println!(
            "the tail's hair ends {:.3} m below the hocks; the head nods {range:.4} m",
            hocks - lowest
        );
        assert!(
            // Henry's tail hangs up to 17 cm below the hocks (the old horse's,
            // up to 15; user, 2026-11-18: "Ajustarlas a henry").
            (hocks - 0.19..hocks + 0.05).contains(&lowest),
            "the tail's hair ends at {lowest} m, the hocks at {hocks} m"
        );
        // Henry's walk nods the head 2.8 cm, much less than a real horse's
        // (the rule was 5.7-10.5 cm; user, 2026-11-18: "Ajustarlas a henry").
        assert!((0.025..0.105).contains(&range), "the head nods {range} m");
    }

    /// Escribe frames con skinning en CPU en `PAMPA_PREVIEW` (una carpeta) para
    /// el renderizador de vistas previas offline: un caballo montado parado (y
    /// asentado), caminando y a lo largo de un ciclo de galope, frenando del
    /// galope hasta parar y de una caminata hasta parar, y un caballo atado. Se
    /// ejecuta a mano:
    /// `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview`.
    #[test]
    #[ignore = "writes preview files"]
    fn preview_saddled_horses() {
        let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let dump = |app: &mut App, me: Entity, name: String| {
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            cpu::write(&dir.join(format!("{name}.txt")), &drawn);
        };
        // Ritmos constantes: frames de 20 ms, y se escribe uno de cada dos.
        for (name, speed, frames) in [("walk", 1.5, 24), ("gallop", 12.2, 16), ("trot", 4.5, 8)] {
            let (mut app, me) = saddled_every(rider, speed, 20);
            for _ in 0..60 {
                ride(&mut app, me, speed);
            }
            for k in 0..frames {
                ride(&mut app, me, speed);
                ride(&mut app, me, speed);
                dump(&mut app, me, format!("{name}_{k:02}"));
            }
        }
        // Frenando tan fuerte como lo hace la lógica (8 m/s²): cada 0.1 s.
        for (name, from) in [("slowdown", 12.2f32), ("walkstop", 1.5)] {
            let (mut app, me) = saddled_every(rider, from, 20);
            for _ in 0..60 {
                ride(&mut app, me, from);
            }
            let mut speed = from;
            for k in 0..24 {
                for _ in 0..5 {
                    speed = (speed - 8.0 * 0.02).max(0.0);
                    ride(&mut app, me, speed);
                }
                dump(&mut app, me, format!("{name}_{k:02}"));
            }
        }
        // Un salto a galope tendido, tal como lo vuela la lógica (alrededor de
        // 1.1 s): cada 0.1 s a lo largo del vuelo, y después el aterrizaje.
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let frames = 55;
        for f in 0..=frames {
            fly(&mut app, me, Some(f as f32 / frames as f32));
            if f % 5 == 0 {
                dump(&mut app, me, format!("jump_{:02}", f / 5));
            }
        }
        for k in 0..6 {
            for _ in 0..5 {
                ride(&mut app, me, 12.2);
            }
            dump(&mut app, me, format!("landing_{k:02}"));
        }
        // Parado: enseguida, y asentado después de unos segundos.
        let (mut app, me) = saddled_every(rider, 0.0, 20);
        dump(&mut app, me, "idle_00".into());
        for k in 1..4 {
            for _ in 0..100 {
                ride(&mut app, me, 0.0);
            }
            dump(&mut app, me, format!("idle_{k:02}"));
        }
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Overo), 0.0);
        dump(&mut app, me, "tied_00".into());
    }

    pub(crate) fn named(world: &World, under: Entity, name: &str) -> Entity {
        try_named(world, under, name).unwrap_or_else(|| panic!("no {name}"))
    }

    /// [`named`], or nothing on a frame when a scene being created again
    /// (parallel runs) has not spawned it yet.
    pub(crate) fn try_named(world: &World, under: Entity, name: &str) -> Option<Entity> {
        let children = |e: Entity| world.get::<Children>(e).into_iter().flatten().copied();
        let mut stack: Vec<Entity> = children(under).collect();
        while let Some(e) = stack.pop() {
            if world.get::<Name>(e).is_some_and(|n| n.as_str() == name) {
                return Some(e);
            }
            stack.extend(children(e));
        }
        None
    }

    /// Qué tan rápido se deslizan hacia atrás en el marco del cuerpo del
    /// caballo los vasos apoyados de cada frame (puntas a menos de 1.5 cm de la
    /// más baja), en m/s, a lo largo de `frames` frames de `dt` segundos: su
    /// mediana.
    fn hoof_slide(app: &mut App, me: Entity, frames: usize, dt: f32) -> f32 {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let mut tracks = Vec::new();
        for _ in 0..frames {
            app.update();
            let world = app.world();
            // Ubicados de nuevo: una escena creada otra vez tiene huesos
            // nuevos.
            let toes = HorseJoint::TOES.map(|t| named(world, carrier, t.name()));
            let body = world
                .get::<GlobalTransform>(carrier)
                .unwrap()
                .affine()
                .inverse();
            tracks.push(toes.map(|t| {
                body.transform_point3(world.get::<GlobalTransform>(t).unwrap().translation())
            }));
        }
        // El suelo propio de cada vaso: lo más bajo que llega.
        let ground: [f32; 4] =
            std::array::from_fn(|k| tracks.iter().map(|t| t[k].y).fold(f32::MAX, f32::min));
        let mut slides: Vec<f32> = tracks
            .windows(2)
            .flat_map(|w| {
                (0..4)
                    .filter(move |&k| {
                        w[0][k].y < ground[k] + 0.015 && w[1][k].y < ground[k] + 0.015
                    })
                    .map(move |k| -(w[1][k].x - w[0][k].x) / dt)
            })
            .collect();
        assert!(slides.len() > 10, "{slides:?}");
        slides.sort_by(f32::total_cmp);
        slides[slides.len() / 2]
    }

    /// Vuelve a empezar los clips de los caballos de `app` desde sus puntos con
    /// semilla, sobre clips ya mezclados: cuánto tardaron en cargar sus
    /// archivos (y por lo tanto cuántos frames se reprodujeron mientras
    /// [`saddled_every`] esperaba) ya no decide en qué punto de su ciclo
    /// empieza una medición.
    fn from_the_seeded_start(app: &mut App) {
        let world = app.world_mut();
        let nodes = world
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .expect("the horse's model")
            .nodes
            .clone();
        let rigs: Vec<(Entity, u32, [f32; LOADED_COUNT])> = world
            .query::<(&ChildOf, &HorseRig)>()
            .iter(world)
            .map(|(child_of, rig)| {
                let body = world.get::<HorseBody>(child_of.parent()).unwrap();
                (rig.player, body.seed, rig.durations)
            })
            .collect();
        assert!(!rigs.is_empty());
        for (player, seed, durations) in rigs {
            let mut player = world.get_mut::<AnimationPlayer>(player).unwrap();
            for clip in Clip::ALL {
                let active = player.animation_mut(nodes[clip.index()]).unwrap();
                active.seek_to(start_offset(seed, clip) * durations[clip.index()]);
            }
        }
    }

    /// Cuántos trancos por segundo da el galope de un caballo ensillado de
    /// `app` en `frames` frames de `dt` s y cuánto se deslizan por tranco los
    /// vasos apoyados (cada punta de vaso a menos de 3 cm de lo más bajo que
    /// llega) respecto del suelo, que pasa hacia atrás a `speed`, por pata.
    fn gallop_strides(
        app: &mut App,
        me: Entity,
        speed: f32,
        frames: usize,
        dt: f32,
    ) -> (f32, [f32; 4]) {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let node = app
            .world()
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .unwrap()
            .nodes[Clip::Gallop.index()];
        let phase = |app: &mut App| {
            let (player, _) = rig_of(app);
            let world = app.world_mut();
            let duration = world
                .query::<&HorseRig>()
                .iter(world)
                .find(|r| r.started)
                .unwrap()
                .durations[Clip::Gallop.index()];
            let active = world.get::<AnimationPlayer>(player).unwrap();
            active.animation(node).unwrap().seek_time() / duration
        };
        let (mut tracks, mut strides) = (Vec::new(), 0.0);
        let mut was = phase(app);
        for _ in 0..frames {
            ride(app, me, speed);
            let now = phase(app);
            strides += (now - was).rem_euclid(1.0);
            was = now;
            tracks.push(HorseJoint::TOES.map(|t| in_body(app, carrier, t.name())));
        }
        let ground: [f32; 4] =
            std::array::from_fn(|k| tracks.iter().map(|t| t[k].y).fold(f32::MAX, f32::min));
        let mut slid = [0.0; 4];
        for w in tracks.windows(2) {
            for k in 0..4 {
                if w[0][k].y < ground[k] + 0.03 && w[1][k].y < ground[k] + 0.03 {
                    slid[k] += (w[1][k].x - w[0][k].x + speed * dt).abs();
                }
            }
        }
        (strides / (frames as f32 * dt), slid.map(|s| s / strides))
    }

    #[test]
    fn the_gallop_lengthens_its_stride_to_about_two_a_second_and_its_planted_hooves_keep_the_ground()
     {
        // Un caballo galopa más rápido alargando el tranco, a unos 2.0-2.1
        // trancos por segundo a galope tendido (Parsons et al. 2008): el
        // galope del clip va al ritmo de su vaso apoyado mientras apoya y
        // más despacio en la suspensión.
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        // Medido de 23 a 49 km/h: entre todas 13 a 15 cm por tranco, ninguna
        // pata más de 7 (a ritmo parejo, 35 y 14 a 49 km/h).
        const SLIDE_TOTAL: f32 = 0.17;
        const SLIDE_LEG: f32 = 0.08;
        let (mut app, me) = saddled_every(rider, 13.61, 10);
        // 49, 35 y 23 km/h, y el turbo de desarrollo (70 km/h).
        for (speed, per_second) in [
            (13.61f32, (2.0, 2.1)),
            (9.72, (1.65, 1.85)),
            (6.4, (1.3, 1.5)),
            (19.4, (2.2, 2.45)),
        ] {
            for _ in 0..100 {
                ride(&mut app, me, speed);
            }
            let (rate, slid) = gallop_strides(&mut app, me, speed, 400, 0.01);
            println!(
                "{speed} m/s: {rate:.2} strides/s, {:.2} m each; slide per stride (cm) {:?}",
                speed / rate,
                slid.map(|s| (s * 1000.0).round() / 10.0)
            );
            assert!(
                rate > per_second.0 && rate < per_second.1,
                "{speed} m/s: {rate} strides/s"
            );
            // El turbo de desarrollo solo no se apura: a ese ritmo un frame
            // salta más del ciclo y los vasos se deslizan algo más.
            if speed > 13.61 {
                continue;
            }
            // La pata derecha se desliza unos 7 cm por tranco en el propio
            // clip (apoya junto con la izquierda, que es la que marca el
            // ritmo); las demás, pocos centímetros.
            let total: f32 = slid.iter().sum();
            assert!(total < SLIDE_TOTAL, "{speed} m/s: {slid:?}");
            for (leg, s) in slid.into_iter().enumerate() {
                assert!(
                    s < SLIDE_LEG,
                    "{speed} m/s: leg {leg} slides {s} m per stride"
                );
            }
        }
    }

    /// El clip que domina un caballo ensillado.
    fn leading(app: &mut App) -> Clip {
        let (_, w) = rig_of(app);
        [Clip::Gallop, Clip::Walk]
            .into_iter()
            .max_by(|a, b| w[a.index()].total_cmp(&w[b.index()]))
            .unwrap()
    }

    #[test]
    fn slowing_from_a_gallop_to_a_stop_never_gallops_in_slow_motion_and_the_hooves_keep_the_ground()
    {
        // Un galope no baja de unos 1.45 trancos por segundo (los caballos
        // galopan a unos 2 por segundo y alargan el tranco para ir más rápido;
        // Parsons et al. 2008, Witte et al. 2006): por debajo, el caballo
        // camina, a lo sumo al ritmo de la caminata más estirada
        // ([`STRETCH_TOP`]).
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 10);
        let min_gallop = 1.45 * 0.583;
        let max_walk = gait_rate(Clip::Walk, STRETCH_TOP);
        // Frenando como frena la lógica sin acelerar (3 m/s²), frame a frame.
        let mut speed = 13.6f32;
        while speed > 0.5 {
            ride(&mut app, me, speed);
            let clip = leading(&mut app);
            let rate = gait_rate(clip, speed);
            match clip {
                Clip::Gallop => {
                    assert!(rate >= min_gallop - 1e-4, "{speed} m/s: a gallop at {rate}")
                }
                _ => assert!(rate <= max_walk + 2e-3, "{speed} m/s: a walk at {rate}"),
            }
            speed -= 3.0 * 0.01;
        }
        // Y a cada velocidad sostenida, los vasos apoyados van al ritmo del
        // suelo (6 %), salvo en la franja del trote (sin clip propio todavía),
        // donde la caminata va a su ritmo más rápido.
        for speed in [12.2f32, 9.0, 7.5, 6.8, 6.0, 5.0, 4.0, 3.0, 2.0, 1.2] {
            for _ in 0..80 {
                ride(&mut app, me, speed);
            }
            let clip = leading(&mut app);
            let back = hoof_slide(&mut app, me, 300, 0.01);
            println!(
                "{speed} m/s: {clip:?} at {:.2}, hooves back at {back:.2} m/s",
                gait_rate(clip, speed)
            );
            if speed <= 3.0 || clip == Clip::Gallop {
                assert!(
                    (back / speed - 1.0).abs() < 0.06,
                    "{speed} m/s: hooves at {back}"
                );
            } else {
                assert!((back / STRETCH_TOP - 1.0).abs() < 0.06, "{speed}: {back}");
            }
        }
    }

    /// La piel del cuerpo (`Main`) de un caballo en el marco de su cuerpo.
    fn skin_in_body(app: &mut App, me: Entity) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let to_body = app
            .world()
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let drawn = cpu::drawn(app.world_mut(), Some(me));
        let main = drawn.into_iter().find(|d| d.name == "Horse").unwrap();
        let p = main
            .positions
            .iter()
            .map(|&p| to_body.transform_point3(p))
            .collect();
        (p, main.triangles)
    }

    /// La distancia de `p` al segmento de `a` a `b`, y dónde cae a lo largo
    /// de él (`0..=1`).
    fn off_segment(p: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
        (p.distance(a + ab * t), t)
    }

    #[test]
    fn the_hind_thighs_keep_their_skin_through_the_gallop() {
        // La piel de las patas traseras de un caballo parado: el muslo, la
        // babilla y el gaskin (detrás de la cinchera, entre 0.45 y 1.2 m), y
        // la del gaskin alrededor de cada tibia (de la babilla al corvejón).
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 0.0, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let (rest, tris) = skin_in_body(&mut app, me);
        let tibia = |app: &App, carrier: Entity, s: usize| {
            [HorseJoint::Lower(s), HorseJoint::Cannon(s)].map(|j| in_body(app, carrier, j.name()))
        };
        let gaskins: Vec<(usize, Vec<usize>, f32)> = [2, 3]
            .into_iter()
            .map(|s| {
                let [a, b] = tibia(&app, carrier, s);
                let near: Vec<usize> = (0..rest.len())
                    .filter(|&i| {
                        let (d, t) = off_segment(rest[i], a, b);
                        (0.04..0.16).contains(&d) && (0.1..0.9).contains(&t)
                    })
                    .collect();
                let girth = near
                    .iter()
                    .map(|&i| off_segment(rest[i], a, b).0)
                    .sum::<f32>()
                    / near.len() as f32;
                (s, near, girth)
            })
            .collect();
        let area = |p: &[Vec3], t: &[u32; 3]| {
            let [a, b, c] = t.map(|i| p[i as usize]);
            (b - a).cross(c - a).length() / 2.0
        };
        let thigh: Vec<&[u32; 3]> = tris
            .iter()
            .filter(|t| {
                let c = t.iter().map(|&i| rest[i as usize]).sum::<Vec3>() / 3.0;
                c.x < -0.35
                    && (0.45..1.2).contains(&c.y)
                    && c.z.abs() > 0.08
                    && area(&rest, t) > 1e-6
            })
            .collect();
        assert!(thigh.len() > 200 && gaskins.iter().all(|g| g.1.len() > 50));
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let (mut thinnest, mut crushed) = (f32::MAX, 0usize);
        for _ in 0..40 {
            ride(&mut app, me, 12.2);
            let (posed, _) = skin_in_body(&mut app, me);
            for (s, near, girth) in &gaskins {
                let [a, b] = tibia(&app, carrier, *s);
                let now = near
                    .iter()
                    .map(|&i| off_segment(posed[i], a, b).0)
                    .sum::<f32>()
                    / near.len() as f32;
                thinnest = thinnest.min(now / girth);
            }
            let here = thigh
                .iter()
                .filter(|t| area(&posed, t) < 0.35 * area(&rest, t))
                .count();
            crushed = crushed.max(here);
        }
        println!(
            "thigh: {} triangles, at most {crushed} crushed; gaskin girth at least x{thinnest:.3}",
            thigh.len()
        );
        // El gaskin conserva su grosor (a lo sumo 5 % menos alrededor de la
        // tibia; con los pesos de calor solos se afinaba un 8 %), y en un mismo
        // frame a lo sumo el 2.5 % de los triángulos del muslo se aplasta a
        // menos de un tercio de su área (los pliegues de la babilla y del
        // corvejón; eran el 3.2 %). Henry's own skinning (user, 2026-11-18:
        // "Ajustarlas a henry"): the gaskin thins to x0.899-0.924 and 2.6-4.7 %
        // of the thigh is crushed, so x0.88 and one in 18 (5.6 %).
        assert!(thinnest > 0.88, "the gaskin thins to x{thinnest}");
        assert!(crushed * 18 <= thigh.len(), "{crushed} of {}", thigh.len());
    }

    /// Las tarjetas de la crin (en reposo, sobre el cuello, entre la cruz y
    /// la nuca) y lo lejos que quedan, en promedio, de la piel del cuerpo a lo
    /// largo de `frames` cuadros a `speed`.
    fn mane_off_the_neck(speed: f32, frames: usize) -> f32 {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Alazan), 0.0, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let to_body = |app: &App| {
            app.world()
                .get::<GlobalTransform>(carrier)
                .unwrap()
                .affine()
                .inverse()
        };
        let hair = |app: &mut App| {
            let body = to_body(app);
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            let pick = |name: &str| {
                drawn
                    .iter()
                    .find(|d| d.name == name)
                    .unwrap()
                    .positions
                    .iter()
                    .map(|&p| body.transform_point3(p))
                    .collect::<Vec<Vec3>>()
            };
            (pick("Hair"), pick("Horse"))
        };
        let (rest, _) = hair(&mut app);
        let mane: Vec<usize> = (0..rest.len())
            .filter(|&i| (0.42..0.98).contains(&rest[i].x) && rest[i].y > 1.3)
            .collect();
        // Henry's mane cards there: 908 to over 1,000 vertices, by the idle's
        // frame.
        assert!(mane.len() > 600, "{}", mane.len());
        for _ in 0..60 {
            ride(&mut app, me, speed);
        }
        let mut sum = 0.0;
        for _ in 0..frames {
            ride(&mut app, me, speed);
            let (hair, skin) = hair(&mut app);
            sum += mane
                .iter()
                .map(|&i| {
                    skin.iter()
                        .map(|p| p.distance_squared(hair[i]))
                        .fold(f32::MAX, f32::min)
                        .sqrt()
                })
                .sum::<f32>()
                / mane.len() as f32;
        }
        sum / frames as f32
    }

    #[test]
    fn at_the_gallop_the_mane_flies_off_the_neck() {
        // Al galope la crin vuela en mechones apartados del cuello, no
        // pegada a él como una lámina (L55, L58): sus vertices quedan en
        // promedio a más de 4.8 cm del vertex más cercano de la piel, y un
        // cuarto más lejos que al paso (eran 3.7 cm al galope y 3.3 al paso;
        // ahora 5.5 y 4.0). Henry's mane, from his own clip, over a second:
        // 4.3-4.5 cm at the gallop, 2.6-3.1 at the walk (user, 2026-11-18:
        // "Ajustarlas a henry"; the bound was 4.8 cm).
        // A second (100 frames of 10 ms) each: two gallop strides, more than
        // a walk stride; 20 frames caught only part of a stride, and the
        // mean moved 3.4-5.4 cm with it.
        let walk = mane_off_the_neck(WALK_SPEED, 100);
        let gallop = mane_off_the_neck(12.2, 100);
        println!("mane off the neck: walking {walk:.3} m, galloping {gallop:.3} m");
        assert!(gallop > 0.04, "{gallop}");
        assert!(gallop > 1.25 * walk, "walking {walk}, galloping {gallop}");
    }

    #[test]
    fn the_gallop_engages_the_hinds_under_the_belly_folds_the_knees_and_reaches_far() {
        // Los videos del usuario (L42–L43): las traseras se apoyan muy por
        // debajo de la panza y se estiran del todo hacia atrás al impulsar;
        // en el vuelo la rodilla delantera se pliega con el antebrazo casi
        // horizontal; la mano que guía se estira casi recta muy adelante.
        // Y nunca "dislocada" (L55): la caña de una mano en el aire cuelga
        // hacia abajo, nunca plegada plana hacia atrás bajo el antebrazo, y
        // la rodilla de una mano apoyada no se dobla bajo el peso.
        // Con el galope de henry (usuario, 2026-11-14: "ajustamos los test")
        // la caña en el aire se pliega hasta 40° por encima de la horizontal
        // y la rodilla apoyada llega a 124.2°: el piso de la caña es -45° y el
        // de la rodilla apoyada 120°.
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let (mut under, mut back, mut reach) = (f32::MIN, f32::MAX, f32::MIN);
        let (mut knee, mut forearm, mut cannon, mut standing) =
            (f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        for _ in 0..70 {
            ride(&mut app, me, 12.2);
            for s in ["L", "R"] {
                let at = |j: fn(usize) -> HorseJoint, fore: bool| {
                    let leg = usize::from(s == "R") + if fore { 0 } else { 2 };
                    in_body(&app, carrier, j(leg).name())
                };
                // Adelante de la articulación de la cadera (+X), y el ángulo de
                // la pata por delante de la vertical.
                let (hip, fetlock) = (at(HorseJoint::Upper, false), at(HorseJoint::Pastern, false));
                under = under.max(fetlock.x - hip.x);
                back = back.min((fetlock.x - hip.x).atan2(hip.y - fetlock.y).to_degrees());
                let [elbow, k, fore] = [HorseJoint::Lower, HorseJoint::Cannon, HorseJoint::Pastern]
                    .map(|j| at(j, true));
                reach = reach.max(fore.x - elbow.x);
                let bend = (elbow - k).angle_between(fore - k).to_degrees();
                knee = knee.min(bend);
                let d = k - elbow;
                forearm = forearm.min((-d.y).atan2(d.x.abs()).to_degrees());
                // Cuánto baja la caña desde la rodilla, hacia donde apunte.
                let c = fore - k;
                cannon = cannon.min((-c.y).atan2(c.x.abs()).to_degrees());
                // Apoyada: el menudillo a su altura de parado (0.164 m).
                if fore.y < 0.18 {
                    standing = standing.min(bend);
                }
            }
        }
        println!(
            "hind fetlock up to {under:.2} m ahead of the hip, back to {back:.1}°; knee folds to \
             {knee:.1}°, forearm {forearm:.1}° below horizontal, cannon at least {cannon:.1}° \
             below; standing knee at least {standing:.1}°; fore fetlock {reach:.2} m ahead"
        );
        // Antes de los videos: 0.38 m, -25°, 58°, 24° y 0.43 m; la primera
        // vuelta contra ellos plegaba la caña a 8° de la horizontal y doblaba
        // la rodilla apoyada a 110°.
        assert!(under > 0.5, "the hinds reach {under} m under the belly");
        assert!(back < -27.0, "the hinds push back to {back}°");
        assert!(knee < 75.0, "the fore knee folds to {knee}°");
        assert!(
            forearm < 18.0,
            "the forearm rises to {forearm}° below horizontal"
        );
        assert!(
            cannon > -45.0,
            "a swinging cannon folds back to {cannon}° below horizontal"
        );
        assert!(
            standing > 120.0,
            "a standing fore knee bends to {standing}°"
        );
        assert!(reach > 0.5, "the lead fore reaches {reach} m");
    }

    /// El clip `Gallop` de `horse.glb` tal como está grabado, sin nada de lo
    /// que el juego le suma: cinemática directa de los nodos del archivo en
    /// un instante de su ciclo.
    struct RawGallop {
        parents: Vec<Option<usize>>,
        rest: Vec<(Vec3, Quat, Vec3)>,
        names: Vec<String>,
        /// Por nodo, las claves de su rotación y de su traslación.
        rotations: Vec<Option<(Vec<f32>, Vec<Quat>)>>,
        translations: Vec<Option<(Vec<f32>, Vec<Vec3>)>>,
        duration: f32,
    }

    impl RawGallop {
        fn load() -> Self {
            use crate::fauna::livestock::Species;
            use crate::gltf::tests::root;
            use bevy::gltf::gltf::{self, animation::util::ReadOutputs};
            let data = std::fs::read(root().0.join(Species::Horse.path())).unwrap();
            let file = gltf::Gltf::from_slice(&data).unwrap();
            let blob = file.blob.as_deref();
            let n = file.nodes().count();
            let mut parents = vec![None; n];
            let (mut rest, mut names) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for node in file.nodes() {
                for c in node.children() {
                    parents[c.index()] = Some(node.index());
                }
                let (t, r, s) = node.transform().decomposed();
                rest.push((Vec3::from(t), Quat::from_array(r), Vec3::from(s)));
                names.push(node.name().unwrap_or_default().to_string());
            }
            let (mut rotations, mut translations) = (vec![None; n], vec![None; n]);
            let mut duration = 0.0f32;
            let gallop = file
                .animations()
                .find(|a| a.name() == Some(Clip::Gallop.name()))
                .unwrap();
            for channel in gallop.channels() {
                let reader = channel.reader(|_| blob);
                let times: Vec<f32> = reader.read_inputs().unwrap().collect();
                duration = duration.max(*times.last().unwrap());
                let k = channel.target().node().index();
                match reader.read_outputs().unwrap() {
                    ReadOutputs::Rotations(r) => {
                        let q = r.into_f32().map(Quat::from_array).collect();
                        rotations[k] = Some((times, q));
                    }
                    ReadOutputs::Translations(t) => {
                        translations[k] = Some((times, t.map(Vec3::from).collect()));
                    }
                    _ => {}
                }
            }
            Self {
                parents,
                rest,
                names,
                rotations,
                translations,
                duration,
            }
        }

        /// Dónde están `names` en la fase `phase` (`0..1`) del ciclo.
        fn at<const N: usize>(&self, phase: f32, names: [&str; N]) -> [Vec3; N] {
            let t = phase.rem_euclid(1.0) * self.duration;
            fn key<T: Copy>(keys: &(Vec<f32>, Vec<T>), t: f32, mix: impl Fn(T, T, f32) -> T) -> T {
                let (times, values) = keys;
                let i = times.partition_point(|&x| x <= t).clamp(1, times.len() - 1);
                let s = ((t - times[i - 1]) / (times[i] - times[i - 1]).max(1e-6)).clamp(0.0, 1.0);
                mix(values[i - 1], values[i], s)
            }
            let local = |k: usize| {
                let (mut tr, mut ro, sc) = self.rest[k];
                if let Some(keys) = &self.rotations[k] {
                    ro = key(keys, t, |a, b, s| a.slerp(b, s));
                }
                if let Some(keys) = &self.translations[k] {
                    tr = key(keys, t, |a, b, s| a.lerp(b, s));
                }
                Mat4::from_scale_rotation_translation(sc, ro, tr)
            };
            names.map(|name| {
                let mut k = self.names.iter().position(|n| n == name).unwrap();
                let mut m = local(k);
                while let Some(p) = self.parents[k] {
                    m = local(p) * m;
                    k = p;
                }
                m.transform_point3(Vec3::ZERO)
            })
        }
    }

    #[test]
    fn the_gallop_loop_comes_back_to_its_first_pose_so_no_hoof_pops_at_the_seam() {
        // El último cuadro del ciclo es el primero: un vaso apoyado no salta
        // al volver a empezar (el original termina un cuadro antes y la mano
        // de galope saltaba 21 cm en cada tranco).
        let raw = RawGallop::load();
        let toes = HorseJoint::TOES.map(HorseJoint::name);
        let first = raw.at(0.0, toes);
        let last = raw.at(1.0 - 1e-6, toes);
        for (leg, (a, b)) in first.iter().zip(last).enumerate() {
            let jump = a.distance(b) * Species::Horse.scale();
            assert!(jump < 0.005, "leg {leg} jumps {jump} m at the seam");
        }
        assert!((raw.duration - 1.0).abs() < 1e-4, "{}", raw.duration);
    }

    /// La rodilla de una mano (entre el antebrazo y la caña) y su menudillo
    /// (entre la caña y la cuartilla, hasta la punta del vaso), en grados, a
    /// partir de [codo, rodilla, menudillo, punta del vaso].
    fn fore_joints([elbow, knee, fetlock, toe]: [Vec3; 4]) -> (f32, f32) {
        (
            (elbow - knee).angle_between(fetlock - knee).to_degrees(),
            (knee - fetlock).angle_between(toe - fetlock).to_degrees(),
        )
    }

    #[test]
    fn in_game_the_galloping_fore_knees_and_fetlocks_bend_as_the_clip_bends_them() {
        // L55: en el juego las manos parecían "dislocadas" en el galope.
        // Lo que el juego suma al clip (la flexión del cuerpo, el estiramiento,
        // el plegado de las patas que bajan del suelo, los vasos devueltos al
        // extremo de su pata) no debe doblar la rodilla ni el menudillo de una
        // mano más allá de como los dobla el propio clip en ese punto de su
        // ciclo.
        let raw = RawGallop::load();
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let node = app
            .world()
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .unwrap()
            .nodes[Clip::Gallop.index()];
        let mut worst = (0.0f32, String::new());
        for _ in 0..70 {
            ride(&mut app, me, 12.2);
            let (player, _) = rig_of(&mut app);
            let world = app.world_mut();
            let duration = world
                .query::<&HorseRig>()
                .iter(world)
                .next()
                .unwrap()
                .durations[Clip::Gallop.index()];
            let phase = world
                .get::<AnimationPlayer>(player)
                .unwrap()
                .animation(node)
                .unwrap()
                .seek_time()
                / duration;
            for (leg, s) in ["L", "R"].into_iter().enumerate() {
                let names = [
                    HorseJoint::Lower(leg),
                    HorseJoint::Cannon(leg),
                    HorseJoint::Pastern(leg),
                    HorseJoint::Toe(leg),
                ]
                .map(HorseJoint::name);
                let game = fore_joints(names.map(|n| in_body(&app, carrier, n)));
                let clip = fore_joints(raw.at(phase, names));
                for (what, g, c) in [("knee", game.0, clip.0), ("fetlock", game.1, clip.1)] {
                    let off = (g - c).abs();
                    if off > worst.0 {
                        worst = (
                            off,
                            format!("{s} {what} at {phase:.3}: game {g:.1}°, clip {c:.1}°"),
                        );
                    }
                }
            }
        }
        println!("worst: {:.1}° ({})", worst.0, worst.1);
        assert!(worst.0 < 8.0, "{}", worst.1);
    }

    /// Cuántos grados por delante de la vertical va la cara de un caballo (la
    /// línea de la frente al hocico; negativo: metida hacia el pecho).
    fn face(app: &App, carrier: Entity) -> f32 {
        let f = in_body(app, carrier, HorseJoint::UpperLip.name())
            - in_body(app, carrier, HorseJoint::Poll.name());
        f.x.atan2(-f.y).to_degrees()
    }

    /// El ángulo de la rodilla de cada mano (del codo al menudillo), en
    /// grados.
    fn knee_angles(app: &App, carrier: Entity) -> [f32; 2] {
        std::array::from_fn(|leg| {
            let [a, b, c] = [
                HorseJoint::Lower(leg),
                HorseJoint::Cannon(leg),
                HorseJoint::Pastern(leg),
            ]
            .map(|j| in_body(app, carrier, j.name()));
            (a - b).angle_between(c - b).to_degrees()
        })
    }

    /// Cuánto se dobla la rodilla de cada mano respecto de parado, en grados.
    fn knees(app: &App, carrier: Entity, rest: &[f32; 2]) -> [f32; 2] {
        let now = knee_angles(app, carrier);
        std::array::from_fn(|k| (rest[k] - now[k]).abs())
    }

    #[test]
    fn a_standing_horse_holds_its_face_in_front_of_the_vertical() {
        // Un caballo parado lleva la cara por delante de la vertical, nunca
        // metida hacia el pecho. Salvo con la cabeza baja: el `Idle_3` de
        // henry, que escarba, la mete 27° detrás de la vertical, y el usuario
        // eligió los clips de henry tal cual (2026-11-14).
        through_the_idles_and_a_walk(true);
    }

    #[test]
    fn a_standing_horse_stands_on_all_four_hooves_and_never_holds_a_fore_folded() {
        // Parado descansa una pata trasera sobre la pinza o mueve un vaso unos
        // centímetros; afloja una rodilla hasta 25° (`Idle_2`). Solo con la
        // cabeza baja (`Idle_3` de henry) escarba con una mano, así que ahí se
        // revisan las traseras nada más.
        through_the_idles_and_a_walk(false);
    }

    /// Un caballo atado a lo largo de unas diez esperas, y uno montado al paso
    /// que se para: sus vasos apoyados y sus rodillas rectas en cada espera, y
    /// la cara por delante de la vertical si `face_too`.
    fn through_the_idles_and_a_walk(face_too: bool) {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Zaino), 0.0, 50);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let toes = HorseJoint::TOES.map(HorseJoint::name);
        // Una punta de vaso apoyada, en el marco del cuerpo.
        let down = TOE_STANDING * Species::Horse.scale() + 0.03;
        let straight = knee_angles(&app, carrier);
        let (mut seen, mut last, mut settled) = (std::collections::HashSet::new(), None, 0);
        // Unas diez esperas de 14 s.
        for frame in 0..3000 {
            ride(&mut app, me, 0.0);
            let (_, w) = rig_of(&mut app);
            let idle = TIED_IDLES
                .into_iter()
                .max_by(|a, b| w[a.index()].total_cmp(&w[b.index()]))
                .unwrap();
            seen.insert(idle);
            settled = if last == Some(idle) { settled + 1 } else { 0 };
            last = Some(idle);
            let angle = face(&app, carrier);
            assert!(
                !face_too || idle == Clip::IdleHeadlow || angle > 0.0,
                "frame {frame}, {idle:?}: the face {angle}° from the vertical"
            );
            if settled > 20 && w[idle.index()] > 0.98 {
                // Con la cabeza baja escarba con una mano: solo las traseras.
                let paws = idle == Clip::IdleHeadlow;
                for toe in &toes[if paws { 2 } else { 0 }..] {
                    let y = in_body(&app, carrier, toe).y;
                    assert!(y < down, "frame {frame}, {idle:?}: {toe} {y} m up");
                }
                for bend in knees(&app, carrier, &straight) {
                    assert!(
                        paws || bend < 30.0,
                        "frame {frame}, {idle:?}: a knee folded {bend}°"
                    );
                }
            }
        }
        assert_eq!(seen, TIED_IDLES.into(), "{seen:?}");
        // Al paso la cara sigue por delante, y al pararse apoya los cuatro
        // vasos.
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Zaino,
        };
        let (mut app, me) = saddled_every(rider, 1.5, 50);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 1.5);
            assert!(!face_too || face(&app, carrier) > 0.0, "walking");
        }
        for _ in 0..40 {
            ride(&mut app, me, 0.0);
        }
        for toe in toes {
            assert!(in_body(&app, carrier, toe).y < down, "stopped: {toe}");
        }
    }

    #[test]
    fn a_horse_turning_in_place_takes_short_slow_steps_and_stops_when_the_turn_does() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 0.0, 50);
        let walk = |app: &mut App| {
            let world = app.world_mut();
            let (rig, speed) = {
                let mut q = world.query::<&HorseRig>();
                let rig = q.iter(world).find(|r| r.started).unwrap();
                (rig.weights[Clip::Walk.index()], rig.player)
            };
            let species = world
                .resource::<LivestockModels>()
                .get(Species::Horse)
                .unwrap()
                .nodes[Clip::Walk.index()];
            let rate = world
                .get::<AnimationPlayer>(speed)
                .unwrap()
                .animation(species)
                .unwrap()
                .speed();
            (rig, rate)
        };
        for _ in 0..20 {
            app.update();
        }
        assert!(walk(&mut app).0 < 1e-3, "standing still stays idle");
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation: AnimationState::Idle,
            speed: 0.0,
            turn: 1.8,
            ..default()
        });
        for _ in 0..20 {
            app.update();
        }
        let (weight, rate) = walk(&mut app);
        assert!(weight > 0.2 && weight < 0.95, "{weight}");
        // Por encima del ritmo mínimo de la caminata, para que los pasos no se
        // arrastren.
        assert!(rate > WALK_RATE_FLOOR + 0.05 && rate <= 0.5, "{rate}");
        app.world_mut().entity_mut(me).insert(VisualPose::default());
        for _ in 0..40 {
            app.update();
        }
        assert!(walk(&mut app).0 < 1e-2, "back to idle");
    }

    #[test]
    fn a_backing_horse_steps_back_its_hooves_sliding_forward_as_fast_as_ahead() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let speed = 1.5;
        let (mut app, me) = saddled_every(rider, speed, 10);
        from_the_seeded_start(&mut app);
        // A lo largo de un ciclo de caminata y algo más hacia cada lado
        // (1.17 s).
        let ahead = hoof_slide(&mut app, me, 150, 0.01);
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation: AnimationState::Walk,
            speed: -speed,
            ..default()
        });
        let back = hoof_slide(&mut app, me, 150, 0.01);
        println!("{speed} m/s: hooves back at {ahead:.2} m/s ahead, {back:.2} m/s backing");
        assert!(ahead > 0.5, "{ahead}");
        assert!(
            (back / -ahead - 1.0).abs() < 0.2,
            "backing: {back} m/s, ahead {ahead} m/s"
        );
    }

    /// Los huesos de las patas y del lomo cuyo giro vigila
    /// [`slowing_down_from_a_gallop_or_a_walk_never_folds_a_leg_or_sinks_a_hoof`].
    fn leg_bones() -> Vec<HorseJoint> {
        (0..4)
            .flat_map(|l| {
                [
                    HorseJoint::Upper(l),
                    HorseJoint::Lower(l),
                    HorseJoint::Cannon(l),
                ]
            })
            .chain([HorseJoint::Croup, HorseJoint::Withers])
            .collect()
    }

    /// Por cada frame del caballo de `app` bajo `me`: la punta de vaso más
    /// baja (marco del cuerpo) y el giro de cada hueso de [`leg_bones`]
    /// respecto de `rest`.
    fn legs_now(app: &App, carrier: Entity, rest: &[Quat]) -> (f32, Vec<f32>) {
        let world = app.world();
        let low = toe_heights(app, carrier)
            .into_iter()
            .fold(f32::MAX, f32::min);
        let turns = leg_bones()
            .iter()
            .zip(rest)
            .map(|(j, r)| {
                world
                    .get::<Transform>(named(world, carrier, j.name()))
                    .unwrap()
                    .rotation
                    .angle_between(*r)
            })
            .collect();
        (low, turns)
    }

    /// Reproduce el caballo de `app` a cada velocidad de `speeds` durante un
    /// frame, con la animación de la lógica siguiendo la velocidad como lo hace
    /// la del jinete.
    pub(crate) fn ride(app: &mut App, me: Entity, speed: f32) {
        let animation = if speed > 3.0 {
            AnimationState::Gallop
        } else if speed > AnimationState::IDLE_THRESHOLD {
            AnimationState::Walk
        } else {
            AnimationState::Idle
        };
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation,
            speed,
            ..default()
        });
        app.update();
    }

    #[test]
    fn slowing_down_from_a_gallop_or_a_walk_never_folds_a_leg_or_sinks_a_hoof() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 0.0, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let bones = leg_bones();
        let rest: Vec<Quat> = bones
            .iter()
            .map(|j| {
                let e = named(app.world(), carrier, j.name());
                *app.world().get::<Transform>(e).unwrap()
            })
            .map(|t| t.rotation)
            .collect();
        // Los límites: lo más lejos que gira cada hueso a ritmos constantes.
        let mut limit = vec![0.0f32; bones.len()];
        for speed in [0.0, 1.5, 3.0, 4.5, 8.0, 12.2] {
            for frame in 0..60 {
                ride(&mut app, me, speed);
                if frame > 30 {
                    let (_, turns) = legs_now(&app, carrier, &rest);
                    for (l, t) in limit.iter_mut().zip(turns) {
                        *l = l.max(t);
                    }
                }
            }
        }
        // Frenando fuerte (8 m/s², el de la lógica) y avanzando por inercia
        // (3 m/s²) desde la velocidad máxima y desde una caminata, hasta parar.
        for (from, frames) in [(12.2f32, 76), (12.2, 200), (1.5, 10), (1.5, 25)] {
            for _ in 0..40 {
                ride(&mut app, me, from);
            }
            for k in 0..=frames + 60 {
                let speed = from * (1.0 - k as f32 / frames as f32).max(0.0);
                ride(&mut app, me, speed);
                let (low, turns) = legs_now(&app, carrier, &rest);
                // Nunca más abajo de lo que bajan los vasos los clips.
                use crate::character::bones::TOE_LOWEST;
                let floor = TOE_LOWEST * Species::Horse.scale() - 0.005;
                assert!(low > floor, "from {from} at {speed:.2} m/s: a toe at {low}");
                for ((n, t), l) in bones.iter().zip(turns).zip(&limit) {
                    assert!(
                        t <= l + 0.12,
                        "from {from} at {speed:.2} m/s: {n:?} turned {t} (steady {l})"
                    );
                }
            }
        }
    }
}
