//! La locomotora: una locomotora con ténder 2-6-0 de fabricación británica del
//! Ferrocarril Oeste, en negro gastado con domo de bronce, chimenea con remate
//! de cobre, testeros rojo oscuro y planchas de chapa antideslizante, dibujada
//! junto con su ténder de seis ruedas como un solo vagón de 14 m.
//!
//! Desde el frente (+X): testero, caja de humo con su puerta, la chimenea, los
//! cilindros y sus guías, las ruedas delanteras, tres ejes motores acoplados
//! bajo guardarruedas y una plataforma lateral, la caldera con zunchos de
//! bronce, los domos de vapor y de arena y las válvulas de seguridad, la cabina
//! con su techo, y después el ténder con su carbón.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;

use super::DRIVER_RADIUS;
use super::builder::{CarBuilder, bar_shape};
use super::kit::{Mat, SMALL_RADIUS};
use super::shape::{Grain, Shape, rounded_rect, stadium};
use super::wheels::WheelStyle;

/// Altura del eje de la caldera sobre la cara superior de los rieles.
pub const BOILER_Y: f32 = 2.62;
/// Parte superior de la plataforma lateral.
const PLATE: f32 = 1.6;
/// Los ejes de las ruedas motrices acopladas.
pub const DRIVERS: [f32; 3] = [0.5, 2.4, 4.3];
/// El eje del bogie delantero.
const LEADING: f32 = 5.95;
/// Los ejes del ténder.
const TENDER_AXLES: [f32; 3] = [-6.1, -4.6, -3.1];
/// La posición de la chimenea a lo largo del vagón y la altura de su remate,
/// por donde sale el humo.
pub const CHIMNEY_X: f32 = 5.7;
pub const CHIMNEY_TOP: f32 = 4.55;

impl CarBuilder<'_, '_> {
    pub fn locomotive(&mut self, length: f32) {
        let front = length / 2.0;
        self.engine_frame(front);
        self.wheels_and_rods();
        self.boiler();
        self.running_gear_up_front();
        self.cab();
        self.tender(front);
        self.smoke(Vec3::new(CHIMNEY_X, CHIMNEY_TOP, 0.0));
    }

