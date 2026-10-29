//! Accesorios de patio: los barriles, el balde, los cajones, la escalera y el
//! farol en glTF de Poly Haven (CC0, ver
//! `assets/models/props/polyhaven/CREDITS.md`) distribuidos alrededor de los
//! galpones y el pozo de un asentamiento.
//!
//! Dónde van se decide aquí, solo a partir del asentamiento, así que siempre es
//! igual: un farol colgado junto al portón grande de cada galpón, una escalera
//! de tijera junto a una pared lateral y cajones en una esquina trasera de
//! algunos galpones, los barriles (unos 33k triángulos, así que como mucho un
//! grupo) detrás del primer galpón de una estancia, y un balde junto al pozo o
//! al bebedero. Un accesorio de pie que caería sobre una huella, sobre otra
//! estructura o fuera del asentamiento se deja afuera. Son decorado: no son
//! obstáculos, y se dibujan dentro del alcance de los accesorios.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;
use pampa_core::town::PlantKind;
use pampa_core::{Settlement, SettlementKind, Structure, StructureKind, Track, Vec2 as GroundVec2};

use super::buildings::local_size;
use super::yaw;
use crate::AssetRoot;
use crate::gltf;

/// Uno de los accesorios de Poly Haven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum YardProp {
    Barrels,
    Bucket,
    Crate,
    Ladder,
    Lantern,
}

impl YardProp {
    pub const ALL: [Self; 5] = [
        Self::Barrels,
        Self::Bucket,
        Self::Crate,
        Self::Ladder,
        Self::Lantern,
    ];

    /// Su archivo glTF, bajo el [`AssetRoot`].
    pub fn path(self) -> &'static str {
        match self {
            Self::Barrels => "models/props/polyhaven/wooden_barrels_01/wooden_barrels_01_1k.gltf",
            Self::Bucket => "models/props/polyhaven/wooden_bucket_01/wooden_bucket_01_1k.gltf",
            Self::Crate => "models/props/polyhaven/wooden_crate_01/wooden_crate_01_1k.gltf",
            Self::Ladder => "models/props/polyhaven/wooden_ladder/wooden_ladder_1k.gltf",
            Self::Lantern => "models/props/polyhaven/wooden_lantern_01/wooden_lantern_01_1k.gltf",
        }
    }

    /// Cuánto se extiende desde su centro sobre el suelo, en metros: la
    /// distancia que mantiene libre de huellas y de otras estructuras.
    pub fn reach(self) -> f32 {
        match self {
            Self::Barrels => 2.6,
            Self::Bucket => 0.25,
            Self::Crate => 0.5,
            Self::Ladder => 0.6,
            Self::Lantern => 0.15,
        }
    }
}

/// Dónde va un accesorio de patio: su centro sobre el suelo, a qué altura sobre
/// el suelo está (apilado o colgado) y su giro alrededor de la vertical.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YardSpot {
    pub prop: YardProp,
    pub at: GroundVec2,
    pub lift: f32,
    pub turn: Quat,
    /// Colgado en una pared, por encima del suelo: no hay nada que mantener
    /// libre.
    pub hung: bool,
}

/// El punto en el suelo de `local` (en el espacio propio de `structure`) y el
/// giro de su frente.
fn on(structure: &Structure, local: Vec3) -> (GroundVec2, Quat) {
    let turn = yaw(structure.front);
    let w = turn * local;
    let c = structure.footprint.center();
    (GroundVec2::new(c.x + w.x, c.y - w.z), turn)
}

/// Todos los accesorios de patio de `settlement`, en un orden fijo.
pub fn yard_props(settlement: &Settlement) -> Vec<YardSpot> {
    let mut spots = Vec::new();
    let mut barrels = settlement.kind == SettlementKind::Ranch;
    let mut push = |structure: &Structure, prop: YardProp, local: Vec3, quarter: bool| {
        let (at, turn) = on(structure, local.with_y(0.0));
        let turn = if quarter {
            turn * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
        } else {
            turn
        };
        spots.push(YardSpot {
            prop,
            at,
            lift: local.y,
            turn,
            hung: prop == YardProp::Lantern,
        });
    };
    for structure in &settlement.structures {
        let size = local_size(structure);
        let (hx, hz) = (size.x / 2.0, size.y / 2.0);
        let v = structure.variant;
        match structure.kind {
            StructureKind::Barn => {
                // Colgado junto al portón grande (tan ancho como lo dibuja el
                // galpón).
                let door = (size.x * 0.3).clamp(4.0, 6.0) / 2.0;
                let side = if v % 2 == 0 { 1.0 } else { -1.0 };
                push(
                    structure,
                    YardProp::Lantern,
                    Vec3::new(side * (door + 0.45), 2.3, hz + 0.12),
                    false,
                );
                if v % 3 != 1 {
                    push(
                        structure,
                        YardProp::Ladder,
                        Vec3::new(side * (hx + 0.7), 0.0, -hz / 3.0),
                        false,
                    );
                }
                if v % 4 != 3 {
                    let corner = Vec3::new(-side * (hx - 0.8), 0.0, -hz - 0.6);
                    push(structure, YardProp::Crate, corner, false);
                    if v % 5 < 2 {
                        push(structure, YardProp::Crate, corner.with_y(0.34), true);
                    }
                }
                if barrels {
                    barrels = false;
                    push(
                        structure,
                        YardProp::Barrels,
                        Vec3::new(side * hx / 3.0, 0.0, -hz - 2.9),
                        false,
                    );
                }
            }
            StructureKind::Well => {
                push(
                    structure,
                    YardProp::Bucket,
                    Vec3::new(hx + 0.35, 0.0, 0.3),
                    false,
                );
            }
            StructureKind::Trough if v % 2 == 0 => {
                push(
                    structure,
                    YardProp::Bucket,
                    Vec3::new(hx + 0.4, 0.0, 0.0),
                    false,
                );
            }
            _ => {}
        }
    }
    spots.retain(|spot| spot.hung || clear(settlement, spot));
    spots
}

