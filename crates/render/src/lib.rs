//! Bevy plugins that draw the Pampa Seca world.
//!
//! Everything here reads logic state from `pampa-core` and turns it into
//! pixels. Nothing here feeds back into game rules.

pub mod atmosphere;
pub mod coords;
pub mod rider;
pub mod terrain;
pub mod town;

pub use atmosphere::{AtmospherePlugin, DUST_COLOR};
pub use coords::world_from_ground;
pub use rider::{RiderVisual, RiderVisualPlugin, ViewDirection, VisualPose};
pub use terrain::{TerrainPlugin, WorldTerrain};
pub use town::{TownPlugin, WindmillRotor, WorldTown};
