//! Los huesos del caballo de henrysteve973 (`assets/models/fauna/henry/`,
//! `tools/henry_horse.py`) por el papel que cumplen en el juego: el único
//! lugar que conoce los nombres `BN_*` de su esqueleto. El código pide un
//! papel ([`HorseJoint`]) y nunca escribe un nombre.
//!
//! Las patas van en el orden mano izquierda, mano derecha, pata izquierda,
//! pata derecha (`0..4`), y cada una baja por cinco papeles: [`Upper`],
//! [`Lower`], [`Cannon`], [`Pastern`] y [`Toe`]. El esqueleto no sigue la
//! anatomía hueso por hueso: en la mano, `Clavicle` lleva la paleta y el brazo
//! juntos (de la cruz al codo), `UpperArm` el antebrazo y `Forearm` la caña.
//!
//! [`Upper`]: HorseJoint::Upper
//! [`Lower`]: HorseJoint::Lower
//! [`Cannon`]: HorseJoint::Cannon
//! [`Pastern`]: HorseJoint::Pastern
//! [`Toe`]: HorseJoint::Toe

/// Un hueso del caballo por su papel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorseJoint {
    /// La raíz del esqueleto, padre de todos los demás: el único hueso que
    /// mueve el movimiento de raíz (solo en `Jump_Land`).
    Root,
    /// La grupa: el primer hueso del lomo, del que cuelgan las patas traseras
    /// y la pelvis.
    Croup,
    /// La cruz: el último hueso del lomo, del que cuelgan las manos y el
    /// cuello.
    Withers,
    /// El lomo bajo la montura: el hueso que lleva la piel del asiento, del
    /// que cuelgan los estribos y sobre el que se sienta el jinete.
    Back,
    /// Los huesos del cuello sobre los que se apoyan las riendas, desde la
    /// cabeza (`0`, detrás de la nuca) hacia la cruz (`2`).
    Neck(usize),
    /// La nuca: la articulación de la cabeza con el cuello.
    Poll,
    /// El labio superior, en la punta del hocico.
    UpperLip,
    /// El hueso más alto de cada pata, el que la balancea entera: en la mano,
    /// la paleta y el brazo desde la cruz; en la pata, el muslo desde la
    /// cadera. Su articulación es la cabeza de la pata.
    Upper(usize),
    /// El antebrazo de la mano, desde el codo, o la pierna de la pata, desde
    /// la babilla.
    Lower(usize),
    /// La caña, desde la rodilla de la mano o el corvejón de la pata: la parte
    /// de la pata que se pliega para levantar el vaso.
    Cannon(usize),
    /// La cuartilla, desde el menudillo.
    Pastern(usize),
    /// La punta del vaso de cada pata: el último hueso de la pata, del que
    /// cuelga la herradura.
    Toe(usize),
}

impl HorseJoint {
    /// Las cuatro puntas de los vasos, en el orden de las patas.
    pub const TOES: [Self; 4] = [Self::Toe(0), Self::Toe(1), Self::Toe(2), Self::Toe(3)];

    /// El nombre de la articulación en `horse.glb`.
    pub fn name(self) -> &'static str {
        // Por pata: mano izquierda, mano derecha, pata izquierda, pata
        // derecha.
        let leg = |l: usize, names: [&'static str; 4]| names[l.min(3)];
        match self {
            Self::Root => "BN_Root_01_01",
            Self::Croup => "BN_Spine_00_02_02",
            Self::Withers => "BN_Spine_03_05_05",
            Self::Back => "BN_Spine_01_03_03",
            Self::Neck(k) => [
                "BN_Neck_03_012_012",
                "BN_Neck_02_011_011",
                "BN_Neck_01_07_07",
            ][k.min(2)],
            Self::Poll => "BN_Head_00_016_015",
            Self::UpperLip => "BN_UP_Lip_030_029",
            Self::Upper(l) => leg(
                l,
                [
                    "BN_L_Clavicle_038_039",
                    "BN_R_Clavicle_043_045",
                    "BN_L_Thing_051_054",
                    "BN_R_Thing_056_060",
                ],
            ),
            Self::Lower(l) => leg(
                l,
                [
                    "BN_L_UpperArm_039_040",
                    "BN_R_UpperArm_044_046",
                    "BN_L_Calf_052_055",
                    "BN_R_Calf_057_061",
                ],
            ),
            Self::Cannon(l) => leg(
                l,
                [
                    "BN_l_Forearm_040_041",
                    "BN_R_Forearm_045_047",
                    "BN_L_HorseLink_053_056",
                    "BN_R_HorseLink_058_062",
                ],
            ),
            Self::Pastern(l) => leg(
                l,
                [
                    "BN_L_Hand_041_042",
                    "BN_R_Hand_046_048",
                    "BN_L_Foot_054_057",
                    "BN_R_Foot_00_063",
                ],
            ),
            Self::Toe(l) => leg(
                l,
                [
                    "BN_L_Toe_042_0_044",
                    "BN_R_Toe_047_0_050",
                    "BN_L_Toe_2_055_0_059",
                    "BN_R_Toe_2_059_0_065",
                ],
            ),
        }
    }
}

