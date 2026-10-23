//! What the protagonist has learned to do: abilities unlocked by the story
//! (or, in development, all at once), which the game asks before letting
//! the player do something.
//!
//! Riding is the first: before the protagonist first mounts Don Hidalgo's
//! horse (the one he lends) no other horse lets him on; from then on any
//! horse does. The lasso, weapons and cars are kept for later.

/// One thing the protagonist may learn to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ability {
    /// Mount any horse, not only the one Don Hidalgo lends.
    Ride,
    /// Throw the lasso.
    Lasso,
    /// Carry and use weapons.
    Weapons,
    /// Drive cars.
    Cars,
}

impl Ability {
    /// Every ability, in the order they are listed.
    pub const ALL: [Self; 4] = [Self::Ride, Self::Lasso, Self::Weapons, Self::Cars];

    fn bit(self) -> u8 {
        1 << self as u8
    }

    /// The short notice the player reads when it unlocks.
    pub fn notice(self) -> &'static str {
        match self {
            Self::Ride => "Ahora podés montar cualquier caballo",
            Self::Lasso => "Ahora podés usar el lazo",
            Self::Weapons => "Ahora podés usar armas",
            Self::Cars => "Ahora podés manejar autos",
        }
    }
}

/// The set of abilities unlocked so far; none at the start of the game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Abilities(u8);

impl Abilities {
    /// Every ability unlocked.
    pub fn all() -> Self {
        Self(Ability::ALL.iter().fold(0, |bits, a| bits | a.bit()))
    }

    /// Whether `ability` is unlocked.
    pub fn has(self, ability: Ability) -> bool {
        self.0 & ability.bit() != 0
    }

    /// Unlocks `ability`; `true` if it was locked until now.
    pub fn unlock(&mut self, ability: Ability) -> bool {
        let new = !self.has(ability);
        self.0 |= ability.bit();
        new
    }

    /// Unlocks every ability and returns those that were locked until now.
    pub fn unlock_all(&mut self) -> Vec<Ability> {
        Ability::ALL
            .into_iter()
            .filter(|&a| self.unlock(a))
            .collect()
    }

    /// Whether the player may get on a horse: Don Hidalgo's (`lent`) always,
    /// any other once [`Ability::Ride`] is unlocked.
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
