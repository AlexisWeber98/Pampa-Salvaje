//! Estado lógico de la animación. El render decide cómo se dibuja: poses de
//! articulaciones ([`crate::pose`]) para personas y caballos.

/// Lo que está haciendo el jinete, sin importar cómo se dibuja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnimationState {
    #[default]
    Idle,
    Walk,
    /// Un galope a caballo; un trote a pie.
    Gallop,
}

impl AnimationState {
    /// Velocidad (m/s, en valor absoluto) por debajo de la cual se considera
    /// que el jinete está quieto.
    pub const IDLE_THRESHOLD: f32 = 0.1;

    /// Clasifica una velocidad sobre el suelo a partir de la velocidad máxima
    /// al caminar.
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
