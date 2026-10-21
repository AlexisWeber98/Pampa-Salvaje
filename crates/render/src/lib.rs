//! Bevy plugins that draw the Pampa Seca world.
//!
//! Everything here reads logic state from `pampa-core` and turns it into
//! pixels. Nothing here feeds back into game rules.

pub mod atmosphere;
pub mod bubble;
pub mod character;
pub mod coords;
pub mod font;
pub mod hud;
pub mod minimap;
pub mod rail;
pub mod signs;
pub mod streaming;
pub mod terrain;
pub mod town;
pub mod train;

pub use atmosphere::{AtmospherePlugin, DUST_COLOR, Sky, Sun, WorldClock, sky_at};
pub use bubble::{Say, SpeechBubble, SpeechBubblePlugin};
pub use character::{
    CharacterVisual, CharacterVisualPlugin, FigureKind, SheetKind, TOWNSFOLK_LOOKS, ViewDirection,
    VisualPose,
};
pub use coords::world_from_ground;
pub use hud::{HudObjective, HudPlugin};
pub use minimap::{MapIcon, MapTarget, MinimapPlugin};
pub use rail::{LineTrack, RailPlugin, WorldRail};
pub use signs::{SignPlugin, Signboard};
pub use streaming::{StreamingPlan, StreamingSet};
pub use terrain::{StreamFocus, TerrainChunk, TerrainPlugin, WorldChunks, WorldTerrain};
pub use town::{ActiveTowns, StreamTowns, TownPlugin, TownRoot, WindmillRotor, WorldTowns};
pub use train::{TRAIN_DRAW_RADIUS, TrainCar, TrainPlugin, TrainWheel, WorldTrain};
