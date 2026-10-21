//! Rigid-part poses for people and horses: how far each joint turns at a
//! point of the gait cycle. Render rotates the parts of a low-poly figure or
//! horse by these angles; logic never needs them.
//!
//! Angles are radians about the body's side-to-side axis, positive when the
//! limb below the joint swings forward. People have no gallop: their
//! [`AnimationState::Gallop`] is a jog. Horses walk, trot and gallop
//! ([`HorseGait`]), and a rider sits the horse's gait ([`riding_pose`]).

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
    /// How far the thighs open sideways (hip abduction): a rider's legs
    /// round the horse's barrel. `0` when standing or walking.
    pub spread: f32,
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
        AnimationState::Gallop => (0.7, 0.75, 1.1, 1.25, 0.2, 0.06),
    }
}

/// The pose of `animation` at `phase` (cycles, wrapped to `0..1`).
///
/// Legs swing in opposite phase, each arm against the leg on its side, and
/// the body rises twice per cycle, once per step. Idle only breathes, the
/// same on both sides.
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
    JointAngles {
        hip: [swing * s, -swing * s],
        // A leg folds while it swings forward (its hip angle rising).
        knee: [-fold * c.max(0.0), -fold * (-c).max(0.0)],
        shoulder: [-arm * s, arm * s],
        elbow: [elbow + 0.1 * (-s).max(0.0), elbow + 0.1 * s.max(0.0)],
        lean,
        bob: bob * s.abs(),
        breath: 0.0,
        nod: -0.04 * (2.0 * angle).cos(),
        spread: 0.0,
        hat,
        drape,
        drape_side,
    }
}

/// Gait cycles per second at ground `speed` (m/s): a cycle is two steps, so
/// feet keep pace with the ground. Standing breathes at a fixed rate.
pub fn cadence(animation: AnimationState, speed: f32) -> f32 {
    let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
    match animation {
        AnimationState::Idle => BREATHS_PER_SECOND,
        // Two steps of about 0.65 m.
        AnimationState::Walk => speed.max(0.8) / 1.3,
        // Two longer, bounding steps of about 1 m.
        AnimationState::Gallop => speed.max(2.0) / 2.0,
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
            AnimationState::Gallop if speed.abs() > Self::TROT_MAX => Self::Gallop,
            AnimationState::Gallop => Self::Trot,
        }
    }
}

/// A horse's leg, in the order of [`HorseAngles`]' arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Leg {
    FrontLeft,
    FrontRight,
    HindLeft,
    HindRight,
}

impl Leg {
    pub const ALL: [Self; 4] = [
        Self::FrontLeft,
        Self::FrontRight,
        Self::HindLeft,
        Self::HindRight,
    ];

    /// Index into [`HorseAngles`]' leg arrays.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn is_front(self) -> bool {
        matches!(self, Self::FrontLeft | Self::FrontRight)
    }

    pub fn is_left(self) -> bool {
        matches!(self, Self::FrontLeft | Self::HindLeft)
    }
}

/// Joint angles and body offsets of one horse pose. Leg arrays are in
/// [`Leg::ALL`] order.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HorseAngles {
    /// Swing of the upper leg (forearm or gaskin), forward positive.
    pub upper: [f32; 4],
    /// Fold of the knee (front) or hock (hind): the cannon tucks back
    /// (always `<= 0`).
    pub lower: [f32; 4],
    /// Fold of the fetlock: the pastern and hoof flip back (`<= 0` while
    /// the leg is in the air).
    pub fetlock: [f32; 4],
    /// Neck nod, down positive.
    pub neck: f32,
    /// Head turn at the poll, down positive; it steadies the head.
    pub head: f32,
    /// Tail lift, up positive.
    pub tail: f32,
    /// Rise of the body in meters.
    pub bob: f32,
    /// Rocking of the body, nose down positive.
    pub pitch: f32,
    /// Rise of the barrel in meters while breathing.
    pub breath: f32,
}

/// Breaths per second of a standing horse.
pub const HORSE_BREATHS_PER_SECOND: f32 = 0.2;

