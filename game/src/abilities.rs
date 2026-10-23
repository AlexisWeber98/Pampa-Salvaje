//! The protagonist's abilities ([`Abilities`]) as game state, which other
//! systems ask before letting the player do something (see
//! [`crate::errand`] for horses).
//!
//! Riding unlocks the first time the protagonist mounts Don Hidalgo's
//! horse: from then on any horse lets him on. Each unlock shows a short
//! notice in the HUD's announcement panel.

use bevy::prelude::*;
use pampa_core::{Abilities, Ability, ErrandEvent};

use crate::errand::ErrandProgress;
use crate::ride::Announcer;

/// What the protagonist can do so far.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerAbilities(pub Abilities);

/// An ability just unlocked (each one is written once).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unlocked(pub Ability);

impl PlayerAbilities {
    /// Unlocks `ability`, telling `unlocked` if it was locked until now.
    pub fn unlock(&mut self, ability: Ability, unlocked: &mut MessageWriter<Unlocked>) {
        if self.0.unlock(ability) {
            unlocked.write(Unlocked(ability));
        }
    }
}

/// Unlocks riding as the protagonist first mounts Don Hidalgo's horse.
pub(crate) fn unlock_on_mount(
    mut progress: MessageReader<ErrandProgress>,
    mut abilities: ResMut<PlayerAbilities>,
    mut unlocked: MessageWriter<Unlocked>,
) {
    for event in progress.read() {
        if event.0 == ErrandEvent::MountedHorse {
            abilities.unlock(Ability::Ride, &mut unlocked);
        }
    }
}

/// Shows each unlock's notice in the announcement panel.
pub(crate) fn announce_unlocks(mut unlocked: MessageReader<Unlocked>, mut announcer: Announcer) {
    // Several at once (all unlocked together): the last one shows.
    if let Some(last) = unlocked.read().last() {
        announcer.show(last.0.notice().to_string());
    }
}
