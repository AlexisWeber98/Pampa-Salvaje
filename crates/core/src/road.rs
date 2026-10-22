//! Dirt roads between neighbouring towns, for carts and riders.
//!
//! A road joins two towns next to each other on the railway line. It leaves
//! each town by the main street exit facing the other town, runs straight
//! out across the town's leveled band, and then follows a gently curving,
//! seeded line that is more direct than the railway: it skips the track's
//! run-outs and its wide curves.
//!
//! The road keeps [`RoadNetwork::RAIL_CLEARANCE`] from the track. It only
//! crosses the line where it must, when its two ends lie on opposite sides
//! of it, and then squarely, at a level crossing ([`LevelCrossing`]).
//!
//! Roads are a world rule like the railway: a road that would run across a
//! third town's leveled ground or leave the world is left out and reported.

use std::collections::HashMap;
use std::fmt;

use crate::rng::Rng;
use crate::{Aabb, RailLine, TownLayout, Vec2, WorldBounds};

/// What a road joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoadKind {
    /// A road between two neighbouring towns.
    Between,
    /// A short dead-end track from a town across its railway, by the
    /// station, out to the chacras and estancias (user, 2026-10-22).
    Rural,
}

/// One road: between two towns, or a rural track out of one.
#[derive(Debug, Clone, PartialEq)]
pub struct Road {
    pub kind: RoadKind,
    /// Index (into the world's towns) of the town the road leaves.
    pub from: usize,
    /// Index of the town the road reaches (`from` again for a rural
    /// track, which ends in the open land).
    pub to: usize,
    /// The centerline, from the street exit of `from` to that of `to` (or
    /// to the rural track's end).
    pub path: Vec<Vec2>,
    /// Where the road leaves `from` and reaches `to`.
    pub ends: [RoadEnd; 2],
    /// Every place the road crosses the railway, in road order.
    pub crossings: Vec<LevelCrossing>,
}

/// Where a road meets a town's street grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadEnd {
    /// The street exit, on the grid's edge.
    pub at: Vec2,
    /// Unit direction out of town, along the street.
    pub out: Vec2,
}

/// A level crossing: a road crossing the railway.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelCrossing {
    /// Where the road and track centerlines meet.
    pub at: Vec2,
    /// Unit direction of the road there, in road order.
    pub road: Vec2,
    /// Unit direction of the track there.
    pub rail: Vec2,
    /// Distance along the road to the crossing, in meters.
    pub along: f32,
}

/// A signpost by a road: a wooden post with one arrow board per town the
/// road leads to.
#[derive(Debug, Clone, PartialEq)]
pub struct Signpost {
    pub at: Vec2,
    pub boards: Vec<SignBoard>,
}

/// One arrow board of a [`Signpost`].
#[derive(Debug, Clone, PartialEq)]
pub struct SignBoard {
    /// What the board reads, e.g. `EDUARDO CASTEX 2,3 KM`.
    pub text: String,
    /// Unit direction the arrow points, along the road.
    pub toward: Vec2,
}

/// Why a road was left out.
#[derive(Debug, Clone, PartialEq)]
pub enum RoadError {
    /// The road between `from` and `to` would run across `other`'s leveled
    /// ground.
    CrossesTown {
        from: String,
        to: String,
        other: String,
    },
    /// The road between `from` and `to` would leave the world.
    OutsideWorld { from: String, to: String },
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
        }
    }
}

impl std::error::Error for RoadError {}

/// One straight piece of a road, for the index.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    a: Vec2,
    b: Vec2,
    road: u32,
    /// Distance along the road to `a`.
    along: f32,
}

/// Every road of the world, with a grid index of their pieces so the ground
/// and the flora can ask cheaply how far the nearest road is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoadNetwork {
    pub roads: Vec<Road>,
    pub signposts: Vec<Signpost>,
    pieces: Vec<Piece>,
    lengths: Vec<f32>,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

/// The railway centerline as one oriented polyline, so "which side" is the
/// same question all along it.
struct Track(Vec<Vec2>);

