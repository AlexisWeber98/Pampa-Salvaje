//! Caminos de tierra entre pueblos vecinos, para carros y jinetes.
//!
//! Un camino une dos pueblos vecinos sobre la línea del ferrocarril, o un pueblo
//! de un ramal con el pueblo más cercano (ver [`road_pairs`]). Sale de cada
//! pueblo por la salida de la calle principal que mira hacia el otro pueblo,
//! atraviesa en línea recta la franja nivelada del pueblo y después sigue una
//! línea con semilla y curvas suaves, más directa que la vía férrea: se saltea
//! los tramos finales de la vía y sus curvas amplias.
//!
//! El camino se mantiene a [`RoadNetwork::RAIL_CLEARANCE`] de la vía. Solo cruza
//! la línea donde debe, cuando sus dos extremos están en lados opuestos de ella,
//! y lo hace en perpendicular, en un paso a nivel ([`LevelCrossing`]).
//!
//! Los caminos son una regla del mundo, como la vía férrea: un camino que
//! atravesaría el terreno nivelado de un tercer pueblo o saldría del mundo se
//! deja fuera y se informa.

use std::collections::HashMap;
use std::fmt;

use crate::rng::Rng;
use crate::{Aabb, Lagoon, RailLine, TownLayout, Vec2, WorldBounds};

/// Qué une un camino.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadKind {
    /// Un camino entre dos pueblos vecinos.
    Between,
    /// Una huella corta sin salida desde un pueblo, cruzando su vía férrea
    /// junto a la estación, hacia las chacras y estancias (usuario,
    /// 2026-10-22).
    Rural,
    /// Una calle del pueblo que cruza la vía férrea que atraviesa el pueblo
    /// ([`crate::RailwayConfig::through`]), en su paso a nivel.
    Street,
    /// Un camino público sin salida desde un pueblo hasta la orilla de una
    /// laguna, que termina en un pequeño retorno junto al agua (usuario,
    /// 2026-10-23).
    Lagoon,
}

/// Un camino: entre dos pueblos, o una huella rural que sale de uno.
#[derive(Debug, Clone, PartialEq)]
pub struct Road {
    pub kind: RoadKind,
    /// Índice (dentro de los pueblos del mundo) del pueblo del que sale el
    /// camino.
    pub from: usize,
    /// Índice del pueblo al que llega el camino (otra vez `from` para una
    /// huella rural, que termina en campo abierto, y para un cruce de calle).
    pub to: usize,
    /// La línea central, desde la salida de calle de `from` hasta la de `to` (o
    /// hasta el final de la huella rural).
    pub path: Vec<Vec2>,
    /// Dónde sale el camino de `from` y dónde llega a `to`.
    pub ends: [RoadEnd; 2],
    /// Todos los lugares donde el camino cruza la vía férrea, en el orden del
    /// camino.
    pub crossings: Vec<LevelCrossing>,
}

/// Donde un camino se encuentra con la cuadrícula de calles de un pueblo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadEnd {
    /// La salida de la calle, en el borde de la cuadrícula.
    pub at: Vec2,
    /// Dirección unitaria hacia afuera del pueblo, a lo largo de la calle.
    pub out: Vec2,
}

/// Un paso a nivel: un camino que cruza la vía férrea.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelCrossing {
    /// Donde se encuentran las líneas centrales del camino y de la vía.
    pub at: Vec2,
    /// Dirección unitaria del camino en ese punto, en el orden del camino.
    pub road: Vec2,
    /// Dirección unitaria de la vía en ese punto.
    pub rail: Vec2,
    /// Distancia a lo largo del camino hasta el paso a nivel, en metros.
    pub along: f32,
}

/// Un cartel indicador junto a un camino: un poste de madera con una tabla en
/// forma de flecha por cada pueblo al que lleva el camino.
#[derive(Debug, Clone, PartialEq)]
pub struct Signpost {
    pub at: Vec2,
    pub boards: Vec<SignBoard>,
}

/// Una tabla en forma de flecha de un [`Signpost`].
#[derive(Debug, Clone, PartialEq)]
pub struct SignBoard {
    /// Lo que dice la tabla, por ejemplo `EDUARDO CASTEX 2,3 KM`.
    pub text: String,
    /// Dirección unitaria hacia la que apunta la flecha, a lo largo del camino.
    pub toward: Vec2,
}

/// Por qué se dejó fuera un camino.
#[derive(Debug, Clone, PartialEq)]
pub enum RoadError {
    /// El camino entre `from` y `to` atravesaría el terreno nivelado de
    /// `other`.
    CrossesTown {
        from: String,
        to: String,
        other: String,
    },
    /// El camino entre `from` y `to` saldría del mundo.
    OutsideWorld { from: String, to: String },
    /// El camino entre `from` y `to` cruzaría la vía férrea a `degrees`, menos
    /// perpendicular que [`RoadNetwork::MIN_CROSSING_ANGLE`].
    UnsquareCrossing {
        from: String,
        to: String,
        degrees: f32,
    },
    /// El camino entre `from` y `to` correría junto a la vía, más cerca que
    /// [`RoadNetwork::NEAR_RAIL`], lejos de sus pasos a nivel.
    NearRail { from: String, to: String },
    /// La huella rural de `town` no tiene un paso libre a través de la playa de
    /// su estación.
    NoRuralCrossing { town: String },
}

impl fmt::Display for RoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CrossesTown { from, to, other } => write!(
                f,
                "roads: the road from `{from}` to `{to}` runs across `{other}`; road left out"
            ),
            Self::OutsideWorld { from, to } => write!(
                f,
                "roads: the road from `{from}` to `{to}` leaves the world; road left out"
            ),
            Self::UnsquareCrossing { from, to, degrees } => write!(
                f,
                "roads: the road from `{from}` to `{to}` crosses the railway at {degrees:.0}°; road left out"
            ),
            Self::NearRail { from, to } => write!(
                f,
                "roads: the road from `{from}` to `{to}` runs too close alongside the railway; road left out"
            ),
            Self::NoRuralCrossing { town } => write!(
                f,
                "roads: `{town}` has no clear way across its station yard; rural track left out"
            ),
        }
    }
}

impl std::error::Error for RoadError {}

/// Un tramo recto de un camino, para el índice.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    a: Vec2,
    b: Vec2,
    road: u32,
    /// Distancia a lo largo del camino hasta `a`.
    along: f32,
}

/// Todos los caminos del mundo, con un índice en grilla de sus tramos para que
/// el suelo y la flora puedan preguntar de forma barata a qué distancia está el
/// camino más cercano.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoadNetwork {
    pub roads: Vec<Road>,
    pub signposts: Vec<Signpost>,
    pieces: Vec<Piece>,
    lengths: Vec<f32>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

/// Las líneas centrales de la vía férrea como polilíneas orientadas, para que
/// "de qué lado" sea la misma pregunta a lo largo de cada una: el recorrido
/// completo de la línea, después el recorrido de cada ramal, que avanza hacia
/// afuera desde su empalme, y después el de cada vía muerta, girado según la
/// orientación de la línea ([`RailLine::track_paths`]). Orientados así, el lado
/// exterior de un ramal es el de la línea: la tierra entre un ramal y la línea
/// de la que sale queda del mismo lado de ambos.
struct Track(Vec<Vec<Vec2>>);

impl Track {
    /// Todos los recorridos de vía de `line` a través de `towns`.
    fn of(line: &RailLine, towns: &[TownLayout]) -> Self {
        Self(
            line.track_paths(towns)
                .iter()
                .map(|p| p.points().to_vec())
                // Una vía muerta va desde su extremo cerrado hasta la vía, en
                // sentido contrario a la orientación de la línea en ese punto:
                // dada vuelta, su izquierda es la de la línea.
                .map(|mut p| {
                    if line.bays.iter().any(|b| b.link.path == p) {
                        p.reverse();
                    }
                    p
                })
                .filter(|p| p.len() >= 2)
                .collect(),
        )
    }

    /// Una vía recta, de principio a fin.
    fn straight(start: Vec2, end: Vec2) -> Self {
        Self(vec![vec![start, end]])
    }

    /// Todos los tramos rectos de todos los recorridos.
    fn pieces(&self) -> impl Iterator<Item = &[Vec2]> + '_ {
        self.0.iter().flat_map(|p| p.windows(2))
    }

    /// Distancia desde `p` hasta la vía más cercana, con signo positivo a la
    /// izquierda de la dirección de esa vía, y la normal unitaria que apunta a
    /// la izquierda en ese punto.
    fn signed(&self, p: Vec2) -> (f32, Vec2) {
        let mut best = (f32::INFINITY, 0.0, Vec2::new(0.0, 1.0));
        for w in self.pieces() {
            let (a, b) = (w[0], w[1]);
            let d = segment_distance(p, a, b);
            if d < best.0 {
                let dir = unit(b - a).unwrap_or(Vec2::new(1.0, 0.0));
                let left = Vec2::new(-dir.y, dir.x);
                let side = if (p - a).dot(left) >= 0.0 { 1.0 } else { -1.0 };
                best = (d, side, left);
            }
        }
        if best.0.is_infinite() {
            return (f32::INFINITY, Vec2::new(0.0, 1.0));
        }
        (best.0 * best.1, best.2)
    }

    /// El primer punto donde el segmento `p`–`q` cruza una vía, empezando desde
    /// `p`, con la dirección unitaria de la vía en ese punto.
    fn first_crossing(&self, p: Vec2, q: Vec2) -> Option<(Vec2, Vec2)> {
        self.pieces()
            .filter_map(|w| {
                let (t, _) = intersect(p, q, w[0], w[1])?;
                Some((t, p + (q - p) * t, unit(w[1] - w[0])?))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, at, dir)| (at, dir))
    }
}

