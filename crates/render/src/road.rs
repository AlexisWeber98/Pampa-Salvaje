//! Caminos de tierra, pasos a nivel y postes indicadores, cargados por
//! streaming junto con los chunks de terreno, como la línea ferroviaria entre
//! pueblos.
//!
//! `pampa-core` decide por dónde corre cada camino ([`RoadNetwork`], que lleva
//! el terreno, recortado a lo largo de él). Cuando se construye un chunk de
//! terreno, las partes de los caminos que caen en él se tienden como una malla
//! de superficie, hija suya, con los pasos a nivel y los postes indicadores
//! ubicados en él; se van cuando se va el chunk.
//!
//! La superficie lleva tierra compactada Ground 102 de ambientCG (CC0 1.0; ver
//! assets/textures/ground/CREDITS.md, cargada por
//! [`crate::ground::road_material`]). Sus bordes se difuminan hacia el suelo
//! mediante el alfa de vértice, y el suelo bajo ellos está gastado hasta la
//! tierra (ver [`crate::terrain`]). Un paso a nivel recibe tablones de madera
//! entre los rieles y junto a ellos, rampas que suben hasta ellos y una Cruz de
//! San Andrés sobre un poste a cada lado; un poste indicador es un poste de
//! madera con una tabla en flecha por pueblo, rotulada en la tipografía de los
//! carteles. Los carteles y los postes indicadores son [`ThinOccluder`]s: se
//! desvanecen cuando tapan al jinete.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{ChunkCoord, LevelCrossing, RoadNetwork, RoadPoint, Signpost, Vec2 as GroundVec2};

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::flora::ThinOccluder;
use crate::ground::{face_tangent, ground_uv, road_material};
use crate::signs::{BoardStyle, board_image, layout_board};
use crate::terrain::{
    ChunkDetail, ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain, mesh_height,
};

/// La parte de la superficie de camino de un chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoadSurface;

/// Un tablón, rampa, poste o tabla de un paso a nivel.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossingPart;

/// Un poste o tabla de un poste indicador de camino.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignpostPart;

/// Marca un chunk cuyos caminos ya se tendieron (aunque no pase ninguno por
/// él).
#[derive(Component, Debug, Clone, Copy)]
struct RoadLaid;

/// Mallas y materiales compartidos por todas las piezas de camino.
#[derive(Resource, Debug, Clone)]
struct RoadKit {
    surface: Handle<StandardMaterial>,
    cube: Handle<Mesh>,
    timber: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    black: Handle<StandardMaterial>,
}

/// Tiende los caminos del [`WorldTerrain`] sobre los chunks de terreno a medida
/// que se construyen. Necesita el [`crate::TerrainPlugin`].
pub struct RoadPlugin;

impl Plugin for RoadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            .add_systems(Startup, setup_kit)
            // Justo después del streaming, como la vía y la flora.
            .add_systems(
                PostUpdate,
                lay_roads
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn setup_kit(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    let plain = |r: f32, g: f32, b: f32, roughness: f32| StandardMaterial {
        base_color: Color::srgb(r, g, b),
        perceptual_roughness: roughness,
        ..default()
    };
    commands.insert_resource(RoadKit {
        surface: materials.add(road_material(
            root.as_deref().map(|r| r.0.as_path()),
            images.as_deref_mut(),
        )),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        timber: materials.add(plain(0.40, 0.31, 0.22, 0.92)),
        white: materials.add(plain(0.86, 0.84, 0.78, 0.7)),
        black: materials.add(plain(0.09, 0.08, 0.08, 0.7)),
    });
}

/// Almacenes de assets a los que se agregan las piezas de los caminos.
type RoadAssets<'a> = (
    ResMut<'a, Assets<Mesh>>,
    ResMut<'a, Assets<StandardMaterial>>,
    Option<ResMut<'a, Assets<Image>>>,
);

