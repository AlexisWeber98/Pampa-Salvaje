//! Una plaza pampeana de 1919, vestida según el [`Paving`] de su pueblo a
//! partir del [`PlazaFurniture`] que dispone core: senderos con cordones bajos,
//! un cruce pavimentado, un monumento al estilo de la Pirámide de Mayo (una
//! base escalonada, un pedestal bajo una cornisa, un obelisco y un remate),
//! bancos de listones sobre laterales de hierro fundido, y una verja baja
//! alrededor del borde.
//!
//! - **Ciudad** ([`Paving::Everywhere`], General Pico): senderos de ladrillo
//!   con cordones de revoque, un monumento de unos 8.8 m sobre tres escalones,
//!   tres bancos por lado y una verja de hierro sobre un zócalo de revoque
//!   entre pilares.
//! - **Pueblo** ([`Paving::Plaza`], Castex): senderos de tierra compactada con
//!   cordones de ladrillo, un monumento de unos 6.4 m sobre dos escalones, un
//!   banco junto a cada sendero (uno por cuadrante), sin verja.
//! - **Aldea** ([`Paving::None`], Metileo): senderos de tierra sin cordones, un
//!   monumento pequeño de unos 4.5 m, dos bancos, sin verja.
//!
//! Todo se construye alrededor del centro de la plaza sobre el suelo, con `x`
//! hacia el este, `y` hacia arriba y `z` hacia el sur (los ejes del mundo),
//! dentro de dos [`Shape`]: el [`PlazaShapes::ground`] (senderos, cordones,
//! cruce y monumento, dibujado hasta la distancia de los edificios) y el
//! [`PlazaShapes::furniture`] (bancos y verja, dibujado hasta la distancia de
//! los accesorios pequeños). Cada uno termina como una mesh por [`Surface`].

use bevy::prelude::*;
use pampa_core::town::{Bench, Paving, PlazaFurniture};
use pampa_core::{Aabb, PLAZA_PATH_WIDTH, Side, Vec2 as GroundVec2};

use super::italianate::{CHAMFER, urn};
use super::materials::Surface;
use super::shape::Shape;
use crate::coords::world_from_ground;

/// Parte superior del césped de la plaza sobre el suelo, en metros (el parche
/// de césped del pueblo).
pub const LAWN_TOP: f32 = 0.05;
/// Parte superior de los senderos y del cruce sobre el suelo, en metros: como
/// antes.
pub const PATH_TOP: f32 = 0.07;
/// Parte superior del cordón de un sendero sobre el suelo, en metros: 3 cm por
/// encima del sendero.
pub const PATH_CURB_TOP: f32 = 0.10;
/// Ancho del cordón de un sendero, sobre el césped de al lado, en metros.
pub const PATH_CURB: f32 = 0.15;
/// Cuánto se meten los senderos y los cordones bajo el suelo, en metros.
const FOOT: f32 = -0.05;

/// Las meshes de una plaza, alrededor de su centro sobre el suelo.
#[derive(Debug, Default)]
pub(crate) struct PlazaShapes {
    /// Senderos, cordones, cruce y monumento.
    pub ground: Shape,
    /// Bancos y verja.
    pub furniture: Shape,
}

/// Las superficies que lleva una plaza de `paving` para sus senderos y sus
/// cordones.
pub(crate) fn path_surfaces(paving: Paving) -> (Surface, Option<Surface>) {
    match paving {
        Paving::Everywhere => (Surface::Paving, Some(Surface::Stucco)),
        Paving::Plaza => (Surface::Earth, Some(Surface::Brick)),
        Paving::None => (Surface::Earth, None),
    }
}

/// Construye la plaza que dispone `f`.
pub(crate) fn plaza(f: &PlazaFurniture) -> PlazaShapes {
    let mut s = PlazaShapes::default();
    let c = f.area.center();
    paths(&mut s.ground, f, c);
    monument(&mut s.ground, f.paving, PATH_TOP);
    for b in &f.benches {
        let at = world_from_ground(b.at - c, LAWN_TOP);
        let mut one = Shape::default();
        bench(&mut one);
        s.furniture.merge(
            one,
            Transform::from_translation(at).with_rotation(facing(b.facing)),
        );
    }
    railing(&mut s.furniture, f, c);
    s
}

/// El giro que lleva el `+Z` de una pieza (su frente) a mirar hacia `side`.
fn facing(side: Side) -> Quat {
    let n = pampa_core::places::side_normal(side);
    let d = world_from_ground(n, 0.0);
    // Un giro solo alrededor de `y`: media vuelta no debe darla vuelta.
    Quat::from_rotation_y(d.x.atan2(d.z))
}

