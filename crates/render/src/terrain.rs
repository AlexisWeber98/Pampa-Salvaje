//! Smooth-shaded ground streamed in square chunks around the rider, each with
//! scattered props (tosca stones and small jarilla shrubs, drawn with the
//! CC0 vegetation models when they are loaded) that give the eye something
//! to measure speed by. The ground is painted per vertex, from grey-green to
//! dry yellow grass, with plain earth on towns' leveled ground, and wears
//! the CC0 ambientCG soil-and-grass detail textures of [`crate::ground`]
//! over that paint. Under the monte patches the colour's alpha carries the
//! leaf litter weight ([`crate::ground::litter_weight`], from the flora's
//! `monte`), which the ground shader blends in. The native flora on it is
//! grown by [`crate::flora`].
//!
//! Every chunk is sampled from the logic [`Terrain`], the single source of
//! truth for height, so neighboring chunks meet without seams, in height and
//! in shading: each corner's normal comes from the height field's slope (see
//! [`build_chunk_mesh`]), so the ground reads as rolling land, not facets.
//! Which chunks
//! exist is decided by a [`StreamingSet`] around [`StreamFocus`]: chunks
//! within the load radius are built, a bounded number per frame, and chunks
//! past the radius plus one are despawned with their props. The chunks
//! around the start are built at once; a chunk streamed in later spawns at
//! once with its props, but its ground mesh, the costly part, is built on
//! the async compute pool and lands a few frames later, out in the fog, so
//! riding never stalls a frame on it.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use pampa_core::flora::grass_dryness;
use pampa_core::{ChunkCoord, FloraSite, RoadNetwork, Terrain, Vec2 as GroundVec2, WorldConfig};

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::flora::{
    Model, VegetationLoad, VegetationModels, WorldFlora, spot_unit, visible_within,
};
use crate::ground::{GroundMaterial, ground_material, ground_uv, litter_weight};
use crate::streaming::StreamingSet;

/// The logic terrain this world is drawn from. Insert before the plugin runs,
/// or the default terrain is used.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTerrain(pub Terrain);

/// The world's size and chunk size. Insert before the plugin runs, or the
/// default world is used.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct WorldChunks(pub WorldConfig);

/// Ground point the world streams around (the rider). The game keeps it up
/// to date; it starts at the origin.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct StreamFocus(pub GroundVec2);

/// One streamed chunk of ground; its props are its children.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainChunk(pub ChunkCoord);

/// Chunk streaming, in `PostUpdate`: builds the chunks near the
/// [`StreamFocus`] and despawns the far ones. Systems that decorate new
/// chunks (see `crate::rail`) run after it, when its commands are applied.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkStreaming;

/// The loaded chunks and their entities.
#[derive(Resource, Debug)]
pub struct ChunkStreamer {
    pub set: StreamingSet,
    entities: HashMap<ChunkCoord, Entity>,
}

/// Streams terrain chunks and their props around [`StreamFocus`].
pub struct TerrainPlugin {
    /// Load radius, in chunks. Chunks past `radius + 1` are dropped.
    pub radius: i32,
    /// Most chunks built per frame (the first frame builds them all).
    pub budget: usize,
    /// Quads along each side of a chunk.
    pub resolution: u32,
}

impl Default for TerrainPlugin {
    fn default() -> Self {
        Self {
            radius: 3,
            budget: 1,
            resolution: 40,
        }
    }
}

impl TerrainPlugin {
    /// Distance from the focus within which the ground is always loaded,
    /// wherever the focus stands in its chunk: `(radius - 1)` chunks. The
    /// dust fog must swallow everything before this distance (see
    /// [`crate::atmosphere::FOG_VISIBILITY`]).
    pub fn guaranteed_reach(&self, chunk_size: f32) -> f32 {
        (self.radius - 1).max(0) as f32 * chunk_size
    }
}

/// Quads along each side of a chunk, as the [`TerrainPlugin`] builds them.
#[derive(Resource, Clone, Copy)]
pub(crate) struct ChunkDetail {
    pub(crate) resolution: u32,
}

/// Meshes and materials shared by every chunk.
#[derive(Resource)]
struct TerrainKit {
    ground: Handle<GroundMaterial>,
    rock_mesh: Handle<Mesh>,
    shrub_mesh: Handle<Mesh>,
    rock_material: Handle<StandardMaterial>,
    shrub_material: Handle<StandardMaterial>,
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<GroundMaterial>>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            .init_resource::<StreamFocus>()
            .insert_resource(ChunkDetail {
                resolution: self.resolution.max(1),
            })
            .insert_resource(ChunkStreamer {
                set: StreamingSet::new(self.radius, self.budget),
                entities: HashMap::new(),
            })
            .add_systems(
                Startup,
                (setup_kit, fill_chunks).chain().after(VegetationLoad),
            )
            .add_systems(
                PostUpdate,
                (land_ground, stream_chunks).chain().in_set(ChunkStreaming),
            );
    }
}

fn setup_kit(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut grounds: ResMut<Assets<GroundMaterial>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    commands.insert_resource(TerrainKit {
        // Vertex colors carry the palette; the material stays white and
        // matte, with the tiled ground detail on top when it is found.
        ground: grounds.add(ground_material(
            root.as_deref().map(|r| r.0.as_path()),
            images.as_deref_mut(),
        )),
        rock_mesh: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        shrub_mesh: meshes.add(
            Sphere::new(0.5)
                .mesh()
                .ico(1)
                .expect("valid ico subdivision"),
        ),
        rock_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.50, 0.44),
            perceptual_roughness: 0.95,
            ..default()
        }),
        shrub_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.40, 0.20),
            perceptual_roughness: 1.0,
            ..default()
        }),
    });
}

/// Everything a chunk spawn needs, bundled for the systems below.
type ChunkInputs<'a> = (
    Res<'a, WorldTerrain>,
    Res<'a, WorldChunks>,
    Res<'a, ChunkDetail>,
    Res<'a, StreamFocus>,
);

