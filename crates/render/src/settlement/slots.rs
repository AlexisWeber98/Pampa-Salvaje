//! Model slots: third-party glTF models that replace a structure kind's
//! procedural mesh, and the scenes the settlements spawn from them.
//!
//! Each kind with a [`StructureKind::slot`] looks for
//! `assets/models/third-party/<slot>/model.glb` (or `model.gltf`) at
//! startup ([`SlotModels`]). Where one is found, a structure of that kind
//! spawns the model's scene, fitted by its [`SlotFit`] (from
//! `slots.ron`, loaded by the game), instead of its procedural mesh:
//!
//! - while the model loads, the procedural mesh stands in ([`StandIn`])
//!   and the scene waits ([`SlotPending`]);
//! - once it is ready the scene spawns, its meshes are dressed with the
//!   structure's shared range and fade ([`SceneDress`]), its rotor node
//!   spins ([`ModelRotor`]) and the stand-in goes;
//! - if it fails to load, the stand-in stays for good.
//!
//! Obstacles and camera occluders come from `pampa-core` and do not change.

use std::collections::HashMap;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::StructureKind;

use super::SettlementPart;
use crate::AssetRoot;
use crate::flora::ThinOccluder;
use crate::gltf::{self, ModelState};

/// Where the model slots live, under the [`AssetRoot`].
pub const SLOT_DIR: &str = "models/third-party";

/// How a slot's model sits in its structure's own space: scaled, turned
/// about the vertical by `yaw` radians, then moved by `offset` meters.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotFit {
    pub scale: f32,
    pub offset: Vec3,
    pub yaw: f32,
    pub rotor: Option<SlotRotor>,
}

impl Default for SlotFit {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: Vec3::ZERO,
            yaw: 0.0,
            rotor: None,
        }
    }
}

impl SlotFit {
    /// The model's transform in its structure's own space.
    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.offset)
            .with_rotation(Quat::from_rotation_y(self.yaw))
            .with_scale(Vec3::splat(self.scale))
    }
}

/// A model's spinning part: its glTF node and the axis, in the node's own
/// space, it turns about.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRotor {
    pub node: String,
    pub axis: Dir3,
}

/// The fits of the model slots, by slot name. Insert before the
/// [`super::SettlementPlugin`] starts; a slot without one is drawn as its
/// model comes.
#[derive(Resource, Debug, Clone, Default)]
pub struct ModelSlots(pub HashMap<String, SlotFit>);

impl ModelSlots {
    /// The fit of `slot`, or the identity.
    pub fn fit(&self, slot: &str) -> SlotFit {
        self.0.get(slot).cloned().unwrap_or_default()
    }
}

/// The folder, under the [`AssetRoot`], the slots are looked up in
/// ([`SLOT_DIR`] unless a test points elsewhere).
#[derive(Resource, Debug, Clone)]
pub struct SlotFolder(pub String);

impl Default for SlotFolder {
    fn default() -> Self {
        Self(SLOT_DIR.to_owned())
    }
}

/// The slot models found at startup, loading or loaded, by slot name.
#[derive(Resource, Debug, Clone, Default)]
pub struct SlotModels(pub HashMap<&'static str, Handle<WorldAsset>>);

/// The model file of `slot` under `folder`, if there is one: `model.glb`
/// first, then `model.gltf`.
pub fn slot_path(root: &AssetRoot, folder: &str, slot: &str) -> Option<String> {
    ["glb", "gltf"]
        .into_iter()
        .map(|ext| format!("{folder}/{slot}/model.{ext}"))
        .find(|path| gltf::is_inside(path) && root.0.join(path).is_file())
}

/// Starts loading every slot that has a model. Without an asset server or
/// root (tests, a bare app) none is loaded and everything is procedural.
pub(super) fn load_slot_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    folder: Res<SlotFolder>,
) {
    let mut models = SlotModels::default();
    if let (Some(server), Some(root)) = (server, root) {
        for slot in StructureKind::SLOTS {
            let Some(path) = slot_path(&root, &folder.0, slot) else {
                continue;
            };
            if let Some(scene) = gltf::load_scene(&server, Some(&root), &path) {
                info!("model slot `{slot}`: {path}");
                models.0.insert(slot, scene);
            }
        }
    }
    commands.insert_resource(models);
}

/// What the meshes of a spawned model scene get: the structure's shared
/// range, its fade shape, the spin of its rotor node, and whether its
/// see-through materials need their glass fixed (see [`dress_scene`]).
#[derive(Component, Clone)]
pub struct SceneDress {
    pub range: VisibilityRange,
    pub fade: Option<ThinOccluder>,
    /// The rotor node and its speed in radians per second.
    pub rotor: Option<(SlotRotor, f32)>,
    pub glass: bool,
}

/// A slot model still loading: its scene is spawned here once ready.
#[derive(Component, Debug, Clone)]
pub struct SlotPending(pub Handle<WorldAsset>);