/// La caja sobre el área del suelo `area` desde `low` hasta `high`, alrededor
/// del punto del suelo `c`.
fn ground_block(
    shape: &mut Shape,
    surface: Surface,
    area: Aabb,
    c: GroundVec2,
    low: f32,
    high: f32,
) {
    let (p, q) = (
        world_from_ground(area.min - c, 0.0),
        world_from_ground(area.max - c, 0.0),
    );
    shape.block(surface, p.min(q).with_y(low), p.max(q).with_y(high));
}

/// El cruce, los cuatro senderos que salen de él hasta el borde de la plaza y
/// sus cordones del lado del césped (ninguno en una aldea).
fn paths(shape: &mut Shape, f: &PlazaFurniture, c: GroundVec2) {
    let (path, curb) = path_surfaces(f.paving);
    let (area, x) = (f.area, f.crossing);
    let h = PLAZA_PATH_WIDTH / 2.0;
    ground_block(shape, path, x, c, FOOT, PATH_TOP);
    let arms = [
        Aabb::new(
            GroundVec2::new(x.max.x, c.y - h),
            GroundVec2::new(area.max.x, c.y + h),
        ),
        Aabb::new(
            GroundVec2::new(area.min.x, c.y - h),
            GroundVec2::new(x.min.x, c.y + h),
        ),
        Aabb::new(
            GroundVec2::new(c.x - h, x.max.y),
            GroundVec2::new(c.x + h, area.max.y),
        ),
        Aabb::new(
            GroundVec2::new(c.x - h, area.min.y),
            GroundVec2::new(c.x + h, x.min.y),
        ),
    ];
    for arm in arms {
        ground_block(shape, path, arm, c, FOOT, PATH_TOP);
    }
    let Some(curb) = curb else {
        return;
    };
    let w = PATH_CURB;
    let mut curbs = Vec::new();
    // A lo largo de cada brazo, a ambos lados, desde el cordón del cruce hasta
    // el borde.
    for (lo, hi) in [(x.max.x + w, area.max.x), (area.min.x, x.min.x - w)] {
        curbs.push(Aabb::new(
            GroundVec2::new(lo, c.y + h),
            GroundVec2::new(hi, c.y + h + w),
        ));
        curbs.push(Aabb::new(
            GroundVec2::new(lo, c.y - h - w),
            GroundVec2::new(hi, c.y - h),
        ));
    }
    for (lo, hi) in [(x.max.y + w, area.max.y), (area.min.y, x.min.y - w)] {
        curbs.push(Aabb::new(
            GroundVec2::new(c.x + h, lo),
            GroundVec2::new(c.x + h + w, hi),
        ));
        curbs.push(Aabb::new(
            GroundVec2::new(c.x - h - w, lo),
            GroundVec2::new(c.x - h, hi),
        ));
    }
    // Alrededor del cruce, entre los brazos, esquinas incluidas.
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let corner = c + GroundVec2::new(sx, sy) * (half(x) + w);
        let inner = c + GroundVec2::new(sx * h, sy * h);
        let edge = c + GroundVec2::new(sx, sy) * half(x);
        // El tramo a lo largo de x (su lado norte o sur), y después a lo largo
        // de y.
        curbs.push(span(
            GroundVec2::new(inner.x, edge.y),
            GroundVec2::new(corner.x, corner.y),
        ));
        curbs.push(span(
            GroundVec2::new(edge.x, inner.y),
            GroundVec2::new(corner.x, edge.y),
        ));
    }
    for a in curbs {
        ground_block(shape, curb, a, c, FOOT, PATH_CURB_TOP);
    }
}

/// La mitad del lado de un cuadrado `a`.
fn half(a: Aabb) -> f32 {
    a.size().x / 2.0
}

/// La caja entre dos esquinas del suelo, en cualquier orden.
fn span(a: GroundVec2, b: GroundVec2) -> Aabb {
    Aabb::new(
        GroundVec2::new(a.x.min(b.x), a.y.min(b.y)),
        GroundVec2::new(a.x.max(b.x), a.y.max(b.y)),
    )
}

/// Las proporciones de un monumento, en metros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Proportions {
    /// El lado y la altura de cada escalón, desde el más bajo.
    pub steps: &'static [(f32, f32)],
    /// El lado y la altura del pedestal.
    pub pedestal: (f32, f32),
    /// El lado de la cornisa y su altura sobre el pedestal.
    pub cornice: (f32, f32),
    /// El dado (ático) sobre la cornisa: lado y altura.
    pub die: (f32, f32),
    /// El lado del obelisco en su pie y en su punta, y su altura.
    pub shaft: (f32, f32, f32),
    /// La altura del piramidión sobre el fuste.
    pub point: f32,
    /// Macetones en las esquinas de la cornisa.
    pub urns: bool,
}

