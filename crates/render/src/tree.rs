//! The towns' plaza trees: procedural paraísos, olmos, eucaliptos and
//! aromos, the trees of 1919 pampa plazas.
//!
//! Each [`Species`] has [`VARIANTS`] seeded trees ([`mesh`]): tapered bark
//! trunks and limbs carrying alpha-tested leaf cards ([`materials`]). They
//! are built once at startup into the shared [`TreeKit`], two meshes a
//! variant and two materials a species, so a town's trees cost no new mesh
//! or material: each plaza tree is a variant's bark and leaves, turned and
//! scaled. Core places the trunks ([`pampa_core::TownLayout::trees`]); the
//! species of each plaza is picked here from the town's seed
//! ([`plaza_species`]), so the layout does not change.

pub mod materials;
pub(crate) mod mesh;

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;

use crate::AssetRoot;

/// Seeded variants built per species.
pub const VARIANTS: usize = 3;

/// A plaza tree's species.
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
}

impl Species {
    /// Every species, in order.
    pub const ALL: [Species; 4] = [Self::Paraiso, Self::Olmo, Self::Eucalipto, Self::Aromo];
}

/// The species planted in plaza `plaza` (0: the main one, 1: the
/// station's, see [`pampa_core::TownLayout::plazas`]) of the town seeded
/// `seed`. A town's two plazas never share a species.
pub fn plaza_species(seed: u32, plaza: usize) -> Species {
    let mix = |salt: u32| {
        let mut h = seed.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^ (h >> 12)
    };
    let main = mix(0x7EE5) as usize % Species::ALL.len();
    let k = if plaza == 0 {
        main
    } else {
        main + 1 + (mix(0x57A7 + plaza as u32) as usize % (Species::ALL.len() - 1))
    };
    Species::ALL[k % Species::ALL.len()]
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
}

/// One species' variants and its two materials.
#[derive(Debug, Clone)]
pub struct SpeciesKit {
    pub variants: Vec<TreeVariant>,
    pub bark: Handle<StandardMaterial>,
    pub leaves: Handle<StandardMaterial>,
}

/// The shared plaza tree meshes and materials, built at startup.
#[derive(Resource, Debug, Clone)]
pub struct TreeKit(Vec<SpeciesKit>);

impl TreeKit {
    /// The kit of `species`.
    pub fn species(&self, species: Species) -> &SpeciesKit {
        &self.0[species as usize]
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

fn setup_trees(
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
    let kits = Species::ALL
        .iter()
        .zip(paint)
        .map(|(&species, [bark, leaves])| SpeciesKit {
            variants: (0..VARIANTS as u32)
                .map(|v| {
                    let tree = mesh::tree(species, v);
                    TreeVariant {
                        height: tree.height,
                        crown_radius: tree.crown_radius,
                        bark: meshes.add(tree.bark.into_mesh()),
                        leaves: meshes.add(tree.leaves.into_mesh()),
                    }
                })
                .collect(),
            bark,
            leaves,
        })
        .collect();
    commands.insert_resource(TreeKit(kits));
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
        for species in Species::ALL {
            let k = kit.species(species);
            assert_eq!(k.variants.len(), VARIANTS);
            for v in &k.variants {
                meshes.insert(v.bark.id());
                meshes.insert(v.leaves.id());
                assert!(v.height > 3.0 && v.crown_radius > 1.0, "{species:?}");
            }
            materials.insert(k.bark.id());
            materials.insert(k.leaves.id());
        }
        assert_eq!(meshes.len(), 2 * VARIANTS * Species::ALL.len());
        assert_eq!(materials.len(), 2 * Species::ALL.len());
        assert_eq!(
            app.world().resource::<Assets<Mesh>>().len(),
            2 * VARIANTS * Species::ALL.len()
        );
    }
}
