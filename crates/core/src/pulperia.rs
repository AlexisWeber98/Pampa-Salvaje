//! El interior de los locales que se recorren ([`walks_in`]): se entra
//! caminando por la puerta del medio del frente. En la pulpería hay un
//! mostrador con reja, estantes, barriles, mesas y bancos; en el almacén de
//! ramos generales, un mostrador largo, estantes hasta el techo, bolsas,
//! barriles, monturas y herramientas, y ninguna mesa.
//!
//! La huella del edificio sigue siendo sólida para ubicar cosas alrededor
//! (palenques, faroles, árboles, vehículos: [`TownLayout::obstacles`]); solo la
//! colisión del mundo y la cámara usan las paredes con su hueco
//! ([`TownLayout::colliders`]).
//!
//! Todo se mide en el marco propio del edificio: `u` a lo largo del frente
//! (hacia la izquierda de quien mira hacia afuera, como el eje +X local del
//! render) y `v` hacia adentro desde el borde de la calle. Las paredes calzan
//! con las que dibuja el render (`masonry` en `town_buildings.rs`): su cara de
//! afuera queda [`PROUD`] hacia adentro de la huella, y miden [`WALL`] de
//! espesor.
//!
//! [`TownLayout::obstacles`]: crate::town::TownLayout::obstacles
//! [`TownLayout::colliders`]: crate::town::TownLayout::colliders

use crate::clock::TimeOfDay;
use crate::places::side_normal;
use crate::rng::Rng;
use crate::town::{Building, BuildingKind};
use crate::{Aabb, Heading, Rider, Vec2};

/// Cuánto queda la cara de afuera de las paredes hacia adentro de la huella,
/// en metros (el `PROUD` del render).
pub const PROUD: f32 = 0.1;
/// El espesor de las paredes de mampostería, en metros.
pub const WALL: f32 = 0.45;
/// El ancho de la puerta, en metros (el de las puertas que dibuja el render):
/// pasa una persona, no un caballo.
pub const DOOR_WIDTH: f32 = 1.3;
/// La altura máxima de la puerta, en metros.
const DOOR_TOP: f32 = 3.4;
/// Cuánto más baja que la pared queda la puerta, en metros.
const DOOR_BELOW_EAVES: f32 = 1.3;

/// Cuánto hay entre el mostrador del almacén y sus estantes, en metros (el
/// lugar del almacenero).
const ALMACEN_BEHIND: f32 = 1.4;
/// La profundidad de los estantes del almacén, en metros.
const GOODS_DEPTH: f32 = 0.5;
/// Cuánto queda entre lo alto de los estantes del almacén y el cielorraso, en
/// metros: llegan al techo.
const GOODS_BELOW_CEILING: f32 = 0.3;
/// Una pila de bolsas, a lo largo y a lo hondo, en metros.
const SACKS: (f32, f32) = (1.4, 0.9);
/// El caballete de las monturas, a lo largo de la pared y hacia adentro, en
/// metros.
const SADDLERY: (f32, f32) = (2.2, 0.8);
/// El tablero de las herramientas, a lo largo de la pared y hacia adentro, en
/// metros.
const TOOLS: (f32, f32) = (2.4, 0.3);
/// La mitad del paso libre desde la puerta hasta el mostrador del almacén,
/// en metros.
const ALMACEN_AISLE: f32 = 1.3;

/// Si `kind` es un local que se recorre por dentro: la pulpería y el almacén
/// de ramos generales.
pub fn walks_in(kind: BuildingKind) -> bool {
    matches!(kind, BuildingKind::Pulperia | BuildingKind::GeneralStore)
}

/// El ancho del pasillo detrás del extremo del mostrador, en metros.
const COUNTER_PASSAGE: f32 = 2.0;
/// Cuánto hay entre el mostrador y los estantes, en metros (el lugar del
/// pulpero).
const BEHIND_COUNTER: f32 = 3.2;
const COUNTER_DEPTH: f32 = 0.8;
const SHELF_DEPTH: f32 = 0.45;
const BARREL: f32 = 0.6;
const TABLE: (f32, f32) = (1.6, 0.8);
const BENCH: (f32, f32) = (1.6, 0.35);
/// Entre el canto de la mesa y el banco, en metros.
const BENCH_GAP: f32 = 0.1;
/// El paso libre entre el banco de atrás de una mesa y el mostrador, en
/// metros.
const TABLE_AISLE: f32 = 0.9;
/// Dónde se sienta cada uno sobre un banco, a cada lado de su centro.
const SEAT_OFFSET: f32 = 0.4;
/// La altura de los bancos, en metros: la parte de arriba del asiento sobre el
/// piso de ladrillo.
pub const BENCH_HEIGHT: f32 = 0.45;
/// Cuánto queda el piso de ladrillo por encima del suelo del lote, en metros
/// (el render lo dibuja ahí).
pub const FLOOR_RISE: f32 = 0.05;
/// Cuánto se aleja del asiento quien se levanta, hacia atrás del banco, en
/// metros.
const STAND_BACK: f32 = 0.6;
/// Hasta dónde sale a la vereda quien se va, desde la puerta, en metros.
const SIDEWALK_OUT: f32 = 2.0;
/// Cuánto camina por la vereda quien se va, hasta perderse de vista, en
/// metros.
const WALK_AWAY: f32 = 32.0;
/// Cuántos parroquianos hay sentados al caer la tarde ([`EVENING`]), como
/// mínimo y como máximo.
const EVENING_PATRONS: (u32, u32) = (4, 8);
/// Desde qué hora termina la jornada de trabajo y llega más gente (supuesto, a
/// revisar).
pub const EVENING: f32 = 18.0;

