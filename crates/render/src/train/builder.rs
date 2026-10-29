//! El constructor de vagones: reúne las formas estáticas de un vagón por
//! material y hace una mesh de cada una, crea sus ruedas, bielas y humo como
//! entidades propias, y construye el bastidor y los bogies sobre los que se
//! apoya cada vagón.

use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use super::kit::{BOGIE_RADIUS, Kit, Mat};
use super::shape::{Grain, Shape};
use super::wheels::{WHEEL_Z, WheelStyle};
use super::{SmokePuff, TrainRod, TrainWheel};

/// Donde la parte superior del bastidor se encuentra con la caja, por encima de
/// la cara superior de los rieles.
pub const DECK: f32 = 1.1;

/// Crea las meshes hijas de un vagón, en su marco local (origen sobre los
/// rieles en el centro del vagón, +X hacia adelante, +Z hacia la derecha del
/// vagón).
pub struct CarBuilder<'a, 'c> {
    pub car: EntityCommands<'c>,
    /// Índice del tren del vagón en [`super::WorldTrains`].
    pub train: usize,
    pub kit: &'a Kit,
    meshes: &'a mut Assets<Mesh>,
    parts: BTreeMap<Mat, Shape>,
    /// El interior del coche, dibujado aparte para poder distinguirlo de la
    /// carcasa.
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

    /// Agrega `shape` a la mesh del vagón de material `mat`.
    pub fn add(&mut self, mat: Mat, shape: Shape) {
        self.parts.entry(mat).or_default().append(&shape);
    }

    /// Agrega `shape` a la mesh interior del vagón de material `mat` (ver
    /// [`super::CoachInterior`]).
    pub fn inside(&mut self, mat: Mat, shape: Shape) {
        self.inside.entry(mat).or_default().append(&shape);
    }

    /// Una caja dentro del vagón.
    pub fn inside_box(&mut self, mat: Mat, center: Vec3, size: Vec3) {
        self.inside(mat, Shape::cuboid(center, size, Grain::Along));
    }

    /// Agrega `shape` desplazada por `t`.
    pub fn put(&mut self, mat: Mat, shape: &Shape, t: Transform) {
        self.add(mat, shape.transformed(t));
    }

    /// Una caja con las tablas a lo largo del primer eje de textura.
    pub fn boxed(&mut self, mat: Mat, center: Vec3, size: Vec3) {
        self.add(mat, Shape::cuboid(center, size, Grain::Along));
    }

    /// Una caja con tablas verticales.
    pub fn boxed_upright(&mut self, mat: Mat, center: Vec3, size: Vec3) {
        self.add(mat, Shape::cuboid(center, size, Grain::Across));
    }

    /// Una barra redonda de radio `r` desde `a` hasta `b`.
    pub fn bar(&mut self, mat: Mat, a: Vec3, b: Vec3, r: f32) {
        self.add(mat, bar_shape(a, b, r, 8));
    }

    /// Un torneado que gira alrededor del eje que parte de `at` en la dirección
    /// `dir`.
    pub fn turned(&mut self, mat: Mat, profile: &[(f32, f32)], sides: usize, at: Vec3, dir: Vec3) {
        let shape = Shape::lathe(profile, sides);
        self.put(
            mat,
            &shape,
            Transform::from_translation(at).with_rotation(Quat::from_rotation_arc(Vec3::X, dir)),
        );
    }

    /// Un par de ruedas sobre un eje en `x`, con el eje entre ellas.
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

    /// Una biela que hacen girar las manivelas de las ruedas motrices (ver
    /// [`TrainRod`]): la forma es la biela en reposo, centrada en `base`.
    pub fn rod(&mut self, shape: Shape, base: Vec3, phase: f32, length: f32) {
        let mesh = self.meshes.add(shape.into_mesh());
        // Acero brillante: las bielas llaman la atención al girar.
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

    /// Bocanadas de humo, bajo un soporte en el origen del vagón para que nunca
    /// cuenten como altura propia del vagón, saliendo de `chimney`.
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

    /// Hace las meshes fusionadas, una hija por material.
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

    /// El bastidor sobre el que se apoya cada vagón salvo la locomotora:
    /// largueros, testeros con sus topes de resorte, un enganche a tornillo,
    /// tensores y un esbozo del equipo de freno.
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
            // Un travesaño en el testero, y el eslabón del enganche a tornillo.
            self.boxed(
                Mat::BLACK,
                Vec3::new(end * (half - 1.0), DECK, 0.0),
                Vec3::new(0.1, 0.22, 2.1),
            );
            // Tensores desde el testero hacia abajo, bajo el centro.
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
        // El cilindro de freno y sus varillas de tiro.
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

    /// Un testero en `x`, mirando hacia `end` (+1 hacia adelante), con dos
    /// topes de resorte y un gancho de enganche.
    pub fn buffer_beam(&mut self, x: f32, end: f32) {
        self.boxed(
            Mat::BUFFER_RED,
            Vec3::new(x, DECK, 0.0),
            Vec3::new(0.2, 0.36, 2.7),
        );
        self.fittings(x + end * 0.1, end, DECK);
    }

    /// Los topes y el enganche delante de la cara de un testero en `x`.
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
        // El perfil corre hacia afuera a lo largo de +X; el tope se apoya en el
        // testero.
        let buffer = Shape::lathe(&buffer, 14);
        let turn = Quat::from_rotation_y(if end > 0.0 { 0.0 } else { PI });
        for side in [-1.0, 1.0] {
            self.put(
                Mat::BLACK,
                &buffer,
                Transform::from_translation(Vec3::new(x, y, side * 0.9)).with_rotation(turn),
            );
        }
        // El gancho de tracción y un eslabón de tornillo colgante.
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

    /// Dos bogies de cuatro ruedas de disco bajo un coche o un vagón de carga.
    pub fn bogies(&mut self, length: f32) {
        for end in [-1.0, 1.0] {
            self.bogie(end * (length / 2.0 - 2.3));
        }
    }

    /// Un bogie centrado en `x`: dos ejes, bastidores de chapa exteriores,
    /// ballestas sobre las cajas de grasa y un travesaño de carga.
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
                // Caja de grasa, ballesta, colgante.
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
            // Los pedestales que unen el bastidor con las cajas de grasa.
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

/// Una barra redonda de radio `r` desde `a` hasta `b`.
pub fn bar_shape(a: Vec3, b: Vec3, r: f32, sides: usize) -> Shape {
    let length = a.distance(b);
    Shape::lathe(&[(0.0, 0.0), (0.0, r), (length, r), (length, 0.0)], sides).transformed(
        Transform::from_translation(a)
            .with_rotation(Quat::from_rotation_arc(Vec3::X, (b - a).normalize())),
    )
}

/// Una ballesta: una lente de `length` de largo, del grosor de una caja a lo
/// largo de Z.
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
