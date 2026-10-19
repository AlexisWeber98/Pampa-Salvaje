//! Character drawing (riders, horses, people on foot): a swappable per-entity
//! visual drawn as 8-direction billboard sprites.

pub mod direction;
pub mod sheet;
pub mod visual;

pub use direction::{ViewDirection, ground_yaw};
pub use sheet::{PERSON_LOOKS, SheetKind};
pub use visual::{Billboard, CharacterVisual, CharacterVisualPlugin, VisualPose};
