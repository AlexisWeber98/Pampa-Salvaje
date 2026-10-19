//! Low-poly ground streamed in square chunks around the rider, each with
//! scattered props (rocks and dry shrubs) that give the eye something to
//! measure speed by.
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
use pampa_core::{ChunkCoord, Terrain, Vec2 as GroundVec2, WorldConfig};

use crate::coords::world_from_ground;
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
            .add_systems(Startup, (setup_kit, fill_chunks).chain())
            .add_systems(PostUpdate, stream_chunks);
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(TerrainKit {
        // Vertex colors carry the palette; the material stays white and matte.
        ground: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            reflectance: 0.1,
            ..default()
        }),
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
            &kit,
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
            &kit,
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
    kit: &TerrainKit,
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
/// Vertices are in world space; the chunk entity stays at the origin.
fn build_chunk_mesh(terrain: &Terrain, coord: ChunkCoord, size: f32, resolution: u32) -> Mesh {
    let corner = |i: u32, j: u32| {
        let ground = chunk_corner(coord, size, resolution, i, j);
        world_from_ground(ground, terrain.height(ground.x, ground.y))
    };

    let quads = (resolution * resolution) as usize;
    let mut positions = Vec::with_capacity(quads * 6);
    let mut colors = Vec::with_capacity(quads * 6);
    for j in 0..resolution {
        for i in 0..resolution {
            let (a, b) = (corner(i, j), corner(i + 1, j));
            let (c, d) = (corner(i + 1, j + 1), corner(i, j + 1));
            // World Z is -north, so (a, b, c) is counter-clockwise seen from above.
            for tri in [[a, b, c], [a, c, d]] {
                let center = (tri[0] + tri[1] + tri[2]) / 3.0;
                let color = ground_color(terrain, center);
                positions.extend(tri.map(|v| v.to_array()));
                colors.extend([color; 3]);
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.compute_normals();
    mesh
}

/// Dry pampa palette: straw grass on the swells, darker earth in the dips,
/// with per-triangle jitter so the facets read.
fn ground_color(terrain: &Terrain, p: Vec3) -> [f32; 4] {
    let dry_grass = Vec3::new(0.64, 0.58, 0.38);
    let earth = Vec3::new(0.46, 0.38, 0.27);
    let t = ((p.y / terrain.amplitude) * 0.5 + 0.5).clamp(0.0, 1.0);
    let jitter = (hash2(p.x, p.z) - 0.5) * 0.08;
    let rgb = earth.lerp(dry_grass, t) + Vec3::splat(jitter);
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
}
