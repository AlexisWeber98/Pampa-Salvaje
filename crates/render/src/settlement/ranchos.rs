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
//!   amasado con paja (`mud_straw`, el revoque de barro de `clay_plaster`
//!   con la paja que le agrega `tools/building_straw.py`, corrido distinto en
//!   cada pared y cuarteado por el shader del desgaste) entre horcones de
//!   madera rústica, bajo un techo de paja larga en hiladas
//!   ([`Surface::StrawThatch`]): a dos aguas tres de cada cuatro, a un agua
//!   el resto ([`StrawRoof`]), con un alero grueso y desparejo.
//!
//! Cada una se arma como las demás casas: en su propio espacio, `x` a lo
//! largo de su frente, `z` hacia afuera del frente y `y` hacia arriba desde el
//! suelo en su centro, con las paredes bajando `drop` por debajo del suelo.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;
use crate::facade::grime;
use crate::facade::walls::{Wall, Walls, gable_roof, lean_to, spread, wedge};

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

/// Ruido suave en `[0, 1)` a lo largo de `x`, uno por cada `seed`: el valor
/// de [`grime::hash`] en cada entero, unido por una curva suave, así que no
/// salta como un escalón entre dos muestras vecinas.
fn wobble(seed: f32, x: f32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let u = f * f * (3.0 - 2.0 * f);
    let (a, b) = (grime::hash(seed, i), grime::hash(seed, i + 1.0));
    a + (b - a) * u
}

/// Cómo cae el techo de paja de un rancho de barro y paja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StrawRoof {
    /// A dos aguas, con su cumbrera de barro y los hastiales de barro: la
    /// forma de la mayoría en la pampa del siglo XIX ("techo de quincha a
    /// dos aguas" en los viajeros; los "mojinetes" de las Instrucciones de
    /// Rosas, 1825).
    Gable,
    /// A un agua: alta sobre la pared del fondo y baja sobre el frente, con
    /// el alero desparejo sobre la puerta y las paredes laterales subiendo en
    /// trapecio hasta la paja. Uno de cada cuatro (ver
    /// `odd/tasks/building-realism.md`, L17).
    Shed,
}

impl StrawRoof {
    /// La caída del rancho de la variante `v`: a un agua uno de cada cuatro,
    /// tomado de bits que no usa nada más del rancho.
    pub(crate) fn of(v: u32) -> Self {
        if (v >> 10) % 4 == 3 {
            Self::Shed
        } else {
            Self::Gable
        }
    }
}

/// La paja de un techo: cuán gruesa es, cuánto se hunde en el medio de su
/// largo, y la semilla de sus irregularidades.
#[derive(Debug, Clone, Copy)]
struct Straw {
    thickness: f32,
    sag: f32,
    seed: f32,
}

/// Una caída de paja, dada por su cara de abajo: el alero va de `eave.0` a
/// `eave.1` (de izquierda a derecha, visto desde afuera) y la caída sube en
/// paralelo hasta `top`, la línea de `eave.0` a `eave.0 + rise`.
#[derive(Debug, Clone, Copy)]
struct Pitch {
    eave: (Vec3, Vec3),
    rise: Vec3,
}

/// Cuánto más larga es cada hilada de paja que su paso, en metros: un poco
/// más de 40 cm a lo largo de la caída, lo que se ve de cada una.
const COURSE: f32 = 0.42;

/// Cada cuánto, a lo largo del alero, se arma la cara de la paja.
const COLUMN: f32 = 0.4;

/// Cada cuánto cuelga un mechón del alero.
const TUFT: f32 = 0.06;

impl Pitch {
    fn along(&self) -> Vec3 {
        self.eave.1 - self.eave.0
    }

    /// Hacia afuera de la cara de la paja.
    fn normal(&self) -> Vec3 {
        self.along().cross(self.rise).normalize()
    }

    /// El punto de la cara de abajo a `s` del largo del alero y `t` de la
    /// caída, hundido en el medio del largo, más en lo alto que en el alero.
    fn under(&self, s: f32, t: f32, straw: &Straw) -> Vec3 {
        let sag = straw.sag * 4.0 * s * (1.0 - s) * (0.4 + 0.6 * t);
        self.eave.0 + self.along() * s + self.rise * t - Vec3::Y * sag
    }
}

