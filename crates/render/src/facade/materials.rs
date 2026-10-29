//! Los materiales PBR de los asentamientos y los pueblos: un material
//! compartido por superficie.
//!
//! Las superficies texturizadas llevan los conjuntos CC0 de
//! `assets/textures/buildings/<set>/` (ambientCG y Poly Haven, más nuestra
//! propia paja; ver el CREDITS.md de esa carpeta). El juego carga los archivos
//! KTX2 listos para la GPU de cada conjunto a través del [`AssetServer`] con un
//! sampler de repetición anisotrópico: `color.ktx2` (sRGB), `normal_gl.ktx2`
//! (+Y de OpenGL, como los lee Bevy) y `orm.ktx2`, con la oclusión en rojo, la
//! rugosidad en verde y la metalicidad completa en azul (el `metallic` propio
//! del material la escala por superficie). `tools/textures_to_ktx2.sh` los
//! genera a partir de las fuentes JPEG que están al lado: UASTC con mips
//! horneados, que Bevy transcodifica al cargar al formato de bloques de la GPU
//! (BC7 en escritorio), un cuarto de la memoria de RGBA8.
//!
//! Algunas superficies llevan el mismo conjunto con otro tinte (los encalados y
//! el revoque gris de los pueblos sobre `plaster_003`, las cinco [`Paint`]
//! sobre `painted_plaster_wall`); el servidor de assets carga los mapas de ese
//! conjunto una sola vez.
//!
//! Los conjuntos de Poly Haven del 2026-10-25 (revoque pintado, ladrillo
//! encalado, revoque emparchado sobre ladrillo rojo, una franja beige y piso de
//! ladrillo) están escaneados más oscuros que la superficie de 1919 que
//! representan. El color de su material eleva el albedo medio de cada escaneo
//! ([`Look::Lifted`]) hasta un objetivo apagado, así que un texel medio cae en
//! el color de la pintura mientras el escaneo conserva sus manchas y juntas.
//!
//! La vía permanente del ferrocarril lleva tres conjuntos CC0 más de Poly Haven
//! (`gravel_road` de Amal Kumar, `rough_wood` de Rob Tuytel y `rust_coarse_01`
//! de Dimitrios Savva y Rico Cilliers; los CREDITS.md de sus carpetas y el
//! CREDITS.md de la raíz les dan crédito): el balasto, los durmientes y los
//! rieles (ver [`crate::rail`]). El escaneo del balasto se eleva hasta una
//! grava gris; la cabeza del riel lleva el escaneo del riel con un acabado más
//! liso y más metálico, la banda de rodadura.
//!
//! El material rodante lleva dos de esos conjuntos y un cuarto, `metal_plate`
//! (una chapa antideslizante de diamante de Rob Tuytel, Poly Haven, CC0): ver
//! [`Livery`].
//!
//! Sin un servidor de assets (tests headless) o cuando faltan los archivos de
//! un conjunto, la superficie vuelve a un color liso del mismo aspecto.

use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormatSupport, ImageAddressMode, ImageFilterMode, ImageLoaderSettings,
    ImageSampler, ImageSamplerDescriptor,
};
use bevy::prelude::*;

use crate::AssetRoot;

/// Dónde viven los conjuntos de texturas de edificios, bajo el [`AssetRoot`].
pub const TEXTURE_DIR: &str = "textures/buildings";

