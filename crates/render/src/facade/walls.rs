//! Walls with their openings, and the roofs over them: the building
//! blocks every settlement house and town building is made of.
//!
//! A [`Walls`] is the four walls of a box in the structure's own space
//! (front at `z1`); each [`Wall`] builds itself with doors and windows left
//! open. The roofs: a gable ([`gable_roof`]), a single slope falling to the
//! front ([`lean_to`]), and a flat azotea behind a parapet
//! ([`parapet_roof`]). The galpón ([`barn`]) is a whole building.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;

/// A doorway or a window in a wall.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opening {
    /// Its middle, in meters along the wall from the wall's start.
    pub at: f32,
    pub width: f32,
    pub bottom: f32,
    pub top: f32,
    pub door: bool,
}

impl Opening {
    /// Its two jambs, in meters along the wall.
    pub fn span(&self) -> (f32, f32) {
        (self.at - self.width / 2.0, self.at + self.width / 2.0)
    }
}

/// One straight wall, `thickness` thick inward from its outer face.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wall {
    /// Where its outer face starts, on the ground.
    pub start: Vec3,
    /// Unit direction along it (local `±X` or `±Z`).
    pub along: Vec3,
    /// Unit direction out of its outer face.
    pub out: Vec3,
    pub length: f32,
    pub thickness: f32,
    pub height: f32,
    pub drop: f32,
}

impl Wall {
    /// The point `a` meters along, `inward` meters in from the outer face,
    /// at height `y`.
    pub fn at(&self, a: f32, inward: f32, y: f32) -> Vec3 {
        self.start + self.along * a - self.out * inward + Vec3::Y * y
    }

    /// An axis-aligned box between two points given in wall terms.
    pub fn piece(
        &self,
        shape: &mut Shape,
        surface: Surface,
        lo: (f32, f32, f32),
        hi: (f32, f32, f32),
    ) {
        let (p, q) = (self.at(lo.0, lo.1, lo.2), self.at(hi.0, hi.1, hi.2));
        // Undersides show only on lintels, which face down into an opening.
        let min = p.min(q);
        let max = p.max(q);
        if lo.2 > 0.0 {
            shape.cuboid(
                surface,
                Transform::from_translation((min + max) / 2.0).with_scale(max - min),
                true,
            );
        } else {
            shape.block(surface, min, max);
        }
    }

    /// The wall in `surface` with `openings` left in it: piers between
    /// them, a lintel over and a sill under each, a dark recess inside, a
    /// plank door (shut unless `open`) or a window with its two shutters
    /// swung back against the wall.
    pub fn build(&self, shape: &mut Shape, surface: Surface, openings: &[Opening], open: u32) {
        let (t, h, drop) = (self.thickness, self.height, self.drop);
        let mut from = 0.0;
        let mut sorted = openings.to_vec();
        sorted.sort_by(|a, b| a.at.total_cmp(&b.at));
        for (k, o) in sorted.iter().enumerate() {
            let (a, b) = o.span();
            if a > from {
                self.piece(shape, surface, (from, 0.0, -drop), (a, t, h));
            }
            if o.top < h {
                self.piece(shape, surface, (a, 0.0, o.top), (b, t, h));
            }
            if o.bottom > -drop + 0.01 {
                self.piece(shape, surface, (a, 0.0, -drop), (b, t, o.bottom));
            }
            // The dark inside, seen through the opening.
            self.piece(
                shape,
                Surface::Recess,
                (a, t * 0.7, o.bottom),
                (b, t * 0.75, o.top),
            );
            if o.door {
                // Planks on a frame; a seeded few doors stand open.
                if (open >> (k % 16)) & 1 == 0 {
                    self.piece(
                        shape,
                        Surface::Planks,
                        (a, t * 0.45, o.bottom),
                        (b, t * 0.5, o.top),
                    );
                }
            } else {
                // The sill sticks out, and the shutters lie back open.
                self.piece(
                    shape,
                    surface,
                    (a - 0.05, -0.06, o.bottom - 0.08),
                    (b + 0.05, 0.0, o.bottom),
                );
                let leaf = o.width / 2.0;
                for (l0, l1) in [(a - leaf - 0.02, a - 0.02), (b + 0.02, b + leaf + 0.02)] {
                    self.piece(
                        shape,
                        Surface::Planks,
                        (l0, -0.05, o.bottom + 0.02),
                        (l1, -0.01, o.top - 0.02),
                    );
                }
                // A glazed sash in the reveal, dark.
                self.piece(
                    shape,
                    Surface::Recess,
                    (a, t * 0.3, o.bottom),
                    (b, t * 0.32, o.top),
                );
            }
            from = b;
        }
        if from < self.length {
            self.piece(shape, surface, (from, 0.0, -drop), (self.length, t, h));
        }
    }
}

