//! Named places: the town's landmarks, with the signs that name them.
//!
//! Pure data derived from a [`TownLayout`]. Sign text is in-world (period
//! Spanish) content; render decides the lettering, the board and its height
//! on the facade.

use crate::{Aabb, Building, BuildingKind, Side, TownLayout, Vec2};

/// Which landmark a place is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceKind {
    GeneralStore,
    PoliceStation,
    Church,
    Station,
    Plaza,
}

impl PlaceKind {
    /// Every landmark kind, in a stable order.
    pub const ALL: [PlaceKind; 5] = [
        PlaceKind::GeneralStore,
        PlaceKind::PoliceStation,
        PlaceKind::Church,
        PlaceKind::Station,
        PlaceKind::Plaza,
    ];

    /// The diegetic name painted on its sign (uppercase Spanish). The
    /// station's sign adds the town's name ([`Place::name`]).
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

/// How a sign is held up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignMount {
    /// Flat against a building facade, above the door.
    Facade,
    /// A free-standing board on two posts.
    Posts,
}

/// One signboard: where it stands on the ground plane and which way it reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sign {
    /// Center of the board's base line on the ground plane. Facade signs sit
    /// just outside the wall; post signs stand on this point.
    pub anchor: Vec2,
    /// The side the lettering faces (outward normal of the board).
    pub facing: Side,
    pub mount: SignMount,
    /// Height of the surface the sign stands on above the ground (a platform).
    pub elevation: f32,
    /// Board width along its face, in meters.
    pub width: f32,
}

impl Sign {
    /// Thickness of a free-standing sign across its face, in meters.
    pub const POST_DEPTH: f32 = 0.3;

    /// Collision footprint of a free-standing sign standing on the ground,
    /// if it has one (facade signs live on walls, raised signs on platforms).
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

/// A named landmark of the town.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub kind: PlaceKind,
    /// The diegetic name painted on its signs: the kind's name, plus the
    /// town's name for the station (e.g. "ESTACIÓN EDUARDO CASTEX").
    pub name: String,
    /// The landmark's ground area (building footprint, or the plaza).
    pub footprint: Aabb,
    /// Signs naming it; the first is the main, street-facing one.
    pub signs: Vec<Sign>,
}

impl Place {
    /// Where the place is marked on a map.
    pub fn center(&self) -> Vec2 {
        self.footprint.center()
    }
}

