//! The tracks and the trodden yards, laid over the ground like the roads
//! (see [`crate::road`]) but with no terrain cut.
//!
//! A track is a strip [`Track::HALF_WIDTH`] meters to either side of its
//! line, sampled every [`STEP`] meters so it drapes over the drawn ground.
//! Across it the vertex alpha draws two wheel ruts of dry mud, the grass
//! showing through between them and beyond feathered edges. Patios and
//! corral floors are square patches of dry ground, opaque inside and
//! feathered at their rim.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use pampa_core::{Aabb, Track, Vec2 as GroundVec2};

use super::materials::Surface;
use crate::coords::world_from_ground;
use crate::ground::face_tangent;

/// How far the decals float over the drawn ground, in meters.
pub(crate) const LIFT: f32 = 0.04;
/// Sample spacing along a track, in meters.
pub(crate) const STEP: f32 = 2.0;
/// Width of a yard's feathered rim, in meters.
const YARD_FEATHER: f32 = 2.0;

/// The track's cross-section: offset from its line (in units of
/// [`Track::HALF_WIDTH`]) and opacity. Two ruts of bare mud, a strip of
/// thin grass between, and feathered shoulders.
const ACROSS: [(f32, f32); 8] = [
    (-1.25, 0.0),
    (-0.88, 0.85),
    (-0.42, 0.85),
    (-0.22, 0.18),
    (0.22, 0.18),
    (0.42, 0.85),
    (0.88, 0.85),
    (1.25, 0.0),
];

/// Triangles collected for one decal mesh.
#[derive(Default)]
struct Decal {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
}

impl Decal {
    /// A quad of two triangles, corners counter-clockwise from above.
    fn quad(&mut self, corners: [(Vec3, f32); 4], tile: f32) {
        let [a, b, c, d] = corners;
        for tri in [[a, b, c], [a, c, d]] {
            let points = tri.map(|(p, _)| p);
            let uv = points.map(|p| [p.x / tile, p.z / tile]);
            self.positions.extend(points.map(|p| p.to_array()));
            self.colors
                .extend(tri.map(|(_, alpha)| [1.0, 1.0, 1.0, alpha]));
            self.tangents.extend([face_tangent(points, uv); 3]);
            self.uvs.extend(uv);
        }
    }

    fn into_mesh(self) -> Option<Mesh> {
        if self.positions.is_empty() {
            return None;
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, self.tangents);
        mesh.compute_normals();
        Some(mesh)
    }
}

/// `points` with extra samples so no piece is longer than [`STEP`].
fn sampled(points: &[GroundVec2]) -> Vec<GroundVec2> {
    let mut out = Vec::new();
    for w in points.windows(2) {
        let n = ((w[0].distance(w[1]) / STEP).ceil() as usize).max(1);
        for k in 0..n {
            out.push(w[0] + (w[1] - w[0]) * (k as f32 / n as f32));
        }
    }
    out.extend(points.last().copied());
    out
}

/// One mesh (world space) for every track in `tracks`, on the `ground`.
pub(crate) fn tracks_mesh(tracks: &[&Track], ground: &impl Fn(GroundVec2) -> f32) -> Option<Mesh> {
    let tile = Surface::Mud.tile();
    let mut decal = Decal::default();
    for track in tracks {
        let points = sampled(&track.points);
        if points.len() < 2 {
            continue;
        }
        // The across direction at each sample, mitred at the bends.
        let normal = |d: GroundVec2| {
            let len = d.length().max(1e-6);
            GroundVec2::new(-d.y / len, d.x / len)
        };
        let rows: Vec<Vec<(Vec3, f32)>> = (0..points.len())
            .map(|k| {
                let prev = points[k.saturating_sub(1)];
                let next = points[(k + 1).min(points.len() - 1)];
                let n = normal(next - prev);
                ACROSS
                    .iter()
                    .map(|&(offset, alpha)| {
                        let g = points[k] + n * (offset * Track::HALF_WIDTH);
                        (world_from_ground(g, ground(g) + LIFT), alpha)
                    })
                    .collect()
            })
            .collect();
        for w in rows.windows(2) {
            for j in 0..ACROSS.len() - 1 {
                // Counter-clockwise seen from above (north is -Z).
                decal.quad([w[0][j], w[1][j], w[1][j + 1], w[0][j + 1]], tile);
            }
        }
    }
    decal.into_mesh().map(|mut mesh| {
        orient_up(&mut mesh);
        mesh
    })
}

/// One mesh (world space) for the trodden `yards`: opaque inside, feathered
/// over [`YARD_FEATHER`] meters at their rim, on the `ground`.
pub(crate) fn yards_mesh(yards: &[Aabb], ground: &impl Fn(GroundVec2) -> f32) -> Option<Mesh> {
    let tile = Surface::Yard.tile();
    let mut decal = Decal::default();
    for yard in yards {
        let outer = yard.expanded(YARD_FEATHER / 2.0);
        let inner = yard.expanded(-YARD_FEATHER / 2.0);
        let xs = grid(outer.min.x, outer.max.x);
        let ys = grid(outer.min.y, outer.max.y);
        let alpha = |p: GroundVec2| {
            let gap = |lo: f32, hi: f32, v: f32| (lo - v).max(v - hi).max(0.0);
            let out = gap(inner.min.x, inner.max.x, p.x).max(gap(inner.min.y, inner.max.y, p.y));
            (1.0 - out / YARD_FEATHER).clamp(0.0, 1.0) * 0.9
        };
        let at = |x: f32, y: f32| {
            let g = GroundVec2::new(x, y);
            (world_from_ground(g, ground(g) + LIFT), alpha(g))
        };
        for j in 0..ys.len() - 1 {
            for i in 0..xs.len() - 1 {
                decal.quad(
                    [
                        at(xs[i], ys[j]),
                        at(xs[i + 1], ys[j]),
                        at(xs[i + 1], ys[j + 1]),
                        at(xs[i], ys[j + 1]),
                    ],
                    tile,
                );
            }
        }
    }
    decal.into_mesh().map(|mut mesh| {
        orient_up(&mut mesh);
        mesh
    })
}

/// Samples from `lo` to `hi` at most [`STEP`] apart, both ends included.
fn grid(lo: f32, hi: f32) -> Vec<f32> {
    let n = (((hi - lo) / STEP).ceil() as usize).max(1);
    (0..=n)
        .map(|k| lo + (hi - lo) * k as f32 / n as f32)
        .collect()
}

/// Turns every triangle of a ground decal to face up, whichever way its
/// corners were wound (a track's turn decides it), and recomputes its
/// normals.
fn orient_up(mesh: &mut Mesh) {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    else {
        return;
    };
    let mut flipped = Vec::new();
    for (t, tri) in points.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let [a, b, c] = [tri[0], tri[1], tri[2]].map(Vec3::from_array);
        if (b - a).cross(c - a).y < 0.0 {
            tri.swap(1, 2);
            flipped.push(t);
        }
    }
    for attribute in [
        Mesh::ATTRIBUTE_COLOR,
        Mesh::ATTRIBUTE_UV_0,
        Mesh::ATTRIBUTE_TANGENT,
    ] {
        match mesh.attribute_mut(attribute) {
            Some(bevy::mesh::VertexAttributeValues::Float32x4(v)) => {
                for &t in &flipped {
                    v.swap(3 * t + 1, 3 * t + 2);
                }
            }
            Some(bevy::mesh::VertexAttributeValues::Float32x2(v)) => {
                for &t in &flipped {
                    v.swap(3 * t + 1, 3 * t + 2);
                }
            }
            _ => {}
        }
    }
    mesh.compute_normals();
}