/// Tiende como hijos de cada chunk recién construido los caminos que caen en
/// él. Un chunk que desaparece antes de que se apliquen los comandos
/// simplemente no recibe ninguno.
fn lay_roads(
    mut commands: Commands,
    (terrain, world, detail): (
        Res<WorldTerrain>,
        Res<WorldChunks>,
        Option<Res<ChunkDetail>>,
    ),
    kit: Option<Res<RoadKit>>,
    chunks: Query<(Entity, &TerrainChunk), Without<RoadLaid>>,
    (mut meshes, mut materials, mut images): RoadAssets,
) {
    let (Some(kit), Some(detail)) = (kit, detail) else {
        return;
    };
    let terrain = &terrain.0;
    let size = world.0.chunk_size;
    for (chunk, coord) in &chunks {
        let mut laid = commands.entity(chunk);
        laid.try_insert(RoadLaid);
        let Some(roads) = terrain.roads.as_deref() else {
            continue;
        };
        let area = coord.0.bounds(size);
        let mut pieces: Vec<RoadBundle> = Vec::new();
        let ground = |p: GroundVec2| mesh_height(terrain, p, size, detail.resolution);
        if let Some(mesh) = surface_mesh(&roads.parts_in(area), &ground, roads) {
            pieces.push((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(kit.surface.clone()),
                Transform::default(),
                Piece::Surface,
                None,
            ));
        }
        for crossing in roads.crossings().filter(|c| owns(coord.0, size, c.at)) {
            let signs = crossing.sign_spots();
            let posted = signs_posted(crossing, roads.crossings());
            for (white, transform) in crossing_pieces(crossing, &ground, posted) {
                // Los carteles se desvanecen cuando estorban a la cámara; los
                // tablones quedan planos.
                let at = GroundVec2::new(transform.translation.x, -transform.translation.z);
                let fade = signs
                    .into_iter()
                    .find(|s| s.distance(at) < 1.0)
                    .map(|s| ThinOccluder::post(s, 0.8, terrain, 3.2));
                let material = match white {
                    Paint::Timber => kit.timber.clone(),
                    Paint::White => kit.white.clone(),
                    Paint::Black => kit.black.clone(),
                };
                pieces.push((
                    Mesh3d(kit.cube.clone()),
                    MeshMaterial3d(material),
                    transform,
                    Piece::Crossing,
                    fade,
                ));
            }
        }
        for post in roads.signposts.iter().filter(|s| owns(coord.0, size, s.at)) {
            let fade = ThinOccluder::post(post.at, BOARD_WIDTH, terrain, POST_HEIGHT);
            for (mesh, material, transform) in signpost_pieces(
                post,
                ground(post.at),
                &kit,
                (&mut meshes, &mut materials, images.as_deref_mut()),
            ) {
                pieces.push((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    transform,
                    Piece::Signpost,
                    Some(fade),
                ));
            }
        }
        if pieces.is_empty() {
            continue;
        }
        laid.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|road| {
                for (mesh, material, transform, piece, fade) in pieces {
                    let mut part = road.spawn((mesh, material, transform));
                    if let Some(fade) = fade {
                        part.insert(fade);
                    }
                    match piece {
                        Piece::Surface => part.insert(RoadSurface),
                        Piece::Crossing => part.insert(CrossingPart),
                        Piece::Signpost => part.insert(SignpostPart),
                    };
                }
            });
        });
    }
}

/// Si el punto `p` (un paso a nivel o un poste indicador) lo dibuja el chunk
/// `coord`: los chunks son dueños de sus bordes oeste y sur, no de los de este
/// y norte, así que un punto sobre un borde se dibuja una sola vez.
fn owns(coord: ChunkCoord, size: f32, p: GroundVec2) -> bool {
    ChunkCoord::containing(p, size) == coord
}

/// Una entidad a crear bajo un chunk, y su forma de fundido si la tiene.
type RoadBundle = (
    Mesh3d,
    MeshMaterial3d<StandardMaterial>,
    Transform,
    Piece,
    Option<ThinOccluder>,
);

