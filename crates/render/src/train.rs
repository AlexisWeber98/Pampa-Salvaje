//! Low-poly line train drawn from the `pampa-core` [`LineTrain`]: a steam
//! locomotive with its tender, passenger coaches, boxcars and open grain
//! hoppers, riding the rails and following the ground and the curves.
//!
//! Logic owns where every car is; this module only decides how each
//! [`CarKind`] looks. The train crosses the open pampa between towns, so
//! its cars hang off no town: one set of car entities lives for the whole
//! game, and each car is shown only while it is within
//! [`TRAIN_DRAW_RADIUS`] of the [`StreamFocus`]. Hidden cars cost no draw
//! calls, and nothing is spawned or despawned as the train comes and goes.
//!
//! The passenger coaches are hollow: thin wooden walls with real window
//! openings glazed with clear panes, a planked floor, a ceiling with oil
//! lamps, end doors, and wooden benches facing each other in bays, laid out
//! by [`CoachBody`] so the seat view looks out of an actual window.

use bevy::prelude::*;
use pampa_core::{CarKind, CoachBody, LineTrain, PlacedCar, Terrain};

use crate::coords::world_from_ground;
use crate::terrain::{StreamFocus, WorldTerrain};

/// The line train, if the world has a line. Logic state: the game
/// advances it, render only reads it.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTrain(pub Option<LineTrain>);

/// Cars farther than this from the [`StreamFocus`] are hidden, in meters:
/// the terrain's guaranteed reach, well past the fog.
pub const TRAIN_DRAW_RADIUS: f32 = 400.0;

/// Draws the line train's cars near the player every frame.
pub struct TrainPlugin;

impl Plugin for TrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTrain>()
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
    /// Index into the consist, front first.
    pub index: usize,
}

/// A wheel turning about its local Y axis (the axle), with its radius.
#[derive(Component, Debug, Clone, Copy)]
pub struct TrainWheel {
    pub radius: f32,
}

/// Height of the rail tops above the ground (see the town's railway).
const RAIL_TOP: f32 = LineTrain::RAIL_TOP;

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
    coach: Handle<StandardMaterial>,
    trim: Handle<StandardMaterial>,
    glass: Handle<StandardMaterial>,
    /// Varnished wood inside the coaches: panelling and benches.
    wood: Handle<StandardMaterial>,
    /// Worn floor planks.
    planks: Handle<StandardMaterial>,
    /// An oil lamp's glowing glass.
    lamp: Handle<StandardMaterial>,
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
            coach: paint(0.20, 0.30, 0.22, 0.1, 0.8),
            trim: paint(0.80, 0.72, 0.55, 0.0, 0.8),
            wood: paint(0.42, 0.27, 0.15, 0.0, 0.6),
            planks: paint(0.33, 0.25, 0.18, 0.0, 0.95),
            glass: materials.add(StandardMaterial {
                base_color: Color::srgba(0.70, 0.78, 0.80, 0.18),
                perceptual_roughness: 0.1,
                reflectance: 0.6,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            lamp: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.85, 0.55),
                emissive: LinearRgba::rgb(3.0, 2.2, 1.0),
                ..default()
            }),
        }
    }
}

/// Spawns child meshes of one car, in its local frame (origin on the rails
/// at the car's middle, +X forward, +Z to the car's right).
struct CarBuilder<'a, 'c> {
    car: EntityCommands<'c>,
    kit: &'a Kit,
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
                TrainWheel { radius },
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

    fn coach(&mut self, length: f32) {
        let k = self.kit;
        let roof = k.roof.clone();
        self.frame(length, 1.1);
        for x in [
            -length / 2.0 + 1.3,
            -length / 2.0 + 2.5,
            length / 2.0 - 2.5,
            length / 2.0 - 1.3,
        ] {
            self.axle(x, 0.45);
        }
        self.coach_shell(length);
        self.coach_inside(length);
        // A rounded-looking roof over the ceiling.
        self.cube(
            &roof,
            Vec3::new(0.0, 3.9, 0.0),
            Vec3::new(length - 0.4, 0.2, 3.0),
        );
        self.cube(
            &roof,
            Vec3::new(0.0, 4.08, 0.0),
            Vec3::new(length - 1.2, 0.16, 2.2),
        );
    }

