//! El controlador de clips: reproduce los clips de animación compartidos
//! ([`clips`](super::clips)) en cada modelo de MakeHuman (ver
//! [`model`](super::model)), con la mirada de la cabeza por encima.
//!
//! La figura sigue siendo el portador: su cuerpo gira hacia el rumbo, llevando
//! el modelo, y [`update_figures`](super::figure) sigue manteniendo en pose sus
//! articulaciones ocultas para `models off`. Ahora son los propios clips del
//! modelo los que mueven el cuerpo:
//!
//! - [`drive_rigs`], antes de los sistemas de animación, elige y pondera los
//!   clips. Parada, una persona reproduce su espera ([`idle_for`]); en
//!   movimiento, la espera se funde con la caminata, el trote y la carrera
//!   según la velocidad sobre el suelo ([`locomotion`]), con cada clip de
//!   caminata ajustado a esa velocidad ([`stride_rate`]), según las piernas del
//!   modelo en relación con las de los clips ([`model_stride`]), para que los
//!   pies se queden quietos sobre el suelo; al retroceder, los clips de
//!   caminata se reproducen hacia atrás al mismo ritmo; al girar en el lugar,
//!   la caminata se reproduce al ritmo corto de [`stepping_speed`]. Los pesos
//!   se acercan suavemente a sus objetivos, y cada clip empieza en el propio
//!   punto con semilla de la persona ([`phase_offset`]). El modelo también
//!   deshace el rebote de la zancada del portador: los clips ya hacen rebotar
//!   la pelvis por su cuenta.
//! - [`fit_rigs`], después de ellos, ajusta el clip al modelo: los clips se
//!   hicieron sobre el protagonista, así que los movimientos de la pelvis se
//!   escalan según las piernas de este modelo y se aplican alrededor de su
//!   propio reposo ([`fit_pelvis`]); después el cuello y la cabeza giran según
//!   la mirada de la figura, la mitad cada uno, por encima del clip
//!   ([`glance_turn`]); un modelo con pollera larga tiene los brazos un poco
//!   separados ([`arm_out`]).
//!
//! Un jinete (ver [`rider`](super::rider)) reproduce solo el clip de sentado
//! ([`SEATED`]): un clip con peso cero igual fija los huesos que solo él anima.
//!
//! Un modelo fuera de la vista (ninguna de sus meshes dibujada) o más lejos que
//! [`FREEZE_FAR`](crate::fauna::FREEZE_FAR) de la cámara pierde su grafo, igual
//! que el ganado: sus huesos conservan su última pose. El protagonista, que
//! siempre es el jugador, nunca se congela.

use bevy::animation::{AnimatedBy, AnimationTargetId};
use bevy::prelude::*;
use pampa_core::AnimationState;

use super::clips::{
    CharacterClips, Clip, Reference, STRIDES, TURN_STEP_RADIUS, idle_for, locomotion, phase_offset,
    playback_rate, stepping_speed,
};
use super::figure::{FigureBody, FigureKind};
use super::model::model_transform;
use super::rider::Riding;
use super::townsfolk::{SKIRT_ARMS_OUT, arm_out};
use super::visual::VisualPose;
use crate::fauna::livestock::{any_in_view, frozen, view_eye};

/// Qué tan rápido se acercan los pesos de los clips a sus objetivos, por
/// segundo.
pub const BLEND_RATE: f32 = 6.0;

/// Los clips que puede reproducir un modelo a pie: las esperas y las zancadas.
/// Cada uno empieza cuando los clips terminaron de cargar y sigue corriendo;
/// solo cambia su peso.
pub const PLAYED: [Clip; 6] = [
    Clip::Idle,
    Clip::IdleTalk,
    Clip::IdleFoldArms,
    Clip::Walk,
    Clip::Jog,
    Clip::Sprint,
];

/// El clip con el que un jinete se sienta en su caballo (ver
/// [`rider`](super::rider)), el único que reproduce: un clip con peso cero
/// igual fija los huesos que solo él anima.
pub const SEATED: Clip = Clip::Drive;

/// Los clips que reproduce un modelo: los de un jinete, o los de una persona a
/// pie.
pub fn played(riding: bool) -> &'static [Clip] {
    if riding { &[SEATED] } else { &PLAYED }
}

/// Marca a una persona cuyas líneas están en pantalla: parada, habla con las
/// manos (ver [`idle_for`]). El juego se lo pone a quien habla.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Talking;

/// El esqueleto y el estado de clips de un modelo de personaje, ubicados cuando
/// se crea su escena; en la raíz del modelo.
#[derive(Component, Debug, Clone)]
pub struct Rig {
    /// El nodo `Root`: tiene el reproductor de animación, y cada hueso que
    /// cuelga de él es un objetivo de los clips.
    pub player: Entity,
    /// La pelvis, su lugar de reposo bajo `Root`, y el largo de las piernas de
    /// este modelo (de la cadera al tobillo), en metros.
    pub pelvis: Option<(Entity, Vec3, f32)>,
    /// El cuello y la cabeza, cada uno con la orientación de reposo de su padre
    /// en el marco del cuerpo.
    pub look: [Option<(Entity, Quat)>; 2],
    /// Un modelo con pollera larga: sus brazos izquierdo y derecho, cada uno
    /// con la orientación de reposo de su padre en el marco del cuerpo,
    /// separados [`SKIRT_ARMS_OUT`] por encima de los clips.
    pub arms: [Option<(Entity, Quat)>; 2],
    /// Todas las meshes del modelo: está a la vista mientras se dibuje alguna.
    pub probes: Vec<Entity>,
    /// Si los huesos conservan su última pose (ver [`frozen`]).
    pub frozen: bool,
    /// El peso actual de cada clip, en el orden de [`Clip::ALL`].
    pub weights: [f32; Clip::COUNT],
    /// Si sus clips se están reproduciendo.
    pub started: bool,
}

impl Rig {
    /// El rig de un modelo, con sus clips reproducidos por `player`.
    pub fn new(player: Entity) -> Self {
        Self {
            player,
            pelvis: None,
            look: [None; 2],
            arms: [None; 2],
            probes: Vec::new(),
            frozen: false,
            weights: [0.0; Clip::COUNT],
            started: false,
        }
    }
}

/// La ruta de nombres de un hueso desde `Root`, tal como el archivo de los
/// clips nombra sus objetivos: el [`AnimationTargetId`] con el que lo animan
/// los clips.
pub fn target_id(path: &[Name]) -> AnimationTargetId {
    AnimationTargetId::from_names(path.iter())
}

/// Convierte `root` (el nodo `Root` del modelo) en el reproductor de los clips
/// y a cada nodo bajo él, según su ruta de nombres desde `Root`, en uno de sus
/// objetivos.
pub(super) fn bind_targets(
    commands: &mut Commands,
    root: Entity,
    names: &Query<&Name>,
    children: &Query<&Children>,
) {
    let mut stack: Vec<(Entity, Vec<Name>)> = vec![(root, Vec::new())];
    while let Some((entity, mut path)) = stack.pop() {
        let Ok(name) = names.get(entity) else {
            continue;
        };
        path.push(name.clone());
        commands
            .entity(entity)
            .try_insert((target_id(&path), AnimatedBy(root)));
        for &child in children.get(entity).into_iter().flatten() {
            stack.push((child, path.clone()));
        }
    }
    commands.entity(root).try_insert(AnimationPlayer::default());
}

