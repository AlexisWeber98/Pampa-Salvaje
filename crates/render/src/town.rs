//! Procedural low-poly town drawn from a `pampa-core` [`TownLayout`]:
//! dirt streets, a tree-lined plaza, whitewash and brick buildings with
//! false-front parapets or pitched sheet-metal roofs, a railway with its
//! station, grain sheds, and windmills over round water tanks.
//!
//! Logic owns every footprint; this module only decides how each
//! [`BuildingKind`] looks.
//!
//! Towns are streamed: a town's meshes are built under one [`TownRoot`]
//! when the rider comes within [`TownActivation`]'s margin of it, and
//! despawned (root and all) past the margin plus the hysteresis. Signs and
//! the train hang their entities off the same root.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use pampa_core::{
    Aabb, Building, BuildingKind, Floor, RailSpan, Side, Terrain, TownActivation, TownLayout,
    Vec2 as GroundVec2,
};

use crate::coords::world_from_ground;
use crate::rail::{TrackKit, track_pieces};
use crate::terrain::{StreamFocus, WorldTerrain, hash2};

/// Every town of the world, in the order the game placed them; a town's
/// index here is its id everywhere in render. Without it nothing is drawn.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTowns(pub Vec<TownLayout>);

/// Which towns are near the rider (see [`TownActivation`]). Render builds
/// the active towns; the game simulates their people.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveTowns(pub TownActivation);

/// Root of one drawn town: every mesh, sign and train car of town `.0`
/// hangs off it, so despawning it drops the whole town.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownRoot(pub usize);

/// Updates [`ActiveTowns`] and spawns or despawns [`TownRoot`]s; runs in
/// `PreUpdate`, so every `Update` system sees this frame's towns.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamTowns;

/// Draws the [`WorldTowns`] near the rider and turns windmill rotors.
pub struct TownPlugin;

impl Plugin for TownPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTowns>()
            .init_resource::<ActiveTowns>()
            .init_resource::<StreamFocus>()
            .init_resource::<WorldTerrain>()
            .add_systems(Startup, setup_kit)
            .add_systems(PreUpdate, stream_towns.in_set(StreamTowns))
            .add_systems(Update, spin_rotors);
    }
}

/// Windmill wheel, turning about its local Z axis.
#[derive(Component, Debug, Clone, Copy)]
pub struct WindmillRotor {
    /// Radians per second.
    pub speed: f32,
}

fn spin_rotors(time: Res<Time>, mut rotors: Query<(&WindmillRotor, &mut Transform)>) {
    for (rotor, mut transform) in &mut rotors {
        transform.rotate_local_z(rotor.speed * time.delta_secs());
    }
}

/// Shared meshes, scaled per instance.
#[derive(Resource)]
struct Shapes {
    cube: Handle<Mesh>,
    /// Unit gable roof: base 1 x 1 on `y = 0`, ridge at `y = 1` along local Z.
    gable: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    pyramid: Handle<Mesh>,
    canopy: Handle<Mesh>,
}

/// Earthy palette matching the dusty atmosphere.
#[derive(Resource)]
struct Palette {
    whitewash: Handle<StandardMaterial>,
    lime_wash: Handle<StandardMaterial>,
    brick: Handle<StandardMaterial>,
    sheet_metal: Handle<StandardMaterial>,
    rusty_metal: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
    opening: Handle<StandardMaterial>,
    street: Handle<StandardMaterial>,
    plaza: Handle<StandardMaterial>,
    path: Handle<StandardMaterial>,
    canopy: Handle<StandardMaterial>,
    trunk: Handle<StandardMaterial>,
    concrete: Handle<StandardMaterial>,
    steel: Handle<StandardMaterial>,
    ballast: Handle<StandardMaterial>,
}

