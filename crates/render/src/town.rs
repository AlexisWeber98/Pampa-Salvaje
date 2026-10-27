//! Procedural low-poly town drawn from a `pampa-core` [`TownLayout`]:
//! dirt streets (one earth mesh with world-space UVs, plus the goods
//! yard's cart tracks) with sidewalks, brick in a city and round a town's
//! plaza and beaten earth blending into the grass elsewhere
//! ([`TownLayout::sidewalks`], [`pampa_core::town::Walk`]), the hitching rails of the store and the
//! palenques ([`crate::facade::palenque`]), a tree-lined plaza (its paths, monument, benches and
//! railing from [`crate::facade::plaza`], its paraísos, olmos, eucaliptos or
//! aromos from [`crate::tree`]), the 1919 Italianate buildings of the
//! facade kit ([`crate::facade::town_buildings`]), a railway with its
//! station, grain sheds, and windmills over round water tanks.
//!
//! Trees beyond the plaza rings ([`TownLayout::shade_trees`]) are the
//! kit's shared meshes too: a city's sidewalk trees are the street species'
//! trimmed variants in their cazuelas ([`tree_pit_shape`]), groves and yard
//! trees the plain ones; a town's front trees are trimmed and stand on the
//! dirt. Yard shrubs and flower beds ([`TownLayout::yard_plants`]) are one
//! small shared mesh each ([`crate::tree::plants`]).
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
//! The streets wear no wheel ruts: people walk and ride them. Cart tracks
//! are drawn only where carts go: from the street to the door of each goods
//! building of the station yard ([`goods_tracks`]; the estancias' huellas
//! are the settlements'). Streets and palenques are one mesh per surface
//! and town, within the buildings' and the props' ranges.
//!
//! A brick sidewalk is a strip of paving [`SIDEWALK_TOP`] over the ground
//! with a stucco curb [`CURB_TOP`] over it on the roadway side; an earth walk
//! has the same curb and, for a floor, a feathered decal of the trodden yard
//! dirt ([`TownEarthWalk`]) a few centimeters over the ground. Both are drawn on the
//! ground like the streets and the plaza paths, which core walks people over
//! at the ground's own height.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use pampa_core::town::{
    PlantKind, PlazaFurniture, ShadeKind, ShadeTree, Sidewalk, Walk, YardPlant,
};
use pampa_core::{
    Aabb, Building, BuildingKind, Floor, RailSpan, Side, StoreStyle, Terrain, TownActivation,
    TownLayout, Vec2 as GroundVec2,
};

use crate::coords::world_from_ground;
use crate::facade::palenque;
use crate::facade::shape::Shape;
use crate::facade::town_buildings::{self, Lot};
use crate::facade::{FacadeMaterials, Surface};
use crate::flora::ThinOccluder;
use crate::rail::{TrackKit, track_pieces};
use crate::settlement::{props, ranges};
use crate::terrain::{StreamFocus, WorldTerrain, hash2};
use crate::tree::{TreeKit, TreeVariant, VARIANTS, plaza_species, street_species, yard_species};

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

/// The beaten-earth walks of a town's blocks (and a village's): one feathered
/// decal ([`crate::settlement::track::walks_mesh`]); their curbs are sidewalk
/// meshes like the brick ones.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownEarthWalk;

/// The cazuelas of a city's street trees: one mesh of rim and earth.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownTreePit;

/// A town's dirt carriageway: its streets' earth, or the wheel ruts worn
/// into it (one mesh per surface).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownStreet;

/// A town's palenques, the store's hitching rail among them: one mesh in
/// timber for all of them.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownPalenque;

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
            concrete: self.concrete.clone(),
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
        spawn_streets(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_palenques(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_sidewalks(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_tree_pits(
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
    b.range = None;
}

/// Height of the dirt carriageway over the ground, meters: under the
/// sidewalks' [`SIDEWALK_TOP`]. Core walks people over it at the ground's
/// own height.
pub const STREET_TOP: f32 = 0.03;
/// How far the carriageway reaches under the ground, meters.
const STREET_FOOT: f32 = -0.05;
/// Height of the cart tracks over the dirt level, meters: a few
/// millimeters, so they never fight it.
pub const RUT_LIFT: f32 = 0.006;
/// Width of one wheel rut, meters.
const RUT_WIDTH: f32 = 0.25;
/// Distance between the two ruts of a cart's track, meters.
const RUT_GAUGE: f32 = 1.4;
/// Shortest stretch of rut worth drawing, meters.
const RUT_MIN: f32 = 1.0;
/// Where a goods track starts out of the facade, meters.
const RUT_START: f32 = 0.3;
/// Gap a goods track keeps from anything in its way, meters.
const RUT_GAP: f32 = 0.3;
/// How far from a goods building's door to look for the street its cart
/// track runs to, meters.
const GOODS_REACH: f32 = 12.0;
/// Length of the track out of a goods building with no street in reach:
/// the loading apron, meters.
const GOODS_APPROACH: f32 = 8.0;

/// Spawns the carriageway of `town` under `root`: one mesh of earth and
/// one of the goods yard's cart tracks, in the buildings' shared range,
/// with world-space UVs so the strips meet at the crossings without a seam.
fn spawn_streets(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[0].clone();
    let (origin, shape) = street_shape(town, terrain);
    for (surface, mesh) in shape.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::from_translation(origin),
            range.clone(),
            TownStreet,
            ChildOf(root),
        ));
    }
}

/// The streets of `town` as one shape built around `origin`, the middle of
/// its grid on the ground: every strip a slab of [`Surface::Earth`] topped
/// at [`STREET_TOP`], and the [`goods_tracks`] in [`Surface::Mud`]. The
/// streets themselves wear no ruts: people walk and ride them.
fn street_shape(town: &TownLayout, terrain: &Terrain) -> (Vec3, Shape) {
    let middle = town.grid().center();
    let origin = world_from_ground(middle, terrain.height(middle.x, middle.y));
    let mut shape = Shape::default();
    for street in &town.streets {
        let c = street.center();
        let ground = terrain.height(c.x, c.y) - origin.y;
        let earth = (ground, STREET_FOOT, STREET_TOP);
        ground_box(
            &mut shape,
            Surface::Earth,
            (street.min, street.max),
            origin,
            earth,
        );
    }
    for rut in goods_tracks(town) {
        let c = rut.center();
        let top = terrain.height(c.x, c.y) - origin.y + STREET_TOP + RUT_LIFT;
        let (p, q) = (
            world_from_ground(rut.min, 0.0) - origin,
            world_from_ground(rut.max, 0.0) - origin,
        );
        let (lo, hi) = (p.min(q), p.max(q));
        // Seen from above, counter-clockwise.
        shape.face(
            Surface::Mud,
            &[
                Vec3::new(lo.x, top, lo.z),
                Vec3::new(lo.x, top, hi.z),
                Vec3::new(hi.x, top, hi.z),
                Vec3::new(hi.x, top, lo.z),
            ],
        );
    }
    (origin, shape)
}