/// A house's four walls round `x0..x1`, `z0..z1` (outer faces): front at
/// `z1`, back at `z0`, both running the full width; the sides between them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walls {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    pub thickness: f32,
    pub height: f32,
    pub drop: f32,
}

impl Walls {
    pub fn front(&self) -> Wall {
        Wall {
            start: Vec3::new(self.x0, 0.0, self.z1),
            along: Vec3::X,
            out: Vec3::Z,
            length: self.x1 - self.x0,
            thickness: self.thickness,
            height: self.height,
            drop: self.drop,
        }
    }

    pub fn back(&self) -> Wall {
        Wall {
            start: Vec3::new(self.x1, 0.0, self.z0),
            along: Vec3::NEG_X,
            out: Vec3::NEG_Z,
            ..self.front()
        }
    }

    /// The side at `x1` (`east = true`) or at `x0`, between the front and
    /// back walls.
    pub fn side(&self, east: bool) -> Wall {
        let t = self.thickness;
        let (x, z, along, out) = if east {
            (self.x1, self.z1 - t, Vec3::NEG_Z, Vec3::X)
        } else {
            (self.x0, self.z0 + t, Vec3::Z, Vec3::NEG_X)
        };
        Wall {
            start: Vec3::new(x, 0.0, z),
            along,
            out,
            length: self.z1 - self.z0 - 2.0 * t,
            ..self.front()
        }
    }

    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn depth(&self) -> f32 {
        self.z1 - self.z0
    }
}

/// `count` openings spread evenly along a wall `length` long: doors at the
/// `doors` positions, windows elsewhere.
pub(crate) fn spread(
    length: f32,
    count: usize,
    doors: &[usize],
    door: (f32, f32),
    window: (f32, f32, f32),
) -> Vec<Opening> {
    (0..count)
        .map(|k| {
            let at = length * (k as f32 + 0.5) / count as f32;
            if doors.contains(&k) {
                Opening {
                    at,
                    width: door.0,
                    bottom: 0.0,
                    top: door.1,
                    door: true,
                }
            } else {
                Opening {
                    at,
                    width: window.0,
                    bottom: window.1,
                    top: window.2,
                    door: false,
                }
            }
        })
        .collect()
}

/// A thick or thin roof slab over a slope from `low` (the eave edge) up to
/// `high` (the ridge or high edge), `width` meters along `X`, its
/// underside on the line.
pub(crate) fn slab(
    shape: &mut Shape,
    surface: Surface,
    low: Vec3,
    high: Vec3,
    width: f32,
    thickness: f32,
) {
    let d = high - low;
    let len = d.length();
    // The slope's direction in the Y–Z plane, and its outward normal.
    let dir = d / len;
    let normal = Vec3::new(0.0, dir.z.abs(), if dir.z < 0.0 { dir.y } else { -dir.y });
    let normal = normal.normalize_or(Vec3::Y);
    let center = (low + high) / 2.0 + normal * (thickness / 2.0);
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir.with_x(0.0).normalize_or(Vec3::Z));
    shape.cuboid(
        surface,
        Transform::from_translation(center)
            .with_rotation(rotation)
            .with_scale(Vec3::new(width, thickness, len)),
        true,
    );
}

