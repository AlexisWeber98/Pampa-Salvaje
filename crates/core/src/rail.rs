//! The railway line: one track through every railway town of the world.
//!
//! Inside a town the track stays axis-aligned along the station (see
//! [`TownLayout::railway`]). Between two towns listed next to each other, a
//! link joins the run-out tip of one town's track nearest the other town to
//! the other town's tip nearest the first, so the town tracks and the links
//! form one continuous line.
//!
//! A link leaves its first tip straight on along that town's track, bends
//! onto the diagonal on a wide circular curve, runs straight, and bends
//! back onto the second town's track just as it reaches that town's tip.
//! The curves live wholly on the link, so a town's own straight track is
//! never cut short, and the line keeps one heading through every tip.
//!
//! The line is a world rule: every railway town must be on it, and no link
//! may cross another town or leave the world. Bad data never crashes; the
//! offending towns or links are left out and reported.

use std::fmt;

use crate::{Aabb, Railway, TownLayout, Vec2, WorldBounds};

/// A stretch of track between two towns of the line.
#[derive(Debug, Clone, PartialEq)]
pub struct RailLink {
    /// Index (into the world's towns) of the town the link leaves.
    pub from: usize,
    /// Index of the town the link reaches.
    pub to: usize,
    /// The straight chord from a run-out tip of `from` to a run-out tip of
    /// `to`. The track itself follows [`Self::path`].
    pub track: Railway,
    /// The centerline the track follows, from `track.start` to `track.end`:
    /// short straight pieces that bend at most [`RailLine::MAX_TURN`]
    /// radians from one to the next.
    pub path: Vec<Vec2>,
    /// The curves of the link, in path order (none on a link that already
    /// runs straight on from both tips).
    pub curves: Vec<RailCurve>,
}

/// A circular curve of the line, fitted into the corner between two
/// straights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailCurve {
    /// Radius of the arc, in meters: [`RailLine::CURVE_RADIUS`] unless the
    /// straights around it are too short for that.
    pub radius: f32,
    /// Distance from the corner to either end of the arc, in meters
    /// (`radius * tan(turn / 2)`).
    pub tangent: f32,
    /// How far the track turns, in radians, counter-clockwise positive.
    pub turn: f32,
}

impl RailLink {
    /// The straight pieces of [`Self::path`], in order.
    pub fn segments(&self) -> impl Iterator<Item = Railway> + '_ {
        let gauge = self.track.gauge;
        self.path.windows(2).map(move |w| Railway {
            start: w[0],
            end: w[1],
            gauge,
        })
    }

    /// Length of the track along [`Self::path`], in meters.
    pub fn length(&self) -> f32 {
        self.path.windows(2).map(|w| w[0].distance(w[1])).sum()
    }
}

/// The railway line through the world's towns.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RailLine {
    /// Indices (into the world's towns) of the towns on the line, in order.
    pub towns: Vec<usize>,
    /// The links between consecutive towns, in line order. A link that
    /// breaks a rule is left out, so the line may have gaps.
    pub links: Vec<RailLink>,
}

/// Why a town or a link was left out of the railway line.
#[derive(Debug, Clone, PartialEq)]
pub enum RailLineError {
    /// The line names a town the world does not have (or could not build).
    UnknownTown { name: String },
    /// The line names a town twice.
    DuplicateTown { name: String },
    /// The line names a town without a railway.
    NoRailway { name: String },
    /// A railway town the line leaves out.
    NotOnLine { name: String },
    /// The link between `from` and `to` runs across `other`'s leveled
    /// ground (`other` may be `from` or `to` itself, when the link comes back
    /// across its own town).
    CrossesTown {
        from: String,
        to: String,
        other: String,
    },
    /// The link between `from` and `to` leaves the world.
    OutsideWorld { from: String, to: String },
}

impl fmt::Display for RailLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "railway line: ")?;
        match self {
            Self::UnknownTown { name } => {
                write!(f, "`{name}` is not a town of the world; left out")
            }
            Self::DuplicateTown { name } => {
                write!(
                    f,
                    "`{name}` is listed more than once; only the first is kept"
                )
            }
            Self::NoRailway { name } => write!(f, "`{name}` has no railway; left out"),
            Self::NotOnLine { name } => write!(
                f,
                "`{name}` has a railway but is not on the line; every railway town must be"
            ),
            Self::CrossesTown { from, to, other } => write!(
                f,
                "the link from `{from}` to `{to}` runs across `{other}`; link left out"
            ),
            Self::OutsideWorld { from, to } => write!(
                f,
                "the link from `{from}` to `{to}` leaves the world; link left out"
            ),
        }
    }
}

impl std::error::Error for RailLineError {}

impl RailLine {
    /// Radius of the line's curves, in meters, where the straights around
    /// a curve leave room for it.
    pub const CURVE_RADIUS: f32 = 500.0;
    /// Longest straight piece a curve is drawn with, in meters.
    pub const CURVE_STEP: f32 = 8.0;
    /// Largest heading change from one piece of a curve to the next, in
    /// radians (2°).
    pub const MAX_TURN: f32 = 2.0 * std::f32::consts::PI / 180.0;

