//! The 1919 recado and rawhide bridle on the model horse (see
//! [`horse`](super::horse)).
//!
//! Every layer fits the horse it is put on: it is wrapped round the horse's
//! own surface at rest, then skinned to the horse's bones with the weights
//! of the skin under it, so it bends with the barrel through every clip and
//! never cuts into it.
//!
//! - **Shells** ([`Shell`]): the sudadera, the carona, the two bastos, the
//!   sheepskin pellón and the cincha round the barrel, and the bridle's
//!   crownpiece, noseband and cheekpieces round the head. Each is a grid of
//!   rays cast out from an axis inside the body ([`Axis`]); where a ray
//!   first leaves the body the layer's inner face lies a little outside it
//!   ([`Offset`]), its outer face a thickness further. The vertices take the
//!   blended weights of the triangle each ray hit ([`RestHorse::cast`]).
//! - **Rings**: the bit, a small iron ring at each corner of the mouth,
//!   skinned like the shells.
//! - **Straps** ([`Strap`]): the reins (from the bit back past the jowl
//!   and along the neck to the rider's hands, or to the pommel on a horse
//!   without one) and the
//!   stirrup leathers (from the saddle to the stirrups) are straight pieces
//!   between two points on different bones, stretched after the transforms
//!   are worked out each frame ([`stretch_straps`]).
//! - **Stirrups**: on a horse without a rider they hang from the saddle; a
//!   seated rider's feet carry them (see [`rider`](super::rider)).
//!
//! The shells and rings are built once, from the first horse dressed, and
//! shared by every horse ([`TackKit`]); each horse only spawns entities.

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::gltf::GltfMaterialName;
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use std::f32::consts::{FRAC_PI_2, PI};

use super::figure::Rgb;
use super::finish::{Finish, Palette};
use super::horse::{HorseModel, rest_affine, transform_of};
use super::model::model_range;

/// A piece of the recado or the bridle.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorsePiece {
    /// The woven blanket under the saddle.
    Sudadera,
    /// The leather layer over it.
    Carona,
    /// The two padded rolls the rider sits between.
    Basto,
    /// The sheepskin on top, where the rider sits.
    Pellon,
    /// The rawhide girth round the belly.
    Cincha,
    /// The rawhide headstall: crownpiece, noseband and cheekpieces.
    Headstall,
    Bit,
    Rein,
    StirrupLeather,
    Stirrup,
}

/// A woven jerga, rust red with use.
const JERGA: Rgb = [124, 56, 42];
const CARONA: Rgb = [88, 60, 40];
const BASTO: Rgb = [64, 44, 30];
const PELLON: Rgb = [208, 196, 170];
const RAWHIDE: Rgb = [166, 140, 104];
const IRON: Rgb = [70, 68, 66];

impl HorsePiece {
    /// Its colour and what it is made of.
    pub fn look(self) -> (Rgb, Finish) {
        match self {
            Self::Sudadera => (JERGA, Finish::Wool),
            Self::Carona => (CARONA, Finish::Leather),
            Self::Basto | Self::StirrupLeather => (BASTO, Finish::Leather),
            Self::Pellon => (PELLON, Finish::Fleece),
            Self::Cincha | Self::Headstall | Self::Rein => (RAWHIDE, Finish::Rawhide),
            Self::Bit | Self::Stirrup => (IRON, Finish::Iron),
        }
    }
}

/// The joints and weights a vertex is skinned with (four, summing to one).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Skin {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

impl Skin {
    /// The four heaviest of `pairs` (joint, weight), normalised.
    fn heaviest(pairs: &[(u16, f32)]) -> Self {
        let mut merged: Vec<(u16, f32)> = Vec::with_capacity(pairs.len());
        for &(j, w) in pairs {
            match merged.iter_mut().find(|(k, _)| *k == j) {
                Some((_, sum)) => *sum += w,
                None => merged.push((j, w)),
            }
        }
        merged.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut skin = Self::default();
        let total: f32 = merged.iter().take(4).map(|(_, w)| w.max(0.0)).sum();
        for (k, (j, w)) in merged.into_iter().take(4).enumerate() {
            skin.joints[k] = j;
            skin.weights[k] = if total > 0.0 { w.max(0.0) / total } else { 0.0 };
        }
        if total <= 0.0 {
            skin.weights[0] = 1.0;
        }
        skin
    }
}

/// Where a ray first left the horse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub distance: f32,
    pub skin: Skin,
}

/// The horse's skin at rest in its body frame (meters, +X its front, +Y
/// up, +Z its right, hooves at 0): what the tack is fitted to.
#[derive(Debug, Clone, Default)]
pub struct RestHorse {
    positions: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    skins: Vec<Skin>,
}

impl RestHorse {
    /// A surface from its vertices (with their skins) and triangles.
    pub fn new(positions: Vec<Vec3>, skins: Vec<Skin>, triangles: Vec<[u32; 3]>) -> Self {
        Self {
            positions,
            triangles,
            skins,
        }
    }

    /// Adds one skinned primitive of the model, posed at rest by `skin`
    /// (each joint's body-frame matrix times its inverse bind pose).
    /// `false` if it lacks positions, joints, weights or indices.
    pub fn add(&mut self, mesh: &Mesh, skin: &[Affine3A]) -> bool {
        let (
            Some(VertexAttributeValues::Float32x3(positions)),
            Some(VertexAttributeValues::Uint16x4(joints)),
            Some(VertexAttributeValues::Float32x4(weights)),
            Some(indices),
        ) = (
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
            mesh.indices(),
        )
        else {
            return false;
        };
        let base = self.positions.len() as u32;
        for ((p, j), w) in positions.iter().zip(joints).zip(weights) {
            let mut at = Vec3::ZERO;
            for k in 0..4 {
                if let Some(m) = skin.get(j[k] as usize) {
                    at += w[k] * m.transform_point3(Vec3::from(*p));
                }
            }
            self.positions.push(at);
            let pairs = [0, 1, 2, 3].map(|k| (j[k], w[k]));
            self.skins.push(Skin::heaviest(&pairs));
        }
        let indices: Vec<u32> = indices.iter().map(|i| i as u32).collect();
        for t in indices.as_chunks::<3>().0 {
            self.triangles.push(t.map(|i| i + base));
        }
        true
    }

