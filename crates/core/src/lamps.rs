//! Faroles de querosén: dónde están en los pueblos y en las estancias.
//!
//! En 1919, un pequeño pueblo de la pampa no tenía alumbrado público eléctrico:
//! un farolero encendía al anochecer faroles de querosén sobre postes de hierro
//! fundido o de madera. Cada pueblo tiene unos pocos alrededor de sus plazas,
//! un par en cada salida de calle por la que sale un camino, y uno en las
//! esquinas: en todas las esquinas (intersecciones) de un pueblo grande y en
//! una de cada dos, en damero, en un pueblo chico. Una estancia tiene uno en
//! cada extremo de la galería de su casco y uno junto a su huella, justo
//! después de la tranquera.
//!
//! La ubicación es determinista y mantiene cada poste fuera de los carriles de
//! circulación (un farol de esquina está en el cordón), de las puertas, de las
//! galerías de los comercios, de los senderos y el monumento de la plaza, de la
//! vía férrea y su playa, de los caminos y de las huellas de los asentamientos
//! y, por último, de todo obstáculo estático del mundo.

use crate::rng::Rng;
use crate::road::segment_distance;
use crate::{
    Aabb, BuildingKind, PLAZA_MONUMENT, PLAZA_PATH_WIDTH, RoadKind, RoadNetwork, Settlement,
    SettlementKind, SettlementLayout, StaticObstacles, StructureKind, TownLayout, TownSize, Track,
    Vec2, plaza_monument, plaza_paths,
};

/// Dónde está un farol, para contarlos y para las decisiones del dibujo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LampSite {
    /// Alrededor de una plaza: junto a la entrada de un sendero, o junto al
    /// monumento.
    Plaza,
    /// Al costado de un camino, donde sale de la cuadrícula de calles.
    Entrance,
    /// En el cordón de una esquina.
    Corner,
    /// En un extremo de la galería de una estancia.
    Homestead,
    /// Junto a la huella de una estancia, justo después de su tranquera.
    Gate,
}

/// Un farol de querosén: un poste con una linterna de vidrio arriba.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreetLamp {
    /// La base del poste.
    pub at: Vec2,
    pub site: LampSite,
}

impl StreetLamp {
    /// Lado de la huella de colisión cuadrada del poste, en metros.
    pub const POST: f32 = 0.24;
    /// Altura del farol, remate incluido, en metros.
    pub const HEIGHT: f32 = 3.35;
    /// Altura del centro del vidrio de la linterna, donde arde la llama.
    pub const FLAME: f32 = 2.8;

    /// Su obstáculo estático: el poste.
    pub fn obstacle(&self) -> Aabb {
        Aabb::from_center_size(self.at, Vec2::new(Self::POST, Self::POST))
    }
}

/// Todos los faroles del mundo: por pueblo y por asentamiento, en el orden de
/// los pueblos del mundo y de [`SettlementLayout::settlements`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreetLamps {
    pub towns: Vec<Vec<StreetLamp>>,
    pub settlements: Vec<Vec<StreetLamp>>,
}

impl StreetLamps {
    /// A qué distancia hacia adentro de ambos bordes de la calle está un farol
    /// de esquina, en metros: en el cordón, fuera del carril de circulación.
    pub const CURB: f32 = 0.7;
    /// Espacio libre mínimo alrededor de un poste, en metros.
    pub const CLEARANCE: f32 = 0.5;
    /// A qué distancia fuera de la salida de un camino desde la cuadrícula está
    /// el par de la entrada.
    pub const ENTRANCE_OUT: f32 = 1.5;
    /// A qué distancia a cada lado de la línea central de un camino está el par
    /// de la entrada: fuera del borde del camino.
    pub const ENTRANCE_SIDE: f32 = RoadNetwork::HALF_WIDTH + 1.0;
    /// A qué distancia hacia adentro del borde de una plaza están sus faroles,
    /// en metros.
    pub const PLAZA_INSET: f32 = 1.0;
    /// Separación entre un farol de plaza y el sendero junto al que está, en
    /// metros.
    pub const PATH_GAP: f32 = 0.7;
    /// Zona prohibida frente a una puerta: esta distancia hacia afuera de la
    /// fachada...
    pub const DOOR_REACH: f32 = 4.0;
    /// ...y esta distancia a cada lado de su centro, en metros.
    pub const DOOR_HALF_WIDTH: f32 = 1.8;
    /// Distancia mínima entre dos faroles, en metros.
    pub const SPACING: f32 = 3.0;
    /// A qué distancia hacia afuera del frente del casco está su par, en
    /// metros.
    pub const GALLERY_OUT: f32 = 1.2;
    /// Separación mínima entre un farol y el surco de una huella, en metros.
    pub const TRACK_GAP: f32 = 0.8;

