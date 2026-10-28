//! The towns' trees: procedural paraísos, olmos, eucaliptos and aromos,
//! the trees of 1919 pampa plazas, streets and yards, and the open
//! country's caldén and chañar ([`Species::FIELD`]).
//!
//! Each [`Species`] has [`VARIANTS`] seeded trees (the field species
//! [`CALDEN_VARIANTS`] and [`CHANAR_VARIANTS`] distinct forms, so the fields
//! do not repeat) ([`mesh`]): tapered bark trunks and limbs carrying
//! alpha-tested leaf cards ([`materials`]). They are built once at startup
//! into the shared [`TreeKit`], two meshes a variant and two materials a
//! species (the field species a few tints of each, [`FIELD_TINTS`]), so a
//! town's trees cost no new mesh or material: each plaza tree is a variant's bark and leaves, turned and
//! scaled. Core places the trunks ([`pampa_core::TownLayout::trees`]); the
//! species of each plaza is picked here from the town's seed
//! ([`plaza_species`]), so the layout does not change. A street tree is a
//! trimmed variant ([`mesh::street_tree`]: a trunk bare to about 2.5 m under
//! a full, narrow crown) of the [`STREET_SPECIES`]; the yard shrubs and flower
//! beds are the [`plants`].

pub mod materials;
pub(crate) mod mesh;
pub mod plants;

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;

use crate::AssetRoot;

/// Seeded variants built per species.
pub const VARIANTS: usize = 3;

/// Distinct forms built of the caldén: [`CALDEN_LIVE`] live ones, then
/// three snags.
pub const CALDEN_VARIANTS: usize = 12;

/// Distinct forms built of the chañar.
pub const CHANAR_VARIANTS: usize = 6;

/// How many of the caldén's variants are live trees; the rest are dead or
/// sparse snags ([`mesh::tree`]).
pub const CALDEN_LIVE: usize = 9;

/// Distinct forms built of each shrub: three shapes (plain, low and
/// spreading, tall and dense), two seeds of each.
pub const SHRUB_VARIANTS: usize = 6;

/// Leaf and bark tints a field species has (shared materials, so trees of
/// every tint still batch by variant and tint).
pub const FIELD_TINTS: usize = 3;

/// A tree's species: the towns' four, the fields' two, the fields' three
/// shrubs and the estancias' poplar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Species {
    /// Paraíso (_Melia azedarach_): a round umbrella crown of pinnate
    /// leaves on a short trunk.
    Paraiso,
    /// Olmo (elm): a tall vase of limbs.
    Olmo,
    /// Eucalipto: a tall straight trunk, an open crown of hanging leaves.
    Eucalipto,
    /// Aromo: low and feathery.
    Aromo,
    /// Caldén (_Neltuma caldenia_), the open country's tree: a short thick
    /// trunk forking low under a broad flat umbrella of small grey-green
    /// leaves; its last variants are bare or sparse.
    Calden,
    /// Chañar (_Geoffroea decorticans_): small, multi-stemmed and twiggy,
    /// a sparse irregular crown.
    Chanar,
    /// Piquillín (_Condalia microphylla_), a shrub of the monte: a dense
    /// thorny mass of fine stems under tiny dark leaves.
    Piquillin,
    /// Jarilla (_Larrea divaricata_), a shrub of the open dry ground: many
    /// upright stems, resinous yellow-green leaves.
    Jarilla,
    /// Fachinal: a thicket of chañar suckers (the chañar's own bark and
    /// leaves).
    Fachinal,
    /// Álamo (Lombardy poplar, _Populus nigra_ 'Italica'), the estancias'
    /// windbreak rows: a tall narrow column of upswept limbs.
    Alamo,
}

impl Species {
    /// The towns' species, in order: what plazas, streets and yards plant.
    pub const ALL: [Species; 4] = [Self::Paraiso, Self::Olmo, Self::Eucalipto, Self::Aromo];
    /// The fields' species, in order.
    pub const FIELD: [Species; 2] = [Self::Calden, Self::Chanar];
    /// The fields' shrubs, in order.
    pub const SHRUBS: [Species; 3] = [Self::Piquillin, Self::Jarilla, Self::Fachinal];
    /// Every species the [`TreeKit`] builds, in `Species as usize` order.
    pub const EVERY: [Species; 10] = [
        Self::Paraiso,
        Self::Olmo,
        Self::Eucalipto,
        Self::Aromo,
        Self::Calden,
        Self::Chanar,
        Self::Piquillin,
        Self::Jarilla,
        Self::Fachinal,
        Self::Alamo,
    ];

    /// How many seeded variants the kit holds of it.
    pub const fn variants(self) -> usize {
        match self {
            Self::Calden => CALDEN_VARIANTS,
            Self::Chanar => CHANAR_VARIANTS,
            Self::Piquillin | Self::Jarilla | Self::Fachinal => SHRUB_VARIANTS,
            _ => VARIANTS,
        }
    }