    /// Builds the line through `towns` (already placed, see
    /// [`crate::place_towns`]) inside `bounds`.
    ///
    /// `names` lists the towns in line order. Without it, every railway
    /// town is on the line, west to east by the center of its grid. Unknown,
    /// duplicated and railway-less names are left out, railway towns the
    /// list misses stay off the line, and links that cross another town or
    /// leave the world are dropped; each is reported.
    pub fn build(
        names: Option<&[String]>,
        towns: &[TownLayout],
        bounds: WorldBounds,
    ) -> (Self, Vec<RailLineError>) {
        let mut errors = Vec::new();
        let order = match names {
            Some(names) => listed(names, towns, &mut errors),
            None => west_to_east(towns),
        };
        errors.extend(
            towns
                .iter()
                .enumerate()
                .filter(|(i, t)| t.railway.is_some() && !order.contains(i))
                .map(|(_, t)| RailLineError::NotOnLine {
                    name: t.name.clone(),
                }),
        );
        let mut links = Vec::new();
        for pair in order.windows(2) {
            match link(towns, pair[0], pair[1], bounds) {
                Ok(link) => links.push(link),
                Err(error) => errors.push(error),
            }
        }
        (
            Self {
                towns: order,
                links,
            },
            errors,
        )
    }

    /// The straight pieces of every link's track, in line order.
    pub fn segments(&self) -> impl Iterator<Item = Railway> + '_ {
        self.links.iter().flat_map(RailLink::segments)
    }

    /// The parts of the links inside `area` (a terrain chunk, a map frame),
    /// one per straight piece, in line order. Parts that only touch its edge
    /// are skipped.
    pub fn segments_in(&self, area: Aabb) -> Vec<RailSpan> {
        let mut spans = Vec::new();
        for link in &self.links {
            let total = link.length();
            let mut from = 0.0;
            for track in link.segments() {
                let length = track.start.distance(track.end);
                if let Some((enter, exit)) = area.segment_overlap(track.start, track.end) {
                    let (enter, exit) = (from + enter * length, from + exit * length);
                    if exit - enter > CLIP_EPSILON {
                        spans.push(RailSpan {
                            track,
                            from,
                            enter,
                            exit,
                            total,
                        });
                    }
                }
                from += length;
            }
        }
        spans
    }
}

impl RailLine {
    /// The whole line as one path a train can run, from the first town's
    /// far run-out tip to the last connected town's far tip: each town's
    /// straight track, tip to tip, joined by the curved links.
    ///
    /// The path stops at the first gap (a link left out); a line without
    /// links is just its first town's track, and a line without towns is
    /// empty.
    pub fn full_path(&self, towns: &[TownLayout]) -> LinePath {
        let served = self.connected_towns();
        let Some(rail) = served.first().and_then(|&i| towns.get(i)?.railway) else {
            return LinePath::default();
        };
        let first_link = served.get(1).and_then(|&next| self.link(served[0], next));
        let mut points = vec![match first_link {
            Some(link) => far_tip(rail, link.track.start),
            None => rail.start,
        }];
        for pair in served.windows(2) {
            if let Some(link) = self.link(pair[0], pair[1]) {
                points.extend(&link.path);
            }
        }
        let last = served.last().and_then(|&i| towns.get(i)?.railway);
        if let (Some(rail), Some(&arrived)) = (last, points.last()) {
            points.push(far_tip(rail, arrived));
        }
        LinePath::new(points)
    }

    /// The towns [`Self::full_path`] runs through, in line order: the first
    /// town and every town reached from it without a gap.
    pub fn connected_towns(&self) -> Vec<usize> {
        let mut served: Vec<usize> = self.towns.iter().take(1).copied().collect();
        for pair in self.towns.windows(2) {
            if self.link(pair[0], pair[1]).is_none() {
                break;
            }
            served.push(pair[1]);
        }
        served
    }

    /// The link from town `from` to town `to`, if the line has it.
    fn link(&self, from: usize, to: usize) -> Option<&RailLink> {
        self.links.iter().find(|l| l.from == from && l.to == to)
    }
}

/// The tip of `rail` farther from `tip`.
fn far_tip(rail: Railway, tip: Vec2) -> Vec2 {
    if rail.start.distance(tip) < rail.end.distance(tip) {
        rail.end
    } else {
        rail.start
    }
}

/// A polyline measured by arc length: where a train runs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinePath {
    points: Vec<Vec2>,
    /// Arc length from the first point to each point.
    arcs: Vec<f32>,
}

impl LinePath {
    /// The path through `points`, dropping repeated points.
    pub fn new(points: Vec<Vec2>) -> Self {
        let mut kept: Vec<Vec2> = Vec::with_capacity(points.len());
        for p in points {
            push_point(&mut kept, p);
        }
        let mut arcs = Vec::with_capacity(kept.len());
        let mut total = 0.0;
        for (i, p) in kept.iter().enumerate() {
            if i > 0 {
                total += kept[i - 1].distance(*p);
            }
            arcs.push(total);
        }
        Self { points: kept, arcs }
    }

    /// The corners of the path, in order.
    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// Length of the path, in meters.
    pub fn length(&self) -> f32 {
        self.arcs.last().copied().unwrap_or(0.0)
    }

    /// The point `distance` meters along the path, clamped to its ends.
    pub fn at(&self, distance: f32) -> Vec2 {
        let Some(i) = self.piece(distance) else {
            return self.points.first().copied().unwrap_or(Vec2::ZERO);
        };
        let (a, b) = (self.points[i], self.points[i + 1]);
        let span = self.arcs[i + 1] - self.arcs[i];
        let t = ((distance - self.arcs[i]) / span).clamp(0.0, 1.0);
        a + (b - a) * t
    }

