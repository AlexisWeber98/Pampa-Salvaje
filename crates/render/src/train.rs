//! The line train drawn from the `pampa-core` [`LineTrain`]: a British-built
//! steam locomotive with its tender, varnished wooden passenger coaches,
//! boarded boxcars and steel grain hoppers, riding the rails and following
//! the ground and the curves. About 1919, on the broad gauge of the
//! Ferrocarril Oeste.
//!
//! Logic owns where every car is; this module only decides how each
//! [`CarKind`] looks. The train crosses the open pampa between towns, so
//! its cars hang off no town: one set of car entities lives for the whole
//! game, and each car is shown only while it is within
//! [`TRAIN_DRAW_RADIUS`] of the [`StreamFocus`]. Hidden cars cost no draw
//! calls, and nothing is spawned or despawned as the train comes and goes.
//!
//! Every car is built from [`shape`]s: rounded lathe turnings (boiler,
//! chimney, domes, buffers, lamps, wheel hubs), profile prisms (the curved
//! roofs, the tender's flanks, rods, seat cushions) and boxes, all with
//! smooth normals and UVs in meters. The shapes of one material are merged
//! into one mesh per car; only the wheels, the rods and the smoke puffs are
//! entities of their own, as they move. The materials are the shared
//! [`FacadeMaterials`] of the towns: the tread plate, rust and planks scans
//! under a black, dark red, maroon, varnish or oxide tint ([`Livery`]),
//! bronze for the brass, galvanised iron for the roofs.
//!
//! The passenger coaches are hollow, laid out by [`CoachBody`] so the seat
//! view looks out of an actual window: panelled and ceilinged in varnished
//! wood, with lined doors, upholstered benches with backs, luggage racks
//! over the windows, oil lamps, a planked floor and glazed droplights.
//!
//! - `kit`: the materials and the wheel meshes.
//! - `builder`: a car's merged parts and its common underframe.
//! - `wheels`: spoked and plate wheels, bogies.
//! - `loco`: the locomotive and tender.
//! - `coach`: the passenger coach, outside and in.
//! - `freight`: the boxcar and the hopper.

mod builder;
mod coach;
mod freight;
mod kit;
mod loco;
pub mod shape;
mod wheels;

#[cfg(test)]
pub(crate) mod tests;

use bevy::prelude::*;
use pampa_core::{CarKind, LineTrain, PlacedCar, SideObstacle, Terrain};

use crate::coords::world_from_ground;
use crate::facade::FacadeMaterials;
use crate::terrain::{StreamFocus, WorldTerrain};
use builder::CarBuilder;
use kit::Kit;

/// train, so a world with no line is simply empty; the index into it is a
/// train's identity ([`TrainCar::train`], the game's `Ride`). Logic state:
/// the game advances them, render only reads them.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTrains(pub Vec<LineTrain>);

impl WorldTrains {
    /// The line train, the first of them.
    pub fn line(&self) -> Option<&LineTrain> {
        self.0.first()
    }

    /// Every car of every train, as the logic world's moving obstacles.
    pub fn obstacles(&self) -> Vec<SideObstacle> {
        self.0.iter().flat_map(LineTrain::obstacles).collect()
    }
}

/// Cars farther than this from the [`StreamFocus`] are hidden, in meters:
/// the terrain's guaranteed reach, well past the fog.
pub const TRAIN_DRAW_RADIUS: f32 = 400.0;

/// Draws the line train's cars near the player every frame.
pub struct TrainPlugin;

impl Plugin for TrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTrains>()
            .add_systems(Update, spawn_cars)
            .add_systems(
                PostUpdate,
                (place_cars, turn_wheels, puff_smoke)
                    .before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

/// Root of one drawn car: local +X points the way the train travels.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainCar {
    /// Index of the train in [`WorldTrains`].
    pub train: usize,
    /// Index into the consist, front first.
    pub index: usize,
}

/// A wheel turning about its local Y axis (the axle), with its radius.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainWheel {
    /// Index of the train in [`WorldTrains`].
    pub train: usize,
    pub radius: f32,
}