    /// How many leaf tints, and bark tints, the kit holds of it.
    pub const fn tints(self) -> usize {
        match self {
            Self::Calden | Self::Chanar | Self::Piquillin | Self::Jarilla | Self::Fachinal => {
                FIELD_TINTS
            }
            _ => 1,
        }
    }
}

/// Hashes `seed` and `salt` into a well-mixed number.
fn mix(seed: u32, salt: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// The species planted in plaza `plaza` (0: the main one, 1: the
/// station's, see [`pampa_core::TownLayout::plazas`]) of the town seeded
/// `seed`. A town's two plazas never share a species.
pub fn plaza_species(seed: u32, plaza: usize) -> Species {
    let main = mix(seed, 0x7EE5) as usize % Species::ALL.len();
    let k = if plaza == 0 {
        main
    } else {
        main + 1 + (mix(seed, 0x57A7 + plaza as u32) as usize % (Species::ALL.len() - 1))
    };
    Species::ALL[k % Species::ALL.len()]
}

/// The species that stand trimmed along streets: the ones whose crowns
/// can be raised to a bare trunk of about 4 m ([`TreeKit`]'s `pruned`
/// variants). The aromo stays low.
pub const STREET_SPECIES: [Species; 3] = [Species::Paraiso, Species::Olmo, Species::Eucalipto];

/// The street species of the trees at `key` (a sidewalk strip, or a spot)
/// of the town seeded `seed`: one species along a whole row.
pub fn street_species(seed: u32, key: u32) -> Species {
    STREET_SPECIES[mix(seed, 0x57EE ^ key) as usize % STREET_SPECIES.len()]
}

/// The species of the yard tree standing at ground spot `at` of the town
/// seeded `seed`.
pub fn yard_species(seed: u32, at: pampa_core::Vec2) -> Species {
    let spot = ((at.x * 8.0) as i32 as u32).wrapping_mul(0x27D4_EB2F) ^ (at.y * 8.0) as i32 as u32;
    Species::ALL[mix(seed, 0x7A2D ^ spot) as usize % Species::ALL.len()]
}

/// One shared variant: its meshes and how big it stands.
#[derive(Debug, Clone)]
pub struct TreeVariant {
    pub bark: Handle<Mesh>,
    pub leaves: Handle<Mesh>,
    /// Its top over the ground, meters.
    pub height: f32,
    /// Its leaves' reach from the trunk, meters.
    pub crown_radius: f32,
    /// How low its lowest leaf hangs over the ground, meters.
    pub crown_base: f32,
}

/// One species' variants and its materials.
#[derive(Debug, Clone)]
pub struct SpeciesKit {
    pub variants: Vec<TreeVariant>,
    /// The variants trimmed for a street (a raised, narrow crown): only the
    /// [`STREET_SPECIES`] have them.
    pub pruned: Vec<TreeVariant>,
    /// The first bark and leaf materials: the town species' only ones.
    pub bark: Handle<StandardMaterial>,
    pub leaves: Handle<StandardMaterial>,
    /// Every bark and leaf tint ([`Species::tints`]), the first of each
    /// being [`Self::bark`] and [`Self::leaves`].
    pub bark_tints: Vec<Handle<StandardMaterial>>,
    pub leaf_tints: Vec<Handle<StandardMaterial>>,
}

/// The shared tree meshes and materials, and the yard plants', built at
/// startup.
#[derive(Resource, Debug, Clone)]
pub struct TreeKit {
    species: Vec<SpeciesKit>,
    pub plants: plants::PlantKit,
}

impl TreeKit {
    /// The kit of `species`.
    pub fn species(&self, species: Species) -> &SpeciesKit {
        &self.species[species as usize]
    }
}

/// Builds the [`TreeKit`] at startup.
pub struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_trees);
    }
}

/// Adds the [`TreePlugin`] to `app` unless another plugin already did.
pub fn ensure_plugin(app: &mut App) {
    if !app.is_plugin_added::<TreePlugin>() {
        app.add_plugins(TreePlugin);
    }
}