impl Proportions {
    /// El monumento de una plaza de `paving`: más imponente a medida que crece
    /// el pueblo. Su escalón más bajo tiene [`PlazaFurniture::monument_side`]
    /// de ancho.
    pub fn of(paving: Paving) -> Self {
        match paving {
            Paving::None => Self {
                steps: &[(2.4, 0.25)],
                pedestal: (1.5, 0.9),
                cornice: (1.7, 0.15),
                die: (1.0, 0.25),
                shaft: (0.7, 0.45, 2.2),
                point: 0.35,
                urns: false,
            },
            Paving::Plaza => Self {
                steps: &[(3.0, 0.25), (2.5, 0.25)],
                pedestal: (1.8, 1.3),
                cornice: (2.05, 0.2),
                die: (1.3, 0.4),
                shaft: (0.85, 0.55, 3.2),
                point: 0.45,
                urns: false,
            },
            Paving::Everywhere => Self {
                steps: &[(3.0, 0.25), (2.6, 0.25), (2.2, 0.25)],
                pedestal: (1.9, 1.7),
                cornice: (2.2, 0.25),
                die: (1.5, 0.5),
                shaft: (1.0, 0.6, 4.6),
                point: 0.6,
                urns: true,
            },
        }
    }

    /// Altura de la punta del remate sobre el pie del monumento.
    #[cfg(test)]
    pub fn height(&self) -> f32 {
        let steps: f32 = self.steps.iter().map(|s| s.1).sum();
        steps + self.pedestal.1 + self.cornice.1 + self.die.1 + self.shaft.2 + self.point + FINIAL
    }
}

/// Altura del remate del monumento (una esfera sobre un collar), en metros.
const FINIAL: f32 = 0.35;

/// El monumento de una plaza de `paving`, apoyado en `foot` sobre el centro de
/// la plaza: escalones de revoque, un pedestal encalado sobre una basa
/// moldurada con una placa de bronce en cada cara, una cornisa de revoque, un
/// dado, un obelisco encalado que se angosta hacia arriba con su piramidión, y
/// un remate de hierro.
pub(crate) fn monument(shape: &mut Shape, paving: Paving, foot: f32) {
    let p = Proportions::of(paving);
    let mut y = foot;
    let square = |side: f32, y0: f32, y1: f32| {
        let h = side / 2.0;
        (Vec3::new(-h, y0, -h), Vec3::new(h, y1, h))
    };
    for &(side, rise) in p.steps {
        let (lo, hi) = square(side, y - 0.05, y + rise);
        shape.chamfered(Surface::Stucco, lo, hi, 0.12);
        y += rise;
    }
    // Pedestal: una basa moldurada, el dado del pedestal y las placas.
    let (side, height) = p.pedestal;
    let (lo, hi) = square(side + 0.12, y, y + 0.18);
    shape.chamfered(Surface::Stucco, lo, hi, CHAMFER);
    let (lo, hi) = square(side, y, y + height);
    shape.chamfered(Surface::Whitewash, lo, hi, CHAMFER);
    if paving != Paving::None {
        let (pw, ph) = (side * 0.5, height * 0.4);
        for k in 0..4 {
            let turn = Quat::from_rotation_y(k as f32 * std::f32::consts::FRAC_PI_2);
            let at = turn * Vec3::new(0.0, y + height * 0.55, side / 2.0 + 0.01);
            shape.cuboid(
                Surface::Bronze,
                Transform::from_translation(at)
                    .with_rotation(turn)
                    .with_scale(Vec3::new(pw, ph, 0.03)),
                false,
            );
        }
    }
    y += height;
    // Cornisa: una faja bajo un goterón más ancho.
    let (side, depth) = p.cornice;
    let (lo, hi) = square(side - 0.12, y, y + depth * 0.45);
    shape.chamfered(Surface::Stucco, lo, hi, CHAMFER);
    let (lo, hi) = square(side, y + depth * 0.45, y + depth);
    shape.chamfered(Surface::Stucco, lo, hi, CHAMFER);
    y += depth;
    if p.urns {
        let r = side / 2.0 - 0.2;
        for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            urn(shape, Surface::Stucco, Vec3::new(sx * r, y, sz * r));
        }
    }
    let (side, height) = p.die;
    let (lo, hi) = square(side, y, y + height);
    shape.chamfered(Surface::Whitewash, lo, hi, CHAMFER);
    y += height;
    let (lo, hi) = square(side + 0.1, y - 0.06, y);
    shape.block(Surface::Stucco, lo, hi);
    // El obelisco, con sus cuatro caras inclinadas hacia adentro, y después su
    // piramidión.
    let (bottom, top, tall) = p.shaft;
    frustum(shape, Surface::Whitewash, y, bottom / 2.0, top / 2.0, tall);
    y += tall;
    super::italianate::spire(shape, Surface::Whitewash, Vec3::Y * y, top, p.point);
    y += p.point;
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(Vec3::Y * (y - 0.08)),
        &[
            (0.05, 0.0),
            (0.03, 0.1),
            (0.11, 0.17),
            (0.12, 0.24),
            (0.08, 0.33),
            (0.0, FINIAL + 0.08),
        ],
        8,
    );
}

