//! Los modelos de los vehículos, hechos en Blender por script
//! (`tools/sulky.py`, `tools/carreton.py`, `tools/carreta.py`,
//! `tools/ford_t.py`; el gig, la otra variante del sulky, de un modelo de
//! Sketchfab con `tools/sulky_gig.py`), sus arreos
//! (`tools/vehicle_harness.py`) y el buey de la carreta (`tools/cattle.py
//! --ox`).
//!
//! - Cada vehículo es una escena glTF colgada del cuerpo del vehículo, girada
//!   de su +Z (su frente en el glTF) al +X del cuerpo. Sus nodos `Wheel.*`
//!   reciben un [`VehicleWheel`] cuando se crea la escena, y giran sobre su X
//!   con lo rodado. Sus materiales llevan el nombre de su conjunto de mapas
//!   (`textures/vehicles/<conjunto>/`, KTX2) y se cambian por los propios
//!   ([`VehicleMaterials`]); su malla lejana (`*_LOD`) se dibuja desde
//!   [`LOD_NEAR`].
//! - Los arreos de un caballo de tiro son mallas con skinning sobre un
//!   esqueleto con los nombres del de henry, en reposo parado como al empezar
//!   `Idle_1`: cada una se pone sobre las articulaciones del caballo del mismo
//!   nombre con las matrices de bind inversas de su propio archivo, así que se
//!   dobla con él en cada clip. El yugo del buey de la izquierda es una pieza
//!   rígida `<pieza>@<hueso>` con sus vértices en el espacio del modelo en su
//!   pose de bind: cuelga del hueso por su matriz de bind inversa
//!   ([`dress_animals`]).
//! - El buey anda con el clip `Walk` llevado por lo rodado, así que sus patas
//!   pisan al paso del vehículo y los dos de la yunta van iguales; parado,
//!   rumia con `Idle` ([`walk_oxen`]).

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{Gltf, GltfMaterial, GltfMaterialName, GltfMesh, GltfNode, GltfSkin};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::Vehicle;

use super::{Rolled, Variant, VehicleHorse, VehicleSeat, VehicleWheel};
use crate::AssetRoot;
use crate::character::rider::PALM;
use crate::character::tack::{SAG_PARTS, Strap, StrapPart, strap_mesh};
use crate::facade::materials::{load_map_with, repeating};
use crate::fauna::livestock::CATTLE_WALK_SPEED;
use crate::gltf::{self, ModelState};
use crate::tree::materials::clamped;

/// Dónde están los modelos de los vehículos, bajo el [`AssetRoot`].
pub const VEHICLES_DIR: &str = "models/vehicles";
/// Dónde están sus conjuntos de mapas.
pub const TEXTURES_DIR: &str = "textures/vehicles";
/// Desde esta distancia de la cámara se dibuja la malla liviana, en metros.
pub const LOD_NEAR: f32 = 45.0;
/// Escala del buey: la del toro del juego (1.55 m a la cruz sobre los 1.40
/// del modelo, como `Species::Bull`), la misma con la que se ajustó el yugo.
pub const OX_SCALE: f32 = 1.55 / 1.40;

/// El archivo del modelo `variant` de `vehicle` (solo el sulky tiene el gig).
pub fn model_path(vehicle: Vehicle, variant: Variant) -> String {
    let name = match vehicle {
        Vehicle::Sulky if variant == Variant::Gig => "sulky_gig",
        Vehicle::Sulky => "sulky",
        Vehicle::Cart => "carreton",
        Vehicle::Carreta => "carreta",
        Vehicle::FordT => "ford_t",
    };
    format!("{VEHICLES_DIR}/{name}.glb")
}

/// El conjunto de mapas del material `name` de un vehículo o de un arreo, y
/// cuánto metal tiene. `Glass` no tiene.
pub fn material_set(name: &str) -> Option<(&'static str, f32)> {
    Some(match name {
        "Wood" => ("veh_wood", 0.0),
        "Paint_Black" => ("veh_paint_black", 0.0),
        "Paint_Red" => ("veh_paint_red", 0.0),
        "Paint_Green" => ("veh_paint_green", 0.0),
        "Paint_Yellow" => ("veh_paint_yellow", 0.0),
        "Iron" => ("veh_iron", 0.7),
        "Hide" => ("veh_hide", 0.0),
        "Cane" => ("veh_cane", 0.0),
        "Leather" => ("veh_leather", 0.0),
        "Brass" => ("veh_brass", 1.0),
        "Rubber" => ("veh_rubber", 0.0),
        // El gig (`tools/sulky_gig.py` `MATERIALS`): los mapas de su modelo.
        "Gig_Paint" => ("gig_paint", 0.0),
        "Gig_Planks" => ("gig_planks", 0.0),
        "Gig_Iron" => ("gig_iron", 0.7),
        "Gig_Steel" => ("gig_steel", 0.7),
        "Gig_Hub" => ("gig_hub", 0.0),
        "Gig_Leather" => ("gig_leather", 0.0),
        "Gig_Hide" => ("gig_hide", 0.0),
        "Gig_Wood" => ("gig_wood", 0.0),
        "Gig_Floor" => ("gig_floor", 0.0),
        _ => return None,
    })
}