pub(crate) fn setup_trees(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    server: Option<Res<AssetServer>>,
    support: Option<Res<CompressedImageFormatSupport>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let paint = materials::tree_materials(
        root.as_deref(),
        server.as_deref(),
        support.as_deref(),
        &mut materials,
    );
    let kits = Species::EVERY
        .iter()
        .zip(paint)
        .map(|(&species, paint)| {
            let mut variant = |tree: mesh::TreeMesh| TreeVariant {
                height: tree.height,
                crown_radius: tree.crown_radius,
                crown_base: tree.crown_base,
                bark: meshes.add(tree.bark.into_mesh()),
                leaves: meshes.add(tree.leaves.into_mesh()),
            };
            let variants = (0..species.variants() as u32)
                .map(|v| variant(mesh::tree(species, v)))
                .collect();
            let pruned = if STREET_SPECIES.contains(&species) {
                (0..VARIANTS as u32)
                    .map(|v| variant(mesh::street_tree(species, v)))
                    .collect()
            } else {
                Vec::new()
            };
            SpeciesKit {
                variants,
                pruned,
                bark: paint.bark[0].clone(),
                leaves: paint.leaves[0].clone(),
                bark_tints: paint.bark,
                leaf_tints: paint.leaves,
            }
        })
        .collect();
    let plants = plants::PlantKit::build(&mut meshes, &mut materials);
    commands.insert_resource(TreeKit {
        species: kits,
        plants,
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn plaza_species_follow_the_seed_and_the_two_plazas_differ() {
        let mut mains = HashSet::new();
        let mut stations = HashSet::new();
        for seed in 0..64 {
            let (main, station) = (plaza_species(seed, 0), plaza_species(seed, 1));
            assert_eq!(main, plaza_species(seed, 0));
            assert_ne!(main, station, "seed {seed}");
            mains.insert(main);
            stations.insert(station);
        }
        // Every species gets planted somewhere.
        assert_eq!(mains.len(), Species::ALL.len());
        assert_eq!(stations.len(), Species::ALL.len());
    }

    #[test]
    fn the_kit_builds_each_variant_once() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(TreePlugin);
        app.update();
        let kit = app.world().resource::<TreeKit>().clone();
        let mut meshes = HashSet::new();
        let mut materials = HashSet::new();
        for species in Species::EVERY {
            let k = kit.species(species);
            assert_eq!(k.variants.len(), species.variants());
            for v in k.variants.iter().chain(&k.pruned) {
                meshes.insert(v.bark.id());
                meshes.insert(v.leaves.id());
                // A tree stands over 3 m; a shrub is a low mass over 1 m.
                let (tall, wide) = if Species::SHRUBS.contains(&species) {
                    (1.0, 0.4)
                } else {
                    (3.0, 1.0)
                };
                assert!(v.height > tall && v.crown_radius > wide, "{species:?}");
            }
            materials.extend(k.bark_tints.iter().chain(&k.leaf_tints).map(|m| m.id()));
        }
        // Every species' variants (the álamo's among them), the trimmed ones of
        // the street species and the plants'.
        let built = 2 * VARIANTS * (Species::ALL.len() + STREET_SPECIES.len())
            + 2 * (CALDEN_VARIANTS + CHANAR_VARIANTS + 3 * SHRUB_VARIANTS + VARIANTS)
            + 2 * plants::VARIANTS as usize;
        let plant_meshes = (kit.plants.shrubs.iter().chain(&kit.plants.flowers)).map(|h| h.id());
        meshes.extend(plant_meshes);
        assert_eq!(meshes.len(), built);
        // The trees' bark and leaf materials, a tint each, and the plants' one.
        materials.insert(kit.plants.material.id());
        let tints: usize = Species::EVERY.iter().map(|s| 2 * s.tints()).sum();
        assert_eq!(materials.len(), tints + 1);
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), built);
    }

    #[test]
    fn trimmed_street_variants_have_a_trunk_bare_to_about_three_meters_and_a_full_crown() {
        for species in STREET_SPECIES {
            for v in 0..VARIANTS as u32 {
                let free = mesh::tree(species, v);
                let trimmed = mesh::street_tree(species, v);
                // A low, full crown: leaves from about 3 m (none under 2.7 m,
                // none above 4.2 m), a good deal of foliage, a street tree's height.
                assert!(
                    (2.0..4.2).contains(&trimmed.crown_base),
                    "{species:?} {v}: {}",
                    trimmed.crown_base
                );
                assert!(trimmed.leaves.positions.len() >= 700, "{species:?} {v}");
                // Narrower than the free tree's crown, but not a column.
                assert!(
                    trimmed.crown_radius < 0.65 * free.crown_radius,
                    "{species:?} {v}"
                );
                assert!(trimmed.crown_radius > 2.2, "{species:?} {v}");
                assert!((5.0..8.0).contains(&trimmed.height), "{species:?} {v}");
            }
        }
        // The aromo has none: it stays a low yard tree.
        assert!(mesh::tree(Species::Aromo, 0).crown_base < 2.0);
    }

    #[test]
    fn street_and_yard_species_follow_the_seed_and_the_spot() {
        let mut rows = HashSet::new();
        let mut yards = HashSet::new();
        for key in 0..64 {
            let species = street_species(7, key);
            assert_eq!(species, street_species(7, key));
            assert!(STREET_SPECIES.contains(&species));
            rows.insert(species);
            yards.insert(yard_species(
                7,
                pampa_core::Vec2::new(key as f32 * 3.3, 12.5),
            ));
        }
        assert_eq!(rows.len(), 3);
        assert_eq!(yards.len(), 4);
        // Another town plants its rows otherwise.
        assert!((0..64).any(|k| street_species(7, k) != street_species(8, k)));
    }
}
