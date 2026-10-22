//! Bevy plugins that draw the Pampa Seca world.
//!
//! Everything here reads logic state from `pampa-core` and turns it into
//! pixels. Nothing here feeds back into game rules.

use std::path::PathBuf;

use bevy::prelude::Resource;

pub mod atmosphere;
pub mod bubble;
pub mod character;
pub mod coords;
pub mod fence;
pub mod flora;
pub mod font;
pub mod ground;
pub mod hud;
pub mod minimap;
pub mod obj;
pub mod rail;
pub mod road;
pub mod signs;
pub mod streaming;
pub mod terrain;
pub mod town;
pub mod train;

pub use atmosphere::{AtmospherePlugin, DUST_COLOR, Sky, Sun, WorldClock, sky_at};
pub use bubble::{Say, SpeechBubble, SpeechBubblePlugin};
pub use character::{
    CharacterVisual, CharacterVisualPlugin, Coat, FigureKind, TOWNSFOLK_LOOKS, VisualPose,
};
pub use coords::world_from_ground;
pub use fence::{FencePiece, FencePlugin, GatePart, WorldFences};
pub use flora::{
    Faded, FloraAccent, FloraPlugin, GrassTuft, SightLine, ThinFadePlugin, ThinOccluder, TreePart,
    VegetationModels, WorldFlora,
};
pub use hud::{HudObjective, HudPlugin};
pub use minimap::{MapIcon, MapTarget, MinimapPlugin};
pub use rail::{LineTrack, RailPlugin, WorldRail};
pub use road::{CrossingPart, RoadPlugin, RoadSurface, SignpostPart};
pub use signs::{SignPlugin, Signboard};
pub use streaming::{StreamingPlan, StreamingSet};
pub use terrain::{StreamFocus, TerrainChunk, TerrainPlugin, WorldChunks, WorldTerrain};
pub use town::{ActiveTowns, StreamTowns, TownPlugin, TownRoot, WindmillRotor, WorldTowns};
pub use train::{TRAIN_DRAW_RADIUS, TrainCar, TrainPlugin, TrainWheel, WorldTrain};

/// The game's `assets/` folder. Vegetation models and ground textures are
/// read from it at startup. Without it, or when a file is missing, their
/// procedural stand-ins are drawn instead.
#[derive(Resource, Debug, Clone)]
pub struct AssetRoot(pub PathBuf);
