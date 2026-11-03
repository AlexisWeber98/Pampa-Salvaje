//! Los vehículos de 1919: el sulky y el carretón, tirados por caballos, la
//! carreta, tirada por una yunta de bueyes, y el Ford T del intendente. Sus
//! modelos están hechos en Blender por script (ver [`models`]).
//!
//! La entidad de un vehículo lleva un [`VehicleVisual`], su [`VisualPose`] (la
//! de sus animales, al frente) y lo que lleva rodado ([`Rolled`]); su
//! `Transform` lleva solo traslación, en el suelo bajo sus animales (bajo el
//! medio del Ford T), como la de un personaje. Debajo cuelga el cuerpo del
//! vehículo ([`VehicleBody`]), girado hacia el rumbo: en su espacio +X es el
//! frente, -Z la izquierda y +Y arriba. De él cuelgan el modelo (con sus
//! ruedas, [`VehicleWheel`], que giran con lo rodado), los animales de tiro
//! (cada caballo un [`CharacterVisual::Draught`] con sus arreos; cada buey un
//! [`VehicleOx`] con el yugo) que andan con la pose del vehículo, y el
//! asiento, con un vecino sentado (un modelo [`Riding`] sobre un asiento sin
//! caballo: ver [`rider`](crate::character::rider)).

pub mod models;

use bevy::app::AnimationSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{Heading, Vehicle};

use crate::character::figure::body_rotation;
use crate::character::model::{CharacterModel, CharacterModels, ModelPending};
use crate::character::rider::{Riding, seat_transform};
use crate::character::{CharacterVisual, Coat, FigureKind, VisualPose};
pub use models::{
    Dressed, ModelLoading, OfVehicle, VehicleMaterials, VehicleModel, VehicleModels, VehicleOx,
    wheel_radius, wheel_turn,
};

/// Cómo se dibuja un vehículo: cuál, quién lo maneja (nadie en un vehículo que
/// espera solo) y el pelaje de cada caballo (el segundo solo lo usa el
/// carretón).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Rolled, Transform, Visibility)]
pub struct VehicleVisual {
    pub vehicle: Vehicle,
    pub driver: Option<FigureKind>,
    pub coats: [Coat; 2],
}

impl VehicleVisual {
    /// El vehículo de un vecino de apariencia `look`, con los pelajes sembrados
    /// a partir de `coat`.
    pub fn seeded(vehicle: Vehicle, look: u8, coat: u32) -> Self {
        Self {
            vehicle,
            driver: Some(FigureKind::townsfolk(look)),
            coats: [Coat::seeded(coat), Coat::seeded(coat.rotate_left(16))],
        }
    }

    /// El mismo vehículo, sin nadie en el asiento.
    pub fn driverless(self) -> Self {
        Self {
            driver: None,
            ..self
        }
    }
}

/// Metros que lleva rodados el vehículo, copiados de la lógica: las ruedas
/// giran con ellos.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct Rolled(pub f32);

/// El cuerpo de un vehículo, hijo de su entidad: girado hacia el rumbo.
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleBody;

/// Una rueda de un vehículo (un nodo `Wheel.*` de su modelo), de `radius`
/// metros.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct VehicleWheel {
    pub radius: f32,
}

/// Un caballo de tiro de un vehículo, hijo de su cuerpo.
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleHorse;

/// El asiento del conductor, hijo del cuerpo del vehículo.
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleSeat;

/// El cuerpo que dibuja el visual actual de un vehículo, en su entidad.
#[derive(Component, Debug, Clone, Copy)]
pub struct VehicleParts {
    pub body: Entity,
}

/// Dónde van los animales y el asiento en el espacio del cuerpo de un
/// vehículo: los marcadores `Animal.*` y `Seat` de su modelo (Blender (x, y,
/// z) es aquí (-y, z, -x)).
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// Dónde va cada animal, sobre el suelo.
    pub animals: &'static [Vec3],
    /// La parte de arriba del asiento, en el medio.
    pub seat: Vec3,
}

/// La yunta de la carreta, a cada lado del pértigo (`tools/vehicle_harness.py`
/// `OX_SPACING` por la escala del buey).
const OX_X: f32 = 0.6255;
const PAIR: [Vec3; 2] = [Vec3::new(0.0, 0.0, -0.62), Vec3::new(0.0, 0.0, 0.62)];
const YOKED: [Vec3; 2] = [Vec3::new(0.0, 0.0, -OX_X), Vec3::new(0.0, 0.0, OX_X)];

impl Layout {
    pub fn of(vehicle: Vehicle) -> Self {
        match vehicle {
            Vehicle::Cart => Self {
                animals: &PAIR,
                seat: Vec3::new(-1.88, 1.74, 0.0),
            },
            Vehicle::Sulky => Self {
                animals: &[Vec3::ZERO],
                seat: Vec3::new(-2.32, 1.40, 0.0),
            },
            Vehicle::Carreta => Self {
                animals: &YOKED,
                seat: Vec3::new(-2.22, 1.42, 0.0),
            },
            Vehicle::FordT => Self {
                animals: &[],
                seat: Vec3::new(-0.05, 1.02, -0.30),
            },
        }
    }
}