/// La rueda llamada `node` de `vehicle`, con su radio, si es una.
pub fn wheel_radius(vehicle: Vehicle, node: &str) -> Option<f32> {
    let radius = match (vehicle, node) {
        (Vehicle::Sulky, "Wheel.L" | "Wheel.R") => 0.70,
        (Vehicle::Carreta, "Wheel.L" | "Wheel.R") => 1.10,
        (Vehicle::Cart, "Wheel.FL" | "Wheel.FR") => 0.50,
        (Vehicle::Cart, "Wheel.RL" | "Wheel.RR") => 0.66,
        (Vehicle::FordT, "Wheel.FL" | "Wheel.FR" | "Wheel.RL" | "Wheel.RR") => 0.38,
        _ => return None,
    };
    Some(radius)
}

/// El giro de una rueda de `radius` metros que lleva `rolled` metros rodados,
/// sobre su eje (la X del glTF): la parte de arriba avanza hacia el frente.
pub fn wheel_turn(rolled: f32, radius: f32) -> Quat {
    Quat::from_rotation_x(rolled / radius)
}

/// El buey: su escena, su archivo y sus clips una vez cargados.
#[derive(Debug, Clone)]
pub struct OxModel {
    pub file: Handle<Gltf>,
    pub scene: Handle<WorldAsset>,
    /// El grafo con `Walk` e `Idle`, armado cuando carga el archivo.
    pub graph: Option<(
        Handle<AnimationGraph>,
        AnimationNodeIndex,
        AnimationNodeIndex,
        f32,
    )>,
}

/// Los modelos de los vehículos encontrados al iniciar (sin ellos, un vehículo
/// se dibuja sin cuerpo: sus caballos y su conductor siguen ahí).
#[derive(Resource, Debug, Clone, Default)]
pub struct VehicleModels {
    pub scenes: HashMap<(Vehicle, Variant), Handle<WorldAsset>>,
    pub harness: Option<Handle<Gltf>>,
    pub yoke: Option<Handle<Gltf>>,
    pub ox: Option<OxModel>,
}

/// Los materiales propios de los vehículos y los arreos, por nombre de
/// material del glTF; y los del buey.
#[derive(Resource, Debug, Clone, Default)]
pub struct VehicleMaterials {
    pub by_name: HashMap<String, Handle<StandardMaterial>>,
    pub ox: HashMap<String, Handle<StandardMaterial>>,
    ox_maps: Option<[Handle<Image>; 4]>,
}

/// La escena de un vehículo, colgada de su cuerpo, para `vehicle`.
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleModel(pub Vehicle);

/// Un modelo cuya escena todavía carga.
#[derive(Component, Debug, Clone)]
pub struct ModelLoading(pub Handle<WorldAsset>);

/// Un buey de la yunta de una carreta: `left` lleva el yugo.
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleOx {
    pub left: bool,
}

/// Qué vehículo mueve una rueda o un buey.
#[derive(Component, Debug, Clone, Copy)]
pub struct OfVehicle(pub Entity);

/// El nodo de la malla de un animal que ya lleva sus arreos (o su yugo): en la
/// escena del animal, así que una escena creada de nuevo se viste otra vez.
#[derive(Component, Debug, Clone, Copy)]
pub struct Dressed;

/// On a draught horse or an ox: the mesh node [`dress_animals`] last dressed
/// (marked [`Dressed`]). While that node is still dressed the animal is not
/// walked again; once its scene is created again (the node is gone) it is.
#[derive(Component, Debug, Clone, Copy)]
pub struct DressedOn(pub Entity);

/// El reproductor de un buey, una vez creada su escena.
#[derive(Component, Debug, Clone, Copy)]
pub struct OxRig {
    pub player: Entity,
}

