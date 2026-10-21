//! Native flora on the streamed chunks: caldén and chañar trees and grass
//! tufts, placed by [`pampa_core::FloraSite`] and drawn as faceted low-poly
//! meshes in the buildings' flat-shaded style.
//!
//! Every mesh and material is built once in a [`FloraKit`] and shared, so
//! Bevy batches identical draws. A chunk's flora are its children and go
//! with it when it is dropped.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use pampa_core::{FloraSite, Species, Tree, TuftKind};

use crate::character::finish::Finish;
use crate::coords::world_from_ground;
use crate::terrain::{ChunkStreaming, TerrainChunk, WorldTerrain};

/// Where the flora grows. Insert it before the plugin runs; without it,
/// chunks grow nothing.
#[derive(Resource, Debug, Clone)]
pub struct WorldFlora(pub FloraSite);

/// A drawn tree part (trunk or crown), a child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreePart(pub Species);

/// A drawn grass tuft, a child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrassTuft(pub TuftKind);

/// Marks a chunk whose flora is grown.
#[derive(Component)]
struct FloraGrown;

/// Grows flora on terrain chunks as they are streamed in. Needs the
/// [`crate::TerrainPlugin`].
pub struct FloraPlugin;

impl Plugin for FloraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTerrain>()
            .add_systems(Startup, setup_kit)
            // Right after streaming, like the track: new chunks get their
            // flora the frame they appear, before transforms propagate.
            .add_systems(
                PostUpdate,
                grow_flora
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Tufts drawn per chunk, at most.
pub const TUFTS_DRAWN: usize = 300;

/// Height the tree meshes are built at, per species; each tree is scaled
/// from it.
fn reference_height(species: Species) -> f32 {
    match species {
        Species::Calden => 7.5,
        Species::Chanar => 4.0,
    }
}

/// Shared tree and tuft meshes and materials.
#[derive(Resource)]
pub struct FloraKit {
    calden_trunk: Handle<Mesh>,
    calden_crown: Handle<Mesh>,
    chanar_trunk: Handle<Mesh>,
    chanar_crown: Handle<Mesh>,
    paja: Handle<Mesh>,
    flechilla: Handle<Mesh>,
    bark: Handle<StandardMaterial>,
    leaves: Handle<StandardMaterial>,
    grass: Handle<StandardMaterial>,
}

impl FloraKit {
    fn tree(&self, species: Species) -> [(Handle<Mesh>, Handle<StandardMaterial>); 2] {
        let (trunk, crown) = match species {
            Species::Calden => (&self.calden_trunk, &self.calden_crown),
            Species::Chanar => (&self.chanar_trunk, &self.chanar_crown),
        };
        [
            (trunk.clone(), self.bark.clone()),
            (crown.clone(), self.leaves.clone()),
        ]
    }

    fn tuft(&self, kind: TuftKind) -> Handle<Mesh> {
        match kind {
            TuftKind::Paja => self.paja.clone(),
            TuftKind::Flechilla => self.flechilla.clone(),
        }
    }
}

/// A white material with `finish`: vertex colours carry the paint.
fn finished(finish: Finish, double_sided: bool) -> StandardMaterial {
    let (perceptual_roughness, metallic, reflectance) = finish.pbr();
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness,
        metallic,
        reflectance,
        double_sided,
        cull_mode: (!double_sided).then_some(bevy::render::render_resource::Face::Back),
        ..default()
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FloraKit {
        calden_trunk: meshes.add(calden_trunk()),
        calden_crown: meshes.add(calden_crown()),
        chanar_trunk: meshes.add(chanar_trunk()),
        chanar_crown: meshes.add(chanar_crown()),
        paja: meshes.add(tuft_mesh(TuftKind::Paja)),
        flechilla: meshes.add(tuft_mesh(TuftKind::Flechilla)),
        bark: materials.add(finished(Finish::Bark, false)),
        leaves: materials.add(finished(Finish::Leaves, false)),
        grass: materials.add(finished(Finish::Leaves, true)),
    });
}