/// The unit ground vector pointing out of `side`.
fn side_vector(side: Side) -> GroundVec2 {
    match side {
        Side::North => GroundVec2::new(0.0, 1.0),
        Side::South => GroundVec2::new(0.0, -1.0),
        Side::East => GroundVec2::new(1.0, 0.0),
        Side::West => GroundVec2::new(-1.0, 0.0),
    }
}

/// Whether `b` is one of the railway's goods buildings: a shed or grain
/// store of the station yard, beside the line outside the grid or in a
/// through line's band, not one of the sheds among the houses.
fn is_goods_yard(town: &TownLayout, b: &Building) -> bool {
    matches!(b.kind, BuildingKind::Shed | BuildingKind::GrainStore)
        && (town
            .rail_band()
            .is_some_and(|band| band.contains(&b.footprint))
            || !town.grid().contains(&b.footprint))
}

/// How long the cart track out of a goods building's `door` facing `n` is:
/// up to a quarter meter short of the first of `streets` within
/// [`GOODS_REACH`], else [`GOODS_APPROACH`]. `None` when the street is so
/// close (or the door already stands on it, where the streets wear no ruts)
/// that no stretch of [`RUT_MIN`] fits: the street is the door's apron.
fn goods_track_length(streets: &[Aabb], door: GroundVec2, n: GroundVec2) -> Option<f32> {
    let to_street = (0..=(GOODS_REACH * 4.0) as usize)
        .map(|k| k as f32 * 0.25)
        .find(|d| streets.iter().any(|s| s.contains_point(door + n * *d)))
        .map(|d| d - 0.25);
    let length = to_street.unwrap_or(GOODS_APPROACH);
    (length - RUT_START >= RUT_MIN).then_some(length)
}

/// The cart tracks of `town`, on the ground: from the door of each goods
/// building ([`is_goods_yard`]) straight out along its front, two wheel
/// ruts a [`RUT_GAUGE`] apart, to the street if one lies within
/// [`GOODS_REACH`] and else [`GOODS_APPROACH`] out. They stop short of
/// whatever stands in the way (buildings, trees, the line, platforms,
/// plazas, sidewalks, signs and palenques).
fn goods_tracks(town: &TownLayout) -> Vec<Aabb> {
    let mut keep_off: Vec<Aabb> = town.obstacles();
    for floor in town.floors() {
        keep_off.push(floor.footprint);
        keep_off.extend(floor.ramps());
    }
    keep_off.extend(town.canopies().iter().map(|c| c.footprint));
    keep_off.extend(town.rail_corridor());
    keep_off.extend(town.plazas());
    keep_off.extend(town.sidewalks().iter().map(|w| w.area));
    let mut out = Vec::new();
    for b in town.buildings.iter().filter(|b| is_goods_yard(town, b)) {
        let n = side_vector(b.front);
        let across = GroundVec2::new(-n.y, n.x);
        let half = b.footprint.size() * 0.5;
        let door = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
        let Some(length) = goods_track_length(&town.streets, door, n) else {
            continue;
        };
        let own = b.collider();
        for wheel in [-1.0, 1.0] {
            let lane = across * (wheel * RUT_GAUGE / 2.0);
            // Where along the front each thing in the way cuts the rut.
            let mut pieces = vec![(RUT_START, length)];
            for o in keep_off.iter().filter(|o| **o != own) {
                let at = |p: GroundVec2| ((p - door - lane).dot(n), (p - door - lane).dot(across));
                let (a, z) = (at(o.min), at(o.max));
                let (d_lo, d_hi) = (a.0.min(z.0) - RUT_GAP, a.0.max(z.0) + RUT_GAP);
                let (c_lo, c_hi) = (a.1.min(z.1), a.1.max(z.1));
                if c_hi <= -RUT_WIDTH / 2.0 || c_lo >= RUT_WIDTH / 2.0 {
                    continue;
                }
                pieces = pieces
                    .into_iter()
                    .flat_map(|(from, to)| {
                        if d_hi <= from || d_lo >= to {
                            vec![(from, to)]
                        } else {
                            vec![(from, d_lo), (d_hi, to)]
                        }
                    })
                    .collect();
            }
            for (from, to) in pieces.into_iter().filter(|(f, t)| t - f >= RUT_MIN) {
                let (p, q) = (door + lane + n * from, door + lane + n * to);
                let reach = GroundVec2::new(across.x.abs(), across.y.abs()) * (RUT_WIDTH / 2.0);
                out.push(Aabb::new(
                    GroundVec2::new(p.x.min(q.x), p.y.min(q.y)) - reach,
                    GroundVec2::new(p.x.max(q.x), p.y.max(q.y)) + reach,
                ));
            }
        }
    }
    out
}

/// Spawns the palenques of `town` under `root`: one mesh in timber, within
/// the small props' range.
fn spawn_palenques(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[1].clone();
    let (origin, shape) = palenque_shape(town, terrain);
    for (surface, mesh) in shape.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::from_translation(origin),
            range.clone(),
            TownPalenque,
            ChildOf(root),
        ));
    }
}

/// The rails of `town` as one shape built around `origin`, the middle of
/// its grid on the ground: the general store's hitching rail and every
/// palenque, each along its footprint's long side.
fn palenque_shape(town: &TownLayout, terrain: &Terrain) -> (Vec3, Shape) {
    let middle = town.grid().center();
    let origin = world_from_ground(middle, terrain.height(middle.x, middle.y));
    let mut shape = Shape::default();
    let rails =
        std::iter::once(town.hitching_post.rail).chain(town.palenques.iter().map(|p| p.rail));
    for rail in rails {
        let size = rail.size();
        let along_x = size.x >= size.y;
        let c = rail.center();
        let at = world_from_ground(c, terrain.height(c.x, c.y)) - origin;
        // A rail along the ground's y runs along the world's z.
        let turn = if along_x {
            0.0
        } else {
            std::f32::consts::FRAC_PI_2
        };
        shape.merge(
            palenque::palenque(size.x.max(size.y)),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(turn)),
        );
    }
    (origin, shape)
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
    // The walkable floor of each ring of earth walks, inside its curbs.
    let rings: Vec<Vec<Aabb>> = town
        .sidewalks()
        .iter()
        .filter(|walk| walk.walk == Walk::Earth)
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|ring| ring.iter().map(|walk| walk_floor(walk)).collect())
        .collect();
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    if let Some(mesh) = crate::settlement::track::walks_mesh(&rings, &ground) {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(Surface::Yard)),
            Transform::default(),
            range.clone(),
            TownEarthWalk,
            ChildOf(root),
        ));
    }
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

/// How wide a cazuela's rim is, meters.
const PIT_RIM: f32 = 0.1;
/// How far a cazuela's earth stands over the paving, meters.
const PIT_EARTH_LIFT: f32 = 0.005;

