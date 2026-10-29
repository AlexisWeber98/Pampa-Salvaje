//! Los árboles de los pueblos: paraísos, olmos, eucaliptos y aromos
//! procedurales, los árboles de las plazas, calles y patios pampeanos de 1919,
//! y el caldén, el chañar, el algarrobo, el tala, el espinillo y el ombú del
//! campo abierto ([`Species::FIELD`]; las especies de pueblo también crecen en
//! el campo).
//!
//! Cada [`Species`] tiene [`VARIANTS`] árboles con semilla (las especies de
//! campo, [`CALDEN_VARIANTS`] y [`CHANAR_VARIANTS`] formas distintas, para que
//! los campos no se repitan) ([`mesh`]): troncos y ramas de corteza ahusados
//! que llevan tarjetas de hojas probadas por alfa ([`materials`]). Se
//! construyen una sola vez al inicio en el [`TreeKit`] compartido, dos mallas
//! por variante y dos materiales por especie (las especies de campo, unos pocos
//! tintes de cada uno, [`FIELD_TINTS`]), así que los árboles de un pueblo no
//! cuestan ninguna malla ni material nuevos: cada árbol de plaza es la corteza
//! y las hojas de una variante, giradas y escaladas. Core ubica los troncos
//! ([`pampa_core::TownLayout::trees`]); la especie de cada plaza se elige aquí
//! a partir de la semilla del pueblo ([`plaza_species`]), así que la
//! disposición no cambia. Un árbol de calle es una variante podada
//! ([`mesh::street_tree`]: un tronco desnudo hasta unos 2.5 m bajo una copa
//! llena y angosta) de las [`STREET_SPECIES`]; los arbustos y canteros de
//! flores de los patios son las [`plants`].

pub mod materials;
pub(crate) mod mesh;
pub mod plants;

use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;

use crate::AssetRoot;

/// Variantes con semilla construidas por especie.
pub const VARIANTS: usize = 3;

/// Formas distintas construidas del caldén: [`CALDEN_LIVE`] vivas, y luego tres
/// árboles secos.
pub const CALDEN_VARIANTS: usize = 12;

/// Formas distintas construidas del chañar.
pub const CHANAR_VARIANTS: usize = 6;

/// Formas distintas construidas del algarrobo, el tala y el espinillo.
pub const ALGARROBO_VARIANTS: usize = 5;
pub const TALA_VARIANTS: usize = 5;
pub const ESPINILLO_VARIANTS: usize = 5;

/// Formas distintas construidas del ombú (un árbol raro, unos pocos).
pub const OMBU_VARIANTS: usize = 4;

/// Cuántas de las variantes del caldén son árboles vivos; el resto son árboles
/// secos, muertos o ralos ([`mesh::tree`]).
pub const CALDEN_LIVE: usize = 9;

/// Formas distintas construidas de cada arbusto: tres formas (simple, bajo y
/// extendido, alto y denso), dos semillas de cada una.
pub const SHRUB_VARIANTS: usize = 6;

/// Tintes de hojas y de corteza que tiene una especie de campo (materiales
/// compartidos, así que los árboles de cada tinte igual se agrupan por variante
/// y tinte).
pub const FIELD_TINTS: usize = 3;

/// La especie de un árbol: las cuatro de los pueblos, las seis de los campos,
/// los tres arbustos de los campos y el álamo de las estancias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Species {
    /// Paraíso (_Melia azedarach_): una copa redonda en forma de sombrilla de
    /// hojas pinnadas sobre un tronco corto.
    Paraiso,
    /// Olmo: un vaso alto de ramas.
    Olmo,
    /// Eucalipto: un tronco alto y recto, una copa abierta de hojas colgantes.
    Eucalipto,
    /// Aromo: bajo y plumoso.
    Aromo,
    /// Caldén (_Neltuma caldenia_), el árbol del campo abierto: un tronco corto
    /// y grueso que se bifurca abajo, bajo una sombrilla ancha y plana de hojas
    /// pequeñas verde grisáceas; sus últimas variantes están peladas o ralas.
    Calden,
    /// Chañar (_Geoffroea decorticans_): pequeño, de varios tallos y ramoso,
    /// con una copa rala e irregular.
    Chanar,
    /// Piquillín (_Condalia microphylla_), un arbusto del monte: una masa densa
    /// y espinosa de tallos finos bajo hojas diminutas y oscuras.
    Piquillin,
    /// Jarilla (_Larrea divaricata_), un arbusto del suelo abierto y seco:
    /// muchos tallos erguidos, hojas resinosas verde amarillentas.
    Jarilla,
    /// Fachinal: una maraña de renuevos de chañar (con la propia corteza y las
    /// hojas del chañar).
    Fachinal,
    /// Álamo (álamo de Lombardía, _Populus nigra_ 'Italica'), las hileras
    /// cortavientos de las estancias: una columna alta y angosta de ramas
    /// levantadas.
    Alamo,
    /// Algarrobo (_Prosopis flexuosa_), el compañero del caldén en el monte:
    /// bajo, ancho y a menudo de varios troncos.
    Algarrobo,
    /// Tala (_Celtis tala_): ramas en zigzag bajo una copa redonda irregular.
    Tala,
    /// Espinillo (_Vachellia caven_): un árbol pequeño de copa redondeada.
    Espinillo,
    /// Ombú (_Phytolacca dioica_): un tronco enorme con contrafuertes bajo una
    /// cúpula ancha y densa.
    Ombu,
}

