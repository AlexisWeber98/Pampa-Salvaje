//! Lugares con nombre: los puntos de referencia del pueblo, con los carteles
//! que los nombran.
//!
//! Datos puros derivados de un [`TownLayout`]. El texto de los carteles es
//! contenido del mundo del juego (español de época); el render decide las
//! letras, el tablero y su altura en la fachada.

use crate::{Aabb, Building, BuildingKind, Heading, Side, TownLayout, TownSize, Vec2};

/// Qué punto de referencia es un lugar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    GeneralStore,
    PoliceStation,
    Church,
    Station,
    Plaza,
}

impl PlaceKind {
    /// Todos los tipos de punto de referencia, en un orden estable.
    pub const ALL: [PlaceKind; 5] = [
        PlaceKind::GeneralStore,
        PlaceKind::PoliceStation,
        PlaceKind::Church,
        PlaceKind::Station,
        PlaceKind::Plaza,
    ];

    /// El nombre diegético pintado en su cartel (en español, en mayúsculas),
    /// como en un pueblo grande: el comercio de un pueblo chico es en cambio
    /// una pulpería ([`StoreStyle`]). El cartel de la estación agrega el nombre
    /// del pueblo ([`Place::name`]).
    pub fn name(self) -> &'static str {
        match self {
            PlaceKind::GeneralStore => "ALMACÉN DE RAMOS GENERALES",
            PlaceKind::PoliceStation => "COMISARÍA",
            PlaceKind::Church => "IGLESIA",
            PlaceKind::Station => "ESTACIÓN",
            PlaceKind::Plaza => "PLAZA",
        }
    }
}

/// Qué es el almacén de ramos generales de un pueblo, según el tamaño del
/// pueblo (usuario, 2026-10-23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreStyle {
    /// El comercio y bar de un pueblo chico: una pulpería.
    Pulperia,
    /// El "almacén de ramos generales" de un pueblo grande.
    GeneralStore,
}

impl StoreStyle {
    /// El comercio de un pueblo de tamaño `size`: una pulpería en un pueblo
    /// chico.
    pub fn of(size: TownSize) -> Self {
        match size {
            TownSize::Small => Self::Pulperia,
            TownSize::Big | TownSize::VeryBig => Self::GeneralStore,
        }
    }

    /// El nombre diegético pintado en su cartel (en español, en mayúsculas).
    pub fn sign(self) -> &'static str {
        match self {
            Self::Pulperia => "PULPERÍA",
            Self::GeneralStore => PlaceKind::GeneralStore.name(),
        }
    }
}

/// Cómo se sostiene un cartel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignMount {
    /// Pegado a la fachada de un edificio, sobre la puerta.
    Facade,
    /// Un tablero independiente sobre dos postes.
    Posts,
}

/// Un cartel: dónde está en el plano del suelo y hacia qué lado se lee.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sign {
    /// Centro de la línea de base del tablero en el plano del suelo. Los
    /// carteles de fachada quedan justo afuera de la pared; los carteles sobre
    /// postes se apoyan en este punto.
    pub anchor: Vec2,
    /// El lado hacia el que miran las letras (la normal hacia afuera del
    /// tablero).
    pub facing: Side,
    pub mount: SignMount,
    /// Altura sobre el suelo de la superficie en la que está el cartel (un
    /// andén).
    pub elevation: f32,
    /// Ancho del tablero a lo largo de su cara, en metros.
    pub width: f32,
}

impl Sign {
    /// Espesor de un cartel independiente a través de su cara, en metros.
    pub const POST_DEPTH: f32 = 0.3;

    /// Huella de colisión de un cartel independiente apoyado en el suelo, si
    /// tiene una (los carteles de fachada están en las paredes y los elevados,
    /// en los andenes).
    pub fn footprint(&self) -> Option<Aabb> {
        if self.mount != SignMount::Posts || self.elevation > 0.0 {
            return None;
        }
        let size = match self.facing {
            Side::North | Side::South => Vec2::new(self.width, Self::POST_DEPTH),
            Side::East | Side::West => Vec2::new(Self::POST_DEPTH, self.width),
        };
        Some(Aabb::from_center_size(self.anchor, size))
    }
}

/// Un punto de referencia del pueblo con nombre.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub kind: PlaceKind,
    /// El nombre diegético pintado en sus carteles: el nombre del tipo, más el
    /// nombre del pueblo en el caso de la estación (por ejemplo, "ESTACIÓN
    /// EDUARDO CASTEX").
    pub name: String,
    /// El área del suelo del punto de referencia (la huella del edificio, o la
    /// plaza).
    pub footprint: Aabb,
    /// Carteles que lo nombran; el primero es el principal, el que da a la
    /// calle.
    pub signs: Vec<Sign>,
}

impl Place {
    /// Dónde se marca el lugar en un mapa.
    pub fn center(&self) -> Vec2 {
        self.footprint.center()
    }
}

