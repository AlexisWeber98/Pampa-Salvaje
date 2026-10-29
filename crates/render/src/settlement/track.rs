//! Las huellas y los patios pisoteados, tendidos sobre el suelo como los
//! caminos (ver [`crate::road`]) pero sin corte de terreno.
//!
//! Una huella es una franja de [`Track::HALF_WIDTH`] metros a cada lado de su
//! línea, muestreada cada [`STEP`] metros para que se apoye sobre el suelo
//! dibujado. A lo ancho, el alfa por vertex dibuja dos surcos de rueda de barro
//! seco, con el pasto asomando entre ellos y más allá de los bordes
//! difuminados. Los patios y los pisos de corral son parches cuadrados de
//! tierra seca, opacos por dentro y difuminados en el borde.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use pampa_core::{Aabb, Track, Vec2 as GroundVec2};

use super::materials::Surface;
use crate::coords::world_from_ground;
use crate::ground::face_tangent;

/// Cuánto flotan los decals sobre el suelo dibujado, en metros.
pub(crate) const LIFT: f32 = 0.04;
/// Separación entre muestras a lo largo de una huella, en metros.
pub(crate) const STEP: f32 = 2.0;
/// Ancho del borde difuminado de un patio, en metros.
const YARD_FEATHER: f32 = 2.0;

/// El corte transversal de la huella: desplazamiento desde su línea (en
/// unidades de [`Track::HALF_WIDTH`]) y opacidad. Dos surcos de barro pelado,
/// una franja de pasto ralo entre ellos y banquinas difuminadas.
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

/// Triángulos reunidos para una mesh de decal.
#[derive(Default)]
struct Decal {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
}

impl Decal {
    /// Un quad de dos triángulos, con las esquinas en sentido antihorario
    /// vistas desde arriba.
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

/// `points` con muestras extra para que ningún tramo sea más largo que
/// [`STEP`].
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

/// Una mesh (en espacio de mundo) para todas las huellas de `tracks`, sobre el
/// `ground`.
pub(crate) fn tracks_mesh(tracks: &[&Track], ground: &impl Fn(GroundVec2) -> f32) -> Option<Mesh> {
    let tile = Surface::Mud.tile();
    let mut decal = Decal::default();
    for track in tracks {
        let points = sampled(&track.points);
        if points.len() < 2 {
            continue;
        }
        // La dirección transversal en cada muestra, en inglete en las curvas.
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
                // En sentido antihorario visto desde arriba (el norte es -Z).
                decal.quad([w[0][j], w[1][j], w[1][j + 1], w[0][j + 1]], tile);
            }
        }
    }
    decal.into_mesh().map(|mut mesh| {
        orient_up(&mut mesh);
        mesh
    })
}

/// Una mesh (en espacio de mundo) para los patios pisoteados `yards`: opacos
/// por dentro, difuminados a lo largo de [`YARD_FEATHER`] metros en su borde,
/// sobre el `ground`.
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

/// Ancho del borde difuminado de un sendero de tierra, en metros.
const WALK_FEATHER: f32 = 2.0;
/// Separación entre muestras sobre un sendero de tierra, en metros.
const WALK_STEP: f32 = 1.0;
/// La máxima opacidad que alcanza un sendero de tierra a lo largo de su centro.
pub(crate) const WALK_OPACITY: f32 = 0.9;

/// Una mesh (en espacio de mundo) para los senderos de tierra apisonada de un
/// pueblo: cada grupo de franjas (el anillo de una manzana) es la tierra
/// pisoteada de [`Surface::Yard`], opaca a lo largo del centro de cada franja y
/// difuminada a lo largo de [`WALK_FEATHER`] metros en sus bordes hacia el
/// suelo de al lado, unos centímetros por encima del `ground`. Dentro de un
/// grupo el alfa es el mayor de los de las franjas, así que una esquina no
/// muestra costura.
pub(crate) fn walks_mesh(
    groups: &[Vec<Aabb>],
    ground: &impl Fn(GroundVec2) -> f32,
) -> Option<Mesh> {
    let tile = Surface::Yard.tile();
    let mut decal = Decal::default();
    for walks in groups.iter().filter(|g| !g.is_empty()) {
        let (mut lo, mut hi) = (walks[0].min, walks[0].max);
        for walk in walks {
            lo = GroundVec2::new(lo.x.min(walk.min.x), lo.y.min(walk.min.y));
            hi = GroundVec2::new(hi.x.max(walk.max.x), hi.y.max(walk.max.y));
        }
        let pad = GroundVec2::new(1.0, 1.0);
        let (lo, hi) = (lo - pad, hi + pad);
        let alpha = |p: GroundVec2| {
            walks
                .iter()
                .map(|walk| {
                    let inner = walk.expanded(-WALK_FEATHER / 2.0);
                    let gap = |lo: f32, hi: f32, v: f32| (lo - v).max(v - hi).max(0.0);
                    let out =
                        gap(inner.min.x, inner.max.x, p.x).max(gap(inner.min.y, inner.max.y, p.y));
                    (1.0 - out / WALK_FEATHER).clamp(0.0, 1.0) * WALK_OPACITY
                })
                .fold(0.0, f32::max)
        };
        let samples = |lo: f32, hi: f32| {
            let n = (((hi - lo) / WALK_STEP).ceil() as usize).max(1);
            (0..=n)
                .map(|k| lo + (hi - lo) * k as f32 / n as f32)
                .collect::<Vec<f32>>()
        };
        let (xs, ys) = (samples(lo.x, hi.x), samples(lo.y, hi.y));
        let at = |x: f32, y: f32| {
            let g = GroundVec2::new(x, y);
            (world_from_ground(g, ground(g) + LIFT), alpha(g))
        };
        for j in 0..ys.len() - 1 {
            for i in 0..xs.len() - 1 {
                let corners = [
                    at(xs[i], ys[j]),
                    at(xs[i + 1], ys[j]),
                    at(xs[i + 1], ys[j + 1]),
                    at(xs[i], ys[j + 1]),
                ];
                if corners.iter().all(|c| c.1 <= 0.0) {
                    continue;
                }
                decal.quad(corners, tile);
            }
        }
    }
    decal.into_mesh().map(|mut mesh| {
        orient_up(&mut mesh);
        mesh
    })
}

/// Muestras desde `lo` hasta `hi` separadas como mucho [`STEP`], con ambos
/// extremos incluidos.
fn grid(lo: f32, hi: f32) -> Vec<f32> {
    let n = (((hi - lo) / STEP).ceil() as usize).max(1);
    (0..=n)
        .map(|k| lo + (hi - lo) * k as f32 / n as f32)
        .collect()
}

/// Hace que cada triángulo de un decal del suelo mire hacia arriba, sin
/// importar en qué sentido estuvieran ordenadas sus esquinas (lo decide el giro
/// de la huella), y vuelve a calcular sus normales.
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