/// Cuánto sobresale el pie de la hilada `k` a `x` metros a lo largo del
/// alero: los atados quedan desparejos, y la del alero, más gruesa.
fn lip(straw: &Straw, k: usize, x: f32) -> f32 {
    let r = wobble(straw.seed + k as f32 * 3.3 + 50.0, x * 0.9);
    if k == 0 {
        0.04 + 0.03 * r
    } else {
        // Donde un atado quedó al ras, la línea de la hilada se corta.
        0.004 + 0.05 * r * r
    }
}

/// Dónde, de 0 a 1 de la caída, empieza la hilada `k` de `n` a `x` metros
/// a lo largo del alero: cada hilada ondula un poco.
fn course_at(straw: &Straw, k: usize, n: usize, x: f32) -> f32 {
    if k == 0 || k >= n {
        return (k.min(n)) as f32 / n as f32;
    }
    let wave = wobble(straw.seed + k as f32 * 7.1, x * 2.2) - 0.5;
    (k as f32 + wave * 0.6) / n as f32
}

/// Cuánto más cuelga la paja del canto del alero bajo su cara de abajo, a
/// `x` metros a lo largo: a veces apenas, a veces un mechón largo, sin
/// escalones.
fn hang(straw: &Straw, x: f32) -> f32 {
    let slow = wobble(straw.seed + 90.0, x * 2.3);
    let quick = wobble(straw.seed + 91.0, x * 9.0);
    0.03 + 0.14 * slow * slow + 0.06 * quick
}

