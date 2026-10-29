//! Ángulo de orientación continuo. La lógica nunca lo cuantiza.

use crate::math::Vec2;
use std::f32::consts::TAU;

/// Ángulo de orientación en radianes, antihorario desde el este, normalizado a
/// `[0, TAU)`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Heading(f32);

impl Heading {
    pub fn from_radians(radians: f32) -> Self {
        let wrapped = radians.rem_euclid(TAU);
        // `rem_euclid` puede redondear hasta exactamente TAU con entradas
        // negativas muy pequeñas.
        Self(if wrapped >= TAU { 0.0 } else { wrapped })
    }

    pub fn radians(self) -> f32 {
        self.0
    }

    /// Devuelve esta orientación girada `delta` radianes (positivo = a la
    /// izquierda).
    pub fn rotated(self, delta: f32) -> Self {
        Self::from_radians(self.0 + delta)
    }

    /// Vector unitario en el plano del suelo que apunta hacia donde mira esta
    /// orientación.
    pub fn forward(self) -> Vec2 {
        let (sin, cos) = self.0.sin_cos();
        Vec2::new(cos, sin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI, TAU};

    const EPS: f32 = 1e-5;

    #[test]
    fn normalizes_into_zero_to_tau() {
        assert!((Heading::from_radians(-FRAC_PI_2).radians() - 3.0 * FRAC_PI_2).abs() < EPS);
        assert!((Heading::from_radians(TAU + 0.25).radians() - 0.25).abs() < EPS);
        assert!(Heading::from_radians(TAU).radians().abs() < EPS);
    }

    #[test]
    fn forward_follows_ccw_from_east_convention() {
        let east = Heading::from_radians(0.0).forward();
        assert!((east.x - 1.0).abs() < EPS && east.y.abs() < EPS);
        let north = Heading::from_radians(FRAC_PI_2).forward();
        assert!(north.x.abs() < EPS && (north.y - 1.0).abs() < EPS);
        let west = Heading::from_radians(PI).forward();
        assert!((west.x + 1.0).abs() < EPS && west.y.abs() < EPS);
    }

    #[test]
    fn rotation_is_continuous_and_wraps() {
        let h = Heading::from_radians(0.1).rotated(-0.3);
        assert!((h.radians() - (TAU - 0.2)).abs() < EPS);
        let small = Heading::from_radians(0.0).rotated(0.0123);
        assert!((small.radians() - 0.0123).abs() < EPS);
    }
}
