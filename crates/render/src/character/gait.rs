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
//! - **Un galope más largo, si hace falta.** Al galope, el recorrido de cada
//!   pata alrededor de su propia media ([`GALLOP_MEANS`]) se estira por
//!   [`STRIDE`], así que el clip cubre más terreno por ciclo y se reproduce más
//!   lento. El galope de Quaternius lo necesitaba (unos 2.3 m por ciclo:
//!   reproducido lo bastante rápido como para no deslizarse a galope tendido,
//!   parecía frenético); el del caballo propio ya da unos 4.15 m por ciclo, así
//!   que no se estira.
//! - **Un cuerpo que se flexiona** ([`articulation`]), sincronizado con el
//!   ciclo del galope: la grupa baja cuando las patas traseras llegan bajo la
//!   panza y sube en el impulso, el lomo se arquea y se estira en contra, así
//!   que la cruz cabecea en sentido opuesto, el cuello bombea contra el tercio
//!   anterior y la cabeza se estabiliza, la cadera rola y gira un poco con la
//!   mano de galope (la cola la mueve el propio clip). La caminata se deja
//!   como está.
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
//!   [`PUSH`]); el ajuste puede recoger además las manos en lo alto
//!   ([`tuck`], [`TUCK`]), pero el clip del caballo propio ya las recoge, así
//!   que está en cero. Cada vaso vuelve al extremo de su pata.
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
/// demás propiedades animadas están en todos los clips. El caballo propio
/// (`tools/horse.py`) anima cada hueso en cada clip, así que no hay ninguna (el
/// de Quaternius tenía 26); el test `the_partial_table_is_the_files` las lee del
/// archivo.
pub const PARTIAL: [(&str, Property, u8); 0] = [];

/// Los huesos que ningún clip gira pero que gira [`fit_horse`]: se devuelven
/// primero al reposo en cada frame, o los giros se acumularían. Ninguno en el
/// caballo propio, cuyos clips fijan cada hueso en cada frame (en el de
/// Quaternius, `Torso` y `Tail1`).
pub const UNANIMATED: [&str; 0] = [];

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

/// Cuánto más larga es la zancada del galope que la del clip: nada. El clip
/// del caballo propio (`tools/horse.py`) ya muestra 7.12 m/s en 0.58 s (unos
/// 4.15 m por zancada), lo que mostraba el de Quaternius estirado 1.4 veces; el
/// galope tendido (12.2 m/s) se reproduce al mismo ritmo que antes, unas 2.9
/// zancadas por segundo.
pub const STRIDE: f32 = 1.0;

/// El giro medio de cada hueso superior de pata que se balancea a lo largo del
/// ciclo del galope (su propia rotación local, `[x, y, z, w]`, medida en
/// `horse.glb`: el promedio normalizado de las claves del clip, con el signo
/// alineado): la zancada se estira alrededor de él. Las partes inferiores de
/// las patas se pliegan como las pliega el clip. `BackShoulder.*` no se mueve en
/// el galope del caballo propio.
pub const GALLOP_MEANS: [(&str, [f32; 4]); 8] = [
    ("FrontUpperLeg.L", [0.71549, 0.62746, 0.20254, 0.23095]),
    ("FrontUpperLeg.R", [0.7146, -0.62668, -0.20494, 0.2337]),
    ("BackShoulder.L", [-0.61181, -0.55903, -0.38261, 0.4084]),
    ("BackShoulder.R", [-0.6118, 0.55903, 0.3826, 0.40841]),
    ("BackLeg.L", [0.7177, 0.66242, 0.14551, 0.15787]),
    ("BackLeg.R", [0.71722, -0.66192, -0.14763, 0.1602]),
    ("BackUpperLeg.L", [0.61759, -0.00002, -0.00088, 0.7865]),
    ("BackUpperLeg.R", [0.61024, 0.00003, 0.00077, 0.79222]),
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

/// En qué punto del ciclo del galope (`0..1`) la grupa está más baja respecto
/// de la cruz en el propio clip: la flexión se suma en fase con la suya.
/// Medido en `horse.glb` como el punto en que la línea de `Tail1` a `Neck1`
/// cabecea más hacia arriba (0.75; en el de Quaternius, 0.57, que coincidía con
/// la trasera izquierda más adelante). Fuera de fase, la flexión desliza los
/// vasos apoyados: en 0.11 (la trasera izquierda más adelante) se deslizan más
/// de 0.5 m/s.
pub const GATHER: f32 = 0.75;

/// El movimiento propio del cuerpo al galope, en radianes, en la fase `phase`
/// de su ciclo (`0..1`): cabeceos alrededor del eje lateral del cuerpo
/// (positivos con la nariz o el extremo delantero hacia arriba) del tercio
/// posterior (`Back`), de cada hueso del lomo, del cuello y de la cabeza, el
/// rolido y el giro de la cadera. La cola la mueve el propio clip (levantada,
/// en curva y con una onda por tranco): girarla además desde el maslo
/// arrastraba la piel de las nalgas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Articulation {
    pub back: f32,
    pub torso: [f32; 3],
    pub neck: f32,
    pub head: f32,
    pub roll: f32,
    pub yaw: f32,
}

