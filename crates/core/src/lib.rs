//! Pure game logic for Pampa Seca.
//!
//! This crate must never depend on Bevy or any renderer: game logic never
//! knows how things are drawn.
//!
//! # Coordinate convention
//!
//! Logic lives on a 2D ground plane: `x` points east, `y` points north.
//! Height above the ground plane is a separate scalar sampled from
//! [`Terrain`]. Angles are radians, counter-clockwise from east, so a
//! positive turn is a turn to the left.

pub mod activation;
pub mod animation;
pub mod bumps;
pub mod chunk;
pub mod collision;
pub mod heading;
pub mod math;
pub mod obstacles;
pub mod placement;
pub mod places;
pub mod rider;
mod rng;
pub mod terrain;
pub mod town;
pub mod townsfolk;
pub mod train;
pub mod world;

pub use activation::{ActivationChange, TownActivation, distance_to_town, nearest_town};
pub use animation::AnimationState;
pub use bumps::{BumpEvent, BumpSeverity, BumpTracker, LinePicker};
pub use chunk::{ChunkCoord, WorldConfig, WorldConfigError};
pub use collision::{Aabb, Body, Circle, SideObstacle};
pub use heading::Heading;
pub use math::Vec2;
pub use obstacles::StaticObstacles;
pub use placement::{PlacementError, place_towns};
pub use places::{Place, PlaceKind, Sign, SignMount};
pub use rider::{MAX_STEP, Rider, RiderInput, RiderParams};
pub use terrain::{FlatArea, Terrain};
pub use town::{
    Building, BuildingKind, HitchingPost, PlazaPlacement, Railway, RailwayConfig, Side,
    TetheredHorse, TownConfig, TownConfigError, TownLayout,
};
pub use townsfolk::{StreetGraph, Townsfolk, Townsperson};
pub use train::{Car, CarKind, PlacedCar, Train, TrainPhase, TrainSchedule, TrainState};
pub use world::{World, WorldBounds};
