//! A 1919 pampa plaza, dressed by its town's [`Paving`] from the
//! [`PlazaFurniture`] core lays out: paths with low curbs, a paved
//! crossing, a monument in the manner of the Pirámide de Mayo (a stepped
//! base, a pedestal under a cornice, an obelisk and a finial), slatted
//! benches on cast-iron ends, and a low railing round the edge.
//!
//! - **City** ([`Paving::Everywhere`], General Pico): brick paths with
//!   stucco curbs, a monument of about 8.8 m on three steps, three benches a
//!   side, and an iron railing over a stucco plinth between piers.
//! - **Town** ([`Paving::Plaza`], Castex): compacted earth paths with brick
//!   curbs, a monument of about 6.4 m on two steps, one bench by each path
//!   (one to a quadrant), no railing.
//! - **Village** ([`Paving::None`], Metileo): earth paths with no curbs, a
//!   small monument of about 4.5 m, two benches, no railing.
//!
//! Everything is built round the plaza's middle on the ground, `x` east,
//! `y` up and `z` south (the world's axes), into two [`Shape`]s: the
//! [`PlazaShapes::ground`] (paths, curbs, crossing and monument, drawn as
//! far as the buildings) and the [`PlazaShapes::furniture`] (benches and
//! railing, drawn as far as the small props). Each ends up as one mesh per
//! [`Surface`].

use bevy::prelude::*;
use pampa_core::town::{Bench, Paving, PlazaFurniture};
use pampa_core::{Aabb, PLAZA_PATH_WIDTH, Side, Vec2 as GroundVec2};

use super::italianate::{CHAMFER, urn};
use super::materials::Surface;
use super::shape::Shape;
use crate::coords::world_from_ground;

/// Top of the plaza's lawn over the ground, meters (the town's lawn patch).
pub const LAWN_TOP: f32 = 0.05;
/// Top of the paths and the crossing over the ground, meters: as before.
pub const PATH_TOP: f32 = 0.07;
/// Top of a path's curb over the ground, meters: 3 cm over the path.
pub const PATH_CURB_TOP: f32 = 0.10;
/// Width of a path's curb, on the lawn beside it, meters.
pub const PATH_CURB: f32 = 0.15;
/// How far paths and curbs reach under the ground, meters.
const FOOT: f32 = -0.05;

/// One plaza's meshes, round its middle on the ground.
#[derive(Debug, Default)]
pub(crate) struct PlazaShapes {
    /// Paths, curbs, crossing and monument.
    pub ground: Shape,
    /// Benches and railing.
    pub furniture: Shape,
}

/// The surfaces a plaza of `paving` wears for its paths and their curbs.
pub(crate) fn path_surfaces(paving: Paving) -> (Surface, Option<Surface>) {
    match paving {
        Paving::Everywhere => (Surface::Paving, Some(Surface::Stucco)),
        Paving::Plaza => (Surface::Earth, Some(Surface::Brick)),
        Paving::None => (Surface::Earth, None),
    }
}

/// Builds the plaza `f` lays out.
pub(crate) fn plaza(f: &PlazaFurniture) -> PlazaShapes {
    let mut s = PlazaShapes::default();
    let c = f.area.center();
    paths(&mut s.ground, f, c);
    monument(&mut s.ground, f.paving, PATH_TOP);
    for b in &f.benches {
        let at = world_from_ground(b.at - c, LAWN_TOP);
        let mut one = Shape::default();
        bench(&mut one);
        s.furniture.merge(
            one,
            Transform::from_translation(at).with_rotation(facing(b.facing)),
        );
    }
    railing(&mut s.furniture, f, c);
    s
}

/// The turn taking a piece's `+Z` (its front) to face `side`.
fn facing(side: Side) -> Quat {
    let n = pampa_core::places::side_normal(side);
    let d = world_from_ground(n, 0.0);
    // A turn about `y` only: a half turn must not flip it over.
    Quat::from_rotation_y(d.x.atan2(d.z))
}

/// The box over ground `area` from `low` to `high`, round ground `c`.
fn ground_block(
    shape: &mut Shape,
    surface: Surface,
    area: Aabb,
    c: GroundVec2,
    low: f32,
    high: f32,
) {
    let (p, q) = (
        world_from_ground(area.min - c, 0.0),
        world_from_ground(area.max - c, 0.0),
    );
    shape.block(surface, p.min(q).with_y(low), p.max(q).with_y(high));
}