/// El cabeceo de la grupa hacia cada lado al galope, en radianes (unos 7°),
/// además del propio del clip: su `Torso` cabecea unos 11° de punta a punta
/// (el de Quaternius, 12.6°), así que la flexión se mantiene.
pub const CROUP: f32 = 0.12;

/// La [`Articulation`] en la fase `phase` del galope. El tercio posterior es el
/// que más cabecea; los huesos del lomo se doblan en contra, cada uno un poco
/// más hacia la cruz, así que el tercio anterior cabecea en sentido opuesto; el
/// cuello bombea contra el tercio anterior y la cabeza deshace la mitad.
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
/// vasos se estiran hacia atrás y hacia abajo al dejar el suelo. El clip del
/// caballo propio despega con las patas traseras todavía cerca del suelo (de
/// la babilla al vaso, 0.97 m al empezar el vuelo), así que el ajuste las
/// termina de estirar.
pub const PUSH: (f32, f32) = (0.3, 0.45);

/// El vuelo (`0..=1`) en el que el impulso ya terminó: a partir de ahí el clip
/// pliega hacia arriba las patas traseras.
pub const PUSH_END: f32 = 0.25;

/// El recogido extra de las manos en lo alto del salto, en radianes: cada
/// antebrazo levantado hacia adelante en el codo y cada rodilla plegada, más
/// de lo que lo hace el clip. Ninguno: el clip del caballo propio ya recoge
/// las manos a 0.32–0.40 m de los codos en lo alto (el de Quaternius las dejaba
/// a unos 0.72 m y se recogían 0.15 y 0.4 rad más); recogerlas otra vez
/// aplasta la piel de la rodilla.
pub const TUCK: (f32, f32) = (0.0, 0.0);

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
        use crate::fauna::livestock::Species;
        use crate::gltf::tests::root;
        use bevy::gltf::gltf::{self, animation::Property as Path};
        let data = std::fs::read(root().0.join(Species::Horse.path())).unwrap();
        let file = gltf::Gltf::from_slice(&data).unwrap();
        // Las propiedades (nodo, ruta) de cada clip, en el orden de
        // [`Clip::ALL`].
        let mut found: std::collections::BTreeMap<(String, &str), u8> = Default::default();
        for (k, clip) in Clip::ALL.iter().enumerate() {
            let animation = file
                .animations()
                .find(|a| a.name() == Some(clip.name()))
                .unwrap();
            for channel in animation.channels() {
                let path = match channel.target().property() {
                    Path::Rotation => "rotation",
                    Path::Translation => "translation",
                    _ => continue,
                };
                let node = channel.target().node().name().unwrap().to_string();
                *found.entry((node, path)).or_default() |= 1 << k;
            }
        }
        let every = (1 << Clip::ALL.len()) - 1;
        let partial: Vec<(String, &str, u8)> = found
            .iter()
            .filter(|(_, m)| **m != every)
            .map(|((n, p), m)| (n.clone(), *p, *m))
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
        // Cada hueso que gira [`fit_horse`] lo giran todos los clips o
        // ninguno ([`UNANIMATED`]), o su giro se completa ([`PARTIAL`]): si
        // no, sus giros se acumularían frame a frame.
        let turned = [
            Bone::Back,
            Bone::Torso,
            Bone::Torso2,
            Bone::Torso3,
            Bone::Neck1,
            Bone::Head,
            Bone::Tail1,
        ]
        .into_iter()
        .chain((0..4).flat_map(|l| [Bone::UpperLeg(l), Bone::Leg(l), Bone::LowerLeg(l)]))
        .map(Bone::name);
        for name in turned {
            let mask = found.get(&(name.clone(), "rotation")).copied().unwrap_or(0);
            let unanimated = UNANIMATED.contains(&name.as_str());
            assert_eq!(mask == 0, unanimated, "{name}: {mask:#b}");
            let listed = PARTIAL
                .iter()
                .any(|(n, p, _)| *n == name && *p == Property::Rotation);
            assert!(mask == 0 || mask == every || listed, "{name}: {mask:#b}");
        }
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
            assert!((a.back - again.back).abs() < 1e-5 && (a.roll - again.roll).abs() < 1e-5);
        }
        // La grupa está en su punto más bajo (el extremo delantero del tercio
        // posterior más arriba) cuando las patas traseras se juntan bajo la
        // panza.
        assert!((low.1 - GATHER).abs() < 0.02, "{low:?}");
    }
}
