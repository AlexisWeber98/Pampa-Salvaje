//! Los muebles del almacén de ramos generales: los estantes hasta el techo con
//! su mercadería y su escalera, el mostrador largo con la balanza, la vitrina
//! y el rollo de papel, las bolsas de yerba, harina y azúcar, el caballete de
//! las monturas con los lazos colgados y el tablero de las herramientas.
//!
//! Cada mueble está arrimado a una pared de la sala ([`Against`]) y se dibuja
//! en su propio marco: `s` a lo largo de la pared, `d` desde ella hacia la
//! sala e `y` desde el piso, así que el mismo dibujo sirve contra cualquier
//! pared.

use bevy::prelude::*;

use super::{
    ROOM_FLOOR, Rect, SHELF_THICK, WOOD, board, bottle, cash_register, counter_body, demijohn,
    gin_bottle, pick, solid,
};
use crate::facade::Surface;
use crate::facade::materials::{Livery, Paint};
use crate::facade::shape::Shape;

/// Contra qué pared de la sala está un mueble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wall {
    /// La del fondo (`z0`).
    Back,
    /// La del frente (`z1`).
    Front,
    /// La de `x0`.
    Left,
    /// La de `x1`.
    Right,
}

/// Un mueble arrimado a una pared de la sala, para dibujarlo en su marco: `s`
/// a lo largo de la pared, `d` desde ella hacia la sala, `y` desde el piso.
#[derive(Debug, Clone, Copy)]
pub(super) struct Against {
    r: Rect,
    wall: Wall,
}

impl Against {
    /// `r`, arrimado a la pared de `floor` que toca a lo largo (la más
    /// cercana; si toca dos, aquella a lo largo de la cual es más largo).
    pub(super) fn of(r: Rect, floor: Rect) -> Self {
        let walls = [
            (Wall::Back, r.z0 - floor.z0, r.x1 - r.x0),
            (Wall::Front, floor.z1 - r.z1, r.x1 - r.x0),
            (Wall::Left, r.x0 - floor.x0, r.z1 - r.z0),
            (Wall::Right, floor.x1 - r.x1, r.z1 - r.z0),
        ];
        let near = walls.iter().map(|w| w.1).fold(f32::INFINITY, f32::min);
        let wall = walls
            .iter()
            .filter(|w| w.1 <= near + 0.05)
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .map_or(Wall::Back, |w| w.0);
        Self { r, wall }
    }

    /// Cuánto mide a lo largo de su pared.
    fn len(&self) -> f32 {
        match self.wall {
            Wall::Back | Wall::Front => self.r.x1 - self.r.x0,
            Wall::Left | Wall::Right => self.r.z1 - self.r.z0,
        }
    }

    /// Cuánto entra desde su pared hacia la sala.
    fn depth(&self) -> f32 {
        match self.wall {
            Wall::Back | Wall::Front => self.r.z1 - self.r.z0,
            Wall::Left | Wall::Right => self.r.x1 - self.r.x0,
        }
    }

    /// El punto `(s, d)` a la altura `y` sobre el piso, en el espacio propio
    /// del edificio.
    fn at(&self, s: f32, d: f32, y: f32) -> Vec3 {
        let r = self.r;
        let y = ROOM_FLOOR + y;
        match self.wall {
            Wall::Back => Vec3::new(r.x0 + s, y, r.z0 + d),
            Wall::Front => Vec3::new(r.x1 - s, y, r.z1 - d),
            Wall::Left => Vec3::new(r.x0 + d, y, r.z1 - s),
            Wall::Right => Vec3::new(r.x1 - d, y, r.z0 + s),
        }
    }

    /// Una caja de `s0..s1`, `d0..d1` e `y0..y1`.
    fn solid(
        &self,
        shape: &mut Shape,
        surface: Surface,
        s: (f32, f32),
        d: (f32, f32),
        y: (f32, f32),
    ) {
        let (a, b) = (self.at(s.0, d.0, y.0), self.at(s.1, d.1, y.1));
        solid(shape, surface, a.min(b), a.max(b));
    }