/// A side or main rod, swung round by the drivers' cranks: its place at
/// rest (`base`), the crank's throw, the drivers' radius, its quartering
/// `phase`, and how long it is (a main rod tilts as it swings; zero for a
/// side rod, which only translates).
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainRod {
    /// Index of the train in [`WorldTrains`].
    pub train: usize,
    pub base: Vec3,
    pub crank: f32,
    pub radius: f32,
    pub phase: f32,
    pub length: f32,
}

/// One puff of chimney smoke, cycling upward while the train moves:
/// `phase` in `[0, 1)` from leaving the chimney to fading out, and the
/// chimney's mouth it leaves from, in the car's frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct SmokePuff {
    /// Index of the train in [`WorldTrains`].
    pub train: usize,
    pub phase: f32,
    pub chimney: Vec3,
}

/// Marks the meshes inside a coach's shell: panelling, floor, ceiling,
/// seats, racks and lamps.
#[derive(Component, Debug, Clone, Copy)]
pub struct CoachInterior;

/// Real seconds a smoke puff takes to rise and fade.
const PUFF_SECONDS: f32 = 1.6;

/// Radius of the locomotive's coupled drivers, which the rods turn with.
const DRIVER_RADIUS: f32 = 0.85;

/// Height of the rail tops above the ground (see the town's railway).
const RAIL_TOP: f32 = LineTrain::RAIL_TOP;

/// The shared car kit, made on first use.
#[derive(Resource)]
struct CarKit(Kit);

/// What the car kit is made from.
type KitAssets<'w> = (
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<StandardMaterial>>,
    Option<Res<'w, FacadeMaterials>>,
);

fn spawn_cars(
    mut commands: Commands,
    trains: Res<WorldTrains>,
    spawned: Query<(), With<TrainCar>>,
    kit: Option<Res<CarKit>>,
    mut assets: KitAssets,
) {
    if trains.0.is_empty() || !spawned.is_empty() {
        return;
    }
    let kit = match kit {
        Some(kit) => kit.0.clone(),
        None => {
            let (meshes, materials, surfaces) = &mut assets;
            // Headless apps have no towns' materials: plain colours then.
            let surfaces = surfaces
                .as_deref()
                .cloned()
                .unwrap_or_else(|| FacadeMaterials::plain(materials));
            let kit = Kit::new(meshes, materials, surfaces);
            commands.insert_resource(CarKit(kit.clone()));
            kit
        }
    };
    for (train_index, train) in trains.0.iter().enumerate() {
        for (index, car) in train.cars().iter().enumerate() {
            let entity = commands.spawn((
                Name::new(format!("Train {train_index} car {index}")),
                TrainCar {
                    train: train_index,
                    index,
                },
                Transform::default(),
                Visibility::Hidden,
            ));
            let mut b = CarBuilder::new(entity, train_index, &kit, &mut assets.0);
            match car.kind {
                CarKind::Locomotive => b.locomotive(car.length),
                CarKind::Coach => b.coach(car.length),
                CarKind::Boxcar => b.boxcar(car.length),
                CarKind::Hopper => b.hopper(car.length),
            }
            b.finish();
        }
    }
}

/// Where a car stands: on the rails over the ground under its two ends,
/// local +X along its heading (ground y is Bevy -Z), pitched with the
/// ground.
pub fn car_transform(p: &PlacedCar, terrain: &Terrain) -> Transform {
    let half = p.direction * (p.length / 2.0);
    let ground = |q: pampa_core::Vec2| terrain.height(q.x, q.y) + RAIL_TOP;
    let (hb, hf) = (ground(p.center - half), ground(p.center + half));
    let yaw = p.direction.y.atan2(p.direction.x);
    let pitch = (hf - hb).atan2(p.length);
    Transform::from_translation(world_from_ground(p.center, (hb + hf) / 2.0))
        .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(pitch))
}

