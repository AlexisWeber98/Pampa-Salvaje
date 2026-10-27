//! The car builder: collects a car's static shapes by material and makes
//! one mesh of each, spawns its wheels, rods and smoke as entities of their
//! own, and builds the underframe and bogies every car stands on.

use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use super::kit::{BOGIE_RADIUS, Kit, Mat};
use super::shape::{Grain, Shape};
use super::wheels::{WHEEL_Z, WheelStyle};
use super::{SmokePuff, TrainRod, TrainWheel};

/// Where the underframe's top meets the body, over the rail tops.
pub const DECK: f32 = 1.1;

/// Spawns child meshes of one car, in its local frame (origin on the rails
/// at the car's middle, +X forward, +Z to the car's right).
pub struct CarBuilder<'a, 'c> {
    pub car: EntityCommands<'c>,
    /// Index of the car's train in [`super::WorldTrains`].
    pub train: usize,
    pub kit: &'a Kit,
    meshes: &'a mut Assets<Mesh>,
    parts: BTreeMap<Mat, Shape>,
    /// The coach's inside, drawn apart so it can be told from the shell.
    inside: BTreeMap<Mat, Shape>,
}

impl<'a, 'c> CarBuilder<'a, 'c> {
    pub fn new(
        car: EntityCommands<'c>,
        train: usize,
        kit: &'a Kit,
        meshes: &'a mut Assets<Mesh>,
    ) -> Self {
        Self {
            car,
            train,
            kit,
            meshes,
            parts: BTreeMap::new(),
            inside: BTreeMap::new(),
        }
    }

    /// Adds `shape` to the car's mesh of `mat`.
    pub fn add(&mut self, mat: Mat, shape: Shape) {
        self.parts.entry(mat).or_default().append(&shape);
    }

    /// Adds `shape` to the car's inside mesh of `mat` (see
    /// [`super::CoachInterior`]).
    pub fn inside(&mut self, mat: Mat, shape: Shape) {
        self.inside.entry(mat).or_default().append(&shape);
    }

    /// A box inside the car.
    pub fn inside_box(&mut self, mat: Mat, center: Vec3, size: Vec3) {
        self.inside(mat, Shape::cuboid(center, size, Grain::Along));
    }

    /// Adds `shape` moved by `t`.
    pub fn put(&mut self, mat: Mat, shape: &Shape, t: Transform) {
        self.add(mat, shape.transformed(t));
    }

    /// A box, its boards running along the first texture axis.
    pub fn boxed(&mut self, mat: Mat, center: Vec3, size: Vec3) {
        self.add(mat, Shape::cuboid(center, size, Grain::Along));
    }

    /// A box with upright boards.
    pub fn boxed_upright(&mut self, mat: Mat, center: Vec3, size: Vec3) {
        self.add(mat, Shape::cuboid(center, size, Grain::Across));
    }

    /// A round bar of radius `r` from `a` to `b`.
    pub fn bar(&mut self, mat: Mat, a: Vec3, b: Vec3, r: f32) {
        self.add(mat, bar_shape(a, b, r, 8));
    }