/// Carga los modelos, los arreos, el buey y sus materiales.
pub fn load_vehicle_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let mut models = VehicleModels::default();
    let mut mats = VehicleMaterials::default();
    if let (Some(server), Some(root), Some(materials)) = (server, root, materials.as_mut()) {
        for key @ (vehicle, variant) in [
            (Vehicle::Sulky, Variant::Own),
            (Vehicle::Sulky, Variant::Gig),
            (Vehicle::Cart, Variant::Own),
            (Vehicle::Carreta, Variant::Own),
            (Vehicle::FordT, Variant::Own),
        ] {
            let path = model_path(vehicle, variant);
            if let Some(scene) = gltf::load_scene(&server, Some(&root), &path) {
                models.scenes.insert(key, scene);
            }
        }
        let load = |name: &str| {
            let path = format!("{VEHICLES_DIR}/{name}.glb");
            root.0.join(&path).is_file().then(|| server.load(path))
        };
        models.harness = load("harness");
        models.yoke = load("yoke");
        let ox_path = format!("{VEHICLES_DIR}/ox.glb");
        if let (Some(file), Some(scene)) =
            (load("ox"), gltf::load_scene(&server, Some(&root), &ox_path))
        {
            models.ox = Some(OxModel {
                file,
                scene,
                graph: None,
            });
        }
        for name in [
            "Wood",
            "Paint_Black",
            "Paint_Red",
            "Paint_Green",
            "Paint_Yellow",
            "Iron",
            "Hide",
            "Cane",
            "Leather",
            "Brass",
            "Rubber",
            "Gig_Paint",
            "Gig_Planks",
            "Gig_Iron",
            "Gig_Steel",
            "Gig_Hub",
            "Gig_Leather",
            "Gig_Hide",
            "Gig_Wood",
            "Gig_Floor",
        ] {
            let (set, metal) = material_set(name).expect("a set");
            let path = |map: &str| format!("{TEXTURES_DIR}/{set}/{map}.ktx2");
            if !root.0.join(path("color")).is_file() {
                warn!("vehicle maps {set} not found; drawing plain colour");
                continue;
            }
            let [color, normal, orm] = [("color", true), ("normal_gl", false), ("orm", false)]
                .map(|(map, srgb)| load_map_with(&server, &path(map), srgb, repeating()));
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(color),
                normal_map_texture: Some(normal),
                metallic_roughness_texture: Some(orm.clone()),
                occlusion_texture: Some(orm),
                metallic: metal,
                perceptual_roughness: 1.0,
                reflectance: 0.35,
                ..default()
            });
            mats.by_name.insert(name.to_string(), material);
        }
        mats.by_name.insert(
            "Glass".into(),
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.55, 0.6, 0.6, 0.25),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.05,
                reflectance: 0.6,
                ..default()
            }),
        );
        let ox_map = |set: &str, map: &str, srgb: bool| {
            let path = format!("{VEHICLES_DIR}/{set}/{map}.ktx2");
            root.0
                .join(&path)
                .is_file()
                .then(|| load_map_with(&server, &path, srgb, clamped()))
        };
        if let (Some(coat), Some(lum), Some(normal), Some(orm)) = (
            ox_map("ox_body", "hosco", true),
            ox_map("ox_body", "color", true),
            ox_map("ox_body", "normal_gl", false),
            ox_map("ox_body", "orm", false),
        ) {
            mats.ox_maps = Some([coat, lum, normal, orm]);
            if let (Some(hair), Some(hn), Some(ho)) = (
                ox_map("ox_hair", "color", true),
                ox_map("ox_hair", "normal_gl", false),
                ox_map("ox_hair", "orm", false),
            ) {
                let m = materials.add(StandardMaterial {
                    base_color_texture: Some(hair),
                    normal_map_texture: Some(hn),
                    metallic_roughness_texture: Some(ho.clone()),
                    occlusion_texture: Some(ho),
                    alpha_mode: AlphaMode::Mask(0.5),
                    cull_mode: None,
                    double_sided: true,
                    perceptual_roughness: 1.0,
                    ..default()
                });
                mats.ox.insert("Hair".into(), m);
            }
        }
    }
    commands.insert_resource(models);
    commands.insert_resource(mats);
}

/// Crea la escena de cada modelo en espera cuando está lista; descarta uno
/// que falló.
pub fn swap_in_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    pending: Query<(Entity, &ModelLoading)>,
) {
    let Some(server) = server else { return };
    for (entity, wait) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<ModelLoading>()
                    .try_insert(WorldAssetRoot(wait.0.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).remove::<ModelLoading>();
            }
            ModelState::Loading => {}
        }
    }
}

/// Las partes de una escena recién creada que se preparan.
type Part<'a> = (
    Option<&'a Name>,
    Option<&'a GltfMaterialName>,
    Option<&'a MeshMaterial3d<StandardMaterial>>,
    Has<Mesh3d>,
    Has<AnimationPlayer>,
);

