//! The settlements' houses and galpones, built procedurally from the 1919
//! building list (`odd/setting-1919.md`) with the facade kit's walls and
//! roofs ([`crate::facade::walls`]).
//!
//! Every building is made in its own space: `x` along its front, `z` out
//! of its front (the front wall's outer face at `+depth / 2`), `y` up from
//! the ground at its middle. Walls run down to `-drop` so they meet the
//! ground on a slope.
//!
//! - The adobe rancho: thick mud-plastered walls under a steep, thick paja
//!   roof with a deep overhang, or under a single slope of old corrugated
//!   iron falling to the front, its walls built up to meet it.
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
use crate::facade::walls::{Walls, barn, chimney, gable_roof, lean_to, parapet_roof, slab, spread};

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
        StructureKind::Barn => barn(
            &mut shape,
            (Surface::OldIron, Surface::OldIron),
            (w, d, h),
            drop,
            v,
            1,
        ),
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
