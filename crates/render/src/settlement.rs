//! Rural settlements (ranchos, puestos, chacras and estancias), streamed
//! with the terrain chunks like the fences and the roads.
//!
//! `pampa-core` lays them out ([`SettlementLayout`]). When a terrain chunk
//! is built, every settlement whose hub lies in it is drawn as the chunk's
//! children, and goes with it:
//!
//! - every house, barn and prop, built procedurally in its own space (see
//!   [`buildings`] and [`props`]) as one mesh per [`Surface`] it wears, all
//!   sharing one PBR material per surface ([`SettlementMaterials`]);
//! - the windpump's wheel apart, spinning as a [`WindmillRotor`] (turned by
//!   the [`crate::TownPlugin`]);
//! - the eucalyptus grove and the poplar row: the vegetation models'
//!   trunks (or their stand-ins) under procedural crowns ([`crown`]), a
//!   Lombardy poplar's column and a eucalyptus's clumps, recoloured per
//!   species;
//! - the tracks (huellas) and the trodden yards as decals over the drawn
//!   ground ([`track`]);
//! - the abandoned wagons, in some yards and on a few road verges;
//! - the Poly Haven yard props about the barns and wells ([`yard`]).
//!
//! A structure kind with a model slot draws a third-party glTF model
//! instead of its procedural mesh when `assets/models/third-party/<slot>/`
//! holds one; the procedural mesh stands in while it loads and stays if it
//! fails ([`slots`]).
//!
//! Corral sides, windpumps, wells, carts, haystacks and trees fade out of
//! the camera's way ([`ThinOccluder`]); houses and barns stop the camera
//! instead (the game's camera occluders). Everything is drawn within a few
//! shared [`VisibilityRange`]s: Bevy keeps a GPU slot for every distinct
//! range for good (see [`crate::flora::TUFT_FADE_STEPS`]).

pub mod buildings;
pub mod crown;
pub mod materials;
pub mod props;
pub mod shape;
pub mod slots;
pub mod track;
pub mod yard;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use pampa_core::settlement::RoadsideWreck;
use pampa_core::{
    Aabb, ChunkCoord, Settlement, SettlementLayout, Side, Species, Structure, StructureKind,
    Terrain, Track, Vec2 as GroundVec2,
};

pub use materials::{SettlementMaterials, Surface};
pub use slots::{ModelRotor, ModelSlots, SlotFit, SlotFolder, SlotModels, SlotRotor, StandIn};
pub use yard::{YardModels, YardProp};

use crate::character::finish::Finish;
use crate::coords::world_from_ground;
use crate::flora::{
    FloraKit, Model, TREE_FADE, TREE_FAR, ThinOccluder, VegetationModels, reference_height, tinted,
    visible_within,
};
use crate::gltf::{self, ModelState};
use crate::terrain::{
    ChunkDetail, ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain, mesh_height,
};
use crate::town::WindmillRotor;
use shape::Shape;
use slots::SceneDress;

/// The world's settlements. Insert before the plugin runs; without it none
/// is drawn.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldSettlements(pub SettlementLayout);

/// A drawn piece of a settlement's structure, child of its chunk: which
/// settlement (an index into [`SettlementLayout::settlements`]), which of its
/// structures, and what it is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementPart {
    pub settlement: u32,
    pub structure: u32,
    pub kind: StructureKind,
}

impl SettlementPart {
    /// The `settlement` of a roadside wreck, whose `structure` indexes
    /// [`SettlementLayout::roadside`].
    pub const ROADSIDE: u32 = u32::MAX;
}

/// A yard prop's scene, child of its chunk: which settlement, and what.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct YardPart {
    pub settlement: u32,
    pub prop: YardProp,
}

/// A settlement's tracks, one decal mesh, child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackSurface(pub u32);

/// A settlement's patio and corral floors, one decal mesh.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct YardSurface(pub u32);

#[derive(Component)]
struct SettlementsLaid;

