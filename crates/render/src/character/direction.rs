//! Picks one of eight sprite directions from a continuous heading.
//!
//! This is the only place the continuous logic heading gets quantized, and it
//! happens purely for drawing, relative to where the camera looks.

use pampa_core::Heading;
use std::f32::consts::{FRAC_PI_4, TAU};

/// Angular width of one view sector.
const SECTOR: f32 = FRAC_PI_4;

/// Number of directional frames in a rider sprite sheet.
pub const DIRECTIONS: usize = 8;

/// Which of the eight drawn views to show, relative to the camera.
///
/// Index `k` means the rider faces `k * 45°` counter-clockwise from the
/// camera's view direction (seen from above):
/// `0` = facing away (back view), `2` = facing screen-left,
/// `4` = facing the camera (front view), `6` = facing screen-right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewDirection(u8);

impl ViewDirection {
    pub const AWAY: Self = Self(0);
    pub const LEFT: Self = Self(2);
    pub const TOWARD: Self = Self(4);
    pub const RIGHT: Self = Self(6);

    /// Builds a direction from a frame index, wrapping modulo 8.
    pub fn from_index(index: usize) -> Self {
        Self((index % DIRECTIONS) as u8)
    }

    /// Nearest of the eight views for a rider with `heading`, seen by a camera
    /// whose ground-plane view direction is `camera_yaw` (radians, same
    /// convention as [`Heading`]).
    pub fn pick(heading: Heading, camera_yaw: f32) -> Self {
        let relative = (heading.radians() - camera_yaw).rem_euclid(TAU);
        let index = (relative / SECTOR).round() as usize;
        Self::from_index(index)
    }

    /// Sprite sheet column for this view.
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// Relative facing angle this view depicts, in radians.
    pub fn relative_angle(self) -> f32 {
        self.0 as f32 * SECTOR
    }
}

/// Ground-plane yaw (core convention: CCW from east, `y` north) of a Bevy
/// world-space direction, where north is -Z. Returns `None` for directions
/// with no horizontal component (camera looking straight down).
pub fn ground_yaw(world_dir_x: f32, world_dir_z: f32) -> Option<f32> {
    if world_dir_x.hypot(world_dir_z) < 1e-6 {
        return None;
    }
    Some((-world_dir_z).atan2(world_dir_x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_8, PI, TAU};

    fn pick(heading: f32, camera_yaw: f32) -> ViewDirection {
        ViewDirection::pick(Heading::from_radians(heading), camera_yaw)
    }

    #[test]
    fn same_direction_as_camera_shows_back() {
        assert_eq!(pick(0.0, 0.0), ViewDirection::AWAY);
        assert_eq!(pick(1.3, 1.3), ViewDirection::AWAY);
    }

    #[test]
    fn cardinal_relative_views() {
        assert_eq!(pick(FRAC_PI_2, 0.0), ViewDirection::LEFT);
        assert_eq!(pick(PI, 0.0), ViewDirection::TOWARD);
        assert_eq!(pick(-FRAC_PI_2, 0.0), ViewDirection::RIGHT);
    }

    #[test]
    fn every_octant_maps_to_its_index() {
        for camera in [0.0, 0.7, -2.0, 5.5] {
            for k in 0..8 {
                let heading = camera + k as f32 * FRAC_PI_4;
                assert_eq!(pick(heading, camera).index(), k, "camera {camera}, k {k}");
            }
        }
    }

    #[test]
    fn picks_nearest_and_wraps_around() {
        assert_eq!(pick(FRAC_PI_8 - 0.01, 0.0).index(), 0);
        assert_eq!(pick(FRAC_PI_8 + 0.01, 0.0).index(), 1);
        assert_eq!(pick(TAU - 0.1, 0.0).index(), 0);
        assert_eq!(pick(0.1, TAU - 0.1).index(), 0);
    }

    #[test]
    fn only_depends_on_relative_angle() {
        // Rotating camera and rider together never changes the drawn view.
        for step in 0..32 {
            let offset = step as f32 * 0.37;
            assert_eq!(pick(2.0 + offset, 0.5 + offset), pick(2.0, 0.5));
        }
    }

    #[test]
    fn relative_angle_round_trips() {
        for k in 0..8 {
            let view = ViewDirection::from_index(k);
            assert_eq!(pick(view.relative_angle(), 0.0), view);
        }
        assert_eq!(ViewDirection::from_index(9).index(), 1);
    }

    #[test]
    fn ground_yaw_maps_bevy_axes_to_core_convention() {
        let eps = 1e-5;
        assert!((ground_yaw(1.0, 0.0).unwrap() - 0.0).abs() < eps); // +X = east
        assert!((ground_yaw(0.0, -1.0).unwrap() - FRAC_PI_2).abs() < eps); // -Z = north
        assert!((ground_yaw(-1.0, 0.0).unwrap().abs() - PI).abs() < eps); // west
        assert!(ground_yaw(0.0, 0.0).is_none());
    }
}
