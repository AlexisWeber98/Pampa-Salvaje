//! El jinete sobre el caballo modelo: un modelo de MakeHuman (ver
//! [`model`](super::model)) sentado sobre el hueso `Torso` del caballo
//! ([`HorseBone::Torso`](super::tack::HorseBone): `BN_Spine_01` de henry; ver
//! [`horse`](super::horse)), así que sube, cabecea y gira con el propio clip
//! del caballo y se mantiene a su paso sin costo extra.
//!
//! Reproduce el clip compartido `drive` (sentado, con las manos hacia adelante
//! sobre las riendas: se entiende mejor que `sit_idle`, cuyas manos descansan
//! en el regazo), y [`pose_riders`] lo ajusta al caballo después de los
//! sistemas de animación, con cada capa calculada de nuevo a partir de la pose
//! del clip, sin reservar memoria:
//!
//! - **al paso** se sienta hondo en el recado, erguido y relajado, con el
//!   asiento acompañando el lomo del caballo (cuelga de él) mientras la columna
//!   compensa el cabeceo y el rolido de la montura ([`saddle_attitude`]), con
//!   retraso ([`COUNTER`]), para que la parte superior del cuerpo se mantenga
//!   tranquila;
//! - **al galope** monta en medio asiento ([`HALF_SEAT`]): el asiento flota
//!   sobre la montura, adelantado sobre las rodillas, y no acompaña el rebote
//!   de la montura (apenas la toca en lo alto de una zancada, nunca por
//!   debajo); las piernas lo absorben, con las rodillas y las caderas
//!   abriéndose y cerrándose y los talones bajando ([`HEEL`]); el jinete se
//!   inclina unos 20° desde las caderas, con la espalda recta ([`LEAN`],
//!   [`HIP_SHARE`]), y hacia atrás al frenar;
//! - **sobre un salto** mantiene el equilibrio: sostiene el medio asiento, se
//!   pliega hacia adelante desde las caderas cuando el caballo despega y en lo
//!   alto ([`JUMP_FOLD`], [`jump_fold`]), absorbiendo en las caderas, y no en
//!   la columna, el cabeceo del caballo desde que dejó el suelo
//!   ([`JUMP_BALANCE`]), con la mano de las riendas hacia adelante y hacia
//!   abajo a lo largo del cuello ([`JUMP_HAND`]); al aterrizar, se endereza y
//!   las rodillas y las caderas ceden, con el asiento bajando hasta la montura
//!   y nunca dentro de ella ([`landing`], [`LANDING_GIVE`]);
//! - la cabeza se mantiene nivelada y mira hacia adelante, estabilizada contra
//!   la inclinación y el cabeceo de la montura;
//! - cada pierna se dobla alrededor del costillar con un IK de dos huesos
//!   ([`two_bone`]): el tobillo hacia su estribo ([`ANKLES`]), la rodilla hacia
//!   afuera y hacia adelante ([`KNEE_POLE`]);
//! - la mano izquierda sostiene ambas riendas (a la manera del gaucho, con la
//!   derecha libre para el rebenque o el lazo) delante de la cabezada al paso
//!   ([`REINS_HAND`]), sobre la cruz al galope, una rienda en cada mano
//!   ([`GALLOP_HANDS`]), y sigue la
//!   boca del caballo para que el contacto se mantenga parejo ([`FOLLOW`]): el
//!   codo se abre cuando la cabeza va hacia adelante y hacia abajo. La mano
//!   derecha descansa junto a su muslo ([`THIGH_HAND`]), suelta, balanceándose
//!   con retraso con el caballo (un poco al paso, más y elevándose al galope),
//!   hacia adelante al frenar y, de vez en cuando, hasta el ala del sombrero
//!   estando quieto ([`free_hand`]); ambos hombros se encogen con retraso con
//!   la montura ([`SHRUG`]);
//! - quieto unos segundos ([`STILL`]), la mano de las riendas se apoya en la
//!   cabezada, el peso se desplaza de vez en cuando y la cabeza mira a su
//!   alrededor.
//!
//! Alguien puede ir **en ancas** ([`Pillion`], en la entidad del personaje
//! montado): [`seat_pillions`] lo sienta sobre la grupa ([`PILLION_SEAT`]),
//! colgado de su hueso ([`HorseJoint::Croup`](super::bones::HorseJoint)), que la
//! sigue en cada andar, justo detrás del recado y del jinete. Sin riendas ni
//! estribos: reproduce `sit_idle` y [`pose_riders`], después de poner en pose
//! al jinete, lo sienta a horcajadas ([`Behind`]): derecho en las caderas
//! contra el cabeceo de la grupa ([`PILLION_LEVEL`]), las piernas colgando a
//! los costados ([`PILLION_ANKLES`]), los hombros en su lugar, agarrado de la
//! faja del jinete ([`PILLION_HOLD`]) y con la cara junto a su hombro derecho
//! ([`PILLION_ASIDE`]). Con alguien en ancas el jinete galopa sentado, sin
//! medio asiento.
//!
//! Los retrasos son resortes con amortiguamiento crítico ([`Spring`]), iguales
//! a cualquier frame rate.
//!
//! Los objetivos están en el espacio propio del modelo del jinete: +Z su
//! frente, +X su izquierda, +Y hacia arriba, desde la pelvis; se midieron sobre
//! el costillar del caballo (0.47 m de ancho en el asiento, la panza a 0.86 m)
//! y las piernas del protagonista (0.865 m de la cadera al tobillo). Una vez
//! que el jinete está sentado ([`seat_riders`]), las riendas van hasta sus
//! manos y los estribos van en sus pies ([`carry_stirrups`]: quedan bajo el
//! torso del caballo, así que una escena de jinete creada de nuevo deja los
//! aperos completos).

use std::f32::consts::FRAC_PI_2;

use bevy::math::Affine3A;
use bevy::prelude::*;
use pampa_core::pulperia::Pastime;

use super::clips::CharacterClips;
use super::figure::FigureKind;
use super::horse::{HorseBody, HorseModel, transform_of};
use super::model::{CharacterModel, CharacterModels, ModelPending};
use super::rig::{GAZE_MAX, Gaze, Rig, fit_pelvis};
use super::tack::{STIRRUP, Strap, Tack};

/// El modelo de un jinete, sentado en un caballo: en la raíz del modelo, hijo
/// del hueso `Torso` del caballo.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Riding {
    /// El cuerpo del caballo, o el asiento de un vehículo (sin caballo
    /// ensillado debajo: ver [`pose_riders`]).
    pub horse: Entity,
    pub kind: FigureKind,
    pub seed: u32,
}

/// Marca a un [`Riding`] sentado en un banco, sin riendas, que pasa el rato
/// con su [`Pastime`]: reproduce el clip de eso
/// ([`bench_clip`](super::rig::bench_clip)) en vez del de las riendas.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct OnBench(pub Pastime);

/// A qué altura del piso está la parte de arriba de un banco ([`OnBench`]): el
/// de una mesa de pulpería. Las piernas bajan hasta que las suelas apoyan en
/// ese piso.
pub const BENCH_HEIGHT: f32 = pampa_core::pulperia::BENCH_HEIGHT;

/// Hacia dónde se doblan las rodillas de alguien sentado en un banco, primero
/// la izquierda y después la derecha: hacia adelante y apenas hacia afuera.
pub const BENCH_KNEE_POLE: [Vec3; 2] = [Vec3::new(0.2, 0.3, 1.0), Vec3::new(-0.2, 0.3, 1.0)];

/// A qué altura por encima de la parte superior del pellón está la articulación
/// de la pelvis del jinete: el fondillo de los pantalones del protagonista está
/// 0.147 m por debajo de ella, un poco hundido en el cuero de oveja.
pub const PELVIS_RISE: f32 = 0.13;

/// Dónde se coloca el modelo de un jinete en el marco del cuerpo del caballo
/// para un asiento sobre el pellón en `seat`: con su pelvis sostenida en su
/// origen ([`pose_riders`]) justo por encima, mirando hacia el frente del
/// caballo (el glTF mira hacia +Z).
pub fn seat_transform(seat: Vec3) -> Affine3A {
    Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2),
        seat + Vec3::Y * PELVIS_RISE,
    )
}

/// Alguien que va en ancas, sobre la grupa detrás del jinete, en el caballo
/// ensillado de un personaje montado ([`CharacterVisual::Mounted`]): se
/// inserta en la entidad del personaje junto a su visual, y se quita para
/// bajarlo. Sin riendas ni estribos; ver [`seat_pillions`].
///
/// [`CharacterVisual::Mounted`]: super::CharacterVisual::Mounted
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pillion(pub FigureKind);

/// Marca el modelo de quien va en ancas ([`Pillion`]): un [`Riding`] cuyo
/// asiento es la grupa, que [`pose_riders`] sienta a horcajadas. Lleva el
/// cuerpo del caballo, contra cuyo cabeceo se mantiene derecho.
#[derive(Component, Debug, Clone, Copy)]
pub struct Behind(pub Entity);

/// En el cuerpo de un caballo con alguien en ancas: quién es, el asiento
/// sobre la grupa (del que cuelga su modelo) y el hueso de la grupa del que
/// cuelga el asiento (otro si la escena del caballo se creó de nuevo).
#[derive(Component, Debug, Clone, Copy)]
pub struct PillionSeat {
    pub kind: FigureKind,
    pub seat: Entity,
    pub croup: Entity,
}

/// Dónde se apoya en la grupa quien va en ancas, en el marco del cuerpo del
/// caballo: sobre la línea del lomo, en lo alto de la grupa (la piel de henry
/// en reposo está a 1.53 m entre -0.45 y -0.30), justo detrás del recado y
/// del jinete (lo último de ellos, el poncho y la bajera, llega a x -0.21),
/// no sobre las caderas, que se mueven mucho. Más adelante, al galope, el ala
/// de su sombrero entraba en el hombro del jinete.
pub const PILLION_SEAT: Vec3 = Vec3::new(-0.38, 1.528, 0.0);

/// A qué altura por encima de la piel de la grupa está la articulación de la
/// pelvis de quien va en ancas: el fondillo de los pantalones queda 0.147 m
/// por debajo de ella, y sin el cuero de oveja del recado no se hunde.
pub const PILLION_RISE: f32 = 0.15;

/// Adónde van los tobillos de quien va en ancas, primero el izquierdo y
/// después el derecho, desde la pelvis (espacio del jinete): colgando sueltos
/// a los costados de la panza, un poco más adelante que las caderas.
pub const PILLION_ANKLES: [Vec3; 2] = [Vec3::new(0.40, -0.66, 0.06), Vec3::new(-0.40, -0.66, 0.06)];

/// Qué parte del cabeceo y del rolido de la grupa (henry galopa con el lomo
/// unos 8° hacia adelante) deshace quien va en ancas en las caderas: sin esto
/// se iría con la cabeza sobre la espalda del jinete.
pub const PILLION_LEVEL: f32 = 0.8;

/// Cuánto se dobla hacia su derecha en el pecho, y cuánto gira la cabeza
/// hacia ese lado, en radianes: pegado a la espalda del jinete, asoma la cara
/// junto a su hombro derecho (el izquierdo lleva el poncho doblado) en lugar
/// de chocar con su cabeza.
pub const PILLION_ASIDE: (f32, f32) = (0.22, 0.45);

/// Dónde se agarra del jinete, primero la mano izquierda y después la
/// derecha: a los costados de su faja, hacia los riñones (más adelante, al
/// galope se le iría de las manos), en el espacio del
/// modelo del jinete desde su `spine_01`.
pub const PILLION_HOLD: [Vec3; 2] = [
    Vec3::new(0.17, -0.03, -0.09),
    Vec3::new(-0.17, -0.03, -0.09),
];

/// Hacia dónde se doblan sus codos al agarrarse: hacia afuera y hacia abajo.
pub const PILLION_ELBOW_POLE: [Vec3; 2] = [Vec3::new(1.0, -1.0, -0.2), Vec3::new(-1.0, -1.0, -0.2)];

/// Hacia dónde se doblan sus rodillas: hacia adelante y hacia afuera,
/// alrededor de la grupa.
pub const PILLION_KNEE_POLE: [Vec3; 2] = [Vec3::new(1.0, -0.2, 1.0), Vec3::new(-1.0, -0.2, 1.0)];

/// Dónde se ubica el modelo de quien va en ancas bajo su asiento
/// ([`PillionSeat`]): mirando hacia el frente del caballo, con la pelvis
/// [`PILLION_RISE`] por encima de la piel.
pub fn pillion_transform() -> Affine3A {
    Affine3A::from_rotation_translation(Quat::from_rotation_y(FRAC_PI_2), Vec3::Y * PILLION_RISE)
}

/// Adónde van los tobillos, primero el izquierdo y después el derecho, desde la
/// pelvis (espacio del jinete): en los estribos, 0.37 m hacia afuera a cada
/// lado (el costillar está a 0.23 m de la columna en su parte más ancha, y las
/// piernas abrazan el recado: los bastos y el cojinillo, medidos con
/// `tack::tests::seat_clearances`), 10 cm adelante, con las suelas de las botas
/// unos 12 cm por debajo de la panza.
pub const ANKLES: [Vec3; 2] = [Vec3::new(0.37, -0.69, 0.1), Vec3::new(-0.37, -0.69, 0.1)];

/// Adónde van los tobillos en medio asiento: los estribos acortados un palmo
/// (las aciones siguen a los pies) y los talones bajo las caderas, así que las
/// rodillas se doblan unos 110°. On henry (H6): (0.36, -0.6, 0.02) ->
/// (0.40, -0.52, 0.1), shorter, wider and under the knee: at 0.36 out the
/// boots went 2.8 cm into the cojinillo, and at -0.6 the knees opened to
/// ~135° (129° already on the old horse); now ~111-115°, the boots at most
/// 1.4 cm into the wool (`FLEECE_GIVE` 2 cm).
pub const GALLOP_ANKLES: [Vec3; 2] = [Vec3::new(0.40, -0.52, 0.1), Vec3::new(-0.40, -0.52, 0.1)];

/// Adónde va la muñeca de la mano de las riendas (la izquierda: un gaucho monta
/// con una sola mano, con ambas riendas en ella): delante de la cabezada, un
/// poco hacia su costado, para que el brazo no toque el poncho doblado sobre
/// ese hombro.
pub const REINS_HAND: Vec3 = Vec3::new(0.12, 0.18, 0.32);

/// Dónde descansa después de unos segundos quieto: sobre la cabezada. Since
/// H6 low in front, over the pommel rather than on it ((0.06, -0.02, 0.22)
/// was out of the arm's reach: the arm locked straight and the forearm went
/// into the belly).
pub const POMMEL_HAND: Vec3 = Vec3::new(0.115, 0.125, 0.26);

/// Dónde descansa la mano libre (la derecha): sobre su muslo, como si
/// sostuviera un rebenque sin apretar, con el codo doblado unos 50°. Since
/// H6 3 cm further out and 2 cm forward ((-0.2, 0.08, 0.1)): sitting upright
/// ([`SIT_UP`]) the upper arm brushed the side of the body.
pub const THIGH_HAND: Vec3 = Vec3::new(-0.23, 0.08, 0.12);

/// Adónde va, de vez en cuando, estando quieto: al ala del sombrero.
pub const HAT_HAND: Vec3 = Vec3::new(-0.1, 0.62, 0.16);

/// Dónde están los hombros del protagonista de MakeHuman sentado, izquierdo y
/// derecho, en el espacio del jinete, y cuánto mide su brazo del hombro a la
/// muñeca, en metros: el cuerpo sobre el que se midieron los destinos de las
/// manos. Cada jinete los lleva a sus propios hombros y a su largo de brazo
/// ([`fit_hand`]), así que un cuerpo de otras proporciones (los hombros más
/// altos, los brazos más largos) no cruza el brazo por el pecho ni lo estira
/// rígido.
pub const REF_SHOULDERS: [Vec3; 2] = [
    Vec3::new(0.16240013, 0.45665276, -0.13270737),
    Vec3::new(-0.14283384, 0.46715158, -0.15927213),
];
pub const REF_ARM: f32 = 0.51243;

/// Un destino de la mano medido sobre el protagonista ([`REF_SHOULDERS`],
/// [`REF_ARM`]) llevado al jinete cuyo hombro sentado está en `shoulder` y
/// cuyo brazo mide `arm`: el mismo camino desde el hombro, a la escala de su
/// brazo.
pub fn fit_hand(target: Vec3, side: usize, (shoulder, arm): (Vec3, f32)) -> Vec3 {
    shoulder + (target - REF_SHOULDERS[side]) * (arm / REF_ARM)
}

/// `target` brought within `reach` (a share of the arm, see [`REACH`]) of an
/// arm `arm` long whose shoulder is now at `shoulder`, along the line from
/// the shoulder.
pub fn within_reach(target: Vec3, shoulder: Vec3, arm: f32, reach: f32) -> Vec3 {
    shoulder + (target - shoulder).clamp_length_max(reach * arm)
}

/// `hand` (model space) kept at least [`CHEST_CLEAR`] in front of the chest
/// joint (`spine_03`, at `chest`) in the half-seat (`half`, `0..1`):
/// following henry's mouth back at the gallop, the rein hand brought the
/// forearm into the sternum.
pub fn clear_of_chest(hand: Vec3, chest: Vec3, half: f32) -> Vec3 {
    let clear = hand.z.max(chest.z + CHEST_CLEAR);
    Vec3::new(hand.x, hand.y, hand.z + (clear - hand.z) * half)
}

/// How far in front of the chest joint the rein hand stays at least in the
/// half-seat, in metres: the chest's depth in front of `spine_03` plus a
/// forearm.
pub const CHEST_CLEAR: f32 = 0.3;

/// How far from the shoulder a hand goes at most, as a share of the arm's
/// length (shoulder to wrist), sitting and in the half-seat: sitting, the
/// elbow always keeps some bend instead of locking straight like a rod; in
/// the half-seat the rein arm may stretch after henry's nodding head.
pub const REACH: (f32, f32) = (0.9, 1.0);

/// How much the rider sits up from the hips out of the half-seat, in radians:
/// the sitting clip leans back about 7°, which put the shoulders behind the
/// hips and the rein hand out of the arm's reach.
pub const SIT_UP: f32 = 0.12;

/// Cuánto se balancea como máximo la mano libre desde su reposo, en metros.
pub const FREE_REACH: f32 = 0.12;

/// Cuánto se encogen los hombros con el movimiento de la montura, en radianes
/// por radián.
pub const SHRUG: f32 = 0.6;

/// Cuánto tiempo está quieto un caballo antes de que se asienten las manos de
/// su jinete, en s.
pub const STILL: f32 = 3.0;

// Qué tan rápido sigue cada uno de los resortes del jinete ([`Spring::step`]),
// en rad/s: cuanto más alto, antes se pone al día (cierra la mayor parte de un
// paso en unos 4 / ω segundos).

