//! Distribución de los asentamientos: cuadrícula de calles, plaza, vía férrea y
//! huellas de los edificios.
//!
//! Datos puros. Una [`TownConfig`] describe un pueblo (tamaño de la cuadrícula,
//! anchos de las manzanas y de las calles, lado de la vía férrea, ubicación de
//! la plaza); [`TownLayout::generate`] la convierte en huellas. El render
//! decide las paredes, los techos y los colores según el [`BuildingKind`]; la
//! lógica solo necesita las huellas para la colisión y el terreno nivelado.
//!
//! Las veredas son franjas de [`Sidewalk::WIDTH`] (3,5 m) a lo largo de los
//! frentes de las manzanas de una ciudad y alrededor de la plaza de un pueblo,
//! dentro de las calles, así que una calle de `street_width` deja una calzada
//! de `street_width - 7` donde sus dos lados están pavimentados (al menos
//! [`TownConfig::MIN_STREET_WIDTH`] - 7 m). Los árboles son datos del núcleo y
//! obstáculos: los anillos de las plazas ([`TownLayout::trees`]) y los
//! [`ShadeTree`]s (árboles de vereda en sus cazuelas, bosquecillos entre los
//! senderos de la plaza, árboles en la tierra frente a las casas, árboles de
//! patio y de puntos de referencia), con las [`YardPlant`]s (arbustos y
//! canteros de flores) de los patios como escenografía.

use std::fmt;

use crate::lamps::StreetLamps;
use crate::places::PLATFORM_SPOT_FROM_EDGE;
use crate::rng::Rng;
use crate::{Aabb, FlatArea, Heading, Terrain, Vec2};

/// Qué es un edificio, para que el render pueda elegir su aspecto y la lógica
/// su función.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    House,
    /// "Almacén de ramos generales": el comercio general del pueblo.
    GeneralStore,
    PoliceStation,
    Church,
    Station,
    /// Plataforma elevada entre la estación y la vía.
    Platform,
    /// Galpón de chapa ondulada.
    Shed,
    /// Galpón largo para almacenar cereal junto a la vía férrea.
    GrainStore,
    /// Tanque de agua "australiano", redondo y bajo.
    WaterTank,
    /// Torre de molino que bombea agua a un tanque.
    Windmill,
}

/// Qué lado de una huella da a la calle (donde va la fachada).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    North,
    South,
    East,
    West,
}

impl Side {
    /// El lado que mira hacia el lado contrario.
    pub fn opposite(self) -> Self {
        match self {
            Side::North => Side::South,
            Side::South => Side::North,
            Side::East => Side::West,
            Side::West => Side::East,
        }
    }
}

/// Un edificio estático: una huella rectangular, una altura de pared y su lado
/// de la calle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Building {
    pub kind: BuildingKind,
    pub footprint: Aabb,
    /// Altura de la pared en metros (los techos y los parapetos son una
    /// decisión del render).
    pub height: f32,
    pub front: Side,
}

impl Building {
    /// Cuánto sobresale el techo de un galpón (de un cobertizo o de un depósito
    /// de cereal) más allá de sus paredes, en metros: sus aleros terminan sobre
    /// la huella, y sus paredes quedan esta distancia hacia adentro de ella (el
    /// `facade::walls::barn` del render).
    pub const BARN_EAVES: f32 = 0.45;

    /// Con qué chocan las personas y los caballos: la huella, o las paredes de
    /// un galpón, a [`Self::BARN_EAVES`] hacia adentro de ella (solo su techo
    /// sobresale).
    pub fn collider(&self) -> Aabb {
        match self.kind {
            BuildingKind::Shed | BuildingKind::GrainStore => {
                self.footprint.expanded(-Self::BARN_EAVES)
            }
            _ => self.footprint,
        }
    }
}

/// Qué calles de un pueblo tienen vereda de ladrillo, según sus manzanas
/// (usuario, 2026-10-25). El resto queda de tierra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paving {
    /// Un pueblito de como mucho [`Paving::VILLAGE_MAX`] manzanas (Metileo):
    /// tierra en todas partes.
    None,
    /// Un pueblo chico (Castex, Monte Nievas, Caleufú): solo el borde propio de
    /// la plaza, un anillo alrededor de ella (usuario, 2026-10-25).
    Plaza,
    /// Un pueblo grande (General Pico, [`TownSize::Big`] o más grande): a lo
    /// largo de todo frente de manzana y alrededor de toda plaza.
    Everywhere,
}

impl Paving {
    /// El máximo de manzanas de un pueblito sin ningún pavimento (3 x 3).
    pub const VILLAGE_MAX: usize = 9;

    /// El pavimento de un pueblo de `blocks` (columnas, filas).
    pub fn of(blocks: (usize, usize)) -> Self {
        match TownSize::of(blocks) {
            _ if blocks.0 * blocks.1 <= Self::VILLAGE_MAX => Self::None,
            TownSize::Small => Self::Plaza,
            TownSize::Big | TownSize::VeryBig => Self::Everywhere,
        }
    }
}

/// De qué está hecha una vereda: de ladrillo en una ciudad, y alrededor de la
/// plaza de un pueblo; en otros lugares, un sendero de tierra apisonada que se
/// funde con el pasto, siempre con sus cordones (usuario, 2026-10-26).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Walk {
    Brick,
    Earth,
}

/// Una franja recta de vereda (de ladrillo o de tierra apisonada, [`Walk`]), apoyada sobre la calle a
/// lo largo del borde de una manzana o de una plaza. Se dibuja sobre el suelo como las calles y los
/// senderos de la plaza, unos centímetros más alta: la gente camina sobre ella a la altura del suelo,
/// así que no cambia en nada cómo se mueve nadie.
#[derive(Debug, Clone, PartialEq)]
pub struct Sidewalk {
    pub area: Aabb,
    /// Sus bordes sobre la calzada, cada uno con un cordón.
    pub curbs: Vec<Side>,
    pub walk: Walk,
}

impl Sidewalk {
    /// Ancho de una vereda, cordón incluido, en metros.
    pub const WIDTH: f32 = 3.5;

    /// Las franjas de una vereda alrededor de todo `area`: las del norte y el
    /// sur doblan en las esquinas, con cordón en sus extremos; las del este y
    /// el oeste van entre ellas.
    fn ring(area: Aabb, walk: Walk) -> [Sidewalk; 4] {
        let w = Self::WIDTH;
        let (min, max) = (area.min, area.max);
        let ends = |side| vec![side, Side::East, Side::West];
        [
            Sidewalk {
                area: Aabb::new(Vec2::new(min.x - w, min.y - w), Vec2::new(max.x + w, min.y)),
                curbs: ends(Side::South),
                walk,
            },
            Sidewalk {
                area: Aabb::new(Vec2::new(min.x - w, max.y), Vec2::new(max.x + w, max.y + w)),
                curbs: ends(Side::North),
                walk,
            },
            Sidewalk {
                area: Aabb::new(Vec2::new(min.x - w, min.y), Vec2::new(min.x, max.y)),
                curbs: vec![Side::West],
                walk,
            },
            Sidewalk {
                area: Aabb::new(Vec2::new(max.x, min.y), Vec2::new(max.x + w, max.y)),
                curbs: vec![Side::East],
                walk,
            },
        ]
    }
}

/// Las veredas de un pueblo pavimentado según `paving`, sobre `grid`, alrededor
/// de su `plaza` principal (las manzanas que ocupa): las de una ciudad son
/// todas de ladrillo; el anillo de la plaza de un pueblo es de ladrillo y sus
/// otras manzanas tienen un sendero de tierra apisonada; las de un pueblito son
/// todas de tierra apisonada. Todas tienen sus cordones.
fn sidewalks(grid: &Grid, plaza: PlazaBlocks, paving: Paving) -> Vec<Sidewalk> {
    let area = grid.span(plaza);
    let blocks = (0..grid.rows).flat_map(|j| (0..grid.cols).map(move |i| (i, j)));
    let walk = match paving {
        Paving::Everywhere => Walk::Brick,
        Paving::Plaza | Paving::None => Walk::Earth,
    };
    // Un pueblito es demasiado chico para el ladrillo, incluso alrededor de su
    // plaza.
    let plaza_walk = match paving {
        Paving::None => Walk::Earth,
        Paving::Plaza | Paving::Everywhere => Walk::Brick,
    };
    blocks
        .filter(|&b| !plaza.contains(b))
        .flat_map(|(i, j)| Sidewalk::ring(grid.block(i, j), walk))
        .chain(Sidewalk::ring(area, plaza_walk))
        .collect()
}

/// Si `a` y `b` comparten un área, no solo un borde o una esquina.
#[cfg(test)]
fn overlap(a: &Aabb, b: &Aabb) -> bool {
    a.min.x.max(b.min.x) < a.max.x.min(b.max.x) && a.min.y.max(b.min.y) < a.max.y.min(b.max.y)
}

/// Un piso elevado por el que camina la gente: el andén de la estación. Su
/// parte superior forma parte de la altura del suelo
/// ([`crate::World::ground_height`]), no es un obstáculo.
///
/// Una rampa corta en cada extremo baja hasta el suelo a lo largo del piso. Sus
/// lados largos son escalones simples: la gente sube desde el lado del pueblo o
/// baja hacia la vía (donde un tren detenido la bloquea, ya que sus vagones son
/// obstáculos en movimiento).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floor {
    pub footprint: Aabb,
    /// Altura de la parte superior sobre el suelo, en metros.
    pub height: f32,
    /// Eje unitario del mundo a lo largo del piso (`+x` o `+y`), en la
    /// dirección de sus rampas.
    pub along: Vec2,
}

impl Floor {
    /// Altura de la parte superior del andén de una estación sobre el suelo, en
    /// metros.
    pub const PLATFORM_HEIGHT: f32 = 0.9;
    /// Distancia desde la línea central de una vía hasta el borde de su andén
    /// junto a la vía, en metros: el costado de los vagones está a 1,5 m, así
    /// que queda un palmo de 0,15 m, como en un andén de trocha ancha.
    pub const PLATFORM_EDGE: f32 = 1.65;
    /// A qué distancia hacia adentro del borde lejano de un andén corre un
    /// sendero hacia él, en metros.
    pub(crate) const WALK_LINE_IN: f32 = 3.5;
    /// Largo de la rampa más allá de cada extremo, en metros (una pendiente
    /// suave de 1:3 para un andén de 0,9 m).
    pub const RAMP_LENGTH: f32 = 3.0;

    /// Eje unitario del mundo transversal al piso.
    pub fn across(&self) -> Vec2 {
        Vec2::new(self.along.y, self.along.x)
    }

    /// La extensión del piso a lo largo de `axis`, como `(low, high)`.
    fn span(&self, axis: Vec2) -> (f32, f32) {
        (self.footprint.min.dot(axis), self.footprint.max.dot(axis))
    }

    /// Altura sobre el suelo en `p`: la parte superior del piso sobre él,
    /// bajando linealmente hasta cero por cualquiera de las rampas. `None`
    /// fuera del piso y de las rampas.
    pub fn rise_at(&self, p: Vec2) -> Option<f32> {
        let (lo, hi) = self.span(self.across());
        let c = p.dot(self.across());
        if c < lo || c > hi {
            return None;
        }
        let (lo, hi) = self.span(self.along);
        let a = p.dot(self.along);
        let past = (lo - a).max(a - hi).max(0.0);
        (past <= Self::RAMP_LENGTH).then(|| self.height * (1.0 - past / Self::RAMP_LENGTH))
    }

    /// Dónde baja un pasajero de un tren detenido del lado del piso más cercano
    /// a `train`: a mitad del largo, un paso hacia adentro desde ese borde
    /// largo (como [`TownLayout::platform_spot`]), y la orientación que se
    /// aleja del borde.
    pub fn spot_beside(&self, train: Vec2) -> (Vec2, Heading) {
        let across = self.across();
        let (lo, hi) = self.span(across);
        let c = train.dot(across);
        let (edge, inward) = if (c - lo).abs() < (hi - c).abs() {
            (lo, 1.0)
        } else {
            (hi, -1.0)
        };
        let at = across * (edge + inward * PLATFORM_SPOT_FROM_EDGE)
            + self.along * self.footprint.center().dot(self.along);
        let out = across * inward;
        (at, Heading::from_radians(out.y.atan2(out.x)))
    }

    /// Las huellas de las dos rampas, primero el extremo bajo a lo largo de
    /// [`Self::along`].
    pub fn ramps(&self) -> [Aabb; 2] {
        let reach = self.along * Self::RAMP_LENGTH;
        let f = self.footprint;
        let low_end = f.max - self.along * f.size().dot(self.along);
        let high_end = f.min + self.along * f.size().dot(self.along);
        [
            Aabb::new(f.min - reach, low_end),
            Aabb::new(high_end, f.max + reach),
        ]
    }
}

/// El techo del andén: una losa de techo plana colgada de la pared de la
/// estación del lado de la vía sobre el andén, apoyada en postes delgados. El
/// render la dibuja a partir de esto, y la cámara la trata como un techo a
/// través del cual no puede ver (una losa, no una columna: la cámara igual
/// puede quedar debajo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canopy {
    /// La huella del techo en el plano del suelo.
    pub footprint: Aabb,
    /// Altura del centro de la losa del techo sobre el suelo bajo la estación.
    pub height: f32,
}

impl Canopy {
    /// Cuánto sobresale el techo desde la pared de la estación, en metros.
    pub const DEPTH: f32 = 5.5;
    /// Altura del techo sobre el suelo bajo la estación, en metros.
    pub const HEIGHT: f32 = 3.6;
    /// Espesor de la losa del techo, en metros.
    pub const THICKNESS: f32 = 0.12;

    /// Altura de la parte inferior de la losa sobre el suelo bajo la estación.
    pub fn bottom(&self) -> f32 {
        self.height - Self::THICKNESS / 2.0
    }

    /// Altura de la parte superior de la losa sobre el suelo bajo la estación.
    pub fn top(&self) -> f32 {
        self.height + Self::THICKNESS / 2.0
    }
}

/// Una línea de ferrocarril recta en el plano del suelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Railway {
    pub start: Vec2,
    pub end: Vec2,
    /// Distancia entre los rieles, en metros.
    pub gauge: f32,
}

/// Un caballo ensillado atado frente a un edificio: nunca se mueve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TetheredHorse {
    pub position: Vec2,
    pub heading: Heading,
    pub footprint: Aabb,
}

impl TetheredHorse {
    /// Largo de la huella de un caballo parado, de la nariz a la cola, en
    /// metros.
    pub const LENGTH: f32 = 2.4;
    /// Ancho de la huella de un caballo parado, en metros.
    pub const WIDTH: f32 = 1.0;

    /// Un caballo atado y parado en `position`, mirando hacia `heading`: su
    /// huella es el rectángulo alineado con los ejes que rodea su cuerpo
    /// girado.
    pub fn standing(position: Vec2, heading: Heading) -> Self {
        let along = heading.forward();
        let half = Vec2::new(
            along.x.abs() * Self::LENGTH + along.y.abs() * Self::WIDTH,
            along.y.abs() * Self::LENGTH + along.x.abs() * Self::WIDTH,
        ) * 0.5;
        Self {
            position,
            heading,
            footprint: Aabb::new(position - half, position + half),
        }
    }
}

/// Un palenque de madera con un caballo atado al lado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitchingPost {
    pub rail: Aabb,
    pub horse: TetheredHorse,
    /// Profundidad de la galería del almacén, en metros: el palenque queda
    /// justo más allá, así que las calles angostas tienen una galería menos
    /// profunda.
    pub veranda: f32,
}

/// Un palenque frente a un punto de referencia además del propio
/// [`HitchingPost`] del almacén de ramos generales: los otros almacenes, las
/// comisarías, la iglesia y la estación. Una ciudad ata un caballo a algunos de
/// ellos ([`Palenque::MAX_HORSES`]); un pueblo o un pueblito los deja vacíos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palenque {
    /// El edificio frente al que está: un índice dentro de
    /// [`TownLayout::buildings`].
    pub building: usize,
    pub rail: Aabb,
    /// El caballo atado al lado, si hay alguno.
    pub horse: Option<TetheredHorse>,
}

impl Palenque {
    /// El máximo de caballos que ata una ciudad en sus palenques, para que sus
    /// figuras animadas sigan siendo pocas.
    pub const MAX_HORSES: usize = 3;
    /// Largo del palenque a lo largo de la calle, en metros.
    pub const LENGTH: f32 = 2.8;
    /// Ancho del palenque a lo ancho de la calle, en metros.
    pub const WIDTH: f32 = 0.2;
    /// Separación que se mantiene entre un palenque (y el lugar de su caballo)
    /// y cualquier edificio, árbol, cartel, poste u otro obstáculo, en metros.
    pub const CLEARANCE: f32 = 0.5;
    /// Separación que se mantiene alrededor de un cruce de calles, donde están
    /// los faroles de esquina.
    pub const CROSSING_MARGIN: f32 = 1.0;
    /// A qué distancia de la línea media de la calle se mantiene un palenque:
    /// el carril que usan los peatones.
    pub const LANE: f32 = 1.0;
    /// A qué distancia hacia afuera de su fachada está el palenque de la
    /// estación, sobre la playa, en metros.
    pub const YARD_OUT: f32 = 2.7;

    /// Cuántos de los `posts` palenques atan un caballo en una ciudad: la
    /// mitad, como mucho [`Self::MAX_HORSES`].
    pub fn horses_for(posts: usize) -> usize {
        (posts / 2).min(Self::MAX_HORSES)
    }

    /// Sus obstáculos estáticos: el palenque, y el caballo cuando hay uno.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        std::iter::once(self.rail).chain(self.horse.map(|h| h.footprint))
    }
}

/// Dónde está un [`ShadeTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShadeKind {
    /// Un árbol de calle en la vereda de una ciudad, en su cazuela (un pequeño
    /// cuadrado de tierra bordeado por el pavimento), con la copa podada.
    Pit,
    /// Un árbol en la tierra frente a una casa de un pueblo o de un pueblito,
    /// con la copa podada.
    Front,
    /// Un árbol en la plaza de una ciudad, en el verde entre sus senderos.
    Grove,
    /// Un árbol en un patio o al lado de un punto de referencia, con la copa
    /// libre.
    Yard,
}

/// Un árbol fuera del anillo de [`TownLayout::trees`]: los árboles de calle,
/// los bosquecillos de la plaza y los árboles de patio. Su tronco es un
/// obstáculo como el de un árbol de plaza ([`TownLayout::obstacles`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadeTree {
    /// El centro del tronco.
    pub at: Vec2,
    pub kind: ShadeKind,
    /// Lo más lejos que puede llegar su copa desde el tronco, en metros: el
    /// render angosta el árbol para que entre, y el núcleo mantiene a todos los
    /// edificios a esa distancia (más un margen) del tronco.
    pub crown: f32,
    /// El vector unitario del suelo hacia el que se inclinan el tronco y la
    /// copa, hacia la calle junto a la que está un árbol podado; cero para los
    /// demás.
    pub toward: Vec2,
    /// La franja de vereda de un árbol [`ShadeKind::Pit`]: una fila comparte
    /// especie.
    pub row: u32,
}

impl ShadeTree {
    /// Lado de una cazuela, borde incluido, en metros.
    pub const PIT_SIDE: f32 = 0.8;
    /// Separación entre una cazuela y el cordón, en metros.
    pub const PIT_FROM_CURB: f32 = 0.4;
    /// Copa de un árbol de cazuela: una vereda de 2,5 m deja su tronco a 1,7 m
    /// de la fachada, así que la copa se mantiene angosta (podada) y se inclina
    /// sobre la calle.
    pub const PIT_CROWN: f32 = 2.3;
    /// A qué distancia de la fachada está un árbol de frente, en metros.
    pub const FRONT_OUT: f32 = 2.2;
    /// Copa de un árbol en la tierra frente a una casa.
    pub const FRONT_CROWN: f32 = 1.9;
    /// Separación mínima entre dos árboles en la tierra frente a las casas, en
    /// metros.
    pub const FRONT_APART: f32 = 13.0;
    /// Un pueblo no tiene filas de árboles de frente: dentro de este alcance de
    /// cualquiera de ellos hay como mucho otros dos, en metros.
    pub const FRONT_RUN: f32 = 30.0;
    /// Copa de un árbol de patio.
    pub const YARD_CROWN: f32 = 3.0;
    /// Separación que se mantiene entre la copa de un árbol de patio y
    /// cualquier edificio, en metros.
    pub const BUILDING_GAP: f32 = 0.6;
    /// Separación entre un árbol de calle y el siguiente a lo largo de su
    /// vereda, en metros (mín, máx). Un 15 % más de árboles que en la primera
    /// plantación (usuario, 2026-10-27: "subile otro 15% a pueblos y aldeas"):
    /// (17, 21.2) / 1.15.
    pub const SPACING: (f32, f32) = (14.8, 18.4);
    /// Separación mínima entre dos troncos, sean lo que sean, en metros.
    pub const APART: f32 = 4.0;
    /// Cuánto se inclina hacia la calle un árbol podado, en grados.
    pub const LEAN: f32 = 5.0;
    /// Copa de un árbol de un bosquecillo de plaza: la propia de la especie,
    /// sin límite.
    pub const GROVE_CROWN: f32 = 6.0;
    /// A qué distancia hacia adentro del borde de su plaza se mantiene un
    /// bosquecillo, en metros.
    const GROVE_INSET: f32 = 4.0;
    /// Separación entre los lugares sobre los que se distribuye un bosquecillo,
    /// en metros (antes eran 10 m; el anillo de la plaza también creció, y el
    /// recorte de un pueblo muy grande se lleva su parte de los bosquecillos).
    const GROVE_STEP: f32 = 8.8;
    /// Cuánto se desvía cada lugar de un bosquecillo, en metros.
    const GROVE_JITTER: f32 = 2.5;
    /// A qué distancia del borde de una manzana está como mínimo un árbol de
    /// patio, en metros.
    const YARD_INSET: f32 = 3.0;
    /// Cuánto más allá de las paredes de un punto de referencia pueden estar
    /// sus árboles, en metros.
    const LANDMARK_REACH: f32 = 7.0;

    /// El terreno que ocupa un tronco en `at`.
    fn trunk(at: Vec2) -> Aabb {
        let side = 2.0 * TREE_TRUNK_RADIUS;
        Aabb::from_center_size(at, Vec2::new(side, side))
    }

    /// El tronco del árbol como el obstáculo con el que chocan personas y
    /// caballos.
    pub fn obstacle(&self) -> Aabb {
        Self::trunk(self.at)
    }
}

/// Qué es una [`YardPlant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlantKind {
    /// Un arbusto bajo contra una pared.
    Shrub,
    /// Un pequeño cantero de flores.
    Flowers,
}

impl PlantKind {
    /// La mitad del lado del terreno que ocupa, en metros.
    pub fn radius(self) -> f32 {
        match self {
            Self::Shrub => 0.6,
            Self::Flowers => 0.7,
        }
    }
}

/// Un arbusto o un cantero de flores en el patio de una casa: escenografía sin
/// obstáculo, fuera de las calles, las puertas, los demás edificios y los
/// troncos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct YardPlant {
    pub at: Vec2,
    pub kind: PlantKind,
}

impl YardPlant {
    /// El terreno que cubre.
    pub fn area(&self) -> Aabb {
        let side = 2.0 * self.kind.radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
    }
}

/// Dónde va la plaza en la cuadrícula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlazaPlacement {
    /// La manzana central cuando hay una; si no, la manzana que mira a la
    /// estación; si no (sin vía férrea), la manzana más cercana al centro.
    #[default]
    Auto,
    /// La manzana central (las dos cantidades de manzanas deben ser impares).
    Center,
    /// La manzana de la fila (o columna) junto a la vía férrea más cercana al
    /// centro de la cuadrícula; la estación se alinea para mirarla del otro
    /// lado de la calle.
    FacingStation,
    /// La manzana `(i, j)`, contada desde la esquina sudoeste.
    Block(usize, usize),
}

/// Por qué lado de la cuadrícula pasa la vía férrea, y a qué distancia; o, para
/// un pueblo que creció a ambos lados de su línea, en qué mitad del pueblo está
/// su estación.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailwayConfig {
    pub side: Side,
    /// Distancia desde el borde de la cuadrícula (el borde exterior de la calle
    /// perimetral) hasta la línea central de la vía, en metros. Para una línea
    /// `through`, desde el borde de la mitad del pueblo donde está la estación.
    pub offset: f32,
    /// La vía atraviesa el pueblo de este a oeste por el medio, entre sus dos
    /// filas de manzanas centrales, en lugar de correr por un costado: el
    /// pueblo crece a ambos lados de la línea, como los grandes pueblos
    /// ferroviarios. `side` (`North` o `South`) es entonces la mitad en la que
    /// está la estación, que la mira del otro lado de la calle. Las calles que
    /// corren de norte a sur cruzan la vía en pasos a nivel, salvo donde está
    /// la playa de la estación.
    pub through: bool,
}

impl RailwayConfig {
    /// La cantidad mínima de filas de manzanas que necesita un pueblo con una
    /// línea `through`: dos a cada lado de la vía.
    pub const MIN_THROUGH_ROWS: usize = 4;
    /// Para una línea `through`, a qué distancia corre la vía del borde de la
    /// mitad del pueblo opuesta a la estación, en metros.
    pub const THROUGH_FAR_SIDE: f32 = 16.0;
}

/// Todo lo necesario para generar un pueblo.
#[derive(Debug, Clone, PartialEq)]
pub struct TownConfig {
    /// El nombre del pueblo (pintado en su estación).
    pub name: String,
    /// Centro de la cuadrícula de calles en coordenadas del mundo.
    pub origin: Vec2,
    /// Manzanas `(east_west, north_south)`: columnas y filas de la cuadrícula.
    pub blocks: (usize, usize),
    /// Lado de una manzana cuadrada, en metros.
    pub block_size: f32,
    /// Ancho de todas las calles, calle perimetral incluida, en metros.
    pub street_width: f32,
    pub railway: Option<RailwayConfig>,
    pub plaza: PlazaPlacement,
    /// Varía los lotes de las casas y los galpones.
    pub seed: u32,
    /// Cuántas comisarías tiene el pueblo, reemplazando la cantidad de su
    /// categoría de tamaño ([`TownSize`]); `None` conserva la de la categoría.
    /// Como mucho la cantidad de la categoría (un pueblo sin comisaría es
    /// `Some(0)`).
    pub police_stations: Option<usize>,
    /// Proporción de los lotes de casas y galpones que están construidos, de 0
    /// a 1; `None` los construye todos, como siempre. Un pueblito o un apeadero
    /// tiene pocos.
    pub density: Option<f32>,
    /// Cuántas personas caminan por el pueblo, reemplazando la multitud de su
    /// categoría de tamaño ([`TownSize`]); `None` conserva la de la categoría.
    /// Como mucho [`TownConfig::MAX_TOWNSFOLK`] (un apeadero vacío es
    /// `Some(0)`).
    pub townsfolk: Option<usize>,
}

impl TownConfig {
    /// La menor cantidad de manzanas a lo largo de cualquiera de los ejes.
    pub const MIN_BLOCKS: usize = 3;
    /// La manzana más chica en la que todavía entran los puntos de referencia y
    /// los lotes, en metros.
    pub const MIN_BLOCK_SIZE: f32 = 30.0;
    /// La calle más angosta que deja una calzada de 3 m entre dos veredas de
    /// 3,5 m, con lugar para los transeúntes (los pueblos distribuidos tienen
    /// 12 m o más, una calzada de 5 m; un caballo atado necesita 12 m).
    pub const MIN_STREET_WIDTH: f32 = 10.0;
    /// Lo más cerca que puede pasar la vía de la cuadrícula: la playa de la
    /// estación lo necesita.
    pub const MIN_RAILWAY_OFFSET: f32 = 24.0;
    /// El máximo de personas que un pueblo puede pedir en `townsfolk`.
    pub const MAX_TOWNSFOLK: usize = 200;

    /// Pueblo inspirado en Eduardo Castex: 4 x 5 manzanas de 40 m (4 de este a
    /// oeste, 5 de norte a sur), calles de 12 m, vía férrea 24 m al norte de la
    /// cuadrícula, en el centro del mundo. Sin manzana central, `Auto` pone la
    /// plaza en la fila superior, mirando a la estación. Es la alternativa
    /// incorporada cuando no se puede usar `assets/world/towns.ron`; el Eduardo
    /// Castex distribuido ahí tiene otra distribución (5 x 4 manzanas, en su
    /// lugar sobre la línea).
    pub fn castex() -> Self {
        Self {
            name: "Eduardo Castex".to_string(),
            origin: Vec2::ZERO,
            blocks: (4, 5),
            block_size: 40.0,
            street_width: 12.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 7,
            police_stations: None,
            density: None,
            townsfolk: None,
        }
    }

    /// Qué tan grande es el pueblo, según su cantidad de manzanas.
    pub fn size(&self) -> TownSize {
        TownSize::of(self.blocks)
    }

    /// Cuántas comisarías tiene el pueblo: el reemplazo, o la cantidad de su
    /// categoría de tamaño.
    pub fn police_count(&self) -> usize {
        self.police_stations
            .unwrap_or_else(|| self.size().police_stations())
    }

    /// Cuántas personas caminan por el pueblo: el reemplazo, o la multitud de
    /// su categoría de tamaño.
    pub fn townsfolk_count(&self) -> usize {
        self.townsfolk
            .unwrap_or_else(|| self.size().townsfolk(self.blocks))
    }
}

/// La categoría de tamaño de un pueblo, según su cantidad de manzanas `n =
/// columns * rows`. Define la plaza principal, la plaza de la estación, los
/// puntos de referencia y la multitud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TownSize {
    /// Hasta [`TownSize::SMALL_MAX`] manzanas: una plaza de una manzana, un
    /// almacén de ramos generales, una comisaría y
    /// [`crate::townsfolk::TOWNSFOLK_COUNT`] personas.
    Small,
    /// Hasta [`TownSize::BIG_MAX`] manzanas: una plaza central de dos manzanas,
    /// una plaza de la estación, dos almacenes de ramos generales y una
    /// comisaría.
    Big,
    /// Más manzanas: una plaza central de 2 x 2 manzanas, una plaza de la
    /// estación, dos almacenes de ramos generales y dos comisarías.
    VeryBig,
}

impl TownSize {
    /// El máximo de manzanas de un pueblo chico.
    pub const SMALL_MAX: usize = 36;
    /// El máximo de manzanas de un pueblo grande.
    pub const BIG_MAX: usize = 63;

    /// La categoría de una cuadrícula de `blocks` (columnas, filas).
    pub fn of(blocks: (usize, usize)) -> Self {
        match blocks.0 * blocks.1 {
            n if n <= Self::SMALL_MAX => Self::Small,
            n if n <= Self::BIG_MAX => Self::Big,
            _ => Self::VeryBig,
        }
    }

    /// Las manzanas que abarca la plaza principal `(east-west, north-south)` en
    /// una cuadrícula de `blocks`: un pueblo grande fusiona dos a lo largo de
    /// su lado más largo (de este a oeste en una cuadrícula cuadrada). Nunca
    /// más manzanas de las que tiene la cuadrícula, por si alguna vez una
    /// cuadrícula fuera más angosta de lo que hoy permite
    /// [`TownConfig::MIN_BLOCKS`].
    pub(crate) fn plaza_span(self, blocks: (usize, usize)) -> (usize, usize) {
        let (w, h) = match self {
            Self::Small => (1, 1),
            Self::Big if blocks.0 >= blocks.1 => (2, 1),
            Self::Big => (1, 2),
            Self::VeryBig => (2, 2),
        };
        (w.min(blocks.0.max(1)), h.min(blocks.1.max(1)))
    }

