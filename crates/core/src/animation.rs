//! Logical animation state. Render decides how it is drawn: joint poses
//! ([`crate::pose`]) for people and horses.

/// What the rider is doing, independent of how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnimationState {
    #[default]
    Idle,
    Walk,
    /// A gallop on horseback; a jog on foot.
    Gallop,
}

impl AnimationState {
    /// Speed (m/s, absolute) below which the rider counts as standing still.
    pub const IDLE_THRESHOLD: f32 = 0.1;

    /// Classifies a ground speed given the top walking speed.
    pub fn from_speed(speed: f32, walk_max: f32) -> Self {
        let speed = speed.abs();
        if speed < Self::IDLE_THRESHOLD {
            Self::Idle
        } else if speed <= walk_max + Self::IDLE_THRESHOLD {
            Self::Walk
        } else {
            Self::Gallop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_speed() {
        assert_eq!(AnimationState::from_speed(0.0, 2.0), AnimationState::Idle);
        assert_eq!(AnimationState::from_speed(0.05, 2.0), AnimationState::Idle);
        assert_eq!(AnimationState::from_speed(1.5, 2.0), AnimationState::Walk);
        assert_eq!(AnimationState::from_speed(-1.0, 2.0), AnimationState::Walk);
        assert_eq!(AnimationState::from_speed(2.0, 2.0), AnimationState::Walk);
        assert_eq!(AnimationState::from_speed(5.0, 2.0), AnimationState::Gallop);
    }
}