impl Track {
    /// Distance from `p` to the track, signed positive left of the line's
    /// direction, and the unit normal pointing left there.
    fn signed(&self, p: Vec2) -> (f32, Vec2) {
        if self.0.len() < 2 {
            return (f32::INFINITY, Vec2::new(0.0, 1.0));
        }
        let mut best = (f32::INFINITY, 0.0, Vec2::new(0.0, 1.0));
        for w in self.0.windows(2) {
            let (a, b) = (w[0], w[1]);
            let d = segment_distance(p, a, b);
            if d < best.0 {
                let dir = unit(b - a).unwrap_or(Vec2::new(1.0, 0.0));
                let left = Vec2::new(-dir.y, dir.x);
                let side = if (p - a).dot(left) >= 0.0 { 1.0 } else { -1.0 };
                best = (d, side, left);
            }
        }
        (best.0 * best.1, best.2)
    }

    /// The first point where the segment `p`–`q` crosses the track, from
    /// `p`, with the track's unit direction there.
    fn first_crossing(&self, p: Vec2, q: Vec2) -> Option<(Vec2, Vec2)> {
        self.0
            .windows(2)
            .filter_map(|w| {
                let (t, _) = intersect(p, q, w[0], w[1])?;
                Some((t, p + (q - p) * t, unit(w[1] - w[0])?))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, at, dir)| (at, dir))
    }
}

impl RoadNetwork {
    /// Width of the road surface, in meters: two carts wide, with room to
    /// become a route later (user, 2026-10-22).
    pub const WIDTH: f32 = 6.5;
    /// Half of [`Self::WIDTH`].
    pub const HALF_WIDTH: f32 = Self::WIDTH / 2.0;
    /// Least distance from the road's centerline to the track's, in meters,
    /// away from level crossings.
    pub const RAIL_CLEARANCE: f32 = 40.0;
    /// How far the road runs straight out of town, in meters: across the
    /// leveled band and a little past it.
    pub const RUN_OUT: f32 = TownLayout::LEVELED_BAND + 20.0;
    /// How deep the road is cut into the ground, in meters.
    pub const CUT_DEPTH: f32 = 0.15;
    /// Width of the leveled ground on either side of the centerline: the
    /// road and a shoulder, in meters.
    pub const CUT_FLAT: f32 = Self::HALF_WIDTH + 1.5;
    /// Where the cut has blended back into the natural ground, in meters
    /// from the centerline.
    pub const CUT_REACH: f32 = Self::HALF_WIDTH + 8.0;
    /// Along the first and last meters of a road, the cut eases in, so the
    /// road meets the town's streets flush.
    const CUT_EASE: f32 = 25.0;
    /// How far past its ends a piece still levels the ground, in meters:
    /// enough to cover the outside of the road's gentle bends.
    const CUT_OVERLAP: f32 = 2.0;
    /// Farthest any query of [`Self::near`] reaches, in meters.
    pub const INDEX_REACH: f32 = 64.0;
    /// Side of one index cell, in meters.
    const CELL: f32 = 32.0;
    /// Spacing of the route's samples, in meters.
    const STEP: f32 = 10.0;
    /// How far from the road's exit, in meters, its signpost stands.
    pub const SIGNPOST_OUT: f32 = 14.0;
    /// Side of a post's square footprint (signposts, crossing signs).
    pub const POST: f32 = 0.25;

