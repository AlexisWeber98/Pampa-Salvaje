//! Los vehículos tirados por caballos: el carro de los caminos y el sulky de
//! las calles, construidos de forma procedural con la madera de los accesorios
//! de los asentamientos ([`Surface::Planks`], la rueda de rayos de la carreta).
//!
//! La entidad de un vehículo lleva un [`VehicleVisual`], su [`VisualPose`] (la
//! de sus caballos, al frente) y lo que lleva rodado ([`Rolled`]); su
//! `Transform` lleva solo traslación, en el suelo bajo los caballos, como la de
//! un personaje. Debajo cuelga el cuerpo del vehículo ([`VehicleBody`]),
//! girado hacia el rumbo: en su espacio +X es el frente, -Z la izquierda y +Y
//! arriba, desde el suelo bajo los caballos. De él cuelgan la caja, las ruedas
//! ([`VehicleWheel`], que giran con lo rodado), los caballos de tiro (cada uno
//! un [`CharacterVisual::Draught`] que camina con la pose del vehículo) y el
//! pescante, con un vecino sentado (un modelo [`Riding`] sobre un asiento sin
//! caballo: ver [`rider`](crate::character::rider)).
//!
//! - El carro: cuatro ruedas, piso, barandas de listones, tabla delantera, un
//!   banco para el conductor y una lanza central entre dos caballos.
//! - El sulky: dos ruedas altas, un asiento con respaldo, un apoyapiés y dos
//!   varas a los costados de un caballo.

use bevy::app::AnimationSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{Heading, Vehicle};

use crate::character::figure::body_rotation;
use crate::character::model::{CharacterModel, CharacterModels, ModelPending};
use crate::character::rider::{Riding, seat_transform};
use crate::character::{CharacterVisual, Coat, FigureKind, VisualPose};
use crate::facade::shape::Shape;
use crate::facade::{FacadeMaterials, Surface};
use crate::settlement::props::wheel;

/// Cómo se dibuja un vehículo: cuál, quién lo maneja y el pelaje de cada
/// caballo (el segundo solo lo usa el carro).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Rolled, Transform, Visibility)]
pub struct VehicleVisual {
    pub vehicle: Vehicle,
    pub driver: FigureKind,
    pub coats: [Coat; 2],
}