    /// Where a ray from `origin` along `dir` (unit) first leaves the body,
    /// with the weights of the triangle there, blended by where it hit.
    /// Rays start inside the body, so the first surface met is its skin.
    pub fn cast(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        let mut best: Option<(f32, usize, f32, f32)> = None;
        for (k, t) in self.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|i| self.positions[i as usize]);
            // Möller–Trumbore.
            let (e1, e2) = (b - a, c - a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - a;
            let u = s.dot(p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(e1);
            let v = dir.dot(q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let d = e2.dot(q) * inv;
            if d > 1e-5 && best.is_none_or(|(b, ..)| d < b) {
                best = Some((d, k, u, v));
            }
        }
        let (distance, k, u, v) = best?;
        let corners = self.triangles[k].map(|i| self.skins[i as usize]);
        let mut pairs = [(0u16, 0.0f32); 12];
        for (c, (skin, bary)) in corners.iter().zip([1.0 - u - v, u, v]).enumerate() {
            for i in 0..4 {
                pairs[c * 4 + i] = (skin.joints[i], skin.weights[i] * bary);
            }
        }
        Some(Hit {
            point: origin + dir * distance,
            distance,
            skin: Skin::heaviest(&pairs),
        })
    }
}

/// A line inside the body that a piece is wrapped round: rays leave it at
/// right angles, at angle `theta` from `up` toward `up × along`... that is,
/// toward the horse's right for positive angles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub origin: Vec3,
    /// Unit, along the axis.
    pub along: Vec3,
    /// Unit, at right angles to `along`: where angle 0 points.
    pub up: Vec3,
}

impl Axis {
    /// The barrel's: along the back, 1.14 m up (its middle at the girth).
    pub const BARREL: Self = Self {
        origin: Vec3::new(0.0, 1.14, 0.0),
        along: Vec3::X,
        up: Vec3::Y,
    };

    /// The head's: from behind the poll down its length to the muzzle;
    /// angle 0 is the forehead.
    pub fn head() -> Self {
        let along = Vec3::new(0.755, -0.656, 0.0).normalize();
        Self {
            origin: Vec3::new(1.1, 1.78, 0.01),
            along,
            up: Vec3::new(-along.y, along.x, 0.0),
        }
    }

    /// The ray `t` meters along the axis, leaving it at angle `theta`.
    pub fn ray(&self, t: f32, theta: f32) -> (Vec3, Vec3) {
        let side = self.along.cross(self.up);
        (
            self.origin + self.along * t,
            (self.up * theta.cos() + side * theta.sin()).normalize(),
        )
    }
}

/// How far a layer's inner face stands off the body, meters: `top` within
/// `flat` radians of angle 0, easing to `edge` by `flat + fade`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    pub top: f32,
    pub edge: f32,
    pub flat: f32,
    pub fade: f32,
}

impl Offset {
    pub const fn even(at: f32) -> Self {
        Self {
            top: at,
            edge: at,
            flat: 0.0,
            fade: 1.0,
        }
    }

    pub fn at(&self, theta: f32) -> f32 {
        let s = ((theta.abs() - self.flat) / self.fade.max(1e-6)).clamp(0.0, 1.0);
        let s = s * s * (3.0 - 2.0 * s);
        self.top + (self.edge - self.top) * s
    }
}

/// A layer wrapped round the body: `along` the axis (meters) and `around`
/// it (radians), standing off by `offset`, `thickness` deep, its outer face
/// roughened by `shag` (a share of the thickness) like a sheepskin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shell {
    pub piece: HorsePiece,
    pub axis: Axis,
    pub along: (f32, f32),
    pub around: (f32, f32),
    pub offset: Offset,
    pub thickness: f32,
    pub shag: f32,
    /// Grid cells along and around.
    pub cells: (usize, usize),
}

/// A skinned mesh in the body frame: positions, skins and triangles, before
/// it becomes a [`Mesh`] ([`skinned_mesh`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TackMesh {
    pub positions: Vec<Vec3>,
    pub skins: Vec<Skin>,
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Shell {
    /// The layer fitted to `horse`, and how many of its rays missed it
    /// (those vertices stand off the axis by the nearest hit's distance).
    pub fn fit(&self, horse: &RestHorse) -> (TackMesh, usize) {
        let (along, around) = self.cells;
        let row = around + 1;
        let mut inner = Vec::with_capacity((along + 1) * row);
        let mut misses = 0;
        let mut last: Option<Hit> = None;
        for i in 0..=along {
            let t = self.along.0 + (self.along.1 - self.along.0) * i as f32 / along as f32;
            for k in 0..=around {
                let theta =
                    self.around.0 + (self.around.1 - self.around.0) * k as f32 / around as f32;
                let (origin, dir) = self.axis.ray(t, theta);
                let hit = match horse.cast(origin, dir) {
                    Some(hit) => hit,
                    None => {
                        misses += 1;
                        let reach = last.map_or(0.2, |h| h.distance);
                        Hit {
                            point: origin + dir * reach,
                            distance: reach,
                            skin: last.map_or_else(Skin::default, |h| h.skin),
                        }
                    }
                };
                last = Some(hit);
                inner.push((hit, dir, theta));
            }
        }
        let mut mesh = TackMesh::default();
        for (face, depth) in [(0, 1.0), (1, 0.0)] {
            for (n, (hit, dir, theta)) in inner.iter().enumerate() {
                let (i, k) = (n / row, n % row);
                // Tufts on the outer face only, the same every time.
                let tuft = if face == 0 {
                    ((i * 7 + k * 13) % 5) as f32 / 4.0
                } else {
                    0.0
                };
                let out =
                    self.offset.at(*theta) + depth * self.thickness * (1.0 - self.shag * tuft);
                mesh.positions.push(hit.point + *dir * out);
                mesh.skins.push(hit.skin);
                mesh.uvs
                    .push([i as f32 / along as f32, k as f32 / around as f32]);
            }
        }
        let at = |face: usize, i: usize, k: usize| (face * (along + 1) * row + i * row + k) as u32;
        let tris = &mut mesh.triangles;
        for i in 0..along {
            for k in 0..around {
                let (a, b, c, d) = (
                    at(0, i, k),
                    at(0, i, k + 1),
                    at(0, i + 1, k),
                    at(0, i + 1, k + 1),
                );
                tris.extend([[a, b, c], [b, d, c]]);
                let (a, b, c, d) = (
                    at(1, i, k),
                    at(1, i, k + 1),
                    at(1, i + 1, k),
                    at(1, i + 1, k + 1),
                );
                tris.extend([[a, c, b], [b, c, d]]);
            }
        }
        // The two ends and the two side edges close the shell.
        for k in 0..around {
            let (o, o1, n, n1) = (at(0, 0, k), at(0, 0, k + 1), at(1, 0, k), at(1, 0, k + 1));
            tris.extend([[o, n, o1], [o1, n, n1]]);
            let (o, o1, n, n1) = (
                at(0, along, k),
                at(0, along, k + 1),
                at(1, along, k),
                at(1, along, k + 1),
            );
            tris.extend([[o, o1, n], [o1, n1, n]]);
        }
        for i in 0..along {
            let (o, o1, n, n1) = (
                at(0, i, around),
                at(0, i + 1, around),
                at(1, i, around),
                at(1, i + 1, around),
            );
            tris.extend([[o, n, o1], [o1, n, n1]]);
            let (o, o1, n, n1) = (at(0, i, 0), at(0, i + 1, 0), at(1, i, 0), at(1, i + 1, 0));
            tris.extend([[o, o1, n], [o1, n1, n]]);
        }
        (mesh, misses)
    }
}

