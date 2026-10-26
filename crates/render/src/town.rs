//! Procedural low-poly town drawn from a `pampa-core` [`TownLayout`]:
//! dirt streets with brick sidewalks where core paves them
//! ([`TownLayout::sidewalks`]), a tree-lined plaza (its paths, monument, benches and
//! railing from [`crate::facade::plaza`], its paraísos, olmos, eucaliptos or
//! aromos from [`crate::tree`]), the 1919 Italianate buildings of the
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
//! far past [`crate::settlement::BUILDING_FAR`] (the activation margin is wider). The
//! plazas' paths, monuments, benches and railings are built the same way. Buildings,
//! sidewalks, small props and trees are drawn within the settlements' shared
//! [`ranges`]: no new [`VisibilityRange`].
//!
//! A sidewalk is a strip of brick paving [`SIDEWALK_TOP`] over the ground
//! with a stucco curb [`CURB_TOP`] over it on the roadway side: drawn on the
//! ground like the streets and the plaza paths, which core walks people over
//! at the ground's own height.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use pampa_core::town::{PlazaFurniture, Sidewalk};
use pampa_core::{
    Aabb, Building, BuildingKind, Floor, RailSpan, Side, StoreStyle, Terrain, TownActivation,
    TownLayout, Vec2 as GroundVec2,
};

use crate::coords::world_from_ground;
use crate::facade::shape::Shape;
use crate::facade::town_buildings::{self, Lot};
use crate::facade::{FacadeMaterials, Surface};
use crate::flora::ThinOccluder;
use crate::rail::{TrackKit, track_pieces};
use crate::settlement::{props, ranges};
use crate::terrain::{StreamFocus, WorldTerrain, hash2};
use crate::tree::{TreeKit, VARIANTS, plaza_species};

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
        crate::tree::ensure_plugin(app);
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

/// One drawn piece of plaza `.0` of its town (an index into
/// [`TownLayout::plazas`]): its paths and monument, or its benches and
/// railing.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownPlaza(pub usize);

/// One piece of a town's sidewalks: a block's or the plaza's ring.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownSidewalk;

/// Shared meshes, scaled per instance.
#[derive(Resource)]
struct Shapes {
    cube: Handle<Mesh>,
    /// The windmill's wheel, one mesh per surface (the settlements' molino
    /// wheel).
    rotor: Vec<(Surface, Handle<Mesh>)>,
}

/// Earthy palette of the streets, plaza, railway and platform.
#[derive(Resource)]
struct Palette {
    timber: Handle<StandardMaterial>,
    street: Handle<StandardMaterial>,
    plaza: Handle<StandardMaterial>,
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
        let timber = matte(0.34, 0.25, 0.18);
        let street = matte(0.58, 0.47, 0.34);
        let plaza = matte(0.47, 0.46, 0.29);
        let concrete = matte(0.62, 0.59, 0.54);
        let ballast = matte(0.48, 0.45, 0.41);
        let steel = materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.34, 0.33),
            metallic: 0.8,
            perceptual_roughness: 0.55,
            ..default()
        });
        Self {
            timber,
            street,
            plaza,
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
    trees: &'a TreeKit,
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
    commands.insert_resource(Shapes { cube, rotor });
    commands.insert_resource(palette);
}