/// Where houses, barns, tanks and haystacks stop being drawn, meters:
/// well past the dust fog, near the streaming edge.
pub const BUILDING_FAR: f32 = 400.0;
const BUILDING_FADE: f32 = 25.0;
/// Where corrals, windpumps and the small props stop being drawn.
pub const PROP_FAR: f32 = 220.0;
const PROP_FADE: f32 = 20.0;

/// The few shared ranges settlements are drawn within: buildings, props,
/// and the trees' (the same as the wild trees').
pub fn ranges() -> [VisibilityRange; 3] {
    [
        visible_within(BUILDING_FAR, BUILDING_FADE),
        visible_within(PROP_FAR, PROP_FADE),
        visible_within(TREE_FAR, TREE_FADE),
    ]
}

/// The meshes and materials shared by every settlement beyond the surface
/// materials: the windpump's wheel and the trees' paint.
#[derive(Resource, Debug, Clone)]
struct SettlementKit {
    rotor: Vec<(Surface, Handle<Mesh>)>,
    /// Bark and leaves of the eucalyptus, then of the poplar.
    trees: [[Handle<StandardMaterial>; 2]; 2],
    /// The seeded crown variants of the eucalyptus, then of the poplar.
    crowns: [Vec<Handle<Mesh>>; 2],
    /// How far the widest crown variant of each species reaches out from
    /// its trunk, per meter of tree height: the fade shape's radius.
    crown_reach: [f32; 2],
}

/// Draws the [`WorldSettlements`] chunk by chunk. Needs the
/// [`crate::TerrainPlugin`]; trees need the [`crate::FloraPlugin`]'s models
/// or stand-ins, and the windpump wheels turn with the
/// [`crate::TownPlugin`].
pub struct SettlementPlugin;

impl Plugin for SettlementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSettlements>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            .init_resource::<ModelSlots>()
            .init_resource::<SlotFolder>()
            .add_systems(
                Startup,
                (
                    materials::setup_materials,
                    setup_kit,
                    slots::load_slot_models,
                    yard::load_yard_models,
                ),
            )
            .add_systems(Update, (slots::swap_in_models, slots::spin_model_rotors))
            .add_observer(slots::dress_scene)
            .add_systems(
                PostUpdate,
                lay_settlements
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let rotor = props::rotor()
        .into_meshes()
        .into_iter()
        .map(|(surface, mesh)| (surface, meshes.add(mesh)))
        .collect();
    // Multiplied over the models' bark and leaf paint (linear, so above 1
    // brightens): the eucalyptus's pale peeling bole and blue-grey leaves,
    // the poplar's grey bark and fresh green.
    let mut paint = |finish: Finish, (r, g, b): (f32, f32, f32)| {
        materials.add(tinted(finish, false, Color::linear_rgb(r, g, b)))
    };
    let trees = [
        [
            paint(Finish::Bark, (5.5, 7.0, 8.0)),
            paint(Finish::Leaves, (1.1, 1.2, 2.6)),
        ],
        [
            paint(Finish::Bark, (2.8, 3.6, 4.2)),
            paint(Finish::Leaves, (1.2, 1.6, 0.95)),
        ],
    ];
    let mut crown_reach = [0.0_f32; 2];
    let crowns = [crown::CrownKind::Eucalyptus, crown::CrownKind::Poplar]
        .into_iter()
        .enumerate()
        .map(|(species, kind)| {
            (0..crown::CROWN_VARIANTS)
                .map(|v| {
                    let mesh = crown::crown(kind, v);
                    crown_reach[species] = crown_reach[species].max(reach_from_axis(&mesh));
                    meshes.add(mesh)
                })
                .collect()
        })
        .collect::<Vec<Vec<Handle<Mesh>>>>();
    let [eucalyptus, poplar]: [Vec<Handle<Mesh>>; 2] =
        crowns.try_into().expect("two species of crowns");
    commands.insert_resource(SettlementKit {
        rotor,
        trees,
        crowns: [eucalyptus, poplar],
        crown_reach,
    });
}