/// El peso que debe tener cada clip para un jinete: solo [`SEATED`].
pub fn seated_targets() -> [f32; Clip::COUNT] {
    let mut out = [0.0; Clip::COUNT];
    out[SEATED.index()] = 1.0;
    out
}

/// El peso que debe tener cada clip, en el orden de [`Clip::ALL`]: una persona
/// de tipo `kind` que se mueve a `speed` (m/s), quizás `talking`.
pub fn targets(kind: FigureKind, talking: bool, speed: f32) -> [f32; Clip::COUNT] {
    let mut out = [0.0; Clip::COUNT];
    let [idle, strides @ ..] = locomotion(speed);
    out[idle_for(kind, talking).index()] = idle;
    for (clip, weight) in STRIDES.into_iter().zip(strides) {
        out[clip.index()] = weight;
    }
    out
}

/// `weights` acercados suavemente a `targets` a lo largo de `dt` segundos.
pub fn ease(weights: &mut [f32; Clip::COUNT], targets: &[f32; Clip::COUNT], dt: f32) {
    let k = 1.0 - (-BLEND_RATE * dt).exp();
    for (w, t) in weights.iter_mut().zip(targets) {
        *w += (t - *w) * k;
    }
}

/// A dónde va la pelvis de un modelo para el lugar `clip` de un clip (hecho
/// sobre el esqueleto `reference`): sus movimientos desde el reposo de la
/// referencia, escalados por la `leg` de este modelo en relación con la de la
/// referencia, alrededor del `rest` de este modelo.
pub fn fit_pelvis(clip: Vec3, reference: Reference, rest: Vec3, leg: f32) -> Vec3 {
    rest + (clip - reference.pelvis) * (leg / reference.leg)
}

/// Qué tan largos son los pasos de un modelo en relación con los de los clips
/// (los de su esqueleto de referencia): el largo de sus piernas dividido por el
/// de la referencia, 1 cuando alguno es desconocido.
pub fn model_stride(pelvis: Option<(Entity, Vec3, f32)>, reference: Option<Reference>) -> f32 {
    match (pelvis, reference) {
        (Some((_, _, leg)), Some(r)) if leg > 0.0 && r.leg > 0.0 => leg / r.leg,
        _ => 1.0,
    }
}

/// La rotación local `local` de un hueso del cuello o de la cabeza, girada
/// `turn` (radianes, alrededor del eje vertical del cuerpo) en el marco del
/// cuerpo; `frame` es la orientación de reposo de su padre en el marco del
/// cuerpo.
pub fn glance_turn(local: Quat, frame: Quat, turn: f32) -> Quat {
    (frame.inverse() * Quat::from_rotation_y(turn) * frame * local).normalize()
}

/// Qué tan rápido se reproduce `clip` para una persona que se mueve a `speed`
/// m/s: al ritmo de la magnitud de la velocidad ([`playback_rate`]), y hacia
/// atrás al retroceder si el clip avanza. Una espera siempre se reproduce hacia
/// adelante.
pub fn stride_rate(clip: Clip, speed: f32) -> f32 {
    let rate = playback_rate(clip, speed.abs());
    if speed < 0.0 && clip.ground_speed().is_some() {
        -rate
    } else {
        rate
    }
}

/// La velocidad sobre el suelo a la que da pasos una persona, negativa al
/// retroceder: ninguna estando parada, salvo el ritmo corto de un giro en el
/// lugar ([`stepping_speed`]).
fn moving(pose: &VisualPose) -> f32 {
    let speed = if pose.animation == AnimationState::Idle {
        0.0
    } else {
        pose.speed
    };
    stepping_speed(speed, pose.turn, TURN_STEP_RADIUS)
}

/// Lo que [`drive_rigs`] lee de un modelo.
type ModelRig = (
    &'static ChildOf,
    &'static mut Rig,
    &'static GlobalTransform,
    &'static mut Transform,
    Option<&'static Riding>,
);

/// Quién es un modelo y qué reproduce: a pie, el tipo y la semilla de su
/// figura, y sus pesos según la velocidad y el habla; en la montura, los de su
/// jinete, sentado.
fn who(
    riding: Option<&Riding>,
    figure: Option<(&FigureBody, f32, bool)>,
) -> Option<(FigureKind, u32, f32, [f32; Clip::COUNT])> {
    match (riding, figure) {
        (Some(r), _) => Some((r.kind, r.seed, 0.0, seated_targets())),
        (None, Some((body, speed, talking))) => Some((
            body.kind,
            body.seed,
            speed,
            targets(body.kind, talking, speed.abs()),
        )),
        (None, None) => None,
    }
}

