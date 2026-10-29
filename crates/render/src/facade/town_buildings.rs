//! Los edificios del pueblo hechos con el kit de fachadas: los frentes
//! italianizantes de 1919 de un pueblo ferroviario pampeano joven
//! (`odd/setting-1919.md`).
//!
//! - Casas: frentes de una planta sobre un zócalo, encalados, blanqueados a la
//!   cal, pintados en un color apagado de 1919 (blanco cal, ocre, rosa pálido,
//!   gris azulado, terracota clara) sobre una franja beige, de ladrillo visto,
//!   de ladrillo bajo un encalado gastado o, de vez en cuando, de ladrillo con
//!   el revoque descascarándose, con pilares esquineros ochavados y pilastras,
//!   una cornisa moldurada y un parapeto (macizo o con balaustrada, algunos con
//!   macetones o un pequeño frontón) que oculta una azotea plana o un techo de
//!   chapa acanalada; en las afueras la mayoría son ranchos más simples de
//!   adobe o encalados bajo chapa acanalada.
//! - El almacén de ramos generales: un parapeto alto y macizo detrás de su
//!   cartel y una galería sobre la vereda con postes de hierro fundido; la
//!   pulpería de un pueblo pequeño es encalada, con rejas y bajo una galería
//!   sobre postes de madera.
//! - La comisaría: encalada, con ventanas enrejadas, un parapeto con
//!   balaustrada y un frontón, y su mástil.
//! - La iglesia: un frente con frontón y un campanario con un cuerpo de
//!   campanas de arcos de medio punto bajo una aguja de zinc.
//! - La estación: un frente de ladrillo al estilo del FCO con pilastras de
//!   revoque, aberturas con guardapolvo y un pabellón central con frontón tanto
//!   del lado de la calle como del de la vía, bajo un techo galvanizado, y la
//!   marquesina del andén que core le da a la cámara, vestida con una tabla de
//!   frente y un faldón de madera sobre columnas de hierro fundido que se
//!   apoyan en el andén.
//! - Galpones y depósitos de granos: galpones de chapa acanalada; el tanque de
//!   agua y el molino son el molino y el tanque australiano de los
//!   asentamientos.
//!
//! Cada pared de mampostería se retira [`PROUD`] de su huella de edificio, así
//! que su ornamentación queda dentro de la huella con la que colisiona core y
//! de la que cuelgan, justo por fuera, los carteles de fachada.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use bevy::prelude::*;
use pampa_core::{Building, BuildingKind, Canopy, Side, StoreStyle, Structure, StructureKind};

use super::italianate::{
    CHAMFER, Crown, PROUD, Posts, arch, corner_piers, cornice, door_leaves, gallery, grille,
    parapet, pediment, pilaster, plinth, spire, surround, urn,
};
use super::materials::{Paint, Surface};
use super::shape::Shape;
use super::walls::{
    BARN_REACH, Opening, Wall, Walls, barn, chimney, flat_roof, gable_roof, lean_to, spread, wedge,
};
use crate::settlement::props;

/// Cuánto bajan las paredes bajo el suelo en el centro de un edificio, en
/// metros.
pub const DROP: f32 = 0.3;

/// Altura de la parte superior del andén de la estación sobre el suelo bajo la
/// estación, en metros: el andén de core (`station_yard`), sobre el que se
/// apoyan las columnas de la marquesina.
pub const PLATFORM_TOP: f32 = 0.9;

/// De qué depende el aspecto de un edificio del pueblo además de su
/// [`Building`] de core.
#[derive(Debug, Clone, Copy)]
pub struct Lot {
    pub building: Building,
    /// Una variante con semilla: qué encalado, techo y ornamentación recibe.
    pub variant: u32,
    /// En las manzanas exteriores del pueblo, donde las casas son más simples.
    pub outskirts: bool,
    /// Qué es el almacén de ramos generales del pueblo.
    pub store: StoreStyle,
    /// Qué profundidad tiene la galería del almacén de ramos generales (la
    /// galería del palenque).
    pub veranda: f32,
}

impl Lot {
    /// Ancho a lo largo de su frente y profundidad hacia atrás.
    pub fn size(&self) -> Vec2 {
        let s = self.building.footprint.size();
        match self.building.front {
            Side::North | Side::South => Vec2::new(s.x, s.y),
            Side::East | Side::West => Vec2::new(s.y, s.x),
        }
    }

    /// La estructura de los asentamientos del mismo tamaño, para el molino y el
    /// tanque que comparten con los pueblos.
    fn structure(&self, kind: StructureKind) -> Structure {
        Structure {
            kind,
            footprint: self.building.footprint,
            height: self.building.height,
            front: self.building.front,
            variant: self.variant,
        }
    }
}

/// La guiñada que orienta el `+Z` propio de un edificio hacia su `front`.
pub fn yaw(front: Side) -> Quat {
    Quat::from_rotation_y(match front {
        Side::South => 0.0,
        Side::North => PI,
        Side::East => FRAC_PI_2,
        Side::West => -FRAC_PI_2,
    })
}

