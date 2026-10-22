//! Wire fences (alambrados) marking out the private land between towns:
//! estancias, tambos, fields and plots, with wooden gates (tranqueras)
//! into them.
//!
//! Roads are public (user, 2026-10-22): no fence or gate ever crosses one.
//! Every road runs between two fences, one along each side
//! [`FenceLayout::ROADSIDE`] meters from its centerline, following its
//! curves: the classic "calle entre alambrados". They end at the towns'
//! leveled ground, by the railway and so at the level crossings.
//!
//! Away from the roads the land is surveyed on a north–south grid, as the
//! pampa was: fence lines run east–west and north–south a seeded
//! 300–800 m apart, so paddocks are a few hundred meters on a side. A grid
//! line stops at a roadside fence and joins it at a corner post. Every
//! stretch of roadside fence between two corners is a field's frontage on
//! the road, and gets one or two open gates into the field. The grid lines
//! have openings every 300–500 m (user, "Alambrados con huecos",
//! 2026-10-22): a field gate standing open, or a broken span.
//!
//! Posts stand at most [`FenceLayout::POST_SPACING`] meters apart, with
//! [`FenceLayout::WIRES`] wires between them. Every span is a thin static
//! obstacle (short boxes along it, so a slanted span stays thin), so riders
//! and walkers are stopped by the wire and get through at the gates, the
//! openings or round a fence's end. Gates and openings are no obstacle.

use crate::rng::Rng;
use crate::{Aabb, RailLine, RoadNetwork, Terrain, TownLayout, Vec2, WorldConfig};

/// One unbroken run of fence: its posts in order, at most
/// [`FenceLayout::POST_SPACING`] meters apart.
#[derive(Debug, Clone, PartialEq)]
pub struct FenceRun {
    pub posts: Vec<Vec2>,
    /// Along a road, rather than on the survey grid.
    pub roadside: bool,
}

/// An open gate in a roadside fence, from the road into a field: two gate
/// posts `width` apart, centered on `at`, along the fence there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub at: Vec2,
    /// Unit direction of the fence at the gate.
    pub along: Vec2,
    pub width: f32,
    /// Two leaves rather than one.
    pub double: bool,
    /// Standing open; a closed gate blocks riders like the wire.
    pub open: bool,
    /// The field frontage it opens from (an index into
    /// [`FenceLayout::frontages`]).
    pub frontage: u32,
}

/// One field's frontage on a road: a stretch of roadside fence between two
/// corners where grid lines meet it, or the fence's ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frontage {
    pub start: Vec2,
    pub end: Vec2,
    /// Length along the fence, in meters.
    pub length: f32,
    /// Fenced, with gates; `false` along a descampado.
    pub fenced: bool,
}

/// A passable opening in a grid fence (user, "Alambrados con huecos",
/// 2026-10-22): one or two spans, centered on `at`, `width` meters along the
/// fence, with no obstacle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opening {
    pub at: Vec2,
    /// Unit direction of the fence line.
    pub along: Vec2,
    pub width: f32,
    pub kind: OpeningKind,
    /// The fenced stretch it was cut into (see [`FenceLayout::stretches`]).
    pub stretch: u32,
}

/// What a fence opening looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningKind {
    /// A field gate with one leaf or two, open or closed.
    FieldGate { double: bool, open: bool },
    /// Fallen or broken wire: leaning posts and a sagging strand.
    Broken,
}

/// Every fence, gate and opening of the world.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FenceLayout {
    pub runs: Vec<FenceRun>,
    /// Open gates from the roads into the fields.
    pub gates: Vec<Gate>,
    /// Every field frontage on a road.
    pub frontages: Vec<Frontage>,
    /// Field gates and broken spans along the grid fences.
    pub openings: Vec<Opening>,
    /// Every fenced grid stretch before its openings were cut: its id and
    /// length in meters.
    pub stretches: Vec<(u32, f32)>,
    /// Where the north–south lines run (their `x`) and the east–west ones
    /// (their `y`), whether fenced there or not.
    pub surveyed: (Vec<f32>, Vec<f32>),
    /// The paddocks left unfenced (descampados, abandoned fields), as
    /// `(i, j)`: between the `i`th and next north–south line and the `j`th
    /// and next east–west line.
    pub descampados: Vec<(usize, usize)>,
}

/// What one span of a grid line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    Fenced,
    Opening,
    Road,
    Blocked,
}

/// A roadside fence before it is cut into frontages and gates: a dense
/// line beside a road, and the corners where grid lines join it (as
/// distances along it).
struct Roadside {
    line: Vec<Vec2>,
    arcs: Vec<f32>,
    corners: Vec<f32>,
}

impl Roadside {
    fn length(&self) -> f32 {
        self.arcs.last().copied().unwrap_or(0.0)
    }

    /// The point `s` meters along, and the unit direction there.
    fn at(&self, s: f32) -> (Vec2, Vec2) {
        let i = self
            .arcs
            .partition_point(|&a| a <= s)
            .saturating_sub(1)
            .min(self.line.len() - 2);
        let (a, b) = (self.line[i], self.line[i + 1]);
        let span = (self.arcs[i + 1] - self.arcs[i]).max(1e-6);
        let t = ((s - self.arcs[i]) / span).clamp(0.0, 1.0);
        (a + (b - a) * t, (b - a) * (1.0 / span))
    }

    /// Posts from `from` to `to` meters along, at most a post spacing apart,
    /// closer round tight bends so no span cuts the curve by more than a
    /// few centimeters.
    fn posts(&self, from: f32, to: f32) -> Vec<Vec2> {
        let n = ((to - from) / FenceLayout::POST_SPACING).ceil().max(1.0) as usize;
        let mut posts = vec![self.at(from).0];
        for k in 0..n {
            let (a, b) = (
                from + (to - from) * (k as f32 / n as f32),
                from + (to - from) * ((k + 1) as f32 / n as f32),
            );
            self.subdivide(a, b, &mut posts);
        }
        posts
    }

    /// Pushes the post at `b`, after any needed between `a` and `b`.
    fn subdivide(&self, a: f32, b: f32, posts: &mut Vec<Vec2>) {
        let (pa, pb) = (self.at(a).0, self.at(b).0);
        let mid = self.at((a + b) / 2.0).0;
        if b - a > 1.0 && mid.distance((pa + pb) * 0.5) > 0.05 {
            self.subdivide(a, (a + b) / 2.0, posts);
            self.subdivide((a + b) / 2.0, b, posts);
        } else {
            posts.push(pb);
        }
    }
}

impl FenceLayout {
    /// Greatest distance between posts, in meters.
    pub const POST_SPACING: f32 = 10.0;
    /// Wires strung between the posts.
    pub const WIRES: usize = 5;
    /// Height of the posts above the ground, in meters.
    pub const HEIGHT: f32 = 1.3;
    /// Thickness of a span's obstacle, in meters: about a post.
    pub const THICKNESS: f32 = 0.15;
    /// Least and most distance between two parallel grid lines, in meters.
    pub const FIELD: (f32, f32) = (300.0, 800.0);
    /// Distance from a road's centerline to the fences along it, in
    /// meters (8.5 m): its edge plus a verge of 5.25 m (so a span cut
    /// across a tight bend still leaves 5 m), so the public calle between the
    /// fences has room for carts and, later, a route.
    pub const ROADSIDE: f32 = RoadNetwork::HALF_WIDTH + 5.25;
    /// Least distance from a fence to the track's centerline, in meters.
    pub const RAIL_MARGIN: f32 = 15.0;
    /// Least distance from a fence to a town's leveled band, in meters.
    pub const TOWN_MARGIN: f32 = 10.0;
    /// Least and most distance between two openings of a grid stretch, in
    /// meters, and how far in from its start the first one is.
    pub const OPENING_SPACING: (f32, f32) = (300.0, 500.0);
    const FIRST_OPENING: (f32, f32) = (100.0, 350.0);
    /// Widths of a single and a double gate into a field, in meters.
    pub const GATE_WIDTH: (f32, f32) = (4.5, 8.0);
    /// Shortest frontage that gets a gate, in meters.
    pub const MIN_FRONTAGE: f32 = 40.0;
    /// Least distance from a gate to a corner, in meters.
    const GATE_CORNER: f32 = 12.0;
    /// Share of field gates standing open; the rest are closed.
    pub const OPEN_GATES: f32 = 0.6;
    /// Share of paddocks left unfenced (descampados).
    pub const DESCAMPADOS: f32 = 0.1;
    /// Least distance from a fence line to the world's edge, in meters.
    pub const EDGE_MARGIN: f32 = 10.0;
    /// Spacing of the dense line a roadside fence is traced along, meters.
    const TRACE: f32 = 2.0;

