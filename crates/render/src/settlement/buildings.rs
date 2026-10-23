//! The settlements' houses and galpones, built procedurally from the 1919
//! building list (`odd/setting-1919.md`).
//!
//! Every building is made in its own space: `x` along its front, `z` out
//! of its front (the front wall's outer face at `+depth / 2`), `y` up from
//! the ground at its middle. Walls run down to `-drop` so they meet the
//! ground on a slope.
//!
//! - The adobe rancho: thick mud-plastered walls under a steep, thick paja
//!   roof with a deep overhang, or under a single slope of old corrugated
//!   iron.
//! - The chacra house and the brick worker houses: whitewash or bare brick,
//!   a flat azotea behind a parapet with a cornice, tall openings with
//!   wooden doors and shutters.
//! - The estancia casco: a long Italianate whitewashed house with pilasters,
//!   a parapet and a front gallery on wooden or iron posts.
//! - The galpón: corrugated-iron walls and gable roof on timber corner
//!   posts, with big plank doors.

use bevy::prelude::*;
use pampa_core::{HouseStyle, Structure, StructureKind};

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
    fn at(&self, a: f32, inward: f32, y: f32) -> Vec3 {
        self.start + self.along * a - self.out * inward + Vec3::Y * y
    }

    /// An axis-aligned box between two points given in wall terms.
    fn piece(&self, shape: &mut Shape, surface: Surface, lo: (f32, f32, f32), hi: (f32, f32, f32)) {
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
            let (a, b) = (o.at - o.width / 2.0, o.at + o.width / 2.0);
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
fn spread(
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
fn slab(shape: &mut Shape, surface: Surface, low: Vec3, high: Vec3, width: f32, thickness: f32) {
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
fn gable_roof(
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

/// A single-slope (a un agua) roof, high over the front wall and low over
/// the back, overhanging `overhang`; the side walls topped with their
/// triangles in `gable`.
fn lean_to(
    shape: &mut Shape,
    (roof, gable): (Surface, Surface),
    walls: &Walls,
    rise: f32,
    overhang: f32,
) {
    let h = walls.height;
    let slope = rise / walls.depth();
    let xc = (walls.x0 + walls.x1) / 2.0;
    let low = Vec3::new(xc, h - overhang * slope, walls.z0 - overhang);
    let high = Vec3::new(xc, h + rise + overhang * slope, walls.z1 + overhang);
    slab(shape, roof, low, high, walls.width() + 2.0 * overhang, 0.06);
    for (x, e) in [(walls.x1, 1.0), (walls.x0, -1.0)] {
        let (f, b, top) = (
            Vec3::new(x, h, walls.z1),
            Vec3::new(x, h, walls.z0),
            Vec3::new(x, h + rise, walls.z1),
        );
        if e > 0.0 {
            shape.face(gable, &[f, b, top]);
        } else {
            shape.face(gable, &[b, f, top]);
        }
    }
}

/// A flat azotea behind a parapet `parapet` tall, with a cornice band
/// under it and a coping on top.
fn parapet_roof(shape: &mut Shape, surface: Surface, walls: &Walls, parapet: f32) {
    let (h, t) = (walls.height, walls.thickness);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    // The brick-tiled roof, just under the parapet's foot.
    shape.block(
        Surface::Brick,
        Vec3::new(x0 + t, h - 0.25, z0 + t),
        Vec3::new(x1 - t, h - 0.1, z1 - t),
    );
    let ring = |shape: &mut Shape, surface: Surface, out: f32, y0: f32, y1: f32, inner: f32| {
        let (a0, a1, b0, b1) = (x0 - out, x1 + out, z0 - out, z1 + out);
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
    };
    ring(shape, surface, 0.0, h, h + parapet, t * 0.6);
    // The cornice under the parapet and the coping over it stand proud.
    ring(shape, surface, 0.1, h - 0.22, h + 0.06, 0.12);
    ring(
        shape,
        surface,
        0.06,
        h + parapet,
        h + parapet + 0.08,
        t * 0.6,
    );
}

/// A brick chimney through a roof at `at`, `top` meters up.
fn chimney(shape: &mut Shape, at: Vec3, top: f32) {
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

/// The size of `structure` in its own space: width along its front and
/// depth out of it.
pub(crate) fn local_size(structure: &Structure) -> Vec2 {
    let s = structure.footprint.size();
    match structure.front {
        pampa_core::Side::North | pampa_core::Side::South => Vec2::new(s.x, s.y),
        _ => Vec2::new(s.y, s.x),
    }
}

/// The house or galpón `structure` in its own space, its walls run
/// `drop` meters below its middle's ground.
pub(crate) fn building(structure: &Structure, drop: f32) -> Shape {
    let size = local_size(structure);
    let (w, d, h) = (size.x, size.y, structure.height);
    let v = structure.variant;
    let mut shape = Shape::default();
    match structure.kind {
        StructureKind::MainHouse(style) | StructureKind::WorkerHouse(style) => {
            let main = matches!(structure.kind, StructureKind::MainHouse(_));
            house(&mut shape, style, main, (w, d, h), drop, v);
        }
        StructureKind::Barn => barn(&mut shape, (w, d, h), drop, v),
        _ => {}
    }
    shape
}

fn house(
    shape: &mut Shape,
    style: HouseStyle,
    main: bool,
    (w, d, h): (f32, f32, f32),
    drop: f32,
    v: u32,
) {
    let walls = |thickness: f32, z1: f32| Walls {
        x0: -w / 2.0,
        x1: w / 2.0,
        z0: -d / 2.0,
        z1,
        thickness,
        height: h,
        drop,
    };
    match style {
        HouseStyle::AdobeThatch | HouseStyle::AdobeIron => {
            // Adobe: thick walls, few and small openings, inset from the
            // footprint by the roof's overhang.
            let inset = if style == HouseStyle::AdobeThatch {
                0.55
            } else {
                0.35
            };
            let walls = Walls {
                x0: -w / 2.0 + inset,
                x1: w / 2.0 - inset,
                z0: -d / 2.0 + inset,
                z1: d / 2.0 - inset,
                ..walls(0.5, 0.0)
            };
            let len = walls.width();
            let rooms = if len > 8.0 { 3 } else { 2 };
            let door = if rooms == 3 { vec![1] } else { vec![0] };
            walls.front().build(
                shape,
                Surface::Adobe,
                &spread(len, rooms, &door, (0.95, 1.95), (0.7, 0.85, 1.65)),
                v,
            );
            walls.back().build(
                shape,
                Surface::Adobe,
                &spread(len, 1, &[], (0.0, 0.0), (0.6, 0.9, 1.5)),
                v,
            );
            for east in [true, false] {
                walls.side(east).build(shape, Surface::Adobe, &[], v);
            }
            if style == HouseStyle::AdobeThatch {
                // Steep paja, thick and low over the walls.
                let rise = walls.depth() / 2.0 * 0.95;
                gable_roof(
                    shape,
                    (Surface::Thatch, Surface::Adobe),
                    &walls,
                    rise,
                    inset,
                    0.3,
                );
            } else {
                lean_to(
                    shape,
                    (Surface::OldIron, Surface::Adobe),
                    &walls,
                    0.7,
                    inset,
                );
                chimney(shape, Vec3::new(walls.x1 - 1.0, h, walls.z0 + 0.8), h + 1.2);
            }
        }
        HouseStyle::BrickParapet => {
            let surface = if v & 0x100 == 0 {
                Surface::Whitewash
            } else {
                Surface::Brick
            };
            let walls = walls(0.4, d / 2.0);
            let len = walls.width();
            let count = ((len / 2.8) as usize).clamp(2, 6);
            let door = vec![count / 2];
            let (door_h, win) = if main {
                ((2.4_f32).min(h - 0.5), (1.0, 0.9, (2.3_f32).min(h - 0.6)))
            } else {
                ((2.1_f32).min(h - 0.4), (0.8, 0.9, (1.9_f32).min(h - 0.5)))
            };
            walls.front().build(
                shape,
                surface,
                &spread(len, count, &door, (1.0, door_h), win),
                v,
            );
            walls.back().build(
                shape,
                surface,
                &spread(len, (count / 2).max(1), &[0], (0.9, door_h), win),
                v >> 3,
            );
            for east in [true, false] {
                let side = walls.side(east);
                let n = (side.length / 4.0) as usize;
                side.build(
                    shape,
                    surface,
                    &spread(side.length, n, &[], (0.0, 0.0), win),
                    v,
                );
            }
            parapet_roof(shape, surface, &walls, 0.7);
            chimney(shape, Vec3::new(walls.x0 + 1.2, h, walls.z0 + 1.0), h + 1.6);
        }
        HouseStyle::GalleryHouse => homestead(shape, (w, d, h), drop, v),
    }
}

/// The Italianate casco: whitewashed, pilastered, a parapet with a
/// balustrade over each bay, and a gallery the whole front long.
fn homestead(shape: &mut Shape, (w, d, h): (f32, f32, f32), drop: f32, v: u32) {
    let gallery = 3.0;
    let walls = Walls {
        x0: -w / 2.0,
        x1: w / 2.0,
        z0: -d / 2.0,
        z1: d / 2.0 - gallery,
        thickness: 0.55,
        height: h,
        drop,
    };
    let len = walls.width();
    let bays = ((len / 3.2) as usize).clamp(5, 9) | 1;
    // Doors in the middle and every other bay; tall windows between.
    let doors: Vec<usize> = (0..bays).filter(|k| k % 2 == (bays / 2) % 2).collect();
    let front = spread(len, bays, &doors, (1.3, 3.0), (1.2, 0.7, 3.0));
    walls.front().build(shape, Surface::Whitewash, &front, v);
    walls.back().build(
        shape,
        Surface::Whitewash,
        &spread(len, bays, &[bays / 2], (1.1, 2.8), (1.1, 0.9, 2.9)),
        v >> 5,
    );
    for east in [true, false] {
        let side = walls.side(east);
        let n = ((side.length / 3.5) as usize).max(1);
        side.build(
            shape,
            Surface::Whitewash,
            &spread(side.length, n, &[], (0.0, 0.0), (1.1, 0.9, 2.9)),
            v,
        );
    }
    // Pilasters between the bays, from the plinth to the cornice.
    for k in 0..=bays {
        let x = walls.x0 + len * k as f32 / bays as f32;
        let x = x.clamp(walls.x0 + 0.25, walls.x1 - 0.25);
        shape.block(
            Surface::Whitewash,
            Vec3::new(x - 0.25, -drop, walls.z1),
            Vec3::new(x + 0.25, h, walls.z1 + 0.1),
        );
    }
    parapet_roof(shape, Surface::Whitewash, &walls, 0.9);
    // Balusters in the parapet over the middle bay: the casco's crest.
    let mid = walls.x0 + len / 2.0;
    for k in 0..9 {
        let x = mid - 1.2 + k as f32 * 0.3;
        shape.beam(
            Surface::Whitewash,
            Vec3::new(x, h + 0.9, walls.z1 + 0.02),
            Vec3::new(x, h + 1.5, walls.z1 + 0.02),
            0.12,
        );
    }
    shape.block(
        Surface::Whitewash,
        Vec3::new(mid - 1.5, h + 1.5, walls.z1 - 0.2),
        Vec3::new(mid + 1.5, h + 1.65, walls.z1 + 0.15),
    );
    // The gallery: a brick floor, a row of posts and an iron roof sloping
    // gently out from under the cornice.
    let (g0, g1) = (walls.z1, walls.z1 + gallery);
    shape.block(
        Surface::Brick,
        Vec3::new(walls.x0, -drop, g0),
        Vec3::new(walls.x1, 0.15, g1),
    );
    let iron_posts = v & 0x40 != 0;
    let posts = (len / 2.6) as usize + 1;
    let top = h - 0.45;
    for k in 0..posts {
        let x = walls.x0 + 0.2 + (len - 0.4) * k as f32 / (posts - 1) as f32;
        let at = Vec3::new(x, 0.15, g1 - 0.2);
        if iron_posts {
            shape.lathe(
                Surface::Steel,
                Transform::from_translation(at),
                &[
                    (0.09, 0.0),
                    (0.07, 0.25),
                    (0.06, top - 0.5),
                    (0.1, top - 0.35),
                    (0.1, top - 0.15),
                ],
                8,
            );
        } else {
            shape.beam(Surface::Planks, at, at.with_y(top - 0.15), 0.18);
        }
    }
    let low = Vec3::new(0.0, top - 0.15, g1 + 0.3);
    let high = Vec3::new(0.0, h - 0.25, g0);
    slab(shape, Surface::OldIron, low, high, len + 0.4, 0.06);
    // The fascia board along the gallery's edge.
    shape.block(
        Surface::Planks,
        Vec3::new(walls.x0 - 0.2, top - 0.4, g1 + 0.2),
        Vec3::new(walls.x1 + 0.2, top - 0.1, g1 + 0.3),
    );
    for x in [walls.x0 + 2.0, walls.x1 - 2.0] {
        chimney(shape, Vec3::new(x, h, walls.z0 + 1.5), h + 2.0);
    }
}

/// The galpón: corrugated iron on timber, a gable roof along its length,
/// and a pair of big plank doors in its front.
fn barn(shape: &mut Shape, (w, d, h): (f32, f32, f32), drop: f32, v: u32) {
    let walls = Walls {
        x0: -w / 2.0,
        x1: w / 2.0,
        z0: -d / 2.0,
        z1: d / 2.0,
        thickness: 0.08,
        height: h,
        drop,
    };
    let door = Opening {
        at: w / 2.0,
        width: (w * 0.3).clamp(4.0, 6.0),
        bottom: 0.0,
        top: (h - 0.8).min(4.0),
        door: true,
    };
    // A big door is mostly shut; one in three stands open on the dark.
    let open = u32::from(v.is_multiple_of(3));
    walls.front().build(shape, Surface::OldIron, &[door], open);
    walls.back().build(shape, Surface::OldIron, &[], 0);
    for east in [true, false] {
        walls.side(east).build(shape, Surface::OldIron, &[], 0);
    }
    // Timber corner posts and a plank skirting, the doors' frame.
    for (x, z) in [
        (walls.x0, walls.z0),
        (walls.x1, walls.z0),
        (walls.x0, walls.z1),
        (walls.x1, walls.z1),
    ] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, -drop, z),
            Vec3::new(x, h, z),
            0.2,
        );
    }
    let (a, b) = (door.at - door.width / 2.0, door.at + door.width / 2.0);
    for x in [walls.x0 + a, walls.x0 + b] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, -drop, walls.z1 + 0.05),
            Vec3::new(x, door.top + 0.15, walls.z1 + 0.05),
            0.18,
        );
    }
    shape.beam(
        Surface::Planks,
        Vec3::new(walls.x0 + a - 0.1, door.top + 0.1, walls.z1 + 0.05),
        Vec3::new(walls.x0 + b + 0.1, door.top + 0.1, walls.z1 + 0.05),
        0.18,
    );
    let rise = d / 2.0 * 0.35;
    gable_roof(
        shape,
        (Surface::OldIron, Surface::OldIron),
        &walls,
        rise,
        0.4,
        0.06,
    );
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
}
