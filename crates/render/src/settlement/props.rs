//! Las estructuras más pequeñas de los asentamientos, construidas de forma
//! procedural en su propio espacio como los edificios (ver
//! [`super::buildings`]): `x` a lo largo del frente, `z` hacia afuera de él,
//! `y` hacia arriba desde el suelo en el centro.
//!
//! - El corral: postes y tres travesaños de tablas curtidas, con su abertura de
//!   4 m en el centro del frente, tal como la deja core.
//! - El molino: una torre de reticulado de acero estilo Aermotor con su
//!   plataforma, caja de engranajes y veleta, y una rueda de muchas aspas
//!   construida aparte para que pueda girar ([`crate::WindmillRotor`]).
//! - El tanque australiano: una pared redonda de chapa acanalada galvanizada
//!   sobre una base de ladrillo, llena de agua. El bebedero: un largo
//!   abrevadero de tablones.
//! - El pozo: un brocal de ladrillo con un armazón de madera, una roldana y un
//!   balde.
//! - La parva (un montón de heno), el palenque y la carreta con sus dos ruedas
//!   altísimas, su caja, su toldo de paja y su lanza.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;
use pampa_core::{Settlement, Structure, StructureKind};

use super::buildings::local_size;
use super::materials::Surface;
use super::shape::Shape;

/// Altura local del suelo bajo un punto del espacio propio de una estructura,
/// relativa a su centro.
pub(crate) type LocalGround<'a> = &'a dyn Fn(Vec3) -> f32;

/// Un lado de un corral: su mesh y los extremos de su línea, en el espacio
/// propio del corral (para su forma de fundido).
pub(crate) struct CorralSide {
    pub shape: Shape,
    pub ends: (Vec3, Vec3),
}

/// Separación entre los postes de un corral, en metros.
const POST_SPACING: f32 = 2.4;
/// Alturas de los travesaños de un corral.
const RAILS: [f32; 3] = [0.45, 0.9, 1.35];

/// Los lados del corral `structure`, cada uno siguiendo el suelo: el frente en
/// dos piezas, una a cada lado de su abertura.
pub(crate) fn corral(structure: &Structure, ground: LocalGround) -> Vec<CorralSide> {
    let size = local_size(structure);
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    let gate = Settlement::CORRAL_GATE / 2.0;
    let runs = [
        (Vec3::new(-hx, 0.0, hz), Vec3::new(-gate, 0.0, hz), true),
        (Vec3::new(gate, 0.0, hz), Vec3::new(hx, 0.0, hz), true),
        (Vec3::new(hx, 0.0, hz), Vec3::new(hx, 0.0, -hz), false),
        (Vec3::new(hx, 0.0, -hz), Vec3::new(-hx, 0.0, -hz), false),
        (Vec3::new(-hx, 0.0, -hz), Vec3::new(-hx, 0.0, hz), false),
    ];
    runs.into_iter()
        .map(|(a, b, front)| {
            let mut shape = Shape::default();
            let at = |p: Vec3| p.with_y(ground(p));
            let n = ((a.distance(b) / POST_SPACING).ceil() as usize).max(1);
            let points: Vec<Vec3> = (0..=n)
                .map(|k| at(a.lerp(b, k as f32 / n as f32)))
                .collect();
            for (k, &p) in points.iter().enumerate() {
                // Los postes de la tranquera son más gruesos y más altos.
                let gate_post = front && ((k == n && a.x < 0.0) || (k == 0 && a.x > 0.0));
                let (side, top) = if gate_post { (0.24, 1.9) } else { (0.15, 1.6) };
                shape.beam(Surface::Planks, p - Vec3::Y * 0.3, p + Vec3::Y * top, side);
            }
            for w in points.windows(2) {
                for y in RAILS {
                    shape.beam(
                        Surface::Planks,
                        w[0] + Vec3::Y * y,
                        w[1] + Vec3::Y * y,
                        0.07,
                    );
                }
            }
            CorralSide {
                shape,
                ends: (a, b),
            }
        })
        .collect()
}

/// Radio de la rueda del molino, en metros.
pub(crate) const WHEEL_RADIUS: f32 = 1.8;