/// Spawns the cazuelas of `town`'s street trees under `root`: one mesh of
/// curb-high rims and one of earth, in the sidewalks' range.
fn spawn_tree_pits(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[0].clone();
    let Some((origin, shape)) = tree_pit_shape(town, terrain) else {
        return;
    };
    for (surface, mesh) in shape.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::from_translation(origin),
            range.clone(),
            TownTreePit,
            ChildOf(root),
        ));
    }
}

/// The cazuelas of `town`'s [`ShadeKind::Pit`] trees as one shape around
/// `origin`, the middle of its grid on the ground: a square of stucco rim
/// (as high as the curb) round [`Surface::Earth`], on the paving. `None`
/// for a town without street trees.
fn tree_pit_shape(town: &TownLayout, terrain: &Terrain) -> Option<(Vec3, Shape)> {
    let pits: Vec<&ShadeTree> = town
        .shade_trees
        .iter()
        .filter(|t| t.kind == ShadeKind::Pit)
        .collect();
    if pits.is_empty() {
        return None;
    }
    let middle = town.grid().center();
    let origin = world_from_ground(middle, terrain.height(middle.x, middle.y));
    let mut shape = Shape::default();
    let (outer, inner) = (
        ShadeTree::PIT_SIDE / 2.0,
        ShadeTree::PIT_SIDE / 2.0 - PIT_RIM,
    );
    for tree in pits {
        let ground = terrain.height(tree.at.x, tree.at.y) - origin.y;
        let corner = |dx: f32, dy: f32| tree.at + GroundVec2::new(dx, dy);
        let rim = (ground, SIDEWALK_FOOT, CURB_TOP);
        // South and north rims whole, west and east ones between them.
        for (a, b) in [
            (corner(-outer, -outer), corner(outer, -inner)),
            (corner(-outer, inner), corner(outer, outer)),
            (corner(-outer, -inner), corner(-inner, inner)),
            (corner(inner, -inner), corner(outer, inner)),
        ] {
            ground_box(&mut shape, Surface::Stucco, (a, b), origin, rim);
        }
        let earth = (ground, SIDEWALK_FOOT, SIDEWALK_TOP + PIT_EARTH_LIFT);
        let soil = (corner(-inner, -inner), corner(inner, inner));
        ground_box(&mut shape, Surface::Earth, soil, origin, earth);
    }
    Some((origin, shape))
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

/// The floor of `walk`: its area less the curb along each curbed edge.
fn walk_floor(walk: &Sidewalk) -> Aabb {
    let (mut lo, mut hi) = (walk.area.min, walk.area.max);
    for side in &walk.curbs {
        match side {
            Side::South => lo.y += CURB_WIDTH,
            Side::North => hi.y -= CURB_WIDTH,
            Side::West => lo.x += CURB_WIDTH,
            Side::East => hi.x -= CURB_WIDTH,
        }
    }
    Aabb::new(lo, hi)
}

/// One strip: brick paving (none on an earth walk), and a curb along each of
/// its curbed edges
/// (the first one whole, the others stopping short of it).
fn sidewalk(shape: &mut Shape, walk: &Sidewalk, origin: Vec3, ground: f32) {
    let w = CURB_WIDTH;
    let a = walk.area;
    // The brick paving stops at the curbs; an earth walk has none (its
    // floor is a decal, [`walk_floor`]).
    if walk.walk == Walk::Brick {
        let floor = walk_floor(walk);
        let paving = (ground, SIDEWALK_FOOT, SIDEWALK_TOP);
        ground_box(
            shape,
            Surface::Paving,
            (floor.min, floor.max),
            origin,
            paving,
        );
    }
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
/// trees of all of them, and the street, grove and yard trees.
fn plazas(b: &mut Builder, town: &TownLayout) {
    for p in town.plazas() {
        plaza(b, p);
    }
    plaza_trees(b, town);
    shade_trees(b, town);
    yard_plants(b, town);
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
        spawn_tree(b, (variant, &bark, &leaves), transform, fade, TownTree);
    }
}

/// A tree's bark and its leaves, two entities sharing the kit's meshes and
/// materials, each fading out of the camera's way and tagged `marker`.
fn spawn_tree(
    b: &mut Builder,
    (variant, bark, leaves): (
        &TreeVariant,
        &Handle<StandardMaterial>,
        &Handle<StandardMaterial>,
    ),
    transform: Transform,
    fade: ThinOccluder,
    marker: impl Bundle + Copy,
) {
    for (mesh, material) in [(&variant.bark, bark), (&variant.leaves, leaves)] {
        b.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            transform,
            ranges()[2].clone(),
            fade,
            marker,
            ChildOf(b.root),
        ));
    }
}

/// Every street, grove and yard tree ([`TownLayout::shade_trees`]) from the
/// shared [`TreeKit`]: a street tree is one of the street species' trimmed
/// variants (a bare trunk to about 4 m, a narrow crown leaning over the
/// street), a grove tree a variant of its plaza's species, a yard tree any
/// species, scaled down to fit its crown's room.
fn shade_trees(b: &mut Builder, town: &TownLayout) {
    let plazas: Vec<Aabb> = town.plazas().collect();
    for tree in &town.shade_trees {
        let look = shade_look(town.seed, &plazas, tree);
        let kit = b.trees.species(look.species);
        let (bark, leaves) = (kit.bark.clone(), kit.leaves.clone());
        let variants = if look.trimmed {
            &kit.pruned
        } else {
            &kit.variants
        };
        let variant = &variants[look.variant];
        let transform = shade_transform(tree, &look, variant.crown_radius, b.ground(tree.at));
        let fade = ThinOccluder::post(
            tree.at,
            variant.crown_radius * transform.scale.x,
            b.terrain,
            variant.height * transform.scale.y,
        );
        spawn_tree(b, (variant, &bark, &leaves), transform, fade, TownShadeTree);
    }
}

/// Every yard shrub and flower bed ([`TownLayout::yard_plants`]): a shared
/// plant mesh turned a little, in the small props' range.
fn yard_plants(b: &mut Builder, town: &TownLayout) {
    let plants = &b.trees.plants;
    for plant in &town.yard_plants {
        let (mesh, material) = (plant_mesh(plants, plant), plants.material.clone());
        let ground = b.ground(plant.at);
        let turn = hash2(plant.at.y + 11.0, plant.at.x) * std::f32::consts::TAU;
        b.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(world_from_ground(plant.at, ground))
                .with_rotation(Quat::from_rotation_y(turn)),
            ranges()[1].clone(),
            TownYardPlant,
            ChildOf(b.root),
        ));
    }
}

/// The shared mesh `plant` is drawn as, one of its kind's variants picked
/// by its spot.
fn plant_mesh(plants: &crate::tree::plants::PlantKit, plant: &YardPlant) -> Handle<Mesh> {
    let pick = |meshes: &[Handle<Mesh>]| {
        let k = (hash2(plant.at.x + 5.0, plant.at.y - 9.0) * meshes.len() as f32) as usize;
        meshes[k % meshes.len()].clone()
    };
    match plant.kind {
        PlantKind::Shrub => pick(&plants.shrubs),
        PlantKind::Flowers => pick(&plants.flowers),
    }
}