/// Prepara un vehículo o un buey en el momento en que se crea su escena: sus
/// ruedas, sus materiales propios, el tramo de distancias de cada malla y,
/// en un buey, su reproductor.
#[allow(clippy::too_many_arguments)]
pub fn dress_model(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    vehicle_models: Query<(&VehicleModel, &ChildOf)>,
    oxen: Query<&ChildOf, With<VehicleOx>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    parts: Query<Part>,
    mats: Option<Res<VehicleMaterials>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    let root = ready.entity;
    let model = vehicle_models.get(root).ok();
    let ox = parents
        .get(root)
        .ok()
        .and_then(|p| oxen.get(p.parent()).ok().map(|_| p.parent()));
    if model.is_none() && ox.is_none() {
        return;
    }
    // La entidad del vehículo: el cuerpo cuelga de ella, el modelo del cuerpo.
    let vehicle_entity = model
        .and_then(|(_, body)| parents.get(body.parent()).ok())
        .map(|p| p.parent())
        .or_else(|| {
            ox.and_then(|o| parents.get(o).ok())
                .and_then(|b| parents.get(b.parent()).ok())
                .map(|p| p.parent())
        });
    for entity in children.iter_descendants(root) {
        let Ok((name, material_name, material, has_mesh, has_player)) = parts.get(entity) else {
            continue;
        };
        if let (Some((&VehicleModel(vehicle), _)), Some(name)) = (model, name)
            && let Some(radius) = wheel_radius(vehicle, name.as_str())
        {
            commands.entity(entity).try_insert(VehicleWheel { radius });
            if let Some(v) = vehicle_entity {
                commands.entity(entity).try_insert(OfVehicle(v));
            }
        }
        if has_player && let Some(o) = ox {
            commands.entity(o).try_insert(OxRig { player: entity });
        }
        if !has_mesh {
            continue;
        }
        let lod = parents
            .get(entity)
            .ok()
            .and_then(|p| parts.get(p.parent()).ok())
            .and_then(|p| p.0)
            .map(|n| n.as_str().ends_with("_LOD"))
            .unwrap_or(false)
            || name.is_some_and(|n| n.as_str().ends_with("_LOD"));
        if model.is_some() {
            commands.entity(entity).try_insert(if lod {
                VisibilityRange {
                    start_margin: LOD_NEAR - 5.0..LOD_NEAR,
                    end_margin: 1e5..1e5 + 1.0,
                    use_aabb: false,
                }
            } else {
                VisibilityRange::abrupt(0.0, LOD_NEAR)
            });
        }
        let (Some(mats), Some(material_name)) = (mats.as_deref(), material_name) else {
            continue;
        };
        let ours = if model.is_some() {
            mats.by_name.get(material_name.0.as_str()).cloned()
        } else {
            ox_material(
                mats,
                &mut materials,
                &material_name.0,
                material.map(|m| &m.0),
            )
        };
        if let Some(ours) = ours {
            commands.entity(entity).try_insert(MeshMaterial3d(ours));
        }
    }
}

/// El material del buey para su material glTF `name`: el pelaje criollo en
/// el cuero, los mechones con alpha test, y las partes de un solo color (el
/// morro, las pezuñas, los cuernos) con su color sobre la luminancia del
/// cuerpo.
fn ox_material(
    mats: &VehicleMaterials,
    materials: &mut Assets<StandardMaterial>,
    name: &str,
    base: Option<&Handle<StandardMaterial>>,
) -> Option<Handle<StandardMaterial>> {
    if let Some(m) = mats.ox.get(name) {
        return Some(m.clone());
    }
    let [coat, lum, normal, orm] = mats.ox_maps.clone()?;
    if name.starts_with("Eye") {
        return None;
    }
    let mut m = base
        .and_then(|b| materials.get(b))
        .cloned()
        .unwrap_or_default();
    m.base_color_texture = Some(if name == "Main" { coat } else { lum });
    if name == "Main" {
        m.base_color = Color::WHITE;
    }
    m.normal_map_texture = Some(normal);
    m.metallic_roughness_texture = Some(orm.clone());
    m.occlusion_texture = Some(orm);
    m.metallic = 0.0;
    m.perceptual_roughness = 1.0;
    Some(materials.add(m))
}

/// De qué cuelga una pieza de arreos.
#[derive(Debug, Clone)]
enum Hang {
    /// Rígida, del hueso de este nombre (`<pieza>@<hueso>`).
    Joint(String),
    /// Con skinning: sobre los huesos del animal con estos nombres, en orden,
    /// con estas matrices de bind inversas (las de su propio archivo).
    Skin(Vec<String>, Handle<SkinnedMeshInverseBindposes>),
}

/// Una pieza de arreos: su nombre, de qué cuelga, su malla y el nombre de su
/// material.
type Piece = (String, Hang, Handle<Mesh>, Option<String>);

/// Las piezas de un glTF de arreos, con su nombre, de qué cuelgan, su malla y
/// el nombre de su material: las rígidas `<pieza>@<hueso>` y las que tienen
/// piel. `None` mientras falte cargar alguno de sus nodos o pieles.
fn pieces(
    file: &Gltf,
    nodes: &Assets<GltfNode>,
    meshes: &Assets<GltfMesh>,
    skins: &Assets<GltfSkin>,
) -> Option<Vec<Piece>> {
    let names: HashMap<AssetId<GltfMaterial>, String> = file
        .named_materials
        .iter()
        .map(|(n, h)| (h.id(), n.to_string()))
        .collect();
    let mut out = Vec::new();
    for (node_name, handle) in &file.named_nodes {
        let node = nodes.get(handle)?;
        let Some(mesh) = node.mesh.as_ref() else {
            continue;
        };
        let hang = if let Some(skin) = node.skin.as_ref() {
            let skin = skins.get(skin)?;
            let joints = skin
                .joints
                .iter()
                .map(|j| nodes.get(j).map(|n| n.name.clone()))
                .collect::<Option<Vec<_>>>()?;
            Hang::Skin(joints, skin.inverse_bind_matrices.clone())
        } else if let Some((_, joint)) = node_name.split_once('@') {
            Hang::Joint(joint.to_string())
        } else {
            continue;
        };
        for p in &meshes.get(mesh)?.primitives {
            let material = p
                .material
                .as_ref()
                .and_then(|m| names.get(&m.id()))
                .cloned();
            out.push((
                node_name.to_string(),
                hang.clone(),
                p.mesh.clone(),
                material,
            ));
        }
    }
    Some(out)
}

