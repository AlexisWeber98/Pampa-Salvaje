//! MakeHuman characters: skinned glTF people that replace a figure's rigid
//! parts (see `assets/characters/README.md` and `tools/mh2gltf.py`).
//!
//! The models are built offline from MakeHuman's CC0 data and a few
//! community assets (CC0, and CC-BY by their authors; credited in
//! `assets/characters/CREDITS.md` and the root `CREDITS.md`). Their 1919
//! wardrobe is mostly made by the converter: shirts, waistcoats, a police
//! guerrera, a three-piece suit, bombachas and knee boots cut from the
//! body and skinned like it, and the hats, kepi, neckerchiefs, faja and
//! rastra, poncho, strap, watch chain and brass buttons. They are drawn by default
//! ([`CharacterModelsShown`]); the debug console's `models off`
//! brings back the rigid figures.
//!
//! A [`FigureKind`] with a model ([`model_name`]) looks for
//! `assets/characters/<name>/model.gltf` at startup ([`CharacterModels`]).
//! With the models shown, a figure of that kind on foot still spawns its
//! rigid body, joints and parts, and also a [`CharacterModel`] under its
//! body:
//!
//! - while the model loads, the rigid figure is drawn and the model waits
//!   ([`ModelPending`]);
//! - once it is ready its scene spawns, turned to face the body's front
//!   (+X; the glTF faces +Z), its meshes get the props' shared range, its
//!   skeleton is found ([`Rig`]) and the rigid parts are hidden (the arms
//!   already hang at the sides: the converter lowers them out of
//!   MakeHuman's A-pose into the rest pose it writes);
//! - if it fails to load, the rigid figure stays for good.
//!
//! The body keeps turning and bobbing as before, carrying the model, and
//! the bone driver ([`rig`](super::rig)) poses its skeleton by the figure's
//! joint angles: it walks, jogs, breathes and glances like the figure.
//! Riders keep their rigid figures for now.

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};

use super::figure::{FigureBody, FigureJoint, FigureKind};
use super::rig::{Bone, Rig, curled, rest_frame};
use crate::AssetRoot;
use crate::gltf::{self, ModelState};

/// Where the character models live, under the [`AssetRoot`].
pub const CHARACTER_DIR: &str = "characters";

/// The model folder of a figure kind, if it has one. Only figures on foot
/// wear it: a rider keeps the rigid figure.
pub fn model_name(kind: FigureKind) -> Option<&'static str> {
    match kind {
        FigureKind::Protagonist => Some("protagonist"),
        FigureKind::Hidalgo => Some("hidalgo"),
        FigureKind::Farach => Some("farach"),
        FigureKind::Bairoletto => Some("bairoletto"),
        FigureKind::Townsfolk(_) => None,
    }
}

/// The model file of `name` under the [`CHARACTER_DIR`].
pub fn model_path(name: &str) -> String {
    format!("{CHARACTER_DIR}/{name}/model.gltf")
}

/// The character models found at startup, loading or loaded.
#[derive(Resource, Debug, Clone, Default)]
pub struct CharacterModels(pub HashMap<FigureKind, Handle<WorldAsset>>);

/// Starts loading every character model there is. Without an asset server
/// or root (tests, a bare app) none is loaded and every figure stays rigid.
pub(super) fn load_character_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
) {
    let mut models = CharacterModels::default();
    if let (Some(server), Some(root)) = (server, root) {
        for kind in FigureKind::all() {
            let Some(name) = model_name(kind) else {
                continue;
            };
            if let Some(scene) = gltf::load_scene(&server, Some(&root), &model_path(name)) {
                info!("character model `{name}`");
                models.0.insert(kind, scene);
            }
        }
    }
    commands.insert_resource(models);
}

/// Whether the character models are drawn instead of the rigid figures
/// (on by default). Turning it on or off redraws the figures standing.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterModelsShown(pub bool);

impl Default for CharacterModelsShown {
    fn default() -> Self {
        Self(true)
    }
}

/// A character model's root, child of its figure's body.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterModel;

/// A character model still loading: its scene spawns here once ready.
#[derive(Component, Debug, Clone)]
pub struct ModelPending(pub Handle<WorldAsset>);

/// The model's root transform under the body: the glTF faces +Z, the
/// body's front is +X.
pub fn model_transform() -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(FRAC_PI_2))
}

/// Puts `kind`'s model, if it has one, under the figure `body`.
pub(super) fn attach_model(
    commands: &mut Commands,
    models: Option<&CharacterModels>,
    body: Entity,
    kind: FigureKind,
) {
    let Some(scene) = models.and_then(|m| m.0.get(&kind)) else {
        return;
    };
    commands.spawn((
        Name::new("Character model"),
        CharacterModel,
        ModelPending(scene.clone()),
        model_transform(),
        Visibility::default(),
        ChildOf(body),
    ));
}