/// A qué hora abre la pulpería: desde entonces la puerta del medio queda
/// abierta (supuesto, a revisar).
pub const OPENS_AT: f32 = 6.0;
/// A qué hora cierra: desde entonces la puerta se cierra en cuanto no queda
/// nadie adentro (supuesto, a revisar).
pub const CLOSES_AT: f32 = 22.0;

/// Si la pulpería atiende a la hora `time`: entre [`OPENS_AT`] y
/// [`CLOSES_AT`].
pub fn is_open(time: TimeOfDay) -> bool {
    (OPENS_AT..CLOSES_AT).contains(&time.hour())
}

/// La puerta abierta de la pulpería, en el medio del frente.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Door {
    /// El centro del umbral, sobre la cara de afuera de la pared.
    pub center: Vec2,
    /// Normal unitaria hacia la calle.
    pub outward: Vec2,
    pub width: f32,
    pub height: f32,
}

/// Qué es cada mueble, para que el render lo dibuje.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FurnitureKind {
    /// El mostrador, con su reja hasta el techo por encima.
    Counter,
    /// Los estantes con botellas contra la pared del fondo.
    Shelves,
    Barrel,
    Table,
    Bench,
    /// El mostrador largo del almacén, de madera, sin reja.
    ShopCounter,
    /// Los estantes del almacén, hasta el techo, con mercadería: latas,
    /// paquetes, frascos y piezas de tela.
    Goods,
    /// Una pila de bolsas de yerba, harina y azúcar sobre el piso.
    Sacks,
    /// Un caballete con monturas, riendas y lazos colgados.
    Saddlery,
    /// Un tablero en la pared con herramientas colgadas: palas, azadas,
    /// hachas y serruchos.
    Tools,
}

/// Un mueble: con qué chocan las personas y qué tan alto es.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Furniture {
    pub kind: FurnitureKind,
    pub footprint: Aabb,
    /// La altura sobre el piso, en metros (la del mostrador sin su reja).
    pub height: f32,
}

/// Un lugar para sentarse en un banco, mirando hacia su mesa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seat {
    pub position: Vec2,
    pub facing: Heading,
    /// La mesa a la que da, por índice.
    pub table: usize,
}

/// Alguien sentado a una mesa de la pulpería: en el asiento `seat` del
/// interior, con el aspecto de vecino `look`, pasando el rato con `pastime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patron {
    pub seat: usize,
    pub look: u8,
    pub pastime: Pastime,
}

/// Qué hace alguien sentado a una mesa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Pastime {
    /// Quieto, con las manos en el regazo.
    #[default]
    Rest,
    /// Juega al truco, con su mano de naipes: en la mesa de los naipes
    /// ([`Interior::card_table`]).
    Cards,
    /// Fuma un cigarrillo armado.
    Smoke,
    /// Toma de un vaso.
    Drink,
    /// Conversa, con las manos.
    Chat,
}

/// El interior de una pulpería.
#[derive(Debug, Clone, PartialEq)]
pub struct Interior {
    pub door: Door,
    /// Las paredes, con el hueco de la puerta entre los dos tramos del frente.
    pub walls: Vec<Aabb>,
    pub furniture: Vec<Furniture>,
    pub seats: Vec<Seat>,
    /// La mesa donde se juega al truco, con el mazo y los naipes sobre ella,
    /// por índice; ninguna en un almacén.
    pub card_table: Option<usize>,
    /// El piso entre las caras de adentro de las paredes.
    pub floor: Aabb,
    /// La altura del cielorraso sobre el piso, en metros.
    pub ceiling: f32,
    /// Dónde atiende el pulpero o el almacenero: detrás del mostrador, de
    /// cara a la calle.
    pub keeper: Keeper,
}

/// La mesa de los naipes de la pulpería `store` con asientos `seats`: una de
/// sus mesas, con semilla en su huella, para que el mismo local juegue
/// siempre en la misma.
fn card_table(store: &Building, seats: &[Seat]) -> Option<usize> {
    let tables = seats.iter().map(|s| s.table).max()? + 1;
    let c = store.footprint.center();
    let mut rng = Rng::new(c.x.to_bits() ^ c.y.to_bits().rotate_left(16), CARD_TABLE_SALT);
    Some(rng.next() as usize % tables)
}

/// La sal de la semilla de [`card_table`].
const CARD_TABLE_SALT: u32 = 7;

/// Qué hace, sin naipes, alguien sentado a una mesa: de cada tres, uno fuma,
/// uno toma y uno conversa.
const PASTIMES: [Pastime; 3] = [Pastime::Smoke, Pastime::Drink, Pastime::Chat];

/// La sal de la semilla de qué hace cada parroquiano.
const PASTIME_SALT: u32 = 3;

/// Dónde está parado quien atiende un local, y hacia dónde mira.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keeper {
    pub position: Vec2,
    pub facing: Heading,
}

/// Cuánto detrás del mostrador se para quien atiende, desde su canto de
/// adentro, en metros.
const KEEPER_BEHIND: f32 = 0.55;

/// Quien atiende en `(u, v)` del marco `frame`, de cara a la calle.
fn keeper(frame: &Frame, u: f32, v: f32) -> Keeper {
    let n = frame.n;
    Keeper {
        position: frame.point(u, v),
        facing: Heading::from_radians(n.y.atan2(n.x)),
    }
}