/// The crossing, the four paths out of it to the plaza's edge, and their
/// curbs on the lawn side (none in a village).
fn paths(shape: &mut Shape, f: &PlazaFurniture, c: GroundVec2) {
    let (path, curb) = path_surfaces(f.paving);
    let (area, x) = (f.area, f.crossing);
    let h = PLAZA_PATH_WIDTH / 2.0;
    ground_block(shape, path, x, c, FOOT, PATH_TOP);
    let arms = [
        Aabb::new(
            GroundVec2::new(x.max.x, c.y - h),
            GroundVec2::new(area.max.x, c.y + h),
        ),
        Aabb::new(
            GroundVec2::new(area.min.x, c.y - h),
            GroundVec2::new(x.min.x, c.y + h),
        ),
        Aabb::new(
            GroundVec2::new(c.x - h, x.max.y),
            GroundVec2::new(c.x + h, area.max.y),
        ),
        Aabb::new(
            GroundVec2::new(c.x - h, area.min.y),
            GroundVec2::new(c.x + h, x.min.y),
        ),
    ];
    for arm in arms {
        ground_block(shape, path, arm, c, FOOT, PATH_TOP);
    }
    let Some(curb) = curb else {
        return;
    };
    let w = PATH_CURB;
    let mut curbs = Vec::new();
    // Along each arm, both sides, from the crossing's curb to the edge.
    for (lo, hi) in [(x.max.x + w, area.max.x), (area.min.x, x.min.x - w)] {
        curbs.push(Aabb::new(
            GroundVec2::new(lo, c.y + h),
            GroundVec2::new(hi, c.y + h + w),
        ));
        curbs.push(Aabb::new(
            GroundVec2::new(lo, c.y - h - w),
            GroundVec2::new(hi, c.y - h),
        ));
    }
    for (lo, hi) in [(x.max.y + w, area.max.y), (area.min.y, x.min.y - w)] {
        curbs.push(Aabb::new(
            GroundVec2::new(c.x + h, lo),
            GroundVec2::new(c.x + h + w, hi),
        ));
        curbs.push(Aabb::new(
            GroundVec2::new(c.x - h - w, lo),
            GroundVec2::new(c.x - h, hi),
        ));
    }
    // Round the crossing, between the arms, its corners included.
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let corner = c + GroundVec2::new(sx, sy) * (half(x) + w);
        let inner = c + GroundVec2::new(sx * h, sy * h);
        let edge = c + GroundVec2::new(sx, sy) * half(x);
        // The run along x (its north or south side), then along y.
        curbs.push(span(
            GroundVec2::new(inner.x, edge.y),
            GroundVec2::new(corner.x, corner.y),
        ));
        curbs.push(span(
            GroundVec2::new(edge.x, inner.y),
            GroundVec2::new(corner.x, edge.y),
        ));
    }
    for a in curbs {
        ground_block(shape, curb, a, c, FOOT, PATH_CURB_TOP);
    }
}

/// Half the side of a square `a`.
fn half(a: Aabb) -> f32 {
    a.size().x / 2.0
}

