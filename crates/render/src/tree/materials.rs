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
    /// Its bark set, by name under [`TREE_TEXTURE_DIR`]. A species with
    /// its own set only needs the set's KTX2 files added there and its one
    /// line here changed. Every species wears a set of its own, so the
    /// tints only shade the real texture. The Poly Haven sets
    /// (`bark_willow`, `bark_platanus`, `bark_bluegum`) are lower-case; the
    /// ambientCG ones are `BarkNNN`.
    pub fn bark_set(self) -> &'static str {
        match self {
            Self::Paraiso => "Bark014",
            Self::Olmo => "Bark001",
            // Mottled smooth grey-beige bark, as photographed.
            Self::Eucalipto => "bark_bluegum",
            Self::Aromo => "Bark002",
            // Dark, deeply furrowed willow bark, shaded grey-brown.
            Self::Calden => "bark_willow",
            // Peeling plane-tree strips, shaded greenish ochre.
            Self::Chanar => "bark_platanus",
        }
    }

    /// Its leaf atlas, composed by `tools/leaf_atlas.py`.
    pub fn leaf_set(self) -> &'static str {
        match self {
            Self::Paraiso => "paraiso_leaves",
            Self::Olmo => "olmo_leaves",
            Self::Eucalipto => "eucalipto_leaves",
            Self::Aromo => "aromo_leaves",
            // Fine feathery sprays, the aromo's atlas, in a dull grey-green.
            Self::Calden => "aromo_leaves",
            // Small simple leaves along a twig, the olmo's atlas, yellower.
            Self::Chanar => "olmo_leaves",
        }
    }

    /// The leaves' tint `tint` over the atlas (sRGB). Tint 0 is the
    /// species' own, the paraíso's yellow scan deepened, the eucalipto
    /// greyed, the aromo silvered; the field species have two more
    /// ([`Species::tints`]), a fresher and a bluer-grey (the chañar's a
    /// yellower one).
    fn leaf_tint(self, tint: usize) -> Color {
        match (self, tint) {
            (Self::Calden, 1) => Color::srgb(0.76, 0.82, 0.58),
            (Self::Calden, 2) => Color::srgb(0.56, 0.66, 0.58),
            (Self::Chanar, 1) => Color::srgb(0.68, 0.84, 0.52),
            (Self::Chanar, 2) => Color::srgb(0.92, 0.9, 0.58),
            _ => self.leaf_tint_base(),
        }
    }

    fn leaf_tint_base(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.78, 0.9, 0.7),
            Self::Olmo => Color::srgb(0.95, 1.0, 0.92),
            Self::Eucalipto => Color::srgb(0.86, 0.95, 0.95),
            Self::Aromo => Color::srgb(0.84, 0.94, 0.9),
            Self::Calden => Color::srgb(0.66, 0.74, 0.62),
            Self::Chanar => Color::srgb(0.8, 0.88, 0.6),
        }
    }

    /// The leaves' plain colour, without their atlas.
    fn leaf_plain(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.32, 0.40, 0.18),
            Self::Olmo => Color::srgb(0.27, 0.34, 0.17),
            Self::Eucalipto => Color::srgb(0.36, 0.42, 0.33),
            Self::Aromo => Color::srgb(0.30, 0.38, 0.27),
            Self::Calden => Color::srgb(0.28, 0.33, 0.22),
            Self::Chanar => Color::srgb(0.38, 0.44, 0.24),
        }
    }

    /// The bark's plain colour, without its maps.
    fn bark_plain(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.30, 0.25, 0.20),
            Self::Olmo => Color::srgb(0.33, 0.29, 0.24),
            Self::Eucalipto => Color::srgb(0.55, 0.50, 0.43),
            Self::Aromo => Color::srgb(0.28, 0.22, 0.17),
            Self::Calden => Color::srgb(0.17, 0.14, 0.11),
            Self::Chanar => Color::srgb(0.48, 0.44, 0.27),
        }
    }

    /// The bark's tint `tint` over its maps (sRGB): the caldén's a dull
    /// grey-brown over its already dark set (its other two darker and
    /// paler), the chañar's strips turned greenish ochre (its other two
    /// greyer and paler).
    pub(crate) fn bark_tint(self, tint: usize) -> Color {
        match (self, tint) {
            (Self::Calden, 1) => Color::srgb(0.7, 0.68, 0.66),
            (Self::Calden, 2) => Color::srgb(1.0, 0.96, 0.9),
            (Self::Chanar, 1) => Color::srgb(0.78, 0.82, 0.6),
            (Self::Chanar, 2) => Color::srgb(1.0, 0.98, 0.78),
            (Self::Calden, _) => Color::srgb(0.84, 0.82, 0.8),
            (Self::Chanar, _) => Color::srgb(0.9, 0.95, 0.68),
            _ => Color::WHITE,
        }
    }
}