    /// Una caja biselada de `s0..s1`, `d0..d1` e `y0..y1`.
    fn chamfered(
        &self,
        shape: &mut Shape,
        surface: Surface,
        s: (f32, f32),
        d: (f32, f32),
        y: (f32, f32),
        chamfer: f32,
    ) {
        let (a, b) = (self.at(s.0, d.0, y.0), self.at(s.1, d.1, y.1));
        shape.chamfered(surface, a.min(b), a.max(b), chamfer);
    }

    /// Una barra de `a` a `b`, cada uno `(s, d, y)`.
    fn beam(
        &self,
        shape: &mut Shape,
        surface: Surface,
        a: (f32, f32, f32),
        b: (f32, f32, f32),
        thick: f32,
    ) {
        shape.beam(
            surface,
            self.at(a.0, a.1, a.2),
            self.at(b.0, b.1, b.2),
            thick,
        );
    }

    /// Una rotación que lleva el eje `+Z` del dibujo (de la pared hacia la
    /// sala) al de la pared.
    fn turn(&self) -> Quat {
        let toward = self.at(0.0, 1.0, 0.0) - self.at(0.0, 0.0, 0.0);
        Quat::from_rotation_arc(Vec3::Z, toward.normalize())
    }
}

/// Las latas de colores de la mercadería: pintura de lata, apagada por el
/// tiempo.
const TINS: [Surface; 5] = [
    Surface::Livery(Livery::BufferRed),
    Surface::Livery(Livery::Maroon),
    Surface::Painted(Paint::Ochre),
    Surface::Painted(Paint::GreyBlue),
    Surface::Livery(Livery::Black),
];

/// Las piezas de tela, en los colores de los géneros de entonces.
const CLOTH: [Surface; 4] = [
    Surface::Painted(Paint::GreyBlue),
    Surface::Painted(Paint::PaleRose),
    Surface::Painted(Paint::Terracotta),
    Surface::Painted(Paint::LimeWhite),
];

/// Cada cuánto va un estante de los altos, en metros.
const GOODS_STEP: f32 = 0.42;
/// Desde qué altura sobre el piso los estantes guardan solo cajas y paquetes,
/// lo que se baja con la escalera, en metros.
const HIGH_SHELF: f32 = 2.2;

