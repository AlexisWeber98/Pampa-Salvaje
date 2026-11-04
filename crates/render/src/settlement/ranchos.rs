//! Las viviendas más humildes de la pampa de 1919, que el campo y las orillas
//! de los pueblos tienen además de los ranchos de adobe
//! ([`super::buildings`]):
//!
//! - El rancho de madera: tablas solapadas (`brown_planks_04`, Poly Haven)
//!   clavadas sobre un armazón de postes que se ve en las esquinas, sobre un
//!   zócalo de tabla, bajo un techo de chapa vieja a dos aguas con su caño de
//!   estufa.
//! - La casilla: una pieza chica de chapa acanalada (vieja o galvanizada)
//!   sobre un armazón de madera, bajo un techo de chapa a un agua, con un
//!   escalón de madera en la puerta y un caño de estufa; la de los peones de
//!   una estancia, de un puesto o de las orillas del pueblo.
//! - El rancho de barro y paja (chorizo): paredes bajas y gruesas de barro
//!   amasado con paja (`mud_straw`, el barro seco y cuarteado de
//!   `clay_block_wall` con la paja que le agrega `tools/building_straw.py`)
//!   entre horcones de madera rústica, bajo un techo de paja empinado y hondo.
//!
//! Cada una se arma como las demás casas: en su propio espacio, `x` a lo
//! largo de su frente, `z` hacia afuera del frente y `y` hacia arriba desde el
//! suelo en su centro, con las paredes bajando `drop` por debajo del suelo.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;
use crate::facade::grime;
use crate::facade::walls::{Wall, Walls, gable_roof, lean_to, spread};

/// Cuánto se aleja de la pared la tierra juntada a su pie, en metros.
const FOOT_GRIME: f32 = 0.3;

/// A qué altura sobre el suelo se tiende esa tierra.
const GROUND: f32 = 0.02;

/// Las paredes de una casa de `w` por `d` y `height` de alto, retiradas
/// `inset` de su huella de edificio, de `thickness` de grosor.
fn walls((w, d): (f32, f32), inset: f32, thickness: f32, height: f32, drop: f32) -> Walls {
    Walls {
        x0: -w / 2.0 + inset,
        x1: w / 2.0 - inset,
        z0: -d / 2.0 + inset,
        z1: d / 2.0 - inset,
        thickness,
        height,
        drop,
    }
}

/// Un poste de madera rústica de `size` de lado en `(x, z)`, desde `-drop`
/// hasta `top`.
fn post(shape: &mut Shape, (x, z): (f32, f32), size: f32, drop: f32, top: f32) {
    let half = size / 2.0;
    shape.block(
        Surface::Sleeper,
        Vec3::new(x - half, -drop, z - half),
        Vec3::new(x + half, top, z + half),
    );
}

/// Las cuatro esquinas exteriores de `walls`, retiradas `half` hacia adentro
/// para que un poste de `2 * half` de lado quede a ras de las dos caras.
fn corners(walls: &Walls, half: f32) -> [(f32, f32); 4] {
    let (x0, x1, z0, z1) = (
        walls.x0 + half,
        walls.x1 - half,
        walls.z0 + half,
        walls.z1 - half,
    );
    [(x0, z0), (x1, z0), (x0, z1), (x1, z1)]
}

/// Un caño de estufa de chapa que sube desde `foot` hasta `top`, con su
/// sombrerete.
fn stovepipe(shape: &mut Shape, foot: Vec3, top: f32) {
    let rise = top - foot.y;
    shape.lathe(
        Surface::Sheet,
        Transform::from_translation(foot),
        &[(0.07, 0.0), (0.07, rise), (0.0, rise)],
        8,
    );
    // Un cono sobre tres patitas cortas, que deja salir el humo.
    shape.lathe(
        Surface::Sheet,
        Transform::from_translation(foot.with_y(top + 0.08)),
        &[(0.0, -0.02), (0.17, 0.0), (0.0, 0.12)],
        8,
    );
}

