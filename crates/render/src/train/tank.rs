//! La locomotora del ramal: una pequeña locomotora tanque 2-6-2T de fabricación
//! británica, en negro gastado con domo de bronce y chimenea con remate de
//! cobre, dibujada como un solo vagón de 12 m. El agua va en tanques laterales
//! a lo largo de la caldera y el carbón en una carbonera detrás de la cabina,
//! así que no tiene ténder.
//!
//! Desde el frente (+X): testero, caja de humo con su puerta, la chimenea
//! corta, los cilindros y sus guías, las ruedas delanteras, los tanques
//! laterales a lo largo de la caldera con su domo de vapor, tres ejes motores
//! acoplados debajo de ellos, la cabina con su techo, las ruedas traseras y la
//! carbonera.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;

use super::DRIVER_RADIUS;
use super::builder::CarBuilder;
use super::kit::{Mat, SMALL_RADIUS};
use super::loco::{BOILER_Y, boss, rod_bar};
use super::shape::{Grain, Shape, rounded_rect};
use super::wheels::WheelStyle;

/// Parte superior de la plataforma lateral y de la plancha del maquinista.
const PLATE: f32 = 1.6;
/// Los ejes de las ruedas motrices acopladas.
const DRIVERS: [f32; 3] = [-0.9, 0.8, 2.5];
/// Los ejes de los bogies delantero y trasero.
const LEADING: f32 = 4.9;
const TRAILING: f32 = -3.5;
/// La posición de la chimenea a lo largo del vagón y la altura de su remate,
/// por donde sale el humo.
const CHIMNEY_X: f32 = 4.8;
const CHIMNEY_TOP: f32 = 4.2;
/// El tramo de los tanques laterales a lo largo de la caldera y su altura
/// media.
const TANK: (f32, f32) = (-1.0, 3.5);
const TANK_Y: f32 = 2.35;
/// La cabina: su pared delantera, la trasera y el techo.
const CAB: (f32, f32) = (-3.0, -1.0);
const ROOF: f32 = 3.88;

impl CarBuilder<'_, '_> {
    pub fn tank_engine(&mut self, length: f32) {
        let front = length / 2.0;
        self.tank_frame(front);
        self.tank_wheels_and_rods();
        self.tank_boiler();
        self.tank_front_gear();
        self.side_tanks();
        self.tank_cab(front);
        self.smoke(Vec3::new(CHIMNEY_X, CHIMNEY_TOP, 0.0));
    }