impl VehicleVisual {
    /// El vehículo de un vecino de apariencia `look`, con los pelajes sembrados
    /// a partir de `coat`.
    pub fn seeded(vehicle: Vehicle, look: u8, coat: u32) -> Self {
        Self {
            vehicle,
            driver: FigureKind::townsfolk(look),
            coats: [Coat::seeded(coat), Coat::seeded(coat.rotate_left(16))],
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

/// Una rueda de un vehículo, hija de su cuerpo, de `radius` metros.
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

/// Dónde va cada cosa en el espacio del cuerpo de un vehículo.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// El centro de cada rueda y su radio.
    pub wheels: &'static [(Vec3, f32)],
    /// Dónde va cada caballo, sobre el suelo.
    pub horses: &'static [Vec3],
    /// La parte de arriba del asiento, en el medio.
    pub seat: Vec3,
}

/// Radio de las ruedas del carro, en metros.
const CART_WHEEL: f32 = 0.55;
/// Radio de las ruedas del sulky, en metros.
const SULKY_WHEEL: f32 = 0.7;
/// El piso del carro: de atrás hacia adelante, su altura y su medio ancho.
const CART_BED: (f32, f32, f32, f32) = (-4.6, -2.0, 0.9, 0.7);

/// Las ruedas del carro.
const CART_WHEELS: [(Vec3, f32); 4] = [
    (Vec3::new(-2.55, CART_WHEEL, -0.85), CART_WHEEL),
    (Vec3::new(-2.55, CART_WHEEL, 0.85), CART_WHEEL),
    (Vec3::new(-4.05, CART_WHEEL, -0.85), CART_WHEEL),
    (Vec3::new(-4.05, CART_WHEEL, 0.85), CART_WHEEL),
];
/// La yunta del carro, a cada lado de la lanza.
const CART_HORSES: [Vec3; 2] = [Vec3::new(0.0, 0.0, -0.6), Vec3::new(0.0, 0.0, 0.6)];
/// Las ruedas del sulky.
const SULKY_WHEELS: [(Vec3, f32); 2] = [
    (Vec3::new(-2.0, SULKY_WHEEL, -0.8), SULKY_WHEEL),
    (Vec3::new(-2.0, SULKY_WHEEL, 0.8), SULKY_WHEEL),
];

impl Layout {
    pub fn of(vehicle: Vehicle) -> Self {
        match vehicle {
            Vehicle::Cart => Self {
                wheels: &CART_WHEELS,
                horses: &CART_HORSES,
                seat: Vec3::new(-2.35, 1.38, 0.0),
            },
            Vehicle::Sulky => Self {
                wheels: &SULKY_WHEELS,
                horses: &[Vec3::ZERO],
                seat: Vec3::new(-2.05, 1.03, 0.0),
            },
        }
    }
}

/// La caja del carro, sin las ruedas: piso sobre dos largueros y dos ejes,
/// barandas de estacas y dos listones a los costados y atrás, la tabla
/// delantera, el banco del conductor y la lanza con su balancín.
pub(crate) fn cart_body() -> Shape {
    let mut shape = Shape::default();
    let (x0, x1, floor, half) = CART_BED;
    let wood = Surface::Planks;
    for x in [-2.55, -4.05] {
        shape.beam(
            wood,
            Vec3::new(x, CART_WHEEL, -0.95),
            Vec3::new(x, CART_WHEEL, 0.95),
            0.1,
        );
    }
    for z in [-0.5, 0.5] {
        shape.beam(
            wood,
            Vec3::new(x0, floor - 0.1, z),
            Vec3::new(x1, floor - 0.1, z),
            0.1,
        );
    }
    shape.block(
        wood,
        Vec3::new(x0, floor - 0.04, -half),
        Vec3::new(x1, floor + 0.04, half),
    );
    let top = floor + 0.65;
    // Las barandas de los costados: estacas y dos listones.
    for z in [-half, half] {
        for k in 0..=5 {
            let x = x0 + (x1 - x0) * k as f32 / 5.0;
            shape.beam(wood, Vec3::new(x, floor, z), Vec3::new(x, top, z), 0.06);
        }
        for y in [floor + 0.3, top - 0.05] {
            shape.beam(wood, Vec3::new(x0, y, z), Vec3::new(x1, y, z), 0.05);
        }
    }
    // Atrás, abierta entre dos listones.
    for y in [floor + 0.3, top - 0.05] {
        shape.beam(wood, Vec3::new(x0, y, -half), Vec3::new(x0, y, half), 0.05);
    }
    // La tabla delantera.
    shape.block(
        wood,
        Vec3::new(x1 - 0.05, floor, -half),
        Vec3::new(x1, top + 0.05, half),
    );
    // El banco, sobre dos patas.
    let seat = Layout::of(Vehicle::Cart).seat;
    shape.block(
        wood,
        Vec3::new(seat.x - 0.15, seat.y - 0.06, -half + 0.05),
        Vec3::new(seat.x + 0.15, seat.y, half - 0.05),
    );
    for z in [-0.5, 0.5] {
        shape.beam(
            wood,
            Vec3::new(seat.x, floor, z),
            Vec3::new(seat.x, seat.y - 0.06, z),
            0.07,
        );
    }
    // La lanza, entre los dos caballos, y el balancín detrás de ellos.
    shape.beam(
        wood,
        Vec3::new(x1, floor - 0.1, 0.0),
        Vec3::new(1.2, 1.0, 0.0),
        0.1,
    );
    shape.beam(
        wood,
        Vec3::new(-1.45, 0.82, -0.9),
        Vec3::new(-1.45, 0.82, 0.9),
        0.08,
    );
    shape
}

/// El sulky, sin las ruedas: el eje, dos varas a los costados del caballo con
/// su travesaño, el asiento con respaldo sobre el eje y un apoyapiés.
pub(crate) fn sulky_body() -> Shape {
    let mut shape = Shape::default();
    let wood = Surface::Planks;
    let axle = Vec3::new(-2.0, SULKY_WHEEL, 0.0);
    shape.beam(wood, axle.with_z(-0.9), axle.with_z(0.9), 0.08);
    for z in [-0.48, 0.48] {
        shape.beam(
            wood,
            Vec3::new(-2.45, 0.86, z),
            Vec3::new(1.0, 1.05, z * 0.88),
            0.06,
        );
        // Los soportes del asiento, del eje a las varas.
        shape.beam(wood, axle.with_z(z), Vec3::new(-2.0, 0.88, z), 0.05);
    }
    shape.beam(
        wood,
        Vec3::new(-1.45, 0.89, -0.48),
        Vec3::new(-1.45, 0.89, 0.48),
        0.06,
    );
    let seat = Layout::of(Vehicle::Sulky).seat;
    shape.block(
        wood,
        Vec3::new(seat.x - 0.2, seat.y - 0.05, -0.32),
        Vec3::new(seat.x + 0.2, seat.y, 0.32),
    );
    for z in [-0.3, 0.3] {
        shape.beam(
            wood,
            Vec3::new(seat.x, 0.86, z),
            Vec3::new(seat.x, seat.y - 0.05, z),
            0.05,
        );
        shape.beam(
            wood,
            Vec3::new(seat.x - 0.2, seat.y, z),
            Vec3::new(seat.x - 0.26, seat.y + 0.3, z),
            0.04,
        );
    }
    shape.beam(
        wood,
        Vec3::new(seat.x - 0.26, seat.y + 0.3, -0.32),
        Vec3::new(seat.x - 0.26, seat.y + 0.3, 0.32),
        0.05,
    );
    // El apoyapiés, entre las varas, delante del asiento.
    shape.block(
        wood,
        Vec3::new(-1.7, 0.62, -0.3),
        Vec3::new(-1.4, 0.66, 0.3),
    );
    for z in [-0.3, 0.3] {
        shape.beam(
            wood,
            Vec3::new(-1.55, 0.64, z),
            Vec3::new(-1.55, 0.88, z),
            0.04,
        );
    }
    shape
}

/// Las meshes compartidas, construidas con el primer vehículo.
struct Kit {
    bodies: [Handle<Mesh>; 2],
    wheels: [(f32, Handle<Mesh>); 2],
    /// La madera cuando no hay [`FacadeMaterials`] (tests headless).
    plain: Handle<StandardMaterial>,
}

impl Kit {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut mesh = |shape: Shape| {
            let mesh = shape
                .into_meshes()
                .into_iter()
                .next()
                .map(|(_, m)| m)
                .unwrap_or_else(|| Mesh::from(Cuboid::default()));
            meshes.add(mesh)
        };
        let rolling = |radius: f32| {
            let mut shape = Shape::default();
            wheel(&mut shape, Vec3::ZERO, radius);
            shape
        };
        Self {
            bodies: [mesh(cart_body()), mesh(sulky_body())],
            wheels: [
                (CART_WHEEL, mesh(rolling(CART_WHEEL))),
                (SULKY_WHEEL, mesh(rolling(SULKY_WHEEL))),
            ],
            plain: materials.add(Color::srgb(0.42, 0.31, 0.2)),
        }
    }

    fn body(&self, vehicle: Vehicle) -> Handle<Mesh> {
        match vehicle {
            Vehicle::Cart => self.bodies[0].clone(),
            Vehicle::Sulky => self.bodies[1].clone(),
        }
    }

    fn wheel(&self, radius: f32) -> Handle<Mesh> {
        self.wheels
            .iter()
            .find(|(r, _)| *r == radius)
            .map_or_else(|| self.wheels[0].1.clone(), |(_, m)| m.clone())
    }
}

/// Dibuja los vehículos.
pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach_vehicles).add_systems(
            PostUpdate,
            roll_vehicles
                .before(AnimationSystems)
                .before(TransformSystems::Propagate),
        );
    }
}