/// La trayectoria suavizada de la montura, cuyo resto (su rebote) absorbe el
/// jinete.
pub const SADDLE_EASE: f32 = 4.0;
/// La inclinación al frenar, entrando y saliendo.
pub const BRAKING_EASE: f32 = 6.0;
/// Las manos que se asientan sobre la cabezada estando quieto: lento.
pub const REST_EASE: f32 = 2.5;
/// El medio asiento que entra con el galope, con retraso.
pub const HALF_SEAT_EASE: f32 = 3.0;
/// La inclinación de la parte superior del cuerpo.
pub const LEAN_EASE: f32 = 4.0;
/// La columna que compensa el cabeceo y el rolido de la montura. Rápido a
/// propósito: la compensación solo tiene que ir uno o dos frames detrás de la
/// montura para mantener la parte superior del cuerpo más estable que las
/// caderas; una lenta compensaría la zancada anterior y, en cambio, haría
/// hamacar al jinete.
/// 40 -> 80 on henry (H6): at the gallop his back snaps 9° nose up in 40 ms
/// once a stride, and at 40 the upper body kept ~70-90% of the saddle's pitch.
pub const COUNTER_EASE: f32 = 80.0;
/// La trayectoria suavizada del freno, cuyo resto absorbe la mano de las
/// riendas.
pub const BIT_EASE: f32 = 3.0;
/// La parte con retraso de la nivelación de la cabeza.
pub const HEAD_EASE: f32 = 8.0;
/// El encogimiento de los hombros.
pub const SHRUG_EASE: f32 = 8.0;
/// La mano libre: un brazo suelto que va con retraso respecto del torso.
pub const FREE_EASE: f32 = 7.0;

/// La desaceleración que corresponde a un frenado completo, en m/s²: la
/// inclinación hacia atrás es la desaceleración del caballo dividida por ella.
/// Acoplado a la lógica: debe coincidir con `pampa_core::RiderParams::braking`
/// (el frenado del jugador).
pub const BRAKING: f32 = 8.0;

/// Por debajo de esta velocidad sobre el suelo un caballo está quieto, en m/s.
pub const STANDING: f32 = 0.05;

/// La velocidad que corresponde a un paso completo, en m/s, para el asiento al
/// paso y el balanceo del brazo libre. Acoplado a la lógica:
/// `pampa_core::RiderParams::walk_max` (4,5 km/h).
pub const WALK_TOP: f32 = 4.5 / 3.6;

/// El desplazamiento del peso estando quieto: su período, en segundos, es el
/// primero más la semilla del jinete módulo el segundo (7–9 s), y su amplitud,
/// en radianes, el tercero.
pub const SHIFT: (f32, u32, f32) = (7.0, 3, 0.035);

/// Cuánto cede como máximo la mano de las riendas con el freno, en metros.
/// 0.12 -> 0.25 on henry (H6): his bit travels 0.33 m up and down and 0.18 m
/// fore and aft against the rider at full gallop.
pub const GIVE_MAX: f32 = 0.25;

/// Cuánto se arquea o se hunde como máximo la espalda, en radianes.
pub const FLEX_MAX: f32 = 0.12;

/// Cómo se balancea la mano libre con el movimiento de la montura más allá de
/// su trayectoria suavizada: cada fila (la x, la y y la z de la mano) pondera
/// la subida, el cabeceo y el rolido de la montura, en metros por metro o por
/// radián.
pub const FREE_SWING: [[f32; 3]; 3] = [[0.15, 0.0, -0.8], [-1.6, 0.3, 0.0], [0.8, 1.2, 0.0]];

/// Cuánto más se balancea el brazo libre al paso que al galope (al paso se
/// balancea con menos movimiento), y cuánto se acerca a la cabezada al frenar
/// (una fracción del camino).
pub const FREE_WALK: f32 = 2.0;
pub const FREE_BRAKE: f32 = 0.45;

/// El toque al ala del sombrero: la sal de su semilla (para que no coincida con
/// la mirada de la cabeza) y la velocidad de su reloj (mucho más raro que una
/// mirada).
pub const HAT_SALT: u32 = 0x5eed;
pub const HAT_PACE: f32 = 0.4;

/// How far the free (right) hand's wrist and fingers go from the reins clip's
/// fist towards their rest while it holds no rein: a loose, half-open hand.
pub const LOOSE_HAND: f32 = 0.8;

/// Over which part of a glance's size (`pampa_core::pose::glance`, at least
/// 0.3) the free hand goes from the thigh to the brim: every touch reaches the
/// brim, rising over the glance's first few tenths of a second instead of its
/// whole slow ramp.
pub const HAT_FROM: f32 = 0.2;
pub const HAT_TO: f32 = 0.3;

/// Hacia dónde se doblan las rodillas, primero la izquierda y después la
/// derecha: hacia adelante y tan afuera como adelante, alrededor del recado.
pub const KNEE_POLE: [Vec3; 2] = [Vec3::new(1.0, -0.1, 1.0), Vec3::new(-1.0, -0.1, 1.0)];

/// Hacia dónde se doblan los codos: hacia abajo, hacia afuera y hacia atrás (el
/// codo de las riendas bien afuera, sobre el extremo colgante del poncho).
pub const ELBOW_POLE: [Vec3; 2] = [Vec3::new(1.2, -1.0, -0.6), Vec3::new(-0.5, -1.0, -0.6)];

/// Cuánto se inclina el jinete hacia adelante en medio asiento, al paso, y
/// hacia atrás frenando tan fuerte como frena la lógica (8 m/s²), en radianes:
/// además de la propia inclinación hacia atrás del clip de sentado, unos 20°
/// respecto de la vertical en medio asiento.
pub const LEAN: (f32, f32, f32) = (0.5, 0.03, 0.12);

/// Cuánto de la inclinación está en las caderas: la espalda se mantiene recta.
pub const HIP_SHARE: f32 = 0.7;

/// El medio asiento al galope: cuánto flota el asiento sobre la montura (antes
/// de quitar el rebote de la montura: en lo alto de una zancada apenas la toca)
/// y cuánto se adelanta sobre las rodillas, en metros.
pub const HALF_SEAT: Vec3 = Vec3::new(0.0, 0.06, 0.1);

/// Cuánto bajan los talones en medio asiento, en radianes (más a medida que el
/// asiento baja sobre los estribos).
pub const HEEL: f32 = 0.2;

/// Where the hands go in the half-seat, the left first: at the gallop the
/// rider takes the reins in both hands (S9), each rein in its own hand, either
/// side of the crest a little apart, forward over the withers and a little
/// above them, clear of the horse's neck. Since H6 (one hand at
/// (0.04, 0.22, 0.4) before).
pub const GALLOP_HANDS: [Vec3; 2] = [Vec3::new(0.09, 0.22, 0.3), Vec3::new(-0.09, 0.22, 0.3)];

/// How far the right hand meets the left one, in the rider's space, while it
/// comes to take its rein going into the half-seat (and leaves it coming out):
/// a hand's width to its right.
pub const MEET: Vec3 = Vec3::new(-0.06, 0.0, 0.0);

/// Cuánto del movimiento del freno sigue la mano de las riendas, al paso y en
/// medio asiento: el contacto se mantiene parejo. In the half-seat 0.85 ->
/// 1.0 on henry (H6), whose head nods far more than the old horse's.
pub const FOLLOW: (f32, f32) = (0.35, 1.0);

/// Cuánto del cabeceo y del rolido de la montura (más allá de su trayectoria
/// suavizada) compensa la columna, para que la parte superior del cuerpo se
/// mantenga más estable que las caderas.
pub const COUNTER: (f32, f32) = (0.95, 0.6);
/// Cuánto se arquea la espalda cuando la montura sube debajo de ella al paso,
/// en radianes por metro.
pub const FLEX: f32 = 1.2;

/// Cuánto se pliega el jinete hacia adelante desde las caderas sobre un salto,
/// en radianes, además de la inclinación del medio asiento ([`jump_fold`]).
pub const JUMP_FOLD: f32 = 0.4;

/// Adónde va la mano de las riendas sobre un salto, además de donde ya está:
/// hacia adelante y hacia abajo a lo largo del cuello del caballo, dándole
/// rienda (una cesión sobre la crin), en metros con el pliegue completo.
/// On henry (H6) 0.16 -> 0.2 m forward, and the hand stops following the mouth
/// while folded: his head comes up and back over the top, and the hand went
/// only 7 cm forward along the horse (the old horse: 24 cm).
pub const JUMP_HAND: Vec3 = Vec3::new(0.0, -0.06, 0.2);

/// Cuánto del cabeceo del caballo sobre un salto (desde que dejó el suelo)
/// absorbe el jinete en las caderas: la parte superior de su cuerpo mantiene el
/// equilibrio.
pub const JUMP_BALANCE: f32 = 0.9;

/// El pliegue que entra y sale: rápido, pero nunca de golpe.
pub const JUMP_EASE: f32 = 14.0;

/// Cuánto le lleva el aterrizaje al jinete, en segundos ([`landing`]): se
/// endereza y se hunde en la montura a través de las rodillas y las caderas, y
/// después sigue cabalgando.
pub const LANDING: f32 = 0.5;

/// Cuánto se endereza al aterrizar, en radianes contra la inclinación del medio
/// asiento, y cuánto de la flotación del medio asiento ceden sus rodillas y
/// caderas en el punto más bajo.
pub const LANDING_GIVE: (f32, f32) = (0.25, 1.0);

/// La fracción del vuelo (`0..1`) en la que el jinete ya está plegado hacia
/// adelante.
pub const JUMP_FOLD_IN: f32 = 0.1;

/// La fracción del vuelo (`0..1`) en la que empieza a enderezarse para el
/// aterrizaje; vuelve a estar erguido cuando el caballo aterriza.
pub const JUMP_SIT_UP: f32 = 0.7;

/// Cuánto está plegado hacia adelante el jinete (`0..1`) en la fracción
/// `flight` (`0..=1`) del salto de su caballo: plegándose cuando el caballo
/// despega ([`JUMP_FOLD_IN`]), plegado en lo alto, y enderezándose otra vez
/// para el aterrizaje (desde [`JUMP_SIT_UP`]).
pub fn jump_fold(flight: f32) -> f32 {
    let smooth = |u: f32| {
        let u = u.clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    };
    smooth(flight / JUMP_FOLD_IN) * (1.0 - smooth((flight - JUMP_SIT_UP) / (1.0 - JUMP_SIT_UP)))
}

/// Cuánto del aterrizaje (`0..1`) absorbe el jinete `since` segundos después de
/// que aterriza su caballo: nada en el contacto, todo a mitad de [`LANDING`]
/// (un cuarto de segundo después), y nada otra vez cuando termina.
pub fn landing(since: f32) -> f32 {
    if !(0.0..LANDING).contains(&since) {
        return 0.0;
    }
    (std::f32::consts::PI * since / LANDING).sin().powi(2)
}

/// Dónde se sostienen las riendas sobre el hueso de la mano (la palma), en su
/// propio espacio.
pub const PALM: Vec3 = Vec3::new(0.0, 0.05, 0.0);

/// Los huesos del jinete que mueven las capas, en el orden de la jerarquía del
/// modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bone {
    Root,
    Pelvis,
    Spine1,
    Spine2,
    Spine3,
    Neck,
    Head,
    Clavicle(usize),
    UpperArm(usize),
    LowerArm(usize),
    Hand(usize),
    Thigh(usize),
    Calf(usize),
    Foot(usize),
    Ball(usize),
}

impl Bone {
    pub const COUNT: usize = 23;

    /// Todos los huesos, cada uno después de su padre.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Root,
        Self::Pelvis,
        Self::Spine1,
        Self::Spine2,
        Self::Spine3,
        Self::Neck,
        Self::Head,
        Self::Clavicle(0),
        Self::Clavicle(1),
        Self::UpperArm(0),
        Self::UpperArm(1),
        Self::LowerArm(0),
        Self::LowerArm(1),
        Self::Hand(0),
        Self::Hand(1),
        Self::Thigh(0),
        Self::Thigh(1),
        Self::Calf(0),
        Self::Calf(1),
        Self::Foot(0),
        Self::Foot(1),
        Self::Ball(0),
        Self::Ball(1),
    ];

    /// El nombre de su nodo en un modelo `game_engine` (0 es el izquierdo).
    pub fn name(self) -> String {
        let side = |s: usize| if s == 0 { "l" } else { "r" };
        match self {
            Self::Root => "Root".into(),
            Self::Pelvis => "pelvis".into(),
            Self::Spine1 => "spine_01".into(),
            Self::Spine2 => "spine_02".into(),
            Self::Spine3 => "spine_03".into(),
            Self::Neck => "neck_01".into(),
            Self::Head => "head".into(),
            Self::Clavicle(s) => format!("clavicle_{}", side(s)),
            Self::UpperArm(s) => format!("upperarm_{}", side(s)),
            Self::LowerArm(s) => format!("lowerarm_{}", side(s)),
            Self::Hand(s) => format!("hand_{}", side(s)),
            Self::Thigh(s) => format!("thigh_{}", side(s)),
            Self::Calf(s) => format!("calf_{}", side(s)),
            Self::Foot(s) => format!("foot_{}", side(s)),
            Self::Ball(s) => format!("ball_{}", side(s)),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0)
    }
}

/// Los huesos de un jinete sentado, ubicados cuando se crea su escena; en la
/// raíz del modelo junto a su [`Rig`].
#[derive(Component, Debug, Clone)]
pub struct Seated {
    /// En el orden de [`Bone::ALL`].
    pub bones: [Entity; Bone::COUNT],
    /// La ubicación de cada estribo sobre el hueso de su pie, primero el
    /// izquierdo y después el derecho: su piso bajo la almohadilla del pie y su
    /// argolla atravesada.
    pub stirrups: [Transform; 2],
    /// A qué altura sobre la suela está el hueso de la almohadilla de cada pie
    /// (`ball`), primero el izquierdo, medido parado en reposo.
    pub ball_rise: [f32; 2],
    /// Each clavicle's turn under `spine_03` standing at rest, the left first:
    /// the reins clip pulls the shoulders ~8 cm back and up behind the neck,
    /// so [`pose_riders`] sets them back to rest before shrugging them.
    pub clavicles: [Quat; 2],
    /// The free (right) hand's turn under its forearm and each of its finger
    /// bones' turn under its parent, standing at rest: the reins clip closes
    /// both hands in a fist, so [`pose_riders`] opens the free one towards
    /// its rest ([`LOOSE_HAND`]) while it holds no rein.
    pub loose: (Quat, Vec<(Entity, Quat)>),
}

impl Seated {
    fn bone(&self, bone: Bone) -> Entity {
        self.bones[bone.index()]
    }

    /// Los huesos del modelo bajo `root` (`named` busca un nodo por nombre),
    /// con los estribos ubicados a partir de su pose de reposo. `None` sin el
    /// esqueleto completo.
    pub fn find(
        root: Entity,
        named: impl Fn(&str) -> Option<Entity>,
        rest: impl Fn(Entity) -> Affine3A,
    ) -> Option<Self> {
        let mut bones = [Entity::PLACEHOLDER; Bone::COUNT];
        for (k, bone) in Bone::ALL.into_iter().enumerate() {
            bones[k] = named(&bone.name())?;
        }
        let seated = Self {
            bones,
            stirrups: [Transform::IDENTITY; 2],
            ball_rise: [0.0; 2],
            clavicles: [Quat::IDENTITY; 2],
            loose: (Quat::IDENTITY, Vec::new()),
        };
        // En el espacio propio del modelo, parado en reposo.
        let to_model = rest(root).inverse();
        let at = |b: Bone| to_model * rest(seated.bone(b));
        let stirrups = [0, 1].map(|s| {
            let (foot, ball) = (at(Bone::Foot(s)), at(Bone::Ball(s)));
            let tread = Vec3::new(
                ball.translation.x,
                STIRRUP.y / 2.0 - 0.01,
                (foot.translation.z + ball.translation.z) / 2.0,
            );
            let ring = Affine3A::from_scale_rotation_translation(STIRRUP, Quat::IDENTITY, tread);
            transform_of(foot.inverse() * ring)
        });
        // Parado en reposo, el origen del modelo está en el piso.
        let ball_rise = [0, 1].map(|s| at(Bone::Ball(s)).translation.y);
        let chest = at(Bone::Spine3).inverse();
        let clavicles = [0, 1].map(|s| {
            let (_, turn, _) = (chest * at(Bone::Clavicle(s))).to_scale_rotation_translation();
            turn
        });
        let local = |parent: Affine3A, child: Affine3A| {
            let (_, turn, _) = (parent.inverse() * child).to_scale_rotation_translation();
            turn
        };
        let wrist = local(at(Bone::LowerArm(1)), at(Bone::Hand(1)));
        let mut fingers = Vec::new();
        for finger in ["thumb", "index", "middle", "ring", "pinky"] {
            let mut parent = at(Bone::Hand(1));
            for k in 1..=3 {
                let Some(bone) = named(&format!("{finger}_0{k}_r")) else {
                    break;
                };
                let place = to_model * rest(bone);
                fingers.push((bone, local(parent, place)));
                parent = place;
            }
        }
        Some(Self {
            stirrups,
            ball_rise,
            clavicles,
            loose: (wrist, fingers),
            ..seated
        })
    }
}

/// La ubicación de un hueso en el espacio del modelo: sin escala (los huesos de
/// `game_engine` no la tienen).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub rotation: Quat,
    pub translation: Vec3,
}

impl Place {
    pub const IDENTITY: Self = Self {
        rotation: Quat::IDENTITY,
        translation: Vec3::ZERO,
    };

    /// La ubicación de un hijo cuyo transform local es `local`.
    pub fn then(&self, local: &Transform) -> Self {
        Self {
            rotation: (self.rotation * local.rotation).normalize(),
            translation: self.translation + self.rotation * local.translation,
        }
    }
}

/// Una cadena de dos huesos (`a` la articulación raíz, `b` la del medio, `c` el
/// extremo) llevada a alcanzar `target`, doblándose hacia `pole`: los giros, en
/// el espacio de la cadena, que llevan el hueso superior (`upper`) y después el
/// inferior (`lower`, después del del superior) a sus nuevos lugares. Cada
/// hueso conserva su largo; el hueso inferior solo pivota alrededor del eje del
/// pliegue, así que la rodilla o el codo nunca se tuercen. Un objetivo fuera de
/// alcance se alcanza hasta donde llega la cadena.
pub fn two_bone(a: Vec3, b: Vec3, c: Vec3, target: Vec3, pole: Vec3) -> (Quat, Quat) {
    let (l1, l2) = (a.distance(b), b.distance(c));
    let to = target - a;
    let reach = to.length().clamp((l1 - l2).abs() + 1e-4, l1 + l2 - 1e-4);
    let dir = to.normalize_or(c - a);
    let side = pole - dir * pole.dot(dir);
    let side = side.normalize_or(dir.any_orthonormal_vector());
    let cos = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let knee = a + (dir * cos + side * sin) * l1;
    let end = a + dir * reach;
    // El hueso superior gira para que coincidan tanto su línea como el plano de
    // su pliegue.
    let frame = |u: Vec3, n: Vec3| Mat3::from_cols(u, n, u.cross(n));
    let (u0, u1) = ((b - a).normalize(), (knee - a).normalize());
    let (n0, n1) = ((b - a).cross(c - b), (knee - a).cross(end - knee));
    let upper = match (n0.try_normalize(), n1.try_normalize()) {
        (Some(n0), Some(n1)) => {
            let (n0, n1) = (
                (n0 - u0 * n0.dot(u0)).normalize(),
                (n1 - u1 * n1.dot(u1)).normalize(),
            );
            Quat::from_mat3(&(frame(u1, n1) * frame(u0, n0).transpose())).normalize()
        }
        _ => Quat::from_rotation_arc(u0, u1),
    };
    let lower = Quat::from_rotation_arc((upper * (c - b)).normalize(), (end - knee).normalize());
    (upper, lower)
}

