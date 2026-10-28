//! Native flora on the streamed chunks: caldén and chañar and grass tufts,
//! placed by [`pampa_core::FloraSite`].
//!
//! Shrubs (piquillín, jarilla and the chañar thickets of the fachinal) are
//! the same kit's low multi-stem forms ([`shrub_variant`]), on the same
//! single tree visibility range. Without the kit they are not drawn.
//!
//! Trees are the textured town tree kit's caldén and chañar
//! ([`crate::tree::Species::FIELD`]: seeded variants with photographic bark
//! and alpha-masked leaf cards), scaled to the height core planted
//! ([`kit_variant`]). Without the kit they fall back to the low-poly CC0
//! models from the Ultimate Nature Pack by Quaternius (see
//! [`load_vegetation`]), recoloured and rescaled in code for the dry 1919
//! pampa, with their faceted normals smoothed when baked (see
//! [`smoothing`]); the tufts, stones and stumps are those models. When a
//! model is missing, the faceted procedural stand-ins below are drawn
//! instead, and only then.
//!
//! Every mesh and material is built once and shared, so Bevy batches
//! identical draws. A chunk's flora are its children and go with it when it
//! is dropped; every model pick, turn and size is seeded by the plant's
//! placement, so a reloaded chunk looks exactly the same. Distance culling
//! ([`VisibilityRange`]) keeps the drawn triangles low without any level of
//! detail swap: a tree is the same model out to the dust fog and fades out
//! inside it, and tufts stay solid near the rider and thin out smoothly
//! farther away.
//!
//! Trees are solid: the follow camera pulls in round their trunks instead.
//! Thin things (fences, signposts and lamps elsewhere) carry a
//! [`ThinOccluder`]: when one comes between the camera and the rider (the
//! [`SightLine`] the game sets each frame), the [`ThinFadePlugin`] turns it
//! see-through instead of the camera pulling in, and restores it once the
//! view clears. Only those indexed near the sight line are looked at.

use std::collections::HashMap;
use std::time::Instant;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::mesh::{PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use pampa_core::flora::Age;
use pampa_core::{FloraSite, Shrub, ShrubKind, Species, Tree, Tuft, TuftKind, Vec2 as GroundVec2};

use crate::AssetRoot;
use crate::character::finish::Finish;
use crate::coords::world_from_ground;
use crate::obj::{self, MaterialColors, ObjGroup, ObjModel};
use crate::smooth::smooth_normals;
use crate::terrain::{ChunkStreaming, TerrainChunk, WorldTerrain};
use crate::tree::{self, TreeKit};

/// Where the flora grows. Insert it before the plugin runs; without it,
/// chunks grow nothing.
#[derive(Resource, Debug, Clone)]
pub struct WorldFlora(pub FloraSite);

/// A drawn tree part (trunk, crown or a model part), a child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreePart(pub Species);

/// A drawn shrub part (stems or leaves), a child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrubPart(pub ShrubKind);

/// A drawn grass tuft or weed, a child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrassTuft(pub TuftKind);

/// A stump or fallen log near a caldén, a child of its chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloraAccent;

/// Marks a chunk whose flora is grown.
#[derive(Component)]
struct FloraGrown;

/// Loading the vegetation models, in `Startup`. Anything that draws them
/// (the terrain's stones and shrubs) runs after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VegetationLoad;

/// Grows flora on terrain chunks as they are streamed in. Needs the
/// [`crate::TerrainPlugin`].
pub struct FloraPlugin;

impl Plugin for FloraPlugin {
    fn build(&self, app: &mut App) {
        tree::ensure_plugin(app);
        app.init_resource::<WorldTerrain>()
            .add_systems(Startup, (load_vegetation.in_set(VegetationLoad), setup_kit))
            // Right after streaming, like the track: new chunks get their
            // flora the frame they appear, before transforms propagate.
            .add_systems(
                PostUpdate,
                grow_flora
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// The segment from the point the camera looks at (the rider) to the
/// camera, in world space; the game sets it every frame. `None` fades
/// nothing.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct SightLine(pub Option<(Vec3, Vec3)>);

/// Something thin that fades out of the way of the camera instead of
/// pulling it in: a vertical slab along ground segment `a`–`b` (a point for
/// a tree or a post), `radius` meters thick on either side, from height
/// `bottom` to `top`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ThinOccluder {
    pub a: GroundVec2,
    pub b: GroundVec2,
    pub radius: f32,
    pub bottom: f32,
    pub top: f32,
}

impl ThinOccluder {
    /// A post or a tree standing at `at` on `terrain`, `radius` wide and
    /// `height` tall.
    pub fn post(at: GroundVec2, radius: f32, terrain: &pampa_core::Terrain, height: f32) -> Self {
        let ground = terrain.height(at.x, at.y);
        Self {
            a: at,
            b: at,
            radius,
            bottom: ground - 0.5,
            top: ground + height,
        }
    }

    /// Whether this stands in the way of the line of sight from `from` to
    /// `to` (world space).
    pub fn blocks(&self, from: Vec3, to: Vec3) -> bool {
        let (p, q) = (
            GroundVec2::new(from.x, -from.z),
            GroundVec2::new(to.x, -to.z),
        );
        let Some((gap, t)) = closest_approach(p, q, self.a, self.b) else {
            return false;
        };
        let y = from.y + (to.y - from.y) * t;
        gap < self.radius && y > self.bottom && y < self.top
    }

    /// Its ground footprint's bounding box, grown by its radius.
    fn reach(&self) -> (GroundVec2, GroundVec2) {
        let r = self.radius;
        (
            GroundVec2::new(self.a.x.min(self.b.x) - r, self.a.y.min(self.b.y) - r),
            GroundVec2::new(self.a.x.max(self.b.x) + r, self.a.y.max(self.b.y) + r),
        )
    }
}

/// Least distance between ground segments `p`–`q` (the sight line) and
/// `a`–`b`, and where along `p`–`q` (`0..=1`) it is reached. `None` for a
/// sight line without length.
fn closest_approach(
    p: GroundVec2,
    q: GroundVec2,
    a: GroundVec2,
    b: GroundVec2,
) -> Option<(f32, f32)> {
    let d = q - p;
    let span = d.dot(d);
    if span <= 1e-9 {
        return None;
    }
    let on_sight = |x: GroundVec2| ((x - p).dot(d) / span).clamp(0.0, 1.0);
    let at = |t: f32| p + d * t;
    // Crossing: distance 0 where they cross.
    let e = b - a;
    let denom = d.x * e.y - d.y * e.x;
    if denom.abs() > 1e-9 {
        let w = a - p;
        let t = (w.x * e.y - w.y * e.x) / denom;
        let u = (w.x * d.y - w.y * d.x) / denom;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return Some((0.0, t));
        }
    }
    let on_occluder = |x: GroundVec2| {
        let span = e.dot(e);
        if span <= 1e-9 {
            return a;
        }
        a + e * ((x - a).dot(e) / span).clamp(0.0, 1.0)
    };
    let candidates = [
        (a, on_sight(a)),
        (b, on_sight(b)),
        (on_occluder(p), 0.0),
        (on_occluder(q), 1.0),
    ];
    candidates
        .into_iter()
        .map(|(x, t)| (x.distance(at(t)), t))
        .min_by(|l, r| l.0.total_cmp(&r.0))
}

/// Marks a faded thin occluder, keeping its own material to restore.
#[derive(Component, Debug, Clone)]
pub struct Faded(pub Handle<StandardMaterial>);

/// How opaque a faded thin occluder is.
pub const FADED_ALPHA: f32 = 0.22;
/// Side of a cell of the thin occluders' index, in meters.
const FADE_CELL: f32 = 16.0;

/// Thin occluders by grid cell, kept in step as they come and go.
#[derive(Resource, Debug, Default)]
struct ThinIndex {
    cells: HashMap<(i32, i32), Vec<Entity>>,
    of: HashMap<Entity, Vec<(i32, i32)>>,
}

fn fade_cell(p: GroundVec2) -> (i32, i32) {
    (
        (p.x / FADE_CELL).floor() as i32,
        (p.y / FADE_CELL).floor() as i32,
    )
}

fn fade_cells((lo, hi): (GroundVec2, GroundVec2)) -> impl Iterator<Item = (i32, i32)> {
    let ((x0, y0), (x1, y1)) = (fade_cell(lo), fade_cell(hi));
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
}

/// See-through copies of the materials of faded occluders, made once.
#[derive(Resource, Debug, Default)]
struct FadeMaterials(HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>);

/// Fades thin occluders that come between the camera and the rider (see
/// [`SightLine`]) and restores them when the view clears.
pub struct ThinFadePlugin;

impl Plugin for ThinFadePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SightLine>()
            .init_resource::<ThinIndex>()
            .init_resource::<FadeMaterials>()
            .add_systems(PostUpdate, (index_thin, fade_thin).chain());
    }
}

impl ThinIndex {
    /// Takes `entity` out of the index, if it is in.
    fn forget(&mut self, entity: Entity) {
        let Some(cells) = self.of.remove(&entity) else {
            return;
        };
        for cell in cells {
            if let Some(list) = self.cells.get_mut(&cell) {
                list.retain(|&e| e != entity);
                if list.is_empty() {
                    self.cells.remove(&cell);
                }
            }
        }
    }
}

/// Keeps the index in step: occluders added, changed (moved or resized:
/// indexed again where they now reach) and removed.
fn index_thin(
    mut index: ResMut<ThinIndex>,
    changed: Query<(Entity, &ThinOccluder), Changed<ThinOccluder>>,
    mut removed: RemovedComponents<ThinOccluder>,
) {
    for entity in removed.read() {
        index.forget(entity);
    }
    for (entity, occluder) in &changed {
        index.forget(entity);
        let cells: Vec<(i32, i32)> = fade_cells(occluder.reach()).collect();
        for &cell in &cells {
            index.cells.entry(cell).or_default().push(entity);
        }
        index.of.insert(entity, cells);
    }
}

fn fade_thin(
    mut commands: Commands,
    sight: Res<SightLine>,
    index: Res<ThinIndex>,
    mut occluders: Query<(
        &ThinOccluder,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&Faded>,
    )>,
    faded: Query<(Entity, &Faded)>,
    mut cache: ResMut<FadeMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut blocking: Vec<Entity> = Vec::new();
    if let Some((from, to)) = sight.0 {
        let (p, q) = (
            GroundVec2::new(from.x, -from.z),
            GroundVec2::new(to.x, -to.z),
        );
        let lo = GroundVec2::new(p.x.min(q.x), p.y.min(q.y));
        let hi = GroundVec2::new(p.x.max(q.x), p.y.max(q.y));
        // Wide enough for the thickest occluder's radius.
        let pad = GroundVec2::new(FADE_CELL, FADE_CELL);
        for cell in fade_cells((lo - pad, hi + pad)) {
            for &entity in index.cells.get(&cell).into_iter().flatten() {
                if blocking.contains(&entity) {
                    continue;
                }
                if occluders
                    .get(entity)
                    .is_ok_and(|(o, _, _)| o.blocks(from, to))
                {
                    blocking.push(entity);
                }
            }
        }
    }
    for (entity, original) in &faded {
        if !blocking.contains(&entity)
            && let Ok((_, mut material, _)) = occluders.get_mut(entity)
        {
            material.0 = original.0.clone();
            commands.entity(entity).remove::<Faded>();
        }
    }
    for entity in blocking {
        let Ok((_, mut material, was)) = occluders.get_mut(entity) else {
            continue;
        };
        if was.is_some() {
            continue;
        }
        let original = material.0.clone();
        let see_through = match cache.0.get(&original.id()) {
            Some(copy) => copy.clone(),
            None => {
                // Not loaded yet: stay solid this frame rather than cache a
                // see-through copy of a blank material for good.
                let Some(mut copy) = materials.get(&original).cloned() else {
                    continue;
                };
                copy.alpha_mode = AlphaMode::Blend;
                copy.base_color.set_alpha(FADED_ALPHA);
                let copy = materials.add(copy);
                cache.0.insert(original.id(), copy.clone());
                copy
            }
        };
        material.0 = see_through;
        commands.entity(entity).insert(Faded(original));
    }
}

