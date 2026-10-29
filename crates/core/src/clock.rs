//! El reloj del juego: la hora del día y el número de día, en segundos de
//! juego.
//!
//! Un día de juego dura [`DAY_REAL_SECONDS`] segundos reales, así que el reloj
//! corre [`BASE_RATE`] veces más rápido que el tiempo real (30×: un minuto de
//! juego cada dos segundos reales). El juego puede frenarlo o acelerarlo con un
//! multiplicador de velocidad (viajar en tren lo hace correr a unas 12×), y
//! puede adelantarlo de golpe (al saltear un viaje o al esperar un tren).
//!
//! El reloj son datos simples: solo avanza cuando se le indica, así que es
//! determinista.

use std::fmt;

/// Segundos de juego en un día.
pub const DAY_SECONDS: f64 = 24.0 * 60.0 * 60.0;
/// Segundos reales que dura un día de juego a la velocidad base (48 minutos).
pub const DAY_REAL_SECONDS: f64 = 48.0 * 60.0;
/// Segundos de juego por segundo real a la velocidad base (30×).
pub const BASE_RATE: f64 = DAY_SECONDS / DAY_REAL_SECONDS;

/// Una hora del día, en segundos de juego desde la medianoche, en `[0,
/// DAY_SECONDS)`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct TimeOfDay(f64);

impl TimeOfDay {
    /// `seconds` desde la medianoche, ajustados a un solo día. Una entrada no
    /// finita se interpreta como medianoche.
    pub fn from_seconds(seconds: f64) -> Self {
        if !seconds.is_finite() {
            return Self(0.0);
        }
        let wrapped = seconds.rem_euclid(DAY_SECONDS);
        // `rem_euclid` puede redondear hasta exactamente un día con negativos
        // muy pequeños.
        Self(if wrapped >= DAY_SECONDS { 0.0 } else { wrapped })
    }

    /// `hour:minute`, ajustado a un solo día (`hm(24, 0)` es medianoche).
    pub fn hm(hour: u32, minute: u32) -> Self {
        Self::from_seconds(f64::from(hour) * 3600.0 + f64::from(minute) * 60.0)
    }

    /// Segundos de juego desde la medianoche.
    pub fn seconds(self) -> f64 {
        self.0
    }

    /// Horas desde la medianoche, con la fracción (07:30 es 7,5).
    pub fn hour(self) -> f32 {
        (self.0 / 3600.0) as f32
    }

    /// Horas y minutos enteros, como los muestra la esfera de un reloj.
    pub fn hour_minute(self) -> (u32, u32) {
        let minutes = (self.0 / 60.0).floor() as u32;
        (minutes / 60 % 24, minutes % 60)
    }
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (h, m) = self.hour_minute();
        write!(f, "{h:02}:{m:02}")
    }
}

/// El reloj del juego: qué día es y la hora del día.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameClock {
    /// Días desde que empezó el juego (el día 0 es el primero).
    day: u32,
    /// Segundos de juego desde la medianoche, en `[0, DAY_SECONDS)`.
    seconds: f64,
    /// Cuánto más rápido o más lento que [`BASE_RATE`] corre el reloj. `1.0` es
    /// la velocidad base; viajar en tren la pone en alrededor de `0.4` (12×).
    pub multiplier: f64,
}

impl Default for GameClock {
    fn default() -> Self {
        Self::at(TimeOfDay::default())
    }
}

impl GameClock {
    /// Día 0 a la hora `time`, corriendo a la velocidad base.
    pub fn at(time: TimeOfDay) -> Self {
        Self {
            day: 0,
            seconds: time.seconds(),
            multiplier: 1.0,
        }
    }

    /// Días desde que empezó el juego.
    pub fn day(&self) -> u32 {
        self.day
    }

    /// La hora del día.
    pub fn time_of_day(&self) -> TimeOfDay {
        TimeOfDay(self.seconds)
    }

    /// Segundos de juego desde la medianoche del día 0: el reloj como un solo
    /// número, para los horarios.
    pub fn elapsed(&self) -> f64 {
        f64::from(self.day) * DAY_SECONDS + self.seconds
    }

    /// Horas desde la medianoche, con la fracción.
    pub fn hour(&self) -> f32 {
        self.time_of_day().hour()
    }

    /// La hora del día como `HH:MM`.
    pub fn hh_mm(&self) -> String {
        self.time_of_day().to_string()
    }

    /// Segundos de juego por segundo real en este momento.
    pub fn rate(&self) -> f64 {
        BASE_RATE * self.multiplier.max(0.0)
    }

    /// Hace correr el reloj durante `real_dt` segundos reales a su velocidad
    /// actual y devuelve los segundos de juego que pasaron. Un `real_dt` no
    /// finito o no positivo no cambia nada.
    pub fn tick(&mut self, real_dt: f32) -> f64 {
        if !(real_dt.is_finite() && real_dt > 0.0) {
            return 0.0;
        }
        let game = f64::from(real_dt) * self.rate();
        self.advance_by(game)
    }