impl Interior {
    /// El interior de `store`, dentro de su huella: el de una pulpería si lo
    /// es, el de un almacén de ramos generales si no ([`walks_in`]).
    pub fn of(store: &Building) -> Self {
        if store.kind == BuildingKind::Pulperia {
            Self::pulperia(store)
        } else {
            Self::almacen(store)
        }
    }

    /// La pulpería: el mostrador con su reja, la estantería del fondo, dos
    /// barriles y tres mesas con sus bancos.
    fn pulperia(store: &Building) -> Self {
        let Shell {
            frame,
            inner,
            front,
            back,
            walls,
            door,
        } = Shell::of(store);
        let mut furniture = Vec::new();
        let mut put = |kind, (u0, u1, v0, v1): (f32, f32, f32, f32), height| {
            furniture.push(Furniture {
                kind,
                footprint: frame.rect(u0, u1, v0, v1),
                height,
            });
        };
        put(
            FurnitureKind::Shelves,
            (
                -inner + SHELF_DEPTH,
                inner - SHELF_DEPTH,
                back - SHELF_DEPTH,
                back,
            ),
            2.4,
        );
        let counter = back - BEHIND_COUNTER - SHELF_DEPTH - COUNTER_DEPTH;
        put(
            FurnitureKind::Counter,
            (
                -inner,
                inner - COUNTER_PASSAGE,
                counter,
                counter + COUNTER_DEPTH,
            ),
            1.05,
        );
        for i in 0..2 {
            let u = -inner + 0.05 + BARREL / 2.0 + i as f32 * (BARREL + 0.1);
            let v = counter - 0.15 - BARREL / 2.0;
            let r = BARREL / 2.0;
            put(FurnitureKind::Barrel, (u - r, u + r, v - r, v + r), 0.9);
        }
        // Dos mesas adelante, a los costados del paso desde la puerta, y una
        // más atrás a la derecha, frente al paso del mostrador.
        let side = inner - 3.15;
        let tables = [
            (-side, front + 2.65),
            (side, front + 2.65),
            (side, front + 6.05),
        ];
        // Una mesa va solo si queda paso entre sus bancos y el mostrador: en una
        // sala poco profunda, la de atrás no entra.
        let reach = TABLE.1 / 2.0 + BENCH_GAP + BENCH.1 + TABLE_AISLE;
        let tables = tables.into_iter().filter(|&(_, v)| v + reach <= counter);
        let mut seats = Vec::new();
        for (table, (u, v)) in tables.enumerate() {
            let (tu, tv) = (TABLE.0 / 2.0, TABLE.1 / 2.0);
            put(FurnitureKind::Table, (u - tu, u + tu, v - tv, v + tv), 0.75);
            for sign in [-1.0, 1.0] {
                let bv = v + sign * (tv + BENCH_GAP + BENCH.1 / 2.0);
                let (bu, bh) = (BENCH.0 / 2.0, BENCH.1 / 2.0);
                put(
                    FurnitureKind::Bench,
                    (u - bu, u + bu, bv - bh, bv + bh),
                    BENCH_HEIGHT,
                );
                // Mirando hacia la mesa: hacia adentro desde el banco de
                // adelante, hacia la calle desde el de atrás.
                let toward = frame.n * sign;
                let facing = Heading::from_radians(toward.y.atan2(toward.x));
                for du in [-SEAT_OFFSET, SEAT_OFFSET] {
                    seats.push(Seat {
                        position: frame.point(u + du, bv),
                        facing,
                        table,
                    });
                }
            }
        }

        Self {
            door,
            walls,
            furniture,
            card_table: card_table(store, &seats),
            seats,
            floor: frame.rect(-inner, inner, front, back),
            ceiling: store.height,
            // Junto a la ventanilla de la reja, cerca del paso al fondo.
            keeper: keeper(
                &frame,
                inner - COUNTER_PASSAGE - 1.2,
                counter + COUNTER_DEPTH + KEEPER_BEHIND,
            ),
        }
    }

