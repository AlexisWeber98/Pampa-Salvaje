//! Rapaces: el modelo low-poly animado de Quaternius (CC0; con crédito en
//! `assets/models/fauna/quaternius/CREDITS.md` y en el `CREDITS.md` de la
//! raíz): el águila del Animal Pack Vol.2, convertida con `tools/fbx2glb.py`
//! (Blender), diezmada y recortada con su planeo y su aleteo
//! ([`Species::animation`]); hace de chimango y de carancho, en sus tamaños y
//! colores.
//! El ñandú es un modelo propio, modelado, con rig y animado por script en
//! Blender (`tools/rhea.py`, en `assets/models/fauna/own/`): un cuerpo
//! continuo con su plumaje horneado de una escultura densa en mapas KTX2
//! (color, normal map y ORM, [`Skin`]), tarjetas de plumas sueltas con alpha
//! test que solo se dibujan cerca ([`DETAIL_FAR`]), y sus propios clips
//! (esperas, pastoreo, caminata y carrera). El caballo criollo (los de corral
//! y los ensillados) también es propio (`tools/horse.py`): su rig copia el de
//! Quaternius, con su piel vestida con mapas KTX2 que multiplican los colores
//! del pelaje y su crin, su copete y su cola en tarjetas con alpha test que se
//! dibujan a cualquier distancia. La vaca y el toro también son propios
//! (`tools/cattle.py`, el caballo con anatomía bovina): cada pelaje
//! (Hereford, Angus, overo y Shorthorn rosillo) viene horneado en un mapa de
//! color propio del cuerpo ([`Paint::Map`]), y además de su malla completa
//! traen una liviana para las tropas lejanas ([`light_range`]). El zorro gris
//! pampeano también es propio (`tools/fox.py`): su único pelaje viene horneado
//! en el mapa de color de su cuerpo y su pelo en el de sus tarjetas, así que
//! sus materiales se dibujan con sus mapas tal como están. Los tres felinos
//! (el gato montés, el puma y el yaguareté) son propios igual
//! (`tools/felid.py`, una anatomía y un rig de gato para las tres especies):
//! cada uno con su pelaje y la cara de una foto horneados en el mapa de su
//! cuerpo, y una malla liviana para cuando están lejos, como los vacunos.
//!
//! Cada [`Species`] se carga una vez al iniciar ([`LivestockModels`]): su
//! escena glTF, sus clips y un [`AnimationGraph`] compartido. Un [`Animal`]
//! de modelo recibe un hijo [`LivestockModel`] que crea la escena compartida
//! una vez que terminó de cargar. El hijo lleva la escala de la especie y gira
//! el modelo (que mira hacia +Z) hacia el rumbo del animal (+X es su frente).
//! Cuando la escena está lista:
//!
//! - sus meshes reciben el rango compartido de su animal ([`Animal::range`]:
//!   el de los accesorios de los asentamientos, el de los edificios para las
//!   rapaces que planean en lo alto); en un modelo con malla liviana, la
//!   completa se dibuja hasta [`LIGHT_FROM`] y la liviana desde ahí hasta el
//!   final del rango, fundidas con dithering ([`light_range`]);
//! - sus materiales se cambian por copias teñidas con el pelaje del animal,
//!   creadas una vez por pelaje y material y compartidas por todos los animales
//!   que lo llevan ([`paint`]); los pelajes de los caballos son colores por
//!   vertex en una copia de la mesh del cuerpo por pelaje, compartida de la
//!   misma forma;
//! - su reproductor de animación recibe el grafo de la especie.
//!
//! En cada frame [`clip_for`] elige el clip a partir del estado de la fauna
//! ([`AnimalPose`]), los clips de caminata se ajustan a la velocidad sobre el
//! suelo ([`clip_speed`]) y cada animal empieza sus clips en su propio desfase
//! con semilla. Un reproductor más lejos que [`FREEZE_FAR`] de la cámara, o
//! fuera de la vista, pierde su grafo: sus huesos conservan su última pose y
//! los sistemas de animación lo saltean. Los vacunos nunca se echan: la
//! simulación no tiene estado de echado (toda pose echada que se probó se veía
//! poco natural en este rig).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::animation::{graph::AnimationNodeIndex, transition::AnimationTransitions};
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{GltfAssetLabel, GltfMaterialName, GltfMeshName};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::fauna::{CritterKind, Leap};
use pampa_core::{CowCoat, Heading};

use super::{Animal, AnimalPose, AnimalSeed, Gait};

use crate::AssetRoot;
use crate::facade::materials::{MAPS, load_map_with};
use crate::gltf::{self, ModelState};
use crate::tree::materials::{LEAF_CUTOFF, clamped};
use bevy::image::CompressedImageFormatSupport;

/// Dónde viven los modelos de animales de Quaternius, bajo el [`AssetRoot`].
pub const LIVESTOCK_DIR: &str = "models/fauna/quaternius";
/// Dónde viven los modelos propios, hechos con los scripts de Blender de
/// `tools/` (el ñandú, `tools/rhea.py`; el caballo, `tools/horse.py`; la vaca
/// y el toro, `tools/cattle.py`; el zorro, `tools/fox.py`; los felinos,
/// `tools/felid.py`), bajo el [`AssetRoot`].
pub const OWN_DIR: &str = "models/fauna/own";

/// Hasta esta distancia de la cámara se dibujan las tarjetas de detalle de un
/// modelo que las tiene (las plumas sueltas del ñandú), en metros; más lejos solo queda el cuerpo,
/// cuyo plumaje horneado ya muestra lo que se ve a esa distancia. Vuelven
/// dentro de [`DETAIL_NEAR`].
pub const DETAIL_FAR: f32 = 40.0;
/// Dentro de esta distancia vuelven las tarjetas, en metros: un poco más
/// cerca que [`DETAIL_FAR`], para que no parpadeen en el borde.
pub const DETAIL_NEAR: f32 = 35.0;

/// Si se dibujan las tarjetas de plumas de un animal a `distance` de la
/// cámara, sabiendo si se dibujaban (`was`).
pub fn detail_shown(was: bool, distance: f32) -> bool {
    distance < if was { DETAIL_FAR } else { DETAIL_NEAR }
}

/// El sufijo del nombre de la malla liviana de un modelo (`Cow_LOD`, de
/// `tools/cattle.py`; `Puma_LOD` y los de los otros felinos, de
/// `tools/felid.py`): la piel sola, diezmada, sin tarjetas de pelo ni ojos.
pub const LIGHT_SUFFIX: &str = "_LOD";
/// Desde esta distancia de la cámara un modelo con malla liviana dibuja la
/// liviana en lugar de la completa, en metros (una vaca de 1.4 m mide ahí
/// unos 35 píxeles de alto en 1080p, y su pelo ya no se distingue).
pub const LIGHT_FROM: f32 = 40.0;
/// Lo que tardan en fundirse las dos mallas alrededor de [`LIGHT_FROM`], en
/// metros: la completa se desvanece con dithering mientras aparece la liviana.
pub const LIGHT_FADE: f32 = 4.0;

/// El rango de una malla de un modelo que tiene malla liviana, a partir del
/// `range` de su animal: la completa (`light` falso) hasta [`LIGHT_FROM`], la
/// liviana desde ahí hasta donde termina `range`.
pub fn light_range(range: &VisibilityRange, light: bool) -> VisibilityRange {
    let swap = LIGHT_FROM - LIGHT_FADE..LIGHT_FROM;
    if light {
        VisibilityRange {
            start_margin: swap,
            ..range.clone()
        }
    } else {
        VisibilityRange {
            end_margin: swap,
            ..range.clone()
        }
    }
}

/// Los mapas KTX2 que viste un material de un modelo propio: un conjunto de
/// [`OWN_DIR`] hecho por `tools/textures_to_ktx2.sh` como los de los edificios
/// (color, normal map y ORM). `masked`: tarjetas con alpha test, con la
/// opacidad en el alfa del color (con los mips que conservan la cobertura,
/// como las hojas de los árboles). `detail`: tarjetas que solo se dibujan
/// cerca ([`DETAIL_FAR`]), como las plumas sueltas del ñandú; la crin y la
/// cola del caballo, en cambio, son su silueta y se dibujan siempre.
/// `strands`: el dorso de cada tarjeta se ilumina con la normal invertida,
/// como un mechón de pelo visto a través (la crin del caballo, que cae del
/// lado derecho, vista desde la izquierda); sin eso, las dos caras miran hacia
/// afuera del cuerpo (las plumas del ñandú).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Skin {
    pub set: &'static str,
    pub masked: bool,
    pub detail: bool,
    pub strands: bool,
}

/// Los conjuntos de mapas de los modelos propios.
pub const SKIN_SETS: [&str; 15] = [
    "rhea_body",
    "rhea_feathers",
    "horse_body",
    "horse_hair",
    "cow_body",
    "bull_body",
    "cattle_hair",
    "fox_body",
    "fox_hair",
    "gato_body",
    "gato_hair",
    "puma_body",
    "puma_hair",
    "yagua_body",
    "yagua_hair",
];

/// La ruta del mapa `name` del conjunto `set`, bajo el [`AssetRoot`].
pub fn skin_path(set: &str, name: &str) -> String {
    format!("{OWN_DIR}/{set}/{name}.ktx2")
}

/// El mapa de color de un pelaje vacuno, dentro del conjunto del cuerpo de la
/// vaca o del toro: `cow_body/hereford.ktx2` y los demás. `tools/cattle.py`
/// hornea en él el pelaje entero (los bordes del blanco del Hereford, los
/// pelos del rosillo, las manchas del overo y la ubre lampiña) por la
/// luminancia neutra del conjunto.
pub fn coat_map_name(coat: CowCoat) -> &'static str {
    match coat {
        CowCoat::Hereford => "hereford",
        CowCoat::Angus => "angus",
        CowCoat::Overo => "overo",
        CowCoat::Roan => "roan",
    }
}

/// Los mapas en carga de un conjunto.
#[derive(Debug, Clone)]
pub struct SkinMaps {
    pub color: Handle<Image>,
    pub normal: Handle<Image>,
    pub orm: Handle<Image>,
}

/// `base` vestido con `skin`: sus mapas sobre blanco, con alpha test y por
/// ambos lados si son tarjetas. Sin los mapas, el cuerpo conserva su color
/// liso y las tarjetas no se dibujan (`None`).
pub fn skinned(
    base: &StandardMaterial,
    skin: Skin,
    maps: Option<SkinMaps>,
) -> Option<StandardMaterial> {
    let Some(maps) = maps else {
        return (!skin.masked).then(|| base.clone());
    };
    let mut m = base.clone();
    m.base_color = Color::WHITE;
    m.base_color_texture = Some(maps.color);
    m.normal_map_texture = Some(maps.normal);
    m.metallic_roughness_texture = Some(maps.orm.clone());
    m.occlusion_texture = Some(maps.orm);
    m.metallic = 0.0;
    m.perceptual_roughness = 1.0;
    m.reflectance = 0.3;
    if skin.masked {
        m.alpha_mode = AlphaMode::Mask(LEAF_CUTOFF);
        // Por ambos lados; el dorso invierte su normal solo en los mechones
        // (en las plumas, como en las hojas de los árboles, las dos caras
        // miran hacia afuera del cuerpo).
        m.cull_mode = None;
        m.double_sided = skin.strands;
    }
    Some(m)
}

/// Más allá de esta distancia de la cámara la animación de un animal se congela
/// en su pose actual, en metros; vuelve a reproducirse una vez que regresa
/// dentro de [`RESUME_NEAR`].
pub const FREEZE_FAR: f32 = 120.0;
/// Dentro de esta distancia un animal congelado vuelve a reproducirse, en
/// metros: un poco más cerca que [`FREEZE_FAR`], para que uno en el borde no
/// parpadee.
pub const RESUME_NEAR: f32 = 110.0;

/// Por encima de esta velocidad sobre el suelo un animal que camina galopa, en
/// m/s.
pub const GALLOP_SPEED: f32 = 2.5;

/// Fundido entre dos clips.
const BLEND: Duration = Duration::from_millis(300);
/// Fundido hacia el salto: más corto, porque el vuelo dura alrededor de un
/// segundo ([`Leap::flight`]).
const LEAP_BLEND: Duration = Duration::from_millis(120);

/// Cuánto dura el clip `Gallop_Jump` del zorro y de los felinos (un brinco
/// sobre una presa, `tools/fox.py` y `tools/felid.py`, `POUNCE_SECONDS`), en
/// segundos.
pub const POUNCE_SECONDS: f32 = 64.0 / 48.0;
/// Cuánto dura el clip `Fence_Leap` del zorro y de los felinos (el salto de
/// un alambrado, `tools/fox.py` y `tools/felid.py`, `FENCE_LEAP_FRAMES`), en
/// segundos: 84 cuadros a 48 por segundo.
pub const FENCE_LEAP_SECONDS: f32 = 84.0 / 48.0;
/// Las fracciones del clip `Fence_Leap` entre las que salta: desde el
/// impulso de las patas (el cuadro 10) hasta que las manos tocan el suelo (el
/// 58), un segundo justo. Un salto en el juego empieza en la primera y, a
/// [`clip_speed`], llega a la segunda al caer. Lo bajo que pasa el cuerpo
/// sobre el alambre en este tramo es lo que resume [`Leap::tuck`]: si cambia
/// el clip, se vuelve a medir.
pub const LEAP_CLIP: (f32, f32) = (10.0 / 84.0, 58.0 / 84.0);

