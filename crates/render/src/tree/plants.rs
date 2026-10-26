//! Yard shrubs and flower beds: little low-poly plants for the yards of the
//! towns and the settlements.
//!
//! A shrub is a few lumpy leaf masses (once-subdivided icospheres, as the
//! settlements' crowns are), a flower bed a mound of earth with a green
//! tuft and about ten octahedral blossoms. Each is one mesh with vertex
//! colours and flat shading, so one white material paints them all.
//! [`VARIANTS`] seeded shapes of each are built once into the shared
//! [`PlantKit`], and every plant is a variant turned about the vertical.
//! They are scenery: no obstacles.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

use crate::settlement::crown::{icosphere, unit};

/// Seeded shapes built of each plant.
pub const VARIANTS: u32 = 3;

/// The blossoms' colours (sRGB): red, yellow, white, violet and orange.
const BLOSSOMS: [[f32; 3]; 5] = [
    [0.85, 0.15, 0.12],
    [0.95, 0.80, 0.15],
    [0.95, 0.95, 0.90],
    [0.60, 0.30, 0.70],
    [0.95, 0.50, 0.15],
];
/// The leaves' colours, light and dark.
const GREENS: [[f32; 3]; 2] = [[0.22, 0.34, 0.14], [0.30, 0.40, 0.18]];
/// The flower bed's earth.
const SOIL: [f32; 3] = [0.28, 0.19, 0.12];

/// One lump of a plant: an ellipsoid (or an octahedron) of `radii` at
/// `center`, lumpy by `seed`.
struct Blob {
    center: Vec3,
    radii: Vec3,
    color: [f32; 3],
    seed: u32,
    octahedron: bool,
}

/// The six points and eight faces of an octahedron on the unit sphere.
fn octahedron() -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let points = vec![
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let faces = vec![
        [0, 2, 4],
        [4, 2, 1],
        [1, 2, 5],
        [5, 2, 0],
        [4, 3, 0],
        [1, 3, 4],
        [5, 3, 1],
        [0, 3, 5],
    ];
    (points, faces)
}

