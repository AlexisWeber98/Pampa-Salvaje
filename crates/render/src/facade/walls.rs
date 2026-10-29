//! Paredes con sus aberturas, y los techos que las cubren: los bloques con los
//! que se hace cada casa de asentamiento y cada edificio de pueblo.
//!
//! Un [`Walls`] son las cuatro paredes de una caja en el espacio propio de la
//! estructura (el frente en `z1`); cada [`Wall`] se construye a sí misma
//! dejando abiertas las puertas y ventanas. Los techos: a dos aguas
//! ([`gable_roof`]), a un agua cayendo hacia el frente ([`lean_to`]), y una
//! azotea plana detrás de un parapeto ([`parapet_roof`]). El galpón ([`barn`])
//! es un edificio completo.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;

/// Una puerta o una ventana en una pared.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Opening {
    /// Su centro, en metros a lo largo de la pared desde el comienzo de la
    /// pared.
    pub at: f32,
    pub width: f32,
    pub bottom: f32,
    pub top: f32,
    pub door: bool,
}

impl Opening {
    /// Sus dos jambas, en metros a lo largo de la pared.
    pub fn span(&self) -> (f32, f32) {
        (self.at - self.width / 2.0, self.at + self.width / 2.0)
    }
}

/// Una pared recta, de `thickness` de grosor hacia adentro desde su cara
/// exterior.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wall {
    /// Dónde empieza su cara exterior, sobre el suelo.
    pub start: Vec3,
    /// Dirección unitaria a lo largo de ella (`±X` o `±Z` locales).
    pub along: Vec3,
    /// Dirección unitaria hacia afuera de su cara exterior.
    pub out: Vec3,
    pub length: f32,
    pub thickness: f32,
    pub height: f32,
    pub drop: f32,
}

impl Wall {
    /// El punto a `a` metros a lo largo, `inward` metros hacia adentro desde la
    /// cara exterior, a la altura `y`.
    pub fn at(&self, a: f32, inward: f32, y: f32) -> Vec3 {
        self.start + self.along * a - self.out * inward + Vec3::Y * y
    }

    /// Una caja alineada con los ejes entre dos puntos dados en términos de
    /// pared.
    pub fn piece(
        &self,
        shape: &mut Shape,
        surface: Surface,
        lo: (f32, f32, f32),
        hi: (f32, f32, f32),
    ) {
        let (p, q) = (self.at(lo.0, lo.1, lo.2), self.at(hi.0, hi.1, hi.2));
        // Las caras inferiores solo se ven en los dinteles, que miran hacia
        // abajo dentro de una abertura.
        let min = p.min(q);
        let max = p.max(q);
        if lo.2 > 0.0 {
            shape.cuboid(
                surface,
                Transform::from_translation((min + max) / 2.0).with_scale(max - min),
                true,
            );
        } else {
            shape.block(surface, min, max);
        }
    }

    /// La pared en `surface` con `openings` dejadas en ella: pilares entre
    /// ellas, un dintel encima y un alféizar debajo de cada una, un hueco
    /// oscuro adentro, una puerta de tablones (cerrada salvo que `open`) o una
    /// ventana con sus dos postigos abiertos contra la pared.
    pub fn build(&self, shape: &mut Shape, surface: Surface, openings: &[Opening], open: u32) {
        let (t, h, drop) = (self.thickness, self.height, self.drop);
        let mut from = 0.0;
        let mut sorted = openings.to_vec();
        sorted.sort_by(|a, b| a.at.total_cmp(&b.at));
        for (k, o) in sorted.iter().enumerate() {
            let (a, b) = o.span();
            if a > from {
                self.piece(shape, surface, (from, 0.0, -drop), (a, t, h));
            }
            if o.top < h {
                self.piece(shape, surface, (a, 0.0, o.top), (b, t, h));
            }
            if o.bottom > -drop + 0.01 {
                self.piece(shape, surface, (a, 0.0, -drop), (b, t, o.bottom));
            }
            // El interior oscuro, visto a través de la abertura.
            self.piece(
                shape,
                Surface::Recess,
                (a, t * 0.7, o.bottom),
                (b, t * 0.75, o.top),
            );
            if o.door {
                // Tablones sobre un marco; unas pocas puertas, según la
                // semilla, quedan abiertas.
                if (open >> (k % 16)) & 1 == 0 {
                    self.piece(
                        shape,
                        Surface::Planks,
                        (a, t * 0.45, o.bottom),
                        (b, t * 0.5, o.top),
                    );
                }
            } else {
                // El alféizar sobresale, y los postigos quedan abiertos hacia
                // atrás.
                self.piece(
                    shape,
                    surface,
                    (a - 0.05, -0.06, o.bottom - 0.08),
                    (b + 0.05, 0.0, o.bottom),
                );
                let leaf = o.width / 2.0;
                for (l0, l1) in [(a - leaf - 0.02, a - 0.02), (b + 0.02, b + leaf + 0.02)] {
                    self.piece(
                        shape,
                        Surface::Planks,
                        (l0, -0.05, o.bottom + 0.02),
                        (l1, -0.01, o.top - 0.02),
                    );
                }
                // Una hoja vidriada en el vano, oscura.
                self.piece(
                    shape,
                    Surface::Recess,
                    (a, t * 0.3, o.bottom),
                    (b, t * 0.32, o.top),
                );
            }
            from = b;
        }
        if from < self.length {
            self.piece(shape, surface, (from, 0.0, -drop), (self.length, t, h));
        }
    }
}