impl RoadNetwork {
    /// Ancho de la superficie del camino, en metros: el de dos carros, con
    /// lugar para convertirse más adelante en una ruta (usuario, 2026-10-22).
    pub const WIDTH: f32 = 6.5;
    /// La mitad de [`Self::WIDTH`].
    pub const HALF_WIDTH: f32 = Self::WIDTH / 2.0;
    /// Distancia mínima desde la línea central del camino hasta la de la vía,
    /// en metros, lejos de los pasos a nivel.
    pub const RAIL_CLEARANCE: f32 = 40.0;
    /// Cuánto sale el camino en línea recta desde el pueblo, en metros:
    /// atravesando la franja nivelada y un poco más allá.
    pub const RUN_OUT: f32 = TownLayout::LEVELED_BAND + 20.0;
    /// Qué tan hondo está cortado el camino en el suelo, en metros.
    pub const CUT_DEPTH: f32 = 0.15;
    /// Ancho del terreno nivelado a cada lado de la línea central: el camino y
    /// una banquina, en metros.
    pub const CUT_FLAT: f32 = Self::HALF_WIDTH + 1.5;
    /// Donde el corte ya se fundió de nuevo con el terreno natural, en metros
    /// desde la línea central.
    pub const CUT_REACH: f32 = Self::HALF_WIDTH + 8.0;
    /// En los primeros y los últimos metros de un camino el corte aparece
    /// suavemente, para que el camino se una al ras con las calles del pueblo.
    const CUT_EASE: f32 = 25.0;
    /// Cuánto más allá de sus extremos un tramo todavía nivela el suelo, en
    /// metros: lo suficiente para cubrir el lado exterior de las curvas suaves
    /// del camino.
    const CUT_OVERLAP: f32 = 2.0;
    /// La mitad del ancho de la plataforma de un paso a nivel a través de la
    /// vía, en metros desde la línea central de la vía: los tablones y el
    /// balasto de al lado, donde el camino queda a la altura de la parte
    /// superior de los rieles.
    pub const CROSSING_DECK: f32 = 1.7;
    /// Largo del acceso de tierra apisonada a cada lado de un paso a nivel, en
    /// metros: el camino sube del suelo a la plataforma a lo largo de él, con
    /// una pendiente más suave que 1 en 10.
    pub const CROSSING_RAMP: f32 = 7.0;
    /// Altura de la plataforma de un paso a nivel sobre el suelo, en metros: la
    /// parte superior de los tablones, justo por debajo de las cabezas de los
    /// rieles.
    pub const CROSSING_RISE: f32 = crate::LineTrain::RAIL_TOP - 0.02;
    /// Al costado del camino, el acceso baja hasta el suelo entre esta cantidad
    /// de metros hacia adentro y hacia afuera del borde del camino.
    pub const CROSSING_SIDE: f32 = 0.8;
    /// La mitad del ancho del lecho de la vía (durmientes y balasto), en metros
    /// desde la línea central de la vía: el corte de un camino lo deja intacto,
    /// así que la vía, el tren y los andenes nunca se hunden en un paso a
    /// nivel.
    pub const RAIL_BED: f32 = 3.0;
    /// Pasado [`Self::RAIL_BED`], el corte vuelve a aparecer suavemente a lo
    /// largo de esta cantidad de metros.
    const RAIL_BLEND: f32 = 6.0;
    /// A lo largo de la vía, el lecho se conserva hasta esta cantidad de metros
    /// de un paso a nivel y se desvanece en los diez siguientes (ahí el corte
    /// ya desapareció hace rato).
    const RAIL_NEAR: f32 = 30.0;
    /// Lo más lejos que llega cualquier consulta de [`Self::near`], en metros.
    pub const INDEX_REACH: f32 = 64.0;
    /// Lado de una celda del índice, en metros.
    const CELL: f32 = 32.0;
    /// Separación de las muestras del trazado, en metros.
    const STEP: f32 = 10.0;
    /// A qué distancia de la salida del camino está su cartel indicador, en
    /// metros.
    pub const SIGNPOST_OUT: f32 = 14.0;
    /// Lado de la huella cuadrada de un poste (carteles indicadores, señales de
    /// paso a nivel).
    pub const POST: f32 = 0.25;
    /// Ángulo mínimo entre un camino y la vía donde se cruzan, en radianes
    /// (80°): los pasos a nivel se construyen en perpendicular.
    pub const MIN_CROSSING_ANGLE: f32 = 80.0 * std::f32::consts::PI / 180.0;
    /// Distancia mínima desde la línea central de un camino hasta la de la vía,
    /// en metros, lejos de sus pasos a nivel y de sus pueblos (ver
    /// [`Self::RAIL_CLEARANCE`]; las curvas que lo mantienen ahí pueden quedar
    /// un poco cortas).
    pub const NEAR_RAIL: f32 = 0.9 * Self::RAIL_CLEARANCE;
    /// Lo más lejos que puede estar un pueblo de una laguna (desde su franja
    /// nivelada hasta el centro de la laguna) para que se trace un camino hasta
    /// ella, en metros.
    pub const LAGOON_REACH: f32 = 2500.0;
    /// Radio del lazo de retorno al final de un camino a una laguna, en metros.
    pub const TURNAROUND: f32 = 6.0;
    /// A qué distancia más allá de la línea del agua está el centro del
    /// retorno, en metros: el borde cercano del lazo llega hasta el barro
    /// húmedo y no más.
    pub const LANDING: f32 = Lagoon::MUD_BAND + Self::HALF_WIDTH + Self::TURNAROUND + 0.5;
    /// Dentro de esta cantidad de metros de un paso a nivel, un camino puede
    /// acercarse a la vía: sus accesos perpendiculares.
    const CROSSING_APPROACH: f32 = Self::RAIL_CLEARANCE + 90.0;

    /// Los caminos entre cada par de pueblos vecinos sobre `line`, y desde cada
    /// pueblo de un ramal hasta su pueblo más cercano, dentro de `bounds`, y
    /// los caminos dejados fuera, cada uno informado. Cruzan por igual la línea
    /// y sus ramales, en pasos a nivel.
    pub fn build(
        towns: &[TownLayout],
        line: &RailLine,
        bounds: WorldBounds,
    ) -> (Self, Vec<RoadError>) {
        let track = Track::of(line, towns);
        let mut roads = Vec::new();
        let mut errors = Vec::new();
        for (from, to) in road_pairs(towns, line) {
            let (Some(a), Some(b)) = (towns.get(from), towns.get(to)) else {
                continue;
            };
            let road = route(a, b, &track, (from, to));
            match check(&road, towns, bounds, &track) {
                Ok(()) => roads.push(road),
                Err(error) => errors.push(error),
            }
        }
        for (i, town) in towns.iter().enumerate() {
            roads.extend(street_crossings(town, i, &track));
            let road = match rural(town, i, &track) {
                Some(Ok(road)) => road,
                Some(Err(error)) => {
                    errors.push(error);
                    continue;
                }
                None => continue,
            };
            match check(&road, towns, bounds, &track) {
                Ok(()) => roads.push(road),
                Err(error) => errors.push(error),
            }
        }
        let names: Vec<&str> = towns.iter().map(|t| t.name.as_str()).collect();
        (Self::new(roads, &names), errors)
    }

