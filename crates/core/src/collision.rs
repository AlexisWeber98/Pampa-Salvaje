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

    /// Smallest displacement that moves this circle at `center` clear of
    /// `body`, or `None` if they do not overlap. Coincident centers part
    /// along `+x`, so the result stays deterministic.
    pub fn push_out_of_body(self, center: Vec2, body: &Body) -> Option<Vec2> {
        let offset = center - body.position;
        let distance = offset.length();
        let needed = self.radius + body.collider.radius;
        if distance >= needed {
            None
        } else if distance > 0.0 {
            Some(offset * ((needed - distance) / distance))
        } else {
            Some(Vec2::new(needed, 0.0))
        }
    }

    /// Whether this circle at `a` overlaps `other` at `b`.
    pub fn overlaps(self, a: Vec2, other: Circle, b: Vec2) -> bool {
        a.distance(b) < self.radius + other.radius
    }
}

/// Another character standing somewhere: a circle at a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub position: Vec2,
    pub collider: Circle,
}

/// A moving box that shoves circles sideways, along `across`, out to the side
/// of its center line they are on: a train car pushing riders off its track
/// instead of carrying them along in front of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SideObstacle {
    pub footprint: Aabb,
    /// Unit direction across the obstacle (across the track).
    pub across: Vec2,
}

impl SideObstacle {
    /// Displacement that moves a `circle` at `center` clear of this box along
    /// `across`, or `None` if they do not overlap.
    pub fn push_out(&self, circle: Circle, center: Vec2) -> Option<Vec2> {
        circle.push_out(center, &self.footprint)?;
        // Signed distance from the box's center line, and its half-width.
        let offset = (center - self.footprint.center()).dot(self.across);
        let size = self.footprint.size();
        let half = (self.across.x.abs() * size.x + self.across.y.abs() * size.y) / 2.0;
        let side = if offset >= 0.0 { 1.0 } else { -1.0 };
        Some(self.across * (side * (half + circle.radius) - offset))
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

    /// Where the segment from `from` to `to` crosses this box, as fractions
    /// `(enter, exit)` of the way along it, clipped to `0..=1`; `None` if it
    /// misses. A segment starting inside enters at `0`.
    pub fn segment_overlap(&self, from: Vec2, to: Vec2) -> Option<(f32, f32)> {
        // Slab method: intersect the parameter ranges inside each axis band.
        let d = to - from;
        let mut enter = 0.0_f32;
        let mut exit = 1.0_f32;
        for (start, delta, lo, hi) in [
            (from.x, d.x, self.min.x, self.max.x),
            (from.y, d.y, self.min.y, self.max.y),
        ] {
            if delta == 0.0 {
                if start < lo || start > hi {
                    return None;
                }
                continue;
            }
            let (a, b) = ((lo - start) / delta, (hi - start) / delta);
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
            if enter > exit {
                return None;
            }
        }
        Some((enter, exit))
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

    #[test]
    fn segment_overlap_reports_entry_and_exit_fractions() {
        let b = unit_box();
        // Straight through along x, from 2 m west to 2 m east.
        let (enter, exit) = b
            .segment_overlap(Vec2::new(-2.0, 1.0), Vec2::new(6.0, 1.0))
            .unwrap();
        assert!((enter - 0.25).abs() < 1e-6 && (exit - 0.75).abs() < 1e-6);
        // Reversed direction swaps which face is entered.
        let (enter, exit) = b
            .segment_overlap(Vec2::new(6.0, 1.0), Vec2::new(-2.0, 1.0))
            .unwrap();
        assert!((enter - 0.25).abs() < 1e-6 && (exit - 0.75).abs() < 1e-6);
        // Diagonal through a corner region.
        let (enter, _) = b
            .segment_overlap(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0))
            .unwrap();
        assert!((enter - 0.5).abs() < 1e-6);
    }

    #[test]
    fn segment_overlap_clips_to_the_segment() {
        let b = unit_box();
        // Starts inside: enters at 0.
        let (enter, exit) = b
            .segment_overlap(Vec2::new(2.0, 1.0), Vec2::new(8.0, 1.0))
            .unwrap();
        assert_eq!(enter, 0.0);
        assert!((exit - 2.0 / 6.0).abs() < 1e-6);
        // Ends inside: exits at 1.
        let (_, exit) = b
            .segment_overlap(Vec2::new(-2.0, 1.0), Vec2::new(1.0, 1.0))
            .unwrap();
        assert_eq!(exit, 1.0);
        // Fully inside, and a degenerate point segment inside.
        assert_eq!(
            b.segment_overlap(Vec2::new(1.0, 1.0), Vec2::new(3.0, 1.5)),
            Some((0.0, 1.0))
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0)),
            Some((0.0, 1.0))
        );
    }

    #[test]
    fn segment_overlap_misses() {
        let b = unit_box();
        // Stops short, passes beside, runs parallel outside, point outside.
        assert_eq!(
            b.segment_overlap(Vec2::new(-3.0, 1.0), Vec2::new(-1.0, 1.0)),
            None
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(-1.0, 3.0), Vec2::new(5.0, 2.5)),
            None
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(-1.0, 5.0), Vec2::new(9.0, 5.0)),
            None
        );
        assert_eq!(
            b.segment_overlap(Vec2::new(9.0, 9.0), Vec2::new(9.0, 9.0)),
            None
        );
    }

    fn car() -> SideObstacle {
        // A 10 m car along x, 3 m wide, centered on the origin.
        SideObstacle {
            footprint: Aabb::from_center_size(Vec2::ZERO, Vec2::new(10.0, 3.0)),
            across: Vec2::new(0.0, 1.0),
        }
    }

    #[test]
    fn side_obstacle_pushes_across_to_the_side_the_circle_is_on() {
        let c = Circle::new(1.0);
        assert_eq!(car().push_out(c, Vec2::new(0.0, 3.0)), None);
        // North half: out to the north face plus the radius.
        let push = car().push_out(c, Vec2::new(2.0, 0.5)).unwrap();
        assert!(close(push, Vec2::new(0.0, 2.0)), "{push:?}");
        // South of the box, overlapping its south face.
        let push = car().push_out(c, Vec2::new(-3.0, -2.0)).unwrap();
        assert!(close(push, Vec2::new(0.0, -0.5)), "{push:?}");
        // Right in front of the car's end: shoved sideways, never forward.
        let push = car().push_out(c, Vec2::new(5.5, -0.1)).unwrap();
        assert!(push.x == 0.0 && push.y < 0.0, "{push:?}");
    }

    #[test]
    fn circles_push_each_other_apart_along_the_line_between_them() {
        let c = Circle::new(0.9);
        let walker = Body {
            position: Vec2::new(1.0, 0.0),
            collider: Circle::new(0.3),
        };
        assert_eq!(c.push_out_of_body(Vec2::new(-0.3, 0.0), &walker), None);
        assert_eq!(c.push_out_of_body(Vec2::new(1.0, 1.2), &walker), None);
        let push = c.push_out_of_body(Vec2::new(0.0, 0.0), &walker).unwrap();
        assert!(close(push, Vec2::new(-0.2, 0.0)), "{push:?}");
        let push = c.push_out_of_body(Vec2::new(1.0, 0.6), &walker).unwrap();
        assert!(close(push, Vec2::new(0.0, 0.6)), "{push:?}");
        // Right on top of each other: apart along +x.
        let push = c.push_out_of_body(walker.position, &walker).unwrap();
        assert!(close(push, Vec2::new(1.2, 0.0)), "{push:?}");
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
