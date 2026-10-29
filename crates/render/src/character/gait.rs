//! Los clips del caballo ensillado, ajustados después de los sistemas de
//! animación (ver [`horse`](super::horse)): todo lo que los clips dejan sin
//! hacer o se quedan cortos.
//!
//! - **Mezcla** ([`complete_blend`]). Bevy pondera cada propiedad de un hueso
//!   solo entre los clips que la animan: una propiedad que solo anima la
//!   caminata (el balanceo del lomo, los objetivos de IK de los vasos)
//!   mantendría la caminata con toda su fuerza mientras la caminata se
//!   desvanece, plegando las patas. Cada una de esas propiedades se devuelve
//!   suavemente hacia su reposo según el peso de los clips que no la animan
//!   ([`PARTIAL`]).
//! - **Un galope más largo.** El galope de Quaternius tiene una zancada corta
//!   (unos 2.3 m por ciclo): reproducido lo bastante rápido como para no
//!   deslizarse a galope tendido, parece frenético. Al galope, el recorrido de
//!   cada pata alrededor de su propia media ([`GALLOP_MEANS`]) se estira por
//!   [`STRIDE`], así que el clip cubre más terreno por ciclo y se reproduce más
//!   lento.
//! - **Un cuerpo que se flexiona** ([`articulation`]), sincronizado con el
//!   ciclo del galope: la grupa baja cuando las patas traseras llegan bajo la
//!   panza y sube en el impulso, el lomo se arquea y se estira en contra, así
//!   que la cruz cabecea en sentido opuesto, el cuello bombea contra el tercio
//!   anterior y la cabeza se estabiliza, la cadera rola y gira un poco con la
//!   mano de galope, y la cola sigue con retraso. La caminata se deja como
//!   está.
//! - **Los vasos se quedan en las patas, por encima del suelo.** Una pata cuyo
//!   extremo queda bajo el suelo por una mezcla o por el estiramiento se pliega
//!   en la rodilla o el corvejón lo justo ([`lift_legs`]). Los vasos del modelo
//!   cuelgan de objetivos de IK, no de las patas, así que después cada vaso se
//!   coloca en el extremo de su pata ([`reattach`]), nunca por debajo de donde
//!   estaría parado.
//! - **Un salto en el aire.** Mientras se reproduce el clip de salto
//!   ([`level_flight`]) la flexión del galope se atenúa según su peso y los
//!   vasos se dejan como los recoge el clip ([`JUMP_FREE`]); se quita la
//!   elevación propia del clip en el hueso `Body` (y con ella la de los
//!   objetivos de IK de los vasos), ya que la entidad ya vuela el arco de la
//!   física. Además del clip, las patas traseras impulsan al caballo,
//!   estirándose hacia atrás y hacia abajo al dejar el suelo ([`push`],
//!   [`PUSH`]), y las manos se recogen un poco más en lo alto ([`tuck`],
//!   [`TUCK`]), con cada vaso devuelto al extremo de su pata.
//!
//! Todo es determinista, se calcula de nuevo en cada frame a partir de la pose
//! de los clips, y no reserva memoria.

use bevy::math::Affine3A;
use bevy::prelude::*;

use crate::fauna::livestock::{CLIP_COUNT, Clip};

/// Qué parte del transform de un hueso anima un clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Rotation,
    Translation,
}

/// Las propiedades que no anima cada clip, con los clips que sí lo hacen (el
/// bit `k` para [`Clip::ALL`]`[k]`), tal como las tiene `horse.glb`; todas las
/// demás propiedades animadas están en todos los clips.
pub const PARTIAL: [(&str, Property, u8); 26] = [
    ("Back", Property::Rotation, 0b0010000),
    ("Back", Property::Translation, 0b1110000),
    ("BackShoulder.L", Property::Translation, 0b1100010),
    ("BackShoulder.R", Property::Translation, 0b1100000),
    ("Ear2.L", Property::Rotation, 0b1101111),
    ("Ear2.R", Property::Rotation, 0b1101111),
    ("Ear3.L", Property::Rotation, 0b1100100),
    ("Ear3.R", Property::Rotation, 0b1100100),
    ("Ear4.L", Property::Rotation, 0b1110000),
    ("Ear4.R", Property::Rotation, 0b1110000),
    ("FrontShoulder.L", Property::Translation, 0b1111101),
    ("FrontShoulder.R", Property::Translation, 0b1110000),
    ("IKBackLeg.L", Property::Rotation, 0b1110000),
    ("IKBackLeg.L", Property::Translation, 0b1110010),
    ("IKBackLeg.R", Property::Rotation, 0b1110100),
    ("IKBackLeg.R", Property::Translation, 0b1110100),
    ("IKFrontLeg.L", Property::Rotation, 0b1110100),
    ("IKFrontLeg.L", Property::Translation, 0b1111101),
    ("IKFrontLeg.R", Property::Rotation, 0b1110000),
    ("IKFrontLeg.R", Property::Translation, 0b1110000),
    ("Neck3", Property::Rotation, 0b1110011),
    ("PoleTarget.L", Property::Translation, 0b1000000),
    ("PoleTarget.R", Property::Translation, 0b1000000),
    ("Tail2", Property::Rotation, 0b1100000),
    ("Torso2", Property::Rotation, 0b0010000),
    ("Torso3", Property::Rotation, 0b0001000),
];