/// The farthest a mesh's points lie from its vertical axis (`x = z = 0`).
fn reach_from_axis(mesh: &Mesh) -> f32 {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return 0.0;
    };
    points
        .iter()
        .map(|p| (p[0] * p[0] + p[2] * p[2]).sqrt())
        .fold(0.0, f32::max)
}

/// What one chunk child is.
#[derive(Clone, Copy, Debug)]
enum Tag {
    Part(SettlementPart),
    Tracks(u32),
    Yards(u32),
    Prop(YardPart),
}

/// What a piece draws: a procedural mesh, or a glTF model's scene (spawned
/// now when `ready`, else once its model has loaded).
enum Look {
    Mesh(Handle<Mesh>, Handle<StandardMaterial>),
    Scene {
        scene: Handle<WorldAsset>,
        ready: bool,
        dress: SceneDress,
    },
}

/// One entity to spawn under a chunk.
struct Piece {
    look: Look,
    transform: Transform,
    range: Option<VisibilityRange>,
    fade: Option<ThinOccluder>,
    rotor: Option<WindmillRotor>,
    tag: Tag,
    /// Drawn only until its structure's slot model is ready.
    stand_in: bool,
}

/// Whether a structure kind's slot model is used, and how far it has
/// loaded.
#[derive(Clone, Debug)]
enum SlotUse {
    /// No model, or it failed: the procedural mesh only.
    Procedural,
    /// Loading: the procedural mesh stands in until it is ready.
    Loading(Handle<WorldAsset>),
    Ready(Handle<WorldAsset>),
}

/// What drawing a settlement needs.
struct Painter<'a> {
    terrain: &'a Terrain,
    /// The drawn ground's height (the chunk mesh's, not the finer logic
    /// terrain's), so nothing floats or sinks.
    ground: &'a dyn Fn(GroundVec2) -> f32,
    surfaces: &'a SettlementMaterials,
    kit: &'a SettlementKit,
    models: Option<&'a VegetationModels>,
    stand_ins: Option<&'a FloraKit>,
    server: Option<&'a AssetServer>,
    slots: Option<&'a SlotModels>,
    fits: &'a ModelSlots,
    yard: Option<&'a YardModels>,
}

/// The yaw that turns a structure's own `+Z` toward its `front`.
fn yaw(front: Side) -> Quat {
    use std::f32::consts::{FRAC_PI_2, PI};
    Quat::from_rotation_y(match front {
        Side::South => 0.0,
        Side::North => PI,
        Side::East => FRAC_PI_2,
        Side::West => -FRAC_PI_2,
    })
}