/// A small ring standing on the body at `hit`, facing out along `dir`:
/// `radius` across, its bar `bar` thick. Skinned as the skin under it.
fn ring_on(hit: &Hit, dir: Vec3, radius: f32, bar: f32, lift: f32) -> TackMesh {
    let unit = ring();
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir);
    let center = hit.point + dir * lift;
    let mut mesh = TackMesh::default();
    let Some(VertexAttributeValues::Float32x3(p)) = unit.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return mesh;
    };
    for v in p {
        let v = Vec3::from(*v) * Vec3::new(2.0 * radius, 2.0 * radius, bar / 0.15);
        mesh.positions.push(center + rotation * v);
        mesh.skins.push(hit.skin);
        mesh.uvs.push([0.0, 0.0]);
    }
    if let Some(indices) = unit.indices() {
        let i: Vec<u32> = indices.iter().map(|i| i as u32).collect();
        mesh.triangles.extend(i.as_chunks::<3>().0);
    }
    mesh
}

/// The unit iron ring: 1 m across in its XY plane (facing +Z), its bar
/// 0.15 m thick.
pub fn ring() -> Mesh {
    Torus::new(0.35, 0.5)
        .mesh()
        .minor_resolution(4)
        .major_resolution(10)
        .build()
        .rotated_by(Quat::from_rotation_x(FRAC_PI_2))
}

/// The unit strap: 1 m long along +Y, centered, 1 m across.
pub fn strap() -> Mesh {
    Cylinder::new(0.5, 1.0).mesh().resolution(5).build()
}

/// `mesh` as a skinned [`Mesh`] with flat normals: its joint indices are
/// into the horse's skin joints.
pub fn skinned_mesh(mesh: &TackMesh) -> Mesh {
    let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
    for t in &mesh.triangles {
        indices.extend(t);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        mesh.positions
            .iter()
            .map(|p| p.to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mesh.uvs.clone())
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_INDEX,
        VertexAttributeValues::Uint16x4(mesh.skins.iter().map(|s| s.joints).collect()),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_WEIGHT,
        mesh.skins.iter().map(|s| s.weights).collect::<Vec<_>>(),
    )
    .with_inserted_indices(Indices::U32(indices))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}

/// The barrel's layers, back to front and inside out: the sudadera, the
/// carona, the bastos either side of the spine, the pellón over them, and
/// the cincha in front of them, round the girth.
pub fn saddle() -> [Shell; 6] {
    let barrel = Axis::BARREL;
    let layer = |piece, along, span: f32, offset, thickness, shag, cells| Shell {
        piece,
        axis: barrel,
        along,
        around: (-span, span),
        offset,
        thickness,
        shag,
        cells,
    };
    let basto = |side: f32| Shell {
        around: (side * 0.21, side * 0.45),
        ..layer(
            HorsePiece::Basto,
            (-0.19, 0.23),
            0.0,
            Offset::even(0.044),
            0.05,
            0.0,
            (8, 4),
        )
    };
    [
        layer(
            HorsePiece::Sudadera,
            (-0.19, 0.24),
            1.45,
            Offset {
                top: 0.016,
                edge: 0.024,
                flat: 1.0,
                fade: 0.45,
            },
            0.012,
            0.0,
            (12, 20),
        ),
        layer(
            HorsePiece::Carona,
            (-0.17, 0.22),
            1.3,
            Offset {
                top: 0.03,
                edge: 0.038,
                flat: 1.0,
                fade: 0.3,
            },
            0.012,
            0.0,
            (10, 18),
        ),
        basto(1.0),
        basto(-1.0),
        layer(
            HorsePiece::Pellon,
            (-0.16, 0.2),
            1.05,
            Offset {
                top: 0.097,
                edge: 0.047,
                flat: 0.5,
                fade: 0.5,
            },
            0.045,
            0.35,
            (9, 16),
        ),
        Shell {
            around: (-PI, PI),
            ..layer(
                HorsePiece::Cincha,
                (0.26, 0.33),
                0.0,
                Offset {
                    top: 0.018,
                    edge: 0.014,
                    flat: 0.4,
                    fade: 0.6,
                },
                0.008,
                0.0,
                (2, 36),
            )
        },
    ]
}

/// The headstall: the crownpiece over the poll behind the ears and down
/// to the cheeks (not round the throat: below it the head runs into the
/// neck), the noseband, and a cheekpiece down each side to the bit.
pub fn bridle() -> [Shell; 4] {
    let head = Axis::head();
    let band = |along, around| Shell {
        piece: HorsePiece::Headstall,
        axis: head,
        along,
        around,
        offset: Offset::even(0.006),
        thickness: 0.01,
        shag: 0.0,
        cells: (1, 28),
    };
    let cheek = |side: f32| Shell {
        cells: (6, 1),
        ..band(
            (0.05, 0.38),
            (side * (FRAC_PI_2 + 0.22), side * (FRAC_PI_2 + 0.36)),
        )
    };
    [
        band((0.03, 0.065), (-1.9, 1.9)),
        band((0.3, 0.33), (-PI, PI)),
        cheek(1.0),
        cheek(-1.0),
    ]
}

/// The ray from inside the muzzle out to the corner of the mouth on side
/// `s` (-1 left, 1 right), where a bit ring sits. The head is a little to
/// the right of the spine (its middle 1.6 cm over).
pub fn bit_ray(s: f32) -> (Vec3, Vec3) {
    (Vec3::new(1.33, 1.48, 0.016), Vec3::Z * s)
}

/// The ray from inside the neck out to its side, a hand below the crest,
/// on side `s`: the reins lie along the neck there.
pub fn neck_ray(s: f32) -> (Vec3, Vec3) {
    (
        Vec3::new(0.7, 1.45, 0.0),
        Vec3::new(0.0, 1.0, 0.55 * s).normalize(),
    )
}

/// The ray from inside the head out to the jowl on side `s`, where the
/// rein turns back from the bit toward the rider's hands, clear of the
/// cheek.
pub fn jowl_ray(s: f32) -> (Vec3, Vec3) {
    (
        Vec3::new(1.12, 1.62, 0.015),
        Vec3::new(0.0, -0.2, s).normalize(),
    )
}

/// The points on the rest horse (body frame) the straps and the rider are
/// hung from, worked out from its surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fittings {
    /// The bit's rings, left then right (on the head).
    pub bit: [Vec3; 2],
    /// Where the reins pass the jowls, from the bit back (on the head).
    pub jowl: [Vec3; 2],
    /// Where they lie along the neck (on its middle bone).
    pub neck: [Vec3; 2],
    /// Where the reins rest on a horse without a rider: on the pommel.
    pub pommel: [Vec3; 2],
    /// Where the stirrup leathers come out from under the pellón.
    pub leather: [Vec3; 2],
    /// Where a stirrup hangs with nobody's foot in it (its ring's middle).
    pub hanging: [Vec3; 2],
    /// The top of the pellón, where the rider sits.
    pub seat: Vec3,
}

