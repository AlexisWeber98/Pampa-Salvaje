//! The plaza trees' materials: one bark and one leaf material per
//! [`Species`].
//!
//! Each wears a KTX2 set from `assets/textures/trees/` (ambientCG, CC0; see
//! that folder's CREDITS.md), made by `tools/textures_to_ktx2.sh` like the
//! building sets: bark tiles with the repeating sampler; the leaf atlas
//! (`tools/leaf_atlas.py`) carries its opacity in the colour's alpha and is
//! sampled clamped to its edges. Leaves are alpha-tested
//! ([`AlphaMode::Mask`]): no sorting, and their cut-outs cast shadows.
//! The atlas's mips are made so that each level draws the share of texels
//! over [`LEAF_CUTOFF`] that the full-size atlas draws (`leaf_atlas.py
//! --mips`): plain mips average a thin leaf with the clear ground round
//! it, under the cutoff, and distant crowns thinned out. Bevy's
//! [`AlphaMode::AlphaToCoverage`] would not help: it sharpens alpha round
//! the same fixed 0.5, and cuts its shadows and prepass at 0.05.
//! They are drawn from both sides without Bevy's double-sided normal flip,
//! so a card's back keeps the normal leaning out of the crown that its
//! front has (see [`super::mesh`]).
//!
//! Without an asset server, on a GPU without compressed formats, or when a
//! set's files are missing, the bark and the cards draw a plain colour of
//! the same look.

use std::path::Path;

use bevy::image::{CompressedImageFormatSupport, ImageAddressMode, ImageSamplerDescriptor};
use bevy::prelude::*;

use super::Species;
use crate::AssetRoot;
use crate::facade::materials::{MAPS, load_map_with, repeating};

/// Where the tree texture sets live, under the [`AssetRoot`].
pub const TREE_TEXTURE_DIR: &str = "textures/trees";

/// The alpha a leaf texel needs to be drawn: `leaf_atlas.py`'s `CUTOFF`
/// (128 of 255) keeps the atlas mips' coverage for it.
pub const LEAF_CUTOFF: f32 = 0.5;

/// The path of map `name` of tree set `set`, under the [`AssetRoot`].
pub fn tree_map_path(set: &str, name: &str) -> String {
    format!("{TREE_TEXTURE_DIR}/{set}/{name}.ktx2")
}

impl Species {
    /// Its bark set.
    pub fn bark_set(self) -> &'static str {
        match self {
            Self::Paraiso => "Bark014",
            Self::Olmo => "Bark001",
            Self::Eucalipto => "Bark009",
            Self::Aromo => "Bark002",
        }
    }

    /// Its leaf atlas, composed by `tools/leaf_atlas.py`.
    pub fn leaf_set(self) -> &'static str {
        match self {
            Self::Paraiso => "paraiso_leaves",
            Self::Olmo => "olmo_leaves",
            Self::Eucalipto => "eucalipto_leaves",
            Self::Aromo => "aromo_leaves",
        }
    }

    /// The leaves' tint over the atlas (sRGB): the paraíso's yellow scan
    /// deepened, the eucalipto greyed, the aromo silvered.
    fn leaf_tint(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.78, 0.9, 0.7),
            Self::Olmo => Color::srgb(0.95, 1.0, 0.92),
            Self::Eucalipto => Color::srgb(0.86, 0.95, 0.95),
            Self::Aromo => Color::srgb(0.84, 0.94, 0.9),
        }
    }

    /// The leaves' plain colour, without their atlas.
    fn leaf_plain(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.32, 0.40, 0.18),
            Self::Olmo => Color::srgb(0.27, 0.34, 0.17),
            Self::Eucalipto => Color::srgb(0.36, 0.42, 0.33),
            Self::Aromo => Color::srgb(0.30, 0.38, 0.27),
        }
    }

    /// The bark's plain colour, without its maps.
    fn bark_plain(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.30, 0.25, 0.20),
            Self::Olmo => Color::srgb(0.33, 0.29, 0.24),
            Self::Eucalipto => Color::srgb(0.55, 0.50, 0.43),
            Self::Aromo => Color::srgb(0.28, 0.22, 0.17),
        }
    }
}

/// A set's loading maps: colour, normal and ORM.
pub(crate) struct Maps {
    pub color: Handle<Image>,
    pub normal: Handle<Image>,
    pub orm: Handle<Image>,
}

/// The clamped sampler of the leaf atlases: a card's edge never wraps
/// round to the far side of the atlas.
pub(crate) fn clamped() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..repeating()
    }
}

/// Starts loading tree set `set` with `sampler`, or warns and returns
/// `None` when one of its files is missing.
fn load_set(
    server: &AssetServer,
    root: &Path,
    set: &str,
    sampler: ImageSamplerDescriptor,
) -> Option<Maps> {
    let missing: Vec<String> = MAPS
        .into_iter()
        .map(|(name, _)| tree_map_path(set, name))
        .filter(|path| !root.join(path).is_file())
        .collect();
    if !missing.is_empty() {
        warn!(
            "tree textures {TREE_TEXTURE_DIR}/{set} not loaded (missing {}); \
             drawing plain colour",
            missing.join(", ")
        );
        return None;
    }
    let [color, normal, orm] = MAPS.map(|(name, srgb)| {
        load_map_with(server, &tree_map_path(set, name), srgb, sampler.clone())
    });
    Some(Maps { color, normal, orm })
}