/// De qué está hecha una pieza de un asentamiento o de un edificio de pueblo.
/// Cada una es un material compartido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Surface {
    /// Paredes de adobe: revoque de barro (`clay_plaster`).
    Adobe,
    /// Revoque de cal encalado (`plaster_003`).
    Whitewash,
    /// Ladrillo visto (`brick_wall_02`), y el ladrillo del brocal de un pozo.
    Brick,
    /// Techado acanalado viejo con vetas de óxido (`corrugated_iron_02`).
    OldIron,
    /// Chapa acanalada galvanizada brillante (`corrugated_iron`): tanques.
    Galvanised,
    /// Tablas curtidas (`brown_planks_05`): puertas, postigos, travesaños,
    /// postes.
    Planks,
    /// Techos de paja y parvas (`thatch`, generada por nosotros).
    Thatch,
    /// El barro seco de las huellas (`brown_mud_dry`), difuminado en el pasto.
    Mud,
    /// Patios pisoteados (`dry_ground_01`), también difuminados.
    Yard,
    /// El interior oscuro de una puerta o ventana abierta.
    Recess,
    /// Acero pintado: la torre del molino, los postes de hierro de la galería.
    Steel,
    /// Chapa galvanizada fina: las aspas y la veleta del molino.
    Sheet,
    /// Agua quieta en un tanque, un bebedero o un pozo.
    Water,
    /// Un encalado ocre cálido sobre revoque (`plaster_003`, teñido): frentes
    /// del pueblo.
    Lime,
    /// Revoque gris (`plaster_003`, teñido): zócalos, alféizares y las molduras
    /// de los frentes de ladrillo; los cordones de las veredas.
    Stucco,
    /// Revoque liso pintado en una de las [`Paint`] apagadas de 1919
    /// (`painted_plaster_wall`): frentes del pueblo.
    Painted(Paint),
    /// Ladrillo bajo un encalado gastado (`whitewashed_brick`).
    WashedBrick,
    /// Ladrillo rojo viejo con el revoque descascarándose a parches
    /// (`red_brick_plaster_patch_02`): frentes más pobres o más viejos.
    PatchedBrick,
    /// Una franja revocada beige (`beige_wall_002`, una tira de 4:1): los
    /// zócalos de los frentes pintados.
    Band,
    /// Piso de ladrillo plano (`brick_floor_003`): las veredas.
    Paving,
    /// Tierra apisonada (`dirt_path`, opaca, a diferencia de
    /// [`Surface::Yard`]): los senderos de la plaza de un pueblo pequeño o de
    /// una aldea, y las veredas de tierra.
    Earth,
    /// Bronce viejo y curtido con una pátina verdosa (`bronze`): las placas de
    /// los monumentos de las plazas.
    Bronze,
    /// Balasto de la vía (`gravel_road`, Poly Haven, Amal Kumar), elevado a una
    /// grava gris: el lecho sobre el que se apoya la vía.
    Ballast,
    /// Durmientes curtidos (`rough_wood`, Poly Haven, Rob Tuytel): la veta
    /// corre a lo largo de cada durmiente.
    Sleeper,
    /// El patín y el alma de los rieles (`rust_coarse_01`, Poly Haven,
    /// Dimitrios Savva y Rico Cilliers): acero oxidado.
    RailSteel,
    /// La banda de rodadura de los rieles (otra vez `rust_coarse_01`): el mismo
    /// escaneo, más liso y más metálico donde lo pulen las ruedas.
    RailHead,
    /// El acero y las tablas del material rodante, un escaneo bajo varias
    /// pinturas (ver [`Livery`]).
    Livery(Livery),
}

/// Con qué está pintado el tren. Cada una es un escaneo bajo un tinte: la chapa
/// antideslizante tal como es, el esmalte negro y el rojo oscuro sobre el óxido
/// (`rust_coarse_01`), el granate, el barniz y el rojo óxido sobre las tablas
/// grises (`brown_planks_05`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Livery {
    /// Chapa antideslizante (`metal_plate`, Poly Haven, Rob Tuytel):
    /// plataformas laterales, planchas del maquinista, estribos y plataformas.
    Tread,
    /// Esmalte negro gastado: la locomotora, el ténder y los bastidores.
    Black,
    /// Rojo oscuro: los testeros.
    BufferRed,
    /// El granate de los coches, entablado.
    Maroon,
    /// Madera marrón barnizada: el revestimiento y los cielorrasos de los
    /// coches.
    Varnish,
    /// Rojo óxido, las tablas de los vagones de carga.
    Oxide,
}

impl Livery {
    /// Todas las libreas, en orden.
    pub const ALL: [Livery; 6] = [
        Livery::Tread,
        Livery::Black,
        Livery::BufferRed,
        Livery::Maroon,
        Livery::Varnish,
        Livery::Oxide,
    ];
}

/// Los colores apagados con los que se pinta un frente de pueblo de 1919
/// (`odd/setting-1919.md`: terrosos, polvorientos, nunca brillantes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Paint {
    LimeWhite,
    Ochre,
    PaleRose,
    GreyBlue,
    Terracotta,
}

impl Paint {
    /// Todas las pinturas, en orden.
    pub const ALL: [Paint; 5] = [
        Paint::LimeWhite,
        Paint::Ochre,
        Paint::PaleRose,
        Paint::GreyBlue,
        Paint::Terracotta,
    ];

    /// Su color medio en la pared (sRGB).
    pub fn albedo(self) -> [f32; 3] {
        match self {
            Self::LimeWhite => [0.86, 0.84, 0.79],
            Self::Ochre => [0.80, 0.67, 0.46],
            Self::PaleRose => [0.80, 0.67, 0.62],
            Self::GreyBlue => [0.67, 0.72, 0.74],
            Self::Terracotta => [0.74, 0.53, 0.41],
        }
    }
}

/// Cómo se elige el color del material de una superficie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Look {
    /// Multiplica el escaneo: sRGB, como mucho blanco.
    Tint([f32; 3]),
    /// Eleva el escaneo, cuyo albedo medio es `scan` (lineal, medido sobre su
    /// `color.jpg`), para que su media pase a ser `target` (sRGB).
    Lifted { scan: [f32; 3], target: [f32; 3] },
}

