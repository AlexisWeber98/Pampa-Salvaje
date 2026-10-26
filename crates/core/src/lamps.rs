//! Kerosene street lamps (faroles): where they stand in the towns and at
//! the estancias.
//!
//! In 1919 a small pampa town had no electric street lighting: a lamplighter
//! lit kerosene faroles on cast-iron or timber posts at dusk. Each town gets
//! a few round its plazas, a pair at every street exit a road leaves by, and
//! one at street corners: at every corner (intersection) of a big town, at
//! every other one, in a checkerboard, in a small town. An estancia gets one
//! at either end of its casco's gallery and one by its track just inside the
//! gate.
//!
//! Placement is deterministic and keeps every post off the driving lanes
//! (a corner lamp stands at the curb), the doors, the store verandas, the
//! plaza paths and monument, the railway and its yard, the roads and the
//! settlement tracks, and finally off every static obstacle of the world.

use crate::rng::Rng;
use crate::road::segment_distance;
use crate::{
    Aabb, BuildingKind, PLAZA_MONUMENT, PLAZA_PATH_WIDTH, RoadKind, RoadNetwork, Settlement,
    SettlementKind, SettlementLayout, StaticObstacles, StructureKind, TownLayout, TownSize, Track,
    Vec2, plaza_monument, plaza_paths,
};

/// Where a lamp stands, for counting and for the drawing's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LampSite {
    /// Round a plaza: by a path's mouth, or by the monument.
    Plaza,
    /// Beside a road where it leaves the street grid.
    Entrance,
    /// At the curb of a street corner.
    Corner,
    /// At an end of an estancia's gallery.
    Homestead,
    /// By an estancia's track, just inside its gate.
    Gate,
}

/// One kerosene street lamp: a post with a glass lantern on top.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreetLamp {
    /// The foot of the post.
    pub at: Vec2,
    pub site: LampSite,
}

impl StreetLamp {
    /// Side of the post's square collision footprint, in meters.
    pub const POST: f32 = 0.24;
    /// Height of the lamp, finial included, in meters.
    pub const HEIGHT: f32 = 3.35;
    /// Height of the middle of the lantern's glass, where the flame burns.
    pub const FLAME: f32 = 2.8;

    /// Its static obstacle: the post.
    pub fn obstacle(&self) -> Aabb {
        Aabb::from_center_size(self.at, Vec2::new(Self::POST, Self::POST))
    }
}

/// Every street lamp of the world: per town and per settlement, in the
/// order of the world's towns and of [`SettlementLayout::settlements`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreetLamps {
    pub towns: Vec<Vec<StreetLamp>>,
    pub settlements: Vec<Vec<StreetLamp>>,
}

impl StreetLamps {
    /// How far a corner lamp stands in from both street edges, meters: at
    /// the curb, clear of the driving lane.
    pub const CURB: f32 = 0.7;
    /// Least clear ground round a post, in meters.
    pub const CLEARANCE: f32 = 0.5;
    /// How far out of a road's exit from the grid the entrance pair stands.
    pub const ENTRANCE_OUT: f32 = 1.5;
    /// How far to either side of a road's centerline the entrance pair
    /// stands: off the road's edge.
    pub const ENTRANCE_SIDE: f32 = RoadNetwork::HALF_WIDTH + 1.0;
    /// How far inside a plaza's edge its lamps stand, meters.
    pub const PLAZA_INSET: f32 = 1.0;
    /// Gap between a plaza lamp and the path it stands beside, meters.
    pub const PATH_GAP: f32 = 0.7;
    /// Keep-out in front of a door: this far out of the facade...
    pub const DOOR_REACH: f32 = 4.0;
    /// ...and this far to either side of its middle, in meters.
    pub const DOOR_HALF_WIDTH: f32 = 1.8;
    /// Least distance between two lamps, in meters.
    pub const SPACING: f32 = 3.0;
    /// How far out of the casco's front its pair stands, meters.
    pub const GALLERY_OUT: f32 = 1.2;
    /// Least gap between a lamp and a track's rut, meters.
    pub const TRACK_GAP: f32 = 0.8;

    /// The lamps of every town of `towns` (with the `roads` leaving them)
    /// and of every estancia of `settlements`, kept off every box of
    /// `obstacles` (the world's static obstacles, lamps not yet among them).
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

    /// Every lamp's post, as a static obstacle.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.all().map(StreetLamp::obstacle)
    }

    /// Every lamp, towns first.
    pub fn all(&self) -> impl Iterator<Item = &StreetLamp> + '_ {
        self.towns.iter().chain(&self.settlements).flatten()
    }

    /// How many lamps there are.
    pub fn count(&self) -> usize {
        self.all().count()
    }
}

/// The lamps of `town`, the `index`-th of the world's towns (the one
/// `roads` name by that index): round its plazas, at its road exits and at
/// its street corners, each clear of the town's keep-outs and of the
/// others.
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

/// The lamps of settlement `s`: an estancia's pair at either end of its
/// casco's gallery and one by its track just inside the gate; none for the
/// poorer kinds.
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

/// Puts lamps up one by one, each only where it fits.
struct Lamplighter<'a> {
    /// Boxes no post may come near.
    keep: Vec<Aabb>,
    roads: Option<&'a RoadNetwork>,
    tracks: &'a [Track],
    lamps: Vec<StreetLamp>,
}

impl Lamplighter<'_> {
    /// Whether a post at `at` keeps its clearance from every keep-out,
    /// road and track, and its spacing from the lamps already up.
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

    /// Puts a lamp up at `at` if it fits; whether it did.
    fn put(&mut self, at: Vec2, site: LampSite) -> bool {
        let fits = self.fits(at);
        if fits {
            self.lamps.push(StreetLamp { at, site });
        }
        fits
    }
}