impl TownLayout {
    /// The town's named landmarks in [`PlaceKind::ALL`] order: every general
    /// store, police station and plaza (bigger towns have several; the
    /// first is the main one), the church and the station.
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
            // Narrow enough for the face of the bell tower above the door.
            PlaceKind::Church => building(BuildingKind::Church)
                .map(|church| {
                    (
                        church.footprint,
                        vec![facade_sign(church.footprint, church.front, 3.4)],
                    )
                })
                .into_iter()
                .collect(),
            PlaceKind::Station => building(BuildingKind::Station)
                .map(|station| {
                    let mut signs = vec![facade_sign(station.footprint, station.front, 14.0)];
                    if let Some(platform) = building(BuildingKind::Platform) {
                        signs.push(platform_board(platform, station));
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

    /// Footprints of free-standing signs on the ground.
    pub(crate) fn sign_obstacles(&self) -> Vec<Aabb> {
        self.places()
            .iter()
            .flat_map(|p| p.signs.iter().filter_map(Sign::footprint))
            .collect()
    }
}

/// The plaza's sign on posts just inside its south edge, beside the central
/// path (closer to it on a narrow plaza).
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

/// Gap between a wall and the board hung on it, in meters.
const FACADE_GAP: f32 = 0.1;

/// Margin a facade board leaves to the wall's ends, in meters.
const FACADE_MARGIN: f32 = 0.5;

/// A board of `width` (narrowed to fit a small facade) centered on the
/// `front` facade of `footprint`.
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

/// The station name board on posts at the track edge of the platform, past
/// the west (or south) end of the station canopy, reading toward the passing
/// trains. The station faces the town, so the track is behind it.
fn platform_board(platform: &Building, station: &Building) -> Sign {
    let width = 7.0;
    let track = station.front.opposite();
    let n = side_normal(track);
    let along = Vec2::new(n.y.abs(), n.x.abs());
    let (p, s) = (platform.footprint, station.footprint);
    // Where the board stands along the track, and how far toward it.
    let at = s.min.dot(along) - 2.5 - width / 2.0;
    let depth = p.size().dot(Vec2::new(n.x.abs(), n.y.abs()));
    let toward = p.center().dot(n) + depth / 2.0 - 0.8;
    let base = p.center();
    Sign {
        anchor: base + along * (at - base.dot(along)) + n * (toward - base.dot(n)),
        facing: track,
        mount: SignMount::Posts,
        elevation: platform.height,
        width,
    }
}

/// Outward unit normal of a footprint side.
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
            // Bigger towns have several general stores, police stations and
            // plazas, listed together.
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
                assert!(p.name.starts_with(p.kind.name()), "{}", p.name);
                assert!(!p.signs.is_empty(), "{:?} has a sign", p.kind);
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
        assert_eq!(
            place(&town(), PlaceKind::GeneralStore).unwrap().name,
            "ALMACÉN DE RAMOS GENERALES"
        );
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
                (PlaceKind::Church, BuildingKind::Church),
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
                // Outside the wall, but only just, on the facade side.
                assert!(!b.footprint.contains_point(sign.anchor), "{kind:?}");
                let gap = b.footprint.closest_point(sign.anchor).distance(sign.anchor);
                assert!(gap > 0.0 && gap < 0.3, "{kind:?} gap {gap}");
                let normal = side_normal(b.front);
                assert!((sign.anchor - b.footprint.center()).dot(normal) > 0.0);
                // Centered on the facade and narrower than it.
                let along = Vec2::new(normal.y.abs(), normal.x.abs());
                let offset = (sign.anchor - b.footprint.center()).dot(along);
                assert!(offset.abs() < 1e-4, "{kind:?} off center by {offset}");
                assert!(
                    sign.width < b.footprint.size().dot(along),
                    "{}: {kind:?} sign wider than its facade",
                    t.name
                );
                // A street lies a few meters out in front of it.
                let faces_street = (6..=12).any(|d| {
                    let ahead = sign.anchor + normal * d as f32;
                    t.streets.iter().any(|s| s.contains_point(ahead))
                });
                assert!(faces_street, "{}: {kind:?} faces a street", t.name);
            }
        }
    }

    #[test]
    fn the_station_also_has_a_name_board_on_the_platform_facing_the_track() {
        for t in sample_towns() {
            let (Some(platform), Some(station_building), Some(railway)) = (
                building(&t, BuildingKind::Platform),
                building(&t, BuildingKind::Station),
                t.railway,
            ) else {
                continue;
            };
            let station = place(&t, PlaceKind::Station).unwrap();
            assert_eq!(station.signs.len(), 2);
            let board = station.signs[1];
            assert_eq!(board.mount, SignMount::Posts);
            let track = station_building.front.opposite();
            assert_eq!(board.facing, track, "{}", t.name);
            assert!(
                platform.footprint.contains_point(board.anchor),
                "{}",
                t.name
            );
            assert_eq!(board.elevation, platform.height);
            let n = side_normal(track);
            assert!(board.anchor.dot(n) < railway.start.dot(n), "{}", t.name);
            assert_eq!(board.footprint(), None, "raised on the platform");
            // Clear of the station's own canopy over the platform.
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let (a, half) = (board.anchor.dot(along), board.width / 2.0);
            let (lo, hi) = (
                station_building.footprint.min.dot(along),
                station_building.footprint.max.dot(along),
            );
            assert!(a - half > hi || a + half < lo, "{}", t.name);
            // Within the platform along the track.
            let (plo, phi) = (
                platform.footprint.min.dot(along),
                platform.footprint.max.dot(along),
            );
            assert!(a - half >= plo && a + half <= phi, "{}", t.name);
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
                // Clear of the central path so walkers and riders can enter.
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