/// Los huesos que ningún clip gira pero que gira [`fit_horse`]: se devuelven
/// primero al reposo en cada frame, o los giros se acumularían.
pub const UNANIMATED: [&str; 2] = ["Torso", "Tail1"];

/// La parte de una propiedad `mask` (ver [`PARTIAL`]) que animan los clips que
/// se reproducen con `weights`: el resto queda en reposo.
pub fn animated_share(mask: u8, weights: &[f32; CLIP_COUNT]) -> f32 {
    Clip::ALL
        .iter()
        .filter(|c| mask & (1 << c.index()) != 0)
        .map(|c| weights[c.index()])
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

/// `value` (lo que fijan los clips) llevado suavemente hacia `rest` según la
/// parte que no tienen los clips que lo fijan.
pub fn complete_blend(property: Property, rest: &Transform, value: &mut Transform, share: f32) {
    match property {
        Property::Rotation => value.rotation = rest.rotation.slerp(value.rotation, share),
        Property::Translation => {
            value.translation = rest.translation.lerp(value.translation, share);
        }
    }
}

/// Cuánto más larga es la zancada del galope que la del clip.
pub const STRIDE: f32 = 1.4;

/// El giro medio de cada hueso superior de pata que se balancea a lo largo del
/// ciclo del galope (su propia rotación local, medida en `horse.glb`): la
/// zancada se estira alrededor de él. Las partes inferiores de las patas se
/// pliegan como las pliega el clip.
pub const GALLOP_MEANS: [(&str, [f32; 4]); 8] = [
    ("FrontUpperLeg.L", [0.72688, 0.63446, 0.17364, 0.1974]),
    ("FrontUpperLeg.R", [0.72373, -0.63646, -0.17442, 0.20176]),
    ("BackShoulder.L", [-0.68568, -0.56806, -0.27013, 0.3663]),
    ("BackShoulder.R", [0.69222, -0.5585, -0.25075, -0.38215]),
    ("BackLeg.L", [0.73969, 0.63967, 0.12762, 0.16553]),
    ("BackLeg.R", [0.73885, -0.64136, -0.12789, 0.16247]),
    ("BackUpperLeg.L", [0.65989, -0.00073, 0.04062, 0.75027]),
    ("BackUpperLeg.R", [0.64587, 0.00303, -0.0341, 0.76268]),
];

/// `rotation` balanceada `k` veces más lejos de `mean`.
pub fn stretch(rotation: Quat, mean: Quat, k: f32) -> Quat {
    let mut delta = mean.inverse() * rotation;
    if delta.w < 0.0 {
        delta = -delta;
    }
    let (axis, angle) = delta.to_axis_angle();
    (mean * Quat::from_axis_angle(axis, angle * k)).normalize()
}

/// En qué punto del ciclo del galope (`0..1`) las patas traseras llegan más
/// lejos bajo la panza y apoya la trasera izquierda: ahí la grupa está más
/// baja.
pub const GATHER: f32 = 0.57;

/// El movimiento propio del cuerpo al galope, en radianes, en la fase `phase`
/// de su ciclo (`0..1`): cabeceos alrededor del eje lateral del cuerpo
/// (positivos con la nariz o el extremo delantero hacia arriba) del tercio
/// posterior (`Back`), de cada hueso del lomo, del cuello y de la cabeza, el
/// rolido y el giro de la cadera, y la elevación de la cola.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Articulation {
    pub back: f32,
    pub torso: [f32; 3],
    pub neck: f32,
    pub head: f32,
    pub roll: f32,
    pub yaw: f32,
    pub tail: f32,
}

/// El cabeceo de la grupa hacia cada lado al galope, en radianes (unos 7°).
pub const CROUP: f32 = 0.12;

/// La [`Articulation`] en la fase `phase` del galope. El tercio posterior es el
/// que más cabecea; los huesos del lomo se doblan en contra, cada uno un poco
/// más hacia la cruz, así que el tercio anterior cabecea en sentido opuesto; el
/// cuello bombea contra el tercio anterior y la cabeza deshace la mitad; la
/// cola va un quinto de ciclo atrasada.
pub fn articulation(phase: f32) -> Articulation {
    use std::f32::consts::TAU;
    let gather = (TAU * (phase - GATHER)).cos();
    let lead = (TAU * (phase - GATHER)).sin();
    let back = CROUP * gather;
    let neck = 0.6 * CROUP * gather;
    Articulation {
        back,
        torso: [-0.7 * back, -0.5 * back, -0.3 * back],
        neck,
        head: -0.5 * neck,
        roll: 0.04 * lead,
        yaw: 0.03 * lead,
        tail: 0.25 * (TAU * (phase - GATHER - 0.2)).cos(),
    }
}