    /// The fences of `config`'s world over `terrain` (with its towns
    /// leveled and its roads laid), kept off the towns of `towns`, the
    /// `line`'s track and the roads.
    pub fn build(
        config: WorldConfig,
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
    ) -> Self {
        let bounds = config.bounds();
        let lo = bounds.min + Vec2::new(Self::EDGE_MARGIN, Self::EDGE_MARGIN);
        let hi = bounds.max - Vec2::new(Self::EDGE_MARGIN, Self::EDGE_MARGIN);
        let mut rng = Rng::new(terrain.seed ^ FENCE_SEED, 0);
        let mut survey = |from: f32, to: f32| {
            let mut at = from + rng.range(0.0, Self::FIELD.1);
            let mut lines = Vec::new();
            while at < to {
                lines.push(at);
                at += rng.range(Self::FIELD.0, Self::FIELD.1);
            }
            lines
        };
        let xs = survey(lo.x, hi.x);
        let ys = survey(lo.y, hi.y);
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        let bands: Vec<Aabb> = towns
            .iter()
            .map(|t| t.leveled().expanded(Self::TOWN_MARGIN))
            .collect();
        let descampados = (0..xs.len().saturating_sub(1))
            .flat_map(|i| (0..ys.len().saturating_sub(1)).map(move |j| (i, j)))
            .filter(|&(i, j)| {
                Rng::new(terrain.seed ^ FENCE_SEED ^ 0xdeca, (i * 4099 + j) as u32).unit()
                    < Self::DESCAMPADOS
            })
            .collect();
        let mut layout = Self {
            surveyed: (xs.clone(), ys.clone()),
            descampados,
            ..Self::default()
        };
        let site = Site {
            terrain,
            rails: &rails,
            bands: &bands,
            lo,
            hi,
        };
        let mut sides = terrain
            .roads
            .as_deref()
            .map(|roads| roadsides(roads, &site))
            .unwrap_or_default();
        let lines = xs
            .iter()
            .map(|&x| (Vec2::new(x, lo.y), Vec2::new(x, hi.y)))
            .chain(ys.iter().map(|&y| (Vec2::new(lo.x, y), Vec2::new(hi.x, y))));
        for (start, end) in lines {
            layout.lay_line(start, end, &site, &mut sides);
        }
        for side in &mut sides {
            layout.cut_frontages(side);
        }
        layout.keep_fields_reachable(&site);
        layout
    }

    /// How many paddocks the survey makes (between lines on all sides).
    pub fn cells(&self) -> usize {
        self.surveyed.0.len().saturating_sub(1) * self.surveyed.1.len().saturating_sub(1)
    }

    /// The paddock `p` is in, if it lies between survey lines on all sides.
    fn cell_of(&self, p: Vec2) -> Option<(usize, usize)> {
        let index = |lines: &[f32], v: f32| {
            let k = lines.partition_point(|&l| l <= v);
            (k > 0 && k < lines.len()).then(|| k - 1)
        };
        Some((index(&self.surveyed.0, p.x)?, index(&self.surveyed.1, p.y)?))
    }

    /// The paddock `p` is in, between survey lines on all sides, as a box.
    pub fn field_of(&self, p: Vec2) -> Option<Aabb> {
        let (i, j) = self.cell_of(p)?;
        let (xs, ys) = &self.surveyed;
        Some(Aabb::new(
            Vec2::new(xs[i], ys[j]),
            Vec2::new(xs[i + 1], ys[j + 1]),
        ))
    }

    /// Whether `p` is in an unfenced paddock.
    pub fn is_descampado(&self, p: Vec2) -> bool {
        self.cell_of(p)
            .is_some_and(|c| self.descampados.binary_search(&c).is_ok())
    }

    /// Whether the grid span from `a` to `b` lies between two unfenced
    /// paddocks, so neither has a fence there.
    fn between_descampados(&self, a: Vec2, b: Vec2) -> bool {
        let mid = (a + b) * 0.5;
        let across = Vec2::new(-(b - a).y, (b - a).x);
        let across = across * (1.0 / across.length().max(1e-6));
        self.is_descampado(mid + across) && self.is_descampado(mid - across)
    }

    /// Lays the posts of the grid line from `start` to `end`, stopping it
    /// at the roadside fences it meets.
    fn lay_line(&mut self, start: Vec2, end: Vec2, site: &Site, sides: &mut [Roadside]) {
        let length = start.distance(end);
        let along = (end - start) * (1.0 / length);
        let spans = (length / Self::POST_SPACING).floor() as usize;
        let post = |k: usize| start + along * (k as f32 * Self::POST_SPACING);
        let whole = Aabb::new(
            Vec2::new(start.x.min(end.x), start.y.min(end.y)),
            Vec2::new(start.x.max(end.x), start.y.max(end.y)),
        )
        .expanded(Self::RAIL_MARGIN);
        let near_rails: Vec<(Vec2, Vec2)> = site
            .rails
            .iter()
            .copied()
            .filter(|&(a, b)| whole.segment_overlap(a, b).is_some())
            .collect();
        // Inside the roadside fences, plus a little room for the corner.
        let road_reach = Self::ROADSIDE + 0.5;
        let mut kind: Vec<Span> = (0..spans)
            .map(|k| {
                let (a, b) = (post(k), post(k + 1));
                let blocked = site
                    .bands
                    .iter()
                    .any(|band| band.segment_overlap(a, b).is_some())
                    || near_rails
                        .iter()
                        .any(|&(r0, r1)| segments_distance(a, b, r0, r1) < Self::RAIL_MARGIN);
                let on_road = site.terrain.roads.as_ref().is_some_and(|roads| {
                    [a, (a + b) * 0.5, b]
                        .iter()
                        .any(|&p| roads.near(p, road_reach).is_some())
                });
                if blocked || self.between_descampados(a, b) {
                    Span::Blocked
                } else if on_road {
                    Span::Road
                } else {
                    Span::Fenced
                }
            })
            .collect();
        self.cut_openings(&mut kind, start, along);
        let mut k = 0;
        while k < spans {
            let first = k;
            while k < spans && kind[k] == kind[first] {
                k += 1;
            }
            if kind[first] != Span::Fenced {
                continue;
            }
            let mut posts: Vec<Vec2> = (first..=k).map(post).collect();
            // Where the line runs on into a road, it ends on the roadside
            // fence, at a corner post.
            if first > 0 && kind[first - 1] == Span::Road {
                let from = post(first);
                if let Some(corner) =
                    join(sides, from, from - along * (3.0 * Self::POST_SPACING), site)
                {
                    posts.insert(0, corner);
                }
            }
            if k < spans && kind[k] == Span::Road {
                let from = post(k);
                if let Some(corner) =
                    join(sides, from, from + along * (3.0 * Self::POST_SPACING), site)
                {
                    posts.push(corner);
                }
            }
            self.runs.push(FenceRun {
                posts: densify(&posts),
                roadside: false,
            });
        }
    }

