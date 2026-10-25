//! The town's buildings on the facade kit: the 1919 Italianate fronts of a
//! young pampa railway town (`odd/setting-1919.md`).
//!
//! - Houses: one-storey whitewashed, lime-washed or bare-brick fronts on a
//!   plinth, with chamfered corner piers and pilasters, a moulded cornice
//!   and a parapet (solid or balustraded, some with urns or a small
//!   pediment) hiding a flat azotea or a corrugated roof; on the outskirts
//!   most are plainer adobe or whitewash ranchos under corrugated iron.
//! - The general store: a tall solid parapet behind its sign and a galería
//!   over the sidewalk on cast-iron posts; a small town's pulpería is
//!   whitewashed, barred and under a galería on timber posts.
//! - The comisaría: whitewashed, barred windows, a balustraded parapet with
//!   a pediment, and its flagpole.
//! - The church: a pedimented front and a bell tower with a belfry of round
//!   arches under a zinc spire.
//! - The station (until its own pass): brick with stucco dressings under a
//!   galvanised roof, and the platform canopy core gives the camera.
//! - Sheds and grain stores: corrugated-iron galpones; the water tank and
//!   the windmill are the settlements' molino and australian tank.
//!
//! Every masonry wall stands [`PROUD`] in from its footprint, so its
//! dressing stays inside the footprint core collides with and the facade
//! signs hang just outside of.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use pampa_core::{Building, BuildingKind, Canopy, Side, StoreStyle, Structure, StructureKind};

use super::italianate::{
    CHAMFER, Crown, PROUD, Posts, arch, corner_piers, cornice, door_leaves, gallery, grille,
    parapet, pediment, pilaster, plinth, spire, surround, urn,
};
use super::materials::Surface;
use super::shape::Shape;
use super::walls::{
    Opening, Wall, Walls, barn, chimney, flat_roof, gable_roof, lean_to, spread, wedge,
};
use crate::settlement::props;

/// How far walls run under the ground at a building's middle, meters.
pub const DROP: f32 = 0.3;

/// What a town building's look depends on beyond its core [`Building`].
#[derive(Debug, Clone, Copy)]
pub struct Lot {
    pub building: Building,
    /// A seeded variant: which wash, roof and dressing it gets.
    pub variant: u32,
    /// On the town's outer blocks, where houses are plainer.
    pub outskirts: bool,
    /// What the town's general store is.
    pub store: StoreStyle,
    /// How deep the general store's galería is (the hitching post's
    /// veranda).
    pub veranda: f32,
}

impl Lot {
    /// Width along its front and depth away from it.
    pub fn size(&self) -> Vec2 {
        let s = self.building.footprint.size();
        match self.building.front {
            Side::North | Side::South => Vec2::new(s.x, s.y),
            Side::East | Side::West => Vec2::new(s.y, s.x),
        }
    }

    /// The settlements' structure of the same size, for the molino and the
    /// tank they share with the towns.
    fn structure(&self, kind: StructureKind) -> Structure {
        Structure {
            kind,
            footprint: self.building.footprint,
            height: self.building.height,
            front: self.building.front,
            variant: self.variant,
        }
    }
}

/// The yaw that turns a building's own `+Z` toward its `front`.
pub fn yaw(front: Side) -> Quat {
    use std::f32::consts::PI;
    Quat::from_rotation_y(match front {
        Side::South => 0.0,
        Side::North => PI,
        Side::East => FRAC_PI_2,
        Side::West => -FRAC_PI_2,
    })
}