/// Los estantes del almacén sobre `r`, contra su pared, de `top` de alto (sobre
/// el piso): un zócalo, el fondo de tablas, parantes cada metro, estantes
/// cada [`GOODS_STEP`] y una cornisa arriba. En cada tramo y estante, según la
/// semilla `v`, latas, paquetes de papel, piezas de tela apiladas, frascos de
/// vidrio, botellas o damajuanas; y si es la estantería larga, una escalera
/// apoyada para alcanzar lo de arriba.
pub(super) fn goods(shape: &mut Shape, wall: Against, top: f32, v: u32) {
    let (len, depth) = (wall.len(), wall.depth());
    wall.solid(
        shape,
        Surface::Barnwood,
        (0.0, len),
        (0.02, depth - 0.02),
        (0.0, 0.12),
    );
    wall.solid(shape, Surface::Planks, (0.0, len), (0.0, 0.02), (0.12, top));
    let bays = (len / 1.0).round().max(1.0) as usize;
    let step = len / bays as f32;
    for k in 0..=bays {
        let s = (step * k as f32).clamp(0.02, len - 0.02);
        wall.solid(
            shape,
            WOOD,
            (s - 0.02, s + 0.02),
            (0.0, depth),
            (0.0, top - 0.12),
        );
    }
    wall.solid(
        shape,
        Surface::Door,
        (0.0, len),
        (0.0, depth + 0.05),
        (top - 0.12, top),
    );
    let rows = (((top - 0.3) - 0.12) / GOODS_STEP).floor().max(1.0) as usize;
    for row in 0..=rows {
        let y = 0.12 + GOODS_STEP * row as f32;
        wall.solid(
            shape,
            Surface::Planks,
            (0.0, len),
            (0.02, depth),
            (y, y + SHELF_THICK),
        );
    }
    let mid = depth / 2.0 + 0.01;
    for bay in 0..bays {
        let (a, b) = (step * bay as f32 + 0.05, step * (bay + 1) as f32 - 0.05);
        for row in 0..=rows {
            let y = 0.12 + GOODS_STEP * row as f32 + SHELF_THICK;
            // Lo que cabe entre este estante y el de arriba (o la cornisa).
            let room = if row == rows {
                top - 0.12 - y
            } else {
                GOODS_STEP - SHELF_THICK
            };
            if room < 0.15 {
                continue;
            }
            let (bay, row) = (bay as u32, row as u32);
            let seed = row * 8;
            let fill =
                |x: f32, k: u32| a + 0.04 + pick(bay, seed + k, v) * (b - a - 0.08 - x).max(0.0);
            // Arriba, una hilera de cajones y bultos; a mano, de todo.
            if y > HIGH_SHELF {
                if pick(bay, seed, v) < 0.25 {
                    continue;
                }
                let n = 1 + (pick(bay, seed + 1, v) * 3.0) as usize;
                let wide = 0.36;
                let start = fill(wide * n as f32, 2);
                let h = (0.3f32).min(room - 0.05);
                for k in 0..n {
                    let s0 = start + wide * k as f32;
                    let crate_ = pick(bay * 5 + k as u32, seed + 5, v) < 0.5;
                    let (surface, d) = if crate_ {
                        (Surface::Planks, (0.04, depth - 0.03))
                    } else {
                        (Surface::Painted(Paint::Ochre), (mid - 0.15, mid + 0.15))
                    };
                    wall.solid(shape, surface, (s0, s0 + wide - 0.03), d, (y, y + h));
                }
                continue;
            }
            let item = (pick(bay, seed, v) * 7.0) as u32;
            match item {
                // Una fila de latas de colores.
                0 | 1 => {
                    let n = 3 + (pick(bay, seed + 1, v) * 5.0) as usize;
                    let gap = 0.12;
                    let start = fill(gap * n as f32, 2);
                    let tin =
                        TINS[(pick(bay, seed + 3, v) * TINS.len() as f32) as usize % TINS.len()];
                    let h = (0.16f32).min(room - 0.04);
                    for k in 0..n {
                        let at = wall.at(start + gap * k as f32 + 0.05, mid, y);
                        shape.lathe(
                            tin,
                            Transform::from_translation(at),
                            &[(0.05, 0.0), (0.05, h), (0.0, h)],
                            6,
                        );
                    }
                }
                // Paquetes de papel atados, de a dos pisos.
                2 => {
                    let n = 2 + (pick(bay, seed + 1, v) * 3.0) as usize;
                    let start = fill(0.2 * n as f32, 2);
                    let h = (0.14f32).min((room - 0.04) / 2.0);
                    for k in 0..n {
                        let s0 = start + 0.2 * k as f32;
                        for layer in 0..2 {
                            let y0 = y + h * layer as f32;
                            let paper = if (k + layer) % 3 == 0 {
                                Surface::Painted(Paint::Ochre)
                            } else {
                                Surface::Painted(Paint::LimeWhite)
                            };
                            wall.solid(
                                shape,
                                paper,
                                (s0, s0 + 0.17),
                                (mid - 0.12, mid + 0.12),
                                (y0, y0 + h - 0.005),
                            );
                        }
                    }
                }
                // Piezas de tela apiladas, cada una de un color.
                3 => {
                    let stacks = 1 + (pick(bay, seed + 1, v) * 2.0) as usize;
                    let start = fill(0.42 * stacks as f32, 2);
                    let h = 0.09;
                    let layers = (((room - 0.04) / h) as usize).min(3);
                    for k in 0..stacks {
                        let s0 = start + 0.42 * k as f32;
                        for layer in 0..layers {
                            let cloth = CLOTH[(pick(bay * 3 + k as u32, seed + 4 + layer as u32, v)
                                * 4.0) as usize
                                % 4];
                            let y0 = y + h * layer as f32;
                            wall.chamfered(
                                shape,
                                cloth,
                                (s0, s0 + 0.38),
                                (0.05, depth - 0.04),
                                (y0, y0 + h - 0.005),
                                0.025,
                            );
                        }
                    }
                }
                // Frascos de vidrio con tapa de lata.
                4 => {
                    let n = 2 + (pick(bay, seed + 1, v) * 4.0) as usize;
                    let gap = 0.17;
                    let start = fill(gap * n as f32, 2);
                    let h = (0.24f32).min(room - 0.06);
                    for k in 0..n {
                        let at = wall.at(start + gap * k as f32 + 0.07, mid, y);
                        let place = Transform::from_translation(at);
                        shape.lathe(
                            Surface::Glass,
                            place,
                            &[(0.065, 0.0), (0.07, h - 0.04), (0.05, h - 0.02)],
                            6,
                        );
                        shape.lathe(
                            Surface::Sheet,
                            place,
                            &[(0.052, h - 0.025), (0.052, h), (0.0, h)],
                            6,
                        );
                    }
                }
                // Botellas, o una ginebra, abajo una damajuana.
                5 => {
                    let n = 2 + (pick(bay, seed + 1, v) * 4.0) as usize;
                    let start = fill(0.09 * n as f32, 2);
                    let gin = pick(bay, seed + 3, v) < 0.35;
                    for k in 0..n {
                        let at = wall.at(start + 0.09 * k as f32 + 0.04, mid, y);
                        if row == 0 && k == 0 {
                            demijohn(shape, at);
                        } else if gin {
                            gin_bottle(shape, at);
                        } else if room > 0.32 {
                            bottle(shape, at);
                        }
                    }
                }
                // Un hueco: lo que se vendió.
                _ => {}
            }
        }
    }
    // La escalera de mano apoyada en la estantería larga, para alcanzar lo de
    // arriba.
    if len > 4.0 {
        let s = len * (0.3 + 0.4 * pick(1, 99, v));
        let (foot, head) = ((s, depth + 0.85, 0.0), (s, depth + 0.06, top - 0.3));
        for side in [-0.22, 0.22] {
            wall.beam(
                shape,
                WOOD,
                (foot.0 + side, foot.1, foot.2),
                (head.0 + side, head.1, head.2),
                0.045,
            );
        }
        let rungs = ((top - 0.6) / 0.32) as usize;
        for k in 1..=rungs {
            let t = k as f32 / (rungs + 1) as f32;
            let (d, y) = (foot.1 + (head.1 - foot.1) * t, head.2 * t);
            wall.beam(shape, WOOD, (s - 0.22, d, y), (s + 0.22, d, y), 0.03);
        }
    }
}

