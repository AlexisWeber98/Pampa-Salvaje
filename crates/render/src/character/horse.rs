//! Saddled criollo horses: Quaternius' animated horse (the corral horses'
//! model, see [`livestock`](crate::fauna::livestock)) wearing a 1919 recado
//! and a rawhide bridle (see [`tack`](super::tack)), tied up or ridden.
//!
//! A saddled horse is a body entity ([`HorseBody`], turned to the heading,
//! +X its front, its right +Z, hooves on its origin) holding the model
//! ([`HorseModel`]). The model waits for its scene ([`HorsePending`]): until
//! it has loaded, nothing is drawn (the scenes load with the game, long
//! before anyone gets on a horse). Once it spawns, [`dress_horse`] gives
//! its meshes the coat and the props' shared range, as the corral horses'
//! do, and finds the bones the gaits drive ([`gait`](super::gait)). The
//! same scene event puts the tack on ([`tack`](super::tack): the skinned
//! recado and bridle, the stirrups and the straps), and once the tack is on
//! a rider with a model sits on its `Torso` bone ([`rider`](super::rider)),
//! so he rises, pitches and turns with the horse's own clip.
//!
//! [`drive_horses`] plays the clips: the idle, then the walk paced to the
//! ground speed up to about 4.5 m/s (there is no trot clip), then the
//! gallop ([`gait_weights`], [`gait_rate`]), each at the ground speed over
//! the speed its hooves show, so they keep still on the ground. A tied
//! horse changes idle now and then, as the corral horses do; a ridden one
//! stands still. A horse out of view or far off keeps its last pose, but
//! never the player's.
//!
//! Coats are painted into the model's vertex colours from where each vertex
//! sits on the standing horse ([`Coat::color_at`]), so patches, dapples and
//! the dun's dorsal stripe run across the body.

use bevy::animation::graph::AnimationGraphHandle;
use bevy::ecs::system::SystemParam;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use pampa_core::{AnimationState, Heading, HorseGait};

use super::figure::{FigureKind, Rgb, body_rotation};
use super::gait::{
    Bone as GaitBone, GALLOP_MEANS, HorseBones, PARTIAL, Property, UNANIMATED, fit_horse,
};
use super::visual::VisualPose;
use crate::fauna::Animal;
use crate::fauna::livestock::{
    self, Clip, CoatCache, HORSE_IDLE, HORSE_WALK_SPEED, LivestockModels, ModelPart, Species,
    any_in_view, dress_meshes, frozen, mix, start_offset, view_eye,
};
use crate::gltf::{self, ModelState};

/// A horse's coat (pelaje).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coat {
    /// Solid dark brown, black mane and tail, no white.
    Zaino,
    /// Grey with lighter dapples.
    Tordillo,
    /// Bay with large white patches.
    Overo,
    /// Chestnut, the mane and tail a little lighter.
    Alazan,
    /// Dun: sandy yellow with black mane, tail and lower legs and a dark
    /// stripe down the back.
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

    /// Don Hidalgo's horse, the one the player rides.
    pub const HIDALGO: Self = Self::Bayo;

    /// A coat picked deterministically from `seed`.
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

    pub(crate) fn mane(self) -> Rgb {
        match self {
            Self::Zaino | Self::Bayo => [34, 28, 24],
            Self::Tordillo => [206, 202, 194],
            Self::Overo => [118, 72, 44],
            Self::Alazan => [176, 104, 58],
        }
    }

    /// The lower legs, darker on most coats.
    pub(crate) fn points(self) -> Rgb {
        match self {
            Self::Zaino => [44, 32, 26],
            Self::Tordillo => [92, 92, 90],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [132, 70, 38],
            Self::Bayo => [40, 32, 26],
        }
    }

    /// The head, a little lighter on the grey.
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

    /// The colour of a coat vertex at `p` on the standing horse, its
    /// surface facing `normal`.
    pub fn color_at(self, p: Vec3, normal: Vec3) -> Rgb {
        const WHITE: Rgb = [232, 228, 218];
        if p.y < POINTS_HEIGHT {
            // Overo stockings on the near fore and the off hind.
            let stocking = (p.x > 0.0 && p.z < 0.0) || (p.x < 0.0 && p.z > 0.0);
            if self == Self::Overo && stocking {
                return WHITE;
            }
            return self.points();
        }
        match self {
            Self::Overo if overo_patch(p) => WHITE,
            Self::Tordillo if dapple(p) => [178, 176, 170],
            // The dun's dorsal stripe, from the withers to the tail.
            Self::Bayo if normal.y > 0.6 && p.z.abs() < 0.05 && (-0.8..0.45).contains(&p.x) => {
                [104, 76, 46]
            }
            _ => self.body(),
        }
    }
}