/// The building of `lot` in its own space (its windmill's wheel apart),
/// or `None` for the platform, which the town draws as a plain floor.
pub fn building(lot: &Lot) -> Option<Shape> {
    let size = lot.size();
    let (w, d, h) = (size.x, size.y, lot.building.height);
    let v = lot.variant;
    let mut shape = Shape::default();
    match lot.building.kind {
        BuildingKind::House => house(&mut shape, (w, d, h), v, lot.outskirts),
        BuildingKind::GeneralStore => store(&mut shape, (w, d, h), v, lot.store, lot.veranda),
        BuildingKind::PoliceStation => police(&mut shape, (w, d, h), v),
        BuildingKind::Church => church(&mut shape, (w, d, h), v),
        BuildingKind::Station => station(&mut shape, (w, d, h), v),
        BuildingKind::Shed | BuildingKind::GrainStore => {
            let iron = if v & 1 == 0 {
                Surface::OldIron
            } else {
                Surface::Galvanised
            };
            let doors = if lot.building.kind == BuildingKind::GrainStore {
                ((w / 14.0) as usize).clamp(1, 4)
            } else {
                1
            };
            barn(
                &mut shape,
                (iron, Surface::OldIron),
                (w, d, h),
                DROP,
                v,
                doors,
            );
        }
        BuildingKind::WaterTank => {
            shape = props::tank(&lot.structure(StructureKind::Tank), DROP);
        }
        BuildingKind::Windmill => {
            shape = props::windpump(&lot.structure(StructureKind::Windpump), DROP);
        }
        BuildingKind::Platform => return None,
    }
    Some(shape)
}

/// Where a windmill's wheel turns in its own space, and its yaw.
pub fn rotor_place(lot: &Lot) -> (Vec3, Quat) {
    props::rotor_place(&lot.structure(StructureKind::Windpump))
}

/// The walls of a masonry building `w` by `d`, [`PROUD`] in from its
/// footprint all round.
fn masonry((w, d, h): (f32, f32, f32), thickness: f32) -> Walls {
    Walls {
        x0: -w / 2.0 + PROUD,
        x1: w / 2.0 - PROUD,
        z0: -d / 2.0 + PROUD,
        z1: d / 2.0 - PROUD,
        thickness,
        height: h,
        drop: DROP,
    }
}

/// What lies behind a parapet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Roof {
    /// A flat brick azotea behind a parapet all round.
    Flat,
    /// Corrugated iron falling to the back behind a front parapet.
    Iron(Surface),
}

/// An Italianate front and the box behind it.
#[derive(Debug, Clone)]
struct Front {
    wall: Surface,
    /// Piers, pilasters, cornice, parapet and frames.
    trim: Surface,
    plinth: Surface,
    bays: usize,
    doors: Vec<usize>,
    /// Door width and height.
    door: (f32, f32),
    /// Window width, sill and head heights.
    window: (f32, f32, f32),
    parapet: f32,
    crown: Crown,
    /// A small pediment over the middle of the parapet.
    crest: bool,
    urns: bool,
    roof: Roof,
    grilles: bool,
    hoods: bool,
    transom: bool,
}

/// Builds `f` on `walls`: the walls with their openings, plinth, piers,
/// pilasters between the bays, cornice, parapet and roof.
fn italianate(shape: &mut Shape, walls: &Walls, f: &Front, v: u32) {
    let (h, t) = (walls.height, walls.thickness);
    let front = walls.front();
    let len = front.length;
    let openings = spread(len, f.bays, &f.doors, f.door, f.window);
    // Every door gets its own leaves below, so the wall leaves them open.
    front.build(shape, f.wall, &openings, u32::MAX);
    dress(shape, &front, &openings, f);
    let back_n = (f.bays / 2).max(1);
    let back = spread(len, back_n, &[0], (1.0, f.door.1.min(2.4)), f.window);
    walls.back().build(shape, f.wall, &back, v);
    plinth(shape, f.plinth, &walls.back(), &back, 0.6, 0.04);
    for east in [true, false] {
        let side = walls.side(east);
        let n = (side.length / 4.5) as usize;
        let windows = spread(side.length, n, &[], (0.0, 0.0), f.window);
        side.build(shape, f.wall, &windows, v);
        plinth(shape, f.plinth, &side, &windows, 0.6, 0.04);
    }
    plinth(shape, f.plinth, &front, &openings, 0.6, 0.05);
    let capital = h - 0.55;
    for k in 1..f.bays {
        let at = len * k as f32 / f.bays as f32;
        pilaster(
            shape,
            f.trim,
            &front,
            at,
            0.42,
            0.08,
            (-DROP, 0.75, capital),
        );
    }
    corner_piers(shape, f.trim, walls, 0.55, PROUD, (-DROP, capital));
    cornice(shape, f.trim, walls);
    // Pedestals over the corner piers and the pilasters.
    let mut pillars: Vec<f32> = (1..f.bays)
        .map(|k| len * k as f32 / f.bays as f32)
        .collect();
    // Over the corner piers, their caps kept inside the footprint.
    let end = 0.2;
    pillars.extend([end, len - end]);
    parapet(
        shape,
        f.trim,
        walls,
        f.parapet,
        (f.crown, f.roof == Roof::Flat),
        &pillars,
    );
    let top = h + f.parapet + 0.18;
    if f.urns {
        for at in [end, len - end] {
            urn(shape, f.trim, front.at(at, t * 0.3, top));
        }
    }
    if f.crest {
        let half = (len * 0.18).clamp(1.0, 2.2);
        let mid = (walls.x0 + walls.x1) / 2.0;
        pediment(
            shape,
            f.trim,
            (mid - half, mid + half),
            walls.z1,
            h + f.parapet + 0.08,
            0.55,
            t * 0.6,
        );
    }
    match f.roof {
        Roof::Flat => flat_roof(shape, walls),
        Roof::Iron(iron) => iron_behind(shape, (iron, f.wall), walls, f.parapet),
    }
}

