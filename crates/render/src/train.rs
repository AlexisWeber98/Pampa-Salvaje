//! Low-poly freight train drawn from a `pampa-core` [`Train`]: a steam
//! locomotive with its tender, boxcars and open grain hoppers, riding the
//! rails and following the ground.
//!
//! Logic owns where every car is; this module only decides how each
//! [`CarKind`] looks.

use bevy::prelude::*;
use pampa_core::{CarKind, PlacedCar, Train};

use crate::coords::world_from_ground;
use crate::terrain::WorldTerrain;
use crate::town::TownRoot;

/// Each town's train, by town index (`None` for a town without a
/// railway). Logic state: the game advances them, render only reads them.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTrains(pub Vec<Option<Train>>);

/// Draws the train of every built town (see [`crate::TownPlugin`]) every
/// frame; its cars hang off the town's [`TownRoot`] and go with it.
pub struct TrainPlugin;

impl Plugin for TrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTrains>()
            .add_systems(Update, spawn_cars)
            .add_systems(
                PostUpdate,
                (place_cars, turn_wheels).before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

/// Root of one drawn car: local +X points the way the train travels.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainCar {
    /// The town whose train this car belongs to.
    pub town: usize,
    /// Index into the consist, front first.
    pub index: usize,
}

/// A wheel turning about its local Y axis (the axle), with its radius.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainWheel {
    /// The town whose train this wheel belongs to.
    pub town: usize,
    pub radius: f32,
}

/// Height of the rail tops above the ground (see the town's railway).
const RAIL_TOP: f32 = 0.5;

#[derive(Clone)]
struct Kit {
    cube: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    hopper: Handle<Mesh>,
    black: Handle<StandardMaterial>,
    smokebox: Handle<StandardMaterial>,
    red: Handle<StandardMaterial>,
    brass: Handle<StandardMaterial>,
    boxcar: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
    hopper_body: Handle<StandardMaterial>,
    grain: Handle<StandardMaterial>,
    coal: Handle<StandardMaterial>,
    window: Handle<StandardMaterial>,
}

impl Kit {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut paint = |r: f32, g: f32, b: f32, metallic: f32, roughness: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                metallic,
                perceptual_roughness: roughness,
                ..default()
            })
        };
        Self {
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(12)),
            // Four-sided funnel: the hopper's discharge chute.
            hopper: meshes.add(Cone::new(0.71, 1.0).mesh().resolution(4)),
            black: paint(0.11, 0.10, 0.10, 0.6, 0.5),
            smokebox: paint(0.17, 0.16, 0.15, 0.3, 0.8),
            red: paint(0.50, 0.14, 0.09, 0.1, 0.7),
            brass: paint(0.62, 0.50, 0.26, 0.8, 0.4),
            boxcar: paint(0.45, 0.24, 0.16, 0.0, 0.9),
            roof: paint(0.40, 0.38, 0.36, 0.4, 0.7),
            hopper_body: paint(0.47, 0.42, 0.34, 0.3, 0.8),
            grain: paint(0.66, 0.54, 0.32, 0.0, 1.0),
            coal: paint(0.07, 0.07, 0.07, 0.0, 1.0),
            window: paint(0.13, 0.11, 0.10, 0.0, 0.9),
        }
    }
}

/// Spawns child meshes of one car, in its local frame (origin on the rails
/// at the car's middle, +X forward, +Z to the car's right).
struct CarBuilder<'a, 'c> {
    car: EntityCommands<'c>,
    kit: &'a Kit,
    town: usize,
}