impl Species {
    /// Las especies de los pueblos, en orden: lo que se planta en plazas,
    /// calles y patios.
    pub const ALL: [Species; 4] = [Self::Paraiso, Self::Olmo, Self::Eucalipto, Self::Aromo];
    /// Las especies propias de los campos, en orden (las especies de pueblo que
    /// se escaparon al campo son [`Self::ALL`]).
    pub const FIELD: [Species; 6] = [
        Self::Calden,
        Self::Chanar,
        Self::Algarrobo,
        Self::Tala,
        Self::Espinillo,
        Self::Ombu,
    ];
    /// Los arbustos de los campos, en orden.
    pub const SHRUBS: [Species; 3] = [Self::Piquillin, Self::Jarilla, Self::Fachinal];
    /// Todas las especies que construye el [`TreeKit`], en el orden de `Species
    /// as usize`.
    pub const EVERY: [Species; 14] = [
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
        Self::Algarrobo,
        Self::Tala,
        Self::Espinillo,
        Self::Ombu,
    ];

    /// Cuántas variantes con semilla de ella contiene el kit.
    pub const fn variants(self) -> usize {
        match self {
            Self::Calden => CALDEN_VARIANTS,
            Self::Chanar => CHANAR_VARIANTS,
            Self::Algarrobo => ALGARROBO_VARIANTS,
            Self::Tala => TALA_VARIANTS,
            Self::Espinillo => ESPINILLO_VARIANTS,
            Self::Ombu => OMBU_VARIANTS,
            Self::Piquillin | Self::Jarilla | Self::Fachinal => SHRUB_VARIANTS,
            _ => VARIANTS,
        }
    }

    /// Cuántos tintes de hojas, y de corteza, de ella contiene el kit.
    pub const fn tints(self) -> usize {
        match self {
            Self::Calden
            | Self::Chanar
            | Self::Algarrobo
            | Self::Tala
            | Self::Espinillo
            | Self::Ombu
            | Self::Piquillin
            | Self::Jarilla
            | Self::Fachinal => FIELD_TINTS,
            _ => 1,
        }
    }
}

