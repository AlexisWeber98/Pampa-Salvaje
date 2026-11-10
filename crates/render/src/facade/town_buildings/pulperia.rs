//! La sala de la pulpería de un pueblo pequeño, la que se ve y se recorre al
//! entrar por la puerta del medio del frente: un piso de ladrillo, un
//! cielorraso de tablas sobre tirantes, el mostrador con su reja de hierro
//! hasta el techo, la estantería del fondo con botellas, ginebras cuadradas,
//! cajones y damajuanas forradas de paja, dos barriles junto al mostrador,
//! tres mesas con sus bancos y un farol de querosén colgado en el medio.
//!
//! Cada mueble está donde core lo pone ([`Interior::of`]), así que la sala
//! dibujada y la que choca no se separan nunca. Es una forma aparte, sin el
//! desgaste del edificio: town la suma a las mallas de la pulpería.
//!
//! Lo que cambia mientras se juega va aparte, porque las mallas del edificio
//! se construyen una sola vez fuera del hilo principal: las dos hojas de la
//! puerta del medio ([`leaves`]), que se abren de día y se cierran de noche,
//! y el tubo de vidrio del farol, que brilla con su llama ([`lamp_chimney`],
//! [`lamp_flame`]). Las mueve y las enciende [`crate::pulperia`].

use bevy::prelude::*;
use pampa_core::pulperia::{FurnitureKind, Interior};
use pampa_core::{Aabb, BuildingKind, StoreStyle};

use super::super::italianate::{FANLIGHT, leaf_spans, panelled_leaf};
use super::super::materials::{Livery, Surface};
use super::super::shape::{Shape, Weather};
use super::{Lot, damp, paints_its_joinery, store_middle_door, streaks, wear, yaw};
use crate::coords::world_from_ground;

/// A qué altura sobre la base del edificio está el piso de ladrillo de la
/// sala, en metros: apenas sobre la vereda y bajo el umbral. Las alturas de
/// los muebles de core se miden desde aquí.
pub(crate) const ROOM_FLOOR: f32 = 0.05;

/// Cuánto más bajo que la pared queda el cielorraso, en metros.
const CEILING_BELOW: f32 = 0.1;

/// La madera de los muebles: curtida y sin pintar.
const WOOD: Surface = Surface::Sleeper;

/// Un rectángulo sobre el piso en el espacio propio del edificio.
#[derive(Debug, Clone, Copy)]
struct Rect {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
}

impl Rect {
    fn center(&self) -> (f32, f32) {
        ((self.x0 + self.x1) / 2.0, (self.z0 + self.z1) / 2.0)
    }
}

/// La sala amueblada de la pulpería de `lot` en su propio espacio (el frente
/// en `+Z`), o `None` si `lot` no es una pulpería.
pub(crate) fn interior(lot: &Lot) -> Option<Shape> {
    let room = room(lot)?;
    let local = |a: Aabb| local_rect(lot, a);
    let floor = local(room.floor);
    let ceiling = room.ceiling - CEILING_BELOW;
    let mut shape = Shape::default();
    floor_and_ceiling(&mut shape, floor, ceiling);
    let mut barrels = 0;
    for f in &room.furniture {
        let r = local(f.footprint);
        let top = ROOM_FLOOR + f.height;
        match f.kind {
            FurnitureKind::Counter => counter(&mut shape, r, top, ceiling),
            FurnitureKind::Shelves => shelves(&mut shape, r, top, lot.variant),
            FurnitureKind::Barrel => {
                barrel(&mut shape, r, top, barrels == 0);
                barrels += 1;
            }
            FurnitureKind::Table => table(&mut shape, r, top),
            FurnitureKind::Bench => bench(&mut shape, r, top),
        }
    }
    kerosene_lamp(&mut shape, lamp_at(lot, &room), ceiling);
    Some(shape)
}

/// La sala de core de `lot`, si es una pulpería (el almacén de un pueblo
/// pequeño).
fn room(lot: &Lot) -> Option<Interior> {
    (lot.building.kind == BuildingKind::GeneralStore && lot.store == StoreStyle::Pulperia)
        .then(|| Interior::of(&lot.building))
}

/// Dónde arde la mecha del farol que cuelga en el medio de `room`, la sala
/// de `lot`, en el espacio propio del edificio.
fn lamp_at(lot: &Lot, room: &Interior) -> Vec3 {
    let (x, z) = local_rect(lot, room.floor).center();
    Vec3::new(x, room.ceiling - CEILING_BELOW - LAMP_DROP, z)
}

