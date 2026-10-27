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
//!
//! The detail is roughly PS2-era and of a British-built Argentine line of
//! about 1919, from shared unit meshes and a few shared materials (with the
//! PBR finishes of [`Finish`]): a banded boiler, smokebox door, headlamp,
//! cylinders, side and main rods that turn with the drivers, a cowcatcher,
//! a cab with spectacle windows and a coaled tender, and a few smoke puffs
//! while moving; boarded coach sides with window frames, a clerestory roof
//! and end platforms with railings; boarded, iron-strapped freight cars; and
//! bogies, buffers and couplers under every car.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{CarKind, CoachBody, LineTrain, PlacedCar, SideObstacle, Terrain};

use crate::character::finish::Finish;

use crate::coords::world_from_ground;
use crate::terrain::{StreamFocus, WorldTerrain};

/// Every train of the world: the line train first, then the branch
/// shuttle when the line has a branch with a bay. A `Vec` rather than one
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

/// Real seconds a smoke puff takes to rise and fade.
const PUFF_SECONDS: f32 = 1.6;

/// Radius of the locomotive's coupled drivers, which the rods turn with.
const DRIVER_RADIUS: f32 = 0.75;

/// Height of the rail tops above the ground (see the town's railway).
const RAIL_TOP: f32 = LineTrain::RAIL_TOP;

#[derive(Clone)]
struct Kit {
    cube: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    hopper: Handle<Mesh>,
    puff: Handle<Mesh>,
    smoke: Handle<StandardMaterial>,
    /// The coach's boarded lower sides.
    boards: Handle<StandardMaterial>,
    /// Panelling inside the coaches.
    panel: Handle<StandardMaterial>,
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

/// A board texture: vertical boards with dark seams and a faint grain,
/// a little lighter or darker each, as RGBA8 sRGB pixels `(width, height)`.
fn paint_boards() -> (u32, u32, Vec<u8>) {
    let (w, h, boards) = (48u32, 32u32, 6u32);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let board = x * boards / w;
            let seam = x % (w / boards) == 0;
            let tone = [214u32, 236, 222, 246, 206, 230][board as usize];
            let grain = ((x * 7 + y * 3 + board * 11) % 9) * 3;
            let v = if seam { 120 } else { tone - grain };
            pixels.extend([v as u8, v as u8, v as u8, 255]);
        }
    }
    (w, h, pixels)
}

