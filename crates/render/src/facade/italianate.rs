//! The dressing of a 1919 Italianate pampa front, on any [`Wall`]: a
//! plinth broken at the doors, chamfered pilasters with bases and capitals,
//! a moulded cornice, a parapet with pedestals, balustrades, urns and a
//! pediment, framed and hooded openings with wooden doors, iron grilles, round
//! arches, and the galería over the sidewalk.
//!
//! Every piece stands at most [`PROUD`] meters out of the wall it dresses,
//! so a wall set that far in from a footprint keeps its whole front inside
//! it (the galería apart, which reaches over the sidewalk).

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;
use super::walls::{Opening, Wall, Walls, slab};

/// How far the dressing stands proud of a wall at most, meters.
pub const PROUD: f32 = 0.1;

/// The bevel on the vertical edges of piers and pilasters, meters.
pub const CHAMFER: f32 = 0.04;

/// The world box between two points given in wall terms (`along`,
/// `inward`, `y`).
fn wall_box(wall: &Wall, lo: (f32, f32, f32), hi: (f32, f32, f32)) -> (Vec3, Vec3) {
    let (p, q) = (wall.at(lo.0, lo.1, lo.2), wall.at(hi.0, hi.1, hi.2));
    (p.min(q), p.max(q))
}

/// The plinth (zócalo) along `wall`, `height` tall and `out` proud, broken
/// at its doors.
pub(crate) fn plinth(
    shape: &mut Shape,
    surface: Surface,
    wall: &Wall,
    openings: &[Opening],
    height: f32,
    out: f32,
) {
    let mut doors: Vec<(f32, f32)> = openings
        .iter()
        .filter(|o| o.bottom <= 0.01)
        .map(Opening::span)
        .collect();
    doors.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut from = 0.0;
    for (a, b) in doors.into_iter().chain([(wall.length, wall.length)]) {
        if a > from + 0.01 {
            let (min, max) = wall_box(wall, (from, -out, -wall.drop), (a, 0.02, height));
            shape.block(surface, min, max);
        }
        from = from.max(b);
    }
}

/// A chamfered pilaster on `wall`, `width` wide round `at` meters along it,
/// `out` proud, from `y0` to `y1`, with a base up to `base` and a capital.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pilaster(
    shape: &mut Shape,
    surface: Surface,
    wall: &Wall,
    at: f32,
    width: f32,
    out: f32,
    (y0, base, y1): (f32, f32, f32),
) {
    let half = width / 2.0;
    let (min, max) = wall_box(wall, (at - half, -out + 0.02, y0), (at + half, 0.02, y1));
    shape.chamfered(surface, min, max, CHAMFER);
    // The base and the capital stand a little wider and prouder.
    let (bout, bhalf) = (out.min(PROUD), half + 0.05);
    let (min, max) = wall_box(wall, (at - bhalf, -bout, y0), (at + bhalf, 0.02, base));
    shape.chamfered(surface, min, max, CHAMFER);
    let (min, max) = wall_box(wall, (at - bhalf, -bout, y1 - 0.2), (at + bhalf, 0.02, y1));
    shape.block(surface, min, max);
}

/// A square chamfered pier on each corner of `walls`, `size` wide, its
/// outer faces `out` proud of the walls, from `y0` to `y1`.
pub(crate) fn corner_piers(
    shape: &mut Shape,
    surface: Surface,
    walls: &Walls,
    size: f32,
    out: f32,
    (y0, y1): (f32, f32),
) {
    for (x, sx) in [(walls.x0 - out, 1.0), (walls.x1 + out, -1.0)] {
        for (z, sz) in [(walls.z0 - out, 1.0), (walls.z1 + out, -1.0)] {
            let a = Vec3::new(x, y0, z);
            let b = Vec3::new(x + sx * size, y1, z + sz * size);
            shape.chamfered(surface, a.min(b), a.max(b), CHAMFER);
        }
    }
}