/// El albedo lineal medio del `color.jpg` de cada escaneo nuevo, medido con
/// `magick color.jpg -colorspace RGB -format '%[fx:mean.r] %[fx:mean.g] %[fx:mean.b]' info:`.
const PAINTED_PLASTER_SCAN: [f32; 3] = [0.407, 0.366, 0.351];
const WASHED_BRICK_SCAN: [f32; 3] = [0.385, 0.326, 0.269];
const PATCHED_BRICK_SCAN: [f32; 3] = [0.243, 0.131, 0.066];
const BAND_SCAN: [f32; 3] = [0.227, 0.158, 0.092];
const PAVING_SCAN: [f32; 3] = [0.115, 0.081, 0.058];
const DIRT_PATH_SCAN: [f32; 3] = [0.126, 0.086, 0.049];
const GRAVEL_SCAN: [f32; 3] = [0.193, 0.099, 0.051];

/// Lo máximo que un material [`Look::Lifted`] puede multiplicar su escaneo, en
/// cualquier canal. Las elevaciones incluidas quedan por debajo y no llevan
/// ningún texel de sus escaneos más allá del blanco (un test lee los escaneos),
/// así que el límite solo frena un escaneo más oscuro que se agregue más
/// adelante: pasado el límite, el resto de la elevación se incorpora al
/// objetivo, que queda como un tono más oscuro de sí mismo en lugar de quemar
/// los texels brillantes del escaneo.
pub const MAX_LIFT: f32 = 2.5;

/// La ganancia lineal por canal que eleva un escaneo de albedo lineal medio
/// `scan` hasta el `target` sRGB, reducida en bloque, conservando el tono, para
/// que ningún canal supere [`MAX_LIFT`].
pub fn lift(scan: [f32; 3], target: [f32; 3]) -> [f32; 3] {
    let t = Color::srgb(target[0], target[1], target[2]).to_linear();
    let gain = [t.red / scan[0], t.green / scan[1], t.blue / scan[2]];
    let steepest = gain.into_iter().fold(0.0, f32::max);
    let fold = (MAX_LIFT / steepest).min(1.0);
    gain.map(|g| g * fold)
}

impl Surface {
    /// Todas las superficies, en orden.
    pub const ALL: [Surface; 36] = [
        Surface::Adobe,
        Surface::Whitewash,
        Surface::Brick,
        Surface::OldIron,
        Surface::Galvanised,
        Surface::Planks,
        Surface::Thatch,
        Surface::Mud,
        Surface::Yard,
        Surface::Recess,
        Surface::Steel,
        Surface::Sheet,
        Surface::Water,
        Surface::Lime,
        Surface::Stucco,
        Surface::Painted(Paint::LimeWhite),
        Surface::Painted(Paint::Ochre),
        Surface::Painted(Paint::PaleRose),
        Surface::Painted(Paint::GreyBlue),
        Surface::Painted(Paint::Terracotta),
        Surface::WashedBrick,
        Surface::PatchedBrick,
        Surface::Band,
        Surface::Paving,
        Surface::Earth,
        Surface::Bronze,
        Surface::Ballast,
        Surface::Sleeper,
        Surface::RailSteel,
        Surface::RailHead,
        Surface::Livery(Livery::Tread),
        Surface::Livery(Livery::Black),
        Surface::Livery(Livery::BufferRed),
        Surface::Livery(Livery::Maroon),
        Surface::Livery(Livery::Varnish),
        Surface::Livery(Livery::Oxide),
    ];