/// A gable roof over `walls`, its ridge along `X`, rising `rise` over the
/// eaves, overhanging `overhang` all round; the gable ends filled in
/// `gable`.
pub(crate) fn gable_roof(
    shape: &mut Shape,
    (roof, gable): (Surface, Surface),
    walls: &Walls,
    rise: f32,
    overhang: f32,
    thickness: f32,
) {
    let (h, zc) = (walls.height, (walls.z0 + walls.z1) / 2.0);
    let half = walls.depth() / 2.0;
    let slope = rise / half;
    let ridge = Vec3::new(0.0, h + rise, zc);
    let xc = (walls.x0 + walls.x1) / 2.0;
    let width = walls.width() + 2.0 * overhang;
    for s in [1.0, -1.0] {
        let eave = Vec3::new(xc, h - overhang * slope, zc + s * (half + overhang));
        slab(shape, roof, eave, ridge.with_x(xc), width, thickness);
    }
    // The gable ends, flush with the side walls' outer faces.
    for (x, e) in [(walls.x1, 1.0), (walls.x0, -1.0)] {
        let (f, b, top) = (
            Vec3::new(x, h, walls.z1),
            Vec3::new(x, h, walls.z0),
            Vec3::new(x, h + rise, zc),
        );
        if e > 0.0 {
            shape.face(gable, &[f, b, top]);
        } else {
            shape.face(gable, &[b, f, top]);
        }
    }
}

/// A solid wall top over `xa..xb`, `za..zb`, from `base` up to a sloped
/// top `ya` high at `za` and `yb` at `zb`, its bottom left open on the wall
/// it crowns.
pub(crate) fn wedge(
    shape: &mut Shape,
    surface: Surface,
    (xa, xb): (f32, f32),
    (za, zb): (f32, f32),
    base: f32,
    (ya, yb): (f32, f32),
) {
    const EPS: f32 = 1e-3;
    // The end profile, in (z, y), without its corners of no height.
    let mut profile = vec![(zb, base), (za, base)];
    if ya - base > EPS {
        profile.push((za, ya));
    }
    if yb - base > EPS {
        profile.push((zb, yb));
    }
    if profile.len() < 3 {
        return;
    }
    let east: Vec<Vec3> = profile.iter().map(|&(z, y)| Vec3::new(xb, y, z)).collect();
    let west: Vec<Vec3> = profile
        .iter()
        .rev()
        .map(|&(z, y)| Vec3::new(xa, y, z))
        .collect();
    shape.face(surface, &east);
    shape.face(surface, &west);
    if yb - base > EPS {
        shape.face(
            surface,
            &[
                Vec3::new(xa, base, zb),
                Vec3::new(xb, base, zb),
                Vec3::new(xb, yb, zb),
                Vec3::new(xa, yb, zb),
            ],
        );
    }
    if ya - base > EPS {
        shape.face(
            surface,
            &[
                Vec3::new(xb, base, za),
                Vec3::new(xa, base, za),
                Vec3::new(xa, ya, za),
                Vec3::new(xb, ya, za),
            ],
        );
    }
    shape.face(
        surface,
        &[
            Vec3::new(xa, ya, za),
            Vec3::new(xa, yb, zb),
            Vec3::new(xb, yb, zb),
            Vec3::new(xb, ya, za),
        ],
    );
}

/// A single-slope (a un agua) roof of corrugated iron, high over the back
/// wall and low over the front so its ridges are seen from the patio,
/// overhanging `overhang` at the eaves and the verges. Every wall is built
/// up in `gable` to meet the sheet: the sides as solid gables, the back
/// and front as sloped bands.
pub(crate) fn lean_to(
    shape: &mut Shape,
    (roof, gable): (Surface, Surface),
    walls: &Walls,
    rise: f32,
    overhang: f32,
) {
    let (h, t) = (walls.height, walls.thickness);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    let slope = rise / walls.depth();
    // The sheet's underside, over the outer faces.
    let under = |z: f32| h + (z1 - z) * slope;
    let xc = (x0 + x1) / 2.0;
    let low = Vec3::new(xc, under(z1 + overhang), z1 + overhang);
    let high = Vec3::new(xc, under(z0 - overhang), z0 - overhang);
    slab(shape, roof, low, high, walls.width() + 2.0 * overhang, 0.06);
    // The side gables run the whole depth, over the corners.
    for xs in [(x1 - t, x1), (x0, x0 + t)] {
        wedge(shape, gable, xs, (z0, z1), h, (under(z0), under(z1)));
    }
    for zs in [(z0, z0 + t), (z1 - t, z1)] {
        wedge(
            shape,
            gable,
            (x0 + t, x1 - t),
            zs,
            h,
            (under(zs.0), under(zs.1)),
        );
    }
}

