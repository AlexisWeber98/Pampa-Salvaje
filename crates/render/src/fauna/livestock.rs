//! Cattle and corral horses: Quaternius' animated low-poly models (CC0,
//! from poly.pizza; credited in `assets/models/fauna/quaternius/CREDITS.md`
//! and the root `CREDITS.md`), trimmed to six clips by `tools/glbtrim.py`.
//!
//! Each [`Species`] loads once at startup ([`LivestockModels`]): its glTF
//! scene, its clips and one shared [`AnimationGraph`]. A cow, bull or horse
//! [`Animal`] gets a [`LivestockModel`] child that spawns the shared scene
//! once it has loaded. The child carries the species' scale (the export is
//! about 3 units to the meter) and turns the model (facing +Z) to the
//! animal's heading (+X is its front). When the scene is ready:
//!
//! - its meshes get the settlement props' shared range (no new
//!   `VisibilityRange`);
//! - its materials are swapped for copies tinted to the animal's coat,
//!   made once per coat and material and shared by every animal wearing it
//!   ([`paint`]); the overo cows' patches and the horses' coats are vertex
//!   colours on one copy of the body mesh per coat, shared the same way;
//! - its animation player gets the species' graph.
//!
//! Every frame [`clip_for`] picks the clip from the fauna state
//! ([`AnimalPose`]), walking clips are paced to the ground speed
//! ([`clip_speed`]) and each animal starts its clips at its own seeded
//! offset. A player farther than [`FREEZE_FAR`] from the camera, or not in
//! view, loses its graph: its bones keep their last pose and the animation
//! systems skip it. The cattle never lie down: the simulation has no lying
//! state (every lying pose tried read unnaturally on this rig).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::animation::{graph::AnimationNodeIndex, transition::AnimationTransitions};
use bevy::gltf::{GltfAssetLabel, GltfMaterialName};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::{CowCoat, Heading};

use super::{Animal, AnimalPose, AnimalSeed, Gait};

use crate::AssetRoot;
use crate::gltf::{self, ModelState};

/// Where the animal models live, under the [`AssetRoot`].
pub const LIVESTOCK_DIR: &str = "models/fauna/quaternius";

/// Past this distance from the camera an animal's animation freezes on its
/// current pose, meters; it plays again once back within [`RESUME_NEAR`].
pub const FREEZE_FAR: f32 = 120.0;
/// Within this distance a frozen animal plays again, meters: a little
/// nearer than [`FREEZE_FAR`], so one at the edge does not flicker.
pub const RESUME_NEAR: f32 = 110.0;

/// Above this ground speed a walking animal gallops, m/s.
pub const GALLOP_SPEED: f32 = 2.5;

/// Fade between two clips.
const BLEND: Duration = Duration::from_millis(300);

/// The animals drawn from a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Species {
    Cow,
    Bull,
    Horse,
}

impl Species {
    pub const ALL: [Self; 3] = [Self::Cow, Self::Bull, Self::Horse];

    /// The species `animal` is drawn as, if it is livestock.
    pub fn of(animal: Animal) -> Option<Self> {
        match animal {
            Animal::Cow(_) => Some(Self::Cow),
            Animal::Bull(_) => Some(Self::Bull),
            Animal::Horse(_) => Some(Self::Horse),
            _ => None,
        }
    }

    /// The model file, under the [`AssetRoot`].
    pub fn path(self) -> String {
        let name = match self {
            Self::Cow => "cow",
            Self::Bull => "bull",
            Self::Horse => "horse",
        };
        format!("{LIVESTOCK_DIR}/{name}.glb")
    }

    /// Height at the withers in the model's own units: the top of the back
    /// above the forelegs, measured on the exported meshes.
    pub const fn model_withers(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => 4.4,
            Self::Horse => 3.6,
        }
    }

    /// Height at the withers in the game, meters: like the procedural
    /// cattle and criollo horses they replace (a cow 1.4 m, a horse 1.5 m),
    /// the bull a little bigger.
    pub const fn withers(self) -> f32 {
        match self {
            Self::Cow => 1.4,
            Self::Bull => 1.55,
            Self::Horse => 1.5,
        }
    }

    /// Meters per model unit.
    pub fn scale(self) -> f32 {
        self.withers() / self.model_withers()
    }

    /// The ground speed its `Walk` clip shows, m/s.
    fn walk_speed(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => 1.0,
            Self::Horse => 1.5,
        }
    }

    /// The ground speed its `Gallop` clip shows, m/s.
    fn gallop_speed(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => 6.0,
            Self::Horse => 9.0,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The clips kept in every model, in the order `tools/glbtrim.py` wrote
/// them (their glTF animation indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clip {
    Idle,
    Idle2,
    IdleHeadlow,
    Eating,
    Walk,
    Gallop,
}