/// El edificio de `lot` en su propio espacio (salvo la rueda de su molino), o
/// `None` para el andén, que el pueblo dibuja como un piso liso.
pub fn building(lot: &Lot) -> Option<Shape> {
    let size = lot.size();
    let (w, d, h) = (size.x, size.y, lot.building.height);
    let v = lot.variant;
    let mut shape = Shape::default();
    match lot.building.kind {
        BuildingKind::House => house(&mut shape, (w, d, h), v, lot.outskirts),
        BuildingKind::GeneralStore => store(&mut shape, (w, d, h), v, lot.store, lot.veranda),
        BuildingKind::PoliceStation => police(&mut shape, (w, d, h), v),
        BuildingKind::Church => church(&mut shape, (w, d, h), v),
        BuildingKind::Station => station(&mut shape, (w, d, h), v),
        BuildingKind::Shed | BuildingKind::GrainStore => {
            let iron = if v & 1 == 0 {
                Surface::OldIron
            } else {
                Surface::Galvanised
            };
            let doors = if lot.building.kind == BuildingKind::GrainStore {
                ((w / 14.0) as usize).clamp(1, 4)
            } else {
                1
            };
            // Sus paredes se retiran del lote para que los aleros terminen
            // sobre él; core colisiona en las paredes (`Building::collider`).
            let inside = (w - 2.0 * BARN_REACH, d - 2.0 * BARN_REACH, h);
            barn(&mut shape, (iron, Surface::OldIron), inside, DROP, v, doors);
        }
        BuildingKind::WaterTank => {
            shape = props::tank(&lot.structure(StructureKind::Tank), DROP);
        }
        BuildingKind::Windmill => {
            shape = props::windpump(&lot.structure(StructureKind::Windpump), DROP);
        }
        BuildingKind::Platform => return None,
    }
    Some(shape)
}

/// Dónde gira la rueda de un molino en su propio espacio, y su guiñada.
pub fn rotor_place(lot: &Lot) -> (Vec3, Quat) {
    props::rotor_place(&lot.structure(StructureKind::Windpump))
}

/// Las paredes de un edificio de mampostería de `w` por `d`, retiradas
/// [`PROUD`] de su huella de edificio todo alrededor.
fn masonry((w, d, h): (f32, f32, f32), thickness: f32) -> Walls {
    Walls {
        x0: -w / 2.0 + PROUD,
        x1: w / 2.0 - PROUD,
        z0: -d / 2.0 + PROUD,
        z1: d / 2.0 - PROUD,
        thickness,
        height: h,
        drop: DROP,
    }
}

/// Lo que hay detrás de un parapeto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Roof {
    /// Una azotea plana de ladrillo detrás de un parapeto todo alrededor.
    Flat,
    /// Chapa acanalada que cae hacia atrás detrás de un parapeto en el frente.
    Iron(Surface),
}

/// Un frente italianizante y la caja detrás de él.
#[derive(Debug, Clone)]
struct Front {
    wall: Surface,
    /// Pilares, pilastras, cornisa, parapeto y marcos.
    trim: Surface,
    plinth: Surface,
    bays: usize,
    doors: Vec<usize>,
    /// Ancho y alto de las puertas.
    door: (f32, f32),
    /// Ancho de las ventanas y alturas del alféizar y del dintel.
    window: (f32, f32, f32),
    parapet: f32,
    crown: Crown,
    /// Un pequeño frontón sobre el centro del parapeto.
    crest: bool,
    urns: bool,
    roof: Roof,
    grilles: bool,
    hoods: bool,
    transom: bool,
}

/// Construye `f` sobre `walls`: las paredes con sus aberturas, el zócalo, los
/// pilares, las pilastras entre los vanos, la cornisa, el parapeto y el techo.
fn italianate(shape: &mut Shape, walls: &Walls, f: &Front, v: u32) {
    let (h, t) = (walls.height, walls.thickness);
    let front = walls.front();
    let len = front.length;
    let openings = spread(len, f.bays, &f.doors, f.door, f.window);
    // Cada puerta recibe sus propias hojas abajo, así que la pared las deja
    // abiertas.
    front.build(shape, f.wall, &openings, u32::MAX);
    dress(shape, &front, &openings, f);
    let back_n = (f.bays / 2).max(1);
    let back = spread(len, back_n, &[0], (1.0, f.door.1.min(2.4)), f.window);
    walls.back().build(shape, f.wall, &back, v);
    plinth(shape, f.plinth, &walls.back(), &back, 0.6, 0.04);
    for east in [true, false] {
        let side = walls.side(east);
        let n = (side.length / 4.5) as usize;
        let windows = spread(side.length, n, &[], (0.0, 0.0), f.window);
        side.build(shape, f.wall, &windows, v);
        plinth(shape, f.plinth, &side, &windows, 0.6, 0.04);
    }
    plinth(shape, f.plinth, &front, &openings, 0.6, 0.05);
    let capital = h - 0.55;
    for k in 1..f.bays {
        let at = len * k as f32 / f.bays as f32;
        pilaster(
            shape,
            f.trim,
            &front,
            at,
            0.42,
            0.08,
            (-DROP, 0.75, capital),
        );
    }
    corner_piers(shape, f.trim, walls, 0.55, PROUD, (-DROP, capital));
    cornice(shape, f.trim, walls);
    // Pedestales sobre los pilares esquineros y las pilastras.
    let mut pillars: Vec<f32> = (1..f.bays)
        .map(|k| len * k as f32 / f.bays as f32)
        .collect();
    // Sobre los pilares esquineros, con sus remates dentro de la huella de
    // edificio.
    let end = 0.2;
    pillars.extend([end, len - end]);
    parapet(
        shape,
        f.trim,
        walls,
        f.parapet,
        (f.crown, f.roof == Roof::Flat),
        &pillars,
    );
    let top = h + f.parapet + 0.18;
    if f.urns {
        for at in [end, len - end] {
            urn(shape, f.trim, front.at(at, t * 0.3, top));
        }
    }
    if f.crest {
        let half = (len * 0.18).clamp(1.0, 2.2);
        let mid = (walls.x0 + walls.x1) / 2.0;
        pediment(
            shape,
            f.trim,
            (mid - half, mid + half),
            walls.z1,
            h + f.parapet + 0.08,
            0.55,
            t * 0.6,
        );
    }
    match f.roof {
        Roof::Flat => flat_roof(shape, walls),
        Roof::Iron(iron) => iron_behind(shape, (iron, f.wall), walls, f.parapet),
    }
}

