//! Flora nativa de la pampa abierta entre los pueblos: matas de pasto (paja y
//! flechilla), caldenes y chañares (con el algarrobo, el tala y el espinillo
//! entre ellos, y los árboles de los pueblos que se escaparon al campo),
//! dispersos, en pequeñas isletas y en las manchas densas de monte, un ombú
//! junto a algunos asentamientos, y los arbustos (piquillín, jarilla y
//! chañarales) del sotobosque.
//!
//! La ubicación es pura y determinista: la flora de cada chunk tiene como
//! semilla su coordenada, así que el render puede reconstruir cualquier chunk
//! en cualquier momento y la lógica puede listar todos los árboles del mundo
//! una sola vez, al cargar, para la colisión.

use crate::rng::Rng;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use crate::{
    Aabb, ChunkCoord, FenceLayout, FlatArea, KeepOut, Lagoon, LinePath, RailLine, RoadNetwork,
    SettlementLayout, Terrain, Vec2, WorldConfig,
};

/// Un árbol nativo del caldenal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Species {
    /// *Prosopis caldenia*: 6–9 m, un tronco oscuro y áspero y una copa amplia
    /// en forma de sombrilla.
    Calden,
    /// *Geoffroea decorticans*: 3–5 m. Crece en matorrales densos (chañarales),
    /// así que se dibuja y colisiona como uno solo.
    Chanar,
    /// *Prosopis flexuosa*: un compañero bajo, ancho y a menudo de varios
    /// troncos del caldén en el monte.
    Algarrobo,
    /// *Celtis tala*: un árbol de borde poco común, con ramas en zigzag bajo
    /// una copa redonda irregular.
    Tala,
    /// *Vachellia caven*: un árbol chico poco común, de copa redondeada, en
    /// terreno abierto y alterado.
    Espinillo,
    /// *Phytolacca dioica*: un tronco enorme con contrafuertes bajo una cúpula
    /// amplia y densa. Se planta solo junto a los asentamientos, nunca en campo
    /// abierto.
    Ombu,
    /// El paraíso de los pueblos, escapado al campo a lo largo de los caminos y
    /// alrededor de los asentamientos.
    Paraiso,
    /// El olmo de los pueblos, escapado como el paraíso.
    Olmo,
    /// El eucalipto de los pueblos, escapado como el paraíso.
    Eucalipto,
    /// El aromo de los pueblos, escapado como el paraíso.
    Aromo,
}

/// En qué etapa de su vida está un árbol: decide el tamaño y el grosor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Age {
    /// Los primeros años de un renuevo: un tallo delgado y una copa chica.
    Sapling,
    /// Un árbol joven, todavía en crecimiento.
    Young,
    /// Un árbol adulto: la mayor parte del monte.
    Mature,
    /// Un gigante viejo poco común: alto, de tronco grueso y copa amplia.
    Old,
}

/// Los árboles nativos de los chunks más recientes, conservados entre llamadas
/// para que los chunks de un streaming (y los extras de cada uno) reutilicen
/// los de los demás: sale primero el más viejo, como mucho `cap` chunks. Un
/// clon empieza vacío, y la caché nunca participa en la comparación de dos
/// sitios.
struct NativeCache {
    cap: usize,
    inner: Mutex<NativeCacheInner>,
}

#[derive(Default)]
struct NativeCacheInner {
    /// Los árboles nativos por chunk y según si los montes se redistribuyeron
    /// (el único ajuste de un sitio que los cambia después de construido).
    map: HashMap<(ChunkCoord, bool), Arc<[Tree]>>,
    order: VecDeque<(ChunkCoord, bool)>,
}

impl NativeCache {
    /// Chunks que se conservan por defecto: la escena de radio 3 son 49 chunks,
    /// y una pasada por todo el mundo, fila por fila, necesita unas cinco filas
    /// de 120.
    const DEFAULT_CAP: usize = 1024;

    fn new(cap: usize) -> Self {
        Self {
            cap,
            inner: Mutex::default(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, NativeCacheInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn len(&self) -> usize {
        self.lock().map.len()
    }

    fn clear(&self) {
        *self.lock() = NativeCacheInner::default();
    }

    fn get_or_insert(
        &self,
        key: (ChunkCoord, bool),
        work: impl FnOnce() -> Vec<Tree>,
    ) -> Arc<[Tree]> {
        if let Some(hit) = self.lock().map.get(&key) {
            return hit.clone();
        }
        // Se calcula fuera del lock; dos hilos pueden hacerlo los dos, y los
        // árboles son los mismos.
        let trees: Arc<[Tree]> = work().into();
        let mut inner = self.lock();
        if inner.map.insert(key, trees.clone()).is_none() {
            inner.order.push_back(key);
            while inner.order.len() > self.cap {
                if let Some(old) = inner.order.pop_front() {
                    inner.map.remove(&old);
                }
            }
        }
        trees
    }
}

impl Clone for NativeCache {
    fn clone(&self) -> Self {
        Self::new(self.cap)
    }
}

impl PartialEq for NativeCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for NativeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NativeCache({}/{})", self.len(), self.cap)
    }
}

/// Un árbol en el plano del suelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    pub at: Vec2,
    pub species: Species,
    pub age: Age,
    /// Altura hasta lo alto de la copa, en metros.
    pub height: f32,
    /// Qué tan anchos son la copa y el tronco respecto de la escala de la
    /// altura (1 es como está modelado): un árbol ancho o uno angosto.
    pub width: f32,
    /// Giro alrededor de la vertical, en radianes.
    pub yaw: f32,
    /// Aspecto con semilla: 32 bits aleatorios (el render elige con sus bytes
    /// la forma, el tono de las hojas y la corteza, y si un caldén es un árbol
    /// seco).
    pub variant: u32,
}

/// Un tipo de arbusto nativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShrubKind {
    /// Piquillín (*Condalia microphylla*): un arbusto espinoso y denso de hojas
    /// oscuras diminutas, el sotobosque del monte.
    Piquillin,
    /// Jarilla (*Larrea divaricata*): un arbusto resinoso verde amarillento de
    /// muchos tallos erguidos, en terreno abierto y seco.
    Jarilla,
    /// Fachinal: un matorral de rebrotes de chañar, a lo largo de los bordes de
    /// las isletas.
    Fachinal,
}

/// Un arbusto en el plano del suelo. No es un obstáculo: un jinete pasa a
/// través de él (decisión del usuario, 2026-10-27).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shrub {
    pub at: Vec2,
    pub kind: ShrubKind,
    /// Altura hasta lo alto, en metros.
    pub height: f32,
    /// Qué tan ancho es respecto de la escala de la altura (1 es como está
    /// modelado).
    pub width: f32,
    /// Giro alrededor de la vertical, en radianes.
    pub yaw: f32,
    /// Aspecto con semilla: 32 bits aleatorios (el render elige la forma y los
    /// tonos).
    pub variant: u32,
}

/// Un tipo de mata de pasto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuftKind {
    /// Paja: pasto en mata alto y áspero.
    Paja,
    /// Flechilla: pasto de aguja bajo, fino y pálido.
    Flechilla,
}

/// Una mata de pasto. Las matas no son obstáculos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuft {
    pub at: Vec2,
    pub kind: TuftKind,
    /// Altura, en metros.
    pub size: f32,
    pub yaw: f32,
}

/// La flora de un chunk.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChunkFlora {
    pub trees: Vec<Tree>,
    pub tufts: Vec<Tuft>,
    pub shrubs: Vec<Shrub>,
}

/// Dónde puede crecer la flora: el mundo, el terreno nivelado de sus pueblos,
/// la línea del ferrocarril y los caminos.
#[derive(Debug, Clone, PartialEq)]
pub struct FloraSite {
    config: WorldConfig,
    flat_areas: Vec<FlatArea>,
    /// Las lagunas: el pasto se detiene en su barro húmedo, los árboles un poco
    /// más lejos.
    lagoons: Vec<Lagoon>,
    rail: Vec<(Vec2, Vec2)>,
    roads: Option<Arc<RoadNetwork>>,
    /// Los chunks hasta cuyos extras puede llegar un camino: cada chunk cuyo
    /// rectángulo, agrandado en [`Self::ESCAPE_ROAD_REACH`], toca los límites
    /// de un tramo de camino. Los demás no tienen ningún árbol escapado junto a
    /// un camino.
    road_chunks: HashSet<ChunkCoord>,
    /// Los árboles nativos de los últimos chunks, compartidos por las llamadas
    /// sobre este sitio.
    natives: NativeCache,
    /// Las líneas de alambrado relevadas: `x` de las norte–sur, `y` de las
    /// este–oeste.
    fences: (Vec<f32>, Vec<f32>),
    /// Las huellas de edificio, los patios y las huellas de tierra de los
    /// asentamientos.
    settlements: Option<Arc<KeepOut>>,
    /// Los centros de todos los asentamientos: la tierra alrededor de un casco,
    /// un puesto, una chacra o un rancho se desmonta para huerta, animales y
    /// cultivos, así que el monte se mantiene un poco alejado
    /// ([`Self::rural`]).
    worked: Vec<Vec2>,
    /// Si [`Self::rural`] escala los montes (siempre, salvo en las pruebas que
    /// miran la forma propia de un monte).
    redistribute: bool,
    seed: u32,
}

impl FloraSite {
    /// El máximo de árboles que planta un chunk: los del monte. Las isletas,
    /// tan densas como siempre, se detienen en [`Self::GROVE_BUDGET`]. Los
    /// presupuestos cuentan los árboles plantados: se quedan
    /// [`Self::TREE_KEEP`] de ellos, así que un chunk contiene como mucho
    /// alrededor de la mitad de estos.
    pub const TREE_BUDGET: usize = 160;
    /// El máximo de árboles que le agregan a un chunk sus isletas y sus árboles
    /// dispersos.
    pub const GROVE_BUDGET: usize = 90;
    /// El máximo de matas que contiene un chunk.
    pub const TUFT_BUDGET: usize = 600;
    /// Distancia mínima desde el tronco de un árbol hasta la línea central de
    /// la vía, en metros: la copa de un caldén se mantiene lejos de los
    /// vagones.
    pub const TREE_RAIL_MARGIN: f32 = 12.0;
    /// Distancia mínima desde una mata hasta la línea central de la vía, en
    /// metros: fuera del balasto.
    pub const TUFT_RAIL_MARGIN: f32 = 3.5;
    /// Distancia mínima desde el tronco de un árbol hasta la línea central de
    /// un camino, en metros: el camino, una banquina y el vuelo de la copa
    /// quedan libres.
    pub const TREE_ROAD_MARGIN: f32 = RoadNetwork::HALF_WIDTH + 6.0;
    /// Distancia mínima desde una mata hasta la línea central de un camino, en
    /// metros.
    pub const TUFT_ROAD_MARGIN: f32 = RoadNetwork::HALF_WIDTH + 0.5;
    /// Distancia mínima desde el tronco de un árbol hasta una línea de
    /// alambrado relevada, en metros: ningún alambrado atraviesa un tronco.
    pub const TREE_FENCE_MARGIN: f32 = 1.5;
    /// Distancia mínima desde un árbol hasta el borde del mundo, en metros.
    pub const TREE_EDGE_MARGIN: f32 = 10.0;
    /// Distancia mínima desde un árbol hasta la línea del agua de una laguna,
    /// en metros.
    pub const TREE_LAGOON_MARGIN: f32 = 12.0;
    /// Distancia mínima desde un árbol hasta la franja nivelada de un pueblo,
    /// en metros.
    pub const TREE_TOWN_MARGIN: f32 = 6.0;
    /// Distancia mínima desde el tronco de un árbol hasta las huellas de
    /// edificio, el patio y las huellas de tierra de un asentamiento, en
    /// metros: las copas se mantienen lejos de los techos y de los surcos.
    pub const TREE_SETTLEMENT_MARGIN: f32 = 4.0;
    /// Distancia mínima desde una mata, una piedra o un arbusto hasta las
    /// huellas de edificio, el patio y las huellas de tierra de un
    /// asentamiento, en metros.
    pub const TUFT_SETTLEMENT_MARGIN: f32 = 0.3;
    /// El máximo de arbustos que contiene un chunk.
    pub const SHRUB_BUDGET: usize = 300;
    /// Distancia mínima desde un arbusto hasta la línea central de la vía, en
    /// metros.
    pub const SHRUB_RAIL_MARGIN: f32 = 6.0;
    /// Distancia mínima desde un arbusto hasta la línea central de un camino,
    /// en metros.
    pub const SHRUB_ROAD_MARGIN: f32 = RoadNetwork::HALF_WIDTH + 1.5;
    /// Distancia mínima desde un arbusto hasta la franja nivelada de un pueblo,
    /// en metros.
    pub const SHRUB_TOWN_MARGIN: f32 = 2.0;
    /// Distancia mínima desde un arbusto hasta la línea del agua de una laguna,
    /// en metros.
    pub const SHRUB_LAGOON_MARGIN: f32 = 4.0;
    /// Distancia mínima desde un arbusto hasta las huellas de edificio, el
    /// patio y las huellas de tierra de un asentamiento, en metros.
    pub const SHRUB_SETTLEMENT_MARGIN: f32 = 1.5;
    /// Distancia mínima desde un arbusto hasta el borde del mundo, en metros.
    pub const SHRUB_EDGE_MARGIN: f32 = 3.0;
    /// Distancia mínima entre dos arbustos, en metros: los matorrales dejan
    /// pasillos por los que un jinete puede abrirse camino, más anchos que el
    /// collider del jinete.
    pub const SHRUB_SPACING: f32 = 1.9;
    /// Espacio libre entre un arbusto y el tronco de un árbol, en metros.
    const SHRUB_TRUNK_GAP: f32 = 0.6;
    /// Qué tan cerca del borde de un chunk debe estar un arbusto para que se
    /// comprueben los árboles del chunk del otro lado, en metros. Debe cubrir
    /// la mayor distancia a la que un tronco aleja a un arbusto: el tronco más
    /// grueso, el arbusto más grueso y [`Self::SHRUB_TRUNK_GAP`] (una prueba lo
    /// exige).
    const SHRUB_BORDER: f32 = 2.0;
    /// La proporción de los árboles plantados que se quedan (usuario, 2026-10-27:
    /// "sacale un 20% al total", después "sacale otro 30%": 0,8 x 0,7, y después "un
    /// 10% más de árboles": 0,56 x 1,1): monte, isletas y árboles solitarios se ralean
    /// por igual. Los arbustos no se ralean.
    pub const TREE_KEEP: f32 = 0.616;
    /// A esta distancia o menos del terreno nivelado de un pueblo (en metros),
    /// el monte y las isletas conservan [`Self::TOWN_NEAR_SHARE`] de su
    /// densidad.
    pub const TOWN_NEAR: f32 = 1500.0;
    /// La proporción de su densidad que conserva la tierra justo alrededor de
    /// un pueblo.
    pub const TOWN_NEAR_SHARE: f32 = 0.12;
    /// Desde esta distancia (en metros) la densidad es la normal; más allá
    /// crece hasta [`Self::FAR_BOOST`].
    pub const TOWN_FULL: f32 = 3000.0;
    /// A qué distancia de un pueblo (en metros) está el campo abierto cuando
    /// alcanza su máxima densidad.
    pub const FAR_FULL: f32 = 9000.0;
    /// Cuántas veces más denso que la densidad normal es el campo lejano.
    pub const FAR_BOOST: f32 = 1.6;
    /// Cuánto bajan (en unidades de ruido) los bordes del monte y de las
    /// isletas del campo lejano por cada unidad de [`Self::rural`] por encima
    /// de 1.
    const FAR_WIDEN: f32 = 0.12;
    /// La proporción de su densidad que conserva la tierra en el centro de un
    /// asentamiento.
    pub const WORKED_SHARE: f32 = 0.3;
    /// A esta distancia o menos del centro de un asentamiento (en metros) la
    /// tierra está trabajada.
    pub const WORKED_NEAR: f32 = 120.0;
    /// Desde esta distancia (en metros) la tierra alrededor de un casco es tan
    /// densa como cualquier otra.
    pub const WORKED_FULL: f32 = 500.0;
    /// Candidatos a árbol que se prueban por chunk de [`Self::REFERENCE_CHUNK`]
    /// metros.
    const TREE_CANDIDATES: f32 = 800.0;
    /// Candidatos a arbusto que se prueban por chunk de
    /// [`Self::REFERENCE_CHUNK`] metros.
    const SHRUB_CANDIDATES: f32 = 500.0;
    /// Candidatos a mata que se prueban por chunk de [`Self::REFERENCE_CHUNK`]
    /// metros.
    const TUFT_CANDIDATES: f32 = 600.0;
    /// El lado de chunk para el que están ajustadas las cantidades de
    /// candidatos, en metros.
    const REFERENCE_CHUNK: f32 = 200.0;
    /// Probabilidad de que un candidato en la pampa abierta se convierta en un
    /// árbol solitario.
    const LONE_TREE: f32 = 0.0012;
    /// Probabilidad extra dentro de una isleta (escalada por la máscara).
    const GROVE_TREE: f32 = 0.4;
    /// La parte de la probabilidad de una isleta que se mantiene entre sus
    /// grupos: sus árboles se juntan en grupos con claros entre ellos.
    const CLUMP_FLOOR: f32 = 0.02;
    /// Cuánto más probable es un candidato en el medio de un nudo (ver
    /// [`Self::knot`]) que fuera de todos: los árboles siembran a sus vecinos,
    /// así que un monte se agrupa en matas con huecos entre ellas.
    const KNOT_BOOST: f32 = 6.0;
    /// Probabilidad de que un candidato en el corazón de una mancha de monte se
    /// convierta en un árbol.
    const MONTE_TREE: f32 = 0.2;
    /// La parte de la probabilidad del monte que se mantiene entre sus nudos:
    /// el monte es denso en toda su extensión, con claros.
    const MONTE_FLOOR: f32 = 0.4;
    /// Proporción acumulada de los árboles que son renuevos, jóvenes y adultos
    /// (el resto son gigantes viejos), en terreno abierto y en el corazón del
    /// monte: el monte es un bosque joven que está rebrotando.
    const AGE_SHARES: [f32; 3] = [0.07, 0.21, 0.965];
    const MONTE_AGE_SHARES: [f32; 3] = [0.22, 0.50, 0.97];