    /// The unit direction of travel `distance` meters along the path (the
    /// heading of the straight piece there), east on an empty path.
    pub fn direction_at(&self, distance: f32) -> Vec2 {
        self.piece(distance)
            .and_then(|i| unit(self.points[i + 1] - self.points[i]))
            .unwrap_or(Vec2::new(1.0, 0.0))
    }

    /// Distance along the path to the point of the path nearest `p`.
    pub fn project(&self, p: Vec2) -> f32 {
        let mut best = (f32::INFINITY, 0.0);
        for (i, w) in self.points.windows(2).enumerate() {
            let d = w[1] - w[0];
            let span = d.length();
            let t = ((p - w[0]).dot(d) / (span * span)).clamp(0.0, 1.0);
            let gap = p.distance(w[0] + d * t);
            if gap < best.0 {
                best = (gap, self.arcs[i] + span * t);
            }
        }
        best.1
    }

    /// Index of the straight piece holding `distance` (clamped), or `None`
    /// when the path has no piece.
    fn piece(&self, distance: f32) -> Option<usize> {
        let pieces = self.points.len().checked_sub(1).filter(|&n| n > 0)?;
        let i = self.arcs.partition_point(|&a| a <= distance);
        Some(i.saturating_sub(1).min(pieces - 1))
    }
}

/// The part of one straight piece of a track inside an area, measured along
/// the whole track (a link's path, or a town's straight track) it belongs
/// to: the piece, how far along the track the piece starts, where the part
/// begins and ends, and the whole track's length, all in meters. Measuring
/// along the whole track lets each part place its sleepers where the whole
/// track would, so parts drawn chunk by chunk and piece by piece meet
/// without a seam.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailSpan {
    /// The straight piece the part lies on.
    pub track: Railway,
    /// Distance along the whole track to `track.start`.
    pub from: f32,
    /// Distance along the whole track to where the part begins.
    pub enter: f32,
    /// Distance along the whole track to where the part ends.
    pub exit: f32,
    /// Length of the whole track.
    pub total: f32,
}

impl RailSpan {
    /// The whole of a single straight track.
    pub fn whole(track: Railway) -> Self {
        let total = track.start.distance(track.end);
        Self {
            track,
            from: 0.0,
            enter: 0.0,
            exit: total,
            total,
        }
    }

    /// Where the part begins on the ground.
    pub fn start(&self) -> Vec2 {
        self.at(self.enter)
    }

    /// Where the part ends on the ground.
    pub fn end(&self) -> Vec2 {
        self.at(self.exit)
    }

    /// The point `distance` meters along the whole track (on this piece's
    /// line).
    fn at(&self, distance: f32) -> Vec2 {
        let along = self.track.end - self.track.start;
        let length = along.length();
        if length <= 0.0 {
            return self.track.start;
        }
        self.track.start + along * ((distance - self.from) / length)
    }
}

/// Clipped parts shorter than this (in meters) only touch an area.
const CLIP_EPSILON: f32 = 1e-3;

/// The towns `names` lists, as indices, keeping the first of each name and
/// leaving out (and reporting) unknown and railway-less towns.
fn listed(names: &[String], towns: &[TownLayout], errors: &mut Vec<RailLineError>) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::new();
    for name in names {
        let Some(index) = towns.iter().position(|t| &t.name == name) else {
            errors.push(RailLineError::UnknownTown { name: name.clone() });
            continue;
        };
        if order.contains(&index) {
            errors.push(RailLineError::DuplicateTown { name: name.clone() });
        } else if towns[index].railway.is_none() {
            errors.push(RailLineError::NoRailway { name: name.clone() });
        } else {
            order.push(index);
        }
    }
    order
}

/// Every railway town, west to east by the center of its grid (list order
/// breaks ties).
fn west_to_east(towns: &[TownLayout]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..towns.len())
        .filter(|&i| towns[i].railway.is_some())
        .collect();
    order.sort_by(|&a, &b| {
        let x = |i: usize| towns[i].grid().center().x;
        x(a).total_cmp(&x(b))
    });
    order
}

