//! Asentamientos rurales detrás de las tranqueras de los campos (usuario,
//! 2026-10-22): los ranchos, puestos, chacras y estancias de La Pampa de 1919.
//!
//! Algunas tranqueras abiertas al costado del camino ([`crate::Gate`]) llevan a un
//! campo poblado. Detrás de una de esas tranqueras, dentro de su celda del
//! relevamiento y lejos de los caminos, la vía férrea, los pueblos y los
//! alambrados, hay un conjunto de edificaciones con semilla: las casas alrededor
//! de un patio de tierra (el centro), con un galpón, corrales, un molino con su
//! tanque y su bebedero, parvas y los lugares donde crecerán sus árboles (un
//! conjunto planta un tercio más de los que corresponden a su tipo, con un flujo
//! aleatorio propio: [`Settlement::MORE_TREES`]; después cada tipo conserva su
//! proporción, [`Settlement::tree_keep`]).
//!
//! Huellas de tierra lo unen con la tranquera: un tronco desde justo dentro de la
//! tranquera hasta el centro, y un ramal desde el centro hasta la puerta de cada
//! casa, galpón y corral. Las huellas son dos surcos de ruedas, no caminos: quedan
//! fuera de [`crate::RoadNetwork`], sin alambrados, carteles indicadores ni corte
//! en el terreno, y no cruzan ningún alambrado salvo por su propia tranquera.
//!
//! Todo está alineado con los ejes, como la cuadrícula del relevamiento. Las
//! casas, los galpones, los tanques y los elementos decorativos son obstáculos
//! rectangulares; un corral son sus tramos de baranda con una abertura, para que
//! un jinete pueda entrar; un lugar de árbol solo bloquea con su tronco, y el
//! render hace crecer el árbol en él. La flora silvestre se mantiene fuera de los
//! conjuntos y de las huellas mediante un [`KeepOut`] indexado.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use crate::rng::Rng;
use crate::road::segment_distance;
use crate::{Aabb, FenceLayout, RailLine, RoadNetwork, Side, Terrain, TownLayout, Vec2};

/// Qué es un asentamiento, del más pobre al más rico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementKind {
    /// Un rancho: una pequeña casa de adobe, quizás un corral, y un pozo.
    Hut,
    /// Un puesto: la choza de un puestero en las tierras de una estancia, con un
    /// corral y una aguada (un bebedero, a veces un molino).
    Outpost,
    /// Una chacra: una granja con una casa de ladrillo o adobe, un galpón, un molino con
    /// su tanque y su bebedero, un corral y parvas.
    Farm,
    /// Una estancia: su casco (la casa principal) con su galería, las casas de sus
    /// peones, galpones, corrales, la aguada y un monte, repartidos en hectáreas.
    Ranch,
}

/// Cómo está construida una casa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HouseStyle {
    /// Paredes de adobe bajo un techo de paja.
    AdobeThatch,
    /// Paredes de adobe bajo un techo de chapa ondulada.
    AdobeIron,
    /// Ladrillo encalado con techo plano detrás de un parapeto.
    BrickParapet,
    /// El casco italianizante de una estancia con una galería a lo largo del
    /// frente.
    GalleryHouse,
}

/// Un edificio o elemento decorativo de un asentamiento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructureKind {
    MainHouse(HouseStyle),
    /// Una casa de peones, o la casa del casero.
    WorkerHouse(HouseStyle),
    /// Un galpón: un cobertizo de chapa para herramientas, carros y cereal.
    Barn,
    /// Corral de postes y barandas, abierto en su lado frontal.
    Corral,
    /// Un molino (bomba de viento): su huella es la base de la torre.
    Windpump,
    /// El tanque "australiano" redondo de chapa ondulada que llena el molino.
    Tank,
    /// El bebedero del que toma agua el ganado.
    Trough,
    /// Un brocal de pozo de ladrillo con su armazón.
    Well,
    /// Una parva de heno.
    Haystack,
    /// Un palenque para atar caballos.
    HitchingPost,
    /// Una carreta: un carro de bueyes de dos ruedas dejado en el patio.
    Cart,
    /// Un carro abandonado de cuatro ruedas (una chata rota) pudriéndose en el
    /// borde de un patio o en la banquina de un camino: escenografía, un
    /// obstáculo chico.
    WreckedWagon,
    /// Donde crece un eucalipto del monte; solo bloquea su tronco.
    Eucalyptus,
    /// Donde crece un álamo de la alameda de entrada; solo bloquea su tronco.
    Poplar,
}

impl StructureKind {
    /// Una casa en la que vive gente.
    pub fn is_house(self) -> bool {
        matches!(self, Self::MainHouse(_) | Self::WorkerHouse(_))
    }

    /// Si una huella lleva hasta su puerta.
    pub fn is_reached(self) -> bool {
        self.is_house() || matches!(self, Self::Barn | Self::Corral)
    }

    /// Un lugar de árbol y no una estructura.
    pub fn is_tree(self) -> bool {
        matches!(self, Self::Eucalyptus | Self::Poplar)
    }

    /// Todos los nombres de ranura de modelo que da [`Self::slot`]: nombres de
    /// carpeta en minúsculas dentro de `assets/models/third-party/`.
    pub const SLOTS: [&'static str; 9] = [
        "windpump",
        "eucalyptus",
        "poplar",
        "well",
        "trough",
        "cart",
        "haystack",
        "hitching_post",
        "wrecked_wagon",
    ];

    /// La ranura de modelo desde la que un glTF descargado reemplaza el mesh
    /// procedural (`assets/models/third-party/<slot>/`), para los elementos que
    /// tienen una.
    pub fn slot(self) -> Option<&'static str> {
        Some(match self {
            Self::Windpump => "windpump",
            Self::Eucalyptus => "eucalyptus",
            Self::Poplar => "poplar",
            Self::Well => "well",
            Self::Trough => "trough",
            Self::Cart => "cart",
            Self::Haystack => "haystack",
            Self::HitchingPost => "hitching_post",
            Self::WreckedWagon => "wrecked_wagon",
            _ => return None,
        })
    }
}

/// Un edificio o elemento decorativo: una huella alineada con los ejes, su
/// altura y el lado hacia el que mira su frente (puerta, galería, abertura del
/// corral): hacia el centro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Structure {
    pub kind: StructureKind,
    pub footprint: Aabb,
    /// Altura de la pared, o la altura del elemento o del árbol, en metros.
    pub height: f32,
    pub front: Side,
    /// Variante con semilla (el render elige con ella una forma o una textura).
    pub variant: u32,
}

impl Structure {
    /// El punto a [`Settlement::DOOR_REACH`] metros hacia afuera del centro de
    /// su frente: donde termina su huella.
    pub fn door(&self) -> Vec2 {
        let (fp, c) = (self.footprint, self.footprint.center());
        let reach = Settlement::DOOR_REACH;
        match self.front {
            Side::North => Vec2::new(c.x, fp.max.y + reach),
            Side::South => Vec2::new(c.x, fp.min.y - reach),
            Side::East => Vec2::new(fp.max.x + reach, c.y),
            Side::West => Vec2::new(fp.min.x - reach, c.y),
        }
    }

    /// El radio del tronco de un lugar de árbol a nivel del suelo, en metros:
    /// el tronco grueso y claro de un eucalipto, el delgado de un álamo. 0 para
    /// cualquier otra cosa.
    pub fn trunk_radius(&self) -> f32 {
        match self.kind {
            StructureKind::Eucalyptus => (0.016 * self.height).clamp(0.2, 0.45),
            StructureKind::Poplar => (0.012 * self.height).clamp(0.12, 0.25),
            _ => 0.0,
        }
    }

    /// Sus obstáculos estáticos: la huella; los tramos de baranda de un corral
    /// con [`Settlement::CORRAL_GATE`] metros abiertos en el centro de su
    /// frente; el tronco de un lugar de árbol, un cuadrado de
    /// [`Self::trunk_radius`] hacia cada lado.
    pub fn obstacles(&self) -> Vec<Aabb> {
        match self.kind {
            k if k.is_tree() => {
                let side = 2.0 * self.trunk_radius();
                vec![Aabb::from_center_size(
                    self.footprint.center(),
                    Vec2::new(side, side),
                )]
            }
            StructureKind::Corral => corral_rails(self.footprint, self.front),
            _ => vec![self.footprint],
        }
    }
}

/// Una huella interna: dos surcos de ruedas a lo largo de `points`, a
/// [`Track::HALF_WIDTH`] metros hacia cada lado.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub points: Vec<Vec2>,
}

impl Track {
    /// La mitad del ancho gastado, en metros (2,6 m en total).
    pub const HALF_WIDTH: f32 = 1.3;

    /// Los tramos rectos, en orden.
    pub fn segments(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.points.windows(2).map(|w| (w[0], w[1]))
    }
}

/// Un asentamiento detrás de una tranquera.
#[derive(Debug, Clone, PartialEq)]
pub struct Settlement {
    pub kind: SettlementKind,
    /// La tranquera abierta detrás de la que está (un índice dentro de
    /// [`FenceLayout::gates`]).
    pub gate: u32,
    /// El centro del patio, donde se encuentran las huellas.
    pub hub: Vec2,
    /// El patio de tierra pisoteada alrededor del centro.
    pub patio: Aabb,
    pub structures: Vec<Structure>,
    /// Primero el tronco desde la tranquera hasta el centro y después un ramal
    /// desde el centro hasta cada puerta.
    pub tracks: Vec<Track>,
    /// Todo lo que cubre: huellas de edificios, patio y huellas de tierra.
    pub bounds: Aabb,
    /// Semilla para la variedad propia del render.
    pub seed: u32,
}

impl Settlement {
    /// A qué distancia hacia afuera de un frente termina su huella, en metros.
    pub const DOOR_REACH: f32 = 3.5;
    /// Ancho de la abertura de un corral, en metros.
    pub const CORRAL_GATE: f32 = 4.0;
    /// Espesor del obstáculo de una baranda de corral, en metros.
    pub const CORRAL_RAIL: f32 = 0.2;
    /// Distancia mínima desde una huella de edificio hasta la línea central de
    /// un camino, en metros: bien detrás del alambrado al costado del camino.
    pub const ROAD_CLEARANCE: f32 = 25.0;
    /// Distancia mínima desde un galpón hasta un camino, en metros: bastante
    /// más que el despeje común, para que un visitante llegue primero al casco
    /// y los galpones queden lejos del costado del camino. El índice de caminos
    /// ve 64 m ([`RoadNetwork::INDEX_REACH`]) y `off_roads_by` agrega 2,5 m,
    /// así que esto es lo máximo que se puede comprobar.
    pub const BARN_ROAD_CLEARANCE: f32 = 60.0;
    /// Distancia mínima desde una huella de edificio hasta la línea central de
    /// la vía, en metros.
    pub const RAIL_CLEARANCE: f32 = 40.0;
    /// Distancia mínima desde una huella de edificio o una huella de tierra
    /// hasta la franja nivelada de un pueblo.
    pub const TOWN_CLEARANCE: f32 = 60.0;
    /// Distancia mínima desde una huella de edificio hasta las líneas del
    /// relevamiento de su celda (los alambrados de la cuadrícula), en metros.
    pub const FENCE_CLEARANCE: f32 = 12.0;
    /// Distancia mínima desde la línea central de una huella hasta las líneas
    /// del relevamiento.
    pub const TRACK_FENCE_CLEARANCE: f32 = 4.0;
    /// Distancia mínima desde la línea central de una huella hasta la de un
    /// camino, pasando su tranquera: sus surcos se mantienen lejos del
    /// alambrado al costado del camino.
    pub const TRACK_ROAD_CLEARANCE: f32 = FenceLayout::ROADSIDE + Track::HALF_WIDTH + 1.0;
    /// Hasta qué distancia de su tranquera el tronco de una huella puede seguir
    /// cerca del camino.
    pub const GATE_APPROACH: f32 = 6.0;
    /// Espacio libre mínimo entre dos huellas de edificio, en metros: cabe un
    /// jinete.
    pub const GAP: f32 = 3.0;
    /// Distancia mínima desde la línea central de una huella hasta una huella
    /// de edificio, en metros.
    pub const CORRIDOR: f32 = Track::HALF_WIDTH + 1.0;
    /// Distancia mínima entre los centros de dos asentamientos, en metros.
    pub const MIN_SPACING: f32 = 300.0;
    /// Eucaliptos extra que planta un conjunto, como proporción de los árboles
    /// que corresponden a su tipo (la cantidad se redondea al azar).
    pub const MORE_TREES: f32 = 0.35;
    /// La proporción de sus árboles que conserva un conjunto de tipo `kind`
    /// (usuario, 2026-10-27: 30 % menos, después "chacras y estancias sacale
    /// del total que tiene ahora, un 30%", y después también puestos y ranchos:
    /// "se desmonta para huerto, animales, plantaciones"): ranchos, puestos y
    /// chacras 0,7, estancias 0,49 (0,7 x 0,7).
    pub fn tree_keep(kind: SettlementKind) -> f32 {
        match kind {
            SettlementKind::Hut | SettlementKind::Outpost | SettlementKind::Farm => 0.7,
            SettlementKind::Ranch => 0.49,
        }
    }
    /// Metros entre los pares de la alameda de una estancia, plantada como
    /// cortina rompevientos (de 4 a 6 m).
    pub const POPLAR_SPACING: f32 = 6.0;
    /// A qué distancia de la línea central del tronco está cada fila de la
    /// alameda.
    pub const POPLAR_OFFSET: f32 = 5.0;
    /// Cuánto más allá del primer punto del tronco (medio metro dentro de la
    /// tranquera) está el último par de la alameda: unos 4 m dentro de la
    /// tranquera.
    pub const POPLAR_GATE_INSET: f32 = 3.5;
    /// Superficie mínima que ocupa el conjunto de una estancia, en metros
    /// cuadrados.
    pub const RANCH_AREA: f32 = 20_000.0;
    /// Probabilidad de que haya un carro abandonado en una chacra y en una
    /// estancia.
    pub const WRECKS: (f32, f32) = (0.3, 0.5);
    /// La huella de un carro abandonado: ancha a lo largo de su frente,
    /// profunda hacia afuera de él, en metros.
    pub const WRECK_SIZE: (f32, f32) = (4.8, 3.6);