    /// El conjunto de texturas que lleva, si tiene uno.
    pub fn set(self) -> Option<&'static str> {
        Some(match self {
            Self::Adobe => "clay_plaster",
            Self::Whitewash | Self::Lime | Self::Stucco => "plaster_003",
            Self::Brick => "brick_wall_02",
            Self::OldIron => "corrugated_iron_02",
            Self::Galvanised => "corrugated_iron",
            Self::Planks => "brown_planks_05",
            Self::Thatch => "thatch",
            Self::Mud => "brown_mud_dry",
            Self::Yard => "dry_ground_01",
            Self::Earth => "dirt_path",
            Self::Painted(_) => "painted_plaster_wall",
            Self::WashedBrick => "whitewashed_brick",
            Self::PatchedBrick => "red_brick_plaster_patch_02",
            Self::Band => "beige_wall_002",
            Self::Paving => "brick_floor_003",
            Self::Bronze => "bronze",
            Self::Ballast => "gravel_road",
            Self::Sleeper => "rough_wood",
            Self::RailSteel | Self::RailHead => "rust_coarse_01",
            Self::Livery(Livery::Tread) => "metal_plate",
            Self::Livery(Livery::Black | Livery::BufferRed) => "rust_coarse_01",
            Self::Livery(Livery::Maroon | Livery::Varnish | Livery::Oxide) => "brown_planks_05",
            Self::Recess | Self::Steel | Self::Sheet | Self::Water => return None,
        })
    }

    /// Metros por repetición de la textura a lo largo de `u`: el ancho del
    /// mundo que cubre una textura.
    pub fn tile(self) -> f32 {
        self.tiles().x
    }

    /// Metros por repetición de la textura a lo largo de `u` y `v`. Solo la
    /// tira de 1024x256 de la franja beige cubre de ancho cuatro veces su
    /// altura.
    pub fn tiles(self) -> Vec2 {
        let tile = match self {
            Self::Brick | Self::Planks | Self::Thatch | Self::WashedBrick => 2.0,
            Self::OldIron | Self::Galvanised | Self::Paving => 2.0,
            // Sus ladrillos son más grandes en el escaneo: hiladas de unos
            // 7 cm.
            Self::PatchedBrick => 1.4,
            Self::Adobe | Self::Whitewash | Self::Lime | Self::Stucco | Self::Mud => 2.5,
            Self::Painted(_) => 2.5,
            Self::Band => return Vec2::new(4.0, 1.0),
            Self::Yard => 3.0,
            Self::Earth => 2.0,
            // Un tile de grava cubre unos 2 m; la madera del escaneo, unos
            // 0.5 m, dos veces el ancho de un durmiente; su acero, unos 2.2 m.
            Self::Ballast => 2.0,
            Self::Sleeper => 0.5,
            Self::RailSteel | Self::RailHead => 2.2,
            Self::Livery(Livery::Black | Livery::BufferRed) => 2.2,
            Self::Livery(Livery::Tread) => 0.5,
            Self::Livery(_) => 2.0,
            // Una placa mide menos de un metro de ancho: el escaneo se repite
            // unas dos veces sobre ella.
            Self::Bronze => 0.5,
            Self::Recess | Self::Steel | Self::Sheet | Self::Water => 1.0,
        };
        Vec2::splat(tile)
    }

    /// El color que su material le da al escaneo.
    pub fn look(self) -> Look {
        let lifted = |scan, target| Look::Lifted { scan, target };
        match self {
            // Un toque del polvo de 1919 sobre los escaneos limpios.
            Self::Whitewash => Look::Tint([0.97, 0.95, 0.90]),
            Self::Galvanised => Look::Tint([0.92, 0.93, 0.94]),
            Self::Lime => Look::Tint([0.96, 0.84, 0.64]),
            Self::Stucco => Look::Tint([0.70, 0.68, 0.64]),
            // Empolvado un poco más oscuro que el escaneo limpio.
            Self::Bronze => Look::Tint([0.88, 0.85, 0.80]),
            Self::Painted(paint) => lifted(PAINTED_PLASTER_SCAN, paint.albedo()),
            Self::WashedBrick => lifted(WASHED_BRICK_SCAN, [0.80, 0.78, 0.73]),
            // Empolvado: un poco más pálido y más marrón que el escaneo limpio.
            Self::PatchedBrick => lifted(PATCHED_BRICK_SCAN, [0.58, 0.45, 0.35]),
            Self::Band => lifted(BAND_SCAN, [0.64, 0.58, 0.50]),
            Self::Paving => lifted(PAVING_SCAN, [0.48, 0.36, 0.30]),
            // Pisoteado un poco más pálido y polvoriento que un patio.
            Self::Earth => lifted(DIRT_PATH_SCAN, [0.50, 0.43, 0.33]),
            // La tierra compactada marrón del escaneo, agrisada hasta una grava
            // polvorienta.
            Self::Ballast => lifted(GRAVEL_SCAN, [0.48, 0.43, 0.37]),
            // Oscuro como la creosota y curtido, más oscuro que el escaneo
            // gris.
            Self::Sleeper => Look::Tint([0.62, 0.52, 0.44]),
            // El óxido de los escaneos está saturado: un poco enfriado hacia el
            // acero.
            Self::RailSteel | Self::RailHead => Look::Tint([0.85, 0.92, 1.0]),
            // El escaneo de óxido es naranja y las tablas, grises: estos tintes
            // llevan un texel medio a un negro gastado, un rojo oscuro, un
            // granate, un marrón barnizado y un rojo óxido, conservando la veta
            // y las manchas.
            Self::Livery(Livery::Tread) => Look::Tint([1.0, 1.0, 1.0]),
            Self::Livery(Livery::Black) => Look::Tint([0.43, 0.71, 1.0]),
            Self::Livery(Livery::BufferRed) => Look::Tint([1.0, 0.37, 0.54]),
            Self::Livery(Livery::Maroon) => Look::Tint([0.61, 0.20, 0.21]),
            Self::Livery(Livery::Varnish) => Look::Tint([0.68, 0.43, 0.28]),
            Self::Livery(Livery::Oxide) => Look::Tint([0.75, 0.37, 0.29]),
            _ => Look::Tint([1.0, 1.0, 1.0]),
        }
    }

    /// Su color liso (sRGB): todo el aspecto sin texturas, y el tinte sobre
    /// ellas.
    fn color(self, textured: bool) -> Color {
        match (self.look(), textured) {
            (Look::Tint([r, g, b]), true) => return Color::srgb(r, g, b),
            (Look::Lifted { scan, target }, true) => {
                // Por encima de 1 donde el escaneo es más oscuro que la
                // pintura, pero nunca por encima de MAX_LIFT.
                let [r, g, b] = lift(scan, target);
                return Color::linear_rgb(r, g, b);
            }
            (
                Look::Lifted {
                    target: [r, g, b], ..
                },
                false,
            ) => return Color::srgb(r, g, b),
            (Look::Tint(_), false) => {}
        }
        let (r, g, b) = match self {
            Self::Adobe => (0.62, 0.50, 0.38),
            Self::Whitewash => (0.88, 0.86, 0.80),
            Self::Brick => (0.58, 0.33, 0.24),
            Self::OldIron => (0.47, 0.38, 0.31),
            Self::Galvanised => (0.64, 0.65, 0.64),
            Self::Planks => (0.42, 0.33, 0.24),
            Self::Thatch => (0.66, 0.56, 0.36),
            Self::Mud => (0.50, 0.40, 0.29),
            Self::Yard => (0.60, 0.52, 0.40),
            Self::Recess => (0.05, 0.045, 0.04),
            Self::Steel => (0.24, 0.24, 0.25),
            Self::Sheet => (0.70, 0.71, 0.71),
            Self::Water => (0.10, 0.13, 0.13),
            Self::Lime => (0.86, 0.75, 0.56),
            Self::Stucco => (0.62, 0.60, 0.57),
            Self::Bronze => (0.45, 0.34, 0.22),
            Self::Sleeper => (0.30, 0.22, 0.16),
            Self::RailSteel => (0.33, 0.22, 0.16),
            Self::RailHead => (0.45, 0.40, 0.36),
            Self::Livery(Livery::Tread) => (0.20, 0.22, 0.18),
            Self::Livery(Livery::Black) => (0.07, 0.07, 0.075),
            Self::Livery(Livery::BufferRed) => (0.40, 0.06, 0.05),
            Self::Livery(Livery::Maroon) => (0.36, 0.10, 0.09),
            Self::Livery(Livery::Varnish) => (0.40, 0.24, 0.13),
            Self::Livery(Livery::Oxide) => (0.45, 0.20, 0.14),
            Self::Painted(_)
            | Self::WashedBrick
            | Self::PatchedBrick
            | Self::Band
            | Self::Paving
            | Self::Earth
            | Self::Ballast => unreachable!("a lifted look has its own plain colour"),
        };
        Color::srgb(r, g, b)
    }

    /// Rugosidad, metalicidad y reflectancia.
    fn pbr(self) -> (f32, f32, f32) {
        match self {
            Self::Galvanised => (1.0, 0.75, 0.5),
            Self::OldIron => (1.0, 0.35, 0.4),
            Self::Sheet => (0.45, 0.8, 0.5),
            Self::Steel => (0.6, 0.6, 0.5),
            // Un metal real; la rugosidad del mapa escala el 1.0.
            Self::Bronze => (1.0, 1.0, 0.5),
            // Acero oxidado; las ruedas pulieron el de la cabeza.
            // Reflectancia baja: el balasto y la madera quedan mate con el sol
            // bajo.
            Self::Ballast => (1.0, 0.0, 0.1),
            Self::Sleeper => (1.0, 0.0, 0.15),
            Self::RailSteel => (1.0, 0.7, 0.4),
            Self::RailHead => (0.55, 0.9, 0.5),
            // Acero pintado: el esmalte es brillante sobre la veta de un metal;
            // el barniz y la pintura de óxido van sobre madera.
            Self::Livery(Livery::Tread) => (0.8, 0.5, 0.4),
            Self::Livery(Livery::Black) => (0.75, 0.4, 0.5),
            Self::Livery(Livery::BufferRed) => (0.75, 0.3, 0.45),
            Self::Livery(Livery::Maroon) => (0.8, 0.0, 0.45),
            Self::Livery(Livery::Varnish) => (0.7, 0.0, 0.5),
            Self::Livery(Livery::Oxide) => (1.0, 0.0, 0.3),
            Self::Water => (0.08, 0.0, 0.5),
            Self::Recess => (1.0, 0.0, 0.1),
            _ => (1.0, 0.0, 0.3),
        }
    }

    /// Los decals tendidos sobre el suelo se mezclan con él según el alfa por
    /// vertex.
    fn blended(self) -> bool {
        matches!(self, Self::Mud | Self::Yard)
    }
}