    /// Probabilidad, por lugar candidato en el mejor anillo de un asentamiento,
    /// de un ombú ([`Self::ombu_zone`]).
    const OMBU_TREE: f32 = 0.0025;
    /// Probabilidad, por lugar candidato junto a un camino o un asentamiento,
    /// de un árbol de pueblo escapado además de los árboles propios del campo
    /// ([`Self::escape`]): los renuevos de las plantaciones, que se dejaron
    /// crecer.
    const ESCAPED_TREE: f32 = 0.03;
    /// Proporción de los árboles del monte que son algarrobos (un compañero del
    /// caldén), en el corazón del monte.
    const ALGARROBO_SHARE: f32 = 0.12;
    /// Proporción de los árboles que son talas, y de los que son espinillos, en
    /// los bordes de las isletas y del monte (en otros lugares, una fracción de
    /// eso; ver [`Self::EDGE_FLOOR`]).
    const EDGE_SHARE: f32 = 0.04;
    /// La parte de [`Self::EDGE_SHARE`] que conservan el tala y el espinillo
    /// fuera de los bordes: unos pocos crecen solos en terreno abierto o
    /// alterado.
    const EDGE_FLOOR: f32 = 0.3;
    /// Proporción de los árboles que son especies de pueblo en el campo
    /// abierto, y donde [`Self::escape`] vale 1 (junto a un camino o un
    /// asentamiento).
    const TOWN_SHARE_OPEN: f32 = 0.004;
    const TOWN_SHARE_NEAR: f32 = 0.09;
    /// Cómo se reparten los árboles de pueblo escapados entre paraíso, olmo,
    /// eucalipto y aromo.
    const TOWN_MIX: [f32; 4] = [0.35, 0.15, 0.30, 0.20];
    /// Proporción acumulada de los ombúes que son jóvenes y adultos (el resto
    /// son viejos): son árboles plantados, ninguno es un renuevo.
    const OMBU_AGE_SHARES: [f32; 3] = [0.0, 0.2, 0.8];

    /// A qué distancia del centro de un asentamiento se puede plantar un ombú,
    /// en metros: al lado del casco, el puesto o el rancho, nunca en campo
    /// abierto.
    pub const OMBU_REACH: f32 = 140.0;
    /// Los caminos están bordeados de árboles de pueblo escapados (paraíso,
    /// olmo, eucalipto, aromo) hasta esta distancia de su línea central, en
    /// metros.
    pub const ESCAPE_ROAD_REACH: f32 = 60.0;
    /// Alrededor del centro de un asentamiento, los árboles de pueblo escapados
    /// se ralean entre [`Self::ESCAPE_HUB_NEAR`] y esta distancia, en metros.
    pub const ESCAPE_HUB_FAR: f32 = 260.0;
    /// Ver [`Self::ESCAPE_HUB_FAR`].
    pub const ESCAPE_HUB_NEAR: f32 = 60.0;

