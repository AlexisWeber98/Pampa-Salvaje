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
//! - the eucalyptus grove and the poplar row, grown from the vegetation
//!   models (or their stand-ins), slender and recoloured per species;
//! - the tracks (huellas) and the trodden yards as decals over the drawn
//!   ground ([`track`]).
//!
//! Corral sides, windpumps, wells, carts, haystacks and trees fade out of
//! the camera's way ([`ThinOccluder`]); houses and barns stop the camera
//! instead (the game's camera occluders). Everything is drawn within a few
//! shared [`VisibilityRange`]s: Bevy keeps a GPU slot for every distinct
//! range for good (see [`crate::flora::TUFT_FADE_STEPS`]).

pub mod buildings;
pub mod materials;
pub mod props;
pub mod shape;
pub mod track;

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{
    Aabb, ChunkCoord, Settlement, SettlementLayout, Side, Species, Structure, StructureKind,
    Terrain, Track, Vec2 as GroundVec2,
};

pub use materials::{SettlementMaterials, Surface};

use crate::character::finish::Finish;
use crate::coords::world_from_ground;
use crate::flora::{
    FloraKit, Model, TREE_FADE, TREE_FAR, ThinOccluder, VegetationModels, reference_height, tinted,
    visible_within,
};
use crate::terrain::{
    ChunkDetail, ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain, mesh_height,
};
use crate::town::WindmillRotor;
use shape::Shape;

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
            .add_systems(Startup, (materials::setup_materials, setup_kit))
            // Repacks textures the frame they arrive (they are inserted in
            // `PreUpdate`), before the render world first copies them.
            .add_systems(Update, materials::finish_textures)
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
    commands.insert_resource(SettlementKit { rotor, trees });
}

/// What one chunk child is.
#[derive(Clone, Copy, Debug)]
enum Tag {
    Part(SettlementPart),
    Tracks(u32),
    Yards(u32),
}