/// Tufts drawn per chunk, at most.
pub const TUFTS_DRAWN: usize = 300;
/// Where trees stop being drawn, in meters from the camera: past the dust
/// fog ([`crate::atmosphere::FOG_VISIBILITY`]), so they fade out inside it.
/// A tree is the same model at every distance; there is no far stand-in.
pub const TREE_FAR: f32 = 200.0;
/// Width of the trees' dithered fade-out, ending at [`TREE_FAR`].
pub(crate) const TREE_FADE: f32 = 15.0;
/// Tufts within this distance of the rider are always drawn solid...
pub const TUFT_SOLID: f32 = 40.0;
/// ...wherever the orbit camera stands: at most this far from the rider
/// (the orbit's longest zoom, `game/src/orbit.rs`).
pub const CAMERA_REACH: f32 = 25.0;
/// Beyond that, each tuft starts fading at its own seeded distance, up to
/// this much farther: the grass thins out smoothly between.
pub const TUFT_SPREAD: f32 = 50.0;
/// Fade starts across [`TUFT_SPREAD`] come in this many steps. Bevy gives
/// every distinct `VisibilityRange` a GPU slot it never frees, and past
/// 65,535 slots it culls every unranged mesh (ground, buildings, the train),
/// so tufts must share a few ranges instead of one each.
pub const TUFT_FADE_STEPS: f32 = 8.0;
/// Width of a tuft's dithered fade-out.
const TUFT_FADE: f32 = 15.0;
/// Where stumps and logs stop being drawn, and the width of their fade.
const ACCENT_FAR: f32 = 120.0;
const ACCENT_FADE: f32 = 15.0;
/// Share of mature caldenes (out of 256) that are dead or sparse snags.
const SNAG_SHARE: u32 = 28;
/// How often each live caldén form ([`tree::CALDEN_LIVE`] of them) is
/// drawn, out of 100: the classic umbrella most, the odd shapes less.
const CALDEN_FORM_WEIGHTS: [u32; tree::CALDEN_LIVE] = [18, 12, 10, 12, 10, 12, 10, 8, 8];
/// The forms an old giant takes: broad, full crowns, never a snag.
const GIANT_FORMS: [usize; 4] = [0, 3, 5, 7];
/// How often each chañar form is drawn, out of 100.
const CHANAR_FORM_WEIGHTS: [u32; tree::CHANAR_VARIANTS] = [24, 16, 16, 14, 16, 14];
/// Share of caldenes with a stump or a fallen log beside them.
const ACCENT_SHARE: f32 = 0.06;
/// Share of tufts drawn as weeds, and the part of that share in flower.
const WEED_SHARE: f32 = 0.06;
const FLOWER_SHARE: f32 = 0.015;
/// A chañar thicket is this wide for its height (the bush model is wider).
const THICKET_WIDTH: f32 = 0.7;
/// How much wider the caldén's crown is drawn than the model's.
const CROWN_WIDEN: f32 = 0.25;
/// Grass tints from grey-green to dry yellow.
const GRASS_TINTS: usize = 4;

/// Where the vegetation models live, under the [`AssetRoot`].
pub const MODEL_DIR: &str = "models/vegetation/quaternius-ultimate-nature";

/// One of the shipped vegetation models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    CommonTree(u8),
    DeadTree(u8),
    Bush(u8),
    Grass,
    TallGrass,
    ShortGrass,
    Wheat,
    Plant(u8),
    Flowers,
    Rock(u8),
    Stump,
    Log,
}

/// What a model stands for in the game; it decides the recolouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A living caldén.
    Calden,
    /// A dead caldén.
    Snag,
    /// A chañar or jarilla thicket.
    Thicket,
    /// Grass tufts and paja brava.
    Grass,
    /// Weeds and wildflowers.
    Weed,
    /// Tosca stones.
    Stone,
    /// Stumps and fallen trunks.
    Deadwood,
}

impl Model {
    /// Every shipped model.
    pub fn all() -> Vec<Self> {
        let mut all = Vec::new();
        for n in 1..=5 {
            all.extend([Self::CommonTree(n), Self::DeadTree(n), Self::Plant(n)]);
        }
        all.extend([Self::Bush(1), Self::Bush(2)]);
        all.extend((1..=4).map(Self::Rock));
        all.extend([
            Self::Grass,
            Self::TallGrass,
            Self::ShortGrass,
            Self::Wheat,
            Self::Flowers,
            Self::Stump,
            Self::Log,
        ]);
        all
    }

    /// The model's OBJ file name.
    pub fn file(self) -> String {
        match self {
            Self::CommonTree(n) => format!("CommonTree_{n}.obj"),
            Self::DeadTree(n) => format!("CommonTree_Dead_{n}.obj"),
            Self::Bush(n) => format!("Bush_{n}.obj"),
            Self::Grass => "Grass.obj".into(),
            Self::TallGrass => "Grass_2.obj".into(),
            Self::ShortGrass => "Grass_Short.obj".into(),
            Self::Wheat => "Wheat.obj".into(),
            Self::Plant(n) => format!("Plant_{n}.obj"),
            Self::Flowers => "Flowers.obj".into(),
            Self::Rock(n) => format!("Rock_{n}.obj"),
            Self::Stump => "TreeStump.obj".into(),
            Self::Log => "WoodLog.obj".into(),
        }
    }

    pub fn role(self) -> Role {
        match self {
            Self::CommonTree(_) => Role::Calden,
            Self::DeadTree(_) => Role::Snag,
            Self::Bush(_) => Role::Thicket,
            Self::Grass | Self::TallGrass | Self::ShortGrass | Self::Wheat => Role::Grass,
            Self::Plant(_) | Self::Flowers => Role::Weed,
            Self::Rock(_) => Role::Stone,
            Self::Stump | Self::Log => Role::Deadwood,
        }
    }
}

/// Deterministic `[0, 1)` for a ground spot and a salt.
pub(crate) fn spot_unit(at: GroundVec2, salt: u32) -> f32 {
    jitter(at.x.to_bits() ^ at.y.to_bits().rotate_left(16), salt)
}

/// Byte `k` (0 to 3) of a tree's seeded look.
fn look_byte(tree: &Tree, k: u32) -> u32 {
    (tree.variant >> (8 * k)) & 0xFF
}

/// The index of `roll` (out of 256) among `weights`, each one's share of
/// their sum.
fn weighted(weights: &[u32], roll: u32) -> usize {
    let total: u32 = weights.iter().sum();
    let mut at = roll * total / 256;
    for (i, w) in weights.iter().enumerate() {
        if at < *w {
            return i;
        }
        at -= w;
    }
    weights.len() - 1
}

/// Whether `tree` is a caldén snag, dead or sparse: a seeded few of the
/// mature ones (old giants and the young stay alive).
pub(crate) fn is_snag(tree: &Tree) -> bool {
    tree.species == Species::Calden && tree.age == Age::Mature && look_byte(tree, 1) < SNAG_SHARE
}

/// The Quaternius model drawn for `tree` when the tree kit is not there
/// ([`kit_variant`] otherwise): caldenes are common trees, a seeded few of
/// them dead snags; chañares are bush thickets.
pub fn tree_model(tree: &Tree) -> Model {
    match tree.species {
        Species::Calden if is_snag(tree) => {
            Model::DeadTree((spot_unit(tree.at, 3) * 5.0) as u8 % 5 + 1)
        }
        Species::Calden => Model::CommonTree((tree.variant % 5) as u8 + 1),
        Species::Chanar => Model::Bush((tree.variant % 2) as u8 + 1),
    }
}

/// The tree kit's species and form drawn for `tree`, from its seed. A
/// caldén snag draws one of the kit's three snag forms, an old giant one of
/// the [`GIANT_FORMS`], any other caldén one of its live forms by their
/// weights; a chañar one of its forms by theirs.
pub(crate) fn kit_variant(tree: &Tree) -> (tree::Species, usize) {
    let roll = look_byte(tree, 0);
    match tree.species {
        Species::Calden if is_snag(tree) => (
            tree::Species::Calden,
            tree::CALDEN_LIVE + roll as usize * (tree::CALDEN_VARIANTS - tree::CALDEN_LIVE) / 256,
        ),
        Species::Calden if tree.age == Age::Old => (
            tree::Species::Calden,
            GIANT_FORMS[roll as usize * GIANT_FORMS.len() / 256],
        ),
        Species::Calden => (tree::Species::Calden, weighted(&CALDEN_FORM_WEIGHTS, roll)),
        Species::Chanar => (tree::Species::Chanar, weighted(&CHANAR_FORM_WEIGHTS, roll)),
    }
}

/// The tree kit's species and form drawn for `shrub`, from its seed.
pub(crate) fn shrub_variant(shrub: &Shrub) -> (tree::Species, usize) {
    let species = match shrub.kind {
        ShrubKind::Piquillin => tree::Species::Piquillin,
        ShrubKind::Jarilla => tree::Species::Jarilla,
        ShrubKind::Fachinal => tree::Species::Fachinal,
    };
    (
        species,
        (shrub.variant & 0xFF) as usize * tree::SHRUB_VARIANTS / 256,
    )
}

/// The bark tint and the leaf tint drawn for `shrub`, from its seed.
pub(crate) fn shrub_tints(shrub: &Shrub) -> (usize, usize) {
    let n = shrub_variant(shrub).0.tints();
    (
        ((shrub.variant >> 16) & 0xFF) as usize * n / 256,
        ((shrub.variant >> 24) & 0xFF) as usize * n / 256,
    )
}

/// The bark tint and the leaf tint drawn for `tree`, from its seed.
pub(crate) fn kit_tints(tree: &Tree) -> (usize, usize) {
    let n = match tree.species {
        Species::Calden => tree::Species::Calden,
        Species::Chanar => tree::Species::Chanar,
    }
    .tints();
    (
        look_byte(tree, 2) as usize * n / 256,
        look_byte(tree, 3) as usize * n / 256,
    )
}

/// The model drawn for `tuft`: tall paja is wheat-like paja brava or tall
/// grass, flechilla is grass or short grass, and a seeded few are weeds
/// and wildflowers.
pub fn tuft_model(tuft: &Tuft) -> Model {
    let roll = spot_unit(tuft.at, 7);
    if roll < FLOWER_SHARE {
        return Model::Flowers;
    }
    if roll < WEED_SHARE {
        return Model::Plant((spot_unit(tuft.at, 8) * 5.0) as u8 % 5 + 1);
    }
    let tall = spot_unit(tuft.at, 9) < 0.5;
    match (tuft.kind, tall) {
        (TuftKind::Paja, true) => Model::Wheat,
        (TuftKind::Paja, false) => Model::TallGrass,
        (TuftKind::Flechilla, true) => Model::Grass,
        (TuftKind::Flechilla, false) => Model::ShortGrass,
    }
}

/// The 1919 pampa's colour (sRGB) and finish for `role`'s MTL material
/// `name`, whose own colour was `kd`.
pub fn recolour(role: Role, name: &str, kd: [f32; 3]) -> ([f32; 3], Finish) {
    let dead = matches!(role, Role::Snag | Role::Deadwood);
    match (role, name) {
        // Weathered grey on dead wood, dark grey-brown bark on the living.
        (_, "Wood") if dead => ([0.42, 0.38, 0.33], Finish::Bark),
        (_, "Wood") => ([0.27, 0.22, 0.18], Finish::Bark),
        (_, "LightWood") => ([0.58, 0.49, 0.37], Finish::Bark),
        // Tosca: the pale beige limestone crust of the pampa.
        (_, "Rock") => ([0.74, 0.67, 0.54], Finish::Rock),
        // A dull olive crown.
        (Role::Calden, "DarkGreen") => ([0.26, 0.30, 0.17], Finish::Leaves),
        (Role::Calden, _) => ([0.34, 0.38, 0.21], Finish::Leaves),
        (Role::Thicket, _) => ([0.42, 0.46, 0.31], Finish::Leaves),
        // Pale straw; each tuft is tinted on top (see `grass_tint`).
        (Role::Grass, _) => ([0.86, 0.82, 0.66], Finish::Leaves),
        (Role::Weed, "Cyan") => ([0.52, 0.50, 0.68], Finish::Leaves),
        (Role::Weed, "Yellow") => ([0.70, 0.62, 0.34], Finish::Leaves),
        (Role::Weed, "Pink") => ([0.72, 0.52, 0.56], Finish::Leaves),
        (Role::Weed, "DarkGreen") => ([0.36, 0.40, 0.25], Finish::Leaves),
        (Role::Weed, _) => ([0.45, 0.47, 0.30], Finish::Leaves),
        (Role::Deadwood, "Mushroom_Top") => ([0.55, 0.42, 0.33], Finish::Leaves),
        (Role::Deadwood, "Mushroom_Bottom") => ([0.78, 0.72, 0.62], Finish::Leaves),
        (Role::Deadwood, _) => ([0.40, 0.42, 0.26], Finish::Leaves),
        // Anything else keeps its own colour, faded halfway to grey.
        _ => {
            let srgb = Color::linear_rgb(kd[0], kd[1], kd[2]).to_srgba();
            let rgb = [srgb.red, srgb.green, srgb.blue];
            let grey = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
            let finish = if role == Role::Stone {
                Finish::Rock
            } else {
                Finish::Leaves
            };
            (rgb.map(|c| 0.5 * (c + grey)), finish)
        }
    }
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How `role`'s models are smoothed when baked: the largest bend between
/// facets that still blends, in degrees, and how far the normals lean
/// toward straight up (0 none, 1 all the way). Crowns, bushes and weeds
/// blend almost every facet; bark and dead wood keep their broken edges a
/// little; tosca stones keep their hard ones. Grass blades are thin cards:
/// leaning their normals up lights a tuft like the ground under it rather
/// than darkening its shaded side.
pub fn smoothing(role: Role) -> (f32, f32) {
    match role {
        Role::Calden | Role::Thicket | Role::Weed => (60.0, 0.0),
        Role::Snag | Role::Deadwood => (50.0, 0.0),
        Role::Grass => (80.0, 0.5),
        Role::Stone => (40.0, 0.0),
    }
}

/// A finish and its positions, normals and colours, while baking.
type BakedPart = (Finish, Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 4]>);