/// Una caída de paja de `straw.thickness` de grueso sobre la cara de abajo de
/// `pitch`: hiladas que se montan una sobre otra, cada una con el pie de sus
/// atados sobresaliendo y ondulando; un canto grueso y desparejo en el alero
/// del que cuelgan mechones; los bordes de los costados también desparejos.
/// `top` cierra también el canto de arriba (un techo a un agua), con un poco
/// de paja colgando.
fn pitch(shape: &mut Shape, pitch: &Pitch, straw: &Straw, top: bool) {
    let thatch = Surface::StrawThatch;
    let len = pitch.along().length();
    let u = pitch.along() / len;
    let n = pitch.normal();
    let up = pitch.rise.normalize();
    let cols = (len / COLUMN).ceil().max(2.0) as usize;
    let courses = (pitch.rise.length() / COURSE).ceil().max(2.0) as usize;
    // Los bordes de los costados se corren hacia afuera o hacia adentro.
    let verge = |s: f32, t: f32| {
        let side = if s <= 0.0 {
            -1.0
        } else if s >= 1.0 {
            1.0
        } else {
            return Vec3::ZERO;
        };
        u * side * 0.08 * (wobble(straw.seed + side * 17.0, t * 6.0) - 0.4)
    };
    // La cara de arriba en la columna `i`, al pie de la hilada `k` (con su
    // pie sobresaliendo) o al final de ella (`foot` falso), donde se mete
    // bajo la siguiente.
    let lump = |s: f32, t: f32| {
        n * 0.04 * (wobble(straw.seed + 60.0 + (t * 5.0).floor() * 1.7, s * len * 1.3) - 0.5)
    };
    let point = |i: usize, k: usize, foot: bool| {
        let s = i as f32 / cols as f32;
        let x = s * len;
        let t = course_at(straw, k, courses, x);
        let p = pitch.under(s, t, straw) + n * straw.thickness + lump(s, t) + verge(s, t);
        if foot {
            p + n * lip(straw, k, x) - up * 0.015
        } else {
            p
        }
    };
    for k in 0..courses {
        for i in 0..cols {
            let (a0, a1) = (point(i, k, true), point(i + 1, k, true));
            let (b0, b1) = (point(i, k + 1, false), point(i + 1, k + 1, false));
            shape.face(thatch, &[a0, a1, b1, b0]);
            if k + 1 < courses {
                // El pie de la hilada de arriba, que mira hacia abajo de la
                // caída: una línea de sombra.
                let (c0, c1) = (point(i, k + 1, true), point(i + 1, k + 1, true));
                shape.face(thatch, &[b0, b1, c1, c0]);
            }
        }
        // Los costados, cerrados entre la cara de arriba y la de abajo.
        for (i, s, side) in [(0, 0.0, -1.0_f32), (cols, 1.0, 1.0)] {
            let (t0, t1) = (
                course_at(straw, k, courses, s * len),
                course_at(straw, k + 1, courses, s * len),
            );
            let (u0, u1) = (
                pitch.under(s, t0, straw) + verge(s, t0),
                pitch.under(s, t1, straw) + verge(s, t1),
            );
            let (a, b) = (point(i, k, true), point(i, k + 1, false));
            if side > 0.0 {
                shape.face(thatch, &[u0, u1, b, a]);
            } else {
                shape.face(thatch, &[a, b, u1, u0]);
            }
        }
    }
    // La cara de abajo.
    for i in 0..cols {
        let (s0, s1) = (i as f32 / cols as f32, (i + 1) as f32 / cols as f32);
        shape.face(
            thatch,
            &[
                pitch.under(s0, 0.0, straw),
                pitch.under(s0, 1.0, straw),
                pitch.under(s1, 1.0, straw),
                pitch.under(s1, 0.0, straw),
            ],
        );
    }
    // El canto del alero: de la cara de arriba, con el pie de la primera
    // hilada, hasta la paja que cuelga bajo la cara de abajo, a pasos cortos
    // para que el borde ondule sin escalones; y los mechones.
    let m = (len / TUFT).ceil() as usize;
    let edge = |j: usize| {
        let s = j as f32 / m as f32;
        let x = s * len;
        let base = pitch.under(s, 0.0, straw) + verge(s, 0.0);
        let top = base + n * (straw.thickness + lip(straw, 0, x)) - up * 0.015 + lump(s, 0.0);
        let bottom = base - Vec3::Y * hang(straw, x) - up * 0.05;
        (top, bottom)
    };
    for j in 0..m {
        let ((t0, b0), (t1, b1)) = (edge(j), edge(j + 1));
        shape.face(thatch, &[b0, b1, t1, t0]);
        // Un mechón que cuelga del canto: tres pajas finas abiertas en
        // abanico, hacia afuera y hacia abajo, de largos distintos.
        let x = (j as f32 + 0.5) * len / m as f32;
        let at = b0.lerp(b1, grime::hash(straw.seed + 7.0, x * 3.1));
        for (f, fan) in [-0.35_f32, 0.0, 0.35].into_iter().enumerate() {
            let r = grime::hash(straw.seed + 8.0 + f as f32, x * 5.3);
            // Casi todas cortas; de tanto en tanto, una larga.
            let reach = 0.02 + 0.06 * r + 0.12 * r.powi(6);
            let half = 0.006 + 0.008 * grime::hash(straw.seed + 11.0 + f as f32, x);
            let tip = at - Vec3::Y * reach - up * reach * 0.3 + u * (fan + (r - 0.5) * 0.3) * reach;
            shape.face(thatch, &[at + u * half, at - u * half, tip]);
        }
    }
    if top {
        // El canto de arriba: la paja cortada, apenas colgando.
        for j in 0..m {
            let s = |j: usize| j as f32 / m as f32;
            let at = |j: usize| {
                let base = pitch.under(s(j), 1.0, straw) + verge(s(j), 1.0);
                let x = s(j) * len;
                (
                    base + n * straw.thickness + lump(s(j), 1.0),
                    base - Vec3::Y * 0.4 * hang(straw, x + 40.0),
                )
            };
            let ((t0, b0), (t1, b1)) = (at(j), at(j + 1));
            shape.face(thatch, &[b1, b0, t0, t1]);
        }
    }
}

/// Un rollo de barro a lo largo de la cumbrera, de `from` a `to`, hundido
/// como la paja y desparejo, amasado a mano en tramos.
fn ridge_roll(shape: &mut Shape, (from, to): (Vec3, Vec3), straw: &Straw) {
    let len = (to - from).length();
    let n = (len / COLUMN).ceil().max(1.0) as usize;
    for k in 0..n {
        let s = |k: usize| k as f32 / n as f32;
        let at = |k: usize| {
            let sag = straw.sag * 4.0 * s(k) * (1.0 - s(k));
            from + (to - from) * s(k) - Vec3::Y * sag
        };
        let (a, b) = (at(k), at(k + 1));
        let along = b - a;
        let r = 0.16 + 0.04 * grime::hash(straw.seed + 3.0, k as f32);
        shape.lathe(
            Surface::MudStraw,
            Transform::from_translation(a)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, along.normalize())),
            &[
                (0.0, -0.03),
                (r, -0.03),
                (r, along.length() + 0.03),
                (0.0, along.length() + 0.03),
            ],
            6,
        );
    }
}