/// Shared town meshes and materials, once [`setup_kit`], the facade kit
/// and the tree kit ran.
type TownKit<'a> = (
    Option<Res<'a, Shapes>>,
    Option<Res<'a, Palette>>,
    Option<Res<'a, FacadeMaterials>>,
    Option<Res<'a, TreeKit>>,
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
    (shapes, palette, surfaces, trees): TownKit,
    roots: Query<(Entity, &TownRoot)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut first_fill_laid: Local<bool>,
) {
    let (Some(shapes), Some(palette), Some(surfaces), Some(trees)) =
        (shapes, palette, surfaces, trees)
    else {
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
            trees: &trees,
            terrain: &terrain.0,
            root,
            range: None,
        };
        build_town(&mut b, town);
        spawn_sidewalks(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        let lots = lots(town, &terrain.0);
        let plazas = plaza_jobs(town, &terrain.0);
        if *first_fill_laid {
            let task = AsyncComputeTaskPool::get().spawn(async move { build_lots(&lots, &plazas) });
            commands.entity(root).insert(PendingBuildings(task));
        } else {
            let built = build_lots(&lots, &plazas);
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

/// Height of a sidewalk's brick paving over the ground, meters: just over
/// the street's 0.03 and under the plaza paths' 0.07.
pub const SIDEWALK_TOP: f32 = 0.06;
/// Height of its curb over the ground, meters: 3 cm over the paving.
pub const CURB_TOP: f32 = 0.09;
/// Width of a curb, meters.
const CURB_WIDTH: f32 = 0.15;
/// How far sidewalks reach under the ground, meters.
const SIDEWALK_FOOT: f32 = -0.05;

/// Spawns the sidewalks of `town` under `root`: one entity per group of
/// touching strips and surface (paving, curb), within the buildings'
/// shared range.
fn spawn_sidewalks(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[0].clone();
    for (origin, shape) in sidewalk_shapes(town.sidewalks(), terrain) {
        for (surface, mesh) in shape.into_meshes() {
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(surfaces.get(surface)),
                Transform::from_translation(origin),
                range.clone(),
                TownSidewalk,
                ChildOf(root),
            ));
        }
    }
}

/// The groups of touching `walks` (a ring's four strips), each built around its first strip's middle on the ground: where
/// it stands in the world, and its shape there.
fn sidewalk_shapes(walks: &[Sidewalk], terrain: &Terrain) -> Vec<(Vec3, Shape)> {
    let mut groups: Vec<(Vec<Aabb>, Vec3, Shape)> = Vec::new();
    for walk in walks {
        let touches = |areas: &Vec<Aabb>| {
            areas
                .iter()
                .any(|a| a.expanded(0.01).intersects(&walk.area))
        };
        let mut touching = (0..groups.len()).filter(|&k| touches(&groups[k].0));
        let k = match touching.next() {
            Some(k) => {
                // A strip joining two groups (a ring's east strip, between
                // its north and south ones) merges them.
                let rest: Vec<usize> = touching.collect();
                for j in rest.into_iter().rev() {
                    let (areas, origin, shape) = groups.remove(j);
                    let into = &mut groups[k];
                    into.0.extend(areas);
                    into.2
                        .merge(shape, Transform::from_translation(origin - into.1));
                }
                k
            }
            None => {
                let c = walk.area.center();
                let origin = world_from_ground(c, terrain.height(c.x, c.y));
                groups.push((Vec::new(), origin, Shape::default()));
                groups.len() - 1
            }
        };
        let (areas, origin, shape) = &mut groups[k];
        areas.push(walk.area);
        let c = walk.area.center();
        let ground = terrain.height(c.x, c.y) - origin.y;
        sidewalk(shape, walk, *origin, ground);
    }
    groups
        .into_iter()
        .map(|(_, origin, shape)| (origin, shape))
        .collect()
}

/// The box between ground corners `a` and `b`, from `low` to `high` over
/// `ground`, in the space of a group standing at `origin`.
fn ground_box(
    shape: &mut Shape,
    surface: Surface,
    (a, b): (GroundVec2, GroundVec2),
    origin: Vec3,
    (ground, low, high): (f32, f32, f32),
) {
    let (p, q) = (
        world_from_ground(a, 0.0) - origin,
        world_from_ground(b, 0.0) - origin,
    );
    shape.block(
        surface,
        p.min(q).with_y(ground + low),
        p.max(q).with_y(ground + high),
    );
}

/// One strip: brick paving, and a curb along each of its curbed edges
/// (the first one whole, the others stopping short of it).
fn sidewalk(shape: &mut Shape, walk: &Sidewalk, origin: Vec3, ground: f32) {
    let w = CURB_WIDTH;
    let a = walk.area;
    // The paving stops at the curbs.
    let (mut lo, mut hi) = (a.min, a.max);
    for side in &walk.curbs {
        match side {
            Side::South => lo.y += w,
            Side::North => hi.y -= w,
            Side::West => lo.x += w,
            Side::East => hi.x -= w,
        }
    }
    let paving = (ground, SIDEWALK_FOOT, SIDEWALK_TOP);
    ground_box(shape, Surface::Paving, (lo, hi), origin, paving);
    let first = walk.curbs.first().copied();
    for (k, &side) in walk.curbs.iter().enumerate() {
        let (mut lo, mut hi) = (a.min, a.max);
        match side {
            Side::South => hi.y = lo.y + w,
            Side::North => lo.y = hi.y - w,
            Side::West => hi.x = lo.x + w,
            Side::East => lo.x = hi.x - w,
        }
        if k > 0 {
            match first {
                Some(Side::South) => lo.y = lo.y.max(a.min.y + w),
                Some(Side::North) => hi.y = hi.y.min(a.max.y - w),
                Some(Side::West) => lo.x = lo.x.max(a.min.x + w),
                Some(Side::East) => hi.x = hi.x.min(a.max.x - w),
                None => {}
            }
        }
        let curb = (ground, SIDEWALK_FOOT, CURB_TOP);
        ground_box(shape, Surface::Stucco, (lo, hi), origin, curb);
    }
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

/// One plaza's lawn. Its paths, monument, benches and railing are built
/// with the buildings ([`plaza_jobs`], [`crate::facade::plaza`]).
fn plaza(b: &mut Builder, p: Aabb) {
    let lawn = b.palette.plaza.clone();
    flat_patch(b, &lawn, &p, crate::facade::plaza::LAWN_TOP);
}

/// Every plaza tree: a variant of its plaza's species ([`plaza_species`])
/// from the shared [`TreeKit`], its bark and its leaves, turned and scaled
/// a little per tree. Each fades out of the way of the camera like a
/// settlement's tree ([`ThinOccluder`]).
fn plaza_trees(b: &mut Builder, town: &TownLayout) {
    for (plaza, tree) in plaza_tree_sites(town) {
        let kit = b.trees.species(plaza_species(town.seed, plaza));
        let (bark, leaves) = (kit.bark.clone(), kit.leaves.clone());
        let ground = b.ground(tree.at);
        let variant = &kit.variants[tree.variant];
        let transform = Transform::from_translation(world_from_ground(tree.at, ground - TREE_SINK))
            .with_rotation(Quat::from_rotation_y(tree.yaw))
            .with_scale(Vec3::splat(tree.scale));
        let fade = ThinOccluder::post(
            tree.at,
            variant.crown_radius * tree.scale,
            b.terrain,
            variant.height * tree.scale,
        );
        for (mesh, material) in [(&variant.bark, &bark), (&variant.leaves, &leaves)] {
            b.commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                transform,
                ranges()[2].clone(),
                fade,
                TownTree,
                ChildOf(b.root),
            ));
        }
    }
}