/// Where in the cycle each leg's hoof strikes the ground, in [`Leg::ALL`]
/// order: the walk's lateral four beats (left hind, left fore, right hind,
/// right fore), the trot's diagonal pairs, and the gallop's hind-then-fore
/// sequence on the left lead.
pub fn leg_offsets(gait: HorseGait) -> [f32; 4] {
    match gait {
        HorseGait::Idle => [0.0; 4],
        HorseGait::Walk => [0.25, 0.75, 0.0, 0.5],
        HorseGait::Trot => [0.0, 0.5, 0.5, 0.0],
        HorseGait::Gallop => [0.42, 0.3, 0.12, 0.0],
    }
}

/// The part of a cycle each hoof spends on the ground.
pub fn duty(gait: HorseGait) -> f32 {
    match gait {
        HorseGait::Idle => 1.0,
        HorseGait::Walk => 0.62,
        HorseGait::Trot => 0.42,
        HorseGait::Gallop => 0.3,
    }
}

/// Where `leg` is in its own stride at `phase`: `0` as its hoof strikes.
fn leg_phase(gait: HorseGait, phase: f32, leg: Leg) -> f32 {
    wrap(phase - leg_offsets(gait)[leg.index()])
}

/// Upper-leg swing, knee or hock fold, and fetlock fold of a leg in `gait`.
fn stride(gait: HorseGait) -> (f32, f32, f32) {
    match gait {
        HorseGait::Idle => (0.0, 0.0, 0.0),
        HorseGait::Walk => (0.3, 0.75, 0.6),
        HorseGait::Trot => (0.42, 1.05, 0.8),
        HorseGait::Gallop => (0.62, 1.3, 1.0),
    }
}

/// Whether `leg`'s hoof is on the ground at `phase` of `gait`.
pub fn in_stance(gait: HorseGait, phase: f32, leg: Leg) -> bool {
    leg_phase(gait, phase, leg) < duty(gait)
}

/// The pose of a horse in `gait` at `phase` (cycles, wrapped to `0..1`).
///
/// Each leg pushes back while its hoof is down and swings forward folded
/// while it is up. The walk nods the head twice a stride, the trot keeps it
/// still and bounces the body, and the gallop rocks the body and swings the
/// neck once a stride.
pub fn horse_pose(gait: HorseGait, phase: f32) -> HorseAngles {
    use std::f32::consts::{PI, TAU};
    let phase = wrap(phase);
    let angle = phase * TAU;
    if gait == HorseGait::Idle {
        // A hind leg rests on its toe; the breath swells the barrel.
        let rest = Leg::HindRight.index();
        let mut p = HorseAngles {
            breath: 0.006 * angle.sin(),
            neck: 0.12 + 0.02 * angle.sin(),
            head: 0.05,
            ..HorseAngles::default()
        };
        p.lower[rest] = -0.18;
        p.fetlock[rest] = -0.45;
        p.upper[rest] = 0.06;
        return p;
    }
    let (swing, fold, flip) = stride(gait);
    let stance = duty(gait);
    let mut p = HorseAngles::default();
    for leg in Leg::ALL {
        let t = leg_phase(gait, phase, leg);
        let i = leg.index();
        if t < stance {
            // Hoof down: the leg sweeps from forward to back, the fetlock
            // sinking under the weight at mid stance.
            let s = t / stance;
            p.upper[i] = swing * (1.0 - 2.0 * s);
            p.fetlock[i] = 0.15 * (PI * s).sin();
        } else {
            let u = (t - stance) / (1.0 - stance);
            let ease = u * u * (3.0 - 2.0 * u);
            p.upper[i] = swing * (2.0 * ease - 1.0);
            let lift = (PI * u).sin();
            p.lower[i] = -fold * lift;
            p.fetlock[i] = -flip * lift;
        }
    }
    match gait {
        HorseGait::Walk => {
            p.neck = 0.1 + 0.07 * (2.0 * angle).sin();
            p.bob = 0.012 * (1.0 + (2.0 * angle).cos());
            p.tail = 0.05;
        }
        HorseGait::Trot => {
            p.neck = 0.05 + 0.025 * (2.0 * angle).sin();
            p.bob = 0.045 * (2.0 * angle).sin().abs();
            p.tail = 0.18;
        }
        _ => {
            p.neck = 0.18 * (angle - 0.6).sin();
            p.pitch = 0.07 * angle.cos();
            p.bob = 0.04 * (1.0 + (angle - 2.0).sin());
            p.tail = 0.4;
        }
    }
    // The head stays steadier than the neck.
    p.head = -0.4 * p.neck;
    p
}