/// Grows the flora of every chunk built since the last run. A chunk that
/// vanishes before the commands apply simply gets none.
fn grow_flora(
    mut commands: Commands,
    flora: Option<Res<WorldFlora>>,
    terrain: Res<WorldTerrain>,
    kit: Option<Res<FloraKit>>,
    chunks: Query<(Entity, &TerrainChunk), Without<FloraGrown>>,
) {
    let (Some(flora), Some(kit)) = (flora, kit) else {
        return;
    };
    for (chunk, coord) in &chunks {
        let grown = flora.0.chunk(coord.0);
        let mut parts: Vec<(Mesh3d, MeshMaterial3d<StandardMaterial>, Transform, Plant)> =
            Vec::with_capacity(grown.trees.len() * 2 + TUFTS_DRAWN);
        for tree in &grown.trees {
            let transform = tree_transform(tree, &terrain.0);
            for (mesh, material) in kit.tree(tree.species) {
                parts.push((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    transform,
                    Plant::Tree(tree.species),
                ));
            }
        }
        for tuft in grown.tufts.iter().take(TUFTS_DRAWN) {
            let base = world_from_ground(tuft.at, terrain.0.height(tuft.at.x, tuft.at.y));
            parts.push((
                Mesh3d(kit.tuft(tuft.kind)),
                MeshMaterial3d(kit.grass.clone()),
                Transform::from_translation(base)
                    .with_rotation(Quat::from_rotation_y(tuft.yaw))
                    .with_scale(Vec3::splat(tuft.size)),
                Plant::Tuft(tuft.kind),
            ));
        }
        let mut grown_chunk = commands.entity(chunk);
        grown_chunk.try_insert(FloraGrown);
        if parts.is_empty() {
            continue;
        }
        grown_chunk.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|children| {
                for (mesh, material, transform, plant) in parts {
                    let mut part = children.spawn((mesh, material, transform));
                    match plant {
                        Plant::Tree(species) => part.insert(TreePart(species)),
                        Plant::Tuft(kind) => part.insert(GrassTuft(kind)),
                    };
                }
            });
        });
    }
}

#[derive(Clone, Copy)]
enum Plant {
    Tree(Species),
    Tuft(TuftKind),
}

/// Where a tree's meshes stand: on the terrain, turned and scaled from the
/// reference height, sunk a little so the trunk never floats on a slope.
fn tree_transform(tree: &Tree, terrain: &pampa_core::Terrain) -> Transform {
    let base = world_from_ground(tree.at, terrain.height(tree.at.x, tree.at.y) - 0.1);
    Transform::from_translation(base)
        .with_rotation(Quat::from_rotation_y(tree.yaw))
        .with_scale(Vec3::splat(tree.height / reference_height(tree.species)))
}

/// Triangles in a flat-shaded triangle-list mesh.
pub fn triangles(mesh: &Mesh) -> usize {
    mesh.count_vertices() / 3
}