/// La velocidad sobre el suelo que muestra el clip `Walk` del caballo a tamaño
/// real, en m/s: el deslizamiento medio hacia atrás de la punta de cada vaso
/// (`FF.*_end`, `FFB.*_end`) mientras está a menos de 1.5 cm de su punto más
/// bajo, a lo largo de todo el ciclo (1.17 s), promediado entre los cuatro: los
/// delanteros a 1.24 m/s y los traseros a 1.23 (el caballo propio,
/// `tools/horse.py`; el de Quaternius mostraba 1.09 por este método).
pub const HORSE_WALK_SPEED: f32 = 1.23;
/// Lo mismo para su clip `Gallop` (0.58 s, unos 4.15 m por zancada): los vasos
/// delanteros a unos 7.14 m/s y los traseros a unos 7.06 (el de Quaternius,
/// 3.94).
pub const HORSE_GALLOP_SPEED: f32 = 7.10;

/// La velocidad sobre el suelo que muestra el clip `Walk` de la vaca y del
/// toro propios (`tools/cattle.py`), en metros del modelo por segundo, medida
/// como la del caballo: el deslizamiento medio hacia atrás de los huesos IK
/// de las pezuñas mientras están a menos de 1.5 cm de su punto más bajo, a lo
/// largo del ciclo (1.04 s). El juego la escala con la especie
/// ([`Species::scale`]).
pub const CATTLE_WALK_SPEED: f32 = 1.04;
/// Lo mismo para su clip `Gallop` (0.44 s).
pub const CATTLE_GALLOP_SPEED: f32 = 5.62;

/// La altura a la cruz de la vaca propia, en metros del modelo: lo más alto
/// del lomo sobre las paletas (a 0.5–0.7 m por delante del origen, a menos de
/// 8 cm de la línea media) en la mesh exportada, con las pezuñas en el origen.
pub const COW_WITHERS: f32 = 1.33;
/// Lo mismo en el toro propio, medido igual: más alto en la cruz por la giba
/// de músculo sobre el cuello.
pub const BULL_WITHERS: f32 = 1.40;

/// La velocidad sobre el suelo que muestra el clip `Walk` del zorro propio
/// (`tools/fox.py`), en metros del modelo por segundo, medida como la de los
/// vacunos (el deslizamiento medio hacia atrás de los huesos IK de las manos y
/// las patas mientras están a menos de 1.5 cm de su punto más bajo, a lo largo
/// del ciclo de 0.58 s). El juego la escala con la especie ([`Species::scale`]).
pub const FOX_WALK_SPEED: f32 = 0.54;
/// Lo mismo para su clip `Gallop`, un galope rotatorio (0.31 s).
pub const FOX_GALLOP_SPEED: f32 = 4.21;
/// La altura a la cruz del zorro propio, en metros del modelo, medida como la
/// de la vaca sobre la mesh exportada.
pub const FOX_WITHERS: f32 = 0.38;

/// La velocidad sobre el suelo que muestra el clip `Walk` del gato montés
/// propio (`tools/felid.py`), en metros del modelo por segundo, medida como la
/// del caballo sobre el modelo exportado: el deslizamiento medio hacia atrás
/// de las puntas de las manos y las patas (`FF.*_end`, `FFB.*_end`) mientras
/// están cerca de su punto más bajo (1.5 cm en el puma, escalados al tamaño de
/// cada felino: 0.63 cm en el gato montés), a lo largo del ciclo. El juego la
/// escala con la especie ([`Species::scale`]).
pub const GATO_WALK_SPEED: f32 = 0.53;
/// Lo mismo para su clip `Gallop`, un galope rotatorio.
pub const GATO_GALLOP_SPEED: f32 = 5.36;
/// La altura a la cruz del gato montés propio, en metros del modelo, medida
/// como la de la vaca sobre la mesh exportada.
pub const GATO_WITHERS: f32 = 0.28;
/// Lo más rápido que se reproduce el galope del gato montés respecto de cómo
/// se creó ([`Species::max_pace`]): la frecuencia del tranco crece al achicarse
/// el cuerpo, así que un gato de 4 kg da muchos más trancos por segundo que un
/// puma. Con este tope su clip alcanza
/// su velocidad de huida (45 km/h) sin que las manos se deslicen.
pub const GATO_GALLOP_RATE: f32 = 2.4;
/// La velocidad del clip `Walk` del puma propio, medida como la del gato
/// montés ([`GATO_WALK_SPEED`]).
pub const PUMA_WALK_SPEED: f32 = 0.80;
/// Lo mismo para su clip `Gallop`.
pub const PUMA_GALLOP_SPEED: f32 = 11.05;
/// La altura a la cruz del puma propio, medida como la del gato montés.
pub const PUMA_WITHERS: f32 = 0.68;
/// La velocidad del clip `Walk` del yaguareté propio, medida como la del gato
/// montés ([`GATO_WALK_SPEED`]).
pub const YAGUA_WALK_SPEED: f32 = 0.73;
/// Lo mismo para su clip `Gallop`.
pub const YAGUA_GALLOP_SPEED: f32 = 10.80;
/// La altura a la cruz del yaguareté propio, medida como la del gato montés.
pub const YAGUA_WITHERS: f32 = 0.75;

/// Los animales que se dibujan a partir de un modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Species {
    Cow,
    Bull,
    Horse,
    /// El zorro gris pampeano: un modelo propio (`tools/fox.py`), vestido con
    /// sus mapas ([`Species::skin`]).
    Fox,
    /// Los tres felinos: modelos propios (`tools/felid.py`, una anatomía de
    /// gato llevada a cada especie), vestidos con sus mapas
    /// ([`Species::skin`]).
    GatoMontes,
    Puma,
    Yaguarete,
    /// Las dos rapaces comparten el águila del mismo pack, teñida.
    Chimango,
    Carancho,
    /// El ñandú: un modelo propio (`tools/rhea.py`), vestido con sus mapas
    /// ([`Species::skin`]).
    Nandu,
}

impl Species {
    pub const ALL: [Self; 10] = [
        Self::Cow,
        Self::Bull,
        Self::Horse,
        Self::Fox,
        Self::GatoMontes,
        Self::Puma,
        Self::Yaguarete,
        Self::Chimango,
        Self::Carancho,
        Self::Nandu,
    ];

    /// La especie con la que se dibuja `animal`, si se dibuja a partir de un
    /// modelo.
    pub fn of(animal: Animal) -> Option<Self> {
        match animal {
            Animal::Cow(_) => Some(Self::Cow),
            Animal::Bull(_) => Some(Self::Bull),
            Animal::Horse(_) => Some(Self::Horse),
            Animal::Zorro => Some(Self::Fox),
            Animal::GatoMontes => Some(Self::GatoMontes),
            Animal::Puma => Some(Self::Puma),
            Animal::Yaguarete => Some(Self::Yaguarete),
            Animal::Chimango => Some(Self::Chimango),
            Animal::Carancho => Some(Self::Carancho),
            Animal::Nandu => Some(Self::Nandu),
            _ => None,
        }
    }

    /// El archivo del modelo, bajo el [`AssetRoot`].
    pub fn path(self) -> String {
        let name = match self {
            Self::Cow => return format!("{OWN_DIR}/cow.glb"),
            Self::Bull => return format!("{OWN_DIR}/bull.glb"),
            Self::Horse => return format!("{OWN_DIR}/horse.glb"),
            Self::Fox => return format!("{OWN_DIR}/fox.glb"),
            Self::GatoMontes => return format!("{OWN_DIR}/gato.glb"),
            Self::Puma => return format!("{OWN_DIR}/puma.glb"),
            Self::Yaguarete => return format!("{OWN_DIR}/yagua.glb"),
            Self::Chimango | Self::Carancho => "eagle",
            Self::Nandu => return format!("{OWN_DIR}/rhea.glb"),
        };
        format!("{LIVESTOCK_DIR}/{name}.glb")
    }

    /// Si es un ave (su medida es la envergadura, y vuela).
    pub const fn bird(self) -> bool {
        matches!(self, Self::Chimango | Self::Carancho)
    }