/// A band round `walls` from `y0` to `y1`, standing `out` proud of their
/// outer faces and reaching `inner` in from them.
pub(crate) fn ring(
    shape: &mut Shape,
    surface: Surface,
    walls: &Walls,
    out: f32,
    (y0, y1): (f32, f32),
    inner: f32,
) {
    let (a0, a1, b0, b1) = (
        walls.x0 - out,
        walls.x1 + out,
        walls.z0 - out,
        walls.z1 + out,
    );
    let w = inner + out;
    shape.block(surface, Vec3::new(a0, y0, b1 - w), Vec3::new(a1, y1, b1));
    shape.block(surface, Vec3::new(a0, y0, b0), Vec3::new(a1, y1, b0 + w));
    shape.block(
        surface,
        Vec3::new(a0, y0, b0 + w),
        Vec3::new(a0 + w, y1, b1 - w),
    );
    shape.block(
        surface,
        Vec3::new(a1 - w, y0, b0 + w),
        Vec3::new(a1, y1, b1 - w),
    );
}

/// A flat azotea behind a parapet `parapet` tall, with a cornice band
/// under it and a coping on top.
pub(crate) fn parapet_roof(shape: &mut Shape, surface: Surface, walls: &Walls, parapet: f32) {
    let (h, t) = (walls.height, walls.thickness);
    flat_roof(shape, walls);
    ring(shape, surface, walls, 0.0, (h, h + parapet), t * 0.6);
    // The cornice under the parapet and the coping over it stand proud.
    ring(shape, surface, walls, 0.1, (h - 0.22, h + 0.06), 0.12);
    ring(
        shape,
        surface,
        walls,
        0.06,
        (h + parapet, h + parapet + 0.08),
        t * 0.6,
    );
}

/// The brick-tiled flat roof inside `walls`, just under a parapet's foot.
pub(crate) fn flat_roof(shape: &mut Shape, walls: &Walls) {
    let (h, t) = (walls.height, walls.thickness);
    shape.block(
        Surface::Brick,
        Vec3::new(walls.x0 + t, h - 0.25, walls.z0 + t),
        Vec3::new(walls.x1 - t, h - 0.1, walls.z1 - t),
    );
}

/// A brick chimney through a roof at `at`, `top` meters up.
pub(crate) fn chimney(shape: &mut Shape, at: Vec3, top: f32) {
    shape.block(
        Surface::Brick,
        at + Vec3::new(-0.3, 0.0, -0.25),
        Vec3::new(at.x + 0.3, top, at.z + 0.25),
    );
    shape.block(
        Surface::Brick,
        Vec3::new(at.x - 0.36, top, at.z - 0.31),
        Vec3::new(at.x + 0.36, top + 0.12, at.z + 0.31),
    );
}

/// How far a galpón's roof reaches out past its walls ([`barn`]): its
/// 0.4 m eaves and verges, with the slab's thickness tilted out. A galpón
/// that must keep inside a footprint stands this far in from it.
pub(crate) const BARN_REACH: f32 = 0.45;