/// Un material compartido por [`Surface`].
#[derive(Resource, Debug, Clone)]
pub struct FacadeMaterials {
    materials: Vec<(Surface, Handle<StandardMaterial>)>,
}

impl FacadeMaterials {
    /// El material de `surface`.
    pub fn get(&self, surface: Surface) -> Handle<StandardMaterial> {
        self.materials
            .iter()
            .find(|(s, _)| *s == surface)
            .map(|(_, m)| m.clone())
            .expect("every surface has a material")
    }

    /// Materiales de color liso para una app sin servidor de assets (tests
    /// headless): cada superficie en su color sin textura.
    pub(crate) fn plain(materials: &mut Assets<StandardMaterial>) -> Self {
        let materials = Surface::ALL
            .iter()
            .map(|&surface| (surface, materials.add(material(surface, None))))
            .collect();
        Self { materials }
    }
}

/// Construye los materiales compartidos, empezando la carga de texturas cuando
/// hay un servidor de assets y una raíz de assets.
///
/// Los mapas son UASTC, que Bevy 0.20 solo puede transcodificar a un formato
/// comprimido de GPU (BC, ASTC o ETC2): en un adaptador sin ninguno de ellos su
/// cargador hace panic en lugar de usar una alternativa. Así que en ese
/// adaptador no se carga ningún mapa y cada superficie dibuja su color liso.
/// Sin el recurso [`CompressedImageFormatSupport`] (apps headless) los mapas se
/// cargan.
pub(crate) fn setup_materials(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    server: Option<Res<AssetServer>>,
    support: Option<Res<CompressedImageFormatSupport>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let compressed = support.is_none_or(|s| !s.0.is_empty());
    if !compressed {
        warn!(
            "the GPU supports no compressed texture format (BC, ASTC or ETC2); \
             building textures are drawn as plain colour"
        );
    }
    let mut textured = 0;
    let built = Surface::ALL
        .iter()
        .map(|&surface| {
            let maps = match (surface.set(), root.as_deref(), server.as_deref()) {
                (Some(set), Some(root), Some(server)) if compressed => {
                    load_set(server, &root.0, set)
                }
                _ => None,
            };
            textured += maps.is_some() as usize;
            (surface, materials.add(material(surface, maps)))
        })
        .collect();
    if root.is_some() && server.is_some() {
        info!(
            "facade materials: {} surfaces, {textured} textured",
            Surface::ALL.len()
        );
    }
    commands.insert_resource(FacadeMaterials { materials: built });
}