    /// Los clips del archivo, en orden: los siete del ganado (el zorro y los
    /// felinos, además, el salto de un alambrado, `Fence_Leap`), o los pocos
    /// que traen el águila (planeo y aleteo) y el ñandú (dos esperas, el
    /// pastoreo, la caminata y la carrera).
    pub fn clip_names(self) -> &'static [&'static str] {
        match self {
            Self::Fox | Self::GatoMontes | Self::Puma | Self::Yaguarete => &[
                "Idle",
                "Idle_2",
                "Idle_Headlow",
                "Eating",
                "Walk",
                "Gallop",
                "Gallop_Jump",
                "Fence_Leap",
            ],
            Self::Cow | Self::Bull | Self::Horse => &[
                "Idle",
                "Idle_2",
                "Idle_Headlow",
                "Eating",
                "Walk",
                "Gallop",
                "Gallop_Jump",
            ],
            Self::Chimango | Self::Carancho => &["Glide", "Flap"],
            Self::Nandu => &["Idle", "Idle_Look", "Graze", "Walk", "Run"],
        }
    }

    /// Los mapas que viste su material `material`, si los tiene: el cuerpo
    /// del ñandú y sus tarjetas de plumas; la piel del caballo (`Main`, cuyos
    /// mapas neutros multiplican los colores por vertex del pelaje, y las
    /// partes de un solo color, que los multiplican por el suyo: los cabos, la
    /// frente, el interior del hocico y los vasos) y su crin, su copete y su
    /// cola (`Hair`, teñidos con el color de la crin del pelaje); el cuero de
    /// la vaca y del toro (`Main`, que además cambia su color por el mapa de
    /// su pelaje, [`Paint::Map`]) y sus partes de un solo color (la ubre, el
    /// morro, las pezuñas y los cuernos), y la borla de la cola, los rulos de
    /// la nuca, los flecos de las orejas y las pestañas (`Hair`); el cuerpo
    /// del zorro y de cada felino (`Main`) y sus mechones de pelo (`Hair`).
    pub fn skin(self, material: &str) -> Option<Skin> {
        let (set, masked, detail, strands) = match (self, material) {
            (Self::Nandu, "Body") => ("rhea_body", false, false, false),
            (Self::Nandu, "Feathers") => ("rhea_feathers", true, true, false),
            (Self::Horse, "Main" | "Main_Dark" | "Main_Light" | "Muzzle" | "Hooves") => {
                ("horse_body", false, false, false)
            }
            (Self::Horse, "Hair") => ("horse_hair", true, false, true),
            (Self::Cow, "Main" | "Main_Light" | "Muzzle" | "Hooves" | "Horns") => {
                ("cow_body", false, false, false)
            }
            (Self::Bull, "Main" | "Main_Light" | "Muzzle" | "Hooves" | "Horns") => {
                ("bull_body", false, false, false)
            }
            (Self::Cow | Self::Bull, "Hair") => ("cattle_hair", true, false, true),
            (Self::Fox, "Main") => ("fox_body", false, false, false),
            (Self::Fox, "Hair") => ("fox_hair", true, false, true),
            (Self::GatoMontes, "Main") => ("gato_body", false, false, false),
            (Self::GatoMontes, "Hair") => ("gato_hair", true, false, true),
            (Self::Puma, "Main") => ("puma_body", false, false, false),
            (Self::Puma, "Hair") => ("puma_hair", true, false, true),
            (Self::Yaguarete, "Main") => ("yagua_body", false, false, false),
            (Self::Yaguarete, "Hair") => ("yagua_hair", true, false, true),
            _ => return None,
        };
        Some(Skin {
            set,
            masked,
            detail,
            strands,
        })
    }

    /// El índice de animación glTF que reproduce `clip`: el mismo en los
    /// modelos de siete clips o más (los que no saltan alambrados no tienen
    /// `Fence_Leap`: su lugar lo toma `Gallop_Jump`, que nunca lo pide); en el águila, el planeo o el aleteo (el
    /// "galope" de un ave); en el ñandú, la espera con la cabeza baja y el comer son su
    /// pastoreo, y el galope su carrera.
    pub fn animation(self, clip: Clip) -> usize {
        if self == Self::Nandu {
            return match clip {
                Clip::Idle => 0,
                Clip::Idle2 => 1,
                Clip::IdleHeadlow | Clip::Eating => 2,
                Clip::Walk => 3,
                Clip::Gallop | Clip::GallopJump | Clip::FenceLeap => 4,
            };
        }
        let count = self.clip_names().len();
        if count >= CLIP_COUNT {
            return if clip.index() < count {
                clip.index()
            } else {
                Clip::GallopJump.index()
            };
        }
        usize::from(matches!(
            clip,
            Clip::Walk | Clip::Gallop | Clip::GallopJump | Clip::FenceLeap
        ))
    }

    /// Su medida de referencia en las unidades propias del modelo, medida
    /// sobre las meshes exportadas: la altura a la cruz de un cuadrúpedo (la
    /// parte superior del lomo por encima de las patas delanteras), la
    /// envergadura de un ave.
    pub const fn model_withers(self) -> f32 {
        match self {
            // Los modelos propios están en metros.
            Self::Cow => COW_WITHERS,
            Self::Bull => BULL_WITHERS,
            Self::Horse => 1.5,
            Self::Fox => FOX_WITHERS,
            Self::GatoMontes => GATO_WITHERS,
            Self::Puma => PUMA_WITHERS,
            Self::Yaguarete => YAGUA_WITHERS,
            Self::Chimango | Self::Carancho => 12.47,
            // Sobre el plumaje del lomo; la cabeza llega a 1.58.
            Self::Nandu => 1.18,
        }
    }

    /// La misma medida en el juego, en metros: como los vacunos y los caballos
    /// criollos procedurales a los que reemplazan (una vaca 1.4 m, un caballo
    /// 1.5 m), con el toro un poco más grande; un zorro gris de 0.38 m a la
    /// cruz, un gato montés de 0.28 m, un puma de 0.7 m y un yaguareté de
    /// 0.75 m; un chimango de 0.9 m de envergadura y un carancho de 1.25 m; un
    /// ñandú de 1.1 m sobre el plumaje del lomo (la cabeza a 1.5 m).
    pub const fn withers(self) -> f32 {
        match self {
            Self::Cow => 1.4,
            Self::Bull => 1.55,
            Self::Horse => 1.5,
            Self::Fox => 0.38,
            Self::GatoMontes => 0.28,
            Self::Puma => 0.7,
            Self::Yaguarete => 0.75,
            Self::Chimango => 0.9,
            Self::Carancho => 1.25,
            Self::Nandu => 1.1,
        }
    }

    /// Metros por unidad del modelo.
    pub fn scale(self) -> f32 {
        self.withers() / self.model_withers()
    }

    /// La velocidad sobre el suelo que muestra su clip `Walk`, en m/s: las de
    /// los modelos propios, medidas sobre sus clips ([`FOX_WALK_SPEED`],
    /// [`GATO_WALK_SPEED`] y las demás), escaladas a su tamaño; la del ñandú,
    /// la que construye `tools/rhea.py` (0.88 unidades por segundo).
    pub fn walk_speed(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => CATTLE_WALK_SPEED * self.scale(),
            Self::Horse => HORSE_WALK_SPEED,
            Self::Fox => FOX_WALK_SPEED * self.scale(),
            Self::GatoMontes => GATO_WALK_SPEED * self.scale(),
            Self::Puma => PUMA_WALK_SPEED * self.scale(),
            Self::Yaguarete => YAGUA_WALK_SPEED * self.scale(),
            Self::Chimango | Self::Carancho => 1.0,
            Self::Nandu => 0.88 * self.scale(),
        }
    }

    /// La velocidad sobre el suelo que muestra su clip `Gallop`, en m/s: las de
    /// los modelos propios, medidas sobre sus clips; la carrera del ñandú,
    /// 7.23 (la zancada de 0.85 apoyada el 28 % de un ciclo de 0.42 s); las
    /// rapaces, su aleteo.
    pub fn gallop_speed(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => CATTLE_GALLOP_SPEED * self.scale(),
            Self::Horse => HORSE_GALLOP_SPEED,
            Self::Fox => FOX_GALLOP_SPEED * self.scale(),
            Self::GatoMontes => GATO_GALLOP_SPEED * self.scale(),
            Self::Puma => PUMA_GALLOP_SPEED * self.scale(),
            Self::Yaguarete => YAGUA_GALLOP_SPEED * self.scale(),
            Self::Nandu => 7.23 * self.scale(),
            _ => self.walk_speed(),
        }
    }

    /// Cómo salta un alambrado ([`CritterKind::leap`]): el zorro y los
    /// felinos.
    pub const fn leap(self) -> Option<Leap> {
        let kind = match self {
            Self::Fox => CritterKind::Zorro,
            Self::GatoMontes => CritterKind::GatoMontes,
            Self::Puma => CritterKind::Puma,
            Self::Yaguarete => CritterKind::Yaguarete,
            _ => return None,
        };
        kind.leap()
    }

    /// Lo más rápido que se reproduce su galope respecto de cómo se creó: los
    /// animales chicos dan pasos más rápidos que lo que muestran sus clips.
    /// El puma y el yaguareté, como el galope del caballo
    /// ([`GALLOP_RATE`](crate::character::horse::GALLOP_RATE)), a lo sumo el
    /// doble: su pique ya alarga la zancada, así que a su velocidad de huida
    /// lo reproducen 1.76 y 1.58 veces más rápido. El gato montés, más
    /// rápido ([`GATO_GALLOP_RATE`]).
    pub const fn max_pace(self) -> f32 {
        match self {
            Self::Cow | Self::Bull | Self::Horse => 1.6,
            Self::Puma | Self::Yaguarete => crate::character::horse::GALLOP_RATE.1,
            Self::GatoMontes => GATO_GALLOP_RATE,
            _ => 2.5,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Los clips que se conservan en cada modelo, en el orden en que los escribió
/// `tools/glbtrim.py` (sus índices de animación glTF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clip {
    Idle,
    Idle2,
    IdleHeadlow,
    Eating,
    Walk,
    Gallop,
    /// El salto en el aire (patas recogidas, cuerpo cabeceando): lo
    /// reproduce un caballo montado en vuelo; en el zorro y los felinos es un
    /// brinco sobre una presa, que el juego todavía no reproduce; los demás
    /// lo mantienen con peso cero.
    GallopJump,
    /// El salto de un alambrado del zorro y de los felinos ([`Gait::Leap`]),
    /// un bote largo y chato que sale del galope y vuelve a él. Solo lo
    /// traen sus modelos: no está en [`Clip::ALL`], los siete que comparten
    /// todos (y que el caballo mezcla por peso).
    FenceLeap,
}

/// Cuántos clips contienen todos los modelos de ganado (la longitud de
/// [`Clip::ALL`]).
pub const CLIP_COUNT: usize = 7;

impl Clip {
    pub const ALL: [Self; CLIP_COUNT] = [
        Self::Idle,
        Self::Idle2,
        Self::IdleHeadlow,
        Self::Eating,
        Self::Walk,
        Self::Gallop,
        Self::GallopJump,
    ];

    /// Los que se cargan de cada modelo, en el orden de sus nodos en el grafo
    /// ([`SpeciesModel::nodes`]): [`Clip::ALL`] y el salto de un alambrado.
    pub const LOADED: [Self; CLIP_COUNT + 1] = [
        Self::Idle,
        Self::Idle2,
        Self::IdleHeadlow,
        Self::Eating,
        Self::Walk,
        Self::Gallop,
        Self::GallopJump,
        Self::FenceLeap,
    ];

    /// El nombre del clip en el archivo glTF.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Idle2 => "Idle_2",
            Self::IdleHeadlow => "Idle_Headlow",
            Self::Eating => "Eating",
            Self::Walk => "Walk",
            Self::Gallop => "Gallop",
            Self::GallopJump => "Gallop_Jump",
            Self::FenceLeap => "Fence_Leap",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Una mezcla con semilla de `seed` y `salt`, para una variedad determinista.
pub(crate) fn mix(seed: u32, salt: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Segundos que aletea una rapaz cada vez, entre sus planeos.
pub const FLAP_SECONDS: f32 = 2.0;

/// Segundos que un caballo de corral mantiene una espera antes de la siguiente.
pub(crate) const HORSE_IDLE: f32 = 14.0;

/// El clip que reproduce `animal` para `pose`. Los animales parados eligen una
/// de las esperas según su `seed`; los caballos de corral, que nunca se mueven,
/// cambian de espera (o pastan) cada [`HORSE_IDLE`] segundos de `clock`.
pub fn clip_for(animal: Animal, pose: &AnimalPose, seed: u32, clock: f32) -> Clip {
    let idles = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];
    match pose.gait {
        // Las rapaces planean y, cada tanto, aletean un par de segundos: el
        // chimango más seguido que el carancho.
        Gait::Glide | Gait::Fly if matches!(animal, Animal::Chimango | Animal::Carancho) => {
            let period = if animal == Animal::Chimango {
                9.0
            } else {
                14.0
            };
            let t = (clock.max(0.0) + (seed % 97) as f32).rem_euclid(period);
            if pose.gait == Gait::Fly || t < FLAP_SECONDS {
                Clip::Gallop
            } else {
                Clip::Idle
            }
        }
        Gait::Graze => Clip::Eating,
        Gait::Walk if pose.speed > GALLOP_SPEED => Clip::Gallop,
        Gait::Walk => Clip::Walk,
        Gait::Run => Clip::Gallop,
        Gait::Leap => Clip::FenceLeap,
        _ if matches!(animal, Animal::Horse(_)) => {
            let turn = ((clock.max(0.0) + (seed % 97) as f32) / HORSE_IDLE) as u32;
            [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow, Clip::Eating][mix(seed, turn) as usize % 4]
        }
        _ => idles[mix(seed, 1) as usize % 3],
    }
}

/// La velocidad de reproducción de `clip` para un animal de `species` que se
/// mueve a `ground` m/s: la caminata y el galope ajustados a ella; el salto de
/// un alambrado ([`Species::leap`]), para que la parte del clip en el aire
/// ([`LEAP_CLIP`], un segundo) dure su vuelo: cerca de como se creó (entre
/// 0.9 y 1 vez); el resto tal como se creó.
pub fn clip_speed(clip: Clip, species: Species, ground: f32) -> f32 {
    match (clip, species.leap()) {
        _ if species.bird() => 1.0,
        (Clip::Walk, _) => (ground / species.walk_speed()).clamp(0.4, 2.0),
        (Clip::Gallop, _) => (ground / species.gallop_speed()).clamp(0.6, species.max_pace()),
        (Clip::FenceLeap, Some(leap)) => {
            (LEAP_CLIP.1 - LEAP_CLIP.0) * FENCE_LEAP_SECONDS / leap.flight()
        }
        _ => 1.0,
    }
}

/// La fracción con semilla de un clip ya reproducida cuando un animal lo
/// empieza, en `[0, 1)`: los vecinos nunca se mueven al mismo paso.
pub fn start_offset(seed: u32, clip: Clip) -> f32 {
    (mix(seed, 7 + clip.index() as u32) % 1000) as f32 / 1000.0
}

/// Si la animación de un animal está congelada: fuera de la vista, o más allá
/// de [`FREEZE_FAR`] (de vuelta dentro de [`RESUME_NEAR`] una vez que `was`
/// congelado).
pub fn frozen(was: bool, distance: f32, in_view: bool) -> bool {
    !in_view || distance > if was { RESUME_NEAR } else { FREEZE_FAR }
}

/// Desde dónde se ve el mundo: la cámara 3D activa dibujada en último lugar (el
/// orden más alto), la entidad más baja entre iguales; `None` si no hay
/// ninguna. La misma cámara sea cual sea el orden en que las liste una
/// consulta.
pub fn view_eye<'a>(
    cameras: impl IntoIterator<Item = (Entity, &'a GlobalTransform, &'a Camera)>,
) -> Option<Vec3> {
    cameras
        .into_iter()
        .filter(|(_, _, camera)| camera.is_active)
        .max_by_key(|(entity, _, camera)| (camera.order, std::cmp::Reverse(*entity)))
        .map(|(_, at, _)| at.translation())
}

/// Si un modelo está a la vista: alguna de sus meshes (`probes`) lo está, según
/// `visible`; una pieza pequeña descartada por culling (un sombrero, un botón)
/// no oculta el resto. Si no hay ninguna mesh a la que preguntar, lo está.
pub fn any_in_view(probes: &[Entity], visible: impl Fn(Entity) -> bool) -> bool {
    probes.is_empty() || probes.iter().any(|&p| visible(p))
}

/// El transform del modelo bajo su animal: girado de mirar hacia +Z al
/// `heading` (+X es el frente del cuerpo), cabeceado `pitch` radianes con la
/// pendiente (positivo: el hocico hacia arriba) y escalado a tamaño real.
pub fn model_transform(species: Species, heading: Heading, pitch: f32) -> Transform {
    let turn = Quat::from_rotation_y(heading.radians())
        * Quat::from_rotation_z(pitch)
        * Quat::from_rotation_y(FRAC_PI_2);
    Transform::from_rotation(turn).with_scale(Vec3::splat(species.scale()))
}

/// Un vertex de la mesh exportada en el marco del cuerpo del animal parado,
/// en metros: +X su frente, +Y hacia arriba, +Z su lado derecho. Todos los
/// modelos salen de Blender (`tools/fbx2glb.py` o los scripts de los
/// modelos propios): Y hacia arriba, en unidades del modelo, mirando hacia +Z.
pub fn body_point(species: Species, raw: Vec3) -> Vec3 {
    // El modelo gira para mirar hacia +X.
    Vec3::new(raw.z, raw.y, -raw.x) * species.scale()
}

/// Cómo pinta un pelaje uno de los materiales del modelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    /// Su propio color, sin cambios.
    Keep,
    /// Un color liso.
    Flat(Color),
    /// Blanco, con el color en los vertices de la copia de la mesh del cuerpo
    /// para este pelaje ([`coat_color_at`]).
    Vertex,
    /// No se dibuja (los cuernos del Angus mocho).
    Hidden,
    /// Blanco, con el pelaje horneado en el mapa de color de su conjunto (los
    /// vacunos, [`coat_map_name`]); sin ese mapa, este color liso.
    Map(Color),
}