    /// The coach's hollow body: floor, side walls with glazed window
    /// openings ([`CoachBody::windows`]) and a cream belt line, end walls
    /// with a door, and the ceiling.
    fn coach_shell(&mut self, length: f32) {
        let k = self.kit;
        let (body, trim, glass, black, planks) = (
            k.coach.clone(),
            k.trim.clone(),
            k.glass.clone(),
            k.black.clone(),
            k.planks.clone(),
        );
        let (floor, ceiling, wall) = (CoachBody::FLOOR, CoachBody::CEILING, CoachBody::WALL);
        let (sill, lintel) = (CoachBody::SILL, CoachBody::LINTEL);
        let inner = length - 2.0 * CoachBody::END_INSET;
        let half = CoachBody::HALF_WIDTH;
        let width = 2.0 * half;
        let band = |lo: f32, hi: f32| ((lo + hi) / 2.0, hi - lo);
        self.cube(
            &planks,
            Vec3::new(0.0, floor - 0.05, 0.0),
            Vec3::new(inner, 0.1, width),
        );
        self.cube(
            &body,
            Vec3::new(0.0, ceiling + 0.04, 0.0),
            Vec3::new(inner, 0.08, width),
        );
        let (windows, open) = CoachBody::windows(length);
        // Piers between the openings, from one end wall to the other.
        let mut edges = vec![-inner / 2.0];
        for &x in &windows {
            edges.extend([x - open / 2.0, x + open / 2.0]);
        }
        edges.push(inner / 2.0);
        for side in [-1.0, 1.0] {
            let z = side * (half - wall / 2.0);
            for (lo, hi) in [(floor, sill), (lintel, ceiling)] {
                let (y, h) = band(lo, hi);
                self.cube(&body, Vec3::new(0.0, y, z), Vec3::new(inner, h, wall));
            }
            let (y, h) = band(sill, lintel);
            for pier in edges.chunks(2) {
                let (x, w) = band(pier[0], pier[1]);
                self.cube(&body, Vec3::new(x, y, z), Vec3::new(w, h, wall));
            }
            for &x in &windows {
                self.cube(&glass, Vec3::new(x, y, z), Vec3::new(open, h, 0.01));
            }
            self.cube(
                &trim,
                Vec3::new(0.0, sill - 0.2, side * (half + 0.01)),
                Vec3::new(inner + 0.02, 0.18, 0.02),
            );
        }
        // End walls, each with a door in the middle.
        let door = 0.9;
        for end in [-1.0, 1.0] {
            let x = end * (inner / 2.0 - wall / 2.0);
            let w = half - door / 2.0;
            for side in [-1.0, 1.0] {
                let (y, h) = band(floor, ceiling);
                self.cube(
                    &body,
                    Vec3::new(x, y, side * (half - w / 2.0)),
                    Vec3::new(wall, h, w),
                );
            }
            let (y, h) = band(floor + 2.0, ceiling);
            self.cube(&body, Vec3::new(x, y, 0.0), Vec3::new(wall, h, door));
            let (y, h) = band(floor, floor + 2.0);
            self.cube(&black, Vec3::new(x, y, 0.0), Vec3::new(wall / 2.0, h, door));
        }
    }