/// A set's loading maps: colour, normal and ORM.
#[derive(Clone)]
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

/// A species' bark and leaf materials, a tint each ([`Species::tints`]).
pub(crate) struct Paint {
    pub bark: Vec<Handle<StandardMaterial>>,
    pub leaves: Vec<Handle<StandardMaterial>>,
}

/// The bark and leaf materials of every species, in [`Species::EVERY`]
/// order, loading their maps when an asset server, an asset root and a
/// compressed texture format are there.
pub(crate) fn tree_materials(
    root: Option<&AssetRoot>,
    server: Option<&AssetServer>,
    support: Option<&CompressedImageFormatSupport>,
    materials: &mut Assets<StandardMaterial>,
) -> Vec<Paint> {
    let compressed = support.is_none_or(|s| !s.0.is_empty());
    let load = |set: &str, sampler| match (root, server) {
        (Some(root), Some(server)) if compressed => load_set(server, &root.0, set, sampler),
        _ => None,
    };
    Species::EVERY
        .iter()
        .map(|&s| {
            let (bark_maps, leaf_maps) = (
                load(s.bark_set(), repeating()),
                load(s.leaf_set(), clamped()),
            );
            Paint {
                bark: (0..s.tints())
                    .map(|t| materials.add(bark_material(s, t, bark_maps.clone())))
                    .collect(),
                leaves: (0..s.tints())
                    .map(|t| materials.add(leaf_material(s, t, leaf_maps.clone())))
                    .collect(),
            }
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

/// The bark of `species`, in tint `tint`.
pub(crate) fn bark_material(species: Species, tint: usize, maps: Option<Maps>) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: if maps.is_some() {
            species.bark_tint(tint)
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

/// The leaf cards of `species` in tint `tint`: alpha-tested, drawn from
/// both sides.
pub(crate) fn leaf_material(species: Species, tint: usize, maps: Option<Maps>) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: if maps.is_some() {
            species.leaf_tint(tint)
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
        for species in Species::EVERY {
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
        for species in Species::EVERY {
            for textured in [true, false] {
                let m = leaf_material(species, 0, textured.then(maps));
                assert_eq!(m.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF), "{species:?}");
                assert_eq!(m.cull_mode, None);
                assert!(!m.double_sided, "no normal flip on the back");
                assert_eq!(m.base_color_texture.is_some(), textured);
                assert_eq!(m.normal_map_texture.is_some(), textured);
                let bark = bark_material(species, 0, textured.then(maps));
                assert_eq!(bark.alpha_mode, AlphaMode::Opaque);
                assert!(bark.cull_mode.is_some(), "bark culls its insides");
                assert_eq!(bark.occlusion_texture.is_some(), textured);
            }
        }
    }

    #[test]
    fn the_calden_wears_dark_furrowed_bark_and_the_chanar_greenish_ochre_strips() {
        let (calden, chanar) = (Species::Calden, Species::Chanar);
        // Each has a set of its own: the caldén's is the dark willow bark,
        // shaded grey-brown; the chañar's the peeling plane bark, tinted
        // green-ochre.
        assert_eq!(calden.bark_set(), "bark_willow");
        assert_eq!(chanar.bark_set(), "bark_platanus");
        let c = calden.bark_tint(0).to_srgba();
        let ch = chanar.bark_tint(0).to_srgba();
        assert!(c.red.max(c.green).max(c.blue) < 0.9, "{c:?}");
        assert!(c.red - c.blue < 0.1, "dull, not coloured: {c:?}");
        assert!(ch.green > ch.blue + 0.2 && ch.red > ch.blue + 0.2, "{ch:?}");
        // The other species' bark is left as photographed.
        assert_eq!(Species::Olmo.bark_tint(0), Color::WHITE);
        // The textured bark carries the tint; plain bark its own colour.
        let textured = bark_material(chanar, 0, Some(maps()));
        assert_eq!(textured.base_color, chanar.bark_tint(0));
        // Fine, dull leaves: the caldén's grey-green sits darker than the
        // aromo's silver.
        let (cl, al) = (
            calden.leaf_tint(0).to_srgba(),
            Species::Aromo.leaf_tint(0).to_srgba(),
        );
        assert!(cl.red < al.red && cl.green < al.green);
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
        assert_eq!(built.len(), Species::EVERY.len());
        for (species, paint) in Species::EVERY.iter().zip(&built) {
            let (bark, leaves) = (
                materials.get(&paint.bark[0]).unwrap(),
                materials.get(&paint.leaves[0]).unwrap(),
            );
            assert!(bark.base_color_texture.is_none() && leaves.base_color_texture.is_none());
            assert_eq!(leaves.base_color, species.leaf_plain());
            assert_eq!(bark.base_color, species.bark_plain());
        }
    }

    #[test]
    fn no_two_species_wear_the_same_bark() {
        // Every species has a bark set of its own, so the tints only shade
        // the real texture and never fake a different bark.
        let sets: Vec<&str> = Species::EVERY.iter().map(|s| s.bark_set()).collect();
        for (i, a) in sets.iter().enumerate() {
            for b in &sets[i + 1..] {
                assert_ne!(a, b, "two species share a bark set");
            }
        }
    }

    #[test]
    fn the_field_species_have_tints_that_differ_and_share_their_maps() {
        for species in Species::EVERY {
            let want = if Species::FIELD.contains(&species) {
                super::super::FIELD_TINTS
            } else {
                1
            };
            assert_eq!(species.tints(), want, "{species:?}");
        }
        for species in Species::FIELD {
            let (bark, leaf): (Vec<_>, Vec<_>) = (0..species.tints())
                .map(|t| (species.bark_tint(t), species.leaf_tint(t)))
                .unzip();
            // Three distinct tints of each, none far from the species' own.
            for tints in [&bark, &leaf] {
                for (i, a) in tints.iter().enumerate() {
                    for b in &tints[i + 1..] {
                        assert_ne!(a, b, "{species:?}");
                    }
                    let (a, own) = (a.to_srgba(), tints[0].to_srgba());
                    let drift = (a.red - own.red).abs()
                        + (a.green - own.green).abs()
                        + (a.blue - own.blue).abs();
                    assert!(drift < 0.5, "{species:?}: {drift}");
                }
            }
            // Each tint is its own material over the same maps.
            let (a, b) = (
                bark_material(species, 0, Some(maps())),
                bark_material(species, 1, Some(maps())),
            );
            assert_ne!(a.base_color, b.base_color);
            assert_eq!(a.base_color_texture, b.base_color_texture);
            let (a, b) = (
                leaf_material(species, 0, Some(maps())),
                leaf_material(species, 2, Some(maps())),
            );
            assert_ne!(a.base_color, b.base_color);
            assert_eq!(a.alpha_mode, b.alpha_mode);
        }
        // The first tint is the town kit's own, and the towns' trees have
        // one only.
        assert_eq!(Species::Calden.bark_tint(0), Color::srgb(0.84, 0.82, 0.8));
        assert_eq!(Species::Olmo.leaf_tint(0), Species::Olmo.leaf_tint(3));
        let mut materials = Assets::<StandardMaterial>::default();
        let built = tree_materials(None, None, None, &mut materials);
        for (species, paint) in Species::EVERY.iter().zip(&built) {
            assert_eq!(paint.bark.len(), species.tints());
            assert_eq!(paint.leaves.len(), species.tints());
        }
    }
}
