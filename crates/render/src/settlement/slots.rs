//! Slots de modelos: modelos glTF de terceros que reemplazan la mesh procedural
//! de un tipo de estructura, y las escenas que los asentamientos crean a partir
//! de ellos.
//!
//! Cada tipo con un [`StructureKind::slot`] busca
//! `assets/models/third-party/<slot>/model.glb` (o `model.gltf`) al iniciar
//! ([`SlotModels`]). Donde se encuentra uno, una estructura de ese tipo crea la
//! escena del modelo, ajustada por su [`SlotFit`] (de `slots.ron`, que carga el
//! juego), en lugar de su mesh procedural:
//!
//! - mientras el modelo carga, la mesh procedural lo sustituye ([`StandIn`]) y
//!   la escena espera ([`SlotPending`]);
//! - cuando está listo, se crea la escena, sus meshes reciben el rango
//!   compartido y el fundido de la estructura ([`SceneDress`]), su nodo de
//!   rotor gira ([`ModelRotor`]) y el sustituto desaparece;
//! - si falla la carga, el sustituto queda para siempre.
//!
//! Los obstáculos y los oclusores de cámara vienen de `pampa-core` y no
//! cambian.

use std::collections::HashMap;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::StructureKind;

use super::SettlementPart;
use crate::AssetRoot;
use crate::flora::ThinOccluder;
use crate::gltf::{self, ModelState};

/// Dónde viven los slots de modelos, bajo el [`AssetRoot`].
pub const SLOT_DIR: &str = "models/third-party";

/// Cómo se ubica el modelo de un slot en el espacio propio de su estructura:
/// escalado, girado alrededor de la vertical `yaw` radianes, y después
/// desplazado `offset` metros.
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
    /// El transform del modelo en el espacio propio de su estructura.
    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.offset)
            .with_rotation(Quat::from_rotation_y(self.yaw))
            .with_scale(Vec3::splat(self.scale))
    }
}

/// La parte giratoria de un modelo: su nodo glTF y el eje, en el espacio propio
/// del nodo, alrededor del cual gira.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotRotor {
    pub node: String,
    pub axis: Dir3,
}

/// Los ajustes de los slots de modelos, por nombre de slot. Se insertan antes
/// de que arranque el [`super::SettlementPlugin`]; un slot sin ajuste se dibuja
/// tal como viene su modelo.
#[derive(Resource, Debug, Clone, Default)]
pub struct ModelSlots(pub HashMap<String, SlotFit>);

impl ModelSlots {
    /// El ajuste de `slot`, o la identidad.
    pub fn fit(&self, slot: &str) -> SlotFit {
        self.0.get(slot).cloned().unwrap_or_default()
    }
}

/// La carpeta, bajo el [`AssetRoot`], en la que se buscan los slots
/// ([`SLOT_DIR`] salvo que un test apunte a otro lugar).
#[derive(Resource, Debug, Clone)]
pub struct SlotFolder(pub String);

impl Default for SlotFolder {
    fn default() -> Self {
        Self(SLOT_DIR.to_owned())
    }
}

/// Los modelos de slot encontrados al iniciar, cargando o ya cargados, por
/// nombre de slot.
#[derive(Resource, Debug, Clone, Default)]
pub struct SlotModels(pub HashMap<&'static str, Handle<WorldAsset>>);

/// El archivo del modelo de `slot` bajo `folder`, si lo hay: primero
/// `model.glb`, después `model.gltf`.
pub fn slot_path(root: &AssetRoot, folder: &str, slot: &str) -> Option<String> {
    ["glb", "gltf"]
        .into_iter()
        .map(|ext| format!("{folder}/{slot}/model.{ext}"))
        .find(|path| gltf::is_inside(path) && root.0.join(path).is_file())
}

/// Empieza a cargar cada slot que tiene un modelo. Sin un servidor de assets o
/// una raíz (tests, una app vacía) no se carga ninguno y todo es procedural.
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

/// Lo que reciben las meshes de una escena de modelo creada: el rango
/// compartido de la estructura, su forma de fundido, el giro de su nodo de
/// rotor y si hay que corregir el vidrio de sus materiales translúcidos (ver
/// [`dress_scene`]).
#[derive(Component, Clone)]
pub struct SceneDress {
    pub range: VisibilityRange,
    pub fade: Option<ThinOccluder>,
    /// El nodo del rotor y su velocidad en radianes por segundo.
    pub rotor: Option<(SlotRotor, f32)>,
    pub glass: bool,
}

/// Un modelo de slot que todavía carga: su escena se crea aquí cuando esté
/// listo.
#[derive(Component, Debug, Clone)]
pub struct SlotPending(pub Handle<WorldAsset>);

/// Una pieza procedural que se dibuja mientras carga el modelo de slot de su
/// estructura, y se elimina cuando la escena del modelo está lista.
#[derive(Component, Debug, Clone, Copy)]
pub struct StandIn;

/// Un nodo de un modelo que gira alrededor de `axis` (en su propio espacio),
/// como la rueda de un molino.
#[derive(Component, Debug, Clone, Copy)]
pub struct ModelRotor {
    pub axis: Dir3,
    /// Radianes por segundo.
    pub speed: f32,
}

/// Crea la escena de cada modelo de slot en espera en cuanto su modelo está
/// listo. Si falló, abandona la espera y conserva el sustituto para siempre
/// (que ya no es un sustituto).
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

/// El aspecto translúcido que se le da al vidrio con mezcla de un glTF cuya
/// textura de color base no tiene alfa: qué tan opaco es.
pub const GLASS_ALPHA: f32 = 0.3;

/// Lo que [`dress_scene`] lee de las entidades de una escena creada.
type SceneParts<'a> = (
    Option<&'a Name>,
    Has<Mesh3d>,
    Option<&'a MeshMaterial3d<StandardMaterial>>,
);

/// Prepara una escena de modelo en el momento en que se crea: cada mesh recibe
/// el rango compartido (y la forma de fundido), el nodo de rotor gira, el
/// vidrio con mezcla se vuelve translúcido y el sustituto de la estructura
/// desaparece.
///
/// Vidrio: un material glTF en modo `BLEND` cuyo color base es un JPEG opaco se
/// multiplica hasta alfa 1, así que se dibuja sólido. El alfa de su color base
/// se fija en [`GLASS_ALPHA`] y se lo vuelve liso, para que los vidrios del
/// farol se vean como vidrio. El material es el propio del glTF, compartido por
/// todas las instancias, y el cambio es idempotente.
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
    // El nodo en sí, no sus primitivas de mesh (que pueden compartir su
    // nombre).
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

/// Hace girar cada [`ModelRotor`].
pub(super) fn spin_model_rotors(time: Res<Time>, mut rotors: Query<(&ModelRotor, &mut Transform)>) {
    for (rotor, mut transform) in &mut rotors {
        transform.rotate_local_axis(rotor.axis, rotor.speed * time.delta_secs());
    }
}