    /// El almacén de ramos generales: un mostrador largo de lado a lado, con
    /// el paso al fondo en un extremo; estantes hasta el techo contra el
    /// fondo y la pared de la izquierda; bolsas y barriles frente al
    /// mostrador; las monturas contra la pared de la derecha y el tablero de
    /// las herramientas contra el frente. Sin mesas: ahí se compra, no se queda.
    fn almacen(store: &Building) -> Self {
        let Shell {
            frame,
            inner,
            front,
            back,
            walls,
            door,
        } = Shell::of(store);
        let mut furniture = Vec::new();
        let mut put = |kind, (u0, u1, v0, v1): (f32, f32, f32, f32), height| {
            furniture.push(Furniture {
                kind,
                footprint: frame.rect(u0, u1, v0, v1),
                height,
            });
        };
        let tall = store.height - GOODS_BELOW_CEILING;
        put(
            FurnitureKind::Goods,
            (-inner, inner, back - GOODS_DEPTH, back),
            tall,
        );
        let counter = back - GOODS_DEPTH - ALMACEN_BEHIND - COUNTER_DEPTH;
        put(
            FurnitureKind::ShopCounter,
            (
                -inner,
                inner - COUNTER_PASSAGE,
                counter,
                counter + COUNTER_DEPTH,
            ),
            1.0,
        );
        // Lo que se despacha del piso queda a un metro del mostrador, para
        // acercarse a atender.
        let shop_floor = counter - 1.0;
        // La pared de la izquierda: estantes desde un metro de la puerta
        // hasta donde empiezan las bolsas.
        let sacks_v = shop_floor - SACKS.1;
        put(
            FurnitureKind::Goods,
            (-inner, -inner + GOODS_DEPTH, front + 1.0, sacks_v - 0.6),
            tall,
        );
        // Bolsas a la izquierda del paso y barriles a la derecha, frente al
        // mostrador.
        let left = -inner + GOODS_DEPTH + 0.3;
        let mut u = left;
        while u + SACKS.0 <= -ALMACEN_AISLE {
            put(
                FurnitureKind::Sacks,
                (u, u + SACKS.0, sacks_v, shop_floor),
                0.7,
            );
            u += SACKS.0 + 0.2;
        }
        let r = BARREL / 2.0;
        let mut u = ALMACEN_AISLE;
        while u + BARREL <= inner - COUNTER_PASSAGE {
            let v = shop_floor - r;
            put(FurnitureKind::Barrel, (u, u + BARREL, v - r, v + r), 0.9);
            u += BARREL + 0.1;
        }
        // Las monturas contra la pared de la derecha, cerca de la puerta, si
        // caben antes de los barriles; el tablero de las herramientas contra
        // el frente, a la derecha de la puerta.
        let saddles = front + 1.0;
        if saddles + SADDLERY.0 <= shop_floor - BARREL - 0.8 {
            put(
                FurnitureKind::Saddlery,
                (inner - SADDLERY.1, inner, saddles, saddles + SADDLERY.0),
                1.1,
            );
        }
        let tools = ALMACEN_AISLE + 0.2;
        let end = (tools + TOOLS.0).min(inner - SADDLERY.1 - 0.2);
        if end - tools >= 1.0 {
            put(
                FurnitureKind::Tools,
                (tools, end, front, front + TOOLS.1),
                2.2,
            );
        }

        Self {
            door,
            walls,
            furniture,
            seats: Vec::new(),
            floor: frame.rect(-inner, inner, front, back),
            ceiling: store.height,
            // En el medio del mostrador, frente a la puerta.
            card_table: None,
            keeper: keeper(&frame, 0.0, counter + COUNTER_DEPTH + KEEPER_BEHIND),
        }
    }

    /// El hueco de la puerta en la pared del frente, de la cara de afuera a la
    /// de adentro.
    pub fn doorway(&self) -> Aabb {
        let d = self.door;
        let across = Vec2::new(-d.outward.y, d.outward.x) * (d.width / 2.0);
        let (a, z) = (d.center + across, d.center - across - d.outward * WALL);
        Aabb::new(
            Vec2::new(a.x.min(z.x), a.y.min(z.y)),
            Vec2::new(a.x.max(z.x), a.y.max(z.y)),
        )
    }

    /// Si una persona parada en `at` está adentro o en el umbral: con ella ahí
    /// la puerta no se cierra, para no dejarla encerrada.
    pub fn holds(&self, at: Vec2) -> bool {
        self.floor.contains_point(at)
            || self
                .doorway()
                .expanded(Rider::PERSON_RADIUS)
                .contains_point(at)
    }

    /// Los parroquianos sentados a las mesas de una pulpería con semilla
    /// `seed` a la hora `time`, cada uno en un asiento distinto
    /// ([`Self::seats`]) con su aspecto de vecino, y ninguno en los asientos
    /// `reserved`:
    ///
    /// - de día, dos frente a frente en una misma mesa, que no es la de un
    ///   asiento reservado;
    /// - desde [`EVENING`], terminada la jornada, entre cuatro y ocho;
    /// - cerrada ([`is_open`]), nadie.
    pub fn patrons(&self, seed: u32, time: TimeOfDay, reserved: &[usize]) -> Vec<Patron> {
        if !is_open(time) {
            return Vec::new();
        }
        let mut rng = Rng::new(seed, 0);
        let mut doing = Rng::new(seed, PASTIME_SALT);
        let cards = self.card_table;
        let mut patron = |seat: usize| Patron {
            seat,
            look: (rng.next() >> 24) as u8,
            pastime: if Some(self.seats[seat].table) == cards {
                Pastime::Cards
            } else {
                PASTIMES[doing.next() as usize % PASTIMES.len()]
            },
        };
        if time.hour() < EVENING {
            let tables: Vec<usize> = self
                .seats
                .iter()
                .map(|s| s.table)
                .filter(|t| !reserved.iter().any(|&r| self.seats[r].table == *t))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if tables.is_empty() {
                return Vec::new();
            }
            let mut pick = Rng::new(seed, 1);
            let table = tables[pick.next() as usize % tables.len()];
            let at: Vec<usize> = (0..self.seats.len())
                .filter(|&s| self.seats[s].table == table)
                .collect();
            let a = at[pick.next() as usize % at.len()];
            // Enfrente: en el otro banco, el más cercano (no el de la diagonal).
            let (seat, there) = (self.seats[a], |b: &usize| self.seats[*b]);
            let across = at
                .iter()
                .copied()
                .filter(|b| there(b).facing != seat.facing)
                .min_by(|b, c| {
                    let d = |x: &usize| there(x).position.distance(seat.position);
                    d(b).total_cmp(&d(c))
                });
            return across.map_or_else(Vec::new, |b| vec![patron(a), patron(b)]);
        }
        let mut free: Vec<usize> = (0..self.seats.len())
            .filter(|s| !reserved.contains(s))
            .collect();
        let mut pick = Rng::new(seed, 2);
        let count = EVENING_PATRONS.0 + pick.next() % (EVENING_PATRONS.1 - EVENING_PATRONS.0 + 1);
        let mut out = Vec::new();
        for _ in 0..count {
            if free.is_empty() {
                break;
            }
            let seat = free.swap_remove(pick.next() as usize % free.len());
            out.push(patron(seat));
        }
        out
    }