    /// The roads between every two towns next to each other on `line`,
    /// inside `bounds`, and the roads left out, each reported.
    pub fn build(
        towns: &[TownLayout],
        line: &RailLine,
        bounds: WorldBounds,
    ) -> (Self, Vec<RoadError>) {
        let track = Track(line.full_path(towns).points().to_vec());
        let mut roads = Vec::new();
        let mut errors = Vec::new();
        for pair in line.towns.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let (Some(a), Some(b)) = (towns.get(from), towns.get(to)) else {
                continue;
            };
            let road = route(a, b, &track, (from, to));
            match check(&road, towns, bounds) {
                Ok(()) => roads.push(road),
                Err(error) => errors.push(error),
            }
        }
        for (i, town) in towns.iter().enumerate() {
            let Some(road) = rural(town, i, &track) else {
                continue;
            };
            match check(&road, towns, bounds) {
                Ok(()) => roads.push(road),
                Err(error) => errors.push(error),
            }
        }
        let names: Vec<&str> = towns.iter().map(|t| t.name.as_str()).collect();
        (Self::new(roads, &names), errors)
    }

    /// The network of `roads`, indexed, with a signpost at every road end
    /// naming the town at the other end (`names` holds the towns' names).
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

    /// The straight pieces of every road, in road order.
    pub fn segments(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.pieces.iter().map(|p| (p.a, p.b))
    }

    /// Every level crossing of every road.
    pub fn crossings(&self) -> impl Iterator<Item = &LevelCrossing> + '_ {
        self.roads.iter().flat_map(|r| r.crossings.iter())
    }

    /// Length of road `index` along its centerline, in meters.
    pub fn length(&self, index: usize) -> f32 {
        self.lengths.get(index).copied().unwrap_or(0.0)
    }

    /// Length of the road between towns `a` and `b` (either way round), if
    /// the network has one.
    pub fn length_between(&self, a: usize, b: usize) -> Option<f32> {
        self.roads
            .iter()
            .position(|r| {
                r.kind == RoadKind::Between
                    && ((r.from, r.to) == (a, b) || (r.from, r.to) == (b, a))
            })
            .map(|i| self.length(i))
    }

    /// Exact distance from `p` to the nearest road centerline (infinite
    /// without roads). Looks at every piece: for tests and rare questions;
    /// see [`Self::near`] for the cheap one.
    pub fn distance(&self, p: Vec2) -> f32 {
        self.pieces
            .iter()
            .map(|s| segment_distance(p, s.a, s.b))
            .fold(f32::INFINITY, f32::min)
    }

    /// Distance from `p` to the nearest road centerline when it is within
    /// `reach` (at most [`Self::INDEX_REACH`]) meters, `None` otherwise.
    pub fn near(&self, p: Vec2, reach: f32) -> Option<f32> {
        let reach = reach.min(Self::INDEX_REACH);
        self.nearby(p)
            .map(|s| segment_distance(p, s.a, s.b))
            .filter(|&d| d <= reach)
            .min_by(f32::total_cmp)
    }

    /// The indexed pieces whose reach covers `p`.
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

    /// How much of the road surface is at `p`, in `[0, 1]`: 1 on the road,
    /// fading to 0 over a meter and a half past its edge. Continuous.
    pub fn surface(&self, p: Vec2) -> f32 {
        let Some(d) = self.near(p, Self::HALF_WIDTH + 1.5) else {
            return 0.0;
        };
        1.0 - smoothstep(Self::HALF_WIDTH - 0.5, Self::HALF_WIDTH + 1.5, d)
    }

    /// The road's cut into the ground at `p`: how strongly it applies, in
    /// `[0, 1]`, and the height it levels the ground to, given the uncut
    /// ground height `base`. Across the road the ground is level, at the
    /// height of the centerline, a little below it; along the road it
    /// follows the land. `None` away from roads.
    ///
    /// Continuous everywhere: each piece within reach pulls toward its own
    /// level by a weight that fades to 0 at [`Self::CUT_REACH`] across it
    /// and within a couple of meters past its ends, and the levels are
    /// averaged by weight.
    pub fn cut(&self, p: Vec2, base: impl Fn(Vec2) -> f32) -> Option<(f32, f32)> {
        let (mut strongest, mut total, mut weighted) = (0.0_f32, 0.0, 0.0);
        for piece in self.nearby(p) {
            let d = piece.b - piece.a;
            let length = d.length();
            if length <= 0.0 {
                continue;
            }
            let dir = d * (1.0 / length);
            // Across the piece's line, and past its ends along it.
            let s = (p - piece.a).dot(dir);
            let off = ((p - piece.a).x * dir.y - (p - piece.a).y * dir.x).abs();
            let past = (-s).max(s - length).max(0.0);
            let w = (1.0 - smoothstep(Self::CUT_FLAT, Self::CUT_REACH, off))
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
        (strongest > 0.0).then(|| (strongest, weighted / total))
    }

    /// The parts of every road inside `area` (a terrain chunk), each as
    /// points along it with the road's left unit normal there and the
    /// distance along the road. A part's ends lie on the area's edge, so
    /// parts drawn chunk by chunk meet.
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
                            // A bend: the miter between the two pieces.
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

    /// Footprints of every signpost and level-crossing sign: thin posts,
    /// all off the road.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let post = Vec2::new(Self::POST, Self::POST);
        self.signposts
            .iter()
            .map(|s| s.at)
            .chain(self.crossings().flat_map(LevelCrossing::sign_spots))
            .map(|at| Aabb::from_center_size(at, post))
            .collect()
    }

    /// A signpost beside every road end, on the right of a rider leaving
    /// town, naming the town at the far end and how far it is.
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