    /// Esta red con un camino hasta cada una de las `lagoons` desde el pueblo
    /// más cercano que esté a [`Self::LAGOON_REACH`] o menos ([`lagoon_road`]),
    /// cada uno con un cartel indicador al comienzo que nombra la laguna, y los
    /// caminos a lagunas dejados fuera, cada uno informado. Las lagunas
    /// deberían venir de [`crate::place_lagoons`], comprobadas contra los
    /// caminos de esta red.
    pub fn with_lagoons(
        self,
        towns: &[TownLayout],
        line: &RailLine,
        bounds: WorldBounds,
        lagoons: &[Lagoon],
    ) -> (Self, Vec<RoadError>) {
        let track = Track::of(line, towns);
        let mut roads = self.roads;
        let mut errors = Vec::new();
        let mut signs = Vec::new();
        for lagoon in lagoons {
            let nearest = towns
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let band = t.leveled();
                    (band.closest_point(lagoon.center).distance(lagoon.center), i)
                })
                .filter(|&(d, _)| d <= Self::LAGOON_REACH)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let Some((_, index)) = nearest else {
                continue;
            };
            let road = lagoon_road(&towns[index], index, lagoon, &track);
            match check(&road, towns, bounds, &track) {
                Ok(()) => {
                    signs.push((roads.len(), lagoon.name.clone()));
                    roads.push(road);
                }
                Err(error) => errors.push(error),
            }
        }
        let names: Vec<&str> = towns.iter().map(|t| t.name.as_str()).collect();
        let mut network = Self::new(roads, &names);
        for (i, name) in signs {
            let end = network.roads[i].ends[0];
            let right = Vec2::new(end.out.y, -end.out.x);
            network.signposts.push(Signpost {
                at: end.at + end.out * Self::SIGNPOST_OUT + right * (Self::HALF_WIDTH + 1.8),
                boards: vec![SignBoard {
                    text: board_text(&name, network.length(i)),
                    toward: end.out,
                }],
            });
        }
        (network, errors)
    }

    /// La red de `roads`, indexada, con un cartel indicador en cada extremo de
    /// camino que nombra el pueblo del otro extremo (`names` contiene los
    /// nombres de los pueblos).
    pub fn new(roads: Vec<Road>, names: &[&str]) -> Self {
        let mut network = Self {
            roads,
            ..Self::default()
        };
        for (r, road) in network.roads.iter().enumerate() {
            let mut along = 0.0;
            for w in road.path.windows(2) {
                let piece = Piece {
                    a: w[0],
                    b: w[1],
                    road: r as u32,
                    along,
                };
                along += w[0].distance(w[1]);
                let index = network.pieces.len() as u32;
                let reach = Aabb::new(
                    Vec2::new(w[0].x.min(w[1].x), w[0].y.min(w[1].y)),
                    Vec2::new(w[0].x.max(w[1].x), w[0].y.max(w[1].y)),
                )
                .expanded(Self::INDEX_REACH);
                for cell in cells_of(reach) {
                    network.cells.entry(cell).or_default().push(index);
                }
                network.pieces.push(piece);
            }
            network.lengths.push(along);
        }
        network.signposts = network.place_signposts(names);
        network
    }

    /// Los tramos rectos de todos los caminos, en el orden de los caminos.
    pub fn segments(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.pieces.iter().map(|p| (p.a, p.b))
    }

    /// Todos los pasos a nivel de todos los caminos.
    pub fn crossings(&self) -> impl Iterator<Item = &LevelCrossing> + '_ {
        self.roads.iter().flat_map(|r| r.crossings.iter())
    }

    /// Longitud del camino `index` a lo largo de su línea central, en metros.
    pub fn length(&self, index: usize) -> f32 {
        self.lengths.get(index).copied().unwrap_or(0.0)
    }

    /// Longitud del camino entre los pueblos `a` y `b` (en cualquier sentido),
    /// si la red tiene uno.
    pub fn length_between(&self, a: usize, b: usize) -> Option<f32> {
        self.roads
            .iter()
            .position(|r| {
                r.kind == RoadKind::Between
                    && ((r.from, r.to) == (a, b) || (r.from, r.to) == (b, a))
            })
            .map(|i| self.length(i))
    }

    /// Distancia exacta desde `p` hasta la línea central de camino más cercana
    /// (infinita si no hay caminos). Revisa todos los tramos: para pruebas y
    /// consultas poco frecuentes; ver [`Self::near`] para la versión barata.
    pub fn distance(&self, p: Vec2) -> f32 {
        self.pieces
            .iter()
            .map(|s| segment_distance(p, s.a, s.b))
            .fold(f32::INFINITY, f32::min)
    }

    /// Distancia desde `p` hasta la línea central de camino más cercana cuando
    /// está a `reach` metros o menos (como mucho [`Self::INDEX_REACH`]); `None`
    /// en caso contrario.
    pub fn near(&self, p: Vec2, reach: f32) -> Option<f32> {
        let reach = reach.min(Self::INDEX_REACH);
        self.nearby(p)
            .map(|s| segment_distance(p, s.a, s.b))
            .filter(|&d| d <= reach)
            .min_by(f32::total_cmp)
    }

    /// Los tramos indexados cuyo alcance cubre `p`.
    fn nearby(&self, p: Vec2) -> impl Iterator<Item = &Piece> + '_ {
        let cell = cell_of(p);
        let finite = p.x.is_finite() && p.y.is_finite();
        self.cells
            .get(&cell)
            .filter(|_| finite)
            .into_iter()
            .flatten()
            .map(|&i| &self.pieces[i as usize])
    }

    /// Cuánta superficie de camino hay en `p`, en `[0, 1]`: 1 sobre el camino,
    /// desvaneciéndose hasta 0 a lo largo de un metro y medio más allá de su
    /// borde. Continua.
    pub fn surface(&self, p: Vec2) -> f32 {
        let Some(d) = self.near(p, Self::HALF_WIDTH + 1.5) else {
            return 0.0;
        };
        1.0 - smoothstep(Self::HALF_WIDTH - 0.5, Self::HALF_WIDTH + 1.5, d)
    }

    /// El corte del camino en el suelo en `p`: con cuánta fuerza se aplica, en
    /// `[0, 1]`, y la altura a la que nivela el suelo, dada la altura del suelo
    /// sin cortar `base`. A lo ancho del camino el suelo está nivelado, a la
    /// altura de la línea central, un poco por debajo de ella; a lo largo del
    /// camino sigue el terreno. `None` lejos de los caminos.
    ///
    /// Continuo en todas partes: cada tramo al alcance tira hacia su propio
    /// nivel con un peso que se desvanece hasta 0 a [`Self::CUT_REACH`] de él y
    /// a un par de metros más allá de sus extremos, y los niveles se promedian
    /// según el peso. No se toca nada que esté a más de [`Self::CUT_REACH`] del
    /// camino. El lecho de la vía en un paso a nivel se deja como está, y el
    /// corte vuelve a aparecer suavemente al lado (ver [`Self::RAIL_BED`]).
    pub fn cut(&self, p: Vec2, base: impl Fn(Vec2) -> f32) -> Option<(f32, f32)> {
        let (mut strongest, mut total, mut weighted) = (0.0_f32, 0.0, 0.0);
        for piece in self.nearby(p) {
            let d = piece.b - piece.a;
            let length = d.length();
            if length <= 0.0 {
                continue;
            }
            let dir = d * (1.0 / length);
            // A lo ancho de la línea del tramo, y más allá de sus extremos a lo
            // largo de ella.
            let s = (p - piece.a).dot(dir);
            let off = ((p - piece.a).x * dir.y - (p - piece.a).y * dir.x).abs();
            let past = (-s).max(s - length).max(0.0);
            // Se desvanece según la distancia real al tramo (no solo a lo ancho
            // de su línea), así que no se toca nada más allá de `CUT_REACH`
            // desde el camino, tampoco alrededor de los extremos de los tramos.
            let reach = off.hypot(past);
            let w = (1.0 - smoothstep(Self::CUT_FLAT, Self::CUT_REACH, reach))
                * (1.0 - smoothstep(0.0, Self::CUT_OVERLAP, past));
            let t = (s / length).clamp(0.0, 1.0);
            let c = piece.a + d * t;
            if w <= 0.0 {
                continue;
            }
            let along = piece.along + piece.a.distance(piece.b) * t;
            let from_end = along.min(self.length(piece.road as usize) - along);
            let depth = Self::CUT_DEPTH * smoothstep(0.0, Self::CUT_EASE, from_end);
            strongest = strongest.max(w);
            total += w;
            weighted += w * (base(c) - depth);
        }
        if strongest <= 0.0 {
            return None;
        }
        let strongest = strongest * self.rail_keep(p);
        (strongest > 0.0).then(|| (strongest, weighted / total))
    }

    /// Cuánto del corte puede aplicarse en `p` al lado de la vía férrea, en
    /// `[0, 1]`: 0 sobre el lecho de la vía en un paso a nivel, subiendo
    /// suavemente hasta 1 más allá de él y lejos de los pasos a nivel.
    /// Continuo.
    fn rail_keep(&self, p: Vec2) -> f32 {
        let reach = Self::RAIL_NEAR + 10.0;
        self.crossings()
            .filter_map(|c| {
                let rel = p - c.at;
                let along = rel.dot(c.rail).abs();
                (along < reach).then(|| {
                    let across = (rel.x * c.rail.y - rel.y * c.rail.x).abs();
                    1.0 - (1.0
                        - smoothstep(Self::RAIL_BED, Self::RAIL_BED + Self::RAIL_BLEND, across))
                        * (1.0 - smoothstep(Self::RAIL_NEAR, reach, along))
                })
            })
            .fold(1.0, f32::min)
    }

    /// Cuánto eleva el acceso de un paso a nivel el camino en `p` sobre el
    /// terreno, en metros: [`Self::CROSSING_RISE`] sobre la plataforma a través
    /// de la vía, bajando suavemente hasta 0 a lo largo de
    /// [`Self::CROSSING_RAMP`] a cada lado sobre el camino (con una pendiente
    /// no mayor que alrededor de 1 en 10), y bajando hasta el suelo en los
    /// bordes del camino ([`Self::CROSSING_SIDE`]); 0 lejos de los pasos a
    /// nivel. Continuo.
    ///
    /// El único perfil del acceso: [`crate::World::ground_height`] lo suma
    /// donde se apoyan los cuerpos y el render apoya sobre él la superficie del
    /// camino. El terreno en sí no se eleva, así que el lecho de la vía queda
    /// como está.
    pub fn crossing_rise(&self, p: Vec2) -> f32 {
        let ramp = Self::CROSSING_DECK + Self::CROSSING_RAMP;
        let (inner, outer) = (
            Self::HALF_WIDTH - Self::CROSSING_SIDE,
            Self::HALF_WIDTH + Self::CROSSING_SIDE,
        );
        self.crossings()
            .map(|c| {
                let rel = p - c.at;
                // A lo ancho de la vía (a lo largo del camino), y a lo ancho
                // del camino.
                let across = (rel.x * c.rail.y - rel.y * c.rail.x).abs();
                let aside = (rel.x * c.road.y - rel.y * c.road.x).abs();
                Self::CROSSING_RISE
                    * (1.0 - smoothstep(Self::CROSSING_DECK, ramp, across))
                    * (1.0 - smoothstep(inner, outer, aside))
            })
            .fold(0.0, f32::max)
    }

    /// Las partes de cada camino dentro de `area` (un chunk del terreno), cada
    /// una como puntos a lo largo de ella con la normal unitaria izquierda del
    /// camino en cada punto y la distancia a lo largo del camino. Los extremos
    /// de una parte están en el borde del área, así que las partes dibujadas
    /// chunk por chunk coinciden.
    pub fn parts_in(&self, area: Aabb) -> Vec<Vec<RoadPoint>> {
        let mut parts = Vec::new();
        for road in &self.roads {
            let mut current: Vec<RoadPoint> = Vec::new();
            let mut along = 0.0;
            for w in road.path.windows(2) {
                let (a, b) = (w[0], w[1]);
                let length = a.distance(b);
                let normal = unit(b - a).map_or(Vec2::new(0.0, 1.0), |d| Vec2::new(-d.y, d.x));
                match area.segment_overlap(a, b) {
                    Some((enter, exit)) if exit > enter => {
                        let point = |t: f32| RoadPoint {
                            at: a + (b - a) * t,
                            normal,
                            along: along + length * t,
                        };
                        if current.is_empty() || enter > 0.0 {
                            if !current.is_empty() {
                                parts.push(std::mem::take(&mut current));
                            }
                            current.push(point(enter));
                        } else if let Some(last) = current.last_mut() {
                            // Una curva: el inglete entre los dos tramos.
                            last.normal = unit(last.normal + normal).unwrap_or(normal);
                        }
                        current.push(point(exit));
                        if exit < 1.0 {
                            parts.push(std::mem::take(&mut current));
                        }
                    }
                    _ => {
                        if current.len() > 1 {
                            parts.push(std::mem::take(&mut current));
                        }
                        current.clear();
                    }
                }
                along += length;
            }
            if current.len() > 1 {
                parts.push(current);
            }
        }
        parts
    }

    /// Huellas de cada cartel indicador y de cada señal de paso a nivel: postes
    /// delgados, todos fuera del camino.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let post = Vec2::new(Self::POST, Self::POST);
        self.signposts
            .iter()
            .map(|s| s.at)
            .chain(self.crossings().flat_map(LevelCrossing::sign_spots))
            .map(|at| Aabb::from_center_size(at, post))
            .collect()
    }

    /// Un cartel indicador al lado de cada extremo de camino, a la derecha de
    /// un jinete que sale del pueblo, que nombra el pueblo del otro extremo y a
    /// qué distancia está.
    fn place_signposts(&self, names: &[&str]) -> Vec<Signpost> {
        let mut posts = Vec::new();
        for (i, road) in self.roads.iter().enumerate() {
            if road.kind != RoadKind::Between {
                continue;
            }
            let km = self.length(i);
            for (end, other) in [(road.ends[0], road.to), (road.ends[1], road.from)] {
                let right = Vec2::new(end.out.y, -end.out.x);
                let at = end.at + end.out * Self::SIGNPOST_OUT + right * (Self::HALF_WIDTH + 1.8);
                let name = names.get(other).copied().unwrap_or("");
                posts.push(Signpost {
                    at,
                    boards: vec![SignBoard {
                        text: board_text(name, km),
                        toward: end.out,
                    }],
                });
            }
        }
        posts
    }
}

