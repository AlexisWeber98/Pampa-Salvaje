//! The sky's geometry, as pure functions of the game clock: the moon's age,
//! phase and position, and how the star field turns over the pampa
//! (about 36.5 degrees south).
//!
//! World space is Bevy's: east is +X, up is +Y, north is -Z.

use bevy::math::{Vec3, Vec4};
use std::f32::consts::TAU;

/// Days from one new moon to the next.
pub const SYNODIC_DAYS: f32 = 29.53;
/// The moon's age on the first game day at midnight: a waxing gibbous.
pub const FIRST_MOON_AGE: f32 = 9.0;
/// The pampa's latitude, in degrees south.
pub const LATITUDE_SOUTH: f32 = 36.5;
/// The sun's height at noon in radians; the moon's path shares it.
const PATH_NOON_ELEVATION: f32 = 1.13;
/// Sidereal days per solar day.
const SIDEREAL_RATE: f64 = 1.002_737_909;
/// Turns of the star field at game midnight of day 0, set so that the
/// Southern Cross (Acrux) crosses the meridian at 21:00 on the first day.
const STAR_OFFSET: f64 = 0.8911;
/// Lowest height the moonlight is lifted to, in radians, so the night stays
/// readable while the disc is under the horizon.
pub const MOONLIGHT_FLOOR: f32 = 0.3;

/// The moon's age in days at `elapsed_days` game days since day 0 began.
pub fn moon_age(elapsed_days: f64) -> f32 {
    (f64::from(FIRST_MOON_AGE) + elapsed_days).rem_euclid(f64::from(SYNODIC_DAYS)) as f32
}

/// The lit fraction of the moon's disc: `0` new, `1` full.
pub fn moon_illumination(age: f32) -> f32 {
    0.5 * (1.0 - (TAU * age / SYNODIC_DAYS).cos())
}

/// A point on the circle the sun and moon follow, `hours` on a 24 h clock:
/// east at 07:00, the northern sky at 13:00, west at 19:00, under the ground
/// at night. Continuous through the whole turn.
pub fn sky_path(hours: f32) -> Vec3 {
    let h = TAU * (hours - 13.0) / 24.0;
    let noon = Vec3::new(0.0, PATH_NOON_ELEVATION.sin(), -PATH_NOON_ELEVATION.cos());
    (Vec3::X * -h.sin() + noon * h.cos()).normalize()
}

/// Where the moon is at `hour`: it trails the sun by `age / 29.53` of a day,
/// so a new moon rides with the sun and a full moon opposes it.
pub fn moon_dir(hour: f32, age: f32) -> Vec3 {
    sky_path(hour - 24.0 * age / SYNODIC_DAYS)
}

/// The direction from the moon toward its light, for drawing the phase: the
/// sun lies behind the moon at new, behind the viewer at full, and to the
/// west of it while it waxes (east while it wanes).
pub fn moon_light_dir(moon: Vec3, age: f32) -> Vec3 {
    let phase = TAU * age / SYNODIC_DAYS;
    let west = (-Vec3::X - moon * (-Vec3::X).dot(moon)).normalize_or(Vec3::Z);
    (moon * phase.cos() + west * phase.sin()).normalize()
}

/// The direction the night light comes from: the moon, raised to at least
/// [`MOONLIGHT_FLOOR`] when it is below that.
pub fn moonlight_dir(moon: Vec3) -> Vec3 {
    let floor = MOONLIGHT_FLOOR.sin();
    if moon.y >= floor {
        return moon;
    }
    let flat = Vec3::new(moon.x, 0.0, moon.z).normalize_or(Vec3::Z);
    (flat * MOONLIGHT_FLOOR.cos() + Vec3::Y * floor).normalize()
}

/// The star field's frame at `elapsed_days`: three rows taking a world
/// direction to celestial coordinates (x toward right ascension 0, y toward
/// 6 h, z toward the north celestial pole, which lies under the northern
/// horizon). The sky turns once per sidereal day: stars rise in the east and
/// set in the west.
pub fn star_frame(elapsed_days: f64) -> [Vec4; 3] {
    let theta = TAU * (elapsed_days * SIDEREAL_RATE + STAR_OFFSET).rem_euclid(1.0) as f32;
    let lat = LATITUDE_SOUTH.to_radians();
    let pole = Vec3::new(0.0, -lat.sin(), -lat.cos());
    let east = Vec3::X;
    let b = pole.cross(east);
    let x = east * theta.cos() - b * theta.sin();
    let y = east * theta.sin() + b * theta.cos();
    [x.extend(0.0), y.extend(0.0), pole.extend(0.0)]
}

/// Applies a [`star_frame`] to a world direction.
pub fn to_celestial(frame: &[Vec4; 3], dir: Vec3) -> Vec3 {
    Vec3::new(
        frame[0].truncate().dot(dir),
        frame[1].truncate().dot(dir),
        frame[2].truncate().dot(dir),
    )
}

