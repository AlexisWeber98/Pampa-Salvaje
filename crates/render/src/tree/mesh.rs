//! Las meshes de los árboles de la plaza: un tronco que se afina y ramas de
//! corteza, y tarjetas de hojas colgadas en las puntas de las ramas, con
//! semilla por especie y variante.
//!
//! Un árbol son dos meshes, corteza y hojas, construidas una vez por variante
//! al iniciar y compartidas por todos los árboles de esa variante (cada uno
//! colocado con su propia guiñada y escala). El tronco, las ramas y las ramitas
//! son tubos abiertos de pocos lados con normales radiales suavizadas y UVs que
//! repiten la corteza cada [`Form::bark_tile`] metros, un número entero de
//! veces alrededor de cada tubo para que no se vea su costura. Una tarjeta es
//! un ramito del atlas de hojas de la especie (un cuarto de él, ver
//! `tools/leaf_atlas.py`), colgado de una punta por la base del ramito y
//! doblado un poco hacia abajo por su propio peso. Sus normales se inclinan
//! hacia afuera desde el centro de la copa, así que la copa se sombrea como una
//! sola masa y no como tarjetas planas dispersas.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::Species;
use crate::ground::face_tangent;

/// Desde dónde crece un ramito en su cuarto del atlas de hojas, como fracción
/// de la altura del cuarto desde su borde inferior (el `BASE` de
/// `leaf_atlas.py`: 32 de 512 texels).
pub const SPRIG_BASE: f32 = 32.0 / 512.0;

/// La forma de una especie: su tronco, sus ramas y sus tarjetas de hojas,
/// dispuestos a partir de [`Form::height`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Form {
    /// La altura para la que se disponen las ramas, en metros (mín, máx): una
    /// escala para la horqueta, el largo de las ramas y el eje central, no la
    /// altura del árbol construido. Esa se mide a partir de la geometría en
    /// [`TreeMesh::height`] y sale más baja donde las ramas se inclinan hacia
    /// afuera (la del aromo es de unos 4.6 m frente a una altura de disposición
    /// de 6.5–7.5).
    pub height: (f32, f32),
    /// Dónde se separan las ramas del tronco, como fracción de la altura.
    pub fork: f32,
    /// El radio del tronco a ras del suelo, en metros.
    pub trunk_radius: f32,
    /// Ramas (mín, máx).
    pub limbs: (u32, u32),
    /// La inclinación de una rama respecto de la vertical, en grados (mín,
    /// máx).
    pub tilt: (f32, f32),
    /// El largo de una rama, como fracción de la altura.
    pub reach: f32,
    /// Cuánto se curva hacia arriba (o hacia abajo, por debajo de cero) la
    /// mitad exterior de una rama, como fracción de su largo.
    pub rise: f32,
    /// Ramitas de cada rama.
    pub twigs: u32,
    /// El tronco sigue hacia arriba a través de las ramas hasta la cima
    /// (eucalipto).
    pub leader: bool,
    /// Tarjetas de hojas en todo el árbol.
    pub cards: u32,
    /// El lado de una tarjeta, en metros.
    pub card: f32,
    /// 0: las tarjetas se extienden hacia afuera y hacia arriba desde su rama;
    /// 1: cuelgan derecho hacia abajo.
    pub hang: f32,
    /// Cuánto se dobla hacia abajo la punta de una tarjeta, como fracción de su
    /// lado.
    pub sag: f32,
    /// Cuánto se dispersan las tarjetas alrededor de su rama, como fracción de
    /// una tarjeta.
    pub scatter: f32,
    /// Metros por repetición de la corteza, alrededor del tubo y a lo largo de
    /// él.
    pub bark_tile: Vec2,
    /// Cuánto se inclina el tronco, como fracción de la altura (una horqueta
    /// baja se inclina menos, ver [`grow`]).
    pub lean: f32,
    /// Cuánto da forma a la copa un viento predominante: 0 es pareja todo
    /// alrededor; 1 tiene las ramas de sotavento bastante más largas y más
    /// planas que las de barlovento, con el tronco inclinado con ellas.
    pub wind: f32,
    /// La proporción de las tarjetas de hojas colgadas en grupos densos en las
    /// puntas de las ramas en lugar de dispersas a lo largo de ellas.
    pub clump: f32,
    /// Un arbusto: sus tallos se afinan hasta puntas finas y llevan ramitas
    /// finas, mientras que las ramas de un árbol terminan con el grosor de una
    /// ramita, sea cual sea el radio del tronco.
    pub shrub: bool,
    /// Raíces tablares que se abren desde el tronco hasta el suelo (las del
    /// ombú).
    pub buttress: u32,
    /// Cuánto zigzaguea el centro de una rama fuera de su línea, como fracción
    /// de su largo (las del tala).
    pub kink: f32,
}

impl Species {
    /// Su forma podada para una calle: un tronco desnudo hasta unos 3 m, y
    /// después una copa más llena pegada al tronco (las ramas se separan a unos
    /// 3.3 m de altura y se inclinan poco), con la corteza, las hojas y el
    /// porte propios de la especie.
    pub(crate) fn street_form(self) -> Form {
        let form = self.form();
        let (tilt, reach) = match self {
            Self::Paraiso => ((24.0, 34.0), 0.15),
            Self::Olmo => ((12.0, 20.0), 0.13),
            _ => ((10.0, 16.0), 0.12),
        };
        Form {
            height: (8.5, 10.0),
            fork: 0.4,
            tilt,
            reach,
            rise: 0.1,
            twigs: 3,
            leader: false,
            cards: form.cards,
            card: form.card * 0.9,
            hang: form.hang.min(0.4),
            scatter: 0.2,
            ..form
        }
    }