/// Dónde arde la mecha del farol de la pulpería de `lot`, en el espacio
/// propio del edificio, o `None` si `lot` no es una pulpería.
pub(crate) fn lamp_flame(lot: &Lot) -> Option<Vec3> {
    room(lot).map(|room| lamp_at(lot, &room))
}

/// Una hoja de la puerta del medio de la pulpería, cerrada, en el espacio
/// propio del edificio, y cómo gira para abrirse.
pub(crate) struct Leaf {
    /// La hoja de tableros, gastada como el resto del frente.
    pub shape: Shape,
    pub hinge: Hinge,
}

/// Cómo gira una hoja de puerta sobre sus bisagras.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hinge {
    /// La arista de atrás de la hoja junto a su jamba, al pie, en el espacio
    /// propio del edificio.
    pub at: Vec3,
    /// Cuánto gira para abrirse del todo, en radianes alrededor de `+Y`: un
    /// cuarto de vuelta hacia adentro de la sala.
    pub swing: f32,
}

impl Hinge {
    /// Cómo se mueve la hoja cerrada cuando está abierta en la fracción
    /// `open` (0, cerrada en su vano; 1, abierta de canto contra el derrame
    /// de su jamba), en el espacio propio del edificio.
    pub fn pose(&self, open: f32) -> Transform {
        let turn = Quat::from_rotation_y(self.swing * open.clamp(0.0, 1.0));
        Transform::from_translation(self.at - turn * self.at).with_rotation(turn)
    }
}

/// Las dos hojas de tableros de la puerta del medio de la pulpería de `lot`,
/// cerradas, o `None` si `lot` no es una pulpería. Son las mismas que las de
/// las otras puertas del frente ([`panelled_leaf`]) y cubren el vano desde el
/// umbral hasta la banderola fija, con el mismo desgaste que el edificio. Se
/// abren hacia adentro: la de la jamba izquierda (`-X`) gira hacia `+Y` y la
/// de la derecha al revés, hasta quedar de canto contra su jamba.
pub(crate) fn leaves(lot: &Lot) -> Option<[Leaf; 2]> {
    room(lot)?;
    let (front, door) = store_middle_door(lot);
    let top = door.top - FANLIGHT;
    // El dorso de la hoja, donde van las bisagras (ver `panelled_leaf`).
    let back = front.thickness * 0.5;
    let [left, right] = leaf_spans(&door);
    let leaf = |span: (f32, f32), jamb: f32, swing: f32| {
        let mut shape = Shape::default();
        shape.set_louvred(paints_its_joinery(lot));
        shape.set_streaks(streaks(lot));
        panelled_leaf(&mut shape, &front, span, (door.bottom, top));
        if let Some(wear) = wear(lot) {
            shape.weather(Weather {
                wear,
                damp: damp(lot),
            });
        }
        Leaf {
            shape,
            hinge: Hinge {
                at: front.at(jamb, back, 0.0),
                swing,
            },
        }
    };
    Some([
        leaf(left, left.0, std::f32::consts::FRAC_PI_2),
        leaf(right, right.1, -std::f32::consts::FRAC_PI_2),
    ])
}

/// El rectángulo `a` de core (en el suelo del mundo) en el espacio propio del
/// edificio de `lot`, con su centro en el origen y el frente en `+Z`.
fn local_rect(lot: &Lot, a: Aabb) -> Rect {
    let c = lot.building.footprint.center();
    let turn = yaw(lot.building.front).inverse();
    let p = turn * world_from_ground(a.min - c, 0.0);
    let q = turn * world_from_ground(a.max - c, 0.0);
    Rect {
        x0: p.x.min(q.x),
        x1: p.x.max(q.x),
        z0: p.z.min(q.z),
        z1: p.z.max(q.z),
    }
}

/// Una caja de `min` a `max` con todas sus caras.
fn solid(shape: &mut Shape, surface: Surface, min: Vec3, max: Vec3) {
    shape.cuboid(
        surface,
        Transform::from_translation((min + max) / 2.0).with_scale(max - min),
        true,
    );
}

/// Una cara horizontal sobre `r` a la altura `y`, mirando hacia arriba o hacia
/// abajo.
fn level(shape: &mut Shape, surface: Surface, r: Rect, y: f32, up: bool) {
    let mut corners = [
        Vec3::new(r.x0, y, r.z0),
        Vec3::new(r.x0, y, r.z1),
        Vec3::new(r.x1, y, r.z1),
        Vec3::new(r.x1, y, r.z0),
    ];
    if !up {
        corners.reverse();
    }
    shape.face(surface, &corners);
}