    /// La flora del mundo de `config` sobre `terrain` (ya nivelado para sus
    /// pueblos, con sus caminos trazados), fuera de la vía de `line` y de los
    /// caminos.
    pub fn new(config: WorldConfig, terrain: &Terrain, line: &RailLine) -> Self {
        let size = config.chunk_size;
        let mut road_chunks = HashSet::new();
        for (a, b) in terrain.roads.iter().flat_map(|r| r.segments()) {
            let reach = Vec2::new(Self::ESCAPE_ROAD_REACH, Self::ESCAPE_ROAD_REACH);
            let lo = ChunkCoord::containing(Vec2::new(a.x.min(b.x), a.y.min(b.y)) - reach, size);
            let hi = ChunkCoord::containing(Vec2::new(a.x.max(b.x), a.y.max(b.y)) + reach, size);
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    road_chunks.insert(ChunkCoord::new(x, y));
                }
            }
        }
        Self {
            config,
            flat_areas: terrain.flat_areas.clone(),
            lagoons: terrain.lagoons.clone(),
            rail: line.segments().map(|s| (s.start, s.end)).collect(),
            roads: terrain.roads.clone(),
            road_chunks,
            natives: NativeCache::new(NativeCache::DEFAULT_CAP),
            fences: (Vec::new(), Vec::new()),
            settlements: None,
            worked: Vec::new(),
            redistribute: true,
            seed: terrain.seed ^ FLORA_SEED,
        }
    }

    /// Esta flora, además fuera de cada tramo de todo el `path` del tren
    /// ([`crate::RailLine::full_path`]): los enlaces y la vía recta propia de
    /// cada pueblo con sus tramos finales más allá del pueblo, que los enlaces
    /// de [`Self::new`] por sí solos dejan fuera.
    pub fn with_track(mut self, path: &LinePath) -> Self {
        self.rail
            .extend(path.points().windows(2).map(|w| (w[0], w[1])));
        self.natives.clear();
        self
    }

    /// Si `p` está al menos a `margin` de cada tramo de vía del que se mantiene
    /// lejos esta flora (ver [`Self::with_track`]): la única zona prohibida que
    /// las piedras y los arbustos del terreno comparten con los árboles y las
    /// matas.
    pub fn off_track(&self, p: Vec2, margin: f32) -> bool {
        rail_distance(&self.rail, p) >= margin
    }

    /// Si una mata, una piedra o un arbusto puede crecer en `p`: lejos del agua
    /// de toda laguna y del angosto barro húmedo a lo largo de ella
    /// ([`Lagoon::MUD_BAND`]). Los juncos se dibujan por separado.
    pub fn off_lagoons(&self, p: Vec2) -> bool {
        self.lagoons.iter().all(|l| !l.is_bare(p))
    }

    /// Si un árbol puede estar en `p`: a [`Self::TREE_LAGOON_MARGIN`] más allá
    /// de la línea del agua de toda laguna.
    fn tree_off_lagoons(&self, p: Vec2) -> bool {
        self.lagoons
            .iter()
            .all(|l| l.offshore(p) >= Self::TREE_LAGOON_MARGIN)
    }

    /// Esta flora, con los árboles fuera de las líneas relevadas de `fences`.
    pub fn with_fences(mut self, fences: &FenceLayout) -> Self {
        self.fences = fences.surveyed.clone();
        self.natives.clear();
        self
    }

    /// Esta flora, además fuera de todo asentamiento de `settlements`: sus
    /// huellas de edificio, su patio y sus huellas de tierra.
    pub fn with_settlements(mut self, settlements: &SettlementLayout) -> Self {
        self.settlements = Some(settlements.keep_out.clone());
        self.worked = settlements.settlements.iter().map(|s| s.hub).collect();
        self.natives.clear();
        self
    }

    /// Si `p` está a más de `margin` metros (como mucho
    /// [`KeepOut::MAX_MARGIN`]) de todo asentamiento del que se mantiene lejos
    /// esta flora (ver [`Self::with_settlements`]): una comprobación indexada y
    /// barata que las piedras y los arbustos del terreno comparten con los
    /// árboles y las matas.
    pub fn off_settlements(&self, p: Vec2, margin: f32) -> bool {
        self.settlements
            .as_ref()
            .is_none_or(|k| k.is_clear(p, margin))
    }

    /// El mundo en el que crece esta flora.
    pub fn config(&self) -> WorldConfig {
        self.config
    }

    /// Los árboles, las matas y los arbustos del chunk `coord`, con su
    /// coordenada como semilla. Los chunks fuera del mundo no tienen nada.
    pub fn chunk(&self, coord: ChunkCoord) -> ChunkFlora {
        if !self.config.has_chunk(coord) {
            return ChunkFlora::default();
        }
        let rail = self.rail_near(coord);
        let trees = self.chunk_trees(coord, &rail);
        let tufts = self.chunk_tufts(coord, &rail, &trees);
        let shrubs = self.chunk_shrubs(coord, &rail, &trees);
        ChunkFlora {
            trees: self.thinned(trees),
            tufts,
            shrubs,
        }
    }

    /// Los arbustos del chunk `coord` (más barato que [`Self::chunk`]).
    pub fn shrubs(&self, coord: ChunkCoord) -> Vec<Shrub> {
        if !self.config.has_chunk(coord) {
            return Vec::new();
        }
        let rail = self.rail_near(coord);
        let trees = self.chunk_trees(coord, &rail);
        self.chunk_shrubs(coord, &rail, &trees)
    }

    /// Los árboles del chunk `coord` (más barato que [`Self::chunk`]).
    pub fn trees(&self, coord: ChunkCoord) -> Vec<Tree> {
        if !self.config.has_chunk(coord) {
            return Vec::new();
        }
        self.thinned(self.chunk_trees(coord, &self.rail_near(coord)))
    }

    /// Los tramos de vía cercanos al chunk `coord`.
    fn rail_near(&self, coord: ChunkCoord) -> Vec<(Vec2, Vec2)> {
        let area = coord.bounds(self.config.chunk_size);
        let reach = area.expanded(Self::TREE_RAIL_MARGIN);
        self.rail
            .iter()
            .copied()
            .filter(|&(a, b)| reach.segment_overlap(a, b).is_some())
            .collect()
    }

    /// Todos los árboles del mundo, chunk por chunk: los mismos árboles que da
    /// [`Self::chunk`].
    pub fn all_trees(&self) -> Vec<Tree> {
        let size = self.config.chunk_size;
        let bounds = self.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        let mut trees = Vec::new();
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                trees.extend(self.trees(ChunkCoord::new(x, y)));
            }
        }
        trees
    }

    /// Todos los arbustos del mundo, chunk por chunk: los mismos arbustos que
    /// da [`Self::chunk`].
    pub fn all_shrubs(&self) -> Vec<Shrub> {
        let size = self.config.chunk_size;
        let bounds = self.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        let mut shrubs = Vec::new();
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                shrubs.extend(self.shrubs(ChunkCoord::new(x, y)));
            }
        }
        shrubs
    }

    /// Todos los árboles y todos los arbustos del mundo, chunk por chunk: los
    /// mismos que dan [`Self::all_trees`] y [`Self::all_shrubs`], calculando
    /// los árboles de cada chunk una sola vez para ambos.
    pub fn all_plants(&self) -> (Vec<Tree>, Vec<Shrub>) {
        let size = self.config.chunk_size;
        let bounds = self.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        let (mut trees, mut shrubs) = (Vec::new(), Vec::new());
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let coord = ChunkCoord::new(x, y);
                let rail = self.rail_near(coord);
                let own = self.chunk_trees(coord, &rail);
                shrubs.extend(self.chunk_shrubs(coord, &rail, &own));
                trees.extend(self.thinned(own));
            }
        }
        (trees, shrubs)
    }

    /// Con cuánta fuerza la máscara de isletas favorece los árboles en `p`, en
    /// `[0, 1]`: ruido de baja frecuencia, para que los árboles se agrupen en
    /// pequeñas isletas.
    pub fn grove(&self, p: Vec2) -> f32 {
        self.grove_wide(p, 0.0)
    }

    /// [`Self::grove`] con sus bordes bajados en `widen`, para que las isletas
    /// cubran más terreno (el campo lejano, [`Self::rural`]).
    fn grove_wide(&self, p: Vec2, widen: f32) -> f32 {
        let n =
            (value_noise(self.seed, p, 170.0) + 0.3 * value_noise(self.seed ^ 0x51, p, 65.0)) / 1.3;
        smoothstep(0.56 - widen, 0.84 - widen, n)
    }

    /// Con cuánta fuerza se agrupan los árboles en `p`, en `[0, 1]`: ruido
    /// sobre unas pocas decenas de metros, para que una isleta se divida en
    /// grupos con claros.
    pub fn clump(&self, p: Vec2) -> f32 {
        let n = (value_noise(self.seed ^ 0xc1a7, p, 44.0)
            + 0.5 * value_noise(self.seed ^ 0x9d, p, 20.0))
            / 1.5;
        smoothstep(0.5, 0.6, n)
    }

    /// Con cuánta fuerza siembran los árboles a sus vecinos en `p`, en `[0,
    /// 1]`: ruido sobre unos pocos metros, para que un monte se agrupe en nudos
    /// de árboles con huecos entre ellos. Es un campo del mundo, no de un
    /// chunk, así que los nudos atraviesan los bordes de los chunks.
    pub fn knot(&self, p: Vec2) -> f32 {
        let n = (value_noise(self.seed ^ 0x6b07, p, 17.0)
            + 0.6 * value_noise(self.seed ^ 0x3c1, p, 8.0))
            / 1.6;
        smoothstep(0.42, 0.62, n)
    }

    /// Qué parte de un monte (un caldenal denso, bosque) hay en `p`, en `[0,
    /// 1]`: ruido a escala del mundo, para que las manchas midan unos cientos
    /// de metros y atraviesen los bordes de los chunks. Entre ellas, la pampa
    /// es pasto abierto con sus árboles dispersos y sus isletas.
    pub fn monte(&self, p: Vec2) -> f32 {
        self.monte_wide(p, 0.0)
    }

    /// [`Self::monte`] con sus bordes bajados en `widen`, para que las manchas
    /// cubran más terreno (el campo lejano, [`Self::rural`]).
    fn monte_wide(&self, p: Vec2, widen: f32) -> f32 {
        let n = (value_noise(self.seed ^ 0x30e7, p, 480.0)
            + 0.35 * value_noise(self.seed ^ 0x30e8, p, 170.0))
            / 1.35;
        smoothstep(0.585 - widen, 0.685 - widen, n)
    }

    /// Qué tan seco está el pasto en `p`, en `[0, 1]`: 0 es verde grisáceo, 1
    /// amarillo seco. De baja frecuencia y continuo.
    pub fn dryness(&self, p: Vec2) -> f32 {
        dryness_of(self.seed, p)
    }

    /// Los árboles de `planted` que se quedan.
    fn thinned(&self, mut planted: Vec<Tree>) -> Vec<Tree> {
        planted.retain(|t| self.kept(t.at));
        planted
    }

    /// Cuánto de su monte, sus isletas y sus árboles solitarios conserva la
    /// tierra en `p`, según qué tan lejos está de un pueblo y de un casco
    /// (usuario, 2026-10-27: "cerca de los poblados no suele haber mucho monte,
    /// pero suele haber en el campo"). Un campo suave del mundo, así que
    /// atraviesa los bordes de los chunks como las máscaras que escala:
    /// [`Self::TOWN_NEAR_SHARE`] a [`Self::TOWN_NEAR`] o menos del terreno
    /// nivelado de un pueblo, subiendo suavemente hasta 1 en
    /// [`Self::TOWN_FULL`] y hasta [`Self::FAR_BOOST`] en [`Self::FAR_FULL`]
    /// (la proporción que pierden los alrededores de los pueblos pasa al campo
    /// abierto); y [`Self::WORKED_SHARE`] en el centro de un asentamiento,
    /// volviendo a 1 en [`Self::WORKED_FULL`].
    pub fn rural(&self, p: Vec2) -> f32 {
        self.rural_by(p, self.hub_distance(p))
    }

    /// Distancia desde `p` hasta el centro de asentamiento más cercano, en
    /// metros (infinita si no hay asentamientos).
    fn hub_distance(&self, p: Vec2) -> f32 {
        self.worked
            .iter()
            .map(|h| h.distance(p))
            .fold(f32::MAX, f32::min)
    }

    /// [`Self::rural`], con la distancia al centro más cercano ya calculada.
    fn rural_by(&self, p: Vec2, casco: f32) -> f32 {
        if !self.redistribute {
            return 1.0;
        }
        let town = self
            .flat_areas
            .iter()
            .map(|a| outside(a, p))
            .fold(f32::MAX, f32::min);
        let by_town = Self::TOWN_NEAR_SHARE
            + (1.0 - Self::TOWN_NEAR_SHARE) * smoothstep(Self::TOWN_NEAR, Self::TOWN_FULL, town)
            + (Self::FAR_BOOST - 1.0) * smoothstep(Self::TOWN_FULL, Self::FAR_FULL, town);
        let by_casco = Self::WORKED_SHARE
            + (1.0 - Self::WORKED_SHARE) * smoothstep(Self::WORKED_NEAR, Self::WORKED_FULL, casco);
        by_town * by_casco
    }

    /// Qué parte del terreno de un ombú es `p`, en `[0, 1]`, a `hub` metros del
    /// centro del asentamiento más cercano: un anillo al lado del casco, el
    /// puesto o el rancho (fuera del patio y del terreno de alrededor, que se
    /// mantienen libres de árboles), que desaparece en [`Self::OMBU_REACH`].
    /// Más allá vale 0, así que ningún ombú crece en campo abierto.
    fn ombu_zone(hub: f32) -> f32 {
        smoothstep(20.0, 45.0, hub)
            * (1.0 - smoothstep(0.6 * Self::OMBU_REACH, Self::OMBU_REACH, hub))
    }

    /// Cuánto favorecen a `p` los árboles de pueblo escapados, en `[0, 1]`: 1
    /// junto a un camino (pasando el margen que los árboles mantienen respecto
    /// de él) y alrededor del centro de un asentamiento, bajando suavemente
    /// hasta 0 a [`Self::ESCAPE_ROAD_REACH`] del camino y a
    /// [`Self::ESCAPE_HUB_FAR`] de un centro; 0 en campo abierto.
    pub fn escape(&self, p: Vec2) -> f32 {
        self.escape_by(p, self.hub_distance(p))
    }

    fn escape_by(&self, p: Vec2, hub: f32) -> f32 {
        let by_hub = 1.0 - smoothstep(Self::ESCAPE_HUB_NEAR, Self::ESCAPE_HUB_FAR, hub);
        let by_road = self
            .roads
            .as_ref()
            .and_then(|r| r.near(p, Self::ESCAPE_ROAD_REACH))
            .map_or(0.0, |d| {
                1.0 - smoothstep(Self::TREE_ROAD_MARGIN, Self::ESCAPE_ROAD_REACH, d)
            });
        by_hub.max(by_road)
    }

    /// La especie de un árbol de campo en un lugar con las máscaras `grove`,
    /// `monte` y `fringe` dadas y la proporción `escape`, a partir de `roll`,
    /// un valor uniforme en `[0, 1)`. El caldén y el chañar son la mayor parte
    /// del campo; el algarrobo acompaña al caldén en el monte, el tala y el
    /// espinillo son unos pocos en los bordes y en terreno abierto, y las
    /// especies de pueblo son todavía más raras, y más comunes junto a caminos
    /// y asentamientos.
    fn species_at(roll: f32, grove: f32, monte: f32, fringe: f32, escape: f32) -> Species {
        let edge = Self::EDGE_SHARE * (Self::EDGE_FLOOR + (1.0 - Self::EDGE_FLOOR) * fringe);
        let bands = [
            (Species::Algarrobo, Self::ALGARROBO_SHARE * monte),
            (Species::Tala, edge),
            (Species::Espinillo, edge),
        ];
        let town = Self::TOWN_SHARE_OPEN + (Self::TOWN_SHARE_NEAR - Self::TOWN_SHARE_OPEN) * escape;
        let mut at = roll;
        for (species, share) in bands {
            if at < share {
                return species;
            }
            at -= share;
        }
        if at < town {
            return Self::town_species(at / town);
        }
        let rest = (at - town) / (1.0 - town - bands.iter().map(|b| b.1).sum::<f32>());
        // Los caldenes dominan el núcleo de las isletas; los chañares llenan
        // sus bordes y el terreno abierto, y el sotobosque del monte.
        let edge = 4.0 * grove * (1.0 - grove);
        let calden = (0.3 + 0.45 * grove - 0.2 * edge + 0.3 * monte).clamp(0.1, 0.9);
        if rest < calden {
            Species::Calden
        } else {
            Species::Chanar
        }
    }

    /// La especie de pueblo que elige un `roll` en `[0, 1)`, según
    /// [`Self::TOWN_MIX`].
    fn town_species(roll: f32) -> Species {
        let mut at = roll;
        for (species, share) in [
            Species::Paraiso,
            Species::Olmo,
            Species::Eucalipto,
            Species::Aromo,
        ]
        .into_iter()
        .zip(Self::TOWN_MIX)
        {
            if at < share {
                return species;
            }
            at -= share;
        }
        Species::Aromo
    }

    /// Si el árbol plantado en `at` se queda: lo hacen [`Self::TREE_KEEP`] de
    /// ellos, elegidos solo por el lugar, así que se van los mismos árboles sea
    /// cual sea el chunk que pregunte y el raleo es parejo en el monte, las
    /// isletas y los árboles solitarios por igual. Un árbol raleado igual ocupa
    /// su lugar mientras se planta el monte (sus vecinos mantienen la
    /// distancia), así que el monte conserva sus grupos y claros y solo queda
    /// con huecos.
    fn kept(&self, at: Vec2) -> bool {
        Rng::new(self.seed ^ at.x.to_bits(), at.y.to_bits()).unit() < Self::TREE_KEEP
    }

    /// Los árboles del chunk `coord` antes del raleo, separados entre sí dentro de él y
    /// respecto de los árboles de sus vecinos: donde un árbol queda demasiado cerca de uno
    /// de un vecino anterior (la fila de abajo, o el chunk del oeste), cede el árbol del
    /// chunk posterior. Cada chunk se sigue decidiendo por separado (siempre los mismos
    /// árboles, en cualquier orden), a partir de los árboles de los vecinos antes de que
    /// cedieran ante los suyos.
    ///
    /// Son aquellos de los que se mantienen lejos los arbustos y las matas, así que quedan
    /// como estaban cuando se ralearon los árboles.
    fn chunk_trees(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)]) -> Vec<Tree> {
        let size = self.config.chunk_size;
        debug_assert_eq!(rail, self.rail_near(coord));
        let mut natives = self.cached_natives(coord).to_vec();
        let mut extras = self.extra_trees(coord, rail, &natives);
        for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0)] {
            let earlier = ChunkCoord::new(coord.x + dx, coord.y + dy);
            let reach = earlier.bounds(size).expanded(Tree::MAX_SPACING);
            let near = |t: &Tree| reach.contains_point(t.at);
            if !self.config.has_chunk(earlier) || !(natives.iter().chain(&extras).any(near)) {
                continue;
            }
            // Los nativos ceden solo ante los nativos anteriores, así que los
            // extras nunca desplazan a uno; los extras ceden ante los extras
            // anteriores (y se mantienen lejos de todos los nativos según su
            // propio sorteo).
            let rail = self.rail_near(earlier);
            let theirs = self.cached_natives(earlier);
            natives.retain(|t| {
                theirs
                    .iter()
                    .all(|u| u.at.distance(t.at) >= u.spacing().max(t.spacing()))
            });
            let theirs = self.extra_trees(earlier, &rail, &theirs);
            extras.retain(|t| {
                theirs
                    .iter()
                    .all(|u| u.at.distance(t.at) >= u.spacing().max(t.spacing()))
            });
        }
        natives.extend(extras);
        natives
    }

    /// Los árboles distribuidos sobre el chunk `coord` (los propios del campo y
    /// los extras), separados entre sí dentro de él.
    fn scattered_trees(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)]) -> Vec<Tree> {
        let mut trees = self.native_trees(coord, rail);
        let extras = self.extra_trees(coord, rail, &trees);
        trees.extend(extras);
        trees
    }

    /// [`Self::native_trees`] del chunk `coord` sobre su propio riel, desde la
    /// caché cuando los tiene.
    fn cached_natives(&self, coord: ChunkCoord) -> Arc<[Tree]> {
        self.natives.get_or_insert((coord, self.redistribute), || {
            self.native_trees(coord, &self.rail_near(coord))
        })
    }

    #[cfg(test)]
    fn with_native_cap(mut self, cap: usize) -> Self {
        self.natives = NativeCache::new(cap);
        self
    }

    /// Los árboles propios del campo del chunk `coord` (el monte, las isletas y
    /// los árboles solitarios), separados entre sí dentro de él. Aquí no se
    /// sortea nada más, así que quedan como siempre estuvieron, agregue lo que
    /// agregue [`Self::extra_trees`].
    fn native_trees(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)]) -> Vec<Tree> {
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 0);
        let candidates = self.scaled(Self::TREE_CANDIDATES);
        let mut trees: Vec<Tree> = Vec::new();
        for _ in 0..candidates {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            let (pick, species_roll, age_roll, height_roll) =
                (rng.unit(), rng.unit(), rng.unit(), rng.unit());
            let (yaw, width_roll) = (rng.unit(), rng.unit());
            let variant = rng.next();
            if trees.len() >= Self::TREE_BUDGET {
                break;
            }
            // Ralos cerca de los pueblos y de los cascos (la probabilidad se
            // reduce), y más extendidos lejos (las manchas y las isletas se
            // expanden: los montes ya son tan densos como pueden, así que hay
            // más, no más densos); los árboles solitarios conservan un poco más
            // que los montes.
            let hub = self.hub_distance(at);
            let rural = self.rural_by(at, hub);
            let widen = Self::FAR_WIDEN * (rural - 1.0).max(0.0);
            let grove = self.grove_wide(at, widen);
            let monte = self.monte_wide(at, widen);
            // La probabilidad de la isleta se agrupa en grupos, y los árboles
            // siembran a sus vecinos en nudos; el monte es denso en toda su
            // extensión, con claros. Ambos son campos del mundo, así que un
            // grupo sigue más allá del borde de un chunk.
            let knot = self.knot(at);
            let gather = Self::CLUMP_FLOOR + (1.0 - Self::CLUMP_FLOOR) * self.clump(at);
            let seeding = 1.0 + (Self::KNOT_BOOST - 1.0) * knot;
            let grove_chance = Self::GROVE_TREE * grove * gather * seeding;
            let monte_chance =
                Self::MONTE_TREE * monte * (Self::MONTE_FLOOR + (1.0 - Self::MONTE_FLOOR) * knot);
            let thin = rural.min(1.0);
            let (grove_chance, monte_chance) = (grove_chance * thin, monte_chance * thin);
            let chance = Self::LONE_TREE * rural.sqrt() + grove_chance.max(monte_chance);
            let escape = self.escape_by(at, hub);
            let species = if pick >= chance {
                continue;
            } else if trees.len() >= Self::GROVE_BUDGET && monte_chance <= grove_chance {
                // Una isleta se detiene en su presupuesto; solo el monte sigue
                // más allá.
                continue;
            } else {
                let fringe = Self::fringe_of(grove, monte);
                Self::species_at(species_roll, grove, monte, fringe, escape)
            };
            let rolls = [age_roll, height_roll, yaw, width_roll];
            let tree = Self::grow(at, species, monte, rolls, variant);
            let spaced = trees
                .iter()
                .all(|t| t.at.distance(at) >= t.spacing().max(tree.spacing()));
            if spaced && self.tree_room(at, rail) {
                trees.push(tree);
            }
        }
        trees
    }

    /// El árbol de especie `species` que crece en `at`, en un lugar con la
    /// máscara `monte` dada, a partir de los valores aleatorios `rolls` de
    /// edad, altura, giro y ancho.
    fn grow(at: Vec2, species: Species, monte: f32, rolls: [f32; 4], variant: u32) -> Tree {
        let [age_roll, height_roll, yaw, width_roll] = rolls;
        let shares = [0, 1, 2].map(|i| {
            if species == Species::Ombu {
                Self::OMBU_AGE_SHARES[i]
            } else {
                Self::AGE_SHARES[i] + (Self::MONTE_AGE_SHARES[i] - Self::AGE_SHARES[i]) * monte
            }
        });
        let age = match shares.iter().position(|&c| age_roll < c) {
            Some(0) => Age::Sapling,
            Some(1) => Age::Young,
            Some(_) => Age::Mature,
            None => Age::Old,
        };
        let (lo, hi) = age.height_range(species);
        // Un gigante viejo es ancho; el resto, un poco angosto o ancho.
        let width = match age {
            // El tronco de un ombú es el más ancho para el collider: se
            // mantiene moderado de ancho, sea cual sea su edad.
            _ if species == Species::Ombu => 0.95 + 0.2 * width_roll,
            Age::Old => 1.15 + 0.25 * width_roll,
            _ => 0.82 + 0.36 * width_roll,
        };
        Tree {
            at,
            species,
            age,
            height: lo + (hi - lo) * height_roll,
            width,
            yaw: yaw * std::f32::consts::TAU,
            variant,
        }
    }

    /// Los árboles que se suman a los propios del campo en el chunk `coord`: un
    /// ombú al lado de un asentamiento, y los árboles de pueblo que se
    /// escaparon por los caminos y los asentamientos. Se plantan después de los
    /// nativos, separados de ellos (y de los de los chunks vecinos, antes de
    /// que estos cedan ante los suyos), así que nunca ocupan el lugar de un
    /// nativo. Cada uno se sortea a partir de un hash de su lugar con su propia
    /// sal, así que el mismo lugar da el mismo árbol sea cual sea el chunk que
    /// pregunte.
    ///
    /// Se reciben los `natives` del chunk. Los extras no se escalan con
    /// [`Self::rural`] (la tierra que los pueblos desmontan es donde están los
    /// árboles plantados) ni se limitan con [`Self::GROVE_BUDGET`] (que detiene
    /// las isletas); se detienen en [`Self::TREE_BUDGET`] junto con los
    /// nativos, y [`Self::thinned`] los ralea como a cualquier otro árbol.
    fn extra_trees(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)], natives: &[Tree]) -> Vec<Tree> {
        const SALT: u32 = 0x0e57_7a11;
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 3);
        let mut extras: Vec<Tree> = Vec::new();
        // Ningún centro llega hasta el chunk: más allá de ambos alcances los
        // centros no favorecen nada, así que no se calcula la distancia por
        // lugar.
        let reach = Self::OMBU_REACH.max(Self::ESCAPE_HUB_FAR);
        let hub_free = self.worked.iter().all(|h| {
            let b = coord.bounds(size);
            let nearest = Vec2::new(h.x.clamp(b.min.x, b.max.x), h.y.clamp(b.min.y, b.max.y));
            nearest.distance(*h) > reach
        });
        // Tampoco hay un camino al alcance de los árboles escapados desde el
        // chunk.
        let road_free = !self.road_chunks.contains(&coord);
        if hub_free && road_free {
            return extras;
        }
        for _ in 0..self.scaled(Self::TREE_CANDIDATES) {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            if natives.len() + extras.len() >= Self::TREE_BUDGET {
                break;
            }
            let hub = if hub_free {
                f32::MAX
            } else {
                self.hub_distance(at)
            };
            let ombu = Self::OMBU_TREE * Self::ombu_zone(hub);
            let escaped = Self::ESCAPED_TREE * self.escape_by(at, hub);
            if ombu + escaped <= 0.0 {
                continue;
            }
            let mut draw = Rng::new(self.seed ^ SALT ^ at.x.to_bits(), at.y.to_bits());
            let pick = draw.unit();
            if pick >= ombu + escaped {
                continue;
            }
            let species = if pick < ombu {
                Species::Ombu
            } else {
                Self::town_species(draw.unit())
            };
            let rolls = [draw.unit(), draw.unit(), draw.unit(), draw.unit()];
            let tree = Self::grow(at, species, self.monte(at), rolls, draw.next());
            let apart = |t: &Tree| t.at.distance(at) >= t.spacing().max(tree.spacing());
            if !extras.iter().all(apart) || !natives.iter().all(apart) || !self.tree_room(at, rail)
            {
                continue;
            }
            // Lejos también de los nativos de los chunks vecinos, tal como
            // están antes de ceder: un superconjunto de lo que se planta ahí.
            let mut clear = true;
            for (dx, dy) in [
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let other = ChunkCoord::new(coord.x + dx, coord.y + dy);
                let reach = other.bounds(size).expanded(Tree::MAX_SPACING);
                if !clear || !self.config.has_chunk(other) || !reach.contains_point(at) {
                    continue;
                }
                clear = self.cached_natives(other).iter().all(apart);
            }
            if clear {
                extras.push(tree);
            }
        }
        extras
    }

    fn chunk_tufts(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)], trees: &[Tree]) -> Vec<Tuft> {
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 1);
        let candidates = self.scaled(Self::TUFT_CANDIDATES);
        let bounds = self.config.bounds();
        let mut tufts = Vec::new();
        for _ in 0..candidates {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            let (kind_roll, size_roll, yaw) = (rng.unit(), rng.unit(), rng.unit());
            if tufts.len() >= Self::TUFT_BUDGET {
                break;
            }
            let inside = at.x - bounds.min.x >= 0.5
                && bounds.max.x - at.x >= 0.5
                && at.y - bounds.min.y >= 0.5
                && bounds.max.y - at.y >= 0.5;
            let open = inside
                && self.flat_areas.iter().all(|a| a.weight(at) == 0.0)
                && self.off_lagoons(at)
                && rail_distance(rail, at) >= Self::TUFT_RAIL_MARGIN
                && self.off_road(at, Self::TUFT_ROAD_MARGIN)
                && self.off_settlements(at, Self::TUFT_SETTLEMENT_MARGIN)
                && trees
                    .iter()
                    .all(|t| t.at.distance(at) > t.trunk_radius() + 0.4);
            if !open {
                continue;
            }
            // La paja se agrupa donde el pasto es más verde; la flechilla, en
            // terreno seco.
            let kind = if kind_roll < 0.25 + 0.4 * (1.0 - self.dryness(at)) {
                TuftKind::Paja
            } else {
                TuftKind::Flechilla
            };
            let (lo, hi) = kind.size_range();
            tufts.push(Tuft {
                at,
                kind,
                size: lo + (hi - lo) * size_roll,
                yaw: yaw * std::f32::consts::TAU,
            });
        }
        tufts
    }

    /// Con cuánta fuerza se agrupan los arbustos en `p`, en `[0, 1]`: ruido
    /// sobre unas pocas decenas de metros, para que el matorral crezca en matas
    /// espesas con terreno abierto entre ellas.
    fn thicket(&self, p: Vec2) -> f32 {
        let n = (value_noise(self.seed ^ 0x7a1c, p, 30.0)
            + 0.5 * value_noise(self.seed ^ 0x7a1d, p, 11.0))
            / 1.5;
        smoothstep(0.38, 0.62, n)
    }

    /// Cuánto está `p` en el borde de una isleta o de un monte, en `[0, 1]`: 1
    /// a mitad de camino hacia afuera de uno, 0 en terreno abierto o en un
    /// núcleo.
    pub fn fringe(&self, p: Vec2) -> f32 {
        Self::fringe_of(self.grove(p), self.monte(p))
    }

    /// [`Self::fringe`] a partir de las máscaras de isleta y monte ya
    /// calculadas.
    fn fringe_of(grove: f32, monte: f32) -> f32 {
        (4.0 * grove * (1.0 - grove)).max(4.0 * monte * (1.0 - monte))
    }

    /// La proporción de cada tipo de arbusto (piquillín, jarilla, fachinal) en
    /// `p`, que suma 1: el piquillín forma el sotobosque del monte, la jarilla
    /// ocupa el terreno abierto y seco, y los rebrotes de chañar del fachinal
    /// bordean las isletas y el monte.
    pub fn shrub_shares(&self, p: Vec2) -> [f32; 3] {
        self.shares_of(p, self.monte(p), self.fringe(p))
    }

    /// [`Self::shrub_shares`] a partir de las máscaras de monte y de borde ya
    /// calculadas.
    fn shares_of(&self, p: Vec2, monte: f32, fringe: f32) -> [f32; 3] {
        let dry = self.dryness(p);
        let weights = [
            0.3 + 0.55 * monte,
            (1.0 - monte) * (0.15 + 0.7 * dry) * (1.0 - 0.6 * fringe),
            0.1 + 0.9 * fringe * (1.0 - 0.5 * monte),
        ];
        let total: f32 = weights.iter().sum();
        weights.map(|w| w / total)
    }

    /// Los arbustos del chunk `coord`, dados sus `trees`: más densos en el
    /// monte y a lo largo de los bordes de las isletas y del monte, escasos en
    /// terreno abierto. Se mantienen lejos de todo aquello de lo que se
    /// mantienen lejos los árboles (un poco más cerca), de los troncos de los
    /// árboles (también los de los chunks vecinos) y separados entre sí dentro
    /// del chunk.
    fn chunk_shrubs(&self, coord: ChunkCoord, rail: &[(Vec2, Vec2)], trees: &[Tree]) -> Vec<Shrub> {
        let size = self.config.chunk_size;
        let min = coord.min(size);
        let mut rng = Rng::new(coord.seed(self.seed), 2);
        let candidates = self.scaled(Self::SHRUB_CANDIDATES);
        let mut shrubs: Vec<Shrub> = Vec::new();
        // Los árboles de los chunks de alrededor, que se buscan cuando un
        // arbusto está cerca del borde que comparten.
        let mut neighbours: Vec<((i32, i32), Vec<Tree>)> = Vec::new();
        const BORDER: f32 = FloraSite::SHRUB_BORDER;
        for _ in 0..candidates {
            let at = min + Vec2::new(rng.unit() * size, rng.unit() * size);
            let (pick, kind_roll, height_roll, width_roll, yaw) =
                (rng.unit(), rng.unit(), rng.unit(), rng.unit(), rng.unit());
            let variant = rng.next();
            if shrubs.len() >= Self::SHRUB_BUDGET {
                break;
            }
            let monte = self.monte(at);
            let fringe = Self::fringe_of(self.grove(at), monte);
            let chance = (0.05 + 0.64 * monte + 0.5 * fringe) * (0.3 + 0.7 * self.thicket(at));
            if pick >= chance.min(0.9) || !self.shrub_room(at, rail) {
                continue;
            }
            let [piquillin, jarilla, _] = self.shares_of(at, monte, fringe);
            let kind = if kind_roll < piquillin {
                ShrubKind::Piquillin
            } else if kind_roll < piquillin + jarilla {
                ShrubKind::Jarilla
            } else {
                ShrubKind::Fachinal
            };
            let (lo, hi) = kind.height_range();
            let shrub = Shrub {
                at,
                kind,
                height: lo + (hi - lo) * height_roll,
                width: 0.85 + 0.35 * width_roll,
                yaw: yaw * std::f32::consts::TAU,
                variant,
            };
            // Primero lo barato: los otros arbustos, que son pocos cerca del
            // candidato.
            if shrubs
                .iter()
                .any(|o| o.at.distance(at) < Self::SHRUB_SPACING)
            {
                continue;
            }
            let clear_of = |t: &Tree| {
                let reach = t.trunk_radius() + shrub.trunk_radius() + Self::SHRUB_TRUNK_GAP;
                let d = t.at - at;
                d.x * d.x + d.y * d.y >= reach * reach
            };
            let sides = |v: f32, lo: f32| {
                if v - lo < BORDER {
                    -1
                } else if lo + size - v < BORDER {
                    1
                } else {
                    0
                }
            };
            let (sx, sy) = (sides(at.x, min.x), sides(at.y, min.y));
            let mut clear = trees.iter().all(clear_of);
            for (dx, dy) in [(sx, 0), (0, sy), (sx, sy)] {
                if !clear || (dx, dy) == (0, 0) {
                    continue;
                }
                let key = (coord.x + dx, coord.y + dy);
                let at_index = match neighbours.iter().position(|(k, _)| *k == key) {
                    Some(i) => i,
                    None => {
                        // Antes de que los árboles del vecino cedan ante los de
                        // sus propios vecinos: un superconjunto de sus árboles,
                        // así que un arbusto se mantiene lejos de todos ellos,
                        // y mucho más barato.
                        let other = ChunkCoord::new(key.0, key.1);
                        let trees = if self.config.has_chunk(other) {
                            self.scattered_trees(other, &self.rail_near(other))
                        } else {
                            Vec::new()
                        };
                        neighbours.push((key, trees));
                        neighbours.len() - 1
                    }
                };
                clear = neighbours[at_index].1.iter().all(clear_of);
            }
            if clear {
                shrubs.push(shrub);
            }
        }
        shrubs
    }

    /// Si un arbusto puede estar en `p`: dentro del mundo, lejos de la franja
    /// de todo pueblo, de la vía, de los caminos, de las líneas de alambrado,
    /// de las lagunas y de los asentamientos.
    fn shrub_room(&self, p: Vec2, rail: &[(Vec2, Vec2)]) -> bool {
        let bounds = self.config.bounds();
        let m = Self::SHRUB_EDGE_MARGIN;
        let inside = p.x - bounds.min.x >= m
            && bounds.max.x - p.x >= m
            && p.y - bounds.min.y >= m
            && bounds.max.y - p.y >= m;
        inside
            && self
                .flat_areas
                .iter()
                .all(|a| outside(a, p) >= a.falloff + Self::SHRUB_TOWN_MARGIN)
            && self
                .lagoons
                .iter()
                .all(|l| l.offshore(p) >= Self::SHRUB_LAGOON_MARGIN)
            && rail_distance(rail, p) >= Self::SHRUB_RAIL_MARGIN
            && self.off_road(p, Self::SHRUB_ROAD_MARGIN)
            && self.off_fences(p)
            && self.off_settlements(p, Self::SHRUB_SETTLEMENT_MARGIN)
    }

    /// Si un árbol puede estar en `p`: dentro del mundo, lejos de la franja de
    /// todo pueblo, de la vía, de los caminos, de las líneas de alambrado y de
    /// los asentamientos.
    fn tree_room(&self, p: Vec2, rail: &[(Vec2, Vec2)]) -> bool {
        let bounds = self.config.bounds();
        let m = Self::TREE_EDGE_MARGIN;
        let inside = p.x - bounds.min.x >= m
            && bounds.max.x - p.x >= m
            && p.y - bounds.min.y >= m
            && bounds.max.y - p.y >= m;
        inside
            && self
                .flat_areas
                .iter()
                .all(|a| outside(a, p) >= a.falloff + Self::TREE_TOWN_MARGIN)
            && self.tree_off_lagoons(p)
            && rail_distance(rail, p) >= Self::TREE_RAIL_MARGIN
            && self.off_road(p, Self::TREE_ROAD_MARGIN)
            && self.off_fences(p)
            && self.off_settlements(p, Self::TREE_SETTLEMENT_MARGIN)
    }

    /// Si `p` está al menos a [`Self::TREE_FENCE_MARGIN`] de toda línea de
    /// alambrado relevada.
    fn off_fences(&self, p: Vec2) -> bool {
        let clear = |lines: &[f32], v: f32| {
            let i = lines.partition_point(|&l| l < v);
            [i.checked_sub(1), Some(i)]
                .into_iter()
                .flatten()
                .filter_map(|i| lines.get(i))
                .all(|l| (l - v).abs() >= Self::TREE_FENCE_MARGIN)
        };
        clear(&self.fences.0, p.x) && clear(&self.fences.1, p.y)
    }

    /// Si `p` está al menos a `margin` metros de todo camino.
    fn off_road(&self, p: Vec2, margin: f32) -> bool {
        self.roads
            .as_ref()
            .is_none_or(|r| r.near(p, margin).is_none_or(|d| d >= margin))
    }

    /// `per_reference` candidatos escalados al área de chunk de este mundo.
    fn scaled(&self, per_reference: f32) -> usize {
        let ratio = self.config.chunk_size / Self::REFERENCE_CHUNK;
        (per_reference * ratio * ratio).round() as usize
    }
}

