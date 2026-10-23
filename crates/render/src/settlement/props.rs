//! The settlements' smaller structures, built procedurally in their own
//! space like the buildings (see [`super::buildings`]): `x` along the
//! front, `z` out of it, `y` up from the ground at the middle.
//!
//! - The corral: posts and three rails of weathered boards, its 4 m
//!   opening in the middle of its front, as core leaves it.
//! - The molino: an Aermotor-style steel lattice tower with its platform,
//!   gearbox and tail vane, and a many-bladed wheel built apart so it can
//!   spin ([`crate::WindmillRotor`]).
//! - The australian tank: a round galvanised corrugated wall on a brick
//!   footing, full of water. The bebedero: a long plank trough.
//! - The well: a brick curb with a timber frame, pulley and bucket.
//! - The parva (a loaf of hay), the palenque and the carreta with its two
//!   very tall wheels, its bed, its paja toldo and its pole.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;
use pampa_core::{Settlement, Structure, StructureKind};

use super::buildings::local_size;
use super::materials::Surface;
use super::shape::Shape;

/// Local height of the ground under a point of a structure's own space,
/// relative to its middle.
pub(crate) type LocalGround<'a> = &'a dyn Fn(Vec3) -> f32;

/// One side of a corral: its mesh and the ends of its line, in the
/// corral's own space (for its fade shape).
pub(crate) struct CorralSide {
    pub shape: Shape,
    pub ends: (Vec3, Vec3),
}

/// Spacing of a corral's posts, in meters.
const POST_SPACING: f32 = 2.4;
/// Heights of a corral's rails.
const RAILS: [f32; 3] = [0.45, 0.9, 1.35];

/// The corral `structure`'s sides, each following the ground: the front
/// in two pieces either side of its opening.
pub(crate) fn corral(structure: &Structure, ground: LocalGround) -> Vec<CorralSide> {
    let size = local_size(structure);
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    let gate = Settlement::CORRAL_GATE / 2.0;
    let runs = [
        (Vec3::new(-hx, 0.0, hz), Vec3::new(-gate, 0.0, hz), true),
        (Vec3::new(gate, 0.0, hz), Vec3::new(hx, 0.0, hz), true),
        (Vec3::new(hx, 0.0, hz), Vec3::new(hx, 0.0, -hz), false),
        (Vec3::new(hx, 0.0, -hz), Vec3::new(-hx, 0.0, -hz), false),
        (Vec3::new(-hx, 0.0, -hz), Vec3::new(-hx, 0.0, hz), false),
    ];
    runs.into_iter()
        .map(|(a, b, front)| {
            let mut shape = Shape::default();
            let at = |p: Vec3| p.with_y(ground(p));
            let n = ((a.distance(b) / POST_SPACING).ceil() as usize).max(1);
            let points: Vec<Vec3> = (0..=n)
                .map(|k| at(a.lerp(b, k as f32 / n as f32)))
                .collect();
            for (k, &p) in points.iter().enumerate() {
                // The gate posts are stouter and taller.
                let gate_post = front && ((k == n && a.x < 0.0) || (k == 0 && a.x > 0.0));
                let (side, top) = if gate_post { (0.24, 1.9) } else { (0.15, 1.6) };
                shape.beam(Surface::Planks, p - Vec3::Y * 0.3, p + Vec3::Y * top, side);
            }
            for w in points.windows(2) {
                for y in RAILS {
                    shape.beam(
                        Surface::Planks,
                        w[0] + Vec3::Y * y,
                        w[1] + Vec3::Y * y,
                        0.07,
                    );
                }
            }
            CorralSide {
                shape,
                ends: (a, b),
            }
        })
        .collect()
}

/// Radius of the molino's wheel, in meters.
pub(crate) const WHEEL_RADIUS: f32 = 1.8;

/// Where the molino's wheel turns in its own space, and its yaw: facing
/// the prevailing wind, a little different on every molino.
pub(crate) fn rotor_place(structure: &Structure) -> (Vec3, Quat) {
    let yaw = Quat::from_rotation_y(0.6 + (structure.variant % 97) as f32 / 97.0 * 0.8);
    let hub = Vec3::new(0.0, structure.height + 0.45, 0.0);
    (hub + yaw * Vec3::Z * 0.7, yaw)
}