    /// Cuts one roadside fence into field frontages at its corners, with one
    /// or two open gates in each, and lays the fence between them.
    fn cut_frontages(&mut self, side: &mut Roadside) {
        let total = side.length();
        side.corners.sort_by(f32::total_cmp);
        let mut bounds = vec![0.0];
        bounds.extend(
            side.corners
                .iter()
                .copied()
                .filter(|&c| c > 1.0 && c < total - 1.0),
        );
        bounds.push(total);
        let mut fenced: Vec<(f32, f32)> = Vec::new();
        for w in bounds.windows(2) {
            let (from, to) = (w[0], w[1]);
            if to - from < 0.5 {
                continue;
            }
            let id = self.frontages.len() as u32;
            // Along a descampado the road runs unfenced.
            let has_fence = !self.is_descampado(side.at((from + to) / 2.0).0);
            self.frontages.push(Frontage {
                start: side.at(from).0,
                end: side.at(to).0,
                length: to - from,
                fenced: has_fence,
            });
            if !has_fence {
                continue;
            }
            let mut rng = Rng::new(
                FENCE_SEED ^ (side.at(from).0.x as i32 as u32).wrapping_mul(0x9e37_79b9),
                (side.at(from).0.y as i32 as u32) ^ id,
            );
            let mut cuts: Vec<(f32, f32, bool, bool)> = Vec::new();
            if to - from >= Self::MIN_FRONTAGE {
                let two = to - from > 300.0 && rng.unit() < 0.5;
                let count = if two { 2 } else { 1 };
                for g in 0..count {
                    let double = rng.unit() < 0.4;
                    let width = if double {
                        Self::GATE_WIDTH.1
                    } else {
                        Self::GATE_WIDTH.0
                    };
                    // Each gate in its own share of the frontage.
                    let share = (to - from) / count as f32;
                    let lo = from + share * g as f32 + Self::GATE_CORNER;
                    let hi = from + share * (g + 1) as f32 - Self::GATE_CORNER - width;
                    if hi <= lo {
                        continue;
                    }
                    let start = rng.range(lo, hi);
                    let open = rng.unit() < Self::OPEN_GATES;
                    cuts.push((start, start + width, double, open));
                }
                // Every field keeps at least one way in from the road.
                if !cuts.iter().any(|c| c.3)
                    && let Some(first) = cuts.first_mut()
                {
                    first.3 = true;
                }
            }
            let mut at = from;
            for (g0, g1, double, open) in cuts {
                fenced.push((at, g0));
                let (center, along) = side.at((g0 + g1) / 2.0);
                self.gates.push(Gate {
                    at: center,
                    along,
                    width: g1 - g0,
                    double,
                    open,
                    frontage: id,
                });
                at = g1;
            }
            fenced.push((at, to));
        }
        // Corners join the stretches on either side of them into one run.
        let mut run: Vec<(f32, f32)> = Vec::new();
        for (from, to) in fenced {
            if let Some(last) = run.last()
                && (last.1 - from).abs() > 1e-3
            {
                self.push_roadside(side, &run);
                run.clear();
            }
            run.push((from, to));
        }
        self.push_roadside(side, &run);
    }

    fn push_roadside(&mut self, side: &Roadside, stretches: &[(f32, f32)]) {
        let mut posts: Vec<Vec2> = Vec::new();
        for &(from, to) in stretches {
            for p in side.posts(from, to) {
                // A post right beside the last one is the same post.
                if posts.last().is_none_or(|l: &Vec2| l.distance(p) > 0.5) {
                    posts.push(p);
                }
            }
        }
        if posts.len() > 1 {
            self.runs.push(FenceRun {
                posts,
                roadside: true,
            });
        }
    }

    /// Cuts seeded openings into every fenced stretch of a grid line whose
    /// spans are `kind`, posts from `start` along `along`.
    fn cut_openings(&mut self, kind: &mut [Span], start: Vec2, along: Vec2) {
        let post = |k: usize| start + along * (k as f32 * Self::POST_SPACING);
        let spacing = Self::POST_SPACING;
        let mut k = 0;
        while k < kind.len() {
            if kind[k] != Span::Fenced {
                k += 1;
                continue;
            }
            let first = k;
            while k < kind.len() && kind[k] == Span::Fenced {
                k += 1;
            }
            let id = self.stretches.len() as u32;
            let length = (k - first) as f32 * spacing;
            self.stretches.push((id, length));
            let corner = post(first);
            let seed = (corner.x as i32 as u32).wrapping_mul(0x9e37_79b9)
                ^ (corner.y as i32 as u32).rotate_left(16)
                ^ FENCE_SEED;
            let mut rng = Rng::new(seed, id);
            let mut at = rng.range(Self::FIRST_OPENING.0, Self::FIRST_OPENING.1);
            loop {
                let wide = if rng.unit() < 0.5 { 1 } else { 2 };
                let span = first + (at / spacing) as usize;
                if span + wide > k {
                    break;
                }
                // Not where a crossing fence line meets this one.
                let center = (post(span) + post(span + wide)) * 0.5;
                if self.near_crossing_line(center, along, wide as f32 * spacing / 2.0 + spacing) {
                    at += 2.0 * spacing;
                    continue;
                }
                let opening = if rng.unit() < 0.5 {
                    OpeningKind::FieldGate {
                        double: rng.unit() < 0.5,
                        open: rng.unit() < Self::OPEN_GATES,
                    }
                } else {
                    OpeningKind::Broken
                };
                for s in &mut kind[span..span + wide] {
                    *s = Span::Opening;
                }
                self.openings.push(Opening {
                    at: center,
                    along,
                    width: wide as f32 * spacing,
                    kind: opening,
                    stretch: id,
                });
                at += rng.range(Self::OPENING_SPACING.0, Self::OPENING_SPACING.1);
            }
        }
    }

    /// Whether a surveyed line square to `along` passes within `reach` of
    /// `p`.
    fn near_crossing_line(&self, p: Vec2, along: Vec2, reach: f32) -> bool {
        let (lines, v) = if along.x.abs() > along.y.abs() {
            (&self.surveyed.0, p.x)
        } else {
            (&self.surveyed.1, p.y)
        };
        lines.iter().any(|l| (l - v).abs() < reach)
    }