/// Los huesos del caballo que mueve este módulo, por nombre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bone {
    Armature,
    Body,
    Back,
    Torso,
    Torso2,
    Torso3,
    Neck1,
    Neck2,
    Neck3,
    Head,
    Tail1,
    /// El hueso superior de cada pata: el hombro delantero o la cadera trasera.
    Shoulder(usize),
    UpperLeg(usize),
    /// El hueso extra de la pata trasera entre la cadera y el muslo.
    Leg(usize),
    LowerLeg(usize),
    End(usize),
    Hoof(usize),
}

/// Las patas, en el orden de índice de pata de [`Bone`].
pub const LEGS: [&str; 4] = ["Front?.L", "Front?.R", "Back?.L", "Back?.R"];

impl Bone {
    pub const COUNT: usize = 11 + 6 * 4;

    pub fn index(self) -> usize {
        match self {
            Self::Armature => 0,
            Self::Body => 1,
            Self::Back => 2,
            Self::Torso => 3,
            Self::Torso2 => 4,
            Self::Torso3 => 5,
            Self::Neck1 => 6,
            Self::Neck2 => 7,
            Self::Neck3 => 8,
            Self::Head => 9,
            Self::Tail1 => 10,
            Self::Shoulder(l) => 11 + l,
            Self::UpperLeg(l) => 15 + l,
            Self::Leg(l) => 19 + l,
            Self::LowerLeg(l) => 23 + l,
            Self::End(l) => 27 + l,
            Self::Hoof(l) => 31 + l,
        }
    }

    /// El nombre de su nodo en `horse.glb`.
    pub fn name(self) -> String {
        let leg = |l: usize, part: &str| LEGS[l].replace('?', part);
        match self {
            Self::Armature => "AnimalArmature".into(),
            Self::Body => "Body".into(),
            Self::Back => "Back".into(),
            Self::Torso => "Torso".into(),
            Self::Torso2 => "Torso2".into(),
            Self::Torso3 => "Torso3".into(),
            Self::Neck1 => "Neck1".into(),
            Self::Neck2 => "Neck2".into(),
            Self::Neck3 => "Neck3".into(),
            Self::Head => "Head".into(),
            Self::Tail1 => "Tail1".into(),
            Self::Shoulder(l) => leg(l, "Shoulder"),
            Self::UpperLeg(l) if l < 2 => leg(l, "UpperLeg"),
            Self::UpperLeg(l) => leg(l, "UpperLeg"),
            // Las patas delanteras no tienen ese hueso: su hombro ocupa su
            // lugar.
            Self::Leg(l) if l < 2 => leg(l, "Shoulder"),
            Self::Leg(l) => leg(l, "Leg"),
            Self::LowerLeg(l) => leg(l, "LowerLeg"),
            Self::End(l) => format!("{}_end", leg(l, "LowerLeg")),
            Self::Hoof(l) if l < 2 => format!("IKFrontLeg.{}", ["L", "R"][l]),
            Self::Hoof(l) => format!("IKBackLeg.{}", ["L", "R"][l - 2]),
        }
    }

    /// Todos los huesos, por [`Self::index`].
    pub fn all() -> [Self; Self::COUNT] {
        let mut out = [Self::Armature; Self::COUNT];
        let fixed = [
            Self::Armature,
            Self::Body,
            Self::Back,
            Self::Torso,
            Self::Torso2,
            Self::Torso3,
            Self::Neck1,
            Self::Neck2,
            Self::Neck3,
            Self::Head,
            Self::Tail1,
        ];
        for b in fixed {
            out[b.index()] = b;
        }
        for l in 0..4 {
            for b in [
                Self::Shoulder(l),
                Self::UpperLeg(l),
                Self::Leg(l),
                Self::LowerLeg(l),
                Self::End(l),
                Self::Hoof(l),
            ] {
                out[b.index()] = b;
            }
        }
        out
    }

    /// La cadena de la pata desde el cuerpo hasta su extremo (las patas
    /// delanteras cuelgan de la cruz, las traseras del tercio posterior).
    fn chain(leg: usize) -> [Self; 8] {
        if leg < 2 {
            [
                Self::Body,
                Self::Back,
                Self::Torso,
                Self::Torso2,
                Self::Shoulder(leg),
                Self::UpperLeg(leg),
                Self::LowerLeg(leg),
                Self::End(leg),
            ]
        } else {
            [
                Self::Body,
                Self::Back,
                Self::Shoulder(leg),
                Self::Leg(leg),
                Self::UpperLeg(leg),
                Self::LowerLeg(leg),
                Self::End(leg),
                Self::End(leg),
            ]
        }
    }
}