    /// Los faroles de cada pueblo de `towns` (con los `roads` que salen de
    /// ellos) y de cada estancia de `settlements`, fuera de todos los
    /// rectángulos de `obstacles` (los obstáculos estáticos del mundo, que
    /// todavía no incluyen los faroles).
    pub fn place(
        towns: &[TownLayout],
        roads: &RoadNetwork,
        settlements: &SettlementLayout,
        obstacles: &StaticObstacles,
    ) -> Self {
        let clear = |lamp: &StreetLamp| {
            let reach = lamp.obstacle().expanded(Self::CLEARANCE);
            obstacles.near(reach).iter().all(|o| !o.intersects(&reach))
        };
        Self {
            towns: towns
                .iter()
                .enumerate()
                .map(|(k, t)| town_lamps(t, k, roads).into_iter().filter(clear).collect())
                .collect(),
            settlements: settlements
                .settlements
                .iter()
                .map(|s| settlement_lamps(s).into_iter().filter(clear).collect())
                .collect(),
        }
    }

    /// El poste de cada farol, como obstáculo estático.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.all().map(StreetLamp::obstacle)
    }

    /// Todos los faroles, primero los de los pueblos.
    pub fn all(&self) -> impl Iterator<Item = &StreetLamp> + '_ {
        self.towns.iter().chain(&self.settlements).flatten()
    }

    /// Cuántos faroles hay.
    pub fn count(&self) -> usize {
        self.all().count()
    }
}

/// Los faroles de `town`, el pueblo número `index` del mundo (el que `roads`
/// nombra con ese índice): alrededor de sus plazas, en sus salidas de caminos y
/// en sus esquinas, cada uno fuera de las zonas prohibidas del pueblo y lejos
/// de los demás.
pub fn town_lamps(town: &TownLayout, index: usize, roads: &RoadNetwork) -> Vec<StreetLamp> {
    let mut keep = keep_outs(town);
    keep.extend(town.obstacles());
    let mut lighter = Lamplighter {
        keep,
        roads: Some(roads),
        tracks: &[],
        lamps: Vec::new(),
    };
    plaza_lamps(town, &mut lighter);
    entrance_lamps(town, index, roads, &mut lighter);
    corner_lamps(town, &mut lighter);
    lighter.lamps
}

/// Los faroles del asentamiento `s`: el par de una estancia en cada extremo de
/// la galería de su casco y uno junto a su huella, justo después de la
/// tranquera; ninguno para los tipos más pobres.
pub fn settlement_lamps(s: &Settlement) -> Vec<StreetLamp> {
    if s.kind != SettlementKind::Ranch {
        return Vec::new();
    }
    let mut lighter = Lamplighter {
        keep: s.structures.iter().flat_map(|st| st.obstacles()).collect(),
        roads: None,
        tracks: &s.tracks,
        lamps: Vec::new(),
    };
    let casco = s
        .structures
        .iter()
        .find(|st| matches!(st.kind, StructureKind::MainHouse(_)));
    if let Some(casco) = casco {
        let n = crate::places::side_normal(casco.front);
        let across = Vec2::new(-n.y, n.x);
        let (facade, wide) = facade(casco.footprint, n);
        for side in [-1.0, 1.0] {
            let at = facade + n * StreetLamps::GALLERY_OUT + across * (side * (wide - 0.8));
            lighter.put(at, LampSite::Homestead);
        }
    }
    if let Some(&[a, b, ..]) = s.tracks.first().map(|t| t.points.as_slice())
        && let Some(along) = unit(b - a)
    {
        let across = Vec2::new(-along.y, along.x);
        for side in [1.0, -1.0] {
            let at = a + along * 0.5 + across * (side * (Track::HALF_WIDTH + 1.2));
            if lighter.put(at, LampSite::Gate) {
                break;
            }
        }
    }
    lighter.lamps
}

/// Coloca los faroles de a uno, cada uno solo donde entra.
struct Lamplighter<'a> {
    /// Rectángulos a los que ningún poste puede acercarse.
    keep: Vec<Aabb>,
    roads: Option<&'a RoadNetwork>,
    tracks: &'a [Track],
    lamps: Vec<StreetLamp>,
}

impl Lamplighter<'_> {
    /// Si un poste en `at` mantiene su espacio libre respecto de toda zona
    /// prohibida, camino y huella, y su separación respecto de los faroles ya
    /// colocados.
    fn fits(&self, at: Vec2) -> bool {
        let reach = StreetLamp {
            at,
            site: LampSite::Corner,
        }
        .obstacle()
        .expanded(StreetLamps::CLEARANCE);
        let lane = RoadNetwork::HALF_WIDTH + StreetLamps::CLEARANCE;
        let rut = Track::HALF_WIDTH + StreetLamps::TRACK_GAP;
        self.keep.iter().all(|k| !k.intersects(&reach))
            && self
                .roads
                .is_none_or(|r| r.near(at, lane + 1.0).is_none_or(|d| d >= lane))
            && self
                .tracks
                .iter()
                .flat_map(Track::segments)
                .all(|(a, b)| segment_distance(at, a, b) >= rut)
            && self
                .lamps
                .iter()
                .all(|l| l.at.distance(at) >= StreetLamps::SPACING)
    }

    /// Coloca un farol en `at` si entra; devuelve si lo hizo.
    fn put(&mut self, at: Vec2, site: LampSite) -> bool {
        let fits = self.fits(at);
        if fits {
            self.lamps.push(StreetLamp { at, site });
        }
        fits
    }
}