/// The moulded cornice round `walls` at the top of their `height`: an
/// architrave, a frieze and a corona stepping out to [`PROUD`].
pub(crate) fn cornice(shape: &mut Shape, surface: Surface, walls: &Walls) {
    let h = walls.height;
    for (out, y0, y1) in [
        (0.03, h - 0.55, h - 0.42),
        (0.015, h - 0.42, h - 0.2),
        (0.06, h - 0.2, h - 0.1),
        (PROUD, h - 0.1, h + 0.04),
    ] {
        super::walls::ring(shape, surface, walls, out, (y0, y1), 0.14);
    }
}

/// What tops a front's parapet between its pedestals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Crown {
    /// A solid wall (behind a sign, or plain).
    Solid,
    /// Turned balusters between a base and a top rail.
    Balustrade,
}

/// A parapet `height` tall over `walls` with a coping on it: `crown` on
/// the front between pedestals over the front's `pillars` (meters along
/// it), and solid on the sides and the back when `all_round` (else the
/// front alone stands up, hiding a roof behind it).
pub(crate) fn parapet(
    shape: &mut Shape,
    surface: Surface,
    walls: &Walls,
    height: f32,
    (crown, all_round): (Crown, bool),
    pillars: &[f32],
) {
    let (h, t) = (walls.height + 0.04, walls.thickness * 0.6);
    let top = walls.height + height;
    if all_round {
        sides_and_back(shape, surface, walls, (h, top), t);
    }
    let front = walls.front();
    let pw = 0.5;
    let mut posts: Vec<f32> = pillars.to_vec();
    posts.sort_by(f32::total_cmp);
    for &at in &posts {
        let (min, max) = wall_box(
            &front,
            (at - pw / 2.0, -0.08, h),
            (at + pw / 2.0, t, top + 0.1),
        );
        shape.chamfered(surface, min, max, CHAMFER);
        let (min, max) = wall_box(
            &front,
            (at - pw / 2.0 - 0.04, -PROUD, top + 0.1),
            (at + pw / 2.0 + 0.04, t + 0.04, top + 0.18),
        );
        shape.block(surface, min, max);
    }
    // The front between the pedestals (and out to the ends).
    let mut from = 0.0;
    for to in posts
        .iter()
        .map(|&p| (p - pw / 2.0, p + pw / 2.0))
        .chain([(front.length, front.length)])
    {
        if to.0 > from + 0.02 {
            front_bay(shape, surface, &front, (from, to.0), (h, top), t, crown);
        }
        from = to.1;
    }
}

/// The solid parapet over the sides and the back of `walls`, from `h` to
/// `top` and `t` thick, with its coping.
fn sides_and_back(
    shape: &mut Shape,
    surface: Surface,
    walls: &Walls,
    (h, top): (f32, f32),
    t: f32,
) {
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    shape.block(surface, Vec3::new(x0, h, z0), Vec3::new(x1, top, z0 + t));
    for (a, b) in [(x0, x0 + t), (x1 - t, x1)] {
        shape.block(surface, Vec3::new(a, h, z0 + t), Vec3::new(b, top, z1 - t));
    }
    let c = 0.05;
    shape.block(
        surface,
        Vec3::new(x0 - c, top, z0 - c),
        Vec3::new(x1 + c, top + 0.08, z0 + t + c),
    );
    for (a, b) in [(x0 - c, x0 + t + c), (x1 - t - c, x1 + c)] {
        shape.block(
            surface,
            Vec3::new(a, top, z0 + t + c),
            Vec3::new(b, top + 0.08, z1 - t - c),
        );
    }
}