/// El techo de paja de un rancho de barro y paja sobre `walls`, que caen
/// como dice `form`: hiladas de paja larga con el pie de sus atados a la
/// vista, un alero grueso y desparejo que sobresale `overhang` y cuelga en
/// mechones, y bordes desparejos que sobresalen `verge` de las paredes de
/// los costados. Lo que suben las paredes de barro hasta la paja lo da
/// [`straw_gables`].
pub(crate) fn straw_roof(
    shape: &mut Shape,
    walls: &Walls,
    form: StrawRoof,
    (overhang, verge): (f32, f32),
    seed: f32,
) {
    let h = walls.height;
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    let (xa, xb) = (x0 - verge, x1 + verge);
    let straw = Straw {
        thickness: 0.28,
        sag: 0.1,
        seed,
    };
    match form {
        StrawRoof::Gable => {
            // A 45 grados, empinada como pide la paja.
            let zc = (z0 + z1) / 2.0;
            let rise = walls.depth() / 2.0;
            let slope = rise / (walls.depth() / 2.0);
            let low = h - overhang * slope;
            let up = |z: f32| Vec3::new(0.0, h + rise - low, zc - z);
            for (from, to, z) in [(xa, xb, z1 + overhang), (xb, xa, z0 - overhang)] {
                let eave = (Vec3::new(from, low, z), Vec3::new(to, low, z));
                pitch(shape, &Pitch { eave, rise: up(z) }, &straw, false);
            }
            let n = Vec2::new(1.0, slope).normalize();
            let crest = h + rise + straw.thickness * n.x - 0.04;
            ridge_roll(
                shape,
                (Vec3::new(xa, crest, zc), Vec3::new(xb, crest, zc)),
                &straw,
            );
        }
        StrawRoof::Shed => {
            // Más tendida (unos 20 grados): baja sobre el frente, alta
            // sobre el fondo.
            let slope = SHED_SLOPE;
            let under = |z: f32| h + (z1 - z) * slope;
            let eave = (
                Vec3::new(xa, under(z1 + overhang), z1 + overhang),
                Vec3::new(xb, under(z1 + overhang), z1 + overhang),
            );
            let back = z0 - overhang * 0.5;
            let rise_to = Vec3::new(0.0, under(back) - eave.0.y, back - eave.0.z);
            pitch(
                shape,
                &Pitch {
                    eave,
                    rise: rise_to,
                },
                &straw,
                true,
            );
        }
    }
}

/// La caída de la cara de abajo de un techo a un agua: cuánto sube por metro
/// hacia el fondo (unos 20 grados, con la paja apretada y embarrada).
const SHED_SLOPE: f32 = 0.36;

/// Lo que suben las paredes de barro de `walls` hasta la cara de abajo de su
/// techo de paja: los hastiales de un techo a dos aguas sobre las paredes de
/// los costados, o, a un agua, los costados en trapecio y una franja sobre
/// el fondo. Cada uno es `(pared, xs, zs, ys)` para [`wedge`] sobre la
/// altura de las paredes, con la pared contada como en [`mud_straw`]: 1 el
/// fondo, 2 el costado este, 3 el oeste.
pub(crate) fn straw_gables(walls: &Walls, form: StrawRoof) -> Vec<(usize, Wedge)> {
    let (h, t) = (walls.height, walls.thickness);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    let sides = [(2, (x1 - t, x1)), (3, (x0, x0 + t))];
    match form {
        StrawRoof::Gable => {
            let zc = (z0 + z1) / 2.0;
            let rise = walls.depth() / 2.0;
            sides
                .into_iter()
                .flat_map(|(k, xs)| {
                    [
                        (k, (xs, (z0, zc), (h, h + rise))),
                        (k, (xs, (zc, z1), (h + rise, h))),
                    ]
                })
                .collect()
        }
        StrawRoof::Shed => {
            let under = |z: f32| h + (z1 - z) * SHED_SLOPE;
            let mut out: Vec<(usize, Wedge)> = sides
                .into_iter()
                .map(|(k, xs)| (k, (xs, (z0, z1), (under(z0), h))))
                .collect();
            out.push((
                1,
                ((x0 + t, x1 - t), (z0, z0 + t), (under(z0), under(z0 + t))),
            ));
            out
        }
    }
}