/// One of a town's drawn plaza trees (its bark or its leaves).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownTree;

/// Where and how one plaza tree stands.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TreeSite {
    at: GroundVec2,
    variant: usize,
    yaw: f32,
    /// Uniform scale, from [`TREE_SCALE_MIN`] to [`TREE_SCALE_MAX`].
    scale: f32,
}

/// The largest a plaza tree is drawn over its variant: its trunk stays
/// within core's [`pampa_core::town::TREE_TRUNK_RADIUS`].
pub(crate) const TREE_SCALE_MAX: f32 = 1.08;

/// The smallest a plaza tree is drawn over its variant: each tree's scale
/// lies between this and [`TREE_SCALE_MAX`], seeded by its spot.
const TREE_SCALE_MIN: f32 = 0.92;

/// How far a plaza tree's foot sits under the ground at its spot, meters:
/// the trunk's flat bottom stays buried where the lawn slopes a little.
const TREE_SINK: f32 = 0.05;

/// Each of `town`'s trees with the index of the plaza it stands in
/// ([`TownLayout::plazas`], [`plaza_of`]), its variant, yaw and scale,
/// seeded by its spot.
fn plaza_tree_sites(town: &TownLayout) -> Vec<(usize, TreeSite)> {
    let plazas: Vec<Aabb> = town.plazas().collect();
    town.trees
        .iter()
        .map(|&at| {
            let plaza = plaza_of(&plazas, at);
            let (a, b) = (hash2(at.x, at.y), hash2(at.y + 7.0, at.x));
            let site = TreeSite {
                at,
                variant: (a * VARIANTS as f32) as usize % VARIANTS,
                yaw: b * std::f32::consts::TAU,
                scale: TREE_SCALE_MAX
                    - (TREE_SCALE_MAX - TREE_SCALE_MIN) * hash2(at.x + 3.0, at.y - 5.0),
            };
            (plaza, site)
        })
        .collect()
}

