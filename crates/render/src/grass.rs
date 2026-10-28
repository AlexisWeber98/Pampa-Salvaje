//! Grass cards: the paja and flechilla tufts of the field.
//!
//! A tuft is three alpha-cut quads fanned 60 degrees apart round a vertical
//! axis, like the tree leaf cards, each wearing one clump of an atlas cut
//! from Poly Haven's `grass_medium_01` (CC0, see the root CREDITS.md): 6
//! triangles a tuft where the Quaternius tufts they replace had 108 to 212.
//! The atlas has four clumps in a 2x2 grid, so a tuft picks one of
//! [`CARD_VARIANTS`] forms. Paja (dry bunch grass) wears the dry diffuse;
//! flechilla wears the green one, tinted toward the dry-green palette. The
//! colour map carries the opacity in its alpha (`tools/textures_to_ktx2.sh`
//! style, mips from `tools/leaf_atlas.py --mips`) and is alpha-tested like
//! the leaves, so there is no sorting.
//!
//! One mesh a (kind, form) and one material a (kind, tint) are shared by
//! every tuft: the tufts' fade is a [`bevy::camera::visibility::VisibilityRange`]
//! on the entity, which [`crate::flora`] keeps to a few seeded steps.
//! Without an asset server, asset root, compressed texture format or the
//! files, the cards draw a plain colour of the same look.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormatSupport;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use pampa_core::TuftKind;

use crate::AssetRoot;
use crate::facade::materials::load_map_with;
use crate::tree::materials::{LEAF_CUTOFF, clamped};

/// Where the grass texture set lives, under the [`AssetRoot`].
pub const GRASS_TEXTURE_DIR: &str = "textures/grass/grass_medium_01";
/// Clump forms in the atlas (a 2x2 grid).
pub const CARD_VARIANTS: usize = 4;
/// Grass tints from grey-green to dry yellow.
pub const GRASS_TINTS: usize = 4;
/// Quads a tuft is built from.
pub const CARDS_PER_TUFT: usize = 3;

/// Index of `kind` in the per-kind tables.
fn slot(kind: TuftKind) -> usize {
    match kind {
        TuftKind::Paja => 0,
        TuftKind::Flechilla => 1,
    }
}

/// The path of `kind`'s colour map, under the [`AssetRoot`]: paja the dry
/// diffuse, flechilla the green one.
pub fn grass_map_path(kind: TuftKind) -> String {
    let name = match kind {
        TuftKind::Paja => "dry_color",
        TuftKind::Flechilla => "green_color",
    };
    format!("{GRASS_TEXTURE_DIR}/{name}.ktx2")
}

/// How wide a card is, for a height of 1: paja is a tall bunch, flechilla
/// spreads wider.
fn card_width(kind: TuftKind) -> f32 {
    match kind {
        TuftKind::Paja => 1.1,
        TuftKind::Flechilla => 1.5,
    }
}