/// One entity to spawn under a chunk.
struct Piece {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    range: Option<VisibilityRange>,
    fade: Option<ThinOccluder>,
    rotor: Option<WindmillRotor>,
    tag: Tag,
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
        pieces
    }

    /// A world-space decal: no range, like the roads.
    fn decal(&self, mesh: Handle<Mesh>, surface: Surface, tag: Tag) -> Piece {
        Piece {
            mesh,
            material: self.surfaces.get(surface),
            transform: Transform::default(),
            range: None,
            fade: None,
            rotor: None,
            tag,
        }
    }

    fn structure(
        &self,
        structure: &Structure,
        tag: SettlementPart,
        meshes: &mut Assets<Mesh>,
        out: &mut Vec<Piece>,
    ) {
        if structure.kind.is_tree() {
            self.tree(structure, tag, out);
            return;
        }
        let (transform, drop) = self.place(structure);
        let at = structure.footprint.center();
        let size = structure.footprint.size();
        let reach = size.x.max(size.y) / 2.0;
        let mut add = |shape: Shape,
                       transform: Transform,
                       range: usize,
                       fade: Option<ThinOccluder>,
                       rotor: Option<WindmillRotor>| {
            for (surface, mesh) in shape.into_meshes() {
                out.push(Piece {
                    mesh: meshes.add(mesh),
                    material: self.surfaces.get(surface),
                    transform,
                    range: Some(ranges()[range].clone()),
                    fade,
                    rotor,
                    tag: Tag::Part(tag),
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
                let speed = 1.4 + (structure.variant % 89) as f32 / 89.0;
                for (surface, mesh) in &self.kit.rotor {
                    out.push(Piece {
                        mesh: mesh.clone(),
                        material: self.surfaces.get(*surface),
                        transform: wheel,
                        range: Some(ranges()[1].clone()),
                        fade: Some(fade),
                        rotor: Some(WindmillRotor { speed }),
                        tag: Tag::Part(tag),
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
            _ => {
                // The well, the hitching post and the cart.
                let fade = ThinOccluder::post(at, reach, self.terrain, structure.height + 1.0);
                add(props::prop(structure, drop), transform, 1, Some(fade), None);
            }
        }
    }

    /// A planted tree on a tree spot: a vegetation model drawn tall and
    /// slender, or its stand-in; nothing when neither is there.
    fn tree(&self, structure: &Structure, tag: SettlementPart, out: &mut Vec<Piece>) {
        let at = structure.footprint.center();
        let h = structure.height;
        let poplar = structure.kind == StructureKind::Poplar;
        // Crown width for its height: the poplar a narrow column.
        let width = if poplar { 0.22 } else { 0.5 };
        let paint = &self.kit.trees[poplar as usize];
        let ground = self.terrain.height(at.x, at.y);
        let turned = Transform::from_translation(world_from_ground(at, (self.ground)(at) - 0.1))
            .with_rotation(Quat::from_rotation_y(
                (structure.variant % 360) as f32 / 57.3,
            ));
        let fade = ThinOccluder {
            a: at,
            b: at,
            radius: 0.8 * width * h / 2.0,
            bottom: ground - 0.5,
            top: ground + h,
        };
        let range = visible_within(TREE_FAR, TREE_FADE);
        let mut push = |mesh: Handle<Mesh>, bark: bool, transform: Transform| {
            out.push(Piece {
                mesh,
                material: paint[if bark { 0 } else { 1 }].clone(),
                transform,
                range: Some(range.clone()),
                fade: Some(fade),
                rotor: None,
                tag: Tag::Part(tag),
            });
        };
        let model = Model::CommonTree((structure.variant % 5) as u8 + 1);
        if let Some(parts) = self.models.and_then(|m| m.finished_parts(model)) {
            for (finish, mesh) in parts {
                push(
                    mesh.clone(),
                    *finish == Finish::Bark,
                    turned.with_scale(Vec3::new(h * width, h, h * width)),
                );
            }
        } else if let Some(kit) = self.stand_ins {
            let k = h / reference_height(Species::Calden);
            let scale = Vec3::new(k * width * 1.6, k, k * width * 1.6);
            for (n, (mesh, _)) in kit.tree(Species::Calden).into_iter().enumerate() {
                push(mesh, n == 0, turned.with_scale(scale));
            }
        }
    }
}

/// Whether chunk `coord` owns `settlement`: its hub lies in it (west and
/// south edges in), as [`SettlementLayout::in_area`] counts.
fn owns(coord: ChunkCoord, size: f32, settlement: &Settlement) -> bool {
    ChunkCoord::containing(settlement.hub, size) == coord
}

/// Every piece the settlements owned by chunk `coord` draw.
fn chunk_pieces(
    painter: &Painter,
    layout: &SettlementLayout,
    coord: ChunkCoord,
    size: f32,
    meshes: &mut Assets<Mesh>,
) -> Vec<Piece> {
    layout
        .settlements
        .iter()
        .enumerate()
        .filter(|(_, s)| owns(coord, size, s))
        .flat_map(|(k, s)| painter.settlement(k as u32, s, meshes))
        .collect()
}

/// What laying the settlements reads.
type LayInputs<'a> = (
    Res<'a, WorldSettlements>,
    Res<'a, WorldTerrain>,
    Res<'a, WorldChunks>,
    Option<Res<'a, ChunkDetail>>,
);

/// Draws the settlements of each newly built chunk as its children. A chunk
/// that vanishes before the commands apply simply gets none.
fn lay_settlements(
    mut commands: Commands,
    (layout, terrain, world, detail): LayInputs,
    (surfaces, kit): (Option<Res<SettlementMaterials>>, Option<Res<SettlementKit>>),
    (models, stand_ins): (Option<Res<VegetationModels>>, Option<Res<FloraKit>>),
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
                    let mut part =
                        children.spawn((Mesh3d(p.mesh), MeshMaterial3d(p.material), p.transform));
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