/// Below this height (m) the legs show the coat's points.
const POINTS_HEIGHT: f32 = 0.5;

/// The overo's white: big blobs on the belly, both flanks, the off side of
/// the croup, both shoulders and both sides of the neck, so some white shows
/// from every side.
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

/// The tordillo's dapples: a cheap 3D interference pattern.
fn dapple(p: Vec3) -> bool {
    let a = (p.x * 23.0 + (p.z * 17.0).sin()).sin();
    let b = (p.y * 21.0 + (p.x * 13.0).cos()).sin();
    let c = (p.z * 19.0 + p.y * 7.0).sin();
    a * b * c > 0.12
}

/// The ground speeds (m/s) across which the idle fades into the walk.
pub const WALK_BAND: (f32, f32) = (0.05, 0.6);

/// The ground speeds (m/s) at which a walking horse breaks into the gallop,
/// and a galloping one comes back to the walk. Logic walks a horse up to
/// about 3 m/s and "gallops" it from there (trotting up to
/// [`HorseGait::TROT_MAX`]); with no trot clip the walk is stretched to
/// about 4.5 m/s. The gait changes outright (and the weights fade fast,
/// [`HORSE_BLEND_RATE`]), never holding the two half and half: their legs
/// do not match.
pub const GALLOP_FROM: (f32, f32) = (4.2, 4.8);

/// Whether a horse moving at `speed` gallops, if it `was` galloping.
pub fn gallops(was: bool, speed: f32) -> bool {
    speed > if was { GALLOP_FROM.0 } else { GALLOP_FROM.1 }
}

/// How much slower or faster than recorded the walk may play: up to 3.8
/// times at 4.8 m/s.
pub const RATE: (f32, f32) = (0.3, 3.9);

/// How much slower or faster than recorded the (stretched) gallop may play:
/// faster reads frantic, so above about 11.6 m/s its hooves slide a little.
pub const GALLOP_RATE: (f32, f32) = (0.5, 2.0);

/// The ground speed the gallop's hooves show with its stride stretched
/// ([`STRIDE`](super::gait::STRIDE)), m/s, measured as the clip's own
/// ([`HORSE_GALLOP_SPEED`](crate::fauna::livestock::HORSE_GALLOP_SPEED)).
pub const GALLOP_SPEED: f32 = 7.3;

/// The weight of each clip, in [`Clip::ALL`] order, for a horse standing
/// with `idle` or moving at `speed` m/s with `stride` (the walk or the
/// gallop): the idle fades into the stride. They add up to one.
pub fn gait_weights(idle: Clip, stride: Clip, speed: f32) -> [f32; 6] {
    let moving = ((speed - WALK_BAND.0) / (WALK_BAND.1 - WALK_BAND.0)).clamp(0.0, 1.0);
    let mut out = [0.0; 6];
    out[idle.index()] = 1.0 - moving;
    out[stride.index()] += moving;
    out
}

/// How fast `clip` plays for a horse moving at `speed` m/s: the ground
/// speed over the speed its hooves show, within [`RATE`]; an idle plays as
/// recorded.
pub fn gait_rate(clip: Clip, speed: f32) -> f32 {
    match clip {
        Clip::Walk => (speed / HORSE_WALK_SPEED).clamp(RATE.0, RATE.1),
        Clip::Gallop => (speed / GALLOP_SPEED).clamp(GALLOP_RATE.0, GALLOP_RATE.1),
        _ => 1.0,
    }
}

/// The idles a tied horse changes between. Tied short to the post, it
/// cannot crop the ground (and its reins, over the pommel, would cut
/// through its neck if it did).
pub const TIED_IDLES: [Clip; 3] = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];

/// A saddled horse's body, child of its character entity: turned to the
/// heading, holding the model.
#[derive(Component, Debug, Clone)]
pub struct HorseBody {
    pub coat: Coat,
    /// Who rides it, if anyone.
    pub rider: Option<FigureKind>,
    /// Its gait: logic's animation and the speed decide it.
    pub gait: HorseGait,
    /// How much of the gallop it plays now (`0..1`): a rider leans into it.
    pub gallop: f32,
    /// Its ground speed now, m/s (none standing).
    pub speed: f32,
    /// Its own seed: where its clips start and which idle a tied horse
    /// plays.
    pub seed: u32,
}

