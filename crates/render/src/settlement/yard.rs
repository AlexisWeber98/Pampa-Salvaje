//! Yard props: the Poly Haven glTF barrels, bucket, crates, ladder and
//! lantern (CC0, see `assets/models/props/polyhaven/CREDITS.md`) set
//! about a settlement's barns and well.
//!
//! Where they go is decided here, from the settlement alone, so it is the
//! same every time: a lantern hung by each barn's big door, a stepladder
//! by one side wall and crates at a back corner of some barns, the barrels
//! (about 33k triangles, so one group at most) behind an estancia's first
//! barn, and a bucket by the well or the trough. A standing prop that would
//! land on a track, on another structure or outside the settlement is left
//! out. They are scenery: no obstacles, drawn within the props' range.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;
use pampa_core::town::PlantKind;
use pampa_core::{Settlement, SettlementKind, Structure, StructureKind, Track, Vec2 as GroundVec2};

use super::buildings::local_size;
use super::yaw;
use crate::AssetRoot;
use crate::gltf;

/// One of the Poly Haven props.
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

    /// Its glTF file, under the [`AssetRoot`].
    pub fn path(self) -> &'static str {
        match self {
            Self::Barrels => "models/props/polyhaven/wooden_barrels_01/wooden_barrels_01_1k.gltf",
            Self::Bucket => "models/props/polyhaven/wooden_bucket_01/wooden_bucket_01_1k.gltf",
            Self::Crate => "models/props/polyhaven/wooden_crate_01/wooden_crate_01_1k.gltf",
            Self::Ladder => "models/props/polyhaven/wooden_ladder/wooden_ladder_1k.gltf",
            Self::Lantern => "models/props/polyhaven/wooden_lantern_01/wooden_lantern_01_1k.gltf",
        }
    }

    /// How far it reaches from its middle on the ground, in meters: what
    /// it keeps clear of tracks and other structures by.
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

/// Where a yard prop goes: its middle on the ground, how far above the
/// ground it sits (stacked or hung), and its turn about the vertical.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YardSpot {
    pub prop: YardProp,
    pub at: GroundVec2,
    pub lift: f32,
    pub turn: Quat,
    /// Hung on a wall, above the ground: nothing to keep clear.
    pub hung: bool,
}

/// The ground point of `local` (in `structure`'s own space) and the turn
/// of its front.
fn on(structure: &Structure, local: Vec3) -> (GroundVec2, Quat) {
    let turn = yaw(structure.front);
    let w = turn * local;
    let c = structure.footprint.center();
    (GroundVec2::new(c.x + w.x, c.y - w.z), turn)
}

/// Every yard prop of `settlement`, in a fixed order.
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
                // Hung beside the big door (as wide as the barn draws it).
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

/// Whether a standing prop at `spot` keeps off every track and every
/// structure but the one it stands by, and inside the settlement.
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

/// A yard shrub or flower bed by a house: what, where, and its turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlantSpot {
    pub kind: PlantKind,
    pub at: GroundVec2,
    pub turn: Quat,
}

/// Every yard plant of `settlement` (30% more yard vegetation, with the
/// extra trees core plants): a shrub by a house's back and side walls and a
/// flower bed by its side, a main house getting more than a peón's. Like the
/// props they are decided from the settlement alone. One that would land on
/// a track, on a structure or the patio, or outside the settlement is left
/// out. They are scenery: no obstacles.
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

/// Whether `spot` keeps off every track, structure and the patio, and
/// stands inside the settlement.
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

/// The yard props' scenes, loading or loaded.
#[derive(Resource, Debug, Clone, Default)]
pub struct YardModels(pub HashMap<YardProp, Handle<WorldAsset>>);

/// Starts loading the yard props. Without an asset server or root, or
/// without the files, there are simply none.
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