/// Dónde gira la rueda del molino en su propio espacio, y su guiñada: de cara
/// al viento predominante, un poco distinta en cada molino.
pub(crate) fn rotor_place(structure: &Structure) -> (Vec3, Quat) {
    let yaw = Quat::from_rotation_y(0.6 + (structure.variant % 97) as f32 / 97.0 * 0.8);
    let hub = Vec3::new(0.0, structure.height + 0.45, 0.0);
    (hub + yaw * Vec3::Z * 0.7, yaw)
}

/// La torre, la plataforma, la caja de engranajes, la veleta y la varilla de
/// bombeo del molino (la rueda es [`rotor`]), metidas `drop` metros en el
/// suelo.
pub(crate) fn windpump(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let foot = local_size(structure).x.min(local_size(structure).y) / 2.0 - 0.1;
    let (top, head) = (structure.height, 0.3);
    let leg = |sx: f32, sz: f32, y: f32| {
        let r = foot + (head - foot) * (y / top);
        Vec3::new(sx * r, y, sz * r)
    };
    let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    for (sx, sz) in corners {
        shape.beam(
            Surface::Steel,
            leg(sx, sz, 0.0) - Vec3::Y * drop,
            leg(sx, sz, top),
            0.09,
        );
        // Un anclaje fijado en hormigón en cada pie.
        let f = leg(sx, sz, 0.0);
        shape.block(
            Surface::Brick,
            f - Vec3::new(0.2, drop, 0.2),
            f + Vec3::new(0.2, 0.15, 0.2),
        );
    }
    // Travesaños en cada nivel y varillas diagonales cruzadas entre ellos.
    let levels = [0.0, 0.22, 0.42, 0.6, 0.76, 0.9];
    for (k, &l) in levels.iter().enumerate() {
        let y = top * l + 0.3;
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            shape.beam(Surface::Steel, leg(a.0, a.1, y), leg(b.0, b.1, y), 0.05);
            if let Some(&next) = levels.get(k + 1) {
                let y2 = top * next + 0.3;
                shape.beam(Surface::Steel, leg(a.0, a.1, y), leg(b.0, b.1, y2), 0.025);
                shape.beam(Surface::Steel, leg(b.0, b.1, y), leg(a.0, a.1, y2), 0.025);
            }
        }
    }
    // La plataforma alrededor de la parte superior y su varilla de bombeo por
    // el centro.
    shape.block(
        Surface::Planks,
        Vec3::new(-0.75, top - 0.6, -0.75),
        Vec3::new(0.75, top - 0.52, 0.75),
    );
    shape.beam(
        Surface::Steel,
        Vec3::new(0.0, 0.2, 0.0),
        Vec3::new(0.0, top, 0.0),
        0.04,
    );
    // El cabezal de la bomba al pie.
    shape.lathe(
        Surface::Steel,
        Transform::IDENTITY,
        &[(0.12, -0.1), (0.12, 0.7), (0.06, 0.8), (0.0, 0.8)],
        8,
    );
    // Caja de engranajes, plataforma giratoria y la veleta en su brazo, con la
    // guiñada de la rueda.
    let (hub, yaw) = rotor_place(structure);
    let mount = Vec3::new(0.0, hub.y, 0.0);
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(Vec3::new(0.0, top, 0.0)),
        &[(0.25, 0.0), (0.25, 0.15), (0.0, 0.15)],
        10,
    );
    shape.cuboid(
        Surface::Steel,
        Transform::from_translation(mount + yaw * Vec3::Z * 0.15)
            .with_rotation(yaw)
            .with_scale(Vec3::new(0.35, 0.4, 0.9)),
        true,
    );
    let tail = mount - yaw * Vec3::Z * 2.4;
    shape.beam(Surface::Steel, mount, tail, 0.06);
    shape.cuboid(
        Surface::Sheet,
        Transform::from_translation(tail - yaw * Vec3::Z * 0.6)
            .with_rotation(yaw)
            .with_scale(Vec3::new(0.03, 1.0, 1.5)),
        true,
    );
    shape
}