impl TownLayout {
    /// Los puntos de referencia con nombre del pueblo en el orden de
    /// [`PlaceKind::ALL`]: cada almacén de ramos generales, comisaría y plaza
    /// (los pueblos más grandes tienen varios; el primero es el principal), la
    /// iglesia y la estación.
    pub fn places(&self) -> Vec<Place> {
        PlaceKind::ALL
            .into_iter()
            .flat_map(|kind| self.places_of(kind))
            .collect()
    }

    fn places_of(&self, kind: PlaceKind) -> Vec<Place> {
        let building = |wanted: BuildingKind| self.buildings.iter().find(|b| b.kind == wanted);
        let areas: Vec<(Aabb, Vec<Sign>)> = match kind {
            PlaceKind::GeneralStore => self
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::GeneralStore)
                .map(|store| {
                    (
                        store.footprint,
                        vec![facade_sign(store.footprint, store.front, 13.0)],
                    )
                })
                .collect(),
            PlaceKind::PoliceStation => self
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::PoliceStation)
                .map(|police| {
                    (
                        police.footprint,
                        vec![facade_sign(police.footprint, police.front, 7.0)],
                    )
                })
                .collect(),
            // Sin cartel: el campanario dice lo que es (usuario, 2026-10-28).
            PlaceKind::Church => building(BuildingKind::Church)
                .map(|church| (church.footprint, Vec::new()))
                .into_iter()
                .collect(),
            PlaceKind::Station => building(BuildingKind::Station)
                .map(|station| {
                    let mut signs = vec![facade_sign(station.footprint, station.front, 14.0)];
                    if let Some(platform) = building(BuildingKind::Platform) {
                        signs.extend(platform_boards(platform, station));
                    }
                    (station.footprint, signs)
                })
                .into_iter()
                .collect(),
            PlaceKind::Plaza => self
                .plazas()
                .map(|plaza| (plaza, vec![plaza_sign(plaza)]))
                .collect(),
        };
        let name = match kind {
            PlaceKind::Station => format!("{} {}", kind.name(), self.name.to_uppercase()),
            PlaceKind::GeneralStore => StoreStyle::of(self.size).sign().to_string(),
            _ => kind.name().to_string(),
        };
        areas
            .into_iter()
            .map(|(footprint, signs)| Place {
                kind,
                name: name.clone(),
                footprint,
                signs,
            })
            .collect()
    }

    /// Dónde espera alguien en la plaza principal: en el sendero que va del
    /// monumento hacia la estación (hacia el sur si no hay estación), a unos
    /// metros, mirando hacia quien entre caminando desde ahí.
    pub fn plaza_spot(&self) -> (Vec2, Heading) {
        let center = self.plaza.center();
        let out = self
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .map_or(Vec2::new(0.0, -1.0), |station| {
                axis_toward(station.footprint.center() - center)
            });
        (center + out * PLAZA_SPOT_DISTANCE, heading_of(out))
    }

    /// Dónde está parado alguien en la puerta de la primera comisaría: del lado
    /// de la calle, a un par de metros del centro de su fachada, mirando hacia
    /// la calle.
    pub fn police_door(&self) -> Option<(Vec2, Heading)> {
        let police = self
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::PoliceStation)?;
        let normal = side_normal(police.front);
        let half = police.footprint.size() * 0.5;
        let reach = normal.x.abs() * half.x + normal.y.abs() * half.y + DOOR_SPOT_DISTANCE;
        Some((
            police.footprint.center() + normal * reach,
            heading_of(normal),
        ))
    }

    /// Dónde baja un pasajero del tren: sobre el andén, a mitad de su largo
    /// (donde se detiene el centro de los coches), un paso hacia adentro desde
    /// su borde junto a la vía, mirando hacia el pueblo. `None` si no hay
    /// andén.
    pub fn platform_spot(&self) -> Option<(Vec2, Heading)> {
        let platform = self
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)?;
        // El andén mira hacia la vía. La estación, y por lo tanto los coches
        // que se detienen frente a ella, están centrados en el andén a lo largo
        // de la vía.
        let track = side_normal(platform.front);
        let p = platform.footprint;
        let half = p.size() * 0.5;
        let half_depth = track.x.abs() * half.x + track.y.abs() * half.y;
        let at = p.center() + track * (half_depth - PLATFORM_SPOT_FROM_EDGE);
        Some((at, heading_of(track * -1.0)))
    }

    /// Huellas de los carteles independientes apoyados en el suelo.
    pub(crate) fn sign_obstacles(&self) -> Vec<Aabb> {
        self.places()
            .iter()
            .flat_map(|p| p.signs.iter().filter_map(Sign::footprint))
            .collect()
    }
}

/// El cartel de la plaza sobre postes, justo dentro de su borde sur, al lado
/// del sendero central (más cerca de él en una plaza angosta).
fn plaza_sign(plaza: Aabb) -> Sign {
    let offset = (plaza.size().x / 2.0 - 3.5).min(10.0);
    Sign {
        anchor: Vec2::new(plaza.center().x + offset, plaza.min.y + 1.0),
        facing: Side::South,
        mount: SignMount::Posts,
        elevation: 0.0,
        width: 3.0,
    }
}