/// Marcos, hojas, rejas y guardapolvos en las `openings` de `wall`.
fn dress(shape: &mut Shape, wall: &Wall, openings: &[Opening], f: &Front) {
    for o in openings {
        if o.door {
            door_leaves(shape, wall, o, f.transom);
            surround(shape, f.trim, wall, o, false);
        } else {
            surround(shape, f.trim, wall, o, f.hoods);
            if f.grilles {
                grille(shape, wall, o);
            }
        }
    }
}

/// Un techo acanalado oculto detrás de un parapeto de `parapet` de alto en el
/// frente, que cae desde justo debajo de su albardilla hasta más allá de la
/// pared del fondo; las paredes laterales se levantan en `gable` hasta
/// alcanzarlo.
fn iron_behind(shape: &mut Shape, (roof, gable): (Surface, Surface), walls: &Walls, parapet: f32) {
    let (h, t) = (walls.height, walls.thickness);
    let (x0, x1, z0, z1) = (walls.x0, walls.x1, walls.z0, walls.z1);
    let (high, low) = (h + parapet - 0.25, h + 0.05);
    let zf = z1 - t * 0.6;
    let slope = (high - low) / (zf - z0);
    let under = |z: f32| low + (z - z0) * slope;
    // Se mantiene dentro de la huella de edificio, incluida la inclinación de
    // la chapa.
    let overhang = PROUD - 0.03;
    super::walls::slab(
        shape,
        roof,
        Vec3::new(0.0, under(z0 - overhang), z0 - overhang),
        Vec3::new(0.0, high, zf),
        walls.width() + 0.2,
        0.06,
    );
    for xs in [(x0, x0 + t), (x1 - t, x1)] {
        wedge(shape, gable, xs, (z0, zf), h, (under(z0), high));
    }
    wedge(
        shape,
        gable,
        (x0 + t, x1 - t),
        (z0, z0 + t),
        h,
        (under(z0), under(z0 + t)),
    );
}

/// Una casa: italianizante en el pueblo, casi siempre un rancho simple en las
/// afueras.
fn house(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32, outskirts: bool) {
    if outskirts && v % 20 < 13 {
        return cottage(shape, (w, d, h), v);
    }
    let (wall, trim, plinth) = house_finish(v);
    let walls = masonry((w, d, h), 0.4);
    let bays = ((walls.width() / 2.8) as usize).clamp(2, 5);
    // La puerta del zaguán en un extremo y ventanas en el resto.
    let door = if v & 0x40 == 0 { 0 } else { bays - 1 };
    let head = (h - 0.85).min(2.7);
    let f = Front {
        wall,
        trim,
        plinth,
        bays,
        doors: vec![door],
        door: (1.1, head),
        window: (0.9, 0.85, head),
        parapet: 0.85,
        crown: if v & 0x80 != 0 {
            Crown::Balustrade
        } else {
            Crown::Solid
        },
        crest: v & 0x300 == 0x300,
        urns: v & 0x400 != 0,
        roof: match (v >> 11) % 5 {
            0 => Roof::Iron(Surface::OldIron),
            1 => Roof::Iron(Surface::Galvanised),
            _ => Roof::Flat,
        },
        grilles: v & 0x2000 != 0,
        hoods: v & 0x4000 != 0,
        transom: true,
    };
    italianate(shape, &walls, &f, v);
}