    /// Almacenes de ramos generales en un pueblo de este tamaño: uno por cada
    /// mitad de un pueblo dividido por su línea, o uno junto a cada plaza
    /// (usuario, 2026-10-23).
    fn general_stores(self) -> usize {
        match self {
            Self::Small => 1,
            Self::Big | Self::VeryBig => 2,
        }
    }

    /// Comisarías, salvo que un pueblo lo reemplace.
    pub fn police_stations(self) -> usize {
        match self {
            Self::Small | Self::Big => 1,
            Self::VeryBig => 2,
        }
    }

    /// Personas que caminan por un pueblo de `blocks`: la multitud de un pueblo
    /// chico, o cuatro por cada cinco manzanas en los pueblos más grandes,
    /// nunca menos.
    fn townsfolk(self, blocks: (usize, usize)) -> usize {
        let small = crate::townsfolk::TOWNSFOLK_COUNT;
        match self {
            Self::Small => small,
            Self::Big | Self::VeryBig => {
                let n = (blocks.0 * blocks.1) as f32;
                small.max((0.8 * n).round() as usize)
            }
        }
    }
}

/// Por qué no se puede construir una [`TownConfig`].
#[derive(Debug, Clone, PartialEq)]
pub enum TownConfigError {
    /// El nombre está vacío o en blanco.
    EmptyName,
    /// Un número es NaN o infinito.
    NotFinite { field: &'static str },
    /// Menos de [`TownConfig::MIN_BLOCKS`] manzanas a lo largo de un eje.
    TooFewBlocks { blocks: (usize, usize), min: usize },
    /// Manzanas más chicas que [`TownConfig::MIN_BLOCK_SIZE`].
    BlockTooSmall { block_size: f32, min: f32 },
    /// Calles más angostas que [`TownConfig::MIN_STREET_WIDTH`].
    StreetTooNarrow { street_width: f32, min: f32 },
    /// La vía está más cerca de la cuadrícula que
    /// [`TownConfig::MIN_RAILWAY_OFFSET`].
    RailwayTooClose { offset: f32, min: f32 },
    /// `PlazaPlacement::Block` fuera de la cuadrícula.
    PlazaOutOfRange {
        block: (usize, usize),
        blocks: (usize, usize),
    },
    /// `PlazaPlacement::Center` en una cuadrícula con una cantidad par de
    /// manzanas.
    NoCentralBlock { blocks: (usize, usize) },
    /// `PlazaPlacement::FacingStation` en un pueblo sin vía férrea.
    NoStationToFace,
    /// `police_stations` pide más de las que coloca la categoría de tamaño.
    TooManyPoliceStations { wanted: usize, max: usize },
    /// `density` fuera del rango de 0 a 1.
    BadDensity { density: f32 },
    /// `townsfolk` pide más de [`TownConfig::MAX_TOWNSFOLK`].
    TooManyTownsfolk { wanted: usize, max: usize },
    /// Una vía férrea `through` en el lado este u oeste: solo corre de este a
    /// oeste.
    ThroughNotEastWest { side: Side },
    /// Una vía férrea `through` en un pueblo con muy pocas filas para crecer a
    /// ambos lados de ella ([`RailwayConfig::MIN_THROUGH_ROWS`]).
    ThroughTooFewRows { rows: usize, min: usize },
    /// La plaza abarca ambos lados de una vía férrea `through`.
    PlazaAcrossRailway,
}

impl fmt::Display for TownConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "the town has no name"),
            Self::NotFinite { field } => write!(f, "`{field}` is not a finite number"),
            Self::TooFewBlocks { blocks, min } => write!(
                f,
                "blocks {blocks:?} are too few: at least {min} along each axis"
            ),
            Self::BlockTooSmall { block_size, min } => write!(
                f,
                "block_size {block_size} m is too small for the landmarks (min {min} m)"
            ),
            Self::StreetTooNarrow { street_width, min } => write!(
                f,
                "street_width {street_width} m is too narrow (min {min} m)"
            ),
            Self::RailwayTooClose { offset, min } => write!(
                f,
                "railway offset {offset} m leaves no room for the station yard (min {min} m)"
            ),
            Self::PlazaOutOfRange { block, blocks } => write!(
                f,
                "plaza block {block:?} is outside the {}x{} grid",
                blocks.0, blocks.1
            ),
            Self::NoCentralBlock { blocks } => write!(
                f,
                "plaza `Center` needs a central block, but the {}x{} grid has an even side",
                blocks.0, blocks.1
            ),
            Self::NoStationToFace => {
                write!(
                    f,
                    "plaza `FacingStation` needs a railway, but there is none"
                )
            }
            Self::TooManyPoliceStations { wanted, max } => write!(
                f,
                "police_stations {wanted} is more than a town of this size has (max {max})"
            ),
            Self::BadDensity { density } => {
                write!(f, "density {density} is outside 0 to 1")
            }
            Self::TooManyTownsfolk { wanted, max } => {
                write!(
                    f,
                    "townsfolk {wanted} is more than a town holds (max {max})"
                )
            }
            Self::ThroughNotEastWest { side } => write!(
                f,
                "a `through` railway runs east-west: its side must be North or South, not {side:?}"
            ),
            Self::ThroughTooFewRows { rows, min } => write!(
                f,
                "a `through` railway needs at least {min} rows of blocks, the town has {rows}"
            ),
            Self::PlazaAcrossRailway => {
                write!(f, "the plaza spans both sides of the `through` railway")
            }
        }
    }
}

impl std::error::Error for TownConfigError {}

/// Un pequeño pueblo ferroviario distribuido en una cuadrícula ortogonal.
#[derive(Debug, Clone, PartialEq)]
pub struct TownLayout {
    /// El nombre del pueblo, tomado de su configuración.
    pub name: String,
    /// Superficies de las calles, incluida la calle perimetral alrededor de la
    /// cuadrícula.
    pub streets: Vec<Aabb>,
    /// La plaza principal: una manzana, o varias fusionadas con las calles
    /// entre ellas en los pueblos más grandes.
    pub plaza: Aabb,
    /// La segunda plaza, más chica, de un pueblo más grande, junto a la
    /// estación: media manzana.
    pub station_plaza: Option<Aabb>,
    /// La categoría de tamaño del pueblo.
    pub size: TownSize,
    /// Posiciones de los troncos de los árboles del anillo de cada plaza.
    pub trees: Vec<Vec2>,
    /// Los demás árboles: árboles de vereda y de calle de tierra, bosquecillos
    /// de plaza, árboles de patio y de puntos de referencia ([`ShadeTree`]).
    pub shade_trees: Vec<ShadeTree>,
    /// Los arbustos y los canteros de flores de los patios detrás de las casas
    /// ([`YardPlant`]).
    pub yard_plants: Vec<YardPlant>,
    pub railway: Option<Railway>,
    pub buildings: Vec<Building>,
    /// Palenque frente al (primer) almacén de ramos generales.
    pub hitching_post: HitchingPost,
    /// Los demás palenques: frente a los demás almacenes de ramos generales,
    /// las comisarías, la iglesia y la estación ([`Palenque`]).
    pub palenques: Vec<Palenque>,
    /// La semilla de la configuración, que también se usa para la multitud del
    /// pueblo.
    pub seed: u32,
    /// Personas que caminan por las calles ([`TownConfig::townsfolk_count`]).
    townsfolk: usize,
    blocks: (usize, usize),
    grid: Aabb,
    extent: Aabb,
    arrival: (Vec2, Heading),
    /// La franja entre las dos mitades de un pueblo que creció a ambos lados de
    /// su línea ([`RailwayConfig::through`]): la vía y la playa de la estación.
    band: Option<Aabb>,
    /// Sus veredas de ladrillo ([`Paving`]).
    sidewalks: Vec<Sidewalk>,
}

/// Radio del tronco que se usa para la colisión de los árboles.
pub const TREE_TRUNK_RADIUS: f32 = 0.35;

/// Ancho de los dos senderos que cruzan cada plaza, en metros.
pub const PLAZA_PATH_WIDTH: f32 = 3.0;
/// Lado del pedestal cuadrado del monumento donde se encuentran los senderos de
/// una plaza, en metros.
pub const PLAZA_MONUMENT: f32 = 3.0;
/// Separación mínima entre el tronco de un árbol de plaza y un sendero o el
/// monumento, en metros: los árboles están en los canteros verdes entre los
/// senderos.
pub const PLAZA_PATH_CLEARANCE: f32 = 1.5;

/// Los dos senderos que cruzan `plaza` por su centro, de borde a borde: primero
/// el este–oeste y después el norte–sur.
pub fn plaza_paths(plaza: Aabb) -> [Aabb; 2] {
    let (c, s) = (plaza.center(), plaza.size());
    [
        Aabb::from_center_size(c, Vec2::new(s.x, PLAZA_PATH_WIDTH)),
        Aabb::from_center_size(c, Vec2::new(PLAZA_PATH_WIDTH, s.y)),
    ]
}

/// El pedestal del monumento en el centro de `plaza`, donde se encuentran sus
/// senderos.
pub fn plaza_monument(plaza: Aabb) -> Aabb {
    Aabb::from_center_size(plaza.center(), Vec2::new(PLAZA_MONUMENT, PLAZA_MONUMENT))
}

/// Lado del cuadrado pavimentado donde se cruzan los senderos de una plaza, en
/// metros: el ancho de un sendero alrededor de todo el pedestal del monumento,
/// para que las personas y los jinetes rodeen el monumento.
pub const PLAZA_CROSSING: f32 = PLAZA_MONUMENT + 2.0 * PLAZA_PATH_WIDTH;

/// El cuadrado pavimentado alrededor del monumento de `plaza`, donde se cruzan
/// sus senderos.
pub fn plaza_crossing(plaza: Aabb) -> Aabb {
    Aabb::from_center_size(plaza.center(), Vec2::new(PLAZA_CROSSING, PLAZA_CROSSING))
}

/// Un banco de plaza de listones con laterales de hierro fundido, en un cantero
/// verde al lado de un sendero, mirando hacia él.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bench {
    /// El centro de su huella.
    pub at: Vec2,
    /// Hacia dónde mira alguien sentado en él: hacia el sendero.
    pub facing: Side,
}

impl Bench {
    /// Largo de un banco a lo largo de su sendero, en metros.
    pub const LENGTH: f32 = 1.8;
    /// Profundidad desde el frente del asiento hasta la parte trasera de sus
    /// patas, en metros.
    pub const DEPTH: f32 = 0.6;
    /// Separación entre el borde del sendero y el frente del banco, en metros:
    /// lugar para el cordón del sendero y los pies de quien se sienta.
    pub const PATH_GAP: f32 = 0.3;
    /// Separación mínima entre dos bancos a lo largo de un sendero, en metros.
    pub const SPACING: f32 = 1.0;

    /// Su obstáculo estático: todo el banco.
    pub fn footprint(&self) -> Aabb {
        let size = match self.facing {
            Side::North | Side::South => Vec2::new(Self::LENGTH, Self::DEPTH),
            Side::East | Side::West => Vec2::new(Self::DEPTH, Self::LENGTH),
        };
        Aabb::from_center_size(self.at, size)
    }
}

/// Lo que hay en una plaza además de sus árboles, según el [`Paving`] de su
/// pueblo (fase 3 de las fachadas de los pueblos): el monumento sobre su
/// pedestal, el cruce pavimentado a su alrededor, bancos al lado de los
/// senderos y, solo en una ciudad, una reja baja de hierro alrededor del borde,
/// abierta en la entrada de cada sendero. El monumento, los bancos y la reja
/// son obstáculos estáticos; los senderos y el cruce quedan libres, así que las
/// calles, el grafo de los vecinos y todos los faroles quedan donde estaban.
#[derive(Debug, Clone, PartialEq)]
pub struct PlazaFurniture {
    /// La plaza.
    pub area: Aabb,
    /// El pavimento de su pueblo, que también define qué tan imponente es la
    /// plaza.
    pub paving: Paving,
    /// El collider del monumento: su escalón más bajo, dentro de
    /// [`plaza_monument`].
    pub monument: Aabb,
    /// El cuadrado pavimentado alrededor del monumento ([`plaza_crossing`]).
    pub crossing: Aabb,
    pub benches: Vec<Bench>,
    /// Las piezas de la reja a lo largo del borde, cada una un rectángulo
    /// delgado.
    pub railing: Vec<Aabb>,
}

impl PlazaFurniture {
    /// A qué distancia hacia adentro del borde de la plaza empieza la reja, en
    /// metros.
    pub const RAILING_INSET: f32 = 0.05;
    /// Profundidad de la reja, pilares incluidos, en metros: se mantiene fuera
    /// del medio metro alrededor de los faroles en las entradas de los
    /// senderos, a un metro hacia adentro del borde.
    pub const RAILING_DEPTH: f32 = 0.3;
    /// Cuánto se abre la reja a cada lado del centro de un sendero: el sendero
    /// y sus cordones.
    pub const MOUTH_HALF: f32 = PLAZA_PATH_WIDTH / 2.0 + 0.15;
    /// Separación mínima desde el cruce hasta el primer banco a lo largo de un
    /// sendero, en metros.
    pub const CROSSING_GAP: f32 = 0.5;
    /// Separación mínima desde el borde de la plaza hasta el último banco a lo
    /// largo de un sendero, en metros: más allá de los faroles en las entradas
    /// de los senderos.
    pub const EDGE_GAP: f32 = 2.6;

    /// Lado del escalón más bajo del monumento: más chico en un pueblito.
    pub fn monument_side(paving: Paving) -> f32 {
        match paving {
            Paving::None => 2.4,
            Paving::Plaza | Paving::Everywhere => PLAZA_MONUMENT,
        }
    }

    /// Cómo se reparten los bancos (usuario, 2026-10-26: las plazas chicas
    /// siguen siendo escasas): qué senderos que salen del cruce tienen alguno
    /// (este, norte, oeste, sur), cuántos cada uno, y si van a ambos lados del
    /// sendero o solo a su izquierda, uno por cuadrante como un molinete. Una
    /// ciudad tiene tres a cada lado de cada sendero (uno en su plaza de la
    /// estación), un pueblo uno junto a cada sendero y un pueblito uno junto al
    /// sendero este y uno junto al oeste.
    pub fn bench_plan(paving: Paving, main: bool) -> (&'static [usize], usize, bool) {
        match paving {
            Paving::Everywhere => (&[0, 1, 2, 3], if main { 3 } else { 1 }, true),
            Paving::Plaza => (&[0, 1, 2, 3], 1, false),
            Paving::None => (&[0, 2], 1, false),
        }
    }

    /// Cuántos bancos tiene una plaza cuando entran todos
    /// ([`Self::bench_plan`]).
    pub fn bench_count(paving: Paving, main: bool) -> usize {
        let (arms, per, both) = Self::bench_plan(paving, main);
        arms.len() * per * if both { 2 } else { 1 }
    }

    /// El mobiliario de `area`, la plaza principal del pueblo cuando `main`, en
    /// un pueblo pavimentado según `paving`. Un banco que quedaría sobre algo
    /// de `keep_off` (troncos de árboles, carteles) se deja fuera.
    pub fn of(area: Aabb, paving: Paving, main: bool, keep_off: &[Aabb]) -> Self {
        let c = area.center();
        let side = Self::monument_side(paving);
        Self {
            area,
            paving,
            monument: Aabb::from_center_size(c, Vec2::new(side, side)),
            crossing: plaza_crossing(area),
            benches: benches(area, Self::bench_plan(paving, main), keep_off),
            // Solo una ciudad enreja su plaza (usuario, 2026-10-26).
            railing: match paving {
                Paving::None | Paving::Plaza => Vec::new(),
                Paving::Everywhere => railing(area),
            },
        }
    }

    /// Sus obstáculos estáticos: el monumento, los bancos y la reja.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        std::iter::once(self.monument)
            .chain(self.benches.iter().map(Bench::footprint))
            .chain(self.railing.iter().copied())
    }
}

/// Los bancos de `area` tal como los reparte `plan`
/// ([`PlazaFurniture::bench_plan`]), cada sendero repartido de forma pareja entre
/// el cruce y los faroles de su entrada, lejos de `keep_off`.
fn benches(area: Aabb, plan: (&[usize], usize, bool), keep_off: &[Aabb]) -> Vec<Bench> {
    let (arms, per_side, both) = plan;
    let (c, half) = (area.center(), area.size() * 0.5);
    let lateral = PLAZA_PATH_WIDTH / 2.0 + Bench::PATH_GAP + Bench::DEPTH / 2.0;
    let start = PLAZA_CROSSING / 2.0 + PlazaFurniture::CROSSING_GAP + Bench::LENGTH / 2.0;
    let mut out = Vec::new();
    let paths = [
        (Side::East, Side::North),
        (Side::North, Side::West),
        (Side::West, Side::South),
        (Side::South, Side::East),
    ];
    for (n, across) in arms.iter().map(|&k| paths[k]) {
        let (n, left) = (
            crate::places::side_normal(n),
            crate::places::side_normal(across),
        );
        let reach = n.x.abs() * half.x + n.y.abs() * half.y;
        let end = reach - PlazaFurniture::EDGE_GAP - Bench::LENGTH / 2.0;
        if end < start {
            continue;
        }
        let pitch = Bench::LENGTH + Bench::SPACING;
        let fit = ((end - start) / pitch) as usize + 1;
        let count = per_side.min(fit);
        for k in 0..count {
            let t = if count == 1 {
                (start + end) / 2.0
            } else {
                start + (end - start) * k as f32 / (count - 1) as f32
            };
            for s in if both { &[1.0, -1.0][..] } else { &[1.0][..] } {
                let s = *s;
                let at = c + n * t + left * (s * lateral);
                let facing = if s > 0.0 { across.opposite() } else { across };
                let bench = Bench { at, facing };
                let room = bench.footprint().expanded(0.1);
                if keep_off.iter().all(|o| !o.intersects(&room)) {
                    out.push(bench);
                }
            }
        }
    }
    out
}

/// La reja alrededor de `area`: a lo largo de cada borde, justo por dentro,
/// abierta en la entrada del sendero. Los tramos norte y sur llegan hasta las
/// esquinas; los del este y el oeste van entre ellos.
fn railing(area: Aabb) -> Vec<Aabb> {
    let (i, d) = (PlazaFurniture::RAILING_INSET, PlazaFurniture::RAILING_DEPTH);
    let (min, max, c) = (area.min, area.max, area.center());
    let m = PlazaFurniture::MOUTH_HALF;
    let mut out = Vec::new();
    for (lo, hi) in [(min.y + i, min.y + i + d), (max.y - i - d, max.y - i)] {
        out.push(Aabb::new(Vec2::new(min.x + i, lo), Vec2::new(c.x - m, hi)));
        out.push(Aabb::new(Vec2::new(c.x + m, lo), Vec2::new(max.x - i, hi)));
    }
    let (y0, y1) = (min.y + i + d, max.y - i - d);
    for (lo, hi) in [(min.x + i, min.x + i + d), (max.x - i - d, max.x - i)] {
        out.push(Aabb::new(Vec2::new(lo, y0), Vec2::new(hi, c.y - m)));
        out.push(Aabb::new(Vec2::new(lo, c.y + m), Vec2::new(hi, y1)));
    }
    out
}

impl TownLayout {
    /// Construye el pueblo que describe `config`, o explica por qué no se
    /// puede.
    pub fn generate(config: &TownConfig) -> Result<Self, TownConfigError> {
        let grid = Grid::validated(config)?;
        let size = config.size();
        let main = main_plaza(config, size)?;
        let plaza = grid.span(main);
        let bounds = grid.bounds();
        let extent = grid.extent(config.railway);

        let police = config.police_count();
        let (landmarks, station) = place_landmarks(&grid, main, size, police, config.railway);
        let mut reserved: Vec<(usize, usize)> = landmarks.iter().map(|l| l.block).collect();
        reserved.extend(main.blocks());
        reserved.extend(station.map(|s| s.block));
        let windmills = windmill_blocks(&grid, &reserved);
        let mut buildings = Vec::new();
        for j in 0..grid.rows {
            for i in 0..grid.cols {
                if main.contains((i, j)) {
                    continue;
                }
                let landmark = landmarks.iter().find(|c| c.block == (i, j));
                fill_block(
                    &grid,
                    (i, j),
                    landmark,
                    windmills.contains(&(i, j)),
                    station.filter(|s| s.block == (i, j)).map(|s| s.area),
                    config,
                    &mut buildings,
                );
            }
        }
        // El primer almacén, la iglesia y la comisaría (si el pueblo tiene una)
        // van primero, en el orden de las manzanas; los puntos de referencia
        // extra siguen en el orden en que se colocaron, así que el primer
        // almacén de ramos generales que se encuentra es aquel en el que está
        // atado el caballo.
        let primary = LANDMARK_PREFERENCES.len() - usize::from(police == 0);
        for extra in &landmarks[primary..] {
            let inside = grid.block(extra.block.0, extra.block.1);
            if let Some(k) = buildings
                .iter()
                .position(|b| b.kind == extra.kind && inside.contains(&b.footprint))
            {
                let landmark = buildings.remove(k);
                buildings.push(landmark);
            }
        }
        let yard_start = buildings.len();
        let railway = config.railway.map(|r| {
            let (half, track) = grid.yard_half(r);
            let along = grid.along_half(track);
            // La estación de una línea que atraviesa el pueblo está en el medio
            // del pueblo; una al costado del pueblo mira hacia la plaza.
            let wanted = if r.through {
                0.0
            } else {
                (plaza.center() - bounds.center()).dot(axis_along(track))
            };
            buildings.extend(station_yard(half, track, r.offset, along, wanted));
            railway_line(half, track, r.offset, along)
        });
        let yard: Vec<Aabb> = buildings[yard_start..]
            .iter()
            .map(|b| b.footprint)
            .collect();

        let store = buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .expect("every town has a general store");
        let hitching_post = hitching_post(store, veranda_depth(grid.street));
        let (w, h) = main.span();
        let mut town = Self {
            name: config.name.clone(),
            streets: grid.streets(main, &yard),
            plaza,
            station_plaza: station.map(|s| s.area),
            size,
            trees: plaza_trees(plaza, (more_steps(4 * w), more_steps(4 * h)))
                .into_iter()
                .chain(station.into_iter().flat_map(|s| {
                    plaza_trees(
                        s.area,
                        (more_steps(s.tree_steps.0), more_steps(s.tree_steps.1)),
                    )
                }))
                .collect(),
            shade_trees: Vec::new(),
            yard_plants: Vec::new(),
            railway,
            buildings,
            hitching_post,
            palenques: Vec::new(),
            seed: config.seed,
            townsfolk: config.townsfolk_count(),
            blocks: config.blocks,
            grid: bounds,
            extent,
            arrival: arrival(&grid, main, config.railway),
            band: grid.band(),
            sidewalks: sidewalks(&grid, main, Paving::of(config.blocks)),
        };
        town.palenques = town.place_palenques();
        town.shade_trees = town.place_shade_trees(&grid, main);
        town.yard_plants = town.place_yard_plants();
        Ok(town)
    }

    /// Un pequeño pueblo ferroviario de la pampa inspirado en asentamientos de
    /// La Pampa de los años 1920–1930, como Eduardo Castex, generado a partir
    /// de [`TownConfig::castex`]. Toma el patrón general (cuadrícula, plaza,
    /// vía férrea en el borde, almacén de ramos generales, galpones de cereal,
    /// molinos) y no es una reconstrucción de ningún pueblo real.
    pub fn castex_inspired() -> Self {
        Self::generate(&TownConfig::castex()).expect("the built-in Castex config is valid")
    }

    /// Sus veredas de ladrillo: a lo largo de todo frente de manzana en un
    /// pueblo grande, alrededor de la plaza en uno chico, ninguna en un
    /// pueblito ([`Paving`]).
    pub fn sidewalks(&self) -> &[Sidewalk] {
        &self.sidewalks
    }

    /// Todas las plazas: la principal y después la plaza de la estación de un
    /// pueblo más grande.
    pub fn plazas(&self) -> impl Iterator<Item = Aabb> + '_ {
        std::iter::once(self.plaza).chain(self.station_plaza)
    }

    /// Lo que hay en cada una de las [`Self::plazas`], en el mismo orden: su
    /// monumento, cruce, bancos y reja ([`PlazaFurniture`]), lejos de sus
    /// árboles y carteles.
    pub fn plaza_furniture(&self) -> Vec<PlazaFurniture> {
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        let keep_off: Vec<Aabb> = self
            .trees
            .iter()
            .map(|&p| Aabb::from_center_size(p, trunk))
            .chain(self.sign_obstacles())
            .collect();
        let paving = Paving::of(self.blocks);
        self.plazas()
            .enumerate()
            .map(|(k, p)| PlazaFurniture::of(p, paving, k == 0, &keep_off))
            .collect()
    }

    /// Cuántas personas caminan por las calles del pueblo: el `townsfolk` de su
    /// configuración o, si no, 16 en un pueblo chico y cuatro por cada cinco
    /// manzanas (al menos 16) en los más grandes.
    pub fn townsfolk_count(&self) -> usize {
        self.townsfolk
    }

    /// Manzanas `(east_west, north_south)`, tomadas de su configuración.
    pub fn blocks(&self) -> (usize, usize) {
        self.blocks
    }

    /// Rectángulo envolvente de la cuadrícula de calles, calle perimetral
    /// incluida.
    pub fn grid(&self) -> Aabb {
        self.grid
    }

    /// Rectángulo envolvente de todo lo construido: la cuadrícula, la playa de
    /// la estación y el corredor de la vía férrea dentro del pueblo.
    pub fn extent(&self) -> Aabb {
        self.extent
    }

    /// La extensión más la franja donde el terreno nivelado se funde de nuevo
    /// con la pampa: todo lo que el pueblo cambia en el suelo.
    pub fn leveled(&self) -> Aabb {
        self.extent.expanded(Self::LEVELED_BAND)
    }

    /// La franja a lo largo de la cual corre la vía férrea, unos metros a cada
    /// lado de la vía, de un extremo de la línea al otro.
    pub fn rail_corridor(&self) -> Option<Aabb> {
        let r = self.railway?;
        let min = Vec2::new(r.start.x.min(r.end.x), r.start.y.min(r.end.y));
        let max = Vec2::new(r.start.x.max(r.end.x), r.start.y.max(r.end.y));
        Some(Aabb::new(min, max).expanded(Self::RAIL_CORRIDOR_HALF))
    }

    /// La franja entre las dos mitades de un pueblo que creció a ambos lados de
    /// su línea ([`RailwayConfig::through`]), de un extremo de la cuadrícula al
    /// otro: la vía, la playa de la estación y los pasos a nivel. `None` para
    /// un pueblo al costado de su línea o sin ella.
    pub fn rail_band(&self) -> Option<Aabb> {
        self.band
    }

    /// Las calles que cruzan una [`Self::rail_band`] en un paso a nivel: la
    /// línea central de cada una a través de la franja, desde el medio de la
    /// calle perimetral de un lado hasta el medio de la del otro, de sur a
    /// norte y de oeste a este.
    pub fn crossing_streets(&self) -> Vec<(Vec2, Vec2)> {
        let Some(band) = self.band else {
            return Vec::new();
        };
        let mut out: Vec<(Vec2, Vec2)> = self
            .streets
            .iter()
            .filter(|s| s.size().y > s.size().x)
            .filter(|s| s.min.y < band.min.y && s.max.y > band.max.y)
            .map(|s| {
                let (x, half) = (s.center().x, s.size().x / 2.0);
                (
                    Vec2::new(x, band.min.y - half),
                    Vec2::new(x, band.max.y + half),
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        out
    }

    /// Terreno a nivelar debajo del pueblo, a la altura natural de su centro.
    pub fn flat_area(&self, terrain: &Terrain) -> FlatArea {
        let extent = self.extent();
        let center = extent.center();
        FlatArea {
            center,
            half_extents: extent.size() * 0.5,
            falloff: Self::LEVELED_BAND,
            height: terrain.natural_height(center.x, center.y),
        }
    }

    /// Huellas de colisión estáticas: el [`Building::collider`] de cada
    /// edificio salvo el del andén (un piso por el que camina la gente, ver
    /// [`Self::floors`]), cada tronco de árbol (los de los anillos de las
    /// plazas y los de los [`ShadeTree`]s), el palenque con su caballo, los
    /// demás palenques con sus caballos, los carteles independientes, y el
    /// monumento, los bancos y la reja de cada plaza
    /// ([`PlazaFurniture::obstacles`]).
    pub fn obstacles(&self) -> Vec<Aabb> {
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        self.buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(Building::collider)
            .chain(self.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)))
            .chain(self.shade_trees.iter().map(ShadeTree::obstacle))
            .chain([self.hitching_post.rail, self.hitching_post.horse.footprint])
            .chain(self.palenques.iter().flat_map(Palenque::obstacles))
            .chain(self.sign_obstacles())
            .chain(
                self.plaza_furniture()
                    .iter()
                    .flat_map(PlazaFurniture::obstacles)
                    .collect::<Vec<_>>(),
            )
            .collect()
    }

    /// Dónde empieza un jinete que llega desde la pampa abierta: en el camino
    /// principal fuera de la cuadrícula, del lado opuesto a la vía férrea,
    /// mirando hacia el pueblo.
    pub fn arrival(&self) -> (Vec2, Heading) {
        self.arrival
    }

    /// Pisos elevados por los que camina la gente: el andén de la estación, a
    /// lo largo de la vía.
    pub fn floors(&self) -> Vec<Floor> {
        self.buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Platform)
            .map(|b| Floor {
                footprint: b.footprint,
                height: b.height,
                along: match b.front {
                    Side::North | Side::South => Vec2::new(1.0, 0.0),
                    Side::East | Side::West => Vec2::new(0.0, 1.0),
                },
            })
            .collect()
    }

    /// Techos sobre el andén: el techo de cada estación, que sobresale
    /// [`Canopy::DEPTH`] desde su pared del lado de la vía (la trasera, opuesta
    /// a su frente sobre la calle) a lo largo de toda la pared.
    pub fn canopies(&self) -> Vec<Canopy> {
        self.buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Station)
            .map(|b| {
                let (f, d) = (b.footprint, Canopy::DEPTH);
                let (min, max) = match b.front.opposite() {
                    Side::North => (Vec2::new(f.min.x, f.max.y), Vec2::new(f.max.x, f.max.y + d)),
                    Side::South => (Vec2::new(f.min.x, f.min.y - d), Vec2::new(f.max.x, f.min.y)),
                    Side::East => (Vec2::new(f.max.x, f.min.y), Vec2::new(f.max.x + d, f.max.y)),
                    Side::West => (Vec2::new(f.min.x - d, f.min.y), Vec2::new(f.min.x, f.max.y)),
                };
                Canopy {
                    footprint: Aabb::new(min, max),
                    height: Canopy::HEIGHT,
                }
            })
            .collect()
    }
}

impl TownLayout {
    /// Ancho de la franja alrededor del pueblo donde el terreno nivelado se
    /// funde de nuevo con la pampa, en metros.
    pub const LEVELED_BAND: f32 = 60.0;
    /// Cuánto sigue la vía más allá de cada extremo de la cuadrícula, en
    /// metros: el tren entra y sale de la línea del pueblo ahí afuera, en la
    /// niebla.
    pub const RAIL_RUN_OUT: f32 = 400.0;
    /// La mitad del ancho de [`Self::rail_corridor`], en metros.
    pub const RAIL_CORRIDOR_HALF: f32 = 4.0;
}