impl Clip {
    pub const ALL: [Self; 6] = [
        Self::Idle,
        Self::Idle2,
        Self::IdleHeadlow,
        Self::Eating,
        Self::Walk,
        Self::Gallop,
    ];

    /// The clip's name in the glTF file.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Idle2 => "Idle_2",
            Self::IdleHeadlow => "Idle_Headlow",
            Self::Eating => "Eating",
            Self::Walk => "Walk",
            Self::Gallop => "Gallop",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A seeded mix of `seed` and `salt`, for deterministic variety.
fn mix(seed: u32, salt: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Seconds a corral horse keeps one idle before the next.
const HORSE_IDLE: f32 = 14.0;

/// The clip `animal` plays for `pose`. Standing animals pick one of the
/// idles by their `seed`; corral horses, which never move, change idle
/// (or crop the ground) every [`HORSE_IDLE`] seconds of `clock`.
pub fn clip_for(animal: Animal, pose: &AnimalPose, seed: u32, clock: f32) -> Clip {
    let idles = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];
    match pose.gait {
        Gait::Graze => Clip::Eating,
        Gait::Walk if pose.speed > GALLOP_SPEED => Clip::Gallop,
        Gait::Walk => Clip::Walk,
        Gait::Run => Clip::Gallop,
        _ if matches!(animal, Animal::Horse(_)) => {
            let turn = ((clock.max(0.0) + (seed % 97) as f32) / HORSE_IDLE) as u32;
            [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow, Clip::Eating][mix(seed, turn) as usize % 4]
        }
        _ => idles[mix(seed, 1) as usize % 3],
    }
}

/// Playback speed of `clip` for an animal of `species` moving at
/// `ground` m/s: the walk and gallop paced to it, the rest as authored.
pub fn clip_speed(clip: Clip, species: Species, ground: f32) -> f32 {
    match clip {
        Clip::Walk => (ground / species.walk_speed()).clamp(0.4, 2.0),
        Clip::Gallop => (ground / species.gallop_speed()).clamp(0.6, 1.6),
        _ => 1.0,
    }
}

/// The seeded share of a clip already played when an animal starts it,
/// in `[0, 1)`: neighbours never move in step.
pub fn start_offset(seed: u32, clip: Clip) -> f32 {
    (mix(seed, 7 + clip.index() as u32) % 1000) as f32 / 1000.0
}

/// Whether an animal's animation is frozen: out of view, or past
/// [`FREEZE_FAR`] (back within [`RESUME_NEAR`] once `was` frozen).
pub fn frozen(was: bool, distance: f32, in_view: bool) -> bool {
    !in_view || distance > if was { RESUME_NEAR } else { FREEZE_FAR }
}

/// The model's transform under its animal: turned from facing +Z to the
/// `heading` (+X is the body's front) and scaled to life size.
pub fn model_transform(species: Species, heading: Heading) -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(heading.radians() + FRAC_PI_2))
        .with_scale(Vec3::splat(species.scale()))
}

/// A vertex of the exported mesh (Z up, about 1/100 of a model unit) in
/// the standing animal's body frame, meters: +X its front, +Y up, +Z its
/// right side.
pub fn body_point(species: Species, raw: Vec3) -> Vec3 {
    // The mesh node turns -90° about X and scales by 100: Z up to Y up.
    let scene = Vec3::new(raw.x, raw.z, -raw.y) * 100.0;
    // Then the model turns to face +X.
    Vec3::new(scene.z, scene.y, -scene.x) * species.scale()
}

/// How a coat paints one of the model's materials.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    /// Its own colour, kept.
    Keep,
    /// A flat colour.
    Flat(Color),
    /// White, with the colour in the vertices of the body mesh's copy for
    /// this coat ([`coat_color_at`]).
    Vertex,
    /// Not drawn (the polled Angus' horns).
    Hidden,
}

