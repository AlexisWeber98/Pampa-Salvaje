//! Los materiales de los árboles de la plaza: un material de corteza y uno de
//! hojas por [`Species`].
//!
//! Cada uno lleva un conjunto KTX2 de `assets/textures/trees/` (ambientCG, CC0;
//! ver el CREDITS.md de esa carpeta), hecho por `tools/textures_to_ktx2.sh`
//! como los conjuntos de los edificios: la corteza se repite con el sampler de
//! repetición; el atlas de hojas (`tools/leaf_atlas.py`) lleva su opacidad en
//! el alfa del color y se muestrea limitado a sus bordes. Las hojas usan alpha
//! test ([`AlphaMode::Mask`]): sin ordenamiento, y sus recortes proyectan
//! sombras. Los mips del atlas se hacen de modo que cada nivel dibuje la misma
//! proporción de texels por encima de [`LEAF_CUTOFF`] que dibuja el atlas a
//! tamaño completo (`leaf_atlas.py --mips`): los mips comunes promedian una
//! hoja delgada con el fondo transparente que la rodea, por debajo del umbral,
//! y las copas lejanas se ralean. El [`AlphaMode::AlphaToCoverage`] de Bevy no
//! ayudaría: afila el alfa alrededor del mismo 0.5 fijo, y corta sus sombras y
//! su prepass en 0.05. Se dibujan por ambos lados sin la inversión de normales
//! de doble cara de Bevy, así que el dorso de una tarjeta conserva la normal
//! inclinada hacia afuera de la copa que tiene su frente (ver [`super::mesh`]).
//!
//! Sin un servidor de assets, en una GPU sin formatos comprimidos, o cuando
//! faltan los archivos de un conjunto, la corteza y las tarjetas se dibujan con
//! un color liso del mismo aspecto.

use std::path::Path;

use bevy::image::{CompressedImageFormatSupport, ImageAddressMode, ImageSamplerDescriptor};
use bevy::prelude::*;

use super::Species;
use crate::AssetRoot;
use crate::facade::materials::{MAPS, load_map_with, repeating};

/// Dónde viven los conjuntos de texturas de árboles, bajo el [`AssetRoot`].
pub const TREE_TEXTURE_DIR: &str = "textures/trees";

/// El alfa que necesita un texel de hoja para dibujarse: el `CUTOFF` de
/// `leaf_atlas.py` (128 de 255) conserva para él la cobertura de los mips del
/// atlas.
pub const LEAF_CUTOFF: f32 = 0.5;

/// La ruta del mapa `name` del conjunto de árbol `set`, bajo el [`AssetRoot`].
pub fn tree_map_path(set: &str, name: &str) -> String {
    format!("{TREE_TEXTURE_DIR}/{set}/{name}.ktx2")
}