/// Los almacenes de los que se leen los arreos y las pieles.
type HarnessAssets<'w> = (
    Option<Res<'w, Assets<Gltf>>>,
    Option<Res<'w, Assets<GltfNode>>>,
    Option<Res<'w, Assets<GltfMesh>>>,
    Option<Res<'w, Assets<GltfSkin>>>,
    Option<Res<'w, Assets<SkinnedMeshInverseBindposes>>>,
);

/// Viste a los caballos de tiro con sus arreos y al buey de la izquierda con
/// el yugo, una vez que su modelo con piel está creado: cada pieza con piel se
/// pone sobre las articulaciones del animal del mismo nombre (junto a su
/// piel); cada pieza rígida cuelga del hueso de su nombre por su matriz de
/// bind inversa.
#[allow(clippy::too_many_arguments)]
pub fn dress_animals(
    mut commands: Commands,
    models: Option<Res<VehicleModels>>,
    mats: Option<Res<VehicleMaterials>>,
    (files, nodes, meshes, skins, binds): HarnessAssets,
    horses: Query<Entity, With<VehicleHorse>>,
    oxen: Query<(Entity, &VehicleOx)>,
    (children, parents, dressed): (Query<&Children>, Query<&ChildOf>, Query<(), With<Dressed>>),
    worn: Query<&DressedOn>,
    skinned: Query<&SkinnedMesh>,
    names: Query<&Name>,
) {
    let (
        Some(models),
        Some(mats),
        Some(files),
        Some(nodes),
        Some(meshes),
        Some(skins),
        Some(binds),
    ) = (models, mats, files, nodes, meshes, skins, binds)
    else {
        return;
    };
    let animals = horses.iter().map(|e| (e, models.harness.as_ref())).chain(
        oxen.iter()
            .map(|(e, ox)| (e, if ox.left { models.yoke.as_ref() } else { None })),
    );
    for (animal, file) in animals {
        // Already dressed and the same scene: nothing to walk.
        if worn.get(animal).is_ok_and(|w| dressed.contains(w.0)) {
            continue;
        }
        let Some((primitive, skin)) = children
            .iter_descendants(animal)
            .find_map(|e| skinned.get(e).ok().map(|s| (e, s)))
        else {
            continue;
        };
        // El nodo de la malla del animal: sus primitivas (la piel, las
        // tarjetas de pelo) pueden ocultarse por separado.
        let body = parents.get(primitive).map_or(primitive, ChildOf::parent);
        if dressed.contains(body) {
            commands.entity(animal).try_insert(DressedOn(body));
            continue;
        }
        let Some(file) = file else {
            commands.entity(body).try_insert(Dressed);
            commands.entity(animal).try_insert(DressedOn(body));
            continue;
        };
        let (Some(file), Some(bind)) = (files.get(file), binds.get(&skin.inverse_bindposes)) else {
            continue;
        };
        let Some(pieces) = pieces(file, &nodes, &meshes, &skins) else {
            continue;
        };
        let joints: HashMap<&str, Entity> = children
            .iter_descendants(animal)
            .filter_map(|e| names.get(e).ok().map(|n| (n.as_str(), e)))
            .collect();
        for (piece, hang, mesh, material) in pieces {
            let Some(material) = material.and_then(|m| mats.by_name.get(&m).cloned()) else {
                continue;
            };
            match hang {
                Hang::Skin(bones, inverse_bindposes) => {
                    let Some(bones) = bones
                        .iter()
                        .map(|b| joints.get(b.as_str()).copied())
                        .collect::<Option<Vec<_>>>()
                    else {
                        warn!("the harness piece {piece} names joints the animal lacks");
                        continue;
                    };
                    commands.spawn((
                        Name::new(format!("Harness {piece}")),
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        SkinnedMesh {
                            inverse_bindposes,
                            joints: bones,
                        },
                        Transform::default(),
                        ChildOf(body),
                    ));
                }
                Hang::Joint(joint) => {
                    let Some(k) = skin
                        .joints
                        .iter()
                        .position(|&j| names.get(j).is_ok_and(|n| n.as_str() == joint))
                    else {
                        continue;
                    };
                    commands.spawn((
                        Name::new(format!("Harness on {joint}")),
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform::from_matrix(bind[k]),
                        ChildOf(skin.joints[k]),
                    ));
                }
            }
        }
        commands.entity(body).try_insert(Dressed);
        commands.entity(animal).try_insert(DressedOn(body));
    }
}

/// The left terret of the draught harness (`tools/vehicle_harness.py`
/// `TERRET`, game metres in Blender's frame) in `harness.glb`'s own space:
/// the file's metres (the game's over `SCALE`, 1.5/1.6), +Y up and the horse
/// facing +Z, its left +X. The right one mirrors X.
pub const TERRET: Vec3 = Vec3::new(0.095 / 0.9375, 1.53 / 0.9375, 0.27 / 0.9375);

