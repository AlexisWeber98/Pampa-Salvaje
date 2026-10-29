//! El jinete sobre el caballo modelo: un modelo de MakeHuman (ver
//! [`model`](super::model)) sentado sobre el hueso `Torso` del caballo (ver
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
//!   ([`REINS_HAND`]), sobre la cruz al galope ([`GALLOP_HAND`]), y sigue la
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

use super::figure::FigureKind;
use super::horse::{HorseBody, HorseModel, transform_of};
use super::model::{CharacterModel, CharacterModels, ModelPending};
use super::rig::Rig;
use super::tack::{STIRRUP, Strap, Tack};

/// El modelo de un jinete, sentado en un caballo: en la raíz del modelo, hijo
/// del hueso `Torso` del caballo.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Riding {
    /// El cuerpo del caballo.
    pub horse: Entity,
    pub kind: FigureKind,
    pub seed: u32,
}

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

/// Adónde van los tobillos, primero el izquierdo y después el derecho, desde la
/// pelvis (espacio del jinete): en los estribos, 0.3 m hacia afuera a cada lado
/// (el costillar está a 0.23 m de la columna en su parte más ancha, y las botas
/// deben librar los faldones), 10 cm adelante, con las suelas de las botas unos
/// 12 cm por debajo de la panza.
pub const ANKLES: [Vec3; 2] = [Vec3::new(0.32, -0.69, 0.1), Vec3::new(-0.32, -0.69, 0.1)];

/// Adónde van los tobillos en medio asiento: los estribos acortados un palmo
/// (las aciones siguen a los pies) y los talones bajo las caderas, así que las
/// rodillas se doblan unos 110°.
pub const GALLOP_ANKLES: [Vec3; 2] = [Vec3::new(0.31, -0.6, 0.02), Vec3::new(-0.31, -0.6, 0.02)];

/// Adónde va la muñeca de la mano de las riendas (la izquierda: un gaucho monta
/// con una sola mano, con ambas riendas en ella): delante de la cabezada, un
/// poco hacia su costado, para que el brazo no toque el poncho doblado sobre
/// ese hombro.
pub const REINS_HAND: Vec3 = Vec3::new(0.12, 0.18, 0.32);

/// Dónde descansa después de unos segundos quieto: sobre la cabezada.
pub const POMMEL_HAND: Vec3 = Vec3::new(0.06, -0.02, 0.22);

/// Dónde descansa la mano libre (la derecha): sobre su muslo, como si
/// sostuviera un rebenque sin apretar, con el codo doblado unos 50°.
pub const THIGH_HAND: Vec3 = Vec3::new(-0.2, 0.08, 0.1);

/// Adónde va, de vez en cuando, estando quieto: al ala del sombrero.
pub const HAT_HAND: Vec3 = Vec3::new(-0.1, 0.62, 0.16);

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
pub const COUNTER_EASE: f32 = 40.0;
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
/// `pampa_core::RiderParams::walk_max`.
pub const WALK_TOP: f32 = 3.0;

/// El desplazamiento del peso estando quieto: su período, en segundos, es el
/// primero más la semilla del jinete módulo el segundo (7–9 s), y su amplitud,
/// en radianes, el tercero.
pub const SHIFT: (f32, u32, f32) = (7.0, 3, 0.035);

/// Cuánto cede como máximo la mano de las riendas con el freno, en metros.
pub const GIVE_MAX: f32 = 0.12;

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

/// La mirada más amplia que da `pampa_core::pose::glance`, en radianes (0.3 +
/// 0.25): una mirada así de amplia es todo el camino hasta el ala del sombrero.
/// Acoplado a esa función.
pub const GLANCE_MAX: f32 = 0.55;

/// Hacia dónde se doblan las rodillas, primero la izquierda y después la
/// derecha: hacia adelante y hacia afuera alrededor del costillar.
pub const KNEE_POLE: [Vec3; 2] = [Vec3::new(0.6, -0.1, 1.0), Vec3::new(-0.6, -0.1, 1.0)];

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

/// Adónde va la mano de las riendas en medio asiento: adelante sobre la cruz y
/// un poco por encima, sin tocar el cuello del caballo.
pub const GALLOP_HAND: Vec3 = Vec3::new(0.04, 0.22, 0.4);