#[derive(Clone, Copy)]
enum Piece {
    Surface,
    Crossing,
    Signpost,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Paint {
    Timber,
    White,
    Black,
}

/// Cuánto flota la superficie sobre el suelo dibujado, en metros.
const LIFT: f32 = 0.05;
/// Ancho de la franja difuminada a lo largo de cada borde del camino, en
/// metros, mitad dentro del camino y mitad más allá de su borde.
const FEATHER: f32 = 1.6;
/// El tinte de tierra del camino (sRGB), sobre el detalle casi blanco de
/// Ground 102.
const DIRT: [f32; 3] = [0.70, 0.60, 0.46];

/// Una malla de superficie para todas las partes `part` de camino de un chunk:
/// una tira a lo largo de cada parte, opaca a lo ancho del camino y difuminada
/// hasta transparente más allá de sus bordes, tendida sobre `ground`. `None`
/// cuando no hay ninguna parte.
///
/// En un paso a nivel la tira es el acceso de tierra apisonada: se apoya sobre
/// el mismo perfil que suben los jinetes ([`RoadNetwork::crossing_rise`]),
/// subiendo sobre el eje del camino hasta la plataforma del paso (como mucho
/// alrededor de 1 en 10), con sus bordes difuminados bajando hasta el suelo a
/// cada lado, así que no muestra ninguna pared ni ningún hueco debajo. Cerca de
/// un paso a nivel la tira se muestrea cada [`RAMP_STEP`] metros para seguir la
/// subida.
pub(crate) fn surface_mesh(
    parts: &[Vec<RoadPoint>],
    ground: &impl Fn(GroundVec2) -> f32,
    roads: &RoadNetwork,
) -> Option<Mesh> {
    let half = RoadNetwork::HALF_WIDTH;
    let across = [
        (-half - FEATHER / 2.0, 0.0),
        (-half + FEATHER / 2.0, 1.0),
        (half - FEATHER / 2.0, 1.0),
        (half + FEATHER / 2.0, 0.0),
    ];
    let tint = LinearRgba::from(Color::srgb(DIRT[0], DIRT[1], DIRT[2]));
    let (mut positions, mut colors, mut uvs, mut tangents) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for part in parts.iter().filter(|p| p.len() > 1) {
        let row = |p: &RoadPoint| {
            across.map(|(offset, alpha)| {
                let g = p.at + p.normal * offset;
                // En el acceso al paso, el perfil sobre el que se apoyan los
                // cuerpos (con sus bordes difuminados abajo, sobre el suelo),
                // un centímetro por debajo de los tablones.
                let lift = LIFT.max(roads.crossing_rise(g) - 0.01);
                (
                    world_from_ground(g, ground(g) + lift),
                    [tint.red, tint.green, tint.blue, alpha],
                )
            })
        };
        let rows: Vec<_> = near_crossings(part, roads).iter().map(row).collect();
        for w in rows.windows(2) {
            for k in 0..across.len() - 1 {
                let (r0, r1, l1, l0) = (w[0][k], w[1][k], w[1][k + 1], w[0][k + 1]);
                // En sentido antihorario visto desde arriba.
                for tri in [[r0, r1, l1], [r0, l1, l0]] {
                    let points = tri.map(|(v, _)| v);
                    let uv = points.map(ground_uv);
                    positions.extend(points.map(|v| v.to_array()));
                    colors.extend(tri.map(|(_, c)| c));
                    tangents.extend([face_tangent(points, uv); 3]);
                    uvs.extend(uv);
                }
            }
        }
    }
    if positions.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh.compute_normals();
    Some(mesh)
}

/// Separación entre las muestras de la tira a lo largo del acceso a un paso a
/// nivel, en metros.
const RAMP_STEP: f32 = 0.5;

/// `part` con puntos extra cada [`RAMP_STEP`] metros dondequiera que el camino
/// sube hacia un paso a nivel, para que la tira siga la subida con suavidad.
fn near_crossings(part: &[RoadPoint], roads: &RoadNetwork) -> Vec<RoadPoint> {
    let reach = RoadNetwork::CROSSING_DECK + RoadNetwork::CROSSING_RAMP + 1.0;
    let mut out = Vec::with_capacity(part.len());
    for w in part.windows(2) {
        let (a, b) = (w[0], w[1]);
        out.push(a);
        let d = b.at - a.at;
        let rises = roads.crossings().any(|c| {
            let t = ((c.at - a.at).dot(d) / d.dot(d).max(1e-9)).clamp(0.0, 1.0);
            c.at.distance(a.at + d * t) < reach
        });
        if !rises {
            continue;
        }
        let steps = (a.at.distance(b.at) / RAMP_STEP).ceil() as usize;
        for k in 1..steps {
            let t = k as f32 / steps as f32;
            out.push(RoadPoint {
                at: a.at + (b.at - a.at) * t,
                normal: a.normal,
                along: a.along + (b.along - a.along) * t,
            });
        }
    }
    out.extend(part.last().copied());
    out
}

/// Cara superior de los rieles sobre el suelo, en metros (ver
/// `crate::railbed`).
use crate::railbed::RAIL_TOP;
/// Ancho y espesor de los tablones, en metros.
const PLANK: Vec2 = Vec2::new(0.24, 0.1);
/// Centros de los tablones a lo ancho de la vía, desde su eje, en metros: cinco
/// entre los rieles (separados 1.676 m, con un canal para la pestaña dentro de
/// cada riel) y dos por fuera de cada riel, una franja de cerca de medio metro.
const PLANKS: [f32; 9] = [-1.24, -1.0, -0.54, -0.27, 0.0, 0.27, 0.54, 1.0, 1.24];

/// Una caja desde el punto del mundo `from` hasta `to` (la línea media de su
/// arista superior), de `width` de ancho y `thickness` de espesor.
fn slab(from: Vec3, to: Vec3, width: f32, thickness: f32) -> Option<Transform> {
    let d = to - from;
    let len = d.length();
    (len > 0.0).then(|| {
        let rotation = Quat::from_rotation_arc(Vec3::X, d / len);
        Transform::from_translation((from + to) / 2.0 - rotation * Vec3::Y * thickness / 2.0)
            .with_rotation(rotation)
            .with_scale(Vec3::new(len, thickness, width))
    })
}

/// El giro que lleva el X local de una caja a la dirección del suelo `d`.
fn yaw(d: GroundVec2) -> Quat {
    Quat::from_rotation_y(d.y.atan2(d.x))
}

/// Qué tan separados pueden estar (en metros) dos pasos a nivel de un mismo
/// camino para que se considere que el camino cruza un solo tramo de vía, con
/// un único par de carteles.
const SHARED_SIGNS: f32 = 20.0;

/// Cuáles de los dos carteles de `crossing` se levantan (primero el del acceso
/// desde atrás según el sentido del camino, luego el de adelante): un camino
/// que cruza dos vías lado a lado (la vía de una estación y su vía muerta) se
/// señaliza una vez a cada lado de ambas, no en cada vía, así que un paso a
/// nivel que tiene otro del mismo camino más afuera en un lado le deja el
/// cartel de ese lado a ese otro.
fn signs_posted<'a>(
    crossing: &LevelCrossing,
    all: impl Iterator<Item = &'a LevelCrossing>,
) -> [bool; 2] {
    let mut posted = [true; 2];
    for other in all {
        let apart = other.at - crossing.at;
        let same_road = other.road.dot(crossing.road).abs() > 0.9;
        if !same_road || apart.length() > SHARED_SIGNS || apart.length() < 1e-3 {
            continue;
        }
        let ahead = apart.dot(crossing.road);
        if ahead < -0.5 {
            posted[0] = false;
        }
        if ahead > 0.5 {
            posted[1] = false;
        }
    }
    posted
}