/// Frames, leaves, grilles and hoods on `openings` of `wall`.
fn dress(shape: &mut Shape, wall: &Wall, openings: &[Opening], f: &Front) {
    for o in openings {
        if o.door {
            door_leaves(shape, wall, o, f.transom);
            surround(shape, f.trim, wall, o, false);
        } else {
            surround(shape, f.trim, wall, o, f.hoods);
            if f.grilles {
                grille(shape, wall, o);
            }
        }
    }
}

/// A corrugated roof hidden behind a front parapet `parapet` tall, falling
/// from just under its coping to past the back wall; the side walls built
/// up in `gable` to meet it.
fn iron_behind(shape: &mut Shape, (roof, gable): (Surface, Surface), walls: &Walls, parapet: f32) {
    let (h, t) = (walls.height, walls.thickness);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    let (high, low) = (h + parapet - 0.25, h + 0.05);
    let zf = z1 - t * 0.6;
    let slope = (high - low) / (zf - z0);
    let under = |z: f32| low + (z - z0) * slope;
    // Kept inside the footprint, the sheet's tilt included.
    let overhang = PROUD - 0.03;
    super::walls::slab(
        shape,
        roof,
        Vec3::new(0.0, under(z0 - overhang), z0 - overhang),
        Vec3::new(0.0, high, zf),
        walls.width() + 0.2,
        0.06,
    );
    for xs in [(x0, x0 + t), (x1 - t, x1)] {
        wedge(shape, gable, xs, (z0, zf), h, (under(z0), high));
    }
    wedge(
        shape,
        gable,
        (x0 + t, x1 - t),
        (z0, z0 + t),
        h,
        (under(z0), under(z0 + t)),
    );
}

/// A house: Italianate in town, mostly a plain rancho on the outskirts.
fn house(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32, outskirts: bool) {
    if outskirts && v % 20 < 13 {
        return cottage(shape, (w, d, h), v);
    }
    let wall = [
        Surface::Whitewash,
        Surface::Lime,
        Surface::Brick,
        Surface::Whitewash,
        Surface::Lime,
    ][(v % 5) as usize];
    let trim = if wall == Surface::Brick && v & 0x20 != 0 {
        Surface::Stucco
    } else {
        Surface::Whitewash
    };
    let walls = masonry((w, d, h), 0.4);
    let bays = ((walls.width() / 2.8) as usize).clamp(2, 5);
    // The zaguán door at one end, windows down the rest.
    let door = if v & 0x40 == 0 { 0 } else { bays - 1 };
    let head = (h - 0.85).min(2.7);
    let f = Front {
        wall,
        trim,
        plinth: Surface::Stucco,
        bays,
        doors: vec![door],
        door: (1.1, head),
        window: (0.9, 0.85, head),
        parapet: 0.85,
        crown: if v & 0x80 != 0 {
            Crown::Balustrade
        } else {
            Crown::Solid
        },
        crest: v & 0x300 == 0x300,
        urns: v & 0x400 != 0,
        roof: match (v >> 11) % 5 {
            0 => Roof::Iron(Surface::OldIron),
            1 => Roof::Iron(Surface::Galvanised),
            _ => Roof::Flat,
        },
        grilles: v & 0x2000 != 0,
        hoods: v & 0x4000 != 0,
        transom: true,
    };
    italianate(shape, &walls, &f, v);
}