/// The galpón: `walls` of corrugated iron on timber, a gable roof along its
/// width (`X`) in `roof`, and `doors` pairs of big plank doors spread along
/// its front; `w`, `d`, `h` its width, depth and wall height.
pub(crate) fn barn(
    shape: &mut Shape,
    (walls, roof): (Surface, Surface),
    (w, d, h): (f32, f32, f32),
    drop: f32,
    v: u32,
    doors: usize,
) {
    let box_ = Walls {
        x0: -w / 2.0,
        x1: w / 2.0,
        z0: -d / 2.0,
        z1: d / 2.0,
        thickness: 0.08,
        height: h,
        drop,
    };
    let doors = doors.max(1);
    let bay = w / doors as f32;
    let mut width = (w * 0.3).clamp(4.0, 6.0);
    if doors > 1 {
        width = width.min(bay - 1.5).max(2.0);
    }
    let openings: Vec<Opening> = (0..doors)
        .map(|k| Opening {
            at: bay * (k as f32 + 0.5),
            width,
            bottom: 0.0,
            top: (h - 0.8).min(4.0),
            door: true,
        })
        .collect();
    // A big door is mostly shut; one in three stands open on the dark.
    let open = u32::from(v.is_multiple_of(3));
    box_.front().build(shape, walls, &openings, open);
    box_.back().build(shape, walls, &[], 0);
    for east in [true, false] {
        box_.side(east).build(shape, walls, &[], 0);
    }
    // Timber corner posts and a plank skirting, the doors' frame.
    for (x, z) in [
        (box_.x0, box_.z0),
        (box_.x1, box_.z0),
        (box_.x0, box_.z1),
        (box_.x1, box_.z1),
    ] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, -drop, z),
            Vec3::new(x, h, z),
            0.2,
        );
    }
    for door in &openings {
        let (a, b) = door.span();
        for x in [box_.x0 + a, box_.x0 + b] {
            shape.beam(
                Surface::Planks,
                Vec3::new(x, -drop, box_.z1 + 0.05),
                Vec3::new(x, door.top + 0.15, box_.z1 + 0.05),
                0.18,
            );
        }
        shape.beam(
            Surface::Planks,
            Vec3::new(box_.x0 + a - 0.1, door.top + 0.1, box_.z1 + 0.05),
            Vec3::new(box_.x0 + b + 0.1, door.top + 0.1, box_.z1 + 0.05),
            0.18,
        );
    }
    let rise = d / 2.0 * 0.35;
    gable_roof(shape, (roof, roof), &box_, rise, 0.4, 0.06);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wall_leaves_its_openings_open() {
        let wall = Walls {
            x0: -5.0,
            x1: 5.0,
            z0: -3.0,
            z1: 3.0,
            thickness: 0.4,
            height: 3.0,
            drop: 0.3,
        }
        .front();
        let mut shape = Shape::default();
        let door = Opening {
            at: 5.0,
            width: 1.0,
            bottom: 0.0,
            top: 2.2,
            door: true,
        };
        // Door open (bit 0 set): nothing of the wall fills the doorway.
        wall.build(&mut shape, Surface::Whitewash, &[door], 1);
        let blocked = shape.points(Surface::Whitewash).chunks(4).any(|q| {
            let c = q.iter().fold(Vec3::ZERO, |a, p| a + *p) / 4.0;
            c.x.abs() < 0.4 && c.y > 0.2 && c.y < 2.0 && c.z > 2.6
        });
        assert!(!blocked);
        assert!(shape.surfaces().any(|s| s == Surface::Recess));
        assert!(!shape.surfaces().any(|s| s == Surface::Planks));
    }

    /// The walls of a chacra's adobe house under a lean-to.
    fn cottage() -> Walls {
        Walls {
            x0: -7.0,
            x1: 7.0,
            z0: -3.8,
            z1: 3.8,
            thickness: 0.5,
            height: 3.4,
            drop: 0.3,
        }
    }

    /// The highest point of `surface` straight over `(x, z)`, if any of its
    /// triangles covers it.
    pub(crate) fn top_at(shape: &Shape, surface: Surface, x: f32, z: f32) -> Option<f32> {
        shape
            .triangles_of(surface)
            .into_iter()
            .filter_map(|[a, b, c]| {
                let (e1, e2, p) = (b - a, c - a, Vec3::new(x, 0.0, z) - a);
                let det = e1.x * e2.z - e2.x * e1.z;
                if det.abs() < 1e-6 {
                    return None;
                }
                let u = (p.x * e2.z - e2.x * p.z) / det;
                let v = (e1.x * p.z - p.x * e1.z) / det;
                (u >= -1e-4 && v >= -1e-4 && u + v <= 1.0 + 1e-4)
                    .then_some(a.y + u * e1.y + v * e2.y)
            })
            .reduce(f32::max)
    }

    #[test]
    fn a_lean_to_rests_on_all_four_walls() {
        let walls = cottage();
        let mut shape = Shape::default();
        lean_to(
            &mut shape,
            (Surface::OldIron, Surface::Adobe),
            &walls,
            0.7,
            0.35,
        );
        for east in [true, false] {
            walls.side(east).build(&mut shape, Surface::Adobe, &[], 0);
        }
        walls.front().build(&mut shape, Surface::Adobe, &[], 0);
        walls.back().build(&mut shape, Surface::Adobe, &[], 0);
        let t = walls.thickness;
        // Down the middle of each wall's top, the wall meets the roof's
        // underside: no gap, and nothing through the sheet.
        let mut columns = Vec::new();
        for k in 0..=20 {
            let z = walls.z0 + 0.05 + (walls.depth() - 0.1) * k as f32 / 20.0;
            columns.push((walls.x1 - t / 2.0, z));
            columns.push((walls.x0 + t / 2.0, z));
        }
        for k in 0..=20 {
            let x = walls.x0 + 0.05 + (walls.width() - 0.1) * k as f32 / 20.0;
            columns.push((x, walls.z1 - t / 2.0));
            columns.push((x, walls.z0 + t / 2.0));
        }
        for (x, z) in columns {
            let wall = top_at(&shape, Surface::Adobe, x, z).expect("a wall here");
            let under = top_at(&shape, Surface::OldIron, x, z).expect("a roof here") - 0.06;
            assert!(
                wall > under - 0.03 && wall < under + 0.01,
                "at ({x}, {z}) the wall tops out at {wall}, the roof's underside is at {under}"
            );
        }
    }

    #[test]
    fn a_lean_to_overhangs_its_walls_a_little() {
        let walls = cottage();
        let mut shape = Shape::default();
        lean_to(
            &mut shape,
            (Surface::OldIron, Surface::Adobe),
            &walls,
            0.7,
            0.35,
        );
        let roof = shape.points(Surface::OldIron);
        let min = roof.iter().fold(Vec3::MAX, |a, p| a.min(*p));
        let max = roof.iter().fold(Vec3::MIN, |a, p| a.max(*p));
        for over in [
            walls.x0 - min.x,
            max.x - walls.x1,
            walls.z0 - min.z,
            max.z - walls.z1,
        ] {
            assert!((0.3..=0.6).contains(&over), "overhang {over}");
        }
    }

    #[test]
    fn the_lean_to_corrugation_runs_down_its_slope() {
        let walls = cottage();
        let mut shape = Shape::default();
        lean_to(
            &mut shape,
            (Surface::OldIron, Surface::Adobe),
            &walls,
            0.7,
            0.35,
        );
        // The sheet's upper face: its four corners.
        let top: Vec<_> = shape
            .vertices(Surface::OldIron)
            .into_iter()
            .filter(|(_, n, _)| n.y > 0.5)
            .collect();
        assert_eq!(top.len(), 4);
        let (high, low) = (
            top.iter().max_by(|a, b| a.0.y.total_cmp(&b.0.y)).unwrap(),
            top.iter().min_by(|a, b| a.0.y.total_cmp(&b.0.y)).unwrap(),
        );
        // The ridges of the texture run along `v`: down the slope `v`
        // changes and `u` does not, and the sheet repeats several times.
        let down = low.2 - high.2;
        assert!(down.y.abs() > 2.0 && down.x.abs() < 0.2, "{down:?}");
        let along = top.iter().map(|(_, _, uv)| uv.x).fold(f32::MIN, f32::max)
            - top.iter().map(|(_, _, uv)| uv.x).fold(f32::MAX, f32::min);
        assert!(along > 2.0, "{along}");
        // The top faces up and out over the front, where it is seen from.
        assert!(top.iter().all(|(_, n, _)| n.z > 0.0), "{top:?}");
    }
}