/// El centro de la fachada de `footprint` que mira hacia `n`, y la mitad de su
/// ancho.
fn facade(footprint: Aabb, n: Vec2) -> (Vec2, f32) {
    let (c, half) = (footprint.center(), footprint.size() * 0.5);
    let across = Vec2::new(-n.y, n.x);
    (
        c + n * (n.x.abs() * half.x + n.y.abs() * half.y),
        across.x.abs() * half.x + across.y.abs() * half.y,
    )
}

fn unit(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length > 1e-6).then(|| v * (1.0 / length))
}

/// Las cuatro salidas de un rectángulo: este, norte, oeste y sur.
const SIDES: [Vec2; 4] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(0.0, -1.0),
];

/// Qué tan generosamente está iluminado un pueblo, según su tamaño (prueba de
/// juego del usuario, 2026-10-23: las plazas chicas tenían demasiados faroles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LampTier {
    /// Un pueblito o un apeadero de como mucho [`LampTier::VERY_SMALL_BLOCKS`]
    /// manzanas (Metileo): dos faroles de plaza, y esquinas solo alrededor de
    /// la plaza y junto a la estación.
    VerySmall,
    /// Un pueblo chico ([`TownSize::Small`]): cuatro faroles de plaza, una
    /// esquina de cada dos.
    Small,
    /// Un pueblo grande: cuatro faroles junto a los senderos de cada plaza,
    /// cuatro más alrededor del monumento principal, todas las esquinas.
    Big,
}

impl LampTier {
    /// Máximo de manzanas de un pueblo muy chico: la cuadrícula más chica, de
    /// 3 x 3.
    pub const VERY_SMALL_BLOCKS: usize = 9;

    /// La categoría de `town`.
    pub fn of(town: &TownLayout) -> Self {
        let (w, h) = town.blocks();
        match town.size {
            _ if w * h <= Self::VERY_SMALL_BLOCKS => Self::VerySmall,
            TownSize::Small => Self::Small,
            TownSize::Big | TownSize::VeryBig => Self::Big,
        }
    }
}

/// El eje unitario que más apunta desde `from` hacia `to`: este, norte, oeste o
/// sur.
fn axis_toward(from: Vec2, to: Vec2) -> Vec2 {
    let d = to - from;
    SIDES
        .into_iter()
        .max_by(|a, b| a.dot(d).total_cmp(&b.dot(d)))
        .unwrap_or(SIDES[1])
}

/// El centro de la estación del pueblo, si tiene una.
fn station(town: &TownLayout) -> Option<Vec2> {
    town.buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Station)
        .map(|b| b.footprint.center())
}

/// Alrededor de las plazas, según [`LampTier`]: uno junto a la entrada de cada
/// sendero, del lado derecho al entrar caminando (en un pueblo muy chico, solo
/// en las dos entradas del sendero que va hacia su estación, o en las de
/// norte-sur si no tiene); en un pueblo grande, cuatro más alrededor del
/// monumento de la plaza principal, uno en cada esquina del cantero.
fn plaza_lamps(town: &TownLayout, lighter: &mut Lamplighter) {
    let tier = LampTier::of(town);
    let beside = PLAZA_PATH_WIDTH.max(PLAZA_MONUMENT) / 2.0 + StreetLamps::PATH_GAP;
    for (k, p) in town.plazas().enumerate() {
        let (c, half) = (p.center(), p.size() * 0.5);
        let main = station(town).map_or(SIDES[1], |s| axis_toward(c, s));
        for n in SIDES {
            if tier == LampTier::VerySmall && n.dot(main).abs() < 0.5 {
                continue;
            }
            let inward = n.x.abs() * half.x + n.y.abs() * half.y - StreetLamps::PLAZA_INSET;
            let left = Vec2::new(-n.y, n.x);
            lighter.put(c + n * inward + left * beside, LampSite::Plaza);
        }
        if k == 0 && tier == LampTier::Big {
            for d in SIDES {
                let corner = d + Vec2::new(-d.y, d.x);
                lighter.put(c + corner * beside, LampSite::Plaza);
            }
        }
    }
}