    /// Thin boxes along every span of every run and across every closed
    /// gate: the fences' static obstacles.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let line = |at: Vec2, along: Vec2, width: f32| {
            (at - along * (width / 2.0), at + along * (width / 2.0))
        };
        let closed_road = self
            .gates
            .iter()
            .filter(|g| !g.open)
            .map(|g| line(g.at, g.along, g.width));
        let closed_grid = self.openings.iter().filter_map(|o| match o.kind {
            OpeningKind::FieldGate { open: false, .. } => Some(line(o.at, o.along, o.width)),
            _ => None,
        });
        self.runs
            .iter()
            .flat_map(FenceRun::spans)
            .chain(closed_road)
            .chain(closed_grid)
            .flat_map(span_boxes)
            .collect()
    }

    /// Makes sure every paddock has a way in: an open gate, a broken span
    /// or an unfenced side. Where one has none, a closed gate on its edge
    /// is opened, or else a span of its longest side is left broken.
    fn keep_fields_reachable(&mut self, site: &Site) {
        let (xs, ys) = self.surveyed.clone();
        for i in 0..xs.len().saturating_sub(1) {
            for j in 0..ys.len().saturating_sub(1) {
                let cell = Aabb::new(Vec2::new(xs[i], ys[j]), Vec2::new(xs[i + 1], ys[j + 1]));
                if self.reachable(cell, site) {
                    continue;
                }
                if let Some(o) = self.openings.iter_mut().find(|o| {
                    matches!(o.kind, OpeningKind::FieldGate { open: false, .. })
                        && on_edge(cell, o.at)
                }) {
                    if let OpeningKind::FieldGate { open, .. } = &mut o.kind {
                        *open = true;
                    }
                    continue;
                }
                self.break_edge(cell, site);
            }
        }
    }

    /// Whether `cell` has a way in.
    fn reachable(&self, cell: Aabb, site: &Site) -> bool {
        let passable = self.openings.iter().any(|o| {
            !matches!(o.kind, OpeningKind::FieldGate { open: false, .. }) && on_edge(cell, o.at)
        });
        let gate = self
            .gates
            .iter()
            .any(|g| g.open && cell.contains_point(g.at));
        if passable || gate {
            return true;
        }
        // An unfenced stretch of an edge, off the roads.
        let near_road = |p: Vec2| {
            site.terrain
                .roads
                .as_ref()
                .is_some_and(|r| r.near(p, Self::ROADSIDE + 1.0).is_some())
        };
        let edges = [
            (cell.min, Vec2::new(cell.max.x, cell.min.y)),
            (Vec2::new(cell.max.x, cell.min.y), cell.max),
            (cell.max, Vec2::new(cell.min.x, cell.max.y)),
            (Vec2::new(cell.min.x, cell.max.y), cell.min),
        ];
        edges.iter().any(|&(a, b)| {
            let length = a.distance(b);
            let mut free = 0;
            let mut k = 2.0;
            while k < length - 2.0 {
                let p = a + (b - a) * (k / length);
                let fenced = self.fenced_at(p);
                if !fenced && !near_road(p) {
                    free += 1;
                    if free >= 4 {
                        return true;
                    }
                } else {
                    free = 0;
                }
                k += 1.0;
            }
            false
        })
    }

    /// Whether a grid run passes through `p` (on a survey line), or a
    /// closed field gate shuts the gap there.
    fn fenced_at(&self, p: Vec2) -> bool {
        let run = self.runs.iter().filter(|r| !r.roadside).any(|r| {
            let (a, b) = (r.start(), r.end());
            let lo = Vec2::new(a.x.min(b.x) - 0.5, a.y.min(b.y) - 0.5);
            let hi = Vec2::new(a.x.max(b.x) + 0.5, a.y.max(b.y) + 0.5);
            p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
        });
        run || self.openings.iter().any(|o| {
            let rel = p - o.at;
            matches!(o.kind, OpeningKind::FieldGate { open: false, .. })
                && rel.dot(o.along).abs() <= o.width / 2.0 + 0.5
                && (rel.x * o.along.y - rel.y * o.along.x).abs() <= 0.5
        })
    }

    /// Leaves one span broken on the longest edge of `cell` that has a
    /// whole span well away from its corners and the roads.
    fn break_edge(&mut self, cell: Aabb, site: &Site) {
        let middle = cell.center();
        let off_road = |p: Vec2| {
            site.terrain
                .roads
                .as_ref()
                .is_none_or(|r| r.near(p, Self::ROADSIDE + 10.0).is_none())
        };
        let mut best: Option<(f32, usize, usize)> = None;
        for (r, run) in self.runs.iter().enumerate().filter(|(_, r)| !r.roadside) {
            for (k, (a, b)) in run.spans().enumerate() {
                let mid = (a + b) * 0.5;
                let whole = (a.distance(b) - Self::POST_SPACING).abs() < 1e-2;
                let corners = [
                    cell.min,
                    cell.max,
                    Vec2::new(cell.min.x, cell.max.y),
                    Vec2::new(cell.max.x, cell.min.y),
                ];
                let off_corners = corners.iter().all(|c| c.distance(mid) >= BREAK_CORNER);
                if whole && on_edge(cell, a) && on_edge(cell, b) && off_road(mid) && off_corners {
                    let d = mid.distance(middle);
                    if best.is_none_or(|(bd, _, _)| d < bd) {
                        best = Some((d, r, k));
                    }
                }
            }
        }
        let Some((_, r, k)) = best else {
            return;
        };
        let run = self.runs[r].clone();
        let (a, b) = (run.posts[k], run.posts[k + 1]);
        self.runs[r].posts.truncate(k + 1);
        let rest: Vec<Vec2> = run.posts[k + 1..].to_vec();
        if rest.len() > 1 {
            self.runs.push(FenceRun {
                posts: rest,
                roadside: false,
            });
        }
        if self.runs[r].posts.len() < 2 {
            self.runs.swap_remove(r);
        }
        self.openings.push(Opening {
            at: (a + b) * 0.5,
            along: (b - a) * (1.0 / a.distance(b)),
            width: a.distance(b),
            kind: OpeningKind::Broken,
            stretch: u32::MAX,
        });
    }

    /// The parts of every run inside `area` (a terrain chunk): whole spans
    /// whose middle lies in it, so neighbouring chunks share none.
    pub fn runs_in(&self, area: Aabb) -> Vec<FenceRun> {
        let mut parts = Vec::new();
        let reach = area.expanded(Self::POST_SPACING);
        for run in &self.runs {
            if !run.posts.iter().any(|p| reach.contains_point(*p))
                && reach.segment_overlap(run.start(), run.end()).is_none()
            {
                continue;
            }
            let mut current: Vec<Vec2> = Vec::new();
            for (a, b) in run.spans() {
                if area_holds(area, (a + b) * 0.5) {
                    if current.is_empty() {
                        current.push(a);
                    }
                    current.push(b);
                } else if !current.is_empty() {
                    parts.push(FenceRun {
                        posts: std::mem::take(&mut current),
                        roadside: run.roadside,
                    });
                }
            }
            if !current.is_empty() {
                parts.push(FenceRun {
                    posts: current,
                    roadside: run.roadside,
                });
            }
        }
        parts
    }

    /// Distance from `p` to the nearest surveyed grid line, fenced there or
    /// not, in meters.
    pub fn line_distance(&self, p: Vec2) -> f32 {
        let nearest = |lines: &[f32], v: f32| {
            let i = lines.partition_point(|&l| l < v);
            [i.checked_sub(1), Some(i)]
                .into_iter()
                .flatten()
                .filter_map(|i| lines.get(i))
                .map(|l| (l - v).abs())
                .fold(f32::INFINITY, f32::min)
        };
        nearest(&self.surveyed.0, p.x).min(nearest(&self.surveyed.1, p.y))
    }
}

impl FenceRun {
    /// The spans between consecutive posts, in order.
    pub fn spans(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.posts.windows(2).map(|w| (w[0], w[1]))
    }

    /// The first post.
    pub fn start(&self) -> Vec2 {
        self.posts[0]
    }

    /// The last post.
    pub fn end(&self) -> Vec2 {
        self.posts[self.posts.len() - 1]
    }
}

/// What the fences keep out of.
struct Site<'a> {
    terrain: &'a Terrain,
    rails: &'a [(Vec2, Vec2)],
    bands: &'a [Aabb],
    lo: Vec2,
    hi: Vec2,
}