impl Palette {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let mut matte = |r: f32, g: f32, b: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: 0.95,
                reflectance: 0.15,
                ..default()
            })
        };
        let whitewash = matte(0.86, 0.82, 0.73);
        let lime_wash = matte(0.84, 0.74, 0.58);
        let brick = matte(0.60, 0.35, 0.25);
        let timber = matte(0.34, 0.25, 0.18);
        let opening = matte(0.16, 0.13, 0.11);
        let street = matte(0.58, 0.47, 0.34);
        let plaza = matte(0.47, 0.46, 0.29);
        let path = matte(0.66, 0.58, 0.45);
        let canopy = matte(0.27, 0.32, 0.17);
        let trunk = matte(0.33, 0.26, 0.19);
        let concrete = matte(0.62, 0.59, 0.54);
        let ballast = matte(0.48, 0.45, 0.41);
        let mut metal = |r: f32, g: f32, b: f32, metallic: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                metallic,
                perceptual_roughness: 0.55,
                ..default()
            })
        };
        Self {
            whitewash,
            lime_wash,
            brick,
            sheet_metal: metal(0.58, 0.59, 0.58, 0.5),
            rusty_metal: metal(0.52, 0.36, 0.26, 0.3),
            timber,
            opening,
            street,
            plaza,
            path,
            canopy,
            trunk,
            concrete,
            steel: metal(0.36, 0.34, 0.33, 0.8),
            ballast,
        }
    }
}

impl Palette {
    /// The track materials, with the shared cube, for [`TrackKit`].
    fn track_kit(&self, cube: Handle<Mesh>) -> TrackKit {
        TrackKit {
            cube,
            ballast: self.ballast.clone(),
            timber: self.timber.clone(),
            steel: self.steel.clone(),
        }
    }
}

/// Spawn helper that keeps the per-element code short. Everything it
/// spawns hangs off the town's `root`.
struct Builder<'c, 'w, 's, 'a> {
    commands: &'c mut Commands<'w, 's>,
    shapes: &'a Shapes,
    palette: &'a Palette,
    terrain: &'a Terrain,
    root: Entity,
}

impl Builder<'_, '_, '_, '_> {
    fn spawn(&mut self, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, t: Transform) {
        self.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            t,
            ChildOf(self.root),
        ));
    }

    /// Axis-aligned box from world `center` with world `size`.
    fn cube(&mut self, material: &Handle<StandardMaterial>, center: Vec3, size: Vec3) {
        let mesh = self.shapes.cube.clone();
        self.spawn(
            &mesh,
            material,
            Transform::from_translation(center).with_scale(size),
        );
    }

    fn ground(&self, p: GroundVec2) -> f32 {
        self.terrain.height(p.x, p.y)
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let palette = Palette::new(&mut materials);
    // The railway line between towns is drawn with the same track kit.
    commands.insert_resource(palette.track_kit(cube.clone()));
    commands.insert_resource(Shapes {
        cube,
        gable: meshes.add(gable_mesh()),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(16)),
        pyramid: meshes.add(Cone::new(0.71, 1.0).mesh().resolution(4)),
        canopy: meshes.add(
            Sphere::new(0.5)
                .mesh()
                .ico(1)
                .expect("valid ico subdivision"),
        ),
    });
    commands.insert_resource(palette);
}

/// Shared town meshes and materials, once [`setup_kit`] ran.
type TownKit<'a> = (Option<Res<'a, Shapes>>, Option<Res<'a, Palette>>);