/// Las cuatro paredes de una casa alrededor de `x0..x1`, `z0..z1` (caras
/// exteriores): el frente en `z1` y el fondo en `z0`, ambos a todo el ancho;
/// los laterales entre ellos.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Walls {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    pub thickness: f32,
    pub height: f32,
    pub drop: f32,
}

impl Walls {
    pub fn front(&self) -> Wall {
        Wall {
            start: Vec3::new(self.x0, 0.0, self.z1),
            along: Vec3::X,
            out: Vec3::Z,
            length: self.x1 - self.x0,
            thickness: self.thickness,
            height: self.height,
            drop: self.drop,
        }
    }

    pub fn back(&self) -> Wall {
        Wall {
            start: Vec3::new(self.x1, 0.0, self.z0),
            along: Vec3::NEG_X,
            out: Vec3::NEG_Z,
            ..self.front()
        }
    }

    /// El lateral en `x1` (`east = true`) o en `x0`, entre las paredes del
    /// frente y del fondo.
    pub fn side(&self, east: bool) -> Wall {
        let t = self.thickness;
        let (x, z, along, out) = if east {
            (self.x1, self.z1 - t, Vec3::NEG_Z, Vec3::X)
        } else {
            (self.x0, self.z0 + t, Vec3::Z, Vec3::NEG_X)
        };
        Wall {
            start: Vec3::new(x, 0.0, z),
            along,
            out,
            length: self.z1 - self.z0 - 2.0 * t,
            ..self.front()
        }
    }

    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn depth(&self) -> f32 {
        self.z1 - self.z0
    }
}

/// `count` aberturas repartidas de forma pareja a lo largo de una pared de
/// `length` de largo: puertas en las posiciones `doors` y ventanas en el resto.
pub(crate) fn spread(
    length: f32,
    count: usize,
    doors: &[usize],
    door: (f32, f32),
    window: (f32, f32, f32),
) -> Vec<Opening> {
    (0..count)
        .map(|k| {
            let at = length * (k as f32 + 0.5) / count as f32;
            if doors.contains(&k) {
                Opening {
                    at,
                    width: door.0,
                    bottom: 0.0,
                    top: door.1,
                    door: true,
                }
            } else {
                Opening {
                    at,
                    width: window.0,
                    bottom: window.1,
                    top: window.2,
                    door: false,
                }
            }
        })
        .collect()
}

/// Una losa de techo gruesa o fina sobre una pendiente desde `low` (el borde
/// del alero) hasta `high` (la cumbrera o el borde alto), de `width` metros a
/// lo largo de `X`, con su cara inferior sobre la línea.
pub(crate) fn slab(
    shape: &mut Shape,
    surface: Surface,
    low: Vec3,
    high: Vec3,
    width: f32,
    thickness: f32,
) {
    let d = high - low;
    let len = d.length();
    // La dirección de la pendiente en el plano Y–Z, y su normal hacia afuera.
    let dir = d / len;
    let normal = Vec3::new(0.0, dir.z.abs(), if dir.z < 0.0 { dir.y } else { -dir.y });
    let normal = normal.normalize_or(Vec3::Y);
    let center = (low + high) / 2.0 + normal * (thickness / 2.0);
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir.with_x(0.0).normalize_or(Vec3::Z));
    shape.cuboid(
        surface,
        Transform::from_translation(center)
            .with_rotation(rotation)
            .with_scale(Vec3::new(width, thickness, len)),
        true,
    );
}