    /// A lathe turning about the axis from `at` along `dir`.
    pub fn turned(&mut self, mat: Mat, profile: &[(f32, f32)], sides: usize, at: Vec3, dir: Vec3) {
        let shape = Shape::lathe(profile, sides);
        self.put(
            mat,
            &shape,
            Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::X, dir)),
        );
    }

    /// A pair of wheels on one axle at `x`, with the axle between them.
    pub fn axle(&mut self, x: f32, radius: f32, style: WheelStyle) {
        let steel = self.kit.material(Mat::TYRE);
        let black = self.kit.material(Mat::BLACK);
        for side in [-1.0, 1.0] {
            let (tyre, centre) = self.kit.wheel(radius, style, side);
            let (steel, black) = (steel.clone(), black.clone());
            let train = self.train;
            self.car.with_children(|car| {
                car.spawn((
                    Mesh3d(tyre),
                    MeshMaterial3d(steel),
                    TrainWheel { train, radius },
                    Transform::from_xyz(x, radius, side * WHEEL_Z)
                        .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
                ))
                .with_child((
                    Mesh3d(centre),
                    MeshMaterial3d(black),
                    Transform::default(),
                ));
            });
        }
        let r = (radius * 0.09).max(0.05);
        self.bar(
            Mat::BLACK,
            Vec3::new(x, radius, -WHEEL_Z),
            Vec3::new(x, radius, WHEEL_Z),
            r,
        );
    }

    /// A rod swung round by the drivers' cranks (see [`TrainRod`]): the
    /// shape is the rod at rest, centred on `base`.
    pub fn rod(&mut self, shape: Shape, base: Vec3, phase: f32, length: f32) {
        let mesh = self.meshes.add(shape.into_mesh());
        // Bright steel: the rods catch the eye as they swing.
        let material = self.kit.material(Mat::TYRE);
        self.car.with_child((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            TrainRod {
                train: self.train,
                base,
                crank: 0.3,
                radius: super::DRIVER_RADIUS,
                phase,
                length,
            },
            Transform::from_translation(base),
        ));
    }

    /// Smoke puffs, under a holder at the car's origin so they never count
    /// as the car's own height, rising from `chimney`.
    pub fn smoke(&mut self, chimney: Vec3) {
        let (mesh, smoke) = (self.kit.puff.clone(), self.kit.smoke.clone());
        let train = self.train;
        self.car.with_children(|car| {
            car.spawn((Transform::default(), Visibility::default()))
                .with_children(|holder| {
                    for i in 0..4 {
                        holder.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(smoke.clone()),
                            SmokePuff {
                                train,
                                phase: i as f32 / 4.0,
                                chimney,
                            },
                            Transform::from_translation(chimney),
                            Visibility::Hidden,
                        ));
                    }
                });
        });
    }

    /// Makes the merged meshes, one child per material.
    pub fn finish(mut self) {
        for (mat, shape) in std::mem::take(&mut self.parts) {
            let mesh = self.meshes.add(shape.into_mesh());
            self.car
                .with_child((Mesh3d(mesh), MeshMaterial3d(self.kit.material(mat))));
        }
        for (mat, shape) in std::mem::take(&mut self.inside) {
            let mesh = self.meshes.add(shape.into_mesh());
            self.car.with_child((
                Mesh3d(mesh),
                MeshMaterial3d(self.kit.material(mat)),
                super::CoachInterior,
            ));
        }
    }

    /// The underframe every car but the engine stands on: solebars, buffer
    /// beams with their sprung buffers, a screw coupling, truss rods and
    /// the hint of brake gear.
    pub fn underframe(&mut self, length: f32) {
        let half = length / 2.0;
        self.boxed(
            Mat::BLACK,
            Vec3::new(0.0, DECK, 0.0),
            Vec3::new(length - 0.9, 0.3, 0.12),
        );
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(0.0, DECK + 0.02, side * 1.05),
                Vec3::new(length - 0.7, 0.26, 0.07),
            );
        }
        for end in [-1.0, 1.0] {
            self.buffer_beam(end * (half - 0.4), end);
            // A headstock cross-brace, and the screw coupling's link.
            self.boxed(
                Mat::BLACK,
                Vec3::new(end * (half - 1.0), DECK, 0.0),
                Vec3::new(0.1, 0.22, 2.1),
            );
            // Truss rods from the headstock down under the middle.
            for side in [-1.0, 1.0] {
                let z = side * 0.55;
                let low = DECK - 0.32;
                let (a, b, c) = (
                    Vec3::new(end * (half - 0.6), DECK - 0.1, z),
                    Vec3::new(end * (half - 2.6), low, z),
                    Vec3::new(end * 0.5, low, z),
                );
                self.bar(Mat::BLACK, a, b, 0.018);
                self.bar(Mat::BLACK, b, c, 0.018);
            }
        }
        // Brake cylinder and its pull rods.
        self.turned(
            Mat::BLACK,
            &[(-0.3, 0.0), (-0.3, 0.15), (0.3, 0.15), (0.3, 0.0)],
            12,
            Vec3::new(0.0, DECK - 0.35, -0.3),
            Vec3::Z,
        );
        for end in [-1.0, 1.0] {
            self.bar(
                Mat::BLACK,
                Vec3::new(0.0, DECK - 0.35, -0.3),
                Vec3::new(end * (half - 2.6), DECK - 0.3, -0.45),
                0.02,
            );
        }
    }

    /// A buffer beam at `x`, facing `end` (+1 forward), with two sprung
    /// buffers and a coupling hook.
    pub fn buffer_beam(&mut self, x: f32, end: f32) {
        self.boxed(
            Mat::BUFFER_RED,
            Vec3::new(x, DECK, 0.0),
            Vec3::new(0.2, 0.36, 2.7),
        );
        self.fittings(x + end * 0.1, end, DECK);
    }

    /// The buffers and coupling in front of a buffer-beam face at `x`.
    pub fn fittings(&mut self, x: f32, end: f32, y: f32) {
        let buffer = [
            (0.0, 0.0),
            (0.0, 0.055),
            (0.2, 0.055),
            (0.2, 0.10),
            (0.24, 0.19),
            (0.28, 0.205),
            (0.30, 0.20),
            (0.30, 0.0),
        ];
        // The profile runs outward along +X; the buffer stands on the beam.
        let buffer = Shape::lathe(&buffer, 14);
        let turn = Quat::from_rotation_y(if end > 0.0 { 0.0 } else { PI });
        for side in [-1.0, 1.0] {
            self.put(
                Mat::BLACK,
                &buffer,
                Transform::from_translation(Vec3::new(x, y, side * 0.9)).with_rotation(turn),
            );
        }
        // The draw hook and a hanging screw link.
        self.boxed(
            Mat::BLACK,
            Vec3::new(x + end * 0.2, y - 0.05, 0.0),
            Vec3::new(0.4, 0.07, 0.09),
        );
        self.boxed(
            Mat::BLACK,
            Vec3::new(x + end * 0.52, y - 0.2, 0.0),
            Vec3::new(0.05, 0.3, 0.07),
        );
    }

    /// Two four-wheeled plate-wheel bogies under a coach or freight car.
    pub fn bogies(&mut self, length: f32) {
        for end in [-1.0, 1.0] {
            self.bogie(end * (length / 2.0 - 2.3));
        }
    }

    /// One bogie centred at `x`: two axles, outside plate frames, leaf
    /// springs over the axle boxes and a bolster.
    fn bogie(&mut self, x: f32) {
        let spring = leaf_spring(0.55);
        for dx in [-0.95, 0.95] {
            self.axle(x + dx, BOGIE_RADIUS, WheelStyle::Plate);
        }
        for side in [-1.0, 1.0] {
            let z = side * 1.04;
            self.boxed(
                Mat::BLACK,
                Vec3::new(x, 0.78, z),
                Vec3::new(2.4, 0.14, 0.06),
            );
            for dx in [-0.95, 0.95] {
                // Axle box, spring, hanger.
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x + dx, BOGIE_RADIUS, z),
                    Vec3::new(0.22, 0.26, 0.1),
                );
                self.put(
                    Mat::BLACK,
                    &spring,
                    Transform::from_xyz(x + dx, 0.6, z + side * 0.04),
                );
                for ex in [-0.24, 0.24] {
                    self.boxed(
                        Mat::BLACK,
                        Vec3::new(x + dx + ex, 0.7, z),
                        Vec3::new(0.03, 0.14, 0.04),
                    );
                }
            }
            // The pedestals tying the frame to the axle boxes.
            for dx in [-1.25, -0.65, 0.65, 1.25] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x + dx, 0.62, z),
                    Vec3::new(0.05, 0.4, 0.05),
                );
            }
        }
        self.boxed(
            Mat::BLACK,
            Vec3::new(x, 0.88, 0.0),
            Vec3::new(0.4, 0.2, 2.1),
        );
    }
}

/// A round bar of radius `r` from `a` to `b`.
pub fn bar_shape(a: Vec3, b: Vec3, r: f32, sides: usize) -> Shape {
    let length = a.distance(b);
    Shape::lathe(&[(0.0, 0.0), (0.0, r), (length, r), (length, 0.0)], sides).transformed(
        Transform::from_translation(a)
            .with_rotation(Quat::from_rotation_arc(Vec3::X, (b - a).normalize())),
    )
}

/// A leaf spring: a lens `length` long, one box thick across Z.
fn leaf_spring(length: f32) -> Shape {
    let mut ring = vec![Vec2::new(-length / 2.0, 0.0)];
    for i in 1..8 {
        let t = i as f32 / 8.0 * 2.0 - 1.0;
        ring.push(Vec2::new(t * length / 2.0, 0.07 * (1.0 - t * t) - 0.0));
    }
    ring.push(Vec2::new(length / 2.0, 0.0));
    ring.push(Vec2::new(length / 2.0, -0.03));
    ring.push(Vec2::new(-length / 2.0, -0.03));
    Shape::along_z(&ring, -0.05, 0.05, false, true)
}