/// Builds every chunk around the start at once, so the first frame shows
/// the whole view.
fn fill_chunks(
    mut commands: Commands,
    (terrain, world, detail, focus): ChunkInputs,
    kit: Res<TerrainKit>,
    (models, flora): (Option<Res<VegetationModels>>, Option<Res<WorldFlora>>),
    mut streamer: ResMut<ChunkStreamer>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let budget = streamer.set.budget;
    streamer.set.budget = usize::MAX;
    let plan = streamer.set.update(&world.0, focus.0);
    streamer.set.budget = budget;
    for coord in plan.spawn {
        let entity = spawn_chunk(
            &mut commands,
            (&kit, models.as_deref()),
            &mut meshes,
            (&terrain.0, &world.0, *detail),
            (coord, flora.as_deref().map(|f| &f.0), GroundBuild::Inline),
        );
        streamer.entities.insert(coord, entity);
    }
}

/// Gives every chunk whose ground finished building its mesh.
fn land_ground(
    mut commands: Commands,
    mut pending: Query<(Entity, &mut PendingGround)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (chunk, mut build) in &mut pending {
        if let Some(mesh) = check_ready(&mut build.0) {
            commands
                .entity(chunk)
                .remove::<PendingGround>()
                .insert(Mesh3d(meshes.add(mesh)));
        }
    }
}

/// Builds the next few chunks near the focus and drops the far ones.
fn stream_chunks(
    mut commands: Commands,
    (terrain, world, detail, focus): ChunkInputs,
    kit: Option<Res<TerrainKit>>,
    (models, flora): (Option<Res<VegetationModels>>, Option<Res<WorldFlora>>),
    mut streamer: ResMut<ChunkStreamer>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(kit) = kit else {
        return;
    };
    let plan = streamer.set.update(&world.0, focus.0);
    for coord in plan.despawn {
        if let Some(entity) = streamer.entities.remove(&coord) {
            // Props are children: they go with the chunk.
            commands.entity(entity).despawn();
        }
    }
    for coord in plan.spawn {
        let entity = spawn_chunk(
            &mut commands,
            (&kit, models.as_deref()),
            &mut meshes,
            (&terrain.0, &world.0, *detail),
            (
                coord,
                flora.as_deref().map(|f| &f.0),
                GroundBuild::Background,
            ),
        );
        streamer.entities.insert(coord, entity);
    }
}

/// A streamed chunk's ground mesh, being built on the async compute pool.
/// [`land_ground`] swaps it for the chunk's `Mesh3d` when it is ready; a
/// chunk despawned first drops it, which cancels the build.
#[derive(Component)]
pub(crate) struct PendingGround(Task<Mesh>);

#[cfg(test)]
impl PendingGround {
    /// Whether the build is done (it lands on the next update).
    pub(crate) fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

/// Blocks until every background build `done` reports on has finished, so
/// the next update lands them all: tests step the async pool without
/// depending on how many frames it takes. Fails after 30 s.
#[cfg(test)]
pub(crate) fn finish_builds<T: Component>(app: &mut App, done: fn(&T) -> bool) {
    let start = std::time::Instant::now();
    loop {
        let mut q = app.world_mut().query::<&T>();
        if q.iter(app.world()).all(done) {
            return;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(30),
            "the background builds never finished"
        );
        std::thread::yield_now();
    }
}

/// How a chunk gets its ground mesh.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GroundBuild {
    /// Built now, on the main thread: the first frame shows the whole view.
    Inline,
    /// Built on the async compute pool, so a chunk streamed in while riding
    /// never stalls a frame. The chunk is at the edge of the load radius,
    /// inside the dust fog, so its ground landing a few frames later is
    /// never seen.
    Background,
}

/// Spawns chunk `coord`: its ground mesh (or the task building it), and its
/// props as children.
fn spawn_chunk(
    commands: &mut Commands,
    (kit, models): (&TerrainKit, Option<&VegetationModels>),
    meshes: &mut Assets<Mesh>,
    (terrain, world, detail): (&Terrain, &WorldConfig, ChunkDetail),
    (coord, track, build): (ChunkCoord, Option<&FloraSite>, GroundBuild),
) -> Entity {
    let (size, resolution) = (world.chunk_size, detail.resolution);
    let mut chunk = commands.spawn((
        Name::new(format!("Terrain chunk {} {}", coord.x, coord.y)),
        TerrainChunk(coord),
        MeshMaterial3d(kit.ground.clone()),
        Transform::default(),
        Visibility::default(),
    ));
    match build {
        GroundBuild::Inline => {
            let mesh = build_chunk_mesh(terrain, coord, size, resolution, track);
            chunk.insert(Mesh3d(meshes.add(mesh)));
        }
        GroundBuild::Background => {
            // Cheap: the terrain shares its roads.
            let (terrain, flora) = (terrain.clone(), track.cloned());
            let task = AsyncComputeTaskPool::get().spawn(async move {
                build_chunk_mesh(&terrain, coord, size, resolution, flora.as_ref())
            });
            chunk.insert(PendingGround(task));
        }
    }
    let chunk = chunk.id();
    for prop in chunk_props(terrain, coord, world.chunk_size, track) {
        if let Some(models) = models
            && spawn_model_prop(commands, models, terrain, &prop, chunk)
        {
            continue;
        }
        let (mesh, material, scale) = if prop.rock {
            (
                &kit.rock_mesh,
                &kit.rock_material,
                Vec3::new(prop.size * 1.3, prop.size * 0.6, prop.size),
            )
        } else {
            (
                &kit.shrub_mesh,
                &kit.shrub_material,
                Vec3::new(prop.size, prop.size * 0.7, prop.size),
            )
        };
        let base = world_from_ground(prop.at, terrain.height(prop.at.x, prop.at.y));
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(base + Vec3::Y * scale.y * 0.35)
                .with_rotation(Quat::from_rotation_y(prop.yaw))
                .with_scale(scale),
            ChildOf(chunk),
        ));
    }
    chunk
}

/// Draws `prop` with a vegetation model: a tosca stone or a small jarilla
/// shrub. Returns `false` when the model was not loaded, so the stand-in is
/// drawn instead.
fn spawn_model_prop(
    commands: &mut Commands,
    models: &VegetationModels,
    terrain: &Terrain,
    prop: &Prop,
    chunk: Entity,
) -> bool {
    let pick = |n: u8| (spot_unit(prop.at, 21) * n as f32) as u8 % n + 1;
    let (model, height) = if prop.rock {
        (Model::Rock(pick(4)), 0.5 * prop.size)
    } else {
        (Model::Bush(pick(2)), 0.7 * prop.size)
    };
    let Some(parts) = models.parts(model, 0) else {
        return false;
    };
    let base = world_from_ground(
        prop.at,
        terrain.height(prop.at.x, prop.at.y) - 0.05 * height,
    );
    let transform = Transform::from_translation(base)
        .with_rotation(Quat::from_rotation_y(prop.yaw))
        .with_scale(Vec3::splat(height));
    for (mesh, material) in parts {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            transform,
            visible_within(PROP_FAR, PROP_FADE),
            ChildOf(chunk),
        ));
    }
    true
}

