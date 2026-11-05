//! Los gorriones y los chingolos (ver `pampa_core::fauna::sparrow`): los
//! modelos propios de `tools/sparrow.py`, a tamaño real (14 y 13 cm), con
//! skinning y sus cinco clips (`Flap`, `FlapBound`, `Hop`, `Peck`, `Idle`).
//!
//! El plugin siembra los grupos (las bandadas de gorriones de cada pueblo, con
//! los pretiles de sus casas como posaderos, [`town_roofs`]; los chingolos de
//! los alambrados de cada chunk) y los trae por streaming alrededor del
//! jugador ([`StreamFocus`]): aparecen los grupos cuyo comedero está a
//! [`SPAWN_RADIUS`] o menos, del más cercano al más lejano, hasta
//! [`MAX_GORRIONES`] y [`MAX_CHINGOLOS`] pájaros, solo de día; se van más allá
//! de [`DESPAWN_RADIUS`]. Los hace avanzar con el jugador como la amenaza de
//! la que vuelan, y cada pájaro reproduce el clip que su estado pide, desde
//! donde lo pide y a su propio ritmo. Cada uno dibuja su malla completa cerca
//! y la liviana (`Gorrion_LOD`, `Chingolo_LOD`) desde `LIGHT_FROM`, con un
//! fundido con dithering ([`light_range`]); ninguna hace sombra. El
//! cuerpo y las plumas dejan pasar algo del sol (`FEATHER_TRANSMISSION`), como
//! en las palomas. Sin los archivos de un modelo, sus pájaros se simulan pero
//! no se dibujan.

use std::collections::HashMap;

use bevy::animation::graph::AnimationNodeIndex;
use bevy::animation::transition::AnimationTransitions;
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{GltfAssetLabel, GltfMaterialName, GltfMeshName};
use bevy::image::CompressedImageFormatSupport;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::fauna::sparrow::place::{fence_sparrows, roof_perches, town_sparrows};
use pampa_core::{
    ChunkCoord, Perch, SparrowClip, SparrowGroup, SparrowKind, Spot, Terrain, TownLayout,
    Vec2 as GroundVec2,
};

use crate::AssetRoot;
use crate::atmosphere::WorldClock;
use crate::coords::world_from_ground;
use crate::facade::materials::{MAPS, load_map_with};
use crate::facade::town_buildings::is_cottage;
use crate::fauna::livestock::{
    FEATHER_TRANSMISSION, LIGHT_SUFFIX, OWN_DIR, Skin, SkinMaps, light_range, skin_path, skinned,
};
use crate::fence::WorldFences;
use crate::gltf;
use crate::settlement::WorldSettlements;
use crate::terrain::{StreamFocus, WorldChunks, WorldTerrain};
use crate::town::{OUTSKIRTS, SIDEWALK_TOP, STREET_TOP, WorldTowns, variant};
use crate::tree::materials::clamped;

/// Hasta qué distancia del jugador aparecen los grupos, en metros: un pájaro
/// de 14 cm a 70 m mide unos 2 píxeles en 1080p (60 grados de campo
/// vertical), así que más lejos no se ve.
pub const SPAWN_RADIUS: f32 = 70.0;
/// Más allá de esta distancia se van, en metros (un margen sobre
/// [`SPAWN_RADIUS`] para que no aparezcan y desaparezcan en el borde).
pub const DESPAWN_RADIUS: f32 = 90.0;
/// Cuántos gorriones y cuántos chingolos hay a la vez como mucho: cada uno es
/// una escena con skinning y su reproductor de animación, lo que más cuesta en
/// la iGPU (las palomas: unas 0.5 ms por 39).
pub const MAX_GORRIONES: usize = 36;
pub const MAX_CHINGOLOS: usize = 8;
/// Las horas del día en que andan, de la mañana a la tarde.
pub const DAY: (f32, f32) = (6.5, 19.5);
/// Hasta dónde llega el rango de sus mallas, en metros.
const FAR: f32 = DESPAWN_RADIUS + 10.0;
/// Cuánto tarda en fundirse un clip con el siguiente, en segundos.
const BLEND: f32 = 0.08;