/// Paint `animal`'s coat gives the material named `material`.
///
/// Cattle: `Main` is the hide and `Main_Light` the face and the lower legs.
/// A Hereford is red with a white face and socks; an Angus all black and
/// polled; an overo white with black patches, its face and legs black.
/// Horses: `Main` is the body (vertex colours, from the criollo coats of
/// the procedural horse), `Hair` the mane and tail, `Main_Dark` and
/// `Main_Light` bits of the head.
pub fn paint(animal: Animal, material: &str) -> Paint {
    let rgb = |c: [u8; 3]| Paint::Flat(Color::srgb_u8(c[0], c[1], c[2]));
    match animal {
        Animal::Cow(coat) | Animal::Bull(coat) => match (coat, material) {
            (CowCoat::Hereford, "Main") => rgb([118, 44, 22]),
            (CowCoat::Hereford, "Main_Light") => rgb([226, 220, 206]),
            (CowCoat::Hereford, "Muzzle") => rgb([200, 150, 132]),
            (CowCoat::Angus, "Main" | "Main_Light") => rgb([22, 20, 19]),
            (CowCoat::Angus, "Muzzle") => rgb([34, 30, 28]),
            (CowCoat::Angus, "Horns") => Paint::Hidden,
            (CowCoat::Overo, "Main") => Paint::Vertex,
            (CowCoat::Overo, "Main_Light") => rgb([24, 22, 20]),
            (CowCoat::Overo, "Muzzle") => rgb([44, 38, 34]),
            _ => Paint::Keep,
        },
        Animal::Horse(coat) => match material {
            "Main" => Paint::Vertex,
            "Hair" => rgb(coat.mane()),
            "Main_Dark" => rgb(coat.points()),
            "Main_Light" => rgb(coat.head()),
            "Hooves" => rgb(coat.hoof()),
            _ => Paint::Keep,
        },
        _ => Paint::Keep,
    }
}

/// Whether point `p` of an overo's body (meters, body frame) lies in a
/// black patch: a few large blotches from crossed waves. An approximation
/// of the real pattern, cheap enough to bake once per vertex.
pub fn overo_black(p: Vec3) -> bool {
    let wave = (p.x * 3.1 + 0.7).sin() * (p.y * 2.3 + p.z * 1.9 + 1.1).sin()
        + 0.6 * (p.x * 5.3 - p.z * 3.7 + 2.0).sin() * (p.y * 4.1 + 0.3).cos();
    wave > 0.3
}

/// The vertex colour of `animal`'s body at `p` (meters, body frame),
/// facing `normal`, as linear RGBA. Only for [`Paint::Vertex`] materials.
pub fn coat_color_at(animal: Animal, p: Vec3, normal: Vec3) -> [f32; 4] {
    let srgb = match animal {
        Animal::Horse(coat) => coat.color_at(p, normal),
        _ if overo_black(p) => [26, 24, 22],
        _ => [230, 226, 216],
    };
    Color::srgb_u8(srgb[0], srgb[1], srgb[2])
        .to_linear()
        .to_f32_array()
}

/// One loaded species: its scene, clips and shared graph.
#[derive(Debug, Clone)]
pub struct SpeciesModel {
    pub scene: Handle<WorldAsset>,
    pub clips: Vec<Handle<AnimationClip>>,
    pub graph: Handle<AnimationGraph>,
    /// The graph node of each [`Clip`], in [`Clip::ALL`] order.
    pub nodes: Vec<AnimationNodeIndex>,
}

/// The livestock models found at startup, by [`Species`] (`None` when the
/// file is missing: that species is not drawn).
#[derive(Resource, Debug, Clone, Default)]
pub struct LivestockModels(pub [Option<SpeciesModel>; 3]);

impl LivestockModels {
    pub fn get(&self, species: Species) -> Option<&SpeciesModel> {
        self.0[species.index()].as_ref()
    }
}

/// The coat copies of the models' materials and body meshes, made once and
/// shared by every animal of that coat.
#[derive(Resource, Debug, Default)]
pub struct CoatCache {
    materials: HashMap<(AssetId<StandardMaterial>, Animal), Handle<StandardMaterial>>,
    meshes: HashMap<(AssetId<Mesh>, Animal), Handle<Mesh>>,
}

impl CoatCache {
    /// Coat copies made so far: materials, then meshes.
    pub fn len(&self) -> (usize, usize) {
        (self.materials.len(), self.meshes.len())
    }
}

/// A livestock model's root, child of its animal.
#[derive(Component, Debug, Clone, Copy)]
pub struct LivestockModel {
    pub species: Species,
}

/// A model still loading: its scene spawns once ready.
#[derive(Component, Debug, Clone)]
pub struct LivestockPending(pub Handle<WorldAsset>);

/// A spawned model's animation state.
#[derive(Component, Debug, Clone)]
pub struct LivestockRig {
    /// The entity with its `AnimationPlayer`.
    pub player: Entity,
    /// A mesh of it, whose visibility says whether it is in view.
    pub probe: Option<Entity>,
    pub clip: Option<Clip>,
    pub frozen: bool,
}