    /// Todas las estructuras de tipo `kind`.
    pub fn of(&self, kind: StructureKind) -> impl Iterator<Item = &Structure> + '_ {
        self.structures.iter().filter(move |s| s.kind == kind)
    }

    /// La huella troncal, desde justo dentro de la tranquera hasta el centro.
    pub fn trunk(&self) -> &Track {
        &self.tracks[0]
    }
}

/// Todos los asentamientos del mundo, con la zona prohibida que respeta la
/// flora silvestre.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettlementLayout {
    pub settlements: Vec<Settlement>,
    pub(crate) keep_out: Arc<KeepOut>,
    /// Los pueblos para los que se pidió una estancia y no obtuvieron ninguna
    /// (ver [`Self::build_with_ranches`]): quien llama avisa sobre ellos.
    pub unplaced_ranches: Vec<usize>,
    /// Carros abandonados en las banquinas de los caminos, entre el borde del
    /// camino y su alambrado: unos pocos en todo el mapa (ver
    /// [`Self::ROADSIDE_WRECKS`]).
    pub roadside: Vec<RoadsideWreck>,
}

/// Un carro abandonado en la banquina de un camino, a lo largo del camino.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadsideWreck {
    /// El centro de su huella.
    pub at: Vec2,
    /// Dirección unitaria del camino que está al lado: su largo corre en esa
    /// dirección.
    pub along: Vec2,
    /// Variante con semilla (el render lo inclina con ella).
    pub variant: u32,
}

impl RoadsideWreck {
    /// Lado de cada uno de los rectángulos que bloquean a lo largo de su
    /// cuerpo, en metros.
    pub const BLOCK: f32 = 2.0;

    /// Las esquinas de su huella girada, [`Settlement::WRECK_SIZE`]: larga a lo
    /// largo del camino, profunda a lo ancho.
    pub fn corners(&self) -> [Vec2; 4] {
        let (wide, deep) = Settlement::WRECK_SIZE;
        let (a, c) = (self.along * (wide / 2.0), self.across() * (deep / 2.0));
        [
            self.at - a - c,
            self.at + a - c,
            self.at + a + c,
            self.at - a + c,
        ]
    }

    /// Dirección unitaria transversal al camino (un cuarto de vuelta desde
    /// `along`).
    pub fn across(&self) -> Vec2 {
        Vec2::new(-self.along.y, self.along.x)
    }

    /// El rectángulo que rodea su huella girada (la flora se mantiene fuera de
    /// él).
    pub fn footprint(&self) -> Aabb {
        let [a, b, c, d] = self.corners();
        Aabb::new(
            Vec2::new(
                a.x.min(b.x).min(c.x).min(d.x),
                a.y.min(b.y).min(c.y).min(d.y),
            ),
            Vec2::new(
                a.x.max(b.x).max(c.x).max(d.x),
                a.y.max(b.y).max(c.y).max(d.y),
            ),
        )
    }

    /// Lo que bloquea al jinete: tres rectángulos de [`Self::BLOCK`] a lo largo
    /// de su cuerpo.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let reach = Settlement::WRECK_SIZE.0 / 2.0 - Self::BLOCK / 2.0;
        [-reach, 0.0, reach]
            .into_iter()
            .map(|t| {
                Aabb::from_center_size(
                    self.at + self.along * t,
                    Vec2::new(Self::BLOCK, Self::BLOCK),
                )
            })
            .collect()
    }
}

impl SettlementLayout {
    /// Proporción de tranqueras abiertas (hacia un campo sin poblar) que llevan
    /// a un asentamiento cerca de un pueblo, y lejos de uno.
    pub const SETTLED: (f32, f32) = (0.6, 0.45);
    /// Distancia desde la franja nivelada de un pueblo dentro de la cual una
    /// tranquera cuenta como cercana al pueblo (ahí se agrupan las chacras), en
    /// metros.
    pub const NEAR_TOWN: f32 = 3000.0;
    /// Intentos de centro por tranquera.
    const HUB_TRIES: usize = 16;
    /// Intentos de centro por tranquera para una estancia forzada: necesita un
    /// campo grande.
    pub const RANCH_TRIES: usize = 64;
    /// A qué distancia de la franja nivelada de un pueblo puede estar la
    /// tranquera de una estancia forzada, en metros: hasta 3 km, y después en
    /// pasos más amplios cuando no entra nada.
    pub const RANCH_REACH: [f32; 4] = [3000.0, 6000.0, 12_000.0, f32::INFINITY];
    /// Distancia desde la tranquera hasta el final del primer tramo recto del
    /// tronco, en metros.
    const ENTRY: f32 = 12.0;
    /// Máximo de carros abandonados en las banquinas de los caminos, en todo el
    /// mapa.
    pub const ROADSIDE_WRECKS: usize = 5;
    /// Probabilidad de que un tramo de alambrado al costado del camino tenga un
    /// carro abandonado.
    const ROADSIDE_CHANCE: f32 = 0.15;
    /// Distancia mínima entre dos carros abandonados al costado del camino, en
    /// metros.
    const ROADSIDE_SPACING: f32 = 1500.0;
    /// Separación entre un carro abandonado y el borde del camino, en metros.
    const VERGE_GAP: f32 = 0.4;

    /// Los asentamientos detrás de las tranqueras abiertas de `fences`, sobre
    /// `terrain` (con sus caminos trazados), lejos de los pueblos de `towns` y
    /// de toda la vía de `line`. Con semilla por tranquera: siempre iguales.
    pub fn build(
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
        fences: &FenceLayout,
    ) -> Self {
        Self::build_with_ranches(terrain, towns, line, fences, &[])
    }

    /// Como [`Self::build`], con una estancia forzada cerca de cada pueblo de
    /// `ranch_towns` (índices dentro de `towns`), colocada antes de la mezcla
    /// con semilla para que los demás asentamientos mantengan su separación
    /// respecto de ella.
    ///
    /// Para cada uno de esos pueblos se prueban las tranqueras abiertas a
    /// [`Self::RANCH_REACH`] o menos de su franja nivelada, de la más cercana a
    /// la más lejana, cada una con hasta [`Self::RANCH_TRIES`] centros; gana la
    /// primera cuyo campo contenga una estancia completa (su casco, al menos
    /// dos casas de trabajadores, un galpón, un corral, la aguada, en más de
    /// [`Settlement::RANCH_AREA`]). Cuando no entra ninguna, el alcance se
    /// amplía paso a paso; un pueblo que sigue sin obtener ninguna se lista en
    /// [`Self::unplaced_ranches`].
    pub fn build_with_ranches(
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
        fences: &FenceLayout,
        ranch_towns: &[usize],
    ) -> Self {
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(line.track_paths(towns).iter().flat_map(|path| {
                path.points()
                    .windows(2)
                    .map(|w| (w[0], w[1]))
                    .collect::<Vec<_>>()
            }))
            .collect();
        // Pueblos y lagunas con sus barrancas.
        let bands: Vec<Aabb> = towns
            .iter()
            .map(|t| t.leveled().expanded(Settlement::TOWN_CLEARANCE))
            .chain(terrain.lagoons.iter().map(|l| l.keep_out()))
            .collect();
        let site = Site {
            terrain,
            fences,
            rails,
            bands,
        };
        let town_gap = |p: Vec2| {
            towns
                .iter()
                .map(|t| t.leveled().closest_point(p).distance(p))
                .fold(f32::INFINITY, f32::min)
        };
        let mut placed = Placed::default();
        let mut unplaced_ranches = Vec::new();
        for &town in ranch_towns {
            let Some(band) = towns.get(town).map(TownLayout::leveled) else {
                unplaced_ranches.push(town);
                continue;
            };
            let gap = |p: Vec2| band.closest_point(p).distance(p);
            let mut open: Vec<(f32, usize)> = fences
                .gates
                .iter()
                .enumerate()
                .filter(|(_, g)| g.open)
                .map(|(i, g)| (gap(g.at), i))
                .collect();
            open.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut done = false;
            let mut tried = 0;
            for reach in Self::RANCH_REACH {
                for &(_, index) in open.iter().filter(|(d, _)| *d <= reach).skip(tried) {
                    tried += 1;
                    let rng = Rng::new(terrain.seed ^ RANCH_SEED, index as u32);
                    let forced = Some((SettlementKind::Ranch, Self::RANCH_TRIES));
                    if site.settle_gate(index, rng, forced, &mut placed, &town_gap) {
                        done = true;
                        break;
                    }
                }
                if done {
                    break;
                }
            }
            if !done {
                unplaced_ranches.push(town);
            }
        }
        for (index, gate) in fences.gates.iter().enumerate() {
            if !gate.open {
                continue;
            }
            let rng = Rng::new(terrain.seed ^ SETTLEMENT_SEED, index as u32);
            site.settle_gate(index, rng, None, &mut placed, &town_gap);
        }
        let settlements = placed.settlements;
        let roadside = site.roadside_wrecks(terrain.seed, &settlements);
        let mut keep_out = KeepOut::default();
        for wreck in &roadside {
            keep_out.push(Shape::Area(wreck.footprint()));
        }
        for s in &settlements {
            keep_out.push(Shape::Area(s.patio));
            for st in &s.structures {
                keep_out.push(Shape::Area(st.footprint));
            }
            for h in &s.tracks {
                for (a, b) in h.segments() {
                    keep_out.push(Shape::Strip(a, b, Track::HALF_WIDTH));
                }
            }
        }
        Self {
            settlements,
            keep_out: Arc::new(keep_out),
            unplaced_ranches,
            roadside,
        }
    }

    /// Cuántos asentamientos son de tipo `kind`.
    pub fn count(&self, kind: SettlementKind) -> usize {
        self.settlements.iter().filter(|s| s.kind == kind).count()
    }

    /// Los obstáculos estáticos de cada asentamiento y de cada carro abandonado
    /// al costado del camino.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .flat_map(Structure::obstacles)
            .chain(self.roadside.iter().flat_map(RoadsideWreck::obstacles))
    }

    /// [`Self::obstacles`] en el mismo orden, cada uno con su altura en metros:
    /// la altura propia de una estructura (un caballo que salta supera un
    /// bebedero o un palenque bajos), sin límite para un carro abandonado al
    /// costado del camino.
    pub fn obstacle_heights(&self) -> impl Iterator<Item = (Aabb, f32)> + '_ {
        self.settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .flat_map(|s| s.obstacles().into_iter().map(|o| (o, s.height)))
            .chain(
                self.roadside
                    .iter()
                    .flat_map(RoadsideWreck::obstacles)
                    .map(|o| (o, f32::INFINITY)),
            )
    }

    /// Los asentamientos cuyo centro está en `area` (un chunk del terreno;
    /// bordes oeste y sur incluidos, este y norte excluidos), para que cada uno
    /// pertenezca a un solo chunk.
    pub fn in_area(&self, area: Aabb) -> impl Iterator<Item = &Settlement> + '_ {
        self.settlements.iter().filter(move |s| {
            let p = s.hub;
            p.x >= area.min.x && p.x < area.max.x && p.y >= area.min.y && p.y < area.max.y
        })
    }

    /// Los asentamientos que llegan hasta `area`.
    pub fn touching(&self, area: Aabb) -> impl Iterator<Item = &Settlement> + '_ {
        self.settlements
            .iter()
            .filter(move |s| s.bounds.intersects(&area))
    }

    /// Las huellas de los asentamientos que contiene [`Self::in_area`].
    pub fn tracks_in(&self, area: Aabb) -> impl Iterator<Item = &Track> + '_ {
        self.in_area(area).flat_map(|s| s.tracks.iter())
    }

    /// Si `p` está a más de `margin` metros (como mucho
    /// [`KeepOut::MAX_MARGIN`]) de toda huella de edificio, patio y huella de
    /// tierra.
    pub fn is_clear(&self, p: Vec2, margin: f32) -> bool {
        self.keep_out.is_clear(p, margin)
    }

    /// El índice de zonas prohibidas de todos los asentamientos.
    pub fn keep_out(&self) -> &KeepOut {
        &self.keep_out
    }
}

/// Aquello de lo que se mantiene lejos cada asentamiento, en todo el mundo.
struct Site<'a> {
    terrain: &'a Terrain,
    fences: &'a FenceLayout,
    rails: Vec<(Vec2, Vec2)>,
    /// Franjas de los pueblos, ya agrandadas en [`Settlement::TOWN_CLEARANCE`].
    bands: Vec<Aabb>,
}

