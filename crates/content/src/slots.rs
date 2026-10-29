//! Cómo se ajusta en su lugar cada modelo de terceros que ocupa una ranura de
//! modelo: `assets/models/third-party/slots.ron`.
//!
//! Una ranura de modelo es una carpeta `assets/models/third-party/<slot>/` (ver
//! [`pampa_core::StructureKind::SLOTS`]); un `model.glb` o `model.gltf` allí
//! reemplaza la mesh procedural de ese tipo de estructura. Este archivo solo
//! indica cómo escalarlo, moverlo y girarlo, y cuál de sus nodos gira. Una
//! ranura con modelo y sin entrada se dibuja tal como viene.

use std::collections::BTreeMap;
use std::path::Path;

use pampa_core::StructureKind;
use serde::Deserialize;

use crate::ContentError;

/// Dónde están los ajustes de las ranuras, relativo a la carpeta de assets.
pub const SLOTS_FILE: &str = "models/third-party/slots.ron";

/// Los ajustes de las ranuras de modelo, por nombre de ranura.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct ModelSlots {
    pub slots: BTreeMap<String, SlotFit>,
}

/// Cómo se ubica el modelo de una ranura en el espacio propio de su estructura
/// (`x` a lo largo del frente, `y` hacia arriba desde el suelo en el centro,
/// `z` hacia afuera del frente).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SlotFit {
    /// Escala uniforme aplicada al modelo.
    #[serde(default = "one")]
    pub scale: f32,
    /// Desplazamiento después de escalar, en metros.
    #[serde(default)]
    pub offset: (f32, f32, f32),
    /// Giro alrededor de la vertical, en grados (antihorario visto desde
    /// arriba).
    #[serde(default)]
    pub yaw: f32,
    /// El nodo que gira, como la rueda de un molino.
    #[serde(default)]
    pub rotor: Option<SlotRotor>,
}

/// La parte giratoria de un modelo: el nombre de su nodo glTF y el eje, en el
/// espacio propio del nodo, alrededor del cual gira.
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
    /// Interpreta y valida el texto RON de un archivo de ranuras.
    pub fn parse(text: &str) -> Result<Self, ContentError> {
        let slots: Self = ron::from_str(text).map_err(ContentError::Parse)?;
        slots.validate()?;
        Ok(slots)
    }

    /// Comprueba que cada ranura sea conocida, su escala positiva, sus números
    /// finitos y su rotor (si lo hay) tenga nombre y un eje distinto de cero.
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

/// Lee el archivo de ranuras en `path`. Un archivo ausente significa que no hay
/// ajustes (cada modelo tal como viene); uno ilegible o no válido es un error,
/// y quien llama recurre a no usar ajustes.
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
        // Todos los campos salvo el nombre de la ranura tienen un valor por
        // defecto.
        let slots = ModelSlots::parse(r#"(slots: { "well": () })"#).unwrap();
        assert_eq!(slots.slots["well"], SlotFit::default());
        let missing = Path::new("definitely/not/here/slots.ron");
        assert_eq!(load_model_slots(missing).unwrap(), ModelSlots::default());
    }
}