/// The mesh of `kind`'s clump `variant`: [`CARDS_PER_TUFT`] quads one meter
/// tall standing on the origin, fanned evenly round the vertical axis, each
/// showing the clump's cell of the atlas with its bottom edge on the ground.
pub fn card_mesh(kind: TuftKind, variant: usize) -> Mesh {
    let half = 0.5 * card_width(kind);
    let (u0, v0) = ((variant % 2) as f32 * 0.5, (variant / 2) as f32 * 0.5);
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for card in 0..CARDS_PER_TUFT {
        let angle = card as f32 / CARDS_PER_TUFT as f32 * std::f32::consts::PI;
        let side = Vec3::new(angle.cos(), 0.0, angle.sin()) * half;
        let first = positions.len() as u32;
        positions.extend([
            (-side).to_array(),
            side.to_array(),
            (side + Vec3::Y).to_array(),
            (Vec3::Y - side).to_array(),
        ]);
        uvs.extend([
            [u0, v0 + 0.5],
            [u0 + 0.5, v0 + 0.5],
            [u0 + 0.5, v0],
            [u0, v0],
        ]);
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    }
    // Normals all straight up: a tuft is lit like the ground under it,
    // front and back alike.
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// `kind`'s tint `tint` (sRGB), from grey-green to dry yellow. It
/// multiplies the map: paja's dry diffuse only shades, flechilla's green
/// one is pushed toward the dry-green palette the old tufts had.
fn tint_color(kind: TuftKind, tint: usize) -> Color {
    let k = tint as f32 / (GRASS_TINTS - 1) as f32;
    let (from, to) = match kind {
        TuftKind::Paja => (Vec3::new(0.80, 0.82, 0.68), Vec3::new(1.0, 0.95, 0.82)),
        TuftKind::Flechilla => (Vec3::new(0.80, 0.88, 0.62), Vec3::new(1.0, 0.88, 0.44)),
    };
    let c = from.lerp(to, k);
    Color::srgb(c.x, c.y, c.z)
}

/// The plain colour of `kind`'s cards without their map.
fn plain_color(kind: TuftKind, tint: usize) -> Color {
    let k = tint as f32 / (GRASS_TINTS - 1) as f32;
    let (from, to) = match kind {
        TuftKind::Paja => (Vec3::new(0.50, 0.46, 0.28), Vec3::new(0.78, 0.68, 0.44)),
        TuftKind::Flechilla => (Vec3::new(0.38, 0.42, 0.24), Vec3::new(0.70, 0.64, 0.36)),
    };
    let c = from.lerp(to, k);
    Color::srgb(c.x, c.y, c.z)
}

/// `kind`'s card material in tint `tint`, wearing `map` when it is loading:
/// alpha-tested and drawn from both sides, like the leaf cards.
pub fn card_material(kind: TuftKind, tint: usize, map: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial {
        base_color: if map.is_some() {
            tint_color(kind, tint)
        } else {
            plain_color(kind, tint)
        },
        base_color_texture: map,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.3,
        alpha_mode: AlphaMode::Mask(LEAF_CUTOFF),
        cull_mode: None,
        // Not `double_sided`: the normals already point up on both faces.
        double_sided: false,
        ..default()
    }
}

/// The cards' meshes and materials, shared by every tuft.
#[derive(Clone)]
pub struct GrassCards {
    /// By kind, then form.
    meshes: Vec<Handle<Mesh>>,
    /// By kind, then tint.
    materials: Vec<Handle<StandardMaterial>>,
}

impl GrassCards {
    /// Builds the cards, wearing `maps` (paja's, flechilla's) when loaded.
    pub fn new(
        maps: [Option<Handle<Image>>; 2],
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
    ) -> Self {
        let kinds = [TuftKind::Paja, TuftKind::Flechilla];
        Self {
            meshes: kinds
                .iter()
                .flat_map(|&kind| (0..CARD_VARIANTS).map(move |v| (kind, v)))
                .map(|(kind, v)| meshes.add(card_mesh(kind, v)))
                .collect(),
            materials: kinds
                .iter()
                .flat_map(|&kind| (0..GRASS_TINTS).map(move |t| (kind, t)))
                .map(|(kind, t)| materials.add(card_material(kind, t, maps[slot(kind)].clone())))
                .collect(),
        }
    }

    /// The mesh and material of `kind`'s clump `variant` in tint `tint`.
    pub fn card(
        &self,
        kind: TuftKind,
        variant: usize,
        tint: usize,
    ) -> (Handle<Mesh>, Handle<StandardMaterial>) {
        (
            self.meshes[slot(kind) * CARD_VARIANTS + variant.min(CARD_VARIANTS - 1)].clone(),
            self.materials[slot(kind) * GRASS_TINTS + tint.min(GRASS_TINTS - 1)].clone(),
        )
    }
}

/// Starts loading both colour maps (paja's, flechilla's), each `None` when
/// an asset server, an asset root or a compressed texture format is
/// missing, or its file is (with a warning for the file).
pub fn load_maps(
    root: Option<&AssetRoot>,
    server: Option<&AssetServer>,
    support: Option<&CompressedImageFormatSupport>,
) -> [Option<Handle<Image>>; 2] {
    let compressed = support.is_none_or(|s| !s.0.is_empty());
    let (Some(root), Some(server), true) = (root, server, compressed) else {
        return [None, None];
    };
    [TuftKind::Paja, TuftKind::Flechilla].map(|kind| {
        let path = grass_map_path(kind);
        if !Path::new(&root.0).join(&path).is_file() {
            warn!("grass texture {path} not loaded (missing); drawing plain colour");
            return None;
        }
        Some(load_map_with(server, &path, true, clamped()))
    })
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    const KINDS: [TuftKind; 2] = [TuftKind::Paja, TuftKind::Flechilla];

    fn floats(mesh: &Mesh, attribute: bevy::mesh::MeshVertexAttribute) -> Vec<Vec<f32>> {
        match mesh.attribute(attribute) {
            Some(VertexAttributeValues::Float32x3(v)) => v.iter().map(|p| p.to_vec()).collect(),
            Some(VertexAttributeValues::Float32x2(v)) => v.iter().map(|p| p.to_vec()).collect(),
            _ => Vec::new(),
        }
    }

    fn tris(mesh: &Mesh) -> usize {
        mesh.indices().map_or(0, |i| i.len() / 3)
    }

    #[test]
    fn a_tuft_is_a_few_fanned_cards_standing_on_the_ground() {
        for kind in KINDS {
            for variant in 0..CARD_VARIANTS {
                let mesh = card_mesh(kind, variant);
                assert_eq!(tris(&mesh), 2 * CARDS_PER_TUFT, "{kind:?} {variant}");
                let at = floats(&mesh, Mesh::ATTRIBUTE_POSITION);
                assert_eq!(at.len(), 4 * CARDS_PER_TUFT);
                let lowest = at.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
                let highest = at.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
                assert_eq!((lowest, highest), (0.0, 1.0));
                // The cards fan out in different planes: not one flat quad.
                assert!(at.iter().any(|p| p[0].abs() > 0.1));
                assert!(at.iter().any(|p| p[2].abs() > 0.1));
                // Normals lean up so a tuft lights like the ground.
                assert!(
                    floats(&mesh, Mesh::ATTRIBUTE_NORMAL)
                        .iter()
                        .all(|n| n[1] > 0.5)
                );
            }
        }
    }

    #[test]
    fn each_variant_reads_its_own_cell_of_the_atlas() {
        for kind in KINDS {
            let mut cells = Vec::new();
            for variant in 0..CARD_VARIANTS {
                let uv = floats(&card_mesh(kind, variant), Mesh::ATTRIBUTE_UV_0);
                assert_eq!(uv.len(), 4 * CARDS_PER_TUFT);
                let u0 = (variant % 2) as f32 * 0.5;
                let u1 = u0 + 0.5;
                let v0 = (variant / 2) as f32 * 0.5;
                for p in &uv {
                    assert!(u0 <= p[0] && p[0] <= u1, "{kind:?} {variant}: {p:?}");
                    assert!(v0 <= p[1] && p[1] <= v0 + 0.5, "{kind:?} {variant}: {p:?}");
                }
                // The base of the card is the cell's bottom edge.
                let base = floats(&card_mesh(kind, variant), Mesh::ATTRIBUTE_POSITION)
                    .iter()
                    .zip(&uv)
                    .all(|(p, t)| (p[1] == 0.0) == (t[1] == v0 + 0.5));
                assert!(base);
                cells.push((uv[0][0].to_bits(), uv[0][1].to_bits()));
            }
            cells.sort();
            cells.dedup();
            assert_eq!(cells.len(), CARD_VARIANTS);
        }
    }

    #[test]
    fn cards_are_alpha_cut_with_the_map_worn() {
        let map = Handle::<Image>::default();
        for kind in KINDS {
            for tint in 0..GRASS_TINTS {
                let m = card_material(kind, tint, Some(map.clone()));
                assert_eq!(m.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
                assert_eq!(m.cull_mode, None, "drawn from both sides");
                assert!(!m.double_sided, "no normal flip on the back");
                assert_eq!(m.base_color_texture, Some(map.clone()));
            }
            // Without the map: the same cards in a plain colour, still cut.
            let plain = card_material(kind, 0, None);
            assert!(plain.base_color_texture.is_none());
            assert_eq!(plain.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
        }
        // Dry to green: the tints differ.
        let tints: Vec<Color> = (0..GRASS_TINTS)
            .map(|t| card_material(TuftKind::Flechilla, t, Some(map.clone())).base_color)
            .collect();
        for (i, a) in tints.iter().enumerate() {
            assert!(tints[i + 1..].iter().all(|b| a != b));
        }
    }

    #[test]
    fn paja_wears_the_dry_map_and_flechilla_the_green() {
        let mut images = Assets::<Image>::default();
        let (dry, green) = (images.add(Image::default()), images.add(Image::default()));
        let (mut meshes, mut materials) = (Assets::<Mesh>::default(), Assets::default());
        let cards = GrassCards::new(
            [Some(dry.clone()), Some(green.clone())],
            &mut meshes,
            &mut materials,
        );
        for variant in 0..CARD_VARIANTS {
            for tint in 0..GRASS_TINTS {
                let worn = |kind| {
                    let (_, material) = cards.card(kind, variant, tint);
                    materials.get(&material).unwrap().base_color_texture.clone()
                };
                assert_eq!(worn(TuftKind::Paja), Some(dry.clone()));
                assert_eq!(worn(TuftKind::Flechilla), Some(green.clone()));
            }
        }
        assert_ne!(dry, green);
    }

    #[test]
    fn cards_share_a_few_meshes_and_materials() {
        let (mut meshes, mut materials) = (Assets::<Mesh>::default(), Assets::default());
        let cards = GrassCards::new([None, None], &mut meshes, &mut materials);
        assert_eq!(meshes.len(), 2 * CARD_VARIANTS);
        assert_eq!(materials.len(), 2 * GRASS_TINTS);
        let mut ids = Vec::new();
        for kind in KINDS {
            for variant in 0..CARD_VARIANTS {
                ids.push(cards.card(kind, variant, 0).0.id());
            }
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 2 * CARD_VARIANTS);
    }

    #[test]
    fn the_shipped_maps_are_where_the_cards_look() {
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let paths: Vec<String> = KINDS.iter().map(|&k| grass_map_path(k)).collect();
        assert_ne!(paths[0], paths[1]);
        assert!(paths[0].contains("dry"), "{}", paths[0]);
        assert!(paths[1].contains("green"), "{}", paths[1]);
        for path in &paths {
            assert!(path.starts_with(GRASS_TEXTURE_DIR));
            assert!(assets.join(path).is_file(), "{path}");
        }
    }
}
