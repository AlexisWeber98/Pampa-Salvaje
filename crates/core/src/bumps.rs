//! Bumps: the rider barging into townsfolk, and who complains about it.
//!
//! [`crate::Townsfolk::make_way`] reports which walkers the rider shoved;
//! [`BumpTracker`] turns those contacts into [`BumpEvent`]s when the rider
//! was moving fast enough, at most once per walker every
//! [`BumpTracker::COOLDOWN`] seconds. [`LinePicker`] then chooses which line
//! of a pool the walker says, deterministically and without repeating the
//! previous line of that pool.

use crate::rng::Rng;
use crate::{AnimationState, RiderParams};

/// How hard the rider bumped somebody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BumpSeverity {
    /// Shoved at a walking pace.
    Walk,
    /// Run into at a gallop.
    Gallop,
}

impl BumpSeverity {
    /// Severity of a contact at `speed` (m/s, either direction), or `None`
    /// below [`BumpTracker::MIN_SPEED`] (a brush at a crawl). Uses the same
    /// gait bands as the rider's animation.
    pub fn from_speed(speed: f32, params: &RiderParams) -> Option<Self> {
        let speed = speed.abs();
        if speed.is_nan() || speed < BumpTracker::MIN_SPEED {
            return None;
        }
        match AnimationState::from_speed(speed, params.walk_max) {
            AnimationState::Gallop => Some(Self::Gallop),
            _ => Some(Self::Walk),
        }
    }
}

/// A walker complains about being bumped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BumpEvent {
    /// Index of the walker in [`crate::Townsfolk::people`].
    pub walker: usize,
    pub severity: BumpSeverity,
}

/// Turns rider contacts into bump events, with a per-walker cooldown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BumpTracker {
    /// Seconds until each walker may complain again (indexed by walker).
    cooldown: Vec<f32>,
}

impl BumpTracker {
    /// Slowest rider speed that counts as a bump, in m/s.
    pub const MIN_SPEED: f32 = 1.5;
    /// Seconds before the same walker complains again.
    pub const COOLDOWN: f32 = 6.0;

    /// Lets `dt` seconds of cooldown pass. Non-finite or non-positive `dt`
    /// changes nothing.
    pub fn tick(&mut self, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        for left in &mut self.cooldown {
            *left = (*left - dt).max(0.0);
        }
    }

    /// Events for the walkers in `contacts` the rider just shoved while
    /// moving at `rider_speed`. Walkers still cooling down stay quiet.
    pub fn register(
        &mut self,
        contacts: &[usize],
        rider_speed: f32,
        params: &RiderParams,
    ) -> Vec<BumpEvent> {
        let Some(severity) = BumpSeverity::from_speed(rider_speed, params) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        for &walker in contacts {
            if walker >= self.cooldown.len() {
                self.cooldown.resize(walker + 1, 0.0);
            }
            if self.cooldown[walker] > 0.0 {
                continue;
            }
            self.cooldown[walker] = Self::COOLDOWN;
            events.push(BumpEvent { walker, severity });
        }
        events
    }
}

/// Chooses lines from pools of lines, deterministically for a seed, never
/// picking the same index of a pool twice in a row when the pool has more
/// than one line.
#[derive(Debug, Clone, PartialEq)]
pub struct LinePicker {
    rng: Rng,
    /// Last index picked per pool id.
    last: Vec<(usize, usize)>,
}