/// Los huesos y reposos del modelo de un caballo, ubicados cuando se crea su
/// escena; en su raíz junto a su [`HorseRig`](super::horse::HorseRig).
#[derive(Component, Debug, Clone)]
pub struct HorseBones {
    /// Por [`Bone::index`].
    pub bones: [Entity; Bone::COUNT],
    /// La ubicación del armature en el marco del cuerpo (la raíz del modelo y
    /// el nodo raíz de la escena por encima de ella).
    pub armature: Affine3A,
    /// El hueso de cada propiedad animada en parte, su reposo y su máscara
    /// (también los giros de los huesos [`UNANIMATED`], sin ningún clip en la
    /// suya).
    pub partial: Vec<(Entity, Property, u8, Transform)>,
    /// Cada hueso de pata estirado, su pata (según [`LEGS`]) y su giro medio al
    /// galope.
    pub means: Vec<(usize, Entity, Quat)>,
    /// A qué altura está en reposo el objetivo de cada vaso en el marco del
    /// cuerpo.
    pub standing: [f32; 4],
    /// La traslación de reposo del hueso `Body` (en el espacio del armature):
    /// la elevación del clip de salto se mide a partir de ella.
    pub body_rest: Vec3,
}

/// La ubicación de `bone` en el espacio del armature, a partir de `transforms`.
fn placed(bones: &HorseBones, chain: &[Bone], transforms: &Query<&mut Transform>) -> Affine3A {
    let mut at = Affine3A::IDENTITY;
    let mut last = None;
    for &b in chain {
        if last == Some(b) {
            continue;
        }
        last = Some(b);
        if let Ok(t) = transforms.get(bones.bones[b.index()]) {
            at *= t.compute_affine();
        }
    }
    at
}

/// La parte de rotación de una ubicación con escala uniforme.
fn turn(a: &Affine3A) -> Quat {
    let (_, rotation, _) = a.to_scale_rotation_translation();
    rotation
}

/// Gira `bone` por `delta`, un giro en el marco del cuerpo (llevado al espacio
/// del armature por `armature`), alrededor de sí mismo.
fn turn_in_body(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    parent: &[Bone],
    bone: Bone,
    delta: Quat,
) {
    let frame = turn(&bones.armature).inverse() * delta * turn(&bones.armature);
    let p = turn(&placed(bones, parent, transforms));
    if let Ok(mut t) = transforms.get_mut(bones.bones[bone.index()]) {
        t.rotation = (p.inverse() * frame * p * t.rotation).normalize();
    }
}

/// Cuánto puede bajar el extremo de una pata por debajo de donde se apoya antes
/// de que su zancada se estire menos, en metros.
pub const PLANTED: f32 = 0.015;

/// Pliega en su articulación inferior (la rodilla o el corvejón) cada pata cuyo
/// extremo quedó bajo el suelo por la mezcla o el estiramiento, lo justo para
/// volver a subirlo: dos clips mitad y mitad pueden bajar más que cualquiera de
/// los dos.
fn lift_legs(bones: &HorseBones, transforms: &mut Query<&mut Transform>) {
    for leg in 0..4 {
        let chain = Bone::chain(leg);
        let floor = bones.standing[leg] - PLANTED;
        let height = |transforms: &Query<&mut Transform>| {
            let end = placed(bones, &chain, transforms);
            bones
                .armature
                .transform_point3(Vec3::from(end.translation))
                .y
        };
        let low = height(transforms);
        if low >= floor {
            continue;
        }
        let lower = bones.bones[Bone::LowerLeg(leg).index()];
        let Ok(start) = transforms.get(lower).map(|t| t.rotation) else {
            continue;
        };
        // El pliegue alrededor del eje lateral del cuerpo, en el espacio del
        // padre del hueso inferior (ahí se aplica su rotación): la cadena hasta
        // él.
        let Some(at) = chain.iter().position(|b| *b == Bone::LowerLeg(leg)) else {
            continue;
        };
        let parent = turn(&placed(bones, &chain[..at], transforms));
        let side = parent.inverse() * turn(&bones.armature).inverse() * Vec3::Z;
        let mut best = (low, start);
        for step in 1..=12 {
            for sign in [1.0, -1.0] {
                let turned =
                    (Quat::from_axis_angle(side, sign * 0.06 * step as f32) * start).normalize();
                if let Ok(mut t) = transforms.get_mut(lower) {
                    t.rotation = turned;
                }
                let h = height(transforms);
                if h > best.0 {
                    best = (h, turned);
                }
            }
            if best.0 >= floor {
                break;
            }
        }
        if let Ok(mut t) = transforms.get_mut(lower) {
            t.rotation = best.1;
        }
    }
}