/// Los asentamientos colocados hasta ahora y las celdas del relevamiento que
/// ocupan.
#[derive(Default)]
struct Placed {
    settlements: Vec<Settlement>,
    used: HashSet<(u32, u32)>,
}

impl Site<'_> {
    /// Intenta poblar el campo detrás de la tranquera `index`: según su
    /// probabilidad y tipo con semilla, o como `forced` (un tipo y sus intentos
    /// de centro) sea cual sea la probabilidad. Devuelve si se colocó un
    /// asentamiento.
    fn settle_gate(
        &self,
        index: usize,
        mut rng: Rng,
        forced: Option<(SettlementKind, usize)>,
        placed: &mut Placed,
        town_gap: &impl Fn(Vec2) -> f32,
    ) -> bool {
        let gate = &self.fences.gates[index];
        let roads = self.terrain.roads.as_deref();
        let into = field_side(roads, gate.at, gate.along);
        let probe = gate.at + into * 3.0;
        if self.fences.is_descampado(probe) {
            return false;
        }
        let Some(cell) = self.fences.field_of(probe) else {
            return false;
        };
        let key = (cell.min.x.to_bits(), cell.min.y.to_bits());
        if placed.used.contains(&key) {
            return false;
        }
        let (kind, tries) = match forced {
            Some(forced) => forced,
            None => {
                let near_town = town_gap(gate.at) < SettlementLayout::NEAR_TOWN;
                let chance = if near_town {
                    SettlementLayout::SETTLED.0
                } else {
                    SettlementLayout::SETTLED.1
                };
                if rng.unit() >= chance {
                    return false;
                }
                (pick_kind(&mut rng, near_town), SettlementLayout::HUB_TRIES)
            }
        };
        let reach = cell.expanded(Settlement::RAIL_CLEARANCE + 1.0);
        let ground = Ground {
            roads,
            rails: self
                .rails
                .iter()
                .copied()
                .filter(|&(a, b)| reach.segment_overlap(a, b).is_some())
                .collect(),
            bands: self
                .bands
                .iter()
                .copied()
                .filter(|b| b.intersects(&reach))
                .collect(),
            cell,
        };
        let start = gate.at + into * 0.5;
        let entry = gate.at + into * SettlementLayout::ENTRY;
        if !ground.track_ok(start, entry, Some(gate.at)) {
            return false;
        }
        let (depth, room) = kind.hub_reach();
        let half = cell.size().x.min(cell.size().y) / 2.0 - 1.0;
        let inner = cell.expanded(-room.min(half));
        for _ in 0..tries {
            // En algún lugar delante de la tranquera, dentro de la celda.
            let ahead = gate.at
                + into * rng.range(depth.0, depth.1)
                + gate.along * rng.range(-depth.1, depth.1) * 0.5;
            let hub = inner.closest_point(ahead);
            let spaced = placed
                .settlements
                .iter()
                .all(|s| s.hub.distance(hub) >= Settlement::MIN_SPACING);
            if (hub - gate.at).dot(into) < depth.0
                || hub.distance(gate.at) > depth.1
                || !spaced
                || !ground.track_ok(entry, hub, None)
            {
                continue;
            }
            let mut compound = Compound::new(&ground, rng, kind, hub, into, [start, entry, hub]);
            let built = compound.raise() && {
                compound.plant_more();
                true
            };
            rng = compound.rng;
            if built {
                placed.used.insert(key);
                let seed = rng.next();
                placed.settlements.push(compound.finish(index as u32, seed));
                return true;
            }
        }
        false
    }
}

impl SettlementKind {
    /// Cuánto adentro del campo (desde el camino de la tranquera) está como
    /// mínimo el centro, a qué distancia de la tranquera como máximo, y el
    /// espacio que deja respecto de los bordes de su celda, en metros.
    fn hub_reach(self) -> ((f32, f32), f32) {
        match self {
            Self::Hut => ((35.0, 250.0), 35.0),
            Self::Outpost => ((50.0, 450.0), 45.0),
            Self::Farm => ((60.0, 350.0), 55.0),
            Self::Ranch => ((140.0, 600.0), 120.0),
        }
    }

    /// Lado del patio, en metros.
    fn patio(self) -> f32 {
        match self {
            Self::Hut | Self::Outpost => 10.0,
            Self::Farm => 16.0,
            Self::Ranch => 24.0,
        }
    }
}

/// El tipo de un asentamiento nuevo: chacras cerca de los pueblos, puestos y
/// ranchos dispersos, estancias poco frecuentes.
fn pick_kind(rng: &mut Rng, near_town: bool) -> SettlementKind {
    use SettlementKind::*;
    let roll = rng.unit();
    let odds: [(SettlementKind, f32); 4] = if near_town {
        [(Farm, 0.55), (Hut, 0.3), (Outpost, 0.1), (Ranch, 0.05)]
    } else {
        [(Hut, 0.35), (Outpost, 0.35), (Farm, 0.2), (Ranch, 0.1)]
    };
    let mut sum = 0.0;
    for (kind, share) in odds {
        sum += share;
        if roll < sum {
            return kind;
        }
    }
    odds[0].0
}

/// Dirección unitaria desde el camino, a través de una tranquera en `at` (con
/// su alambrado a lo largo de `along`), hacia el campo.
fn field_side(roads: Option<&RoadNetwork>, at: Vec2, along: Vec2) -> Vec2 {
    let across = Vec2::new(-along.y, along.x);
    let gap = |p: Vec2| {
        roads
            .and_then(|r| r.near(p, RoadNetwork::INDEX_REACH))
            .unwrap_or(f32::INFINITY)
    };
    if gap(at + across * 4.0) >= gap(at - across * 4.0) {
        across
    } else {
        across * -1.0
    }
}

/// Aquello de lo que se mantiene lejos un asentamiento: los caminos, la vía
/// férrea y los pueblos cercanos a su celda, y los alambrados de la celda.
struct Ground<'a> {
    roads: Option<&'a RoadNetwork>,
    rails: Vec<(Vec2, Vec2)>,
    /// Franjas de los pueblos, ya agrandadas en [`Settlement::TOWN_CLEARANCE`].
    bands: Vec<Aabb>,
    cell: Aabb,
}

impl Ground<'_> {
    /// Si una huella de edificio puede estar sobre `area`.
    fn area_ok(&self, area: Aabb) -> bool {
        self.area_ok_by(area, Settlement::FENCE_CLEARANCE, true)
    }

    /// [`Self::area_ok`] con su propio despeje `fence`, y el de los caminos
    /// solo cuando `roads`: la alameda llega hasta la tranquera, sobre la
    /// banquina del camino.
    fn area_ok_by(&self, area: Aabb, fence: f32, roads: bool) -> bool {
        self.cell.expanded(-fence).contains(&area)
            && self.bands.iter().all(|b| !b.intersects(&area))
            && self.rails.iter().all(|&(a, b)| {
                area.expanded(Settlement::RAIL_CLEARANCE)
                    .segment_overlap(a, b)
                    .is_none()
            })
            && (!roads || self.off_roads(area))
    }

    /// Si `area` está a [`Settlement::ROAD_CLEARANCE`] de todo camino: muestras
    /// a lo largo de sus bordes separadas como mucho 5 m, cada una exigida
    /// 2,5 m más lejos, y su centro.
    fn off_roads(&self, area: Aabb) -> bool {
        self.off_roads_by(area, Settlement::ROAD_CLEARANCE)
    }

    /// [`Self::off_roads`] para cualquier `clearance`.
    fn off_roads_by(&self, area: Aabb, clearance: f32) -> bool {
        let Some(roads) = self.roads else {
            return true;
        };
        let reach = clearance + 2.5;
        let size = area.size();
        let (nx, ny) = (
            (size.x / 5.0).ceil().max(1.0) as usize,
            (size.y / 5.0).ceil().max(1.0) as usize,
        );
        let clear = |p: Vec2| roads.near(p, reach).is_none();
        clear(area.center())
            && (0..=nx).all(|k| {
                let x = area.min.x + size.x * (k as f32 / nx as f32);
                clear(Vec2::new(x, area.min.y)) && clear(Vec2::new(x, area.max.y))
            })
            && (0..=ny).all(|k| {
                let y = area.min.y + size.y * (k as f32 / ny as f32);
                clear(Vec2::new(area.min.x, y)) && clear(Vec2::new(area.max.x, y))
            })
    }

    /// Si una huella puede ir en línea recta de `a` a `b`: dentro de la celda
    /// lejos de sus alambrados, lejos de los caminos pero a
    /// [`Settlement::GATE_APPROACH`] o menos de su `gate`, lejos del riel y de
    /// los pueblos. Se muestrea cada metro.
    fn track_ok(&self, a: Vec2, b: Vec2, gate: Option<Vec2>) -> bool {
        let inner = self.cell.expanded(-Settlement::TRACK_FENCE_CLEARANCE);
        let n = (a.distance(b).ceil() as usize).max(1);
        (0..=n).all(|k| {
            let p = a + (b - a) * (k as f32 / n as f32);
            let approach = gate.is_some_and(|g| g.distance(p) <= Settlement::GATE_APPROACH);
            let placed = if approach {
                self.cell.contains_point(p)
            } else {
                inner.contains_point(p)
                    && self
                        .roads
                        .is_none_or(|r| r.near(p, Settlement::TRACK_ROAD_CLEARANCE).is_none())
            };
            placed
                && self.bands.iter().all(|band| !band.contains_point(p))
                && self
                    .rails
                    .iter()
                    .all(|&(r0, r1)| segment_distance(p, r0, r1) >= Settlement::RAIL_CLEARANCE)
        })
    }
}

/// Un edificio o elemento decorativo a colocar: su tipo, su tamaño (a lo largo
/// de su frente y después en profundidad) y su altura.
#[derive(Clone, Copy)]
struct Part {
    kind: StructureKind,
    size: Vec2,
    height: f32,
}

/// Dónde buscar el centro de una parte: a `radius` metros (mínimo, máximo) de
/// `around`, en una dirección a `spread` radianes o menos de `bearing`.
#[derive(Clone, Copy)]
struct Spot {
    around: Vec2,
    radius: (f32, f32),
    bearing: f32,
    spread: f32,
}

/// Un asentamiento en construcción.
struct Compound<'a> {
    ground: &'a Ground<'a>,
    rng: Rng,
    kind: SettlementKind,
    hub: Vec2,
    /// Dirección unitaria desde la tranquera hacia el campo.
    into: Vec2,
    patio: Aabb,
    structures: Vec<Structure>,
    tracks: Vec<Track>,
}

impl<'a> Compound<'a> {
    /// Intentos para colocar una parte.
    const TRIES: usize = 24;

    fn new(
        ground: &'a Ground<'a>,
        rng: Rng,
        kind: SettlementKind,
        hub: Vec2,
        into: Vec2,
        trunk: [Vec2; 3],
    ) -> Self {
        let side = kind.patio();
        Self {
            ground,
            rng,
            kind,
            hub,
            into,
            patio: Aabb::from_center_size(hub, Vec2::new(side, side)),
            structures: Vec::new(),
            tracks: vec![Track {
                points: trunk.to_vec(),
            }],
        }
    }

    /// Levanta el conjunto de su tipo. `false` cuando una parte que debe tener
    /// no entra.
    fn raise(&mut self) -> bool {
        if !self.ground.area_ok(self.patio) {
            return false;
        }
        match self.kind {
            SettlementKind::Hut => self.hut(),
            SettlementKind::Outpost => self.outpost(),
            SettlementKind::Farm => self.farm(),
            SettlementKind::Ranch => self.ranch(),
        }
    }

    fn hut(&mut self) -> bool {
        let style = if self.chance(0.65) {
            HouseStyle::AdobeThatch
        } else {
            HouseStyle::AdobeIron
        };
        let house = self.part(
            StructureKind::MainHouse(style),
            (8.0, 10.0),
            (4.5, 5.5),
            2.6,
        );
        if self.place(house, self.ahead(10.0, 16.0, 0.6)).is_none() {
            return false;
        }
        if self.chance(0.7) {
            let well = self.part(StructureKind::Well, (1.6, 1.6), (1.6, 1.6), 0.9);
            self.place(well, self.round(8.0, 16.0));
        }
        if self.chance(0.5) {
            self.corral((12.0, 16.0), self.round(18.0, 30.0));
        }
        self.extras(0.6, 0.25, (7.0, 10.0), (10.0, 20.0));
        true
    }

    fn outpost(&mut self) -> bool {
        let style = if self.chance(0.5) {
            HouseStyle::AdobeThatch
        } else {
            HouseStyle::AdobeIron
        };
        let hut = self.part(StructureKind::MainHouse(style), (6.0, 8.0), (4.0, 5.0), 2.5);
        if self.place(hut, self.ahead(9.0, 14.0, 0.8)).is_none() {
            return false;
        }
        if !self.corral((18.0, 24.0), self.round(20.0, 34.0)) {
            return false;
        }
        let windpump = self.chance(0.5);
        let tank = windpump && self.chance(0.5);
        self.water_point(windpump, tank, (14.0, 26.0));
        self.extras(0.7, 0.0, (7.0, 10.0), (10.0, 20.0));
        true
    }

