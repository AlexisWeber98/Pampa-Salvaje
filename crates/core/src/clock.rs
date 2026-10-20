//! The game clock: the time of day and the day count, in game seconds.
//!
//! One game day lasts [`DAY_REAL_SECONDS`] real seconds, so the clock runs
//! [`BASE_RATE`] times faster than real time (30×: one game minute every
//! two real seconds). The game can slow it down or speed it up with a rate
//! multiplier (riding the train runs it at about 12×), and can jump it
//! forward (skipping a trip, waiting for a train).
//!
//! The clock is plain data: it only moves when told to, so it is
//! deterministic.

use std::fmt;

/// Game seconds in one day.
pub const DAY_SECONDS: f64 = 24.0 * 60.0 * 60.0;
/// Real seconds one game day lasts at the base rate (48 minutes).
pub const DAY_REAL_SECONDS: f64 = 48.0 * 60.0;
/// Game seconds per real second at the base rate (30×).
pub const BASE_RATE: f64 = DAY_SECONDS / DAY_REAL_SECONDS;

/// A time of day, in game seconds since midnight, in `[0, DAY_SECONDS)`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct TimeOfDay(f64);

impl TimeOfDay {
    /// `seconds` since midnight, wrapped into one day. Non-finite input
    /// reads as midnight.
    pub fn from_seconds(seconds: f64) -> Self {
        if !seconds.is_finite() {
            return Self(0.0);
        }
        let wrapped = seconds.rem_euclid(DAY_SECONDS);
        // `rem_euclid` can round up to exactly one day for tiny negatives.
        Self(if wrapped >= DAY_SECONDS { 0.0 } else { wrapped })
    }

    /// `hour:minute`, wrapped into one day (`hm(24, 0)` is midnight).
    pub fn hm(hour: u32, minute: u32) -> Self {
        Self::from_seconds(f64::from(hour) * 3600.0 + f64::from(minute) * 60.0)
    }

    /// Game seconds since midnight.
    pub fn seconds(self) -> f64 {
        self.0
    }

    /// Hours since midnight, with the fraction (07:30 is 7.5).
    pub fn hour(self) -> f32 {
        (self.0 / 3600.0) as f32
    }

    /// Whole hours and minutes, as a clock face shows them.
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

/// The game clock: which day it is and the time of day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameClock {
    /// Days since the game began (day 0 is the first).
    day: u32,
    /// Game seconds since midnight, in `[0, DAY_SECONDS)`.
    seconds: f64,
    /// How much faster or slower than [`BASE_RATE`] the clock runs. `1.0`
    /// is the base rate; riding the train sets about `0.4` (12×).
    pub multiplier: f64,
}

impl Default for GameClock {
    fn default() -> Self {
        Self::at(TimeOfDay::default())
    }
}

impl GameClock {
    /// Day 0 at `time`, running at the base rate.
    pub fn at(time: TimeOfDay) -> Self {
        Self {
            day: 0,
            seconds: time.seconds(),
            multiplier: 1.0,
        }
    }

    /// Days since the game began.
    pub fn day(&self) -> u32 {
        self.day
    }

    /// The time of day.
    pub fn time_of_day(&self) -> TimeOfDay {
        TimeOfDay(self.seconds)
    }

    /// Game seconds since midnight of day 0: the clock as one number, for
    /// timetables.
    pub fn elapsed(&self) -> f64 {
        f64::from(self.day) * DAY_SECONDS + self.seconds
    }

    /// Hours since midnight, with the fraction.
    pub fn hour(&self) -> f32 {
        self.time_of_day().hour()
    }

    /// The time of day as `HH:MM`.
    pub fn hh_mm(&self) -> String {
        self.time_of_day().to_string()
    }

    /// Game seconds per real second right now.
    pub fn rate(&self) -> f64 {
        BASE_RATE * self.multiplier.max(0.0)
    }

    /// Runs the clock for `real_dt` real seconds at its current rate and
    /// returns the game seconds that passed. Non-finite or non-positive
    /// `real_dt` changes nothing.
    pub fn tick(&mut self, real_dt: f32) -> f64 {
        if !(real_dt.is_finite() && real_dt > 0.0) {
            return 0.0;
        }
        let game = f64::from(real_dt) * self.rate();
        self.advance_by(game)
    }

    /// Jumps the clock `game_seconds` forward, wrapping at midnight into the
    /// next day, and returns how far it went. Non-finite or negative input
    /// changes nothing: the clock never runs backwards.
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

    /// Jumps the clock forward to the next time it reads `time` (staying
    /// put if it already does) and returns the game seconds skipped.
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
        // One game minute every two real seconds.
        assert_eq!(clock.tick(2.0), 60.0);
        assert_eq!(clock.hh_mm(), "07:01");
        // 48 real minutes make a whole day.
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
        // Already there: no jump.
        assert_eq!(clock.advance_until(TimeOfDay::hm(7, 0)), 0.0);
        // Past it today: tomorrow's.
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
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert_eq!(clock.tick(dt), 0.0);
        }
        for g in [f64::NAN, f64::NEG_INFINITY, -5.0] {
            assert_eq!(clock.advance_by(g), 0.0);
        }
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