/// Un tronco de pirámide cuadrado alrededor del eje `y` desde `y0`, con
/// semianchos `bottom` y `top`, de `height` de alto, sin tapas.
fn frustum(shape: &mut Shape, surface: Surface, y0: f32, bottom: f32, top: f32, height: f32) {
    let ring = |h: f32, y: f32| {
        [
            Vec3::new(-h, y, h),
            Vec3::new(h, y, h),
            Vec3::new(h, y, -h),
            Vec3::new(-h, y, -h),
        ]
    };
    let (a, b) = (ring(bottom, y0), ring(top, y0 + height));
    for k in 0..4 {
        let n = (k + 1) % 4;
        shape.face(surface, &[a[k], a[n], b[n], b[k]]);
    }
}

/// Altura del asiento de un banco, en metros.
pub(crate) const SEAT: f32 = 0.44;

/// Un banco de listones en su propio espacio: [`Bench::LENGTH`] a lo largo de
/// `x`, con el frente hacia `+z`, dentro de [`Bench::DEPTH`] de adelante hacia
/// atrás, sobre el suelo en `y = 0`. Dos laterales de hierro fundido (patas,
/// soporte del asiento, soporte del respaldo y apoyabrazos) sostienen cuatro
/// listones de asiento y tres de respaldo.
pub(crate) fn bench(shape: &mut Shape) {
    let (half, d) = (Bench::LENGTH / 2.0, Bench::DEPTH / 2.0);
    let iron = 0.05;
    for x in [-(half - 0.12), half - 0.12] {
        let p = |y: f32, z: f32| Vec3::new(x, y, z);
        let beams = [
            (p(0.0, d - 0.06), p(SEAT, d - 0.12)),
            (p(0.0, -d + 0.04), p(SEAT, -d + 0.16)),
            (p(SEAT - 0.03, d - 0.08), p(SEAT - 0.03, -d + 0.12)),
            (p(SEAT, -d + 0.14), p(0.86, -d + 0.04)),
            (p(0.64, d - 0.08), p(0.62, -d + 0.1)),
            (p(SEAT, d - 0.1), p(0.64, d - 0.1)),
        ];
        for (a, b) in beams {
            shape.beam(Surface::Steel, a, b, iron);
        }
    }
    // Listones del asiento, de adelante hacia atrás.
    let length = Bench::LENGTH - 0.04;
    for k in 0..4 {
        let z = d - 0.1 - k as f32 * 0.095;
        shape.block(
            Surface::Planks,
            Vec3::new(-length / 2.0, SEAT, z - 0.04),
            Vec3::new(length / 2.0, SEAT + 0.035, z + 0.04),
        );
    }
    // Listones del respaldo, inclinados hacia atrás con el soporte de hierro.
    let lean = Quat::from_rotation_x(-0.25);
    for k in 0..3 {
        let y = SEAT + 0.15 + k as f32 * 0.11;
        let z = -d + 0.12 - (y - SEAT) * 0.25;
        shape.cuboid(
            Surface::Planks,
            Transform::from_xyz(0.0, y, z)
                .with_rotation(lean)
                .with_scale(Vec3::new(length, 0.08, 0.03)),
            true,
        );
    }
}

/// Altura del zócalo de revoque de la verja, en metros.
pub(crate) const RAILING_PLINTH: f32 = 0.35;
/// Altura del pasamanos superior de la verja sobre el césped, en metros.
pub(crate) const RAILING_TOP: f32 = 1.05;