/// El mostrador largo del almacén sobre `r`, de `top` de alto, con el lado de
/// los clientes hacia la calle (`z1`): el mismo cuerpo de madera que el de la
/// pulpería, sin reja, y encima una balanza de platillos, una vitrina de
/// vidrio en un extremo y el rollo de papel de envolver en su soporte.
pub(super) fn shop_counter(shape: &mut Shape, r: Rect, top: f32) {
    counter_body(shape, r, top);
    let y = top;
    // La vitrina, en el extremo de `x0`: un cajón de vidrio con marco.
    let (c0, c1) = (r.x0 + 0.1, (r.x0 + 1.3).min(r.x1 - 0.5));
    let (z0, z1) = (r.z0 + 0.08, r.z1 - 0.06);
    let high = y + 0.38;
    solid(
        shape,
        Surface::Glass,
        Vec3::new(c0, y, z0),
        Vec3::new(c1, high, z1),
    );
    for (x, z) in [(c0, z0), (c0, z1), (c1, z0), (c1, z1)] {
        shape.beam(
            Surface::Door,
            Vec3::new(x, y, z),
            Vec3::new(x, high + 0.01, z),
            0.03,
        );
    }
    board(
        shape,
        Surface::Door,
        Rect {
            x0: c0 - 0.02,
            x1: c1 + 0.02,
            z0: z0 - 0.02,
            z1: z1 + 0.02,
        },
        high + 0.03,
        0.03,
    );
    for k in 0..4 {
        let x = c0 + 0.15 + 0.25 * k as f32;
        if x + 0.1 > c1 {
            break;
        }
        let tin = TINS[k % TINS.len()];
        shape.lathe(
            tin,
            Transform::from_xyz(x, y + 0.01, (z0 + z1) / 2.0),
            &[(0.06, 0.0), (0.06, 0.08), (0.0, 0.08)],
            8,
        );
    }
    // La balanza de platillos, en el medio.
    let mid = (r.x0 + r.x1) / 2.0;
    let z = (r.z0 + r.z1) / 2.0;
    solid(
        shape,
        Surface::Livery(Livery::Black),
        Vec3::new(mid - 0.2, y, z - 0.1),
        Vec3::new(mid + 0.2, y + 0.06, z + 0.1),
    );
    shape.beam(
        Surface::Bronze,
        Vec3::new(mid, y + 0.06, z),
        Vec3::new(mid, y + 0.3, z),
        0.025,
    );
    shape.beam(
        Surface::Bronze,
        Vec3::new(mid - 0.22, y + 0.3, z),
        Vec3::new(mid + 0.22, y + 0.3, z),
        0.015,
    );
    for side in [-0.22, 0.22] {
        let x = mid + side;
        shape.beam(
            Surface::Bronze,
            Vec3::new(x, y + 0.3, z),
            Vec3::new(x, y + 0.14, z),
            0.008,
        );
        shape.lathe(
            Surface::Bronze,
            Transform::from_xyz(x, y + 0.12, z),
            &[(0.0, 0.0), (0.1, 0.01), (0.11, 0.025)],
            10,
        );
    }
    // La caja registradora, entre la balanza y el rollo de papel, con las
    // teclas hacia el almacenero.
    cash_register(shape, Vec3::new((mid + r.x1 - 0.6) / 2.0, y, z - 0.05));
    // El rollo de papel de envolver, acostado en su soporte de hierro, cerca
    // del paso del almacenero.
    let px = r.x1 - 0.6;
    for side in [-0.32, 0.32] {
        shape.beam(
            Surface::Steel,
            Vec3::new(px + side, y, z),
            Vec3::new(px + side, y + 0.2, z),
            0.02,
        );
    }
    shape.lathe(
        Surface::Painted(Paint::Ochre),
        Transform::from_xyz(px - 0.28, y + 0.18, z)
            .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
        &[(0.0, 0.0), (0.08, 0.0), (0.08, 0.56), (0.0, 0.56)],
        10,
    );
}