/// Rows of the J2000 equatorial to galactic rotation: x toward the galactic
/// centre, y toward longitude 90, z toward the north galactic pole.
const GALACTIC_FROM_EQUATORIAL: [Vec3; 3] = [
    Vec3::new(-0.054_875_56, -0.873_437_1, -0.483_835),
    Vec3::new(0.494_109_43, -0.444_829_6, 0.746_982_2),
    Vec3::new(-0.867_666_1, -0.198_076_4, 0.455_983_8),
];

/// The star map's frame at `elapsed_days`: three rows taking a world
/// direction to galactic coordinates (x toward the galactic centre, y toward
/// longitude 90, z toward the north galactic pole), which is how the star
/// map texture is laid out. It is [`star_frame`] followed by the fixed
/// equatorial to galactic rotation.
pub fn galactic_frame(elapsed_days: f64) -> [Vec4; 3] {
    let f = star_frame(elapsed_days);
    let row = |g: Vec3| {
        (f[0].truncate() * g.x + f[1].truncate() * g.y + f[2].truncate() * g.z).extend(0.0)
    };
    [
        row(GALACTIC_FROM_EQUATORIAL[0]),
        row(GALACTIC_FROM_EQUATORIAL[1]),
        row(GALACTIC_FROM_EQUATORIAL[2]),
    ]
}

/// The galactic longitude and latitude, in degrees, of the world direction
/// `dir` under `frame` (a [`galactic_frame`]); longitude in `[0, 360)`.
pub fn galactic_coords(frame: &[Vec4; 3], dir: Vec3) -> (f32, f32) {
    let g = to_celestial(frame, dir);
    let l = g.y.atan2(g.x).to_degrees().rem_euclid(360.0);
    (l, g.z.clamp(-1.0, 1.0).asin().to_degrees())
}

