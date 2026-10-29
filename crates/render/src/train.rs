//! El tren de la línea, dibujado a partir del [`LineTrain`] de `pampa-core`:
//! una locomotora a vapor de fabricación británica con su ténder, coches de
//! pasajeros de madera barnizada, vagones cerrados de tablas y tolvas de acero
//! para cereal, que corren sobre los rieles y siguen el suelo y las curvas.
//! Hacia 1919, sobre la trocha ancha del Ferrocarril Oeste.
//!
//! La lógica decide dónde está cada vagón; este módulo solo decide cómo se ve
//! cada [`CarKind`]. El tren cruza la pampa abierta entre pueblos, así que sus
//! vagones no cuelgan de ningún pueblo: un único conjunto de entidades de vagón
//! vive durante todo el juego, y cada vagón se muestra solo mientras está
//! dentro de [`TRAIN_DRAW_RADIUS`] del [`StreamFocus`]. Los vagones ocultos no
//! cuestan draw calls, y no se crea ni se elimina nada cuando el tren va y
//! viene.
//!
//! Cada vagón se construye a partir de [`shape`]s: piezas redondeadas de torno
//! (caldera, chimenea, domos, topes, faroles, cubos de rueda), prismas de
//! perfil (los techos curvos, los costados del ténder, las bielas, los
//! almohadones de los asientos) y cajas, todas con normales suaves y UV en
//! metros. Las formas de un mismo material se fusionan en una malla por vagón;
//! solo las ruedas, las bielas y las bocanadas de humo son entidades propias,
//! porque se mueven. Los materiales son los [`FacadeMaterials`] compartidos de
//! los pueblos: los escaneos de chapa estriada, óxido y tablones bajo un tinte
//! negro, rojo oscuro, granate, barniz u óxido ([`Livery`]), bronce para el
//! latón, hierro galvanizado para los techos.
//!
//! Los coches de pasajeros son huecos, dispuestos por [`CoachBody`] para que la
//! vista desde el asiento mire a través de una ventana real: con paneles y
//! cielorraso de madera barnizada, puertas con filetes, bancos tapizados con
//! respaldo, portaequipajes sobre las ventanas, faroles de aceite, un piso de
//! tablones y ventanillas de guillotina vidriadas.
//!
//! - `kit`: los materiales y las mallas de las ruedas.
//! - `builder`: las piezas fusionadas de un vagón y su bastidor común.
//! - `wheels`: ruedas de rayos y de disco, bogies.
//! - `loco`: la locomotora y el ténder.
//! - `coach`: el coche de pasajeros, por fuera y por dentro.
//! - `freight`: el vagón cerrado y la tolva.

mod builder;
mod coach;
mod freight;
mod kit;
mod loco;
pub mod shape;
mod tank;
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

/// tren, así que un mundo sin línea simplemente está vacío; el índice dentro de
/// ella es la identidad de un tren ([`TrainCar::train`], el `Ride` del juego).
/// Estado lógico: el juego los hace avanzar y render solo los lee.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTrains(pub Vec<LineTrain>);

impl WorldTrains {
    /// El tren de la línea, el primero de ellos.
    pub fn line(&self) -> Option<&LineTrain> {
        self.0.first()
    }

    /// Todos los vagones de todos los trenes, como obstáculos móviles del mundo
    /// lógico.
    pub fn obstacles(&self) -> Vec<SideObstacle> {
        self.0.iter().flat_map(LineTrain::obstacles).collect()
    }
}

/// Los vagones a más de esta distancia del [`StreamFocus`] se ocultan, en
/// metros: el alcance garantizado del terreno, bastante más allá de la niebla.
pub const TRAIN_DRAW_RADIUS: f32 = 400.0;

/// Dibuja en cada frame los vagones del tren de la línea cercanos al jugador.
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

/// Raíz de un vagón dibujado: el +X local apunta en el sentido en que viaja el
/// tren.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainCar {
    /// Índice del tren en [`WorldTrains`].
    pub train: usize,
    /// Índice dentro de la formación, empezando por el frente.
    pub index: usize,
}

/// Una rueda que gira alrededor de su eje Y local (el eje), con su radio.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainWheel {
    /// Índice del tren en [`WorldTrains`].
    pub train: usize,
    pub radius: f32,
}

/// Una biela de acoplamiento o motriz, que giran las manivelas de las ruedas
/// motrices: su lugar en reposo (`base`), el radio de la manivela, el radio de
/// las ruedas motrices, su desfase `phase` y su largo (una biela motriz se
/// inclina al girar; cero para una biela de acoplamiento, que solo se
/// traslada).
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainRod {
    /// Índice del tren en [`WorldTrains`].
    pub train: usize,
    pub base: Vec3,
    pub crank: f32,
    pub radius: f32,
    pub phase: f32,
    pub length: f32,
}