    /// Los bastidores de chapa, el testero delantero con sus faroles y
    /// accesorios, la plataforma lateral de cada lado con su faldón y los
    /// guardarruedas sobre las ruedas motrices.
    fn engine_frame(&mut self, front: f32) {
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(2.7, 1.1, side * 0.62),
                Vec3::new(8.4, 0.5, 0.06),
            );
        }
        // Travesaños entre las chapas.
        for x in [-1.0, 1.4, 3.4, 5.2] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(x, 1.1, 0.0),
                Vec3::new(0.12, 0.4, 1.2),
            );
        }
        // El testero, sus topes y el enganche, y dos faroles.
        self.buffer_beam(front - 0.4, 1.0);
        self.boxed(
            Mat::BLACK,
            Vec3::new(front - 0.7, 0.95, 0.0),
            Vec3::new(0.5, 0.5, 1.0),
        );
        // Plataformas laterales de chapa antideslizante, un faldón y los
        // guardarruedas.
        for side in [-1.0, 1.0] {
            let z = side * 1.18;
            self.boxed(
                Mat::TREAD,
                Vec3::new(3.725, PLATE - 0.03, z),
                Vec3::new(5.65, 0.06, 0.46),
            );
            self.boxed(
                Mat::BLACK,
                Vec3::new(3.725, PLATE - 0.11, side * 1.4),
                Vec3::new(5.65, 0.16, 0.04),
            );
        }
        let splasher = splasher_shape();
        for x in DRIVERS {
            for side in [-1.0, 1.0] {
                self.put(
                    Mat::BLACK,
                    &splasher,
                    Transform::from_xyz(x, 0.0, side * 1.18),
                );
            }
        }
        // La silla de la caja de humo y una chapa antideslizante frontal que
        // cruza el testero.
        self.boxed(
            Mat::TREAD,
            Vec3::new(front - 0.7, PLATE - 0.03, 0.0),
            Vec3::new(0.9, 0.06, 1.0),
        );
    }

    /// Las seis ruedas motrices, las delanteras y las del ténder, y las cuatro
    /// bielas que hacen girar las manivelas de las motrices.
    fn wheels_and_rods(&mut self) {
        for x in DRIVERS {
            self.axle(x, DRIVER_RADIUS, WheelStyle::Spoked(12));
        }
        self.axle(LEADING, SMALL_RADIUS, WheelStyle::Spoked(8));
        for x in TENDER_AXLES {
            self.axle(x, SMALL_RADIUS, WheelStyle::Spoked(8));
        }
        // Cajas de grasa bajo el bastidor, en los ejes motores, y resortes.
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
        // Bielas: las manivelas del lado derecho (+Z) van un cuarto de vuelta
        // adelantadas; las motrices giran en sentido horario, hacia una fase
        // menor.
        for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let phase = (i as f32 * -FRAC_PI_2).rem_euclid(TAU);
            // La biela de acoplamiento a lo largo de las tres ruedas motrices.
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
            let reach = 2.8;
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

    /// La caldera, la caja de humo, la chimenea, los domos, las válvulas y los
    /// pasamanos.
    fn boiler(&mut self) {
        let y = BOILER_Y;
        // El cuerpo, la caja de humo y la puerta abombada en un solo torneado.
        let body = Shape::lathe(
            &[
                (0.8, 0.0),
                (0.8, 0.82),
                (1.2, 0.82),
                (5.25, 0.78),
                (5.25, 0.8),
                (6.2, 0.8),
                (6.27, 0.76),
                (6.35, 0.66),
                (6.42, 0.34),
                (6.46, 0.0),
            ],
            28,
        );
        self.put(Mat::BLACK, &body, Transform::from_xyz(0.0, y, 0.0));
        // Zunchos de bronce de la caldera.
        for (x, r) in [(1.7, 0.815), (2.9, 0.80), (4.1, 0.785), (5.15, 0.78)] {
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
        // El aro y el cierre de la puerta de la caja de humo.
        let ring = Shape::lathe(&[(6.18, 0.74), (6.18, 0.8), (6.24, 0.8), (6.24, 0.74)], 28);
        self.put(Mat::BRASS, &ring, Transform::from_xyz(0.0, y, 0.0));
        self.turned(
            Mat::BRASS,
            &[(0.0, 0.0), (0.0, 0.08), (0.05, 0.11), (0.1, 0.0)],
            10,
            Vec3::new(6.45, y, 0.0),
            Vec3::X,
        );
        // Chimenea: cónica, acampanada en la boca, con un remate de cobre cuya
        // parte superior es la boca del humo.
        let base = y + 0.78 - 0.1;
        self.turned(
            Mat::BLACK,
            &[
                (0.0, 0.0),
                (0.0, 0.36),
                (0.12, 0.25),
                (0.25, 0.19),
                (0.9, 0.19),
                (1.0, 0.23),
                (1.08, 0.23),
                (1.08, 0.0),
            ],
            16,
            Vec3::new(CHIMNEY_X, base, 0.0),
            Vec3::Y,
        );
        let height = CHIMNEY_TOP - base;
        self.turned(
            Mat::BRASS,
            &[
                (1.05, 0.0),
                (1.05, 0.25),
                (height - 0.06, 0.29),
                (height, 0.27),
                (height, 0.20),
                (height, 0.0),
            ],
            16,
            Vec3::new(CHIMNEY_X, base, 0.0),
            Vec3::Y,
        );
        // Domo de vapor de bronce pulido sobre una base negra, y el domo de
        // arena.
        let dome = [
            (0.0, 0.0),
            (0.0, 0.40),
            (0.08, 0.34),
            (0.24, 0.30),
            (0.36, 0.30),
            (0.46, 0.25),
            (0.52, 0.14),
            (0.54, 0.0),
        ];
        self.turned(
            Mat::BRASS,
            &dome,
            20,
            Vec3::new(2.5, y + 0.66, 0.0),
            Vec3::Y,
        );
        self.turned(
            Mat::BLACK,
            &dome.map(|(h, r)| (h * 0.7, r * 0.7)),
            14,
            Vec3::new(3.75, y + 0.68, 0.0),
            Vec3::Y,
        );
        // Columnas de válvulas de seguridad delante de la cabina, y un silbato
        // en el frente de la cabina.
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
                Vec3::new(1.35, y + 0.74, z),
                Vec3::Y,
            );
        }
        self.turned(
            Mat::BRASS,
            &[(0.0, 0.0), (0.0, 0.04), (0.28, 0.035), (0.3, 0.0)],
            8,
            Vec3::new(1.15, y + 0.75, 0.3),
            Vec3::Y,
        );
        // Pasamanos a lo largo de la caldera, sobre soportes con perilla.
        for side in [-1.0, 1.0] {
            let z = side * 0.9;
            self.bar(
                Mat::BRASS,
                Vec3::new(1.5, y + 0.38, z),
                Vec3::new(6.1, y + 0.38, z),
                0.025,
            );
            for x in [1.6, 3.0, 4.4, 5.9] {
                self.bar(
                    Mat::BRASS,
                    Vec3::new(x, y + 0.2, side * 0.78),
                    Vec3::new(x, y + 0.38, z),
                    0.02,
                );
            }
        }
        // Un farol en la caja de humo, y un soporte de farol a cada lado del
        // testero.
        let lamp = Shape::cuboid(Vec3::ZERO, Vec3::new(0.3, 0.42, 0.3), Grain::Along);
        self.put(Mat::BLACK, &lamp, Transform::from_xyz(6.0, y + 0.98, 0.0));
        self.boxed(
            Mat::Lamp,
            Vec3::new(6.155, y + 0.98, 0.0),
            Vec3::new(0.02, 0.26, 0.2),
        );
        self.boxed(
            Mat::BLACK,
            Vec3::new(6.0, y + 0.76, 0.0),
            Vec3::new(0.14, 0.08, 0.14),
        );
        // Una placa de número de bronce en la puerta de la caja de humo.
        self.boxed(
            Mat::BRASS,
            Vec3::new(6.38, y - 0.2, 0.0),
            Vec3::new(0.02, 0.18, 0.4),
        );
    }

    /// Los cilindros con sus tapas y cajas de válvulas, las guías de la
    /// cruceta, los vástagos de los pistones y el bastidor del bogie.
    fn running_gear_up_front(&mut self) {
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
            self.put(Mat::BLACK, &cylinder, Transform::from_xyz(5.35, 0.85, z));
            // La caja de válvulas arriba, y el tubo de vapor hasta la caja de
            // humo.
            self.turned(
                Mat::BLACK,
                &[(0.0, 0.0), (0.0, 0.12), (0.7, 0.12), (0.7, 0.0)],
                12,
                Vec3::new(5.4, 1.28, z),
                Vec3::X,
            );
            self.bar(
                Mat::BLACK,
                Vec3::new(5.7, 1.35, z - side * 0.2),
                Vec3::new(6.0, 2.2, side * 0.78),
                0.05,
            );
            // Correderas y el yugo de guía de la cruceta.
            for dy in [0.11, -0.11] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(5.1, 0.85 + dy, side * 1.10),
                    Vec3::new(0.5, 0.025, 0.05),
                );
            }
            self.boxed(
                Mat::BLACK,
                Vec3::new(4.9, 0.85, side * 1.17),
                Vec3::new(0.06, 0.3, 0.12),
            );
            // Purgadores de los cilindros.
            self.boxed(
                Mat::BRASS,
                Vec3::new(5.5, 0.58, z),
                Vec3::new(0.06, 0.06, 0.06),
            );
        }
        // Las cajas de grasa y los resortes del bogie delantero.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(LEADING, 0.5, side * 0.72),
                Vec3::new(0.22, 0.24, 0.1),
            );
        }
    }

    /// La cabina: la plancha del maquinista, las chapas laterales con ventanas,
    /// la pared delantera con sus vidrios frontales, el techo y los estribos.
    fn cab(&mut self) {
        let (back, front) = (-1.7, 1.0);
        let half = 1.3;
        let (floor, roof) = (PLATE, 3.88);
        // La plancha del maquinista, y una plancha de paso hacia el ténder.
        self.boxed(
            Mat::TREAD,
            Vec3::new((back + front) / 2.0 - 0.1, floor - 0.03, 0.0),
            Vec3::new(front - back + 0.2, 0.06, 2.8),
        );
        self.boxed(
            Mat::TREAD,
            Vec3::new(-1.85, floor - 0.03, 0.0),
            Vec3::new(0.35, 0.06, 1.6),
        );
        // Chapas laterales: por debajo y por encima de la ventana, los parantes
        // y el recorte bajo de atrás.
        let window = (-0.35, 0.65, 2.5, 3.2);
        for side in [-1.0, 1.0] {
            let z = side * half;
            let mut sheet = |x0: f32, x1: f32, y0: f32, y1: f32| {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new((x0 + x1) / 2.0, (y0 + y1) / 2.0, z),
                    Vec3::new(x1 - x0, y1 - y0, 0.05),
                );
            };
            sheet(-0.95, front, floor, window.2);
            sheet(-0.95, front, window.3, roof);
            sheet(-0.95, window.0, window.2, window.3);
            sheet(window.1, front, window.2, window.3);
            sheet(back, -0.95, floor, 2.55);
            // Un ribete de bronce alrededor de la ventana y una placa de
            // número.
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
                Mat::BRASS,
                Vec3::new(0.15, 2.05, z + side * 0.03),
                Vec3::new(0.5, 0.2, 0.02),
            );
            // El vidrio de la ventana.
            self.boxed(
                Mat::Dark,
                Vec3::new((window.0 + window.1) / 2.0, (window.2 + window.3) / 2.0, z),
                Vec3::new(window.1 - window.0, window.3 - window.2, 0.01),
            );
            // Estribos de la cabina de chapa antideslizante.
            for (x, y) in [(-1.2, 1.2), (-1.2, 0.85)] {
                self.boxed(
                    Mat::TREAD,
                    Vec3::new(x, y, side * (half + 0.05)),
                    Vec3::new(0.5, 0.05, 0.3),
                );
            }
            self.boxed(
                Mat::BLACK,
                Vec3::new(-1.45, 1.0, side * (half + 0.03)),
                Vec3::new(0.04, 0.9, 0.04),
            );
            self.boxed(
                Mat::BLACK,
                Vec3::new(-0.95, 1.0, side * (half + 0.03)),
                Vec3::new(0.04, 0.9, 0.04),
            );
        }
        // La pared delantera, alrededor de la caldera: los costados, por encima
        // y por debajo de ella.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(front, (floor + roof) / 2.0, side * 1.07),
                Vec3::new(0.06, roof - floor, 0.46),
            );
            // Vidrios frontales en aros de bronce.
            self.turned(
                Mat::Dark,
                &[(0.0, 0.0), (0.0, 0.17)],
                14,
                Vec3::new(front - 0.035, 2.95, side * 1.07),
                -Vec3::X,
            );
            self.turned(
                Mat::BRASS,
                &[(0.0, 0.17), (0.0, 0.21), (0.03, 0.21), (0.03, 0.17)],
                14,
                Vec3::new(front - 0.03, 2.95, side * 1.07),
                -Vec3::X,
            );
        }
        self.boxed(
            Mat::BLACK,
            Vec3::new(front, 3.64 + (roof - 3.64) / 2.0, 0.0),
            Vec3::new(0.06, roof - 3.64, 1.8),
        );
        self.boxed(
            Mat::BLACK,
            Vec3::new(front, (floor + 1.8) / 2.0, 0.0),
            Vec3::new(0.06, 1.8 - floor, 1.8),
        );
        // La puerta del hogar en el frente de la caldera, un manómetro, en la
        // oscuridad.
        self.boxed(
            Mat::Dark,
            Vec3::new(front - 0.07, 2.0, 0.0),
            Vec3::new(0.02, 0.3, 0.36),
        );
        self.turned(
            Mat::BRASS,
            &[(0.0, 0.0), (0.0, 0.08), (0.03, 0.08), (0.03, 0.0)],
            10,
            Vec3::new(front - 0.08, 3.0, -0.4),
            -Vec3::X,
        );
        // El techo: abovedado, sobresaliendo de las chapas.
        let mut ring: Vec<Vec2> = (0..=10)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / 10.0;
                Vec2::new(1.5 * a.cos(), roof + 0.2 * a.sin())
            })
            .collect();
        // El último punto es el espejo del primero en los aleros: cerrado
        // plano.
        ring.dedup();
        let roof_shape = Shape::along_x(&ring, back - 0.15, front + 0.35, true, true);
        self.add(Mat::BLACK, roof_shape);
        // Ventilación del techo.
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
            Vec3::new(-0.4, roof + 0.18, 0.0),
            Vec3::Y,
        );
    }

    /// El ténder de seis ruedas: un tanque redondeado, su carbón, escotillas y
    /// barandas, los bastidores y el testero trasero.
    fn tender(&mut self, front: f32) {
        let (x0, x1) = (-front + 0.5, -2.0);
        // La sección transversal del tanque, redondeada en los hombros.
        let tank = rounded_rect(2.7, 1.7, 0.22, 5);
        let tank = tank
            .iter()
            .map(|p| *p + Vec2::new(0.0, 2.1))
            .collect::<Vec<_>>();
        self.add(Mat::BLACK, Shape::along_x(&tank, x0, x1, true, true));
        // El espacio del carbón: un borde elevado y la pila.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new((x0 + x1) / 2.0, 3.0, side * 1.28),
                Vec3::new(x1 - x0 - 0.2, 0.2, 0.08),
            );
        }
        for end in [x0 + 0.15, x1 - 0.05] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(end, 3.0, 0.0),
                Vec3::new(0.08, 0.2, 2.6),
            );
        }
        let heap: Vec<Vec2> = (0..=10)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / 10.0;
                Vec2::new(1.15 * a.cos(), 2.98 + 0.6 * a.sin())
            })
            .collect();
        self.add(
            Mat::Coal,
            Shape::along_x(&heap, x0 + 0.3, x1 - 0.2, true, true),
        );
        // ¿Filete rojo oscuro? Molduras color crema a lo largo de los laterales
        // del tanque.
        for side in [-1.0, 1.0] {
            for y in [1.55, 2.6] {
                self.boxed(
                    Mat::Cream,
                    Vec3::new((x0 + x1) / 2.0 + 0.05, y, side * 1.352),
                    Vec3::new(x1 - x0 - 0.3, 0.025, 0.01),
                );
            }
            // Flejes verticales remachados.
            for x in [x0 + 0.5, (x0 + x1) / 2.0, x1 - 0.5] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 2.1, side * 1.355),
                    Vec3::new(0.08, 1.4, 0.015),
                );
            }
            // Pasamanos del ténder y un grifo indicador del nivel de agua.
            self.bar(
                Mat::BRASS,
                Vec3::new(x1 - 0.1, 2.9, side * 1.15),
                Vec3::new(x1 - 0.1, 3.3, side * 1.15),
                0.02,
            );
        }
        // La escotilla de carga de agua arriba, detrás del carbón; la caja de
        // herramientas.
        self.turned(
            Mat::BLACK,
            &[(0.0, 0.0), (0.0, 0.3), (0.1, 0.27), (0.1, 0.0)],
            14,
            Vec3::new(x0 + 0.7, 3.0, 0.0),
            Vec3::Y,
        );
        // Bastidores, cajas de grasa y resortes, y los colgantes del freno del
        // ténder.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new((x0 + x1) / 2.0, 1.1, side * 0.62),
                Vec3::new(x1 - x0 - 0.2, 0.3, 0.06),
            );
            for x in TENDER_AXLES {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 0.6, side * 1.0),
                    Vec3::new(0.24, 0.24, 0.1),
                );
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 0.95, side * 1.0),
                    Vec3::new(0.7, 0.05, 0.06),
                );
            }
        }
        self.boxed(
            Mat::BLACK,
            Vec3::new((x0 + x1) / 2.0, 1.28, 0.0),
            Vec3::new(x1 - x0 - 0.2, 0.15, 2.0),
        );
        // El testero trasero y sus accesorios.
        self.buffer_beam(-front + 0.4, -1.0);
        // La barra de tracción entre la locomotora y el ténder.
        self.add(
            Mat::BLACK,
            bar_shape(
                Vec3::new(-2.0, 1.05, 0.0),
                Vec3::new(-1.6, 1.05, 0.0),
                0.06,
                8,
            ),
        );
        // Un soporte de farol trasero.
        self.boxed(
            Mat::BLACK,
            Vec3::new(x0 + 0.05, 2.3, 0.0),
            Vec3::new(0.1, 0.3, 0.3),
        );
    }
}