/// An outskirts rancho: thick adobe or whitewashed walls set in under a
/// corrugated roof, a door and a window or two, and a chimney.
fn cottage(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let inset = 0.4;
    let walls = Walls {
        x0: -w / 2.0 + inset,
        x1: w / 2.0 - inset,
        z0: -d / 2.0 + inset,
        z1: d / 2.0 - inset,
        thickness: 0.5,
        height: (h - 0.6).max(2.8),
        drop: DROP,
    };
    let wall = if v & 1 == 0 {
        Surface::Adobe
    } else {
        Surface::Whitewash
    };
    let len = walls.width();
    let rooms = if len > 8.0 { 3 } else { 2 };
    let door = if rooms == 3 { vec![1] } else { vec![0] };
    walls.front().build(
        shape,
        wall,
        &spread(len, rooms, &door, (1.0, 2.0), (0.75, 0.85, 1.7)),
        v,
    );
    walls.back().build(
        shape,
        wall,
        &spread(len, 1, &[], (0.0, 0.0), (0.6, 0.9, 1.5)),
        v,
    );
    for east in [true, false] {
        walls.side(east).build(shape, wall, &[], v);
    }
    if v & 2 == 0 {
        lean_to(shape, (Surface::OldIron, wall), &walls, 0.7, inset - 0.05);
    } else {
        let rise = walls.depth() / 2.0 * 0.4;
        gable_roof(
            shape,
            (Surface::OldIron, wall),
            &walls,
            rise,
            inset - 0.05,
            0.06,
        );
    }
    let top = walls.height + 1.4;
    chimney(
        shape,
        Vec3::new(walls.x1 - 1.0, walls.height, walls.z0 + 0.8),
        top,
    );
}

/// The general store, or a small town's pulpería, with its galería.
fn store(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32, style: StoreStyle, veranda: f32) {
    let walls = masonry((w, d, h), 0.45);
    let bays = (((walls.width() / 3.2) as usize).clamp(3, 7)) | 1;
    // Doors in the middle and every other bay, shop windows between.
    let doors: Vec<usize> = (0..bays).filter(|k| k % 2 == (bays / 2) % 2).collect();
    let general = style == StoreStyle::GeneralStore;
    let wall = match (general, v % 3) {
        (true, 0) => Surface::Lime,
        (true, _) => Surface::Brick,
        (false, 0) => Surface::Lime,
        (false, _) => Surface::Whitewash,
    };
    let f = Front {
        wall,
        // Whitewashed mouldings and parapet over the brick or the wash.
        trim: Surface::Whitewash,
        plinth: Surface::Stucco,
        bays,
        doors,
        door: (1.3, (h - 1.3).min(3.4)),
        window: (1.3, 0.7, (h - 1.3).min(3.4)),
        // Tall and solid: the store's sign hangs on it.
        parapet: 2.3,
        crown: Crown::Solid,
        crest: general,
        urns: general,
        roof: if v & 8 == 0 {
            Roof::Flat
        } else {
            Roof::Iron(Surface::OldIron)
        },
        grilles: !general,
        hoods: general,
        transom: true,
    };
    italianate(shape, &walls, &f, v);
    let (roof, posts) = if general {
        (Surface::Galvanised, Posts::Iron)
    } else {
        (Surface::OldIron, Posts::Timber)
    };
    // From under the cornice, out over the sidewalk to the footprint's
    // street edge plus the veranda.
    let mut over = walls.front();
    over.start.x -= PROUD;
    over.length += 2.0 * PROUD;
    let high = h - 0.7;
    let depth = veranda + PROUD;
    gallery(shape, roof, posts, &over, depth, (high, high - 0.3 * depth));
}

