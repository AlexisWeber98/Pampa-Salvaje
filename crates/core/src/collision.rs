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

    /// Smallest displacement that moves this circle at `center` out of `aabb`,
    /// or `None` if they do not overlap.
    pub fn push_out(self, center: Vec2, aabb: &Aabb) -> Option<Vec2> {
        let closest = aabb.closest_point(center);
        let offset = center - closest;
        let distance = offset.length();
        if distance >= self.radius {
            return None;
        }
        if distance > 0.0 {
            return Some(offset * ((self.radius - distance) / distance));
        }
        // Center inside the box: leave through the nearest edge.
        let exits = [
            (center.x - aabb.min.x, Vec2::new(-1.0, 0.0)),
            (aabb.max.x - center.x, Vec2::new(1.0, 0.0)),
            (center.y - aabb.min.y, Vec2::new(0.0, -1.0)),
            (aabb.max.y - center.y, Vec2::new(0.0, 1.0)),
        ];
        let (depth, normal) = exits
            .into_iter()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .expect("four exits");
        Some(normal * (depth + self.radius))
    }

    /// Whether this circle at `a` overlaps `other` at `b`.
    pub fn overlaps(self, a: Vec2, other: Circle, b: Vec2) -> bool {
        a.distance(b) < self.radius + other.radius
    }
}

/// Axis-aligned box on the ground plane: a static obstacle footprint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub const fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    /// Box of `size` (width along `x`, depth along `y`) centered on `center`.
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let half = size * 0.5;
        Self::new(center - half, center + half)
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Whether the two boxes share interior area (touching edges do not count).
    pub fn intersects(&self, other: &Aabb) -> bool {
        self.min.x < other.max.x
            && other.min.x < self.max.x
            && self.min.y < other.max.y
            && other.min.y < self.max.y
    }

    /// Whether `other` lies fully inside this box (edges included).
    pub fn contains(&self, other: &Aabb) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && other.max.x <= self.max.x
            && other.max.y <= self.max.y
    }

    pub fn contains_point(&self, p: Vec2) -> bool {
        (self.min.x..=self.max.x).contains(&p.x) && (self.min.y..=self.max.y).contains(&p.y)
    }

    /// Point of this box closest to `p`.
    pub fn closest_point(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            p.x.clamp(self.min.x, self.max.x),
            p.y.clamp(self.min.y, self.max.y),
        )
    }

    /// The box grown by `margin` on every side.
    pub fn expanded(&self, margin: f32) -> Self {
        Self::new(
            self.min - Vec2::new(margin, margin),
            self.max + Vec2::new(margin, margin),
        )
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

    #[test]
    fn aabb_overlap_and_containment() {
        let a = Aabb::new(Vec2::ZERO, Vec2::new(4.0, 2.0));
        assert!(a.intersects(&Aabb::from_center_size(
            Vec2::new(4.0, 1.0),
            Vec2::new(1.0, 1.0)
        )));
        assert!(!a.intersects(&Aabb::new(Vec2::new(4.0, 0.0), Vec2::new(5.0, 2.0))));
        assert!(a.contains(&Aabb::new(Vec2::new(1.0, 0.0), Vec2::new(4.0, 1.0))));
        assert_eq!(a.center(), Vec2::new(2.0, 1.0));
        assert_eq!(a.closest_point(Vec2::new(9.0, -3.0)), Vec2::new(4.0, 0.0));
        assert_eq!(a.expanded(1.0).size(), Vec2::new(6.0, 4.0));
    }

    fn unit_box() -> Aabb {
        Aabb::new(Vec2::ZERO, Vec2::new(4.0, 2.0))
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-5
    }

    #[test]
    fn separate_circle_needs_no_push() {
        let c = Circle::new(1.0);
        assert_eq!(c.push_out(Vec2::new(-1.5, 1.0), &unit_box()), None);
        assert_eq!(c.push_out(Vec2::new(-1.0, 1.0), &unit_box()), None);
        // Near a corner but outside the rounded corner region.
        assert_eq!(c.push_out(Vec2::new(-0.8, -0.8), &unit_box()), None);
    }

    #[test]
    fn pushes_out_through_the_nearest_face() {
        let c = Circle::new(1.0);
        let push = c.push_out(Vec2::new(-0.5, 1.0), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(-0.5, 0.0)), "{push:?}");
        let push = c.push_out(Vec2::new(2.0, 2.25), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(0.0, 0.75)), "{push:?}");
    }

    #[test]
    fn pushes_out_diagonally_at_corners() {
        let c = Circle::new(1.0);
        let center = Vec2::new(4.5, 2.5);
        let push = c.push_out(center, &unit_box()).unwrap();
        let resolved = center + push;
        assert!((resolved.distance(Vec2::new(4.0, 2.0)) - 1.0).abs() < 1e-5);
        assert!((push.x - push.y).abs() < 1e-5 && push.x > 0.0);
    }

    #[test]
    fn center_inside_box_exits_through_the_closest_edge() {
        let c = Circle::new(1.0);
        // Closest to the bottom edge (y = 0).
        let push = c.push_out(Vec2::new(1.5, 0.4), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(0.0, -1.4)), "{push:?}");
        // Closest to the right edge (x = 4).
        let push = c.push_out(Vec2::new(3.8, 1.0), &unit_box()).unwrap();
        assert!(close(push, Vec2::new(1.2, 0.0)), "{push:?}");
    }
}
