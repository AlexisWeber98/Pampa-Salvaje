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
//! A branch ([`RailBranch`]) leaves the line at a junction town and runs
//! out to towns only it serves: its first link starts at the junction's
//! tip nearest the branch, the same tip the line may already leave by, so
//! the two tracks part there on their own curves (Metileo to Caleufú, as
//! the real Metileo–Arizona branch of 1911).
//!
//! A branch's junction town also gets a bay ([`RailBay`]): a dead-end track
//! beside the station track, opposite the station platform, so the
//! branch's shuttle never stands on the line train's rails.
//!
//! The line is a world rule: every railway town must be on it or on a
//! branch, and no link may cross another town or leave the world. Bad data
//! never crashes; the offending towns or links are left out and reported.

use std::fmt;

use crate::train::{branch_consist, consist_length};
use crate::{Aabb, BuildingKind, Floor, Railway, RoadNetwork, TownLayout, Vec2, WorldBounds};

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
    /// The links between consecutive towns, in line order, then each
    /// branch's links in branch order. A link that breaks a rule is left
    /// out, so the line may have gaps.
    pub links: Vec<RailLink>,
    /// The branches off the line, in the order they were listed.
    pub branches: Vec<RailBranch>,
    /// The bay track at each branch's junction town, in branch order (a
    /// branch whose junction has no room for one has none).
    pub bays: Vec<RailBay>,
}

/// A branch of the railway: track from a town on the line (the junction)
/// out through towns no other track reaches. Its links live in
/// [`RailLine::links`] with the line's, so everything that lays or draws
/// track treats it alike; only the line train keeps to the line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RailBranch {
    /// Indices (into the world's towns) of the branch's towns, in order,
    /// the junction first.
    pub towns: Vec<usize>,
}

/// A dead-end bay track at a branch's junction town, beside the station
/// track on the side away from the town and joined to it by a turnout, so
/// the branch's shuttle stands there while the line train keeps the
/// station track.
///
/// Its [`Self::link`] is one more piece of track (`from` and `to` are the
/// junction), so everything that lays, draws or keeps off the line's
/// track sees the bay with no change. The path runs from the buffer stop,
/// level with the station platform's far end from the junction tip,
/// straight along the bay past the platform, then out through the turnout
/// (two arcs of [`RailBay::TURNOUT_RADIUS`]) onto the station track's
/// straight run-out toward the junction tip, where the branch goes on.
/// The shuttle stands opposite the station platform, at the bay's own
/// platform ([`Self::platform`]); people cross the tracks between the
/// platforms at the road's level crossing, on [`Self::walks`].
#[derive(Debug, Clone, PartialEq)]
pub struct RailBay {
    /// Index (into the world's branches) of the branch the bay serves.
    pub branch: usize,
    /// Index (into the world's towns) of the junction town.
    pub town: usize,
    /// The bay and its turnout, from the buffer stop to the point where it
    /// joins the station track.
    pub link: RailLink,
    /// Where the middle of the shuttle's consist stands, stopped in the
    /// bay opposite the middle of the station platform.
    pub stop: Vec2,
    /// The bay's platform, on its far side from the station track, along
    /// the standing shuttle and opposite the station platform: built and
    /// as high as a station platform.
    pub platform: Floor,
    /// The beaten-earth walks from each platform's end toward the junction
    /// tip to the road that crosses the station track there, so people
    /// change platforms over its level crossing: the station platform's
    /// walk, then the bay platform's. Empty until
    /// [`RailLine::lay_walks`] has seen the roads.
    pub walks: Vec<Aabb>,
}

impl RailBay {
    /// Distance from the station track's centerline to the bay's, in
    /// meters (3 m wide cars and a 1.5 m gap between them).
    pub const SPACING: f32 = 4.5;
    /// Free track between the buffer stop and the tail of a standing
    /// shuttle, in meters.
    pub const BUFFER_GAP: f32 = 2.0;
    /// Straight track past the front of a standing shuttle before the
    /// turnout begins, in meters.
    pub const FRONT_CLEAR: f32 = 10.0;
    /// Run-out left between the end of the turnout and the junction tip,
    /// in meters.
    const RUN_OUT_LEFT: f32 = 100.0;
    /// Radius of the bay's turnout arcs, in meters: tighter than the
    /// line's, so the bay rejoins the station track short of the road
    /// that crosses it by the platform (a sharp turnout, as a yard's).
    pub const TURNOUT_RADIUS: f32 = 150.0;
    /// Distance from the bay's centerline to its platform's track edge, in
    /// meters: as at a station (3 m wide cars leave 2.5 m).
    pub const PLATFORM_EDGE: f32 = 4.0;
    /// Width of the bay platform across the track, in meters.
    pub const PLATFORM_DEPTH: f32 = 7.0;
    /// Width of the walks from the platforms to the road, in meters.
    pub const WALK_WIDTH: f32 = 2.0;
    /// How far from the road's centerline a walk ends, in meters: inside
    /// the roadway's edge, so the two meet.
    const WALK_INTO_ROAD: f32 = 2.5;
    /// Farthest a walk reaches from its platform in search of the road,
    /// in meters.
    const WALK_REACH: f32 = 150.0;
    /// How near the station track's level crossing a walk meets the road,
    /// in meters.
    const WALK_NEAR_CROSSING: f32 = 25.0;

