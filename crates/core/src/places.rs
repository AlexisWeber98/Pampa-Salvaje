//! Named places: the town's landmarks, with the signs that name them.
//!
//! Pure data derived from a [`TownLayout`]. Sign text is in-world (period
//! Spanish) content; render decides the lettering, the board and its height
//! on the facade.

use crate::{Aabb, BuildingKind, Side, TownLayout, Vec2};

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

    /// The diegetic name painted on its sign (uppercase Spanish).
    pub fn name(self) -> &'static str {
        match self {
            PlaceKind::GeneralStore => "ALMACÉN DE RAMOS GENERALES",
            PlaceKind::PoliceStation => "COMISARÍA",
            PlaceKind::Church => "IGLESIA",
            PlaceKind::Station => "ESTACIÓN EDUARDO CASTEX",
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
    /// The diegetic name painted on its signs.
    pub name: &'static str,
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
    /// The town's named landmarks, one per [`PlaceKind`], in [`PlaceKind::ALL`]
    /// order.
    pub fn places(&self) -> Vec<Place> {
        PlaceKind::ALL
            .into_iter()
            .filter_map(|kind| self.place(kind))
            .collect()
    }

    fn place(&self, kind: PlaceKind) -> Option<Place> {
        let building = |wanted: BuildingKind| self.buildings.iter().find(|b| b.kind == wanted);
        let (footprint, signs) = match kind {
            PlaceKind::GeneralStore => {
                let store = building(BuildingKind::GeneralStore)?;
                (
                    store.footprint,
                    vec![facade_sign(store.footprint, store.front, 13.0)],
                )
            }
            PlaceKind::PoliceStation => {
                let police = building(BuildingKind::PoliceStation)?;
                (
                    police.footprint,
                    vec![facade_sign(police.footprint, police.front, 7.0)],
                )
            }
            PlaceKind::Church => {
                // Narrow enough for the face of the bell tower above the door.
                let church = building(BuildingKind::Church)?;
                (
                    church.footprint,
                    vec![facade_sign(church.footprint, church.front, 3.4)],
                )
            }
            PlaceKind::Station => {
                let station = building(BuildingKind::Station)?;
                let mut signs = vec![facade_sign(station.footprint, station.front, 14.0)];
                if let Some(platform) = building(BuildingKind::Platform) {
                    signs.push(platform_board(
                        platform.footprint,
                        platform.height,
                        station.footprint,
                    ));
                }
                (station.footprint, signs)
            }
            PlaceKind::Plaza => {
                // On posts just inside the south edge, beside the central path.
                let anchor = Vec2::new(self.plaza.center().x + 10.0, self.plaza.min.y + 1.0);
                let sign = Sign {
                    anchor,
                    facing: Side::South,
                    mount: SignMount::Posts,
                    elevation: 0.0,
                    width: 3.0,
                };
                (self.plaza, vec![sign])
            }
        };
        Some(Place {
            kind,
            name: kind.name(),
            footprint,
            signs,
        })
    }

    /// Footprints of free-standing signs on the ground.
    pub(crate) fn sign_obstacles(&self) -> Vec<Aabb> {
        self.places()
            .iter()
            .flat_map(|p| p.signs.iter().filter_map(Sign::footprint))
            .collect()
    }
}

/// Gap between a wall and the board hung on it, in meters.
const FACADE_GAP: f32 = 0.1;

/// A board of `width` centered on the `front` facade of `footprint`.
fn facade_sign(footprint: Aabb, front: Side, width: f32) -> Sign {
    let normal = side_normal(front);
    let half = footprint.size() * 0.5;
    let reach = normal.x.abs() * half.x + normal.y.abs() * half.y + FACADE_GAP;
    Sign {
        anchor: footprint.center() + normal * reach,
        facing: front,
        mount: SignMount::Facade,
        elevation: 0.0,
        width,
    }
}

