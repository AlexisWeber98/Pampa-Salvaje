//! Los clips del caballo ensillado, ajustados después de los sistemas de
//! animación (ver [`horse`](super::horse)): lo que los clips del caballo de
//! henrysteve973 dejan corto. Los huesos se piden por su papel
//! ([`HorseJoint`]).
//!
//! - **Una caminata más larga en la franja del trote** ([`stretch_swings`]).
//!   La caminata del clip (0.92 m/s a su ritmo grabado) se reproduce tal cual
//!   hasta lo que camina la lógica, 4,5 km/h
//!   ([`WALK_TOP`](super::horse::WALK_TOP), S8); por encima, sin clip de
//!   trote, lo que falta se reparte por igual entre el ritmo
//!   ([`gait_rate`](super::horse::gait_rate)) y el tranco. El balanceo de cada pata visto
//!   desde su cabeza (la cruz o la cadera) se estira alrededor de su ángulo
//!   medio ([`WALK_SWING`]) lo que pide
//!   [`walk_stride`](super::horse::walk_stride), hasta 1.55 veces: girar el hueso más alto de la
//!   pata en el plano del cuerpo estira el recorrido del vaso sin cambiar cómo
//!   se pliega la pata, y los vasos apoyados van al ritmo del suelo.
//! - **Los vasos por encima del suelo** ([`lift_legs`]). Una pata cuyo vaso
//!   quedó más abajo de lo que lo bajan los clips ([`TOE_LOWEST`]), por una
//!   mezcla de dos clips, se pliega en la rodilla o el corvejón lo justo.
//!
//! Lo que hacía la capa para el caballo anterior y ya no hace falta: el galope
//! de henrysteve973 no se estira: alarga el tranco reproduciéndose más rápido
//! mientras apoya y más despacio en la suspensión
//! ([`gallop_rate`](super::horse::gallop_rate)); ya flexiona el lomo (la grupa y la
//! cruz giran entre sí unos 0.26 rad por tranco y la línea de la grupa a la
//! cruz cabecea 13°, cuando la del caballo anterior cabeceaba 11° y la capa le
//! sumaba la flexión); sus vasos son parte de las patas (no cuelgan de
//! objetivos de IK que haya que devolver al extremo de la pata); y el salto ya
//! impulsa con las patas traseras estiradas hacia atrás al despegar y recoge
//! las manos (la altura del salto la ajusta
//! [`ground_horses`](super::horse::ground_horses)).
//!
//! Todo es determinista, se calcula de nuevo en cada frame a partir de la pose
//! de los clips, y no reserva memoria.

use bevy::math::Affine3A;
use bevy::prelude::*;

use super::bones::{HorseJoint, TOE_LOWEST};
use crate::fauna::livestock::{CLIP_COUNT, Clip};

/// El ángulo medio del balanceo de cada pata en la caminata, en radianes: la
/// dirección de la cabeza de la pata ([`HorseJoint::Upper`]) a la punta de su
/// vaso ([`HorseJoint::Toe`]) en el plano del cuerpo, positiva con el vaso
/// por delante de la vertical; el promedio de 192 muestras del ciclo de
/// `Walk` en `horse.glb`. Las cuatro van un poco por detrás: la cabeza de la
/// mano está en la cruz y la paleta baja inclinada hacia adelante.
pub const WALK_SWING: [f32; 4] = [-0.0899, -0.0958, -0.0997, -0.1013];

/// Los huesos de las patas de un caballo, ubicados cuando se crea su escena;
/// en su raíz junto a su [`HorseRig`](super::horse::HorseRig).
#[derive(Component, Debug, Clone)]
pub struct HorseBones {
    /// La ubicación del padre de la raíz del esqueleto en el marco del modelo
    /// (el del archivo: +Z hacia adelante, +Y hacia arriba, +X a la izquierda
    /// del caballo, en metros del modelo).
    pub base: Affine3A,
    /// Por pata, la cadena de huesos desde la raíz del esqueleto
    /// ([`HorseJoint::Root`]) hasta la punta del vaso ([`HorseJoint::Toe`]).
    pub legs: [Vec<Entity>; 4],
    /// Dónde está en cada cadena el hueso más alto de la pata
    /// ([`HorseJoint::Upper`]).
    pub upper: [usize; 4],
    /// Dónde está en cada cadena la caña ([`HorseJoint::Cannon`]).
    pub cannon: [usize; 4],
    /// Lo más bajo que puede quedar la punta de un vaso en el marco del modelo
    /// ([`TOE_LOWEST`]).
    pub floor: f32,
}