/// One point of a road part inside an area (see [`RoadNetwork::parts_in`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadPoint {
    pub at: Vec2,
    /// Unit normal to the left of the road's direction.
    pub normal: Vec2,
    /// Distance along the whole road, in meters.
    pub along: f32,
}

impl LevelCrossing {
    /// Distance from the crossing, along the road, to the St Andrew's cross
    /// signs on either side, in meters: clear of the track's ballast.
    pub const SIGN_BACK: f32 = 6.0;

    /// Where the two St Andrew's cross signs stand: one before the track on
    /// each approach, on the right of the traffic coming to it.
    pub fn sign_spots(&self) -> [Vec2; 2] {
        let right = Vec2::new(self.road.y, -self.road.x);
        let side = RoadNetwork::HALF_WIDTH + 1.2;
        [
            self.at - self.road * Self::SIGN_BACK + right * side,
            self.at + self.road * Self::SIGN_BACK - right * side,
        ]
    }

    /// The angle between road and track, in radians, in `[0, π/2]`.
    pub fn angle(&self) -> f32 {
        let cross = self.road.x * self.rail.y - self.road.y * self.rail.x;
        cross.abs().clamp(0.0, 1.0).asin()
    }
}

/// What a signpost board reads: the town's name in capitals and the
/// distance in kilometers to a tenth, with a decimal comma.
pub fn board_text(name: &str, meters: f32) -> String {
    let tenths = (meters / 100.0).round() as u64;
    format!("{} {},{} KM", name.to_uppercase(), tenths / 10, tenths % 10)
}

/// The exit of `town`'s street grid facing `toward`: the side whose outward
/// direction points most at it (never the railway's side, which the yard
/// fills), and on that side the street through the grid nearest its middle.
fn exit(town: &TownLayout, toward: Vec2) -> RoadEnd {
    let grid = town.grid();
    let center = grid.center();
    let rail_side = town.railway.map(|r| ((r.start + r.end) * 0.5) - center);
    let sides = [
        Vec2::new(1.0, 0.0),
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, -1.0),
    ];
    let want = unit(toward - center).unwrap_or(Vec2::new(1.0, 0.0));
    let out = sides
        .into_iter()
        .filter(|s| rail_side.is_none_or(|r| r.dot(*s) <= r.length() * 0.5))
        .max_by(|a, b| a.dot(want).total_cmp(&b.dot(want)))
        .unwrap_or(Vec2::new(1.0, 0.0));
    // Streets running out through that side, ring streets left out.
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

/// The road from town `a` to town `b`.
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

/// The rural track out of `town` across its railway: the street through
/// the grid that runs out square to the track nearest the station, with a
/// clear way across the station yard, a level crossing, and past the
/// leveled band a seeded, gently curving dead end of 300–800 m.
fn rural(town: &TownLayout, index: usize, track: &Track) -> Option<Road> {
    let rail = town.railway?;
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
    // A street running out through the yard, square to the track...
    let through = town
        .streets
        .iter()
        .filter(|s| s.size().dot(axis) > s.size().dot(along) && reaches(s))
        .map(|s| s.center().dot(along))
        .filter(|&c| clear_at(c))
        .min_by(|a, b| (a - station).abs().total_cmp(&(b - station).abs()));
    // ...or, where the yard fills the town's whole side, the ring street
    // along the track carried on past the grid's nearer end.
    let ring = town
        .streets
        .iter()
        .filter(|s| s.size().dot(along) > s.size().dot(axis) && reaches(s))
        .map(|s| s.center().dot(axis))
        .next()?;
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
        None => return None,
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
        crossings = crossings_of(&path, &Track(vec![rail.start, rail.end]));
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
    Some(Road {
        kind: RoadKind::Rural,
        from: index,
        to: index,
        path,
        ends,
        crossings,
    })
}

