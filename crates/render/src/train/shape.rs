//! A small mesh builder for the rolling stock: lathe turnings (boilers,
//! chimneys, wheels' hubs, lamps), prisms of a drawn profile (roofs, rods,
//! seat cushions) and boxes, each with smooth or creased normals and UVs in
//! meters, so a texture tiles at its own scale however a part is sized.
//!
//! A [`Shape`] is plain data in a car's frame (+X forward, +Y up, +Z right,
//! origin on the rail top); the car builder merges the shapes of one
//! material into one mesh.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Two neighbouring edges whose normals are at most this far apart share
/// smooth normals at their common point; a sharper corner is creased.
const CREASE_COS: f32 = 0.6;

/// Which way a flat part's boards run in its texture: along the first
/// texture axis of the face (horizontal boards on a side wall) or across it
/// (vertical boards).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    Along,
    Across,
}

/// Vertices with normals and UVs, and the triangles over them.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Shape {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Adds `other`'s vertices and triangles.
    pub fn append(&mut self, other: &Shape) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    /// The shape moved by `t`: positions scaled, turned and shifted, normals
    /// turned (and corrected for a non-uniform scale).
    pub fn transformed(&self, t: Transform) -> Shape {
        let inverse_transpose = Mat3::from_diagonal(t.scale).inverse().transpose();
        let mut out = self.clone();
        for p in &mut out.positions {
            *p = t.transform_point(Vec3::from(*p)).to_array();
        }
        for n in &mut out.normals {
            *n = (t.rotation * (inverse_transpose * Vec3::from(*n)))
                .normalize_or_zero()
                .to_array();
        }
        out
    }

    pub fn translated(&self, by: Vec3) -> Shape {
        self.transformed(Transform::from_translation(by))
    }

    /// The vertex count so far, as an index base.
    fn base(&self) -> u32 {
        self.positions.len() as u32
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, uv: Vec2) {
        self.positions.push(p.to_array());
        self.normals.push(n.to_array());
        self.uvs.push(uv.to_array());
    }

    /// A triangle over three vertices already pushed, wound so that it faces
    /// the way its vertices' normals do.
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let at = |i: u32| Vec3::from(self.positions[i as usize]);
        let facing = (at(b) - at(a)).cross(at(c) - at(a));
        let normal = [a, b, c]
            .into_iter()
            .map(|i| Vec3::from(self.normals[i as usize]))
            .sum::<Vec3>();
        if facing.length_squared() < 1e-14 {
            return;
        }
        if facing.dot(normal) >= 0.0 {
            self.indices.extend([a, b, c]);
        } else {
            self.indices.extend([a, c, b]);
        }
    }

    /// A flat quad with its UVs in meters, projected along the dominant axis
    /// of its normal: `(z, y)` for a face turned along X, `(x, z)` for one
    /// turned up or down, `(x, y)` for one turned along Z. [`Grain::Across`]
    /// swaps the two texture axes.
    pub fn flat(&mut self, corners: [Vec3; 4], grain: Grain) {
        let normal = (corners[1] - corners[0])
            .cross(corners[3] - corners[0])
            .normalize_or_zero();
        let base = self.base();
        for c in corners {
            let (u, v) = project(c, normal);
            let uv = match grain {
                Grain::Along => Vec2::new(u, v),
                Grain::Across => Vec2::new(v, u),
            };
            self.vertex(c, normal, uv);
        }
        self.triangle(base, base + 1, base + 2);
        self.triangle(base, base + 2, base + 3);
    }

    /// A flat convex polygon `ring`, fanned from its first corner, facing
    /// `normal`.
    pub fn flat_polygon(&mut self, ring: &[Vec3], normal: Vec3, grain: Grain) {
        let base = self.base();
        for &c in ring {
            let (u, v) = project(c, normal);
            let uv = match grain {
                Grain::Along => Vec2::new(u, v),
                Grain::Across => Vec2::new(v, u),
            };
            self.vertex(c, normal, uv);
        }
        for i in 1..ring.len() as u32 - 1 {
            self.triangle(base, base + i, base + i + 1);
        }
    }

    /// A box.
    pub fn cuboid(center: Vec3, size: Vec3, grain: Grain) -> Shape {
        let mut shape = Shape::default();
        let h = size / 2.0;
        let corner = |sx: f32, sy: f32, sz: f32| center + Vec3::new(sx * h.x, sy * h.y, sz * h.z);
        let faces = [
            // +X, -X, +Y, -Y, +Z, -Z, each counter-clockwise from outside.
            [(1., -1., 1.), (1., -1., -1.), (1., 1., -1.), (1., 1., 1.)],
            [
                (-1., -1., -1.),
                (-1., -1., 1.),
                (-1., 1., 1.),
                (-1., 1., -1.),
            ],
            [(-1., 1., 1.), (1., 1., 1.), (1., 1., -1.), (-1., 1., -1.)],
            [
                (-1., -1., -1.),
                (1., -1., -1.),
                (1., -1., 1.),
                (-1., -1., 1.),
            ],
            [(-1., -1., 1.), (1., -1., 1.), (1., 1., 1.), (-1., 1., 1.)],
            [
                (1., -1., -1.),
                (-1., -1., -1.),
                (-1., 1., -1.),
                (1., 1., -1.),
            ],
        ];
        for face in faces {
            shape.flat(face.map(|(x, y, z)| corner(x, y, z)), grain);
        }
        shape
    }

    /// A surface of revolution about the X axis: `profile` is `(x, radius)`
    /// points, `sides` the facets round. Smooth where the profile bends
    /// gently, creased where it turns sharply (repeat a point to start a
    /// new run); a radius of zero closes the end. UVs: `u` is the way
    /// round in meters, `v` the profile's length.
    pub fn lathe(profile: &[(f32, f32)], sides: usize) -> Shape {
        let points: Vec<Vec2> = profile.iter().map(|&(x, r)| Vec2::new(x, r)).collect();
        let edges = creased(&points, false);
        let mut shape = Shape::default();
        let mut arc = 0.0;
        for (k, edge) in edges.iter().enumerate() {
            let (a, b) = (points[k], points[k + 1]);
            let len = a.distance(b);
            let mean = (a.y + b.y) / 2.0;
            let base = shape.base();
            for i in 0..=sides {
                let angle = TAU * i as f32 / sides as f32;
                let (sin, cos) = angle.sin_cos();
                let u = angle * mean;
                for (point, normal, v) in [(a, edge[0], arc), (b, edge[1], arc + len)] {
                    shape.vertex(
                        Vec3::new(point.x, point.y * cos, point.y * sin),
                        Vec3::new(normal.x, normal.y * cos, normal.y * sin),
                        Vec2::new(u, v),
                    );
                }
            }
            for i in 0..sides as u32 {
                let q = base + 2 * i;
                shape.triangle(q, q + 1, q + 3);
                shape.triangle(q, q + 3, q + 2);
            }
            arc += len;
        }
        shape
    }

    /// The closed polygon `ring` (counter-clockwise, in a plane's `(a, b)`
    /// coordinates) pulled `depth` along the plane's normal from `-depth / 2`
    /// to `depth / 2`, mapped into the frame by `place`, which sends
    /// `(a, b, along)` to a point. `smooth` rounds a corner's normals
    /// between neighbouring edges where they bend gently. UVs: `u` along
    /// the pull, `v` along the ring, in meters; the end caps (when `caps`,
    /// fanned from the first corner: the ring must be convex) are flat.
    pub fn prism(
        ring: &[Vec2],
        depth: f32,
        smooth: bool,
        caps: bool,
        place: impl Fn(f32, f32, f32) -> Vec3,
    ) -> Shape {
        let mut shape = Shape::default();
        let mut closed = ring.to_vec();
        closed.push(ring[0]);
        let edges = if smooth {
            creased(&closed, true)
        } else {
            flat_edges(&closed)
        };
        let origin = place(0.0, 0.0, 0.0);
        // Directions map linearly; take the normal out of `place` too.
        let turn = |v: Vec2| place(v.x, v.y, 0.0) - origin;
        let mut arc = 0.0;
        for (k, edge) in edges.iter().enumerate() {
            let (a, b) = (closed[k], closed[k + 1]);
            let len = a.distance(b);
            let base = shape.base();
            for (point, normal, v) in [(a, edge[0], arc), (b, edge[1], arc + len)] {
                for (end, along) in [(0.0, -depth / 2.0), (1.0, depth / 2.0)] {
                    shape.vertex(
                        place(point.x, point.y, along),
                        turn(normal).normalize_or_zero(),
                        Vec2::new(end * depth, v),
                    );
                }
            }
            shape.triangle(base, base + 1, base + 3);
            shape.triangle(base, base + 3, base + 2);
            arc += len;
        }
        if caps {
            let out = place(0.0, 0.0, 1.0) - origin;
            for (sign, along) in [(-1.0, -depth / 2.0), (1.0, depth / 2.0)] {
                let corners: Vec<Vec3> = ring.iter().map(|p| place(p.x, p.y, along)).collect();
                shape.flat_polygon(&corners, out * sign, Grain::Along);
            }
        }
        shape
    }

    /// `prism` along X: `ring` in `(z, y)`, from `x0` to `x1`.
    pub fn along_x(ring: &[Vec2], x0: f32, x1: f32, smooth: bool, caps: bool) -> Shape {
        let mid = (x0 + x1) / 2.0;
        Shape::prism(ring, x1 - x0, smooth, caps, move |z, y, x| {
            Vec3::new(mid + x, y, z)
        })
    }

    /// `prism` along Z: `ring` in `(x, y)`, from `z0` to `z1`.
    pub fn along_z(ring: &[Vec2], z0: f32, z1: f32, smooth: bool, caps: bool) -> Shape {
        let mid = (z0 + z1) / 2.0;
        Shape::prism(ring, z1 - z0, smooth, caps, move |x, y, z| {
            Vec3::new(x, y, mid + z)
        })
    }

    /// `prism` along Y: `ring` in `(x, z)`, from `y0` to `y1`.
    pub fn along_y(ring: &[Vec2], y0: f32, y1: f32, smooth: bool, caps: bool) -> Shape {
        let mid = (y0 + y1) / 2.0;
        Shape::prism(ring, y1 - y0, smooth, caps, move |x, z, y| {
            Vec3::new(x, mid + y, z)
        })
    }

    /// The mesh to draw.
    pub fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }

    /// The box round every vertex: `(min, max)`.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.positions.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))),
        )
    }
}