impl Painter<'_> {
    /// Whether `kind`'s slot model is used, and how far it has loaded.
    fn slot_use(&self, kind: StructureKind) -> SlotUse {
        let (Some(slot), Some(models), Some(server)) = (kind.slot(), self.slots, self.server)
        else {
            return SlotUse::Procedural;
        };
        let Some(scene) = models.0.get(slot) else {
            return SlotUse::Procedural;
        };
        match gltf::model_state(server, scene) {
            ModelState::Ready => SlotUse::Ready(scene.clone()),
            ModelState::Loading => SlotUse::Loading(scene.clone()),
            ModelState::Failed => SlotUse::Procedural,
        }
    }

    /// The scene piece of `kind`'s slot model at `transform` (the
    /// structure's own space), when it has one.
    fn slot_scene(
        &self,
        kind: StructureKind,
        transform: Transform,
        range: VisibilityRange,
        fade: Option<ThinOccluder>,
        speed: f32,
        tag: Tag,
    ) -> Option<Piece> {
        let (scene, ready) = match self.slot_use(kind) {
            SlotUse::Procedural => return None,
            SlotUse::Loading(scene) => (scene, false),
            SlotUse::Ready(scene) => (scene, true),
        };
        let fit = self.fits.fit(kind.slot()?);
        Some(Piece {
            look: Look::Scene {
                scene,
                ready,
                dress: SceneDress {
                    range,
                    fade,
                    rotor: fit.rotor.clone().map(|r| (r, speed)),
                    glass: false,
                },
            },
            transform: transform * fit.transform(),
            range: None,
            fade: None,
            rotor: None,
            tag,
            stand_in: false,
        })
    }

    /// Where `structure` stands: its middle on the ground, turned to its
    /// front; and how far its lowest corner lies under that, plus a margin,
    /// for its walls and footings to reach down to.
    fn place(&self, structure: &Structure) -> (Transform, f32) {
        let fp = structure.footprint;
        let base = (self.ground)(fp.center());
        let low = [
            fp.min,
            fp.max,
            GroundVec2::new(fp.min.x, fp.max.y),
            GroundVec2::new(fp.max.x, fp.min.y),
        ]
        .into_iter()
        .map(|p| (self.ground)(p))
        .fold(base, f32::min);
        let transform = Transform::from_translation(world_from_ground(fp.center(), base))
            .with_rotation(yaw(structure.front));
        (transform, base - low + 0.3)
    }

    /// Every piece of settlement `index`.
    fn settlement(
        &self,
        index: u32,
        settlement: &Settlement,
        meshes: &mut Assets<Mesh>,
    ) -> Vec<Piece> {
        let mut pieces = Vec::new();
        for (k, structure) in settlement.structures.iter().enumerate() {
            let tag = SettlementPart {
                settlement: index,
                structure: k as u32,
                kind: structure.kind,
            };
            self.structure(structure, tag, meshes, &mut pieces);
        }
        let tracks: Vec<&Track> = settlement.tracks.iter().collect();
        if let Some(mesh) = track::tracks_mesh(&tracks, &self.ground) {
            pieces.push(self.decal(meshes.add(mesh), Surface::Mud, Tag::Tracks(index)));
        }
        let yards: Vec<Aabb> = std::iter::once(settlement.patio)
            .chain(settlement.of(StructureKind::Corral).map(|c| c.footprint))
            .collect();
        if let Some(mesh) = track::yards_mesh(&yards, &self.ground) {
            pieces.push(self.decal(meshes.add(mesh), Surface::Yard, Tag::Yards(index)));
        }
        if let Some(models) = self.yard {
            for spot in yard::yard_props(settlement) {
                let Some(scene) = models.0.get(&spot.prop) else {
                    continue;
                };
                let base = (self.ground)(spot.at) + spot.lift;
                pieces.push(Piece {
                    look: Look::Scene {
                        scene: scene.clone(),
                        ready: true,
                        dress: SceneDress {
                            range: ranges()[1].clone(),
                            fade: None,
                            rotor: None,
                            glass: spot.prop == YardProp::Lantern,
                        },
                    },
                    transform: Transform::from_translation(world_from_ground(spot.at, base))
                        .with_rotation(spot.turn),
                    range: None,
                    fade: None,
                    rotor: None,
                    tag: Tag::Prop(YardPart {
                        settlement: index,
                        prop: spot.prop,
                    }),
                    stand_in: false,
                });
            }
        }
        pieces
    }

    /// Roadside wreck `index`: lying along the road, a little tilted.
    fn roadside(&self, index: u32, wreck: &RoadsideWreck, meshes: &mut Assets<Mesh>) -> Vec<Piece> {
        let (wide, deep) = Settlement::WRECK_SIZE;
        let structure = Structure {
            kind: StructureKind::WreckedWagon,
            footprint: Aabb::from_center_size(wreck.at, GroundVec2::new(wide, deep)),
            height: 2.2,
            front: Side::North,
            variant: wreck.variant,
        };
        let tag = SettlementPart {
            settlement: SettlementPart::ROADSIDE,
            structure: index,
            kind: StructureKind::WreckedWagon,
        };
        // Its own `x` along the road.
        let heading = Quat::from_rotation_y(wreck.along.y.atan2(wreck.along.x));
        let mut pieces = Vec::new();
        self.structure_turned(&structure, tag, Some(heading), meshes, &mut pieces);
        pieces
    }

    /// A world-space decal: no range, like the roads.
    fn decal(&self, mesh: Handle<Mesh>, surface: Surface, tag: Tag) -> Piece {
        Piece {
            look: Look::Mesh(mesh, self.surfaces.get(surface)),
            transform: Transform::default(),
            range: None,
            fade: None,
            rotor: None,
            tag,
            stand_in: false,
        }
    }

    fn structure(
        &self,
        structure: &Structure,
        tag: SettlementPart,
        meshes: &mut Assets<Mesh>,
        out: &mut Vec<Piece>,
    ) {
        self.structure_turned(structure, tag, None, meshes, out);
    }

    /// Every piece of `structure`, turned by `heading` instead of toward
    /// its front when given (a roadside wreck along its road). A kind
    /// whose slot model is ready draws that model only; while it loads, the
    /// procedural pieces stand in.
    fn structure_turned(
        &self,
        structure: &Structure,
        tag: SettlementPart,
        heading: Option<Quat>,
        meshes: &mut Assets<Mesh>,
        out: &mut Vec<Piece>,
    ) {
        if structure.kind.is_tree() {
            self.tree(structure, tag, out);
            return;
        }
        let (mut transform, drop) = self.place(structure);
        if let Some(heading) = heading {
            transform.rotation = heading;
        }
        if structure.kind == StructureKind::WreckedWagon {
            // Settled askew where it broke down.
            let v = structure.variant;
            let (roll, pitch) = ((v % 7) as f32 - 3.0, (v / 7 % 5) as f32 - 2.0);
            transform.rotation *= Quat::from_rotation_x(roll * 0.012)
                * Quat::from_rotation_z(pitch * 0.015)
                * Quat::from_rotation_y((v % 2) as f32 * std::f32::consts::PI);
        }
        let at = structure.footprint.center();
        let size = structure.footprint.size();
        let reach = size.x.max(size.y) / 2.0;
        let slot = self.slot_use(structure.kind);
        if matches!(slot, SlotUse::Ready(_)) {
            let (range, fade, speed) = self.slot_look(structure, at, reach);
            out.extend(self.slot_scene(
                structure.kind,
                transform,
                range,
                fade,
                speed,
                Tag::Part(tag),
            ));
            return;
        }
        if matches!(slot, SlotUse::Loading(_)) {
            let (range, fade, speed) = self.slot_look(structure, at, reach);
            out.extend(self.slot_scene(
                structure.kind,
                transform,
                range,
                fade,
                speed,
                Tag::Part(tag),
            ));
        }
        let stand_in = matches!(slot, SlotUse::Loading(_));
        let mut add = |shape: Shape,
                       transform: Transform,
                       range: usize,
                       fade: Option<ThinOccluder>,
                       rotor: Option<WindmillRotor>| {
            for (surface, mesh) in shape.into_meshes() {
                out.push(Piece {
                    look: Look::Mesh(meshes.add(mesh), self.surfaces.get(surface)),
                    transform,
                    range: Some(ranges()[range].clone()),
                    fade,
                    rotor,
                    tag: Tag::Part(tag),
                    stand_in,
                });
            }
        };
        match structure.kind {
            StructureKind::MainHouse(_) | StructureKind::WorkerHouse(_) | StructureKind::Barn => {
                add(
                    buildings::building(structure, drop),
                    transform,
                    0,
                    None,
                    None,
                );
            }
            StructureKind::Corral => {
                let ground = |p: Vec3| {
                    let w = transform.transform_point(p.with_y(0.0));
                    (self.ground)(GroundVec2::new(w.x, -w.z)) - transform.translation.y
                };
                for side in props::corral(structure, &ground) {
                    let (a, b) = (
                        transform.transform_point(side.ends.0),
                        transform.transform_point(side.ends.1),
                    );
                    let (a, b) = (GroundVec2::new(a.x, -a.z), GroundVec2::new(b.x, -b.z));
                    let fade = ThinOccluder {
                        a,
                        b,
                        radius: 0.5,
                        bottom: transform.translation.y - drop - 0.5,
                        top: transform.translation.y + structure.height + 0.6,
                    };
                    add(side.shape, transform, 1, Some(fade), None);
                }
            }
            StructureKind::Windpump => {
                let fade = ThinOccluder::post(
                    at,
                    props::WHEEL_RADIUS,
                    self.terrain,
                    structure.height + 2.5,
                );
                add(props::prop(structure, drop), transform, 1, Some(fade), None);
                let (hub, turn) = props::rotor_place(structure);
                let wheel = transform * Transform::from_translation(hub).with_rotation(turn);
                let speed = rotor_speed(structure);
                for (surface, mesh) in &self.kit.rotor {
                    out.push(Piece {
                        look: Look::Mesh(mesh.clone(), self.surfaces.get(*surface)),
                        transform: wheel,
                        range: Some(ranges()[1].clone()),
                        fade: Some(fade),
                        rotor: Some(WindmillRotor { speed }),
                        tag: Tag::Part(tag),
                        stand_in,
                    });
                }
            }
            StructureKind::Tank => {
                add(props::prop(structure, drop), transform, 0, None, None);
            }
            StructureKind::Haystack => {
                let fade = ThinOccluder::post(at, reach * 0.8, self.terrain, structure.height);
                add(props::prop(structure, drop), transform, 0, Some(fade), None);
            }
            StructureKind::Trough => {
                add(props::prop(structure, drop), transform, 1, None, None);
            }
            StructureKind::WreckedWagon => {
                // The carreta's frame stands in, sunk to its axles.
                let fade = ThinOccluder::post(at, reach, self.terrain, structure.height + 1.0);
                let sunk = transform * Transform::from_xyz(0.0, -0.25, 0.0);
                add(props::prop(structure, drop), sunk, 1, Some(fade), None);
            }
            _ => {
                // The well, the hitching post and the cart.
                let fade = ThinOccluder::post(at, reach, self.terrain, structure.height + 1.0);
                add(props::prop(structure, drop), transform, 1, Some(fade), None);
            }
        }
    }

    /// The shared range, fade shape and rotor speed `structure`'s slot
    /// model is drawn with: the same as its procedural pieces'.
    fn slot_look(
        &self,
        structure: &Structure,
        at: GroundVec2,
        reach: f32,
    ) -> (VisibilityRange, Option<ThinOccluder>, f32) {
        let terrain = self.terrain;
        let h = structure.height;
        match structure.kind {
            StructureKind::Windpump => (
                ranges()[1].clone(),
                Some(ThinOccluder::post(
                    at,
                    props::WHEEL_RADIUS,
                    terrain,
                    h + 2.5,
                )),
                rotor_speed(structure),
            ),
            StructureKind::Haystack => (
                ranges()[0].clone(),
                Some(ThinOccluder::post(at, reach * 0.8, terrain, h)),
                0.0,
            ),
            StructureKind::Trough => (ranges()[1].clone(), None, 0.0),
            _ => (
                ranges()[1].clone(),
                Some(ThinOccluder::post(at, reach, terrain, h + 1.0)),
                0.0,
            ),
        }
    }

    /// A planted tree on a tree spot: a vegetation model's trunk (or its
    /// stand-in's) drawn slender, under one of the species' shared
    /// procedural crowns ([`crown`]), picked by its variant.
    fn tree(&self, structure: &Structure, tag: SettlementPart, out: &mut Vec<Piece>) {
        let at = structure.footprint.center();
        let h = structure.height;
        let poplar = structure.kind == StructureKind::Poplar;
        // Trunk width for its height: the poplar's slender, the
        // eucalyptus's spreading into its crown.
        let width = if poplar { 0.22 } else { 0.5 };
        let paint = &self.kit.trees[poplar as usize];
        let ground = self.terrain.height(at.x, at.y);
        let turned = Transform::from_translation(world_from_ground(at, (self.ground)(at) - 0.1))
            .with_rotation(Quat::from_rotation_y(
                (structure.variant % 360) as f32 / 57.3,
            ));
        // The crown's whole reach, so the camera never looks through
        // leaves that do not fade.
        let crown_radius = self.kit.crown_reach[poplar as usize] * h;
        let fade = ThinOccluder {
            a: at,
            b: at,
            radius: crown_radius,
            bottom: ground - 0.5,
            top: ground + h,
        };
        let range = visible_within(TREE_FAR, TREE_FADE);
        // A tree slot's model is fitted per meter of the spot's height.
        let slot = self.slot_use(structure.kind);
        if !matches!(slot, SlotUse::Procedural) {
            let per_meter = Transform::from_scale(Vec3::splat(h));
            out.extend(self.slot_scene(
                structure.kind,
                turned * per_meter,
                range.clone(),
                Some(fade),
                0.0,
                Tag::Part(tag),
            ));
            if matches!(slot, SlotUse::Ready(_)) {
                return;
            }
        }
        let stand_in = matches!(slot, SlotUse::Loading(_));
        let mut push = |mesh: Handle<Mesh>, bark: bool, transform: Transform| {
            out.push(Piece {
                look: Look::Mesh(mesh, paint[if bark { 0 } else { 1 }].clone()),
                transform,
                range: Some(range.clone()),
                fade: Some(fade),
                rotor: None,
                tag: Tag::Part(tag),
                stand_in,
            });
        };
        let model = Model::CommonTree((structure.variant % 5) as u8 + 1);
        if let Some(parts) = self.models.and_then(|m| m.finished_parts(model)) {
            for (_, mesh) in parts.iter().filter(|(f, _)| *f == Finish::Bark) {
                push(
                    mesh.clone(),
                    true,
                    turned.with_scale(Vec3::new(h * width, h, h * width)),
                );
            }
        } else if let Some(kit) = self.stand_ins {
            let k = h / reference_height(Species::Calden);
            let scale = Vec3::new(k * width * 1.6, k, k * width * 1.6);
            let [(trunk, _), _] = kit.tree(Species::Calden);
            push(trunk, true, turned.with_scale(scale));
        } else {
            // No trunk to carry it: no floating crown either.
            return;
        }
        let crowns = &self.kit.crowns[poplar as usize];
        let crown = crowns[(structure.variant / 7) as usize % crowns.len()].clone();
        push(crown, false, turned.with_scale(Vec3::splat(h)));
    }
}

