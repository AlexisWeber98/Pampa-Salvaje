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

pub mod animation;
pub mod collision;
pub mod heading;
pub mod math;
pub mod rider;
pub mod terrain;
pub mod town;
pub mod townsfolk;
pub mod train;
pub mod world;

pub use animation::AnimationState;
pub use collision::{Aabb, Body, Circle, SideObstacle};
pub use heading::Heading;
pub use math::Vec2;
pub use rider::{MAX_STEP, Rider, RiderInput, RiderParams};
pub use terrain::{FlatArea, Terrain};
pub use town::{Building, BuildingKind, HitchingPost, Railway, Side, TetheredHorse, TownLayout};
pub use townsfolk::{StreetGraph, Townsfolk, Townsperson};
pub use train::{Car, CarKind, PlacedCar, Train, TrainPhase, TrainSchedule, TrainState};
pub use world::{PATCH_HALF_EXTENT, World, WorldBounds};