/// Trocha ancha, en metros.
const RAIL_GAUGE: f32 = 1.676;
/// Tamaño de manzana para el que se dibujaron las proporciones de los puntos de
/// referencia y de los lotes, en metros.
const REFERENCE_BLOCK: f32 = 40.0;
/// A qué distancia fuera de la cuadrícula llega un jinete, en metros.
const ARRIVAL_DISTANCE: f32 = 25.0;

/// La geometría de la cuadrícula de calles, derivada de una configuración
/// validada.
#[derive(Debug, Clone, Copy)]
struct Grid {
    origin: Vec2,
    cols: usize,
    rows: usize,
    block: f32,
    street: f32,
    /// Una línea que atraviesa el pueblo entre sus dos mitades.
    split: Option<Split>,
}

/// Dónde divide la cuadrícula en dos mitades una línea que atraviesa el pueblo.
#[derive(Debug, Clone, Copy)]
struct Split {
    /// La primera fila de manzanas al norte de la línea.
    row: usize,
    /// Ancho de la franja entre las calles perimetrales de las dos mitades, en
    /// metros.
    gap: f32,
}

/// La primera fila de manzanas al norte de una línea que atraviesa un pueblo de
/// `rows` filas.
fn split_row(rows: usize) -> usize {
    rows / 2
}

impl Grid {
    /// La cuadrícula de `config`, después de comprobar cada uno de sus números.
    fn validated(config: &TownConfig) -> Result<Self, TownConfigError> {
        if config.name.trim().is_empty() {
            return Err(TownConfigError::EmptyName);
        }
        let finite = [
            (
                "origin",
                config.origin.x.is_finite() && config.origin.y.is_finite(),
            ),
            ("block_size", config.block_size.is_finite()),
            ("street_width", config.street_width.is_finite()),
            (
                "railway.offset",
                config.railway.is_none_or(|r| r.offset.is_finite()),
            ),
        ];
        if let Some((field, _)) = finite.into_iter().find(|(_, ok)| !ok) {
            return Err(TownConfigError::NotFinite { field });
        }
        let (cols, rows) = config.blocks;
        let min = TownConfig::MIN_BLOCKS;
        if cols < min || rows < min {
            return Err(TownConfigError::TooFewBlocks {
                blocks: config.blocks,
                min,
            });
        }
        if config.block_size < TownConfig::MIN_BLOCK_SIZE {
            return Err(TownConfigError::BlockTooSmall {
                block_size: config.block_size,
                min: TownConfig::MIN_BLOCK_SIZE,
            });
        }
        if config.street_width < TownConfig::MIN_STREET_WIDTH {
            return Err(TownConfigError::StreetTooNarrow {
                street_width: config.street_width,
                min: TownConfig::MIN_STREET_WIDTH,
            });
        }
        if let Some(r) = config.railway
            && r.offset < TownConfig::MIN_RAILWAY_OFFSET
        {
            return Err(TownConfigError::RailwayTooClose {
                offset: r.offset,
                min: TownConfig::MIN_RAILWAY_OFFSET,
            });
        }
        if let Some(density) = config.density
            && !(0.0..=1.0).contains(&density)
        {
            return Err(TownConfigError::BadDensity { density });
        }
        let through = config.railway.filter(|r| r.through);
        if let Some(r) = through {
            if matches!(r.side, Side::East | Side::West) {
                return Err(TownConfigError::ThroughNotEastWest { side: r.side });
            }
            if rows < RailwayConfig::MIN_THROUGH_ROWS {
                return Err(TownConfigError::ThroughTooFewRows {
                    rows,
                    min: RailwayConfig::MIN_THROUGH_ROWS,
                });
            }
        }
        // Solo se reservan las manzanas de los puntos de referencia de la
        // categoría: entran menos comisarías, pero no más.
        let max = config.size().police_stations();
        if let Some(wanted) = config.police_stations
            && wanted > max
        {
            return Err(TownConfigError::TooManyPoliceStations { wanted, max });
        }
        if let Some(wanted) = config.townsfolk
            && wanted > TownConfig::MAX_TOWNSFOLK
        {
            return Err(TownConfigError::TooManyTownsfolk {
                wanted,
                max: TownConfig::MAX_TOWNSFOLK,
            });
        }
        Ok(Self {
            origin: config.origin,
            cols,
            rows,
            block: config.block_size,
            street: config.street_width,
            split: through.map(|r| Split {
                row: split_row(rows),
                gap: r.offset + RailwayConfig::THROUGH_FAR_SIDE,
            }),
        })
    }

    /// La mitad del tamaño de la cuadrícula, calle perimetral incluida (y, si
    /// una línea la divide, las calles perimetrales de ambas mitades y la
    /// franja entre ellas).
    fn half(&self) -> Vec2 {
        let span = |n: usize| n as f32 * self.block + (n + 1) as f32 * self.street;
        let split = self.split.map_or(0.0, |s| self.street + s.gap);
        Vec2::new(span(self.cols), span(self.rows) + split) * 0.5
    }

    fn bounds(&self) -> Aabb {
        Aabb::from_center_size(self.origin, self.half() * 2.0)
    }

    /// `x` inferior de la calle norte-sur `k` (0 = la calle perimetral oeste).
    fn lane_x(&self, k: usize) -> f32 {
        self.origin.x - self.half().x + k as f32 * (self.block + self.street)
    }

    /// `y` inferior de la calle este-oeste `k` (0 = la calle perimetral sur).
    /// Pasando una línea que atraviesa el pueblo, las `k` a partir de su fila
    /// de división son las calles de la mitad norte ([`Self::south_lane`] es la
    /// última de la mitad sur).
    fn lane_y(&self, k: usize) -> f32 {
        let shift = self
            .split
            .filter(|s| k >= s.row)
            .map_or(0.0, |s| self.street + s.gap);
        self.origin.y - self.half().y + k as f32 * (self.block + self.street) + shift
    }

    /// `y` inferior de la calle perimetral norte de la mitad sur, junto a una
    /// línea que atraviesa el pueblo.
    fn south_lane(&self) -> Option<f32> {
        let s = self.split?;
        Some(self.origin.y - self.half().y + s.row as f32 * (self.block + self.street))
    }

    /// La franja entre las dos mitades de una cuadrícula dividida.
    fn band(&self) -> Option<Aabb> {
        let s = self.split?;
        let b = self.bounds();
        let lo = self.south_lane()? + self.street;
        Some(Aabb::new(
            Vec2::new(b.min.x, lo),
            Vec2::new(b.max.x, lo + s.gap),
        ))
    }

    /// La parte de la cuadrícula contra la que se coloca la playa de la
    /// estación, y el lado de ella por el que corre la vía: toda la cuadrícula
    /// y el lado de la vía férrea o, para una línea que atraviesa el pueblo, la
    /// mitad en la que está la estación y su lado que mira hacia la vía.
    fn yard_half(&self, railway: RailwayConfig) -> (Aabb, Side) {
        let b = self.bounds();
        match self.band() {
            Some(band) if railway.side == Side::North => (
                Aabb::new(Vec2::new(b.min.x, band.max.y), b.max),
                Side::South,
            ),
            Some(band) => (
                Aabb::new(b.min, Vec2::new(b.max.x, band.min.y)),
                Side::North,
            ),
            None => (b, railway.side),
        }
    }

    /// Huella de la manzana `(i, j)`, contada desde la esquina sudoeste.
    fn block(&self, i: usize, j: usize) -> Aabb {
        let min = Vec2::new(self.lane_x(i) + self.street, self.lane_y(j) + self.street);
        Aabb::new(min, min + Vec2::new(self.block, self.block))
    }

    /// Las manzanas de `plaza` fusionadas en una sola área, incluidas las
    /// calles entre ellas.
    fn span(&self, plaza: PlazaBlocks) -> Aabb {
        let (lo, hi) = (plaza.min, plaza.max);
        Aabb::new(self.block(lo.0, lo.1).min, self.block(hi.0, hi.1).max)
    }

    /// Todas las calles norte-sur, de oeste a este, y después todas las calles
    /// este-oeste, de sur a norte. Una calle que corre entre las manzanas de
    /// una `plaza` fusionada se detiene en sus bordes: se convierte en dos
    /// tramos, uno a cada lado. Una calle norte-sur que llegaría a la playa de
    /// la estación `yard` de una línea que atraviesa el pueblo se detiene en la
    /// franja a cada lado; las demás la cruzan.
    fn streets(&self, plaza: PlazaBlocks, yard: &[Aabb]) -> Vec<Aabb> {
        let b = self.bounds();
        let area = self.span(plaza);
        let band = self.band();
        // `lane` de `lo` a `hi` a lo largo de ella, menos cada uno de los
        // `cuts`.
        let pieces = |lo: f32, hi: f32, cuts: &[(f32, f32)]| {
            let mut out = vec![(lo, hi)];
            for &(c0, c1) in cuts {
                out = out
                    .into_iter()
                    .flat_map(|(a, z)| {
                        if c1 <= a || c0 >= z {
                            vec![(a, z)]
                        } else {
                            vec![(a, c0), (c1, z)]
                        }
                    })
                    .filter(|(a, z)| z > a)
                    .collect();
            }
            out
        };
        let mut out = Vec::new();
        for k in 0..=self.cols {
            let x = self.lane_x(k);
            let inner = plaza.min.0 < k && k <= plaza.max.0;
            let mut cuts: Vec<(f32, f32)> = inner
                .then_some((area.min.y, area.max.y))
                .into_iter()
                .collect();
            if let Some(band) = band {
                let lane = Aabb::new(
                    Vec2::new(x, band.min.y),
                    Vec2::new(x + self.street, band.max.y),
                )
                .expanded(YARD_CLEARANCE);
                if yard.iter().any(|y| y.intersects(&lane)) {
                    cuts.push((band.min.y, band.max.y));
                }
            }
            for (y0, y1) in pieces(b.min.y, b.max.y, &cuts) {
                out.push(Aabb::new(Vec2::new(x, y0), Vec2::new(x + self.street, y1)));
            }
        }
        for k in 0..=self.rows {
            // La franja de una línea que atraviesa el pueblo: primero la calle
            // perimetral de la mitad sur.
            if let (Some(split), Some(y)) = (self.split, self.south_lane())
                && split.row == k
            {
                out.push(Aabb::new(
                    Vec2::new(b.min.x, y),
                    Vec2::new(b.max.x, y + self.street),
                ));
            }
            let y = self.lane_y(k);
            let inner = plaza.min.1 < k && k <= plaza.max.1;
            let cut = inner.then_some((area.min.x, area.max.x));
            let cuts: Vec<(f32, f32)> = cut.into_iter().collect();
            for (x0, x1) in pieces(b.min.x, b.max.x, &cuts) {
                out.push(Aabb::new(Vec2::new(x0, y), Vec2::new(x1, y + self.street)));
            }
        }
        out
    }

    /// La cuadrícula más la playa de la estación y el corredor de la vía férrea
    /// al lado (los de una línea que atraviesa el pueblo quedan dentro de
    /// ella).
    fn extent(&self, railway: Option<RailwayConfig>) -> Aabb {
        let grid = self.bounds();
        let Some(r) = railway.filter(|r| !r.through) else {
            return grid;
        };
        let along = self.along_half(r.side);
        let yard = yard_box(grid, r.side, (-along, along), (0.0, r.offset + 4.0));
        Aabb::new(
            Vec2::new(grid.min.x.min(yard.min.x), grid.min.y.min(yard.min.y)),
            Vec2::new(grid.max.x.max(yard.max.x), grid.max.y.max(yard.max.y)),
        )
    }

    /// La mitad del largo de la cuadrícula a lo largo de una vía del lado
    /// `side`.
    fn along_half(&self, side: Side) -> f32 {
        match side {
            Side::North | Side::South => self.half().x,
            Side::East | Side::West => self.half().y,
        }
    }

    /// Escala de las proporciones relativas a la manzana.
    fn scale(&self) -> f32 {
        self.block / REFERENCE_BLOCK
    }
}

/// El índice más cercano al medio de `n` manzanas; en caso de empate gana el
/// índice más bajo.
fn middle(n: usize) -> usize {
    (n - 1) / 2
}

/// La manzana que contiene la plaza.
///
/// Con una línea que atraviesa el pueblo ([`RailwayConfig::through`]), mirar a
/// la estación es la fila junto a la vía en la mitad de la estación; y `Auto`
/// toma, en esa mitad, la manzana una columna al oeste del medio y dos filas
/// hacia atrás desde la vía (la última fila si hay menos), para que la plaza
/// principal quede a unas pocas manzanas de la estación, como en los grandes
/// pueblos ferroviarios.
fn plaza_block(config: &TownConfig) -> Result<(usize, usize), TownConfigError> {
    let (cols, rows) = config.blocks;
    let through = config.railway.filter(|r| r.through);
    let central = cols % 2 == 1 && rows % 2 == 1 && through.is_none();
    let row = split_row(rows);
    let facing = |side: Side| match (side, through.is_some()) {
        (Side::North, true) => (middle(cols), row),
        (_, true) => (middle(cols), row - 1),
        (Side::North, false) => (middle(cols), rows - 1),
        (Side::South, false) => (middle(cols), 0),
        (Side::East, false) => (cols - 1, middle(rows)),
        (Side::West, false) => (0, middle(rows)),
    };
    if let (Some(r), PlazaPlacement::Auto) = (through, config.plaza) {
        let i = middle(cols).saturating_sub(1);
        let j = match r.side {
            Side::North => (row + 2).min(rows - 1),
            _ => (row - 1).saturating_sub(2),
        };
        return Ok((i, j));
    }
    match config.plaza {
        PlazaPlacement::Center if central => Ok((cols / 2, rows / 2)),
        PlazaPlacement::Center => Err(TownConfigError::NoCentralBlock {
            blocks: config.blocks,
        }),
        PlazaPlacement::Block(i, j) if i < cols && j < rows => Ok((i, j)),
        PlazaPlacement::Block(i, j) => Err(TownConfigError::PlazaOutOfRange {
            block: (i, j),
            blocks: config.blocks,
        }),
        PlazaPlacement::FacingStation => config
            .railway
            .map(|r| facing(r.side))
            .ok_or(TownConfigError::NoStationToFace),
        PlazaPlacement::Auto if central => Ok((cols / 2, rows / 2)),
        PlazaPlacement::Auto => Ok(config
            .railway
            .map_or((middle(cols), middle(rows)), |r| facing(r.side))),
    }
}

/// Las manzanas de la plaza principal: columnas `min.0..=max.0`, filas
/// `min.1..=max.1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PlazaBlocks {
    min: (usize, usize),
    max: (usize, usize),
}

impl PlazaBlocks {
    fn contains(&self, (i, j): (usize, usize)) -> bool {
        (self.min.0..=self.max.0).contains(&i) && (self.min.1..=self.max.1).contains(&j)
    }

    /// Manzanas `(east-west, north-south)`.
    fn span(&self) -> (usize, usize) {
        (self.max.0 - self.min.0 + 1, self.max.1 - self.min.1 + 1)
    }

    fn blocks(&self) -> impl Iterator<Item = (usize, usize)> {
        let (lo, hi) = (self.min, self.max);
        (lo.1..=hi.1).flat_map(move |j| (lo.0..=hi.0).map(move |i| (i, j)))
    }

    /// Las manzanas justo después del lado `side` de la plaza (de oeste a este,
    /// o de sur a norte), posiblemente fuera de la cuadrícula.
    fn beside(&self, side: Side) -> Vec<(isize, isize)> {
        let (lo, hi) = (
            (self.min.0 as isize, self.min.1 as isize),
            (self.max.0 as isize, self.max.1 as isize),
        );
        match side {
            Side::West => (lo.1..=hi.1).map(|j| (lo.0 - 1, j)).collect(),
            Side::East => (lo.1..=hi.1).map(|j| (hi.0 + 1, j)).collect(),
            Side::South => (lo.0..=hi.0).map(|i| (i, lo.1 - 1)).collect(),
            Side::North => (lo.0..=hi.0).map(|i| (i, hi.1 + 1)).collect(),
        }
    }

    /// Manzanas desde `b` hasta la plaza a lo largo de cada eje: positivo
    /// cuando la plaza está al este (al norte), cero cuando `b` está a su misma
    /// altura.
    fn offset(&self, (i, j): (usize, usize)) -> (isize, isize) {
        let toward = |v: usize, lo: usize, hi: usize| {
            if v < lo {
                (lo - v) as isize
            } else if v > hi {
                -((v - hi) as isize)
            } else {
                0
            }
        };
        (
            toward(i, self.min.0, self.max.0),
            toward(j, self.min.1, self.max.1),
        )
    }

    /// El lado de una manzana en `offset` (de [`Self::offset`]) que mira hacia
    /// la plaza, a lo largo del eje con el desplazamiento mayor.
    fn facing((di, dj): (isize, isize)) -> Side {
        if di.abs() >= dj.abs() {
            if di > 0 { Side::East } else { Side::West }
        } else if dj > 0 {
            Side::North
        } else {
            Side::South
        }
    }
}

/// Las manzanas de la plaza principal. La plaza de un pueblo chico es la
/// manzana que elige [`PlazaPlacement`]. La de un pueblo más grande abarca
/// varias ([`TownSize::plaza_span`]): centrada en la cuadrícula para `Auto` y
/// `Center` (los empates van al oeste o al sur) y, si no, cubriendo la manzana
/// elegida, tan cerca del centro como lo permita.
fn main_plaza(config: &TownConfig, size: TownSize) -> Result<PlazaBlocks, TownConfigError> {
    let anchor = plaza_block(config)?;
    let (w, h) = size.plaza_span(config.blocks);
    let through = config.railway.is_some_and(|r| r.through);
    let centered =
        matches!(config.plaza, PlazaPlacement::Auto | PlazaPlacement::Center) && !through;
    let start = |n: usize, len: usize, a: usize| {
        let mid = (n - len) / 2;
        if size == TownSize::Small {
            a
        } else if centered {
            mid
        } else {
            mid.clamp(a.saturating_sub(len - 1), a.min(n - len))
        }
    };
    let min = (
        start(config.blocks.0, w, anchor.0),
        start(config.blocks.1, h, anchor.1),
    );
    let plaza = PlazaBlocks {
        min,
        max: (min.0 + w - 1, min.1 + h - 1),
    };
    let row = split_row(config.blocks.1);
    if through && plaza.min.1 < row && plaza.max.1 >= row {
        return Err(TownConfigError::PlazaAcrossRailway);
    }
    Ok(plaza)
}

/// Un edificio de referencia que mira hacia la plaza desde una manzana cercana.
#[derive(Debug, Clone, Copy)]
struct Landmark {
    kind: BuildingKind,
    block: (usize, usize),
    /// El lado de la manzana en el que está (el lado de su fachada).
    front: Side,
}

/// Dónde prefiere estar cada punto de referencia, visto desde la plaza, en
/// orden. Una plaza central tiene el almacén al oeste, la iglesia al norte y la
/// comisaría al este.
const LANDMARK_PREFERENCES: [(BuildingKind, [Side; 4]); 3] = [
    (
        BuildingKind::GeneralStore,
        [Side::West, Side::East, Side::South, Side::North],
    ),
    (
        BuildingKind::Church,
        [Side::North, Side::South, Side::East, Side::West],
    ),
    (
        BuildingKind::PoliceStation,
        [Side::East, Side::West, Side::South, Side::North],
    ),
];

/// Coloca los puntos de referencia del pueblo, repartidos por el pueblo para
/// que tenga vida en todas partes (usuario, 2026-10-23), y la plaza de la
/// estación de un pueblo más grande. Los devuelve como el almacén de ramos
/// generales principal, la iglesia y la primera comisaría (salvo que
/// `police_count` sea cero), y después el resto: el almacén de la estación de
/// un pueblo más grande, las demás comisarías y cualquier almacén extra.
///
/// La iglesia mira hacia la plaza principal desde la primera manzana vecina
/// libre según sus preferencias (ver [`LANDMARK_PREFERENCES`]). Después vienen
/// la plaza de la estación de un pueblo más grande y su almacén (ver
/// [`station_plaza`] y [`station_store`]). Todos los demás puntos de referencia
/// se dispersan ([`disperse`]).
fn place_landmarks(
    grid: &Grid,
    plaza: PlazaBlocks,
    size: TownSize,
    police_count: usize,
    railway: Option<RailwayConfig>,
) -> (Vec<Landmark>, Option<StationPlaza>) {
    let [store, church, police] = LANDMARK_PREFERENCES;
    let none = |_: (usize, usize)| false;
    let church = place_landmark(grid, plaza, church.0, &church.1, &none);
    let mut placed = vec![church];
    let station = (size != TownSize::Small).then(|| station_plaza(grid, plaza, &placed, railway));
    let mut rest = Vec::new();
    if let Some(station) = &station {
        let s = station_store(grid, plaza, station, &placed);
        placed.push(s);
        rest.push(s);
    }
    let reserved: Vec<(usize, usize)> = station.iter().map(|s| s.block).collect();
    let main_store = disperse(grid, plaza, store.0, &placed, &reserved);
    placed.push(main_store);
    let mut first = vec![main_store, church];
    let extra_stores = size.general_stores() - 1 - usize::from(size != TownSize::Small);
    let kinds = std::iter::repeat_n(police.0, police_count)
        .chain(std::iter::repeat_n(store.0, extra_stores));
    for (k, kind) in kinds.enumerate() {
        let landmark = disperse(grid, plaza, kind, &placed, &reserved);
        placed.push(landmark);
        if k == 0 && kind == BuildingKind::PoliceStation {
            first.push(landmark);
        } else {
            rest.push(landmark);
        }
    }
    first.extend(rest);
    (first, station)
}

/// Manzanas entre dos edificios cívicos, como mínimo: nunca uno al lado del
/// otro.
const CIVIC_SPACING: usize = 2;

/// Manzanas mínimas entre dos puntos de referencia del mismo tipo en un pueblo
/// de `cols` x `rows`: más o menos la mitad del pueblo de distancia.
fn same_kind_spacing(cols: usize, rows: usize) -> usize {
    CIVIC_SPACING.max(cols.max(rows) / 2)
}

/// Manzanas de distancia, el máximo a lo largo de cualquiera de los ejes.
fn blocks_apart(a: (usize, usize), b: (usize, usize)) -> usize {
    a.0.abs_diff(b.0).max(a.1.abs_diff(b.1))
}

/// Un punto de referencia de tipo `kind` disperso por el pueblo: en una manzana
/// que no sea la plaza, ni `reserved` (la de la plaza de la estación) ni
/// `placed`, al menos a [`CIVIC_SPACING`] manzanas de cada punto de referencia
/// `placed` y a [`same_kind_spacing`] de los de su tipo, tan cerca de la plaza
/// como eso permita (la menor cantidad de manzanas a lo largo de cualquiera de
/// los ejes, después la columna más baja y después la fila). En un pueblo
/// dividido por una línea que lo atraviesa, va a la mitad que tenga menos de su
/// tipo (la de la plaza en caso de empate), para que cada mitad tenga el suyo.
/// Cuando ninguna manzana mantiene la separación, toma la más alejada de las
/// demás. Mira hacia la plaza a lo largo del eje con el desplazamiento mayor.
fn disperse(
    grid: &Grid,
    plaza: PlazaBlocks,
    kind: BuildingKind,
    placed: &[Landmark],
    reserved: &[(usize, usize)],
) -> Landmark {
    let half = |b: (usize, usize)| grid.split.map(|s| b.1 >= s.row);
    let wanted = grid.split.map(|_| {
        let count = |north: bool| {
            placed
                .iter()
                .filter(|l| l.kind == kind && half(l.block) == Some(north))
                .count()
        };
        let plaza_north = half(plaza.min) == Some(true);
        match count(true).cmp(&count(false)) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => plaza_north,
        }
    });
    let same = same_kind_spacing(grid.cols, grid.rows);
    let free: Vec<(usize, usize)> = (0..grid.cols)
        .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
        .filter(|&b| !plaza.contains(b) && !reserved.contains(&b))
        .filter(|&b| placed.iter().all(|l| l.block != b))
        .filter(|&b| wanted.is_none() || half(b) == wanted)
        .collect();
    // Manzanas hasta la plaza, el máximo a lo largo de cualquiera de los ejes:
    // una manzana en diagonal cuenta tan cerca como una al lado, así que los
    // puntos de referencia todavía pueden abrirse en abanico alrededor de una
    // plaza chica.
    let to_plaza = |b: (usize, usize)| {
        let (di, dj) = plaza.offset(b);
        di.unsigned_abs().max(dj.unsigned_abs())
    };
    let nearest_civic = |b: (usize, usize)| {
        placed
            .iter()
            .map(|l| blocks_apart(l.block, b))
            .min()
            .unwrap_or(usize::MAX)
    };
    let spaced = |b: (usize, usize)| {
        placed.iter().all(|l| {
            let apart = blocks_apart(l.block, b);
            apart >= CIVIC_SPACING && (l.kind != kind || apart >= same)
        })
    };
    let block = free
        .iter()
        .copied()
        .filter(|&b| spaced(b))
        .min_by_key(|&b| (to_plaza(b), b.0, b.1))
        .or_else(|| {
            free.iter()
                .copied()
                .min_by_key(|&b| (std::cmp::Reverse(nearest_civic(b)), to_plaza(b), b.0, b.1))
        })
        .expect("a town has a free block for every landmark");
    Landmark {
        kind,
        block,
        front: PlazaBlocks::facing(plaza.offset(block)),
    }
}

/// Un punto de referencia de tipo `kind` que mira hacia `plaza`: la primera
/// manzana junto a ella, en el orden de `preferences` (de oeste a este, o de
/// sur a norte, a lo largo de un lado), que no sea la plaza ni esté en `taken`,
/// mirándola del otro lado de la calle; si no, la manzana de ese tipo más
/// cercana (la menor cantidad de manzanas de distancia, después la columna más
/// baja y después la fila más baja), mirando hacia la plaza a lo largo del eje
/// con el desplazamiento mayor.
fn place_landmark(
    grid: &Grid,
    plaza: PlazaBlocks,
    kind: BuildingKind,
    preferences: &[Side],
    taken: &dyn Fn((usize, usize)) -> bool,
) -> Landmark {
    let inside = |i: isize, j: isize| {
        (0..grid.cols as isize).contains(&i) && (0..grid.rows as isize).contains(&j)
    };
    let free = |b: (usize, usize)| !plaza.contains(b) && !taken(b);
    let neighbor = preferences.iter().find_map(|&toward| {
        plaza.beside(toward).into_iter().find_map(|(i, j)| {
            let block = (i as usize, j as usize);
            (inside(i, j) && free(block)).then_some(Landmark {
                kind,
                block,
                front: toward.opposite(),
            })
        })
    });
    neighbor.unwrap_or_else(|| {
        let mut blocks: Vec<(usize, usize)> = (0..grid.cols)
            .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
            .filter(|&b| free(b))
            .collect();
        blocks.sort_by_key(|&b| {
            let (di, dj) = plaza.offset(b);
            (di.unsigned_abs() + dj.unsigned_abs(), b.0, b.1)
        });
        let block = blocks[0];
        let front = PlazaBlocks::facing(plaza.offset(block));
        Landmark { kind, block, front }
    })
}

/// Manzanas con un molino y su tanque de agua en el fondo: la esquina sudoeste,
/// la columna este una fila por debajo de la superior y la segunda columna de
/// la fila superior, salteando las manzanas `reserved` (plazas y puntos de
/// referencia).
fn windmill_blocks(grid: &Grid, reserved: &[(usize, usize)]) -> Vec<(usize, usize)> {
    [(0, 0), (grid.cols - 1, grid.rows - 2), (1, grid.rows - 1)]
        .into_iter()
        .filter(|b| !reserved.contains(b))
        .collect()
}

/// La plaza de la estación de un pueblo más grande: la mitad de `block`, de su
/// lado `toward`.
#[derive(Debug, Clone, Copy)]
struct StationPlaza {
    block: (usize, usize),
    toward: Side,
    area: Aabb,
    /// Pasos del anillo de árboles a lo largo de x y de y: la mitad a lo ancho
    /// de la media manzana.
    tree_steps: (usize, usize),
}

/// Elige la plaza de la estación de un pueblo más grande.
///
/// Con vía férrea: la manzana de la fila del lado de la vía que mira a la
/// estación (que está alineada con la plaza principal; su manzana oeste o sur
/// cuando abarca dos) o, cuando esa manzana contiene la plaza principal o un
/// punto de referencia, la manzana libre más cercana a lo largo de esa fila
/// (los empates van al índice más bajo); la plaza ocupa la mitad más cercana a
/// la vía férrea. Sin vía férrea (o sin manzanas libres en esa fila): la
/// manzana libre más alejada de la plaza principal (los empates van a la
/// columna más baja y después a la fila); la plaza ocupa la mitad que mira
/// hacia la plaza principal.
fn station_plaza(
    grid: &Grid,
    plaza: PlazaBlocks,
    landmarks: &[Landmark],
    railway: Option<RailwayConfig>,
) -> StationPlaza {
    let free = |b: (usize, usize)| !plaza.contains(b) && landmarks.iter().all(|l| l.block != b);
    let on_row = railway.and_then(|r| {
        let (cols, rows) = (grid.cols, grid.rows);
        // La estación de una línea que atraviesa el pueblo está en el medio del
        // pueblo, en la fila de su mitad junto a la vía.
        let split = grid.split.map(|s| s.row);
        let (row, toward): (Vec<(usize, usize)>, Side) = match (r.side, split) {
            (Side::North, Some(s)) => ((0..cols).map(|i| (i, s)).collect(), Side::South),
            (_, Some(s)) => ((0..cols).map(|i| (i, s - 1)).collect(), Side::North),
            (Side::North, None) => ((0..cols).map(|i| (i, rows - 1)).collect(), r.side),
            (Side::South, None) => ((0..cols).map(|i| (i, 0)).collect(), r.side),
            (Side::East, None) => ((0..rows).map(|j| (cols - 1, j)).collect(), r.side),
            (Side::West, None) => ((0..rows).map(|j| (0, j)).collect(), r.side),
        };
        // Índice a lo largo de la fila de la manzana que mira a la estación.
        let along = |b: (usize, usize)| match r.side {
            Side::North | Side::South => b.0,
            Side::East | Side::West => b.1,
        };
        let facing = if split.is_some() {
            middle(cols)
        } else {
            along(plaza.min)
        };
        row.into_iter()
            .filter(|&b| free(b))
            .min_by_key(|&b| (along(b).abs_diff(facing), along(b)))
            .map(|b| (b, toward))
    });
    let (block, toward) = on_row.unwrap_or_else(|| {
        let block = (0..grid.cols)
            .flat_map(|i| (0..grid.rows).map(move |j| (i, j)))
            .filter(|&b| free(b))
            .min_by_key(|&b| {
                let (di, dj) = plaza.offset(b);
                let d = di.unsigned_abs() + dj.unsigned_abs();
                (std::cmp::Reverse(d), b.0, b.1)
            })
            .expect("a bigger town has free blocks");
        (block, PlazaBlocks::facing(plaza.offset(block)))
    });
    let b = grid.block(block.0, block.1);
    let area = side_box(b, toward, 0.0, grid.block, grid.block / 2.0);
    let tree_steps = match toward {
        Side::North | Side::South => (4, 2),
        Side::East | Side::West => (2, 4),
    };
    StationPlaza {
        block,
        toward,
        area,
        tree_steps,
    }
}