/// The comisaría: whitewashed, barred, balustraded, with its flagpole.
fn police(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let walls = masonry((w, d, h), 0.45);
    let bays = (((walls.width() / 2.9) as usize).clamp(3, 7)) | 1;
    let f = Front {
        wall: Surface::Whitewash,
        trim: Surface::Whitewash,
        plinth: Surface::Stucco,
        bays,
        doors: vec![bays / 2],
        // Low enough for the sign over the door.
        door: (1.4, 2.5),
        window: (1.0, 0.9, 2.5),
        parapet: 1.0,
        crown: Crown::Balustrade,
        crest: true,
        urns: true,
        roof: Roof::Flat,
        grilles: true,
        hoods: false,
        transom: true,
    };
    italianate(shape, &walls, &f, v);
    // The flagpole on the flat_roof, by a front corner, with its finial.
    let foot = Vec3::new(walls.x1 - 1.0, h - 0.1, walls.z1 - 1.0);
    let top = h + 5.5;
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(foot),
        &[(0.06, 0.0), (0.04, top - foot.y), (0.0, top - foot.y)],
        6,
    );
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(foot.with_y(top)),
        &[(0.0, 0.0), (0.08, 0.06), (0.08, 0.12), (0.0, 0.18)],
        6,
    );
}

/// How far the church's tower stands out of its nave's front, meters.
const TOWER_OUT: f32 = 0.7;

/// The church: a pedimented nave with arched windows and a bell tower in
/// the middle of its front.
fn church(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    // The nave stands back behind its tower, which reaches the footprint.
    let mut walls = masonry((w, d, h), 0.6);
    walls.z1 -= TOWER_OUT;
    let (wall, trim) = (Surface::Whitewash, Surface::Whitewash);
    let tower = 4.0_f32.min(walls.width() - 2.0);
    let len = walls.width();
    // The nave front: an arched window either side of the tower.
    let side = (len - tower) / 2.0;
    let mut front_openings = Vec::new();
    if side > 2.4 {
        for at in [side / 2.0, len - side / 2.0] {
            front_openings.push(Opening {
                at,
                width: 1.1,
                bottom: 1.6,
                top: (h - 1.4).min(5.0),
                // Glazed, unshuttered: built as an open doorway.
                door: true,
            });
        }
    }
    let front = walls.front();
    front.build(shape, wall, &front_openings, u32::MAX);
    for o in &front_openings {
        arch(shape, (wall, trim), &front, o);
    }
    plinth(shape, Surface::Stucco, &front, &front_openings, 0.7, 0.05);
    walls.back().build(shape, wall, &[], 0);
    plinth(shape, Surface::Stucco, &walls.back(), &[], 0.7, 0.04);
    for east in [true, false] {
        let side = walls.side(east);
        let n = ((side.length / 4.0) as usize).max(1);
        let windows: Vec<Opening> = (0..n)
            .map(|k| Opening {
                at: side.length * (k as f32 + 0.5) / n as f32,
                width: 1.1,
                bottom: 2.0,
                top: (h - 1.2).min(5.4),
                // Glazed, unshuttered: built as an open doorway.
                door: true,
            })
            .collect();
        side.build(shape, wall, &windows, u32::MAX);
        for o in &windows {
            arch(shape, (wall, trim), &side, o);
        }
        plinth(shape, Surface::Stucco, &side, &windows, 0.7, 0.04);
        // Buttressing pilasters between the windows.
        for k in 0..=n {
            let at = (side.length * k as f32 / n as f32).clamp(0.3, side.length - 0.3);
            pilaster(shape, trim, &side, at, 0.5, 0.09, (-DROP, 0.85, h - 0.55));
        }
    }
    corner_piers(shape, trim, &walls, 0.7, PROUD, (-DROP, h - 0.55));
    cornice(shape, trim, &walls);
    // The nave's gable roof runs back from the front: built along X, then
    // turned so its ridge runs along Z.
    let rise = len / 2.0 * 0.55;
    let turned = Walls {
        x0: walls.z0,
        x1: walls.z1,
        z0: walls.x0,
        z1: walls.x1,
        ..walls
    };
    let mut roof = Shape::default();
    gable_roof(
        &mut roof,
        (Surface::OldIron, wall),
        &turned,
        rise,
        0.05,
        0.08,
    );
    shape.merge(
        roof,
        Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
    );
    // The pediment over the front gable.
    pediment(
        shape,
        trim,
        (walls.x0, walls.x1),
        walls.z1 + PROUD - 0.06,
        h + 0.04,
        rise + 0.1,
        0.3,
    );
    bell_tower(shape, &walls, tower, rise + 0.5, v);
}

