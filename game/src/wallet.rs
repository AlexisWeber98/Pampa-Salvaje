//! La plata del protagonista ([`PlayerWallet`]) como estado del juego, siempre
//! a la vista en el HUD ([`HudMoney`]) debajo del reloj. Empieza sin un peso.
//! Junto a ella, las pocas cosas que lleva encima ([`PlayerItems`]); las dos
//! se ven en el inventario de la I (`crate::inventory`).

use bevy::prelude::*;
use pampa_core::{Items, Money};
use pampa_render::HudMoney;

/// La plata que lleva el protagonista.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerWallet(pub Money);

/// Las cosas que lleva el protagonista (la carta de Don Hidalgo...).
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerItems(pub Items);

/// Mantiene la línea de la plata del HUD al día con la billetera.
pub(crate) fn show_money(wallet: Res<PlayerWallet>, mut money: ResMut<HudMoney>) {
    money.set_if_neq(HudMoney(Some(wallet.0.to_string())));
}
