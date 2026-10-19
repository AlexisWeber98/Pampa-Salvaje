//! The static world the rider moves through: terrain and playable bounds.

use crate::{Terrain, Vec2};

/// Half the side length of the square terrain patch, in meters.
///
/// Render draws exactly this patch, so the playable bounds and the drawn
/// ground always agree.
pub const PATCH_HALF_EXTENT: f32 = 300.0;

/// Axis-aligned playable area on the ground plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min: Vec2,
    pub max: Vec2,
}

/// Result of keeping a circle inside [`WorldBounds`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contained {
    pub position: Vec2,
    /// Unit direction the circle was pushed back in along `x`, if it was.
    pub push_x: Option<Vec2>,
    /// Unit direction the circle was pushed back in along `y`, if it was.
    pub push_y: Option<Vec2>,
}

impl WorldBounds {
    /// The drawn terrain patch, centered on the origin.
    pub const PATCH: Self = Self::centered(PATCH_HALF_EXTENT);

    /// A square of side `2 * half_extent` centered on the origin.
    pub const fn centered(half_extent: f32) -> Self {
        Self {
            min: Vec2::new(-half_extent, -half_extent),
            max: Vec2::new(half_extent, half_extent),
        }
    }

    /// Side lengths along `x` and `y`.
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Clamps a circle of `radius` at `center` so it lies fully inside.
    pub fn contain(&self, center: Vec2, radius: f32) -> Contained {
        let (x, push_x) = clamp_axis(center.x, self.min.x + radius, self.max.x - radius);
        let (y, push_y) = clamp_axis(center.y, self.min.y + radius, self.max.y - radius);
        Contained {
            position: Vec2::new(x, y),
            push_x: push_x.map(|sign| Vec2::new(sign, 0.0)),
            push_y: push_y.map(|sign| Vec2::new(0.0, sign)),
        }
    }
}

/// Clamps `value` into `[lo, hi]`, reporting the sign of the push, if any.
fn clamp_axis(value: f32, lo: f32, hi: f32) -> (f32, Option<f32>) {
    if value < lo {
        (lo, Some(1.0))
    } else if value > hi {
        (hi, Some(-1.0))
    } else {
        (value, None)
    }
}

/// Everything static the rider collides with or stands on.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub terrain: Terrain,
    pub bounds: WorldBounds,
}

impl World {
    /// Open terrain inside the drawn patch, with nothing built on it.
    pub fn open(terrain: Terrain) -> Self {
        Self {
            terrain,
            bounds: WorldBounds::PATCH,
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::open(Terrain::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_bounds_match_the_drawn_patch() {
        assert_eq!(WorldBounds::PATCH.size(), Vec2::new(600.0, 600.0));
        assert_eq!(WorldBounds::PATCH.min, Vec2::new(-300.0, -300.0));
    }

    #[test]
    fn inside_points_are_untouched() {
        let c = WorldBounds::PATCH.contain(Vec2::new(10.0, -20.0), 1.0);
        assert_eq!(c.position, Vec2::new(10.0, -20.0));
        assert_eq!((c.push_x, c.push_y), (None, None));
    }

    #[test]
    fn keeps_the_whole_circle_inside_and_reports_push_directions() {
        let c = WorldBounds::PATCH.contain(Vec2::new(400.0, -299.5), 1.0);
        assert_eq!(c.position, Vec2::new(299.0, -299.0));
        assert_eq!(c.push_x, Some(Vec2::new(-1.0, 0.0)));
        assert_eq!(c.push_y, Some(Vec2::new(0.0, 1.0)));
    }
}