impl CarBuilder<'_, '_> {
    fn part(&mut self, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, t: Transform) {
        self.car
            .with_child((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), t));
    }

    fn cube(&mut self, material: &Handle<StandardMaterial>, center: Vec3, size: Vec3) {
        let mesh = self.kit.cube.clone();
        self.part(
            &mesh,
            material,
            Transform::from_translation(center).with_scale(size),
        );
    }

    /// Cylinder lying along the car (boiler, smokebox).
    fn lying(
        &mut self,
        material: &Handle<StandardMaterial>,
        center: Vec3,
        radius: f32,
        length: f32,
    ) {
        let mesh = self.kit.cylinder.clone();
        self.part(
            &mesh,
            material,
            Transform::from_translation(center)
                .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::new(2.0 * radius, length, 2.0 * radius)),
        );
    }

    /// Upright cylinder standing on `base` (chimney, dome).
    fn upright(
        &mut self,
        material: &Handle<StandardMaterial>,
        base: Vec3,
        radius: f32,
        height: f32,
    ) {
        let mesh = self.kit.cylinder.clone();
        self.part(
            &mesh,
            material,
            Transform::from_translation(base + Vec3::Y * height / 2.0).with_scale(Vec3::new(
                2.0 * radius,
                height,
                2.0 * radius,
            )),
        );
    }

    /// A pair of wheels on one axle at `x` along the car.
    fn axle(&mut self, x: f32, radius: f32) {
        let mesh = self.kit.cylinder.clone();
        let material = self.kit.black.clone();
        for side in [-1.0, 1.0] {
            self.car.with_child((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                TrainWheel {
                    town: self.town,
                    radius,
                },
                Transform::from_xyz(x, radius, side * 0.95)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(2.0 * radius, 0.18, 2.0 * radius)),
            ));
        }
    }

    /// Underframe and buffers shared by every car.
    fn frame(&mut self, length: f32, deck: f32) {
        let (black, red) = (self.kit.black.clone(), self.kit.red.clone());
        self.cube(
            &black,
            Vec3::new(0.0, deck, 0.0),
            Vec3::new(length - 0.6, 0.35, 2.5),
        );
        for end in [-1.0, 1.0] {
            self.cube(
                &red,
                Vec3::new(end * (length / 2.0 - 0.25), deck, 0.0),
                Vec3::new(0.3, 0.45, 2.7),
            );
        }
    }

    fn locomotive(&mut self, length: f32) {
        let k = self.kit;
        let (black, smokebox, red, brass) = (
            k.black.clone(),
            k.smokebox.clone(),
            k.red.clone(),
            k.brass.clone(),
        );
        let front = length / 2.0;
        self.frame(length, 1.1);
        // Three coupled drivers, a leading truck and the tender's axles.
        for x in [1.2, 2.9, 4.6] {
            self.axle(x, 0.75);
        }
        self.axle(front - 1.0, 0.45);
        for x in [-front + 1.2, -front + 3.0] {
            self.axle(x, 0.45);
        }
        // Boiler, smokebox, chimney with its flared cap, steam and sand domes.
        self.lying(&black, Vec3::new(2.6, 2.3, 0.0), 0.95, 6.2);
        self.lying(&smokebox, Vec3::new(front - 1.3, 2.3, 0.0), 1.0, 1.2);
        self.upright(&smokebox, Vec3::new(front - 1.4, 3.1, 0.0), 0.3, 1.3);
        self.upright(&black, Vec3::new(front - 1.4, 4.3, 0.0), 0.45, 0.25);
        self.upright(&brass, Vec3::new(3.0, 3.1, 0.0), 0.38, 0.6);
        self.upright(&black, Vec3::new(1.4, 3.1, 0.0), 0.3, 0.45);
        // Red running boards and a buffer beam with a pilot under it.
        for side in [-1.0, 1.0] {
            self.cube(
                &red,
                Vec3::new(2.6, 1.45, side * 1.2),
                Vec3::new(7.0, 0.1, 0.45),
            );
        }
        self.cube(
            &red,
            Vec3::new(front - 0.15, 0.6, 0.0),
            Vec3::new(0.3, 0.5, 2.4),
        );
        // Cab with side windows and an overhanging roof.
        let cab_x = -1.4;
        self.cube(
            &black,
            Vec3::new(cab_x, 2.55, 0.0),
            Vec3::new(2.6, 2.6, 2.8),
        );
        let window = k.window.clone();
        for side in [-1.0, 1.0] {
            self.cube(
                &window,
                Vec3::new(cab_x + 0.3, 3.0, side * 1.41),
                Vec3::new(0.9, 0.8, 0.02),
            );
        }
        self.cube(
            &smokebox,
            Vec3::new(cab_x, 3.95, 0.0),
            Vec3::new(3.0, 0.15, 3.1),
        );
        // Tender heaped with coal.
        let tender_x = -front + 2.2;
        self.cube(
            &black,
            Vec3::new(tender_x, 2.1, 0.0),
            Vec3::new(4.2, 1.9, 2.7),
        );
        let coal = k.coal.clone();
        self.cube(
            &coal,
            Vec3::new(tender_x + 0.4, 3.1, 0.0),
            Vec3::new(2.8, 0.35, 2.3),
        );
    }

    fn boxcar(&mut self, length: f32) {
        let k = self.kit;
        let (body, roof, window) = (k.boxcar.clone(), k.roof.clone(), k.window.clone());
        self.frame(length, 1.1);
        for x in [
            -length / 2.0 + 1.3,
            -length / 2.0 + 2.5,
            length / 2.0 - 2.5,
            length / 2.0 - 1.3,
        ] {
            self.axle(x, 0.45);
        }
        self.cube(
            &body,
            Vec3::new(0.0, 2.6, 0.0),
            Vec3::new(length - 0.7, 2.6, 2.8),
        );
        self.cube(
            &roof,
            Vec3::new(0.0, 3.98, 0.0),
            Vec3::new(length - 0.5, 0.16, 3.0),
        );
        // Sliding doors on both sides.
        for side in [-1.0, 1.0] {
            self.cube(
                &window,
                Vec3::new(0.0, 2.45, side * 1.41),
                Vec3::new(1.8, 2.1, 0.03),
            );
        }
    }

    fn hopper(&mut self, length: f32) {
        let k = self.kit;
        let (body, grain, black) = (k.hopper_body.clone(), k.grain.clone(), k.black.clone());
        self.frame(length, 1.1);
        for x in [
            -length / 2.0 + 1.3,
            -length / 2.0 + 2.5,
            length / 2.0 - 2.5,
            length / 2.0 - 1.3,
        ] {
            self.axle(x, 0.45);
        }
        // Open bin with ribs, heaped with grain, over two discharge chutes.
        self.cube(
            &body,
            Vec3::new(0.0, 2.6, 0.0),
            Vec3::new(length - 0.8, 1.8, 2.8),
        );
        self.cube(
            &grain,
            Vec3::new(0.0, 3.5, 0.0),
            Vec3::new(length - 1.2, 0.12, 2.5),
        );
        for x in [-0.3, 0.0, 0.3] {
            self.cube(
                &black,
                Vec3::new(x * length, 2.6, 0.0),
                Vec3::new(0.15, 1.85, 2.9),
            );
        }
        let chute = k.hopper.clone();
        for x in [-length / 4.0, length / 4.0] {
            self.part(
                &chute,
                &body,
                Transform::from_xyz(x, 1.2, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::PI))
                    .with_scale(Vec3::new(2.4, 1.0, 2.4)),
            );
        }
    }
}