impl Species {
    /// Su conjunto de corteza, por nombre bajo [`TREE_TEXTURE_DIR`]. Para una
    /// especie con su propio conjunto solo hace falta agregar ahí los archivos
    /// KTX2 del conjunto y cambiar aquí su única línea. Cada especie lleva un
    /// conjunto propio, así que los tintes solo matizan la textura real. Los
    /// conjuntos de Poly Haven (`bark_willow`, `bark_platanus`, `bark_bluegum`)
    /// van en minúsculas; los de ambientCG son `BarkNNN`.
    pub fn bark_set(self) -> &'static str {
        match self {
            Self::Paraiso => "Bark014",
            Self::Olmo => "Bark001",
            // Corteza lisa gris beige moteada, tal como se fotografió.
            Self::Eucalipto => "bark_bluegum",
            Self::Aromo => "Bark002",
            // Corteza de sauce oscura y muy surcada, matizada en gris pardo.
            Self::Calden => "bark_willow",
            // Tiras de plátano que se descascaran, matizadas en ocre verdoso.
            Self::Chanar | Self::Fachinal => "bark_platanus",
            // Tallos verticales delgados, oscuros y ásperos.
            Self::Jarilla => "bark_willow_02",
            // Ramitas espinosas gris pardo.
            Self::Piquillin => "Bark009",
            // Corteza gris surcada, el conjunto del olmo matizado en un gris
            // más frío.
            Self::Alamo => "Bark001",
            // Corteza rojiza fibrosa y agrietada.
            Self::Algarrobo => "chinese_cedar_bark",
            // Corteza gris pardo clara que se parte con la edad.
            Self::Tala => "chinese_hackberry_bark",
            // Corteza marrón oscura con grietas oblicuas profundas.
            Self::Espinillo => "bark_brown_01",
            // Corteza clara, lisa y esponjosa.
            Self::Ombu => "Bark005",
        }
    }

    /// Su atlas de hojas, compuesto por `tools/leaf_atlas.py`.
    pub fn leaf_set(self) -> &'static str {
        match self {
            Self::Paraiso => "paraiso_leaves",
            Self::Olmo => "olmo_leaves",
            Self::Eucalipto => "eucalipto_leaves",
            Self::Aromo => "aromo_leaves",
            // Ramilletes finos y plumosos, el atlas del aromo, en un verde
            // grisáceo apagado.
            Self::Calden => "aromo_leaves",
            // Hojas simples pequeñas a lo largo de una ramita, el atlas del
            // olmo, más amarillas.
            Self::Chanar | Self::Fachinal => "olmo_leaves",
            // Hojas diminutas y oscuras: las simples y pequeñas del olmo, verde
            // profundo.
            Self::Piquillin => "olmo_leaves",
            // Folíolos angostos y resinosos, verde amarillento.
            Self::Jarilla => "eucalipto_leaves",
            // Hojas simples pequeñas a lo largo de una ramita, el atlas del
            // olmo, verde fresco.
            Self::Alamo => "olmo_leaves",
            // Hojas bipinnadas finas y colgantes: el atlas plumoso del aromo,
            // en un verde fresco.
            Self::Algarrobo => "aromo_leaves",
            // Hojas simples pequeñas y aserradas: el atlas del olmo, verde
            // medio.
            Self::Tala => "olmo_leaves",
            // Hojas bipinnadas diminutas: el atlas del aromo, en un verde
            // oscuro apagado.
            Self::Espinillo => "aromo_leaves",
            // Hojas simples grandes y brillantes: el atlas del olmo dibujado en
            // tarjetas grandes, verde profundo.
            Self::Ombu => "olmo_leaves",
        }
    }

    /// El tinte `tint` de las hojas sobre el atlas (sRGB). El tinte 0 es el
    /// propio de la especie: el escaneo amarillo del paraíso oscurecido, el
    /// eucalipto agrisado, el aromo plateado; las especies de campo tienen dos
    /// más ([`Species::tints`]), uno más fresco y uno gris más azulado (el del
    /// chañar, uno más amarillo).
    fn leaf_tint(self, tint: usize) -> Color {
        match (self, tint) {
            (Self::Calden, 1) => Color::srgb(0.76, 0.82, 0.58),
            (Self::Calden, 2) => Color::srgb(0.56, 0.66, 0.58),
            (Self::Chanar, 1) => Color::srgb(0.68, 0.84, 0.52),
            (Self::Chanar, 2) => Color::srgb(0.92, 0.9, 0.58),
            (Self::Fachinal, 1) => Color::srgb(0.68, 0.84, 0.52),
            (Self::Fachinal, 2) => Color::srgb(0.92, 0.9, 0.58),
            (Self::Piquillin, 1) => Color::srgb(0.36, 0.5, 0.3),
            (Self::Piquillin, 2) => Color::srgb(0.46, 0.58, 0.34),
            (Self::Jarilla, 1) => Color::srgb(0.9, 0.9, 0.4),
            (Self::Jarilla, 2) => Color::srgb(0.7, 0.82, 0.42),
            (Self::Algarrobo, 1) => Color::srgb(0.7, 0.84, 0.46),
            (Self::Algarrobo, 2) => Color::srgb(0.54, 0.7, 0.5),
            (Self::Tala, 1) => Color::srgb(0.6, 0.76, 0.4),
            (Self::Tala, 2) => Color::srgb(0.42, 0.6, 0.46),
            (Self::Espinillo, 1) => Color::srgb(0.5, 0.62, 0.34),
            (Self::Espinillo, 2) => Color::srgb(0.36, 0.5, 0.4),
            (Self::Ombu, 1) => Color::srgb(0.34, 0.54, 0.32),
            (Self::Ombu, 2) => Color::srgb(0.46, 0.68, 0.34),
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
            Self::Chanar | Self::Fachinal => Color::srgb(0.8, 0.88, 0.6),
            Self::Piquillin => Color::srgb(0.28, 0.42, 0.26),
            Self::Jarilla => Color::srgb(0.82, 0.88, 0.45),
            Self::Alamo => Color::srgb(0.72, 0.9, 0.55),
            Self::Algarrobo => Color::srgb(0.62, 0.78, 0.5),
            Self::Tala => Color::srgb(0.5, 0.68, 0.42),
            Self::Espinillo => Color::srgb(0.4, 0.56, 0.36),
            Self::Ombu => Color::srgb(0.4, 0.62, 0.36),
        }
    }

    /// El color liso de las hojas, sin su atlas.
    fn leaf_plain(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.32, 0.40, 0.18),
            Self::Olmo => Color::srgb(0.27, 0.34, 0.17),
            Self::Eucalipto => Color::srgb(0.36, 0.42, 0.33),
            Self::Aromo => Color::srgb(0.30, 0.38, 0.27),
            Self::Calden => Color::srgb(0.28, 0.33, 0.22),
            Self::Chanar | Self::Fachinal => Color::srgb(0.38, 0.44, 0.24),
            Self::Piquillin => Color::srgb(0.16, 0.26, 0.14),
            Self::Jarilla => Color::srgb(0.46, 0.5, 0.22),
            Self::Alamo => Color::srgb(0.3, 0.4, 0.17),
            Self::Algarrobo => Color::srgb(0.3, 0.4, 0.2),
            Self::Tala => Color::srgb(0.26, 0.36, 0.18),
            Self::Espinillo => Color::srgb(0.2, 0.3, 0.15),
            Self::Ombu => Color::srgb(0.2, 0.32, 0.14),
        }
    }

    /// El color liso de la corteza, sin sus mapas.
    fn bark_plain(self) -> Color {
        match self {
            Self::Paraiso => Color::srgb(0.30, 0.25, 0.20),
            Self::Olmo => Color::srgb(0.33, 0.29, 0.24),
            Self::Eucalipto => Color::srgb(0.55, 0.50, 0.43),
            Self::Aromo => Color::srgb(0.28, 0.22, 0.17),
            Self::Calden => Color::srgb(0.17, 0.14, 0.11),
            Self::Chanar | Self::Fachinal => Color::srgb(0.48, 0.44, 0.27),
            Self::Piquillin => Color::srgb(0.3, 0.26, 0.2),
            Self::Jarilla => Color::srgb(0.24, 0.19, 0.14),
            Self::Alamo => Color::srgb(0.32, 0.3, 0.27),
            Self::Algarrobo => Color::srgb(0.35, 0.25, 0.18),
            Self::Tala => Color::srgb(0.4, 0.36, 0.3),
            Self::Espinillo => Color::srgb(0.2, 0.15, 0.1),
            Self::Ombu => Color::srgb(0.5, 0.46, 0.38),
        }
    }

    /// El tinte `tint` de la corteza sobre sus mapas (sRGB): el del caldén, un
    /// gris pardo apagado sobre su conjunto ya oscuro (sus otros dos, más
    /// oscuro y más pálido), y las tiras del chañar vueltas ocre verdoso (sus
    /// otros dos, más gris y más pálido).
    pub(crate) fn bark_tint(self, tint: usize) -> Color {
        match (self, tint) {
            (Self::Calden, 1) => Color::srgb(0.7, 0.68, 0.66),
            (Self::Calden, 2) => Color::srgb(1.0, 0.96, 0.9),
            (Self::Chanar, 1) => Color::srgb(0.78, 0.82, 0.6),
            (Self::Chanar, 2) => Color::srgb(1.0, 0.98, 0.78),
            (Self::Calden, _) => Color::srgb(0.84, 0.82, 0.8),
            (Self::Chanar, _) => Color::srgb(0.9, 0.95, 0.68),
            (Self::Fachinal, 1) => Color::srgb(0.78, 0.82, 0.6),
            (Self::Fachinal, 2) => Color::srgb(1.0, 0.98, 0.78),
            (Self::Fachinal, _) => Color::srgb(0.9, 0.95, 0.68),
            // Los tallos de los arbustos se ven poco: un gris pardo, más
            // oscuro, más pálido.
            (Self::Piquillin, 1) => Color::srgb(0.7, 0.66, 0.6),
            (Self::Piquillin, 2) => Color::srgb(1.0, 0.95, 0.88),
            (Self::Piquillin, _) => Color::srgb(0.85, 0.8, 0.74),
            (Self::Jarilla, 1) => Color::srgb(0.75, 0.7, 0.62),
            (Self::Jarilla, 2) => Color::srgb(1.0, 0.95, 0.85),
            (Self::Jarilla, _) => Color::srgb(0.88, 0.84, 0.76),
            // Un gris más frío que el marrón cálido del olmo, con la misma
            // textura.
            (Self::Alamo, _) => Color::srgb(0.78, 0.8, 0.82),
            // Los conjuntos de los nuevos árboles de campo se matizan un poco,
            // con un tinte más oscuro y uno más pálido junto al primero.
            (Self::Algarrobo, 1) => Color::srgb(0.78, 0.7, 0.62),
            (Self::Algarrobo, 2) => Color::srgb(1.0, 0.92, 0.84),
            (Self::Algarrobo, _) => Color::srgb(0.9, 0.82, 0.74),
            (Self::Tala, 1) => Color::srgb(0.8, 0.78, 0.72),
            (Self::Tala, 2) => Color::srgb(1.0, 0.98, 0.92),
            (Self::Tala, _) => Color::srgb(0.95, 0.92, 0.86),
            (Self::Espinillo, 1) => Color::srgb(0.65, 0.58, 0.52),
            (Self::Espinillo, 2) => Color::srgb(0.95, 0.86, 0.76),
            (Self::Espinillo, _) => Color::srgb(0.8, 0.7, 0.62),
            (Self::Ombu, 1) => Color::srgb(0.8, 0.8, 0.74),
            (Self::Ombu, 2) => Color::srgb(1.0, 0.96, 0.88),
            (Self::Ombu, _) => Color::srgb(0.9, 0.88, 0.8),
            _ => Color::WHITE,
        }
    }
}