/// Un techo a dos aguas sobre `walls`, con la cumbrera a lo largo de `X`,
/// subiendo `rise` por encima de los aleros y sobresaliendo `overhang` todo
/// alrededor; los hastiales se rellenan en `gable`.
pub(crate) fn gable_roof(
    shape: &mut Shape,
    (roof, gable): (Surface, Surface),
    walls: &Walls,
    rise: f32,
    overhang: f32,
    thickness: f32,
) {
    let (h, zc) = (walls.height, (walls.z0 + walls.z1) / 2.0);
    let half = walls.depth() / 2.0;
    let slope = rise / half;
    let ridge = Vec3::new(0.0, h + rise, zc);
    let xc = (walls.x0 + walls.x1) / 2.0;
    let width = walls.width() + 2.0 * overhang;
    for s in [1.0, -1.0] {
        let eave = Vec3::new(xc, h - overhang * slope, zc + s * (half + overhang));
        slab(shape, roof, eave, ridge.with_x(xc), width, thickness);
    }
    // Los hastiales, al ras de las caras exteriores de las paredes laterales.
    for (x, e) in [(walls.x1, 1.0), (walls.x0, -1.0)] {
        let (f, b, top) = (
            Vec3::new(x, h, walls.z1),
            Vec3::new(x, h, walls.z0),
            Vec3::new(x, h + rise, zc),
        );
        if e > 0.0 {
            shape.face(gable, &[f, b, top]);
        } else {
            shape.face(gable, &[b, f, top]);
        }
    }
}

/// Un remate de pared macizo sobre `xa..xb`, `za..zb`, desde `base` hasta una
/// parte superior inclinada de altura `ya` en `za` y `yb` en `zb`, con la base
/// abierta sobre la pared que corona.
pub(crate) fn wedge(
    shape: &mut Shape,
    surface: Surface,
    (xa, xb): (f32, f32),
    (za, zb): (f32, f32),
    base: f32,
    (ya, yb): (f32, f32),
) {
    const EPS: f32 = 1e-3;
    // El perfil del extremo, en (z, y), sin sus esquinas de altura cero.
    let mut profile = vec![(zb, base), (za, base)];
    if ya - base > EPS {
        profile.push((za, ya));
    }
    if yb - base > EPS {
        profile.push((zb, yb));
    }
    if profile.len() < 3 {
        return;
    }
    let east: Vec<Vec3> = profile.iter().map(|&(z, y)| Vec3::new(xb, y, z)).collect();
    let west: Vec<Vec3> = profile
        .iter()
        .rev()
        .map(|&(z, y)| Vec3::new(xa, y, z))
        .collect();
    shape.face(surface, &east);
    shape.face(surface, &west);
    if yb - base > EPS {
        shape.face(
            surface,
            &[
                Vec3::new(xa, base, zb),
                Vec3::new(xb, base, zb),
                Vec3::new(xb, yb, zb),
                Vec3::new(xa, yb, zb),
            ],
        );
    }
    if ya - base > EPS {
        shape.face(
            surface,
            &[
                Vec3::new(xb, base, za),
                Vec3::new(xa, base, za),
                Vec3::new(xa, ya, za),
                Vec3::new(xb, ya, za),
            ],
        );
    }
    shape.face(
        surface,
        &[
            Vec3::new(xa, ya, za),
            Vec3::new(xa, yb, zb),
            Vec3::new(xb, yb, zb),
            Vec3::new(xb, ya, za),
        ],
    );
}

/// Un techo de una sola caída (a un agua) de chapa acanalada, alto sobre la
/// pared del fondo y bajo sobre el frente para que sus canaletas se vean desde
/// el patio, sobresaliendo `overhang` en los aleros y en los bordes laterales.
/// Cada pared se levanta en `gable` hasta encontrarse con la chapa: los
/// laterales como hastiales macizos, el fondo y el frente como franjas
/// inclinadas.
pub(crate) fn lean_to(
    shape: &mut Shape,
    (roof, gable): (Surface, Surface),
    walls: &Walls,
    rise: f32,
    overhang: f32,
) {
    let (h, t) = (walls.height, walls.thickness);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    let slope = rise / walls.depth();
    // La cara inferior de la chapa, sobre las caras exteriores.
    let under = |z: f32| h + (z1 - z) * slope;
    let xc = (x0 + x1) / 2.0;
    let low = Vec3::new(xc, under(z1 + overhang), z1 + overhang);
    let high = Vec3::new(xc, under(z0 - overhang), z0 - overhang);
    slab(shape, roof, low, high, walls.width() + 2.0 * overhang, 0.06);
    // Los hastiales laterales abarcan toda la profundidad, sobre las esquinas.
    for xs in [(x1 - t, x1), (x0, x0 + t)] {
        wedge(shape, gable, xs, (z0, z1), h, (under(z0), under(z1)));
    }
    for zs in [(z0, z0 + t), (z1 - t, z1)] {
        wedge(
            shape,
            gable,
            (x0 + t, x1 - t),
            zs,
            h,
            (under(zs.0), under(zs.1)),
        );
    }
}