/// The molino's tower, platform, gearbox, vane and pump rod (the wheel is
/// [`rotor`]), `drop` meters into the ground.
pub(crate) fn windpump(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let foot = local_size(structure).x.min(local_size(structure).y) / 2.0 - 0.1;
    let (top, head) = (structure.height, 0.3);
    let leg = |sx: f32, sz: f32, y: f32| {
        let r = foot + (head - foot) * (y / top);
        Vec3::new(sx * r, y, sz * r)
    };
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for (sx, sz) in corners {
        shape.beam(
            Surface::Steel,
            leg(sx, sz, 0.0) - Vec3::Y * drop,
            leg(sx, sz, top),
            0.09,
        );
        // A concrete-set anchor at each foot.
        let f = leg(sx, sz, 0.0);
        shape.block(
            Surface::Brick,
            f - Vec3::new(0.2, drop, 0.2),
            f + Vec3::new(0.2, 0.15, 0.2),
        );
    }
    // Girts at every level and crossed diagonal rods between.
    let levels = [0.0, 0.22, 0.42, 0.6, 0.76, 0.9];
    for (k, &l) in levels.iter().enumerate() {
        let y = top * l + 0.3;
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            shape.beam(Surface::Steel, leg(a.0, a.1, y), leg(b.0, b.1, y), 0.05);
            if let Some(&next) = levels.get(k + 1) {
                let y2 = top * next + 0.3;
                shape.beam(Surface::Steel, leg(a.0, a.1, y), leg(b.0, b.1, y2), 0.025);
                shape.beam(Surface::Steel, leg(b.0, b.1, y), leg(a.0, a.1, y2), 0.025);
            }
        }
    }
    // The platform round the top and its pump rod down the middle.
    shape.block(
        Surface::Planks,
        Vec3::new(-0.75, top - 0.6, -0.75),
        Vec3::new(0.75, top - 0.52, 0.75),
    );
    shape.beam(
        Surface::Steel,
        Vec3::new(0.0, 0.2, 0.0),
        Vec3::new(0.0, top, 0.0),
        0.04,
    );
    // The pump head at the foot.
    shape.lathe(
        Surface::Steel,
        Transform::IDENTITY,
        &[(0.12, -0.1), (0.12, 0.7), (0.06, 0.8), (0.0, 0.8)],
        8,
    );
    // Gearbox, turntable and the tail vane on its boom, in the wheel's yaw.
    let (hub, yaw) = rotor_place(structure);
    let mount = Vec3::new(0.0, hub.y, 0.0);
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(Vec3::new(0.0, top, 0.0)),
        &[(0.25, 0.0), (0.25, 0.15), (0.0, 0.15)],
        10,
    );
    shape.cuboid(
        Surface::Steel,
        Transform::from_translation(mount + yaw * Vec3::Z * 0.15)
            .with_rotation(yaw)
            .with_scale(Vec3::new(0.35, 0.4, 0.9)),
        true,
    );
    let tail = mount - yaw * Vec3::Z * 2.4;
    shape.beam(Surface::Steel, mount, tail, 0.06);
    shape.cuboid(
        Surface::Sheet,
        Transform::from_translation(tail - yaw * Vec3::Z * 0.6)
            .with_rotation(yaw)
            .with_scale(Vec3::new(0.03, 1.0, 1.5)),
        true,
    );
    shape
}