/// Los huesos del jinete tal como los lee y los escribe [`pose_riders`].
type Bones<'w, 's> = Query<'w, 's, &'static mut Transform>;

/// Dobla `upper`, `lower` y `end` (hijos de un hueso ubicado en `parent`) para
/// que el extremo alcance `target`, doblándose hacia `pole`; el extremo
/// conserva el giro de su ubicación tomado del clip. No devuelve nada: escribe
/// los huesos.
fn reach(
    transforms: &mut Bones,
    parent: Place,
    [upper, lower, end]: [Entity; 3],
    target: Vec3,
    pole: Vec3,
) {
    let (Ok(u), Ok(l), Ok(e)) = (
        transforms.get(upper).copied(),
        transforms.get(lower).copied(),
        transforms.get(end).copied(),
    ) else {
        return;
    };
    let pu = parent.then(&u);
    let pl = pu.then(&l);
    let pe = pl.then(&e);
    let (d1, d2) = two_bone(pu.translation, pl.translation, pe.translation, target, pole);
    let upper_turn = (d1 * pu.rotation).normalize();
    let lower_turn = (d2 * d1 * pl.rotation).normalize();
    if let Ok(mut t) = transforms.get_mut(upper) {
        t.rotation = (parent.rotation.inverse() * upper_turn).normalize();
    }
    if let Ok(mut t) = transforms.get_mut(lower) {
        t.rotation = (upper_turn.inverse() * lower_turn).normalize();
    }
    if let Ok(mut t) = transforms.get_mut(end) {
        t.rotation = (lower_turn.inverse() * pe.rotation).normalize();
    }
}

/// Un resorte con amortiguamiento crítico: un valor que se acerca suavemente a
/// un objetivo sin pasarse, igual sin importar cómo se divida el tiempo en
/// frames.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
}

impl Spring {
    /// Avanza `dt` segundos hacia `target`, a la frecuencia angular `omega`
    /// (unos 4.7/`omega` segundos para asentarse); devuelve el valor.
    pub fn step(&mut self, target: f32, omega: f32, dt: f32) -> f32 {
        let dt = dt.max(0.0);
        let y = self.value - target;
        let c = self.velocity + omega * y;
        let e = (-omega * dt).exp();
        self.value = target + (y + c * dt) * e;
        self.velocity = (self.velocity - omega * c * dt) * e;
        self.value
    }

    /// En `value`, quieto.
    pub fn at(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
        }
    }
}

/// El movimiento secundario de un jinete: los resortes y relojes de sus capas,
/// en la raíz del modelo.
#[derive(Component, Debug, Clone, Default)]
pub struct RiderMotion {
    /// La altura, el cabeceo y el rolido de la montura, suavizados.
    pub saddle: [Spring; 3],
    /// El contracabeceo y el rolido de la columna, y los de la cabeza, con
    /// retraso.
    pub counter: [Spring; 2],
    pub head: Spring,
    pub lean: Spring,
    pub braking: Spring,
    /// Qué tan metido está en el medio asiento (`0..1`), acercándose suavemente
    /// con el galope.
    pub half: Spring,
    /// Cómo tenía la cabeza el jinete cuando se sentó (su cabeceo en el marco
    /// del cuerpo del caballo): la mantiene así.
    pub head0: f32,
    pub head_primed: bool,
    /// Qué tan asentadas están las manos (`0..1`), después de estar quieto.
    pub rest: Spring,
    /// El freno, suavizado, en el espacio del jinete.
    pub bit: [Spring; 3],
    /// La mano libre, con retraso, en el espacio del jinete.
    pub free: [Spring; 3],
    /// El encogimiento de los hombros.
    pub shrug: Spring,
    /// Cada brazo, izquierdo y derecho, tal como se sentó: dónde está su hombro
    /// en el espacio del jinete y cuánto mide hasta la muñeca. Las manos van a
    /// sus destinos medidos sobre el protagonista ([`REF_SHOULDERS`],
    /// [`REF_ARM`]) llevados a estos hombros y a este largo.
    pub arms: [Option<(Vec3, f32)>; 2],
    /// Qué tan plegado hacia adelante está sobre un salto (`0..1`), acercándose
    /// suavemente.
    pub fold: Spring,
    /// Si el caballo estaba en el aire en el frame anterior.
    pub flying: bool,
    /// Cuánto tiempo pasó desde que aterrizó el caballo, en segundos (nada una
    /// vez que terminó el aterrizaje).
    pub landed: Option<f32>,
    /// El cabeceo de la montura a lo largo del caballo, suavizado en el suelo y
    /// retenido sobre un salto: el nivel contra el que mantiene el equilibrio.
    pub level: Spring,
    /// Cuánto de su equilibrio de salto está activo (`0..1`, no una altura):
    /// completo desde el despegue, y se desvanece una vez que aterrizó.
    pub poise: Spring,
    /// La semilla del jinete, para sus propios momentos.
    pub seed: u32,
    pub speed: f32,
    pub still: f32,
    pub clock: f32,
    pub primed: bool,
    /// Cuánto gira la cabeza una persona sentada en un banco hacia lo que
    /// mira ([`Gaze`]), acercándose suavemente.
    pub gaze: Spring,
}

/// Qué tan rápido gira la cabeza una persona sentada hacia lo que mira
/// ([`Gaze`]): la frecuencia angular de su resorte (se asienta en algo menos
/// de un segundo).
pub const GAZE_EASE: f32 = 6.0;

/// Lo que un jinete lee de su caballo: las cadenas desde la raíz del modelo
/// hasta su `Torso` y su `Head` (las ubicaciones en el marco del cuerpo de la
/// montura y del freno), y el centro del freno en la cabeza. En el cuerpo del
/// caballo.
#[derive(Component, Debug, Clone)]
pub struct Saddle {
    pub torso: Vec<Entity>,
    /// Desde la raíz del modelo hasta la grupa ([`Tack::croup`]), de la que
    /// cuelga quien va en ancas.
    pub croup: Vec<Entity>,
    pub head: Vec<Entity>,
    pub bit: Vec3,
    /// The `Torso` bone's turn at rest in the body frame: the saddle's level
    /// ([`saddle_attitude`]).
    pub rest: Quat,
}

/// La ubicación del último elemento de `chain` (desde la raíz del modelo del
/// caballo hacia abajo), en el marco del cuerpo del caballo.
fn chain_affine(chain: &[Entity], transforms: &Bones) -> Affine3A {
    chain.iter().fold(Affine3A::IDENTITY, |at, e| {
        at * transforms
            .get(*e)
            .map_or(Affine3A::IDENTITY, |t| t.compute_affine())
    })
}

/// Una rotación `r` dada en el marco del cuerpo del caballo, como una rotación
/// en el espacio del jinete (`frame`: el giro del jinete en el marco del
/// cuerpo).
fn into_rider(r: Quat, frame: Quat) -> Quat {
    frame.inverse() * r * frame
}

/// Gira `bone` (hijo de un hueso ubicado en `parent`) por `turn`, una rotación
/// en el espacio del jinete.
fn turn_bone(transforms: &mut Bones, parent: Place, bone: Entity, turn: Quat) {
    if let Ok(mut t) = transforms.get_mut(bone) {
        t.rotation = (parent.rotation.inverse() * turn * parent.rotation * t.rotation).normalize();
    }
}

/// The saddle's pitch (positive nose up) and roll (positive right side up),
/// in radians, from the `Torso` bone's turn in the horse's body frame (`tilt`)
/// and its turn at rest (`rest`, where the rider sat down): measured on the
/// body's own axes (+X forward, +Z the horse's right), so it does not depend
/// on how the bone's own axes run (henry's `Spine_01` runs its +Y forward, its
/// +X to the horse's left and its +Z down).
pub fn saddle_attitude(tilt: Quat, rest: Quat) -> (f32, f32) {
    let turn = tilt * rest.inverse();
    (
        (turn * Vec3::X).y.clamp(-1.0, 1.0).asin(),
        (turn * Vec3::Z).y.clamp(-1.0, 1.0).asin(),
    )
}

/// Qué tanto se paró una persona sentada en un banco
/// ([`CharacterVisual::Seated`](super::visual::CharacterVisual::Seated)), de 0
/// (sentada) a 1 (de pie junto al banco). Va en la entidad de la persona; el
/// juego lo avanza. Mientras se para, su modelo pasa del clip sentado al de la
/// espera de pie, su pelvis sube y sus pies siguen apoyados en el piso.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct Rising(pub f32);

/// A qué altura sobre la parte de arriba del banco va el origen del modelo de
/// alguien que se paró `rise` ([`Rising`]): sentado, con la pelvis
/// [`PELVIS_RISE`] por encima; de pie, en el piso, como una figura a pie.
pub fn bench_lift(rise: f32) -> f32 {
    PELVIS_RISE + (-BENCH_HEIGHT - PELVIS_RISE) * rise.clamp(0.0, 1.0)
}

/// Sienta al conductor de un vehículo (ver [`vehicle`](crate::vehicle)): su
/// pelvis en el origen del modelo, puesto sobre el asiento como un jinete
/// ([`seat_transform`]). En un banco ([`OnBench`]) además baja los pies hasta
/// el piso, [`BENCH_HEIGHT`] por debajo del asiento: el clip se sienta más
/// alto, y sus pies colgarían. Al pararse (`rise`, ver [`Rising`]) la pelvis
/// pasa de ese lugar al que le da el clip, el modelo baja con [`bench_lift`]
/// y el piso queda donde estaba.
fn sit_on_bench(transforms: &mut Bones, seated: &Seated, on_bench: bool, rise: f32) {
    let Some(root) = transforms.get(seated.bone(Bone::Root)).ok().copied() else {
        return;
    };
    if let Ok(mut pelvis) = transforms.get_mut(seated.bone(Bone::Pelvis)) {
        // Dónde la pone el clip, en el espacio del modelo; sentada va en su
        // origen. Al pararse sube pareja en el espacio del asiento, de
        // PELVIS_RISE a la altura del clip sobre el modelo ya bajado.
        let clip = root.translation + root.rotation * pelvis.translation;
        let lift = bench_lift(rise);
        let height = PELVIS_RISE + (lift + clip.y - PELVIS_RISE) * rise;
        let target = Vec3::new(clip.x * rise, height - lift, clip.z * rise);
        pelvis.translation = root.rotation.inverse() * (target - root.translation);
    }
    if !on_bench {
        return;
    }
    let get = |transforms: &Bones, bone: Bone| transforms.get(seated.bone(bone)).ok().copied();
    let root = Place {
        rotation: root.rotation,
        translation: root.translation,
    };
    let Some(pelvis) = get(transforms, Bone::Pelvis) else {
        return;
    };
    let pelvis = root.then(&pelvis);
    // El piso, en el espacio del modelo (sentado, su origen es la pelvis).
    let floor = -(bench_lift(rise) + BENCH_HEIGHT);
    for (s, pole) in BENCH_KNEE_POLE.into_iter().enumerate() {
        let (Some(thigh), Some(calf), Some(foot), Some(ball)) = (
            get(transforms, Bone::Thigh(s)),
            get(transforms, Bone::Calf(s)),
            get(transforms, Bone::Foot(s)),
            get(transforms, Bone::Ball(s)),
        ) else {
            continue;
        };
        let ankle = pelvis.then(&thigh).then(&calf).then(&foot);
        let drop = ankle.then(&ball).translation.y - (floor + seated.ball_rise[s]);
        if drop > 0.0 {
            let target = ankle.translation - Vec3::Y * drop;
            let bones = [Bone::Thigh(s), Bone::Calf(s), Bone::Foot(s)].map(|b| seated.bone(b));
            reach(transforms, pelvis, bones, target, pole);
        }
    }
}

/// Sienta a horcajadas a quien va en ancas ([`Behind`]), con la pelvis ya
/// sobre su asiento girado `tilt` (en el marco del cuerpo del caballo): se
/// mantiene derecho en las caderas contra lo que cabecea y rola la grupa
/// ([`PILLION_LEVEL`]), y las piernas cuelgan a los costados
/// ([`PILLION_ANKLES`]), las rodillas hacia afuera ([`PILLION_KNEE_POLE`]).
/// Con los hombros en su lugar (el clip los echa atrás), se agarra del
/// jinete: las muñecas van a `hold` (espacio del modelo), su faja
/// ([`PILLION_HOLD`]); sin jinete, las manos quedan en el regazo, como en el
/// clip. Pegado a su espalda, se dobla hacia la derecha en el pecho y asoma
/// la cara junto a su hombro ([`PILLION_ASIDE`]).
fn straddle(transforms: &mut Bones, seated: &Seated, tilt: Quat, hold: Option<[Vec3; 2]>) {
    let get = |transforms: &Bones, bone: Bone| transforms.get(seated.bone(bone)).ok().copied();
    let Some(root) = get(transforms, Bone::Root) else {
        return;
    };
    // Lo que la grupa lo saca de la vertical, deshecho en su mayor parte, en
    // el espacio del modelo (girado `pillion_transform` sobre el asiento).
    let model = tilt * Quat::from_rotation_y(FRAC_PI_2);
    let upright = Quat::from_rotation_arc(tilt * Vec3::Y, Vec3::Y);
    let level = into_rider(Quat::IDENTITY.slerp(upright, PILLION_LEVEL), model);
    if let Ok(mut pelvis) = transforms.get_mut(seated.bone(Bone::Pelvis)) {
        pelvis.rotation =
            (root.rotation.inverse() * level * root.rotation * pelvis.rotation).normalize();
    }
    let Some(pelvis) = get(transforms, Bone::Pelvis) else {
        return;
    };
    let pelvis = Place::IDENTITY.then(&root).then(&pelvis);
    for s in 0..2 {
        let leg = [Bone::Thigh(s), Bone::Calf(s), Bone::Foot(s)].map(|b| seated.bone(b));
        reach(
            transforms,
            pelvis,
            leg,
            PILLION_ANKLES[s],
            PILLION_KNEE_POLE[s],
        );
    }
    // Doblado hacia su derecha en el pecho.
    let (Some(spine1), Some(spine2)) =
        (get(transforms, Bone::Spine1), get(transforms, Bone::Spine2))
    else {
        return;
    };
    let spine2 = pelvis.then(&spine1).then(&spine2);
    turn_bone(
        transforms,
        spine2,
        seated.bone(Bone::Spine3),
        Quat::from_rotation_z(PILLION_ASIDE.0),
    );
    let Some(spine3) = get(transforms, Bone::Spine3) else {
        return;
    };
    let chest = spine2.then(&spine3);
    for s in 0..2 {
        if let Ok(mut t) = transforms.get_mut(seated.bone(Bone::Clavicle(s))) {
            t.rotation = seated.clavicles[s];
        }
    }
    // Cada hombro y el largo de su brazo, del hombro a la muñeca.
    let arm = |transforms: &Bones, chest: Place, s: usize| {
        let (clavicle, upper) = (
            get(transforms, Bone::Clavicle(s))?,
            get(transforms, Bone::UpperArm(s))?,
        );
        let (lower, wrist) = (
            get(transforms, Bone::LowerArm(s))?,
            get(transforms, Bone::Hand(s))?,
        );
        let shoulder = chest.then(&clavicle);
        let length = lower.translation.length() + wrist.translation.length();
        Some((shoulder, shoulder.then(&upper).translation, length))
    };
    for s in 0..2 {
        let Some(hold) = hold else {
            continue;
        };
        let Some((shoulder, joint, length)) = arm(transforms, chest, s) else {
            continue;
        };
        let target = within_reach(hold[s], joint, length, REACH.1);
        let bones = [Bone::UpperArm(s), Bone::LowerArm(s), Bone::Hand(s)].map(|b| seated.bone(b));
        reach(transforms, shoulder, bones, target, PILLION_ELBOW_POLE[s]);
    }
    turn_head(transforms, seated, -PILLION_ASIDE.1);
}

/// Gira el cuello y la cabeza de una persona sentada `turn` radianes alrededor
/// del eje vertical del modelo (positivo hacia la izquierda), la mitad cada
/// uno, por encima del clip.
fn turn_head(transforms: &mut Bones, seated: &Seated, turn: f32) {
    if turn.abs() < 1e-4 {
        return;
    }
    let get = |transforms: &Bones, bone: Bone| transforms.get(seated.bone(bone)).ok().copied();
    let Some(root) = get(transforms, Bone::Root) else {
        return;
    };
    let mut chest = Place {
        rotation: root.rotation,
        translation: root.translation,
    };
    for bone in [Bone::Pelvis, Bone::Spine1, Bone::Spine2, Bone::Spine3] {
        if let Some(t) = get(transforms, bone) {
            chest = chest.then(&t);
        }
    }
    let half = Quat::from_rotation_y(turn / 2.0);
    turn_bone(transforms, chest, seated.bone(Bone::Neck), half);
    if let Some(neck) = get(transforms, Bone::Neck) {
        turn_bone(transforms, chest.then(&neck), seated.bone(Bone::Head), half);
    }
}

/// Qué tanto se paró ([`Rising`]) la persona dueña del banco `seat` (el
/// asiento cuelga de su entidad); 0 si sigue sentada.
pub(super) fn rise_of(seat: Entity, parents: &Query<&ChildOf>, risers: &Query<&Rising>) -> f32 {
    parents
        .get(seat)
        .ok()
        .and_then(|person| risers.get(person.parent()).ok())
        .map_or(0.0, |r| r.0.clamp(0.0, 1.0))
}

/// Lo que [`pose_riders`] lee de cada jinete o persona sentada.
type Sitter = (
    Entity,
    &'static Rig,
    &'static Riding,
    &'static Seated,
    &'static mut RiderMotion,
    &'static mut Visibility,
    Has<OnBench>,
    Option<&'static Behind>,
);

