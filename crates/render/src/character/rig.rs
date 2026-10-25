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
//!   ([`playback_rate`]) so the feet keep still on the ground. The weights
//!   ease toward their targets, and every clip starts at the person's own
//!   seeded point ([`phase_offset`]). The model also undoes the carrier's
//!   stride bob: the clips bob the pelvis themselves.
//! - [`fit_rigs`], after them, fits the clip to the model: the clips were
//!   made on the protagonist, so the pelvis's moves are scaled by this
//!   model's legs and set about its own rest ([`fit_pelvis`]); then the
//!   neck and head turn by the figure's glance, half each, on top of the
//!   clip ([`glance_turn`]).
//!
//! A model out of view (none of its meshes drawn) or farther than
//! [`FREEZE_FAR`](crate::fauna::FREEZE_FAR) from the camera loses its graph,
//! as the livestock do: its bones keep their last pose. The protagonist,
//! only ever the player, never freezes.

use bevy::animation::{AnimatedBy, AnimationTargetId};
use bevy::prelude::*;
use pampa_core::AnimationState;

use super::clips::{
    CharacterClips, Clip, Reference, STRIDES, idle_for, locomotion, phase_offset, playback_rate,
};
use super::figure::{FigureBody, FigureKind};
use super::model::model_transform;
use super::visual::VisualPose;
use crate::fauna::livestock::{any_in_view, frozen, view_eye};

/// How fast the clips' weights ease toward their targets, per second.
pub const BLEND_RATE: f32 = 6.0;

/// The clips a model may play: the idles and the strides. Each starts once
/// the clips have loaded and keeps running; only its weight changes.
pub const PLAYED: [Clip; 6] = [
    Clip::Idle,
    Clip::IdleTalk,
    Clip::IdleFoldArms,
    Clip::Walk,
    Clip::Jog,
    Clip::Sprint,
];

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

/// The local rotation `local` of a neck or head bone, turned by `turn`
/// (radians, about the body's up) in the body frame; `frame` is its
/// parent's rest orientation in the body frame.
pub fn glance_turn(local: Quat, frame: Quat, turn: f32) -> Quat {
    (frame.inverse() * Quat::from_rotation_y(turn) * frame * local).normalize()
}

/// The ground speed a person moves at: none while standing.
fn moving(pose: &VisualPose) -> f32 {
    if pose.animation == AnimationState::Idle {
        0.0
    } else {
        pose.speed.max(0.0)
    }
}

/// What [`drive_rigs`] reads of a model.
type ModelRig = (
    &'static ChildOf,
    &'static mut Rig,
    &'static GlobalTransform,
    &'static mut Transform,
);

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
    for (child_of, mut rig, at, mut transform) in &mut rigs {
        let Ok((body, carried, person)) = bodies.get(child_of.parent()) else {
            continue;
        };
        // The clips bob the pelvis: undo the carrier's bob.
        transform.set_if_neq(model_transform().with_translation(Vec3::Y * -carried.translation.y));
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        // Visibility is only known with a camera drawing.
        let in_view =
            eye.is_none() || any_in_view(&rig.probes, |p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = body.kind != FigureKind::Protagonist && frozen(rig.frozen, distance, in_view);
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
        let (speed, talking) = people
            .get(person.parent())
            .map_or((0.0, false), |(pose, talking)| (moving(pose), talking));
        let goal = targets(body.kind, talking, speed);
        if !rig.started {
            let Some(loaded) = loaded.as_deref() else {
                continue;
            };
            let durations: Option<Vec<f32>> = PLAYED
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
            let offset = phase_offset(body.seed);
            for (clip, duration) in PLAYED.into_iter().zip(durations) {
                player
                    .play(clips.node(clip))
                    .repeat()
                    .seek_to(offset * duration);
            }
            rig.weights = goal;
            rig.started = true;
        }
        ease(&mut rig.weights, &goal, dt);
        for clip in PLAYED {
            if let Some(active) = player.animation_mut(clips.node(clip)) {
                active
                    .set_weight(rig.weights[clip.index()])
                    .set_speed(playback_rate(clip, speed));
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
            targets(FigureKind::Protagonist, false, 6.0 / 3.6),
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
        // Only clips the driver plays are ever weighed.
        for speed in [0.0, 0.3, 1.0, 3.0, 5.0, 7.0, 9.0] {
            for kind in FigureKind::all() {
                for talking in [false, true] {
                    let w = targets(kind, talking, speed);
                    for clip in Clip::ALL {
                        assert!(PLAYED.contains(&clip) || w[clip.index()] == 0.0, "{clip:?}");
                    }
                    assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                }
            }
        }
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
    fn freezing_goes_by_the_camera_drawn_last_not_the_first_one_found() {
        let (mut app, rigs) = stand_ins(&[FigureKind::Hidalgo]);
        let rig = rigs[0].1;
        // An overlay camera far off, spawned first, drawn first; the game's
        // camera near, drawn last.
        app.world_mut().spawn((
            Camera3d::default(),
            Camera {
                order: -1,
                ..default()
            },
            Transform::from_xyz(0.0, 1.6, 900.0),
        ));
        app.world_mut().spawn((
            Camera3d::default(),
            Camera {
                order: 1,
                ..default()
            },
            Transform::from_xyz(0.0, 1.6, 4.0),
        ));
        for _ in 0..3 {
            app.update();
        }
        assert!(!frozen_now(&app, rig));
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
    fn a_planted_foot_slides_back_at_the_ground_speed_walking_and_jogging() {
        // In place, the planted foot moves back under the body as fast as
        // the body goes forward: on the ground it keeps still.
        for (animation, speed) in [
            (AnimationState::Walk, 6.0 / 3.6),
            (AnimationState::Gallop, 5.0),
        ] {
            let (mut app, rigs) = playing(&[FigureKind::Protagonist], animation, speed);
            balls(&mut app, rigs[0].1, 30);
            let frames = balls(&mut app, rigs[0].1, 60);
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
}
