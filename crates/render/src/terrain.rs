//! Low-poly ground streamed in square chunks around the rider, each with
//! scattered props (tosca stones and small jarilla shrubs, drawn with the
//! CC0 vegetation models when they are loaded) that give the eye something
//! to measure speed by. The ground is painted per vertex, from grey-green to
//! dry yellow grass, with plain earth on towns' leveled ground, and wears
//! the CC0 ambientCG soil-and-grass detail textures of [`crate::ground`]
//! over that paint. The native flora on it is grown by [`crate::flora`].
//!
//! Every chunk is sampled from the logic [`Terrain`], the single source of
//! truth for height, so neighboring chunks meet without seams. Which chunks
//! exist is decided by a [`StreamingSet`] around [`StreamFocus`]: chunks
//! within the load radius are built, a bounded number per frame, and chunks
//! past the radius plus one are despawned with their props.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use pampa_core::flora::grass_dryness;
use pampa_core::{ChunkCoord, Terrain, Vec2 as GroundVec2, WorldConfig};

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::flora::{Model, VegetationLoad, VegetationModels, spot_unit, visible_within};
use crate::ground::{face_tangent, ground_material, ground_uv};
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

#[derive(Resource, Clone, Copy)]
struct ChunkDetail {
    resolution: u32,
}

/// Meshes and materials shared by every chunk.
#[derive(Resource)]
struct TerrainKit {
    ground: Handle<StandardMaterial>,
    rock_mesh: Handle<Mesh>,
    shrub_mesh: Handle<Mesh>,
    rock_material: Handle<StandardMaterial>,
    shrub_material: Handle<StandardMaterial>,
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTerrain>()
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
            .add_systems(PostUpdate, stream_chunks.in_set(ChunkStreaming));
    }
}

fn setup_kit(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    commands.insert_resource(TerrainKit {
        // Vertex colors carry the palette; the material stays white and
        // matte, with the tiled ground detail on top when it is found.
        ground: materials.add(ground_material(
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
    models: Option<Res<VegetationModels>>,
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
            coord,
        );
        streamer.entities.insert(coord, entity);
    }
}

/// Builds the next few chunks near the focus and drops the far ones.
fn stream_chunks(
    mut commands: Commands,
    (terrain, world, detail, focus): ChunkInputs,
    kit: Option<Res<TerrainKit>>,
    models: Option<Res<VegetationModels>>,
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
            coord,
        );
        streamer.entities.insert(coord, entity);
    }
}