/// A box's `(u, v)` in meters on a face with this normal.
fn project(p: Vec3, normal: Vec3) -> (f32, f32) {
    let a = normal.abs();
    if a.x >= a.y && a.x >= a.z {
        (p.z, p.y)
    } else if a.y >= a.z {
        (p.x, p.z)
    } else {
        (p.x, p.y)
    }
}

/// The outward unit normal of every edge of `points`, and, per edge, its
/// two end normals: the edge's own, or the average with the neighbour's
/// where they bend by less than the crease angle. `wrap` joins the last
/// edge to the first (a closed ring).
fn creased(points: &[Vec2], wrap: bool) -> Vec<[Vec2; 2]> {
    let own = edge_normals(points, wrap);
    let n = own.len();
    let blend = |a: Vec2, b: Vec2| {
        if a.dot(b) >= CREASE_COS {
            (a + b).normalize_or_zero()
        } else {
            a
        }
    };
    (0..n)
        .map(|k| {
            let before = if k > 0 {
                Some(own[k - 1])
            } else if wrap {
                Some(own[n - 1])
            } else {
                None
            };
            let after = if k + 1 < n {
                Some(own[k + 1])
            } else if wrap {
                Some(own[0])
            } else {
                None
            };
            [
                before.map_or(own[k], |b| blend(own[k], b)),
                after.map_or(own[k], |a| blend(own[k], a)),
            ]
        })
        .collect()
}

