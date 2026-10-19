//! Rider drawing: swappable per-entity visual, 8-direction billboard sprites.

pub mod direction;
pub mod sheet;
pub mod visual;

pub use direction::{ViewDirection, ground_yaw};
pub use visual::{Billboard, RiderVisual, RiderVisualPlugin, VisualPose};
