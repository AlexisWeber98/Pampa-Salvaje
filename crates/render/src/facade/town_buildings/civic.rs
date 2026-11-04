//! Los edificios institucionales de un pueblo grande, con el kit de fachadas:
//! la sucursal de banco y la intendencia municipal, en el estilo académico
//! italianizante con el que se construían hacia 1919 (frentes de revoque símil
//! piedra, almohadillado en la planta baja, órdenes de pilastras gigantes,
//! aberturas de medio punto con clave, balaustrada y frontón).

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::super::italianate::{
    Crown, PROUD, arch, baluster, corner_piers, cornice, door_leaves, grille, parapet, pediment,
    pilaster, plinth, surround, urn,
};
use super::super::materials::{Paint, Surface};
use super::super::shape::Shape;
use super::super::walls::{Opening, Wall, Walls, flat_roof, ring};
use super::{DROP, Front, Roof, back_and_sides, foot, masonry};

/// Altura de la franja lisa del friso de un banco, bajo su cornisa, en la que
/// `signs` pinta su nombre: de `h - BANK_FRIEZE.0` a `h - BANK_FRIEZE.1`.
pub const BANK_FRIEZE: (f32, f32) = (2.0, 0.95);

/// Altura del ático macizo del pabellón central de la intendencia, sobre su
/// cornisa, en el que `signs` pinta su nombre.
pub const TOWN_HALL_ATTIC: f32 = 1.15;

/// Una franja de sillares almohadillados sobre `wall`, de `a` a `b` a lo largo
/// de ella y de `y0` a `y1`: hiladas de `course` de alto separadas por una
/// junta rehundida, como el revoque símil piedra de las plantas bajas
/// académicas.
fn rustication(
    shape: &mut Shape,
    surface: Surface,
    wall: &Wall,
    (a, b): (f32, f32),
    (y0, y1): (f32, f32),
) {
    if b - a < 0.05 {
        return;
    }
    let course = 0.42;
    let joint = 0.05;
    let mut y = y0;
    while y + joint < y1 {
        let top = (y + course).min(y1);
        wall.piece(shape, surface, (a, -0.035, y + joint), (b, 0.0, top));
        y += course;
    }
}

/// Las jambas entre las aberturas de `openings` en `wall`, de `y0` a `y1`, con
/// almohadillado (y los extremos, hasta `margin` de cada punta).
fn rusticate_between(
    shape: &mut Shape,
    surface: Surface,
    wall: &Wall,
    openings: &[Opening],
    (y0, y1): (f32, f32),
    margin: f32,
) {
    let mut from = margin;
    for o in openings {
        let (a, b) = o.span();
        rustication(shape, surface, wall, (from, a - 0.12), (y0, y1));
        from = b + 0.12;
    }
    rustication(shape, surface, wall, (from, wall.length - margin), (y0, y1));
}

/// La clave saliente en lo alto del arco de la abertura `o` de `wall`.
fn keystone(shape: &mut Shape, surface: Surface, wall: &Wall, o: &Opening) {
    wall.piece(
        shape,
        surface,
        (o.at - 0.13, -0.09, o.top - 0.12),
        (o.at + 0.13, 0.02, o.top + 0.22),
    );
}

/// Una fila de dentículos bajo la cornisa de `wall`, a la altura `y`.
fn dentils(shape: &mut Shape, surface: Surface, wall: &Wall, (a, b): (f32, f32), y: f32) {
    let pitch = 0.26;
    let n = ((b - a) / pitch) as usize;
    for k in 0..n {
        let x = a + pitch * (k as f32 + 0.3);
        wall.piece(shape, surface, (x, -0.07, y - 0.12), (x + 0.12, 0.02, y));
    }
}