/// The joint that carries the terrets' rein ends: the first of the two bones
/// of the back under the harness pad (which rides both, half and half).
pub const TERRET_JOINT: &str = "BN_Spine_01_03_03";

/// How much a driver's rein sags from the terret to the hand, as a share of
/// its length: a little, so it runs over the croup (straight stretches,
/// 2.4 m on the sulky, cleared it by 10 cm or more; at 0.05 the sulky's
/// would sag into the quarters).
pub const DRIVER_REIN_SAG: f32 = 0.01;

/// A driver's rein: its width (a flat leather line, `vehicle_harness.REIN`)
/// and its thickness over that width.
pub const DRIVER_REIN: (f32, f32) = (0.02, 0.25);

/// Where the reins end on a vehicle with nobody on the seat, in the seat's
/// space (+X ahead, +Y up): where a seated driver's hands are (on the sulky,
/// measured 0.24 m ahead of and 0.47 m over the seat).
pub const IDLE_HANDS: Vec3 = Vec3::new(0.24, 0.47, 0.0);

/// A rein from a draught horse's terret to the driver's left hand, which
/// holds all the reins as a driver does (or, with nobody driving, over the
/// seat): `side` 0 is the horse's left rein. The stretch from the bit to the
/// terret is the harness's (`Harness Reins`), on the horse's head.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverRein {
    pub horse: Entity,
    pub side: usize,
}

/// The point `at` of `horse`'s harness (in `harness.glb`'s space) on the
/// horse's joint `joint`, once its harness is on: the joint and the point in
/// its space (the harness's own inverse bind of `joint` takes it there).
fn harness_point(
    horse: Entity,
    joint: &str,
    at: Vec3,
    children: &Query<&Children>,
    pieces: &Query<(&Name, &SkinnedMesh)>,
    names: &Query<&Name>,
    binds: &Assets<SkinnedMeshInverseBindposes>,
) -> Option<(Entity, Vec3)> {
    let (_, skin) = children.iter_descendants(horse).find_map(|e| {
        pieces
            .get(e)
            .ok()
            .filter(|(n, _)| n.as_str() == "Harness Saddle")
    })?;
    let k = skin
        .joints
        .iter()
        .position(|&j| names.get(j).is_ok_and(|n| n.as_str() == joint))?;
    let bind = binds.get(&skin.inverse_bindposes)?.get(k)?;
    Some((skin.joints[k], bind.transform_point3(at)))
}

/// `at` on the horse's left (`side` 0) or mirrored to its right.
fn sided(at: Vec3, side: usize) -> Vec3 {
    if side == 0 {
        at
    } else {
        at * Vec3::new(-1.0, 1.0, 1.0)
    }
}

/// The terret end of rein `side` of `horse`.
fn terret_end(
    horse: Entity,
    side: usize,
    children: &Query<&Children>,
    pieces: &Query<(&Name, &SkinnedMesh)>,
    names: &Query<&Name>,
    binds: &Assets<SkinnedMeshInverseBindposes>,
) -> Option<(Entity, Vec3)> {
    harness_point(
        horse,
        TERRET_JOINT,
        sided(TERRET, side),
        children,
        pieces,
        names,
        binds,
    )
}

/// The hand end of rein `side` (of the vehicle with seat `seat`): the
/// driver's left palm, the two reins of a horse a finger apart (like a
/// rider's, [`rein_end`](crate::character::rider::rein_end)); with nobody
/// driving, [`IDLE_HANDS`] over the seat.
fn hand_end(
    seat: Entity,
    side: usize,
    children: &Query<&Children>,
    names: &Query<&Name>,
) -> (Entity, Vec3) {
    let apart = Vec3::X * if side == 0 { -0.012 } else { 0.012 };
    children
        .iter_descendants(seat)
        .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "hand_l"))
        .map_or((seat, IDLE_HANDS + Vec3::Z * apart.x), |hand| {
            (hand, PALM + apart)
        })
}