/// Una tabla gruesa de `r`, de `top - thick` a `top`, con las aristas
/// verticales biseladas y su cara de abajo.
fn board(shape: &mut Shape, surface: Surface, r: Rect, top: f32, thick: f32) {
    shape.chamfered(
        surface,
        Vec3::new(r.x0, top - thick, r.z0),
        Vec3::new(r.x1, top, r.z1),
        0.02,
    );
    level(shape, surface, r, top - thick, false);
}

/// El piso de ladrillo y el cielorraso de tablas sobre sus tirantes, de
/// frente a fondo.
fn floor_and_ceiling(shape: &mut Shape, r: Rect, ceiling: f32) {
    level(shape, Surface::Paving, r, ROOM_FLOOR, true);
    level(shape, Surface::Planks, r, ceiling, false);
    let n = ((r.x1 - r.x0) / 1.1).round().max(1.0) as usize;
    let step = (r.x1 - r.x0) / n as f32;
    for k in 1..n {
        let x = r.x0 + step * k as f32;
        let y = ceiling - 0.07;
        shape.beam(WOOD, Vec3::new(x, y, r.z0), Vec3::new(x, y, r.z1), 0.13);
    }
}

/// El mostrador sobre `r`, del lado de la calle en `z1`: un zócalo oscuro, el
/// frente de madera pintada con sus montantes y travesaños, la tapa de tablón
/// que sobresale hacia los parroquianos, y encima la reja de hierro hasta el
/// cielorraso, con su ventanilla para pasar la mercadería.
fn counter(shape: &mut Shape, r: Rect, top: f32, ceiling: f32) {
    let y0 = ROOM_FLOOR;
    let base = y0 + 0.12;
    let under = top - 0.05;
    solid(
        shape,
        Surface::Barnwood,
        Vec3::new(r.x0, y0, r.z0 + 0.06),
        Vec3::new(r.x1 - 0.06, base, r.z1 - 0.05),
    );
    solid(
        shape,
        Surface::Door,
        Vec3::new(r.x0, base, r.z0 + 0.04),
        Vec3::new(r.x1 - 0.03, under, r.z1 - 0.02),
    );
    // Los travesaños y los montantes del frente, en relieve sobre él.
    let face = r.z1 - 0.02;
    for (a, b) in [(base, base + 0.1), (under - 0.1, under)] {
        solid(
            shape,
            Surface::Door,
            Vec3::new(r.x0, a, face),
            Vec3::new(r.x1 - 0.03, b, face + 0.02),
        );
    }
    let n = ((r.x1 - r.x0) / 0.85).round().max(1.0) as usize;
    let step = (r.x1 - 0.03 - r.x0) / n as f32;
    for k in 1..n {
        let x = r.x0 + step * k as f32;
        solid(
            shape,
            Surface::Door,
            Vec3::new(x - 0.035, base + 0.1, face),
            Vec3::new(x + 0.035, under - 0.1, face + 0.02),
        );
    }
    board(
        shape,
        Surface::Planks,
        Rect {
            x0: r.x0,
            x1: r.x1 + 0.02,
            z0: r.z0 + 0.02,
            z1: r.z1 + 0.03,
        },
        top,
        0.05,
    );
    // La reja: barrotes cuadrados entre planchuelas, sobre el canto de
    // adelante del mostrador, y una ventanilla cerca del paso del pulpero.
    let z = r.z1 - 0.15;
    let (lo, hi) = (r.x0 + 0.05, r.x1 - 0.05);
    let window = (r.x1 - 1.55, r.x1 - 0.85);
    let sill = top + 0.45;
    let bars = ((hi - lo) / 0.13).round().max(1.0) as usize;
    for k in 0..=bars {
        let x = lo + (hi - lo) * k as f32 / bars as f32;
        let from = if x > window.0 && x < window.1 {
            sill
        } else {
            top
        };
        shape.beam(
            Surface::Steel,
            Vec3::new(x, from, z),
            Vec3::new(x, ceiling - 0.01, z),
            0.018,
        );
    }
    for y in [top + 0.03, sill, (sill + ceiling) / 2.0, ceiling - 0.25] {
        shape.beam(
            Surface::Steel,
            Vec3::new(lo - 0.03, y, z),
            Vec3::new(hi + 0.03, y, z),
            0.035,
        );
    }
    for x in [lo, hi] {
        shape.beam(
            Surface::Steel,
            Vec3::new(x, top, z),
            Vec3::new(x, ceiling, z),
            0.05,
        );
    }
}