/// El giro de la escena de un modelo de vehículo o de animal: de mirar hacia
/// +Z (el glTF) al +X del cuerpo.
fn model_rotation() -> Quat {
    Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
}

/// Dibuja los vehículos.
pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, models::load_vehicle_models)
            .add_systems(
                Update,
                (
                    attach_vehicles,
                    models::swap_in_models,
                    models::ox_graph,
                    models::dress_animals,
                ),
            )
            .add_observer(models::dress_model)
            .add_systems(
                PostUpdate,
                (roll_vehicles, models::walk_oxen)
                    .before(AnimationSystems)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Dibuja los vehículos nuevos y los cambiados, descartando lo que dibujaba el
/// anterior.
fn attach_vehicles(
    mut commands: Commands,
    (models, characters): (Option<Res<VehicleModels>>, Option<Res<CharacterModels>>),
    changed: Query<(Entity, &VehicleVisual, Option<&VehicleParts>), Changed<VehicleVisual>>,
) {
    for (entity, visual, parts) in &changed {
        if let Some(parts) = parts {
            commands.entity(parts.body).try_despawn();
        }
        let layout = Layout::of(visual.vehicle);
        let body = commands
            .spawn((
                Name::new("Vehicle body"),
                VehicleBody,
                Transform::default(),
                Visibility::default(),
                ChildOf(entity),
            ))
            .id();
        let mut model = commands.spawn((
            Name::new("Vehicle model"),
            VehicleModel(visual.vehicle),
            Transform::from_rotation(model_rotation()),
            Visibility::default(),
            ChildOf(body),
        ));
        if let Some(scene) = models.as_ref().and_then(|m| m.scenes.get(&visual.vehicle)) {
            model.insert(ModelLoading(scene.clone()));
        }
        for (k, (&at, &coat)) in layout.animals.iter().zip(&visual.coats).enumerate() {
            if visual.vehicle.oxen() {
                let mut ox = commands.spawn((
                    Name::new("Ox"),
                    VehicleOx { left: k == 0 },
                    OfVehicle(entity),
                    Transform::from_translation(at),
                    Visibility::default(),
                    ChildOf(body),
                ));
                ox.with_children(|ox| {
                    let mut scene = ox.spawn((
                        Name::new("Ox model"),
                        Transform::from_rotation(model_rotation())
                            .with_scale(Vec3::splat(models::OX_SCALE)),
                        Visibility::default(),
                    ));
                    if let Some(m) = models.as_ref().and_then(|m| m.ox.as_ref()) {
                        scene.insert(ModelLoading(m.scene.clone()));
                    }
                });
            } else {
                // Su rumbo es el del cuerpo del vehículo: el propio queda en
                // cero.
                commands.spawn((
                    Name::new("Draught horse"),
                    VehicleHorse,
                    CharacterVisual::Draught(coat),
                    Transform::from_translation(at),
                    ChildOf(body),
                ));
            }
        }
        let seat = commands
            .spawn((
                Name::new("Vehicle seat"),
                VehicleSeat,
                Transform::from_translation(layout.seat),
                Visibility::default(),
                ChildOf(body),
            ))
            .id();
        let driver = visual.driver.and_then(|kind| {
            let scene = characters.as_deref()?.of(kind)?.clone();
            Some((kind, scene))
        });
        if let Some((kind, scene)) = driver {
            commands.spawn((
                Name::new("Driver model"),
                CharacterModel,
                ModelPending(scene),
                Riding {
                    horse: seat,
                    kind,
                    seed: entity.index_u32(),
                },
                Transform::from_matrix(Mat4::from(seat_transform(Vec3::ZERO))),
                // Se dibuja una vez sentado: su escena se crea parada en
                // reposo.
                Visibility::Hidden,
                ChildOf(seat),
            ));
        }
        commands.entity(entity).try_insert(VehicleParts { body });
    }
}

/// Gira el cuerpo de cada vehículo hacia su rumbo, sus ruedas según lo rodado
/// y hace andar a sus caballos con su pose.
fn roll_vehicles(
    vehicles: Query<(&VisualPose, &Rolled, &VehicleParts)>,
    mut bodies: Query<(&mut Transform, &Children), With<VehicleBody>>,
    mut wheels: Query<(&VehicleWheel, &OfVehicle, &mut Transform), Without<VehicleBody>>,
    mut horses: Query<&mut VisualPose, (With<VehicleHorse>, Without<VehicleParts>)>,
) {
    for (wheel, of, mut t) in &mut wheels {
        if let Ok((_, rolled, _)) = vehicles.get(of.0) {
            t.rotation = wheel_turn(rolled.0, wheel.radius);
        }
    }
    for (pose, _, parts) in &vehicles {
        let Ok((mut transform, children)) = bodies.get_mut(parts.body) else {
            continue;
        };
        transform.rotation = body_rotation(pose.heading);
        for &child in children {
            if let Ok(mut horse) = horses.get_mut(child) {
                horse.set_if_neq(VisualPose {
                    heading: Heading::default(),
                    animation: pose.animation,
                    speed: pose.speed,
                    turn: 0.0,
                    flight: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