/// El almacén de ramos generales de la plaza de la estación: en una manzana
/// libre a su lado, mirándola del otro lado de la calle, nunca del lado de su
/// mitad con casas.
fn station_store(
    grid: &Grid,
    plaza: PlazaBlocks,
    station: &StationPlaza,
    landmarks: &[Landmark],
) -> Landmark {
    let preferences: Vec<Side> = [Side::West, Side::East, Side::South, Side::North]
        .into_iter()
        .filter(|&s| s != station.toward.opposite())
        .collect();
    let taken = |b: (usize, usize)| plaza.contains(b) || landmarks.iter().any(|l| l.block == b);
    let single = PlazaBlocks {
        min: station.block,
        max: station.block,
    };
    place_landmark(
        grid,
        single,
        BuildingKind::GeneralStore,
        &preferences,
        &taken,
    )
}

/// Rectángulo contra el borde `side` de la manzana `b`, de `a0` a `a1` a lo
/// largo de ese borde (de oeste a este, o de sur a norte) y con `depth` de
/// profundidad dentro de la manzana.
fn side_box(b: Aabb, side: Side, a0: f32, a1: f32, depth: f32) -> Aabb {
    match side {
        Side::South => Aabb::new(
            Vec2::new(b.min.x + a0, b.min.y),
            Vec2::new(b.min.x + a1, b.min.y + depth),
        ),
        Side::North => Aabb::new(
            Vec2::new(b.min.x + a0, b.max.y - depth),
            Vec2::new(b.min.x + a1, b.max.y),
        ),
        Side::West => Aabb::new(
            Vec2::new(b.min.x, b.min.y + a0),
            Vec2::new(b.min.x + depth, b.min.y + a1),
        ),
        Side::East => Aabb::new(
            Vec2::new(b.max.x - depth, b.min.y + a0),
            Vec2::new(b.max.x, b.min.y + a1),
        ),
    }
}

fn building(kind: BuildingKind, footprint: Aabb, height: f32, front: Side) -> Building {
    Building {
        kind,
        footprint,
        height,
        front,
    }
}

/// Proporciones de la huella de un punto de referencia sobre una manzana de
/// referencia de 40 m: inicio y fin a lo largo de su borde sobre la calle,
/// profundidad dentro de la manzana y altura de pared.
fn landmark_shape(kind: BuildingKind) -> (f32, f32, f32, f32) {
    match kind {
        BuildingKind::GeneralStore => (12.0, 28.0, 14.0, 5.0),
        BuildingKind::PoliceStation => (13.0, 27.0, 12.0, 4.5),
        // Iglesia: nave profunda detrás de un frente angosto.
        _ => (14.0, 26.0, 22.0, 7.0),
    }
}

/// Pequeño hash determinista de las coordenadas del lote en `[0, 1)`.
fn lot_noise(i: usize, j: usize, lot: usize, salt: u32) -> f32 {
    let mut v = (i as u32).wrapping_mul(73_856_093)
        ^ (j as u32).wrapping_mul(19_349_663)
        ^ (lot as u32).wrapping_mul(83_492_791)
        ^ salt.wrapping_mul(2_654_435_761);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v & 0xffff) as f32 / 65_536.0
}

/// Bordea con casas los bordes de una manzana sobre la calle (galpones en las
/// afueras) y agrega su punto de referencia o el molino del fondo, si hay. Cada
/// proporción está dibujada para una manzana de 40 m y se escala con la
/// manzana. Cuando una segunda plaza ocupa la mitad de la manzana, la otra
/// mitad recibe todos los lotes que quedan lejos de ella.
fn fill_block(
    grid: &Grid,
    (i, j): (usize, usize),
    landmark: Option<&Landmark>,
    windmill: bool,
    plaza: Option<Aabb>,
    config: &TownConfig,
    out: &mut Vec<Building>,
) {
    let seed = config.seed;
    let b = grid.block(i, j);
    let s = grid.scale();
    if let Some(l) = landmark {
        let (a0, a1, depth, height) = landmark_shape(l.kind);
        out.push(building(
            l.kind,
            side_box(b, l.front, a0 * s, a1 * s, depth * s),
            height,
            l.front,
        ));
    }
    let taken = landmark.map(|l| l.front);
    let outskirts = i == 0 || j == 0 || i == grid.cols - 1 || j == grid.rows - 1;
    let density = match (plaza, outskirts) {
        (Some(_), _) => 1.0,
        (None, true) => 0.45,
        (None, false) => 0.75,
    } * config.density.unwrap_or(1.0);
    let noise = |lot: usize, salt: u32| lot_noise(i, j, lot, salt ^ seed.wrapping_mul(0x9e37_79b9));

    // Lotes: (lado de la calle, inicio a lo largo de la calle, largo del lote).
    // Los lados norte y sur tienen tres lotes; el este y el oeste, uno en el
    // medio, lejos de las esquinas.
    let lots = [
        (Side::South, 1.0, 10.0),
        (Side::South, 15.0, 10.0),
        (Side::South, 29.0, 10.0),
        (Side::North, 1.0, 10.0),
        (Side::North, 15.0, 10.0),
        (Side::North, 29.0, 10.0),
        (Side::West, 14.0, 12.0),
        (Side::East, 14.0, 12.0),
    ];
    for (n, &(side, start, length)) in lots.iter().enumerate() {
        if Some(side) == taken || noise(n, 1) > density {
            continue;
        }
        let width = length - 2.0 * noise(n, 2);
        let depth = 8.0 + 3.0 * noise(n, 3);
        let a = start + (length - width) / 2.0;
        let footprint = side_box(b, side, a * s, (a + width) * s, depth * s);
        if plaza.is_some_and(|p| p.intersects(&footprint)) {
            continue;
        }
        let shed = outskirts && noise(n, 4) < 0.25;
        let (kind, height) = if shed {
            (BuildingKind::Shed, 4.0)
        } else {
            (BuildingKind::House, 3.6 + 0.8 * noise(n, 5))
        };
        out.push(building(kind, footprint, height, side));
    }

    if windmill {
        // Torre y tanque centrados en el fondo.
        let c = b.center();
        let tower = Aabb::new(
            Vec2::new(c.x - 5.5, c.y - 1.5),
            Vec2::new(c.x - 2.5, c.y + 1.5),
        );
        out.extend(windmill_with_tank(tower, Vec2::new(1.0, 0.0)));
    }
}

/// Una torre de molino de 3 m sobre `tower` y su tanque redondo de 7 m, 1 m más
/// allá en la dirección de `toward`.
fn windmill_with_tank(tower: Aabb, toward: Vec2) -> [Building; 2] {
    let reach = 1.5 + 1.0 + 3.5;
    let tank_center = tower.center() + toward * reach;
    [
        building(BuildingKind::Windmill, tower, 9.0, Side::South),
        building(
            BuildingKind::WaterTank,
            Aabb::from_center_size(tank_center, Vec2::new(7.0, 7.0)),
            1.5,
            Side::South,
        ),
    ]
}

/// Rectángulo al lado de la cuadrícula del lado `side`: `u` a lo largo del
/// borde de la cuadrícula (desde la línea central de la cuadrícula, de oeste a
/// este o de sur a norte) y `v` hacia afuera desde el borde de la cuadrícula.
fn yard_box(grid: Aabb, side: Side, u: (f32, f32), v: (f32, f32)) -> Aabb {
    let c = grid.center();
    let (min, max) = match side {
        Side::North => (
            Vec2::new(c.x + u.0, grid.max.y + v.0),
            Vec2::new(c.x + u.1, grid.max.y + v.1),
        ),
        Side::South => (
            Vec2::new(c.x + u.0, grid.min.y - v.1),
            Vec2::new(c.x + u.1, grid.min.y - v.0),
        ),
        Side::East => (
            Vec2::new(grid.max.x + v.0, c.y + u.0),
            Vec2::new(grid.max.x + v.1, c.y + u.1),
        ),
        Side::West => (
            Vec2::new(grid.min.x - v.1, c.y + u.0),
            Vec2::new(grid.min.x - v.0, c.y + u.1),
        ),
    };
    Aabb::new(min, max)
}

/// La mitad del largo del andén, en metros: más largo que el tren.
const PLATFORM_HALF: f32 = 30.0;

/// Estación, andén, depósitos de cereal, un galpón y un molino entre la calle
/// perimetral y la vía, anclados a la vía. La estación está centrada respecto
/// de la plaza a lo largo de la vía (lo bastante lejos de los extremos de la
/// cuadrícula para que entre todo el andén) y la mira del otro lado de la calle
/// perimetral; los depósitos de cereal bordean la vía de un lado, y el galpón y
/// el molino del otro, hasta donde llega la cuadrícula.
///
/// `bounds` es la parte de la cuadrícula contra la que se coloca la playa (toda
/// la cuadrícula, o la mitad de la estación de una línea que atraviesa el
/// pueblo), `side` el lado de ella por el que corre la vía a `offset` metros,
/// `half` la mitad de su largo a lo largo de la vía, y `wanted` dónde debería
/// estar la estación a lo largo de la vía (desde su centro).
fn station_yard(bounds: Aabb, side: Side, r: f32, half: f32, wanted: f32) -> Vec<Building> {
    let along = axis_along(side);
    let c = wanted.clamp(-half + PLATFORM_HALF, half - PLATFORM_HALF);
    let reach = half - 2.0;
    let yard =
        |u0: f32, u1: f32, v0: f32, v1: f32| yard_box(bounds, side, (c + u0, c + u1), (v0, v1));
    let (to_grid, to_track) = (side.opposite(), side);

    let mut out = vec![
        building(
            BuildingKind::Station,
            yard(-12.0, 12.0, r - 20.0, r - 11.0),
            5.0,
            to_grid,
        ),
        building(
            BuildingKind::Platform,
            yard(
                -PLATFORM_HALF,
                PLATFORM_HALF,
                r - 11.0,
                r - Floor::PLATFORM_EDGE,
            ),
            Floor::PLATFORM_HEIGHT,
            to_track,
        ),
    ];
    // Depósitos de cereal más allá del andén, hacia +u.
    let mut start = PLATFORM_HALF + 10.0;
    for length in [40.0, 35.0] {
        let end = (start + length).min(reach - c);
        if end - start < 20.0 {
            break;
        }
        out.push(building(
            BuildingKind::GrainStore,
            yard(start, end, r - 20.0, r - 8.0),
            6.0,
            to_track,
        ));
        start = end + 10.0;
    }
    // Un molino y un galpón antes del andén, hacia -u.
    if c - 45.0 >= -reach {
        let tower = yard(-45.0, -42.0, r - 17.0, r - 14.0);
        out.extend(windmill_with_tank(tower, along));
    }
    if c - 75.0 >= -reach {
        out.push(building(
            BuildingKind::Shed,
            yard(-75.0, -50.0, r - 20.0, r - 10.0),
            4.5,
            to_grid,
        ));
    }
    out
}

/// Eje unitario del mundo a lo largo de una vía del lado `side` de una
/// cuadrícula.
fn axis_along(side: Side) -> Vec2 {
    match side {
        Side::North | Side::South => Vec2::new(1.0, 0.0),
        Side::East | Side::West => Vec2::new(0.0, 1.0),
    }
}

/// A qué distancia se mantiene una calle norte-sur de la playa de la estación
/// para cruzar la franja de una línea que atraviesa el pueblo, en metros.
const YARD_CLEARANCE: f32 = 2.0;

/// La vía: paralela al lado `side` de `grid` a `offset`, extendiéndose
/// [`TownLayout::RAIL_RUN_OUT`] más allá de cada extremo de la cuadrícula.
fn railway_line(grid: Aabb, side: Side, offset: f32, along_half: f32) -> Railway {
    // Un rectángulo de tamaño cero sobre la línea central de la vía.
    let line = yard_box(grid, side, (0.0, 0.0), (offset, offset));
    let reach = along_half + TownLayout::RAIL_RUN_OUT;
    let c = grid.center();
    let (start, end) = match side {
        Side::North | Side::South => (
            Vec2::new(c.x - reach, line.min.y),
            Vec2::new(c.x + reach, line.min.y),
        ),
        Side::East | Side::West => (
            Vec2::new(line.min.x, c.y - reach),
            Vec2::new(line.min.x, c.y + reach),
        ),
    };
    Railway {
        start,
        end,
        gauge: RAIL_GAUGE,
    }
}

/// Profundidad de la galería del almacén de ramos generales: 3 m en las calles
/// anchas, menos en las angostas para que el caballo atado quede lejos de los
/// peatones que van por el medio.
fn veranda_depth(street: f32) -> f32 {
    (street / 2.0 - 2.7).clamp(1.2, Sidewalk::WIDTH)
}

/// Un palenque del lado de la calle de `store`, más allá de su `veranda`, con
/// un caballo atado al lado mirando a lo largo de la calle.
fn hitching_post(store: &Building, veranda: f32) -> HitchingPost {
    let (rail, horse) = tied_post(store, veranda + 0.2, 0.0, 2.8);
    HitchingPost {
        rail,
        horse,
        veranda,
    }
}

/// Un palenque de largo `length` a lo largo de la calle, a `rail_offset` de la
/// fachada de `building` y a `lateral` metros de su centro (positivo hacia la
/// izquierda de la normal hacia afuera), con un caballo atado al lado mirando a
/// lo largo de la calle.
fn tied_post(
    building: &Building,
    rail_offset: f32,
    lateral: f32,
    length: f32,
) -> (Aabb, TetheredHorse) {
    // Normal de la fachada hacia afuera y la dirección a lo largo de la calle.
    let normal = crate::places::side_normal(building.front);
    let along = Vec2::new(normal.y.abs(), normal.x.abs());
    let half = building.footprint.size() * 0.5;
    let face = building.footprint.center()
        + normal * (normal.x.abs() * half.x + normal.y.abs() * half.y)
        + Vec2::new(-normal.y, normal.x) * lateral;
    // Rectángulo alineado con los ejes de `length` a lo largo de la calle y
    // `width` a lo ancho.
    let boxed = |center: Vec2, length: f32, width: f32| {
        let size = along * length + normal * width;
        Aabb::from_center_size(center, Vec2::new(size.x.abs(), size.y.abs()))
    };
    // Justo más allá de la galería.
    let (rail_width, horse_width, horse_length) = (0.2, 1.0, 2.4);
    let rail = boxed(face + normal * rail_offset, length, rail_width);
    let horse_center = face + normal * (rail_offset + rail_width / 2.0 + 0.1 + horse_width / 2.0);
    (
        rail,
        TetheredHorse {
            position: horse_center,
            heading: Heading::from_radians(along.y.atan2(along.x)),
            footprint: boxed(horse_center, horse_length, horse_width),
        },
    )
}

impl TownLayout {
    /// Los palenques del pueblo: uno frente a cada almacén de ramos generales
    /// salvo el primero (cuyo palenque es [`Self::hitching_post`]), comisaría,
    /// iglesia y estación que tenga lugar para uno, en el orden de los
    /// edificios. Una ciudad ata un caballo a una mitad de ellos elegida por
    /// semilla, como mucho [`Palenque::MAX_HORSES`].
    fn place_palenques(&self) -> Vec<Palenque> {
        let first_store = self
            .buildings
            .iter()
            .position(|b| b.kind == BuildingKind::GeneralStore);
        // Terreno del que se mantiene lejos un palenque, con la separación que
        // necesita cada uno.
        let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
        let mut solid: Vec<Aabb> = self
            .buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(Building::collider)
            .chain(self.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)))
            .chain([self.hitching_post.rail, self.hitching_post.horse.footprint])
            .chain(self.sign_obstacles())
            .chain(
                self.plaza_furniture()
                    .iter()
                    .flat_map(PlazaFurniture::obstacles)
                    .collect::<Vec<_>>(),
            )
            .collect();
        let mut open: Vec<Aabb> = self.sidewalks.iter().map(|s| s.area).collect();
        open.extend(self.plazas());
        for floor in self.floors() {
            open.push(floor.footprint);
            open.extend(floor.ramps());
        }
        open.extend(self.canopies().iter().map(|c| c.footprint));
        open.extend(self.rail_corridor());
        // Los faroles de esquina están en los cruces de calles.
        open.extend(self.crossings(Palenque::CROSSING_MARGIN));
        // El terreno frente a cada puerta, tal como lo mantienen libre los
        // faroles.
        open.extend(self.door_zones(0.0));
        // Más allá de la vereda, y más allá de la galería del almacén si la
        // calle es ancha.
        let offset = (self.hitching_post.veranda + 0.2).max(Sidewalk::WIDTH + 0.2);
        let mut posts: Vec<Palenque> = Vec::new();
        for (k, b) in self.buildings.iter().enumerate() {
            let wanted = matches!(
                b.kind,
                BuildingKind::GeneralStore
                    | BuildingKind::PoliceStation
                    | BuildingKind::Church
                    | BuildingKind::Station
            );
            if !wanted || Some(k) == first_store {
                continue;
            }
            // Al lado de la puerta, primero del lado elegido por semilla,
            // después más lejos.
            let first = if Rng::new(self.seed ^ 0x9a1e, k as u32).below(2) == 0 {
                1.0
            } else {
                -1.0
            };
            let laterals: Vec<f32> = (0..4)
                .flat_map(|step| {
                    let d = DOOR_SIDE + 1.5 * step as f32;
                    [first * d, -first * d]
                })
                .collect();
            // Una ciudad busca primero lugar para un caballo; en cualquier otro
            // lugar, o cuando la calle es demasiado angosta, alcanza con el
            // palenque solo.
            let passes: &[bool] = if self.size == TownSize::Small {
                &[false]
            } else {
                &[true, false]
            };
            'found: for &with_horse in passes {
                for &lateral in &laterals {
                    // El palenque de la estación está sobre su playa, sin vereda que
                    // despejar.
                    let out = if b.kind == BuildingKind::Station {
                        Palenque::YARD_OUT
                    } else {
                        offset
                    };
                    let (rail, horse) = tied_post(b, out, lateral, Palenque::LENGTH);
                    let spot = if with_horse {
                        Aabb::new(
                            Vec2::new(
                                rail.min.x.min(horse.footprint.min.x),
                                rail.min.y.min(horse.footprint.min.y),
                            ),
                            Vec2::new(
                                rail.max.x.max(horse.footprint.max.x),
                                rail.max.y.max(horse.footprint.max.y),
                            ),
                        )
                    } else {
                        rail
                    };
                    if self.palenque_fits(b.kind, &spot, &solid, &open) {
                        solid.push(spot);
                        posts.push(Palenque {
                            building: k,
                            rail,
                            horse: with_horse.then_some(horse),
                        });
                        break 'found;
                    }
                }
            }
        }
        // Una ciudad ata un caballo a una mitad de sus palenques elegida por
        // semilla (los de valores aleatorios más bajos que tengan lugar para
        // uno).
        let mut order: Vec<usize> = (0..posts.len())
            .filter(|&k| posts[k].horse.is_some())
            .collect();
        order.sort_by_key(|&k| Rng::new(self.seed ^ 0x7e11, k as u32).next());
        order.truncate(Palenque::horses_for(posts.len()));
        for (k, post) in posts.iter_mut().enumerate() {
            if !order.contains(&k) {
                post.horse = None;
            }
        }
        posts
    }

    /// Si entra un palenque de un edificio de tipo `kind` que ocupe `spot`:
    /// sobre una calle y fuera de su carril central (el de la estación, sobre
    /// la playa que tiene enfrente), a `Palenque::CLEARANCE` de todo obstáculo
    /// `solid` y lejos de toda área `open`.
    fn palenque_fits(
        &self,
        kind: BuildingKind,
        spot: &Aabb,
        solid: &[Aabb],
        open: &[Aabb],
    ) -> bool {
        let on_ground = if kind == BuildingKind::Station {
            self.extent.contains(spot)
        } else {
            self.on_street_off_lane(spot, Palenque::LANE)
        };
        // La estación está en la franja de una línea que atraviesa el pueblo;
        // nada más lo está.
        let off_band =
            kind == BuildingKind::Station || self.band.is_none_or(|b| !b.intersects(spot));
        on_ground
            && off_band
            && solid
                .iter()
                .all(|o| !o.intersects(&spot.expanded(Palenque::CLEARANCE)))
            && open.iter().all(|o| !o.intersects(spot))
    }
}

impl TownLayout {
    /// Si `spot` está dentro de una franja de calle y al menos a `lane` metros
    /// de su línea media, de un lado de ella.
    fn on_street_off_lane(&self, spot: &Aabb, lane: f32) -> bool {
        self.streets.iter().any(|street| {
            let across = if street.size().x > street.size().y {
                Vec2::new(0.0, 1.0)
            } else {
                Vec2::new(1.0, 0.0)
            };
            let middle = street.center().dot(across);
            let (lo, hi) = (spot.min.dot(across) - middle, spot.max.dot(across) - middle);
            street.contains(spot) && lo * hi > 0.0 && lo.abs().min(hi.abs()) >= lane
        })
    }

    /// Los cruces de calles, donde una calle norte-sur se encuentra con una
    /// este-oeste, cada uno agrandado `margin` metros.
    fn crossings(&self, margin: f32) -> Vec<Aabb> {
        let (north_south, east_west): (Vec<&Aabb>, Vec<&Aabb>) =
            self.streets.iter().partition(|s| s.size().y > s.size().x);
        let mut out = Vec::new();
        for x in &north_south {
            for y in &east_west {
                if x.intersects(y) {
                    let crossing =
                        Aabb::new(Vec2::new(x.min.x, y.min.y), Vec2::new(x.max.x, y.max.y));
                    out.push(crossing.expanded(margin));
                }
            }
        }
        out
    }

    /// El terreno frente a cada puerta (salvo la del andén), tal como lo
    /// mantienen libre los faroles, cada franja `extra` metros más ancha a
    /// ambos lados.
    fn door_zones(&self, extra: f32) -> Vec<Aabb> {
        let reach = StreetLamps::DOOR_HALF_WIDTH + extra;
        self.buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(|b| {
                let n = crate::places::side_normal(b.front);
                let side = Vec2::new(-n.y, n.x);
                let half = b.footprint.size() * 0.5;
                let facade = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                let (a, z) = (
                    facade + side * reach,
                    facade + n * StreetLamps::DOOR_REACH - side * reach,
                );
                Aabb::new(
                    Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                    Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                )
            })
            .collect()
    }
}

/// Aquello de lo que se mantienen lejos los árboles de
/// [`TownLayout::place_shade_trees`], y dónde están hasta ahora.
struct ShadeGround {
    /// Obstáculos de los que un tronco se mantiene a
    /// [`ShadeGround::CLEARANCE`].
    solid: Vec<Aabb>,
    /// Terreno donde no puede haber ningún tronco: cruces, puertas, galerías,
    /// la línea.
    zones: Vec<Aabb>,
    /// El terreno frente a cada puerta, más ancho: ningún árbol de calle está
    /// sobre él.
    pit_doors: Vec<Aabb>,
    /// El anillo alrededor de cada plaza y su vereda: ningún tronco salvo el de
    /// un bosquecillo.
    plazas: Vec<Aabb>,
    /// Todas las huellas de los edificios (excluida la del andén).
    buildings: Vec<Aabb>,
    /// Todos los troncos colocados hasta ahora, incluidos los de los anillos de
    /// las plazas.
    taken: Vec<Vec2>,
}

impl ShadeGround {
    /// Separación entre un tronco y cualquier obstáculo, en metros.
    const CLEARANCE: f32 = 1.0;
    /// Separación que se mantiene respecto de un cruce, donde están los faroles
    /// de esquina, en metros.
    const CROSSING: f32 = 3.5;
    /// Ancho extra que se deja al lado de una puerta, en metros.
    const DOOR: f32 = 1.8;
    /// Ancho extra que deja al lado de una puerta un árbol de calle, cuya copa
    /// es más ancha, en metros.
    const PIT_DOOR: f32 = 2.4;
    /// Profundidad de la zona de la galería de un almacén, en metros: la vereda
    /// y su galería.
    const GALLERY: f32 = 3.0;
    /// Ancho extra que se deja al lado de la galería de un almacén, en metros.
    const GALLERY_SIDE: f32 = 2.5;
    /// Separación que se mantiene entre una planta de patio y cualquier
    /// obstáculo, en metros.
    const PLANT_GAP: f32 = 0.4;

    fn new(town: &TownLayout) -> Self {
        let mut zones = town.crossings(Self::CROSSING);
        zones.extend(town.door_zones(Self::DOOR));
        for b in town
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::GeneralStore)
        {
            let n = crate::places::side_normal(b.front);
            let side = Vec2::new(-n.y, n.x);
            let half = b.footprint.size() * 0.5;
            let facade = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
            let wide = side.x.abs() * half.x + side.y.abs() * half.y + Self::GALLERY_SIDE;
            let (a, z) = (
                facade + side * wide,
                facade + n * Self::GALLERY - side * wide,
            );
            zones.push(Aabb::new(
                Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                Vec2::new(a.x.max(z.x), a.y.max(z.y)),
            ));
        }
        for floor in town.floors() {
            zones.push(floor.footprint.expanded(Self::CLEARANCE));
            zones.extend(floor.ramps().map(|r| r.expanded(Self::CLEARANCE)));
        }
        zones.extend(
            town.canopies()
                .iter()
                .map(|c| c.footprint.expanded(Self::CLEARANCE)),
        );
        zones.extend(town.rail_corridor().map(|c| c.expanded(Self::CLEARANCE)));
        zones.extend(town.rail_band().map(|b| b.expanded(Self::CLEARANCE)));
        Self {
            solid: town.obstacles(),
            zones,
            pit_doors: town.door_zones(Self::PIT_DOOR),
            plazas: town
                .plazas()
                .map(|p| p.expanded(Sidewalk::WIDTH + 0.3))
                .collect(),
            buildings: town
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect(),
            taken: town.trees.clone(),
        }
    }

    /// Si un tronco puede estar en `p`: lejos de los obstáculos y de las zonas
    /// (y de las plazas salvo que `in_plaza`), y a [`ShadeTree::APART`] de
    /// todos los demás troncos.
    fn fits(&self, p: Vec2, in_plaza: bool) -> bool {
        let trunk = ShadeTree::trunk(p);
        let spot = trunk.expanded(Self::CLEARANCE);
        self.solid.iter().all(|o| !o.intersects(&spot))
            && self.zones.iter().all(|z| !z.intersects(&trunk))
            && (in_plaza || self.plazas.iter().all(|z| !z.intersects(&trunk)))
            && self.taken.iter().all(|t| t.distance(p) >= ShadeTree::APART)
    }

    /// La distancia desde `p` hasta el edificio más cercano.
    fn building_gap(&self, p: Vec2) -> f32 {
        self.buildings
            .iter()
            .map(|b| b.closest_point(p).distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    fn plant(&mut self, out: &mut Vec<ShadeTree>, tree: ShadeTree) {
        self.taken.push(tree.at);
        self.solid.push(tree.obstacle());
        out.push(tree);
    }
}

impl TownLayout {
    /// Los árboles fuera de los anillos de las plazas, deterministas a partir
    /// de la semilla del pueblo:
    ///
    /// - los árboles de vereda de una ciudad en sus cazuelas, una copa podada
    ///   cada 8 a 10 m, a 0,4 m del cordón hacia adentro, lejos de las puertas,
    ///   galerías, cruces, palenques y carteles ([`ShadeKind::Pit`]);
    /// - bosquecillos entre los senderos de las plazas de una ciudad
    ///   ([`ShadeKind::Grove`]);
    /// - los árboles de patio de una ciudad y los que están junto a su iglesia,
    ///   sus comisarías y su estación, y los escasos de un pueblo detrás de sus
    ///   casas ([`ShadeKind::Yard`], nunca al alcance de una copa de un
    ///   edificio);
    /// - los escasos árboles de un pueblo en la tierra frente a algunas casas
    ///   ([`ShadeKind::Front`]).
    ///
    /// Cada tronco se mantiene a [`ShadeTree::APART`] de los demás.
    fn place_shade_trees(&self, grid: &Grid, main: PlazaBlocks) -> Vec<ShadeTree> {
        let city = self.size != TownSize::Small;
        let mut ground = ShadeGround::new(self);
        let mut out: Vec<ShadeTree> = Vec::new();
        let trunk = ShadeTree::trunk;

        // Árboles de vereda, a lo largo del cordón de cada franja.
        if city {
            let inset = ShadeTree::PIT_FROM_CURB + ShadeTree::PIT_SIDE / 2.0;
            for (row, walk) in self.sidewalks.iter().enumerate() {
                let Some(&curb) = walk.curbs.first() else {
                    continue;
                };
                let out_of = crate::places::side_normal(curb);
                let a = walk.area;
                let level = out_of.x.abs() < 0.5;
                let (lo, hi) = if level {
                    (a.min.x, a.max.x)
                } else {
                    (a.min.y, a.max.y)
                };
                let edge = match curb {
                    Side::North => a.max.y,
                    Side::South => a.min.y,
                    Side::East => a.max.x,
                    Side::West => a.min.x,
                };
                let at = |t: f32| {
                    if level {
                        Vec2::new(t, edge - out_of.y * inset)
                    } else {
                        Vec2::new(edge - out_of.x * inset, t)
                    }
                };
                let mut rng = Rng::new(self.seed ^ 0x57ee, row as u32);
                let (near, far) = ShadeTree::SPACING;
                let mut t = lo + rng.range(0.0, far);
                let mut last = f32::NEG_INFINITY;
                while t < hi {
                    // El lugar elegido por semilla, o uno un poco más lejos o
                    // más cerca (nunca más cerca que la separación mínima
                    // respecto del último árbol).
                    let found = [0.0, 1.5, -1.5, 3.0, -3.0]
                        .iter()
                        .map(|d| t + d)
                        .find(|&t| {
                            (lo..=hi).contains(&t)
                                && t - last >= near
                                && ground.fits(at(t), false)
                                && ground
                                    .pit_doors
                                    .iter()
                                    .all(|d| !d.intersects(&ShadeTree::trunk(at(t))))
                        });
                    if let Some(found) = found {
                        t = found;
                        last = t;
                        let tree = ShadeTree {
                            at: at(t),
                            kind: ShadeKind::Pit,
                            crown: ShadeTree::PIT_CROWN,
                            toward: out_of,
                            row: row as u32,
                        };
                        ground.plant(&mut out, tree);
                    }
                    t += rng.range(near, far);
                }
            }
        }

        // Bosquecillos en el verde entre los senderos de las plazas de una
        // ciudad.
        if city {
            let reach = TREE_TRUNK_RADIUS + PLAZA_PATH_CLEARANCE + 0.5;
            for (k, plaza) in self.plazas().enumerate() {
                let keep_off: Vec<Aabb> = plaza_paths(plaza)
                    .into_iter()
                    .chain([plaza_monument(plaza), plaza_crossing(plaza)])
                    .collect();
                let bed = plaza.expanded(-ShadeTree::GROVE_INSET);
                let mut rng = Rng::new(self.seed ^ 0x6a0e, k as u32);
                let cols = (bed.size().x / ShadeTree::GROVE_STEP).floor() as usize + 1;
                let rows = (bed.size().y / ShadeTree::GROVE_STEP).floor() as usize + 1;
                for j in 0..rows {
                    for i in 0..cols {
                        let jitter = Vec2::new(
                            rng.range(-ShadeTree::GROVE_JITTER, ShadeTree::GROVE_JITTER),
                            rng.range(-ShadeTree::GROVE_JITTER, ShadeTree::GROVE_JITTER),
                        );
                        let p = bed.min
                            + Vec2::new(i as f32, j as f32) * ShadeTree::GROVE_STEP
                            + jitter;
                        let clear = keep_off
                            .iter()
                            .all(|a| a.closest_point(p).distance(p) >= reach);
                        if bed.contains_point(p) && clear && ground.fits(p, true) {
                            let tree = ShadeTree {
                                at: p,
                                kind: ShadeKind::Grove,
                                crown: ShadeTree::GROVE_CROWN,
                                toward: Vec2::ZERO,
                                row: 0,
                            };
                            ground.plant(&mut out, tree);
                        }
                    }
                }
            }
        }

        // Un árbol de patio en el primero de unos pocos lugares elegidos por
        // semilla en `area` que esté a una distancia de copa de todo edificio y
        // fuera de las calles.
        let yard = |ground: &mut ShadeGround,
                    out: &mut Vec<ShadeTree>,
                    area: Aabb,
                    stream: u32,
                    tries: u32| {
            let mut rng = Rng::new(self.seed ^ 0x7a2d, stream);
            for _ in 0..tries {
                let p = Vec2::new(
                    rng.range(area.min.x, area.max.x),
                    rng.range(area.min.y, area.max.y),
                );
                let off_streets = self
                    .streets
                    .iter()
                    .all(|s| !s.intersects(&trunk(p).expanded(0.5)));
                let reach = ShadeTree::YARD_CROWN + ShadeTree::BUILDING_GAP;
                if off_streets && ground.building_gap(p) >= reach && ground.fits(p, false) {
                    let tree = ShadeTree {
                        at: p,
                        kind: ShadeKind::Yard,
                        crown: ShadeTree::YARD_CROWN,
                        toward: Vec2::ZERO,
                        row: 0,
                    };
                    ground.plant(out, tree);
                    return;
                }
            }
        };

        // Junto a la iglesia, las comisarías y la estación de una ciudad.
        if city {
            for (k, b) in self.buildings.iter().enumerate() {
                let trees = match b.kind {
                    BuildingKind::Church | BuildingKind::Station => 2,
                    BuildingKind::PoliceStation => 1,
                    _ => continue,
                };
                let around = b.footprint.expanded(ShadeTree::LANDMARK_REACH);
                for n in 0..trees {
                    yard(&mut ground, &mut out, around, 0x1000 + 4 * k as u32 + n, 24);
                }
            }
        }

        // Detrás de las casas: el patio de cada manzana, el de una ciudad con
        // unos pocos árboles, el de un pueblo escaso.
        for (i, j) in (0..grid.rows).flat_map(|j| (0..grid.cols).map(move |i| (i, j))) {
            let block = grid.block(i, j);
            if main.contains((i, j)) || self.plazas().any(|p| p.intersects(&block)) {
                continue;
            }
            let stream = (j * grid.cols + i) as u32;
            let mut rng = Rng::new(self.seed ^ 0x3d1a, stream);
            // Un 30 % más que la primera plantación y después un 15 % más
            // (usuario, 2026-10-27): las manzanas de una ciudad tienen hasta
            // tres árboles, un cuarto en siete de cada diez y un quinto en dos
            // de cada diez; las de un pueblo, uno en nueve de cada diez.
            let wanted = if city {
                1 + rng.below(3)
                    + usize::from(rng.below(100) < 70)
                    + usize::from(rng.below(100) < 40)
            } else {
                usize::from(rng.below(100) < 92)
            };
            let area = block.expanded(-ShadeTree::YARD_INSET);
            for n in 0..wanted {
                yard(
                    &mut ground,
                    &mut out,
                    area,
                    0x4000 + 4 * stream + n as u32,
                    8,
                );
            }
        }

        // En la tierra frente a algunas casas de un pueblo.
        if !city {
            for (k, b) in self
                .buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == BuildingKind::House)
            {
                let mut rng = Rng::new(self.seed ^ 0xf20e, k as u32);
                if rng.below(100) >= 63 {
                    continue;
                }
                let n = crate::places::side_normal(b.front);
                let side = Vec2::new(-n.y, n.x);
                let half = b.footprint.size() * 0.5;
                let facade = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                let wide = side.x.abs() * half.x + side.y.abs() * half.y;
                let first = if rng.below(2) == 0 { 1.0 } else { -1.0 };
                let lateral = rng.range(2.6, wide + 1.5);
                let spot = [first, -first].iter().find_map(|d| {
                    let p = facade + n * ShadeTree::FRONT_OUT + side * (d * lateral);
                    let lane = Palenque::LANE + TREE_TRUNK_RADIUS;
                    let on_dirt = self.on_street_off_lane(&trunk(p), lane);
                    // Nunca una fila: con este, ningún árbol tiene más de otros
                    // dos dentro de FRONT_RUN.
                    let fronts = || out.iter().filter(|t| t.kind == ShadeKind::Front);
                    let within = |t: &ShadeTree, q: Vec2| t.at.distance(q) < ShadeTree::FRONT_RUN;
                    let sparse = fronts().all(|t| t.at.distance(p) >= ShadeTree::FRONT_APART)
                        && fronts().filter(|t| within(t, p)).count() < 2
                        && fronts()
                            .filter(|t| within(t, p))
                            .all(|t| fronts().filter(|u| within(u, t.at)).count() <= 2);
                    (on_dirt && sparse && ground.fits(p, false)).then_some(p)
                });
                if let Some(p) = spot {
                    let tree = ShadeTree {
                        at: p,
                        kind: ShadeKind::Front,
                        crown: ShadeTree::FRONT_CROWN,
                        toward: n,
                        row: 0,
                    };
                    ground.plant(&mut out, tree);
                }
            }
        }
        // La plaza de General Pico tenía demasiado follaje (usuario,
        // 2026-10-26, dos veces): un pueblo muy grande pierde una quinta parte
        // de sus árboles de plaza y después otra quinta parte de lo que quedó
        // (36 de cada 100 en total), tomados de los bosquecillos con valores
        // aleatorios con semilla para que se conserven los anillos a lo largo
        // de los bordes.
        if self.size == TownSize::VeryBig {
            let groves: Vec<usize> = (0..out.len())
                .filter(|&k| out[k].kind == ShadeKind::Grove)
                .collect();
            let cut = (self.trees.len() + groves.len()) * 36 / 100;
            let mut order = groves;
            order.sort_by_key(|&k| Rng::new(self.seed ^ 0x71a5, k as u32).next());
            let gone: Vec<usize> = order.into_iter().take(cut).collect();
            let mut k = 0;
            out.retain(|_| {
                k += 1;
                !gone.contains(&(k - 1))
            });
        }
        out
    }
}

