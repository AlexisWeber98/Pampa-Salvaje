//! MakeHuman characters: skinned glTF people that replace a figure's rigid
//! parts (see `assets/characters/README.md` and `tools/mh2gltf.py`).
//!
//! The models are built offline from MakeHuman's CC0 data and a few
//! community assets (CC0, and CC-BY by their authors; credited in
//! `assets/characters/CREDITS.md` and the root `CREDITS.md`). Their 1919
//! wardrobe is mostly made by the converter: shirts, waistcoats, a police
//! guerrera, a three-piece suit, bombachas and knee boots cut from the
//! body and skinned like it, and the hats, kepi, neckerchiefs, faja and
//! rastra, poncho, strap, watch chain and brass buttons; and for the
//! townsfolk ([`townsfolk`](super::townsfolk)) a boina and alpargatas, and
//! the women's blouses, long skirts, aprons, crossed shawls, a headscarf and
//! hair worn up. They are drawn by default
//! ([`CharacterModelsShown`]); the debug console's `models off`
//! brings back the rigid figures of the people on foot. Riders are always
//! models: the rigid rider is gone with the procedural horse (see
//! [`rider`](super::rider)).
//!
//! A [`FigureKind`]'s model ([`model_name`]; a townsperson's by their
//! look) is `assets/characters/<name>/model.gltf`, loaded once at startup
//! ([`CharacterModels`]).
//! With the models shown, a figure of that kind on foot still spawns its
//! rigid body, joints and parts, and also a [`CharacterModel`] under its
//! body:
//!
//! - while the model loads, the rigid figure is drawn and the model waits
//!   ([`ModelPending`]);
//! - once it is ready its scene spawns, turned to face the body's front
//!   (+X; the glTF faces +Z), its meshes get the props' shared range, its
//!   `Root` node gets the shared clips' player and its bones become their
//!   targets ([`Rig`]), and the rigid parts are hidden;
//! - if it fails to load, the rigid figure stays for good.
//!
//! The body keeps turning to the heading as before, carrying the model,
//! and the clip driver ([`rig`](super::rig)) plays the shared animation
//! clips ([`clips`](super::clips)) on it: it stands, walks, jogs and talks
//! with them, and glances around like the figure.
//!
//! A rider's model has no figure: the saddled horse spawns it on its
//! `Torso` bone once its own scene has spawned ([`Riding`]), and it is
//! dressed the same, with its bones found for the seat ([`Seated`]).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};

use super::clips::CharacterClips;
use super::figure::{FigureBody, FigureJoint, FigureKind};
use super::horse::rest_affine;
use super::rider::{RiderMotion, Riding, Seated};
use super::rig::{Rig, bind_targets, rest_frame};
use super::townsfolk::{TintCache, Worn, dress_townsperson, outfit};
use crate::AssetRoot;
use crate::gltf::{self, ModelState};

/// Where the character models live, under the [`AssetRoot`].
pub const CHARACTER_DIR: &str = "characters";

/// The model folder of a figure kind: on foot (unless the models are off)
/// and in the saddle. A townsperson's is their look's outfit
/// ([`townsfolk`](super::townsfolk)).
pub fn model_name(kind: FigureKind) -> Option<&'static str> {
    match kind {
        FigureKind::Protagonist => Some("protagonist"),
        FigureKind::Hidalgo => Some("hidalgo"),
        FigureKind::Farach => Some("farach"),
        FigureKind::Bairoletto => Some("bairoletto"),
        FigureKind::Townsfolk(look) => Some(outfit(look)),
    }
}

/// The model file of `name` under the [`CHARACTER_DIR`].
pub fn model_path(name: &str) -> String {
    format!("{CHARACTER_DIR}/{name}/model.gltf")
}