/// El rancho de madera en su propio espacio.
pub(crate) fn wooden(shape: &mut Shape, (w, d, h): (f32, f32, f32), drop: f32, v: u32) {
    // Postigos y puerta de madera sin pintar; ventanas vidriadas.
    shape.set_rustic(true);
    let inset = 0.35;
    // Bajo: de 2,4 a 2,8 m hasta el alero, sea cual sea la altura que core le
    // da a una casa del pueblo.
    let walls = walls((w, d), inset, 0.12, (h - 0.2).clamp(2.4, 2.8), drop);
    let len = walls.width();
    let rooms = if len > 7.0 { 3 } else { 2 };
    let door = vec![if v & 1 == 0 { 0 } else { rooms - 1 }];
    let head = 1.95_f32.min(walls.height - 0.3);
    walls.front().build(
        shape,
        Surface::Weatherboard,
        &spread(len, rooms, &door, (0.85, head), (0.7, 0.9, 1.6)),
        v,
    );
    walls.back().build(
        shape,
        Surface::Weatherboard,
        &spread(len, 1, &[], (0.0, 0.0), (0.6, 1.0, 1.6)),
        v >> 4,
    );
    for east in [true, false] {
        walls.side(east).build(shape, Surface::Weatherboard, &[], v);
    }
    // El armazón: postes en las esquinas, y una tabla de zócalo todo
    // alrededor, más oscura, que es la primera en pudrirse.
    for at in corners(&walls, 0.08) {
        post(shape, at, 0.16, drop, walls.height);
    }
    let skirt = |shape: &mut Shape, min: Vec3, max: Vec3| shape.block(Surface::Sleeper, min, max);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    skirt(
        shape,
        Vec3::new(x0, -drop, z1),
        Vec3::new(x1, 0.22, z1 + 0.03),
    );
    skirt(
        shape,
        Vec3::new(x0, -drop, z0 - 0.03),
        Vec3::new(x1, 0.22, z0),
    );
    skirt(
        shape,
        Vec3::new(x1, -drop, z0),
        Vec3::new(x1 + 0.03, 0.22, z1),
    );
    skirt(
        shape,
        Vec3::new(x0 - 0.03, -drop, z0),
        Vec3::new(x0, 0.22, z1),
    );
    let rise = walls.depth() / 2.0 * 0.45;
    gable_roof(
        shape,
        (Surface::OldIron, Surface::Weatherboard),
        &walls,
        rise,
        inset - 0.05,
        0.06,
    );
    let at = Vec3::new(walls.x1 - 1.0, 0.0, (z0 + z1) / 2.0 - walls.depth() * 0.2);
    let roof = walls.height + rise * (1.0 - (at.z - (z0 + z1) / 2.0).abs() / (walls.depth() / 2.0));
    stovepipe(shape, at.with_y(roof - 0.2), roof + 0.9);
    grime::apron(shape, (x0, x1, z0, z1), (GROUND, GROUND), FOOT_GRIME);
}

