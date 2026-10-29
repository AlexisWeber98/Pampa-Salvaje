//! Rigid-part poses for people: how far each joint turns at a point of the
//! gait cycle. Render rotates the parts of a low-poly figure by these
//! angles; logic never needs them. Horses are animated models: only their
//! gait is worked out here ([`HorseGait`]).
//!
//! Angles are radians about the body's side-to-side axis, positive when the
//! limb below the joint swings forward. People have no gallop: their
//! [`AnimationState::Gallop`] is a jog.

use crate::AnimationState;

/// Joint angles and body offsets of one pose.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JointAngles {
    /// Hip swing (upper leg), left and right.
    pub hip: [f32; 2],
    /// Knee bend, left and right: the lower leg folds back (always `<= 0`).
    pub knee: [f32; 2],
    /// Shoulder swing (upper arm), left and right.
    pub shoulder: [f32; 2],
    /// Elbow bend, left and right: the forearm folds forward (always `>= 0`).
    pub elbow: [f32; 2],
    /// Forward lean of the torso.
    pub lean: f32,
    /// Rise of the whole body in meters (the stride's bob).
    pub bob: f32,
    /// Rise of the torso in meters while breathing.
    pub breath: f32,
    /// Head nod (forward positive) that steadies the gaze.
    pub nod: f32,
    /// Hat tilt, brim down in front positive: the brim follows the step.
    pub hat: f32,
    /// Fore-and-aft swing of a poncho, shawl or skirt, hem forward positive.
    pub drape: f32,
    /// Side-to-side sway of the same, hem to the right positive.
    pub drape_side: f32,
}

/// Breaths per second while standing.
pub const BREATHS_PER_SECOND: f32 = 0.25;

/// Swing amplitude (hip, shoulder, knee fold, elbow fold), lean, bob.
fn gait(animation: AnimationState) -> (f32, f32, f32, f32, f32, f32) {
    match animation {
        AnimationState::Idle => (0.0, 0.0, 0.0, 0.08, 0.0, 0.0),
        AnimationState::Walk => (0.42, 0.38, 0.55, 0.15, 0.03, 0.025),
        AnimationState::Gallop => (0.5, 0.75, 1.1, 1.25, 0.2, 0.06),
    }
}

/// The pose of `animation` at `phase` (cycles, wrapped to `0..1`).
///
/// Legs swing in opposite phase, each arm against the leg on its side, and
/// the body rises twice per cycle, once per step. A walking leg sweeps its
/// foot back at an even pace while on the ground ([`foot_sweep`]); a jogging
/// one swings evenly. Idle only breathes, the same on both sides.
pub fn pose(animation: AnimationState, phase: f32) -> JointAngles {
    use std::f32::consts::TAU;
    let phase = wrap(phase);
    let angle = phase * TAU;
    let (swing, arm, fold, elbow, lean, bob) = gait(animation);
    let (hat, drape, drape_side) = drape(animation, phase);
    if animation == AnimationState::Idle {
        return JointAngles {
            elbow: [elbow; 2],
            breath: 0.012 * angle.sin(),
            nod: 0.02 * angle.sin(),
            hat,
            drape,
            drape_side,
            ..JointAngles::default()
        };
    }
    let (s, c) = angle.sin_cos();
    let hip = |phase: f32| match animation {
        AnimationState::Walk => (foot_sweep(phase) * swing.sin()).asin(),
        _ => swing * (phase * TAU).sin(),
    };
    JointAngles {
        hip: [hip(phase), hip(phase + 0.5)],
        // A leg folds while it swings forward (its hip angle rising).
        knee: [-fold * c.max(0.0), -fold * (-c).max(0.0)],
        shoulder: [-arm * s, arm * s],
        elbow: [elbow + 0.1 * (-s).max(0.0), elbow + 0.1 * s.max(0.0)],
        lean,
        bob: bob * s.abs(),
        breath: 0.0,
        nod: -0.04 * (2.0 * angle).cos(),
        hat,
        drape,
        drape_side,
    }
}

