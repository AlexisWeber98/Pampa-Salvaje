//! Character drawing: people and horses are low-poly 3D figures built from
//! rigid lofted parts, posed by `pampa_core::pose`. Each entity's visual is
//! swappable (on foot, mounted, a horse tied up).

pub mod figure;
pub mod horse;
pub mod visual;

pub use figure::{FigureBody, FigureJoint, FigureKind, FigurePart, TOWNSFOLK_LOOKS};
pub use horse::{Coat, HorseBody, HorseJointPart, HorsePartMesh};
pub use visual::{CharacterVisual, CharacterVisualPlugin, VisualPart, VisualPose};
