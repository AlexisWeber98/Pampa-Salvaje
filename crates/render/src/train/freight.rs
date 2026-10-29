//! Los vagones de carga: un furgón de tablas verticales rojo óxido con puerta
//! corrediza, un bastidor de hierro y un techo curvo de chapa acanalada vieja,
//! y una tolva granera abierta de acero colmada de trigo sobre dos bocas de
//! descarga. Ambos van sobre bogies de ruedas de disco.

use std::f32::consts::PI;

use bevy::prelude::*;

use super::builder::CarBuilder;
use super::kit::Mat;
use super::shape::{Grain, Shape};

impl CarBuilder<'_, '_> {
    pub fn boxcar(&mut self, length: f32) {
        self.underframe(length);
        self.bogies(length);
        let body = length - 0.7;
        let (floor, eave) = (1.3, 3.72);
        // La caja de tablas, con las tablas verticales, y un piso de tablones
        // que se ve por debajo.
        self.boxed_upright(
            Mat::OXIDE,
            Vec3::new(0.0, (floor + eave) / 2.0, 0.0),
            Vec3::new(body, eave - floor, 2.8),
        );
        // El techo abovedado de chapa vieja, que sobresale de los laterales.
        let reach = length / 2.0 - 0.1;
        let roof: Vec<Vec2> = (0..=12)
            .map(|i| {
                let a = PI * i as f32 / 12.0;
                Vec2::new(1.5 * a.cos(), eave + 0.34 * a.sin())
            })
            .collect();
        self.add(
            Mat::OLD_IRON,
            Shape::along_x(&roof, -reach, reach, true, true),
        );
        // Una pasarela a lo largo de la cumbrera, al ras de la parte superior.
        self.boxed(
            Mat::VARNISH,
            Vec3::new(0.0, 4.04, 0.0),
            Vec3::new(length - 1.6, 0.04, 0.5),
        );
        for side in [-1.0, 1.0] {
            let z = side * 1.4;
            // Bastidor de hierro en los laterales: montantes, un larguero
            // superior y uno inferior, y riostras diagonales a cada lado de la
            // puerta.
            for x in (-4..=4).map(|i| i as f32 * (body - 0.2) / 8.0) {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, (floor + eave) / 2.0, z + side * 0.012),
                    Vec3::new(0.08, eave - floor, 0.03),
                );
            }
            for y in [floor + 0.05, eave - 0.05] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(0.0, y, z + side * 0.012),
                    Vec3::new(body, 0.1, 0.03),
                );
            }
            for dx in [-1.0, 1.0] {
                let near = Vec3::new(dx * 1.1, floor + 0.1, z + side * 0.02);
                let far = Vec3::new(dx * (body / 2.0 - 0.3), eave - 0.1, z + side * 0.02);
                self.bar(Mat::BLACK, near, far, 0.03);
            }
            // La puerta corrediza: una hoja barnizada colgada de un riel, con
            // una barra de cierre y su guía.
            self.boxed_upright(
                Mat::PLANKS,
                Vec3::new(0.3, (floor + 0.2 + eave - 0.15) / 2.0, z + side * 0.035),
                Vec3::new(1.9, eave - floor - 0.35, 0.05),
            );
            for x in [-0.65, 1.25] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x + 0.0, (floor + eave) / 2.0, z + side * 0.065),
                    Vec3::new(0.07, eave - floor - 0.3, 0.02),
                );
            }
            self.bar(
                Mat::BLACK,
                Vec3::new(-0.65, floor + 0.3, z + side * 0.08),
                Vec3::new(1.25, eave - 0.3, z + side * 0.08),
                0.025,
            );
            self.boxed(
                Mat::BLACK,
                Vec3::new(0.1, eave - 0.07, z + side * 0.07),
                Vec3::new(3.9, 0.06, 0.05),
            );
            self.boxed(
                Mat::BLACK,
                Vec3::new(-0.5, 2.3, z + side * 0.09),
                Vec3::new(0.12, 0.05, 0.04),
            );
            // Persianas bajo los aleros en los extremos.
            for end in [-1.0, 1.0] {
                for k in (0..3).map(|k| k as f32) {
                    self.boxed(
                        Mat::Dark,
                        Vec3::new(
                            end * (body / 2.0 - 0.9),
                            eave - 0.4 - k * 0.1,
                            z + side * 0.005,
                        ),
                        Vec3::new(0.5, 0.05, 0.012),
                    );
                }
            }
        }
        // Columna y volante del freno de mano, y escaleras en los extremos.
        for end in [-1.0, 1.0] {
            let x = end * (body / 2.0 + 0.08);
            self.boxed_upright(
                Mat::OXIDE,
                Vec3::new(x - end * 0.04, (floor + eave) / 2.0, 0.0),
                Vec3::new(0.04, eave - floor, 2.8),
            );
            for z in [-1.2, 1.2] {
                self.bar(
                    Mat::BLACK,
                    Vec3::new(x + end * 0.05, floor, z),
                    Vec3::new(x + end * 0.05, eave, z),
                    0.02,
                );
            }
            for k in (0..6).map(|k| k as f32) {
                self.bar(
                    Mat::BLACK,
                    Vec3::new(x + end * 0.05, floor + 0.35 + k * 0.4, -1.2),
                    Vec3::new(x + end * 0.05, floor + 0.35 + k * 0.4, -0.85),
                    0.015,
                );
            }
        }
        let brake = body / 2.0 + 0.2;
        self.bar(
            Mat::BLACK,
            Vec3::new(brake, floor, 0.8),
            Vec3::new(brake, 3.3, 0.8),
            0.025,
        );
        self.turned(
            Mat::BLACK,
            &[
                (-0.02, 0.17),
                (-0.02, 0.2),
                (0.02, 0.2),
                (0.02, 0.17),
                (-0.02, 0.17),
            ],
            14,
            Vec3::new(brake, 3.36, 0.8),
            Vec3::Y,
        );
    }

    pub fn hopper(&mut self, length: f32) {
        self.underframe(length);
        self.bogies(length);
        let body = length - 0.8;
        let (rim, shoulder) = (3.42, 2.35);
        // La tolva: laterales rectos de acero sobre dos embudos.
        self.boxed(
            Mat::STEEL,
            Vec3::new(0.0, (rim + shoulder) / 2.0, 0.0),
            Vec3::new(body, rim - shoulder, 2.8),
        );
        for x in [-length / 4.0, length / 4.0] {
            self.add(Mat::STEEL, funnel(x, shoulder, 1.6, body / 2.0 - 0.1, 2.7));
            // La compuerta de descarga y su volante.
            self.boxed(
                Mat::BLACK,
                Vec3::new(x, 1.55, 0.0),
                Vec3::new(0.7, 0.2, 0.7),
            );
            self.turned(
                Mat::BLACK,
                &[
                    (-0.02, 0.12),
                    (-0.02, 0.15),
                    (0.02, 0.15),
                    (0.02, 0.12),
                    (-0.02, 0.12),
                ],
                12,
                Vec3::new(x, 1.55, 0.55),
                Vec3::Z,
            );
        }
        // Trigo colmado, redondeado.
        let heap: Vec<Vec2> = (0..=12)
            .map(|i| {
                let a = PI * i as f32 / 12.0;
                Vec2::new(1.28 * a.cos(), rim - 0.12 + 0.26 * a.sin())
            })
            .collect();
        self.add(
            Mat::Surface(crate::facade::Surface::Thatch),
            Shape::along_x(&heap, -body / 2.0 + 0.1, body / 2.0 - 0.1, true, true),
        );
        // Refuerzos, ángulo del borde y costuras.
        for side in [-1.0, 1.0] {
            let z = side * 1.4;
            for x in (-4..=4).map(|i| i as f32 * (body - 0.4) / 8.0) {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, (rim + shoulder) / 2.0, z + side * 0.04),
                    Vec3::new(0.12, rim - shoulder, 0.08),
                );
            }
            self.boxed(
                Mat::BLACK,
                Vec3::new(0.0, rim, z + side * 0.04),
                Vec3::new(body + 0.1, 0.1, 0.1),
            );
            self.boxed(
                Mat::BLACK,
                Vec3::new(0.0, shoulder, z + side * 0.02),
                Vec3::new(body, 0.08, 0.06),
            );
        }
        for end in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(end * (body / 2.0 + 0.02), rim, 0.0),
                Vec3::new(0.1, 0.1, 2.9),
            );
            // Escalera en el extremo, columna y volante del freno de mano.
            for z in [-1.1, -0.75] {
                self.bar(
                    Mat::BLACK,
                    Vec3::new(end * (body / 2.0 + 0.1), 1.3, z),
                    Vec3::new(end * (body / 2.0 + 0.1), rim, z),
                    0.02,
                );
            }
            for k in (0..5).map(|k| k as f32) {
                self.bar(
                    Mat::BLACK,
                    Vec3::new(end * (body / 2.0 + 0.1), 1.65 + k * 0.38, -1.1),
                    Vec3::new(end * (body / 2.0 + 0.1), 1.65 + k * 0.38, -0.75),
                    0.014,
                );
            }
            self.bar(
                Mat::BLACK,
                Vec3::new(end * (body / 2.0 + 0.2), 1.3, 0.9),
                Vec3::new(end * (body / 2.0 + 0.2), 3.0, 0.9),
                0.025,
            );
            self.turned(
                Mat::BLACK,
                &[
                    (-0.02, 0.17),
                    (-0.02, 0.2),
                    (0.02, 0.2),
                    (0.02, 0.17),
                    (-0.02, 0.17),
                ],
                14,
                Vec3::new(end * (body / 2.0 + 0.2), 3.05, 0.9),
                Vec3::Y,
            );
        }
    }
}