impl HorseBones {
    /// Los huesos de un caballo a partir de las cadenas de sus patas (desde la
    /// raíz del esqueleto hasta la punta del vaso, con el nombre de cada
    /// hueso) y de la ubicación del padre de la raíz en el marco del modelo.
    pub fn new(base: Affine3A, legs: [Vec<(Entity, &str)>; 4]) -> Option<Self> {
        let at = |leg: usize, joint: HorseJoint| {
            legs[leg].iter().position(|(_, n)| *n == joint.name())
        };
        let mut upper = [0; 4];
        let mut cannon = [0; 4];
        for leg in 0..4 {
            upper[leg] = at(leg, HorseJoint::Upper(leg))?;
            cannon[leg] = at(leg, HorseJoint::Cannon(leg))?;
            if legs[leg].last()?.1 != HorseJoint::Toe(leg).name() || cannon[leg] <= upper[leg] {
                return None;
            }
        }
        Some(Self {
            base,
            legs: legs.map(|chain| chain.into_iter().map(|(e, _)| e).collect()),
            upper,
            cannon,
            floor: TOE_LOWEST,
        })
    }
}

/// La ubicación en el marco del modelo del último hueso de `chain` (una
/// cadena desde la raíz del esqueleto), a partir de `transforms`.
fn placed(bones: &HorseBones, chain: &[Entity], transforms: &Query<&mut Transform>) -> Affine3A {
    let mut at = bones.base;
    for &e in chain {
        if let Ok(t) = transforms.get(e) {
            at *= t.compute_affine();
        }
    }
    at
}

/// La parte de rotación de una ubicación.
fn turn(a: &Affine3A) -> Quat {
    let (_, rotation, _) = a.to_scale_rotation_translation();
    rotation
}

/// Dónde está la punta del vaso de la pata `leg` en el marco del modelo.
fn toe(bones: &HorseBones, leg: usize, transforms: &Query<&mut Transform>) -> Vec3 {
    Vec3::from(placed(bones, &bones.legs[leg], transforms).translation)
}

/// Gira el hueso `at` de la cadena de la pata `leg` por `delta`, un giro en el
/// marco del modelo, alrededor de su propia articulación.
fn turn_in_model(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    leg: usize,
    at: usize,
    delta: Quat,
) {
    let chain = &bones.legs[leg];
    let parent = turn(&placed(bones, &chain[..at], transforms));
    if let Ok(mut t) = transforms.get_mut(chain[at]) {
        t.rotation = (parent.inverse() * delta * parent * t.rotation).normalize();
    }
}

/// El balanceo de la pata `leg`: el ángulo de la cabeza de la pata a la punta
/// de su vaso en el plano del cuerpo, como [`WALK_SWING`].
fn swing(bones: &HorseBones, leg: usize, transforms: &Query<&mut Transform>) -> f32 {
    let chain = &bones.legs[leg];
    let head = Vec3::from(placed(bones, &chain[..=bones.upper[leg]], transforms).translation);
    let d = toe(bones, leg, transforms) - head;
    d.z.atan2(-d.y)
}

/// Estira `k` veces el balanceo de cada pata alrededor de su ángulo medio de
/// la caminata ([`WALK_SWING`]): gira el hueso más alto de la pata alrededor
/// del eje lateral del cuerpo, así que el resto de la pata se pliega como la
/// pliega el clip y el vaso recorre `k` veces más terreno por tranco.
fn stretch_swings(bones: &HorseBones, transforms: &mut Query<&mut Transform>, k: f32) {
    for leg in 0..4 {
        let extra = (k - 1.0) * (swing(bones, leg, transforms) - WALK_SWING[leg]);
        // Un giro positivo alrededor de +X lleva hacia atrás (-Z) un vaso que
        // cuelga (-Y): el balanceo crece hacia adelante con el giro negativo.
        turn_in_model(
            bones,
            transforms,
            leg,
            bones.upper[leg],
            Quat::from_rotation_x(-extra),
        );
    }
}

