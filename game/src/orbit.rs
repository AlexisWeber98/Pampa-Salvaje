//! Matemática de órbita para la cámara en tercera persona, libre de Bevy para
//! poder probarla por separado.
//!
//! La cámara gira alrededor del jinete sobre una esfera: `yaw` es el ángulo en
//! el plano del suelo desde el jinete hasta la cámara (radianes, en sentido
//! antihorario desde el este, como `Heading`), `pitch` su elevación sobre el
//! horizonte y `distance` el radio de la esfera. El yaw da la vuelta completa;
//! el pitch y la distancia se mantienen dentro de límites. Tras un rato sin
//! entrada de cámara, la cámara vuelve suavemente detrás de un jinete en
//! movimiento.

use pampa_core::Vec2;
use std::f32::consts::{PI, TAU};

/// Ajuste para [`Orbit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitParams {
    /// Radianes de órbita por píxel de movimiento del mouse.
    pub mouse_sensitivity: f32,
    /// Radianes por segundo de órbita mientras se mantiene Q o E.
    pub key_rate: f32,
    /// Metros de zoom por línea de rueda.
    pub zoom_step: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    /// Elevación más baja sobre el horizonte, en radianes.
    pub min_pitch: f32,
    /// Elevación más alta sobre el horizonte, en radianes.
    pub max_pitch: f32,
    /// Segundos sin entrada de cámara antes de que la cámara se recentre.
    pub recenter_delay: f32,
    /// Velocidad del jinete sobre el suelo (m/s, absoluta) necesaria para
    /// recentrar.
    pub recenter_min_speed: f32,
    /// Tasa exponencial de la transición de recentrado (más alta = más rápida).
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

/// Entrada de cámara para un frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OrbitInput {
    /// Movimiento del mouse en píxeles: `x` a la derecha, `y` hacia abajo.
    pub look: (f32, f32),
    /// `-1.0..=1.0`: positivo orbita como mover el mouse a la derecha (E).
    pub keys: f32,
    /// Líneas de rueda: positivo acerca el zoom.
    pub zoom: f32,
}

impl OrbitInput {
    /// Si el jugador tocó la cámara en este frame.
    pub fn is_active(&self) -> bool {
        self.look != (0.0, 0.0) || self.keys != 0.0 || self.zoom != 0.0
    }
}

/// Dónde está la cámara en su órbita alrededor del jinete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    /// Ángulo en el suelo desde el jinete hasta la cámara, ajustado a
    /// `[0, TAU)`.
    pub yaw: f32,
    /// Elevación sobre el horizonte, en radianes.
    pub pitch: f32,
    /// Distancia desde el punto observado, en metros.
    pub distance: f32,
    /// Segundos desde la última entrada de cámara.
    pub idle: f32,
}

impl Orbit {
    /// Elevación por defecto: un poco por encima de la cabeza del jinete.
    pub const DEFAULT_PITCH: f32 = 0.26;
    /// Distancia por defecto desde el punto observado, en metros.
    pub const DEFAULT_DISTANCE: f32 = 9.5;

    /// Justo detrás de un jinete que mira hacia `heading` (radianes), con el
    /// pitch y la distancia por defecto.
    pub fn behind(heading: f32) -> Self {
        Self {
            yaw: wrap(heading + PI),
            pitch: Self::DEFAULT_PITCH,
            distance: Self::DEFAULT_DISTANCE,
            idle: 0.0,
        }
    }

    /// Aplica un frame de `input` y, tras suficiente tiempo inactivo, lleva
    /// suavemente la cámara detrás de un jinete que mira hacia `heading` y se
    /// mueve a `speed`. Una entrada o un `dt` no finitos cuentan como ninguno.
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
            // El mouse a la derecha (o E) gira la vista a la derecha: la cámara
            // se mueve en sentido horario alrededor del jinete, vista desde
            // arriba.
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

    /// Posición de la cámara relativa al punto observado: el desplazamiento en
    /// el plano del suelo y la altura sobre él.
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

/// `angle` ajustado a `[0, TAU)`.
fn wrap(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(TAU);
    // `rem_euclid` puede redondear hasta exactamente TAU con entradas negativas
    // diminutas.
    if wrapped >= TAU { 0.0 } else { wrapped }
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// Ángulo más corto con signo de `from` a `to`, en `(-PI, PI]`.
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
        // El jinete mira al norte: la cámara está al sur de él, sobre el suelo.
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
        // Una vuelta completa en muchos pequeños movimientos del mouse a la
        // derecha.
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
        // Mover el mouse a la derecha gira la vista a la derecha: la cámara se
        // mueve en sentido horario (vista desde arriba) alrededor del jinete.
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
        orbit.yaw = heading + FRAC_PI_2; // Mirando el costado del jinete.
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
        // Detrás queda justo debajo de TAU; la cámara empieza justo por encima
        // de cero.
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