/// La sucursal de banco: un frente académico de revoque gris símil piedra, de
/// una planta alta, con la planta almohadillada entre aberturas de medio punto
/// (la puerta en el medio, ventanas vidriadas con rejas a los lados), pilastras
/// gigantes, un friso liso para su nombre, dentículos bajo la cornisa y una
/// balaustrada con macetones y un frontón sobre la puerta, ocultando una
/// azotea plana.
pub(super) fn bank(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let walls = masonry((w, d, h), 0.55);
    let (wall, trim) = (Surface::Ashlar, Surface::Whitewash);
    let front = walls.front();
    let len = front.length;
    let bays = ((len / 3.4) as usize).clamp(3, 5) | 1;
    // Las aberturas terminan bajo el friso del nombre.
    let top = (h - BANK_FRIEZE.0 - 0.3).min(4.0);
    let openings: Vec<Opening> = (0..bays)
        .map(|k| {
            let door = k == bays / 2;
            Opening {
                at: len * (k as f32 + 0.5) / bays as f32,
                width: if door { 1.9 } else { 1.35 },
                bottom: if door { 0.0 } else { 0.95 },
                top,
                // Vidriadas, sin postigos: construidas como puertas abiertas
                // (las hojas de la puerta van aparte).
                door: true,
            }
        })
        .collect();
    front.build(shape, wall, &openings, u32::MAX);
    for o in &openings {
        arch(shape, (wall, trim), &front, o);
        keystone(shape, trim, &front, o);
        if o.bottom > 0.0 {
            grille(shape, &front, o);
            // El alféizar moldurado.
            let (a, b) = o.span();
            front.piece(
                shape,
                trim,
                (a - 0.12, -0.1, o.bottom - 0.12),
                (b + 0.12, 0.02, o.bottom),
            );
        } else {
            door_leaves(shape, &front, o, true);
        }
    }
    rusticate_between(shape, wall, &front, &openings, (0.95, top), 0.55);
    let f = Front {
        wall,
        trim,
        plinth: Surface::Plinth,
        bays,
        doors: vec![bays / 2],
        door: (1.9, top),
        window: (1.2, 1.0, top - 0.6),
        parapet: 1.0,
        crown: Crown::Balustrade,
        crest: true,
        urns: true,
        roof: Roof::Flat,
        grilles: true,
        hoods: false,
        transom: true,
    };
    back_and_sides(shape, &walls, &f, v);
    plinth(shape, Surface::Plinth, &front, &openings, 0.95, 0.09);
    // El orden gigante: pilastras de la base a la cornisa entre los vanos.
    let capital = h - 0.55;
    let pillars: Vec<f32> = (1..bays).map(|k| len * k as f32 / bays as f32).collect();
    for &at in &pillars {
        pilaster(shape, trim, &front, at, 0.6, 0.09, (-DROP, 1.1, capital));
    }
    corner_piers(shape, trim, &walls, 0.7, PROUD, (-DROP, capital));
    // El friso: una moldura bajo la franja del nombre.
    front.piece(
        shape,
        trim,
        (0.6, -0.06, h - BANK_FRIEZE.0 - 0.14),
        (len - 0.6, 0.02, h - BANK_FRIEZE.0),
    );
    dentils(shape, trim, &front, (0.7, len - 0.7), capital);
    cornice(shape, trim, &walls);
    let mut posts = pillars.clone();
    let end = 0.25;
    posts.extend([end, len - end]);
    parapet(
        shape,
        trim,
        &walls,
        f.parapet,
        (Crown::Balustrade, true),
        &posts,
    );
    let crown = h + f.parapet + 0.18;
    for at in [end, len - end] {
        urn(shape, trim, front.at(at, walls.thickness * 0.3, crown));
    }
    let mid = (walls.x0 + walls.x1) / 2.0;
    let half = len / bays as f32 / 2.0 + 0.35;
    pediment(
        shape,
        trim,
        (mid - half, mid + half),
        walls.z1,
        h + f.parapet + 0.08,
        0.8,
        walls.thickness * 0.6,
    );
    flat_roof(shape, &walls);
    foot(shape, &walls);
}

