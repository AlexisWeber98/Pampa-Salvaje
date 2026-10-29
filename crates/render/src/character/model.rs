//! Personajes de MakeHuman: personas glTF con skinning que reemplazan las
//! piezas rígidas de una figura (ver `assets/characters/README.md` y
//! `tools/mh2gltf.py`).
//!
//! Los modelos se construyen offline a partir de los datos CC0 de MakeHuman y
//! de algunos assets de la comunidad (CC0 y CC-BY de sus autores; con crédito
//! en `assets/characters/CREDITS.md` y en el `CREDITS.md` de la raíz). Su
//! vestuario de 1919 lo hace en su mayor parte el conversor: camisas, chalecos,
//! una guerrera de policía, un traje de tres piezas, bombachas y botas altas
//! cortadas a partir del cuerpo y con skinning como él, y los sombreros, el
//! quepí, los pañuelos de cuello, la faja y la rastra, el poncho, la correa, la
//! cadena del reloj y los botones de bronce; y para los vecinos
//! ([`townsfolk`](super::townsfolk)) una boina y alpargatas, y para las mujeres
//! blusas, polleras largas, delantales, chales cruzados, un pañuelo en la
//! cabeza y el pelo recogido. Se dibujan por defecto
//! ([`CharacterModelsShown`]); el `models off` de la consola de depuración
//! vuelve a traer las figuras rígidas de las personas a pie. Los jinetes
//! siempre son modelos: el jinete rígido se fue junto con el caballo procedural
//! (ver [`rider`](super::rider)).
//!
//! El modelo de un [`FigureKind`] ([`model_name`]; el de un vecino según su
//! aspecto) es `assets/characters/<name>/model.gltf`, que se carga una vez al
//! iniciar ([`CharacterModels`]). Con los modelos visibles, una figura de ese
//! tipo a pie igual crea su cuerpo rígido, sus articulaciones y sus piezas, y
//! además un [`CharacterModel`] bajo su cuerpo:
//!
//! - mientras el modelo carga, se dibuja la figura rígida y el modelo espera
//!   ([`ModelPending`]);
//! - cuando está listo, se crea su escena, girada para mirar hacia el frente
//!   del cuerpo (+X; el glTF mira hacia +Z), sus meshes reciben el rango
//!   compartido de los accesorios, su nodo `Root` recibe el reproductor de los
//!   clips compartidos y sus huesos pasan a ser sus objetivos ([`Rig`]), y las
//!   piezas rígidas se ocultan;
//! - si falla la carga, la figura rígida queda para siempre.
//!
//! El cuerpo sigue girando hacia el rumbo como antes, llevando el modelo, y el
//! controlador de clips ([`rig`](super::rig)) reproduce sobre él los clips de
//! animación compartidos ([`clips`](super::clips)): con ellos se queda parado,
//! camina, trota y habla, y mira a su alrededor como la figura.
//!
//! El modelo de un jinete no tiene figura: el caballo ensillado lo crea sobre
//! su hueso `Torso` una vez que se creó su propia escena ([`Riding`]), y se
//! prepara igual, con sus huesos ubicados para la montura ([`Seated`]).

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

/// Dónde viven los modelos de personaje, bajo el [`AssetRoot`].
pub const CHARACTER_DIR: &str = "characters";

/// La carpeta del modelo de un tipo de figura: a pie (salvo que los modelos
/// estén desactivados) y en la montura. La de un vecino es el atuendo de su
/// aspecto ([`townsfolk`](super::townsfolk)).
pub fn model_name(kind: FigureKind) -> Option<&'static str> {
    match kind {
        FigureKind::Protagonist => Some("protagonist"),
        FigureKind::Hidalgo => Some("hidalgo"),
        FigureKind::Farach => Some("farach"),
        FigureKind::Bairoletto => Some("bairoletto"),
        FigureKind::Townsfolk(look) => Some(outfit(look)),
    }
}

/// El archivo del modelo `name` bajo el [`CHARACTER_DIR`].
pub fn model_path(name: &str) -> String {
    format!("{CHARACTER_DIR}/{name}/model.gltf")
}