/// Activates the towns near [`StreamFocus`]: builds the newly active ones
/// under a fresh [`TownRoot`] and despawns the roots of the ones left behind.
fn stream_towns(
    mut commands: Commands,
    (towns, terrain, focus): (Res<WorldTowns>, Res<WorldTerrain>, Res<StreamFocus>),
    mut active: ResMut<ActiveTowns>,
    (shapes, palette): TownKit,
    roots: Query<(Entity, &TownRoot)>,
) {
    let (Some(shapes), Some(palette)) = (shapes, palette) else {
        return;
    };
    let change = active.0.update(&towns.0, focus.0);
    for (entity, root) in &roots {
        if change.deactivated.contains(&root.0) {
            commands.entity(entity).despawn();
        }
    }
    for index in change.activated {
        let town = &towns.0[index];
        let root = commands
            .spawn((
                Name::new(format!("Town {}", town.name)),
                TownRoot(index),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        let mut b = Builder {
            commands: &mut commands,
            shapes: &shapes,
            palette: &palette,
            terrain: &terrain.0,
            root,
        };
        build_town(&mut b, town);
    }
}

/// Every mesh of `town`, under the builder's root.
fn build_town(b: &mut Builder, town: &TownLayout) {
    for street in &town.streets {
        let mat = b.palette.street.clone();
        flat_patch(b, &mat, street, 0.03);
    }
    plazas(b, town);
    railway(b, town);
    for (n, building) in town.buildings.iter().enumerate() {
        spawn_building(b, building, n, town.hitching_post.veranda);
    }
    for floor in town.floors() {
        floor_ramps(b, &floor);
    }
    hitching_rail(b, &town.hitching_post.rail);
}

/// Thickness of a ramp slab, in meters.
const RAMP_THICKNESS: f32 = 0.2;

/// A concrete slab sloping from each end of a platform's top down to the
/// ground, along the ramps logic walks people over ([`Floor::rise_at`]).
fn floor_ramps(b: &mut Builder, floor: &Floor) {
    let concrete = b.palette.concrete.clone();
    let f = floor.footprint;
    let mid = floor.across() * f.center().dot(floor.across());
    let width = f.size().dot(floor.across());
    let (lo, hi) = (f.min.dot(floor.along), f.max.dot(floor.along));
    for (end, out) in [(lo, -1.0), (hi, 1.0)] {
        // From the platform's edge (top) to the ramp's foot (ground).
        let top = mid + floor.along * end;
        let foot = top + floor.along * (out * Floor::RAMP_LENGTH);
        let (top3, foot3) = (
            world_from_ground(top, b.ground(top) + floor.height),
            world_from_ground(foot, b.ground(foot)),
        );
        let slope = (foot3 - top3).normalize();
        let side = slope.cross(Vec3::Y).normalize();
        let up = side.cross(slope);
        let rotation = Quat::from_mat3(&Mat3::from_cols(slope, up, side));
        let center = (top3 + foot3) * 0.5 - up * (RAMP_THICKNESS / 2.0);
        let mesh = b.shapes.cube.clone();
        b.spawn(
            &mesh,
            &concrete,
            Transform::from_translation(center)
                .with_rotation(rotation)
                .with_scale(Vec3::new(top3.distance(foot3), RAMP_THICKNESS, width)),
        );
    }
}

/// Two timber posts with a rail between them, along the rail footprint's
/// long side. The tethered horse itself is a character billboard.
fn hitching_rail(b: &mut Builder, rail: &Aabb) {
    let timber = b.palette.timber.clone();
    let size = rail.size();
    let along_x = size.x >= size.y;
    let length = size.x.max(size.y);
    let c = rail.center();
    let y = b.ground(c);
    for end in [-0.5, 0.5] {
        let offset = end * (length - 0.2);
        let p = if along_x {
            GroundVec2::new(c.x + offset, c.y)
        } else {
            GroundVec2::new(c.x, c.y + offset)
        };
        b.cube(
            &timber,
            world_from_ground(p, y + 0.55),
            Vec3::new(0.16, 1.1, 0.16),
        );
    }
    let bar = if along_x {
        Vec3::new(length, 0.1, 0.1)
    } else {
        Vec3::new(0.1, 0.1, length)
    };
    b.cube(&timber, world_from_ground(c, y + 1.0), bar);
}

/// A thin slab lying on (leveled) ground over `area`.
fn flat_patch(b: &mut Builder, material: &Handle<StandardMaterial>, area: &Aabb, lift: f32) {
    let center = area.center();
    let size = area.size();
    let y = b.ground(center);
    b.cube(
        material,
        world_from_ground(center, y + lift - 0.05),
        Vec3::new(size.x, 0.1, size.y),
    );
}

/// Every plaza (the main one, and a big town's second one), then the
/// trees of all of them.
fn plazas(b: &mut Builder, town: &TownLayout) {
    for p in town.plazas() {
        plaza(b, p);
    }
    plaza_trees(b, town);
}

/// One plaza: lawn, a cross of paths and a monument where they meet.
fn plaza(b: &mut Builder, p: Aabb) {
    let (plaza, path) = (b.palette.plaza.clone(), b.palette.path.clone());
    flat_patch(b, &plaza, &p, 0.05);
    // A cross of paths meeting at a small monument.
    let c = p.center();
    let s = p.size();
    for size in [GroundVec2::new(s.x, 3.0), GroundVec2::new(3.0, s.y)] {
        flat_patch(b, &path, &Aabb::from_center_size(c, size), 0.07);
    }
    let y = b.ground(c);
    let (concrete, whitewash) = (b.palette.concrete.clone(), b.palette.whitewash.clone());
    b.cube(
        &concrete,
        world_from_ground(c, y + 0.4),
        Vec3::new(3.0, 0.8, 3.0),
    );
    b.cube(
        &whitewash,
        world_from_ground(c, y + 2.2),
        Vec3::new(0.9, 2.8, 0.9),
    );
}

/// Every plaza tree: a trunk and a round crown.
fn plaza_trees(b: &mut Builder, town: &TownLayout) {
    for (n, &tree) in town.trees.iter().enumerate() {
        let y = b.ground(tree);
        let jitter = hash2(tree.x, tree.y);
        let trunk_h = 2.6 + jitter;
        let crown = 3.6 + 1.4 * hash2(tree.y, n as f32);
        let (trunk, canopy) = (b.palette.trunk.clone(), b.palette.canopy.clone());
        let cylinder = b.shapes.cylinder.clone();
        b.spawn(
            &cylinder,
            &trunk,
            Transform::from_translation(world_from_ground(tree, y + trunk_h / 2.0))
                .with_scale(Vec3::new(0.5, trunk_h, 0.5)),
        );
        let sphere = b.shapes.canopy.clone();
        b.spawn(
            &sphere,
            &canopy,
            Transform::from_translation(world_from_ground(tree, y + trunk_h + crown * 0.3))
                .with_rotation(Quat::from_rotation_y(jitter * 6.0))
                .with_scale(Vec3::new(crown, crown * 0.8, crown)),
        );
    }
}

/// Ballast bed, sleepers and two rails following the ground, drawn with
/// the pieces the railway line between towns uses too.
fn railway(b: &mut Builder, town: &TownLayout) {
    let Some(rail) = town.railway else {
        return;
    };
    let kit = b.palette.track_kit(b.shapes.cube.clone());
    for (part, transform) in track_pieces(RailSpan::whole(rail), b.terrain) {
        b.spawn(&kit.cube, kit.material(part), transform);
    }
}

/// World-space frame of a building: ground center, wall height, the footprint
/// size along the facade (`width`) and away from it (`depth`), and the
/// outward facade normal.
struct Frame {
    base: Vec3,
    height: f32,
    width: f32,
    depth: f32,
    front: Vec3,
    /// Rotation that maps local +Z to `front`.
    facing: Quat,
}

impl Frame {
    fn new(b: &Builder, building: &Building) -> Self {
        let center = building.footprint.center();
        let size = building.footprint.size();
        let base = world_from_ground(center, b.ground(center));
        let (front, width, depth) = match building.front {
            Side::North => (Vec3::NEG_Z, size.x, size.y),
            Side::South => (Vec3::Z, size.x, size.y),
            Side::East => (Vec3::X, size.y, size.x),
            Side::West => (Vec3::NEG_X, size.y, size.x),
        };
        Self {
            base,
            height: building.height,
            width,
            depth,
            front,
            facing: Quat::from_rotation_arc(Vec3::Z, front),
        }
    }

    /// World position of a point given in facade space: `x` along the facade
    /// (right when facing it from the street is -x), `y` up, `z` toward the street.
    fn at(&self, x: f32, y: f32, z: f32) -> Vec3 {
        self.base + self.facing * Vec3::new(x, y, z)
    }

    /// A box in facade space.
    fn cube(&self, b: &mut Builder, material: &Handle<StandardMaterial>, at: Vec3, size: Vec3) {
        let mesh = b.shapes.cube.clone();
        b.spawn(
            &mesh,
            material,
            Transform::from_translation(self.at(at.x, at.y, at.z))
                .with_rotation(self.facing)
                .with_scale(size),
        );
    }

    fn walls(&self, b: &mut Builder, material: &Handle<StandardMaterial>) {
        // Sink walls slightly so they meet the ground cleanly.
        self.cube(
            b,
            material,
            Vec3::new(0.0, (self.height - 0.2) / 2.0, 0.0),
            Vec3::new(self.width, self.height + 0.2, self.depth),
        );
    }

    /// Gable roof on top of the walls. `ridge_along_facade` puts the ridge
    /// parallel to the street; otherwise the gable faces the street.
    fn gable_roof(
        &self,
        b: &mut Builder,
        material: &Handle<StandardMaterial>,
        rise: f32,
        ridge_along_facade: bool,
    ) {
        let overhang = 0.4;
        let (span, length, turn) = if ridge_along_facade {
            (
                self.depth,
                self.width,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            )
        } else {
            (self.width, self.depth, Quat::IDENTITY)
        };
        let mesh = b.shapes.gable.clone();
        b.spawn(
            &mesh,
            material,
            Transform::from_translation(self.at(0.0, self.height, 0.0))
                .with_rotation(self.facing * turn)
                .with_scale(Vec3::new(span + overhang, rise, length + overhang)),
        );
    }

    /// Flat roof slab plus a stepped false front rising above it on the street side.
    fn false_front(&self, b: &mut Builder, wall: &Handle<StandardMaterial>, extra: f32) {
        let roof = b.palette.sheet_metal.clone();
        self.cube(
            b,
            &roof,
            Vec3::new(0.0, self.height + 0.05, -0.3),
            Vec3::new(self.width - 0.2, 0.15, self.depth - 0.6),
        );
        let z = self.depth / 2.0;
        self.cube(
            b,
            wall,
            Vec3::new(0.0, self.height + extra / 2.0, z - 0.15),
            Vec3::new(self.width + 0.2, extra, 0.3),
        );
        // Raised central step and a dark cornice line.
        self.cube(
            b,
            wall,
            Vec3::new(0.0, self.height + extra + 0.3, z - 0.15),
            Vec3::new(self.width * 0.4, 0.6, 0.3),
        );
        let trim = b.palette.timber.clone();
        self.cube(
            b,
            &trim,
            Vec3::new(0.0, self.height + extra - 0.1, z),
            Vec3::new(self.width + 0.3, 0.2, 0.15),
        );
    }

    /// A door flanked by windows on the street facade.
    fn openings(&self, b: &mut Builder, door_height: f32) {
        let dark = b.palette.opening.clone();
        let z = self.depth / 2.0 + 0.03;
        self.cube(
            b,
            &dark,
            Vec3::new(0.0, door_height / 2.0, z),
            Vec3::new(1.2, door_height, 0.08),
        );
        let windows = if self.width > 9.0 { 2 } else { 1 };
        for k in 1..=windows {
            for side in [-1.0, 1.0] {
                let x = side * (1.2 + k as f32 * self.width / (2.0 * windows as f32 + 1.0));
                if x.abs() + 0.6 < self.width / 2.0 {
                    self.cube(b, &dark, Vec3::new(x, 1.7, z), Vec3::new(0.9, 1.3, 0.08));
                }
            }
        }
    }

    /// Veranda roof over the sidewalk, on timber posts.
    fn veranda(&self, b: &mut Builder, depth: f32, height: f32) {
        let (roof, posts) = (b.palette.sheet_metal.clone(), b.palette.timber.clone());
        let z = self.depth / 2.0 + depth / 2.0;
        self.cube(
            b,
            &roof,
            Vec3::new(0.0, height, z),
            Vec3::new(self.width, 0.12, depth),
        );
        let n = (self.width / 3.5).floor().max(1.0) as usize;
        for k in 0..=n {
            let x = -self.width / 2.0 + 0.2 + k as f32 * (self.width - 0.4) / n as f32;
            self.cube(
                b,
                &posts,
                Vec3::new(x, height / 2.0, self.depth / 2.0 + depth - 0.15),
                Vec3::new(0.18, height, 0.18),
            );
        }
    }
}

/// Spawns one building; `veranda` is the general store's veranda depth.
fn spawn_building(b: &mut Builder, building: &Building, n: usize, veranda: f32) {
    let f = Frame::new(b, building);
    let pick = hash2(n as f32 * 1.37, building.footprint.min.x * 0.11);
    let p = b.palette;
    let (whitewash, lime, brick) = (p.whitewash.clone(), p.lime_wash.clone(), p.brick.clone());
    let (metal, rust) = (p.sheet_metal.clone(), p.rusty_metal.clone());
    match building.kind {
        BuildingKind::House => {
            let wall = match (pick * 3.0) as u32 {
                0 => &brick,
                1 => &lime,
                _ => &whitewash,
            };
            f.walls(b, wall);
            if pick < 0.45 {
                f.false_front(b, wall, 1.3);
            } else {
                let roof = if pick > 0.8 { &rust } else { &metal };
                f.gable_roof(b, roof, 1.8, true);
            }
            f.openings(b, 2.2);
        }
        BuildingKind::GeneralStore => {
            f.walls(b, &brick);
            f.false_front(b, &brick, 2.2);
            f.openings(b, 2.6);
            f.veranda(b, veranda, 3.2);
        }
        BuildingKind::PoliceStation => {
            f.walls(b, &whitewash);
            f.false_front(b, &whitewash, 1.6);
            f.openings(b, 2.4);
            // Flagpole by the door.
            let pole = b.palette.timber.clone();
            f.cube(
                b,
                &pole,
                Vec3::new(f.width / 2.0 - 0.8, f.height + 2.5, f.depth / 2.0 - 0.6),
                Vec3::new(0.12, 5.0, 0.12),
            );
        }
        BuildingKind::Church => {
            f.walls(b, &whitewash);
            f.gable_roof(b, &rust, 3.5, false);
            f.openings(b, 3.2);
            // Bell tower at the front, with a pyramid roof and a cross.
            let tower = 4.0;
            let top = f.height + 6.0;
            let z = f.depth / 2.0 - tower / 2.0;
            f.cube(
                b,
                &whitewash,
                Vec3::new(0.0, top / 2.0, z),
                Vec3::new(tower, top, tower),
            );
            let pyramid = b.shapes.pyramid.clone();
            b.spawn(
                &pyramid,
                &rust,
                Transform::from_translation(f.at(0.0, top + 1.25, z))
                    .with_rotation(f.facing * Quat::from_rotation_y(std::f32::consts::FRAC_PI_4))
                    .with_scale(Vec3::new(tower + 0.4, 2.5, tower + 0.4)),
            );
            let cross = b.palette.timber.clone();
            f.cube(
                b,
                &cross,
                Vec3::new(0.0, top + 3.2, z),
                Vec3::new(0.15, 1.4, 0.15),
            );
            f.cube(
                b,
                &cross,
                Vec3::new(0.0, top + 3.4, z),
                Vec3::new(0.8, 0.15, 0.15),
            );
        }
        BuildingKind::Station => {
            f.walls(b, &brick);
            f.gable_roof(b, &metal, 2.2, true);
            f.openings(b, 2.6);
            // Platform canopy on the track side (the back of the building).
            let back = Frame {
                front: -f.front,
                facing: f.facing * Quat::from_rotation_y(std::f32::consts::PI),
                ..f
            };
            back.openings(b, 2.6);
            back.veranda(b, 5.5, 3.6);
        }
        BuildingKind::Platform => {
            let concrete = b.palette.concrete.clone();
            f.cube(
                b,
                &concrete,
                Vec3::new(0.0, (f.height - 0.2) / 2.0, 0.0),
                Vec3::new(f.width, f.height + 0.2, f.depth),
            );
        }
        BuildingKind::Shed | BuildingKind::GrainStore => {
            let walls = if pick < 0.5 { &metal } else { &rust };
            f.walls(b, walls);
            let rise = if building.kind == BuildingKind::GrainStore {
                2.8
            } else {
                1.6
            };
            f.gable_roof(b, &metal, rise, building.kind == BuildingKind::GrainStore);
            let door = b.palette.opening.clone();
            f.cube(
                b,
                &door,
                Vec3::new(0.0, f.height * 0.4, f.depth / 2.0 + 0.03),
                Vec3::new((f.width * 0.4).min(5.0), f.height * 0.8, 0.08),
            );
        }
        BuildingKind::WaterTank => {
            let cylinder = b.shapes.cylinder.clone();
            let diameter = f.width.min(f.depth);
            b.spawn(
                &cylinder,
                &metal,
                Transform::from_translation(f.at(0.0, f.height / 2.0, 0.0))
                    .with_scale(Vec3::new(diameter, f.height, diameter)),
            );
        }
        BuildingKind::Windmill => windmill(b, &f, n),
    }
}

/// Lattice tower of four leaning legs with a turning wheel and a tail vane.
fn windmill(b: &mut Builder, f: &Frame, n: usize) {
    let steel = b.palette.steel.clone();
    let metal = b.palette.sheet_metal.clone();
    let foot = f.width.min(f.depth) / 2.0;
    let top = f.height;
    let head = 0.3;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let a = f.at(sx * foot, -0.2, sz * foot);
        let c = f.at(sx * head, top, sz * head);
        beam(b, &steel, a, c, 0.12);
    }
    // Cross braces at two levels.
    for level in [0.35, 0.7] {
        let r = foot + (head - foot) * level;
        let y = top * level;
        for (a, c) in [
            ((-r, -r), (r, -r)),
            ((r, -r), (r, r)),
            ((r, r), (-r, r)),
            ((-r, r), (-r, -r)),
        ] {
            beam(b, &steel, f.at(a.0, y, a.1), f.at(c.0, y, c.1), 0.06);
        }
    }

    // The wheel faces the prevailing wind; vary it a little per windmill.
    let yaw = Quat::from_rotation_y(0.6 + hash2(n as f32, 3.3) * 0.8);
    let hub = f.at(0.0, top + 0.4, 0.0);
    let wheel = b
        .commands
        .spawn((
            Name::new("Windmill wheel"),
            WindmillRotor {
                speed: 1.5 + hash2(n as f32, 7.7),
            },
            Transform::from_translation(hub + yaw * Vec3::Z * 0.6).with_rotation(yaw),
            Visibility::default(),
            ChildOf(b.root),
        ))
        .id();
    let blades = 12;
    for k in 0..blades {
        let angle = k as f32 * std::f32::consts::TAU / blades as f32;
        let rot = Quat::from_rotation_z(angle);
        b.commands.entity(wheel).with_child((
            Mesh3d(b.shapes.cube.clone()),
            MeshMaterial3d(metal.clone()),
            Transform::from_translation(rot * Vec3::new(0.0, 1.1, 0.0))
                .with_rotation(rot * Quat::from_rotation_y(0.35))
                .with_scale(Vec3::new(0.35, 1.6, 0.03)),
        ));
    }
    // Tail vane behind the hub.
    let vane = Transform::from_translation(hub - yaw * Vec3::Z * 1.4)
        .with_rotation(yaw)
        .with_scale(Vec3::new(0.04, 0.9, 2.0));
    let mesh = b.shapes.cube.clone();
    b.spawn(&mesh, &metal, vane);
}

/// A square-section beam between two world points.
fn beam(
    b: &mut Builder,
    material: &Handle<StandardMaterial>,
    from: Vec3,
    to: Vec3,
    thickness: f32,
) {
    let d = to - from;
    let len = d.length();
    if len <= 0.0 {
        return;
    }
    let mesh = b.shapes.cube.clone();
    b.spawn(
        &mesh,
        material,
        Transform::from_translation((from + to) / 2.0)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, d / len))
            .with_scale(Vec3::new(thickness, len, thickness)),
    );
}