/// La intendencia municipal: el frente más ancho y alto del pueblo, de dos
/// plantas, pintado de ocre con molduras blancas. La planta baja está
/// almohadillada, con ventanas de medio punto enrejadas; la alta tiene ventanas
/// con marcos y frontoncitos. En el medio sobresale un pabellón con la puerta
/// de arco, un balcón enrasado con balaustres en la planta alta, pilastras en
/// ambas plantas, un ático macizo para su nombre y un frontón con el reloj
/// municipal; detrás, el mástil de la bandera.
pub(super) fn town_hall(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    // El pabellón llega hasta la huella; el resto del frente se retira detrás
    // de él.
    let out = 0.5;
    let mut walls = masonry((w, d, h), 0.55);
    walls.z1 -= out;
    let (wall, trim, base) = (
        Surface::Render(Paint::Ochre),
        Surface::Whitewash,
        Surface::Plinth,
    );
    let floor = (h * 0.5).max(3.8);
    let front = walls.front();
    let len = front.length;
    let pav = (len * 0.4).clamp(6.0, 9.0);
    let (p0, p1) = (len / 2.0 - pav / 2.0, len / 2.0 + pav / 2.0);
    let lower = Wall {
        height: floor,
        ..front
    };
    let upper = Wall {
        start: front.start + Vec3::Y * floor,
        height: h - floor,
        drop: 0.0,
        ..front
    };
    // Las alas a cada lado del pabellón: ventanas parejas en ambas plantas.
    let per_wing = ((p0 - 0.8) / 2.9).floor().max(1.0) as usize;
    let wing_at = |k: usize| {
        let step = (p0 - 0.6) / per_wing as f32;
        let a = 0.6 + step * (k as f32 + 0.5);
        [a, len - a]
    };
    let ground_top = floor - 0.55;
    let mut ground = Vec::new();
    let mut high = Vec::new();
    for k in 0..per_wing {
        for at in wing_at(k) {
            ground.push(Opening {
                at,
                width: 1.25,
                bottom: 0.9,
                top: ground_top,
                door: true,
            });
            high.push(Opening {
                at,
                width: 1.15,
                bottom: 0.75,
                top: h - floor - 1.0,
                door: true,
            });
        }
    }
    ground.sort_by(|a, b| a.at.total_cmp(&b.at));
    high.sort_by(|a, b| a.at.total_cmp(&b.at));
    lower.build(shape, wall, &ground, u32::MAX);
    upper.build(shape, wall, &high, u32::MAX);
    for o in &ground {
        arch(shape, (base, trim), &lower, o);
        keystone(shape, trim, &lower, o);
        grille(shape, &lower, o);
    }
    rusticate_between(shape, base, &lower, &ground, (0.9, floor - 0.15), 0.6);
    for o in &high {
        surround(shape, trim, &upper, o, true);
        // Un frontoncito sobre cada ventana alta.
        let (a, b) = o.span();
        let (start, along) = (upper.at(a - 0.2, 0.0, 0.0), upper.along);
        let x0 = start.x;
        let x1 = x0 + along.x * (b - a + 0.4);
        pediment(
            shape,
            trim,
            (x0.min(x1), x0.max(x1)),
            walls.z1,
            floor + o.top + 0.24,
            0.32,
            0.14,
        );
        // El antepecho con balaustres.
        for k in 0..4 {
            let along = a + (b - a) * (k as f32 + 0.5) / 4.0;
            baluster(shape, trim, upper.at(along, 0.08, o.bottom - 0.6), 0.55);
        }
        upper.piece(
            shape,
            trim,
            (a - 0.1, -0.06, o.bottom - 0.66),
            (b + 0.1, 0.02, o.bottom - 0.6),
        );
    }
    let f = Front {
        wall,
        trim,
        plinth: base,
        bays: 2 * per_wing + 1,
        doors: vec![per_wing],
        door: (1.2, 2.6),
        window: (1.1, 1.0, ground_top - 0.4),
        parapet: 1.0,
        crown: Crown::Balustrade,
        crest: false,
        urns: true,
        roof: Roof::Flat,
        grilles: true,
        hoods: true,
        transom: true,
    };
    back_and_sides(shape, &walls, &f, v);
    plinth(shape, base, &lower, &ground, 0.9, 0.08);
    // La cornisa intermedia entre las plantas, alrededor de todo el edificio.
    ring(shape, trim, &walls, 0.07, (floor - 0.12, floor + 0.1), 0.14);
    corner_piers(shape, trim, &walls, 0.7, PROUD, (-DROP, h - 0.55));
    dentils(shape, trim, &front, (0.7, len - 0.7), h - 0.55);
    cornice(shape, trim, &walls);
    let end = 0.25;
    let posts: Vec<f32> = [end, p0 - 0.3, p1 + 0.3, len - end].to_vec();
    parapet(
        shape,
        trim,
        &walls,
        f.parapet,
        (Crown::Balustrade, true),
        &posts,
    );
    for at in [end, len - end] {
        urn(
            shape,
            trim,
            front.at(at, walls.thickness * 0.3, h + f.parapet + 0.18),
        );
    }
    flat_roof(shape, &walls);
    pavilion(shape, &walls, (p0, p1), (floor, out), (wall, trim, base));
    // El mástil de la bandera, sobre la azotea detrás del ático.
    let mid = (walls.x0 + walls.x1) / 2.0;
    let foot = Vec3::new(mid, h - 0.1, walls.z1 - 1.6);
    let top = h + 7.0;
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(foot),
        &[(0.07, 0.0), (0.045, top - foot.y), (0.0, top - foot.y)],
        6,
    );
    shape.lathe(
        Surface::Bronze,
        Transform::from_translation(foot.with_y(top)),
        &[(0.0, 0.0), (0.1, 0.08), (0.1, 0.14), (0.0, 0.24)],
        8,
    );
    super::foot(shape, &walls);
}

