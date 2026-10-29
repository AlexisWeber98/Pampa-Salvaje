//! Correspondencia entre las coordenadas de suelo de `pampa-core` y el espacio
//! del mundo de Bevy.
//!
//! Core usa `x` hacia el este e `y` hacia el norte sobre el plano del suelo,
//! más una altura aparte. Bevy usa Y hacia arriba y -Z hacia adelante, así que
//! el norte corresponde a -Z.

use bevy::math::Vec3;
use pampa_core::Vec2;

/// Posición en el espacio del mundo de un punto del plano del suelo elevado a
/// `height`.
pub fn world_from_ground(point: Vec2, height: f32) -> Vec3 {
    Vec3::new(point.x, height, -point.y)
}