/// La verja de la ciudad (solo la plaza de una ciudad tiene una): barrotes de
/// hierro entre dos barras sobre un zócalo de revoque a lo largo de cada una de
/// las piezas de `f`, con pilares de revoque en las esquinas y en las entradas
/// de los senderos.
fn railing(shape: &mut Shape, f: &PlazaFurniture, c: GroundVec2) {
    if f.railing.is_empty() {
        return;
    }
    let top = LAWN_TOP + RAILING_TOP;
    let base = LAWN_TOP + RAILING_PLINTH;
    for piece in &f.railing {
        let size = piece.size();
        let along_x = size.x >= size.y;
        let (lo, hi) = (
            world_from_ground(piece.min - c, 0.0),
            world_from_ground(piece.max - c, 0.0),
        );
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let mid = (lo + hi) / 2.0;
        let length = if along_x { size.x } else { size.y };
        let dir = if along_x { Vec3::X } else { Vec3::Z };
        let start = mid - dir * (length / 2.0);
        let t = 0.16;
        let across = Vec3::ONE - dir;
        let (a, b) = (mid - across * (t / 2.0), mid + across * (t / 2.0));
        let (a, b) = (a - dir * (length / 2.0), b + dir * (length / 2.0));
        shape.block(Surface::Stucco, a.with_y(LAWN_TOP - 0.05), b.with_y(base));
        // Dos barras, y después barrotes entre ellas y más allá de la superior.
        for y in [base + 0.08, top - 0.08] {
            shape.beam(
                Surface::Steel,
                start.with_y(y),
                (start + dir * length).with_y(y),
                0.03,
            );
        }
        let count = (length / PICKET_PITCH).floor().max(1.0) as usize;
        for k in 1..count {
            let at = start + dir * (k as f32 * length / count as f32);
            shape.cuboid(
                Surface::Steel,
                Transform::from_translation(at.with_y((base + top) / 2.0 + 0.02))
                    .with_scale(Vec3::new(0.02, top - base + 0.04, 0.02)),
                false,
            );
        }
        // Un poste grueso cada pocos metros mantiene derecho el tramo.
        let posts = (length / 2.5).floor() as usize;
        for k in 1..posts {
            let at = start + dir * (k as f32 * length / posts as f32);
            shape.block(
                Surface::Steel,
                (at - Vec3::new(0.03, 0.0, 0.03)).with_y(base - 0.02),
                (at + Vec3::new(0.03, 0.0, 0.03)).with_y(top + 0.04),
            );
        }
    }
    // Pilares en las esquinas y a ambos lados de cada entrada, todos dentro del
    // collider de la verja.
    for p in stands(f, PIER) {
        let at = world_from_ground(p - c, 0.0);
        let w = PIER - 0.01;
        shape.chamfered(
            Surface::Stucco,
            (at - Vec3::new(w, 0.0, w)).with_y(LAWN_TOP - 0.05),
            (at + Vec3::new(w, 0.0, w)).with_y(top + 0.15),
            CHAMFER,
        );
        shape.block(
            Surface::Stucco,
            (at - Vec3::splat(PIER)).with_y(top + 0.15),
            (at + Vec3::splat(PIER)).with_y(top + 0.21),
        );
    }
}

/// Espacio entre los barrotes de la verja, en metros.
const PICKET_PITCH: f32 = 0.25;

/// La mitad del lado de un pilar de revoque de la verja de la ciudad (su
/// capitel), en metros.
pub(crate) const PIER: f32 = 0.14;

/// Dónde tiene la verja de `f` sus pilares o postes gruesos, de `half` de ancho
/// hacia cada lado: las cuatro esquinas y ambos lados de la entrada de cada
/// sendero, justo al costado de la entrada.
pub(crate) fn stands(f: &PlazaFurniture, half: f32) -> Vec<GroundVec2> {
    let area = f.area;
    let inset = PlazaFurniture::RAILING_INSET + PlazaFurniture::RAILING_DEPTH / 2.0;
    let (lo, hi) = (
        area.min + GroundVec2::new(inset, inset),
        area.max - GroundVec2::new(inset, inset),
    );
    let mut out = vec![
        lo,
        hi,
        GroundVec2::new(lo.x, hi.y),
        GroundVec2::new(hi.x, lo.y),
    ];
    let (m, mouth) = (area.center(), PlazaFurniture::MOUTH_HALF + half);
    for s in [-1.0, 1.0] {
        let (x, y) = (m.x + s * mouth, m.y + s * mouth);
        out.extend([
            GroundVec2::new(x, lo.y),
            GroundVec2::new(x, hi.y),
            GroundVec2::new(lo.x, y),
            GroundVec2::new(hi.x, y),
        ]);
    }
    out
}

#[cfg(test)]
mod tests;
