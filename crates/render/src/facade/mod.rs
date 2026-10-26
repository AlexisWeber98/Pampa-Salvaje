//! The facade kit shared by the rural settlements and the towns: a mesh
//! builder with world-scaled UVs ([`shape`]), the shared textured PBR
//! materials ([`materials`]), walls with their openings and the roofs over
//! them ([`walls`]), and the Italianate dressing of a 1919 pampa front
//! ([`italianate`]): plinths, chamfered pilasters, moulded cornices,
//! parapets with balustrades and pedestals, framed openings and galerías.
//! [`plaza`] dresses a town's plaza with the same pieces: curbed paths,
//! the monument, benches and the railing.
//!
//! Everything is built in a structure's own space, `x` along its front,
//! `z` out of it and `y` up from the ground at its middle, into a [`Shape`]
//! that ends up as one mesh per [`Surface`] it wears.

pub mod italianate;
pub mod materials;
pub(crate) mod palenque;
pub(crate) mod plaza;
pub mod shape;
pub(crate) mod town_buildings;
pub mod walls;

pub use materials::{FacadeMaterials, Surface};

use bevy::prelude::*;

/// Builds the shared [`FacadeMaterials`] at startup. The settlement and
/// town plugins both add it when it is not there yet.
pub struct FacadePlugin;

impl Plugin for FacadePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, materials::setup_materials);
    }
}

/// Adds the [`FacadePlugin`] to `app` unless another plugin already did.
pub fn ensure_plugin(app: &mut App) {
    if !app.is_plugin_added::<FacadePlugin>() {
        app.add_plugins(FacadePlugin);
    }
}