/// La casilla en su propio espacio: de chapa vieja o galvanizada según `v`.
pub(crate) fn casilla(shape: &mut Shape, (w, d, h): (f32, f32, f32), drop: f32, v: u32) {
    let inset = 0.25;
    // Una casilla es chica: a lo sumo 4 m por 3 m dentro de su huella.
    let (w, d) = (w.min(4.0 + 2.0 * inset), d.min(3.0 + 2.0 * inset));
    let walls = walls((w, d), inset, 0.06, (h - 0.3).clamp(2.2, 2.5), drop);
    // Madera sin pintar; vidrio en la mitad de ellas.
    shape.set_rustic(true);
    shape.set_glazed(v & 8 == 0);
    let iron = if v & 2 == 0 {
        Surface::OldIron
    } else {
        Surface::Galvanised
    };
    let len = walls.width();
    let door_at = if v & 1 == 0 { 0 } else { 1 };
    let head = 1.9_f32.min(walls.height - 0.25);
    walls.front().build(
        shape,
        iron,
        &spread(len, 2, &[door_at], (0.8, head), (0.6, 1.0, 1.55)),
        v,
    );
    walls.back().build(shape, iron, &[], v);
    for east in [true, false] {
        let side = walls.side(east);
        let windows = if east == (v & 4 == 0) {
            spread(side.length, 1, &[], (0.0, 0.0), (0.5, 1.05, 1.5))
        } else {
            Vec::new()
        };
        side.build(shape, iron, &windows, v);
    }
    for at in corners(&walls, 0.05) {
        post(shape, at, 0.1, drop, walls.height);
    }
    lean_to(shape, (Surface::OldIron, iron), &walls, 0.45, inset - 0.03);
    // El óxido que chorrea desde el borde de arriba y los clavos de la chapa.
    let seed = (v % 9973) as f32;
    let sides = [
        walls.front(),
        walls.back(),
        walls.side(true),
        walls.side(false),
    ];
    for (k, wall) in sides.iter().enumerate() {
        rust_runs(shape, wall, seed + k as f32 * 7.7);
    }
    // El escalón de madera frente a la puerta.
    let door = walls.x0 + len * (door_at as f32 + 0.5) / 2.0;
    shape.block(
        Surface::Sleeper,
        Vec3::new(door - 0.5, -drop, walls.z1),
        Vec3::new(door + 0.5, 0.16, walls.z1 + 0.3),
    );
    let at = Vec3::new(walls.x0 + 0.6, 0.0, walls.z0 + 0.6);
    let roof = walls.height + 0.45 * (walls.z1 - at.z) / walls.depth();
    stovepipe(shape, at.with_y(roof - 0.2), roof + 0.8);
    grime::apron(
        shape,
        (walls.x0, walls.x1, walls.z0, walls.z1),
        (GROUND, GROUND),
        FOOT_GRIME,
    );
}

/// Chorreados de óxido por la cara de la chapa `wall`, desde su borde de
/// arriba.
fn rust_runs(shape: &mut Shape, wall: &Wall, seed: f32) {
    grime::streaks(
        shape,
        wall,
        (0.1, wall.length - 0.1),
        wall.height - 0.04,
        0.35,
        (0.3, 1.7),
        seed,
        grime::RUST,
    );
}

/// El barro que se ablanda y se escurre al pie de una pared de barro y paja,
/// redondeándolo: una falda inclinada que sale `out` metros de su cara al
/// nivel del suelo y la encuentra a `up` metros de alto, un poco más larga que
/// la pared para redondear las esquinas.
fn mud_foot(shape: &mut Shape, wall: &Wall, (out, up): (f32, f32)) {
    let (a, b) = (-out, wall.length + out);
    shape.face(
        Surface::MudStraw,
        &[
            wall.at(a, -out, -0.05),
            wall.at(b, -out, -0.05),
            wall.at(b, 0.0, up),
            wall.at(a, 0.0, up),
        ],
    );
}

