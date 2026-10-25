//! Character drawing. Every person on foot is a skinned MakeHuman model that
//! plays retargeted animation clips (see [`model`], [`clips`] and [`rig`]):
//! the protagonist, the errand's people and the townsfolk, five outfits
//! tinted per person ([`townsfolk`]). The low-poly rigid figures
//! ([`figure`], [`finish`]) stay as their hidden carriers and as `models
//! off` (and a model that fails to load). Saddled
//! horses are Quaternius' animated horse in a recado and bridle ([`horse`],
//! [`tack`]), with a model rider on top ([`rider`]). Each entity's visual
//! is swappable (on foot, mounted, a horse tied up).

pub mod clips;
#[cfg(test)]
pub(crate) mod cpu;
pub mod figure;
pub mod finish;
pub mod gait;
pub mod horse;
pub mod model;
pub mod rider;
pub mod rig;
pub mod tack;
pub mod townsfolk;
pub mod visual;

pub use clips::CharacterClips;
pub use figure::{FigureBody, FigureJoint, FigureKind, FigurePart, TOWNSFOLK_LOOKS};
pub use horse::{Coat, HorseBody, HorseModel, HorseRig};
pub use model::{CharacterModel, CharacterModels, CharacterModelsShown};
pub use rider::{RiderMotion, Riding, Saddle, Seated};
pub use rig::{Rig, Talking};
pub use tack::{HorsePiece, Strap, Tack};
pub use townsfolk::TintCache;
pub use visual::{CharacterVisual, CharacterVisualPlugin, VisualPart, VisualPose};