/// How far behind the girth the rider sits: the lowest point of the back,
/// between the withers and the croup.
pub const SEAT_X: f32 = -0.02;

impl Fittings {
    /// The fittings on `horse`, or `None` if it is not shaped like one.
    pub fn find(horse: &RestHorse) -> Option<Self> {
        let barrel = Axis::BARREL;
        let pellon = saddle()[4];
        let side = |f: &dyn Fn(f32) -> Option<Vec3>| Some([f(-1.0)?, f(1.0)?]);
        let out = |(origin, dir): (Vec3, Vec3), by: f32| {
            horse.cast(origin, dir).map(|h| h.point + dir * by)
        };
        let bit = side(&|s| out(bit_ray(s), 0.012))?;
        let jowl = side(&|s| out(jowl_ray(s), 0.035))?;
        let neck = side(&|s| out(neck_ray(s), 0.035))?;
        let top = |x: f32, theta: f32, extra: f32| {
            let (origin, dir) = barrel.ray(x, theta);
            horse
                .cast(origin, dir)
                .map(|h| h.point + dir * (pellon.offset.at(theta) + pellon.thickness + extra))
        };
        let pommel = side(&|s| top(0.2, s * 0.3, 0.01))?;
        let leather = side(&|s| top(0.08, s * 1.0, 0.012))?;
        let seat = top(SEAT_X, 0.0, 0.0)?;
        // Straight down from the leather, clear of the barrel below.
        let hanging = side(&|s| {
            let (origin, dir) = barrel.ray(0.08, s * FRAC_PI_2);
            let flank = horse.cast(origin, dir)?;
            Some(Vec3::new(0.08, 0.98, s * (flank.point.z.abs() + 0.06)))
        })?;
        Some(Self {
            bit,
            jowl,
            neck,
            pommel,
            leather,
            hanging,
            seat,
        })
    }
}

/// Every shell and ring of the tack fitted to `horse`, and the rays that
/// missed it.
pub fn fit_tack(horse: &RestHorse) -> (Vec<(HorsePiece, TackMesh)>, usize) {
    let mut out = Vec::new();
    let mut misses = 0;
    for shell in saddle().iter().chain(&bridle()) {
        let (mesh, missed) = shell.fit(horse);
        misses += missed;
        out.push((shell.piece, mesh));
    }
    for s in [-1.0, 1.0] {
        let (origin, dir) = bit_ray(s);
        match horse.cast(origin, dir) {
            Some(hit) => out.push((HorsePiece::Bit, ring_on(&hit, dir, 0.03, 0.008, 0.02))),
            None => misses += 1,
        }
    }
    (out, misses)
}

/// The tack built once and shared by every horse.
#[derive(Debug, Clone)]
pub struct TackKit {
    /// Each skinned shell and ring with its material.
    pub pieces: Vec<(HorsePiece, Handle<Mesh>, Handle<StandardMaterial>)>,
    /// The tack's own inverse bind poses: each horse joint's body-frame
    /// rest, inverted (its vertices are in the body frame).
    pub bindposes: Handle<SkinnedMeshInverseBindposes>,
    pub strap: Handle<Mesh>,
    pub ring: Handle<Mesh>,
    pub fittings: Fittings,
}

/// The size of a stirrup: across, tall, and its bar's depth.
pub const STIRRUP: Vec3 = Vec3::new(0.13, 0.16, 0.012 / 0.15);

/// How thick the reins and the stirrup leathers are, meters.
pub const REIN: f32 = 0.014;
pub const LEATHER: f32 = 0.03;

/// A straight piece of tack between two points that move apart: each end
/// is a point in an entity's own space. Stretched by [`stretch_straps`].
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Strap {
    pub ends: [(Entity, Vec3); 2],
    /// Across, meters.
    pub width: f32,
}

/// A horse's tack, as a rider takes it up: the straps run to the rider's
/// hand and the stirrups go on its feet; it sits on the torso bone at the
/// seat. On the horse's body.
#[derive(Component, Debug, Clone, Copy)]
pub struct Tack {
    /// The reins' last stretch (from the neck), left then right.
    pub reins: [Entity; 2],
    /// The stirrups, left then right.
    pub stirrups: [Entity; 2],
    /// The horse's `Torso` and `Head` bones.
    pub torso: Entity,
    pub head: Entity,
    /// The top of the pellón (body frame), and the torso's rest placement.
    pub seat: Vec3,
    pub torso_rest: Affine3A,
    /// The middle of the bit's rings, on the head.
    pub bit: Vec3,
}

/// The bones the tack and the rider hang from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorseBone {
    /// The middle of the back, under the saddle.
    Torso,
    /// The middle of the neck.
    Neck2,
    Head,
}

impl HorseBone {
    pub fn name(self) -> &'static str {
        match self {
            Self::Torso => "Torso",
            Self::Neck2 => "Neck2",
            Self::Head => "Head",
        }
    }
}

/// What the tack is fitted to on a horse's scene: its nodes' names and
/// rests, and each skinned primitive with its material.
#[derive(SystemParam)]
pub(super) struct TackScene<'w, 's> {
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static Transform>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    skins: Query<
        'w,
        's,
        (
            &'static SkinnedMesh,
            &'static Mesh3d,
            Option<&'static GltfMaterialName>,
        ),
    >,
}

/// The stores the tack is built from.
#[derive(SystemParam)]
pub(super) struct TackStores<'w> {
    materials: ResMut<'w, Assets<StandardMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    images: ResMut<'w, Assets<Image>>,
    bindposes: Option<ResMut<'w, Assets<SkinnedMeshInverseBindposes>>>,
    palette: ResMut<'w, Palette>,
}

/// The tack kit, its bones' rest placements converted: the fittings in
/// their bones' own spaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalFittings {
    /// On the head.
    pub bit: [Vec3; 2],
    pub jowl: [Vec3; 2],
    /// On `Neck2`.
    pub neck: [Vec3; 2],
    /// On the torso, like the rest.
    pub pommel: [Vec3; 2],
    pub leather: [Vec3; 2],
    pub hanging: [Transform; 2],
}