    fn farm(&mut self) -> bool {
        let style = if self.chance(0.6) {
            HouseStyle::BrickParapet
        } else {
            HouseStyle::AdobeIron
        };
        let house = self.part(
            StructureKind::MainHouse(style),
            (12.0, 16.0),
            (7.0, 9.0),
            3.4,
        );
        let Some(casco) = self.place(house, self.ahead(12.0, 20.0, 0.5)) else {
            return false;
        };
        let barn = self.part(StructureKind::Barn, (14.0, 20.0), (8.0, 10.0), 4.5);
        if self.place_behind(barn, casco, (22.0, 40.0)).is_none() {
            return false;
        }
        if !self.water_point(true, true, (20.0, 36.0)) {
            return false;
        }
        self.corral((18.0, 26.0), self.round(30.0, 50.0));
        if self.chance(0.35) {
            let style = if self.chance(0.5) {
                HouseStyle::AdobeIron
            } else {
                HouseStyle::AdobeThatch
            };
            let worker = self.part(
                StructureKind::WorkerHouse(style),
                (8.0, 10.0),
                (4.5, 5.5),
                2.7,
            );
            self.place(worker, self.round(25.0, 45.0));
        }
        for _ in 0..1 + self.rng.below(3) {
            let hay = self.part(StructureKind::Haystack, (5.0, 7.0), (5.0, 7.0), 4.0);
            self.place(hay, self.round(30.0, 60.0));
        }
        self.extras(0.8, 0.5, (8.0, 12.0), (14.0, 30.0));
        for _ in 0..self.rng.below(5) {
            let tree = self.tree(StructureKind::Eucalyptus);
            self.place(tree, self.round(16.0, 34.0));
        }
        if self.chance(Settlement::WRECKS.0) {
            self.wreck((40.0, 70.0));
        }
        true
    }

    fn ranch(&mut self) -> bool {
        let homestead = self.part(
            StructureKind::MainHouse(HouseStyle::GalleryHouse),
            (22.0, 30.0),
            (14.0, 18.0),
            4.5,
        );
        let Some(casco) = self.place(homestead, self.ahead(18.0, 26.0, 0.3)) else {
            return false;
        };
        let homestead = self.structures[casco].footprint;
        // Primero, para que las casas de peones y sus huellas se mantengan
        // lejos de la alameda en lugar de ocupar su lugar.
        self.poplar_row();
        // Las casas de peones, cada una en su propio sector alrededor del
        // patio.
        let houses = 2 + self.rng.below(3);
        let turn = self.rng.range(0.0, TAU);
        let mut built = 0;
        for k in 0..houses {
            let style = if self.chance(0.6) {
                HouseStyle::AdobeIron
            } else {
                HouseStyle::BrickParapet
            };
            let house = self.part(
                StructureKind::WorkerHouse(style),
                (9.0, 12.0),
                (5.0, 6.0),
                2.8,
            );
            let mut spot = self.round(70.0, 130.0);
            spot.bearing = turn + TAU * k as f32 / houses as f32;
            spot.spread = PI / houses as f32;
            if self.place(house, spot).is_some() {
                built += 1;
            }
        }
        if built < 2 {
            return false;
        }
        let mut barns = 0;
        for _ in 0..1 + self.rng.below(2) {
            let barn = self.part(StructureKind::Barn, (20.0, 30.0), (10.0, 14.0), 5.5);
            barns += self.place_behind(barn, casco, (40.0, 80.0)).is_some() as usize;
        }
        // Los corrales van con los galpones, detrás del casco.
        let mut corrals = 0;
        for _ in 0..1 + self.rng.below(3) {
            let behind = self.behind(casco, (45.0, 100.0), 1.0);
            corrals += self.corral((22.0, 34.0), behind) as usize;
        }
        if barns == 0 || corrals == 0 || !self.water_point(true, true, (35.0, 70.0)) {
            return false;
        }
        for _ in 0..self.rng.below(4) {
            let hay = self.part(StructureKind::Haystack, (5.0, 7.0), (5.0, 7.0), 4.0);
            self.place(hay, self.round(60.0, 120.0));
        }
        for _ in 0..1 + self.rng.below(2) {
            let post = self.part(StructureKind::HitchingPost, (3.0, 3.0), (0.3, 0.3), 1.1);
            self.place(post, self.ahead(13.0, 20.0, 1.0));
        }
        let cart = self.part(StructureKind::Cart, (5.0, 5.0), (2.2, 2.2), 2.5);
        self.place(cart, self.round(25.0, 50.0));
        self.grove(homestead);
        let bounds = self.bounds();
        if bounds.size().x * bounds.size().y < Settlement::RANCH_AREA {
            return false;
        }
        // Se sortea solo una vez que la estancia está en pie, para que un
        // centro reintentado saque los mismos números que antes.
        if self.chance(Settlement::WRECKS.1) {
            self.wreck((60.0, 110.0));
        }
        true
    }

    /// Un carro abandonado en el borde del patio, a `at` metros del centro: se
    /// coloca al final, así que todas las puertas y huellas ya están colocadas
    /// y se mantiene lejos de ellas como cualquier huella de edificio.
    fn wreck(&mut self, at: (f32, f32)) {
        let (wide, deep) = Settlement::WRECK_SIZE;
        let part = self.part(StructureKind::WreckedWagon, (wide, wide), (deep, deep), 2.2);
        self.place(part, self.round(at.0, at.1));
    }

    /// La alameda: álamos en pares enfrentados a ambos lados del tronco,
    /// separados por [`Settlement::POPLAR_SPACING`], desde unos 4 m dentro de
    /// la tranquera hasta 20 m antes del centro, sea cual sea el largo del
    /// camino de entrada. Las filas siguen el tronco dibujado (tranquera,
    /// curva, centro), cada par a ambos lados de su propio tramo.
    /// [`Self::thin_trees`] no los toca: conserva uno de cada dos árboles de
    /// los demás en el orden de plantación, lo que talaría un lado o
    /// desalinearía los pares.
    fn poplar_row(&mut self) {
        let [gate, bend, hub] = [
            self.tracks[0].points[0],
            self.tracks[0].points[1],
            self.tracks[0].points[2],
        ];
        let last = bend.distance(hub);
        let total = last + gate.distance(bend);
        let mut along = total - Settlement::POPLAR_GATE_INSET;
        while along >= 20.0 {
            let (from, to, back) = if along <= last {
                (hub, bend, along)
            } else {
                (bend, gate, along - last)
            };
            let dir = (to - from) * (1.0 / from.distance(to).max(1e-6));
            let across = Vec2::new(-dir.y, dir.x);
            let before = self.structures.len();
            let mut planted = 0;
            for side in [-Settlement::POPLAR_OFFSET, Settlement::POPLAR_OFFSET] {
                let at = from + dir * back + across * side;
                let tree = self.tree(StructureKind::Poplar);
                planted += self.place_at_by(tree, at, true).is_some() as usize;
            }
            // En la curva el árbol del lado interior puede caer sobre la huella
            // del otro tramo: se saltea el par en lugar de dejar un árbol solo.
            if planted == 1 {
                self.structures.truncate(before + 1);
                self.structures.pop();
            }
            along -= Settlement::POPLAR_SPACING;
        }
    }

    /// Un monte de eucaliptos al lado del casco: una grilla con variación
    /// aleatoria de 6 m de separación.
    fn grove(&mut self, homestead: Aabb) {
        let across = Vec2::new(-self.into.y, self.into.x);
        let side = if self.chance(0.5) { 1.0 } else { -1.0 };
        let (rows, cols) = (4 + self.rng.below(3), 5 + self.rng.below(4));
        let corner = homestead.center()
            + self.into * self.rng.range(20.0, 40.0)
            + across * (side * self.rng.range(15.0, 35.0));
        for r in 0..rows {
            for c in 0..cols {
                let jitter = Vec2::new(self.rng.range(-1.5, 1.5), self.rng.range(-1.5, 1.5));
                let at = corner + Vec2::new(c as f32 * 6.0, r as f32 * 6.0) + jitter;
                let tree = self.tree(StructureKind::Eucalyptus);
                self.place_at(tree, at);
            }
        }
    }

    /// Un molino (o un pozo) con su bebedero, y el tanque cuando `tank`, a
    /// `radius` del centro. `false` cuando el molino o el pozo no entra.
    fn water_point(&mut self, windpump: bool, tank: bool, radius: (f32, f32)) -> bool {
        let source = if windpump {
            self.part(StructureKind::Windpump, (2.4, 2.4), (2.4, 2.4), 10.0)
        } else {
            self.part(StructureKind::Well, (1.6, 1.6), (1.6, 1.6), 0.9)
        };
        let Some(at) = self.place(source, self.round(radius.0, radius.1)) else {
            return false;
        };
        let at = self.structures[at].footprint.center();
        if tank {
            let d = self.rng.range(7.0, 10.0);
            let tank = self.part(StructureKind::Tank, (d, d), (d, d), 1.6);
            self.place(
                tank,
                Spot {
                    around: at,
                    ..self.round(9.0, 12.0)
                },
            );
        }
        let trough = self.part(StructureKind::Trough, (6.0, 8.0), (0.9, 0.9), 0.6);
        self.place(
            trough,
            Spot {
                around: at,
                ..self.round(5.0, 9.0)
            },
        );
        true
    }

    /// Un corral de unos `side` metros de lado; devuelve si entra.
    fn corral(&mut self, side: (f32, f32), spot: Spot) -> bool {
        let corral = self.part(StructureKind::Corral, side, side, 1.6);
        self.place(corral, spot).is_some()
    }

    /// Un palenque junto al patio y una carreta en el patio, cada uno según su
    /// probabilidad.
    fn extras(&mut self, post: f32, cart: f32, post_at: (f32, f32), cart_at: (f32, f32)) {
        if self.chance(post) {
            let part = self.part(StructureKind::HitchingPost, (3.0, 3.0), (0.3, 0.3), 1.1);
            self.place(part, self.round(post_at.0, post_at.1));
        }
        if self.chance(cart) {
            let part = self.part(StructureKind::Cart, (5.0, 5.0), (2.2, 2.2), 2.5);
            self.place(part, self.round(cart_at.0, cart_at.1));
        }
    }

    /// Planta un 30 % más de eucaliptos de los que ya tiene el conjunto, con un
    /// flujo aleatorio propio para que el resto de la distribución no se mueva.
    fn plant_more(&mut self) {
        let trees = self.structures.iter().filter(|s| s.kind.is_tree()).count();
        let mut own = Rng::new(0x70a1_a2c3 ^ self.hub.x.to_bits() ^ self.hub.y.to_bits(), 1);
        let extra = (Settlement::MORE_TREES * trees as f32 + own.unit()) as usize;
        std::mem::swap(&mut self.rng, &mut own);
        for _ in 0..extra {
            let tree = self.tree(StructureKind::Eucalyptus);
            self.place(tree, self.round(16.0, 34.0));
        }
        std::mem::swap(&mut self.rng, &mut own);
        self.thin_trees(own.unit());
    }

    /// Un conjunto conserva [`Settlement::tree_keep`] de sus árboles: un árbol se queda cuando su parte del total acumulado cruza
    /// un número entero, así que los que se van quedan repartidos de forma pareja en el orden en que se plantaron (las filas de una
    /// grilla, los pares de una alameda) y nada más se mueve. `start` en [0, 1) desplaza el total.
    fn thin_trees(&mut self, start: f32) {
        let keep = Settlement::tree_keep(self.kind);
        if keep >= 1.0 {
            return;
        }
        let mut seen = 0.0_f32;
        self.structures.retain(|s| {
            if !s.kind.is_tree() || s.kind == StructureKind::Poplar {
                return true;
            }
            // El épsilon evita que una parte que cae justo en un número entero
            // se redondee hacia abajo.
            let at = |n: f32| (start + n * keep + 1e-4).floor();
            let stays = at(seen + 1.0) > at(seen);
            seen += 1.0;
            stays
        });
    }

    fn tree(&mut self, kind: StructureKind) -> Part {
        let height = match kind {
            StructureKind::Poplar => self.rng.range(14.0, 20.0),
            _ => self.rng.range(18.0, 28.0),
        };
        Part {
            kind,
            size: Vec2::new(1.0, 1.0),
            height,
        }
    }

    fn part(
        &mut self,
        kind: StructureKind,
        wide: (f32, f32),
        deep: (f32, f32),
        height: f32,
    ) -> Part {
        let size = Vec2::new(
            self.rng.range(wide.0, wide.1),
            self.rng.range(deep.0, deep.1),
        );
        Part { kind, size, height }
    }

    fn chance(&mut self, p: f32) -> bool {
        self.rng.unit() < p
    }

    /// Detrás del patio, visto desde la tranquera.
    fn ahead(&self, near: f32, far: f32, spread: f32) -> Spot {
        Spot {
            around: self.hub,
            radius: (near, far),
            bearing: self.into.y.atan2(self.into.x),
            spread,
        }
    }

    /// En cualquier lugar alrededor del patio.
    fn round(&self, near: f32, far: f32) -> Spot {
        Spot {
            spread: PI,
            ..self.ahead(near, far, 0.0)
        }
    }

    /// La dirección unitaria del último acceso, desde la curva del tronco hasta
    /// el centro: por donde un visitante cabalga hacia el casco.
    fn lane(&self) -> Vec2 {
        let entry = self.tracks[0].points[1];
        (self.hub - entry) * (1.0 / self.hub.distance(entry).max(1e-6))
    }

