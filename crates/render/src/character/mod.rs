//! Character drawing: people on foot are low-poly 3D figures built from
//! rigid parts; horses and the mounted rider are still 8-direction
//! billboard sprites. Each entity's visual is swappable.

pub mod direction;
pub mod figure;
mod paint;
pub mod sheet;
pub mod visual;

pub use direction::{ViewDirection, ground_yaw};
pub use figure::{FigureBody, FigureJoint, FigureKind, FigurePart, TOWNSFOLK_LOOKS};
pub use sheet::{FRAME_SIZE, GAIT_FRAMES, SheetKind};
pub use visual::{Billboard, CharacterVisual, CharacterVisualPlugin, VisualPart, VisualPose};
