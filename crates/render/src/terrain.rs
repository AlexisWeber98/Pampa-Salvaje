//! Low-poly heightmap mesh built from the logic terrain, plus scattered props
//! (rocks and dry shrubs) that give the eye something to measure speed by.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use pampa_core::{PATCH_HALF_EXTENT, Terrain, Vec2 as GroundVec2};

use crate::coords::world_from_ground;

/// The logic terrain this world is drawn from. Insert before the plugin runs,
/// or the default terrain is used.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct WorldTerrain(pub Terrain);

/// Builds the terrain mesh and props at startup.
pub struct TerrainPlugin {
    /// Side length of the square patch, in meters, centered on the origin.
    /// Defaults to the core world bounds so the drawn ground is the playable area.
    pub size: f32,
    /// Quads per side.
    pub resolution: u32,
}

impl Default for TerrainPlugin {
    fn default() -> Self {
        Self {
            size: 2.0 * PATCH_HALF_EXTENT,
            resolution: 150,
        }
    }
}

#[derive(Resource, Clone, Copy)]
struct TerrainLayout {
    size: f32,
    resolution: u32,
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTerrain>()
            .insert_resource(TerrainLayout {
                size: self.size,
                resolution: self.resolution,
            })
            .add_systems(Startup, (spawn_terrain, spawn_props));
    }
}

fn spawn_terrain(
    mut commands: Commands,
    terrain: Res<WorldTerrain>,
    layout: Res<TerrainLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = build_terrain_mesh(&terrain.0, layout.size, layout.resolution);
    commands.spawn((
        Name::new("Terrain"),
        Mesh3d(meshes.add(mesh)),
        // Vertex colors carry the palette; the material stays white and matte.
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            reflectance: 0.1,
            ..default()
        })),
    ));
}

/// Non-indexed triangle list so normals come out flat: the low-poly look.
fn build_terrain_mesh(terrain: &Terrain, size: f32, resolution: u32) -> Mesh {
    let step = size / resolution as f32;
    let half = size / 2.0;
    let corner = |i: u32, j: u32| {
        let ground = GroundVec2::new(-half + i as f32 * step, -half + j as f32 * step);
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

fn spawn_props(
    mut commands: Commands,
    terrain: Res<WorldTerrain>,
    layout: Res<TerrainLayout>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let rock_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let shrub_mesh = meshes.add(
        Sphere::new(0.5)
            .mesh()
            .ico(1)
            .expect("valid ico subdivision"),
    );
    let rock_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.50, 0.44),
        perceptual_roughness: 0.95,
        ..default()
    });
    let shrub_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.40, 0.20),
        perceptual_roughness: 1.0,
        ..default()
    });

    let half = layout.size / 2.0 - 5.0;
    for n in 0..400u32 {
        let fx = hash2(n as f32, 1.7) * 2.0 - 1.0;
        let fy = hash2(n as f32, 9.3) * 2.0 - 1.0;
        let ground = GroundVec2::new(fx * half, fy * half);
        // Settlements are cleared ground: no rocks or shrubs on leveled land.
        if let Some(area) = terrain.0.flat_area
            && area.weight(ground) > 0.0
        {
            continue;
        }
        let size = 0.4 + hash2(n as f32, 4.1) * 1.2;
        let yaw = hash2(n as f32, 6.6) * std::f32::consts::TAU;
        let base = world_from_ground(ground, terrain.0.height(ground.x, ground.y));

        let (mesh, material, scale) = if n % 3 == 0 {
            (
                &rock_mesh,
                &rock_material,
                Vec3::new(size * 1.3, size * 0.6, size),
            )
        } else {
            (
                &shrub_mesh,
                &shrub_material,
                Vec3::new(size, size * 0.7, size),
            )
        };
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(base + Vec3::Y * scale.y * 0.35)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(scale),
        ));
    }
}