/// Una barra plana de biela de `length` de largo y `width` de ancho, de 5 cm de
/// grosor, en el plano XY (el grosor a lo largo de Z), con los extremos
/// redondeados.
pub(super) fn rod_bar(length: f32, width: f32) -> Shape {
    Shape::along_z(&stadium(length, width, 5), -0.025, 0.025, false, true)
}

/// Un saliente de muñón en una biela: un saliente redondo corto en `x` a lo
/// largo de ella.
pub(super) fn boss(x: f32, r: f32) -> Shape {
    Shape::lathe(&[(-0.04, 0.0), (-0.04, r), (0.07, r), (0.07, 0.0)], 10).transformed(
        Transform::from_xyz(x, 0.0, 0.0).with_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
    )
}

/// El guardarruedas sobre una rueda motriz acoplada: el segmento del círculo de
/// su alcance por encima de la plataforma lateral, de 0.46 m de ancho en Z
/// alrededor del origen.
fn splasher_shape() -> Shape {
    let reach = DRIVER_RADIUS + 0.12;
    let up = PLATE - DRIVER_RADIUS;
    let start = (up / reach).asin();
    let ring: Vec<Vec2> = (0..=10)
        .map(|i| {
            let a = start + (std::f32::consts::PI - 2.0 * start) * i as f32 / 10.0;
            Vec2::new(a.cos() * reach, DRIVER_RADIUS + a.sin() * reach)
        })
        .collect();
    Shape::along_z(&ring, -0.23, 0.23, true, true)
}