    /// Los bastidores de chapa, los testeros en ambos extremos, la plataforma
    /// lateral de chapa antideslizante junto a la caja de humo y la plancha del
    /// maquinista bajo la cabina y la carbonera.
    fn tank_frame(&mut self, front: f32) {
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(-0.2, 1.1, side * 0.62),
                Vec3::new(9.6, 0.5, 0.06),
            );
        }
        for x in [-4.4, -2.0, 0.0, 1.7, 3.4, 4.4] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(x, 1.1, 0.0),
                Vec3::new(0.12, 0.4, 1.2),
            );
        }
        self.buffer_beam(front - 0.4, 1.0);
        self.buffer_beam(-front + 0.4, -1.0);
        self.boxed(
            Mat::BLACK,
            Vec3::new(front - 0.7, 0.95, 0.0),
            Vec3::new(0.5, 0.5, 1.0),
        );
        // Plataformas laterales delante de los tanques, con un faldón, y una
        // chapa frontal que cruza el testero.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::TREAD,
                Vec3::new(4.45, PLATE - 0.03, side * 1.1),
                Vec3::new(1.9, 0.06, 0.6),
            );
            self.boxed(
                Mat::BLACK,
                Vec3::new(4.45, PLATE - 0.11, side * 1.4),
                Vec3::new(1.9, 0.16, 0.04),
            );
        }
        self.boxed(
            Mat::TREAD,
            Vec3::new(front - 0.7, PLATE - 0.03, 0.0),
            Vec3::new(0.9, 0.06, 1.0),
        );
        // La plancha del maquinista bajo la cabina y la carbonera.
        let (x0, x1) = (-front + 0.5, CAB.1 + 0.1);
        self.boxed(
            Mat::TREAD,
            Vec3::new((x0 + x1) / 2.0, PLATE - 0.03, 0.0),
            Vec3::new(x1 - x0, 0.06, 2.8),
        );
    }

    /// Las seis ruedas motrices, las ruedas delanteras y traseras, y las cuatro
    /// bielas que hacen girar las manivelas de las motrices.
    fn tank_wheels_and_rods(&mut self) {
        for x in DRIVERS {
            self.axle(x, DRIVER_RADIUS, WheelStyle::Spoked(12));
        }
        for x in [LEADING, TRAILING] {
            self.axle(x, SMALL_RADIUS, WheelStyle::Spoked(8));
        }
        // Cajas de grasa y resortes en todos los ejes.
        let spring = Shape::cuboid(Vec3::ZERO, Vec3::new(0.9, 0.05, 0.06), Grain::Along);
        for x in DRIVERS {
            for side in [-1.0, 1.0] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 0.85, side * 0.7),
                    Vec3::new(0.3, 0.3, 0.12),
                );
                self.put(
                    Mat::BLACK,
                    &spring,
                    Transform::from_xyz(x, 1.35, side * 0.7),
                );
            }
        }
        for x in [LEADING, TRAILING] {
            for side in [-1.0, 1.0] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 0.5, side * 0.72),
                    Vec3::new(0.22, 0.24, 0.1),
                );
            }
        }
        // Bielas, como en la locomotora con ténder: las manivelas del lado
        // derecho (+Z) van un cuarto de vuelta adelantadas; las motrices giran
        // en sentido horario.
        for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let phase = (i as f32 * -FRAC_PI_2).rem_euclid(TAU);
            let pitch = DRIVERS[1] - DRIVERS[0];
            let mut coupling = rod_bar(2.0 * pitch + 0.3, 0.12);
            for k in -1..=1 {
                coupling.append(&boss(k as f32 * pitch, 0.09));
            }
            self.rod(
                coupling.translated(Vec3::new(0.0, 0.0, side * 1.03)),
                Vec3::new(DRIVERS[1], DRIVER_RADIUS, 0.0),
                phase,
                0.0,
            );
            // La biela motriz, desde el muñón de la rueda motriz central hasta
            // la cruceta.
            let reach = 2.6;
            let mut main = rod_bar(reach + 0.2, 0.1);
            main.append(&boss(-reach / 2.0, 0.08));
            self.rod(
                main.translated(Vec3::new(0.0, 0.0, side * 1.10)),
                Vec3::new(DRIVERS[1] + reach / 2.0, DRIVER_RADIUS, 0.0),
                phase,
                reach,
            );
        }
    }

    /// La caldera con el revestimiento del hogar, la caja de humo, la chimenea,
    /// el domo, las válvulas de seguridad, los pasamanos y el farol delantero.
    fn tank_boiler(&mut self) {
        let y = BOILER_Y;
        let body = Shape::lathe(
            &[
                (-1.0, 0.0),
                (-1.0, 0.86),
                (-0.4, 0.86),
                (0.1, 0.8),
                (3.4, 0.78),
                (3.4, 0.8),
                (5.2, 0.8),
                (5.27, 0.76),
                (5.35, 0.66),
                (5.42, 0.34),
                (5.46, 0.0),
            ],
            28,
        );
        self.put(Mat::BLACK, &body, Transform::from_xyz(0.0, y, 0.0));
        for (x, r) in [(0.6, 0.815), (1.8, 0.80), (3.0, 0.785), (5.1, 0.80)] {
            let band = Shape::lathe(
                &[
                    (x - 0.05, r),
                    (x - 0.05, r + 0.02),
                    (x + 0.05, r + 0.02),
                    (x + 0.05, r),
                ],
                28,
            );
            self.put(Mat::BRASS, &band, Transform::from_xyz(0.0, y, 0.0));
        }
        // El aro y el cierre de la puerta de la caja de humo, y una placa de
        // número.
        let ring = Shape::lathe(&[(5.18, 0.74), (5.18, 0.8), (5.24, 0.8), (5.24, 0.74)], 28);
        self.put(Mat::BRASS, &ring, Transform::from_xyz(0.0, y, 0.0));
        self.turned(
            Mat::BRASS,
            &[(0.0, 0.0), (0.0, 0.08), (0.05, 0.11), (0.1, 0.0)],
            10,
            Vec3::new(5.45, y, 0.0),
            Vec3::X,
        );
        self.boxed(
            Mat::BRASS,
            Vec3::new(5.38, y - 0.2, 0.0),
            Vec3::new(0.02, 0.18, 0.4),
        );
        // Una chimenea corta, acampanada en la boca, con un remate de cobre
        // cuya parte superior es la boca del humo.
        let base = y + 0.78 - 0.1;
        let height = CHIMNEY_TOP - base;
        self.turned(
            Mat::BLACK,
            &[
                (0.0, 0.0),
                (0.0, 0.34),
                (0.1, 0.24),
                (0.2, 0.19),
                (0.6, 0.19),
                (0.68, 0.23),
                (0.74, 0.23),
                (0.74, 0.0),
            ],
            16,
            Vec3::new(CHIMNEY_X, base, 0.0),
            Vec3::Y,
        );
        self.turned(
            Mat::BRASS,
            &[
                (0.72, 0.0),
                (0.72, 0.25),
                (height - 0.05, 0.29),
                (height, 0.27),
                (height, 0.20),
                (height, 0.0),
            ],
            16,
            Vec3::new(CHIMNEY_X, base, 0.0),
            Vec3::Y,
        );
        // El domo de vapor entre los tanques, de bronce, y las válvulas de
        // seguridad detrás de la chimenea.
        self.turned(
            Mat::BRASS,
            &[
                (0.0, 0.0),
                (0.0, 0.40),
                (0.08, 0.34),
                (0.24, 0.30),
                (0.36, 0.30),
                (0.46, 0.25),
                (0.52, 0.14),
                (0.54, 0.0),
            ],
            20,
            Vec3::new(1.4, y + 0.66, 0.0),
            Vec3::Y,
        );
        for z in [-0.14, 0.14] {
            self.turned(
                Mat::BRASS,
                &[
                    (0.0, 0.0),
                    (0.0, 0.1),
                    (0.22, 0.08),
                    (0.42, 0.11),
                    (0.5, 0.06),
                    (0.52, 0.0),
                ],
                12,
                Vec3::new(-0.45, y + 0.74, z),
                Vec3::Y,
            );
        }
        // Pasamanos a lo largo de la caja de humo, sobre soportes con perilla,
        // y un farol delantero.
        for side in [-1.0, 1.0] {
            let z = side * 0.9;
            self.bar(
                Mat::BRASS,
                Vec3::new(3.5, y + 0.38, z),
                Vec3::new(5.0, y + 0.38, z),
                0.025,
            );
            for x in [3.6, 4.9] {
                self.bar(
                    Mat::BRASS,
                    Vec3::new(x, y + 0.2, side * 0.78),
                    Vec3::new(x, y + 0.38, z),
                    0.02,
                );
            }
        }
        let lamp = Shape::cuboid(Vec3::ZERO, Vec3::new(0.3, 0.42, 0.3), Grain::Along);
        self.put(Mat::BLACK, &lamp, Transform::from_xyz(5.0, y + 0.98, 0.0));
        self.boxed(
            Mat::Lamp,
            Vec3::new(5.155, y + 0.98, 0.0),
            Vec3::new(0.02, 0.26, 0.2),
        );
        self.boxed(
            Mat::BLACK,
            Vec3::new(5.0, y + 0.76, 0.0),
            Vec3::new(0.14, 0.08, 0.14),
        );
    }

    /// Los cilindros con sus cajas de válvulas, las guías de la cruceta y los
    /// resortes de las ruedas delanteras.
    fn tank_front_gear(&mut self) {
        let cylinder = Shape::lathe(
            &[
                (0.0, 0.0),
                (0.0, 0.2),
                (0.04, 0.24),
                (0.8, 0.24),
                (0.84, 0.2),
                (0.84, 0.0),
            ],
            16,
        );
        for side in [-1.0, 1.0] {
            let z = side * 1.18;
            self.put(Mat::BLACK, &cylinder, Transform::from_xyz(3.7, 0.85, z));
            self.turned(
                Mat::BLACK,
                &[(0.0, 0.0), (0.0, 0.12), (0.7, 0.12), (0.7, 0.0)],
                12,
                Vec3::new(3.75, 1.28, z),
                Vec3::X,
            );
            // Tubo de vapor hasta la caja de humo, correderas y un yugo de
            // guía.
            self.bar(
                Mat::BLACK,
                Vec3::new(4.35, 1.35, z - side * 0.2),
                Vec3::new(4.5, 2.2, side * 0.78),
                0.05,
            );
            for dy in [0.11, -0.11] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(3.45, 0.85 + dy, side * 1.10),
                    Vec3::new(0.5, 0.025, 0.05),
                );
            }
            self.boxed(
                Mat::BLACK,
                Vec3::new(3.25, 0.85, side * 1.17),
                Vec3::new(0.06, 0.3, 0.12),
            );
            self.boxed(
                Mat::BRASS,
                Vec3::new(3.85, 0.58, z),
                Vec3::new(0.06, 0.06, 0.06),
            );
        }
    }

    /// Los tanques laterales a lo largo de la caldera: cajas redondeadas con
    /// molduras color crema, flejes remachados, una tapa de carga y una baranda
    /// de bronce arriba.
    fn side_tanks(&mut self) {
        let (x0, x1) = TANK;
        let section = rounded_rect(0.46, 1.2, 0.14, 4);
        let mid = (x0 + x1) / 2.0;
        for side in [-1.0, 1.0] {
            let z = side * 1.07;
            let ring: Vec<Vec2> = section.iter().map(|p| *p + Vec2::new(z, TANK_Y)).collect();
            self.add(Mat::BLACK, Shape::along_x(&ring, x0, x1, true, true));
            for y in [TANK_Y - 0.4, TANK_Y + 0.4] {
                self.boxed(
                    Mat::Cream,
                    Vec3::new(mid, y, side * 1.302),
                    Vec3::new(x1 - x0 - 0.3, 0.025, 0.01),
                );
            }
            for x in [x0 + 0.5, mid, x1 - 0.5] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, TANK_Y, side * 1.305),
                    Vec3::new(0.08, 0.9, 0.015),
                );
            }
            // La tapa de carga arriba, y una baranda de bronce a lo largo del
            // hombro.
            self.turned(
                Mat::BRASS,
                &[(0.0, 0.0), (0.0, 0.16), (0.05, 0.14), (0.05, 0.0)],
                12,
                Vec3::new(x1 - 0.8, TANK_Y + 0.6, z),
                Vec3::Y,
            );
            self.bar(
                Mat::BRASS,
                Vec3::new(x0 + 0.2, TANK_Y + 0.66, side * 1.25),
                Vec3::new(x1 - 0.2, TANK_Y + 0.66, side * 1.25),
                0.02,
            );
        }
    }

    /// La cabina con su techo abovedado y sus vidrios frontales, y la carbonera
    /// detrás de ella, con el farol trasero.
    fn tank_cab(&mut self, front: f32) {
        let (back, wall) = CAB;
        let half = 1.3;
        let (floor, roof) = (PLATE, ROOF);
        // Chapas laterales: una ventana en la cabina, una elevación baja
        // alrededor de la carbonera.
        let window = (-2.6, -1.5, 2.5, 3.2);
        for side in [-1.0, 1.0] {
            let z = side * half;
            let mut sheet = |x0: f32, x1: f32, y0: f32, y1: f32| {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new((x0 + x1) / 2.0, (y0 + y1) / 2.0, z),
                    Vec3::new(x1 - x0, y1 - y0, 0.05),
                );
            };
            sheet(back, wall, floor, window.2);
            sheet(back, wall, window.3, roof);
            sheet(back, window.0, window.2, window.3);
            sheet(window.1, wall, window.2, window.3);
            sheet(-front + 0.5, back, floor, 3.1);
            let rim = |x0: f32, x1: f32, y0: f32, y1: f32| {
                Shape::cuboid(
                    Vec3::new((x0 + x1) / 2.0, (y0 + y1) / 2.0, z + side * 0.03),
                    Vec3::new(x1 - x0, y1 - y0, 0.02),
                    Grain::Along,
                )
            };
            for shape in [
                rim(window.0, window.1, window.2, window.2 + 0.04),
                rim(window.0, window.1, window.3 - 0.04, window.3),
                rim(window.0, window.0 + 0.04, window.2, window.3),
                rim(window.1 - 0.04, window.1, window.2, window.3),
            ] {
                self.add(Mat::BRASS, shape);
            }
            self.boxed(
                Mat::Dark,
                Vec3::new((window.0 + window.1) / 2.0, (window.2 + window.3) / 2.0, z),
                Vec3::new(window.1 - window.0, window.3 - window.2, 0.01),
            );
            // Una placa de número de bronce en la carbonera, y los estribos de
            // la cabina.
            self.boxed(
                Mat::BRASS,
                Vec3::new(-4.2, 2.45, z + side * 0.03),
                Vec3::new(0.5, 0.2, 0.02),
            );
            for y in [1.2, 0.85] {
                self.boxed(
                    Mat::TREAD,
                    Vec3::new(-1.7, y, side * (half + 0.05)),
                    Vec3::new(0.5, 0.05, 0.3),
                );
            }
            for x in [-2.0, -1.4] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 1.0, side * (half + 0.03)),
                    Vec3::new(0.04, 0.9, 0.04),
                );
            }
        }
        // La pared delantera alrededor de la caldera, con los vidrios
        // frontales, y la puerta del hogar detrás.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(wall, (floor + roof) / 2.0, side * 1.07),
                Vec3::new(0.06, roof - floor, 0.46),
            );
            self.turned(
                Mat::Dark,
                &[(0.0, 0.0), (0.0, 0.17)],
                14,
                Vec3::new(wall - 0.035, 2.95, side * 1.07),
                -Vec3::X,
            );
            self.turned(
                Mat::BRASS,
                &[(0.0, 0.17), (0.0, 0.21), (0.03, 0.21), (0.03, 0.17)],
                14,
                Vec3::new(wall - 0.03, 2.95, side * 1.07),
                -Vec3::X,
            );
        }
        self.boxed(
            Mat::BLACK,
            Vec3::new(wall, 3.64 + (roof - 3.64) / 2.0, 0.0),
            Vec3::new(0.06, roof - 3.64, 1.8),
        );
        self.boxed(
            Mat::BLACK,
            Vec3::new(wall, (floor + 1.8) / 2.0, 0.0),
            Vec3::new(0.06, 1.8 - floor, 1.8),
        );
        self.boxed(
            Mat::Dark,
            Vec3::new(wall - 0.07, 2.0, 0.0),
            Vec3::new(0.02, 0.3, 0.36),
        );
        // El techo: abovedado, sobresaliendo de las chapas.
        let mut ring: Vec<Vec2> = (0..=10)
            .map(|i| {
                let a = PI * i as f32 / 10.0;
                Vec2::new(1.5 * a.cos(), roof + 0.2 * a.sin())
            })
            .collect();
        ring.dedup();
        self.add(
            Mat::BLACK,
            Shape::along_x(&ring, back - 0.15, wall + 0.35, true, true),
        );
        self.turned(
            Mat::BLACK,
            &[
                (0.0, 0.0),
                (0.0, 0.2),
                (0.08, 0.2),
                (0.14, 0.12),
                (0.16, 0.0),
            ],
            12,
            Vec3::new(-1.6, roof + 0.18, 0.0),
            Vec3::Y,
        );
        // La carbonera: pared trasera, un borde elevado y el carbón apilado
        // dentro.
        let (x0, x1) = (-front + 0.5, back);
        self.boxed(
            Mat::BLACK,
            Vec3::new(x0 + 0.05, 2.35, 0.0),
            Vec3::new(0.1, 1.5, 2.6),
        );
        self.boxed(
            Mat::BLACK,
            Vec3::new(x1 - 0.05, 2.0, 0.0),
            Vec3::new(0.1, 0.8, 2.6),
        );
        let heap: Vec<Vec2> = (0..=10)
            .map(|i| {
                let a = PI * i as f32 / 10.0;
                Vec2::new(1.2 * a.cos(), 3.05 + 0.55 * a.sin())
            })
            .collect();
        self.add(
            Mat::Coal,
            Shape::along_x(&heap, x0 + 0.15, x1 - 0.1, true, true),
        );
        // Un soporte de farol trasero y un pasamanos.
        self.boxed(
            Mat::BLACK,
            Vec3::new(x0 - 0.05, 2.95, 0.0),
            Vec3::new(0.1, 0.3, 0.3),
        );
        for side in [-1.0, 1.0] {
            self.bar(
                Mat::BRASS,
                Vec3::new(x0 + 0.1, 2.9, side * 1.15),
                Vec3::new(x0 + 0.1, 3.4, side * 1.15),
                0.02,
            );
        }
    }
}