/// The fences along both sides of every road, traced
/// [`FenceLayout::ROADSIDE`] out from the centerline, and broken off where
/// they would stand on a town's band, by the railway (so at every level
/// crossing), off the world or near another road.
fn roadsides(roads: &RoadNetwork, site: &Site) -> Vec<Roadside> {
    let mut sides = Vec::new();
    let near_rail = |p: Vec2| {
        site.rails
            .iter()
            .any(|&(a, b)| crate::road::segment_distance(p, a, b) < FenceLayout::RAIL_MARGIN)
    };
    for road in &roads.roads {
        let traced = trace(&road.path, FenceLayout::TRACE);
        for side in [-1.0_f32, 1.0] {
            let mut line: Vec<Vec2> = Vec::new();
            let mut flush = |line: &mut Vec<Vec2>| {
                if line.len() > 2 {
                    sides.push(Roadside::new(std::mem::take(line)));
                }
                line.clear();
            };
            for &(p, normal) in &traced {
                let q = p + normal * (side * FenceLayout::ROADSIDE);
                let ok = q.x >= site.lo.x
                    && q.x <= site.hi.x
                    && q.y >= site.lo.y
                    && q.y <= site.hi.y
                    && !site.bands.iter().any(|b| b.contains_point(q))
                    && !near_rail(q)
                    && roads.near(q, FenceLayout::ROADSIDE - 0.05).is_none();
                if ok {
                    line.push(q);
                } else {
                    flush(&mut line);
                }
            }
            flush(&mut line);
        }
    }
    sides
}

impl Roadside {
    fn new(line: Vec<Vec2>) -> Self {
        let mut arcs = Vec::with_capacity(line.len());
        let mut total = 0.0;
        for (i, p) in line.iter().enumerate() {
            if i > 0 {
                total += line[i - 1].distance(*p);
            }
            arcs.push(total);
        }
        Self {
            line,
            arcs,
            corners: Vec::new(),
        }
    }
}

/// Points along `path` about `step` meters apart, with the path's left unit
/// normal at each (mitered at bends).
fn trace(path: &[Vec2], step: f32) -> Vec<(Vec2, Vec2)> {
    let mut out = Vec::new();
    let unit = |v: Vec2| v * (1.0 / v.length().max(1e-9));
    for (i, w) in path.windows(2).enumerate() {
        let d = unit(w[1] - w[0]);
        let left = Vec2::new(-d.y, d.x);
        let length = w[0].distance(w[1]);
        let n = (length / step).ceil().max(1.0) as usize;
        for k in 0..n {
            let normal = if k == 0 && i > 0 {
                let before = unit(path[i] - path[i - 1]);
                unit(Vec2::new(-before.y, before.x) + left)
            } else {
                left
            };
            out.push((w[0] + d * (length * k as f32 / n as f32), normal));
        }
    }
    if let (Some(&last), Some(&(_, normal))) = (path.last(), out.last()) {
        out.push((last, normal));
    }
    out
}

/// Where the grid line from `from` toward `to` first meets a roadside
/// fence: that point, recorded as a corner of the fence it meets.
fn join(sides: &mut [Roadside], from: Vec2, to: Vec2, site: &Site) -> Option<Vec2> {
    let mut best: Option<(f32, usize, f32)> = None;
    for (n, side) in sides.iter().enumerate() {
        for (i, w) in side.line.windows(2).enumerate() {
            if let Some((t, u)) = crate::road::intersect(from, to, w[0], w[1])
                && best.is_none_or(|(bt, _, _)| t < bt)
            {
                let arc = side.arcs[i] + (side.arcs[i + 1] - side.arcs[i]) * u;
                best = Some((t, n, arc));
            }
        }
    }
    let (t, n, arc) = best?;
    let corner = from + (to - from) * t;
    // Only the near side's fence: never across the road to the far one.
    let clear = site.terrain.roads.as_ref().is_none_or(|roads| {
        (0..=10).all(|k| {
            let p = from + (corner - from) * (k as f32 / 10.0);
            roads.near(p, FenceLayout::ROADSIDE - 0.05).is_none()
        })
    });
    if !clear {
        return None;
    }
    sides[n].corners.push(arc);
    Some(corner)
}

/// `posts` with posts added wherever two are farther apart than
/// [`FenceLayout::POST_SPACING`].
fn densify(posts: &[Vec2]) -> Vec<Vec2> {
    let mut out = vec![posts[0]];
    for w in posts.windows(2) {
        let n = (w[0].distance(w[1]) / FenceLayout::POST_SPACING - 1e-3)
            .ceil()
            .max(1.0) as usize;
        for k in 1..=n {
            out.push(w[0] + (w[1] - w[0]) * (k as f32 / n as f32));
        }
    }
    out
}

/// Whether `p` lies on the edge of `cell` (within a few centimeters).
fn on_edge(cell: Aabb, p: Vec2) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() < 0.05;
    let within = |v: f32, lo: f32, hi: f32| v >= lo - 0.05 && v <= hi + 0.05;
    ((near(p.x, cell.min.x) || near(p.x, cell.max.x)) && within(p.y, cell.min.y, cell.max.y))
        || ((near(p.y, cell.min.y) || near(p.y, cell.max.y)) && within(p.x, cell.min.x, cell.max.x))
}

/// Whether `p` is inside `area`, its west and south edges in and its east
/// and north edges out, so a point on a shared edge has one chunk.
fn area_holds(area: Aabb, p: Vec2) -> bool {
    p.x >= area.min.x && p.x < area.max.x && p.y >= area.min.y && p.y < area.max.y
}

/// Thin boxes along one span: one for an axis-aligned span, short ones
/// along a slanted span so none bulges more than a few centimeters off it.
fn span_boxes((a, b): (Vec2, Vec2)) -> Vec<Aabb> {
    let t = FenceLayout::THICKNESS / 2.0;
    let d = b - a;
    let slant = d.x.abs().min(d.y.abs());
    let pieces = (slant / SLANT_BULGE).ceil().max(1.0) as usize;
    (0..pieces)
        .map(|k| {
            let (p, q) = (
                a + d * (k as f32 / pieces as f32),
                a + d * ((k + 1) as f32 / pieces as f32),
            );
            Aabb::new(
                Vec2::new(p.x.min(q.x) - t, p.y.min(q.y) - t),
                Vec2::new(p.x.max(q.x) + t, p.y.max(q.y) + t),
            )
        })
        .collect()
}

/// Least distance from a span broken to open a sealed paddock to the
/// paddock's corners, in meters.
const BREAK_CORNER: f32 = 30.0;

/// Most a box of a slanted span reaches across it, in meters.
const SLANT_BULGE: f32 = 0.3;

