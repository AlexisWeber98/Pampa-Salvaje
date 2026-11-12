//! La sala de la pulpería de un pueblo pequeño, la que se ve y se recorre al
//! entrar por la puerta del medio del frente: un piso de ladrillo, un
//! cielorraso de tablas sobre tirantes, el mostrador con su reja de hierro
//! hasta el techo, la estantería del fondo con botellas, ginebras cuadradas,
//! cajones y damajuanas forradas de paja, dos barriles junto al mostrador,
//! tres mesas con sus bancos y un farol de querosén colgado en el medio; sobre
//! el mostrador, la caja registradora, vasos y frascos, y sobre las mesas,
//! vasos, botellas y a veces los naipes de un truco. El
//! almacén de ramos generales tiene la misma puerta, el mismo piso, cielorraso
//! y farol, y sus propios muebles ([`almacen`]).
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

mod almacen;

use bevy::prelude::*;
use pampa_core::Aabb;
use pampa_core::pulperia::{FurnitureKind, Interior, walks_in};

use super::super::italianate::{FANLIGHT, leaf_spans, panelled_leaf};
use super::super::materials::{Livery, Paint, Surface};
use super::super::shape::{Shape, Weather};
use super::{Lot, damp, paints_its_joinery, store_middle_door, streaks, wear, yaw};
use crate::coords::world_from_ground;

/// A qué altura sobre la base del edificio está el piso de ladrillo de la
/// sala, en metros: apenas sobre la vereda y bajo el umbral. Las alturas de
/// los muebles de core se miden desde aquí. Es el de core, donde el juego
/// sienta a los parroquianos.
pub(crate) const ROOM_FLOOR: f32 = pampa_core::pulperia::FLOOR_RISE;

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

/// La sala amueblada de la pulpería o el almacén de `lot` en su propio
/// espacio (el frente en `+Z`), o `None` si `lot` no se recorre por dentro.
pub(crate) fn interior(lot: &Lot) -> Option<Shape> {
    let room = room(lot)?;
    let local = |a: Aabb| local_rect(lot, a);
    let floor = local(room.floor);
    let ceiling = room.ceiling - CEILING_BELOW;
    let mut shape = Shape::default();
    floor_and_ceiling(&mut shape, floor, ceiling);
    let mut barrels = 0;
    let mut tables = 0;
    for f in &room.furniture {
        let r = local(f.footprint);
        let top = ROOM_FLOOR + f.height;
        match f.kind {
            FurnitureKind::Counter => {
                counter(&mut shape, r, top, ceiling);
                counter_things(&mut shape, r, top);
            }
            FurnitureKind::Shelves => shelves(&mut shape, r, top, lot.variant),
            FurnitureKind::Barrel => {
                barrel(&mut shape, r, top, barrels == 0);
                barrels += 1;
            }
            FurnitureKind::Table => {
                table(&mut shape, r, top);
                table_things(&mut shape, r, top, tables, lot.variant);
                tables += 1;
            }
            FurnitureKind::Bench => bench(&mut shape, r, top),
            FurnitureKind::ShopCounter => almacen::shop_counter(&mut shape, r, top),
            FurnitureKind::Goods => {
                let wall = almacen::Against::of(r, floor);
                almacen::goods(&mut shape, wall, f.height, lot.variant);
            }
            FurnitureKind::Sacks => almacen::sacks(&mut shape, r, top),
            FurnitureKind::Saddlery => {
                almacen::saddlery(&mut shape, almacen::Against::of(r, floor), top)
            }
            FurnitureKind::Tools => almacen::tools(&mut shape, almacen::Against::of(r, floor), top),
        }
    }
    kerosene_lamp(&mut shape, lamp_at(lot, &room), ceiling);
    Some(shape)
}

/// La sala de core de `lot`, si es un local que se recorre: la pulpería o un
/// almacén ([`walks_in`]).
fn room(lot: &Lot) -> Option<Interior> {
    walks_in(lot.building.kind).then(|| Interior::of(&lot.building))
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
    counter_body(shape, r, top);
    grille(shape, r, top, ceiling);
}