/// One of a town's drawn yard shrubs or flower beds.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownYardPlant;

/// One of a town's drawn street, grove or yard trees (its bark or its leaves).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownShadeTree;

/// Which tree a [`ShadeTree`] is drawn as.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ShadeLook {
    species: crate::tree::Species,
    /// Drawn from the trimmed variants.
    trimmed: bool,
    variant: usize,
    yaw: f32,
    /// Seeded by its spot, from 0 to 1: its size within the range.
    jitter: f32,
}

/// The look of `tree` in the town seeded `seed` with `plazas`: street trees
/// of a sidewalk row share a species; the others follow the plaza or the spot.
fn shade_look(seed: u32, plazas: &[Aabb], tree: &ShadeTree) -> ShadeLook {
    let at = tree.at;
    let (a, b) = (hash2(at.x, at.y), hash2(at.y + 7.0, at.x));
    let spot = (a * 65_536.0) as u32;
    let species = match tree.kind {
        ShadeKind::Pit => street_species(seed, tree.row),
        ShadeKind::Front => street_species(seed, spot),
        ShadeKind::Grove => plaza_species(seed, plaza_of(plazas, at)),
        ShadeKind::Yard => yard_species(seed, at),
    };
    ShadeLook {
        species,
        trimmed: matches!(tree.kind, ShadeKind::Pit | ShadeKind::Front),
        variant: (hash2(at.x + 1.0, at.y + 2.0) * VARIANTS as f32) as usize % VARIANTS,
        yaw: b * std::f32::consts::TAU,
        jitter: hash2(at.x + 3.0, at.y - 5.0),
    }
}