/// The index of the plaza of `plazas` that tree `at` stands in. Core plants
/// every tree inside a plaza; one outside them all (after a layout change)
/// is filed under the nearest, with a warning, rather than quietly under
/// the main plaza.
fn plaza_of(plazas: &[Aabb], at: GroundVec2) -> usize {
    if let Some(inside) = plazas.iter().position(|p| p.contains_point(at)) {
        return inside;
    }
    let away = |p: &Aabb| p.closest_point(at).distance(at);
    let nearest = (0..plazas.len())
        .min_by(|&a, &b| away(&plazas[a]).total_cmp(&away(&plazas[b])))
        .unwrap_or(0);
    warn!(
        "plaza tree at ({:.1}, {:.1}) stands in no plaza; drawn as plaza {nearest}'s",
        at.x, at.y
    );
    nearest
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

/// What a built piece of a town is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// Building `.0` of [`TownLayout::buildings`].
    Building(usize),
    /// Plaza `.0` of [`TownLayout::plazas`].
    Plaza(usize),
}

/// One plaza of the town to build: what core puts in it, and its middle
/// on the ground in the world.
#[derive(Debug, Clone)]
struct PlazaJob {
    index: usize,
    furniture: PlazaFurniture,
    base: Vec3,
}

/// Every plaza of `town`, placed on `terrain`.
fn plaza_jobs(town: &TownLayout, terrain: &Terrain) -> Vec<PlazaJob> {
    town.plaza_furniture()
        .into_iter()
        .enumerate()
        .map(|(index, furniture)| {
            let c = furniture.area.center();
            PlazaJob {
                index,
                base: world_from_ground(c, terrain.height(c.x, c.y)),
                furniture,
            }
        })
        .collect()
}

/// One built piece (a building or a plaza's share of one range): its
/// meshes in its own space, where it stands, the index of its shared
/// range, and a windmill's wheel.
struct Built {
    part: Part,
    transform: Transform,
    meshes: Vec<(Surface, Mesh)>,
    range: usize,
    rotor: Option<(Transform, WindmillRotor)>,
}