    /// Adelanta el reloj `game_seconds`, pasando al día siguiente al cruzar la
    /// medianoche, y devuelve cuánto avanzó. Una entrada no finita o negativa
    /// no cambia nada: el reloj nunca corre hacia atrás.
    pub fn advance_by(&mut self, game_seconds: f64) -> f64 {
        if !(game_seconds.is_finite() && game_seconds > 0.0) {
            return 0.0;
        }
        let total = self.seconds + game_seconds;
        let days = (total / DAY_SECONDS).floor();
        self.day = self.day.saturating_add(days as u32);
        self.seconds = TimeOfDay::from_seconds(total - days * DAY_SECONDS).seconds();
        game_seconds
    }

    /// Adelanta el reloj hasta la próxima vez que marque `time` (sin moverse si
    /// ya la marca) y devuelve los segundos de juego salteados.
    pub fn advance_until(&mut self, time: TimeOfDay) -> f64 {
        let wait = (time.seconds() - self.seconds).rem_euclid(DAY_SECONDS);
        let wait = if wait >= DAY_SECONDS { 0.0 } else { wait };
        self.advance_by(wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_rate_is_thirty_times_real_time() {
        assert_eq!(BASE_RATE, 30.0);
        let mut clock = GameClock::at(TimeOfDay::hm(7, 0));
        // Un minuto de juego cada dos segundos reales.
        assert_eq!(clock.tick(2.0), 60.0);
        assert_eq!(clock.hh_mm(), "07:01");
        // 48 minutos reales forman un día entero.
        let mut day = GameClock::default();
        for _ in 0..48 * 60 {
            day.tick(1.0);
        }
        assert_eq!((day.day(), day.hh_mm()), (1, "00:00".to_string()));
    }

    #[test]
    fn the_multiplier_scales_the_rate() {
        let mut clock = GameClock::at(TimeOfDay::hm(7, 0));
        clock.multiplier = 0.4;
        assert!((clock.rate() - 12.0).abs() < 1e-9);
        clock.tick(10.0);
        assert!((clock.time_of_day().seconds() - (7.0 * 3600.0 + 120.0)).abs() < 1e-6);
    }

    #[test]
    fn wraps_at_midnight_and_counts_the_days() {
        let mut clock = GameClock::at(TimeOfDay::hm(23, 59));
        clock.advance_by(120.0);
        assert_eq!(clock.day(), 1);
        assert_eq!(clock.hh_mm(), "00:01");
        clock.advance_by(3.0 * DAY_SECONDS + 3600.0);
        assert_eq!(clock.day(), 4);
        assert_eq!(clock.hh_mm(), "01:01");
        assert!((clock.elapsed() - (4.0 * DAY_SECONDS + 3660.0)).abs() < 1e-6);
    }

    #[test]
    fn advance_until_reaches_the_next_occurrence() {
        let mut clock = GameClock::at(TimeOfDay::hm(6, 0));
        assert_eq!(clock.advance_until(TimeOfDay::hm(7, 0)), 3600.0);
        assert_eq!((clock.day(), clock.hh_mm()), (0, "07:00".to_string()));
        // Ya está ahí: no hay salto.
        assert_eq!(clock.advance_until(TimeOfDay::hm(7, 0)), 0.0);
        // Ya pasó hoy: el de mañana.
        clock.advance_by(60.0);
        let skipped = clock.advance_until(TimeOfDay::hm(7, 0));
        assert_eq!(skipped, DAY_SECONDS - 60.0);
        assert_eq!((clock.day(), clock.hh_mm()), (1, "07:00".to_string()));
    }

    #[test]
    fn hour_and_formatting() {
        let clock = GameClock::at(TimeOfDay::hm(16, 30));
        assert_eq!(clock.hour(), 16.5);
        assert_eq!(clock.hh_mm(), "16:30");
        assert_eq!(TimeOfDay::hm(24, 5).to_string(), "00:05");
        assert_eq!(TimeOfDay::from_seconds(-60.0).to_string(), "23:59");
        assert_eq!(TimeOfDay::hm(9, 7).hour_minute(), (9, 7));
    }

    #[test]
    fn bad_input_never_moves_the_clock() {
        let mut clock = GameClock::at(TimeOfDay::hm(12, 0));
        let before = clock;
        for dt in [-1.0, 0.0] {
            assert_eq!(clock.tick(dt), 0.0);
        }
        assert_eq!(clock.advance_by(-5.0), 0.0);
        assert_eq!(clock, before);
    }

    #[test]
    fn ticking_and_jumping_agree() {
        let mut ticked = GameClock::at(TimeOfDay::hm(5, 0));
        let mut jumped = ticked;
        let mut total = 0.0;
        for _ in 0..10_000 {
            total += ticked.tick(1.0 / 60.0);
        }
        jumped.advance_by(total);
        assert_eq!(ticked.day(), jumped.day());
        assert!((ticked.elapsed() - jumped.elapsed()).abs() < 1e-6);
    }
}
