//! Procedural low-poly town drawn from a `pampa-core` [`TownLayout`]:
//! dirt streets, a tree-lined plaza, the 1919 Italianate buildings of the
//! facade kit ([`crate::facade::town_buildings`]), a railway with its
//! station, grain sheds, and windmills over round water tanks.
//!
//! Logic owns every footprint; this module only decides how each
//! [`BuildingKind`] looks.
//!
//! Towns are streamed: a town's meshes are built under one [`TownRoot`]
//! when the rider comes within [`TownActivation`]'s margin of it, and
//! despawned (root and all) past the margin plus the hysteresis. Signs and
//! the train hang their entities off the same root.
//!
//! Each building is one mesh per [`Surface`] it wears, sharing the kit's
//! [`FacadeMaterials`]. The first town filled is built at once, so the
//! first frame shows it; a town streamed in later builds its buildings on
//! the async compute pool and they land a few frames after its root, still
//! far past [`crate::settlement::BUILDING_FAR`] (the activation margin is wider). Buildings,
//! small props and trees are drawn within the settlements' shared
//! [`ranges`]: no new [`VisibilityRange`].

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use pampa_core::{
    Aabb, Building, BuildingKind, Floor, RailSpan, StoreStyle, Terrain, TownActivation, TownLayout,
    Vec2 as GroundVec2,
};

use crate::coords::world_from_ground;
use crate::facade::town_buildings::{self, Lot};
use crate::facade::{FacadeMaterials, Surface};
use crate::rail::{TrackKit, track_pieces};
use crate::settlement::{props, ranges};
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
        crate::facade::ensure_plugin(app);
        app.init_resource::<WorldTowns>()
            .init_resource::<ActiveTowns>()
            .init_resource::<StreamFocus>()
            .init_resource::<WorldTerrain>()
            .add_systems(Startup, setup_kit)
            .add_systems(
                PreUpdate,
                (stream_towns, land_buildings).chain().in_set(StreamTowns),
            )
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

/// One drawn piece of building `.0` of its town (an index into
/// [`TownLayout::buildings`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownBuilding(pub usize);

/// Shared meshes, scaled per instance.
#[derive(Resource)]
struct Shapes {
    cube: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    canopy: Handle<Mesh>,
    /// The windmill's wheel, one mesh per surface (the settlements' molino
    /// wheel).
    rotor: Vec<(Surface, Handle<Mesh>)>,
}

/// Earthy palette of the streets, plaza, trees, railway and platform.
#[derive(Resource)]
struct Palette {
    whitewash: Handle<StandardMaterial>,
    timber: Handle<StandardMaterial>,
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
        let timber = matte(0.34, 0.25, 0.18);
        let street = matte(0.58, 0.47, 0.34);
        let plaza = matte(0.47, 0.46, 0.29);
        let path = matte(0.66, 0.58, 0.45);
        let canopy = matte(0.27, 0.32, 0.17);
        let trunk = matte(0.33, 0.26, 0.19);
        let concrete = matte(0.62, 0.59, 0.54);
        let ballast = matte(0.48, 0.45, 0.41);
        let steel = materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.34, 0.33),
            metallic: 0.8,
            perceptual_roughness: 0.55,
            ..default()
        });
        Self {
            whitewash,
            timber,
            street,
            plaza,
            path,
            canopy,
            trunk,
            concrete,
            steel,
            ballast,
        }
    }

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
/// spawns hangs off the town's `root`, within `range` when one is set.
struct Builder<'c, 'w, 's, 'a> {
    commands: &'c mut Commands<'w, 's>,
    shapes: &'a Shapes,
    palette: &'a Palette,
    terrain: &'a Terrain,
    root: Entity,
    range: Option<VisibilityRange>,
}

impl Builder<'_, '_, '_, '_> {
    fn spawn(&mut self, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, t: Transform) {
        let mut e = self.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            t,
            ChildOf(self.root),
        ));
        if let Some(range) = &self.range {
            e.insert(range.clone());
        }
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
    let rotor = props::rotor()
        .into_meshes()
        .into_iter()
        .map(|(surface, mesh)| (surface, meshes.add(mesh)))
        .collect();
    commands.insert_resource(Shapes {
        cube,
        cylinder: meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(16)),
        canopy: meshes.add(
            Sphere::new(0.5)
                .mesh()
                .ico(1)
                .expect("valid ico subdivision"),
        ),
        rotor,
    });
    commands.insert_resource(palette);
}