    /// Un lugar a `radius` metros (mínimo, máximo) del casco `casco`, del lado
    /// opuesto a lo largo de [`Self::lane`], a `spread` radianes o menos.
    fn behind(&self, casco: usize, radius: (f32, f32), spread: f32) -> Spot {
        let lane = self.lane();
        Spot {
            around: self.structures[casco].footprint.center(),
            radius,
            bearing: lane.y.atan2(lane.x),
            spread,
        }
    }

    /// Coloca un galpón detrás del casco `casco`: su borde cercano más allá del
    /// borde lejano del casco a lo largo del camino de entrada, y al menos a
    /// [`Settlement::BARN_ROAD_CLEARANCE`] de todo camino. Un intento que rompe
    /// cualquiera de las dos reglas se deshace, junto con su huella.
    fn place_behind(&mut self, part: Part, casco: usize, radius: (f32, f32)) -> Option<usize> {
        let lane = self.lane();
        let center = self.structures[casco].footprint.center();
        let corners = |fp: Aabb| {
            [
                fp.min,
                fp.max,
                Vec2::new(fp.min.x, fp.max.y),
                Vec2::new(fp.max.x, fp.min.y),
            ]
            .map(|c| (c - center).dot(lane))
        };
        let casco_end = corners(self.structures[casco].footprint)
            .into_iter()
            .fold(f32::MIN, f32::max);
        for _ in 0..Self::TRIES {
            let spot = self.behind(casco, radius, 0.7);
            let angle = spot.bearing + self.rng.range(-spot.spread, spot.spread);
            let r = self.rng.range(radius.0, radius.1);
            let at = center + Vec2::new(angle.cos(), angle.sin()) * r;
            let (structures, tracks) = (self.structures.len(), self.tracks.len());
            let Some(index) = self.place_at(part, at) else {
                continue;
            };
            let fp = self.structures[index].footprint;
            let near = corners(fp).into_iter().fold(f32::MAX, f32::min);
            if near >= casco_end
                && self
                    .ground
                    .off_roads_by(fp, Settlement::BARN_ROAD_CLEARANCE)
            {
                return Some(index);
            }
            self.structures.truncate(structures);
            self.tracks.truncate(tracks);
        }
        None
    }

    /// Coloca `part` en algún lugar de `spot`, mirando hacia el centro, con su
    /// huella de ramal cuando tiene una puerta. Devuelve su índice.
    fn place(&mut self, part: Part, spot: Spot) -> Option<usize> {
        for _ in 0..Self::TRIES {
            let angle = spot.bearing + self.rng.range(-spot.spread, spot.spread);
            let r = self.rng.range(spot.radius.0, spot.radius.1);
            let at = spot.around + Vec2::new(angle.cos(), angle.sin()) * r;
            if let Some(index) = self.place_at(part, at) {
                return Some(index);
            }
        }
        None
    }

    /// Coloca `part` centrada en `at`, si entra ahí.
    fn place_at(&mut self, part: Part, at: Vec2) -> Option<usize> {
        self.place_at_by(part, at, false)
    }

    /// [`Self::place_at`]; los árboles `lane` de la alameda pueden estar a 2 m
    /// de los alambrados de la celda y sobre la banquina del camino.
    fn place_at_by(&mut self, part: Part, at: Vec2, lane: bool) -> Option<usize> {
        let front = facing(self.hub - at);
        let size = match front {
            Side::North | Side::South => part.size,
            Side::East | Side::West => Vec2::new(part.size.y, part.size.x),
        };
        let structure = Structure {
            kind: part.kind,
            footprint: Aabb::from_center_size(at, size),
            height: part.height,
            front,
            variant: self.rng.next() >> 8,
        };
        if !self.fits_by(structure.footprint, lane) {
            return None;
        }
        if part.kind.is_reached() {
            let door = structure.door();
            if !self.branch_ok(self.hub, door, structure.footprint) {
                return None;
            }
            self.tracks.push(Track {
                points: vec![self.hub, door],
            });
        }
        self.structures.push(structure);
        Some(self.structures.len() - 1)
    }

    /// Si `footprint` está lejos de las zonas prohibidas del terreno, del
    /// patio, de toda otra huella de edificio y de toda huella de tierra. Los
    /// árboles de la alameda (`lane`) usan el criterio de terreno más
    /// permisivo.
    fn fits_by(&self, footprint: Aabb, lane: bool) -> bool {
        let corridor = footprint.expanded(Settlement::CORRIDOR);
        let ground = if lane {
            self.ground.area_ok_by(footprint, 2.0, false)
        } else {
            self.ground.area_ok(footprint)
        };
        ground
            && !self.patio.intersects(&footprint)
            && self
                .structures
                .iter()
                .all(|s| !s.footprint.expanded(Settlement::GAP).intersects(&footprint))
            && self
                .tracks
                .iter()
                .flat_map(Track::segments)
                .all(|(a, b)| corridor.segment_overlap(a, b).is_none())
    }

    /// Si un ramal puede ir de `a` a `b`, lejos de toda huella de edificio y de
    /// `own` (aquella a la que lleva).
    fn branch_ok(&self, a: Vec2, b: Vec2, own: Aabb) -> bool {
        self.ground.track_ok(a, b, None)
            && self
                .structures
                .iter()
                .map(|s| s.footprint)
                .chain([own])
                .all(|f| {
                    f.expanded(Settlement::CORRIDOR)
                        .segment_overlap(a, b)
                        .is_none()
                })
    }

    /// Todo lo que cubre el conjunto.
    fn bounds(&self) -> Aabb {
        let mut bounds = self.patio;
        let mut grow = |area: Aabb| {
            bounds = Aabb::new(
                Vec2::new(bounds.min.x.min(area.min.x), bounds.min.y.min(area.min.y)),
                Vec2::new(bounds.max.x.max(area.max.x), bounds.max.y.max(area.max.y)),
            );
        };
        for s in &self.structures {
            grow(s.footprint);
        }
        for p in self.tracks.iter().flat_map(|h| h.points.iter()) {
            grow(Aabb::from_center_size(
                *p,
                Vec2::new(2.0, 2.0) * Track::HALF_WIDTH,
            ));
        }
        bounds
    }

    fn finish(self, gate: u32, seed: u32) -> Settlement {
        Settlement {
            kind: self.kind,
            gate,
            hub: self.hub,
            patio: self.patio,
            bounds: self.bounds(),
            structures: self.structures,
            tracks: self.tracks,
            seed,
        }
    }
}

/// El lado de una huella de edificio que mira en la dirección de `toward`.
fn facing(toward: Vec2) -> Side {
    if toward.x.abs() > toward.y.abs() {
        if toward.x > 0.0 {
            Side::East
        } else {
            Side::West
        }
    } else if toward.y > 0.0 {
        Side::North
    } else {
        Side::South
    }
}

/// Rectángulos delgados a lo largo de los cuatro lados de un corral, con
/// [`Settlement::CORRAL_GATE`] metros abiertos en el centro de su `front`.
fn corral_rails(fp: Aabb, front: Side) -> Vec<Aabb> {
    let t = Settlement::CORRAL_RAIL / 2.0;
    let (lo, hi) = (fp.min, fp.max);
    let sides = [
        (Side::South, Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y)),
        (Side::North, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, hi.y)),
        (Side::West, Vec2::new(lo.x, lo.y), Vec2::new(lo.x, hi.y)),
        (Side::East, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
    ];
    let rail = |a: Vec2, b: Vec2| {
        Aabb::new(
            Vec2::new(a.x.min(b.x) - t, a.y.min(b.y) - t),
            Vec2::new(a.x.max(b.x) + t, a.y.max(b.y) + t),
        )
    };
    let mut rails = Vec::new();
    for (side, a, b) in sides {
        if side == front {
            let mid = (a + b) * 0.5;
            let half = (b - a) * (Settlement::CORRAL_GATE / 2.0 / a.distance(b));
            rails.push(rail(a, mid - half));
            rails.push(rail(mid + half, b));
        } else {
            rails.push(rail(a, b));
        }
    }
    rails
}

/// Una forma de zona prohibida: un área, o una franja de `half` metros a cada
/// lado de un segmento.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Area(Aabb),
    Strip(Vec2, Vec2, f32),
}

impl Shape {
    /// Qué tan afuera de ella está `p` (0 adentro).
    fn distance(&self, p: Vec2) -> f32 {
        match *self {
            Shape::Area(area) => area.closest_point(p).distance(p),
            Shape::Strip(a, b, half) => (segment_distance(p, a, b) - half).max(0.0),
        }
    }

    fn bounds(&self) -> Aabb {
        match *self {
            Shape::Area(area) => area,
            Shape::Strip(a, b, half) => Aabb::new(
                Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                Vec2::new(a.x.max(b.x), a.y.max(b.y)),
            )
            .expanded(half),
        }
    }
}

/// El terreno del que se mantiene lejos la flora silvestre: las huellas de
/// edificio, los patios y las huellas de tierra de cada asentamiento, indexados
/// en una grilla uniforme para que la comprobación de un punto solo mire las
/// formas cercanas.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeepOut {
    shapes: Vec<Shape>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl KeepOut {
    /// Lado de una celda del índice, en metros.
    pub const CELL: f32 = 32.0;
    /// El margen más amplio que puede pedir una comprobación, en metros.
    pub const MAX_MARGIN: f32 = 8.0;

    fn push(&mut self, shape: Shape) {
        let index = self.shapes.len() as u32;
        let reach = shape.bounds().expanded(Self::MAX_MARGIN);
        let (x0, y0) = cell_of(reach.min);
        let (x1, y1) = cell_of(reach.max);
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.cells.entry((x, y)).or_default().push(index);
            }
        }
        self.shapes.push(shape);
    }

    /// Las formas indexadas en la celda de `p`.
    fn nearby(&self, p: Vec2) -> &[u32] {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return &[];
        }
        self.cells.get(&cell_of(p)).map_or(&[], Vec::as_slice)
    }

    /// Si `p` está a más de `margin` metros (como mucho [`Self::MAX_MARGIN`])
    /// de toda forma de zona prohibida.
    pub fn is_clear(&self, p: Vec2, margin: f32) -> bool {
        let margin = margin.min(Self::MAX_MARGIN);
        self.nearby(p)
            .iter()
            .all(|&i| self.shapes[i as usize].distance(p) > margin)
    }

    /// Cuántas formas revisa una comprobación en `p`: lo que cuesta
    /// [`Self::is_clear`].
    pub fn scanned(&self, p: Vec2) -> usize {
        self.nearby(p).len()
    }
}

fn cell_of(p: Vec2) -> (i32, i32) {
    (
        (p.x / KeepOut::CELL).floor() as i32,
        (p.y / KeepOut::CELL).floor() as i32,
    )
}

/// Se mezcla con la semilla del terreno para los asentamientos.
const SETTLEMENT_SEED: u32 = 0x5e77_1e05;
/// Se mezcla con la semilla del terreno para las estancias forzadas.
const RANCH_SEED: u32 = 0x7a2c_4e11;
/// Se mezcla con la semilla del terreno para los carros abandonados al costado
/// del camino.
const WRECK_SEED: u32 = 0x3c4a_7a5e;