/// The fittings in the bones' spaces, from their rest placements `rest`
/// (the torso's, the neck's and the head's).
pub fn local_fittings(f: &Fittings, rest: [Affine3A; 3]) -> LocalFittings {
    let [torso, neck, head] = rest.map(|a| a.inverse());
    let on = |a: Affine3A, p: [Vec3; 2]| p.map(|p| a.transform_point3(p));
    let hang = |p: Vec3| {
        transform_of(
            torso
                * Affine3A::from_scale_rotation_translation(
                    STIRRUP,
                    Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    p,
                ),
        )
    };
    LocalFittings {
        bit: on(head, f.bit),
        jowl: on(head, f.jowl),
        neck: on(neck, f.neck),
        pommel: on(torso, f.pommel),
        leather: on(torso, f.leather),
        hanging: f.hanging.map(hang),
    }
}

/// The kit as one horse puts it on: the shared meshes, its fittings and
/// the torso's rest (a rider is seated from it).
#[derive(Debug, Clone)]
pub struct HorseTack {
    pub kit: TackKit,
    pub local: LocalFittings,
    pub torso: Affine3A,
}

/// Builds the tack kit from a horse's scene under `root`: its rest skin in
/// the body frame, the shells fitted to it, and the inverse bind poses
/// that put body-frame vertices on its joints. `None` if the model lacks
/// its skin or bones.
fn build_tack(
    scene: &TackScene,
    stores: &mut TackStores,
    root: Entity,
    joints: &[Entity],
    bindposes: &Handle<SkinnedMeshInverseBindposes>,
) -> Option<HorseTack> {
    let rest: Vec<Affine3A> = joints
        .iter()
        .map(|j| rest_affine(*j, root, &scene.parents, &scene.transforms))
        .collect();
    let inverse_binds = stores.bindposes.as_ref()?.get(bindposes)?;
    let skin: Vec<Affine3A> = rest
        .iter()
        .zip(inverse_binds.iter())
        .map(|(r, ibm)| *r * Affine3A::from_mat4(*ibm))
        .collect();
    let mut horse = RestHorse::default();
    for entity in scene.children.iter_descendants(root) {
        let Ok((_, mesh, name)) = scene.skins.get(entity) else {
            continue;
        };
        // The eyes stand off the face; the tack goes round them.
        if name.is_some_and(|n| n.0.starts_with("Eye")) {
            continue;
        }
        if let Some(mesh) = stores.meshes.get(&mesh.0) {
            horse.add(mesh, &skin);
        }
    }
    let (fitted, misses) = fit_tack(&horse);
    if misses > 0 {
        warn!("the horse's tack missed its body {misses} times");
    }
    let fittings = Fittings::find(&horse)?;
    let bone = |b: HorseBone| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == b.name()))
            .map(|e| rest_affine(e, root, &scene.parents, &scene.transforms))
    };
    let bones = [
        bone(HorseBone::Torso)?,
        bone(HorseBone::Neck2)?,
        bone(HorseBone::Head)?,
    ];
    let inverse: Vec<Mat4> = rest.iter().map(|r| Mat4::from(r.inverse())).collect();
    let pieces = fitted
        .iter()
        .map(|(piece, mesh)| {
            let handle = stores.meshes.add(skinned_mesh(mesh));
            let paint = material(
                *piece,
                &mut stores.palette,
                &mut stores.materials,
                &mut stores.images,
            );
            (*piece, handle, paint)
        })
        .collect();
    let kit = TackKit {
        pieces,
        bindposes: stores.bindposes.as_mut()?.add(inverse),
        strap: stores.meshes.add(strap()),
        ring: stores.meshes.add(ring()),
        fittings,
    };
    Some(HorseTack {
        local: local_fittings(&kit.fittings, bones),
        torso: bones[0],
        kit,
    })
}

/// Puts the tack on a horse model the moment its scene spawns (see the
/// module notes). The kit is built from the first horse dressed; a scene
/// spawned again (its asset reloaded) has the old tack taken off first
/// (the stirrups and any rider went with its bones).
pub(super) fn dress_tack(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    mut stores: TackStores,
    scene: TackScene,
    roots: Query<&ChildOf, With<HorseModel>>,
    pieces: Query<(), With<HorsePiece>>,
    mut tack: Local<Option<HorseTack>>,
) {
    let root = ready.entity;
    let Ok(child_of) = roots.get(root) else {
        return;
    };
    let carrier = child_of.parent();
    for &child in scene.children.get(carrier).into_iter().flatten() {
        if pieces.contains(child) {
            commands.entity(child).try_despawn();
        }
    }
    // The skin of the body, its joints and their bind poses.
    let skin = scene.children.iter_descendants(root).find_map(|e| {
        let (skin, _, name) = scene.skins.get(e).ok()?;
        name.is_some_and(|n| n.0 == "Main").then(|| skin.clone())
    });
    let Some(skin) = skin else {
        warn!("horse model without its body's skin: no tack");
        return;
    };
    if tack.is_none() {
        *tack = build_tack(
            &scene,
            &mut stores,
            root,
            &skin.joints,
            &skin.inverse_bindposes,
        );
    }
    let Some(tack) = tack.as_ref() else {
        warn!("the horse's tack could not be fitted");
        return;
    };
    let bone = |b: HorseBone| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == b.name()))
    };
    let (Some(torso), Some(neck), Some(head)) = (
        bone(HorseBone::Torso),
        bone(HorseBone::Neck2),
        bone(HorseBone::Head),
    ) else {
        return;
    };
    put_on_tack(
        &mut commands,
        &mut stores,
        tack,
        carrier,
        &skin,
        [torso, neck, head],
    );
}