/// The molino's wheel in its own space, turning about `Z`: eighteen
/// curved galvanised blades between an inner and an outer steel ring, on
/// a hub.
pub(crate) fn rotor() -> Shape {
    let mut shape = Shape::default();
    let blades = 18;
    let inner = 0.6;
    for k in 0..blades {
        let a = k as f32 / blades as f32 * TAU;
        let dir = Vec3::new(a.cos(), a.sin(), 0.0);
        let mid = dir * ((inner + WHEEL_RADIUS) / 2.0);
        // Each blade twisted into the wind, wider at the rim.
        let turn = Quat::from_rotation_z(a - FRAC_PI_2) * Quat::from_rotation_y(0.5);
        shape.cuboid(
            Surface::Sheet,
            Transform::from_translation(mid)
                .with_rotation(turn)
                .with_scale(Vec3::new(0.32, WHEEL_RADIUS - inner, 0.02)),
            true,
        );
    }
    for r in [inner, WHEEL_RADIUS * 0.75, WHEEL_RADIUS] {
        let n = 24;
        for k in 0..n {
            let (a, b) = (k as f32 / n as f32 * TAU, (k + 1) as f32 / n as f32 * TAU);
            shape.beam(
                Surface::Steel,
                Vec3::new(a.cos(), a.sin(), 0.0) * r,
                Vec3::new(b.cos(), b.sin(), 0.0) * r,
                0.04,
            );
        }
    }
    for k in 0..6 {
        let a = k as f32 / 6.0 * TAU;
        shape.beam(
            Surface::Steel,
            Vec3::ZERO,
            Vec3::new(a.cos(), a.sin(), 0.0) * inner,
            0.04,
        );
    }
    shape.lathe(
        Surface::Steel,
        Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        &[(0.0, -0.2), (0.12, -0.2), (0.12, 0.15), (0.0, 0.25)],
        10,
    );
    shape
}

/// The australian tank: corrugated galvanised walls round a brick footing,
/// the water a little under the rim.
pub(crate) fn tank(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let r = local_size(structure).x.min(local_size(structure).y) / 2.0 - 0.15;
    let h = structure.height;
    let segments = 40;
    shape.lathe(
        Surface::Brick,
        Transform::IDENTITY,
        &[(r + 0.15, -drop), (r + 0.15, 0.2), (r - 0.05, 0.2)],
        segments,
    );
    shape.lathe(
        Surface::Galvanised,
        Transform::IDENTITY,
        &[(r, 0.2), (r, h)],
        segments,
    );
    // The rolled rim, then the inner wall back down.
    shape.lathe(
        Surface::Galvanised,
        Transform::IDENTITY,
        &[
            (r, h),
            (r + 0.02, h + 0.04),
            (r - 0.04, h + 0.04),
            (r - 0.04, h),
        ],
        segments,
    );
    shape.lathe(
        Surface::Galvanised,
        Transform::IDENTITY,
        &[(r - 0.04, h), (r - 0.04, h - 0.4)],
        segments,
    );
    shape.disc(
        Surface::Water,
        Transform::from_xyz(0.0, h - 0.3, 0.0),
        r - 0.03,
        segments,
    );
    shape
}

/// The bebedero: a long trough of planks on short legs, brim-full.
pub(crate) fn trough(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let size = local_size(structure);
    let (hx, hz, h) = (size.x / 2.0, size.y / 2.0, structure.height);
    let wall = 0.06;
    // The sides and ends, then the water between.
    shape.block(
        Surface::Planks,
        Vec3::new(-hx, 0.15, hz - wall),
        Vec3::new(hx, h, hz),
    );
    shape.block(
        Surface::Planks,
        Vec3::new(-hx, 0.15, -hz),
        Vec3::new(hx, h, -hz + wall),
    );
    shape.block(
        Surface::Planks,
        Vec3::new(-hx, 0.15, -hz + wall),
        Vec3::new(-hx + wall, h, hz - wall),
    );
    shape.block(
        Surface::Planks,
        Vec3::new(hx - wall, 0.15, -hz + wall),
        Vec3::new(hx, h, hz - wall),
    );
    shape.cuboid(
        Surface::Planks,
        Transform::from_xyz(0.0, 0.18, 0.0).with_scale(Vec3::new(2.0 * hx, 0.06, 2.0 * hz)),
        true,
    );
    let y = h - 0.08;
    shape.face(
        Surface::Water,
        &[
            Vec3::new(-hx + wall, y, hz - wall),
            Vec3::new(hx - wall, y, hz - wall),
            Vec3::new(hx - wall, y, -hz + wall),
            Vec3::new(-hx + wall, y, -hz + wall),
        ],
    );
    let legs = ((2.0 * hx / 1.8) as usize).max(2);
    for k in 0..legs {
        let x = -hx + 0.2 + (2.0 * hx - 0.4) * k as f32 / (legs - 1) as f32;
        for z in [-hz + 0.1, hz - 0.1] {
            shape.beam(
                Surface::Planks,
                Vec3::new(x, -drop, z),
                Vec3::new(x, 0.2, z),
                0.1,
            );
        }
    }
    shape
}

