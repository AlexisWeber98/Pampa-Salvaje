//! Construye el mundo y sus pueblos a partir de `assets/world/towns.ron`.
//!
//! Los pueblos se necesitan antes de que arranque la app (el terreno nivelado,
//! la gente y los trenes salen de ellos), cuando el log todavía no está listo,
//! así que los problemas se guardan en [`TownReport`] y se registran al inicio.

use std::path::Path;

use bevy::prelude::*;
use pampa_content::load_towns_or_builtin;
use pampa_core::{
    Lagoon, PlaceKind, RailLine, RoadNetwork, Terrain, TownLayout, WorldConfig, place_lagoons,
    place_towns,
};
use pampa_render::lettering::{self, Face};

/// El mundo a construir, sus pueblos, y lo que salió mal al leerlos.
#[derive(Debug, Clone)]
pub struct LoadedWorld {
    pub world: WorldConfig,
    /// Cada pueblo válido, en el orden del archivo; nunca vacío. El jugador
    /// empieza en el primero.
    pub towns: Vec<TownLayout>,
    /// La línea del ferrocarril que pasa por los pueblos, con sus tramos
    /// comprobados contra ellos.
    pub line: RailLine,
    /// Los caminos de tierra entre pueblos vecinos, y la huella rural de cada
    /// pueblo ferroviario a través de su paso a nivel.
    pub roads: RoadNetwork,
    /// Las lagunas, apartadas de los pueblos, del ferrocarril y de los caminos,
    /// con su agua ajustada contra el terreno por defecto.
    pub lagoons: Vec<Lagoon>,
    /// Problemas encontrados, listos para registrarse como avisos.
    pub warnings: Vec<String>,
}

/// Carga el archivo de pueblos en `path` y ubica cada pueblo válido en su
/// mundo, recurriendo al Eduardo Castex incorporado cuando no se puede
/// construir ningún pueblo; luego tiende la línea del ferrocarril y los caminos
/// entre ellos y coloca las lagunas apartadas de todo.
pub fn load_world(path: &Path) -> LoadedWorld {
    let (content, errors) = load_towns_or_builtin(path);
    let mut warnings: Vec<String> = errors.iter().map(|e| format!("towns: {e}")).collect();
    let mut world = content.world;
    let mut names = content.line;
    let mut branches = content.branches;
    // Content ya ubicó estos pueblos una vez; esto solo los construye.
    let (mut towns, rejected) = place_towns(&world, &content.towns);
    warnings.extend(rejected.iter().map(|e| format!("towns: {e}")));
    if towns.is_empty() {
        // La línea del archivo nombra pueblos que ya no están.
        names = None;
        branches.clear();
        warnings.push(
            "towns: no town fits the world; using the built-in Eduardo Castex in the default world"
                .to_string(),
        );
        world = WorldConfig::default();
        towns = vec![TownLayout::castex_inspired()];
    }
    for town in &towns {
        warnings.extend(unsupported_glyphs(town));
    }
    let (mut line, broken) =
        RailLine::build_with_branches(names.as_deref(), &branches, &towns, world.bounds());
    warnings.extend(broken.iter().map(|e| format!("towns: {e}")));
    let (roads, left_out) = RoadNetwork::build(&towns, &line, world.bounds());
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    let (lagoons, left_out) = place_lagoons(
        &world,
        &Terrain::default(),
        &towns,
        &line,
        &roads,
        &content.lagoons,
    );
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    // Un camino público hasta cada laguna desde el pueblo más cercano.
    let (roads, left_out) = roads.with_lagoons(&towns, &line, world.bounds(), &lagoons);
    warnings.extend(left_out.iter().map(|e| format!("towns: {e}")));
    // Los senderos desde los andenes de las vías muertas hasta el cruce del
    // camino junto a la estación.
    line.lay_walks(&towns, &roads);
    LoadedWorld {
        world,
        towns,
        line,
        roads,
        lagoons,
        warnings,
    }
}

/// Un aviso por cada cartel de estación de `town` que las tipografías no pueden
/// dibujar por completo.
fn unsupported_glyphs(town: &TownLayout) -> Vec<String> {
    let stations = town
        .places()
        .into_iter()
        .filter(|p| p.kind == PlaceKind::Station);
    let mut warnings = Vec::new();
    for place in stations {
        let mut unsupported: Vec<char> = Vec::new();
        for c in place.name.chars().filter(|&c| {
            !(lettering::supports(Face::Body, c) && lettering::supports(Face::Display, c))
        }) {
            if !unsupported.contains(&c) {
                unsupported.push(c);
            }
        }
        if !unsupported.is_empty() {
            warnings.push(format!(
                "towns: the station sign \"{}\" uses characters the typefaces cannot draw: {unsupported:?}",
                place.name
            ));
        }
    }
    warnings
}

/// Lo que informó la carga de los pueblos, registrado una sola vez al inicio.
#[derive(Resource, Debug, Clone, Default)]
pub struct TownReport {
    pub names: Vec<String>,
    pub warnings: Vec<String>,
}

impl LoadedWorld {
    /// El terreno por defecto con cada laguna hundida en él, antes de nivelar
    /// los pueblos.
    pub fn terrain(&self) -> Terrain {
        self.lagoons
            .iter()
            .fold(Terrain::default(), |t, l| t.with_lagoon(l.clone()))
    }
}

impl TownReport {
    pub fn new(loaded: &LoadedWorld) -> Self {
        Self {
            names: loaded.towns.iter().map(|t| t.name.clone()).collect(),
            warnings: loaded.warnings.clone(),
        }
    }
}