/// Cuánto del movimiento del freno sigue la mano de las riendas, al paso y en
/// medio asiento: el contacto se mantiene parejo.
pub const FOLLOW: (f32, f32) = (0.35, 0.85);

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
pub const JUMP_HAND: Vec3 = Vec3::new(0.0, -0.06, 0.16);

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
        Some(Self { stirrups, ..seated })
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
}

/// Lo que un jinete lee de su caballo: las cadenas desde la raíz del modelo
/// hasta su `Torso` y su `Head` (las ubicaciones en el marco del cuerpo de la
/// montura y del freno), y el centro del freno en la cabeza. En el cuerpo del
/// caballo.
#[derive(Component, Debug, Clone)]
pub struct Saddle {
    pub torso: Vec<Entity>,
    pub head: Vec<Entity>,
    pub bit: Vec3,
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

/// El cabeceo de la montura (positivo con la nariz hacia arriba) y su rolido
/// (positivo con su lado derecho hacia arriba), en radianes, a partir del giro
/// del hueso `Torso` en el marco del cuerpo del caballo: el hueso recorre su
/// propio +Y hacia el frente del caballo, y su +X hacia la derecha del caballo.
pub fn saddle_attitude(tilt: Quat) -> (f32, f32) {
    (
        (tilt * Vec3::Y).y.clamp(-1.0, 1.0).asin(),
        (tilt * Vec3::X).y.clamp(-1.0, 1.0).asin(),
    )
}

/// Ajusta cada jinete que se reproduce a su caballo (ver las notas del módulo),
/// después de que los clips lo pusieron en pose (a él y al caballo) y antes de
/// que se calculen los transforms.
pub(super) fn pose_riders(
    time: Res<Time>,
    horses: Query<(&HorseBody, &Saddle)>,
    mut riders: Query<(
        Entity,
        &Rig,
        &Riding,
        &Seated,
        &mut RiderMotion,
        &mut Visibility,
    )>,
    mut transforms: Bones,
) {
    let dt = time.delta_secs();
    for (me, rig, riding, seated, mut m, mut shown) in &mut riders {
        if rig.frozen || !rig.started {
            continue;
        }
        // En pose a partir de aquí: se dibuja.
        shown.set_if_neq(Visibility::Inherited);
        let Ok((horse, saddle)) = horses.get(riding.horse) else {
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
        let (saddle_pitch, saddle_roll) = saddle_attitude(tilt);
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
        // caballo: su cabeceo desde que dejó el suelo (el hueso `Torso` recorre
        // su propio +Y, hacia adelante) se absorbe en las caderas, entrando y
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
        // El medio asiento entra con el galope, con retraso.
        let half = m.half.step(gallop, HALF_SEAT_EASE, dt).clamp(0.0, 1.0);
        let walking = (horse.speed / WALK_TOP).min(1.0) * (1.0 - gallop);
        let lean = m.lean.step(
            LEAN.0 * half + LEAN.1 * walking - LEAN.2 * braking,
            LEAN_EASE,
            dt,
        ) + JUMP_FOLD * fold
            - LANDING_GIVE.0 * give
            + balance;
        // Sobre un salto las caderas absorben el cabeceo (`balance`), no la
        // columna.
        let counter_pitch = m.counter[0].step(COUNTER.0 * pitch * (1.0 - poise), COUNTER_EASE, dt);
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
            Quat::from_rotation_y(glance) * into_rider(Quat::from_rotation_z(share * level), frame)
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
        let slack = slack.clamp_length_max(GIVE_MAX) * follow * (1.0 - rest);
        let reins = REINS_HAND.lerp(GALLOP_HAND, half).lerp(POMMEL_HAND, rest) + JUMP_HAND * fold;
        let free = free_hand(
            &mut m,
            (rise, pitch, roll),
            (walking, gallop, braking, rest),
            dt,
        );
        let hands = [reins + slack, seat + free];
        // Los hombros acompañan al torso, con retraso: hacia arriba cuando la
        // montura baja, hacia adelante cuando cabecea hacia abajo.
        let shrug = m.shrug.step(SHRUG * (pitch - 2.0 * rise), SHRUG_EASE, dt);
        for s in 0..2 {
            let side = if s == 0 { 1.0 } else { -1.0 };
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
            let arm = [Bone::UpperArm(s), Bone::LowerArm(s), Bone::Hand(s)].map(|b| seated.bone(b));
            reach(&mut transforms, shoulder, arm, hands[s], ELBOW_POLE[s]);
        }
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
    let brim = (pampa_core::pose::glance(m.seed ^ HAT_SALT, m.clock * HAT_PACE).abs() / GLANCE_MAX)
        .min(1.0);
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
            head: chain(tack.head),
            bit: tack.bit,
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
                let apart = Vec3::X * (if s == 0 { -0.012 } else { 0.012 });
                rein.ends[1] = (seated.bone(Bone::Hand(0)), PALM + apart);
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
    fn the_saddle_pitches_with_the_horses_nose_and_rolls_with_its_sides() {
        // El hueso `Torso` en reposo: recorre su propio +Y hacia el frente del
        // caballo (+X), su +X hacia la derecha del caballo (+Z), y su +Z hacia
        // arriba.
        let rest = Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::X, Vec3::Y));
        for angle in [-0.4f32, -0.1, 0.1, 0.4] {
            // La nariz hacia arriba (alrededor del eje lateral del cuerpo, +Z)
            // es solo un cabeceo.
            let (pitch, roll) = saddle_attitude(Quat::from_rotation_z(angle) * rest);
            assert!((pitch - angle).abs() < 1e-4, "{angle}: pitch {pitch}");
            assert!(roll.abs() < 1e-4, "{angle}: roll {roll}");
            // El lado derecho hacia arriba (alrededor del eje longitudinal del
            // cuerpo, +X), solo un rolido.
            let (pitch, roll) = saddle_attitude(Quat::from_rotation_x(-angle) * rest);
            assert!(pitch.abs() < 1e-4, "{angle}: pitch {pitch}");
            assert!((roll - angle).abs() < 1e-4, "{angle}: roll {roll}");
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

    // --- con el caballo y el jinete incluidos --------------------------------

    use crate::character::cpu;
    use crate::character::horse::tests::{named, ride, saddled_every};
    use crate::character::tack::tests::{horse_skin, signed_distance};
    use crate::character::tack::{HorsePiece, Strap, Tack};
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
        assert_eq!(world.get::<Name>(seat.parent()).unwrap().as_str(), "Torso");
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
            let torso = named(world, carrier, "Torso");
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
                named(world, carrier, "Torso"),
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
        for _ in 0..40 {
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
        assert!(mean(top, 2) > reach + 0.1, "{} vs {reach}", mean(top, 2));
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
        for _ in 0..80 {
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
            let [torso, horse_head] = ["Torso", "Head"].map(|n| named(world, carrier, n));
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
        assert!(spread(&reach) < 0.08, "hand to bit {}", spread(&reach));
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
        let saddle = ["Sudadera", "Carona", "Basto", "Pellon"];
        let (mut elbow, mut places) = (Vec::new(), Vec::new());
        let (mut skin_gap, mut saddle_gap) = (f32::MAX, f32::MAX);
        for k in 0..frames {
            app.update();
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let [upper, lower, hand, finger] =
                ["upperarm_r", "lowerarm_r", "hand_r", "middle_02_r"]
                    .map(|n| named(world, model, n));
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
    fn the_free_arm_swings_with_the_gallop_less_at_the_walk_and_keeps_off_the_horse() {
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
        let [
            (walk_bend, walk_moves, walk_mean),
            (gallop_bend, gallop_moves, _),
            _,
        ] = seen[..]
        else {
            unreachable!()
        };
        // El galope la balancea, flexionando el codo; el paso menos, pero algo.
        assert!(
            gallop_bend > 6.0 && gallop_moves > 0.03,
            "{gallop_bend}° {gallop_moves} m"
        );
        assert!(
            gallop_moves < FREE_REACH + 0.05,
            "never flailing: {gallop_moves}"
        );
        assert!(walk_moves < gallop_moves && walk_bend < gallop_bend);
        assert!(walk_moves > 0.003, "the walk sways it: {walk_moves}");
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
        assert_eq!(count(&mut app), (22, 1, 1));
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
        assert_eq!(count(&mut app), (22, 1, 1));
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
                let model = named(app.world(), carrier, "Rider model");
                deepest = deepest.max(poncho_in_arms(app.world_mut(), model));
            }
            println!("{speed} m/s: the poncho {deepest:.3} m into an arm");
            assert!(
                deepest < 0.005,
                "{speed} m/s: the poncho {deepest} m into an arm"
            );
        }
    }
}