/// The well: a round brick curb, a timber frame with a pulley, a bucket
/// on its rope, and dark water down inside.
pub(crate) fn well(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let r = local_size(structure).x.min(local_size(structure).y) / 2.0 - 0.05;
    let h = structure.height;
    let segments = 20;
    shape.lathe(
        Surface::Brick,
        Transform::IDENTITY,
        &[
            (r, -drop),
            (r, h),
            (r, h),
            (r - 0.24, h),
            (r - 0.24, h),
            (r - 0.24, -0.6),
        ],
        segments,
    );
    shape.disc(
        Surface::Water,
        Transform::from_xyz(0.0, -0.6, 0.0),
        r - 0.2,
        segments,
    );
    let top = h + 1.5;
    for x in [-r + 0.1, r - 0.1] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, h - 0.4, 0.0),
            Vec3::new(x, top, 0.0),
            0.12,
        );
    }
    shape.beam(
        Surface::Planks,
        Vec3::new(-r - 0.05, top, 0.0),
        Vec3::new(r + 0.05, top, 0.0),
        0.12,
    );
    shape.lathe(
        Surface::Planks,
        Transform::from_xyz(0.0, top - 0.17, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        &[(0.0, -0.04), (0.12, -0.04), (0.12, 0.04), (0.0, 0.04)],
        10,
    );
    shape.beam(
        Surface::Planks,
        Vec3::new(0.12, top - 0.17, 0.0),
        Vec3::new(0.12, h + 0.55, 0.0),
        0.015,
    );
    shape.lathe(
        Surface::Galvanised,
        Transform::from_xyz(0.12, h + 0.25, 0.0),
        &[
            (0.0, 0.0),
            (0.12, 0.0),
            (0.15, 0.3),
            (0.13, 0.3),
            (0.1, 0.03),
        ],
        10,
    );
    shape
}

/// A parva: a loaf of hay over its footprint, rounded on top.
pub(crate) fn haystack(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let size = local_size(structure);
    let h = structure.height;
    shape.lathe(
        Surface::Thatch,
        Transform::from_scale(Vec3::new(size.x / 2.0, 1.0, size.y / 2.0)),
        &[
            (0.92, -drop),
            (1.0, h * 0.25),
            (0.98, h * 0.5),
            (0.86, h * 0.72),
            (0.6, h * 0.9),
            (0.25, h * 0.98),
            (0.0, h),
        ],
        20,
    );
    shape
}

/// The palenque: two stout posts and a rail to tie horses to.
pub(crate) fn hitching_post(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let hx = local_size(structure).x / 2.0 - 0.15;
    let h = structure.height;
    for x in [-hx, hx] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, -drop, 0.0),
            Vec3::new(x, h, 0.0),
            0.16,
        );
    }
    shape.beam(
        Surface::Planks,
        Vec3::new(-hx - 0.1, h - 0.1, 0.0),
        Vec3::new(hx + 0.1, h - 0.1, 0.0),
        0.1,
    );
    shape
}