/// A qué distancia del monumento de la plaza está [`TownLayout::plaza_spot`],
/// en metros: más allá de la base del monumento, bien dentro del anillo de
/// árboles.
const PLAZA_SPOT_DISTANCE: f32 = 6.0;
/// A qué distancia de una fachada está [`TownLayout::police_door`], en metros.
const DOOR_SPOT_DISTANCE: f32 = 2.0;
/// A qué distancia hacia adentro del borde del andén junto a la vía está
/// [`TownLayout::platform_spot`], en metros: la vía pasa a
/// [`Floor::PLATFORM_EDGE`] (1,65 m) de ese borde, así que el lugar queda a
/// 2,65 m de la vía, a 1,15 m de un tren de 3 m de ancho, y lejos de los postes
/// del techo de la estación.
pub(crate) const PLATFORM_SPOT_FROM_EDGE: f32 = 1.0;

/// El eje del mundo más cercano a la dirección de `v` (el sur si es cero).
fn axis_toward(v: Vec2) -> Vec2 {
    if v.x.abs() > v.y.abs() {
        Vec2::new(v.x.signum(), 0.0)
    } else if v.y != 0.0 {
        Vec2::new(0.0, v.y.signum())
    } else {
        Vec2::new(0.0, -1.0)
    }
}

/// La orientación que apunta en la dirección unitaria `d`.
fn heading_of(d: Vec2) -> Heading {
    Heading::from_radians(d.y.atan2(d.x))
}

/// Separación entre una pared y el tablero colgado en ella, en metros.
const FACADE_GAP: f32 = 0.1;

/// Margen que deja un tablero de fachada hasta los extremos de la pared, en
/// metros.
const FACADE_MARGIN: f32 = 0.5;

/// Un tablero de ancho `width` (angostado para que entre en una fachada chica)
/// centrado en la fachada `front` de `footprint`.
fn facade_sign(footprint: Aabb, front: Side, width: f32) -> Sign {
    let normal = side_normal(front);
    let half = footprint.size() * 0.5;
    let facade = normal.y.abs() * footprint.size().x + normal.x.abs() * footprint.size().y;
    let width = width.min(facade - 2.0 * FACADE_MARGIN);
    let reach = normal.x.abs() * half.x + normal.y.abs() * half.y + FACADE_GAP;
    Sign {
        anchor: footprint.center() + normal * reach,
        facing: front,
        mount: SignMount::Facade,
        elevation: 0.0,
        width,
    }
}

/// A qué distancia hacia adentro de su huella están las paredes de la estación,
/// en metros (el `PROUD` del render): las paredes son esta medida más cortas en
/// cada extremo.
pub const STATION_WALL_INSET: f32 = 0.1;

/// Vanos (aberturas, con puertas en los impares) en una pared de estación de
/// `length` metros de largo: uno cada 3 m, de 3 a 9, siempre una cantidad impar
/// para que haya una puerta en el medio.
pub fn station_bays(length: f32) -> usize {
    ((length / 3.0) as usize).clamp(3, 9) | 1
}

/// Ancho del tablero de horarios en la pared de la estación, en metros.
pub const TIMETABLE_BOARD_WIDTH: f32 = 1.4;
/// Altura del centro del tablero de horarios sobre el piso del andén, en
/// metros: al lado de las puertas y ventanas, a la altura de los ojos.
pub const TIMETABLE_BOARD_HEIGHT: f32 = 1.7;

impl TownLayout {
    /// Los tres tableros de horarios de la estación, cada uno un [`Sign`] de
    /// fachada colgado en un pilar entre aberturas, donde no hay puerta ni
    /// ventana: primero el de la pared del lado de la vía, mirando hacia el
    /// andén (la estación mira hacia el pueblo, así que la vía queda detrás), y
    /// después los dos del frente que da a la calle, uno a cada lado del vano
    /// de entrada, mirando hacia la calle. Vacío si no hay estación.
    pub fn timetable_boards(&self) -> Vec<Sign> {
        let Some(station) = self
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
        else {
            return Vec::new();
        };
        // El piso del andén sobre el que está el tablero del lado de la vía.
        let platform = self
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .map_or(0.0, |p| p.height);
        let footprint = station.footprint;
        let board = |facing: Side, side: f32, elevation: f32| {
            let normal = side_normal(facing);
            let along = Vec2::new(normal.y.abs(), normal.x.abs());
            let length = footprint.size().dot(along) - 2.0 * STATION_WALL_INSET;
            // El borde entre el vano del medio y su vecino: las aberturas están
            // centradas en sus vanos, así que queda libre de todas ellas.
            let pier = length / (2.0 * station_bays(length) as f32);
            let half = footprint.size() * 0.5;
            let reach = normal.x.abs() * half.x + normal.y.abs() * half.y + FACADE_GAP;
            Sign {
                anchor: footprint.center() + normal * reach + along * (side * pier),
                facing,
                mount: SignMount::Facade,
                elevation,
                width: TIMETABLE_BOARD_WIDTH,
            }
        };
        vec![
            board(station.front.opposite(), 1.0, platform),
            board(station.front, -1.0, 0.0),
            board(station.front, 1.0, 0.0),
        ]
    }
}