/// Puts the shared tack on the horse `carrier`: the skinned shells on its
/// `skin`'s joints, the stirrups hanging from the torso and the straps (in
/// order torso, neck, head: `bones`).
fn put_on_tack(
    commands: &mut Commands,
    stores: &mut TackStores,
    tack: &HorseTack,
    carrier: Entity,
    skin: &SkinnedMesh,
    [torso, neck, head]: [Entity; 3],
) {
    let range = model_range();
    let kit = &tack.kit;
    let local = &tack.local;
    for (piece, mesh, paint) in &kit.pieces {
        commands.spawn((
            Name::new(format!("{piece:?}")),
            *piece,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(paint.clone()),
            SkinnedMesh {
                inverse_bindposes: kit.bindposes.clone(),
                joints: skin.joints.clone(),
            },
            Transform::default(),
            range.clone(),
            ChildOf(carrier),
        ));
    }
    let mut paint = |piece| {
        material(
            piece,
            &mut stores.palette,
            &mut stores.materials,
            &mut stores.images,
        )
    };
    let (iron, leather, rein) = (
        paint(HorsePiece::Stirrup),
        paint(HorsePiece::StirrupLeather),
        paint(HorsePiece::Rein),
    );
    let spawn_strap =
        |commands: &mut Commands, piece, ends, width, material: &Handle<StandardMaterial>| {
            commands
                .spawn((
                    Name::new(format!("{piece:?}")),
                    piece,
                    Strap { ends, width },
                    Mesh3d(kit.strap.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    range.clone(),
                    ChildOf(carrier),
                ))
                .id()
        };
    let mut reins = [Entity::PLACEHOLDER; 2];
    let mut stirrups = [Entity::PLACEHOLDER; 2];
    for s in 0..2 {
        spawn_strap(
            commands,
            HorsePiece::Rein,
            [(head, local.bit[s]), (head, local.jowl[s])],
            REIN,
            &rein,
        );
        spawn_strap(
            commands,
            HorsePiece::Rein,
            [(head, local.jowl[s]), (neck, local.neck[s])],
            REIN,
            &rein,
        );
        reins[s] = spawn_strap(
            commands,
            HorsePiece::Rein,
            [(neck, local.neck[s]), (torso, local.pommel[s])],
            REIN,
            &rein,
        );
        stirrups[s] = commands
            .spawn((
                Name::new("Stirrup"),
                HorsePiece::Stirrup,
                Mesh3d(kit.ring.clone()),
                MeshMaterial3d(iron.clone()),
                local.hanging[s],
                range.clone(),
                ChildOf(torso),
            ))
            .id();
        spawn_strap(
            commands,
            HorsePiece::StirrupLeather,
            [(torso, local.leather[s]), (stirrups[s], Vec3::Y * 0.5)],
            LEATHER,
            &leather,
        );
    }
    commands.entity(carrier).try_insert(Tack {
        reins,
        stirrups,
        torso,
        head,
        seat: tack.kit.fittings.seat,
        torso_rest: tack.torso,
        bit: (local.bit[0] + local.bit[1]) / 2.0,
    });
}

/// The world placement of a strap from `a` to `b`, `width` across.
pub fn strap_between(a: Vec3, b: Vec3, width: f32) -> GlobalTransform {
    let along = b - a;
    let rotation = Quat::from_rotation_arc(Vec3::Y, along.normalize_or(Vec3::Y));
    GlobalTransform::from(Transform {
        translation: (a + b) / 2.0,
        rotation,
        scale: Vec3::new(width, along.length().max(1e-4), width),
    })
}

/// Stretches every strap between its two ends, once the transforms are
/// worked out: it is drawn this frame where its ends are this frame.
pub(super) fn stretch_straps(
    mut straps: Query<(&Strap, &ChildOf, &mut Transform, &mut GlobalTransform)>,
    places: Query<&GlobalTransform, Without<Strap>>,
) {
    for (strap, child_of, mut transform, mut global) in &mut straps {
        let [(a, at_a), (b, at_b)] = strap.ends;
        let (Ok(a), Ok(b), Ok(parent)) =
            (places.get(a), places.get(b), places.get(child_of.parent()))
        else {
            continue;
        };
        let placed = strap_between(
            a.transform_point(at_a),
            b.transform_point(at_b),
            strap.width,
        );
        *global = placed;
        *transform =
            Transform::from_matrix(Mat4::from(parent.affine().inverse() * placed.affine()));
    }
}

/// The material of `piece`.
pub fn material(
    piece: HorsePiece,
    palette: &mut Palette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Handle<StandardMaterial> {
    let (color, finish) = piece.look();
    palette.material(color, finish, materials, images)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A stand-in horse: a closed tube along +X round the barrel's axis
    /// (radius 0.28, from -0.8 to 0.8 m), its skin turning from joint 0 at
    /// the back to joint 1 at the front, a head-like tube round the head's
    /// axis on joint 2 and a neck on joint 3.
    pub(crate) fn tube_horse() -> RestHorse {
        let mut positions = Vec::new();
        let mut skins = Vec::new();
        let mut triangles = Vec::new();
        let mut tube = |axis: Axis, length: (f32, f32), radius: f32, joint: Option<u16>| {
            let base = positions.len() as u32;
            let (rings, around) = (16, 24);
            for i in 0..=rings {
                let t = length.0 + (length.1 - length.0) * i as f32 / rings as f32;
                for k in 0..around {
                    let theta = k as f32 / around as f32 * std::f32::consts::TAU;
                    let (o, d) = axis.ray(t, theta);
                    positions.push(o + d * radius);
                    let front = i as f32 / rings as f32;
                    skins.push(match joint {
                        Some(j) => Skin {
                            joints: [j, 0, 0, 0],
                            weights: [1.0, 0.0, 0.0, 0.0],
                        },
                        None => Skin {
                            joints: [0, 1, 0, 0],
                            weights: [1.0 - front, front, 0.0, 0.0],
                        },
                    });
                }
            }
            for i in 0..rings {
                for k in 0..around {
                    let at = |i: u32, k: u32| base + i * around as u32 + k % around as u32;
                    let (i, k) = (i as u32, k as u32);
                    triangles.push([at(i, k), at(i + 1, k), at(i, k + 1)]);
                    triangles.push([at(i, k + 1), at(i + 1, k), at(i + 1, k + 1)]);
                }
            }
        };
        tube(Axis::BARREL, (-0.8, 0.8), 0.28, None);
        tube(Axis::head(), (-0.1, 0.55), 0.13, Some(2));
        let neck = Vec3::new(0.64, 0.768, 0.0);
        tube(
            Axis {
                origin: Vec3::new(0.6, 1.2, 0.0),
                along: neck,
                up: Vec3::new(-neck.y, neck.x, 0.0),
            },
            (0.0, 0.7),
            0.12,
            Some(3),
        );
        RestHorse::new(positions, skins, triangles)
    }

    #[test]
    fn a_ray_from_inside_meets_the_skin_with_its_blended_weights() {
        let horse = tube_horse();
        let (origin, dir) = Axis::BARREL.ray(0.4, 0.0);
        let hit = horse.cast(origin, dir).unwrap();
        // The polygon's flat between corners sits a little inside the
        // circle it is built on.
        assert!((hit.distance - 0.28).abs() < 0.01, "{hit:?}");
        assert!((hit.point.x - 0.4).abs() < 1e-4);
        // A quarter of the way from the front: 75% the front joint.
        let front = hit.skin.joints.iter().position(|j| *j == 1).unwrap();
        assert!((hit.skin.weights[front] - 0.75).abs() < 0.02, "{hit:?}");
        assert!((hit.skin.weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        // Outside the body, a ray away from it meets nothing.
        assert!(horse.cast(Vec3::new(0.0, 3.0, 0.0), Vec3::Y).is_none());
    }

    #[test]
    fn a_shell_lies_its_offset_off_the_body_and_its_thickness_deep() {
        let horse = tube_horse();
        let shell = saddle()[0];
        let (mesh, misses) = shell.fit(&horse);
        assert_eq!(misses, 0);
        let (along, around) = shell.cells;
        let face = (along + 1) * (around + 1);
        assert_eq!(mesh.positions.len(), 2 * face);
        for k in 0..face {
            let (outer, inner) = (mesh.positions[k], mesh.positions[face + k]);
            let off = |p: Vec3| Vec2::new(p.y - 1.14, p.z).length() - 0.28;
            assert!(
                (off(inner) - shell.offset.top).abs() < 0.012,
                "{} at {inner}",
                off(inner)
            );
            assert!((outer.distance(inner) - shell.thickness).abs() < 1e-4);
            // Inside and out share the skin under them.
            assert_eq!(mesh.skins[k], mesh.skins[face + k]);
        }
        // The front edge follows the front joint more than the back one.
        let weight_of = |s: &Skin, j: u16| {
            s.joints
                .iter()
                .zip(s.weights)
                .filter(|(k, _)| **k == j)
                .map(|(_, w)| w)
                .sum::<f32>()
        };
        assert!(weight_of(&mesh.skins[face - 1], 1) > weight_of(&mesh.skins[0], 1));
        assert!(
            mesh.triangles
                .iter()
                .flatten()
                .all(|i| (*i as usize) < mesh.positions.len())
        );
    }

    #[test]
    fn the_saddle_layers_nest_and_the_pellon_is_thick_and_shaggy() {
        let horse = tube_horse();
        let up = |piece: HorsePiece| {
            let shell = saddle().into_iter().find(|s| s.piece == piece).unwrap();
            let (mesh, _) = shell.fit(&horse);
            let (along, around) = shell.cells;
            let face = (along + 1) * (around + 1);
            // The outer face's highest point and the inner face's, over
            // the spine.
            let top = |range: std::ops::Range<usize>| {
                mesh.positions[range]
                    .iter()
                    .filter(|p| p.z.abs() < 0.03)
                    .map(|p| p.y)
                    .fold(f32::MIN, f32::max)
            };
            (top(face..2 * face), top(0..face))
        };
        let (_, sudadera) = up(HorsePiece::Sudadera);
        let (carona_in, carona) = up(HorsePiece::Carona);
        let (pellon_in, pellon) = up(HorsePiece::Pellon);
        assert!(
            sudadera < carona_in && carona < pellon_in,
            "{sudadera} {carona_in} {carona} {pellon_in}"
        );
        assert!(pellon - pellon_in > 0.03, "a thick sheepskin");
        // The bastos lie between the carona and the pellón.
        for basto in saddle().iter().filter(|s| s.piece == HorsePiece::Basto) {
            let (mesh, _) = basto.fit(&horse);
            let high = mesh.positions.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            assert!(high > carona && high < pellon, "{high}");
        }
    }

    #[test]
    fn the_fittings_are_found_on_the_body() {
        let horse = tube_horse();
        let f = Fittings::find(&horse).unwrap();
        for s in 0..2 {
            // Left is -Z.
            let sign = if s == 0 { -1.0 } else { 1.0 };
            for p in [f.pommel[s], f.leather[s], f.hanging[s]] {
                assert!(p.z * sign > 0.0, "{s}: {p}");
            }
            assert!(f.hanging[s].z.abs() > 0.28, "clear of the barrel");
        }
        let seat = Vec2::new(f.seat.y - 1.14, f.seat.z).length() - 0.28;
        let pellon = saddle()[4];
        assert!(
            (seat - pellon.offset.top - pellon.thickness).abs() < 0.012,
            "{seat}"
        );
    }

    #[test]
    fn a_strap_runs_from_one_end_to_the_other() {
        let (a, b) = (Vec3::new(1.0, 2.0, 0.5), Vec3::new(-0.5, 1.0, 0.2));
        let placed = strap_between(a, b, 0.02);
        assert!(placed.transform_point(Vec3::Y * 0.5).distance(b) < 1e-5);
        assert!(placed.transform_point(Vec3::Y * -0.5).distance(a) < 1e-5);
        let (scale, _, _) = placed.to_scale_rotation_translation();
        assert!((scale.x - 0.02).abs() < 1e-6 && (scale.z - 0.02).abs() < 1e-6);
    }

    // --- with the shipped horse ----------------------------------------------

    use crate::character::cpu;
    use crate::character::horse::tests::saddled;
    use crate::character::horse::{HorseBody, TIED_IDLES};
    use crate::character::{CharacterVisual, Coat, FigureKind};
    use bevy::mesh::skinning::SkinnedMesh;

    /// The signed distance from `p` to the nearest of `triangles` (over
    /// `positions`), positive on the side its winding faces.
    pub(crate) fn signed_distance(p: Vec3, positions: &[Vec3], triangles: &[[u32; 3]]) -> f32 {
        let mut best = (f32::MAX, 0.0);
        for t in triangles {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            let q = closest_on_triangle(p, a, b, c);
            let d = p.distance_squared(q);
            if d < best.0 {
                let n = (b - a).cross(c - a).normalize_or_zero();
                best = (d, (p - q).dot(n).signum());
            }
        }
        best.0.sqrt() * best.1
    }

    /// The closest point to `p` on the triangle `a b c` (Ericson).
    pub(crate) fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
        let (ab, ac, ap) = (b - a, c - a, p - a);
        let (d1, d2) = (ab.dot(ap), ac.dot(ap));
        if d1 <= 0.0 && d2 <= 0.0 {
            return a;
        }
        let bp = p - b;
        let (d3, d4) = (ab.dot(bp), ac.dot(bp));
        if d3 >= 0.0 && d4 <= d3 {
            return b;
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            return a + ab * (d1 / (d1 - d3));
        }
        let cp = p - c;
        let (d5, d6) = (ab.dot(cp), ac.dot(cp));
        if d6 >= 0.0 && d5 <= d6 {
            return c;
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            return a + ac * (d2 / (d2 - d6));
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
        }
        let denom = 1.0 / (va + vb + vc);
        a + ab * (vb * denom) + ac * (vc * denom)
    }

    /// Whether the segment `a b` passes through any of `triangles`.
    pub(crate) fn crosses(a: Vec3, b: Vec3, positions: &[Vec3], triangles: &[[u32; 3]]) -> bool {
        let dir = b - a;
        triangles.iter().any(|t| {
            let [p, q, r] = t.map(|i| positions[i as usize]);
            let (e1, e2) = (q - p, r - p);
            let h = dir.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-12 {
                return false;
            }
            let s = a - p;
            let u = s.dot(h) / det;
            let qv = s.cross(e1);
            let v = dir.dot(qv) / det;
            let t = e2.dot(qv) / det;
            (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)
        })
    }

    /// The horse's skin (its body's materials, not its mane or eyes) in
    /// `drawn`: positions and triangles, merged.
    pub(crate) fn horse_skin(drawn: &[cpu::Drawn]) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for d in drawn {
            if ["Main", "Main_Dark", "Main_Light", "Muzzle", "Hooves"].contains(&d.name.as_str()) {
                let base = positions.len() as u32;
                positions.extend(&d.positions);
                triangles.extend(d.triangles.iter().map(|t| t.map(|i| i + base)));
            }
        }
        (positions, triangles)
    }

    /// The worst clearances over `frames` frames of `app`'s saddled horse
    /// under `me`, within 10 cm of its skin: how far the tack's shells stand
    /// off it (negative: inside it), how far the rider's model does, and
    /// the straps that cut through the skin.
    pub(crate) fn clearances(app: &mut App, me: Entity, frames: usize) -> (f32, f32, Vec<String>) {
        let shells = [
            "Sudadera",
            "Carona",
            "Basto",
            "Pellon",
            "Cincha",
            "Headstall",
            "Bit",
        ];
        let not_rider = [
            "Main",
            "Main_Dark",
            "Main_Light",
            "Muzzle",
            "Hooves",
            "Hair",
            "Eye_Black",
            "Eye_White",
            "Rein",
            "StirrupLeather",
            "Stirrup",
        ];
        let (mut tack, mut rider, mut cuts) = (f32::MAX, f32::MAX, Vec::new());
        for _ in 0..frames {
            app.update();
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            let (skin, tris) = horse_skin(&drawn);
            for d in &drawn {
                let (is_tack, is_rider) = (
                    shells.contains(&d.name.as_str()),
                    !shells.contains(&d.name.as_str()) && !not_rider.contains(&d.name.as_str()),
                );
                if !is_tack && !is_rider {
                    continue;
                }
                for p in d.positions.iter().step_by(3) {
                    let s = signed_distance(*p, &skin, &tris);
                    // The nearest triangle's side only tells inside from
                    // outside near the skin (far off, a thin ear's back
                    // can be nearest).
                    if s.abs() > 0.1 {
                        continue;
                    }
                    let worst = if is_tack { &mut tack } else { &mut rider };
                    *worst = worst.min(s);
                }
            }
            let world = app.world_mut();
            let mut straps = world.query::<(&Strap, &HorsePiece)>();
            for (strap, piece) in straps.iter(world) {
                let at = |(e, p): (Entity, Vec3)| {
                    world.get::<GlobalTransform>(e).unwrap().transform_point(p)
                };
                if crosses(at(strap.ends[0]), at(strap.ends[1]), &skin, &tris) {
                    let n = |e: Entity| world.get::<Name>(e).map(|n| n.to_string());
                    cuts.push(format!(
                        "{piece:?} {:?}-{:?} {:.2} {:.2}",
                        n(strap.ends[0].0),
                        n(strap.ends[1].0),
                        at(strap.ends[0]),
                        at(strap.ends[1])
                    ));
                }
            }
        }
        (tack, rider, cuts)
    }

    #[test]
    fn a_saddled_horse_wears_its_tack_on_its_own_joints() {
        let (mut app, _) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let world = app.world_mut();
        // Every piece of the recado and the bridle, each skinned on the
        // horse's own joints (the skin's), in the props' range.
        let props = crate::settlement::ranges()[1].clone();
        let mut pieces = world.query::<(
            &HorsePiece,
            &ChildOf,
            Option<&SkinnedMesh>,
            Option<&bevy::camera::visibility::VisibilityRange>,
        )>();
        let found: Vec<(HorsePiece, bool)> = pieces
            .iter(world)
            .map(|(p, _, skin, range)| {
                assert!(range == Some(&props), "{p:?}");
                (*p, skin.is_some())
            })
            .collect();
        for piece in [
            HorsePiece::Sudadera,
            HorsePiece::Carona,
            HorsePiece::Basto,
            HorsePiece::Pellon,
            HorsePiece::Cincha,
            HorsePiece::Headstall,
            HorsePiece::Bit,
        ] {
            assert!(found.contains(&(piece, true)), "{piece:?} skinned");
        }
        let count = |p| found.iter().filter(|(f, _)| *f == p).count();
        assert_eq!(count(HorsePiece::Basto), 2);
        assert_eq!(count(HorsePiece::Bit), 2);
        assert_eq!(count(HorsePiece::Stirrup), 2);
        assert_eq!(count(HorsePiece::StirrupLeather), 2);
        assert_eq!(count(HorsePiece::Rein), 6, "three stretches a side");
        let main = world
            .query::<(&SkinnedMesh, &bevy::gltf::GltfMaterialName)>()
            .iter(world)
            .find(|(_, n)| n.0 == "Main")
            .map(|(s, _)| s.joints.clone())
            .unwrap();
        for (piece, _, skin, _) in pieces.iter(world) {
            if let Some(skin) = skin {
                assert_eq!(skin.joints, main, "{piece:?}");
            }
        }
    }

    #[test]
    fn the_tack_clears_the_horse_through_every_gait() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        // Standing, walking, the stretched walk and the top speed: over a
        // whole stride or more each.
        for (speed, frames) in [(0.0, 8), (1.5, 24), (4.5, 12), (12.2, 12)] {
            let (mut app, me) = saddled(rider, speed);
            for _ in 0..30 {
                app.update();
            }
            let (tack, _, cuts) = clearances(&mut app, me, frames);
            println!("{speed} m/s: tack {tack:.4} m off the skin, cuts {cuts:?}");
            assert!(
                tack > 0.002,
                "{speed} m/s: the tack cuts into the horse ({tack})"
            );
            assert!(cuts.is_empty(), "{speed} m/s: {cuts:?}");
        }
    }

    #[test]
    fn a_tied_horse_keeps_its_tack_and_reins_clear_through_every_idle() {
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Overo), 0.0);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let body = app.world().get::<HorseBody>(carrier).unwrap().clone();
        // Each idle once it has faded in.
        let mut checked = Vec::new();
        let mut held = 0;
        let mut last = None;
        for _ in 0..6000 {
            app.update();
            let clock = app.world().resource::<Time>().elapsed_secs();
            let idle = body.idle(clock);
            held = if last == Some(idle) { held + 1 } else { 0 };
            last = Some(idle);
            if held == 30 && !checked.contains(&idle) {
                let (tack, _, cuts) = clearances(&mut app, me, 8);
                assert!(tack > 0.002, "{idle:?}: {tack}");
                assert!(cuts.is_empty(), "{idle:?}: {cuts:?}");
                checked.push(idle);
            }
            if checked.len() == TIED_IDLES.len() {
                break;
            }
        }
        assert_eq!(checked.len(), TIED_IDLES.len(), "{checked:?}");
        // Nobody rides it: the stirrups hang from the saddle, the reins
        // rest on the pommel.
        let tack = *app.world().get::<Tack>(carrier).unwrap();
        for s in 0..2 {
            let hung = app
                .world()
                .get::<ChildOf>(tack.stirrups[s])
                .unwrap()
                .parent();
            assert_eq!(app.world().get::<Name>(hung).unwrap().as_str(), "Torso");
            let rein = app.world().get::<Strap>(tack.reins[s]).unwrap();
            assert_eq!(rein.ends[1].0, hung, "to the pommel");
        }
    }
}