    /// El asiento de una de las mesas de adelante (las más cercanas a la
    /// puerta) que mira hacia la puerta, el más cercano al paso desde ella:
    /// quien entra ve de frente a quien está sentado ahí.
    pub fn front_seat(&self) -> usize {
        let door = self.door;
        let depth = |p: Vec2| (door.center - p).dot(door.outward);
        let across = |p: Vec2| (p - door.center).dot(Vec2::new(-door.outward.y, door.outward.x));
        (0..self.seats.len())
            .filter(|&s| self.seats[s].facing.forward().dot(door.outward) > 0.0)
            .min_by(|&a, &b| {
                let key = |s: usize| {
                    let seat = self.seats[s];
                    (depth(seat.position), across(seat.position).abs())
                };
                let ((da, xa), (db, xb)) = (key(a), key(b));
                // Las mesas de adelante están a la misma profundidad.
                if (da - db).abs() > 0.5 {
                    da.total_cmp(&db)
                } else {
                    xa.total_cmp(&xb)
                }
            })
            .expect("every pulpería has seats facing the door")
    }

    /// El camino de quien se levanta del asiento `seat` y se va: un paso atrás
    /// del banco, al paso que viene de la puerta, por la puerta hasta la
    /// vereda, y a lo largo de ella hasta perderse de vista. Todo por piso
    /// libre, salvo el primer paso, que pasa por encima del banco.
    pub fn way_out(&self, seat: usize) -> Vec<Vec2> {
        let door = self.door;
        let along = Vec2::new(-door.outward.y, door.outward.x);
        let s = self.seats[seat];
        let up = s.position - s.facing.forward() * STAND_BACK;
        let depth = (door.center - up).dot(door.outward);
        let aisle = door.center - door.outward * depth;
        let sidewalk = door.center + door.outward * SIDEWALK_OUT;
        vec![
            s.position,
            up,
            aisle,
            sidewalk,
            sidewalk + along * WALK_AWAY,
        ]
    }

    /// Con qué chocan las personas adentro: las paredes y los muebles.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.walls
            .iter()
            .copied()
            .chain(self.furniture.iter().map(|f| f.footprint))
    }
}

/// Lo que comparten todas las salas: las paredes con el hueco de la puerta del
/// medio del frente, en el marco del edificio.
struct Shell {
    frame: Frame,
    /// La cara de adentro de las paredes de los costados, a cada lado del eje.
    inner: f32,
    /// La cara de adentro de la pared del frente.
    front: f32,
    /// La cara de adentro de la pared del fondo.
    back: f32,
    walls: Vec<Aabb>,
    door: Door,
}

impl Shell {
    fn of(store: &Building) -> Self {
        let frame = Frame::of(store);
        let (width, depth) = (frame.width, frame.depth);
        let outer = width / 2.0 - PROUD;
        let inner = outer - WALL;
        let (front, back) = (PROUD + WALL, depth - PROUD - WALL);
        let jamb = DOOR_WIDTH / 2.0;
        let walls = vec![
            frame.rect(-outer, -jamb, PROUD, front),
            frame.rect(jamb, outer, PROUD, front),
            frame.rect(-outer, outer, back, depth - PROUD),
            frame.rect(-outer, -inner, front, back),
            frame.rect(inner, outer, front, back),
        ];
        let door = Door {
            center: frame.point(0.0, PROUD),
            outward: frame.n,
            width: DOOR_WIDTH,
            height: (store.height - DOOR_BELOW_EAVES).min(DOOR_TOP),
        };
        Self {
            frame,
            inner,
            front,
            back,
            walls,
            door,
        }
    }
}

/// El marco propio de un edificio: `u` a lo largo del frente, `v` hacia
/// adentro desde el borde de la calle.
struct Frame {
    facade: Vec2,
    n: Vec2,
    along: Vec2,
    width: f32,
    depth: f32,
}

impl Frame {
    fn of(b: &Building) -> Self {
        let n = side_normal(b.front);
        let along = Vec2::new(-n.y, n.x);
        let size = b.footprint.size();
        let depth = size.dot(n).abs();
        Self {
            facade: b.footprint.center() + n * (depth / 2.0),
            n,
            along,
            width: size.dot(along).abs(),
            depth,
        }
    }

    fn point(&self, u: f32, v: f32) -> Vec2 {
        self.facade + self.along * u - self.n * v
    }