/// Los mapas en carga de un conjunto: color, normal y ORM.
#[derive(Clone)]
pub(crate) struct Maps {
    pub color: Handle<Image>,
    pub normal: Handle<Image>,
    pub orm: Handle<Image>,
}

/// El sampler limitado de los atlas de hojas: el borde de una tarjeta nunca da
/// la vuelta hasta el lado opuesto del atlas.
pub(crate) fn clamped() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..repeating()
    }
}

/// Empieza a cargar el conjunto de árbol `set` con `sampler`, o advierte y
/// devuelve `None` cuando falta uno de sus archivos.
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

/// Los materiales de corteza y de hojas de una especie, un tinte cada uno
/// ([`Species::tints`]).
pub(crate) struct Paint {
    pub bark: Vec<Handle<StandardMaterial>>,
    pub leaves: Vec<Handle<StandardMaterial>>,
}

/// Los materiales de corteza y de hojas de todas las especies, en el orden de
/// [`Species::EVERY`], cargando sus mapas cuando hay un servidor de assets, una
/// raíz de assets y un formato de textura comprimido.
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

/// Pone `maps` sobre `material`, con la oclusión y la rugosidad del único mapa
/// ORM.
fn wear(material: &mut StandardMaterial, maps: Maps) {
    material.base_color_texture = Some(maps.color);
    material.normal_map_texture = Some(maps.normal);
    material.metallic_roughness_texture = Some(maps.orm.clone());
    material.occlusion_texture = Some(maps.orm);
}

