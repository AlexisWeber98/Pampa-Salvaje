//! Las palomas de las bandadas: los modelos propios de la torcaza (*Zenaida
//! auriculata*) y del picazuró (*Patagioenas picazuro*), hechos con
//! `tools/pigeon.py` (ver `tools/README.md`), en lugar de las siluetas.
//!
//! Cada modelo es una escena glTF con skinning, de tamaño real (la torcaza de
//! 0.41 m de envergadura, el picazuró de 0.62 m), con su malla completa (unos
//! 12.000 triángulos) y su malla liviana (`Torcaza_LOD`, `Picazuro_LOD`: unos
//! 945), que se dibuja desde [`LIGHT_FROM`] metros; las bandadas vuelan a 30 m
//! o más de la cámara, así que casi siempre se dibuja la liviana. Cada paloma
//! reproduce en bucle `FlapGlide` (tres aletazos y un planeo corto, como
//! vuelan las bandadas), empezando en su propio punto del clip y a su propio
//! ritmo, así que la bandada no aletea al unísono. Las plumas de vuelo y el
//! cuerpo dejan pasar algo de la luz del sol (`FEATHER_TRANSMISSION`, el de
//! las rapaces), así que desde abajo, contra el cielo, se leen sus colores.
//! Sin el archivo de un modelo, o si no carga, sus pájaros siguen siendo
//! siluetas.

use std::collections::HashMap;

use bevy::animation::graph::AnimationNodeIndex;
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{GltfAssetLabel, GltfMaterialName, GltfMeshName};
use bevy::image::CompressedImageFormatSupport;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};

use crate::AssetRoot;
use crate::facade::materials::{MAPS, load_map_with};
use crate::fauna::livestock::{
    FEATHER_TRANSMISSION, LIGHT_SUFFIX, OWN_DIR, Skin, SkinMaps, light_range, skin_path, skinned,
};
use crate::gltf::{self, ModelState};
use crate::tree::materials::clamped;

pub use crate::fauna::livestock::LIGHT_FROM;

/// Las dos palomas de las bandadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pigeon {
    /// La torcaza, la de las grandes bandadas.
    Torcaza,
    /// El picazuró, más grande y más oscuro.
    Picazuro,
}

/// El clip que reproducen las palomas, `FlapGlide`: su índice en el GLB
/// (`tools/glbtrim.py --keep Flap Glide FlapGlide`).
pub const FLAP_GLIDE: usize = 2;

/// Hasta dónde llega el rango de cualquier malla de una paloma, en metros: más
/// allá de toda distancia a la que vuela una (la niebla la oculta mucho antes).
const FAR: f32 = 2000.0;

impl Pigeon {
    pub const ALL: [Pigeon; 2] = [Pigeon::Torcaza, Pigeon::Picazuro];

