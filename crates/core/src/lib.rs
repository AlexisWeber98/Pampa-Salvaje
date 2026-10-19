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

pub use animation::AnimationState;
pub use collision::Circle;
pub use heading::Heading;
pub use math::Vec2;
pub use rider::{Rider, RiderInput, RiderParams};
pub use terrain::Terrain;
