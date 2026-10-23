//! A small mesh builder for the procedural settlement buildings and props.
//!
//! Every piece is added to the buffer of its [`Surface`], so a structure
//! ends up as one mesh per surface it wears. Faces carry flat normals,
//! UVs scaled to the world (one texture repeat every [`Surface::tile`]
//! meters, whatever the size of the piece) and tangents for the normal
//! maps.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::materials::Surface;
use crate::ground::face_tangent;

/// The vertices of one surface's mesh while it is built.
#[derive(Default, Debug, Clone)]
struct Buffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// One structure's meshes, one per surface, in the structure's own space.
#[derive(Default, Debug, Clone)]
pub(crate) struct Shape {
    parts: Vec<(Surface, Buffers)>,
}

/// The unit cube's faces, corners counter-clockwise from outside: +X, -X,
/// +Y, +Z, -Z, -Y.
const CUBE: [[[f32; 3]; 4]; 6] = [
    [
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [0.5, 0.5, 0.5],
        [0.5, -0.5, 0.5],
    ],
    [
        [-0.5, -0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, -0.5],
    ],
    [
        [-0.5, 0.5, -0.5],
        [-0.5, 0.5, 0.5],
        [0.5, 0.5, 0.5],
        [0.5, 0.5, -0.5],
    ],
    [
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ],
    [
        [0.5, -0.5, -0.5],
        [-0.5, -0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [0.5, 0.5, -0.5],
    ],
    [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, -0.5, 0.5],
        [-0.5, -0.5, 0.5],
    ],
];

/// The planar UV of `p` on a face with normal `n`: walls and slopes run
/// their `u` level and their `v` up the face; floors and roofs flat on the
/// ground take the ground plane's.
fn planar_uv(p: Vec3, n: Vec3, tile: f32) -> [f32; 2] {
    if n.y.abs() > 0.9 {
        return [p.x / tile, p.z / tile];
    }
    let u = Vec3::Y.cross(n).normalize_or(Vec3::X);
    let v = n.cross(u);
    [p.dot(u) / tile, -p.dot(v) / tile]
}

impl Shape {
    fn buffers(&mut self, surface: Surface) -> &mut Buffers {
        let k = match self.parts.iter().position(|(s, _)| *s == surface) {
            Some(k) => k,
            None => {
                self.parts.push((surface, Buffers::default()));
                self.parts.len() - 1
            }
        };
        &mut self.parts[k].1
    }

    /// A flat polygon (a triangle or a quad), corners counter-clockwise
    /// seen from outside.
    pub fn face(&mut self, surface: Surface, corners: &[Vec3]) {
        debug_assert!(corners.len() == 3 || corners.len() == 4);
        let n = (corners[1] - corners[0])
            .cross(corners[2] - corners[0])
            .normalize_or(Vec3::Y);
        let tile = surface.tile();
        let uv: Vec<[f32; 2]> = corners.iter().map(|&p| planar_uv(p, n, tile)).collect();
        let tangent = face_tangent([corners[0], corners[1], corners[2]], [uv[0], uv[1], uv[2]]);
        let b = self.buffers(surface);
        let base = b.positions.len() as u32;
        for (p, uv) in corners.iter().zip(uv) {
            b.positions.push(p.to_array());
            b.normals.push(n.to_array());
            b.uvs.push(uv);
            b.tangents.push(tangent);
        }
        b.indices.extend([base, base + 1, base + 2]);
        if corners.len() == 4 {
            b.indices.extend([base, base + 2, base + 3]);
        }
    }

    /// A box: the unit cube through `transform`. `bottom` keeps its
    /// underside (left out for things standing on the ground).
    pub fn cuboid(&mut self, surface: Surface, transform: Transform, bottom: bool) {
        let m = transform.to_matrix();
        let faces = if bottom { &CUBE[..] } else { &CUBE[..5] };
        for face in faces {
            let c = face.map(|p| m.transform_point3(Vec3::from_array(p)));
            self.face(surface, &c);
        }
    }

    /// An axis-aligned box from `min` to `max`, without its underside.
    pub fn block(&mut self, surface: Surface, min: Vec3, max: Vec3) {
        self.cuboid(
            surface,
            Transform::from_translation((min + max) / 2.0).with_scale(max - min),
            false,
        );
    }

