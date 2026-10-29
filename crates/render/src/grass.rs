//! Tarjetas de pasto: las matas de paja y flechilla del campo.
//!
//! Una mata son tres quads recortados por alfa, abiertos en abanico a 60 grados
//! entre sí alrededor de un eje vertical, como las tarjetas de hojas de los
//! árboles. Cada uno lleva una macolla de un atlas recortado de
//! `grass_medium_01` de Poly Haven (CC0, ver el CREDITS.md de la raíz): 6
//! triángulos por mata, donde las matas de Quaternius a las que reemplazan
//! tenían de 108 a 212. El atlas tiene cuatro macollas en una grilla de 2x2,
//! así que una mata elige una de las [`CARD_VARIANTS`] formas. La paja
//! (gramínea seca en macolla) lleva el difuso seco; la flechilla lleva el
//! verde, teñido hacia la paleta verde seca. El mapa de color lleva la opacidad
//! en su alfa (al estilo de `tools/textures_to_ktx2.sh`, con mips de
//! `tools/leaf_atlas.py --mips`) y se prueba por alfa como las hojas, así que
//! no hay ordenamiento.
//!
//! Una malla por (tipo, forma) y un material por (tipo, tinte) se comparten
//! entre todas las matas: el fundido de las matas es un
//! [`bevy::camera::visibility::VisibilityRange`] en la entidad, que
//! [`crate::flora`] limita a unos pocos escalones con semilla. Sin servidor de
//! assets, raíz de assets, formato de textura comprimida o los archivos, las
//! tarjetas se dibujan en un color liso del mismo aspecto.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormatSupport;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use pampa_core::TuftKind;

use crate::AssetRoot;
use crate::facade::materials::load_map_with;
use crate::tree::materials::{LEAF_CUTOFF, clamped};

/// Dónde está el conjunto de texturas de pasto, bajo el [`AssetRoot`].
pub const GRASS_TEXTURE_DIR: &str = "textures/grass/grass_medium_01";
/// Formas de macolla en el atlas (una grilla de 2x2).
pub const CARD_VARIANTS: usize = 4;
/// Tintes del pasto, de verde grisáceo a amarillo seco.
pub const GRASS_TINTS: usize = 4;
/// Quads con los que se construye una mata.
pub const CARDS_PER_TUFT: usize = 3;

/// Índice de `kind` en las tablas por tipo.
fn slot(kind: TuftKind) -> usize {
    match kind {
        TuftKind::Paja => 0,
        TuftKind::Flechilla => 1,
    }
}

/// La ruta del mapa de color de `kind`, bajo el [`AssetRoot`]: la paja usa el
/// difuso seco y la flechilla el verde.
pub fn grass_map_path(kind: TuftKind) -> String {
    let name = match kind {
        TuftKind::Paja => "dry_color",
        TuftKind::Flechilla => "green_color",
    };
    format!("{GRASS_TEXTURE_DIR}/{name}.ktx2")
}

/// Qué tan ancha es una tarjeta para una altura de 1: la paja es una macolla
/// alta y la flechilla se extiende más a lo ancho.
fn card_width(kind: TuftKind) -> f32 {
    match kind {
        TuftKind::Paja => 1.1,
        TuftKind::Flechilla => 1.5,
    }
}

/// La malla de la macolla `variant` de `kind`: [`CARDS_PER_TUFT`] quads de un
/// metro de alto parados sobre el origen, repartidos en abanico alrededor del
/// eje vertical, cada uno mostrando la celda del atlas de la macolla con su
/// borde inferior sobre el suelo.
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
    // Todas las normales apuntan derecho hacia arriba: una mata se ilumina como
    // el suelo que tiene debajo, por delante y por detrás por igual.
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

/// El tinte `tint` de `kind` (sRGB), de verde grisáceo a amarillo seco.
/// Multiplica el mapa: el difuso seco de la paja solo se sombrea, y el verde de
/// la flechilla se lleva hacia la paleta verde seca que tenían las matas
/// anteriores.
fn tint_color(kind: TuftKind, tint: usize) -> Color {
    let k = tint as f32 / (GRASS_TINTS - 1) as f32;
    let (from, to) = match kind {
        TuftKind::Paja => (Vec3::new(0.80, 0.82, 0.68), Vec3::new(1.0, 0.95, 0.82)),
        TuftKind::Flechilla => (Vec3::new(0.80, 0.88, 0.62), Vec3::new(1.0, 0.88, 0.44)),
    };
    let c = from.lerp(to, k);
    Color::srgb(c.x, c.y, c.z)
}

/// El color liso de las tarjetas de `kind` sin su mapa.
fn plain_color(kind: TuftKind, tint: usize) -> Color {
    let k = tint as f32 / (GRASS_TINTS - 1) as f32;
    let (from, to) = match kind {
        TuftKind::Paja => (Vec3::new(0.50, 0.46, 0.28), Vec3::new(0.78, 0.68, 0.44)),
        TuftKind::Flechilla => (Vec3::new(0.38, 0.42, 0.24), Vec3::new(0.70, 0.64, 0.36)),
    };
    let c = from.lerp(to, k);
    Color::srgb(c.x, c.y, c.z)
}

/// El material de tarjeta de `kind` en el tinte `tint`, con `map` cuando se
/// está cargando: probado por alfa y dibujado por ambas caras, como las
/// tarjetas de hojas.
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
        // No es `double_sided`: las normales ya apuntan hacia arriba en ambas
        // caras.
        double_sided: false,
        ..default()
    }
}

/// Las mallas y materiales de las tarjetas, compartidos por todas las matas.
#[derive(Clone)]
pub struct GrassCards {
    /// Por tipo, luego por forma.
    meshes: Vec<Handle<Mesh>>,
    /// Por tipo, luego por tinte.
    materials: Vec<Handle<StandardMaterial>>,
}

impl GrassCards {
    /// Construye las tarjetas, con `maps` (el de la paja, el de la flechilla)
    /// cuando están cargados.
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

    /// La malla y el material de la macolla `variant` de `kind` en el tinte
    /// `tint`.
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

/// Empieza a cargar ambos mapas de color (el de la paja, el de la flechilla),
/// cada uno `None` cuando falta un servidor de assets, una raíz de assets o un
/// formato de textura comprimida, o falta su archivo (con una advertencia en el
/// caso del archivo).
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
                // Las tarjetas se abren en abanico en planos distintos: no es
                // un único quad plano.
                assert!(at.iter().any(|p| p[0].abs() > 0.1));
                assert!(at.iter().any(|p| p[2].abs() > 0.1));
                // Las normales se inclinan hacia arriba para que una mata se
                // ilumine como el suelo.
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
                // La base de la tarjeta es el borde inferior de la celda.
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
            // Sin el mapa: las mismas tarjetas en un color liso, todavía
            // recortadas.
            let plain = card_material(kind, 0, None);
            assert!(plain.base_color_texture.is_none());
            assert_eq!(plain.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
        }
        // De seco a verde: los tintes difieren.
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