/// La pared, las molduras y el zócalo de una casa del pueblo, según los bits
/// bajos de su variante (`v % 16`, así que los demás bits quedan libres para la
/// ornamentación): de 16 casas, 6 están pintadas (2 ocre, una de cada una de
/// las otras pinturas), 3 blanqueadas a la cal, 2 encaladas, 2 de ladrillo
/// visto, 2 de ladrillo bajo un encalado gastado y 1 con el revoque
/// descascarándose del ladrillo, ya que el pueblo es joven.
pub(crate) fn house_finish(v: u32) -> (Surface, Surface, Surface) {
    let wall = match v % 16 {
        0 | 1 => Surface::Whitewash,
        2 | 3 | 15 => Surface::Lime,
        4 | 5 => Surface::Brick,
        6 => Surface::Painted(Paint::LimeWhite),
        7 | 8 => Surface::Painted(Paint::Ochre),
        9 => Surface::Painted(Paint::PaleRose),
        10 => Surface::Painted(Paint::GreyBlue),
        11 => Surface::Painted(Paint::Terracotta),
        12 | 13 => Surface::WashedBrick,
        _ => Surface::PatchedBrick,
    };
    let stucco_trim = v & 0x20 != 0;
    match wall {
        // Molduras blancas en un frente de color, grises en uno blanco.
        Surface::Painted(Paint::LimeWhite) => (wall, Surface::Stucco, Surface::Band),
        Surface::Painted(_) => (wall, Surface::Whitewash, Surface::Band),
        Surface::Brick if stucco_trim => (wall, Surface::Stucco, Surface::Stucco),
        Surface::WashedBrick if stucco_trim => (wall, Surface::Whitewash, Surface::Band),
        Surface::PatchedBrick => (wall, Surface::Stucco, Surface::Stucco),
        _ => (wall, Surface::Whitewash, Surface::Stucco),
    }
}

/// Un rancho de las afueras: paredes gruesas de adobe o encaladas, retiradas
/// bajo un techo de chapa acanalada, una puerta y una o dos ventanas, y una
/// chimenea.
fn cottage(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let inset = 0.4;
    let walls = Walls {
        x0: -w / 2.0 + inset,
        x1: w / 2.0 - inset,
        z0: -d / 2.0 + inset,
        z1: d / 2.0 - inset,
        thickness: 0.5,
        height: (h - 0.6).max(2.8),
        drop: DROP,
    };
    // Adobe o encalado; uno de cada cuatro ranchos encalados es de ladrillo.
    let wall = match (v & 1, (v >> 4) % 4) {
        (0, _) => Surface::Adobe,
        (_, 0) => Surface::WashedBrick,
        _ => Surface::Whitewash,
    };
    let len = walls.width();
    let rooms = if len > 8.0 { 3 } else { 2 };
    let door = if rooms == 3 { vec![1] } else { vec![0] };
    walls.front().build(
        shape,
        wall,
        &spread(len, rooms, &door, (1.0, 2.0), (0.75, 0.85, 1.7)),
        v,
    );
    walls.back().build(
        shape,
        wall,
        &spread(len, 1, &[], (0.0, 0.0), (0.6, 0.9, 1.5)),
        v,
    );
    for east in [true, false] {
        walls.side(east).build(shape, wall, &[], v);
    }
    if v & 2 == 0 {
        lean_to(shape, (Surface::OldIron, wall), &walls, 0.7, inset - 0.05);
    } else {
        let rise = walls.depth() / 2.0 * 0.4;
        gable_roof(
            shape,
            (Surface::OldIron, wall),
            &walls,
            rise,
            inset - 0.05,
            0.06,
        );
    }
    let top = walls.height + 1.4;
    chimney(
        shape,
        Vec3::new(walls.x1 - 1.0, walls.height, walls.z0 + 0.8),
        top,
    );
}

/// El almacén de ramos generales, o la pulpería de un pueblo pequeño, con su
/// galería.
fn store(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32, style: StoreStyle, veranda: f32) {
    let walls = masonry((w, d, h), 0.45);
    let bays = (((walls.width() / 3.2) as usize).clamp(3, 7)) | 1;
    // Puertas en el centro y en un vano de por medio, vidrieras entre ellas.
    let doors: Vec<usize> = (0..bays).filter(|k| k % 2 == (bays / 2) % 2).collect();
    let general = style == StoreStyle::GeneralStore;
    // Un almacén es de ladrillo, blanqueado a la cal o pintado de ocre o de
    // gris azulado; una pulpería, encalada, blanqueada a la cal o de ladrillo
    // viejo que pierde su revoque.
    let wall = match (general, v % 6) {
        (true, 0) => Surface::Lime,
        (true, 1 | 2) => Surface::Brick,
        (true, 3) => Surface::WashedBrick,
        (true, 4) => Surface::Painted(Paint::Ochre),
        (true, _) => Surface::Painted(Paint::GreyBlue),
        (false, 0 | 1) => Surface::Lime,
        (false, 2) => Surface::PatchedBrick,
        (false, _) => Surface::Whitewash,
    };
    let plinth = match wall {
        Surface::Painted(_) | Surface::WashedBrick => Surface::Band,
        _ => Surface::Stucco,
    };
    let f = Front {
        wall,
        // Molduras y parapeto encalados sobre el ladrillo o el encalado.
        trim: Surface::Whitewash,
        plinth,
        bays,
        doors,
        door: (1.3, (h - 1.3).min(3.4)),
        window: (1.3, 0.7, (h - 1.3).min(3.4)),
        // Alto y macizo: de él cuelga el cartel del almacén.
        parapet: 2.3,
        crown: Crown::Solid,
        crest: general,
        urns: general,
        roof: if v & 8 == 0 {
            Roof::Flat
        } else {
            Roof::Iron(Surface::OldIron)
        },
        grilles: !general,
        hoods: general,
        transom: true,
    };
    italianate(shape, &walls, &f, v);
    let (roof, posts) = if general {
        (Surface::Galvanised, Posts::Iron)
    } else {
        (Surface::OldIron, Posts::Timber)
    };
    // Desde debajo de la cornisa, hacia afuera sobre la vereda hasta el borde
    // de la huella de edificio del lado de la calle más la galería.
    let mut over = walls.front();
    over.start.x -= PROUD;
    over.length += 2.0 * PROUD;
    let high = h - 0.7;
    let depth = veranda + PROUD;
    gallery(shape, roof, posts, &over, depth, (high, high - 0.3 * depth));
}

