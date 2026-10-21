//! Low-poly 3D criollo horses, built like the people (see
//! [`figure`](super::figure)): lofted parts hung off a tree of joints, so
//! animation only turns joints (see [`pampa_core::pose::horse_pose`]).
//!
//! A horse is a body entity (turned to the heading) holding its joints:
//! the trunk, the neck, the head and ears, the tail, and three joints down
//! each leg (forearm or gaskin, knee or hock, fetlock). It faces local +X,
//! its right is +Z and its hooves stand on the body's origin, about 1.45 m
//! below the withers: a compact, sturdy Argentine criollo.
//!
//! Coats are painted into per-vertex colours from where each vertex sits
//! on the standing horse, so patches, dapples and the dun's dorsal stripe
//! run across parts. Every horse wears a 1919 recado (a woven sudadera, a
//! leather carona, the bastos, a sheepskin pellón, a rawhide cincha and iron
//! stirrups) and a rawhide bridle with reins. A rider ([`spawn_horse`] with
//! a rider) sits at [`SEAT`].
//!
//! Ambient occlusion is baked into the painted vertices (under the belly,
//! between the legs, under the saddle's edge and the jaw), the head wears a
//! small painted face texture (eyes, nostrils, a star or blaze), and the
//! ears flick and the tail swishes on their own (see [`pampa_core::pose`]).

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use pampa_core::{HorseAngles, HorseGait, Leg, pose};
use std::f32::consts::FRAC_PI_2;

use super::figure::Paints;
use super::finish::{Finish, ramp};

use super::figure::{
    BODY_SIDES, Cloth, Joint, LIMB_SIDES, Rgb, Ring, Side, limb, loft, loft_cloth,
};

/// A horse's coat (pelaje).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coat {
    /// Solid dark brown, black mane and tail, no white.
    Zaino,
    /// Grey with lighter dapples.
    Tordillo,
    /// Bay with large white patches.
    Overo,
    /// Chestnut, the mane and tail a little lighter.
    Alazan,
    /// Dun: sandy yellow with black mane, tail and lower legs and a dark
    /// stripe down the back.
    Bayo,
}

impl Coat {
    pub const ALL: [Self; 5] = [
        Self::Zaino,
        Self::Tordillo,
        Self::Overo,
        Self::Alazan,
        Self::Bayo,
    ];

    /// Don Hidalgo's horse, the one the player rides.
    pub const HIDALGO: Self = Self::Bayo;

    /// A coat picked deterministically from `seed`.
    pub fn seeded(seed: u32) -> Self {
        let mut h = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        Self::ALL[(h % Self::ALL.len() as u32) as usize]
    }

    fn body(self) -> Rgb {
        match self {
            Self::Zaino => [78, 50, 34],
            Self::Tordillo => [118, 118, 114],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [154, 80, 42],
            Self::Bayo => [198, 162, 104],
        }
    }

    fn mane(self) -> Rgb {
        match self {
            Self::Zaino | Self::Bayo => [34, 28, 24],
            Self::Tordillo => [206, 202, 194],
            Self::Overo => [118, 72, 44],
            Self::Alazan => [176, 104, 58],
        }
    }

    /// The lower legs, darker on most coats.
    fn points(self) -> Rgb {
        match self {
            Self::Zaino => [44, 32, 26],
            Self::Tordillo => [92, 92, 90],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [132, 70, 38],
            Self::Bayo => [40, 32, 26],
        }
    }

    /// The head, a little lighter on the grey.
    pub(super) fn head(self) -> Rgb {
        match self {
            Self::Tordillo => [150, 150, 146],
            _ => self.body(),
        }
    }

    pub(super) fn hoof(self) -> Rgb {
        match self {
            Self::Tordillo | Self::Overo => [78, 68, 58],
            _ => [38, 32, 28],
        }
    }

    /// The colour of a coat vertex at `p` on the standing horse, its
    /// surface facing `normal`.
    pub fn color_at(self, p: Vec3, normal: Vec3) -> Rgb {
        const WHITE: Rgb = [232, 228, 218];
        if p.y < POINTS_HEIGHT {
            // Overo stockings on the near fore and the off hind.
            let stocking = (p.x > 0.0 && p.z < 0.0) || (p.x < 0.0 && p.z > 0.0);
            if self == Self::Overo && stocking {
                return WHITE;
            }
            return self.points();
        }
        match self {
            Self::Overo if overo_patch(p) => WHITE,
            Self::Tordillo if dapple(p) => [178, 176, 170],
            // The dun's dorsal stripe, from the withers to the tail.
            Self::Bayo if normal.y > 0.6 && p.z.abs() < 0.05 && (-0.8..0.45).contains(&p.x) => {
                [104, 76, 46]
            }
            _ => self.body(),
        }
    }
}

/// Below this height (m) the legs show the coat's points.
const POINTS_HEIGHT: f32 = 0.5;

/// The overo's white: big blobs on the belly, both flanks, the off side of
/// the croup, both shoulders and both sides of the neck, so some white shows
/// from every side.
fn overo_patch(p: Vec3) -> bool {
    const BLOBS: [([f32; 3], [f32; 3]); 8] = [
        ([0.0, 0.8, 0.0], [0.5, 0.2, 0.4]),
        ([-0.15, 1.08, -0.26], [0.36, 0.26, 0.16]),
        ([0.12, 1.02, 0.26], [0.38, 0.32, 0.18]),
        ([-0.56, 1.18, 0.24], [0.22, 0.22, 0.16]),
        ([0.78, 1.5, -0.12], [0.2, 0.24, 0.12]),
        ([0.52, 1.1, 0.22], [0.2, 0.26, 0.14]),
        ([0.9, 1.62, 0.1], [0.18, 0.2, 0.1]),
        ([0.45, 1.0, -0.22], [0.2, 0.26, 0.14]),
    ];
    BLOBS.iter().any(|(c, r)| {
        let d = (p - Vec3::from(*c)) / Vec3::from(*r);
        d.length_squared() < 1.0
    })
}

/// The tordillo's dapples: a cheap 3D interference pattern.
fn dapple(p: Vec3) -> bool {
    let a = (p.x * 23.0 + (p.z * 17.0).sin()).sin();
    let b = (p.y * 21.0 + (p.x * 13.0).cos()).sin();
    let c = (p.z * 19.0 + p.y * 7.0).sin();
    a * b * c > 0.12
}

/// A pivot of the horse that animation turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorseJoint {
    /// The body: barrel, chest and croup, and the saddle on them.
    Trunk,
    Neck,
    /// The poll, where the head hangs from the neck.
    Head,
    Ear(Side),
    Tail,
    /// The forearm (front) or gaskin (hind) swings here.
    Upper(Leg),
    /// The knee (front) or hock (hind).
    Lower(Leg),
    Fetlock(Leg),
}

/// Where the neck rises from the chest, as a unit direction (forward, up).
const NECK_DIRECTION: Vec2 = Vec2::new(0.659, 0.752);
/// How far up the neck the poll is.
const NECK_LENGTH: f32 = 0.8;
/// The neck's lean forward from upright, radians.
const NECK_TILT: f32 = 0.72;
/// The head's lean back from upright: it hangs down and forward.
const HEAD_TILT: f32 = -0.62;

fn side_of(leg: Leg) -> f32 {
    if leg.is_left() { -1.0 } else { 1.0 }
}

impl HorseJoint {
    /// Every joint, each after its parent.
    pub const ALL: [Self; 18] = [
        Self::Trunk,
        Self::Neck,
        Self::Head,
        Self::Ear(Side::Left),
        Self::Ear(Side::Right),
        Self::Tail,
        Self::Upper(Leg::FrontLeft),
        Self::Upper(Leg::FrontRight),
        Self::Upper(Leg::HindLeft),
        Self::Upper(Leg::HindRight),
        Self::Lower(Leg::FrontLeft),
        Self::Lower(Leg::FrontRight),
        Self::Lower(Leg::HindLeft),
        Self::Lower(Leg::HindRight),
        Self::Fetlock(Leg::FrontLeft),
        Self::Fetlock(Leg::FrontRight),
        Self::Fetlock(Leg::HindLeft),
        Self::Fetlock(Leg::HindRight),
    ];