/// The church's bell tower, `size` square, standing [`TOWER_OUT`] out of
/// the `nave` front and rising `over` its cornice: a door under a round
/// arch, a belfry of round arches, a drum and a zinc spire with an iron
/// cross.
fn bell_tower(shape: &mut Shape, nave: &Walls, size: f32, over: f32, _v: u32) {
    let (wall, trim) = (Surface::Whitewash, Surface::Whitewash);
    let h = nave.height;
    let front = nave.z1 + TOWER_OUT;
    let shaft = Walls {
        x0: -size / 2.0,
        x1: size / 2.0,
        z0: front - size,
        z1: front,
        thickness: 0.5,
        height: h + over,
        drop: DROP,
    };
    let door = Opening {
        at: size / 2.0,
        width: 1.6,
        bottom: 0.0,
        top: 3.4,
        door: true,
    };
    shaft.front().build(shape, wall, &[door], u32::MAX);
    let square = Opening { top: 2.6, ..door };
    door_leaves(shape, &shaft.front(), &square, false);
    arch(shape, (wall, trim), &shaft.front(), &door);
    plinth(shape, Surface::Stucco, &shaft.front(), &[door], 0.7, PROUD);
    for east in [true, false] {
        shaft.side(east).build(shape, wall, &[], 0);
    }
    // Chamfered piers up the shaft's corners, a cornice over it.
    corner_piers(shape, trim, &shaft, 0.45, PROUD, (-DROP, h + over));
    let mut level = shaft;
    level.height = h + over;
    cornice(shape, trim, &level);
    // The belfry: four walls of round arches on the shaft.
    let base = h + over;
    let belfry = Walls {
        x0: -size / 2.0 + 0.15,
        x1: size / 2.0 - 0.15,
        z0: front - size + 0.15,
        z1: front - 0.15,
        thickness: 0.45,
        height: 3.6,
        drop: 0.0,
    };
    let lift = |mut wall: Wall| {
        wall.start.y = base;
        wall
    };
    let open = Opening {
        at: belfry.width() / 2.0,
        width: 1.3,
        bottom: 0.6,
        top: 3.0,
        // Open, unshuttered: built as a doorway left open.
        door: true,
    };
    for face in [
        belfry.front(),
        belfry.back(),
        belfry.side(true),
        belfry.side(false),
    ] {
        let face = lift(face);
        let o = Opening {
            at: face.length / 2.0,
            ..open
        };
        face.build(shape, wall, &[o], u32::MAX);
        arch(shape, (wall, trim), &face, &o);
    }
    let mut top = belfry;
    top.height = base + 3.6;
    corner_piers(shape, trim, &top, 0.4, 0.05, (base, base + 3.6));
    cornice(shape, trim, &top);
    // A square drum, then the spire and its cross.
    let drum = size - 0.9;
    let y = base + 3.6 + 0.04;
    shape.chamfered(
        wall,
        Vec3::new(-drum / 2.0, y, front - size / 2.0 - drum / 2.0),
        Vec3::new(drum / 2.0, y + 0.9, front - size / 2.0 + drum / 2.0),
        CHAMFER * 3.0,
    );
    let centre = Vec3::new(0.0, y + 0.9, front - size / 2.0);
    shape.block(
        trim,
        centre - Vec3::new(drum / 2.0 + 0.1, 0.0, drum / 2.0 + 0.1),
        centre + Vec3::new(drum / 2.0 + 0.1, 0.12, drum / 2.0 + 0.1),
    );
    let spire_h = size * 1.1;
    spire(
        shape,
        Surface::Sheet,
        centre + Vec3::Y * 0.12,
        drum,
        spire_h,
    );
    let tip = centre + Vec3::Y * (0.12 + spire_h);
    shape.beam(
        Surface::Steel,
        tip - Vec3::Y * 0.1,
        tip + Vec3::Y * 1.3,
        0.08,
    );
    shape.beam(
        Surface::Steel,
        tip + Vec3::new(-0.35, 0.9, 0.0),
        tip + Vec3::new(0.35, 0.9, 0.0),
        0.08,
    );
}