/// One bay of the front parapet, `a..b` along `front`, from `y0` to `top`.
fn front_bay(
    shape: &mut Shape,
    surface: Surface,
    front: &Wall,
    (a, b): (f32, f32),
    (y0, top): (f32, f32),
    t: f32,
    crown: Crown,
) {
    let coping = |shape: &mut Shape| {
        let (min, max) = wall_box(front, (a, -0.05, top), (b, t + 0.05, top + 0.08));
        shape.block(surface, min, max);
    };
    match crown {
        Crown::Solid => {
            let (min, max) = wall_box(front, (a, 0.0, y0), (b, t, top));
            shape.block(surface, min, max);
            coping(shape);
        }
        Crown::Balustrade => {
            let (base, rail) = (y0 + 0.18, top - 0.1);
            let (min, max) = wall_box(front, (a, 0.0, y0), (b, t, base));
            shape.block(surface, min, max);
            let (min, max) = wall_box(front, (a, 0.0, rail), (b, t, top));
            shape.cuboid(
                surface,
                Transform::from_translation((min + max) / 2.0).with_scale(max - min),
                true,
            );
            coping(shape);
            let n = ((b - a) / 0.3).floor().max(1.0) as usize;
            for k in 0..n {
                let at = a + (b - a) * (k as f32 + 0.5) / n as f32;
                let foot = front.at(at, t / 2.0, base);
                baluster(shape, surface, foot, rail - base);
            }
        }
    }
}

/// A turned baluster `height` tall standing on `foot`.
pub(crate) fn baluster(shape: &mut Shape, surface: Surface, foot: Vec3, height: f32) {
    let l = height;
    shape.lathe(
        surface,
        Transform::from_translation(foot),
        &[(0.055, 0.0), (0.075, 0.3 * l), (0.035, 0.75 * l), (0.06, l)],
        5,
    );
}

/// An urn (macetón) on top of a pedestal at `foot`.
pub(crate) fn urn(shape: &mut Shape, surface: Surface, foot: Vec3) {
    shape.lathe(
        surface,
        Transform::from_translation(foot),
        &[
            (0.12, 0.0),
            (0.08, 0.1),
            (0.17, 0.3),
            (0.14, 0.5),
            (0.18, 0.55),
            (0.0, 0.6),
        ],
        8,
    );
}

/// A triangular pediment `x0..x1` wide on top of `base`, rising `rise`,
/// its face at `z` (local, facing `+Z`) and `depth` deep, with raking
/// cornices along its slopes.
pub(crate) fn pediment(
    shape: &mut Shape,
    surface: Surface,
    (x0, x1): (f32, f32),
    z: f32,
    base: f32,
    rise: f32,
    depth: f32,
) {
    let xm = (x0 + x1) / 2.0;
    let (l, r, apex) = (
        Vec3::new(x0, base, z),
        Vec3::new(x1, base, z),
        Vec3::new(xm, base + rise, z),
    );
    let back = Vec3::new(0.0, 0.0, -depth);
    shape.face(surface, &[l, r, apex]);
    shape.face(surface, &[r + back, l + back, apex + back]);
    shape.face(surface, &[r, r + back, apex + back, apex]);
    shape.face(surface, &[l + back, l, apex, apex + back]);
    // The raking cornices, standing out over the face.
    let lift = Vec3::new(0.0, 0.06, -depth / 2.0 + 0.04);
    let thick = depth + 0.08;
    for (from, to) in [(l, apex), (r, apex)] {
        let d = (to - from).normalize();
        shape.beam(
            surface,
            from + d * 0.12 + lift,
            to + d * 0.1 + lift,
            thick.min(0.2),
        );
    }
    shape.block(
        surface,
        Vec3::new(x0 - 0.1, base - 0.1, z - depth),
        Vec3::new(x1 + 0.1, base + 0.02, z + 0.06),
    );
}

/// A moulded frame round opening `o` of `wall`: jambs and a head standing
/// a little proud, and a hood cornice over it when `hood`.
pub(crate) fn surround(shape: &mut Shape, surface: Surface, wall: &Wall, o: &Opening, hood: bool) {
    let (a, b) = o.span();
    let (w, out) = (0.11, 0.035);
    let bottom = if o.door { 0.0 } else { o.bottom - 0.08 };
    for (j0, j1) in [(a - w, a), (b, b + w)] {
        let (min, max) = wall_box(wall, (j0, -out, bottom), (j1, 0.02, o.top + w));
        shape.block(surface, min, max);
    }
    let (min, max) = wall_box(wall, (a, -out, o.top), (b, 0.02, o.top + w));
    shape.cuboid(
        surface,
        Transform::from_translation((min + max) / 2.0).with_scale(max - min),
        true,
    );
    if hood {
        let (min, max) = wall_box(
            wall,
            (a - w - 0.08, -0.08, o.top + w),
            (b + w + 0.08, 0.02, o.top + w + 0.12),
        );
        shape.cuboid(
            surface,
            Transform::from_translation((min + max) / 2.0).with_scale(max - min),
            true,
        );
    }
}