    /// The joint this one hangs from (`None`: the body).
    pub fn parent(self) -> Option<Self> {
        match self {
            Self::Trunk => None,
            Self::Neck | Self::Tail | Self::Upper(_) => Some(Self::Trunk),
            Self::Head => Some(Self::Neck),
            Self::Ear(_) => Some(Self::Head),
            Self::Lower(leg) => Some(Self::Upper(leg)),
            Self::Fetlock(leg) => Some(Self::Lower(leg)),
        }
    }

    /// Offset from the parent joint (or the hooves) when standing.
    pub fn rest(self) -> Vec3 {
        match self {
            Self::Trunk => Vec3::new(0.0, 1.05, 0.0),
            Self::Neck => Vec3::new(0.52, 0.2, 0.0),
            Self::Head => (NECK_DIRECTION * NECK_LENGTH).extend(0.0),
            Self::Ear(s) => Vec3::new(0.03, 0.04, 0.055 * s.z()),
            Self::Tail => Vec3::new(-0.76, 0.28, 0.0),
            Self::Upper(leg) if leg.is_front() => Vec3::new(0.45, -0.12, 0.15 * side_of(leg)),
            Self::Upper(leg) => Vec3::new(-0.52, -0.06, 0.15 * side_of(leg)),
            Self::Lower(leg) if leg.is_front() => Vec3::new(0.0, -0.45, 0.0),
            Self::Lower(_) => Vec3::new(-0.1, -0.48, 0.0),
            Self::Fetlock(leg) if leg.is_front() => Vec3::new(0.0, -0.31, 0.0),
            Self::Fetlock(_) => Vec3::new(0.0, -0.34, 0.0),
        }
    }

    /// Standing position above the hooves.
    pub fn rest_position(self) -> Vec3 {
        self.rest() + self.parent().map_or(Vec3::ZERO, Self::rest_position)
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|j| *j == self).unwrap_or(0)
    }

    /// This joint's turn in `a`, with the ears flicked back by `ears` (left,
    /// right) and the tail swished aside by `swish`.
    fn rotation(self, a: &HorseAngles, ears: [f32; 2], swish: f32) -> Quat {
        // Positive Z turns a hanging limb toward +X (forward) and lifts the
        // nose.
        let angle = match self {
            Self::Trunk => -a.pitch,
            Self::Neck => -a.neck,
            Self::Head => -a.head,
            Self::Ear(s) => ears[usize::from(s == Side::Right)],
            Self::Tail => return Quat::from_rotation_x(-swish) * Quat::from_rotation_z(-a.tail),
            Self::Upper(leg) => a.upper[leg.index()],
            Self::Lower(leg) => a.lower[leg.index()],
            Self::Fetlock(leg) => a.fetlock[leg.index()],
        };
        Quat::from_rotation_z(angle)
    }
}

/// The shared unit meshes horse parts are scaled copies of. Lofts fit a 1 m
/// box centered on the origin, so a part's size is its bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorseShape {
    /// The barrel, along +X: deep and round.
    Barrel,
    /// The chest and shoulders, rounding off forward.
    Chest,
    /// The hindquarters, rounding off at the buttocks.
    Croup,
    /// The neck: deep at its base, flattened sideways, narrow at the throat.
    Neck,
    /// The head along its length, muzzle at the bottom, jowls at the top.
    Head,
    Ear,
    /// Hair with hard strands: the mane and the forelock.
    Mane,
    /// The tail's hair, full and tapering.
    Tail,
    /// Tapered upper legs: forearm, gaskin and the dock of the tail.
    Forearm,
    /// The slim cannon and pastern.
    Cannon,
    Hoof,
    /// Rounded lumps: withers, shoulders, thighs, knees, hocks, fetlocks,
    /// muzzle and jowls.
    Lump,
    /// Tiny bumps: eyes and nostrils.
    Bump,
    /// A thin ring round a part: the cincha and the bridle's straps.
    Band,
    /// The woven sudadera under the saddle: a thin shell over the back.
    Blanket,
    /// The leather carona over it, a little stiffer and narrower.
    Carona,
    /// The sheepskin pellón on top: thick, with a shaggy surface.
    Fleece,
    /// One of the bastos, the padded rolls of the recado, along +X.
    Roll,
    /// A leather or rawhide strap along +Y: stirrup leathers, reins and
    /// cheekpieces.
    Strap,
    /// An iron ring: cincha rings, bit rings and the stirrups.
    Ring,
}

impl HorseShape {
    pub const ALL: [Self; 20] = [
        Self::Barrel,
        Self::Chest,
        Self::Croup,
        Self::Neck,
        Self::Head,
        Self::Ear,
        Self::Mane,
        Self::Tail,
        Self::Forearm,
        Self::Cannon,
        Self::Hoof,
        Self::Lump,
        Self::Bump,
        Self::Band,
        Self::Blanket,
        Self::Carona,
        Self::Fleece,
        Self::Roll,
        Self::Strap,
        Self::Ring,
    ];

