//! Rigid-part poses for people on foot: how far each joint turns at a point
//! of the gait cycle. Render rotates the parts of a low-poly figure by these
//! angles; logic never needs them.
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
    if animation == AnimationState::Idle {
        return JointAngles {
            elbow: [elbow; 2],
            breath: 0.012 * angle.sin(),
            nod: 0.02 * angle.sin(),
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
}
