//! Collision shapes owned by logic, never derived from sprite pixels.

use crate::math::Vec2;

/// A circle on the ground plane (a vertical capsule seen from above).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub radius: f32,
}

impl Circle {
    pub const fn new(radius: f32) -> Self {
        Self { radius }
    }

    /// Whether this circle at `a` overlaps `other` at `b`.
    pub fn overlaps(self, a: Vec2, other: Circle, b: Vec2) -> bool {
        a.distance(b) < self.radius + other.radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_depends_on_radii_sum() {
        let c = Circle::new(1.0);
        assert!(c.overlaps(Vec2::ZERO, Circle::new(0.5), Vec2::new(1.4, 0.0)));
        assert!(!c.overlaps(Vec2::ZERO, Circle::new(0.5), Vec2::new(1.6, 0.0)));
    }
}
