//! Caballos criollos ensillados: el caballo criollo propio (`tools/horse.py`, el
//! modelo de los caballos de corral, ver [`livestock`](crate::fauna::livestock)) con un
//! recado de 1919 y una brida de cuero crudo (ver [`tack`](super::tack)), atado
//! o montado.
//!
//! Un caballo ensillado es una entidad de cuerpo ([`HorseBody`], girada hacia
//! el rumbo, +X su frente, su derecha +Z, los vasos en su origen) que contiene
//! el modelo ([`HorseModel`]). El modelo espera su escena ([`HorsePending`]):
//! hasta que termina de cargar no se dibuja nada (las escenas cargan con el
//! juego, mucho antes de que alguien se suba a un caballo). Cuando se crea,
//! [`dress_horse`] les da a sus meshes el pelaje y el rango compartido de los
//! accesorios, como a los caballos de corral, y ubica los huesos que mueven los
//! andares ([`gait`](super::gait)). El mismo evento de escena pone los aperos
//! ([`tack`](super::tack): el recado y la brida con skinning, los estribos y
//! las correas), y una vez puestos los aperos un jinete con modelo se sienta
//! sobre su hueso `Torso` ([`rider`](super::rider)), así que sube, cabecea y
//! gira con el propio clip del caballo.
//!
//! [`drive_horses`] reproduce los clips: la espera, después la caminata al
//! ritmo de la velocidad sobre el suelo hasta unos 4.5 m/s (no hay clip de
//! trote), y después el galope ([`gait_weights`], [`gait_rate`]), cada uno a la
//! velocidad sobre el suelo dividida por la velocidad que muestran sus vasos,
//! para que se queden quietos sobre el suelo; al retroceder, la zancada se
//! reproduce hacia atrás al mismo ritmo; al girar en el lugar, camina en el
//! sitio al ritmo corto de [`stepping_speed`]. Un caballo atado cambia de
//! espera de vez en cuando, como los caballos de corral; uno montado se queda
//! quieto. En el aire ([`VisualPose::flight`](super::VisualPose)) solo se
//! reproduce el clip de salto ([`Clip::GallopJump`], que gana peso a
//! [`JUMP_BLEND_RATE`]): nunca avanza con el reloj, sino que en cada frame se
//! fija en el [`jump_time`] del propio vuelo de la física, así que el recogido
//! y el cabeceo aterrizan con el arco; cuando termina, el galope retoma con las
//! manos recién apoyadas ([`GALLOP_LANDING`]), así que las patas traseras
//! apoyan después. El vuelo empieza en el propio impulso del clip,
//! demorándose en él ([`JUMP_LINGER`]), y el ajuste agrega el empuje de las
//! patas traseras y un recogido más profundo de las manos (ver
//! [`gait`](super::gait)). Un caballo fuera de la vista o lejos conserva su
//! última pose, pero nunca el del jugador.
//!
//! Los pelajes se pintan en los colores por vertex del modelo según dónde está
//! cada vertex en el caballo parado ([`Coat::color_at`]), así que las manchas,
//! los rodados y la raya de mula del bayo recorren el cuerpo; el pigmento se
//! sombrea además con la forma del caballo (el lomo y lo alto del cuello más
//! oscuros, el vientre y los ijares más claros, manchones suaves), y los mapas
//! del cuerpo agregan el pelo, los surcos de los músculos y el brillo.

use bevy::animation::graph::AnimationGraphHandle;
use bevy::ecs::system::SystemParam;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use pampa_core::{AnimationState, Heading, HorseGait};