/// El modelo de cada especie, bajo el [`AssetRoot`].
pub fn path(kind: SparrowKind) -> String {
    format!("{OWN_DIR}/{}.glb", name(kind))
}

fn name(kind: SparrowKind) -> &'static str {
    match kind {
        SparrowKind::Gorrion => "gorrion",
        SparrowKind::Chingolo => "chingolo",
    }
}

/// La altura del cuerpo sobre los dedos en los clips de suelo, en metros
/// (`tools/sparrow.py`'s `stand_height()`): en los de vuelo el centro del
/// cuerpo va en el origen, así que en el aire el modelo sube esto.
pub fn stand_height(kind: SparrowKind) -> f32 {
    match kind {
        SparrowKind::Gorrion => 0.031,
        SparrowKind::Chingolo => 0.032,
    }
}

/// Cuánto sobre el terreno está la superficie de `perch` (la calle y la vereda
/// se dibujan unos centímetros arriba); los posaderos ya traen su altura.
pub fn surface_lift(perch: Perch) -> f32 {
    match perch {
        Perch::Street => STREET_TOP,
        Perch::Sidewalk => SIDEWALK_TOP,
        _ => 0.0,
    }
}

/// Los mapas del material `material` de `kind`: el cuerpo (`Main`) y las
/// tarjetas de plumas (`Feathers`, con alpha test y de dos capas).
pub fn skin(kind: SparrowKind, material: &str) -> Option<Skin> {
    let set = match (kind, material) {
        (SparrowKind::Gorrion, "Main") => "gorrion_body",
        (SparrowKind::Gorrion, "Feathers") => "gorrion_feathers",
        (SparrowKind::Chingolo, "Main") => "chingolo_body",
        (SparrowKind::Chingolo, "Feathers") => "chingolo_feathers",
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

/// Los pretiles de las casas de `town` donde se posan los gorriones: los de
/// todas sus casas italianizantes, es decir, todas menos los ranchos de las
/// afueras ([`is_cottage`], la misma regla con la que se construyen).
pub fn town_roofs(town: &TownLayout, terrain: &Terrain) -> Vec<Spot> {
    let grid = town.grid();
    roof_perches(town, terrain, |i| {
        let b = &town.buildings[i];
        let c = b.footprint.center();
        let edge = (c.x - grid.min.x)
            .min(grid.max.x - c.x)
            .min(c.y - grid.min.y)
            .min(grid.max.y - c.y);
        !is_cottage(edge < OUTSKIRTS, variant(i, &b.footprint))
    })
}

/// Un modelo cargado.
#[derive(Debug, Clone)]
pub struct SparrowModel {
    pub scene: Handle<WorldAsset>,
    pub graph: Handle<AnimationGraph>,
    /// El nodo de cada clip, en el orden de [`SparrowClip::ALL`].
    pub nodes: [AnimationNodeIndex; 5],
    maps: HashMap<&'static str, SkinMaps>,
}

/// Los modelos encontrados al iniciar y los materiales vestidos, compartidos.
#[derive(Resource, Debug, Default)]
pub struct SparrowModels {
    models: [Option<SparrowModel>; 2],
    materials: HashMap<(SparrowKind, AssetId<StandardMaterial>), Handle<StandardMaterial>>,
}

impl SparrowModels {
    pub fn get(&self, kind: SparrowKind) -> Option<&SparrowModel> {
        self.models[kind as usize].as_ref()
    }
}

fn clip_index(clip: SparrowClip) -> usize {
    SparrowClip::ALL
        .iter()
        .position(|&c| c == clip)
        .unwrap_or(0)
}

/// Un pájaro dibujado: su grupo y su lugar en él.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparrowBird {
    pub group: u64,
    pub index: usize,
    pub kind: SparrowKind,
}

/// El reproductor de animación del modelo de un pájaro y el clip que
/// reproduce.
#[derive(Component, Debug, Clone, Copy)]
pub struct SparrowRig {
    pub player: Entity,
    pub clip: Option<SparrowClip>,
}

/// La raíz del modelo de un pájaro, hija de su [`SparrowBird`].
#[derive(Component, Debug, Clone, Copy)]
pub struct SparrowModel3d {
    pub kind: SparrowKind,
    pub bird: Entity,
}

/// Los grupos sembrados (por pueblo y por chunk, una vez) y los vivos.
#[derive(Resource, Debug, Default)]
pub struct Sparrows {
    /// Los de cada pueblo, sembrados la primera vez que hacen falta.
    towns: Option<Vec<Vec<SparrowGroup>>>,
    chunks: HashMap<ChunkCoord, Vec<SparrowGroup>>,
    /// Los grupos vivos, con la entidad de cada pájaro.
    pub live: Vec<(SparrowGroup, Vec<Entity>)>,
    /// Dónde estaba el jugador al último reparto.
    planned_at: Option<GroundVec2>,
}

impl Sparrows {
    /// Cuántos pájaros vivos de `kind`.
    pub fn count(&self, kind: SparrowKind) -> usize {
        self.live
            .iter()
            .filter(|(g, _)| g.kind == kind)
            .map(|(g, _)| g.birds.len())
            .sum()
    }
}

/// Se mezcla con la semilla del terreno.
const SEED: u32 = 0x5a77_1919;

pub struct SparrowPlugin;

impl Plugin for SparrowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sparrows>()
            .add_systems(Startup, load_sparrows)
            .add_systems(Update, (stream_sparrows, step_sparrows).chain())
            .add_observer(dress_sparrow);
    }
}