/// La rueda del molino en su propio espacio, girando alrededor de `Z`:
/// dieciocho aspas curvas galvanizadas entre un aro interior y uno exterior de
/// acero, sobre un cubo.
pub(crate) fn rotor() -> Shape {
    let mut shape = Shape::default();
    let blades = 18;
    let inner = 0.6;
    for k in 0..blades {
        let a = k as f32 / blades as f32 * TAU;
        let dir = Vec3::new(a.cos(), a.sin(), 0.0);
        let mid = dir * ((inner + WHEEL_RADIUS) / 2.0);
        // Cada aspa torcida hacia el viento, más ancha en el borde.
        let turn = Quat::from_rotation_z(a - FRAC_PI_2) * Quat::from_rotation_y(0.5);
        shape.cuboid(
            Surface::Sheet,
            Transform::from_translation(mid)
                .with_rotation(turn)
                .with_scale(Vec3::new(0.32, WHEEL_RADIUS - inner, 0.02)),
            true,
        );
    }
    for r in [inner, WHEEL_RADIUS * 0.75, WHEEL_RADIUS] {
        let n = 24;
        for k in 0..n {
            let (a, b) = (k as f32 / n as f32 * TAU, (k + 1) as f32 / n as f32 * TAU);
            shape.beam(
                Surface::Steel,
                Vec3::new(a.cos(), a.sin(), 0.0) * r,
                Vec3::new(b.cos(), b.sin(), 0.0) * r,
                0.04,
            );
        }
    }
    for k in 0..6 {
        let a = k as f32 / 6.0 * TAU;
        shape.beam(
            Surface::Steel,
            Vec3::ZERO,
            Vec3::new(a.cos(), a.sin(), 0.0) * inner,
            0.04,
        );
    }
    shape.lathe(
        Surface::Steel,
        Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        &[(0.0, -0.2), (0.12, -0.2), (0.12, 0.15), (0.0, 0.25)],
        10,
    );
    shape
}

/// El tanque australiano: paredes de chapa acanalada galvanizada alrededor de
/// una base de ladrillo, con el agua un poco por debajo del borde.
pub(crate) fn tank(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let r = local_size(structure).x.min(local_size(structure).y) / 2.0 - 0.15;
    let h = structure.height;
    let segments = 40;
    shape.lathe(
        Surface::Brick,
        Transform::IDENTITY,
        &[(r + 0.15, -drop), (r + 0.15, 0.2), (r - 0.05, 0.2)],
        segments,
    );
    shape.lathe(
        Surface::Galvanised,
        Transform::IDENTITY,
        &[(r, 0.2), (r, h)],
        segments,
    );
    // El borde enrollado, y después la pared interior bajando de nuevo.
    shape.lathe(
        Surface::Galvanised,
        Transform::IDENTITY,
        &[
            (r, h),
            (r + 0.02, h + 0.04),
            (r - 0.04, h + 0.04),
            (r - 0.04, h),
        ],
        segments,
    );
    shape.lathe(
        Surface::Galvanised,
        Transform::IDENTITY,
        &[(r - 0.04, h), (r - 0.04, h - 0.4)],
        segments,
    );
    shape.disc(
        Surface::Water,
        Transform::from_xyz(0.0, h - 0.3, 0.0),
        r - 0.03,
        segments,
    );
    shape
}

/// El bebedero: un largo abrevadero de tablones sobre patas cortas, lleno hasta
/// el borde.
pub(crate) fn trough(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let size = local_size(structure);
    let (hx, hz, h) = (size.x / 2.0, size.y / 2.0, structure.height);
    let wall = 0.06;
    // Los laterales y los extremos, y después el agua entre ellos.
    shape.block(
        Surface::Planks,
        Vec3::new(-hx, 0.15, hz - wall),
        Vec3::new(hx, h, hz),
    );
    shape.block(
        Surface::Planks,
        Vec3::new(-hx, 0.15, -hz),
        Vec3::new(hx, h, -hz + wall),
    );
    shape.block(
        Surface::Planks,
        Vec3::new(-hx, 0.15, -hz + wall),
        Vec3::new(-hx + wall, h, hz - wall),
    );
    shape.block(
        Surface::Planks,
        Vec3::new(hx - wall, 0.15, -hz + wall),
        Vec3::new(hx, h, hz - wall),
    );
    shape.cuboid(
        Surface::Planks,
        Transform::from_xyz(0.0, 0.18, 0.0).with_scale(Vec3::new(2.0 * hx, 0.06, 2.0 * hz)),
        true,
    );
    let y = h - 0.08;
    shape.face(
        Surface::Water,
        &[
            Vec3::new(-hx + wall, y, hz - wall),
            Vec3::new(hx - wall, y, hz - wall),
            Vec3::new(hx - wall, y, -hz + wall),
            Vec3::new(-hx + wall, y, -hz + wall),
        ],
    );
    let legs = ((2.0 * hx / 1.8) as usize).max(2);
    for k in 0..legs {
        let x = -hx + 0.2 + (2.0 * hx - 0.4) * k as f32 / (legs - 1) as f32;
        for z in [-hz + 0.1, hz - 0.1] {
            shape.beam(
                Surface::Planks,
                Vec3::new(x, -drop, z),
                Vec3::new(x, 0.2, z),
                0.1,
            );
        }
    }
    shape
}