impl TownLayout {
    /// Los arbustos y los canteros de flores de los patios: detrás de cada casa
    /// (un galpón no tiene), contra sus paredes trasera y laterales, con
    /// semilla. Cada uno queda lejos de todo obstáculo (también de los
    /// troncos), de las zonas de las puertas y las galerías, de las calles y
    /// las plazas, y de todas las demás plantas.
    fn place_yard_plants(&self) -> Vec<YardPlant> {
        let ground = ShadeGround::new(self);
        let mut out: Vec<YardPlant> = Vec::new();
        for (k, b) in self
            .buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == BuildingKind::House)
        {
            let n = crate::places::side_normal(b.front);
            let side = Vec2::new(-n.y, n.x);
            let half = b.footprint.size() * 0.5;
            let deep = n.x.abs() * half.x + n.y.abs() * half.y;
            let wide = side.x.abs() * half.x + side.y.abs() * half.y;
            let mut rng = Rng::new(self.seed ^ 0x91a7, k as u32);
            let mut wanted = Vec::new();
            if rng.below(100) < 40 {
                wanted.push(PlantKind::Shrub);
            }
            if rng.below(100) < 15 {
                wanted.push(PlantKind::Shrub);
            }
            if rng.below(100) < 25 {
                wanted.push(PlantKind::Flowers);
            }
            for kind in wanted {
                for _ in 0..8 {
                    // Detrás de la casa, o a su lado hacia el fondo.
                    let along = rng.range(-wide, wide);
                    let out_of = rng.range(1.0, 2.6);
                    let p = b.footprint.center() - n * (deep + out_of) + side * along;
                    let plant = YardPlant { at: p, kind };
                    let spot = plant.area();
                    let free = ground
                        .solid
                        .iter()
                        .all(|o| !o.intersects(&spot.expanded(ShadeGround::PLANT_GAP)))
                        && ground.zones.iter().all(|z| !z.intersects(&spot))
                        && ground.plazas.iter().all(|z| !z.intersects(&spot))
                        && self
                            .streets
                            .iter()
                            .all(|s| !s.intersects(&spot.expanded(0.5)))
                        && out.iter().all(|q| q.at.distance(p) >= 1.6);
                    if free {
                        out.push(plant);
                        break;
                    }
                }
            }
        }
        out
    }
}

/// A qué distancia del centro de una fachada está el centro del palenque más
/// cercano: más allá de la zona prohibida de la puerta y de la mitad del
/// palenque, en metros.
const DOOR_SIDE: f32 = StreetLamps::DOOR_HALF_WIDTH + 0.3 + Palenque::LENGTH / 2.0;

/// Árboles en un anillo justo dentro del borde de la plaza, dejando el centro
/// libre: `steps.0` huecos a lo largo de los bordes x y `steps.1` a lo largo de
/// los bordes y. Solo se plantan los canteros verdes: un lugar del anillo sobre
/// un sendero, o a [`PLAZA_PATH_CLEARANCE`] o menos de uno o del monumento,
/// queda vacío.
fn plaza_trees(plaza: Aabb, steps: (usize, usize)) -> Vec<Vec2> {
    let reach = TREE_TRUNK_RADIUS + PLAZA_PATH_CLEARANCE;
    let keep_off: Vec<Aabb> = plaza_paths(plaza)
        .into_iter()
        .chain([plaza_monument(plaza)])
        .collect();
    let mut trees = plaza_ring(plaza, steps);
    trees.retain(|&t| {
        keep_off
            .iter()
            .all(|a| a.closest_point(t).distance(t) >= reach)
    });
    trees
}

/// La cantidad de pasos del anillo de una plaza con unos lugares más (usuario,
/// 2026-10-27: "subile otro 15% a pueblos y aldeas"); los pasos son enteros,
/// así que el anillo crece menos que el resto.
fn more_steps(steps: usize) -> usize {
    (steps as f32 * PLAZA_RING_MORE).round() as usize
}

/// Cuántas veces los pasos de la primera plantación toma el anillo de una
/// plaza.
const PLAZA_RING_MORE: f32 = 1.08;

/// Lugares en un anillo justo dentro del borde de la plaza (ver
/// [`plaza_trees`]).
fn plaza_ring(plaza: Aabb, steps: (usize, usize)) -> Vec<Vec2> {
    let inset = 3.0;
    let (lo, hi) = (
        plaza.min + Vec2::new(inset, inset),
        plaza.max - Vec2::new(inset, inset),
    );
    let mut trees = Vec::new();
    for k in 0..=steps.0.max(steps.1) {
        if k <= steps.0 {
            let x = lo.x + (hi.x - lo.x) * (k as f32 / steps.0 as f32);
            trees.push(Vec2::new(x, lo.y));
            trees.push(Vec2::new(x, hi.y));
        }
        if k != 0 && k < steps.1 {
            let y = lo.y + (hi.y - lo.y) * (k as f32 / steps.1 as f32);
            trees.push(Vec2::new(lo.x, y));
            trees.push(Vec2::new(hi.x, y));
        }
    }
    trees
}