/// Los archivos de un conjunto de texturas, bajo `TEXTURE_DIR/<set>/`: color
/// (sRGB), normal y ORM (lineal).
pub const MAPS: [(&str, bool); 3] = [("color", true), ("normal_gl", false), ("orm", false)];

/// La ruta del mapa `name` del conjunto `set`, bajo el [`AssetRoot`].
pub fn map_path(set: &str, name: &str) -> String {
    format!("{TEXTURE_DIR}/{set}/{name}.ktx2")
}

/// Los mapas en carga de un conjunto: color, normal y el mapa empaquetado de
/// oclusión, rugosidad y metalicidad.
struct Maps {
    color: Handle<Image>,
    normal: Handle<Image>,
    orm: Handle<Image>,
}

/// Empieza a cargar el conjunto de texturas `set`, o advierte y devuelve `None`
/// cuando falta uno de sus archivos.
fn load_set(server: &AssetServer, root: &Path, set: &str) -> Option<Maps> {
    let missing: Vec<String> = MAPS
        .into_iter()
        .map(|(name, _)| map_path(set, name))
        .filter(|path| !root.join(path).is_file())
        .collect();
    if !missing.is_empty() {
        warn!(
            "building textures {TEXTURE_DIR}/{set} not loaded (missing {}); \
             drawing plain colour",
            missing.join(", ")
        );
        return None;
    }
    let [color, normal, orm] =
        MAPS.map(|(name, srgb)| load_map(server, &map_path(set, name), srgb));
    Some(Maps { color, normal, orm })
}

/// Carga un mapa KTX2 con el sampler de repetición. Sus mips vienen horneados y
/// se usa tal cual, así que vive solo en la GPU.
pub(crate) fn load_map(server: &AssetServer, path: &str, srgb: bool) -> Handle<Image> {
    load_map_with(server, path, srgb, repeating())
}

/// Carga un mapa KTX2 como [`load_map`], con `sampler`.
pub(crate) fn load_map_with(
    server: &AssetServer,
    path: &str,
    srgb: bool,
    sampler: ImageSamplerDescriptor,
) -> Handle<Image> {
    server
        .load_builder()
        .with_settings(move |s: &mut ImageLoaderSettings| {
            s.is_srgb = srgb;
            s.sampler = ImageSampler::Descriptor(sampler.clone());
            s.asset_usage = RenderAssetUsages::RENDER_WORLD;
        })
        .load(path.to_owned())
}

/// El sampler de repetición anisotrópico de las superficies con tiles.
pub(crate) fn repeating() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    }
}