/// Puts the models on (or takes them off) the figures already standing
/// when [`CharacterModelsShown`] changes; taken off, their rigid parts
/// show again.
pub(super) fn toggle_character_models(
    mut commands: Commands,
    shown: Res<CharacterModelsShown>,
    models: Option<Res<CharacterModels>>,
    bodies: Query<(Entity, &FigureBody, Option<&Children>)>,
    attached: Query<(), With<CharacterModel>>,
    rigid: Query<(), With<FigureJoint>>,
) {
    if !shown.is_changed() {
        return;
    }
    for (body, figure, children) in &bodies {
        if figure.seat.is_some() {
            continue;
        }
        let children = children.into_iter().flatten().copied();
        let has_model = children.clone().any(|c| attached.contains(c));
        if shown.0 && !has_model {
            attach_model(&mut commands, models.as_deref(), body, figure.kind);
        } else if !shown.0 && has_model {
            for child in children {
                if attached.contains(child) {
                    commands.entity(child).try_despawn();
                } else if rigid.contains(child) {
                    commands.entity(child).try_insert(Visibility::Inherited);
                }
            }
        }
    }
}

/// Spawns each waiting model's scene once ready; drops a failed one, so
/// its figure stays rigid.
pub(super) fn swap_in_character_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    pending: Query<(Entity, &ModelPending)>,
) {
    let Some(server) = server else {
        return;
    };
    for (entity, wait) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<ModelPending>()
                    .try_insert(WorldAssetRoot(wait.0.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).try_despawn();
            }
            ModelState::Loading => {}
        }
    }
}

/// The range a character model's meshes are drawn within: the settlement
/// props' shared one (no new GPU slot).
pub fn model_range() -> VisibilityRange {
    crate::settlement::ranges()[1].clone()
}