/// Los tablones y las dos Cruces de San Andrés de `crossing`, cada uno un cubo
/// unitario con su transformación, apoyados sobre `height`: el suelo dibujado,
/// como la superficie del camino, para que nada flote sobre él ni se hunda en
/// él. Los tablones quedan al ras de las cabezas de los rieles, entre los
/// rieles y en una franja de cerca de medio metro por fuera de cada uno,
/// exactamente del ancho del camino a lo largo de la vía. El camino mismo sube
/// hasta ellos (ver [`surface_mesh`]): sin rampas de madera.
fn crossing_pieces(
    crossing: &LevelCrossing,
    height: &impl Fn(GroundVec2) -> f32,
    posted: [bool; 2],
) -> Vec<(Paint, Transform)> {
    let rail = crossing.rail;
    let across = GroundVec2::new(-rail.y, rail.x);
    // Los tablones cubren el camino donde cruza, medidos a lo largo de la vía.
    let span = RoadNetwork::WIDTH / crossing.angle().sin().max(0.3);
    let mut pieces = Vec::new();
    for offset in PLANKS {
        let c = crossing.at + across * offset;
        let (a, b) = (c - rail * (span / 2.0), c + rail * (span / 2.0));
        let top = |p: GroundVec2| world_from_ground(p, height(p) + RAIL_TOP - 0.02);
        if let Some(t) = slab(top(a), top(b), PLANK.x, PLANK.y) {
            pieces.push((Paint::Timber, t));
        }
    }
    for ((spot, facing), posted) in crossing
        .sign_spots()
        .into_iter()
        .zip([crossing.road * -1.0, crossing.road])
        .zip(posted)
    {
        if !posted {
            continue;
        }
        pieces.extend(cross_sign(spot, facing, height(spot)));
    }
    pieces
}

/// Una Cruz de San Andrés sobre su poste en el punto del suelo `at`, con el
/// frente girado hacia la dirección del suelo `facing` (el tránsito al que
/// advierte).
fn cross_sign(at: GroundVec2, facing: GroundVec2, ground: f32) -> Vec<(Paint, Transform)> {
    let post_height = 3.0;
    let post = Transform::from_translation(world_from_ground(at, ground + post_height / 2.0))
        .with_scale(Vec3::new(0.12, post_height, 0.12));
    // X local a lo ancho del tránsito, Z local hacia él.
    let turn = yaw(GroundVec2::new(facing.y, -facing.x));
    let center = world_from_ground(at, ground + post_height - 0.55) + turn * Vec3::Z * 0.08;
    let mut pieces = vec![(Paint::Timber, post)];
    for tilt in [0.7, -0.7] {
        let r = turn * Quat::from_rotation_z(tilt);
        pieces.push((
            Paint::Black,
            Transform::from_translation(center)
                .with_rotation(r)
                .with_scale(Vec3::new(1.3, 0.24, 0.04)),
        ));
        pieces.push((
            Paint::White,
            Transform::from_translation(center + turn * Vec3::Z * 0.025)
                .with_rotation(r)
                .with_scale(Vec3::new(1.22, 0.17, 0.03)),
        ));
    }
    pieces
}

/// Altura de un poste indicador y tamaño de sus tablas en flecha, en metros.
const POST_HEIGHT: f32 = 2.5;
const BOARD_WIDTH: f32 = 1.7;
const BOARD_HEIGHT: f32 = 0.34;

type Asset3d = (Handle<Mesh>, Handle<StandardMaterial>, Transform);