/// A flat-shaded triangle soup with per-vertex colours.
#[derive(Default)]
struct Facets {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

/// An sRGB colour as linear vertex paint.
fn paint(r: f32, g: f32, b: f32) -> [f32; 4] {
    LinearRgba::from(Color::srgb(r, g, b)).to_f32_array()
}

fn shade(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

impl Facets {
    fn tri(&mut self, [a, b, c]: [Vec3; 3], colors: [[f32; 4]; 3]) {
        self.positions.extend([a, b, c].map(|v| v.to_array()));
        self.colors.extend(colors);
    }

    /// A tapered tube through `rings` (center, radius), `sides` facets
    /// round, closed at the top.
    fn loft(&mut self, rings: &[(Vec3, f32)], sides: u32, color: [f32; 4]) {
        let frames: Vec<Vec<Vec3>> = rings
            .iter()
            .enumerate()
            .map(|(k, &(center, radius))| {
                let next = rings.get(k + 1).or(rings.get(k.wrapping_sub(1)));
                let axis = next.map_or(Vec3::Y, |n| {
                    let d = if k + 1 < rings.len() {
                        n.0 - center
                    } else {
                        center - n.0
                    };
                    d.normalize_or(Vec3::Y)
                });
                let side = axis.any_orthonormal_vector();
                let other = axis.cross(side);
                (0..sides)
                    .map(|s| {
                        let a = s as f32 / sides as f32 * std::f32::consts::TAU;
                        center + (side * a.cos() + other * a.sin()) * radius
                    })
                    .collect()
            })
            .collect();
        let n = rings.len();
        for k in 0..n.saturating_sub(1) {
            // Darker low down: the trunk's foot sits in shade.
            let (lo, hi) = (
                shade(color, 0.7 + 0.3 * k as f32 / (n - 1) as f32),
                shade(color, 0.7 + 0.3 * (k + 1) as f32 / (n - 1) as f32),
            );
            for s in 0..sides as usize {
                let t = (s + 1) % sides as usize;
                let (a, b) = (frames[k][s], frames[k][t]);
                let (c, d) = (frames[k + 1][t], frames[k + 1][s]);
                self.tri([a, c, b], [lo, hi, lo]);
                self.tri([a, d, c], [lo, hi, hi]);
            }
        }
        if let (Some(top), Some(&(center, _))) = (frames.last(), rings.last()) {
            let c = shade(color, 1.0);
            for s in 0..sides as usize {
                let t = (s + 1) % sides as usize;
                self.tri([top[s], top[t], center], [c; 3]);
            }
        }
    }

    /// A lumpy leaf cluster: an icosphere stretched to `radii`, each vertex
    /// pushed in or out a little by `seed`. Its underside is darker.
    fn blob(&mut self, center: Vec3, radii: Vec3, seed: u32, color: [f32; 4]) {
        let (points, indices) = icosphere();
        let lumpy: Vec<Vec3> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let k = 0.82 + 0.3 * jitter(seed, i as u32);
                center + *p * radii * k
            })
            .collect();
        for tri in indices.as_chunks::<3>().0 {
            let v = [lumpy[tri[0]], lumpy[tri[1]], lumpy[tri[2]]];
            let colors = v.map(|p| {
                let up = ((p.y - center.y) / radii.y).clamp(-1.0, 1.0);
                shade(color, 0.78 + 0.22 * (up * 0.5 + 0.5))
            });
            self.tri(v, colors);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.compute_normals();
        mesh
    }
}

/// Unit icosphere (one subdivision: 80 faces), as points and indices.
fn icosphere() -> (Vec<Vec3>, Vec<usize>) {
    let mesh = Sphere::new(1.0)
        .mesh()
        .ico(1)
        .expect("valid ico subdivision");
    let points = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|&v| Vec3::from(v)).collect(),
        _ => Vec::new(),
    };
    let indices = mesh
        .indices()
        .map(|i| i.iter().collect())
        .unwrap_or_default();
    (points, indices)
}

/// Deterministic `[0, 1)` from a seed and an index.
fn jitter(seed: u32, i: u32) -> f32 {
    let mut v = seed ^ i.wrapping_mul(0x9e37_79b9);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v >> 8) as f32 / (1u32 << 24) as f32
}

/// Caldén bark: dark, grey-brown.
fn calden_bark() -> [f32; 4] {
    paint(0.27, 0.22, 0.18)
}

/// Chañar bark: smooth and greenish.
fn chanar_bark() -> [f32; 4] {
    paint(0.46, 0.49, 0.34)
}

/// A caldén 7.5 m tall: a short thick trunk forking into three limbs.
fn calden_trunk() -> Mesh {
    let mut f = Facets::default();
    let bark = calden_bark();
    let fork = Vec3::new(0.15, 2.2, 0.0);
    f.loft(
        &[
            (Vec3::ZERO, 0.3),
            (Vec3::new(0.05, 1.1, 0.02), 0.24),
            (fork, 0.2),
        ],
        7,
        bark,
    );
    for (k, angle) in [0.3_f32, 2.4, 4.4].into_iter().enumerate() {
        let out = Vec3::new(angle.cos(), 0.0, angle.sin());
        let mid = fork + out * 1.2 + Vec3::Y * 1.3;
        let tip = fork + out * (2.4 + 0.3 * k as f32) + Vec3::Y * 2.7;
        f.loft(&[(fork, 0.16), (mid, 0.11), (tip, 0.06)], 6, bark);
    }
    f.into_mesh()
}