/// Un número de 0 a 1 de `a`, `b` y la semilla `v`.
fn pick(a: u32, b: u32, v: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b) ^ v;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h >> 8) as f32 / 16_777_216.0
}

/// Las alturas de los estantes sobre el piso, con la de arriba enrasada con
/// la altura que core le da a la estantería.
const SHELF_BOARDS: [f32; 6] = [0.12, 0.7, 1.12, 1.54, 1.96, 2.375];

/// El espesor de cada estante.
const SHELF_THICK: f32 = 0.025;

/// La estantería contra la pared del fondo (`z0`), de `top` de alto:
/// parantes cada 1,2 m, seis estantes y, según la semilla `v`, damajuanas
/// forradas de paja abajo, cajones en el segundo estante y botellas en los
/// del medio.
fn shelves(shape: &mut Shape, r: Rect, top: f32, v: u32) {
    let y0 = ROOM_FLOOR;
    let bays = ((r.x1 - r.x0) / 1.2).round().max(1.0) as usize;
    let step = (r.x1 - r.x0) / bays as f32;
    solid(
        shape,
        Surface::Barnwood,
        Vec3::new(r.x0, y0, r.z0 + 0.02),
        Vec3::new(r.x1, y0 + SHELF_BOARDS[0], r.z1 - 0.02),
    );
    for k in 0..=bays {
        let x = (r.x0 + step * k as f32).clamp(r.x0 + 0.0125, r.x1 - 0.0125);
        solid(
            shape,
            WOOD,
            Vec3::new(x - 0.0125, y0, r.z0),
            Vec3::new(x + 0.0125, top, r.z1),
        );
    }
    for y in SHELF_BOARDS {
        let y = y0 + y;
        solid(
            shape,
            Surface::Planks,
            Vec3::new(r.x0, y, r.z0),
            Vec3::new(r.x1, (y + SHELF_THICK).min(top), r.z1),
        );
    }
    let z = (r.z0 + r.z1) / 2.0;
    for bay in 0..bays {
        let (a, b) = (
            r.x0 + step * bay as f32 + 0.04,
            r.x0 + step * (bay + 1) as f32 - 0.04,
        );
        let bay = bay as u32;
        // Abajo, una damajuana de vez en cuando.
        if pick(bay, 0, v) < 0.4 {
            let x = a + 0.2 + pick(bay, 1, v) * (b - a - 0.4).max(0.0);
            demijohn(shape, Vec3::new(x, y0 + SHELF_BOARDS[0] + SHELF_THICK, z));
        }
        // En el segundo estante, un cajón de madera.
        if pick(bay, 2, v) < 0.5 {
            let x = a + 0.2 + pick(bay, 3, v) * (b - a - 0.4).max(0.0);
            let y = y0 + SHELF_BOARDS[1] + SHELF_THICK;
            solid(
                shape,
                Surface::Planks,
                Vec3::new(x - 0.18, y, z - 0.16),
                Vec3::new(x + 0.18, y + 0.26, z + 0.16),
            );
        }
        // Grupos de botellas en los tres estantes del medio.
        for (row, &board_y) in SHELF_BOARDS[2..5].iter().enumerate() {
            let seed = 10 + row as u32 * 4;
            if pick(bay, seed, v) < 0.3 {
                continue;
            }
            let n = 2 + (pick(bay, seed + 1, v) * 4.0) as usize;
            let gap = 0.09;
            let spare = (b - a - 0.1 - gap * (n - 1) as f32).max(0.0);
            let start = a + 0.05 + pick(bay, seed + 2, v) * spare;
            let gin = pick(bay, seed + 3, v) < 0.35;
            let y = y0 + board_y + SHELF_THICK;
            for k in 0..n {
                let at = Vec3::new(start + gap * k as f32, y, z);
                if gin {
                    gin_bottle(shape, at);
                } else {
                    bottle(shape, at);
                }
            }
        }
    }
}

/// Una botella de vino de vidrio oscuro parada en `at`.
fn bottle(shape: &mut Shape, at: Vec3) {
    shape.lathe(
        Surface::Glass,
        Transform::from_translation(at),
        &[(0.036, 0.0), (0.036, 0.2), (0.015, 0.25), (0.013, 0.3)],
        6,
    );
}