/// Qué tan alto está el hueso de la punta de un vaso apoyado, en metros del
/// modelo: los cuatro entre 0.093 y 0.108 en el primer cuadro de `Idle_1`, y
/// lo más bajo que llegan en el galope entre 0.077 y 0.094 (el hueso queda
/// dentro del vaso, sobre la herradura).
pub const TOE_STANDING: f32 = 0.09;

/// Lo más bajo que llega el hueso de la punta de un vaso en los clips de
/// zancada, en metros del modelo: las manos de la caminata bajan hasta 0.047
/// en el apoyo (la pinza del vaso entra un poco en el suelo), las del galope
/// hasta 0.077. Más abajo, un vaso lo hundió una mezcla de clips.
pub const TOE_LOWEST: f32 = 0.04;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_names_a_joint_of_the_horse_and_each_leg_hangs_from_its_body() {
        use crate::fauna::livestock::Species;
        use crate::gltf::tests::root;
        use bevy::gltf::gltf;
        let data = std::fs::read(root().0.join(Species::Horse.path())).unwrap();
        let file = gltf::Gltf::from_slice(&data).unwrap();
        let mut parent = std::collections::HashMap::new();
        for node in file.nodes() {
            for c in node.children() {
                parent.insert(
                    c.name().unwrap_or_default(),
                    node.name().unwrap_or_default(),
                );
            }
        }
        let below = |joint: HorseJoint, above: HorseJoint| {
            let mut at = joint.name();
            while let Some(p) = parent.get(at) {
                if *p == above.name() {
                    return true;
                }
                at = p;
            }
            false
        };
        for leg in 0..4 {
            let roles = [
                HorseJoint::Upper(leg),
                HorseJoint::Lower(leg),
                HorseJoint::Cannon(leg),
                HorseJoint::Pastern(leg),
                HorseJoint::Toe(leg),
            ];
            // Cada papel cuelga del anterior, y la pata de su parte del lomo.
            for pair in roles.windows(2) {
                assert!(below(pair[1], pair[0]), "{:?} under {:?}", pair[1], pair[0]);
            }
            let body = if leg < 2 {
                HorseJoint::Withers
            } else {
                HorseJoint::Croup
            };
            assert!(below(roles[0], body), "{:?} under {body:?}", roles[0]);
            // La izquierda y la derecha, cada una de su lado.
            let side = if leg % 2 == 0 { "_L_" } else { "_R_" };
            for role in roles {
                assert!(
                    role.name().to_uppercase().contains(side),
                    "{role:?}: {}",
                    role.name()
                );
            }
        }
        assert!(below(HorseJoint::UpperLip, HorseJoint::Poll));
        assert!(below(HorseJoint::Poll, HorseJoint::Neck(0)));
        assert!(below(HorseJoint::Neck(0), HorseJoint::Neck(1)));
        assert!(below(HorseJoint::Neck(1), HorseJoint::Neck(2)));
        assert!(below(HorseJoint::Neck(2), HorseJoint::Withers));
        assert!(below(HorseJoint::Withers, HorseJoint::Back));
        assert!(below(HorseJoint::Back, HorseJoint::Croup));
        assert!(below(HorseJoint::Croup, HorseJoint::Root));
    }
}