/// Puts every car near the focus where logic says, on the rails over the
/// ground and turned along the track, and hides the others.
fn place_cars(
    trains: Res<WorldTrains>,
    terrain: Res<WorldTerrain>,
    focus: Option<Res<StreamFocus>>,
    mut cars: Query<(&TrainCar, &mut Transform, &mut Visibility)>,
) {
    let placed: Vec<_> = trains.0.iter().map(LineTrain::placed_cars).collect();
    for (car, mut transform, mut visibility) in &mut cars {
        let Some(p) = placed.get(car.train).and_then(|cars| cars.get(car.index)) else {
            continue;
        };
        let near = focus
            .as_ref()
            .is_none_or(|f| f.0.distance(p.center) <= TRAIN_DRAW_RADIUS);
        *visibility = if near {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if !near {
            continue;
        }
        let placed = car_transform(p, &terrain.0);
        transform.translation = placed.translation;
        transform.rotation = placed.rotation;
    }
}

/// Rolls the wheels with the train's speed, and swings the rods round
/// with the drivers.
fn turn_wheels(
    time: Res<Time>,
    trains: Res<WorldTrains>,
    mut wheels: Query<(&TrainWheel, &mut Transform), Without<TrainRod>>,
    mut rods: Query<(&mut TrainRod, &mut Transform), Without<TrainWheel>>,
) {
    // How far each train rolled this frame.
    let rolled: Vec<f32> = trains
        .0
        .iter()
        .map(|t| t.state().speed * time.delta_secs())
        .collect();
    for (wheel, mut transform) in &mut wheels {
        let distance = rolled.get(wheel.train).copied().unwrap_or(0.0);
        if distance == 0.0 {
            continue;
        }
        // The wheel's axle is its local Y; rolling forward turns it clockwise
        // seen from the car's right side.
        transform.rotate_local_y(-distance / wheel.radius);
    }
    for (mut rod, mut transform) in &mut rods {
        let distance = rolled.get(rod.train).copied().unwrap_or(0.0);
        if distance == 0.0 {
            continue;
        }
        // Clockwise too: a crank pin at the front ([`rod_pose`]'s phase 0)
        // goes down as the drivers roll forward.
        rod.phase = (rod.phase - distance / rod.radius).rem_euclid(std::f32::consts::TAU);
        *transform = rod_pose(&rod, transform.scale);
    }
}

/// Where a rod is at its crank angle, counted from the front (+X) toward
/// the top (+Y): a side rod circles with the crank
/// pins; a main rod's back end does, its front end slides with the
/// crosshead, so it moves half as far and tilts.
pub fn rod_pose(rod: &TrainRod, scale: Vec3) -> Transform {
    let (sin, cos) = rod.phase.sin_cos();
    let (along, up) = (cos * rod.crank, sin * rod.crank);
    let (offset, tilt) = if rod.length > 0.0 {
        (
            Vec3::new(along, up / 2.0, 0.0),
            (up / rod.length).clamp(-1.0, 1.0).asin(),
        )
    } else {
        (Vec3::new(along, up, 0.0), 0.0)
    };
    Transform::from_translation(rod.base + offset)
        .with_rotation(Quat::from_rotation_z(-tilt))
        .with_scale(scale)
}

/// Chimney smoke while the train moves: each puff rises, drifts back and
/// swells from the chimney, then starts again; hidden at a standstill.
fn puff_smoke(
    time: Res<Time>,
    trains: Res<WorldTrains>,
    mut puffs: Query<(&mut SmokePuff, &mut Transform, &mut Visibility)>,
) {
    for (mut puff, mut transform, mut visibility) in &mut puffs {
        let Some(train) = trains.0.get(puff.train) else {
            continue;
        };
        let speed = train.state().speed;
        if speed <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        puff.phase = (puff.phase + time.delta_secs() / PUFF_SECONDS).fract();
        let t = puff.phase;
        transform.translation =
            puff.chimney + Vec3::new(-t * (1.0 + speed * 0.25), 0.3 + t * 2.0, 0.0);
        transform.scale = Vec3::splat(0.5 + t * 1.6);
        visibility.set_if_neq(Visibility::Inherited);
    }
}