/// Whether chunk `coord` owns `settlement`: its hub lies in it (west and
/// south edges in), as [`SettlementLayout::in_area`] counts.
fn owns(coord: ChunkCoord, size: f32, settlement: &Settlement) -> bool {
    ChunkCoord::containing(settlement.hub, size) == coord
}

/// Every piece the settlements owned by chunk `coord` draw, and the
/// roadside wrecks lying in it.
fn chunk_pieces(
    painter: &Painter,
    layout: &SettlementLayout,
    coord: ChunkCoord,
    size: f32,
    meshes: &mut Assets<Mesh>,
) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = layout
        .settlements
        .iter()
        .enumerate()
        .filter(|(_, s)| owns(coord, size, s))
        .flat_map(|(k, s)| painter.settlement(k as u32, s, meshes))
        .collect();
    for (k, wreck) in layout.roadside.iter().enumerate() {
        if ChunkCoord::containing(wreck.at, size) == coord {
            pieces.extend(painter.roadside(k as u32, wreck, meshes));
        }
    }
    pieces
}

/// How fast a windpump's wheel turns, radians per second: a little
/// different on every one.
fn rotor_speed(structure: &Structure) -> f32 {
    1.4 + (structure.variant % 89) as f32 / 89.0
}

/// What laying the settlements reads.
type LayInputs<'a> = (
    Res<'a, WorldSettlements>,
    Res<'a, WorldTerrain>,
    Res<'a, WorldChunks>,
    Option<Res<'a, ChunkDetail>>,
);