impl Age {
    /// Todas las edades, de la más joven a la más vieja.
    pub const ALL: [Age; 4] = [Self::Sapling, Self::Young, Self::Mature, Self::Old];

    /// Cuánto más grueso de lo que indica su altura crece el tronco de un
    /// caldén: el tallo de un renuevo y un tronco adulto son alrededor de una
    /// décima más robustos, y el de un gigante viejo, cuatro décimas.
    pub fn girth(self) -> f32 {
        match self {
            Self::Sapling => 1.3,
            Self::Young | Self::Mature => 1.1,
            Self::Old => 1.4,
        }
    }

    /// Altura mínima y máxima de una especie `species` de esta edad, en metros.
    pub fn height_range(self, species: Species) -> (f32, f32) {
        // Renuevo, joven, adulto y viejo, en ese orden.
        let [sapling, young, mature, old] = match species {
            Species::Calden => [(2.0, 3.5), (3.5, 6.0), (6.0, 9.0), (10.0, 12.0)],
            Species::Chanar => [(1.5, 2.8), (2.8, 3.8), (3.8, 5.0), (5.0, 6.5)],
            // Bajo y ancho: 4-8 m.
            Species::Algarrobo => [(1.8, 3.0), (3.0, 4.5), (4.5, 6.5), (6.5, 8.0)],
            // Un árbol bajo y arbustivo de 4-9 m.
            Species::Tala => [(1.5, 2.5), (2.5, 4.0), (4.0, 6.5), (6.5, 8.5)],
            // Hasta 5 m.
            Species::Espinillo => [(1.2, 2.0), (2.0, 3.0), (3.0, 4.2), (4.2, 5.0)],
            // 12-18 m cuando es viejo; como mínimo se planta como árbol joven.
            Species::Ombu => [(5.0, 6.0), (6.0, 10.0), (10.0, 13.0), (13.0, 16.0)],
            Species::Paraiso => [(2.0, 4.0), (4.0, 7.0), (7.0, 10.0), (10.0, 12.0)],
            Species::Olmo => [(3.0, 5.0), (5.0, 9.0), (9.0, 14.0), (14.0, 18.0)],
            Species::Eucalipto => [(4.0, 8.0), (8.0, 14.0), (14.0, 19.0), (19.0, 22.0)],
            Species::Aromo => [(1.5, 2.5), (2.5, 4.0), (4.0, 6.0), (6.0, 7.0)],
        };
        match self {
            Self::Sapling => sapling,
            Self::Young => young,
            Self::Mature => mature,
            Self::Old => old,
        }
    }
}

impl Species {
    /// El árbol más bajo y el más alto de cualquier edad, en metros.
    pub fn height_range(self) -> (f32, f32) {
        let (lo, _) = Age::Sapling.height_range(self);
        let (_, hi) = Age::Old.height_range(self);
        (lo, hi)
    }
}

impl ShrubKind {
    /// Todos los tipos de arbusto.
    pub const ALL: [ShrubKind; 3] = [Self::Piquillin, Self::Jarilla, Self::Fachinal];

    /// El arbusto más bajo y el más alto de este tipo, en metros: el piquillín
    /// crece 0,8-2 m, hasta 3 m; la jarilla 1-3 m; los rebrotes del fachinal,
    /// un poco más altos que los renuevos del chañar.
    pub fn height_range(self) -> (f32, f32) {
        match self {
            Self::Piquillin => (1.0, 3.0),
            Self::Jarilla => (1.0, 2.5),
            Self::Fachinal => (1.8, 3.2),
        }
    }
}

impl Shrub {
    /// El núcleo denso de tallos a nivel del suelo, en metros. El piquillín
    /// espinoso y los rebrotes de un matorral son más robustos que los tallos
    /// delgados de la jarilla.
    pub fn trunk_radius(&self) -> f32 {
        self.height
            * match self.kind {
                ShrubKind::Piquillin => 0.1,
                ShrubKind::Jarilla => 0.07,
                ShrubKind::Fachinal => 0.09,
            }
    }

    /// Radio del arbusto visto desde arriba, en metros.
    pub fn crown_radius(&self) -> f32 {
        let base = match self.kind {
            ShrubKind::Piquillin => 0.5,
            ShrubKind::Jarilla => 0.35,
            ShrubKind::Fachinal => 0.4,
        };
        base * self.width * self.height
    }

    /// La huella del núcleo de tallos, un cuadrado tan ancho como el núcleo. El
    /// mundo no la planta: los arbustos nunca bloquean.
    pub fn obstacle(&self) -> Aabb {
        let side = 2.0 * self.trunk_radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
    }
}

impl TuftKind {
    /// La mata más baja y la más alta, en metros.
    pub fn size_range(self) -> (f32, f32) {
        match self {
            Self::Paja => (0.5, 0.9),
            Self::Flechilla => (0.25, 0.5),
        }
    }
}

impl Tree {
    /// Radio del tronco a nivel del suelo, en metros. El de un chañar es el
    /// núcleo denso de tallos de su matorral, que ningún jinete atraviesa.
    pub fn trunk_radius(&self) -> f32 {
        match self.species {
            Species::Calden => 0.035 * self.age.girth() * self.height,
            Species::Chanar => 0.09 * self.height,
            Species::Algarrobo => 0.07 * self.height,
            Species::Tala => 0.07 * self.height,
            Species::Espinillo => 0.09 * self.height,
            // Un tronco enorme con contrafuertes.
            Species::Ombu => 0.1 * self.height,
            Species::Paraiso => 0.055 * self.height,
            Species::Olmo => 0.05 * self.height,
            Species::Eucalipto => 0.045 * self.height,
            Species::Aromo => 0.055 * self.height,
        }
    }

    /// Radio de la copa vista desde arriba, en metros.
    pub fn crown_radius(&self) -> f32 {
        let base = match self.species {
            Species::Calden => 0.55,
            Species::Chanar => 0.35,
            Species::Algarrobo => 0.6,
            Species::Tala => 0.4,
            Species::Espinillo => 0.45,
            Species::Ombu => 0.5,
            Species::Paraiso => 0.5,
            Species::Olmo => 0.4,
            Species::Eucalipto => 0.22,
            Species::Aromo => 0.5,
        };
        base * self.width * self.height
    }

    /// La huella del tronco, un cuadrado tan ancho como el tronco: un obstáculo
    /// estático.
    pub fn obstacle(&self) -> Aabb {
        let side = 2.0 * self.trunk_radius();
        Aabb::from_center_size(self.at, Vec2::new(side, side))
    }

    /// El mayor [`Self::spacing`] de cualquier árbol, en metros.
    const MAX_SPACING: f32 = 8.0;

    /// Distancia mínima hasta el tronco de otro árbol, en metros: un árbol más
    /// grande ocupa más lugar, un renuevo puede estar a la sombra de uno viejo.
    fn spacing(&self) -> f32 {
        match self.species {
            Species::Calden | Species::Algarrobo => 1.6 + 0.3 * self.height,
            Species::Chanar | Species::Tala => 1.4 + 0.25 * self.height,
            Species::Espinillo => 1.3 + 0.25 * self.height,
            Species::Ombu => 3.0 + 0.3 * self.height,
            Species::Paraiso => 1.8 + 0.3 * self.height,
            Species::Olmo => 2.0 + 0.3 * self.height,
            Species::Eucalipto => 2.0 + 0.25 * self.height,
            Species::Aromo => 1.4 + 0.3 * self.height,
        }
    }
}

/// Qué tan seco está el pasto de `terrain` en `p`, en `[0, 1]`: lo mismo que
/// [`FloraSite::dryness`] para un sitio sobre ese terreno. El render tiñe el
/// suelo con esto.
pub fn grass_dryness(terrain: &Terrain, p: Vec2) -> f32 {
    dryness_of(terrain.seed ^ FLORA_SEED, p)
}

fn dryness_of(seed: u32, p: Vec2) -> f32 {
    let s = seed ^ 0xd1e5;
    let n = (value_noise(s, p, 420.0) + 0.35 * value_noise(s ^ 0x77, p, 130.0)) / 1.35;
    smoothstep(0.3, 0.7, n)
}

/// Se mezcla con la semilla del terreno para todos los flujos de flora.
const FLORA_SEED: u32 = 0xf10a_2a11;

/// Distancia desde `p` hasta el núcleo plano de `area` (0 adentro).
fn outside(area: &FlatArea, p: Vec2) -> f32 {
    let dx = ((p.x - area.center.x).abs() - area.half_extents.x).max(0.0);
    let dy = ((p.y - area.center.y).abs() - area.half_extents.y).max(0.0);
    dx.hypot(dy)
}