/// The station: brick with stucco dressings, a galvanised gable roof, and
/// the platform canopy on the track side exactly where core puts it.
fn station(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let walls = masonry((w, d, h), 0.45);
    let bays = ((walls.width() / 3.0) as usize).clamp(3, 9) | 1;
    let f = Front {
        wall: Surface::Brick,
        trim: Surface::Stucco,
        plinth: Surface::Stucco,
        bays,
        doors: (0..bays).filter(|k| k % 2 == 1).collect(),
        door: (1.3, 2.8),
        window: (1.1, 0.9, 2.8),
        parapet: 0.0,
        crown: Crown::Solid,
        crest: false,
        urns: false,
        roof: Roof::Flat,
        grilles: false,
        hoods: true,
        transom: true,
    };
    let len = walls.width();
    let front = walls.front();
    let openings = spread(len, f.bays, &f.doors, f.door, f.window);
    front.build(shape, f.wall, &openings, u32::MAX);
    dress(shape, &front, &openings, &f);
    plinth(shape, f.plinth, &front, &openings, 0.6, 0.05);
    // The track side mirrors the town side.
    let back = walls.back();
    back.build(shape, f.wall, &openings, u32::MAX);
    dress(shape, &back, &openings, &f);
    plinth(shape, f.plinth, &back, &openings, 0.6, 0.05);
    for east in [true, false] {
        let side = walls.side(east);
        let windows = spread(side.length, 2, &[], (0.0, 0.0), f.window);
        side.build(shape, f.wall, &windows, v);
        dress(shape, &side, &windows, &f);
        plinth(shape, f.plinth, &side, &windows, 0.6, 0.04);
    }
    corner_piers(shape, f.trim, &walls, 0.55, PROUD, (-DROP, h - 0.55));
    cornice(shape, f.trim, &walls);
    gable_roof(
        shape,
        (Surface::Galvanised, Surface::Brick),
        &walls,
        2.2,
        0.5,
        0.08,
    );
    chimney(shape, Vec3::new(walls.x0 + 2.0, h, 0.0), h + 2.4);
    chimney(shape, Vec3::new(walls.x1 - 2.0, h, 0.0), h + 2.4);
    // Core's `TownLayout::canopies` gives the camera this same roof.
    let (bottom, top) = (
        Canopy::HEIGHT - Canopy::THICKNESS / 2.0,
        Canopy::HEIGHT + Canopy::THICKNESS / 2.0,
    );
    let (z0, z1) = (-d / 2.0 - Canopy::DEPTH, -d / 2.0);
    shape.cuboid(
        Surface::Galvanised,
        Transform::from_translation(Vec3::new(0.0, (bottom + top) / 2.0, (z0 + z1) / 2.0))
            .with_scale(Vec3::new(w, top - bottom, z1 - z0)),
        true,
    );
    let n = ((w / 4.0) as usize).max(1);
    for k in 0..=n {
        let x = -w / 2.0 + 0.3 + (w - 0.6) * k as f32 / n as f32;
        shape.lathe(
            Surface::Steel,
            Transform::from_xyz(x, 0.0, z0 + 0.3),
            &[
                (0.1, -DROP),
                (0.07, 0.4),
                (0.06, bottom - 0.3),
                (0.11, bottom),
            ],
            8,
        );
    }
}

#[cfg(test)]
mod tests;
