//! Character drawing: people and horses are low-poly 3D figures built from
//! rigid lofted parts, posed by `pampa_core::pose` (or, for the errand's
//! people and the protagonist on foot, skinned MakeHuman models whose bones
//! take the same pose: see [`model`] and [`rig`]), with a PBR finish per
//! material and small painted textures. Each entity's visual is
//! swappable (on foot, mounted, a horse tied up).

pub mod figure;
pub mod finish;
pub mod horse;
pub mod model;
pub mod rig;
pub mod visual;

pub use figure::{FigureBody, FigureJoint, FigureKind, FigurePart, TOWNSFOLK_LOOKS};
pub use horse::{Coat, HorseBody, HorseJointPart, HorsePartMesh};
pub use model::{CharacterModel, CharacterModels, CharacterModelsShown};
pub use rig::{Bone, Rig, RigPose};
pub use visual::{CharacterVisual, CharacterVisualPlugin, VisualPart, VisualPose};