/// El poste y las tablas en flecha de un poste indicador, con su pie a la
/// altura del suelo dibujado `ground`: cada tabla apunta a lo largo de su
/// camino, hacia afuera del poste, y se lee desde ambas caras.
fn signpost_pieces(
    post: &Signpost,
    ground: f32,
    kit: &RoadKit,
    (meshes, materials, images): (
        &mut Assets<Mesh>,
        &mut Assets<StandardMaterial>,
        Option<&mut Assets<Image>>,
    ),
) -> Vec<Asset3d> {
    let mut pieces = vec![(
        kit.cube.clone(),
        kit.timber.clone(),
        Transform::from_translation(world_from_ground(post.at, ground + POST_HEIGHT / 2.0))
            .with_scale(Vec3::new(0.14, POST_HEIGHT, 0.14)),
    )];
    let mut images = images;
    for (i, board) in post.boards.iter().enumerate() {
        let layout = layout_board(&board.text, BOARD_WIDTH, BOARD_HEIGHT);
        let (w, h) = (layout.width_m, layout.height_m);
        let turn = yaw(board.toward);
        let center = world_from_ground(post.at, ground + POST_HEIGHT - 0.3 - i as f32 * 0.45)
            + turn * Vec3::X * (w / 2.0);
        // La tabla, con una punta aguzada en la dirección de la flecha.
        pieces.push((
            kit.cube.clone(),
            kit.timber.clone(),
            Transform::from_translation(center)
                .with_rotation(turn)
                .with_scale(Vec3::new(w + 0.06, h + 0.06, 0.05)),
        ));
        let tip = h * std::f32::consts::FRAC_1_SQRT_2;
        pieces.push((
            kit.cube.clone(),
            kit.timber.clone(),
            Transform::from_translation(center + turn * Vec3::X * (w / 2.0))
                .with_rotation(turn * Quat::from_rotation_z(std::f32::consts::FRAC_PI_4))
                .with_scale(Vec3::new(tip, tip, 0.05)),
        ));
        let Some(images) = images.as_deref_mut() else {
            continue;
        };
        let seed = post.at.x * 0.37 + i as f32;
        let face = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(board_image(
                &board.text,
                layout,
                BoardStyle::signpost(),
                seed,
            ))),
            perceptual_roughness: 0.9,
            reflectance: 0.2,
            ..default()
        });
        let quad = meshes.add(Rectangle::new(w, h));
        for (side, flip) in [
            (1.0, Quat::IDENTITY),
            (-1.0, Quat::from_rotation_y(std::f32::consts::PI)),
        ] {
            pieces.push((
                quad.clone(),
                face.clone(),
                Transform::from_translation(center + turn * Vec3::Z * (side * 0.03))
                    .with_rotation(turn * flip),
            ));
        }
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lettering::{self, Face};
    use crate::{StreamFocus, TerrainPlugin};
    use pampa_core::{
        PlazaPlacement, RailLine, RailwayConfig, Side, Terrain, TownConfig, TownLayout,
        WorldConfig, place_towns,
    };
    use std::sync::Arc;

    /// Dos pueblos ferroviarios a 3 km de distancia en una misma fila, su línea
    /// y sus caminos.
    fn world() -> (WorldConfig, Vec<TownLayout>, Terrain) {
        let config = WorldConfig::default();
        let town = |name: &str, x: f32, seed| TownConfig {
            name: name.into(),
            origin: GroundVec2::new(x, 0.0),
            blocks: (4, 4),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        let (towns, errors) = place_towns(
            &config,
            &[town("Oeste", -1500.0, 1), town("Este", 1500.0, 2)],
        );
        assert!(errors.is_empty(), "{errors:?}");
        let (line, errors) = RailLine::build(None, &towns, config.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        let (roads, errors) = RoadNetwork::build(&towns, &line, config.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        let terrain = Terrain::default().with_roads(Arc::new(roads));
        (config, towns, terrain)
    }

    fn app(focus: GroundVec2) -> App {
        let (config, _, terrain) = world();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldChunks(config))
            .insert_resource(WorldTerrain(terrain))
            .insert_resource(StreamFocus(focus))
            .add_plugins((
                TerrainPlugin {
                    radius: 1,
                    budget: 1,
                    resolution: 8,
                },
                RoadPlugin,
            ));
        app
    }

    fn count<T: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<T>>()
            .iter(app.world())
            .count()
    }

    fn chunk_of<T: Component>(app: &mut App) -> Vec<ChunkCoord> {
        let mut q = app.world_mut().query_filtered::<&ChildOf, With<T>>();
        let parents: Vec<Entity> = q.iter(app.world()).map(|c| c.parent()).collect();
        let mut chunks = app.world_mut().query::<&TerrainChunk>();
        parents
            .into_iter()
            .map(|p| chunks.get(app.world(), p).unwrap().0)
            .collect()
    }

    #[test]
    fn road_chunks_get_their_surface_crossing_and_signposts_and_drop_them() {
        let (config, towns, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        // En la huella rural del pueblo del oeste, junto a su paso a nivel.
        let crossing = roads
            .crossings()
            .find(|c| towns[0].leveled().contains_point(c.at))
            .copied()
            .unwrap();
        let mut app = app(crossing.at);
        app.update();
        app.update();
        let surfaces = chunk_of::<RoadSurface>(&mut app);
        assert!(surfaces.contains(&config.chunk_of(crossing.at)));
        // Cada chunk con una parte de camino tiene exactamente una superficie;
        // ningún otro.
        let mut loaded = app.world_mut().query::<&TerrainChunk>();
        let loaded: Vec<ChunkCoord> = loaded.iter(app.world()).map(|c| c.0).collect();
        for c in &loaded {
            let has = !roads.parts_in(c.bounds(config.chunk_size)).is_empty();
            let n = surfaces.iter().filter(|s| *s == c).count();
            assert_eq!(n, usize::from(has), "{c:?}");
        }
        // Tablones y dos carteles sobre postes, en el chunk del paso a nivel.
        let parts = chunk_of::<CrossingPart>(&mut app);
        assert_eq!(
            parts
                .iter()
                .filter(|c| **c == config.chunk_of(crossing.at))
                .count(),
            PLANKS.len() + 2 * 5
        );
        // Se cabalga hasta un poste indicador: su chunk lo recibe, con poste,
        // tabla, punta y caras.
        let post = roads.signposts[0].at;
        app.insert_resource(StreamFocus(post));
        for _ in 0..20 {
            app.update();
        }
        let posts = chunk_of::<SignpostPart>(&mut app);
        assert_eq!(
            posts
                .iter()
                .filter(|c| **c == config.chunk_of(post))
                .count(),
            1 + 2 + 2
        );
        let fading = app
            .world_mut()
            .query_filtered::<(), (With<SignpostPart>, With<ThinOccluder>)>()
            .iter(app.world())
            .count();
        assert_eq!(fading, count::<SignpostPart>(&mut app));
        // Se cabalga lejos: los chunks se van con sus caminos.
        app.insert_resource(StreamFocus(GroundVec2::new(-3500.0, 3500.0)));
        for _ in 0..40 {
            app.update();
        }
        assert_eq!(count::<RoadSurface>(&mut app), 0);
        assert_eq!(count::<CrossingPart>(&mut app), 0);
        assert_eq!(count::<SignpostPart>(&mut app), 0);
    }

    #[test]
    fn the_surface_lies_on_the_drawn_ground_and_feathers_at_its_edges() {
        let (config, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let (a, _) = roads.segments().nth(200).unwrap();
        let area = config.chunk_of(a).bounds(config.chunk_size);
        let ground = |p: GroundVec2| mesh_height(&terrain, p, config.chunk_size, 40);
        let mesh = surface_mesh(&roads.parts_in(area), &ground, &roads).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours")
        };
        for (p, c) in points.iter().zip(colors) {
            let g = GroundVec2::new(p[0], -p[2]);
            assert!((p[1] - ground(g) - LIFT).abs() < 1e-4);
            let d = roads.distance(g);
            // Opaca sobre el camino, transparente más allá de su borde.
            if c[3] == 1.0 {
                assert!(d < RoadNetwork::HALF_WIDTH);
            } else {
                assert!(d > RoadNetwork::HALF_WIDTH);
            }
        }
        // Las normales apuntan hacia arriba: la tira se enrolla en sentido
        // antihorario vista desde arriba.
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals")
        };
        assert!(normals.iter().all(|n| n[1] > 0.9));
    }

    #[test]
    fn drawn_ground_heights_match_the_chunk_mesh_corners_and_stay_continuous() {
        let (config, _, terrain) = world();
        let size = config.chunk_size;
        for &(x, y) in &[(0.0, 0.0), (5.0, 5.0), (200.0, -15.0), (-35.0, 15.0)] {
            let p = GroundVec2::new(x, y);
            assert!((mesh_height(&terrain, p, size, 40) - terrain.height(x, y)).abs() < 1e-4);
        }
        let mut last = mesh_height(&terrain, GroundVec2::new(-30.0, 3.3), size, 40);
        for k in 1..2000 {
            let p = GroundVec2::new(-30.0 + k as f32 * 0.03, 3.3 + k as f32 * 0.011);
            let h = mesh_height(&terrain, p, size, 40);
            assert!((h - last).abs() < 0.02, "{p:?}");
            last = h;
        }
    }

    #[test]
    fn a_crossing_or_signpost_on_a_chunk_border_belongs_to_one_chunk() {
        let size = WorldConfig::default().chunk_size;
        for p in [
            GroundVec2::new(size, size),
            GroundVec2::new(size, size * 0.5),
            GroundVec2::new(-size * 0.5, 0.0),
            GroundVec2::new(-size, -size),
        ] {
            let owners = (-2..=2)
                .flat_map(|x| (-2..=2).map(move |y| ChunkCoord::new(x, y)))
                .filter(|&c| owns(c, size, p))
                .count();
            assert_eq!(owners, 1, "{p:?}");
        }
    }

    /// Hace avanzar la app hasta que el chunk bajo `at` tiene sus piezas de
    /// camino.
    fn laid_at(at: GroundVec2) -> (App, Terrain, f32) {
        let (config, _, terrain) = world();
        let mut app = app(at);
        for _ in 0..4 {
            app.update();
        }
        (app, terrain, config.chunk_size)
    }

    #[test]
    fn crossing_parts_and_signposts_stand_on_the_drawn_ground() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let crossing = *roads.crossings().next().unwrap();
        let (mut app, terrain, size) = laid_at(crossing.at);
        // Las mallas de chunk de la app de prueba tienen 8 quads por lado.
        let drawn = |p: GroundVec2| mesh_height(&terrain, p, size, 8);
        let mut q = app
            .world_mut()
            .query_filtered::<&Transform, With<CrossingPart>>();
        let parts: Vec<Transform> = q.iter(app.world()).copied().collect();
        let planks: Vec<&Transform> = parts.iter().filter(|t| t.scale.z == PLANK.x).collect();
        assert_eq!(planks.len(), PLANKS.len());
        for t in planks {
            // Ambos extremos de la arista superior del tablón (es recta; el
            // suelo dibujado puede doblarse entre ellos donde se juntan sus
            // triángulos).
            let mid = t.translation + t.rotation * Vec3::Y * (t.scale.y / 2.0);
            for end in [-1.0, 1.0] {
                let p = mid + t.rotation * Vec3::X * (end * t.scale.x / 2.0);
                let top = p.y - drawn(GroundVec2::new(p.x, -p.z));
                assert!((top - (RAIL_TOP - 0.02)).abs() < 1e-3, "{top}");
            }
        }
        // Los postes de las cruces se apoyan sobre el suelo dibujado.
        let posts: Vec<&Transform> = parts.iter().filter(|t| t.scale.y == 3.0).collect();
        assert_eq!(posts.len(), 2);
        for t in posts {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            assert!((t.translation.y - 1.5 - drawn(g)).abs() < 1e-3);
        }
        let post = roads.signposts[0].at;
        let (mut app, terrain, size) = laid_at(post);
        let mut q = app
            .world_mut()
            .query_filtered::<&Transform, With<SignpostPart>>();
        let foot = q
            .iter(app.world())
            .find(|t| t.scale.y == POST_HEIGHT)
            .map(|t| t.translation.y - POST_HEIGHT / 2.0)
            .expect("a signpost");
        assert!((foot - mesh_height(&terrain, post, size, 8)).abs() < 1e-3);
    }

    /// Cada vértice de la malla de superficie del chunk que contiene
    /// `crossing`, con su alfa.
    fn crossing_surface(crossing: &LevelCrossing, terrain: &Terrain) -> Vec<(Vec3, f32)> {
        let size = WorldConfig::default().chunk_size;
        let roads = terrain.roads.as_deref().unwrap();
        let area = ChunkCoord::containing(crossing.at, size).bounds(size);
        let ground = |p: GroundVec2| mesh_height(terrain, p, size, 8);
        let mesh = surface_mesh(&roads.parts_in(area), &ground, roads).unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours")
        };
        points
            .iter()
            .zip(colors)
            .map(|(p, c)| (Vec3::from_array(*p), c[3]))
            .collect()
    }

    #[test]
    fn the_road_rises_gently_on_its_own_axis_to_the_crossing_deck() {
        let (config, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let ground = |p: GroundVec2| mesh_height(&terrain, p, config.chunk_size, 8);
        for crossing in roads.crossings() {
            let road = crossing.road;
            let side = GroundVec2::new(-road.y, road.x);
            let surface = crossing_surface(crossing, &terrain);
            let mut raised = 0;
            for &(v, alpha) in &surface {
                let g = GroundVec2::new(v.x, -v.z);
                let rel = g - crossing.at;
                let lift = v.y - ground(g);
                // Nunca por encima de los tablones; elevada solo sobre el
                // camino mismo, centrada en su eje.
                assert!(lift <= RAIL_TOP - 0.02 + 1e-3, "{lift}");
                if lift > LIFT + 0.01 {
                    raised += 1;
                    assert!(
                        rel.dot(side).abs() <= RoadNetwork::HALF_WIDTH + FEATHER / 2.0 + 1e-3,
                        "{rel:?}"
                    );
                    assert!(alpha > 0.0 || rel.dot(side).abs() < RoadNetwork::HALF_WIDTH);
                }
            }
            assert!(raised > 20, "{raised}");
            // A lo largo de cada borde del camino opaco: desde el suelo hasta
            // la plataforma, sin superar una pendiente de 1:8, sin huecos,
            // hasta el paso a nivel de ambos lados.
            for edge in [-1.0, 1.0] {
                let mut axis: Vec<(f32, f32)> = surface
                    .iter()
                    .filter(|(_, alpha)| *alpha == 1.0)
                    .map(|&(v, _)| (GroundVec2::new(v.x, -v.z), v.y))
                    .filter(|&(g, _)| (g - crossing.at).dot(side) * edge > 0.0)
                    .map(|(g, y)| ((g - crossing.at).dot(road), y - ground(g)))
                    .filter(|(s, _)| s.abs() < 20.0)
                    .collect();
                axis.sort_by(|a, b| a.0.total_cmp(&b.0));
                assert!(axis.first().unwrap().0 < -15.0 && axis.last().unwrap().0 > 15.0);
                for w in axis.windows(2) {
                    assert!(w[1].0 - w[0].0 < 1.5, "a gap in the strip at {w:?}");
                    if w[1].0 - w[0].0 > 0.05 {
                        let slope = (w[1].1 - w[0].1).abs() / (w[1].0 - w[0].0);
                        assert!(slope <= 1.0 / 8.0, "{slope} at {w:?}");
                    }
                }
                let top = axis
                    .iter()
                    .filter(|(s, _)| s.abs() < 1.0)
                    .map(|(_, lift)| *lift)
                    .fold(f32::MAX, f32::min);
                assert!(top > RAIL_TOP - 0.06, "{top}");
                let foot = axis.iter().filter(|(s, _)| s.abs() > 12.0);
                assert!(
                    foot.into_iter()
                        .all(|(_, lift)| (*lift - LIFT).abs() < 1e-3)
                );
            }
        }
    }

    #[test]
    fn a_crossing_has_planks_the_road_wide_and_nothing_above_the_rails() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let crossing = roads.crossings().next().unwrap();
        let height = |p: GroundVec2| terrain.height(p.x, p.y);
        let pieces = crossing_pieces(crossing, &height, [true; 2]);
        let across = GroundVec2::new(-crossing.rail.y, crossing.rail.x);
        let span = RoadNetwork::WIDTH / crossing.angle().sin();
        for (_, t) in &pieces {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            // Las cruces sobre sus postes son las únicas partes altas.
            if t.scale.y >= 0.17 || t.scale.x >= 1.2 {
                continue;
            }
            let top = t.translation.y + t.scale.y / 2.0 - height(g);
            assert!(top <= RAIL_TOP + 0.1, "{top}");
        }
        let planks: Vec<&Transform> = pieces
            .iter()
            .filter(|(p, t)| *p == Paint::Timber && t.scale.z == PLANK.x)
            .map(|(_, t)| t)
            .collect();
        assert_eq!(planks.len(), PLANKS.len());
        for t in planks {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            // Tan largo como ancho es el camino, medido a lo largo de la vía
            // (el tablón sigue la suave pendiente del suelo).
            assert!((t.scale.x - span).abs() < 0.05, "{}", t.scale.x);
            let off = (g - crossing.at).dot(across).abs() + PLANK.x / 2.0;
            assert!(off <= 1.4, "{off}");
        }
        // Tablones entre los rieles y justo por fuera de ellos (separados
        // 1.676 m): una franja de cerca de medio metro por fuera de cada uno.
        let outer = PLANKS.iter().fold(0.0_f32, |m, p| m.max(p.abs())) + PLANK.x / 2.0;
        assert!((0.4..=0.6).contains(&(outer - 1.676 / 2.0)), "{outer}");
        // Solo los tablones y las dos cruces sobre postes: sin rampas de
        // madera.
        assert_eq!(pieces.len(), PLANKS.len() + 2 * 5);
    }

    #[test]
    fn crossing_signs_fade_out_of_the_cameras_way_and_planks_do_not() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let crossing = *roads.crossings().next().unwrap();
        let (mut app, _, _) = laid_at(crossing.at);
        let mut q = app
            .world_mut()
            .query_filtered::<(&Transform, Option<&ThinOccluder>), With<CrossingPart>>();
        let parts: Vec<(Transform, bool)> = q
            .iter(app.world())
            .map(|(t, fade)| (*t, fade.is_some()))
            .collect();
        let near = |t: &Transform| {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            roads
                .crossings()
                .any(|c| c.sign_spots().iter().any(|s| s.distance(g) < 1.0))
        };
        let (signs, planks): (Vec<_>, Vec<_>) = parts.iter().partition(|(t, _)| near(t));
        // Cada cruz: su poste y dos brazos negros y dos blancos.
        assert_eq!(signs.len() % 10, 0);
        assert!(!signs.is_empty());
        assert!(
            signs.iter().all(|(_, fades)| *fades),
            "every post and arm fades"
        );
        assert!(!planks.is_empty());
        assert!(
            planks.iter().all(|(_, fades)| !*fades),
            "planks lie flat, never fade"
        );
    }

    #[test]
    fn signpost_boards_read_in_the_display_face() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        assert!(!roads.signposts.is_empty());
        for post in &roads.signposts {
            for board in &post.boards {
                assert!(
                    board
                        .text
                        .chars()
                        .all(|c| lettering::supports(Face::Display, c)),
                    "{}",
                    board.text
                );
                assert!(board.text.ends_with(" KM"));
                let layout = layout_board(&board.text, BOARD_WIDTH, BOARD_HEIGHT);
                assert!(layout.height_m <= BOARD_HEIGHT + 1e-4);
            }
        }
        // Los nombres que trae el juego, como los imprimirían los postes
        // indicadores.
        for name in ["Eduardo Castex", "Monte Nievas", "General Pico"] {
            let text = pampa_core::road::board_text(name, 2349.0);
            assert!(
                text.chars().all(|c| lettering::supports(Face::Display, c)),
                "{text}"
            );
        }
    }

    #[test]
    fn a_crossing_lays_planks_level_with_the_rails_and_signs_on_posts() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let crossing = roads.crossings().next().unwrap();
        let pieces = crossing_pieces(
            crossing,
            &|p: GroundVec2| terrain.height(p.x, p.y),
            [true; 2],
        );
        let planks: Vec<&Transform> = pieces
            .iter()
            .filter(|(p, t)| *p == Paint::Timber && t.scale.z == PLANK.x)
            .map(|(_, t)| t)
            .collect();
        assert_eq!(planks.len(), PLANKS.len());
        for t in planks {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            let top = t.translation.y + PLANK.y / 2.0 - terrain.height(g.x, g.y);
            assert!((top - (RAIL_TOP - 0.02)).abs() < 0.05, "{top}");
            // Lo bastante largo para llevar el camino entero de un lado a otro.
            assert!(t.scale.x >= RoadNetwork::WIDTH);
        }
        let whites = pieces.iter().filter(|(p, _)| *p == Paint::White).count();
        assert_eq!(whites, 4);
    }

    /// Un paso a nivel de un camino que va hacia el este sobre una vía que va
    /// hacia el norte, a `x` metros a lo largo del camino.
    fn crossing_at(x: f32) -> LevelCrossing {
        LevelCrossing {
            at: GroundVec2::new(x, 0.0),
            road: GroundVec2::new(1.0, 0.0),
            rail: GroundVec2::new(0.0, 1.0),
            along: x,
        }
    }

    #[test]
    fn a_road_over_two_tracks_side_by_side_is_signed_once_on_each_side() {
        // La vía de una estación y su vía muerta, a 4.5 m: cuatro carteles
        // pasaron a ser dos.
        let (a, b) = (crossing_at(100.0), crossing_at(104.5));
        let both = [a, b];
        let posted: Vec<[bool; 2]> = both.iter().map(|c| signs_posted(c, both.iter())).collect();
        // El primero conserva el cartel anterior a él, el segundo el posterior.
        assert_eq!(posted, [[true, false], [false, true]]);
        // Un paso a nivel solitario conserva ambos; un paso a nivel de otro
        // camino, lejano o perpendicular a este, no se queda con sus carteles.
        assert_eq!(signs_posted(&a, [a].iter()), [true, true]);
        let far = crossing_at(160.0);
        assert_eq!(signs_posted(&a, [a, far].iter()), [true, true]);
        let other = LevelCrossing {
            road: GroundVec2::new(0.0, 1.0),
            rail: GroundVec2::new(1.0, 0.0),
            ..crossing_at(104.5)
        };
        assert_eq!(signs_posted(&a, [a, other].iter()), [true, true]);
    }

    #[test]
    fn every_crossing_of_the_shipped_kind_of_world_has_exactly_two_signs() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let signs = |c: &LevelCrossing| {
            let height = |p: GroundVec2| terrain.height(p.x, p.y);
            crossing_pieces(c, &height, signs_posted(c, roads.crossings()))
                .into_iter()
                .filter(|(p, t)| *p == Paint::Timber && t.scale.y > 2.0)
                .count()
        };
        for c in roads.crossings() {
            // Los pasos a nivel de este tramo de camino, lado a lado.
            let group = roads
                .crossings()
                .filter(|o| o.road.dot(c.road).abs() > 0.9 && o.at.distance(c.at) <= SHARED_SIGNS);
            assert_eq!(group.map(signs).sum::<usize>(), 2, "{:?}", c.at);
        }
    }

    #[test]
    fn the_planks_lie_flush_with_the_new_rail_tops() {
        let (_, _, terrain) = world();
        let roads = terrain.roads.clone().unwrap();
        let crossing = roads.crossings().next().unwrap();
        let height = |p: GroundVec2| terrain.height(p.x, p.y);
        for (paint, t) in crossing_pieces(crossing, &height, [false; 2]) {
            assert_eq!(paint, Paint::Timber);
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            // Las cabezas de los rieles están a 0.50 sobre el suelo; los
            // tablones, 2 cm por debajo, entre los rieles y junto a ellos.
            let top = t.translation.y + t.scale.y / 2.0 - height(g);
            assert!((top - 0.50).abs() < 0.05, "{top}");
        }
    }
}