/// The shared car kit, made on first use.
#[derive(Resource)]
struct CarKit(Kit);

fn spawn_cars(
    mut commands: Commands,
    trains: Res<WorldTrains>,
    roots: Query<(Entity, &TownRoot), Added<TownRoot>>,
    kit: Option<Res<CarKit>>,
    mut assets: (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
) {
    let with_train = |town: &TownRoot| matches!(trains.0.get(town.0), Some(Some(_)));
    if !roots.iter().any(|(_, town)| with_train(town)) {
        return;
    }
    let kit = match kit {
        Some(kit) => kit.0.clone(),
        None => {
            let kit = Kit::new(&mut assets.0, &mut assets.1);
            commands.insert_resource(CarKit(kit.clone()));
            kit
        }
    };
    for (root, town) in &roots {
        let Some(Some(train)) = trains.0.get(town.0) else {
            continue;
        };
        for (index, car) in train.cars.iter().enumerate() {
            let entity = commands.spawn((
                Name::new(format!("Train car {index}")),
                TrainCar {
                    town: town.0,
                    index,
                },
                Transform::default(),
                Visibility::Hidden,
                ChildOf(root),
            ));
            let mut b = CarBuilder {
                car: entity,
                kit: &kit,
                town: town.0,
            };
            match car.kind {
                CarKind::Locomotive => b.locomotive(car.length),
                CarKind::Boxcar => b.boxcar(car.length),
                CarKind::Hopper => b.hopper(car.length),
            }
        }
    }
}

/// Puts every car where logic says, on the rails over the ground, and hides
/// the cars that are off the railway.
fn place_cars(
    trains: Res<WorldTrains>,
    terrain: Res<WorldTerrain>,
    mut cars: Query<(&TrainCar, &mut Transform, &mut Visibility)>,
) {
    let placed: Vec<Vec<PlacedCar>> = trains
        .0
        .iter()
        .map(|t| t.as_ref().map(Train::placed_cars).unwrap_or_default())
        .collect();
    for (car, mut transform, mut visibility) in &mut cars {
        let Some(p) = placed.get(car.town).and_then(|cars| cars.get(car.index)) else {
            continue;
        };
        *visibility = if p.on_rail {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        // Pitch with the ground under the two ends of the car.
        let half = p.direction * (p.length / 2.0);
        let ground = |q: pampa_core::Vec2| terrain.0.height(q.x, q.y) + RAIL_TOP;
        let (back, front) = (p.center - half, p.center + half);
        let (wb, wf) = (
            world_from_ground(back, ground(back)),
            world_from_ground(front, ground(front)),
        );
        let forward = (wf - wb).normalize_or(Vec3::X);
        transform.translation = (wb + wf) / 2.0;
        transform.rotation = Quat::from_rotation_arc(Vec3::X, forward);
    }
}

/// Rolls the wheels with their train's speed.
fn turn_wheels(
    time: Res<Time>,
    trains: Res<WorldTrains>,
    mut wheels: Query<(&TrainWheel, &mut Transform)>,
) {
    for (wheel, mut transform) in &mut wheels {
        let Some(Some(train)) = trains.0.get(wheel.town) else {
            continue;
        };
        let distance = train.state().speed * time.delta_secs();
        // The wheel's axle is its local Y; rolling forward turns it clockwise
        // seen from the car's right side.
        transform.rotate_local_y(-distance / wheel.radius);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{Terrain, TownLayout, TrainSchedule};

    fn app_with_train(time: f32) -> App {
        let town = TownLayout::castex_inspired();
        let mut train = Train::for_town(&town, TrainSchedule::default()).expect("a railway");
        train.step(time);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTerrain(Terrain::default()))
            .insert_resource(crate::WorldTowns(vec![town]))
            .insert_resource(WorldTrains(vec![Some(train)]))
            .add_plugins((crate::TownPlugin, TrainPlugin));
        app
    }

    fn cars(app: &mut App) -> Vec<(usize, Vec3, Visibility)> {
        let mut query = app
            .world_mut()
            .query::<(&TrainCar, &Transform, &Visibility)>();
        let mut out: Vec<_> = query
            .iter(app.world())
            .map(|(c, t, v)| (c.index, t.translation, *v))
            .collect();
        out.sort_by_key(|c| c.0);
        out
    }

    #[test]
    fn cars_follow_logic_and_hide_off_the_railway() {
        // Three seconds in: the locomotive is on the patch, the tail is not yet.
        let mut app = app_with_train(3.0);
        app.update();
        let train = app.world().resource::<WorldTrains>().0[0].clone().unwrap();
        let placed = train.placed_cars();
        let drawn = cars(&mut app);
        assert_eq!(drawn.len(), placed.len());
        for ((_, at, visibility), p) in drawn.iter().zip(&placed) {
            assert!((at.x - p.center.x).abs() < 1e-3 && (at.z + p.center.y).abs() < 1e-3);
            let expected = if p.on_rail {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            assert_eq!(*visibility, expected);
        }
        assert!(drawn.iter().any(|c| c.2 == Visibility::Inherited));
        assert!(
            drawn.iter().any(|c| c.2 == Visibility::Hidden),
            "tail still off the patch"
        );
    }
}
