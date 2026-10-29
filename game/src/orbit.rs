//! Orbit math for the third-person camera, kept free of Bevy so it can be
//! tested on its own.
//!
//! The camera circles the rider on a sphere: `yaw` is the ground-plane
//! angle from the rider to the camera (radians, counter-clockwise from
//! east, like `Heading`), `pitch` its elevation above the horizon and
//! `distance` the sphere's radius. Yaw goes all the way around; pitch and
//! distance stay within limits. After a while without camera input, the
//! camera eases back behind a moving rider.

use pampa_core::Vec2;
use std::f32::consts::{PI, TAU};

/// Tuning for [`Orbit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitParams {
    /// Radians of orbit per pixel of mouse motion.
    pub mouse_sensitivity: f32,
    /// Radians per second of orbit while Q or E is held.
    pub key_rate: f32,
    /// Meters of zoom per wheel line.
    pub zoom_step: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// Lowest elevation above the horizon, in radians.
    pub min_pitch: f32,
    /// Highest elevation above the horizon, in radians.
    pub max_pitch: f32,
    /// Seconds without camera input before the camera recenters.
    pub recenter_delay: f32,
    /// Rider ground speed (m/s, absolute) needed to recenter.
    pub recenter_min_speed: f32,
    /// Exponential rate of the recenter ease (higher = quicker).
    pub recenter_rate: f32,
}

impl Default for OrbitParams {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 0.003,
            key_rate: 2.0,
            zoom_step: 1.0,
            min_distance: 6.0,
            max_distance: 25.0,
            min_pitch: 5f32.to_radians(),
            max_pitch: 60f32.to_radians(),
            recenter_delay: 2.0,
            recenter_min_speed: 0.5,
            recenter_rate: 2.5,
        }
    }
}

/// Camera input for one frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OrbitInput {
    /// Mouse motion in pixels: `x` right, `y` down.
    pub look: (f32, f32),
    /// `-1.0..=1.0`: positive orbits like moving the mouse right (E).
    pub keys: f32,
    /// Wheel lines: positive zooms in.
    pub zoom: f32,
}

impl OrbitInput {
    /// Whether the player touched the camera this frame.
    pub fn is_active(&self) -> bool {
        self.look != (0.0, 0.0) || self.keys != 0.0 || self.zoom != 0.0
    }
}

/// Where the camera sits on its orbit around the rider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    /// Ground angle from the rider to the camera, wrapped to `[0, TAU)`.
    pub yaw: f32,
    /// Elevation above the horizon, in radians.
    pub pitch: f32,
    /// Distance from the look-at point, in meters.
    pub distance: f32,
    /// Seconds since the last camera input.
    pub idle: f32,
}

impl Orbit {
    /// Default elevation: a little above the rider's head.
    pub const DEFAULT_PITCH: f32 = 0.26;
    /// Default distance from the look-at point, in meters.
    pub const DEFAULT_DISTANCE: f32 = 9.5;

    /// Right behind a rider facing `heading` (radians), at the default
    /// pitch and distance.
    pub fn behind(heading: f32) -> Self {
        Self {
            yaw: wrap(heading + PI),
            pitch: Self::DEFAULT_PITCH,
            distance: Self::DEFAULT_DISTANCE,
            idle: 0.0,
        }
    }