/// La pintura que el pelaje de `animal` le da al material llamado `material`.
///
/// Vacunos (`tools/cattle.py` `COATS`): `Main` es el cuero, con el pelaje
/// horneado en su mapa ([`Paint::Map`]: sin él, el color del cuerpo);
/// `Main_Light` la piel lampiña de la ubre; `Muzzle` el morro pelado; `Hair`
/// la borla, los rulos y los flecos. Un Hereford es colorado con la cara, el
/// pecho, la línea de abajo y las patas blancas; un Angus, todo negro y
/// mocho; un overo, blanco con manchas negras, con la cara y las patas
/// negras; un Shorthorn rosillo, de pelos colorados y blancos mezclados.
/// Caballos: `Main` es el cuerpo (colores por vertex, de
/// los pelajes criollos, [`Coat`](crate::character::Coat)), `Hair` la crin y la
/// cola, y `Main_Dark` y `Main_Light` partes de la cabeza.
pub fn paint(animal: Animal, material: &str) -> Paint {
    let rgb = |c: [u8; 3]| Paint::Flat(Color::srgb_u8(c[0], c[1], c[2]));
    match animal {
        Animal::Cow(coat) | Animal::Bull(coat) => {
            // El cuerpo, la piel de la ubre, el morro, el pelo largo, las
            // pezuñas y los cuernos.
            let [main, skin, muzzle, hair, hoof, horns] = match coat {
                CowCoat::Hereford => [
                    [108, 38, 20],
                    [214, 160, 142],
                    [206, 156, 136],
                    [232, 226, 212],
                    [64, 54, 46],
                    [226, 212, 178],
                ],
                CowCoat::Angus => [
                    [26, 23, 21],
                    [40, 34, 32],
                    [30, 27, 25],
                    [22, 20, 19],
                    [30, 27, 24],
                    [226, 212, 178],
                ],
                CowCoat::Overo => [
                    [230, 226, 216],
                    [60, 52, 48],
                    [44, 38, 34],
                    [26, 24, 22],
                    [40, 34, 30],
                    [222, 206, 170],
                ],
                // Sin su mapa, el gris acerado que se ve a media distancia (un
                // poco azulado: la luz cálida del juego lo deja gris).
                CowCoat::Roan => [
                    [144, 160, 177],
                    [212, 166, 148],
                    [200, 152, 132],
                    [132, 112, 106],
                    [78, 66, 54],
                    [228, 216, 184],
                ],
            };
            match material {
                "Main" => Paint::Map(Color::srgb_u8(main[0], main[1], main[2])),
                "Main_Light" => rgb(skin),
                "Muzzle" => rgb(muzzle),
                "Hair" => rgb(hair),
                "Hooves" => rgb(hoof),
                "Horns" if coat == CowCoat::Angus => Paint::Hidden,
                "Horns" => rgb(horns),
                _ => Paint::Keep,
            }
        }
        Animal::Horse(coat) => match material {
            "Main" => Paint::Vertex,
            "Hair" => rgb(coat.mane()),
            "Main_Dark" => rgb(coat.points()),
            "Main_Light" => rgb(coat.head()),
            "Hooves" => rgb(coat.hoof()),
            // La piel desnuda de los ollares y de la boca.
            "Muzzle" => rgb([48, 42, 38]),
            _ => Paint::Keep,
        },
        // El zorro gris y los felinos: su único pelaje está horneado en los
        // mapas de su cuerpo y de su pelo (`tools/fox.py`, `tools/felid.py`),
        // que se dibujan tal como están.
        Animal::Zorro | Animal::GatoMontes | Animal::Puma | Animal::Yaguarete => Paint::Keep,
        Animal::Chimango => match material {
            "Wings" => rgb([118, 86, 56]),
            "Head" => rgb([196, 168, 124]),
            "Beak" => rgb([196, 186, 156]),
            "Claws" => rgb([160, 148, 112]),
            _ => Paint::Keep,
        },
        // Oscuro, con el cuello claro, la cara rojo anaranjada y las patas
        // amarillas.
        Animal::Carancho => match material {
            "Wings" => rgb([40, 32, 26]),
            "Head" => rgb([228, 218, 194]),
            "Beak" => rgb([214, 92, 40]),
            "Claws" => rgb([214, 182, 60]),
            _ => Paint::Keep,
        },
        _ => Paint::Keep,
    }
}

/// El color por vertex del cuerpo de `animal` en `p` (en metros, marco del
/// cuerpo), mirando hacia `normal`, como RGBA lineal. Solo para los materiales
/// [`Paint::Vertex`].
pub fn coat_color_at(animal: Animal, p: Vec3, normal: Vec3) -> [f32; 4] {
    let srgb = match animal {
        Animal::Horse(coat) => coat.color_at(p, normal),
        // Los demás no tienen materiales por vertex.
        _ => [255, 255, 255],
    };
    Color::srgb_u8(srgb[0], srgb[1], srgb[2])
        .to_linear()
        .to_f32_array()
}

/// Una especie cargada: su escena, sus clips y su grafo compartido.
#[derive(Debug, Clone)]
pub struct SpeciesModel {
    /// El archivo completo, retenido para que se cargue una sola vez: si solo
    /// se retienen partes etiquetadas de él, una parte pedida más tarde vuelve
    /// a cargar el archivo, y la escena creada a partir de la primera carga se
    /// crea otra vez.
    pub file: Handle<bevy::gltf::Gltf>,
    pub scene: Handle<WorldAsset>,
    pub clips: Vec<Handle<AnimationClip>>,
    pub graph: Handle<AnimationGraph>,
    /// El nodo del grafo de cada [`Clip`], en el orden de [`Clip::LOADED`].
    pub nodes: Vec<AnimationNodeIndex>,
}

/// Los modelos de ganado encontrados al iniciar, por [`Species`] (`None` cuando
/// falta el archivo: esa especie no se dibuja).
#[derive(Resource, Debug, Clone, Default)]
pub struct LivestockModels(pub [Option<SpeciesModel>; Species::ALL.len()]);

impl LivestockModels {
    pub fn get(&self, species: Species) -> Option<&SpeciesModel> {
        self.0[species.index()].as_ref()
    }
}

/// Las copias por pelaje de los materiales y las meshes del cuerpo de los
/// modelos, creadas una vez y compartidas por todos los animales de ese pelaje.
#[derive(Resource, Debug, Default)]
pub struct CoatCache {
    materials: HashMap<(AssetId<StandardMaterial>, Animal), Handle<StandardMaterial>>,
    meshes: HashMap<(AssetId<Mesh>, Animal), Handle<Mesh>>,
    /// Los mapas de cada conjunto de [`SKIN_SETS`] que se encontró.
    skins: HashMap<&'static str, SkinMaps>,
    /// Los mapas de color de los pelajes vacunos que se encontraron, por
    /// conjunto y pelaje ([`coat_map_name`]).
    coat_maps: HashMap<(&'static str, CowCoat), Handle<Image>>,
}

impl CoatCache {
    /// Las copias por pelaje creadas hasta ahora: materiales, y después meshes.
    pub fn len(&self) -> (usize, usize) {
        (self.materials.len(), self.meshes.len())
    }
}

/// La raíz de un modelo de ganado, hija de su animal.
#[derive(Component, Debug, Clone, Copy)]
pub struct LivestockModel {
    pub species: Species,
}

/// Un modelo que todavía carga: su escena se crea cuando esté listo.
#[derive(Component, Debug, Clone)]
pub struct LivestockPending(pub Handle<WorldAsset>);

/// El estado de animación de un modelo creado.
#[derive(Component, Debug, Clone)]
pub struct LivestockRig {
    /// La entidad con su `AnimationPlayer`.
    pub player: Entity,
    /// Sus meshes dibujadas: está a la vista mientras alguna lo esté.
    pub probes: Vec<Entity>,
    pub clip: Option<Clip>,
    pub frozen: bool,
    /// Sus tarjetas de plumas, que solo se dibujan cerca ([`detail_shown`]).
    pub detail: Vec<Entity>,
    pub detail_shown: bool,
}

/// Empieza a cargar los modelos y construye el grafo de cada uno. Sin un
/// servidor de assets, una raíz o el plugin de animación (tests, una app vacía)
/// no carga ninguno.
pub(super) fn load_livestock(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    graphs: Option<ResMut<Assets<AnimationGraph>>>,
    support: Option<Res<CompressedImageFormatSupport>>,
) {
    let (Some(server), Some(root), Some(mut graphs)) = (server, root, graphs) else {
        commands.init_resource::<CoatCache>();
        commands.insert_resource(LivestockModels::default());
        return;
    };
    // Los mapas de los modelos propios, cuando están todos sus archivos y la
    // GPU tiene formatos comprimidos (los KTX2 se transcodifican a uno).
    let mut cache = CoatCache::default();
    if support.as_ref().is_none_or(|s| !s.0.is_empty()) {
        for set in SKIN_SETS {
            let paths = MAPS.map(|(name, srgb)| (skin_path(set, name), srgb));
            if let Some((missing, _)) = paths.iter().find(|(p, _)| !root.0.join(p).is_file()) {
                warn!("fauna maps {missing} not found; drawing plain colour");
                continue;
            }
            let [color, normal, orm] =
                paths.map(|(path, srgb)| load_map_with(&server, &path, srgb, clamped()));
            cache.skins.insert(set, SkinMaps { color, normal, orm });
            for coat in CowCoat::ALL {
                let path = skin_path(set, coat_map_name(coat));
                if matches!(set, "cow_body" | "bull_body") && root.0.join(&path).is_file() {
                    let map = load_map_with(&server, &path, true, clamped());
                    cache.coat_maps.insert((set, coat), map);
                }
            }
        }
    }
    commands.insert_resource(cache);
    let mut models = LivestockModels::default();
    for species in Species::ALL {
        let path = species.path();
        let Some(scene) = gltf::load_scene(&server, Some(&root), &path) else {
            continue;
        };
        let file = server.load(path.clone());
        // Un clip por cada [`Clip`], aunque el archivo traiga menos (ver
        // [`Species::animation`]).
        let clips: Vec<Handle<AnimationClip>> = Clip::LOADED
            .iter()
            .map(|&c| {
                let index = species.animation(c);
                server.load(GltfAssetLabel::Animation(index).from_asset(path.clone()))
            })
            .collect();
        let (graph, nodes) = AnimationGraph::from_clips(clips.iter().cloned());
        models.0[species.index()] = Some(SpeciesModel {
            file,
            scene,
            clips,
            graph: graphs.add(graph),
            nodes,
        });
    }
    commands.insert_resource(models);
}

/// Cuelga un modelo bajo cada animal nuevo que se dibuja a partir de uno.
pub(super) fn attach_livestock(
    mut commands: Commands,
    models: Option<Res<LivestockModels>>,
    added: Query<(Entity, &Animal, &AnimalPose), Added<Animal>>,
) {
    for (entity, &animal, pose) in &added {
        let Some(species) = Species::of(animal) else {
            continue;
        };
        let mut model = commands.spawn((
            Name::new("Livestock model"),
            LivestockModel { species },
            model_transform(species, pose.heading, pose.pitch),
            Visibility::default(),
            ChildOf(entity),
        ));
        if let Some(m) = models.as_ref().and_then(|m| m.get(species)) {
            model.insert(LivestockPending(m.scene.clone()));
        }
    }
}

/// Crea la escena de cada modelo en espera cuando está listo; descarta uno que
/// falló.
pub(super) fn swap_in_livestock(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    pending: Query<(Entity, &LivestockPending)>,
) {
    let Some(server) = server else { return };
    for (entity, wait) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<LivestockPending>()
                    .try_insert(WorldAssetRoot(wait.0.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).remove::<LivestockPending>();
            }
            ModelState::Loading => {}
        }
    }
}