/// Two plank leaves shut in door `o` of `wall`, set back in its reveal,
/// with a glazed transom over them when `transom`.
pub(crate) fn door_leaves(shape: &mut Shape, wall: &Wall, o: &Opening, transom: bool) {
    let (a, b) = o.span();
    let t = wall.thickness;
    let top = if transom { o.top - 0.5 } else { o.top };
    let m = (a + b) / 2.0;
    for (l0, l1) in [(a, m - 0.015), (m + 0.015, b)] {
        wall.piece(
            shape,
            Surface::Planks,
            (l0, t * 0.45, o.bottom),
            (l1, t * 0.5, top),
        );
    }
    if transom {
        wall.piece(
            shape,
            Surface::Planks,
            (a, t * 0.44, top),
            (b, t * 0.5, top + 0.08),
        );
        wall.piece(
            shape,
            Surface::Recess,
            (a, t * 0.46, top + 0.08),
            (b, t * 0.48, o.top),
        );
    }
}

/// Iron bars (a grille) across window `o` of `wall`, just inside its face.
pub(crate) fn grille(shape: &mut Shape, wall: &Wall, o: &Opening) {
    let (a, b) = o.span();
    let n = ((b - a) / 0.13).floor().max(2.0) as usize;
    for k in 1..n {
        let at = a + (b - a) * k as f32 / n as f32;
        shape.beam(
            Surface::Steel,
            wall.at(at, 0.06, o.bottom),
            wall.at(at, 0.06, o.top),
            0.025,
        );
    }
    for y in [o.bottom + 0.25, o.top - 0.25] {
        shape.beam(
            Surface::Steel,
            wall.at(a, 0.06, y),
            wall.at(b, 0.06, y),
            0.025,
        );
    }
}

/// The round head of opening `o` in `wall`: the wall filled in between
/// its square top and a half circle springing `o.width / 2` below it, and
/// a moulded archivolt round the curve in `trim`.
pub(crate) fn arch(
    shape: &mut Shape,
    (surface, trim): (Surface, Surface),
    wall: &Wall,
    o: &Opening,
) {
    let (a, b) = o.span();
    let r = o.width / 2.0;
    let spring = o.top - r;
    let depth = wall.thickness * 0.7;
    let n = 8;
    let arc = |k: usize| {
        let angle = std::f32::consts::PI * (1.0 - k as f32 / n as f32);
        (o.at + r * angle.cos(), spring + r * angle.sin())
    };
    for k in 0..n {
        let ((u0, v0), (u1, v1)) = (arc(k), arc(k + 1));
        let (u0, u1) = (u0.clamp(a, b), u1.clamp(a, b));
        let p = |u: f32, inward: f32, y: f32| wall.at(u, inward, y);
        // Face, soffit and back, wound outward along the wall's `out`.
        let face = [
            p(u0, 0.0, v0),
            p(u1, 0.0, v1),
            p(u1, 0.0, o.top),
            p(u0, 0.0, o.top),
        ];
        let soffit = [
            p(u0, depth, v0),
            p(u1, depth, v1),
            p(u1, 0.0, v1),
            p(u0, 0.0, v0),
        ];
        let back = [
            p(u1, depth, v1),
            p(u0, depth, v0),
            p(u0, depth, o.top),
            p(u1, depth, o.top),
        ];
        for mut quad in [face, soffit, back] {
            let n = (quad[1] - quad[0]).cross(quad[2] - quad[0]);
            let centre = quad.iter().sum::<Vec3>() / 4.0;
            // The soffit faces the arch's centre, down into the opening.
            let want = if quad == soffit {
                wall.at(o.at, depth / 2.0, spring) - centre
            } else if quad == back {
                -wall.out
            } else {
                wall.out
            };
            if n.dot(want) < 0.0 {
                quad.reverse();
            }
            shape.face(surface, &quad);
        }
        shape.beam(
            trim,
            p(arc(k).0, -0.03, arc(k).1 + 0.05),
            p(arc(k + 1).0, -0.03, arc(k + 1).1 + 0.05),
            0.1,
        );
    }
}

