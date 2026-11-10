//! El interior de una pulpería, el almacén de un pueblo chico
//! ([`crate::places::StoreStyle::Pulperia`]): se entra caminando por la puerta
//! del medio del frente, y adentro hay un mostrador, estantes, barriles, mesas
//! y bancos.
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

use crate::places::side_normal;
use crate::town::Building;
use crate::{Aabb, Heading, Vec2};

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
/// Dónde se sienta cada uno sobre un banco, a cada lado de su centro.
const SEAT_OFFSET: f32 = 0.4;

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
}

/// El interior de una pulpería.
#[derive(Debug, Clone, PartialEq)]
pub struct Interior {
    pub door: Door,
    /// Las paredes, con el hueco de la puerta entre los dos tramos del frente.
    pub walls: Vec<Aabb>,
    pub furniture: Vec<Furniture>,
    pub seats: Vec<Seat>,
    /// El piso entre las caras de adentro de las paredes.
    pub floor: Aabb,
    /// La altura del cielorraso sobre el piso, en metros.
    pub ceiling: f32,
}

impl Interior {
    /// El interior de `store`, dentro de su huella.
    pub fn of(store: &Building) -> Self {
        let frame = Frame::of(store);
        let (width, depth) = (frame.width, frame.depth);
        let half = width / 2.0;
        let outer = half - PROUD;
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
        let mut seats = Vec::new();
        for (u, v) in tables {
            let (tu, tv) = (TABLE.0 / 2.0, TABLE.1 / 2.0);
            put(FurnitureKind::Table, (u - tu, u + tu, v - tv, v + tv), 0.75);
            for sign in [-1.0, 1.0] {
                let bv = v + sign * (tv + BENCH_GAP + BENCH.1 / 2.0);
                let (bu, bh) = (BENCH.0 / 2.0, BENCH.1 / 2.0);
                put(
                    FurnitureKind::Bench,
                    (u - bu, u + bu, bv - bh, bv + bh),
                    0.45,
                );
                // Mirando hacia la mesa: hacia adentro desde el banco de
                // adelante, hacia la calle desde el de atrás.
                let toward = frame.n * sign;
                let facing = Heading::from_radians(toward.y.atan2(toward.x));
                for du in [-SEAT_OFFSET, SEAT_OFFSET] {
                    seats.push(Seat {
                        position: frame.point(u + du, bv),
                        facing,
                    });
                }
            }
        }

        Self {
            door,
            walls,
            furniture,
            seats,
            floor: frame.rect(-inner, inner, front, back),
            ceiling: store.height,
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

    /// Con qué chocan las personas adentro: las paredes y los muebles.
    pub fn obstacles(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.walls
            .iter()
            .copied()
            .chain(self.furniture.iter().map(|f| f.footprint))
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
    use crate::town::{BuildingKind, TownConfig, TownLayout, TownSize};
    use crate::world::World;
    use crate::{Rider, Terrain};

    fn castex() -> TownLayout {
        TownLayout::generate(&TownConfig::castex()).unwrap()
    }

    fn store(t: &TownLayout) -> &Building {
        t.buildings
            .iter()
            .find(|b| b.kind == BuildingKind::GeneralStore)
            .unwrap()
    }

    #[test]
    fn only_a_small_towns_store_is_a_pulperia_with_an_interior() {
        let t = castex();
        assert_eq!(StoreStyle::of(t.size), StoreStyle::Pulperia);
        let (b, inside) = t.pulperia().expect("Castex has a pulpería");
        assert_eq!(b, store(&t));
        assert_eq!(inside, Interior::of(b));
        let mut big = t.clone();
        big.size = TownSize::Big;
        assert_eq!(big.pulperia(), None);
        assert_eq!(big.colliders(), big.obstacles());
    }

    #[test]
    fn people_walk_in_through_the_door_but_not_through_the_walls_or_a_horse() {
        let t = castex();
        let (b, inside) = t.pulperia().unwrap();
        let world = World::with_town(Terrain::default(), &t);
        let person = Circle {
            radius: Rider::PERSON_RADIUS,
        };
        let settle = |collider: Circle, p: Vec2| world.resolve(collider, p, |_| {});
        let door = inside.door;
        // En el umbral y adentro, en el paso de la puerta: nada lo empuja.
        for v in [0.0, 1.0, 4.0] {
            let p = door.center - door.outward * v;
            assert!(b.footprint.contains_point(p));
            assert_eq!(settle(person, p), p, "{v} m in");
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

    #[test]
    fn walls_and_furniture_stay_inside_the_footprint_and_leave_the_door_clear() {
        for t in crate::town::tests::sample_towns() {
            let Some((b, inside)) = t.pulperia() else {
                continue;
            };
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
            assert_eq!(inside.seats.len(), 12);
        }
    }

    #[test]
    fn placement_keeps_the_whole_footprint_solid() {
        let t = castex();
        let (b, inside) = t.pulperia().unwrap();
        assert!(t.obstacles().contains(&b.collider()));
        let colliders = t.colliders();
        assert!(!colliders.contains(&b.collider()));
        assert!(inside.obstacles().all(|o| colliders.contains(&o)));
        assert_eq!(
            colliders.len(),
            t.obstacles().len() - 1 + inside.obstacles().count()
        );
    }
}
