//! Las habilidades del protagonista ([`Abilities`]) como estado del juego, que
//! otros sistemas consultan antes de dejar que el jugador haga algo (ver
//! [`crate::errand`] para los caballos).
//!
//! Montar se desbloquea la primera vez que el protagonista monta el caballo de
//! Don Hidalgo: desde entonces cualquier caballo lo deja subir. Cada desbloqueo
//! muestra un breve aviso en el panel de anuncios del HUD.

use bevy::prelude::*;
use pampa_core::{Abilities, Ability, ErrandEvent};

use crate::errand::ErrandProgress;
use crate::ride::Announcer;

/// Lo que el protagonista puede hacer hasta ahora.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerAbilities(pub Abilities);

/// Una habilidad recién desbloqueada (cada una se escribe una sola vez).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unlocked(pub Ability);

impl PlayerAbilities {
    /// Desbloquea `ability` y avisa a `unlocked` si estaba bloqueada hasta
    /// ahora.
    pub fn unlock(&mut self, ability: Ability, unlocked: &mut MessageWriter<Unlocked>) {
        if self.0.unlock(ability) {
            unlocked.write(Unlocked(ability));
        }
    }
}

/// Desbloquea la monta cuando el protagonista monta por primera vez el caballo
/// de Don Hidalgo.
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

/// Muestra el aviso de cada desbloqueo en el panel de anuncios.
pub(crate) fn announce_unlocks(mut unlocked: MessageReader<Unlocked>, mut announcer: Announcer) {
    // Varios a la vez (desbloqueados juntos): se muestra el último.
    if let Some(last) = unlocked.read().last() {
        announcer.show(last.0.notice().to_string());
    }
}