/// Gives each draught horse its two reins from its terrets to the driver's
/// hands, once its harness is on, and keeps their ends on the current joints
/// (a horse's or a driver's scene may be created again, and the driver comes
/// after the vehicle). An end is looked for again only while it is missing
/// or, for the hands, while nobody holds the reins.
#[allow(clippy::too_many_arguments)]
pub fn rein_drivers(
    mut commands: Commands,
    mats: Option<Res<VehicleMaterials>>,
    binds: Option<Res<Assets<SkinnedMeshInverseBindposes>>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    horses: Query<(Entity, &ChildOf), With<VehicleHorse>>,
    seats: Query<(Entity, &ChildOf), With<VehicleSeat>>,
    (children, names, pieces): (Query<&Children>, Query<&Name>, Query<(&Name, &SkinnedMesh)>),
    places: Query<(), With<GlobalTransform>>,
    mut reins: Query<(Entity, &DriverRein, &mut Strap)>,
    mut rein_mesh: Local<Option<Handle<Mesh>>>,
) {
    let (Some(mats), Some(binds), Some(meshes)) = (mats, binds, meshes.as_mut()) else {
        return;
    };
    let Some(leather) = mats.by_name.get("Leather").cloned() else {
        return;
    };
    let mut have: HashMap<(Entity, usize), Entity> = HashMap::default();
    for (e, rein, _) in &reins {
        have.insert((rein.horse, rein.side), e);
    }
    for (horse, body) in &horses {
        let body = body.parent();
        let Some(seat) = seats
            .iter()
            .find(|(_, p)| p.parent() == body)
            .map(|(e, _)| e)
        else {
            continue;
        };
        for side in 0..2 {
            if let Some(&e) = have.get(&(horse, side)) {
                let Ok((_, _, mut strap)) = reins.get_mut(e) else {
                    continue;
                };
                let [terret, hand] = strap.ends;
                let terret = if places.contains(terret.0) {
                    terret
                } else {
                    match terret_end(horse, side, &children, &pieces, &names, &binds) {
                        Some(t) => t,
                        None => continue,
                    }
                };
                let hand = if places.contains(hand.0) && hand.0 != seat {
                    hand
                } else {
                    hand_end(seat, side, &children, &names)
                };
                if strap.ends != [terret, hand] {
                    strap.ends = [terret, hand];
                }
                continue;
            }
            let Some(terret) = terret_end(horse, side, &children, &pieces, &names, &binds) else {
                continue;
            };
            let hand = hand_end(seat, side, &children, &names);
            let mesh = rein_mesh
                .get_or_insert_with(|| meshes.add(strap_mesh(6, DRIVER_REIN.1, (0.0, 0.5), 8.0)))
                .clone();
            commands
                .spawn((
                    Name::new("Driver rein"),
                    DriverRein { horse, side },
                    Strap {
                        ends: [terret, hand],
                        width: DRIVER_REIN.0,
                        flat: false,
                        sag: DRIVER_REIN_SAG,
                    },
                    Transform::default(),
                    Visibility::default(),
                    ChildOf(body),
                ))
                .with_children(|parts| {
                    for k in 0..SAG_PARTS {
                        parts.spawn((
                            Name::new("Driver rein"),
                            StrapPart { k, n: SAG_PARTS },
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(leather.clone()),
                            Transform::default(),
                        ));
                    }
                });
        }
    }
}

/// The left end of the draught harness's breeching (`tools/vehicle_harness.py`
/// `BREECH`), in `harness.glb`'s space like [`TERRET`]; the right mirrors X.
pub const BREECH: Vec3 = Vec3::new(0.277 / 0.9375, 1.20 / 0.9375, -0.34 / 0.9375);

/// The joint that carries the breeching's ends: the skin there takes half its
/// weight from this bone (the rest from the pelvis and the thigh), and a point
/// on it keeps within a millimetre of its standing distance off the skin
/// through `Idle_1`, `Walk` and `Gallop` (a point fixed on the sulky went
/// 2.6 cm into the quarters at the gallop).
pub const BREECH_JOINT: &str = "BN_Spine_00_02_02";

/// Where the sulky's left breeching strap is buckled round its left shaft, in
/// the body's space (`tools/sulky.py` `shaft_at(1, BREECH_TIE)`; Blender
/// (x, y, z) is here (-y, z, -x); `tools/test_vehicle.py` checks it); the right
/// mirrors Z. The same on the gig, whose shafts `tools/sulky_gig.py` bends onto
/// the sulky's line along the horse (`tools/test_sulky_gig.py` checks its
/// shaft there).
pub const SULKY_BREECH_TIE: Vec3 = Vec3::new(-0.40, 1.046, -0.354);

/// A breeching strap's width and its thickness over that width
/// (`tools/sulky.py`'s 3 cm by 6 mm).
pub const BREECH_STRAP: (f32, f32) = (0.03, 0.2);

/// A strap from the end of a draught horse's breeching (`side` 0 its left) to
/// the vehicle's shaft, which holds the vehicle back going downhill: drawn
/// here, from the horse's quarters, as they move under it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreechingStrap {
    pub horse: Entity,
    pub side: usize,
}