/// The link from town `from` to town `to`, both with a railway, or why it
/// breaks the rules: its curved track must stay inside `bounds` and clear
/// of every other town's leveled ground, ballast included. It may only
/// leave its own two towns' ground, never come back across it.
fn link(
    towns: &[TownLayout],
    from: usize,
    to: usize,
    bounds: WorldBounds,
) -> Result<RailLink, RailLineError> {
    let (a, b) = (&towns[from], &towns[to]);
    // Only railway towns get on the line, so this never fires.
    let (Some(rail_a), Some(rail_b)) = (a.railway, b.railway) else {
        let name = if a.railway.is_none() {
            &a.name
        } else {
            &b.name
        };
        return Err(RailLineError::NoRailway { name: name.clone() });
    };
    let (start, start_back) = nearest_tip(rail_a, b.grid().center());
    let (end, end_back) = nearest_tip(rail_b, a.grid().center());
    // Leave `a` heading out along its track; reach `b` heading into it.
    let out = unit(start - start_back).unwrap_or(Vec2::ZERO);
    let into = unit(end_back - end).unwrap_or(Vec2::ZERO);
    let (path, curves) = smooth_link(start, out, end, into, RailLine::CURVE_RADIUS);

    let names = || (a.name.clone(), b.name.clone());
    let half = TownLayout::RAIL_CORRIDOR_HALF;
    let inside = |p: &Vec2| {
        p.x - half >= bounds.min.x
            && p.y - half >= bounds.min.y
            && p.x + half <= bounds.max.x
            && p.y + half <= bounds.max.y
    };
    // Every corner of the path inside the (convex) world keeps all of it in.
    if !path.iter().all(inside) {
        let (from, to) = names();
        return Err(RailLineError::OutsideWorld { from, to });
    }
    let crossed = towns.iter().enumerate().find(|&(i, other)| {
        let ground = other.leveled().expanded(half);
        i != from
            && i != to
            && path
                .windows(2)
                .any(|w| ground.segment_overlap(w[0], w[1]).is_some())
    });
    // Out of the first town, it never comes back; into the last, it only
    // arrives (read from that end, it never comes back either).
    let own = |town: &TownLayout| town.leveled().expanded(half);
    let crossed = crossed.map(|(_, other)| other).or_else(|| {
        let back_into_a = reenters(own(a), path.windows(2));
        let back_into_b = reenters(own(b), path.windows(2).rev());
        if back_into_a {
            Some(a)
        } else if back_into_b {
            Some(b)
        } else {
            None
        }
    });
    if let Some(other) = crossed {
        let (from, to) = names();
        return Err(RailLineError::CrossesTown {
            from,
            to,
            other: other.name.clone(),
        });
    }
    Ok(RailLink {
        from,
        to,
        track: Railway {
            start,
            end,
            gauge: rail_a.gauge,
        },
        path,
        curves,
    })
}

/// Whether a path, read as its `pieces` in order, comes back onto `band`
/// after it has left it: a link leaving its town's ground and crossing it
/// again further on.
fn reenters<'a>(band: Aabb, pieces: impl Iterator<Item = &'a [Vec2]>) -> bool {
    let mut left = false;
    for w in pieces {
        let on = band.segment_overlap(w[0], w[1]).is_some();
        if left && on {
            return true;
        }
        left |= !on;
    }
    false
}

/// The tip of `rail` nearest `toward`, and its other tip.
fn nearest_tip(rail: Railway, toward: Vec2) -> (Vec2, Vec2) {
    if rail.start.distance(toward) <= rail.end.distance(toward) {
        (rail.start, rail.end)
    } else {
        (rail.end, rail.start)
    }
}

/// The centerline of a link from `start`, leaving along unit `out`, to
/// `end`, arriving along unit `into`, and its curves.
///
/// The link is the polyline `start → p1 → p2 → end`, where `p1` lies ahead
/// of `start` along `out` and `p2` behind `end` along `into`, with each
/// corner filleted by an arc of `radius`. The corners sit exactly one
/// tangent length from the tips, so the first arc begins at `start` and the
/// second ends at `end`: the link joins both town tracks without a kink, a
/// gap or an overlap. Tangent lengths are capped at a quarter of the
/// tip-to-tip distance, which keeps each within half of the middle
/// straight; a capped curve gets the radius that fits.
fn smooth_link(
    start: Vec2,
    out: Vec2,
    end: Vec2,
    into: Vec2,
    radius: f32,
) -> (Vec<Vec2>, Vec<RailCurve>) {
    let cap = start.distance(end) / 4.0;
    let corners = |(ta, tb): (f32, f32)| (start + out * ta, end - into * tb);
    // The tangent lengths depend on the middle straight's heading, which
    // depends on them: iterate to a fixed point (a few rounds suffice, since
    // the tangents are short against the link).
    let mut tangents = (0.0, 0.0);
    for _ in 0..64 {
        let (p1, p2) = corners(tangents);
        let Some(middle) = unit(p2 - p1) else {
            break;
        };
        let next = (
            tangent_length(radius, turn(out, middle)).min(cap),
            tangent_length(radius, turn(middle, into)).min(cap),
        );
        let settled = (next.0 - tangents.0).abs() < 1e-4 && (next.1 - tangents.1).abs() < 1e-4;
        tangents = next;
        if settled {
            break;
        }
    }
    let (p1, p2) = corners(tangents);
    let middle = unit(p2 - p1).unwrap_or(out);
    let mut path = vec![start];
    let mut curves = Vec::new();
    curves.extend(fillet(&mut path, p1, out, middle, tangents.0));
    curves.extend(fillet(&mut path, p2, middle, into, tangents.1));
    push_point(&mut path, end);
    (path, curves)
}

/// Appends to `path` the arc rounding `corner` between the unit headings
/// `before` and `after`, `tangent` meters to either side of it, in straight
/// pieces at most [`RailLine::CURVE_STEP`] long that turn at most
/// [`RailLine::MAX_TURN`] each. Without a turn, nothing is added.
fn fillet(
    path: &mut Vec<Vec2>,
    corner: Vec2,
    before: Vec2,
    after: Vec2,
    tangent: f32,
) -> Option<RailCurve> {
    let delta = turn(before, after);
    if delta.abs() < 1e-5 || tangent <= 0.0 {
        return None;
    }
    let radius = tangent / (delta.abs() / 2.0).tan();
    let (from, to) = (corner - before * tangent, corner + after * tangent);
    let left = Vec2::new(-before.y, before.x);
    let center = from + left * (radius * delta.signum());
    let arc = radius * delta.abs();
    let pieces = (delta.abs() / RailLine::MAX_TURN)
        .ceil()
        .max((arc / RailLine::CURVE_STEP).ceil())
        .max(1.0) as usize;
    push_point(path, from);
    for i in 1..pieces {
        let angle = delta * i as f32 / pieces as f32;
        push_point(path, center + rotate(from - center, angle));
    }
    push_point(path, to);
    Some(RailCurve {
        radius,
        tangent,
        turn: delta,
    })
}