/// El pozo: un brocal redondo de ladrillo, un armazón de madera con una
/// roldana, un balde colgado de su soga y agua oscura en el fondo.
pub(crate) fn well(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let r = local_size(structure).x.min(local_size(structure).y) / 2.0 - 0.05;
    let h = structure.height;
    let segments = 20;
    shape.lathe(
        Surface::Brick,
        Transform::IDENTITY,
        &[
            (r, -drop),
            (r, h),
            (r, h),
            (r - 0.24, h),
            (r - 0.24, h),
            (r - 0.24, -0.6),
        ],
        segments,
    );
    shape.disc(
        Surface::Water,
        Transform::from_xyz(0.0, -0.6, 0.0),
        r - 0.2,
        segments,
    );
    let top = h + 1.5;
    for x in [-r + 0.1, r - 0.1] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, h - 0.4, 0.0),
            Vec3::new(x, top, 0.0),
            0.12,
        );
    }
    shape.beam(
        Surface::Planks,
        Vec3::new(-r - 0.05, top, 0.0),
        Vec3::new(r + 0.05, top, 0.0),
        0.12,
    );
    shape.lathe(
        Surface::Planks,
        Transform::from_xyz(0.0, top - 0.17, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
        &[(0.0, -0.04), (0.12, -0.04), (0.12, 0.04), (0.0, 0.04)],
        10,
    );
    shape.beam(
        Surface::Planks,
        Vec3::new(0.12, top - 0.17, 0.0),
        Vec3::new(0.12, h + 0.55, 0.0),
        0.015,
    );
    shape.lathe(
        Surface::Galvanised,
        Transform::from_xyz(0.12, h + 0.25, 0.0),
        &[
            (0.0, 0.0),
            (0.12, 0.0),
            (0.15, 0.3),
            (0.13, 0.3),
            (0.1, 0.03),
        ],
        10,
    );
    shape
}

/// Una parva: un montón de heno sobre su huella, redondeado arriba.
pub(crate) fn haystack(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let size = local_size(structure);
    let h = structure.height;
    shape.lathe(
        Surface::Thatch,
        Transform::from_scale(Vec3::new(size.x / 2.0, 1.0, size.y / 2.0)),
        &[
            (0.92, -drop),
            (1.0, h * 0.25),
            (0.98, h * 0.5),
            (0.86, h * 0.72),
            (0.6, h * 0.9),
            (0.25, h * 0.98),
            (0.0, h),
        ],
        20,
    );
    shape
}

/// El palenque: dos postes gruesos y una barra para atar caballos.
pub(crate) fn hitching_post(structure: &Structure, drop: f32) -> Shape {
    let mut shape = Shape::default();
    let hx = local_size(structure).x / 2.0 - 0.15;
    let h = structure.height;
    for x in [-hx, hx] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, -drop, 0.0),
            Vec3::new(x, h, 0.0),
            0.16,
        );
    }
    shape.beam(
        Surface::Planks,
        Vec3::new(-hx - 0.1, h - 0.1, 0.0),
        Vec3::new(hx + 0.1, h - 0.1, 0.0),
        0.1,
    );
    shape
}