/// Un par en cada salida de la cuadrícula por la que un camino (no una calle
/// que cruza la vía férrea) sale de `town`, uno a cada lado del camino, justo
/// afuera.
fn entrance_lamps(town: &TownLayout, index: usize, roads: &RoadNetwork, lighter: &mut Lamplighter) {
    let grid = town.grid();
    let mut exits: Vec<crate::RoadEnd> = Vec::new();
    for road in roads.roads.iter().filter(|r| r.kind != RoadKind::Street) {
        for (t, end) in [(road.from, road.ends[0]), (road.to, road.ends[1])] {
            if t == index
                && on_edge(grid, end.at)
                && exits.iter().all(|e| e.at.distance(end.at) > 1.0)
            {
                exits.push(end);
            }
        }
    }
    for end in exits {
        let across = Vec2::new(-end.out.y, end.out.x);
        for side in [1.0, -1.0] {
            let at = end.at
                + end.out * StreetLamps::ENTRANCE_OUT
                + across * (side * StreetLamps::ENTRANCE_SIDE);
            lighter.put(at, LampSite::Entrance);
        }
    }
}

/// Un farol en el cordón de una esquina: en todas las esquinas de un pueblo
/// grande, en una de cada dos (en damero) en un pueblo chico y, en uno muy
/// chico, solo en las esquinas de la plaza y en las dos esquinas más cercanas a
/// la estación. La esquina elegida por la semilla entre las cuatro se prueba
/// primero; si no entra, se prueba la siguiente.
fn corner_lamps(town: &TownLayout, lighter: &mut Lamplighter) {
    let on_street = |p: Vec2| town.streets.iter().any(|s| s.contains_point(p));
    let lines = |across: fn(&Aabb) -> bool, key: fn(Vec2) -> f32| {
        let mut lines: Vec<f32> = town
            .streets
            .iter()
            .filter(|s| across(s))
            .map(|s| key(s.center()))
            .collect();
        lines.sort_by(f32::total_cmp);
        lines.dedup();
        lines
    };
    let xs = lines(|s| s.size().y > s.size().x, |c| c.x);
    let ys = lines(|s| s.size().x > s.size().y, |c| c.y);
    let width = town
        .streets
        .iter()
        .map(|s| s.size().x.min(s.size().y))
        .fold(f32::INFINITY, f32::min);
    let tier = LampTier::of(town);
    let (grid, band) = (town.grid(), town.rail_band());
    let crossings: Vec<Vec2> = ys
        .iter()
        .flat_map(|&y| xs.iter().map(move |&x| Vec2::new(x, y)))
        .filter(|&p| on_street(p))
        .collect();
    // Los de un pueblo muy chico: alrededor de su plaza y las dos más cercanas
    // a su estación.
    let by_plaza = town.plaza.expanded(width + 0.1);
    let mut by_station = crossings.clone();
    if let Some(s) = station(town) {
        by_station.sort_by(|a, b| a.distance(s).total_cmp(&b.distance(s)));
        by_station.truncate(2);
    } else {
        by_station.clear();
    }
    for (j, &y) in ys.iter().enumerate() {
        for (i, &x) in xs.iter().enumerate() {
            let p = Vec2::new(x, y);
            let lit = match tier {
                LampTier::Big => true,
                LampTier::Small => (i + j) % 2 == 0,
                LampTier::VerySmall => by_plaza.contains_point(p) || by_station.contains(&p),
            };
            if !on_street(p) || !lit {
                continue;
            }
            let first = Rng::new(town.seed ^ 0x1a4b, (j * xs.len() + i) as u32).below(4);
            for k in 0..4 {
                let d = SIDES[(first + k) % 4];
                let diagonal = d + Vec2::new(-d.y, d.x);
                let block = p + diagonal * (width / 2.0 + 0.5);
                if on_street(block)
                    || !grid.contains_point(block)
                    || band.is_some_and(|b| b.contains_point(block))
                {
                    continue;
                }
                let at = p + diagonal * (width / 2.0 - StreetLamps::CURB);
                if lighter.put(at, LampSite::Corner) {
                    break;
                }
            }
        }
    }
}

/// Si `p` está en el borde de `grid`, con una tolerancia de medio metro.
fn on_edge(grid: Aabb, p: Vec2) -> bool {
    let d = [
        p.x - grid.min.x,
        grid.max.x - p.x,
        p.y - grid.min.y,
        grid.max.y - p.y,
    ];
    d.iter().all(|&d| d > -0.5) && d.iter().any(|d| d.abs() < 0.5)
}