use super::clips::stepping_speed;
use super::figure::{FigureKind, Rgb, body_rotation};
use super::gait::{
    Bone as GaitBone, GALLOP_MEANS, HorseBones, PARTIAL, Property, UNANIMATED, fit_horse,
};
use super::visual::VisualPose;
use crate::fauna::Animal;
use crate::fauna::livestock::{
    self, CLIP_COUNT, Clip, CoatCache, HORSE_IDLE, HORSE_WALK_SPEED, LivestockModels, ModelPart,
    Species, any_in_view, dress_meshes, frozen, mix, start_offset, view_eye,
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
    pub const HIDALGO: Self = Self::Bayo;

    /// Un pelaje elegido de forma determinista a partir de `seed`.
    pub fn seeded(seed: u32) -> Self {
        let mut h = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        Self::ALL[(h % Self::ALL.len() as u32) as usize]
    }

    fn body(self) -> Rgb {
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
    /// cuello. Tiñe el mapa neutro de las tarjetas de pelo (`horse_hair`).
    pub(crate) fn mane(self) -> Rgb {
        match self {
            Self::Zaino | Self::Bayo => [34, 28, 24],
            Self::Tordillo => [206, 202, 194],
            Self::Overo => [88, 50, 30],
            Self::Alazan => [214, 150, 92],
        }
    }

    /// La parte baja de las patas, más oscura en la mayoría de los pelajes.
    pub(crate) fn points(self) -> Rgb {
        match self {
            Self::Zaino => [44, 32, 26],
            Self::Tordillo => [92, 92, 90],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [132, 70, 38],
            Self::Bayo => [40, 32, 26],
        }
    }

    /// La cabeza, un poco más clara en el tordillo.
    pub(crate) fn head(self) -> Rgb {
        match self {
            Self::Tordillo => [150, 150, 146],
            _ => self.body(),
        }
    }

    pub(crate) fn hoof(self) -> Rgb {
        match self {
            Self::Tordillo | Self::Overo => [78, 68, 58],
            _ => [38, 32, 28],
        }
    }

    /// El color de un vertex del pelaje en `p` sobre el caballo parado, con su
    /// superficie mirando hacia `normal`.
    pub fn color_at(self, p: Vec3, normal: Vec3) -> Rgb {
        const WHITE: Rgb = [232, 228, 218];
        if p.y < POINTS_HEIGHT {
            // Medias del overo en la mano izquierda y en la pata derecha.
            let stocking = (p.x > 0.0 && p.z < 0.0) || (p.x < 0.0 && p.z > 0.0);
            if self == Self::Overo && stocking {
                return WHITE;
            }
            return self.points();
        }
        let c = match self {
            // El blanco no tiene pigmento que sombrear.
            Self::Overo if overo_patch(p) => return WHITE,
            Self::Tordillo if dapple(p) => [178, 176, 170],
            // La raya de mula del bayo, desde la cruz hasta la cola.
            Self::Bayo if normal.y > 0.6 && p.z.abs() < 0.05 && (-0.8..0.45).contains(&p.x) => {
                [104, 76, 46]
            }
            _ => self.body(),
        };
        let k = depth(p, normal);
        c.map(|v| (v as f32 * k).round().clamp(0.0, 255.0) as u8)
    }
}

/// La profundidad del pelaje en `p` (mirando hacia `normal`): un factor que
/// multiplica su color, como en un caballo de verdad, cuyo pelo es más oscuro
/// a lo largo del lomo y en lo alto del cuello y más claro en el vientre, la
/// cinchera y los ijares, con manchones suaves del tamaño de una parte del
/// cuerpo para que ninguna zona sea de un solo color plano.
fn depth(p: Vec3, normal: Vec3) -> f32 {
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    // El lomo y la grupa, mirando hacia arriba.
    let topline = smooth((normal.y - 0.35) / 0.45) * smooth((p.y - 1.15) / 0.2);
    // Lo alto del cuello, delante de la cruz.
    let crest =
        smooth((p.x - 0.45) / 0.2) * smooth((p.y - 1.35) / 0.15) * smooth((normal.y + 0.1) / 0.5);
    // El vientre y la cinchera, mirando hacia abajo, y los ijares.
    let under = smooth((-normal.y - 0.15) / 0.5) * smooth((1.3 - p.y) / 0.2);
    let flank = smooth((normal.z.abs() - 0.6) / 0.3) * smooth((1.15 - p.y) / 0.15);
    let mottle = (p.x * 3.1 + 1.3).sin() * (p.y * 2.7 + 0.4).sin() * (p.z * 3.4 + 2.2).sin()
        + 0.5 * (p.x * 7.3 - p.z * 5.9 + 0.7).sin() * (p.y * 6.1 + 1.9).cos();
    1.0 - 0.16 * topline - 0.12 * crest + 0.12 * under + 0.06 * flank + 0.06 * mottle
}

/// Por debajo de esta altura (en m) las patas muestran los cabos del pelaje.
const POINTS_HEIGHT: f32 = 0.5;

/// El blanco del overo: grandes manchas en la panza, en ambos costados, en el
/// lado derecho de la grupa, en ambas paletas y a ambos lados del cuello, así
/// que algo de blanco se ve desde cualquier lado.
fn overo_patch(p: Vec3) -> bool {
    const BLOBS: [([f32; 3], [f32; 3]); 8] = [
        ([0.0, 0.8, 0.0], [0.5, 0.2, 0.4]),
        ([-0.15, 1.08, -0.26], [0.36, 0.26, 0.16]),
        ([0.12, 1.02, 0.26], [0.38, 0.32, 0.18]),
        ([-0.56, 1.18, 0.24], [0.22, 0.22, 0.16]),
        ([0.78, 1.5, -0.12], [0.2, 0.24, 0.12]),
        ([0.52, 1.1, 0.22], [0.2, 0.26, 0.14]),
        ([0.9, 1.62, 0.1], [0.18, 0.2, 0.1]),
        ([0.45, 1.0, -0.22], [0.2, 0.26, 0.14]),
    ];
    BLOBS.iter().any(|(c, r)| {
        let d = (p - Vec3::from(*c)) / Vec3::from(*r);
        d.length_squared() < 1.0
    })
}

/// Los rodados del tordillo: un patrón de interferencia 3D barato.
fn dapple(p: Vec3) -> bool {
    let a = (p.x * 23.0 + (p.z * 17.0).sin()).sin();
    let b = (p.y * 21.0 + (p.x * 13.0).cos()).sin();
    let c = (p.z * 19.0 + p.y * 7.0).sin();
    a * b * c > 0.12
}

/// A qué distancia (en m) están los vasos de un caballo del centro de un giro
/// en el lugar: más que los pies de una persona, para que sus pasos mantengan
/// un ritmo de caminata (con la velocidad de giro del juego de 1.8 rad/s,
/// 0.54 m/s).
pub const HORSE_TURN_STEP_RADIUS: f32 = 0.3;

/// Las velocidades sobre el suelo (m/s) entre las que la espera se funde con la
/// caminata.
pub const WALK_BAND: (f32, f32) = (0.05, 0.6);

/// Las velocidades sobre el suelo (m/s) a las que un caballo que camina pasa al
/// galope, y uno que galopa vuelve a la caminata: alrededor de
/// [`HorseGait::TROT_MAX`] (6.5 m/s), donde la lógica deja de trotar y galopa,
/// y nunca por debajo de lo que el galope muestra a su ritmo mínimo
/// ([`GALLOP_RATE`]: 0.85 × 7.20 = 6.12 m/s). La lógica camina hasta 3 m/s y
/// trota entre 3 y 6.5; sin clip de trote todavía, en esa franja la caminata
/// va a su ritmo más rápido ([`RATE`]) y los vasos se deslizan hacia atrás lo
/// que le falta. El andar cambia de golpe (y los pesos se funden rápido,
/// [`HORSE_BLEND_RATE`], en una fracción de tranco, al paso de la mano de
/// galope, [`in_step`]), sin mantener nunca los dos mitad y mitad: sus patas
/// no coinciden.
pub const GALLOP_FROM: (f32, f32) = (6.35, 6.65);

/// Si un caballo que se mueve a `speed` galopa, según si `was` galopando.
pub fn gallops(was: bool, speed: f32) -> bool {
    speed > if was { GALLOP_FROM.0 } else { GALLOP_FROM.1 }
}

/// Cuánto más lento o más rápido que lo grabado puede reproducirse la caminata:
/// hasta la caminata más rápida de la lógica (3 m/s, 2.44 veces; antes se
/// estiraba hasta 4.8 m/s, casi 4 veces, y parecía frenética).
pub const RATE: (f32, f32) = (0.3, 2.44);

/// Cuánto más lento o más rápido que lo grabado puede reproducirse el galope.
/// El clip da 1.71 trancos por segundo; un caballo galopa a unos 2 por segundo
/// y va más rápido sobre todo alargando el tranco (Parsons et al. 2008: 2.0 a
/// 9.5 m/s, 2.1 a 12.5; Witte et al. 2006: 2.3 a 18), así que por debajo de
/// unos 1.45 por segundo (0.85) parece un galope en cámara lenta: ahí ya
/// camina ([`GALLOP_FROM`]). Más rápido que el doble parece frenético, así que
/// por encima de unos 14.4 m/s sus vasos se deslizan un poco.
pub const GALLOP_RATE: (f32, f32) = (0.85, 2.0);

/// La velocidad sobre el suelo que muestran los vasos del galope con su zancada
/// estirada ([`STRIDE`](super::gait::STRIDE)), en m/s, medida como la del
/// propio clip
/// ([`HORSE_GALLOP_SPEED`](crate::fauna::livestock::HORSE_GALLOP_SPEED)): la
/// mediana del deslizamiento de las puntas de los vasos apoyados en el juego,
/// dividida por el ritmo, con la flexión del cuerpo: 7.19, 7.20 y 7.21 m/s a 8,
/// 12.2 y 19.4 m/s (con el galope anterior a los videos del usuario, 7.45).
pub const GALLOP_SPEED: f32 = 7.20;

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
/// m/s: la velocidad sobre el suelo dividida por la velocidad que muestran sus
/// vasos, dentro de [`RATE`], negativa al retroceder; una espera se reproduce
/// tal como se grabó.
pub fn gait_rate(clip: Clip, speed: f32) -> f32 {
    let back = if speed < 0.0 { -1.0 } else { 1.0 };
    let speed = speed.abs();
    match clip {
        Clip::Walk => back * (speed / HORSE_WALK_SPEED).clamp(RATE.0, RATE.1),
        Clip::Gallop => back * (speed / GALLOP_SPEED).clamp(GALLOP_RATE.0, GALLOP_RATE.1),
        _ => 1.0,
    }
}

/// Las esperas entre las que alterna un caballo atado. Atado corto al poste, no
/// puede pastar (y sus riendas, sobre la cabezada, le atravesarían el cuello si
/// lo hiciera).
pub const TIED_IDLES: [Clip; 3] = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];

/// El cuerpo de un caballo ensillado, hijo de su entidad de personaje: girado
/// hacia el rumbo, contiene el modelo.
#[derive(Component, Debug, Clone)]
pub struct HorseBody {
    pub coat: Coat,
    /// Quién lo monta, si alguien lo hace.
    pub rider: Option<FigureKind>,
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
    /// El peso actual de cada clip, en el orden de [`Clip::ALL`].
    pub weights: [f32; CLIP_COUNT],
    /// Si sus clips se están reproduciendo.
    pub started: bool,
    /// Si sus huesos conservan su última pose.
    pub frozen: bool,
    /// La duración de cada clip, en segundos, una vez que terminó de cargar.
    pub durations: [f32; CLIP_COUNT],
    /// El clip de zancada que reproduce principalmente (la caminata o el
    /// galope): el otro se pone al mismo paso cuando toma el relevo.
    pub lead: Clip,
    /// Qué tan avanzado está el vuelo de un salto (`0..=1`), nada en el suelo.
    pub flight: Option<f32>,
}

