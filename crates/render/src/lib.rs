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
pub mod facade;
pub mod fade;
pub mod fauna;
pub mod fence;
pub mod flora;
pub mod font;
pub mod gltf;
pub mod ground;
pub mod hud;
pub mod lagoon;
pub mod lamps;
pub mod minimap;
pub mod obj;
pub mod rail;
pub mod road;
pub mod settlement;
pub mod signs;
pub mod smooth;
pub mod streaming;
pub mod terrain;
pub mod town;
pub mod train;
pub mod tree;

pub use atmosphere::{AtmospherePlugin, CloseShadows, DUST_COLOR, Sky, Sun, WorldClock, sky_at};
pub use bubble::{Say, SpeechBubble, SpeechBubblePlugin};
pub use character::{
    CharacterModelsShown, CharacterVisual, CharacterVisualPlugin, Coat, EYE_IN_HEAD, FigureKind,
    OWN_HEAD_LAYER, TOWNSFOLK_LOOKS, Talking, ViewerBody, ViewerHead, VisualPose,
};
pub use coords::world_from_ground;
pub use facade::{FacadeMaterials, Surface};
pub use fade::{FadeLayer, FadePlugin, ScreenFade};
pub use fauna::{ANIMATE_FAR, Animal, AnimalPose, AnimalSeed, FaunaKit, FaunaPlugin, Gait};
pub use fence::{FencePiece, FencePlugin, GatePart, WorldFences};
pub use flora::{
    Faded, FloraAccent, FloraPlugin, GrassTuft, SightLine, ThinFadePlugin, ThinOccluder, TreePart,
    VegetationModels, WorldFlora,
};
pub use hud::{HudAnnouncement, HudHint, HudObjective, HudPlugin, HudRide};
pub use lagoon::{LagoonPlugin, LagoonReeds, LagoonWater};
pub use lamps::{LampGlass, LampLight, LampPlugin, LampPost, WorldLamps};
pub use minimap::{MapIcon, MapTarget, MinimapPlugin};
pub use rail::{LineTrack, RailPlugin, WorldRail};
pub use road::{CrossingPart, RoadPlugin, RoadSurface, SignpostPart};
pub use settlement::{
    ModelRotor, ModelSlots, SettlementPart, SettlementPlugin, SlotFit, SlotRotor, TrackSurface,
    WorldSettlements, YardPart, YardProp, YardSurface,
};
pub use signs::{SignPlugin, Signboard};
pub use streaming::{StreamingPlan, StreamingSet};
pub use terrain::{StreamFocus, TerrainChunk, TerrainPlugin, WorldChunks, WorldTerrain};
pub use town::{ActiveTowns, StreamTowns, TownPlugin, TownRoot, WindmillRotor, WorldTowns};
pub use train::{TRAIN_DRAW_RADIUS, TrainCar, TrainPlugin, TrainWheel, WorldTrain, car_transform};
pub use tree::{Species, TreeKit, TreePlugin};

/// The game's `assets/` folder. Vegetation models and ground textures are
/// read from it at startup. Without it, or when a file is missing, their
/// procedural stand-ins are drawn instead.
#[derive(Resource, Debug, Clone)]
pub struct AssetRoot(pub PathBuf);