/// `obj` as one mesh per finish, recoloured for `model`'s role into vertex
/// colours and scaled to stand on the origin 1 unit tall, so a transform
/// scale is its height in meters. Caldén crowns are widened. Vertices are
/// a little darker low down: cheap baked occlusion. The pack ships one
/// normal per face; they are smoothed here, once, per [`smoothing`]. A
/// group that is not whole triangles with a normal per position is skipped
/// with a warning.
pub fn bake(model: Model, obj: &ObjModel, colors: &MaterialColors) -> Vec<(Finish, Mesh)> {
    // A group must be whole triangles with one normal per position; a
    // malformed one is left out rather than shifting every later group.
    let groups: Vec<&ObjGroup> = obj
        .groups
        .iter()
        .filter(|g| {
            let whole = g.positions.len() == g.normals.len() && g.positions.len() % 3 == 0;
            if !whole {
                warn!(
                    "vegetation {model:?}: group `{}` skipped, {} positions for {} normals",
                    g.material,
                    g.positions.len(),
                    g.normals.len()
                );
            }
            whole
        })
        .collect();
    let ys = groups.iter().flat_map(|g| g.positions.iter().map(|p| p[1]));
    let (lo, hi) = ys.fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let height = (hi - lo).max(1e-3);
    let role = model.role();
    // Stood up and widened first, so the smooth normals fit the final shape.
    let stand = |p: &[f32; 3]| {
        let y = (p[1] - lo) / height;
        let widen = if role == Role::Calden {
            1.0 + CROWN_WIDEN * smoothstep(0.35, 0.75, y)
        } else {
            1.0
        };
        [p[0] / height * widen, y, p[2] / height * widen]
    };
    let stood: Vec<[f32; 3]> = groups
        .iter()
        .flat_map(|g| g.positions.iter().map(stand))
        .collect();
    let (angle, lift) = smoothing(role);
    let smooth = smooth_normals(&stood, angle);
    let mut parts: Vec<BakedPart> = Vec::new();
    // Each group's own slice of the positions and smooth normals.
    let mut start = 0;
    for group in groups {
        let end = start + group.positions.len();
        let (stood, smooth) = (&stood[start..end], &smooth[start..end]);
        start = end;
        let kd = colors.get(&group.material).copied().unwrap_or([0.5; 3]);
        let (rgb, finish) = recolour(role, &group.material, kd);
        let color = LinearRgba::from(Color::srgb(rgb[0], rgb[1], rgb[2]));
        let k = match parts.iter().position(|p| p.0 == finish) {
            Some(k) => k,
            None => {
                parts.push((finish, Vec::new(), Vec::new(), Vec::new()));
                parts.len() - 1
            }
        };
        let part = &mut parts[k];
        for ((&p, &s), n) in stood.iter().zip(smooth).zip(&group.normals) {
            let y = p[1];
            part.1.push(p);
            // A face wound against the file's own normal (a handful in the
            // pack) keeps the file's normal rather than shading inside out.
            let (file, s) = (Vec3::from_array(*n), Vec3::from_array(s));
            let s = if s.dot(file) < 0.0 { file } else { s };
            part.2
                .push(s.lerp(Vec3::Y, lift).normalize_or(Vec3::Y).to_array());
            let shade = 0.72 + 0.28 * y.clamp(0.0, 1.0);
            part.3.push([
                color.red * shade,
                color.green * shade,
                color.blue * shade,
                1.0,
            ]);
        }
    }
    parts
        .into_iter()
        .map(|(finish, positions, normals, colors)| {
            let mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors);
            (finish, mesh)
        })
        .collect()
}

/// The vegetation models, baked once at startup, and the few materials
/// every one of them shares.
#[derive(Resource, Clone)]
pub struct VegetationModels {
    parts: HashMap<Model, Vec<(Finish, Handle<Mesh>)>>,
    bark: Handle<StandardMaterial>,
    leaves: Handle<StandardMaterial>,
    weeds: Handle<StandardMaterial>,
    rock: Handle<StandardMaterial>,
    grass: [Handle<StandardMaterial>; GRASS_TINTS],
}

impl VegetationModels {
    /// Whether `model` was loaded.
    pub fn has(&self, model: Model) -> bool {
        self.parts.contains_key(&model)
    }

    /// `model`'s meshes and materials, the grass tinted `tint` (see
    /// [`grass_tint`]); `None` when it was not loaded.
    pub fn parts(
        &self,
        model: Model,
        tint: usize,
    ) -> Option<Vec<(Handle<Mesh>, Handle<StandardMaterial>)>> {
        let parts = self.parts.get(&model)?;
        Some(
            parts
                .iter()
                .map(|(finish, mesh)| (mesh.clone(), self.material(model, *finish, tint)))
                .collect(),
        )
    }

    /// `model`'s meshes with their finishes, for a caller that paints them
    /// with its own materials; `None` when it was not loaded.
    pub(crate) fn finished_parts(&self, model: Model) -> Option<&[(Finish, Handle<Mesh>)]> {
        self.parts.get(&model).map(Vec::as_slice)
    }

    fn material(&self, model: Model, finish: Finish, tint: usize) -> Handle<StandardMaterial> {
        match (model.role(), finish) {
            (_, Finish::Bark) => self.bark.clone(),
            (_, Finish::Rock) => self.rock.clone(),
            (Role::Grass, _) => self.grass[tint.min(GRASS_TINTS - 1)].clone(),
            (Role::Weed, _) => self.weeds.clone(),
            _ => self.leaves.clone(),
        }
    }
}

/// Which grass tint a tuft at `at` takes, from grey-green (0) to dry yellow:
/// the ground's dryness there, jittered per tuft.
pub fn grass_tint(dryness: f32, at: GroundVec2) -> usize {
    let k = dryness * (GRASS_TINTS - 1) as f32 + (spot_unit(at, 4) - 0.5) * 1.4;
    k.round().clamp(0.0, (GRASS_TINTS - 1) as f32) as usize
}

/// A white material with `finish`: vertex colours carry the paint.
fn finished(finish: Finish, double_sided: bool) -> StandardMaterial {
    tinted(finish, double_sided, Color::WHITE)
}

pub(crate) fn tinted(finish: Finish, double_sided: bool, base_color: Color) -> StandardMaterial {
    let (perceptual_roughness, metallic, reflectance) = finish.pbr();
    StandardMaterial {
        base_color,
        perceptual_roughness,
        metallic,
        reflectance,
        double_sided,
        cull_mode: (!double_sided).then_some(bevy::render::render_resource::Face::Back),
        ..default()
    }
}

/// Loads and bakes every vegetation model from the [`AssetRoot`]: the
/// "Ultimate Nature Pack by Quaternius, CC0 1.0; see
/// assets/models/vegetation/quaternius-ultimate-nature/CREDITS.md".
///
/// It runs once, at startup; chunks only reuse the handles. A missing or
/// broken file logs a warning and leaves that model out, so the
/// procedural stand-in is drawn in its place.
pub fn load_vegetation(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let started = Instant::now();
    let mut parts = HashMap::new();
    let mut triangles = 0;
    if let Some(root) = root {
        let dir = root.0.join(MODEL_DIR);
        for model in Model::all() {
            let path = dir.join(model.file());
            match obj::load(&path) {
                Ok((obj, colors)) => {
                    triangles += obj.triangles();
                    let baked = bake(model, &obj, &colors)
                        .into_iter()
                        .map(|(finish, mesh)| (finish, meshes.add(mesh)))
                        .collect();
                    parts.insert(model, baked);
                }
                Err(error) => warn!(
                    "vegetation model {} not loaded ({error}); drawing its stand-in",
                    path.display()
                ),
            }
        }
        info!(
            "vegetation: {} models, {triangles} triangles, loaded in {:.0} ms",
            parts.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    // Grey-green to dry yellow, multiplied over the pale straw paint.
    let green = Vec3::new(0.66, 0.72, 0.56);
    let dry = Vec3::new(1.0, 0.88, 0.58);
    let grass = std::array::from_fn(|k| {
        let c = green.lerp(dry, k as f32 / (GRASS_TINTS - 1) as f32);
        materials.add(tinted(Finish::Leaves, true, Color::srgb(c.x, c.y, c.z)))
    });
    commands.insert_resource(VegetationModels {
        parts,
        bark: materials.add(finished(Finish::Bark, false)),
        leaves: materials.add(finished(Finish::Leaves, false)),
        weeds: materials.add(finished(Finish::Leaves, true)),
        rock: materials.add(finished(Finish::Rock, false)),
        grass,
    });
}

/// Height the stand-in tree meshes are built at, per species; each tree is
/// scaled from it.
pub(crate) fn reference_height(species: Species) -> f32 {
    match species {
        Species::Calden => 7.5,
        Species::Chanar => 4.0,
    }
}

/// The procedural stand-in meshes and materials.
#[derive(Resource, Clone)]
pub struct FloraKit {
    calden_trunk: Handle<Mesh>,
    calden_crown: Handle<Mesh>,
    chanar_trunk: Handle<Mesh>,
    chanar_crown: Handle<Mesh>,
    paja: Handle<Mesh>,
    flechilla: Handle<Mesh>,
    bark: Handle<StandardMaterial>,
    leaves: Handle<StandardMaterial>,
    grass: Handle<StandardMaterial>,
}

impl FloraKit {
    pub(crate) fn tree(&self, species: Species) -> [(Handle<Mesh>, Handle<StandardMaterial>); 2] {
        let (trunk, crown) = match species {
            Species::Calden => (&self.calden_trunk, &self.calden_crown),
            Species::Chanar => (&self.chanar_trunk, &self.chanar_crown),
        };
        [
            (trunk.clone(), self.bark.clone()),
            (crown.clone(), self.leaves.clone()),
        ]
    }

    fn tuft(&self, kind: TuftKind) -> Handle<Mesh> {
        match kind {
            TuftKind::Paja => self.paja.clone(),
            TuftKind::Flechilla => self.flechilla.clone(),
        }
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FloraKit {
        calden_trunk: meshes.add(calden_trunk()),
        calden_crown: meshes.add(calden_crown()),
        chanar_trunk: meshes.add(chanar_trunk()),
        chanar_crown: meshes.add(chanar_crown()),
        paja: meshes.add(tuft_mesh(TuftKind::Paja)),
        flechilla: meshes.add(tuft_mesh(TuftKind::Flechilla)),
        bark: materials.add(finished(Finish::Bark, false)),
        leaves: materials.add(finished(Finish::Leaves, false)),
        grass: materials.add(finished(Finish::Leaves, true)),
    });
}

/// Drawn solid from the camera out to `far - fade` meters, then dithered
/// out by `far`. There is no near cut-off and no level-of-detail swap: a
/// plant never changes shape, it only fades at the far end.
pub(crate) fn visible_within(far: f32, fade: f32) -> VisibilityRange {
    VisibilityRange {
        start_margin: 0.0..0.0,
        end_margin: far - fade..far,
        use_aabb: false,
    }
}

#[derive(Clone, Copy)]
enum Plant {
    Tree(Species),
    Tuft(TuftKind),
    Shrub(ShrubKind),
    Accent,
}

/// One entity to spawn under a chunk.
type Part = (
    Mesh3d,
    MeshMaterial3d<StandardMaterial>,
    Transform,
    VisibilityRange,
    Plant,
);

/// What drawing a chunk's flora needs.
struct Painter<'a> {
    terrain: &'a pampa_core::Terrain,
    site: &'a FloraSite,
    kit: &'a FloraKit,
    models: Option<&'a VegetationModels>,
    trees: Option<&'a TreeKit>,
}

impl Painter<'_> {
    fn tree(&self, tree: &Tree, out: &mut Vec<Part>) {
        let base = world_from_ground(tree.at, self.terrain.height(tree.at.x, tree.at.y) - 0.1);
        let turned =
            Transform::from_translation(base).with_rotation(Quat::from_rotation_y(tree.yaw));
        let model = tree_model(tree);
        // Trees are solid: the follow camera pulls in round a trunk instead
        // of the tree fading out of its way.
        let plant = Plant::Tree(tree.species);
        let range = visible_within(TREE_FAR, TREE_FADE);
        if tree.species == Species::Calden {
            self.accent(tree, out);
        }
        if let Some(kit) = self.trees {
            // The textured tree kit, a form's two shared meshes scaled to
            // the height core planted and the width it gave the tree, in
            // the bark and leaf tints its seed draws.
            let (species, v) = kit_variant(tree);
            let (bark_tint, leaf_tint) = kit_tints(tree);
            let own = kit.species(species);
            let variant = &own.variants[v];
            let high = tree.height / variant.height;
            let scale = Vec3::new(high * tree.width, high, high * tree.width);
            let (bark, leaves) = (&own.bark_tints[bark_tint], &own.leaf_tints[leaf_tint]);
            for (mesh, material) in [(&variant.bark, bark), (&variant.leaves, leaves)] {
                let transform = turned.with_scale(scale);
                out.push(part(
                    mesh.clone(),
                    material.clone(),
                    transform,
                    range.clone(),
                    plant,
                ));
            }
            return;
        }
        let Some(parts) = self.models.and_then(|m| m.parts(model, 0)) else {
            // The model is missing: its stand-in, at every distance.
            let stand_in =
                turned.with_scale(Vec3::splat(tree.height / reference_height(tree.species)));
            for (mesh, material) in self.kit.tree(tree.species) {
                out.push(part(mesh, material, stand_in, range.clone(), plant));
            }
            return;
        };
        let h = tree.height;
        let scale = match tree.species {
            Species::Calden => Vec3::splat(h),
            Species::Chanar => Vec3::new(h * THICKET_WIDTH, h, h * THICKET_WIDTH),
        };
        for (mesh, material) in parts {
            out.push(part(
                mesh,
                material,
                turned.with_scale(scale),
                range.clone(),
                plant,
            ));
        }
    }

    /// A shrub: a kit form's two shared meshes scaled to the height and
    /// width core gave it. Without the kit it is not drawn.
    fn shrub(&self, shrub: &Shrub, out: &mut Vec<Part>) {
        let Some(kit) = self.trees else {
            return;
        };
        let base = world_from_ground(shrub.at, self.terrain.height(shrub.at.x, shrub.at.y) - 0.05);
        let turned =
            Transform::from_translation(base).with_rotation(Quat::from_rotation_y(shrub.yaw));
        let (species, v) = shrub_variant(shrub);
        let (bark_tint, leaf_tint) = shrub_tints(shrub);
        let own = kit.species(species);
        let variant = &own.variants[v];
        let high = shrub.height / variant.height;
        let scale = Vec3::new(high * shrub.width, high, high * shrub.width);
        let range = visible_within(TREE_FAR, TREE_FADE);
        let (bark, leaves) = (&own.bark_tints[bark_tint], &own.leaf_tints[leaf_tint]);
        for (mesh, material) in [(&variant.bark, bark), (&variant.leaves, leaves)] {
            out.push(part(
                mesh.clone(),
                material.clone(),
                turned.with_scale(scale),
                range.clone(),
                Plant::Shrub(shrub.kind),
            ));
        }
    }

    /// A seeded few caldenes get a stump or a fallen log beside them.
    fn accent(&self, tree: &Tree, out: &mut Vec<Part>) {
        if spot_unit(tree.at, 11) >= ACCENT_SHARE {
            return;
        }
        let model = if spot_unit(tree.at, 13) < 0.5 {
            Model::Stump
        } else {
            Model::Log
        };
        let Some(parts) = self.models.and_then(|m| m.parts(model, 0)) else {
            return;
        };
        let angle = spot_unit(tree.at, 12) * std::f32::consts::TAU;
        let at = tree.at + GroundVec2::new(angle.cos(), angle.sin()) * (0.6 * tree.crown_radius());
        let size = 0.5 * (0.85 + 0.3 * spot_unit(tree.at, 14));
        let transform = Transform::from_translation(world_from_ground(
            at,
            self.terrain.height(at.x, at.y) - 0.03,
        ))
        .with_rotation(Quat::from_rotation_y(angle * 3.0))
        .with_scale(Vec3::splat(size));
        for (mesh, material) in parts {
            out.push(part(
                mesh,
                material,
                transform,
                visible_within(ACCENT_FAR, ACCENT_FADE),
                Plant::Accent,
            ));
        }
    }

    fn tuft(&self, tuft: &Tuft, out: &mut Vec<Part>) {
        let base = world_from_ground(tuft.at, self.terrain.height(tuft.at.x, tuft.at.y));
        let transform =
            Transform::from_translation(base).with_rotation(Quat::from_rotation_y(tuft.yaw));
        // Solid near the rider; past that each tuft fades at one of a few
        // seeded distances, so the grass thins out in fine steps.
        let step = (spot_unit(tuft.at, 5) * TUFT_FADE_STEPS).floor() / TUFT_FADE_STEPS;
        let fade_from = TUFT_SOLID + CAMERA_REACH + TUFT_SPREAD * step;
        let range = visible_within(fade_from + TUFT_FADE, TUFT_FADE);
        let plant = Plant::Tuft(tuft.kind);
        let model = tuft_model(tuft);
        let tint = grass_tint(self.site.dryness(tuft.at), tuft.at);
        match self.models.and_then(|m| m.parts(model, tint)) {
            Some(parts) => {
                let size = if model.role() == Role::Weed { 1.1 } else { 1.0 } * tuft.size;
                for (mesh, material) in parts {
                    out.push(part(
                        mesh,
                        material,
                        transform.with_scale(Vec3::splat(size)),
                        range.clone(),
                        plant,
                    ));
                }
            }
            None => out.push(part(
                self.kit.tuft(tuft.kind),
                self.kit.grass.clone(),
                transform.with_scale(Vec3::splat(tuft.size)),
                range,
                plant,
            )),
        }
    }
}