/// Shared town meshes and materials, once [`setup_kit`] and the facade
/// kit ran.
type TownKit<'a> = (
    Option<Res<'a, Shapes>>,
    Option<Res<'a, Palette>>,
    Option<Res<'a, FacadeMaterials>>,
);

/// A town's buildings, being built on the async compute pool;
/// [`land_buildings`] spawns them when ready. Despawning the town's root
/// first drops it, which cancels the build.
#[derive(Component)]
struct PendingBuildings(Task<Vec<Built>>);

/// Activates the towns near [`StreamFocus`]: builds the newly active ones
/// under a fresh [`TownRoot`] and despawns the roots of the ones left behind.
/// The first run lays its towns' buildings at once; later ones build them
/// off the main thread.
fn stream_towns(
    mut commands: Commands,
    (towns, terrain, focus): (Res<WorldTowns>, Res<WorldTerrain>, Res<StreamFocus>),
    mut active: ResMut<ActiveTowns>,
    (shapes, palette, surfaces): TownKit,
    roots: Query<(Entity, &TownRoot)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut first_fill_laid: Local<bool>,
) {
    let (Some(shapes), Some(palette), Some(surfaces)) = (shapes, palette, surfaces) else {
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
            range: None,
        };
        build_town(&mut b, town);
        let lots = lots(town, &terrain.0);
        if *first_fill_laid {
            let task = AsyncComputeTaskPool::get().spawn(async move { build_lots(&lots) });
            commands.entity(root).insert(PendingBuildings(task));
        } else {
            let built = build_lots(&lots);
            spawn_built(&mut commands, root, built, &shapes, &surfaces, &mut meshes);
        }
    }
    *first_fill_laid = true;
}

/// Spawns every town's buildings whose build finished.
fn land_buildings(
    mut commands: Commands,
    mut pending: Query<(Entity, &mut PendingBuildings)>,
    (shapes, surfaces): (Option<Res<Shapes>>, Option<Res<FacadeMaterials>>),
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(shapes), Some(surfaces)) = (shapes, surfaces) else {
        return;
    };
    for (root, mut build) in &mut pending {
        if let Some(built) = check_ready(&mut build.0) {
            commands.entity(root).remove::<PendingBuildings>();
            spawn_built(&mut commands, root, built, &shapes, &surfaces, &mut meshes);
        }
    }
}

/// Every mesh of `town` but its buildings, under the builder's root.
fn build_town(b: &mut Builder, town: &TownLayout) {
    for street in &town.streets {
        let mat = b.palette.street.clone();
        flat_patch(b, &mat, street, 0.03);
    }
    plazas(b, town);
    railway(b, town);
    b.range = Some(ranges()[0].clone());
    for building in &town.buildings {
        if building.kind == BuildingKind::Platform {
            platform(b, building);
        }
    }
    for floor in town.floors() {
        floor_ramps(b, &floor);
    }
    b.range = Some(ranges()[1].clone());
    hitching_rail(b, &town.hitching_post.rail);
    b.range = None;
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
/// long side. The tethered horse itself is a 3D character figure.
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
    b.range = Some(ranges()[1].clone());
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
    b.range = None;
}