/// El pabellón central de la intendencia, de `p0` a `p1` a lo largo del frente
/// de `walls`, que sobresale `out` de él: la puerta de arco abajo, la puerta
/// balcón arriba (sobre el piso `floor`), pilastras en ambas plantas, su
/// cornisa, el ático y el frontón con el reloj.
fn pavilion(
    shape: &mut Shape,
    walls: &Walls,
    (p0, p1): (f32, f32),
    (floor, out): (f32, f32),
    (wall, trim, base): (Surface, Surface, Surface),
) {
    let h = walls.height;
    let face = walls.z1 + out;
    let pav = p1 - p0;
    let lower = Wall {
        start: Vec3::new(walls.x0 + p0, 0.0, face),
        along: Vec3::X,
        out: Vec3::Z,
        length: pav,
        thickness: out + 0.05,
        height: floor,
        drop: DROP,
    };
    let upper = Wall {
        start: lower.start + Vec3::Y * floor,
        height: h - floor,
        drop: 0.0,
        ..lower
    };
    let door = Opening {
        at: pav / 2.0,
        width: 2.1,
        bottom: 0.0,
        top: floor - 0.35,
        door: true,
    };
    lower.build(shape, base, &[door], u32::MAX);
    arch(shape, (base, trim), &lower, &door);
    keystone(shape, trim, &lower, &door);
    door_leaves(shape, &lower, &door, true);
    rusticate_between(shape, base, &lower, &[door], (0.0, floor - 0.15), 0.05);
    let balcony = Opening {
        at: pav / 2.0,
        width: 1.5,
        bottom: 0.15,
        top: h - floor - 0.8,
        door: true,
    };
    upper.build(shape, wall, &[balcony], u32::MAX);
    arch(shape, (wall, trim), &upper, &balcony);
    keystone(shape, trim, &upper, &balcony);
    // El balcón, enrasado con el frente: balaustres dentro del vano.
    let (a, b) = balcony.span();
    for k in 0..6 {
        let at = a + (b - a) * (k as f32 + 0.5) / 6.0;
        baluster(shape, trim, upper.at(at, 0.1, balcony.bottom), 0.85);
    }
    upper.piece(
        shape,
        trim,
        (a, 0.02, balcony.bottom + 0.85),
        (b, 0.18, balcony.bottom + 0.95),
    );
    // Pilastras en las dos plantas, en las esquinas y junto a las puertas.
    for at in [0.35, pav - 0.35, pav / 2.0 - 1.6, pav / 2.0 + 1.6] {
        pilaster(
            shape,
            trim,
            &lower,
            at,
            0.55,
            0.08,
            (-DROP, 0.9, floor - 0.15),
        );
        pilaster(
            shape,
            trim,
            &upper,
            at,
            0.5,
            0.08,
            (0.12, 0.45, h - floor - 0.55),
        );
    }
    // La cornisa intermedia y la de arriba, sobre el frente del pabellón.
    lower.piece(
        shape,
        trim,
        (-0.06, -0.07, floor - 0.12),
        (pav + 0.06, out, floor + 0.1),
    );
    for (o, y0, y1) in [(0.03, h - 0.55, h - 0.2), (0.08, h - 0.2, h + 0.04)] {
        lower.piece(shape, trim, (-o, -o, y0), (pav + o, out, y1));
    }
    // El ático macizo, con el nombre, y su albardilla.
    let attic = h + TOWN_HALL_ATTIC;
    lower.piece(shape, wall, (0.0, 0.0, h + 0.04), (pav, 0.5, attic));
    for at in [0.3, pav - 0.3] {
        lower.piece(
            shape,
            trim,
            (at - 0.3, -0.06, h + 0.04),
            (at + 0.3, 0.5, attic + 0.08),
        );
    }
    lower.piece(
        shape,
        trim,
        (-0.05, -0.08, attic),
        (pav + 0.05, 0.55, attic + 0.12),
    );
    // El frontón, con el reloj en el tímpano.
    let (x0, x1) = (walls.x0 + p0, walls.x0 + p1);
    let rise = 1.5;
    pediment(
        shape,
        trim,
        (x0 + 0.1, x1 - 0.1),
        face,
        attic + 0.12,
        rise,
        0.7,
    );
    clock(
        shape,
        Vec3::new((x0 + x1) / 2.0, attic + 0.12 + rise * 0.4, face - 0.06),
    );
}