/// Coloca el objetivo de IK de cada vaso en el extremo de su pata, girado tanto
/// como giró la parte inferior de la pata desde `before` (los giros de las
/// partes inferiores tal como los dejaron los clips), y nunca por debajo de
/// donde estaría parado (o del lugar propio del clip, si el clip lo tiene más
/// abajo). La mezcla de clips pondera por separado los giros de las patas y los
/// objetivos de los vasos: sin esto, un vaso se separa de su pata entre dos
/// clips.
fn reattach(bones: &HorseBones, transforms: &mut Query<&mut Transform>, before: &[Quat; 4]) {
    for (leg, was_turned) in before.iter().enumerate() {
        let chain = Bone::chain(leg);
        let end = placed(bones, &chain, transforms);
        let lower = turn(&placed(bones, &chain[..chain.len() - 1], transforms));
        let entity = bones.bones[Bone::Hoof(leg).index()];
        let Ok(mut hoof) = transforms.get_mut(entity) else {
            continue;
        };
        let spin = lower * was_turned.inverse();
        let to_body = bones.armature;
        let was = to_body.transform_point3(hoof.translation).y;
        let floor = was.min(bones.standing[leg]);
        let mut body = to_body.transform_point3(Vec3::from(end.translation));
        body.y = body.y.max(floor);
        hoof.translation = to_body.inverse().transform_point3(body);
        hoof.rotation = (spin * hoof.rotation).normalize();
    }
}

/// La parte del peso del clip de salto a partir de la cual los vasos ya no se
/// sujetan al suelo (ver [`fit_horse`]).
pub const JUMP_FREE: f32 = 0.05;

/// Quita del caballo la elevación propia del clip de salto: `Body` lleva el
/// agachado y el salto del clip, pero la altura del caballo en el aire es el
/// arco de la física (el juego eleva la entidad entera según él), así que la
/// elevación del clip se le sumaría. El desplazamiento vertical del hueso
/// respecto del reposo (en el marco del cuerpo) se quita, en la proporción
/// `jump` (el peso del clip), después de que lo fijan los clips y antes de que
/// nada cuelgue de él; el recogido y el cabeceo se mantienen.
///
/// Los objetivos de IK de los vasos son hermanos del cuerpo bajo el armature,
/// no sus hijos, y el clip los eleva junto con él: se bajan el mismo
/// desplazamiento, o los vasos se quedarían arriba donde los puso el clip
/// mientras las patas bajan, separados de sus patas.
///
/// Leer el hueso después de la animación (en lugar de muestrear la curva) sigue
/// lo que realmente produjo la mezcla, en la escala propia del modelo.
fn level_flight(bones: &HorseBones, transforms: &mut Query<&mut Transform>, jump: f32) {
    let Ok(body) = transforms.get(bones.bones[Bone::Body.index()]) else {
        return;
    };
    let rise = bones
        .armature
        .transform_vector3(body.translation - bones.body_rest)
        .y;
    let drop = bones
        .armature
        .inverse()
        .transform_vector3(Vec3::Y * rise * jump);
    let hooves = (0..4).map(|leg| bones.bones[Bone::Hoof(leg).index()]);
    for entity in std::iter::once(bones.bones[Bone::Body.index()]).chain(hooves) {
        if let Ok(mut t) = transforms.get_mut(entity) {
            t.translation -= drop;
        }
    }
}

/// El impulso de las patas traseras al despegar, en radianes: cada muslo
/// balanceado hacia atrás en la cadera y cada corvejón abierto, así que los
/// vasos se estiran hacia atrás y hacia abajo al dejar el suelo (el clip tiene
/// el impulso, pero demasiado corto para que se note).
pub const PUSH: (f32, f32) = (0.3, 0.45);

/// El vuelo (`0..=1`) en el que el impulso ya terminó: a partir de ahí el clip
/// pliega hacia arriba las patas traseras.
pub const PUSH_END: f32 = 0.25;

/// El recogido extra de las manos en lo alto del salto, en radianes: cada
/// antebrazo levantado hacia adelante en el codo y cada rodilla plegada, un
/// poco más de lo que lo hace el clip.
pub const TUCK: (f32, f32) = (0.15, 0.4);

/// Cuánto impulso de las patas traseras (`0..1`) hay en la fracción `flight`
/// (`0..=1`) de un salto: todo cuando el caballo deja el suelo, y se desvanece
/// hasta [`PUSH_END`].
pub fn push(flight: f32) -> f32 {
    let u = (flight / PUSH_END).clamp(0.0, 1.0);
    1.0 - u * u * (3.0 - 2.0 * u)
}