/// Every plaza tree: a trunk and a round crown.
fn plaza_trees(b: &mut Builder, town: &TownLayout) {
    b.range = Some(ranges()[2].clone());
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
    b.range = None;
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

/// The station platform: a plain concrete floor, its top at the height
/// core walks people on.
fn platform(b: &mut Builder, building: &Building) {
    let concrete = b.palette.concrete.clone();
    let (c, size, h) = (
        building.footprint.center(),
        building.footprint.size(),
        building.height,
    );
    let y = b.ground(c);
    b.cube(
        &concrete,
        world_from_ground(c, y + (h - 0.2) / 2.0),
        Vec3::new(size.x, h + 0.2, size.y),
    );
}

/// How far in from the street grid's edge a house counts as on the
/// outskirts, meters: about one block.
const OUTSKIRTS: f32 = 50.0;

/// A building of the town to build, with where it stands.
#[derive(Debug, Clone, Copy)]
struct Job {
    index: usize,
    lot: Lot,
    /// Its footprint's middle on the ground, in the world.
    base: Vec3,
}

/// Every building of `town` the facade kit builds (the platform apart),
/// placed on `terrain`.
fn lots(town: &TownLayout, terrain: &Terrain) -> Vec<Job> {
    let grid = town.grid();
    let store = StoreStyle::of(town.size);
    town.buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind != BuildingKind::Platform)
        .map(|(index, &building)| {
            let c = building.footprint.center();
            let edge = (c.x - grid.min.x)
                .min(grid.max.x - c.x)
                .min(c.y - grid.min.y)
                .min(grid.max.y - c.y);
            Job {
                index,
                lot: Lot {
                    building,
                    variant: variant(index, &building.footprint),
                    outskirts: edge < OUTSKIRTS,
                    store,
                    veranda: town.hitching_post.veranda,
                },
                base: world_from_ground(c, terrain.height(c.x, c.y)),
            }
        })
        .collect()
}

/// A well-mixed seed from a building's index and footprint.
fn variant(index: usize, footprint: &Aabb) -> u32 {
    let mut v = (index as u32).wrapping_mul(0x9e37_79b9)
        ^ (footprint.min.x.to_bits()).wrapping_mul(0x85eb_ca6b)
        ^ (footprint.min.y.to_bits()).wrapping_mul(0xc2b2_ae35);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^ (v >> 16)
}

/// One built building: its meshes in its own space, where it stands, the
/// index of its shared range, and a windmill's wheel.
struct Built {
    index: usize,
    transform: Transform,
    meshes: Vec<(Surface, Mesh)>,
    range: usize,
    rotor: Option<(Transform, WindmillRotor)>,
}

/// Builds every job's meshes.
fn build_lots(jobs: &[Job]) -> Vec<Built> {
    jobs.iter()
        .filter_map(|job| {
            let shape = town_buildings::building(&job.lot)?;
            let kind = job.lot.building.kind;
            let transform = Transform::from_translation(job.base)
                .with_rotation(town_buildings::yaw(job.lot.building.front));
            let rotor = (kind == BuildingKind::Windmill).then(|| {
                let (hub, yaw) = town_buildings::rotor_place(&job.lot);
                let speed = 1.5 + hash2(job.index as f32, 7.7);
                (
                    Transform::from_translation(transform.transform_point(hub))
                        .with_rotation(transform.rotation * yaw),
                    WindmillRotor { speed },
                )
            });
            // Small things go before the houses do.
            let range = match kind {
                BuildingKind::Windmill | BuildingKind::WaterTank => 1,
                _ => 0,
            };
            Some(Built {
                index: job.index,
                transform,
                meshes: shape.into_meshes(),
                range,
                rotor,
            })
        })
        .collect()
}