/// Empieza a cargar los modelos, sus mapas y sus clips. Sin un servidor de
/// assets, una raíz o el plugin de animación, ninguno.
fn load_sparrows(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    graphs: Option<ResMut<Assets<AnimationGraph>>>,
    support: Option<Res<CompressedImageFormatSupport>>,
) {
    let mut out = SparrowModels::default();
    if let (Some(server), Some(root), Some(mut graphs)) = (server, root, graphs) {
        let compressed = support.as_ref().is_none_or(|s| !s.0.is_empty());
        for kind in SparrowKind::ALL {
            let path = path(kind);
            if !root.0.join(&path).is_file() {
                continue;
            }
            let Some(scene) = gltf::load_scene(&server, Some(&root), &path) else {
                continue;
            };
            let mut maps = HashMap::new();
            for material in ["Main", "Feathers"] {
                let Some(skin) = skin(kind, material) else {
                    continue;
                };
                let paths = MAPS.map(|(name, srgb)| (skin_path(skin.set, name), srgb));
                if !compressed || paths.iter().any(|(p, _)| !root.0.join(p).is_file()) {
                    warn!("sparrow maps {} not found; drawing plain colour", skin.set);
                    continue;
                }
                let [color, normal, orm] =
                    paths.map(|(path, srgb)| load_map_with(&server, &path, srgb, clamped()));
                maps.insert(skin.set, SkinMaps { color, normal, orm });
            }
            let clips = SparrowClip::ALL.map(|c| {
                server.load(GltfAssetLabel::Animation(clip_index(c)).from_asset(path.clone()))
            });
            let (graph, nodes) = AnimationGraph::from_clips(clips);
            let nodes: [AnimationNodeIndex; 5] = std::array::from_fn(|k| nodes[k]);
            out.models[kind as usize] = Some(SparrowModel {
                scene,
                graph: graphs.add(graph),
                nodes,
                maps,
            });
        }
    }
    commands.insert_resource(out);
}

/// Si en `hour` andan afuera.
pub fn out_at(hour: f32) -> bool {
    (DAY.0..DAY.1).contains(&hour)
}