/// Un tronco de pirámide invertido de chapas planas de acero centrado en `x`:
/// desde `top` (una boca de `length` por `width`) hasta una salida de 0.5 m en
/// `bottom`.
fn funnel(x: f32, top: f32, bottom: f32, length: f32, width: f32) -> Shape {
    let mut shape = Shape::default();
    let (tl, tw) = (length, width / 2.0);
    let (bl, bw) = (0.25, 0.25);
    let up = |dx: f32, dz: f32, y: f32| Vec3::new(x + dx, y, dz);
    let centre = Vec3::new(x, (top + bottom) / 2.0, 0.0);
    for (a, b) in [
        ((1.0, -1.0), (1.0, 1.0)),
        ((1.0, 1.0), (-1.0, 1.0)),
        ((-1.0, 1.0), (-1.0, -1.0)),
        ((-1.0, -1.0), (1.0, -1.0)),
    ] {
        let mut quad = [
            up(a.0 * tl, a.1 * tw, top),
            up(b.0 * tl, b.1 * tw, top),
            up(b.0 * bl, b.1 * bw, bottom),
            up(a.0 * bl, a.1 * bw, bottom),
        ];
        // Caras hacia afuera, sin importar en qué sentido iban las esquinas.
        let normal = (quad[1] - quad[0]).cross(quad[3] - quad[0]);
        if normal.dot((quad[0] + quad[2]) / 2.0 - centre) < 0.0 {
            quad.reverse();
        }
        shape.flat(quad, Grain::Along);
    }
    shape
}