/// La comisaría: encalada, enrejada, con balaustrada, con su mástil.
fn police(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let walls = masonry((w, d, h), 0.45);
    let bays = (((walls.width() / 2.9) as usize).clamp(3, 7)) | 1;
    let f = Front {
        wall: Surface::Whitewash,
        trim: Surface::Whitewash,
        plinth: Surface::Stucco,
        bays,
        doors: vec![bays / 2],
        // Lo bastante bajo para el cartel sobre la puerta.
        door: (1.4, 2.5),
        window: (1.0, 0.9, 2.5),
        parapet: 1.0,
        crown: Crown::Balustrade,
        crest: true,
        urns: true,
        roof: Roof::Flat,
        grilles: true,
        hoods: false,
        transom: true,
    };
    italianate(shape, &walls, &f, v);
    // El mástil sobre la azotea plana, junto a una esquina del frente, con su
    // remate.
    let foot = Vec3::new(walls.x1 - 1.0, h - 0.1, walls.z1 - 1.0);
    let top = h + 5.5;
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(foot),
        &[(0.06, 0.0), (0.04, top - foot.y), (0.0, top - foot.y)],
        6,
    );
    shape.lathe(
        Surface::Steel,
        Transform::from_translation(foot.with_y(top)),
        &[(0.0, 0.0), (0.08, 0.06), (0.08, 0.12), (0.0, 0.18)],
        6,
    );
}

/// Cuánto sobresale el campanario de la iglesia del frente de su nave, en
/// metros.
const TOWER_OUT: f32 = 0.7;

/// La iglesia: una nave con frontón y ventanas de arco y un campanario en el
/// centro de su frente.
fn church(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    // La nave se retira detrás de su campanario, que llega hasta la huella de
    // edificio.
    let mut walls = masonry((w, d, h), 0.6);
    walls.z1 -= TOWER_OUT;
    let (wall, trim) = (Surface::Whitewash, Surface::Whitewash);
    let tower = 4.0_f32.min(walls.width() - 2.0);
    let len = walls.width();
    // El frente de la nave: una ventana de arco a cada lado del campanario.
    let side = (len - tower) / 2.0;
    let mut front_openings = Vec::new();
    if side > 2.4 {
        for at in [side / 2.0, len - side / 2.0] {
            front_openings.push(Opening {
                at,
                width: 1.1,
                bottom: 1.6,
                top: (h - 1.4).min(5.0),
                // Vidriada, sin postigos: construida como una puerta abierta.
                door: true,
            });
        }
    }
    let front = walls.front();
    front.build(shape, wall, &front_openings, u32::MAX);
    for o in &front_openings {
        arch(shape, (wall, trim), &front, o);
    }
    plinth(shape, Surface::Stucco, &front, &front_openings, 0.7, 0.05);
    walls.back().build(shape, wall, &[], 0);
    plinth(shape, Surface::Stucco, &walls.back(), &[], 0.7, 0.04);
    for east in [true, false] {
        let side = walls.side(east);
        let n = ((side.length / 4.0) as usize).max(1);
        let windows: Vec<Opening> = (0..n)
            .map(|k| Opening {
                at: side.length * (k as f32 + 0.5) / n as f32,
                width: 1.1,
                bottom: 2.0,
                top: (h - 1.2).min(5.4),
                // Vidriada, sin postigos: construida como una puerta abierta.
                door: true,
            })
            .collect();
        side.build(shape, wall, &windows, u32::MAX);
        for o in &windows {
            arch(shape, (wall, trim), &side, o);
        }
        plinth(shape, Surface::Stucco, &side, &windows, 0.7, 0.04);
        // Pilastras de contrafuerte entre las ventanas.
        for k in 0..=n {
            let at = (side.length * k as f32 / n as f32).clamp(0.3, side.length - 0.3);
            pilaster(shape, trim, &side, at, 0.5, 0.09, (-DROP, 0.85, h - 0.55));
        }
    }
    corner_piers(shape, trim, &walls, 0.7, PROUD, (-DROP, h - 0.55));
    cornice(shape, trim, &walls);
    // El techo a dos aguas de la nave va hacia atrás desde el frente: se
    // construye a lo largo de X y después se gira para que su cumbrera corra a
    // lo largo de Z.
    let rise = len / 2.0 * 0.55;
    let turned = Walls {
        x0: walls.z0,
        x1: walls.z1,
        z0: walls.x0,
        z1: walls.x1,
        ..walls
    };
    let mut roof = Shape::default();
    gable_roof(
        &mut roof,
        (Surface::OldIron, wall),
        &turned,
        rise,
        0.05,
        0.08,
    );
    shape.merge(
        roof,
        Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2)),
    );
    // El frontón sobre el hastial del frente.
    pediment(
        shape,
        trim,
        (walls.x0, walls.x1),
        walls.z1 + PROUD - 0.06,
        h + 0.04,
        rise + 0.1,
        0.3,
    );
    bell_tower(shape, &walls, tower, rise + 0.5, v);
}