    /// Builds this unit mesh.
    pub fn mesh(self) -> Mesh {
        let along_x = Quat::from_rotation_z(-FRAC_PI_2);
        let sphere = Sphere::new(0.5);
        let r = |y, rx, rz| Ring(y, rx, rz, 0.0, 1.0);
        match self {
            // Lofted rump to chest along Y, then laid along +X; a ring's X
            // half extent becomes the height.
            Self::Barrel => loft(
                &[
                    Ring(-0.5, 0.4, 0.42, 0.0, 0.92),
                    r(-0.4, 0.46, 0.47),
                    r(-0.25, 0.49, 0.5),
                    r(-0.05, 0.5, 0.5),
                    r(0.15, 0.5, 0.49),
                    r(0.32, 0.48, 0.46),
                    r(0.45, 0.43, 0.4),
                    Ring(0.5, 0.36, 0.33, 0.0, 0.95),
                ],
                BODY_SIDES,
            )
            .rotated_by(along_x),
            Self::Chest => loft(
                &[
                    r(-0.5, 0.42, 0.42),
                    r(-0.2, 0.5, 0.5),
                    r(0.15, 0.48, 0.47),
                    r(0.35, 0.4, 0.4),
                    r(0.47, 0.26, 0.28),
                    Ring(0.5, 0.1, 0.12, 0.0, 0.95),
                ],
                BODY_SIDES,
            )
            .rotated_by(along_x),
            Self::Croup => loft(
                &[
                    Ring(-0.5, 0.12, 0.12, 0.0, 0.9),
                    r(-0.44, 0.3, 0.32),
                    r(-0.3, 0.44, 0.46),
                    r(-0.1, 0.5, 0.5),
                    r(0.2, 0.48, 0.48),
                    r(0.5, 0.42, 0.44),
                ],
                BODY_SIDES,
            )
            .rotated_by(along_x),
            Self::Neck => loft(
                &[
                    r(-0.5, 0.42, 0.45),
                    r(-0.35, 0.5, 0.5),
                    Ring(-0.1, 0.44, 0.42, -0.03, 1.0),
                    Ring(0.15, 0.36, 0.36, -0.03, 1.0),
                    r(0.35, 0.32, 0.33),
                    Ring(0.47, 0.3, 0.3, 0.0, 0.85),
                    Ring(0.5, 0.2, 0.2, 0.0, 0.8),
                ],
                LIMB_SIDES,
            ),
            // Muzzle (bottom) to poll (top); +X is the face, -X the jaw.
            Self::Head => loft(
                &[
                    Ring(-0.5, 0.22, 0.28, 0.08, 0.92),
                    r(-0.42, 0.34, 0.38),
                    Ring(-0.25, 0.3, 0.34, 0.08, 1.0),
                    Ring(0.0, 0.33, 0.38, 0.06, 1.0),
                    Ring(0.18, 0.4, 0.46, -0.04, 1.0),
                    Ring(0.32, 0.42, 0.5, -0.08, 0.95),
                    Ring(0.44, 0.34, 0.42, -0.06, 0.95),
                    Ring(0.5, 0.2, 0.26, -0.05, 0.9),
                ],
                LIMB_SIDES,
            ),
            Self::Ear => loft(
                &[
                    Ring(-0.5, 0.3, 0.4, 0.0, 0.8),
                    r(-0.3, 0.5, 0.5),
                    r(0.1, 0.42, 0.4),
                    r(0.4, 0.2, 0.2),
                    r(0.5, 0.05, 0.05),
                ],
                8,
            ),
            Self::Mane => loft_cloth(
                &[
                    Ring(-0.5, 0.3, 0.4, 0.0, 0.9),
                    r(-0.4, 0.5, 0.5),
                    r(0.4, 0.5, 0.5),
                    Ring(0.5, 0.3, 0.4, 0.0, 0.9),
                ],
                10,
                Cloth {
                    folds: 5,
                    depth: 0.3,
                    scuff: 0.1,
                },
            )
            .with_duplicated_vertices()
            .with_computed_flat_normals(),
            Self::Tail => loft_cloth(
                &[
                    Ring(-0.5, 0.25, 0.3, 0.0, 0.85),
                    r(-0.4, 0.45, 0.45),
                    r(-0.1, 0.5, 0.5),
                    r(0.2, 0.4, 0.42),
                    r(0.4, 0.3, 0.32),
                    r(0.5, 0.18, 0.2),
                ],
                12,
                Cloth {
                    folds: 6,
                    depth: 0.3,
                    scuff: 0.1,
                },
            ),
            Self::Forearm => loft(&limb(0.5, 0.3)[1..], LIMB_SIDES),
            Self::Cannon => loft(
                &[
                    Ring(-0.5, 0.25, 0.25, 0.0, 0.9),
                    r(-0.42, 0.5, 0.5),
                    r(-0.1, 0.42, 0.44),
                    r(0.3, 0.44, 0.46),
                    r(0.42, 0.5, 0.5),
                    r(0.5, 0.25, 0.25),
                ],
                10,
            ),
            Self::Hoof => loft(
                &[
                    Ring(-0.5, 0.38, 0.4, 0.05, 0.8),
                    Ring(-0.45, 0.45, 0.48, 0.05, 0.9),
                    Ring(-0.2, 0.42, 0.45, 0.02, 1.0),
                    Ring(0.3, 0.36, 0.38, -0.06, 1.0),
                    Ring(0.5, 0.3, 0.32, -0.1, 1.0),
                ],
                12,
            ),
            Self::Lump => sphere
                .mesh()
                .ico(1)
                .unwrap_or_else(|_| sphere.mesh().uv(10, 6)),
            Self::Bump => sphere.mesh().uv(8, 5),
            Self::Band => Cylinder::new(0.5, 1.0).mesh().resolution(14).build(),
            Self::Blanket => arch(1.2, 0.06, 0.0, 12, 4),
            Self::Carona => arch(1.05, 0.08, 0.0, 12, 4),
            Self::Fleece => arch(0.95, 0.15, 0.035, 14, 6),
            Self::Roll => loft(
                &[
                    Ring(-0.5, 0.3, 0.35, 0.0, 0.9),
                    r(-0.4, 0.5, 0.5),
                    r(0.4, 0.5, 0.5),
                    Ring(0.5, 0.3, 0.35, 0.0, 0.9),
                ],
                10,
            )
            .rotated_by(along_x),
            Self::Strap => loft(&[r(-0.5, 0.5, 0.5), r(0.5, 0.5, 0.5)], 6)
                .with_duplicated_vertices()
                .with_computed_flat_normals(),
            Self::Ring => Torus::new(0.35, 0.5)
                .mesh()
                .minor_resolution(4)
                .major_resolution(8)
                .build(),
        }
    }
}

/// A saddle layer: a shell over the top of a cylinder along X of radius
/// 0.5, `span` radians either side of the top, `thickness` deep, with hard
/// edges. `shag` roughens its outer face like a sheepskin.
fn arch(span: f32, thickness: f32, shag: f32, around: usize, along: usize) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    let inner = 0.5 - thickness;
    let mut positions = Vec::new();
    // Outer face, then inner face: (along + 1) x (around + 1) points each.
    for radius in [0.5, inner] {
        for i in 0..=along {
            let x = i as f32 / along as f32 - 0.5;
            for k in 0..=around {
                let a = span * (2.0 * k as f32 / around as f32 - 1.0);
                let tuft = if radius == 0.5 {
                    ((i * 7 + k * 13) % 5) as f32 / 4.0
                } else {
                    0.0
                };
                let r = radius * (1.0 - shag * tuft);
                positions.push([x, r * a.cos(), r * a.sin()]);
            }
        }
    }
    let row = around + 1;
    let at = |face: usize, i: usize, k: usize| (face * (along + 1) * row + i * row + k) as u32;
    let mut indices: Vec<u32> = Vec::new();
    for i in 0..along {
        for k in 0..around {
            let (a, b, c, d) = (
                at(0, i, k),
                at(0, i, k + 1),
                at(0, i + 1, k),
                at(0, i + 1, k + 1),
            );
            indices.extend([a, b, c, b, d, c]);
            let (a, b, c, d) = (
                at(1, i, k),
                at(1, i, k + 1),
                at(1, i + 1, k),
                at(1, i + 1, k + 1),
            );
            indices.extend([a, c, b, b, c, d]);
        }
    }
    // The ends (front and back) and the two lower edges close the shell.
    for k in 0..around {
        let (o, o1, n, n1) = (at(0, 0, k), at(0, 0, k + 1), at(1, 0, k), at(1, 0, k + 1));
        indices.extend([o, n, o1, o1, n, n1]);
        let (o, o1, n, n1) = (
            at(0, along, k),
            at(0, along, k + 1),
            at(1, along, k),
            at(1, along, k + 1),
        );
        indices.extend([o, o1, n, o1, n1, n]);
    }
    for i in 0..along {
        let (o, o1, n, n1) = (
            at(0, i, around),
            at(0, i + 1, around),
            at(1, i, around),
            at(1, i + 1, around),
        );
        indices.extend([o, n, o1, o1, n, n1]);
        let (o, o1, n, n1) = (at(0, i, 0), at(0, i + 1, 0), at(1, i, 0), at(1, i + 1, 0));
        indices.extend([o, o1, n, o1, n1, n]);
    }
    let colors = vec![[1.0_f32; 4]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}

/// What a horse part is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorsePiece {
    Barrel,
    Chest,
    Withers,
    Croup,
    Neck,
    Mane,
    Forelock,
    Head,
    Muzzle,
    Jowl,
    Ear,
    Eye,
    Nostril,
    /// The fleshy root of the tail, and its hair.
    Dock,
    Tail,
    Shoulder,
    Forearm,
    Thigh,
    Gaskin,
    Knee,
    Hock,
    Cannon,
    Fetlock,
    Pastern,
    Hoof,
    // The recado and the bridle.
    /// The woven blanket under the saddle.
    Sudadera,
    /// The leather layer over it.
    Carona,
    /// The two padded rolls the rider sits between.
    Basto,
    /// The sheepskin on top.
    Pellon,
    /// The rawhide girth round the belly, and its iron rings.
    Cincha,
    CinchaRing,
    StirrupLeather,
    Stirrup,
    /// The rawhide headstall: crownpiece, noseband and cheekpieces.
    Headstall,
    Bit,
    Rein,
}