    /// El rectángulo `[u0, u1] x [v0, v1]` sobre el suelo (los frentes siempre
    /// miran a un punto cardinal, así que sigue alineado con los ejes).
    fn rect(&self, u0: f32, u1: f32, v0: f32, v1: f32) -> Aabb {
        let (a, z) = (self.point(u0, v0), self.point(u1, v1));
        Aabb::new(
            Vec2::new(a.x.min(z.x), a.y.min(z.y)),
            Vec2::new(a.x.max(z.x), a.y.max(z.y)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::Circle;
    use crate::places::StoreStyle;
    use crate::town::{BuildingKind, TownConfig, TownLayout};
    use crate::world::World;
    use crate::{Rider, Terrain};

    fn castex() -> TownLayout {
        TownLayout::generate(&TownConfig::castex()).unwrap()
    }

    fn store(t: &TownLayout) -> &Building {
        t.buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Pulperia)
            .unwrap()
    }

    #[test]
    fn a_town_has_a_pulperia_with_an_interior_and_an_almacen_apart() {
        let t = castex();
        let (b, inside) = t.pulperia().expect("Castex has a pulpería");
        assert_eq!(b, store(&t));
        assert_eq!(StoreStyle::of(b.kind), Some(StoreStyle::Pulperia));
        assert_eq!(inside, Interior::of(b));
        // Su almacén de ramos generales es otro edificio.
        let almacen = t
            .buildings
            .iter()
            .find(|a| a.kind == BuildingKind::GeneralStore)
            .expect("Castex has an almacén");
        assert_ne!(almacen.footprint, b.footprint);
        assert_eq!(StoreStyle::of(almacen.kind), Some(StoreStyle::GeneralStore));
        // Sin pulpería ni almacén, nada se recorre por dentro.
        let mut none = t.clone();
        none.buildings.retain(|a| a.kind != BuildingKind::Pulperia);
        assert_eq!(none.pulperia(), None);
        assert!(
            none.colliders() != none.obstacles(),
            "the almacén still walks in"
        );
        none.buildings.retain(|a| !walks_in(a.kind));
        assert_eq!(none.rooms().count(), 0);
        assert_eq!(none.colliders(), none.obstacles());
    }

    #[test]
    fn people_walk_in_through_the_door_but_not_through_the_walls_or_a_horse() {
        let t = castex();
        let world = World::with_town(Terrain::default(), &t);
        let person = Circle {
            radius: Rider::PERSON_RADIUS,
        };
        let settle = |collider: Circle, p: Vec2| world.resolve(collider, p, |_| {});
        // La pulpería y el almacén.
        assert_eq!(t.rooms().count(), 2);
        for (_, b, inside) in t.rooms() {
            let door = inside.door;
            // En el umbral y adentro, en el paso de la puerta: nada lo empuja.
            for v in [0.0, 1.0, 4.0] {
                let p = door.center - door.outward * v;
                assert!(b.footprint.contains_point(p));
                assert_eq!(settle(person, p), p, "{:?}: {v} m in", b.kind);
            }
            // Contra el frente, a un costado de la puerta: queda afuera.
            let wall = door.center + Vec2::new(-door.outward.y, door.outward.x) * 3.0;
            let pushed = settle(person, wall - door.outward * 0.2);
            assert!((pushed - wall).dot(door.outward) >= Rider::PERSON_RADIUS - 1e-3);
            // Un caballo no entra por la puerta.
            let horse = Circle {
                radius: Rider::COLLIDER_RADIUS,
            };
            let at = door.center - door.outward * 0.3;
            assert_ne!(settle(horse, at), at);
        }
    }

    #[test]
    fn walls_and_furniture_stay_inside_the_footprint_and_leave_the_door_clear() {
        for t in crate::town::tests::sample_towns() {
            for (_, b, inside) in t.rooms() {
                assert!(inside.door.height > 2.0 && inside.door.height <= b.height);
                let doorway = Aabb::from_center_size(
                    inside.door.center - inside.door.outward * 2.0,
                    Vec2::new(DOOR_WIDTH - 0.01, DOOR_WIDTH - 0.01),
                );
                for o in inside.obstacles() {
                    assert!(b.footprint.contains(&o), "{}: {o:?}", t.name);
                    assert!(!o.intersects(&doorway), "{}: {o:?}", t.name);
                }
                // El hueco calza entre los dos tramos del frente.
                let doorway = inside.doorway();
                assert!(b.footprint.contains(&doorway), "{}", t.name);
                let size = doorway.size();
                let (wide, deep) = (size.x.max(size.y), size.x.min(size.y));
                assert!((wide - DOOR_WIDTH).abs() < 1e-4, "{}: {size:?}", t.name);
                assert!((deep - WALL).abs() < 1e-4, "{}: {size:?}", t.name);
                let touching = inside
                    .walls
                    .iter()
                    .filter(|w| w.expanded(1e-3).intersects(&doorway))
                    .count();
                assert_eq!(touching, 2, "{}", t.name);
                for f in &inside.furniture {
                    assert!(inside.floor.contains(&f.footprint), "{}: {f:?}", t.name);
                }
                for s in &inside.seats {
                    // Junto a una mesa, mirándola.
                    let table = inside
                        .furniture
                        .iter()
                        .filter(|f| f.kind == FurnitureKind::Table)
                        .map(|f| f.footprint.center())
                        .min_by(|a, b| a.distance(s.position).total_cmp(&b.distance(s.position)))
                        .unwrap();
                    let to = table - s.position;
                    let to = to * (1.0 / to.length());
                    assert!(s.facing.forward().dot(to) > 0.7, "{}: {s:?}", t.name);
                }
                // Tres mesas de cuatro, o dos si la sala es poco profunda; el
                // almacén, ninguna.
                let seats: &[usize] = if b.kind == BuildingKind::Pulperia {
                    &[8, 12]
                } else {
                    &[0]
                };
                assert!(seats.contains(&inside.seats.len()), "{}", t.name);
                // Ningún mueble se encima con otro.
                for (i, f) in inside.furniture.iter().enumerate() {
                    for g in &inside.furniture[i + 1..] {
                        assert!(
                            !f.footprint.expanded(-1e-3).intersects(&g.footprint),
                            "{}: {f:?} / {g:?}",
                            t.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn whoever_keeps_the_room_stands_behind_the_counter_on_free_floor_facing_the_door() {
        let mut rooms = 0;
        for t in crate::town::tests::sample_towns() {
            for (_, b, inside) in t.rooms() {
                rooms += 1;
                let k = inside.keeper;
                let door = inside.door;
                let depth = |p: Vec2| (door.center - p).dot(door.outward);
                assert!(inside.floor.contains_point(k.position), "{}", t.name);
                let body = Aabb::from_center_size(
                    k.position,
                    Vec2::new(2.0 * Rider::PERSON_RADIUS, 2.0 * Rider::PERSON_RADIUS),
                );
                for o in inside.obstacles() {
                    assert!(!o.intersects(&body), "{} {:?}: {o:?}", t.name, b.kind);
                }
                // Detrás del mostrador: más adentro que él, y frente a él.
                let counter = inside
                    .furniture
                    .iter()
                    .find(|f| matches!(f.kind, FurnitureKind::Counter | FurnitureKind::ShopCounter))
                    .unwrap()
                    .footprint;
                assert!(depth(k.position) > depth(counter.center()) + COUNTER_DEPTH / 2.0);
                let across = Vec2::new(-door.outward.y, door.outward.x);
                let span = |a: Aabb| {
                    let (p, q) = (a.min.dot(across), a.max.dot(across));
                    (p.min(q), p.max(q))
                };
                let (lo, hi) = span(counter);
                let at = k.position.dot(across);
                assert!(at > lo && at < hi, "{}: along the counter", t.name);
                assert!(k.facing.forward().dot(door.outward) > 0.99, "{}", t.name);
            }
        }
        assert!(rooms >= 4, "{rooms}");
    }

    #[test]
    fn an_almacen_has_a_long_counter_goods_to_the_ceiling_and_a_clear_way_to_it() {
        let mut almacenes = 0;
        for t in crate::town::tests::sample_towns() {
            for (_, b, inside) in t.rooms() {
                if b.kind != BuildingKind::GeneralStore {
                    continue;
                }
                almacenes += 1;
                let count = |k| inside.furniture.iter().filter(|f| f.kind == k).count();
                for k in [
                    FurnitureKind::ShopCounter,
                    FurnitureKind::Goods,
                    FurnitureKind::Sacks,
                    FurnitureKind::Barrel,
                    FurnitureKind::Saddlery,
                    FurnitureKind::Tools,
                ] {
                    assert!(count(k) > 0, "{}: no {k:?}", t.name);
                }
                for k in [
                    FurnitureKind::Table,
                    FurnitureKind::Bench,
                    FurnitureKind::Counter,
                ] {
                    assert_eq!(count(k), 0, "{}: {k:?}", t.name);
                }
                let along = |a: Aabb| {
                    let n = inside.door.outward;
                    a.size().dot(Vec2::new(n.y, n.x)).abs()
                };
                let counter = inside
                    .furniture
                    .iter()
                    .find(|f| f.kind == FurnitureKind::ShopCounter)
                    .unwrap();
                assert!(
                    along(counter.footprint) >= 0.6 * along(inside.floor),
                    "{}: a long counter",
                    t.name
                );
                for g in inside
                    .furniture
                    .iter()
                    .filter(|f| f.kind == FurnitureKind::Goods)
                {
                    assert!(
                        g.height >= inside.ceiling - GOODS_BELOW_CEILING - 1e-4,
                        "{}: up to the ceiling",
                        t.name
                    );
                }
                // Ningún mueble se encima con otro.
                for (i, f) in inside.furniture.iter().enumerate() {
                    for g in &inside.furniture[i + 1..] {
                        assert!(
                            !f.footprint.expanded(-1e-3).intersects(&g.footprint),
                            "{}: {f:?} / {g:?}",
                            t.name
                        );
                    }
                }
                // De la puerta al mostrador, el paso queda libre.
                let door = inside.door;
                let to_counter = (door.center - counter.footprint.center()).dot(door.outward)
                    - COUNTER_DEPTH / 2.0;
                for step in 1..((to_counter - 0.1) as i32) {
                    let p = door.center - door.outward * step as f32;
                    let path = Aabb::from_center_size(p, Vec2::new(0.9, 0.9));
                    for o in inside.obstacles() {
                        assert!(!o.intersects(&path), "{}: {p:?} blocked by {o:?}", t.name);
                    }
                }
            }
        }
        assert!(almacenes >= 2, "{almacenes}");
    }

    #[test]
    fn the_front_seat_faces_the_door_and_its_way_out_crosses_free_floor_to_the_street() {
        for t in crate::town::tests::sample_towns() {
            let Some((_, inside)) = t.pulperia() else {
                continue;
            };
            let door = inside.door;
            let depth = |p: Vec2| (door.center - p).dot(door.outward);
            let seat = inside.seats[inside.front_seat()];
            // En la mesa más cercana a la puerta, mirando hacia ella.
            let tables: Vec<Vec2> = inside
                .furniture
                .iter()
                .filter(|f| f.kind == FurnitureKind::Table)
                .map(|f| f.footprint.center())
                .collect();
            let nearest = tables.iter().map(|&c| depth(c)).fold(f32::MAX, f32::min);
            assert!(
                (depth(tables[seat.table]) - nearest).abs() < 1e-3,
                "{}",
                t.name
            );
            assert!(seat.facing.forward().dot(door.outward) > 0.99, "{}", t.name);

            let way = inside.way_out(inside.front_seat());
            assert_eq!(way[0], seat.position, "{}", t.name);
            let end = *way.last().unwrap();
            assert!(end.distance(door.center) > 25.0, "{}: {end:?}", t.name);
            // Pasado el banco del que se levanta, nada en el camino: ni muebles
            // ni paredes ni nada de la calle; y sale por el hueco de la puerta.
            let colliders = t.colliders();
            let mut through_door = false;
            for pair in way[1..].windows(2) {
                let steps = (pair[0].distance(pair[1]) / 0.1).ceil() as usize;
                for k in 0..=steps {
                    let p = pair[0] + (pair[1] - pair[0]) * (k as f32 / steps as f32);
                    through_door |= inside.doorway().contains_point(p);
                    for o in &colliders {
                        assert!(
                            o.closest_point(p).distance(p) >= Rider::PERSON_RADIUS,
                            "{}: {p:?} touches {o:?}",
                            t.name
                        );
                    }
                }
            }
            assert!(through_door, "{}", t.name);
        }
    }

    #[test]
    fn it_attends_from_six_in_the_morning_to_ten_at_night() {
        let open = |h, m| is_open(TimeOfDay::hm(h, m));
        assert!(!open(5, 59));
        assert!(open(6, 0));
        assert!(open(21, 59));
        assert!(!open(22, 0));
        assert!(!open(0, 0));
    }

    #[test]
    fn two_share_a_table_by_day_more_come_after_work_and_none_while_shut() {
        let t = castex();
        let (_, inside) = t.pulperia().unwrap();
        let table = |s: usize| inside.seats[s].table;
        for seed in 0..40 {
            // De día, dos frente a frente en la misma mesa, lejos de un asiento
            // reservado.
            let day = inside.patrons(seed, TimeOfDay::hm(11, 0), &[0]);
            assert_eq!(day.len(), 2, "seed {seed}");
            let (a, b) = (day[0].seat, day[1].seat);
            assert_eq!(table(a), table(b), "seed {seed}");
            assert_ne!(table(a), table(0), "seed {seed}: the reserved table");
            let turn = inside.seats[a].facing.radians() - inside.seats[b].facing.radians();
            assert!(
                (turn.abs() - std::f32::consts::PI).abs() < 1e-4,
                "face to face"
            );
            // Después del trabajo, más gente, nunca en el asiento reservado.
            let evening = inside.patrons(seed, TimeOfDay::hm(19, 30), &[0]);
            assert!(
                (4..=8).contains(&evening.len()),
                "seed {seed}: {}",
                evening.len()
            );
            let seats: std::collections::BTreeSet<_> = evening.iter().map(|p| p.seat).collect();
            assert_eq!(seats.len(), evening.len(), "one per seat");
            assert!(!seats.contains(&0));
            // Cerrada, nadie.
            assert!(inside.patrons(seed, TimeOfDay::hm(23, 0), &[]).is_empty());
            assert!(inside.patrons(seed, TimeOfDay::hm(4, 0), &[]).is_empty());
        }
        assert_eq!(inside.patrons(3, TimeOfDay::hm(17, 59), &[]).len(), 2);
        assert!(inside.patrons(3, TimeOfDay::hm(18, 0), &[]).len() >= 4);
    }

    #[test]
    fn whoever_sits_at_the_truco_table_plays_and_the_rest_smoke_drink_or_chat() {
        let t = castex();
        let (_, inside) = t.pulperia().unwrap();
        let tables = inside.seats.iter().map(|s| s.table).max().unwrap() + 1;
        let cards = inside.card_table.expect("a pulpería has a truco table");
        assert!(cards < tables);
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..60 {
            for time in [TimeOfDay::hm(11, 0), TimeOfDay::hm(20, 0)] {
                for p in inside.patrons(seed, time, &[]) {
                    let playing = inside.seats[p.seat].table == cards;
                    assert_eq!(p.pastime == Pastime::Cards, playing, "seed {seed}: {p:?}");
                    seen.insert(format!("{:?}", p.pastime));
                }
            }
        }
        assert_eq!(seen.len(), 4, "cards, smoke, drink and chat all come: {seen:?}");
        // Nadie juega en el almacén: no tiene mesas.
        let almacen = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .expect("Castex has an almacén");
        assert_eq!(Interior::of(almacen).card_table, None);
    }

    #[test]
    fn someone_inside_or_in_the_doorway_is_held_and_someone_outside_is_not() {
        let t = castex();
        let (_, inside) = t.pulperia().unwrap();
        let door = inside.door;
        let at = |v: f32| door.center - door.outward * v;
        assert!(inside.holds(at(5.0)), "inside");
        assert!(inside.holds(at(WALL / 2.0)), "in the doorway");
        // Afuera, pegado al umbral: el cuerpo todavía toca el hueco.
        assert!(inside.holds(at(-0.2)));
        assert!(!inside.holds(at(-1.0)), "on the sidewalk");
    }

    #[test]
    fn placement_keeps_the_whole_footprint_solid() {
        let t = castex();
        let colliders = t.colliders();
        let mut swapped = 0;
        for (_, b, inside) in t.rooms() {
            assert!(t.obstacles().contains(&b.collider()));
            assert!(!colliders.contains(&b.collider()));
            assert!(inside.obstacles().all(|o| colliders.contains(&o)));
            swapped += inside.obstacles().count();
        }
        let rooms = t.rooms().count();
        assert_eq!(rooms, 2, "the pulpería and the almacén");
        assert_eq!(colliders.len(), t.obstacles().len() - rooms + swapped);
    }
}