/// The world direction of a star at right ascension `ra` and declination
/// `dec` (degrees) in the frame of `frame`.
pub fn star_dir(frame: &[Vec4; 3], ra: f32, dec: f32) -> Vec3 {
    let (ra, dec) = (ra.to_radians(), dec.to_radians());
    let c = Vec3::new(dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin());
    frame[0].truncate() * c.x + frame[1].truncate() * c.y + frame[2].truncate() * c.z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_moon_ages_one_synodic_month_then_starts_over() {
        assert!((moon_age(0.0) - 9.0).abs() < 1e-4);
        assert!((moon_age(5.0) - 14.0).abs() < 1e-4);
        assert!((moon_age(20.53) - 0.0).abs() < 1e-3 || (moon_age(20.53) - 29.53).abs() < 1e-3);
        assert!((moon_age(30.0) - 9.47).abs() < 1e-3);
    }

    #[test]
    fn the_phase_runs_new_full_new() {
        assert!(moon_illumination(0.0) < 1e-4);
        assert!((moon_illumination(7.38) - 0.5).abs() < 0.01);
        assert!((moon_illumination(14.765) - 1.0).abs() < 1e-4);
        assert!(moon_illumination(22.0) > 0.4 && moon_illumination(22.0) < 0.6);
    }

    #[test]
    fn a_full_moon_opposes_the_sun_and_a_new_one_rides_with_it() {
        for hour in [0.0_f32, 6.0, 13.0, 21.0] {
            let sun = sky_path(hour);
            let full = moon_dir(hour, SYNODIC_DAYS / 2.0);
            assert!(full.dot(sun) < -0.999, "{hour}");
            let new = moon_dir(hour, 0.0);
            assert!(new.dot(sun) > 0.999, "{hour}");
        }
    }

    #[test]
    fn the_sun_path_rises_east_and_sets_west_through_the_north() {
        assert!(sky_path(7.0).x > 0.99);
        assert!(sky_path(19.0).x < -0.99);
        let noon = sky_path(13.0);
        assert!(noon.y > 0.9 && noon.z < 0.0);
        assert!(sky_path(1.0).y < 0.0);
    }

    #[test]
    fn the_lit_side_follows_the_phase() {
        let moon = Vec3::new(0.0, 0.5, -0.8).normalize();
        // Full: lit from the viewer's side. New: from behind the moon.
        assert!(moon_light_dir(moon, SYNODIC_DAYS / 2.0).dot(-moon) > 0.99);
        assert!(moon_light_dir(moon, 0.01).dot(moon) > 0.99);
        // Waxing: lit from the west (-X), waning from the east.
        assert!(moon_light_dir(moon, 7.38).x < -0.9);
        assert!(moon_light_dir(moon, 22.1).x > 0.9);
    }

    #[test]
    fn moonlight_never_sinks_below_its_floor() {
        let low = moonlight_dir(Vec3::new(0.3, -0.9, 0.2).normalize());
        assert!(low.y >= 0.29 && low.y < 0.31, "{low:?}");
        let high = Vec3::new(0.1, 0.9, 0.1).normalize();
        assert_eq!(moonlight_dir(high), high);
    }

    #[test]
    fn the_south_celestial_pole_stands_36_degrees_up_in_the_south() {
        for days in [0.0, 0.37, 5.9] {
            let pole = star_dir(&star_frame(days), 0.0, -90.0);
            assert!(pole.z > 0.8, "south is +Z: {pole:?}");
            assert!(
                (pole.y - 36.5_f32.to_radians().sin()).abs() < 1e-3,
                "{pole:?}"
            );
        }
    }

    #[test]
    fn stars_rise_in_the_east_and_set_in_the_west() {
        // A star on the celestial equator crosses the northern sky.
        let at = |days: f64| star_dir(&star_frame(days), 100.0, 0.0);
        let mut last = at(0.0);
        let mut crossed_up = false;
        for k in 1..=240 {
            let now = at(f64::from(k) / 240.0);
            if last.y < 0.0 && now.y >= 0.0 {
                assert!(now.x > 0.9, "rises in the east: {now:?}");
                crossed_up = true;
            }
            if last.y >= 0.0 && now.y < 0.0 {
                assert!(now.x < -0.9, "sets in the west: {now:?}");
            }
            last = now;
        }
        assert!(crossed_up);
    }

    #[test]
    fn the_southern_cross_is_high_in_the_south_at_nine_on_the_first_night() {
        let acrux = star_dir(&star_frame(21.0 / 24.0), 186.65, -63.10);
        assert!(acrux.x.abs() < 0.03, "on the meridian: {acrux:?}");
        assert!(acrux.z > 0.0, "in the south");
        let elevation = acrux.y.asin().to_degrees();
        assert!((60.0..66.0).contains(&elevation), "{elevation}");
    }

    #[test]
    fn the_galactic_frame_puts_known_objects_at_their_galactic_coordinates() {
        let frame = galactic_frame(0.4);
        // The south celestial pole is at l 302.9, b -27.1; the Large
        // Magellanic Cloud at l 280.5, b -32.9.
        let pole = star_dir(&star_frame(0.4), 0.0, -90.0);
        let (l, b) = galactic_coords(&frame, pole);
        assert!(
            (l - 302.93).abs() < 0.05 && (b + 27.13).abs() < 0.05,
            "{l} {b}"
        );
        let lmc = star_dir(&star_frame(0.4), 80.89, -69.76);
        let (l, b) = galactic_coords(&frame, lmc);
        assert!(
            (l - 280.47).abs() < 0.05 && (b + 32.89).abs() < 0.05,
            "{l} {b}"
        );
        // Fomalhaut, far from both: l 20.49, b -64.91.
        let fomalhaut = star_dir(&star_frame(0.4), 344.41, -29.62);
        let (l, b) = galactic_coords(&frame, fomalhaut);
        assert!(
            (l - 20.49).abs() < 0.05 && (b + 64.91).abs() < 0.05,
            "{l} {b}"
        );
    }

    #[test]
    fn the_sky_turns_with_the_hour_and_comes_round_every_sidereal_day() {
        let at =
            |days: f64| to_celestial(&galactic_frame(days), Vec3::new(0.3, 0.7, -0.4).normalize());
        assert!(at(0.2).distance(at(0.3)) > 0.1, "an hour and a half later");
        assert!(at(0.2).distance(at(0.2 + 1.0 / 1.002_737_909)) < 1e-3);
        // Seen from the pampa the galactic centre rises in the east and
        // sets in the west, once a day.
        let gc = |days: f64| {
            let f = galactic_frame(days);
            Vec3::new(f[0].x, f[1].x, f[2].x)
        };
        let mut up_hours = 0;
        for k in 0..240 {
            if gc(f64::from(k) / 240.0).y > 0.0 {
                up_hours += 1;
            }
        }
        assert!((70..170).contains(&up_hours), "{up_hours}");
    }

    #[test]
    fn the_star_frame_is_a_rotation() {
        let f = star_frame(3.3);
        let (x, y, z) = (f[0].truncate(), f[1].truncate(), f[2].truncate());
        assert!((x.length() - 1.0).abs() < 1e-4 && (y.length() - 1.0).abs() < 1e-4);
        assert!(x.dot(y).abs() < 1e-4 && x.dot(z).abs() < 1e-4);
        assert!(x.cross(y).dot(z) > 0.999);
        let d = to_celestial(&f, Vec3::new(0.2, 0.9, -0.3).normalize());
        assert!((d.length() - 1.0).abs() < 1e-4);
    }
}