/// The carreta: a bed between two very tall wheels, a paja toldo arched
/// over it and its long pole resting on the ground ahead.
pub(crate) fn cart(structure: &Structure) -> Shape {
    let mut shape = Shape::default();
    let size = local_size(structure);
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    let radius = (structure.height / 2.0).min(1.25);
    let axle = Vec3::new(-0.4, radius, 0.0);
    for z in [-hz + 0.12, hz - 0.12] {
        wheel(&mut shape, axle.with_z(z), radius);
    }
    shape.beam(
        Surface::Planks,
        axle.with_z(-hz + 0.05),
        axle.with_z(hz - 0.05),
        0.12,
    );
    // The bed: a floor, stakes and side boards.
    let (b0, b1) = (-hx + 0.3, 1.2);
    let bed = radius + 0.1;
    let bw = hz - 0.35;
    shape.cuboid(
        Surface::Planks,
        Transform::from_xyz((b0 + b1) / 2.0, bed, 0.0).with_scale(Vec3::new(
            b1 - b0,
            0.1,
            2.0 * bw,
        )),
        true,
    );
    for z in [-bw, bw] {
        for k in 0..5 {
            let x = b0 + (b1 - b0) * k as f32 / 4.0;
            shape.beam(
                Surface::Planks,
                Vec3::new(x, bed, z),
                Vec3::new(x, bed + 0.7, z),
                0.07,
            );
        }
        for y in [0.25, 0.55] {
            shape.beam(
                Surface::Planks,
                Vec3::new(b0, bed + y, z),
                Vec3::new(b1, bed + y, z),
                0.05,
            );
        }
    }
    // The toldo: an arch of paja over hoops, open at both ends.
    let arch = 8;
    let at = |k: usize, x: f32| {
        let a = k as f32 / arch as f32 * std::f32::consts::PI;
        Vec3::new(x, bed + 0.7 + a.sin() * 0.75, a.cos() * (bw + 0.05))
    };
    let (t0, t1) = (b0 + 0.1, b1 - 0.1);
    for k in 0..arch {
        let (p, q) = (at(k, t0), at(k + 1, t0));
        let (p1, q1) = (at(k, t1), at(k + 1, t1));
        shape.face(Surface::Thatch, &[p, p1, q1, q]);
        shape.face(Surface::Thatch, &[p, q, q1, p1]);
    }
    // The pole, its far end resting on the ground.
    shape.beam(
        Surface::Planks,
        Vec3::new(b1 - 0.6, bed - 0.05, 0.0),
        Vec3::new(hx - 0.1, 0.15, 0.0),
        0.12,
    );
    shape
}

/// A spoked wheel round `hub` (axle along `Z`).
fn wheel(shape: &mut Shape, hub: Vec3, radius: f32) {
    let rim = 20;
    for k in 0..rim {
        let (a, b) = (
            k as f32 / rim as f32 * TAU,
            (k + 1) as f32 / rim as f32 * TAU,
        );
        shape.beam(
            Surface::Planks,
            hub + Vec3::new(a.cos(), a.sin(), 0.0) * radius,
            hub + Vec3::new(b.cos(), b.sin(), 0.0) * radius,
            0.11,
        );
    }
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU;
        let dir = Vec3::new(a.cos(), a.sin(), 0.0);
        shape.beam(
            Surface::Planks,
            hub + dir * 0.18,
            hub + dir * (radius - 0.05),
            0.05,
        );
    }
    shape.lathe(
        Surface::Planks,
        Transform::from_translation(hub).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        &[
            (0.0, -0.18),
            (0.16, -0.18),
            (0.2, 0.0),
            (0.16, 0.18),
            (0.0, 0.18),
        ],
        12,
    );
}

/// The prop `structure` in its own space, for every kind but the houses,
/// galpones, corrals, trees and the molino's wheel. An abandoned wagon
/// without its model is drawn as a carreta.
pub(crate) fn prop(structure: &Structure, drop: f32) -> Shape {
    match structure.kind {
        StructureKind::Windpump => windpump(structure, drop),
        StructureKind::Tank => tank(structure, drop),
        StructureKind::Trough => trough(structure, drop),
        StructureKind::Well => well(structure, drop),
        StructureKind::Haystack => haystack(structure, drop),
        StructureKind::HitchingPost => hitching_post(structure, drop),
        StructureKind::Cart | StructureKind::WreckedWagon => cart(structure),
        _ => Shape::default(),
    }
}