/// The middle of the facade of `footprint` facing `n`, and half its width.
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

/// The four ways out of a box, east, north, west and south.
const SIDES: [Vec2; 4] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(0.0, -1.0),
];

/// How generously a town is lit, from its size (user playtest, 2026-10-23:
/// small plazas had too many lamps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LampTier {
    /// A village or halt of at most [`LampTier::VERY_SMALL_BLOCKS`] blocks
    /// (Metileo): two plaza lamps, corners only round the plaza and by the
    /// station.
    VerySmall,
    /// A small town ([`TownSize::Small`]): four plaza lamps, every other
    /// corner.
    Small,
    /// A big town: four lamps by each plaza's paths, four more round the
    /// main monument, every corner.
    Big,
}

impl LampTier {
    /// Most blocks of a very small town: the smallest grid, 3 x 3.
    pub const VERY_SMALL_BLOCKS: usize = 9;

    /// The tier of `town`.
    pub fn of(town: &TownLayout) -> Self {
        let (w, h) = town.blocks();
        match town.size {
            _ if w * h <= Self::VERY_SMALL_BLOCKS => Self::VerySmall,
            TownSize::Small => Self::Small,
            TownSize::Big | TownSize::VeryBig => Self::Big,
        }
    }
}

/// The unit axis from `from` toward `to` the most: east, north, west or
/// south.
fn axis_toward(from: Vec2, to: Vec2) -> Vec2 {
    let d = to - from;
    SIDES
        .into_iter()
        .max_by(|a, b| a.dot(d).total_cmp(&b.dot(d)))
        .unwrap_or(SIDES[1])
}

/// The town's station's middle, if it has one.
fn station(town: &TownLayout) -> Option<Vec2> {
    town.buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Station)
        .map(|b| b.footprint.center())
}

/// Round the plazas, by [`LampTier`]: one by each path's mouth, on the
/// right-hand side as one walks in (a very small town only at the two
/// mouths on the path toward its station, or north-south without one); in
/// a big town four more round the main plaza's monument, one in each bed
/// corner.
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

/// A pair at every exit of the grid a road (not a street across the
/// railway) leaves `town` by, one to either side of the road just outside.
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

/// One lamp at the curb of a street corner: at every corner of a big town,
/// at every other one (a checkerboard) in a small town, and in a very small
/// one only at the plaza's corners and the two corners nearest the station.
/// The seeded corner of the four comes first; the next one is tried when it
/// does not fit.
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
    // A very small town's: round its plaza, and the two nearest its station.
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

/// Whether `p` lies on the edge of `grid`, within half a meter.
fn on_edge(grid: Aabb, p: Vec2) -> bool {
    let d = [
        p.x - grid.min.x,
        grid.max.x - p.x,
        p.y - grid.min.y,
        grid.max.y - p.y,
    ];
    d.iter().all(|&d| d > -0.5) && d.iter().any(|d| d.abs() < 0.5)
}

/// The box between two corners `a` and `b`, in any order.
fn between(a: Vec2, b: Vec2) -> Aabb {
    Aabb::new(
        Vec2::new(a.x.min(b.x), a.y.min(b.y)),
        Vec2::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

/// What a town's lamps keep off besides its obstacles: the ground in front
/// of every door, every store's veranda, the platforms and their canopies,
/// the railway and its yard, and every plaza's paths and monument.
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

    /// Street intersections of `town`: where a north-south street crosses
    /// an east-west one.
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
                // One by each path's mouth, none round the monument.
                LampTier::Small => assert_eq!(main, 4, "{}", town.name),
                // Four more round the monument; four in the station plaza.
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

    /// Metileo as `assets/world/towns.ron` ships it: a 3 x 3 halt.
    fn metileo() -> TownLayout {
        TownLayout::generate(&crate::TownConfig {
            name: "Metileo".into(),
            origin: Vec2::ZERO,
            blocks: (3, 3),
            block_size: 40.0,
            street_width: 8.0,
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
        // At the two mouths of the path toward the station: on one line.
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
            // Inside the square where the two streets cross.
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
                    // A corner whose four curbs are all taken (a yard, a
                    // veranda, a door) goes without.
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
                // On a street only at its curb, never in the lane.
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
        // The lamps keep off every obstacle: a palenque on a lamp's spot
        // would drop it. The same town without its palenques lights the same.
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

    /// The cells of a `step` grid over `area` someone of `radius` reaches
    /// on foot from `from`, kept off `blocked`.
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
        // The shipped towns with all their lamps, and the sample towns
        // (a village among them) with their plaza lamps.
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
            // Every plaza lamp still stands where it stood before the
            // furniture: by each mouth, and round a city's monument.
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
            // And clear of every obstacle, the furniture's included.
            let obstacles = town.obstacles();
            for l in &plaza {
                let reach = l.obstacle().expanded(StreetLamps::CLEARANCE);
                assert!(obstacles.iter().all(|o| !o.intersects(&reach)), "{l:?}");
            }
            let mut blocked = obstacles;
            blocked.extend(lamps.iter().map(StreetLamp::obstacle));
            for (k, f) in town.plaza_furniture().into_iter().enumerate() {
                let (c, half) = (f.area.center(), f.area.size() * 0.5);
                // A walker and a rider, from one path's mouth just outside
                // the plaza, reach the other three and walk round the
                // monument; the walker reaches the plaza's meeting spot.
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