/// The box between two ground corners in any order.
fn span(a: GroundVec2, b: GroundVec2) -> Aabb {
    Aabb::new(
        GroundVec2::new(a.x.min(b.x), a.y.min(b.y)),
        GroundVec2::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

/// How a monument is proportioned, meters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Proportions {
    /// Each step's side and rise, from the lowest.
    pub steps: &'static [(f32, f32)],
    /// The pedestal's side and height.
    pub pedestal: (f32, f32),
    /// The cornice's side and depth over the pedestal.
    pub cornice: (f32, f32),
    /// The die (attic) over the cornice: side and height.
    pub die: (f32, f32),
    /// The obelisk's side at its foot and at its top, and its height.
    pub shaft: (f32, f32, f32),
    /// The pyramidion's height over the shaft.
    pub point: f32,
    /// Urns on the cornice's corners.
    pub urns: bool,
}

impl Proportions {
    /// The monument of a plaza of `paving`: grander as the town grows. Its
    /// lowest step is [`PlazaFurniture::monument_side`] wide.
    pub fn of(paving: Paving) -> Self {
        match paving {
            Paving::None => Self {
                steps: &[(2.4, 0.25)],
                pedestal: (1.5, 0.9),
                cornice: (1.7, 0.15),
                die: (1.0, 0.25),
                shaft: (0.7, 0.45, 2.2),
                point: 0.35,
                urns: false,
            },
            Paving::Plaza => Self {
                steps: &[(3.0, 0.25), (2.5, 0.25)],
                pedestal: (1.8, 1.3),
                cornice: (2.05, 0.2),
                die: (1.3, 0.4),
                shaft: (0.85, 0.55, 3.2),
                point: 0.45,
                urns: false,
            },
            Paving::Everywhere => Self {
                steps: &[(3.0, 0.25), (2.6, 0.25), (2.2, 0.25)],
                pedestal: (1.9, 1.7),
                cornice: (2.2, 0.25),
                die: (1.5, 0.5),
                shaft: (1.0, 0.6, 4.6),
                point: 0.6,
                urns: true,
            },
        }
    }

    /// Height of the finial's tip over the monument's foot.
    #[cfg(test)]
    pub fn height(&self) -> f32 {
        let steps: f32 = self.steps.iter().map(|s| s.1).sum();
        steps + self.pedestal.1 + self.cornice.1 + self.die.1 + self.shaft.2 + self.point + FINIAL
    }
}

/// Height of the monument's finial (a ball on a collar), meters.
const FINIAL: f32 = 0.35;

/// The monument for a plaza of `paving`, standing at `foot` over the
/// plaza's middle: stucco steps, a whitewashed pedestal on a moulded base
/// with a bronze plaque on each face, a stucco cornice, a die, a tapering
/// whitewashed obelisk with its pyramidion, and an iron finial.
pub(crate) fn monument(shape: &mut Shape, paving: Paving, foot: f32) {
    let p = Proportions::of(paving);
    let mut y = foot;
    let square = |side: f32, y0: f32, y1: f32| {
        let h = side / 2.0;
        (Vec3::new(-h, y0, -h), Vec3::new(h, y1, h))
    };
    for &(side, rise) in p.steps {
        let (lo, hi) = square(side, y - 0.05, y + rise);
        shape.chamfered(Surface::Stucco, lo, hi, 0.12);
        y += rise;
    }
    // Pedestal: a moulded base, the die of the pedestal, plaques.
    let (side, height) = p.pedestal;
    let (lo, hi) = square(side + 0.12, y, y + 0.18);
    shape.chamfered(Surface::Stucco, lo, hi, CHAMFER);
    let (lo, hi) = square(side, y, y + height);
    shape.chamfered(Surface::Whitewash, lo, hi, CHAMFER);
    if paving != Paving::None {
        let (pw, ph) = (side * 0.5, height * 0.4);
        for k in 0..4 {
            let turn = Quat::from_rotation_y(k as f32 * std::f32::consts::FRAC_PI_2);
            let at = turn * Vec3::new(0.0, y + height * 0.55, side / 2.0 + 0.01);
            shape.cuboid(
                Surface::Bronze,
                Transform::from_translation(at)
                    .with_rotation(turn)
                    .with_scale(Vec3::new(pw, ph, 0.03)),
                false,
            );
        }
    }
    y += height;
    // Cornice: a fascia under a wider drip.
    let (side, depth) = p.cornice;
    let (lo, hi) = square(side - 0.12, y, y + depth * 0.45);
    shape.chamfered(Surface::Stucco, lo, hi, CHAMFER);
    let (lo, hi) = square(side, y + depth * 0.45, y + depth);
    shape.chamfered(Surface::Stucco, lo, hi, CHAMFER);
    y += depth;
    if p.urns {
        let r = side / 2.0 - 0.2;
        for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            urn(shape, Surface::Stucco, Vec3::new(sx * r, y, sz * r));
        }
    }
    let (side, height) = p.die;
    let (lo, hi) = square(side, y, y + height);
    shape.chamfered(Surface::Whitewash, lo, hi, CHAMFER);
    y += height;
    let (lo, hi) = square(side + 0.1, y - 0.06, y);
    shape.block(Surface::Stucco, lo, hi);
    // The obelisk, its four faces leaning in, then its pyramidion.
    let (bottom, top, tall) = p.shaft;
    frustum(shape, Surface::Whitewash, y, bottom / 2.0, top / 2.0, tall);
    y += tall;
    super::italianate::spire(shape, Surface::Whitewash, Vec3::Y * y, top, p.point);
    y += p.point;
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(Vec3::Y * (y - 0.08)),
        &[
            (0.05, 0.0),
            (0.03, 0.1),
            (0.11, 0.17),
            (0.12, 0.24),
            (0.08, 0.33),
            (0.0, FINIAL + 0.08),
        ],
        8,
    );
}