    /// Applies one frame of `input` and, after enough idle time, eases the
    /// camera behind a rider facing `heading` that moves at `speed`.
    /// Non-finite input or `dt` counts as none.
    pub fn update(
        &mut self,
        input: OrbitInput,
        heading: f32,
        speed: f32,
        params: &OrbitParams,
        dt: f32,
    ) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        let input = OrbitInput {
            look: (finite_or_zero(input.look.0), finite_or_zero(input.look.1)),
            keys: finite_or_zero(input.keys).clamp(-1.0, 1.0),
            zoom: finite_or_zero(input.zoom),
        };
        if input.is_active() {
            self.idle = 0.0;
            // Mouse right (or E) swings the view right: the camera goes
            // clockwise around the rider, seen from above.
            let turn = input.look.0 * params.mouse_sensitivity + input.keys * params.key_rate * dt;
            self.yaw = wrap(self.yaw - turn);
            self.pitch = (self.pitch + input.look.1 * params.mouse_sensitivity)
                .clamp(params.min_pitch, params.max_pitch);
            self.distance = (self.distance - input.zoom * params.zoom_step)
                .clamp(params.min_distance, params.max_distance);
            return;
        }
        self.idle += dt;
        let moving = speed.is_finite() && speed.abs() > params.recenter_min_speed;
        if self.idle >= params.recenter_delay && moving && heading.is_finite() {
            let behind = wrap(heading + PI);
            let ease = 1.0 - (-params.recenter_rate * dt).exp();
            self.yaw = wrap(self.yaw + angle_between(self.yaw, behind) * ease);
        }
    }

    /// Camera position relative to the look-at point: the ground-plane
    /// offset and the height above it.
    pub fn offset(&self) -> (Vec2, f32) {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let ground = self.distance * cos_pitch;
        (
            Vec2::new(cos_yaw * ground, sin_yaw * ground),
            self.distance * sin_pitch,
        )
    }
}