/// The models laying the settlements may draw.
type LayModels<'a> = (
    Option<Res<'a, AssetServer>>,
    Option<Res<'a, SlotModels>>,
    Res<'a, ModelSlots>,
    Option<Res<'a, YardModels>>,
);

/// Draws the settlements of each newly built chunk as its children. A chunk
/// that vanishes before the commands apply simply gets none.
fn lay_settlements(
    mut commands: Commands,
    (layout, terrain, world, detail): LayInputs,
    (surfaces, kit): (Option<Res<SettlementMaterials>>, Option<Res<SettlementKit>>),
    (models, stand_ins): (Option<Res<VegetationModels>>, Option<Res<FloraKit>>),
    (server, slots, fits, yard): LayModels,
    chunks: Query<(Entity, &TerrainChunk), Without<SettlementsLaid>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(surfaces), Some(kit), Some(detail)) = (surfaces, kit, detail) else {
        return;
    };
    let size = world.0.chunk_size;
    let terrain = &terrain.0;
    let ground = |p: GroundVec2| mesh_height(terrain, p, size, detail.resolution);
    let painter = Painter {
        terrain,
        ground: &ground,
        surfaces: &surfaces,
        kit: &kit,
        models: models.as_deref(),
        stand_ins: stand_ins.as_deref(),
        server: server.as_deref(),
        slots: slots.as_deref(),
        fits: &fits,
        yard: yard.as_deref(),
    };
    for (chunk, coord) in &chunks {
        let mut laid = commands.entity(chunk);
        laid.try_insert(SettlementsLaid);
        let pieces = chunk_pieces(&painter, &layout.0, coord.0, size, &mut meshes);
        if pieces.is_empty() {
            continue;
        }
        laid.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|children| {
                for p in pieces {
                    let mut part = children.spawn(p.transform);
                    match p.look {
                        Look::Mesh(mesh, material) => {
                            part.insert((Mesh3d(mesh), MeshMaterial3d(material)));
                        }
                        Look::Scene {
                            scene,
                            ready,
                            dress,
                        } => {
                            part.insert((dress, Visibility::default()));
                            if ready {
                                part.insert(WorldAssetRoot(scene));
                            } else {
                                part.insert(slots::SlotPending(scene));
                            }
                        }
                    }
                    if p.stand_in {
                        part.insert(StandIn);
                    }
                    if let Some(range) = p.range {
                        part.insert(range);
                    }
                    if let Some(fade) = p.fade {
                        part.insert(fade);
                    }
                    if let Some(rotor) = p.rotor {
                        part.insert(rotor);
                    }
                    match p.tag {
                        Tag::Part(tag) => part.insert(tag),
                        Tag::Tracks(k) => part.insert(TrackSurface(k)),
                        Tag::Yards(k) => part.insert(YardSurface(k)),
                        Tag::Prop(prop) => part.insert(prop),
                    };
                }
            });
        });
    }
}

/// Triangles `structure` draws, wheel and trees aside: what the budget
/// tests and the startup log count.
pub fn structure_triangles(structure: &Structure) -> usize {
    match structure.kind {
        StructureKind::MainHouse(_) | StructureKind::WorkerHouse(_) | StructureKind::Barn => {
            buildings::building(structure, 0.3).triangles()
        }
        StructureKind::Corral => props::corral(structure, &|_| 0.0)
            .iter()
            .map(|s| s.shape.triangles())
            .sum(),
        StructureKind::Windpump => {
            props::prop(structure, 0.3).triangles() + props::rotor().triangles()
        }
        k if k.is_tree() => 0,
        _ => props::prop(structure, 0.3).triangles(),
    }
}

#[cfg(test)]
mod tests;