/// A square frustum round the `y` axis from `y0`, `bottom` and `top` half
/// wide, `height` tall, with no caps.
fn frustum(shape: &mut Shape, surface: Surface, y0: f32, bottom: f32, top: f32, height: f32) {
    let ring = |h: f32, y: f32| {
        [
            Vec3::new(-h, y, h),
            Vec3::new(h, y, h),
            Vec3::new(h, y, -h),
            Vec3::new(-h, y, -h),
        ]
    };
    let (a, b) = (ring(bottom, y0), ring(top, y0 + height));
    for k in 0..4 {
        let n = (k + 1) % 4;
        shape.face(surface, &[a[k], a[n], b[n], b[k]]);
    }
}

/// Seat height of a bench, meters.
pub(crate) const SEAT: f32 = 0.44;

/// A slatted bench in its own space: [`Bench::LENGTH`] along `x`, its
/// front toward `+z`, within [`Bench::DEPTH`] front to back, on the ground
/// at `y = 0`. Two cast-iron ends (legs, seat bearer, back support and
/// armrest) carry four seat slats and three back slats.
pub(crate) fn bench(shape: &mut Shape) {
    let (half, d) = (Bench::LENGTH / 2.0, Bench::DEPTH / 2.0);
    let iron = 0.05;
    for x in [-(half - 0.12), half - 0.12] {
        let p = |y: f32, z: f32| Vec3::new(x, y, z);
        let beams = [
            (p(0.0, d - 0.06), p(SEAT, d - 0.12)),
            (p(0.0, -d + 0.04), p(SEAT, -d + 0.16)),
            (p(SEAT - 0.03, d - 0.08), p(SEAT - 0.03, -d + 0.12)),
            (p(SEAT, -d + 0.14), p(0.86, -d + 0.04)),
            (p(0.64, d - 0.08), p(0.62, -d + 0.1)),
            (p(SEAT, d - 0.1), p(0.64, d - 0.1)),
        ];
        for (a, b) in beams {
            shape.beam(Surface::Steel, a, b, iron);
        }
    }
    // Seat slats, front to back.
    let length = Bench::LENGTH - 0.04;
    for k in 0..4 {
        let z = d - 0.1 - k as f32 * 0.095;
        shape.block(
            Surface::Planks,
            Vec3::new(-length / 2.0, SEAT, z - 0.04),
            Vec3::new(length / 2.0, SEAT + 0.035, z + 0.04),
        );
    }
    // Back slats, leaning back with the iron support.
    let lean = Quat::from_rotation_x(-0.25);
    for k in 0..3 {
        let y = SEAT + 0.15 + k as f32 * 0.11;
        let z = -d + 0.12 - (y - SEAT) * 0.25;
        shape.cuboid(
            Surface::Planks,
            Transform::from_xyz(0.0, y, z)
                .with_rotation(lean)
                .with_scale(Vec3::new(length, 0.08, 0.03)),
            true,
        );
    }
}

/// Height of the railing's stucco plinth, meters.
pub(crate) const RAILING_PLINTH: f32 = 0.35;
/// Height of the railing's top rail over the lawn, meters.
pub(crate) const RAILING_TOP: f32 = 1.05;