/// Cuánto recogido extra de las manos (`0..1`) hay en la fracción `flight` de
/// un salto: nada en ambos extremos, todo en lo alto, suave en todo el
/// recorrido.
pub fn tuck(flight: f32) -> f32 {
    (std::f32::consts::PI * flight.clamp(0.0, 1.0))
        .sin()
        .powi(2)
}

/// Agrega el impulso de las patas traseras y el recogido de las manos
/// ([`PUSH`], [`TUCK`]) a la pose del clip de salto, en la fracción `flight`
/// del salto y ponderado por `jump` (el peso del clip), y después devuelve cada
/// vaso al extremo de su pata ([`reattach`]: nunca por debajo de donde se
/// apoya, así que el impulso no hunde los vasos al despegar).
fn push_and_tuck(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    flight: f32,
    jump: f32,
    before: &[Quat; 4],
) {
    let (push, tuck) = (push(flight) * jump, tuck(flight) * jump);
    if push < 1e-4 && tuck < 1e-4 {
        return;
    }
    // Alrededor del eje lateral del cuerpo: positivo balancea hacia adelante
    // una pata que cuelga.
    let pitch = Quat::from_rotation_z;
    for leg in 0..4 {
        let chain = Bone::chain(leg);
        let parent = |bone: Bone| {
            let at = chain.iter().position(|b| *b == bone).unwrap_or(0);
            &chain[..at]
        };
        if leg < 2 {
            let (forearm, knee) = (Bone::UpperLeg(leg), Bone::LowerLeg(leg));
            turn_in_body(
                bones,
                transforms,
                parent(forearm),
                forearm,
                pitch(TUCK.0 * tuck),
            );
            turn_in_body(bones, transforms, parent(knee), knee, pitch(-TUCK.1 * tuck));
        } else {
            let (thigh, hock) = (Bone::Leg(leg), Bone::LowerLeg(leg));
            turn_in_body(
                bones,
                transforms,
                parent(thigh),
                thigh,
                pitch(-PUSH.0 * push),
            );
            turn_in_body(bones, transforms, parent(hock), hock, pitch(-PUSH.1 * push));
        }
    }
    reattach(bones, transforms, before);
}