/// El camino principal de entrada: la calle norte-sur justo al este de la
/// plaza, a la que se entra desde el sur (desde el norte cuando la vía férrea
/// pasa al sur del pueblo, o cuando la estación de una línea que atraviesa el
/// pueblo está en la mitad norte).
fn arrival(grid: &Grid, plaza: PlazaBlocks, railway: Option<RailwayConfig>) -> (Vec2, Heading) {
    let bounds = grid.bounds();
    let road_x = grid.lane_x(plaza.max.0 + 1) + grid.street / 2.0;
    if railway.is_some_and(|r| (r.side == Side::South) != r.through) {
        (
            Vec2::new(road_x, bounds.max.y + ARRIVAL_DISTANCE),
            Heading::from_radians(-std::f32::consts::FRAC_PI_2),
        )
    } else {
        (
            Vec2::new(road_x, bounds.min.y - ARRIVAL_DISTANCE),
            Heading::from_radians(std::f32::consts::FRAC_PI_2),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Rider;

    /// Configuraciones contra las que se comprueba cada invariante: cuadrículas
    /// cuadradas y alargadas, cantidades de manzanas pares e impares, manzanas
    /// de 32–50 m, calles de 8–14 m, la vía férrea de cada lado o ausente, cada
    /// ubicación de plaza, y pueblos grandes y muy grandes.
    pub(crate) fn sample_configs() -> Vec<TownConfig> {
        let base = TownConfig::castex();
        let rail = |side, offset| {
            Some(RailwayConfig {
                side,
                offset,
                through: false,
            })
        };
        vec![
            base.clone(),
            five_by_five(),
            TownConfig {
                name: "Tres".into(),
                blocks: (3, 3),
                block_size: 32.0,
                street_width: 12.0,
                seed: 1,
                ..base.clone()
            },
            TownConfig {
                name: "Seis por cuatro".into(),
                origin: Vec2::new(-20.0, 15.0),
                blocks: (6, 4),
                block_size: 50.0,
                street_width: 16.0,
                railway: rail(Side::South, 30.0),
                seed: 99,
                ..base.clone()
            },
            TownConfig {
                name: "Este".into(),
                blocks: (4, 4),
                block_size: 36.0,
                street_width: 12.0,
                railway: rail(Side::East, 26.0),
                ..base.clone()
            },
            TownConfig {
                name: "Oeste".into(),
                blocks: (3, 5),
                block_size: 45.0,
                street_width: 13.0,
                railway: rail(Side::West, 24.0),
                plaza: PlazaPlacement::Block(0, 0),
                ..base.clone()
            },
            TownConfig {
                name: "Sin tren".into(),
                blocks: (5, 3),
                railway: None,
                ..base.clone()
            },
            TownConfig {
                name: "Sin tren par".into(),
                blocks: (4, 6),
                block_size: 34.0,
                railway: None,
                ..base.clone()
            },
            TownConfig {
                name: "Frente".into(),
                blocks: (5, 5),
                block_size: 38.0,
                street_width: 12.0,
                plaza: PlazaPlacement::FacingStation,
                ..base.clone()
            },
            // Pueblos grandes (de 37 a 63 manzanas): una plaza de dos manzanas
            // y una plaza de la estación.
            TownConfig {
                name: "Siete".into(),
                blocks: (7, 7),
                block_size: 36.0,
                street_width: 12.0,
                railway: rail(Side::East, 26.0),
                seed: 5,
                ..base.clone()
            },
            TownConfig {
                name: "Larga".into(),
                blocks: (10, 4),
                block_size: 34.0,
                street_width: 12.0,
                railway: rail(Side::South, 24.0),
                seed: 21,
                ..base.clone()
            },
            TownConfig {
                name: "Alta".into(),
                blocks: (4, 10),
                block_size: 36.0,
                street_width: 12.0,
                railway: rail(Side::West, 24.0),
                seed: 8,
                ..base.clone()
            },
            TownConfig {
                name: "Grande sin tren".into(),
                blocks: (7, 6),
                railway: None,
                seed: 3,
                ..base.clone()
            },
            // Pueblos muy grandes (64 manzanas o más): una plaza de 2 x 2.
            TownConfig {
                name: "Muy grande".into(),
                blocks: (8, 8),
                seed: 11,
                ..base.clone()
            },
            TownConfig {
                name: "Muy grande sin tren".into(),
                blocks: (9, 8),
                block_size: 36.0,
                railway: None,
                seed: 4,
                ..base.clone()
            },
            // Pueblos que crecieron a ambos lados de una línea que los
            // atraviesa por el medio.
            TownConfig {
                name: "Partida".into(),
                blocks: (8, 8),
                railway: Some(RailwayConfig {
                    side: Side::North,
                    offset: 24.0,
                    through: true,
                }),
                seed: 13,
                ..base.clone()
            },
            TownConfig {
                name: "Partida sur".into(),
                blocks: (5, 4),
                block_size: 34.0,
                street_width: 12.0,
                railway: Some(RailwayConfig {
                    side: Side::South,
                    offset: 28.0,
                    through: true,
                }),
                seed: 21,
                density: Some(0.5),
                ..base
            },
        ]
    }

    /// Una cuadrícula de 5 x 5 manzanas de 40 m y calles de 12 m con una plaza
    /// central.
    fn five_by_five() -> TownConfig {
        TownConfig {
            name: "Cinco por cinco".into(),
            blocks: (5, 5),
            street_width: 14.0,
            ..TownConfig::castex()
        }
    }

    /// Todos los pueblos de ejemplo, generados.
    pub(crate) fn sample_towns() -> Vec<TownLayout> {
        sample_configs()
            .iter()
            .map(|c| TownLayout::generate(c).unwrap_or_else(|e| panic!("{}: {e}", c.name)))
            .collect()
    }

    fn count(t: &TownLayout, kind: BuildingKind) -> usize {
        t.buildings.iter().filter(|b| b.kind == kind).count()
    }

    fn find(t: &TownLayout, kind: BuildingKind) -> Option<&Building> {
        t.buildings.iter().find(|b| b.kind == kind)
    }

    /// Vector unitario que apunta hacia afuera de `side`.
    fn normal(side: Side) -> Vec2 {
        crate::places::side_normal(side)
    }

    #[test]
    fn generation_is_deterministic_and_the_seed_varies_lots() {
        for c in sample_configs() {
            assert_eq!(TownLayout::generate(&c), TownLayout::generate(&c));
            let other = TownConfig {
                seed: c.seed.wrapping_add(1),
                ..c.clone()
            };
            let (a, b) = (
                TownLayout::generate(&c).unwrap(),
                TownLayout::generate(&other).unwrap(),
            );
            assert_ne!(a.buildings, b.buildings, "{}", c.name);
            assert_eq!((a.streets, a.plaza), (b.streets, b.plaza));
        }
        assert_eq!(
            TownLayout::castex_inspired(),
            TownLayout::generate(&TownConfig::castex()).unwrap()
        );
    }

    #[test]
    fn has_the_institutions_of_a_railway_town() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let railway = usize::from(c.railway.is_some());
            let (stores, police) = match c.size() {
                TownSize::Small => (1, 1),
                TownSize::Big => (2, 1),
                TownSize::VeryBig => (2, 2),
            };
            assert_eq!(count(&t, BuildingKind::GeneralStore), stores, "{}", c.name);
            assert_eq!(count(&t, BuildingKind::PoliceStation), police, "{}", c.name);
            assert_eq!(count(&t, BuildingKind::Church), 1, "{}", c.name);
            for kind in [BuildingKind::Station, BuildingKind::Platform] {
                assert_eq!(count(&t, kind), railway, "{}: {kind:?}", c.name);
            }
            let blocks = c.blocks.0 * c.blocks.1;
            assert!(count(&t, BuildingKind::House) >= blocks, "{}", c.name);
            assert!(count(&t, BuildingKind::Windmill) >= 1, "{}", c.name);
            assert_eq!(
                count(&t, BuildingKind::Windmill),
                count(&t, BuildingKind::WaterTank)
            );
            assert!(t.trees.len() >= 8);
            assert_eq!(t.name, c.name);
        }
        let castex = TownLayout::castex_inspired();
        assert!(count(&castex, BuildingKind::House) >= 30);
        assert!(count(&castex, BuildingKind::Shed) + count(&castex, BuildingKind::GrainStore) >= 3);
    }

    #[test]
    fn the_grid_follows_the_config() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let (cols, rows) = c.blocks;
            let (b, w) = (c.block_size, c.street_width);
            // Una calle entre las manzanas de una plaza fusionada se corta en
            // dos.
            let (pw, ph) = t.size.plaza_span(c.blocks);
            let cut = (pw - 1) + (ph - 1);
            // Una línea que atraviesa el pueblo agrega una calle perimetral a
            // cada lado de su franja, y corta las calles que llegarían a su
            // playa.
            let (split, band) = match c.railway.filter(|r| r.through) {
                Some(r) => (
                    1 + (cols + 1 - t.crossing_streets().len()),
                    w + r.offset + RailwayConfig::THROUGH_FAR_SIDE,
                ),
                None => (0, 0.0),
            };
            assert_eq!(
                t.streets.len(),
                cols + 1 + rows + 1 + cut + split,
                "{}",
                c.name
            );
            let size = t.grid().size();
            let expected = Vec2::new(
                cols as f32 * b + (cols + 1) as f32 * w,
                rows as f32 * b + (rows + 1) as f32 * w + band,
            );
            assert!(size.distance(expected) < 1e-3, "{}: {size:?}", c.name);
            assert!(t.grid().center().distance(c.origin) < 1e-3, "{}", c.name);
            // Manzanas enteras, con las calles entre ellas.
            let merged = |n: usize| n as f32 * b + (n - 1) as f32 * w;
            let plaza = Vec2::new(merged(pw), merged(ph));
            assert!(t.plaza.size().distance(plaza) < 1e-3, "{}", c.name);
            for street in &t.streets {
                let size = street.size();
                assert!(
                    (size.x - w).abs() < 1e-4 || (size.y - w).abs() < 1e-4,
                    "{street:?}"
                );
                assert!(t.grid().contains(street));
            }
            // La plaza está rodeada de calles por los cuatro lados.
            for side in [Side::North, Side::South, Side::East, Side::West] {
                let n = normal(side);
                let reach = t.plaza.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
                let beside = t.plaza.center() + n * (reach + w / 2.0);
                assert!(t.streets.iter().any(|s| s.contains_point(beside)));
            }
        }
    }

    #[test]
    fn the_plaza_takes_the_central_block_when_there_is_one() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let central = c.blocks.0 % 2 == 1 && c.blocks.1 % 2 == 1;
            if c.plaza == PlazaPlacement::Auto && central {
                assert!(t.plaza.contains_point(t.grid().center()), "{}", c.name);
            }
        }
    }

    /// Comprueba que la plaza esté junto a la vía férrea, la más cercana al
    /// centro de la cuadrícula a lo largo de ella, con la estación alineada
    /// para mirarla del otro lado de la calle.
    fn assert_plaza_faces_the_station(c: &TownConfig, t: &TownLayout) {
        let side = c.railway.unwrap().side;
        let n = normal(side);
        let along = Vec2::new(n.y.abs(), n.x.abs());
        let grid = t.grid();
        // La manzana de la plaza linda con la calle perimetral del lado de la
        // vía férrea.
        let edge =
            |b: Aabb| b.center().dot(n) + b.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
        assert!(
            (edge(grid) - edge(t.plaza) - c.street_width).abs() < 1e-3,
            "{}: plaza not on the railway row",
            c.name
        );
        // La más cercana al centro a lo largo de la vía (los empates van al
        // índice más bajo).
        let offset = (t.plaza.center() - grid.center()).dot(along);
        let pitch = c.block_size + c.street_width;
        assert!(offset.abs() <= pitch / 2.0 + 1e-3, "{}: {offset}", c.name);
        assert!(offset <= 1e-3, "{}: ties go to the lower index", c.name);
        // La estación mira hacia la plaza, centrada respecto de ella, del otro
        // lado de la calle.
        let station = find(t, BuildingKind::Station).expect("a station");
        assert_eq!(station.front, side.opposite(), "{}", c.name);
        let off_axis = (station.footprint.center() - t.plaza.center()).dot(along);
        assert!(
            off_axis.abs() < 1e-3,
            "{}: station off by {off_axis}",
            c.name
        );
        assert!((station.footprint.center() - t.plaza.center()).dot(n) > 0.0);
    }

    #[test]
    fn without_a_central_block_the_plaza_faces_the_station() {
        let mut checked = 0;
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let central = c.blocks.0 % 2 == 1 && c.blocks.1 % 2 == 1;
            let facing = match c.plaza {
                _ if through(c) => false,
                PlazaPlacement::FacingStation => true,
                PlazaPlacement::Auto => {
                    !central && c.railway.is_some() && c.size() == TownSize::Small
                }
                _ => false,
            };
            if facing {
                assert_plaza_faces_the_station(c, &t);
                checked += 1;
            }
        }
        assert!(checked >= 3, "{checked} facing-station towns");
    }

    #[test]
    fn the_station_is_aligned_with_a_central_plaza_too() {
        let t = TownLayout::generate(&five_by_five()).unwrap();
        let station = find(&t, BuildingKind::Station).unwrap();
        assert!((station.footprint.center().x - t.plaza.center().x).abs() < 1e-3);
    }

    #[test]
    fn castex_is_four_by_five_with_the_plaza_facing_the_station() {
        let c = TownConfig::castex();
        assert_eq!(c.name, "Eduardo Castex");
        assert_eq!(
            (c.blocks, c.block_size, c.street_width),
            ((4, 5), 40.0, 12.0)
        );
        assert_eq!(c.plaza, PlazaPlacement::Auto);
        assert_eq!(
            c.railway,
            Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            })
        );
        let t = TownLayout::castex_inspired();
        assert_plaza_faces_the_station(&c, &t);
        // La iglesia está del otro lado de la calle, al sur de la plaza,
        // mirándola.
        let church = find(&t, BuildingKind::Church).unwrap();
        assert_eq!(church.front, Side::North);
        assert!(church.footprint.max.y < t.plaza.min.y);
    }

    #[test]
    fn an_explicit_plaza_block_is_honored() {
        let c = TownConfig {
            plaza: PlazaPlacement::Block(1, 3),
            ..TownConfig::castex()
        };
        let t = TownLayout::generate(&c).unwrap();
        let grid = t.grid();
        let pitch = c.block_size + c.street_width;
        let expected =
            grid.min + Vec2::new(c.street_width, c.street_width) + Vec2::new(pitch, 3.0 * pitch);
        assert!(t.plaza.min.distance(expected) < 1e-3, "{:?}", t.plaza);
        // Sin vía férrea y sin manzana central: la manzana más cercana al
        // centro.
        let even = TownConfig {
            blocks: (4, 6),
            railway: None,
            ..TownConfig::castex()
        };
        let t = TownLayout::generate(&even).unwrap();
        let pitch = even.block_size + even.street_width;
        let grid = t.grid();
        let expected = grid.min
            + Vec2::new(even.street_width, even.street_width)
            + Vec2::new(pitch, 2.0 * pitch);
        assert!(
            t.plaza.min.distance(expected) < 1e-3,
            "nearest center, lower index"
        );
    }

    #[test]
    fn landmarks_face_the_plaza_from_neighboring_blocks() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            for kind in [
                BuildingKind::GeneralStore,
                BuildingKind::PoliceStation,
                BuildingKind::Church,
            ] {
                let b = find(&t, kind).unwrap();
                // Su fachada mira hacia la plaza.
                let to_plaza = t.plaza.center() - b.footprint.center();
                assert!(to_plaza.dot(normal(b.front)) > 0.0, "{}: {kind:?}", c.name);
                // Y hay una calle justo delante.
                let half = b.footprint.size() * 0.5;
                let n = normal(b.front);
                let ahead = b.footprint.center()
                    + n * (n.x.abs() * half.x + n.y.abs() * half.y + c.street_width / 2.0);
                assert!(
                    t.streets.iter().any(|s| s.contains_point(ahead)),
                    "{}: {kind:?}",
                    c.name
                );
            }
        }
        // Alrededor de una plaza central: la iglesia al norte, mirándola; el
        // almacén y la comisaría alejados de ella y entre sí.
        let t = TownLayout::generate(&five_by_five()).unwrap();
        assert_eq!(find(&t, BuildingKind::Church).unwrap().front, Side::South);
    }

    /// Los edificios cívicos de `t`: almacenes, comisarías y la iglesia.
    fn civic(t: &TownLayout) -> Vec<&Building> {
        t.buildings
            .iter()
            .filter(|b| {
                matches!(
                    b.kind,
                    BuildingKind::GeneralStore | BuildingKind::PoliceStation | BuildingKind::Church
                )
            })
            .collect()
    }

    #[test]
    fn civic_buildings_are_spread_over_every_town() {
        // Usuario, 2026-10-23: nunca uno al lado del otro, los del mismo tipo a
        // más o menos la mitad del pueblo de distancia, para que todo el pueblo
        // tenga vida.
        let mut configs = small_configs();
        configs.extend(
            sample_configs()
                .into_iter()
                .filter(|c| c.size() != TownSize::Small),
        );
        for c in configs {
            let t = TownLayout::generate(&c).unwrap();
            let pitch = c.block_size + c.street_width;
            let same = (same_kind_spacing(c.blocks.0, c.blocks.1) - 1) as f32 * pitch;
            let buildings = civic(&t);
            for (k, a) in buildings.iter().enumerate() {
                for b in &buildings[k + 1..] {
                    let d = a.footprint.center() - b.footprint.center();
                    let apart = d.x.abs().max(d.y.abs());
                    // Al menos una manzana entera de por medio, no una calle.
                    assert!(apart > pitch, "{}: {a:?} by {b:?}", c.name);
                    if a.kind == b.kind {
                        assert!(apart > same, "{}: two {:?} {apart} m apart", c.name, a.kind);
                    }
                }
            }
        }
        // La dispersión no es casualidad de los ejemplos: un pueblo chico
        // también mantiene separados su almacén, su comisaría y su iglesia.
        let t = TownLayout::castex_inspired();
        assert_eq!(civic(&t).len(), 3);
    }

    #[test]
    fn a_town_split_by_its_line_has_a_store_and_a_comisaria_on_each_side() {
        let (_, t) = partida();
        let y = t.railway.unwrap().start.y;
        for kind in [BuildingKind::GeneralStore, BuildingKind::PoliceStation] {
            let sides: Vec<bool> = all(&t, kind)
                .iter()
                .map(|b| b.footprint.center().y > y)
                .collect();
            assert_eq!(sides.len(), 2, "{kind:?}");
            assert_ne!(sides[0], sides[1], "{kind:?} both on one side");
        }
        // La iglesia mira hacia la plaza, en la mitad de la estación.
        let church = find(&t, BuildingKind::Church).unwrap();
        assert!(church.footprint.center().y > y);
    }

    #[test]
    fn landmarks_scale_with_the_block() {
        let small = TownLayout::generate(&TownConfig {
            block_size: 32.0,
            ..TownConfig::castex()
        })
        .unwrap();
        let big = TownLayout::generate(&TownConfig {
            block_size: 50.0,
            ..TownConfig::castex()
        })
        .unwrap();
        for kind in [BuildingKind::GeneralStore, BuildingKind::Church] {
            let (a, b) = (find(&small, kind).unwrap(), find(&big, kind).unwrap());
            let ratio = b.footprint.size().x / a.footprint.size().x;
            assert!((ratio - 50.0 / 32.0).abs() < 1e-3, "{kind:?} {ratio}");
        }
    }

    #[test]
    fn buildings_never_overlap_streets_plaza_or_each_other() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            for (i, a) in t.buildings.iter().enumerate() {
                for plaza in t.plazas() {
                    assert!(
                        !a.footprint.intersects(&plaza),
                        "{}: {a:?} in plaza {plaza:?}",
                        c.name
                    );
                }
                for street in &t.streets {
                    assert!(
                        !a.footprint.intersects(street),
                        "{}: {a:?} on {street:?}",
                        c.name
                    );
                }
                for b in &t.buildings[i + 1..] {
                    assert!(
                        !a.footprint.intersects(&b.footprint),
                        "{}: {a:?} vs {b:?}",
                        c.name
                    );
                }
                assert!(a.height > 0.0 && a.footprint.size().x > 0.0 && a.footprint.size().y > 0.0);
            }
        }
    }

    #[test]
    fn trees_stand_inside_the_plaza() {
        for t in sample_towns() {
            let inside = |p: Vec2| {
                t.plazas()
                    .any(|plaza| plaza.expanded(-TREE_TRUNK_RADIUS).contains_point(p))
            };
            assert!(t.trees.iter().all(|&p| inside(p)), "{}", t.name);
        }
    }

    #[test]
    fn plaza_trees_stand_in_the_beds_clear_of_the_paths_and_the_monument() {
        let mut checked = 0;
        for t in sample_towns() {
            for &tree in &t.trees {
                let plaza = t
                    .plazas()
                    .find(|p| p.contains_point(tree))
                    .expect("every tree is in a plaza");
                let reach = TREE_TRUNK_RADIUS + PLAZA_PATH_CLEARANCE;
                for keep_off in plaza_paths(plaza)
                    .into_iter()
                    .chain([plaza_monument(plaza)])
                {
                    let gap = keep_off.closest_point(tree).distance(tree);
                    assert!(
                        gap >= reach - 1e-4,
                        "{}: tree {tree:?} {gap} m off a path",
                        t.name
                    );
                }
                checked += 1;
            }
            // Sigue siendo una plaza con sombra.
            assert!(t.trees.len() >= 8, "{}", t.name);
        }
        assert!(checked > 100, "{checked}");
    }

    #[test]
    fn railway_runs_past_the_town_edge_by_the_station() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let Some(config) = c.railway else {
                assert_eq!(t.railway, None);
                continue;
            };
            if config.through {
                // En cambio, por el medio: ver su propia prueba.
                continue;
            }
            let rail = t.railway.expect("a railway");
            let n = normal(config.side);
            let grid = t.grid();
            let along = Vec2::new(n.y.abs(), n.x.abs());
            // Una línea recta paralela al lado de la cuadrícula, que pasa más
            // allá de ambos extremos de la cuadrícula por el tramo final.
            assert!((rail.end - rail.start).dot(n).abs() < 1e-4);
            let edge =
                grid.center().dot(n) + grid.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
            assert!(
                (rail.start.dot(n) - edge - config.offset).abs() < 1e-3,
                "{}",
                c.name
            );
            let (lo, hi) = (
                grid.min.dot(along) - TownLayout::RAIL_RUN_OUT,
                grid.max.dot(along) + TownLayout::RAIL_RUN_OUT,
            );
            assert!((rail.start.dot(along) - lo).abs() < 1e-3, "{}", c.name);
            assert!((rail.end.dot(along) - hi).abs() < 1e-3, "{}", c.name);
            let corridor = t.rail_corridor().expect("a corridor");
            assert!(corridor.contains_point(rail.start) && corridor.contains_point(rail.end));
            assert!(
                (corridor.size().dot(Vec2::new(n.x.abs(), n.y.abs()))
                    - 2.0 * TownLayout::RAIL_CORRIDOR_HALF)
                    .abs()
                    < 1e-3
            );
            // La estación está entre la cuadrícula y la vía, cerca de ella.
            let station = find(&t, BuildingKind::Station).unwrap();
            let near = station.footprint.center().dot(n)
                - station
                    .footprint
                    .size()
                    .dot(Vec2::new(n.x.abs(), n.y.abs()))
                    / 2.0;
            let far = near
                + station
                    .footprint
                    .size()
                    .dot(Vec2::new(n.x.abs(), n.y.abs()));
            assert!(near > edge, "{}", c.name);
            assert!(rail.start.dot(n) - far < 15.0, "{}", c.name);
            // El ancho de un tren: el borde del andén queda justo afuera.
            let corridor = Aabb::from_center_size(
                rail.start,
                Vec2::new(n.x.abs(), n.y.abs()) * 3.0 + along * 2000.0,
            );
            for b in &t.buildings {
                assert!(
                    !b.footprint.intersects(&corridor),
                    "{}: {b:?} on the track",
                    c.name
                );
            }
        }
    }

    #[test]
    fn everything_built_sits_on_level_ground() {
        let terrain = Terrain::default();
        for t in sample_towns() {
            let area = t.flat_area(&terrain);
            assert_eq!(
                area.height,
                terrain.natural_height(area.center.x, area.center.y)
            );
            let leveled = terrain.clone().with_flat_area(area);
            assert_eq!(t.leveled(), t.extent().expanded(area.falloff), "{}", t.name);
            assert!(t.extent().contains(&t.grid()));
            for b in &t.buildings {
                assert!(t.extent().contains(&b.footprint), "{}: {b:?}", t.name);
                for corner in [b.footprint.min, b.footprint.max] {
                    assert_eq!(leveled.height(corner.x, corner.y), area.height);
                }
            }
        }
    }

    #[test]
    fn obstacles_cover_buildings_and_trees() {
        for t in sample_towns() {
            let obstacles = t.obstacles();
            let solid = t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform);
            let furniture: Vec<Aabb> = t
                .plaza_furniture()
                .iter()
                .flat_map(PlazaFurniture::obstacles)
                .collect();
            let palenques: usize = t.palenques.iter().map(|p| p.obstacles().count()).sum();
            assert_eq!(
                obstacles.len(),
                solid.clone().count()
                    + t.trees.len()
                    + t.shade_trees.len()
                    + 2
                    + palenques
                    + t.sign_obstacles().len()
                    + furniture.len()
            );
            assert!(
                t.shade_trees
                    .iter()
                    .all(|s| obstacles.contains(&s.obstacle()))
            );
            assert!(furniture.iter().all(|f| obstacles.contains(f)));
            assert!(obstacles.contains(&t.hitching_post.rail));
            assert!(obstacles.contains(&t.hitching_post.horse.footprint));
            for b in solid {
                assert!(obstacles.contains(&b.collider()));
            }
        }
    }

    #[test]
    fn plaza_furniture_follows_the_town_size() {
        let mut seen = std::collections::HashSet::new();
        for t in sample_towns() {
            let paving = Paving::of(t.blocks());
            let all = t.plaza_furniture();
            assert_eq!(all.len(), t.plazas().count(), "{}", t.name);
            for (k, (f, area)) in all.iter().zip(t.plazas()).enumerate() {
                seen.insert(paving);
                assert_eq!((f.area, f.paving), (area, paving));
                // El monumento está sobre el núcleo del pedestal que siempre se
                // le reserva, más chico en un pueblito.
                assert!(plaza_monument(area).contains(&f.monument));
                assert_eq!(f.monument.center(), area.center());
                let side = if paving == Paving::None { 2.4 } else { 3.0 };
                assert!((f.monument.size().x - side).abs() < 1e-4, "{}", t.name);
                assert_eq!(f.crossing, plaza_crossing(area));
                // Una reja solo en la ciudad, abierta en las cuatro entradas.
                let railed = paving == Paving::Everywhere;
                assert_eq!(f.railing.len(), if railed { 8 } else { 0 }, "{}", t.name);
                // Bancos según la categoría: 24 alrededor de la plaza principal
                // de una ciudad, 4 en la de un pueblo, 2 en la de un pueblito.
                let want = PlazaFurniture::bench_count(paving, k == 0);
                let main_want = match paving {
                    Paving::Everywhere => 24,
                    Paving::Plaza => 4,
                    Paving::None => 2,
                };
                assert!(!f.benches.is_empty(), "{}", t.name);
                assert!(f.benches.len() <= want, "{}", t.name);
                if k == 0 {
                    assert_eq!(f.benches.len(), main_want, "{}", t.name);
                }
                // Una plaza escasa pone como mucho un banco por cuadrante.
                if paving != Paving::Everywhere {
                    let c = area.center();
                    let quadrants: std::collections::HashSet<(bool, bool)> = f
                        .benches
                        .iter()
                        .map(|b| (b.at.x > c.x, b.at.y > c.y))
                        .collect();
                    assert_eq!(quadrants.len(), f.benches.len(), "{}", t.name);
                }
            }
        }
        assert_eq!(seen.len(), 3, "the samples cover every tier: {seen:?}");
    }

    #[test]
    fn plaza_furniture_keeps_off_the_paths_streets_trees_and_signs() {
        for t in sample_towns() {
            let mut clear: Vec<Aabb> = t.streets.clone();
            clear.extend(t.sidewalks().iter().map(|s| s.area));
            clear.extend(t.sign_obstacles());
            let trunk = Vec2::new(2.0 * TREE_TRUNK_RADIUS, 2.0 * TREE_TRUNK_RADIUS);
            clear.extend(t.trees.iter().map(|&p| Aabb::from_center_size(p, trunk)));
            for f in t.plaza_furniture() {
                let mut paths = plaza_paths(f.area).to_vec();
                for o in f
                    .benches
                    .iter()
                    .map(Bench::footprint)
                    .chain(f.railing.clone())
                {
                    assert!(f.area.contains(&o), "{}: {o:?} outside", t.name);
                    assert!(!o.intersects(&f.crossing), "{}: {o:?}", t.name);
                    for c in paths.iter().chain(&clear) {
                        assert!(!o.intersects(c), "{}: {o:?} on {c:?}", t.name);
                    }
                }
                // Un banco mira hacia su sendero, con su frente justo al lado
                // del sendero.
                for b in &f.benches {
                    let front = b.at + crate::places::side_normal(b.facing) * (Bench::DEPTH / 2.0);
                    let gap = paths
                        .iter()
                        .map(|p| p.closest_point(front).distance(front))
                        .fold(f32::MAX, f32::min);
                    assert!((gap - Bench::PATH_GAP).abs() < 1e-3, "{}: {b:?}", t.name);
                }
                // Los senderos quedan abiertos de borde a borde: la reja solo
                // corre a su lado.
                paths.push(f.crossing);
                for r in &f.railing {
                    let mouth = Aabb::from_center_size(
                        f.area.center(),
                        Vec2::new(2.0, 2.0) * PlazaFurniture::MOUTH_HALF,
                    );
                    let across_x = Aabb::new(
                        Vec2::new(f.area.min.x, mouth.min.y),
                        Vec2::new(f.area.max.x, mouth.max.y),
                    );
                    let across_y = Aabb::new(
                        Vec2::new(mouth.min.x, f.area.min.y),
                        Vec2::new(mouth.max.x, f.area.max.y),
                    );
                    assert!(
                        !r.intersects(&across_x) && !r.intersects(&across_y),
                        "{r:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_galpon_collides_at_its_walls_not_at_its_eaves() {
        let mut galpones = 0;
        for t in sample_towns() {
            let obstacles = t.obstacles();
            for b in t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
            {
                let barn = matches!(b.kind, BuildingKind::Shed | BuildingKind::GrainStore);
                let inset = if barn { Building::BARN_EAVES } else { 0.0 };
                let want = b.footprint.expanded(-inset);
                assert_eq!(b.collider(), want, "{}: {:?}", t.name, b.kind);
                assert!(obstacles.contains(&want), "{}: {:?}", t.name, b.kind);
                galpones += usize::from(barn);
            }
        }
        assert!(galpones > 10, "{galpones} galpones");
    }

    /// Si `p` está sobre una de las calles de `t`.
    fn on_street(t: &TownLayout, p: Vec2) -> bool {
        t.streets.iter().any(|s| s.contains_point(p))
    }

    #[test]
    fn paving_follows_the_town_size() {
        assert_eq!(Paving::of((3, 3)), Paving::None);
        assert_eq!(Paving::of((4, 3)), Paving::Plaza);
        assert_eq!(Paving::of((5, 4)), Paving::Plaza);
        assert_eq!(Paving::of((6, 6)), Paving::Plaza);
        assert_eq!(Paving::of((7, 6)), Paving::Everywhere);
        assert_eq!(Paving::of((8, 8)), Paving::Everywhere);
    }

    #[test]
    fn sidewalks_lie_on_the_streets_clear_of_buildings_and_plazas() {
        let pico = TownLayout::generate(&TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        let mut paved = 0;
        for t in sample_towns().into_iter().chain([pico]) {
            for walk in t.sidewalks() {
                let a = walk.area;
                assert!(
                    (a.size().x.min(a.size().y) - Sidewalk::WIDTH).abs() < 1e-3,
                    "{}: {a:?}",
                    t.name
                );
                // Esquinas (justo por dentro) y centro sobre una calle.
                let inner = a.expanded(-0.01);
                for p in [
                    inner.min,
                    inner.max,
                    Vec2::new(inner.min.x, inner.max.y),
                    a.center(),
                ] {
                    assert!(
                        on_street(&t, p),
                        "{}: {a:?} off the street at {p:?}",
                        t.name
                    );
                }
                for b in &t.buildings {
                    assert!(
                        !overlap(&a, &b.footprint),
                        "{}: {a:?} on {:?}",
                        t.name,
                        b.kind
                    );
                }
                for plaza in t.plazas() {
                    assert!(!overlap(&a, &plaza), "{}: {a:?} in a plaza", t.name);
                }
                assert!(!walk.curbs.is_empty());
                paved += 1;
            }
        }
        assert!(paved > 200, "{paved} strips");
    }

    #[test]
    fn a_city_paves_every_block_in_brick_a_town_its_plaza_and_a_village_nothing_but_earth() {
        let towns = sample_towns();
        let count =
            |t: &TownLayout, walk: Walk| t.sidewalks().iter().filter(|w| w.walk == walk).count();
        // Cada franja de cada pueblo tiene sus cordones.
        for t in &towns {
            assert!(
                t.sidewalks().iter().all(|w| !w.curbs.is_empty()),
                "{}",
                t.name
            );
        }
        // Pico: un anillo alrededor de cada manzana salvo la de la plaza, y el de la
        // plaza, todo de ladrillo.
        let pico = TownLayout::generate(&TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        assert_eq!(pico.sidewalks().len(), 4 * (64 - 4 + 1));
        assert_eq!(count(&pico, Walk::Brick), 244);
        // Castex (20 manzanas): ladrillo alrededor de la única manzana de la
        // plaza, cuatro franjas, cada una contra la plaza; tierra alrededor de
        // las otras 19 manzanas.
        let castex = &towns[0];
        assert_eq!(Paving::of(castex.blocks()), Paving::Plaza);
        assert_eq!(castex.sidewalks().len(), 80);
        assert_eq!(
            (count(castex, Walk::Brick), count(castex, Walk::Earth)),
            (4, 76)
        );
        let ring = castex.plaza.expanded(Sidewalk::WIDTH);
        for walk in castex.sidewalks().iter().filter(|w| w.walk == Walk::Brick) {
            assert!(
                ring.contains(&walk.area),
                "{:?} off the plaza's edge",
                walk.area
            );
            assert!(!overlap(&walk.area, &castex.plaza));
        }
        // Un pueblito de 3 x 3: nueve anillos menos el de la manzana de la
        // plaza más el de la plaza, todos de tierra apisonada.
        let village = towns.iter().find(|t| t.blocks() == (3, 3)).unwrap();
        assert_eq!(Paving::of(village.blocks()), Paving::None);
        assert_eq!(village.sidewalks().len(), 36);
        assert_eq!(count(village, Walk::Earth), 36);
        // El anillo de la plaza son cuatro franjas contra la plaza, de tierra
        // apisonada como el resto, nunca de ladrillo.
        let ring = village.plaza.expanded(Sidewalk::WIDTH);
        let round_plaza = village
            .sidewalks()
            .iter()
            .filter(|w| ring.contains(&w.area) && !overlap(&w.area, &village.plaza))
            .filter(|w| overlap(&w.area.expanded(0.01), &village.plaza))
            .count();
        assert_eq!(round_plaza, 4);
    }

    #[test]
    fn the_platform_is_a_floor_with_clear_ramps_not_an_obstacle() {
        let mut checked = 0;
        for t in sample_towns() {
            let obstacles = t.obstacles();
            let platforms: Vec<&Building> = t
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::Platform)
                .collect();
            let floors = t.floors();
            assert_eq!(floors.len(), platforms.len(), "{}", t.name);
            for (floor, platform) in floors.iter().zip(platforms) {
                assert!(!obstacles.contains(&platform.footprint), "{}", t.name);
                assert_eq!(floor.footprint, platform.footprint);
                assert_eq!(floor.height, platform.height);
                // A lo largo de la vía, así que las rampas van a lo largo de
                // ella.
                let rail = t.railway.unwrap();
                let track = rail.end - rail.start;
                assert!((floor.along.dot(track) - track.length()).abs() < 1e-3);
                // Nada construido está sobre el andén ni sobre sus rampas.
                for area in floor.ramps().into_iter().chain([floor.footprint]) {
                    for o in &obstacles {
                        let inside = Aabb::new(
                            area.min + Vec2::new(0.01, 0.01),
                            area.max - Vec2::new(0.01, 0.01),
                        );
                        assert!(!o.intersects(&inside), "{}: {o:?} on {area:?}", t.name);
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 3);
    }

    #[test]
    fn the_station_canopy_hangs_over_the_platform_from_the_track_side_wall() {
        let mut checked = 0;
        for t in sample_towns() {
            let canopies = t.canopies();
            let Some(station) = find(&t, BuildingKind::Station) else {
                assert!(canopies.is_empty(), "{}", t.name);
                continue;
            };
            let platform = find(&t, BuildingKind::Platform).expect("a platform");
            assert_eq!(canopies.len(), 1, "{}", t.name);
            let canopy = canopies[0];
            // Al ras de la pared trasera de la estación, del lado de la vía.
            let (s, c) = (station.footprint, canopy.footprint);
            assert!(!s.intersects(&c.expanded(-0.01)), "{}", t.name);
            assert!(
                s.intersects(&c.expanded(0.01)),
                "{}: touches the station",
                t.name
            );
            let back = station.front.opposite();
            let toward = c.center() - s.center();
            let out = match back {
                Side::North => toward.y,
                Side::South => -toward.y,
                Side::East => toward.x,
                Side::West => -toward.x,
            };
            assert!(out > 0.0, "{}: on the track side", t.name);
            // Tan ancho como la estación y tan profundo como el techo que
            // dibuja el render.
            let size = c.size();
            let (width, depth, wall) = match back {
                Side::North | Side::South => (size.x, size.y, s.size().x),
                Side::East | Side::West => (size.y, size.x, s.size().y),
            };
            assert!((depth - Canopy::DEPTH).abs() < 1e-4, "{}", t.name);
            assert!((width - wall).abs() < 1e-4, "{}", t.name);
            // Sobre el andén, con espacio libre por encima de su parte
            // superior.
            assert!(platform.footprint.contains(&c), "{}", t.name);
            assert!(canopy.bottom() - platform.height > 2.0, "{}", t.name);
            assert!(canopy.top() > canopy.bottom());
            checked += 1;
        }
        assert!(checked > 3);
    }

    /// Distancia desde la línea central de la vía férrea hasta los lados
    /// cercano y lejano de `area`, a lo ancho de la vía (las vías de los
    /// pueblos corren a lo largo de un eje).
    fn across_the_track(t: &TownLayout, area: Aabb) -> (f32, f32) {
        let rail = t.railway.unwrap();
        let span = rail.end - rail.start;
        let dir = span * (1.0 / span.length());
        let normal = Vec2::new(-dir.y, dir.x);
        let (lo, hi) = (area.min, area.max);
        [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y)]
            .iter()
            .map(|c| (*c - rail.start).dot(normal).abs())
            .fold((f32::MAX, f32::MIN), |(n, f), d| (n.min(d), f.max(d)))
    }

    #[test]
    fn every_platforms_track_edge_is_1_65_m_from_the_track_and_its_far_edge_11_m() {
        let mut checked = 0;
        for t in sample_towns() {
            let Some(platform) = find(&t, BuildingKind::Platform) else {
                continue;
            };
            let (near, far) = across_the_track(&t, platform.footprint);
            assert!((near - 1.65).abs() < 1e-3, "{}: {near}", t.name);
            assert!((far - 11.0).abs() < 1e-3, "{}: {far}", t.name);
            assert_eq!(Floor::PLATFORM_EDGE, 1.65);
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn the_canopy_edge_and_its_posts_stay_5_5_m_from_the_track_clear_of_a_train() {
        let mut checked = 0;
        for t in sample_towns() {
            let Some(canopy) = t.canopies().first().copied() else {
                continue;
            };
            let (near, far) = across_the_track(&t, canopy.footprint);
            // El techo termina donde terminaba, a 5,5 m de la vía (sus columnas
            // están 0,4 m hacia adentro del borde, a 5,1 m): el costado de un
            // tren está a 1,5 m, así que ninguno de los dos llega por encima de
            // los vagones.
            assert!((near - 5.5).abs() < 1e-3, "{}: {near}", t.name);
            assert!(far > near);
            assert!(near > crate::LineTrain::WIDTH / 2.0 + 3.0, "{}", t.name);
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn a_horse_is_tied_in_front_of_the_general_store() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let store = find(&t, BuildingKind::GeneralStore).unwrap();
            let post = t.hitching_post;
            let horse = post.horse;
            // En la calle a la que mira el almacén, justo más allá de su
            // galería.
            let street = t
                .streets
                .iter()
                .find(|s| s.contains(&horse.footprint))
                .expect("horse stands on a street");
            assert!(street.contains(&post.rail));
            let n = normal(store.front);
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let half = store.footprint.size() * 0.5;
            let face = store.footprint.center().dot(n) + half.dot(Vec2::new(n.x.abs(), n.y.abs()));
            let horse_half = horse.footprint.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
            let gap = horse.footprint.center().dot(n) - horse_half - face;
            assert!(
                (post.veranda..post.veranda + 3.5).contains(&gap),
                "{}: gap {gap}",
                c.name
            );
            assert!((1.0..=3.5).contains(&post.veranda));
            let off = (horse.footprint.center() - store.footprint.center()).dot(along);
            assert!(off.abs() < 1e-4);
            assert_eq!(horse.position, horse.footprint.center());
            // Mira a lo largo de la calle, atado al lado del palenque sin tocarlo.
            assert!(horse.heading.forward().dot(n).abs() < 1e-5);
            assert!(!horse.footprint.intersects(&post.rail));
            // Deja lugar para pasar: un jinete entra entre el caballo y el otro
            // lado.
            let far =
                street.center().dot(n) + street.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
            let room = far - (horse.footprint.center().dot(n) + horse_half);
            assert!(
                room > 2.0 * Rider::COLLIDER_RADIUS + 2.0,
                "{}: room {room}",
                c.name
            );
            // Y lejos de los carriles de los peatones por el medio de la calle.
            let middle = street.center().dot(n);
            assert!(
                middle - (horse.footprint.center().dot(n) + horse_half) > 1.25,
                "{}",
                c.name
            );
        }
    }

    #[test]
    fn arrival_is_on_open_road_outside_the_grid_facing_in() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let (spawn, heading) = t.arrival();
            let grid = t.grid();
            assert!(!grid.contains_point(spawn), "{}", c.name);
            let inward = heading.forward();
            assert!(
                grid.contains_point(spawn + inward * 40.0),
                "{}: faces into town",
                c.name
            );
            if let Some(r) = c.railway.filter(|r| !r.through) {
                assert!(
                    inward.dot(normal(r.side)) > -1e-4,
                    "{}: comes from away from the track",
                    c.name
                );
            }
            // Con una línea por el medio, entra del lado de la estación, desde
            // el borde opuesto a la vía.
            if let (Some(r), Some(band)) = (c.railway, t.rail_band()) {
                assert!(
                    (spawn - band.center()).dot(normal(r.side)) > 0.0,
                    "{}",
                    c.name
                );
            }
            let road = t.streets.iter().any(|s| {
                s.contains_point(spawn + inward * 30.0)
                    && (s.size().dot(Vec2::new(inward.x.abs(), inward.y.abs())) > c.street_width)
            });
            assert!(road, "{}: lines up with a street into town", c.name);
            for o in t.obstacles() {
                assert!(!o.expanded(2.0).contains_point(spawn));
            }
            assert!(t.leveled().expanded(-5.0).contains_point(spawn));
        }
    }

    #[test]
    fn invalid_configs_are_rejected_with_the_right_error() {
        let base = TownConfig::castex();
        let cases = [
            (
                TownConfig {
                    name: "  ".into(),
                    ..base.clone()
                },
                TownConfigError::EmptyName,
            ),
            (
                TownConfig {
                    block_size: f32::NAN,
                    ..base.clone()
                },
                TownConfigError::NotFinite {
                    field: "block_size",
                },
            ),
            (
                TownConfig {
                    origin: Vec2::new(f32::INFINITY, 0.0),
                    ..base.clone()
                },
                TownConfigError::NotFinite { field: "origin" },
            ),
            (
                TownConfig {
                    blocks: (2, 5),
                    ..base.clone()
                },
                TownConfigError::TooFewBlocks {
                    blocks: (2, 5),
                    min: 3,
                },
            ),
            (
                TownConfig {
                    blocks: (5, 0),
                    ..base.clone()
                },
                TownConfigError::TooFewBlocks {
                    blocks: (5, 0),
                    min: 3,
                },
            ),
            (
                TownConfig {
                    block_size: 20.0,
                    ..base.clone()
                },
                TownConfigError::BlockTooSmall {
                    block_size: 20.0,
                    min: 30.0,
                },
            ),
            (
                TownConfig {
                    street_width: 6.0,
                    ..base.clone()
                },
                TownConfigError::StreetTooNarrow {
                    street_width: 6.0,
                    min: 10.0,
                },
            ),
            (
                TownConfig {
                    railway: Some(RailwayConfig {
                        side: Side::North,
                        offset: 10.0,
                        through: false,
                    }),
                    ..base.clone()
                },
                TownConfigError::RailwayTooClose {
                    offset: 10.0,
                    min: 24.0,
                },
            ),
            (
                TownConfig {
                    plaza: PlazaPlacement::Block(5, 0),
                    ..base.clone()
                },
                TownConfigError::PlazaOutOfRange {
                    block: (5, 0),
                    blocks: (4, 5),
                },
            ),
            (
                TownConfig {
                    blocks: (4, 5),
                    plaza: PlazaPlacement::Center,
                    ..base.clone()
                },
                TownConfigError::NoCentralBlock { blocks: (4, 5) },
            ),
            (
                TownConfig {
                    railway: None,
                    plaza: PlazaPlacement::FacingStation,
                    ..base.clone()
                },
                TownConfigError::NoStationToFace,
            ),
        ];
        for (config, expected) in cases {
            assert_eq!(TownLayout::generate(&config), Err(expected.clone()));
            assert!(!expected.to_string().is_empty());
        }
        // Hasta dónde llega un pueblo es asunto del mundo, no del pueblo:
        // cualquier tamaño y origen que se pueda construir es un pueblo válido.
        for config in [
            TownConfig {
                blocks: (12, 5),
                ..base.clone()
            },
            TownConfig {
                origin: Vec2::new(5150.0, -2000.0),
                ..base.clone()
            },
            TownConfig {
                block_size: 90.0,
                ..base.clone()
            },
        ] {
            TownLayout::generate(&config).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn a_town_is_the_same_wherever_it_stands() {
        let offset = Vec2::new(3000.0, -2000.0);
        for c in sample_configs() {
            let here = TownLayout::generate(&c).unwrap();
            let there = TownLayout::generate(&TownConfig {
                origin: c.origin + offset,
                ..c.clone()
            })
            .unwrap();
            let moved = |a: Aabb| Aabb::new(a.min + offset, a.max + offset);
            let near =
                |a: Aabb, b: Aabb| a.min.distance(b.min) < 1e-2 && a.max.distance(b.max) < 1e-2;
            assert!(near(there.grid(), moved(here.grid())), "{}", c.name);
            assert!(near(there.extent(), moved(here.extent())), "{}", c.name);
            assert!(near(there.leveled(), moved(here.leveled())), "{}", c.name);
            assert!(near(there.plaza, moved(here.plaza)), "{}", c.name);
            match (there.station_plaza, here.station_plaza) {
                (Some(a), Some(b)) => assert!(near(a, moved(b)), "{}", c.name),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
            assert_eq!(there.streets.len(), here.streets.len());
            for (a, b) in there.streets.iter().zip(&here.streets) {
                assert!(near(*a, moved(*b)), "{}", c.name);
            }
            assert_eq!(there.buildings.len(), here.buildings.len());
            for (a, b) in there.buildings.iter().zip(&here.buildings) {
                assert_eq!((a.kind, a.front), (b.kind, b.front));
                assert!(near(a.footprint, moved(b.footprint)), "{}", c.name);
            }
            assert!(there.arrival().0.distance(here.arrival().0 + offset) < 1e-2);
            assert_eq!(there.arrival().1, here.arrival().1);
            match (there.railway, here.railway) {
                (Some(a), Some(b)) => {
                    assert!(a.start.distance(b.start + offset) < 1e-2);
                    assert!(a.end.distance(b.end + offset) < 1e-2);
                }
                (None, None) => {}
                other => panic!("{other:?}"),
            }
            let graph = |t: &TownLayout| crate::StreetGraph::from_town(t).nodes;
            for (a, b) in graph(&there).iter().zip(graph(&here)) {
                assert!(a.distance(b + offset) < 1e-2, "{}", c.name);
            }
            for (a, b) in there.places().iter().zip(here.places()) {
                assert_eq!(a.kind, b.kind);
                for (sa, sb) in a.signs.iter().zip(&b.signs) {
                    assert!(sa.anchor.distance(sb.anchor + offset) < 1e-2);
                }
            }
            assert_eq!(there.seed, c.seed);
        }
    }

    /// Hash FNV-1a de todo lo que un pueblo construye sobre el suelo, para
    /// fijar las distribuciones.
    fn fingerprint(t: &TownLayout) -> u64 {
        let text = format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            t.streets,
            t.plaza,
            t.trees,
            t.railway,
            t.buildings,
            t.hitching_post,
            t.arrival(),
            t.extent(),
            t.places()
        );
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        })
    }

    /// Si la línea de `c` atraviesa el pueblo por el medio: estos pueblos
    /// tienen sus propias pruebas, no las de un pueblo al costado de su línea.
    fn through(c: &TownConfig) -> bool {
        c.railway.is_some_and(|r| r.through)
    }

    /// Pueblos chicos al costado de su línea o sin ella: las configuraciones de
    /// ejemplo de 36 manzanas o menos, más Monte Nievas y el Castex de 5 x 4 de
    /// `assets/world/towns.ron`.
    fn small_configs() -> Vec<TownConfig> {
        let shipped = [
            TownConfig {
                name: "Monte Nievas".into(),
                blocks: (4, 3),
                seed: 7,
                // Tal como se distribuye: sin comisaría (la historia del
                // encargo).
                police_stations: Some(0),
                ..TownConfig::castex()
            },
            TownConfig {
                blocks: (5, 4),
                seed: 2,
                ..TownConfig::castex()
            },
        ];
        shipped
            .into_iter()
            .chain(
                sample_configs()
                    .into_iter()
                    .filter(|c| c.size() == TownSize::Small && !through(c)),
            )
            .collect()
    }

    /// Pueblos de ejemplo grandes y muy grandes, generados, con sus
    /// configuraciones.
    fn bigger_towns() -> Vec<(TownConfig, TownLayout)> {
        sample_configs()
            .into_iter()
            .zip(sample_towns())
            .filter(|(c, _)| c.size() != TownSize::Small && !through(c))
            .collect()
    }

    fn all(t: &TownLayout, kind: BuildingKind) -> Vec<&Building> {
        t.buildings.iter().filter(|b| b.kind == kind).collect()
    }

    /// Manzanas `(east-west, north-south)` que abarca la plaza principal.
    fn plaza_span(c: &TownConfig, t: &TownLayout) -> (usize, usize) {
        let pitch = c.block_size + c.street_width;
        let blocks = |len: f32| ((len + c.street_width) / pitch).round() as usize;
        (blocks(t.plaza.size().x), blocks(t.plaza.size().y))
    }

    /// Comprueba que `b` mire hacia `area` con una calle justo delante.
    fn assert_faces(t: &TownLayout, b: &Building, area: Aabb, street_width: f32) {
        let n = normal(b.front);
        assert!(
            (area.center() - b.footprint.center()).dot(n) > 0.0,
            "{}: {b:?} looks away from {area:?}",
            t.name
        );
        let half = b.footprint.size() * 0.5;
        let ahead = b.footprint.center()
            + n * (n.x.abs() * half.x + n.y.abs() * half.y + street_width / 2.0);
        assert!(
            t.streets.iter().any(|s| s.contains_point(ahead)),
            "{}: no street in front of {b:?}",
            t.name
        );
    }

    #[test]
    fn town_size_follows_the_block_count() {
        for (blocks, size) in [
            ((3, 3), TownSize::Small),
            ((5, 4), TownSize::Small),
            ((6, 6), TownSize::Small),
            ((9, 4), TownSize::Small),
            ((37, 1), TownSize::Big),
            ((7, 6), TownSize::Big),
            ((9, 7), TownSize::Big),
            ((8, 8), TownSize::VeryBig),
            ((16, 4), TownSize::VeryBig),
        ] {
            let c = TownConfig {
                blocks,
                ..TownConfig::castex()
            };
            assert_eq!(c.size(), size, "{blocks:?}");
        }
        assert!(sample_configs().iter().any(|c| c.size() == TownSize::Big));
        assert!(
            sample_configs()
                .iter()
                .any(|c| c.size() == TownSize::VeryBig)
        );
    }

    #[test]
    fn small_towns_keep_one_plaza_store_and_police_station() {
        for c in small_configs() {
            let t = TownLayout::generate(&c).unwrap();
            assert_eq!(t.size, TownSize::Small, "{}", c.name);
            assert_eq!(t.station_plaza, None, "{}", c.name);
            assert_eq!(t.plazas().count(), 1, "{}", c.name);
            assert_eq!(plaza_span(&c, &t), (1, 1), "{}", c.name);
            assert_eq!(count(&t, BuildingKind::GeneralStore), 1, "{}", c.name);
            // Una, salvo que se reemplace (Monte Nievas no tiene ninguna).
            let police = c.police_stations.unwrap_or(1);
            assert_eq!(count(&t, BuildingKind::PoliceStation), police, "{}", c.name);
            assert_eq!(t.townsfolk_count(), crate::townsfolk::TOWNSFOLK_COUNT);
        }
    }

    #[test]
    fn small_towns_keep_their_layout_byte_for_byte() {
        // Huellas digitales de las distribuciones generadas antes de que
        // existieran los pueblos grandes. Todas se volvieron a fijar a propósito
        // (2026-10-23): el almacén y la comisaría se reparten por el pueblo en
        // lugar de estar alrededor de la plaza, y el almacén de un pueblo chico
        // es una pulpería. Antes de eso (PR-F1), los árboles de plaza dejaron de
        // estar sobre los senderos de la plaza, así que los lugares del anillo
        // sobre los senderos y al lado de ellos quedan vacíos. Antes, Monte
        // Nievas se volvió a fijar cuando se distribuyó con `police_stations:
        // Some(0)`. Los pueblos con andén se volvieron a fijar otra vez
        // (2026-10-26): la estación tiene dos tableros con su nombre en lugar de
        // uno (`places()` forma parte de la huella digital); los dos pueblos sin
        // tren no cambian. Tres se volvieron a fijar por las veredas de 2,5 m
        // (2026-10-26): la galería del almacén se mantiene sobre la vereda, así
        // que en una calle de más de 10,4 m tiene 2,5 m de profundidad, no 3
        // (Cinco por cinco, Seis por cuatro, Oeste). Los nuevos árboles de
        // sombra no forman parte de la huella digital. Los once se volvieron a
        // fijar para S6c (2026-10-26): las veredas miden 3,5 m y cada calle de
        // ejemplo es 2 m más ancha, así que se movieron todas las cuadrículas y
        // la galería del almacén. Otros dos se volvieron a fijar para S6d (Tres
        // y Este: sus calles de ejemplo de 10 y 11 m ahora miden 12 m, porque
        // todos los pueblos tienen veredas). Nueve se volvieron a fijar para T5b
        // (2026-10-27): todos los andenes crecieron 2,35 m hacia la vía (el
        // borde a 1,65 m de ella, antes 4 m); los dos pueblos sin tren no
        // cambian. Los once se volvieron a fijar cuando la iglesia perdió su
        // cartel (2026-10-28): los carteles de los lugares forman parte de la
        // huella digital.
        let pinned = [
            0x724c_91c9_470f_e41f,
            0xddf4_88db_9aca_9d42,
            0xabf1_d06d_e5fe_009f,
            0x1967_ad10_c15d_9323,
            0x4d89_5840_612f_7b5e,
            0x13b1_8853_df0f_ebac,
            0x365f_dd8c_2dfa_515e,
            0x1aae_2a35_d655_4343,
            0x22e7_02d3_ccc3_bbdf,
            0x4d1c_69a6_fa5a_47b4,
            0x8854_fa05_02dd_27b9,
        ];
        let configs = small_configs();
        assert_eq!(configs.len(), pinned.len());
        for (c, expected) in configs.iter().zip(pinned) {
            let t = TownLayout::generate(c).unwrap();
            assert_eq!(fingerprint(&t), expected, "{} changed", c.name);
        }
    }

    #[test]
    fn a_police_override_of_zero_leaves_the_town_without_a_comisaria() {
        let mut configs = small_configs();
        configs.extend(bigger_towns().into_iter().map(|(c, _)| c));
        for c in configs {
            let none = TownConfig {
                police_stations: Some(0),
                ..c.clone()
            };
            let t = TownLayout::generate(&none).unwrap();
            assert_eq!(count(&t, BuildingKind::PoliceStation), 0, "{}", c.name);
            assert!(
                t.places()
                    .iter()
                    .all(|p| p.kind != crate::PlaceKind::PoliceStation),
                "{}",
                c.name
            );
            assert_eq!(t.police_door(), None, "{}", c.name);
            // Todo lo demás que necesita un pueblo sigue ahí.
            let default = TownLayout::generate(&c).unwrap();
            assert_eq!(count(&t, BuildingKind::Church), 1, "{}", c.name);
            assert_eq!(
                count(&t, BuildingKind::GeneralStore),
                count(&default, BuildingKind::GeneralStore),
                "{}",
                c.name
            );
            assert_eq!(t.plaza, default.plaza, "{}", c.name);
            assert_eq!(t.hitching_post, default.hitching_post, "{}", c.name);
        }
    }

    #[test]
    fn the_police_override_can_lower_but_not_raise_the_tiers_count() {
        for (c, _) in bigger_towns()
            .into_iter()
            .filter(|(c, _)| c.size() == TownSize::VeryBig)
        {
            let one = TownConfig {
                police_stations: Some(1),
                ..c.clone()
            };
            let t = TownLayout::generate(&one).unwrap();
            assert_eq!(count(&t, BuildingKind::PoliceStation), 1, "{}", c.name);
        }
        for c in small_configs() {
            // La cantidad propia de la categoría equivale a no reemplazar nada.
            let same = TownConfig {
                police_stations: Some(1),
                ..c.clone()
            };
            if c.police_stations.is_none() {
                assert_eq!(TownLayout::generate(&same), TownLayout::generate(&c));
            }
            let two = TownConfig {
                police_stations: Some(2),
                ..c.clone()
            };
            let error = TownLayout::generate(&two).unwrap_err();
            assert_eq!(
                error,
                TownConfigError::TooManyPoliceStations { wanted: 2, max: 1 },
                "{}",
                c.name
            );
            assert!(error.to_string().contains("police_stations 2"), "{error}");
        }
    }

    /// Si `plaza` cubre la manzana `(i, j)` del pueblo `c`.
    fn covers(c: &TownConfig, t: &TownLayout, plaza: Aabb, (i, j): (usize, usize)) -> bool {
        let pitch = c.block_size + c.street_width;
        let g = t.grid().min;
        let center = Vec2::new(
            g.x + pitch * i as f32 + c.block_size / 2.0,
            g.y + pitch * j as f32 + c.block_size / 2.0,
        );
        plaza.contains_point(center)
    }

    #[test]
    fn bigger_towns_honor_block_and_facing_station_plazas() {
        let base = |blocks, plaza| TownConfig {
            name: "Grande".into(),
            blocks,
            plaza,
            ..TownConfig::castex()
        };
        for blocks in [(8, 6), (6, 8), (9, 9)] {
            for block in [(0, 0), (blocks.0 - 1, 2), (3, blocks.1 - 1)] {
                let c = base(blocks, PlazaPlacement::Block(block.0, block.1));
                let t = TownLayout::generate(&c).unwrap();
                assert!(covers(&c, &t, t.plaza, block), "{blocks:?} {block:?}");
                let expected = c.size().plaza_span(c.blocks);
                assert_eq!(plaza_span(&c, &t), expected, "{blocks:?} {block:?}");
                assert!(t.grid().contains(&t.plaza), "{blocks:?} {block:?}");
                for b in &t.buildings {
                    assert!(
                        !b.footprint.intersects(&t.plaza.expanded(-0.01)),
                        "{:?}",
                        b.kind
                    );
                }
            }
            // Mirando a la estación: en la fila junto a la vía férrea (al
            // norte), tan cerca del medio como permiten las manzanas enteras.
            let c = base(blocks, PlazaPlacement::FacingStation);
            let t = TownLayout::generate(&c).unwrap();
            assert!(
                covers(&c, &t, t.plaza, (blocks.0 / 2, blocks.1 - 1))
                    || covers(&c, &t, t.plaza, ((blocks.0 - 1) / 2, blocks.1 - 1)),
                "{blocks:?}: {:?} in {:?}",
                t.plaza,
                t.grid()
            );
            let off = t.plaza.center().x - t.grid().center().x;
            let pitch = c.block_size + c.street_width;
            assert!(off.abs() <= pitch, "{blocks:?}: {off}");
            assert_eq!(plaza_span(&c, &t), c.size().plaza_span(c.blocks));
        }
    }

    #[test]
    fn the_plaza_span_never_outgrows_a_narrow_grid() {
        // La cuadrícula más angosta permitida, igual muy grande.
        let narrow = (TownConfig::MIN_BLOCKS, 64 / TownConfig::MIN_BLOCKS + 1);
        let size = TownSize::of(narrow);
        assert_ne!(size, TownSize::Small);
        let (w, h) = size.plaza_span(narrow);
        assert!(w <= narrow.0 && h <= narrow.1);
        // Si la cuadrícula mínima alguna vez se achicara, la extensión igual
        // entra.
        for blocks in [(1, 64), (64, 1), (1, 40), (2, 70)] {
            for size in [TownSize::Small, TownSize::Big, TownSize::VeryBig] {
                let (w, h) = size.plaza_span(blocks);
                assert!(w <= blocks.0 && h <= blocks.1, "{size:?} {blocks:?}");
            }
        }
        let c = TownConfig {
            blocks: narrow,
            ..TownConfig::castex()
        };
        let t = TownLayout::generate(&c).unwrap();
        assert!(t.grid().contains(&t.plaza));
    }

    #[test]
    fn bigger_towns_merge_blocks_into_a_central_plaza() {
        for (c, t) in bigger_towns() {
            let span = plaza_span(&c, &t);
            let expected = match (c.size(), c.blocks.0 >= c.blocks.1) {
                (TownSize::Big, true) => (2, 1),
                (TownSize::Big, false) => (1, 2),
                _ => (2, 2),
            };
            assert_eq!(span, expected, "{}", c.name);
            // Centrada: tan cerca del centro de la cuadrícula como permiten las
            // manzanas enteras.
            let pitch = c.block_size + c.street_width;
            let off = t.plaza.center() - t.grid().center();
            assert!(off.x.abs() <= pitch / 2.0 + 1e-3, "{}: {off:?}", c.name);
            assert!(off.y.abs() <= pitch / 2.0 + 1e-3, "{}: {off:?}", c.name);
            assert!(
                off.x <= 1e-3 && off.y <= 1e-3,
                "{}: ties go west and south",
                c.name
            );
            // Sus propios árboles, repartidos también a lo largo de sus bordes
            // más largos.
            let inner = t.plaza.expanded(-TREE_TRUNK_RADIUS);
            let trees = t.trees.iter().filter(|&&p| inner.contains_point(p)).count();
            assert!(trees >= 8 * span.0.max(span.1), "{}: {trees}", c.name);
        }
    }

    #[test]
    fn no_street_runs_through_a_merged_plaza() {
        for (c, t) in bigger_towns() {
            for street in &t.streets {
                assert!(!street.intersects(&t.plaza), "{}: {street:?}", c.name);
            }
            // Sigue rodeada de calles por todos los lados.
            for side in [Side::North, Side::South, Side::East, Side::West] {
                let n = normal(side);
                let reach = t.plaza.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
                let beside = t.plaza.center() + n * (reach + c.street_width / 2.0);
                assert!(
                    t.streets.iter().any(|s| s.contains_point(beside)),
                    "{}",
                    c.name
                );
            }
            // Los peatones no tienen ninguna intersección dentro de ella ni
            // forma de cruzarla.
            let graph = crate::StreetGraph::from_town(&t);
            let (cols, rows) = c.blocks;
            let (w, h) = plaza_span(&c, &t);
            let inner = (w - 1) * (h - 1);
            assert_eq!(
                graph.nodes.len(),
                (cols + 1) * (rows + 1) - inner,
                "{}",
                c.name
            );
            for (a, links) in graph.links.iter().enumerate() {
                assert!(!t.plaza.contains_point(graph.nodes[a]), "{}", c.name);
                for &b in links {
                    let mid = (graph.nodes[a] + graph.nodes[b]) * 0.5;
                    assert!(!t.plaza.contains_point(mid), "{}: across the plaza", c.name);
                }
            }
            // No hay nada en ella salvo sus árboles, su cartel y su mobiliario.
            let mut sign: Vec<Aabb> = t.sign_obstacles();
            sign.extend(
                t.plaza_furniture()
                    .iter()
                    .flat_map(PlazaFurniture::obstacles),
            );
            for o in t.obstacles() {
                let tree = t.trees.iter().any(|&p| o.contains_point(p))
                    || t.shade_trees.iter().any(|s| o.contains_point(s.at));
                if !tree && !sign.contains(&o) {
                    assert!(!o.intersects(&t.plaza), "{}: {o:?} in the plaza", c.name);
                }
            }
        }
    }

    #[test]
    fn bigger_towns_scale_stores_police_and_crowds() {
        for (c, t) in bigger_towns() {
            let n = c.blocks.0 * c.blocks.1;
            let (stores, police) = match c.size() {
                TownSize::Big => (2, 1),
                _ => (2, 2),
            };
            assert_eq!(
                all(&t, BuildingKind::GeneralStore).len(),
                stores,
                "{}",
                c.name
            );
            assert_eq!(
                all(&t, BuildingKind::PoliceStation).len(),
                police,
                "{}",
                c.name
            );
            let crowd = (0.8 * n as f32).round() as usize;
            assert_eq!(t.townsfolk_count(), crowd.max(16), "{}", c.name);
        }
        let pico = TownLayout::generate(&TownConfig {
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        assert_eq!(pico.size, TownSize::VeryBig);
        assert_eq!(count(&pico, BuildingKind::GeneralStore), 2);
        assert_eq!(pico.townsfolk_count(), 51);
    }

    #[test]
    fn a_townsfolk_override_wins_over_the_size_formula() {
        for (blocks, formula) in [((4, 5), 16), ((8, 8), 51)] {
            let base = TownConfig {
                blocks,
                ..TownConfig::castex()
            };
            assert_eq!(base.townsfolk, None);
            assert_eq!(base.townsfolk_count(), formula);
            for wanted in [0, 10, TownConfig::MAX_TOWNSFOLK] {
                let c = TownConfig {
                    townsfolk: Some(wanted),
                    ..base.clone()
                };
                assert_eq!(c.townsfolk_count(), wanted);
                assert_eq!(TownLayout::generate(&c).unwrap().townsfolk_count(), wanted);
            }
        }
    }

    #[test]
    fn too_many_townsfolk_are_rejected() {
        let wanted = TownConfig::MAX_TOWNSFOLK + 1;
        let error = TownLayout::generate(&TownConfig {
            townsfolk: Some(wanted),
            ..TownConfig::castex()
        })
        .unwrap_err();
        assert_eq!(
            error,
            TownConfigError::TooManyTownsfolk {
                wanted,
                max: TownConfig::MAX_TOWNSFOLK
            }
        );
        assert!(error.to_string().contains("townsfolk 201"), "{error}");
    }

    #[test]
    fn landmarks_of_bigger_towns_face_their_plaza() {
        for (c, t) in bigger_towns() {
            let stores = all(&t, BuildingKind::GeneralStore);
            let (last, central) = stores.split_last().unwrap();
            // Todos los almacenes salvo el último miran hacia la plaza
            // principal, el primero con el caballo atado enfrente.
            for store in central {
                assert_faces(&t, store, t.plaza, c.street_width);
            }
            assert_faces(&t, last, t.station_plaza.unwrap(), c.street_width);
            for police in all(&t, BuildingKind::PoliceStation) {
                assert_faces(&t, police, t.plaza, c.street_width);
            }
            let horse = t.hitching_post.horse.position;
            let gap = |s: &Building| s.footprint.closest_point(horse).distance(horse);
            assert!(
                stores[1..].iter().all(|s| gap(stores[0]) < gap(s)),
                "{}",
                c.name
            );
        }
    }

    #[test]
    fn the_station_plaza_is_half_a_block_facing_the_station() {
        let mut checked = 0;
        for (c, t) in bigger_towns() {
            let station_plaza = t.station_plaza.expect("a station plaza");
            assert!(!station_plaza.intersects(&t.plaza), "{}", c.name);
            assert!(t.grid().contains(&station_plaza), "{}", c.name);
            let inner = station_plaza.expanded(-TREE_TRUNK_RADIUS);
            assert!(
                t.trees.iter().filter(|&&p| inner.contains_point(p)).count() >= 8,
                "{}",
                c.name
            );
            let area = |a: Aabb| a.size().x * a.size().y;
            assert!(area(station_plaza) < area(t.plaza), "{}", c.name);
            let Some(r) = c.railway else {
                // Sin vía férrea: lejos de la plaza principal.
                let pitch = c.block_size + c.street_width;
                let d = station_plaza.center() - t.plaza.center();
                assert!(d.x.abs() + d.y.abs() > 2.0 * pitch, "{}", c.name);
                continue;
            };
            let n = normal(r.side);
            let across = Vec2::new(n.x.abs(), n.y.abs());
            let along = Vec2::new(n.y.abs(), n.x.abs());
            // Media manzana de profundidad, una manzana entera de largo.
            assert!((station_plaza.size().dot(across) - c.block_size / 2.0).abs() < 1e-3);
            assert!((station_plaza.size().dot(along) - c.block_size).abs() < 1e-3);
            // La mitad del lado de la vía férrea de una manzana de la fila de
            // la vía férrea.
            let edge = |b: Aabb| b.center().dot(n) + b.size().dot(across) / 2.0;
            assert!(
                (edge(t.grid()) - edge(station_plaza) - c.street_width).abs() < 1e-3,
                "{}: station plaza not against the railway-side street",
                c.name
            );
            // A manzana y media o menos de la estación a lo largo de la vía.
            let station = find(&t, BuildingKind::Station).unwrap().footprint.center();
            let pitch = c.block_size + c.street_width;
            let distance = (station_plaza.center() - station).dot(along).abs();
            assert!(distance <= 1.5 * pitch + 1e-3, "{}: {distance}", c.name);
            checked += 1;
        }
        assert!(checked >= 3, "{checked} bigger railway towns");
    }

    #[test]
    fn the_rest_of_the_station_plaza_block_keeps_houses() {
        for (c, t) in bigger_towns() {
            let half = t.station_plaza.unwrap();
            // La otra mitad: el lado de la plaza de la estación sin calle.
            let blocks: Vec<Aabb> = [Side::North, Side::South, Side::East, Side::West]
                .into_iter()
                .map(|side| {
                    let n = normal(side);
                    let grow = n * half.size().dot(Vec2::new(n.x.abs(), n.y.abs()));
                    Aabb::new(
                        Vec2::new(
                            half.min.x.min(half.min.x + grow.x),
                            half.min.y.min(half.min.y + grow.y),
                        ),
                        Vec2::new(
                            half.max.x.max(half.max.x + grow.x),
                            half.max.y.max(half.max.y + grow.y),
                        ),
                    )
                })
                .filter(|b| t.streets.iter().all(|s| !s.intersects(&b.expanded(-0.01))))
                .collect();
            let [block] = blocks[..] else {
                panic!("{}: {blocks:?}", c.name);
            };
            let lots = t
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, BuildingKind::House | BuildingKind::Shed))
                .filter(|b| block.contains(&b.footprint))
                .count();
            assert!(lots >= 1, "{}: no houses beside the station plaza", c.name);
        }
    }

    #[test]
    fn errors_explain_themselves() {
        let error = TownConfigError::BlockTooSmall {
            block_size: 20.0,
            min: 30.0,
        };
        assert!(error.to_string().contains("block_size 20 m"), "{error}");
        let error = TownConfigError::NoCentralBlock { blocks: (4, 5) };
        assert!(error.to_string().contains("4x5"), "{error}");
    }

    /// El pueblo de ejemplo muy grande que creció a ambos lados de su línea.
    fn partida() -> (TownConfig, TownLayout) {
        let c = sample_configs()
            .into_iter()
            .find(|c| c.name == "Partida")
            .unwrap();
        let t = TownLayout::generate(&c).unwrap();
        (c, t)
    }

    #[test]
    fn a_through_line_splits_the_town_with_blocks_and_crossings_on_both_sides() {
        let (_, t) = partida();
        let band = t.rail_band().expect("a band");
        let rail = t.railway.unwrap();
        assert!(band.min.y < rail.start.y && rail.start.y < band.max.y);
        assert_eq!(rail.start.y, rail.end.y);
        // La cuadrícula contiene la franja: su extensión es la propia
        // cuadrícula.
        assert_eq!(t.extent(), t.grid());
        assert!(t.grid().contains(&band));
        // Calles perimetrales a ambos lados de la franja, a lo largo de todo su
        // largo.
        for y in [band.min.y, band.max.y] {
            assert!(
                t.streets.iter().any(|s| s.size().x >= band.size().x - 1e-3
                    && (s.max.y - y).abs() < 1e-3
                    || (s.min.y - y).abs() < 1e-3 && s.size().x >= band.size().x - 1e-3),
                "no ring street at {y}"
            );
        }
        // Las calles que cruzan atraviesan la franja en perpendicular, lejos de
        // la playa; las demás se detienen en sus bordes.
        let crossings = t.crossing_streets();
        assert!(crossings.len() >= 3 && crossings.len() < 9, "{crossings:?}");
        for (a, b) in &crossings {
            assert_eq!(a.x, b.x);
            assert!(a.y < band.min.y && b.y > band.max.y);
        }
        for s in t.streets.iter().filter(|s| s.size().y > s.size().x) {
            let inside = s.min.y < band.max.y && s.max.y > band.min.y;
            let across = s.min.y < band.min.y && s.max.y > band.max.y;
            assert!(!inside || across, "{s:?} ends in the band");
        }
        for b in t.buildings.iter().filter(|b| band.contains(&b.footprint)) {
            for s in &t.streets {
                assert!(!s.intersects(&b.footprint), "{s:?} through {b:?}");
            }
        }
        // La estación mira hacia su mitad del pueblo del otro lado de la calle
        // perimetral, con el andén entre ella y la vía.
        let station = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .unwrap();
        assert_eq!(station.front, Side::North);
        let canopy = t.canopies()[0];
        assert!(canopy.footprint.max.y <= station.footprint.min.y + 1e-3);
        // Su plaza de 2 x 2 queda entera en la mitad de la estación, al
        // noroeste de la estación, sin ninguna calle que la atraviese; la plaza
        // de la estación es la media manzana del otro lado de la calle
        // perimetral frente a la estación.
        assert!(t.plaza.min.y > band.max.y);
        let to_plaza = t.plaza.center() - station.footprint.center();
        assert!(to_plaza.x < 0.0 && to_plaza.y > 0.0, "{to_plaza:?}");
        for street in &t.streets {
            assert!(!street.intersects(&t.plaza), "{street:?}");
        }
        let station_plaza = t.station_plaza.expect("a station plaza");
        let ring = t
            .streets
            .iter()
            .find(|s| s.size().x > s.size().y && (s.min.y - band.max.y).abs() < 1e-3)
            .unwrap();
        assert!(
            (station_plaza.min.y - ring.max.y).abs() < 1e-3,
            "{station_plaza:?}"
        );
        assert!((station_plaza.center().x - station.footprint.center().x).abs() < 50.0);
    }

    #[test]
    fn a_through_line_needs_room_and_an_east_west_track() {
        let (c, _) = partida();
        let rail = c.railway.unwrap();
        let east = TownConfig {
            railway: Some(RailwayConfig {
                side: Side::East,
                ..rail
            }),
            ..c.clone()
        };
        assert_eq!(
            TownLayout::generate(&east),
            Err(TownConfigError::ThroughNotEastWest { side: Side::East })
        );
        let low = TownConfig {
            blocks: (6, 3),
            ..c.clone()
        };
        assert!(matches!(
            TownLayout::generate(&low),
            Err(TownConfigError::ThroughTooFewRows { rows: 3, min: 4 })
        ));
        // Una plaza elegida del otro lado de la vía se rechaza; la de su propia
        // mitad está bien.
        let across = TownConfig {
            plaza: PlazaPlacement::Block(2, 3),
            ..c.clone()
        };
        assert_eq!(
            TownLayout::generate(&across),
            Err(TownConfigError::PlazaAcrossRailway)
        );
        let south = TownConfig {
            plaza: PlazaPlacement::Block(2, 1),
            ..c.clone()
        };
        assert!(
            TownLayout::generate(&south).unwrap().plaza.max.y < rail.offset.max(0.0) + c.origin.y
        );
        for e in [
            TownConfigError::ThroughNotEastWest { side: Side::West },
            TownConfigError::ThroughTooFewRows { rows: 3, min: 4 },
            TownConfigError::PlazaAcrossRailway,
            TownConfigError::BadDensity { density: 2.0 },
        ] {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn density_thins_the_lots_and_must_be_a_share() {
        let base = TownConfig::castex();
        let count = |c: &TownConfig| {
            TownLayout::generate(c)
                .unwrap()
                .buildings
                .iter()
                .filter(|b| matches!(b.kind, BuildingKind::House | BuildingKind::Shed))
                .count()
        };
        let full = count(&base);
        let thin = count(&TownConfig {
            density: Some(0.3),
            ..base.clone()
        });
        assert!(thin * 2 < full, "{thin} of {full}");
        let houses = |c: &TownConfig| {
            TownLayout::generate(c)
                .unwrap()
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .count()
        };
        let none = houses(&TownConfig {
            density: Some(0.0),
            ..base.clone()
        });
        assert_eq!(none, 0);
        for density in [-0.1, 1.5, f32::NAN] {
            assert!(matches!(
                TownLayout::generate(&TownConfig {
                    density: Some(density),
                    ..base.clone()
                }),
                Err(TownConfigError::BadDensity { .. })
            ));
        }
    }

    /// Los edificios que tienen un palenque además del palenque del almacén.
    fn wants_palenque(t: &TownLayout, k: usize) -> bool {
        let first_store = t
            .buildings
            .iter()
            .position(|b| b.kind == BuildingKind::GeneralStore);
        matches!(
            t.buildings[k].kind,
            BuildingKind::GeneralStore
                | BuildingKind::PoliceStation
                | BuildingKind::Church
                | BuildingKind::Station
        ) && Some(k) != first_store
    }

    /// Todos los pueblos parecidos a los distribuidos, con los de línea que los
    /// atraviesa.
    fn towns_with_names() -> Vec<(String, TownLayout)> {
        sample_towns()
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect()
    }

    #[test]
    fn every_landmark_but_the_first_store_has_a_palenque_before_it() {
        for (name, t) in towns_with_names() {
            let mut wanted = 0;
            for k in (0..t.buildings.len()).filter(|&k| wants_palenque(&t, k)) {
                wanted += 1;
                let b = &t.buildings[k];
                let p = t
                    .palenques
                    .iter()
                    .find(|p| p.building == k)
                    .unwrap_or_else(|| panic!("{name}: no palenque before {:?}", b.kind));
                // Delante de la fachada, del lado de la calle, a lo largo de la
                // calle.
                let n = normal(b.front);
                let ahead = (p.rail.center() - b.footprint.center()).dot(n);
                let half = b.footprint.size() * 0.5;
                assert!(ahead > n.x.abs() * half.x + n.y.abs() * half.y, "{name}");
                let size = p.rail.size();
                let long = if n.x.abs() > 0.5 { size.y } else { size.x };
                assert!((long - Palenque::LENGTH).abs() < 1e-4, "{name}");
            }
            assert_eq!(t.palenques.len(), wanted, "{name}: one per landmark");
            // Nunca uno frente al almacén que ya tiene su palenque.
            assert!(t.palenques.iter().all(|p| wants_palenque(&t, p.building)));
        }
    }

    #[test]
    fn only_cities_tie_horses_to_their_palenques_and_only_a_few() {
        let mut cities = 0;
        for (name, t) in towns_with_names() {
            let horses = t.palenques.iter().filter(|p| p.horse.is_some()).count();
            if t.size == TownSize::Small {
                assert_eq!(horses, 0, "{name}: a town's palenques stay empty");
            } else {
                cities += 1;
                let posts = t.palenques.len();
                let share = Palenque::horses_for(posts);
                assert!(horses <= share, "{name}");
                // Donde las calles son lo bastante anchas para dejarle lugar a
                // un caballo.
                let narrowest = t
                    .streets
                    .iter()
                    .map(|s| s.size().x.min(s.size().y))
                    .fold(f32::INFINITY, f32::min);
                if narrowest >= 12.0 {
                    assert_eq!(horses, share, "{name}");
                }
                assert!(horses >= 1, "{name}");
            }
        }
        assert!(cities >= 2);
    }

    #[test]
    fn a_city_ties_half_its_palenques_to_a_horse_up_to_a_cap() {
        let tied: Vec<usize> = [0, 1, 2, 3, 4, 5, 6, 7, 8, 20]
            .into_iter()
            .map(Palenque::horses_for)
            .collect();
        assert_eq!(tied, [0, 0, 1, 1, 2, 2, 3, 3, 3, 3]);
        assert_eq!(Palenque::MAX_HORSES, 3);
    }

    #[test]
    fn the_seed_picks_which_palenques_tie_a_horse() {
        let pico = |seed| {
            TownLayout::generate(&TownConfig {
                name: "Pico".into(),
                blocks: (8, 8),
                seed,
                ..TownConfig::castex()
            })
            .unwrap()
        };
        let tied = |t: &TownLayout| -> Vec<bool> {
            t.palenques.iter().map(|p| p.horse.is_some()).collect()
        };
        assert_eq!(tied(&pico(5)), tied(&pico(5)));
        assert!(
            (0..8).any(|seed| tied(&pico(seed)) != tied(&pico(seed + 1))),
            "eight neighboring seeds all tie the same posts"
        );
        for seed in 0..8 {
            let t = pico(seed);
            let count = t.palenques.iter().filter(|p| p.horse.is_some()).count();
            assert!((1..=Palenque::MAX_HORSES).contains(&count), "seed {seed}");
        }
    }

    #[test]
    fn a_palenque_tethers_its_horse_alongside_without_touching_the_rail() {
        let mut seen = 0;
        for (name, t) in towns_with_names() {
            for p in t.palenques.iter().filter(|p| p.horse.is_some()) {
                let horse = p.horse.unwrap();
                let n = normal(t.buildings[p.building].front);
                assert!(!horse.footprint.intersects(&p.rail), "{name}");
                assert_eq!(horse.position, horse.footprint.center(), "{name}");
                // A lo largo de la calle, del lado de la calle respecto del
                // palenque.
                assert!(horse.heading.forward().dot(n).abs() < 1e-5, "{name}");
                assert!((horse.footprint.center() - p.rail.center()).dot(n) > 0.0);
                seen += 1;
            }
        }
        assert!(seen > 0);
    }

    #[test]
    fn rails_and_horses_of_the_palenques_are_obstacles() {
        assert!(
            towns_with_names()
                .iter()
                .any(|(_, t)| !t.palenques.is_empty())
        );
        for (name, t) in towns_with_names() {
            let obstacles = t.obstacles();
            let horses = t.palenques.iter().filter(|p| p.horse.is_some()).count();
            let furniture: usize = t
                .plaza_furniture()
                .iter()
                .map(|f| f.obstacles().count())
                .sum();
            assert_eq!(
                obstacles.len(),
                t.buildings
                    .iter()
                    .filter(|b| b.kind != BuildingKind::Platform)
                    .count()
                    + t.trees.len()
                    + t.shade_trees.len()
                    + 2
                    + t.palenques.len()
                    + horses
                    + t.sign_obstacles().len()
                    + furniture,
                "{name}"
            );
            for p in &t.palenques {
                assert!(p.obstacles().all(|o| obstacles.contains(&o)), "{name}");
            }
        }
    }

    #[test]
    fn palenques_stand_clear_of_everything_else() {
        let placed: usize = towns_with_names()
            .iter()
            .map(|(_, t)| t.palenques.len())
            .sum();
        assert!(placed > 40, "{placed} palenques in the sample towns");
        for (name, t) in towns_with_names() {
            let walks: Vec<Aabb> = t.sidewalks().iter().map(|s| s.area).collect();
            let obstacles = t.obstacles();
            for (i, p) in t.palenques.iter().enumerate() {
                let spots: Vec<Aabb> = p.obstacles().collect();
                // Sobre una calle, fuera del carril de los peatones por su
                // medio (el de la estación, sobre la playa que tiene enfrente).
                if t.buildings[p.building].kind == BuildingKind::Station {
                    assert!(spots.iter().all(|o| t.extent().contains(o)), "{name}");
                } else {
                    let street = t
                        .streets
                        .iter()
                        .find(|s| spots.iter().all(|o| s.contains(o)))
                        .unwrap_or_else(|| panic!("{name}: palenque {i} leaves the street"));
                    let across = if street.size().x > street.size().y {
                        Vec2::new(0.0, 1.0)
                    } else {
                        Vec2::new(1.0, 0.0)
                    };
                    let middle = street.center().dot(across);
                    for o in &spots {
                        let (lo, hi) = (o.min.dot(across) - middle, o.max.dot(across) - middle);
                        assert!(
                            lo * hi > 0.0 && lo.abs().min(hi.abs()) >= Palenque::LANE,
                            "{name}: palenque {i} in the walkers' lane"
                        );
                    }
                }
                for o in &spots {
                    // Fuera de las veredas.
                    assert!(walks.iter().all(|w| !overlap(w, o)), "{name}: {i}");
                    // Y lejos de todo otro obstáculo, con lugar para rodearlo
                    // caminando.
                    let reach = o.expanded(Palenque::CLEARANCE - 0.01);
                    for other in &obstacles {
                        if spots.contains(other) {
                            continue;
                        }
                        assert!(!overlap(&reach, other), "{name}: palenque {i} by {other:?}");
                    }
                }
                // Lejos de las puertas, las plazas, la línea y los cruces de calles.
                for (k, other) in t.buildings.iter().enumerate() {
                    if other.kind == BuildingKind::Platform {
                        continue;
                    }
                    let m = normal(other.front);
                    let h = other.footprint.size() * 0.5;
                    let facade = other.footprint.center() + m * (m.x.abs() * h.x + m.y.abs() * h.y);
                    let side = Vec2::new(-m.y, m.x);
                    let (a, z) = (facade + side * 1.8, facade + m * 4.0 - side * 1.8);
                    let door = Aabb::new(
                        Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                        Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                    );
                    assert!(spots.iter().all(|o| !overlap(o, &door)), "{name}: {k} {i}");
                }
                for z in t.plazas().chain(t.floors().iter().map(|f| f.footprint)) {
                    assert!(spots.iter().all(|o| !overlap(o, &z)), "{name}: {i}");
                }
                // La propia estación está en la franja de una línea que
                // atraviesa el pueblo.
                let band = t
                    .rail_band()
                    .filter(|_| t.buildings[p.building].kind != BuildingKind::Station);
                for z in t.rail_corridor().into_iter().chain(band) {
                    assert!(spots.iter().all(|o| !overlap(o, &z)), "{name}: {i}");
                }
                for (x, y) in t
                    .streets
                    .iter()
                    .filter(|s| s.size().y > s.size().x)
                    .flat_map(|x| {
                        t.streets
                            .iter()
                            .filter(|s| s.size().x > s.size().y)
                            .map(move |y| (x, y))
                    })
                    .filter(|(x, y)| overlap(x, y))
                {
                    let crossing =
                        Aabb::new(Vec2::new(x.min.x, y.min.y), Vec2::new(x.max.x, y.max.y))
                            .expanded(Palenque::CROSSING_MARGIN - 0.01);
                    assert!(spots.iter().all(|o| !overlap(o, &crossing)), "{name}: {i}");
                }
            }
            // Y separados entre sí.
            for (i, a) in t.palenques.iter().enumerate() {
                for b in &t.palenques[i + 1..] {
                    assert!(
                        a.obstacles()
                            .all(|x| b.obstacles().all(|y| !overlap(&x, &y)))
                    );
                }
            }
        }
    }

    #[test]
    fn a_palenque_spot_needs_a_street_room_to_pass_and_free_ground() {
        let t = TownLayout::castex_inspired();
        let post = t
            .palenques
            .iter()
            .find(|p| t.buildings[p.building].kind == BuildingKind::Church)
            .unwrap();
        let spot = post.rail;
        let kind = BuildingKind::Church;
        let beside = |gap: f32| {
            let size = spot.size();
            let shift = Vec2::new(size.x + gap, 0.0);
            Aabb::new(spot.min + shift, spot.max + shift)
        };
        assert!(t.palenque_fits(kind, &spot, &[], &[]));
        // Otro obstáculo dentro del despeje lo bloquea; uno justo más allá, no.
        let near = beside(Palenque::CLEARANCE - 0.1);
        let far = beside(Palenque::CLEARANCE + 0.1);
        assert!(!t.palenque_fits(kind, &spot, &[near], &[]));
        assert!(t.palenque_fits(kind, &spot, &[far], &[]));
        // Un área de zona prohibida lo bloquea solo cuando se superpone.
        assert!(!t.palenque_fits(kind, &spot, &[], &[spot.expanded(-0.01)]));
        assert!(t.palenque_fits(kind, &spot, &[], &[beside(0.0)]));
        // Fuera de toda calle, o en el carril del medio, no entra.
        let off = Aabb::new(
            spot.min + Vec2::new(0.0, 15.0),
            spot.max + Vec2::new(0.0, 15.0),
        );
        assert!(!t.palenque_fits(kind, &off, &[], &[]));
        let street = t.streets.iter().find(|s| s.contains(&spot)).unwrap();
        let lane = Aabb::from_center_size(street.center(), spot.size());
        assert!(!t.palenque_fits(kind, &lane, &[], &[]));
    }

    fn pico_with_streets(street_width: f32) -> TownLayout {
        TownLayout::generate(&TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            street_width,
            ..TownConfig::castex()
        })
        .unwrap()
    }

    #[test]
    fn sidewalks_are_three_and_a_half_meters_and_the_galeria_stays_over_them() {
        assert!((Sidewalk::WIDTH - 3.5).abs() < 1e-6);
        // La galería del almacén es tan profunda como permite la calle, nunca más
        // allá de la vereda.
        for (street, veranda) in [
            (10.0, 2.3),
            (11.0, 2.8),
            (12.0, 3.3),
            (14.0, 3.5),
            (16.0, 3.5),
        ] {
            let t = pico_with_streets(street);
            assert!(
                (t.hitching_post.veranda - veranda).abs() < 1e-4,
                "{street} m street: veranda {}",
                t.hitching_post.veranda
            );
            for walk in t.sidewalks() {
                let a = walk.area.size();
                assert!((a.x.min(a.y) - 3.5).abs() < 1e-3, "{street} m street");
            }
        }
    }

    #[test]
    fn a_tied_horse_needs_a_twelve_meter_street_and_the_rails_still_fit_on_narrow_ones() {
        for street in [10.0, 11.0, 12.0, 14.0] {
            let t = pico_with_streets(street);
            assert!(
                t.palenques.len() >= 4,
                "{street} m: {} posts",
                t.palenques.len()
            );
            // El palenque de la estación está sobre su playa, no sobre una
            // calle.
            let on_streets: Vec<&Palenque> = t
                .palenques
                .iter()
                .filter(|p| t.buildings[p.building].kind != BuildingKind::Station)
                .collect();
            let horses: Vec<_> = on_streets.iter().filter_map(|p| p.horse).collect();
            if street < 12.0 {
                assert!(
                    horses.is_empty(),
                    "{street} m street has no room for a horse"
                );
            } else {
                assert!(!horses.is_empty(), "{street} m street should tie a horse");
            }
            // Más allá de la vereda de 3,5 m, con el carril del medio (1 m hacia cada
            // lado) libre.
            for p in on_streets {
                let street_strip = t
                    .streets
                    .iter()
                    .find(|s| s.contains(&p.rail))
                    .unwrap_or_else(|| panic!("{street} m: rail off the street"));
                let across = if street_strip.size().x > street_strip.size().y {
                    Vec2::new(0.0, 1.0)
                } else {
                    Vec2::new(1.0, 0.0)
                };
                let middle = street_strip.center().dot(across);
                for o in p.obstacles() {
                    let near = (o.min.dot(across) - middle)
                        .abs()
                        .min((o.max.dot(across) - middle).abs());
                    assert!(near >= 1.0 - 1e-4, "{street} m: {near} m from the middle");
                    assert!(
                        t.sidewalks().iter().all(|w| !overlap(&w.area, &o)),
                        "{street} m: on the walk"
                    );
                }
            }
        }
    }

    fn trees_of(t: &TownLayout, kind: ShadeKind) -> Vec<ShadeTree> {
        t.shade_trees
            .iter()
            .copied()
            .filter(|s| s.kind == kind)
            .collect()
    }

    #[test]
    fn only_cities_get_street_trees_and_groves_and_only_towns_front_trees() {
        let (mut cities, mut towns) = (0, 0);
        for (name, t) in towns_with_names() {
            let (pits, groves) = (trees_of(&t, ShadeKind::Pit), trees_of(&t, ShadeKind::Grove));
            let fronts = trees_of(&t, ShadeKind::Front);
            if t.size == TownSize::Small {
                towns += 1;
                assert!(pits.is_empty() && groves.is_empty(), "{name}");
                assert!(!fronts.is_empty(), "{name}: no tree in front of any house");
            } else {
                cities += 1;
                assert!(fronts.is_empty(), "{name}");
                assert!(pits.len() > 100 && groves.len() >= 4, "{name}");
            }
            // Patios en todas partes, pero solo unos pocos en un pueblito.
            assert!(!trees_of(&t, ShadeKind::Yard).is_empty(), "{name}");
        }
        assert!(cities >= 4 && towns >= 6);
    }

    #[test]
    fn shade_trees_are_seeded_and_the_seed_moves_them() {
        let pico = |seed| {
            TownLayout::generate(&TownConfig {
                name: "Pico".into(),
                blocks: (8, 8),
                seed,
                ..TownConfig::castex()
            })
            .unwrap()
        };
        assert_eq!(pico(7).shade_trees, pico(7).shade_trees);
        // Las mismas veredas, otra semilla: el primer árbol de cada fila
        // empieza en otro punto a lo largo de ella.
        let first = |t: &TownLayout, row: u32| {
            t.shade_trees
                .iter()
                .find(|s| s.kind == ShadeKind::Pit && s.row == row)
                .map(|s| s.at.x + s.at.y)
        };
        let (a, b) = (pico(3), pico(4));
        assert_eq!(a.sidewalks(), b.sidewalks());
        let rows = a.sidewalks().len() as u32;
        let moved = (0..rows)
            .filter(|&r| match (first(&a, r), first(&b, r)) {
                (Some(x), Some(y)) => (x - y).abs() > 0.5,
                _ => false,
            })
            .count();
        assert!(moved * 2 > rows as usize, "{moved} of {rows} rows moved");
    }

    #[test]
    fn street_trees_stand_in_their_pits_forty_centimeters_from_the_curb() {
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size != TownSize::Small)
        {
            for tree in trees_of(&t, ShadeKind::Pit) {
                let walk = &t.sidewalks()[tree.row as usize];
                let curb = walk.curbs[0];
                let n = normal(curb);
                // El borde exterior (el del cordón) de la franja a lo largo de
                // su normal.
                let edge = match curb {
                    Side::North => walk.area.max.y,
                    Side::South => walk.area.min.y,
                    Side::East => walk.area.max.x,
                    Side::West => walk.area.min.x,
                };
                let on_edge = if n.x.abs() > 0.5 {
                    Vec2::new(edge, tree.at.y)
                } else {
                    Vec2::new(tree.at.x, edge)
                };
                let gap = on_edge.distance(tree.at);
                // Una cazuela de 0,8 m, con su borde a 0,4 m hacia adentro del
                // cordón.
                assert!((gap - 0.8).abs() < 1e-3, "{name}: pit center {gap} m in");
                assert!(walk.area.contains_point(tree.at), "{name}");
                assert_eq!(tree.toward, n, "{name}: leans over the street");
                assert!((tree.crown - 2.3).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn a_street_row_keeps_fifteen_to_eighteen_meters_between_its_trees() {
        let mut gaps = Vec::new();
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size != TownSize::Small)
        {
            for row in 0..t.sidewalks().len() {
                let mut at: Vec<Vec2> = trees_of(&t, ShadeKind::Pit)
                    .into_iter()
                    .filter(|s| s.row as usize == row)
                    .map(|s| s.at)
                    .collect();
                at.sort_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)));
                for pair in at.windows(2) {
                    let gap = pair[0].distance(pair[1]);
                    assert!(gap >= 14.8 - 1e-3, "{name}: row {row}, {gap} m apart");
                    gaps.push(gap);
                }
            }
        }
        assert!(gaps.len() > 250, "{} gaps", gaps.len());
        // La mayoría están a los 14,8 a 18,4 m planificados; el resto saltean
        // una puerta o un farol.
        let planned = gaps.iter().filter(|&&g| g < 24.0).count();
        assert!(planned * 10 > gaps.len() * 8, "{planned} of {}", gaps.len());
    }

    #[test]
    fn the_sample_cities_got_a_tenth_fewer_street_trees_and_the_towns_half_as_many_front_ones_again()
     {
        // Conteos antes de la segunda prueba de juego del usuario (2026-10-26),
        // con los anchos de calle del 2026-10-26: 2247 árboles de calle en las
        // ciudades de estos ejemplos; antes de la primera prueba de juego, 119
        // árboles de frente en los pueblos (con los anchos anteriores); 189
        // árboles de frente antes del 15 % más.
        let (mut pits, mut fronts) = (0, 0);
        for (_, t) in towns_with_names() {
            pits += trees_of(&t, ShadeKind::Pit).len();
            fronts += trees_of(&t, ShadeKind::Front).len();
        }
        // Después, un 15 % más (2026-10-27): 2039 antes, así que de 1,12 a 1,18
        // veces.
        assert!(
            (2039 * 112 / 100..=2039 * 118 / 100).contains(&pits),
            "{pits} street trees"
        );
        assert!(
            (189 * 112 / 100..=189 * 123 / 100).contains(&fronts),
            "{fronts} front trees"
        );
    }

    /// Distancia desde `p` hasta el más cercano de los `boxes`.
    fn gap_to(boxes: impl IntoIterator<Item = Aabb>, p: Vec2) -> f32 {
        boxes
            .into_iter()
            .map(|b| b.closest_point(p).distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn shade_trees_keep_a_crown_off_every_building() {
        let mut checked = 0;
        for (name, t) in towns_with_names() {
            let walls: Vec<Aabb> = t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect();
            for tree in &t.shade_trees {
                let gap = gap_to(walls.iter().copied(), tree.at);
                // Separaciones literales: un árbol de cazuela queda a 2,7 m de
                // su fachada (la vereda de 3,5 m menos la cazuela), uno de
                // frente a 2,2 m y uno de patio a 3,6 m.
                let least = match tree.kind {
                    ShadeKind::Pit => 2.65,
                    ShadeKind::Front => 2.15,
                    ShadeKind::Yard => 3.5,
                    ShadeKind::Grove => 4.0,
                };
                assert!(gap >= least, "{name}: {:?} {gap} m off a wall", tree.kind);
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    #[test]
    fn shade_trees_stand_clear_of_doors_galerias_crossings_palenques_and_each_other() {
        for (name, t) in towns_with_names() {
            let trunks: Vec<Vec2> = t
                .trees
                .iter()
                .copied()
                .chain(t.shade_trees.iter().map(|s| s.at))
                .collect();
            for (k, tree) in t.shade_trees.iter().enumerate() {
                for (j, other) in trunks.iter().enumerate() {
                    if j != t.trees.len() + k {
                        assert!(
                            tree.at.distance(*other) >= 3.99,
                            "{name}: trunks {k} and {j}"
                        );
                    }
                }
                // Más allá de los cruces de los faroles de esquina.
                for (x, y) in t
                    .streets
                    .iter()
                    .filter(|s| s.size().y > s.size().x)
                    .flat_map(|x| {
                        t.streets
                            .iter()
                            .filter(|y| y.size().x > y.size().y)
                            .map(move |y| (x, y))
                    })
                    .filter(|(x, y)| x.intersects(y))
                {
                    let crossing =
                        Aabb::new(Vec2::new(x.min.x, y.min.y), Vec2::new(x.max.x, y.max.y));
                    assert!(
                        crossing.closest_point(tree.at).distance(tree.at) >= 1.9
                            || !on_street(&t, tree.at),
                        "{name}: tree {k} at a crossing"
                    );
                }
                for b in t
                    .buildings
                    .iter()
                    .filter(|b| b.kind != BuildingKind::Platform)
                {
                    let m = normal(b.front);
                    let h = b.footprint.size() * 0.5;
                    let facade = b.footprint.center() + m * (m.x.abs() * h.x + m.y.abs() * h.y);
                    let side = Vec2::new(-m.y, m.x);
                    let (a, z) = (facade + side * 2.5, facade + m * 4.0 - side * 2.5);
                    let door = Aabb::new(
                        Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                        Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                    );
                    assert!(!door.contains_point(tree.at), "{name}: tree {k} at a door");
                    if b.kind == BuildingKind::GeneralStore {
                        let wide = h.dot(Vec2::new(side.x.abs(), side.y.abs())) + 1.4;
                        let (a, z) = (facade + side * wide, facade + m * 3.0 - side * wide);
                        let gallery = Aabb::new(
                            Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                            Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                        );
                        assert!(
                            !gallery.contains_point(tree.at),
                            "{name}: tree {k} at a galería"
                        );
                    }
                }
                // Lejos de todo palenque, su caballo, el palenque del almacén y
                // todo cartel.
                let posts = t
                    .palenques
                    .iter()
                    .flat_map(Palenque::obstacles)
                    .chain([t.hitching_post.rail, t.hitching_post.horse.footprint])
                    .chain(t.sign_obstacles());
                assert!(gap_to(posts, tree.at) >= 1.3, "{name}: tree {k} by a post");
            }
        }
    }

    #[test]
    fn groves_stand_between_the_plaza_paths_and_yard_trees_beside_the_landmarks() {
        let mut groves = 0;
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size != TownSize::Small)
        {
            for tree in trees_of(&t, ShadeKind::Grove) {
                let plaza = t
                    .plazas()
                    .find(|p| p.contains_point(tree.at))
                    .unwrap_or_else(|| panic!("{name}: a grove tree off the plazas"));
                for keep_off in plaza_paths(plaza)
                    .into_iter()
                    .chain([plaza_monument(plaza)])
                {
                    assert!(
                        keep_off.closest_point(tree.at).distance(tree.at) >= 1.8,
                        "{name}: a grove tree on a path"
                    );
                }
                // En el medio del verde, no solo a lo largo de su borde.
                groves += usize::from(plaza.expanded(-10.0).contains_point(tree.at));
            }
            // Un árbol a 8 m o menos de cada iglesia, comisaría y estación (la
            // estación de una línea que atraviesa el pueblo está en su franja
            // de playa, sin lugar).
            let yards = trees_of(&t, ShadeKind::Yard);
            for b in t.buildings.iter().filter(|b| {
                matches!(b.kind, BuildingKind::Church | BuildingKind::PoliceStation)
                    || (b.kind == BuildingKind::Station && t.rail_band().is_none())
            }) {
                let near = yards
                    .iter()
                    .any(|y| b.footprint.closest_point(y.at).distance(y.at) <= 8.0);
                assert!(near, "{name}: no tree by the {:?}", b.kind);
            }
        }
        assert!(
            groves > 20,
            "{groves} grove trees 10 m or more in from a plaza's edge"
        );
    }

    #[test]
    fn front_trees_are_few_and_far_apart_on_the_dirt_beside_the_lane() {
        let mut seen = 0;
        for (name, t) in towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size == TownSize::Small)
        {
            let fronts = trees_of(&t, ShadeKind::Front);
            let houses = t
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .count();
            // Escasos: no más de una casa de cada dos, nunca dos a menos de
            // 13 m, nunca más de otros dos a menos de 30 m: ninguna fila continua.
            assert!(
                fronts.len() * 2 <= houses,
                "{name}: {} of {houses}",
                fronts.len()
            );
            for (k, a) in fronts.iter().enumerate() {
                let run = fronts.iter().filter(|b| a.at.distance(b.at) < 30.0).count() - 1;
                assert!(run <= 2, "{name}: a row of {} trees", run + 1);
                for b in &fronts[k + 1..] {
                    assert!(a.at.distance(b.at) >= 12.9, "{name}: trees too close");
                }
                // Sobre una calle, fuera del carril de sus peatones, sobre una
                // vereda de tierra apisonada (sin cazuela) y al menos a un metro
                // hacia adentro de su cordón.
                assert!(on_street(&t, a.at), "{name}");
                let on: Vec<&Sidewalk> = t
                    .sidewalks()
                    .iter()
                    .filter(|w| w.area.contains_point(a.at))
                    .collect();
                assert!(!on.is_empty(), "{name}: a front tree off the walks");
                assert!(on.iter().all(|w| w.walk == Walk::Earth), "{name}: on brick");
                // Hacia adentro del cordón a lo largo de la franja sobre la que
                // está (un árbol junto a una esquina también queda sobre la
                // franja transversal, cuyo cordón está cerca).
                let curb = on
                    .iter()
                    .map(|w| match w.curbs[0] {
                        Side::North => w.area.max.y - a.at.y,
                        Side::South => a.at.y - w.area.min.y,
                        Side::East => w.area.max.x - a.at.x,
                        Side::West => a.at.x - w.area.min.x,
                    })
                    .fold(f32::MIN, f32::max);
                assert!(curb >= 1.0, "{name}: {curb} m from the curb");
                assert!(a.crown <= 1.9 + 1e-6 && a.toward.length() > 0.99, "{name}");
                seen += 1;
            }
        }
        assert!(seen > 50, "{seen}");
    }

    #[test]
    fn yards_have_thirty_percent_more_trees_and_shrubs_and_flower_beds_besides() {
        // Árboles de patio antes de la prueba de juego (2026-10-26): 99 en los
        // pueblos de ejemplo, 648 en las ciudades de ejemplo.
        let (mut small, mut city) = (0, 0);
        let (mut shrubs, mut flowers) = (0, 0);
        for (_, t) in towns_with_names() {
            let yards = trees_of(&t, ShadeKind::Yard).len();
            if t.size == TownSize::Small {
                small += yards;
            } else {
                city += yards;
            }
            shrubs += t
                .yard_plants
                .iter()
                .filter(|p| p.kind == PlantKind::Shrub)
                .count();
            flowers += t
                .yard_plants
                .iter()
                .filter(|p| p.kind == PlantKind::Flowers)
                .count();
        }

        // Después del 15 % más (2026-10-27): los pueblos de ejemplo 178, las
        // ciudades 968.
        assert!(
            (165..=190).contains(&small),
            "{small} yard trees in the towns"
        );
        assert!(
            (910..=1030).contains(&city),
            "{city} yard trees in the cities"
        );
        // Plantas además: al menos un tercio de la cantidad de árboles.
        assert!(
            shrubs * 3 > (small + city) && flowers * 3 > (small + city) / 2,
            "{shrubs} {flowers}"
        );
    }

    #[test]
    fn yard_plants_stay_behind_the_houses_off_everything_else() {
        let mut checked = 0;
        for (name, t) in towns_with_names() {
            let walls: Vec<Aabb> = t
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect();
            let obstacles = t.obstacles();
            for (k, plant) in t.yard_plants.iter().enumerate() {
                let spot = plant.area();
                // Un arbusto mide 1,2 m de ancho; un cantero de flores, 1,4 m.
                let side = spot.size().x;
                assert!(
                    (side - 1.2).abs() < 1e-3 || (side - 1.4).abs() < 1e-3,
                    "{name}"
                );
                assert!(
                    walls.iter().all(|w| !overlap(w, &spot)),
                    "{name}: plant {k} in a building"
                );
                assert!(
                    obstacles.iter().all(|o| !overlap(o, &spot.expanded(0.3))),
                    "{name}: plant {k} by an obstacle"
                );
                assert!(
                    t.streets.iter().all(|s| !overlap(s, &spot)),
                    "{name}: plant {k} on a street"
                );
                assert!(t.plazas().all(|p| !overlap(&p, &spot)), "{name}");
                assert!(
                    t.sidewalks().iter().all(|w| !overlap(&w.area, &spot)),
                    "{name}"
                );
                // Fuera del terreno frente a cada puerta (1,8 m a cada lado, 4 m
                // hacia afuera).
                for b in t
                    .buildings
                    .iter()
                    .filter(|b| b.kind != BuildingKind::Platform)
                {
                    let m = normal(b.front);
                    let h = b.footprint.size() * 0.5;
                    let facade = b.footprint.center() + m * (m.x.abs() * h.x + m.y.abs() * h.y);
                    let across = Vec2::new(-m.y, m.x);
                    let (a, z) = (facade + across * 1.8, facade + m * 4.0 - across * 1.8);
                    let door = Aabb::new(
                        Vec2::new(a.x.min(z.x), a.y.min(z.y)),
                        Vec2::new(a.x.max(z.x), a.y.max(z.y)),
                    );
                    assert!(!overlap(&door, &spot), "{name}: plant {k} at a door");
                }
                for other in &t.yard_plants[k + 1..] {
                    assert!(
                        plant.at.distance(other.at) >= 1.59,
                        "{name}: plants too close"
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    #[test]
    fn a_very_big_town_has_over_a_third_fewer_plaza_trees_taken_from_the_groves() {
        // Antes del raleo (usuario, 2026-10-26) los ejemplos muy grandes tenían
        // 90, 75 y 90 árboles de plaza (los 36 y 54 de los anillos, 39 y 54 de
        // los bosquecillos); una quinta parte menos después de la primera
        // prueba de juego (72, 60, 72), 36 de cada 100 menos después de la
        // segunda (58, 48, 58), todo de los bosquecillos; después, un 15 % más
        // de árboles en los pueblos (2026-10-27): los anillos crecieron a 44 y
        // el recorte, una proporción del total, con ellos (71, 64, 73).
        let very_big: Vec<(String, TownLayout)> = towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size == TownSize::VeryBig)
            .collect();
        assert_eq!(very_big.len(), 3);
        for ((name, t), total) in very_big.iter().zip([71, 64, 73]) {
            let groves = trees_of(t, ShadeKind::Grove).len();
            assert_eq!(t.trees.len(), 44, "{name}: the rings stay whole");
            assert_eq!(t.trees.len() + groves, total, "{name}");
            // Sigue habiendo bosquecillos en el medio, no solo en el borde.
            let plaza = t.plaza.expanded(-12.0);
            let inside = trees_of(t, ShadeKind::Grove)
                .iter()
                .filter(|g| plaza.contains_point(g.at))
                .count();
            assert!(inside >= 4, "{name}: {inside} grove trees in the middle");
        }
        // Los bosquecillos de los pueblos grandes con la grilla más densa
        // (antes 9, 7, 11, 17).
        let big: Vec<(String, TownLayout)> = towns_with_names()
            .into_iter()
            .filter(|(_, t)| t.size == TownSize::Big)
            .collect();
        assert_eq!(big.len(), 4);
        for ((name, t), groves) in big.iter().zip([10, 12, 11, 19]) {
            assert_eq!(trees_of(t, ShadeKind::Grove).len(), groves, "{name}");
        }
    }

    #[test]
    fn towns_plant_fifteen_percent_more_trees() {
        // Árboles de los pueblos de ejemplo antes de los extra (2026-10-27):
        // anillos de plaza 340, árboles de calle 2039, de frente 189,
        // bosquecillos 100, patios 1013, 3681 en total. Los pasos enteros y el
        // recorte de bosquecillos de un pueblo muy grande hacen que las partes
        // se aparten de un 15 % exacto.
        let count = |kind| {
            sample_towns()
                .iter()
                .flat_map(|t| t.shade_trees.iter())
                .filter(|s| s.kind == kind)
                .count()
        };
        let ring: usize = sample_towns().iter().map(|t| t.trees.len()).sum();
        let total = ring
            + [
                ShadeKind::Pit,
                ShadeKind::Front,
                ShadeKind::Grove,
                ShadeKind::Yard,
            ]
            .map(count)
            .iter()
            .sum::<usize>();
        let share = total as f32 / 3681.0;
        assert!((1.12..=1.18).contains(&share), "{total} trees: {share}");
        assert!(count(ShadeKind::Pit) as f32 >= 1.12 * 2039.0);
        assert!(count(ShadeKind::Yard) as f32 >= 1.08 * 1013.0);
        assert!(ring as f32 >= 1.08 * 340.0);
    }
}