/// `angle` wrapped to `[0, TAU)`.
fn wrap(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(TAU);
    // `rem_euclid` can round up to exactly TAU for tiny negative inputs.
    if wrapped >= TAU { 0.0 } else { wrapped }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Signed shortest angle from `from` to `to`, in `(-PI, PI]`.
pub fn angle_between(from: f32, to: f32) -> f32 {
    let d = (to - from).rem_euclid(TAU);
    if d > PI { d - TAU } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    const DT: f32 = 1.0 / 60.0;

    fn params() -> OrbitParams {
        OrbitParams::default()
    }

    fn idle_for(orbit: &mut Orbit, heading: f32, speed: f32, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            orbit.update(OrbitInput::default(), heading, speed, &params(), DT);
        }
    }

    #[test]
    fn starts_behind_the_rider() {
        let orbit = Orbit::behind(FRAC_PI_2);
        let (ground, height) = orbit.offset();
        // Rider faces north: the camera is south of it, above the ground.
        assert!(ground.x.abs() < 1e-4 && ground.y < 0.0, "{ground:?}");
        assert!(height > 0.0);
        let length = (ground.length().powi(2) + height * height).sqrt();
        assert!((length - Orbit::DEFAULT_DISTANCE).abs() < 1e-3);
    }

    #[test]
    fn mouse_orbits_a_full_turn_and_wraps() {
        let p = params();
        let mut orbit = Orbit::behind(0.0);
        let start = orbit.offset().0;
        let mut quadrants = [false; 4];
        // One full turn in many small mouse moves to the right.
        let pixels = TAU / p.mouse_sensitivity;
        let frames = 240;
        for _ in 0..frames {
            let input = OrbitInput {
                look: (pixels / frames as f32, 0.0),
                ..Default::default()
            };
            orbit.update(input, 0.0, 0.0, &p, DT);
            assert!((0.0..TAU).contains(&orbit.yaw), "wrapped: {}", orbit.yaw);
            let g = orbit.offset().0;
            quadrants[usize::from(g.x < 0.0) + 2 * usize::from(g.y < 0.0)] = true;
        }
        assert_eq!(quadrants, [true; 4], "went all the way around");
        assert!(
            orbit.offset().0.distance(start) < 1e-2,
            "back where it began"
        );
        // Moving the mouse right swings the view right: the camera goes
        // clockwise (seen from above) around the rider.
        let mut right = Orbit::behind(0.0);
        right.update(
            OrbitInput {
                look: (10.0, 0.0),
                ..Default::default()
            },
            0.0,
            0.0,
            &p,
            DT,
        );
        assert!(angle_between(Orbit::behind(0.0).yaw, right.yaw) < 0.0);
    }

    #[test]
    fn keys_orbit_at_a_fixed_rate() {
        let p = params();
        let mut orbit = Orbit::behind(0.0);
        let before = orbit.yaw;
        let q = OrbitInput {
            keys: -1.0,
            ..Default::default()
        };
        orbit.update(q, 0.0, 0.0, &p, 0.5);
        assert!((angle_between(before, orbit.yaw) - p.key_rate * 0.5).abs() < 1e-4);
        let e = OrbitInput {
            keys: 1.0,
            ..Default::default()
        };
        orbit.update(e, 0.0, 0.0, &p, 0.5);
        assert!(angle_between(before, orbit.yaw).abs() < 1e-4);
    }

    #[test]
    fn pitch_is_clamped() {
        let p = params();
        let mut orbit = Orbit::behind(0.0);
        let down = OrbitInput {
            look: (0.0, 1e5),
            ..Default::default()
        };
        orbit.update(down, 0.0, 0.0, &p, DT);
        assert_eq!(orbit.pitch, p.max_pitch, "mouse down looks down from above");
        let up = OrbitInput {
            look: (0.0, -1e5),
            ..Default::default()
        };
        orbit.update(up, 0.0, 0.0, &p, DT);
        assert_eq!(orbit.pitch, p.min_pitch);
        assert!(orbit.offset().1 > 0.0, "never below the horizon");
    }

    #[test]
    fn zoom_is_clamped() {
        let p = params();
        let mut orbit = Orbit::behind(0.0);
        let zoom = |lines: f32| OrbitInput {
            zoom: lines,
            ..Default::default()
        };
        orbit.update(zoom(1.0), 0.0, 0.0, &p, DT);
        assert!((orbit.distance - (Orbit::DEFAULT_DISTANCE - p.zoom_step)).abs() < 1e-4);
        orbit.update(zoom(100.0), 0.0, 0.0, &p, DT);
        assert_eq!(orbit.distance, p.min_distance);
        orbit.update(zoom(-100.0), 0.0, 0.0, &p, DT);
        assert_eq!(orbit.distance, p.max_distance);
    }

    #[test]
    fn recenters_behind_a_moving_rider_after_idling() {
        let p = params();
        let heading = 1.0;
        let mut orbit = Orbit::behind(heading);
        orbit.yaw = heading + FRAC_PI_2; // Looking at the rider's side.
        let side = orbit.yaw;
        idle_for(&mut orbit, heading, 5.0, p.recenter_delay - 0.2);
        assert_eq!(orbit.yaw, side, "waits before recentering");
        idle_for(&mut orbit, heading, 5.0, 0.5);
        let behind = Orbit::behind(heading).yaw;
        let eased = angle_between(behind, orbit.yaw).abs();
        assert!(
            eased > 0.05 && eased < FRAC_PI_2,
            "eases, does not snap: {eased}"
        );
        idle_for(&mut orbit, heading, 5.0, 3.0);
        assert!(angle_between(behind, orbit.yaw).abs() < 0.02);
    }

    #[test]
    fn does_not_recenter_while_standing_still() {
        let heading = 1.0;
        let mut orbit = Orbit::behind(heading);
        orbit.yaw = 4.0;
        idle_for(&mut orbit, heading, 0.0, 10.0);
        assert_eq!(orbit.yaw, 4.0);
        idle_for(&mut orbit, heading, 0.2, 10.0);
        assert_eq!(orbit.yaw, 4.0, "too slow to count as moving");
    }

    #[test]
    fn does_not_recenter_while_the_camera_is_used() {
        let p = params();
        let heading = 1.0;
        let mut orbit = Orbit::behind(heading);
        orbit.yaw = 4.0;
        let fiddle = OrbitInput {
            zoom: 0.001,
            ..Default::default()
        };
        for _ in 0..(10.0 / DT) as usize {
            orbit.update(fiddle, heading, 8.0, &p, DT);
        }
        assert_eq!(orbit.yaw, 4.0);
        assert!(orbit.idle < DT * 1.5);
    }

    #[test]
    fn recenters_the_short_way_across_the_seam() {
        let p = params();
        // Behind is just below TAU; the camera starts just above zero.
        let heading = PI - 0.1;
        let mut orbit = Orbit::behind(heading);
        orbit.yaw = 0.3;
        orbit.idle = p.recenter_delay;
        for _ in 0..(3.0 / DT) as usize {
            orbit.update(OrbitInput::default(), heading, 5.0, &p, DT);
            let d = angle_between(orbit.yaw, Orbit::behind(heading).yaw);
            assert!(
                d.abs() <= 0.4 + 1e-4,
                "took the long way: yaw {}",
                orbit.yaw
            );
        }
    }
}