/// Si un accesorio de pie en `spot` se mantiene lejos de toda huella y de toda
/// estructura salvo aquella junto a la que está, y dentro del asentamiento.
fn clear(settlement: &Settlement, spot: &YardSpot) -> bool {
    let reach = spot.prop.reach();
    let off_tracks = settlement
        .tracks
        .iter()
        .flat_map(Track::segments)
        .all(|(a, b)| {
            let ab = b - a;
            let t = ((spot.at - a).dot(ab) / ab.dot(ab).max(1e-6)).clamp(0.0, 1.0);
            spot.at.distance(a + ab * t) > Track::HALF_WIDTH + reach
        });
    let off_others = settlement
        .structures
        .iter()
        .all(|s| !s.footprint.expanded(reach).contains_point(spot.at));
    off_tracks
        && off_others
        && !settlement.patio.contains_point(spot.at)
        && settlement.bounds.contains_point(spot.at)
}

/// Un arbusto o un cantero de patio junto a una casa: qué es, dónde va y su
/// giro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlantSpot {
    pub kind: PlantKind,
    pub at: GroundVec2,
    pub turn: Quat,
}

/// Todas las plantas de patio de `settlement` (30% más de vegetación de patio,
/// con los árboles extra que planta core): un arbusto junto a las paredes
/// trasera y laterales de una casa y un cantero a su costado, y la casa
/// principal recibe más que la de un peón. Igual que los accesorios, se deciden
/// solo a partir del asentamiento. Una que caería sobre una huella, sobre una
/// estructura o el patio, o fuera del asentamiento, se deja afuera. Son
/// decorado: no son obstáculos.
pub fn yard_plants(settlement: &Settlement) -> Vec<PlantSpot> {
    let mut spots = Vec::new();
    for structure in settlement.structures.iter().filter(|s| s.kind.is_house()) {
        let size = local_size(structure);
        let (hx, hz) = (size.x / 2.0, size.y / 2.0);
        let v = structure.variant;
        let main = matches!(structure.kind, StructureKind::MainHouse(_));
        let side = if v % 2 == 0 { 1.0 } else { -1.0 };
        let mut push = |kind: PlantKind, local: Vec3| {
            let (at, _) = on(structure, local);
            let turn =
                Quat::from_rotation_y((v.wrapping_add(local.x.to_bits()) % 360) as f32 / 57.3);
            spots.push(PlantSpot { kind, at, turn });
        };
        if main || v % 3 != 0 {
            push(
                PlantKind::Shrub,
                Vec3::new(side * (hx + 0.9), 0.0, -hz / 3.0),
            );
        }
        if main {
            push(
                PlantKind::Shrub,
                Vec3::new(-side * hx * 0.5, 0.0, -hz - 0.9),
            );
        }
        if v % 10 < if main { 7 } else { 3 } {
            push(
                PlantKind::Flowers,
                Vec3::new(-side * (hx + 1.1), 0.0, hz * 0.5),
            );
        }
    }
    spots.retain(|spot| plant_clear(settlement, spot));
    spots
}

/// Si `spot` se mantiene lejos de toda huella, estructura y del patio, y queda
/// dentro del asentamiento.
fn plant_clear(settlement: &Settlement, spot: &PlantSpot) -> bool {
    let reach = spot.kind.radius();
    let off_tracks = settlement
        .tracks
        .iter()
        .flat_map(Track::segments)
        .all(|(a, b)| {
            let ab = b - a;
            let t = ((spot.at - a).dot(ab) / ab.dot(ab).max(1e-6)).clamp(0.0, 1.0);
            spot.at.distance(a + ab * t) > Track::HALF_WIDTH + reach
        });
    let off_structures = settlement
        .structures
        .iter()
        .all(|s| !s.footprint.expanded(reach).contains_point(spot.at));
    off_tracks
        && off_structures
        && !settlement.patio.expanded(reach).contains_point(spot.at)
        && settlement.bounds.contains_point(spot.at)
}

/// Las escenas de los accesorios de patio, cargando o ya cargadas.
#[derive(Resource, Debug, Clone, Default)]
pub struct YardModels(pub HashMap<YardProp, Handle<WorldAsset>>);

/// Empieza a cargar los accesorios de patio. Sin un servidor de assets o una
/// raíz, o sin los archivos, simplemente no hay ninguno.
pub(super) fn load_yard_models(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
) {
    let mut models = YardModels::default();
    if let (Some(server), Some(root)) = (server, root) {
        for prop in YardProp::ALL {
            if root.0.join(prop.path()).is_file()
                && let Some(scene) = gltf::load_scene(&server, Some(&root), prop.path())
            {
                models.0.insert(prop, scene);
            }
        }
    }
    commands.insert_resource(models);
}