/// Ajusta la pose de un caballo para `weights` (en el orden de [`Clip::ALL`])
/// en la fase `phase` del ciclo del galope y en la fracción `flight` de un
/// salto (nada en el suelo): ver las notas del módulo.
pub fn fit_horse(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    weights: &[f32; CLIP_COUNT],
    phase: f32,
    flight: Option<f32>,
) {
    for (entity, property, mask, rest) in &bones.partial {
        if let Ok(mut t) = transforms.get_mut(*entity) {
            complete_blend(*property, rest, &mut t, animated_share(*mask, weights));
        }
    }
    let jump = weights[Clip::GallopJump.index()];
    if jump > 0.0 {
        level_flight(bones, transforms, jump);
    }
    let before: [Quat; 4] = std::array::from_fn(|leg| {
        let chain = Bone::chain(leg);
        turn(&placed(bones, &chain[..chain.len() - 1], transforms))
    });
    // El recogido y el cabeceo propios del salto reemplazan a los del galope:
    // sus patas deben dejar el suelo, así que tampoco se las sujeta a él.
    if jump >= JUMP_FREE {
        if let Some(flight) = flight {
            push_and_tuck(bones, transforms, flight, jump, &before);
        }
        return;
    }
    let gallop = weights[Clip::Gallop.index()] * (1.0 - jump);
    if gallop < 1e-3 {
        lift_legs(bones, transforms);
        reattach(bones, transforms, &before);
        return;
    }
    let a = articulation(phase);
    let pitch = |angle: f32| Quat::from_rotation_z(angle * gallop);
    use Bone as B;
    let spine = [
        B::Body,
        B::Back,
        B::Torso,
        B::Torso2,
        B::Torso3,
        B::Neck1,
        B::Neck2,
        B::Neck3,
    ];
    let hips = Quat::from_rotation_y(a.yaw * gallop)
        * Quat::from_rotation_x(a.roll * gallop)
        * pitch(a.back);
    turn_in_body(bones, transforms, &spine[..1], B::Back, hips);
    for (k, bone) in [B::Torso, B::Torso2, B::Torso3].into_iter().enumerate() {
        turn_in_body(bones, transforms, &spine[..2 + k], bone, pitch(a.torso[k]));
    }
    turn_in_body(bones, transforms, &spine[..5], B::Neck1, pitch(a.neck));
    turn_in_body(bones, transforms, &spine, B::Head, pitch(a.head));
    turn_in_body(bones, transforms, &spine[..2], B::Tail1, pitch(-a.tail));
    // El recorrido de cada pata estirado, mientras su extremo siga por encima
    // del suelo.
    let k = 1.0 + (STRIDE - 1.0) * gallop;
    for leg in 0..4 {
        let mut clip = [(Entity::PLACEHOLDER, Quat::IDENTITY, Quat::IDENTITY); 3];
        let mut n = 0;
        for (l, entity, mean) in &bones.means {
            if *l == leg
                && n < clip.len()
                && let Ok(t) = transforms.get(*entity)
            {
                clip[n] = (*entity, t.rotation, *mean);
                n += 1;
            }
        }
        let floor = bones.standing[leg] - PLANTED;
        for share in [1.0, 0.75, 0.5, 0.25, 0.0] {
            let kk = 1.0 + (k - 1.0) * share;
            for (entity, rotation, mean) in &clip[..n] {
                if let Ok(mut t) = transforms.get_mut(*entity) {
                    t.rotation = stretch(*rotation, *mean, kk);
                }
            }
            let end = placed(bones, &Bone::chain(leg), transforms);
            if bones
                .armature
                .transform_point3(Vec3::from(end.translation))
                .y
                >= floor
            {
                break;
            }
        }
    }
    lift_legs(bones, transforms);
    reattach(bones, transforms, &before);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_is_whole_at_takeoff_and_gone_by_its_end_and_the_tuck_peaks_mid_flight() {
        assert_eq!(push(0.0), 1.0);
        assert!(push(0.1) > 0.3 && push(0.1) < 1.0);
        for f in [PUSH_END, 0.5, 1.0] {
            assert_eq!(push(f), 0.0, "{f}");
        }
        assert!(tuck(0.0).abs() < 1e-6 && tuck(1.0).abs() < 1e-6);
        assert!((tuck(0.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_property_the_fading_clips_do_not_animate_eases_back_to_rest() {
        // Solo la caminata y el galope balancean el lomo.
        let mask = 0b110000;
        let walking = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(animated_share(mask, &walking), 1.0);
        let halfway = [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0];
        assert!((animated_share(mask, &halfway) - 0.5).abs() < 1e-6);
        let rest = Transform::from_xyz(0.0, 1.0, 0.0);
        let mut walked =
            Transform::from_xyz(0.0, 1.2, 0.0).with_rotation(Quat::from_rotation_x(0.6));
        complete_blend(Property::Translation, &rest, &mut walked, 0.5);
        complete_blend(Property::Rotation, &rest, &mut walked, 0.5);
        assert!((walked.translation.y - 1.1).abs() < 1e-6);
        assert!((walked.rotation.angle_between(Quat::IDENTITY) - 0.3).abs() < 1e-5);
        // Parado: completamente en reposo.
        complete_blend(
            Property::Rotation,
            &rest,
            &mut walked,
            animated_share(mask, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        );
        assert!(walked.rotation.angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn the_partial_table_is_the_files() {
        use crate::gltf::tests::root;
        let data = std::fs::read(root().0.join("models/fauna/quaternius/horse.glb")).unwrap();
        let len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let json = String::from_utf8(data[20..20 + len].to_vec()).unwrap();
        // Nombres de nodo en el orden del archivo, y los pares (nodo, ruta) de
        // cada animación.
        let names: Vec<&str> = {
            // La lista de nodos de nivel superior es la última clave del
            // archivo.
            let nodes = &json[json.rfind("\"nodes\":[").unwrap()..];
            nodes
                .match_indices("\"name\":\"")
                .map(|(i, m)| {
                    let s = &nodes[i + m.len()..];
                    &s[..s.find('"').unwrap()]
                })
                .collect()
        };
        let animations = &json[json.find("\"animations\":[").unwrap()..];
        let mut found: std::collections::BTreeMap<(String, &str), u8> = Default::default();
        for (k, clip) in Clip::ALL.iter().enumerate() {
            let start = animations
                .find(&format!("{{\"name\":\"{}\",\"channels\":[", clip.name()))
                .unwrap();
            let end = animations[start..].find("],\"samplers\"").unwrap() + start;
            let channels = &animations[start..end];
            for (i, _) in channels.match_indices("\"node\":") {
                let rest = &channels[i + 7..];
                let node: usize = rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap()]
                    .parse()
                    .unwrap();
                let path = &rest[..rest.find('}').unwrap()];
                let path = if path.contains("rotation") {
                    "rotation"
                } else if path.contains("translation") {
                    "translation"
                } else {
                    continue;
                };
                *found.entry((names[node].to_string(), path)).or_default() |= 1 << k;
            }
        }
        let partial: Vec<(String, &str, u8)> = found
            .into_iter()
            .filter(|(_, m)| *m != (1 << Clip::ALL.len()) - 1)
            .map(|((n, p), m)| (n, p, m))
            .collect();
        let table: Vec<(String, &str, u8)> = PARTIAL
            .iter()
            .map(|(n, p, m)| {
                let p = if *p == Property::Rotation {
                    "rotation"
                } else {
                    "translation"
                };
                (n.to_string(), p, *m)
            })
            .collect();
        assert_eq!(partial, table);
    }

    #[test]
    fn a_stretched_swing_reaches_further_about_its_mean() {
        let mean = Quat::from_euler(EulerRot::XYZ, 0.3, -0.2, 0.9);
        let swung = mean * Quat::from_rotation_z(0.4);
        let stretched = stretch(swung, mean, 1.5);
        assert!((stretched.angle_between(mean) - 0.6).abs() < 1e-4);
        assert!(
            (mean.inverse() * stretched)
                .to_axis_angle()
                .0
                .distance(Vec3::Z)
                < 1e-4
        );
        assert!(stretch(mean, mean, 1.5).angle_between(mean) < 1e-4);
        assert!(stretch(swung, mean, 1.0).angle_between(swung) < 1e-4);
    }

    /// Eleva el suelo 12 cm bajo las patas `legs` de un caballo ensillado
    /// parado, para que [`lift_legs`] tenga que plegarlas, y comprueba que cada
    /// una se pliega en su articulación inferior (la rodilla o el corvejón)
    /// alrededor del eje lateral del cuerpo: su extremo sube, la articulación
    /// se queda y el extremo conserva su desplazamiento lateral.
    fn folds_in_the_sagittal_plane(legs: [usize; 2]) {
        use crate::character::horse::tests::saddled;
        use crate::character::{CharacterVisual, Coat};
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, _) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let world = app.world_mut();
        let mut bones = world.query::<&HorseBones>().single(world).unwrap().clone();
        // Las posiciones en el marco del cuerpo de la articulación inferior y
        // del extremo de cada pata.
        let at = |world: &mut World, bones: &HorseBones, leg: usize| {
            let bones = bones.clone();
            world
                .run_system_once(move |transforms: Query<&mut Transform>| {
                    let chain = Bone::chain(leg);
                    let joint = chain
                        .iter()
                        .position(|b| *b == Bone::LowerLeg(leg))
                        .unwrap();
                    let body = |c: &[Bone]| {
                        let p = placed(&bones, c, &transforms);
                        bones.armature.transform_point3(Vec3::from(p.translation))
                    };
                    (body(&chain[..=joint]), body(&chain))
                })
                .unwrap()
        };
        let before = legs.map(|leg| at(world, &bones, leg));
        for (k, &leg) in legs.iter().enumerate() {
            bones.standing[leg] = before[k].1.y + PLANTED + 0.12;
        }
        let folding = bones.clone();
        world
            .run_system_once(move |mut transforms: Query<&mut Transform>| {
                lift_legs(&folding, &mut transforms)
            })
            .unwrap();
        for (k, &leg) in legs.iter().enumerate() {
            let (joint, end) = before[k];
            let (joint2, end2) = at(world, &bones, leg);
            assert!(end2.y > end.y + 0.03, "leg {leg} folded: {end} -> {end2}");
            assert!(
                joint2.distance(joint) < 1e-4,
                "leg {leg}'s joint stays: {joint} {joint2}"
            );
            // Un pliegue alrededor del eje lateral conserva exactamente el
            // desplazamiento lateral del extremo (alrededor del propio eje del
            // hueso, en cambio, se desvía ~1 mm).
            assert!(
                (end2.z - end.z).abs() < 2e-4,
                "leg {leg} twisted sideways: {end} -> {end2}"
            );
        }
    }

    #[test]
    fn a_folded_hind_leg_stays_in_the_bodys_sagittal_plane() {
        folds_in_the_sagittal_plane([2, 3]);
    }

    #[test]
    fn a_folded_front_leg_stays_in_the_bodys_sagittal_plane() {
        // Las patas delanteras se pliegan en la rodilla: su cadena termina en
        // un único `End`, así que el padre del hueso inferior está en otro
        // lugar de ella que en una pata trasera.
        folds_in_the_sagittal_plane([0, 1]);
    }

    #[test]
    fn the_croup_and_the_withers_pitch_against_each_other_through_the_loop() {
        let mut low = (f32::MIN, 0.0);
        for k in 0..100 {
            let phase = k as f32 / 100.0;
            let a = articulation(phase);
            // La cruz gira según la grupa y cada hueso del lomo.
            let withers = a.back + a.torso.iter().sum::<f32>();
            assert!(a.back * withers <= 0.0, "{phase}: {a:?}");
            assert!(a.back.abs() <= CROUP + 1e-6 && a.roll.abs() < 0.05);
            if a.back > low.0 {
                low = (a.back, phase);
            }
            let again = articulation(phase + 1.0);
            assert!((a.back - again.back).abs() < 1e-5 && (a.tail - again.tail).abs() < 1e-5);
        }
        // La grupa está en su punto más bajo (el extremo delantero del tercio
        // posterior más arriba) cuando las patas traseras se juntan bajo la
        // panza.
        assert!((low.1 - GATHER).abs() < 0.02, "{low:?}");
    }
}