/// El campanario de la iglesia, de `size` de lado, que sobresale [`TOWER_OUT`]
/// del frente de la nave `nave` y sube `over` por encima de su cornisa: una
/// puerta bajo un arco de medio punto, un cuerpo de campanas de arcos de medio
/// punto, un tambor y una aguja de zinc con una cruz de hierro.
fn bell_tower(shape: &mut Shape, nave: &Walls, size: f32, over: f32, _v: u32) {
    let (wall, trim) = (Surface::Whitewash, Surface::Whitewash);
    let h = nave.height;
    let front = nave.z1 + TOWER_OUT;
    let shaft = Walls {
        x0: -size / 2.0,
        x1: size / 2.0,
        z0: front - size,
        z1: front,
        thickness: 0.5,
        height: h + over,
        drop: DROP,
    };
    let door = Opening {
        at: size / 2.0,
        width: 1.6,
        bottom: 0.0,
        top: 3.4,
        door: true,
    };
    shaft.front().build(shape, wall, &[door], u32::MAX);
    let square = Opening { top: 2.6, ..door };
    door_leaves(shape, &shaft.front(), &square, false);
    arch(shape, (wall, trim), &shaft.front(), &door);
    plinth(shape, Surface::Stucco, &shaft.front(), &[door], 0.7, PROUD);
    for east in [true, false] {
        shaft.side(east).build(shape, wall, &[], 0);
    }
    // Pilares ochavados subiendo por las esquinas del fuste, y una cornisa
    // encima.
    corner_piers(shape, trim, &shaft, 0.45, PROUD, (-DROP, h + over));
    let mut level = shaft;
    level.height = h + over;
    cornice(shape, trim, &level);
    // El cuerpo de campanas: cuatro paredes de arcos de medio punto sobre el
    // fuste.
    let base = h + over;
    let belfry = Walls {
        x0: -size / 2.0 + 0.15,
        x1: size / 2.0 - 0.15,
        z0: front - size + 0.15,
        z1: front - 0.15,
        thickness: 0.45,
        height: 3.6,
        drop: 0.0,
    };
    let lift = |mut wall: Wall| {
        wall.start.y = base;
        wall
    };
    let open = Opening {
        at: belfry.width() / 2.0,
        width: 1.3,
        bottom: 0.6,
        top: 3.0,
        // Abierto, sin postigos: construido como una puerta que queda abierta.
        door: true,
    };
    for face in [
        belfry.front(),
        belfry.back(),
        belfry.side(true),
        belfry.side(false),
    ] {
        let face = lift(face);
        let o = Opening {
            at: face.length / 2.0,
            ..open
        };
        face.build(shape, wall, &[o], u32::MAX);
        arch(shape, (wall, trim), &face, &o);
    }
    let mut top = belfry;
    top.height = base + 3.6;
    corner_piers(shape, trim, &top, 0.4, 0.05, (base, base + 3.6));
    cornice(shape, trim, &top);
    // Un tambor cuadrado, y después la aguja y su cruz.
    let drum = size - 0.9;
    let y = base + 3.6 + 0.04;
    shape.chamfered(
        wall,
        Vec3::new(-drum / 2.0, y, front - size / 2.0 - drum / 2.0),
        Vec3::new(drum / 2.0, y + 0.9, front - size / 2.0 + drum / 2.0),
        CHAMFER * 3.0,
    );
    let centre = Vec3::new(0.0, y + 0.9, front - size / 2.0);
    shape.block(
        trim,
        centre - Vec3::new(drum / 2.0 + 0.1, 0.0, drum / 2.0 + 0.1),
        centre + Vec3::new(drum / 2.0 + 0.1, 0.12, drum / 2.0 + 0.1),
    );
    let spire_h = size * 1.1;
    spire(
        shape,
        Surface::Sheet,
        centre + Vec3::Y * 0.12,
        drum,
        spire_h,
    );
    let tip = centre + Vec3::Y * (0.12 + spire_h);
    shape.beam(
        Surface::Steel,
        tip - Vec3::Y * 0.1,
        tip + Vec3::Y * 1.3,
        0.08,
    );
    shape.beam(
        Surface::Steel,
        tip + Vec3::new(-0.35, 0.9, 0.0),
        tip + Vec3::new(0.35, 0.9, 0.0),
        0.08,
    );
}