/// Una pila de bolsas de arpillera sobre `r`, de `top` de alto: una hilera
/// acostada en el piso, otra encima y, en una punta, una bolsa abierta
/// parada, llena de harina hasta el borde.
pub(super) fn sacks(shape: &mut Shape, r: Rect, top: f32) {
    let height = top - ROOM_FLOOR;
    let lie = (height / 2.6).min(0.26);
    let (w, d) = (r.x1 - r.x0, r.z1 - r.z0);
    // La bolsa abierta va en la punta de `x1`.
    let open = 0.5;
    let n = ((w - open) / 0.62).floor().max(1.0) as usize;
    let each = (w - open) / n as f32;
    for k in 0..n {
        let x0 = r.x0 + each * k as f32;
        solid_sack(
            shape,
            Vec3::new(x0 + 0.02, ROOM_FLOOR, r.z0 + 0.05),
            Vec3::new(x0 + each - 0.02, ROOM_FLOOR + lie, r.z1 - 0.05),
        );
    }
    for k in 0..n.saturating_sub(1) {
        let x0 = r.x0 + each * (k as f32 + 0.5);
        let (y0, y1) = (ROOM_FLOOR + lie - 0.03, ROOM_FLOOR + 2.0 * lie - 0.03);
        solid_sack(
            shape,
            Vec3::new(x0 + 0.02, y0, r.z0 + 0.12),
            Vec3::new(x0 + each - 0.02, y1, r.z1 - 0.12),
        );
    }
    let (x, z) = (r.x1 - open / 2.0, (r.z0 + r.z1) / 2.0);
    let rad = (open / 2.0 - 0.03).min(d / 2.0 - 0.03);
    let tall = height.min(0.75);
    let place = Transform::from_xyz(x, ROOM_FLOOR, z);
    shape.lathe(
        Surface::Thatch,
        place,
        &[
            (rad * 0.9, 0.0),
            (rad, 0.08),
            (rad, tall - 0.12),
            (rad * 1.1, tall - 0.04),
            (rad * 1.15, tall),
        ],
        10,
    );
    // La harina, un poco abajo del borde arremangado.
    shape.lathe(
        Surface::Painted(Paint::LimeWhite),
        place,
        &[
            (rad * 0.98, tall - 0.1),
            (rad * 0.6, tall - 0.06),
            (0.0, tall - 0.04),
        ],
        10,
    );
}