    /// The dead end of the track.
    pub fn buffer(&self) -> Vec2 {
        self.link.path[0]
    }

    /// Unit direction of the bay at its buffer stop, pointing out of the
    /// bay (the way the shuttle leaves).
    pub fn heading(&self) -> Vec2 {
        unit(self.link.path[1] - self.link.path[0]).unwrap_or(Vec2::new(1.0, 0.0))
    }
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
    /// A branch whose first town (`name`) is not on the line, or that
    /// names no town past it.
    BadJunction { name: String },
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
            Self::BadJunction { name } => write!(
                f,
                "the branch from `{name}` must start at a town on the line and reach a town past it; branch left out"
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
        Self::build_with_branches(names, &[], towns, bounds)
    }

    /// [`Self::build`], with the `branches` off the line: each lists a
    /// town on the line (the junction) and then, in order, the towns the
    /// branch runs out to. Branch towns follow the line's naming rules
    /// (never a town already on the line or another branch); a branch
    /// whose junction is not on the line, or that keeps no town past it,
    /// is left out and reported.
    pub fn build_with_branches(
        names: Option<&[String]>,
        branches: &[Vec<String>],
        towns: &[TownLayout],
        bounds: WorldBounds,
    ) -> (Self, Vec<RailLineError>) {
        let mut errors = Vec::new();
        let order = match names {
            Some(names) => listed(names, towns, &mut errors),
            None => west_to_east(towns),
        };
        let mut kept: Vec<RailBranch> = Vec::new();
        for list in branches {
            let junction = list
                .first()
                .and_then(|name| towns.iter().position(|t| &t.name == name))
                .filter(|i| order.contains(i));
            let taken: Vec<usize> = order
                .iter()
                .chain(kept.iter().flat_map(|b| &b.towns))
                .copied()
                .collect();
            let past = listed(list.get(1..).unwrap_or_default(), towns, &mut errors);
            let mut past = past;
            past.retain(|i| {
                let free = !taken.contains(i);
                if !free {
                    errors.push(RailLineError::DuplicateTown {
                        name: towns[*i].name.clone(),
                    });
                }
                free
            });
            match junction {
                Some(junction) if !past.is_empty() => {
                    let mut branch = vec![junction];
                    branch.extend(past);
                    kept.push(RailBranch { towns: branch });
                }
                _ => errors.push(RailLineError::BadJunction {
                    name: list.first().cloned().unwrap_or_default(),
                }),
            }
        }
        errors.extend(
            towns
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    t.railway.is_some()
                        && !order.contains(i)
                        && !kept.iter().any(|b| b.towns.contains(i))
                })
                .map(|(_, t)| RailLineError::NotOnLine {
                    name: t.name.clone(),
                }),
        );
        let mut links = Vec::new();
        let pairs = order
            .windows(2)
            .chain(kept.iter().flat_map(|b| b.towns.windows(2)));
        for pair in pairs {
            match link(towns, pair[0], pair[1], bounds) {
                Ok(link) => links.push(link),
                Err(error) => errors.push(error),
            }
        }
        let mut bays: Vec<RailBay> = Vec::new();
        for (index, branch) in kept.iter().enumerate() {
            let first = links
                .iter()
                .find(|l| l.from == branch.towns[0] && l.to == branch.towns[1]);
            let beside = bays.iter().filter(|b| b.town == branch.towns[0]).count();
            if let Some(bay) = first.and_then(|f| bay(towns, index, f, beside)) {
                bays.push(bay);
            }
        }
        (
            Self {
                towns: order,
                links,
                branches: kept,
                bays,
            },
            errors,
        )
    }

    /// The straight pieces of every link's track, in line order, then
    /// those of each bay.
    pub fn segments(&self) -> impl Iterator<Item = Railway> + '_ {
        self.tracks().flat_map(RailLink::segments)
    }

    /// Every piece of track between towns and in junction bays: the links,
    /// then the bays.
    fn tracks(&self) -> impl Iterator<Item = &RailLink> + '_ {
        self.links.iter().chain(self.bays.iter().map(|b| &b.link))
    }

    /// The parts of the links inside `area` (a terrain chunk, a map frame),
    /// one per straight piece, in line order. Parts that only touch its edge
    /// are skipped.
    pub fn segments_in(&self, area: Aabb) -> Vec<RailSpan> {
        let mut spans = Vec::new();
        for link in self.tracks() {
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
        self.path_through(&self.connected_towns(), towns)
    }

    /// The towns [`Self::full_path`] runs through, in line order: the first
    /// town and every town reached from it without a gap.
    pub fn connected_towns(&self) -> Vec<usize> {
        self.connected(&self.towns)
    }

    /// Each branch as one path, like [`Self::full_path`]: from the
    /// junction's far tip, through its station and out along the branch
    /// to the last connected town's far tip, so it runs outward from the
    /// line. A branch whose first link is missing is just the junction's
    /// track.
    pub fn branch_paths(&self, towns: &[TownLayout]) -> Vec<LinePath> {
        self.branches
            .iter()
            .map(|b| self.path_through(&self.connected(&b.towns), towns))
            .collect()
    }

    /// Every path of track a train could run: [`Self::full_path`] first,
    /// then [`Self::branch_paths`], then each bay from its buffer stop to
    /// where it joins the station track. Each town's own track and its
    /// run-outs are on one of them.
    pub fn track_paths(&self, towns: &[TownLayout]) -> Vec<LinePath> {
        let mut paths = vec![self.full_path(towns)];
        paths.extend(self.branch_paths(towns));
        paths.extend(self.bays.iter().map(|b| LinePath::new(b.link.path.clone())));
        paths
    }

    /// Each branch as the path its shuttle runs, like [`Self::branch_paths`]
    /// but out of the junction's bay: from the bay's buffer stop, through
    /// the turnout, along the station track's run-out and the branch to the
    /// last connected town's far tip. A branch without a bay runs its
    /// [`Self::branch_paths`] path.
    pub fn shuttle_paths(&self, towns: &[TownLayout]) -> Vec<LinePath> {
        self.branch_paths(towns)
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                let Some(bay) = self.bay_of(index) else {
                    return path;
                };
                // The bay joins the straight run-out: carry on from there.
                let joined = *bay.link.path.last().expect("a bay has a path");
                let at = path.project(joined) + CLIP_EPSILON;
                let mut points = bay.link.path.clone();
                points.extend(
                    path.points()
                        .iter()
                        .zip(&path.arcs)
                        .filter(|&(_, &arc)| arc > at)
                        .map(|(&p, _)| p),
                );
                LinePath::new(points)
            })
            .collect()
    }

    /// The bays' platforms, as floors people walk on and board from.
    pub fn floors(&self) -> Vec<Floor> {
        self.bays.iter().map(|b| b.platform).collect()
    }

    /// Lays each bay's [`RailBay::walks`]: from the end of the station
    /// platform and of the bay platform toward the junction tip, straight
    /// along the platform's middle to the road that crosses the station
    /// track there. People change trains over that level crossing: it lies
    /// past the turnout, beyond where any train stands, so it is always
    /// open. A bay with no such road, or one whose platforms do not reach
    /// it, gets no walks.
    pub fn lay_walks(&mut self, towns: &[TownLayout], roads: &RoadNetwork) {
        for bay in &mut self.bays {
            bay.walks = towns
                .get(bay.town)
                .and_then(|town| walks(bay, town, roads))
                .unwrap_or_default();
        }
    }

    /// The bay of branch `branch` (index into [`Self::branches`]).
    pub fn bay_of(&self, branch: usize) -> Option<&RailBay> {
        self.bays.iter().find(|b| b.branch == branch)
    }

    /// The towns of `order` reached from its first without a gap.
    fn connected(&self, order: &[usize]) -> Vec<usize> {
        let mut served: Vec<usize> = order.iter().take(1).copied().collect();
        for pair in order.windows(2) {
            if self.link(pair[0], pair[1]).is_none() {
                break;
            }
            served.push(pair[1]);
        }
        served
    }

    /// The path through `served` (towns joined by links, in order): the
    /// first town's far tip, every link, the last town's far tip.
    fn path_through(&self, served: &[usize], towns: &[TownLayout]) -> LinePath {
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

/// The bay for `branch` at its junction, beside the station track of the
/// town the branch's `first` link leaves, `beside` other bays already there
/// (each gets the next track out). The bay lies on the side of the track
/// away from the town, opposite the station platform: its buffer stop level
/// with the platform's end away from the junction tip, the shuttle standing
/// at the platform's middle, and its turnout on the run-out past the
/// platform's other end. `None`
/// for a town without a railway or one grown on both sides of its line,
/// and when the run-out is too short to hold the bay and its turnout.
fn bay(towns: &[TownLayout], branch: usize, first: &RailLink, beside: usize) -> Option<RailBay> {
    let town = towns.get(first.from)?;
    let rail = town.railway?;
    if town.rail_band().is_some() {
        return None;
    }
    let tip = first.track.start;
    let back = far_tip(rail, tip);
    let out = unit(tip - back)?;
    // The side of the track away from the town.
    let left = Vec2::new(-out.y, out.x);
    let side = if (town.grid().center() - back).dot(left) > 0.0 {
        left * -1.0
    } else {
        left
    };
    // Along the track from its far tip, and out from it.
    let at = |x: f32, y: f32| back + out * x + side * y;
    // The station platform's extent along the track, from its far tip.
    let platform = town
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Platform)
        .flat_map(|b| {
            let f = b.footprint;
            [
                f.min,
                f.max,
                Vec2::new(f.min.x, f.max.y),
                Vec2::new(f.max.x, f.min.y),
            ]
        })
        .map(|c| (c - back).dot(out))
        .fold(None, |span: Option<(f32, f32)>, x| {
            Some(span.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))))
        })
        .unwrap_or_else(|| {
            let middle = back.distance(tip) / 2.0;
            (middle, middle)
        });

    let consist = consist_length(&branch_consist());
    let gap = RailBay::SPACING * (beside + 1) as f32;
    let radius = RailBay::TURNOUT_RADIUS;
    // Two arcs of `radius` bring the bay down onto the track.
    let angle = (1.0 - gap / (2.0 * radius)).clamp(-1.0, 1.0).acos();
    let buffer = platform.0;
    let held = RailBay::BUFFER_GAP + consist;
    // The turnout starts at the platform's other end, past the shuttle's
    // front (a platform too short for it is outrun).
    let turnout = platform.1.max(buffer + held + RailBay::FRONT_CLEAR);
    let stop = ((platform.0 + platform.1) / 2.0).max(buffer + held - consist / 2.0);
    if turnout + 2.0 * radius * angle.sin() + RailBay::RUN_OUT_LEFT > back.distance(tip) {
        return None;
    }
    let pieces = (angle / RailLine::MAX_TURN)
        .max(radius * angle / RailLine::CURVE_STEP)
        .ceil()
        .max(1.0) as usize;
    let mut path = vec![at(buffer, gap), at(turnout, gap)];
    // First arc: leaves the straight bay toward the track.
    let (sin, cos) = (angle.sin(), angle.cos());
    for k in 1..=pieces {
        let phi = angle * k as f32 / pieces as f32;
        path.push(at(
            turnout + radius * phi.sin(),
            gap - radius * (1.0 - phi.cos()),
        ));
    }
    // Second arc: back to the track's heading.
    let (mid_x, mid_y) = (turnout + radius * sin, gap - radius * (1.0 - cos));
    for k in 1..=pieces {
        let psi = angle * k as f32 / pieces as f32;
        let heading = angle - psi;
        path.push(at(
            mid_x + radius * (sin - heading.sin()),
            mid_y - radius * (heading.cos() - cos),
        ));
    }
    // The first arc turns toward the track, the second back.
    let toward = turn(out, side * -1.0).signum();
    let tangent = radius * (angle / 2.0).tan();
    let curve = |sign: f32| RailCurve {
        radius,
        tangent,
        turn: sign * toward * angle,
    };
    let end = *path.last()?;
    // Axis-aligned boxes from frame corners (the towns' tracks run along
    // an axis): `x` along the track, `y` out from it.
    let area = |x: (f32, f32), y: (f32, f32)| {
        let (p, q) = (at(x.0, y.0), at(x.1, y.1));
        Aabb::new(
            Vec2::new(p.x.min(q.x), p.y.min(q.y)),
            Vec2::new(p.x.max(q.x), p.y.max(q.y)),
        )
    };
    let axis = |v: Vec2| {
        if v.x.abs() > v.y.abs() {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(0.0, 1.0)
        }
    };
    let edge = gap + RailBay::PLATFORM_EDGE;
    Some(RailBay {
        branch,
        town: first.from,
        link: RailLink {
            from: first.from,
            to: first.from,
            track: Railway {
                start: path[0],
                end,
                gauge: rail.gauge,
            },
            path,
            curves: vec![curve(1.0), curve(-1.0)],
        },
        stop: at(stop, gap),
        platform: Floor {
            footprint: area((buffer, turnout), (edge, edge + RailBay::PLATFORM_DEPTH)),
            height: Floor::PLATFORM_HEIGHT,
            along: axis(out),
        },
        walks: Vec::new(),
    })
}