/// Ajusta cada jinete que se reproduce a su caballo (ver las notas del módulo),
/// después de que los clips lo pusieron en pose (a él y al caballo) y antes de
/// que se calculen los transforms.
///
/// While a rider is mounted this system owns the end of the horse's right
/// rein (`Strap::ends[1]` of `Tack::reins[1]`): every frame it puts it
/// where [`rein_end`] says (the left hand, or the right one in the
/// half-seat). Anything that later hands that rein elsewhere (dismounting,
/// leading the horse by it) must first take the rider off the horse, or this
/// system overwrites it on the next frame.
#[allow(clippy::too_many_arguments)]
pub(super) fn pose_riders(
    time: Res<Time>,
    clips: Option<Res<CharacterClips>>,
    horses: Query<(&HorseBody, &Saddle, Has<PillionSeat>)>,
    (tacks, mut straps): (Query<&Tack>, Query<&mut Strap>),
    mut riders: Query<Sitter>,
    (parents, risers, gazes): (Query<&ChildOf>, Query<&Rising>, Query<&Gaze>),
    mut transforms: Bones,
    mut fronts: Local<Vec<(Entity, Entity, [Entity; 3])>>,
) {
    let dt = time.delta_secs();
    // Los jinetes de los caballos ensillados, de los que se agarra quien va
    // en ancas: el caballo, el modelo y sus huesos hasta `spine_01`.
    fronts.clear();
    fronts.extend(
        riders
            .iter()
            .filter(|(.., behind)| behind.is_none())
            .filter(|(_, _, riding, ..)| horses.contains(riding.horse))
            .map(|(me, _, riding, seated, ..)| {
                let waist = [Bone::Root, Bone::Pelvis, Bone::Spine1].map(|b| seated.bone(b));
                (riding.horse, me, waist)
            }),
    );
    // Primero los jinetes y después quienes van en ancas, que se agarran de
    // ellos ya puestos en pose en este frame.
    for pass in [false, true] {
        for (me, rig, riding, seated, mut m, mut shown, on_bench, behind) in &mut riders {
            if behind.is_some() != pass {
                continue;
            }
            if rig.frozen || !rig.started {
                continue;
            }
            // En pose a partir de aquí: se dibuja.
            shown.set_if_neq(Visibility::Inherited);
            let Ok((horse, saddle, doubled)) = horses.get(riding.horse) else {
                // En el pescante de un vehículo: la pelvis en el origen del
                // modelo, sobre el asiento, y el resto como lo pone el clip.
                let rise = if on_bench {
                    rise_of(riding.horse, &parents, &risers)
                } else {
                    0.0
                };
                if rise > 0.0 {
                    if let Ok(mut model) = transforms.get_mut(me) {
                        model.translation.y = bench_lift(rise);
                    }
                    // De pie, la pelvis a la altura de sus propias piernas, como
                    // la de una figura a pie (ver `fit_rigs`).
                    let reference = clips.as_deref().and_then(|c| c.reference);
                    if let (Some(reference), Some((pelvis, rest, leg))) = (reference, rig.pelvis)
                        && let Ok(mut t) = transforms.get_mut(pelvis)
                    {
                        t.translation = fit_pelvis(t.translation, reference, rest, leg);
                    }
                }
                sit_on_bench(&mut transforms, seated, on_bench, rise);
                if let Some(&Behind(carrier)) = behind {
                    // Su asiento y la cintura del jinete en el marco del cuerpo
                    // del caballo, con la pose de este frame: cómo lo gira la
                    // grupa, y adónde van sus manos (en el espacio de su modelo).
                    let saddle = horses.get(carrier).ok().map(|(_, saddle, _)| saddle);
                    let local = |e: Entity| transforms.get(e).ok().map(|t| t.compute_affine());
                    let seat = saddle.and_then(|saddle| {
                        Some(chain_affine(&saddle.croup, &transforms) * local(riding.horse)?)
                    });
                    let (_, tilt, _) = seat.unwrap_or_default().to_scale_rotation_translation();
                    let hold = seat.zip(saddle).and_then(|(seat, saddle)| {
                        let &(_, model, [root, pelvis, spine]) =
                            fronts.iter().find(|f| f.0 == carrier)?;
                        let front = chain_affine(&saddle.torso, &transforms) * local(model)?;
                        let waist = Place::IDENTITY
                            .then(transforms.get(root).ok()?)
                            .then(transforms.get(pelvis).ok()?)
                            .then(transforms.get(spine).ok()?)
                            .translation;
                        let mine = (seat * local(me)?).inverse();
                        Some(
                            PILLION_HOLD
                                .map(|h| mine.transform_point3(front.transform_point3(waist + h))),
                        )
                    });
                    straddle(&mut transforms, seated, tilt, hold);
                }
                if on_bench {
                    // Gira la cabeza hacia lo que mira, si mira algo.
                    let want = parents
                        .get(riding.horse)
                        .ok()
                        .and_then(|person| gazes.get(person.parent()).ok())
                        .map_or(0.0, |g| g.0.clamp(-GAZE_MAX, GAZE_MAX));
                    let turn = m.gaze.step(want, GAZE_EASE, dt);
                    turn_head(&mut transforms, seated, turn);
                }
                continue;
            };
            let (Some(seat), Some(root)) = (
                transforms.get(me).ok().copied(),
                transforms.get(seated.bone(Bone::Root)).ok().copied(),
            ) else {
                continue;
            };
            // La montura en el marco del cuerpo, y el jinete en ella.
            let torso = chain_affine(&saddle.torso, &transforms);
            let rider = torso * seat.compute_affine();
            let (_, frame, _) = rider.to_scale_rotation_translation();
            let (_, tilt, _) = torso.to_scale_rotation_translation();
            // La altura del asiento (donde se apoya la pelvis), y el cabeceo y el
            // rolido de la montura.
            let (saddle_pitch, saddle_roll) = saddle_attitude(tilt, saddle.rest);
            let now = [rider.translation.y, saddle_pitch, saddle_roll];
            let head = chain_affine(&saddle.head, &transforms);
            let bit = rider
                .inverse()
                .transform_point3(head.transform_point3(saddle.bit));
            if !m.primed {
                m.saddle = now.map(Spring::at);
                m.level = Spring::at(saddle_pitch);
                m.bit = bit.to_array().map(Spring::at);
                m.free = THIGH_HAND.to_array().map(Spring::at);
                m.seed = riding.seed;
                m.speed = horse.speed;
                m.primed = true;
            }
            m.clock = pampa_core::tick_clock(m.clock, dt);
            // Un salto: plegado hacia adelante desde el despegue y en lo alto, y
            // después enderezándose y absorbiendo el aterrizaje en las rodillas y
            // las caderas.
            let fold = m
                .fold
                .step(horse.flight.map_or(0.0, jump_fold), JUMP_EASE, dt)
                .clamp(0.0, 1.0);
            m.landed = match (m.flying, horse.flight) {
                (_, Some(_)) => None,
                (true, None) => Some(0.0),
                (false, None) => m.landed.map(|t| t + dt).filter(|t| *t < LANDING),
            };
            m.flying = horse.flight.is_some();
            let give = m.landed.map_or(0.0, landing);
            // Los clips del andar se detienen en el aire: el medio asiento se
            // mantiene durante el salto y su aterrizaje.
            let jumping = f32::from(m.flying || m.landed.is_some());
            // Lo que hace la montura más allá de su trayectoria suavizada.
            let mut off = [0.0; 3];
            for k in 0..3 {
                let smooth = m.saddle[k].step(now[k], SADDLE_EASE, dt);
                off[k] = now[k] - smooth;
            }
            // Sobre un salto mantiene el equilibrio cabecee como cabecee el
            // caballo: su cabeceo desde que dejó el suelo (ver [`saddle_attitude`])
            // se absorbe en las caderas, entrando y
            // saliendo de forma gradual.
            let level = if jumping > 0.0 {
                m.level.value
            } else {
                m.level.step(saddle_pitch, SADDLE_EASE, dt)
            };
            // Enseguida al despegar (el caballo todavía no cabeceó), y se desvanece
            // una vez que aterrizó.
            if jumping > 0.0 {
                m.poise = Spring::at(1.0);
            }
            let poise = m.poise.step(0.0, JUMP_EASE, dt).clamp(0.0, 1.0);
            let balance = JUMP_BALANCE * poise * (saddle_pitch - level);
            let [rise, pitch, roll] = off;
            // Velocidad, frenado y quietud.
            let accel = if dt > 0.0 {
                (horse.speed - m.speed) / dt
            } else {
                0.0
            };
            m.speed = horse.speed;
            let braking = m
                .braking
                .step((-accel / BRAKING).clamp(0.0, 1.0), BRAKING_EASE, dt);
            m.still = if horse.speed < STANDING {
                m.still + dt
            } else {
                0.0
            };
            let settled = f32::from(m.still > STILL);
            let rest = m.rest.step(settled, REST_EASE, dt).clamp(0.0, 1.0);
            let gallop = horse.gallop.max(jumping).clamp(0.0, 1.0);
            // El medio asiento entra con el galope, con retraso. Con alguien en
            // ancas agarrado de su cintura no se para en los estribos: galopa
            // sentado (salvo sobre un salto).
            let rising = if doubled { jumping } else { gallop };
            let half = m.half.step(rising, HALF_SEAT_EASE, dt).clamp(0.0, 1.0);
            let walking = (horse.speed / WALK_TOP).min(1.0) * (1.0 - gallop);
            let lean = m.lean.step(
            LEAN.0 * half + LEAN.1 * walking - LEAN.2 * braking + SIT_UP * (1.0 - half),
            LEAN_EASE,
            dt,
        ) + JUMP_FOLD * fold
            - LANDING_GIVE.0 * give
            + balance
            // Balanced over his own feet, not over the saddle: the saddle's
            // smoothed pitch (henry's gallop carries the back ~8° nose down)
            // is taken back at the hips; its quick nods go to the spine.
            + level;
            // Sobre un salto las caderas absorben el cabeceo (`balance`), no la
            // columna.
            let counter_pitch =
                m.counter[0].step(COUNTER.0 * pitch * (1.0 - poise), COUNTER_EASE, dt);
            let counter_roll = m.counter[1].step(COUNTER.1 * roll, COUNTER_EASE, dt);
            let slack = Vec3::new(
                bit.x - m.bit[0].step(bit.x, BIT_EASE, dt),
                bit.y - m.bit[1].step(bit.y, BIT_EASE, dt),
                bit.z - m.bit[2].step(bit.z, BIT_EASE, dt),
            );
            let get = |t: &Bones, b: Bone| t.get(seated.bone(b)).copied().ok();
            let root = Place::IDENTITY.then(&root);
            // El asiento. Al paso se sienta hondo en el recado; en medio
            // asiento flota por encima de él, adelantado sobre las rodillas, sin
            // acompañar el rebote de la montura: lo absorben las piernas. Nunca
            // por debajo del asiento.
            // Al aterrizar las rodillas y las caderas ceden, y el asiento baja
            // hasta la montura (nunca dentro de ella).
            let hover = (HALF_SEAT.y - rise).clamp(0.0, HALF_SEAT.y * 2.0)
                * (1.0 - LANDING_GIVE.1 * give).max(0.0);
            let seat = Vec3::new(0.0, half * hover, half * HALF_SEAT.z);
            let period = SHIFT.0 + (riding.seed % SHIFT.1) as f32;
            let shift = Quat::from_rotation_z(
                SHIFT.2 * rest * (std::f32::consts::TAU * m.clock / period).sin(),
            );
            // Una espalda recta: la inclinación está sobre todo en las caderas.
            let tip = Quat::from_rotation_x(HIP_SHARE * lean) * shift;
            if let Ok(mut pelvis) = transforms.get_mut(seated.bone(Bone::Pelvis)) {
                pelvis.translation = root.rotation.inverse() * (seat - root.translation);
                pelvis.rotation =
                    (root.rotation.inverse() * tip * root.rotation * pelvis.rotation).normalize();
            }
            let Some(pelvis) = get(&transforms, Bone::Pelvis) else {
                continue;
            };
            let pelvis = root.then(&pelvis);
            // La columna: el resto de la inclinación, compensando la montura con
            // retraso.
            let counter = |share: f32| {
                into_rider(
                    Quat::from_rotation_z(-share * counter_pitch)
                        * Quat::from_rotation_x(share * counter_roll),
                    frame,
                )
            };
            turn_bone(
                &mut transforms,
                pelvis,
                seated.bone(Bone::Spine1),
                Quat::from_rotation_x((1.0 - HIP_SHARE) * lean) * counter(0.6),
            );
            let Some(spine1) = get(&transforms, Bone::Spine1) else {
                continue;
            };
            let spine1 = pelvis.then(&spine1);
            let flex = (FLEX * rise * (1.0 - half)).clamp(-FLEX_MAX, FLEX_MAX);
            turn_bone(
                &mut transforms,
                spine1,
                seated.bone(Bone::Spine2),
                Quat::from_rotation_x(flex) * counter(0.4),
            );
            let mut chest = spine1;
            for b in [Bone::Spine2, Bone::Spine3] {
                if let Some(t) = get(&transforms, b) {
                    chest = chest.then(&t);
                }
            }
            // La cabeza nivelada y mirando hacia adelante (como la tenía al
            // sentarse), estabilizada con un leve retraso, y mirando a su alrededor
            // estando quieto.
            let (Some(neck), Some(skull)) =
                (get(&transforms, Bone::Neck), get(&transforms, Bone::Head))
            else {
                continue;
            };
            let up = frame * chest.then(&neck).then(&skull).rotation * Vec3::Y;
            let nodding = up.x.atan2(up.y);
            if !m.head_primed {
                m.head0 = nodding;
                m.head_primed = true;
            }
            let off_level = nodding - m.head0;
            // La mayor parte enseguida y el resto con un leve retraso: estable, no
            // rígida.
            let level = 0.85 * off_level + 0.15 * m.head.step(off_level, HEAD_EASE, dt);
            let glance = 0.5 * rest * pampa_core::pose::glance(riding.seed, m.clock);
            let nod = |share: f32| {
                Quat::from_rotation_y(glance)
                    * into_rider(Quat::from_rotation_z(share * level), frame)
            };
            turn_bone(&mut transforms, chest, seated.bone(Bone::Neck), nod(0.5));
            if let Some(neck) = get(&transforms, Bone::Neck) {
                turn_bone(
                    &mut transforms,
                    chest.then(&neck),
                    seated.bone(Bone::Head),
                    nod(0.5),
                );
            }
            // Las piernas hacia los estribos, con los talones abajo en medio
            // asiento y más a medida que el asiento baja sobre ellos.
            let heel = Quat::from_rotation_x(-half * (HEEL + 2.0 * (HALF_SEAT.y - hover).max(0.0)));
            for s in 0..2 {
                let leg = [Bone::Thigh(s), Bone::Calf(s), Bone::Foot(s)].map(|b| seated.bone(b));
                let ankle = ANKLES[s].lerp(GALLOP_ANKLES[s], half);
                reach(&mut transforms, pelvis, leg, ankle, KNEE_POLE[s]);
                if let (Some(thigh), Some(calf)) = (
                    get(&transforms, Bone::Thigh(s)),
                    get(&transforms, Bone::Calf(s)),
                ) {
                    let shin = pelvis.then(&thigh).then(&calf);
                    turn_bone(&mut transforms, shin, seated.bone(Bone::Foot(s)), heel);
                }
            }
            // La mano de las riendas sigue la boca del caballo (más en medio
            // asiento: los codos se abren cuando la cabeza va hacia adelante y
            // hacia abajo); la mano libre descansa junto al muslo, acompañando al
            // asiento.
            let follow = FOLLOW.0 + (FOLLOW.1 - FOLLOW.0) * half;
            // Folded over a jump the hand gives rein along the crest
            // ([`JUMP_HAND`]) instead of following the mouth: henry lifts his head
            // up and back over the top, and following it would pull the hand back.
            let slack = slack.clamp_length_max(GIVE_MAX) * follow * (1.0 - rest) * (1.0 - fold);
            let reins = REINS_HAND
                .lerp(GALLOP_HANDS[0], half)
                .lerp(POMMEL_HAND, rest);
            let free = free_hand(
                &mut m,
                (rise, pitch, roll),
                (walking, gallop, braking, rest),
                dt,
            );
            // Going into the half-seat the right hand comes to the left one and
            // takes its rein (half way in), then both part to either side of the
            // crest; coming out, the other way round.
            let meet = REINS_HAND.lerp(GALLOP_HANDS[0], 0.5) + MEET;
            let right = if half < 0.5 {
                (seat + free).lerp(meet, half * 2.0)
            } else {
                meet.lerp(GALLOP_HANDS[1], half * 2.0 - 1.0)
            };
            // Over a jump both hands give rein forward along the crest.
            let release = slack + JUMP_HAND * fold;
            let hands = [
                reins + release,
                right + release * (2.0 * half - 1.0).max(0.0),
            ];
            // The right rein in the right hand once both hands are on the reins.
            // Checked every frame (written only when it differs), so a missed
            // frame or a scene created again never leaves it in the wrong hand.
            if let Ok(tack) = tacks.get(riding.horse)
                && let Ok(mut rein) = straps.get_mut(tack.reins[1])
            {
                let end = rein_end(seated, 1, half > 0.5);
                if rein.ends[1] != end {
                    rein.ends[1] = end;
                }
            }
            // Los hombros acompañan al torso, con retraso: hacia arriba cuando la
            // montura baja, hacia adelante cuando cabecea hacia abajo.
            let shrug = m.shrug.step(SHRUG * (pitch - 2.0 * rise), SHRUG_EASE, dt);
            // Where the left (rein) hand goes, once worked out: the right one
            // meets it there.
            let mut left_hand = None;
            for s in 0..2 {
                let side = if s == 0 { 1.0 } else { -1.0 };
                if let Ok(mut t) = transforms.get_mut(seated.bone(Bone::Clavicle(s))) {
                    t.rotation = seated.clavicles[s];
                }
                turn_bone(
                    &mut transforms,
                    chest,
                    seated.bone(Bone::Clavicle(s)),
                    Quat::from_rotation_x(0.5 * shrug) * Quat::from_rotation_z(side * shrug),
                );
                let Some(clavicle) = get(&transforms, Bone::Clavicle(s)) else {
                    continue;
                };
                let shoulder = chest.then(&clavicle);
                let (Some(upper), Some(lower), Some(wrist)) = (
                    get(&transforms, Bone::UpperArm(s)),
                    get(&transforms, Bone::LowerArm(s)),
                    get(&transforms, Bone::Hand(s)),
                ) else {
                    continue;
                };
                let fit = *m.arms[s].get_or_insert((
                    shoulder.then(&upper).translation,
                    lower.translation.length() + wrist.translation.length(),
                ));
                let arm =
                    [Bone::UpperArm(s), Bone::LowerArm(s), Bone::Hand(s)].map(|b| seated.bone(b));
                let mut target =
                    clear_of_chest(fit_hand(hands[s], s, fit), chest.translation, half);
                match left_hand {
                    None => left_hand = Some(target),
                    Some(left) => target = meet_left(target, left, half),
                }
                reach(
                    &mut transforms,
                    shoulder,
                    arm,
                    within_reach(
                        target,
                        shoulder.then(&upper).translation,
                        fit.1,
                        REACH.0 + (REACH.1 - REACH.0) * half,
                    ),
                    ELBOW_POLE[s],
                );
            }
            // The free hand opens out of the reins clip's fist, closing again as
            // it comes to take the right rein going into the half-seat.
            let open = LOOSE_HAND * (1.0 - 2.0 * half).clamp(0.0, 1.0);
            let (wrist, fingers) = &seated.loose;
            for (bone, rest) in std::iter::once((seated.bone(Bone::Hand(1)), wrist))
                .chain(fingers.iter().map(|(bone, rest)| (*bone, rest)))
            {
                if let Ok(mut t) = transforms.get_mut(bone) {
                    t.rotation = t.rotation.slerp(*rest, open);
                }
            }
        }
    }
}