/// Straps each sulky horse's breeching to the shafts once its harness is on,
/// and keeps the horse's end on its current joint (its scene may be created
/// again). The carretón carries no breeching strap.
#[allow(clippy::too_many_arguments)]
pub fn strap_breeching(
    mut commands: Commands,
    mats: Option<Res<VehicleMaterials>>,
    binds: Option<Res<Assets<SkinnedMeshInverseBindposes>>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    vehicles: Query<(&super::VehicleVisual, &super::VehicleParts)>,
    horses: Query<(Entity, &ChildOf), With<VehicleHorse>>,
    (children, names, pieces): (Query<&Children>, Query<&Name>, Query<(&Name, &SkinnedMesh)>),
    places: Query<(), With<GlobalTransform>>,
    mut straps: Query<(&BreechingStrap, &mut Strap)>,
    mut strap_mesh_handle: Local<Option<Handle<Mesh>>>,
) {
    let (Some(mats), Some(binds), Some(meshes)) = (mats, binds, meshes.as_mut()) else {
        return;
    };
    let Some(leather) = mats.by_name.get("Leather").cloned() else {
        return;
    };
    let mut have: HashSet<(Entity, usize)> = HashSet::default();
    for (tie, mut strap) in &mut straps {
        have.insert((tie.horse, tie.side));
        if places.contains(strap.ends[0].0) {
            continue;
        }
        let at = sided(BREECH, tie.side);
        if let Some(end) = harness_point(
            tie.horse,
            BREECH_JOINT,
            at,
            &children,
            &pieces,
            &names,
            &binds,
        ) {
            strap.ends[0] = end;
        }
    }
    for (horse, body) in &horses {
        let body = body.parent();
        let on_sulky = vehicles
            .iter()
            .any(|(v, parts)| parts.body == body && v.vehicle == Vehicle::Sulky);
        if !on_sulky {
            continue;
        }
        for side in 0..2 {
            if have.contains(&(horse, side)) {
                continue;
            }
            let at = sided(BREECH, side);
            let Some(end) =
                harness_point(horse, BREECH_JOINT, at, &children, &pieces, &names, &binds)
            else {
                continue;
            };
            let tie = SULKY_BREECH_TIE
                * if side == 0 {
                    Vec3::ONE
                } else {
                    Vec3::new(1.0, 1.0, -1.0)
                };
            let mesh = strap_mesh_handle
                .get_or_insert_with(|| meshes.add(strap_mesh(8, BREECH_STRAP.1, (0.0, 0.5), 2.0)))
                .clone();
            commands.spawn((
                Name::new("Breeching strap"),
                BreechingStrap { horse, side },
                // Flat: its width along the body's +X (ahead), its face out.
                Strap {
                    ends: [end, (body, tie)],
                    width: BREECH_STRAP.0,
                    flat: true,
                    sag: 0.0,
                },
                Mesh3d(mesh),
                MeshMaterial3d(leather.clone()),
                Transform::default(),
                Visibility::default(),
                ChildOf(body),
            ));
        }
    }
}

/// Arma el grafo del buey cuando carga su archivo.
pub fn ox_graph(
    mut models: Option<ResMut<VehicleModels>>,
    files: Option<Res<Assets<Gltf>>>,
    mut graphs: Option<ResMut<Assets<AnimationGraph>>>,
    clips: Option<Res<Assets<AnimationClip>>>,
) {
    let (Some(models), Some(graphs), Some(files), Some(clips)) =
        (models.as_mut(), graphs.as_mut(), files, clips)
    else {
        return;
    };
    let Some(ox) = models.ox.as_mut() else { return };
    if ox.graph.is_some() {
        return;
    }
    let Some(file) = files.get(&ox.file) else {
        return;
    };
    let (Some(walk), Some(idle)) = (
        file.named_animations.get("Walk"),
        file.named_animations.get("Idle"),
    ) else {
        return;
    };
    let length = clips.get(walk).map_or(1.04, |c| c.duration());
    let (graph, nodes) = AnimationGraph::from_clips([walk.clone(), idle.clone()]);
    ox.graph = Some((graphs.add(graph), nodes[0], nodes[1], length));
}

/// Hace andar a los bueyes con lo rodado de su carreta: el clip `Walk` en el
/// punto de su ciclo que corresponde a la distancia andada (los dos de la
/// yunta al mismo paso, que es el del suelo), o `Idle` parados.
pub fn walk_oxen(
    mut commands: Commands,
    models: Option<Res<VehicleModels>>,
    oxen: Query<(&OxRig, &OfVehicle)>,
    vehicles: Query<(&Rolled, &crate::character::VisualPose)>,
    mut players: Query<(&mut AnimationPlayer, Option<&AnimationGraphHandle>)>,
) {
    let Some((graph, walk, idle, length)) = models
        .as_ref()
        .and_then(|m| m.ox.as_ref())
        .and_then(|o| o.graph.clone())
    else {
        return;
    };
    for (rig, of) in &oxen {
        let Ok((rolled, pose)) = vehicles.get(of.0) else {
            continue;
        };
        let Ok((mut player, handle)) = players.get_mut(rig.player) else {
            continue;
        };
        if handle.is_none() {
            commands
                .entity(rig.player)
                .try_insert(AnimationGraphHandle(graph.clone()));
        }
        // Metros que cubre un ciclo del clip a la escala del buey.
        let stride = CATTLE_WALK_SPEED * OX_SCALE * length;
        if pose.speed.abs() > 0.05 {
            player.stop(idle);
            let active = player.play(walk);
            active.set_speed(0.0);
            let t = (rolled.0 / stride).rem_euclid(1.0) * length;
            active.seek_to(t);
        } else {
            player.stop(walk);
            player.play(idle).repeat();
        }
    }
}