/// Una bolsa acostada entre `min` y `max`, con las puntas redondeadas.
fn solid_sack(shape: &mut Shape, min: Vec3, max: Vec3) {
    let r = ((max - min).min_element() * 0.35).min(0.08);
    shape.chamfered(Surface::Thatch, min, max, r);
}

/// El caballete de las monturas contra su pared, de `top` de alto: un larguero
/// sobre dos pares de patas abiertas y dos recados de cuero encima, con sus
/// faldones y estribos; en la pared, dos lazos de cuero crudo enrollados en
/// sus clavijas.
pub(super) fn saddlery(shape: &mut Shape, wall: Against, top: f32) {
    let (len, depth) = (wall.len(), wall.depth());
    let height = top - ROOM_FLOOR;
    let d = depth / 2.0 + 0.05;
    let rail = height - 0.12;
    wall.beam(shape, WOOD, (0.1, d, rail), (len - 0.1, d, rail), 0.08);
    for s in [0.2, len - 0.2] {
        for side in [-0.25, 0.25] {
            wall.beam(shape, WOOD, (s, d + side, 0.0), (s, d, rail), 0.06);
        }
    }
    let leather = Surface::Livery(Livery::Maroon);
    let saddles = ((len - 0.4) / 0.85).floor().clamp(1.0, 2.0) as usize;
    let gap = (len - 0.2) / saddles as f32;
    for k in 0..saddles {
        let s = 0.1 + gap * (k as f32 + 0.5);
        // El asiento sobre el larguero.
        wall.chamfered(
            shape,
            leather,
            (s - 0.28, s + 0.28),
            (d - 0.2, d + 0.2),
            (rail + 0.04, rail + 0.16),
            0.05,
        );
        // Los bastos del frente y de atrás, más altos.
        for e in [-0.26, 0.2] {
            wall.chamfered(
                shape,
                leather,
                (s + e, s + e + 0.06),
                (d - 0.18, d + 0.18),
                (rail + 0.04, rail + 0.24),
                0.02,
            );
        }
        // Los faldones a cada lado, colgando, y un estribo de hierro.
        for side in [-1.0, 1.0] {
            let dd = d + side * 0.26;
            wall.solid(
                shape,
                leather,
                (s - 0.22, s + 0.22),
                (dd - 0.012, dd + 0.012),
                (rail - 0.38, rail + 0.06),
            );
            let ds = d + side * 0.3;
            wall.beam(
                shape,
                Surface::Barnwood,
                (s, ds, rail),
                (s, ds, rail - 0.55),
                0.02,
            );
            wall.solid(
                shape,
                Surface::Steel,
                (s - 0.07, s + 0.07),
                (ds - 0.03, ds + 0.03),
                (rail - 0.63, rail - 0.55),
            );
        }
    }
    // Los lazos, en la pared encima del caballete.
    let turn = wall.turn() * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for k in 0..saddles {
        let s = 0.1 + gap * (k as f32 + 0.5);
        let peg = wall.at(s, 0.0, 1.75);
        wall.beam(shape, WOOD, (s, 0.0, 1.75), (s, 0.12, 1.77), 0.03);
        shape.lathe(
            Surface::Barnwood,
            Transform::from_translation(peg).with_rotation(turn),
            &[
                (0.2, 0.02),
                (0.22, 0.05),
                (0.2, 0.08),
                (0.17, 0.05),
                (0.2, 0.02),
            ],
            14,
        );
    }
}