/// La estación: un frente de ladrillo al estilo del FCO con ornamentos de
/// revoque (un zócalo, pilastras entre sus vanos, aberturas con guardapolvo,
/// una cornisa) y un pabellón central bajo un frontón, igual del lado de la
/// calle y del de la vía, bajo un techo galvanizado a dos aguas; y la
/// marquesina del andén del lado de la vía exactamente donde la pone core,
/// sobre columnas de hierro fundido.
fn station(shape: &mut Shape, (w, d, h): (f32, f32, f32), v: u32) {
    let walls = masonry((w, d, h), 0.45);
    let f = station_front(&walls);
    // El lado de la vía refleja el del pueblo: una sola cara, girada media
    // vuelta.
    let mut face = Shape::default();
    station_face(&mut face, &walls, &f);
    shape.merge(face.clone(), Transform::IDENTITY);
    shape.merge(face, Transform::from_rotation(Quat::from_rotation_y(PI)));
    for east in [true, false] {
        let side = walls.side(east);
        let windows = spread(side.length, 2, &[], (0.0, 0.0), f.window);
        side.build(shape, f.wall, &windows, v);
        dress(shape, &side, &windows, &f);
        plinth(shape, f.plinth, &side, &windows, 0.6, 0.04);
    }
    corner_piers(shape, f.trim, &walls, 0.55, PROUD, (-DROP, h - 0.55));
    cornice(shape, f.trim, &walls);
    // Los aleros terminan justo dentro de la huella de edificio, sobre la
    // cornisa (el grosor de la losa se inclina unos centímetros hacia afuera
    // más allá de su vuelo): el collider, y los carteles con el nombre colgados
    // justo por fuera, siguen siendo correctos.
    gable_roof(
        shape,
        (Surface::Galvanised, Surface::Brick),
        &walls,
        2.2,
        0.05,
        0.08,
    );
    chimney(shape, Vec3::new(walls.x0 + 2.0, h, 0.0), h + 2.4);
    chimney(shape, Vec3::new(walls.x1 - 2.0, h, 0.0), h + 2.4);
    canopy(shape, (w, d));
}

/// Las aberturas y los ornamentos de las paredes largas de una estación.
fn station_front(walls: &Walls) -> Front {
    let bays = pampa_core::station_bays(walls.width());
    Front {
        wall: Surface::Brick,
        trim: Surface::Stucco,
        plinth: Surface::Stucco,
        bays,
        doors: (0..bays).filter(|k| k % 2 == 1).collect(),
        door: (1.3, 2.8),
        window: (1.1, 0.9, 2.8),
        parapet: 0.0,
        crown: Crown::Solid,
        crest: false,
        urns: false,
        roof: Roof::Flat,
        grilles: false,
        hoods: true,
        transom: true,
    }
}

/// Las puertas y ventanas a lo largo de una pared larga de la estación de una
/// huella de edificio de `w` por `d` (iguales en ambas caras, espejadas), como
/// `(first jamb, second jamb)` en metros desde el centro de la pared.
#[cfg(test)]
pub(crate) fn station_openings((w, d, h): (f32, f32, f32)) -> Vec<(f32, f32)> {
    let walls = masonry((w, d, h), 0.45);
    let f = station_front(&walls);
    spread(walls.width(), f.bays, &f.doors, f.door, f.window)
        .iter()
        .map(|o| {
            let (a, b) = o.span();
            (a - walls.width() / 2.0, b - walls.width() / 2.0)
        })
        .collect()
}

/// El frente a la calle de la estación sobre `walls`, visto desde `+Z`: la
/// pared con sus aberturas, el zócalo, las pilastras y el pabellón sobre sus
/// vanos centrales.
fn station_face(shape: &mut Shape, walls: &Walls, f: &Front) {
    let (h, len) = (walls.height, walls.width());
    let front = walls.front();
    let openings = spread(len, f.bays, &f.doors, f.door, f.window);
    front.build(shape, f.wall, &openings, u32::MAX);
    dress(shape, &front, &openings, f);
    plinth(shape, f.plinth, &front, &openings, 0.6, 0.05);
    // Los tres vanos centrales forman el pabellón: sus bordes llevan pilastras
    // más anchas que las que hay entre los demás vanos.
    let middle = f.bays / 2;
    let (first, last) = (middle.saturating_sub(1), (middle + 2).min(f.bays));
    let capital = h - 0.55;
    for k in 1..f.bays {
        let edge = k == first || k == last;
        let width = if edge { 0.7 } else { 0.42 };
        let at = len * k as f32 / f.bays as f32;
        pilaster(
            shape,
            f.trim,
            &front,
            at,
            width,
            0.08,
            (-DROP, 0.75, capital),
        );
    }
    // Por encima de la cornisa el pabellón se eleva en un ático de ladrillo
    // entre extremos de revoque, un remate y un frontón.
    let x0 = walls.x0 + len * first as f32 / f.bays as f32;
    let x1 = walls.x0 + len * last as f32 / f.bays as f32;
    let z = walls.z1;
    let (base, attic) = (h + 0.04, h + 0.95);
    shape.block(
        f.wall,
        Vec3::new(x0 + 0.15, base, z - 0.5),
        Vec3::new(x1 - 0.15, attic, z),
    );
    for (a, b) in [(x0, x0 + 0.15), (x1 - 0.15, x1)] {
        shape.block(
            f.trim,
            Vec3::new(a, base, z - 0.5),
            Vec3::new(b, attic, z + 0.04),
        );
    }
    shape.block(
        f.trim,
        Vec3::new(x0 - 0.08, attic, z - 0.5),
        Vec3::new(x1 + 0.08, attic + 0.13, z + 0.09),
    );
    pediment(
        shape,
        f.trim,
        (x0 - 0.08, x1 + 0.08),
        z - 0.02,
        attic + 0.13,
        1.3,
        0.45,
    );
}

