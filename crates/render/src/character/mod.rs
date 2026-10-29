//! Dibujo de los personajes. Cada persona a pie es un modelo de MakeHuman con
//! skinning que reproduce clips de animación retargeteados (ver [`model`],
//! [`clips`] y [`rig`]): el protagonista, la gente de los mandados y los
//! vecinos, con cinco atuendos teñidos por persona ([`townsfolk`]). Las figuras
//! rígidas low-poly ([`figure`], [`finish`]) quedan como sus portadores ocultos
//! y como `models off` (y cuando un modelo no carga). Los caballos ensillados
//! son el caballo animado de Quaternius con recado y brida ([`horse`],
//! [`tack`]), con un jinete modelo encima ([`rider`]). El visual de cada
//! entidad es intercambiable (a pie, montado, un caballo atado). La cabeza del
//! propio jugador se marca para la cámara en primera persona ([`viewer`]).

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
pub mod viewer;
pub mod visual;

pub use clips::CharacterClips;
pub use figure::{FigureBody, FigureJoint, FigureKind, FigurePart, TOWNSFOLK_LOOKS};
pub use horse::{Coat, HorseBody, HorseModel, HorseRig};
pub use model::{CharacterModel, CharacterModels, CharacterModelsShown};
pub use rider::{RiderMotion, Riding, Saddle, Seated};
pub use rig::{Rig, Talking};
pub use tack::{HorsePiece, Strap, Tack};
pub use townsfolk::TintCache;
pub use viewer::{EYE_IN_HEAD, OWN_HEAD_LAYER, ViewerBody, ViewerHead};
pub use visual::{CharacterVisual, CharacterVisualPlugin, VisualPart, VisualPose};