/// El tablero de las herramientas contra su pared, de `top` de alto: las
/// tablas con un listón de clavijas y, colgadas, palas, azadas, una horquilla,
/// un hacha y un serrucho, mientras quepan.
pub(super) fn tools(shape: &mut Shape, wall: Against, top: f32) {
    let len = wall.len();
    let height = top - ROOM_FLOOR;
    wall.solid(
        shape,
        Surface::Planks,
        (0.0, len),
        (0.0, 0.025),
        (0.35, height),
    );
    let rail = height - 0.25;
    wall.solid(
        shape,
        WOOD,
        (0.0, len),
        (0.025, 0.06),
        (rail - 0.04, rail + 0.04),
    );
    let handle = |shape: &mut Shape, s: f32, low: f32| {
        wall.beam(shape, WOOD, (s, 0.09, rail + 0.06), (s, 0.09, low), 0.035);
    };
    let mut s = 0.22;
    let mut k = 0;
    while s + 0.2 < len {
        match k % 5 {
            // Una pala de punta.
            0 | 3 => {
                handle(shape, s, 0.75);
                wall.chamfered(
                    shape,
                    Surface::Steel,
                    (s - 0.12, s + 0.12),
                    (0.07, 0.1),
                    (0.42, 0.78),
                    0.03,
                );
            }
            // Una azada, con la hoja hacia la pared.
            1 => {
                handle(shape, s, 0.55);
                wall.solid(
                    shape,
                    Surface::Steel,
                    (s - 0.11, s + 0.11),
                    (0.04, 0.2),
                    (0.5, 0.58),
                );
            }
            // Una horquilla de tres dientes.
            2 => {
                handle(shape, s, 0.8);
                wall.solid(
                    shape,
                    Surface::Steel,
                    (s - 0.1, s + 0.1),
                    (0.07, 0.11),
                    (0.78, 0.82),
                );
                for t in [-0.09, 0.0, 0.09] {
                    wall.beam(
                        shape,
                        Surface::Steel,
                        (s + t, 0.09, 0.8),
                        (s + t, 0.09, 0.45),
                        0.012,
                    );
                }
            }
            // Un hacha, y un serrucho al lado.
            _ => {
                wall.beam(
                    shape,
                    WOOD,
                    (s, 0.09, rail + 0.06),
                    (s, 0.09, rail - 0.75),
                    0.035,
                );
                wall.chamfered(
                    shape,
                    Surface::Steel,
                    (s - 0.03, s + 0.13),
                    (0.06, 0.12),
                    (rail - 0.85, rail - 0.7),
                    0.01,
                );
                let saw = s + 0.24;
                if saw + 0.1 < len {
                    wall.solid(
                        shape,
                        Surface::Steel,
                        (saw - 0.08, saw + 0.08),
                        (0.07, 0.075),
                        (rail - 0.7, rail - 0.06),
                    );
                    wall.solid(
                        shape,
                        WOOD,
                        (saw - 0.06, saw + 0.06),
                        (0.06, 0.09),
                        (rail - 0.06, rail + 0.06),
                    );
                    s += 0.24;
                }
            }
        }
        s += 0.38;
        k += 1;
    }
}