/// Los almacenes y recursos con los que se dibuja un vehículo.
type Stores<'a> = (
    ResMut<'a, Assets<Mesh>>,
    ResMut<'a, Assets<StandardMaterial>>,
    Option<Res<'a, FacadeMaterials>>,
    Option<Res<'a, CharacterModels>>,
);

/// Dibuja los vehículos nuevos y los cambiados, descartando lo que dibujaba el
/// anterior.
fn attach_vehicles(
    mut commands: Commands,
    (mut meshes, mut materials, facade, characters): Stores,
    changed: Query<(Entity, &VehicleVisual, Option<&VehicleParts>), Changed<VehicleVisual>>,
    mut kit: Local<Option<Kit>>,
) {
    for (entity, visual, parts) in &changed {
        let kit = kit.get_or_insert_with(|| Kit::new(&mut meshes, &mut materials));
        if let Some(parts) = parts {
            commands.entity(parts.body).try_despawn();
        }
        let wood = facade
            .as_deref()
            .map_or_else(|| kit.plain.clone(), |f| f.get(Surface::Planks));
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
        commands.spawn((
            Mesh3d(kit.body(visual.vehicle)),
            MeshMaterial3d(wood.clone()),
            Transform::default(),
            ChildOf(body),
        ));
        for &(hub, radius) in layout.wheels {
            commands.spawn((
                VehicleWheel { radius },
                Mesh3d(kit.wheel(radius)),
                MeshMaterial3d(wood.clone()),
                Transform::from_translation(hub),
                ChildOf(body),
            ));
        }
        for (&at, &coat) in layout.horses.iter().zip(&visual.coats) {
            // Su rumbo es el del cuerpo del vehículo: el propio queda en cero.
            commands.spawn((
                Name::new("Draught horse"),
                VehicleHorse,
                CharacterVisual::Draught(coat),
                Transform::from_translation(at),
                ChildOf(body),
            ));
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
        let scene = characters
            .as_deref()
            .and_then(|c| c.of(visual.driver))
            .cloned();
        if let Some(scene) = scene {
            commands.spawn((
                Name::new("Driver model"),
                CharacterModel,
                ModelPending(scene),
                Riding {
                    horse: seat,
                    kind: visual.driver,
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

/// Gira el cuerpo de cada vehículo hacia su rumbo, sus ruedas según lo rodado y
/// hace andar a sus caballos con su pose.
fn roll_vehicles(
    vehicles: Query<(&VisualPose, &Rolled, &VehicleParts)>,
    mut bodies: Query<(&mut Transform, &Children), With<VehicleBody>>,
    mut wheels: Query<(&VehicleWheel, &mut Transform), Without<VehicleBody>>,
    mut horses: Query<&mut VisualPose, (With<VehicleHorse>, Without<VehicleParts>)>,
) {
    for (pose, rolled, parts) in &vehicles {
        let Ok((mut transform, children)) = bodies.get_mut(parts.body) else {
            continue;
        };
        transform.rotation = body_rotation(pose.heading);
        for &child in children {
            if let Ok((wheel, mut t)) = wheels.get_mut(child) {
                // Hacia adelante (+X) la parte de arriba de la rueda avanza.
                t.rotation = Quat::from_rotation_z(-rolled.0 / wheel.radius);
            }
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