/// Appends `p` unless it repeats the last point.
fn push_point(path: &mut Vec<Vec2>, p: Vec2) {
    if path
        .last()
        .is_none_or(|&last| last.distance(p) > CLIP_EPSILON)
    {
        path.push(p);
    }
}

/// Distance from a corner turning `delta` radians to where an arc of
/// `radius` meets the straights. Near a U-turn it grows without bound, so
/// callers cap it.
fn tangent_length(radius: f32, delta: f32) -> f32 {
    // Half a U-turn is π/2, where tan blows up (and flips sign in f32).
    radius * (delta.abs() / 2.0).min(1.55).tan()
}

/// The signed angle from unit `a` to unit `b`, counter-clockwise positive.
fn turn(a: Vec2, b: Vec2) -> f32 {
    (a.x * b.y - a.y * b.x).atan2(a.dot(b))
}

/// `v` rotated `angle` radians counter-clockwise.
fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// `v` scaled to length one, or `None` when it has no length.
fn unit(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length > 0.0).then(|| v * (1.0 / length))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{PlazaPlacement, RailwayConfig, Side, TownConfig, WorldConfig, place_towns};

    fn config(name: &str, x: f32, y: f32, blocks: (usize, usize), rail: bool) -> TownConfig {
        TownConfig {
            name: name.into(),
            origin: Vec2::new(x, y),
            blocks,
            block_size: 40.0,
            street_width: 10.0,
            railway: rail.then_some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
        }
    }

    fn place(world: WorldConfig, configs: &[TownConfig]) -> Vec<TownLayout> {
        let (towns, errors) = place_towns(&world, configs);
        assert!(errors.is_empty(), "{errors:?}");
        towns
    }

    /// The three towns of the line, as `assets/world/towns.ron` lays them
    /// out, in file order.
    fn pampa_line() -> (WorldConfig, Vec<TownLayout>) {
        let world = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let towns = place(
            world,
            &[
                config("Monte Nievas", -3050.0, -1950.0, (4, 3), true),
                config("Eduardo Castex", -5150.0, -2850.0, (5, 4), true),
                config("General Pico", 2917.0, 1783.0, (8, 8), true),
            ],
        );
        (world, towns)
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|&n| n.to_string()).collect()
    }

    fn line_names(line: &RailLine, towns: &[TownLayout]) -> Vec<String> {
        line.towns.iter().map(|&i| towns[i].name.clone()).collect()
    }

    /// Three towns on one east-west row, 2 km apart, the middle one
    /// optionally without a railway.
    fn row(middle_rail: bool) -> Vec<TownLayout> {
        place(
            WorldConfig::default(),
            &[
                config("Oeste", -2000.0, 0.0, (4, 4), true),
                config("Medio", 0.0, 0.0, (4, 4), middle_rail),
                config("Este", 2000.0, 0.0, (4, 4), true),
            ],
        )
    }

    #[test]
    fn the_pampa_line_connects_the_three_towns_without_warnings() {
        let (world, towns) = pampa_line();
        let list = names(&["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (line, errors) = RailLine::build(Some(&list), &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line_names(&line, &towns), list);
        let pairs: Vec<(usize, usize)> = line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(pairs, [(1, 0), (0, 2)]);
        // Each link bends twice, so it is drawn with many straight pieces.
        assert!(line.segments().count() > 2 * 4);
    }

    #[test]
    fn links_join_the_run_out_tips_into_one_continuous_line() {
        let (world, towns) = pampa_line();
        let list = names(&["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (line, _) = RailLine::build(Some(&list), &towns, world.bounds());
        let rail = |i: usize| towns[i].railway.expect("a railway");
        for link in &line.links {
            let (from, to) = (rail(link.from), rail(link.to));
            assert!(
                [from.start, from.end].contains(&link.track.start),
                "{link:?}"
            );
            assert!([to.start, to.end].contains(&link.track.end), "{link:?}");
            assert_eq!(link.track.gauge, from.gauge);
        }
        // All three towns lie west to east and their tracks run east-west:
        // each link leaves from the east tip and arrives at the west tip, so
        // the middle town's track carries the line through.
        let (castex, nievas, pico) = (rail(1), rail(0), rail(2));
        assert_eq!(line.links[0].track.start, castex.end);
        assert_eq!(line.links[0].track.end, nievas.start);
        assert_eq!(line.links[1].track.start, nievas.end);
        assert_eq!(line.links[1].track.end, pico.start);
    }

    #[test]
    fn without_a_list_the_line_runs_west_to_east() {
        let world = WorldConfig::default();
        let towns = place(
            world,
            &[
                config("Este", 2000.0, 0.0, (4, 4), true),
                config("Sin Tren", 0.0, 2500.0, (3, 3), false),
                config("Oeste", -2000.0, 0.0, (4, 4), true),
                config("Medio", 0.0, 0.0, (4, 4), true),
            ],
        );
        let (line, errors) = RailLine::build(None, &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line_names(&line, &towns), ["Oeste", "Medio", "Este"]);
        assert_eq!(line.links.len(), 2);
    }

    #[test]
    fn a_link_that_comes_back_across_its_own_towns_band_is_caught() {
        let band = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let out = [
            Vec2::new(50.0, 50.0),
            Vec2::new(150.0, 50.0),
            Vec2::new(300.0, 50.0),
        ];
        // Leaving the band and staying out is fine.
        assert!(!reenters(band, out.windows(2)));
        assert!(!reenters(
            band,
            [Vec2::new(200.0, 0.0), Vec2::new(300.0, 0.0)].windows(2)
        ));
        // Out and back in is not.
        let back = [
            Vec2::new(50.0, 50.0),
            Vec2::new(150.0, 50.0),
            Vec2::new(150.0, 200.0),
            Vec2::new(50.0, 200.0),
            Vec2::new(50.0, 90.0),
        ];
        assert!(reenters(band, back.windows(2)));
        // The shipped line never does.
        let (towns, line) = shipped();
        for link in &line.links {
            for (town, from_end) in [(link.from, false), (link.to, true)] {
                let band = towns[town]
                    .leveled()
                    .expanded(TownLayout::RAIL_CORRIDOR_HALF);
                let pieces = link.path.windows(2);
                let again = if from_end {
                    reenters(band, pieces.rev())
                } else {
                    reenters(band, pieces)
                };
                assert!(!again, "{}", towns[town].name);
            }
        }
    }

    #[test]
    fn a_link_across_a_third_town_is_left_out_naming_it() {
        let towns = row(false);
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build(Some(&names(&["Oeste", "Este"])), &towns, bounds);
        assert_eq!(line_names(&line, &towns), ["Oeste", "Este"]);
        assert!(line.links.is_empty(), "{:?}", line.links);
        assert_eq!(
            errors,
            [RailLineError::CrossesTown {
                from: "Oeste".into(),
                to: "Este".into(),
                other: "Medio".into(),
            }]
        );
        let message = errors[0].to_string();
        for name in ["Oeste", "Este", "Medio"] {
            assert!(message.contains(name), "{message}");
        }
        // The default line is held to the same rule.
        let (line, errors) = RailLine::build(None, &towns, bounds);
        assert!(line.links.is_empty());
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn unknown_duplicated_and_railway_less_names_warn_and_are_left_out() {
        let towns = row(false);
        let bounds = WorldConfig::default().bounds();
        let list = names(&["Oeste", "Rancul", "Oeste", "Medio"]);
        let (line, errors) = RailLine::build(Some(&list), &towns, bounds);
        assert_eq!(line_names(&line, &towns), ["Oeste"]);
        assert_eq!(
            errors,
            [
                RailLineError::UnknownTown {
                    name: "Rancul".into()
                },
                RailLineError::DuplicateTown {
                    name: "Oeste".into()
                },
                RailLineError::NoRailway {
                    name: "Medio".into()
                },
                RailLineError::NotOnLine {
                    name: "Este".into()
                },
            ]
        );
        for (error, name) in errors.iter().zip(["Rancul", "Oeste", "Medio", "Este"]) {
            assert!(error.to_string().contains(name), "{error}");
        }
    }

    #[test]
    fn a_railway_town_missing_from_the_list_warns() {
        let towns = row(true);
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build(Some(&names(&["Oeste", "Medio"])), &towns, bounds);
        assert_eq!(line_names(&line, &towns), ["Oeste", "Medio"]);
        assert_eq!(line.links.len(), 1);
        assert_eq!(
            errors,
            [RailLineError::NotOnLine {
                name: "Este".into()
            }]
        );
    }

    #[test]
    fn a_link_leaving_the_world_is_left_out() {
        let towns = row(true);
        // A world too small for the links (the towns themselves were placed
        // in a bigger one).
        let bounds = WorldBounds::centered(1200.0);
        let (line, errors) = RailLine::build(None, &towns, bounds);
        assert!(line.links.is_empty());
        assert_eq!(
            errors[0],
            RailLineError::OutsideWorld {
                from: "Oeste".into(),
                to: "Medio".into(),
            }
        );
        assert!(errors[0].to_string().contains("leaves the world"));
    }

    /// The line as `assets/world/towns.ron` ships it.
    pub(crate) fn shipped() -> (Vec<TownLayout>, RailLine) {
        let (world, towns) = pampa_line();
        let list = names(&["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (line, errors) = RailLine::build(Some(&list), &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line)
    }

    /// The largest heading change between consecutive pieces of `points`.
    fn sharpest_bend(points: &[Vec2]) -> f32 {
        points
            .windows(3)
            .map(|w| {
                let a = unit(w[1] - w[0]).expect("no zero-length piece");
                let b = unit(w[2] - w[1]).expect("no zero-length piece");
                turn(a, b).abs()
            })
            .fold(0.0, f32::max)
    }

    /// The tip of `rail` other than `tip`.
    fn other_tip(rail: Railway, tip: Vec2) -> Vec2 {
        if rail.start == tip {
            rail.end
        } else {
            rail.start
        }
    }

    #[test]
    fn the_line_bends_at_most_two_degrees_from_one_piece_to_the_next() {
        let (towns, line) = shipped();
        let rail = |i: usize| towns[i].railway.expect("a railway");
        for link in &line.links {
            // Ride in along the first town's track and out along the next's,
            // so the joins at the tips count too.
            let mut ride = vec![other_tip(rail(link.from), link.track.start)];
            ride.extend(&link.path);
            ride.push(other_tip(rail(link.to), link.track.end));
            let sharpest = sharpest_bend(&ride);
            assert!(
                sharpest <= RailLine::MAX_TURN + 1e-4,
                "bends {}° at once",
                sharpest.to_degrees()
            );
            // It does turn: the links run diagonally between east-west tracks.
            let turned: f32 = link.curves.iter().map(|c| c.turn.abs()).sum();
            assert!(turned.to_degrees() > 20.0, "{:?}", link.curves);
        }
    }

    #[test]
    fn each_link_path_runs_unbroken_from_tip_to_tip() {
        let (_, line) = shipped();
        for link in &line.links {
            assert_eq!(link.path.first(), Some(&link.track.start));
            assert_eq!(link.path.last(), Some(&link.track.end));
            let pieces: Vec<Railway> = link.segments().collect();
            assert_eq!(pieces.len(), link.path.len() - 1);
            for pair in pieces.windows(2) {
                assert_eq!(pair[0].end, pair[1].start);
            }
            // Curve pieces stay short; only the straights run long.
            let short = pieces
                .iter()
                .filter(|p| p.start.distance(p.end) <= RailLine::CURVE_STEP + 1e-2)
                .count();
            assert!(short >= pieces.len() - 1, "{short} of {}", pieces.len());
            // Bending adds only a little to the straight chord. At 1:6 the
            // Castex–Monte Nievas link turns 62° and is 6.8% longer than its
            // chord, so the bound leaves 8%.
            let chord = link.track.start.distance(link.track.end);
            assert!(
                link.length() > chord && link.length() < chord * 1.08,
                "{} {chord}",
                link.length()
            );
        }
    }

    #[test]
    fn curves_take_the_full_radius_where_the_straights_leave_room() {
        let (_, line) = shipped();
        for link in &line.links {
            assert_eq!(link.curves.len(), 2, "{:?}", link.curves);
            let chord = link.track.start.distance(link.track.end);
            for curve in &link.curves {
                assert!(
                    (curve.radius - RailLine::CURVE_RADIUS).abs() < 1.0,
                    "{curve:?}"
                );
                let expected = curve.radius * (curve.turn.abs() / 2.0).tan();
                assert!((curve.tangent - expected).abs() < 1e-2, "{curve:?}");
                assert!(curve.tangent <= chord / 4.0, "{curve:?}");
            }
            // Both town tracks run east-west: the link turns back the way
            // it turned.
            let net = link.curves[0].turn + link.curves[1].turn;
            assert!(net.abs() < 1e-3, "{:?}", link.curves);
        }
    }

    #[test]
    fn a_curve_between_short_straights_is_tightened_to_fit() {
        // A right-angle turn within 200 m each way: a 500 m radius would
        // need tangents of 207 m, but only a quarter of the 283 m chord fits.
        let (start, end) = (Vec2::ZERO, Vec2::new(200.0, 200.0));
        let (path, curves) = smooth_link(
            start,
            Vec2::new(1.0, 0.0),
            end,
            Vec2::new(0.0, 1.0),
            RailLine::CURVE_RADIUS,
        );
        let cap = start.distance(end) / 4.0;
        assert_eq!(curves.len(), 2, "{curves:?}");
        for curve in &curves {
            assert!((curve.tangent - cap).abs() < 1e-2, "{curve:?}");
            assert!(curve.radius < RailLine::CURVE_RADIUS, "{curve:?}");
            let fits = curve.radius * (curve.turn.abs() / 2.0).tan();
            assert!((fits - curve.tangent).abs() < 1e-2, "{curve:?}");
        }
        assert_eq!(path.first(), Some(&start));
        assert_eq!(path.last(), Some(&end));
        let mut ride = vec![Vec2::new(-10.0, 0.0)];
        ride.extend(&path);
        ride.push(Vec2::new(200.0, 210.0));
        assert!(sharpest_bend(&ride) <= RailLine::MAX_TURN + 1e-4);
    }

    #[test]
    fn a_link_already_heading_straight_on_has_no_curves() {
        let (path, curves) = smooth_link(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(900.0, 0.0),
            Vec2::new(1.0, 0.0),
            RailLine::CURVE_RADIUS,
        );
        assert!(curves.is_empty(), "{curves:?}");
        assert_eq!(path, [Vec2::ZERO, Vec2::new(900.0, 0.0)]);
    }

    #[test]
    fn segments_in_clips_the_links_to_an_area() {
        let (world, towns) = pampa_line();
        let (line, _) = RailLine::build(None, &towns, world.bounds());
        let link = &line.links[1];
        // The long straight in the middle of the link, and how far along
        // the link it starts.
        let len = |p: &Railway| p.start.distance(p.end);
        let (from, piece) = link
            .segments()
            .scan(0.0, |along, p| {
                let at = *along;
                *along += len(&p);
                Some((at, p))
            })
            .max_by(|a, b| len(&a.1).total_cmp(&len(&b.1)))
            .expect("pieces");
        let middle = (piece.start + piece.end) * 0.5;
        let area = Aabb::from_center_size(middle, Vec2::new(200.0, 200.0));
        let parts = line.segments_in(area);
        assert_eq!(parts.len(), 1, "{parts:?}");
        let part = parts[0];
        assert_eq!(part.track, piece);
        assert_eq!(part.from, from);
        assert_eq!(part.total, link.length());
        for p in [part.start(), part.end()] {
            assert!(area.expanded(1e-2).contains_point(p), "{p:?}");
        }
        assert!(part.exit - part.enter > 100.0);
        let half = from + len(&piece) / 2.0;
        assert!(part.enter < half && half < part.exit, "{part:?}");
        // The parts of two neighboring areas meet end to end.
        let east = Aabb::new(
            Vec2::new(area.max.x, area.min.y),
            Vec2::new(area.max.x + 200.0, area.max.y),
        );
        let next = line.segments_in(east);
        if let Some(next) = next.first() {
            assert!((next.enter - part.exit).abs() < 1e-2, "{next:?} {part:?}");
        }
        // An area off the line holds nothing.
        let off = Aabb::from_center_size(Vec2::new(0.0, -7000.0), Vec2::new(200.0, 200.0));
        assert!(line.segments_in(off).is_empty());
    }

    #[test]
    fn segments_in_measures_along_the_whole_curved_link() {
        let (world, towns) = pampa_line();
        let (line, _) = RailLine::build(None, &towns, world.bounds());
        let bounds = world.bounds();
        let parts = line.segments_in(Aabb::new(bounds.min, bounds.max));
        let link = &line.links[0];
        let parts = &parts[..link.path.len() - 1];
        assert_eq!(parts[0].enter, 0.0);
        for (part, ends) in parts.iter().zip(link.path.windows(2)) {
            assert_eq!(part.from, part.enter);
            assert_eq!(part.total, link.length());
            assert!(part.start().distance(ends[0]) < 1e-2);
            assert!(part.end().distance(ends[1]) < 1e-2);
        }
        for pair in parts.windows(2) {
            assert_eq!(pair[0].exit, pair[1].enter);
        }
        let last = parts.last().expect("parts");
        assert!((last.exit - link.length()).abs() < 1e-2);
    }

    #[test]
    fn the_full_path_runs_castex_to_pico_through_every_town_track() {
        let (towns, line) = shipped();
        let path = line.full_path(&towns);
        let rail = |i: usize| towns[i].railway.expect("a railway");
        let (castex, nievas, pico) = (rail(1), rail(0), rail(2));
        let points = path.points();
        // From Castex's west run-out tip to Pico's east run-out tip.
        assert_eq!(points[0], castex.start);
        assert_eq!(*points.last().unwrap(), pico.end);
        // Monte Nievas's whole straight track is one piece of the path.
        let i = points.iter().position(|&p| p == nievas.start).unwrap();
        assert_eq!(points[i + 1], nievas.end);
        assert_eq!(line.connected_towns(), [1, 0, 2]);
        // Arc length is the sum of the pieces, and it bends gently.
        let sum: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((path.length() - sum).abs() < 1e-2);
        assert!(sharpest_bend(points) <= RailLine::MAX_TURN + 1e-4);
        let links: f32 = line.links.iter().map(RailLink::length).sum();
        let tracks = [castex, nievas, pico].map(|r| r.start.distance(r.end));
        assert!((path.length() - links - tracks.iter().sum::<f32>()).abs() < 0.5);
    }

    #[test]
    fn line_paths_are_measured_by_arc_length() {
        let path = LinePath::new(vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 5.0),
        ]);
        assert_eq!(path.points().len(), 3, "repeats dropped");
        assert_eq!(path.length(), 15.0);
        assert_eq!(path.at(4.0), Vec2::new(4.0, 0.0));
        assert_eq!(path.at(12.0), Vec2::new(10.0, 2.0));
        assert_eq!(path.at(-3.0), Vec2::new(0.0, 0.0));
        assert_eq!(path.at(99.0), Vec2::new(10.0, 5.0));
        assert_eq!(path.direction_at(3.0), Vec2::new(1.0, 0.0));
        assert_eq!(path.direction_at(11.0), Vec2::new(0.0, 1.0));
        assert_eq!(path.project(Vec2::new(6.0, -2.0)), 6.0);
        assert_eq!(path.project(Vec2::new(14.0, 3.0)), 13.0);
        assert_eq!(LinePath::default().length(), 0.0);
        assert_eq!(LinePath::default().at(1.0), Vec2::ZERO);
    }

    #[test]
    fn a_gap_ends_the_full_path_and_a_lone_town_is_its_track() {
        let towns = row(true);
        let (mut line, _) = RailLine::build(None, &towns, WorldConfig::default().bounds());
        assert_eq!(line.links.len(), 2);
        line.links.remove(1);
        assert_eq!(line.connected_towns(), [0, 1]);
        let path = line.full_path(&towns);
        let medio = towns[1].railway.unwrap();
        assert_eq!(*path.points().last().unwrap(), medio.end);
        line.links.clear();
        let oeste = towns[0].railway.unwrap();
        assert_eq!(line.full_path(&towns).points(), [oeste.start, oeste.end]);
        assert_eq!(RailLine::default().full_path(&towns).length(), 0.0);
    }
}