/// The bark and leaf materials of every species, in [`Species::ALL`]
/// order, loading their maps when an asset server, an asset root and a
/// compressed texture format are there.
pub(crate) fn tree_materials(
    root: Option<&AssetRoot>,
    server: Option<&AssetServer>,
    support: Option<&CompressedImageFormatSupport>,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<[Handle<StandardMaterial>; 2]> {
    let compressed = support.is_none_or(|s| !s.0.is_empty());
    let load = |set: &str, sampler| match (root, server) {
        (Some(root), Some(server)) if compressed => load_set(server, &root.0, set, sampler),
        _ => None,
    };
    Species::ALL
        .iter()
        .map(|&s| {
            let bark = bark_material(s, load(s.bark_set(), repeating()));
            let leaves = leaf_material(s, load(s.leaf_set(), clamped()));
            [materials.add(bark), materials.add(leaves)]
        })
        .collect()
}

/// Wears `maps` on `material`, occlusion and roughness from the one ORM map.
fn wear(material: &mut StandardMaterial, maps: Maps) {
    material.base_color_texture = Some(maps.color);
    material.normal_map_texture = Some(maps.normal);
    material.metallic_roughness_texture = Some(maps.orm.clone());
    material.occlusion_texture = Some(maps.orm);
}

/// The bark of `species`.
pub(crate) fn bark_material(species: Species, maps: Option<Maps>) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: if maps.is_some() {
            Color::WHITE
        } else {
            species.bark_plain()
        },
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.3,
        ..default()
    };
    if let Some(maps) = maps {
        wear(&mut material, maps);
    }
    material
}

/// The leaf cards of `species`: alpha-tested, drawn from both sides.
pub(crate) fn leaf_material(species: Species, maps: Option<Maps>) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: if maps.is_some() {
            species.leaf_tint()
        } else {
            species.leaf_plain()
        },
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.35,
        alpha_mode: AlphaMode::Mask(LEAF_CUTOFF),
        cull_mode: None,
        // Not `double_sided`: that would flip the back's normals into the
        // crown, darkening every card seen from behind.
        double_sided: false,
        ..default()
    };
    if let Some(maps) = maps {
        wear(&mut material, maps);
    }
    material
}

#[cfg(test)]
mod tests {
    use bevy::image::ImageAddressMode;

    use super::*;

    fn maps() -> Maps {
        Maps {
            color: Handle::default(),
            normal: Handle::default(),
            orm: Handle::default(),
        }
    }

    #[test]
    fn every_species_names_shipped_bark_and_leaf_sets() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for species in Species::ALL {
            for set in [species.bark_set(), species.leaf_set()] {
                for (map, _) in MAPS {
                    let file = root.join(tree_map_path(set, map));
                    assert!(file.is_file(), "{}", file.display());
                }
            }
        }
    }

    #[test]
    fn leaves_are_alpha_tested_cards_seen_from_both_sides() {
        for species in Species::ALL {
            for textured in [true, false] {
                let m = leaf_material(species, textured.then(maps));
                assert_eq!(m.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF), "{species:?}");
                assert_eq!(m.cull_mode, None);
                assert!(!m.double_sided, "no normal flip on the back");
                assert_eq!(m.base_color_texture.is_some(), textured);
                assert_eq!(m.normal_map_texture.is_some(), textured);
                let bark = bark_material(species, textured.then(maps));
                assert_eq!(bark.alpha_mode, AlphaMode::Opaque);
                assert!(bark.cull_mode.is_some(), "bark culls its insides");
                assert_eq!(bark.occlusion_texture.is_some(), textured);
            }
        }
    }

    #[test]
    fn the_atlas_mips_keep_coverage_for_this_cutoff() {
        // The lowest byte alpha drawn is the one `leaf_atlas.py` keeps
        // each mip's share of texels over.
        let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/leaf_atlas.py");
        let text = std::fs::read_to_string(tool).unwrap();
        let cutoff: u32 = text
            .lines()
            .find_map(|l| l.strip_prefix("CUTOFF = "))
            .expect("leaf_atlas.py sets CUTOFF")
            .parse()
            .unwrap();
        assert_eq!(cutoff, (LEAF_CUTOFF * 255.0).ceil() as u32);
    }

    #[test]
    fn the_atlas_clamps_and_the_bark_repeats() {
        let atlas = clamped();
        assert_eq!(atlas.address_mode_u, ImageAddressMode::ClampToEdge);
        assert_eq!(atlas.address_mode_v, ImageAddressMode::ClampToEdge);
        let bark = repeating();
        assert_eq!(bark.address_mode_u, ImageAddressMode::Repeat);
        assert_eq!(bark.address_mode_v, ImageAddressMode::Repeat);
    }

    #[test]
    fn missing_sets_draw_plain_colour() {
        let mut materials = Assets::<StandardMaterial>::default();
        // No asset server: nothing loads, every species still has both.
        let built = tree_materials(None, None, None, &mut materials);
        assert_eq!(built.len(), Species::ALL.len());
        for (species, [bark, leaves]) in Species::ALL.iter().zip(&built) {
            let (bark, leaves) = (materials.get(bark).unwrap(), materials.get(leaves).unwrap());
            assert!(bark.base_color_texture.is_none() && leaves.base_color_texture.is_none());
            assert_eq!(leaves.base_color, species.leaf_plain());
            assert_eq!(bark.base_color, species.bark_plain());
        }
    }
}