/// The station name board on posts at the track edge of the platform, west
/// of the station canopy, reading toward the passing trains.
fn platform_board(platform: Aabb, height: f32, station: Aabb) -> Sign {
    let width = 7.0;
    Sign {
        anchor: Vec2::new(station.min.x - 2.5 - width / 2.0, platform.max.y - 0.8),
        facing: Side::North,
        mount: SignMount::Posts,
        elevation: height,
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
    use crate::Building;

    fn town() -> TownLayout {
        TownLayout::castex_inspired()
    }

    fn building(t: &TownLayout, kind: BuildingKind) -> Building {
        *t.buildings.iter().find(|b| b.kind == kind).unwrap()
    }

    fn place(t: &TownLayout, kind: PlaceKind) -> Place {
        t.places().into_iter().find(|p| p.kind == kind).unwrap()
    }

    #[test]
    fn one_place_per_landmark_with_its_spanish_name() {
        let places = town().places();
        let kinds: Vec<_> = places.iter().map(|p| p.kind).collect();
        assert_eq!(kinds, PlaceKind::ALL);
        for p in &places {
            assert_eq!(p.name, p.kind.name());
            assert!(!p.name.trim().is_empty());
            assert_eq!(p.name, p.name.to_uppercase(), "{}", p.name);
            assert!(!p.signs.is_empty(), "{:?} has a sign", p.kind);
            assert!(p.signs.iter().all(|s| s.width > 0.5));
        }
        assert_eq!(
            place(&town(), PlaceKind::GeneralStore).name,
            "ALMACÉN DE RAMOS GENERALES"
        );
        assert_eq!(
            place(&town(), PlaceKind::Station).name,
            "ESTACIÓN EDUARDO CASTEX"
        );
    }

    #[test]
    fn places_are_deterministic() {
        assert_eq!(town().places(), town().places());
    }

    #[test]
    fn building_signs_hang_just_outside_the_street_facade() {
        let t = town();
        for (kind, building_kind) in [
            (PlaceKind::GeneralStore, BuildingKind::GeneralStore),
            (PlaceKind::PoliceStation, BuildingKind::PoliceStation),
            (PlaceKind::Church, BuildingKind::Church),
            (PlaceKind::Station, BuildingKind::Station),
        ] {
            let b = building(&t, building_kind);
            let p = place(&t, kind);
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
            assert!(sign.width < b.footprint.size().dot(along), "{kind:?}");
            // A street lies a few meters out in front of it.
            let ahead = sign.anchor + normal * 6.0;
            assert!(
                t.streets.iter().any(|s| s.contains_point(ahead)),
                "{kind:?} faces a street"
            );
        }
    }

    #[test]
    fn the_station_also_has_a_name_board_on_the_platform_facing_the_track() {
        let t = town();
        let platform = building(&t, BuildingKind::Platform);
        let station = place(&t, PlaceKind::Station);
        assert_eq!(station.signs.len(), 2);
        let board = station.signs[1];
        assert_eq!(board.mount, SignMount::Posts);
        assert_eq!(board.facing, Side::North);
        assert!(platform.footprint.contains_point(board.anchor));
        assert_eq!(board.elevation, platform.height);
        assert!(board.anchor.y < t.railway.start.y);
        assert_eq!(board.footprint(), None, "raised on the platform");
        // Clear of the station's own canopy over the platform.
        let station_building = building(&t, BuildingKind::Station);
        assert!(
            board.anchor.x - board.width / 2.0 > station_building.footprint.max.x
                || board.anchor.x + board.width / 2.0 < station_building.footprint.min.x
        );
    }

    #[test]
    fn the_plaza_sign_stands_inside_its_south_edge_clear_of_trees() {
        let t = town();
        let plaza = place(&t, PlaceKind::Plaza);
        assert_eq!(plaza.footprint, t.plaza);
        let sign = plaza.signs[0];
        assert_eq!(sign.mount, SignMount::Posts);
        assert_eq!(sign.facing, Side::South);
        let area = sign
            .footprint()
            .expect("a post sign on the ground collides");
        assert!(t.plaza.contains(&area));
        assert!(sign.anchor.y - t.plaza.min.y < 2.5);
        for &tree in &t.trees {
            assert!(!area.expanded(0.5).contains_point(tree), "{tree:?}");
        }
        // Clear of the central path so walkers and riders can enter.
        assert!((sign.anchor.x - t.plaza.center().x).abs() - sign.width / 2.0 > 2.0);
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
        let t = town();
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