/// Ancho de cada tablero con el nombre de la estación en el andén, en metros:
/// un tablero largo de época, pero lo bastante corto como para quedar lejos de
/// los extremos del andén.
const PLATFORM_BOARD_WIDTH: f32 = 5.0;
/// Separación desde el extremo de un andén hasta el tablero más cercano, en
/// metros.
const PLATFORM_BOARD_MARGIN: f32 = 1.5;
/// A qué distancia hacia adentro del borde del andén junto a la vía está un
/// tablero, en metros.
const PLATFORM_BOARD_FROM_EDGE: f32 = 0.8;

/// Los dos tableros con el nombre de la estación sobre postes en el borde del
/// andén junto a la vía, uno cerca de cada extremo, que se leen hacia los
/// trenes para que un pasajero que mira por la ventanilla de un coche vea el
/// nombre. Los extremos quedan más allá del tramo frente al que se detienen los
/// coches, así que un pasajero nunca baja frente a uno. La estación mira hacia
/// el pueblo, así que la vía queda detrás.
fn platform_boards(platform: &Building, station: &Building) -> [Sign; 2] {
    let track = station.front.opposite();
    let n = side_normal(track);
    let along = Vec2::new(n.y.abs(), n.x.abs());
    let p = platform.footprint;
    let half_length = p.size().dot(along) / 2.0;
    let half_depth = p.size().dot(Vec2::new(n.x.abs(), n.y.abs())) / 2.0;
    let reach = half_length - PLATFORM_BOARD_MARGIN - PLATFORM_BOARD_WIDTH / 2.0;
    let edge = p.center() + n * (half_depth - PLATFORM_BOARD_FROM_EDGE);
    [-1.0, 1.0].map(|end| Sign {
        anchor: edge + along * (end * reach),
        facing: track,
        mount: SignMount::Posts,
        elevation: platform.height,
        width: PLATFORM_BOARD_WIDTH,
    })
}