impl HorseBody {
    /// The idle it stands with at `clock` seconds: a ridden horse stands
    /// still; a tied one changes idle now and then, as the corral horses
    /// do ([`TIED_IDLES`]).
    pub fn idle(&self, clock: f32) -> Clip {
        match self.rider {
            Some(_) => Clip::Idle,
            None => {
                let turn = ((clock.max(0.0) + (self.seed % 97) as f32) / HORSE_IDLE) as u32;
                TIED_IDLES[mix(self.seed, turn) as usize % TIED_IDLES.len()]
            }
        }
    }

    /// Whether its clips keep playing out of view or far off: the player's
    /// horse always does.
    pub fn never_freezes(&self) -> bool {
        self.rider == Some(FigureKind::Protagonist)
    }
}

/// The horse model's root, child of its [`HorseBody`].
#[derive(Component, Debug, Clone, Copy)]
pub struct HorseModel;

/// A horse model waiting for its scene to load.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorsePending;

/// A spawned horse model's animation state, on its root.
#[derive(Component, Debug, Clone)]
pub struct HorseRig {
    /// The entity with its `AnimationPlayer`.
    pub player: Entity,
    /// Its drawn meshes: it is in view while any of them is.
    pub probes: Vec<Entity>,
    /// Each clip's weight now, in [`Clip::ALL`] order.
    pub weights: [f32; 6],
    /// Whether its clips are playing.
    pub started: bool,
    /// Whether its bones keep their last pose.
    pub frozen: bool,
    /// Each clip's length, seconds, once it has loaded.
    pub durations: [f32; 6],
    /// The stride clip it is mostly playing (the walk or the gallop): the
    /// other is put in step with it as it takes over.
    pub lead: Clip,
}

impl HorseRig {
    /// A model's rig, its clips played by `player`, seen through `probes`.
    pub fn new(player: Entity, probes: Vec<Entity>) -> Self {
        Self {
            player,
            probes,
            weights: [0.0; 6],
            started: false,
            frozen: false,
            durations: [0.0; 6],
            lead: Clip::Walk,
        }
    }
}

/// Where the walk's loop is in step with the gallop's: each with the left
/// hind striking (the walk's at 0.25 of its loop, the gallop's at
/// [`GATHER`](super::gait::GATHER)).
pub const WALK_FROM_GALLOP: f32 = 0.25 - super::gait::GATHER;

/// The share of its loop (`0..1`) a stride clip taking the lead from
/// `from` at `phase` of its loop starts at.
pub fn in_step(to: Clip, phase: f32) -> f32 {
    match to {
        Clip::Walk => (phase + WALK_FROM_GALLOP).rem_euclid(1.0),
        _ => (phase - WALK_FROM_GALLOP).rem_euclid(1.0),
    }
}

/// How fast the clips' weights ease toward their targets, per second: a
/// quick fade, so two clips are seldom seen half and half.
pub const HORSE_BLEND_RATE: f32 = 12.0;

/// The model's transform under its body: the horse's export turned to face
/// +X and scaled to life (the corral horses' scale).
pub fn model_transform() -> Transform {
    livestock::model_transform(Species::Horse, Heading::default())
}

/// Spawns a saddled horse in `coat` (ridden by `rider`) under `parent`:
/// its body, and its model waiting for the scene. Returns the body.
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

/// Spawns each waiting horse model's scene once it has loaded. Without the
/// horse's file nothing is drawn.
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

/// `entity`'s rest placement in the body frame of the model `root` (its
/// own transform and its ancestors', up to and including the root's).
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

/// A transform from an affine placement.
pub(super) fn transform_of(affine: Affine3A) -> Transform {
    Transform::from_matrix(Mat4::from(affine))
}

/// What a horse model is dressed from: its nodes' names, rests and
/// meshes.
#[derive(SystemParam)]
pub(super) struct HorseScene<'w, 's> {
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static Transform>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    parts: Query<'w, 's, ModelPart>,
}

/// The stores a horse is dressed from.
#[derive(SystemParam)]
pub(super) struct HorseStores<'w> {
    cache: Option<ResMut<'w, CoatCache>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    models: Option<Res<'w, LivestockModels>>,
}

/// Dresses a horse model the moment its scene spawns: its coat and range,
/// its clips' graph and its bones for [`fit_horses`]. (The tack and the
/// rider are put on by their own modules.)
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

/// The bones [`fit_horses`] moves under the model `root`, and their rests.
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
    })
}