/// Mezcla `seed` y `salt` mediante un hash en un número bien mezclado.
fn mix(seed: u32, salt: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// La especie plantada en la plaza `plaza` (0: la principal, 1: la de la
/// estación, ver [`pampa_core::TownLayout::plazas`]) del pueblo con semilla
/// `seed`. Las dos plazas de un pueblo nunca comparten especie.
pub fn plaza_species(seed: u32, plaza: usize) -> Species {
    let main = mix(seed, 0x7EE5) as usize % Species::ALL.len();
    let k = if plaza == 0 {
        main
    } else {
        main + 1 + (mix(seed, 0x57A7 + plaza as u32) as usize % (Species::ALL.len() - 1))
    };
    Species::ALL[k % Species::ALL.len()]
}

/// Las especies que se ven podadas a lo largo de las calles: aquellas cuyas
/// copas se pueden levantar hasta dejar un tronco desnudo de unos 4 m (las
/// variantes `pruned` del [`TreeKit`]). El aromo se queda bajo.
pub const STREET_SPECIES: [Species; 3] = [Species::Paraiso, Species::Olmo, Species::Eucalipto];

/// La especie de calle de los árboles en `key` (una franja de vereda, o un
/// lugar) del pueblo con semilla `seed`: una sola especie a lo largo de toda
/// una hilera.
pub fn street_species(seed: u32, key: u32) -> Species {
    STREET_SPECIES[mix(seed, 0x57EE ^ key) as usize % STREET_SPECIES.len()]
}

/// La especie del árbol de patio ubicado en el lugar del suelo `at` del pueblo
/// con semilla `seed`.
pub fn yard_species(seed: u32, at: pampa_core::Vec2) -> Species {
    let spot = ((at.x * 8.0) as i32 as u32).wrapping_mul(0x27D4_EB2F) ^ (at.y * 8.0) as i32 as u32;
    Species::ALL[mix(seed, 0x7A2D ^ spot) as usize % Species::ALL.len()]
}

/// Una variante compartida: sus mallas y qué tan grande es.
#[derive(Debug, Clone)]
pub struct TreeVariant {
    pub bark: Handle<Mesh>,
    pub leaves: Handle<Mesh>,
    /// Su punta sobre el suelo, en metros.
    pub height: f32,
    /// Hasta dónde llegan sus hojas desde el tronco, en metros.
    pub crown_radius: f32,
    /// Qué tan bajo cuelga su hoja más baja sobre el suelo, en metros.
    pub crown_base: f32,
}

/// Las variantes de una especie y sus materiales.
#[derive(Debug, Clone)]
pub struct SpeciesKit {
    pub variants: Vec<TreeVariant>,
    /// Las variantes podadas para una calle (una copa levantada y angosta):
    /// solo las tienen las [`STREET_SPECIES`].
    pub pruned: Vec<TreeVariant>,
    /// Los primeros materiales de corteza y de hojas: los únicos de las
    /// especies de pueblo.
    pub bark: Handle<StandardMaterial>,
    pub leaves: Handle<StandardMaterial>,
    /// Todos los tintes de corteza y de hojas ([`Species::tints`]); el primero
    /// de cada uno es [`Self::bark`] y [`Self::leaves`].
    pub bark_tints: Vec<Handle<StandardMaterial>>,
    pub leaf_tints: Vec<Handle<StandardMaterial>>,
}

/// Las mallas y materiales compartidos de los árboles, y los de las plantas de
/// patio, construidos al inicio.
#[derive(Resource, Debug, Clone)]
pub struct TreeKit {
    species: Vec<SpeciesKit>,
    pub plants: plants::PlantKit,
}

impl TreeKit {
    /// El kit de `species`.
    pub fn species(&self, species: Species) -> &SpeciesKit {
        &self.species[species as usize]
    }
}

/// Construye el [`TreeKit`] al inicio.
pub struct TreePlugin;

impl Plugin for TreePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_trees);
    }
}

/// Agrega el [`TreePlugin`] a `app`, salvo que otro plugin ya lo haya hecho.
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
        // Cada especie se planta en algún lugar.
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
                // Un árbol mide más de 2 m (las formas bajas del tala y del
                // espinillo); un arbusto es una masa baja de más de 1 m.
                let (tall, wide) = if Species::SHRUBS.contains(&species) {
                    (1.0, 0.4)
                } else {
                    (2.0, 1.0)
                };
                assert!(v.height > tall && v.crown_radius > wide, "{species:?}");
            }
            materials.extend(k.bark_tints.iter().chain(&k.leaf_tints).map(|m| m.id()));
        }
        // Las variantes de cada especie (entre ellas las del álamo), las
        // podadas de las especies de calle y las de las plantas.
        let built = 2 * VARIANTS * (Species::ALL.len() + STREET_SPECIES.len())
            + 2 * (CALDEN_VARIANTS + CHANAR_VARIANTS + 3 * SHRUB_VARIANTS + VARIANTS)
            + 2 * (ALGARROBO_VARIANTS + TALA_VARIANTS + ESPINILLO_VARIANTS + OMBU_VARIANTS)
            + 2 * plants::VARIANTS as usize;
        let plant_meshes = (kit.plants.shrubs.iter().chain(&kit.plants.flowers)).map(|h| h.id());
        meshes.extend(plant_meshes);
        assert_eq!(meshes.len(), built);
        // Los materiales de corteza y de hojas de los árboles, uno por tinte, y
        // el de las plantas.
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
                // Una copa baja y llena: hojas desde unos 3 m (ninguna por
                // debajo de 2.7 m, ninguna por encima de 4.2 m), bastante
                // follaje, la altura de un árbol de calle.
                assert!(
                    (2.0..4.2).contains(&trimmed.crown_base),
                    "{species:?} {v}: {}",
                    trimmed.crown_base
                );
                assert!(trimmed.leaves.positions.len() >= 700, "{species:?} {v}");
                // Más angosta que la copa del árbol libre, pero no una columna.
                assert!(
                    trimmed.crown_radius < 0.65 * free.crown_radius,
                    "{species:?} {v}"
                );
                assert!(trimmed.crown_radius > 2.2, "{species:?} {v}");
                assert!((5.0..8.0).contains(&trimmed.height), "{species:?} {v}");
            }
        }
        // El aromo no tiene: sigue siendo un árbol bajo de patio.
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
        // Otro pueblo planta sus hileras de otra manera.
        assert!((0..64).any(|k| street_species(7, k) != street_species(8, k)));
    }
}
