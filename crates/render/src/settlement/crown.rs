//! Procedural crowns for the settlements' planted trees.
//!
//! The vegetation models' crowns are a few big leaf masses made for the
//! broad caldén. Squeezed to a Lombardy poplar's narrow column, they turn
//! into thin slabs; stretched to a eucalyptus they lose its clumped look.
//! So the planted trees keep the model's trunk (or its stand-in) and wear
//! a crown built here instead: overlapping, lumpy low-poly leaf masses
//! (once-subdivided icospheres), flat-shaded like the models and painted
//! in the same leaf colours, so the poplar and eucalyptus paints tint them
//! like the models' leaves.
//!
//! - The poplar: a stack of masses tapering at the foot and the tip, a
//!   column about a fifth as wide as the tree is tall.
//! - The eucalyptus: a loose, irregular crown of clumps round the top half,
//!   about half as wide as tall.
//!
//! A crown is built for a 1 m tall tree (scale it by the tree's height),
//! from its foot on the ground. A few seeded variants per species are
//! built once and shared.

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

/// Seeded crown shapes built per species.
pub const CROWN_VARIANTS: u32 = 4;

/// Which planted tree a crown is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrownKind {
    Poplar,
    Eucalyptus,
}

/// The leaf colours (sRGB) of the vegetation models' crowns, light and
/// dark: the same paint the species materials tint.
const LEAVES: [[f32; 3]; 2] = [[0.34, 0.38, 0.21], [0.26, 0.30, 0.17]];

/// Deterministic `[0, 1)` for a seed and an index.
pub(crate) fn unit(seed: u32, i: u32) -> f32 {
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

/// A once-subdivided icosahedron on the unit sphere: 42 points, 80
/// counter-clockwise triangles.
pub(crate) fn icosphere() -> (Vec<Vec3>, Vec<[u32; 3]>) {
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

/// One leaf mass: an ellipsoid of `radii` at `center`, lumpy by `seed`.
struct Mass {
    center: Vec3,
    radii: Vec3,
    seed: u32,
    dark: bool,
}

/// The masses of crown `kind`, variant `variant`.
fn masses(kind: CrownKind, variant: u32) -> Vec<Mass> {
    let seed = variant.wrapping_mul(7919) ^ (kind as u32).wrapping_mul(104_729);
    let r = |i: u32| unit(seed, i);
    match kind {
        CrownKind::Poplar => {
            // A column from a fifth of the way up to the tip, widest a
            // little below its middle.
            let count = 7;
            let (foot, tip) = (0.2 + 0.04 * r(0), 0.97);
            let step = (tip - foot) / count as f32;
            (0..count)
                .map(|k| {
                    let t = (k as f32 + 0.5) / count as f32;
                    let girth = (PI * (0.12 + 0.88 * t)).sin().powf(0.6) * (1.0 - 0.35 * t);
                    let width = 0.1 * girth * (0.9 + 0.2 * r(10 + k));
                    let angle = r(20 + k) * TAU;
                    let off = width * 0.25 * r(30 + k);
                    Mass {
                        center: Vec3::new(
                            angle.cos() * off,
                            foot + step * (k as f32 + 0.5),
                            angle.sin() * off,
                        ),
                        radii: Vec3::new(width, step * 0.95, width * (0.9 + 0.2 * r(40 + k))),
                        seed: seed ^ k,
                        dark: k % 3 == 0,
                    }
                })
                .collect()
        }
        CrownKind::Eucalyptus => {
            // A ring of clumps round the top half and one over the middle.
            let count = 5;
            let turn = r(0) * TAU;
            let mut masses: Vec<Mass> = (0..count)
                .map(|k| {
                    let angle = turn + TAU * (k as f32 + 0.3 * r(10 + k)) / count as f32;
                    let out = 0.1 + 0.05 * r(20 + k);
                    let size = 0.1 + 0.04 * r(30 + k);
                    Mass {
                        center: Vec3::new(
                            angle.cos() * out,
                            0.62 + 0.2 * r(40 + k),
                            angle.sin() * out,
                        ),
                        radii: Vec3::new(size, size * 0.75, size),
                        seed: seed ^ k,
                        dark: k % 2 == 0,
                    }
                })
                .collect();
            masses.push(Mass {
                center: Vec3::new(0.0, 0.86, 0.0),
                radii: Vec3::new(0.12, 0.1, 0.12),
                seed: seed ^ 99,
                dark: false,
            });
            masses
        }
    }
}

/// The crown of `kind`, variant `variant`, for a 1 m tall tree: flat-shaded
/// with the models' leaf colours, a little darker low down.
pub fn crown(kind: CrownKind, variant: u32) -> Mesh {
    let (sphere, faces) = icosphere();
    let (mut positions, mut colors) = (Vec::new(), Vec::new());
    for mass in masses(kind, variant) {
        let points: Vec<Vec3> = sphere
            .iter()
            .enumerate()
            .map(|(i, p)| {
                // Lumpy: each point pushed in or out a little.
                let lump = 0.85 + 0.3 * unit(mass.seed, i as u32);
                mass.center + *p * mass.radii * lump
            })
            .collect();
        let leaf = LEAVES[mass.dark as usize];
        for face in &faces {
            for &i in face {
                let p = points[i as usize];
                let shade = 0.72 + 0.28 * p.y.clamp(0.0, 1.0);
                let c = LinearRgba::from(Color::srgb(leaf[0], leaf[1], leaf[2]));
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

    #[test]
    fn crowns_have_depth_their_shape_and_a_modest_budget() {
        for kind in [CrownKind::Poplar, CrownKind::Eucalyptus] {
            let mut shapes = Vec::new();
            for variant in 0..CROWN_VARIANTS {
                let mesh = crown(kind, variant);
                let tris = triangles(&mesh);
                assert!((300..=800).contains(&tris), "{kind:?}: {tris}");
                let p = points(&mesh);
                let (lo, hi) = p.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
                    (lo.min(*p), hi.max(*p))
                });
                let size = hi - lo;
                assert!(
                    size.x.min(size.z) >= 0.6 * size.x.max(size.z),
                    "{kind:?} {size:?}"
                );
                assert!(lo.y > 0.1 && hi.y <= 1.05, "{kind:?} {lo:?} {hi:?}");
                match kind {
                    // A column some five times as tall as wide.
                    CrownKind::Poplar => {
                        assert!(size.y >= 3.0 * size.x.max(size.z), "{size:?}");
                        assert!(size.x.min(size.z) >= 0.12, "{size:?}");
                    }
                    CrownKind::Eucalyptus => {
                        assert!(size.x.min(size.z) >= 0.35, "{size:?}");
                    }
                }
                // Faces point out of the crown, not into it.
                let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
                    Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) => n.clone(),
                    _ => panic!(),
                };
                let center = (lo + hi) / 2.0;
                let outward = p
                    .iter()
                    .zip(&normals)
                    .filter(|(p, n)| (**p - center).dot(Vec3::from_array(**n)) > 0.0)
                    .count();
                assert!(
                    outward * 10 > p.len() * 7,
                    "{kind:?}: {outward}/{}",
                    p.len()
                );
                shapes.push(p);
            }
            for i in 0..shapes.len() {
                for j in i + 1..shapes.len() {
                    assert_ne!(shapes[i], shapes[j], "{kind:?} variants {i} and {j}");
                }
            }
        }
    }
}