/// The walks of `bay` (see [`RailLine::lay_walks`]): the station
/// platform's, then the bay platform's. `None` without a level crossing of
/// the station track past the turnout, or when a walk finds no road.
fn walks(bay: &RailBay, town: &TownLayout, roads: &RoadNetwork) -> Option<Vec<Aabb>> {
    let station = *town.floors().first()?;
    let out = bay.heading();
    let join = *bay.link.path.last()?;
    // Past the bay's join, out along the station track, on it.
    let crossing = roads
        .crossings()
        .filter(|c| {
            let rel = c.at - join;
            let off = rel.x * out.y - rel.y * out.x;
            off.abs() < 0.5 && rel.dot(out) > RailBay::WALK_INTO_ROAD
        })
        .min_by(|a, b| (a.at - join).dot(out).total_cmp(&(b.at - join).dot(out)))?;
    let reach = |floor: Floor| {
        let (along, across) = (floor.along, floor.across());
        // The platform's end toward the tip, and a ramp past it.
        let sign = along.dot(out).signum();
        let (lo, hi) = (
            floor.footprint.min.dot(along),
            floor.footprint.max.dot(along),
        );
        let start = (if sign > 0.0 { hi } else { lo }) + sign * Floor::RAMP_LENGTH;
        let line = floor.footprint.center().dot(across);
        let at = |a: f32| along * a + across * line;
        let steps = (RailBay::WALK_REACH / 0.25) as usize;
        let end = (0..=steps)
            .map(|k| start + sign * 0.25 * k as f32)
            .find(|&a| {
                roads
                    .near(at(a), RailBay::WALK_INTO_ROAD)
                    .is_some_and(|d| d <= RailBay::WALK_INTO_ROAD)
            })?;
        let (p, q) = (at(start), at(end));
        let half = across * (RailBay::WALK_WIDTH / 2.0);
        let corners = [p - half, p + half, q - half, q + half];
        let (mut min, mut max) = (corners[0], corners[0]);
        for c in corners {
            min = Vec2::new(min.x.min(c.x), min.y.min(c.y));
            max = Vec2::new(max.x.max(c.x), max.y.max(c.y));
        }
        // The road at the end of the walk is by the crossing.
        (at(end).distance(crossing.at) < RailBay::WALK_NEAR_CROSSING).then(|| Aabb::new(min, max))
    };
    Some(vec![reach(station)?, reach(bay.platform)?])
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
    use crate::{
        LineTrain, PlazaPlacement, RailwayConfig, Side, TownConfig, WorldConfig, place_towns,
    };

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
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
            density: None,
            townsfolk: None,
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
        // Monte Nievas, and the others at their real offsets from it
        // (east, north, in meters) scaled 1:7 and rounded to the meter.
        let nievas = (-2450.0, -1950.0);
        let at = |east: f32, north: f32| {
            (
                (nievas.0 + east / 7.0).round(),
                (nievas.1 + north / 7.0).round(),
            )
        };
        let (castex, pico) = (at(-12_600.0, -5_400.0), at(35_800.0, 22_400.0));
        let towns = place(
            world,
            &[
                config("Monte Nievas", nievas.0, nievas.1, (4, 3), true),
                config("Eduardo Castex", castex.0, castex.1, (5, 4), true),
                config("General Pico", pico.0, pico.1, (8, 8), true),
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
            // Bending adds only a little to the straight chord. At 1:7 the
            // Castex–Monte Nievas link turns 73° and is 9.2% longer than its
            // chord, so the bound leaves 10%.
            let chord = link.track.start.distance(link.track.end);
            assert!(
                link.length() > chord && link.length() < chord * 1.10,
                "{} {chord}",
                link.length()
            );
        }
    }

    #[test]
    fn curves_take_the_full_radius_where_the_straights_leave_room() {
        let (_, line) = shipped();
        let mut full = 0;
        for link in &line.links {
            assert_eq!(link.curves.len(), 2, "{:?}", link.curves);
            let chord = link.track.start.distance(link.track.end);
            for curve in &link.curves {
                let expected = curve.radius * (curve.turn.abs() / 2.0).tan();
                assert!((curve.tangent - expected).abs() < 1e-2, "{curve:?}");
                assert!(curve.tangent <= chord / 4.0 + 1e-2, "{curve:?}");
                if curve.tangent < chord / 4.0 - 1e-2 {
                    // Room to spare: the full radius.
                    assert!(
                        (curve.radius - RailLine::CURVE_RADIUS).abs() < 1.0,
                        "{curve:?}"
                    );
                    full += 1;
                } else {
                    // Capped at a quarter of the chord: tighter, but still a
                    // wide curve. At 1:7 the short Castex–Monte Nievas link
                    // turns 73° on curves of about 362 m.
                    assert!(curve.radius < RailLine::CURVE_RADIUS, "{curve:?}");
                    assert!(curve.radius > 300.0, "{curve:?}");
                }
            }
            // Both town tracks run east-west: the link turns back the way
            // it turned.
            let net = link.curves[0].turn + link.curves[1].turn;
            assert!(net.abs() < 1e-3, "{:?}", link.curves);
        }
        // The long Monte Nievas–General Pico link keeps the full radius.
        assert_eq!(full, 2, "{:?}", line.links);
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

    /// The row of three towns and Norte north-west of Medio, for a branch
    /// off the line at Medio.
    pub(crate) fn branched() -> (Vec<TownLayout>, Vec<String>, Vec<Vec<String>>) {
        let towns = place(
            WorldConfig::default(),
            &[
                config("Oeste", -2000.0, 0.0, (4, 4), true),
                config("Medio", 0.0, 0.0, (4, 4), true),
                config("Este", 2000.0, 0.0, (4, 4), true),
                config("Norte", -2200.0, 1800.0, (4, 4), true),
            ],
        );
        (
            towns,
            names(&["Oeste", "Medio", "Este"]),
            vec![names(&["Medio", "Norte"])],
        )
    }

    #[test]
    fn a_branch_leaves_the_line_at_its_junction_tip_and_runs_out_to_its_town() {
        let (towns, list, branches) = branched();
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.towns, [0, 1, 2]);
        assert_eq!(line.branches, [RailBranch { towns: vec![1, 3] }]);
        let pairs: Vec<(usize, usize)> = line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(pairs, [(0, 1), (1, 2), (1, 3)]);
        // The branch starts at Medio's west tip, where the line arrives from
        // Oeste, and reaches Norte's tip nearest Medio.
        let (into_medio, branch) = (&line.links[0], &line.links[2]);
        let (medio, norte) = (towns[1].railway.unwrap(), towns[3].railway.unwrap());
        assert_eq!(branch.track.start, medio.start);
        assert_eq!(into_medio.track.end, medio.start);
        assert_eq!(branch.track.end, norte.end);
        // Without the branch, Norte is a railway town off the line.
        let (_, errors) = RailLine::build(Some(&list), &towns, bounds);
        assert_eq!(
            errors,
            [RailLineError::NotOnLine {
                name: "Norte".into()
            }]
        );
    }

    #[test]
    fn the_branch_path_runs_outward_unbroken_and_the_train_keeps_to_the_line() {
        let (towns, list, branches) = branched();
        let bounds = WorldConfig::default().bounds();
        let (line, _) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        let (plain, _) = RailLine::build(Some(&list), &towns, bounds);
        // The line train's path is the line's alone.
        assert_eq!(line.full_path(&towns), plain.full_path(&towns));
        assert_eq!(line.connected_towns(), [0, 1, 2]);
        let paths = line.branch_paths(&towns);
        assert_eq!(paths.len(), 1);
        let points = paths[0].points();
        // From Medio's far (east) tip, through its station and the junction,
        // out to Norte's far (west) tip, never bending more than the line.
        let (medio, norte) = (towns[1].railway.unwrap(), towns[3].railway.unwrap());
        assert_eq!(points.first(), Some(&medio.end));
        assert!(points.contains(&medio.start));
        assert_eq!(points.last(), Some(&norte.start));
        assert!(sharpest_bend(points) <= RailLine::MAX_TURN + 1e-4);
        let expected = medio.start.distance(medio.end)
            + line.links[2].length()
            + norte.start.distance(norte.end);
        assert!((paths[0].length() - expected).abs() < 0.1);
        // The line, the branch, then the junction's bay.
        let tracks = line.track_paths(&towns);
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[1], paths[0]);
        assert_eq!(tracks[2].points(), line.bays[0].link.path);
        // Laid the same way every time.
        let (again, _) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert_eq!(again, line);
    }

    #[test]
    fn bad_branches_warn_and_are_left_out() {
        let (towns, list, _) = branched();
        let bounds = WorldConfig::default().bounds();
        let build = |branches: &[&[&str]]| {
            let branches: Vec<Vec<String>> = branches.iter().map(|b| names(b)).collect();
            RailLine::build_with_branches(Some(&list), &branches, &towns, bounds)
        };
        let bad = |name: &str| RailLineError::BadJunction { name: name.into() };
        let off = RailLineError::NotOnLine {
            name: "Norte".into(),
        };
        // The junction must be on the line.
        let (line, errors) = build(&[&["Norte", "Este"]]);
        assert!(line.branches.is_empty() && line.links.len() == 2);
        assert!(errors.contains(&bad("Norte")), "{errors:?}");
        // A branch needs a town past its junction, one not on the line.
        let (_, errors) = build(&[&["Medio"]]);
        assert_eq!(errors, [bad("Medio"), off.clone()]);
        let (_, errors) = build(&[&["Medio", "Este"]]);
        let duplicate = RailLineError::DuplicateTown {
            name: "Este".into(),
        };
        assert_eq!(errors, [duplicate, bad("Medio"), off.clone()]);
        let (_, errors) = build(&[&["Medio", "Nada"]]);
        let unknown = RailLineError::UnknownTown {
            name: "Nada".into(),
        };
        assert_eq!(errors, [unknown, bad("Medio"), off]);
        assert!(bad("Medio").to_string().contains("branch left out"));
        // A town is on one branch only.
        let (line, errors) = build(&[&["Medio", "Norte"], &["Oeste", "Norte"]]);
        assert_eq!(line.branches.len(), 1);
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    /// The branched world's one bay, with the junction's station track.
    fn bayed() -> (Vec<TownLayout>, RailLine, Railway) {
        let (towns, list, branches) = branched();
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        let track = towns[1].railway.unwrap();
        (towns, line, track)
    }

    /// Distance from `p` to the segment `a`-`b`.
    fn gap(p: Vec2, a: Vec2, b: Vec2) -> f32 {
        let d = b - a;
        let t = ((p - a).dot(d) / d.dot(d)).clamp(0.0, 1.0);
        p.distance(a + d * t)
    }

    #[test]
    fn a_junction_town_gets_a_bay_beside_its_station_track_at_the_junction_end() {
        let (towns, line, track) = bayed();
        // One bay, for the one branch, at its junction (Medio).
        assert_eq!(line.bays.len(), 1);
        let bay = line.bay_of(0).expect("a bay");
        assert_eq!((bay.branch, bay.town), (0, 1));
        assert_eq!((bay.link.from, bay.link.to), (1, 1));
        assert_eq!(line.bay_of(1), None);
        let medio = &towns[1];
        // Beside the track on the side away from the town, 4.5 m out.
        let (buffer, heading) = (bay.buffer(), bay.heading());
        // Broad-gauge track spacing: 4.5 m, a cars' width and a half.
        assert_eq!(RailBay::SPACING, 4.5);
        assert!((gap(buffer, track.start, track.end) - 4.5).abs() < 1e-3);
        assert!(buffer.y > track.start.y, "away from the town (north)");
        assert!(medio.grid().center().y < track.start.y);
        // It leaves toward the junction tip (west), the dead end to the east.
        let tip = line.links[2].track.start;
        assert_eq!(tip, track.start);
        assert!(
            (heading - Vec2::new(-1.0, 0.0)).length() < 1e-3,
            "{heading:?}"
        );
        assert!(buffer.x > tip.x && buffer.x < track.end.x);
        // The buffer stop stands level with the station platform's east end
        // (far from the junction tip), so the bay runs along the platform.
        let platform = medio
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .unwrap();
        assert!((buffer.x - platform.footprint.max.x).abs() < 1e-3);
        // Room for the shuttle: the consist, the gap at the buffer and the
        // clear track before the turnout.
        let straight = bay
            .link
            .path
            .iter()
            .take_while(|p| (p.y - buffer.y).abs() < 1e-3)
            .last()
            .unwrap();
        let length = buffer.distance(*straight);
        let consist = consist_length(&branch_consist());
        assert!(consist > 30.0 && consist < 50.0, "{consist}");
        assert!(
            length >= consist + 10.0,
            "{length} m for a {consist} m consist"
        );
        // The consist's middle stands in the bay opposite the middle of the
        // station platform, its tail clear of the buffer.
        assert!((bay.stop.y - buffer.y).abs() < 1e-3);
        assert!((bay.stop.x - platform.footprint.center().x).abs() < 1e-3);
        assert!(buffer.x - bay.stop.x - consist / 2.0 >= RailBay::BUFFER_GAP);
        // Clear of every building and of the platform, by the cars' width.
        let wide = 3.0 / 2.0 + 0.5;
        for b in &medio.buildings {
            let keep = b.footprint.expanded(wide);
            for w in bay.link.path.windows(2) {
                assert!(keep.segment_overlap(w[0], w[1]).is_none(), "{:?}", b.kind);
            }
        }
        // A town no branch leaves from gets none, nor does a line without
        // branches.
        assert!(line.bays.iter().all(|b| b.town == 1));
        let (_, list, _) = branched();
        let (plain, _) = RailLine::build(Some(&list), &towns, WorldConfig::default().bounds());
        assert!(plain.bays.is_empty());
        assert_eq!(plain.track_paths(&towns).len(), 1);
    }

    #[test]
    fn the_bay_has_a_station_built_platform_along_the_standing_shuttle() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let (buffer, out) = (bay.buffer(), bay.heading());
        // Across the tracks, away from the station track.
        let across = Vec2::new(-out.y, out.x);
        let across = across * across.dot(buffer - track.start).signum();
        let platform = bay.platform;
        let station = towns[1].floors()[0];
        // The station platform's build: height, depth, its edge 4 m from
        // the track, on the bay's far side from the station track.
        assert_eq!(platform.height, station.height);
        let (lo, hi) = (platform.footprint.min, platform.footprint.max);
        let corners = [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y)];
        let off = |c: &Vec2| (*c - buffer).dot(across);
        let (near, far) = corners
            .iter()
            .map(off)
            .fold((f32::MAX, f32::MIN), |(n, f), d| (n.min(d), f.max(d)));
        assert!(
            (near - 4.0).abs() < 1e-3 && (far - 11.0).abs() < 1e-3,
            "{near} {far}"
        );
        let depth = |f: &Floor| f.footprint.size().x.min(f.footprint.size().y);
        assert_eq!(depth(&platform), depth(&station));
        // The cars' side (1.5 m from the centerline) leaves 2.5 m to it.
        assert!(near - LineTrain::WIDTH / 2.0 >= 2.5 - 1e-3);
        // Along the track it runs from the buffer stop past the front of
        // the standing consist, whole of it beside the platform.
        let consist = consist_length(&branch_consist());
        let along = |c: Vec2| (c - buffer).dot(out);
        let (a, b) = corners
            .iter()
            .map(|c| along(*c))
            .fold((f32::MAX, f32::MIN), |(l, h), d| (l.min(d), h.max(d)));
        assert!(a.abs() < 1e-3);
        assert!(b >= RailBay::BUFFER_GAP + consist);
        assert_eq!(platform.along.dot(out).abs(), 1.0);
        // Beside the straight bay: before the turnout bends it away.
        assert!(b <= bay.link.path[1].distance(buffer) + 1e-3);
    }

    #[test]
    fn the_bay_platform_stands_opposite_the_station_platform_with_the_shuttle_between() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let station = towns[1].floors()[0];
        let platform = bay.platform;
        // The same stretch of the track, end to end, 60 m of it.
        assert_eq!(platform.footprint.min.x, station.footprint.min.x);
        assert_eq!(platform.footprint.max.x, station.footprint.max.x);
        assert_eq!(platform.footprint.size().x, 60.0);
        // On the other side of the station track from the station platform.
        assert!(station.footprint.max.y < track.start.y);
        assert!(platform.footprint.min.y > track.start.y);
        // The standing shuttle's coaches lie wholly opposite it, nose and
        // tail within the platform's length, at its middle.
        let consist = consist_length(&branch_consist());
        let (stop, half) = (bay.stop.x, consist / 2.0);
        assert!(stop - half >= platform.footprint.min.x, "{stop}");
        assert!(stop + half <= platform.footprint.max.x, "{stop}");
        assert!((stop - station.footprint.center().x).abs() < 1e-3);
        // The turnout begins where the platform ends, so the bay runs
        // straight the whole length of the platform.
        let straight = bay.link.path[1];
        assert!((straight.x - platform.footprint.min.x).abs() < 1e-3);
        // The line's floors are the bays' platforms.
        assert_eq!(line.floors(), [platform]);
    }

    #[test]
    fn the_bay_joins_the_station_track_by_a_smooth_turnout_within_the_run_out() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let path = &bay.link.path;
        assert!(sharpest_bend(path) <= RailLine::MAX_TURN + 1e-4);
        // Two arcs of the bay's 150 m radius, bending opposite ways, no net
        // turn.
        let curves = &bay.link.curves;
        assert_eq!(curves.len(), 2);
        assert!(curves.iter().all(|c| (c.radius - 150.0).abs() < 1e-3));
        assert!((curves[0].turn + curves[1].turn).abs() < 1e-5);
        // It ends on the station track, heading along it, short of the tip.
        let join = *path.last().unwrap();
        assert!((join.y - track.start.y).abs() < 1e-2, "{join:?}");
        assert!(join.x > track.start.x + 90.0, "within the run-out");
        let last = path[path.len() - 1] - path[path.len() - 2];
        assert!(last.y.abs() / last.length() < 0.02, "{last:?}");
        // The turnout lies past the platform, never over the yard.
        let medio = &towns[1];
        let grid = medio.grid();
        for p in path {
            assert!(p.y > grid.max.y, "{p:?}");
        }
        // The link measures like any other.
        assert_eq!(bay.link.track.start, bay.buffer());
        assert_eq!(bay.link.track.end, join);
        let length: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((bay.link.length() - length).abs() < 1e-3);
    }

    #[test]
    fn the_shuttle_path_runs_unbroken_from_the_buffer_stop_to_the_branch_end() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let paths = line.shuttle_paths(&towns);
        assert_eq!(paths.len(), 1);
        let points = paths[0].points();
        let norte = towns[3].railway.unwrap();
        assert_eq!(points.first(), Some(&bay.buffer()));
        assert_eq!(points.last(), Some(&norte.start));
        assert!(sharpest_bend(points) <= RailLine::MAX_TURN + 1e-4);
        // Out of the bay it takes the branch path's own track: the station
        // track's run-out west of the turnout, then the branch link.
        let join = *bay.link.path.last().unwrap();
        let on_branch = line.branch_paths(&towns).remove(0);
        let rest = on_branch.length() - on_branch.project(join);
        assert!((paths[0].length() - (bay.link.length() + rest)).abs() < 0.1);
        assert!(points.contains(&track.start));
        let at = paths[0].project(join);
        // Until the last stretch of the turnout it keeps off the station
        // track, so no train on it shares the line train's rails.
        for k in 0..=200 {
            let d = at * k as f32 / 200.0;
            if d < at - 20.0 {
                let off = gap(paths[0].at(d), track.start, track.end);
                assert!(off > 0.3, "{d} {off}");
            }
        }
        // Laid the same way every time; a branch without a bay keeps its path.
        let (again, _) = RailLine::build_with_branches(
            Some(&names(&["Oeste", "Medio", "Este"])),
            &[names(&["Medio", "Norte"])],
            &towns,
            WorldConfig::default().bounds(),
        );
        assert_eq!(again.shuttle_paths(&towns), paths);
        let mut bare = line.clone();
        bare.bays.clear();
        assert_eq!(bare.shuttle_paths(&towns), bare.branch_paths(&towns));
    }

    #[test]
    fn the_bay_is_track_everything_that_keeps_off_track_sees() {
        let (towns, line, _) = bayed();
        let bay = line.bay_of(0).unwrap();
        // The pieces of the bay are among the line's pieces.
        let pieces: Vec<Railway> = line.segments().collect();
        for w in bay.link.path.windows(2) {
            assert!(pieces.iter().any(|r| (r.start, r.end) == (w[0], w[1])));
        }
        // And are cut to the area around it like any link's.
        let area = Aabb::from_center_size(bay.stop, Vec2::new(40.0, 40.0));
        let spans = line.segments_in(area);
        assert!(spans.iter().any(|s| s.total == bay.link.length()));
        // Roads see it as a track path of its own.
        assert_eq!(
            line.track_paths(&towns).last().unwrap().points(),
            bay.link.path
        );
    }
}