/// Un techo de paja a dos aguas sobre `walls`, con la cumbrera `rise` por
/// encima de la pared, un alero de `overhang` metros y `thickness` de grueso:
/// el haz de paja apoya sobre las paredes, se hunde `sag` metros en el medio
/// de su largo, termina en un canto desparejo que cuelga en mechones, y lleva
/// en la cumbrera un rollo de barro (`ridge`) que la sella. Las paredes de los
/// hastiales se levantan en `gable` hasta tocarlo. `seed` varía los mechones.
#[allow(clippy::too_many_arguments)]
pub(crate) fn thatch_roof(
    shape: &mut Shape,
    walls: &Walls,
    (rise, overhang, thickness): (f32, f32, f32),
    sag: f32,
    (gable, ridge): (Surface, Surface),
    seed: f32,
) {
    let h = walls.height;
    let zc = (walls.z0 + walls.z1) / 2.0;
    let half = walls.depth() / 2.0;
    let slope = rise / half;
    let reach = half + overhang;
    // La cara de abajo pasa por el borde de arriba de las paredes.
    let eave = h - overhang * slope;
    let (xa, xb) = (walls.x0 - overhang * 0.6, walls.x1 + overhang * 0.6);
    let n = ((xb - xa) / 0.5).ceil().max(2.0) as usize;
    let x_at = |k: usize| xa + (xb - xa) * k as f32 / n as f32;
    let sag_at = |k: usize| {
        let t = k as f32 / n as f32;
        sag * 4.0 * t * (1.0 - t)
    };
    // Cuánto más cuelga el canto en cada punto.
    let ragged = |k: usize, side: f32| 0.04 + 0.1 * grime::hash(seed + k as f32 * 1.3, side);
    let thatch = Surface::Thatch;
    for k in 0..n {
        let (x0, x1) = (x_at(k), x_at(k + 1));
        let (s0, s1) = (sag_at(k), sag_at(k + 1));
        for side in [1.0_f32, -1.0] {
            // El alero de este lado, en `z`, y la cumbrera.
            let ze = zc + side * reach;
            let top = |x: f32, s: f32, z: f32, edge: bool| {
                let y = if edge { eave } else { h + rise };
                // El canto se hunde menos que la cumbrera.
                let s = if edge { s * 0.5 } else { s };
                Vec3::new(x, y + thickness - s, z)
            };
            let (e0, e1) = (top(x0, s0, ze, true), top(x1, s1, ze, true));
            let (r0, r1) = (top(x0, s0, zc, false), top(x1, s1, zc, false));
            let (d0, d1) = (ragged(k, side), ragged(k + 1, side));
            let (u0, u1) = (
                e0 - Vec3::Y * (thickness + d0),
                e1 - Vec3::Y * (thickness + d1),
            );
            let (v0, v1) = (r0 - Vec3::Y * thickness, r1 - Vec3::Y * thickness);
            if side > 0.0 {
                shape.face(thatch, &[e0, e1, r1, r0]);
                shape.face(thatch, &[v0, v1, u1, u0]);
                shape.face(thatch, &[u0, u1, e1, e0]);
            } else {
                shape.face(thatch, &[e1, e0, r0, r1]);
                shape.face(thatch, &[v1, v0, u0, u1]);
                shape.face(thatch, &[u1, u0, e0, e1]);
            }
            // Mechones que cuelgan del canto, un poco hacia afuera.
            for j in 0..2 {
                let t = (j as f32 + 0.5) / 2.0;
                let x = x0 + (x1 - x0) * t;
                let r = grime::hash(seed + x * 3.1, side + j as f32);
                let (w, len) = (0.08 + 0.08 * r, 0.05 + 0.12 * r);
                let y = u0.y + (u1.y - u0.y) * t;
                let z = ze + side * 0.02;
                let hang = [
                    Vec3::new(x - w, y - len, z + side * 0.03),
                    Vec3::new(x + w, y - len, z + side * 0.03),
                    Vec3::new(x + w, y + 0.02, z),
                    Vec3::new(x - w, y + 0.02, z),
                ];
                if side > 0.0 {
                    shape.face(thatch, &hang);
                } else {
                    shape.face(thatch, &[hang[1], hang[0], hang[3], hang[2]]);
                }
            }
        }
        // El rollo de barro de la cumbrera.
        let (r0, r1) = (
            Vec3::new(x0, h + rise + thickness - s0 - 0.04, zc),
            Vec3::new(x1, h + rise + thickness - s1 - 0.04, zc),
        );
        let along = r1 - r0;
        shape.lathe(
            ridge,
            Transform::from_translation(r0)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, along.normalize())),
            &[
                (0.0, -0.02),
                (0.15, -0.02),
                (0.15, along.length() + 0.02),
                (0.0, along.length() + 0.02),
            ],
            6,
        );
    }
    // Los cantos de los extremos, cerrados de los dos lados (se ven desde
    // afuera y desde abajo).
    for (x, k) in [(xa, 0), (xb, n)] {
        let s = sag_at(k);
        for side in [1.0_f32, -1.0] {
            let ze = zc + side * reach;
            let e = Vec3::new(x, eave + thickness - s * 0.5, ze);
            let r = Vec3::new(x, h + rise + thickness - s, zc);
            let quad = [e - Vec3::Y * thickness, r - Vec3::Y * thickness, r, e];
            shape.face(thatch, &quad);
            shape.face(thatch, &[quad[3], quad[2], quad[1], quad[0]]);
        }
    }
    // Los hastiales.
    let peak = h + rise;
    for (x, out) in [(walls.x0, -1.0_f32), (walls.x1, 1.0)] {
        let (a, b) = (Vec3::new(x, h, walls.z0), Vec3::new(x, h, walls.z1));
        let top = Vec3::new(x, peak, zc);
        if out < 0.0 {
            shape.face(gable, &[a, b, top]);
        } else {
            shape.face(gable, &[b, a, top]);
        }
    }
}