/// Una franja alrededor de `walls` desde `y0` hasta `y1`, que sobresale `out`
/// de sus caras exteriores y se mete `inner` hacia adentro desde ellas.
pub(crate) fn ring(
    shape: &mut Shape,
    surface: Surface,
    walls: &Walls,
    out: f32,
    (y0, y1): (f32, f32),
    inner: f32,
) {
    let (a0, a1, b0, b1) = (
        walls.x0 - out,
        walls.x1 + out,
        walls.z0 - out,
        walls.z1 + out,
    );
    let w = inner + out;
    shape.block(surface, Vec3::new(a0, y0, b1 - w), Vec3::new(a1, y1, b1));
    shape.block(surface, Vec3::new(a0, y0, b0), Vec3::new(a1, y1, b0 + w));
    shape.block(
        surface,
        Vec3::new(a0, y0, b0 + w),
        Vec3::new(a0 + w, y1, b1 - w),
    );
    shape.block(
        surface,
        Vec3::new(a1 - w, y0, b0 + w),
        Vec3::new(a1, y1, b1 - w),
    );
}

/// Una azotea plana detrás de un parapeto de `parapet` de alto, con una franja
/// de cornisa debajo y una albardilla encima.
pub(crate) fn parapet_roof(shape: &mut Shape, surface: Surface, walls: &Walls, parapet: f32) {
    let (h, t) = (walls.height, walls.thickness);
    flat_roof(shape, walls);
    ring(shape, surface, walls, 0.0, (h, h + parapet), t * 0.6);
    // La cornisa bajo el parapeto y la albardilla encima sobresalen.
    ring(shape, surface, walls, 0.1, (h - 0.22, h + 0.06), 0.12);
    ring(
        shape,
        surface,
        walls,
        0.06,
        (h + parapet, h + parapet + 0.08),
        t * 0.6,
    );
}

/// El techo plano de baldosas de ladrillo dentro de `walls`, justo por debajo
/// del pie de un parapeto.
pub(crate) fn flat_roof(shape: &mut Shape, walls: &Walls) {
    let (h, t) = (walls.height, walls.thickness);
    shape.block(
        Surface::Brick,
        Vec3::new(walls.x0 + t, h - 0.25, walls.z0 + t),
        Vec3::new(walls.x1 - t, h - 0.1, walls.z1 - t),
    );
}

/// Una chimenea de ladrillo que atraviesa un techo en `at`, de `top` metros de
/// alto.
pub(crate) fn chimney(shape: &mut Shape, at: Vec3, top: f32) {
    shape.block(
        Surface::Brick,
        at + Vec3::new(-0.3, 0.0, -0.25),
        Vec3::new(at.x + 0.3, top, at.z + 0.25),
    );
    shape.block(
        Surface::Brick,
        Vec3::new(at.x - 0.36, top, at.z - 0.31),
        Vec3::new(at.x + 0.36, top + 0.12, at.z + 0.31),
    );
}

/// Los aleros y los bordes laterales de un galpón: cuánto sobresale su techo de
/// sus paredes.
const BARN_OVERHANG: f32 = 0.4;
/// El grosor de la chapa del techo de un galpón.
const BARN_SHEET: f32 = 0.05;

/// Cuánto sobresale el techo de un galpón más allá de sus paredes ([`barn`]):
/// sus aleros y bordes laterales, más como mucho el grosor de su chapa
/// inclinada hacia afuera. Un galpón que debe quedar dentro de una huella de
/// edificio se retira esa distancia de ella: el
/// [`pampa_core::Building::BARN_EAVES`] de core, donde está su collider.
pub(crate) const BARN_REACH: f32 = BARN_OVERHANG + BARN_SHEET;

