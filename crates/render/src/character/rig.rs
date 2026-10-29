//! The clip driver: plays the shared animation clips ([`clips`](super::clips))
//! on each MakeHuman model (see [`model`](super::model)), with the head's
//! glance on top.
//!
//! The figure stays the carrier: its body turns to the heading, carrying
//! the model, and [`update_figures`](super::figure) still keeps its hidden
//! joints posed for `models off`. The model's own clips now move the body:
//!
//! - [`drive_rigs`], before the animation systems, picks and weighs the
//!   clips. Standing, a person plays their idle ([`idle_for`]); moving,
//!   the idle fades into the walk, the jog and the sprint by ground speed
//!   ([`locomotion`]), each walking clip paced to that speed
//!   ([`stride_rate`]), over the model's legs against the clips'
//!   ([`model_stride`]), so the feet keep still on the ground; backing
//!   up, the walking clips play backward at the same pace; turning on the
//!   spot, the walk plays at the small pace of [`stepping_speed`]. The weights
//!   ease toward their targets, and every clip starts at the person's own
//!   seeded point ([`phase_offset`]). The model also undoes the carrier's
//!   stride bob: the clips bob the pelvis themselves.
//! - [`fit_rigs`], after them, fits the clip to the model: the clips were
//!   made on the protagonist, so the pelvis's moves are scaled by this
//!   model's legs and set about its own rest ([`fit_pelvis`]); then the
//!   neck and head turn by the figure's glance, half each, on top of the
//!   clip ([`glance_turn`]); a model in a long skirt has its upper arms
//!   turned out a little ([`arm_out`]).
//!
//! A rider (see [`rider`](super::rider)) plays only the seated clip
//! ([`SEATED`]): a clip at no weight still sets the bones only it animates.
//!
//! A model out of view (none of its meshes drawn) or farther than
//! [`FREEZE_FAR`](crate::fauna::FREEZE_FAR) from the camera loses its graph,
//! as the livestock do: its bones keep their last pose. The protagonist,
//! only ever the player, never freezes.

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

/// How fast the clips' weights ease toward their targets, per second.
pub const BLEND_RATE: f32 = 6.0;

/// The clips a model on foot may play: the idles and the strides. Each
/// starts once the clips have loaded and keeps running; only its weight
/// changes.
pub const PLAYED: [Clip; 6] = [
    Clip::Idle,
    Clip::IdleTalk,
    Clip::IdleFoldArms,
    Clip::Walk,
    Clip::Jog,
    Clip::Sprint,
];

/// The clip a rider sits its horse with (see [`rider`](super::rider)),
/// the only one it plays: a clip at no weight still sets the bones only it
/// animates.
pub const SEATED: Clip = Clip::Drive;

/// The clips a model plays: a rider's, or those of a person on foot.
pub fn played(riding: bool) -> &'static [Clip] {
    if riding { &[SEATED] } else { &PLAYED }
}

/// Marks a person whose lines are on screen: standing, they talk with
/// their hands (see [`idle_for`]). The game puts it on the speaker.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Talking;

/// A character model's skeleton and clip state, found when its scene
/// spawns; on the model's root.
#[derive(Component, Debug, Clone)]
pub struct Rig {
    /// The `Root` node: it has the animation player, and every bone under
    /// it is a target of the clips.
    pub player: Entity,
    /// The pelvis, its rest place under `Root`, and this model's leg
    /// length (hip to ankle), meters.
    pub pelvis: Option<(Entity, Vec3, f32)>,
    /// The neck and the head, each with its parent's rest orientation in
    /// the body frame.
    pub look: [Option<(Entity, Quat)>; 2],
    /// A model in a long skirt: its left and right upper arms, each with
    /// its parent's rest orientation in the body frame, turned out by
    /// [`SKIRT_ARMS_OUT`] on top of the clips.
    pub arms: [Option<(Entity, Quat)>; 2],
    /// Every mesh of the model: it is in view while any of them is drawn.
    pub probes: Vec<Entity>,
    /// Whether the bones keep their last pose (see [`frozen`]).
    pub frozen: bool,
    /// Each clip's weight now, in [`Clip::ALL`] order.
    pub weights: [f32; Clip::COUNT],
    /// Whether its clips are playing.
    pub started: bool,
}

