//! How each third-party model that fills a model slot is fitted in place:
//! `assets/models/third-party/slots.ron`.
//!
//! A model slot is a folder `assets/models/third-party/<slot>/` (see
//! [`pampa_core::StructureKind::SLOTS`]); a `model.glb` or `model.gltf`
//! there replaces the procedural mesh of that structure kind. This file
//! only says how to scale, move and turn it, and which of its nodes spins.
//! A slot with a model and no entry is drawn as it comes.

use std::collections::BTreeMap;
use std::path::Path;

use pampa_core::StructureKind;
use serde::Deserialize;

use crate::ContentError;

/// Where the slot fits live, relative to the assets folder.
pub const SLOTS_FILE: &str = "models/third-party/slots.ron";

/// The fits of the model slots, by slot name.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct ModelSlots {
    pub slots: BTreeMap<String, SlotFit>,
}

/// How a slot's model sits in its structure's own space (`x` along the
/// front, `y` up from the ground at the middle, `z` out of the front).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SlotFit {
    /// Uniform scale applied to the model.
    #[serde(default = "one")]
    pub scale: f32,
    /// Offset after scaling, in meters.
    #[serde(default)]
    pub offset: (f32, f32, f32),
    /// Turn about the vertical, in degrees (counter-clockwise from above).
    #[serde(default)]
    pub yaw: f32,
    /// The node that spins, like a windpump's wheel.
    #[serde(default)]
    pub rotor: Option<SlotRotor>,
}

/// A model's spinning part: its glTF node name and the axis, in the
/// node's own space, it turns about.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SlotRotor {
    pub node: String,
    pub axis: (f32, f32, f32),
}

fn one() -> f32 {
    1.0
}

impl Default for SlotFit {
    fn default() -> Self {
        Self {
            scale: 1.0,
            offset: (0.0, 0.0, 0.0),
            yaw: 0.0,
            rotor: None,
        }
    }
}

impl ModelSlots {
    /// Parses and validates the RON text of a slots file.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let slots: Self = ron::from_str(text).map_err(ContentError::Parse)?;
        slots.validate()?;
        Ok(slots)
    }

    /// Checks every slot is known, its scale positive, its numbers finite
    /// and its rotor (if any) named with a non-zero axis.
    pub fn validate(&self) -> Result<(), ContentError> {
        for (slot, fit) in &self.slots {
            let bad = |reason| {
                Err(ContentError::Slot {
                    slot: slot.clone(),
                    reason,
                })
            };
            if !StructureKind::SLOTS.contains(&slot.as_str()) {
                return bad("no structure kind has this slot");
            }
            let (x, y, z) = fit.offset;
            if !(fit.scale.is_finite() && fit.scale > 0.0) {
                return bad("scale must be a positive number");
            }
            if ![x, y, z, fit.yaw].iter().all(|v| v.is_finite()) {
                return bad("offset and yaw must be finite");
            }
            if let Some(rotor) = &fit.rotor {
                let (x, y, z) = rotor.axis;
                let length = (x * x + y * y + z * z).sqrt();
                if rotor.node.trim().is_empty() {
                    return bad("the rotor needs a node name");
                }
                if !(length.is_finite() && length > 1e-6) {
                    return bad("the rotor axis must be a non-zero vector");
                }
            }
        }
        Ok(())
    }
}

/// Reads the slots file at `path`. A missing file means no fits (every
/// model as it comes); an unreadable or invalid one is an error, and the
/// caller falls back to no fits.
pub fn load_model_slots(path: &Path) -> Result<ModelSlots, ContentError> {
    if !path.exists() {
        return Ok(ModelSlots::default());
    }
    let text = std::fs::read_to_string(path).map_err(|source| ContentError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    ModelSlots::parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_path;

    const SHIPPED: &str = include_str!("../../../assets/models/third-party/slots.ron");

    #[test]
    fn the_shipped_slots_parse_and_name_known_slots() {
        let slots = ModelSlots::parse(SHIPPED).expect("shipped slots.ron is valid");
        let pump = &slots.slots["windpump"];
        assert!(pump.scale > 0.5 && pump.scale < 1.5, "{pump:?}");
        assert_eq!(pump.rotor.as_ref().map(|r| r.node.as_str()), Some("rotor"));
        assert_eq!(load_model_slots(&asset_path(SLOTS_FILE)).unwrap(), slots);
    }

    #[test]
    fn bad_slots_are_refused_and_a_missing_file_fits_nothing() {
        let refused = |text: &str| ModelSlots::parse(text).unwrap_err();
        assert!(matches!(
            refused(r#"(slots: { "carreta": () })"#),
            ContentError::Slot { .. }
        ));
        assert!(matches!(
            refused(r#"(slots: { "cart": (scale: 0.0) })"#),
            ContentError::Slot { .. }
        ));
        assert!(matches!(
            refused(r#"(slots: { "windpump": (rotor: Some((node: "", axis: (0.0, 0.0, 1.0)))) })"#),
            ContentError::Slot { .. }
        ));
        assert!(matches!(
            refused(
                r#"(slots: { "windpump": (rotor: Some((node: "r", axis: (0.0, 0.0, 0.0)))) })"#
            ),
            ContentError::Slot { .. }
        ));
        assert!(matches!(refused("(slots: 3)"), ContentError::Parse(_)));
        // Every field but the slot name has a default.
        let slots = ModelSlots::parse(r#"(slots: { "well": () })"#).unwrap();
        assert_eq!(slots.slots["well"], SlotFit::default());
        let missing = Path::new("definitely/not/here/slots.ron");
        assert_eq!(load_model_slots(missing).unwrap(), ModelSlots::default());
    }
}