/// Dresses a character model the moment its scene spawns: the shared
/// range on its meshes, its skeleton found for the bone driver, its fingers
/// curled, and its figure's rigid parts hidden.
#[allow(clippy::too_many_arguments)]
pub(super) fn dress_character(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    models: Query<&ChildOf, With<CharacterModel>>,
    children: Query<&Children>,
    parts: Query<(Option<&Name>, Has<Mesh3d>)>,
    rigid: Query<(), With<FigureJoint>>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
) {
    let root = ready.entity;
    let Ok(body) = models.get(root) else {
        return;
    };
    let range = model_range();
    let mut rig = Rig::default();
    for entity in children.iter_descendants(root) {
        let Ok((name, mesh)) = parts.get(entity) else {
            continue;
        };
        if mesh {
            commands.entity(entity).try_insert(range.clone());
            rig.probe.get_or_insert(entity);
        }
        let (Some(name), Ok(t)) = (name, transforms.get(entity)) else {
            continue;
        };
        if let Some(bone) = Bone::named(name.as_str()) {
            let frame = rest_frame(entity, root, &parents, &transforms);
            rig.bind(bone, entity, t, frame);
        } else if let Some(curl) = curled(name.as_str(), t.rotation) {
            commands.entity(entity).try_insert(t.with_rotation(curl));
        }
    }
    if !rig.is_complete() {
        warn!("character model without the whole game_engine skeleton: some bones stay still");
    }
    commands.entity(root).try_insert(rig);
    for &child in children.get(body.parent()).into_iter().flatten() {
        if rigid.contains(child) {
            commands.entity(child).try_insert(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::tests::{root, settle, unfinished_app};
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
    use bevy::mesh::{Indices, Mesh};

    /// The triangle budget of one character model.
    const BUDGET: usize = 15_000;
    /// Fewer triangles than this means a model lost its body or clothes
    /// (the lightest, with its garments cut from the body, has about 4,900).
    const FLOOR: usize = 3_000;

    #[test]
    fn the_protagonist_and_the_errand_people_have_models_and_each_is_shipped() {
        assert_eq!(model_name(FigureKind::Townsfolk(1)), None);
        for kind in [
            FigureKind::Protagonist,
            FigureKind::Hidalgo,
            FigureKind::Farach,
            FigureKind::Bairoletto,
        ] {
            let path = model_path(model_name(kind).unwrap());
            assert!(gltf::is_inside(&path));
            assert!(root().0.join(&path).is_file(), "{path}");
        }
    }

    #[test]
    fn every_character_model_loads_skinned_within_the_triangle_budget() {
        let mut app = unfinished_app(&root());
        app.init_asset::<SkinnedMeshInverseBindposes>();
        app.finish();
        app.cleanup();
        let server = app.world().resource::<AssetServer>().clone();
        for name in ["hidalgo", "farach", "bairoletto", "protagonist"] {
            let path = model_path(name);
            let scene = gltf::load_scene(&server, Some(&root()), &path).expect(&path);
            assert_eq!(settle(&mut app, &scene), ModelState::Ready, "{path}");
            let gltf: Handle<bevy::gltf::Gltf> = server.load(path.clone());
            while !app
                .world()
                .resource::<Assets<bevy::gltf::Gltf>>()
                .contains(&gltf)
            {
                app.update();
            }
            let world = app.world();
            let gltf = world
                .resource::<Assets<bevy::gltf::Gltf>>()
                .get(&gltf)
                .unwrap();
            assert_eq!(gltf.skins.len(), 1, "{name}");
            let mut triangles = 0;
            for mesh in &gltf.meshes {
                let mesh = world
                    .resource::<Assets<bevy::gltf::GltfMesh>>()
                    .get(mesh)
                    .unwrap();
                for primitive in &mesh.primitives {
                    let mesh = world
                        .resource::<Assets<Mesh>>()
                        .get(&primitive.mesh)
                        .unwrap();
                    assert!(
                        mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some(),
                        "{name}"
                    );
                    assert!(
                        mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some(),
                        "{name}"
                    );
                    triangles += match mesh.indices() {
                        Some(Indices::U16(i)) => i.len() / 3,
                        Some(Indices::U32(i)) => i.len() / 3,
                        None => mesh.count_vertices() / 3,
                    };
                }
            }
            assert!(
                (FLOOR..=BUDGET).contains(&triangles),
                "{name}: {triangles} triangles"
            );
        }
    }

    /// The character plugin, headless, with a stand-in model for Farach
    /// (never loaded: no asset server), and Farach standing.
    fn toggle_app() -> (App, Entity) {
        use crate::character::{CharacterVisual, CharacterVisualPlugin};
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(CharacterVisualPlugin);
        app.update();
        let mut models = CharacterModels::default();
        models.0.insert(FigureKind::Farach, Handle::default());
        app.insert_resource(models);
        let farach = app
            .world_mut()
            .spawn(CharacterVisual::Figure(FigureKind::Farach))
            .id();
        app.world_mut()
            .spawn(CharacterVisual::Figure(FigureKind::Townsfolk(0)));
        app.update();
        (app, farach)
    }

    fn model_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<CharacterModel>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn the_models_are_shown_by_default_and_off_restores_the_figures() {
        let (mut app, _) = toggle_app();
        assert!(app.world().resource::<CharacterModelsShown>().0);
        // Farach, standing, has his model; the townsperson has none.
        assert_eq!(model_count(&mut app), 1);
        app.update();
        assert_eq!(model_count(&mut app), 1, "attached once");

        // Off: the models go and the rigid parts show.
        app.insert_resource(CharacterModelsShown(false));
        app.update();
        assert_eq!(model_count(&mut app), 0);
        let mut joints = app
            .world_mut()
            .query_filtered::<&Visibility, With<FigureJoint>>();
        assert!(joints.iter(app.world()).all(|v| *v != Visibility::Hidden));

        // A figure drawn while they are off stays rigid.
        app.world_mut()
            .spawn(crate::character::CharacterVisual::Figure(
                FigureKind::Farach,
            ));
        app.update();
        assert_eq!(model_count(&mut app), 0);

        // On again: both standing Farachs get theirs.
        app.insert_resource(CharacterModelsShown(true));
        app.update();
        assert_eq!(model_count(&mut app), 2);
    }

    #[test]
    fn the_protagonist_wears_his_model_on_foot_but_not_in_the_saddle() {
        use crate::character::{CharacterVisual, Coat};
        let (mut app, _) = toggle_app();
        let mut models = app.world_mut().resource_mut::<CharacterModels>();
        models.0.insert(FigureKind::Protagonist, Handle::default());
        let me = app
            .world_mut()
            .spawn(CharacterVisual::Figure(FigureKind::Protagonist))
            .id();
        app.update();
        assert_eq!(model_count(&mut app), 2, "Farach and the clerk on foot");
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::HIDALGO,
            });
        app.update();
        assert_eq!(model_count(&mut app), 1, "the rider stays rigid");
        // Even when the models are turned off and on again.
        app.insert_resource(CharacterModelsShown(false));
        app.update();
        app.insert_resource(CharacterModelsShown(true));
        app.update();
        assert_eq!(model_count(&mut app), 1);
    }

    #[test]
    fn the_model_turns_to_the_bodys_front() {
        let front = model_transform().rotation * Vec3::Z;
        assert!(front.distance(Vec3::X) < 1e-5, "{front}");
    }
}