/// Starts loading the three models and builds each one's graph. Without
/// an asset server, a root or the animation plugin (tests, a bare app)
/// none loads.
pub(super) fn load_livestock(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    graphs: Option<ResMut<Assets<AnimationGraph>>>,
) {
    commands.init_resource::<CoatCache>();
    let (Some(server), Some(root), Some(mut graphs)) = (server, root, graphs) else {
        commands.insert_resource(LivestockModels::default());
        return;
    };
    let mut models = LivestockModels::default();
    for species in Species::ALL {
        let path = species.path();
        let Some(scene) = gltf::load_scene(&server, Some(&root), &path) else {
            continue;
        };
        let clips: Vec<Handle<AnimationClip>> = Clip::ALL
            .iter()
            .map(|c| server.load(GltfAssetLabel::Animation(c.index()).from_asset(path.clone())))
            .collect();
        let (graph, nodes) = AnimationGraph::from_clips(clips.iter().cloned());
        models.0[species.index()] = Some(SpeciesModel {
            scene,
            clips,
            graph: graphs.add(graph),
            nodes,
        });
    }
    commands.insert_resource(models);
}

/// Hangs a model under every new cow, bull and corral horse.
pub(super) fn attach_livestock(
    mut commands: Commands,
    models: Option<Res<LivestockModels>>,
    added: Query<(Entity, &Animal, &AnimalPose), Added<Animal>>,
) {
    for (entity, &animal, pose) in &added {
        let Some(species) = Species::of(animal) else {
            continue;
        };
        let mut model = commands.spawn((
            Name::new("Livestock model"),
            LivestockModel { species },
            model_transform(species, pose.heading),
            Visibility::default(),
            ChildOf(entity),
        ));
        if let Some(m) = models.as_ref().and_then(|m| m.get(species)) {
            model.insert(LivestockPending(m.scene.clone()));
        }
    }
}

/// Spawns each waiting model's scene once ready; drops a failed one.
pub(super) fn swap_in_livestock(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    pending: Query<(Entity, &LivestockPending)>,
) {
    let Some(server) = server else { return };
    for (entity, wait) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<LivestockPending>()
                    .try_insert(WorldAssetRoot(wait.0.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).remove::<LivestockPending>();
            }
            ModelState::Loading => {}
        }
    }
}

/// What [`dress_livestock`] tints with.
type Stores<'w> = (
    ResMut<'w, CoatCache>,
    ResMut<'w, Assets<StandardMaterial>>,
    ResMut<'w, Assets<Mesh>>,
);

/// Dresses a model the moment its scene spawns: the shared range on its
/// meshes, its coat, and the species' graph on its player.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn dress_livestock(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    (mut cache, mut materials, mut meshes): Stores,
    models: Option<Res<LivestockModels>>,
    roots: Query<(&ChildOf, &LivestockModel)>,
    animals: Query<&Animal>,
    children: Query<&Children>,
    parts: Query<(
        Option<&Mesh3d>,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&GltfMaterialName>,
        Has<AnimationPlayer>,
    )>,
) {
    let root = ready.entity;
    let Ok((parent, model)) = roots.get(root) else {
        return;
    };
    let (Ok(&animal), Some(species)) = (
        animals.get(parent.parent()),
        models.as_ref().and_then(|m| m.get(model.species)),
    ) else {
        return;
    };
    let range = crate::settlement::ranges()[1].clone();
    let mut player = None;
    let mut probe = None;
    for entity in children.iter_descendants(root) {
        let Ok((mesh, material, name, has_player)) = parts.get(entity) else {
            continue;
        };
        if has_player {
            player = Some(entity);
        }
        let Some(mesh) = mesh else { continue };
        let mut part = commands.entity(entity);
        part.try_insert(range.clone());
        let paint = name.map_or(Paint::Keep, |n| paint(animal, &n.0));
        if paint == Paint::Hidden {
            part.try_insert(Visibility::Hidden);
            continue;
        }
        probe.get_or_insert(entity);
        if let Some(material) = material {
            let tinted = cache
                .materials
                .entry((material.0.id(), animal))
                .or_insert_with(|| {
                    let mut m = materials.get(&material.0).cloned().unwrap_or_default();
                    match paint {
                        Paint::Flat(color) => m.base_color = color,
                        Paint::Vertex => m.base_color = Color::WHITE,
                        Paint::Keep | Paint::Hidden => {}
                    }
                    // The export's metallic sheen reads as plastic.
                    m.metallic = 0.0;
                    m.perceptual_roughness = 0.9;
                    materials.add(m)
                })
                .clone();
            part.try_insert(MeshMaterial3d(tinted));
        }
        if paint == Paint::Vertex {
            let key = (mesh.0.id(), animal);
            let coloured = match cache.meshes.get(&key) {
                Some(h) => Some(h.clone()),
                None => meshes
                    .get(&mesh.0)
                    .map(|m| coat_mesh(m, animal, model.species))
                    .map(|m| {
                        let h = meshes.add(m);
                        cache.meshes.insert(key, h.clone());
                        h
                    }),
            };
            if let Some(coloured) = coloured {
                part.try_insert(Mesh3d(coloured));
            }
        }
    }
    if let Some(player) = player {
        commands.entity(player).try_insert((
            AnimationGraphHandle(species.graph.clone()),
            AnimationTransitions::new(),
        ));
        commands.entity(root).try_insert(LivestockRig {
            player,
            probe,
            clip: None,
            frozen: false,
        });
    }
}