/// A caldén's broad umbrella crown: a wide flat central cluster and four
/// around it, from about 4.3 m to the top.
fn calden_crown() -> Mesh {
    let mut f = Facets::default();
    let leaves = paint(0.34, 0.38, 0.21);
    f.blob(
        Vec3::new(0.0, 6.2, 0.0),
        Vec3::new(2.8, 1.1, 2.8),
        11,
        leaves,
    );
    for (k, angle) in [0.3_f32, 1.9, 3.3, 4.8].into_iter().enumerate() {
        let out = Vec3::new(angle.cos(), 0.0, angle.sin()) * 2.6;
        let tone = shade(leaves, 0.94 + 0.04 * k as f32);
        f.blob(
            out + Vec3::Y * (5.6 + 0.2 * k as f32),
            Vec3::new(1.8, 0.95, 1.7),
            20 + k as u32,
            tone,
        );
    }
    f.into_mesh()
}

/// A chañar 4 m tall: a slim leaning trunk splitting into two stems.
fn chanar_trunk() -> Mesh {
    let mut f = Facets::default();
    let bark = chanar_bark();
    let fork = Vec3::new(0.2, 1.6, 0.05);
    f.loft(
        &[
            (Vec3::ZERO, 0.13),
            (Vec3::new(0.1, 0.8, 0.0), 0.11),
            (fork, 0.09),
        ],
        6,
        bark,
    );
    for angle in [0.6_f32, 3.6] {
        let out = Vec3::new(angle.cos(), 0.0, angle.sin());
        f.loft(
            &[(fork, 0.08), (fork + out * 0.8 + Vec3::Y * 1.2, 0.04)],
            5,
            bark,
        );
    }
    f.into_mesh()
}

/// A chañar's small rounded crown of three clusters.
fn chanar_crown() -> Mesh {
    let mut f = Facets::default();
    let leaves = paint(0.40, 0.46, 0.25);
    f.blob(
        Vec3::new(0.2, 3.2, 0.0),
        Vec3::new(1.3, 0.8, 1.2),
        31,
        leaves,
    );
    f.blob(
        Vec3::new(0.9, 2.9, 0.4),
        Vec3::new(0.8, 0.6, 0.8),
        32,
        shade(leaves, 0.95),
    );
    f.blob(
        Vec3::new(-0.5, 2.8, -0.5),
        Vec3::new(0.8, 0.6, 0.8),
        33,
        shade(leaves, 1.03),
    );
    f.into_mesh()
}