/// The right hand's target `right` drawn to the left hand's `left` (both
/// model space, fitted to this rider's arms) as it comes to take its rein:
/// all the way to [`MEET`] beside it half way into the half-seat (`half`
/// 0.5, where the rein changes hands), not at all sitting or in the full
/// half-seat. Fitting each hand to its own shoulder ([`fit_hand`]) and the
/// rein hand's give to the mouth moved the two apart: the rein's end jumped
/// 13-16 cm between the palms.
pub fn meet_left(right: Vec3, left: Vec3, half: f32) -> Vec3 {
    let pull = 1.0 - (2.0 * half - 1.0).abs();
    right.lerp(left + MEET, pull.clamp(0.0, 1.0))
}

/// Where rein `s` (0 the left) ends on the rider: both in the left hand, a
/// finger's width apart, or, with both hands on the reins (the half-seat),
/// each in its own hand.
pub fn rein_end(seated: &Seated, s: usize, two_hands: bool) -> (Entity, Vec3) {
    if two_hands && s == 1 {
        (seated.bone(Bone::Hand(1)), PALM)
    } else {
        let apart = Vec3::X * (if s == 0 { -0.012 } else { 0.012 });
        (seated.bone(Bone::Hand(0)), PALM + apart)
    }
}

/// Adónde va la mano libre (la derecha), desde donde está el asiento, en el
/// espacio del jinete: sobre su muslo, balanceándose con retraso con el caballo
/// (un poco al paso, más y elevándose al galope), hacia adelante hacia la
/// cabezada al frenar y, estando quieto, de vez en cuando hasta el ala del
/// sombrero. `motion` es la subida, el cabeceo y el rolido de la montura más
/// allá de sus trayectorias suavizadas; `gaits`, cuánto camina, galopa, frena y
/// se ha asentado.
pub fn free_hand(
    m: &mut RiderMotion,
    (rise, pitch, roll): (f32, f32, f32),
    (walking, gallop, braking, rest): (f32, f32, f32, f32),
    dt: f32,
) -> Vec3 {
    let [x, y, z] = FREE_SWING;
    let stride = Vec3::new(
        x[0] * rise + x[2] * roll,
        y[0] * rise + y[1] * pitch,
        z[1] * pitch + z[0] * rise,
    );
    let swing = stride * (FREE_WALK * walking + gallop);
    let brake = (POMMEL_HAND - THIGH_HAND) * FREE_BRAKE * braking;
    // All the way to the brim or not at all: scaled by the glance's amount
    // (0.3-0.55 of the widest) the hand often stopped half way, a fist
    // held in front of the chest for seconds, and rose over its slow ramp.
    let glance = pampa_core::pose::glance(m.seed ^ HAT_SALT, m.clock * HAT_PACE).abs();
    let up = ((glance - HAT_FROM) / (HAT_TO - HAT_FROM)).clamp(0.0, 1.0);
    let brim = up * up * (3.0 - 2.0 * up);
    let hat = (HAT_HAND - THIGH_HAND) * brim * rest;
    let target = THIGH_HAND + (swing + brake).clamp_length_max(FREE_REACH) + hat;
    // Con retraso respecto del torso: un brazo suelto, nunca rígido, nunca
    // agitándose.
    let at = [0, 1, 2].map(|k| m.free[k].step(target[k], FREE_EASE, dt));
    Vec3::from_array(at)
}

/// Sienta al jinete de cada caballo ensillado una vez puestos sus aperos: el
/// modelo del jinete sobre el hueso `Torso` en el asiento, esperando su escena,
/// y lo que el jinete lee del caballo ([`Saddle`]). Los jinetes sin modelo
/// montan una montura vacía.
pub(super) fn spawn_riders(
    mut commands: Commands,
    characters: Option<Res<CharacterModels>>,
    horses: Query<(Entity, &HorseBody, &Tack), Changed<Tack>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HorseModel>>,
) {
    for (carrier, body, tack) in &horses {
        // Desde la raíz del modelo del caballo hasta `bone`.
        let chain = |bone: Entity| {
            let mut chain = vec![bone];
            let mut at = bone;
            while !roots.contains(at) {
                let Ok(child_of) = parents.get(at) else {
                    break;
                };
                at = child_of.parent();
                chain.push(at);
            }
            chain.reverse();
            chain
        };
        commands.entity(carrier).try_insert(Saddle {
            torso: chain(tack.torso),
            croup: chain(tack.croup),
            head: chain(tack.head),
            bit: tack.bit,
            rest: tack.torso_rest.to_scale_rotation_translation().1,
        });
        let model = body
            .rider
            .and_then(|kind| Some((kind, characters.as_ref()?.of(kind)?.clone())));
        if let Some((kind, scene)) = model {
            commands.spawn((
                Name::new("Rider model"),
                CharacterModel,
                ModelPending(scene),
                Riding {
                    horse: carrier,
                    kind,
                    seed: body.seed,
                },
                transform_of(tack.torso_rest.inverse() * seat_transform(tack.seat)),
                // Se dibuja una vez sentado: su escena se crea parada en
                // reposo.
                Visibility::Hidden,
                ChildOf(tack.torso),
            ));
        }
    }
}

/// Sienta o baja a quien va en ancas ([`Pillion`]) en el caballo de cada
/// personaje montado: un asiento sobre la grupa ([`PILLION_SEAT`]) bajo su
/// hueso ([`Tack::croup`]), que la sigue en cada andar, y su modelo encima, oculto hasta que se
/// sienta. Lo vuelve a sentar si cambia quién es o si la escena del caballo
/// se creó de nuevo, y lo baja cuando se quita el [`Pillion`]. Un caballo sin
/// jinete no lleva a nadie en ancas.
pub(super) fn seat_pillions(
    mut commands: Commands,
    characters: Option<Res<CharacterModels>>,
    horses: Query<(Entity, &HorseBody, &Tack, &ChildOf, Option<&PillionSeat>)>,
    pillions: Query<&Pillion>,
) {
    for (carrier, body, tack, person, seated) in &horses {
        let want = pillions
            .get(person.parent())
            .ok()
            .filter(|_| body.rider.is_some())
            .map(|p| p.0);
        match (want, seated) {
            (None, None) => continue,
            (Some(kind), Some(s)) if s.kind == kind && s.croup == tack.croup => continue,
            _ => {}
        }
        if let Some(s) = seated {
            commands.entity(s.seat).try_despawn();
            commands.entity(carrier).remove::<PillionSeat>();
        }
        let Some(kind) = want else {
            continue;
        };
        let Some(scene) = characters.as_ref().and_then(|c| c.of(kind)).cloned() else {
            continue;
        };
        let seat = commands
            .spawn((
                Name::new("Pillion seat"),
                transform_of(tack.croup_rest.inverse() * Affine3A::from_translation(PILLION_SEAT)),
                Visibility::default(),
                ChildOf(tack.croup),
            ))
            .id();
        commands.spawn((
            Name::new("Pillion model"),
            CharacterModel,
            ModelPending(scene),
            Riding {
                horse: seat,
                kind,
                // Otra semilla que la del jinete: no se mueven a la par.
                seed: body.seed.wrapping_mul(31).wrapping_add(7),
            },
            Behind(carrier),
            transform_of(pillion_transform()),
            Visibility::Hidden,
            ChildOf(seat),
        ));
        commands.entity(carrier).try_insert(PillionSeat {
            kind,
            seat,
            croup: tack.croup,
        });
    }
}

/// Un estribo que lleva el pie de un jinete sentado. Queda bajo el torso del
/// caballo, con el resto de los aperos, ya que la escena del jinete (y los
/// huesos de sus pies) pueden volver a crearse; [`carry_stirrups`] lo pone en
/// el pie en cada frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct OnFoot {
    /// La raíz del modelo del jinete: mientras está oculta (todavía sin pose)
    /// el estribo se queda donde estaba.
    pub rider: Entity,
    pub foot: Entity,
    /// Su ubicación sobre el hueso del pie ([`Seated::stirrups`]).
    pub at: Transform,
}

/// Una vez que un jinete está sentado, y otra vez cada vez que se vuelven a
/// ubicar sus huesos (su escena creada de nuevo), las riendas de su caballo van
/// hasta su mano izquierda y sus estribos van en sus pies.
pub(super) fn seat_riders(
    mut commands: Commands,
    seated: Query<(Entity, &Riding, &Seated), Changed<Seated>>,
    tacks: Query<&Tack>,
    mut straps: Query<&mut Strap>,
) {
    for (rider, riding, seated) in &seated {
        let Ok(tack) = tacks.get(riding.horse) else {
            continue;
        };
        for s in 0..2 {
            // Ambas riendas en la mano izquierda, separadas el ancho de un
            // dedo.
            if let Ok(mut rein) = straps.get_mut(tack.reins[s]) {
                rein.ends[1] = rein_end(seated, s, false);
            }
            commands.entity(tack.stirrups[s]).try_insert(OnFoot {
                rider,
                foot: seated.bone(Bone::Foot(s)),
                at: seated.stirrups[s],
            });
        }
    }
}

