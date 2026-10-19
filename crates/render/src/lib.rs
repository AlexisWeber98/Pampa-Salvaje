//! Bevy plugins that draw the Pampa Seca world.
//!
//! Everything here reads logic state from `pampa-core` and turns it into
//! pixels. Nothing here feeds back into game rules.

pub mod atmosphere;
pub mod bubble;
pub mod character;
pub mod coords;
pub mod font;
pub mod minimap;
pub mod signs;
pub mod streaming;
pub mod terrain;
pub mod town;
pub mod train;

pub use atmosphere::{AtmospherePlugin, DUST_COLOR};
pub use bubble::{Say, SpeechBubble, SpeechBubblePlugin};
pub use character::{
    CharacterVisual, CharacterVisualPlugin, PERSON_LOOKS, SheetKind, ViewDirection, VisualPose,
};
pub use coords::world_from_ground;
pub use minimap::{MapIcon, MinimapPlugin};
pub use signs::{SignPlugin, Signboard};
pub use streaming::{StreamingPlan, StreamingSet};
pub use terrain::{StreamFocus, TerrainChunk, TerrainPlugin, WorldChunks, WorldTerrain};
pub use town::{ActiveTowns, StreamTowns, TownPlugin, TownRoot, WindmillRotor, WorldTowns};
pub use train::{TrainCar, TrainPlugin, TrainWheel, WorldTrains};