/// Un punto de una parte de camino dentro de un área (ver
/// [`RoadNetwork::parts_in`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadPoint {
    pub at: Vec2,
    /// Normal unitaria a la izquierda de la dirección del camino.
    pub normal: Vec2,
    /// Distancia a lo largo de todo el camino, en metros.
    pub along: f32,
}

impl LevelCrossing {
    /// Distancia desde el paso a nivel, a lo largo del camino, hasta las
    /// señales de cruz de San Andrés de cada lado, en metros: lejos del balasto
    /// de la vía.
    pub const SIGN_BACK: f32 = 6.0;

    /// Dónde están las dos señales de cruz de San Andrés: una antes de la vía
    /// en cada acceso, a la derecha del tránsito que se acerca.
    pub fn sign_spots(&self) -> [Vec2; 2] {
        let right = Vec2::new(self.road.y, -self.road.x);
        let side = RoadNetwork::HALF_WIDTH + 1.2;
        [
            self.at - self.road * Self::SIGN_BACK + right * side,
            self.at + self.road * Self::SIGN_BACK - right * side,
        ]
    }

    /// El ángulo entre el camino y la vía, en radianes, en `[0, π/2]`.
    pub fn angle(&self) -> f32 {
        let cross = self.road.x * self.rail.y - self.road.y * self.rail.x;
        cross.abs().clamp(0.0, 1.0).asin()
    }
}

/// Lo que dice la tabla de un cartel indicador: el nombre del pueblo en
/// mayúsculas y la distancia en kilómetros con un decimal, con coma decimal.
pub fn board_text(name: &str, meters: f32) -> String {
    let tenths = (meters / 100.0).round() as u64;
    format!("{} {},{} KM", name.to_uppercase(), tenths / 10, tenths % 10)
}

/// Los pueblos que unen los caminos entre pueblos, como `(from, to)`: cada par
/// de pueblos vecinos sobre la línea y después cada pueblo de un ramal (más
/// allá de su empalme) con el pueblo más cercano según el centro de su
/// cuadrícula, sea cual sea; un ramal llega adonde no va ningún camino de la
/// línea, y su pueblo más cercano a menudo está fuera del ramal (el de Caleufú
/// es Castex, no Metileo). Un par que ya está unido no se une dos veces.
fn road_pairs(towns: &[TownLayout], line: &RailLine) -> Vec<(usize, usize)> {
    let mut pairs: Vec<(usize, usize)> = line.towns.windows(2).map(|w| (w[0], w[1])).collect();
    let center = |i: usize| towns[i].grid().center();
    for branch in &line.branches {
        for &town in branch.towns.iter().skip(1).filter(|&&i| i < towns.len()) {
            let nearest = (0..towns.len()).filter(|&i| i != town).min_by(|&a, &b| {
                let d = |i: usize| center(i).distance(center(town));
                d(a).total_cmp(&d(b))
            });
            let Some(other) = nearest else {
                continue;
            };
            let joined = pairs
                .iter()
                .any(|&(a, b)| (a, b) == (town, other) || (b, a) == (town, other));
            if !joined {
                pairs.push((other, town));
            }
        }
    }
    pairs
}