/// Pondera y marca el ritmo de los clips de cada modelo (ver las notas del
/// módulo), y congela un modelo fuera de la vista o lejano, salvo el del
/// protagonista.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_rigs(
    mut commands: Commands,
    time: Res<Time>,
    clips: Option<Res<CharacterClips>>,
    loaded: Option<Res<Assets<AnimationClip>>>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    bodies: Query<(&FigureBody, &Transform, &ChildOf), Without<Rig>>,
    people: Query<(&VisualPose, Has<Talking>)>,
    mut rigs: Query<ModelRig>,
    mut players: Query<&mut AnimationPlayer>,
    seen: Query<&ViewVisibility>,
) {
    let dt = time.delta_secs();
    let eye = view_eye(cameras);
    for (child_of, mut rig, at, mut transform, riding) in &mut rigs {
        let figure = bodies
            .get(child_of.parent())
            .ok()
            .map(|(body, carried, person)| {
                // Los clips hacen rebotar la pelvis: se deshace el rebote del
                // portador.
                transform.set_if_neq(
                    model_transform().with_translation(Vec3::Y * -carried.translation.y),
                );
                let (speed, talking) = people
                    .get(person.parent())
                    .map_or((0.0, false), |(pose, talking)| (moving(pose), talking));
                (body, speed, talking)
            });
        let Some((kind, seed, speed, goal)) = who(riding, figure) else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        // La visibilidad solo se conoce con una cámara dibujando.
        let in_view =
            eye.is_none() || any_in_view(&rig.probes, |p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = kind != FigureKind::Protagonist && frozen(rig.frozen, distance, in_view);
        let Some(clips) = clips.as_deref() else {
            rig.frozen = freeze;
            continue;
        };
        if freeze != rig.frozen {
            rig.frozen = freeze;
            let mut player = commands.entity(rig.player);
            if freeze {
                player.try_remove::<AnimationGraphHandle>();
            } else {
                player.try_insert(AnimationGraphHandle(clips.graph.clone()));
            }
        }
        if rig.frozen {
            continue;
        }
        let Ok(mut player) = players.get_mut(rig.player) else {
            continue;
        };
        if !rig.started {
            let Some(loaded) = loaded.as_deref() else {
                continue;
            };
            let durations: Option<Vec<f32>> = played(riding.is_some())
                .iter()
                .map(|c| {
                    loaded
                        .get(&clips.clips[c.index()])
                        .map(AnimationClip::duration)
                })
                .collect();
            let Some(durations) = durations else {
                continue;
            };
            let offset = phase_offset(seed);
            for (&clip, duration) in played(riding.is_some()).iter().zip(durations) {
                player
                    .play(clips.node(clip))
                    .repeat()
                    .seek_to(offset * duration);
            }
            rig.weights = goal;
            rig.started = true;
        }
        ease(&mut rig.weights, &goal, dt);
        // Los clips dan los pasos que dan las piernas del protagonista: una
        // persona con las piernas más cortas da pasos más cortos, así que los
        // reproduce más rápido para mantener los pies quietos sobre el suelo.
        let stride = model_stride(rig.pelvis, clips.reference);
        for &clip in played(riding.is_some()) {
            if let Some(active) = player.animation_mut(clips.node(clip)) {
                active
                    .set_weight(rig.weights[clip.index()])
                    .set_speed(stride_rate(clip, speed / stride));
            }
        }
    }
}

/// Ajusta a cada modelo que se reproduce sus clips: su pelvis según sus propias
/// piernas y su cabeza girada según la mirada de su figura.
pub(super) fn fit_rigs(
    clips: Option<Res<CharacterClips>>,
    bodies: Query<&FigureBody>,
    rigs: Query<(&ChildOf, &Rig)>,
    mut transforms: Query<&mut Transform>,
) {
    let reference = clips.and_then(|c| c.reference);
    for (child_of, rig) in &rigs {
        if rig.frozen || !rig.started {
            continue;
        }
        if let (Some(reference), Some((pelvis, rest, leg))) = (reference, rig.pelvis)
            && let Ok(mut t) = transforms.get_mut(pelvis)
        {
            t.translation = fit_pelvis(t.translation, reference, rest, leg);
        }
        let Ok(body) = bodies.get(child_of.parent()) else {
            continue;
        };
        for (entity, frame) in rig.look.into_iter().flatten() {
            if let Ok(mut t) = transforms.get_mut(entity) {
                t.rotation = glance_turn(t.rotation, frame, body.glance / 2.0);
            }
        }
        for ((entity, frame), out) in rig
            .arms
            .into_iter()
            .zip([1.0, -1.0])
            .filter_map(|(a, o)| Some((a?, o)))
        {
            if let Ok(mut t) = transforms.get_mut(entity) {
                t.rotation = arm_out(t.rotation, frame, out * SKIRT_ARMS_OUT);
            }
        }
    }
}

/// La orientación de reposo en el marco del cuerpo del espacio del que cuelga
/// `entity`: las rotaciones de sus ancestros, hasta la `root` del modelo
/// inclusive (girada hacia el frente del cuerpo).
pub(super) fn rest_frame(
    entity: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    transforms: &Query<&Transform>,
) -> Quat {
    let mut frame = Quat::IDENTITY;
    let mut at = entity;
    while at != root {
        let Ok(child_of) = parents.get(at) else {
            break;
        };
        at = child_of.parent();
        if let Ok(t) = transforms.get(at) {
            frame = t.rotation * frame;
        }
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::clips::{JOG_BAND, Reference};
    use crate::character::{CharacterVisual, CharacterVisualPlugin};
    use crate::fauna::FREEZE_FAR;

    #[test]
    fn each_person_plays_their_idle_standing_and_the_strides_moving() {
        let only = |w: [f32; Clip::COUNT], clip: Clip| {
            w.iter()
                .enumerate()
                .all(|(k, w)| *w == if k == clip.index() { 1.0 } else { 0.0 })
        };
        assert!(only(
            targets(FigureKind::Farach, false, 0.0),
            Clip::IdleFoldArms
        ));
        assert!(only(targets(FigureKind::Hidalgo, false, 0.0), Clip::Idle));
        assert!(only(
            targets(FigureKind::Hidalgo, true, 0.0),
            Clip::IdleTalk
        ));
        assert!(only(
            targets(FigureKind::Bairoletto, false, 0.0),
            Clip::Idle
        ));
        assert!(only(
            targets(FigureKind::Protagonist, false, 1.6),
            Clip::Walk
        ));
        assert!(only(
            targets(FigureKind::Protagonist, false, 5.0),
            Clip::Jog
        ));
        assert!(only(targets(FigureKind::Farach, false, 9.0), Clip::Sprint));
        // Entre una caminata y un trote, ambos; parado, la espera de Farach se
        // desvanece cuando arranca.
        let between = targets(FigureKind::Farach, false, (JOG_BAND.0 + JOG_BAND.1) / 2.0);
        assert!(between[Clip::Walk.index()] > 0.4 && between[Clip::Jog.index()] > 0.4);
        let setting_off = targets(FigureKind::Farach, false, 0.3);
        assert!(setting_off[Clip::IdleFoldArms.index()] > 0.3);
        assert!(setting_off[Clip::Walk.index()] > 0.3);
        // Una mezcla suma uno, y pondera solo los clips que reproduce el
        // controlador.
        for w in [between, setting_off] {
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
            for clip in Clip::ALL {
                assert!(PLAYED.contains(&clip) || w[clip.index()] == 0.0, "{clip:?}");
            }
        }
    }

    #[test]
    fn backing_up_plays_the_strides_backward_at_the_forward_pace_and_the_idles_forward() {
        for clip in STRIDES {
            for speed in [0.5, 1.6, 5.0, 9.0] {
                let ahead = stride_rate(clip, speed);
                assert!(ahead > 0.0);
                assert_eq!(stride_rate(clip, -speed), -ahead, "{clip:?} at {speed}");
            }
        }
        for clip in [Clip::Idle, Clip::IdleTalk, Clip::IdleFoldArms, SEATED] {
            assert_eq!(stride_rate(clip, -1.6), 1.0, "{clip:?}");
        }
        assert!(stride_rate(Clip::Walk, 0.0) > 0.0);
        assert!(stride_rate(Clip::Walk, -0.0) > 0.0, "a stop is no reverse");
    }

    #[test]
    fn the_weights_ease_to_their_targets() {
        let mut w = targets(FigureKind::Hidalgo, false, 0.0);
        let walk = targets(FigureKind::Hidalgo, false, 1.5);
        ease(&mut w, &walk, 0.05);
        assert!(
            w[Clip::Walk.index()] > 0.1 && w[Clip::Walk.index()] < 0.5,
            "{w:?}"
        );
        for _ in 0..40 {
            ease(&mut w, &walk, 0.05);
        }
        assert!((w[Clip::Walk.index()] - 1.0).abs() < 1e-3 && w[Clip::Idle.index()] < 1e-3);
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "still adds up");
    }

    #[test]
    fn a_models_pelvis_moves_by_its_own_legs_about_its_own_rest() {
        let reference = Reference {
            pelvis: Vec3::new(0.94, 0.03, 0.0),
            leg: 0.865,
        };
        let rest = Vec3::new(0.91, 0.02, 0.0);
        let leg = 0.83;
        assert!(fit_pelvis(reference.pelvis, reference, rest, leg).distance(rest) < 1e-6);
        let bob = Vec3::new(-0.05, 0.01, 0.02);
        let fitted = fit_pelvis(reference.pelvis + bob, reference, rest, leg);
        assert!(fitted.distance(rest + bob * (leg / reference.leg)) < 1e-6);
    }

    #[test]
    fn a_glance_turns_the_bone_about_the_bodys_up() {
        let frame = Quat::from_euler(EulerRot::XYZ, 0.7, -1.2, 2.1);
        let local = Quat::from_euler(EulerRot::XYZ, -0.4, 0.3, 0.9);
        let turned = frame * glance_turn(local, frame, 0.3);
        assert!(turned.angle_between(Quat::from_rotation_y(0.3) * frame * local) < 1e-5);
        assert!(glance_turn(local, frame, 0.0).angle_between(local) < 1e-5);
    }

    /// El plugin de personajes sin assets, con clips sustitutos y un modelo
    /// puesto a mano bajo la figura de cada una de las `people`: el reproductor
    /// de su rig y dos meshes (un sombrero y después el cuerpo). Devuelve el
    /// tipo, el rig y las meshes de cada persona.
    fn stand_ins(people: &[FigureKind]) -> (App, Vec<(FigureKind, Entity, [Entity; 2])>) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(CharacterVisualPlugin);
        app.insert_resource(CharacterClips {
            file: Handle::default(),
            clips: vec![Handle::default(); Clip::COUNT],
            graph: Handle::default(),
            nodes: (0..Clip::COUNT)
                .map(|k| bevy::animation::graph::AnimationNodeIndex::new(k + 1))
                .collect(),
            reference: None,
        });
        let people: Vec<(FigureKind, Entity)> = people
            .iter()
            .map(|&kind| {
                (
                    kind,
                    app.world_mut().spawn(CharacterVisual::Figure(kind)).id(),
                )
            })
            .collect();
        app.update();
        let mut out = Vec::new();
        for (kind, person) in people {
            let world = app.world_mut();
            let body = world
                .get::<Children>(person)
                .unwrap()
                .iter()
                .find(|c| world.get::<FigureBody>(*c).is_some())
                .unwrap();
            let player = world
                .spawn((AnimationPlayer::default(), AnimationGraphHandle::default()))
                .id();
            let meshes = [(); 2].map(|_| world.spawn(ViewVisibility::VISIBLE).id());
            let mut rig = Rig::new(player);
            rig.probes = meshes.to_vec();
            let model = world.spawn((rig, Transform::default(), ChildOf(body))).id();
            out.push((kind, model, meshes));
        }
        (app, out)
    }

    fn frozen_now(app: &App, rig: Entity) -> bool {
        let rig = app.world().get::<Rig>(rig).unwrap();
        let graph = app
            .world()
            .get::<AnimationGraphHandle>(rig.player)
            .is_some();
        assert_eq!(rig.frozen, !graph, "the graph goes with the freeze");
        rig.frozen
    }

    #[test]
    fn a_model_far_off_or_out_of_view_keeps_its_pose_but_the_protagonists_never_does() {
        let (mut app, rigs) = stand_ins(&[FigureKind::Hidalgo, FigureKind::Protagonist]);
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Transform::from_xyz(0.0, 1.6, 4.0)))
            .id();
        // La posición de la cámara llega al controlador con un frame de
        // retraso.
        app.update();
        app.update();
        for &(_, rig, _) in &rigs {
            assert!(!frozen_now(&app, rig));
        }
        // Lejos: Don Hidalgo se congela; el protagonista sigue.
        let far = |app: &mut App, z: f32| {
            app.world_mut()
                .get_mut::<Transform>(camera)
                .unwrap()
                .translation
                .z = z;
            app.update();
            app.update();
        };
        far(&mut app, FREEZE_FAR + 50.0);
        for &(kind, rig, _) in &rigs {
            assert_eq!(
                frozen_now(&app, rig),
                kind != FigureKind::Protagonist,
                "{kind:?}"
            );
        }
        // Cerca otra vez: vuelve a reproducir.
        far(&mut app, 4.0);
        for &(_, rig, _) in &rigs {
            assert!(!frozen_now(&app, rig));
        }
        // Su sombrero descartado por culling, su cuerpo dibujado: sigue a la
        // vista.
        for &(_, _, [hat, _]) in &rigs {
            app.world_mut()
                .entity_mut(hat)
                .insert(ViewVisibility::HIDDEN);
        }
        app.update();
        for &(_, rig, _) in &rigs {
            assert!(!frozen_now(&app, rig));
        }
        // No se dibuja nada de él: fuera de la vista, congelado, salvo el
        // protagonista.
        for &(_, _, [_, body]) in &rigs {
            app.world_mut()
                .entity_mut(body)
                .insert(ViewVisibility::HIDDEN);
        }
        app.update();
        for &(kind, rig, _) in &rigs {
            assert_eq!(
                frozen_now(&app, rig),
                kind != FigureKind::Protagonist,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_model_undoes_the_carriers_stride_bob() {
        use crate::character::VisualPose;
        let (mut app, rigs) = stand_ins(&[FigureKind::Protagonist]);
        let person = app
            .world_mut()
            .query_filtered::<Entity, With<CharacterVisual>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(person).insert(VisualPose {
            animation: AnimationState::Walk,
            speed: 1.5,
            ..default()
        });
        let mut bobbed = false;
        for _ in 0..20 {
            app.update();
            let world = app.world();
            let model = world.get::<Transform>(rigs[0].1).unwrap();
            let body = world.get::<ChildOf>(rigs[0].1).unwrap().parent();
            let carried = world.get::<Transform>(body).unwrap().translation.y;
            assert!((model.translation.y + carried).abs() < 1e-6);
            assert!(model.rotation.abs_diff_eq(model_transform().rotation, 1e-6));
            bobbed |= carried.abs() > 1e-3;
        }
        assert!(bobbed, "the carrier bobs while walking");
    }

    // --- con los modelos y los clips incluidos ------------------------------

    /// El plugin de personajes con el servidor de assets y el plugin de
    /// animación, y las `people` separadas un metro a lo largo de +X, en la
    /// pose `animation` a `speed`; vuelve cuando cada modelo reproduce sus
    /// clips, cada rig con su tipo.
    fn playing(
        people: &[FigureKind],
        animation: AnimationState,
        speed: f32,
    ) -> (App, Vec<(FigureKind, Entity)>) {
        use crate::character::VisualPose;
        use crate::gltf::tests::{root, unfinished_app};
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(50),
            ))
            .add_plugins(CharacterVisualPlugin);
        app.finish();
        app.cleanup();
        for (k, &kind) in people.iter().enumerate() {
            app.world_mut().spawn((
                CharacterVisual::Figure(kind),
                VisualPose {
                    animation,
                    speed,
                    ..VisualPose::default()
                },
                Transform::from_xyz(k as f32, 0.0, 0.0),
            ));
        }
        for _ in 0..2000 {
            app.update();
            let world = app.world_mut();
            let mut rigs = world.query::<(Entity, &Rig, &ChildOf)>();
            let found: Vec<(Entity, Entity)> = rigs
                .iter(world)
                .filter(|(_, r, _)| r.started)
                .map(|(e, _, c)| (e, c.parent()))
                .collect();
            let referenced = world.resource::<CharacterClips>().reference.is_some();
            if found.len() == people.len() && referenced {
                let kinds = found
                    .into_iter()
                    .map(|(rig, body)| (world.get::<FigureBody>(body).unwrap().kind, rig))
                    .collect();
                return (app, kinds);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the models never played");
    }

    /// Pone a todos en `app` en la pose `animation` a `speed` a partir del
    /// frame siguiente.
    fn repose(app: &mut App, animation: AnimationState, speed: f32) {
        use crate::character::VisualPose;
        let world = app.world_mut();
        let people: Vec<Entity> = world
            .query_filtered::<Entity, With<CharacterVisual>>()
            .iter(world)
            .collect();
        for person in people {
            world.entity_mut(person).insert(VisualPose {
                animation,
                speed,
                ..VisualPose::default()
            });
        }
    }

    /// La entidad llamada `name` bajo el modelo `rig`.
    fn bone(world: &World, rig: Entity, name: &str) -> Entity {
        let children = |e: Entity| world.get::<Children>(e).into_iter().flatten().copied();
        let mut stack: Vec<Entity> = children(rig).collect();
        while let Some(e) = stack.pop() {
            if world.get::<Name>(e).is_some_and(|n| n.as_str() == name) {
                return e;
            }
            stack.extend(children(e));
        }
        panic!("no {name}");
    }

    fn at(world: &World, entity: Entity) -> Vec3 {
        world.get::<GlobalTransform>(entity).unwrap().translation()
    }

    #[test]
    fn the_clips_bind_to_the_models_bones_and_move_them() {
        let (mut app, rigs) = playing(&[FigureKind::Protagonist], AnimationState::Walk, 1.5);
        let rig = rigs[0].1;
        let world = app.world_mut();
        // Cada hueso que animan los clips es un hueso del modelo.
        let mut ids = world.query::<&AnimationTargetId>();
        let model: std::collections::HashSet<AnimationTargetId> =
            ids.iter(world).copied().collect();
        let clips = world.resource::<CharacterClips>().clone();
        let walk = world
            .resource::<Assets<AnimationClip>>()
            .get(&clips.clips[Clip::Walk.index()])
            .unwrap();
        let animated: Vec<&AnimationTargetId> = walk.curves().keys().collect();
        assert_eq!(animated.len(), 52);
        assert!(animated.iter().all(|id| model.contains(id)));
        // Y los mueven: el muslo se balancea a lo largo de la zancada.
        let thigh = bone(world, rig, "thigh_l");
        let mut turns = Vec::new();
        for _ in 0..30 {
            app.update();
            turns.push(app.world().get::<Transform>(thigh).unwrap().rotation);
        }
        let swing = turns
            .iter()
            .map(|q| q.angle_between(turns[0]))
            .fold(0.0, f32::max);
        assert!(swing > 0.5, "{swing}");
    }

    /// Las almohadillas de los pies en cada frame de 50 ms, en posiciones del
    /// mundo, durante `frames`.
    fn balls(app: &mut App, rig: Entity, frames: usize) -> Vec<[Vec3; 2]> {
        let ends = ["ball_l", "ball_r"].map(|n| bone(app.world(), rig, n));
        (0..frames)
            .map(|_| {
                app.update();
                ends.map(|e| at(app.world(), e))
            })
            .collect()
    }

    /// Qué tan rápido se desliza hacia atrás en promedio el pie apoyado (la
    /// almohadilla más baja, sobre el suelo), en m/s.
    fn slide(frames: &[[Vec3; 2]]) -> f32 {
        let ground = frames
            .iter()
            .flatten()
            .map(|p| p.y)
            .fold(f32::MAX, f32::min);
        let mut slides = Vec::new();
        for pair in frames.windows(2) {
            for k in 0..2 {
                if pair.iter().all(|f| f[k].y < ground + 0.01) {
                    slides.push(-(pair[1][k].x - pair[0][k].x) / 0.05);
                }
            }
        }
        assert!(slides.len() > 5, "{slides:?}");
        slides.sort_by(f32::total_cmp);
        slides[slides.len() / 2]
    }

    #[test]
    fn a_planted_foot_slides_at_the_ground_speed_walking_jogging_and_backing_up() {
        // En el lugar, el pie apoyado se mueve hacia atrás bajo el cuerpo tan
        // rápido como el cuerpo avanza: sobre el suelo se queda quieto. Al
        // retroceder, se mueve hacia adelante tan rápido como el cuerpo
        // retrocede: sin moonwalk.
        let (mut app, rigs) = playing(&[FigureKind::Protagonist], AnimationState::Walk, 1.6);
        for (animation, speed) in [
            (AnimationState::Walk, 1.6),
            (AnimationState::Gallop, 5.0),
            (AnimationState::Walk, -1.6),
        ] {
            repose(&mut app, animation, speed);
            balls(&mut app, rigs[0].1, 30);
            let frames = balls(&mut app, rigs[0].1, 120);
            let slid = slide(&frames);
            assert!(
                (slid / speed - 1.0).abs() < 0.12,
                "{animation:?}: {slid} m/s at {speed}"
            );
        }
    }

    #[test]
    fn every_model_stands_on_its_own_feet_and_farach_folds_his_arms() {
        let people = [
            FigureKind::Protagonist,
            FigureKind::Hidalgo,
            FigureKind::Farach,
        ];
        let (mut app, rigs) = playing(&people, AnimationState::Idle, 0.0);
        let mut lowest = Vec::new();
        for &(kind, rig) in &rigs {
            let frames = balls(&mut app, rig, 20);
            let low = frames
                .iter()
                .flatten()
                .map(|p| p.y)
                .fold(f32::MAX, f32::min);
            lowest.push((kind, low));
        }
        // Las almohadillas de los pies de cada modelo descansan unos 2.4 cm por
        // encima; una pelvis puesta donde está la del protagonista levantaría
        // la de Don Hidalgo 2.5 cm.
        for &(kind, low) in &lowest {
            assert!((low - 0.024).abs() < 0.012, "{kind:?}: {low}");
        }
        let (_, farach) = *rigs.iter().find(|(k, _)| *k == FigureKind::Farach).unwrap();
        let rig = app.world().get::<Rig>(farach).unwrap();
        assert!(rig.weights[Clip::IdleFoldArms.index()] > 0.99);
        // Sus manos están cruzadas sobre el pecho: cada una del otro lado.
        let world = app.world();
        let body = world.get::<ChildOf>(farach).unwrap().parent();
        let to_body = world
            .get::<GlobalTransform>(body)
            .unwrap()
            .affine()
            .inverse();
        let side = |name: &str| to_body.transform_point3(at(world, bone(world, farach, name)));
        let (left, right) = (side("hand_l"), side("hand_r"));
        // La izquierda del cuerpo es -Z.
        assert!(left.z > right.z, "{left} {right}");
        assert!(left.y > 1.1 && right.y > 1.1, "{left} {right}");
    }

    #[test]
    fn turning_in_place_takes_slow_steps_and_stopping_the_turn_goes_back_to_idle() {
        use crate::character::VisualPose;
        let (mut app, rigs) = playing(&[FigureKind::Protagonist], AnimationState::Idle, 0.0);
        let rig = rigs[0].1;
        let person = app
            .world_mut()
            .query_filtered::<Entity, With<CharacterVisual>>()
            .single(app.world())
            .unwrap();
        let state = |app: &mut App| {
            let world = app.world_mut();
            let weight = world.get::<Rig>(rig).unwrap().weights[Clip::Walk.index()];
            let mut players = world.query::<&AnimationPlayer>();
            let player = world.get::<Rig>(rig).unwrap().player;
            let clips = world.resource::<CharacterClips>().node(Clip::Walk);
            let rate = players
                .get(world, player)
                .unwrap()
                .animation(clips)
                .unwrap()
                .speed();
            (weight, rate)
        };
        // Parado, sin girar: espera, con la caminata en peso cero.
        for _ in 0..10 {
            app.update();
        }
        assert!(state(&mut app).0 < 1e-3);
        // Parado y girando (la lógica igual dice Idle sin velocidad).
        app.world_mut().entity_mut(person).insert(VisualPose {
            animation: AnimationState::Idle,
            speed: 0.0,
            turn: 1.8,
            ..default()
        });
        for _ in 0..30 {
            app.update();
        }
        let (weight, rate) = state(&mut app);
        assert!(weight > 0.2 && weight < 0.95, "{weight}");
        assert!(rate > 0.0 && rate <= 0.6, "{rate}");
        // Liberado: de vuelta a la espera.
        app.world_mut()
            .entity_mut(person)
            .insert(VisualPose::default());
        for _ in 0..60 {
            app.update();
        }
        assert!(state(&mut app).0 < 1e-2);
    }

    #[test]
    fn whoever_is_marked_talking_talks_with_their_hands_and_stops_when_unmarked() {
        let (mut app, rigs) = playing(&[FigureKind::Hidalgo], AnimationState::Idle, 0.0);
        let rig = rigs[0].1;
        let person = app
            .world_mut()
            .query_filtered::<Entity, With<CharacterVisual>>()
            .single(app.world())
            .unwrap();
        let weight =
            |app: &App, clip: Clip| app.world().get::<Rig>(rig).unwrap().weights[clip.index()];
        assert!(weight(&app, Clip::Idle) > 0.99 && weight(&app, Clip::IdleTalk) < 0.01);
        // El juego marca a quien habla en su entidad de personaje.
        app.world_mut().entity_mut(person).insert(Talking);
        for _ in 0..30 {
            app.update();
        }
        assert!(
            weight(&app, Clip::IdleTalk) > 0.99,
            "{}",
            weight(&app, Clip::IdleTalk)
        );
        assert!(weight(&app, Clip::Idle) < 0.01);
        app.world_mut().entity_mut(person).remove::<Talking>();
        for _ in 0..30 {
            app.update();
        }
        assert!(weight(&app, Clip::Idle) > 0.99 && weight(&app, Clip::IdleTalk) < 0.01);
    }

    #[test]
    fn the_glance_turns_the_head_within_bounds_and_never_adds_up() {
        let (mut app, rigs) = playing(&[FigureKind::Hidalgo], AnimationState::Idle, 0.0);
        let rig = rigs[0].1;
        let body = app.world().get::<ChildOf>(rig).unwrap().parent();
        let (head, chest) = (
            bone(app.world(), rig, "head"),
            bone(app.world(), rig, "spine_03"),
        );
        // El giro de la cabeza alrededor del eje vertical del cuerpo, a partir
        // del del pecho.
        let yaw = |app: &App| {
            let world = app.world();
            let turn = |e: Entity| {
                world
                    .get::<GlobalTransform>(e)
                    .unwrap()
                    .to_scale_rotation_translation()
                    .1
            };
            let to_body = turn(body).inverse();
            let ahead = |q: Quat| {
                let f = to_body * q * Vec3::Z;
                f.z.atan2(f.x)
            };
            let d = ahead(turn(head)) - ahead(turn(chest));
            (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
        };
        let mut aside = 0.0f32;
        let mut ahead = Vec::new();
        // Quince segundos: una mirada hacia un costado y de vuelta.
        for _ in 0..300 {
            app.update();
            let glance = app.world().get::<FigureBody>(body).unwrap().glance;
            let y = yaw(&app);
            aside = aside.max((y - ahead.first().copied().unwrap_or(y)).abs());
            if glance.abs() < 0.005 {
                ahead.push(y);
            }
        }
        assert!(ahead.len() > 100, "{}", ahead.len());
        // Mirando al frente está donde estaba cada vez: nada se acumula.
        let (lo, hi) = ahead
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(hi - lo < 0.1, "looking ahead from {lo} to {hi}");
        // Una mirada la gira, nunca más allá del alcance de la mirada (0.55
        // rad).
        assert!(aside > 0.15 && aside < 0.65, "{aside}");
    }

    #[test]
    fn walking_knees_bend_smoothly_never_lock_or_twist_round_the_loop() {
        use crate::character::VisualPose;
        // El protagonista a su 1.6 m/s, un peón y una mujer al 1.4 de un
        // vecino, a 60 frames por segundo durante un ciclo completo y algo más,
        // pasando por su vuelta al inicio.
        let walkers = [
            (FigureKind::Protagonist, 1.6),
            (FigureKind::townsfolk(1), 1.4),
            (FigureKind::townsfolk(3), 1.4),
        ];
        let kinds = walkers.map(|(kind, _)| kind);
        let (mut app, rigs) = playing(&kinds, AnimationState::Walk, 1.4);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        ));
        struct Walker {
            kind: FigureKind,
            speed: f32,
            body: Entity,
            legs: [[Entity; 3]; 2],
            pelvis: Entity,
            least: f32,
            jump: f32,
            twist: f32,
            step: f32,
            last: Option<([f32; 2], f32)>,
        }
        let mut walkers: Vec<Walker> = rigs
            .iter()
            .map(|&(kind, rig)| {
                let speed = walkers.iter().find(|(k, _)| *k == kind).unwrap().1;
                let world = app.world();
                let body = world.get::<ChildOf>(rig).unwrap().parent();
                let legs = ["l", "r"].map(|s| {
                    [
                        format!("thigh_{s}"),
                        format!("calf_{s}"),
                        format!("foot_{s}"),
                    ]
                    .map(|n| bone(world, rig, &n))
                });
                Walker {
                    kind,
                    speed,
                    body,
                    legs,
                    pelvis: bone(world, rig, "pelvis"),
                    least: f32::MAX,
                    jump: 0.0,
                    twist: 0.0,
                    step: 0.0,
                    last: None,
                }
            })
            .collect();
        for w in &walkers {
            let person = app.world().get::<ChildOf>(w.body).unwrap().parent();
            app.world_mut().entity_mut(person).insert(VisualPose {
                animation: AnimationState::Walk,
                speed: w.speed,
                ..default()
            });
        }
        for _ in 0..30 {
            app.update();
        }
        for _ in 0..100 {
            app.update();
            let world = app.world();
            for w in &mut walkers {
                let forward = world.get::<GlobalTransform>(w.body).unwrap().rotation() * Vec3::X;
                let mut bends = [0.0; 2];
                for (k, [hip, knee, ankle]) in w.legs.iter().enumerate() {
                    let (h, n, a) = (at(world, *hip), at(world, *knee), at(world, *ankle));
                    let (thigh, shin) = ((n - h).normalize(), (a - n).normalize());
                    bends[k] = thigh.angle_between(shin).to_degrees();
                    // Doblada hacia atrás, nunca hacia adelante: la canilla
                    // gira desde el muslo alejándose del frente del cuerpo.
                    let ahead = thigh.cross(shin).dot(thigh.cross(forward));
                    if ahead > 0.0 && bends[k] > 1.0 {
                        bends[k] = -bends[k];
                    }
                    let plane = thigh.cross(forward).normalize();
                    w.twist = w
                        .twist
                        .max(shin.dot(plane).clamp(-1.0, 1.0).asin().to_degrees().abs());
                }
                w.least = w.least.min(bends[0]).min(bends[1]);
                let y = at(world, w.pelvis).y;
                if let Some((was, height)) = w.last {
                    w.jump = w
                        .jump
                        .max((bends[0] - was[0]).abs())
                        .max((bends[1] - was[1]).abs());
                    w.step = w.step.max((y - height).abs());
                }
                w.last = Some((bends, y));
            }
        }
        for w in &walkers {
            let (kind, speed) = (w.kind, w.speed);
            println!(
                "{kind:?} at {speed} m/s: knees bend {:.1} deg at least, change {:.1} deg a frame at most, \
                 shins {:.1} deg off the hinge plane, the pelvis moves {:.1} mm a frame at most",
                w.least,
                w.jump,
                w.twist,
                w.step * 1000.0
            );
            assert!(
                w.least >= 3.0,
                "{kind:?}: a knee locks or bends back ({})",
                w.least
            );
            assert!(
                w.jump < 12.0,
                "{kind:?}: a knee snaps {} degrees in a frame",
                w.jump
            );
            assert!(
                w.twist < 6.0,
                "{kind:?}: a shin twists {} degrees off its hinge",
                w.twist
            );
            assert!(
                w.step < 0.012,
                "{kind:?}: the hips step {} m in a frame",
                w.step
            );
        }
    }

    #[test]
    fn the_women_keep_their_legs_in_their_skirts_and_their_hands_out_of_them() {
        use crate::character::townsfolk::tests::skirt_pokes;
        // La campesina con pañuelo en la cabeza y la joven del pueblo con
        // rodete.
        let women = [FigureKind::townsfolk(3), FigureKind::townsfolk(4)];
        let (mut app, rigs) = playing(&women, AnimationState::Idle, 0.0);
        for &(_, rig) in &rigs {
            let arms = app.world().get::<Rig>(rig).unwrap().arms;
            assert!(arms.iter().all(Option::is_some), "her arms are held out");
        }
        for (animation, speed, talking) in [
            (AnimationState::Idle, 0.0, false),
            (AnimationState::Walk, 1.6, false),
            (AnimationState::Idle, 0.0, true),
        ] {
            repose(&mut app, animation, speed);
            if talking {
                let people: Vec<Entity> = app
                    .world_mut()
                    .query_filtered::<Entity, With<CharacterVisual>>()
                    .iter(app.world())
                    .collect();
                for person in people {
                    app.world_mut().entity_mut(person).insert(Talking);
                }
            }
            // Hasta entrar en el clip (la charla entra de forma gradual), y
            // después dos segundos de él.
            for _ in 0..30 {
                app.update();
            }
            let (mut legs, mut arms) = (-1.0f32, -1.0f32);
            for _ in 0..40 {
                app.update();
                for &(kind, rig) in &rigs {
                    let (l, a) = skirt_pokes(app.world_mut(), rig).expect("a skirt");
                    assert!(l > -1.0, "{kind:?}: no leg met the skirt");
                    legs = legs.max(l);
                    arms = arms.max(a);
                }
            }
            println!("{animation:?} talking {talking}: legs {legs:.4} m out, arms {arms:.4} m in");
            assert!(
                legs <= 0.0,
                "{animation:?}: a leg {legs} m through the skirt"
            );
            assert!(arms <= 0.0, "{animation:?}: a hand {arms} m into the skirt");
        }
    }

    #[test]
    fn townsfolk_wear_their_outfits_in_tinted_cloth_shared_by_whoever_wears_the_same() {
        use crate::character::townsfolk::{TintCache, tint};
        use bevy::gltf::GltfMaterialName;
        // Cada atuendo una vez, y dos personas vestidas igual.
        let mut looks: Vec<u8> = (0..5).collect();
        looks.push(3);
        let crowd: Vec<FigureKind> = looks.iter().map(|&l| FigureKind::townsfolk(l)).collect();
        let (mut app, rigs) = playing(&crowd, AnimationState::Idle, 0.0);
        app.update();
        let world = app.world_mut();
        // El aspecto de cada persona y su ropa: (nombre del material,
        // material).
        type Cloth = Vec<(String, AssetId<StandardMaterial>)>;
        let mut worn: Vec<(u8, Cloth)> = Vec::new();
        for &(kind, rig) in &rigs {
            let FigureKind::Townsfolk(look) = kind else {
                panic!("{kind:?}");
            };
            let mut cloth = Vec::new();
            let mut stack = vec![rig];
            while let Some(e) = stack.pop() {
                if let (Some(m), Some(name)) = (
                    world.get::<MeshMaterial3d<StandardMaterial>>(e),
                    world.get::<GltfMaterialName>(e),
                ) {
                    let colour = world
                        .resource::<Assets<StandardMaterial>>()
                        .get(&m.0)
                        .unwrap()
                        .base_color;
                    if let Some(rgb) = tint(look, &name.0) {
                        assert_eq!(
                            colour,
                            Color::srgb_u8(rgb[0], rgb[1], rgb[2]),
                            "{look} {}",
                            name.0
                        );
                    }
                    cloth.push((name.0.clone(), m.0.id()));
                }
                stack.extend(world.get::<Children>(e).into_iter().flatten().copied());
            }
            cloth.sort();
            worn.push((look, cloth));
        }
        // El mismo aspecto, los mismos materiales (exactamente las mismas
        // copias).
        let alike: Vec<&Vec<_>> = worn
            .iter()
            .filter(|(l, _)| *l == 3)
            .map(|(_, c)| c)
            .collect();
        assert_eq!(alike.len(), 2);
        assert_eq!(alike[0], alike[1]);
        // Cada copia teñida se usa, y se comparte: menos copias que prendas
        // teñidas en la multitud.
        let copies = world.resource::<TintCache>().len();
        let mut handles = std::collections::HashSet::new();
        let mut pieces = 0;
        for (look, cloth) in &worn {
            for (name, id) in cloth {
                if tint(*look, name).is_some() {
                    handles.insert(*id);
                    pieces += 1;
                }
            }
        }
        assert!(copies > 5, "{copies}");
        assert_eq!(handles.len(), copies);
        assert!(
            copies < pieces,
            "{copies} copies for {pieces} tinted pieces"
        );
        assert!(worn.iter().all(|(_, c)| c.len() >= 5));
    }

    /// P4: el costo de CPU de una plaza llena, headless: los 31 vecinos que
    /// tiene General Pico, cada uno con el modelo de su aspecto, y el
    /// protagonista, todos dentro del rango de congelamiento y a la vista (nada
    /// congelado), dos de cada tres caminando. Imprime, por frame, la
    /// actualización completa con ellos comparada con la misma app sin ellos
    /// (la ponderación de los clips, la animación de Bevy, el ajuste de la
    /// pelvis y la mirada, la pose de los portadores y los transforms), la
    /// preparación del skinning (las matrices de articulación de cada mesh con
    /// skinning, tal como las calcula la extracción del renderizador) y los
    /// triángulos dibujados. Se ejecuta en release:
    /// `cargo test -p pampa-render --release --lib plaza -- --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn plaza_bench() {
        use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
        use std::time::Instant;
        const FRAMES: usize = 600;
        let people: Vec<FigureKind> = std::iter::once(FigureKind::Protagonist)
            .chain((0..31).map(FigureKind::townsfolk))
            .collect();
        let (mut app, rigs) = playing(&people, AnimationState::Walk, 1.6);
        // Dos de cada tres caminan y el resto está parado; repartidos por la
        // plaza.
        let persons: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<CharacterVisual>>()
            .iter(app.world())
            .collect();
        for (k, &person) in persons.iter().enumerate() {
            let mut e = app.world_mut().entity_mut(person);
            e.get_mut::<Transform>().unwrap().translation =
                Vec3::new((k % 8) as f32 * 6.0, 0.0, (k / 8) as f32 * 6.0);
            if k % 3 == 2 {
                e.insert(crate::character::VisualPose::default());
            }
        }
        for _ in 0..60 {
            app.update();
        }
        let time = |app: &mut App, frames: usize| {
            let start = Instant::now();
            for _ in 0..frames {
                app.update();
            }
            start.elapsed().as_secs_f64() * 1e3 / frames as f64
        };
        let with = time(&mut app, FRAMES);
        // Las piezas rígidas de las figuras, ocultas bajo los modelos, que se
        // conservan para `models off`: lo que cuestan (sus transforms,
        // calculados en cada frame bajo el cuerpo en movimiento).
        let parts: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<crate::character::FigurePart>>()
            .iter(app.world())
            .collect();
        let hidden = parts.len();
        for part in parts {
            app.world_mut().entity_mut(part).despawn();
        }
        let bare = time(&mut app, FRAMES);
        // La preparación del skinning: la matriz de cada articulación, en cada
        // frame.
        let world = app.world_mut();
        let mut skins = world.query::<&SkinnedMesh>();
        let skins: Vec<SkinnedMesh> = skins.iter(world).cloned().collect();
        let binds = world.resource::<Assets<SkinnedMeshInverseBindposes>>();
        let binds: Vec<Vec<Mat4>> = skins
            .iter()
            .map(|s| {
                binds
                    .get(&s.inverse_bindposes)
                    .map_or(Vec::new(), |b| b.to_vec())
            })
            .collect();
        let start = Instant::now();
        let mut joints = 0usize;
        let mut sink = Mat4::ZERO;
        for _ in 0..FRAMES {
            for (skin, ibm) in skins.iter().zip(&binds) {
                for (j, b) in skin.joints.iter().zip(ibm.iter()) {
                    if let Some(g) = world.get::<GlobalTransform>(*j) {
                        sink += g.to_matrix() * *b;
                        joints += 1;
                    }
                }
            }
        }
        let prep = start.elapsed().as_secs_f64() * 1e3 / FRAMES as f64;
        std::hint::black_box(sink);
        // Triángulos dibujados: cada mesh bajo cada modelo.
        let mut triangles = 0usize;
        let mut q = world.query::<(&Mesh3d, &ChildOf)>();
        let drawn: Vec<(Mesh3d, Entity)> = q
            .iter(world)
            .map(|(m, c)| (m.clone(), c.parent()))
            .collect();
        let meshes = world.resource::<Assets<Mesh>>();
        let models: std::collections::HashSet<Entity> = rigs.iter().map(|r| r.1).collect();
        for (mesh, parent) in &drawn {
            let mut at = *parent;
            let mut under = false;
            for _ in 0..64 {
                if models.contains(&at) {
                    under = true;
                    break;
                }
                match world.get::<ChildOf>(at) {
                    Some(c) => at = c.parent(),
                    None => break,
                }
            }
            if under && let Some(m) = meshes.get(&mesh.0) {
                triangles += m.indices().map_or(0, |i| i.len() / 3);
            }
        }
        // La misma app sin nadie.
        for person in persons {
            app.world_mut().entity_mut(person).despawn();
        }
        for _ in 0..30 {
            app.update();
        }
        let without = time(&mut app, FRAMES);
        println!(
            "plaza ({} people, {} walking): update {with:.3} ms/frame, {without:.3} without them: {:.3} ms for them, \
             of which {:.3} ms for the {hidden} hidden rigid parts; skinning prep {prep:.3} ms/frame \
             ({} joints); {triangles} triangles drawn",
            people.len(),
            people.len() - people.len() / 3,
            with - without,
            with - bare,
            joints / FRAMES,
        );
    }

    #[test]
    fn the_poncho_stays_over_the_shoulder_and_arm_on_foot() {
        let (mut app, rigs) = playing(&[FigureKind::Protagonist], AnimationState::Idle, 0.0);
        let model = rigs[0].1;
        for (animation, speed) in [
            (AnimationState::Idle, 0.0),
            (AnimationState::Walk, 1.6),
            (AnimationState::Gallop, 5.0),
        ] {
            repose(&mut app, animation, speed);
            // Hasta entrar en el clip, y después un segundo y medio de él.
            for _ in 0..30 {
                app.update();
            }
            let mut deepest = 0.0f32;
            for _ in 0..30 {
                app.update();
                deepest = deepest.max(crate::character::rider::tests::poncho_in_arms(
                    app.world_mut(),
                    model,
                ));
            }
            println!("{animation:?}: the poncho {deepest:.3} m into an arm");
            assert!(
                deepest < 0.005,
                "{animation:?}: the poncho {deepest} m into an arm"
            );
        }
    }
}