/// La corteza de `species`, en el tinte `tint`.
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

/// Las tarjetas de hojas de `species` en el tinte `tint`: con alpha test,
/// dibujadas por ambos lados.
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
        // Sin `double_sided`: eso invertiría las normales del dorso hacia la
        // copa, oscureciendo cada tarjeta vista desde atrás.
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
        // Cada una tiene un conjunto propio: el del caldén es la corteza oscura
        // de sauce, matizada en gris pardo; el del chañar, la corteza de
        // plátano que se descascara, teñida de ocre verdoso.
        assert_eq!(calden.bark_set(), "bark_willow");
        assert_eq!(chanar.bark_set(), "bark_platanus");
        let c = calden.bark_tint(0).to_srgba();
        let ch = chanar.bark_tint(0).to_srgba();
        assert!(c.red.max(c.green).max(c.blue) < 0.9, "{c:?}");
        assert!(c.red - c.blue < 0.1, "dull, not coloured: {c:?}");
        assert!(ch.green > ch.blue + 0.2 && ch.red > ch.blue + 0.2, "{ch:?}");
        // La corteza de las demás especies queda tal como se fotografió.
        assert_eq!(Species::Olmo.bark_tint(0), Color::WHITE);
        // La corteza texturizada lleva el tinte; la corteza lisa, su propio
        // color.
        let textured = bark_material(chanar, 0, Some(maps()));
        assert_eq!(textured.base_color, chanar.bark_tint(0));
        // Hojas finas y apagadas: el verde grisáceo del caldén queda más oscuro
        // que el plateado del aromo.
        let (cl, al) = (
            calden.leaf_tint(0).to_srgba(),
            Species::Aromo.leaf_tint(0).to_srgba(),
        );
        assert!(cl.red < al.red && cl.green < al.green);
    }

    #[test]
    fn the_atlas_mips_keep_coverage_for_this_cutoff() {
        // El alfa de byte más bajo que se dibuja es aquel por encima del cual
        // `leaf_atlas.py` conserva la proporción de texels de cada mip.
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
        // Sin servidor de assets: no carga nada, y cada especie igual tiene
        // ambos.
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
        // Cada especie tiene un conjunto de corteza propio, así que los tintes
        // solo matizan la textura real y nunca imitan una corteza distinta.
        // El fachinal son los retoños de un chañar: lleva la corteza del
        // chañar.
        assert_eq!(Species::Fachinal.bark_set(), Species::Chanar.bark_set());
        // El álamo toma prestados los surcos grises del olmo (todos los demás
        // conjuntos incluidos son de otra especie), matizados en un gris más
        // frío para que los dos no parezcan un mismo árbol.
        assert_eq!(Species::Alamo.bark_set(), Species::Olmo.bark_set());
        assert_ne!(Species::Alamo.bark_tint(0), Species::Olmo.bark_tint(0));
        let sets: Vec<&str> = Species::EVERY
            .iter()
            .filter(|&&s| s != Species::Fachinal && s != Species::Alamo)
            .map(|s| s.bark_set())
            .collect();
        for (i, a) in sets.iter().enumerate() {
            for b in &sets[i + 1..] {
                assert_ne!(a, b, "two species share a bark set");
            }
        }
    }

    #[test]
    fn each_new_field_species_wears_its_own_bark_set_and_a_leaf_atlas_that_exists() {
        let want = [
            (Species::Algarrobo, "chinese_cedar_bark", "aromo_leaves"),
            (Species::Tala, "chinese_hackberry_bark", "olmo_leaves"),
            (Species::Espinillo, "bark_brown_01", "aromo_leaves"),
            (Species::Ombu, "Bark005", "olmo_leaves"),
        ];
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for (species, bark, leaves) in want {
            assert_eq!(species.bark_set(), bark, "{species:?}");
            assert_eq!(species.leaf_set(), leaves, "{species:?}");
            // Ninguna otra especie lleva esa corteza.
            let wearers = Species::EVERY
                .iter()
                .filter(|s| s.bark_set() == bark)
                .count();
            assert_eq!(wearers, 1, "{bark}");
            for map in ["color", "normal_gl", "orm"] {
                let file = root.join(tree_map_path(bark, map));
                assert!(file.is_file(), "{}", file.display());
            }
        }
    }

    #[test]
    fn the_field_species_have_tints_that_differ_and_share_their_maps() {
        for species in Species::EVERY {
            let want = if Species::FIELD.contains(&species) || Species::SHRUBS.contains(&species) {
                super::super::FIELD_TINTS
            } else {
                1
            };
            assert_eq!(species.tints(), want, "{species:?}");
        }
        for species in Species::FIELD.into_iter().chain(Species::SHRUBS) {
            let (bark, leaf): (Vec<_>, Vec<_>) = (0..species.tints())
                .map(|t| (species.bark_tint(t), species.leaf_tint(t)))
                .unzip();
            // Tres tintes distintos de cada una, ninguno lejos del propio de la
            // especie.
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
            // Cada tinte es un material propio sobre los mismos mapas.
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
        // El primer tinte es el propio del kit del pueblo, y los árboles de los
        // pueblos tienen uno solo.
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