/// Where model stones and shrubs stop being drawn, in meters, and the
/// width of their dithered fade-out.
const PROP_FAR: f32 = 160.0;
const PROP_FADE: f32 = 15.0;

/// Ground corner `(i, j)` of a chunk split into `resolution` quads per side.
/// The last corner is the next chunk's first, computed the same way, so
/// neighbors share their border vertices exactly.
fn chunk_corner(coord: ChunkCoord, size: f32, resolution: u32, i: u32, j: u32) -> GroundVec2 {
    let axis = |c: i32, k: u32| {
        let (lo, hi) = (c as f32 * size, (c + 1) as f32 * size);
        if k == 0 {
            lo
        } else if k == resolution {
            hi
        } else {
            lo + (hi - lo) * (k as f32 / resolution as f32)
        }
    };
    GroundVec2::new(axis(coord.x, i), axis(coord.y, j))
}

/// Height of the drawn ground at ground point `p`: the terrain sampled at
/// the chunk grid's corners, split into the same two triangles per quad as
/// [`build_chunk_mesh`]. Things laid flat on the ground (the road surface)
/// follow this, not the finer logic terrain, so the mesh never pokes
/// through them.
pub(crate) fn mesh_height(terrain: &Terrain, p: GroundVec2, size: f32, resolution: u32) -> f32 {
    let coord = ChunkCoord::containing(p, size);
    let min = coord.min(size);
    let quad = size / resolution as f32;
    let fx = ((p.x - min.x) / quad).clamp(0.0, resolution as f32);
    let fy = ((p.y - min.y) / quad).clamp(0.0, resolution as f32);
    let (i, j) = (
        (fx.floor() as u32).min(resolution - 1),
        (fy.floor() as u32).min(resolution - 1),
    );
    let (u, v) = (fx - i as f32, fy - j as f32);
    let h = |i: u32, j: u32| {
        let c = chunk_corner(coord, size, resolution, i, j);
        terrain.height(c.x, c.y)
    };
    let (a, c) = (h(i, j), h(i + 1, j + 1));
    // Triangles (a, b, c) below the diagonal a–c and (a, c, d) above it.
    if u >= v {
        a + (h(i + 1, j) - a) * (u - v) + (c - a) * v
    } else {
        a + (h(i, j + 1) - a) * (v - u) + (c - a) * u
    }
}

/// The smooth normal and the normal-map tangent (bitangent sign in `w`) of
/// the ground where it rises `dx` meters per meter east and `dy` per meter
/// north. The tangent follows the world-space U of [`ground_uv`], as
/// [`crate::ground::face_tangent`] does for a flat face on the same slope.
pub(crate) fn slope_frame(dx: f32, dy: f32) -> ([f32; 3], [f32; 4]) {
    // World X is east and world Z is south: along them the ground rises by
    // `dx` and by `-dy`.
    let (along_u, along_v) = (Vec3::new(1.0, dx, 0.0), Vec3::new(0.0, -dy, 1.0));
    let normal = along_v.cross(along_u).normalize_or(Vec3::Y);
    let tangent = along_u.normalize();
    let w = if normal.cross(tangent).dot(along_v) < 0.0 {
        -1.0
    } else {
        1.0
    };
    (normal.to_array(), [tangent.x, tangent.y, tangent.z, w])
}

