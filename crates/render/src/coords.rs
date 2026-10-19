//! Mapping between `pampa-core` ground coordinates and Bevy world space.
//!
//! Core uses `x` east, `y` north on the ground plane plus a separate height.
//! Bevy is Y-up with -Z forward, so north maps to -Z.

use bevy::math::Vec3;
use pampa_core::Vec2;

/// World-space position of a ground-plane point raised to `height`.
pub fn world_from_ground(point: Vec2, height: f32) -> Vec3 {
    Vec3::new(point.x, height, -point.y)
}
