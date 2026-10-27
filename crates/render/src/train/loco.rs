//! The locomotive: a British-built 2-6-0 tender engine of the Ferrocarril
//! Oeste, in worn black with a brass dome, copper-capped chimney, dark red
//! buffer beams and tread-plate footplates, drawn together with its
//! six-wheel tender as one 14 m car.
//!
//! From the front (+X): buffer beam, smokebox with its door, the chimney,
//! cylinders and their guides, the leading wheels, three coupled drivers
//! under splashers and a running plate, the boiler with brass bands, steam
//! and sand domes and safety valves, the cab with its roof, then the
//! tender with its coal.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;

use super::DRIVER_RADIUS;
use super::builder::{CarBuilder, bar_shape};
use super::kit::{Mat, SMALL_RADIUS};
use super::shape::{Grain, Shape, rounded_rect, stadium};
use super::wheels::WheelStyle;

/// Axle height of the boiler over the rail tops.
pub const BOILER_Y: f32 = 2.62;
/// Top of the running plate.
const PLATE: f32 = 1.6;
/// The coupled drivers' axles.
pub const DRIVERS: [f32; 3] = [0.5, 2.4, 4.3];
/// The leading truck's axle.
const LEADING: f32 = 5.95;
/// The tender's axles.
const TENDER_AXLES: [f32; 3] = [-6.1, -4.6, -3.1];
/// The chimney's place along the car, and the height of its top, where the
/// smoke leaves.
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

    /// The plate frames, the front buffer beam with its lamps and
    /// fittings, the running plate on each side with its valance and the
    /// splashers over the drivers.
    fn engine_frame(&mut self, front: f32) {
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(2.7, 1.1, side * 0.62),
                Vec3::new(8.4, 0.5, 0.06),
            );
        }
        // Cross stretchers between the plates.
        for x in [-1.0, 1.4, 3.4, 5.2] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(x, 1.1, 0.0),
                Vec3::new(0.12, 0.4, 1.2),
            );
        }
        // The buffer beam, its buffers and coupling, and two lamps.
        self.buffer_beam(front - 0.4, 1.0);
        self.boxed(
            Mat::BLACK,
            Vec3::new(front - 0.7, 0.95, 0.0),
            Vec3::new(0.5, 0.5, 1.0),
        );
        // Running plates of tread plate, a valance and the splashers.
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
        // The smokebox saddle and a front tread plate across the beam.
        self.boxed(
            Mat::TREAD,
            Vec3::new(front - 0.7, PLATE - 0.03, 0.0),
            Vec3::new(0.9, 0.06, 1.0),
        );
    }

    /// The six drivers, the leading and the tender's wheels, and the four
    /// rods the drivers' cranks swing.
    fn wheels_and_rods(&mut self) {
        for x in DRIVERS {
            self.axle(x, DRIVER_RADIUS, WheelStyle::Spoked(12));
        }
        self.axle(LEADING, SMALL_RADIUS, WheelStyle::Spoked(8));
        for x in TENDER_AXLES {
            self.axle(x, SMALL_RADIUS, WheelStyle::Spoked(8));
        }
        // Axle boxes under the frame, on the driving axles, and springs.
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
        // Rods: the right side (+Z) cranks lead by a quarter turn; the
        // drivers turn clockwise, toward a lower phase.
        for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let phase = (i as f32 * -FRAC_PI_2).rem_euclid(TAU);
            // The coupling rod across the three drivers.
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

    /// The boiler, smokebox, chimney, domes, valves, handrails.
    fn boiler(&mut self) {
        let y = BOILER_Y;
        // Barrel, smokebox and the dished door in one turning.
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
        // Brass boiler bands.
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
        // The smokebox door's ring and dart.
        let ring = Shape::lathe(&[(6.18, 0.74), (6.18, 0.8), (6.24, 0.8), (6.24, 0.74)], 28);
        self.put(Mat::BRASS, &ring, Transform::from_xyz(0.0, y, 0.0));
        self.turned(
            Mat::BRASS,
            &[(0.0, 0.0), (0.0, 0.08), (0.05, 0.11), (0.1, 0.0)],
            10,
            Vec3::new(6.45, y, 0.0),
            Vec3::X,
        );
        // Chimney: tapered, flared at the mouth, with a copper cap whose
        // top is the smoke's mouth.
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
        // Steam dome in polished brass over a black base, the sand dome.
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
        // Safety-valve columns ahead of the cab, a whistle on the cab front.
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
        // Handrails along the boiler, on knobbed posts.
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
        // A smokebox headlamp, and a lamp iron on each side of the beam.
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
        // A brass number plate on the smokebox door.
        self.boxed(
            Mat::BRASS,
            Vec3::new(6.38, y - 0.2, 0.0),
            Vec3::new(0.02, 0.18, 0.4),
        );
    }

    /// Cylinders with their covers and valve chests, the crosshead guides,
    /// piston rods and the bogie's frame.
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
            // Valve chest above, steam pipe to the smokebox.
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
            // Slide bars and the crosshead's guide yoke.
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
            // Cylinder drain cocks.
            self.boxed(
                Mat::BRASS,
                Vec3::new(5.5, 0.58, z),
                Vec3::new(0.06, 0.06, 0.06),
            );
        }
        // The leading truck's axle boxes and springs.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(LEADING, 0.5, side * 0.72),
                Vec3::new(0.22, 0.24, 0.1),
            );
        }
    }

    /// The cab: footplate, side sheets with windows, front wall with its
    /// spectacle glasses, the roof and the steps.
    fn cab(&mut self) {
        let (back, front) = (-1.7, 1.0);
        let half = 1.3;
        let (floor, roof) = (PLATE, 3.88);
        // Footplate, a bridge plate to the tender.
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
        // Side sheets: below and above the window, the pillars, the low
        // cut-away behind.
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
            // A brass edging round the window and a number plate.
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
            // The window glass.
            self.boxed(
                Mat::Dark,
                Vec3::new((window.0 + window.1) / 2.0, (window.2 + window.3) / 2.0, z),
                Vec3::new(window.1 - window.0, window.3 - window.2, 0.01),
            );
            // Cab steps of tread plate.
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
        // The front wall, round the boiler: the sides, over it and under it.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(front, (floor + roof) / 2.0, side * 1.07),
                Vec3::new(0.06, roof - floor, 0.46),
            );
            // Spectacle glasses in brass rings.
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
        // The backhead's firebox door, a pressure gauge, in the dark.
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
        // The roof: arched, overhanging the sheets.
        let mut ring: Vec<Vec2> = (0..=10)
            .map(|i| {
                let a = std::f32::consts::PI * i as f32 / 10.0;
                Vec2::new(1.5 * a.cos(), roof + 0.2 * a.sin())
            })
            .collect();
        // The last point is the first's mirror at the eaves: closed flat.
        ring.dedup();
        let roof_shape = Shape::along_x(&ring, back - 0.15, front + 0.35, true, true);
        self.add(Mat::BLACK, roof_shape);
        // Roof ventilator.
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

    /// The six-wheel tender: a rounded tank, its coal, hatches and rails,
    /// frames and the rear buffer beam.
    fn tender(&mut self, front: f32) {
        let (x0, x1) = (-front + 0.5, -2.0);
        // The tank's cross-section, rounded at its shoulders.
        let tank = rounded_rect(2.7, 1.7, 0.22, 5);
        let tank = tank
            .iter()
            .map(|p| *p + Vec2::new(0.0, 2.1))
            .collect::<Vec<_>>();
        self.add(Mat::BLACK, Shape::along_x(&tank, x0, x1, true, true));
        // The coal space: a coaming, and the heap.
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
        // Dark red lining stripe? Cream beading along the tank sides.
        for side in [-1.0, 1.0] {
            for y in [1.55, 2.6] {
                self.boxed(
                    Mat::Cream,
                    Vec3::new((x0 + x1) / 2.0 + 0.05, y, side * 1.352),
                    Vec3::new(x1 - x0 - 0.3, 0.025, 0.01),
                );
            }
            // Riveted vertical straps.
            for x in [x0 + 0.5, (x0 + x1) / 2.0, x1 - 0.5] {
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 2.1, side * 1.355),
                    Vec3::new(0.08, 1.4, 0.015),
                );
            }
            // Tender handrails and a water gauge cock.
            self.bar(
                Mat::BRASS,
                Vec3::new(x1 - 0.1, 2.9, side * 1.15),
                Vec3::new(x1 - 0.1, 3.3, side * 1.15),
                0.02,
            );
        }
        // Water filler hatch on top, behind the coal; tool box.
        self.turned(
            Mat::BLACK,
            &[(0.0, 0.0), (0.0, 0.3), (0.1, 0.27), (0.1, 0.0)],
            14,
            Vec3::new(x0 + 0.7, 3.0, 0.0),
            Vec3::Y,
        );
        // Frames, axle boxes and springs, the tender's brake hangers.
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
        // The rear buffer beam and its fittings.
        self.buffer_beam(-front + 0.4, -1.0);
        // The drawbar between engine and tender.
        self.add(
            Mat::BLACK,
            bar_shape(
                Vec3::new(-2.0, 1.05, 0.0),
                Vec3::new(-1.6, 1.05, 0.0),
                0.06,
                8,
            ),
        );
        // A rear lamp bracket.
        self.boxed(
            Mat::BLACK,
            Vec3::new(x0 + 0.05, 2.3, 0.0),
            Vec3::new(0.1, 0.3, 0.3),
        );
    }
}

/// A flat rod bar `length` long and `width` wide, 5 cm thick, in the XY
/// plane (thickness along Z), its ends round.
pub(super) fn rod_bar(length: f32, width: f32) -> Shape {
    Shape::along_z(&stadium(length, width, 5), -0.025, 0.025, false, true)
}

/// A pin boss on a rod: a short round boss at `x` along it.
pub(super) fn boss(x: f32, r: f32) -> Shape {
    Shape::lathe(&[(-0.04, 0.0), (-0.04, r), (0.07, r), (0.07, 0.0)], 10).transformed(
        Transform::from_xyz(x, 0.0, 0.0).with_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
    )
}

/// The splasher over a coupled driver: the segment of the circle of its
/// reach above the running plate, 0.46 m wide in Z about the origin.
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