/// Pliega en la caña (la rodilla o el corvejón) cada pata cuyo vaso quedó por
/// debajo de [`HorseBones::floor`], lo justo para volver a subirlo: dos clips
/// mitad y mitad pueden bajarlo más que cualquiera de los dos. El pliegue es
/// alrededor del eje lateral del cuerpo, en el sentido que más lo sube.
fn lift_legs(bones: &HorseBones, transforms: &mut Query<&mut Transform>) {
    for leg in 0..4 {
        let low = toe(bones, leg, transforms).y;
        if low >= bones.floor {
            continue;
        }
        let at = bones.cannon[leg];
        let cannon = bones.legs[leg][at];
        let Ok(start) = transforms.get(cannon).map(|t| t.rotation) else {
            continue;
        };
        // El eje lateral del cuerpo en el espacio del padre de la caña (ahí se
        // aplica su rotación).
        let parent = turn(&placed(bones, &bones.legs[leg][..at], transforms));
        let side = parent.inverse() * Vec3::X;
        let mut best = (low, start);
        for step in 1..=12 {
            for sign in [1.0, -1.0] {
                let turned =
                    (Quat::from_axis_angle(side, sign * 0.06 * step as f32) * start).normalize();
                if let Ok(mut t) = transforms.get_mut(cannon) {
                    t.rotation = turned;
                }
                let h = toe(bones, leg, transforms).y;
                if h > best.0 {
                    best = (h, turned);
                }
            }
            if best.0 >= bones.floor {
                break;
            }
        }
        if let Ok(mut t) = transforms.get_mut(cannon) {
            t.rotation = best.1;
        }
    }
}