/// Trae los grupos cercanos al jugador y se lleva los lejanos (ver la
/// documentación del módulo).
#[allow(clippy::too_many_arguments)]
fn stream_sparrows(
    mut commands: Commands,
    mut sparrows: ResMut<Sparrows>,
    focus: Option<Res<StreamFocus>>,
    clock: Option<Res<WorldClock>>,
    towns: Option<Res<WorldTowns>>,
    terrain: Option<Res<WorldTerrain>>,
    fences: Option<Res<WorldFences>>,
    chunks: Option<Res<WorldChunks>>,
    settlements: Option<Res<WorldSettlements>>,
    models: Option<Res<SparrowModels>>,
) {
    let (Some(focus), Some(terrain)) = (focus, terrain) else {
        return;
    };
    let at = focus.0;
    let terrain = &terrain.0;
    let sparrows = &mut *sparrows;
    // Se van los que quedaron lejos.
    sparrows.live.retain(|(group, birds)| {
        let keep = group.patch.area.center().distance(at) <= DESPAWN_RADIUS;
        if !keep {
            for &bird in birds {
                commands.entity(bird).despawn();
            }
        }
        keep
    });
    if sparrows.planned_at.is_some_and(|p| p.distance(at) < 5.0) {
        return;
    }
    sparrows.planned_at = Some(at);
    let hour = clock.map_or(crate::atmosphere::DEFAULT_HOUR, |c| c.0.hour());
    if !out_at(hour) {
        return;
    }
    let seed = terrain.seed ^ SEED;
    let towns_seeded = sparrows.towns.get_or_insert_with(|| {
        towns.as_ref().map_or(Vec::new(), |t| {
            t.0.iter()
                .enumerate()
                .map(|(i, town)| {
                    let roofs = town_roofs(town, terrain);
                    town_sparrows(town, i, &roofs, terrain, seed)
                })
                .collect()
        })
    });
    let mut candidates: Vec<SparrowGroup> = towns_seeded
        .iter()
        .flatten()
        .filter(|g| g.patch.area.center().distance(at) <= SPAWN_RADIUS)
        .cloned()
        .collect();
    if let (Some(fences), Some(chunks)) = (fences, chunks) {
        let size = chunks.0.chunk_size;
        // Los chunks que tocan el círculo, también en diagonal.
        let reach = (SPAWN_RADIUS / size).ceil() as i32 + 1;
        for coord in ChunkCoord::containing(at, size).within(reach) {
            if !chunks.0.has_chunk(coord) {
                continue;
            }
            let groups = sparrows.chunks.entry(coord).or_insert_with(|| {
                let clear = |p: GroundVec2| {
                    terrain.flat_areas.iter().all(|a| {
                        let dx = ((p.x - a.center.x).abs() - a.half_extents.x).max(0.0);
                        let dy = ((p.y - a.center.y).abs() - a.half_extents.y).max(0.0);
                        dx.hypot(dy) >= a.falloff
                    }) && terrain.lagoons.iter().all(|l| !l.is_water(p))
                        && settlements.as_ref().is_none_or(|s| s.0.is_clear(p, 2.0))
                };
                fence_sparrows(&fences.0, coord, size, terrain, seed, clear)
            });
            candidates.extend(
                groups
                    .iter()
                    .filter(|g| g.patch.area.center().distance(at) <= SPAWN_RADIUS)
                    .cloned(),
            );
        }
    }
    candidates.sort_by(|a, b| {
        a.patch
            .area
            .center()
            .distance(at)
            .total_cmp(&b.patch.area.center().distance(at))
    });
    for group in candidates {
        if sparrows.live.iter().any(|(g, _)| g.id == group.id) {
            continue;
        }
        let cap = match group.kind {
            SparrowKind::Gorrion => MAX_GORRIONES,
            SparrowKind::Chingolo => MAX_CHINGOLOS,
        };
        if sparrows.count(group.kind) + group.birds.len() > cap {
            continue;
        }
        let model = models.as_ref().and_then(|m| m.get(group.kind));
        let birds = (0..group.birds.len())
            .map(|index| {
                let bird = commands
                    .spawn((
                        SparrowBird {
                            group: group.id,
                            index,
                            kind: group.kind,
                        },
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id();
                if let Some(model) = model {
                    commands.spawn((
                        SparrowModel3d {
                            kind: group.kind,
                            bird,
                        },
                        WorldAssetRoot(model.scene.clone()),
                        Transform::default(),
                        ChildOf(bird),
                    ));
                }
                bird
            })
            .collect();
        sparrows.live.push((group, birds));
    }
}

/// Hace avanzar a cada grupo con el jugador como amenaza y pone a cada pájaro
/// en su lugar, con su clip.
fn step_sparrows(
    time: Res<Time>,
    mut sparrows: ResMut<Sparrows>,
    focus: Option<Res<StreamFocus>>,
    terrain: Option<Res<WorldTerrain>>,
    mut birds: Query<(&mut Transform, &mut Visibility, Option<&mut SparrowRig>), With<SparrowBird>>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    models: Option<Res<SparrowModels>>,
) {
    let (Some(focus), Some(terrain)) = (focus, terrain) else {
        return;
    };
    let terrain = &terrain.0;
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let threat = Some((focus.0, ground(focus.0)));
    let dt = time.delta_secs().min(0.1);
    for (group, entities) in &mut sparrows.live {
        group.step(dt, threat, &ground);
        let model = models.as_ref().and_then(|m| m.get(group.kind));
        for (bird, &entity) in group.birds.iter().zip(entities.iter()) {
            let Ok((mut transform, mut visibility, rig)) = birds.get_mut(entity) else {
                continue;
            };
            let lift = match bird.perch {
                Some(perch) => surface_lift(perch),
                None => stand_height(group.kind),
            };
            *transform = Transform::from_translation(world_from_ground(bird.at, bird.y + lift))
                .with_rotation(
                    Quat::from_rotation_y(bird.heading.radians())
                        * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                );
            visibility.set_if_neq(if bird.gone() {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
            let (Some(mut rig), Some(model)) = (rig, model) else {
                continue;
            };
            let clip = bird.clip();
            let Ok((mut player, mut transitions)) = players.get_mut(rig.player) else {
                continue;
            };
            if rig.clip != Some(clip) {
                let blend = if rig.clip.is_some() { BLEND } else { 0.0 };
                transitions
                    .play(
                        &mut player,
                        model.nodes[clip_index(clip)],
                        std::time::Duration::from_secs_f32(blend),
                    )
                    .repeat()
                    .set_speed(bird.pace)
                    .seek_to(bird.clip_time());
                rig.clip = Some(clip);
            }
        }
    }
}

/// Lo que [`dress_sparrow`] lee de los nodos de un modelo.
type Part = (
    Option<&'static Mesh3d>,
    Option<&'static MeshMaterial3d<StandardMaterial>>,
    Option<&'static GltfMaterialName>,
    Option<&'static GltfMeshName>,
    Has<AnimationPlayer>,
);

/// Prepara un pájaro en el momento en que se crea su escena: cada malla en su
/// tramo de distancias (la completa cerca, la liviana lejos y sin sombra),
/// vestida con sus mapas, y su reproductor listo para sus clips.
#[allow(clippy::too_many_arguments)]
fn dress_sparrow(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    mut models: Option<ResMut<SparrowModels>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<&SparrowModel3d>,
    children: Query<&Children>,
    parts: Query<Part>,
) {
    let root = ready.entity;
    let (Ok(&bird), Some(models)) = (roots.get(root), models.as_mut()) else {
        return;
    };
    let Some(model) = models.get(bird.kind).cloned() else {
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
            commands.entity(entity).try_insert((
                AnimationGraphHandle(model.graph.clone()),
                AnimationTransitions::new(),
            ));
            commands.entity(bird.bird).try_insert(SparrowRig {
                player: entity,
                clip: None,
            });
        }
        if mesh.is_none() {
            continue;
        }
        let light = mesh_name.is_some_and(|n| n.0.ends_with(LIGHT_SUFFIX));
        let mut part = commands.entity(entity);
        // Sin sombra, ni cerca: la de las tarjetas de plumas con alpha test
        // en las cascadas costó unas 20 ms por cuadro con 13 gorriones en la
        // iGPU, y la de un pájaro de 14 cm es una mancha de pocos píxeles.
        part.try_insert((light_range(&base, light), NotShadowCaster));
        let skin = name.and_then(|n| skin(bird.kind, &n.0));
        let maps = skin.and_then(|s| model.maps.get(s.set).cloned());
        // Las tarjetas de plumas sin sus mapas serían rectángulos lisos.
        if skin.is_some_and(|s| s.masked) && maps.is_none() {
            part.try_insert(Visibility::Hidden);
            continue;
        }
        let Some(material) = material else { continue };
        let dressed = models
            .materials
            .entry((bird.kind, material.0.id()))
            .or_insert_with(|| {
                let mut m = materials.get(&material.0).cloned().unwrap_or_default();
                m.metallic = 0.0;
                m.perceptual_roughness = 0.9;
                if let Some(worn) = skin.and_then(|s| skinned(&m, s, maps)) {
                    m = worn;
                }
                // Como en las palomas: el plumaje del cuerpo deja pasar algo
                // del sol, así que en vuelo, desde abajo, se lee su color.
                if name.is_some_and(|n| n.0 == "Main") {
                    m.diffuse_transmission = FEATHER_TRANSMISSION;
                }
                materials.add(m)
            })
            .clone();
        part.try_insert(MeshMaterial3d(dressed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::town_buildings::{Lot, building, yaw};
    use pampa_core::town::TownConfig;
    use pampa_core::{BuildingKind, StoreStyle};

    fn castex() -> (TownLayout, Terrain) {
        (
            TownLayout::generate(&TownConfig::castex()).unwrap(),
            Terrain::default(),
        )
    }

    /// La altura más alta del edificio `i` de `town`, construido por el kit de
    /// fachadas, sobre el punto `p` del suelo, en el mundo (`None`: nada ahí).
    fn built_top(town: &TownLayout, terrain: &Terrain, i: usize, p: GroundVec2) -> Option<f32> {
        let b = town.buildings[i];
        let grid = town.grid();
        let c = b.footprint.center();
        let edge = (c.x - grid.min.x)
            .min(grid.max.x - c.x)
            .min(c.y - grid.min.y)
            .min(grid.max.y - c.y);
        let lot = Lot {
            building: b,
            variant: variant(i, &b.footprint),
            outskirts: edge < OUTSKIRTS,
            store: StoreStyle::of(town.size),
            veranda: town.hitching_post.veranda,
        };
        let shape = building(&lot)?;
        let base = world_from_ground(c, terrain.height(c.x, c.y));
        let local = yaw(b.front).inverse() * (world_from_ground(p, 0.0) - base);
        let (x, z) = (local.x, local.z);
        let mut top: Option<f32> = None;
        for surface in shape.surfaces().collect::<Vec<_>>() {
            for [a, b, c] in shape.triangles_of(surface) {
                // La recta vertical por (x, z) contra el triángulo.
                let d = (b.z - c.z) * (a.x - c.x) + (c.x - b.x) * (a.z - c.z);
                if d.abs() < 1e-9 {
                    continue;
                }
                let l1 = ((b.z - c.z) * (x - c.x) + (c.x - b.x) * (z - c.z)) / d;
                let l2 = ((c.z - a.z) * (x - c.x) + (a.x - c.x) * (z - c.z)) / d;
                let l3 = 1.0 - l1 - l2;
                if l1 >= -1e-5 && l2 >= -1e-5 && l3 >= -1e-5 {
                    let y = l1 * a.y + l2 * b.y + l3 * c.y + base.y;
                    top = Some(top.map_or(y, |t: f32| t.max(y)));
                }
            }
        }
        top
    }

    fn house_at(town: &TownLayout, p: GroundVec2) -> usize {
        town.buildings
            .iter()
            .position(|b| b.footprint.contains_point(p))
            .unwrap()
    }

    #[test]
    fn gorriones_perch_on_the_built_parapet_coping_and_never_on_a_cottage() {
        let (town, terrain) = castex();
        let roofs = town_roofs(&town, &terrain);
        assert!(roofs.len() >= 10, "{}", roofs.len());
        for spot in &roofs {
            let i = house_at(&town, spot.at);
            let across = GroundVec2::new(-spot.along.y, spot.along.x);
            for (offset, side) in [
                (-spot.reach, 0.0),
                (0.0, -1.0),
                (0.0, 1.0),
                (spot.reach, 0.0),
            ] {
                let (p, y) = spot.place(offset);
                let p = p + across * (side * spot.depth);
                let top = built_top(&town, &terrain, i, p).expect("over the house");
                assert!(
                    (top - y).abs() < 0.01,
                    "house {i}: coping at {top}, perch at {y}"
                );
            }
        }
        // Las casas de las afueras que son ranchos no tienen pretil: ahí no
        // hay albardilla a la altura del pretil, y no se posan en ellas.
        let every = roof_perches(&town, &terrain, |_| true);
        let mut cottages = 0;
        for spot in &every {
            let i = house_at(&town, spot.at);
            let b = &town.buildings[i];
            let grid = town.grid();
            let c = b.footprint.center();
            let edge = (c.x - grid.min.x)
                .min(grid.max.x - c.x)
                .min(c.y - grid.min.y)
                .min(grid.max.y - c.y);
            if is_cottage(edge < OUTSKIRTS, variant(i, &b.footprint)) {
                cottages += 1;
                assert!(!roofs.contains(spot));
                let top = built_top(&town, &terrain, i, spot.at);
                assert!(top.is_none_or(|t| (t - spot.y).abs() > 0.05), "{top:?}");
            }
        }
        assert!(cottages > 0);
        assert!(roofs.iter().all(|s| every.contains(s)));
        assert_eq!(
            town.buildings[house_at(&town, roofs[0].at)].kind,
            BuildingKind::House
        );
    }

    /// Una app headless con el plugin, Castex, un alambrado este–oeste de
    /// 2 km al norte del pueblo, el jugador en `focus` y la hora `hour`.
    fn app(focus: GroundVec2, hour: u32, models: bool) -> App {
        use pampa_core::{GameClock, TimeOfDay};
        let (town, terrain) = castex();
        let fences = field_fence();
        let mut app = if models {
            use crate::gltf::tests::{root, unfinished_app};
            let mut app = unfinished_app(&root());
            app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
                .add_plugins(bevy::animation::AnimationPlugin)
                .insert_resource(root());
            app
        } else {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app
        };
        app.insert_resource(WorldTowns(vec![town]))
            .insert_resource(WorldTerrain(terrain))
            .insert_resource(WorldFences(fences))
            .insert_resource(WorldChunks::default())
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(hour, 0))))
            .add_plugins(SparrowPlugin);
        app.finish();
        app.cleanup();
        app.update();
        app
    }

    /// Un alambrado este–oeste de 2 km, al norte del pueblo.
    fn field_fence() -> pampa_core::FenceLayout {
        let posts = (0..=200)
            .map(|k| GroundVec2::new(-1000.0 + 10.0 * k as f32, 600.0))
            .collect();
        pampa_core::FenceLayout {
            runs: vec![pampa_core::FenceRun {
                posts,
                roadside: true,
            }],
            ..pampa_core::FenceLayout::default()
        }
    }

    fn plaza() -> GroundVec2 {
        castex().0.plaza.center()
    }

    #[test]
    fn sparrows_stream_near_the_player_by_day_up_to_their_caps_and_leave_when_far() {
        let mut town = app(plaza(), 13, false);
        let sparrows = town.world().resource::<Sparrows>();
        let gorriones = sparrows.count(SparrowKind::Gorrion);
        assert!((3..=MAX_GORRIONES).contains(&gorriones), "{gorriones}");
        assert_eq!(sparrows.count(SparrowKind::Chingolo), 0);
        for (g, birds) in &sparrows.live {
            assert!(g.patch.area.center().distance(plaza()) <= SPAWN_RADIUS);
            assert_eq!(birds.len(), g.birds.len());
        }
        let drawn = town
            .world_mut()
            .query::<&SparrowBird>()
            .iter(town.world())
            .count();
        assert_eq!(drawn, gorriones);
        // En el campo, junto al alambrado: chingolos, uno o dos por grupo.
        let fences = field_fence();
        let terrain = Terrain::default();
        let size = WorldChunks::default().0.chunk_size;
        let seeded = (-5..5)
            .flat_map(|x| {
                let coord = ChunkCoord::containing(GroundVec2::new(x as f32 * size, 600.0), size);
                fence_sparrows(&fences, coord, size, &terrain, terrain.seed ^ SEED, |_| {
                    true
                })
            })
            .next()
            .expect("chingolos along 2 km of fence");
        let field = app(seeded.patch.area.center(), 13, false);
        let sparrows = field.world().resource::<Sparrows>();
        let chingolos = sparrows.count(SparrowKind::Chingolo);
        assert!((1..=MAX_CHINGOLOS).contains(&chingolos), "{chingolos}");
        assert_eq!(sparrows.count(SparrowKind::Gorrion), 0);
        // De noche, ninguno.
        let night = app(plaza(), 23, false);
        assert!(night.world().resource::<Sparrows>().live.is_empty());
        // Lejos, se van todos.
        town.world_mut().resource_mut::<StreamFocus>().0 = plaza() + GroundVec2::new(0.0, -2000.0);
        town.update();
        assert!(town.world().resource::<Sparrows>().live.is_empty());
        let left = town
            .world_mut()
            .query::<&SparrowBird>()
            .iter(town.world())
            .count();
        assert_eq!(left, 0);
    }

    #[test]
    fn sparrows_draw_their_own_models_at_life_size_staggered_light_far_and_without_shadows() {
        use bevy::gltf::GltfMeshName;
        let mut app = app(plaza(), 13, true);
        let birds = app
            .world()
            .resource::<Sparrows>()
            .count(SparrowKind::Gorrion);
        let mut rigs = app.world_mut().query::<&SparrowRig>();
        for _ in 0..3000 {
            app.update();
            if rigs.iter(app.world()).filter(|r| r.clip.is_some()).count() == birds {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let world = app.world_mut();
        assert_eq!(rigs.iter(world).filter(|r| r.clip.is_some()).count(), birds);
        // Cada uno desde su propio punto de su clip.
        let mut players = world.query::<&AnimationPlayer>();
        let mut starts: Vec<f32> = players
            .iter(world)
            .filter_map(|p| p.playing_animations().next().map(|a| a.1.seek_time()))
            .collect();
        assert_eq!(starts.len(), birds);
        starts.sort_by(f32::total_cmp);
        assert!(starts[birds - 1] - starts[0] > 0.5, "{starts:?}");
        // La malla completa cerca, la liviana lejos, ninguna con sombra.
        let mut meshes = world.query::<(&GltfMeshName, &VisibilityRange, Has<NotShadowCaster>)>();
        let (mut full, mut light) = (0, 0);
        for (name, range, shadowless) in meshes.iter(world) {
            if name.0.ends_with(LIGHT_SUFFIX) {
                assert_eq!(range.start_margin.end, crate::fauna::livestock::LIGHT_FROM);
                assert!(shadowless);
                light += 1;
            } else {
                assert_eq!(range.end_margin.end, crate::fauna::livestock::LIGHT_FROM);
                assert!(shadowless);
                full += 1;
            }
        }
        assert!(full >= birds && light >= birds, "{full} {light}");
        // El cuerpo deja pasar algo del sol.
        let mut parts = world.query::<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>();
        let assets = world.resource::<Assets<StandardMaterial>>();
        let lit: Vec<f32> = parts
            .iter(world)
            .filter(|(n, _)| n.0 == "Main")
            .map(|(_, m)| assets.get(&m.0).unwrap().diffuse_transmission)
            .collect();
        assert!(!lit.is_empty() && lit.iter().all(|t| *t > 0.0));
        // A tamaño real, sobre su superficie.
        let mut roots = world.query::<(&Transform, &SparrowBird)>();
        assert!(roots.iter(world).all(|(t, _)| t.scale == Vec3::ONE));
        let mut models = world.query_filtered::<&Transform, With<SparrowModel3d>>();
        assert!(models.iter(world).all(|t| *t == Transform::default()));
    }
}