impl Kit {
    fn new(
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        images: Option<&mut Assets<Image>>,
    ) -> Self {
        let boards = images.map(|images| {
            let (w, h, pixels) = paint_boards();
            let mut image = Image::new(
                Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                pixels,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
            images.add(image)
        });
        let mut finish = |rgb: [f32; 3], finish: Finish, repeat: Option<f32>| {
            let (perceptual_roughness, metallic, reflectance) = finish.pbr();
            materials.add(StandardMaterial {
                base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
                base_color_texture: repeat.and(boards.clone()),
                uv_transform: Affine2::from_scale(Vec2::splat(repeat.unwrap_or(1.0))),
                perceptual_roughness,
                metallic,
                reflectance,
                ..default()
            })
        };
        // Varnished wood: smoother than leather, not metal.
        let wood = Finish::Rawhide;
        let boards_mat = finish([0.20, 0.30, 0.22], wood, Some(16.0));
        let boxcar = finish([0.45, 0.24, 0.16], Finish::Bark, Some(12.0));
        let planks = finish([0.36, 0.27, 0.19], Finish::Bark, Some(10.0));
        let panel = finish([0.46, 0.30, 0.17], wood, Some(14.0));
        let black = finish([0.11, 0.10, 0.10], Finish::Iron, None);
        let brass = finish([0.62, 0.50, 0.26], Finish::Brass, None);
        let smoke = materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.53, 0.50, 0.35),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            ..default()
        });
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
            puff: meshes.add(Sphere::new(0.5).mesh().ico(1).expect("a low sphere")),
            smoke,
            boards: boards_mat,
            panel,
            black,
            smokebox: paint(0.17, 0.16, 0.15, 0.3, 0.8),
            red: paint(0.50, 0.14, 0.09, 0.1, 0.7),
            brass,
            boxcar,
            roof: paint(0.40, 0.38, 0.36, 0.4, 0.7),
            hopper_body: paint(0.47, 0.42, 0.34, 0.3, 0.8),
            grain: paint(0.66, 0.54, 0.32, 0.0, 1.0),
            coal: paint(0.07, 0.07, 0.07, 0.0, 1.0),
            window: paint(0.13, 0.11, 0.10, 0.0, 0.9),
            coach: paint(0.20, 0.30, 0.22, 0.1, 0.8),
            trim: paint(0.80, 0.72, 0.55, 0.0, 0.8),
            wood: paint(0.42, 0.27, 0.15, 0.0, 0.6),
            planks,
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
    /// Index of the car's train in [`WorldTrains`].
    train: usize,
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
                TrainWheel {
                    train: self.train,
                    radius,
                },
                Transform::from_xyz(x, radius, side * 0.95)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::new(2.0 * radius, 0.18, 2.0 * radius)),
            ));
        }
    }

    /// Underframe, buffer beams, a pair of sprung buffers and a coupling
    /// hook at each end, shared by every car.
    fn frame(&mut self, length: f32, deck: f32) {
        let (black, red) = (self.kit.black.clone(), self.kit.red.clone());
        self.cube(
            &black,
            Vec3::new(0.0, deck, 0.0),
            Vec3::new(length - 0.6, 0.35, 2.5),
        );
        for end in [-1.0, 1.0] {
            let beam = end * (length / 2.0 - 0.25);
            self.cube(&red, Vec3::new(beam, deck, 0.0), Vec3::new(0.3, 0.45, 2.7));
            for side in [-1.0, 1.0] {
                let z = side * 0.85;
                self.lying(
                    &black,
                    Vec3::new(end * (length / 2.0 - 0.18), deck, z),
                    0.07,
                    0.16,
                );
                self.lying(
                    &black,
                    Vec3::new(end * (length / 2.0 - 0.05), deck, z),
                    0.19,
                    0.08,
                );
            }
            self.cube(
                &black,
                Vec3::new(end * (length / 2.0 - 0.07), deck - 0.05, 0.0),
                Vec3::new(0.14, 0.1, 0.12),
            );
        }
    }

    /// Two four-wheeled bogies under a coach or a freight car: side frames,
    /// a bolster and two axles each.
    fn bogies(&mut self, length: f32) {
        let black = self.kit.black.clone();
        for end in [-1.0, 1.0] {
            let x = end * (length / 2.0 - 1.9);
            self.axle(x - 0.6, 0.45);
            self.axle(x + 0.6, 0.45);
            for side in [-1.0, 1.0] {
                self.cube(
                    &black,
                    Vec3::new(x, 0.5, side * 1.07),
                    Vec3::new(2.0, 0.22, 0.08),
                );
            }
            self.cube(&black, Vec3::new(x, 0.78, 0.0), Vec3::new(0.35, 0.18, 2.1));
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
            self.axle(x, DRIVER_RADIUS);
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
        self.cube(
            &coal,
            Vec3::new(tender_x + 0.6, 3.33, 0.0),
            Vec3::new(1.8, 0.2, 1.6),
        );
        self.locomotive_detail(length);
    }

    /// The engine's finer parts: boiler bands, smokebox door, headlamp,
    /// cylinders with their rods, cowcatcher, cab spectacles, tender
    /// coping, and the chimney's smoke.
    fn locomotive_detail(&mut self, length: f32) {
        let k = self.kit;
        let (black, brass, window, lamp) = (
            k.black.clone(),
            k.brass.clone(),
            k.window.clone(),
            k.lamp.clone(),
        );
        let front = length / 2.0;
        for x in [0.2, 1.8, 3.4, 5.0] {
            self.lying(&brass, Vec3::new(x, 2.3, 0.0), 0.97, 0.07);
        }
        // Smokebox door with its dart, and a headlamp on top.
        self.lying(&black, Vec3::new(front - 0.68, 2.3, 0.0), 0.8, 0.06);
        self.cube(
            &brass,
            Vec3::new(front - 0.64, 2.3, 0.0),
            Vec3::new(0.05, 0.12, 0.12),
        );
        self.cube(
            &black,
            Vec3::new(front - 0.55, 3.45, 0.0),
            Vec3::new(0.35, 0.4, 0.35),
        );
        self.cube(
            &lamp,
            Vec3::new(front - 0.37, 3.45, 0.0),
            Vec3::new(0.02, 0.22, 0.22),
        );
        // Cylinders, and the rods the drivers swing round.
        for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
            let z = side * 1.25;
            self.lying(&black, Vec3::new(5.9, 0.85, z), 0.3, 1.0);
            // Right-hand (+Z) cranks lead by a quarter turn; the drivers
            // turn clockwise, toward a lower phase.
            let phase = (i as f32 * -std::f32::consts::FRAC_PI_2).rem_euclid(std::f32::consts::TAU);
            self.rod(
                Vec3::new(2.9, 0.75, side * 1.12),
                Vec3::new(3.6, 0.1, 0.06),
                phase,
                0.0,
            );
            self.rod(
                Vec3::new(4.4, 0.8, side * 1.2),
                Vec3::new(3.0, 0.12, 0.06),
                phase,
                3.0,
            );
        }
        // A slatted cowcatcher under the buffer beam.
        for z in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            self.part(
                &k.cube,
                &black,
                Transform::from_xyz(front - 0.4, 0.38, z)
                    .with_rotation(Quat::from_rotation_z(-0.7))
                    .with_scale(Vec3::new(0.65, 0.06, 0.08)),
            );
        }
        self.cube(
            &black,
            Vec3::new(front - 0.25, 0.2, 0.0),
            Vec3::new(0.06, 0.06, 2.3),
        );
        // Spectacle windows in the cab front, over the boiler.
        for side in [-1.0, 1.0] {
            self.cube(
                &window,
                Vec3::new(-0.09, 3.25, side * 0.85),
                Vec3::new(0.02, 0.45, 0.5),
            );
        }
        // Coping round the tender's top.
        let tender_x = -front + 2.2;
        for side in [-1.0, 1.0] {
            self.cube(
                &black,
                Vec3::new(tender_x, 3.12, side * 1.33),
                Vec3::new(4.2, 0.16, 0.05),
            );
        }
        // Smoke puffs, under a holder at the car's origin so they never
        // count as the car's own height.
        let (mesh, smoke) = (k.puff.clone(), k.smoke.clone());
        let base = Vec3::new(front - 1.4, 4.55, 0.0);
        let train = self.train;
        self.car.with_children(|car| {
            car.spawn((Transform::default(), Visibility::default()))
                .with_children(|holder| {
                    for i in 0..4 {
                        holder.spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(smoke.clone()),
                            SmokePuff {
                                train,
                                phase: i as f32 / 4.0,
                                chimney: base,
                            },
                            Transform::from_translation(base),
                            Visibility::Hidden,
                        ));
                    }
                });
        });
    }

    /// A rod swung round by the drivers' cranks (see [`TrainRod`]).
    fn rod(&mut self, base: Vec3, size: Vec3, phase: f32, length: f32) {
        let mesh = self.kit.cube.clone();
        let material = self.kit.black.clone();
        self.car.with_child((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            TrainRod {
                train: self.train,
                base,
                crank: 0.3,
                radius: DRIVER_RADIUS,
                phase,
                length,
            },
            Transform::from_translation(base).with_scale(size),
        ));
    }

    fn coach(&mut self, length: f32) {
        let k = self.kit;
        let (roof, window, black) = (k.roof.clone(), k.window.clone(), k.black.clone());
        self.frame(length, 1.1);
        self.bogies(length);
        self.coach_shell(length);
        self.coach_inside(length);
        // The roof over the ceiling, with a glazed clerestory along it.
        self.cube(
            &roof,
            Vec3::new(0.0, 3.9, 0.0),
            Vec3::new(length - 0.4, 0.2, 3.0),
        );
        self.cube(
            &window,
            Vec3::new(0.0, 4.04, 0.0),
            Vec3::new(length - 1.6, 0.07, 1.46),
        );
        self.cube(
            &roof,
            Vec3::new(0.0, 4.08, 0.0),
            Vec3::new(length - 1.2, 0.16, 1.4),
        );
        // Open end platforms: railings either side of the door, and a step.
        let (floor, half) = (CoachBody::FLOOR, CoachBody::HALF_WIDTH);
        for end in [-1.0, 1.0] {
            let x = end * (length / 2.0 - 0.12);
            for side in [-1.0, 1.0] {
                let span = half - 0.45;
                self.cube(
                    &black,
                    Vec3::new(x, floor + 1.0, side * (0.45 + span / 2.0)),
                    Vec3::new(0.04, 0.04, span),
                );
                for z in [0.5, half - 0.05] {
                    self.cube(
                        &black,
                        Vec3::new(x, floor + 0.5, side * z),
                        Vec3::new(0.04, 1.0, 0.04),
                    );
                }
                self.cube(
                    &black,
                    Vec3::new(x, 0.75, side * (half - 0.2)),
                    Vec3::new(0.3, 0.05, 0.4),
                );
            }
        }
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
            for (lo, hi, skin) in [(floor, sill, &k.boards), (lintel, ceiling, &body)] {
                let (y, h) = band(lo, hi);
                self.cube(skin, Vec3::new(0.0, y, z), Vec3::new(inner, h, wall));
            }
            // Panelling inside, a sill ledge and a header outside.
            let (y, h) = band(floor, sill);
            let inside = side * (half - wall - 0.01);
            self.cube(
                &k.panel,
                Vec3::new(0.0, y, inside),
                Vec3::new(inner - 0.1, h, 0.02),
            );
            let outside = side * (half + 0.02);
            for (y, h) in [(sill - 0.03, 0.06), (lintel + 0.03, 0.06)] {
                self.cube(&trim, Vec3::new(0.0, y, outside), Vec3::new(inner, h, 0.04));
            }
            let (y, h) = band(sill, lintel);
            for pier in edges.chunks(2) {
                let (x, w) = band(pier[0], pier[1]);
                self.cube(&body, Vec3::new(x, y, z), Vec3::new(w, h, wall));
            }
            for &x in &windows {
                self.cube(&glass, Vec3::new(x, y, z), Vec3::new(open, h, 0.01));
                // The window's frame posts.
                for post in [x - open / 2.0 - 0.025, x + open / 2.0 + 0.025] {
                    self.cube(&trim, Vec3::new(post, y, outside), Vec3::new(0.05, h, 0.04));
                }
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

    /// Iron strapping on a freight car's sides between heights `lo` and
    /// `hi`: a chord along the top and bottom and an upright every 1.5 m.
    fn strapping(&mut self, length: f32, lo: f32, hi: f32) {
        let black = self.kit.black.clone();
        let span = length - 0.7;
        for side in [-1.0, 1.0] {
            let z = side * 1.42;
            for y in [lo + 0.05, hi - 0.05] {
                self.cube(&black, Vec3::new(0.0, y, z), Vec3::new(span, 0.1, 0.03));
            }
            let count = (span / 1.5).floor() as usize;
            for i in 0..=count {
                let x = -span / 2.0 + span * i as f32 / count.max(1) as f32;
                self.cube(
                    &black,
                    Vec3::new(x, (lo + hi) / 2.0, z),
                    Vec3::new(0.08, hi - lo, 0.03),
                );
            }
        }
    }

    fn boxcar(&mut self, length: f32) {
        let k = self.kit;
        let (body, roof, window) = (k.boxcar.clone(), k.roof.clone(), k.window.clone());
        self.frame(length, 1.1);
        self.bogies(length);
        self.strapping(length, 1.3, 3.9);
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
        // Sliding doors on both sides, hung from a rail.
        for side in [-1.0, 1.0] {
            self.cube(
                &window,
                Vec3::new(0.0, 2.45, side * 1.44),
                Vec3::new(1.8, 2.1, 0.03),
            );
            self.cube(
                &self.kit.black.clone(),
                Vec3::new(0.0, 3.6, side * 1.46),
                Vec3::new(3.8, 0.06, 0.04),
            );
        }
    }

    fn hopper(&mut self, length: f32) {
        let k = self.kit;
        let (body, grain, black) = (k.hopper_body.clone(), k.grain.clone(), k.black.clone());
        self.frame(length, 1.1);
        self.bogies(length);
        self.strapping(length, 1.7, 3.5);
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

/// The assets the car kit is made in (images only for the board texture).
type KitAssets<'w> = (
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<StandardMaterial>>,
    Option<ResMut<'w, Assets<Image>>>,
);

/// The shared car kit, made on first use.
#[derive(Resource)]
struct CarKit(Kit);

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
            let kit = Kit::new(&mut assets.0, &mut assets.1, assets.2.as_deref_mut());
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
            let mut b = CarBuilder {
                car: entity,
                train: train_index,
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{
        BASE_RATE, PlazaPlacement, RailLine, RailwayConfig, Side, Terrain, Timetable, TownConfig,
        TownLayout, Vec2 as GroundVec2, WorldConfig, place_towns,
    };
    use std::f32::consts::{PI, TAU};
    use std::time::Duration;

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
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        // Monte Nievas, and the others at their real offsets from it
        // (east, north, in meters) scaled 1:7 and rounded to the meter.
        let nievas = (-2450.0, -1950.0);
        let at = |east: f32, north: f32| {
            (
                (nievas.0 + east / 7.0).round(),
                (nievas.1 + north / 7.0).round(),
            )
        };
        let (castex, pico) = (at(-12_600.0, -5_400.0), at(35_800.0, 22_400.0));
        let (towns, errors) = place_towns(
            &world,
            &[
                town("Monte Nievas", nievas.0, nievas.1, (4, 3)),
                town("Eduardo Castex", castex.0, castex.1, (5, 4)),
                town("General Pico", pico.0, pico.1, (8, 8)),
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
            .insert_resource(WorldTrains(vec![train]))
            .add_plugins(TrainPlugin);
        app
    }

    /// A second, shorter train (a shuttle's loco, coach and boxcar) over the
    /// same line, leaving `down` and `up` (hours of the day), so it keeps to
    /// its own timetable apart from the line train.
    pub(crate) fn shuttle(line: &LineTrain, towns: &[TownLayout], down: u32, up: u32) -> LineTrain {
        let cars = [
            (CarKind::Locomotive, 14.0),
            (CarKind::Coach, 12.0),
            (CarKind::Boxcar, 10.0),
        ]
        .map(|(kind, length)| pampa_core::Car { kind, length })
        .to_vec();
        let served: Vec<usize> = line.stations().iter().map(|s| s.town).collect();
        let timetable = Timetable {
            down: vec![pampa_core::TimeOfDay::hm(down, 0)],
            up: vec![pampa_core::TimeOfDay::hm(up, 0)],
            ..Timetable::default()
        };
        LineTrain::on_path(line.path().clone(), &served, towns, cars, timetable)
            .expect("a shuttle on the line")
    }

    /// An app with the line train and the shuttle (leaving Castex at
    /// 22:00, Pico at 23:00, so parked at Castex all day) at `game_seconds`.
    fn app_with_two_trains(game_seconds: f64, focus: GroundVec2) -> App {
        let (towns, mut line) = line_train();
        let mut shuttle = shuttle(&line, &towns, 22, 23);
        line.advance(0.0, game_seconds, BASE_RATE);
        shuttle.advance(0.0, game_seconds, BASE_RATE);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTerrain(Terrain::default()))
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldTrains(vec![line, shuttle]))
            .add_plugins(TrainPlugin);
        app
    }

    /// Every car entity's (train, car) pair with its transform and visibility.
    fn all_cars(app: &mut App) -> Vec<(usize, usize, Transform, Visibility)> {
        let mut query = app
            .world_mut()
            .query::<(&TrainCar, &Transform, &Visibility)>();
        let mut out: Vec<_> = query
            .iter(app.world())
            .map(|(c, t, v)| (c.train, c.index, *t, *v))
            .collect();
        out.sort_by_key(|c| (c.0, c.1));
        out
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
        let placed = app.world().resource::<WorldTrains>().0[0]
            .clone()
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
        let train = app.world().resource::<WorldTrains>().0[0].clone();
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
        let train = app.world().resource::<WorldTrains>().0[0].clone();
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
            parts.iter().any(|(m, a, b)| *m == kit.boards
                && below.cmpge(*a).all()
                && below.cmple(*b).all())
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

    /// Triangles drawn for car `index`, over every mesh under it.
    fn triangles(app: &mut App, index: usize) -> usize {
        let mut q = app.world_mut().query::<(Entity, &TrainCar)>();
        let root = q
            .iter(app.world())
            .find(|(_, c)| c.index == index)
            .unwrap()
            .0;
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mut stack = vec![root];
        let mut total = 0;
        while let Some(e) = stack.pop() {
            if let Some(children) = app.world().get::<Children>(e) {
                stack.extend(children.iter());
            }
            if let Some(mesh) = app.world().get::<Mesh3d>(e) {
                let mesh = meshes.get(&mesh.0).unwrap();
                total += mesh.indices().map_or(mesh.count_vertices(), |i| i.len()) / 3;
            }
        }
        total
    }

    #[test]
    fn every_car_stays_within_a_ps2_era_triangle_budget() {
        let mut app = app_with_train(3600.0, GroundVec2::ZERO);
        app.update();
        let train = app.world().resource::<WorldTrains>().0[0].clone();
        for (index, car) in train.cars().iter().enumerate() {
            let tris = triangles(&mut app, index);
            println!("{:?}: {tris} triangles", car.kind);
            // The counts the line-train notes record, with a quarter's slack
            // either way.
            let documented = match car.kind {
                CarKind::Locomotive => 2156.0,
                CarKind::Coach => 2348.0,
                CarKind::Boxcar => 1124.0,
                CarKind::Hopper => 1100.0,
            };
            let range = (documented * 0.75) as usize..=(documented * 1.25) as usize;
            assert!(
                range.contains(&tris),
                "{:?}: {tris} not in {range:?}",
                car.kind
            );
        }
    }

    #[test]
    fn the_rods_swing_round_with_the_cranks() {
        let rod = TrainRod {
            train: 0,
            base: Vec3::new(2.9, 0.75, 1.12),
            crank: 0.3,
            radius: 0.75,
            phase: 0.0,
            length: 0.0,
        };
        let at = |phase: f32| rod_pose(&TrainRod { phase, ..rod }, Vec3::ONE).translation;
        assert!((at(0.0) - rod.base - Vec3::new(0.3, 0.0, 0.0)).length() < 1e-5);
        assert!(
            (at(std::f32::consts::FRAC_PI_2) - rod.base - Vec3::new(0.0, 0.3, 0.0)).length() < 1e-5
        );
        // A side rod stays level; a main rod tilts with the crank.
        assert_eq!(
            rod_pose(&TrainRod { phase: 1.0, ..rod }, Vec3::ONE).rotation,
            Quat::IDENTITY
        );
        let main = TrainRod {
            length: 3.0,
            phase: 1.0,
            ..rod
        };
        assert!(
            rod_pose(&main, Vec3::ONE)
                .rotation
                .angle_between(Quat::IDENTITY)
                > 0.01
        );
        // In the app, the rods turn with the drivers as the train runs: by
        // the distance rolled over the drivers' radius, in their sense.
        let mut app = running_app();
        let speed = app
            .world()
            .resource::<WorldTrains>()
            .line()
            .unwrap()
            .state()
            .speed;
        assert!(speed > 1.0, "the train is under way: {speed}");
        app.update();
        let rods = |app: &mut App| {
            let mut q = app.world_mut().query::<&TrainRod>();
            q.iter(app.world()).copied().collect::<Vec<_>>()
        };
        // A driver's crank angle in the car's side plane: where its local
        // +X (a point on the rim) has turned to about the car's Z axis.
        let drivers = |app: &mut App| {
            let mut q = app.world_mut().query::<(&TrainWheel, &Transform)>();
            q.iter(app.world())
                .filter(|(w, _)| w.radius == DRIVER_RADIUS)
                .map(|(_, t)| {
                    let rim = t.rotation * Vec3::X;
                    rim.y.atan2(rim.x)
                })
                .collect::<Vec<_>>()
        };
        let (rods_before, drivers_before) = (rods(&mut app), drivers(&mut app));
        assert_eq!(rods_before.len(), 4, "two side rods and two main rods");
        assert_eq!(drivers_before.len(), 6, "three coupled drivers a side");
        app.update();
        let turned = speed * app.world().resource::<Time>().delta_secs() / DRIVER_RADIUS;
        assert!(turned > 0.05, "a measurable turn: {turned}");
        let wrap = |a: f32| (a + PI).rem_euclid(TAU) - PI;
        for (before, after) in drivers_before.iter().zip(drivers(&mut app)) {
            // Rolling toward +X, a driver's front goes down: clockwise.
            assert!(
                (wrap(after - before) + turned).abs() < 1e-3,
                "driver {before} -> {after}, turned {turned}"
            );
        }
        for (before, after) in rods_before.iter().zip(rods(&mut app)) {
            assert_eq!(before.radius, DRIVER_RADIUS);
            assert!(
                (wrap(after.phase - before.phase) + turned).abs() < 1e-3,
                "rod turned {}, drivers {}",
                after.phase - before.phase,
                -turned
            );
        }
    }

    #[test]
    fn smoke_puffs_from_the_chimney_only_while_the_train_moves() {
        let puffs = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query::<(&SmokePuff, &Transform, &Visibility)>();
            q.iter(app.world())
                .map(|(p, t, v)| (*p, *t, *v))
                .collect::<Vec<_>>()
        };
        // Parked overnight: no smoke.
        let mut app = app_with_train(3600.0, GroundVec2::ZERO);
        app.update();
        app.update();
        let parked = puffs(&mut app);
        assert_eq!(parked.len(), 4);
        assert!(parked.iter().all(|p| p.2 == Visibility::Hidden));
        // Every puff leaves from the top of the chimney's cap.
        let chimney = parked[0].0.chimney;
        assert!(parked.iter().all(|p| p.0.chimney == chimney));
        assert!(boxes(&mut app, 0).iter().any(|(_, a, b)| {
            let mid = (*a + *b) / 2.0;
            (mid.x - chimney.x).abs() < 1e-4
                && (mid.z - chimney.z).abs() < 1e-4
                && (b.y - chimney.y).abs() < 1e-4
        }));
        // Under way: shown, rising from the chimney, cycling round.
        let mut app = running_app();
        app.update();
        let mut wrapped = false;
        let mut last = puffs(&mut app);
        for _ in 0..40 {
            app.update();
            let now = puffs(&mut app);
            for ((before, _, _), (puff, t, v)) in last.iter().zip(&now) {
                assert_eq!(*v, Visibility::Inherited);
                assert!((0.0..1.0).contains(&puff.phase));
                let rise = t.translation.y - puff.chimney.y;
                assert!((rise - (0.3 + puff.phase * 2.0)).abs() < 1e-4);
                assert!(t.translation.x <= puff.chimney.x);
                wrapped |= puff.phase < before.phase;
            }
            last = now;
        }
        assert!(wrapped, "puffs start again from the chimney");
    }

    /// The train under way out of Monte Nievas toward Castex, near the
    /// focus, with each frame a fixed twentieth of a second.
    fn running_app() -> App {
        let (_, probe) = line_train();
        let (_, depart) = probe.stop_times(pampa_core::Run::Up, 1).unwrap();
        let mut app = app_with_train(7.0 * 3600.0 + (depart + 30.0) * BASE_RATE, GroundVec2::ZERO);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )));
        app
    }

    #[test]
    fn cars_far_from_the_focus_are_hidden() {
        // Overnight at Pico, with the focus at Castex.
        let mut app = app_with_train(3600.0, GroundVec2::new(-4250.0, -2721.0));
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

    #[test]
    fn every_train_gets_its_own_cars() {
        let mut app = app_with_two_trains(3600.0, GroundVec2::ZERO);
        app.update();
        let drawn = all_cars(&mut app);
        let of = |train: usize| drawn.iter().filter(|c| c.0 == train).count();
        assert_eq!((of(0), of(1)), (8, 3), "the line train and the shuttle");
        assert_eq!(drawn.len(), 11);
        // The shuttle's cars are its own consist, locomotive first.
        let trains = app.world().resource::<WorldTrains>().clone();
        assert_eq!(trains.0[1].cars().len(), 3);
        let mut roots = app.world_mut().query::<(Entity, &TrainCar)>();
        let loco = roots
            .iter(app.world())
            .find(|(_, c)| (c.train, c.index) == (1, 0))
            .unwrap()
            .0;
        let wheels = app.world().get::<Children>(loco).unwrap();
        assert!(
            wheels
                .iter()
                .any(|e| app.world().get::<TrainRod>(e).is_some_and(|r| r.train == 1)),
            "the shuttle's locomotive has its rods"
        );
        // Parts of the second train carry its index.
        let mut parts = app.world_mut().query::<&TrainWheel>();
        let mut mine = |t: usize| parts.iter(app.world()).filter(|w| w.train == t).count();
        let (line, shuttle) = (mine(0), mine(1));
        assert!(shuttle > 0 && shuttle < line, "{shuttle} wheels of {line}");
    }

    #[test]
    fn each_car_follows_its_own_train() {
        // Overnight: the line train at Pico, the shuttle at Castex; the
        // focus at Castex, so only the shuttle's cars are near.
        let mut app = app_with_two_trains(3600.0, GroundVec2::ZERO);
        let castex = app.world().resource::<WorldTrains>().0[1].placed_cars()[1].center;
        app.insert_resource(StreamFocus(castex));
        app.update();
        app.update();
        let trains = app.world().resource::<WorldTrains>().clone();
        let drawn = all_cars(&mut app);
        for (train, index, t, visibility) in &drawn {
            let p = trains.0[*train].placed_cars()[*index];
            if *train == 1 {
                assert_eq!(*visibility, Visibility::Inherited, "shuttle car {index}");
                let at = t.translation;
                assert!((at.x - p.center.x).abs() < 1e-3 && (at.z + p.center.y).abs() < 1e-3);
            } else {
                assert_eq!(*visibility, Visibility::Hidden, "line car {index} far away");
            }
        }
        // And the other way round: at Pico only the line train is near.
        let pico = trains.0[0].placed_cars()[0].center;
        app.insert_resource(StreamFocus(pico));
        app.update();
        for (train, index, _, visibility) in all_cars(&mut app) {
            let near = train == 0;
            assert_eq!(
                visibility == Visibility::Inherited,
                near,
                "train {train} car {index}"
            );
        }
    }

    #[test]
    fn each_train_rolls_its_own_wheels_and_smokes_for_itself() {
        // The line train runs out of Monte Nievas; the shuttle, leaving at
        // 22:00, is parked at Castex.
        let (_, probe) = line_train();
        let (_, depart) = probe.stop_times(pampa_core::Run::Up, 1).unwrap();
        let at = 7.0 * 3600.0 + (depart + 30.0) * BASE_RATE;
        let mut app = app_with_two_trains(at, GroundVec2::ZERO);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )));
        let speeds: Vec<f32> = app
            .world()
            .resource::<WorldTrains>()
            .0
            .iter()
            .map(|t| t.state().speed)
            .collect();
        assert!(speeds[0] > 1.0 && speeds[1] == 0.0, "{speeds:?}");
        app.update();
        let spin = |app: &mut App| {
            let mut q = app.world_mut().query::<(&TrainWheel, &Transform)>();
            q.iter(app.world())
                .map(|(w, t)| (w.train, t.rotation))
                .collect::<Vec<_>>()
        };
        let before = spin(&mut app);
        let puffs = |app: &mut App, train: usize| {
            let mut q = app.world_mut().query::<(&SmokePuff, &Visibility)>();
            q.iter(app.world())
                .filter(|(p, _)| p.train == train)
                .map(|(_, v)| *v)
                .collect::<Vec<_>>()
        };
        app.update();
        let after = spin(&mut app);
        for ((train, a), (_, b)) in before.iter().zip(&after) {
            assert_eq!(a != b, *train == 0, "train {train}'s wheels");
        }
        assert!(after.iter().any(|w| w.0 == 1), "the shuttle has wheels");
        // The rods swing with their own train's drivers only.
        let phases = |app: &mut App| {
            let mut q = app.world_mut().query::<&TrainRod>();
            q.iter(app.world())
                .map(|r| (r.train, r.phase))
                .collect::<Vec<_>>()
        };
        let rods = phases(&mut app);
        app.update();
        let swung = phases(&mut app);
        assert!(rods.iter().any(|r| r.0 == 1), "the shuttle has rods");
        for ((train, a), (_, b)) in rods.iter().zip(&swung) {
            assert_eq!(a != b, *train == 0, "train {train}'s rods");
        }
        // Smoke: the line train's puffs show, the parked shuttle's do not.
        let (line, shuttle) = (puffs(&mut app, 0), puffs(&mut app, 1));
        assert_eq!((line.len(), shuttle.len()), (4, 4));
        assert!(line.iter().all(|v| *v == Visibility::Inherited));
        assert!(shuttle.iter().all(|v| *v == Visibility::Hidden));
    }
}