/// El galpón: `walls` de chapa acanalada sobre madera, un techo a dos aguas a
/// lo largo de su ancho (`X`) en `roof`, y `doors` pares de grandes portones de
/// tablones repartidos a lo largo de su frente; `w`, `d`, `h` son su ancho, su
/// profundidad y la altura de sus paredes.
pub(crate) fn barn(
    shape: &mut Shape,
    (walls, roof): (Surface, Surface),
    (w, d, h): (f32, f32, f32),
    drop: f32,
    v: u32,
    doors: usize,
) {
    let box_ = Walls {
        x0: -w / 2.0,
        x1: w / 2.0,
        z0: -d / 2.0,
        z1: d / 2.0,
        thickness: 0.08,
        height: h,
        drop,
    };
    let doors = doors.max(1);
    let bay = w / doors as f32;
    let mut width = (w * 0.3).clamp(4.0, 6.0);
    if doors > 1 {
        width = width.min(bay - 1.5).max(2.0);
    }
    let openings: Vec<Opening> = (0..doors)
        .map(|k| Opening {
            at: bay * (k as f32 + 0.5),
            width,
            bottom: 0.0,
            top: (h - 0.8).min(4.0),
            door: true,
        })
        .collect();
    // Un portón grande está casi siempre cerrado; uno de cada tres queda
    // abierto hacia la oscuridad.
    let open = u32::from(v.is_multiple_of(3));
    box_.front().build(shape, walls, &openings, open);
    box_.back().build(shape, walls, &[], 0);
    for east in [true, false] {
        box_.side(east).build(shape, walls, &[], 0);
    }
    // Postes esquineros de madera y un zócalo de tablones, el marco de los
    // portones.
    for (x, z) in [
        (box_.x0, box_.z0),
        (box_.x1, box_.z0),
        (box_.x0, box_.z1),
        (box_.x1, box_.z1),
    ] {
        shape.beam(
            Surface::Planks,
            Vec3::new(x, -drop, z),
            Vec3::new(x, h, z),
            0.2,
        );
    }
    for door in &openings {
        let (a, b) = door.span();
        for x in [box_.x0 + a, box_.x0 + b] {
            shape.beam(
                Surface::Planks,
                Vec3::new(x, -drop, box_.z1 + 0.05),
                Vec3::new(x, door.top + 0.15, box_.z1 + 0.05),
                0.18,
            );
        }
        shape.beam(
            Surface::Planks,
            Vec3::new(box_.x0 + a - 0.1, door.top + 0.1, box_.z1 + 0.05),
            Vec3::new(box_.x0 + b + 0.1, door.top + 0.1, box_.z1 + 0.05),
            0.18,
        );
    }
    let rise = d / 2.0 * 0.35;
    gable_roof(shape, (roof, roof), &box_, rise, BARN_OVERHANG, BARN_SHEET);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wall_leaves_its_openings_open() {
        let wall = Walls {
            x0: -5.0,
            x1: 5.0,
            z0: -3.0,
            z1: 3.0,
            thickness: 0.4,
            height: 3.0,
            drop: 0.3,
        }
        .front();
        let mut shape = Shape::default();
        let door = Opening {
            at: 5.0,
            width: 1.0,
            bottom: 0.0,
            top: 2.2,
            door: true,
        };
        // Portón abierto (bit 0 activado): nada de la pared rellena el vano.
        wall.build(&mut shape, Surface::Whitewash, &[door], 1);
        let blocked = shape.points(Surface::Whitewash).chunks(4).any(|q| {
            let c = q.iter().fold(Vec3::ZERO, |a, p| a + *p) / 4.0;
            c.x.abs() < 0.4 && c.y > 0.2 && c.y < 2.0 && c.z > 2.6
        });
        assert!(!blocked);
        assert!(shape.surfaces().any(|s| s == Surface::Recess));
        assert!(!shape.surfaces().any(|s| s == Surface::Planks));
    }

    /// Las paredes de la casa de adobe de una chacra bajo un techo a un agua.
    fn cottage() -> Walls {
        Walls {
            x0: -7.0,
            x1: 7.0,
            z0: -3.8,
            z1: 3.8,
            thickness: 0.5,
            height: 3.4,
            drop: 0.3,
        }
    }

    /// El punto más alto de `surface` justo encima de `(x, z)`, si alguno de
    /// sus triángulos lo cubre.
    pub(crate) fn top_at(shape: &Shape, surface: Surface, x: f32, z: f32) -> Option<f32> {
        shape
            .triangles_of(surface)
            .into_iter()
            .filter_map(|[a, b, c]| {
                let (e1, e2, p) = (b - a, c - a, Vec3::new(x, 0.0, z) - a);
                let det = e1.x * e2.z - e2.x * e1.z;
                if det.abs() < 1e-6 {
                    return None;
                }
                let u = (p.x * e2.z - e2.x * p.z) / det;
                let v = (e1.x * p.z - p.x * e1.z) / det;
                (u >= -1e-4 && v >= -1e-4 && u + v <= 1.0 + 1e-4)
                    .then_some(a.y + u * e1.y + v * e2.y)
            })
            .reduce(f32::max)
    }

    #[test]
    fn a_lean_to_rests_on_all_four_walls() {
        let walls = cottage();
        let mut shape = Shape::default();
        lean_to(
            &mut shape,
            (Surface::OldIron, Surface::Adobe),
            &walls,
            0.7,
            0.35,
        );
        for east in [true, false] {
            walls.side(east).build(&mut shape, Surface::Adobe, &[], 0);
        }
        walls.front().build(&mut shape, Surface::Adobe, &[], 0);
        walls.back().build(&mut shape, Surface::Adobe, &[], 0);
        let t = walls.thickness;
        // A lo largo del centro de la parte superior de cada pared, la pared se
        // encuentra con la cara inferior del techo: sin hueco y sin atravesar
        // la chapa.
        let mut columns = Vec::new();
        for k in 0..=20 {
            let z = walls.z0 + 0.05 + (walls.depth() - 0.1) * k as f32 / 20.0;
            columns.push((walls.x1 - t / 2.0, z));
            columns.push((walls.x0 + t / 2.0, z));
        }
        for k in 0..=20 {
            let x = walls.x0 + 0.05 + (walls.width() - 0.1) * k as f32 / 20.0;
            columns.push((x, walls.z1 - t / 2.0));
            columns.push((x, walls.z0 + t / 2.0));
        }
        for (x, z) in columns {
            let wall = top_at(&shape, Surface::Adobe, x, z).expect("a wall here");
            let under = top_at(&shape, Surface::OldIron, x, z).expect("a roof here") - 0.06;
            assert!(
                wall > under - 0.03 && wall < under + 0.01,
                "at ({x}, {z}) the wall tops out at {wall}, the roof's underside is at {under}"
            );
        }
    }

    #[test]
    fn a_lean_to_overhangs_its_walls_a_little() {
        let walls = cottage();
        let mut shape = Shape::default();
        lean_to(
            &mut shape,
            (Surface::OldIron, Surface::Adobe),
            &walls,
            0.7,
            0.35,
        );
        let roof = shape.points(Surface::OldIron);
        let min = roof.iter().fold(Vec3::MAX, |a, p| a.min(*p));
        let max = roof.iter().fold(Vec3::MIN, |a, p| a.max(*p));
        for over in [
            walls.x0 - min.x,
            max.x - walls.x1,
            walls.z0 - min.z,
            max.z - walls.z1,
        ] {
            assert!((0.3..=0.6).contains(&over), "overhang {over}");
        }
    }

    #[test]
    fn the_lean_to_corrugation_runs_down_its_slope() {
        let walls = cottage();
        let mut shape = Shape::default();
        lean_to(
            &mut shape,
            (Surface::OldIron, Surface::Adobe),
            &walls,
            0.7,
            0.35,
        );
        // La cara superior de la chapa: sus cuatro esquinas.
        let top: Vec<_> = shape
            .vertices(Surface::OldIron)
            .into_iter()
            .filter(|(_, n, _)| n.y > 0.5)
            .collect();
        assert_eq!(top.len(), 4);
        let (high, low) = (
            top.iter().max_by(|a, b| a.0.y.total_cmp(&b.0.y)).unwrap(),
            top.iter().min_by(|a, b| a.0.y.total_cmp(&b.0.y)).unwrap(),
        );
        // Las canaletas de la textura corren a lo largo de `v`: pendiente abajo
        // cambia `v` y no `u`, y la chapa se repite varias veces.
        let down = low.2 - high.2;
        assert!(down.y.abs() > 2.0 && down.x.abs() < 0.2, "{down:?}");
        let along = top.iter().map(|(_, _, uv)| uv.x).fold(f32::MIN, f32::max)
            - top.iter().map(|(_, _, uv)| uv.x).fold(f32::MAX, f32::min);
        assert!(along > 2.0, "{along}");
        // La parte superior mira hacia arriba y hacia afuera sobre el frente,
        // desde donde se la ve.
        assert!(top.iter().all(|(_, n, _)| n.z > 0.0), "{top:?}");
    }
}