/// The city's railing (only a city's plaza has one): iron pickets between
/// two rails over a stucco plinth along each of `f`'s pieces, with stucco
/// piers at the corners and the path mouths.
fn railing(shape: &mut Shape, f: &PlazaFurniture, c: GroundVec2) {
    if f.railing.is_empty() {
        return;
    }
    let top = LAWN_TOP + RAILING_TOP;
    let base = LAWN_TOP + RAILING_PLINTH;
    for piece in &f.railing {
        let size = piece.size();
        let along_x = size.x >= size.y;
        let (lo, hi) = (
            world_from_ground(piece.min - c, 0.0),
            world_from_ground(piece.max - c, 0.0),
        );
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let mid = (lo + hi) / 2.0;
        let length = if along_x { size.x } else { size.y };
        let dir = if along_x { Vec3::X } else { Vec3::Z };
        let start = mid - dir * (length / 2.0);
        let t = 0.16;
        let across = Vec3::ONE - dir;
        let (a, b) = (mid - across * (t / 2.0), mid + across * (t / 2.0));
        let (a, b) = (a - dir * (length / 2.0), b + dir * (length / 2.0));
        shape.block(Surface::Stucco, a.with_y(LAWN_TOP - 0.05), b.with_y(base));
        // Two rails, then pickets between them and past the top.
        for y in [base + 0.08, top - 0.08] {
            shape.beam(
                Surface::Steel,
                start.with_y(y),
                (start + dir * length).with_y(y),
                0.03,
            );
        }
        let count = (length / PICKET_PITCH).floor().max(1.0) as usize;
        for k in 1..count {
            let at = start + dir * (k as f32 * length / count as f32);
            shape.cuboid(
                Surface::Steel,
                Transform::from_translation(at.with_y((base + top) / 2.0 + 0.02))
                    .with_scale(Vec3::new(0.02, top - base + 0.04, 0.02)),
                false,
            );
        }
        // A stout post every few meters keeps the run upright.
        let posts = (length / 2.5).floor() as usize;
        for k in 1..posts {
            let at = start + dir * (k as f32 * length / posts as f32);
            shape.block(
                Surface::Steel,
                (at - Vec3::new(0.03, 0.0, 0.03)).with_y(base - 0.02),
                (at + Vec3::new(0.03, 0.0, 0.03)).with_y(top + 0.04),
            );
        }
    }
    // Piers at the corners and on both sides of each mouth, all within the
    // railing's collider.
    for p in stands(f, PIER) {
        let at = world_from_ground(p - c, 0.0);
        let w = PIER - 0.01;
        shape.chamfered(
            Surface::Stucco,
            (at - Vec3::new(w, 0.0, w)).with_y(LAWN_TOP - 0.05),
            (at + Vec3::new(w, 0.0, w)).with_y(top + 0.15),
            CHAMFER,
        );
        shape.block(
            Surface::Stucco,
            (at - Vec3::splat(PIER)).with_y(top + 0.15),
            (at + Vec3::splat(PIER)).with_y(top + 0.21),
        );
    }
}

/// Gap between the railing's pickets, meters.
const PICKET_PITCH: f32 = 0.25;

/// Half the side of a city railing's stucco pier (its cap), meters.
pub(crate) const PIER: f32 = 0.14;

/// Where the railing of `f` has its piers or stout posts, `half` wide
/// each way: the four corners, and both sides of each path's mouth, just
/// off the mouth.
pub(crate) fn stands(f: &PlazaFurniture, half: f32) -> Vec<GroundVec2> {
    let area = f.area;
    let inset = PlazaFurniture::RAILING_INSET + PlazaFurniture::RAILING_DEPTH / 2.0;
    let (lo, hi) = (
        area.min + GroundVec2::new(inset, inset),
        area.max - GroundVec2::new(inset, inset),
    );
    let mut out = vec![
        lo,
        hi,
        GroundVec2::new(lo.x, hi.y),
        GroundVec2::new(hi.x, lo.y),
    ];
    let (m, mouth) = (area.center(), PlazaFurniture::MOUTH_HALF + half);
    for s in [-1.0, 1.0] {
        let (x, y) = (m.x + s * mouth, m.y + s * mouth);
        out.extend([
            GroundVec2::new(x, lo.y),
            GroundVec2::new(x, hi.y),
            GroundVec2::new(lo.x, y),
            GroundVec2::new(hi.x, y),
        ]);
    }
    out
}

#[cfg(test)]
mod tests;