/// How a galería's posts are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Posts {
    /// Turned cast iron with a capital and scrolled brackets.
    Iron,
    /// Square timber with knee braces.
    Timber,
}

/// A galería over the sidewalk in front of `front` (a front wall, its
/// outer face where it starts), `depth` deep: a corrugated `roof` sloping
/// from `high` at the wall down to `low` at its posts, a fascia board along
/// its edge and a row of `posts` about every three meters.
pub(crate) fn gallery(
    shape: &mut Shape,
    roof: Surface,
    posts: Posts,
    front: &Wall,
    depth: f32,
    (high, low): (f32, f32),
) {
    let len = front.length;
    let edge = depth - 0.15;
    let n = ((len / 3.0).round() as usize).max(1);
    let top = low - 0.25;
    for k in 0..=n {
        let at = 0.2 + (len - 0.4) * k as f32 / n as f32;
        let foot = front.at(at, -edge, 0.0);
        match posts {
            Posts::Iron => {
                shape.lathe(
                    Surface::Steel,
                    Transform::from_translation(foot),
                    &[
                        (0.1, 0.0),
                        (0.08, 0.3),
                        (0.055, 0.45),
                        (0.055, top - 0.4),
                        (0.09, top - 0.25),
                        (0.11, top),
                    ],
                    8,
                );
            }
            Posts::Timber => shape.beam(Surface::Planks, foot, foot.with_y(top), 0.16),
        }
        // Brackets from the post up to the fascia beam, along the front.
        let reach = 0.5;
        for side in [-1.0, 1.0] {
            let along = (at + side * reach).clamp(0.0, len);
            if (along - at).abs() < 0.1 {
                continue;
            }
            let (s, t) = match posts {
                Posts::Iron => (Surface::Steel, 0.04),
                Posts::Timber => (Surface::Planks, 0.08),
            };
            shape.beam(s, foot.with_y(top - 0.55), front.at(along, -edge, top), t);
        }
    }
    // The beam on the posts, the fascia over it, and the sheet.
    let (min, max) = wall_box(
        front,
        (0.0, -edge - 0.09, top),
        (len, -edge + 0.09, low - 0.05),
    );
    shape.cuboid(
        Surface::Planks,
        Transform::from_translation((min + max) / 2.0).with_scale(max - min),
        true,
    );
    let (min, max) = wall_box(
        front,
        (0.0, -depth - 0.05, low - 0.3),
        (len, -depth + 0.02, low + 0.02),
    );
    shape.cuboid(
        Surface::Planks,
        Transform::from_translation((min + max) / 2.0).with_scale(max - min),
        true,
    );
    // The sheet, built along local X then turned onto the wall.
    let mut sheet = Shape::default();
    slab(
        &mut sheet,
        roof,
        Vec3::new(0.0, low, depth + 0.05),
        Vec3::new(0.0, high, 0.0),
        len,
        0.05,
    );
    let turn = Quat::from_rotation_arc(Vec3::Z, front.out);
    let mid = front.at(len / 2.0, 0.0, 0.0);
    shape.merge(sheet, Transform::from_translation(mid).with_rotation(turn));
}

/// A four-sided spire (or pyramid roof) on a square `size` wide at `base`,
/// its tip `height` over it.
pub(crate) fn spire(shape: &mut Shape, surface: Surface, centre: Vec3, size: f32, height: f32) {
    let h = size / 2.0;
    let c = [
        centre + Vec3::new(-h, 0.0, h),
        centre + Vec3::new(h, 0.0, h),
        centre + Vec3::new(h, 0.0, -h),
        centre + Vec3::new(-h, 0.0, -h),
    ];
    let tip = centre + Vec3::Y * height;
    for k in 0..4 {
        shape.face(surface, &[c[k], c[(k + 1) % 4], tip]);
    }
}