/// Con qué tiñe [`dress_livestock`].
type Stores<'w> = (
    ResMut<'w, CoatCache>,
    ResMut<'w, Assets<StandardMaterial>>,
    ResMut<'w, Assets<Mesh>>,
);

/// Prepara un modelo en el momento en que se crea su escena: el rango
/// compartido en sus meshes, su pelaje y el grafo de la especie en su
/// reproductor.
#[allow(clippy::too_many_arguments)]
pub(super) fn dress_livestock(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    (mut cache, mut materials, mut meshes): Stores,
    models: Option<Res<LivestockModels>>,
    roots: Query<(&ChildOf, &LivestockModel)>,
    animals: Query<&Animal>,
    children: Query<&Children>,
    parts: Query<ModelPart>,
) {
    let root = ready.entity;
    let Ok((parent, model)) = roots.get(root) else {
        return;
    };
    let (Ok(&animal), Some(species)) = (
        animals.get(parent.parent()),
        models.as_ref().and_then(|m| m.get(model.species)),
    ) else {
        return;
    };
    // Sus tarjetas de plumas, cuando tiene y se dibujan (ver [`skinned`]).
    let detail = children
        .iter_descendants(root)
        .filter(|&e| {
            parts.get(e).is_ok_and(|(mesh, _, name, _, _)| {
                mesh.is_some()
                    && name
                        .and_then(|n| model.species.skin(&n.0))
                        .is_some_and(|s| s.detail && cache.skins.contains_key(s.set))
            })
        })
        .collect();
    let (player, probes) = dress_meshes(
        &mut commands,
        (&mut cache, &mut materials, &mut meshes),
        root,
        (animal, model.species),
        &children,
        &parts,
    );
    if let Some(player) = player {
        commands.entity(player).try_insert((
            AnimationGraphHandle(species.graph.clone()),
            AnimationTransitions::new(),
        ));
        commands.entity(root).try_insert(LivestockRig {
            player,
            probes,
            clip: None,
            frozen: false,
            detail,
            detail_shown: true,
        });
    }
}

/// Lo que [`dress_meshes`] lee de los nodos de un modelo: su mesh, su material
/// y el nombre del material, si es el reproductor de animación y el nombre de
/// su malla (la liviana termina en [`LIGHT_SUFFIX`]).
pub(crate) type ModelPart = (
    Option<&'static Mesh3d>,
    Option<&'static MeshMaterial3d<StandardMaterial>>,
    Option<&'static GltfMaterialName>,
    Has<AnimationPlayer>,
    Option<&'static GltfMeshName>,
);

/// Si la malla llamada `name` es la liviana de su modelo.
fn is_light(name: Option<&GltfMeshName>) -> bool {
    name.is_some_and(|n| n.0.ends_with(LIGHT_SUFFIX))
}

/// Prepara las meshes del modelo creado bajo `root` como `animal` (de
/// `species`): el rango compartido de los accesorios, los materiales teñidos
/// del pelaje y el cuerpo con colores por vertex (copias creadas una vez por
/// pelaje, en `cache`), y las partes ocultas ocultas. Devuelve la entidad con
/// su reproductor de animación y las meshes dibujadas (sus sondas de
/// visibilidad). Los modelos de los caballos ensillados se preparan igual (ver
/// [`horse`](crate::character::horse)).
pub(crate) fn dress_meshes(
    commands: &mut Commands,
    (cache, materials, meshes): (
        &mut CoatCache,
        &mut Assets<StandardMaterial>,
        &mut Assets<Mesh>,
    ),
    root: Entity,
    (animal, species): (Animal, Species),
    children: &Query<&Children>,
    parts: &Query<ModelPart>,
) -> (Option<Entity>, Vec<Entity>) {
    let range = animal.range();
    // Con una malla liviana, cada malla se dibuja en su tramo de distancias.
    let has_light = children
        .iter_descendants(root)
        .any(|e| parts.get(e).is_ok_and(|p| p.0.is_some() && is_light(p.4)));
    let mut player = None;
    let mut probes = Vec::new();
    for entity in children.iter_descendants(root) {
        let Ok((mesh, material, name, has_player, mesh_name)) = parts.get(entity) else {
            continue;
        };
        if has_player {
            player = Some(entity);
        }
        let Some(mesh) = mesh else { continue };
        let mut part = commands.entity(entity);
        if has_light {
            part.try_insert(light_range(&range, is_light(mesh_name)));
        } else {
            part.try_insert(range.clone());
        }
        let paint = name.map_or(Paint::Keep, |n| paint(animal, &n.0));
        let skin = name.and_then(|n| species.skin(&n.0));
        let maps = skin.and_then(|s| cache.skins.get(s.set).cloned());
        let coat_map = match (animal, skin) {
            (Animal::Cow(coat) | Animal::Bull(coat), Some(s)) => {
                cache.coat_maps.get(&(s.set, coat)).cloned()
            }
            _ => None,
        };
        // Las tarjetas de plumas sin sus mapas serían rectángulos lisos.
        if paint == Paint::Hidden || skin.is_some_and(|s| s.masked) && maps.is_none() {
            part.try_insert(Visibility::Hidden);
            continue;
        }
        probes.push(entity);
        if let Some(material) = material {
            let tinted = cache
                .materials
                .entry((material.0.id(), animal))
                .or_insert_with(|| {
                    let mut m = materials.get(&material.0).cloned().unwrap_or_default();
                    // El brillo metálico de la exportación parece plástico.
                    m.metallic = 0.0;
                    m.perceptual_roughness = 0.9;
                    if let Some(worn) = skin.and_then(|s| skinned(&m, s, maps)) {
                        m = worn;
                    }
                    // Después de los mapas: el color liso tiñe los suyos (la
                    // crin del caballo).
                    match paint {
                        Paint::Flat(color) => m.base_color = color,
                        Paint::Vertex => m.base_color = Color::WHITE,
                        // El pelaje horneado reemplaza la luminancia neutra
                        // (que ya lleva multiplicada).
                        Paint::Map(color) => match coat_map {
                            Some(map) if m.base_color_texture.is_some() => {
                                m.base_color = Color::WHITE;
                                m.base_color_texture = Some(map);
                            }
                            _ => m.base_color = color,
                        },
                        Paint::Keep | Paint::Hidden => {}
                    }
                    materials.add(m)
                })
                .clone();
            part.try_insert(MeshMaterial3d(tinted));
        }
        if paint == Paint::Vertex {
            let key = (mesh.0.id(), animal);
            let coloured = match cache.meshes.get(&key) {
                Some(h) => Some(h.clone()),
                None => meshes
                    .get(&mesh.0)
                    .map(|m| coat_mesh(m, animal, species))
                    .map(|m| {
                        let h = meshes.add(m);
                        cache.meshes.insert(key, h.clone());
                        h
                    }),
            };
            if let Some(coloured) = coloured {
                part.try_insert(Mesh3d(coloured));
            }
        }
    }
    (player, probes)
}

/// `mesh` con cada vertex coloreado para el pelaje de `animal`.
fn coat_mesh(mesh: &Mesh, animal: Animal, species: Species) -> Mesh {
    let mut out = mesh.clone();
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => Vec::new(),
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => vec![[0.0, 0.0, 1.0]; positions.len()],
    };
    let colors: Vec<[f32; 4]> = positions
        .iter()
        .zip(normals.iter().chain(std::iter::repeat(&[0.0, 0.0, 1.0])))
        .map(|(p, n)| {
            let at = body_point(species, Vec3::from(*p));
            let normal = body_point(species, Vec3::from(*n)).normalize_or_zero();
            coat_color_at(animal, at, normal)
        })
        .collect();
    out.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    out
}

/// Un modelo tal como lo ve [`drive_livestock`].
type ModelRig = (
    &'static ChildOf,
    &'static LivestockModel,
    &'static mut Transform,
    &'static GlobalTransform,
    Option<&'static mut LivestockRig>,
);