    /// Inside a second-class coach: wooden benches facing each other in a
    /// bay at each window, both sides of the aisle, and oil lamps under the
    /// ceiling.
    fn coach_inside(&mut self, length: f32) {
        let k = self.kit;
        let (wood, lamp) = (k.wood.clone(), k.lamp.clone());
        let (windows, _) = CoachBody::windows(length);
        let pitch = (length - 3.0) / windows.len() as f32;
        let floor = CoachBody::FLOOR;
        let reach = CoachBody::HALF_WIDTH - CoachBody::WALL;
        for &x in &windows {
            for facing in [-1.0, 1.0] {
                // The bench at this end of the bay faces its middle.
                let back = x - facing * (pitch / 2.0 - 0.1);
                let seat = back + facing * 0.25;
                for side in [-1.0, 1.0] {
                    let z = side * (reach - 0.5);
                    self.cube(
                        &wood,
                        Vec3::new(seat, floor + 0.45, z),
                        Vec3::new(0.45, 0.08, 1.0),
                    );
                    self.cube(
                        &wood,
                        Vec3::new(back, floor + 0.75, z),
                        Vec3::new(0.08, 0.7, 1.0),
                    );
                }
            }
        }
        for x in [-length / 4.0, 0.0, length / 4.0] {
            self.cube(
                &lamp,
                Vec3::new(x, CoachBody::CEILING - 0.15, 0.0),
                Vec3::new(0.18, 0.22, 0.18),
            );
        }
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
    train: Res<WorldTrain>,
    spawned: Query<(), With<TrainCar>>,
    kit: Option<Res<CarKit>>,
    mut assets: (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>),
) {
    let Some(train) = &train.0 else {
        return;
    };
    if !spawned.is_empty() {
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
    for (index, car) in train.cars().iter().enumerate() {
        let entity = commands.spawn((
            Name::new(format!("Train car {index}")),
            TrainCar { index },
            Transform::default(),
            Visibility::Hidden,
        ));
        let mut b = CarBuilder {
            car: entity,
            kit: &kit,
        };
        match car.kind {
            CarKind::Locomotive => b.locomotive(car.length),
            CarKind::Coach => b.coach(car.length),
            CarKind::Boxcar => b.boxcar(car.length),
            CarKind::Hopper => b.hopper(car.length),
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
    train: Res<WorldTrain>,
    terrain: Res<WorldTerrain>,
    focus: Option<Res<StreamFocus>>,
    mut cars: Query<(&TrainCar, &mut Transform, &mut Visibility)>,
) {
    let Some(train) = &train.0 else {
        return;
    };
    let placed = train.placed_cars();
    for (car, mut transform, mut visibility) in &mut cars {
        let Some(p) = placed.get(car.index) else {
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

/// Rolls the wheels with the train's speed.
fn turn_wheels(
    time: Res<Time>,
    train: Res<WorldTrain>,
    mut wheels: Query<(&TrainWheel, &mut Transform)>,
) {
    let Some(train) = &train.0 else {
        return;
    };
    let distance = train.state().speed * time.delta_secs();
    if distance == 0.0 {
        return;
    }
    for (wheel, mut transform) in &mut wheels {
        // The wheel's axle is its local Y; rolling forward turns it clockwise
        // seen from the car's right side.
        transform.rotate_local_y(-distance / wheel.radius);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pampa_core::{
        BASE_RATE, PlazaPlacement, RailLine, RailwayConfig, Side, Terrain, Timetable, TownConfig,
        TownLayout, Vec2 as GroundVec2, WorldConfig, place_towns,
    };

    /// Castex, Monte Nievas and Pico as `assets/world/towns.ron` lays them
    /// out, and the line train through them.
    pub(crate) fn line_train() -> (Vec<TownLayout>, LineTrain) {
        let world = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let town = |name: &str, x: f32, y: f32, blocks| TownConfig {
            name: name.into(),
            origin: GroundVec2::new(x, y),
            blocks,
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
        };
        let (towns, errors) = place_towns(
            &world,
            &[
                town("Monte Nievas", -3050.0, -1950.0, (4, 3)),
                town("Eduardo Castex", -5150.0, -2850.0, (5, 4)),
                town("General Pico", 2917.0, 1783.0, (8, 8)),
            ],
        );
        assert!(errors.is_empty(), "{errors:?}");
        let names: Vec<String> = ["Eduardo Castex", "Monte Nievas", "General Pico"]
            .map(String::from)
            .to_vec();
        let (line, _) = RailLine::build(Some(&names), &towns, world.bounds());
        let train = LineTrain::new(&line, &towns, Timetable::default()).expect("a train");
        (towns, train)
    }

    fn app_with_train(game_seconds: f64, focus: GroundVec2) -> App {
        let (_, mut train) = line_train();
        train.advance(0.0, game_seconds, BASE_RATE);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTerrain(Terrain::default()))
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldTrain(Some(train)))
            .add_plugins(TrainPlugin);
        app
    }

    fn cars(app: &mut App) -> Vec<(usize, Transform, Visibility)> {
        let mut query = app
            .world_mut()
            .query::<(&TrainCar, &Transform, &Visibility)>();
        let mut out: Vec<_> = query
            .iter(app.world())
            .map(|(c, t, v)| (c.index, *t, *v))
            .collect();
        out.sort_by_key(|c| c.0);
        out
    }

    #[test]
    fn cars_near_the_focus_follow_logic_along_the_curves() {
        // Two minutes out of Monte Nievas, on the way to Pico.
        let (_, probe) = line_train();
        let (_, depart) = probe.stop_times(pampa_core::Run::Down, 1).unwrap();
        let at = 7.0 * 3600.0 + (depart + 120.0) * BASE_RATE;
        let mut app = app_with_train(at, GroundVec2::ZERO);
        let placed = app
            .world()
            .resource::<WorldTrain>()
            .0
            .clone()
            .unwrap()
            .placed_cars();
        app.insert_resource(StreamFocus(placed[0].center));
        app.update();
        app.update();
        let drawn = cars(&mut app);
        assert_eq!(drawn.len(), placed.len());
        assert_eq!(
            drawn.len(),
            8,
            "the locomotive, two coaches and five freight cars"
        );
        for ((_, t, visibility), p) in drawn.iter().zip(&placed) {
            assert_eq!(*visibility, Visibility::Inherited);
            let at = t.translation;
            assert!((at.x - p.center.x).abs() < 1e-3 && (at.z + p.center.y).abs() < 1e-3);
            // Local +X points along the car's ground heading.
            let forward = t.rotation * Vec3::X;
            let flat = GroundVec2::new(forward.x, -forward.z);
            let flat = flat * (1.0 / flat.length());
            assert!(
                flat.dot(p.direction) > 0.9999,
                "{flat:?} vs {:?}",
                p.direction
            );
        }
    }

    #[test]
    fn each_car_is_drawn_as_tall_as_core_says() {
        let mut app = app_with_train(3600.0, GroundVec2::ZERO);
        app.update();
        let train = app.world().resource::<WorldTrain>().0.clone().unwrap();
        let mut query = app.world_mut().query::<(&TrainCar, &Children)>();
        let cars: Vec<(usize, Vec<Entity>)> = query
            .iter(app.world())
            .map(|(c, children)| (c.index, children.iter().collect()))
            .collect();
        assert_eq!(cars.len(), 8);
        for (index, children) in cars {
            // Every part is a unit mesh scaled into place: its top is the
            // highest corner of the transformed unit cube.
            let top = children
                .iter()
                .map(|&e| *app.world().get::<Transform>(e).unwrap())
                .flat_map(|t| {
                    (0..8).map(move |k| {
                        let corner = Vec3::new(
                            if k & 1 == 0 { -0.5 } else { 0.5 },
                            if k & 2 == 0 { -0.5 } else { 0.5 },
                            if k & 4 == 0 { -0.5 } else { 0.5 },
                        );
                        t.transform_point(corner).y
                    })
                })
                .fold(f32::MIN, f32::max);
            let kind = train.cars()[index].kind;
            assert!(
                (top - kind.height()).abs() < 0.02,
                "{kind:?}: drawn {top}, core {}",
                kind.height()
            );
        }
    }

    /// Every child box of car `index`: material and world-free local box
    /// (min, max) of its unit mesh scaled into place.
    fn boxes(app: &mut App, index: usize) -> Vec<(Handle<StandardMaterial>, Vec3, Vec3)> {
        let mut q = app.world_mut().query::<(&TrainCar, &Children)>();
        let children: Vec<Entity> = q
            .iter(app.world())
            .find(|(c, _)| c.index == index)
            .map(|(_, ch)| ch.iter().collect())
            .unwrap();
        children
            .into_iter()
            .filter_map(|e| {
                let t = *app.world().get::<Transform>(e)?;
                let m = app
                    .world()
                    .get::<MeshMaterial3d<StandardMaterial>>(e)?
                    .0
                    .clone();
                let a = t.transform_point(Vec3::splat(-0.5));
                let b = t.transform_point(Vec3::splat(0.5));
                Some((m, a.min(b), a.max(b)))
            })
            .collect()
    }

    #[test]
    fn coaches_are_hollow_with_open_glazed_windows_and_benches_inside() {
        let mut app = app_with_train(3600.0, GroundVec2::ZERO);
        app.update();
        let train = app.world().resource::<WorldTrain>().0.clone().unwrap();
        let index = train
            .cars()
            .iter()
            .position(|c| c.kind == CarKind::Coach)
            .unwrap();
        let length = train.cars()[index].length;
        let kit = app.world().resource::<CarKit>().0.clone();
        let parts = boxes(&mut app, index);
        let (windows, open) = CoachBody::windows(length);
        assert!(windows.len() >= 4);
        let mid_y = (CoachBody::SILL + CoachBody::LINTEL) / 2.0;
        for &x in &windows {
            for side in [-1.0f32, 1.0] {
                let z = side * (CoachBody::HALF_WIDTH - CoachBody::WALL / 2.0);
                let p = Vec3::new(x, mid_y, z);
                let inside = |(_, a, b): &&(Handle<StandardMaterial>, Vec3, Vec3)| {
                    p.cmpge(*a - 1e-4).all() && p.cmple(*b + 1e-4).all()
                };
                let hits: Vec<_> = parts.iter().filter(inside).collect();
                // Only the clear pane fills the opening.
                assert_eq!(hits.len(), 1, "window at {x}, side {side}");
                assert_eq!(hits[0].0, kit.glass);
            }
        }
        // Wall beside every window, below the sill.
        let below = Vec3::new(
            windows[0],
            CoachBody::SILL - 0.3,
            CoachBody::HALF_WIDTH - 0.04,
        );
        assert!(
            parts
                .iter()
                .any(|(m, a, b)| *m == kit.coach && below.cmpge(*a).all() && below.cmple(*b).all())
        );
        // Benches: a seat and a back at each end of each bay, both sides.
        let wood = parts.iter().filter(|(m, _, _)| *m == kit.wood).count();
        assert_eq!(wood, windows.len() * 2 * 2 * 2);
        // The window seat is clear of every part and beside an opening.
        let (along, across, eye) = CoachBody::window_seat(length, 1.0);
        let seat = Vec3::new(along, eye, across);
        assert!(
            !parts
                .iter()
                .any(|(m, a, b)| *m != kit.glass && seat.cmpge(*a).all() && seat.cmple(*b).all())
        );
        assert!(windows.iter().any(|&x| (along - x).abs() < open / 2.0));
        assert!(eye > CoachBody::SILL && eye < CoachBody::LINTEL);
    }

    #[test]
    fn cars_far_from_the_focus_are_hidden() {
        // Overnight at Pico, with the focus at Castex.
        let mut app = app_with_train(3600.0, GroundVec2::new(-5150.0, -2850.0));
        app.update();
        app.update();
        let drawn = cars(&mut app);
        assert_eq!(drawn.len(), 8);
        assert!(drawn.iter().all(|c| c.2 == Visibility::Hidden));
        // No train, no cars.
        let mut empty = App::new();
        empty
            .add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTerrain(Terrain::default()))
            .add_plugins(TrainPlugin);
        empty.update();
        assert!(cars(&mut empty).is_empty());
    }
}
