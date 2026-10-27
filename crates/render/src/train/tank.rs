//! The branch engine: a small British-built 2-6-2T tank engine, in worn
//! black with a brass dome and copper-capped chimney, drawn as one 12 m
//! car. Its water rides in side tanks along the boiler and its coal in a
//! bunker behind the cab, so there is no tender.
//!
//! From the front (+X): buffer beam, smokebox with its door, the short
//! chimney, cylinders and their guides, the leading wheels, the side
//! tanks along the boiler with its steam dome, three coupled drivers
//! under them, the cab with its roof, the trailing wheels and the coal
//! bunker.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;

use super::DRIVER_RADIUS;
use super::builder::CarBuilder;
use super::kit::{Mat, SMALL_RADIUS};
use super::loco::{BOILER_Y, boss, rod_bar};
use super::shape::{Grain, Shape, rounded_rect};
use super::wheels::WheelStyle;

/// Top of the running plate and footplate.
const PLATE: f32 = 1.6;
/// The coupled drivers' axles.
const DRIVERS: [f32; 3] = [-0.9, 0.8, 2.5];
/// The leading and trailing trucks' axles.
const LEADING: f32 = 4.9;
const TRAILING: f32 = -3.5;
/// The chimney's place along the car, and the height of its top, where the
/// smoke leaves.
const CHIMNEY_X: f32 = 4.8;
const CHIMNEY_TOP: f32 = 4.2;
/// The side tanks' span along the boiler, and their middle height.
const TANK: (f32, f32) = (-1.0, 3.5);
const TANK_Y: f32 = 2.35;
/// The cab: its front wall, back and roof.
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

    /// The plate frames, the buffer beams at both ends, the tread-plate
    /// running plate beside the smokebox and the footplate under cab and
    /// bunker.
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
        // Running plates ahead of the tanks, with a valance, and a front
        // plate across the beam.
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
        // The footplate under the cab and the bunker.
        let (x0, x1) = (-front + 0.5, CAB.1 + 0.1);
        self.boxed(
            Mat::TREAD,
            Vec3::new((x0 + x1) / 2.0, PLATE - 0.03, 0.0),
            Vec3::new(x1 - x0, 0.06, 2.8),
        );
    }

    /// The six drivers, the leading and trailing wheels, and the four rods
    /// the drivers' cranks swing.
    fn tank_wheels_and_rods(&mut self) {
        for x in DRIVERS {
            self.axle(x, DRIVER_RADIUS, WheelStyle::Spoked(12));
        }
        for x in [LEADING, TRAILING] {
            self.axle(x, SMALL_RADIUS, WheelStyle::Spoked(8));
        }
        // Axle boxes and springs on every axle.
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
        // Rods, as the tender engine's: the right side (+Z) cranks lead
        // by a quarter turn; the drivers turn clockwise.
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
            // The connecting rod, from the middle driver's pin to the
            // crosshead.
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

    /// The boiler with its firebox wrapper, smokebox, chimney, dome,
    /// safety valves, handrails and headlamp.
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
        // The smokebox door's ring and dart, and a number plate.
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
        // A short chimney, flared at the mouth, with a copper cap whose
        // top is the smoke's mouth.
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
        // The steam dome between the tanks, in brass, and the safety
        // valves behind the chimney.
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
        // Handrails along the smokebox, on knobbed posts, and a headlamp.
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

    /// Cylinders with their valve chests, the crosshead guides and the
    /// leading wheels' springs.
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
            // Steam pipe up to the smokebox, slide bars and a guide yoke.
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

    /// The side tanks along the boiler: rounded boxes with cream beading,
    /// riveted straps, a filler cap and a brass rail on top.
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
            // The filler cap on top, and a brass rail along the shoulder.
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

    /// The cab with its arched roof and spectacle glasses, and the coal
    /// bunker behind it, with the rear lamp.
    fn tank_cab(&mut self, front: f32) {
        let (back, wall) = CAB;
        let half = 1.3;
        let (floor, roof) = (PLATE, ROOF);
        // Side sheets: a window in the cab, a low rise round the bunker.
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
            // A brass number plate on the bunker, and the cab steps.
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
        // The front wall round the boiler, with spectacle glasses, and the
        // firebox door behind.
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
        // The roof: arched, overhanging the sheets.
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
        // The bunker: end wall, a coaming and the coal heaped in it.
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
        // A rear lamp bracket and handrail.
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