    pub fn name(self) -> &'static str {
        match self {
            Self::Torcaza => "torcaza",
            Self::Picazuro => "picazuro",
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    /// El archivo del modelo, bajo el [`AssetRoot`].
    pub fn path(self) -> String {
        format!("{OWN_DIR}/{}.glb", self.name())
    }

    /// Los mapas que viste el material `material` del modelo: el cuerpo
    /// (`Main`), y las tarjetas de plumas (`Feathers`) con alpha test, de dos
    /// capas, como las de las rapaces.
    pub fn skin(self, material: &str) -> Option<Skin> {
        let set = match (self, material) {
            (Self::Torcaza, "Main") => "torcaza_body",
            (Self::Torcaza, "Feathers") => "torcaza_feathers",
            (Self::Picazuro, "Main") => "picazuro_body",
            (Self::Picazuro, "Feathers") => "picazuro_feathers",
            _ => return None,
        };
        let cards = material == "Feathers";
        Some(Skin {
            set,
            masked: cards,
            detail: false,
            strands: false,
            layered: cards,
        })
    }
}

/// Un modelo de paloma en carga o cargado.
#[derive(Debug, Clone)]
pub struct PigeonModel {
    pub scene: Handle<WorldAsset>,
    pub clip: Handle<AnimationClip>,
    pub graph: Handle<AnimationGraph>,
    pub node: AnimationNodeIndex,
    /// Los mapas de `Main` y de `Feathers`, cuando están sus archivos.
    maps: HashMap<&'static str, SkinMaps>,
}

/// Los modelos de las palomas encontrados al iniciar, y los materiales
/// vestidos, creados una vez y compartidos por toda la bandada.
#[derive(Resource, Debug, Default)]
pub struct PigeonModels {
    models: [Option<PigeonModel>; 2],
    materials: HashMap<(Pigeon, AssetId<StandardMaterial>), Handle<StandardMaterial>>,
}

impl PigeonModels {
    pub fn get(&self, pigeon: Pigeon) -> Option<&PigeonModel> {
        self.models[pigeon.index()].as_ref()
    }
}

/// La raíz del modelo de una paloma, hija de su pájaro: dónde empieza su clip
/// (en vueltas) y a qué velocidad lo reproduce.
#[derive(Component, Debug, Clone, Copy)]
pub struct PigeonModel3d {
    pub pigeon: Pigeon,
    pub phase: f32,
    pub speed: f32,
}

/// Un modelo que todavía carga: su escena se crea cuando esté lista.
#[derive(Component, Debug, Clone)]
pub struct PigeonPending(pub Handle<WorldAsset>);

/// Un pájaro dibujado con su modelo (no con una silueta): va a tamaño real.
#[derive(Component, Debug, Clone, Copy)]
pub struct Modelled;

/// Empieza a cargar los modelos, sus mapas y su clip. Sin un servidor de
/// assets, una raíz o el plugin de animación (tests, una app vacía), ninguno:
/// las bandadas quedan en siluetas.
pub(super) fn load_pigeons(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    graphs: Option<ResMut<Assets<AnimationGraph>>>,
    support: Option<Res<CompressedImageFormatSupport>>,
) {
    let mut out = PigeonModels::default();
    if let (Some(server), Some(root), Some(mut graphs)) = (server, root, graphs) {
        let compressed = support.as_ref().is_none_or(|s| !s.0.is_empty());
        for pigeon in Pigeon::ALL {
            let path = pigeon.path();
            if !root.0.join(&path).is_file() {
                continue;
            }
            let Some(scene) = gltf::load_scene(&server, Some(&root), &path) else {
                continue;
            };
            let mut maps = HashMap::new();
            for material in ["Main", "Feathers"] {
                let Some(skin) = pigeon.skin(material) else {
                    continue;
                };
                let paths = MAPS.map(|(name, srgb)| (skin_path(skin.set, name), srgb));
                if !compressed || paths.iter().any(|(p, _)| !root.0.join(p).is_file()) {
                    warn!("pigeon maps {} not found; drawing plain colour", skin.set);
                    continue;
                }
                let [color, normal, orm] =
                    paths.map(|(path, srgb)| load_map_with(&server, &path, srgb, clamped()));
                maps.insert(skin.set, SkinMaps { color, normal, orm });
            }
            let clip = server.load(GltfAssetLabel::Animation(FLAP_GLIDE).from_asset(path));
            let (graph, node) = AnimationGraph::from_clip(clip.clone());
            out.models[pigeon.index()] = Some(PigeonModel {
                scene,
                clip,
                graph: graphs.add(graph),
                node,
                maps,
            });
        }
    }
    commands.insert_resource(out);
}

/// Crea la escena de cada modelo en espera cuando está lista. Si falló, su
/// pájaro vuelve a ser una silueta.
pub(super) fn swap_in_pigeons(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    kit: Option<Res<super::BirdKit>>,
    pending: Query<(Entity, &PigeonPending, &ChildOf)>,
) {
    let Some(server) = server else { return };
    for (entity, wait, child_of) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<PigeonPending>()
                    .try_insert(WorldAssetRoot(wait.0.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).despawn();
                if let Some(kit) = kit.as_ref() {
                    commands
                        .entity(child_of.parent())
                        .remove::<Modelled>()
                        .try_insert((
                            Mesh3d(kit.frames[super::FLAP_FRAMES].clone()),
                            MeshMaterial3d(kit.material.clone()),
                        ));
                }
            }
            ModelState::Loading => {}
        }
    }
}