/// La salida de la cuadrícula de calles de `town` que mira hacia `toward`: el
/// lado cuya dirección hacia afuera más apunta hacia ahí (nunca el lado de la
/// vía férrea, que ocupa la playa), y de ese lado la calle que atraviesa la
/// cuadrícula más cercana a su centro.
fn exit(town: &TownLayout, toward: Vec2) -> RoadEnd {
    let grid = town.grid();
    let center = grid.center();
    let rail_side = town.railway.map(|r| ((r.start + r.end) * 0.5) - center);
    // Una línea que atraviesa el pueblo sale por ambos extremos: un camino que
    // saliera por ahí iría pegado a la vía.
    let rail_axis = town
        .railway
        .filter(|_| town.rail_band().is_some())
        .and_then(|r| unit(r.end - r.start));
    let sides = [
        Vec2::new(1.0, 0.0),
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, -1.0),
    ];
    let want = unit(toward - center).unwrap_or(Vec2::new(1.0, 0.0));
    let out = sides
        .into_iter()
        .filter(|s| match rail_axis {
            Some(axis) => axis.dot(*s).abs() < 0.5,
            None => rail_side.is_none_or(|r| r.dot(*s) <= r.length() * 0.5),
        })
        .max_by(|a, b| a.dot(want).total_cmp(&b.dot(want)))
        .unwrap_or(Vec2::new(1.0, 0.0));
    // Calles que salen por ese lado, sin contar las calles perimetrales.
    let across = Vec2::new(-out.y, out.x);
    let edge = if out.x + out.y > 0.0 {
        grid.max.dot(out)
    } else {
        grid.min.dot(out)
    };
    let (lo, hi) = (
        grid.min.dot(across).min(grid.max.dot(across)),
        grid.max.dot(across).max(grid.min.dot(across)),
    );
    let mid = center.dot(across);
    let street = town
        .streets
        .iter()
        .filter(|s| {
            let size = s.size();
            let long = size.dot(Vec2::new(out.x.abs(), out.y.abs()));
            let wide = size.dot(Vec2::new(across.x.abs(), across.y.abs()));
            let reaches = (s.max.dot(out).max(s.min.dot(out)) - edge).abs() < 1e-3;
            let c = s.center().dot(across);
            long > wide && reaches && c - wide > lo + 1e-3 && c + wide < hi - 1e-3
        })
        .map(|s| s.center().dot(across))
        .min_by(|a, b| {
            let key = |c: f32| ((c - mid).abs(), -(c * want.dot(across)));
            key(*a)
                .partial_cmp(&key(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(mid);
    RoadEnd {
        at: out * edge + across * street,
        out,
    }
}

/// El camino del pueblo `a` al pueblo `b`.
fn route(a: &TownLayout, b: &TownLayout, track: &Track, (from, to): (usize, usize)) -> Road {
    let start = exit(a, b.grid().center());
    let end = exit(b, a.grid().center());
    let p = start.at + start.out * RoadNetwork::RUN_OUT;
    let q = end.at + end.out * RoadNetwork::RUN_OUT;
    let mut rng = Rng::new(a.seed.wrapping_mul(31).wrapping_add(b.seed) ^ ROAD_SEED, 0);
    let mut path = vec![start.at];
    path.extend(between(p, q, track, &mut rng, 3));
    path.push(end.at);
    let path = chaikin(path, 2);
    let crossings = crossings_of(&path, track);
    Road {
        kind: RoadKind::Between,
        from,
        to,
        path,
        ends: [start, end],
        crossings,
    }
}

/// La huella rural que sale de `town` cruzando su vía férrea: la calle que
/// atraviesa la cuadrícula y sale perpendicular a la vía más cerca de la
/// estación, con un paso libre a través de la playa de la estación, un paso a
/// nivel y, más allá de la franja nivelada, un tramo sin salida con semilla y
/// curvas suaves de 300–800 m. `None` para un pueblo sin vía férrea; un error
/// cuando no hay un paso libre a través de su playa.
fn rural(town: &TownLayout, index: usize, track: &Track) -> Option<Result<Road, RoadError>> {
    // Un pueblo a ambos lados de su línea llega al otro lado con sus calles.
    if town.rail_band().is_some() {
        return None;
    }
    let rail = town.railway?;
    let blocked = || {
        Err(RoadError::NoRuralCrossing {
            town: town.name.clone(),
        })
    };
    let grid = town.grid();
    let along = unit(rail.end - rail.start)?;
    let along = Vec2::new(along.x.abs(), along.y.abs());
    let middle = (rail.start + rail.end) * 0.5;
    let mut out = Vec2::new(along.y, along.x);
    if (middle - grid.center()).dot(out) < 0.0 {
        out = out * -1.0;
    }
    let axis = Vec2::new(out.x.abs(), out.y.abs());
    let edge = if sign(out) > 0.0 {
        grid.max.dot(axis)
    } else {
        grid.min.dot(axis)
    };
    let rails = middle.dot(axis);
    let station = town
        .buildings
        .iter()
        .find(|b| b.kind == crate::BuildingKind::Station)
        .map_or(middle, |b| b.footprint.center())
        .dot(along);
    let clear = RoadNetwork::HALF_WIDTH + 1.5;
    let beyond = rails + 6.0 * sign(out);
    let reaches =
        |s: &Aabb| (s.min.dot(axis) - edge).abs() < 1e-3 || (s.max.dot(axis) - edge).abs() < 1e-3;
    let clear_at = |c: f32| {
        let corridor = Aabb::new(
            along * (c - clear) + axis * edge.min(beyond),
            along * (c + clear) + axis * edge.max(beyond),
        );
        town.buildings
            .iter()
            .all(|b| !b.footprint.intersects(&corridor))
    };
    // Una calle que sale atravesando la playa, perpendicular a la vía...
    let through = town
        .streets
        .iter()
        .filter(|s| s.size().dot(axis) > s.size().dot(along) && reaches(s))
        .map(|s| s.center().dot(along))
        .filter(|&c| clear_at(c))
        .min_by(|a, b| (a - station).abs().total_cmp(&(b - station).abs()));
    // ...o, donde la playa ocupa todo el lado del pueblo, la calle perimetral a
    // lo largo de la vía prolongada más allá del extremo más cercano de la
    // cuadrícula.
    let Some(ring) = town
        .streets
        .iter()
        .filter(|s| s.size().dot(along) > s.size().dot(axis) && reaches(s))
        .map(|s| s.center().dot(axis))
        .next()
    else {
        return Some(blocked());
    };
    let (lo, hi) = (grid.min.dot(along), grid.max.dot(along));
    let (end, past) = if (station - lo).abs() <= (hi - station).abs() {
        (lo, lo - PAST_GRID)
    } else {
        (hi, hi + PAST_GRID)
    };
    let mut path = Vec::new();
    let line = match through {
        Some(c) => c,
        None if clear_at(past) => {
            path.push(along * end + axis * ring);
            past
        }
        None => return Some(blocked()),
    };
    let start = along * line + axis * edge;
    path.push(start);
    let straight = (rails - edge).abs() + 4.0 + RoadNetwork::RUN_OUT;
    let p = start + out * straight;
    let mut rng = Rng::new(town.seed.wrapping_mul(97) ^ ROAD_SEED, 1);
    let length = rng.range(RURAL_LENGTH.0, RURAL_LENGTH.1);
    let turn = rng.range(-0.45, 0.45);
    let (c, s) = (turn.cos(), turn.sin());
    let heading = Vec2::new(out.x * c - out.y * s, out.x * s + out.y * c);
    let q = p + heading * (length - straight).max(150.0);
    let side = track.signed(p).0.signum();
    let side = if side.is_finite() && side != 0.0 {
        side
    } else {
        1.0
    };
    path.extend(on_side(p, q, side, track, &mut rng));
    let path = chaikin(path, 2);
    let mut crossings = crossings_of(&path, track);
    if crossings.is_empty() {
        crossings = crossings_of(&path, &Track::straight(rail.start, rail.end));
    }
    let n = path.len();
    let ends = [
        RoadEnd {
            at: path[0],
            out: unit(path[1] - path[0]).unwrap_or(out),
        },
        RoadEnd {
            at: path[n - 1],
            out: unit(path[n - 1] - path[n - 2]).unwrap_or(heading),
        },
    ];
    Some(Ok(Road {
        kind: RoadKind::Rural,
        from: index,
        to: index,
        path,
        ends,
        crossings,
    }))
}

/// El camino público desde `town` (índice `index`) hasta `lagoon`.
///
/// Sale del pueblo por una calle del lado de la cuadrícula que mira hacia la
/// laguna, la más cercana a la laguna sobre ese lado que se mantiene a
/// [`RoadNetwork::RAIL_CLEARANCE`] de la vía (la línea de un pueblo puede salir
/// por ese lado); atraviesa en línea recta la franja nivelada y después sigue
/// una línea con semilla y curvas suaves, lejos de la vía, hasta la orilla de
/// la laguna, y termina ahí en un lazo de retorno de
/// [`RoadNetwork::TURNAROUND`] metros cuyo borde cercano se detiene en el barro
/// húmedo ([`RoadNetwork::LANDING`]).
fn lagoon_road(town: &TownLayout, index: usize, lagoon: &Lagoon, track: &Track) -> Road {
    let grid = town.grid();
    let center = grid.center();
    let rail_side = town
        .railway
        .filter(|_| town.rail_band().is_none())
        .map(|r| ((r.start + r.end) * 0.5) - center);
    let want = unit(lagoon.center - center).unwrap_or(Vec2::new(1.0, 0.0));
    let sides = [
        Vec2::new(1.0, 0.0),
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, -1.0),
    ];
    let out = sides
        .into_iter()
        .filter(|s| rail_side.is_none_or(|r| r.dot(*s) <= r.length() * 0.5))
        .max_by(|a, b| a.dot(want).total_cmp(&b.dot(want)))
        .unwrap_or(Vec2::new(1.0, 0.0));
    let across = Vec2::new(-out.y, out.x);
    let edge = if out.x + out.y > 0.0 {
        grid.max.dot(out)
    } else {
        grid.min.dot(out)
    };
    let (lo, hi) = (
        grid.min.dot(across).min(grid.max.dot(across)),
        grid.max.dot(across).max(grid.min.dot(across)),
    );
    let goal = lagoon.center.dot(across);
    let clear_of_rail = |c: f32| {
        let at = across * c + out * edge;
        track.signed(at).0.abs() >= RoadNetwork::RAIL_CLEARANCE
    };
    let street = town
        .streets
        .iter()
        .filter(|s| {
            let size = s.size();
            let long = size.dot(Vec2::new(out.x.abs(), out.y.abs()));
            let wide = size.dot(Vec2::new(across.x.abs(), across.y.abs()));
            let reaches = (s.max.dot(out).max(s.min.dot(out)) - edge).abs() < 1e-3;
            let c = s.center().dot(across);
            long > wide && reaches && c - wide > lo + 1e-3 && c + wide < hi - 1e-3
        })
        .map(|s| s.center().dot(across))
        .filter(|&c| clear_of_rail(c))
        .min_by(|a, b| (a - goal).abs().total_cmp(&(b - goal).abs()))
        .unwrap_or((lo + hi) / 2.0);
    let start = out * edge + across * street;
    let p = start + out * RoadNetwork::RUN_OUT;
    // El centro del retorno, en la orilla, del lado que mira hacia el pueblo.
    let toward = unit(p - lagoon.center).unwrap_or(out * -1.0);
    let angle = toward.y.atan2(toward.x);
    let reach = lagoon.shore_radius(angle) + RoadNetwork::LANDING;
    let hub = lagoon.center + toward * reach;
    let r = RoadNetwork::TURNAROUND;
    let q = hub + toward * r;
    let mut rng = Rng::new(town.seed.wrapping_mul(131) ^ ROAD_SEED, 2);
    let mut path = vec![start];
    path.extend(between(p, q, track, &mut rng, 3));
    // Alrededor del lazo y de vuelta adonde empezó, en sentido antihorario.
    let steps = 16;
    for k in 1..=steps {
        let a = angle + std::f32::consts::TAU * k as f32 / steps as f32;
        push_unique(&mut path, hub + Vec2::new(a.cos(), a.sin()) * r);
    }
    let path = chaikin(path, 2);
    let crossings = crossings_of(&path, track);
    let n = path.len();
    let ends = [
        RoadEnd {
            at: path[0],
            out: unit(path[1] - path[0]).unwrap_or(out),
        },
        RoadEnd {
            at: path[n - 1],
            out: unit(path[n - 1] - path[n - 2]).unwrap_or(toward * -1.0),
        },
    ];
    Road {
        kind: RoadKind::Lagoon,
        from: index,
        to: index,
        path,
        ends,
        crossings,
    }
}

/// Agrega `p` a `path` salvo que ya sea (casi) el último punto.
fn push_unique(path: &mut Vec<Vec2>, p: Vec2) {
    if path.last().is_none_or(|l| l.distance(p) > 1e-3) {
        path.push(p);
    }
}

/// Los pasos a nivel de las calles de `town` sobre una línea que lo atraviesa
/// ([`TownLayout::crossing_streets`]): un camino recto y corto para cada uno,
/// para que el suelo suba hasta una plataforma de tablones y el paso a nivel
/// tenga sus señales, como el de un camino. Ninguno para un pueblo junto a su
/// línea.
fn street_crossings(town: &TownLayout, index: usize, track: &Track) -> Vec<Road> {
    let Some(rail) = town.railway else {
        return Vec::new();
    };
    town.crossing_streets()
        .into_iter()
        .map(|(a, b)| {
            let path = vec![a, b];
            let mut crossings = crossings_of(&path, track);
            if crossings.is_empty() {
                crossings = crossings_of(&path, &Track::straight(rail.start, rail.end));
            }
            let out = unit(b - a).unwrap_or(Vec2::new(0.0, 1.0));
            Road {
                kind: RoadKind::Street,
                from: index,
                to: index,
                path,
                ends: [
                    RoadEnd {
                        at: a,
                        out: out * -1.0,
                    },
                    RoadEnd { at: b, out },
                ],
                crossings,
            }
        })
        .collect()
}

/// `1` a lo largo de un eje positivo, `-1` a lo largo de uno negativo.
fn sign(axis: Vec2) -> f32 {
    if axis.x + axis.y >= 0.0 { 1.0 } else { -1.0 }
}

/// El trayecto de `p` a `q` (ambos incluidos), quedándose de un lado de la vía,
/// o cruzándola en perpendicular cuando `p` y `q` están en lados opuestos.
fn between(p: Vec2, q: Vec2, track: &Track, rng: &mut Rng, depth: u32) -> Vec<Vec2> {
    let side = |x: Vec2| track.signed(x).0.signum();
    let (sp, sq) = (side(p), side(q));
    if sp == sq || depth == 0 {
        return on_side(p, q, sp, track, rng);
    }
    let Some((at, dir)) = track.first_crossing(p, q) else {
        return on_side(p, q, sp, track, rng);
    };
    // En perpendicular a través de la vía, desde bien atrás del lado de `p`
    // hasta bien adentro del lado de `q`.
    let left = Vec2::new(-dir.y, dir.x);
    let toward_q = left * sq;
    let back = RoadNetwork::RAIL_CLEARANCE + 40.0;
    let (before, after) = (at - toward_q * back, at + toward_q * back);
    let mut path = between(p, before, track, rng, depth - 1);
    let steps = (2.0 * back / RoadNetwork::STEP).ceil() as usize;
    for k in 1..steps {
        path.push(before + (after - before) * (k as f32 / steps as f32));
    }
    path.extend(between(after, q, track, rng, depth - 1));
    path
}

/// El trayecto de `p` a `q` (ambos incluidos) del lado `side` de la vía: la
/// línea recta con una ondulación suave con semilla, desviada lejos de la vía
/// donde quedaría a menos de [`RoadNetwork::RAIL_CLEARANCE`] de ella.
fn on_side(p: Vec2, q: Vec2, side: f32, track: &Track, rng: &mut Rng) -> Vec<Vec2> {
    let length = p.distance(q);
    let Some(dir) = unit(q - p) else {
        return vec![p, q];
    };
    let perp = Vec2::new(-dir.y, dir.x);
    let n = ((length / RoadNetwork::STEP).ceil() as usize).max(2);
    let t: Vec<f32> = (0..=n).map(|i| i as f32 / n as f32).collect();
    // Unas pocas curvas sinusoidales con semilla, que se anulan en ambos
    // extremos: una curva suave.
    let amplitude = (0.012 * length).min(WIGGLE_MAX);
    let bends: Vec<f32> = (1..=3).map(|k| rng.range(-1.0, 1.0) / k as f32).collect();
    let mut offset: Vec<f32> = t
        .iter()
        .map(|&t| {
            let s: f32 = bends
                .iter()
                .enumerate()
                .map(|(k, b)| b * (std::f32::consts::PI * (k + 1) as f32 * t).sin())
                .sum();
            amplitude * s / 1.84
        })
        .collect();
    let at = |offset: &[f32], i: usize| p + dir * (length * t[i]) + perp * offset[i];
    let taper = |i: usize| smoothstep(0.0, TAPER, length * t[i].min(1.0 - t[i]));
    for _ in 0..8 {
        let (mut push_left, mut push_right) = (vec![0.0_f32; n + 1], vec![0.0_f32; n + 1]);
        let mut pushed = false;
        for i in 1..n {
            let (signed, left) = track.signed(at(&offset, i));
            let gap = signed * side;
            if gap >= RoadNetwork::RAIL_CLEARANCE {
                continue;
            }
            // Moverse a lo largo de `perp` hacia este lado de la vía.
            let k = perp.dot(left * side);
            let k = k.signum() * k.abs().max(0.3);
            let shift = (RoadNetwork::RAIL_CLEARANCE + 1.0 - gap) / k;
            pushed = true;
            let reach = (BEND_REACH / (length * (t[1] - t[0]))).ceil() as usize;
            for j in i.saturating_sub(reach)..=(i + reach).min(n) {
                let d = length * (t[j] - t[i]).abs();
                if d >= BEND_REACH {
                    continue;
                }
                let v = shift * 0.5 * (1.0 + (std::f32::consts::PI * d / BEND_REACH).cos());
                let v = v * taper(j);
                if v > 0.0 {
                    push_left[j] = push_left[j].max(v);
                } else {
                    push_right[j] = push_right[j].min(v);
                }
            }
        }
        if !pushed {
            break;
        }
        for j in 0..=n {
            offset[j] += push_left[j] + push_right[j];
        }
    }
    (0..=n).map(|i| at(&offset, i)).collect()
}

/// Todos los lugares donde `path` cruza la vía, en el orden del recorrido.
fn crossings_of(path: &[Vec2], track: &Track) -> Vec<LevelCrossing> {
    let mut found = Vec::new();
    let mut along = 0.0;
    for w in path.windows(2) {
        let mut here: Vec<(f32, LevelCrossing)> = track
            .pieces()
            .filter_map(|r| {
                let (t, _) = intersect(w[0], w[1], r[0], r[1])?;
                Some((
                    t,
                    LevelCrossing {
                        at: w[0] + (w[1] - w[0]) * t,
                        road: unit(w[1] - w[0])?,
                        rail: unit(r[1] - r[0])?,
                        along: along + w[0].distance(w[1]) * t,
                    },
                ))
            })
            .collect();
        here.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.extend(here.into_iter().map(|(_, c)| c));
        along += w[0].distance(w[1]);
    }
    // Un cruce exactamente sobre la unión entre dos tramos de vía se encuentra
    // dos veces.
    found.dedup_by(|a, b| a.at.distance(b.at) < 1e-3);
    found
}

/// Comprueba `road` contra los otros pueblos, el borde del mundo y `track`:
/// debe cruzar en perpendicular (las alternativas de [`between`] y [`rural`]
/// pueden no hacerlo) y mantenerse a [`RoadNetwork::NEAR_RAIL`] de la vía lejos
/// de sus pasos a nivel y de sus propios pueblos.
fn check(
    road: &Road,
    towns: &[TownLayout],
    bounds: WorldBounds,
    track: &Track,
) -> Result<(), RoadError> {
    let name = |i: usize| towns[i].name.clone();
    let to = match road.kind {
        RoadKind::Between => name(road.to),
        RoadKind::Rural => "the open land".to_string(),
        RoadKind::Street => name(road.to),
        RoadKind::Lagoon => "its lagoon".to_string(),
    };
    let margin = RoadNetwork::CUT_REACH;
    let inside = road.path.iter().all(|p| {
        p.x - bounds.min.x >= margin
            && bounds.max.x - p.x >= margin
            && p.y - bounds.min.y >= margin
            && bounds.max.y - p.y >= margin
    });
    if !inside {
        return Err(RoadError::OutsideWorld {
            from: name(road.from),
            to,
        });
    }
    for (i, town) in towns.iter().enumerate() {
        if i == road.from || i == road.to {
            continue;
        }
        let band = town.leveled().expanded(margin);
        if road
            .path
            .windows(2)
            .any(|w| band.segment_overlap(w[0], w[1]).is_some())
        {
            return Err(RoadError::CrossesTown {
                from: name(road.from),
                to,
                other: town.name.clone(),
            });
        }
    }
    if let Some(c) = road
        .crossings
        .iter()
        .find(|c| c.angle() < RoadNetwork::MIN_CROSSING_ANGLE)
    {
        return Err(RoadError::UnsquareCrossing {
            from: name(road.from),
            to,
            degrees: c.angle().to_degrees(),
        });
    }
    let own = |p: Vec2| {
        [road.from, road.to]
            .iter()
            .filter_map(|&i| towns.get(i))
            .any(|t| t.leveled().expanded(RoadNetwork::RUN_OUT).contains_point(p))
    };
    let hugs = road.path.iter().any(|&p| {
        let approach = road
            .crossings
            .iter()
            .any(|c| c.at.distance(p) < RoadNetwork::CROSSING_APPROACH);
        !approach && !own(p) && track.signed(p).0.abs() < RoadNetwork::NEAR_RAIL
    });
    if hugs {
        return Err(RoadError::NearRail {
            from: name(road.from),
            to,
        });
    }
    Ok(())
}

/// Mayor ondulación lateral de un camino, en metros.
const WIGGLE_MAX: f32 = 30.0;
/// A lo largo de cuánto camino se extiende una curva que se aleja de la vía, en
/// metros.
const BEND_REACH: f32 = 300.0;
/// A lo largo de esta cantidad de metros desde cada extremo de un tramo, las
/// curvas aparecen suavemente.
const TAPER: f32 = 60.0;
/// Cuánto más allá del extremo de la cuadrícula cruza la vía férrea una huella
/// rural cuando la playa de la estación ocupa todo el lado del pueblo, en
/// metros.
const PAST_GRID: f32 = 15.0;
/// Huella rural más corta y más larga, en metros.
const RURAL_LENGTH: (f32, f32) = (300.0, 800.0);
/// Se mezcla con la semilla de cada camino.
const ROAD_SEED: u32 = 0x60ad_5eed;

/// Recorte de esquinas: cada pasada reemplaza cada esquina por dos puntos a un
/// cuarto del recorrido de sus tramos, conservando el primer y el último punto.
fn chaikin(points: Vec<Vec2>, passes: usize) -> Vec<Vec2> {
    let mut points = points;
    for _ in 0..passes {
        if points.len() < 3 {
            return points;
        }
        let mut next = Vec::with_capacity(points.len() * 2);
        next.push(points[0]);
        for (i, w) in points.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            if i > 0 {
                next.push(a + (b - a) * 0.25);
            }
            if i + 2 < points.len() {
                next.push(a + (b - a) * 0.75);
            }
        }
        next.push(points[points.len() - 1]);
        points = next;
    }
    points
}

/// Dónde se cruzan los segmentos `p`–`q` y `a`–`b`: los parámetros a lo largo
/// de cada uno, en `[0, 1]`; `None` si no se cruzan (o son paralelos).
pub(crate) fn intersect(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> Option<(f32, f32)> {
    let r = q - p;
    let s = b - a;
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-9 {
        return None;
    }
    let d = a - p;
    let t = (d.x * s.y - d.y * s.x) / denom;
    let u = (d.x * r.y - d.y * r.x) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some((t, u))
}

/// El punto del segmento `a`–`b` más cercano a `p`, y su parámetro.
fn closest(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f32) {
    let d = b - a;
    let span = d.dot(d);
    if span <= 0.0 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(d) / span).clamp(0.0, 1.0);
    (a + d * t, t)
}

/// Distancia desde `p` hasta el segmento `a`–`b`.
pub(crate) fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    p.distance(closest(p, a, b).0)
}

