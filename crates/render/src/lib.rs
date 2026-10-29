//! Plugins de Bevy que dibujan el mundo de Pampa Salvaje.
//!
//! Todo lo que hay aquí lee el estado lógico de `pampa-core` y lo convierte en
//! píxeles. Nada de esto retroalimenta las reglas del juego.

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
pub mod gltf;
pub mod grass;
pub mod ground;
pub mod hud;
pub mod lagoon;
pub mod lamps;
pub mod lettering;
pub mod minimap;
pub mod obj;
pub mod rail;
mod railbed;
pub mod road;
pub mod settlement;
pub mod signs;
pub mod sky;
pub mod smooth;
pub mod streaming;
pub mod terrain;
pub mod town;
pub mod train;
pub mod tree;
pub mod typeface;

pub use atmosphere::{AtmospherePlugin, CloseShadows, DUST_COLOR, Sky, Sun, WorldClock, sky_at};
pub use bubble::{Say, SpeechBubble, SpeechBubblePlugin};
pub use character::{
    CharacterModelsShown, CharacterVisual, CharacterVisualPlugin, Coat, EYE_IN_HEAD, FigureKind,
    OWN_HEAD_LAYER, TOWNSFOLK_LOOKS, Talking, ViewerBody, ViewerHead, VisualPose,
};
pub use coords::world_from_ground;
pub use facade::{FacadeMaterials, Livery, Surface};
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

pub use signs::{SignPlugin, Signboard, TIMETABLE_BOARD_TEXT, TimetableBoard};
pub use sky::SkyPlugin;
pub use streaming::{StreamingPlan, StreamingSet};
pub use terrain::{StreamFocus, TerrainChunk, TerrainPlugin, WorldChunks, WorldTerrain};
pub use town::{ActiveTowns, StreamTowns, TownPlugin, TownRoot, WindmillRotor, WorldTowns};
pub use train::{TRAIN_DRAW_RADIUS, TrainCar, TrainPlugin, TrainWheel, WorldTrains, car_transform};
pub use tree::{Species, TreeKit, TreePlugin};
pub use typeface::{TypefacePlugin, Typefaces};

/// La carpeta `assets/` del juego. De ella se leen al inicio los modelos de
/// vegetación y las texturas del suelo. Sin ella, o cuando falta un archivo, se
/// dibujan en su lugar sus sustitutos procedurales.
#[derive(Resource, Debug, Clone)]
pub struct AssetRoot(pub PathBuf);