/// An indexed grid of `(resolution + 1)²` corners, two triangles per quad
/// (the split [`mesh_height`] follows). Vertices are in world space; the
/// chunk entity stays at the origin. UVs are world space too
/// ([`ground_uv`]), and the paint is sampled per corner.
///
/// Normals are smooth: each corner's comes from the height field's slope
/// over one quad either way (central differences of [`Terrain::height`]),
/// with the matching tangent for the ground's normal map. The slope is
/// sampled on a lattice numbered across the whole world, not per chunk, so
/// the corners two chunks share get exactly the same normal and the ground
/// shades without seams.
fn build_chunk_mesh(
    terrain: &Terrain,
    coord: ChunkCoord,
    size: f32,
    resolution: u32,
    flora: Option<&FloraSite>,
) -> Mesh {
    let side = resolution + 1;
    let quad = size / resolution as f32;
    // Heights on the world lattice from one quad before this chunk to one
    // past it: lattice point `n` along an axis is at `n * quad` meters.
    let (lattice, origin) = (side + 2, |c: i32| c * resolution as i32 - 1);
    let (ox, oy) = (origin(coord.x), origin(coord.y));
    let heights: Vec<f32> = (0..lattice)
        .flat_map(|j| (0..lattice).map(move |i| (i, j)))
        .map(|(i, j)| {
            let at = |o: i32, k: u32| (o + k as i32) as f32 * quad;
            terrain.height(at(ox, i), at(oy, j))
        })
        .collect();
    // Corner `(i, j)` is lattice point `(i + 1, j + 1)`.
    let lat = |i: u32, j: u32| heights[(j * lattice + i) as usize];

    let corners = (side * side) as usize;
    let mut positions = Vec::with_capacity(corners);
    let mut colors = Vec::with_capacity(corners);
    let mut uvs = Vec::with_capacity(corners);
    let mut normals = Vec::with_capacity(corners);
    let mut tangents = Vec::with_capacity(corners);
    for j in 0..side {
        for i in 0..side {
            let ground = chunk_corner(coord, size, resolution, i, j);
            let p = world_from_ground(ground, terrain.height(ground.x, ground.y));
            let dx = (lat(i + 2, j + 1) - lat(i, j + 1)) / (2.0 * quad);
            let dy = (lat(i + 1, j + 2) - lat(i + 1, j)) / (2.0 * quad);
            let (normal, tangent) = slope_frame(dx, dy);
            positions.push(p.to_array());
            colors.push(ground_color(terrain, flora, ground));
            uvs.push(ground_uv(p));
            normals.push(normal);
            tangents.push(tangent);
        }
    }
    let at = |i: u32, j: u32| j * side + i;
    let mut indices = Vec::with_capacity((resolution * resolution * 6) as usize);
    for j in 0..resolution {
        for i in 0..resolution {
            let (a, b) = (at(i, j), at(i + 1, j));
            let (c, d) = (at(i + 1, j + 1), at(i, j + 1));
            // World Z is -north, so (a, b, c) is counter-clockwise seen from above.
            indices.extend([a, b, c, a, c, d]);
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
    .with_inserted_indices(Indices::U32(indices))
}

/// Grey-green grass where the pampa is greener (sRGB).
const GREEN_GRASS: Vec3 = Vec3::new(0.49, 0.54, 0.38);
/// Dry yellow grass (sRGB).
const DRY_GRASS: Vec3 = Vec3::new(0.71, 0.63, 0.40);
/// Trodden earth of a town's leveled ground (sRGB): plain, so streets and
/// buildings read on it.
const TOWN_GROUND: Vec3 = Vec3::new(0.60, 0.53, 0.40);
/// Worn earth beside a road (sRGB), and how far past the road's edge the
/// wear reaches, in meters.
const ROAD_EARTH: Vec3 = Vec3::new(0.60, 0.52, 0.39);
const ROAD_VERGE: f32 = 4.0;
/// Wet earth of a lagoon's bottom and shore (sRGB): it reaches only
/// [`pampa_core::Lagoon::MUD_BAND`] up the bank, then the grass resumes.
const LAGOON_MUD: Vec3 = Vec3::new(0.36, 0.33, 0.25);
/// Darkest shade slopes and dips may give the ground.
const SHADE_MIN: f32 = 0.82;

/// The darkest and brightest linear colour the ground may take.
#[cfg(test)]
fn palette_range() -> ([f32; 3], [f32; 3]) {
    let colors = [GREEN_GRASS, DRY_GRASS, TOWN_GROUND, ROAD_EARTH, LAGOON_MUD];
    let lo = colors.iter().fold(Vec3::ONE, |a, c| a.min(*c)) * SHADE_MIN;
    let hi = colors.iter().fold(Vec3::ZERO, |a, c| a.max(*c));
    let linear = |v: Vec3| {
        let c = LinearRgba::from(Color::srgb(v.x, v.y, v.z));
        [c.red, c.green, c.blue]
    };
    (linear(lo), linear(hi))
}

/// Ground colour at ground point `g`: grey-green to dry yellow by
/// low-frequency noise ([`grass_dryness`]), plain earth on town ground,
/// darker on slopes and in dips. A function of the position alone, so
/// neighbouring chunks agree on their shared vertices.
fn ground_color(terrain: &Terrain, flora: Option<&FloraSite>, g: GroundVec2) -> [f32; 4] {
    let grass = GREEN_GRASS.lerp(DRY_GRASS, grass_dryness(terrain, g));
    let town = terrain
        .flat_areas
        .iter()
        .map(|a| a.weight(g))
        .fold(0.0, f32::max);
    let rgb = grass.lerp(TOWN_GROUND, town);
    // Worn earth along the roads, under their feathered edges.
    let worn = terrain.roads.as_ref().map_or(0.0, |r| {
        let reach = RoadNetwork::HALF_WIDTH + ROAD_VERGE;
        r.near(g, reach)
            .map_or(0.0, |d| 1.0 - smoothstep(RoadNetwork::HALF_WIDTH, reach, d))
    });
    let rgb = rgb.lerp(ROAD_EARTH, 0.6 * worn);
    let wet = lagoon_wetness(terrain, g);
    let rgb = rgb.lerp(LAGOON_MUD, wet);
    // Leaf litter under the monte (the alpha): none without the flora,
    // and none on cleared ground, the road's worn verge or a lagoon's mud.
    let litter = flora.map_or(0.0, |f| litter_weight(f.monte(g), town.max(worn).max(wet)));
    let e = 0.75;
    let dx = terrain.height(g.x + e, g.y) - terrain.height(g.x - e, g.y);
    let dy = terrain.height(g.x, g.y + e) - terrain.height(g.x, g.y - e);
    let slope = dx.hypot(dy) / (2.0 * e);
    let dip = 0.5 - 0.5 * (terrain.height(g.x, g.y) / terrain.amplitude).clamp(-1.0, 1.0);
    let shade = (1.0 - 0.6 * slope - 0.05 * dip).clamp(SHADE_MIN, 1.0);
    let rgb = rgb * shade;
    let mut color = LinearRgba::from(Color::srgb(rgb.x, rgb.y, rgb.z)).to_f32_array();
    color[3] = litter;
    color
}

/// How wet the ground at `g` is, from 0 (dry grass) to 1 (mud): wet under
/// a lagoon's water, drying over its narrow mud band up the bank.
pub(crate) fn lagoon_wetness(terrain: &Terrain, g: GroundVec2) -> f32 {
    terrain
        .lagoons
        .iter()
        .filter(|l| l.touches(g))
        .map(|l| 1.0 - smoothstep(0.0, pampa_core::Lagoon::MUD_BAND, l.offshore(g)))
        .fold(0.0, f32::max)
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cheap deterministic hash of a 2D point into `[0, 1)`.
pub(crate) fn hash2(x: f32, y: f32) -> f32 {
    let v = (x * 12.9898 + y * 78.233).sin() * 43_758.547;
    v - v.floor()
}

/// Ground area per prop, in square meters (about 44 props per 200 m chunk).
const AREA_PER_PROP: f32 = 900.0;
/// Mixed into every chunk's prop seed.
const PROP_SEED: u32 = 0x5eed_0f1a;

/// One rock or shrub on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Prop {
    at: GroundVec2,
    rock: bool,
    size: f32,
    yaw: f32,
}

/// Least distance from a stone or shrub to the track's centerline, in
/// meters: off the ballast, like a grass tuft.
const PROP_RAIL_MARGIN: f32 = FloraSite::TUFT_RAIL_MARGIN;

/// Least distance from a stone or shrub to a road's edge, in meters.
const PROP_ROAD_MARGIN: f32 = 2.0;

/// Least distance from a stone or shrub to a settlement's footprints,
/// patios and tracks, in meters.
const PROP_SETTLEMENT_MARGIN: f32 = 2.0;

/// The props of chunk `coord`, seeded by its coordinate, skipping leveled
/// ground (towns are cleared land), the lagoons and their wet mud, the roads and, given the flora's
/// keep-out (`track`), the whole train path with [`PROP_RAIL_MARGIN`] and
/// the rural settlements with [`PROP_SETTLEMENT_MARGIN`].
fn chunk_props(
    terrain: &Terrain,
    coord: ChunkCoord,
    size: f32,
    track: Option<&FloraSite>,
) -> Vec<Prop> {
    let mut state = coord.seed(PROP_SEED) | 1;
    let mut unit = move || {
        // Xorshift32.
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state >> 8) as f32 / (1u32 << 24) as f32
    };
    let min = coord.min(size);
    let count = (size * size / AREA_PER_PROP).round() as u32;
    (0..count)
        .filter_map(|n| {
            let at = min + GroundVec2::new(unit() * size, unit() * size);
            let prop = Prop {
                at,
                rock: n % 3 == 0,
                size: 0.4 + unit() * 1.2,
                yaw: unit() * std::f32::consts::TAU,
            };
            let leveled = terrain.flat_areas.iter().any(|a| a.weight(at) > 0.0)
                || terrain.lagoons.iter().any(|l| l.is_bare(at));
            let on_road = terrain.roads.as_ref().is_some_and(|r| {
                r.near(at, RoadNetwork::HALF_WIDTH + PROP_ROAD_MARGIN)
                    .is_some()
            });
            let on_track = track.is_some_and(|t| !t.off_track(at, PROP_RAIL_MARGIN));
            let settled = track.is_some_and(|t| !t.off_settlements(at, PROP_SETTLEMENT_MARGIN));
            (!leveled && !on_road && !on_track && !settled).then_some(prop)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::face_tangent;
    use pampa_core::{FlatArea, TownLayout};

    fn app(focus: GroundVec2) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(StreamFocus(focus))
            .add_plugins(TerrainPlugin {
                radius: 2,
                budget: 3,
                resolution: 4,
            });
        app
    }

    fn chunks(app: &mut App) -> Vec<ChunkCoord> {
        let mut q = app.world_mut().query::<&TerrainChunk>();
        let mut v: Vec<ChunkCoord> = q.iter(app.world()).map(|c| c.0).collect();
        v.sort();
        v
    }

    #[test]
    fn the_first_frame_builds_every_chunk_around_the_focus() {
        let mut app = app(GroundVec2::new(30.0, 30.0));
        app.update();
        let mut expected = WorldConfig::default().chunks_near(GroundVec2::new(30.0, 30.0), 2);
        expected.sort();
        assert_eq!(chunks(&mut app), expected);
    }

    #[test]
    fn riding_away_builds_ahead_within_budget_and_drops_far_chunks_with_props() {
        let mut app = app(GroundVec2::ZERO);
        app.update();
        let props = |app: &mut App| {
            let mut q = app.world_mut().query_filtered::<(), With<ChildOf>>();
            q.iter(app.world()).count()
        };
        let props_at_start = props(&mut app);
        assert!(props_at_start > 0);

        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(2000.0, 0.0);
        app.update();
        let after_one = chunks(&mut app);
        // Far chunks went at once; only `budget` new ones were built.
        assert_eq!(after_one.len(), 3, "{after_one:?}");
        for _ in 0..20 {
            app.update();
        }
        let mut expected = WorldConfig::default().chunks_near(GroundVec2::new(2000.0, 0.0), 2);
        expected.sort();
        assert_eq!(chunks(&mut app), expected);
        // Memory stays bounded: the old chunks' props went with them.
        assert!(props(&mut app) <= props_at_start * 3 / 2);
    }

    #[test]
    fn a_streamed_chunk_builds_its_ground_off_the_main_schedule() {
        let mut app = app(GroundVec2::ZERO);
        app.update();
        let ground = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<(&TerrainChunk, Option<&Mesh3d>), With<TerrainChunk>>();
            q.iter(app.world())
                .map(|(c, m)| (c.0, m.map(|m| m.id())))
                .collect::<Vec<_>>()
        };
        // The first frame shows the whole view: every ground is built.
        assert!(ground(&mut app).iter().all(|(_, m)| m.is_some()));
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(2000.0, 0.0);
        app.update();
        // The chunks streamed in exist at once (their props and decorations
        // land the same frame), but the update that spawned them did not
        // build their ground: it is built on the async compute pool.
        let streamed = ground(&mut app);
        assert_eq!(streamed.len(), 3);
        assert!(streamed.iter().all(|(_, m)| m.is_none()), "{streamed:?}");
        // It lands once built, the same mesh the main thread would have
        // built. (Each update may stream in more chunks: step until all
        // have landed.)
        for _ in 0..50 {
            if ground(&mut app).iter().all(|(_, m)| m.is_some()) {
                break;
            }
            finish_builds(&mut app, PendingGround::is_finished);
            app.update();
        }
        let terrain = Terrain::default();
        for (coord, mesh) in ground(&mut app) {
            let mesh = mesh.expect("the ground lands");
            let built = app.world().resource::<Assets<Mesh>>().get(mesh).cloned();
            let expected = build_chunk_mesh(&terrain, coord, 200.0, 4, None);
            assert_eq!(
                built.map(|m| m.attribute(Mesh::ATTRIBUTE_POSITION).cloned()),
                Some(expected.attribute(Mesh::ATTRIBUTE_POSITION).cloned())
            );
        }
    }

    #[test]
    fn a_chunk_gone_before_its_ground_lands_drops_the_build() {
        let mut app = app(GroundVec2::ZERO);
        app.update();
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(2000.0, 0.0);
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<Entity, With<PendingGround>>();
        let streamed: Vec<Entity> = q.iter(app.world()).collect();
        assert_eq!(streamed.len(), 3);
        // Back before any of it lands: the chunks go, and their builds
        // with them.
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::ZERO;
        for _ in 0..10 {
            app.update();
        }
        for chunk in streamed {
            assert!(app.world().get_entity(chunk).is_err(), "{chunk} stayed");
        }
        // The chunks back near the focus build on the pool too: wait for
        // those builds instead of counting frames, so a slow pool cannot
        // leave one pending. Nothing else may be.
        finish_builds(&mut app, PendingGround::is_finished);
        app.update();
        let mut q = app.world_mut().query::<&PendingGround>();
        assert_eq!(q.iter(app.world()).count(), 0, "a build stayed pending");
    }

    #[test]
    fn neighboring_chunks_share_their_border_heights() {
        let terrain = Terrain::default();
        let (size, res) = (200.0, 40);
        for (a, b) in [
            (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)),
            (ChunkCoord::new(-3, 5), ChunkCoord::new(-2, 5)),
        ] {
            for j in 0..=res {
                let left = chunk_corner(a, size, res, res, j);
                let right = chunk_corner(b, size, res, 0, j);
                assert_eq!(left, right);
                assert_eq!(
                    terrain.height(left.x, left.y),
                    terrain.height(right.x, right.y)
                );
            }
        }
        let mesh = build_chunk_mesh(&terrain, ChunkCoord::new(2, -1), size, 4, None);
        assert_eq!(mesh.count_vertices(), 5 * 5);
        assert_eq!(mesh.indices().map(|i| i.len()), Some(4 * 4 * 6));
    }

    #[test]
    fn props_are_seeded_per_chunk_and_skip_leveled_ground() {
        let terrain = Terrain::default();
        let c = ChunkCoord::new(4, -2);
        let props = chunk_props(&terrain, c, 200.0, None);
        assert_eq!(props, chunk_props(&terrain, c, 200.0, None));
        assert_eq!(props.len(), 44);
        assert!(props.iter().all(|p| c.bounds(200.0).contains_point(p.at)));
        assert_ne!(
            props,
            chunk_props(&terrain, ChunkCoord::new(5, -2), 200.0, None)
        );

        let town = TownLayout::castex_inspired();
        let leveled = terrain.clone().with_flat_area(town.flat_area(&terrain));
        let area: FlatArea = leveled.flat_areas[0];
        for c in [ChunkCoord::new(0, 0), ChunkCoord::new(-1, -1)] {
            let props = chunk_props(&leveled, c, 200.0, None);
            assert!(props.len() < 44);
            assert!(props.iter().all(|p| area.weight(p.at) == 0.0));
        }
    }

    #[test]
    fn no_stone_or_shrub_stands_on_the_trains_path() {
        let (towns, train) = crate::train::tests::line_train();
        let world = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut terrain = Terrain::default();
        for town in &towns {
            let area = town.flat_area(&terrain);
            terrain = terrain.with_flat_area(area);
        }
        let names: Vec<String> = ["Eduardo Castex", "Monte Nievas", "General Pico"]
            .map(String::from)
            .to_vec();
        let (line, _) = pampa_core::RailLine::build(Some(&names), &towns, world.bounds());
        let path = train.path();
        let site = FloraSite::new(world, &terrain, &line).with_track(path);
        let mut chunks: Vec<ChunkCoord> = (0..=(path.length() / 20.0) as usize)
            .map(|k| world.chunk_of(path.at(k as f32 * 20.0)))
            .collect();
        chunks.sort_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let mut props = 0;
        for c in chunks {
            for p in chunk_props(&terrain, c, world.chunk_size, Some(&site)) {
                let gap = p.at.distance(path.at(path.project(p.at)));
                assert!(
                    gap >= PROP_RAIL_MARGIN - 1e-3,
                    "{p:?}: {gap} m from the track"
                );
                props += 1;
            }
        }
        assert!(props > 500, "{props}");
    }

    #[test]
    fn no_stone_or_shrub_stands_on_a_settlement() {
        let (towns, _) = crate::train::tests::line_train();
        let world = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let names: Vec<String> = ["Eduardo Castex", "Monte Nievas", "General Pico"]
            .map(String::from)
            .to_vec();
        let (line, _) = pampa_core::RailLine::build(Some(&names), &towns, world.bounds());
        let (roads, _) = RoadNetwork::build(&towns, &line, world.bounds());
        let mut terrain = Terrain::default().with_roads(std::sync::Arc::new(roads));
        for town in &towns {
            let area = town.flat_area(&terrain);
            terrain = terrain.with_flat_area(area);
        }
        let fences = pampa_core::FenceLayout::build(world, &terrain, &towns, &line);
        let layout = pampa_core::SettlementLayout::build(&terrain, &towns, &line, &fences);
        assert!(!layout.settlements.is_empty());
        let site = FloraSite::new(world, &terrain, &line).with_settlements(&layout);
        let mut checked = 0;
        for s in &layout.settlements {
            let lo = ChunkCoord::containing(s.bounds.min, world.chunk_size);
            let hi = ChunkCoord::containing(s.bounds.max, world.chunk_size);
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    for p in chunk_props(
                        &terrain,
                        ChunkCoord::new(x, y),
                        world.chunk_size,
                        Some(&site),
                    ) {
                        assert!(
                            layout.is_clear(p.at, PROP_SETTLEMENT_MARGIN - 1e-3),
                            "{p:?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 100, "{checked}");
    }

    #[test]
    fn the_fog_hides_the_streaming_edge() {
        let reach = TerrainPlugin::default().guaranteed_reach(WorldConfig::DEFAULT_CHUNK_SIZE);
        assert_eq!(reach, 400.0);
        assert!(crate::atmosphere::FOG_VISIBILITY <= reach);
        // Wherever the focus stands in its chunk, every point within the
        // reach lies in a loaded chunk.
        let world = WorldConfig::default();
        let plugin = TerrainPlugin::default();
        for focus in [
            GroundVec2::new(1.0, 1.0),
            GroundVec2::new(199.0, 199.0),
            GroundVec2::new(100.0, 199.0),
        ] {
            let loaded = world.chunks_near(focus, plugin.radius);
            for k in 0..72 {
                let angle = k as f32 * std::f32::consts::TAU / 72.0;
                let p = focus + GroundVec2::new(angle.cos(), angle.sin()) * (reach - 0.5);
                assert!(loaded.contains(&world.chunk_of(p)), "{focus:?} -> {p:?}");
            }
        }
    }
    #[test]
    fn the_ground_material_wears_the_ambientcg_textures_or_falls_back() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let material = |root: std::path::PathBuf| {
            let mut app = app(GroundVec2::new(30.0, 30.0));
            app.init_resource::<Assets<Image>>()
                .insert_resource(crate::AssetRoot(root));
            app.update();
            let mut q = app
                .world_mut()
                .query::<(&MeshMaterial3d<GroundMaterial>, &TerrainChunk)>();
            let id = q.iter(app.world()).next().expect("a chunk").0.id();
            let materials = app.world().resource::<Assets<GroundMaterial>>();
            let ground = materials.get(id).unwrap().clone();
            let (m, litter) = (ground.base, ground.extension.litter);
            let images = app.world().resource::<Assets<Image>>();
            let sizes: Vec<u32> = [&m.base_color_texture, &m.normal_map_texture, &litter]
                .into_iter()
                .flatten()
                .map(|h| images.get(h).unwrap().texture_descriptor.mip_level_count)
                .collect();
            (m, sizes)
        };
        let (textured, mips) = material(root.clone());
        assert_eq!(mips.len(), 3, "colour, normal and leaf litter: {mips:?}");
        assert!(textured.base_color_texture.is_some());
        assert!(textured.normal_map_texture.is_some());
        assert!(textured.occlusion_texture.is_some());
        assert!(textured.metallic_roughness_texture.is_some());
        assert_eq!(textured.metallic, 0.0);
        assert!(mips.iter().all(|&m| m == 10), "512 px down to 1: {mips:?}");
        // Missing files: plain vertex-painted ground, no panic.
        let (plain, none) = material(root.join("nowhere"));
        assert!(none.is_empty(), "no textures, no litter: {none:?}");
        assert!(plain.base_color_texture.is_none() && plain.normal_map_texture.is_none());
    }

    /// A point deep in a monte patch of the default world's flora.
    fn monte_heart(flora: &FloraSite) -> GroundVec2 {
        (-60..60)
            .flat_map(|i| (-60..60).map(move |j| GroundVec2::new(i as f32 * 50.0, j as f32 * 50.0)))
            .find(|&p| flora.monte(p) > 0.97)
            .expect("a monte patch within 3 km")
    }

    fn default_flora() -> FloraSite {
        FloraSite::new(
            WorldConfig::default(),
            &Terrain::default(),
            &pampa_core::RailLine::default(),
        )
    }

    #[test]
    fn the_ground_alpha_carries_the_litter_under_the_monte_only() {
        let (flora, terrain) = (default_flora(), Terrain::default());
        let heart = monte_heart(&flora);
        let alpha = |f: Option<&FloraSite>, p| ground_color(&terrain, f, p)[3];
        assert_eq!(alpha(Some(&flora), heart), 1.0);
        // Open field: the first point of a scan with no monte at all.
        let open = (0..400)
            .map(|i| GroundVec2::new(i as f32 * 37.0, 11.0 * i as f32))
            .find(|&p| flora.monte(p) == 0.0)
            .expect("open pampa");
        assert_eq!(alpha(Some(&flora), open), 0.0);
        // Flora not there yet: no litter, whatever the monte would say.
        assert_eq!(alpha(None, heart), 0.0);
        // Walking out of the patch's heart a meter at a time, the weight
        // falls to nothing through a feather, never in a jump.
        let walk: Vec<f32> = (0..600)
            .map(|m| alpha(Some(&flora), heart + GroundVec2::new(m as f32, 0.0)))
            .collect();
        assert!(walk.contains(&0.0), "the patch ends within 600 m");
        assert!(walk.iter().any(|a| (0.1..0.9).contains(a)), "no feather");
        assert!(
            walk.windows(2).all(|w| (w[1] - w[0]).abs() < 0.1),
            "{walk:?}"
        );
    }

    #[test]
    fn a_towns_leveled_ground_keeps_the_monte_litter_off() {
        let (flora, plain) = (default_flora(), Terrain::default());
        let heart = monte_heart(&flora);
        let town = plain.clone().with_flat_area(FlatArea {
            center: heart,
            half_extents: GroundVec2::new(40.0, 40.0),
            falloff: 60.0,
            height: 0.0,
        });
        let alpha = |t: &Terrain, p| ground_color(t, Some(&flora), p)[3];
        assert_eq!(alpha(&plain, heart), 1.0);
        assert_eq!(alpha(&town, heart), 0.0);
        // The falloff band feathers it back in.
        let rim = heart + GroundVec2::new(70.0, 0.0);
        assert!((0.0..1.0).contains(&alpha(&town, rim)) && alpha(&town, rim) > 0.0);
    }

    #[test]
    fn ground_uvs_tile_in_world_space_and_meet_at_chunk_borders() {
        use bevy::mesh::VertexAttributeValues as V;
        let terrain = Terrain::default();
        let attrs = |c: ChunkCoord| {
            let mesh = build_chunk_mesh(&terrain, c, 200.0, 8, None);
            let Some(V::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned() else {
                panic!("positions")
            };
            let Some(V::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).cloned() else {
                panic!("uvs")
            };
            let Some(V::Float32x4(t)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT).cloned() else {
                panic!("tangents")
            };
            assert_eq!(uv.len(), p.len());
            assert_eq!(t.len(), p.len());
            p.into_iter().zip(uv).collect::<Vec<_>>()
        };
        let mut shared = 0;
        for (a, b) in [
            (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)),
            (ChunkCoord::new(3, -2), ChunkCoord::new(3, -1)),
        ] {
            let (a, b) = (attrs(a), attrs(b));
            for (p, uv) in &a {
                // One tile every `TILE` meters of world X and Z.
                assert!((uv[0] * crate::ground::TILE - p[0]).abs() < 1e-3);
                assert!((uv[1] * crate::ground::TILE - p[2]).abs() < 1e-3);
                for (q, w) in &b {
                    if p == q {
                        assert_eq!(uv, w, "UV seam at {p:?}");
                        shared += 1;
                    }
                }
            }
        }
        // An 8-quad border shares its 9 corners, once each now the grid is
        // indexed.
        assert_eq!(shared, 2 * 9);
    }

    /// A mesh's positions, normals and tangents.
    type Vectors = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 4]>);

    fn vectors(mesh: &Mesh) -> Vectors {
        use bevy::mesh::VertexAttributeValues as V;
        let Some(V::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned() else {
            panic!("positions")
        };
        let Some(V::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).cloned() else {
            panic!("normals")
        };
        let Some(V::Float32x4(t)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT).cloned() else {
            panic!("tangents")
        };
        (p, n, t)
    }

    #[test]
    fn ground_normals_are_smooth_and_meet_exactly_across_chunk_borders() {
        let terrain = Terrain::default();
        let mut shared = 0;
        for (a, b) in [
            (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)),
            (ChunkCoord::new(3, -2), ChunkCoord::new(3, -1)),
            (ChunkCoord::new(-4, 7), ChunkCoord::new(-3, 8)),
        ] {
            // Every vertex at one position carries one normal and tangent,
            // within a chunk and across the border.
            let mut at: HashMap<[u32; 3], ([f32; 3], [f32; 4])> = HashMap::new();
            for coord in [a, b] {
                let (p, n, t) = vectors(&build_chunk_mesh(&terrain, coord, 200.0, 8, None));
                for ((p, n), t) in p.into_iter().zip(n).zip(t) {
                    match at.get(&p.map(f32::to_bits)) {
                        Some(&(m, u)) => {
                            assert_eq!((m, u), (n, t), "seam at {p:?}");
                            shared += 1;
                        }
                        None => {
                            at.insert(p.map(f32::to_bits), (n, t));
                        }
                    }
                }
            }
        }
        // Two 8-quad borders and one diagonal corner.
        assert_eq!(shared, 9 + 9 + 1);
    }

    #[test]
    fn ground_normals_follow_the_slope_of_the_height_field() {
        // A plane rising half a meter east and a quarter north per meter.
        let (n, t) = slope_frame(0.5, 0.25);
        let (n, tv) = (Vec3::from_array(n), Vec3::new(t[0], t[1], t[2]));
        let east = world_from_ground(GroundVec2::new(1.0, 0.0), 0.5);
        let north = world_from_ground(GroundVec2::new(0.0, 1.0), 0.25);
        assert!((n.length() - 1.0).abs() < 1e-6 && n.y > 0.0, "{n:?}");
        assert!(
            n.dot(east).abs() < 1e-6 && n.dot(north).abs() < 1e-6,
            "{n:?}"
        );
        // The tangent is the one a face on that plane gets, sign and all.
        let face = [Vec3::ZERO, east, east + north];
        let flat = face_tangent(face, face.map(ground_uv));
        assert!(
            (tv - Vec3::new(flat[0], flat[1], flat[2])).length() < 1e-5,
            "{t:?} {flat:?}"
        );
        assert_eq!(t[3], flat[3]);
        // Level ground faces straight up.
        assert_eq!(slope_frame(0.0, 0.0).0, [0.0, 1.0, 0.0]);

        // On the real terrain, each vertex's normal is the height field's
        // slope over one quad either way.
        let terrain = Terrain::default();
        let (size, res) = (200.0, 8);
        let q = size / res as f32;
        let (p, n, _) = vectors(&build_chunk_mesh(
            &terrain,
            ChunkCoord::new(2, -1),
            size,
            res,
            None,
        ));
        let mut tilted = 0;
        for (p, n) in p.into_iter().zip(n) {
            let (x, y) = (p[0], -p[2]);
            let h = |x: f32, y: f32| terrain.height(x, y);
            let dx = (h(x + q, y) - h(x - q, y)) / (2.0 * q);
            let dy = (h(x, y + q) - h(x, y - q)) / (2.0 * q);
            let expected = Vec3::from_array(slope_frame(dx, dy).0);
            let got = Vec3::from_array(n);
            assert!(
                got.dot(expected) > 0.99999,
                "{p:?}: {got:?} vs {expected:?}"
            );
            tilted += usize::from(got.y < 0.99999);
        }
        assert!(tilted > 10, "the test terrain should not be flat: {tilted}");
    }

    #[test]
    fn ground_colour_is_continuous_across_chunk_borders_and_in_the_palette() {
        let town = TownLayout::castex_inspired();
        let terrain = Terrain::default();
        let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
        let colors = |c: ChunkCoord| {
            let mesh = build_chunk_mesh(&terrain, c, 200.0, 8, None);
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned()
            else {
                panic!("positions")
            };
            let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR).cloned()
            else {
                panic!("colors")
            };
            p.into_iter().zip(c).collect::<Vec<([f32; 3], [f32; 4])>>()
        };
        let (lo, hi) = palette_range();
        let mut shared = 0;
        for (a, b) in [
            (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0)),
            (ChunkCoord::new(3, -2), ChunkCoord::new(3, -1)),
            (ChunkCoord::new(-1, 0), ChunkCoord::new(0, 0)),
        ] {
            let (a, b) = (colors(a), colors(b));
            for (p, c) in &a {
                for k in 0..3 {
                    assert!(c[k] >= lo[k] - 1e-4 && c[k] <= hi[k] + 1e-4, "{c:?}");
                }
                for (q, d) in &b {
                    if p == q {
                        assert_eq!(c, d, "seam at {p:?}");
                        shared += 1;
                    }
                }
            }
        }
        assert_eq!(shared, 3 * 9, "every border corner is shared once");
        // The grass varies from dry yellow to grey-green between towns.
        let open = colors(ChunkCoord::new(6, 6));
        let greens: Vec<f32> = open.iter().map(|(_, c)| c[1] / c[0]).collect();
        let (gmin, gmax) = greens
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &g| (a.min(g), b.max(g)));
        let mut spread = gmax - gmin;
        for c in [ChunkCoord::new(-8, 4), ChunkCoord::new(12, -9)] {
            for (_, col) in colors(c) {
                spread = spread.max((col[1] / col[0] - gmin).abs());
            }
        }
        assert!(spread > 0.1, "{spread}");
    }

    #[test]
    fn only_a_narrow_band_of_mud_rings_a_lagoon() {
        let config = pampa_core::LagoonConfig {
            name: "Laguna".into(),
            center: GroundVec2::new(3401.0, 841.0),
            size: GroundVec2::new(215.0, 145.0),
            seed: 5,
        };
        let lagoon = pampa_core::Lagoon::new(&config, &Terrain::default());
        let terrain = Terrain::default().with_lagoon(lagoon.clone());
        for p in lagoon.ring(64, -1.0) {
            assert_eq!(lagoon_wetness(&terrain, p), 1.0);
        }
        for p in lagoon.ring(64, pampa_core::Lagoon::MUD_BAND + 0.1) {
            assert_eq!(lagoon_wetness(&terrain, p), 0.0, "{p:?}");
        }
        // Past the mud, stones and shrubs come back.
        let keep = lagoon.keep_out();
        let (lo, hi) = (
            ChunkCoord::containing(keep.min, 200.0),
            ChunkCoord::containing(keep.max, 200.0),
        );
        let mut near = 0;
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                for prop in chunk_props(&terrain, ChunkCoord { x: cx, y: cy }, 200.0, None) {
                    assert!(!lagoon.is_bare(prop.at));
                    near += usize::from(lagoon.offshore(prop.at) < 30.0);
                }
            }
        }
        assert!(near > 3, "{near}");
    }
}
