//! La mugre que el tiempo deja en un edificio y que el shader del desgaste
//! ([`super::materials::Weathering`]) no puede ubicar solo, porque depende de
//! la forma: los chorreados de la lluvia bajo cada alféizar y bajo la cornisa,
//! y la tierra que se junta contra el pie de las paredes, sobre la vereda y el
//! patio. Son decals de [`Surface::Grime`], mezclados según el alfa de sus
//! vértices: oscuros donde la mugre es más espesa y desvaneciéndose hacia
//! afuera, así que nunca tienen un borde duro.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;
use super::walls::Wall;

/// El color de un chorreado: el hollín y el polvo que arrastra el agua.
pub const STREAK: [f32; 3] = [0.10, 0.095, 0.085];

/// El color de los chorreados de óxido que baja por la chapa desde sus
/// clavos, sus solapes y su borde de arriba.
pub const RUST: [f32; 3] = [0.30, 0.13, 0.05];

/// El color de la tierra contra el pie de una pared: el loess de la pampa,
/// húmedo.
pub const SOIL: [f32; 3] = [0.30, 0.23, 0.16];

/// Cuánto se despega un decal de la cara de su pared, en metros.
const OFF_WALL: f32 = 0.004;

/// Un número de 0 a 1 que depende solo de `a` y `b`.
pub(crate) fn hash(a: f32, b: f32) -> f32 {
    let h = (a * 127.1 + b * 311.7).sin() * 43_758.547;
    h - h.floor()
}

/// Chorreados de lluvia que bajan por la cara de `wall` desde debajo de una
/// saliente (un alféizar, una cornisa) que va de `span.0` a `span.1` metros a
/// lo largo de ella, a la altura `top`: uno cada `every` metros más o menos,
/// cada uno de un largo entre `reach.0` y `reach.1`, más oscuro arriba y
/// desvaneciéndose hacia abajo, sin bajar del suelo, del color `colour`
/// ([`STREAK`], o [`RUST`] en la chapa). `seed` los varía, y decide si esta
/// saliente chorrea o no ([`Shape::streaks`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn streaks(
    shape: &mut Shape,
    wall: &Wall,
    span: (f32, f32),
    top: f32,
    every: f32,
    reach: (f32, f32),
    seed: f32,
    colour: [f32; 3],
) {
    // Solo la parte [`Shape::streaks`] de las salientes chorrea.
    if hash(seed, 17.3) >= shape.streaks() {
        return;
    }
    let (a, b) = span;
    let n = ((b - a) / every).round().max(1.0) as usize;
    for k in 0..n {
        let r = |salt: f32| hash(seed + k as f32 * 1.37, salt);
        let at = a + (b - a) * (k as f32 + 0.15 + 0.7 * r(1.0)) / n as f32;
        let width = 0.1 + 0.25 * r(2.0);
        let length = reach.0 + (reach.1 - reach.0) * r(3.0);
        let bottom = (top - length).max(0.05);
        let middle = top - (top - bottom) * 0.45;
        let alpha = 0.8 + 0.2 * r(4.0);
        // Más ancho y oscuro arriba, donde gotea la saliente; parejo hasta la
        // mitad, después angostándose y desvaneciéndose al bajar. Oscuro en
        // el medio y desvanecido hacia los costados, sin bordes duros.
        let rows = [
            (bottom, width * 0.15, 0.0),
            (middle, width * 0.4, alpha * 0.85),
            (top, width / 2.0, alpha),
        ];
        for pair in rows.windows(2) {
            let [(y0, h0, a0), (y1, h1, a1)] = [pair[0], pair[1]];
            let p = |x: f32, y: f32| wall.at(at + x, -OFF_WALL, y);
            let [cr, cg, cb] = colour;
            let (c0, c1) = ([cr, cg, cb, a0], [cr, cg, cb, a1]);
            let (e0, e1) = ([cr, cg, cb, 0.0], [cr, cg, cb, 0.0]);
            shape.faded(
                Surface::Grime,
                &[p(-h0, y0), p(0.0, y0), p(0.0, y1), p(-h1, y1)],
                [e0, c0, c1, e1],
            );
            shape.faded(
                Surface::Grime,
                &[p(0.0, y0), p(h0, y0), p(h1, y1), p(0.0, y1)],
                [c0, e0, e1, c1],
            );
        }
    }
}

/// La tierra que se junta al pie de las cuatro paredes de un rectángulo de
/// `x0..x1` por `z0..z1` (en el espacio de la estructura, con el frente hacia
/// `+Z`): una franja de `width` metros sobre el suelo, oscura contra la pared
/// y desvaneciéndose hacia afuera. La del frente se tiende a `front`
/// metros sobre el suelo (sobre la vereda); las demás, a `rest`.
pub(crate) fn apron(
    shape: &mut Shape,
    (x0, x1, z0, z1): (f32, f32, f32, f32),
    (front, rest): (f32, f32),
    width: f32,
) {
    let inner = [SOIL[0], SOIL[1], SOIL[2], 0.85];
    let outer = [SOIL[0], SOIL[1], SOIL[2], 0.0];
    // Cada lado: dos esquinas contra la pared, en sentido antihorario visto
    // desde arriba, y hacia dónde queda afuera.
    let sides = [
        (Vec3::new(x1, front, z1), Vec3::new(x0, front, z1), Vec3::Z),
        (
            Vec3::new(x0, rest, z0),
            Vec3::new(x1, rest, z0),
            Vec3::NEG_Z,
        ),
        (Vec3::new(x1, rest, z0), Vec3::new(x1, rest, z1), Vec3::X),
        (
            Vec3::new(x0, rest, z1),
            Vec3::new(x0, rest, z0),
            Vec3::NEG_X,
        ),
    ];
    for (a, b, out) in sides {
        let corners = [a, b, b + out * width, a + out * width];
        shape.faded(Surface::Grime, &corners, [inner, inner, outer, outer]);
    }
}