fn part(
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    range: VisibilityRange,
    plant: Plant,
) -> Part {
    (
        Mesh3d(mesh),
        MeshMaterial3d(material),
        transform,
        range,
        plant,
    )
}

/// Grows the flora of every chunk built since the last run. A chunk that
/// vanishes before the commands apply simply gets none.
fn grow_flora(
    mut commands: Commands,
    flora: Option<Res<WorldFlora>>,
    terrain: Res<WorldTerrain>,
    kit: Option<Res<FloraKit>>,
    models: Option<Res<VegetationModels>>,
    trees: Option<Res<TreeKit>>,
    chunks: Query<(Entity, &TerrainChunk), Without<FloraGrown>>,
) {
    let (Some(flora), Some(kit)) = (flora, kit) else {
        return;
    };
    let painter = Painter {
        terrain: &terrain.0,
        site: &flora.0,
        kit: &kit,
        models: models.as_deref(),
        trees: trees.as_deref(),
    };
    for (chunk, coord) in &chunks {
        let grown = flora.0.chunk(coord.0);
        let mut parts: Vec<Part> =
            Vec::with_capacity(grown.trees.len() * 4 + grown.shrubs.len() * 2 + TUFTS_DRAWN);
        for tree in &grown.trees {
            painter.tree(tree, &mut parts);
        }
        for shrub in &grown.shrubs {
            painter.shrub(shrub, &mut parts);
        }
        for tuft in grown.tufts.iter().take(TUFTS_DRAWN) {
            painter.tuft(tuft, &mut parts);
        }
        let mut grown_chunk = commands.entity(chunk);
        grown_chunk.try_insert(FloraGrown);
        if parts.is_empty() {
            continue;
        }
        grown_chunk.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|children| {
                for (mesh, material, transform, range, plant) in parts {
                    let mut part = children.spawn((mesh, material, transform, range));
                    match plant {
                        Plant::Tree(species) => part.insert(TreePart(species)),
                        Plant::Tuft(kind) => part.insert(GrassTuft(kind)),
                        Plant::Shrub(kind) => part.insert(ShrubPart(kind)),
                        Plant::Accent => part.insert(FloraAccent),
                    };
                }
            });
        });
    }
}

/// Triangles in a triangle-list mesh, flat-shaded or indexed.
pub fn triangles(mesh: &Mesh) -> usize {
    mesh.indices()
        .map_or(mesh.count_vertices(), |indices| indices.len())
        / 3
}

/// A flat-shaded triangle soup with per-vertex colours.
#[derive(Default)]
struct Facets {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

/// An sRGB colour as linear vertex paint.
fn paint(r: f32, g: f32, b: f32) -> [f32; 4] {
    LinearRgba::from(Color::srgb(r, g, b)).to_f32_array()
}

fn shade(c: [f32; 4], k: f32) -> [f32; 4] {
    [c[0] * k, c[1] * k, c[2] * k, c[3]]
}

impl Facets {
    fn tri(&mut self, [a, b, c]: [Vec3; 3], colors: [[f32; 4]; 3]) {
        self.positions.extend([a, b, c].map(|v| v.to_array()));
        self.colors.extend(colors);
    }

    /// A tapered tube through `rings` (center, radius), `sides` facets
    /// round, closed at the top.
    fn loft(&mut self, rings: &[(Vec3, f32)], sides: u32, color: [f32; 4]) {
        let frames: Vec<Vec<Vec3>> = rings
            .iter()
            .enumerate()
            .map(|(k, &(center, radius))| {
                let next = rings.get(k + 1).or(rings.get(k.wrapping_sub(1)));
                let axis = next.map_or(Vec3::Y, |n| {
                    let d = if k + 1 < rings.len() {
                        n.0 - center
                    } else {
                        center - n.0
                    };
                    d.normalize_or(Vec3::Y)
                });
                let side = axis.any_orthonormal_vector();
                let other = axis.cross(side);
                (0..sides)
                    .map(|s| {
                        let a = s as f32 / sides as f32 * std::f32::consts::TAU;
                        center + (side * a.cos() + other * a.sin()) * radius
                    })
                    .collect()
            })
            .collect();
        let n = rings.len();
        for k in 0..n.saturating_sub(1) {
            // Darker low down: the trunk's foot sits in shade.
            let (lo, hi) = (
                shade(color, 0.7 + 0.3 * k as f32 / (n - 1) as f32),
                shade(color, 0.7 + 0.3 * (k + 1) as f32 / (n - 1) as f32),
            );
            for s in 0..sides as usize {
                let t = (s + 1) % sides as usize;
                let (a, b) = (frames[k][s], frames[k][t]);
                let (c, d) = (frames[k + 1][t], frames[k + 1][s]);
                self.tri([a, c, b], [lo, hi, lo]);
                self.tri([a, d, c], [lo, hi, hi]);
            }
        }
        if let (Some(top), Some(&(center, _))) = (frames.last(), rings.last()) {
            let c = shade(color, 1.0);
            for s in 0..sides as usize {
                let t = (s + 1) % sides as usize;
                self.tri([top[s], top[t], center], [c; 3]);
            }
        }
    }

    /// A lumpy leaf cluster: an icosphere stretched to `radii`, each vertex
    /// pushed in or out a little by `seed`. Its underside is darker.
    fn blob(&mut self, center: Vec3, radii: Vec3, seed: u32, color: [f32; 4]) {
        let (points, indices) = icosphere();
        let lumpy: Vec<Vec3> = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let k = 0.82 + 0.3 * jitter(seed, i as u32);
                center + *p * radii * k
            })
            .collect();
        for tri in indices.as_chunks::<3>().0 {
            let v = [lumpy[tri[0]], lumpy[tri[1]], lumpy[tri[2]]];
            let colors = v.map(|p| {
                let up = ((p.y - center.y) / radii.y).clamp(-1.0, 1.0);
                shade(color, 0.78 + 0.22 * (up * 0.5 + 0.5))
            });
            self.tri(v, colors);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.compute_normals();
        mesh
    }
}

/// Unit icosphere (one subdivision: 80 faces), as points and indices.
fn icosphere() -> (Vec<Vec3>, Vec<usize>) {
    let mesh = Sphere::new(1.0)
        .mesh()
        .ico(1)
        .expect("valid ico subdivision");
    let points = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|&v| Vec3::from(v)).collect(),
        _ => Vec::new(),
    };
    let indices = mesh
        .indices()
        .map(|i| i.iter().collect())
        .unwrap_or_default();
    (points, indices)
}

/// Deterministic `[0, 1)` from a seed and an index.
fn jitter(seed: u32, i: u32) -> f32 {
    let mut v = seed ^ i.wrapping_mul(0x9e37_79b9);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v >> 8) as f32 / (1u32 << 24) as f32
}

/// Caldén bark: dark, grey-brown.
fn calden_bark() -> [f32; 4] {
    paint(0.27, 0.22, 0.18)
}

/// Chañar bark: smooth and greenish.
fn chanar_bark() -> [f32; 4] {
    paint(0.46, 0.49, 0.34)
}

/// A caldén 7.5 m tall: a short thick trunk forking into three limbs.
fn calden_trunk() -> Mesh {
    let mut f = Facets::default();
    let bark = calden_bark();
    let fork = Vec3::new(0.15, 2.2, 0.0);
    f.loft(
        &[
            (Vec3::ZERO, 0.3),
            (Vec3::new(0.05, 1.1, 0.02), 0.24),
            (fork, 0.2),
        ],
        7,
        bark,
    );
    for (k, angle) in [0.3_f32, 2.4, 4.4].into_iter().enumerate() {
        let out = Vec3::new(angle.cos(), 0.0, angle.sin());
        let mid = fork + out * 1.2 + Vec3::Y * 1.3;
        let tip = fork + out * (2.4 + 0.3 * k as f32) + Vec3::Y * 2.7;
        f.loft(&[(fork, 0.16), (mid, 0.11), (tip, 0.06)], 6, bark);
    }
    f.into_mesh()
}

/// A caldén's broad umbrella crown: a wide flat central cluster and four
/// around it, from about 4.3 m to the top.
fn calden_crown() -> Mesh {
    let mut f = Facets::default();
    let leaves = paint(0.34, 0.38, 0.21);
    f.blob(
        Vec3::new(0.0, 6.2, 0.0),
        Vec3::new(2.8, 1.1, 2.8),
        11,
        leaves,
    );
    for (k, angle) in [0.3_f32, 1.9, 3.3, 4.8].into_iter().enumerate() {
        let out = Vec3::new(angle.cos(), 0.0, angle.sin()) * 2.6;
        let tone = shade(leaves, 0.94 + 0.04 * k as f32);
        f.blob(
            out + Vec3::Y * (5.6 + 0.2 * k as f32),
            Vec3::new(1.8, 0.95, 1.7),
            20 + k as u32,
            tone,
        );
    }
    f.into_mesh()
}

/// A chañar 4 m tall: a slim leaning trunk splitting into two stems.
fn chanar_trunk() -> Mesh {
    let mut f = Facets::default();
    let bark = chanar_bark();
    let fork = Vec3::new(0.2, 1.6, 0.05);
    f.loft(
        &[
            (Vec3::ZERO, 0.13),
            (Vec3::new(0.1, 0.8, 0.0), 0.11),
            (fork, 0.09),
        ],
        6,
        bark,
    );
    for angle in [0.6_f32, 3.6] {
        let out = Vec3::new(angle.cos(), 0.0, angle.sin());
        f.loft(
            &[(fork, 0.08), (fork + out * 0.8 + Vec3::Y * 1.2, 0.04)],
            5,
            bark,
        );
    }
    f.into_mesh()
}