/// Los modelos de personaje encontrados al iniciar, cargando o ya cargados, por
/// carpeta de modelo: cada uno se carga una sola vez, lo use quien lo use.
#[derive(Resource, Debug, Clone, Default)]
pub struct CharacterModels(pub HashMap<&'static str, Handle<WorldAsset>>);

impl CharacterModels {
    /// El modelo con el que se dibuja `kind`, si hay uno.
    pub fn of(&self, kind: FigureKind) -> Option<&Handle<WorldAsset>> {
        self.0.get(model_name(kind)?)
    }
}

/// Empieza a cargar todos los modelos de personaje que haya. Sin un servidor de
/// assets o una raíz (tests, una app vacía) no se carga ninguno y todas las
/// figuras quedan rígidas.
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

/// Si los modelos de personaje se dibujan en lugar de las figuras rígidas
/// (activado por defecto). Activarlo o desactivarlo vuelve a dibujar las
/// figuras que están en pie.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharacterModelsShown(pub bool);

impl Default for CharacterModelsShown {
    fn default() -> Self {
        Self(true)
    }
}

/// La raíz de un modelo de personaje, hija del cuerpo de su figura.
#[derive(Component, Debug, Clone, Copy)]
pub struct CharacterModel;

/// Un modelo de personaje que todavía carga: su escena se crea aquí cuando esté
/// listo.
#[derive(Component, Debug, Clone)]
pub struct ModelPending(pub Handle<WorldAsset>);

/// El transform raíz del modelo bajo el cuerpo: el glTF mira hacia +Z y el
/// frente del cuerpo es +X.
pub fn model_transform() -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(FRAC_PI_2))
}

/// Pone el modelo de `kind`, si tiene uno, bajo la figura `body`.
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

/// Pone los modelos en las figuras que ya están en pie (o los quita) cuando
/// cambia [`CharacterModelsShown`]; al quitarlos, sus piezas rígidas vuelven a
/// verse.
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

/// Crea la escena de cada modelo en espera cuando está listo; descarta uno que
/// falló, así que su figura queda rígida.
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

/// El rango dentro del cual se dibujan las meshes de un modelo de personaje: el
/// compartido de los accesorios de los asentamientos (sin un slot de GPU
/// nuevo).
pub fn model_range() -> VisibilityRange {
    crate::settlement::ranges()[1].clone()
}

/// Prepara un modelo de personaje en el momento en que se crea su escena: el
/// rango compartido en sus meshes, la ropa de un vecino teñida (y, con pollera
/// larga, los brazos a separar ubicados), su `Root` convertido en el
/// reproductor de los clips con cada hueso como objetivo ([`bind_targets`]), su
/// rig ubicado para el controlador de clips y las piezas rígidas de su figura
/// ocultas. Los huesos de un jinete se ubican para la montura ([`Seated`]) en
/// lugar del ajuste de la pelvis y la mirada.
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
    // La ropa de un vecino en sus propios colores; una pollera larga separa un
    // poco los brazos.
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
                    // Oculto hasta que se lo pone en pose en la montura (otra
                    // vez, si su escena se volvió a crear): se crea parado en
                    // reposo.
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

    /// El presupuesto de triángulos de un modelo de personaje.
    const BUDGET: usize = 15_000;
    /// El tope del modelo de un vecino (usuario, 2026-10-25): puede haber 31 de
    /// ellos parados en una plaza.
    const TOWNSFOLK_BUDGET: usize = 11_000;
    /// Menos triángulos que esto significa que un modelo perdió su cuerpo o su
    /// ropa (el más liviano, con sus prendas cortadas a partir del cuerpo,
    /// tiene unos 4,900).
    const FLOOR: usize = 3_000;

    /// El glTF incluido de la carpeta de modelo `name`, solo su JSON: lo que
    /// dice el archivo, sin cargar sus meshes.
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

    /// El plugin de personajes, headless, con un modelo sustituto para Farach
    /// (que nunca se carga: no hay servidor de assets), y Farach parado.
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
        // Farach, parado, tiene su modelo; el vecino no tiene ninguno.
        assert_eq!(model_count(&mut app), 1);
        app.update();
        assert_eq!(model_count(&mut app), 1, "attached once");

        // Desactivados: los modelos se van y se ven las piezas rígidas.
        app.insert_resource(CharacterModelsShown(false));
        app.update();
        assert_eq!(model_count(&mut app), 0);
        let mut joints = app
            .world_mut()
            .query_filtered::<&Visibility, With<FigureJoint>>();
        assert!(joints.iter(app.world()).all(|v| *v != Visibility::Hidden));

        // Una figura dibujada mientras están desactivados queda rígida.
        app.world_mut()
            .spawn(crate::character::CharacterVisual::Figure(
                FigureKind::Farach,
            ));
        app.update();
        assert_eq!(model_count(&mut app), 0);

        // Activados otra vez: los dos Farach parados reciben el suyo.
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
        // Su figura y su modelo se van; el modelo de su jinete espera la escena
        // del caballo (ninguna en esta app).
        assert_eq!(
            model_count(&mut app),
            1,
            "no model on a figure in the saddle"
        );
        // Incluso cuando los modelos se desactivan y se vuelven a activar.
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