/// Una ginebra en su botella cuadrada de vidrio negro, con el pico redondo.
fn gin_bottle(shape: &mut Shape, at: Vec3) {
    shape.chamfered(
        Surface::Livery(Livery::Black),
        at + Vec3::new(-0.034, 0.0, -0.034),
        at + Vec3::new(0.034, 0.22, 0.034),
        0.012,
    );
    shape.lathe(
        Surface::Livery(Livery::Black),
        Transform::from_translation(at + Vec3::Y * 0.22),
        &[(0.014, 0.0), (0.011, 0.06)],
        6,
    );
}

/// Una damajuana forrada de paja trenzada, con el cuello de vidrio, parada en
/// `at`.
fn demijohn(shape: &mut Shape, at: Vec3) {
    let place = Transform::from_translation(at);
    shape.lathe(
        Surface::Thatch,
        place,
        &[
            (0.12, 0.0),
            (0.17, 0.07),
            (0.18, 0.18),
            (0.15, 0.29),
            (0.07, 0.35),
        ],
        8,
    );
    shape.lathe(
        Surface::Glass,
        place,
        &[(0.07, 0.35), (0.035, 0.38), (0.03, 0.44)],
        8,
    );
}

/// El radio de un barril de `height` de alto a la altura `y`: 25 cm en las
/// cabezas y 29 cm en la panza.
fn barrel_radius(y: f32, height: f32) -> f32 {
    let t = (2.0 * y / height - 1.0).clamp(-1.0, 1.0);
    0.25 + 0.04 * (1.0 - t * t)
}

/// Un barril de duelas parado en el centro de `r`, de `top` de alto, con la
/// tapa hundida en su gargol, cuatro zunchos de hierro y, si `tap`, una
/// canilla de bronce hacia la calle.
fn barrel(shape: &mut Shape, r: Rect, top: f32, tap: bool) {
    let (x, z) = r.center();
    let height = top - ROOM_FLOOR;
    let place = Transform::from_xyz(x, ROOM_FLOOR, z);
    let mut staves: Vec<(f32, f32)> = (0..=6)
        .map(|k| {
            let y = height * k as f32 / 6.0;
            (barrel_radius(y, height), y)
        })
        .collect();
    // El canto de las duelas, el gargol y la tapa, con aristas vivas.
    let (rim, lid) = (barrel_radius(height, height), height - 0.03);
    staves.extend([
        (rim, height),
        (rim - 0.018, height),
        (rim - 0.018, height),
        (rim - 0.018, lid),
        (rim - 0.018, lid),
        (0.0, lid),
    ]);
    shape.lathe(WOOD, place, &staves, 16);
    for y in [0.06, 0.22, height - 0.22, height - 0.06] {
        let r = barrel_radius(y, height) + 0.005;
        shape.lathe(Surface::Steel, place, &[(r, y - 0.022), (r, y + 0.022)], 16);
    }
    if tap {
        let y = 0.16;
        let out = barrel_radius(y, height);
        shape.beam(
            Surface::Bronze,
            Vec3::new(x, ROOM_FLOOR + y, z + out - 0.02),
            Vec3::new(x, ROOM_FLOOR + y, z + out + 0.07),
            0.025,
        );
        shape.beam(
            Surface::Bronze,
            Vec3::new(x, ROOM_FLOOR + y, z + out + 0.06),
            Vec3::new(x, ROOM_FLOOR + y - 0.05, z + out + 0.06),
            0.018,
        );
    }
}

/// Una mesa de tablón sobre `r`, de `top` de alto: la tapa biselada, un
/// bastidor debajo, cuatro patas y un travesaño bajo entre ellas.
fn table(shape: &mut Shape, r: Rect, top: f32) {
    let thick = 0.045;
    board(shape, WOOD, r, top, thick);
    let inset = 0.07;
    let (x0, x1, z0, z1) = (r.x0 + inset, r.x1 - inset, r.z0 + inset, r.z1 - inset);
    let (a, b) = (top - thick - 0.09, top - thick);
    for (lo, hi) in [
        (Vec3::new(x0, a, z0), Vec3::new(x1, b, z0 + 0.025)),
        (Vec3::new(x0, a, z1 - 0.025), Vec3::new(x1, b, z1)),
        (Vec3::new(x0, a, z0), Vec3::new(x0 + 0.025, b, z1)),
        (Vec3::new(x1 - 0.025, a, z0), Vec3::new(x1, b, z1)),
    ] {
        solid(shape, WOOD, lo, hi);
    }
    let legs = [(x0 + 0.03, z0 + 0.03), (x1 - 0.03, z0 + 0.03)];
    for (x, z) in legs.into_iter().chain(legs.map(|(x, _)| (x, z1 - 0.03))) {
        shape.beam(
            WOOD,
            Vec3::new(x, ROOM_FLOOR, z),
            Vec3::new(x, top - thick, z),
            0.06,
        );
    }
    let low = ROOM_FLOOR + 0.16;
    let mid = (z0 + z1) / 2.0;
    for x in [x0 + 0.03, x1 - 0.03] {
        shape.beam(
            WOOD,
            Vec3::new(x, low, z0 + 0.03),
            Vec3::new(x, low, z1 - 0.03),
            0.04,
        );
    }
    shape.beam(
        WOOD,
        Vec3::new(x0 + 0.03, low, mid),
        Vec3::new(x1 - 0.03, low, mid),
        0.04,
    );
}