impl HorseRig {
    /// El rig de un modelo, con sus clips reproducidos por `player`, visto a
    /// través de `probes`.
    pub fn new(player: Entity, probes: Vec<Entity>) -> Self {
        Self {
            player,
            probes,
            weights: [0.0; CLIP_COUNT],
            started: false,
            frozen: false,
            durations: [0.0; CLIP_COUNT],
            lead: Clip::Walk,
            flight: None,
        }
    }
}

/// Dónde está el ciclo de la caminata al mismo paso que el del galope: cada uno
/// con la mano izquierda (la de galope) apoyando (el de la caminata en
/// [`WALK_PLANT`], el del galope en [`GALLOP_PLANT`]).
pub const WALK_FROM_GALLOP: f32 = WALK_PLANT - GALLOP_PLANT;

/// En qué punto de su ciclo (`0..1`) apoya la mano izquierda, la de galope, en
/// el galope: la punta de su vaso (`FF.L_end`) apoya en 0.532 (0.556 antes de
/// que la mano se estirara más adelante, contra los videos del usuario).
pub const GALLOP_PLANT: f32 = 0.532;

/// En qué punto de su ciclo (`0..1`) apoya la mano izquierda en la caminata:
/// la punta de su vaso (`FF.L_end`) apoya en 0.333.
pub const WALK_PLANT: f32 = 0.333;

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

/// La parte del clip `Gallop_Jump` que es el vuelo, en segundos del clip
/// (medida sobre las curvas de su `Body` y de las puntas de los vasos
/// traseros): desde el impulso de las patas traseras, cuando las puntas de los
/// dos vasos traseros dejan el suelo (a más de 1.5 cm de su punto más bajo),
/// estiradas hacia atrás, hasta el `Body` de vuelta a su altura de reposo. Lo
/// que viene antes es el agachado y la preparación (las manos despegan
/// primero), y lo que viene después, el asentamiento, del que se encargan los
/// clips de zancada. En el caballo propio, 0.414 y 1.146 s (en el de
/// Quaternius, 0.43 y 1.17).
pub const JUMP_FLIGHT: (f32, f32) = (0.41, 1.15);

/// En qué punto de su ciclo (`0..1`) retoma el galope al aterrizar de un salto:
/// recién apoyada la mano de galope (la izquierda, que en el clip apoya en
/// 0.532), con las patas traseras todavía atrás, que entonces avanzan bajo el
/// cuerpo y apoyan después de las manos, como en las láminas de saltos de
/// Muybridge (*Animal Locomotion*, 1887, lám. 639).
pub const GALLOP_LANDING: f32 = 0.54;

/// Cuántos segundos del clip se alarga el impulso del despegue: el clip empieza
/// lento (a alrededor de un tercio de su ritmo) en el impulso de las patas
/// traseras, para que se note cuando el caballo deja el suelo, y se pone al día
/// hacia un tercio del vuelo.
pub const JUMP_LINGER: f32 = 0.15;

/// En qué punto del clip `Gallop_Jump` está un caballo en la fracción `flight`
/// (`0..=1`) de su salto, en segundos: el clip se reparte a lo largo del propio
/// vuelo de la física, no se reproduce con el reloj, y se demora en el impulso
/// ([`JUMP_LINGER`]).
pub fn jump_time(flight: f32) -> f32 {
    let f = flight.clamp(0.0, 1.0);
    let span = JUMP_FLIGHT.1 - JUMP_FLIGHT.0 + JUMP_LINGER;
    JUMP_FLIGHT.0 + span * f - JUMP_LINGER * (1.0 - (1.0 - f).powi(4))
}

/// El transform del modelo bajo su cuerpo: la exportación del caballo girada
/// para mirar hacia +X y escalada a tamaño real (la escala de los caballos de
/// corral).
pub fn model_transform() -> Transform {
    livestock::model_transform(Species::Horse, Heading::default())
}

/// Crea un caballo ensillado con el pelaje `coat` (montado por `rider`) bajo
/// `parent`: su cuerpo y su modelo esperando la escena. Devuelve el cuerpo.
pub(super) fn spawn_horse(
    commands: &mut Commands,
    parent: Entity,
    coat: Coat,
    rider: Option<FigureKind>,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Horse"),
            HorseBody {
                coat,
                rider,
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
    meshes: ResMut<'w, Assets<Mesh>>,
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
        meshes,
        ..
    } = &mut stores
    else {
        return;
    };
    let (player, probes) = dress_meshes(
        &mut commands,
        (cache, materials, meshes),
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
        None => warn!("horse model without its skeleton: its gallop stays the clip's"),
    }
}

/// Los huesos que mueve [`fit_horses`] bajo el modelo `root`, y sus reposos.
fn find_bones(scene: &HorseScene, root: Entity) -> Option<HorseBones> {
    let named = |name: &str| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == name))
    };
    let all = GaitBone::all();
    let mut bones = [Entity::PLACEHOLDER; GaitBone::COUNT];
    for (k, bone) in all.into_iter().enumerate() {
        bones[k] = named(&bone.name())?;
    }
    let rest = |e: Entity| scene.transforms.get(e).ok().copied();
    let mut partial = Vec::with_capacity(PARTIAL.len() + UNANIMATED.len());
    let unanimated = UNANIMATED.map(|n| (n, Property::Rotation, 0));
    for (name, property, mask) in PARTIAL.into_iter().chain(unanimated) {
        let e = named(name)?;
        partial.push((e, property, mask, rest(e)?));
    }
    let mut means = Vec::with_capacity(GALLOP_MEANS.len());
    for (name, q) in GALLOP_MEANS {
        let leg = usize::from(name.ends_with(".R")) + if name.starts_with("Back") { 2 } else { 0 };
        means.push((leg, named(name)?, Quat::from_array(q).normalize()));
    }
    let armature = rest_affine(
        bones[GaitBone::Armature.index()],
        root,
        &scene.parents,
        &scene.transforms,
    );
    let standing = [0, 1, 2, 3].map(|leg| {
        let hoof = bones[GaitBone::Hoof(leg).index()];
        rest_affine(hoof, root, &scene.parents, &scene.transforms)
            .translation
            .y
    });
    Some(HorseBones {
        bones,
        armature,
        partial,
        means,
        standing,
        body_rest: rest(bones[GaitBone::Body.index()])?.translation,
    })
}