/// El reloj municipal en el tímpano del frontón, con su cara en `at` (mirando
/// hacia `+Z`): una esfera blanca dentro de un aro de bronce, sus horas y sus
/// agujas.
fn clock(shape: &mut Shape, at: Vec3) {
    let toward = Quat::from_rotation_x(FRAC_PI_2);
    let r = 0.42;
    shape.lathe(
        Surface::Bronze,
        Transform::from_translation(at).with_rotation(toward),
        &[(0.0, 0.0), (r + 0.07, 0.0), (r + 0.07, 0.07), (0.0, 0.07)],
        20,
    );
    shape.lathe(
        Surface::Whitewash,
        Transform::from_translation(at + Vec3::Z * 0.07).with_rotation(toward),
        &[(0.0, 0.0), (r, 0.0), (r, 0.015), (0.0, 0.015)],
        20,
    );
    let dial = at + Vec3::Z * 0.09;
    for k in 0..12 {
        let angle = k as f32 * std::f32::consts::TAU / 12.0;
        let dir = Vec3::new(angle.sin(), angle.cos(), 0.0);
        shape.beam(
            Surface::Steel,
            dial + dir * (r - 0.1),
            dial + dir * (r - 0.03),
            0.025,
        );
    }
    // Las diez y diez.
    for (angle, length) in [
        (-0.35 * std::f32::consts::PI, 0.22),
        (0.35 * std::f32::consts::PI, 0.32),
    ] {
        let dir = Vec3::new(angle.sin(), angle.cos(), 0.0);
        shape.beam(Surface::Steel, dial, dial + dir * length, 0.03);
    }
}