impl LinePicker {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: Rng::new(seed, 0),
            last: Vec::new(),
        }
    }

    /// An index into pool `pool` holding `n` lines, or `None` when it is
    /// empty.
    pub fn pick(&mut self, pool: usize, n: usize) -> Option<usize> {
        if n == 0 {
            return None;
        }
        let slot = match self.last.iter().position(|&(p, _)| p == pool) {
            Some(slot) => slot,
            None => {
                self.last.push((pool, usize::MAX));
                self.last.len() - 1
            }
        };
        let last = self.last[slot].1;
        let index = if n > 1 && last < n {
            // Pick among the other n - 1 lines, skipping over the last one.
            let i = self.rng.below(n - 1);
            if i >= last { i + 1 } else { i }
        } else {
            self.rng.below(n)
        };
        self.last[slot].1 = index;
        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_follows_the_gait() {
        let p = RiderParams::default();
        assert_eq!(BumpSeverity::from_speed(0.0, &p), None);
        assert_eq!(BumpSeverity::from_speed(1.0, &p), None, "a crawl");
        assert_eq!(BumpSeverity::from_speed(1.49, &p), None);
        assert_eq!(
            BumpSeverity::from_speed(BumpTracker::MIN_SPEED, &p),
            Some(BumpSeverity::Walk)
        );
        assert_eq!(
            BumpSeverity::from_speed(p.walk_max, &p),
            Some(BumpSeverity::Walk)
        );
        assert_eq!(
            BumpSeverity::from_speed(p.gallop_max, &p),
            Some(BumpSeverity::Gallop)
        );
        // Backing into somebody counts by speed too.
        assert_eq!(BumpSeverity::from_speed(-2.0, &p), Some(BumpSeverity::Walk));
        for speed in [2.0, 5.0, 11.0] {
            let animation = AnimationState::from_speed(speed, p.walk_max);
            let gallop = BumpSeverity::from_speed(speed, &p) == Some(BumpSeverity::Gallop);
            assert_eq!(gallop, animation == AnimationState::Gallop, "{speed}");
        }
    }

    #[test]
    fn a_fast_contact_fires_once_then_cools_down() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        assert_eq!(
            t.register(&[3], 9.0, &p),
            [BumpEvent {
                walker: 3,
                severity: BumpSeverity::Gallop
            }]
        );
        // Still shoving the same person: quiet during the cooldown.
        for _ in 0..119 {
            t.tick(0.05);
            assert!(t.register(&[3], 9.0, &p).is_empty());
        }
        // Somebody else may still complain.
        assert_eq!(t.register(&[1], 2.0, &p).len(), 1);
        t.tick(0.06);
        assert_eq!(
            t.register(&[3], 2.0, &p),
            [BumpEvent {
                walker: 3,
                severity: BumpSeverity::Walk
            }]
        );
    }

    #[test]
    fn slow_contacts_never_fire_nor_start_a_cooldown() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        assert!(t.register(&[0, 2], 0.8, &p).is_empty());
        assert_eq!(t.register(&[0, 2], 9.0, &p).len(), 2);
    }

    #[test]
    fn several_walkers_and_duplicates_in_one_contact_list() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        let events = t.register(&[5, 0, 5], 3.5, &p);
        assert_eq!(
            events,
            [
                BumpEvent {
                    walker: 5,
                    severity: BumpSeverity::Gallop
                },
                BumpEvent {
                    walker: 0,
                    severity: BumpSeverity::Gallop
                },
            ]
        );
    }

    #[test]
    fn bad_dt_does_not_shorten_the_cooldown() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        t.register(&[0], 9.0, &p);
        for dt in [-100.0, 0.0] {
            t.tick(dt);
        }
        assert!(t.register(&[0], 9.0, &p).is_empty());
        t.tick(BumpTracker::COOLDOWN);
        assert_eq!(t.register(&[0], 9.0, &p).len(), 1);
    }

    #[test]
    fn picker_never_repeats_and_covers_the_pool() {
        let mut picker = LinePicker::new(7);
        for n in [2usize, 3, 10] {
            let mut seen = vec![0usize; n];
            let mut last = None;
            for _ in 0..400 {
                let i = picker.pick(n, n).unwrap();
                assert!(i < n);
                assert_ne!(Some(i), last, "pool of {n} repeated {i}");
                last = Some(i);
                seen[i] += 1;
            }
            assert!(seen.iter().all(|&k| k > 0), "{seen:?}");
        }
    }

    #[test]
    fn picker_tracks_pools_separately_and_handles_tiny_pools() {
        let mut picker = LinePicker::new(1);
        assert_eq!(picker.pick(0, 0), None);
        assert_eq!(picker.pick(0, 1), Some(0));
        assert_eq!(picker.pick(0, 1), Some(0), "one line may repeat");
        // Interleaving pools: each pool still never repeats itself.
        let (mut a, mut b) = (None, None);
        for _ in 0..200 {
            let x = picker.pick(1, 4);
            let y = picker.pick(2, 4);
            assert_ne!(x, a);
            assert_ne!(y, b);
            (a, b) = (x, y);
        }
        // A pool that shrank (content reloaded) never yields a stale index.
        picker.pick(3, 10);
        for _ in 0..50 {
            assert!(picker.pick(3, 2).unwrap() < 2);
        }
    }

    #[test]
    fn picker_is_deterministic_per_seed() {
        let run = |seed| {
            let mut p = LinePicker::new(seed);
            (0..50).map(|_| p.pick(0, 8).unwrap()).collect::<Vec<_>>()
        };
        assert_eq!(run(4), run(4));
        assert_ne!(run(4), run(5));
    }
}