/// Gait cycles per second of a horse in `gait` at ground `speed` (m/s): a
/// cycle is one full stride, so hooves keep pace with the ground. Standing
/// breathes at a fixed rate.
pub fn horse_cadence(gait: HorseGait, speed: f32) -> f32 {
    let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
    match gait {
        HorseGait::Idle => HORSE_BREATHS_PER_SECOND,
        // Strides of about 1.7 m, 2.6 m and 4.4 m.
        HorseGait::Walk => speed.max(0.8) / 1.7,
        HorseGait::Trot => speed.max(3.0) / 2.6,
        HorseGait::Gallop => speed.max(6.0) / 4.4,
    }
}

/// `phase` advanced by `dt` seconds of `gait` at `speed`, wrapped to `0..1`.
pub fn horse_advance(phase: f32, gait: HorseGait, speed: f32, dt: f32) -> f32 {
    wrap(phase + horse_cadence(gait, speed) * dt.max(0.0))
}

/// How a rider sits a horse in `gait` at `phase`: seated, thighs forward
/// and open round the barrel, lower legs hanging to the stirrups, hands
/// forward at the reins. The hands follow the horse's nod, and the rider
/// leans into the gallop.
pub fn riding_pose(gait: HorseGait, phase: f32) -> JointAngles {
    use std::f32::consts::TAU;
    let angle = wrap(phase) * TAU;
    // The hands follow the horse's mouth: twice a stride at the walk, once
    // at the gallop, barely at the trot.
    let (give, lean, bounce) = match gait {
        HorseGait::Idle => (0.0, 0.0, 0.0),
        HorseGait::Walk => (0.06 * (2.0 * angle).sin(), 0.03, 0.0),
        HorseGait::Trot => (0.02 * (2.0 * angle).sin(), 0.06, 0.012),
        HorseGait::Gallop => (0.14 * (angle - 0.6).sin(), 0.22, 0.02),
    };
    // The hat and poncho bounce with the horse as they would with a step.
    let step = match gait {
        HorseGait::Idle => AnimationState::Idle,
        HorseGait::Walk => AnimationState::Walk,
        HorseGait::Trot | HorseGait::Gallop => AnimationState::Gallop,
    };
    let (hat, drape, drape_side) = drape(step, phase);
    let breath = if gait == HorseGait::Idle {
        0.01 * angle.sin()
    } else {
        0.0
    };
    JointAngles {
        hip: [1.05; 2],
        knee: [-1.0; 2],
        shoulder: [0.42 + give; 2],
        elbow: [1.05 - give; 2],
        lean,
        bob: bounce * (2.0 * angle).sin().abs(),
        breath,
        nod: 0.05,
        spread: 0.55,
        hat,
        drape,
        drape_side,
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

/// How far each ear (left, right) of a horse is flicked back at `time`
/// seconds: an occasional quick twitch, deterministic per `seed`.
pub fn ear_flick(seed: u32, time: f32) -> [f32; 2] {
    let flick = |salt: u32| {
        let seed = mix(seed ^ salt);
        let period = 3.5 + (seed % 3) as f32;
        moment(seed, time, period, 0.35)
            .map_or(0.0, |(s, _)| 0.7 * (std::f32::consts::PI * s).sin())
    };
    [flick(0x1eaf), flick(0x0e44)]
}

/// Sideways swish of a horse's tail (radians, to the right positive): a
/// sway with the stride while moving, and now and then a swish at flies,
/// deterministic per `seed`.
pub fn tail_swish(seed: u32, time: f32, gait: HorseGait, phase: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let period = 4.0 + (mix(seed ^ 0x7a11) % 4) as f32;
    let at_flies = moment(seed ^ 0x7a11, time, period, 1.2).map_or(0.0, |(s, h)| {
        let side = if h & 1 == 0 { 1.0 } else { -1.0 };
        side * 0.5 * (TAU * s).sin() * (PI * s).sin()
    });
    let sway = match gait {
        HorseGait::Idle => return at_flies,
        HorseGait::Walk => 0.08,
        HorseGait::Trot => 0.1,
        HorseGait::Gallop => 0.06,
    };
    sway * (wrap(phase) * TAU).sin() + 0.5 * at_flies
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
            assert!((p.hip[0] + p.hip[1]).abs() < 1e-6, "t {t}: {p:?}");
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
        assert!(peak(Gallop) > peak(Walk) + 0.1);
        assert!(pose(Gallop, 0.1).lean > pose(Walk, 0.1).lean);
        let bob = |a| samples().map(|t| pose(a, t).bob).fold(0.0, f32::max);
        assert!(bob(Gallop) > bob(Walk));
    }

    #[test]
    fn the_phase_advances_with_speed_whatever_the_frame_rate() {
        // Proportional to speed above the slowest stride.
        let walk = |s| cadence(Walk, s);
        assert!((walk(2.4) - 2.0 * walk(1.2)).abs() < 1e-6);
        let jog = |s| cadence(Gallop, s);
        assert!((jog(6.0) - 2.0 * jog(3.0)).abs() < 1e-6);
        assert!(cadence(Walk, f32::NAN) > 0.0, "never stalls");
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

    fn fine() -> impl Iterator<Item = f32> {
        (0..200).map(|i| i as f32 / 200.0)
    }

    /// Phases where `leg`'s hoof strikes the ground.
    fn footfalls(gait: HorseGait, leg: Leg) -> Vec<f32> {
        fine()
            .filter(|&t| in_stance(gait, t, leg) && !in_stance(gait, t - 0.005, leg))
            .collect()
    }

    #[test]
    fn the_horse_trots_in_the_slow_part_of_its_gallop_range() {
        assert_eq!(HorseGait::of(Idle, 0.0), HorseGait::Idle);
        assert_eq!(HorseGait::of(Walk, 2.0), HorseGait::Walk);
        assert_eq!(HorseGait::of(Gallop, 4.0), HorseGait::Trot);
        assert_eq!(HorseGait::of(Gallop, -4.0), HorseGait::Trot);
        assert_eq!(HorseGait::of(Gallop, 11.0), HorseGait::Gallop);
        assert_eq!(HorseGait::of(Gallop, f32::NAN), HorseGait::Trot);
    }

    #[test]
    fn the_walk_has_four_evenly_spaced_beats() {
        let gait = HorseGait::Walk;
        let mut beats: Vec<f32> = Leg::ALL
            .iter()
            .map(|&leg| {
                let falls = footfalls(gait, leg);
                assert_eq!(falls.len(), 1, "{leg:?}: one footfall per stride");
                falls[0]
            })
            .collect();
        beats.sort_by(f32::total_cmp);
        for pair in beats.windows(2) {
            assert!((pair[1] - pair[0] - 0.25).abs() < 0.01, "{beats:?}");
        }
        // Lateral sequence: left hind, left fore, right hind, right fore.
        let o = leg_offsets(gait);
        let at = |leg: Leg| o[leg.index()];
        assert!(at(Leg::HindLeft) < at(Leg::FrontLeft));
        assert!(at(Leg::FrontLeft) < at(Leg::HindRight));
        assert!(at(Leg::HindRight) < at(Leg::FrontRight));
        // Always two or three hooves down.
        for t in fine() {
            let down = Leg::ALL.iter().filter(|&&l| in_stance(gait, t, l)).count();
            assert!((2..=3).contains(&down), "t {t}: {down} down");
        }
    }

    #[test]
    fn the_trot_moves_diagonal_pairs_together() {
        let gait = HorseGait::Trot;
        for t in fine() {
            let p = horse_pose(gait, t);
            let pair = |a: Leg, b: Leg| {
                let (a, b) = (a.index(), b.index());
                (p.upper[a] - p.upper[b]).abs() < 1e-6 && (p.lower[a] - p.lower[b]).abs() < 1e-6
            };
            assert!(pair(Leg::FrontLeft, Leg::HindRight), "t {t}: {p:?}");
            assert!(pair(Leg::FrontRight, Leg::HindLeft), "t {t}: {p:?}");
            // The two pairs alternate.
            let a = in_stance(gait, t, Leg::FrontLeft);
            let b = in_stance(gait, t, Leg::FrontRight);
            assert!(!(a && b), "t {t}: both pairs down");
        }
        let swing = |leg: Leg| fine().map(move |t| horse_pose(gait, t).upper[leg.index()].abs());
        assert!(swing(Leg::FrontLeft).fold(0.0, f32::max) > 0.3);
    }

    #[test]
    fn the_gallop_has_a_suspension_and_reaches_further() {
        let gait = HorseGait::Gallop;
        let airborne = fine().any(|t| Leg::ALL.iter().all(|&l| !in_stance(gait, t, l)));
        assert!(airborne, "a moment with all four hooves off the ground");
        let reach = |g: HorseGait| {
            fine()
                .map(|t| horse_pose(g, t).upper[0].abs())
                .fold(0.0, f32::max)
        };
        assert!(reach(HorseGait::Gallop) > reach(HorseGait::Trot));
        assert!(reach(HorseGait::Trot) > reach(HorseGait::Walk));
        let rise = |g: HorseGait| fine().map(|t| horse_pose(g, t).bob).fold(0.0, f32::max);
        assert!(rise(HorseGait::Gallop) > rise(HorseGait::Walk));
        // The body rocks and the neck nods with the stride.
        let pitch = fine().map(|t| horse_pose(gait, t).pitch.abs());
        assert!(pitch.fold(0.0, f32::max) > 0.03);
        let nods = |g: HorseGait| {
            let n: Vec<f32> = fine().map(|t| horse_pose(g, t).neck).collect();
            n.iter().fold(f32::MIN, |a, b| a.max(*b)) - n.iter().fold(f32::MAX, |a, b| a.min(*b))
        };
        assert!(nods(HorseGait::Gallop) > nods(HorseGait::Trot));
        assert!(nods(HorseGait::Walk) > 0.05);
    }

    #[test]
    fn legs_fold_while_in_the_air_and_never_bend_the_wrong_way() {
        for gait in [HorseGait::Walk, HorseGait::Trot, HorseGait::Gallop] {
            for leg in Leg::ALL {
                let mut folded = false;
                for t in fine() {
                    let p = horse_pose(gait, t);
                    let i = leg.index();
                    assert!(p.lower[i] <= 0.0, "{gait:?} {leg:?} t {t}");
                    if !in_stance(gait, t, leg) {
                        folded |= p.lower[i] < -0.3;
                    }
                }
                assert!(folded, "{gait:?} {leg:?} tucks in the air");
            }
        }
    }

    #[test]
    fn a_standing_horse_breathes_and_rests_one_hind_leg() {
        let mut breathes = false;
        for t in samples() {
            let p = horse_pose(HorseGait::Idle, t);
            assert_eq!(
                p.upper[Leg::FrontLeft.index()],
                p.upper[Leg::FrontRight.index()]
            );
            assert_eq!(p.bob, 0.0);
            breathes |= p.breath.abs() > 0.003;
            // One hind hoof tipped onto its toe, the other carrying weight.
            let (left, right) = (
                p.fetlock[Leg::HindLeft.index()],
                p.fetlock[Leg::HindRight.index()],
            );
            assert!((left - right).abs() > 0.2, "t {t}: {left} {right}");
        }
        assert!(breathes);
        assert_eq!(
            horse_cadence(HorseGait::Idle, 3.0),
            HORSE_BREATHS_PER_SECOND
        );
    }

    #[test]
    fn the_horse_cadence_follows_speed_whatever_the_frame_rate() {
        for gait in [HorseGait::Walk, HorseGait::Trot, HorseGait::Gallop] {
            let at = |s: f32| horse_cadence(gait, s);
            assert!(at(12.0) > at(6.0) && at(6.0) >= at(2.0), "{gait:?}");
            assert!(at(f32::NAN) > 0.0, "{gait:?} never stalls");
        }
        // A trot at 5 m/s is between a walk and a gallop in strides per second.
        assert!(horse_cadence(HorseGait::Trot, 5.0) > horse_cadence(HorseGait::Walk, 2.0));
        let mut small = 0.1;
        for _ in 0..10 {
            small = horse_advance(small, HorseGait::Trot, 5.0, 0.01);
        }
        let big = horse_advance(0.1, HorseGait::Trot, 5.0, 0.1);
        assert!((small - big).abs() < 1e-5, "{small} vs {big}");
        assert_eq!(horse_advance(0.4, HorseGait::Walk, 2.0, -1.0), 0.4);
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
        assert_eq!(glance(3, f32::NAN), 0.0);
    }

    #[test]
    fn horses_flick_their_ears_and_swish_their_tails() {
        for seed in [0, 99] {
            let ears: Vec<[f32; 2]> = times().map(|t| ear_flick(seed, t)).collect();
            assert!(ears.iter().flatten().all(|e| (0.0..=0.8).contains(e)));
            for side in 0..2 {
                assert!(
                    ears.iter().any(|e| e[side] > 0.3),
                    "seed {seed} ear {side} flicks"
                );
            }
            assert!(ears.iter().filter(|e| e[0] == 0.0).count() > ears.len() / 2);
            let idle: Vec<f32> = times()
                .map(|t| tail_swish(seed, t, HorseGait::Idle, 0.0))
                .collect();
            assert!(idle.iter().all(|s| s.abs() <= 0.6));
            assert!(idle.iter().any(|s| s.abs() > 0.25), "swishes at flies");
            assert_eq!(
                idle,
                times()
                    .map(|t| tail_swish(seed, t, HorseGait::Idle, 0.0))
                    .collect::<Vec<_>>()
            );
        }
        // Moving, the tail sways with the stride.
        let sway: Vec<f32> = samples()
            .map(|p| tail_swish(5, 0.0, HorseGait::Trot, p))
            .collect();
        assert!(sway.iter().any(|s| *s > 0.05) && sway.iter().any(|s| *s < -0.05));
        assert_eq!(ear_flick(1, f32::INFINITY), [0.0; 2]);
    }

    #[test]
    fn a_rider_sits_astride_with_hands_at_the_reins() {
        for gait in HorseGait::ALL {
            for t in samples() {
                let p = riding_pose(gait, t);
                // Thighs forward and open round the barrel, shins hanging.
                for side in 0..2 {
                    assert!(p.hip[side] > 0.8, "{gait:?} {t}: {p:?}");
                    let shin = p.hip[side] + p.knee[side];
                    assert!(shin.abs() < 0.35, "{gait:?} {t}: shin at {shin}");
                    assert!(p.shoulder[side] > 0.2 && p.elbow[side] > 0.6);
                }
                assert!(p.spread > 0.3);
                assert_eq!(p.hip[0], p.hip[1], "both legs alike");
            }
        }
        assert!(riding_pose(HorseGait::Gallop, 0.3).lean > riding_pose(HorseGait::Walk, 0.3).lean);
        // The hands give with the horse's nod.
        let hands: Vec<f32> = samples()
            .map(|t| riding_pose(HorseGait::Walk, t).shoulder[0])
            .collect();
        assert!(hands.iter().any(|h| (h - hands[0]).abs() > 0.02));
    }
}