/// Ajusta la pose de un caballo para `weights` (en el orden de [`Clip::ALL`])
/// con la caminata estirada `stride` veces
/// ([`walk_stride`](super::horse::walk_stride), uno sin estirar): ver las
/// notas del módulo. El estiramiento va en proporción al peso de la caminata.
pub fn fit_horse(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    weights: &[f32; CLIP_COUNT],
    stride: f32,
) {
    let k = 1.0 + (stride - 1.0) * weights[Clip::Walk.index()].clamp(0.0, 1.0);
    if k > 1.0 + 1e-4 {
        stretch_swings(bones, transforms, k);
    }
    lift_legs(bones, transforms);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::horse::HORSE_CLIPS;

    #[test]
    fn every_clip_a_horse_plays_moves_the_same_joints_so_the_blend_is_whole() {
        use crate::fauna::livestock::Species;
        use crate::gltf::tests::root;
        use bevy::gltf::gltf::{self, animation::Property as Path};
        let data = std::fs::read(root().0.join(Species::Horse.path())).unwrap();
        let file = gltf::Gltf::from_slice(&data).unwrap();
        // Bevy pondera cada propiedad de un hueso solo entre los clips que la
        // animan: una que animara un solo clip conservaría ese clip con toda
        // su fuerza mientras se desvanece. Las propiedades (nodo, ruta) de
        // cada clip.
        let animated = |clip: Clip| {
            let name = Species::Horse.clip_names()[Species::Horse.animation(clip)];
            let animation = file
                .animations()
                .find(|a| a.name() == Some(name))
                .unwrap();
            let mut out = std::collections::BTreeSet::new();
            for channel in animation.channels() {
                let path = match channel.target().property() {
                    Path::Rotation => "rotation",
                    Path::Translation => "translation",
                    Path::Scale => "scale",
                    _ => continue,
                };
                out.insert((channel.target().node().name().unwrap().to_string(), path));
            }
            out
        };
        let idle = animated(Clip::Idle);
        for clip in HORSE_CLIPS {
            // Ninguno mueve más que los huesos de las esperas (de los que
            // tienen movimiento de raíz, `Jump_Land` no se reproduce, y
            // `Buck_2` va y viene a lo largo del cuerpo y termina donde
            // empezó: `ground_horses` se lo deja mover solo mientras
            // corcovea).
            let mut moved = animated(clip);
            if clip == Clip::Buck2 {
                assert!(moved.remove(&(HorseJoint::Root.name().to_string(), "translation")));
            }
            assert_eq!(moved, idle, "{clip:?}");
        }
        // Los huesos que gira este módulo los fija cada clip en cada frame: si
        // no, sus giros se acumularían frame a frame.
        for leg in 0..4 {
            for joint in [HorseJoint::Upper(leg), HorseJoint::Cannon(leg)] {
                assert!(
                    idle.contains(&(joint.name().to_string(), "rotation")),
                    "{joint:?}"
                );
            }
        }
    }

    /// Un caballo ensillado parado: su app y sus huesos.
    fn standing() -> (App, HorseBones) {
        use crate::character::horse::tests::saddled;
        use crate::character::{CharacterVisual, Coat};
        let (mut app, _) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let world = app.world_mut();
        let bones = world.query::<&HorseBones>().single(world).unwrap().clone();
        (app, bones)
    }

    /// Las posiciones en el marco del modelo de la caña (su articulación) y
    /// de la punta del vaso de la pata `leg`.
    fn leg_at(world: &mut World, bones: &HorseBones, leg: usize) -> (Vec3, Vec3) {
        use bevy::ecs::system::RunSystemOnce;
        let bones = bones.clone();
        world
            .run_system_once(move |transforms: Query<&mut Transform>| {
                let chain = &bones.legs[leg];
                let at = |c: &[Entity]| Vec3::from(placed(&bones, c, &transforms).translation);
                (at(&chain[..=bones.cannon[leg]]), at(chain))
            })
            .unwrap()
    }

    /// Eleva el suelo 12 cm sobre las patas `legs` de un caballo ensillado
    /// parado, para que [`lift_legs`] tenga que plegarlas, y comprueba que cada
    /// una se pliega en la caña (la rodilla o el corvejón) alrededor del eje
    /// lateral del cuerpo: su vaso sube, la articulación se queda y el vaso
    /// conserva su desplazamiento lateral.
    fn folds_in_the_sagittal_plane(legs: [usize; 2]) {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, mut bones) = standing();
        let world = app.world_mut();
        let before = legs.map(|leg| leg_at(world, &bones, leg));
        bones.floor = before.iter().map(|b| b.1.y).fold(f32::MIN, f32::max) + 0.12;
        let folding = bones.clone();
        world
            .run_system_once(move |mut transforms: Query<&mut Transform>| {
                lift_legs(&folding, &mut transforms)
            })
            .unwrap();
        for (k, &leg) in legs.iter().enumerate() {
            let (joint, end) = before[k];
            let (joint2, end2) = leg_at(world, &bones, leg);
            assert!(end2.y > end.y + 0.03, "leg {leg} folded: {end} -> {end2}");
            assert!(
                joint2.distance(joint) < 1e-4,
                "leg {leg}'s joint stays: {joint} {joint2}"
            );
            // Un pliegue alrededor del eje lateral conserva exactamente el
            // desplazamiento lateral del vaso.
            assert!(
                (end2.x - end.x).abs() < 2e-4,
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
        folds_in_the_sagittal_plane([0, 1]);
    }

    #[test]
    fn a_stretched_swing_reaches_k_times_further_about_its_mean_and_keeps_the_leg_folded() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, bones) = standing();
        let world = app.world_mut();
        let read = |world: &mut World, bones: &HorseBones| {
            let bones = bones.clone();
            world
                .run_system_once(move |transforms: Query<&mut Transform>| {
                    std::array::from_fn::<_, 4, _>(|leg| {
                        let chain = &bones.legs[leg];
                        let cannon = turn(&placed(&bones, &chain[..=bones.cannon[leg]], &transforms));
                        let upper = turn(&placed(&bones, &chain[..=bones.upper[leg]], &transforms));
                        (swing(&bones, leg, &transforms), upper.inverse() * cannon)
                    })
                })
                .unwrap()
        };
        let before = read(world, &bones);
        let stretching = bones.clone();
        world
            .run_system_once(move |mut transforms: Query<&mut Transform>| {
                stretch_swings(&stretching, &mut transforms, 1.5)
            })
            .unwrap();
        let after = read(world, &bones);
        for leg in 0..4 {
            let (was, shape) = before[leg];
            let (now, shape2) = after[leg];
            let goal = WALK_SWING[leg] + 1.5 * (was - WALK_SWING[leg]);
            assert!((now - goal).abs() < 1e-3, "leg {leg}: {was} -> {now}, not {goal}");
            // La pata se pliega igual por debajo del hueso girado (el ángulo
            // entre dos giros casi iguales sale de un arcocoseno: el ruido de
            // f32 llega a unos 5e-4 rad).
            let bent = shape.angle_between(shape2);
            assert!(bent < 2e-3, "leg {leg} bent {bent} rad");
        }
    }
}
