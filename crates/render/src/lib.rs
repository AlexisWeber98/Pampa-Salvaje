//! Bevy plugins that draw the Pampa Seca world.
//!
//! Everything here reads logic state from `pampa-core` and turns it into
//! pixels. Nothing here feeds back into game rules.

pub mod atmosphere;
pub mod character;
pub mod coords;
pub mod terrain;
pub mod town;
pub mod train;

pub use atmosphere::{AtmospherePlugin, DUST_COLOR};
pub use character::{
    CharacterVisual, CharacterVisualPlugin, PERSON_LOOKS, SheetKind, ViewDirection, VisualPose,
};
pub use coords::world_from_ground;
pub use terrain::{TerrainPlugin, WorldTerrain};
pub use town::{TownPlugin, WindmillRotor, WorldTown};
pub use train::{TrainCar, TrainPlugin, TrainWheel, WorldTrain};