/// Normal unitaria hacia afuera de un lado de una huella.
pub fn side_normal(side: Side) -> Vec2 {
    match side {
        Side::North => Vec2::new(0.0, 1.0),
        Side::South => Vec2::new(0.0, -1.0),
        Side::East => Vec2::new(1.0, 0.0),
        Side::West => Vec2::new(-1.0, 0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::town::tests::{sample_configs, sample_towns};

    fn town() -> TownLayout {
        TownLayout::castex_inspired()
    }

    fn building(t: &TownLayout, kind: BuildingKind) -> Option<Building> {
        t.buildings.iter().find(|b| b.kind == kind).copied()
    }

    fn all(t: &TownLayout, kind: BuildingKind) -> usize {
        t.buildings.iter().filter(|b| b.kind == kind).count()
    }

    fn place(t: &TownLayout, kind: PlaceKind) -> Option<Place> {
        t.places().into_iter().find(|p| p.kind == kind)
    }

    #[test]
    fn one_place_per_landmark_with_its_spanish_name() {
        for (c, t) in sample_configs().iter().zip(sample_towns()) {
            let places = t.places();
            let kinds: Vec<_> = places.iter().map(|p| p.kind).collect();
            // Los pueblos más grandes tienen varios almacenes de ramos
            // generales, comisarías y plazas, que se listan juntos.
            let expected: Vec<_> = PlaceKind::ALL
                .into_iter()
                .flat_map(|k| {
                    let n = match k {
                        PlaceKind::GeneralStore => all(&t, BuildingKind::GeneralStore),
                        PlaceKind::PoliceStation => all(&t, BuildingKind::PoliceStation),
                        PlaceKind::Church => 1,
                        PlaceKind::Station => usize::from(c.railway.is_some()),
                        PlaceKind::Plaza => t.plazas().count(),
                    };
                    std::iter::repeat_n(k, n)
                })
                .collect();
            assert_eq!(kinds, expected, "{}", c.name);
            for p in &places {
                assert!(!p.name.trim().is_empty());
                assert_eq!(p.name, p.name.to_uppercase(), "{}", p.name);
                let sign = match p.kind {
                    PlaceKind::GeneralStore => StoreStyle::of(t.size).sign(),
                    kind => kind.name(),
                };
                assert!(p.name.starts_with(sign), "{}", p.name);
                // La iglesia no lleva cartel (usuario, 2026-10-28): su
                // campanario dice lo que es.
                assert_eq!(
                    p.signs.is_empty(),
                    p.kind == PlaceKind::Church,
                    "{:?} signs",
                    p.kind
                );
                assert!(p.signs.iter().all(|s| s.width > 0.5));
            }
            if let Some(station) = place(&t, PlaceKind::Station) {
                assert_eq!(
                    station.name,
                    format!("ESTACIÓN {}", c.name.to_uppercase()),
                    "the station carries the town's name"
                );
            }
        }
        // El comercio de un pueblo chico es una pulpería; el de un pueblo
        // grande, un almacén de ramos generales.
        assert_eq!(town().size, crate::TownSize::Small);
        assert_eq!(
            place(&town(), PlaceKind::GeneralStore).unwrap().name,
            "PULPERÍA"
        );
        let big = TownLayout::generate(&crate::TownConfig {
            blocks: (8, 8),
            ..crate::TownConfig::castex()
        })
        .unwrap();
        for store in big
            .places()
            .iter()
            .filter(|p| p.kind == PlaceKind::GeneralStore)
        {
            assert_eq!(store.name, "ALMACÉN DE RAMOS GENERALES");
        }
        assert_eq!(
            place(&town(), PlaceKind::Station).unwrap().name,
            "ESTACIÓN EDUARDO CASTEX"
        );
    }

    #[test]
    fn places_list_every_plaza_general_store_and_police_station() {
        for t in sample_towns() {
            let footprints = |kind: PlaceKind| -> Vec<Aabb> {
                t.places()
                    .into_iter()
                    .filter(|p| p.kind == kind)
                    .map(|p| p.footprint)
                    .collect()
            };
            assert_eq!(
                footprints(PlaceKind::Plaza),
                t.plazas().collect::<Vec<_>>(),
                "{}",
                t.name
            );
            let stores: Vec<Aabb> = t
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::GeneralStore)
                .map(|b| b.footprint)
                .collect();
            assert_eq!(footprints(PlaceKind::GeneralStore), stores, "{}", t.name);
            let police: Vec<Aabb> = t
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::PoliceStation)
                .map(|b| b.footprint)
                .collect();
            assert_eq!(footprints(PlaceKind::PoliceStation), police, "{}", t.name);
        }
    }

    #[test]
    fn places_are_deterministic() {
        assert_eq!(town().places(), town().places());
    }

    #[test]
    fn building_signs_hang_just_outside_the_street_facade() {
        for t in sample_towns() {
            for (kind, building_kind) in [
                (PlaceKind::GeneralStore, BuildingKind::GeneralStore),
                (PlaceKind::PoliceStation, BuildingKind::PoliceStation),
                (PlaceKind::Station, BuildingKind::Station),
            ] {
                let Some(b) = building(&t, building_kind) else {
                    continue;
                };
                let p = place(&t, kind).unwrap();
                assert_eq!(p.footprint, b.footprint);
                let sign = p.signs[0];
                assert_eq!(sign.mount, SignMount::Facade);
                assert_eq!(sign.facing, b.front, "{kind:?}");
                assert_eq!(sign.elevation, 0.0);
                // Afuera de la pared, pero apenas, del lado de la fachada.
                assert!(!b.footprint.contains_point(sign.anchor), "{kind:?}");
                let gap = b.footprint.closest_point(sign.anchor).distance(sign.anchor);
                assert!(gap > 0.0 && gap < 0.3, "{kind:?} gap {gap}");
                let normal = side_normal(b.front);
                assert!((sign.anchor - b.footprint.center()).dot(normal) > 0.0);
                // Centrado en la fachada y más angosto que ella.
                let along = Vec2::new(normal.y.abs(), normal.x.abs());
                let offset = (sign.anchor - b.footprint.center()).dot(along);
                assert!(offset.abs() < 1e-4, "{kind:?} off center by {offset}");
                assert!(
                    sign.width < b.footprint.size().dot(along),
                    "{}: {kind:?} sign wider than its facade",
                    t.name
                );
                // Hay una calle unos metros más adelante.
                let faces_street = (6..=12).any(|d| {
                    let ahead = sign.anchor + normal * d as f32;
                    t.streets.iter().any(|s| s.contains_point(ahead))
                });
                assert!(faces_street, "{}: {kind:?} faces a street", t.name);
            }
        }
    }

    /// La mitad del tramo de andén frente al que se detienen los coches del
    /// tren de la línea, en metros: el tren se detiene con sus coches centrados
    /// en el medio del andén (ver `platform_spot`).
    fn coaches_half() -> f32 {
        let (first, last) = crate::train::coaches_span(&crate::train::line_consist()).unwrap();
        (last - first) / 2.0
    }

    #[test]
    fn the_station_has_a_name_board_at_each_end_of_the_platform_facing_the_track() {
        for t in sample_towns() {
            let (Some(platform), Some(station_building), Some(railway)) = (
                building(&t, BuildingKind::Platform),
                building(&t, BuildingKind::Station),
                t.railway,
            ) else {
                continue;
            };
            let station = place(&t, PlaceKind::Station).unwrap();
            assert_eq!(station.signs.len(), 3, "{}: facade + two boards", t.name);
            let track = station_building.front.opposite();
            let n = side_normal(track);
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let middle = platform.footprint.center().dot(along);
            let (plo, phi) = (
                platform.footprint.min.dot(along),
                platform.footprint.max.dot(along),
            );
            let mut ends = Vec::new();
            for board in &station.signs[1..] {
                assert_eq!(board.mount, SignMount::Posts);
                assert_eq!(board.facing, track, "{}", t.name);
                assert!(
                    platform.footprint.contains_point(board.anchor),
                    "{}",
                    t.name
                );
                assert_eq!(board.elevation, platform.height);
                // En el borde del andén junto a la vía, del lado cercano de los
                // rieles.
                assert!(board.anchor.dot(n) < railway.start.dot(n), "{}", t.name);
                let edge = platform.footprint.center().dot(n)
                    + platform
                        .footprint
                        .size()
                        .dot(Vec2::new(n.x.abs(), n.y.abs()))
                        / 2.0;
                assert!(edge - board.anchor.dot(n) < 1.0, "{}: by the edge", t.name);
                assert_eq!(board.footprint(), None, "raised on the platform");
                // Un tablero de época: largo, pero no tanto como el andén
                // entero.
                assert!((4.0..=6.0).contains(&board.width), "{}", t.name);
                // Dentro del andén a lo largo de la vía, con un margen.
                let (a, half) = (board.anchor.dot(along), board.width / 2.0);
                assert!(a - half >= plo + 1.0 && a + half <= phi - 1.0, "{}", t.name);
                // Lejos del tramo frente al que se detienen los coches, que
                // contiene todos los lugares de descenso y todas las puertas de
                // los coches.
                assert!(
                    (a - middle).abs() - half >= coaches_half() + 1.0,
                    "{}",
                    t.name
                );
                ends.push((a - middle).signum());
            }
            ends.sort_by(f32::total_cmp);
            assert_eq!(ends, [-1.0, 1.0], "{}: one at each end", t.name);
        }
    }

    #[test]
    fn every_station_has_three_timetable_boards_one_on_each_wall_side_of_the_entrance() {
        let mut stations = 0;
        for t in sample_towns() {
            let boards = t.timetable_boards();
            let Some(station) = building(&t, BuildingKind::Station) else {
                assert!(boards.is_empty(), "{}", t.name);
                continue;
            };
            stations += 1;
            assert_eq!(boards.len(), 3, "{}", t.name);
            let platform = building(&t, BuildingKind::Platform).unwrap();
            // Primero el del lado de la vía, después los dos del frente que da
            // a la calle.
            let wanted = [
                (station.front.opposite(), platform.height),
                (station.front, 0.0),
                (station.front, 0.0),
            ];
            let mut street = Vec::new();
            for (board, (facing, elevation)) in boards.iter().zip(wanted) {
                assert_eq!(board.facing, facing, "{}", t.name);
                assert_eq!(board.mount, SignMount::Facade);
                assert_eq!(board.elevation, elevation, "{}", t.name);
                assert!(board.width < 2.0);
                let n = side_normal(board.facing);
                let wall = station.footprint.center().dot(n)
                    + station
                        .footprint
                        .size()
                        .dot(Vec2::new(n.x.abs(), n.y.abs()))
                        / 2.0;
                let gap = board.anchor.dot(n) - wall;
                assert!((gap - 0.1).abs() < 1e-4, "{}: gap {gap}", t.name);
                // Dentro del largo de la pared, a un ancho de pilar de su
                // centro.
                let along = Vec2::new(n.y.abs(), n.x.abs());
                let offset = (board.anchor - station.footprint.center()).dot(along);
                assert!(offset.abs() > 0.5, "{}: on the middle bay", t.name);
                assert!(
                    offset.abs() + board.width / 2.0 < station.footprint.size().dot(along) / 2.0,
                    "{}",
                    t.name
                );
                if board.facing == station.front {
                    street.push(offset);
                }
            }
            // Los tableros de la calle flanquean la entrada: a la misma
            // distancia a cada lado del centro.
            assert_eq!(street.len(), 2);
            assert!(
                (street[0] + street[1]).abs() < 1e-4,
                "{}: {street:?}",
                t.name
            );
            assert!(street[0] * street[1] < 0.0, "{}", t.name);
        }
        assert!(stations > 0);
    }

    #[test]
    fn station_bays_are_odd_between_three_and_nine() {
        assert_eq!(station_bays(1.0), 3);
        assert_eq!(station_bays(23.8), 7);
        assert_eq!(station_bays(24.0), 9);
        assert_eq!(station_bays(100.0), 9);
    }

    #[test]
    fn no_alighting_spot_meets_a_name_board() {
        for t in sample_towns() {
            let Some((spot, _)) = t.platform_spot() else {
                continue;
            };
            let platform = building(&t, BuildingKind::Platform).unwrap();
            let track = side_normal(platform.front);
            let along = Vec2::new(track.y.abs(), track.x.abs());
            let station = place(&t, PlaceKind::Station).unwrap();
            // Un pasajero baja al lado de cualquier puerta de coche, en
            // cualquier punto del tramo de los coches, un paso hacia adentro
            // desde el borde junto a la vía.
            for board in &station.signs[1..] {
                let reach = board.width / 2.0 + crate::Rider::PERSON_RADIUS;
                let a = board.anchor.dot(along);
                let lo = spot.dot(along) - coaches_half();
                let hi = spot.dot(along) + coaches_half();
                assert!(a + reach < lo || a - reach > hi, "{}: {board:?}", t.name);
            }
        }
    }

    #[test]
    fn the_plaza_sign_stands_inside_its_south_edge_clear_of_trees() {
        for t in sample_towns() {
            let plazas: Vec<Place> = t
                .places()
                .into_iter()
                .filter(|p| p.kind == PlaceKind::Plaza)
                .collect();
            assert_eq!(plazas.len(), t.plazas().count(), "{}", t.name);
            for (plaza, area) in plazas.iter().zip(t.plazas()) {
                assert_eq!(plaza.footprint, area);
                let sign = plaza.signs[0];
                assert_eq!(sign.mount, SignMount::Posts);
                assert_eq!(sign.facing, Side::South);
                let board = sign
                    .footprint()
                    .expect("a post sign on the ground collides");
                assert!(area.contains(&board), "{}: {board:?}", t.name);
                assert!(sign.anchor.y - area.min.y < 2.5);
                for &tree in &t.trees {
                    assert!(!board.expanded(0.5).contains_point(tree), "{tree:?}");
                }
                // Lejos del sendero central para que peatones y jinetes puedan
                // entrar.
                assert!((sign.anchor.x - area.center().x).abs() - sign.width / 2.0 > 2.0);
            }
        }
    }

    #[test]
    fn post_sign_footprints_span_the_board_across_its_facing() {
        let sign = Sign {
            anchor: Vec2::new(10.0, 5.0),
            facing: Side::South,
            mount: SignMount::Posts,
            elevation: 0.0,
            width: 3.0,
        };
        let close = |a: Vec2, b: Vec2| a.distance(b) < 1e-4;
        let area = sign.footprint().unwrap();
        assert!(close(area.center(), sign.anchor));
        assert!(close(area.size(), Vec2::new(3.0, Sign::POST_DEPTH)));
        let east = Sign {
            facing: Side::East,
            ..sign
        };
        assert!(close(
            east.footprint().unwrap().size(),
            Vec2::new(Sign::POST_DEPTH, 3.0)
        ));
        let facade = Sign {
            mount: SignMount::Facade,
            ..sign
        };
        assert_eq!(facade.footprint(), None);
    }

    /// Separación desde `p` hasta el obstáculo más cercano de `town`, en
    /// metros.
    fn clearance(t: &TownLayout, p: Vec2) -> f32 {
        t.obstacles()
            .iter()
            .map(|o| o.closest_point(p).distance(p))
            .fold(f32::INFINITY, f32::min)
    }

    #[test]
    fn the_plaza_spot_is_on_a_path_inside_the_main_plaza_and_clear() {
        for t in sample_towns() {
            let (at, heading) = t.plaza_spot();
            assert!(t.plaza.contains_point(at), "{}: {at:?}", t.name);
            let c = t.plaza.center();
            // En uno de los brazos de la cruz de senderos, lejos del monumento.
            let off = at - c;
            assert!(
                off.x.abs() < 1.0 || off.y.abs() < 1.0,
                "{}: {off:?}",
                t.name
            );
            let d = off.length();
            assert!(
                (4.0..=10.0).contains(&d),
                "{}: {d} m from the monument",
                t.name
            );
            // Mirando hacia afuera a lo largo del brazo, de espaldas al
            // monumento.
            assert!(heading.forward().dot(off) > 0.9 * d, "{}", t.name);
            assert!(clearance(&t, at) > 1.5, "{}: {}", t.name, clearance(&t, at));
            // Hacia la estación, cuando hay una.
            if let Some(station) = building(&t, BuildingKind::Station) {
                let to_station = station.footprint.center() - c;
                assert!(off.dot(to_station) > 0.0, "{}", t.name);
            }
        }
    }

    #[test]
    fn the_police_door_spot_is_on_the_street_in_front_of_the_door_and_clear() {
        for t in sample_towns() {
            let police = building(&t, BuildingKind::PoliceStation).unwrap();
            let (at, heading) = t.police_door().expect("every town has a police station");
            let normal = side_normal(police.front);
            assert_eq!(heading.forward().dot(normal).round(), 1.0, "{}", t.name);
            // Delante del centro de la fachada.
            let off = at - police.footprint.center();
            let along = Vec2::new(normal.y.abs(), normal.x.abs());
            assert!(off.dot(along).abs() < 1e-3, "{}", t.name);
            let gap = police.footprint.closest_point(at).distance(at);
            assert!((1.0..=3.0).contains(&gap), "{}: {gap}", t.name);
            // Fuera de todo obstáculo, con lugar para una persona al lado.
            assert!(clearance(&t, at) > 1.0, "{}: {}", t.name, clearance(&t, at));
            // Hay una calle justo adelante.
            let ahead = (0..=6).any(|d| {
                let p = at + normal * d as f32;
                t.streets.iter().any(|s| s.contains_point(p))
            });
            assert!(ahead, "{}", t.name);
        }
    }

    #[test]
    fn the_platform_spot_is_on_the_platform_by_the_track_facing_town() {
        let mut checked = 0;
        for t in sample_towns() {
            let Some(platform) = building(&t, BuildingKind::Platform) else {
                assert_eq!(t.platform_spot(), None, "{}", t.name);
                continue;
            };
            let railway = t.railway.unwrap();
            let (at, heading) = t.platform_spot().unwrap();
            let track = side_normal(platform.front);
            // Sobre el andén, lejos de todo obstáculo a más que el radio de una
            // persona.
            assert!(platform.footprint.contains_point(at), "{}", t.name);
            assert!(
                clearance(&t, at) > crate::Rider::PERSON_RADIUS + 0.5,
                "{}: {}",
                t.name,
                clearance(&t, at)
            );
            // Un paso hacia adentro desde el borde junto a la vía (a 1,65 m de
            // la vía), lejos de un tren detenido en la vía.
            let off_track = (railway.start - at).dot(track).abs();
            let car_half = crate::LineTrain::WIDTH / 2.0;
            assert!((off_track - 2.65).abs() < 0.01, "{}: {off_track}", t.name);
            assert!(
                off_track > car_half + crate::Rider::PERSON_RADIUS,
                "{}: {off_track}",
                t.name
            );
            // A mitad de su largo, donde se detienen los coches.
            let along = Vec2::new(track.y.abs(), track.x.abs());
            let middle = platform.footprint.center().dot(along);
            assert!((at.dot(along) - middle).abs() < 1e-3, "{}", t.name);
            // Mirando hacia el pueblo: de espaldas a la vía.
            assert_eq!(heading.forward().dot(track).round(), -1.0, "{}", t.name);
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn the_game_starts_on_the_platform_beside_the_dwelling_coaches() {
        use crate::{BASE_RATE, GameClock, LinePhase, LineTrain, Run};
        let (towns, line) = crate::rail::tests::shipped();
        let first = &towns[0];
        let fresh =
            LineTrain::new(&line, &towns, crate::Timetable::default()).expect("a line train");
        // El juego empieza con el tren de la mañana (de subida) desde Pico
        // detenido en el primer pueblo; el tren de la tarde (de bajada) desde
        // Castex se detiene en el mismo lugar, mirando hacia el otro lado
        // después de dar la vuelta en Castex.
        for run in [Run::Up, Run::Down] {
            let mut train = fresh.clone();
            let clock = GameClock::at(train.stop_at(0, run).unwrap());
            train.advance(0.0, clock.elapsed(), BASE_RATE);
            assert_eq!(train.state().phase, LinePhase::Dwelling);
            assert_eq!(train.state().run, run);
            beside_the_coaches(first, &towns, &train);
        }
    }

    /// El lugar del andén está sobre el andén, al lado del centro de los coches
    /// del tren detenido y lejos de todo.
    fn beside_the_coaches(first: &TownLayout, towns: &[TownLayout], train: &crate::LineTrain) {
        use crate::{CarKind, Circle, LineTrain, Rider};
        let mut world = crate::World::with_towns(
            crate::WorldConfig {
                half_extent: crate::WorldConfig::MAX_HALF_EXTENT,
                ..Default::default()
            },
            crate::Terrain::default(),
            towns,
        );
        world.moving = train.obstacles();
        let (at, heading) = first.platform_spot().unwrap();
        let platform = building(first, BuildingKind::Platform).unwrap();
        // Parado sobre el andén.
        let ground = world.terrain.height(at.x, at.y);
        assert_eq!(world.ground_height(at), ground + platform.height);
        let person = Rider::on_foot(at, heading, &world);
        assert_eq!(person.height, ground + platform.height);
        // Al lado del centro de los coches, justo enfrente de ellos.
        let coaches: Vec<_> = train
            .placed_cars()
            .into_iter()
            .filter(|c| c.kind == CarKind::Coach)
            .collect();
        assert_eq!(coaches.len(), 2);
        let middle = (coaches[0].center + coaches[1].center) * 0.5;
        let d = coaches[0].direction;
        let along = (at - middle).dot(d).abs();
        let across = (at - middle).dot(Vec2::new(-d.y, d.x)).abs();
        assert!(along < 1.0, "{along}");
        // A 2,65 m del centro de la vía: el borde del andén a 1,65 m y un paso
        // hacia adentro.
        assert!((across - 2.65).abs() < 0.01, "{across}");
        assert!(across > LineTrain::WIDTH / 2.0 + Rider::PERSON_RADIUS);
        // Lejos de cada vagón y de todo lo construido: nada los mueve.
        let c = Circle::new(Rider::PERSON_RADIUS);
        assert_eq!(world.resolve(c, at, |_| ()), at);
        for car in &world.moving {
            assert!(car.push_out(c, at).is_none(), "{car:?}");
        }
    }

    #[test]
    fn ground_post_signs_are_obstacles() {
        for t in sample_towns() {
            let obstacles = t.obstacles();
            for p in t.places() {
                for s in p.signs {
                    if let Some(area) = s.footprint() {
                        assert!(obstacles.contains(&area), "{:?}", p.kind);
                    }
                }
            }
        }
    }
}
