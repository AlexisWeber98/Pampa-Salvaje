//! Ruedas: una llanta de acero con su pestaña del lado de adentro, y un centro
//! de rayos (con contrapeso en las motrices) o un disco embutido.
//!
//! La mesh de una rueda gira alrededor de su eje Y local, que el constructor
//! del vagón coloca a lo largo del eje; `side` indica para qué lado del vagón
//! es (+1 a la derecha): la pestaña va del lado de adentro, hacia el centro de
//! la vía.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;

use super::shape::{Grain, Shape};

/// La mitad de la distancia entre los centros de un par de llantas: los rieles
/// están a 0.838 m del centro de la vía.
pub const WHEEL_Z: f32 = 0.86;
/// El ancho de una llanta.
pub const TYRE_WIDTH: f32 = 0.14;
/// Facetas alrededor de una rueda de rayos, y de una rueda de disco.
const SPOKED_SIDES: usize = 20;
const PLATE_SIDES: usize = 16;
/// Cuánto baja una pestaña por debajo de la banda de rodadura, bajo la cara
/// superior del riel.
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
    /// Rayos redondos, esta cantidad.
    Spoked(usize),
    /// Un disco embutido.
    Plate,
}

/// La llanta, en el marco del torno (eje X, pestaña hacia -X), girada luego
/// para que su eje sea Y.
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

/// El centro: cubo, rayos o disco, y el contrapeso de una rueda motriz.
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
            // La pina donde terminan los rayos, dentro de la llanta.
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
                // Un contrapeso opuesto a la manivela, una cuña entre rayos.
                let reach = inner - 0.03;
                let wedge = [Vec2::ZERO]
                    .into_iter()
                    .chain((0..=4).map(|i| {
                        let a = FRAC_PI_2 - 0.45 + 0.9 * i as f32 / 4.0;
                        Vec2::new(a.cos(), a.sin()) * reach
                    }))
                    .collect::<Vec<_>>();
                // Estirado a lo largo de X, en (z, y).
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

/// Una pieza de rueda en el marco del torno (eje X, pestaña o lado interior
/// hacia -X) girada al eje Y, espejada para el lado izquierdo.
fn seated(shape: Shape, side: f32) -> Shape {
    let to_y = Transform::from_rotation(Quat::from_rotation_z(FRAC_PI_2));
    let shape = shape.transformed(to_y);
    if side < 0.0 {
        shape.transformed(Transform::from_rotation(Quat::from_rotation_x(PI)))
    } else {
        shape
    }
}