fn flat_edges(points: &[Vec2]) -> Vec<[Vec2; 2]> {
    edge_normals(points, true)
        .into_iter()
        .map(|n| [n, n])
        .collect()
}

/// The unit normal of each edge of a polyline. A closed `ring` has its
/// normals outward whichever way it runs; an open profile (a lathe's
/// `(x, radius)` points, run from the low end to the high along the
/// outside) has them to the left of its direction. A zero-length edge keeps
/// its predecessor's.
fn edge_normals(points: &[Vec2], ring: bool) -> Vec<Vec2> {
    let counter_clockwise = !ring
        || points
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f32>()
            >= 0.0;
    let mut last = Vec2::Y;
    points
        .windows(2)
        .map(|w| {
            let d = (w[1] - w[0]).normalize_or_zero();
            if d != Vec2::ZERO {
                // Right of the direction is outward for a ring going
                // counter-clockwise; the profile wants the left.
                let right = Vec2::new(d.y, -d.x);
                last = match (ring, counter_clockwise) {
                    (true, true) | (false, false) => right,
                    _ => -right,
                };
            }
            last
        })
        .collect()
}

/// `n + 1` points of the arc of radius `r` about `center` from angle `a0`
/// to `a1`, in radians.
pub fn arc(center: Vec2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Vec2> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            center + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// A rounded rectangle `w` by `h` about the origin, counter-clockwise,
/// corners of radius `r` in `n` steps.
pub fn rounded_rect(w: f32, h: f32, r: f32, n: usize) -> Vec<Vec2> {
    let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
    let mut ring = Vec::new();
    for (corner, a0) in [
        (Vec2::new(hx, hy), 0.0),
        (Vec2::new(-hx, hy), PI / 2.0),
        (Vec2::new(-hx, -hy), PI),
        (Vec2::new(hx, -hy), 3.0 * PI / 2.0),
    ] {
        ring.extend(arc(corner, r, a0, a0 + PI / 2.0, n));
    }
    ring
}

/// A bar `length` long and `width` wide about the origin along X, its ends
/// round.
pub fn stadium(length: f32, width: f32, n: usize) -> Vec<Vec2> {
    let r = width / 2.0;
    let half = length / 2.0 - r;
    let mut ring = arc(Vec2::new(half, 0.0), r, -PI / 2.0, PI / 2.0, n);
    ring.extend(arc(Vec2::new(-half, 0.0), r, PI / 2.0, 3.0 * PI / 2.0, n));
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle faces the way its vertices' normals do, none is
    /// degenerate, every index is in range.
    fn assert_sound(shape: &Shape) {
        assert!(shape.triangles() > 0);
        assert_eq!(shape.positions.len(), shape.normals.len());
        assert_eq!(shape.positions.len(), shape.uvs.len());
        for t in shape.indices.chunks(3) {
            let at = |i: u32| Vec3::from(shape.positions[i as usize]);
            let facing = (at(t[1]) - at(t[0])).cross(at(t[2]) - at(t[0]));
            assert!(facing.length() > 1e-7, "degenerate");
            let normal: Vec3 = t
                .iter()
                .map(|&i| Vec3::from(shape.normals[i as usize]))
                .sum();
            assert!(facing.dot(normal) > 0.0, "winding against its normal");
        }
        for n in &shape.normals {
            assert!((Vec3::from(*n).length() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_box_is_twelve_triangles_facing_out_with_uvs_in_meters() {
        let shape = Shape::cuboid(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(2.0, 1.0, 0.5),
            Grain::Along,
        );
        assert_sound(&shape);
        assert_eq!(shape.triangles(), 12);
        // Every normal points away from the middle.
        for (p, n) in shape.positions.iter().zip(&shape.normals) {
            let out = Vec3::from(*p) - Vec3::new(1.0, 2.0, 3.0);
            assert!(out.dot(Vec3::from(*n)) > 0.0);
        }
        // The +Z face spans 2 m in x and 1 m in y: so does its UV.
        let front: Vec<_> = shape
            .positions
            .iter()
            .zip(&shape.uvs)
            .zip(&shape.normals)
            .filter(|(_, n)| n[2] > 0.9)
            .map(|((_, uv), _)| *uv)
            .collect();
        let span = |k: usize| {
            front.iter().map(|uv| uv[k]).fold(f32::MIN, f32::max)
                - front.iter().map(|uv| uv[k]).fold(f32::MAX, f32::min)
        };
        assert!((span(0) - 2.0).abs() < 1e-5 && (span(1) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn across_grain_swaps_the_texture_axes() {
        let along = Shape::cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5), Grain::Along);
        let across = Shape::cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5), Grain::Across);
        for (a, b) in along.uvs.iter().zip(&across.uvs) {
            assert_eq!([a[1], a[0]], *b);
        }
    }

    #[test]
    fn a_lathe_is_round_smooth_and_closed_at_a_zero_radius() {
        // A cylinder 2 m long and 1 m across, capped by two discs.
        let shape = Shape::lathe(&[(-1.0, 0.0), (-1.0, 0.5), (1.0, 0.5), (1.0, 0.0)], 16);
        assert_sound(&shape);
        let (lo, hi) = shape.bounds();
        assert!((lo - Vec3::new(-1.0, -0.5, -0.5)).abs().max_element() < 1e-5);
        assert!((hi - Vec3::new(1.0, 0.5, 0.5)).abs().max_element() < 1e-5);
        // The barrel's normals are radial, the caps' axial.
        for (p, n) in shape.positions.iter().zip(&shape.normals) {
            let (p, n) = (Vec3::from(*p), Vec3::from(*n));
            if n.x.abs() < 0.01 {
                let radial = Vec3::new(0.0, p.y, p.z).normalize();
                assert!(radial.dot(n) > 0.999);
            } else {
                assert!(n.x.abs() > 0.999);
            }
        }
        // Round the barrel, `u` runs in meters: the circumference.
        let barrel: Vec<f32> = shape
            .uvs
            .iter()
            .zip(&shape.normals)
            .filter(|(_, n)| n[0].abs() < 0.01)
            .map(|(uv, _)| uv[0])
            .collect();
        let span = barrel.iter().copied().fold(f32::MIN, f32::max);
        assert!((span - PI).abs() < 1e-4, "{span}");
    }

    #[test]
    fn a_gentle_bend_is_smooth_and_a_sharp_one_is_creased() {
        // Profile with a gentle bend then a right angle.
        let shape = Shape::lathe(&[(0.0, 1.0), (1.0, 1.4), (2.0, 1.6), (2.0, 0.0)], 8);
        let normals_at = |x: f32| -> Vec<Vec3> {
            shape
                .positions
                .iter()
                .zip(&shape.normals)
                .filter(|(p, _)| (p[0] - x).abs() < 1e-5 && p[2].abs() < 1e-5 && p[1] > 0.5)
                .map(|(_, n)| Vec3::from(*n))
                .collect()
        };
        // At x = 1 both edges share one blended normal.
        let mid = normals_at(1.0);
        assert!(mid.len() >= 2 && mid.windows(2).all(|w| w[0].dot(w[1]) > 0.9999));
        // At x = 2 the barrel and the disc keep their own: perpendicular.
        let corner = normals_at(2.0);
        assert!(corner.iter().any(|n| n.x > 0.99) && corner.iter().any(|n| n.x < 0.3));
    }

    #[test]
    fn a_prism_along_x_has_smooth_sides_and_flat_caps_in_meters() {
        let ring = rounded_rect(1.0, 0.6, 0.2, 4);
        let shape = Shape::along_x(&ring, 0.0, 3.0, true, true);
        assert_sound(&shape);
        let (lo, hi) = shape.bounds();
        assert!(
            (lo - Vec3::new(0.0, -0.3, -0.5)).abs().max_element() < 1e-5,
            "{lo}"
        );
        assert!((hi - Vec3::new(3.0, 0.3, 0.5)).abs().max_element() < 1e-5);
        // `u` runs the 3 m of its length.
        let u_max = shape.uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
        assert!((u_max - 3.0).abs() < 1e-5);
        // The side normals stay perpendicular to its length.
        for (n, uv) in shape.normals.iter().zip(&shape.uvs) {
            if n[0].abs() > 0.5 {
                assert!(n[0].abs() > 0.999 && uv[0].abs() < 3.01);
            }
        }
    }

    #[test]
    fn a_stadium_bar_is_as_long_and_wide_as_asked() {
        let ring = stadium(3.0, 0.2, 4);
        let xs = ring.iter().map(|p| p.x);
        let ys = ring.iter().map(|p| p.y);
        assert!((xs.clone().fold(f32::MIN, f32::max) - 1.5).abs() < 1e-5);
        assert!((xs.fold(f32::MAX, f32::min) + 1.5).abs() < 1e-5);
        assert!((ys.fold(f32::MIN, f32::max) - 0.1).abs() < 1e-5);
    }
}