/// El material de `surface`, con `maps` cuando se están cargando.
fn material(surface: Surface, maps: Option<Maps>) -> StandardMaterial {
    let (perceptual_roughness, metallic, reflectance) = surface.pbr();
    let mut material = StandardMaterial {
        base_color: surface.color(maps.is_some()),
        perceptual_roughness,
        metallic,
        reflectance,
        ..default()
    };
    if surface.blended() {
        // Tendido sobre el suelo como el camino: mezclado según el alfa por
        // vertex y acercado un poco hacia la cámara para que nunca compita con
        // el terreno.
        material.alpha_mode = AlphaMode::Blend;
        material.depth_bias = 2.0;
    }
    if let Some(maps) = maps {
        material.base_color_texture = Some(maps.color);
        material.normal_map_texture = Some(maps.normal);
        // Un mapa para ambos: Bevy lee la oclusión del rojo, la rugosidad del
        // verde y la metalicidad del azul.
        material.metallic_roughness_texture = Some(maps.orm.clone());
        material.occlusion_texture = Some(maps.orm);
    }
    material
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_orm_map_serves_occlusion_and_metallic_roughness() {
        let orm = Handle::<Image>::default();
        let maps = Maps {
            color: Handle::default(),
            normal: Handle::default(),
            orm: orm.clone(),
        };
        let material = material(Surface::Brick, Some(maps));
        assert_eq!(material.metallic_roughness_texture, Some(orm.clone()));
        assert_eq!(material.occlusion_texture, Some(orm));
    }

    #[test]
    fn the_new_scans_are_lifted_to_muted_1919_colours() {
        let mut lifted = 0;
        for surface in Surface::ALL {
            let Look::Lifted { scan, target } = surface.look() else {
                continue;
            };
            lifted += 1;
            // Un texel medio del escaneo cae en el color objetivo.
            let tint = surface.color(true).to_linear();
            let want = Color::srgb(target[0], target[1], target[2]).to_linear();
            for (t, (s, w)) in [tint.red, tint.green, tint.blue]
                .into_iter()
                .zip(scan.into_iter().zip([want.red, want.green, want.blue]))
            {
                assert!((t * s - w).abs() < 1e-3, "{surface:?}: {t} x {s} vs {w}");
            }
            // Sin textura, es ese color.
            assert_eq!(
                surface.color(false),
                Color::srgb(target[0], target[1], target[2])
            );
            // Apagados: polvorientos, nunca brillantes ni saturados, nunca de
            // un blanco enceguecedor.
            let (max, min) = (
                target.iter().copied().fold(0.0, f32::max),
                target.iter().copied().fold(1.0, f32::min),
            );
            assert!((0.4..=0.87).contains(&max), "{surface:?}: {target:?}");
            assert!(
                (max - min) / max <= 0.45,
                "{surface:?}: {target:?} too saturated"
            );
        }
        assert_eq!(
            lifted, 11,
            "five paints, two bricks, the band, the paving, the earth and the ballast"
        );
    }

    #[test]
    fn no_lift_passes_the_bound_and_a_steeper_one_folds_into_its_target() {
        for surface in Surface::ALL {
            let Look::Lifted { .. } = surface.look() else {
                continue;
            };
            let tint = surface.color(true).to_linear();
            for g in [tint.red, tint.green, tint.blue] {
                assert!(g <= MAX_LIFT + 1e-6, "{surface:?}: {g}");
            }
        }
        // Un escaneo demasiado oscuro para su pintura: la elevación se detiene
        // en el límite, con la misma proporción en cada canal, así que el texel
        // medio cae en un tono más oscuro del objetivo.
        let (scan, target) = ([0.05, 0.04, 0.03], [0.80, 0.70, 0.60]);
        let gain = lift(scan, target);
        let steepest = gain.into_iter().fold(0.0, f32::max);
        assert!((steepest - MAX_LIFT).abs() < 1e-5, "{gain:?}");
        let t = Color::srgb(target[0], target[1], target[2]).to_linear();
        let full = [t.red / scan[0], t.green / scan[1], t.blue / scan[2]];
        let k = gain[0] / full[0];
        assert!(k < 1.0);
        for (g, f) in gain.into_iter().zip(full) {
            assert!((g / f - k).abs() < 1e-5, "hue kept: {gain:?} vs {full:?}");
        }
        // Por debajo del límite, la elevación es la proporción completa.
        assert_eq!(lift([0.4, 0.4, 0.4], [0.5, 0.5, 0.5])[0], {
            let t = Color::srgb(0.5, 0.5, 0.5).to_linear();
            t.red / 0.4
        });
    }

    /// La fracción del escaneo de `set` (su `color.jpg`) cuyos texels lleva más
    /// allá del blanco algún canal de `gain`.
    fn clipped(set: &str, gain: [f32; 3]) -> f32 {
        use bevy::image::{CompressedImageFormats, ImageType};
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let file = root.join(TEXTURE_DIR).join(set).join("color.jpg");
        let bytes = std::fs::read(&file).unwrap();
        let image = Image::from_buffer(
            &bytes,
            ImageType::Extension("jpg"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::Default,
            RenderAssetUsages::default(),
        )
        .unwrap();
        let data = image.data.as_ref().unwrap();
        let texels = (image.width() * image.height()) as usize;
        assert_eq!(data.len(), texels * 4, "{set}: RGBA8");
        let linear: Vec<f32> = (0..=255u8)
            .map(|v| Srgba::gamma_function(f32::from(v) / 255.0))
            .collect();
        let over = data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| (0..3).any(|c| linear[usize::from(px[c])] * gain[c] > 1.0))
            .count();
        over as f32 / texels as f32
    }

    #[test]
    fn the_lifted_scans_hardly_clip() {
        for surface in Surface::ALL {
            let (Look::Lifted { .. }, Some(set)) = (surface.look(), surface.set()) else {
                continue;
            };
            let tint = surface.color(true).to_linear();
            let share = clipped(set, [tint.red, tint.green, tint.blue]);
            println!("{surface:?}: {:.4}% of the scan past white", share * 100.0);
            assert!(share <= 0.001, "{surface:?}: {share}");
        }
    }

    #[test]
    fn the_track_surfaces_wear_their_sets_and_fall_back_to_colour() {
        for (surface, set, tile) in [
            (Surface::Ballast, "gravel_road", 2.0),
            (Surface::Sleeper, "rough_wood", 0.5),
            (Surface::RailSteel, "rust_coarse_01", 2.2),
            (Surface::RailHead, "rust_coarse_01", 2.2),
        ] {
            assert_eq!(surface.set(), Some(set));
            assert_eq!(surface.tiles(), Vec2::splat(tile));
            // Sin un servidor de assets: un color liso, nunca blanco.
            let plain = surface.color(false).to_srgba();
            assert!(
                plain.red < 0.7 && plain.blue < 0.7,
                "{surface:?}: {plain:?}"
            );
            let m = material(surface, None);
            assert!(m.base_color_texture.is_none());
        }
        // La cabeza es la más lisa y más metálica de las dos del riel.
        let (body, head) = (Surface::RailSteel.pbr(), Surface::RailHead.pbr());
        assert!(head.0 < body.0 && head.1 > body.1);
    }

    #[test]
    fn the_train_liveries_tint_their_scans_to_black_red_maroon_brown_and_oxide() {
        for (livery, set, tile) in [
            (Livery::Tread, "metal_plate", 0.5),
            (Livery::Black, "rust_coarse_01", 2.2),
            (Livery::BufferRed, "rust_coarse_01", 2.2),
            (Livery::Maroon, "brown_planks_05", 2.0),
            (Livery::Varnish, "brown_planks_05", 2.0),
            (Livery::Oxide, "brown_planks_05", 2.0),
        ] {
            let surface = Surface::Livery(livery);
            assert_eq!(surface.set(), Some(set), "{livery:?}");
            assert_eq!(surface.tiles(), Vec2::splat(tile), "{livery:?}");
            assert!(Surface::ALL.contains(&surface));
            // Un tinte solo oscurece su escaneo.
            let Look::Tint(tint) = surface.look() else {
                panic!("{livery:?} is a tint");
            };
            assert!(tint.iter().all(|c| (0.0..=1.0).contains(c)));
        }
        // A qué llega un texel medio (lineal): las medias medidas de los
        // escaneos por el tinte caen cerca del propio color de la pintura
        // (sRGB).
        let mean = |scan: [f32; 3], livery: Livery| {
            let Look::Tint(t) = Surface::Livery(livery).look() else {
                unreachable!()
            };
            let t = Color::srgb(t[0], t[1], t[2]).to_linear();
            let m = [scan[0] * t.red, scan[1] * t.green, scan[2] * t.blue];
            Color::linear_rgb(m[0], m[1], m[2]).to_srgba()
        };
        let rust = [0.129, 0.0436, 0.0152];
        let planks = [0.328, 0.303, 0.253];
        let black = mean(rust, Livery::Black);
        assert!(
            black.red.max(black.green).max(black.blue) < 0.22,
            "{black:?}"
        );
        let maroon = mean(planks, Livery::Maroon);
        assert!(
            maroon.red > 2.0 * maroon.green && maroon.red < 0.5,
            "{maroon:?}"
        );
        let oxide = mean(planks, Livery::Oxide);
        assert!(oxide.red > 0.35 && oxide.green < 0.3, "{oxide:?}");
        // La madera pintada no tiene metal; el esmalte sobre acero tiene algo.
        assert_eq!(Surface::Livery(Livery::Maroon).pbr().1, 0.0);
        assert!(Surface::Livery(Livery::Black).pbr().1 > 0.2);
    }

    #[test]
    fn the_band_strip_keeps_its_texels_square() {
        // beige_wall_002 mide 1024 x 256: una repetición cuatro veces más ancha
        // que alta.
        let band = Surface::Band.tiles();
        assert!((band.x - 4.0 * band.y).abs() < 1e-6, "{band}");
        for surface in Surface::ALL.into_iter().filter(|&s| s != Surface::Band) {
            let t = surface.tiles();
            assert_eq!(t.x, t.y, "{surface:?}");
        }
    }

    #[test]
    fn every_textured_surface_names_a_shipped_set() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for surface in Surface::ALL {
            let Some(set) = surface.set() else {
                continue;
            };
            for (map, _) in MAPS {
                let file = root.join(map_path(set, map));
                assert!(file.is_file(), "{}", file.display());
            }
        }
    }
}