fn unit(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length > 1e-9).then(|| v * (1.0 / length))
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn cell_of(p: Vec2) -> (i32, i32) {
    (
        (p.x / RoadNetwork::CELL).floor() as i32,
        (p.y / RoadNetwork::CELL).floor() as i32,
    )
}

fn cells_of(area: Aabb) -> impl Iterator<Item = (i32, i32)> {
    let (x0, y0) = cell_of(area.min);
    let (x1, y1) = cell_of(area.max);
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::rail::tests::shipped;
    use crate::{PlazaPlacement, RailwayConfig, Side, TownConfig, WorldConfig, place_towns};

    fn big_world() -> WorldConfig {
        WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        }
    }

    /// Los pueblos distribuidos, su línea y sus caminos.
    pub(crate) fn shipped_roads() -> (Vec<TownLayout>, RailLine, RoadNetwork) {
        let (towns, line) = shipped();
        let (roads, errors) = RoadNetwork::build(&towns, &line, big_world().bounds());
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line, roads)
    }

    fn track(towns: &[TownLayout], line: &RailLine) -> Track {
        Track::of(line, towns)
    }

    fn between(roads: &RoadNetwork) -> Vec<(usize, &Road)> {
        roads
            .roads
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == RoadKind::Between)
            .collect()
    }

    fn on_edge(grid: Aabb, p: Vec2) -> bool {
        let near = |a: f32, b: f32| (a - b).abs() < 1e-3;
        (near(p.x, grid.min.x) || near(p.x, grid.max.x)) && (grid.min.y..=grid.max.y).contains(&p.y)
            || (near(p.y, grid.min.y) || near(p.y, grid.max.y))
                && (grid.min.x..=grid.max.x).contains(&p.x)
    }

    #[test]
    fn roads_join_neighbouring_towns_from_street_exits_and_keep_clear_of_the_rail() {
        let (towns, line, roads) = shipped_roads();
        let joined: Vec<(usize, usize)> = between(&roads)
            .iter()
            .map(|(_, r)| (r.from, r.to))
            .collect();
        // Castex–Monte Nievas y Monte Nievas–General Pico.
        assert_eq!(joined, [(1, 0), (0, 2)]);
        let track = track(&towns, &line);
        for (_, road) in between(&roads) {
            let (a, b) = (&towns[road.from], &towns[road.to]);
            assert_eq!(road.path[0], road.ends[0].at);
            assert_eq!(*road.path.last().unwrap(), road.ends[1].at);
            assert!(on_edge(a.grid(), road.ends[0].at), "{:?}", road.ends);
            assert!(on_edge(b.grid(), road.ends[1].at), "{:?}", road.ends);
            // Sale de una calle, mirando hacia el otro pueblo.
            for (end, town) in [(road.ends[0], a), (road.ends[1], b)] {
                assert!(
                    town.streets
                        .iter()
                        .any(|s| s.expanded(1e-3).contains_point(end.at))
                );
            }
            assert!(road.ends[0].out.dot(b.grid().center() - a.grid().center()) > 0.0);
            // Sin franja nivelada de un tercer pueblo, sin paso a nivel, lejos
            // del riel.
            for (i, town) in towns.iter().enumerate() {
                if i != road.from && i != road.to {
                    let band = town.leveled();
                    assert!(road.path.iter().all(|p| !band.contains_point(*p)));
                }
            }
            assert!(road.crossings.is_empty());
            for p in &road.path {
                let gap = track.signed(*p).0.abs();
                assert!(gap > 0.95 * RoadNetwork::RAIL_CLEARANCE, "{p:?}: {gap}");
            }
            // Con curvas suaves: ninguna curva cerrada.
            for w in road.path.windows(3) {
                let (u, v) = (unit(w[1] - w[0]).unwrap(), unit(w[2] - w[1]).unwrap());
                assert!(u.dot(v) > 0.9, "{w:?}");
            }
        }
        let (again, _) = RoadNetwork::build(&towns, &line, big_world().bounds());
        assert_eq!(again, roads);
    }

    #[test]
    fn roads_are_wide_enough_for_carts_and_future_routes() {
        assert!(
            (6.0..=7.0).contains(&RoadNetwork::WIDTH),
            "{}",
            RoadNetwork::WIDTH
        );
        assert_eq!(RoadNetwork::HALF_WIDTH * 2.0, RoadNetwork::WIDTH);
        // El corte y la franja dibujada siguen el ancho.
        const {
            assert!(RoadNetwork::CUT_FLAT > RoadNetwork::HALF_WIDTH);
            assert!(RoadNetwork::CUT_REACH > RoadNetwork::CUT_FLAT);
        }
        let (_, _, roads) = shipped_roads();
        let (a, b) = roads.segments().nth(300).unwrap();
        let mid = (a + b) * 0.5;
        let left = Vec2::new(-(b - a).y, (b - a).x) * (1.0 / a.distance(b));
        assert_eq!(
            roads.surface(mid + left * (RoadNetwork::HALF_WIDTH - 0.6)),
            1.0
        );
    }

    #[test]
    fn roads_are_more_direct_than_the_railway() {
        let (towns, line, roads) = shipped_roads();
        let path = line.full_path(&towns);
        for (i, road) in between(&roads) {
            let by_rail = (path.project(road.ends[0].at) - path.project(road.ends[1].at)).abs();
            let by_road = roads.length(i);
            assert!(
                by_road < by_rail,
                "{}: road {by_road} m, rail {by_rail} m",
                towns[road.from].name
            );
            assert_eq!(roads.length_between(road.to, road.from), Some(by_road));
        }
        assert_eq!(roads.length_between(1, 2), None);
    }

    #[test]
    fn every_railway_town_has_one_square_level_crossing_beside_its_station() {
        let (towns, _, roads) = shipped_roads();
        for (i, town) in towns.iter().enumerate() {
            let crossings: Vec<&LevelCrossing> = roads
                .roads
                .iter()
                .filter(|r| r.from == i)
                .flat_map(|r| r.crossings.iter())
                .collect();
            assert_eq!(crossings.len(), 1, "{}", town.name);
            let rural: Vec<(usize, &Road)> = roads
                .roads
                .iter()
                .enumerate()
                .filter(|(_, r)| r.kind == RoadKind::Rural && r.from == i)
                .collect();
            let [(index, road)] = rural[..] else {
                panic!("{}: {} rural tracks", town.name, rural.len())
            };
            let crossing = crossings[0];
            assert_eq!(road.crossings[0], *crossing);
            assert!(
                crossing.angle() > 80f32.to_radians(),
                "{}",
                crossing.angle()
            );
            let station = town
                .buildings
                .iter()
                .find(|b| b.kind == crate::BuildingKind::Station)
                .unwrap();
            assert!(
                crossing.at.distance(station.footprint.center()) < 160.0,
                "{}",
                town.name
            );
            // Sale de la cuadrícula del pueblo, cruza la playa en perpendicular
            // y llega a campo abierto.
            assert!(on_edge(town.grid(), road.path[0]), "{:?}", road.path[0]);
            let rail = town.railway.unwrap();
            assert!(crate::road::segment_distance(crossing.at, rail.start, rail.end) < 1e-3);
            for p in &road.path {
                for b in &town.buildings {
                    assert!(
                        !b.footprint
                            .expanded(RoadNetwork::HALF_WIDTH)
                            .contains_point(*p),
                        "{:?}",
                        b.kind
                    );
                }
            }
            let length = roads.length(index);
            assert!((300.0..=850.0).contains(&length), "{length}");
            let tip = road.ends[1].at;
            assert!(!town.leveled().contains_point(tip));
        }
    }

    /// Dos pueblos con ferrocarril a 3 km de distancia, con la vía del de oeste
    /// al norte del pueblo y la del de este al sur: el camino entre ellos tiene
    /// que cruzar la línea.
    fn crossing_world() -> (Vec<TownLayout>, RailLine, RoadNetwork) {
        let town = |name: &str, x: f32, side| TownConfig {
            name: name.into(),
            origin: Vec2::new(x, 0.0),
            blocks: (4, 4),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig {
                side,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 3,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        let world = WorldConfig::default();
        let (towns, errors) = place_towns(
            &world,
            &[
                town("Oeste", -1500.0, Side::North),
                town("Este", 1500.0, Side::South),
            ],
        );
        assert!(errors.is_empty(), "{errors:?}");
        let (line, errors) = RailLine::build(None, &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        let (roads, errors) = RoadNetwork::build(&towns, &line, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line, roads)
    }

    #[test]
    fn rail_crossings_are_found_exactly_and_crossed_square() {
        let (towns, line, roads) = crossing_world();
        let track = track(&towns, &line);
        let (_, road) = between(&roads)[0];
        assert_eq!(road.crossings.len(), 1, "{:?}", road.crossings);
        // Cada paso a nivel está sobre las dos líneas centrales, y no se cruzan
        // en ningún otro lugar.
        let mut brute = 0;
        for w in road.path.windows(2) {
            for r in track.pieces() {
                brute += usize::from(intersect(w[0], w[1], r[0], r[1]).is_some());
            }
        }
        assert_eq!(brute, road.crossings.len());
        for c in &road.crossings {
            let on_road = road
                .path
                .windows(2)
                .map(|w| segment_distance(c.at, w[0], w[1]));
            let on_rail = track.pieces().map(|w| segment_distance(c.at, w[0], w[1]));
            assert!(on_road.fold(f32::INFINITY, f32::min) < 1e-3);
            assert!(on_rail.fold(f32::INFINITY, f32::min) < 1e-3);
            assert!(c.angle() > 70f32.to_radians(), "{}", c.angle().to_degrees());
            assert!(c.along > 0.0 && c.along < roads.length_between(0, 1).unwrap());
            // Sus señales están fuera del camino y fuera de la vía.
            for spot in c.sign_spots() {
                let gap = roads.distance(spot);
                assert!(gap > RoadNetwork::HALF_WIDTH + 0.5, "{gap}");
                assert!(track.signed(spot).0.abs() > 4.0);
            }
        }
        // Lejos del paso a nivel, el camino se mantiene lejos del riel.
        for p in &road.path {
            let near_crossing = road.crossings.iter().any(|c| c.at.distance(*p) < 130.0);
            if !near_crossing {
                assert!(
                    track.signed(*p).0.abs() > 0.9 * RoadNetwork::RAIL_CLEARANCE,
                    "{p:?}"
                );
            }
        }
    }

    /// Un camino rural corto del primer pueblo distribuido a lo largo de
    /// `path`, comprobado.
    fn checked(path: Vec<Vec2>) -> Result<(), RoadError> {
        let (towns, line, _) = shipped_roads();
        let track = track(&towns, &line);
        let crossings = crossings_of(&path, &track);
        let road = Road {
            kind: RoadKind::Rural,
            from: 0,
            to: 0,
            ends: [
                RoadEnd {
                    at: path[0],
                    out: Vec2::new(1.0, 0.0),
                },
                RoadEnd {
                    at: path[path.len() - 1],
                    out: Vec2::new(1.0, 0.0),
                },
            ],
            path,
            crossings,
        };
        check(&road, &towns, big_world().bounds(), &track)
    }

    /// Un punto y la dirección unitaria de la vía en ese punto, bien afuera
    /// entre Monte Nievas y General Pico.
    fn open_track() -> (Vec2, Vec2) {
        let (towns, line, _) = shipped_roads();
        let path = line.full_path(&towns);
        let d = path.length() * 0.7;
        (path.at(d), path.direction_at(d))
    }

    #[test]
    fn a_road_crossing_the_track_at_a_slant_or_hugging_it_is_left_out() {
        let (at, dir) = open_track();
        let slant = Vec2::new(dir.x - dir.y, dir.y + dir.x) * std::f32::consts::FRAC_1_SQRT_2;
        let error = checked(vec![at - slant * 40.0, at + slant * 40.0]).expect_err("45°");
        assert!(
            matches!(error, RoadError::UnsquareCrossing { .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains("45"), "{error}");
        let left = Vec2::new(-dir.y, dir.x);
        let along = |s: f32| at + dir * s + left * 12.0;
        let error = checked(vec![along(-30.0), along(0.0), along(30.0)]).expect_err("hugging");
        assert!(matches!(error, RoadError::NearRail { .. }), "{error:?}");
        // En perpendicular, y bien lejos: está bien.
        assert_eq!(checked(vec![at - left * 60.0, at + left * 60.0]), Ok(()));
        assert_eq!(
            checked(vec![along(-30.0) + left * 50.0, along(30.0) + left * 50.0]),
            Ok(())
        );
    }

    #[test]
    fn a_rural_track_with_no_way_across_the_yard_is_reported() {
        let (mut towns, line, _) = shipped_roads();
        // Llenar el lado del ferrocarril del primer pueblo con un solo galpón
        // largo, desde el borde de la cuadrícula hasta más allá de la vía, a lo
        // largo de todo el tramo final.
        let rail = towns[0].railway.unwrap();
        let mut shed = towns[0].buildings[0];
        shed.footprint = Aabb::new(
            Vec2::new(rail.start.x.min(rail.end.x), rail.start.y.min(rail.end.y)),
            Vec2::new(rail.start.x.max(rail.end.x), rail.start.y.max(rail.end.y)),
        )
        .expanded(30.0);
        towns[0].buildings.push(shed);
        let (roads, errors) = RoadNetwork::build(&towns, &line, big_world().bounds());
        let name = towns[0].name.clone();
        assert!(
            errors.contains(&RoadError::NoRuralCrossing { town: name.clone() }),
            "{errors:?}"
        );
        assert!(errors.iter().any(|e| e.to_string().contains(&name)));
        assert!(
            roads
                .roads
                .iter()
                .all(|r| !(r.kind == RoadKind::Rural && r.from == 0))
        );
    }

    #[test]
    fn the_index_finds_the_exact_distance_near_roads() {
        let (_, _, roads) = shipped_roads();
        let mut rng = Rng::new(4, 4);
        let mut near = 0;
        for (a, b) in roads.segments().step_by(17) {
            for _ in 0..4 {
                let p = a
                    + (b - a) * rng.unit()
                    + Vec2::new(rng.range(-70.0, 70.0), rng.range(-70.0, 70.0));
                let exact = roads.distance(p);
                match roads.near(p, 40.0) {
                    Some(d) => {
                        assert!((d - exact).abs() < 1e-4, "{p:?}");
                        near += 1;
                    }
                    None => assert!(exact > 40.0, "{p:?}: {exact}"),
                }
                let surface = roads.surface(p);
                assert!((0.0..=1.0).contains(&surface));
                if exact < RoadNetwork::HALF_WIDTH - 0.5 {
                    assert_eq!(surface, 1.0);
                }
                if exact > RoadNetwork::HALF_WIDTH + 1.5 {
                    assert_eq!(surface, 0.0);
                }
            }
        }
        assert!(near > 100, "{near}");
        assert_eq!(RoadNetwork::default().near(Vec2::ZERO, 10.0), None);
    }

    #[test]
    fn signposts_name_the_far_town_and_its_distance_off_the_road() {
        assert_eq!(
            board_text("Eduardo Castex", 2349.0),
            "EDUARDO CASTEX 2,3 KM"
        );
        assert_eq!(board_text("General Pico", 6951.0), "GENERAL PICO 7,0 KM");
        assert_eq!(board_text("Monte Nievas", 949.0), "MONTE NIEVAS 0,9 KM");
        let (towns, _, roads) = shipped_roads();
        assert_eq!(roads.signposts.len(), 4);
        for (i, road) in between(&roads) {
            let km = (roads.length(i) / 100.0).round() / 10.0;
            for (end, far) in [(road.ends[0], road.to), (road.ends[1], road.from)] {
                let post = roads
                    .signposts
                    .iter()
                    .find(|s| s.at.distance(end.at) < 20.0)
                    .expect("a signpost at every road end");
                let board = &post.boards[0];
                let name = towns[far].name.to_uppercase();
                assert!(board.text.starts_with(&name), "{}", board.text);
                let shown: f32 = board.text[name.len()..]
                    .trim()
                    .trim_end_matches(" KM")
                    .replace(',', ".")
                    .parse()
                    .unwrap();
                assert!((shown - km).abs() < 0.05, "{} vs {km}", board.text);
                assert_eq!(board.toward, end.out);
                assert!(roads.distance(post.at) > RoadNetwork::HALF_WIDTH + 1.0);
            }
        }
        // Los postes y las señales de paso a nivel son obstáculos delgados,
        // ninguno sobre un camino.
        let obstacles = roads.obstacles();
        assert_eq!(obstacles.len(), 4 + 2 * roads.crossings().count());
        for o in &obstacles {
            assert!(o.size().x <= RoadNetwork::POST + 1e-6);
            assert!(roads.distance(o.center()) > RoadNetwork::HALF_WIDTH + 0.5);
        }
    }

    #[test]
    fn chunk_parts_add_up_to_the_whole_road_and_meet_at_borders() {
        let (_, _, roads) = shipped_roads();
        let size = 200.0;
        let mut total = 0.0;
        let mut ends = Vec::new();
        for y in -20..20 {
            for x in -40..40 {
                let area = crate::ChunkCoord::new(x, y).bounds(size);
                for part in roads.parts_in(area) {
                    assert!(part.len() > 1);
                    for w in part.windows(2) {
                        total += w[0].at.distance(w[1].at);
                        assert!(w[1].along >= w[0].along);
                    }
                    for p in &part {
                        assert!(area.expanded(1e-2).contains_point(p.at));
                        assert!((p.normal.length() - 1.0).abs() < 1e-4);
                    }
                    ends.push(part[0].at);
                    ends.push(part[part.len() - 1].at);
                }
            }
        }
        let whole: f32 = (0..roads.roads.len()).map(|i| roads.length(i)).sum();
        assert!((total - whole).abs() < 0.5, "{total} vs {whole}");
        // Cada extremo de parte es un extremo de camino o se une con el extremo
        // de otra parte.
        let road_ends: Vec<Vec2> = roads
            .roads
            .iter()
            .flat_map(|r| r.ends.map(|e| e.at))
            .collect();
        for e in &ends {
            let shared = ends.iter().filter(|o| o.distance(*e) < 1e-2).count() > 1;
            assert!(
                shared || road_ends.iter().any(|r| r.distance(*e) < 1e-2),
                "{e:?}"
            );
        }
    }

    #[test]
    fn a_public_road_runs_from_the_nearest_town_to_the_lagoon_bank() {
        let (towns, line, roads) = shipped_roads();
        let terrain = crate::Terrain::default();
        let config = crate::lagoon::tests::arocena_config();
        let (lagoons, errors) = crate::place_lagoons(
            &big_world(),
            &terrain,
            &towns,
            &line,
            &roads,
            std::slice::from_ref(&config),
        );
        assert!(errors.is_empty(), "{errors:?}");
        let lagoon = &lagoons[0];
        let before = roads.roads.len();
        let (roads, errors) = roads.with_lagoons(&towns, &line, big_world().bounds(), &lagoons);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(roads.roads.len(), before + 1);
        let road = roads.roads.last().unwrap();
        assert_eq!(road.kind, RoadKind::Lagoon);
        // Desde General Pico, el pueblo más cercano, saliendo del borde de su
        // cuadrícula del lado de la laguna.
        let pico = towns.iter().position(|t| t.name == "General Pico").unwrap();
        assert_eq!((road.from, road.to), (pico, pico));
        let grid = towns[pico].grid();
        assert!(on_edge(grid, road.path[0]), "{:?}", road.path[0]);
        assert!(road.ends[0].out.dot(lagoon.center - grid.center()) > 0.0);
        // Nunca cruza la vía, y se mantiene lejos de ella fuera del pueblo.
        assert!(road.crossings.is_empty());
        // Termina en la orilla: su borde llega hasta el barro húmedo y no más,
        // y su retorno queda cerca del agua.
        for &p in &road.path {
            let off = lagoon.offshore(p);
            assert!(
                off >= Lagoon::MUD_BAND + RoadNetwork::HALF_WIDTH - 0.5,
                "{p:?}: {off}"
            );
        }
        let nearest = road
            .path
            .iter()
            .map(|&p| lagoon.offshore(p))
            .fold(f32::MAX, f32::min);
        assert!(
            nearest < Lagoon::MUD_BAND + RoadNetwork::HALF_WIDTH + 2.0,
            "{nearest}"
        );
        // Solo su extremo entra en la zona prohibida de la laguna: el resto del
        // camino se mantiene lejos de la barranca y de su margen.
        let keep = lagoon.keep_out();
        let entered = road
            .path
            .iter()
            .position(|&p| keep.contains_point(p))
            .unwrap();
        assert!(
            road.path[entered..]
                .iter()
                .all(|&p| keep.expanded(1.0).contains_point(p))
        );
        // Un lazo de retorno al final, y un cartel indicador que nombra la
        // laguna.
        let tail = &road.path[entered..];
        let span = tail.iter().fold(0.0_f32, |m, a| {
            tail.iter().fold(m, |m, b| m.max(a.distance(*b)))
        });
        assert!(span > RoadNetwork::TURNAROUND, "{span}");
        assert!(roads.signposts.iter().any(|s| {
            s.boards
                .iter()
                .any(|b| b.text.starts_with("LAGUNA LA AROCENA "))
        }));
        // Se traza igual siempre.
        let (again, _) =
            shipped_roads()
                .2
                .with_lagoons(&towns, &line, big_world().bounds(), &lagoons);
        assert_eq!(again, roads);
    }

    #[test]
    fn a_branch_town_gets_a_road_to_its_nearest_town_and_crossings_on_the_branch() {
        let (towns, list, branches) = crate::rail::tests::branched();
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        // Norte, en el ramal desde Medio, queda más cerca de Oeste.
        assert_eq!(road_pairs(&towns, &line), [(0, 1), (1, 2), (0, 3)]);
        let (roads, errors) = RoadNetwork::build(&towns, &line, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        let road = roads
            .roads
            .iter()
            .find(|r| r.kind == RoadKind::Between && (r.from, r.to) == (0, 3))
            .expect("a road from Oeste to Norte");
        // Cruza en perpendicular el tramo final de Oeste en su camino hacia el
        // norte.
        assert_eq!(road.crossings.len(), 1, "{:?}", road.crossings);
        assert!(road.crossings[0].angle() > RoadNetwork::MIN_CROSSING_ANGLE);
        // Cada extremo tiene su cartel indicador que nombra el otro pueblo.
        let named = |name: &str| {
            roads
                .signposts
                .iter()
                .flat_map(|s| &s.boards)
                .any(|b| b.text.starts_with(name))
        };
        assert!(named("NORTE") && named("OESTE"));
        // La huella rural de Norte cruza el ramal en un paso a nivel, y ningún
        // camino se encuentra con el ramal en ningún lugar sin uno.
        let branch = &line.branch_paths(&towns)[0];
        let rural = roads
            .roads
            .iter()
            .find(|r| r.kind == RoadKind::Rural && r.from == 3)
            .expect("Norte's rural track");
        assert_eq!(rural.crossings.len(), 1);
        for road in &roads.roads {
            let mut brute = Vec::new();
            for w in road.path.windows(2) {
                for r in branch.points().windows(2) {
                    if let Some((t, _)) = intersect(w[0], w[1], r[0], r[1]) {
                        brute.push(w[0] + (w[1] - w[0]) * t);
                    }
                }
            }
            for at in brute {
                assert!(
                    road.crossings.iter().any(|c| c.at.distance(at) < 1e-2),
                    "{:?} meets the branch at {at:?}",
                    road.kind
                );
            }
        }
    }
}