/// How a horse part is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paint {
    /// The coat's pattern, painted per vertex.
    Coat,
    /// The coat's mane and tail colour, per vertex.
    Mane,
    /// The coat's hoof colour.
    Hoof,
    /// A fixed colour (eyes, tack).
    Flat(Rgb),
    /// The coat's face texture (eyes, nostrils, markings) over shades.
    Face,
}

/// One part of a horse at rest: a scaled, turned unit [`HorseShape`]
/// centered at `center` relative to its joint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HorsePart {
    pub piece: HorsePiece,
    pub joint: HorseJoint,
    pub shape: HorseShape,
    pub paint: Paint,
    pub center: Vec3,
    pub size: Vec3,
    pub rotation: Quat,
}

impl HorsePart {
    /// What it is made of, on a horse of `coat`.
    pub fn finish(&self, coat: Coat) -> Finish {
        use HorsePiece::*;
        match (self.paint, self.piece) {
            (Paint::Coat, _) => Finish::Coat,
            (Paint::Mane, _) => Finish::Hair,
            (Paint::Hoof, _) => Finish::Hoof,
            (Paint::Face, _) => Finish::HorseFace(coat),
            (_, Eye) => Finish::Eye,
            (_, Sudadera) => Finish::Wool,
            (_, Pellon) => Finish::Fleece,
            (_, Carona | Basto | StirrupLeather) => Finish::Leather,
            (_, Cincha | Headstall | Rein) => Finish::Rawhide,
            (_, CinchaRing | Stirrup | Bit) => Finish::Iron,
            _ => Finish::Skin,
        }
    }

    /// From the unit mesh to the standing horse.
    pub fn rest_transform(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            self.size,
            self.rotation,
            self.joint.rest_position() + self.center,
        )
    }
}

/// A turn that leans a part's +Y axis forward by `angle` (toward +X).
fn tilt(angle: f32) -> Quat {
    Quat::from_rotation_z(-angle)
}

/// Down the head from the poll to the muzzle.
fn down_head() -> Vec3 {
    tilt(HEAD_TILT) * Vec3::NEG_Y
}

/// Out of the face (forward and up from the head's axis).
fn face() -> Vec3 {
    tilt(HEAD_TILT) * Vec3::X
}

/// Where a rider sits, on the standing horse: the top of the pellón.
pub const SEAT: Vec3 = Vec3::new(-0.05, 1.49, 0.0);

/// How far below the pelvis joint a rider's hips reach.
const HIP_DROP: f32 = 0.11;

/// Where a rider's figure body stands relative to the horse's trunk joint,
/// so its hips rest on [`SEAT`].
pub fn rider_offset() -> Vec3 {
    let pelvis = Joint::Pelvis.rest_position().y;
    Vec3::new(SEAT.x, SEAT.y + HIP_DROP - pelvis, SEAT.z) - HorseJoint::Trunk.rest_position()
}

const EYE: Rgb = [24, 18, 16];
const NOSTRIL: Rgb = [30, 22, 20];
/// A woven jerga, rust red with use.
const JERGA: Rgb = [124, 56, 42];
const CARONA: Rgb = [88, 60, 40];
const BASTO: Rgb = [64, 44, 30];
const PELLON: Rgb = [208, 196, 170];
const RAWHIDE: Rgb = [166, 140, 104];
const IRON: Rgb = [70, 68, 66];

/// Every part of the horse, standing, without tack. The same for every
/// coat: the coat only changes the paint. Deterministic.
#[rustfmt::skip]
pub fn horse_parts() -> Vec<HorsePart> {
    use HorseJoint as J;
    use HorsePiece::*;
    use HorseShape as S;
    let mut parts = Vec::with_capacity(64);
    let mut add = |piece, joint, shape, paint, center: Vec3, size: [f32; 3], rotation| {
        parts.push(HorsePart { piece, joint, shape, paint, center, size: Vec3::from(size), rotation });
    };
    let id = Quat::IDENTITY;
    let v = Vec3::new;

    add(Barrel, J::Trunk, S::Barrel, Paint::Coat, Vec3::ZERO, [1.25, 0.62, 0.52], id);
    add(Chest, J::Trunk, S::Chest, Paint::Coat, v(0.5, 0.02, 0.0), [0.44, 0.62, 0.46], id);
    add(Withers, J::Trunk, S::Lump, Paint::Coat, v(0.33, 0.32, 0.0), [0.42, 0.16, 0.22], id);
    add(Croup, J::Trunk, S::Croup, Paint::Coat, v(-0.48, 0.06, 0.0), [0.6, 0.66, 0.54], id);

    // The neck rises forward from the chest; the mane runs along its crest.
    let neck = tilt(NECK_TILT);
    let mid = (NECK_DIRECTION * 0.4).extend(0.0);
    add(Neck, J::Neck, S::Neck, Paint::Coat, mid, [0.42, 0.86, 0.3], neck);
    let crest = neck * Vec3::NEG_X;
    add(Mane, J::Neck, S::Mane, Paint::Mane, mid + crest * 0.16 + (NECK_DIRECTION * 0.03).extend(0.0), [0.1, 0.78, 0.06], neck);

    // The head hangs from the poll, the muzzle forward and down.
    let head = tilt(HEAD_TILT);
    let (down, out) = (down_head(), face());
    add(Head, J::Head, S::Head, Paint::Face, down * 0.27, [0.26, 0.58, 0.2], head);
    add(Muzzle, J::Head, S::Lump, Paint::Coat, down * 0.52 + out * 0.01, [0.15, 0.13, 0.135], head);
    add(Jowl, J::Head, S::Lump, Paint::Coat, down * 0.14 - out * 0.06, [0.17, 0.16, 0.19], head);
    add(Forelock, J::Head, S::Mane, Paint::Mane, v(0.06, 0.01, 0.0), [0.05, 0.13, 0.07], tilt(1.2));
    for z in [-1.0, 1.0] {
        add(Eye, J::Head, S::Bump, Paint::Flat(EYE), down * 0.13 + out * 0.035 + v(0.0, 0.0, 0.098 * z), [0.045, 0.035, 0.025], head);
        add(Nostril, J::Head, S::Bump, Paint::Flat(NOSTRIL), down * 0.5 + out * 0.065 + v(0.0, 0.0, 0.04 * z), [0.035, 0.045, 0.022], head);
    }
    for s in Side::BOTH {
        add(Ear, J::Ear(s), S::Ear, Paint::Coat, v(0.0, 0.07, 0.0), [0.055, 0.14, 0.035], Quat::from_rotation_x(-0.2 * s.z()));
    }

    add(Dock, J::Tail, S::Forearm, Paint::Mane, v(-0.04, -0.1, 0.0), [0.1, 0.26, 0.09], tilt(0.35));
    add(Tail, J::Tail, S::Tail, Paint::Mane, v(-0.1, -0.45, 0.0), [0.2, 0.72, 0.13], tilt(0.12));

    for leg in Leg::ALL {
        if leg.is_front() {
            add(Shoulder, J::Upper(leg), S::Lump, Paint::Coat, v(0.02, 0.12, 0.0), [0.26, 0.42, 0.2], id);
            add(Forearm, J::Upper(leg), S::Forearm, Paint::Coat, v(0.0, -0.2, 0.0), [0.15, 0.52, 0.14], id);
            add(Knee, J::Lower(leg), S::Lump, Paint::Coat, v(0.005, 0.0, 0.0), [0.11, 0.11, 0.1], id);
            add(Cannon, J::Lower(leg), S::Cannon, Paint::Coat, v(0.0, -0.15, 0.0), [0.085, 0.36, 0.08], id);
        } else {
            add(Thigh, J::Upper(leg), S::Lump, Paint::Coat, v(0.0, 0.1, 0.0), [0.36, 0.5, 0.24], id);
            add(Gaskin, J::Upper(leg), S::Forearm, Paint::Coat, v(-0.05, -0.24, 0.0), [0.15, 0.46, 0.13], tilt(0.21));
            add(Hock, J::Lower(leg), S::Lump, Paint::Coat, v(-0.015, 0.0, 0.0), [0.12, 0.11, 0.1], id);
            add(Cannon, J::Lower(leg), S::Cannon, Paint::Coat, v(0.0, -0.16, 0.0), [0.085, 0.38, 0.08], id);
        }
        add(Fetlock, J::Fetlock(leg), S::Lump, Paint::Coat, Vec3::ZERO, [0.1, 0.09, 0.095], id);
        add(Pastern, J::Fetlock(leg), S::Cannon, Paint::Coat, v(0.025, -0.06, 0.0), [0.07, 0.12, 0.068], tilt(-0.35));
        add(Hoof, J::Fetlock(leg), S::Hoof, Paint::Hoof, v(0.04, -0.12, 0.0), [0.13, 0.1, 0.12], id);
    }
    parts
}