/// Un banco de tablón sobre `r`, de `top` de alto, sobre cuatro patas
/// abiertas y un travesaño.
fn bench(shape: &mut Shape, r: Rect, top: f32) {
    let thick = 0.045;
    board(shape, Surface::Barnwood, r, top, thick);
    let (_, z) = r.center();
    let half = ((r.z1 - r.z0) / 2.0 - 0.06).max(0.02);
    let seat = top - thick;
    for (x, splay) in [(r.x0 + 0.16, -0.05), (r.x1 - 0.16, 0.05)] {
        for side in [-1.0, 1.0] {
            shape.beam(
                Surface::Barnwood,
                Vec3::new(x + splay, ROOM_FLOOR, z + side * half),
                Vec3::new(x, seat, z + side * (half - 0.05)),
                0.045,
            );
        }
    }
    shape.beam(
        Surface::Barnwood,
        Vec3::new(r.x0 + 0.18, ROOM_FLOOR + 0.18, z),
        Vec3::new(r.x1 - 0.18, ROOM_FLOOR + 0.18, z),
        0.04,
    );
}

/// Cuánto baja la mecha del farol desde el cielorraso, en metros.
const LAMP_DROP: f32 = 1.4;

/// El farol de querosén que cuelga del cielorraso a `ceiling` con la mecha
/// en `flame`: el depósito de bronce, la pantalla de lata enlozada de blanco
/// por dentro y la varilla que lo cuelga. El tubo de vidrio que rodea la
/// llama es [`lamp_chimney`].
fn kerosene_lamp(shape: &mut Shape, flame: Vec3, ceiling: f32) {
    // La base del depósito, debajo de la mecha.
    let base = flame - Vec3::Y * 0.13;
    let place = Transform::from_translation(base);
    shape.lathe(
        Surface::Bronze,
        place,
        &[
            (0.0, 0.0),
            (0.04, 0.005),
            (0.075, 0.04),
            (0.08, 0.07),
            (0.06, 0.1),
            (0.03, 0.11),
            (0.03, 0.125),
        ],
        12,
    );
    // El tubo de vidrio va aparte, con su propio brillo ([`lamp_chimney`]).
    // La pantalla: lata por fuera, enlozado blanco por dentro.
    shape.lathe(
        Surface::Sheet,
        place,
        &[(0.24, 0.3), (0.12, 0.38), (0.05, 0.42)],
        12,
    );
    shape.lathe(
        Surface::Whitewash,
        place,
        &[(0.05, 0.418), (0.12, 0.378), (0.24, 0.298)],
        12,
    );
    // Tres brazos del depósito a la pantalla y la varilla hasta el techo.
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        let (s, c) = a.sin_cos();
        shape.beam(
            Surface::Steel,
            base + Vec3::new(0.075 * c, 0.07, 0.075 * s),
            base + Vec3::new(0.2 * c, 0.33, 0.2 * s),
            0.008,
        );
    }
    shape.beam(
        Surface::Steel,
        base + Vec3::Y * 0.42,
        Vec3::new(base.x, ceiling, base.z),
        0.012,
    );
}

/// El tubo de vidrio del farol alrededor de su llama, con la mecha en el
/// origen: se dibuja aparte del resto de la sala porque brilla según el
/// reloj, en el material propio de los faroles de las pulperías.
pub(crate) fn lamp_chimney() -> Shape {
    let mut shape = Shape::default();
    shape.lathe(
        Surface::Glass,
        Transform::from_translation(-Vec3::Y * 0.13),
        &[
            (0.03, 0.125),
            (0.045, 0.17),
            (0.045, 0.21),
            (0.025, 0.3),
            (0.025, 0.36),
        ],
        12,
    );
    shape
}