/// Gira cada modelo y reproduce su clip: elegido a partir del estado de la
/// fauna, ajustado a su velocidad, empezado en su desfase con semilla, y
/// congelado lejos o fuera de la vista.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_livestock(
    mut commands: Commands,
    time: Res<Time>,
    models: Option<Res<LivestockModels>>,
    clips: Option<Res<Assets<AnimationClip>>>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    animals: Query<(&Animal, &AnimalPose, Option<&AnimalSeed>)>,
    mut rigs: Query<ModelRig>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    seen: Query<&ViewVisibility>,
) {
    let clock = time.elapsed_secs();
    let eye = view_eye(cameras);
    for (child_of, model, mut transform, at, rig) in &mut rigs {
        let Ok((&animal, pose, seed)) = animals.get(child_of.parent()) else {
            continue;
        };
        // Solo cuando se movió: un animal quieto deja intacto su subárbol.
        transform.set_if_neq(model_transform(model.species, pose.heading, pose.pitch));
        let (Some(mut rig), Some(species)) =
            (rig, models.as_ref().and_then(|m| m.get(model.species)))
        else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        // La visibilidad solo se conoce con una cámara dibujando.
        let in_view =
            eye.is_none() || any_in_view(&rig.probes, |p| seen.get(p).is_ok_and(|v| v.get()));
        if !rig.detail.is_empty() {
            let shown = detail_shown(rig.detail_shown, distance);
            if shown != rig.detail_shown {
                rig.detail_shown = shown;
                let visibility = if shown {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                for &card in &rig.detail {
                    commands.entity(card).try_insert(visibility);
                }
            }
        }
        let freeze = frozen(rig.frozen, distance, in_view);
        if freeze != rig.frozen {
            rig.frozen = freeze;
            let mut player = commands.entity(rig.player);
            if freeze {
                player.try_remove::<AnimationGraphHandle>();
            } else {
                player.try_insert(AnimationGraphHandle(species.graph.clone()));
            }
        }
        if rig.frozen {
            continue;
        }
        let Ok((mut player, mut transitions)) = players.get_mut(rig.player) else {
            continue;
        };
        let seed = seed.map_or(child_of.parent().index_u32(), |s| s.0);
        let clip = clip_for(animal, pose, seed, clock);
        let node = species.nodes[clip.index()];
        if rig.clip != Some(clip) {
            let blend = if rig.clip.is_some() {
                BLEND
            } else {
                Duration::ZERO
            };
            let duration = clips
                .as_ref()
                .and_then(|c| c.get(&species.clips[clip.index()]))
                .map_or(0.0, AnimationClip::duration);
            if clip == Clip::FenceLeap {
                // Un salto se ve una vez, desde el impulso.
                transitions
                    .play(&mut player, node, blend.min(LEAP_BLEND))
                    .seek_to(LEAP_CLIP.0 * duration);
            } else {
                transitions
                    .play(&mut player, node, blend)
                    .repeat()
                    .seek_to(start_offset(seed, clip) * duration);
            }
            rig.clip = Some(clip);
        }
        if let Some(active) = player.animation_mut(node) {
            active.set_speed(clip_speed(clip, model.species, pose.speed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Coat;
    use crate::gltf::tests::{root, settle, unfinished_app};
    use bevy::camera::visibility::VisibilityRange;
    use pampa_core::fauna::CritterKind;

    fn standing(gait: Gait, speed: f32) -> AnimalPose {
        AnimalPose {
            gait,
            speed,
            ..default()
        }
    }

    #[test]
    fn fauna_states_map_to_clips() {
        let cow = Animal::Cow(CowCoat::Hereford);
        let clip = |animal, gait, speed| clip_for(animal, &standing(gait, speed), 3, 0.0);
        assert_eq!(clip(cow, Gait::Graze, 0.0), Clip::Eating);
        assert_eq!(clip(cow, Gait::Walk, 0.5), Clip::Walk);
        assert_eq!(
            clip(cow, Gait::Walk, 1.3),
            Clip::Walk,
            "walking off a rider"
        );
        assert_eq!(clip(cow, Gait::Walk, 4.0), Clip::Gallop);
        assert_eq!(clip(cow, Gait::Run, 4.0), Clip::Gallop);
        // Parado: una de las esperas, la misma para el mismo animal, y no la
        // misma para toda la tropa.
        let idles: std::collections::HashSet<Clip> = (0..40)
            .map(|seed| {
                let c = clip_for(cow, &standing(Gait::Stand, 0.0), seed, 0.0);
                assert_eq!(c, clip_for(cow, &standing(Gait::Stand, 0.0), seed, 99.0));
                c
            })
            .collect();
        assert_eq!(
            idles,
            [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow].into(),
            "{idles:?}"
        );
        // Un caballo de corral cambia lo que hace cada tanto, de forma
        // determinista.
        let horse = Animal::Horse(Coat::Bayo);
        let seen: std::collections::HashSet<Clip> = (0..40)
            .map(|k| {
                clip_for(
                    horse,
                    &standing(Gait::Stand, 0.0),
                    11,
                    k as f32 * HORSE_IDLE,
                )
            })
            .collect();
        assert!(seen.len() >= 3 && seen.contains(&Clip::Eating), "{seen:?}");
        assert_eq!(
            clip_for(horse, &standing(Gait::Stand, 0.0), 11, 30.0),
            clip_for(horse, &standing(Gait::Stand, 0.0), 11, 30.0)
        );
    }

    #[test]
    fn felines_gallop_with_their_own_clip_and_raptors_glide_with_a_few_flaps() {
        for species in [Species::GatoMontes, Species::Puma, Species::Yaguarete] {
            let clip = |c| species.clip_names()[species.animation(c)];
            assert_eq!(clip(Clip::Gallop), "Gallop", "{species:?}");
            assert_eq!(clip(Clip::Walk), "Walk", "{species:?}");
            assert_eq!(clip(Clip::Idle2), "Idle_2", "{species:?}");
        }
        // Huyen a su velocidad de pique con el galope dentro de su tope (las
        // manos no se deslizan): el puma y el yaguareté, con la zancada larga
        // de su pique, a menos de 1.8 veces lo que se creó; el gato montés,
        // chico, con más trancos por segundo.
        for (species, kind) in [
            (Species::GatoMontes, CritterKind::GatoMontes),
            (Species::Puma, CritterKind::Puma),
            (Species::Yaguarete, CritterKind::Yaguarete),
        ] {
            let run = kind.profile().run;
            let rate = run / species.gallop_speed();
            println!("{species:?}: gallop at {run:.1} m/s plays {rate:.2} times as made");
            assert_eq!(clip_speed(Clip::Gallop, species, run), rate);
            assert!(rate <= species.max_pace(), "{species:?}: {rate}");
            assert!(
                species == Species::GatoMontes || rate < 1.8,
                "{species:?}: {rate}"
            );
            // Después del pique, a su velocidad de crucero, el galope sigue
            // ajustado al suelo, sin llegar al piso de la velocidad.
            let cruise = kind.profile().cruise;
            let slow = cruise / species.gallop_speed();
            println!("{species:?}: cruise at {cruise:.1} m/s plays {slow:.2} times as made");
            assert_eq!(clip_speed(Clip::Gallop, species, cruise), slow);
        }
        assert_eq!(
            Species::Fox.animation(Clip::IdleHeadlow),
            Clip::IdleHeadlow.index()
        );
        for raptor in [Animal::Chimango, Animal::Carancho] {
            let gliding = standing(Gait::Glide, 0.0);
            let clips: Vec<Clip> = (0..400)
                .map(|k| clip_for(raptor, &gliding, 5, k as f32 * 0.1))
                .collect();
            let flaps = clips.iter().filter(|&&c| c == Clip::Gallop).count();
            // Mayormente planea; aletea un par de segundos de cada tanto.
            assert!((20..=120).contains(&flaps), "{raptor:?}: {flaps}");
            assert!(
                clips
                    .iter()
                    .all(|&c| matches!(c, Clip::Idle | Clip::Gallop))
            );
            let species = Species::of(raptor).unwrap();
            assert_eq!(species.animation(Clip::Gallop), 1, "Flap");
            assert_eq!(clip_speed(Clip::Gallop, species, 0.0), 1.0);
        }
        // El ganado sigue galopando a lo sumo a 1.6 veces su clip.
        assert_eq!(clip_speed(Clip::Gallop, Species::Cow, 50.0), 1.6);
    }

    #[test]
    fn leapers_play_their_jump_over_the_whole_flight() {
        let leapers = [
            (Animal::Zorro, CritterKind::Zorro),
            (Animal::GatoMontes, CritterKind::GatoMontes),
            (Animal::Puma, CritterKind::Puma),
            (Animal::Yaguarete, CritterKind::Yaguarete),
        ];
        for (animal, kind) in leapers {
            let species = Species::of(animal).unwrap();
            let clip = clip_for(animal, &standing(Gait::Leap, 5.0), 3, 0.0);
            assert_eq!(clip, Clip::FenceLeap, "{animal:?}");
            assert_eq!(species.clip_names()[species.animation(clip)], "Fence_Leap");
            // Del impulso a las manos en el suelo, en lo que dura su vuelo,
            // sea cual sea su velocidad, cerca de como se creó.
            let flight = kind.leap().unwrap().flight();
            let rate = clip_speed(clip, species, 5.0);
            assert_eq!(rate, clip_speed(clip, species, 15.0));
            let air = (LEAP_CLIP.1 - LEAP_CLIP.0) * FENCE_LEAP_SECONDS / rate;
            assert!(
                (air - flight).abs() < 1e-4,
                "{animal:?}: {air} s for {flight} s"
            );
            assert!((0.88..=1.05).contains(&rate), "{animal:?}: {rate}");
            println!("{animal:?}: the leap plays {rate:.2} times as made");
        }
        // Los demás no saltan: su clip sigue tal como se creó, y el brinco
        // queda para su presa.
        assert_eq!(Species::Nandu.leap(), None);
        assert_eq!(clip_speed(Clip::GallopJump, Species::Horse, 5.0), 1.0);
        assert_eq!(clip_speed(Clip::GallopJump, Species::Puma, 5.0), 1.0);
        assert_eq!(Species::Horse.animation(Clip::FenceLeap), 6);
    }

    #[test]
    fn the_nandu_plays_its_own_graze_walk_and_run() {
        let species = Species::of(Animal::Nandu).expect("a model");
        let clip = |c| species.clip_names()[species.animation(c)];
        assert_eq!(clip(Clip::Idle), "Idle");
        assert_eq!(clip(Clip::Idle2), "Idle_Look");
        assert_eq!(clip(Clip::Eating), "Graze");
        assert_eq!(clip(Clip::Walk), "Walk");
        assert_eq!(clip(Clip::Gallop), "Run");
        // Huye a 13 m/s sin que su carrera llegue al tope de velocidad.
        let pace = clip_speed(Clip::Gallop, species, 13.0);
        assert!(pace > 1.0 && pace < species.max_pace(), "{pace}");
    }

    #[test]
    fn the_nandu_wears_its_feather_maps_and_alpha_tested_cards() {
        let body = Species::Nandu.skin("Body").expect("body maps");
        let cards = Species::Nandu.skin("Feathers").expect("feather maps");
        assert!(!body.masked && cards.masked);
        for skin in [body, cards] {
            for (name, _) in crate::facade::materials::MAPS {
                let path = root().0.join(skin_path(skin.set, name));
                assert!(path.is_file(), "{}", path.display());
            }
        }
        let maps = || SkinMaps {
            color: Handle::default(),
            normal: Handle::default(),
            orm: Handle::default(),
        };
        let base = StandardMaterial::default();
        let worn = skinned(&base, cards, Some(maps())).expect("drawn");
        assert_eq!(worn.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
        assert_eq!(worn.cull_mode, None);
        assert!(!worn.double_sided, "both faces look out of the body");
        assert!(worn.base_color_texture.is_some() && worn.normal_map_texture.is_some());
        let worn = skinned(&base, body, Some(maps())).expect("drawn");
        assert_eq!(worn.alpha_mode, AlphaMode::Opaque);
        assert_eq!(worn.base_color, Color::WHITE);
        // Sin sus mapas el cuerpo conserva su color liso y las tarjetas no se
        // dibujan.
        assert!(skinned(&base, body, None).is_some_and(|m| m.base_color == base.base_color));
        assert!(skinned(&base, cards, None).is_none());
    }

    #[test]
    fn a_herd_far_off_swaps_to_its_light_mesh_with_a_dithered_fade() {
        let range = Animal::Cow(CowCoat::Angus).range();
        let (full, light) = (light_range(&range, false), light_range(&range, true));
        assert_eq!(full.start_margin, range.start_margin);
        assert_eq!(full.end_margin.end, LIGHT_FROM);
        assert_eq!(
            light.start_margin, full.end_margin,
            "one fades in as the other fades out"
        );
        assert_eq!(light.end_margin, range.end_margin);
        assert!(full.end_margin.start < full.end_margin.end);
    }

    #[test]
    fn feather_cards_draw_only_near_the_camera() {
        assert!(detail_shown(false, 10.0));
        assert!(
            !detail_shown(false, DETAIL_FAR - 1.0),
            "not until well inside"
        );
        assert!(detail_shown(true, DETAIL_FAR - 1.0));
        assert!(!detail_shown(true, DETAIL_FAR + 1.0));
    }

    #[test]
    fn wild_coats_tint_per_species_and_own_coats_wear_their_maps() {
        let flat = |animal, material| match paint(animal, material) {
            Paint::Flat(c) => c.to_srgba(),
            other => panic!("{animal:?} {material}: {other:?}"),
        };
        // El zorro gris y los felinos llevan su pelaje en sus mapas: sus
        // materiales no se tiñen, y los mapas de su cuerpo y de su pelo están
        // en disco.
        for animal in [
            Animal::Zorro,
            Animal::GatoMontes,
            Animal::Puma,
            Animal::Yaguarete,
        ] {
            let species = Species::of(animal).unwrap();
            for material in ["Main", "Hair"] {
                assert_eq!(paint(animal, material), Paint::Keep, "{animal:?}");
                let skin = species.skin(material).expect("own maps");
                assert_eq!(skin.masked, material == "Hair");
                for (name, _) in crate::facade::materials::MAPS {
                    let path = root().0.join(skin_path(skin.set, name));
                    assert!(path.is_file(), "{}", path.display());
                }
            }
        }
        let (chimango, carancho) = (
            flat(Animal::Chimango, "Wings"),
            flat(Animal::Carancho, "Wings"),
        );
        assert!(carancho.red < chimango.red * 0.5, "a dark carancho");
        let face = flat(Animal::Carancho, "Beak");
        assert!(face.red > face.green * 2.0, "a red-orange face");
    }

    #[test]
    fn walking_clips_are_paced_to_the_ground_speed_and_seeded_apart() {
        let slow = clip_speed(Clip::Walk, Species::Cow, 0.5);
        let shy = clip_speed(Clip::Walk, Species::Cow, 1.3);
        assert!(slow < 1.0 && shy > 1.0 && shy > slow, "{slow} {shy}");
        assert_eq!(clip_speed(Clip::Walk, Species::Cow, 0.0), 0.4, "clamped");
        assert_eq!(clip_speed(Clip::Eating, Species::Cow, 0.0), 1.0);
        let offsets: Vec<f32> = (0..20).map(|s| start_offset(s, Clip::Eating)).collect();
        assert!(offsets.iter().all(|o| (0.0..1.0).contains(o)));
        let distinct: std::collections::HashSet<u32> =
            offsets.iter().map(|o| (o * 1000.0) as u32).collect();
        assert!(distinct.len() >= 15, "{offsets:?}");
    }

    #[test]
    fn the_view_is_the_active_camera_drawn_last_whatever_the_query_order() {
        let mut world = World::new();
        let [a, b, c] = [(); 3].map(|_| world.spawn_empty().id());
        let at = |x: f32| GlobalTransform::from_translation(Vec3::X * x);
        let (near, far, off) = (at(1.0), at(500.0), at(9.0));
        let main = Camera {
            order: 1,
            ..default()
        };
        let other = Camera::default();
        let inactive = Camera {
            order: 5,
            is_active: false,
            ..default()
        };
        // Gana el orden activo más alto, listado primero o último.
        let cams = [(a, &far, &other), (b, &near, &main), (c, &off, &inactive)];
        assert_eq!(view_eye(cams), Some(Vec3::X));
        let mut reversed = cams;
        reversed.reverse();
        assert_eq!(view_eye(reversed), Some(Vec3::X));
        // Órdenes iguales: la entidad más baja, en cualquiera de los dos
        // listados.
        let ties = [(b, &far, &other), (a, &near, &other)];
        let lowest = if a < b { Vec3::X } else { Vec3::X * 500.0 };
        assert_eq!(view_eye(ties), Some(lowest));
        let mut reversed = ties;
        reversed.reverse();
        assert_eq!(view_eye(reversed), Some(lowest));
        assert_eq!(view_eye([(c, &off, &inactive)]), None);
    }

    #[test]
    fn a_model_is_in_view_while_any_of_its_meshes_is() {
        let mut world = World::new();
        let [hat, body] = [(); 2].map(|_| world.spawn_empty().id());
        // El sombrero descartado por culling, el cuerpo dibujado: a la vista.
        assert!(any_in_view(&[hat, body], |e| e == body));
        assert!(!any_in_view(&[hat, body], |_| false));
        assert!(any_in_view(&[], |_| false), "nothing to ask");
    }

    #[test]
    fn animation_freezes_far_off_or_out_of_view() {
        assert!(!frozen(false, 50.0, true));
        assert!(frozen(false, 50.0, false), "culled");
        assert!(!frozen(false, FREEZE_FAR - 1.0, true));
        assert!(frozen(false, FREEZE_FAR + 1.0, true));
        // Entre los dos sigue haciendo lo que estaba haciendo.
        let edge = (FREEZE_FAR + RESUME_NEAR) / 2.0;
        assert!(frozen(true, edge, true));
        assert!(!frozen(false, edge, true));
        assert!(!frozen(true, RESUME_NEAR - 1.0, true));
        // Cada animal congelado se sigue dibujando: el rango de los accesorios
        // llega más lejos.
        let props = crate::settlement::ranges()[1].clone();
        assert!(props.end_margin.start > FREEZE_FAR);
    }

    #[test]
    fn models_are_scaled_to_life_and_turned_to_the_heading() {
        let heading = Heading::from_radians(0.7);
        for species in Species::ALL {
            let t = model_transform(species, heading, 0.0);
            // El frente del modelo (+Z) apunta a lo largo del rumbo del cuerpo.
            let front = (t.rotation * Vec3::Z).normalize();
            let along = Quat::from_rotation_y(heading.radians()) * Vec3::X;
            assert!(front.distance(along) < 1e-5, "{species:?}: {front}");
            // Cuesta arriba levanta el hocico con la pendiente, sin ladearse.
            let t = model_transform(species, heading, 0.2);
            let front = t.rotation * Vec3::Z;
            assert!(
                (front.y - 0.2f32.sin()).abs() < 1e-5,
                "{species:?}: {front}"
            );
            let flat = Vec3::new(front.x, 0.0, front.z).normalize();
            assert!(flat.distance(along) < 1e-5, "{species:?}: {front}");
            assert!((t.rotation * Vec3::X).y.abs() < 1e-5, "level across");
            let withers = t.scale.y * species.model_withers();
            assert!((withers - species.withers()).abs() < 1e-4);
        }
        // Un gato montés chico, un puma y un yaguareté grandes; el carancho
        // más grande que el chimango.
        assert!(Species::GatoMontes.withers() < 0.35);
        assert!(Species::Puma.withers() > 2.0 * Species::GatoMontes.withers());
        assert!(Species::Yaguarete.withers() >= Species::Puma.withers());
        assert!(Species::Carancho.withers() > Species::Chimango.withers());
        assert!(Species::Bull.withers() > Species::Cow.withers());
        assert!((1.3..1.5).contains(&Species::Cow.withers()));
        assert!((1.4..1.6).contains(&Species::Horse.withers()));
        // Blender: Y hacia arriba, la cabeza hacia +Z.
        let up = body_point(Species::Cow, Vec3::new(0.0, 0.01, 0.0));
        let head = body_point(Species::Cow, Vec3::new(0.0, 0.0, 0.01));
        assert!(up.y > 0.0 && up.x.abs() < 1e-6 && head.x > 0.0 && head.y.abs() < 1e-6);
    }

    #[test]
    fn coats_tint_per_breed_and_cattle_wear_their_baked_coat_maps() {
        let tint = |animal, material| paint(animal, material);
        let srgb = |p: Paint| match p {
            Paint::Flat(c) | Paint::Map(c) => c.to_srgba(),
            other => panic!("{other:?}"),
        };
        // Sin su mapa, el cuero de un Hereford es colorado, el de un Angus
        // negro, el de un overo blanco y el de un rosillo gris acerado, nunca
        // rosado.
        let red = srgb(tint(Animal::Cow(CowCoat::Hereford), "Main"));
        assert!(
            red.red > red.green * 1.8 && red.red > red.blue * 2.0,
            "{red:?}"
        );
        assert!(srgb(tint(Animal::Cow(CowCoat::Angus), "Main")).red < 0.12);
        assert!(srgb(tint(Animal::Cow(CowCoat::Overo), "Main")).green > 0.8);
        let roan = srgb(tint(Animal::Cow(CowCoat::Roan), "Main"));
        assert!(
            roan.blue > roan.red && roan.blue - roan.red < 0.16,
            "{roan:?}"
        );
        for coat in CowCoat::ALL {
            assert!(matches!(tint(Animal::Cow(coat), "Main"), Paint::Map(_)));
            // La ubre es piel lampiña, nunca del color del pelo (salvo el
            // Angus, cuya piel es oscura como su pelo).
            let skin = srgb(tint(Animal::Cow(coat), "Main_Light"));
            assert!(skin.red > skin.blue, "{coat:?}: {skin:?}");
            // Cada pelaje está horneado en el mapa del cuerpo de la vaca y del
            // toro.
            for set in ["cow_body", "bull_body"] {
                let path = root().0.join(skin_path(set, coat_map_name(coat)));
                assert!(path.is_file(), "{}", path.display());
            }
        }
        // La borla del Hereford es blanca; el Angus es mocho.
        assert!(srgb(tint(Animal::Cow(CowCoat::Hereford), "Hair")).blue > 0.8);
        assert_eq!(tint(Animal::Cow(CowCoat::Angus), "Horns"), Paint::Hidden);
        assert_eq!(
            tint(Animal::Cow(CowCoat::Hereford), "Eye_Black"),
            Paint::Keep
        );
        // El toro lleva el pelaje de su raza como las vacas.
        for material in ["Main", "Main_Light", "Muzzle", "Hooves", "Horns", "Hair"] {
            for coat in CowCoat::ALL {
                assert_eq!(
                    tint(Animal::Bull(coat), material),
                    tint(Animal::Cow(coat), material)
                );
            }
        }
        // Los caballos toman los pelajes criollos: el cuerpo en colores por
        // vertex, la crin en su color.
        for coat in Coat::ALL {
            assert_eq!(tint(Animal::Horse(coat), "Main"), Paint::Vertex);
            let m = coat.mane();
            assert_eq!(
                tint(Animal::Horse(coat), "Hair"),
                Paint::Flat(Color::srgb_u8(m[0], m[1], m[2]))
            );
            let p = Vec3::new(0.0, 1.1, 0.4);
            let c = coat.color_at(p, Vec3::Z);
            let want = Color::srgb_u8(c[0], c[1], c[2]).to_linear().to_f32_array();
            assert_eq!(coat_color_at(Animal::Horse(coat), p, Vec3::Z), want);
        }
    }

    /// Los modelos incluidos, cargados en modo headless.
    fn loaded() -> (App, Vec<Handle<bevy::gltf::Gltf>>) {
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin);
        app.finish();
        app.cleanup();
        let server = app.world().resource::<AssetServer>().clone();
        let mut handles = Vec::new();
        for species in Species::ALL {
            let path = species.path();
            let scene = gltf::load_scene(&server, Some(&root()), &path).expect(&path);
            assert_eq!(settle(&mut app, &scene), ModelState::Ready, "{path}");
            let handle: Handle<bevy::gltf::Gltf> = server.load(path.clone());
            while !app
                .world()
                .resource::<Assets<bevy::gltf::Gltf>>()
                .contains(&handle)
            {
                app.update();
            }
            handles.push(handle);
        }
        (app, handles)
    }

    #[test]
    fn every_model_loads_skinned_with_its_clips_at_life_size() {
        let (app, handles) = loaded();
        let world = app.world();
        for (species, handle) in Species::ALL.into_iter().zip(handles) {
            let gltf = world
                .resource::<Assets<bevy::gltf::Gltf>>()
                .get(&handle)
                .unwrap();
            // Los clips que reproduce el juego, en el orden en que los
            // reproduce.
            let names = species.clip_names();
            assert_eq!(gltf.animations.len(), names.len(), "{species:?}");
            for clip in Clip::LOADED {
                let index = species.animation(clip);
                let named = &gltf.named_animations[names[index]];
                assert_eq!(*named, gltf.animations[index], "{species:?} {clip:?}");
                let clip = world
                    .resource::<Assets<AnimationClip>>()
                    .get(named)
                    .unwrap();
                assert!(clip.duration() > 0.3, "{species:?}");
            }
            // El brinco y el salto de un alambrado del zorro y de los felinos
            // duran lo que se supone al reproducirlos.
            if species.leap().is_some() {
                for (name, seconds) in [
                    ("Gallop_Jump", POUNCE_SECONDS),
                    ("Fence_Leap", FENCE_LEAP_SECONDS),
                ] {
                    let jump = &gltf.named_animations[name];
                    let jump = world.resource::<Assets<AnimationClip>>().get(jump).unwrap();
                    assert!(
                        (jump.duration() - seconds).abs() < 0.03,
                        "{species:?} {name}: {}",
                        jump.duration()
                    );
                }
            }
            assert_eq!(gltf.skins.len(), 1);
            let skin = world
                .resource::<Assets<bevy::gltf::GltfSkin>>()
                .get(&gltf.skins[0])
                .unwrap();
            let joints = match species {
                // Los 50 de Quaternius más la crin, el copete, la cola, la
                // cara y los seis músculos (`tools/horse.py` `MUSCLES`).
                Species::Horse => 68,
                // Los nombres de Quaternius con dos huesos por oreja, la
                // mandíbula y nueve de cola (`tools/fox.py`, `tools/felid.py`).
                Species::Fox | Species::GatoMontes | Species::Puma | Species::Yaguarete => 49,
                Species::Chimango | Species::Carancho => 30,
                // Los 50 de Quaternius menos los que no tiene un vacuno,
                // más las orejas, la cara, la cola, la papada, la ubre (el
                // escroto) y los seis músculos (`tools/cattle.py`).
                Species::Cow | Species::Bull => 58,
                Species::Nandu => 20,
            };
            assert_eq!(skin.joints.len(), joints, "{species:?}");
            // Su cuerpo está a tamaño real en el marco del cuerpo: la altura del
            // lomo entre la grupa y la mitad del largo (sin la cabeza, el
            // cuello ni la cola), o la envergadura de un ave.
            let mut points = Vec::new();
            let mut triangles = 0;
            let mut names: Vec<&str> = gltf.named_materials.keys().map(|k| &**k).collect();
            names.sort();
            // La malla completa (la liviana, si la tiene, se mira aparte).
            let full = gltf
                .named_meshes
                .iter()
                .find(|(name, _)| !name.ends_with(LIGHT_SUFFIX))
                .map_or(&gltf.meshes[0], |(_, h)| h);
            let mesh = world
                .resource::<Assets<bevy::gltf::GltfMesh>>()
                .get(full)
                .unwrap();
            for primitive in &mesh.primitives {
                let m = world
                    .resource::<Assets<Mesh>>()
                    .get(&primitive.mesh)
                    .unwrap();
                triangles += m.indices().map_or(0, |i| i.len() / 3);
                let Some(VertexAttributeValues::Float32x3(p)) =
                    m.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("positions")
                };
                points.extend(p.iter().map(|p| body_point(species, Vec3::from(*p))));
            }
            let (lo, hi) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), b| {
                (lo.min(b.x), hi.max(b.x))
            });
            let quarter = (hi - lo) / 4.0;
            let top = if species.bird() {
                let span = |f: fn(f32, f32) -> f32, from| points.iter().map(|b| b.z).fold(from, f);
                span(f32::max, f32::MIN) - span(f32::min, f32::MAX)
            } else {
                points
                    .iter()
                    .filter(|b| (lo + quarter..lo + 2.0 * quarter).contains(&b.x))
                    .map(|b| b.y)
                    .fold(0.0, f32::max)
            };
            println!(
                "{species:?}: {triangles} triangles, back {top:.2} m ({:.4} model), materials {names:?}",
                top / species.scale()
            );
            let budget = match species {
                Species::Nandu => 6_500,
                Species::Horse => 12_500,
                // La vaca 8.192, el toro 8.792.
                Species::Cow | Species::Bull => 9_000,
                // 12.524: el cuerpo unos 5.200, las tarjetas de pelo y los ojos.
                Species::Fox => 13_000,
                // Gato montés 11.618, puma 11.326, yaguareté 11.414: el cuerpo, las
                // tarjetas de pelo y los ojos (más la malla liviana, aparte).
                Species::GatoMontes | Species::Puma | Species::Yaguarete => 12_000,
                _ => 2_500,
            };
            assert!(triangles <= budget, "{species:?}: {triangles}");
            let light = gltf
                .named_meshes
                .iter()
                .filter(|(name, _)| name.ends_with(LIGHT_SUFFIX))
                .map(|(_, h)| {
                    world
                        .resource::<Assets<bevy::gltf::GltfMesh>>()
                        .get(h)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            if matches!(
                species,
                Species::Cow
                    | Species::Bull
                    | Species::GatoMontes
                    | Species::Puma
                    | Species::Yaguarete
            ) {
                // Una malla liviana para las tropas y los felinos lejanos: la
                // piel sola, con los materiales del cuerpo, unas seis veces
                // más liviana o más.
                let [light] = light[..] else {
                    panic!("{species:?}: {} light meshes", light.len())
                };
                let mut light_triangles = 0;
                for primitive in &light.primitives {
                    let m = world
                        .resource::<Assets<Mesh>>()
                        .get(&primitive.mesh)
                        .unwrap();
                    light_triangles += m.indices().map_or(0, |i| i.len() / 3);
                    assert!(m.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
                    assert!(m.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
                }
                println!("{species:?}: light mesh {light_triangles} triangles");
                assert!(light_triangles <= 1_500, "{species:?}: {light_triangles}");
                assert!(light_triangles * 5 < triangles);
            } else {
                assert!(light.is_empty(), "{species:?}");
            }
            if matches!(
                species,
                Species::Nandu
                    | Species::Horse
                    | Species::Cow
                    | Species::Bull
                    | Species::Fox
                    | Species::GatoMontes
                    | Species::Puma
                    | Species::Yaguarete
            ) {
                // Sus mapas necesitan UV y tangentes en cada primitiva.
                let m = world.resource::<Assets<Mesh>>();
                for primitive in &mesh.primitives {
                    let p = m.get(&primitive.mesh).unwrap();
                    assert!(p.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
                    assert!(p.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
                }
            }
            let materials: &[&str] = match species {
                Species::Cow | Species::Bull => {
                    &["Main", "Main_Light", "Muzzle", "Hooves", "Horns", "Hair"]
                }
                Species::Horse => &[
                    "Main",
                    "Hair",
                    "Main_Dark",
                    "Main_Light",
                    "Muzzle",
                    "Hooves",
                    "Eye_Black",
                    "Eye_White",
                ],
                Species::Fox | Species::GatoMontes | Species::Puma | Species::Yaguarete => {
                    &["Main", "Hair", "Eye_Black", "Eye_Iris"]
                }
                Species::Chimango | Species::Carancho => &["Wings", "Head", "Beak", "Claws"],
                Species::Nandu => &["Body", "Feathers"],
            };
            for material in materials {
                assert!(names.contains(material), "{species:?}: {names:?}");
            }
            let want = species.withers();
            assert!(
                (want * 0.9..want * 1.1).contains(&top),
                "{species:?}: back at {top}"
            );
        }
    }

    /// El plugin de fauna con los modelos incluidos, headless.
    fn fauna_app() -> App {
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .add_plugins(super::super::FaunaPlugin);
        app.finish();
        app.cleanup();
        app
    }

    /// Actualiza `app` hasta que cada modelo tenga su rig; hace panic después
    /// de 10 s.
    fn until_rigged(app: &mut App, count: usize) {
        for _ in 0..2000 {
            app.update();
            let mut q = app.world_mut().query::<&LivestockRig>();
            if q.iter(app.world()).count() == count {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the models never spawned");
    }

    #[test]
    fn a_leaping_puma_fades_into_its_jump_once_from_the_take_off() {
        let mut app = fauna_app();
        let running = AnimalPose {
            gait: Gait::Run,
            speed: 10.0,
            ..default()
        };
        let puma = app
            .world_mut()
            .spawn((Animal::Puma, running, AnimalSeed(3), Transform::default()))
            .id();
        until_rigged(&mut app, 1);
        app.update();
        app.world_mut().get_mut::<AnimalPose>(puma).unwrap().gait = Gait::Leap;
        app.update();
        let world = app.world_mut();
        let rig = world
            .query::<&LivestockRig>()
            .single(world)
            .unwrap()
            .clone();
        assert_eq!(rig.clip, Some(Clip::FenceLeap));
        let models = world.resource::<LivestockModels>();
        let model = models.get(Species::Puma).unwrap();
        let (jump, gallop) = (
            model.nodes[Clip::FenceLeap.index()],
            model.nodes[Clip::Gallop.index()],
        );
        let player = world.get::<AnimationPlayer>(rig.player).unwrap();
        let active = player.animation(jump).expect("the jump plays");
        // Una sola vez, desde el impulso, estirado sobre el vuelo.
        assert_eq!(
            active.repeat_mode(),
            bevy::animation::RepeatAnimation::Never
        );
        let start = LEAP_CLIP.0 * FENCE_LEAP_SECONDS;
        assert!(
            (active.seek_time() - start).abs() < 0.1,
            "{} s",
            active.seek_time()
        );
        assert_eq!(
            active.speed(),
            clip_speed(Clip::FenceLeap, Species::Puma, 10.0)
        );
        // El galope se funde en vez de cortarse.
        assert!(player.animation(gallop).is_some(), "the gallop fades out");
        let transitions = world.get::<AnimationTransitions>(rig.player).unwrap();
        assert_eq!(transitions.get_main_animation(), Some(jump));
    }

    #[test]
    fn a_nandu_drops_its_feather_cards_far_off() {
        let mut app = fauna_app();
        app.world_mut().spawn((Animal::Nandu, AnimalSeed(3)));
        until_rigged(&mut app, 1);
        app.update();
        let world = app.world_mut();
        let rig = world
            .query::<&LivestockRig>()
            .single(world)
            .unwrap()
            .clone();
        assert!(!rig.detail.is_empty(), "its feather cards");
        let mut cards = Vec::new();
        for &card in &rig.detail {
            let material = world.get::<MeshMaterial3d<StandardMaterial>>(card).unwrap();
            let m = world
                .resource::<Assets<StandardMaterial>>()
                .get(&material.0)
                .unwrap();
            assert_eq!(m.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
            cards.push(card);
        }
        let shown = |world: &World| {
            cards
                .iter()
                .all(|&c| world.get::<Visibility>(c) != Some(&Visibility::Hidden))
        };
        let camera = world
            .spawn((Camera3d::default(), Transform::from_xyz(0.0, 2.0, 8.0)))
            .id();
        app.update();
        assert!(shown(app.world()), "near");
        let far = Transform::from_xyz(0.0, 2.0, DETAIL_FAR + 10.0);
        app.world_mut().entity_mut(camera).insert(far);
        app.update();
        app.update();
        assert!(!shown(app.world()), "far");
    }

    #[test]
    fn a_corral_horse_wears_its_coat_maps_and_a_mane_tinted_hair_mask_at_any_distance() {
        let mut app = fauna_app();
        let coat = Coat::Tordillo;
        app.world_mut().spawn((Animal::Horse(coat), AnimalSeed(3)));
        until_rigged(&mut app, 1);
        app.update();
        let world = app.world_mut();
        let rig = world
            .query::<&LivestockRig>()
            .single(world)
            .unwrap()
            .clone();
        // La crin y la cola son su silueta: no se quitan de lejos.
        assert!(rig.detail.is_empty(), "{:?}", rig.detail);
        let mane = coat.mane();
        let mut seen = Vec::new();
        let mut parts = world.query::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &GltfMaterialName,
        )>();
        for (mesh, material, name) in parts.iter(world) {
            let m = world
                .resource::<Assets<StandardMaterial>>()
                .get(&material.0)
                .unwrap();
            let coloured = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_COLOR)
                .is_some();
            match name.0.as_str() {
                "Main" => {
                    assert!(m.base_color_texture.is_some() && m.normal_map_texture.is_some());
                    assert_eq!(m.base_color, Color::WHITE);
                    assert_eq!(m.alpha_mode, AlphaMode::Opaque);
                    assert!(coloured, "the coat in its vertex colours");
                }
                "Hair" => {
                    assert!(m.base_color_texture.is_some());
                    assert_eq!(m.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
                    assert_eq!(m.cull_mode, None);
                    assert!(m.double_sided, "the backs of the strands lit");
                    assert_eq!(
                        m.base_color,
                        Color::srgb_u8(mane[0], mane[1], mane[2]),
                        "tinted with the mane"
                    );
                    assert!(!coloured);
                }
                // Las partes de un solo color, con los mismos mapas.
                "Main_Dark" | "Main_Light" | "Muzzle" | "Hooves" => {
                    assert!(m.base_color_texture.is_some(), "{}", name.0);
                    assert_eq!(m.alpha_mode, AlphaMode::Opaque);
                    assert_ne!(m.base_color, Color::WHITE, "{}", name.0);
                    assert!(!coloured);
                }
                _ => assert!(m.base_color_texture.is_none(), "{}", name.0),
            }
            seen.push(name.0.clone());
        }
        assert!(seen.contains(&"Main".to_string()) && seen.contains(&"Hair".to_string()));
    }

    #[test]
    fn animals_spawn_tinted_and_animated_and_freeze_far_off() {
        let mut app = fauna_app();
        let grazing = AnimalPose {
            gait: Gait::Graze,
            ..default()
        };
        let herd = [
            Animal::Cow(CowCoat::Angus),
            Animal::Cow(CowCoat::Angus),
            Animal::Cow(CowCoat::Overo),
            Animal::Bull(CowCoat::Overo),
            Animal::Horse(Coat::Tordillo),
        ];
        for (k, animal) in herd.into_iter().enumerate() {
            app.world_mut().spawn((
                animal,
                grazing,
                AnimalSeed(k as u32),
                Transform::from_xyz(k as f32 * 4.0, 0.0, 0.0),
            ));
        }
        until_rigged(&mut app, herd.len());
        app.update();
        let world = app.world_mut();
        // Cada reproductor tiene el grafo de su especie y reproduce su clip.
        let mut rigs = world.query::<(&LivestockRig, &LivestockModel)>();
        let rigs: Vec<(LivestockRig, Species)> = rigs
            .iter(world)
            .map(|(r, m)| (r.clone(), m.species))
            .collect();
        for (rig, _) in &rigs {
            assert!(!rig.frozen);
            assert!(rig.clip.is_some());
            let player = world.get::<AnimationPlayer>(rig.player).unwrap();
            assert_eq!(player.playing_animations().count(), 1);
            assert!(world.get::<AnimationGraphHandle>(rig.player).is_some());
        }
        let cows = rigs.iter().filter(|r| r.1 == Species::Cow).count();
        assert_eq!(cows, 3);
        // Las meshes comparten el rango de los accesorios (los vacunos lo
        // reparten entre la malla completa, cerca, y la liviana, lejos), y las
        // copias por pelaje se crean una vez por pelaje: los dos Angus
        // comparten las suyas.
        let props = crate::settlement::ranges()[1].clone();
        let angus_map =
            world.resource::<CoatCache>().coat_maps[&("cow_body", CowCoat::Angus)].clone();
        let mut parts = world.query::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &GltfMaterialName,
            &VisibilityRange,
            Option<&Visibility>,
            &GltfMeshName,
        )>();
        let mut angus_main = std::collections::HashSet::new();
        let (mut horns_hidden, mut light, mut coloured_meshes) = (0, 0, 0);
        for (mesh, material, name, range, visibility, mesh_name) in parts.iter(world) {
            let cattle = mesh_name.0.starts_with("Cow");
            let is_light = mesh_name.0.ends_with(LIGHT_SUFFIX);
            light += usize::from(is_light);
            let want = if cattle {
                light_range(&props, is_light)
            } else {
                props.clone()
            };
            assert!(*range == want, "{}: a new range", mesh_name.0);
            let m = world
                .resource::<Assets<StandardMaterial>>()
                .get(&material.0)
                .unwrap();
            if name.0 == "Horns" && visibility == Some(&Visibility::Hidden) {
                horns_hidden += 1;
                continue;
            }
            assert_eq!(m.metallic, 0.0, "{}", name.0);
            if cattle && name.0 == "Main" {
                // El pelaje horneado en su mapa, sin teñir.
                assert_eq!(m.base_color, Color::WHITE);
                assert!(m.base_color_texture.is_some() && m.normal_map_texture.is_some());
                if m.base_color_texture.as_ref() == Some(&angus_map) {
                    angus_main.insert(material.0.id());
                }
            }
            if cattle && name.0 == "Hair" {
                assert_eq!(m.alpha_mode, AlphaMode::Mask(LEAF_CUTOFF));
                assert!(!is_light, "no hair cards far off");
            }
            let coloured = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_COLOR)
                .is_some();
            assert!(!coloured || !cattle && name.0 == "Main", "{}", mesh_name.0);
            coloured_meshes += usize::from(coloured);
        }
        assert_eq!(angus_main.len(), 1, "one shared Angus hide");
        // Los Angus son mochos, de cerca y de lejos.
        assert_eq!(horns_hidden, 4, "the Angus are polled");
        assert!(light >= 4, "every head of cattle has its light mesh");
        let (materials, meshes) = world.resource::<CoatCache>().len();
        // Dos pelajes de vaca, un toro overo y un caballo, unos pocos
        // materiales cada uno; solo el cuerpo del caballo lleva colores por
        // vertex.
        assert!(materials <= 4 * 8, "{materials}");
        assert_eq!((meshes, coloured_meshes), (1, 1));

        // Una cámara lejos: cada animal se congela y se le quita su grafo.
        world.spawn((Camera3d::default(), Transform::from_xyz(0.0, 2.0, 500.0)));
        app.update();
        app.update();
        let world = app.world_mut();
        let mut rigs = world.query::<&LivestockRig>();
        let rigs: Vec<LivestockRig> = rigs.iter(world).cloned().collect();
        for rig in rigs {
            assert!(rig.frozen);
            assert!(world.get::<AnimationGraphHandle>(rig.player).is_none());
        }
    }
}