impl Rig {
    /// A model's rig, its clips played by `player`.
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

/// The name path of a bone from `Root`, as the clips' file names its
/// targets: the [`AnimationTargetId`] the clips animate it by.
pub fn target_id(path: &[Name]) -> AnimationTargetId {
    AnimationTargetId::from_names(path.iter())
}

/// Makes `root` (the model's `Root` node) the clips' player and every node
/// under it, by its name path from `Root`, one of their targets.
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

/// The weight each clip should have for a rider: only [`SEATED`].
pub fn seated_targets() -> [f32; Clip::COUNT] {
    let mut out = [0.0; Clip::COUNT];
    out[SEATED.index()] = 1.0;
    out
}

/// The weight each clip should have, in [`Clip::ALL`] order: a person of
/// `kind` moving at `speed` (m/s), maybe `talking`.
pub fn targets(kind: FigureKind, talking: bool, speed: f32) -> [f32; Clip::COUNT] {
    let mut out = [0.0; Clip::COUNT];
    let [idle, strides @ ..] = locomotion(speed);
    out[idle_for(kind, talking).index()] = idle;
    for (clip, weight) in STRIDES.into_iter().zip(strides) {
        out[clip.index()] = weight;
    }
    out
}

/// `weights` eased toward `targets` over `dt` seconds.
pub fn ease(weights: &mut [f32; Clip::COUNT], targets: &[f32; Clip::COUNT], dt: f32) {
    let k = 1.0 - (-BLEND_RATE * dt).exp();
    for (w, t) in weights.iter_mut().zip(targets) {
        *w += (t - *w) * k;
    }
}

/// Where a model's pelvis goes for a clip's `clip` place (made on the
/// `reference` skeleton): its moves from the reference's rest, scaled by
/// this model's `leg` over the reference's, about this model's `rest`.
pub fn fit_pelvis(clip: Vec3, reference: Reference, rest: Vec3, leg: f32) -> Vec3 {
    rest + (clip - reference.pelvis) * (leg / reference.leg)
}

/// How long a model's steps are against the clips' own (their
/// reference skeleton's): its legs' length over the reference's, 1 when
/// either is unknown.
pub fn model_stride(pelvis: Option<(Entity, Vec3, f32)>, reference: Option<Reference>) -> f32 {
    match (pelvis, reference) {
        (Some((_, _, leg)), Some(r)) if leg > 0.0 && r.leg > 0.0 => leg / r.leg,
        _ => 1.0,
    }
}

/// The local rotation `local` of a neck or head bone, turned by `turn`
/// (radians, about the body's up) in the body frame; `frame` is its
/// parent's rest orientation in the body frame.
pub fn glance_turn(local: Quat, frame: Quat, turn: f32) -> Quat {
    (frame.inverse() * Quat::from_rotation_y(turn) * frame * local).normalize()
}

/// How fast `clip` plays for a person moving at `speed` m/s: paced by
/// the speed's size ([`playback_rate`]), and backward when backing up if
/// the clip travels. An idle always plays forward.
pub fn stride_rate(clip: Clip, speed: f32) -> f32 {
    let rate = playback_rate(clip, speed.abs());
    if speed < 0.0 && clip.ground_speed().is_some() {
        -rate
    } else {
        rate
    }
}

/// The ground speed a person steps at, negative backing up: none while
/// standing, but the small pace of a turn on the spot
/// ([`stepping_speed`]).
fn moving(pose: &VisualPose) -> f32 {
    let speed = if pose.animation == AnimationState::Idle {
        0.0
    } else {
        pose.speed
    };
    stepping_speed(speed, pose.turn, TURN_STEP_RADIUS)
}

/// What [`drive_rigs`] reads of a model.
type ModelRig = (
    &'static ChildOf,
    &'static mut Rig,
    &'static GlobalTransform,
    &'static mut Transform,
    Option<&'static Riding>,
);

/// Who a model is and what it plays: on foot, its figure's kind and seed,
/// and its weights by speed and speech; in the saddle, its rider's, seated.
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

/// Weighs and paces each model's clips (see the module notes), and freezes
/// a model out of view or far off, except the protagonist's.
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
                // The clips bob the pelvis: undo the carrier's bob.
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
        // Visibility is only known with a camera drawing.
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
        // The clips step as the protagonist's legs do: a person with
        // shorter legs takes shorter steps, so plays them faster to keep
        // the feet still on the ground.
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

/// Fits each playing model's clips to it: its pelvis by its own legs, and
/// its head turned by its figure's glance.
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

/// The rest orientation in the body frame of the space `entity` hangs in:
/// its ancestors' rotations, up to and including the model's `root`
/// (turned to the body's front).
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
        // Between a walk and a jog, both; standing still, Farach's idle
        // fades as he sets off.
        let between = targets(FigureKind::Farach, false, (JOG_BAND.0 + JOG_BAND.1) / 2.0);
        assert!(between[Clip::Walk.index()] > 0.4 && between[Clip::Jog.index()] > 0.4);
        let setting_off = targets(FigureKind::Farach, false, 0.3);
        assert!(setting_off[Clip::IdleFoldArms.index()] > 0.3);
        assert!(setting_off[Clip::Walk.index()] > 0.3);
        // A blend adds up, and weighs only clips the driver plays.
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

