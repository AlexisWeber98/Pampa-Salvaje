//! Character drawing. The protagonist and the errand's people are skinned
//! MakeHuman models that play retargeted animation clips (see [`model`],
//! [`clips`] and [`rig`]); the townsfolk are low-poly 3D figures built from
//! rigid lofted parts, posed by `pampa_core::pose`, with a PBR finish per
//! material and small painted textures ([`figure`], [`finish`]). Saddled
//! horses are Quaternius' animated horse in a recado and bridle ([`horse`],
//! [`tack`]). Each entity's visual
//! is swappable (on foot, mounted, a horse tied up).

pub mod clips;
#[cfg(test)]
pub(crate) mod cpu;
pub mod figure;
pub mod finish;
pub mod gait;
pub mod horse;
pub mod model;
pub mod rig;
pub mod tack;
pub mod visual;

pub use clips::CharacterClips;
pub use figure::{FigureBody, FigureJoint, FigureKind, FigurePart, TOWNSFOLK_LOOKS};
pub use horse::{Coat, HorseBody, HorseModel, HorseRig};
pub use model::{CharacterModel, CharacterModels, CharacterModelsShown};
pub use rig::{Rig, Talking};
pub use tack::{HorsePiece, Strap, Tack};
pub use visual::{CharacterVisual, CharacterVisualPlugin, VisualPart, VisualPose};
