//! Las habilidades del protagonista ([`Abilities`]) como estado del juego, que
//! otros sistemas consultan antes de dejar que el jugador haga algo (ver
//! [`crate::errand`] para los caballos).
//!
//! Montar se desbloquea la primera vez que el protagonista monta el caballo de
//! Don Hidalgo: desde entonces cualquier caballo lo deja subir. Cada desbloqueo
//! muestra un breve aviso en el panel de anuncios del HUD.
//!
//! Con todas las habilidades desbloqueadas (el comando `unlock all` de la
//! consola), el mapa ampliado muestra todo el mapa del juego ([`MapReach`]); si
//! no, unos kilómetros alrededor del jinete.

use bevy::prelude::*;
use pampa_core::{Aabb, Abilities, Ability, ErrandEvent};
use pampa_render::MapReach;

use crate::errand::ErrandProgress;
use crate::player::GameWorld;
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

/// Con todo desbloqueado, el mapa ampliado muestra todo el mapa del juego; si
/// no, la zona alrededor del jinete.
pub(crate) fn reveal_map(
    abilities: Res<PlayerAbilities>,
    world: Res<GameWorld>,
    mut reach: ResMut<MapReach>,
) {
    let bounds = world.0.bounds;
    let wanted = if abilities.0 == Abilities::all() {
        MapReach::Whole(Aabb {
            min: bounds.min,
            max: bounds.max,
        })
    } else {
        MapReach::Around
    };
    reach.set_if_neq(wanted);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocking_everything_shows_the_whole_map_when_enlarged() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PlayerAbilities>()
            .init_resource::<GameWorld>()
            .init_resource::<MapReach>()
            .add_systems(Update, reveal_map);
        app.world_mut()
            .resource_mut::<PlayerAbilities>()
            .0
            .unlock(Ability::Ride);
        app.update();
        assert_eq!(*app.world().resource::<MapReach>(), MapReach::Around);
        app.world_mut().resource_mut::<PlayerAbilities>().0 = Abilities::all();
        app.update();
        let bounds = app.world().resource::<GameWorld>().0.bounds;
        assert_eq!(
            *app.world().resource::<MapReach>(),
            MapReach::Whole(Aabb {
                min: bounds.min,
                max: bounds.max
            })
        );
    }
}