/// Where a walking leg's foot is, front `1` to back `-1`, at `phase` of
/// its cycle (the left leg's; the right one is half a cycle on). From the
/// front at `0.25` it sweeps back at an even pace for half a cycle, so a
/// foot on the ground keeps still on it, then eases forward again.
pub fn foot_sweep(phase: f32) -> f32 {
    let p = wrap(phase - 0.25);
    if p < 0.5 {
        1.0 - 4.0 * p
    } else {
        let t = 2.0 * p - 1.0;
        -1.0 + 2.0 * t * t * (3.0 - 2.0 * t)
    }
}

/// Gait cycles per second at ground `speed` (m/s): a cycle is two steps, so
/// feet keep pace with the ground. Standing breathes at a fixed rate.
pub fn cadence(animation: AnimationState, speed: f32) -> f32 {
    let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
    match animation {
        AnimationState::Idle => BREATHS_PER_SECOND,
        // Two steps of about 0.7 m: the foot on the ground sweeps back as
        // far as the body goes in a step.
        AnimationState::Walk => speed.max(0.8) / 1.4,
        // Two bounding steps of about 1.35 m, with a moment in the air:
        // the foot meets the ground in mid-swing, sweeping back as fast as
        // the body goes.
        AnimationState::Gallop => speed.max(2.0) / 2.7,
    }
}

/// `phase` advanced by `dt` seconds of `animation` at `speed`, wrapped to
/// `0..1`.
pub fn advance(phase: f32, animation: AnimationState, speed: f32, dt: f32) -> f32 {
    let step = cadence(animation, speed) * dt.max(0.0);
    wrap(phase + step)
}

/// How a horse moves. Logic only knows [`AnimationState`]; the horse trots
/// in the slower part of its gallop range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HorseGait {
    /// Standing: breathing, the weight shifted onto three legs.
    #[default]
    Idle,
    /// The four-beat walk.
    Walk,
    /// The two-beat trot: diagonal pairs of legs together.
    Trot,
    /// The gallop, with a moment of suspension.
    Gallop,
}

impl HorseGait {
    /// Every gait, slowest first.
    pub const ALL: [Self; 4] = [Self::Idle, Self::Walk, Self::Trot, Self::Gallop];

    /// Speed (m/s) above which a horse in its gallop range gallops rather
    /// than trots.
    pub const TROT_MAX: f32 = 6.5;

    /// The gait for logic's `animation` at ground `speed` (m/s).
    pub fn of(animation: AnimationState, speed: f32) -> Self {
        match animation {
            AnimationState::Idle => Self::Idle,
            AnimationState::Walk => Self::Walk,
            // Not finite: the slower gait.
            AnimationState::Gallop if speed.is_finite() && speed.abs() > Self::TROT_MAX => {
                Self::Gallop
            }
            AnimationState::Gallop => Self::Trot,
        }
    }
}

/// Secondary motion: how far the hat, a poncho or a skirt swing at `phase`
/// of a person's `animation` (see [`JointAngles::hat`]). They lag the
/// step, so they swing after the body.
pub fn drape(animation: AnimationState, phase: f32) -> (f32, f32, f32) {
    use std::f32::consts::TAU;
    let angle = wrap(phase) * TAU;
    // Brim, fore-and-aft and side amplitudes.
    let (brim, fore, side) = match animation {
        AnimationState::Idle => return (0.0, 0.012 * angle.sin(), 0.0),
        AnimationState::Walk => (0.04, 0.07, 0.04),
        AnimationState::Gallop => (0.09, 0.15, 0.08),
    };
    (
        brim * (2.0 * angle - 0.6).sin(),
        fore * (2.0 * angle - 1.0).sin(),
        side * (angle - 0.8).sin(),
    )
}