/// A grass tuft one meter tall: thin blades fanned out from the root,
/// green-brown at the base and pale straw at the dry tips.
fn tuft_mesh(kind: TuftKind) -> Mesh {
    let (blades, spread, base, tip) = match kind {
        TuftKind::Paja => (9, 0.35, paint(0.42, 0.42, 0.24), paint(0.78, 0.70, 0.46)),
        TuftKind::Flechilla => (7, 0.55, paint(0.55, 0.52, 0.32), paint(0.86, 0.80, 0.58)),
    };
    let mut f = Facets::default();
    for b in 0..blades {
        let a = b as f32 / blades as f32 * std::f32::consts::TAU + jitter(7, b) * 0.6;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let across = Vec3::new(-out.z, 0.0, out.x) * 0.05;
        let lean = spread * (0.6 + 0.6 * jitter(9, b));
        let top = out * lean + Vec3::Y * (0.75 + 0.25 * jitter(5, b));
        let root = out * 0.04;
        f.tri([root - across, root + across, top], [base, base, tip]);
    }
    f.into_mesh()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{StreamFocus, TerrainPlugin};
    use pampa_core::{ChunkCoord, RailLine, Terrain, Vec2 as GroundVec2, WorldConfig};

    fn site() -> FloraSite {
        FloraSite::new(
            WorldConfig::default(),
            &Terrain::default(),
            &RailLine::default(),
        )
    }

    fn app(focus: GroundVec2) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldFlora(site()))
            .add_plugins((
                TerrainPlugin {
                    radius: 1,
                    budget: 9,
                    resolution: 4,
                },
                FloraPlugin,
            ));
        app
    }

    /// The center of a chunk holding a grove.
    fn grove_chunk() -> GroundVec2 {
        let size = WorldConfig::default().chunk_size;
        let flora = site();
        (-10..10)
            .flat_map(|x| (-10..10).map(move |y| ChunkCoord::new(x, y)))
            .find(|&c| flora.trees(c).len() >= 10)
            .expect("a grove")
            .center(size)
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<T>>();
        q.iter(app.world()).count()
    }

    #[test]
    fn a_chunk_grows_trees_and_tufts_and_drops_them_with_it() {
        let focus = grove_chunk();
        let mut app = app(focus);
        app.update();
        let flora = site();
        let world = WorldConfig::default();
        let loaded = world.chunks_near(focus, 1);
        let (trees, tufts): (usize, usize) = loaded
            .iter()
            .map(|&c| {
                let f = flora.chunk(c);
                (f.trees.len(), f.tufts.len().min(TUFTS_DRAWN))
            })
            .fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
        assert!(trees >= 10 && tufts > 100, "{trees} {tufts}");
        // Two parts per tree (trunk and crown), one entity per tuft, all
        // children of their chunk.
        assert_eq!(count::<TreePart>(&mut app), trees * 2);
        assert_eq!(count::<GrassTuft>(&mut app), tufts);
        let mut q = app
            .world_mut()
            .query_filtered::<&ChildOf, Or<(With<TreePart>, With<GrassTuft>)>>();
        let parents: Vec<Entity> = q.iter(app.world()).map(|c| c.parent()).collect();
        let mut chunks = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainChunk>>();
        let chunks: Vec<Entity> = chunks.iter(app.world()).collect();
        assert!(parents.iter().all(|p| chunks.contains(p)));

        // Ride far away: the old chunks' flora goes with them.
        app.world_mut().resource_mut::<StreamFocus>().0 = focus + GroundVec2::new(3000.0, 0.0);
        app.update();
        let far = world.chunks_near(focus + GroundVec2::new(3000.0, 0.0), 1);
        let expected: usize = far.iter().map(|&c| flora.chunk(c).trees.len() * 2).sum();
        assert_eq!(count::<TreePart>(&mut app), expected);
    }

    #[test]
    fn tufts_and_trees_share_their_meshes_and_materials() {
        let mut app = app(grove_chunk());
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>), With<GrassTuft>>();
        let mut meshes: Vec<AssetId<Mesh>> = Vec::new();
        let mut materials: Vec<AssetId<StandardMaterial>> = Vec::new();
        for (mesh, material) in q.iter(app.world()) {
            if !meshes.contains(&mesh.id()) {
                meshes.push(mesh.id());
            }
            if !materials.contains(&material.id()) {
                materials.push(material.id());
            }
        }
        assert!(meshes.len() <= 2 && materials.len() == 1, "{meshes:?}");
        let mut q = app.world_mut().query_filtered::<&Mesh3d, With<TreePart>>();
        let mut tree_meshes: Vec<AssetId<Mesh>> = Vec::new();
        for mesh in q.iter(app.world()) {
            if !tree_meshes.contains(&mesh.id()) {
                tree_meshes.push(mesh.id());
            }
        }
        assert!(tree_meshes.len() <= 4, "{}", tree_meshes.len());
    }

    #[test]
    fn meshes_are_faceted_and_cheap() {
        let calden = triangles(&calden_trunk()) + triangles(&calden_crown());
        let chanar = triangles(&chanar_trunk()) + triangles(&chanar_crown());
        assert!((300..=800).contains(&calden), "{calden}");
        assert!((150..=500).contains(&chanar), "{chanar}");
        for kind in [TuftKind::Paja, TuftKind::Flechilla] {
            assert!(triangles(&tuft_mesh(kind)) <= 10);
        }
        // The caldén's crown spreads wider than the chañar's.
        let width = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => {
                p.iter().map(|v| v[0].hypot(v[2])).fold(0.0, f32::max)
            }
            _ => 0.0,
        };
        assert!(width(&calden_crown()) / 7.5 > width(&chanar_crown()) / 4.0);
    }
}