/// Builds every job's meshes, then every plaza's: its paths, curbs and
/// monument within the buildings' range, its benches and railing within
/// the props'.
fn build_lots(jobs: &[Job], plazas: &[PlazaJob]) -> Vec<Built> {
    let plazas = plazas.iter().flat_map(|job| {
        let shapes = crate::facade::plaza::plaza(&job.furniture);
        let transform = Transform::from_translation(job.base);
        [(shapes.ground, 0), (shapes.furniture, 1)]
            .into_iter()
            .map(move |(shape, range)| Built {
                part: Part::Plaza(job.index),
                transform,
                meshes: shape.into_meshes(),
                range,
                rotor: None,
            })
    });
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
                part: Part::Building(job.index),
                transform,
                meshes: shape.into_meshes(),
                range,
                rotor,
            })
        })
        .chain(plazas)
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
            let mut e = commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(surfaces.get(surface)),
                b.transform,
                range.clone(),
                ChildOf(root),
            ));
            match b.part {
                Part::Building(index) => e.insert(TownBuilding(index)),
                Part::Plaza(index) => e.insert(TownPlaza(index)),
            };
        }
        let (Some((transform, rotor)), Part::Building(index)) = (b.rotor, b.part) else {
            continue;
        };
        {
            let wheel = commands
                .spawn((
                    Name::new("Windmill wheel"),
                    rotor,
                    transform,
                    Visibility::default(),
                    TownBuilding(index),
                    ChildOf(root),
                ))
                .id();
            for (surface, mesh) in &shapes.rotor {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(surfaces.get(*surface)),
                    Transform::IDENTITY,
                    range.clone(),
                    TownBuilding(index),
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
    fn each_plaza_is_one_mesh_per_surface_and_range_with_the_shared_materials() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let id = |s: Surface| facade.get(s).id();
        let mut q = app.world_mut().query::<(
            &TownPlaza,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
        )>();
        let ranges = ranges();
        let mut seen = HashSet::new();
        let (mut ground, mut furniture) = (HashSet::new(), HashSet::new());
        for (plaza, material, range) in q.iter(app.world()) {
            let r = ranges
                .iter()
                .position(|x| x == range)
                .expect("a shared range");
            assert!(
                seen.insert((plaza.0, r, material.id())),
                "one mesh per surface"
            );
            match r {
                0 => ground.insert(material.id()),
                1 => furniture.insert(material.id()),
                _ => panic!("plazas draw within the buildings' and props' ranges"),
            };
        }
        // Castex is a small town: earth paths with brick curbs and a
        // stucco and whitewash monument as far as the buildings; iron and
        // timber benches and an iron railing as far as the props.
        let want = [
            Surface::Earth,
            Surface::Brick,
            Surface::Stucco,
            Surface::Whitewash,
        ];
        assert!(want.iter().all(|&s| ground.contains(&id(s))), "{ground:?}");
        assert!(!ground.contains(&id(Surface::Paving)));
        let want = [Surface::Planks, Surface::Steel];
        assert_eq!(furniture, want.iter().map(|&s| id(s)).collect());
    }

    #[test]
    fn plaza_trees_share_the_kit_and_fade_out_of_the_way() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let kit = app.world().resource::<crate::tree::TreeKit>().clone();
        let species = plaza_species(town.seed, 0);
        let own = kit.species(species);
        let meshes: HashSet<AssetId<Mesh>> = own
            .variants
            .iter()
            .flat_map(|v| [v.bark.id(), v.leaves.id()])
            .collect();
        let mut q = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
            &ThinOccluder,
            &Transform,
        ), With<TownTree>>();
        let mut drawn = 0;
        let mut used = HashSet::new();
        for (mesh, material, range, fade, at) in q.iter(app.world()) {
            drawn += 1;
            // Castex has one plaza: its species' shared meshes and
            // materials, as far as the trees reach.
            assert!(meshes.contains(&mesh.id()));
            assert!([own.bark.id(), own.leaves.id()].contains(&material.id()));
            assert!(range == &ranges()[2]);
            used.insert(mesh.id());
            // Round the trunk, as tall as the tree.
            let foot = Vec2::new(at.translation.x, -at.translation.z);
            assert_eq!((fade.a, fade.b), (foot, foot));
            assert!(
                fade.radius > 1.0 && fade.top - fade.bottom > 3.0,
                "{fade:?}"
            );
            assert!(at.scale.x <= TREE_SCALE_MAX && at.scale.x >= TREE_SCALE_MIN);
        }
        // A bark and a leaf mesh per tree, the variants shared.
        assert_eq!(drawn, 2 * town.trees.len());
        assert!(town.trees.len() > 2 * VARIANTS);
        assert_eq!(
            used.len(),
            2 * VARIANTS,
            "every variant drawn, none rebuilt"
        );
    }

    #[test]
    fn a_town_with_a_station_plaza_plants_another_species_there() {
        // General Pico as the preview lays it: a city with a station plaza.
        let town = TownLayout::generate(&pampa_core::TownConfig {
            name: "General Pico".into(),
            origin: Vec2::ZERO,
            blocks: (8, 8),
            block_size: 40.0,
            railway: Some(pampa_core::RailwayConfig {
                side: pampa_core::Side::North,
                offset: 24.0,
                through: true,
            }),
            seed: 13,
            ..TownConfig::castex()
        })
        .unwrap();
        let sites = plaza_tree_sites(&town);
        let plazas: HashSet<usize> = sites.iter().map(|(p, _)| *p).collect();
        assert_eq!(plazas, HashSet::from([0, 1]), "trees in both plazas");
        for (plaza, site) in &sites {
            let area = town.plazas().nth(*plaza).unwrap();
            assert!(area.contains_point(site.at));
        }
        assert_ne!(plaza_species(town.seed, 0), plaza_species(town.seed, 1));
    }

    #[test]
    fn every_tree_stands_in_exactly_one_plaza_and_is_filed_under_it() {
        // Every town shape the shipped towns take (3 to 8 blocks a side,
        // with and without a railway, a through line making a station
        // plaza) over a few seeds: core plants each tree in one plaza, so
        // a layout change that moves one out fails here, not quietly.
        let mut towns = 0;
        for blocks in [(3, 3), (4, 3), (4, 4), (4, 5), (5, 4), (8, 8)] {
            for seed in [1, 2, 13, 19] {
                for railway in [None, Some(false), Some(true)] {
                    let config = TownConfig {
                        blocks,
                        seed,
                        railway: railway.map(|through| pampa_core::RailwayConfig {
                            side: pampa_core::Side::North,
                            offset: 24.0,
                            through,
                        }),
                        ..TownConfig::castex()
                    };
                    let Ok(town) = TownLayout::generate(&config) else {
                        continue;
                    };
                    towns += 1;
                    let plazas: Vec<Aabb> = town.plazas().collect();
                    for (plaza, site) in plaza_tree_sites(&town) {
                        let holding: Vec<usize> = (0..plazas.len())
                            .filter(|&i| plazas[i].contains_point(site.at))
                            .collect();
                        assert_eq!(holding, [plaza], "{blocks:?} seed {seed} {:?}", site.at);
                    }
                }
            }
        }
        assert!(towns > 50, "{towns} towns generated");
    }

    #[test]
    fn a_tree_outside_every_plaza_goes_to_the_nearest() {
        let plazas = [
            Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(40.0, 40.0)),
            Aabb::new(Vec2::new(100.0, 0.0), Vec2::new(140.0, 40.0)),
        ];
        assert_eq!(plaza_of(&plazas, Vec2::new(20.0, 20.0)), 0);
        assert_eq!(plaza_of(&plazas, Vec2::new(120.0, 40.0)), 1, "on the edge");
        assert_eq!(plaza_of(&plazas, Vec2::new(141.0, 20.0)), 1, "just outside");
        assert_eq!(plaza_of(&plazas, Vec2::new(60.0, -5.0)), 0);
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

    /// Writes three towns as shipped (General Pico, Eduardo Castex,
    /// Metileo) to `PAMPA_PREVIEW` (a folder) for the offline renderer, in
    /// the format of `facade::town_buildings::tests::preview_town_street`:
    /// buildings, streets, sidewalks, and the plazas (lawn, paths,
    /// monument, benches, railing and trees) on flat ground. Run
    /// with `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview_town`.
    #[test]
    #[ignore = "writes preview files"]
    fn preview_town_sidewalks() {
        use std::fmt::Write as _;
        let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let config = |name: &str, blocks, street_width, seed, through| pampa_core::TownConfig {
            name: name.into(),
            origin: Vec2::ZERO,
            blocks,
            block_size: 40.0,
            street_width,
            railway: Some(pampa_core::RailwayConfig {
                side: pampa_core::Side::North,
                offset: 24.0,
                through,
            }),
            plaza: pampa_core::PlazaPlacement::Auto,
            seed,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        let towns = [
            ("pico", config("General Pico", (8, 8), 10.0, 13, true)),
            ("castex", config("Eduardo Castex", (5, 4), 10.0, 2, false)),
            (
                "metileo",
                pampa_core::TownConfig {
                    density: Some(0.3),
                    ..config("Metileo", (3, 3), 8.0, 19, false)
                },
            ),
        ];
        for (name, config) in towns {
            let town = TownLayout::generate(&config).unwrap();
            let terrain = WorldTerrain::default().0;
            let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
            let floor = terrain.height(0.0, 0.0);
            let mut text = String::new();
            let put_part = |text: &mut String, shape: &Shape, surface: Surface, at: Transform| {
                for (p, n, uv) in shape.vertices(surface) {
                    let p = at.transform_point(p) - Vec3::Y * floor;
                    let n = at.rotation * n;
                    let _ = writeln!(
                        text,
                        "v {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} {:.4} {:.4}",
                        p.x, p.y, p.z, n.x, n.y, n.z, uv.x, uv.y
                    );
                }
                for t in shape.indices(surface).chunks(3) {
                    let _ = writeln!(text, "f {} {} {}", t[0], t[1], t[2]);
                }
            };
            let put = |text: &mut String, shape: &Shape, at: Transform| {
                for surface in shape.surfaces().collect::<Vec<_>>() {
                    let _ = writeln!(text, "m {surface:?}");
                    put_part(text, shape, surface, at);
                }
            };
            // Ground patches as named flat quads.
            let patch = |text: &mut String, name: &str, a: &Aabb, lift: f32| {
                let (p, q) = (
                    world_from_ground(a.min, lift),
                    world_from_ground(a.max, lift),
                );
                let _ = writeln!(text, "m {name}");
                for (x, z) in [(p.x, p.z), (q.x, p.z), (q.x, q.z), (p.x, q.z)] {
                    let _ = writeln!(text, "v {x:.3} {lift:.3} {z:.3} 0 1 0 0 0");
                }
                let _ = writeln!(text, "f 0 2 1\nf 0 3 2");
            };
            for street in &town.streets {
                patch(&mut text, "Street", street, 0.03);
            }
            for plaza in town.plazas() {
                patch(&mut text, "Plaza", &plaza, crate::facade::plaza::LAWN_TOP);
            }
            for job in plaza_jobs(&town, &terrain) {
                let shapes = crate::facade::plaza::plaza(&job.furniture);
                let at = Transform::from_translation(job.base);
                put(&mut text, &shapes.ground, at);
                put(&mut text, &shapes.furniture, at);
            }
            // Trees as the game draws them: a variant of the plaza's
            // species, turned and scaled.
            for (plaza, site) in plaza_tree_sites(&town) {
                let species = plaza_species(town.seed, plaza);
                let tree = crate::tree::mesh::tree(species, site.variant as u32);
                let at = Transform::from_translation(world_from_ground(
                    site.at,
                    terrain.height(site.at.x, site.at.y) - TREE_SINK,
                ))
                .with_rotation(Quat::from_rotation_y(site.yaw))
                .with_scale(Vec3::splat(site.scale));
                put_tree(&mut text, species, &tree, at, floor);
            }
            for (origin, shape) in sidewalk_shapes(town.sidewalks(), &terrain) {
                put(&mut text, &shape, Transform::from_translation(origin));
            }
            for job in lots(&town, &terrain) {
                let Some(shape) = town_buildings::building(&job.lot) else {
                    continue;
                };
                let at = Transform::from_translation(job.base)
                    .with_rotation(town_buildings::yaw(job.lot.building.front));
                put(&mut text, &shape, at);
            }
            std::fs::write(dir.join(format!("town_{name}.txt")), text).expect("preview written");
            // Where the plaza is, for framing the camera.
            let c = town.plaza.center();
            println!(
                "{name}: plaza at x {} z {}, grid {:?}",
                c.x,
                -c.y,
                town.grid()
            );
        }
    }

    /// Writes `tree` placed `at` to a preview `text`: its bark as
    /// `m Bark(<species>)`, its leaves as `m Leaves(<species>)`, `floor`
    /// lifted off the heights.
    fn put_tree(
        text: &mut String,
        species: crate::tree::Species,
        tree: &crate::tree::mesh::TreeMesh,
        at: Transform,
        floor: f32,
    ) {
        use std::fmt::Write as _;
        for (part, b) in [("Bark", &tree.bark), ("Leaves", &tree.leaves)] {
            let _ = writeln!(text, "m {part}({species:?})");
            for ((p, n), uv) in b.positions.iter().zip(&b.normals).zip(&b.uvs) {
                let (p, n) = (at.transform_point(*p) - Vec3::Y * floor, at.rotation * *n);
                let _ = writeln!(
                    text,
                    "v {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} {:.4} {:.4}",
                    p.x, p.y, p.z, n.x, n.y, n.z, uv[0], uv[1]
                );
            }
            for t in b.indices.chunks(3) {
                let _ = writeln!(text, "f {} {} {}", t[0], t[1], t[2]);
            }
        }
    }

    /// Writes every variant of the four species in a row, 14 m apart
    /// (paraíso, olmo, eucalipto, aromo from west to east, the variants
    /// north to south), to `PAMPA_PREVIEW/trees.txt` for the offline
    /// renderer. Run with
    /// `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview_tree_species`.
    #[test]
    #[ignore = "writes preview files"]
    fn preview_tree_species() {
        let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
            return;
        };
        let mut text = String::new();
        for (i, &species) in crate::tree::Species::ALL.iter().enumerate() {
            for v in 0..VARIANTS {
                let tree = crate::tree::mesh::tree(species, v as u32);
                let at = Transform::from_xyz(i as f32 * 14.0, 0.0, v as f32 * 14.0);
                put_tree(&mut text, species, &tree, at, 0.0);
            }
        }
        let path = std::path::PathBuf::from(dir).join("trees.txt");
        std::fs::write(&path, text).expect("preview written");
        println!("wrote {}", path.display());
    }

    #[test]
    fn a_small_town_paves_round_its_plaza_with_the_shared_paving_and_curb() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let shared = [
            facade.get(Surface::Paving).id(),
            facade.get(Surface::Stucco).id(),
        ];
        let mut q = app.world_mut().query_filtered::<
            (&MeshMaterial3d<StandardMaterial>, &VisibilityRange),
            With<TownSidewalk>,
        >();
        let walks: Vec<_> = q.iter(app.world()).collect();
        // Castex is small: only the plaza's ring, its paving and its curb.
        assert_eq!(walks.len(), 2, "sidewalk meshes");
        for (material, range) in walks {
            assert!(shared.contains(&material.id()));
            assert!(range == &ranges()[0], "a range of its own");
        }
    }

    /// Every sidewalk point of `town`, over the ground under it.
    fn sidewalk_heights(town: &TownLayout, terrain: &Terrain) -> Vec<(Surface, f32)> {
        sidewalk_shapes(town.sidewalks(), terrain)
            .into_iter()
            .flat_map(|(origin, shape)| {
                shape
                    .surfaces()
                    .flat_map(|s| {
                        shape.points(s).into_iter().map(move |p| {
                            let p = p + origin;
                            (s, p.y - terrain.height(p.x, -p.z))
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn sidewalks_stand_a_few_centimeters_proud_like_the_plaza_paths() {
        // People walk at the ground's own height over the streets (0.03 m),
        // the paths (0.07 m) and so the sidewalks: their paving and curb
        // stay within those few centimeters.
        let pico = TownLayout::generate(&pampa_core::TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        // The game levels the ground under a town.
        let terrain = WorldTerrain::default().0;
        let terrain = terrain.clone().with_flat_area(pico.flat_area(&terrain));
        let heights = sidewalk_heights(&pico, &terrain);
        let top = |surface| {
            heights
                .iter()
                .filter(|(s, _)| *s == surface)
                .fold(f32::MIN, |m, (_, y)| m.max(*y))
        };
        assert!((top(Surface::Paving) - SIDEWALK_TOP).abs() < 1e-3);
        assert!((top(Surface::Stucco) - CURB_TOP).abs() < 1e-3);
        const { assert!(CURB_TOP <= 0.1, "a curb people step over unawares") };
        for (surface, y) in heights {
            assert!(
                (SIDEWALK_FOOT - 1e-3..=CURB_TOP + 1e-3).contains(&y),
                "{surface:?} at {y}"
            );
        }
        // A ring per block but the plaza's four, and the plaza's own: one
        // shape each.
        assert_eq!(
            sidewalk_shapes(pico.sidewalks(), &terrain).len(),
            64 - 4 + 1
        );
    }
}