/// A strap from `a` to `b` (joint space), `thickness` across.
fn strap(a: Vec3, b: Vec3, thickness: f32) -> (Vec3, [f32; 3], Quat) {
    let along = b - a;
    let rotation = Quat::from_rotation_arc(Vec3::Y, along.normalize_or(Vec3::Y));
    (
        (a + b) / 2.0,
        [thickness, along.length(), thickness],
        rotation,
    )
}

/// Where a mounted rider's hands hold the reins, on the standing horse.
const HANDS: Vec3 = Vec3::new(0.38, 1.82, 0.12);
/// Where the reins rest on a horse without a rider: over the pommel.
const POMMEL: Vec3 = Vec3::new(0.22, 1.42, 0.13);

/// The recado, the bridle and the reins. With a rider (`mounted`) the
/// stirrups hang where its feet are and the reins run to its hands; tied up,
/// the stirrups hang straight and the reins lie over the pommel.
#[rustfmt::skip]
pub fn tack_parts(mounted: bool) -> Vec<HorsePart> {
    use HorseJoint as J;
    use HorsePiece::*;
    use HorseShape as S;
    let mut parts = Vec::with_capacity(24);
    let mut add = |piece, joint, shape, color, (center, size, rotation): (Vec3, [f32; 3], Quat)| {
        parts.push(HorsePart { piece, joint, shape, paint: Paint::Flat(color), center, size: Vec3::from(size), rotation });
    };
    let id = Quat::IDENTITY;
    let v = Vec3::new;
    let trunk = HorseJoint::Trunk.rest_position();
    let saddle = SEAT.x;

    // The recado, layer by layer over the back.
    add(Sudadera, J::Trunk, S::Blanket, JERGA, (v(saddle, 0.0, 0.0), [0.74, 0.665, 0.56], id));
    add(Carona, J::Trunk, S::Carona, CARONA, (v(saddle, 0.0, 0.0), [0.64, 0.69, 0.585], id));
    for z in [-0.085, 0.085] {
        add(Basto, J::Trunk, S::Roll, BASTO, (v(saddle, 0.37, z), [0.56, 0.08, 0.09], id));
    }
    let pellon_top = SEAT.y - trunk.y;
    add(Pellon, J::Trunk, S::Fleece, PELLON, (v(saddle, pellon_top - 0.69, 0.0), [0.58, 1.38, 0.74], id));
    add(Cincha, J::Trunk, S::Band, RAWHIDE, (v(0.2, 0.0, 0.0), [0.64, 0.09, 0.545], tilt(FRAC_PI_2)));
    for z in [-1.0, 1.0] {
        add(CinchaRing, J::Trunk, S::Ring, IRON, (v(0.2, -0.05, 0.275 * z), [0.09, 0.1, 0.09], Quat::from_rotation_x(FRAC_PI_2)));
        // Stirrup leathers from under the bastos down to the stirrups.
        let top = v(saddle + 0.15, 0.37, 0.27 * z);
        let foot = if mounted { v(saddle + 0.29, -0.01, 0.31 * z) } else { v(saddle + 0.15, -0.04, 0.29 * z) };
        add(StirrupLeather, J::Trunk, S::Strap, BASTO, strap(top, foot, 0.025));
        add(Stirrup, J::Trunk, S::Ring, IRON, (foot - v(0.0, 0.065, 0.0), [0.13, 0.2, 0.11], tilt(FRAC_PI_2)));
    }

    // The rawhide bridle: crownpiece behind the ears, noseband, cheekpieces
    // and the bit's rings at the corners of the mouth.
    let head = tilt(HEAD_TILT);
    let (down, out) = (down_head(), face());
    add(Headstall, J::Head, S::Band, RAWHIDE, (down * 0.06 - out * 0.018, [0.21, 0.024, 0.2], head));
    add(Headstall, J::Head, S::Band, RAWHIDE, (down * 0.38 + out * 0.018, [0.175, 0.024, 0.155], head));
    let bit = down * 0.46 - out * 0.03;
    for z in [-1.0, 1.0] {
        add(Headstall, J::Head, S::Strap, RAWHIDE, strap(down * 0.08 + v(0.0, 0.0, 0.1 * z), bit + v(0.0, 0.0, 0.08 * z), 0.016));
        add(Bit, J::Head, S::Ring, IRON, (bit + v(0.0, 0.0, 0.08 * z), [0.05, 0.1, 0.05], Quat::from_rotation_x(FRAC_PI_2)));
        // The reins, on the neck: from the bit to the hands or the pommel.
        let neck = HorseJoint::Neck.rest_position();
        let from = HorseJoint::Head.rest() + bit + v(0.0, 0.0, 0.09 * z);
        let to = if mounted { HANDS } else { POMMEL };
        let to = v(to.x, to.y, to.z * z) - neck;
        add(Rein, J::Neck, S::Strap, RAWHIDE, strap(from, to, 0.014));
    }
    parts
}

/// The height of the withers above the hooves.
pub fn withers_height() -> f32 {
    horse_parts()
        .iter()
        .filter(|p| p.piece == HorsePiece::Withers)
        .map(|p| p.joint.rest_position().y + p.center.y + p.size.y / 2.0)
        .fold(0.0, f32::max)
}

/// A colour's linear channels, for vertex colours.
fn linear(color: Rgb) -> [f32; 3] {
    let c = Color::srgb_u8(color[0], color[1], color[2]).to_linear();
    [c.red, c.green, c.blue]
}

/// How much ambient light reaches a vertex of `piece` at `p` on the standing
/// horse, facing `n`: less under the belly, between the legs, just under the
/// saddle's edge and under the jaw.
pub fn occlusion(piece: HorsePiece, p: Vec3, n: Vec3) -> f32 {
    use HorsePiece::*;
    let mut light = 1.0;
    if matches!(piece, Barrel | Chest | Croup) {
        light -= 0.35 * ramp(-n.y, 0.2, 0.9) * (1.0 - ramp(p.y, 0.85, 1.05));
    }
    if matches!(
        piece,
        Shoulder | Forearm | Thigh | Gaskin | Knee | Hock | Cannon | Fetlock | Pastern
    ) {
        let inward = ramp(-n.z * p.z.signum(), 0.2, 0.9);
        light -= 0.3 * inward * (0.5 + 0.5 * ramp(p.y, 0.3, 0.9));
    }
    if matches!(piece, Barrel | Chest) {
        let along = ramp(p.x, -0.5, -0.4) * (1.0 - ramp(p.x, 0.3, 0.4));
        let edge = 1.0 - ramp((p.y - 1.16).abs(), 0.0, 0.06);
        light -= 0.22 * along * edge * ramp(n.z.abs(), 0.3, 0.8);
    }
    if matches!(piece, Head | Jowl | Neck) {
        light -= 0.25 * ramp(-n.y, 0.4, 0.95);
    }
    light
}