/// The character models found at startup, loading or loaded, by model
/// folder: each is loaded once, whoever wears it.
#[derive(Resource, Debug, Clone, Default)]
pub struct CharacterModels(pub HashMap<&'static str, Handle<WorldAsset>>);

impl CharacterModels {
    /// The model `kind` is drawn with, if there is one.
    pub fn of(&self, kind: FigureKind) -> Option<&Handle<WorldAsset>> {
        self.0.get(model_name(kind)?)
    }
}

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
            if models.0.contains_key(name) {
                continue;
            }
            if let Some(scene) = gltf::load_scene(&server, Some(&root), &model_path(name)) {
                info!("character model `{name}`");
                models.0.insert(name, scene);
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
    let Some(scene) = models.and_then(|m| m.of(kind)) else {
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
/// range on its meshes, a townsperson's clothes tinted (and, in a long
/// skirt, the arms to hold out found), its `Root` made the clips' player with every bone
/// a target ([`bind_targets`]), its rig found for the clip driver, and its
/// figure's rigid parts hidden. A rider's bones are found for the seat
/// ([`Seated`]) instead of the pelvis fit and the glance.
#[allow(clippy::too_many_arguments)]
pub(super) fn dress_character(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    clips: Option<Res<CharacterClips>>,
    models: Query<(&ChildOf, Option<&Riding>), With<CharacterModel>>,
    children: Query<&Children>,
    names: Query<&Name>,
    (meshes, worn, figures): (Query<(), With<Mesh3d>>, Query<Worn>, Query<&FigureBody>),
    (cache, materials): (
        Option<ResMut<TintCache>>,
        Option<ResMut<Assets<StandardMaterial>>>,
    ),
    rigid: Query<(), With<FigureJoint>>,
    transforms: Query<&Transform>,
    parents: Query<&ChildOf>,
) {
    let root = ready.entity;
    let Ok((body, riding)) = models.get(root) else {
        return;
    };
    let kind = riding
        .map(|r| r.kind)
        .or_else(|| figures.get(body.parent()).ok().map(|f| f.kind));
    let riding = riding.is_some();
    let range = model_range();
    let named = |name: &str| {
        children
            .iter_descendants(root)
            .find(|e| names.get(*e).is_ok_and(|n| n.as_str() == name))
    };
    let mut probes = Vec::new();
    for entity in children.iter_descendants(root) {
        if meshes.contains(entity) {
            commands.entity(entity).try_insert(range.clone());
            probes.push(entity);
        }
    }
    // A townsperson's clothes in their own colours; a long skirt holds the
    // arms out a little.
    let skirt = match (kind, cache, materials) {
        (Some(kind), Some(mut cache), Some(mut materials)) => dress_townsperson(
            &mut commands,
            (&mut cache, &mut materials),
            kind,
            probes
                .iter()
                .filter_map(|&e| worn.get(e).ok().map(|w| (e, w))),
        ),
        _ => false,
    };
    if let Some(skeleton) = named("Root") {
        bind_targets(&mut commands, skeleton, &names, &children);
        if let Some(clips) = clips.as_deref() {
            commands
                .entity(skeleton)
                .try_insert(AnimationGraphHandle(clips.graph.clone()));
        }
        let mut rig = Rig::new(skeleton);
        rig.probes = probes;
        if riding {
            let rest = |e: Entity| rest_affine(e, root, &parents, &transforms);
            match Seated::find(root, named, rest) {
                Some(seated) => {
                    // Hidden until posed in the saddle (again, if its scene
                    // was spawned again): it spawns standing at rest.
                    commands.entity(root).try_insert((
                        seated,
                        RiderMotion::default(),
                        Visibility::Hidden,
                    ));
                }
                None => warn!("rider model without the whole game_engine skeleton"),
            }
            commands.entity(root).try_insert(rig);
            return;
        }
        let at = |name: &str| named(name).and_then(|e| transforms.get(e).ok().map(|t| (e, t)));
        if let (Some((pelvis, rest)), Some((_, knee)), Some((_, ankle))) =
            (at("pelvis"), at("calf_l"), at("foot_l"))
        {
            let leg = knee.translation.length() + ankle.translation.length();
            rig.pelvis = Some((pelvis, rest.translation, leg));
        }
        for (k, name) in ["neck_01", "head"].into_iter().enumerate() {
            rig.look[k] = named(name).map(|e| (e, rest_frame(e, root, &parents, &transforms)));
        }
        if skirt {
            for (k, name) in ["upperarm_l", "upperarm_r"].into_iter().enumerate() {
                rig.arms[k] = named(name).map(|e| (e, rest_frame(e, root, &parents, &transforms)));
            }
        }
        if rig.pelvis.is_none() || rig.look.iter().any(Option::is_none) {
            warn!("character model without the whole game_engine skeleton: it may not move right");
        }
        commands.entity(root).try_insert(rig);
    } else {
        warn!("character model without a `Root` skeleton: it stands still");
    }
    for &child in children.get(body.parent()).into_iter().flatten() {
        if rigid.contains(child) {
            commands.entity(child).try_insert(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::gltf::tests::root;
    use bevy::mesh::Mesh;

    /// The triangle budget of one character model.
    const BUDGET: usize = 15_000;
    /// The cap of one townsperson's model (user, 2026-10-25): 31 of them
    /// may stand in a plaza.
    const TOWNSFOLK_BUDGET: usize = 11_000;
    /// Fewer triangles than this means a model lost its body or clothes
    /// (the lightest, with its garments cut from the body, has about 4,900).
    const FLOOR: usize = 3_000;

    /// The shipped glTF of the model folder `name`, its JSON only: what
    /// the file says, without loading its meshes.
    pub(crate) fn shipped_model(name: &str) -> bevy::gltf::gltf::Document {
        let bytes = std::fs::read(root().0.join(model_path(name))).unwrap();
        bevy::gltf::gltf::Gltf::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .document
    }

    #[test]
    fn every_character_model_is_skinned_within_the_triangle_budget() {
        use bevy::gltf::gltf::Semantic;
        let townsfolk = crate::character::townsfolk::OUTFITS;
        for name in ["hidalgo", "farach", "bairoletto", "protagonist"]
            .into_iter()
            .chain(townsfolk)
        {
            let model = shipped_model(name);
            assert_eq!(model.skins().count(), 1, "{name}");
            let mut triangles = 0;
            for primitive in model.meshes().flat_map(|m| m.primitives()) {
                assert!(primitive.get(&Semantic::Joints(0)).is_some(), "{name}");
                assert!(primitive.get(&Semantic::Weights(0)).is_some(), "{name}");
                triangles += match primitive.indices() {
                    Some(indices) => indices.count() / 3,
                    None => primitive.get(&Semantic::Positions).unwrap().count() / 3,
                };
            }
            let budget = if townsfolk.contains(&name) {
                TOWNSFOLK_BUDGET
            } else {
                BUDGET
            };
            println!("{name}: {triangles} triangles");
            assert!(
                (FLOOR..=budget).contains(&triangles),
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
        models.0.insert("farach", Handle::default());
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
    fn the_toggle_puts_models_on_people_on_foot_and_leaves_riders_to_their_horse() {
        use crate::character::{CharacterVisual, Coat};
        let (mut app, _) = toggle_app();
        let mut models = app.world_mut().resource_mut::<CharacterModels>();
        models.0.insert("protagonist", Handle::default());
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
        // His figure and its model go; his rider's model waits for the
        // horse's scene (none in this app).
        assert_eq!(
            model_count(&mut app),
            1,
            "no model on a figure in the saddle"
        );
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
