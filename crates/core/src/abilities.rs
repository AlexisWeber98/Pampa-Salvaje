//! Lo que el protagonista aprendió a hacer: habilidades que desbloquea la
//! historia (o, en desarrollo, todas a la vez) y que el juego consulta antes de
//! dejar que el jugador haga algo.
//!
//! Montar es la primera: antes de que el protagonista monte por primera vez el
//! caballo de Don Hidalgo (el que él presta), ningún otro caballo lo deja
//! subir; a partir de entonces, cualquier caballo lo hace. El lazo, las armas y
//! los autos quedan para más adelante.

/// Algo que el protagonista puede aprender a hacer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ability {
    /// Montar cualquier caballo, no solo el que presta Don Hidalgo.
    Ride,
    /// Tirar el lazo.
    Lasso,
    /// Llevar y usar armas.
    Weapons,
    /// Conducir autos.
    Cars,
}

impl Ability {
    /// Todas las habilidades, en el orden en que se listan.
    pub const ALL: [Self; 4] = [Self::Ride, Self::Lasso, Self::Weapons, Self::Cars];

    fn bit(self) -> u8 {
        1 << self as u8
    }

    /// El aviso breve que lee el jugador cuando se desbloquea.
    pub fn notice(self) -> &'static str {
        match self {
            Self::Ride => "Ahora podés montar cualquier caballo",
            Self::Lasso => "Ahora podés usar el lazo",
            Self::Weapons => "Ahora podés usar armas",
            Self::Cars => "Ahora podés manejar autos",
        }
    }
}

/// El conjunto de habilidades desbloqueadas hasta ahora; ninguna al empezar el
/// juego.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Abilities(u8);

impl Abilities {
    /// Todas las habilidades desbloqueadas.
    pub fn all() -> Self {
        Self(Ability::ALL.iter().fold(0, |bits, a| bits | a.bit()))
    }

    /// Si `ability` está desbloqueada.
    pub fn has(self, ability: Ability) -> bool {
        self.0 & ability.bit() != 0
    }

    /// Desbloquea `ability`; `true` si estaba bloqueada hasta ahora.
    pub fn unlock(&mut self, ability: Ability) -> bool {
        let new = !self.has(ability);
        self.0 |= ability.bit();
        new
    }

    /// Desbloquea todas las habilidades y devuelve las que estaban bloqueadas
    /// hasta ahora.
    pub fn unlock_all(&mut self) -> Vec<Ability> {
        Ability::ALL
            .into_iter()
            .filter(|&a| self.unlock(a))
            .collect()
    }

    /// Si el jugador puede subirse a un caballo: al de Don Hidalgo (`lent`)
    /// siempre, a cualquier otro una vez desbloqueada [`Ability::Ride`].
    pub fn may_mount(self, lent: bool) -> bool {
        lent || self.has(Ability::Ride)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_starts_with_nothing_unlocked() {
        let start = Abilities::default();
        for a in Ability::ALL {
            assert!(!start.has(a), "{a:?}");
        }
    }

    #[test]
    fn unlocking_is_reported_once_and_keeps_the_others() {
        let mut abilities = Abilities::default();
        assert!(abilities.unlock(Ability::Ride));
        assert!(!abilities.unlock(Ability::Ride), "only once");
        assert!(abilities.has(Ability::Ride));
        assert!(!abilities.has(Ability::Lasso));
        assert!(abilities.unlock(Ability::Cars));
        assert!(abilities.has(Ability::Ride) && abilities.has(Ability::Cars));
    }

    #[test]
    fn unlock_all_returns_only_the_newly_unlocked() {
        let mut abilities = Abilities::default();
        abilities.unlock(Ability::Ride);
        let new = abilities.unlock_all();
        assert_eq!(new, [Ability::Lasso, Ability::Weapons, Ability::Cars]);
        assert_eq!(abilities, Abilities::all());
        assert!(abilities.unlock_all().is_empty());
    }

    #[test]
    fn only_the_lent_horse_is_mountable_until_riding_is_unlocked() {
        let mut abilities = Abilities::default();
        assert!(abilities.may_mount(true));
        assert!(!abilities.may_mount(false));
        abilities.unlock(Ability::Ride);
        assert!(abilities.may_mount(true) && abilities.may_mount(false));
    }

    #[test]
    fn every_ability_has_a_notice() {
        for a in Ability::ALL {
            assert!(a.notice().starts_with("Ahora podés"), "{a:?}");
        }
    }
}