/// Where and how a variant of `crown_radius` stands for `tree` over `ground`. A trimmed tree
/// keeps its height (a little taller, so its crown stays over 3.5 m), is
/// never wider than its crown, and leans [`ShadeTree::LEAN`] degrees toward
/// the street; any other is scaled evenly to fit its crown's room. A tree
/// with no street direction (zero, or straight up) stands upright.
fn shade_transform(
    tree: &ShadeTree,
    look: &ShadeLook,
    crown_radius: f32,
    ground: f32,
) -> Transform {
    let size = TREE_SCALE_MIN + (TREE_SCALE_MAX - TREE_SCALE_MIN) * look.jitter;
    let fit = tree.crown / crown_radius;
    let at = world_from_ground(tree.at, ground - TREE_SINK);
    let yaw = Quat::from_rotation_y(look.yaw);
    if !look.trimmed {
        return Transform::from_translation(at)
            .with_rotation(yaw)
            .with_scale(Vec3::splat(size.min(fit)));
    }
    let wide = size.min(fit);
    let toward = Vec3::new(tree.toward.x, 0.0, -tree.toward.y);
    let lean = Vec3::Y
        .cross(toward)
        .try_normalize()
        .map_or(Quat::IDENTITY, |axis| {
            Quat::from_axis_angle(axis, ShadeTree::LEAN.to_radians())
        });
    Transform::from_translation(at)
        .with_rotation(lean * yaw)
        .with_scale(Vec3::new(wide, 1.0 + 0.08 * look.jitter, wide))
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
    use crate::tree::{STREET_SPECIES, Species};
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
        // stucco and whitewash monument with bronze plaques as far as the buildings; iron and
        // timber benches and an iron railing as far as the props.
        let want = [
            Surface::Earth,
            Surface::Brick,
            Surface::Stucco,
            Surface::Whitewash,
            Surface::Bronze,
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
    /// buildings, the dirt streets with the goods yard's cart tracks, the
    /// palenques, sidewalks, and the plazas (lawn, paths, monument, benches,
    /// railing and trees) on flat ground. Run
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
            ("pico", config("General Pico", (8, 8), 13.0, 13, true)),
            ("castex", config("Eduardo Castex", (5, 4), 12.0, 2, false)),
            (
                "metileo",
                pampa_core::TownConfig {
                    density: Some(0.3),
                    ..config("Metileo", (3, 3), 10.0, 19, false)
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
            // The dirt streets with the goods yard's cart tracks, and the
            // palenques, as the game builds them.
            let (origin, streets) = street_shape(&town, &terrain);
            put(&mut text, &streets, Transform::from_translation(origin));
            let (origin, rails) = palenque_shape(&town, &terrain);
            put(&mut text, &rails, Transform::from_translation(origin));
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
    fn a_town_paves_its_plaza_in_brick_and_every_other_block_in_earth_with_curbs() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let mut q = app.world_mut().query_filtered::<
            (&MeshMaterial3d<StandardMaterial>, &VisibilityRange),
            With<TownSidewalk>,
        >();
        let (mut brick, mut curbs) = (0, 0);
        for (material, range) in q.iter(app.world()) {
            assert!(range == &ranges()[0], "a range of its own");
            if material.id() == facade.get(Surface::Paving).id() {
                brick += 1;
            } else {
                assert_eq!(material.id(), facade.get(Surface::Stucco).id());
                curbs += 1;
            }
        }
        // Castex has 20 blocks: one brick ring round the plaza's block, and a
        // curb mesh for it and for each of the other 19 blocks' rings.
        assert_eq!(brick, 1, "brick sidewalk meshes");
        assert_eq!(curbs, 20, "curb meshes");
        // The earth walks inside those 19 rings: one feathered yard decal.
        let mut q = app.world_mut().query_filtered::<
            (&MeshMaterial3d<StandardMaterial>, &VisibilityRange),
            With<TownEarthWalk>,
        >();
        let earth: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(earth.len(), 1, "earth walk meshes");
        assert_eq!(earth[0].0.id(), facade.get(Surface::Yard).id());
        assert!(earth[0].1 == &ranges()[0]);
    }

    #[test]
    fn the_three_tiers_pave_as_the_rule_says() {
        let terrain = |t: &TownLayout| {
            let ground = WorldTerrain::default().0;
            ground.clone().with_flat_area(t.flat_area(&ground))
        };
        let surfaces = |t: &TownLayout, walk: Walk| -> HashSet<Surface> {
            let walks: Vec<Sidewalk> = t
                .sidewalks()
                .iter()
                .filter(|w| w.walk == walk)
                .cloned()
                .collect();
            sidewalk_shapes(&walks, &terrain(t))
                .iter()
                .flat_map(|(_, shape)| shape.surfaces())
                .collect()
        };
        let drawn = towns_to_draw();
        let (village, town, city) = (&drawn[0].0, &drawn[1].0, &drawn[2].0);
        // A city: brick and curbs, no earth. A town: brick round the plaza,
        // earth elsewhere, curbs on both. A village: earth and curbs, no brick.
        assert_eq!(
            surfaces(city, Walk::Brick),
            HashSet::from([Surface::Paving, Surface::Stucco])
        );
        assert!(surfaces(city, Walk::Earth).is_empty());
        assert_eq!(
            surfaces(town, Walk::Brick),
            HashSet::from([Surface::Paving, Surface::Stucco])
        );
        // Earth walks draw a curb and no paving.
        assert_eq!(
            surfaces(town, Walk::Earth),
            HashSet::from([Surface::Stucco])
        );
        assert!(surfaces(village, Walk::Brick).is_empty());
        assert_eq!(
            surfaces(village, Walk::Earth),
            HashSet::from([Surface::Stucco])
        );
    }

    #[test]
    fn an_earth_walk_is_flush_opaque_along_its_middle_and_feathered_at_its_edges() {
        let (town, terrain) = &towns_to_draw()[1];
        let rings: Vec<Vec<Aabb>> = town
            .sidewalks()
            .iter()
            .filter(|w| w.walk == Walk::Earth)
            .collect::<Vec<_>>()
            .chunks(4)
            .map(|ring| ring.iter().map(|w| walk_floor(w)).collect())
            .collect();
        assert_eq!(rings.len(), 19);
        let areas: Vec<Aabb> = rings.iter().flatten().copied().collect();
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        let mesh = crate::settlement::track::walks_mesh(&rings, &ground).expect("a walk");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!()
        };
        let (mut solid, mut edge) = (0, 0);
        for (p, c) in points.iter().zip(colors) {
            let g = GroundVec2::new(p[0], -p[2]);
            // Flush: 3 to 5 cm over the ground (the streets are 3 cm, the
            // brick walks 6, the curbs 9).
            let over = p[1] - terrain.height(g.x, g.y);
            assert!((0.03..=0.05).contains(&over), "{over} m over the ground");
            let near = areas
                .iter()
                .map(|a| a.closest_point(g).distance(g))
                .fold(f32::INFINITY, f32::min);
            // Nothing visible past a meter of feather (the mesh's own corners
            // reach under 2 m, with no alpha); opaque (0.9) only on the walk.
            assert!(near <= 2.0, "{near} m off the walk");
            assert!(
                c[3] < 1e-3 || near <= 1.0 + 1e-3,
                "{near} m off, alpha {}",
                c[3]
            );
            assert!(c[3] <= 0.9 + 1e-4);
            if c[3] > 0.89 {
                solid += 1;
                assert!(near < 1e-3, "opaque {near} m off the walk");
            }
            if c[3] < 0.05 {
                edge += 1;
            }
        }
        assert!(
            solid > 500 && edge > 500,
            "{solid} solid and {edge} feathered corners"
        );
        // Grassy edges: several alpha steps between the middle and the rim.
        let steps: HashSet<u32> = colors.iter().map(|c| (c[3] * 10.0) as u32).collect();
        assert!(steps.len() >= 4, "{steps:?}");
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

    /// A village, a small town, a city, and a city grown on both sides of its
    /// line, with the ground leveled under each as the game does.
    fn towns_to_draw() -> Vec<(TownLayout, Terrain)> {
        let line = |through| {
            Some(pampa_core::RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through,
            })
        };
        [
            ((3, 3), false),
            ((5, 4), false),
            ((8, 8), false),
            ((8, 8), true),
        ]
        .into_iter()
        .map(|(blocks, through)| {
            let town = TownLayout::generate(&pampa_core::TownConfig {
                name: "Pueblo".into(),
                blocks,
                railway: line(through),
                ..TownConfig::castex()
            })
            .unwrap();
            let terrain = WorldTerrain::default().0;
            let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
            (town, terrain)
        })
        .collect()
    }

    /// Every vertex of `shape`'s `surface`, as `(world position, normal, uv)`
    /// for a shape standing at `origin`.
    fn world_vertices(
        origin: Vec3,
        shape: &Shape,
        surface: Surface,
    ) -> Vec<(Vec3, Vec3, bevy::math::Vec2)> {
        shape
            .vertices(surface)
            .into_iter()
            .map(|(p, n, uv)| (p + origin, n, uv))
            .collect()
    }

    /// The ground point under a world position.
    fn ground_of(p: Vec3) -> GroundVec2 {
        GroundVec2::new(p.x, -p.z)
    }

    #[test]
    fn a_town_draws_its_streets_as_one_earth_mesh_and_one_track_mesh() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let mut q = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
            &ChildOf,
        ), With<TownStreet>>();
        let streets: Vec<_> = q.iter(app.world()).collect();
        let roots = streets
            .iter()
            .map(|(.., root)| root.parent())
            .collect::<HashSet<_>>()
            .len();
        assert_eq!(streets.len(), 2 * roots, "one mesh per surface and town");
        let meshes = app.world().resource::<Assets<Mesh>>();
        for surface in [Surface::Earth, Surface::Mud] {
            let wearing: Vec<_> = streets
                .iter()
                .filter(|(_, m, ..)| m.id() == facade.get(surface).id())
                .collect();
            assert_eq!(wearing.len(), roots, "{surface:?} once per town");
            for (mesh, _, range, _) in wearing {
                assert!(**range == ranges()[0], "a range of its own");
                let mesh = meshes.get(&mesh.0).unwrap();
                assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
                assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
            }
        }
        // No flat-colored street cubes any more: every street piece is textured.
        assert_eq!(
            streets
                .iter()
                .map(|(_, m, ..)| m.id())
                .collect::<HashSet<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn street_strips_share_world_space_uvs_so_crossings_show_no_seam() {
        let tile = Surface::Earth.tile();
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = street_shape(&town, &terrain);
            let mut seen: std::collections::HashMap<(i32, i32), bevy::math::Vec2> =
                Default::default();
            let mut shared = 0;
            for (p, n, uv) in world_vertices(origin, &shape, Surface::Earth) {
                // The tops lie flat on the ground: the planar UV of the world.
                if n.y < 0.9 {
                    continue;
                }
                let want = bevy::math::Vec2::new((p.x - origin.x) / tile, (p.z - origin.z) / tile);
                assert!(uv.distance(want) < 1e-3, "{}: uv {uv:?} at {p}", town.name);
                // Whichever strip drew a point, it got the same UV.
                let key = ((p.x * 100.0).round() as i32, (p.z * 100.0).round() as i32);
                if let Some(other) = seen.insert(key, uv) {
                    assert!(other.distance(uv) < 1e-3, "{}: a seam at {p}", town.name);
                    shared += 1;
                }
            }
            assert!(
                shared > 0,
                "{}: the strips overlap at the crossings",
                town.name
            );
        }
    }

    #[test]
    fn the_dirt_covers_every_street_at_the_old_height() {
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = street_shape(&town, &terrain);
            let tops: Vec<(Vec3, Vec3, bevy::math::Vec2)> =
                world_vertices(origin, &shape, Surface::Earth)
                    .into_iter()
                    .filter(|(_, n, _)| n.y > 0.9)
                    .collect();
            for (p, _, _) in &tops {
                let g = ground_of(*p);
                assert!(
                    ((p.y - terrain.height(g.x, g.y)) - 0.03).abs() < 1e-3,
                    "{}: top at {p}",
                    town.name
                );
            }
            // Every strip's four corners are corners of the dirt.
            for street in &town.streets {
                for corner in [
                    street.min,
                    street.max,
                    GroundVec2::new(street.min.x, street.max.y),
                    GroundVec2::new(street.max.x, street.min.y),
                ] {
                    assert!(
                        tops.iter()
                            .any(|(p, _, _)| ground_of(*p).distance(corner) < 1e-3),
                        "{}: {street:?} is not covered",
                        town.name
                    );
                }
            }
        }
    }

    /// The goods buildings of `town`, told apart from the sheds among the
    /// houses by what the station yard builds: grain stores and its long
    /// 25 m shed.
    fn goods(town: &TownLayout) -> Vec<&Building> {
        town.buildings
            .iter()
            .filter(|b| {
                b.kind == BuildingKind::GrainStore
                    || (b.kind == BuildingKind::Shed
                        && b.footprint.size().x.max(b.footprint.size().y) >= 20.0)
            })
            .collect()
    }

    /// The building (of any kind) a `track` piece starts out of: the one
    /// whose footprint lies nearest it.
    fn owner<'t>(town: &'t TownLayout, track: &Aabb) -> &'t Building {
        let gap = |b: &Building| {
            let p = b.footprint.closest_point(track.center());
            let q = track.closest_point(p);
            p.distance(q)
        };
        town.buildings
            .iter()
            .min_by(|a, b| gap(a).total_cmp(&gap(b)))
            .unwrap()
    }

    #[test]
    fn only_the_railways_goods_yard_has_cart_tracks_in_town() {
        let mut yards = 0;
        for (town, _) in towns_to_draw() {
            let tracks = goods_tracks(&town);
            assert!(
                !tracks.is_empty(),
                "{}: the goods yard has tracks",
                town.name
            );
            for track in &tracks {
                let b = owner(&town, track);
                assert!(
                    goods(&town).contains(&b),
                    "{}: a track out of a {:?}",
                    town.name,
                    b.kind
                );
            }
            // The sheds among the houses, the station, houses, stores and
            // the plazas have none.
            for b in town.buildings.iter().filter(|b| !goods(&town).contains(b)) {
                assert!(
                    tracks.iter().all(|t| b
                        .footprint
                        .closest_point(t.center())
                        .distance(t.center())
                        > 1.0),
                    "{}: a track at a {:?}",
                    town.name,
                    b.kind
                );
            }
            // Every grain store and the yard's shed lead out a track.
            for b in town
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::GrainStore)
            {
                assert!(
                    tracks.iter().any(|t| b
                        .footprint
                        .closest_point(t.center())
                        .distance(t.center())
                        < 14.0),
                    "{}: a grain store without a track",
                    town.name
                );
            }
            let sheds = goods(&town)
                .iter()
                .filter(|b| b.kind == BuildingKind::Shed)
                .count();
            assert_eq!(sheds, 1, "{}: one goods shed in the yard", town.name);
            // Every goods building leads out two ruts a gauge apart.
            for b in goods(&town) {
                let n = side_vector(b.front);
                let across = GroundVec2::new(-n.y, n.x);
                let half = b.footprint.size() * 0.5;
                let door = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                let mine: Vec<f32> = tracks
                    .iter()
                    .filter(|t| t.center().distance(door) < GOODS_REACH + 1.0)
                    .map(|t| (t.center() - door).dot(across))
                    .collect();
                assert!(!mine.is_empty(), "{}: {:?} has no track", town.name, b.kind);
                for lane in [-1.0, 1.0] {
                    let want = lane * 0.7;
                    assert!(
                        mine.iter().any(|o| (o - want).abs() < 1e-3),
                        "{}",
                        town.name
                    );
                }
                yards += 1;
            }
            for street in &town.streets {
                // The plain streets are dirt, no more: no rut lies on one.
                assert!(
                    tracks.iter().all(|t| !t.intersects(street)),
                    "{}",
                    town.name
                );
            }
        }
        assert!(yards >= 8);
    }

    #[test]
    fn goods_tracks_run_to_the_street_or_a_short_apron_and_keep_clear() {
        let mut to_street = 0;
        let mut aprons = 0;
        for (town, _) in towns_to_draw() {
            let obstacles = town.obstacles();
            for track in goods_tracks(&town) {
                // Short, along the front, off everything.
                let size = track.size();
                assert!(
                    size.x.max(size.y) <= GOODS_REACH + RUT_WIDTH,
                    "{}",
                    town.name
                );
                assert!(size.x.min(size.y) <= RUT_WIDTH + 1e-4, "{}", town.name);
                for o in obstacles
                    .iter()
                    .filter(|o| !goods(&town).iter().any(|b| b.collider() == **o))
                {
                    assert!(!o.intersects(&track), "{}: a track on {o:?}", town.name);
                }
                for w in town.sidewalks() {
                    assert!(!w.area.intersects(&track), "{}", town.name);
                }
                if let Some(corridor) = town.rail_corridor() {
                    assert!(
                        !corridor.intersects(&track),
                        "{}: a track on the line",
                        town.name
                    );
                }
                // Ends at a street when one is near its end, else is an apron.
                let touches = town
                    .streets
                    .iter()
                    .any(|s| s.expanded(RUT_GAP).intersects(&track));
                if touches {
                    to_street += 1;
                } else {
                    aprons += 1;
                }
            }
        }
        assert!(to_street > 0, "a goods building by a street");
        assert!(aprons > 0, "a goods building with an apron");
    }

    #[test]
    fn a_goods_door_on_or_next_to_a_street_gets_no_track_and_others_stop_short() {
        // A street along x from y = 0 to 10; the doors look north (+y) at it.
        let street = [Aabb::new(
            GroundVec2::new(-50.0, 0.0),
            GroundVec2::new(50.0, 10.0),
        )];
        let north = GroundVec2::new(0.0, 1.0);
        let at = |y: f32| GroundVec2::new(3.0, y);
        // On the street's edge, or inside it: the street is the apron.
        assert_eq!(goods_track_length(&street, at(0.0), north), None);
        assert_eq!(goods_track_length(&street, at(5.0), north), None);
        // Too near for a stretch worth drawing (0.3 m start plus 1 m).
        assert_eq!(goods_track_length(&street, at(-1.0), north), None);
        // A track runs up to a quarter meter short of the street.
        let far = goods_track_length(&street, at(-4.0), north).unwrap();
        assert!((far - 3.75).abs() < 1e-4, "{far}");
        // Out of the street's reach: the 8 m apron.
        let apron = goods_track_length(&street, at(-30.0), north).unwrap();
        assert!((apron - 8.0).abs() < 1e-4, "{apron}");
        // Facing away from the street: the apron too.
        let away = goods_track_length(&street, at(-4.0), GroundVec2::new(0.0, -1.0)).unwrap();
        assert!((away - 8.0).abs() < 1e-4, "{away}");
    }

    /// Towns of other street widths (10, 11, 13 and 16 m), a small one and cities.
    fn varied_streets() -> Vec<(TownLayout, Terrain)> {
        [
            ((4, 4), 10.0),
            ((6, 6), 11.0),
            ((8, 8), 16.0),
            ((5, 5), 13.0),
        ]
        .into_iter()
        .map(|(blocks, street_width)| {
            let town = TownLayout::generate(&pampa_core::TownConfig {
                name: "Calles".into(),
                blocks,
                street_width,
                seed: 5,
                ..TownConfig::castex()
            })
            .unwrap();
            let terrain = WorldTerrain::default().0;
            let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
            (town, terrain)
        })
        .collect()
    }

    #[test]
    fn a_town_draws_its_shade_trees_from_the_shared_kit_two_entities_each() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let kit = app.world().resource::<crate::tree::TreeKit>().clone();
        // Castex is a small town: trimmed trees in front of houses, free
        // ones in the yards, all from the kit's shared meshes.
        let plazas: Vec<Aabb> = town.plazas().collect();
        let mut expected: HashSet<AssetId<Mesh>> = HashSet::new();
        let mut trimmed = 0;
        for tree in &town.shade_trees {
            let look = shade_look(town.seed, &plazas, tree);
            let own = kit.species(look.species);
            let variants = if look.trimmed {
                &own.pruned
            } else {
                &own.variants
            };
            let v = &variants[look.variant];
            expected.extend([v.bark.id(), v.leaves.id()]);
            trimmed += usize::from(look.trimmed);
        }
        assert!(trimmed > 0 && trimmed < town.shade_trees.len());
        let mut q = app
            .world_mut()
            .query_filtered::<(&Mesh3d, &VisibilityRange, &ThinOccluder), With<TownShadeTree>>();
        let mut drawn = 0;
        for (mesh, range, fade) in q.iter(app.world()) {
            drawn += 1;
            assert!(expected.contains(&mesh.id()));
            assert!(range == &ranges()[2]);
            assert!(fade.top - fade.bottom > 3.0, "{fade:?}");
        }
        assert_eq!(drawn, 2 * town.shade_trees.len());
        assert!(town.shade_trees.len() > 5);
        // No new mesh: the kit's, as many as it built.
        let meshes = app.world().resource::<Assets<Mesh>>().len();
        let kit_meshes = 2 * VARIANTS * (Species::ALL.len() + STREET_SPECIES.len());
        assert!(meshes > kit_meshes);
    }

    #[test]
    fn a_town_draws_its_yard_plants_one_entity_each_from_the_shared_kit() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let kit = app
            .world()
            .resource::<crate::tree::TreeKit>()
            .plants
            .clone();
        let mut q = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
        ), With<TownYardPlant>>();
        let mut drawn = 0;
        for (mesh, material, range) in q.iter(app.world()) {
            drawn += 1;
            assert!(
                kit.shrubs
                    .iter()
                    .chain(&kit.flowers)
                    .any(|h| h.id() == mesh.id())
            );
            assert_eq!(material.id(), kit.material.id());
            assert!(range == &ranges()[1]);
        }
        assert_eq!(drawn, town.yard_plants.len());
        assert!(drawn > 10, "{drawn}");
        // Shrubs and beds both: more than one mesh in use.
        let used: HashSet<AssetId<Mesh>> = q.iter(app.world()).map(|(m, _, _)| m.id()).collect();
        assert!(used.len() >= 3, "{} meshes", used.len());
    }

    #[test]
    fn a_tree_with_no_street_direction_or_no_crown_still_gets_a_finite_transform() {
        let look = ShadeLook {
            species: Species::Olmo,
            trimmed: true,
            variant: 0,
            yaw: 0.7,
            jitter: 0.5,
        };
        let tree = ShadeTree {
            at: Vec2::new(3.0, 4.0),
            kind: ShadeKind::Pit,
            crown: 2.3,
            toward: Vec2::ZERO,
            row: 0,
        };
        for crown_radius in [0.0, 2.0] {
            let at = shade_transform(&tree, &look, crown_radius, 0.0);
            assert!(at.translation.is_finite() && at.scale.is_finite());
            assert!(at.rotation.is_finite(), "{crown_radius}: {:?}", at.rotation);
            // No direction to lean toward: it stands upright.
            assert!((at.rotation * Vec3::Y).angle_between(Vec3::Y) < 1e-3);
        }
        let plain = ShadeLook {
            trimmed: false,
            ..look
        };
        assert!(shade_transform(&tree, &plain, 0.0, 0.0).scale.is_finite());
    }

    #[test]
    fn trimmed_street_trees_keep_their_crown_over_1_9_m_and_off_the_facades() {
        let mut checked = 0;
        for (town, _) in towns_to_draw().into_iter().chain(varied_streets()) {
            let walls: Vec<Aabb> = town
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect();
            let plazas: Vec<Aabb> = town.plazas().collect();
            for tree in town
                .shade_trees
                .iter()
                .filter(|t| t.kind != ShadeKind::Grove)
            {
                let look = shade_look(town.seed, &plazas, tree);
                let mesh = if look.trimmed {
                    crate::tree::mesh::street_tree(look.species, look.variant as u32)
                } else {
                    crate::tree::mesh::tree(look.species, look.variant as u32)
                };
                let at = shade_transform(tree, &look, mesh.crown_radius, 0.0);
                // Never wider than the room core left it.
                assert!(
                    at.scale.x * mesh.crown_radius <= tree.crown + 1e-3,
                    "{:?}",
                    tree.kind
                );
                // A trimmed tree stands nearly straight (user, 2026-10-26): at most
                // six degrees off the vertical, and leaning toward the street.
                let up = at.rotation * Vec3::Y;
                let off = up.angle_between(Vec3::Y).to_degrees();
                if look.trimmed {
                    assert!(
                        off > 1.0 && off <= 6.0,
                        "{:?}: leans {off} degrees",
                        tree.kind
                    );
                    let toward = Vec3::new(tree.toward.x, 0.0, -tree.toward.y);
                    assert!(up.dot(toward) > 0.0, "leans away from the street");
                } else {
                    assert!(off < 0.01);
                }
                for leaf in &mesh.leaves.positions {
                    let p = at.transform_point(*leaf);
                    let ground = GroundVec2::new(p.x, -p.z);
                    if look.trimmed {
                        // Bare trunk to about 2.5 m: no leaf under 1.9 m.
                        assert!(p.y >= 1.9, "{:?}: a leaf at {:.2} m", look.species, p.y);
                    }
                    for wall in &walls {
                        let gap = wall.closest_point(ground).distance(ground);
                        assert!(
                            gap >= 0.5,
                            "{} {:?}: a leaf {gap:.2} m off a wall",
                            town.name,
                            tree.kind
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 500, "{checked}");
    }

    #[test]
    fn street_trees_are_trimmed_species_in_rows_and_yards_take_any() {
        let pico = &towns_to_draw()[2].0;
        let plazas: Vec<Aabb> = pico.plazas().collect();
        let mut rows: std::collections::HashMap<u32, HashSet<Species>> = Default::default();
        let mut yards = HashSet::new();
        for tree in &pico.shade_trees {
            let look = shade_look(pico.seed, &plazas, tree);
            match tree.kind {
                ShadeKind::Pit => {
                    assert!(look.trimmed);
                    // The species Aromo stays low: never along a street.
                    assert_ne!(look.species, Species::Aromo);
                    rows.entry(tree.row).or_default().insert(look.species);
                }
                ShadeKind::Yard => {
                    assert!(!look.trimmed);
                    yards.insert(look.species);
                }
                ShadeKind::Grove => assert!(!look.trimmed),
                ShadeKind::Front => unreachable!("a city has no front trees"),
            }
        }
        // One species a row, different species across the town.
        assert!(rows.values().all(|s| s.len() == 1));
        let across: HashSet<Species> = rows.values().flatten().copied().collect();
        assert_eq!(across.len(), 3, "{across:?}");
        assert_eq!(yards.len(), 4, "{yards:?}");
    }

    #[test]
    fn a_cazuela_is_a_curb_high_rim_round_earth_on_the_walk() {
        let (city, terrain) = &towns_to_draw()[2];
        let (origin, shape) = tree_pit_shape(city, terrain).expect("a city has pits");
        let pits = city
            .shade_trees
            .iter()
            .filter(|t| t.kind == ShadeKind::Pit)
            .count();
        let rim = world_vertices(origin, &shape, Surface::Stucco);
        let soil = world_vertices(origin, &shape, Surface::Earth);
        // A soil block (20 vertices, no underside) and four rim slabs for every pit.
        assert_eq!(soil.len(), 20 * pits);
        assert_eq!(rim.len(), 4 * soil.len());
        let top = |v: &[(Vec3, Vec3, bevy::math::Vec2)]| {
            v.iter()
                .filter(|(_, n, _)| n.y > 0.99)
                .map(|(p, _, _)| p.y - terrain.height(p.x, -p.z))
                .fold(f32::NEG_INFINITY, f32::max)
        };
        // The rim as high as the curb (9 cm) and the earth just over the paving (6 cm).
        assert!((top(&rim) - 0.09).abs() < 2e-3, "{}", top(&rim));
        assert!((top(&soil) - 0.065).abs() < 2e-3, "{}", top(&soil));
        // Every vertex lies on a sidewalk, 0.4 m or more in from the curb's edge.
        for (p, _, _) in &rim {
            let g = GroundVec2::new(p.x, -p.z);
            assert!(
                city.sidewalks()
                    .iter()
                    .any(|w| w.area.expanded(1e-3).contains_point(g)),
                "{g:?} off the walk"
            );
        }
        assert!(pits > 100);
        // A small town has none.
        let (small, terrain) = &towns_to_draw()[1];
        assert!(tree_pit_shape(small, terrain).is_none());
    }

    #[test]
    fn cart_tracks_lie_a_few_millimeters_over_the_dirt_level() {
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = street_shape(&town, &terrain);
            let verts = world_vertices(origin, &shape, Surface::Mud);
            assert!(!verts.is_empty(), "{}", town.name);
            for (p, n, _) in &verts {
                assert!(n.y > 0.99, "{}: a rut that leans", town.name);
                let g = ground_of(*p);
                let over = p.y - terrain.height(g.x, g.y);
                // 5 mm or more over the street's dirt (no fight with it),
                // and under the sidewalks' paving.
                assert!(over >= 0.03 + 0.005, "{}: {over} m", town.name);
                assert!(over < SIDEWALK_TOP, "{}: {over} m", town.name);
                // And never over a street's dirt: the streets wear no ruts.
                for street in &town.streets {
                    assert!(
                        !street.expanded(-0.01).contains_point(g),
                        "{}: a rut at {g:?} on a street",
                        town.name
                    );
                }
            }
            // No two ruts overlap.
            let tracks = goods_tracks(&town);
            for (i, a) in tracks.iter().enumerate() {
                assert!(
                    tracks[i + 1..].iter().all(|b| !a.intersects(b)),
                    "{}",
                    town.name
                );
            }
        }
    }

    #[test]
    fn a_town_draws_all_its_rails_as_one_timber_mesh_in_the_small_props_range() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let mut q = app.world_mut().query_filtered::<(
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
            &ChildOf,
        ), With<TownPalenque>>();
        let rails: Vec<_> = q.iter(app.world()).collect();
        let roots = rails
            .iter()
            .map(|(.., root)| root.parent())
            .collect::<HashSet<_>>()
            .len();
        assert_eq!(rails.len(), roots, "one mesh per town");
        assert!(roots >= 1);
        for (material, range, _) in rails {
            assert_eq!(material.id(), facade.get(Surface::Planks).id());
            assert!(*range == ranges()[1], "the small props' range");
        }
    }

    #[test]
    fn every_rail_of_a_town_gets_a_palenque_along_its_footprint() {
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = palenque_shape(&town, &terrain);
            let rails: Vec<Aabb> = std::iter::once(town.hitching_post.rail)
                .chain(town.palenques.iter().map(|p| p.rail))
                .collect();
            assert!(
                rails.len() > 1,
                "{}: palenques besides the store's",
                town.name
            );
            let verts: Vec<Vec3> = world_vertices(origin, &shape, Surface::Planks)
                .into_iter()
                .map(|(p, _, _)| p)
                .collect();
            let mut claimed = 0;
            for rail in &rails {
                let mine: Vec<&Vec3> = verts
                    .iter()
                    .filter(|p| rail.expanded(0.2).contains_point(ground_of(**p)))
                    .collect();
                assert!(!mine.is_empty(), "{}: nothing at {rail:?}", town.name);
                claimed += mine.len();
                // End to end along the rail's long side, a post wide across.
                let (lo, hi) = mine.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                    let g = ground_of(**p);
                    let t = if rail.size().x >= rail.size().y {
                        g.x
                    } else {
                        g.y
                    };
                    (lo.min(t), hi.max(t))
                });
                let long = rail.size().x.max(rail.size().y);
                assert!((hi - lo - long).abs() < 1e-2, "{}: {}", town.name, hi - lo);
                // The posts stand on the ground there and the bar rides at its height.
                let c = rail.center();
                let ground = terrain.height(c.x, c.y);
                let top = mine.iter().fold(f32::MIN, |m, p| m.max(p.y)) - ground;
                assert!((top - 1.15).abs() < 1e-3, "{}", town.name);
                assert!(mine.iter().any(|p| ((p.y - ground) - 1.0).abs() < 0.06));
            }
            // Nothing else is drawn, and it stays light.
            assert_eq!(claimed, verts.len(), "{}: stray timber", town.name);
            assert!(shape.triangles() < 120 * rails.len(), "{}", town.name);
        }
    }
}