/// `1` along a positive axis, `-1` along a negative one.
fn sign(axis: Vec2) -> f32 {
    if axis.x + axis.y >= 0.0 { 1.0 } else { -1.0 }
}

/// The way from `p` to `q` (both included), staying on one side of the
/// track, or crossing it squarely when `p` and `q` lie on opposite sides.
fn between(p: Vec2, q: Vec2, track: &Track, rng: &mut Rng, depth: u32) -> Vec<Vec2> {
    let side = |x: Vec2| track.signed(x).0.signum();
    let (sp, sq) = (side(p), side(q));
    if sp == sq || depth == 0 {
        return on_side(p, q, sp, track, rng);
    }
    let Some((at, dir)) = track.first_crossing(p, q) else {
        return on_side(p, q, sp, track, rng);
    };
    // Square across the track, from well back on `p`'s side to well out on
    // `q`'s.
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

/// The way from `p` to `q` (both included) on side `side` of the track: the
/// straight line with a gentle seeded wiggle, bent away from the track
/// wherever it would come within [`RoadNetwork::RAIL_CLEARANCE`] of it.
fn on_side(p: Vec2, q: Vec2, side: f32, track: &Track, rng: &mut Rng) -> Vec<Vec2> {
    let length = p.distance(q);
    let Some(dir) = unit(q - p) else {
        return vec![p, q];
    };
    let perp = Vec2::new(-dir.y, dir.x);
    let n = ((length / RoadNetwork::STEP).ceil() as usize).max(2);
    let t: Vec<f32> = (0..=n).map(|i| i as f32 / n as f32).collect();
    // A few seeded sine bends, vanishing at both ends: a gentle curve.
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
            // Move along `perp` toward this side of the track.
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

/// Every place `path` crosses the track, in path order.
fn crossings_of(path: &[Vec2], track: &Track) -> Vec<LevelCrossing> {
    let mut found = Vec::new();
    let mut along = 0.0;
    for w in path.windows(2) {
        let mut here: Vec<(f32, LevelCrossing)> = track
            .0
            .windows(2)
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
    // A crossing exactly on a joint between two track pieces is found twice.
    found.dedup_by(|a, b| a.at.distance(b.at) < 1e-3);
    found
}

/// Checks `road` against the other towns and the world's edge.
fn check(road: &Road, towns: &[TownLayout], bounds: WorldBounds) -> Result<(), RoadError> {
    let name = |i: usize| towns[i].name.clone();
    let to = match road.kind {
        RoadKind::Between => name(road.to),
        RoadKind::Rural => "the open land".to_string(),
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
    Ok(())
}

/// Largest sideways wiggle of a road, in meters.
const WIGGLE_MAX: f32 = 30.0;
/// How far along the road a bend away from the track spreads, in meters.
const BEND_REACH: f32 = 300.0;
/// Over this many meters from either end of a stretch, bends ease in.
const TAPER: f32 = 60.0;
/// How far past the grid's end a rural track crosses the railway when the
/// station yard fills the town's whole side, in meters.
const PAST_GRID: f32 = 15.0;
/// Shortest and longest rural track, in meters.
const RURAL_LENGTH: (f32, f32) = (300.0, 800.0);
/// Mixed into each road's seed.
const ROAD_SEED: u32 = 0x60ad_5eed;

/// Corner cutting: each pass replaces every corner by two points a quarter
/// of the way along its pieces, keeping the first and last points.
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

/// Where segments `p`–`q` and `a`–`b` cross: the parameters along each, in
/// `[0, 1]`; `None` if they do not (or are parallel).
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

/// The point of segment `a`–`b` nearest `p`, and its parameter.
fn closest(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f32) {
    let d = b - a;
    let span = d.dot(d);
    if span <= 0.0 {
        return (a, 0.0);
    }
    let t = ((p - a).dot(d) / span).clamp(0.0, 1.0);
    (a + d * t, t)
}

/// Distance from `p` to the segment `a`–`b`.
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

    /// The shipped towns, their line and their roads.
    pub(crate) fn shipped_roads() -> (Vec<TownLayout>, RailLine, RoadNetwork) {
        let (towns, line) = shipped();
        let (roads, errors) = RoadNetwork::build(&towns, &line, big_world().bounds());
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line, roads)
    }

    fn track(towns: &[TownLayout], line: &RailLine) -> Track {
        Track(line.full_path(towns).points().to_vec())
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
        // Castex–Monte Nievas and Monte Nievas–General Pico.
        assert_eq!(joined, [(1, 0), (0, 2)]);
        let track = track(&towns, &line);
        for (_, road) in between(&roads) {
            let (a, b) = (&towns[road.from], &towns[road.to]);
            assert_eq!(road.path[0], road.ends[0].at);
            assert_eq!(*road.path.last().unwrap(), road.ends[1].at);
            assert!(on_edge(a.grid(), road.ends[0].at), "{:?}", road.ends);
            assert!(on_edge(b.grid(), road.ends[1].at), "{:?}", road.ends);
            // Out of a street, facing the other town.
            for (end, town) in [(road.ends[0], a), (road.ends[1], b)] {
                assert!(
                    town.streets
                        .iter()
                        .any(|s| s.expanded(1e-3).contains_point(end.at))
                );
            }
            assert!(road.ends[0].out.dot(b.grid().center() - a.grid().center()) > 0.0);
            // No third town's leveled band, no crossing, clear of the rail.
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
            // Gently curving: no sharp bend.
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
        // The cut and the drawn strip follow the width.
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
            // Out of the town's grid, square across the yard, into the open.
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

    /// Two railway towns 3 km apart, the west one's track north of it and
    /// the east one's south: the road between them must cross the line.
    fn crossing_world() -> (Vec<TownLayout>, RailLine, RoadNetwork) {
        let town = |name: &str, x: f32, side| TownConfig {
            name: name.into(),
            origin: Vec2::new(x, 0.0),
            blocks: (4, 4),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig { side, offset: 24.0 }),
            plaza: PlazaPlacement::Auto,
            seed: 3,
            police_stations: None,
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
        // Every crossing lies on both centerlines, and they cross nowhere else.
        let mut brute = 0;
        for w in road.path.windows(2) {
            for r in track.0.windows(2) {
                brute += usize::from(intersect(w[0], w[1], r[0], r[1]).is_some());
            }
        }
        assert_eq!(brute, road.crossings.len());
        for c in &road.crossings {
            let on_road = road
                .path
                .windows(2)
                .map(|w| segment_distance(c.at, w[0], w[1]));
            let on_rail = track
                .0
                .windows(2)
                .map(|w| segment_distance(c.at, w[0], w[1]));
            assert!(on_road.fold(f32::INFINITY, f32::min) < 1e-3);
            assert!(on_rail.fold(f32::INFINITY, f32::min) < 1e-3);
            assert!(c.angle() > 70f32.to_radians(), "{}", c.angle().to_degrees());
            assert!(c.along > 0.0 && c.along < roads.length_between(0, 1).unwrap());
            // Its signs stand off the road and off the track.
            for spot in c.sign_spots() {
                let gap = roads.distance(spot);
                assert!(gap > RoadNetwork::HALF_WIDTH + 0.5, "{gap}");
                assert!(track.signed(spot).0.abs() > 4.0);
            }
        }
        // Away from the crossing, the road keeps clear of the rail.
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
        assert_eq!(roads.near(Vec2::new(f32::NAN, 0.0), 10.0), None);
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
        // Posts and crossing signs are thin obstacles, none on a road.
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
        // Every part end is a road end or meets another part's end.
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
}