/// `mesh` with each vertex coloured for `animal`'s coat.
fn coat_mesh(mesh: &Mesh, animal: Animal, species: Species) -> Mesh {
    let mut out = mesh.clone();
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => Vec::new(),
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => vec![[0.0, 0.0, 1.0]; positions.len()],
    };
    let colors: Vec<[f32; 4]> = positions
        .iter()
        .zip(normals.iter().chain(std::iter::repeat(&[0.0, 0.0, 1.0])))
        .map(|(p, n)| {
            let at = body_point(species, Vec3::from(*p));
            let normal = body_point(species, Vec3::from(*n)).normalize_or_zero();
            coat_color_at(animal, at, normal)
        })
        .collect();
    out.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    out
}

/// A model as [`drive_livestock`] sees it.
type ModelRig = (
    &'static ChildOf,
    &'static LivestockModel,
    &'static mut Transform,
    &'static GlobalTransform,
    Option<&'static mut LivestockRig>,
);

/// Turns each model, and plays its clip: chosen from the fauna state,
/// paced to its speed, started at its seeded offset, and frozen far off or
/// out of view.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_livestock(
    mut commands: Commands,
    time: Res<Time>,
    models: Option<Res<LivestockModels>>,
    clips: Option<Res<Assets<AnimationClip>>>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    animals: Query<(&Animal, &AnimalPose, Option<&AnimalSeed>)>,
    mut rigs: Query<ModelRig>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    seen: Query<&ViewVisibility>,
) {
    let clock = time.elapsed_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (child_of, model, mut transform, at, rig) in &mut rigs {
        let Ok((&animal, pose, seed)) = animals.get(child_of.parent()) else {
            continue;
        };
        // Only when it moved: a still animal leaves its subtree untouched.
        transform.set_if_neq(model_transform(model.species, pose.heading));
        let (Some(mut rig), Some(species)) =
            (rig, models.as_ref().and_then(|m| m.get(model.species)))
        else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        // Visibility is only known with a camera drawing.
        let in_view = eye.is_none() || rig.probe.is_none_or(|p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = frozen(rig.frozen, distance, in_view);
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
        let Ok((mut player, mut transitions)) = players.get_mut(rig.player) else {
            continue;
        };
        let seed = seed.map_or(child_of.parent().index_u32(), |s| s.0);
        let clip = clip_for(animal, pose, seed, clock);
        let node = species.nodes[clip.index()];
        if rig.clip != Some(clip) {
            let blend = if rig.clip.is_some() {
                BLEND
            } else {
                Duration::ZERO
            };
            let duration = clips
                .as_ref()
                .and_then(|c| c.get(&species.clips[clip.index()]))
                .map_or(0.0, AnimationClip::duration);
            transitions
                .play(&mut player, node, blend)
                .repeat()
                .seek_to(start_offset(seed, clip) * duration);
            rig.clip = Some(clip);
        }
        if let Some(active) = player.animation_mut(node) {
            active.set_speed(clip_speed(clip, model.species, pose.speed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Coat;
    use crate::gltf::tests::{root, settle, unfinished_app};
    use bevy::camera::visibility::VisibilityRange;

    fn standing(gait: Gait, speed: f32) -> AnimalPose {
        AnimalPose {
            gait,
            speed,
            ..default()
        }
    }

    #[test]
    fn fauna_states_map_to_clips() {
        let cow = Animal::Cow(CowCoat::Hereford);
        let clip = |animal, gait, speed| clip_for(animal, &standing(gait, speed), 3, 0.0);
        assert_eq!(clip(cow, Gait::Graze, 0.0), Clip::Eating);
        assert_eq!(clip(cow, Gait::Walk, 0.5), Clip::Walk);
        assert_eq!(
            clip(cow, Gait::Walk, 1.3),
            Clip::Walk,
            "walking off a rider"
        );
        assert_eq!(clip(cow, Gait::Walk, 4.0), Clip::Gallop);
        assert_eq!(clip(cow, Gait::Run, 4.0), Clip::Gallop);
        // Standing: one of the idles, the same for the same animal, and
        // not the same for the whole herd.
        let idles: std::collections::HashSet<Clip> = (0..40)
            .map(|seed| {
                let c = clip_for(cow, &standing(Gait::Stand, 0.0), seed, 0.0);
                assert_eq!(c, clip_for(cow, &standing(Gait::Stand, 0.0), seed, 99.0));
                c
            })
            .collect();
        assert_eq!(
            idles,
            [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow].into(),
            "{idles:?}"
        );
        // A corral horse changes what it does now and then, deterministically.
        let horse = Animal::Horse(Coat::Bayo);
        let seen: std::collections::HashSet<Clip> = (0..40)
            .map(|k| {
                clip_for(
                    horse,
                    &standing(Gait::Stand, 0.0),
                    11,
                    k as f32 * HORSE_IDLE,
                )
            })
            .collect();
        assert!(seen.len() >= 3 && seen.contains(&Clip::Eating), "{seen:?}");
        assert_eq!(
            clip_for(horse, &standing(Gait::Stand, 0.0), 11, 30.0),
            clip_for(horse, &standing(Gait::Stand, 0.0), 11, 30.0)
        );
    }

    #[test]
    fn walking_clips_are_paced_to_the_ground_speed_and_seeded_apart() {
        let slow = clip_speed(Clip::Walk, Species::Cow, 0.5);
        let shy = clip_speed(Clip::Walk, Species::Cow, 1.3);
        assert!(slow < 1.0 && shy > 1.0 && shy > slow, "{slow} {shy}");
        assert_eq!(clip_speed(Clip::Walk, Species::Cow, 0.0), 0.4, "clamped");
        assert_eq!(clip_speed(Clip::Eating, Species::Cow, 0.0), 1.0);
        let offsets: Vec<f32> = (0..20).map(|s| start_offset(s, Clip::Eating)).collect();
        assert!(offsets.iter().all(|o| (0.0..1.0).contains(o)));
        let distinct: std::collections::HashSet<u32> =
            offsets.iter().map(|o| (o * 1000.0) as u32).collect();
        assert!(distinct.len() >= 15, "{offsets:?}");
    }

    #[test]
    fn animation_freezes_far_off_or_out_of_view() {
        assert!(!frozen(false, 50.0, true));
        assert!(frozen(false, 50.0, false), "culled");
        assert!(!frozen(false, FREEZE_FAR - 1.0, true));
        assert!(frozen(false, FREEZE_FAR + 1.0, true));
        // Between the two it keeps whatever it was doing.
        let edge = (FREEZE_FAR + RESUME_NEAR) / 2.0;
        assert!(frozen(true, edge, true));
        assert!(!frozen(false, edge, true));
        assert!(!frozen(true, RESUME_NEAR - 1.0, true));
        // Every frozen animal is still drawn: the props' range reaches past.
        let props = crate::settlement::ranges()[1].clone();
        assert!(props.end_margin.start > FREEZE_FAR);
    }

    #[test]
    fn models_are_scaled_to_life_and_turned_to_the_heading() {
        let heading = Heading::from_radians(0.7);
        for species in Species::ALL {
            let t = model_transform(species, heading);
            // The model's front (+Z) points along the body's heading.
            let front = (t.rotation * Vec3::Z).normalize();
            let along = Quat::from_rotation_y(heading.radians()) * Vec3::X;
            assert!(front.distance(along) < 1e-5, "{species:?}: {front}");
            let withers = t.scale.y * species.model_withers();
            assert!((withers - species.withers()).abs() < 1e-4);
        }
        assert!(Species::Bull.withers() > Species::Cow.withers());
        assert!((1.3..1.5).contains(&Species::Cow.withers()));
        assert!((1.4..1.6).contains(&Species::Horse.withers()));
        // A raw vertex up the mesh's Z is up in the body; one down its -Y
        // (the head's side) is at the body's front.
        let up = body_point(Species::Cow, Vec3::new(0.0, 0.0, 0.01));
        let head = body_point(Species::Cow, Vec3::new(0.0, -0.01, 0.0));
        assert!(up.y > 0.0 && up.x.abs() < 1e-6 && head.x > 0.0 && head.y.abs() < 1e-6);
    }

    #[test]
    fn coats_tint_per_breed_and_overos_are_patched() {
        let tint = |animal, material| paint(animal, material);
        let red = tint(Animal::Cow(CowCoat::Hereford), "Main");
        let Paint::Flat(red) = red else {
            panic!("{red:?}")
        };
        let red = red.to_srgba();
        assert!(
            red.red > red.green * 1.8 && red.red > red.blue * 2.0,
            "{red:?}"
        );
        let Paint::Flat(face) = tint(Animal::Cow(CowCoat::Hereford), "Main_Light") else {
            panic!()
        };
        assert!(face.to_srgba().green > 0.8, "a white face");
        for material in ["Main", "Main_Light"] {
            let Paint::Flat(black) = tint(Animal::Cow(CowCoat::Angus), material) else {
                panic!()
            };
            assert!(black.to_srgba().red < 0.1, "black Angus");
        }
        assert_eq!(tint(Animal::Cow(CowCoat::Angus), "Horns"), Paint::Hidden);
        assert_eq!(tint(Animal::Cow(CowCoat::Overo), "Main"), Paint::Vertex);
        assert_eq!(
            tint(Animal::Cow(CowCoat::Hereford), "Eye_Black"),
            Paint::Keep
        );
        // The bull wears its breed's coat like the cows.
        for material in ["Main", "Main_Light", "Muzzle", "Horns"] {
            for coat in CowCoat::ALL {
                assert_eq!(
                    tint(Animal::Bull(coat), material),
                    tint(Animal::Cow(coat), material)
                );
            }
        }
        // About a third to a half of an overo's barrel is black.
        let mut black = 0;
        let mut all = 0;
        for i in 0..30 {
            for j in 0..10 {
                for k in 0..8 {
                    let p = Vec3::new(
                        -1.0 + i as f32 * 0.07,
                        0.7 + j as f32 * 0.07,
                        -0.4 + k as f32 * 0.1,
                    );
                    all += 1;
                    black += usize::from(overo_black(p));
                }
            }
        }
        let share = black as f32 / all as f32;
        assert!((0.2..0.55).contains(&share), "{share}");
        // Horses take the criollo coats: the body in vertex colours like
        // the procedural horse's, the mane its colour.
        for coat in Coat::ALL {
            assert_eq!(tint(Animal::Horse(coat), "Main"), Paint::Vertex);
            let m = coat.mane();
            assert_eq!(
                tint(Animal::Horse(coat), "Hair"),
                Paint::Flat(Color::srgb_u8(m[0], m[1], m[2]))
            );
            let p = Vec3::new(0.0, 1.1, 0.4);
            let c = coat.color_at(p, Vec3::Z);
            let want = Color::srgb_u8(c[0], c[1], c[2]).to_linear().to_f32_array();
            assert_eq!(coat_color_at(Animal::Horse(coat), p, Vec3::Z), want);
        }
    }

    /// The three shipped models, loaded headless.
    fn loaded() -> (App, Vec<Handle<bevy::gltf::Gltf>>) {
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin);
        app.finish();
        app.cleanup();
        let server = app.world().resource::<AssetServer>().clone();
        let mut handles = Vec::new();
        for species in Species::ALL {
            let path = species.path();
            let scene = gltf::load_scene(&server, Some(&root()), &path).expect(&path);
            assert_eq!(settle(&mut app, &scene), ModelState::Ready, "{path}");
            let handle: Handle<bevy::gltf::Gltf> = server.load(path.clone());
            while !app
                .world()
                .resource::<Assets<bevy::gltf::Gltf>>()
                .contains(&handle)
            {
                app.update();
            }
            handles.push(handle);
        }
        (app, handles)
    }

    #[test]
    fn the_three_models_load_skinned_with_their_clips_at_life_size() {
        let (app, handles) = loaded();
        let world = app.world();
        for (species, handle) in Species::ALL.into_iter().zip(handles) {
            let gltf = world
                .resource::<Assets<bevy::gltf::Gltf>>()
                .get(&handle)
                .unwrap();
            // The clips the game plays, in the order it plays them by.
            assert_eq!(gltf.animations.len(), Clip::ALL.len(), "{species:?}");
            for clip in Clip::ALL {
                let named = &gltf.named_animations[clip.name()];
                assert_eq!(
                    *named,
                    gltf.animations[clip.index()],
                    "{species:?} {clip:?}"
                );
                let clip = world
                    .resource::<Assets<AnimationClip>>()
                    .get(named)
                    .unwrap();
                assert!(clip.duration() > 0.3, "{species:?}");
            }
            assert_eq!(gltf.skins.len(), 1);
            let skin = world
                .resource::<Assets<bevy::gltf::GltfSkin>>()
                .get(&gltf.skins[0])
                .unwrap();
            let joints = match species {
                Species::Horse => 50,
                _ => 42,
            };
            assert_eq!(skin.joints.len(), joints, "{species:?}");
            // Its body stands at life size in the body frame.
            let mut top = 0.0f32;
            let mut triangles = 0;
            let mut names: Vec<&str> = gltf.named_materials.keys().map(|k| &**k).collect();
            names.sort();
            let mesh = world
                .resource::<Assets<bevy::gltf::GltfMesh>>()
                .get(&gltf.meshes[0])
                .unwrap();
            for primitive in &mesh.primitives {
                let m = world
                    .resource::<Assets<Mesh>>()
                    .get(&primitive.mesh)
                    .unwrap();
                triangles += m.indices().map_or(0, |i| i.len() / 3);
                let Some(VertexAttributeValues::Float32x3(p)) =
                    m.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("positions")
                };
                for p in p {
                    let b = body_point(species, Vec3::from(*p));
                    // The back, behind the head and neck.
                    if b.x < 0.2 {
                        top = top.max(b.y);
                    }
                }
            }
            println!("{species:?}: {triangles} triangles, back {top:.2} m, materials {names:?}");
            assert!(triangles <= 2_500, "{species:?}: {triangles}");
            for material in ["Main", "Main_Light", "Muzzle", "Hooves"] {
                assert!(names.contains(&material), "{species:?}: {names:?}");
            }
            let want = species.withers();
            assert!(
                (want * 0.9..want * 1.1).contains(&top),
                "{species:?}: back at {top}"
            );
        }
    }

    /// The fauna plugin with the shipped models, headless.
    fn fauna_app() -> App {
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .add_plugins(super::super::FaunaPlugin);
        app.finish();
        app.cleanup();
        app
    }

    /// Updates `app` until every model has its rig; panics after 10 s.
    fn until_rigged(app: &mut App, count: usize) {
        for _ in 0..2000 {
            app.update();
            let mut q = app.world_mut().query::<&LivestockRig>();
            if q.iter(app.world()).count() == count {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the models never spawned");
    }

    #[test]
    fn animals_spawn_tinted_and_animated_and_freeze_far_off() {
        let mut app = fauna_app();
        let grazing = AnimalPose {
            gait: Gait::Graze,
            ..default()
        };
        let herd = [
            Animal::Cow(CowCoat::Angus),
            Animal::Cow(CowCoat::Angus),
            Animal::Cow(CowCoat::Overo),
            Animal::Bull(CowCoat::Overo),
            Animal::Horse(Coat::Tordillo),
        ];
        for (k, animal) in herd.into_iter().enumerate() {
            app.world_mut().spawn((
                animal,
                grazing,
                AnimalSeed(k as u32),
                Transform::from_xyz(k as f32 * 4.0, 0.0, 0.0),
            ));
        }
        until_rigged(&mut app, herd.len());
        app.update();
        let world = app.world_mut();
        // Every player has its species' graph and plays its clip.
        let mut rigs = world.query::<(&LivestockRig, &LivestockModel)>();
        let rigs: Vec<(LivestockRig, Species)> = rigs
            .iter(world)
            .map(|(r, m)| (r.clone(), m.species))
            .collect();
        for (rig, _) in &rigs {
            assert!(!rig.frozen);
            assert!(rig.clip.is_some());
            let player = world.get::<AnimationPlayer>(rig.player).unwrap();
            assert_eq!(player.playing_animations().count(), 1);
            assert!(world.get::<AnimationGraphHandle>(rig.player).is_some());
        }
        let cows = rigs.iter().filter(|r| r.1 == Species::Cow).count();
        assert_eq!(cows, 3);
        // The meshes share the props' range, and the coat copies are made
        // once per coat: the two Angus share theirs.
        let props = crate::settlement::ranges()[1].clone();
        let mut parts = world.query::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &GltfMaterialName,
            &VisibilityRange,
            Option<&Visibility>,
        )>();
        let mut angus_main = std::collections::HashSet::new();
        let mut horns_hidden = 0;
        for (mesh, material, name, range, visibility) in parts.iter(world) {
            assert!(*range == props, "a new range");
            let m = world
                .resource::<Assets<StandardMaterial>>()
                .get(&material.0)
                .unwrap();
            if name.0 == "Horns" && visibility == Some(&Visibility::Hidden) {
                horns_hidden += 1;
                continue;
            }
            assert_eq!(m.metallic, 0.0, "{}", name.0);
            if name.0 == "Main" && m.base_color.to_srgba().red < 0.1 {
                angus_main.insert(material.0.id());
            }
            let coloured = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_COLOR)
                .is_some();
            assert_eq!(coloured, m.base_color == Color::WHITE && name.0 == "Main");
        }
        assert_eq!(angus_main.len(), 1, "one shared Angus hide");
        assert_eq!(horns_hidden, 2, "the Angus are polled");
        let (materials, meshes) = world.resource::<CoatCache>().len();
        // Two cow coats, an overo bull and a horse, a few materials each;
        // the overo cow's, the overo bull's and the horse's body meshes.
        assert!(materials <= 4 * 8, "{materials}");
        assert_eq!(meshes, 3);

        // A camera far off: every animal freezes, its graph taken away.
        world.spawn((Camera3d::default(), Transform::from_xyz(0.0, 2.0, 500.0)));
        app.update();
        app.update();
        let world = app.world_mut();
        let mut rigs = world.query::<&LivestockRig>();
        let rigs: Vec<LivestockRig> = rigs.iter(world).cloned().collect();
        for rig in rigs {
            assert!(rig.frozen);
            assert!(world.get::<AnimationGraphHandle>(rig.player).is_none());
        }
    }
}
