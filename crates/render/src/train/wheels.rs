//! Wheels: a steel tyre with its flange on the inside, and a centre of
//! spokes (with a counterweight on the drivers) or a dished plate.
//!
//! A wheel mesh turns about its local Y axis, which the car builder lays
//! along the axle; `side` says which side of the car it is for (+1 right):
//! the flange is on the inside, toward the track's middle.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;

use super::shape::{Grain, Shape};

/// Half the distance between a pair of tyres' middles: the rails stand
/// 0.838 m from the track's middle.
pub const WHEEL_Z: f32 = 0.86;
/// A tyre's width.
pub const TYRE_WIDTH: f32 = 0.14;
/// Facets round a spoked wheel, and a plate wheel.
const SPOKED_SIDES: usize = 20;
const PLATE_SIDES: usize = 16;
/// How far a flange reaches below the tread, under the rail top.
pub const FLANGE: f32 = 0.03;

impl WheelStyle {
    fn sides(self) -> usize {
        match self {
            WheelStyle::Spoked(_) => SPOKED_SIDES,
            WheelStyle::Plate => PLATE_SIDES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WheelStyle {
    /// Round spokes, this many.
    Spoked(usize),
    /// A dished plate.
    Plate,
}

/// The tyre, in the lathe frame (axis X, flange toward -X), then turned
/// so that its axis is Y.
pub fn wheel_tyre(radius: f32, style: WheelStyle, side: f32) -> Shape {
    let sides = style.sides();
    let (w, inner) = (TYRE_WIDTH / 2.0, radius * 0.88);
    let tyre = Shape::lathe(
        &[
            (-w, inner),
            (-w, radius + FLANGE),
            (-w + 0.025, radius + FLANGE),
            (-w + 0.045, radius),
            (w, radius),
            (w, inner),
        ],
        sides,
    );
    seated(tyre, side)
}

/// The centre: hub, spokes or plate, and a driver's counterweight.
pub fn wheel_centre(radius: f32, style: WheelStyle, side: f32) -> Shape {
    let inner = radius * 0.88;
    let mut centre = Shape::lathe(
        &[
            (-0.10, 0.0),
            (-0.10, 0.12),
            (0.07, 0.13),
            (0.07, 0.09),
            (0.14, 0.09),
            (0.14, 0.0),
        ],
        10,
    );
    match style {
        WheelStyle::Spoked(count) => {
            let length = inner - 0.1;
            let spoke = Shape::cuboid(
                Vec3::new(0.0, 0.1 + length / 2.0, 0.0),
                Vec3::new(0.07, length, 0.05),
                Grain::Along,
            );
            for k in 0..count {
                let angle = TAU * k as f32 / count as f32;
                centre.append(
                    &spoke.transformed(Transform::from_rotation(Quat::from_rotation_x(angle))),
                );
            }
            // The felloe the spokes meet, inside the tyre.
            centre.append(&Shape::lathe(
                &[
                    (-0.05, inner - 0.06),
                    (-0.05, inner),
                    (0.05, inner),
                    (0.05, inner - 0.06),
                    (-0.05, inner - 0.06),
                ],
                style.sides(),
            ));
            if radius > 0.7 {
                // A counterweight opposite the crank, a wedge between spokes.
                let reach = inner - 0.03;
                let wedge = [Vec2::ZERO]
                    .into_iter()
                    .chain((0..=4).map(|i| {
                        let a = FRAC_PI_2 - 0.45 + 0.9 * i as f32 / 4.0;
                        Vec2::new(a.cos(), a.sin()) * reach
                    }))
                    .collect::<Vec<_>>();
                // Pulled along X, in (z, y).
                let ring: Vec<Vec2> = wedge.iter().map(|p| Vec2::new(p.x, p.y)).collect();
                centre.append(&Shape::along_x(&ring, 0.01, 0.07, false, true));
            }
        }
        WheelStyle::Plate => {
            centre.append(&Shape::lathe(
                &[
                    (-0.03, 0.12),
                    (-0.012, inner),
                    (0.012, inner),
                    (0.03, 0.12),
                    (-0.03, 0.12),
                ],
                style.sides(),
            ));
        }
    }
    seated(centre, side)
}

/// A lathe-frame wheel part (axis X, flange or inside toward -X) turned to
/// axis Y, flipped for the left side.
fn seated(shape: Shape, side: f32) -> Shape {
    let to_y = Transform::from_rotation(Quat::from_rotation_z(FRAC_PI_2));
    let shape = shape.transformed(to_y);
    if side < 0.0 {
        shape.transformed(Transform::from_rotation(Quat::from_rotation_x(PI)))
    } else {
        shape
    }
}