    /// A square-section beam `thickness` thick from `from` to `to`, its
    /// ends closed.
    pub fn beam(&mut self, surface: Surface, from: Vec3, to: Vec3, thickness: f32) {
        let d = to - from;
        let len = d.length();
        if len <= 1e-4 {
            return;
        }
        self.cuboid(
            surface,
            Transform::from_translation((from + to) / 2.0)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, d / len))
                .with_scale(Vec3::new(thickness, len, thickness)),
            true,
        );
    }

    /// A surface of revolution round the local Y axis of `transform`:
    /// `profile` is (radius, height) from bottom to top, `segments` round.
    /// Smooth normals round, `u` along the circumference and `v` up the
    /// profile, both in meters over the surface's tile. A radius of 0 closes
    /// the shape there. Normals face out when the profile runs upward on the
    /// outside; pass it top to bottom for an inner wall.
    pub fn lathe(
        &mut self,
        surface: Surface,
        transform: Transform,
        profile: &[(f32, f32)],
        segments: u32,
    ) {
        let m = transform.to_matrix();
        let normal_m = m.inverse().transpose();
        let tile = surface.tile();
        let b = self.buffers(surface);
        let base = b.positions.len() as u32;
        let ring = segments + 1;
        // Arc length up the profile, for `v`.
        let mut along = vec![0.0f32; profile.len()];
        for k in 1..profile.len() {
            let (r0, y0) = profile[k - 1];
            let (r1, y1) = profile[k];
            along[k] = along[k - 1] + (r1 - r0).hypot(y1 - y0);
        }
        let mean_r = profile
            .iter()
            .map(|p| p.0.abs())
            .fold(0.0, f32::max)
            .max(0.05);
        for (k, &(r, y)) in profile.iter().enumerate() {
            // The profile's slope here, for the normal; a point repeated
            // in the profile makes a hard edge, each copy taking the slope
            // of its own side.
            let last = profile.len() - 1;
            let before = if k > 0 && profile[k - 1] != profile[k] {
                k - 1
            } else {
                k
            };
            let after = if k < last && profile[k + 1] != profile[k] {
                k + 1
            } else {
                k
            };
            let (a, c) = if before == after {
                (profile[k.saturating_sub(1)], profile[(k + 1).min(last)])
            } else {
                (profile[before], profile[after])
            };
            let (dr, dy) = (c.0 - a.0, c.1 - a.1);
            for s in 0..ring {
                let angle = s as f32 / segments as f32 * std::f32::consts::TAU;
                let (sin, cos) = angle.sin_cos();
                let local = Vec3::new(r * cos, y, r * sin);
                let n_local = Vec3::new(dy * cos, -dr, dy * sin).normalize_or(Vec3::Y);
                let t_local = Vec3::new(-sin, 0.0, cos);
                let p = m.transform_point3(local);
                let n = normal_m.transform_vector3(n_local).normalize_or(Vec3::Y);
                let t = m.transform_vector3(t_local).normalize_or(Vec3::X);
                // `v` grows down the profile, so the bitangent points down
                // it: its sign keeps the normal map's +Y up.
                let up = m.transform_vector3(Vec3::new(dr * cos, dy, dr * sin));
                let w = if n.cross(t).dot(-up) < 0.0 { -1.0 } else { 1.0 };
                b.positions.push(p.to_array());
                b.normals.push(n.to_array());
                b.uvs.push([angle * mean_r / tile, -along[k] / tile]);
                b.tangents.push([t.x, t.y, t.z, w]);
            }
        }
        for k in 0..profile.len() as u32 - 1 {
            for s in 0..segments {
                let (a, c) = (base + k * ring + s, base + (k + 1) * ring + s);
                b.indices.extend([a, c + 1, a + 1, a, c, c + 1]);
            }
        }
    }

    /// A flat disc of `radius` facing up, round the origin of `transform`.
    pub fn disc(&mut self, surface: Surface, transform: Transform, radius: f32, segments: u32) {
        let m = transform.to_matrix();
        let center = m.transform_point3(Vec3::ZERO);
        for s in 0..segments {
            let at = |k: u32| {
                let a = k as f32 / segments as f32 * std::f32::consts::TAU;
                m.transform_point3(Vec3::new(radius * a.cos(), 0.0, radius * a.sin()))
            };
            self.face(surface, &[center, at(s + 1), at(s)]);
        }
    }

    /// Triangles in all its meshes.
    pub fn triangles(&self) -> usize {
        self.parts.iter().map(|(_, b)| b.indices.len() / 3).sum()
    }

    /// The surfaces it wears.
    #[cfg(test)]
    pub fn surfaces(&self) -> impl Iterator<Item = Surface> + '_ {
        self.parts.iter().map(|(s, _)| *s)
    }

    /// The points of the mesh of `surface`, for tests and bounds.
    #[cfg(test)]
    pub fn points(&self, surface: Surface) -> Vec<Vec3> {
        self.parts
            .iter()
            .filter(|(s, _)| *s == surface)
            .flat_map(|(_, b)| b.positions.iter().map(|p| Vec3::from_array(*p)))
            .collect()
    }

    /// The triangles of the mesh of `surface`, for tests.
    #[cfg(test)]
    pub fn triangles_of(&self, surface: Surface) -> Vec<[Vec3; 3]> {
        self.parts
            .iter()
            .filter(|(s, _)| *s == surface)
            .flat_map(|(_, b)| {
                b.indices
                    .chunks(3)
                    .map(|t| [0, 1, 2].map(|k| Vec3::from_array(b.positions[t[k] as usize])))
            })
            .collect()
    }

    /// The vertices of the mesh of `surface` as (position, normal, uv), for
    /// tests.
    #[cfg(test)]
    pub fn vertices(&self, surface: Surface) -> Vec<(Vec3, Vec3, Vec2)> {
        self.parts
            .iter()
            .filter(|(s, _)| *s == surface)
            .flat_map(|(_, b)| {
                (0..b.positions.len()).map(|i| {
                    (
                        Vec3::from_array(b.positions[i]),
                        Vec3::from_array(b.normals[i]),
                        Vec2::from_array(b.uvs[i]),
                    )
                })
            })
            .collect()
    }

    /// One mesh per surface.
    pub fn into_meshes(self) -> Vec<(Surface, Mesh)> {
        self.parts
            .into_iter()
            .filter(|(_, b)| !b.indices.is_empty())
            .map(|(surface, b)| {
                let mesh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::RENDER_WORLD,
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, b.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, b.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, b.uvs)
                .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, b.tangents)
                .with_inserted_indices(Indices::U32(b.indices));
                (surface, mesh)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uvs_repeat_with_the_world_not_the_piece() {
        let mut shape = Shape::default();
        // A 10 m wall and a 1 m one: the long wall repeats ten times more.
        shape.block(Surface::Whitewash, Vec3::ZERO, Vec3::new(10.0, 3.0, 0.5));
        let tile = Surface::Whitewash.tile();
        let mut meshes = shape.into_meshes();
        let (_, mesh) = meshes.remove(0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!()
        };
        // The front (+Z) face: the cube's fourth, four corners.
        let front = &uvs[12..16];
        let span = front.iter().map(|u| u[0]).fold(f32::MIN, f32::max)
            - front.iter().map(|u| u[0]).fold(f32::MAX, f32::min);
        assert!((span - 10.0 / tile).abs() < 1e-3, "{span}");
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    #[test]
    fn a_lathe_closes_round_and_faces_out() {
        let mut shape = Shape::default();
        shape.lathe(
            Surface::Galvanised,
            Transform::IDENTITY,
            &[(2.0, 0.0), (2.0, 1.5)],
            24,
        );
        assert_eq!(shape.triangles(), 48);
        let mesh = shape.into_meshes().remove(0).1;
        let (
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)),
            Some(bevy::mesh::VertexAttributeValues::Float32x3(n)),
        ) = (
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL),
        )
        else {
            panic!()
        };
        for (p, n) in p.iter().zip(n) {
            let out = Vec3::new(p[0], 0.0, p[2]).normalize();
            assert!(Vec3::from_array(*n).dot(out) > 0.99);
        }
    }
}