/// `part`'s unit mesh with `coat` painted into its vertex colours (times
/// the shade already there) and its ambient occlusion baked in, from where
/// each vertex sits on the standing horse.
pub fn painted_mesh(part: &HorsePart, coat: Coat) -> Mesh {
    use bevy::mesh::VertexAttributeValues as V;
    let mut mesh = part.shape.mesh();
    let place = part.rest_transform();
    let turn = Mat3::from_quat(part.rotation) * Mat3::from_diagonal(part.size.recip());
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(V::Float32x3(p)) => p.clone(),
        _ => return mesh,
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(V::Float32x3(n)) => n.clone(),
        _ => vec![[0.0, 1.0, 0.0]; positions.len()],
    };
    let shades = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(V::Float32x4(c)) => c.clone(),
        _ => vec![[1.0; 4]; positions.len()],
    };
    let colors: Vec<[f32; 4]> = positions
        .iter()
        .zip(&normals)
        .zip(&shades)
        .map(|((p, n), shade)| {
            let at = place.transform_point3(Vec3::from(*p));
            let normal = (turn * Vec3::from(*n)).normalize_or_zero();
            let color = match part.paint {
                Paint::Coat => coat.color_at(at, normal),
                Paint::Mane => coat.mane(),
                Paint::Hoof => coat.hoof(),
                Paint::Flat(c) => c,
                // The colour is in the face texture.
                Paint::Face => [255; 3],
            };
            let light = occlusion(part.piece, at, normal);
            let [r, g, b] = linear(color).map(|c| c * light);
            [shade[0] * r, shade[1] * g, shade[2] * b, 1.0]
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

/// Unit meshes for flat parts and painted meshes per coat (built when a
/// coat is first drawn), shared by every horse; materials come from the
/// [`Palette`](super::finish::Palette).
#[derive(Resource, Default)]
pub struct HorseKit {
    units: HashMap<HorseShape, Handle<Mesh>>,
    /// By coat and index into [`horse_parts`].
    painted: HashMap<(Coat, usize), Handle<Mesh>>,
}

impl HorseKit {
    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            units: HorseShape::ALL.map(|s| (s, meshes.add(s.mesh()))).into(),
            ..default()
        }
    }

    /// The mesh and material of body part `index` (or of a tack part, with
    /// no index) in `coat`.
    fn draw(
        &mut self,
        part: &HorsePart,
        index: Option<usize>,
        coat: Coat,
        meshes: &mut Assets<Mesh>,
        (palette, materials, images): Paints,
    ) -> Option<(Handle<Mesh>, Handle<StandardMaterial>)> {
        let finish = part.finish(coat);
        let (mesh, color) = match (part.paint, index) {
            (Paint::Coat | Paint::Mane | Paint::Face, Some(index)) => {
                let mesh = self
                    .painted
                    .entry((coat, index))
                    .or_insert_with(|| meshes.add(painted_mesh(part, coat)))
                    .clone();
                // White: the colour is in the vertices or the face texture.
                (mesh, [255; 3])
            }
            (Paint::Hoof, _) => (self.units.get(&part.shape)?.clone(), coat.hoof()),
            (Paint::Flat(color), _) => (self.units.get(&part.shape)?.clone(), color),
            _ => return None,
        };
        Some((mesh, palette.material(color, finish, materials, images)))
    }
}

/// The turning body of a horse, child of the character entity.
#[derive(Component, Debug, Clone)]
pub struct HorseBody {
    pub coat: Coat,
    /// The gait drawn: logic's animation and the speed decide it.
    pub gait: HorseGait,
    /// How far through the stride (or breath) cycle, in cycles (`0..1`).
    pub phase: f32,
    /// Seconds since it was drawn, and its own seed, for ear flicks and
    /// tail swishes.
    pub clock: f32,
    pub seed: u32,
    /// Joint entities in [`HorseJoint::ALL`] order.
    joints: Vec<Entity>,
}

/// A joint entity of a horse.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorseJointPart(pub HorseJoint);

/// A part mesh of a horse or its tack.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorsePartMesh(pub HorsePiece);

/// Spawns a saddled horse in `coat` under `parent`, tacked for a rider when
/// `mounted`; returns its body and its trunk joint (where a rider sits).
pub(super) fn spawn_horse(
    commands: &mut Commands,
    (kit, meshes): (&mut HorseKit, &mut Assets<Mesh>),
    (palette, materials, images): Paints,
    parent: Entity,
    coat: Coat,
    phase: f32,
    mounted: bool,
) -> (Entity, Entity) {
    let body = commands
        .spawn((
            Name::new("Horse"),
            Transform::default(),
            Visibility::default(),
            ChildOf(parent),
        ))
        .id();
    let mut joints: Vec<Entity> = Vec::with_capacity(HorseJoint::ALL.len());
    for joint in HorseJoint::ALL {
        let holder = joint.parent().map_or(body, |p| joints[p.index()]);
        joints.push(
            commands
                .spawn((
                    HorseJointPart(joint),
                    Transform::from_translation(joint.rest()),
                    Visibility::default(),
                    ChildOf(holder),
                ))
                .id(),
        );
    }
    let body_parts = horse_parts()
        .into_iter()
        .enumerate()
        .map(|(i, p)| (p, Some(i)));
    let tack = tack_parts(mounted).into_iter().map(|p| (p, None));
    for (part, index) in body_parts.chain(tack) {
        let paints = (&mut *palette, &mut *materials, &mut *images);
        let Some((mesh, material)) = kit.draw(&part, index, coat, meshes, paints) else {
            continue;
        };
        commands.spawn((
            HorsePartMesh(part.piece),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(part.center)
                .with_rotation(part.rotation)
                .with_scale(part.size),
            ChildOf(joints[part.joint.index()]),
        ));
    }
    let trunk = joints[HorseJoint::Trunk.index()];
    commands.entity(body).insert(HorseBody {
        coat,
        gait: HorseGait::Idle,
        phase,
        clock: 0.0,
        seed: parent.index_u32(),
        joints,
    });
    (body, trunk)
}