/// Fits each playing horse's pose after its clips (see
/// [`gait`](super::gait)): the blend completed, the gallop stretched and
/// flexed, the hooves kept on the legs.
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
        fit_horse(bones, &mut transforms, &rig.weights, phase);
    }
}

/// Turns each saddled horse to its heading and plays its clips (see the
/// module notes): weighed and paced by speed, started at its seeded point,
/// frozen far off or out of view unless it is the player's.
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
        let speed = match poses.get(parent.parent()) {
            Ok(pose) if pose.animation != AnimationState::Idle => pose.speed.abs(),
            _ => 0.0,
        };
        let stride = if gallops(rig.lead == Clip::Gallop, speed) {
            Clip::Gallop
        } else {
            Clip::Walk
        };
        let goal = gait_weights(body.idle(clock), stride, speed);
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
        // The stride clip taking over starts in step with the one leaving.
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
        let k = 1.0 - (-HORSE_BLEND_RATE * dt).exp();
        for (w, t) in rig.weights.iter_mut().zip(goal) {
            *w += (t - *w) * k;
        }
        body.gallop = rig.weights[Clip::Gallop.index()];
        body.speed = speed;
        for clip in Clip::ALL {
            if let Some(active) = player.animation_mut(species.nodes[clip.index()]) {
                active
                    .set_weight(rig.weights[clip.index()])
                    .set_speed(gait_rate(clip, speed));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A stand-in horse's surface: points on its barrel, croup and
    /// shoulders (a tube along +X round 1.14 m up), its neck, and its four
    /// legs, each with its outward normal.
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
        let only = |w: [f32; 6], clip: Clip| {
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
        for k in 0..1300 {
            let speed = k as f32 * 0.01;
            for stride in [walk, gallop] {
                let w = gait_weights(Clip::Idle, stride, speed);
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{k}");
                // Never the walk and the gallop at once.
                assert!(w[walk.index()] * w[gallop.index()] == 0.0);
            }
        }
        // Logic's walk walks, the trot's fast half and the gallop gallop,
        // with a margin either way so a steady pace never flickers.
        assert!(!gallops(false, 3.0) && !gallops(false, 4.5) && gallops(false, 4.9));
        assert!(gallops(true, 4.5) && !gallops(true, 4.1));
        assert!(gallops(false, 12.2) && gallops(true, 12.2));
    }

    #[test]
    fn the_walk_and_the_gallop_play_at_the_ground_speed_over_their_own() {
        assert!((gait_rate(Clip::Walk, HORSE_WALK_SPEED) - 1.0).abs() < 1e-6);
        assert!((gait_rate(Clip::Gallop, GALLOP_SPEED) - 1.0).abs() < 1e-6);
        // Logic's walk and the walk stretched to the top of the trot band.
        assert!((gait_rate(Clip::Walk, 3.0) - 3.0 / HORSE_WALK_SPEED).abs() < 1e-5);
        assert!((gait_rate(Clip::Walk, 4.8) - 4.8 / HORSE_WALK_SPEED).abs() < 1e-5);
        // The long gallop paced to the ground up to its cap, then capped:
        // never frantic.
        assert!((gait_rate(Clip::Gallop, 8.0) - 8.0 / GALLOP_SPEED).abs() < 1e-5);
        assert!((gait_rate(Clip::Gallop, 12.2) - 12.2 / GALLOP_SPEED).abs() < 1e-5);
        assert_eq!(gait_rate(Clip::Gallop, 19.4), GALLOP_RATE.1);
        assert_eq!(gait_rate(Clip::Gallop, 0.0), GALLOP_RATE.0);
        assert_eq!(gait_rate(Clip::Walk, 0.0), RATE.0, "clamped");
        assert_eq!(gait_rate(Clip::Idle, 9.0), 1.0, "an idle as recorded");
        assert_eq!(gait_rate(Clip::Eating, 9.0), 1.0);
    }

    #[test]
    fn a_ridden_horse_stands_still_and_a_tied_one_changes_idle() {
        let mut body = HorseBody {
            coat: Coat::Zaino,
            rider: Some(FigureKind::Protagonist),
            gait: HorseGait::Idle,
            gallop: 0.0,
            speed: 0.0,
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

    // --- with the shipped horse, rider and clips -----------------------------

    use crate::character::cpu;
    use crate::character::{CharacterVisual, CharacterVisualPlugin};
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;

    /// The fauna and character plugins with the shipped models, and `visual`
    /// at the origin facing +X, moving at `speed`; returns once the horse
    /// plays its clips.
    pub(crate) fn saddled(visual: CharacterVisual, speed: f32) -> (App, Entity) {
        saddled_every(visual, speed, 50)
    }

    /// [`saddled`], with frames `ms` milliseconds long.
    pub(crate) fn saddled_every(visual: CharacterVisual, speed: f32, ms: u64) -> (App, Entity) {
        use crate::gltf::tests::{root, unfinished_app};
        let mut app = unfinished_app(&root());
        app.init_asset::<SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(ms),
            ))
            .add_plugins((crate::fauna::FaunaPlugin, CharacterVisualPlugin));
        // Frames since a scene asset last changed: a scene spawned before
        // its asset settles is spawned again, and the tests hold its bones.
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
        // Until the horse plays, the same model for a while, its assets
        // settled.
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
            if held.1 >= 10 && world.resource::<Settled>().0 >= 10 {
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
        // Not a rigid block: the croup and the withers turn apart and back.
        let bend = flex
            .iter()
            .map(|q| q.angle_between(flex[0]))
            .fold(0.0, f32::max);
        assert!(bend > 0.1, "the horse's back bends over a stride: {bend}");
    }

    /// Writes CPU-skinned frames to `PAMPA_PREVIEW` (a folder) for the
    /// offline preview renderer: a ridden horse standing (and settled),
    /// walking and through a gallop's loop, braking from the gallop to a
    /// stand and from a walk to a stand, and a tied horse. Run by hand:
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
        // Steady paces: 20 ms frames, every other one written.
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
        // Braking as hard as logic does (8 m/s²): every 0.1 s.
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
        // Standing: at once, and settled after a few seconds.
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

    /// How fast each frame's planted hooves (toes within 1.5 cm of the
    /// lowest) slide back in the horse's body frame, m/s, over `frames`
    /// frames `dt` seconds long: their median.
    fn hoof_slide(app: &mut App, me: Entity, frames: usize, dt: f32) -> f32 {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let mut tracks = Vec::new();
        for _ in 0..frames {
            app.update();
            let world = app.world();
            // Found afresh: a scene spawned again has new bones.
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
        // Each hoof's own ground: the lowest it goes.
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

    #[test]
    fn the_long_gallop_keeps_its_hooves_still_up_to_its_fastest_unhurried_pace() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        for speed in [8.0, 12.2, 19.4] {
            let (mut app, me) = saddled_every(rider, speed, 10);
            for _ in 0..100 {
                app.update();
            }
            let back = hoof_slide(&mut app, me, 300, 0.01);
            let rate = gait_rate(Clip::Gallop, speed);
            let (natural, slide) = (back / rate, speed - back);
            println!(
                "{speed} m/s: hooves back at {back:.2} m/s (rate {rate:.2}, natural {natural:.2}), slide {slide:.2} m/s"
            );
            // The stretched loop shows the speed it is paced by.
            assert!((natural / GALLOP_SPEED - 1.0).abs() < 0.06, "{natural}");
            if speed <= GALLOP_SPEED * GALLOP_RATE.1 {
                assert!(slide.abs() < 0.5, "{speed} m/s: slides {slide} m/s");
            } else {
                // Past the cap (the dev turbo) it slides, no faster.
                assert!(slide > 0.0 && slide < speed - GALLOP_SPEED * GALLOP_RATE.1 + 0.5);
            }
        }
    }

    /// Per frame of `app`'s horse under `me`: the lowest toe (body frame),
    /// the furthest any hoof stands from its leg's end, and each leg
    /// bone's turn away from `rest` (by [`LEG_BONES`]).
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

    /// Plays `app`'s horse at each speed of `speeds` for a frame, logic's
    /// animation following the speed as the rider's does.
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
        // The limits: as far as each bone turns at steady paces.
        let mut limit = vec![0.0f32; LEG_BONES.len()];
        for speed in [0.0, 1.5, 3.0, 4.5, 8.0, 12.2] {
            for frame in 0..90 {
                ride(&mut app, me, speed);
                if frame > 40 {
                    let (_, _, turns) = legs_now(&app, carrier, &rest);
                    for (l, t) in limit.iter_mut().zip(turns) {
                        *l = l.max(t);
                    }
                }
            }
        }
        // Braking hard (8 m/s², logic's) and coasting (3 m/s²) from the top
        // speed and from a walk, to a stand.
        for (from, frames) in [(12.2f32, 76), (12.2, 200), (1.5, 10), (1.5, 25)] {
            for _ in 0..60 {
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