/// Pone cada estribo llevado en el pie de su jinete, una vez calculados los
/// transforms (antes de que se estiren las correas hasta él).
pub(super) fn carry_stirrups(
    mut stirrups: Query<(&OnFoot, &ChildOf, &mut Transform, &mut GlobalTransform)>,
    places: Query<&GlobalTransform, Without<OnFoot>>,
    shown: Query<&Visibility>,
) {
    for (on, child_of, mut transform, mut global) in &mut stirrups {
        if shown.get(on.rider).is_ok_and(|v| *v == Visibility::Hidden) {
            continue;
        }
        let (Ok(foot), Ok(parent)) = (places.get(on.foot), places.get(child_of.parent())) else {
            continue;
        };
        let placed = foot.affine() * on.at.compute_affine();
        *global = GlobalTransform::from(placed);
        *transform = transform_of(parent.affine().inverse() * placed);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn two_bones_reach_the_target_bending_toward_the_pole_and_keep_their_lengths() {
        // Una pierna que cuelga doblada: cadera, rodilla adelante, tobillo
        // debajo.
        let (a, b, c) = (
            Vec3::ZERO,
            Vec3::new(0.0, -0.3, 0.3),
            Vec3::new(0.0, -0.7, 0.25),
        );
        for (target, pole) in [
            (Vec3::new(0.3, -0.69, 0.1), Vec3::new(0.45, -0.1, 1.0)),
            (Vec3::new(-0.2, -0.5, 0.3), Vec3::new(-1.0, 0.0, 0.2)),
            (Vec3::new(0.0, -0.3, -0.4), Vec3::new(0.0, 1.0, 0.0)),
        ] {
            let (d1, d2) = two_bone(a, b, c, target, pole);
            let knee = a + d1 * (b - a);
            let end = knee + d2 * d1 * (c - b);
            assert!(end.distance(target) < 1e-4, "{end} {target}");
            assert!((knee.distance(a) - b.distance(a)).abs() < 1e-5);
            assert!((end.distance(knee) - c.distance(b)).abs() < 1e-5);
            // La rodilla queda del lado del polo respecto de la línea de la
            // cadera al tobillo.
            let line = (target - a).normalize();
            let off = (knee - a) - line * (knee - a).dot(line);
            assert!(off.dot(pole) > 0.0, "{knee}");
            // El hueso inferior solo pivota: alrededor del eje del pliegue.
            let (axis, _) = d2.to_axis_angle();
            let bend = (knee - a).cross(end - knee).normalize();
            assert!(axis.cross(bend).length() < 1e-3, "{axis} {bend}");
        }
        // Fuera de alcance: recto hacia el objetivo, hasta donde llega la
        // cadena.
        let far = Vec3::new(0.0, -5.0, 0.0);
        let (d1, d2) = two_bone(a, b, c, far, Vec3::Z);
        let end = a + d1 * (b - a) + d2 * d1 * (c - b);
        let reach = b.distance(a) + c.distance(b);
        assert!(
            (end.length() - reach).abs() < 1e-3 && end.y < -reach + 1e-3,
            "{end}"
        );
    }

    #[test]
    fn the_upper_bone_keeps_its_twist_round_the_bend() {
        // El eje del pliegue gira con el hueso: un marco sobre el hueso
        // superior con su bisagra a lo largo del eje del pliegue lo mantiene
        // ahí.
        let (a, b, c) = (
            Vec3::ZERO,
            Vec3::new(0.0, -0.4, 0.1),
            Vec3::new(0.0, -0.8, -0.1),
        );
        let target = Vec3::new(0.25, -0.6, 0.2);
        let (d1, _) = two_bone(a, b, c, target, Vec3::new(1.0, 0.0, 1.0));
        let hinge = (b - a).cross(c - b).normalize();
        let knee = d1 * (b - a);
        let (d1b, d2) = two_bone(a, b, c, target, Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(d1, d1b, "deterministic");
        let end = knee + d2 * d1 * (c - b);
        let new_hinge = knee.cross(end - knee).normalize();
        assert!(
            (d1 * hinge).distance(new_hinge) < 1e-4,
            "{} {new_hinge}",
            d1 * hinge
        );
    }

    #[test]
    fn a_placement_composes_like_the_transforms() {
        let parent = Place {
            rotation: Quat::from_euler(EulerRot::XYZ, 0.3, -0.5, 0.9),
            translation: Vec3::new(0.1, 0.9, -0.2),
        };
        let local = Transform::from_xyz(0.0, 0.4, 0.05).with_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.2,
            0.1,
            0.4,
        ));
        let child = parent.then(&local);
        let want = Transform::from_translation(parent.translation)
            .with_rotation(parent.rotation)
            .compute_affine()
            * local.compute_affine();
        assert!(child.translation.distance(want.translation.into()) < 1e-6);
        assert!(
            child
                .rotation
                .angle_between(Quat::from_mat3a(&want.matrix3))
                < 1e-5
        );
    }

    #[test]
    fn the_rider_faces_the_horses_front_with_the_pelvis_above_the_seat() {
        let seat = Vec3::new(-0.02, 1.55, 0.0);
        let at = seat_transform(seat);
        assert!(
            at.transform_vector3(Vec3::Z).distance(Vec3::X) < 1e-6,
            "front"
        );
        // La izquierda del modelo (+X) es la izquierda del caballo (-Z).
        assert!(at.transform_vector3(Vec3::X).distance(Vec3::NEG_Z) < 1e-6);
        assert!(
            at.transform_point3(Vec3::ZERO)
                .distance(seat + Vec3::Y * PELVIS_RISE)
                < 1e-6
        );
        // Los pies bajan a cada lado, y la mano de las riendas va adelante y
        // arriba.
        const { assert!(REINS_HAND.z > 0.2 && REINS_HAND.y > 0.0 && THIGH_HAND.x < 0.0) };
        for s in 0..2 {
            let side = if s == 0 { 1.0 } else { -1.0 };
            assert!(ANKLES[s].x * side > 0.25 && ANKLES[s].y < -0.6);
            assert!(KNEE_POLE[s].x * side > 0.0 && ELBOW_POLE[s].x * side > 0.0);
        }
    }

    #[test]
    fn standing_the_free_hand_goes_all_the_way_to_the_brim_or_stays_on_the_thigh() {
        // Two minutes standing still, for a few riders: the hand is either
        // near the thigh, near the brim, or quickly on its way between them,
        // never held half way up in front of the chest.
        let dt = 0.02;
        let (mut mid, mut up) = (0, 0);
        for seed in 0..12 {
            let mut m = RiderMotion {
                seed,
                free: THIGH_HAND.to_array().map(Spring::at),
                ..default()
            };
            for frame in 0..6000 {
                m.clock = frame as f32 * dt;
                let hand = free_hand(&mut m, (0.0, 0.0, 0.0), (0.0, 0.0, 0.0, 1.0), dt);
                let way = (hand.y - THIGH_HAND.y) / (HAT_HAND.y - THIGH_HAND.y);
                if way > 0.9 {
                    up += 1;
                } else if way > 0.2 {
                    mid += 1;
                }
            }
        }
        assert!(up > 0, "never touched the brim");
        assert!(mid * 4 < up, "half way up {mid} frames, at the brim {up}");
    }

    #[test]
    fn in_the_half_seat_the_hands_stay_well_in_front_of_the_chest() {
        let chest = Vec3::new(0.0, 0.3, 0.1);
        let near = Vec3::new(0.1, 0.2, 0.25);
        // Sitting, where the hand goes is left alone.
        assert_eq!(clear_of_chest(near, chest, 0.0), near);
        // In the half-seat, pushed forward to the clearance, only forward.
        let held = clear_of_chest(near, chest, 1.0);
        assert!((held - Vec3::new(0.1, 0.2, 0.1 + CHEST_CLEAR)).length() < 1e-6);
        let far = Vec3::new(0.1, 0.2, 0.6);
        assert_eq!(clear_of_chest(far, chest, 1.0), far);
        // Half way in, half way there.
        let mid = clear_of_chest(near, chest, 0.5).z;
        assert!((mid - (0.25 + held.z) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn the_saddle_pitches_with_the_horses_nose_and_rolls_with_its_sides() {
        // The `Torso` bone at rest, whatever its own axes: the old horse's ran
        // its +Y forward (+X), its +X to the right (+Z) and its +Z up; henry's
        // `Spine_01` runs its +Y forward, its +X to the left (-Z) and its +Z
        // down.
        for rest in [
            Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::X, Vec3::Y)),
            Quat::from_mat3(&Mat3::from_cols(Vec3::NEG_Z, Vec3::X, Vec3::NEG_Y)),
        ] {
            for angle in [-0.4f32, -0.1, 0.1, 0.4] {
                // Nose up (about the body's lateral axis, +Z) is only a pitch.
                let (pitch, roll) = saddle_attitude(Quat::from_rotation_z(angle) * rest, rest);
                assert!((pitch - angle).abs() < 1e-4, "{angle}: pitch {pitch}");
                assert!(roll.abs() < 1e-4, "{angle}: roll {roll}");
                // Right side up (about the body's long axis, +X), only a roll.
                let (pitch, roll) = saddle_attitude(Quat::from_rotation_x(-angle) * rest, rest);
                assert!(pitch.abs() < 1e-4, "{angle}: pitch {pitch}");
                assert!((roll - angle).abs() < 1e-4, "{angle}: roll {roll}");
            }
        }
    }

    #[test]
    fn a_spring_eases_without_overshoot_the_same_at_any_frame_rate() {
        let mut fine = Spring::at(1.0);
        let mut coarse = Spring::at(1.0);
        let mut last = 1.0;
        for _ in 0..100 {
            let v = fine.step(0.0, 6.0, 0.01);
            assert!(v <= last + 1e-6 && v >= -1e-6, "{v}");
            last = v;
        }
        for _ in 0..10 {
            coarse.step(0.0, 6.0, 0.1);
        }
        assert!(
            (fine.value - coarse.value).abs() < 1e-5,
            "{fine:?} {coarse:?}"
        );
        assert!((fine.velocity - coarse.velocity).abs() < 1e-4);
        // Con amortiguamiento crítico desde el reposo: (1 + omega t) e^(-omega
        // t), arrancando suavemente y asentado después de unos 4.7/omega
        // segundos.
        let want = (1.0 + 6.0) * (-6.0f32).exp();
        assert!((fine.value - want).abs() < 1e-4, "{fine:?} {want}");
        let mut start = Spring::at(1.0);
        start.step(0.0, 6.0, 0.01);
        assert!(1.0 - start.value < 0.005, "it sets off gently: {start:?}");
        assert_eq!(Spring::at(2.0).step(2.0, 6.0, 0.1), 2.0, "at rest stays");
    }

    #[test]
    fn a_stirrup_stays_put_while_its_rider_is_hidden_and_goes_back_on_the_foot_once_shown() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let torso = world
            .spawn((
                Transform::default(),
                GlobalTransform::from_xyz(0.0, 1.2, 0.0),
            ))
            .id();
        let foot = world.spawn(GlobalTransform::from_xyz(0.3, 0.5, 0.1)).id();
        let rider = world.spawn(Visibility::Hidden).id();
        let hanging = Transform::from_xyz(0.25, -0.6, 0.0);
        let on = OnFoot {
            rider,
            foot,
            at: Transform::from_xyz(0.0, -0.05, 0.02),
        };
        let stirrup = world
            .spawn((
                on,
                hanging,
                GlobalTransform::from_xyz(0.25, 0.6, 0.0),
                ChildOf(torso),
            ))
            .id();
        let place = |world: &World| {
            (
                *world.get::<Transform>(stirrup).unwrap(),
                world.get::<GlobalTransform>(stirrup).unwrap().translation(),
            )
        };
        // Oculto (su escena recién creada, todavía sin pose): se deja donde
        // colgaba, sin ponerlo en un pie parado en reposo.
        world.run_system_once(carry_stirrups).unwrap();
        assert_eq!(place(&world), (hanging, Vec3::new(0.25, 0.6, 0.0)));
        // Visible: en el pie, con su lugar local bajo el torso a la par.
        world.entity_mut(rider).insert(Visibility::Inherited);
        world.run_system_once(carry_stirrups).unwrap();
        let (local, global) = place(&world);
        assert!(
            global.distance(Vec3::new(0.3, 0.45, 0.12)) < 1e-5,
            "{global}"
        );
        assert!(
            (local.translation - Vec3::new(0.3, -0.75, 0.12)).length() < 1e-5,
            "{local:?}"
        );
        // Y sigue al pie.
        *world.get_mut::<GlobalTransform>(foot).unwrap() = GlobalTransform::from_xyz(0.2, 0.7, 0.0);
        world.run_system_once(carry_stirrups).unwrap();
        assert!(place(&world).1.distance(Vec3::new(0.2, 0.65, 0.02)) < 1e-5);
        // Oculto otra vez: conserva su último lugar.
        world.entity_mut(rider).insert(Visibility::Hidden);
        *world.get_mut::<GlobalTransform>(foot).unwrap() = GlobalTransform::from_xyz(9.0, 9.0, 9.0);
        world.run_system_once(carry_stirrups).unwrap();
        assert!(place(&world).1.distance(Vec3::new(0.2, 0.65, 0.02)) < 1e-5);
    }

    #[test]
    fn a_pillion_follows_the_rider_the_croup_and_a_new_horse() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let mut people = CharacterModels::default();
        people.0.insert("bairoletto", Handle::default());
        world.insert_resource(people);
        let me = world.spawn(Pillion(FigureKind::Bairoletto)).id();
        let horse = |world: &mut World| {
            let croup = world.spawn_empty().id();
            let tack = Tack {
                reins: [Entity::PLACEHOLDER; 2],
                stirrups: [Entity::PLACEHOLDER; 2],
                torso: croup,
                head: Entity::PLACEHOLDER,
                seat: Vec3::ZERO,
                torso_rest: Affine3A::IDENTITY,
                bit: Vec3::ZERO,
                croup,
                croup_rest: Affine3A::IDENTITY,
            };
            let body = HorseBody {
                coat: Coat::Bayo,
                rider: Some(FigureKind::Protagonist),
                saddled: true,
                gait: pampa_core::HorseGait::Idle,
                gallop: 0.0,
                speed: 0.0,
                flight: None,
                seed: 3,
            };
            let carrier = world.spawn((body, tack, ChildOf(me))).id();
            // La grupa es un hueso de la escena del caballo.
            world.entity_mut(croup).insert(ChildOf(carrier));
            carrier
        };
        // Su asiento colgado de la grupa, con su modelo encima, y nadie más.
        let seat = |world: &mut World, carrier: Entity| {
            world.run_system_once(seat_pillions).unwrap();
            let models: Vec<_> = world
                .query::<(&Behind, &ChildOf)>()
                .iter(world)
                .map(|(b, c)| (b.0, c.parent()))
                .collect();
            let seat = world.get::<PillionSeat>(carrier).map(|s| s.seat);
            if let Some(seat) = seat {
                let croup = world.get::<Tack>(carrier).unwrap().croup;
                assert_eq!(world.get::<ChildOf>(seat).unwrap().parent(), croup);
                assert_eq!(models, [(carrier, seat)]);
            } else {
                assert!(models.is_empty(), "{models:?}");
            }
            seat
        };
        let first = horse(&mut world);
        let on = seat(&mut world, first).expect("he sits behind the rider");
        // Sin jinete se baja; con el jinete otra vez, vuelve a subir.
        world.get_mut::<HorseBody>(first).unwrap().rider = None;
        assert_eq!(seat(&mut world, first), None);
        assert!(world.get_entity(on).is_err());
        world.get_mut::<HorseBody>(first).unwrap().rider = Some(FigureKind::Protagonist);
        let on = seat(&mut world, first).expect("he gets back on");
        // La escena del caballo creada de nuevo: otra grupa, otro asiento.
        let croup = world.spawn(ChildOf(first)).id();
        world.get_mut::<Tack>(first).unwrap().croup = croup;
        let again = seat(&mut world, first).expect("he sits on the new croup");
        assert_ne!(again, on);
        assert!(world.get_entity(on).is_err());
        // Al bajarse el jugador se va el caballo; al volver a subir, en otro
        // caballo, sigue en ancas.
        world.entity_mut(first).despawn();
        let second = horse(&mut world);
        seat(&mut world, second).expect("he rides behind again");
    }

    // --- con el caballo y el jinete incluidos --------------------------------

    use crate::character::cpu;
    use crate::character::horse::tests::{named, ride, saddled_every, try_named};
    use crate::character::tack::tests::{horse_skin, signed_distance};
    use crate::character::tack::{HorseBone, HorsePiece, Strap, Tack};
    use crate::character::{CharacterVisual, Coat, FigureKind, HorseModel, Rig};

    /// [`saddled_every`] con un jinete, que vuelve cuando el jinete también
    /// reproduce sus clips, sentado.
    pub(crate) fn seated_every(visual: CharacterVisual, speed: f32, ms: u64) -> (App, Entity) {
        let (mut app, me) = saddled_every(visual, speed, ms);
        for _ in 0..3000 {
            app.update();
            let world = app.world_mut();
            if world
                .query::<(&Rig, &Seated)>()
                .iter(world)
                .any(|(r, _)| r.started)
            {
                // Un frame más: las correas y los estribos del asiento se
                // trasladan.
                app.update();
                return (app, me);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the rider never sat");
    }

    pub(crate) fn seated(visual: CharacterVisual, speed: f32) -> (App, Entity) {
        seated_every(visual, speed, 50)
    }

    /// Cuánto se hunde el poncho del modelo bajo `model` en la parte superior
    /// de sus brazos (o en sus hombros), en metros: para cada brazo, la parte
    /// superior es una cápsula alrededor del hueso desde la articulación del
    /// hombro hasta el codo, tan gruesa como la manga alrededor de su centro
    /// (la distancia mediana de los puntos de la camisa ahí), y la respuesta es
    /// el punto del poncho más profundo dentro de cualquiera de las dos (0
    /// cuando no hay ninguno). Solo cuenta lo que queda hacia afuera de la
    /// axila (no más de 4 cm hacia adentro desde la articulación del hombro, a
    /// lo ancho del modelo): más cerca de la columna la cápsula abarcaría el
    /// pecho y la espalda.
    pub(crate) fn poncho_in_arms(world: &mut World, model: Entity) -> f32 {
        let drawn = cpu::drawn(world, Some(model));
        let find = |name: &str| {
            let mut stack = vec![model];
            while let Some(e) = stack.pop() {
                if world.get::<Name>(e).is_some_and(|n| n.as_str() == name) {
                    return world.get::<GlobalTransform>(e).map(|g| g.translation());
                }
                stack.extend(world.get::<Children>(e).into_iter().flatten().copied());
            }
            None
        };
        let mesh = |name: &'static str| {
            drawn
                .iter()
                .filter(move |d| d.name == name)
                .flat_map(|d| d.positions.iter().copied())
        };
        let to_model = world
            .get::<GlobalTransform>(model)
            .map_or(bevy::math::Affine3A::IDENTITY, |g| g.affine().inverse());
        let mut deepest = 0.0f32;
        for side in ["l", "r"] {
            let (Some(a), Some(b)) = (
                find(&format!("upperarm_{side}")),
                find(&format!("lowerarm_{side}")),
            ) else {
                continue;
            };
            let along = b - a;
            let place = |p: Vec3| {
                let t = (p - a).dot(along) / along.length_squared();
                (t, p.distance(a + along * t.clamp(0.0, 1.0)))
            };
            let mut sleeve: Vec<f32> = mesh("shirt")
                .map(place)
                .filter(|(t, d)| (0.3..0.7).contains(t) && *d < 0.15)
                .map(|(_, d)| d)
                .collect();
            if sleeve.is_empty() {
                continue;
            }
            sleeve.sort_by(f32::total_cmp);
            let radius = sleeve[sleeve.len() / 2];
            let shoulder = to_model.transform_point3(a).x.abs();
            for p in mesh("poncho") {
                if to_model.transform_point3(p).x.abs() < shoulder - 0.04 {
                    continue;
                }
                let (t, d) = place(p);
                if (0.0..=1.0).contains(&t) && d < radius {
                    deepest = deepest.max(radius - d);
                }
            }
        }
        deepest
    }

    /// El ángulo interior en `b` entre `a` y `c`, en grados.
    fn angle_at(a: Vec3, b: Vec3, c: Vec3) -> f32 {
        (a - b).angle_between(c - b).to_degrees()
    }

    fn spread(v: &[f32]) -> f32 {
        v.iter().fold(f32::MIN, |a, b| a.max(*b)) - v.iter().fold(f32::MAX, |a, b| a.min(*b))
    }

    #[test]
    fn a_rider_sits_on_the_horses_torso_with_the_reins_and_the_stirrups() {
        let (mut app, me) = seated(
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            },
            0.0,
        );
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        // El modelo del jinete se sienta sobre el hueso del torso del caballo.
        let mut riders = world.query::<(Entity, &Riding, &Seated, &ChildOf)>();
        let (rider, riding, seated, seat) = riders.single(world).unwrap();
        let seated = seated.clone();
        assert_eq!(
            (riding.horse, riding.kind),
            (carrier, FigureKind::Protagonist)
        );
        assert_eq!(
            world.get::<Name>(seat.parent()).unwrap().as_str(),
            HorseBone::Torso.name()
        );
        // Las riendas van hasta sus manos y los estribos están en sus pies.
        let tack = *world.get::<Tack>(carrier).unwrap();
        for s in 0..2 {
            let rein = world.get::<Strap>(tack.reins[s]).unwrap();
            // Ambas en la mano izquierda.
            assert_eq!(
                rein.ends[1].0,
                seated.bones[Bone::ALL.iter().position(|b| *b == Bone::Hand(0)).unwrap()]
            );
            // Llevado por el pie, conservado bajo el torso del caballo.
            let on = world.get::<OnFoot>(tack.stirrups[s]).unwrap();
            assert_eq!(
                world.get::<Name>(on.foot).unwrap().as_str(),
                ["foot_l", "foot_r"][s]
            );
            assert_eq!((on.rider, on.at), (rider, seated.stirrups[s]));
            let hung = world.get::<ChildOf>(tack.stirrups[s]).unwrap().parent();
            assert_eq!(hung, tack.torso);
            let (stirrup, foot) = (
                world.get::<GlobalTransform>(tack.stirrups[s]).unwrap(),
                world.get::<GlobalTransform>(on.foot).unwrap(),
            );
            let want = foot.affine() * on.at.compute_affine();
            assert!(
                stirrup.translation().distance(Vec3::from(want.translation)) < 1e-4,
                "on the foot"
            );
        }
    }

    /// Los frames que puede tardar la escena ya cargada de un jinete en volver
    /// a crearse, sentarse y mostrarse.
    const RESPAWN_FRAMES: usize = 3;

    #[test]
    fn a_respawned_rider_keeps_the_stirrups_on_its_new_feet_and_the_reins_in_its_new_hand() {
        use bevy::world_serialization::WorldAssetRoot;
        let (mut app, me) = seated(
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            },
            1.5,
        );
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let tack = *world.get::<Tack>(carrier).unwrap();
        let (rider, old) = world
            .query_filtered::<(Entity, &Seated), With<Riding>>()
            .single(world)
            .map(|(e, s)| (e, s.clone()))
            .unwrap();
        // Su escena creada otra vez: huesos nuevos.
        world
            .get_mut::<WorldAssetRoot>(rider)
            .unwrap()
            .set_changed();
        // La escena ya está cargada: se vuelve a crear, se prepara y se pone en
        // pose en unos pocos frames, sin esperar al servidor de assets.
        let mut new = None;
        for frame in 0..RESPAWN_FRAMES {
            app.update();
            let world = app.world();
            let seated = world.get::<Seated>(rider).unwrap();
            let posed = world.get::<Visibility>(rider) == Some(&Visibility::Inherited);
            if seated.bones[0] != old.bones[0] && posed {
                new = Some((frame, seated.clone()));
                break;
            }
        }
        let (frame, new) = new.expect("the rider's scene spawned again and was posed");
        println!("respawned and seated after {} frames", frame + 1);
        let bone = |b: Bone| new.bones[Bone::ALL.iter().position(|x| *x == b).unwrap()];
        // A lo largo de unos pocos frames de la caminata: los estribos van en
        // los pies nuevos y las riendas terminan en la mano nueva.
        for _ in 0..10 {
            app.update();
            let world = app.world();
            for s in 0..2 {
                let on = world
                    .get::<OnFoot>(tack.stirrups[s])
                    .expect("the stirrup is kept");
                assert_eq!(on.foot, bone(Bone::Foot(s)));
                let foot = world.get::<GlobalTransform>(on.foot).unwrap();
                let at = world.get::<GlobalTransform>(tack.stirrups[s]).unwrap();
                let want = foot.affine() * on.at.compute_affine();
                assert!(at.translation().distance(Vec3::from(want.translation)) < 1e-4);
                let rein = world.get::<Strap>(tack.reins[s]).unwrap();
                assert_eq!(rein.ends[1].0, bone(Bone::Hand(0)));
            }
        }
    }

    #[test]
    fn over_a_jump_the_rider_stays_on_the_saddle_folds_forward_and_sits_up_to_land() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // Cuánto se inclina el jinete hacia adelante respecto de la vertical
        // (su cuello sobre su pelvis), en grados; cuánto está su asiento por
        // encima de la montura, en su propio marco (del asiento), en metros; y
        // su mano de las riendas por delante de la montura, a lo largo del
        // caballo, en metros.
        let measure = |app: &App| {
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let [pelvis, neck, hand] =
                ["pelvis", "neck_01", "hand_l"].map(|n| named(world, model, n));
            let torso = named(world, carrier, HorseBone::Torso.name());
            let g = |e: Entity| *world.get::<GlobalTransform>(e).unwrap();
            let body = g(carrier).affine().inverse();
            let back = body.transform_vector3(g(neck).translation() - g(pelvis).translation());
            let gap = g(model)
                .affine()
                .inverse()
                .transform_point3(g(pelvis).translation())
                .y;
            let reach = body.transform_point3(g(hand).translation()).x
                - body.transform_point3(g(torso).translation()).x;
            (back.x.atan2(back.y).to_degrees(), gap, reach)
        };
        // La pelvis en el marco de la montura (del hueso `Torso`).
        let seat = |app: &App| {
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let [torso, pelvis] = [
                named(world, carrier, HorseBone::Torso.name()),
                named(world, model, "pelvis"),
            ];
            let g = |e: Entity| world.get::<GlobalTransform>(e).unwrap().affine();
            g(torso)
                .inverse()
                .transform_point3(Vec3::from(g(pelvis).translation))
        };
        let fly = |app: &mut App, flight: Option<f32>| {
            app.world_mut()
                .entity_mut(me)
                .insert(crate::character::VisualPose {
                    animation: pampa_core::AnimationState::Gallop,
                    speed: 12.2,
                    flight,
                    ..default()
                });
            app.update();
        };
        let mean = |v: &[(f32, f32, f32)], k: usize| {
            v.iter().map(|m| [m.0, m.1, m.2][k]).sum::<f32>() / v.len() as f32
        };
        for _ in 0..80 {
            fly(&mut app, None);
        }
        let mut ground = Vec::new();
        // Two whole strides (henry gallops at 12.2 m/s in ~0.48 s, 24 frames):
        // a mean over a stride and a half depends on where it starts.
        for _ in 0..48 {
            fly(&mut app, None);
            ground.push(measure(&app));
        }
        let saddle = seat(&app);
        // El vuelo completo de un salto, a su ritmo real (1.1 s de frames de
        // 20 ms), con el jinete quedándose sobre la montura.
        let mut flight = Vec::new();
        for k in 0..55 {
            fly(&mut app, Some(k as f32 / 54.0));
            flight.push(measure(&app));
            let off = seat(&app).distance(saddle);
            assert!(off < 0.15, "frame {k}: the rider is {off} m off the saddle");
        }
        let mut landing = Vec::new();
        for _ in 0..100 {
            fly(&mut app, None);
            landing.push(measure(&app));
        }
        assert!(seat(&app).distance(saddle) < 0.15, "and back on it");
        let (lean, reach) = (mean(&ground, 0), mean(&ground, 2));
        let all: Vec<_> = flight.iter().chain(&landing).collect();
        let jolt = all
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).abs())
            .fold(0.0, f32::max);
        let top = &flight[15..45];
        let sitting = landing[..25].iter().map(|m| m.0).fold(f32::MAX, f32::min);
        println!(
            "gallop {lean:.1}° {reach:.2} m, over the top {:.1}° {:.2} m, landing {sitting:.1}°, after {:.1}°, jolt {jolt:.1}°",
            mean(top, 0),
            mean(top, 2),
            mean(&landing[60..], 0),
        );
        // Plegado hacia adelante desde las caderas en lo alto, con la mano
        // hacia adelante a lo largo del cuello.
        let leans: Vec<f32> = top.iter().map(|m| m.0).collect();
        assert!(
            leans.iter().all(|l| (lean + 12.0..lean + 35.0).contains(l)),
            "{leans:?} vs {lean}"
        );
        // 0.1 -> 0.02 m (H6, L35-L38): at the gallop the hands already stay
        // 0.3 m in front of the chest, and with both on the reins (S9) the
        // left one starts further out, so they have less forward to go
        // (measured +0.03..+0.09 m).
        assert!(mean(top, 2) > reach + 0.02, "{} vs {reach}", mean(top, 2));
        // Enderezándose para aterrizar, y después siguiendo en medio asiento.
        assert!(sitting < lean - 6.0, "{sitting} vs {lean}");
        assert!((mean(&landing[60..], 0) - lean).abs() < 4.0);
        // Nunca de golpe, y nunca dentro de la montura.
        assert!(jolt < 5.0, "{jolt}° in a frame");
        let lowest = all.iter().map(|m| m.1).fold(f32::MAX, f32::min);
        assert!(lowest > -0.002, "the seat {lowest} m into the saddle");
    }

    #[test]
    fn at_the_gallop_the_rider_rides_the_half_seat() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        // Until the half-seat has fully come in (`HALF_SEAT_EASE`): still
        // settling, the lean drifts a couple of degrees from stride to stride.
        for _ in 0..160 {
            app.update();
        }
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let (mut gap, mut knee, mut elbow, mut reach, mut mouth) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut saddle, mut skull, mut nods, mut pitches) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut closest = f32::MAX;
        let mut leans = Vec::new();
        // Dos zancadas y algo más.
        for k in 0..40 {
            app.update();
            let world = app.world();
            // Ubicados de nuevo en cada frame: una escena creada otra vez tiene
            // huesos nuevos.
            let model = named(world, carrier, "Rider model");
            let [torso, horse_head] =
                [HorseBone::Torso, HorseBone::Head].map(|b| named(world, carrier, b.name()));
            let [pelvis, thigh, calf, foot, head, upper, lower, hand, neck] = [
                "pelvis",
                "thigh_l",
                "calf_l",
                "foot_l",
                "head",
                "upperarm_l",
                "lowerarm_l",
                "hand_l",
                "neck_01",
            ]
            .map(|n| named(world, model, n));
            let bit = world.get::<Saddle>(carrier).unwrap().bit;
            let g = |e: Entity| *world.get::<GlobalTransform>(e).unwrap();
            let at = |e: Entity| g(e).translation();
            let turn = |e: Entity| g(e).to_scale_rotation_translation().1;
            let body = g(carrier).affine().inverse();
            // El asiento por encima de la montura, en el propio marco (del
            // asiento) del jinete.
            gap.push(g(model).affine().inverse().transform_point3(at(pelvis)).y);
            knee.push(angle_at(at(thigh), at(calf), at(foot)));
            elbow.push(angle_at(at(upper), at(lower), at(hand)));
            let bit_at = g(horse_head).transform_point(bit);
            reach.push(at(hand).distance(bit_at));
            mouth.push(body.transform_point3(bit_at).x);
            saddle.push(at(torso).y);
            skull.push(at(head).y);
            let pitch = |q: Quat| {
                let up = body.transform_vector3(q * Vec3::Y);
                up.x.atan2(up.y)
            };
            nods.push(pitch(turn(head)));
            // El cabeceo de la montura a lo largo del caballo (el hueso recorre
            // +Y).
            pitches.push((body.transform_vector3(turn(torso) * Vec3::Y)).y.asin());
            let back = body.transform_vector3(at(neck) - at(pelvis));
            leans.push(back.x.atan2(back.y).to_degrees());
            let head_at = at(head);
            if k % 4 == 0 {
                // La cabeza del jinete nunca cerca de la cabeza ni del cuello
                // del caballo.
                let drawn = cpu::drawn(app.world_mut(), Some(carrier));
                let (skin, _) = horse_skin(&drawn);
                let to_body = |p: Vec3| body.transform_point3(p);
                for p in skin.iter().filter(|p| to_body(**p).x > 0.45) {
                    closest = closest.min(p.distance(head_at));
                }
            }
        }
        println!(
            "gap {:.3}..{:.3} m, knee {:.0}..{:.0}°, elbow {:.0}..{:.0}°, hand to bit {:.3}..{:.3} m, saddle {:.3} m, head {:.3} m, head pitch {:.3}, saddle pitch {:.3}, closest {closest:.2} m",
            gap.iter().fold(f32::MAX, |a, b| a.min(*b)),
            gap.iter().fold(f32::MIN, |a, b| a.max(*b)),
            knee.iter().fold(f32::MAX, |a, b| a.min(*b)),
            knee.iter().fold(f32::MIN, |a, b| a.max(*b)),
            elbow.iter().fold(f32::MAX, |a, b| a.min(*b)),
            elbow.iter().fold(f32::MIN, |a, b| a.max(*b)),
            reach.iter().fold(f32::MAX, |a, b| a.min(*b)),
            reach.iter().fold(f32::MIN, |a, b| a.max(*b)),
            spread(&saddle),
            spread(&skull),
            spread(&nods),
            spread(&pitches),
        );
        // Inclinado hacia adelante unos 15–25° respecto de la vertical.
        let lean = leans.iter().sum::<f32>() / leans.len() as f32;
        println!(
            "lean {lean:.1}°, spread {:.1}° against the saddle's {:.1}°",
            spread(&leans),
            spread(&pitches).to_degrees()
        );
        assert!((15.0..25.0).contains(&lean), "lean {lean}°");
        // La columna compensa el cabeceo de la montura: la parte superior del
        // cuerpo cabecea menos que el caballo que tiene debajo.
        assert!(
            spread(&leans) < 0.8 * spread(&pitches).to_degrees(),
            "the rider pitches {}° on a saddle pitching {}°",
            spread(&leans),
            spread(&pitches).to_degrees()
        );
        // El asiento flota y cede, nunca por debajo de la montura.
        assert!(gap.iter().all(|g| *g > -0.002), "{gap:?}");
        assert!(spread(&gap) > 0.02, "the seat gives {}", spread(&gap));
        // Su cabeza lejos de la cabeza y del cuello del caballo, hagan lo que
        // hagan.
        assert!(
            closest > 0.3,
            "the rider's head {closest} m from the horse's"
        );
        // Las rodillas lo absorben: unos 110°, abriéndose y cerrándose.
        let mean = knee.iter().sum::<f32>() / knee.len() as f32;
        assert!((100.0..125.0).contains(&mean), "knee {mean}°");
        assert!(spread(&knee) > 4.0, "the knee flexes {}°", spread(&knee));
        // La cabeza va más estable que la montura, en altura y en cabeceo.
        assert!(
            spread(&skull) < 0.8 * spread(&saddle),
            "{} {}",
            spread(&skull),
            spread(&saddle)
        );
        assert!(
            spread(&nods) < spread(&pitches),
            "{} {}",
            spread(&nods),
            spread(&pitches)
        );
        // La mano de las riendas mantiene un contacto parejo: sigue la boca,
        // con el codo abriéndose cuando la cabeza va hacia adelante y
        // cerrándose cuando vuelve.
        // 0.08 -> 0.17 m (H6, L35-L38): henry's head nods 0.33 m, and the
        // hands do not follow it back into the rider's chest (measured
        // 0.094-0.150 m with both hands on the reins).
        assert!(spread(&reach) < 0.17, "hand to bit {}", spread(&reach));
        let n = elbow.len() as f32;
        let (me_, mm) = (elbow.iter().sum::<f32>() / n, mouth.iter().sum::<f32>() / n);
        let cov: f32 = elbow
            .iter()
            .zip(&mouth)
            .map(|(e, m)| (e - me_) * (m - mm))
            .sum();
        let var = |v: &[f32], m: f32| v.iter().map(|x| (x - m) * (x - m)).sum::<f32>().sqrt();
        let corr = cov / (var(&elbow, me_) * var(&mouth, mm)).max(1e-9);
        assert!(corr > 0.3, "the elbow follows the mouth: {corr}");
    }

    /// The rider model on `carrier`, once its scene (maybe being created again
    /// in parallel runs) has its bones: riding on at `speed` until then.
    fn posed_model(app: &mut App, me: Entity, carrier: Entity, speed: f32) -> Entity {
        // Under a loaded full run the scene can take a while to come back.
        const PATIENCE: usize = 3000;
        for _ in 0..PATIENCE {
            let world = app.world();
            if let Some(m) = try_named(world, carrier, "Rider model")
                .filter(|m| try_named(world, *m, "hand_l").is_some())
            {
                return m;
            }
            ride(app, me, speed);
        }
        panic!("no rider model with a hand_l after {PATIENCE} frames of riding at {speed} m/s");
    }

    /// How far the bone lines of the rein arm (the left) and of the free arm
    /// keep outside the rider's body (its torso's skin and clothes, only where
    /// no arm or leg bone carries them), each as its upper arm (from a third
    /// down: its top sits in the shoulder) and its forearm (to the wrist), in
    /// metres; below zero, inside. The bone line runs along the middle of the
    /// arm: a sleeve's width (~2-3 cm) is touching.
    fn arms_off_the_body(app: &mut App, model: Entity) -> ([f32; 2], [f32; 2]) {
        let drawn = cpu::drawn(app.world_mut(), Some(model));
        let world = app.world();
        const LIMBS: [&str; 12] = [
            "arm", "hand", "thumb", "index", "middle", "ring", "pinky", "clavicle", "thigh",
            "calf", "foot", "ball",
        ];
        let limb = |e: Option<Entity>| {
            e.and_then(|e| world.get::<Name>(e))
                .is_some_and(|n| LIMBS.iter().any(|k| n.as_str().contains(k)))
        };
        const TORSO: [&str; 5] = ["m021_body", "shirt", "waistcoat", "sash", "bombacha"];
        let (mut body, mut tris) = (Vec::new(), Vec::new());
        for d in drawn.iter().filter(|d| TORSO.contains(&d.name.as_str())) {
            let base = body.len() as u32;
            body.extend(&d.positions);
            tris.extend(
                d.triangles
                    .iter()
                    .filter(|t| t.iter().all(|i| !limb(d.strongest[*i as usize])))
                    .map(|t| t.map(|i| i + base)),
            );
        }
        let at = |b: &str| {
            world
                .get::<GlobalTransform>(named(world, model, b))
                .unwrap()
                .translation()
        };
        let [rein, free] = ["l", "r"].map(|side| {
            let [a, b, c] = ["upperarm", "lowerarm", "hand"].map(|n| at(&format!("{n}_{side}")));
            let gap = |from: Vec3, to: Vec3, start: f32| {
                (0..=10)
                    .map(|k| from.lerp(to, start + (1.0 - start) * k as f32 / 10.0))
                    .map(|q| signed_distance(q, &body, &tris))
                    .fold(f32::MAX, f32::min)
            };
            [gap(a, b, 0.3), gap(b, c, 0.0)]
        });
        (rein, free)
    }

    /// Where the head sits from the middle of the shoulders (the upper arm
    /// joints), in the chest's own frame (`spine_03`): posed under `model`, or
    /// standing at rest in the protagonist's glTF.
    fn head_from_shoulders(world: &World, model: Entity) -> Vec3 {
        let at = |b: &str| {
            *world
                .get::<GlobalTransform>(named(world, model, b))
                .unwrap()
        };
        let mid = (at("upperarm_l").translation() + at("upperarm_r").translation()) / 2.0;
        at("spine_03")
            .affine()
            .inverse()
            .transform_vector3(at("head").translation() - mid)
    }

    fn rest_head_from_shoulders() -> Vec3 {
        let doc = crate::character::model::tests::shipped_model("protagonist");
        let mut parent = std::collections::HashMap::new();
        for n in doc.nodes() {
            for c in n.children() {
                parent.insert(c.index(), n.index());
            }
        }
        let at = |name: &str| {
            let mut at = doc
                .nodes()
                .find(|n| n.name() == Some(name))
                .map(|n| n.index());
            let mut m = Mat4::IDENTITY;
            while let Some(k) = at {
                let (t, r, s) = doc.nodes().nth(k).unwrap().transform().decomposed();
                m = Mat4::from_scale_rotation_translation(s.into(), Quat::from_array(r), t.into())
                    * m;
                at = parent.get(&k).copied();
            }
            m
        };
        let p = |name: &str| at(name).transform_point3(Vec3::ZERO);
        let mid = (p("upperarm_l") + p("upperarm_r")) / 2.0;
        at("spine_03").inverse().transform_vector3(p("head") - mid)
    }

    /// Seated still, settled and walking: the rein arm (the left) bends at the
    /// elbow instead of locking straight, the rider sits upright (the sitting
    /// clip leans back about 7°, which put the shoulders behind the hips), his
    /// head sits over his shoulders as standing at rest (the clip pulled the
    /// shoulders ~8 cm back, leaving the head ahead of them), and no arm passes
    /// through his body.
    #[test]
    fn sitting_and_walking_the_arms_bend_and_keep_out_of_the_body() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 0.0, 20);
        let rest_head = rest_head_from_shoulders();
        // Just seated, settled with the hand resting low in front, walking.
        for (speed, settle) in [(0.0, 20), (0.0, 200), (1.25, 120)] {
            for _ in 0..settle {
                ride(&mut app, me, speed);
            }
            let carrier = app.world().get::<Children>(me).unwrap()[0];
            let (mut elbow, mut lean, mut head) = (Vec::new(), Vec::new(), 0.0f32);
            for _ in 0..10 {
                ride(&mut app, me, speed);
                let model = posed_model(&mut app, me, carrier, speed);
                let world = app.world();
                let seat = world
                    .get::<GlobalTransform>(model)
                    .unwrap()
                    .affine()
                    .inverse();
                let at = |b: &str| {
                    seat.transform_point3(
                        world
                            .get::<GlobalTransform>(named(world, model, b))
                            .unwrap()
                            .translation(),
                    )
                };
                elbow.push(angle_at(at("upperarm_l"), at("lowerarm_l"), at("hand_l")));
                let back = at("neck_01") - at("pelvis");
                lean.push(back.z.atan2(back.y).to_degrees());
                head = head.max(head_from_shoulders(world, model).distance(rest_head));
            }
            let model = posed_model(&mut app, me, carrier, speed);
            let (rein, free) = arms_off_the_body(&mut app, model);
            let (rein, free) = (rein[0].min(rein[1]), free[0].min(free[1]));
            let n = elbow.len() as f32;
            let (elbow, lean) = (elbow.iter().sum::<f32>() / n, lean.iter().sum::<f32>() / n);
            println!(
                "{speed} m/s after {settle} frames: elbow {elbow:.0}°, lean {lean:.1}°, head {head:.3} m off its rest, arms off the body: rein {rein:.3} m, free {free:.3} m"
            );
            assert!(
                (95.0..150.0).contains(&elbow),
                "{speed} m/s: the rein elbow at {elbow}°"
            );
            assert!(lean.abs() < 4.0, "{speed} m/s: leaning {lean}°");
            assert!(head < 0.04, "{speed} m/s: the head {head} m off its rest");
            // Standing still, now and then the free hand goes up to the hat
            // brim, past the chest: then only never inside the body.
            assert!(
                rein > 0.02,
                "{speed} m/s: the rein arm {rein} m off the body"
            );
            let sleeve = if speed > 0.0 { 0.02 } else { 0.0 };
            assert!(
                free > sleeve,
                "{speed} m/s: the free arm {free} m off the body"
            );
        }
    }

    /// At full gallop the rider holds the reins in both hands (S9), each rein
    /// in its own hand; over a couple of strides neither arm goes into his
    /// chest, the reins never cut the horse, and his head stays over his
    /// shoulders.
    #[test]
    fn at_the_gallop_both_hands_hold_the_reins_clear_of_the_chest_and_the_head_over_the_shoulders()
    {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        for _ in 0..160 {
            app.update();
        }
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let rest_head = rest_head_from_shoulders();
        let mut arms = [[f32::MAX; 2]; 2];
        let mut head = 0.0f32;
        // Every frame of a whole stride (24 frames) and a bit: the hands
        // follow the mouth back toward the chest at one point of it.
        for _ in 0..28 {
            app.update();
            let model = posed_model(&mut app, me, carrier, 12.2);
            head = head.max(head_from_shoulders(app.world(), model).distance(rest_head));
            let (rein, free) = arms_off_the_body(&mut app, model);
            for (k, side) in [rein, free].into_iter().enumerate() {
                arms[k] = [arms[k][0].min(side[0]), arms[k][1].min(side[1])];
            }
        }
        // Each rein ends in its own hand.
        let world = app.world_mut();
        let seated = world
            .query_filtered::<&Seated, With<Riding>>()
            .single(world)
            .unwrap()
            .clone();
        let tack = *world.get::<Tack>(carrier).unwrap();
        for s in 0..2 {
            let rein = world.get::<Strap>(tack.reins[s]).unwrap();
            assert_eq!(rein.ends[1].0, seated.bone(Bone::Hand(s)), "rein {s}");
        }
        let (_, _, cuts) = crate::character::tack::tests::clearances(&mut app, me, 24);
        println!(
            "gallop: left upper arm {:.3} m and forearm {:.3} m, right {:.3} m and {:.3} m off the body, head {head:.3} m off its rest, cuts {cuts:?}",
            arms[0][0], arms[0][1], arms[1][0], arms[1][1]
        );
        for (side, [upper, fore]) in ["left", "right"].into_iter().zip(arms) {
            // The forearm reaches forward in front of the chest, well off it
            // (measured 8.6-11 cm; without the hands kept forward, 0.6-5.7).
            // The upper arm hangs by the side of the ribs: never inside.
            assert!(fore > 0.07, "the {side} forearm {fore} m off the body");
            assert!(upper > 0.0, "the {side} upper arm {upper} m off the body");
        }
        assert!(cuts.is_empty(), "{cuts:?}");
        // Leaning into the gallop the head is held level, bending the neck a
        // little more than sitting.
        assert!(head < 0.05, "the head {head} m off its rest");
        // Back to the walk, both reins in the left hand again, where they
        // were seated.
        for _ in 0..160 {
            ride(&mut app, me, 1.5);
        }
        for s in 0..2 {
            let rein = app.world().get::<Strap>(tack.reins[s]).unwrap();
            assert_eq!(rein.ends[1], rein_end(&seated, s, false), "walk: rein {s}");
        }
    }

    #[test]
    fn walking_the_rider_sits_deep_and_upright() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 1.5, 20);
        for _ in 0..80 {
            app.update();
        }
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let mut gaps = Vec::new();
        let mut leans = Vec::new();
        for _ in 0..30 {
            app.update();
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let (pelvis, neck) = (
                named(world, model, "pelvis"),
                named(world, model, "neck_01"),
            );
            let g = |e: Entity| *world.get::<GlobalTransform>(e).unwrap();
            let seat = g(model).affine().inverse();
            let (p, n) = (
                seat.transform_point3(g(pelvis).translation()),
                seat.transform_point3(g(neck).translation()),
            );
            gaps.push(p.y);
            leans.push((n - p).z.atan2((n - p).y).to_degrees());
        }
        assert!(
            gaps.iter().all(|g| g.abs() < 0.005),
            "deep in the saddle: {gaps:?}"
        );
        let lean = leans.iter().sum::<f32>() / leans.len() as f32;
        // Erguido: los hombros sobre las caderas (el clip de sentado se inclina
        // hacia atrás unos 5°).
        assert!(lean.abs() < 8.0, "upright: {lean}°");
    }

    /// A lo largo de `frames` frames del jinete sobre el caballo de `app`: los
    /// ángulos del codo libre (el derecho, en grados), los lugares de la mano
    /// libre en el marco del asiento del jinete, y qué tan cerca llega la mano
    /// de la piel del caballo (con signo: por debajo de cero, dentro de ella) y
    /// de la montura.
    fn free_arm(app: &mut App, me: Entity, frames: usize) -> (Vec<f32>, Vec<Vec3>, f32, f32) {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let saddle = [
            "Bajera",
            "Matra",
            "Carona",
            "Basto",
            "Pellon",
            "Vellon",
            "Sobrepuesto",
            "Cinchon",
        ];
        let (mut elbow, mut places) = (Vec::new(), Vec::new());
        let (mut skin_gap, mut saddle_gap) = (f32::MAX, f32::MAX);
        for k in 0..frames {
            app.update();
            let world = app.world();
            // A frame while the rider's scene is being created again.
            let Some(model) = try_named(world, carrier, "Rider model") else {
                continue;
            };
            let Some([upper, lower, hand, finger]) =
                ["upperarm_r", "lowerarm_r", "hand_r", "middle_02_r"]
                    .map(|n| try_named(world, model, n))
                    .into_iter()
                    .collect::<Option<Vec<Entity>>>()
                    .and_then(|v| <[Entity; 4]>::try_from(v).ok())
            else {
                continue;
            };
            let at = |e: Entity| world.get::<GlobalTransform>(e).unwrap().translation();
            elbow.push(angle_at(at(upper), at(lower), at(hand)));
            let seat = world
                .get::<GlobalTransform>(model)
                .unwrap()
                .affine()
                .inverse();
            places.push(seat.transform_point3(at(hand)));
            let points = [at(hand), at(finger)];
            if k % 3 == 0 {
                let drawn = cpu::drawn(app.world_mut(), Some(carrier));
                let (skin, tris) = horse_skin(&drawn);
                for p in points {
                    skin_gap = skin_gap.min(signed_distance(p, &skin, &tris));
                    for d in drawn.iter().filter(|d| saddle.contains(&d.name.as_str())) {
                        for q in &d.positions {
                            saddle_gap = saddle_gap.min(p.distance(*q));
                        }
                    }
                }
            }
        }
        (elbow, places, skin_gap, saddle_gap)
    }

    #[test]
    fn the_free_hand_hangs_open_and_closes_on_its_rein_at_the_gallop() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 1.5, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // The middle finger's curl at its second joint and the free wrist's
        // turn off its rest; the free (right) hand against the left on the
        // reins.
        let curls = |app: &mut App, speed: f32| {
            for _ in 0..160 {
                ride(app, me, speed);
            }
            let (mut free, mut reins, mut wrist) = (0.0f32, f32::MAX, 0.0f32);
            let mut seen = 0;
            for _ in 0..40 {
                ride(app, me, speed);
                let world = app.world();
                // A frame while the rider's scene is being created again.
                let Some(model) = try_named(world, carrier, "Rider model") else {
                    continue;
                };
                let (Some(seated), Some(right), Some(left), Some(hand)) = (
                    world.get::<Seated>(model),
                    try_named(world, model, "middle_02_r"),
                    try_named(world, model, "middle_02_l"),
                    try_named(world, model, "hand_r"),
                ) else {
                    continue;
                };
                let turn = |e: Entity| world.get::<Transform>(e).unwrap().rotation;
                free = free.max(turn(right).angle_between(Quat::IDENTITY));
                reins = reins.min(turn(left).angle_between(Quat::IDENTITY));
                wrist = wrist.max(turn(hand).angle_between(seated.loose.0));
                seen += 1;
            }
            assert!(seen > 20, "{speed} m/s: only {seen} of 40 frames measured");
            println!(
                "{speed} m/s: free finger {:.0}°, reins finger {:.0}°, free wrist {:.0}° off rest",
                free.to_degrees(),
                reins.to_degrees(),
                wrist.to_degrees()
            );
            (free, reins, wrist)
        };
        for speed in [0.0, 1.5] {
            let (free, reins, wrist) = curls(&mut app, speed);
            // Open, much less curled than the hand on the reins, the wrist
            // near its rest.
            assert!(
                free < reins * 0.5,
                "{speed} m/s: free {free}, reins {reins}"
            );
            assert!(wrist < 0.35, "{speed} m/s: wrist {wrist} off rest");
        }
        // At the gallop the right hand holds its rein: a fist like the left.
        let (free, reins, _) = curls(&mut app, 12.2);
        assert!(free > reins * 0.8, "gallop: free {free}, reins {reins}");
    }

    #[test]
    fn going_into_the_gallop_the_right_hand_takes_its_rein_beside_the_left_closed() {
        // S9: from the thigh the right hand comes to the left one on the
        // reins ([`MEET`], half way into the half-seat), closing its fingers
        // on the way, and takes the right rein there: the rein's end moves
        // from one palm to the other, so the palms must be close then.
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 1.5, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..160 {
            ride(&mut app, me, 1.5);
        }
        // On the frame the right rein changes hands: the palms apart (where
        // its end was against where it is) and the right middle finger's
        // curl over the left one's (1: a fist like the hand on the reins).
        let mut switch = None;
        let mut seen = 0;
        for _ in 0..200 {
            ride(&mut app, me, 12.2);
            let world = app.world();
            let Some(model) = try_named(world, carrier, "Rider model") else {
                continue;
            };
            let (Some(seated), Some(tack)) =
                (world.get::<Seated>(model), world.get::<Tack>(carrier))
            else {
                continue;
            };
            let found = ["middle_02_l", "middle_02_r"].map(|n| try_named(world, model, n));
            let [Some(fl), Some(fr)] = found else {
                continue;
            };
            seen += 1;
            let end = world.get::<Strap>(tack.reins[1]).unwrap().ends[1];
            if end.0 != seated.bone(Bone::Hand(1)) {
                continue;
            }
            let at = |(e, p): (Entity, Vec3)| {
                world.get::<GlobalTransform>(e).unwrap().transform_point(p)
            };
            let curl = |e: Entity| {
                world
                    .get::<Transform>(e)
                    .unwrap()
                    .rotation
                    .angle_between(Quat::IDENTITY)
            };
            let was = at(rein_end(seated, 1, false));
            switch = Some((was.distance(at(end)), curl(fr) / curl(fl).max(1e-3)));
            break;
        }
        let (apart, closed) =
            switch.unwrap_or_else(|| panic!("never changed hands ({seen} frames)"));
        println!("the right rein changes hands {apart:.3} m apart, the fist closed x{closed:.2}");
        assert!(apart < 0.1, "the rein's end jumps {apart} m");
        assert!(closed > 0.8, "the right hand closed only x{closed}");
    }

    #[test]
    /// The free (right) arm sways at the walk and keeps off the horse; since
    /// S9 it takes its own rein at the gallop (see
    /// `at_the_gallop_both_hands_hold_the_reins_...`), so the gallop only checks
    /// that it keeps off the horse.
    fn the_free_arm_sways_at_the_walk_and_keeps_off_the_horse() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let mut seen = Vec::new();
        // Primero al paso: bajar la velocidad desde el galope frenaría la mano
        // hacia la cabezada. Parado, un ciclo completo de la espera del caballo
        // (3.33 s).
        let (mut app, me) = seated_every(rider, 1.5, 20);
        for (speed, frames) in [(1.5, 60), (12.2, 40), (0.0, 170)] {
            for _ in 0..80 {
                ride(&mut app, me, speed);
            }
            let (elbow, places, skin, saddle) = free_arm(&mut app, me, frames);
            let moved = places
                .iter()
                .map(|p| p.distance(places[0]))
                .fold(0.0, f32::max);
            let mean = elbow.iter().sum::<f32>() / elbow.len() as f32;
            println!(
                "{speed} m/s: elbow {:.0}° ({:.0}° range), hand moves {moved:.3} m, {skin:.3} m off the horse, {saddle:.3} m off the saddle",
                mean,
                spread(&elbow)
            );
            // Nunca dentro del caballo ni de la montura.
            assert!(skin > 0.02, "{speed} m/s: the hand {skin} m off the horse");
            assert!(
                saddle > 0.02,
                "{speed} m/s: the hand {saddle} m off the saddle"
            );
            seen.push((spread(&elbow), moved, mean));
        }
        let [(_, walk_moves, walk_mean), _, _] = seen[..] else {
            unreachable!()
        };
        // El paso la balancea, poco.
        assert!(walk_moves > 0.003, "the walk sways it: {walk_moves}");
        assert!(
            walk_moves < FREE_REACH + 0.05,
            "never flailing: {walk_moves}"
        );
        // En reposo el codo está doblado unos 30–60° (un ángulo de 120–150°).
        assert!((120.0..150.0).contains(&walk_mean), "{walk_mean}°");
    }

    #[test]
    fn getting_off_leaves_nothing_of_the_horse_and_getting_on_again_seats_the_rider() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated(rider, 0.0);
        let count = |app: &mut App| {
            let world = app.world_mut();
            (
                world.query::<&HorsePiece>().iter(world).count(),
                world.query::<&Riding>().iter(world).count(),
                world.query::<&HorseModel>().iter(world).count(),
            )
        };
        assert_eq!(count(&mut app), (38, 1, 1));
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Figure(FigureKind::Protagonist));
        app.update();
        assert_eq!(count(&mut app), (0, 0, 0), "nothing of the horse left");
        // De vuelta en la montura: el caballo nuevo se prepara y sienta a su
        // jinete.
        app.world_mut().entity_mut(me).insert(rider);
        for _ in 0..200 {
            app.update();
            let world = app.world_mut();
            let seated = world
                .query::<(&Rig, &Seated)>()
                .iter(world)
                .any(|(r, _)| r.started);
            if seated {
                break;
            }
        }
        app.update();
        assert_eq!(count(&mut app), (38, 1, 1));
        let world = app.world_mut();
        let (riding, seated) = world.query::<(&Riding, &Seated)>().single(world).unwrap();
        let (horse, hand) = (
            riding.horse,
            seated.bones[Bone::ALL.iter().position(|b| *b == Bone::Hand(0)).unwrap()],
        );
        let tack = *world.get::<Tack>(horse).unwrap();
        assert_eq!(world.get::<Strap>(tack.reins[0]).unwrap().ends[1].0, hand);
    }

    #[test]
    fn the_riders_poncho_stays_over_his_shoulder_and_arm() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 0.0, 20);
        for (speed, frames) in [(0.0, 10), (1.5, 30), (12.2, 30)] {
            for _ in 0..80 {
                ride(&mut app, me, speed);
            }
            let carrier = app.world().get::<Children>(me).unwrap()[0];
            let mut deepest = 0.0f32;
            for _ in 0..frames {
                app.update();
                // A frame while the rider's scene is being created again.
                let Some(model) = try_named(app.world(), carrier, "Rider model") else {
                    continue;
                };
                deepest = deepest.max(poncho_in_arms(app.world_mut(), model));
            }
            println!("{speed} m/s: the poncho {deepest:.3} m into an arm");
            assert!(
                deepest < 0.005,
                "{speed} m/s: the poncho {deepest} m into an arm"
            );
        }
    }

    /// Si `e` es `ancestor` o cuelga de él.
    fn under(world: &World, mut e: Entity, ancestor: Entity) -> bool {
        loop {
            if e == ancestor {
                return true;
            }
            let Some(parent) = world.get::<ChildOf>(e) else {
                return false;
            };
            e = parent.parent();
        }
    }

    #[test]
    fn a_pillion_sits_on_the_croup_behind_the_rider_and_gets_off() {
        let (mut app, me) = seated(
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            },
            0.0,
        );
        app.world_mut()
            .entity_mut(me)
            .insert(Pillion(FigureKind::Bairoletto));
        let pillion = (0..3000)
            .find_map(|_| {
                app.update();
                let world = app.world_mut();
                let found = world
                    .query_filtered::<(Entity, &Rig), With<Behind>>()
                    .iter(world)
                    .find(|(_, r)| r.started)
                    .map(|(e, _)| e);
                if found.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                found
            })
            .expect("the pillion never sat");
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let tack = *world.get::<Tack>(carrier).unwrap();
        // Sobre su asiento, colgado de la grupa del caballo, y sin riendas:
        // siguen en la mano del jinete.
        let riding = *world.get::<Riding>(pillion).unwrap();
        let seated = world.get::<Seated>(pillion).unwrap().clone();
        assert_eq!(riding.kind, FigureKind::Bairoletto);
        assert_eq!(
            world.get::<ChildOf>(pillion).unwrap().parent(),
            riding.horse
        );
        assert_eq!(
            world.get::<ChildOf>(riding.horse).unwrap().parent(),
            tack.croup
        );
        assert_eq!(
            world.get::<Name>(tack.croup).unwrap().as_str(),
            crate::character::bones::HorseJoint::Croup.name()
        );
        let front = world
            .query_filtered::<Entity, (With<Riding>, Without<Behind>)>()
            .single(world)
            .unwrap();
        let waist = world.get::<Seated>(front).unwrap().bone(Bone::Spine1);
        let arms = [Bone::UpperArm(0), Bone::UpperArm(1)].map(|b| seated.bone(b));
        let reins = world.get::<Strap>(tack.reins[0]).unwrap().ends[1].0;
        assert!(under(world, reins, front), "the reins stay with the rider");
        // Parado, al paso y al galope: sobre la piel de la grupa (la tela de
        // la bombacha apenas contra el cuero, como el jinete en la lana);
        // pegado al jinete sin entrar en él, salvo los brazos, que lo abrazan,
        // con las muñecas en su faja (por celdas de 5 cm, el ala del sombrero
        // apoyada en su hombro derecho cuenta hasta 1.9 cm al galope; sin
        // enderezarse contra la grupa entraba 6 cm); y con los hombros sobre
        // el pecho, no echados atrás. El tiempo es simulado: la carga solo
        // cambia en qué fase del jinete se sienta, y en 21 fases (un segundo,
        // más de dos trancos de galope) el ala no pasó de 1.9 cm.
        for speed in [0.0, 1.6, 8.0] {
            ride(&mut app, me, speed);
            let (mut sunk, mut gap) = (f32::MAX, f32::MAX);
            let (mut grip, mut shoulders) = (0.0f32, f32::MAX);
            for _ in 0..60 {
                app.update();
                let world = app.world_mut();
                let body = world
                    .get::<GlobalTransform>(carrier)
                    .unwrap()
                    .affine()
                    .inverse();
                // Las muñecas en la cintura del jinete, y los hombros sobre el
                // pecho, no detrás (en el espacio del modelo: +Z al frente).
                let global = |e: Entity| world.get::<GlobalTransform>(e).unwrap().affine();
                let (rider, spine) = (global(front), global(waist).translation);
                let mine = global(pillion).inverse();
                let chest =
                    mine.transform_point3(global(seated.bone(Bone::Spine3)).translation.into());
                for (s, at) in PILLION_HOLD.iter().enumerate() {
                    let hold = rider
                        .transform_point3(rider.inverse().transform_point3(spine.into()) + *at);
                    let wrist = global(seated.bone(Bone::Hand(s))).translation;
                    grip = grip.max(hold.distance(wrist.into()));
                    let shoulder = global(seated.bone(Bone::UpperArm(s))).translation;
                    shoulders = shoulders.min(mine.transform_point3(shoulder.into()).z - chest.z);
                }
                let drawn = crate::character::tack::tests::drawn_on_their_joints(world, me);
                let (skin, tris) = horse_skin(&drawn);
                let near = crate::character::tack::tests::Near::new(&skin, &tris);
                // Por celdas de 5 cm de alto y de ancho: lo más adelante del de
                // ancas y lo más atrás del jinete, a lo largo del caballo.
                let mut ahead = std::collections::HashMap::<(i32, i32), f32>::new();
                let mut rear = std::collections::HashMap::<(i32, i32), f32>::new();
                for d in &drawn {
                    let back = under(world, d.entity, pillion);
                    if !back && !under(world, d.entity, front) {
                        continue;
                    }
                    for (k, p) in d.positions.iter().enumerate().step_by(3) {
                        let q = body.transform_point3(*p);
                        let band = ((q.y / 0.05).floor() as i32, (q.z / 0.05).floor() as i32);
                        // Los brazos lo abrazan: van por delante a propósito.
                        let arm = d.strongest[k]
                            .is_some_and(|j| arms.iter().any(|a| under(world, j, *a)));
                        if back {
                            if !arm {
                                let a = ahead.entry(band).or_insert(f32::MIN);
                                *a = a.max(q.x);
                            }
                            if let Some(s) = near.signed_distance(*p) {
                                sunk = sunk.min(s);
                            }
                        } else {
                            let r = rear.entry(band).or_insert(f32::MAX);
                            *r = r.min(q.x);
                        }
                    }
                }
                for (band, a) in &ahead {
                    if let Some(r) = rear.get(band) {
                        gap = gap.min(r - a);
                    }
                }
            }
            println!(
                "{speed} m/s: the pillion {sunk:.3} m from the skin, {gap:.3} m behind the rider, wrists {grip:.3} m from the waist, shoulders {shoulders:.3} m ahead of the chest",
            );
            assert!(sunk > -0.015, "{speed} m/s: {sunk} m into the horse");
            assert!(gap > -0.025, "{speed} m/s: {gap} m into the rider");
            assert!(
                grip < 0.04,
                "{speed} m/s: the wrists {grip} m off the waist"
            );
            assert!(
                shoulders > 0.05,
                "{speed} m/s: the shoulders {shoulders} m ahead of the chest"
            );
        }
        // Al quitarlo se baja: ni su modelo ni su asiento.
        app.world_mut().entity_mut(me).remove::<Pillion>();
        app.update();
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Behind>().iter(world).count(), 0);
        assert!(world.get::<PillionSeat>(carrier).is_none());
        assert!(world.get_entity(riding.horse).is_err());
    }
}