/// Turns, bobs and poses each horse from its character's pose: the gait
/// comes from logic's animation and speed, the stride is paced by speed.
pub(super) fn update_horses(
    time: Res<Time>,
    poses: Query<&super::VisualPose>,
    mut bodies: Query<(&ChildOf, &mut HorseBody, &mut Transform), Without<HorseJointPart>>,
    mut joints: Query<(&HorseJointPart, &mut Transform), Without<HorseBody>>,
) {
    let dt = time.delta_secs();
    for (child_of, mut body, mut transform) in &mut bodies {
        let Ok(visual) = poses.get(child_of.parent()) else {
            continue;
        };
        body.gait = HorseGait::of(visual.animation, visual.speed);
        body.phase = pose::horse_advance(body.phase, body.gait, visual.speed, dt);
        let angles = pose::horse_pose(body.gait, body.phase);
        body.clock += dt;
        let ears = pose::ear_flick(body.seed, body.clock);
        let swish = pose::tail_swish(body.seed, body.clock, body.gait, body.phase);
        transform.rotation = super::figure::body_rotation(visual.heading);
        transform.translation.y = angles.bob;
        for &entity in &body.joints {
            let Ok((joint, mut transform)) = joints.get_mut(entity) else {
                continue;
            };
            let breath = if joint.0 == HorseJoint::Trunk {
                angles.breath
            } else {
                0.0
            };
            transform.translation = joint.0.rest() + Vec3::Y * breath;
            transform.rotation = joint.0.rotation(&angles, ears, swish);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::figure::{FigureKind, Piece, figure_parts};
    use super::*;

    /// Every coat vertex of the standing horse: position, normal, colour.
    fn coat_vertices(coat: Coat, pieces: &[HorsePiece]) -> Vec<(Vec3, Vec3, Rgb)> {
        horse_parts()
            .iter()
            .filter(|p| pieces.is_empty() || pieces.contains(&p.piece))
            .filter(|p| matches!(p.paint, Paint::Coat | Paint::Mane))
            .flat_map(|part| {
                let mesh = part.shape.mesh();
                let place = part.rest_transform();
                let turn = Mat3::from_quat(part.rotation) * Mat3::from_diagonal(part.size.recip());
                let positions = mesh
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .unwrap()
                    .as_float3()
                    .unwrap()
                    .to_vec();
                let normals = mesh
                    .attribute(Mesh::ATTRIBUTE_NORMAL)
                    .unwrap()
                    .as_float3()
                    .unwrap()
                    .to_vec();
                let part = *part;
                positions.into_iter().zip(normals).map(move |(p, n)| {
                    let at = place.transform_point3(Vec3::from(p));
                    let normal = (turn * Vec3::from(n)).normalize_or_zero();
                    let color = match part.paint {
                        Paint::Mane => coat.mane(),
                        _ => coat.color_at(at, normal),
                    };
                    (at, normal, color)
                })
            })
            .collect()
    }

    fn luma(c: Rgb) -> f32 {
        0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32
    }

    fn triangles(shape: HorseShape) -> usize {
        shape.mesh().triangles().map(|t| t.count()).unwrap_or(0)
    }

    #[test]
    fn a_criollo_stands_about_one_forty_five_at_the_withers_on_its_hooves() {
        let withers = withers_height();
        assert!((1.35..=1.6).contains(&withers), "{withers} m");
        let all = coat_vertices(Coat::Zaino, &[]);
        let top_of_back = all
            .iter()
            .filter(|(p, _, _)| (-0.5..0.3).contains(&p.x))
            .map(|(p, _, _)| p.y)
            .fold(0.0, f32::max);
        assert!(
            top_of_back <= withers + 0.02,
            "the back ({top_of_back}) is below the withers ({withers})"
        );
        // Hooves on the ground.
        let sole = horse_parts()
            .iter()
            .filter(|p| p.piece == HorsePiece::Hoof)
            .map(|p| p.joint.rest_position().y + p.center.y - p.size.y / 2.0)
            .fold(f32::MAX, f32::min);
        assert!(sole.abs() < 1e-4, "{sole}");
        // Compact: about as long from chest to buttock as it is tall.
        let (back, front) = all
            .iter()
            .filter(|(p, _, _)| p.y > 0.8 && p.y < 1.3)
            .fold((f32::MAX, f32::MIN), |(b, f), (p, _, _)| {
                (b.min(p.x), f.max(p.x))
            });
        let length = front - back;
        assert!((1.35..1.75).contains(&length), "{length} m");
        assert_eq!(horse_parts(), horse_parts(), "deterministic");
    }

    #[test]
    fn a_horse_has_ps1_to_ps2_era_detail() {
        let body: usize = horse_parts().iter().map(|p| triangles(p.shape)).sum();
        let tack: usize = tack_parts(true).iter().map(|p| triangles(p.shape)).sum();
        println!("horse {body} triangles, tack {tack}");
        assert!((4000..=6000).contains(&body), "{body}");
        assert!(tack < 2000, "{tack}");
        for piece in [
            HorsePiece::Barrel,
            HorsePiece::Neck,
            HorsePiece::Head,
            HorsePiece::Mane,
            HorsePiece::Tail,
            HorsePiece::Ear,
            HorsePiece::Hoof,
            HorsePiece::Cannon,
            HorsePiece::Fetlock,
            HorsePiece::Forearm,
            HorsePiece::Gaskin,
            HorsePiece::Muzzle,
        ] {
            assert!(horse_parts().iter().any(|p| p.piece == piece), "{piece:?}");
        }
        assert_eq!(
            horse_parts()
                .iter()
                .filter(|p| p.piece == HorsePiece::Hoof)
                .count(),
            4
        );
    }

    #[test]
    fn every_coat_looks_different() {
        let mean = |coat| {
            let v = coat_vertices(
                coat,
                &[HorsePiece::Barrel, HorsePiece::Croup, HorsePiece::Neck],
            );
            let sum = v.iter().fold(Vec3::ZERO, |s, (_, _, c)| {
                s + Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)
            });
            sum / v.len() as f32
        };
        for (i, a) in Coat::ALL.iter().enumerate() {
            for b in &Coat::ALL[i + 1..] {
                let d = mean(*a).distance(mean(*b));
                assert!(d > 20.0, "{a:?} vs {b:?}: {d}");
            }
        }
    }

    #[test]
    fn the_bayo_has_black_points_and_a_dorsal_stripe() {
        let coat = Coat::Bayo;
        let all = coat_vertices(coat, &[]);
        let legs: Vec<_> = all.iter().filter(|(p, _, _)| p.y < 0.4).collect();
        assert!(!legs.is_empty());
        assert!(
            legs.iter().all(|(_, _, c)| luma(*c) < 60.0),
            "black lower legs"
        );
        let body = coat_vertices(
            coat,
            &[HorsePiece::Barrel, HorsePiece::Chest, HorsePiece::Croup],
        );
        let flank = body
            .iter()
            .filter(|(p, n, _)| p.y > 0.9 && n.z.abs() > 0.8)
            .map(|(_, _, c)| luma(*c));
        assert!(
            flank.clone().all(|l| l > 140.0),
            "a sandy body: {:?}",
            flank.clone().filter(|l| *l <= 140.0).collect::<Vec<_>>()
        );
        for (_, _, c) in coat_vertices(coat, &[HorsePiece::Mane, HorsePiece::Tail]) {
            assert!(luma(c) < 60.0, "black mane and tail");
        }
        let stripe: Vec<_> = all
            .iter()
            .filter(|(p, n, _)| n.y > 0.8 && p.z.abs() < 0.02 && (-0.6..0.2).contains(&p.x))
            .collect();
        assert!(stripe.len() >= 3, "{}", stripe.len());
        assert!(
            stripe.iter().all(|(_, _, c)| luma(*c) < 100.0),
            "a dark stripe down the back"
        );
        // The other coats have no stripe.
        assert!(
            coat_vertices(Coat::Alazan, &[])
                .iter()
                .all(|(_, _, c)| *c != [104, 76, 46])
        );
    }

    #[test]
    fn the_overo_shows_white_patches_from_every_three_quarter_view() {
        let all = coat_vertices(Coat::Overo, &[]);
        for (x, z) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let toward_viewer = Vec3::new(x, 0.6, z).normalize();
            let seen: Vec<_> = all
                .iter()
                .filter(|(p, n, _)| n.dot(toward_viewer) > 0.3 && p.y > POINTS_HEIGHT)
                .collect();
            let white = seen.iter().filter(|(_, _, c)| luma(*c) > 200.0).count();
            let share = white as f32 / seen.len() as f32;
            println!("overo from ({x}, {z}): {share}");
            assert!((0.12..0.7).contains(&share), "from ({x}, {z}): {share}");
        }
        // White hooves under white stockings.
        assert_ne!(Coat::Overo.hoof(), Coat::Zaino.hoof());
    }

    #[test]
    fn the_tordillo_is_dappled() {
        let barrel = coat_vertices(Coat::Tordillo, &[HorsePiece::Barrel, HorsePiece::Croup]);
        let light = barrel.iter().filter(|(_, _, c)| luma(*c) > 160.0).count();
        let dark = barrel.iter().filter(|(_, _, c)| luma(*c) < 130.0).count();
        let n = barrel.len();
        assert!(
            light * 7 > n && dark * 7 > n,
            "{light} light, {dark} dark of {n}"
        );
    }

    #[test]
    fn shade_gathers_under_the_belly_between_the_legs_and_under_the_saddle() {
        let parts = horse_parts();
        let vertices = |pieces: &[HorsePiece]| -> Vec<(HorsePiece, Vec3, Vec3)> {
            parts
                .iter()
                .filter(|p| pieces.contains(&p.piece))
                .flat_map(|part| {
                    let mesh = part.shape.mesh();
                    let place = part.rest_transform();
                    let turn =
                        Mat3::from_quat(part.rotation) * Mat3::from_diagonal(part.size.recip());
                    let p = mesh
                        .attribute(Mesh::ATTRIBUTE_POSITION)
                        .unwrap()
                        .as_float3()
                        .unwrap()
                        .to_vec();
                    let n = mesh
                        .attribute(Mesh::ATTRIBUTE_NORMAL)
                        .unwrap()
                        .as_float3()
                        .unwrap()
                        .to_vec();
                    let piece = part.piece;
                    p.into_iter().zip(n).map(move |(p, n)| {
                        (
                            piece,
                            place.transform_point3(Vec3::from(p)),
                            (turn * Vec3::from(n)).normalize(),
                        )
                    })
                })
                .collect()
        };
        let mean = |v: Vec<f32>| v.iter().sum::<f32>() / v.len().max(1) as f32;
        let body = vertices(&[HorsePiece::Barrel]);
        let belly = mean(
            body.iter()
                .filter(|(_, _, n)| n.y < -0.7)
                .map(|(k, p, n)| occlusion(*k, *p, *n))
                .collect(),
        );
        let flank = mean(
            body.iter()
                .filter(|(_, p, n)| n.z.abs() > 0.95 && p.x.abs() < 0.5)
                .map(|(k, p, n)| occlusion(*k, *p, *n))
                .collect(),
        );
        assert!(belly < 0.8 && flank > 0.95, "belly {belly}, flank {flank}");
        let saddle_edge = mean(
            body.iter()
                .filter(|(_, p, n)| {
                    n.z.abs() > 0.8 && (1.12..1.2).contains(&p.y) && p.x.abs() < 0.2
                })
                .map(|(k, p, n)| occlusion(*k, *p, *n))
                .collect(),
        );
        assert!(saddle_edge < flank - 0.1, "under the saddle {saddle_edge}");
        let legs = vertices(&[HorsePiece::Forearm, HorsePiece::Gaskin]);
        let side = |inner: bool| {
            mean(
                legs.iter()
                    .filter(|(_, p, n)| (n.z * p.z.signum() < -0.7) == inner && n.z.abs() > 0.7)
                    .map(|(k, p, n)| occlusion(*k, *p, *n))
                    .collect(),
            )
        };
        assert!(
            side(true) < side(false) - 0.15,
            "inner {} outer {}",
            side(true),
            side(false)
        );
        // The painted mesh carries it: a uniform zaino is darker underneath.
        let barrel = parts
            .iter()
            .find(|p| p.piece == HorsePiece::Barrel)
            .unwrap();
        let mesh = painted_mesh(barrel, Coat::Zaino);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours");
        };
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
            .as_float3()
            .unwrap();
        let red = |below: bool| {
            mean(
                colors
                    .iter()
                    .zip(normals)
                    .filter(|(_, n)| (n[1] < -0.7) == below)
                    .map(|(c, _)| c[0])
                    .collect(),
            )
        };
        assert!(red(true) < red(false), "the barrel's underside is darker");
    }

    #[test]
    fn horse_parts_have_their_finishes() {
        let coat = Coat::Alazan;
        let finish = |piece| {
            horse_parts()
                .into_iter()
                .chain(tack_parts(true))
                .find(|p| p.piece == piece)
                .map(|p| p.finish(coat))
                .unwrap()
        };
        assert_eq!(finish(HorsePiece::Barrel), Finish::Coat);
        assert_eq!(
            finish(HorsePiece::Head),
            Finish::HorseFace(coat),
            "a face texture"
        );
        assert_eq!(finish(HorsePiece::Tail), Finish::Hair);
        assert_eq!(finish(HorsePiece::Hoof), Finish::Hoof);
        assert_eq!(finish(HorsePiece::Carona), Finish::Leather);
        assert_eq!(finish(HorsePiece::Rein), Finish::Rawhide);
        assert_eq!(finish(HorsePiece::Stirrup), Finish::Iron);
        assert_eq!(finish(HorsePiece::Pellon), Finish::Fleece);
        assert_eq!(finish(HorsePiece::Sudadera), Finish::Wool);
        // The head's loft wraps the face texture: u 0.5 down the face.
        let head = horse_parts()
            .into_iter()
            .find(|p| p.piece == HorsePiece::Head)
            .unwrap();
        assert!(head.shape.mesh().attribute(Mesh::ATTRIBUTE_UV_0).is_some());
    }

    #[test]
    fn coats_are_seeded_deterministically_and_hidalgo_rides_a_bayo() {
        assert_eq!(Coat::HIDALGO, Coat::Bayo);
        for seed in 0..50 {
            assert_eq!(Coat::seeded(seed), Coat::seeded(seed));
        }
        let seen: std::collections::HashSet<_> = (0..200).map(Coat::seeded).collect();
        assert_eq!(seen.len(), Coat::ALL.len(), "every coat turns up");
        let part = horse_parts()[0];
        let a = painted_mesh(&part, Coat::Overo);
        let b = painted_mesh(&part, Coat::Overo);
        assert_eq!(
            format!("{:?}", a.attribute(Mesh::ATTRIBUTE_COLOR)),
            format!("{:?}", b.attribute(Mesh::ATTRIBUTE_COLOR))
        );
    }

    #[test]
    fn the_rider_sits_on_the_pellon_with_feet_in_the_stirrups() {
        // The pellón's top is the seat.
        let pellon = tack_parts(true)
            .into_iter()
            .find(|p| p.piece == HorsePiece::Pellon)
            .unwrap();
        let top = pellon.joint.rest_position().y + pellon.center.y + pellon.size.y / 2.0;
        assert!((top - SEAT.y).abs() < 0.01, "{top}");
        assert!(SEAT.y > withers_height(), "the seat is above the withers");
        // The clerk's hips rest on it.
        let offset = rider_offset() + HorseJoint::Trunk.rest_position();
        let hips = figure_parts(FigureKind::Protagonist)
            .into_iter()
            .find(|p| p.piece == Piece::Hips)
            .unwrap();
        let bottom = offset.y + hips.joint.rest_position().y + hips.center.y - hips.size.y / 2.0;
        assert!(
            (bottom - SEAT.y).abs() < 0.02,
            "hips at {bottom}, seat at {}",
            SEAT.y
        );
        // Stirrups by the rider's feet, one each side, outside the barrel.
        let stirrups: Vec<_> = tack_parts(true)
            .into_iter()
            .filter(|p| p.piece == HorsePiece::Stirrup)
            .collect();
        assert_eq!(stirrups.len(), 2);
        for s in &stirrups {
            let at = s.joint.rest_position() + s.center;
            assert!((0.8..1.1).contains(&at.y) && at.z.abs() > 0.26, "{at}");
        }
        // The recado: sudadera, carona, two bastos, pellón and cincha.
        let tack = tack_parts(false);
        for piece in [
            HorsePiece::Sudadera,
            HorsePiece::Carona,
            HorsePiece::Basto,
            HorsePiece::Pellon,
            HorsePiece::Cincha,
            HorsePiece::Headstall,
            HorsePiece::Bit,
            HorsePiece::Rein,
            HorsePiece::StirrupLeather,
        ] {
            assert!(tack.iter().any(|p| p.piece == piece), "{piece:?}");
        }
    }
}