/// Spawns chunk `coord`: its ground mesh, and its props as children.
fn spawn_chunk(
    commands: &mut Commands,
    (kit, models): (&TerrainKit, Option<&VegetationModels>),
    meshes: &mut Assets<Mesh>,
    (terrain, world, detail): (&Terrain, &WorldConfig, ChunkDetail),
    coord: ChunkCoord,
) -> Entity {
    let mesh = build_chunk_mesh(terrain, coord, world.chunk_size, detail.resolution);
    let chunk = commands
        .spawn((
            Name::new(format!("Terrain chunk {} {}", coord.x, coord.y)),
            TerrainChunk(coord),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(kit.ground.clone()),
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    for prop in chunk_props(terrain, coord, world.chunk_size) {
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

/// Non-indexed triangle list so normals come out flat: the low-poly look.
/// Vertices are in world space; the chunk entity stays at the origin. UVs
/// are world space too ([`ground_uv`]), with flat per-face tangents for the
/// ground's normal map.
fn build_chunk_mesh(terrain: &Terrain, coord: ChunkCoord, size: f32, resolution: u32) -> Mesh {
    let corner = |i: u32, j: u32| {
        let ground = chunk_corner(coord, size, resolution, i, j);
        (
            world_from_ground(ground, terrain.height(ground.x, ground.y)),
            ground_color(terrain, ground),
        )
    };

    let quads = (resolution * resolution) as usize;
    let mut positions = Vec::with_capacity(quads * 6);
    let mut colors = Vec::with_capacity(quads * 6);
    let mut uvs = Vec::with_capacity(quads * 6);
    let mut tangents = Vec::with_capacity(quads * 6);
    for j in 0..resolution {
        for i in 0..resolution {
            let (a, b) = (corner(i, j), corner(i + 1, j));
            let (c, d) = (corner(i + 1, j + 1), corner(i, j + 1));
            // World Z is -north, so (a, b, c) is counter-clockwise seen from above.
            for tri in [[a, b, c], [a, c, d]] {
                let points = tri.map(|(v, _)| v);
                let uv = points.map(ground_uv);
                positions.extend(points.map(|v| v.to_array()));
                colors.extend(tri.map(|(_, c)| c));
                tangents.extend([face_tangent(points, uv); 3]);
                uvs.extend(uv);
            }
        }
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
    mesh
}

/// Grey-green grass where the pampa is greener (sRGB).
const GREEN_GRASS: Vec3 = Vec3::new(0.49, 0.54, 0.38);
/// Dry yellow grass (sRGB).
const DRY_GRASS: Vec3 = Vec3::new(0.71, 0.63, 0.40);
/// Trodden earth of a town's leveled ground (sRGB): plain, so streets and
/// buildings read on it.
const TOWN_GROUND: Vec3 = Vec3::new(0.60, 0.53, 0.40);
/// Darkest shade slopes and dips may give the ground.
const SHADE_MIN: f32 = 0.82;

/// The darkest and brightest linear colour the ground may take.
#[cfg(test)]
fn palette_range() -> ([f32; 3], [f32; 3]) {
    let colors = [GREEN_GRASS, DRY_GRASS, TOWN_GROUND];
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
fn ground_color(terrain: &Terrain, g: GroundVec2) -> [f32; 4] {
    let grass = GREEN_GRASS.lerp(DRY_GRASS, grass_dryness(terrain, g));
    let town = terrain
        .flat_areas
        .iter()
        .map(|a| a.weight(g))
        .fold(0.0, f32::max);
    let rgb = grass.lerp(TOWN_GROUND, town);
    let e = 0.75;
    let dx = terrain.height(g.x + e, g.y) - terrain.height(g.x - e, g.y);
    let dy = terrain.height(g.x, g.y + e) - terrain.height(g.x, g.y - e);
    let slope = dx.hypot(dy) / (2.0 * e);
    let dip = 0.5 - 0.5 * (terrain.height(g.x, g.y) / terrain.amplitude).clamp(-1.0, 1.0);
    let shade = (1.0 - 0.6 * slope - 0.05 * dip).clamp(SHADE_MIN, 1.0);
    let rgb = rgb * shade;
    LinearRgba::from(Color::srgb(rgb.x, rgb.y, rgb.z)).to_f32_array()
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

/// The props of chunk `coord`, seeded by its coordinate, skipping leveled
/// ground (settlements are cleared land).
fn chunk_props(terrain: &Terrain, coord: ChunkCoord, size: f32) -> Vec<Prop> {
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
            let leveled = terrain.flat_areas.iter().any(|a| a.weight(at) > 0.0);
            (!leveled).then_some(prop)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mesh = build_chunk_mesh(&terrain, ChunkCoord::new(2, -1), size, 4);
        assert_eq!(mesh.count_vertices(), 4 * 4 * 6);
    }

    #[test]
    fn props_are_seeded_per_chunk_and_skip_leveled_ground() {
        let terrain = Terrain::default();
        let c = ChunkCoord::new(4, -2);
        let props = chunk_props(&terrain, c, 200.0);
        assert_eq!(props, chunk_props(&terrain, c, 200.0));
        assert_eq!(props.len(), 44);
        assert!(props.iter().all(|p| c.bounds(200.0).contains_point(p.at)));
        assert_ne!(props, chunk_props(&terrain, ChunkCoord::new(5, -2), 200.0));

        let town = TownLayout::castex_inspired();
        let leveled = terrain.clone().with_flat_area(town.flat_area(&terrain));
        let area: FlatArea = leveled.flat_areas[0];
        for c in [ChunkCoord::new(0, 0), ChunkCoord::new(-1, -1)] {
            let props = chunk_props(&leveled, c, 200.0);
            assert!(props.len() < 44);
            assert!(props.iter().all(|p| area.weight(p.at) == 0.0));
        }
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
                .query::<(&MeshMaterial3d<StandardMaterial>, &TerrainChunk)>();
            let id = q.iter(app.world()).next().expect("a chunk").0.id();
            let materials = app.world().resource::<Assets<StandardMaterial>>();
            let m = materials.get(id).unwrap().clone();
            let images = app.world().resource::<Assets<Image>>();
            let sizes: Vec<u32> = [&m.base_color_texture, &m.normal_map_texture]
                .into_iter()
                .flatten()
                .map(|h| images.get(h).unwrap().texture_descriptor.mip_level_count)
                .collect();
            (m, sizes)
        };
        let (textured, mips) = material(root.clone());
        assert!(textured.base_color_texture.is_some());
        assert!(textured.normal_map_texture.is_some());
        assert!(textured.occlusion_texture.is_some());
        assert!(textured.metallic_roughness_texture.is_some());
        assert_eq!(textured.metallic, 0.0);
        assert!(mips.iter().all(|&m| m == 10), "512 px down to 1: {mips:?}");
        // Missing files: plain vertex-painted ground, no panic.
        let (plain, _) = material(root.join("nowhere"));
        assert!(plain.base_color_texture.is_none() && plain.normal_map_texture.is_none());
    }

    #[test]
    fn ground_uvs_tile_in_world_space_and_meet_at_chunk_borders() {
        use bevy::mesh::VertexAttributeValues as V;
        let terrain = Terrain::default();
        let attrs = |c: ChunkCoord| {
            let mesh = build_chunk_mesh(&terrain, c, 200.0, 8);
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
        assert!(shared > 20, "{shared}");
    }

    #[test]
    fn ground_colour_is_continuous_across_chunk_borders_and_in_the_palette() {
        let town = TownLayout::castex_inspired();
        let terrain = Terrain::default();
        let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
        let colors = |c: ChunkCoord| {
            let mesh = build_chunk_mesh(&terrain, c, 200.0, 8);
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
        assert!(shared > 50, "{shared}");
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
}