/// El cuerpo de madera de un mostrador sobre `r`, de `top` de alto, con el
/// lado de los clientes en `z1`: el zócalo oscuro, el frente pintado con sus
/// montantes y travesaños, y la tapa de tablón que sobresale.
fn counter_body(shape: &mut Shape, r: Rect, top: f32) {
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
}

/// La reja de hierro del mostrador de la pulpería sobre `r`, de su tapa a
/// `top` hasta el cielorraso a `ceiling`.
fn grille(shape: &mut Shape, r: Rect, top: f32, ceiling: f32) {
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

/// Lo que hay sobre el mostrador de la pulpería sobre `r`, de `top` de alto:
/// la caja registradora entre la ventanilla y el paso al fondo; junto a la
/// ventanilla, una botella y vasos para servir; y en la otra punta, un frasco
/// de caramelos y otro de aceitunas.
fn counter_things(shape: &mut Shape, r: Rect, top: f32) {
    let z = (r.z0 + r.z1) / 2.0;
    cash_register(shape, Vec3::new(r.x1 - 0.45, top, z - 0.05));
    let window = r.x1 - 1.2;
    bottle(shape, Vec3::new(window - 0.2, top, z - 0.1));
    for (dx, dz) in [(0.0, 0.05), (0.12, -0.08), (0.22, 0.1)] {
        glass(shape, Vec3::new(window + dx, top, z + dz));
    }
    for (k, fill) in [Paint::Terracotta, Paint::GreyBlue].into_iter().enumerate() {
        jar(shape, Vec3::new(r.x0 + 0.3 + 0.3 * k as f32, top, z), fill);
    }
}

/// Lo que hay sobre la mesa `index` de la pulpería, sobre `r` y de `top` de
/// alto, según la semilla `v`: vasos con una botella, o los naipes de un
/// truco a medio jugar con los vasos de los que juegan.
fn table_things(shape: &mut Shape, r: Rect, top: f32, index: usize, v: u32) {
    let (x, z) = r.center();
    let long_x = r.x1 - r.x0 > r.z1 - r.z0;
    // A lo largo de la mesa, hacia cada cabecera.
    let along = |t: f32, a: f32| {
        if long_x {
            Vec3::new(x + t, top, z + a)
        } else {
            Vec3::new(x + a, top, z + t)
        }
    };
    let cards = (index as u32 + v) % 3 == 1;
    if cards {
        // El mazo, las cartas de cada mano boca abajo y las tiradas en el
        // medio, cada una un poco torcida.
        let card = |shape: &mut Shape, at: Vec3, turn: f32, back: bool| {
            let surface = if back {
                Surface::Painted(Paint::Terracotta)
            } else {
                Surface::Painted(Paint::LimeWhite)
            };
            shape.cuboid(
                surface,
                Transform::from_translation(at + Vec3::Y * 0.002)
                    .with_rotation(Quat::from_rotation_y(turn))
                    .with_scale(Vec3::new(0.06, 0.003, 0.09)),
                false,
            );
        };
        shape.cuboid(
            Surface::Painted(Paint::Terracotta),
            Transform::from_translation(along(-0.5, 0.0) + Vec3::Y * 0.012)
                .with_rotation(Quat::from_rotation_y(0.2))
                .with_scale(Vec3::new(0.06, 0.024, 0.09)),
            false,
        );
        for (k, t) in [-0.25, 0.25].into_iter().enumerate() {
            for c in 0..3 {
                let a = (c as f32 - 1.0) * 0.04;
                card(
                    shape,
                    along(t, a + 0.18 - 0.36 * k as f32),
                    0.3 * (c as f32 - 1.0),
                    true,
                );
            }
        }
        for (k, t) in [-0.06, 0.02, 0.1].into_iter().enumerate() {
            card(
                shape,
                along(t, 0.02 * k as f32),
                0.5 * k as f32 - 0.4,
                false,
            );
        }
        for t in [-0.6, 0.6] {
            glass(shape, along(t, 0.2));
        }
        return;
    }
    let n = 2 + (pick(index as u32, 7, v) * 3.0) as usize;
    for k in 0..n {
        let t = -0.55 + 1.1 * k as f32 / (n - 1).max(1) as f32;
        let side = if k % 2 == 0 { 0.2 } else { -0.2 };
        glass(shape, along(t, side));
    }
    if pick(index as u32, 8, v) < 0.7 {
        bottle(shape, along(0.05, 0.0));
    } else {
        gin_bottle(shape, along(0.05, 0.0));
    }
}

/// Un vaso de vidrio grueso, sin tapa, parado en `at`.
fn glass(shape: &mut Shape, at: Vec3) {
    shape.lathe(
        Surface::Glass,
        Transform::from_translation(at),
        &[(0.0, 0.0), (0.03, 0.0), (0.037, 0.1)],
        8,
    );
}

/// Un frasco de vidrio con tapa de lata parado en `at`, lleno de caramelos o
/// aceitunas del color `fill`.
fn jar(shape: &mut Shape, at: Vec3, fill: Paint) {
    let place = Transform::from_translation(at);
    shape.lathe(
        Surface::Painted(fill),
        place,
        &[(0.0, 0.01), (0.075, 0.01), (0.075, 0.17), (0.0, 0.19)],
        8,
    );
    shape.lathe(
        Surface::Glass,
        place,
        &[(0.08, 0.0), (0.085, 0.2), (0.06, 0.24)],
        8,
    );
    shape.lathe(
        Surface::Sheet,
        place,
        &[(0.064, 0.235), (0.064, 0.27), (0.0, 0.27)],
        8,
    );
}

/// Una caja registradora de bronce labrado parada sobre el mostrador en `at`,
/// con las teclas hacia el que atiende (`-Z`): el cajón abajo, el cuerpo con
/// tres hileras de teclas negras en escalera, el visor
/// arriba con su vidrio y la manivela al costado.
fn cash_register(shape: &mut Shape, at: Vec3) {
    let p = |x: f32, y: f32, z: f32| at + Vec3::new(x, y, z);
    solid(
        shape,
        Surface::Bronze,
        p(-0.22, 0.0, -0.18),
        p(0.22, 0.1, 0.18),
    );
    shape.beam(
        Surface::Bronze,
        p(-0.06, 0.05, -0.185),
        p(0.06, 0.05, -0.185),
        0.015,
    );
    shape.chamfered(
        Surface::Bronze,
        p(-0.2, 0.1, -0.02),
        p(0.2, 0.34, 0.16),
        0.03,
    );
    for row in 0..3 {
        let (y, z) = (0.13 + 0.06 * row as f32, -0.15 + 0.045 * row as f32);
        solid(
            shape,
            Surface::Bronze,
            p(-0.2, 0.1, z - 0.02),
            p(0.2, y, z + 0.03),
        );
        for k in 0..6 {
            let x = -0.15 + 0.06 * k as f32;
            solid(
                shape,
                Surface::Livery(Livery::Black),
                p(x - 0.015, y, z - 0.015),
                p(x + 0.015, y + 0.035, z + 0.015),
            );
        }
    }
    shape.chamfered(
        Surface::Bronze,
        p(-0.12, 0.34, 0.0),
        p(0.12, 0.46, 0.12),
        0.02,
    );
    for z in [-0.002, 0.122] {
        solid(
            shape,
            Surface::Glass,
            p(-0.09, 0.37, z - 0.003),
            p(0.09, 0.43, z + 0.003),
        );
    }
    shape.beam(
        Surface::Steel,
        p(0.2, 0.22, 0.06),
        p(0.27, 0.22, 0.06),
        0.02,
    );
    shape.beam(
        Surface::Steel,
        p(0.27, 0.22, 0.06),
        p(0.27, 0.12, 0.0),
        0.018,
    );
    shape.beam(
        Surface::Livery(Livery::Black),
        p(0.27, 0.12, 0.0),
        p(0.32, 0.12, 0.0),
        0.03,
    );
}