/// Ajusta la pose de cada caballo que se reproduce después de sus clips (ver
/// [`gait`](super::gait)): la mezcla completada, el galope estirado y
/// flexionado, los vasos mantenidos en las patas.
pub(super) fn fit_horses(
    models: Option<Res<LivestockModels>>,
    rigs: Query<(&HorseRig, &HorseBones)>,
    players: Query<&AnimationPlayer>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(species) = models.as_ref().and_then(|m| m.get(Species::Horse)) else {
        return;
    };
    let gallop = Clip::Gallop.index();
    for (rig, bones) in &rigs {
        if rig.frozen || !rig.started {
            continue;
        }
        let phase = players
            .get(rig.player)
            .ok()
            .and_then(|p| p.animation(species.nodes[gallop]))
            .map_or(0.0, |a| a.seek_time() / rig.durations[gallop].max(1e-3));
        fit_horse(bones, &mut transforms, &rig.weights, phase, rig.flight);
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
        // En el aire solo se reproduce el salto, a lo largo del vuelo.
        let flight = poses.get(parent.parent()).ok().and_then(|p| p.flight);
        let mut goal = gait_weights(body.idle(clock), stride, speed);
        if flight.is_some() {
            goal = [0.0; CLIP_COUNT];
            goal[Clip::GallopJump.index()] = 1.0;
        }
        if !rig.started {
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
            for (k, (clip, duration)) in Clip::ALL.into_iter().zip(durations).enumerate() {
                player
                    .play(species.nodes[clip.index()])
                    .repeat()
                    .seek_to(start_offset(body.seed, clip) * duration);
                rig.durations[k] = duration;
            }
            rig.weights = goal;
            rig.started = true;
        }
        // Al aterrizar, el galope retoma con las manos apoyadas.
        if body.flight.is_some()
            && flight.is_none()
            && let Some(active) = player.animation_mut(species.nodes[Clip::Gallop.index()])
        {
            active.seek_to(GALLOP_LANDING * rig.durations[Clip::Gallop.index()]);
        }
        // El clip de zancada que toma el relevo empieza al mismo paso que el
        // que sale.
        let lead = stride;
        if lead != rig.lead {
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
        body.gallop = rig.weights[Clip::Gallop.index()];
        body.speed = speed;
        body.flight = flight;
        rig.flight = flight;
        for clip in Clip::ALL {
            if let Some(active) = player.animation_mut(species.nodes[clip.index()]) {
                if clip == Clip::GallopJump {
                    // Nunca se reproduce con el reloj (así que nunca hace
                    // bucle): se fija en el punto del vuelo, y se deja ahí
                    // mientras se desvanece.
                    if let Some(flight) = flight {
                        active.seek_to(jump_time(flight));
                    }
                    active.set_weight(rig.weights[clip.index()]).set_speed(0.0);
                } else {
                    active
                        .set_weight(rig.weights[clip.index()])
                        .set_speed(gait_rate(clip, velocity));
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// La superficie de un caballo sustituto: puntos en su costillar, su grupa
    /// y sus paletas (un tubo a lo largo de +X alrededor de 1.14 m de altura),
    /// su cuello y sus cuatro patas, cada uno con su normal hacia afuera.
    fn coat_points() -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::new();
        for i in 0..40 {
            let x = -0.85 + 1.45 * i as f32 / 39.0;
            for k in 0..32 {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                let n = Vec3::new(0.0, a.cos(), a.sin());
                out.push((Vec3::new(x, 1.14 + 0.29 * n.y, 0.23 * n.z), n));
            }
        }
        for i in 0..20 {
            let t = i as f32 / 19.0;
            let c = Vec3::new(0.5 + 0.45 * t, 1.3 + 0.45 * t, 0.0);
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                let n = Vec3::new(-0.7 * a.cos(), 0.7 * a.cos(), a.sin()).normalize();
                out.push((c + n * 0.13, n));
            }
        }
        for (x, z) in [(0.55, -0.17), (0.55, 0.17), (-0.65, -0.17), (-0.65, 0.17)] {
            for j in 0..20 {
                for k in 0..10 {
                    let a = k as f32 / 10.0 * std::f32::consts::TAU;
                    let n = Vec3::new(a.cos(), 0.0, a.sin());
                    out.push((Vec3::new(x, 0.9 * j as f32 / 19.0, z) + n * 0.06, n));
                }
            }
        }
        out
    }

    fn coat_colors(coat: Coat) -> Vec<(Vec3, Vec3, Rgb)> {
        coat_points()
            .into_iter()
            .map(|(p, n)| (p, n, coat.color_at(p, n)))
            .collect()
    }

    fn luma(c: Rgb) -> f32 {
        0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32
    }

    #[test]
    fn every_coat_looks_different() {
        let mean = |coat| {
            let v: Vec<_> = coat_colors(coat)
                .into_iter()
                .filter(|(p, _, _)| p.y > POINTS_HEIGHT)
                .collect();
            let sum = v.iter().fold(Vec3::ZERO, |s, (_, _, c)| {
                s + Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)
            });
            sum / v.len() as f32
        };
        for (i, a) in Coat::ALL.iter().enumerate() {
            for b in &Coat::ALL[i + 1..] {
                let d = mean(*a).distance(mean(*b));
                assert!(d > 20.0, "{a:?} vs {b:?}: {d}");
            }
        }
    }

    #[test]
    fn every_coat_has_depth_a_darker_topline_lighter_underside_and_soft_mottling() {
        for coat in Coat::ALL {
            // El costillar, de la cruz a la grupa, sin manchas blancas.
            let barrel: Vec<(Vec3, Vec3, f32)> = coat_colors(coat)
                .into_iter()
                .filter(|(p, _, c)| {
                    (-0.6..0.4).contains(&p.x) && p.y > 0.9 && *c != [232, 228, 218]
                })
                .map(|(p, n, c)| (p, n, luma(c)))
                .collect();
            let mut l: Vec<f32> = barrel.iter().map(|b| b.2).collect();
            l.sort_by(f32::total_cmp);
            let mean = l.iter().sum::<f32>() / l.len() as f32;
            let spread = (l[l.len() * 95 / 100] - l[l.len() * 5 / 100]) / mean;
            let side = |f: &dyn Fn(Vec3) -> bool| {
                let v: Vec<f32> = barrel.iter().filter(|b| f(b.1)).map(|b| b.2).collect();
                v.iter().sum::<f32>() / v.len() as f32
            };
            let (top, belly) = (side(&|n| n.y > 0.8), side(&|n| n.y < -0.6));
            assert!(spread > 0.15, "{coat:?}: a flat coat ({spread})");
            assert!(top < belly * 0.88, "{coat:?}: topline {top}, belly {belly}");
        }
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
    fn the_bayo_has_black_points_and_a_dorsal_stripe() {
        let all = coat_colors(Coat::Bayo);
        let legs: Vec<_> = all.iter().filter(|(p, _, _)| p.y < 0.4).collect();
        assert!(!legs.is_empty());
        assert!(legs.iter().all(|(_, _, c)| luma(*c) < 60.0), "black legs");
        let flank = all
            .iter()
            .filter(|(p, n, _)| p.y > 0.9 && n.z.abs() > 0.8 && p.x < 0.45);
        assert!(flank.clone().count() > 20);
        assert!(flank.clone().all(|(_, _, c)| luma(*c) > 140.0), "sandy");
        assert!(luma(Coat::Bayo.mane()) < 60.0, "black mane and tail");
        let stripe: Vec<_> = all
            .iter()
            .filter(|(p, n, _)| n.y > 0.8 && p.z.abs() < 0.02 && (-0.6..0.2).contains(&p.x))
            .collect();
        assert!(stripe.len() >= 3, "{}", stripe.len());
        assert!(
            stripe.iter().all(|(_, _, c)| luma(*c) < 100.0),
            "a dark stripe"
        );
        assert!(
            coat_colors(Coat::Alazan)
                .iter()
                .all(|(_, _, c)| *c != [104, 76, 46]),
            "the other coats have none"
        );
    }

    #[test]
    fn the_overo_shows_white_patches_from_every_three_quarter_view() {
        let all = coat_colors(Coat::Overo);
        for (x, z) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let toward = Vec3::new(x, 0.6, z).normalize();
            let seen: Vec<_> = all
                .iter()
                .filter(|(p, n, _)| n.dot(toward) > 0.3 && p.y > POINTS_HEIGHT)
                .collect();
            let white = seen.iter().filter(|(_, _, c)| luma(*c) > 200.0).count();
            let share = white as f32 / seen.len() as f32;
            assert!((0.08..0.7).contains(&share), "from ({x}, {z}): {share}");
        }
        assert_ne!(Coat::Overo.hoof(), Coat::Zaino.hoof());
    }

    #[test]
    fn the_tordillo_is_dappled() {
        let barrel: Vec<_> = coat_colors(Coat::Tordillo)
            .into_iter()
            .filter(|(p, _, _)| p.y > 0.8)
            .collect();
        let light = barrel.iter().filter(|(_, _, c)| luma(*c) > 160.0).count();
        let dark = barrel.iter().filter(|(_, _, c)| luma(*c) < 130.0).count();
        let n = barrel.len();
        assert!(
            light * 7 > n && dark * 7 > n,
            "{light} light, {dark} dark of {n}"
        );
    }

    #[test]
    fn coats_are_seeded_deterministically_and_hidalgo_rides_a_bayo() {
        assert_eq!(Coat::HIDALGO, Coat::Bayo);
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
    fn the_walk_and_the_gallop_play_at_the_ground_speed_over_their_own() {
        assert!((gait_rate(Clip::Walk, HORSE_WALK_SPEED) - 1.0).abs() < 1e-6);
        assert!((gait_rate(Clip::Gallop, GALLOP_SPEED) - 1.0).abs() < 1e-6);
        // La caminata de la lógica al ritmo del suelo; en la franja del trote,
        // a su ritmo más rápido.
        assert!((gait_rate(Clip::Walk, 3.0) - 3.0 / HORSE_WALK_SPEED).abs() < 1e-5);
        assert_eq!(
            gait_rate(Clip::Walk, 4.8),
            RATE.1,
            "the trot band: as fast as it walks"
        );
        // El galope largo al ritmo del suelo hasta su tope, y después limitado:
        // nunca frenético.
        assert!((gait_rate(Clip::Gallop, 8.0) - 8.0 / GALLOP_SPEED).abs() < 1e-5);
        assert!((gait_rate(Clip::Gallop, 12.2) - 12.2 / GALLOP_SPEED).abs() < 1e-5);
        assert_eq!(gait_rate(Clip::Gallop, 19.4), GALLOP_RATE.1);
        assert_eq!(gait_rate(Clip::Gallop, 0.0), GALLOP_RATE.0);
        assert_eq!(gait_rate(Clip::Walk, 0.0), RATE.0, "clamped");
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
    fn a_ridden_horse_stands_still_and_a_tied_one_changes_idle() {
        let mut body = HorseBody {
            coat: Coat::Zaino,
            rider: Some(FigureKind::Protagonist),
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
    fn the_galloping_horse_bends_its_back_through_the_stride() {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..80 {
            ride(&mut app, me, 12.2);
        }
        let mut flex = Vec::new();
        for _ in 0..40 {
            ride(&mut app, me, 12.2);
            let [croup, withers] = ["Back", "Torso3"].map(|n| named(app.world(), carrier, n));
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
    fn rig_of(app: &mut App) -> (Entity, [f32; CLIP_COUNT]) {
        let world = app.world_mut();
        let (_, rig) = world
            .query::<(Entity, &HorseRig)>()
            .iter(world)
            .find(|(_, r)| r.started)
            .unwrap();
        (rig.player, rig.weights)
    }

    /// El vaso de cada pata (su objetivo de IK) desde su articulación superior
    /// (la cadera trasera, el codo delantero), en el marco del cuerpo del
    /// caballo: `[front L, front R, hind L, hind R]`.
    fn hooves_from_joints(app: &App, carrier: Entity) -> [Vec3; 4] {
        let world = app.world();
        let to_body = world
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let at = |n: &str| {
            let e = named(world, carrier, n);
            to_body.transform_point3(world.get::<GlobalTransform>(e).unwrap().translation())
        };
        [
            ("FrontUpperLeg.L", "IKFrontLeg.L"),
            ("FrontUpperLeg.R", "IKFrontLeg.R"),
            ("BackLeg.L", "IKBackLeg.L"),
            ("BackLeg.R", "IKBackLeg.R"),
        ]
        .map(|(joint, hoof)| at(hoof) - at(joint))
    }

    #[test]
    fn a_jump_holds_the_clip_at_the_flights_point_tucks_the_legs_and_lands_back_in_the_gallop() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let node = app
            .world()
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .unwrap()
            .nodes[Clip::GallopJump.index()];
        let jump = Clip::GallopJump.index();
        let seek = |app: &mut App| {
            let (player, _) = rig_of(app);
            let player = app.world().get::<AnimationPlayer>(player).unwrap();
            let active = player.animation(node).unwrap();
            (active.seek_time(), active.speed())
        };
        // Las puntas y el cuerpo en el marco del cuerpo del caballo.
        let in_body = |app: &App, names: &[&str]| -> Vec<Vec3> {
            let world = app.world();
            let to_body = world
                .get::<GlobalTransform>(carrier)
                .unwrap()
                .affine()
                .inverse();
            names
                .iter()
                .map(|n| {
                    let e = named(world, carrier, n);
                    to_body.transform_point3(world.get::<GlobalTransform>(e).unwrap().translation())
                })
                .collect()
        };
        let toes = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"];
        let body_rest = {
            let world = app.world_mut();
            let (_, bones) = world
                .query::<(&HorseRig, &HorseBones)>()
                .iter(world)
                .next()
                .unwrap();
            bones.armature.transform_point3(bones.body_rest).y
        };
        // El galope: el clip de salto en cero, y el suelo de los vasos.
        let mut ground = f32::MAX;
        for _ in 0..80 {
            ride(&mut app, me, 12.2);
            ground = in_body(&app, &toes)
                .iter()
                .map(|t| t.y)
                .fold(ground, f32::min);
        }
        let (_, galloping) = rig_of(&mut app);
        assert!(galloping[jump] < 1e-3 && galloping[Clip::Gallop.index()] > 0.95);
        // El vuelo abarca el propio vuelo del clip, entero.
        assert_eq!(jump_time(0.0), JUMP_FLIGHT.0);
        assert_eq!(jump_time(1.0), JUMP_FLIGHT.1);
        let legs = [
            ("FrontLowerLeg.L_end", "IKFrontLeg.L"),
            ("FrontLowerLeg.R_end", "IKFrontLeg.R"),
            ("BackLowerLeg.L_end", "IKBackLeg.L"),
            ("BackLowerLeg.R_end", "IKBackLeg.R"),
        ];
        for flight in [
            0.0, 0.05, 0.1, 0.2, 0.25, 0.3, 0.45, 0.5, 0.7, 0.8, 0.9, 1.0,
        ] {
            for _ in 0..10 {
                fly(&mut app, me, Some(flight));
            }
            // El clip de salto se fija en el punto del vuelo, nunca se
            // reproduce con el reloj.
            let (_, weights) = rig_of(&mut app);
            assert!(weights[jump] > 0.99, "{flight}: {weights:?}");
            assert!(
                weights[Clip::Gallop.index()] < 0.01,
                "{flight}: {weights:?}"
            );
            let (at, speed) = seek(&mut app);
            assert!((at - jump_time(flight)).abs() < 1e-3, "{flight}: {at}");
            assert_eq!(speed, 0.0);
            // En el aire el cuerpo conserva su altura: se quita la elevación
            // propia del clip.
            if [0.0, 0.2, 0.5, 0.8, 1.0].contains(&flight) {
                let height = in_body(&app, &["Body"])[0].y;
                assert!(
                    (height - body_rest).abs() < 0.03,
                    "{flight}: {height} against rest {body_rest}"
                );
            }
            // Los vasos cuelgan de objetivos de IK bajo el armature, no de las
            // patas: el clip mantiene cada objetivo en el extremo de su pata (a
            // menos de 4 cm, donde su IK no llega), y quitar del cuerpo la
            // elevación del clip no debe dejar atrás los objetivos.
            if (0.1..=0.9).contains(&flight) {
                let world = app.world();
                let at = |n: &str| {
                    world
                        .get::<GlobalTransform>(named(world, carrier, n))
                        .unwrap()
                        .translation()
                };
                for (leg, (end, hoof)) in legs.iter().enumerate() {
                    let gap = at(end).distance(at(hoof));
                    assert!(gap < 0.05, "flight {flight}, leg {leg}: {gap} m");
                }
            }
            let hooves = hooves_from_joints(&app, carrier);
            // Al dejar el suelo: cada pata trasera se estira hacia atrás y
            // hacia abajo desde la cadera, cerca de su largo completo (el clip
            // solo todavía la tiene bajo la panza en ese punto, con el vaso por
            // delante de la cadera).
            if flight == 0.05 {
                for leg in &hooves[2..] {
                    assert!(leg.x < -0.3 && leg.length() > 1.0, "pushing: {leg}");
                }
            }
            if flight == 0.5 {
                // A mitad del vuelo el impulso ya terminó: plegada y
                // arrastrada por detrás de la babilla, nunca bajo la panza
                // (Muybridge, Animal Locomotion, lám. 639).
                for leg in &hooves[2..] {
                    assert!(leg.x < -0.1 && leg.length() < 1.0, "folded: {leg}");
                }
                // En lo alto los cuatro vasos están recogidos, lejos del suelo.
                for (leg, toe) in in_body(&app, &toes).iter().enumerate() {
                    assert!(
                        toe.y > ground + 0.1,
                        "leg {leg}: {} against the ground at {ground}",
                        toe.y
                    );
                }
            }
            // En lo alto las manos se recogen cerca de los codos (el clip las
            // deja a 0.32–0.40 m de ellos).
            if flight == 0.45 || flight == 0.5 {
                let [l, r, ..] = hooves;
                let reach = (l.length() + r.length()) / 2.0;
                assert!(reach < 0.66, "flight {flight}: {l} {r}");
            }
            // Un punto fijado no sigue avanzando.
            fly(&mut app, me, Some(flight));
            assert_eq!(seek(&mut app).0, at, "{flight}");
        }
        // Aterrizado: de vuelta al galope, el salto en cero, y se queda ahí (el
        // clip no queda en bucle).
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let (_, weights) = rig_of(&mut app);
        assert!(weights[jump] < 1e-3, "{weights:?}");
        assert!(weights[Clip::Gallop.index()] > 0.95, "{weights:?}");
        assert!((seek(&mut app).0 - JUMP_FLIGHT.1).abs() < 1e-3);
        // Los pesos de los clips de zancada son los que da el andar solo.
        let goal = gait_weights(Clip::Idle, Clip::Gallop, 12.2);
        for (w, g) in weights.iter().zip(goal) {
            assert!((w - g).abs() < 0.05, "{weights:?} vs {goal:?}");
        }
    }

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
        // La punta de la cola, detrás de las nalgas (a unos 0.85 m detrás de
        // la cinchera) y de los corvejones: entre las patas quedaría por
        // delante de ellos.
        let behind = |app: &App, when: &str| {
            let tip = in_body(app, carrier, "Tail9");
            let hocks = ["BackLowerLeg.L", "BackLowerLeg.R"].map(|n| in_body(app, carrier, n).x);
            assert!(tip.x < -0.85, "{when}: the tail's tip at {tip}");
            assert!(
                hocks.iter().all(|&h| tip.x < h - 0.1),
                "{when}: the tail's tip at {tip}, the hocks at {hocks:?}"
            );
        };
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        for k in 0..30 {
            ride(&mut app, me, 12.2);
            behind(&app, &format!("gallop frame {k}"));
        }
        for f in 0..=20 {
            fly(&mut app, me, Some(f as f32 / 20.0));
            behind(&app, &format!("flight {}", f as f32 / 20.0));
        }
    }

    #[test]
    fn a_jump_lands_on_the_trailing_fore_then_the_leading_one_and_the_hinds_last() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        // Las alturas de las puntas sobre el suelo en que se apoyan (el marco
        // del cuerpo, sin el arco de la física), y cuánto alcanza cada mano
        // desde el codo y cuánto queda cada pata trasera detrás de la babilla.
        let at = |app: &mut App, f: f32| {
            for _ in 0..4 {
                fly(app, me, Some(f));
            }
            let toes = ["FF.R_end", "FF.L_end", "FFB.R_end", "FFB.L_end"]
                .map(|n| in_body(app, carrier, n).y);
            let hooves = hooves_from_joints(app, carrier);
            (toes, hooves)
        };
        let ground = in_body(&app, carrier, "FF.R_end").y.min(0.05);
        // Las manos siguen plegadas durante casi todo el descenso: una que se
        // estirara antes colgaría bajo un cuerpo todavía en el aire ("parado"
        // en la caída).
        for f in [0.6, 0.7, 0.8, 0.85] {
            let (_, hooves) = at(&mut app, f);
            for fore in &hooves[..2] {
                assert!(fore.length() < 0.5, "flight {f}: unfolded early {fore}");
            }
        }
        // Al final se despliegan, la que va detrás (la derecha, con mano de
        // galope izquierda) primero.
        let (toes, _) = at(&mut app, 0.95);
        assert!(toes[0] < toes[1] - 0.1, "the trailing fore leads: {toes:?}");
        let (toes, hooves) = at(&mut app, 1.0);
        assert!(
            toes[0] < ground + 0.06,
            "landing on the trailing fore: {toes:?}"
        );
        // Casi todo su largo (de pie, unos 0.75 m del codo al menudillo).
        assert!(hooves[0].length() > 0.6, "unfolded: {:?}", hooves[0]);
        // Las patas traseras aterrizan después: todavía arriba y por detrás de
        // las babillas cuando las manos llegan al suelo.
        for (toe, hind) in toes[2..].iter().zip(&hooves[2..]) {
            assert!(*toe > toes[0] + 0.15 && hind.x < -0.3, "{toes:?} {hind}");
        }
    }

    #[test]
    fn after_a_jump_the_gallop_takes_over_on_its_leading_fore() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        for f in 0..=20 {
            fly(&mut app, me, Some(f as f32 / 20.0));
        }
        ride(&mut app, me, 12.2);
        let (player, _) = rig_of(&mut app);
        let world = app.world_mut();
        let node = world
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .unwrap()
            .nodes[Clip::Gallop.index()];
        let rig = world
            .query::<&HorseRig>()
            .iter(world)
            .next()
            .unwrap()
            .clone();
        let world = app.world();
        let active = world
            .get::<AnimationPlayer>(player)
            .unwrap()
            .animation(node)
            .unwrap();
        let phase = active.seek_time() / rig.durations[Clip::Gallop.index()];
        // Un frame de 20 ms después del aterrizaje, al ritmo del galope.
        let step = 0.02 * gait_rate(Clip::Gallop, 12.2) / rig.durations[Clip::Gallop.index()];
        assert!((phase - GALLOP_LANDING - step).abs() < 0.02, "{phase}");
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
            let dock = p("Tail4") - p("Tail2");
            let hair = p("Tail9") - p("Tail7");
            bends.push(dock.angle_between(hair).to_degrees());
            tips.push(p("Tail9").y);
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
        let hocks = in_body(&app, carrier, "BackLowerLeg.L").y;
        assert!(
            (hocks - 0.15..hocks + 0.05).contains(&lowest),
            "the tail's hair ends at {lowest} m, the hocks at {hocks} m"
        );
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
                in_body(&app, carrier, "Head").y
            })
            .collect();
        let range = heights.iter().copied().fold(f32::MIN, f32::max)
            - heights.iter().copied().fold(f32::MAX, f32::min);
        assert!((0.057..0.105).contains(&range), "the head nods {range} m");
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
        let children = |e: Entity| world.get::<Children>(e).into_iter().flatten().copied();
        let mut stack: Vec<Entity> = children(under).collect();
        while let Some(e) = stack.pop() {
            if world.get::<Name>(e).is_some_and(|n| n.as_str() == name) {
                return e;
            }
            stack.extend(children(e));
        }
        panic!("no {name}");
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
            let toes = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"]
                .map(|n| named(world, carrier, n));
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
        let rigs: Vec<(Entity, u32, [f32; CLIP_COUNT])> = world
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

    #[test]
    fn the_long_gallop_keeps_its_hooves_still_up_to_its_fastest_unhurried_pace() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 8.0, 10);
        for speed in [8.0, 12.2, 19.4] {
            app.world_mut().entity_mut(me).insert(VisualPose {
                animation: AnimationState::Gallop,
                speed,
                ..default()
            });
            from_the_seeded_start(&mut app);
            for _ in 0..100 {
                app.update();
            }
            let back = hoof_slide(&mut app, me, 300, 0.01);
            let rate = gait_rate(Clip::Gallop, speed);
            let (natural, slide) = (back / rate, speed - back);
            println!(
                "{speed} m/s: hooves back at {back:.2} m/s (rate {rate:.2}, natural {natural:.2}), slide {slide:.2} m/s"
            );
            // El ciclo estirado muestra la velocidad con la que se marca su
            // ritmo.
            assert!((natural / GALLOP_SPEED - 1.0).abs() < 0.06, "{natural}");
            if speed <= GALLOP_SPEED * GALLOP_RATE.1 {
                assert!(slide.abs() < 0.5, "{speed} m/s: slides {slide} m/s");
            } else {
                // Pasado el tope (el turbo de desarrollo) se desliza, sin ir
                // más rápido.
                assert!(slide > 0.0 && slide < speed - GALLOP_SPEED * GALLOP_RATE.1 + 0.5);
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
        // camina, a lo sumo al ritmo de la caminata más rápida de la lógica
        // (3 m/s).
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 10);
        let min_gallop = 1.45 * 0.583;
        let max_walk = 3.0 / HORSE_WALK_SPEED;
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
                assert!(
                    (back / (max_walk * HORSE_WALK_SPEED) - 1.0).abs() < 0.06,
                    "{speed}: {back}"
                );
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
        let main = drawn.into_iter().find(|d| d.name == "Main").unwrap();
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
        let tibia = |app: &App, s: &str| {
            [format!("BackUpperLeg.{s}"), format!("BackLowerLeg.{s}")]
                .map(|n| in_body(app, carrier, &n))
        };
        let gaskins: Vec<(&str, Vec<usize>, f32)> = ["L", "R"]
            .into_iter()
            .map(|s| {
                let [a, b] = tibia(&app, s);
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
                let [a, b] = [format!("BackUpperLeg.{s}"), format!("BackLowerLeg.{s}")]
                    .map(|n| in_body(&app, carrier, &n));
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
        // corvejón; eran el 3.2 %).
        assert!(thinnest > 0.95, "the gaskin thins to x{thinnest}");
        assert!(crushed * 40 <= thigh.len(), "{crushed} of {}", thigh.len());
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
            (pick("Hair"), pick("Main"))
        };
        let (rest, _) = hair(&mut app);
        let mane: Vec<usize> = (0..rest.len())
            .filter(|&i| (0.42..0.98).contains(&rest[i].x) && rest[i].y > 1.3)
            .collect();
        assert!(mane.len() > 1000, "{}", mane.len());
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
        // ahora 5.5 y 4.0).
        let walk = mane_off_the_neck(HORSE_WALK_SPEED, 20);
        let gallop = mane_off_the_neck(12.2, 20);
        println!("mane off the neck: walking {walk:.3} m, galloping {gallop:.3} m");
        assert!(gallop > 0.048, "{gallop}");
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
                let at = |n: &str| in_body(&app, carrier, &format!("{n}.{s}"));
                // Adelante de la articulación de la cadera (+X), y el ángulo de
                // la pata por delante de la vertical.
                let (hip, fetlock) = (at("BackLeg"), at("IKBackLeg"));
                under = under.max(fetlock.x - hip.x);
                back = back.min((fetlock.x - hip.x).atan2(hip.y - fetlock.y).to_degrees());
                let [elbow, k, fore] = ["FrontUpperLeg", "FrontLowerLeg", "IKFrontLeg"].map(at);
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
            cannon > 22.0,
            "a swinging cannon folds back to {cannon}° below horizontal"
        );
        assert!(
            standing > 125.0,
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

    /// La rodilla de una mano (entre el antebrazo y la caña) y su menudillo
    /// (entre la caña y el vaso, hasta la punta), en grados, a partir de
    /// [codo, rodilla, menudillo, punta del vaso].
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
            for s in ["L", "R"] {
                let names = [
                    format!("FrontUpperLeg.{s}"),
                    format!("FrontLowerLeg.{s}"),
                    format!("FrontLowerLeg.{s}_end"),
                    format!("FF.{s}_end"),
                ];
                let game = fore_joints(names.clone().map(|n| in_body(&app, carrier, &n)));
                let clip = fore_joints(raw.at(phase, names.each_ref().map(|n| n.as_str())));
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

    #[test]
    fn the_quarter_the_gaskin_and_the_shoulder_change_shape_through_the_gallop() {
        // Los músculos no solo siguen a los huesos (`tools/horse.py`
        // `MUSCLES`): cada uno se engrosa con la flexión de su articulación a
        // lo largo del galope, y su piel se aleja de su eje cuando se engrosa
        // (medido en el marco del hueso, así que su movimiento rígido no
        // cuenta).
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 10);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let muscles = [
            "Quarter.L",
            "Gaskin.L",
            "Scapula.L",
            "Quarter.R",
            "Gaskin.R",
            "Scapula.R",
        ];
        // Por músculo: (su grosor, la distancia media de su piel) en el más
        // flojo y en el más contraído, y cuántos vertices mueve.
        let mut slack = [(f32::MAX, 0.0f32); 6];
        let mut bunched = [(f32::MIN, 0.0f32); 6];
        let mut skin = [0usize; 6];
        for _ in 0..70 {
            ride(&mut app, me, 12.2);
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            let main = drawn.iter().find(|d| d.name == "Main").unwrap();
            let world = app.world();
            for (k, name) in muscles.iter().enumerate() {
                let bone = named(world, carrier, name);
                let (scale, along, centre) = world
                    .get::<GlobalTransform>(bone)
                    .unwrap()
                    .to_scale_rotation_translation();
                // Lo lejos que está del eje del músculo, a lo ancho.
                let mine: Vec<f32> = main
                    .positions
                    .iter()
                    .zip(&main.strongest)
                    .filter(|(_, j)| **j == Some(bone))
                    .map(|(p, _)| {
                        let q = along.inverse() * (*p - centre);
                        q.x.hypot(q.z)
                    })
                    .collect();
                skin[k] = mine.len();
                let mean = mine.iter().sum::<f32>() / mine.len().max(1) as f32;
                let thick = scale.x / scale.y;
                if thick < slack[k].0 {
                    slack[k] = (thick, mean);
                }
                if thick > bunched[k].0 {
                    bunched[k] = (thick, mean);
                }
            }
        }
        for (k, name) in muscles.iter().enumerate() {
            println!(
                "{name}: {} vertices; thickness x{:.3}, its skin x{:.3} from slack to bunched",
                skin[k],
                bunched[k].0 / slack[k].0,
                bunched[k].1 / slack[k].1
            );
        }
        for (k, name) in muscles.iter().enumerate() {
            assert!(skin[k] > 5, "{name} moves {} vertices", skin[k]);
            // Al menos un 10 % más grueso respecto de su largo, y su piel más
            // afuera (la de la paleta, también movida por la mano que barre
            // debajo, que tiene la mayor parte de su peso, apenas: x1.04).
            assert!(
                bunched[k].0 / slack[k].0 > 1.1,
                "{name}: {:?} {:?}",
                slack[k],
                bunched[k]
            );
            assert!(
                bunched[k].1 > slack[k].1 * 1.02,
                "{name}: {:?} {:?}",
                slack[k],
                bunched[k]
            );
        }
    }

    /// Cuántos grados por delante de la vertical va la cara de un caballo (la
    /// línea de la frente al hocico; negativo: metida hacia el pecho).
    fn face(app: &App, carrier: Entity) -> f32 {
        let f = in_body(app, carrier, "LipUpper") - in_body(app, carrier, "Forelock");
        f.x.atan2(-f.y).to_degrees()
    }

    /// Cuánto se dobla la rodilla de cada mano respecto de parado, en grados.
    fn knees(app: &App, carrier: Entity, rest: &[f32; 2]) -> [f32; 2] {
        std::array::from_fn(|k| {
            let s = ["L", "R"][k];
            let [a, b, c] = [
                format!("FrontUpperLeg.{s}"),
                format!("FrontLowerLeg.{s}"),
                format!("IKFrontLeg.{s}"),
            ]
            .map(|n| in_body(app, carrier, &n));
            (rest[k] - (a - b).angle_between(c - b).to_degrees()).abs()
        })
    }

    #[test]
    fn a_standing_horse_holds_its_face_in_front_of_the_vertical_and_stands_on_all_four_hooves() {
        // Un caballo parado lleva la cara unos 20–30° por delante de la
        // vertical; dormitando con la cabeza baja, todavía por delante; nunca
        // metida hacia el pecho. Parado descansa una pata trasera sobre la
        // pinza o mueve un vaso unos centímetros; nunca sostiene una mano
        // plegada en el aire.
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Zaino), 0.0, 50);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let toes = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"];
        let straight = {
            let mut r = [0.0; 2];
            for (k, s) in ["L", "R"].iter().enumerate() {
                let [a, b, c] = [
                    format!("FrontUpperLeg.{s}"),
                    format!("FrontLowerLeg.{s}"),
                    format!("IKFrontLeg.{s}"),
                ]
                .map(|n| in_body(&app, carrier, &n));
                r[k] = (a - b).angle_between(c - b).to_degrees();
            }
            r
        };
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
                angle > 5.0,
                "frame {frame}, {idle:?}: the face {angle}° from the vertical"
            );
            if settled > 20 && w[idle.index()] > 0.98 {
                for toe in toes {
                    let y = in_body(&app, carrier, toe).y;
                    assert!(y < 0.03, "frame {frame}, {idle:?}: {toe} {y} m up");
                }
                for bend in knees(&app, carrier, &straight) {
                    assert!(
                        bend < 20.0,
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
            assert!(face(&app, carrier) > 5.0, "walking");
        }
        for _ in 0..40 {
            ride(&mut app, me, 0.0);
        }
        for toe in toes {
            assert!(in_body(&app, carrier, toe).y < 0.03, "stopped: {toe}");
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
        assert!(rate > RATE.0 + 0.05 && rate <= 0.5, "{rate}");
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

    /// Por cada frame del caballo de `app` bajo `me`: la punta más baja (marco
    /// del cuerpo), lo más lejos que está cualquier vaso del extremo de su
    /// pata, y el giro de cada hueso de pata respecto de `rest` (según
    /// [`LEG_BONES`]).
    const LEG_BONES: [&str; 14] = [
        "FrontUpperLeg.L",
        "FrontUpperLeg.R",
        "FrontLowerLeg.L",
        "FrontLowerLeg.R",
        "BackShoulder.L",
        "BackShoulder.R",
        "BackLeg.L",
        "BackLeg.R",
        "BackUpperLeg.L",
        "BackUpperLeg.R",
        "BackLowerLeg.L",
        "BackLowerLeg.R",
        "Back",
        "Body",
    ];

    fn legs_now(app: &App, carrier: Entity, rest: &[Quat]) -> (f32, f32, Vec<f32>) {
        let world = app.world();
        let body = world
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let at = |n: &str| {
            body.transform_point3(
                world
                    .get::<GlobalTransform>(named(world, carrier, n))
                    .unwrap()
                    .translation(),
            )
        };
        let low = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"]
            .map(|n| at(n).y)
            .into_iter()
            .fold(f32::MAX, f32::min);
        let apart = [
            ("IKFrontLeg.L", "FrontLowerLeg.L_end"),
            ("IKFrontLeg.R", "FrontLowerLeg.R_end"),
            ("IKBackLeg.L", "BackLowerLeg.L_end"),
            ("IKBackLeg.R", "BackLowerLeg.R_end"),
        ]
        .map(|(h, e)| at(h).distance(at(e)))
        .into_iter()
        .fold(0.0, f32::max);
        let turns = LEG_BONES
            .iter()
            .zip(rest)
            .map(|(n, r)| {
                world
                    .get::<Transform>(named(world, carrier, n))
                    .unwrap()
                    .rotation
                    .angle_between(*r)
            })
            .collect();
        (low, apart, turns)
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
        let rest: Vec<Quat> = LEG_BONES
            .iter()
            .map(|n| {
                let e = named(app.world(), carrier, n);
                *app.world().get::<Transform>(e).unwrap()
            })
            .map(|t| t.rotation)
            .collect();
        // Los límites: lo más lejos que gira cada hueso a ritmos constantes.
        let mut limit = vec![0.0f32; LEG_BONES.len()];
        for speed in [0.0, 1.5, 3.0, 4.5, 8.0, 12.2] {
            for frame in 0..60 {
                ride(&mut app, me, speed);
                if frame > 30 {
                    let (_, _, turns) = legs_now(&app, carrier, &rest);
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
                let (low, apart, turns) = legs_now(&app, carrier, &rest);
                assert!(low > -0.03, "from {from} at {speed:.2} m/s: a toe at {low}");
                assert!(
                    apart < 0.06,
                    "from {from} at {speed:.2} m/s: a hoof {apart} m off its leg"
                );
                for ((n, t), l) in LEG_BONES.iter().zip(turns).zip(&limit) {
                    assert!(
                        t <= l + 0.12,
                        "from {from} at {speed:.2} m/s: {n} turned {t} (steady {l})"
                    );
                }
            }
        }
    }
}