/// El rectángulo entre dos esquinas `a` y `b`, en cualquier orden.
fn between(a: Vec2, b: Vec2) -> Aabb {
    Aabb::new(
        Vec2::new(a.x.min(b.x), a.y.min(b.y)),
        Vec2::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

/// Aquello de lo que se mantienen lejos los faroles de un pueblo además de sus
/// obstáculos: el terreno frente a cada puerta, la galería de cada comercio,
/// los andenes y sus techos, la vía férrea y su playa, y los senderos y el
/// monumento de cada plaza.
fn keep_outs(town: &TownLayout) -> Vec<Aabb> {
    let mut keep = Vec::new();
    for b in &town.buildings {
        if b.kind == BuildingKind::Platform {
            continue;
        }
        let n = crate::places::side_normal(b.front);
        let across = Vec2::new(-n.y, n.x);
        let (facade, wide) = facade(b.footprint, n);
        keep.push(between(
            facade + across * StreetLamps::DOOR_HALF_WIDTH,
            facade + n * StreetLamps::DOOR_REACH - across * StreetLamps::DOOR_HALF_WIDTH,
        ));
        if b.kind == BuildingKind::GeneralStore {
            let deep = town.hitching_post.veranda + 0.3;
            keep.push(between(
                facade + across * (wide + 0.3),
                facade + n * deep - across * (wide + 0.3),
            ));
        }
    }
    keep.extend(town.floors().iter().map(|f| f.footprint));
    keep.extend(town.canopies().iter().map(|c| c.footprint));
    keep.extend(town.rail_corridor());
    keep.extend(town.rail_band());
    for p in town.plazas() {
        keep.extend(plaza_paths(p));
        keep.push(plaza_monument(p));
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::road::tests::shipped_roads;

    fn shipped() -> (Vec<TownLayout>, RoadNetwork, Vec<Vec<StreetLamp>>) {
        let (towns, _, roads) = shipped_roads();
        let lamps = towns
            .iter()
            .enumerate()
            .map(|(k, t)| town_lamps(t, k, &roads))
            .collect();
        (towns, roads, lamps)
    }

    fn of(lamps: &[StreetLamp], site: LampSite) -> Vec<StreetLamp> {
        lamps.iter().copied().filter(|l| l.site == site).collect()
    }

    /// Intersecciones de calles de `town`: donde una calle norte-sur cruza una
    /// este-oeste.
    fn intersections(town: &TownLayout) -> Vec<Vec2> {
        let on = |p: Vec2| town.streets.iter().any(|s| s.contains_point(p));
        let mut xs: Vec<f32> = town
            .streets
            .iter()
            .filter(|s| s.size().y > s.size().x)
            .map(|s| s.center().x)
            .collect();
        let mut ys: Vec<f32> = town
            .streets
            .iter()
            .filter(|s| s.size().x > s.size().y)
            .map(|s| s.center().y)
            .collect();
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        ys.sort_by(f32::total_cmp);
        ys.dedup();
        ys.iter()
            .flat_map(|&y| xs.iter().map(move |&x| Vec2::new(x, y)))
            .filter(|&p| on(p))
            .collect()
    }

    #[test]
    fn plaza_lamps_scale_with_the_town() {
        let (towns, _, lamps) = shipped();
        let mut tiers = Vec::new();
        for (town, lamps) in towns.iter().zip(&lamps) {
            let plaza = of(lamps, LampSite::Plaza);
            let tier = LampTier::of(town);
            tiers.push(tier);
            let main = plaza
                .iter()
                .filter(|l| town.plaza.contains_point(l.at))
                .count();
            println!("{} ({tier:?}): {main} lamps in the main plaza", town.name);
            match tier {
                // Uno junto a la entrada de cada sendero, ninguno alrededor del
                // monumento.
                LampTier::Small => assert_eq!(main, 4, "{}", town.name),
                // Cuatro más alrededor del monumento; cuatro en la plaza de la
                // estación.
                LampTier::Big => {
                    assert_eq!(main, 8, "{}", town.name);
                    let station = town.station_plaza.expect("a big town's station plaza");
                    let there = plaza.iter().filter(|l| station.contains_point(l.at));
                    assert_eq!(there.count(), 4, "{}", town.name);
                }
                LampTier::VerySmall => unreachable!("no village on the shipped fixture"),
            }
            assert_eq!(
                plaza.len(),
                town.plazas()
                    .map(|p| plaza.iter().filter(|l| p.contains_point(l.at)).count())
                    .sum::<usize>(),
                "every plaza lamp stands in a plaza"
            );
        }
        assert!(tiers.contains(&LampTier::Small) && tiers.contains(&LampTier::Big));
    }

    /// Metileo tal como lo distribuye `assets/world/towns.ron`: un apeadero de
    /// 3 x 3.
    fn metileo() -> TownLayout {
        TownLayout::generate(&crate::TownConfig {
            name: "Metileo".into(),
            origin: Vec2::ZERO,
            blocks: (3, 3),
            block_size: 40.0,
            street_width: 12.0,
            railway: Some(crate::RailwayConfig {
                side: crate::Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: crate::PlazaPlacement::Auto,
            seed: 19,
            police_stations: Some(0),
            density: Some(0.3),
            townsfolk: None,
        })
        .expect("the shipped Metileo is valid")
    }

    #[test]
    fn a_village_gets_two_plaza_lamps_and_corners_only_by_the_plaza_and_station() {
        let town = metileo();
        assert_eq!(LampTier::of(&town), LampTier::VerySmall);
        let lamps = town_lamps(&town, 0, &RoadNetwork::new(Vec::new(), &[]));
        let plaza = of(&lamps, LampSite::Plaza);
        assert_eq!(plaza.len(), 2, "{plaza:?}");
        // En las dos entradas del sendero hacia la estación: sobre una misma
        // línea.
        let c = town.plaza.center();
        let station = station(&town).unwrap();
        let axis = axis_toward(c, station);
        for l in &plaza {
            assert!((l.at - c).dot(axis).abs() > town.plaza.size().x / 2.0 - 2.0);
        }
        let corners = of(&lamps, LampSite::Corner);
        let all = intersections(&town).len();
        println!(
            "Metileo: {} plaza, {} corner lamps of {all} corners",
            plaza.len(),
            corners.len()
        );
        assert!((2..=6).contains(&corners.len()), "{}", corners.len());
        let near = town.plaza.expanded(12.0);
        for l in &corners {
            assert!(
                near.contains_point(l.at) || l.at.distance(station) < 40.0,
                "{l:?} is neither by the plaza nor by the station"
            );
        }
    }

    #[test]
    fn every_road_out_of_a_town_leaves_between_a_pair_of_lamps() {
        let (towns, roads, lamps) = shipped();
        for (k, (town, lamps)) in towns.iter().zip(&lamps).enumerate() {
            let entrance = of(lamps, LampSite::Entrance);
            let exits: Vec<Vec2> = roads
                .roads
                .iter()
                .filter(|r| r.kind != RoadKind::Street)
                .flat_map(|r| [(r.from, r.ends[0]), (r.to, r.ends[1])])
                .filter(|&(t, end)| t == k && on_edge(town.grid(), end.at))
                .map(|(_, end)| end.at)
                .collect();
            assert!(!exits.is_empty(), "{}", town.name);
            println!(
                "{}: {} exits, {} lamps",
                town.name,
                exits.len(),
                entrance.len()
            );
            for at in exits {
                let near = entrance.iter().filter(|l| l.at.distance(at) < 6.0).count();
                assert_eq!(near, 2, "{} exit {at:?}", town.name);
            }
        }
    }

    #[test]
    fn big_towns_light_every_corner_and_small_towns_some() {
        let (towns, _, lamps) = shipped();
        let mut tiers = 0;
        for (town, lamps) in towns.iter().zip(&lamps) {
            let corners = intersections(town);
            // Dentro del cuadrado donde se cruzan las dos calles.
            let half = town
                .streets
                .iter()
                .map(|s| s.size().x.min(s.size().y))
                .fold(f32::INFINITY, f32::min)
                / 2.0;
            let lit = corners
                .iter()
                .filter(|&&c| {
                    lamps.iter().any(|l| {
                        let d = l.at - c;
                        l.site == LampSite::Corner && d.x.abs() < half && d.y.abs() < half
                    })
                })
                .count();
            let share = lit as f32 / corners.len() as f32;
            println!(
                "{} ({:?}): {lit} of {} corners lit",
                town.name,
                town.size,
                corners.len()
            );
            match town.size {
                TownSize::Small => {
                    tiers |= 1;
                    assert!((0.25..=0.6).contains(&share), "{}: {share}", town.name);
                }
                TownSize::Big | TownSize::VeryBig => {
                    tiers |= 2;
                    // Una esquina cuyos cuatro cordones están todos ocupados
                    // (una playa, una galería, una puerta) se queda sin farol.
                    assert!(share >= 0.9, "{}: {share}", town.name);
                }
            }
            assert_eq!(
                of(lamps, LampSite::Corner).len(),
                lit,
                "one lamp per lit corner"
            );
        }
        assert_eq!(tiers, 3, "the shipped line has small and big towns");
    }

    #[test]
    fn town_lamps_keep_off_lanes_doors_verandas_paths_and_the_railway() {
        let (towns, roads, lamps) = shipped();
        for (town, lamps) in towns.iter().zip(&lamps) {
            let keep = keep_outs(town);
            let obstacles = town.obstacles();
            for lamp in lamps {
                let post = lamp.obstacle();
                for k in &keep {
                    assert!(!post.intersects(k), "{} {lamp:?} in {k:?}", town.name);
                }
                for o in &obstacles {
                    assert!(!post.intersects(o), "{} {lamp:?} on {o:?}", town.name);
                }
                // En una calle, solo en su cordón, nunca en el carril.
                for s in town.streets.iter().filter(|s| s.contains_point(lamp.at)) {
                    let (lo, hi) = (lamp.at - s.min, s.max - lamp.at);
                    let edge = if s.size().x > s.size().y {
                        lo.y.min(hi.y)
                    } else {
                        lo.x.min(hi.x)
                    };
                    assert!(edge <= 1.0, "{} {lamp:?} in the lane of {s:?}", town.name);
                }
                assert!(
                    roads.distance(lamp.at) >= RoadNetwork::HALF_WIDTH + 0.5,
                    "{} {lamp:?} on a road",
                    town.name
                );
                for p in town.plazas() {
                    for path in plaza_paths(p).iter().chain([&plaza_monument(p)]) {
                        assert!(!post.intersects(path), "{} {lamp:?}", town.name);
                    }
                }
            }
            for (i, a) in lamps.iter().enumerate() {
                for b in &lamps[i + 1..] {
                    assert!(a.at.distance(b.at) >= StreetLamps::SPACING, "{a:?} {b:?}");
                }
            }
        }
    }

    #[test]
    fn palenques_cost_a_town_no_lamp() {
        // Los faroles se mantienen lejos de todo obstáculo: un palenque en el
        // lugar de un farol lo eliminaría. El mismo pueblo sin sus palenques
        // queda iluminado igual.
        let (towns, roads, lamps) = shipped();
        let mut with_posts = 0;
        for (k, (town, lit)) in towns.iter().zip(&lamps).enumerate() {
            let mut bare = town.clone();
            bare.palenques.clear();
            with_posts += usize::from(!town.palenques.is_empty());
            assert_eq!(&town_lamps(&bare, k, &roads), lit, "{}", town.name);
        }
        assert!(with_posts > 0, "the shipped towns have palenques");
    }

    #[test]
    fn shade_trees_cost_a_town_no_lamp() {
        // Los árboles se mantienen lejos de los lugares de los faroles (los
        // cruces, el borde de la plaza, las puertas): el mismo pueblo sin sus
        // árboles queda iluminado igual.
        let (towns, roads, lamps) = shipped();
        let mut with_trees = 0;
        for (k, (town, lit)) in towns.iter().zip(&lamps).enumerate() {
            let mut bare = town.clone();
            bare.shade_trees.clear();
            with_trees += town.shade_trees.len();
            assert_eq!(&town_lamps(&bare, k, &roads), lit, "{}", town.name);
            // Y un poste y un tronco quedan a un palmo de distancia.
            for lamp in lit {
                for tree in &town.shade_trees {
                    let gap = lamp.at.distance(tree.at);
                    assert!(gap >= 1.0, "{}: a tree {gap} m from {lamp:?}", town.name);
                }
            }
        }
        assert!(with_trees > 100, "the shipped towns have shade trees");
    }

    #[test]
    fn placement_is_deterministic() {
        let (_, _, a) = shipped();
        let (_, _, b) = shipped();
        assert_eq!(a, b);
    }

    #[test]
    fn estancias_have_lamps_clear_of_their_buildings_and_tracks() {
        let (_, _, _, _, layout) = crate::settlement::tests::settled();
        let lamps = StreetLamps {
            towns: Vec::new(),
            settlements: layout.settlements.iter().map(settlement_lamps).collect(),
        };
        let mut ranches = 0;
        for (s, lamps) in layout.settlements.iter().zip(&lamps.settlements) {
            if s.kind != SettlementKind::Ranch {
                assert!(lamps.is_empty(), "{:?}", s.kind);
                continue;
            }
            ranches += 1;
            assert_eq!(of(lamps, LampSite::Homestead).len(), 2, "{lamps:?}");
            assert_eq!(of(lamps, LampSite::Gate).len(), 1, "{lamps:?}");
            for lamp in lamps {
                let post = lamp.obstacle();
                for o in s.structures.iter().flat_map(|st| st.obstacles()) {
                    assert!(!post.expanded(0.3).intersects(&o), "{lamp:?} on {o:?}");
                }
                for (a, b) in s.tracks.iter().flat_map(Track::segments) {
                    let gap = segment_distance(lamp.at, a, b);
                    assert!(gap >= Track::HALF_WIDTH + 0.5, "{lamp:?} on a track");
                }
            }
        }
        assert!(ranches >= 2, "the fixture has its two estancias");
        assert_eq!(
            lamps,
            StreetLamps {
                towns: Vec::new(),
                settlements: layout.settlements.iter().map(settlement_lamps).collect(),
            }
        );
    }

    /// Las celdas de una grilla de paso `step` sobre `area` a las que alguien
    /// de radio `radius` llega a pie desde `from`, sin pasar por `blocked`.
    fn reachable(area: Aabb, blocked: &[Aabb], radius: f32, from: Vec2) -> Vec<Vec2> {
        let step = 0.25;
        let (w, h) = (
            (area.size().x / step) as usize + 1,
            (area.size().y / step) as usize + 1,
        );
        let at = |i: usize, j: usize| area.min + Vec2::new(i as f32, j as f32) * step;
        let free = |p: Vec2| {
            blocked
                .iter()
                .all(|o| o.closest_point(p).distance(p) > radius)
        };
        let mut seen = vec![false; w * h];
        let start = (
            ((from.x - area.min.x) / step).round() as usize,
            ((from.y - area.min.y) / step).round() as usize,
        );
        assert!(free(at(start.0, start.1)), "{from:?} is blocked");
        let mut todo = vec![start];
        seen[start.1 * w + start.0] = true;
        let mut out = Vec::new();
        while let Some((i, j)) = todo.pop() {
            out.push(at(i, j));
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= w as i64 || nj >= h as i64 {
                    continue;
                }
                let (ni, nj) = (ni as usize, nj as usize);
                if !seen[nj * w + ni] && free(at(ni, nj)) {
                    seen[nj * w + ni] = true;
                    todo.push((ni, nj));
                }
            }
        }
        out
    }

    #[test]
    fn plaza_furniture_keeps_every_plaza_lamp_and_the_paths_walkable() {
        // Los pueblos distribuidos con todos sus faroles, y los pueblos de
        // ejemplo (entre ellos un pueblito) con sus faroles de plaza.
        let (shipped, _, lamps) = shipped();
        let samples = crate::town::tests::sample_towns();
        let plaza_only = samples.iter().map(|t| {
            let mut keep = keep_outs(t);
            keep.extend(t.obstacles());
            let mut lighter = Lamplighter {
                keep,
                roads: None,
                tracks: &[],
                lamps: Vec::new(),
            };
            plaza_lamps(t, &mut lighter);
            lighter.lamps
        });
        let towns: Vec<(&TownLayout, Vec<StreetLamp>)> = shipped
            .iter()
            .zip(lamps)
            .chain(samples.iter().zip(plaza_only))
            .collect();
        let mut tiers = std::collections::HashSet::new();
        for (town, lamps) in &towns {
            tiers.insert(LampTier::of(town));
            // Cada farol de plaza sigue donde estaba antes del mobiliario:
            // junto a cada entrada, y alrededor del monumento de una ciudad.
            let plaza = of(lamps, LampSite::Plaza);
            let main = plaza
                .iter()
                .filter(|l| town.plaza.contains_point(l.at))
                .count();
            let want = match LampTier::of(town) {
                LampTier::VerySmall => 2,
                LampTier::Small => 4,
                LampTier::Big => 8,
            };
            assert_eq!(main, want, "{}", town.name);
            // Y lejos de todo obstáculo, incluido el del mobiliario.
            let obstacles = town.obstacles();
            for l in &plaza {
                let reach = l.obstacle().expanded(StreetLamps::CLEARANCE);
                assert!(obstacles.iter().all(|o| !o.intersects(&reach)), "{l:?}");
            }
            let mut blocked = obstacles;
            blocked.extend(lamps.iter().map(StreetLamp::obstacle));
            for (k, f) in town.plaza_furniture().into_iter().enumerate() {
                let (c, half) = (f.area.center(), f.area.size() * 0.5);
                // Un peatón y un jinete, desde la entrada de un sendero justo
                // afuera de la plaza, llegan a las otras tres y rodean el
                // monumento caminando; el peatón llega al punto de encuentro de
                // la plaza.
                let mouths: Vec<Vec2> = SIDES
                    .iter()
                    .map(|&n| c + n * (n.x.abs() * half.x + n.y.abs() * half.y + 0.5))
                    .collect();
                let around: Vec<Vec2> = SIDES
                    .iter()
                    .map(|&n| c + n * (PLAZA_MONUMENT / 2.0 + 1.5))
                    .collect();
                for radius in [crate::Rider::PERSON_RADIUS, crate::Rider::COLLIDER_RADIUS] {
                    let cells = reachable(f.area.expanded(1.0), &blocked, radius, mouths[0]);
                    let near = |p: &Vec2| cells.iter().any(|q| q.distance(*p) < 0.3);
                    for p in mouths.iter().chain(&around) {
                        assert!(
                            near(p),
                            "{} plaza {k}: {p:?} out of reach at {radius}",
                            town.name
                        );
                    }
                    if k == 0 && radius < 0.5 {
                        let (spot, _) = town.plaza_spot();
                        assert!(near(&spot), "{}: the plaza spot", town.name);
                    }
                }
            }
        }
        assert_eq!(tiers.len(), 3, "{tiers:?}");
    }

    #[test]
    fn placing_keeps_off_the_world_and_counts_every_lamp() {
        let (towns, _, roads) = shipped_roads();
        let mut blocked = StaticObstacles::new(Vec::new());
        let (_, _, free) = shipped();
        let victim = free[0][0];
        blocked.push(victim.obstacle());
        let placed = StreetLamps::place(&towns, &roads, &SettlementLayout::default(), &blocked);
        assert!(!placed.towns[0].contains(&victim));
        assert_eq!(placed.towns[0].len(), free[0].len() - 1);
        assert_eq!(placed.count(), placed.obstacles().count());
        assert_eq!(placed.count(), free.iter().map(Vec::len).sum::<usize>() - 1);
    }
}