    /// Su forma: la sombrilla redonda del paraíso, el florero alto del olmo, la
    /// copa alta y abierta de hojas colgantes del eucalipto, la copa baja y
    /// plumosa del aromo.
    pub(crate) fn form(self) -> Form {
        match self {
            Self::Paraiso => Form {
                height: (7.5, 8.5),
                fork: 0.45,
                trunk_radius: 0.2,
                limbs: (5, 6),
                tilt: (55.0, 70.0),
                reach: 0.3,
                rise: 0.15,
                twigs: 3,
                leader: false,
                cards: 120,
                card: 1.5,
                hang: 0.1,
                sag: 0.12,
                scatter: 0.3,
                bark_tile: Vec2::new(1.0, 1.0),
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            Self::Olmo => Form {
                height: (11.0, 13.0),
                fork: 0.33,
                trunk_radius: 0.28,
                limbs: (4, 5),
                tilt: (15.0, 25.0),
                reach: 0.5,
                rise: 0.08,
                twigs: 4,
                leader: false,
                cards: 140,
                card: 1.6,
                hang: 0.15,
                sag: 0.15,
                scatter: 0.3,
                // Bark001 es el doble de alto que de ancho.
                bark_tile: Vec2::new(0.8, 1.6),
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            Self::Eucalipto => Form {
                height: (14.0, 17.0),
                fork: 0.45,
                trunk_radius: 0.3,
                limbs: (6, 7),
                tilt: (30.0, 50.0),
                reach: 0.2,
                rise: -0.12,
                twigs: 3,
                leader: true,
                cards: 150,
                card: 1.6,
                hang: 0.7,
                sag: 0.1,
                scatter: 0.3,
                bark_tile: Vec2::new(1.2, 1.2),
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            Self::Aromo => Form {
                height: (6.5, 7.5),
                fork: 0.24,
                trunk_radius: 0.15,
                limbs: (5, 6),
                tilt: (40.0, 60.0),
                reach: 0.32,
                rise: 0.12,
                twigs: 3,
                leader: false,
                cards: 130,
                card: 1.3,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.3,
                bark_tile: Vec2::new(0.8, 0.8),
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            // El caldén: un tronco corto y grueso que se bifurca bajo en ramas
            // casi horizontales, una sombrilla ancha y plana, más ancha que
            // alta, de hojas pequeñas y finas, colgadas en grupos densos. Su
            // tronco queda dentro de lo que planta
            // `pampa_core::Tree::trunk_radius` (0.035 de la altura). Esta es la
            // forma clásica; ver [`Self::variant_form`] para el resto.
            Self::Calden => Form {
                height: (6.0, 7.0),
                fork: 0.25,
                trunk_radius: 0.14,
                limbs: (6, 7),
                tilt: (45.0, 60.0),
                reach: 0.42,
                rise: 0.1,
                twigs: 4,
                leader: false,
                cards: 170,
                card: 1.3,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.3,
                // Gruesa, para que los surcos sobrevivan a los mipmaps a
                // distancia.
                bark_tile: Vec2::new(1.2, 1.8),
                lean: 0.04,
                wind: 0.0,
                clump: 0.6,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            // El chañar: varios tallos delgados que se abren en abanico desde
            // una sola cepa baja, con muchas ramitas, y una copa irregular de
            // hojas pequeñas en grupos.
            Self::Chanar => Form {
                height: (3.4, 4.2),
                fork: 0.08,
                trunk_radius: 0.09,
                limbs: (5, 7),
                tilt: (15.0, 32.0),
                reach: 0.75,
                rise: 0.12,
                twigs: 5,
                leader: false,
                cards: 110,
                card: 0.6,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.5,
                bark_tile: Vec2::new(0.8, 1.6),
                lean: 0.04,
                wind: 0.0,
                clump: 0.5,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            // El piquillín: un arbusto espinoso denso, con muchos tallos finos
            // desde el suelo bajo una masa de hojas diminutas.
            Self::Piquillin => Form {
                height: (1.4, 1.7),
                fork: 0.05,
                trunk_radius: 0.035,
                limbs: (5, 6),
                tilt: (28.0, 48.0),
                reach: 0.8,
                rise: 0.1,
                twigs: 2,
                leader: false,
                cards: 50,
                card: 0.24,
                hang: 0.15,
                sag: 0.15,
                scatter: 0.4,
                bark_tile: Vec2::new(0.3, 0.5),
                lean: 0.02,
                wind: 0.0,
                clump: 0.5,
                shrub: true,
                buttress: 0,
                kink: 0.0,
            },
            // La jarilla: muchos tallos erguidos y delgados, ralos y resinosos.
            Self::Jarilla => Form {
                height: (1.2, 1.5),
                fork: 0.04,
                trunk_radius: 0.03,
                limbs: (7, 8),
                tilt: (6.0, 16.0),
                reach: 0.95,
                rise: 0.05,
                twigs: 1,
                leader: false,
                cards: 55,
                card: 0.2,
                hang: 0.1,
                sag: 0.1,
                scatter: 0.35,
                bark_tile: Vec2::new(0.3, 0.6),
                lean: 0.01,
                wind: 0.0,
                clump: 0.3,
                shrub: true,
                buttress: 0,
                kink: 0.0,
            },
            // El fachinal: retoños de chañar, una maraña de tallos delgados.
            Self::Fachinal => Form {
                height: (1.8, 2.1),
                fork: 0.06,
                trunk_radius: 0.05,
                limbs: (5, 6),
                tilt: (10.0, 26.0),
                reach: 0.8,
                rise: 0.1,
                twigs: 2,
                leader: false,
                cards: 50,
                card: 0.4,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.5,
                bark_tile: Vec2::new(0.5, 1.0),
                lean: 0.03,
                wind: 0.0,
                clump: 0.5,
                shrub: true,
                buttress: 0,
                kink: 0.0,
            },
            // El álamo de Lombardía: un fuste corto, y después un haz de ramas
            // levantadas casi verticales formando una columna de alrededor de
            // un quinto de ancho respecto de su altura, con hojas pequeñas a lo
            // largo de ellas. Su tronco queda dentro del radio con el que
            // colisiona `pampa_core::Structure::trunk_radius` para un álamo
            // (0.012 de la altura).
            Self::Alamo => Form {
                height: (14.0, 16.0),
                fork: 0.08,
                trunk_radius: 0.16,
                limbs: (10, 12),
                tilt: (1.5, 4.0),
                reach: 0.75,
                rise: 0.05,
                twigs: 0,
                leader: false,
                cards: 170,
                card: 1.0,
                hang: 0.05,
                sag: 0.1,
                scatter: 0.15,
                bark_tile: Vec2::new(0.8, 1.6),
                lean: 0.01,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            // El algarrobo: un árbol bajo y ancho, a menudo de varios tallos,
            // con varios tallos robustos desde cerca del suelo bajo una copa
            // ancha de hojas finas y colgantes.
            Self::Algarrobo => Form {
                height: (5.0, 6.0),
                fork: 0.16,
                trunk_radius: 0.16,
                limbs: (4, 6),
                tilt: (28.0, 45.0),
                reach: 0.6,
                rise: 0.18,
                twigs: 4,
                leader: false,
                cards: 150,
                card: 1.1,
                hang: 0.3,
                sag: 0.2,
                scatter: 0.35,
                bark_tile: Vec2::new(1.0, 1.6),
                lean: 0.1,
                wind: 0.0,
                clump: 0.45,
                shrub: false,
                buttress: 0,
                kink: 0.0,
            },
            // El tala: un tronco corto y ramas torcidas en zigzag, con muchas
            // ramitas, bajo una copa redonda e irregular de hojas pequeñas.
            Self::Tala => Form {
                height: (4.5, 5.5),
                fork: 0.16,
                trunk_radius: 0.12,
                limbs: (5, 7),
                tilt: (35.0, 60.0),
                reach: 0.55,
                rise: 0.1,
                twigs: 5,
                leader: false,
                cards: 140,
                card: 0.7,
                hang: 0.15,
                sag: 0.15,
                scatter: 0.5,
                bark_tile: Vec2::new(1.0, 1.6),
                lean: 0.05,
                wind: 0.0,
                clump: 0.4,
                shrub: false,
                buttress: 0,
                kink: 0.22,
            },
            // El espinillo: un árbol pequeño, con un tallo corto bajo una copa
            // redondeada de hojas diminutas.
            Self::Espinillo => Form {
                height: (3.4, 4.2),
                fork: 0.2,
                trunk_radius: 0.09,
                limbs: (4, 5),
                tilt: (30.0, 45.0),
                reach: 0.5,
                rise: 0.1,
                twigs: 5,
                leader: false,
                cards: 150,
                card: 0.45,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.45,
                bark_tile: Vec2::new(0.9, 1.6),
                lean: 0.06,
                wind: 0.0,
                clump: 0.5,
                shrub: false,
                buttress: 0,
                kink: 0.05,
            },
            // El ombú: un tronco enorme y abultado con raíces tablares, un
            // fuste corto, y después unas pocas ramas macizas bajo una cúpula
            // ancha y densa. Sus raíces quedan dentro del radio con el que
            // colisiona `pampa_core::Tree::trunk_radius` para él (un décimo de
            // la altura).
            Self::Ombu => Form {
                height: (9.0, 10.0),
                fork: 0.28,
                trunk_radius: 0.6,
                limbs: (6, 7),
                tilt: (40.0, 55.0),
                reach: 0.4,
                rise: 0.2,
                twigs: 2,
                leader: false,
                cards: 150,
                card: 1.9,
                hang: 0.15,
                sag: 0.15,
                scatter: 0.35,
                bark_tile: Vec2::new(1.6, 2.0),
                lean: 0.03,
                wind: 0.0,
                clump: 0.75,
                shrub: false,
                buttress: 5,
                kink: 0.0,
            },
        }
    }

    /// Su forma para la variante `seed`. El caldén tiene
    /// [`crate::tree::CALDEN_LIVE`] formas vivas (una sombrilla clásica, uno
    /// bajo de varios tallos, un árbol inclinado, una copa asimétrica formada
    /// por el viento, uno alto y angosto, uno ancho y plano, uno redondo y
    /// denso, una horqueta alta y uno inclinado y azotado por el viento), y
    /// después tres secos (ralo, muerto y un tocón quebrado); el chañar tiene
    /// [`crate::tree::CHANAR_VARIANTS`] formas. Todas las demás especies tienen
    /// su [`Self::form`] para cualquier semilla.
    pub(crate) fn variant_form(self, seed: u32) -> Form {
        let form = self.form();
        let v = seed as usize;
        match self {
            Self::Calden => match v {
                // Clásico: un solo tronco, una sombrilla pareja.
                0 => form,
                // De varios tallos con horqueta baja: muchas ramas robustas
                // desde cerca del suelo.
                1 => Form {
                    fork: 0.12,
                    limbs: (7, 8),
                    tilt: (22.0, 42.0),
                    reach: 0.5,
                    rise: 0.15,
                    cards: 180,
                    clump: 0.7,
                    ..form
                },
                // Inclinado: todo el árbol se ladea y la copa cuelga hacia un
                // lado.
                2 => Form {
                    fork: 0.3,
                    lean: 0.17,
                    tilt: (40.0, 58.0),
                    reach: 0.4,
                    ..form
                },
                // Asimétrico: formado por el viento, con ramas largas y bajas
                // hacia el lado de sotavento.
                3 => Form {
                    fork: 0.22,
                    tilt: (55.0, 72.0),
                    reach: 0.46,
                    wind: 0.9,
                    lean: 0.09,
                    ..form
                },
                // Alto y angosto: una horqueta alta bajo ramas empinadas.
                4 => Form {
                    height: (7.5, 8.5),
                    fork: 0.42,
                    limbs: (5, 6),
                    tilt: (14.0, 28.0),
                    reach: 0.28,
                    rise: 0.2,
                    cards: 160,
                    card: 1.2,
                    ..form
                },
                // Ancho y plano: ramas largas casi horizontales, una copa baja.
                5 => Form {
                    height: (4.8, 5.6),
                    fork: 0.4,
                    limbs: (7, 8),
                    tilt: (68.0, 80.0),
                    reach: 0.62,
                    rise: 0.12,
                    cards: 190,
                    card: 1.4,
                    clump: 0.6,
                    ..form
                },
                // Denso y redondo: una copa compacta llena de hojas.
                6 => Form {
                    fork: 0.4,
                    limbs: (7, 8),
                    tilt: (32.0, 48.0),
                    reach: 0.34,
                    twigs: 3,
                    cards: 190,
                    card: 1.5,
                    scatter: 0.25,
                    clump: 0.85,
                    ..form
                },
                // Horqueta alta: pocas ramas gruesas en lo alto de un tronco
                // alto.
                7 => Form {
                    height: (6.8, 7.6),
                    fork: 0.46,
                    limbs: (3, 4),
                    tilt: (35.0, 50.0),
                    reach: 0.5,
                    twigs: 5,
                    cards: 170,
                    ..form
                },
                // Azotado por el viento: muy inclinado, con la copa extendida a
                // favor del viento.
                8 => Form {
                    fork: 0.36,
                    tilt: (50.0, 70.0),
                    reach: 0.5,
                    rise: 0.05,
                    wind: 0.8,
                    lean: 0.14,
                    ..form
                },
                // Ralo: una copa raleada.
                9 => Form {
                    limbs: (5, 6),
                    cards: 60,
                    clump: 0.3,
                    ..form
                },
                // Muerto: ramas desnudas y retorcidas que se elevan, con unas
                // últimas hojas.
                10 => Form {
                    limbs: (4, 5),
                    tilt: (35.0, 60.0),
                    reach: 0.7,
                    rise: 0.3,
                    twigs: 5,
                    cards: 16,
                    clump: 0.0,
                    ..form
                },
                // Un tocón quebrado: sin la parte superior, con ramas cortas y
                // un rastro de hojas.
                _ => Form {
                    height: (3.8, 4.6),
                    trunk_radius: 0.12,
                    fork: 0.35,
                    limbs: (3, 4),
                    tilt: (30.0, 50.0),
                    reach: 0.4,
                    rise: 0.25,
                    twigs: 4,
                    cards: 12,
                    clump: 0.0,
                    ..form
                },
            },
            Self::Chanar => match v {
                // El abanico clásico de tallos.
                0 => form,
                // Alto y delgado.
                1 => Form {
                    height: (4.2, 4.8),
                    tilt: (8.0, 18.0),
                    reach: 0.55,
                    ..form
                },
                // Bajo y extendido.
                2 => Form {
                    height: (2.8, 3.2),
                    tilt: (35.0, 55.0),
                    reach: 0.95,
                    ..form
                },
                // Inclinado.
                3 => Form {
                    lean: 0.12,
                    fork: 0.22,
                    ..form
                },
                // Una maraña densa.
                4 => Form {
                    limbs: (7, 8),
                    cards: 150,
                    card: 0.65,
                    clump: 0.8,
                    scatter: 0.35,
                    ..form
                },
                // Formado por el viento.
                _ => Form {
                    wind: 0.8,
                    lean: 0.1,
                    tilt: (25.0, 45.0),
                    ..form
                },
            },
            // Los arbustos: la forma simple, una baja y extendida, una alta y
            // densa, con la semilla girando los tallos de cada una.
            Self::Piquillin | Self::Jarilla | Self::Fachinal => match v % 3 {
                0 => form,
                1 => Form {
                    height: (form.height.0 * 0.7, form.height.1 * 0.7),
                    tilt: (form.tilt.0 + 14.0, form.tilt.1 + 16.0),
                    reach: form.reach * 1.2,
                    ..form
                },
                _ => Form {
                    height: (form.height.0 * 1.25, form.height.1 * 1.25),
                    tilt: (form.tilt.0 * 0.7, form.tilt.1 * 0.7),
                    clump: (form.clump + 0.2).min(1.0),
                    ..form
                },
            },
            Self::Algarrobo => match v {
                // El clásico: unos pocos tallos que se abren en abanico desde
                // abajo.
                0 => form,
                // Un solo tronco robusto que se bifurca bajo, con una copa
                // ancha.
                1 => Form {
                    fork: 0.22,
                    limbs: (5, 6),
                    tilt: (50.0, 65.0),
                    reach: 0.62,
                    trunk_radius: 0.2,
                    ..form
                },
                // Torcido: todo el árbol se inclina.
                2 => Form {
                    fork: 0.16,
                    lean: 0.13,
                    cards: 120,
                    ..form
                },
                // Un florero más alto de tallos empinados.
                3 => Form {
                    height: (6.5, 7.5),
                    tilt: (14.0, 26.0),
                    reach: 0.45,
                    cards: 170,
                    ..form
                },
                // Bajo y muy ancho.
                _ => Form {
                    height: (3.6, 4.2),
                    limbs: (5, 6),
                    tilt: (58.0, 74.0),
                    reach: 0.9,
                    cards: 100,
                    card: 1.0,
                    ..form
                },
            },
            Self::Tala => match v {
                0 => form,
                // Bajo, achaparrado y extendido.
                1 => Form {
                    height: (3.2, 3.8),
                    tilt: (50.0, 70.0),
                    reach: 0.85,
                    cards: 110,
                    ..form
                },
                // Más alto y más erguido.
                2 => Form {
                    height: (6.0, 7.0),
                    tilt: (18.0, 32.0),
                    reach: 0.4,
                    kink: 0.15,
                    cards: 170,
                    ..form
                },
                // Inclinado, doblado por el viento.
                3 => Form {
                    lean: 0.1,
                    tilt: (45.0, 70.0),
                    reach: 0.65,
                    ..form
                },
                // Una copa redonda, densa y compacta.
                _ => Form {
                    reach: 0.4,
                    twigs: 6,
                    cards: 190,
                    card: 0.6,
                    clump: 0.6,
                    kink: 0.12,
                    ..form
                },
            },
            Self::Espinillo => match v {
                0 => form,
                // Un arbolito bajo y extendido.
                1 => Form {
                    height: (2.6, 3.0),
                    fork: 0.16,
                    tilt: (45.0, 62.0),
                    reach: 0.8,
                    cards: 110,
                    ..form
                },
                // Un solo tallo más alto.
                2 => Form {
                    height: (4.4, 5.0),
                    fork: 0.3,
                    tilt: (20.0, 32.0),
                    reach: 0.4,
                    cards: 130,
                    ..form
                },
                // Inclinado.
                3 => Form {
                    lean: 0.18,
                    fork: 0.25,
                    ..form
                },
                // Denso y redondo.
                _ => Form {
                    limbs: (5, 6),
                    cards: 190,
                    card: 0.5,
                    clump: 0.7,
                    ..form
                },
            },
            Self::Ombu => match v {
                0 => form,
                // Más ancho y más plano.
                1 => Form {
                    fork: 0.3,
                    tilt: (55.0, 68.0),
                    reach: 0.52,
                    cards: 160,
                    ..form
                },
                // Una cúpula más alta sobre un fuste más largo.
                2 => Form {
                    height: (10.0, 11.0),
                    fork: 0.34,
                    tilt: (30.0, 45.0),
                    reach: 0.36,
                    ..form
                },
                // Inclinado, con pocas ramas macizas.
                _ => Form {
                    fork: 0.3,
                    limbs: (4, 5),
                    lean: 0.04,
                    wind: 0.5,
                    card: 2.1,
                    ..form
                },
            },
            _ => form,
        }
    }
}

/// Los grupos densos de hojas que cuelga una copa ([`Form::clump`]).
const CLUMPS: usize = 9;

/// Los lados del tronco, de una rama y de una ramita.
const TRUNK_SIDES: u32 = 8;
const LIMB_SIDES: u32 = 6;
const TWIG_SIDES: u32 = 3;

/// Los vertices de una mesh, antes de convertirse en una [`Mesh`].
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Buffers {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Buffers {
    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, uv: [f32; 2]) -> u32 {
        self.positions.push(p);
        self.normals.push(n);
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }

    /// Dos triángulos sobre las esquinas `a b c d`, en sentido antihorario.
    fn quad(&mut self, [a, b, c, d]: [u32; 4]) {
        self.indices.extend([a, b, c, a, c, d]);
    }

    /// Tangentes por vertex para los normal maps: la tangente UV de cada
    /// triángulo, sumada alrededor de sus vertices y ortogonalizada respecto de
    /// sus normales.
    pub fn tangents(&self) -> Vec<[f32; 4]> {
        let mut sum = vec![Vec3::ZERO; self.positions.len()];
        let mut sign = vec![0.0f32; self.positions.len()];
        for t in self.indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize);
            let f = face_tangent(
                [self.positions[a], self.positions[b], self.positions[c]],
                [self.uvs[a], self.uvs[b], self.uvs[c]],
            );
            for v in [a, b, c] {
                sum[v] += Vec3::new(f[0], f[1], f[2]);
                sign[v] += f[3];
            }
        }
        sum.iter()
            .zip(&sign)
            .zip(&self.normals)
            .map(|((t, w), n)| {
                let t = (*t - *n * n.dot(*t)).normalize_or(n.any_orthonormal_vector());
                [t.x, t.y, t.z, if *w < 0.0 { -1.0 } else { 1.0 }]
            })
            .collect()
    }

    pub fn into_mesh(self) -> Mesh {
        let tangents = self.tangents();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            self.positions
                .iter()
                .map(|p| p.to_array())
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            self.normals
                .iter()
                .map(|n| n.to_array())
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Un árbol construido: su corteza y sus hojas, parado en el origen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TreeMesh {
    pub bark: Buffers,
    pub leaves: Buffers,
    /// Su cima, en metros sobre el suelo.
    pub height: f32,
    /// Hasta dónde llegan sus hojas desde el eje del tronco, en metros.
    pub crown_radius: f32,
    /// Qué tan bajo cuelga su hoja más baja sobre el suelo, en metros.
    pub crown_base: f32,
}

/// Un pequeño generador con semilla (SplitMix64): la misma semilla, el mismo
/// árbol.
struct Rng(u64);

impl Rng {
    fn new(species: Species, seed: u32) -> Self {
        Self(u64::from(seed) << 8 | species as u64)
    }

    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, (lo, hi): (f32, f32)) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Un punto en la bola unitaria.
    fn ball(&mut self) -> Vec3 {
        loop {
            let p = Vec3::new(self.unit(), self.unit(), self.unit()) * 2.0 - 1.0;
            if p.length_squared() <= 1.0 {
                return p;
            }
        }
    }
}

/// Un vector unitario horizontal a `angle` radianes de +X.
fn heading(angle: f32) -> Vec3 {
    Vec3::new(angle.cos(), 0.0, angle.sin())
}

/// Un tubo abierto a través de `points` con radios `radii`, con `sides` lados
/// alrededor y su corteza repitiéndose cada `tile` metros (alrededor, a lo
/// largo).
fn tube(out: &mut Buffers, points: &[Vec3], radii: &[f32], sides: u32, tile: Vec2) {
    let wraps = (std::f32::consts::TAU * radii[0] / tile.x).round().max(1.0);
    let mut side = Vec3::ZERO;
    let mut along = 0.0;
    let mut rings = Vec::new();
    for (i, (&p, &r)) in points.iter().zip(radii).enumerate() {
        let ahead = points[(i + 1).min(points.len() - 1)] - points[i.saturating_sub(1)];
        let dir = ahead.normalize_or(Vec3::Y);
        // Arrastrar el inicio del anillo a lo largo de las curvas, para que el
        // tubo nunca se retuerza.
        side = (side - dir * dir.dot(side)).normalize_or(dir.any_orthonormal_vector());
        let up = dir.cross(side);
        if i > 0 {
            along += p.distance(points[i - 1]);
        }
        let ring: Vec<u32> = (0..=sides)
            .map(|k| {
                let a = std::f32::consts::TAU * k as f32 / sides as f32;
                let n = side * a.cos() + up * a.sin();
                out.vertex(
                    p + n * r,
                    n,
                    [wraps * k as f32 / sides as f32, along / tile.y],
                )
            })
            .collect();
        rings.push(ring);
    }
    for pair in rings.windows(2) {
        for k in 0..sides as usize {
            // En sentido antihorario visto desde afuera: subiendo por el tubo,
            // y después alrededor.
            out.quad([pair[0][k], pair[0][k + 1], pair[1][k + 1], pair[1][k]]);
        }
    }
}

/// Un tramo de rama con hojas: la mitad exterior de una rama, una ramita, la
/// parte superior de un eje central. Las tarjetas cuelgan a lo largo de él, más
/// hacia su extremo.
struct Leafy {
    from: Vec3,
    to: Vec3,
}

/// Construye la variante `seed` de `species`.
pub(crate) fn tree(species: Species, seed: u32) -> TreeMesh {
    grow(species.variant_form(seed), Rng::new(species, seed))
}

/// Construye la variante `seed` de `species` podada para una calle
/// ([`Species::street_form`]).
pub(crate) fn street_tree(species: Species, seed: u32) -> TreeMesh {
    grow(species.street_form(), Rng::new(species, seed ^ 0x57ee))
}

/// Hace crecer un árbol de forma `form`, tomando su aleatoriedad de `rng`.
fn grow(form: Form, mut rng: Rng) -> TreeMesh {
    let height = rng.range(form.height);
    let mut bark = Buffers::default();
    let mut leafy = Vec::new();
    let r0 = form.trunk_radius;
    // Una horqueta baja se inclina menos: el tronco es corto, y una inclinación
    // del 4% de la altura lo ladearía mucho (todos los árboles del pueblo se
    // bifurcan por encima de 0.2).
    let lean_angle = rng.range((0.0, std::f32::consts::TAU));
    // Un árbol azotado por el viento se inclina, y sus ramas se extienden, en
    // el mismo sentido.
    let wind_angle = if form.wind > 0.0 {
        rng.range((0.0, std::f32::consts::TAU))
    } else {
        lean_angle
    };
    let lean = heading(wind_angle) * form.lean * height * (form.fork * 5.0).min(1.0);
    let fork = form.fork * height;
    // El tronco, un poco ensanchado al pie, un poco inclinado.
    let top = if form.leader { 0.92 * height } else { fork };
    let steps = if form.leader { 4 } else { 2 };
    let trunk: Vec<Vec3> = (0..=steps)
        .map(|k| {
            let t = k as f32 / steps as f32;
            Vec3::Y * top * t + lean * t * t * (fork / top)
        })
        .collect();
    let radii: Vec<f32> = (0..=steps)
        .map(|k| {
            let t = k as f32 / steps as f32;
            if form.leader {
                r0 * (1.0 - 0.85 * t)
            } else {
                r0 * (1.0 - 0.3 * t)
            }
        })
        .collect();
    tube(&mut bark, &trunk, &radii, TRUNK_SIDES, form.bark_tile);
    // Raíces tablares: desde el tronco abultado hacia afuera y hacia abajo,
    // dentro del suelo, dentro del collider del pie (el tubo de una raíz llega
    // a 1.2 radios del tronco).
    for k in 0..form.buttress {
        let out = heading(
            std::f32::consts::TAU * (k as f32 + rng.range((-0.15, 0.15))) / form.buttress as f32,
        );
        let climb = (0.8 * fork).min(2.0);
        let up = r0 * (1.0 - 0.3 * climb / fork);
        let roots = [
            out * 0.8 * up + Vec3::Y * climb,
            out * r0 * 1.0 + Vec3::Y * climb * 0.45,
            out * r0 * 1.1 + Vec3::Y * r0 * 0.12,
        ];
        tube(
            &mut bark,
            &roots,
            &[r0 * 0.3, r0 * 0.2, r0 * 0.08],
            LIMB_SIDES,
            form.bark_tile,
        );
    }
    if form.leader {
        leafy.push(Leafy {
            from: trunk[steps - 1],
            to: trunk[steps],
        });
    }
    let on_trunk = |h: f32| {
        let t = (h / top).clamp(0.0, 1.0);
        Vec3::Y * h + lean * t * t * (fork / top)
    };
    let limbs = form.limbs.0 + (rng.unit() * (form.limbs.1 - form.limbs.0 + 1) as f32) as u32;
    let limbs = limbs.min(form.limbs.1);
    let turn0 = rng.range((0.0, std::f32::consts::TAU));
    for i in 0..limbs {
        let around =
            turn0 + std::f32::consts::TAU * (i as f32 + rng.range((-0.2, 0.2))) / limbs as f32;
        let out = heading(around);
        // A favor del viento (1) las ramas son largas y bajas; en contra (-1),
        // cortas y empinadas.
        let lee = (around - wind_angle).cos() * form.wind;
        let tilt = (rng.range(form.tilt) + 12.0 * lee).to_radians();
        let length = form.reach * height * rng.range((0.85, 1.1)) * (1.0 + 0.55 * lee);
        // Las ramas se separan alrededor de la horqueta; el ascenso de un eje
        // central, en su mitad superior.
        let h = if form.leader {
            fork + (top - fork) * 0.8 * (i as f32 + rng.unit()) / limbs as f32
        } else {
            fork * rng.range((0.9, 1.05))
        };
        let start = on_trunk(h);
        let dir = out * tilt.sin() + Vec3::Y * tilt.cos();
        let end_dir = (dir + Vec3::Y * form.rise * 2.0).normalize();
        // Una rama en zigzag se arquea fuera de su línea hacia un lado, y
        // después vuelve hacia el otro.
        let zig = heading(around + std::f32::consts::FRAC_PI_2)
            * (form.kink * length * if i % 2 == 0 { 1.0 } else { -1.0 });
        let mid = start + dir * length * 0.5 + zig;
        let end = mid + end_dir * length * 0.5 - zig * 2.0;
        let width = (r0 * 0.55).min(r0 * 0.5 + 0.03);
        // Los tallos de un arbusto se afinan hasta una punta fina; las ramas de
        // un árbol, hasta el grosor de una ramita (según [`Form::shrub`], no
        // según qué tan delgado sea el tronco).
        let (tip, twig_radii) = if form.shrub {
            (width * 0.3, [0.012, 0.005])
        } else {
            (0.035, [0.03, 0.012])
        };
        tube(
            &mut bark,
            &[start - dir * width, mid, end],
            &[width, width * 0.55, tip],
            LIMB_SIDES,
            form.bark_tile,
        );
        leafy.push(Leafy { from: mid, to: end });
        for j in 0..form.twigs {
            let t = 0.35 + 0.6 * (j as f32 + rng.unit()) / form.twigs as f32;
            let from = if t < 0.5 {
                start.lerp(mid, t * 2.0)
            } else {
                mid.lerp(end, t * 2.0 - 1.0)
            };
            let side = if j % 2 == 0 { 1.0 } else { -1.0 };
            let twig_dir = (end_dir
                + heading(around + side * rng.range((0.7, 1.1))) * 0.9
                + Vec3::Y * rng.range((-0.1, 0.4)))
            .normalize();
            let twig = length * rng.range((0.32, 0.45));
            let to = from + twig_dir * twig;
            tube(
                &mut bark,
                &[from, to],
                &twig_radii,
                TWIG_SIDES,
                form.bark_tile,
            );
            leafy.push(Leafy { from, to });
        }
    }
    let leaves = cards(&form, &leafy, &mut rng);
    let height = bark
        .positions
        .iter()
        .chain(&leaves.positions)
        .map(|p| p.y)
        .fold(0.0, f32::max);
    let crown_radius = leaves
        .positions
        .iter()
        .map(|p| Vec2::new(p.x, p.z).length())
        .fold(0.0, f32::max);
    let crown_base = leaves
        .positions
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min);
    TreeMesh {
        bark,
        leaves,
        height,
        crown_radius,
        crown_base,
    }
}

/// Las tarjetas de hojas: [`Form::cards`] de ellas a lo largo de los tramos
/// `leafy`, cada una un ramito del atlas.
fn cards(form: &Form, leafy: &[Leafy], rng: &mut Rng) -> Buffers {
    let mut out = Buffers::default();
    let ends = leafy.iter().map(|l| l.to).sum::<Vec3>() / leafy.len() as f32;
    // El centro de la copa: debajo de sus extremos, a su propia altura.
    let centre = Vec3::new(0.0, ends.y * 0.9, 0.0);
    let total: f32 = leafy.iter().map(|l| l.from.distance(l.to)).sum();
    let side = form.card;
    // Un tramo según su largo.
    let pick_stretch = |rng: &mut Rng| {
        let mut pick = rng.unit() * total;
        leafy
            .iter()
            .find(|l| {
                pick -= l.from.distance(l.to);
                pick <= 0.0
            })
            .unwrap_or(&leafy[leafy.len() - 1])
    };
    // Dónde cuelgan los grupos densos: hacia los extremos de los tramos.
    let clumps: Vec<Vec3> = if form.clump > 0.0 {
        (0..CLUMPS)
            .map(|_| {
                let l = pick_stretch(rng);
                l.from.lerp(l.to, 0.6 + 0.4 * rng.unit())
            })
            .collect()
    } else {
        Vec::new()
    };
    for _ in 0..form.cards {
        let (at, dir) = if !clumps.is_empty() && rng.unit() < form.clump {
            // La tarjeta de un grupo: dentro del alcance de una tarjeta desde
            // su centro, para que las hojas se superpongan en una sola masa.
            let c = clumps[(rng.unit() * CLUMPS as f32) as usize % CLUMPS];
            (
                c + rng.ball() * side * 1.3,
                (c - centre).normalize_or(Vec3::Y),
            )
        } else {
            // Un lugar sobre un tramo, mayormente hacia su extremo.
            let stretch = pick_stretch(rng);
            let t = 0.2 + 0.8 * rng.unit().sqrt();
            (
                stretch.from.lerp(stretch.to, t),
                (stretch.to - stretch.from).normalize_or(Vec3::Y),
            )
        };
        let outward = (at - centre).normalize_or(dir);
        {
            // Hacia dónde crece el ramito desde su base: hacia afuera a lo
            // largo de la rama y lejos de la copa, o colgando.
            let reach = (dir + outward + rng.ball() * 0.9 + Vec3::Y * 0.3).normalize_or(Vec3::Y);
            let up = reach.lerp(Vec3::NEG_Y, form.hang).normalize_or(Vec3::NEG_Y);
            let right = up
                .cross(rng.ball() + Vec3::new(0.01, 0.0, 0.0))
                .normalize_or(up.any_orthonormal_vector());
            let face = right.cross(up);
            let base = at + rng.ball() * form.scatter * side + outward * side * 0.1;
            let bottom = base - up * SPRIG_BASE * side;
            let quarter = (rng.unit() * 4.0) as u32 % 4;
            let (u0, v0) = ((quarter % 2) as f32 * 0.5, (quarter / 2) as f32 * 0.5);
            let mut rows = [[0u32; 2]; 3];
            for (r, row) in rows.iter_mut().enumerate() {
                let t = r as f32 / 2.0;
                for (s, slot) in row.iter_mut().enumerate() {
                    let across = s as f32 - 0.5;
                    let p = bottom + right * across * side + up * t * side
                        - Vec3::Y * form.sag * side * t * t;
                    // Inclinar la normal hacia afuera desde el centro de la
                    // copa.
                    let facing = if face.dot(p - centre) < 0.0 {
                        -face
                    } else {
                        face
                    };
                    let n = (facing * 0.4 + (p - centre).normalize_or(Vec3::Y)).normalize();
                    *slot = out.vertex(p, n, [u0 + s as f32 * 0.5, v0 + (1.0 - t) * 0.5]);
                }
            }
            for k in 0..2 {
                let ([a, b], [d, c]) = (rows[k], rows[k + 1]);
                out.quad([a, b, c, d]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::VARIANTS;

    fn all() -> Vec<(Species, u32, TreeMesh)> {
        Species::ALL
            .iter()
            .flat_map(|&s| (0..VARIANTS as u32).map(move |v| (s, v, tree(s, v))))
            .collect()
    }

    #[test]
    fn every_tree_keeps_to_the_triangle_budget() {
        for (species, v, t) in all() {
            let (bark, leaves) = (t.bark.triangles(), t.leaves.triangles());
            println!("{species:?} {v}: {bark} bark + {leaves} leaves");
            assert!(bark + leaves <= 1000, "{species:?} {v}: {bark} + {leaves}");
            assert!(bark + leaves >= 500, "{species:?} {v}: {bark} + {leaves}");
            assert!(bark >= 100 && leaves >= 300, "{species:?} {v}");
            for b in [&t.bark, &t.leaves] {
                let n = b.positions.len();
                assert_eq!(b.normals.len(), n);
                assert_eq!(b.uvs.len(), n);
                assert!(b.indices.iter().all(|&i| (i as usize) < n));
            }
        }
    }

    #[test]
    fn a_variant_is_its_seed_and_the_seeds_differ() {
        for &species in &Species::ALL {
            assert_eq!(tree(species, 1), tree(species, 1));
            assert_ne!(tree(species, 1).leaves, tree(species, 2).leaves);
        }
    }

    #[test]
    fn the_trunk_stands_within_its_collider() {
        for (species, v, t) in all() {
            for p in t.bark.positions.iter().filter(|p| p.y < 0.5) {
                let r = Vec2::new(p.x, p.z).length();
                // Tan ancho como lo dibuja cualquier plaza.
                assert!(
                    r * crate::town::TREE_SCALE_MAX <= pampa_core::town::TREE_TRUNK_RADIUS,
                    "{species:?} {v}: {r}"
                );
            }
            // Y la corteza llega al suelo.
            let foot = t.bark.positions.iter().map(|p| p.y).fold(1.0, f32::min);
            assert!(foot.abs() < 0.02, "{species:?} {v}: {foot}");
        }
    }

    #[test]
    fn each_species_keeps_its_shape() {
        let mean = |s: Species, f: fn(&TreeMesh) -> f32| {
            (0..VARIANTS as u32).map(|v| f(&tree(s, v))).sum::<f32>() / VARIANTS as f32
        };
        let h = |s| mean(s, |t| t.height);
        let w = |s| mean(s, |t| 2.0 * t.crown_radius);
        for s in Species::ALL {
            println!("{s:?}: {:.1} m tall, {:.1} m wide", h(s), w(s));
        }
        // El eucalipto sobresale por encima del resto; el aromo es el más bajo.
        for s in [Species::Paraiso, Species::Olmo, Species::Aromo] {
            assert!(h(Species::Eucalipto) > h(s) + 2.0, "{s:?}");
            if s != Species::Aromo {
                assert!(h(s) > h(Species::Aromo) + 1.0, "{s:?}");
            }
        }
        // La sombrilla del paraíso es más ancha que alta; el florero del olmo y
        // el eucalipto son más altos que anchos.
        assert!(w(Species::Paraiso) > h(Species::Paraiso));
        assert!(h(Species::Olmo) > w(Species::Olmo));
        assert!(h(Species::Eucalipto) > 1.5 * w(Species::Eucalipto));
        // El olmo es un florero: sus hojas se extienden más arriba que abajo.
        let t = tree(Species::Olmo, 0);
        let spread = |lo: f32, hi: f32| {
            t.leaves
                .positions
                .iter()
                .filter(|p| p.y >= lo * t.height && p.y < hi * t.height)
                .map(|p| Vec2::new(p.x, p.z).length())
                .fold(0.0, f32::max)
        };
        assert!(spread(0.7, 1.0) > spread(0.3, 0.55), "a vase");
        // Las tarjetas del eucalipto cuelgan: la mayoría de los ramitos apuntan
        // hacia abajo.
        let t = tree(Species::Eucalipto, 0);
        let down = t
            .leaves
            .positions
            .chunks(6)
            .filter(|c| c[4].y < c[0].y)
            .count();
        assert!(down * 4 > t.leaves.positions.len() / 6 * 3, "{down}");
    }

    #[test]
    fn leaf_normals_lean_out_of_the_crown() {
        for (species, v, t) in all() {
            let p = &t.leaves.positions;
            let centre = p.iter().sum::<Vec3>() / p.len() as f32;
            let out = p
                .iter()
                .zip(&t.leaves.normals)
                .filter(|(p, n)| n.dot(**p - centre) > 0.0)
                .count();
            assert!(
                out * 10 >= p.len() * 9,
                "{species:?} {v}: {out} of {}",
                p.len()
            );
            assert!(
                t.leaves
                    .normals
                    .iter()
                    .all(|n| (n.length() - 1.0).abs() < 1e-4)
            );
        }
    }

    #[test]
    fn each_card_shows_one_sprig_hung_by_its_base() {
        let t = tree(Species::Paraiso, 0);
        for card in t.leaves.uvs.chunks(6) {
            let (u0, v0) = (card[0][0], card[0][1] - 0.5);
            assert!(u0 == 0.0 || u0 == 0.5, "{card:?}");
            assert!(v0 == 0.0 || v0 == 0.5, "{card:?}");
            for uv in card {
                assert!((u0..=u0 + 0.5).contains(&uv[0]), "{card:?}");
                assert!((v0..=v0 + 0.5).contains(&uv[1]), "{card:?}");
            }
            // La fila de la punta muestrea la parte superior del cuarto.
            assert_eq!(card[4][1], v0);
        }
        // Se usan todos los cuartos.
        let quarters: std::collections::HashSet<_> = t
            .leaves
            .uvs
            .chunks(6)
            .map(|c| (c[0][0].to_bits(), c[0][1].to_bits()))
            .collect();
        assert_eq!(quarters.len(), 4);
    }

    #[test]
    fn bark_repeats_whole_times_round_and_tangents_follow_it() {
        for (species, v, t) in all() {
            let b = &t.bark;
            for (tangent, n) in b.tangents().iter().zip(&b.normals) {
                let t3 = Vec3::new(tangent[0], tangent[1], tangent[2]);
                assert!((t3.length() - 1.0).abs() < 1e-3, "{species:?} {v}");
                assert!(t3.dot(*n).abs() < 1e-3, "{species:?} {v}");
            }
            // La costura de un tubo: el último vertex de cada anillo coincide
            // con el primero, con un número entero de repeticiones entre ellos.
            let u_max = b.uvs.iter().map(|uv| uv[0]).fold(0.0, f32::max);
            assert_eq!(u_max, u_max.round(), "{species:?} {v}");
        }
    }

    fn field() -> Vec<(Species, u32, TreeMesh)> {
        Species::FIELD
            .iter()
            .flat_map(|&s| (0..s.variants() as u32).map(move |v| (s, v, tree(s, v))))
            .collect()
    }

    fn live(species: Species, v: u32) -> bool {
        species != Species::Calden || (v as usize) < crate::tree::CALDEN_LIVE
    }

    /// Lo que distingue la silueta de una forma: su altura, el ancho de su
    /// copa, cuánto se aparta el centro de la copa del tronco, qué tan bajo
    /// cuelga (como fracción de la altura), cuántas hojas lleva y cuánto se
    /// extiende la corteza en la mitad inferior, como fracción de la altura
    /// (tallos que se abren en abanico desde el suelo frente a un solo tronco).
    fn outline(t: &TreeMesh) -> [f32; 6] {
        let p = &t.leaves.positions;
        let middle = p.iter().map(|p| Vec2::new(p.x, p.z)).sum::<Vec2>() / p.len() as f32;
        let spread = t
            .bark
            .positions
            .iter()
            .filter(|p| (0.15..0.5).contains(&(p.y / t.height)))
            .map(|p| Vec2::new(p.x, p.z).distance(middle))
            .fold(0.0, f32::max);
        [
            t.height,
            2.0 * t.crown_radius,
            middle.length(),
            t.crown_base / t.height,
            t.leaves.triangles() as f32,
            spread / t.height,
        ]
    }

    #[test]
    fn field_trees_keep_to_the_town_trees_triangle_budget() {
        for (species, v, t) in field() {
            let (bark, leaves) = (t.bark.triangles(), t.leaves.triangles());
            println!("{species:?} {v}: {bark} bark + {leaves} leaves");
            assert!(bark + leaves <= 1200, "{species:?} {v}: {bark} + {leaves}");
            if live(species, v) {
                assert!(bark + leaves >= 650, "{species:?} {v}: {bark} + {leaves}");
            }
            assert!(bark >= 100 && leaves >= 16, "{species:?} {v}");
            for b in [&t.bark, &t.leaves] {
                let n = b.positions.len();
                assert_eq!(b.normals.len(), n);
                assert_eq!(b.uvs.len(), n);
                assert!(b.indices.iter().all(|&i| (i as usize) < n));
            }
        }
    }

    #[test]
    fn field_variants_are_seeded_and_all_differ() {
        assert_eq!(Species::Calden.variants(), 12);
        assert_eq!(Species::Chanar.variants(), 6);
        for species in Species::FIELD {
            let n = species.variants() as u32;
            for v in 0..n {
                assert_eq!(tree(species, v), tree(species, v));
                for w in v + 1..n {
                    assert_ne!(tree(species, v).bark, tree(species, w).bark);
                    assert_ne!(tree(species, v).leaves, tree(species, w).leaves);
                }
            }
        }
    }

    #[test]
    fn the_forms_of_a_field_species_differ_in_outline_pairwise() {
        for species in Species::FIELD {
            let forms: Vec<(u32, [f32; 6])> = (0..species.variants() as u32)
                .filter(|&v| live(species, v))
                .map(|v| (v, outline(&tree(species, v))))
                .collect();
            for (v, o) in &forms {
                println!("{species:?} {v}: {o:.2?}");
            }
            // Al menos 8 formas vivas de caldén, 4 de ombú y 5 de cada una de
            // las demás.
            let least = match species {
                Species::Calden => 8,
                Species::Ombu => 4,
                _ => 5,
            };
            assert!(forms.len() >= least, "{species:?}");
            for (i, (a, oa)) in forms.iter().enumerate() {
                for (b, ob) in &forms[i + 1..] {
                    // Claramente distintas en algún rasgo: por más de un quinto
                    // de él (la inclinación, por más de un décimo de la
                    // altura).
                    let apart = (0..6).any(|k| {
                        // La inclinación y la extensión de la corteza, un
                        // décimo de la altura o menos; el resto, un quinto del
                        // mayor.
                        let scale = match k {
                            2 => 0.1 * oa[0],
                            3 | 5 => 0.04,
                            _ => 0.2 * oa[k].max(ob[k]),
                        };
                        (oa[k] - ob[k]).abs() > scale
                    });
                    assert!(apart, "{species:?} forms {a} and {b}: {oa:?} {ob:?}");
                }
            }
        }
        // Algunas están inclinadas o son asimétricas (el centro de la copa bien
        // apartado del tronco), algunas son altas y angostas, algunas anchas y
        // planas.
        let calden: Vec<[f32; 6]> = (0..crate::tree::CALDEN_LIVE as u32)
            .map(|v| outline(&tree(Species::Calden, v)))
            .collect();
        let ratio = |o: &[f32; 6]| o[1] / o[0];
        assert!(calden.iter().filter(|o| o[2] > 0.1 * o[0]).count() >= 3);
        assert!(calden.iter().any(|o| ratio(o) < 1.0), "a tall narrow form");
        assert!(calden.iter().any(|o| ratio(o) > 2.2), "a broad flat form");
        // Las formas moldeadas por el viento (3 y 8) quedan asimétricas, bien
        // apartadas de sus troncos; la sombrilla clásica queda sobre el suyo.
        for v in [3, 8] {
            assert!(
                calden[v][2] > 0.25 * calden[v][0],
                "form {v}: {:?}",
                calden[v]
            );
        }
        assert!(calden[0][2] < 0.15 * calden[0][0], "{:?}", calden[0]);
        // Ninguna copa viva se mete en el suelo.
        assert!(calden.iter().all(|o| o[3] > -0.06), "{calden:?}");
        // Los árboles secos tienen pocas hojas, y un tocón queda bajo.
        let stub = tree(Species::Calden, 11);
        let live_height = calden.iter().map(|o| o[0]).fold(f32::MAX, f32::min);
        assert!(stub.height < live_height);
    }

    #[test]
    fn field_trunks_stand_within_the_collision_core_plants_for_them() {
        use pampa_core::flora::Age;
        use pampa_core::{Species as Core, Tree, Vec2 as Ground};
        // Las especies del pueblo que se escaparon al campo también quedan
        // dentro de su radio de core.
        let town = Species::ALL
            .iter()
            .flat_map(|&s| (0..s.variants() as u32).map(move |v| (s, v, tree(s, v))));
        for (species, v, t) in field().into_iter().chain(town) {
            let core = crate::flora::core_species(species);
            for age in Age::ALL {
                // Un ombú nunca se planta como renoval.
                if core == Core::Ombu && age == Age::Sapling {
                    continue;
                }
                // Solo un caldén maduro llega a estar seco.
                if !live(species, v) && age != Age::Mature {
                    continue;
                }
                let (lo, hi) = age.height_range(core);
                // El más ancho al que se planta la edad.
                let width = match (core, age) {
                    // El ancho de un ombú se mantiene modesto a cualquier edad.
                    (Core::Ombu, _) => 1.15,
                    (_, Age::Old) => 1.4,
                    _ => 1.18,
                };
                for h in [lo, 0.5 * (lo + hi), hi] {
                    let planted = Tree {
                        at: Ground::ZERO,
                        species: core,
                        age,
                        height: h,
                        width,
                        yaw: 0.0,
                        variant: 0,
                    };
                    let scale = h / t.height;
                    // Cada vertex de corteza en el primer quinto de metro sobre
                    // el suelo (las ramas de un renoval se separan a medio
                    // metro de altura).
                    for p in t.bark.positions.iter().filter(|p| p.y * scale < 0.2) {
                        let r = Vec2::new(p.x, p.z).length() * scale * width;
                        assert!(
                            r <= planted.trunk_radius(),
                            "{species:?} {v} {age:?} at {h} m: {r} over {}",
                            planted.trunk_radius()
                        );
                    }
                }
            }
            let foot = t.bark.positions.iter().map(|p| p.y).fold(1.0, f32::min);
            // Un tronco inclinado ladea su primer anillo un poco bajo el suelo.
            assert!(foot.abs() < 0.06, "{species:?} {v}: {foot}");
        }
    }

    #[test]
    fn the_calden_is_a_broad_low_forking_crown_and_the_chanar_a_small_stand_of_stems() {
        let mean = |s: Species, f: fn(&TreeMesh) -> f32| {
            let n = (0..s.variants() as u32).filter(|&v| live(s, v));
            let all: Vec<f32> = n.map(|v| f(&tree(s, v))).collect();
            all.iter().sum::<f32>() / all.len() as f32
        };
        let calden = Species::Calden;
        // Más ancho que alto en conjunto, y sus hojas cuelgan de ramas bajas.
        let (h, w) = (
            mean(calden, |t| t.height),
            mean(calden, |t| 2.0 * t.crown_radius),
        );
        println!("caldén {h:.1} x {w:.1}");
        assert!(w > 1.15 * h, "{w} wide, {h} tall");
        assert!(mean(calden, |t| t.crown_base / t.height) < 0.45);
        // Hojas pequeñas, no más gruesas que las plumosas del aromo, y todavía
        // más pequeñas en el chañar.
        // (Las hojas grandes y brillantes del ombú son la única excepción.)
        for s in Species::FIELD.into_iter().filter(|&s| s != Species::Ombu) {
            assert!(s.form().card <= Species::Aromo.form().card, "{s:?}");
        }
        assert!(Species::Chanar.form().card < 0.6 * Species::Calden.form().card);
        let leaves = |s| mean(s, |t| t.leaves.triangles() as f32);
        assert!(leaves(Species::Chanar) < 0.8 * leaves(calden));
        // El chañar es un árbol pequeño de varios tallos: se abren en abanico
        // abajo, bastante más allá del ancho de la cepa de la que salen.
        for v in 0..Species::Chanar.variants() as u32 {
            let t = tree(Species::Chanar, v);
            let spread = |lo: f32, hi: f32| {
                t.bark
                    .positions
                    .iter()
                    .filter(|p| p.y >= lo * t.height && p.y < hi * t.height)
                    .map(|p| Vec2::new(p.x, p.z).length())
                    .fold(0.0, f32::max)
            };
            assert!(spread(0.35, 0.6) > 2.0 * spread(0.0, 0.05).max(0.05), "{v}");
            // Varios tallos ya están separados a un tercio de la altura:
            // corteza bien apartada del eje, en al menos tres de ocho sectores
            // alrededor.
            let mut sectors = [false; 8];
            for p in t.bark.positions.iter() {
                let off = Vec2::new(p.x, p.z);
                if (0.25 * t.height..0.55 * t.height).contains(&p.y)
                    && off.length() > 0.04 * t.height
                {
                    let turn = off.y.atan2(off.x) / std::f32::consts::TAU;
                    sectors[((turn + 1.0).fract() * 8.0) as usize % 8] = true;
                }
            }
            let stems = sectors.iter().filter(|&&s| s).count();
            assert!(stems >= 3, "{v}: bark on {stems} sectors");
        }
        // Las últimas variantes del caldén están muertas o ralas: pocas hojas.
        let full = leaves(calden);
        for v in crate::tree::CALDEN_LIVE as u32..calden.variants() as u32 {
            let t = tree(calden, v);
            assert!((t.leaves.triangles() as f32) < 0.35 * full, "{v}");
            assert!(t.leaves.triangles() >= 16, "{v}");
        }
    }

    #[test]
    fn field_leaf_normals_lean_out_and_bark_tiles_whole() {
        for (species, v, t) in field() {
            let p = &t.leaves.positions;
            let centre = p.iter().sum::<Vec3>() / p.len() as f32;
            let out = p
                .iter()
                .zip(&t.leaves.normals)
                .filter(|(p, n)| n.dot(**p - centre) > 0.0)
                .count();
            assert!(
                out * 10 >= p.len() * 8,
                "{species:?} {v}: {out} of {}",
                p.len()
            );
            let u_max = t.bark.uvs.iter().map(|uv| uv[0]).fold(0.0, f32::max);
            assert_eq!(u_max, u_max.round(), "{species:?} {v}");
        }
    }

    #[test]
    fn field_bark_repeats_coarsely_so_its_furrows_survive_the_mipmaps() {
        // Las primeras formas de campo repetían la corteza cada 0.5 x 0.9 m (el
        // caldén) y 0.5 x 0.8 m (el chañar): demasiado fino para leerse a la
        // distancia. Ahora una repetición abarca más de un metro alrededor y
        // unos 1.5 m a lo largo, y los árboles del pueblo conservan la suya.
        for species in Species::FIELD {
            for v in 0..species.variants() as u32 {
                let tile = species.variant_form(v).bark_tile;
                assert!(tile.x >= 0.8 && tile.y >= 1.5, "{species:?} {v}: {tile}");
            }
        }
        assert_eq!(Species::Paraiso.form().bark_tile, Vec2::ONE);
        assert_eq!(Species::Olmo.form().bark_tile, Vec2::new(0.8, 1.6));
        // La repetición completa da una vuelta alrededor del tronco de un
        // caldén, y la corteza de un tronco se estira a lo largo de menos de
        // una repetición y media.
        let t = tree(Species::Calden, 0);
        let foot: Vec<[f32; 2]> = t
            .bark
            .positions
            .iter()
            .zip(&t.bark.uvs)
            .filter(|(p, _)| p.y < 0.01)
            .map(|(_, uv)| *uv)
            .collect();
        assert!(!foot.is_empty());
        assert_eq!(foot.iter().map(|uv| uv[0]).fold(0.0, f32::max), 1.0);
        let along = t.bark.uvs.iter().map(|uv| uv[1]).fold(0.0, f32::max);
        assert!(along < 2.5, "{along}");
    }

    /// La proporción del área de la copa (vista de costado, sobre las celdas al
    /// alcance de una tarjeta) que cubre una hoja.
    fn crown_cover(t: &TreeMesh) -> f32 {
        let cell = 0.12;
        let tris: Vec<[Vec2; 3]> = t
            .leaves
            .indices
            .chunks(3)
            .map(|i| {
                [i[0], i[1], i[2]].map(|i| {
                    let p = t.leaves.positions[i as usize];
                    Vec2::new(p.x, p.y)
                })
            })
            .collect();
        let inside = |q: Vec2, [a, b, c]: [Vec2; 3]| {
            let side = |u: Vec2, v: Vec2| (v - u).perp_dot(q - u);
            let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
            !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
        };
        let centres: Vec<Vec2> = tris.iter().map(|t| (t[0] + t[1] + t[2]) / 3.0).collect();
        let (mut zone, mut covered) = (0, 0);
        let (lo, hi) = (
            tris.iter().flatten().fold(Vec2::MAX, |m, p| m.min(*p)),
            tris.iter().flatten().fold(Vec2::MIN, |m, p| m.max(*p)),
        );
        let mut y = lo.y;
        while y < hi.y {
            let mut x = lo.x;
            while x < hi.x {
                let q = Vec2::new(x, y);
                // La zona: cerca de las hojas, no afuera en el cielo.
                if centres.iter().any(|c| c.distance(q) < 0.9) {
                    zone += 1;
                    covered += usize::from(tris.iter().any(|t| inside(q, *t)));
                }
                x += cell;
            }
            y += cell;
        }
        covered as f32 / zone as f32
    }

    #[test]
    fn live_crowns_are_denser_than_the_first_field_crowns_with_fewer_holes() {
        // El primer caldén de F1: 150 tarjetas de 0.9 m dispersas a lo largo de
        // las ramas.
        let old = Form {
            cards: 150,
            card: 0.9,
            hang: 0.25,
            scatter: 0.35,
            clump: 0.0,
            ..Species::Calden.form()
        };
        let old = (0..4u32)
            .map(|v| crown_cover(&grow(old, Rng::new(Species::Calden, v))))
            .sum::<f32>()
            / 4.0;
        let new: Vec<f32> = (0..crate::tree::CALDEN_LIVE as u32)
            .map(|v| crown_cover(&tree(Species::Calden, v)))
            .collect();
        let mean = new.iter().sum::<f32>() / new.len() as f32;
        println!("crown cover: old {old:.2}, new {mean:.2} {new:.2?}");
        assert!(mean > old + 0.08, "{mean} against {old}");
        assert!(new.iter().all(|&c| c > old), "{new:?}");
        // La mayor parte de una copa viva es hoja.
        assert!(new.iter().filter(|&&c| c > 0.6).count() >= 5, "{new:?}");
    }

    #[test]
    fn the_towns_trees_keep_the_neutral_form_fields() {
        for species in Species::ALL {
            let f = species.form();
            assert_eq!((f.lean, f.wind, f.clump), (0.04, 0.0, 0.0), "{species:?}");
            // Cada variante de una especie del pueblo es su única forma.
            assert_eq!(species.variant_form(2).height, f.height);
        }
    }

    /// El área total de una mesh de corteza: una rama más delgada tiene menos.
    fn bark_area(t: &TreeMesh) -> f32 {
        let b = &t.bark;
        b.indices
            .chunks(3)
            .map(|c| {
                let [p, q, r] = [c[0], c[1], c[2]].map(|i| b.positions[i as usize]);
                (q - p).cross(r - p).length() / 2.0
            })
            .sum()
    }

    #[test]
    fn only_a_shrub_form_gets_fine_stem_tips_whatever_its_trunk() {
        // Un árbol de tronco delgado (una forma de chañar a 4 cm) y la misma
        // forma marcada como arbusto, crecidos con la misma semilla: los tallos
        // del arbusto terminan más finos y las ramas del árbol conservan el
        // grosor de una ramita.
        let thin = Form {
            trunk_radius: 0.04,
            ..Species::Chanar.form()
        };
        let tree_area = bark_area(&grow(thin, Rng::new(Species::Chanar, 0)));
        let shrub = Form {
            shrub: true,
            ..thin
        };
        let shrub_area = bark_area(&grow(shrub, Rng::new(Species::Chanar, 0)));
        assert!(
            tree_area > shrub_area * 1.05,
            "tree {tree_area} vs shrub {shrub_area}"
        );
        // Un arbusto real conserva sus puntas finas: sin marcar, crecería más
        // grueso.
        let piquillin = Species::Piquillin.form();
        let fine = bark_area(&grow(piquillin, Rng::new(Species::Piquillin, 0)));
        let plain = Form {
            shrub: false,
            ..piquillin
        };
        let thick = bark_area(&grow(plain, Rng::new(Species::Piquillin, 0)));
        assert!(thick > fine * 1.05, "unmarked {thick} vs shrub {fine}");
    }

    #[test]
    fn the_shrub_species_are_the_shrub_forms_and_no_tree_is() {
        for species in Species::EVERY {
            let shrub = Species::SHRUBS.contains(&species);
            for v in 0..species.variants() as u32 {
                assert_eq!(species.variant_form(v).shrub, shrub, "{species:?} {v}");
            }
            assert_eq!(species.form().shrub, shrub, "{species:?}");
            // Una forma de calle conserva el carácter de arbusto de su especie.
            assert_eq!(species.street_form().shrub, shrub, "{species:?}");
        }
    }

    fn shrubs() -> Vec<(Species, u32, TreeMesh)> {
        Species::SHRUBS
            .iter()
            .flat_map(|&s| (0..s.variants() as u32).map(move |v| (s, v, tree(s, v))))
            .collect()
    }

    #[test]
    fn shrubs_are_low_multi_stem_forms_within_their_triangle_budget() {
        for (species, v, t) in shrubs() {
            let tris = t.bark.triangles() + t.leaves.triangles();
            println!(
                "{species:?} {v}: {tris} triangles, {:.2} m high, {:.2} m reach",
                t.height, t.crown_radius
            );
            // Barato, para que una maraña de ellos entre en el frame: 200-500.
            assert!((200..=500).contains(&tris), "{species:?} {v}: {tris}");
            // Bajo: muy por debajo de cualquier árbol, y más ancho que el pie
            // de los tallos.
            assert!(
                (0.9..3.0).contains(&t.height),
                "{species:?} {v}: {}",
                t.height
            );
            assert!(
                t.crown_radius > 0.4 && t.crown_radius < 2.2,
                "{species:?} {v}"
            );
            // Muchos tallos desde el suelo, no un solo tronco: las hojas
            // cuelgan bajas.
            assert!(t.crown_base < 0.7 * t.height, "{species:?} {v}");
            assert!(!t.leaves.positions.is_empty() && !t.bark.positions.is_empty());
        }
    }

    #[test]
    fn shrub_variants_are_seeded_and_differ_in_shape() {
        for species in Species::SHRUBS {
            let built: Vec<TreeMesh> = (0..species.variants() as u32)
                .map(|v| tree(species, v))
                .collect();
            // La misma semilla construye el mismo arbusto.
            assert_eq!(built[2], tree(species, 2), "{species:?}");
            for (i, a) in built.iter().enumerate() {
                for b in &built[i + 1..] {
                    assert_ne!(a.bark.positions, b.bark.positions, "{species:?}");
                }
            }
            // Las tres formas difieren en altura: baja y extendida, simple, y
            // alta y densa.
            let (plain, low, tall) = (&built[0], &built[1], &built[2]);
            assert!(
                low.height < plain.height && plain.height < tall.height,
                "{species:?}"
            );
        }
    }

    #[test]
    fn each_shrub_species_stands_apart_from_the_others() {
        let mean = |s: Species, f: fn(&TreeMesh) -> f32| {
            let v: Vec<f32> = (0..s.variants() as u32).map(|v| f(&tree(s, v))).collect();
            v.iter().sum::<f32>() / v.len() as f32
        };
        // Jarilla: tallos erguidos, así que es angosta para su altura; el
        // piquillín, una masa ancha y densa de hojas diminutas; el fachinal, el
        // más alto.
        let narrow = |s| mean(s, |t| t.crown_radius / t.height);
        assert!(narrow(Species::Jarilla) < narrow(Species::Piquillin));
        assert!(mean(Species::Fachinal, |t| t.height) > mean(Species::Piquillin, |t| t.height));
        assert!(mean(Species::Piquillin, |t| t.leaves.triangles() as f32) > 0.0);
    }
    #[test]
    fn the_alamo_is_a_tall_narrow_column_within_the_field_triangle_budget() {
        for v in 0..Species::Alamo.variants() as u32 {
            let t = tree(Species::Alamo, v);
            let tris = t.bark.triangles() + t.leaves.triangles();
            assert!((200..=1200).contains(&tris), "variant {v}: {tris}");
            // La columna de un álamo de Lombardía: al menos 3.5 veces más alta
            // que ancha, mientras que el eucalipto lo es alrededor de
            // 1.5 veces.
            assert!(
                t.height >= 3.5 * 2.0 * t.crown_radius,
                "variant {v}: {} m tall, {} m wide",
                t.height,
                2.0 * t.crown_radius
            );
            // Hojas desde el centro de las ramas hasta la cima, con el fuste
            // inferior desnudo como en una cortina rompevientos que se poda
            // sola.
            assert!(
                t.crown_base < 0.55 * t.height,
                "variant {v}: {}",
                t.crown_base
            );
            assert!(t.height > 12.0, "variant {v}: {}", t.height);
        }
    }

    #[test]
    fn the_alamos_trunk_stands_within_what_core_collides_for_a_poplar() {
        for v in 0..Species::Alamo.variants() as u32 {
            let t = tree(Species::Alamo, v);
            let foot = t
                .bark
                .positions
                .iter()
                .filter(|p| p.y < 0.5)
                .map(|p| Vec2::new(p.x, p.z).length())
                .fold(0.0, f32::max);
            // El radio de core para un álamo de esta altura, a cualquier
            // escala.
            let core = 0.012 * t.height;
            assert!(foot <= core, "variant {v}: bark {foot}, core {core}");
        }
    }

    /// La media de `f` sobre las formas de una especie.
    fn mean_of(s: Species, f: impl Fn(&TreeMesh) -> f32) -> f32 {
        let all: Vec<f32> = (0..s.variants() as u32).map(|v| f(&tree(s, v))).collect();
        all.iter().sum::<f32>() / all.len() as f32
    }

    /// La corteza más lejana del eje por debajo de la altura `y`, en metros.
    fn foot(t: &TreeMesh, y: f32) -> f32 {
        t.bark
            .positions
            .iter()
            .filter(|p| p.y < y)
            .map(|p| Vec2::new(p.x, p.z).length())
            .fold(0.0, f32::max)
    }

    #[test]
    fn the_new_field_species_keep_to_their_triangle_budget_and_wear_a_form_each() {
        for species in [
            Species::Algarrobo,
            Species::Tala,
            Species::Espinillo,
            Species::Ombu,
        ] {
            for v in 0..species.variants() as u32 {
                let t = tree(species, v);
                let tris = t.bark.triangles() + t.leaves.triangles();
                assert!((200..=1200).contains(&tris), "{species:?} {v}: {tris}");
            }
        }
    }

    #[test]
    fn the_algarrobo_is_low_wide_and_multi_stem() {
        let (h, w) = (
            mean_of(Species::Algarrobo, |t| t.height),
            mean_of(Species::Algarrobo, |t| 2.0 * t.crown_radius),
        );
        println!("algarrobo {h:.1} x {w:.1}");
        assert!(h < 6.0 && w > 1.6 * h, "{h} tall, {w} wide");
        // Varios tallos se abren en abanico desde abajo, más allá de la cepa de
        // la que salen.
        let spread = mean_of(Species::Algarrobo, |t| foot(t, 0.5 * t.height) / t.height);
        assert!(spread > 0.25, "{spread}");
        // Más bajo que los caldenes vecinos entre los que crece.
        assert!(h < mean_of(Species::Calden, |t| t.height) + 1.0);
    }

    #[test]
    fn the_tala_zigzags_into_an_irregular_round_crown() {
        // Sus ramas se arquean fuera de su línea y vuelven; sin eso la misma
        // semilla hace crecer ramas rectas.
        let form = Species::Tala.form();
        assert!(form.kink >= 0.15);
        for other in Species::EVERY.into_iter().filter(|&s| s != Species::Tala) {
            assert!(other.form().kink < 0.1, "{other:?}");
        }
        let zig = grow(form, Rng::new(Species::Tala, 0));
        let straight = grow(Form { kink: 0.0, ..form }, Rng::new(Species::Tala, 0));
        assert_ne!(zig.bark, straight.bark);
        // El zigzag de las ramas aparta del tronco el alcance de la corteza.
        let reach = |t: &TreeMesh| foot(t, f32::MAX);
        assert!(
            reach(&zig) > 1.02 * reach(&straight),
            "{} {}",
            reach(&zig),
            reach(&straight)
        );
        // Una copa irregular: el centro de las hojas se aparta del tronco en
        // algunas formas, y ninguna forma es una columna.
        let (w, h) = (
            mean_of(Species::Tala, |t| 2.0 * t.crown_radius),
            mean_of(Species::Tala, |t| t.height),
        );
        assert!(w > 1.3 * h, "{w} wide, {h} tall");
        assert!(
            (0..Species::Tala.variants() as u32).any(|v| outline(&tree(Species::Tala, v))[2] > 0.5)
        );
    }

    #[test]
    fn the_espinillo_is_the_smallest_rounded_crown() {
        let h = |s| mean_of(s, |t| t.height);
        for s in [
            Species::Calden,
            Species::Algarrobo,
            Species::Tala,
            Species::Ombu,
        ] {
            assert!(h(Species::Espinillo) < h(s), "{s:?}");
        }
        assert!(Species::Espinillo.variants() >= 5);
        for v in 0..Species::Espinillo.variants() as u32 {
            let t = tree(Species::Espinillo, v);
            // Un árbol pequeño de menos de 5 m, con la copa más o menos tan
            // ancha como alta o un poco más ancha: redonda, ni una columna ni
            // una sombrilla plana.
            let ratio = 2.0 * t.crown_radius / t.height;
            assert!(
                t.height < 5.0 && (0.8..2.4).contains(&ratio),
                "{v}: {} {ratio}",
                t.height
            );
        }
    }

    #[test]
    fn the_ombu_has_a_huge_buttressed_trunk_under_a_broad_dense_dome() {
        let form = Species::Ombu.form();
        assert_eq!(form.buttress, 5);
        for other in Species::EVERY.into_iter().filter(|&s| s != Species::Ombu) {
            assert_eq!(other.form().buttress, 0, "{other:?}");
        }
        for v in 0..Species::Ombu.variants() as u32 {
            let t = tree(Species::Ombu, v);
            // Las raíces se abren más allá del tronco al pie.
            let plain = grow(
                Form {
                    buttress: 0,
                    ..Species::Ombu.variant_form(v)
                },
                Rng::new(Species::Ombu, v),
            );
            let (roots, trunk) = (foot(&t, 0.3), foot(&plain, 0.3));
            assert!(roots > 1.08 * trunk, "{v}: {roots} against {trunk}");
            // Un tronco enorme: más robusto en relación con su altura que el de
            // cualquier otro árbol.
            for other in [Species::Calden, Species::Eucalipto, Species::Algarrobo] {
                let o = tree(other, 0);
                assert!(
                    foot(&t, 0.3) / t.height > 2.0 * foot(&o, 0.3) / o.height,
                    "{v} against {other:?}"
                );
            }
            // Una cúpula ancha de hojas grandes, densa.
            assert!(t.crown_radius * 2.0 > 1.2 * t.height, "{v}");
            assert!(t.leaves.triangles() >= 560, "{v}");
        }
        assert!(Species::Ombu.form().card >= 1.8);
    }
}