/// A cheap integer hash for deterministic moments.
fn mix(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// The secondary-motion clock ([`glance`]) wraps after this many seconds
/// (7.7 h): a multiple of every cycle it uses (6–10 s), so each starts a
/// fresh cycle, at rest, right at the wrap, and an `f32` clock never grows
/// coarse.
pub const MOTION_CLOCK_PERIOD: f32 = 27_720.0;

/// The secondary-motion `clock` advanced by `dt` seconds, wrapped at
/// [`MOTION_CLOCK_PERIOD`]. A clock that is not finite restarts at 0; a
/// step that is not finite (or negative) is ignored.
pub fn tick_clock(clock: f32, dt: f32) -> f32 {
    let clock = if clock.is_finite() { clock } else { 0.0 };
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    (clock + dt).rem_euclid(MOTION_CLOCK_PERIOD)
}

/// Where `time` falls in a run of `period`-second cycles of a `seed`, each
/// holding one event `length` seconds long at a hashed moment: the event's
/// progress `0..1` while it lasts, and the cycle's hash.
fn moment(seed: u32, time: f32, period: f32, length: f32) -> Option<(f32, u32)> {
    if !time.is_finite() || time < 0.0 {
        return None;
    }
    let cycle = (time / period).floor();
    let h = mix(seed ^ mix(cycle as u32).wrapping_mul(0x9e37_79b9));
    let start = (h % 1000) as f32 / 1000.0 * (period - length);
    let s = (time - cycle * period - start) / length;
    (0.0..1.0).contains(&s).then_some((s, h))
}

/// Smoothly up from 0 to 1 over the first `edge` of `0..1`, down over the last.
fn plateau(s: f32, edge: f32) -> f32 {
    let smooth = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    smooth(s / edge).min(smooth((1.0 - s) / edge))
}

/// How far an idle person glances aside (radians, left positive) at `time`
/// seconds: now and then, deterministically per `seed`, a look to one side
/// that eases in, holds and eases back.
pub fn glance(seed: u32, time: f32) -> f32 {
    let period = 6.0 + (mix(seed) % 5) as f32;
    let Some((s, h)) = moment(seed, time, period, 2.4) else {
        return 0.0;
    };
    let side = if h & 0x10000 == 0 { 1.0 } else { -1.0 };
    let amount = 0.3 + 0.25 * ((h >> 20) % 100) as f32 / 100.0;
    side * amount * plateau(s, 0.3)
}

/// `phase` wrapped to `0..1`, negative phases included (`-0.25` is `0.75`);
/// anything not finite is `0`.
fn wrap(phase: f32) -> f32 {
    let wrapped = phase.rem_euclid(1.0);
    // A tiny negative phase rounds up to exactly 1.0.
    if wrapped.is_finite() && wrapped < 1.0 {
        wrapped
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AnimationState::*;

    fn samples() -> impl Iterator<Item = f32> {
        (0..40).map(|i| i as f32 / 40.0)
    }

    #[test]
    fn idle_is_symmetric_and_only_breathes() {
        let mut breathes = false;
        for t in samples() {
            let p = pose(Idle, t);
            assert_eq!(p.hip[0], p.hip[1]);
            assert_eq!(p.knee[0], p.knee[1]);
            assert_eq!(p.shoulder[0], p.shoulder[1]);
            assert_eq!(p.elbow[0], p.elbow[1]);
            assert_eq!(
                (p.hip[0], p.shoulder[0], p.bob, p.lean),
                (0.0, 0.0, 0.0, 0.0)
            );
            breathes |= p.breath.abs() > 0.005;
        }
        assert!(breathes, "the chest rises and falls");
    }

    #[test]
    fn walking_legs_swing_in_opposite_phase_and_arms_against_them() {
        let mut forward = [false; 2];
        for t in samples() {
            let p = pose(Walk, t);
            assert!(
                (p.hip[1] - pose(Walk, t + 0.5).hip[0]).abs() < 1e-6,
                "t {t}: {p:?}"
            );
            assert!(p.hip[0] * p.hip[1] <= 0.0, "t {t}: {p:?}");
            assert!(p.hip[0] * p.shoulder[0] <= 0.0, "left arm against left leg");
            assert!(
                p.hip[1] * p.shoulder[1] <= 0.0,
                "right arm against right leg"
            );
            assert!(p.knee.iter().all(|k| *k <= 0.0) && p.elbow.iter().all(|e| *e >= 0.0));
            assert!(p.bob >= 0.0);
            for (stepped, hip) in forward.iter_mut().zip(p.hip) {
                *stepped |= hip > 0.3;
            }
        }
        assert_eq!(forward, [true, true], "each leg steps forward");
        // The head bobs: two rises per cycle.
        assert!(pose(Walk, 0.25).bob > 0.02 && pose(Walk, 0.0).bob < 1e-6);
    }

    #[test]
    fn jogging_swings_wider_and_leans_forward() {
        let peak = |a| {
            samples()
                .map(|t| pose(a, t).hip[0].abs())
                .fold(0.0, f32::max)
        };
        assert!(peak(Gallop) > peak(Walk) + 0.05);
        assert!(pose(Gallop, 0.1).lean > pose(Walk, 0.1).lean);
        let bob = |a| samples().map(|t| pose(a, t).bob).fold(0.0, f32::max);
        assert!(bob(Gallop) > bob(Walk));
    }

    #[test]
    fn a_walking_foot_sweeps_back_evenly_while_its_leg_is_straight() {
        // Half a cycle with the knee straight, the foot (at `sin(hip)` of
        // the leg's length forward) moves back the same each moment.
        let at = |t: f32| pose(Walk, t).hip[0].sin();
        let mut straight = 0;
        for k in 0..200 {
            let t = k as f32 / 200.0;
            if [t - 0.01, t, t + 0.01]
                .iter()
                .any(|&t| pose(Walk, t).knee[0] < 0.0)
            {
                continue;
            }
            straight += 1;
            let (a, b, c) = (at(t - 0.01), at(t), at(t + 0.01));
            assert!(b < a && ((a - b) - (b - c)).abs() < 1e-4, "t {t}");
        }
        assert!(straight > 80, "{straight}");
    }

    #[test]
    fn a_foot_on_the_ground_keeps_pace_with_it() {
        // A person's leg, hip to ankle, about.
        const LEG: f32 = 0.85;
        for speed in [1.2, 6.0 / 3.6] {
            // Walking, a straight leg carries the body through a whole step:
            // its foot sweeps from one end of the swing to the other.
            let swing = pose(Walk, 0.25).hip[0];
            let sweep = 2.0 * LEG * swing.sin();
            let step = speed / cadence(Walk, speed) / 2.0;
            assert!((sweep / step - 1.0).abs() < 0.06, "{sweep} {step}");
        }
        for speed in [3.0, 18.0 / 3.6] {
            // Jogging, the foot lands in mid-swing, sweeping back fastest.
            let swing = pose(Gallop, 0.25).hip[0];
            let foot = swing * LEG * std::f32::consts::TAU * cadence(Gallop, speed);
            assert!((foot / speed - 1.0).abs() < 0.06, "{foot} {speed}");
        }
    }

    #[test]
    fn the_phase_advances_with_speed_whatever_the_frame_rate() {
        // Proportional to speed above the slowest stride.
        let walk = |s| cadence(Walk, s);
        assert!((walk(2.4) - 2.0 * walk(1.2)).abs() < 1e-6);
        let jog = |s| cadence(Gallop, s);
        assert!((jog(6.0) - 2.0 * jog(3.0)).abs() < 1e-6);
        assert_eq!(cadence(Idle, 5.0), BREATHS_PER_SECOND);

        // Ten 10 ms frames reach where one 100 ms frame does.
        let mut small = 0.3;
        for _ in 0..10 {
            small = advance(small, Walk, 1.5, 0.01);
        }
        let big = advance(0.3, Walk, 1.5, 0.1);
        assert!((small - big).abs() < 1e-5, "{small} vs {big}");
        let wrapped = advance(0.9, Gallop, 4.0, 0.5);
        assert!((0.0..1.0).contains(&wrapped));
        assert_eq!(advance(0.4, Walk, 1.5, -1.0), 0.4, "time never runs back");
        // A negative phase wraps into 0..1 too, to the same point of the cycle.
        let from_negative = advance(-0.25, Walk, 1.5, 0.0);
        assert!((from_negative - 0.75).abs() < 1e-6, "{from_negative}");
        assert!((0.0..1.0).contains(&advance(-3.6, Gallop, 4.0, 0.1)));
        assert!((0.0..1.0).contains(&advance(-1e-9, Walk, 1.5, 0.0)));
        assert_eq!(pose(Walk, -0.75), pose(Walk, 0.25));
    }

    #[test]
    fn the_motion_clock_wraps_seamlessly_and_never_loses_precision() {
        // A clock left running for weeks stays small and keeps ticking.
        let late = tick_clock(1.0e9, 0.016);
        assert!((0.0..MOTION_CLOCK_PERIOD).contains(&late), "{late}");
        assert!((tick_clock(late, 0.25) - late - 0.25).abs() < 1e-2);
        assert!((tick_clock(MOTION_CLOCK_PERIOD - 0.01, 0.02) - 0.01).abs() < 1e-2);
        // The glance is at rest across the wrap, so it is seamless
        // whatever the seed.
        for seed in 0..64 {
            for t in [MOTION_CLOCK_PERIOD - 0.01, 0.0, 0.01] {
                assert!(glance(seed, t).abs() < 0.02, "{seed} {t}");
            }
        }
    }

    #[test]
    fn the_horse_trots_in_the_slow_part_of_its_gallop_range() {
        assert_eq!(HorseGait::of(Idle, 0.0), HorseGait::Idle);
        assert_eq!(HorseGait::of(Walk, 2.0), HorseGait::Walk);
        assert_eq!(HorseGait::of(Gallop, 4.0), HorseGait::Trot);
        assert_eq!(HorseGait::of(Gallop, -4.0), HorseGait::Trot);
        assert_eq!(HorseGait::of(Gallop, 11.0), HorseGait::Gallop);
    }

    fn times() -> impl Iterator<Item = f32> {
        (0..3000).map(|i| i as f32 * 0.02)
    }

    #[test]
    fn hats_and_drapes_follow_the_step_within_bounds() {
        for animation in [Idle, Walk, Gallop] {
            for t in samples() {
                let (hat, fore, side) = drape(animation, t);
                assert!(hat.abs() <= 0.12 && fore.abs() <= 0.2 && side.abs() <= 0.12);
                let next = drape(animation, t + 1.0);
                assert!(
                    (next.1 - fore).abs() < 1e-5 && (next.0 - hat).abs() < 1e-5,
                    "cyclic"
                );
                let p = pose(animation, t);
                assert_eq!((p.hat, p.drape, p.drape_side), (hat, fore, side));
            }
        }
        let swing = |a| samples().map(|t| drape(a, t).1.abs()).fold(0.0, f32::max);
        assert!(swing(Walk) > 0.03 && swing(Gallop) > swing(Walk));
        let brim = |a| samples().map(|t| drape(a, t).0.abs()).fold(0.0, f32::max);
        assert!(brim(Walk) > 0.02 && brim(Idle) == 0.0);
        // It lags the step: the skirt is not at its peak when the leg is.
        assert!(drape(Walk, 0.25).1.abs() < swing(Walk) * 0.95);
    }

    #[test]
    fn idle_people_glance_around_now_and_then() {
        for seed in [0, 7, 1234] {
            let looks: Vec<f32> = times().map(|t| glance(seed, t)).collect();
            assert!(looks.iter().all(|g| g.abs() <= 0.6), "bounded");
            let aside = looks.iter().filter(|g| g.abs() > 0.2).count();
            let ahead = looks.iter().filter(|g| g.abs() < 0.01).count();
            assert!(aside > 0, "seed {seed} glances");
            assert!(ahead > looks.len() / 2, "seed {seed} mostly looks ahead");
            assert_eq!(looks, times().map(|t| glance(seed, t)).collect::<Vec<_>>());
            // Smooth: no jumps between 20 ms frames.
            assert!(looks.windows(2).all(|w| (w[1] - w[0]).abs() < 0.08));
        }
        let a: Vec<f32> = times().map(|t| glance(1, t)).collect();
        let b: Vec<f32> = times().map(|t| glance(2, t)).collect();
        assert_ne!(a, b, "people glance at their own moments");
    }
}