/// La carreta: una caja entre dos ruedas altísimas, un toldo de paja arqueado
/// encima y su larga lanza apoyada en el suelo por delante.
pub(crate) fn cart(structure: &Structure) -> Shape {
    let mut shape = Shape::default();
    let size = local_size(structure);
    let (hx, hz) = (size.x / 2.0, size.y / 2.0);
    let radius = (structure.height / 2.0).min(1.25);
    let axle = Vec3::new(-0.4, radius, 0.0);
    for z in [-hz + 0.12, hz - 0.12] {
        wheel(&mut shape, axle.with_z(z), radius);
    }
    shape.beam(
        Surface::Planks,
        axle.with_z(-hz + 0.05),
        axle.with_z(hz - 0.05),
        0.12,
    );
    // La caja: un piso, estacas y tablas laterales.
    let (b0, b1) = (-hx + 0.3, 1.2);
    let bed = radius + 0.1;
    let bw = hz - 0.35;
    shape.cuboid(
        Surface::Planks,
        Transform::from_xyz((b0 + b1) / 2.0, bed, 0.0).with_scale(Vec3::new(
            b1 - b0,
            0.1,
            2.0 * bw,
        )),
        true,
    );
    for z in [-bw, bw] {
        for k in 0..5 {
            let x = b0 + (b1 - b0) * k as f32 / 4.0;
            shape.beam(
                Surface::Planks,
                Vec3::new(x, bed, z),
                Vec3::new(x, bed + 0.7, z),
                0.07,
            );
        }
        for y in [0.25, 0.55] {
            shape.beam(
                Surface::Planks,
                Vec3::new(b0, bed + y, z),
                Vec3::new(b1, bed + y, z),
                0.05,
            );
        }
    }
    // El toldo: un arco de paja sobre aros, abierto en ambos extremos.
    let arch = 8;
    let at = |k: usize, x: f32| {
        let a = k as f32 / arch as f32 * std::f32::consts::PI;
        Vec3::new(x, bed + 0.7 + a.sin() * 0.75, a.cos() * (bw + 0.05))
    };
    let (t0, t1) = (b0 + 0.1, b1 - 0.1);
    for k in 0..arch {
        let (p, q) = (at(k, t0), at(k + 1, t0));
        let (p1, q1) = (at(k, t1), at(k + 1, t1));
        shape.face(Surface::Thatch, &[p, p1, q1, q]);
        shape.face(Surface::Thatch, &[p, q, q1, p1]);
    }
    // La lanza, con el extremo lejano apoyado en el suelo.
    shape.beam(
        Surface::Planks,
        Vec3::new(b1 - 0.6, bed - 0.05, 0.0),
        Vec3::new(hx - 0.1, 0.15, 0.0),
        0.12,
    );
    shape
}

/// Una rueda de rayos alrededor de `hub` (eje a lo largo de `Z`).
fn wheel(shape: &mut Shape, hub: Vec3, radius: f32) {
    let rim = 20;
    for k in 0..rim {
        let (a, b) = (
            k as f32 / rim as f32 * TAU,
            (k + 1) as f32 / rim as f32 * TAU,
        );
        shape.beam(
            Surface::Planks,
            hub + Vec3::new(a.cos(), a.sin(), 0.0) * radius,
            hub + Vec3::new(b.cos(), b.sin(), 0.0) * radius,
            0.11,
        );
    }
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU;
        let dir = Vec3::new(a.cos(), a.sin(), 0.0);
        shape.beam(
            Surface::Planks,
            hub + dir * 0.18,
            hub + dir * (radius - 0.05),
            0.05,
        );
    }
    shape.lathe(
        Surface::Planks,
        Transform::from_translation(hub).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
        &[
            (0.0, -0.18),
            (0.16, -0.18),
            (0.2, 0.0),
            (0.16, 0.18),
            (0.0, 0.18),
        ],
        12,
    );
}

/// El accesorio `structure` en su propio espacio, para todos los tipos salvo
/// las casas, los galpones, los corrales, los árboles y la rueda del molino.
/// Una carreta abandonada sin su modelo se dibuja como carreta.
pub(crate) fn prop(structure: &Structure, drop: f32) -> Shape {
    match structure.kind {
        StructureKind::Windpump => windpump(structure, drop),
        StructureKind::Tank => tank(structure, drop),
        StructureKind::Trough => trough(structure, drop),
        StructureKind::Well => well(structure, drop),
        StructureKind::Haystack => haystack(structure, drop),
        StructureKind::HitchingPost => hitching_post(structure, drop),
        StructureKind::Cart | StructureKind::WreckedWagon => cart(structure),
        _ => Shape::default(),
    }
}