/// Una bocanada de humo de la chimenea, que sube en ciclos mientras el tren se
/// mueve: `phase` en `[0, 1)` desde que sale de la chimenea hasta que se
/// desvanece, y la boca de la chimenea de la que sale, en el marco del vagón.
#[derive(Component, Debug, Clone, Copy)]
pub struct SmokePuff {
    /// Índice del tren en [`WorldTrains`].
    pub train: usize,
    pub phase: f32,
    pub chimney: Vec3,
}

/// Marca las mallas del interior de la carrocería de un coche: paneles, piso,
/// cielorraso, asientos, portaequipajes y faroles.
#[derive(Component, Debug, Clone, Copy)]
pub struct CoachInterior;

/// Segundos reales que tarda una bocanada de humo en subir y desvanecerse.
const PUFF_SECONDS: f32 = 1.6;

/// Radio de las ruedas motrices acopladas de la locomotora, con las que giran
/// las bielas.
const DRIVER_RADIUS: f32 = 0.85;

/// Altura de la cara superior de los rieles sobre el suelo (ver el ferrocarril
/// del pueblo).
const RAIL_TOP: f32 = LineTrain::RAIL_TOP;

/// El kit compartido de los vagones, creado la primera vez que se usa.
#[derive(Resource)]
struct CarKit(Kit);

/// De qué se hace el kit de los vagones.
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
            // Las apps headless no tienen los materiales de los pueblos:
            // entonces se usan colores lisos.
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
                CarKind::TankEngine => b.tank_engine(car.length),
                CarKind::Coach => b.coach(car.length),
                CarKind::Boxcar => b.boxcar(car.length),
                CarKind::Hopper => b.hopper(car.length),
            }
            b.finish();
        }
    }
}

/// Dónde está un vagón: sobre los rieles por encima del suelo bajo sus dos
/// extremos, con el +X local a lo largo de su rumbo (la y del suelo es -Z en
/// Bevy), inclinado según el suelo.
pub fn car_transform(p: &PlacedCar, terrain: &Terrain) -> Transform {
    let half = p.direction * (p.length / 2.0);
    let ground = |q: pampa_core::Vec2| terrain.height(q.x, q.y) + RAIL_TOP;
    let (hb, hf) = (ground(p.center - half), ground(p.center + half));
    let yaw = p.direction.y.atan2(p.direction.x);
    let pitch = (hf - hb).atan2(p.length);
    Transform::from_translation(world_from_ground(p.center, (hb + hf) / 2.0))
        .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_z(pitch))
}

/// Pone cada vagón cercano al foco donde dice la lógica, sobre los rieles por
/// encima del suelo y girado a lo largo de la vía, y oculta los demás.
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

/// Hace rodar las ruedas con la velocidad del tren y gira las bielas junto con
/// las ruedas motrices.
fn turn_wheels(
    time: Res<Time>,
    trains: Res<WorldTrains>,
    mut wheels: Query<(&TrainWheel, &mut Transform), Without<TrainRod>>,
    mut rods: Query<(&mut TrainRod, &mut Transform), Without<TrainWheel>>,
) {
    // Cuánto rodó cada tren en este frame.
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
        // El eje de la rueda es su Y local; rodar hacia adelante la gira en
        // sentido horario vista desde el lado derecho del vagón.
        transform.rotate_local_y(-distance / wheel.radius);
    }
    for (mut rod, mut transform) in &mut rods {
        let distance = rolled.get(rod.train).copied().unwrap_or(0.0);
        if distance == 0.0 {
            continue;
        }
        // También en sentido horario: un muñón de manivela al frente (fase 0 de
        // [`rod_pose`]) baja cuando las ruedas motrices ruedan hacia adelante.
        rod.phase = (rod.phase - distance / rod.radius).rem_euclid(std::f32::consts::TAU);
        *transform = rod_pose(&rod, transform.scale);
    }
}

/// Dónde está una biela según el ángulo de su manivela, contado desde el frente
/// (+X) hacia arriba (+Y): una biela de acoplamiento da vueltas con los muñones
/// de manivela; el extremo trasero de una biela motriz también, pero su extremo
/// delantero se desliza con la cruceta, así que se mueve la mitad y se inclina.
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

/// Humo de la chimenea mientras el tren se mueve: cada bocanada sube, se
/// desplaza hacia atrás y se hincha desde la chimenea, y luego vuelve a
/// empezar; se oculta con el tren detenido.
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