impl Site<'_> {
    /// Unos pocos carros abandonados en las banquinas de los caminos, con
    /// semilla por tramo de alambrado al costado del camino: según su
    /// probabilidad, el primer tramo a partir de uno elegido por semilla cuya
    /// banquina contenga un carro a lo largo del camino, entre el borde del
    /// camino y el alambrado, lejos de tranqueras, aberturas, carteles
    /// indicadores y pasos a nivel, rieles, pueblos, asentamientos y los demás
    /// carros abandonados.
    fn roadside_wrecks(&self, seed: u32, settlements: &[Settlement]) -> Vec<RoadsideWreck> {
        let Some(roads) = self.terrain.roads.as_deref() else {
            return Vec::new();
        };
        let road_parts = roads.obstacles();
        let mut wrecks: Vec<RoadsideWreck> = Vec::new();
        for (index, run) in self.fences.runs.iter().enumerate() {
            if wrecks.len() >= SettlementLayout::ROADSIDE_WRECKS {
                break;
            }
            if !run.roadside || run.posts.len() < 4 {
                continue;
            }
            let mut rng = Rng::new(seed ^ WRECK_SEED, index as u32);
            if rng.unit() >= SettlementLayout::ROADSIDE_CHANCE {
                continue;
            }
            // Tramos dentro del recorrido (no en sus extremos, junto a un paso
            // a nivel o una tranquera), a partir de uno elegido por semilla.
            let spans = run.posts.len() - 3;
            let first = rng.below(spans);
            let variant = rng.next() >> 8;
            let found = (0..spans).find_map(|k| {
                let k = 1 + (first + k) % spans;
                let (a, b) = (run.posts[k], run.posts[k + 1]);
                let wreck = self.on_verge(roads, a, b, variant)?;
                let fp = wreck.footprint();
                let clear = self
                    .fences
                    .gates
                    .iter()
                    .all(|g| g.at.distance(wreck.at) > 25.0)
                    && self
                        .fences
                        .openings
                        .iter()
                        .all(|o| o.at.distance(wreck.at) > 25.0)
                    && road_parts.iter().all(|o| !o.expanded(3.0).intersects(&fp))
                    && self.rails.iter().all(|&(p, q)| {
                        segment_distance(wreck.at, p, q) > RoadNetwork::RAIL_CLEARANCE
                    })
                    && self.bands.iter().all(|band| !band.intersects(&fp))
                    && settlements
                        .iter()
                        .all(|s| !s.bounds.expanded(30.0).intersects(&fp))
                    && wrecks
                        .iter()
                        .all(|w| w.at.distance(wreck.at) >= SettlementLayout::ROADSIDE_SPACING);
                clear.then_some(wreck)
            });
            wrecks.extend(found);
        }
        wrecks
    }

    /// Un carro a lo largo del tramo de alambrado `a`–`b`, del lado del camino,
    /// con su lado cercano un poco apartado del borde del camino, si queda
    /// entero dentro de la banquina.
    fn on_verge(
        &self,
        roads: &RoadNetwork,
        a: Vec2,
        b: Vec2,
        variant: u32,
    ) -> Option<RoadsideWreck> {
        let length = a.distance(b);
        if length < 1.0 {
            return None;
        }
        let along = (b - a) * (1.0 / length);
        let mid = (a + b) * 0.5;
        let across = Vec2::new(-along.y, along.x);
        let toward = if roads.distance(mid + across) < roads.distance(mid - across) {
            across
        } else {
            across * -1.0
        };
        let deep = Settlement::WRECK_SIZE.1;
        let out = RoadNetwork::HALF_WIDTH + SettlementLayout::VERGE_GAP + deep / 2.0;
        let wreck = RoadsideWreck {
            at: mid + toward * (roads.distance(mid) - out),
            along,
            variant,
        };
        let inside = wreck.corners().iter().all(|&c| {
            let d = roads.distance(c);
            let verge = RoadNetwork::HALF_WIDTH + SettlementLayout::VERGE_GAP * 0.5
                ..=FenceLayout::ROADSIDE - 0.3;
            verge.contains(&d)
        });
        inside.then_some(wreck)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fence::tests::{fenced, reached};
    use crate::{ChunkCoord, FloraSite, StaticObstacles, World};

    /// Los pueblos del fixture de alambrados que tienen una estancia cerca, tal
    /// como los pide el juego: Eduardo Castex y General Pico.
    fn ranch_towns(towns: &[TownLayout]) -> Vec<usize> {
        ["Eduardo Castex", "General Pico"]
            .iter()
            .filter_map(|name| towns.iter().position(|t| t.name == *name))
            .collect()
    }

    /// El mundo del fixture de alambrados con sus asentamientos colocados y
    /// asentados, con las estancias del juego cerca de Castex y Pico (la mezcla
    /// con semilla por sí sola puede no sacar ninguna). Se construye una sola
    /// vez y se comparte: las pruebas que cambian el mundo primero lo clonan.
    pub(crate) fn settled() -> &'static (
        World,
        Vec<TownLayout>,
        RailLine,
        FenceLayout,
        SettlementLayout,
    ) {
        type Settled = (
            World,
            Vec<TownLayout>,
            RailLine,
            FenceLayout,
            SettlementLayout,
        );
        static SETTLED: std::sync::LazyLock<Settled> = std::sync::LazyLock::new(|| {
            let (world, towns, line, fences) = fenced();
            let mut world = world.clone();
            let ranches = ranch_towns(towns);
            assert_eq!(ranches.len(), 2);
            let layout =
                SettlementLayout::build_with_ranches(&world.terrain, towns, line, fences, &ranches);
            let count = world.settle(&layout);
            let open = fences.gates.iter().filter(|g| g.open).count();
            println!(
                "settlements: {} behind {open} open gates ({} huts, {} outposts, {} farms, \
                 {} ranchs), {count} obstacles",
                layout.settlements.len(),
                layout.count(SettlementKind::Hut),
                layout.count(SettlementKind::Outpost),
                layout.count(SettlementKind::Farm),
                layout.count(SettlementKind::Ranch),
            );
            (world, towns.clone(), line.clone(), fences.clone(), layout)
        });
        &SETTLED
    }

    /// Dirección unitaria desde el camino, a través de `gate`, hacia su campo,
    /// según la distancia exacta al camino.
    fn into_field(roads: &RoadNetwork, gate: &crate::Gate) -> Vec2 {
        let out = Vec2::new(-gate.along.y, gate.along.x);
        if roads.distance(gate.at + out) > roads.distance(gate.at) {
            out
        } else {
            out * -1.0
        }
    }

    /// Puntos a lo largo de cada huella de `s`, separados `step` metros.
    fn track_points(s: &Settlement, step: f32) -> Vec<Vec2> {
        s.tracks
            .iter()
            .flat_map(Track::segments)
            .flat_map(|(a, b)| {
                let n = (a.distance(b) / step).ceil().max(1.0) as usize;
                (0..=n).map(move |k| a + (b - a) * (k as f32 / n as f32))
            })
            .collect()
    }

    /// Puntos alrededor del borde de `area`, separados más o menos un metro.
    fn edge_points(area: Aabb) -> Vec<Vec2> {
        let size = area.size();
        let (nx, ny) = (size.x.ceil() as usize, size.y.ceil() as usize);
        let mut points = Vec::new();
        for k in 0..=nx {
            let x = area.min.x + size.x * (k as f32 / nx as f32);
            points.extend([Vec2::new(x, area.min.y), Vec2::new(x, area.max.y)]);
        }
        for k in 0..=ny {
            let y = area.min.y + size.y * (k as f32 / ny as f32);
            points.extend([Vec2::new(area.min.x, y), Vec2::new(area.max.x, y)]);
        }
        points
    }

    #[test]
    fn settlements_are_laid_the_same_way_every_time_and_sparsely() {
        let (world, towns, line, fences, layout) = settled();
        assert_eq!(
            *layout,
            SettlementLayout::build_with_ranches(
                &world.terrain,
                towns,
                line,
                fences,
                &ranch_towns(towns)
            )
        );
        // La construcción simple, sin estancias forzadas, también tiene
        // semilla.
        let plain = SettlementLayout::build(&world.terrain, towns, line, fences);
        assert_eq!(
            plain,
            SettlementLayout::build(&world.terrain, towns, line, fences)
        );
        assert!(!plain.settlements.is_empty());
        let open = fences.gates.iter().filter(|g| g.open).count();
        let total = layout.settlements.len();
        // Solo algunas tranqueras abiertas llevan a un asentamiento, y las
        // estancias son pocas.
        assert!(total >= 10, "{total}");
        assert!(total * 2 < open, "{total} of {open}");
        let count = |k| layout.count(k);
        use SettlementKind::*;
        assert!(count(Ranch) >= 1, "{}", count(Ranch));
        assert!(count(Ranch) * 3 <= total, "{}", count(Ranch));
        for k in [Hut, Outpost, Farm] {
            assert!(count(k) >= 1, "{k:?}");
        }
        // Otra semilla genera otro campo.
        let mut other = world.terrain.clone();
        other.seed ^= 0x55;
        assert_ne!(
            layout
                .settlements
                .iter()
                .map(|s| s.gate)
                .collect::<Vec<_>>(),
            SettlementLayout::build_with_ranches(&other, towns, line, fences, &ranch_towns(towns))
                .settlements
                .iter()
                .map(|s| s.gate)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_settlement_lies_behind_an_open_gate_on_its_field_side() {
        let (world, _, _, fences, layout) = settled();
        let roads = world.terrain.roads.clone().unwrap();
        let mut cells: Vec<Aabb> = Vec::new();
        for s in &layout.settlements {
            let gate = &fences.gates[s.gate as usize];
            assert!(gate.open, "{gate:?}");
            let into = into_field(&roads, gate);
            let start = s.trunk().points[0];
            assert!(start.distance(gate.at) < 1.0, "{start:?} {gate:?}");
            assert!((start - gate.at).dot(into) > 0.0);
            assert!((s.hub - gate.at).dot(into) > 30.0, "{:?}", s.hub);
            assert_eq!(*s.trunk().points.last().unwrap(), s.hub);
            let cell = fences.field_of(gate.at + into * 3.0).unwrap();
            assert!(cell.contains(&s.bounds), "{cell:?} {:?}", s.bounds);
            assert!(!fences.is_descampado(s.hub));
            // Un asentamiento por campo, y bien separados.
            assert!(!cells.contains(&cell));
            cells.push(cell);
            for t in &layout.settlements {
                if t != s {
                    assert!(t.hub.distance(s.hub) >= Settlement::MIN_SPACING);
                    assert!(!t.bounds.intersects(&s.bounds));
                }
            }
        }
    }

    #[test]
    fn settlements_keep_clear_of_roads_rails_towns_and_fences() {
        let (world, towns, line, fences, layout) = settled();
        let roads = world.terrain.roads.clone().unwrap();
        let rails: Vec<(Vec2, Vec2)> = line
            .full_path(towns)
            .points()
            .windows(2)
            .map(|w| (w[0], w[1]))
            .chain(line.segments().map(|s| (s.start, s.end)))
            .collect();
        let wire = StaticObstacles::new(fences.obstacles());
        for s in &layout.settlements {
            let cell = fences.field_of(s.hub).unwrap();
            let near_rails: Vec<(Vec2, Vec2)> = rails
                .iter()
                .copied()
                .filter(|&(a, b)| s.bounds.expanded(100.0).segment_overlap(a, b).is_some())
                .collect();
            for st in &s.structures {
                let fp = st.footprint;
                // La alameda llega hasta la tranquera: se mantiene a 2 m de los
                // alambrados y puede estar sobre la banquina del camino.
                let lane = st.kind == StructureKind::Poplar;
                assert!(
                    cell.expanded(if lane {
                        -2.0
                    } else {
                        -Settlement::FENCE_CLEARANCE
                    })
                    .contains(&fp),
                    "{st:?}"
                );
                assert!(wire.near(fp.expanded(1.0)).is_empty(), "{st:?}");
                for t in towns {
                    assert!(
                        !t.leveled()
                            .expanded(Settlement::TOWN_CLEARANCE)
                            .intersects(&fp)
                    );
                }
                for p in edge_points(fp) {
                    assert!(
                        lane || roads.near(p, Settlement::ROAD_CLEARANCE - 0.01).is_none(),
                        "{st:?} at {p:?}"
                    );
                    for &(a, b) in &near_rails {
                        assert!(segment_distance(p, a, b) >= Settlement::RAIL_CLEARANCE - 0.01);
                    }
                }
            }
            // Las huellas se quedan en el campo, lejos del camino y del riel, y
            // no cruzan ningún alambre: el tronco pasa por las hojas abiertas
            // de la tranquera.
            let gate = fences.gates[s.gate as usize].at;
            for p in track_points(s, 0.5) {
                assert!(cell.contains_point(p), "{p:?}");
                assert!(
                    wire.near(Aabb::from_center_size(p, Vec2::new(0.6, 0.6)))
                        .is_empty(),
                    "a track crosses a fence at {p:?}"
                );
                if p.distance(gate) > Settlement::GATE_APPROACH {
                    assert!(
                        cell.expanded(-Settlement::TRACK_FENCE_CLEARANCE)
                            .contains_point(p)
                    );
                    assert!(
                        roads
                            .near(p, Settlement::TRACK_ROAD_CLEARANCE - 0.01)
                            .is_none(),
                        "{p:?}"
                    );
                }
                for &(a, b) in &near_rails {
                    assert!(segment_distance(p, a, b) >= Settlement::RAIL_CLEARANCE - 0.01);
                }
            }
        }
    }

    #[test]
    fn buildings_overlap_neither_each_other_nor_the_tracks() {
        let (_, _, _, _, layout) = settled();
        for s in &layout.settlements {
            for (i, a) in s.structures.iter().enumerate() {
                assert!(!s.patio.intersects(&a.footprint), "{a:?}");
                for b in &s.structures[i + 1..] {
                    assert!(
                        !a.footprint
                            .expanded(Settlement::GAP - 0.01)
                            .intersects(&b.footprint),
                        "{a:?} {b:?}"
                    );
                }
                for (p, q) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(
                        a.footprint
                            .expanded(Track::HALF_WIDTH)
                            .segment_overlap(p, q)
                            .is_none(),
                        "{a:?} on the track {p:?}-{q:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn tracks_lead_from_the_gate_to_every_door() {
        let (_, _, _, _, layout) = settled();
        for s in &layout.settlements {
            // Todos los ramales salen del centro, donde termina el tronco.
            for h in &s.tracks[1..] {
                assert_eq!(h.points[0], s.hub);
            }
            for st in s.structures.iter().filter(|st| st.kind.is_reached()) {
                let door = st.door();
                assert!(
                    s.tracks[1..]
                        .iter()
                        .any(|h| h.points.last().unwrap().distance(door) < 1e-3),
                    "no track to {st:?}"
                );
            }
            assert_eq!(
                s.tracks.len(),
                1 + s
                    .structures
                    .iter()
                    .filter(|st| st.kind.is_reached())
                    .count()
            );
            let houses = s.structures.iter().filter(|st| st.kind.is_house()).count();
            let main = s
                .structures
                .iter()
                .filter(|st| matches!(st.kind, StructureKind::MainHouse(_)))
                .count();
            assert!(houses >= 1 && main == 1, "{s:?}");
        }
    }

    #[test]
    fn each_kind_has_the_buildings_of_its_time() {
        let (_, _, _, _, layout) = settled();
        use StructureKind::*;
        for s in &layout.settlements {
            let n = |k| s.of(k).count();
            let style = s
                .structures
                .iter()
                .find_map(|st| match st.kind {
                    MainHouse(style) => Some(style),
                    _ => None,
                })
                .unwrap();
            match s.kind {
                SettlementKind::Hut => {
                    assert!(matches!(
                        style,
                        HouseStyle::AdobeThatch | HouseStyle::AdobeIron
                    ));
                    assert_eq!(n(Barn) + n(Windpump), 0, "{s:?}");
                }
                SettlementKind::Outpost => {
                    assert!(n(Corral) == 1 && n(Trough) <= 1, "{s:?}");
                    assert!(n(Windpump) + n(Well) == 1, "{s:?}");
                }
                SettlementKind::Farm => {
                    assert!(style != HouseStyle::GalleryHouse);
                    assert!(n(Barn) == 1 && n(Windpump) == 1, "{s:?}");
                }
                SettlementKind::Ranch => {
                    assert_eq!(style, HouseStyle::GalleryHouse);
                    let workers = s
                        .structures
                        .iter()
                        .filter(|st| matches!(st.kind, WorkerHouse(_)))
                        .count();
                    assert!((2..=4).contains(&workers), "{workers}");
                    assert!((1..=2).contains(&n(Barn)));
                    assert!(n(Corral) >= 1 && n(Windpump) == 1);
                    assert!(n(Eucalyptus) + n(Poplar) >= 10, "{s:?}");
                    let size = s.bounds.size();
                    assert!(
                        size.x * size.y >= Settlement::RANCH_AREA,
                        "{} m²",
                        size.x * size.y
                    );
                }
            }
        }
    }

    #[test]
    fn a_rider_rides_in_through_the_gate_to_every_door_and_into_the_corrals() {
        let (world, towns, line, fences, layout) = settled();
        let mut world = world.clone();
        let flora = FloraSite::new(world.config, &world.terrain, line)
            .with_track(&line.full_path(towns))
            .with_fences(fences)
            .with_settlements(layout);
        world.plant(&flora);
        let roads = world.terrain.roads.clone().unwrap();
        let mut seen_kinds = HashSet::new();
        let mut checked = 0;
        for s in &layout.settlements {
            if !seen_kinds.insert(s.kind) && checked >= 8 {
                continue;
            }
            let gate = &fences.gates[s.gate as usize];
            let from = gate.at - into_field(&roads, gate) * 4.0;
            let window = Aabb::new(
                Vec2::new(s.bounds.min.x.min(from.x), s.bounds.min.y.min(from.y)),
                Vec2::new(s.bounds.max.x.max(from.x), s.bounds.max.y.max(from.y)),
            )
            .expanded(20.0);
            let window = Aabb::new(
                Vec2::new(window.min.x.floor(), window.min.y.floor()),
                Vec2::new(window.max.x.floor(), window.max.y.floor()),
            );
            let (seen, w) = reached(&world, window, &[from]);
            let at = |p: Vec2| {
                let q = p - window.min;
                seen[q.y as usize * w + q.x as usize]
            };
            for st in &s.structures {
                if st.kind.is_reached() {
                    assert!(at(st.door()), "cannot ride to {st:?} of {:?}", s.kind);
                }
                if st.kind == StructureKind::Corral {
                    assert!(at(st.footprint.center()), "cannot ride into {st:?}");
                    assert_eq!(st.obstacles().len(), 5);
                }
            }
            checked += 1;
        }
        assert!(seen_kinds.len() == 4 && checked >= 8, "{seen_kinds:?}");
    }

    #[test]
    fn flora_keeps_off_the_settlements_through_a_cheap_index() {
        let (world, _, _, _, layout) = settled();
        let (flora, trees, _) = crate::flora::tests::shipped();
        assert!(trees.len() > 5_000);
        let near = |p: Vec2, margin: f32| {
            layout
                .touching(Aabb::from_center_size(p, Vec2::new(1.0, 1.0)).expanded(margin))
                .any(|s| {
                    s.structures
                        .iter()
                        .any(|st| st.footprint.closest_point(p).distance(p) <= margin)
                        || s.tracks
                            .iter()
                            .flat_map(Track::segments)
                            .any(|(a, b)| segment_distance(p, a, b) <= Track::HALF_WIDTH + margin)
                })
        };
        for t in trees {
            assert!(!near(t.at, FloraSite::TREE_SETTLEMENT_MARGIN), "{t:?}");
        }
        let mut tufts = 0;
        let size = world.config.chunk_size;
        for s in layout.settlements.iter().take(12) {
            for p in [s.hub, s.bounds.min, s.bounds.max] {
                for t in flora.chunk(ChunkCoord::containing(p, size)).tufts {
                    assert!(!near(t.at, FloraSite::TUFT_SETTLEMENT_MARGIN), "{t:?}");
                    tufts += 1;
                }
            }
            // En las huellas y en los edificios no crece nada; lejos, puede
            // crecer cualquier cosa.
            for p in track_points(s, 2.0) {
                assert!(!layout.is_clear(p, 0.0) && !flora.off_settlements(p, 0.0));
            }
            for st in &s.structures {
                assert!(!layout.is_clear(st.footprint.center(), 0.0));
            }
        }
        assert!(tufts > 1_000, "{tufts}");
        // La comprobación solo mira unas pocas formas cercanas, en cualquier
        // lugar.
        let busiest = layout
            .settlements
            .iter()
            .flat_map(|s| track_points(s, 4.0))
            .map(|p| layout.keep_out().scanned(p))
            .max()
            .unwrap();
        assert!(busiest <= 80, "{busiest}");
        let far = Vec2::new(1e6, 1e6);
        assert!(layout.is_clear(far, 4.0) && layout.keep_out().scanned(far) == 0);
    }

    #[test]
    fn chunks_share_out_the_settlements_once_each() {
        let (world, _, _, _, layout) = settled();
        let size = world.config.chunk_size;
        let mut owned = 0;
        let mut tracks = 0;
        let bounds = world.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let area = ChunkCoord::new(x, y).bounds(size);
                owned += layout.in_area(area).count();
                tracks += layout.tracks_in(area).count();
                for s in layout.in_area(area) {
                    assert!(layout.touching(area).any(|t| t == s));
                }
            }
        }
        assert_eq!(owned, layout.settlements.len());
        assert_eq!(
            tracks,
            layout
                .settlements
                .iter()
                .map(|s| s.tracks.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn every_structure_blocks_up_to_its_own_height() {
        let (world, _, _, _, layout) = settled();
        // Los mismos rectángulos que `obstacles`, en el mismo orden: asentar
        // con alturas no descarta ninguno.
        let boxes: Vec<Aabb> = layout.obstacle_heights().map(|(o, _)| o).collect();
        assert_eq!(boxes, layout.obstacles().collect::<Vec<_>>());
        let wreck_parts: Vec<Aabb> = layout
            .roadside
            .iter()
            .flat_map(RoadsideWreck::obstacles)
            .collect();
        let structures = layout.settlements.iter().flat_map(|s| s.structures.iter());
        let mut checked = 0;
        for s in structures {
            for o in s.obstacles() {
                let found = |clearance| world.obstacles.near_above(o, clearance).contains(&&o);
                assert!(found(s.height - 0.01), "{:?} under its top", s.kind);
                // Otra huella de edificio puede compartir el rectángulo (la
                // propia de un carro abandonado, por ejemplo).
                if !wreck_parts.contains(&o) {
                    assert!(!found(s.height), "{:?} at its top", s.kind);
                }
                checked += 1;
            }
        }
        assert!(checked > 0);
        // Los carros abandonados al costado del camino no tienen altura: nunca
        // se superan.
        for o in &wreck_parts {
            assert!(world.obstacles.near_above(*o, 1e6).contains(&o));
        }
    }

    #[test]
    fn a_corral_is_open_only_at_its_gate() {
        let fp = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(20.0, 20.0));
        let corral = Structure {
            kind: StructureKind::Corral,
            footprint: fp,
            height: 1.6,
            front: Side::North,
            variant: 0,
        };
        let rails = corral.obstacles();
        assert_eq!(rails.len(), 5);
        let blocked = |p: Vec2| rails.iter().any(|r| r.contains_point(p));
        assert!(!blocked(Vec2::new(10.0, 20.0)), "the gate is open");
        assert!(blocked(Vec2::new(5.0, 20.0)) && blocked(Vec2::new(10.0, 0.0)));
        assert!(blocked(Vec2::new(0.0, 10.0)) && blocked(Vec2::new(20.0, 10.0)));
        assert_eq!(
            corral.door(),
            Vec2::new(10.0, 20.0 + Settlement::DOOR_REACH)
        );
        assert_eq!(StructureKind::Windpump.slot(), Some("windpump"));
        assert!(StructureKind::Eucalyptus.is_tree());
    }

    #[test]
    fn every_slot_name_is_listed_once_as_a_plain_folder_name() {
        use StructureKind::*;
        let kinds = [
            MainHouse(HouseStyle::AdobeThatch),
            WorkerHouse(HouseStyle::BrickParapet),
            Barn,
            Corral,
            Windpump,
            Tank,
            Trough,
            Well,
            Haystack,
            HitchingPost,
            Cart,
            WreckedWagon,
            Eucalyptus,
            Poplar,
        ];
        let mut slots: Vec<&str> = kinds.iter().filter_map(|k| k.slot()).collect();
        let mut listed = StructureKind::SLOTS.to_vec();
        slots.sort_unstable();
        listed.sort_unstable();
        assert_eq!(slots, listed, "every kind with a slot is listed, once");
        for slot in StructureKind::SLOTS {
            assert!(
                !slot.is_empty()
                    && slot
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{slot}"
            );
        }
        // Los edificios y el corral solo se construyen de forma procedural.
        assert!(Barn.slot().is_none() && Corral.slot().is_none() && Tank.slot().is_none());
    }

    #[test]
    fn tree_spots_block_with_their_trunks_only() {
        let (world, _, _, _, layout) = settled();
        let trees: Vec<&Structure> = layout
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
            .collect();
        assert!(!trees.is_empty());
        let collider = crate::Circle::new(crate::Rider::COLLIDER_RADIUS);
        for tree in trees {
            let at = tree.footprint.center();
            let trunk = tree.obstacles();
            assert_eq!(trunk.len(), 1, "{tree:?}");
            let r = tree.trunk_radius();
            assert!((trunk[0].size().x - 2.0 * r).abs() < 1e-3);
            // Un jinete que camina hacia el tronco sale empujado del rectángulo
            // del propio tronco, no solo de su centro.
            let pushed = world.resolve(collider, at, |_| ());
            let clear = r + crate::Rider::COLLIDER_RADIUS - 1e-3;
            assert!(
                (pushed.x - at.x).abs() >= clear || (pushed.y - at.y).abs() >= clear,
                "{tree:?} pushed to {pushed:?}"
            );
            // Ningún tronco queda sobre una huella: los surcos y la alameda
            // entre sus álamos siguen abiertos para el jinete.
            for s in &layout.settlements {
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(
                        trunk[0]
                            .expanded(crate::Rider::COLLIDER_RADIUS)
                            .segment_overlap(a, b)
                            .is_none(),
                        "{tree:?} blocks a track"
                    );
                }
            }
        }
        // `World::settle` cuenta los troncos junto con los demás obstáculos.
        assert_eq!(
            layout.obstacles().count(),
            layout
                .settlements
                .iter()
                .flat_map(|s| s.structures.iter())
                .map(|s| s.obstacles().len())
                .sum::<usize>()
                + 3 * layout.roadside.len()
        );
        let mut bare = World::with_towns(world.config, crate::Terrain::default(), &[]);
        let trunks = layout
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
            .count();
        assert!(bare.settle(layout) >= trunks + layout.settlements.len());
        // Los radios se mantienen en el rango de un tronco a cualquier altura.
        for (kind, lo, hi) in [
            (StructureKind::Eucalyptus, 0.2, 0.45),
            (StructureKind::Poplar, 0.12, 0.25),
        ] {
            for height in [1.0, 20.0, 80.0] {
                let r = Structure {
                    kind,
                    height,
                    ..*layout.settlements[0].structures.first().unwrap()
                }
                .trunk_radius();
                assert!((lo..=hi).contains(&r), "{kind:?} {height}: {r}");
            }
        }
        let r = Structure {
            kind: StructureKind::Eucalyptus,
            height: 25.0,
            ..*layout.settlements[0].structures.first().unwrap()
        }
        .trunk_radius();
        assert!((r - 0.4).abs() < 1e-5, "{r}");
    }

    #[test]
    fn abandoned_wagons_are_few_and_stay_off_roads_tracks_and_doors() {
        let (world, _, _, fences, layout) = settled();
        let roads = world.terrain.roads.as_deref().expect("roads laid");
        // En los patios: como mucho uno por asentamiento, solo en chacras y
        // estancias, lejos de toda huella (y por lo tanto de toda puerta) y
        // patio.
        let mut yard = 0;
        for s in &layout.settlements {
            let wrecks: Vec<&Structure> = s.of(StructureKind::WreckedWagon).collect();
            assert!(wrecks.len() <= 1, "{s:?}");
            if !wrecks.is_empty() {
                assert!(matches!(
                    s.kind,
                    SettlementKind::Farm | SettlementKind::Ranch
                ));
            }
            for w in wrecks {
                yard += 1;
                assert!(!w.footprint.intersects(&s.patio));
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(
                        w.footprint
                            .expanded(Track::HALF_WIDTH)
                            .segment_overlap(a, b)
                            .is_none(),
                        "{w:?} on a track"
                    );
                }
                for other in s.structures.iter().filter(|o| o.kind.is_reached()) {
                    assert!(!w.footprint.contains_point(other.door()), "{w:?} at a door");
                }
            }
        }
        // En las banquinas: unos pocos, entre el borde del camino y el
        // alambrado, nunca sobre el camino, una tranquera o un cartel
        // indicador, bien separados.
        let verge = &layout.roadside;
        println!("wrecks: {yard} in yards, {} on verges", verge.len());
        assert!(verge.len() <= SettlementLayout::ROADSIDE_WRECKS);
        assert!(!verge.is_empty(), "the shipped map has long straight roads");
        let signposts = roads.obstacles();
        for w in verge {
            for c in w.corners().into_iter().chain([w.at]) {
                let d = roads.distance(c);
                assert!(d > RoadNetwork::HALF_WIDTH, "{w:?} on the road");
                assert!(d < FenceLayout::ROADSIDE, "{w:?} past the fence");
            }
            let fp = w.footprint();
            assert!(signposts.iter().all(|o| !o.intersects(&fp)), "{w:?}");
            assert!(
                fences
                    .gates
                    .iter()
                    .all(|g| !fp.expanded(10.0).contains_point(g.at))
            );
            // Su cuerpo bloquea al jinete, sin llegar hasta el camino.
            for o in w.obstacles() {
                assert!(world.obstacles.contains(&o), "{w:?} does not block");
                for c in [
                    o.min,
                    o.max,
                    Vec2::new(o.min.x, o.max.y),
                    Vec2::new(o.max.x, o.min.y),
                ] {
                    assert!(
                        roads.distance(c) > RoadNetwork::HALF_WIDTH,
                        "{o:?} on the road"
                    );
                }
            }
            assert!(!layout.is_clear(w.at, 0.0), "flora keeps off it");
            assert!((w.along.length() - 1.0).abs() < 1e-4);
        }
        for (i, a) in verge.iter().enumerate() {
            for b in &verge[i + 1..] {
                assert!(a.at.distance(b.at) >= SettlementLayout::ROADSIDE_SPACING);
            }
        }
    }

    #[test]
    fn a_town_asked_for_an_estancia_gets_one_nearby_or_is_reported() {
        let (world, towns, line, fences) = fenced();
        let plain = SettlementLayout::build(&world.terrain, towns, line, fences);
        let asked: Vec<usize> = (0..towns.len()).chain([towns.len() + 3]).collect();
        let layout =
            SettlementLayout::build_with_ranches(&world.terrain, towns, line, fences, &asked);
        let again =
            SettlementLayout::build_with_ranches(&world.terrain, towns, line, fences, &asked);
        assert_eq!(layout, again);
        // El pueblo que falta se informa, nunca se descarta en silencio.
        assert!(layout.unplaced_ranches.contains(&(towns.len() + 3)));
        // Las estancias forzadas se colocan primero, en el orden pedido: una
        // por pueblo ubicado, antes de la mezcla con semilla.
        let placed: Vec<usize> = asked
            .iter()
            .copied()
            .filter(|t| !layout.unplaced_ranches.contains(t))
            .collect();
        assert!(!placed.is_empty(), "no town in range got its estancia");
        for (k, &t) in placed.iter().enumerate() {
            let town = &towns[t];
            let forced = &layout.settlements[k];
            assert_eq!(forced.kind, SettlementKind::Ranch, "{}", town.name);
            // Viene de la pasada forzada (con su propia semilla), no de la
            // mezcla.
            assert!(!plain.settlements.contains(forced), "{}", town.name);
            // Las tranqueras se prueban de la más cercana a la más lejana y las
            // tranqueras de un paso nunca se vuelven a probar, así que el paso
            // en el que se encontró es el primero cuyo alcance incluye su
            // tranquera: aquí nunca el último, sin límite.
            let gate = fences.gates[forced.gate as usize].at;
            let gap = town.leveled().closest_point(gate).distance(gate);
            let step = SettlementLayout::RANCH_REACH
                .iter()
                .position(|&reach| gap <= reach)
                .expect("the last step reaches anywhere");
            println!("{}: estancia {gap:.0} m out, step {step}", town.name);
            assert!(
                step < SettlementLayout::RANCH_REACH.len() - 1,
                "{}: {gap} m",
                town.name
            );
        }
        assert!(layout.count(SettlementKind::Ranch) >= towns.len().min(1));
        // Cada asentamiento mantiene su separación, también los forzados.
        for (i, a) in layout.settlements.iter().enumerate() {
            for b in &layout.settlements[i + 1..] {
                assert!(a.hub.distance(b.hub) >= Settlement::MIN_SPACING);
            }
        }
        // No pedir nada coloca la mezcla con semilla como antes.
        assert!(plain.unplaced_ranches.is_empty());
        assert_eq!(
            SettlementLayout::build_with_ranches(&world.terrain, towns, line, fences, &[]),
            plain
        );
    }

    #[test]
    fn settlements_plant_their_trees_clear_of_everything() {
        let (_, _, _, _, layout) = settled();
        let trees: Vec<&Structure> = layout
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
            .collect();
        assert!(!trees.is_empty());
        for s in &layout.settlements {
            for (k, tree) in s
                .structures
                .iter()
                .enumerate()
                .filter(|(_, t)| t.kind.is_tree())
            {
                let at = tree.footprint.center();
                assert!(!s.patio.contains_point(at), "{s:?}");
                for (j, other) in s.structures.iter().enumerate().filter(|(j, _)| *j != k) {
                    assert!(
                        !other.footprint.expanded(2.9).intersects(&tree.footprint),
                        "{k} by {j}"
                    );
                }
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    assert!(segment_distance(at, a, b) >= 2.2, "a tree on a huella");
                }
            }
        }
    }

    #[test]
    fn a_compound_keeps_its_kinds_share_of_its_trees_spread_evenly() {
        let ground = Ground {
            roads: None,
            rails: Vec::new(),
            bands: Vec::new(),
            cell: Aabb::from_center_size(Vec2::ZERO, Vec2::new(2000.0, 2000.0)),
        };
        for (kind, kept, gap) in [
            (SettlementKind::Hut, 14, 2),
            (SettlementKind::Outpost, 14, 2),
            (SettlementKind::Farm, 14, 2),
            (SettlementKind::Ranch, 10, 3),
        ] {
            for start in [0.0, 0.3, 0.99] {
                let hub = Vec2::new(0.0, -300.0);
                let mut c = Compound::new(
                    &ground,
                    Rng::new(7, 9),
                    kind,
                    hub,
                    Vec2::new(0.0, 1.0),
                    [hub, hub, hub],
                );
                let spots: Vec<Vec2> = (0..20)
                    .map(|k| Vec2::new(k as f32 * 12.0 - 120.0, 0.0))
                    .collect();
                for &at in &spots {
                    let tree = c.tree(StructureKind::Eucalyptus);
                    assert!(c.place_at(tree, at).is_some());
                }
                c.thin_trees(start);
                let left: Vec<usize> = spots
                    .iter()
                    .enumerate()
                    .filter(|(_, at)| c.structures.iter().any(|s| s.footprint.center() == **at))
                    .map(|(k, _)| k)
                    .collect();
                // 20 árboles a 0,49 conservan 9 o 10 según el desplazamiento.
                let ok = left.len() == kept || (kept == 10 && left.len() == 9);
                assert!(ok, "{kind:?} {start}: {}", left.len());
                // De forma pareja: los que se van nunca están seguidos.
                for w in left.windows(2) {
                    assert!(w[1] - w[0] <= gap, "{kind:?} {left:?}");
                }
            }
        }
    }
    /// La dirección unitaria del último acceso de una estancia, desde la curva
    /// de su tronco hacia el centro, y el casco.
    fn approach(s: &Settlement) -> (Vec2, Vec2, &Structure) {
        let entry = s.tracks[0].points[1];
        let lane = (s.hub - entry) * (1.0 / s.hub.distance(entry));
        let casco = s
            .structures
            .iter()
            .find(|st| matches!(st.kind, StructureKind::MainHouse(_)))
            .unwrap();
        (entry, lane, casco)
    }

    /// Dónde está `p` respecto del tronco de `s`: metros a lo largo de él desde
    /// el centro, y metros hacia su izquierda (positivo) o derecha.
    fn along_the_trunk(s: &Settlement, p: Vec2) -> (f32, f32) {
        let [gate, bend, hub] = [s.tracks[0].points[0], s.tracks[0].points[1], s.hub];
        let mut best = (f32::MAX, 0.0, 0.0);
        let mut start = 0.0;
        for (a, b) in [(hub, bend), (bend, gate)] {
            let len = a.distance(b);
            let dir = (b - a) * (1.0 / len);
            let d = p - a;
            let along = d.dot(dir).clamp(0.0, len);
            let across = dir.x * d.y - dir.y * d.x;
            let off = (d - dir * along).length();
            if off < best.0 {
                best = (off, start + along, across);
            }
            start += len;
        }
        (best.1, best.2)
    }

    #[test]
    fn the_alameda_lines_both_sides_of_the_lane_up_to_the_gate() {
        let (_, _, _, _, layout) = settled();
        let ranches: Vec<_> = layout
            .settlements
            .iter()
            .filter(|s| s.kind == SettlementKind::Ranch)
            .collect();
        assert!(!ranches.is_empty());
        for s in ranches {
            let (mut lefts, mut rights) = (Vec::new(), Vec::new());
            let mut nearest_gate = f32::MAX;
            for p in s.of(StructureKind::Poplar) {
                let at = p.footprint.center();
                let (along, across) = along_the_trunk(s, at);
                if across.abs() > Settlement::POPLAR_OFFSET + 1.0 {
                    continue; // el monte, no la fila
                }
                nearest_gate = nearest_gate.min(at.distance(s.tracks[0].points[0]));
                if across > 0.0 {
                    lefts.push(along);
                } else {
                    rights.push(along);
                }
            }
            // El último par está dentro de la tranquera, no en su poste.
            assert!(
                nearest_gate <= 8.0,
                "the alameda stops {nearest_gate} m from the gate at {:?}",
                s.hub
            );
            for side in [&mut lefts, &mut rights] {
                side.sort_by(f32::total_cmp);
                assert!(side.len() >= 4, "{} poplars at {:?}", side.len(), s.hub);
                // Denso: una cortina rompevientos, con como mucho un par
                // salteado (la curva).
                let gaps = side.windows(2).map(|w| w[1] - w[0]);
                let skipped = gaps
                    .clone()
                    .filter(|g| *g > Settlement::POPLAR_SPACING + 1.0)
                    .count();
                assert!(
                    skipped <= 1
                        && gaps.fold(0.0, f32::max) <= 2.0 * Settlement::POPLAR_SPACING + 1.0,
                    "gaps in the alameda at {:?}",
                    s.hub
                );
            }
            // Una alameda: cada árbol tiene un compañero enfrente, al otro lado
            // del camino de entrada.
            for (mine, other) in [(&lefts, &rights), (&rights, &lefts)] {
                for a in mine {
                    assert!(
                        other.iter().any(|b| (a - b).abs() <= 1.0),
                        "a poplar {a} m along the lane has no partner across it at {:?}",
                        s.hub
                    );
                }
            }
            assert_eq!(lefts.len(), rights.len());
        }
    }

    #[test]
    fn barns_stand_behind_the_casco_and_far_from_the_main_road() {
        let (world, _, _, _, layout) = settled();
        let roads = world.terrain.roads.as_deref().unwrap();
        let mut barns = 0;
        for s in &layout.settlements {
            for barn in s.of(StructureKind::Barn) {
                barns += 1;
                let (entry, lane, casco) = approach(s);
                let far = |fp: Aabb| {
                    [
                        fp.min,
                        fp.max,
                        Vec2::new(fp.min.x, fp.max.y),
                        Vec2::new(fp.max.x, fp.min.y),
                    ]
                    .map(|c| (c - casco.footprint.center()).dot(lane))
                };
                let nearest = far(barn.footprint).into_iter().fold(f32::MAX, f32::min);
                let casco_end = far(casco.footprint).into_iter().fold(f32::MIN, f32::max);
                assert!(
                    nearest >= casco_end,
                    "{:?} is not behind {casco:?}",
                    barn.footprint
                );
                assert!(
                    barn.footprint.center().distance(entry)
                        > casco.footprint.center().distance(entry)
                );
                let fp = barn.footprint;
                for p in [
                    fp.center(),
                    fp.min,
                    fp.max,
                    Vec2::new(fp.min.x, fp.max.y),
                    Vec2::new(fp.max.x, fp.min.y),
                ] {
                    assert!(
                        roads.near(p, Settlement::BARN_ROAD_CLEARANCE).is_none(),
                        "a barn within {} m of a road at {p:?}",
                        Settlement::BARN_ROAD_CLEARANCE
                    );
                }
            }
        }
        assert!(barns >= 2, "{barns}");
    }
}