/// Least distance between segments `a`–`b` and `c`–`d`.
fn segments_distance(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f32 {
    if crate::road::intersect(a, b, c, d).is_some() {
        return 0.0;
    }
    use crate::road::segment_distance as sd;
    sd(a, c, d)
        .min(sd(b, c, d))
        .min(sd(c, a, b))
        .min(sd(d, a, b))
}

/// Mixed into the terrain seed for the survey.
const FENCE_SEED: u32 = 0xa1a3_b4ad;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Circle, FloraSite, Heading, Rider, RiderInput, RiderParams, World};

    fn config() -> WorldConfig {
        WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        }
    }

    /// The shipped towns with roads laid, their fences, and the world.
    pub(crate) fn fenced() -> (World, Vec<TownLayout>, RailLine, FenceLayout) {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let mut world = World::with_towns(config(), Terrain::default(), &towns);
        world.lay_roads(roads);
        let started = std::time::Instant::now();
        let fences = FenceLayout::build(config(), &world.terrain, &towns, &line);
        let built = started.elapsed();
        let started = std::time::Instant::now();
        let count = world.fence(&fences);
        let indexed = started.elapsed();
        println!(
            "fences: {} runs, {} gates, {count} obstacles; laid in {built:?}, indexed in {indexed:?}",
            fences.runs.len(),
            fences.gates.len(),
        );
        // Generous bounds (about 0.1 s each in a dev build): laying and
        // indexing the fences stay cheap at startup.
        assert!(built.as_secs_f32() < 10.0, "laid in {built:?}");
        assert!(indexed.as_secs_f32() < 10.0, "indexed in {indexed:?}");
        (world, towns, line, fences)
    }

    /// One 400 m paddock fenced all round, its south side broken off around
    /// `gaps` (as `(x, width)`), each a closed field gate; then the
    /// reachability pass.
    fn sealed_paddock(gaps: &[(f32, f32)]) -> FenceLayout {
        let side = 400.0;
        let posts = |a: Vec2, b: Vec2| -> Vec<Vec2> {
            let n = (a.distance(b) / FenceLayout::POST_SPACING).round() as usize;
            (0..=n)
                .map(|k| a + (b - a) * (k as f32 / n as f32))
                .collect()
        };
        let run = |posts: Vec<Vec2>| FenceRun {
            posts,
            roadside: false,
        };
        let mut layout = FenceLayout {
            surveyed: (vec![0.0, side], vec![0.0, side]),
            ..FenceLayout::default()
        };
        let (c0, c1, c2, c3) = (
            Vec2::new(0.0, 0.0),
            Vec2::new(side, 0.0),
            Vec2::new(side, side),
            Vec2::new(0.0, side),
        );
        layout.runs.push(run(posts(c1, c2)));
        layout.runs.push(run(posts(c2, c3)));
        layout.runs.push(run(posts(c3, c0)));
        // The south side, in pieces between the gates.
        let mut from = 0.0;
        for &(x, width) in gaps {
            layout.runs.push(run(posts(
                Vec2::new(from, 0.0),
                Vec2::new(x - width / 2.0, 0.0),
            )));
            layout.openings.push(Opening {
                at: Vec2::new(x, 0.0),
                along: Vec2::new(1.0, 0.0),
                width,
                kind: OpeningKind::FieldGate {
                    double: width > 6.0,
                    open: false,
                },
                stretch: 0,
            });
            from = x + width / 2.0;
        }
        layout.runs.push(run(posts(Vec2::new(from, 0.0), c1)));
        let terrain = Terrain::default();
        let site = Site {
            terrain: &terrain,
            rails: &[],
            bands: &[],
            lo: Vec2::new(-10.0, -10.0),
            hi: Vec2::new(side + 10.0, side + 10.0),
        };
        layout.keep_fields_reachable(&site);
        layout
    }

    /// The passable openings on the paddock's edge.
    fn ways_in(layout: &FenceLayout) -> usize {
        let cell = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(400.0, 400.0));
        layout
            .openings
            .iter()
            .filter(|o| !matches!(o.kind, OpeningKind::FieldGate { open: false, .. }))
            .filter(|o| on_edge(cell, o.at))
            .count()
    }

    #[test]
    fn a_paddock_ringed_only_by_closed_gates_gets_a_way_in() {
        let layout = sealed_paddock(&[(120.0, 8.0), (300.0, 4.5)]);
        assert!(ways_in(&layout) >= 1, "{:?}", layout.openings);
    }

    #[test]
    fn a_paddock_with_no_opening_at_all_gets_a_broken_span() {
        let layout = sealed_paddock(&[]);
        assert_eq!(ways_in(&layout), 1, "{:?}", layout.openings);
        let broken = layout.openings[0];
        assert_eq!(broken.kind, OpeningKind::Broken);
        // A whole span, well away from the corners, and no longer fenced.
        assert!((broken.width - FenceLayout::POST_SPACING).abs() < 1e-3);
        let corners = [0.0, 400.0];
        assert!(
            corners
                .iter()
                .flat_map(|&x| corners.iter().map(move |&y| Vec2::new(x, y)))
                .all(|c| c.distance(broken.at) >= 30.0)
        );
        let spans: Vec<(Vec2, Vec2)> = layout.runs.iter().flat_map(FenceRun::spans).collect();
        assert!(
            spans.iter().all(|&(a, b)| (a + b) * 0.5 != broken.at),
            "the broken span is still strung"
        );
        assert_eq!(spans.len(), 4 * 40 - 1);
    }

    #[test]
    fn the_land_is_surveyed_into_large_fields_the_same_way_every_time() {
        let (world, towns, line, fences) = fenced();
        assert_eq!(
            fences,
            FenceLayout::build(config(), &world.terrain, &towns, &line)
        );
        for lines in [&fences.surveyed.0, &fences.surveyed.1] {
            assert!(lines.len() > 15, "{}", lines.len());
            for w in lines.windows(2) {
                let field = w[1] - w[0];
                assert!((300.0..=800.0).contains(&field), "{field}");
            }
        }
        for run in &fences.runs {
            let (start, end) = (run.start(), run.end());
            assert!(
                run.roadside || start.x == end.x || start.y == end.y,
                "{run:?}"
            );
            for (a, b) in run.spans() {
                assert!(a.distance(b) <= FenceLayout::POST_SPACING + 1e-2);
                assert!(a.distance(b) > 0.1);
            }
        }
        let other = Terrain {
            seed: 11,
            ..world.terrain.clone()
        };
        assert_ne!(
            FenceLayout::build(config(), &other, &towns, &line).surveyed,
            fences.surveyed
        );
    }

    #[test]
    fn fences_keep_out_of_towns_the_rail_corridor_and_the_roads() {
        let (world, towns, line, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        let mut spans = 0;
        for run in &fences.runs {
            for (a, b) in run.spans() {
                for p in [a, (a + b) * 0.5, b] {
                    for town in &towns {
                        assert!(!town.leveled().contains_point(p), "{p:?} in {}", town.name);
                    }
                    let rail = rails
                        .iter()
                        .map(|&(r0, r1)| crate::road::segment_distance(p, r0, r1))
                        .fold(f32::INFINITY, f32::min);
                    assert!(rail >= FenceLayout::RAIL_MARGIN, "{p:?}: {rail}");
                    assert!(
                        roads.distance(p) > crate::RoadNetwork::HALF_WIDTH + 1.0,
                        "{p:?}"
                    );
                }
                spans += 1;
            }
        }
        assert!(spans > 20_000, "{spans}");
    }

    /// Points along every road every `step` meters with the road's left
    /// unit normal there.
    fn along_roads(roads: &crate::RoadNetwork, step: f32) -> Vec<(usize, Vec2, Vec2)> {
        let mut out = Vec::new();
        for (i, road) in roads.roads.iter().enumerate() {
            let mut next = 0.0;
            let mut along = 0.0;
            for w in road.path.windows(2) {
                let length = w[0].distance(w[1]);
                let d = (w[1] - w[0]) * (1.0 / length);
                while next <= along + length {
                    let p = w[0] + d * (next - along);
                    out.push((i, p, Vec2::new(-d.y, d.x)));
                    next += step;
                }
                along += length;
            }
        }
        out
    }

    #[test]
    fn no_fence_crosses_a_road_and_a_rider_rides_every_road_end_to_end() {
        let (world, _, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        for run in &fences.runs {
            for (a, b) in run.spans() {
                for k in 0..=4 {
                    let p = a + (b - a) * (k as f32 / 4.0);
                    // A public calle: at least 5 m from the road's edge.
                    let gap = roads.distance(p) - crate::RoadNetwork::HALF_WIDTH;
                    assert!(gap >= 5.0 - 1e-3, "{p:?}: {gap}");
                }
                for (r0, r1) in roads.segments() {
                    assert!(
                        crate::road::intersect(a, b, r0, r1).is_none(),
                        "{a:?}–{b:?}"
                    );
                }
            }
        }
        let c = Circle::new(Rider::COLLIDER_RADIUS);
        for (_, p, _) in along_roads(&roads, 1.0) {
            assert_eq!(world.resolve(c, p, |_| ()), p, "blocked at {p:?}");
        }
        // Signposts and crossing signs stand clear of every fence.
        for post in roads.obstacles() {
            let c = post.center();
            for run in &fences.runs {
                for (a, b) in run.spans() {
                    assert!(crate::road::segment_distance(c, a, b) > 1.0, "{c:?}");
                }
            }
        }
    }

    #[test]
    fn roads_run_between_fences_on_both_sides_except_by_towns_and_the_rail() {
        let (world, towns, line, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        let sides: Vec<(Vec2, Vec2)> = fences
            .runs
            .iter()
            .filter(|r| r.roadside)
            .flat_map(|r| r.spans().collect::<Vec<_>>())
            .collect();
        let (mut checked, mut fenced) = (0, 0);
        for (_, p, normal) in along_roads(&roads, 20.0) {
            for side in [-1.0, 1.0] {
                let q = p + normal * (side * FenceLayout::ROADSIDE);
                let free = towns.iter().any(|t| {
                    t.leveled()
                        .expanded(FenceLayout::TOWN_MARGIN + 15.0)
                        .contains_point(q)
                }) || rails.iter().any(|&(a, b)| {
                    crate::road::segment_distance(q, a, b) < FenceLayout::RAIL_MARGIN + 15.0
                });
                if free || fences.is_descampado(q) {
                    continue;
                }
                checked += 1;
                let on_fence = sides
                    .iter()
                    .any(|&(a, b)| crate::road::segment_distance(q, a, b) < 0.6);
                let at_gate = fences
                    .gates
                    .iter()
                    .any(|g| g.at.distance(q) < g.width / 2.0 + 1.0);
                assert!(on_fence || at_gate, "no fence beside the road at {q:?}");
                fenced += 1;
            }
        }
        assert!(checked > 500, "{checked}");
        assert_eq!(fenced, checked);
    }

    #[test]
    fn every_field_on_a_road_has_an_open_gate_into_it() {
        let (world, _, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        assert!(fences.frontages.len() > 20, "{}", fences.frontages.len());
        for (id, frontage) in fences.frontages.iter().enumerate() {
            let gates = fences
                .gates
                .iter()
                .filter(|g| g.frontage == id as u32)
                .count();
            let open = fences
                .gates
                .iter()
                .filter(|g| g.frontage == id as u32 && g.open)
                .count();
            if !frontage.fenced {
                assert_eq!(gates, 0);
            } else if frontage.length >= FenceLayout::MIN_FRONTAGE {
                assert!((1..=2).contains(&gates), "{frontage:?}: {gates}");
                assert!(open >= 1, "{frontage:?}: every gate closed");
            }
        }
        let obstacles = fences.obstacles();
        for gate in fences.gates.iter().filter(|g| g.open) {
            // In the roadside fence, parallel to the road, open.
            let gap = roads.distance(gate.at);
            assert!((gap - FenceLayout::ROADSIDE).abs() < 0.5, "{gate:?}: {gap}");
            // Clear across its opening, but for the posts' own boxes at
            // its very ends.
            for k in 2..=8 {
                let p = gate.at + gate.along * (gate.width * (k as f32 / 10.0 - 0.5));
                assert!(
                    !obstacles.iter().any(|o| o.expanded(0.1).contains_point(p)),
                    "{gate:?}"
                );
            }
        }
        let doubles = fences.gates.iter().filter(|g| g.double).count();
        println!("road gates {}, double {doubles}", fences.gates.len());
    }

    #[test]
    fn a_rider_turns_off_the_road_through_a_gate_into_the_field() {
        let (world, _, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        for gate in fences.gates.iter().filter(|g| g.open).step_by(5).take(6) {
            // From the road's centerline square through the gate.
            let out = Vec2::new(-gate.along.y, gate.along.x);
            let out = if roads.distance(gate.at + out) > roads.distance(gate.at) {
                out
            } else {
                out * -1.0
            };
            let start = gate.at - out * FenceLayout::ROADSIDE;
            let mut rider = Rider::new(start, Heading::from_radians(out.y.atan2(out.x)), &world);
            let input = RiderInput {
                throttle: 1.0,
                ..Default::default()
            };
            for _ in 0..300 {
                rider.step(input, &RiderParams::default(), &world, 1.0 / 60.0);
            }
            let into = (rider.position - gate.at).dot(out);
            assert!(into > 3.0, "stopped at {:?} by {gate:?}", rider.position);
        }
    }

    #[test]
    fn a_rider_is_stopped_by_the_wire() {
        let (mut world, towns, line, fences) = fenced();
        let flora = FloraSite::new(world.config, &world.terrain, &line).with_fences(&fences);
        world.plant(&flora);
        let _ = towns;
        // A long fenced run along y, far from roads: ride at it from 15 m.
        let run = fences
            .runs
            .iter()
            .find(|r| {
                !r.roadside && r.start().x == r.end().x && r.start().distance(r.end()) > 200.0
            })
            .expect("a long north-south run");
        let mid = (run.start() + run.end()) * 0.5;
        let start = mid + Vec2::new(15.0, 0.0);
        let mut rider = Rider::new(start, Heading::from_radians(std::f32::consts::PI), &world);
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        for _ in 0..180 {
            rider.step(input, &RiderParams::default(), &world, 1.0 / 60.0);
        }
        assert!(
            rider.position.x > mid.x + FenceLayout::THICKNESS / 2.0,
            "through the fence: {:?}",
            rider.position
        );
        assert!(rider.position.x < mid.x + 2.0, "{:?}", rider.position);
    }

    #[test]
    fn every_long_stretch_has_openings_every_300_to_500_m_off_the_rail_and_towns() {
        let (world, towns, line, fences) = fenced();
        // A stretch: a fenced line between two gaps that are not openings.
        let mut by_stretch: std::collections::BTreeMap<u32, Vec<&Opening>> = Default::default();
        for o in fences.openings.iter().filter(|o| o.stretch != u32::MAX) {
            by_stretch.entry(o.stretch).or_default().push(o);
        }
        for (stretch, length) in &fences.stretches {
            let openings = by_stretch.get(stretch).map_or(0, Vec::len);
            if *length > 500.0 {
                assert!(openings >= 1, "stretch {stretch} of {length} m has none");
            }
        }
        for list in by_stretch.values() {
            for w in list.windows(2) {
                let gap = w[0].at.distance(w[1].at);
                // 300–500 m, nudged on up to 40 m where a crossing line is in the way.
                assert!((290.0..=550.0).contains(&gap), "{gap}");
            }
        }
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        for o in &fences.openings {
            for town in &towns {
                assert!(!town.leveled().contains_point(o.at));
            }
            let rail = rails
                .iter()
                .map(|&(a, b)| crate::road::segment_distance(o.at, a, b))
                .fold(f32::INFINITY, f32::min);
            assert!(rail >= FenceLayout::RAIL_MARGIN);
            assert!(o.width == 10.0 || o.width == 20.0, "{o:?}");
        }
        // About half field gates, half broken spans.
        let gates = fences
            .openings
            .iter()
            .filter(|o| matches!(o.kind, OpeningKind::FieldGate { .. }))
            .count();
        let n = fences.openings.len();
        println!("openings: {n}, field gates {gates}, broken {}", n - gates);
        assert!(n > 500, "{n}");
        assert!(gates * 3 > n && gates * 3 < 2 * n, "{gates} of {n}");
        let _ = world;
    }

    #[test]
    fn no_opening_span_is_an_obstacle_and_a_rider_rides_through() {
        let (world, _, _, fences) = fenced();
        let c = Circle::new(Rider::COLLIDER_RADIUS);
        let obstacles = fences.obstacles();
        let passable = |o: &&Opening| !matches!(o.kind, OpeningKind::FieldGate { open: false, .. });
        for o in fences.openings.iter().filter(passable) {
            let half = o.along * (o.width / 2.0 - 0.2);
            let gap = Aabb::new(
                Vec2::new(
                    (o.at - half).x.min((o.at + half).x),
                    (o.at - half).y.min((o.at + half).y),
                ),
                Vec2::new(
                    (o.at - half).x.max((o.at + half).x),
                    (o.at - half).y.max((o.at + half).y),
                ),
            );
            assert!(!obstacles.iter().any(|b| b.intersects(&gap)), "{o:?}");
        }
        // Ride square through one opening of each kind.
        for kind in [true, false] {
            let o = fences
                .openings
                .iter()
                .find(|o| match o.kind {
                    OpeningKind::FieldGate { open, .. } => kind && open,
                    OpeningKind::Broken => !kind,
                })
                .unwrap();
            let across = Vec2::new(-o.along.y, o.along.x);
            let start = o.at - across * 12.0;
            let heading = Heading::from_radians(across.y.atan2(across.x));
            let mut rider = Rider::new(start, heading, &world);
            let input = RiderInput {
                throttle: 1.0,
                ..Default::default()
            };
            for _ in 0..600 {
                rider.step(input, &RiderParams::default(), &world, 1.0 / 60.0);
            }
            let through = (rider.position - o.at).dot(across);
            assert!(through > 5.0, "stopped at {:?} by {o:?}", rider.position);
            let _ = c;
        }
    }

    #[test]
    fn about_four_in_ten_field_gates_are_closed_and_a_closed_gate_blocks() {
        let (world, _, _, fences) = fenced();
        let road_gates = fences.gates.iter().map(|g| g.open);
        let grid_gates = fences.openings.iter().filter_map(|o| match o.kind {
            OpeningKind::FieldGate { open, .. } => Some(open),
            OpeningKind::Broken => None,
        });
        let all: Vec<bool> = road_gates.chain(grid_gates).collect();
        let closed = all.iter().filter(|o| !**o).count();
        println!("field gates {}, closed {closed}", all.len());
        let share = closed as f32 / all.len() as f32;
        assert!((0.3..0.48).contains(&share), "{share}");
        // Ride square at a closed grid gate: stopped at the gate line.
        let o = fences
            .openings
            .iter()
            .find(|o| matches!(o.kind, OpeningKind::FieldGate { open: false, .. }))
            .unwrap();
        let across = Vec2::new(-o.along.y, o.along.x);
        let start = o.at - across * 12.0;
        let mut rider = Rider::new(
            start,
            Heading::from_radians(across.y.atan2(across.x)),
            &world,
        );
        let input = RiderInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..600 {
            rider.step(input, &RiderParams::default(), &world, 1.0 / 60.0);
        }
        let through = (rider.position - o.at).dot(across);
        assert!(through < 0.0, "through a closed gate: {:?}", rider.position);
    }

    #[test]
    fn about_one_paddock_in_ten_is_unfenced_and_the_same_every_time() {
        let (world, towns, line, fences) = fenced();
        let cells = fences.cells();
        let open = fences.descampados.len();
        let share = open as f32 / cells as f32;
        println!("descampados {open} of {cells}");
        assert!((0.07..0.13).contains(&share), "{share}");
        assert_eq!(
            fences.descampados,
            FenceLayout::build(config(), &world.terrain, &towns, &line).descampados
        );
        // No roadside fence runs along a descampado.
        for run in fences.runs.iter().filter(|r| r.roadside) {
            for (a, b) in run.spans() {
                assert!(!fences.is_descampado((a + b) * 0.5), "{a:?}");
            }
        }
    }

    /// Cells of a 1 m raster over `window` a rider can stand in, reached
    /// from `from` through the gaps in `obstacles`.
    pub(crate) fn reached(world: &World, window: Aabb, from: &[Vec2]) -> (Vec<bool>, usize) {
        let w = window.size().x as usize;
        let h = window.size().y as usize;
        let mut blocked = vec![false; w * h];
        let r = Rider::COLLIDER_RADIUS * 0.8;
        for o in world.obstacles.near(window) {
            let o = o.expanded(r);
            let x0 = ((o.min.x - window.min.x).floor().max(0.0)) as usize;
            let y0 = ((o.min.y - window.min.y).floor().max(0.0)) as usize;
            let x1 = ((o.max.x - window.min.x).ceil() as usize).min(w);
            let y1 = ((o.max.y - window.min.y).ceil() as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = window.min + Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    if o.contains_point(c) {
                        blocked[y * w + x] = true;
                    }
                }
            }
        }
        let mut seen = vec![false; w * h];
        let mut stack: Vec<usize> = from
            .iter()
            .filter(|p| window.contains_point(**p))
            .map(|p| {
                let q = *p - window.min;
                q.y as usize * w + q.x as usize
            })
            .filter(|&i| !blocked[i])
            .collect();
        for &i in &stack {
            seen[i] = true;
        }
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let mut go = |j: usize| {
                if !seen[j] && !blocked[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                go(i - 1);
            }
            if x + 1 < w {
                go(i + 1);
            }
            if y > 0 {
                go(i - w);
            }
            if y + 1 < h {
                go(i + w);
            }
        }
        (seen, w)
    }

    #[test]
    fn every_field_can_be_reached_from_the_road() {
        let (world, towns, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        // Around Monte Nievas and the first stretch of the road to Pico.
        let window = Aabb::new(Vec2::new(-4600.0, -3600.0), Vec2::new(-1000.0, -400.0));
        let from: Vec<Vec2> = along_roads(&roads, 5.0)
            .into_iter()
            .map(|(_, p, _)| p)
            .collect();
        let (seen, w) = reached(&world, window, &from);
        let at = |p: Vec2| {
            let q = p - window.min;
            seen[q.y as usize * w + q.x as usize]
        };
        let (xs, ys) = &fences.surveyed;
        let mut checked = 0;
        for i in 0..xs.len() - 1 {
            for j in 0..ys.len() - 1 {
                let cell = Aabb::new(Vec2::new(xs[i], ys[j]), Vec2::new(xs[i + 1], ys[j + 1]));
                if !window.contains(&cell) {
                    continue;
                }
                // Sample each field's middle, off roads, towns and the rail.
                let c = cell.center();
                let clear = roads.distance(c) > 30.0
                    && towns
                        .iter()
                        .all(|t| !t.leveled().expanded(30.0).contains_point(c))
                    && world
                        .obstacles
                        .near(Aabb::from_center_size(c, Vec2::new(4.0, 4.0)))
                        .is_empty();
                if !clear {
                    continue;
                }
                assert!(at(c), "the field around {c:?} cannot be reached");
                checked += 1;
            }
        }
        assert!(checked >= 10, "{checked}");
    }

    #[test]
    fn trees_keep_off_the_fence_lines() {
        let (world, _, line, fences) = fenced();
        let flora = FloraSite::new(world.config, &world.terrain, &line).with_fences(&fences);
        let trees = flora.all_trees();
        assert!(trees.len() > 5_000);
        for t in &trees {
            assert!(
                fences.line_distance(t.at) >= FloraSite::TREE_FENCE_MARGIN,
                "{t:?}"
            );
        }
    }
}