/// Unit gable roof prism, flat-shaded: base `[-0.5, 0.5]²` at `y = 0`, ridge
/// along Z at `y = 1`. Scale it to span, rise and length per building.
fn gable_mesh() -> Mesh {
    let a = [-0.5, 0.0, -0.5];
    let b = [0.5, 0.0, -0.5];
    let c = [0.5, 0.0, 0.5];
    let d = [-0.5, 0.0, 0.5];
    let r1 = [0.0, 1.0, -0.5];
    let r2 = [0.0, 1.0, 0.5];
    // Counter-clockwise seen from outside.
    let positions = vec![
        a, d, r2, a, r2, r1, // west slope
        c, b, r1, c, r1, r2, // east slope
        d, c, r2, // front gable (+Z)
        b, a, r1, // back gable (-Z)
    ];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.compute_normals();
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{TownConfig, Vec2};

    fn two_towns() -> Vec<TownLayout> {
        let far = TownConfig {
            name: "Lejos".into(),
            origin: Vec2::new(3000.0, -2000.0),
            ..TownConfig::castex()
        };
        vec![
            TownLayout::castex_inspired(),
            TownLayout::generate(&far).unwrap(),
        ]
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTowns(two_towns()))
            .add_plugins(TownPlugin);
        app
    }

    fn roots(app: &mut App) -> Vec<usize> {
        let mut q = app.world_mut().query::<&TownRoot>();
        let mut v: Vec<usize> = q.iter(app.world()).map(|r| r.0).collect();
        v.sort_unstable();
        v
    }

    fn town_meshes(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<Mesh3d>>();
        q.iter(app.world()).count()
    }

    #[test]
    fn only_towns_near_the_focus_are_built_and_far_ones_are_dropped() {
        let mut app = app();
        app.update();
        assert_eq!(roots(&mut app), vec![0]);
        let castex = town_meshes(&mut app);
        assert!(castex > 100, "{castex} meshes");

        // Ride to the far town: Castex goes, the far town comes.
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(2950.0, -1950.0);
        app.update();
        assert_eq!(roots(&mut app), vec![1]);
        assert!(app.world().resource::<ActiveTowns>().0.is_active(1));
        // Same layout shifted: about as many meshes (the long track rounds
        // into a few more or fewer pieces far from the origin), none of them
        // left over from Castex.
        let far = town_meshes(&mut app);
        assert!(far.abs_diff(castex) < castex / 50, "{far} vs {castex}");

        // Halfway between them: nothing is drawn.
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(1500.0, -1000.0);
        app.update();
        assert!(roots(&mut app).is_empty());
        assert_eq!(town_meshes(&mut app), 0);
    }
}
