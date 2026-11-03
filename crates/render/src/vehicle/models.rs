//! Los modelos de los vehículos, hechos en Blender por script
//! (`tools/sulky.py`, `tools/carreton.py`, `tools/carreta.py`,
//! `tools/ford_t.py`), sus arreos (`tools/vehicle_harness.py`) y el buey de la
//! carreta (`tools/cattle.py --ox`).
//!
//! - Cada vehículo es una escena glTF colgada del cuerpo del vehículo, girada
//!   de su +Z (su frente en el glTF) al +X del cuerpo. Sus nodos `Wheel.*`
//!   reciben un [`VehicleWheel`] cuando se crea la escena, y giran sobre su X
//!   con lo rodado. Sus materiales llevan el nombre de su conjunto de mapas
//!   (`textures/vehicles/<conjunto>/`, KTX2) y se cambian por los propios
//!   ([`VehicleMaterials`]); su malla lejana (`*_LOD`) se dibuja desde
//!   [`LOD_NEAR`].
//! - Los arreos de un caballo de tiro y el yugo del buey de la izquierda son
//!   piezas rígidas `<pieza>@<hueso>` con sus vértices en el espacio del
//!   modelo en su pose de bind: cada una cuelga del hueso por su matriz de
//!   bind inversa, así que acompaña cada clip ([`dress_animals`]).
//! - El buey anda con el clip `Walk` llevado por lo rodado, así que sus patas
//!   pisan al paso del vehículo y los dos de la yunta van iguales; parado,
//!   rumia con `Idle` ([`walk_oxen`]).

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{Gltf, GltfMaterial, GltfMaterialName, GltfMesh, GltfNode};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::Vehicle;

use super::{Rolled, VehicleHorse, VehicleWheel};
use crate::AssetRoot;
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

/// El archivo del modelo de `vehicle`.
pub fn model_path(vehicle: Vehicle) -> String {
    let name = match vehicle {
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
    pub scenes: HashMap<Vehicle, Handle<WorldAsset>>,
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

/// Un animal que ya lleva sus arreos (o su yugo).
#[derive(Component, Debug, Clone, Copy)]
pub struct Dressed;

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
        for vehicle in [
            Vehicle::Sulky,
            Vehicle::Cart,
            Vehicle::Carreta,
            Vehicle::FordT,
        ] {
            if let Some(scene) = gltf::load_scene(&server, Some(&root), &model_path(vehicle)) {
                models.scenes.insert(vehicle, scene);
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

/// Las piezas `<pieza>@<hueso>` de un glTF de arreos, con su malla y el
/// nombre de su material.
fn pieces<'a>(
    file: &'a Gltf,
    nodes: &'a Assets<GltfNode>,
    meshes: &'a Assets<GltfMesh>,
) -> Vec<(&'a str, Handle<Mesh>, Option<String>)> {
    let names: HashMap<AssetId<GltfMaterial>, String> = file
        .named_materials
        .iter()
        .map(|(n, h)| (h.id(), n.to_string()))
        .collect();
    let mut out = Vec::new();
    for (node_name, handle) in &file.named_nodes {
        let Some((_, joint)) = node_name.split_once('@') else {
            continue;
        };
        let Some(mesh) = nodes
            .get(handle)
            .and_then(|n| n.mesh.as_ref())
            .and_then(|m| meshes.get(m))
        else {
            continue;
        };
        for p in &mesh.primitives {
            let material = p
                .material
                .as_ref()
                .and_then(|m| names.get(&m.id()))
                .cloned();
            out.push((joint, p.mesh.clone(), material));
        }
    }
    out
}

/// Los almacenes de los que se leen los arreos y las pieles.
type HarnessAssets<'w> = (
    Option<Res<'w, Assets<Gltf>>>,
    Option<Res<'w, Assets<GltfNode>>>,
    Option<Res<'w, Assets<GltfMesh>>>,
    Option<Res<'w, Assets<SkinnedMeshInverseBindposes>>>,
);

/// Viste a los caballos de tiro con sus arreos y al buey de la izquierda con
/// el yugo, una vez que su modelo con piel está creado: cada pieza cuelga del
/// hueso de su nombre por su matriz de bind inversa.
#[allow(clippy::too_many_arguments)]
pub fn dress_animals(
    mut commands: Commands,
    models: Option<Res<VehicleModels>>,
    mats: Option<Res<VehicleMaterials>>,
    (files, nodes, meshes, binds): HarnessAssets,
    horses: Query<Entity, (With<VehicleHorse>, Without<Dressed>)>,
    oxen: Query<(Entity, &VehicleOx), Without<Dressed>>,
    children: Query<&Children>,
    skins: Query<&SkinnedMesh>,
    names: Query<&Name>,
) {
    let (Some(models), Some(mats), Some(files), Some(nodes), Some(meshes), Some(binds)) =
        (models, mats, files, nodes, meshes, binds)
    else {
        return;
    };
    let animals = horses.iter().map(|e| (e, models.harness.as_ref())).chain(
        oxen.iter()
            .map(|(e, ox)| (e, if ox.left { models.yoke.as_ref() } else { None })),
    );
    for (animal, file) in animals {
        let Some(skin) = children
            .iter_descendants(animal)
            .find_map(|e| skins.get(e).ok())
        else {
            continue;
        };
        let Some(file) = file else {
            commands.entity(animal).try_insert(Dressed);
            continue;
        };
        let (Some(file), Some(bind)) = (files.get(file), binds.get(&skin.inverse_bindposes)) else {
            continue;
        };
        for (joint, mesh, material) in pieces(file, &nodes, &meshes) {
            let Some(k) = skin
                .joints
                .iter()
                .position(|&j| names.get(j).is_ok_and(|n| n.as_str() == joint))
            else {
                continue;
            };
            let Some(material) = material.and_then(|m| mats.by_name.get(&m).cloned()) else {
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
        commands.entity(animal).try_insert(Dressed);
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
