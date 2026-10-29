//! La geometría del cielo, como funciones puras del reloj del juego: la edad,
//! la fase y la posición de la luna, y cómo gira el campo de estrellas sobre la
//! pampa (a unos 36.5 grados sur).
//!
//! El espacio de mundo es el de Bevy: el este es +X, arriba es +Y, el norte es
//! -Z.

use bevy::math::{Vec3, Vec4};
use std::f32::consts::TAU;

/// Días de una luna nueva a la siguiente.
pub const SYNODIC_DAYS: f32 = 29.53;
/// La edad de la luna en el primer día de juego a medianoche: gibosa creciente.
pub const FIRST_MOON_AGE: f32 = 9.0;
/// La latitud de la pampa, en grados sur.
pub const LATITUDE_SOUTH: f32 = 36.5;
/// La altura del sol al mediodía en radianes; la trayectoria de la luna la
/// comparte.
const PATH_NOON_ELEVATION: f32 = 1.13;
/// Días siderales por día solar.
const SIDEREAL_RATE: f64 = 1.002_737_909;
/// Vueltas del campo de estrellas a la medianoche de juego del día 0, ajustadas
/// para que la Cruz del Sur (Acrux) cruce el meridiano a las 21:00 del primer
/// día.
const STAR_OFFSET: f64 = 0.8911;
/// La altura mínima a la que se eleva la luz de la luna, en radianes, para que
/// la noche siga siendo legible mientras el disco está bajo el horizonte.
pub const MOONLIGHT_FLOOR: f32 = 0.3;

/// La edad de la luna en días a `elapsed_days` días de juego desde que empezó
/// el día 0.
pub fn moon_age(elapsed_days: f64) -> f32 {
    (f64::from(FIRST_MOON_AGE) + elapsed_days).rem_euclid(f64::from(SYNODIC_DAYS)) as f32
}

/// La fracción iluminada del disco lunar: `0` nueva, `1` llena.
pub fn moon_illumination(age: f32) -> f32 {
    0.5 * (1.0 - (TAU * age / SYNODIC_DAYS).cos())
}

/// Un punto del círculo que siguen el sol y la luna, a `hours` en un reloj de
/// 24 h: al este a las 07:00, en el cielo del norte a las 13:00, al oeste a las
/// 19:00, bajo el suelo de noche. Continuo a lo largo de toda la vuelta.
pub fn sky_path(hours: f32) -> Vec3 {
    let h = TAU * (hours - 13.0) / 24.0;
    let noon = Vec3::new(0.0, PATH_NOON_ELEVATION.sin(), -PATH_NOON_ELEVATION.cos());
    (Vec3::X * -h.sin() + noon * h.cos()).normalize()
}

/// Dónde está la luna a la hora `hour`: va detrás del sol `age / 29.53` de un
/// día, así que una luna nueva acompaña al sol y una luna llena está opuesta a
/// él.
pub fn moon_dir(hour: f32, age: f32) -> Vec3 {
    sky_path(hour - 24.0 * age / SYNODIC_DAYS)
}

/// La dirección desde la luna hacia su luz, para dibujar la fase: el sol queda
/// detrás de la luna en luna nueva, detrás del observador en luna llena, y al
/// oeste de ella mientras crece (al este mientras mengua).
pub fn moon_light_dir(moon: Vec3, age: f32) -> Vec3 {
    let phase = TAU * age / SYNODIC_DAYS;
    let west = (-Vec3::X - moon * (-Vec3::X).dot(moon)).normalize_or(Vec3::Z);
    (moon * phase.cos() + west * phase.sin()).normalize()
}

/// La dirección de la que viene la luz nocturna: la luna, elevada al menos
/// hasta [`MOONLIGHT_FLOOR`] cuando está por debajo.
pub fn moonlight_dir(moon: Vec3) -> Vec3 {
    let floor = MOONLIGHT_FLOOR.sin();
    if moon.y >= floor {
        return moon;
    }
    let flat = Vec3::new(moon.x, 0.0, moon.z).normalize_or(Vec3::Z);
    (flat * MOONLIGHT_FLOOR.cos() + Vec3::Y * floor).normalize()
}

/// El marco del campo de estrellas en `elapsed_days`: tres filas que llevan una
/// dirección del mundo a coordenadas celestes (x hacia la ascensión recta 0, y
/// hacia las 6 h, z hacia el polo celeste norte, que queda bajo el horizonte
/// norte). El cielo da una vuelta por día sideral: las estrellas salen por el
/// este y se ponen por el oeste.
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

/// Aplica un [`star_frame`] a una dirección del mundo.
pub fn to_celestial(frame: &[Vec4; 3], dir: Vec3) -> Vec3 {
    Vec3::new(
        frame[0].truncate().dot(dir),
        frame[1].truncate().dot(dir),
        frame[2].truncate().dot(dir),
    )
}

/// Filas de la rotación de ecuatoriales J2000 a galácticas: x hacia el centro
/// galáctico, y hacia la longitud 90, z hacia el polo galáctico norte.
const GALACTIC_FROM_EQUATORIAL: [Vec3; 3] = [
    Vec3::new(-0.054_875_56, -0.873_437_1, -0.483_835),
    Vec3::new(0.494_109_43, -0.444_829_6, 0.746_982_2),
    Vec3::new(-0.867_666_1, -0.198_076_4, 0.455_983_8),
];

/// El marco del mapa de estrellas en `elapsed_days`: tres filas que llevan una
/// dirección del mundo a coordenadas galácticas (x hacia el centro galáctico, y
/// hacia la longitud 90, z hacia el polo galáctico norte), que es como está
/// dispuesta la textura del mapa de estrellas. Es [`star_frame`] seguido de la
/// rotación fija de ecuatoriales a galácticas.
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

/// La longitud y la latitud galácticas, en grados, de la dirección del mundo
/// `dir` bajo `frame` (un [`galactic_frame`]); la longitud en `[0, 360)`.
pub fn galactic_coords(frame: &[Vec4; 3], dir: Vec3) -> (f32, f32) {
    let g = to_celestial(frame, dir);
    let l = g.y.atan2(g.x).to_degrees().rem_euclid(360.0);
    (l, g.z.clamp(-1.0, 1.0).asin().to_degrees())
}

/// La dirección del mundo de una estrella con ascensión recta `ra` y
/// declinación `dec` (en grados) en el marco `frame`.
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
        // Llena: iluminada desde el lado del observador. Nueva: desde detrás de
        // la luna.
        assert!(moon_light_dir(moon, SYNODIC_DAYS / 2.0).dot(-moon) > 0.99);
        assert!(moon_light_dir(moon, 0.01).dot(moon) > 0.99);
        // Creciente: iluminada desde el oeste (-X); menguante, desde el este.
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
        // Una estrella sobre el ecuador celeste cruza el cielo del norte.
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
        // El polo celeste sur está en l 302.9, b -27.1; la Gran Nube de
        // Magallanes, en l 280.5, b -32.9.
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
        // Fomalhaut, lejos de ambos: l 20.49, b -64.91.
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
        // Visto desde la pampa, el centro galáctico sale por el este y se pone
        // por el oeste, una vez por día.
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