/// Un remate macizo de pared para [`wedge`]: `xs`, `zs` y las alturas de su
/// parte de arriba en `zs.0` y `zs.1`.
pub(crate) type Wedge = ((f32, f32), (f32, f32), (f32, f32));

/// Dónde corre la textura de barro de cada pared de un rancho de barro y
/// paja (0 el frente, 1 el fondo, 2 y 3 los costados): un corrimiento con
/// semilla a lo largo, para que dos paredes (ni dos ranchos) no muestren el
/// mismo pedazo del escaneo. El frente y el fondo se corren cada uno a lo
/// largo de `x`; a lo largo de `z` se corre todo el rancho junto, porque las
/// puntas del frente y del fondo quedan a ras de las caras de los costados
/// y tienen que seguir su barro sin costura. Lo que suben y bajan las
/// paredes no se puede correr.
fn mud_shift(v: u32, wall: usize) -> Vec3 {
    let seed = (v % 9973) as f32;
    let x = grime::hash(seed + wall as f32 * 31.7, 1.0) * 2.0;
    Vec3::new(x, 0.0, grime::hash(seed, 3.0) * 2.0)
}

/// El rancho de barro y paja en su propio espacio.
pub(crate) fn mud_straw(shape: &mut Shape, (w, d, h): (f32, f32, f32), drop: f32, v: u32) {
    // Un vano chico sin vidrio, con un postigo de tabla, y una puerta de
    // tablas sin pintar.
    shape.set_rustic(true);
    shape.set_glazed(false);
    let form = StrawRoof::of(v);
    // Un alero hondo de paja, sobre paredes bajas y gruesas: de 2 a 2,3 m de
    // alto, y a lo sumo 7 por 4,5 m, que no da para más el barro; a un agua,
    // a lo sumo 3,6 m de fondo, que si no el fondo sube demasiado.
    let inset = 0.6;
    let deep = match form {
        StrawRoof::Gable => 4.5,
        StrawRoof::Shed => 3.6,
    };
    let (w, d) = (w.min(7.0 + 2.0 * inset), d.min(deep + 2.0 * inset));
    let walls = walls((w, d), inset, 0.35, (h - 0.5).clamp(2.0, 2.3), drop);
    let len = walls.width();
    let door = vec![if v & 1 == 0 { 0 } else { 1 }];
    let head = 1.75_f32.min(walls.height - 0.2);
    // Cada pared se arma aparte, corrida por su `mud_shift`, y se trae a su
    // lugar: su barro se lee como otro pedazo del escaneo. Las paredes de los
    // costados llevan también lo que sube hasta la paja.
    let piece = || {
        let mut part = Shape::default();
        part.set_rustic(true);
        part.set_glazed(false);
        part
    };
    let mut parts = [piece(), piece(), piece(), piece()];
    let shifts: [Vec3; 4] = std::array::from_fn(|k| mud_shift(v, k));
    let moved = |k: usize| walls_moved(&walls, shifts[k]);
    moved(0).front().build(
        &mut parts[0],
        Surface::MudStraw,
        &spread(len, 2, &door, (0.85, head), (0.5, 0.95, 1.45)),
        v,
    );
    moved(1)
        .back()
        .build(&mut parts[1], Surface::MudStraw, &[], v);
    for (k, east) in [(2, true), (3, false)] {
        moved(k)
            .side(east)
            .build(&mut parts[k], Surface::MudStraw, &[], v);
    }
    for (k, (xs, zs, ys)) in straw_gables(&walls, form) {
        let by = shifts[k];
        wedge(
            &mut parts[k],
            Surface::MudStraw,
            (xs.0 + by.x, xs.1 + by.x),
            (zs.0 + by.z, zs.1 + by.z),
            walls.height,
            ys,
        );
    }
    for (part, by) in parts.into_iter().zip(shifts) {
        shape.merge(part, Transform::from_translation(-by));
    }
    straw_roof(
        shape,
        &walls,
        form,
        (inset - 0.3, inset - 0.35),
        (v % 9973) as f32,
    );
    // Los horcones: postes rústicos en las esquinas y a mitad de las paredes
    // largas, un poco afuera de la cara del barro y un poco más altos que
    // ella, donde apoya el techo.
    let under = |z: f32| match form {
        StrawRoof::Gable => walls.height,
        StrawRoof::Shed => walls.height + (walls.z1 - z) * SHED_SLOPE,
    };
    for at in corners(&walls, 0.06) {
        post(shape, at, 0.2, drop, under(at.1) + 0.12);
    }
    let mid = (walls.x0 + walls.x1) / 2.0;
    if len > 4.5 {
        // En el medio, entre la puerta y la ventana del frente.
        for z in [walls.z0 + 0.04, walls.z1 - 0.04] {
            post(shape, (mid, z), 0.18, drop, under(z) + 0.12);
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
    grime::apron(
        shape,
        (walls.x0, walls.x1, walls.z0, walls.z1),
        (GROUND, GROUND),
        FOOT_GRIME,
    );
}

/// `walls` corridas `by`, también en su altura y en lo que bajan: la misma
/// pared en otro lugar del espacio de las texturas.
fn walls_moved(walls: &Walls, by: Vec3) -> Walls {
    Walls {
        x0: walls.x0 + by.x,
        x1: walls.x1 + by.x,
        z0: walls.z0 + by.z,
        z1: walls.z1 + by.z,
        ..*walls
    }
}

#[cfg(test)]
mod tests {
    use pampa_core::{Aabb, HouseStyle, Side, Structure, StructureKind, Vec2 as GroundVec2};

    use super::*;

    /// La variante más chica de cada caída.
    const GABLE: u32 = 0;
    const SHED: u32 = 3 << 10;

    /// Un rancho de barro y paja de huella `w` por `d`, variante `v`.
    fn rancho((w, d): (f32, f32), v: u32) -> Shape {
        let mut shape = Shape::default();
        mud_straw(&mut shape, (w, d, 2.7), 0.3, v);
        shape
    }

    fn walls_of((w, d): (f32, f32), v: u32) -> Walls {
        let deep = match StrawRoof::of(v) {
            StrawRoof::Gable => 4.5,
            StrawRoof::Shed => 3.6,
        };
        let (w, d) = (w.min(8.2), d.min(deep + 1.2));
        walls((w, d), 0.6, 0.35, 2.2, 0.3)
    }

    /// La paja más alta cerca del plano `z`, dentro de las paredes.
    fn thatch_top_near(shape: &Shape, walls: &Walls, z: f32) -> f32 {
        shape
            .points(Surface::StrawThatch)
            .iter()
            .filter(|p| (p.z - z).abs() < 0.3 && p.x > walls.x0 && p.x < walls.x1)
            .map(|p| p.y)
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn one_mud_and_straw_rancho_in_four_is_a_una_agua() {
        let sheds = (0..4096u32)
            .map(|v| v.wrapping_mul(0x9e37_79b9) ^ v)
            .filter(|&v| StrawRoof::of(v) == StrawRoof::Shed)
            .count();
        assert!((800..=1250).contains(&sheds), "{sheds} of 4096");
        assert_eq!(StrawRoof::of(GABLE), StrawRoof::Gable);
        assert_eq!(StrawRoof::of(SHED), StrawRoof::Shed);
    }

    #[test]
    fn a_dos_aguas_peaks_over_the_middle_and_falls_to_both_eaves() {
        let size = (8.4, 4.85);
        let (shape, w) = (rancho(size, GABLE), walls_of(size, GABLE));
        let zc = (w.z0 + w.z1) / 2.0;
        let (mid, front, back) = (
            thatch_top_near(&shape, &w, zc),
            thatch_top_near(&shape, &w, w.z1),
            thatch_top_near(&shape, &w, w.z0),
        );
        assert!(
            mid > front + 1.0 && mid > back + 1.0,
            "{mid} {front} {back}"
        );
        assert!((front - back).abs() < 0.2, "{front} {back}");
        // Su cumbrera de barro la sella por encima de la paja.
        let ridge = shape
            .points(Surface::MudStraw)
            .iter()
            .map(|p| p.y)
            .fold(f32::MIN, f32::max);
        assert!(ridge > mid, "{ridge} over {mid}");
    }

    #[test]
    fn a_una_agua_rises_from_the_front_eave_to_the_back_wall() {
        let size = (8.4, 4.85);
        let (shape, w) = (rancho(size, SHED), walls_of(size, SHED));
        let (front, back) = (
            thatch_top_near(&shape, &w, w.z1),
            thatch_top_near(&shape, &w, w.z0),
        );
        // Un metro y algo más alta atrás, sin cumbrera.
        assert!(back > front + 1.0, "{back} over {front}");
        let mud = shape.points(Surface::MudStraw);
        let top = |near: f32| {
            mud.iter()
                .filter(|p| (p.z - near).abs() < 0.4)
                .map(|p| p.y)
                .fold(f32::MIN, f32::max)
        };
        // Las paredes de barro suben en trapecio hasta la paja, sin pasarla.
        assert!(top(w.z0) > top(w.z1) + 1.0, "{} {}", top(w.z0), top(w.z1));
        assert!(top(w.z0) < back, "{} under {back}", top(w.z0));
        // Más angosta que una a dos aguas, que el fondo no suba demasiado.
        assert!(w.depth() <= 3.6 + 1e-3);
    }

    #[test]
    fn the_thatch_eave_hangs_ragged_without_steps() {
        let size = (8.4, 4.85);
        for v in [GABLE, SHED] {
            let (shape, w) = (rancho(size, v), walls_of(size, v));
            // El borde de abajo del canto de la paja sobre el frente, de punta
            // a punta, en tramos de 10 cm (sin los mechones finos que
            // cuelgan de él).
            let mut low: Vec<(i32, f32)> = Vec::new();
            let edge = shape
                .triangles_of(Surface::StrawThatch)
                .into_iter()
                .filter(|[a, b, c]| (*b - *a).cross(*c - *a).length() / 2.0 > 0.004)
                .flatten();
            for p in edge {
                if p.z > w.z1 + 0.1 && p.x > w.x0 && p.x < w.x1 {
                    let bin = (p.x * 10.0).floor() as i32;
                    match low.iter_mut().find(|(b, _)| *b == bin) {
                        Some((_, y)) => *y = y.min(p.y),
                        None => low.push((bin, p.y)),
                    }
                }
            }
            low.sort_by_key(|(b, _)| *b);
            let ys: Vec<f32> = low.iter().map(|(_, y)| *y).collect();
            assert!(ys.len() > 40, "{}", ys.len());
            let (lo, hi) = ys
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), &y| (a.min(y), b.max(y)));
            // Cuelga entre unos centímetros y un mechón largo...
            assert!(hi - lo > 0.1, "{v:#x}: {lo}..{hi}");
            // ...sin tramos parejos como bloques: nunca tres seguidos al
            // mismo nivel.
            for run in ys.windows(3) {
                assert!(
                    (run[0] - run[1]).abs() > 1e-3 || (run[1] - run[2]).abs() > 1e-3,
                    "{v:#x}: {run:?}"
                );
            }
        }
    }

    #[test]
    fn the_roof_face_is_laid_in_courses_that_step_out() {
        let size = (8.4, 4.85);
        let (shape, w) = (rancho(size, GABLE), walls_of(size, GABLE));
        // Bajando por la caída del frente en una franja del medio, el pie de
        // cada hilada sobresale sobre la de abajo: una cara que mira hacia
        // afuera y hacia abajo, una sombra, en cada hilada.
        let mid = (w.x0 + w.x1) / 2.0;
        let mut feet: Vec<f32> = shape
            .triangles_of(Surface::StrawThatch)
            .into_iter()
            .filter_map(|[a, b, c]| {
                let n = (b - a).cross(c - a).normalize_or_zero();
                let centre = (a + b + c) / 3.0;
                let strip = (centre.x - mid).abs() < 0.2;
                let zc = (w.z0 + w.z1) / 2.0;
                let up_the_roof = centre.z < w.z1 && centre.z > zc && centre.y > w.height;
                // Hacia abajo de la caída del frente: +Z y -Y por igual.
                let down = n.dot(Vec3::new(0.0, -1.0, 1.0).normalize());
                (strip && up_the_roof && down > 0.5).then_some(centre.y)
            })
            .collect();
        feet.sort_by(f32::total_cmp);
        feet.dedup_by(|a, b| (*a - *b).abs() < 0.1);
        // De la pared a la cumbrera, unos 2,5 m de caída: cinco hiladas o
        // más.
        assert!(feet.len() >= 5, "{feet:?}");
    }

    #[test]
    fn each_mud_wall_shows_its_own_stretch_of_the_scan() {
        let size = (8.4, 4.85);
        // Cuánto está corrida la textura de cada cara de pared hacia afuera,
        // por la dirección de esa cara.
        let shifts = |v: u32| {
            let shape = rancho(size, v);
            let w = walls_of(size, v);
            let tile = Surface::MudStraw.tile();
            let mut out: Vec<(IVec3, f32)> = Vec::new();
            for (p, n, uv) in shape.vertices(Surface::MudStraw) {
                // Solo las caras de afuera de las cuatro paredes.
                let axis = n.round();
                let outer = match axis.as_ivec3().to_array() {
                    [1, 0, 0] => (p.x - w.x1).abs() < 1e-3,
                    [-1, 0, 0] => (p.x - w.x0).abs() < 1e-3,
                    [0, 0, 1] => (p.z - w.z1).abs() < 1e-3,
                    [0, 0, -1] => (p.z - w.z0).abs() < 1e-3,
                    _ => false,
                };
                if (n - axis).length() > 1e-3 || !outer || p.y > w.height {
                    continue;
                }
                let along = Vec3::Y.cross(n);
                let shift = (uv.x * tile - p.dot(along)).rem_euclid(tile);
                let key = axis.as_ivec3();
                match out.iter().find(|(k, _)| *k == key) {
                    // Toda una pared lleva el mismo corrimiento.
                    Some((_, s)) => assert!(
                        (s - shift).abs() < 1e-3 || (s - shift).abs() > tile - 1e-3,
                        "{v:#x} {key}: {s} vs {shift}"
                    ),
                    None => out.push((key, shift)),
                }
            }
            out.sort_by_key(|(k, _)| (k.x, k.z));
            out
        };
        let one = shifts(GABLE);
        assert_eq!(one.len(), 4, "{one:?}");
        // Cada pared, un pedazo distinto del escaneo...
        for (k, a) in one.iter().enumerate() {
            for b in &one[k + 1..] {
                assert!((a.1 - b.1).abs() > 0.02, "{one:?}");
            }
        }
        // ...y otro rancho, otros pedazos.
        let other = shifts(1 << 12);
        for (a, b) in one.iter().zip(&other) {
            assert!((a.1 - b.1).abs() > 0.02, "{one:?} {other:?}");
        }
    }

    #[test]
    fn a_mud_and_straw_rancho_keeps_to_the_outskirts_triangle_budget() {
        // Las afueras de un pueblo ponen a lo sumo 4000 triángulos por casa
        // (town_buildings::tests): el rancho más grande, de las dos caídas.
        for v in [GABLE, SHED, 0x1234_5677, 0x0fed_cba9] {
            for size in [(8.0, 8.5), (9.8, 5.1), (6.0, 4.7)] {
                let tris = rancho(size, v).triangles();
                println!("{v:#x} {size:?} {:?}: {tris} triangles", StrawRoof::of(v));
                assert!(
                    (800..=4000).contains(&tris),
                    "{v:#x} {size:?} {:?}: {tris}",
                    StrawRoof::of(v)
                );
            }
        }
    }

    #[test]
    fn only_the_mud_and_straw_wall_cracks_in_the_shader() {
        for surface in Surface::ALL {
            let cracks = surface.breakup()[3];
            assert_eq!(cracks > 0.0, surface == Surface::MudStraw, "{surface:?}");
        }
    }

    #[test]
    fn the_adobe_rancho_and_the_haystack_keep_the_old_thatch() {
        let structure = |kind| Structure {
            kind,
            footprint: Aabb::from_center_size(GroundVec2::ZERO, GroundVec2::new(8.0, 5.0)),
            height: 2.6,
            front: Side::South,
            variant: 0,
        };
        let adobe = super::super::buildings::building(
            &structure(StructureKind::MainHouse(HouseStyle::AdobeThatch)),
            0.3,
        );
        let stack = super::super::props::haystack(&structure(StructureKind::Haystack), 0.3);
        for shape in [adobe, stack] {
            assert!(shape.surfaces().any(|s| s == Surface::Thatch));
            assert!(!shape.surfaces().any(|s| s == Surface::StrawThatch));
        }
        // El de barro y paja lleva la paja nueva, y no la vieja.
        let mud = rancho((8.0, 5.0), GABLE);
        assert!(mud.surfaces().any(|s| s == Surface::StrawThatch));
        assert!(!mud.surfaces().any(|s| s == Surface::Thatch));
    }
}
