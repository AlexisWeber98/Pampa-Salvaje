//! Arbustos y canteros de patio: pequeñas plantas low-poly para los patios de
//! los pueblos y de los asentamientos.
//!
//! Un arbusto son unas pocas masas de hojas irregulares (icosferas subdivididas
//! una vez); un cantero, un montículo de tierra con una mata verde y unas diez
//! flores octaédricas. Cada uno es una sola mesh con colores por vertex y
//! sombreado plano, así que un único material blanco los pinta a todos. Se
//! construyen una vez [`VARIANTS`] formas con semilla de cada uno en el
//! [`PlantKit`] compartido, y cada planta es una variante girada alrededor de
//! la vertical. Son decorado: no son obstáculos.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

/// Formas con semilla que se construyen de cada planta.
pub const VARIANTS: u32 = 3;

/// Los colores de las flores (sRGB): rojo, amarillo, blanco, violeta y naranja.
const BLOSSOMS: [[f32; 3]; 5] = [
    [0.85, 0.15, 0.12],
    [0.95, 0.80, 0.15],
    [0.95, 0.95, 0.90],
    [0.60, 0.30, 0.70],
    [0.95, 0.50, 0.15],
];
/// Los colores de las hojas, claro y oscuro.
const GREENS: [[f32; 3]; 2] = [[0.22, 0.34, 0.14], [0.30, 0.40, 0.18]];
/// La tierra del cantero.
const SOIL: [f32; 3] = [0.28, 0.19, 0.12];

/// Un valor determinista en `[0, 1)` para una semilla y un índice.
fn unit(seed: u32, i: u32) -> f32 {
    let mut x = seed
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(i.wrapping_mul(0x85eb_ca6b))
        ^ 0x27d4_eb2f;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Un icosaedro subdividido una vez sobre la esfera unitaria: 42 puntos, 80
/// triángulos en sentido antihorario.
fn icosphere() -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let mut points: Vec<Vec3> = [
        (-1.0, t, 0.0),
        (1.0, t, 0.0),
        (-1.0, -t, 0.0),
        (1.0, -t, 0.0),
        (0.0, -1.0, t),
        (0.0, 1.0, t),
        (0.0, -1.0, -t),
        (0.0, 1.0, -t),
        (t, 0.0, -1.0),
        (t, 0.0, 1.0),
        (-t, 0.0, -1.0),
        (-t, 0.0, 1.0),
    ]
    .map(|(x, y, z)| Vec3::new(x, y, z).normalize())
    .to_vec();
    let faces: [[u32; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let mut middles: HashMap<(u32, u32), u32> = HashMap::new();
    let mut middle = |a: u32, b: u32, points: &mut Vec<Vec3>| {
        *middles.entry((a.min(b), a.max(b))).or_insert_with(|| {
            points.push(((points[a as usize] + points[b as usize]) / 2.0).normalize());
            points.len() as u32 - 1
        })
    };
    let mut split = Vec::with_capacity(80);
    for [a, b, c] in faces {
        let ab = middle(a, b, &mut points);
        let bc = middle(b, c, &mut points);
        let ca = middle(c, a, &mut points);
        split.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
    }
    (points, split)
}

/// Un bulto de una planta: un elipsoide (o un octaedro) de radios `radii` en
/// `center`, irregular según `seed`.
struct Blob {
    center: Vec3,
    radii: Vec3,
    color: [f32; 3],
    seed: u32,
    octahedron: bool,
}

/// Los seis puntos y las ocho caras de un octaedro sobre la esfera unitaria.
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

/// La mesh de `blobs`: con sombreado plano, un poco más oscura abajo.
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

/// Arbusto `variant`: cuatro masas irregulares de alrededor de un metro de
/// alto, dentro de 0.6 m de su centro.
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

/// Cantero `variant`: un montículo de tierra, una mata de hojas y diez flores,
/// dentro de 0.7 m de su centro y de un tercio de metro de alto.
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

/// Las meshes de plantas compartidas y su único material, construidos al
/// iniciar junto con el [`crate::tree::TreeKit`].
#[derive(Debug, Clone)]
pub struct PlantKit {
    pub shrubs: Vec<Handle<Mesh>>,
    pub flowers: Vec<Handle<Mesh>>,
    /// Blanco con colores por vertex: la pintura de todas las plantas.
    pub material: Handle<StandardMaterial>,
}

impl PlantKit {
    /// Agrega las meshes y el material a `meshes` y `materials`.
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
            // Core le da a un arbusto 0.6 m a cada lado.
            assert!((0.35..=0.6).contains(&around), "shrub {v}: {around}");
            assert!(
                lo >= 0.0 && (0.6..=1.0).contains(&hi),
                "shrub {v}: {lo}..{hi}"
            );
            // Cuatro masas de 80 caras.
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
            // Core le da a un cantero 0.7 m a cada lado; bajo, por debajo de
            // 0.4 m, con la tierra hundida unos centímetros en el suelo.
            assert!((0.5..=0.7).contains(&around), "bed {v}: {around}");
            assert!(
                lo >= -0.1 && (0.2..=0.4).contains(&hi),
                "bed {v}: {lo}..{hi}"
            );
            // Tierra y mata (80 caras cada una) y diez flores de ocho.
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
                // El más brillante de los vertices superiores de cada flor: su
                // propio color.
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