/// Registra los pueblos que se construyeron y cualquier problema al cargarlos.
pub fn log_town_report(report: Res<TownReport>) {
    for warning in &report.warnings {
        warn!("{warning}");
    }
    info!(
        "towns: {} built: {}",
        report.names.len(),
        report.names.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::town::Walk;
    use pampa_core::{BuildingKind, Terrain, TownSize, Townsfolk, Vec2, World};

    fn temp_towns(name: &str, text: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pampa-game-towns-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("towns.ron");
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_shipped_file_builds_monte_nievas_first_without_warnings() {
        let loaded = crate::shipped_world::loaded();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.world.half_extent, WorldConfig::MAX_HALF_EXTENT);
        let names: Vec<&str> = loaded.towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Monte Nievas",
                "Eduardo Castex",
                "Metileo",
                "General Pico",
                "Caleufú"
            ]
        );
        // General Pico es muy grande: una plaza de 2 x 2, una plaza de la
        // estación, dos almacenes de ramos generales, dos comisarías y las 31
        // personas que fija su archivo de pueblos (por su tamaño le tocarían
        // 51).
        let pico = &loaded.towns[3];
        assert_eq!(pico.size, TownSize::VeryBig);
        assert!(pico.station_plaza.is_some());
        let count = |kind: PlaceKind| pico.places().iter().filter(|p| p.kind == kind).count();
        assert_eq!(count(PlaceKind::GeneralStore), 2);
        assert_eq!(count(PlaceKind::PoliceStation), 2);
        assert_eq!(count(PlaceKind::Plaza), 2);
        assert_eq!(pico.townsfolk_count(), 31);
        for small in loaded.towns.iter().filter(|t| t.name != "General Pico") {
            assert_eq!(small.size, TownSize::Small, "{}", small.name);
            assert_eq!(small.station_plaza, None, "{}", small.name);
            assert_eq!(small.townsfolk_count(), 10, "{}", small.name);
        }
        // Una línea del ferrocarril recorre Castex, Monte Nievas, Metileo y
        // General Pico, y un ramal sale de ella en Metileo hacia Caleufú.
        assert_eq!(loaded.line.towns, [1, 0, 2, 3]);
        let links: Vec<(usize, usize)> = loaded.line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(links, [(1, 0), (0, 2), (2, 3), (2, 4)]);
        assert_eq!(loaded.line.branches.len(), 1);
        assert_eq!(loaded.line.branches[0].towns, [2, 4]);
        // Los almacenes de los pueblos pequeños son pulperías; los de Pico,
        // almacenes de ramos generales.
        for town in &loaded.towns {
            let expected = if town.size == TownSize::Small {
                "PULPERÍA"
            } else {
                "ALMACÉN DE RAMOS GENERALES"
            };
            for store in town
                .places()
                .iter()
                .filter(|p| p.kind == PlaceKind::GeneralStore)
            {
                assert_eq!(store.name, expected, "{}", town.name);
            }
        }
        // Se coloca la Laguna La Arocena, apartada de todo.
        assert_eq!(loaded.lagoons.len(), 1);
        assert_eq!(loaded.lagoons[0].name, "Laguna La Arocena");
    }

    #[test]
    fn general_pico_grows_on_both_sides_of_the_line_through_its_middle() {
        let loaded = crate::shipped_world::loaded();
        let pico = &loaded.towns[3];
        let rail = pico.railway.expect("a railway");
        let band = pico.rail_band().expect("the line runs through Pico");
        let grid = pico.grid();
        // La vía va de este a oeste por el centro del pueblo, no por su borde:
        // a menos de una manzana de la fila del medio, dentro de la franja.
        let y = rail.start.y;
        assert_eq!(rail.end.y, y);
        assert!(
            (y - grid.center().y).abs() < 40.0,
            "{y} vs {:?}",
            grid.center()
        );
        assert!(band.min.y < y && y < band.max.y);
        assert!(rail.start.x < grid.min.x && rail.end.x > grid.max.x);
        // Casas a ambos lados, y nada construido sobre la vía.
        let houses = |north: bool| {
            pico.buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .filter(|b| (b.footprint.center().y > y) == north)
                .count()
        };
        assert!(
            houses(true) > 30 && houses(false) > 30,
            "{} / {}",
            houses(true),
            houses(false)
        );
        let corridor = pico.rail_corridor().unwrap();
        for b in pico
            .buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
        {
            assert!(!b.footprint.intersects(&corridor), "{b:?} on the track");
        }
        // (El borde del andén está a 1.65 m de la vía, dentro del corredor). La
        // estación y su andén están en el centro del pueblo, del lado norte de
        // la vía, con el andén a lo largo de ella.
        let of = |kind| pico.buildings.iter().find(|b| b.kind == kind).unwrap();
        let (station, platform) = (of(BuildingKind::Station), of(BuildingKind::Platform));
        assert!(band.contains(&station.footprint) && band.contains(&platform.footprint));
        assert!(station.footprint.min.y > platform.footprint.max.y - 1e-3);
        assert!(platform.footprint.min.y > y);
        assert!((platform.footprint.center().x - grid.center().x).abs() < 1.0);
        // Un almacén y una comisaría a cada lado de la línea, cada uno al
        // servicio de su mitad del pueblo.
        for kind in [BuildingKind::GeneralStore, BuildingKind::PoliceStation] {
            let north: Vec<bool> = pico
                .buildings
                .iter()
                .filter(|b| b.kind == kind)
                .map(|b| b.footprint.center().y > y)
                .collect();
            assert_eq!(north.len(), 2, "{kind:?}");
            assert_ne!(north[0], north[1], "{kind:?}");
        }
        // La plaza principal queda al noroeste de la estación, a unas cuadras.
        let to_plaza = pico.plaza.center() - station.footprint.center();
        assert!(to_plaza.x < -40.0 && to_plaza.y > 60.0, "{to_plaza:?}");
        assert!(pico.plaza.min.y > band.max.y);
        // Cada calle norte-sur que cruza la vía tiene su paso a nivel, sobre un
        // camino de calle, perpendicular a la vía; las demás terminan antes del
        // playón de la estación.
        let streets = pico.crossing_streets();
        assert!(streets.len() >= 3, "{streets:?}");
        let crossings: Vec<_> = loaded
            .roads
            .roads
            .iter()
            .filter(|r| r.kind == pampa_core::RoadKind::Street && r.from == 3)
            .flat_map(|r| r.crossings.iter())
            .collect();
        assert_eq!(crossings.len(), streets.len());
        for (c, (a, _)) in crossings.iter().zip(&streets) {
            assert!((c.at.x - a.x).abs() < 1e-3 && (c.at.y - y).abs() < 1e-3);
            assert!(c.angle() > pampa_core::RoadNetwork::MIN_CROSSING_ANGLE);
        }
        let yard: Vec<_> = pico
            .buildings
            .iter()
            .filter(|b| band.contains(&b.footprint))
            .collect();
        for s in &pico.streets {
            for b in &yard {
                assert!(!s.intersects(&b.footprint), "{s:?} into {b:?}");
            }
        }
        // Los peatones cruzan la línea por los pasos a nivel.
        let graph = pampa_core::StreetGraph::from_town(pico);
        let across = graph.links.iter().enumerate().any(|(n, links)| {
            links
                .iter()
                .any(|&m| (graph.nodes[n].y - y) * (graph.nodes[m].y - y) < 0.0)
        });
        assert!(across);
        // El suelo sube hasta la plataforma de cada paso a nivel y el jinete la
        // cruza, de una mitad del pueblo a la otra.
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        world.lay_roads(loaded.roads.clone());
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        for (a, b) in &streets {
            let deck = world.ground_height(Vec2::new(a.x, y)) - world.terrain.height(a.x, y);
            assert!(
                (deck - pampa_core::RoadNetwork::CROSSING_RISE).abs() < 0.05,
                "{deck}"
            );
            for k in 0..=20 {
                let p = *a + (*b - *a) * (k as f32 / 20.0);
                assert_eq!(world.resolve(collider, p, |_| ()), p, "{p:?}");
            }
        }
    }

    #[test]
    fn general_picos_sidewalks_keep_off_the_railway_band_and_the_platform() {
        let loaded = crate::shipped_world::loaded();
        let pico = &loaded.towns[3];
        assert_eq!(
            pampa_core::town::Paving::of(pico.blocks()),
            pampa_core::town::Paving::Everywhere
        );
        let band = pico.rail_band().expect("the line runs through Pico");
        let corridor = pico.rail_corridor().unwrap();
        let floors = pico.floors();
        assert!(!floors.is_empty(), "Pico's platform");
        let walks = pico.sidewalks();
        assert!(walks.len() > 200, "{} strips", walks.len());
        for walk in walks {
            let a = walk.area;
            assert!(!a.intersects(&band), "{a:?} on the railway band {band:?}");
            assert!(!a.intersects(&corridor), "{a:?} on the track");
            for floor in &floors {
                for area in floor.ramps().into_iter().chain([floor.footprint]) {
                    assert!(!a.intersects(&area), "{a:?} on the platform {area:?}");
                }
            }
        }
    }

    #[test]
    fn the_train_stops_at_metileo_by_its_platform() {
        use pampa_core::{LineTrain, Run, Timetable};
        let loaded = crate::shipped_world::loaded();
        let metileo = &loaded.towns[2];
        assert_eq!(metileo.name, "Metileo");
        // Un apeadero: unas pocas casas alrededor de su almacén, sin comisaría.
        let count = |kind| metileo.buildings.iter().filter(|b| b.kind == kind).count();
        assert_eq!(count(BuildingKind::PoliceStation), 0);
        assert_eq!(count(BuildingKind::GeneralStore), 1);
        assert!((3..=15).contains(&(count(BuildingKind::House) + count(BuildingKind::Shed))));
        let train = LineTrain::new(&loaded.line, &loaded.towns, Timetable::default()).unwrap();
        let station = train.station_of_town(2).expect("Metileo is a station");
        // Ambos viajes paran allí: el ascendente desde Pico y el descendente
        // hacia Pico.
        for run in [Run::Up, Run::Down] {
            let (arrive, depart) = train.stop_times(run, station).unwrap();
            assert!(depart > arrive, "{run:?}");
            let mid = train.state_at(run, (arrive + depart) / 2.0);
            assert!(pampa_core::stands_at(&mid, station), "{run:?}");
        }
        assert!(train.misplaced_stops().is_empty());
        // Su andén es accesible a pie desde la calle.
        let (spot, _) = metileo.platform_spot().expect("a platform");
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
        assert_eq!(world.resolve(collider, spot, |_| ()), spot);
        assert!(world.ground_height(spot) > world.terrain.height(spot.x, spot.y) + 0.5);
    }

    /// El frente más angosto que admite una casa de pueblo de tres módulos:
    /// tres módulos de 2.8 m entre paredes retiradas 0.1 m del lote (el kit de
    /// fachadas de `pampa_render`).
    const THREE_BAYS: f32 = 3.0 * 2.8 + 2.0 * 0.1;

    /// Ancho de `b` a lo largo de su frente, en metros.
    fn frontage(b: &pampa_core::Building) -> f32 {
        let s = b.footprint.size();
        match b.front {
            pampa_core::Side::North | pampa_core::Side::South => s.x,
            pampa_core::Side::East | pampa_core::Side::West => s.y,
        }
    }

    #[test]
    fn metileo_has_lots_wide_enough_for_three_bay_houses_and_a_windowed_church() {
        // Las manzanas de Metileo miden 40 m, como en los otros pueblos. Sus
        // manzanas de 30 m (hasta 2026-10-25) dejaban lotes de casa de 6–7.5 m,
        // de dos módulos cada uno, y un frente de iglesia de 9 m sin lugar para
        // una ventana junto a su torre.
        let loaded = crate::shipped_world::loaded();
        let metileo = &loaded.towns[2];
        assert_eq!(metileo.name, "Metileo");
        let of = |kind| metileo.buildings.iter().filter(move |b| b.kind == kind);
        let houses: Vec<f32> = of(BuildingKind::House).map(frontage).collect();
        // Ninguna casa más angosta que la más angosta de los otros pueblos
        // (8 m), y la mayoría lo bastante anchas para tres módulos de 2.8 m
        // entre sus paredes.
        assert!(houses.iter().all(|&w| w >= 8.0), "{houses:?}");
        let three = houses.iter().filter(|&&w| w >= THREE_BAYS).count();
        assert!(2 * three >= houses.len(), "{houses:?}");
        // El frente de la iglesia mide 12 m, como en todos los demás pueblos:
        // junto a su torre de 4 m deja 3.9 m de pared de nave a cada lado
        // (dentro de sus retiros de 0.1 m), más que los 2.4 m que necesita una
        // ventana en arco.
        let church = of(BuildingKind::Church).map(frontage).collect::<Vec<_>>();
        assert_eq!(church.len(), 1);
        assert!(church[0] >= 12.0 - 1e-3, "{church:?}");
    }

    #[test]
    fn the_shipped_streets_are_two_meters_wider_and_picos_three_and_every_town_paves_by_its_tier() {
        let loaded = crate::shipped_world::loaded();
        // (nombre, ancho de la calle, calzada que queda entre las veredas),
        // todo literal: aquí la calzada no se deriva del ancho.
        let wanted: [(&str, f32, f32); 5] = [
            ("Monte Nievas", 12.0, 5.0),
            ("Eduardo Castex", 12.0, 5.0),
            ("Metileo", 12.0, 5.0),
            ("General Pico", 13.0, 6.0),
            ("Caleufú", 12.0, 5.0),
        ];
        assert_eq!(loaded.towns.len(), wanted.len());
        for (town, &(name, width, carriageway)) in loaded.towns.iter().zip(&wanted) {
            assert_eq!(town.name, name);
            // La calle de circunvalación alrededor de la cuadrícula es tan
            // ancha como las calles.
            let first = town
                .streets
                .iter()
                .map(|s| s.size().x.min(s.size().y))
                .fold(f32::INFINITY, f32::min);
            assert!((first - width).abs() < 1e-3, "{name}: streets {first} m");
            // Todos los pueblos tienen veredas (3.5 m por lado), incluida la
            // aldea, y dejan una calzada de 5 m, en Pico de 6 m.
            assert!(!town.sidewalks().is_empty(), "{name}");
            assert_eq!(pampa_core::town::Sidewalk::WIDTH, 3.5);
            assert_sidewalks_line_every_cross_section(town, carriageway);
            // Ladrillo: todas las de Pico, el anillo de la plaza de un pueblo
            // (4 franjas), ninguna de una aldea; cada franja tiene sus
            // cordones.
            let brick = town
                .sidewalks()
                .iter()
                .filter(|w| w.walk == Walk::Brick)
                .count();
            let expected = match name {
                "General Pico" => town.sidewalks().len(),
                "Metileo" => 0,
                _ => 4,
            };
            assert_eq!(brick, expected, "{name}");
            assert!(
                town.sidewalks().iter().all(|w| !w.curbs.is_empty()),
                "{name}"
            );
        }
    }

    /// Comprueba cada sección transversal a lo largo de la mitad central de
    /// cada calle del pueblo (en sus extremos, la vereda de una calle
    /// transversal puede atravesarla, y una intersección no tiene frente), un
    /// borde de calle a la vez. Una vereda corresponde a un borde exactamente
    /// donde el terreno más allá es una manzana o una plaza: dentro de la
    /// cuadrícula, no en otra calle y no en la franja del ferrocarril (donde
    /// están la vía y el playón de la estación; el borde exterior de la calle
    /// de circunvalación da a la pampa). Donde corresponde una, debe tener
    /// `Sidewalk::WIDTH` de profundidad desde el borde; donde no corresponde
    /// ninguna, nada puede cubrir ese borde; nada cubre el centro de la calle;
    /// y donde corresponden ambas, `carriageway` es lo que queda.
    fn assert_sidewalks_line_every_cross_section(
        town: &pampa_core::town::TownLayout,
        carriageway: f32,
    ) {
        const SAMPLES: usize = 16;
        const EPS: f32 = 1e-3;
        let width = pampa_core::town::Sidewalk::WIDTH;
        let name = &town.name;
        let axis = |v: Vec2, x: bool| if x { v.x } else { v.y };
        let band = town.rail_band();
        let block_land = |p: Vec2| {
            town.grid().contains_point(p)
                && !band.is_some_and(|b| b.contains_point(p))
                && !town.streets.iter().any(|s| s.contains_point(p))
        };
        for street in &town.streets {
            let size = street.size();
            // Si la calle va a lo largo de x; el eje transversal es el otro.
            let x = size.x >= size.y;
            let (lo, hi) = (axis(street.min, !x), axis(street.max, !x));
            for k in 0..SAMPLES {
                let t = 0.25 + 0.5 * (k as f32 + 0.5) / SAMPLES as f32;
                let at = axis(street.min, x) + t * axis(size, x);
                let point = |across: f32| {
                    if x {
                        Vec2::new(at, across)
                    } else {
                        Vec2::new(across, at)
                    }
                };
                // Una intersección no tiene frente: las veredas de la calle
                // transversal rodean sus esquinas.
                let middle = point((lo + hi) / 2.0);
                if town
                    .streets
                    .iter()
                    .any(|o| o != street && o.contains_point(middle))
                {
                    continue;
                }
                let mut covered: Vec<(f32, f32)> = town
                    .sidewalks()
                    .iter()
                    .map(|w| w.area)
                    .filter(|a| axis(a.min, x) < at && at < axis(a.max, x))
                    .map(|a| (axis(a.min, !x).max(lo), axis(a.max, !x).min(hi)))
                    .filter(|(a, z)| a < z)
                    .collect();
                covered.sort_by(|a, b| a.0.total_cmp(&b.0));
                // Los tramos que las veredas dejan en contacto con cada borde,
                // y lo que cubren en total.
                let (mut low, mut high, mut total) = (0.0f32, 0.0f32, 0.0f32);
                let mut edge = lo;
                for &(a, z) in &covered {
                    if a <= lo + EPS {
                        low = low.max(z - lo);
                    }
                    if z >= hi - EPS {
                        high = high.max(hi - a);
                    }
                    total += (z - a.max(edge)).max(0.0);
                    edge = edge.max(z);
                }
                let (low_belongs, high_belongs) =
                    (block_land(point(lo - 0.25)), block_land(point(hi + 0.25)));
                let here = format!("{name}: street {street:?} across {at}");
                let depth = |belongs: bool| if belongs { width } else { 0.0 };
                assert!(
                    (low - depth(low_belongs)).abs() < EPS,
                    "{here}: low {low} m"
                );
                assert!(
                    (high - depth(high_belongs)).abs() < EPS,
                    "{here}: high {high} m"
                );
                assert!(
                    (total - low - high).abs() < EPS,
                    "{here}: a sidewalk off the edges, {total} m covered"
                );
                if low_belongs && high_belongs {
                    let left = hi - lo - total;
                    assert!((left - carriageway).abs() < EPS, "{here}: {left} m");
                }
            }
        }
    }

    #[test]
    fn metileo_and_caleufu_keep_clear_of_their_neighbours_roads_and_settlements() {
        let loaded = crate::shipped_world::loaded();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        const METILEO: usize = 2;
        let metileo = &loaded.towns[METILEO];
        // Todavía centrado en su origen, ahora de 168 m de ancho: tres manzanas
        // de 40 m y cuatro calles de 12 m.
        let grid = metileo.grid();
        assert_eq!(grid.center(), Vec2::new(288.0, -544.0));
        assert_eq!(grid.size(), Vec2::new(168.0, 168.0));
        // Nada construido sobre otra cosa: edificios, calles y la plaza.
        let built = &metileo.buildings;
        for (i, a) in built.iter().enumerate() {
            for b in &built[i + 1..] {
                assert!(!a.footprint.intersects(&b.footprint), "{a:?} on {b:?}");
            }
            assert!(
                !a.footprint.intersects(&metileo.plaza),
                "{a:?} in the plaza"
            );
            for s in &metileo.streets {
                assert!(!a.footprint.intersects(s), "{a:?} in {s:?}");
            }
        }
        // Metileo y Caleufú quedan apartados de los otros pueblos, de la
        // laguna, de los caminos de otros pueblos y de todos los asentamientos.
        const CALEUFU: usize = 4;
        let laid = crate::shipped_world::laid();
        for town in [METILEO, CALEUFU] {
            let band = loaded.towns[town].leveled();
            for (i, other) in loaded.towns.iter().enumerate() {
                if i != town {
                    let near = other.leveled().intersects(&band.expanded(500.0));
                    assert!(!near, "{} by {}", loaded.towns[town].name, other.name);
                }
            }
            for lagoon in &loaded.lagoons {
                assert!(!lagoon.keep_out().intersects(&band.expanded(500.0)));
            }
            // Solo sus propios caminos llegan a su terreno.
            let reach = band.expanded(100.0);
            for road in &loaded.roads.roads {
                if road.from != town && road.to != town {
                    let on = road.path.iter().any(|p| reach.contains_point(*p));
                    assert!(!on, "{:?} {}->{}", road.kind, road.from, road.to);
                }
            }
            for settlement in &laid.settlements.settlements {
                assert!(
                    !settlement.bounds.intersects(&band),
                    "{:?}",
                    settlement.kind
                );
            }
        }
        // Caleufú queda dentro del borde oeste, y también las prolongaciones de
        // su vía.
        let caleufu = &loaded.towns[CALEUFU];
        assert_eq!(caleufu.name, "Caleufú");
        let bounds = loaded.world.bounds();
        let inner = caleufu.leveled().expanded(100.0);
        assert!(inner.min.x > bounds.min.x && inner.max.x < bounds.max.x);
        assert!(inner.min.y > bounds.min.y && inner.max.y < bounds.max.y);
        let rail = caleufu.railway.expect("a railway");
        assert!(rail.start.x - TownLayout::RAIL_CORRIDOR_HALF > bounds.min.x + 200.0);
    }

    #[test]
    fn no_two_level_crossings_of_the_shipped_world_share_a_stretch_of_road() {
        // Cada paso a nivel tiene señales a ambos lados (dos cruces de San
        // Andrés, a 6 m): pasos a nivel de un mismo camino a menos de 20 m
        // amontonarían cuatro señales alrededor de un solo paso (`signs_posted`
        // de render los reduce a dos). El mundo incluido no tiene ninguno: los
        // más cercanos están a 53 m.
        let loaded = crate::shipped_world::loaded();
        let all: Vec<_> = loaded.roads.crossings().copied().collect();
        assert!(all.len() >= 10, "{}", all.len());
        for (i, c) in all.iter().enumerate() {
            for o in &all[i + 1..] {
                let same_road = c.road.dot(o.road).abs() > 0.9;
                assert!(
                    !same_road || c.at.distance(o.at) > 40.0,
                    "{:?} {:?}",
                    c.at,
                    o.at
                );
            }
        }
    }

    #[test]
    fn the_shipped_world_has_its_roads_and_one_level_crossing_by_each_station() {
        let loaded = crate::shipped_world::loaded();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let roads = &loaded.roads;
        assert!(roads.length_between(0, 1).is_some());
        assert!(roads.length_between(0, 2).is_some());
        for (i, town) in loaded.towns.iter().enumerate() {
            // General Pico, a ambos lados de su línea, la cruza en cambio por
            // sus calles (ver el test de arriba).
            if town.rail_band().is_some() {
                continue;
            }
            // Su huella rural cruza su propia línea junto a la estación, una
            // sola vez: la vía muerta a su lado (la de Metileo) se vuelve a
            // unir a la línea al este del camino (el camino que sale de Castex
            // hacia Caleufú también cruza la prolongación de la línea; ver los
            // tests de Caleufú).
            let crossings: Vec<_> = roads
                .roads
                .iter()
                .filter(|r| r.from == i && r.kind == pampa_core::RoadKind::Rural)
                .flat_map(|r| r.crossings.iter())
                .collect();
            assert_eq!(crossings.len(), 1, "{}", town.name);
            let station = town
                .buildings
                .iter()
                .find(|b| b.kind == BuildingKind::Station)
                .unwrap();
            for crossing in &crossings {
                assert!(crossing.at.distance(station.footprint.center()) < 160.0);
            }
            // Su huella rural va desde la calle por encima del paso a nivel.
            assert!(
                roads
                    .roads
                    .iter()
                    .any(|r| r.from == i && r.kind == pampa_core::RoadKind::Rural)
            );
        }
    }

    #[test]
    fn a_broken_railway_line_warns_naming_the_towns_and_keeps_them() {
        let path = temp_towns(
            "line",
            r#"(towns: [
                (name: "Oeste", origin: (-2000.0, 0.0), blocks: (4, 4), block_size: 40.0,
                 street_width: 10.0, railway: Some((side: North, offset: 24.0))),
                (name: "Medio", blocks: (4, 4), block_size: 40.0, street_width: 10.0),
                (name: "Este", origin: (2000.0, 0.0), blocks: (4, 4), block_size: 40.0,
                 street_width: 10.0, railway: Some((side: North, offset: 24.0))),
            ], line: Some(["Oeste", "Este", "Este", "Rancul"]))"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns.len(), 3);
        assert_eq!(loaded.line.towns, [0, 2]);
        assert!(loaded.line.links.is_empty());
        let [duplicate, unknown, crossing, road] = &loaded.warnings[..] else {
            panic!("{:?}", loaded.warnings)
        };
        assert!(duplicate.contains("`Este`"), "{duplicate}");
        assert!(unknown.contains("`Rancul`"), "{unknown}");
        assert!(crossing.contains("`Medio`"), "{crossing}");
        // El camino entre ellos también cruzaría Medio, así que se deja afuera.
        assert!(road.contains("road") && road.contains("`Medio`"), "{road}");
        let report = TownReport::new(&loaded);
        assert_eq!(report.warnings, loaded.warnings);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_world_follows_the_loaded_town() {
        let path = temp_towns(
            "small",
            r#"(towns: [(name: "General Alvear", blocks: (3, 4), block_size: 34.0,
                street_width: 11.0, railway: Some((side: South, offset: 26.0)), seed: 3)])"#,
        );
        let loaded = load_world(&path);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let town = loaded.towns[0].clone();
        assert_eq!(town.name, "General Alvear");
        assert_eq!(town.streets.len(), 4 + 5);
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        assert_eq!(world.obstacles, town.obstacles());
        let (spawn, _) = town.arrival();
        assert!(world.obstacles.iter().all(|o| !o.contains_point(spawn)));
        // La estación está junto al ferrocarril, dentro de su extensión, para
        // que el tren de la línea pare allí.
        let rail = town.railway.expect("a railway");
        let station = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .expect("a station")
            .footprint
            .center();
        let along = rail.end - rail.start;
        let t = (station - rail.start).dot(along) / along.dot(along);
        assert!(
            (0.0..=1.0).contains(&t),
            "station off the track's span: {t}"
        );
        let crowd = Townsfolk::new(&town, 4, 1);
        assert_eq!(crowd.graph.nodes.len(), 4 * 5);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn every_town_is_built_at_its_origin_and_clashes_are_reported() {
        let path = temp_towns(
            "many",
            r#"(world: (half_extent: 5000.0), towns: [
                (name: "Eduardo Castex", blocks: (4, 5), block_size: 40.0, street_width: 10.0,
                 railway: Some((side: North, offset: 24.0)), seed: 7),
                (name: "Florencio Varela", blocks: (3, 3), block_size: 40.0, street_width: 10.0),
                (name: "Lejos", origin: (3000.0, -2000.0), blocks: (3, 4), block_size: 36.0,
                 street_width: 11.0, railway: Some((side: South, offset: 26.0)), seed: 11),
            ])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.world.half_extent, 5000.0);
        let names: Vec<&str> = loaded.towns.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Eduardo Castex", "Lejos"]);
        assert_eq!(loaded.towns[1].grid().center(), Vec2::new(3000.0, -2000.0));
        let [warning] = &loaded.warnings[..] else {
            panic!("{:?}", loaded.warnings)
        };
        assert!(
            warning.contains("Florencio Varela") && warning.contains("Eduardo Castex"),
            "{warning}"
        );
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        assert_eq!(world.terrain.flat_areas.len(), 2);
        let report = TownReport::new(&loaded);
        assert_eq!(report.names, ["Eduardo Castex", "Lejos"]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_bad_file_falls_back_to_castex_with_warnings() {
        let path = temp_towns(
            "bad",
            r#"(towns: [(name: "Ciudad #1", blocks: (3, 3), block_size: 40.0, street_width: 2.0)])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns, vec![TownLayout::castex_inspired()]);
        assert!(
            loaded.warnings.iter().any(|w| w.contains("street_width")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();

        let path = temp_towns(
            "glyphs",
            r#"(towns: [(name: "Ciudad 漢1", blocks: (3, 3), block_size: 40.0, street_width: 10.0,
                railway: Some((side: North, offset: 24.0)))])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns[0].name, "Ciudad 漢1");
        assert!(
            loaded.warnings.iter().any(|w| w.contains("'漢'")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_world_too_small_for_any_town_falls_back_to_castex_in_the_default_world() {
        let path = temp_towns(
            "tiny",
            r#"(world: (half_extent: 100.0, chunk_size: 50.0), towns: [
                (name: "A", blocks: (3, 3), block_size: 40.0, street_width: 10.0)])"#,
        );
        let loaded = load_world(&path);
        assert_eq!(loaded.towns, vec![TownLayout::castex_inspired()]);
        assert_eq!(loaded.world, WorldConfig::default());
        assert!(
            loaded.warnings.iter().any(|w| w.contains("default world")),
            "{:?}",
            loaded.warnings
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// Caleufú, el quinto pueblo del mundo incluido, y su ramal.
    mod caleufu {
        use super::*;
        use pampa_core::{
            Aabb, BASE_RATE, CarKind, Floor, LineTrain, MAX_STOP_MISS, RoadKind, RoadNetwork, Run,
            Terrain, Timetable,
        };

        const CASTEX: usize = 1;
        const METILEO: usize = 2;
        const CALEUFU: usize = 4;

        fn loaded() -> &'static LoadedWorld {
            let loaded = crate::shipped_world::loaded();
            assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
            assert_eq!(loaded.towns[CALEUFU].name, "Caleufú");
            loaded
        }

        /// Dónde se cruzan los segmentos `p`–`q` y `a`–`b`, si se cruzan.
        fn crossing(p: Vec2, q: Vec2, a: Vec2, b: Vec2) -> Option<Vec2> {
            let (r, s) = (q - p, b - a);
            let denom = r.x * s.y - r.y * s.x;
            if denom.abs() < 1e-9 {
                return None;
            }
            let d = a - p;
            let t = (d.x * s.y - d.y * s.x) / denom;
            let u = (d.x * r.y - d.y * r.x) / denom;
            ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| p + r * t)
        }

        #[test]
        fn caleufu_is_a_small_railway_town_with_its_pulperia_and_comisaria() {
            let loaded = loaded();
            let town = &loaded.towns[CALEUFU];
            assert_eq!(town.size, TownSize::Small);
            let places = town.places();
            let named = |kind: PlaceKind| -> Vec<&str> {
                places
                    .iter()
                    .filter(|p| p.kind == kind)
                    .map(|p| p.name.as_str())
                    .collect()
            };
            assert_eq!(named(PlaceKind::GeneralStore), ["PULPERÍA"]);
            assert_eq!(named(PlaceKind::PoliceStation).len(), 1);
            assert_eq!(named(PlaceKind::Plaza).len(), 1);
            assert_eq!(named(PlaceKind::Station), ["ESTACIÓN CALEUFÚ"]);
            let houses = town
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::House)
                .count();
            assert!(houses >= 8, "{houses}");
            // La pulpería y la comisaría están a varias manzanas de distancia.
            let at = |kind: PlaceKind| places.iter().find(|p| p.kind == kind).unwrap().center();
            let apart = at(PlaceKind::GeneralStore) - at(PlaceKind::PoliceStation);
            let pitch = 40.0 + 10.0;
            assert!(apart.x.abs().max(apart.y.abs()) >= 1.9 * pitch, "{apart:?}");
        }

        #[test]
        fn the_branch_joins_the_line_at_metileo_and_runs_unbroken_to_caleufus_station() {
            let loaded = loaded();
            let line = &loaded.line;
            let metileo = loaded.towns[METILEO].railway.unwrap();
            let caleufu = loaded.towns[CALEUFU].railway.unwrap();
            let branch = line
                .links
                .iter()
                .find(|l| (l.from, l.to) == (METILEO, CALEUFU))
                .expect("the branch link");
            // El empalme: la punta oeste de Metileo, adonde llega la línea
            // desde Monte Nievas.
            let from_nievas = line.links.iter().find(|l| l.to == METILEO).unwrap();
            assert_eq!(branch.track.start, metileo.start);
            assert_eq!(from_nievas.track.end, metileo.start);
            assert_eq!(branch.track.end, caleufu.end);
            // Un único recorrido sin cortes, con curvas suaves, por la estación
            // de Metileo y hasta la punta lejana de Caleufú.
            let paths = line.branch_paths(&loaded.towns);
            let points = paths[0].points();
            assert_eq!(points.first(), Some(&metileo.end));
            assert_eq!(points.last(), Some(&caleufu.start));
            let expected = metileo.start.distance(metileo.end)
                + branch.length()
                + caleufu.start.distance(caleufu.end);
            assert!((paths[0].length() - expected).abs() < 0.5);
            let sharpest = points
                .windows(3)
                .map(|w| {
                    let a = (w[1] - w[0]) * (1.0 / w[0].distance(w[1]));
                    let b = (w[2] - w[1]) * (1.0 / w[1].distance(w[2]));
                    (a.x * b.y - a.y * b.x).atan2(a.dot(b)).abs()
                })
                .fold(0.0, f32::max);
            let max = pampa_core::RailLine::MAX_TURN + 1e-4;
            assert!(sharpest <= max, "{}", sharpest.to_degrees());
            let radius = pampa_core::RailLine::CURVE_RADIUS - 0.1;
            assert!(branch.curves.iter().all(|c| c.radius >= radius));
            // Pasado el empalme, el ramal se separa de la línea para siempre.
            let main = line.full_path(&loaded.towns);
            for p in &branch.path {
                if p.distance(metileo.start) > 400.0 {
                    let gap = main.at(main.project(*p)).distance(*p);
                    assert!(gap > 50.0, "{p:?} {gap}");
                }
            }
            // El andén de Caleufú está junto al ramal.
            let (spot, _) = loaded.towns[CALEUFU].platform_spot().expect("a platform");
            let on = paths[0].at(paths[0].project(spot));
            assert!(on.distance(spot) < 12.0, "{}", on.distance(spot));
            // El tren de la línea sigue recorriendo solo la línea.
            let train =
                pampa_core::LineTrain::new(line, &loaded.towns, pampa_core::Timetable::default())
                    .unwrap();
            assert_eq!(train.station_of_town(CALEUFU), None);
            assert_eq!(train.stations().len(), 4);
            assert!(train.misplaced_stops().is_empty());
        }

        #[test]
        fn a_road_joins_caleufu_to_castex_and_every_road_on_the_branch_crosses_it_level() {
            let loaded = loaded();
            let roads = &loaded.roads;
            // Su pueblo más cercano es Eduardo Castex, a unos 5.9 km.
            let road = roads
                .roads
                .iter()
                .find(|r| r.kind == RoadKind::Between && (r.from, r.to) == (CASTEX, CALEUFU))
                .expect("Castex to Caleufú");
            let length = roads.length_between(CASTEX, CALEUFU).unwrap();
            assert!((5_000.0..7_000.0).contains(&length), "{length}");
            // Cruza en perpendicular la prolongación de Castex, al oeste del
            // pueblo.
            assert_eq!(road.crossings.len(), 1);
            let crossing_at = road.crossings[0];
            assert!(crossing_at.angle() > RoadNetwork::MIN_CROSSING_ANGLE);
            assert!(crossing_at.at.x < loaded.towns[CASTEX].grid().min.x);
            // Los carteles indicadores de cada extremo nombran al otro pueblo.
            let boards: Vec<&str> = roads
                .signposts
                .iter()
                .flat_map(|s| &s.boards)
                .map(|b| b.text.as_str())
                .collect();
            assert!(
                boards.iter().any(|b| b.starts_with("CALEUFÚ ")),
                "{boards:?}"
            );
            let castex = boards.iter().filter(|b| b.starts_with("EDUARDO CASTEX "));
            assert!(castex.count() >= 2, "{boards:?}");
            // La huella rural de Caleufú cruza el ramal junto a la estación.
            let rural = roads
                .roads
                .iter()
                .find(|r| r.kind == RoadKind::Rural && r.from == CALEUFU)
                .expect("Caleufú's rural track");
            assert_eq!(rural.crossings.len(), 1);
            // Donde un camino se encuentra con el ramal, tiene un paso a nivel.
            let branch = &loaded.line.branch_paths(&loaded.towns)[0];
            let mut met = 0;
            for road in &roads.roads {
                for w in road.path.windows(2) {
                    for r in branch.points().windows(2) {
                        let Some(at) = crossing(w[0], w[1], r[0], r[1]) else {
                            continue;
                        };
                        met += 1;
                        let level = road.crossings.iter().any(|c| c.at.distance(at) < 1e-2);
                        assert!(level, "{:?} meets the branch at {at:?}", road.kind);
                    }
                }
            }
            assert!(met >= 1);
        }

        /// El tren de la línea y la lanzadera del mundo incluido.
        fn trains(loaded: &LoadedWorld) -> (LineTrain, LineTrain) {
            let line = LineTrain::new(&loaded.line, &loaded.towns, Timetable::default())
                .expect("a line train");
            let path = loaded.line.shuttle_paths(&loaded.towns).remove(0);
            let bay = loaded.line.bay_of(0).expect("a bay");
            let stops = [(METILEO, bay.stop)];
            let shuttle =
                LineTrain::branch_shuttle(&line, path, &[METILEO, CALEUFU], &stops, &loaded.towns)
                    .expect("a shuttle");
            (line, shuttle)
        }

        fn jumped(train: &LineTrain, at: f64) -> LineTrain {
            let mut train = train.clone();
            train.advance(0.0, at, BASE_RATE);
            train
        }

        #[test]
        fn metileos_bay_its_platform_and_walks_lie_beyond_the_track_clear_of_everything() {
            let loaded = loaded();
            let metileo = &loaded.towns[METILEO];
            let track = metileo.railway.unwrap();
            let bay = loaded.line.bay_of(0).expect("Metileo's bay");
            assert_eq!((bay.town, bay.branch), (METILEO, 0));
            // Al norte de la vía de la estación (del lado opuesto al pueblo), a
            // 4.5 m, con su extremo muerto en el extremo este del andén, hacia
            // el oeste.
            assert!(bay.buffer().y > track.start.y);
            assert!((bay.buffer().y - track.start.y - 4.5).abs() < 1e-3);
            assert!(bay.buffer().x < track.end.x && bay.buffer().x > track.start.x);
            assert!(bay.heading().x < -0.99);
            // Nada construido, plantado o alambrado está sobre ella, ni una
            // laguna, un asentamiento o un camino; el despeje de 40 m de los
            // caminos también los mantiene fuera.
            let laid = crate::shipped_world::laid();
            let car = 3.0 / 2.0 + 0.2;
            let around = |a: Vec2, b: Vec2| {
                let (lo, hi) = (
                    Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                    Vec2::new(a.x.max(b.x), a.y.max(b.y)),
                );
                Aabb::new(lo, hi).expanded(car)
            };
            for w in bay.link.path.windows(2) {
                let reach = around(w[0], w[1]);
                // Cada obstáculo se mantiene a medio ancho de vagón de la vía.
                for o in laid.world.obstacles.near(reach) {
                    for k in 0..=8 {
                        let p = w[0] + (w[1] - w[0]) * (k as f32 / 8.0);
                        let off = o.closest_point(p).distance(p);
                        assert!(off >= 1.5, "{o:?} stands {off} m from the bay at {p:?}");
                    }
                }
                for lagoon in &loaded.lagoons {
                    assert!(lagoon.keep_out().segment_overlap(w[0], w[1]).is_none());
                }
                for settlement in &laid.settlements.settlements {
                    assert!(!settlement.bounds.intersects(&reach));
                }
            }
            // Ningún camino se encuentra con la vía muerta: la huella rural de
            // Metileo cruza solo la vía de la estación, al oeste de donde se
            // une la vía muerta.
            let mut met = 0;
            for road in &loaded.roads.roads {
                for r in road.path.windows(2) {
                    for w in bay.link.path.windows(2) {
                        let Some(at) = crossing(r[0], r[1], w[0], w[1]) else {
                            continue;
                        };
                        met += 1;
                        assert_eq!((road.kind, road.from), (RoadKind::Rural, METILEO));
                        let level = road.crossings.iter().any(|c| c.at.distance(at) < 1e-2);
                        assert!(level, "{at:?}");
                    }
                }
            }
            assert_eq!(met, 0);
            // La flora se mantiene fuera de ella como de cualquier vía: un
            // punto 8 m al norte de la vía muerta está a 12.5 m de la vía de la
            // estación, así que solo la vía muerta en sí lo descarta.
            let beside = bay.stop + Vec2::new(0.0, 8.0);
            assert!(!laid.flora.off_track(beside, 10.0));
            assert!(laid.flora.off_track(beside, 7.0));
            // Tampoco nada que el propio pueblo haya plantado o atado.
            for w in bay.link.path.windows(2) {
                let keep = around(w[0], w[1]);
                let trees = metileo.trees.iter().copied();
                let shade = metileo.shade_trees.iter().map(|t| t.at);
                let plants = metileo.yard_plants.iter().map(|p| p.at);
                for p in trees.chain(shade).chain(plants) {
                    assert!(!keep.contains_point(p), "{p:?}");
                }
                for palenque in &metileo.palenques {
                    assert!(!keep.intersects(&palenque.rail));
                }
            }
            // El mundo construido camina y sube al tren desde su andén, que se
            // mantiene despejado como la vía muerta.
            assert!(laid.world.floors.contains(&bay.platform));
            let (road, crossing) = metileos_road(loaded);
            for area in platform_works(bay) {
                let keep = area.expanded(1.0);
                // Ningún edificio, cartel, tramo de alambrado, tronco de árbol
                // ni objeto.
                assert!(
                    laid.world
                        .obstacles
                        .near(keep)
                        .iter()
                        .all(|o| !o.intersects(&keep)),
                    "{area:?}"
                );
                // Ningún camino, ni las señales y los tablones del paso a
                // nivel.
                for w in road.path.windows(2) {
                    let reach = keep.expanded(RoadNetwork::HALF_WIDTH);
                    assert!(reach.segment_overlap(w[0], w[1]).is_none(), "{area:?}");
                }
                assert!(crossing.at.distance(area.center()) > 15.0);
                // Nada de flora donde está el andén: las matas lo atravesarían,
                // los árboles crecerían en él.
                let chunk = laid.world.config.chunk_of(area.center());
                let flora = laid.flora.chunk(chunk);
                assert!(flora.tufts.iter().all(|t| !keep.contains_point(t.at)));
                assert!(
                    flora
                        .trees
                        .iter()
                        .all(|t| !keep.expanded(2.0).contains_point(t.at))
                );
            }
            // Los senderos, por su parte, se mantienen a medio metro de todo lo
            // construido, plantado o transitado, y de las lagunas y los
            // asentamientos; terminan en el camino.
            assert_eq!(bay.walks.len(), 2);
            // Cada uno mide 2 m de ancho a lo largo del centro de su andén,
            // desde el pie de la rampa oeste del andén.
            let station = loaded.towns[METILEO].floors()[0];
            for (walk, floor) in bay.walks.iter().zip([station, bay.platform]) {
                assert!((walk.size().y - 2.0).abs() < 1e-3, "{walk:?}");
                // A 3.5 m hacia adentro del borde lejano del andén (a 11 m de
                // su vía).
                let far = if floor == station {
                    -436.0 - 11.0
                } else {
                    -436.0 + 4.5 + 11.0
                };
                let line = if floor == station {
                    far + 3.5
                } else {
                    far - 3.5
                };
                assert!((walk.center().y - line).abs() < 1e-3, "{walk:?}");
                assert!((walk.max.x - (floor.footprint.min.x - 3.0)).abs() < 1e-3);
            }
            for walk in &bay.walks {
                let keep = walk.expanded(0.5);
                assert!(
                    laid.world
                        .obstacles
                        .near(keep)
                        .iter()
                        .all(|o| !o.intersects(&keep)),
                    "{walk:?}"
                );
                for lagoon in &loaded.lagoons {
                    assert!(!lagoon.keep_out().intersects(walk));
                }
                for settlement in &laid.settlements.settlements {
                    assert!(!settlement.bounds.intersects(walk));
                }
                let chunk = laid.world.config.chunk_of(walk.center());
                let flora = laid.flora.chunk(chunk);
                assert!(flora.tufts.iter().all(|t| !keep.contains_point(t.at)));
                assert!(flora.trees.iter().all(|t| !keep.contains_point(t.at)));
                let ends = [
                    Vec2::new(walk.min.x, walk.center().y),
                    Vec2::new(walk.max.x, walk.center().y),
                ];
                let by_road = ends.iter().filter(|p| road_distance(loaded, **p) < 3.0);
                assert_eq!(by_road.count(), 1, "{walk:?}");
            }
            // El suelo bajo el andén está nivelado dentro de los 0.2 m que la
            // losa llega por debajo de él, así que se apoya plano.
            let ground = |p: Vec2| laid.world.terrain.height(p.x, p.y);
            let f = bay.platform.footprint;
            let heights: Vec<f32> = [
                f.min,
                f.max,
                Vec2::new(f.min.x, f.max.y),
                Vec2::new(f.max.x, f.min.y),
                f.center(),
            ]
            .into_iter()
            .map(ground)
            .collect();
            let (lo, hi) = heights
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
            assert!(hi - lo < 0.2, "{heights:?}");
        }

        #[test]
        fn the_trains_keep_apart_all_day_and_stand_in_the_bay_clear_of_the_crossing_and_walks() {
            let loaded = loaded();
            let (line, shuttle) = trains(loaded);
            let bay = loaded.line.bay_of(0).unwrap();
            let track = loaded.towns[METILEO].railway.unwrap();
            let (platform, _) = loaded.towns[CALEUFU].platform_spot().expect("a platform");
            let (_, crossing) = metileos_road(loaded);
            // La plataforma y los accesos del paso a nivel, y la franja de cada
            // sendero.
            let deck = Aabb::from_center_size(
                crossing.at,
                Vec2::new(2.0 * (RoadNetwork::HALF_WIDTH + 1.0), 2.0 * 10.0),
            );
            let mut keep = vec![deck];
            keep.extend(bay.walks.iter().copied());
            let (mut meetings, mut at_caleufu) = (0, 0);
            let (mut standing, mut tightest, mut nearest) = (0, f32::MAX, f32::MAX);
            // Los momentos en que la lanzadera pasa por el desvío (el tren de
            // la línea está en marcha, o fuera del camino), y qué tan cerca
            // están entonces los dos vagones.
            let (mut on_turnout, mut turnout_gap) = (0, f32::MAX);
            let join = bay.link.path.last().unwrap().x;
            // Dos días de juego, cada 15 segundos de juego: ambos viajes de la
            // línea, ambos viajes de ida y vuelta de la lanzadera, cada espera
            // en la terminal y cada paso por el desvío. Y cada 2 s desde antes
            // de que el tren de la línea entre a Metileo hasta después de que
            // la lanzadera se fue, alrededor de cada una de sus paradas allí.
            let mut times: Vec<f64> = (0..(2 * 86_400 / 15))
                .map(|k| f64::from(k) * 15.0)
                .collect();
            for run in [Run::Up, Run::Down] {
                for at in line.stops_at(METILEO, run) {
                    let from = at.seconds() - 1800.0;
                    times.extend((0..1800).map(|k| from + 2.0 * f64::from(k)));
                }
            }
            for t in times {
                let (l, s) = (jumped(&line, t), jumped(&shuttle, t));
                let (lc, sc) = (l.obstacles(), s.obstacles());
                for a in &lc {
                    for b in &sc {
                        assert!(
                            !a.footprint.expanded(0.4).intersects(&b.footprint),
                            "t = {t}: {:?} {:?}",
                            a.footprint,
                            b.footprint
                        );
                        let gap = |x: (f32, f32), y: (f32, f32)| (x.0 - y.1).max(y.0 - x.1);
                        let (f, g) = (a.footprint, b.footprint);
                        let clear = gap((f.min.x, f.max.x), (g.min.x, g.max.x))
                            .max(gap((f.min.y, f.max.y), (g.min.y, g.max.y)));
                        tightest = tightest.min(clear);
                        let turning = s.state().phase == pampa_core::LinePhase::Running
                            && b.footprint.min.x < bay.platform.footprint.min.x
                            && b.footprint.max.x > join;
                        if turning {
                            turnout_gap = turnout_gap.min(clear);
                        }
                    }
                }
                let state = s.state();
                if state.station == Some(0) {
                    // Detenida en Metileo: en la vía muerta, junto a la vía de
                    // la estación, nunca sobre ella.
                    for car in s.placed_cars() {
                        let off = car.center.y - track.start.y;
                        assert!((off - 4.5).abs() < 0.2, "{off}");
                        // Antes del paragolpes.
                        assert!(car.center.x + car.length / 2.0 < bay.buffer().x - 1.0);
                    }
                    if l.state().station == Some(2) {
                        meetings += 1;
                    }
                }
                if state.station == Some(1) {
                    // Detenida en Caleufú: coches junto al andén.
                    let cars = s.placed_cars();
                    let coach = cars.iter().find(|c| c.kind == CarKind::Coach).unwrap();
                    assert!(coach.center.distance(platform) < MAX_STOP_MISS);
                    at_caleufu += 1;
                }
                if s.state().phase == pampa_core::LinePhase::Running
                    && sc.iter().any(|c| {
                        c.footprint.min.x < bay.platform.footprint.min.x && c.footprint.max.x > join
                    })
                {
                    on_turnout += 1;
                    // No con el tren de la línea detenido en la estación: el
                    // margen del horario mantiene a la lanzadera fuera del
                    // desvío en ese momento.
                    assert!(!pampa_core::stands_at(&l.state(), 2), "t = {t}");
                }
                for (train, cars) in [(&l, &lc), (&s, &sc)] {
                    let state = train.state();
                    let here = state.station.is_some_and(|k| {
                        let town = train.stations().get(k).map(|st| st.town);
                        town == Some(METILEO) && pampa_core::stands_at(&state, k)
                    });
                    if !here {
                        continue;
                    }
                    standing += 1;
                    for car in cars {
                        for k in &keep {
                            assert!(
                                !car.footprint.intersects(k),
                                "t = {t}: {:?} on {k:?}",
                                car.footprint
                            );
                        }
                        nearest = nearest.min(
                            car.footprint
                                .closest_point(crossing.at)
                                .distance(crossing.at),
                        );
                    }
                }
            }
            // Los trenes estuvieron juntos en Metileo, y se comprobó.
            assert!(meetings > 100, "{meetings}");
            assert!(at_caleufu > 100, "{at_caleufu}");
            assert!(standing > 1000, "{standing}");
            // La lanzadera pasa por el desvío con el tren de la línea en la
            // estación, y los trenes nunca se acercan a menos de 0.4 m.
            assert!(on_turnout > 50, "{on_turnout} samples on the turnout");
            assert!(turnout_gap > 0.4, "{turnout_gap}");
            assert!(tightest > 0.4, "{tightest}");
            assert!(nearest > 25.0, "{nearest}");
        }

        /// Huellas en planta del andén con sus rampas.
        fn platform_works(bay: &pampa_core::RailBay) -> Vec<Aabb> {
            let f = bay.platform;
            f.ramps().into_iter().chain([f.footprint]).collect()
        }

        /// El camino rural de Metileo y el paso a nivel que forma con la vía de
        /// la estación.
        fn metileos_road(loaded: &LoadedWorld) -> (&pampa_core::Road, pampa_core::LevelCrossing) {
            let road = loaded
                .roads
                .roads
                .iter()
                .find(|r| r.kind == RoadKind::Rural && r.from == METILEO)
                .expect("Metileo's rural road");
            let track_y = loaded.towns[METILEO].railway.unwrap().start.y;
            let crossing = road
                .crossings
                .iter()
                .copied()
                .find(|c| (c.at.y - track_y).abs() < 1e-2)
                .expect("a crossing of the station track");
            (road, crossing)
        }

        /// Distancia desde `p` hasta el camino más cercano.
        fn road_distance(loaded: &LoadedWorld, p: Vec2) -> f32 {
            loaded.roads.distance(p)
        }

        #[test]
        fn the_roads_and_their_level_crossings_by_metileo_are_where_they_were() {
            // La vía muerta se movió enfrente del andén; el camino conserva su
            // único cruce de la vía de la estación (alrededor de x 189.7) y no
            // tiene ninguno de la vía muerta, que ahora se une a la
            // prolongación al este de él.
            let loaded = loaded();
            let (road, crossing) = metileos_road(loaded);
            assert!((crossing.at.x - 189.75).abs() < 0.1, "{:?}", crossing.at);
            assert!((crossing.at.y + 436.0).abs() < 1e-2);
            assert_eq!(road.crossings.len(), 1);
            let bay = loaded.line.bay_of(0).unwrap();
            let join = *bay.link.path.last().unwrap();
            assert!(join.x > crossing.at.x + 10.0, "{join:?}");
            assert_eq!(road.path[3], Vec2::new(189.93747, -440.12494));
        }

        #[test]
        fn a_walker_goes_from_the_station_platform_to_the_bay_platform_over_the_road_crossing() {
            let loaded = loaded();
            let (line, shuttle) = trains(loaded);
            let bay = loaded.line.bay_of(0).unwrap();
            let (road, level) = metileos_road(loaded);
            let (start, _) = loaded.towns[METILEO].platform_spot().unwrap();
            let mut world = crate::shipped_world::laid().world.clone();
            let collider = pampa_core::Circle::new(pampa_core::Rider::COLLIDER_RADIUS);
            let (w0, w1) = (bay.walks[0], bay.walks[1]);
            // Hacia el oeste por el sendero del andén de la estación hasta el
            // camino, a lo largo del camino por encima del paso a nivel, hacia
            // el este por el sendero del andén de la vía muerta, y arriba por
            // su rampa.
            let end0 = Vec2::new(w0.min.x, w0.center().y);
            let end1 = Vec2::new(w1.min.x, w1.center().y);
            let near = |p: Vec2| {
                road.path
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
                    .map(|(i, _)| i)
                    .unwrap()
            };
            let (a, b) = (near(end0), near(end1));
            assert!(a < b, "the road runs north past the crossing");
            let mut route = vec![start, Vec2::new(start.x, end0.y), end0];
            route.extend(road.path[a..=b].iter().copied());
            route.extend([end1, Vec2::new(bay.platform.footprint.center().x, end1.y)]);
            // El paso a nivel está en el camino.
            assert!(route.windows(2).any(|w| {
                crossing(w[0], w[1], crossing_rail(loaded).0, crossing_rail(loaded).1).is_some()
            }));
            let mut checked = 0;
            let mut nearest = f32::MAX;
            for run in [Run::Up, Run::Down] {
                for at in line.stops_at(METILEO, run) {
                    let (l, s) = (
                        jumped(&line, at.seconds() + 30.0),
                        jumped(&shuttle, at.seconds() + 30.0),
                    );
                    if !(l.state().station == Some(2) && s.state().station == Some(0)) {
                        continue;
                    }
                    world.moving = l.obstacles().into_iter().chain(s.obstacles()).collect();
                    let mut last = world.ground_height(start);
                    for w in route.windows(2) {
                        let steps = (w[0].distance(w[1]) / 0.25).ceil() as usize;
                        for k in 0..=steps {
                            let p = w[0] + (w[1] - w[0]) * (k as f32 / steps as f32);
                            assert_eq!(world.resolve(collider, p, |_| ()), p, "blocked at {p:?}");
                            let h = world.ground_height(p);
                            assert!((h - last).abs() < 0.5, "a {} m step at {p:?}", h - last);
                            last = h;
                        }
                    }
                    // Termina en el andén de la vía muerta, a su altura.
                    let end = *route.last().unwrap();
                    assert!(bay.platform.footprint.contains_point(end), "{end:?}");
                    let rise = world.ground_height(end) - world.terrain.height(end.x, end.y);
                    assert!((rise - Floor::PLATFORM_HEIGHT).abs() < 1e-3, "{rise}");
                    // Y de vuelta, en el otro sentido, igual de despejado.
                    for p in route.iter().rev() {
                        assert_eq!(world.resolve(collider, *p, |_| ()), *p, "{p:?}");
                    }
                    for car in world.moving.iter().map(|o| o.footprint) {
                        nearest = nearest.min(car.closest_point(level.at).distance(level.at));
                    }
                    checked += 1;
                }
            }
            assert!(checked >= 2, "{checked} moments with both trains in");
            // Ninguno de los dos trenes detenidos llega a menos de 25 m del
            // paso a nivel.
            assert!(nearest > 25.0, "{nearest}");
        }

        #[test]
        fn every_station_of_the_shipped_world_boards_and_alights_from_its_platform_beside_the_cars()
        {
            use pampa_core::{Boarding, Rider, alight_spot, boarding, coach_doors};
            let loaded = loaded();
            let (line, shuttle) = trains(loaded);
            let mut world = crate::shipped_world::laid().world.clone();
            let bay = loaded.line.bay_of(0).unwrap().platform;
            let mut checked = std::collections::BTreeSet::new();
            for (t, train) in [&line, &shuttle].into_iter().enumerate() {
                for town in 0..loaded.towns.len() {
                    for run in [Run::Up, Run::Down] {
                        for at in train.stops_at(town, run) {
                            // 30 s después de iniciada la parada: detenido (en
                            // parada, o en espera en una terminal, donde la
                            // locomotora del tren de la línea está en el
                            // extremo del andén, no en su centro).
                            let train = jumped(train, at.seconds() + 30.0);
                            let phase = train.state().phase;
                            if phase == pampa_core::LinePhase::Running {
                                continue;
                            }
                            let station = train.stations()[train.state().station.unwrap()];
                            world.moving = train.obstacles();
                            let floor = if station.bay {
                                bay
                            } else {
                                loaded.towns[town].floors()[0]
                            };
                            // El coche más cercano al centro del andén.
                            let middle = floor.footprint.center();
                            let cars = train.placed_cars();
                            let coach = (0..cars.len())
                                .filter(|&i| cars[i].kind == CarKind::Coach)
                                .min_by(|&a, &b| {
                                    let d = |i: usize| cars[i].center.distance(middle);
                                    d(a).total_cmp(&d(b))
                                })
                                .unwrap();
                            let placed = cars[coach];
                            let (spot, _) = if station.bay {
                                bay.spot_beside(placed.center)
                            } else {
                                loaded.towns[town].platform_spot().unwrap()
                            };
                            // Junto a la puerta más cercana del coche, a un
                            // paso a lo largo del andén desde el punto central:
                            // sube de inmediato.
                            let door = coach_doors(&placed)
                                .into_iter()
                                .min_by(|a, b| a.distance(spot).total_cmp(&b.distance(spot)))
                                .unwrap();
                            let by_door = spot
                                + floor.along * (door.dot(floor.along) - spot.dot(floor.along));
                            let one = [train.clone()];
                            let on_foot = |at: Vec2| {
                                Rider::on_foot(at, pampa_core::Heading::from_radians(0.0), &world)
                            };
                            let name = &loaded.towns[town].name;
                            assert!(
                                floor.footprint.contains_point(by_door),
                                "{name}: {by_door:?}"
                            );
                            assert!(
                                matches!(
                                    boarding(&one, &on_foot(by_door), &world.floors),
                                    Boarding::Board(_)
                                ),
                                "{name} train {t}: no boarding at {by_door:?}, door {door:?}"
                            );
                            // También desde el punto central, dondequiera que
                            // el tren esté detenido en él (en parada, y durante
                            // toda la espera de la lanzadera en la terminal).
                            if phase == pampa_core::LinePhase::Dwelling || t == 1 {
                                assert!(
                                    matches!(
                                        boarding(&one, &on_foot(spot), &world.floors),
                                        Boarding::Board(_)
                                    ),
                                    "{name} train {t}: no boarding at the spot {spot:?}"
                                );
                            }
                            // Bajarse pone al jinete en el andén, fuera de los
                            // vagones.
                            let (down, _) = alight_spot(&train, coach, &loaded.towns, &world)
                                .unwrap_or_else(|| panic!("{} alights", loaded.towns[town].name));
                            assert!(
                                world
                                    .floors
                                    .iter()
                                    .any(|f| f.footprint.contains_point(down))
                            );
                            let off = down.distance(world.resolve(
                                pampa_core::Circle::new(Rider::PERSON_RADIUS),
                                down,
                                |_| (),
                            ));
                            assert!(off < 1e-3, "{} alights in a car", loaded.towns[town].name);
                            checked.insert((t, town));
                        }
                    }
                }
            }
            assert_eq!(
                checked.into_iter().collect::<Vec<_>>(),
                [(0, 0), (0, 1), (0, 2), (0, 3), (1, 2), (1, 4)]
            );
        }

        /// Los dos puntos de la vía de la estación alrededor del paso a nivel,
        /// para un test de segmentos.
        fn crossing_rail(loaded: &LoadedWorld) -> (Vec2, Vec2) {
            let y = loaded.towns[METILEO].railway.unwrap().start.y;
            (Vec2::new(0.0, y), Vec2::new(500.0, y))
        }

        #[test]
        fn the_shuttle_is_boarded_from_caleufus_middle_spot_all_through_its_layover() {
            let loaded = loaded();
            let (_, shuttle) = trains(loaded);
            let (middle, _) = loaded.towns[CALEUFU].platform_spot().unwrap();
            let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
            world.lay_rail(&loaded.line);
            let rider =
                pampa_core::Rider::on_foot(middle, pampa_core::Heading::from_radians(0.0), &world);
            let station = shuttle.station_of_town(CALEUFU).unwrap();
            let mut boarded = 0;
            for arrive in shuttle.arrivals_at(Run::Down, station) {
                let (leave, _) = shuttle.next_departure_after(arrive.seconds() + 1.0);
                // De la llegada a la salida: antes y después de que dé la
                // vuelta.
                for k in 0..=10 {
                    let t = arrive.seconds() + (leave - arrive.seconds()) * f64::from(k) / 10.0;
                    let now = jumped(&shuttle, t.clamp(arrive.seconds() + 1.0, leave - 1.0));
                    assert!(pampa_core::stands_at(&now.state(), station));
                    let got =
                        pampa_core::boarding(std::slice::from_ref(&now), &rider, &world.floors);
                    assert!(
                        matches!(got, pampa_core::Boarding::Board(_)),
                        "{t}: {got:?}"
                    );
                    boarded += 1;
                }
            }
            assert!(boarded >= 22, "{boarded}");
        }
    }
}
