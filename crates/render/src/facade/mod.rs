//! El kit de fachadas que comparten los asentamientos rurales y los pueblos: un
//! constructor de meshes con UVs a escala de mundo ([`shape`]), los materiales
//! PBR texturizados compartidos ([`materials`]), las paredes con sus aberturas
//! y los techos que las cubren ([`walls`]), y la ornamentación italianizante de
//! un frente pampeano de 1919 ([`italianate`]): zócalos, pilastras ochavadas,
//! cornisas molduradas, parapetos con balaustradas y pedestales, aberturas
//! enmarcadas y galerías. [`plaza`] viste la plaza de un pueblo con las mismas
//! piezas: senderos con cordón, el monumento, los bancos y la verja.
//!
//! Todo se construye en el espacio propio de la estructura, con `x` a lo largo
//! de su frente, `z` hacia afuera y `y` hacia arriba desde el suelo en su
//! centro, dentro de un [`Shape`] que termina como una mesh por cada
//! [`Surface`] que lleva.

pub mod italianate;
pub mod materials;
pub(crate) mod palenque;
pub(crate) mod plaza;
pub mod shape;
pub(crate) mod town_buildings;
pub mod walls;

pub use materials::{FacadeMaterials, Livery, Surface};

use bevy::prelude::*;

/// Construye los [`FacadeMaterials`] compartidos al iniciar. Los plugins de
/// asentamientos y de pueblos lo agregan los dos cuando todavía no está.
pub struct FacadePlugin;

impl Plugin for FacadePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, materials::setup_materials);
    }
}

/// Agrega el [`FacadePlugin`] a `app` salvo que otro plugin ya lo haya hecho.
pub fn ensure_plugin(app: &mut App) {
    if !app.is_plugin_added::<FacadePlugin>() {
        app.add_plugins(FacadePlugin);
    }
}
