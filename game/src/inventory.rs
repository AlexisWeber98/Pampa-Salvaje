//! El inventario en pantalla: I lo abre y lo cierra sobre el juego, que sigue
//! andando detrás. Muestra la plata de [`PlayerWallet`] (escrita igual que en
//! el HUD) y las cosas de [`PlayerItems`], y se mantiene al día mientras está
//! abierto: si Lisandro recibe o entrega algo, el panel cambia. El menú de pausa
//! lo cierra (ver `crate::menu`). Durante una
//! escena ([`ControlLock`]) la I no hace nada.

use bevy::prelude::*;
use pampa_render::{InventoryContents, InventoryPlugin as InventoryPanelPlugin, InventoryView};

use crate::player::ControlLock;
use crate::wallet::{PlayerItems, PlayerWallet};

/// La tecla que abre y cierra el inventario.
pub const INVENTORY_KEY: KeyCode = KeyCode::KeyI;

/// Si el inventario está abierto.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InventoryOpen(pub bool);

/// Abre y cierra el inventario con I, y lo mantiene al día.
pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<InventoryPanelPlugin>() {
            app.add_plugins(InventoryPanelPlugin);
        }
        app.init_resource::<InventoryOpen>()
            .init_resource::<PlayerWallet>()
            .init_resource::<PlayerItems>()
            .init_resource::<ControlLock>()
            .add_systems(Update, (toggle, show).chain());
    }
}

/// I abre o cierra el inventario, salvo durante una escena.
fn toggle(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    lock: Res<ControlLock>,
    mut open: ResMut<InventoryOpen>,
) {
    if !lock.0 && keys.is_some_and(|k| k.just_pressed(INVENTORY_KEY)) {
        open.0 = !open.0;
    }
}

/// Escribe en [`InventoryView`] lo que lleva el protagonista mientras el
/// inventario está abierto; cerrado, lo oculta.
fn show(
    open: Res<InventoryOpen>,
    wallet: Res<PlayerWallet>,
    items: Res<PlayerItems>,
    mut view: ResMut<InventoryView>,
) {
    let contents = open.0.then(|| InventoryContents {
        money: wallet.0.to_string(),
        items: items.0.clone(),
    });
    view.set_if_neq(InventoryView(contents));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};
    use bevy::input::{ButtonState, InputPlugin};
    use pampa_core::{Item, Money};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin, InventoryPlugin));
        app.update();
        app
    }

    /// Una tecla que baja y sube, tal como la envía la ventana.
    fn press(app: &mut App, key: KeyCode) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            app.world_mut().write_message(KeyboardInput {
                key_code: key,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
            app.update();
        }
    }

    fn view(app: &App) -> Option<InventoryContents> {
        app.world().resource::<InventoryView>().0.clone()
    }

    #[test]
    fn i_opens_and_closes_the_inventory_with_the_money_and_the_things() {
        let mut app = app();
        app.world_mut().resource_mut::<PlayerWallet>().0 = Money::new(3, 0);
        app.world_mut()
            .resource_mut::<PlayerItems>()
            .0
            .receive(Item::Letter);
        assert_eq!(view(&app), None, "closed at first");

        press(&mut app, INVENTORY_KEY);
        let shown = view(&app).expect("I opens it");
        assert_eq!(shown.money, "$ 3,00");
        assert!(shown.items.has(Item::Letter));

        press(&mut app, INVENTORY_KEY);
        assert_eq!(view(&app), None, "I again closes it");
    }

    #[test]
    fn i_does_nothing_while_a_scene_plays() {
        let mut app = app();
        app.world_mut().resource_mut::<ControlLock>().0 = true;
        press(&mut app, INVENTORY_KEY);
        assert_eq!(view(&app), None);
    }

    #[test]
    fn the_open_inventory_follows_what_is_received_and_handed_over() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<PlayerItems>()
            .0
            .receive(Item::Letter);
        press(&mut app, INVENTORY_KEY);

        let mut items = app.world_mut().resource_mut::<PlayerItems>();
        items.0.hand_over(Item::Letter);
        items.0.receive(Item::Cheese);
        app.update();
        let shown = view(&app).expect("still open");
        assert!(shown.items.has(Item::Cheese));
        assert!(!shown.items.has(Item::Letter));
    }
}