    /// The character plugin without assets, with stand-in clips, and a
    /// model under each of `people`'s figures by hand: its rig's player,
    /// and two meshes (a hat, then the body). Returns each person's kind,
    /// rig and meshes.
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
        // The camera's place reaches the driver a frame late.
        app.update();
        app.update();
        for &(_, rig, _) in &rigs {
            assert!(!frozen_now(&app, rig));
        }
        // Far off: Don Hidalgo freezes; the protagonist carries on.
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
        // Near again: it plays again.
        far(&mut app, 4.0);
        for &(_, rig, _) in &rigs {
            assert!(!frozen_now(&app, rig));
        }
        // Its hat culled, its body drawn: still in view.
        for &(_, _, [hat, _]) in &rigs {
            app.world_mut()
                .entity_mut(hat)
                .insert(ViewVisibility::HIDDEN);
        }
        app.update();
        for &(_, rig, _) in &rigs {
            assert!(!frozen_now(&app, rig));
        }
        // Nothing of it drawn: out of view, frozen, but the protagonist.
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

    // --- with the shipped models and clips ----------------------------------

    /// The character plugin with the asset server and the animation plugin,
    /// and `people` a meter apart along +X, posed `animation` at `speed`;
    /// returns once every model plays its clips, each rig with its kind.
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

    /// Poses everyone in `app` `animation` at `speed` from the next frame.
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

    /// The entity named `name` under the model `rig`.
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
        // Every bone the clips animate is a bone of the model.
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
        // And they move them: the thigh swings through the stride.
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

    /// Each 50 ms frame's balls of the feet, world places, for `frames`.
    fn balls(app: &mut App, rig: Entity, frames: usize) -> Vec<[Vec3; 2]> {
        let ends = ["ball_l", "ball_r"].map(|n| bone(app.world(), rig, n));
        (0..frames)
            .map(|_| {
                app.update();
                ends.map(|e| at(app.world(), e))
            })
            .collect()
    }