/// Spawns `built` under `root`: one entity per building and surface.
fn spawn_built(
    commands: &mut Commands,
    root: Entity,
    built: Vec<Built>,
    shapes: &Shapes,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let ranges = ranges();
    for b in built {
        let range = &ranges[b.range];
        for (surface, mesh) in b.meshes {
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(surfaces.get(surface)),
                b.transform,
                range.clone(),
                TownBuilding(b.index),
                ChildOf(root),
            ));
        }
        if let Some((transform, rotor)) = b.rotor {
            let wheel = commands
                .spawn((
                    Name::new("Windmill wheel"),
                    rotor,
                    transform,
                    Visibility::default(),
                    TownBuilding(b.index),
                    ChildOf(root),
                ))
                .id();
            for (surface, mesh) in &shapes.rotor {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(surfaces.get(*surface)),
                    Transform::IDENTITY,
                    range.clone(),
                    TownBuilding(b.index),
                    ChildOf(wheel),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

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

    fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), F>();
        q.iter(app.world()).count()
    }

    /// Updates until no town's buildings are still being built.
    fn settle(app: &mut App) {
        for _ in 0..2000 {
            if count::<With<PendingBuildings>>(app) == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }
        panic!("the town's buildings never landed");
    }

    #[test]
    fn only_towns_near_the_focus_are_built_and_far_ones_are_dropped() {
        let mut app = app();
        app.update();
        assert_eq!(roots(&mut app), vec![0]);
        // The first town is laid at once, buildings and all.
        assert_eq!(count::<With<PendingBuildings>>(&mut app), 0);
        let castex = count::<With<Mesh3d>>(&mut app);
        assert!(castex > 100, "{castex} meshes");
        assert!(count::<With<TownBuilding>>(&mut app) > 100);

        // Ride to the far town: Castex goes, the far town comes, its
        // buildings built off the main thread and landing a little later.
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(2950.0, -1950.0);
        app.update();
        assert_eq!(roots(&mut app), vec![1]);
        assert!(app.world().resource::<ActiveTowns>().0.is_active(1));
        settle(&mut app);
        // Same layout shifted, other seeds per building: about as many
        // meshes, none of them left over from Castex.
        let far = count::<With<Mesh3d>>(&mut app);
        assert!(far.abs_diff(castex) < castex / 5, "{far} vs {castex}");
        assert!(count::<With<TownBuilding>>(&mut app) > 100);

        // Halfway between them: nothing is drawn.
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(1500.0, -1000.0);
        app.update();
        assert!(roots(&mut app).is_empty());
        assert_eq!(count::<With<Mesh3d>>(&mut app), 0);
    }

    #[test]
    fn a_town_dropped_while_building_lands_nothing() {
        let mut app = app();
        app.update();
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(2950.0, -1950.0);
        app.update();
        assert_eq!(count::<With<PendingBuildings>>(&mut app), 1);
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(1500.0, -1000.0);
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }
        assert_eq!(count::<With<Mesh3d>>(&mut app), 0);
    }

    #[test]
    fn each_building_is_one_mesh_per_surface_with_the_shared_materials() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let shared: HashSet<AssetId<StandardMaterial>> =
            Surface::ALL.iter().map(|&s| facade.get(s).id()).collect();
        let mut q = app
            .world_mut()
            .query::<(&TownBuilding, &MeshMaterial3d<StandardMaterial>, &ChildOf)>();
        let wheels: HashSet<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<WindmillRotor>>()
            .iter(app.world())
            .collect();
        let mut per: Vec<Vec<AssetId<StandardMaterial>>> = vec![Vec::new(); town.buildings.len()];
        for (b, m, parent) in q.iter(app.world()) {
            // A windmill's wheel is its own mesh per surface.
            if !wheels.contains(&parent.parent()) {
                per[b.0].push(m.id());
            }
        }
        for (k, (building, materials)) in town.buildings.iter().zip(&per).enumerate() {
            if building.kind == BuildingKind::Platform {
                assert!(materials.is_empty());
                continue;
            }
            assert!(!materials.is_empty(), "building {k} ({:?})", building.kind);
            assert!(materials.iter().all(|m| shared.contains(m)));
            let distinct: HashSet<_> = materials.iter().collect();
            assert_eq!(
                distinct.len(),
                materials.len(),
                "building {k}: one mesh per surface"
            );
        }
    }

    #[test]
    fn towns_draw_within_the_shared_ranges_only() {
        let mut app = app();
        app.update();
        let mut q = app
            .world_mut()
            .query::<(&VisibilityRange, Has<TownBuilding>, Has<Mesh3d>)>();
        let mut seen = HashSet::new();
        let mut buildings = 0;
        for (range, building, mesh) in q.iter(app.world()) {
            assert!(ranges().contains(range), "a range of its own");
            seen.insert(range.clone());
            buildings += usize::from(building && mesh);
        }
        // Bevy never frees a range's GPU slot: towns add none.
        assert!(seen.len() <= ranges().len());
        // Every building mesh is ranged, the houses at BUILDING_FAR.
        assert_eq!(
            buildings,
            count::<(With<TownBuilding>, With<Mesh3d>)>(&mut app)
        );
        assert!(seen.contains(&ranges()[0]));
    }
}