/// Distancia desde `p` hasta el más cercano de los segmentos de `rail`.
fn rail_distance(rail: &[(Vec2, Vec2)], p: Vec2) -> f32 {
    rail.iter()
        .map(|&(a, b)| segment_distance(p, a, b))
        .fold(f32::INFINITY, f32::min)
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Ruido de valores suave en `[0, 1]` con celdas de grilla de `cell` metros de
/// ancho.
fn value_noise(seed: u32, p: Vec2, cell: f32) -> f32 {
    let (x, y) = (p.x / cell, p.y / cell);
    let (i, j) = (x.floor(), y.floor());
    let (fx, fy) = (fade(x - i), fade(y - j));
    let (i, j) = (i as i32, j as i32);
    let at =
        |i: i32, j: i32| Rng::new(seed ^ (i as u32).wrapping_mul(0x85eb_ca6b), j as u32).unit();
    let bottom = at(i, j) + (at(i + 1, j) - at(i, j)) * fx;
    let top = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * fx;
    bottom + (top - bottom) * fy
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Distancia desde `p` hasta el segmento `a`–`b`.
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let span = d.dot(d);
    if span <= 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(d) / span).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Circle, Heading, Rider, RiderInput, RiderParams, TownConfig, World, place_towns};

    /// Dos pueblos con ferrocarril a 3,6 km de distancia y la línea entre
    /// ellos.
    fn site() -> (World, RailLine, FloraSite) {
        let configs = [
            TownConfig::castex(),
            TownConfig {
                name: "Lejos".into(),
                origin: Vec2::new(3000.0, -2000.0),
                ..TownConfig::castex()
            },
        ];
        let config = WorldConfig::default();
        let (towns, errors) = place_towns(&config, &configs);
        assert!(errors.is_empty(), "{errors:?}");
        let world = World::with_towns(config, Terrain::default(), &towns);
        let (line, errors) = RailLine::build(None, &towns, world.bounds);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.links.len(), 1);
        // Los montes tal como son en sí mismos, sin escalar según la distancia
        // a los pueblos (ver `redistributed`).
        let mut flora = FloraSite::new(config, &world.terrain, &line);
        flora.redistribute = false;
        (world, line, flora)
    }

    /// La flora de [`site`] con el monte escalado según la distancia a los
    /// pueblos.
    fn redistributed() -> (World, RailLine, FloraSite) {
        let (world, line, mut flora) = site();
        flora.redistribute = true;
        (world, line, flora)
    }

    fn rail_distance(line: &RailLine, p: Vec2) -> f32 {
        line.segments()
            .map(|s| segment_distance(p, s.start, s.end))
            .fold(f32::INFINITY, f32::min)
    }

    /// Un bloque de chunks entre los dos pueblos, a ambos lados de la línea.
    fn between() -> Vec<ChunkCoord> {
        (0..14)
            .flat_map(|x| (-11..1).map(move |y| ChunkCoord::new(x, y)))
            .collect()
    }

    #[test]
    fn scatter_is_deterministic_per_chunk() {
        let (_, _, flora) = site();
        let a = flora.chunk(ChunkCoord::new(5, -4));
        assert!(!a.tufts.is_empty());
        assert_eq!(a, flora.chunk(ChunkCoord::new(5, -4)));
        assert_ne!(a, flora.chunk(ChunkCoord::new(6, -4)));
        let size = WorldConfig::default().chunk_size;
        let bounds = ChunkCoord::new(5, -4).bounds(size);
        assert!(a.tufts.iter().all(|t| bounds.contains_point(t.at)));
        assert!(a.trees.iter().all(|t| bounds.contains_point(t.at)));
        assert!(a.tufts.len() <= FloraSite::TUFT_BUDGET);
        assert!(a.trees.len() <= FloraSite::TREE_BUDGET);
    }

    #[test]
    fn trees_keep_their_spacing_across_chunk_borders() {
        let (_, _, flora) = site();
        let trees: Vec<(ChunkCoord, Tree)> = between()
            .into_iter()
            .flat_map(|c| flora.trees(c).into_iter().map(move |t| (c, t)))
            .collect();
        assert!(trees.len() > 200, "{}", trees.len());
        let mut near_borders = 0;
        for (i, (ca, a)) in trees.iter().enumerate() {
            for (cb, b) in &trees[i + 1..] {
                if ca == cb {
                    continue;
                }
                let gap = a.at.distance(b.at);
                if gap < 10.0 {
                    near_borders += 1;
                }
                let least = a.spacing().max(b.spacing());
                assert!(
                    gap >= least,
                    "{ca:?} {:?} and {cb:?} {:?}: {gap}",
                    a.at,
                    b.at
                );
            }
        }
        assert!(near_borders > 0, "no pair close across a border to check");
        // Sigue siendo determinista, chunk por chunk.
        for c in between().into_iter().step_by(7) {
            assert_eq!(flora.trees(c), flora.trees(c));
            assert_eq!(flora.trees(c), flora.chunk(c).trees);
        }
    }

    /// Todos los árboles de un bloque amplio entre los pueblos y de un bloque
    /// rico en isletas al sudoeste.
    fn sample(flora: &FloraSite) -> Vec<Tree> {
        between()
            .into_iter()
            .chain((-19..1).flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y))))
            .flat_map(|c| flora.trees(c))
            .collect()
    }

    #[test]
    fn trees_have_their_species_sizes() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let of = |s: Species| trees.iter().filter(|t| t.species == s).collect::<Vec<_>>();
        let (calden, chanar) = (of(Species::Calden), of(Species::Chanar));
        assert!(
            calden.len() > 20 && chanar.len() > 20,
            "{} {}",
            calden.len(),
            chanar.len()
        );
        for t in &trees {
            // La altura de cada árbol corresponde a su clase de edad.
            let (lo, hi) = t.age.height_range(t.species);
            assert!((lo..=hi).contains(&t.height), "{t:?}");
            let (lo, hi) = t.species.height_range();
            assert!((lo..=hi).contains(&t.height), "{t:?}");
        }
        // Los árboles adultos conservan los tamaños documentados.
        let mature = |ts: &[&Tree], lo: f32, hi: f32| {
            ts.iter()
                .filter(|t| t.age == Age::Mature)
                .all(|t| (lo..=hi).contains(&t.height))
        };
        assert!(mature(&calden, 6.0, 9.0));
        assert!(mature(&chanar, 3.8, 5.0));
        // Un chañar se dibuja como un matorral denso: su obstáculo es el núcleo
        // de tallos del matorral, más ancho que un solo tronco.
        assert!(
            chanar
                .iter()
                .all(|t| t.trunk_radius() >= 0.08 * t.height && t.trunk_radius() < t.crown_radius())
        );
        assert!(
            trees
                .iter()
                // (El tronco de un eucalipto gigante, el más robusto aquí, mide
                // menos de un metro. El ombú, cuyo tronco viejo mide 1,6 m, queda
                // fuera: este sitio no tiene asentamientos, así que aquí no crece
                // ninguno.)
                .filter(|t| t.species != Species::Ombu)
                .all(|t| t.trunk_radius() > 0.05 && t.trunk_radius() < 1.0)
        );
    }

    #[test]
    fn trees_come_in_age_classes_and_their_collision_grows_with_them() {
        let (_, _, flora) = site();
        // Fuera del monte, que tiene un bosque más joven (ver
        // `the_monte_is_a_young_stand_with_a_chanar_understory`).
        let trees: Vec<Tree> = sample(&flora)
            .into_iter()
            .filter(|t| flora.monte(t.at) < 0.02)
            .collect();
        for species in [Species::Calden, Species::Chanar] {
            let own: Vec<&Tree> = trees.iter().filter(|t| t.species == species).collect();
            let share =
                |age: Age| own.iter().filter(|t| t.age == age).count() as f32 / own.len() as f32;
            println!(
                "{species:?}: {:.3} sapling {:.3} young {:.3} mature {:.3} old",
                share(Age::Sapling),
                share(Age::Young),
                share(Age::Mature),
                share(Age::Old)
            );
            assert!((0.04..=0.09).contains(&share(Age::Sapling)), "{species:?}");
            assert!((0.10..=0.20).contains(&share(Age::Young)), "{species:?}");
            assert!((0.70..=0.85).contains(&share(Age::Mature)), "{species:?}");
            // Los gigantes viejos son raros, pero hay algunos.
            assert!((0.025..=0.04).contains(&share(Age::Old)), "{species:?}");
            // Una clase mayor es más alta, tiene el tronco más ancho y ocupa
            // más lugar, en promedio y sin superposición de los extremos.
            let mean = |age: Age, f: fn(&Tree) -> f32| {
                let v: Vec<f32> = own.iter().filter(|t| t.age == age).map(|t| f(t)).collect();
                v.iter().sum::<f32>() / v.len() as f32
            };
            for pair in Age::ALL.windows(2) {
                for f in [
                    (|t| t.height) as fn(&Tree) -> f32,
                    |t| t.trunk_radius(),
                    |t| t.spacing(),
                ] {
                    assert!(mean(pair[0], f) < mean(pair[1], f), "{species:?} {pair:?}");
                }
            }
        }
        // Un caldén gigante mide 10-12 m, con un tronco más grueso que el de
        // cualquier árbol adulto y una copa más ancha que la de cualquier árbol
        // adulto.
        let calden = |age: Age| {
            trees
                .iter()
                .filter(move |t| t.species == Species::Calden && t.age == age)
        };
        let max = |age: Age, f: fn(&Tree) -> f32| calden(age).map(f).fold(0.0, f32::max);
        let min = |age: Age, f: fn(&Tree) -> f32| calden(age).map(f).fold(f32::MAX, f32::min);
        assert!(min(Age::Old, |t| t.height) >= 10.0);
        assert!(max(Age::Old, |t| t.height) <= 12.0);
        assert!(
            min(Age::Old, |t| t.trunk_radius()) > 1.15 * max(Age::Mature, |t| t.trunk_radius())
        );
        assert!(min(Age::Old, |t| t.crown_radius()) > max(Age::Mature, |t| t.crown_radius()));
        assert!(max(Age::Sapling, |t| t.height) <= 3.5);
        // El ancho de cada árbol es propio y moderado, y los gigantes son
        // anchos.
        for t in &trees {
            assert!((0.8..=1.4).contains(&t.width), "{t:?}");
        }
        let widths: std::collections::HashSet<u32> =
            trees.iter().map(|t| t.width.to_bits()).collect();
        assert!(widths.len() > trees.len() / 2, "{}", widths.len());
        // Cada árbol sigue teniendo su propio giro y su propia semilla.
        let looks: std::collections::HashSet<u32> = trees.iter().map(|t| t.variant).collect();
        assert!(looks.len() > trees.len() * 9 / 10);
    }

    /// Distancia desde cada árbol cuya máscara de isleta supera `grove` hasta
    /// su vecino más cercano.
    fn grove_neighbour_gaps(flora: &FloraSite, trees: &[Tree], grove: f32) -> Vec<f32> {
        trees
            .iter()
            .enumerate()
            .filter(|(_, a)| flora.grove(a.at) > grove)
            .map(|(i, a)| {
                trees
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, b)| b.at.distance(a.at))
                    .fold(f32::INFINITY, f32::min)
            })
            .collect()
    }

    #[test]
    fn groves_are_clumps_with_gaps_not_an_even_stand() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let gaps = grove_neighbour_gaps(&flora, &trees, 0.6);
        let n = gaps.len() as f32;
        assert!(n > 1000.0, "{n}");
        let mean = gaps.iter().sum::<f32>() / n;
        let sd = (gaps.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / n).sqrt();
        let close = gaps.iter().filter(|&&d| d < 6.0).count() as f32 / n;
        println!(
            "grove gaps: mean {mean:.1} m, cv {:.2}, {close:.2} close",
            sd / mean
        );
        // Conteos en cuadrantes de 24 m sobre las isletas: una distribución
        // aleatoria tiene una varianza igual a su media (un índice de 1), y una
        // pareja, mucho menor.
        let mut counts = Vec::new();
        for qx in -158..8 {
            for qy in 25..158 {
                let (x, y) = (qx as f32 * 24.0, qy as f32 * 24.0);
                if flora.grove(Vec2::new(x + 12.0, y + 12.0)) > 0.6 {
                    let n = trees
                        .iter()
                        .filter(|t| {
                            (x..x + 24.0).contains(&t.at.x) && (y..y + 24.0).contains(&t.at.y)
                        })
                        .count();
                    counts.push(n as f32);
                }
            }
        }
        let q = counts.len() as f32;
        let qmean = counts.iter().sum::<f32>() / q;
        let qvar = counts.iter().map(|c| (c - qmean).powi(2)).sum::<f32>() / q;
        println!(
            "quadrats: {q} of mean {qmean:.2}, dispersion {:.2}",
            qvar / qmean
        );
        assert!(q > 100.0, "{q}");
        // Las isletas parejas daban 0,97; los grupos con huecos quedan bien por
        // encima.
        assert!(qvar / qmean > 1.6, "dispersion {}", qvar / qmean);
        // La dispersión de los vecinos más cercanos (cv) de un monte parejo
        // queda por debajo del 0,52 de uno aleatorio (las isletas parejas daban
        // 0,43).
        assert!(sd / mean > 0.5, "cv {}", sd / mean);
        // Muchos árboles están cerca entre sí.
        assert!(close > 0.3, "{close}");
    }

    #[test]
    fn chanares_are_a_bigger_share_where_the_groves_thin_out() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let edge: Vec<&Tree> = trees
            .iter()
            .filter(|t| (0.15..0.85).contains(&flora.grove(t.at)))
            .collect();
        let core: Vec<&Tree> = trees.iter().filter(|t| flora.grove(t.at) >= 0.85).collect();
        // Entre los dos árboles principales: las otras especies son unas pocas
        // por cada cien.
        let share = |ts: &[&Tree]| {
            let main: Vec<&&Tree> = ts
                .iter()
                .filter(|t| matches!(t.species, Species::Calden | Species::Chanar))
                .collect();
            main.iter().filter(|t| t.species == Species::Chanar).count() as f32 / main.len() as f32
        };
        println!(
            "chañar share: edge {:.2} of {}, core {:.2} of {}",
            share(&edge),
            edge.len(),
            share(&core),
            core.len()
        );
        assert!(edge.len() > 500 && core.len() > 500);
        // Antes: alrededor de 0,48 en los bordes. Los chañares llenan los
        // bordes y los caldenes conservan los núcleos.
        assert!(share(&edge) > 0.6, "{}", share(&edge));
        assert!(share(&edge) > share(&core) + 0.2);
    }

    #[test]
    fn nothing_grows_on_town_ground_the_rail_corridor_or_off_the_world() {
        let (world, line, flora) = site();
        let bounds = world.bounds;
        let mut chunks = between();
        // Alrededor de los dos pueblos, y a lo largo de los bordes oeste y sur
        // del mundo.
        chunks.extend((-4..4).flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y))));
        chunks.extend((11..19).flat_map(|x| (-14..-6).map(move |y| ChunkCoord::new(x, y))));
        chunks.extend((-20..-17).flat_map(|x| (-20..-10).map(move |y| ChunkCoord::new(x, y))));
        let (mut trees, mut tufts) = (0, 0);
        for c in chunks {
            let f = flora.chunk(c);
            for t in &f.trees {
                for area in &world.terrain.flat_areas {
                    assert_eq!(area.weight(t.at), 0.0, "tree on town ground {t:?}");
                }
                assert!(
                    rail_distance(&line, t.at) >= FloraSite::TREE_RAIL_MARGIN,
                    "{t:?}"
                );
                assert!(
                    t.at.x - bounds.min.x >= FloraSite::TREE_EDGE_MARGIN,
                    "{t:?}"
                );
                assert!(
                    t.at.y - bounds.min.y >= FloraSite::TREE_EDGE_MARGIN,
                    "{t:?}"
                );
                trees += 1;
            }
            for t in &f.tufts {
                for area in &world.terrain.flat_areas {
                    assert_eq!(area.weight(t.at), 0.0, "tuft on town ground {t:?}");
                }
                assert!(
                    rail_distance(&line, t.at) >= FloraSite::TUFT_RAIL_MARGIN,
                    "{t:?}"
                );
                assert!(bounds.contain(t.at, 0.5).push_x.is_none(), "{t:?}");
                assert!(bounds.contain(t.at, 0.5).push_y.is_none(), "{t:?}");
                tufts += 1;
            }
        }
        assert!(trees > 50 && tufts > 5000, "{trees} {tufts}");
        // Los chunks completamente fuera del mundo no tienen nada.
        assert_eq!(flora.chunk(ChunkCoord::new(-21, 0)), ChunkFlora::default());
        assert_eq!(flora.chunk(ChunkCoord::new(0, 20)), ChunkFlora::default());
    }

    /// Correlación de Pearson de dos series de igual longitud.
    fn correlation(a: &[f32], b: &[f32]) -> f32 {
        let n = a.len() as f32;
        let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
        let cov: f32 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
        let va: f32 = a.iter().map(|x| (x - ma).powi(2)).sum();
        let vb: f32 = b.iter().map(|y| (y - mb).powi(2)).sum();
        cov / (va * vb).sqrt()
    }

    #[test]
    fn clumping_is_continuous_across_chunk_borders() {
        let (_, _, flora) = site();
        let trees = sample(&flora);
        let size = WorldConfig::default().chunk_size;
        // Árboles en la franja de 12 m de ancho y 20 m de largo desde `x` a lo
        // largo de `y`.
        let strip = |x: f32, y: f32| {
            trees
                .iter()
                .filter(|t| (x..x + 12.0).contains(&t.at.x) && (y..y + 20.0).contains(&t.at.y))
                .count() as f32
        };
        // Por cada borde vertical de chunk del bloque rico en isletas, se
        // emparejan las franjas a cada lado de él y, como control, dos franjas
        // vecinas bien dentro de un mismo chunk.
        let (mut across, mut across_to, mut within, mut within_to) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for k in -18..0 {
            let seam = k as f32 * size;
            for j in 15..95 {
                let y = j as f32 * 20.0;
                across.push(strip(seam - 12.0, y));
                across_to.push(strip(seam, y));
                within.push(strip(seam + 38.0, y));
                within_to.push(strip(seam + 50.0, y));
            }
        }
        // Los propios campos continúan a través de un borde: no hay saltos
        // entre dos puntos a una décima de metro a cada lado de uno.
        for k in -18..0 {
            for j in 0..200 {
                let p = Vec2::new(k as f32 * size, -1900.0 + j as f32 * 19.3);
                let q = p - Vec2::new(0.1, 0.0);
                for (a, b) in [
                    (flora.knot(p), flora.knot(q)),
                    (flora.clump(p), flora.clump(q)),
                    (flora.grove(p), flora.grove(q)),
                    (flora.monte(p), flora.monte(q)),
                ] {
                    assert!((a - b).abs() < 0.1, "{a} {b} at {p:?}");
                }
            }
        }
        let cross = correlation(&across, &across_to);
        let inside = correlation(&within, &within_to);
        println!("strip correlation: across a border {cross:.2}, within a chunk {inside:.2}");
        assert!(inside > 0.3, "no clumping to speak of: {inside}");
        // Un borde no corta los grupos: los dos lados coinciden más o menos
        // tanto como dos franjas de un mismo chunk.
        assert!(cross > 0.85 * inside, "across {cross}, within {inside}");
    }

    /// Todos los chunks del mundo por defecto.
    fn world_chunks() -> Vec<ChunkCoord> {
        (-20..20)
            .flat_map(|x| (-20..20).map(move |y| ChunkCoord::new(x, y)))
            .collect()
    }

    /// El mínimo, la media y el máximo de `field` sobre una grilla de 5 por 5
    /// en el chunk `c`.
    fn chunk_range(c: ChunkCoord, field: impl Fn(Vec2) -> f32) -> (f32, f32, f32) {
        let b = c.bounds(WorldConfig::default().chunk_size);
        let step = (b.max - b.min) * 0.25;
        let all: Vec<f32> = (0..25)
            .map(|k| field(b.min + Vec2::new((k % 5) as f32 * step.x, (k / 5) as f32 * step.y)))
            .collect();
        (
            all.iter().copied().fold(f32::MAX, f32::min),
            all.iter().sum::<f32>() / 25.0,
            all.iter().copied().fold(f32::MIN, f32::max),
        )
    }

    #[test]
    fn monte_patches_are_world_scale_smooth_and_a_share_of_the_pampa() {
        let (_, _, flora) = site();
        for k in 0..500 {
            let p = Vec2::new(k as f32 * 7.3 - 1800.0, k as f32 * -3.1 + 400.0);
            let m = flora.monte(p);
            assert!((0.0..=1.0).contains(&m));
            // También continuo a través de cualquier borde de chunk.
            assert!((m - flora.monte(p + Vec2::new(1.0, 0.0))).abs() < 0.05);
            assert!((m - flora.monte(p + Vec2::new(0.0, 1.0))).abs() < 0.05);
        }
        // Recorrer las filas del mundo cada 100 m, muestreando cada 20 m.
        let (mut inside, mut total) = (0, 0);
        let mut runs: Vec<f32> = Vec::new();
        for row in -19..19 {
            let mut run = 0;
            for col in -100..100 {
                let p = Vec2::new(col as f32 * 20.0, row as f32 * 100.0 + 50.0);
                total += 1;
                if flora.monte(p) > 0.5 {
                    inside += 1;
                    run += 1;
                } else if run > 0 {
                    runs.push(run as f32 * 20.0);
                    run = 0;
                }
            }
            if run > 0 {
                runs.push(run as f32 * 20.0);
            }
        }
        let share = inside as f32 / total as f32;
        let mean = runs.iter().sum::<f32>() / runs.len() as f32;
        let widest = runs.iter().copied().fold(0.0, f32::max);
        println!(
            "monte: {share:.3} of the world, {} crossings of mean {mean:.0} m, widest {widest:.0} m",
            runs.len()
        );
        // Una buena parte de la pampa, pero no la mayor parte.
        assert!((0.21..0.25).contains(&share), "{share}");
        // Manchas de unos cientos de metros, las más grandes bastante más.
        assert!((150.0..600.0).contains(&mean), "{mean}");
        assert!(widest >= 400.0, "{widest}");
    }

    #[test]
    fn the_monte_is_far_denser_than_the_open_pampa() {
        let (_, _, flora) = site();
        let (mut inside, mut groves, mut open) = (Vec::new(), Vec::new(), Vec::new());
        let (mut peak, mut grove_peak) = (0.0_f32, 0.0_f32);
        for c in world_chunks() {
            let (monte, grove) = (
                chunk_range(c, |p| flora.monte(p)),
                chunk_range(c, |p| flora.grove(p)),
            );
            let n = flora.trees(c).len() as f32;
            if monte.1 > 0.8 {
                peak = peak.max(n);
                // Donde una isleta no se suma.
                if grove.2 < 0.3 {
                    inside.push(n);
                }
            } else if monte.2 < 0.02 && grove.1 > 0.6 {
                groves.push(n);
                grove_peak = grove_peak.max(n);
            } else if monte.2 < 0.02 && grove.2 < 0.02 {
                open.push(n);
            }
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        println!(
            "trees per chunk: monte {:.1} of {} chunks (up to {peak}), groves {:.1} of {}, \
             open {:.1} of {}",
            mean(&inside),
            inside.len(),
            mean(&groves),
            groves.len(),
            mean(&open),
            open.len()
        );
        assert!(inside.len() >= 10 && groves.len() >= 10 && open.len() >= 50);
        // Denso, dentro del presupuesto por chunk, y muchas veces los pocos
        // árboles dispersos de la pampa abierta (las isletas, algo más chico,
        // ya llenan el presupuesto). Los conteos son los árboles que quedan
        // después del raleo ([`FloraSite::TREE_KEEP`]), así que los
        // presupuestos son límites de los árboles plantados y no se alcanzan.
        assert!(mean(&inside) >= 40.0, "{}", mean(&inside));
        assert!(peak <= FloraSite::TREE_BUDGET as f32);
        // Las isletas conservan su antiguo presupuesto; solo el monte supera lo
        // que conserva una isleta (el presupuesto raleado).
        assert!(grove_peak <= FloraSite::GROVE_BUDGET as f32, "{grove_peak}");
        let grove_kept = FloraSite::GROVE_BUDGET as f32 * FloraSite::TREE_KEEP;
        assert!(peak > grove_kept, "{peak}");
        assert!(mean(&inside) >= 6.0 * mean(&open));
        // Sin claros: hasta el chunk de monte más liviano es un bosque.
        let lightest = inside.iter().copied().fold(f32::MAX, f32::min);
        assert!(lightest >= 20.0, "{lightest}");
    }

    #[test]
    fn the_monte_is_a_young_stand_with_a_chanar_understory() {
        let (_, _, flora) = site();
        let trees: Vec<Tree> = world_chunks()
            .into_iter()
            .flat_map(|c| flora.trees(c))
            .collect();
        let zone = |lo: f32, hi: f32| -> Vec<&Tree> {
            trees
                .iter()
                .filter(|t| (lo..=hi).contains(&flora.monte(t.at)))
                .collect()
        };
        let (heart, open) = (zone(0.9, 1.0), zone(0.0, 0.01));
        let share = |ts: &[&Tree], f: fn(&Tree) -> bool| {
            ts.iter().filter(|t| f(t)).count() as f32 / ts.len() as f32
        };
        let young = |t: &Tree| t.age <= Age::Young;
        let chanar = |t: &Tree| t.species == Species::Chanar;
        let giant = |t: &Tree| t.age == Age::Old;
        println!(
            "monte {} trees: {:.2} sapling or young, {:.2} chañar, {:.3} giants; open {}: {:.2}, {:.2}",
            heart.len(),
            share(&heart, young),
            share(&heart, chanar),
            share(&heart, giant),
            open.len(),
            share(&open, young),
            share(&open, chanar)
        );
        assert!(heart.len() > 1000 && open.len() > 500);
        // Edades mezcladas: muchos renuevos y árboles jóvenes, pero también
        // adultos y algún gigante viejo.
        assert!(share(&heart, young) > 0.45, "{}", share(&heart, young));
        assert!(share(&heart, young) > share(&open, young) + 0.2);
        assert!(share(&heart, |t| t.age == Age::Sapling) > 0.2);
        assert!(share(&heart, |t| t.age == Age::Mature) > 0.42);
        assert!(share(&heart, giant) > 0.02);
        // Los caldenes forman el dosel y los chañares el sotobosque.
        assert!((0.2..0.5).contains(&share(&heart, chanar)));
    }

    /// Todos los arbustos del mundo por defecto, con su chunk.
    fn world_shrubs(flora: &FloraSite) -> Vec<(ChunkCoord, Shrub)> {
        world_chunks()
            .into_iter()
            .flat_map(|c| flora.shrubs(c).into_iter().map(move |s| (c, s)))
            .collect()
    }

    #[test]
    fn shrubs_are_deterministic_per_chunk_and_stay_in_budget() {
        let (_, _, flora) = site();
        let size = WorldConfig::default().chunk_size;
        let mut most = 0;
        for c in world_chunks().into_iter().step_by(3) {
            let own = flora.shrubs(c);
            assert_eq!(own, flora.shrubs(c));
            assert_eq!(own, flora.chunk(c).shrubs);
            assert!(own.len() <= FloraSite::SHRUB_BUDGET);
            let bounds = c.bounds(size);
            assert!(own.iter().all(|s| bounds.contains_point(s.at)));
            most = most.max(own.len());
        }
        // Los chunks más densos alcanzan el presupuesto; los vecinos difieren.
        assert!(most >= 100, "{most}");
        assert_ne!(
            flora.shrubs(ChunkCoord::new(5, -4)),
            flora.shrubs(ChunkCoord::new(6, -4))
        );
        assert_eq!(flora.shrubs(ChunkCoord::new(-21, 0)), Vec::new());
    }

    #[test]
    fn shrubs_are_densest_in_the_monte_and_on_the_fringes_and_sparse_in_the_open() {
        let (_, _, flora) = site();
        let (mut monte, mut fringe, mut open) = (Vec::new(), Vec::new(), Vec::new());
        for c in world_chunks() {
            let m = chunk_range(c, |p| flora.monte(p));
            let f = chunk_range(c, |p| flora.fringe(p));
            let g = chunk_range(c, |p| flora.grove(p));
            let n = flora.shrubs(c).len() as f32;
            if m.1 > 0.8 {
                monte.push(n);
            } else if f.1 > 0.45 && m.1 < 0.5 {
                fringe.push(n);
            } else if m.2 < 0.02 && g.2 < 0.02 {
                open.push(n);
            }
        }
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        println!(
            "shrubs per chunk: monte {:.1} of {}, fringe {:.1} of {}, open {:.1} of {}",
            mean(&monte),
            monte.len(),
            mean(&fringe),
            fringe.len(),
            mean(&open),
            open.len()
        );
        assert!(monte.len() >= 15 && fringe.len() >= 15 && open.len() >= 50);
        assert!(mean(&monte) >= 40.0, "{}", mean(&monte));
        assert!(mean(&monte) >= 10.0 * mean(&open));
        assert!(mean(&fringe) >= 3.0 * mean(&open));
        // La pampa abierta no está pelada de matorral: un arbusto solitario
        // aquí y allá.
        assert!(mean(&open) > 1.0, "{}", mean(&open));
    }

    #[test]
    fn each_zone_has_its_shrub_species() {
        let (_, _, flora) = site();
        let shrubs = world_shrubs(&flora);
        let zone = |f: &dyn Fn(Vec2) -> bool| -> [f32; 3] {
            let own: Vec<&Shrub> = shrubs.iter().map(|(_, s)| s).filter(|s| f(s.at)).collect();
            assert!(own.len() > 1000, "{}", own.len());
            ShrubKind::ALL
                .map(|k| own.iter().filter(|s| s.kind == k).count() as f32 / own.len() as f32)
        };
        let monte = zone(&|p| flora.monte(p) > 0.9);
        let fringe = zone(&|p| flora.fringe(p) > 0.8 && flora.monte(p) < 0.5);
        let open =
            zone(&|p| flora.monte(p) < 0.01 && flora.grove(p) < 0.02 && flora.dryness(p) > 0.6);
        println!(
            "shares (piquillín, jarilla, fachinal): monte {monte:.2?}, fringe {fringe:.2?}, open dry {open:.2?}"
        );
        // El piquillín es el sotobosque del monte, el fachinal bordea las
        // isletas y la jarilla ocupa el terreno abierto y seco.
        assert!(monte[0] > 0.7 && monte[1] < 0.05, "{monte:?}");
        assert!(fringe[2] > 0.5 && fringe[2] > fringe[0], "{fringe:?}");
        assert!(
            open[1] > 0.45 && open[1] > open[0] && open[1] > open[2],
            "{open:?}"
        );
        for kind in 0..3 {
            for zone in [monte, fringe, open] {
                assert!(zone[kind] >= 0.0);
            }
        }
        // Las proporciones suman uno en todas partes.
        for k in 0..200 {
            let p = Vec2::new(k as f32 * 17.0 - 1700.0, k as f32 * -11.0 + 900.0);
            let total: f32 = flora.shrub_shares(p).iter().sum();
            assert!((total - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn shrubs_have_their_kind_sizes_and_each_its_own_look() {
        let (_, _, flora) = site();
        let shrubs: Vec<Shrub> = world_shrubs(&flora).into_iter().map(|(_, s)| s).collect();
        for kind in ShrubKind::ALL {
            let (lo, hi) = kind.height_range();
            let own: Vec<&Shrub> = shrubs.iter().filter(|s| s.kind == kind).collect();
            assert!(own.len() > 1000, "{kind:?}");
            for s in &own {
                assert!((lo..=hi).contains(&s.height), "{s:?}");
                assert!((0.85..=1.2).contains(&s.width), "{s:?}");
                assert!((0.07..0.35).contains(&s.trunk_radius()), "{s:?}");
                assert!(s.crown_radius() > s.trunk_radius());
            }
        }
        // Cada arbusto tiene su propio giro y su propia semilla.
        let looks: std::collections::HashSet<u32> = shrubs.iter().map(|s| s.variant).collect();
        assert!(looks.len() > shrubs.len() * 99 / 100);
    }

    #[test]
    fn shrubs_keep_off_town_ground_the_rail_the_world_edge_and_every_trunk() {
        let (world, line, flora) = site();
        let bounds = world.bounds;
        let mut chunks = between();
        chunks.extend((-4..4).flat_map(|x| (-4..4).map(move |y| ChunkCoord::new(x, y))));
        chunks.extend((-20..-17).flat_map(|x| (-20..-10).map(move |y| ChunkCoord::new(x, y))));
        // El monte y las isletas del sudoeste, llenos de ambos.
        chunks.extend((-19..1).flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y))));
        let trees: Vec<Tree> = chunks.iter().flat_map(|&c| flora.trees(c)).collect();
        let (mut count, mut near_border) = (0, 0);
        let size = WorldConfig::default().chunk_size;
        for c in chunks {
            for s in flora.shrubs(c) {
                for area in &world.terrain.flat_areas {
                    assert!(
                        outside(area, s.at) >= area.falloff + FloraSite::SHRUB_TOWN_MARGIN,
                        "shrub on town ground {s:?}"
                    );
                }
                assert!(
                    rail_distance(&line, s.at) >= FloraSite::SHRUB_RAIL_MARGIN,
                    "{s:?}"
                );
                assert!(
                    bounds
                        .contain(s.at, FloraSite::SHRUB_EDGE_MARGIN)
                        .push_x
                        .is_none()
                );
                assert!(
                    bounds
                        .contain(s.at, FloraSite::SHRUB_EDGE_MARGIN)
                        .push_y
                        .is_none()
                );
                // Lejos de todo tronco, también de los de los chunks vecinos, y
                // con lugar para que el jinete pase entre ellos.
                for t in &trees {
                    assert!(
                        t.at.distance(s.at)
                            >= t.trunk_radius() + s.trunk_radius() + FloraSite::SHRUB_TRUNK_GAP
                                - 1e-3,
                        "{s:?} too near the trunk of {t:?}"
                    );
                }
                let b = c.bounds(size);
                near_border += usize::from(
                    (s.at.x - b.min.x)
                        .min(b.max.x - s.at.x)
                        .min((s.at.y - b.min.y).min(b.max.y - s.at.y))
                        < 2.0,
                );
                count += 1;
            }
        }
        assert!(count > 5000 && near_border > 100, "{count} {near_border}");
    }

    #[test]
    fn the_shrub_border_covers_the_farthest_a_trunk_keeps_a_shrub_off() {
        // El tronco de árbol más robusto entre todas las especies y edades, en
        // su altura máxima.
        let trunk = [Species::Calden, Species::Chanar]
            .iter()
            .flat_map(|&species| Age::ALL.iter().map(move |&age| (species, age)))
            .map(|(species, age)| {
                Tree {
                    at: Vec2::ZERO,
                    species,
                    age,
                    height: age.height_range(species).1,
                    width: 1.0,
                    yaw: 0.0,
                    variant: 0,
                }
                .trunk_radius()
            })
            .fold(0.0, f32::max);
        let shrub = ShrubKind::ALL
            .iter()
            .map(|&k| {
                let sample = Shrub {
                    at: Vec2::ZERO,
                    kind: k,
                    height: k.height_range().1,
                    width: 1.0,
                    yaw: 0.0,
                    variant: 0,
                };
                sample.trunk_radius()
            })
            .fold(0.0, f32::max);
        let reach = trunk + shrub + FloraSite::SHRUB_TRUNK_GAP;
        assert!(
            FloraSite::SHRUB_BORDER >= reach,
            "a shrub {reach} m from a trunk across the border is missed ({} m)",
            FloraSite::SHRUB_BORDER
        );
    }

    #[test]
    fn shrubs_keep_off_roads_and_lagoons_and_apart() {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let roads = world.terrain.roads.clone().expect("roads laid");
        let flora = FloraSite::new(config, &world.terrain, &line);
        let mut chunks: Vec<ChunkCoord> = roads
            .segments()
            .step_by(40)
            .map(|(a, _)| config.chunk_of(a))
            .collect();
        chunks.sort_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let mut shrubs = 0;
        for c in chunks {
            let own = flora.shrubs(c);
            for (i, s) in own.iter().enumerate() {
                assert!(
                    roads.distance(s.at) >= FloraSite::SHRUB_ROAD_MARGIN,
                    "{s:?}"
                );
                // Pasillos entre los arbustos de un chunk.
                assert!(
                    own[i + 1..]
                        .iter()
                        .all(|o| o.at.distance(s.at) >= FloraSite::SHRUB_SPACING),
                    "{s:?}"
                );
                shrubs += 1;
            }
        }
        assert!(shrubs > 500, "{shrubs}");

        let lagoon_config = crate::lagoon::tests::arocena_config();
        let lagoon = Lagoon::new(&lagoon_config, &Terrain::default());
        let terrain = Terrain::default().with_lagoon(lagoon.clone());
        let world = WorldConfig::default();
        let (line, _) = RailLine::build(None, &[], world.bounds());
        let flora = FloraSite::new(world, &terrain, &line);
        let keep = lagoon.keep_out();
        let (lo, hi) = (
            ChunkCoord::containing(keep.min, world.chunk_size),
            ChunkCoord::containing(keep.max, world.chunk_size),
        );
        let mut near = 0;
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                for s in flora.shrubs(ChunkCoord { x: cx, y: cy }) {
                    let off = lagoon.offshore(s.at);
                    assert!(off >= FloraSite::SHRUB_LAGOON_MARGIN, "{s:?}");
                    near += usize::from(off < 40.0);
                }
            }
        }
        assert!(near > 10, "{near}");
    }

    #[test]
    fn planting_the_world_makes_no_shrub_an_obstacle_and_a_rider_rides_through_one() {
        let (mut world, _, flora) = site();
        let before = world.obstacles.len();
        let planted = world.plant(&flora);
        let shrubs = world_shrubs(&flora);
        // Solo se plantan los árboles: los arbustos (y el pasto) nunca
        // bloquean.
        assert_eq!(planted, flora.all_trees().len());
        assert_eq!(world.obstacles.len(), before + planted);
        for (_, s) in shrubs.iter().step_by(97) {
            assert!(!world.obstacles.contains(&s.obstacle()), "{s:?}");
        }
        // Un piquillín alto sin nada plantado en el pasillo al oeste.
        let lane = |s: &Shrub| Aabb::new(s.at - Vec2::new(1.0, 3.0), s.at + Vec2::new(11.0, 3.0));
        let shrub = shrubs
            .iter()
            .map(|(_, s)| *s)
            .find(|s| {
                s.kind == ShrubKind::Piquillin
                    && s.height > 2.5
                    && world.obstacles.near(lane(s)).is_empty()
            })
            .expect("a tall piquillín in the open");
        // Cabalgar hacia su núcleo desde 10 m al este, hacia el oeste, al
        // galope: el jinete lo atraviesa de largo.
        let mut rider = Rider::new(
            shrub.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let params = RiderParams::default();
        let mut through = false;
        for _ in 0..240 {
            rider.step(input, &params, &world, 1.0 / 60.0);
            through |= shrub.obstacle().contains_point(rider.position);
        }
        assert!(through, "never reached the shrub's core");
        assert!(rider.position.x < shrub.at.x - 1.0, "{:?}", rider.position);
    }

    #[test]
    fn a_planted_chunk_stops_a_rider_at_its_tree_not_its_shrub_and_unplanting_frees_it() {
        let (mut world, _, flora) = site();
        // Un chunk con los dos tipos de planta.
        let coord = between()
            .into_iter()
            .find(|&c| {
                flora.trees(c).iter().any(|t| t.species == Species::Calden)
                    && flora
                        .shrubs(c)
                        .iter()
                        .any(|s| s.kind == ShrubKind::Piquillin)
            })
            .expect("a chunk with a caldén and a piquillín");
        let before = world.obstacles.len();
        let trees = flora.trees(coord);
        assert_eq!(world.plant_chunk(&flora, coord), trees.len());
        // Cargado por streaming, no permanente: la lista permanente no se toca,
        // y en la capa solo hay árboles.
        assert_eq!(world.obstacles.len(), before);
        assert_eq!(world.obstacles.streamed_len(), trees.len());
        let params = RiderParams::default();
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let radius = Circle::new(Rider::COLLIDER_RADIUS).radius;
        let calden = *trees.iter().find(|t| t.species == Species::Calden).unwrap();
        // Cabalgar hacia el tronco desde 10 m al este, hacia el oeste, al
        // galope.
        let mut rider = Rider::new(
            calden.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        for _ in 0..120 {
            rider.step(input, &params, &world, 1.0 / 60.0);
            let gap = calden
                .obstacle()
                .closest_point(rider.position)
                .distance(rider.position);
            assert!(gap >= radius - 1e-3, "inside the tree");
        }
        assert!(rider.position.x > calden.at.x - 0.5, "{:?}", rider.position);
        // Un arbusto del mismo chunk no detiene a un jinete.
        let shrub = flora
            .shrubs(coord)
            .into_iter()
            .find(|s| {
                let lane = Aabb::new(s.at - Vec2::new(1.0, 3.0), s.at + Vec2::new(11.0, 3.0));
                s.kind == ShrubKind::Piquillin && world.obstacles.near(lane).is_empty()
            })
            .expect("a lone piquillín");
        let mut rider = Rider::new(
            shrub.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        for _ in 0..240 {
            rider.step(input, &params, &world, 1.0 / 60.0);
        }
        assert!(rider.position.x < shrub.at.x - 1.0, "{:?}", rider.position);
        // Sin plantar, el terreno del árbol vuelve a estar libre.
        assert!(world.unplant_chunk(coord));
        assert!(!world.unplant_chunk(coord));
        assert_eq!(world.obstacles.streamed_len(), 0);
        let mut rider = Rider::new(
            calden.at + Vec2::new(10.0, 0.0),
            Heading::from_radians(std::f32::consts::PI),
            &world,
        );
        for _ in 0..240 {
            rider.step(input, &params, &world, 1.0 / 60.0);
        }
        assert!(rider.position.x < calden.at.x - 1.0, "{:?}", rider.position);
    }

    #[test]
    fn planting_a_chunk_again_gives_the_same_trees_and_every_chunk_adds_up_to_the_world() {
        let (mut world, _, flora) = site();
        let coord = ChunkCoord::new(5, -4);
        let first = world.plant_chunk(&flora, coord);
        let area = coord.bounds(world.config.chunk_size).expanded(20.0);
        let seen: Vec<Aabb> = world.obstacles.near(area).into_iter().copied().collect();
        // Reemplazar, no apilar: una segunda plantación da el mismo conjunto.
        assert_eq!(world.plant_chunk(&flora, coord), first);
        assert_eq!(world.obstacles.streamed_len(), first);
        let again: Vec<Aabb> = world.obstacles.near(area).into_iter().copied().collect();
        assert_eq!(seen, again);
        // Descartadas y vueltas a cargar, las mismas huellas.
        world.unplant_chunk(coord);
        assert!(world.obstacles.near(area).is_empty());
        world.plant_chunk(&flora, coord);
        let back: Vec<Aabb> = world.obstacles.near(area).into_iter().copied().collect();
        assert_eq!(seen, back);
        // Todos los chunks del mundo, cargados por streaming, son lo que agrega
        // plantar el mundo.
        let mut all = World::default();
        all.plant(&flora);
        let mut streamed = World::default();
        let total: usize = world_chunks()
            .into_iter()
            .map(|c| streamed.plant_chunk(&flora, c))
            .sum();
        assert_eq!(total, all.obstacles.len());
        assert_eq!(streamed.obstacles.streamed_len(), all.obstacles.len());
    }

    /// Todos los chunks del mundo: sus árboles antes del raleo y después.
    fn planted_and_kept(flora: &FloraSite) -> Vec<(ChunkCoord, Vec<Tree>, Vec<Tree>)> {
        let size = flora.config.chunk_size;
        let bounds = flora.config.bounds();
        let lo = ChunkCoord::containing(bounds.min, size);
        let hi = ChunkCoord::containing(bounds.max, size);
        (lo.y..=hi.y)
            .flat_map(|y| (lo.x..=hi.x).map(move |x| ChunkCoord::new(x, y)))
            .map(|c| {
                let planted = flora.chunk_trees(c, &flora.rail_near(c));
                (c, planted, flora.trees(c))
            })
            .collect()
    }

    #[test]
    fn about_half_the_trees_stay_evenly_over_monte_groves_and_lone_trees() {
        let (_, _, flora) = site();
        let all = planted_and_kept(&flora);
        // La proporción que queda en cada tipo de terreno: el monte, las
        // isletas, los árboles solitarios de la pampa abierta (plantados,
        // conservados).
        let mut kinds = [(0.0_f32, 0.0_f32); 3];
        let (mut planted, mut kept) = (0.0_f32, 0.0_f32);
        for (c, before, after) in &all {
            // Los árboles conservados son exactamente los plantados que se
            // quedan.
            assert!(after.iter().all(|t| before.contains(t)));
            assert!(after.len() <= before.len());
            let (monte, grove) = (
                chunk_range(*c, |p| flora.monte(p)),
                chunk_range(*c, |p| flora.grove(p)),
            );
            let kind = if monte.1 > 0.8 {
                0
            } else if monte.2 < 0.02 && grove.1 > 0.6 {
                1
            } else if monte.2 < 0.02 && grove.2 < 0.02 {
                2
            } else {
                continue;
            };
            kinds[kind].0 += before.len() as f32;
            kinds[kind].1 += after.len() as f32;
        }
        for (_, before, after) in &all {
            planted += before.len() as f32;
            kept += after.len() as f32;
        }
        println!("trees: {planted} planted, {kept} kept; per kind {kinds:?}");
        assert!(planted > 50_000.0, "{planted}");
        let share = kept / planted;
        assert!((share - 0.616).abs() < 0.01, "{share}");
        assert_eq!(FloraSite::TREE_KEEP, 0.616);
        for (kind, (before, after)) in kinds.iter().enumerate() {
            // Dentro de tres desviaciones estándar de una proporción justa de
            // 0,616.
            let slack = 3.0 * (0.2365 / before).sqrt();
            assert!(*before > 50.0, "kind {kind}: {before}");
            assert!(
                (after / before - 0.616).abs() < slack,
                "kind {kind}: {after}/{before}"
            );
        }
    }

    #[test]
    fn the_same_trees_go_whichever_chunk_asks_and_shrubs_are_not_thinned() {
        let (_, _, flora) = site();
        for c in world_chunks().into_iter().step_by(37) {
            let chunk = flora.chunk(c);
            assert_eq!(chunk.trees, flora.trees(c));
            assert_eq!(chunk.shrubs, flora.shrubs(c));
        }
        let (trees, shrubs) = flora.all_plants();
        assert_eq!(trees, flora.all_trees());
        assert_eq!(shrubs, flora.all_shrubs());
    }

    #[test]
    fn trees_cluster_in_groves() {
        let (_, _, flora) = site();
        let counts: Vec<f32> = (-19..1)
            .flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y)))
            .map(|c| flora.chunk(c).trees.len() as f32)
            .collect();
        let n = counts.len() as f32;
        let mean = counts.iter().sum::<f32>() / n;
        let variance = counts.iter().map(|c| (c - mean).powi(2)).sum::<f32>() / n;
        // Una distribución uniforme (Poisson) tiene una varianza igual a su
        // media.
        assert!(mean > 2.0, "mean {mean}");
        assert!(variance > 4.0 * mean, "variance {variance}, mean {mean}");
        // Algunos chunks son pampa abierta, otros contienen una isleta.
        assert!(counts.iter().any(|&c| c <= 2.0));
        assert!(counts.iter().any(|&c| c >= 25.0));
        // La máscara es suave: no hay saltos entre dos puntos a un metro de
        // distancia.
        for k in 0..500 {
            let p = Vec2::new(k as f32 * 7.3 - 1800.0, k as f32 * -3.1 + 400.0);
            let g = flora.grove(p);
            assert!((0.0..=1.0).contains(&g));
            assert!((g - flora.grove(p + Vec2::new(1.0, 0.0))).abs() < 0.05);
        }
    }

    #[test]
    fn dryness_is_smooth_and_varied() {
        let (_, _, flora) = site();
        let (mut lo, mut hi) = (1.0_f32, 0.0_f32);
        for k in 0..2000 {
            let p = Vec2::new(k as f32 * 3.7 - 3000.0, (k as f32 * 0.37).sin() * 900.0);
            let d = flora.dryness(p);
            assert!((0.0..=1.0).contains(&d));
            assert!((d - flora.dryness(p + Vec2::new(0.5, 0.5))).abs() < 0.02);
            assert_eq!(d, grass_dryness(&Terrain::default(), p));
            lo = lo.min(d);
            hi = hi.max(d);
        }
        assert!(hi - lo > 0.6, "{lo}..{hi}");
    }

    #[test]
    fn every_tree_is_an_obstacle_and_a_rider_is_pushed_off_a_calden_trunk() {
        let (mut world, _, flora) = site();
        let before = world.obstacles.len();
        let planted = world.plant(&flora);
        let all = flora.all_trees();
        // El tronco de cada árbol es un obstáculo.
        assert_eq!(planted, all.len());
        assert_eq!(world.obstacles.len(), before + planted);
        // Los árboles de cada chunk están en la lista del mundo, esté el chunk
        // cargado o no.
        for c in between() {
            for tree in flora.chunk(c).trees {
                assert!(all.contains(&tree), "{tree:?}");
                assert!(world.obstacles.contains(&tree.obstacle()));
            }
        }
        let calden = *all
            .iter()
            .find(|t| t.species == Species::Calden)
            .expect("a caldén");
        // Cabalgar hacia el tronco desde 10 m al este, hacia el oeste, al
        // galope.
        let start = calden.at + Vec2::new(10.0, 0.0);
        let mut rider = Rider::new(start, Heading::from_radians(std::f32::consts::PI), &world);
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        let params = RiderParams::default();
        let radius = Circle::new(Rider::COLLIDER_RADIUS).radius;
        for _ in 0..120 {
            rider.step(input, &params, &world, 1.0 / 60.0);
            let gap = calden
                .obstacle()
                .closest_point(rider.position)
                .distance(rider.position);
            assert!(
                gap >= radius - 1e-3,
                "inside the trunk at {:?}",
                rider.position
            );
        }
        // Se detuvo contra el tronco en lugar de pasarlo.
        assert!(rider.position.x > calden.at.x, "{:?}", rider.position);
    }

    #[test]
    fn nothing_grows_on_the_trains_path_anywhere_along_the_line() {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let path = line.full_path(&towns);
        let flora = FloraSite::new(config, &world.terrain, &line).with_track(&path);
        let gap = |p: Vec2| p.distance(path.at(path.project(p)));
        // Cada chunk que cruza el recorrido, muestreado cada 20 m.
        let mut chunks: Vec<ChunkCoord> = (0..=(path.length() / 20.0) as usize)
            .map(|k| config.chunk_of(path.at(k as f32 * 20.0)))
            .collect();
        chunks.sort_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let (mut trees, mut tufts, mut shrubs) = (0, 0, 0);
        for c in chunks {
            let f = flora.chunk(c);
            for t in &f.trees {
                // El tronco y la copa se mantienen lejos de los vagones.
                let g = gap(t.at);
                assert!(
                    g >= FloraSite::TREE_RAIL_MARGIN - 1e-3,
                    "{t:?}: {g} m from the track"
                );
                trees += 1;
            }
            for t in &f.tufts {
                assert!(gap(t.at) >= FloraSite::TUFT_RAIL_MARGIN - 1e-3, "{t:?}");
                tufts += 1;
            }
            // El núcleo de tallos de un arbusto tampoco bloquea nunca la línea.
            for sh in &f.shrubs {
                assert!(gap(sh.at) >= FloraSite::SHRUB_RAIL_MARGIN - 1e-3, "{sh:?}");
                shrubs += 1;
            }
        }
        assert!(
            trees > 20 && tufts > 1000 && shrubs > 100,
            "{trees} {tufts} {shrubs}"
        );
        // El margen cubre el medio ancho de un vagón con lugar para una copa.
        const { assert!(FloraSite::TREE_RAIL_MARGIN > crate::LineTrain::WIDTH / 2.0 + 6.0) };
    }

    #[test]
    fn nothing_grows_on_a_road_or_its_verges() {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let roads = world.terrain.roads.clone().expect("roads laid");
        let flora = FloraSite::new(config, &world.terrain, &line);
        let mut chunks: Vec<ChunkCoord> = roads
            .segments()
            .step_by(40)
            .map(|(a, _)| config.chunk_of(a))
            .collect();
        chunks.sort_by_key(|c| (c.x, c.y));
        chunks.dedup();
        let (mut trees, mut tufts) = (0, 0);
        for c in chunks {
            let f = flora.chunk(c);
            for t in &f.trees {
                let gap = roads.distance(t.at);
                assert!(gap >= FloraSite::TREE_ROAD_MARGIN, "{t:?}: {gap}");
                trees += 1;
            }
            for t in &f.tufts {
                assert!(roads.distance(t.at) >= FloraSite::TUFT_ROAD_MARGIN, "{t:?}");
                tufts += 1;
            }
            assert_eq!(f.trees, flora.trees(c));
        }
        assert!(trees > 20 && tufts > 2000, "{trees} {tufts}");
    }

    #[test]
    fn grass_grows_to_the_lagoons_wet_mud_and_trees_keep_back() {
        let config = crate::lagoon::tests::arocena_config();
        let lagoon = Lagoon::new(&config, &Terrain::default());
        let terrain = Terrain::default().with_lagoon(lagoon.clone());
        let world = WorldConfig::default();
        let (line, _) = RailLine::build(None, &[], world.bounds());
        let flora = FloraSite::new(world, &terrain, &line);
        let keep = lagoon.keep_out();
        let (lo, hi) = (
            ChunkCoord::containing(keep.min, world.chunk_size),
            ChunkCoord::containing(keep.max, world.chunk_size),
        );
        let mut near = 0;
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                let chunk = flora.chunk(ChunkCoord { x: cx, y: cy });
                for t in &chunk.tufts {
                    assert!(!lagoon.is_bare(t.at), "{t:?} on the mud");
                    let off = lagoon.offshore(t.at);
                    near += usize::from(off < Lagoon::MUD_BAND + 20.0);
                }
                for t in &chunk.trees {
                    assert!(
                        lagoon.offshore(t.at) >= FloraSite::TREE_LAGOON_MARGIN,
                        "{t:?}"
                    );
                }
            }
        }
        // La barranca tiene pasto hasta el barro.
        assert!(near > 100, "{near}");
    }

    /// Distancia desde `p` hasta el núcleo nivelado del pueblo más cercano, en
    /// metros.
    fn town_distance(world: &World, p: Vec2) -> f32 {
        world
            .terrain
            .flat_areas
            .iter()
            .map(|a| outside(a, p))
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn the_monte_keeps_back_from_the_towns_and_the_world_total_holds() {
        // Los pueblos distribuidos en el mundo más grande (2026-10-27, antes de la
        // redistribución, con 0,56 conservado): 190.919 árboles, 28,4 por chunk a menos de
        // 1,5 km de un pueblo y 26,3 más allá de 3 km. Después de "un 10% mas de arboles
        // grandes y zonas boscosas" (0,616 conservado, montes más viejos, un monte más
        // amplio): 218.995 árboles, 6,0 por chunk cerca, 35,4 lejos. El mundo de 2 pueblos
        // de `site()` es demasiado chico para darle al campo lejano lugar para compensarlo.
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads);
        let flora = FloraSite::new(config, &world.terrain, &line);
        let size = config.chunk_size;
        let (lo, hi) = (
            ChunkCoord::containing(config.bounds().min, size),
            ChunkCoord::containing(config.bounds().max, size),
        );
        let (mut near, mut far, mut total, mut peak) = ((0, 0), (0, 0), 0, 0);
        for y in lo.y..=hi.y {
            for x in lo.x..=hi.x {
                let c = ChunkCoord::new(x, y);
                let n = flora.trees(c).len();
                peak = peak.max(flora.chunk_trees(c, &flora.rail_near(c)).len());
                total += n;
                match town_distance(&world, c.bounds(size).center()) {
                    d if d < FloraSite::TOWN_NEAR => (near.0, near.1) = (near.0 + n, near.1 + 1),
                    d if d > FloraSite::TOWN_FULL => (far.0, far.1) = (far.0 + n, far.1 + 1),
                    _ => {}
                }
            }
        }
        let density = |(n, chunks): (usize, usize)| n as f32 / chunks as f32;
        println!(
            "trees: {total}, near {:.1} a chunk, far {:.1}, peak planted {peak}",
            density(near),
            density(far)
        );
        assert!(near.1 > 300 && far.1 > 3000, "{near:?} {far:?}");
        assert!(density(near) < 0.4 * density(far), "{near:?} {far:?}");
        let held = total as f32 / 218_995.0;
        assert!((0.95..=1.05).contains(&held), "{total} trees: {held}");
        // El aumento extiende más los montes; el presupuesto de un chunk queda
        // lejos.
        assert!(peak < FloraSite::TREE_BUDGET, "{peak}");
    }

    #[test]
    fn the_land_round_a_settlement_is_thinner_than_the_open_field_further_out() {
        let (world, ..) = crate::settlement::tests::settled();
        let (flora, trees, hubs) = shipped();
        assert_eq!(flora.worked.len(), hubs.len());
        let away = |p: Vec2| hubs.iter().map(|h| h.distance(p)).fold(f32::MAX, f32::min);
        // Árboles y terreno según la distancia al centro más cercano: el anillo
        // que desmonta un casco y el campo a su alrededor.
        let (mut ring, mut field) = ((0.0_f32, 0.0_f32), (0.0_f32, 0.0_f32));
        // El monte, las isletas y los árboles solitarios que ralea `rural`: no
        // el ombú ni los árboles de pueblo escapados, que atrae un
        // asentamiento.
        let native = |t: &Tree| !is_town(t.species) && t.species != Species::Ombu;
        for t in trees.iter().filter(|t| native(t)) {
            match away(t.at) {
                d if (FloraSite::WORKED_NEAR..FloraSite::WORKED_FULL).contains(&d) => ring.0 += 1.0,
                d if (800.0..1500.0).contains(&d) => field.0 += 1.0,
                _ => {}
            }
        }
        let bounds = world.config.bounds();
        let mut at = bounds.min;
        while at.y < bounds.max.y {
            at.x = bounds.min.x;
            while at.x < bounds.max.x {
                match away(at) {
                    d if (FloraSite::WORKED_NEAR..FloraSite::WORKED_FULL).contains(&d) => {
                        ring.1 += 1.0
                    }
                    d if (800.0..1500.0).contains(&d) => field.1 += 1.0,
                    _ => {}
                }
                at.x += 40.0;
            }
            at.y += 40.0;
        }
        let (ring, field) = (ring.0 / ring.1, field.0 / field.1);
        println!("trees per sample: ring {ring:.4}, field {field:.4}");
        assert!(field > 0.0 && ring < 0.7 * field, "{ring} {field}");
    }

    #[test]
    fn the_rural_share_is_smooth_across_chunk_borders_and_follows_the_distance() {
        let (world, _, flora) = redistributed();
        let area = world.terrain.flat_areas[0];
        // Al oeste del pueblo, lejos del otro.
        let west = area.center.x - area.half_extents.x;
        let at = |d: f32| flora.rural(Vec2::new(west - d, area.center.y));
        // Desde el borde del pueblo, pasando su anillo cercano, hasta el campo
        // lejano.
        assert!(
            (at(10.0) - FloraSite::TOWN_NEAR_SHARE).abs() < 0.02,
            "{}",
            at(10.0)
        );
        assert!(
            (at(FloraSite::TOWN_FULL) - 1.0).abs() < 0.05,
            "{}",
            at(3000.0)
        );
        assert!(at(100.0) < 0.2 && at(2000.0) > at(1000.0) && at(2900.0) > 0.8);
        assert!((at(FloraSite::FAR_FULL) - FloraSite::FAR_BOOST).abs() < 0.01);
        // Continuo: no hay escalones en 1 m en ningún punto de la línea, a
        // través de cada borde de chunk que cruza.
        let mut last = at(0.0);
        for k in 1..8000 {
            let now = at(k as f32);
            assert!((now - last).abs() < 0.01, "{k} m: {last} to {now}");
            last = now;
        }
        // La misma proporción sea cual sea el sitio que pregunte (es una
        // función del lugar, no de ningún estado): un segundo sitio, construido
        // por separado, coincide.
        let p = Vec2::new(west - 1234.5, area.center.y + 321.0);
        let (_, _, other) = redistributed();
        assert_eq!(flora.rural(p), other.rural(p));
    }

    #[test]
    fn redistributed_trees_keep_their_spacing_across_chunk_borders_and_are_the_same_whichever_chunk_asks()
     {
        let (world, _, flora) = redistributed();
        // Chunks que están sobre la rampa de salida del pueblo (de 1,5 a 3 km).
        let area = world.terrain.flat_areas[0];
        let size = world.config.chunk_size;
        let from = world.config.chunk_of(Vec2::new(
            area.center.x + area.half_extents.x + 1500.0,
            area.center.y,
        ));
        let block: Vec<ChunkCoord> = (0..6)
            .flat_map(|dx| (-3..3).map(move |dy| ChunkCoord::new(from.x + dx, from.y + dy)))
            .filter(|c| world.config.has_chunk(*c))
            .collect();
        let trees: Vec<(ChunkCoord, Tree)> = block
            .iter()
            .flat_map(|&c| flora.trees(c).into_iter().map(move |t| (c, t)))
            .collect();
        assert!(trees.len() > 100, "{} over {size} m chunks", trees.len());
        for (i, (ca, a)) in trees.iter().enumerate() {
            for (cb, b) in &trees[i + 1..] {
                if ca != cb {
                    assert!(a.at.distance(b.at) >= a.spacing().max(b.spacing()));
                }
            }
        }
        for &c in &block {
            assert_eq!(flora.trees(c), flora.chunk(c).trees);
        }
    }

    /// La flora del mundo distribuido (pueblos, caminos, alambrados y
    /// asentamientos tal como los coloca el juego) con todos los árboles que
    /// planta, y los centros de los asentamientos. Se construye una sola vez
    /// para las pruebas que cuentan especies en todo el mundo.
    pub(crate) fn shipped() -> &'static (FloraSite, Vec<Tree>, Vec<Vec2>) {
        static SHIPPED: std::sync::OnceLock<(FloraSite, Vec<Tree>, Vec<Vec2>)> =
            std::sync::OnceLock::new();
        SHIPPED.get_or_init(|| {
            let (world, towns, line, fences, layout) = crate::settlement::tests::settled();
            let flora = FloraSite::new(world.config, &world.terrain, line)
                .with_track(&line.full_path(towns))
                .with_fences(fences)
                .with_settlements(layout);
            let hubs = layout.settlements.iter().map(|s| s.hub).collect();
            let trees = flora.all_trees();
            (flora, trees, hubs)
        })
    }

    fn share_of(trees: &[Tree], keep: impl Fn(Species) -> bool) -> f32 {
        trees.iter().filter(|t| keep(t.species)).count() as f32 / trees.len() as f32
    }

    fn is_town(species: Species) -> bool {
        matches!(
            species,
            Species::Paraiso | Species::Olmo | Species::Eucalipto | Species::Aromo
        )
    }

    #[test]
    fn the_new_species_keep_their_shares_of_the_shipped_world() {
        let (_, trees, hubs) = shipped();
        assert!(trees.len() > 100_000 && !hubs.is_empty(), "{}", trees.len());
        let share = |s: Species| share_of(trees, |t| t == s);
        for s in [
            Species::Calden,
            Species::Chanar,
            Species::Algarrobo,
            Species::Tala,
            Species::Espinillo,
            Species::Ombu,
        ] {
            println!(
                "{s:?}: {:.4} ({})",
                share(s),
                trees.iter().filter(|t| t.species == s).count()
            );
        }
        let town = share_of(trees, is_town);
        println!("town species: {town:.4}");
        for s in [
            Species::Paraiso,
            Species::Olmo,
            Species::Eucalipto,
            Species::Aromo,
        ] {
            println!("  {s:?}: {:.4}", share(s));
            assert!(share(s) > 0.0005, "{s:?}");
        }
        let calden = share(Species::Calden);
        // El caldén sigue dominando el campo; el algarrobo es un compañero
        // modesto; el tala y el espinillo son una proporción chica, bastante
        // por debajo.
        assert!(calden > 0.4, "{calden}");
        assert!((0.04..=0.12).contains(&share(Species::Algarrobo)));
        for s in [Species::Tala, Species::Espinillo] {
            assert!((0.01..=0.045).contains(&share(s)), "{s:?} {}", share(s));
            assert!(share(s) < 0.15 * calden, "{s:?}");
        }
        // Los árboles de pueblo escapados son unos pocos por cada mil, por
        // debajo de todo compañero nativo: casi todos junto a un camino o un
        // asentamiento.
        assert!((0.006..=0.04).contains(&town), "{town}");
        // El ombú es raro: más o menos uno por asentamiento en un mundo de
        // 200.000 árboles.
        let ombu = trees.iter().filter(|t| t.species == Species::Ombu).count();
        assert!((8..=60).contains(&ombu), "{ombu}");
    }

    #[test]
    fn the_ombu_stands_only_beside_a_settlement_and_never_in_the_open_field() {
        let (_, trees, hubs) = shipped();
        let away = |p: Vec2| hubs.iter().map(|h| h.distance(p)).fold(f32::MAX, f32::min);
        let ombus: Vec<&Tree> = trees
            .iter()
            .filter(|t| t.species == Species::Ombu)
            .collect();
        assert!(!ombus.is_empty());
        for t in &ombus {
            assert!(
                away(t.at) <= FloraSite::OMBU_REACH,
                "an ombú {} m from the nearest settlement",
                away(t.at)
            );
        }
        // Y no lo tiene cada asentamiento: algunos no tienen ninguno.
        let with: usize = hubs
            .iter()
            .filter(|h| {
                ombus
                    .iter()
                    .any(|t| t.at.distance(**h) <= FloraSite::OMBU_REACH)
            })
            .count();
        println!("{with} of {} settlements have an ombú", hubs.len());
        assert!(with * 10 >= hubs.len() * 2 && with < hubs.len(), "{with}");
        // Un sitio sin asentamientos nunca planta uno.
        let (_, _, open) = site();
        assert!(sample(&open).iter().all(|t| t.species != Species::Ombu));
    }

    #[test]
    fn the_algarrobo_grows_in_the_monte_and_tala_and_espinillo_on_its_edges() {
        let (flora, trees, _) = shipped();
        let mean = |s: Species, f: &dyn Fn(Vec2) -> f32| {
            let own: Vec<&Tree> = trees.iter().filter(|t| t.species == s).collect();
            own.iter().map(|t| f(t.at)).sum::<f32>() / own.len() as f32
        };
        let monte = |p: Vec2| flora.monte(p);
        let fringe = |p: Vec2| flora.fringe(p);
        let (alg, calden) = (
            mean(Species::Algarrobo, &monte),
            mean(Species::Calden, &monte),
        );
        println!("monte: algarrobo {alg:.2}, caldén {calden:.2}");
        assert!(alg > 0.8 && alg > calden + 0.1, "{alg} {calden}");
        for s in [Species::Tala, Species::Espinillo] {
            let (own, all) = (mean(s, &fringe), mean(Species::Calden, &fringe));
            println!("fringe: {s:?} {own:.2}, caldén {all:.2}");
            assert!(own > all + 0.05, "{s:?}");
            assert!(mean(s, &monte) < alg, "{s:?} is not a monte companion");
        }
    }

    #[test]
    fn escaped_town_trees_are_denser_by_roads_and_settlements_than_in_the_open_field() {
        let (flora, trees, hubs) = shipped();
        let roads = flora.roads.as_ref().expect("the shipped roads");
        let away = |p: Vec2| hubs.iter().map(|h| h.distance(p)).fold(f32::MAX, f32::min);
        let near = |p: Vec2| {
            roads.near(p, FloraSite::ESCAPE_ROAD_REACH).is_some()
                || away(p) < FloraSite::ESCAPE_HUB_FAR
        };
        let open = |p: Vec2| roads.distance(p) > 400.0 && away(p) > 1000.0;
        let (mut by, mut apart) = ((0.0_f32, 0.0_f32), (0.0_f32, 0.0_f32));
        for t in trees {
            let town = f32::from(is_town(t.species));
            if near(t.at) {
                by = (by.0 + town, by.1 + 1.0);
            } else if open(t.at) {
                apart = (apart.0 + town, apart.1 + 1.0);
            }
        }
        // Por árbol que hay ahí, y por hectárea de terreno.
        let (by_share, open_share) = (by.0 / by.1, apart.0 / apart.1);
        println!(
            "town share of trees: by {by_share:.4} ({}), open {open_share:.4} ({})",
            by.0, apart.0
        );
        assert!(
            open_share > 0.0 && by_share > 3.0 * open_share,
            "{by_share} {open_share}"
        );
        let bounds = flora.config.bounds();
        let (mut ground_by, mut ground_open) = (0.0_f32, 0.0_f32);
        let mut at = bounds.min;
        while at.y < bounds.max.y {
            at.x = bounds.min.x;
            while at.x < bounds.max.x {
                if near(at) {
                    ground_by += 1.0;
                } else if open(at) {
                    ground_open += 1.0;
                }
                at.x += 50.0;
            }
            at.y += 50.0;
        }
        let (d_by, d_open) = (by.0 / ground_by, apart.0 / ground_open);
        println!("town trees per sample: by {d_by:.5}, open {d_open:.5}");
        assert!(d_by > 5.0 * d_open, "{d_by} {d_open}");
    }

    #[test]
    fn the_ombu_and_escaped_trees_come_on_top_of_the_native_trees_and_never_replace_them() {
        let (flora, _, hubs) = shipped();
        let (mut natives, mut extras) = (0, 0);
        for hub in hubs {
            for c in flora.config.chunks_near(*hub, 3) {
                let rail = flora.rail_near(c);
                let planted = flora.chunk_trees(c, &rail);
                // Los árboles propios del campo del chunk, que ceden solo ante
                // los árboles propios de sus vecinos anteriores (como lo hacían
                // antes de que hubiera extras), están todos entre lo plantado.
                let mut native = flora.native_trees(c, &rail);
                for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0)] {
                    let earlier = ChunkCoord::new(c.x + dx, c.y + dy);
                    if !flora.config.has_chunk(earlier) {
                        continue;
                    }
                    let theirs = flora.native_trees(earlier, &flora.rail_near(earlier));
                    native.retain(|t| {
                        theirs
                            .iter()
                            .all(|u| u.at.distance(t.at) >= u.spacing().max(t.spacing()))
                    });
                }
                for t in &native {
                    assert!(
                        planted.contains(t),
                        "{c:?}: the native tree at {:?} is gone",
                        t.at
                    );
                }
                // Plantados uno al lado del otro: ninguno está más cerca que su
                // separación.
                for (i, a) in planted.iter().enumerate() {
                    for b in &planted[i + 1..] {
                        assert!(
                            a.at.distance(b.at) >= a.spacing().max(b.spacing()),
                            "{c:?}: {:?} at {:?} crowds {:?} at {:?}",
                            a.species,
                            a.at,
                            b.species,
                            b.at
                        );
                    }
                }
                natives += native.len();
                extras += planted.len() - native.len();
            }
        }
        println!("round the settlements: {natives} native trees, {extras} extra");
        assert!(natives > 0 && extras > 0, "{natives} {extras}");
    }

    #[test]
    fn every_species_fits_its_spacing_and_collision_to_its_size() {
        for species in [
            Species::Calden,
            Species::Chanar,
            Species::Algarrobo,
            Species::Tala,
            Species::Espinillo,
            Species::Ombu,
            Species::Paraiso,
            Species::Olmo,
            Species::Eucalipto,
            Species::Aromo,
        ] {
            let mut last = (0.0, 0.0);
            for age in Age::ALL {
                let (lo, hi) = age.height_range(species);
                assert!(
                    lo < hi && lo >= last.0 && hi > last.1,
                    "{species:?} {age:?}"
                );
                last = (lo, hi);
                let tree = |height| Tree {
                    at: Vec2::ZERO,
                    species,
                    age,
                    height,
                    width: 1.0,
                    yaw: 0.0,
                    variant: 0,
                };
                // La mayor separación que pide cualquier árbol es lo que buscan
                // los chunks vecinos.
                assert!(
                    tree(hi).spacing() <= Tree::MAX_SPACING,
                    "{species:?} {age:?}"
                );
                assert!(tree(hi).trunk_radius() < tree(hi).crown_radius());
            }
        }
        // El tronco del ombú es el más robusto de todos; el espinillo, el árbol
        // más bajo.
        let trunk = |s: Species| {
            Tree {
                at: Vec2::ZERO,
                species: s,
                age: Age::Mature,
                height: Age::Mature.height_range(s).1,
                width: 1.0,
                yaw: 0.0,
                variant: 0,
            }
            .trunk_radius()
        };
        for s in [Species::Calden, Species::Eucalipto, Species::Algarrobo] {
            assert!(trunk(Species::Ombu) > 1.5 * trunk(s), "{s:?}");
        }
        assert!(Species::Espinillo.height_range().1 <= 5.0);
        assert!(Species::Ombu.height_range().1 >= 15.0);
    }

    /// Un hash de todos los árboles que [`FloraSite::chunk_trees`] planta en
    /// los chunks alrededor de los centros distribuidos y en un barrido
    /// disperso del campo abierto. Se tomó con el recálculo directo por vecino,
    /// así que cualquier atajo en el camino de los extras debe conservarlo.
    #[test]
    fn the_planted_trees_of_the_shipped_world_keep_their_golden_hash() {
        let (flora, _, hubs) = shipped();
        let size = flora.config.chunk_size;
        let mut chunks: Vec<ChunkCoord> = hubs
            .iter()
            .flat_map(|h| flora.config.chunks_near(*h, 3))
            .collect();
        let b = flora.config.bounds();
        let (lo, hi) = (
            ChunkCoord::containing(b.min, size),
            ChunkCoord::containing(b.max, size),
        );
        for y in (lo.y..=hi.y).step_by(7) {
            for x in (lo.x..=hi.x).step_by(7) {
                chunks.push(ChunkCoord::new(x, y));
            }
        }
        chunks.sort();
        chunks.dedup();
        let (mut hash, mut count) = (0xcbf2_9ce4_8422_2325_u64, 0_usize);
        for c in chunks.into_iter().filter(|c| flora.config.has_chunk(*c)) {
            for t in flora.chunk_trees(c, &flora.rail_near(c)) {
                for w in [
                    t.at.x.to_bits(),
                    t.at.y.to_bits(),
                    t.variant,
                    t.species as u32,
                ] {
                    hash = (hash ^ u64::from(w)).wrapping_mul(0x0100_0000_01b3);
                }
                count += 1;
            }
        }
        assert_eq!((count, hash), (12028, 0x85d0_b8bb_c3da_7e5e));
    }

    /// Cargar chunks por streaming a través de una caché chica, y el primero de
    /// nuevo una vez que los desplazó, planta lo mismo que un sitio sin nada en
    /// caché.
    #[test]
    fn the_native_cache_plants_what_a_fresh_site_plants_even_after_eviction() {
        let (flora, _, hubs) = shipped();
        let streamed = flora.clone().with_native_cap(8);
        let chunks: Vec<ChunkCoord> = flora.config.chunks_near(hubs[0], 3);
        for pass in 0..2 {
            for &c in &chunks {
                let fresh = flora.clone().chunk_trees(c, &flora.rail_near(c));
                let cached = streamed.chunk_trees(c, &streamed.rail_near(c));
                assert_eq!(cached, fresh, "{c:?} on pass {pass}");
            }
        }
    }

    #[test]
    fn the_native_cache_keeps_to_its_cap() {
        let (flora, _, hubs) = shipped();
        let streamed = flora.clone().with_native_cap(8);
        for c in flora.config.chunks_near(hubs[0], 3) {
            streamed.chunk_trees(c, &streamed.rail_near(c));
            assert!(streamed.natives.len() <= 8);
        }
        assert_eq!(streamed.natives.len(), 8);
        assert_eq!(flora.clone().natives.len(), 0, "a clone starts empty");
    }
}