    /// How fast the planted foot (the lower ball, on the ground) slides
    /// back on average, m/s.
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
        // In place, the planted foot moves back under the body as fast as
        // the body goes forward: on the ground it keeps still. Backing, it
        // moves forward as fast as the body goes back: no moonwalk.
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
        // Each model's balls of the feet rest about 2.4 cm up; a pelvis
        // set where the protagonist's is would lift Don Hidalgo's 2.5 cm.
        for &(kind, low) in &lowest {
            assert!((low - 0.024).abs() < 0.012, "{kind:?}: {low}");
        }
        let (_, farach) = *rigs.iter().find(|(k, _)| *k == FigureKind::Farach).unwrap();
        let rig = app.world().get::<Rig>(farach).unwrap();
        assert!(rig.weights[Clip::IdleFoldArms.index()] > 0.99);
        // His hands are crossed over his chest: each on the other side.
        let world = app.world();
        let body = world.get::<ChildOf>(farach).unwrap().parent();
        let to_body = world
            .get::<GlobalTransform>(body)
            .unwrap()
            .affine()
            .inverse();
        let side = |name: &str| to_body.transform_point3(at(world, bone(world, farach, name)));
        let (left, right) = (side("hand_l"), side("hand_r"));
        // The body's left is -Z.
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
        // Standing, not turning: idle, the walk at no weight.
        for _ in 0..10 {
            app.update();
        }
        assert!(state(&mut app).0 < 1e-3);
        // Standing, turning (logic still says Idle at no speed).
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
        // Released: back to the idle.
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
        // The game marks the speaker on its character entity.
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
        // The head's turn about the body's up, from the chest's.
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
        // Fifteen seconds: a glance aside and back.
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
        // Looking ahead it is where it was each time: nothing builds up.
        let (lo, hi) = ahead
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(hi - lo < 0.1, "looking ahead from {lo} to {hi}");
        // A glance turns it, never past the glance's reach (0.55 rad).
        assert!(aside > 0.15 && aside < 0.65, "{aside}");
    }

    #[test]
    fn walking_knees_bend_smoothly_never_lock_or_twist_round_the_loop() {
        use crate::character::VisualPose;
        // The protagonist at his 1.6 m/s, a peón and a woman at a
        // townsperson's 1.4, at 60 frames a second for a whole loop and
        // more, through its wrap.
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
                    // Bent back, never forward: the shin turns from the
                    // thigh away from the body's front.
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
        // The countrywoman in a headscarf and the young townswoman in a bun.
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
            // Into the clip (the talk eases in), then two seconds of it.
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
        // Every outfit once, and two people dressed alike.
        let mut looks: Vec<u8> = (0..5).collect();
        looks.push(3);
        let crowd: Vec<FigureKind> = looks.iter().map(|&l| FigureKind::townsfolk(l)).collect();
        let (mut app, rigs) = playing(&crowd, AnimationState::Idle, 0.0);
        app.update();
        let world = app.world_mut();
        // Each person's look and their clothes: (material name, material).
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
        // The same look, the same materials (the very same copies).
        let alike: Vec<&Vec<_>> = worn
            .iter()
            .filter(|(l, _)| *l == 3)
            .map(|(_, c)| c)
            .collect();
        assert_eq!(alike.len(), 2);
        assert_eq!(alike[0], alike[1]);
        // Each tinted copy is worn, and shared: fewer copies than tinted
        // pieces of clothing in the crowd.
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

    /// P4: the CPU cost of a full plaza, headless: the 31 townsfolk General
    /// Pico has, each in their look's model, and the protagonist, all within
    /// the freeze range and in view (nothing frozen), two in three walking.
    /// Prints, per frame, the whole update with them against the same app
    /// without them (the clips' weighing, Bevy's animation, the pelvis fit
    /// and glance, the carriers' posing and the transforms), the skinning's
    /// preparation (each skinned mesh's joint matrices, as the renderer's
    /// extraction computes them), and the triangles drawn. Run it in
    /// release: `cargo test -p pampa-render --release --lib plaza -- --ignored --nocapture`.
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
        // Two in three walk, the rest stand; spread over the plaza.
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
        // The figures' rigid parts, hidden under the models, kept for
        // `models off`: what they cost (their transforms, worked out every
        // frame under the moving body).
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
        // The skinning's preparation: every joint's matrix, every frame.
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
        // Triangles drawn: every mesh under every model.
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
        // The same app without anyone.
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
            // Into the clip, then a second and a half of it.
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