/// The mesh of `blobs`: flat-shaded, a little darker low down.
fn build(blobs: &[Blob]) -> Mesh {
    let (sphere, faces) = icosphere();
    let (cone, cone_faces) = octahedron();
    let (mut positions, mut colors) = (Vec::new(), Vec::new());
    for blob in blobs {
        let (points, faces) = if blob.octahedron {
            (&cone, &cone_faces)
        } else {
            (&sphere, &faces)
        };
        let moved: Vec<Vec3> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let lump = if blob.octahedron {
                    1.0
                } else {
                    0.85 + 0.3 * unit(blob.seed, i as u32)
                };
                blob.center + *p * blob.radii * lump
            })
            .collect();
        for face in faces {
            for &i in face {
                let p = moved[i as usize];
                let shade = 0.75 + 0.25 * (p.y / 1.0).clamp(0.0, 1.0);
                let c = LinearRgba::from(Color::srgb(blob.color[0], blob.color[1], blob.color[2]));
                positions.push(p.to_array());
                colors.push([c.red * shade, c.green * shade, c.blue * shade, 1.0]);
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

/// Shrub `variant`: four lumpy masses about a meter tall, within 0.6 m of
/// its middle.
pub fn shrub(variant: u32) -> Mesh {
    let seed = variant.wrapping_mul(7919) ^ 0x5b3d;
    let r = |i: u32| unit(seed, i);
    let blobs: Vec<Blob> = (0..4)
        .map(|k| {
            let angle = std::f32::consts::TAU * (k as f32 + 0.3 * r(k)) / 4.0;
            let out = 0.12 + 0.1 * r(10 + k);
            let size = 0.3 + 0.08 * r(20 + k);
            Blob {
                center: Vec3::new(
                    angle.cos() * out,
                    0.38 + 0.12 * r(30 + k),
                    angle.sin() * out,
                ),
                radii: Vec3::new(size, size * 0.85, size),
                color: GREENS[(k % 2) as usize],
                seed: seed ^ k,
                octahedron: false,
            }
        })
        .collect();
    build(&blobs)
}

/// Flower bed `variant`: a mound of earth, a tuft of leaves and ten
/// blossoms, within 0.7 m of its middle and a third of a meter high.
pub fn flower_bed(variant: u32) -> Mesh {
    let seed = variant.wrapping_mul(104_729) ^ 0xf10e;
    let r = |i: u32| unit(seed, i);
    let mut blobs = vec![
        Blob {
            center: Vec3::new(0.0, 0.04, 0.0),
            radii: Vec3::new(0.58, 0.1, 0.4),
            color: SOIL,
            seed,
            octahedron: false,
        },
        Blob {
            center: Vec3::new(0.0, 0.12, 0.0),
            radii: Vec3::new(0.5, 0.14, 0.3),
            color: GREENS[0],
            seed: seed ^ 1,
            octahedron: false,
        },
    ];
    for k in 0..10 {
        let angle = std::f32::consts::TAU * r(40 + k);
        let out = r(50 + k).sqrt();
        let size = 0.07 + 0.03 * r(60 + k);
        blobs.push(Blob {
            center: Vec3::new(
                angle.cos() * out * 0.5,
                0.2 + 0.08 * r(70 + k),
                angle.sin() * out * 0.3,
            ),
            radii: Vec3::splat(size),
            color: BLOSSOMS[((r(80 + k) * BLOSSOMS.len() as f32) as usize) % BLOSSOMS.len()],
            seed: seed ^ k,
            octahedron: true,
        });
    }
    build(&blobs)
}

/// The shared plant meshes and their one material, built at startup with
/// the [`crate::tree::TreeKit`].
#[derive(Debug, Clone)]
pub struct PlantKit {
    pub shrubs: Vec<Handle<Mesh>>,
    pub flowers: Vec<Handle<Mesh>>,
    /// White with vertex colours: every plant's paint.
    pub material: Handle<StandardMaterial>,
}

impl PlantKit {
    /// Adds the meshes and the material to `meshes` and `materials`.
    pub(crate) fn build(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        Self {
            shrubs: (0..VARIANTS).map(|v| meshes.add(shrub(v))).collect(),
            flowers: (0..VARIANTS).map(|v| meshes.add(flower_bed(v))).collect(),
            material: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.95,
                reflectance: 0.05,
                ..default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flora::triangles;

    fn points(mesh: &Mesh) -> Vec<Vec3> {
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        p.iter().map(|p| Vec3::from_array(*p)).collect()
    }

    fn reach(mesh: &Mesh) -> (f32, f32, f32) {
        let p = points(mesh);
        let around = p
            .iter()
            .map(|p| Vec2::new(p.x, p.z).length())
            .fold(0.0, f32::max);
        let (lo, hi) = p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
        (around, lo, hi)
    }

    #[test]
    fn shrubs_are_a_meter_of_leaf_masses_within_the_ground_core_gives_them() {
        let mut shapes = Vec::new();
        for v in 0..VARIANTS {
            let mesh = shrub(v);
            let (around, lo, hi) = reach(&mesh);
            // Core gives a shrub 0.6 m to either side.
            assert!((0.35..=0.6).contains(&around), "shrub {v}: {around}");
            assert!(
                lo >= 0.0 && (0.6..=1.0).contains(&hi),
                "shrub {v}: {lo}..{hi}"
            );
            // Four masses of 80 faces.
            assert_eq!(triangles(&mesh), 320, "shrub {v}");
            shapes.push(points(&mesh));
        }
        assert!(shapes[0] != shapes[1] && shapes[1] != shapes[2]);
    }

    #[test]
    fn flower_beds_are_low_with_ten_blossoms_in_five_colors_within_their_ground() {
        for v in 0..VARIANTS {
            let mesh = flower_bed(v);
            let (around, lo, hi) = reach(&mesh);
            // Core gives a bed 0.7 m to either side; low, under 0.4 m, its
            // earth sunk a few centimeters into the ground.
            assert!((0.5..=0.7).contains(&around), "bed {v}: {around}");
            assert!(
                lo >= -0.1 && (0.2..=0.4).contains(&hi),
                "bed {v}: {lo}..{hi}"
            );
            // Soil and tuft (80 faces each) and ten blossoms of eight.
            assert_eq!(triangles(&mesh), 2 * 80 + 10 * 8, "bed {v}");
        }
        let colors: std::collections::HashSet<[u32; 3]> = (0..VARIANTS)
            .flat_map(|v| {
                let mesh = flower_bed(v);
                let Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                else {
                    panic!()
                };
                // The brightest of each blossom's top vertices: its own color.
                c.iter()
                    .filter(|c| c[0] > 0.5 || c[1] > 0.5)
                    .map(|c| {
                        [
                            (c[0] * 8.0) as u32,
                            (c[1] * 8.0) as u32,
                            (c[2] * 8.0) as u32,
                        ]
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(colors.len() >= 5, "{} blossom shades", colors.len());
    }
}