/// A chañar's small rounded crown of three clusters.
fn chanar_crown() -> Mesh {
    let mut f = Facets::default();
    let leaves = paint(0.40, 0.46, 0.25);
    f.blob(
        Vec3::new(0.2, 3.2, 0.0),
        Vec3::new(1.3, 0.8, 1.2),
        31,
        leaves,
    );
    f.blob(
        Vec3::new(0.9, 2.9, 0.4),
        Vec3::new(0.8, 0.6, 0.8),
        32,
        shade(leaves, 0.95),
    );
    f.blob(
        Vec3::new(-0.5, 2.8, -0.5),
        Vec3::new(0.8, 0.6, 0.8),
        33,
        shade(leaves, 1.03),
    );
    f.into_mesh()
}

/// A grass tuft one meter tall: thin blades fanned out from the root,
/// green-brown at the base and pale straw at the dry tips.
fn tuft_mesh(kind: TuftKind) -> Mesh {
    let (blades, spread, base, tip) = match kind {
        TuftKind::Paja => (9, 0.35, paint(0.42, 0.42, 0.24), paint(0.78, 0.70, 0.46)),
        TuftKind::Flechilla => (7, 0.55, paint(0.55, 0.52, 0.32), paint(0.86, 0.80, 0.58)),
    };
    let mut f = Facets::default();
    for b in 0..blades {
        let a = b as f32 / blades as f32 * std::f32::consts::TAU + jitter(7, b) * 0.6;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        let across = Vec3::new(-out.z, 0.0, out.x) * 0.05;
        let lean = spread * (0.6 + 0.6 * jitter(9, b));
        let top = out * lean + Vec3::Y * (0.75 + 0.25 * jitter(5, b));
        let root = out * 0.04;
        f.tri([root - across, root + across, top], [base, base, tip]);
    }
    f.into_mesh()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmosphere::FOG_VISIBILITY;
    use crate::terrain::{StreamFocus, TerrainPlugin};
    use crate::tree::{CALDEN_LIVE, Species as KitSpecies};
    use pampa_core::flora::Age;
    use pampa_core::{ChunkCoord, RailLine, Terrain, Vec2 as GroundVec2, WorldConfig};

    fn site() -> FloraSite {
        FloraSite::new(
            WorldConfig::default(),
            &Terrain::default(),
            &RailLine::default(),
        )
    }

    fn app(focus: GroundVec2) -> App {
        app_with(focus, None, 1, 4)
    }

    fn assets() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
    }

    fn app_with(
        focus: GroundVec2,
        root: Option<std::path::PathBuf>,
        radius: i32,
        resolution: u32,
    ) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldFlora(site()))
            .add_plugins((
                TerrainPlugin {
                    radius,
                    budget: 64,
                    resolution,
                },
                FloraPlugin,
            ));
        if let Some(root) = root {
            app.insert_resource(AssetRoot(root));
        }
        app
    }

    /// Distinct asset ids in `ids`.
    fn distinct<T: PartialEq>(ids: impl Iterator<Item = T>) -> usize {
        let mut seen: Vec<T> = Vec::new();
        for id in ids {
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        seen.len()
    }

    fn grove_trees() -> Vec<Tree> {
        let flora = site();
        (-12..12)
            .flat_map(|x| (-12..12).map(move |y| ChunkCoord::new(x, y)))
            .flat_map(|c| flora.trees(c))
            .collect()
    }

    /// Like [`app_with`] over the shipped assets, with an asset server and
    /// a GPU that takes BC7, so the tree kit's materials are textured.
    fn textured_app(focus: GroundVec2) -> App {
        use bevy::image::{CompressedImageFormatSupport, CompressedImageFormats};
        let formats = CompressedImageFormats::BC;
        let root = AssetRoot(assets());
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, crate::gltf::asset_plugin(&root)))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<Image>()
            .register_asset_loader(bevy::image::ImageLoader::new(formats))
            .insert_resource(root)
            .insert_resource(CompressedImageFormatSupport(formats))
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldFlora(site()))
            .add_plugins((
                TerrainPlugin {
                    radius: 1,
                    budget: 64,
                    resolution: 4,
                },
                FloraPlugin,
            ));
        app
    }

    /// Every tree part of `app`: its species, mesh, material, transform
    /// and range.
    type DrawnTree = (
        Species,
        AssetId<Mesh>,
        AssetId<StandardMaterial>,
        Transform,
        VisibilityRange,
    );

    fn drawn_trees(app: &mut App) -> Vec<DrawnTree> {
        let mut q = app.world_mut().query::<(
            &TreePart,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &Transform,
            &VisibilityRange,
        )>();
        q.iter(app.world())
            .map(|(p, m, mat, t, r)| (p.0, m.id(), mat.id(), *t, r.clone()))
            .collect()
    }

    #[test]
    fn a_field_tree_never_fades_out_of_the_cameras_way() {
        let mut app = textured_app(grove_chunk());
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<Has<ThinOccluder>, With<TreePart>>();
        let parts: Vec<bool> = q.iter(app.world()).collect();
        assert!(parts.len() > 20, "{}", parts.len());
        assert!(parts.iter().all(|fades| !fades));
    }

    #[test]
    fn field_trees_wear_the_textured_tree_kit() {
        let focus = grove_chunk();
        let mut app = textured_app(focus);
        app.update();
        let kit = app.world().resource::<TreeKit>().clone();
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        for species in KitSpecies::FIELD {
            let k = kit.species(species);
            let (bark, leaves) = (
                materials.get(&k.bark).unwrap(),
                materials.get(&k.leaves).unwrap(),
            );
            assert!(bark.base_color_texture.is_some(), "{species:?} bark");
            assert!(bark.normal_map_texture.is_some(), "{species:?} bark");
            assert!(leaves.base_color_texture.is_some(), "{species:?} leaves");
            assert_eq!(leaves.alpha_mode, AlphaMode::Mask(0.5), "{species:?}");
        }
        // Every tree is a bark and a leaf entity of its species' kit.
        let drawn = drawn_trees(&mut app);
        let planted: Vec<Tree> = WorldConfig::default()
            .chunks_near(focus, 1)
            .iter()
            .flat_map(|&c| site().trees(c))
            .collect();
        assert_eq!(drawn.len(), planted.len() * 2);
        for species in [Species::Calden, Species::Chanar] {
            assert!(planted.iter().any(|t| t.species == species), "{species:?}");
        }
        let mut variants = std::collections::HashSet::new();
        let mut tinted = std::collections::HashSet::new();
        for (species, mesh, material, ..) in &drawn {
            let k = kit.species(kit_variant_species(*species));
            let bark = k.variants.iter().any(|v| v.bark.id() == *mesh);
            let leaves = k.variants.iter().any(|v| v.leaves.id() == *mesh);
            assert!(bark != leaves, "{species:?} draws a kit mesh");
            let tints = if bark { &k.bark_tints } else { &k.leaf_tints };
            assert!(tints.iter().any(|m| m.id() == *material), "{species:?}");
            variants.insert((*species as u8, *mesh));
            tinted.insert((*species as u8, *material));
        }
        // Several seeded variants of each: the fields do not repeat.
        for species in [Species::Calden, Species::Chanar] {
            let n = variants.iter().filter(|v| v.0 == species as u8).count();
            assert!(n >= 8, "{species:?}: {n} meshes");
            // And their bark and leaves in several tints.
            let m = tinted.iter().filter(|v| v.0 == species as u8).count();
            assert!(m >= 4, "{species:?}: {m} materials");
        }
    }

    fn kit_variant_species(species: Species) -> KitSpecies {
        match species {
            Species::Calden => KitSpecies::Calden,
            Species::Chanar => KitSpecies::Chanar,
        }
    }

    #[test]
    fn a_kit_tree_stands_at_its_core_size_and_keeps_its_range() {
        let mut app = textured_app(grove_chunk());
        app.update();
        let kit = app.world().resource::<TreeKit>().clone();
        let range = visible_within(TREE_FAR, TREE_FADE);
        let mut widths = std::collections::HashSet::new();
        for (species, mesh, _, transform, drawn_range) in drawn_trees(&mut app) {
            let k = kit.species(kit_variant_species(species));
            let v = k
                .variants
                .iter()
                .find(|v| v.bark.id() == mesh || v.leaves.id() == mesh)
                .unwrap();
            // Scaled to a height core plants, and wider or narrower by the
            // width it gave the tree, evenly across and no more.
            let (lo, hi) = species.height_range();
            let h = v.height * transform.scale.y;
            assert!((lo - 0.01..=hi + 0.01).contains(&h), "{species:?} {h}");
            assert_eq!(transform.scale.x, transform.scale.z);
            let width = transform.scale.x / transform.scale.y;
            assert!((0.8..=1.4).contains(&width), "{species:?} {width}");
            widths.insert((width * 50.0).round() as i32);
            // One shared range, as the Quaternius trees had.
            assert!(drawn_range == range);
        }
        // Trees come narrow and broad, not all as modelled.
        assert!(widths.len() >= 8, "{widths:?}");
    }

    #[test]
    fn a_trees_kit_variant_and_tints_are_its_seed() {
        use std::collections::{HashMap, HashSet};
        let flora = site();
        let trees: Vec<Tree> = (-19..1)
            .flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y)))
            .flat_map(|c| flora.trees(c))
            .collect();
        let mut forms: HashMap<Species, HashSet<usize>> = HashMap::new();
        let mut tints: HashMap<Species, (HashSet<usize>, HashSet<usize>)> = HashMap::new();
        let (mut snags, mut mature, mut caldenes) = (0, 0, 0);
        for tree in &trees {
            let (species, v) = kit_variant(tree);
            assert_eq!((species, v), kit_variant(tree));
            assert!(v < species.variants(), "{species:?} {v}");
            let (bark, leaf) = kit_tints(tree);
            assert!(bark < species.tints() && leaf < species.tints());
            let seen = tints.entry(tree.species).or_default();
            seen.0.insert(bark);
            seen.1.insert(leaf);
            forms.entry(tree.species).or_default().insert(v);
            if tree.species == Species::Calden {
                assert_eq!(species, KitSpecies::Calden);
                caldenes += 1;
                mature += usize::from(tree.age == Age::Mature);
                // A snag draws a dead or sparse form, and only the mature
                // trees are snags; an old giant is a broad live form.
                assert_eq!(v >= CALDEN_LIVE, is_snag(tree));
                snags += usize::from(is_snag(tree));
                if is_snag(tree) {
                    assert_eq!(tree.age, Age::Mature);
                }
                if tree.age == Age::Old {
                    assert!(GIANT_FORMS.contains(&v), "{v}");
                }
            } else {
                assert_eq!(species, KitSpecies::Chanar);
            }
        }
        // Every form of both species is drawn, among the many trees.
        assert_eq!(forms[&Species::Calden].len(), KitSpecies::Calden.variants());
        assert_eq!(forms[&Species::Chanar].len(), KitSpecies::Chanar.variants());
        for species in [Species::Calden, Species::Chanar] {
            assert_eq!(tints[&species].0.len(), crate::tree::FIELD_TINTS);
            assert_eq!(tints[&species].1.len(), crate::tree::FIELD_TINTS);
        }
        // The live forms share out by their weights: the classic form is
        // the commonest, no live form is rare or rules.
        let live: Vec<&Tree> = trees
            .iter()
            .filter(|t| t.species == Species::Calden && !is_snag(t) && t.age != Age::Old)
            .collect();
        for (form, weight) in CALDEN_FORM_WEIGHTS.iter().enumerate() {
            let n = live.iter().filter(|t| kit_variant(t).1 == form).count();
            let share = n as f32 / live.len() as f32;
            let want = *weight as f32 / 100.0;
            assert!(
                (share - want).abs() < 0.03,
                "form {form}: {share} for {want}"
            );
        }
        let share = snags as f32 / mature as f32;
        println!("{snags} snags of {mature} mature of {caldenes} caldenes");
        assert!((0.08..=0.14).contains(&share), "{share}");
    }

    /// The parts a [`Painter`] draws for `tree`, with the tree kit or without.
    fn painted(app: &App, tree: &Tree, with_kit: bool) -> Vec<Part> {
        let kit = app.world().resource::<FloraKit>();
        let models = app.world().resource::<VegetationModels>();
        let trees = app.world().resource::<TreeKit>();
        let terrain = &app.world().resource::<WorldTerrain>().0;
        let site = site();
        let painter = Painter {
            terrain,
            site: &site,
            kit,
            models: Some(models),
            trees: with_kit.then_some(trees),
        };
        let mut out = Vec::new();
        painter.tree(tree, &mut out);
        out
    }

    fn mesh_ids(parts: &[Part]) -> Vec<AssetId<Mesh>> {
        parts.iter().map(|p| p.0.0.id()).collect()
    }

    #[test]
    fn without_the_kit_the_quaternius_models_are_drawn() {
        let mut app = app_with(grove_chunk(), Some(assets()), 1, 4);
        app.update();
        let models = app.world().resource::<VegetationModels>().clone();
        for tree in grove_trees().iter().take(40) {
            let without = painted(&app, tree, false);
            let accent = |p: &&Part| !matches!(p.4, Plant::Accent);
            let drawn: Vec<_> = without.iter().filter(accent).map(|p| p.0.0.id()).collect();
            let want: Vec<_> = models
                .parts(tree_model(tree), 0)
                .unwrap()
                .iter()
                .map(|(m, _)| m.id())
                .collect();
            assert_eq!(drawn, want, "{tree:?}");
            // And the kit draws something else, two parts.
            let with = painted(&app, tree, true);
            assert_eq!(with.iter().filter(accent).count(), 2);
            assert!(
                with.iter()
                    .filter(accent)
                    .all(|p| !drawn.contains(&p.0.0.id()))
            );
        }
    }

    #[test]
    fn the_stand_ins_remain_when_the_kit_and_the_models_are_both_missing() {
        let mut app = app_with(grove_chunk(), None, 1, 4);
        app.update();
        let kit = app.world().resource::<FloraKit>().clone();
        let terrain = app.world().resource::<WorldTerrain>().0.clone();
        let site = site();
        let painter = Painter {
            terrain: &terrain,
            site: &site,
            kit: &kit,
            models: None,
            trees: None,
        };
        for tree in grove_trees().iter().take(20) {
            let mut out = Vec::new();
            painter.tree(tree, &mut out);
            let want = match tree.species {
                Species::Calden => [kit.calden_trunk.id(), kit.calden_crown.id()],
                Species::Chanar => [kit.chanar_trunk.id(), kit.chanar_crown.id()],
            };
            assert_eq!(mesh_ids(&out), want.to_vec(), "{tree:?}");
        }
    }

    #[test]
    fn kit_caldenes_keep_their_stump_and_log_accents() {
        let mut app = app_with(grove_chunk(), Some(assets()), 1, 4);
        app.update();
        let trees = grove_trees();
        let with_accent =
            |t: &&Tree| t.species == Species::Calden && spot_unit(t.at, 11) < ACCENT_SHARE;
        let tree = trees
            .iter()
            .find(with_accent)
            .expect("a caldén with an accent");
        let parts = painted(&app, tree, true);
        let accents = parts
            .iter()
            .filter(|p| matches!(p.4, Plant::Accent))
            .count();
        assert!(accents >= 1, "{accents}");
        // The kit's tree under it: two parts, as without an accent.
        assert_eq!(parts.len() - accents, 2);
        // A chañar never gets one.
        let chanar = trees.iter().find(|t| t.species == Species::Chanar).unwrap();
        assert!(
            painted(&app, chanar, true)
                .iter()
                .all(|p| !matches!(p.4, Plant::Accent))
        );
    }

    /// The triangles of a kit form's bark and leaves.
    fn form_triangles(meshes: &Assets<Mesh>, variant: &crate::tree::TreeVariant) -> usize {
        [&variant.bark, &variant.leaves]
            .iter()
            .map(|h| triangles(meshes.get(*h).unwrap()))
            .sum()
    }

    #[test]
    fn a_dense_chunk_of_kit_trees_stays_within_the_per_tree_budget() {
        let flora = site();
        let densest = (-12..12)
            .flat_map(|x| (-12..12).map(move |y| ChunkCoord::new(x, y)))
            .max_by_key(|&c| flora.trees(c).len())
            .expect("chunks");
        let trees = flora.trees(densest);
        let mut app = app(densest.center(WorldConfig::default().chunk_size));
        app.update();
        let kit = app.world().resource::<TreeKit>().clone();
        let meshes = app.world().resource::<Assets<Mesh>>();
        // The per-tree range comes from the forms themselves, not from a
        // number a seed could undercut: the lightest and the heaviest of
        // every field form (the stub snag is the lightest caldén).
        let forms: Vec<usize> = KitSpecies::FIELD
            .iter()
            .flat_map(|&s| kit.species(s).variants.iter())
            .map(|v| form_triangles(meshes, v))
            .collect();
        let (lightest, heaviest) = (
            *forms.iter().min().expect("forms"),
            *forms.iter().max().expect("forms"),
        );
        // The town trees' range (0.7-1.2k) at the top, the snags under it.
        assert!(heaviest <= 1200, "{heaviest}");
        assert!(lightest >= 200, "{lightest}");
        let mut total = 0;
        for tree in &trees {
            let (species, v) = kit_variant(tree);
            let tris = form_triangles(meshes, &kit.species(species).variants[v]);
            assert!(
                (lightest..=heaviest).contains(&tris),
                "{species:?} {v}: {tris}"
            );
            total += tris;
        }
        println!(
            "densest chunk: {} trees, {total} triangles (forms {lightest}-{heaviest})",
            trees.len()
        );
        assert!(trees.len() >= 60, "{}", trees.len());
        // Every one at the top of the range, at the most, over the budget.
        assert!(total <= FloraSite::TREE_BUDGET * heaviest, "{total}");
    }

    /// The chunk with the most shrubs near the origin.
    fn shrub_chunk() -> ChunkCoord {
        let flora = site();
        (-12..12)
            .flat_map(|x| (-12..12).map(move |y| ChunkCoord::new(x, y)))
            .max_by_key(|&c| flora.shrubs(c).len())
            .expect("chunks")
    }

    #[test]
    fn shrubs_wear_the_kit_in_seeded_forms_on_the_one_tree_range() {
        let coord = shrub_chunk();
        let mut app = textured_app(coord.center(WorldConfig::default().chunk_size));
        app.update();
        let kit = app.world().resource::<TreeKit>().clone();
        let planted: Vec<Shrub> = WorldConfig::default()
            .chunks_near(coord.center(WorldConfig::default().chunk_size), 1)
            .iter()
            .flat_map(|&c| site().shrubs(c))
            .collect();
        let mut q = app.world_mut().query::<(
            &ShrubPart,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &Transform,
            &VisibilityRange,
        )>();
        let drawn: Vec<_> = q
            .iter(app.world())
            .map(|(p, m, mat, t, r)| (p.0, m.id(), mat.id(), *t, r.clone()))
            .collect();
        // A bark and a leaf entity each.
        assert_eq!(drawn.len(), planted.len() * 2);
        assert!(planted.len() > 200, "{}", planted.len());
        let range = visible_within(TREE_FAR, TREE_FADE);
        let mut forms = std::collections::HashSet::new();
        for (kind, mesh, material, transform, drawn_range) in &drawn {
            // The one shared tree range: no per-entity ranges.
            assert!(*drawn_range == range);
            let (species, _) = shrub_variant(&Shrub {
                at: GroundVec2::ZERO,
                kind: *kind,
                height: 1.0,
                width: 1.0,
                yaw: 0.0,
                variant: 0,
            });
            let k = kit.species(species);
            let bark = k.variants.iter().any(|v| v.bark.id() == *mesh);
            let leaves = k.variants.iter().any(|v| v.leaves.id() == *mesh);
            assert!(bark != leaves, "{kind:?} draws a kit mesh");
            let tints = if bark { &k.bark_tints } else { &k.leaf_tints };
            assert!(tints.iter().any(|m| m.id() == *material), "{kind:?}");
            // Scaled evenly across, within the heights core planted.
            let v = k
                .variants
                .iter()
                .find(|v| v.bark.id() == *mesh || v.leaves.id() == *mesh)
                .unwrap();
            assert_eq!(transform.scale.x, transform.scale.z);
            let (lo, hi) = kind.height_range();
            let h = v.height * transform.scale.y;
            assert!((lo - 0.01..=hi + 0.01).contains(&h), "{kind:?} {h}");
            forms.insert((*kind as u8, *mesh));
        }
        // Several seeded forms of each kind.
        for kind in ShrubKind::ALL {
            let n = forms.iter().filter(|f| f.0 == kind as u8).count();
            assert!(n >= 8, "{kind:?}: {n} meshes");
        }
        // The form and the tints are the shrub's seed, nothing else.
        for shrub in &planted {
            assert_eq!(
                shrub_variant(shrub),
                shrub_variant(&Shrub {
                    at: GroundVec2::ZERO,
                    ..*shrub
                })
            );
            assert_eq!(
                shrub_tints(shrub),
                shrub_tints(&Shrub { yaw: 0.0, ..*shrub })
            );
        }
        let seen: std::collections::HashSet<usize> =
            planted.iter().map(|s| shrub_variant(s).1).collect();
        assert_eq!(seen.len(), tree::SHRUB_VARIANTS);
    }

    #[test]
    fn without_the_tree_kit_no_shrub_is_drawn() {
        let coord = shrub_chunk();
        let mut app = app_with(
            coord.center(WorldConfig::default().chunk_size),
            Some(assets()),
            1,
            4,
        );
        app.update();
        let (kit, models, trees) = (
            app.world().resource::<FloraKit>(),
            app.world().resource::<VegetationModels>(),
            app.world().resource::<TreeKit>(),
        );
        let terrain = &app.world().resource::<WorldTerrain>().0;
        let site = site();
        let shrubs = site.shrubs(coord);
        let paint = |with_kit: bool| {
            let painter = Painter {
                terrain,
                site: &site,
                kit,
                models: Some(models),
                trees: with_kit.then_some(trees),
            };
            let mut out = Vec::new();
            for shrub in &shrubs {
                painter.shrub(shrub, &mut out);
            }
            out
        };
        assert!(shrubs.len() > 100);
        assert_eq!(paint(true).len(), shrubs.len() * 2);
        assert!(paint(false).is_empty());
    }

    #[test]
    fn the_densest_shrub_chunk_stays_within_the_per_shrub_budget() {
        let coord = shrub_chunk();
        let shrubs = site().shrubs(coord);
        let mut app = app(coord.center(WorldConfig::default().chunk_size));
        app.update();
        let kit = app.world().resource::<TreeKit>().clone();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mut total = 0;
        for shrub in &shrubs {
            let (species, v) = shrub_variant(shrub);
            let tris = form_triangles(meshes, &kit.species(species).variants[v]);
            assert!((200..=500).contains(&tris), "{species:?} {v}: {tris}");
            total += tris;
        }
        println!(
            "densest shrub chunk: {} shrubs, {total} triangles",
            shrubs.len()
        );
        assert!(shrubs.len() >= 200, "{}", shrubs.len());
        assert!(shrubs.len() <= FloraSite::SHRUB_BUDGET);
        assert!(total <= FloraSite::SHRUB_BUDGET * 500, "{total}");
    }

    #[test]
    fn each_flora_kind_maps_to_its_model_set() {
        let trees = grove_trees();
        let calden = trees.iter().filter(|t| t.species == Species::Calden);
        let (mut dead, mut caldenes) = (0, 0);
        for tree in calden {
            caldenes += 1;
            match tree_model(tree) {
                Model::DeadTree(n) => {
                    assert!((1..=5).contains(&n));
                    dead += 1;
                }
                Model::CommonTree(n) => assert!((1..=5).contains(&n)),
                other => panic!("caldén drawn as {other:?}"),
            }
        }
        for tree in trees.iter().filter(|t| t.species == Species::Chanar) {
            assert!(matches!(tree_model(tree), Model::Bush(1 | 2)));
        }
        // A few dead snags, deterministically by seed: a tenth of the
        // mature caldenes, a few percent of all trees.
        let share = dead as f32 / trees.len() as f32;
        println!(
            "{dead} snags of {caldenes} caldenes, {} trees: {share}",
            trees.len()
        );
        assert!((0.02..=0.08).contains(&share), "{share}");
        assert_eq!(tree_model(&trees[0]), tree_model(&trees[0]));

        let flora = site();
        let tufts: Vec<Tuft> = (0..6)
            .flat_map(|x| flora.chunk(ChunkCoord::new(x, 3)).tufts)
            .collect();
        let mut weeds = 0;
        for tuft in &tufts {
            let model = tuft_model(tuft);
            match (tuft.kind, model) {
                (_, Model::Plant(1..=5) | Model::Flowers) => weeds += 1,
                (TuftKind::Paja, Model::Wheat | Model::TallGrass) => {}
                (TuftKind::Flechilla, Model::Grass | Model::ShortGrass) => {}
                other => panic!("{other:?}"),
            }
        }
        let share = weeds as f32 / tufts.len() as f32;
        assert!((0.02..=0.12).contains(&share), "weeds {share}");
        // Every model has a file, and the set covers the 28 shipped files.
        let all = Model::all();
        assert_eq!(all.len(), 28);
        for model in &all {
            assert!(
                assets().join(MODEL_DIR).join(model.file()).exists(),
                "{model:?}"
            );
        }
    }

    fn loaded(model: Model) -> (ObjModel, MaterialColors) {
        obj::load(&assets().join(MODEL_DIR).join(model.file())).unwrap()
    }

    fn colours(mesh: &Mesh) -> Vec<[f32; 4]> {
        match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(c)) => c.clone(),
            _ => Vec::new(),
        }
    }

    fn points(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn recolouring_turns_the_green_leaves_dull_olive_and_stands_models_up() {
        let (obj, kd) = loaded(Model::CommonTree(1));
        let baked = bake(Model::CommonTree(1), &obj, &kd);
        let finishes: Vec<Finish> = baked.iter().map(|(f, _)| *f).collect();
        assert_eq!(finishes, vec![Finish::Bark, Finish::Leaves]);
        let source = kd["Green"];
        let leaves = colours(&baked[1].1);
        let mean = leaves
            .iter()
            .fold([0.0; 3], |a, c| [a[0] + c[0], a[1] + c[1], a[2] + c[2]]);
        let mean = mean.map(|c| c / leaves.len() as f32);
        // Source: saturated green. Now: olive, redder and brighter.
        assert!(
            mean[0] / mean[1] > source[0] / source[1] + 0.15,
            "{mean:?} {source:?}"
        );
        assert!(
            mean[2] / mean[1] < source[2] / source[1],
            "{mean:?} {source:?}"
        );
        // One unit tall, standing on the origin; the crown widened.
        let all: Vec<[f32; 3]> = baked.iter().flat_map(|(_, m)| points(m)).collect();
        let (lo, hi) = all
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
        assert!(lo.abs() < 1e-5 && (hi - 1.0).abs() < 1e-5, "{lo} {hi}");
        let raw_height = 2.48;
        let raw_width = obj
            .groups
            .iter()
            .flat_map(|g| &g.positions)
            .map(|p| p[0].hypot(p[2]))
            .fold(0.0, f32::max);
        let width = all.iter().map(|p| p[0].hypot(p[2])).fold(0.0, f32::max);
        assert!(width > raw_width / raw_height * 1.1, "{width}");
        // Its trunk is as thick as the obstacle `flora` plants for it.
        let trunk = points(&baked[0].1)
            .into_iter()
            .filter(|p| (0.06..0.14).contains(&p[1]))
            .map(|p| p[0].hypot(p[2]))
            .fold(0.0, f32::max);
        assert!(
            (0.025..0.05).contains(&trunk),
            "trunk radius / height {trunk}"
        );
        // Rocks are tosca beige and rough.
        let (obj, kd) = loaded(Model::Rock(1));
        let rock = bake(Model::Rock(1), &obj, &kd);
        assert_eq!(rock[0].0, Finish::Rock);
        let c = colours(&rock[0].1)[0];
        assert!(c[0] > c[2] && c[1] > c[2], "beige {c:?}");
    }

    #[test]
    fn a_malformed_group_is_skipped_and_the_others_keep_their_own_vertices() {
        let group = |positions: &[[f32; 3]], normals: usize| ObjGroup {
            material: "Rock".into(),
            positions: positions.to_vec(),
            normals: vec![[0.0, 0.0, 1.0]; normals],
            uvs: None,
        };
        // Two good triangles one unit tall, standing on the origin already.
        let a = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let b = [[3.0, 0.0, 0.0], [4.0, 0.0, 0.0], [3.0, 1.0, 0.0]];
        let obj = ObjModel {
            mtllib: None,
            groups: vec![
                // Four positions for three normals: it would shift every
                // later group by one vertex.
                group(&[[9.0, 0.5, 0.0]; 4], 3),
                group(&a, 3),
                // Not whole triangles.
                group(&[[9.0, 0.5, 0.0]; 2], 2),
                group(&b, 3),
            ],
        };
        let baked = bake(Model::Rock(1), &obj, &MaterialColors::new());
        let all: Vec<[f32; 3]> = baked.iter().flat_map(|(_, m)| points(m)).collect();
        assert_eq!(all, [a, b].concat());
        for (_, mesh) in &baked {
            assert_eq!(normals(mesh).len(), points(mesh).len());
        }
    }

    fn normals(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
            _ => panic!("normals"),
        }
    }

    /// Vertices of a mesh grouped by position (welded points), by normal.
    fn welded(mesh: &Mesh) -> Vec<Vec<Vec3>> {
        let mut at: HashMap<[u32; 3], Vec<Vec3>> = HashMap::new();
        for (p, n) in points(mesh).into_iter().zip(normals(mesh)) {
            at.entry(p.map(f32::to_bits))
                .or_default()
                .push(Vec3::from_array(n));
        }
        at.into_values().filter(|ns| ns.len() > 1).collect()
    }

    /// The share of a mesh's welded points whose vertices all carry one
    /// normal: about 0 for flat shading, 1 for smooth without creases.
    fn smooth_share(mesh: &Mesh) -> f32 {
        let shared = welded(mesh);
        let smooth = shared
            .iter()
            .filter(|ns| ns.iter().all(|n| n.dot(ns[0]) > 0.9999))
            .count();
        smooth as f32 / shared.len().max(1) as f32
    }

    /// Of the vertex pairs at one point whose normals are within 30 degrees
    /// (neighbouring facets on one side of a surface), the share that carry
    /// the same normal. Faceted meshes score about 0. Unlike
    /// [`smooth_share`], the two faces of a card (a grass blade) welded
    /// back to back do not count against it.
    fn blend_share(mesh: &Mesh) -> f32 {
        let (mut near, mut same) = (0, 0);
        for ns in welded(mesh) {
            for (k, a) in ns.iter().enumerate() {
                for b in &ns[k + 1..] {
                    if a.dot(*b) > 30f32.to_radians().cos() {
                        near += 1;
                        same += usize::from(a.dot(*b) > 0.9999);
                    }
                }
            }
        }
        same as f32 / near.max(1) as f32
    }

    #[test]
    fn baked_vegetation_has_smooth_normals_with_creases_kept() {
        let bake_of = |model| {
            let (obj, kd) = loaded(model);
            bake(model, &obj, &kd)
        };
        // Crowns, bushes and grass blend across their facets.
        for model in [
            Model::CommonTree(1),
            Model::Bush(1),
            Model::Grass,
            Model::Plant(3),
        ] {
            for (finish, mesh) in bake_of(model) {
                let share = blend_share(&mesh);
                assert!(share > 0.9, "{model:?} {finish:?}: {share}");
                let n = normals(&mesh);
                assert_eq!(n.len(), mesh.count_vertices(), "{model:?}");
                assert_eq!(colours(&mesh).len(), n.len());
                for x in n {
                    assert!((Vec3::from_array(x).length() - 1.0).abs() < 1e-4);
                }
            }
        }
        // Tosca stones keep some hard edges.
        let rock = &bake_of(Model::Rock(1))[0].1;
        let share = smooth_share(rock);
        assert!(share < 0.95, "rock creases: {share}");
        // Grass blades lean their normals up, so a tuft lights like the
        // ground under it instead of going dark on its shaded side.
        let grass = &bake_of(Model::Grass)[0].1;
        let up = normals(grass).iter().map(|n| n[1]).sum::<f32>() / grass.count_vertices() as f32;
        assert!(up > 0.5, "grass normals lean up: {up}");
        // Baking is deterministic.
        let again = &bake_of(Model::Grass)[0].1;
        assert_eq!(normals(grass), normals(again));
    }

    #[test]
    fn models_replace_the_stand_ins_and_missing_ones_fall_back() {
        let focus = grove_chunk();
        let mut with = app_with(focus, Some(assets()), 1, 4);
        with.update();
        let models = with.world().resource::<VegetationModels>();
        assert!(Model::all().into_iter().all(|m| models.has(m)));
        let mut q = with.world_mut().query_filtered::<&Mesh3d, With<TreePart>>();
        let drawn: Vec<AssetId<Mesh>> = q.iter(with.world()).map(|m| m.id()).collect();
        let kit = with.world().resource::<FloraKit>();
        let stand_ins = [
            kit.calden_trunk.id(),
            kit.calden_crown.id(),
            kit.chanar_trunk.id(),
            kit.chanar_crown.id(),
        ];
        assert!(
            drawn.iter().any(|id| !stand_ins.contains(id)),
            "models drawn"
        );
        assert!(
            !drawn
                .iter()
                .any(|id| *id == stand_ins[2] || *id == stand_ins[3]),
            "chañar stand-ins replaced"
        );
        // Shared: a handful of meshes and materials for hundreds of parts.
        let mut q = with
            .world_mut()
            .query::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>();
        let pairs: Vec<_> = q
            .iter(with.world())
            .map(|(m, mat)| (m.id(), mat.id()))
            .collect();
        assert!(
            distinct(pairs.iter().map(|p| p.0)) <= 9 + 28 * 2 + 6 + 2 * 3 * tree::SHRUB_VARIANTS,
            "{}",
            pairs.len()
        );
        // The field trees' bark and leaves come in a few tints each.
        let tints = 2
            * (crate::tree::FIELD_TINTS - 1)
            * (KitSpecies::FIELD.len() + KitSpecies::SHRUBS.len());
        // And the shrubs' own first bark and leaves.
        assert!(distinct(pairs.iter().map(|p| p.1)) <= 16 + 2 * KitSpecies::SHRUBS.len() + tints);
        // Tufts are tinted from grey-green to dry yellow.
        let mut q = with
            .world_mut()
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<GrassTuft>>();
        assert!(distinct(q.iter(with.world()).map(|m| m.id())) >= 3);

        // No asset folder, or a missing file: the kit's trees are drawn
        // all the same (they need no model), and the stand-ins only when
        // the kit is missing too.
        let mut without = app_with(focus, Some(assets().join("nowhere")), 1, 4);
        without.update();
        let models = without.world().resource::<VegetationModels>();
        assert!(!models.has(Model::CommonTree(1)));
        assert!(models.parts(Model::CommonTree(1), 0).is_none());
        let trees: usize = WorldConfig::default()
            .chunks_near(focus, 1)
            .iter()
            .map(|&c| site().trees(c).len())
            .sum();
        assert_eq!(count::<TreePart>(&mut without), trees * 2);
    }

    #[test]
    fn visible_triangles_stay_under_budget_at_the_streaming_radius() {
        // The densest grove nearby, with the default streaming radius and
        // ground resolution.
        let flora = site();
        let size = WorldConfig::default().chunk_size;
        let densest = (-12..12)
            .flat_map(|x| (-12..12).map(move |y| ChunkCoord::new(x, y)))
            .max_by_key(|&c| flora.trees(c).len())
            .expect("chunks");
        let focus = densest.center(size);
        let mut app = app_with(focus, Some(assets()), 3, 40);
        app.update();
        let eye = world_from_ground(focus, 0.0);
        let mut q = app
            .world_mut()
            .query::<(&Mesh3d, &Transform, Option<&VisibilityRange>)>();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let (mut visible, mut loaded, mut ground) = (0, 0, 0);
        for (mesh, transform, range) in q.iter(app.world()) {
            let tris = meshes.get(&mesh.0).map_or(0, triangles);
            loaded += tris;
            // Chunk meshes sit at the origin in world-space vertices.
            let Some(range) = range else {
                ground += tris;
                continue;
            };
            let d = transform.translation.with_y(0.0).distance(eye.with_y(0.0));
            if range.start_margin.start <= d && d < range.end_margin.end {
                visible += tris;
            }
        }
        let flora_visible = visible;
        visible += ground;
        println!(
            "radius 3 over {} trees: {visible} triangles drawn ({ground} ground, \
             {flora_visible} plants), {loaded} loaded",
            flora.trees(densest).len()
        );
        assert!(visible < 1_200_000, "{visible}");
        assert!(visible < loaded, "distance culling works");
    }

    /// How opaque `range` draws at `d` meters: 0 hidden, 1 solid, dithered
    /// in between.
    fn opacity(range: &VisibilityRange, d: f32) -> f32 {
        let ramp = |r: &std::ops::Range<f32>| {
            if r.end > r.start {
                ((d - r.start) / (r.end - r.start)).clamp(0.0, 1.0)
            } else if d >= r.start {
                1.0
            } else {
                0.0
            }
        };
        ramp(&range.start_margin).min(1.0 - ramp(&range.end_margin))
    }

    /// A drawn mesh and the distances it is drawn at.
    type RangedMesh = (AssetId<Mesh>, VisibilityRange);

    /// Every drawn tree part of a grove, with models loaded, grouped by the
    /// spot it stands on.
    fn tree_groups(app: &mut App) -> Vec<Vec<RangedMesh>> {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Mesh3d, &Transform, &VisibilityRange), With<TreePart>>();
        let mut groups: Vec<(Vec3, Vec<RangedMesh>)> = Vec::new();
        for (mesh, transform, range) in q.iter(app.world()) {
            let at = transform.translation;
            let entry = (mesh.id(), range.clone());
            match groups.iter_mut().find(|(p, _)| p.distance(at) < 1e-3) {
                Some((_, parts)) => parts.push(entry),
                None => groups.push((at, vec![entry])),
            }
        }
        groups.into_iter().map(|(_, parts)| parts).collect()
    }

    #[test]
    fn a_tree_is_the_same_model_at_every_distance() {
        let mut app = app_with(grove_chunk(), Some(assets()), 1, 4);
        app.update();
        let groups = tree_groups(&mut app);
        assert!(groups.len() >= 10);
        for parts in &groups {
            let drawn = |d: f32| -> Vec<AssetId<Mesh>> {
                let mut ids: Vec<AssetId<Mesh>> = parts
                    .iter()
                    .filter(|(_, r)| opacity(r, d) > 0.0)
                    .map(|(id, _)| *id)
                    .collect();
                ids.sort();
                ids
            };
            let near = drawn(20.0);
            assert!(!near.is_empty());
            for d in [69.0, 71.0, 150.0] {
                assert_eq!(drawn(d), near, "at {d} m");
            }
        }
    }

    #[test]
    fn visibility_ranges_leave_no_gap_before_the_fog() {
        let mut app = app_with(grove_chunk(), Some(assets()), 1, 4);
        app.update();
        // Trees are solid all the way to the dust fog; their fade-out is
        // inside it.
        for parts in tree_groups(&mut app) {
            for (_, range) in &parts {
                assert!(
                    range.end_margin.start >= FOG_VISIBILITY,
                    "{:?}",
                    range.end_margin
                );
            }
            for step in 0..(FOG_VISIBILITY as i32 * 2) {
                let d = step as f32 * 0.5;
                let solid = parts.iter().map(|(_, r)| opacity(r, d)).fold(0.0, f32::max);
                assert!(solid >= 1.0, "a tree half drawn at {d} m");
            }
        }
        // Tufts stay solid near the rider, wherever the camera orbits.
        let mut q = app
            .world_mut()
            .query_filtered::<&VisibilityRange, With<GrassTuft>>();
        for range in q.iter(app.world()) {
            assert_eq!(range.start_margin, 0.0..0.0);
            assert!(range.end_margin.start >= TUFT_SOLID + CAMERA_REACH);
        }
        // Bevy keeps every distinct range in a GPU slot for the app's whole
        // life and culls unranged meshes once 65,535 are used: tufts share a
        // few stepped ranges instead of one each.
        let distinct: std::collections::HashSet<_> = q.iter(app.world()).cloned().collect();
        assert!(
            distinct.len() <= TUFT_FADE_STEPS as usize,
            "{} distinct tuft ranges",
            distinct.len()
        );
        // Stones, shrubs, stumps and logs: solid from the rider, a fade at
        // the far end only.
        let mut q = app
            .world_mut()
            .query_filtered::<&VisibilityRange, (Without<GrassTuft>, Without<TreePart>)>();
        for range in q.iter(app.world()) {
            assert_eq!(range.start_margin, 0.0..0.0);
            assert!(range.end_margin.start >= TUFT_SOLID + CAMERA_REACH);
        }
    }

    /// Everything drawn on chunk `coord`: mesh, material and transform bits.
    fn chunk_scene(app: &mut App, coord: ChunkCoord) -> Vec<(String, [u32; 10])> {
        let mut chunks = app.world_mut().query::<(Entity, &TerrainChunk)>();
        let chunk = chunks
            .iter(app.world())
            .find(|(_, c)| c.0 == coord)
            .map(|(e, _)| e)
            .expect("chunk loaded");
        let mut q = app.world_mut().query::<(
            &ChildOf,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &Transform,
        )>();
        let mut scene: Vec<(String, [u32; 10])> = q
            .iter(app.world())
            .filter(|(parent, ..)| parent.parent() == chunk)
            .map(|(_, mesh, material, t)| {
                let (r, s, p) = (t.rotation, t.scale, t.translation);
                (
                    format!("{:?} {:?}", mesh.id(), material.id()),
                    [p.x, p.y, p.z, r.x, r.y, r.z, r.w, s.x, s.y, s.z].map(f32::to_bits),
                )
            })
            .collect();
        scene.sort();
        scene
    }

    #[test]
    fn a_reloaded_chunk_shows_exactly_the_same_scene() {
        let focus = grove_chunk();
        let coord = WorldConfig::default().chunk_of(focus);
        let mut app = app_with(focus, Some(assets()), 1, 4);
        app.update();
        let before = chunk_scene(&mut app, coord);
        assert!(before.len() > 100);
        app.world_mut().resource_mut::<StreamFocus>().0 = focus + GroundVec2::new(3000.0, 0.0);
        app.update();
        app.world_mut().resource_mut::<StreamFocus>().0 = focus;
        app.update();
        assert_eq!(chunk_scene(&mut app, coord), before);
    }

    #[test]
    fn stand_ins_are_drawn_only_for_missing_models() {
        let mut app = app_with(grove_chunk(), Some(assets()), 1, 4);
        app.update();
        let kit = app.world().resource::<FloraKit>();
        let stand_ins = [
            kit.calden_trunk.id(),
            kit.calden_crown.id(),
            kit.chanar_trunk.id(),
            kit.chanar_crown.id(),
            kit.paja.id(),
            kit.flechilla.id(),
        ];
        let mut q = app.world_mut().query::<&Mesh3d>();
        assert!(q.iter(app.world()).all(|m| !stand_ins.contains(&m.id())));
    }

    /// The center of a chunk holding a grove.
    fn grove_chunk() -> GroundVec2 {
        let size = WorldConfig::default().chunk_size;
        let flora = site();
        (-10..10)
            .flat_map(|x| (-10..10).map(move |y| ChunkCoord::new(x, y)))
            .find(|&c| flora.trees(c).len() >= 10)
            .expect("a grove")
            .center(size)
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<T>>();
        q.iter(app.world()).count()
    }

    #[test]
    fn a_chunk_grows_trees_and_tufts_and_drops_them_with_it() {
        let focus = grove_chunk();
        let mut app = app(focus);
        app.update();
        let flora = site();
        let world = WorldConfig::default();
        let loaded = world.chunks_near(focus, 1);
        let (trees, tufts): (usize, usize) = loaded
            .iter()
            .map(|&c| {
                let f = flora.chunk(c);
                (f.trees.len(), f.tufts.len().min(TUFTS_DRAWN))
            })
            .fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
        assert!(trees >= 10 && tufts > 100, "{trees} {tufts}");
        // Two parts per tree (trunk and crown), one entity per tuft, all
        // children of their chunk.
        assert_eq!(count::<TreePart>(&mut app), trees * 2);
        assert_eq!(count::<GrassTuft>(&mut app), tufts);
        let mut q = app
            .world_mut()
            .query_filtered::<&ChildOf, Or<(With<TreePart>, With<GrassTuft>)>>();
        let parents: Vec<Entity> = q.iter(app.world()).map(|c| c.parent()).collect();
        let mut chunks = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainChunk>>();
        let chunks: Vec<Entity> = chunks.iter(app.world()).collect();
        assert!(parents.iter().all(|p| chunks.contains(p)));

        // Ride far away: the old chunks' flora goes with them.
        app.world_mut().resource_mut::<StreamFocus>().0 = focus + GroundVec2::new(3000.0, 0.0);
        app.update();
        let far = world.chunks_near(focus + GroundVec2::new(3000.0, 0.0), 1);
        let expected: usize = far.iter().map(|&c| flora.chunk(c).trees.len() * 2).sum();
        assert_eq!(count::<TreePart>(&mut app), expected);
    }

    #[test]
    fn tufts_and_trees_share_their_meshes_and_materials() {
        let mut app = app(grove_chunk());
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>), With<GrassTuft>>();
        let mut meshes: Vec<AssetId<Mesh>> = Vec::new();
        let mut materials: Vec<AssetId<StandardMaterial>> = Vec::new();
        for (mesh, material) in q.iter(app.world()) {
            if !meshes.contains(&mesh.id()) {
                meshes.push(mesh.id());
            }
            if !materials.contains(&material.id()) {
                materials.push(material.id());
            }
        }
        assert!(meshes.len() <= 2 && materials.len() == 1, "{meshes:?}");
        // Trees are the kit's: a bark and a leaf mesh a variant, a bark and
        // a leaf material a species, however many trees there are.
        let drawn = drawn_trees(&mut app);
        let tree_meshes = distinct(drawn.iter().map(|t| t.1));
        let tree_materials = distinct(drawn.iter().map(|t| t.2));
        let variants: usize = KitSpecies::FIELD.iter().map(|s| s.variants()).sum();
        assert!(drawn.len() > 2 * variants, "{}", drawn.len());
        assert!(tree_meshes <= 2 * variants, "{tree_meshes}");
        assert!(
            tree_materials <= 2 * crate::tree::FIELD_TINTS * KitSpecies::FIELD.len(),
            "{tree_materials}"
        );
    }

    fn fade_app() -> (App, Entity, Handle<StandardMaterial>) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(ThinFadePlugin);
        let solid = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let tree = app
            .world_mut()
            .spawn((
                ThinOccluder {
                    a: GroundVec2::new(0.0, 5.0),
                    b: GroundVec2::new(0.0, 5.0),
                    radius: 2.0,
                    bottom: -0.5,
                    top: 8.0,
                },
                MeshMaterial3d(solid.clone()),
            ))
            .id();
        (app, tree, solid)
    }

    #[test]
    fn a_thin_occluder_between_the_camera_and_the_rider_fades_and_comes_back() {
        let (mut app, tree, solid) = fade_app();
        let rider = Vec3::new(0.0, 1.5, 0.0);
        // Camera 12 m south of the rider, the occluder 5 m north: clear.
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(0.0, 4.0, 12.0)));
        app.update();
        assert!(app.world().get::<Faded>(tree).is_none());
        // Camera 12 m north: the tree is in the way.
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(0.0, 4.0, -12.0)));
        app.update();
        let faded = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(tree)
            .unwrap()
            .0
            .clone();
        assert_ne!(faded, solid);
        assert_eq!(app.world().get::<Faded>(tree).unwrap().0, solid);
        let see_through = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(&faded)
            .unwrap();
        assert_eq!(see_through.alpha_mode, AlphaMode::Blend);
        assert!((see_through.base_color.alpha() - FADED_ALPHA).abs() < 1e-6);
        // Over the top of the crown, the camera sees past it.
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(0.0, 30.0, -12.0)));
        app.update();
        assert!(app.world().get::<Faded>(tree).is_none());
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(tree)
                .unwrap()
                .0,
            solid
        );
        // A despawned occluder leaves the index.
        app.world_mut().despawn(tree);
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(0.0, 4.0, -12.0)));
        app.update();
        assert!(app.world().resource::<ThinIndex>().of.is_empty());
    }

    #[test]
    fn a_moved_occluder_is_found_where_it_now_stands() {
        let (mut app, tree, solid) = fade_app();
        app.update();
        // The tree is moved 100 m east, 5 m north of a rider there.
        let mut occluder = app.world_mut().get_mut::<ThinOccluder>(tree).unwrap();
        occluder.a = GroundVec2::new(100.0, 5.0);
        occluder.b = GroundVec2::new(100.0, 5.0);
        let rider = Vec3::new(100.0, 1.5, 0.0);
        // Camera to the north: it is in the way where it now stands.
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(100.0, 4.0, -12.0)));
        app.update();
        assert_eq!(
            app.world().get::<Faded>(tree).map(|f| f.0.clone()),
            Some(solid)
        );
        // Where it stood, nothing is left in the way.
        let rider = Vec3::new(0.0, 1.5, 0.0);
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(0.0, 4.0, -12.0)));
        app.update();
        assert!(app.world().get::<Faded>(tree).is_none());
        assert_eq!(app.world().resource::<ThinIndex>().of.len(), 1);
    }

    #[test]
    fn a_material_not_loaded_yet_is_not_faded_into_a_blank_copy() {
        let (mut app, tree, _) = fade_app();
        // The occluder's material is not in the store (yet).
        let pending: Handle<StandardMaterial> = Handle::default();
        app.world_mut()
            .get_mut::<MeshMaterial3d<StandardMaterial>>(tree)
            .unwrap()
            .0 = pending.clone();
        let rider = Vec3::new(0.0, 1.5, 0.0);
        app.world_mut().resource_mut::<SightLine>().0 = Some((rider, Vec3::new(0.0, 4.0, -12.0)));
        app.update();
        assert!(app.world().get::<Faded>(tree).is_none());
        assert!(app.world().resource::<FadeMaterials>().0.is_empty());
        // Once loaded, it fades into a see-through copy of itself.
        let red = StandardMaterial {
            base_color: Color::srgb(0.8, 0.1, 0.1),
            ..default()
        };
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .insert(pending.id(), red)
            .unwrap();
        app.update();
        assert!(app.world().get::<Faded>(tree).is_some());
        let copy = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(tree)
            .unwrap()
            .0
            .clone();
        let copy = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(&copy)
            .unwrap();
        assert!((copy.base_color.to_srgba().red - 0.8).abs() < 1e-3);
        assert!((copy.base_color.alpha() - FADED_ALPHA).abs() < 1e-6);
    }

    #[test]
    fn only_occluders_across_the_sight_line_block_it() {
        let fence = ThinOccluder {
            a: GroundVec2::new(-20.0, 4.0),
            b: GroundVec2::new(20.0, 4.0),
            radius: 0.4,
            bottom: -0.5,
            top: 1.5,
        };
        let rider = Vec3::new(0.0, 1.0, 0.0);
        // North of the rider, the camera low: the fence is in the way.
        assert!(fence.blocks(rider, Vec3::new(3.0, 1.2, -10.0)));
        // High camera: the line passes over the wire.
        assert!(!fence.blocks(rider, Vec3::new(3.0, 6.0, -10.0)));
        // Camera south: nothing between.
        assert!(!fence.blocks(rider, Vec3::new(3.0, 1.2, 10.0)));
        // Past the fence's end.
        assert!(!fence.blocks(rider, Vec3::new(60.0, 1.2, -10.0)));
        let post = ThinOccluder::post(
            GroundVec2::new(0.0, 6.0),
            0.5,
            &pampa_core::Terrain::default(),
            3.0,
        );
        let ground = pampa_core::Terrain::default().height(0.0, 6.0);
        let eye = Vec3::new(0.0, ground + 1.5, -12.0);
        assert!(post.blocks(Vec3::new(0.0, ground + 1.5, 0.0), eye));
        assert!(!post.blocks(
            Vec3::new(2.0, ground + 1.5, 0.0),
            Vec3::new(2.0, ground + 1.5, -12.0)
        ));
    }

    #[test]
    fn triangles_counts_indexed_meshes_by_their_indices() {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        );
        // Four vertices, two triangles; a soup of three has one.
        assert_eq!(
            triangles(
                &mesh
                    .clone()
                    .with_inserted_indices(bevy::mesh::Indices::U32(vec![0, 1, 2, 0, 2, 3]))
            ),
            2
        );
        mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION);
        let soup = mesh.with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
        );
        assert_eq!(triangles(&soup), 1);
    }

    #[test]
    fn meshes_are_faceted_and_cheap() {
        let calden = triangles(&calden_trunk()) + triangles(&calden_crown());
        let chanar = triangles(&chanar_trunk()) + triangles(&chanar_crown());
        assert!((300..=800).contains(&calden), "{calden}");
        assert!((150..=500).contains(&chanar), "{chanar}");
        for kind in [TuftKind::Paja, TuftKind::Flechilla] {
            assert!(triangles(&tuft_mesh(kind)) <= 10);
        }
        // The caldén's crown spreads wider than the chañar's.
        let width = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => {
                p.iter().map(|v| v[0].hypot(v[2])).fold(0.0, f32::max)
            }
            _ => 0.0,
        };
        assert!(width(&calden_crown()) / 7.5 > width(&chanar_crown()) / 4.0);
    }
}