/// Lo que [`dress_pigeon`] lee de los nodos de un modelo.
type Part = (
    Option<&'static Mesh3d>,
    Option<&'static MeshMaterial3d<StandardMaterial>>,
    Option<&'static GltfMaterialName>,
    Option<&'static GltfMeshName>,
    Has<AnimationPlayer>,
);

/// Prepara una paloma en el momento en que se crea su escena: cada malla en su
/// tramo de distancias (la completa cerca, la liviana desde [`LIGHT_FROM`]),
/// sin sombras, vestida con sus mapas; y su clip en bucle desde su propio
/// punto, a su propio ritmo.
#[allow(clippy::too_many_arguments)]
pub(super) fn dress_pigeon(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    mut models: Option<ResMut<PigeonModels>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    clips: Res<Assets<AnimationClip>>,
    roots: Query<&PigeonModel3d>,
    children: Query<&Children>,
    parts: Query<Part>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let root = ready.entity;
    let (Ok(&bird), Some(models)) = (roots.get(root), models.as_mut()) else {
        return;
    };
    let Some(model) = models.get(bird.pigeon).cloned() else {
        return;
    };
    let base = VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: FAR..FAR + 1.0,
        use_aabb: false,
    };
    for entity in children.iter_descendants(root) {
        let Ok((mesh, material, name, mesh_name, has_player)) = parts.get(entity) else {
            continue;
        };
        if has_player {
            let duration = clips.get(&model.clip).map_or(1.0, |c| c.duration());
            commands
                .entity(entity)
                .try_insert(AnimationGraphHandle(model.graph.clone()));
            if let Ok(mut player) = players.get_mut(entity) {
                player
                    .play(model.node)
                    .repeat()
                    .set_speed(bird.speed)
                    .seek_to(bird.phase.rem_euclid(1.0) * duration);
            }
        }
        if mesh.is_none() {
            continue;
        }
        let light = mesh_name.is_some_and(|n| n.0.ends_with(LIGHT_SUFFIX));
        let mut part = commands.entity(entity);
        part.try_insert((
            light_range(&base, light),
            NotShadowCaster,
            NotShadowReceiver,
        ));
        let skin = name.and_then(|n| bird.pigeon.skin(&n.0));
        let maps = skin.and_then(|s| model.maps.get(s.set).cloned());
        // Las tarjetas de plumas sin sus mapas serían rectángulos lisos.
        if skin.is_some_and(|s| s.masked) && maps.is_none() {
            part.try_insert(Visibility::Hidden);
            continue;
        }
        let Some(material) = material else { continue };
        let dressed = models
            .materials
            .entry((bird.pigeon, material.0.id()))
            .or_insert_with(|| {
                let mut m = materials.get(&material.0).cloned().unwrap_or_default();
                // El brillo metálico de la exportación parece plástico.
                m.metallic = 0.0;
                m.perceptual_roughness = 0.9;
                if let Some(worn) = skin.and_then(|s| skinned(&m, s, maps)) {
                    m = worn;
                }
                // El cuerpo, como las plumas de vuelo ([`skinned`]): el
                // plumaje del vientre y del pecho deja pasar algo de la luz del
                // sol, así que desde abajo, contra el cielo, se lee su color
                // (el pecho rosado de la torcaza) y no una silueta parda.
                if name.is_some_and(|n| n.0 == "Main") {
                    m.diffuse_transmission = FEATHER_TRANSMISSION;
                }
                materials.add(m)
            })
            .clone();
        part.try_insert(MeshMaterial3d(dressed));
    }
}