/// A procedural piece drawn while its structure's slot model loads, and
/// despawned when the model's scene is ready.
#[derive(Component, Debug, Clone, Copy)]
pub struct StandIn;

/// A model's node spinning about `axis` (its own space), like a windpump's
/// wheel.
#[derive(Component, Debug, Clone, Copy)]
pub struct ModelRotor {
    pub axis: Dir3,
    /// Radians per second.
    pub speed: f32,
}

/// Spawns each waiting slot model's scene once its model is ready. If it
/// failed, drops the wait and keeps the stand-in for good (no longer a
/// stand-in).
pub(super) fn swap_in_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    pending: Query<(
        Entity,
        &SlotPending,
        Option<&SettlementPart>,
        Option<&ChildOf>,
    )>,
    stand_ins: Query<(Entity, &SettlementPart, &ChildOf), With<StandIn>>,
) {
    let Some(server) = server else {
        return;
    };
    for (entity, wait, part, parent) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                let scene = wait.0.clone();
                commands
                    .entity(entity)
                    .remove::<SlotPending>()
                    .try_insert(WorldAssetRoot(scene));
            }
            ModelState::Failed => {
                commands.entity(entity).try_despawn();
                if let (Some(part), Some(parent)) = (part, parent) {
                    for (stand_in, of, chunk) in &stand_ins {
                        if of == part && chunk.parent() == parent.parent() {
                            commands.entity(stand_in).try_remove::<StandIn>();
                        }
                    }
                }
            }
            ModelState::Loading => {}
        }
    }
}

/// The see-through look given to a glTF's blended glass whose base colour
/// texture has no alpha: how opaque it is.
pub const GLASS_ALPHA: f32 = 0.3;

/// What [`dress_scene`] reads off a spawned scene's entities.
type SceneParts<'a> = (
    Option<&'a Name>,
    Has<Mesh3d>,
    Option<&'a MeshMaterial3d<StandardMaterial>>,
);

/// Dresses a model scene the moment it spawns: every mesh gets the shared
/// range (and fade shape), the rotor node spins, blended glass is made
/// see-through, and the structure's stand-in goes.
///
/// Glass: a glTF material in `BLEND` mode whose base colour is an opaque
/// JPEG multiplies to alpha 1, so it draws solid. Its base colour alpha is
/// set to [`GLASS_ALPHA`] and it is made smooth, so the lantern's panes read
/// as glass. The material is the glTF's own, shared by every instance, and
/// the change is idempotent.
pub(super) fn dress_scene(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    dresses: Query<(&SceneDress, Option<&SettlementPart>, Option<&ChildOf>)>,
    children: Query<&Children>,
    parts: Query<SceneParts>,
    stand_ins: Query<(Entity, &SettlementPart, &ChildOf), With<StandIn>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let root = ready.entity;
    let Ok((dress, part, parent)) = dresses.get(root) else {
        return;
    };
    let mut named = Vec::new();
    for entity in children.iter_descendants(root) {
        let Ok((name, mesh, material)) = parts.get(entity) else {
            continue;
        };
        if mesh {
            let mut dressed = commands.entity(entity);
            dressed.try_insert(dress.range.clone());
            if let Some(fade) = dress.fade {
                dressed.try_insert(fade);
            }
        }
        if dress.glass
            && let Some(material) = material
            && let Some(mut glass) = materials.get_mut(&material.0)
            && glass.alpha_mode == AlphaMode::Blend
            && glass.base_color.alpha() > GLASS_ALPHA
        {
            glass.base_color.set_alpha(GLASS_ALPHA);
            glass.perceptual_roughness = 0.08;
            glass.reflectance = 0.6;
        }
        if let Some((rotor, _)) = &dress.rotor
            && name.is_some_and(|n| n.as_str() == rotor.node)
        {
            named.push((entity, mesh));
        }
    }
    // The node itself, not its mesh primitives (which may share its name).
    let spun = named
        .iter()
        .find(|(_, mesh)| !mesh)
        .or(named.first())
        .map(|(e, _)| *e);
    if let (Some((rotor, speed)), Some(entity)) = (&dress.rotor, spun) {
        commands.entity(entity).try_insert(ModelRotor {
            axis: rotor.axis,
            speed: *speed,
        });
    }
    if let (Some(part), Some(parent)) = (part, parent) {
        for (entity, of, chunk) in &stand_ins {
            if of == part && chunk.parent() == parent.parent() {
                commands.entity(entity).try_despawn();
            }
        }
    }
}

/// Turns every [`ModelRotor`].
pub(super) fn spin_model_rotors(time: Res<Time>, mut rotors: Query<(&ModelRotor, &mut Transform)>) {
    for (rotor, mut transform) in &mut rotors {
        transform.rotate_local_axis(rotor.axis, rotor.speed * time.delta_secs());
    }
}