/// El rancho de barro y paja en su propio espacio.
pub(crate) fn mud_straw(shape: &mut Shape, (w, d, h): (f32, f32, f32), drop: f32, v: u32) {
    // Un vano chico sin vidrio, con un postigo de tabla, y una puerta de
    // tablas sin pintar.
    shape.set_rustic(true);
    shape.set_glazed(false);
    // Un alero hondo de paja, sobre paredes bajas y gruesas: de 2 a 2,3 m de
    // alto, y a lo sumo 7 por 4,5 m, que no da para más el barro.
    let inset = 0.6;
    let (w, d) = (w.min(7.0 + 2.0 * inset), d.min(4.5 + 2.0 * inset));
    let walls = walls((w, d), inset, 0.35, (h - 0.5).clamp(2.0, 2.3), drop);
    let len = walls.width();
    let door = vec![if v & 1 == 0 { 0 } else { 1 }];
    let head = 1.75_f32.min(walls.height - 0.2);
    walls.front().build(
        shape,
        Surface::MudStraw,
        &spread(len, 2, &door, (0.85, head), (0.5, 0.95, 1.45)),
        v,
    );
    walls.back().build(shape, Surface::MudStraw, &[], v);
    for east in [true, false] {
        walls.side(east).build(shape, Surface::MudStraw, &[], v);
    }
    // Los horcones: postes rústicos en las esquinas y a mitad de las paredes
    // largas, un poco afuera de la cara del barro y un poco más altos que él,
    // donde apoya el techo.
    let top = walls.height + 0.12;
    for at in corners(&walls, 0.06) {
        post(shape, at, 0.2, drop, top);
    }
    let mid = (walls.x0 + walls.x1) / 2.0;
    if len > 4.5 {
        // En el medio, entre la puerta y la ventana del frente.
        for z in [walls.z0 + 0.04, walls.z1 - 0.04] {
            post(shape, (mid, z), 0.18, drop, top);
        }
    }
    for wall in [
        walls.front(),
        walls.back(),
        walls.side(true),
        walls.side(false),
    ] {
        mud_foot(shape, &wall, (0.1, 0.35));
    }
    let rise = walls.depth() / 2.0 * 1.0;
    // El canto de la paja termina dentro de la huella.
    thatch_roof(
        shape,
        &walls,
        (rise, inset - 0.3, 0.25),
        0.12,
        (Surface::MudStraw, Surface::MudStraw),
        (v % 9973) as f32,
    );
    grime::apron(
        shape,
        (walls.x0, walls.x1, walls.z0, walls.z1),
        (GROUND, GROUND),
        FOOT_GRIME,
    );
}