/// La marquesina del andén: la losa de core (`Canopy`) con una tabla de frente,
/// un faldón de madera calada y cabios, sostenida por columnas de hierro
/// fundido apoyadas en el andén con ménsulas de hierro hasta el techo.
fn canopy(shape: &mut Shape, (w, d): (f32, f32)) {
    let (bottom, top) = (
        Canopy::HEIGHT - Canopy::THICKNESS / 2.0,
        Canopy::HEIGHT + Canopy::THICKNESS / 2.0,
    );
    let (z0, z1) = (-d / 2.0 - Canopy::DEPTH, -d / 2.0);
    shape.cuboid(
        Surface::Galvanised,
        Transform::from_translation(Vec3::new(0.0, (bottom + top) / 2.0, (z0 + z1) / 2.0))
            .with_scale(Vec3::new(w, top - bottom, z1 - z0)),
        true,
    );
    // La tabla de frente sobre el borde de la losa (apenas hacia adentro de él,
    // para que los dos nunca compartan una cara) y el faldón debajo: una fila
    // de rombos.
    let edge = z0 + 0.01;
    let fascia = 0.22;
    shape.block(
        Surface::Planks,
        Vec3::new(-w / 2.0, bottom - fascia, edge),
        Vec3::new(w / 2.0, top, edge + 0.07),
    );
    let lozenge = 0.18;
    let count = (w / 0.45) as usize;
    for k in 0..count {
        let x = -w / 2.0 + (k as f32 + 0.5) * w / count as f32;
        shape.cuboid(
            Surface::Planks,
            Transform::from_xyz(x, bottom - fascia - lozenge * 0.7, edge + 0.04)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_4))
                .with_scale(Vec3::new(lozenge, lozenge, 0.04)),
            true,
        );
    }
    // Columnas cada tres a cuatro metros, las dos exteriores justo dentro de
    // los extremos de la losa, un poco hacia adentro de su borde; un cabio
    // sobre cada una cruzando la cara inferior; ménsulas con volutas desde cada
    // una hasta los cabios y la pared.
    let n = ((w - 0.9) / 3.5).ceil().max(1.0) as usize;
    let zc = z0 + 0.4;
    for k in 0..=n {
        let x = -w / 2.0 + 0.45 + (w - 0.9) * k as f32 / n as f32;
        shape.lathe(
            Surface::Steel,
            Transform::from_xyz(x, PLATFORM_TOP, zc),
            &column(bottom - PLATFORM_TOP),
            10,
        );
        shape.block(
            Surface::Planks,
            Vec3::new(x - 0.06, bottom - 0.14, z0 + 0.08),
            Vec3::new(x + 0.06, bottom, z1),
        );
        let neck = Vec3::new(x, bottom - 0.75, zc);
        for side in [-1.0, 1.0] {
            let to = Vec3::new(x + side * 0.9, bottom - 0.14, zc);
            if to.x.abs() < w / 2.0 - 0.05 {
                shape.beam(Surface::Steel, neck, to, 0.05);
            }
        }
        shape.beam(
            Surface::Steel,
            neck,
            Vec3::new(x, bottom - 0.14, zc + 0.9),
            0.05,
        );
    }
}

/// El perfil de una columna de hierro fundido de `height` de alto (radio,
/// altura), desde su base: un plinto, un pie moldurado, un fuste que se
/// angosta, un anillo de collarino y un capitel acampanado bajo el techo.
fn column(height: f32) -> Vec<(f32, f32)> {
    let top = height;
    vec![
        (0.19, 0.0),
        (0.19, 0.12),
        (0.19, 0.12),
        (0.14, 0.16),
        (0.14, 0.16),
        (0.12, 0.24),
        (0.08, 0.34),
        (0.075, 0.5),
        (0.062, top - 0.62),
        (0.09, top - 0.58),
        (0.09, top - 0.52),
        (0.065, top - 0.48),
        (0.065, top - 0.44),
        (0.11, top - 0.34),
        (0.16, top - 0.2),
        (0.2, top - 0.1),
        (0.2, top),
    ]
}

#[cfg(test)]
mod tests;
