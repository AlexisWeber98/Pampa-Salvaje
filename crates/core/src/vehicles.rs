//! Los vehículos de 1919: los tirados por caballos o por bueyes (el sulky, el
//! carretón y la carreta) y el Ford T del intendente de General Pico.
//!
//! Un vehículo en marcha comparte la lógica de un jinete
//! ([`Traveller`](crate::Traveller), [`TownRider`](crate::TownRider)): su
//! posición es la de sus animales, al frente, y la caja va detrás, a lo largo
//! del rumbo. Como obstáculo ocupa todo su largo ([`Vehicle::bodies`]); render
//! decide cómo se ve (los modelos de `tools/sulky.py`, `tools/carreton.py`,
//! `tools/carreta.py` y `tools/ford_t.py`).
//!
//! Además, cada pueblo tiene vehículos estacionados con sus animales
//! ([`parked_in`], usuario, 2026-11-03: "uno o dos en ciudades, uno en
//! pueblos, uno muy esporádicamente en aldeas"), sembrados a partir de la
//! semilla del pueblo, junto al palenque del almacén o de otro edificio con
//! palenque; en General Pico, el Ford T del intendente, sin nadie, contra el
//! cordón frente a la puerta de la intendencia y por la mano izquierda, como se
//! circulaba hasta 1945 ([`Parked::driven`]).

use crate::places::side_normal;
use crate::rng::Rng;
use crate::town::{BuildingKind, Paving, TownConfig};
use crate::{Aabb, Body, Circle, Heading, Rider, TownLayout, Vec2};

/// Qué vehículo lleva alguien en lugar de montar, o qué está estacionado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vehicle {
    /// El carretón: cuatro ruedas, una caja de tablas y una lanza central
    /// entre dos caballos.
    Cart,
    /// El sulky: dos ruedas altas, un asiento y dos varas a los costados de un
    /// caballo.
    Sulky,
    /// La carreta: dos ruedas altísimas, techo de cuero y un largo pértigo
    /// hasta el yugo de una yunta de bueyes.
    Carreta,
    /// El Ford T del intendente: sin animales.
    FordT,
}

impl Vehicle {
    /// Los que pueden andar por los caminos (el Ford T solo está en General
    /// Pico).
    pub const DRAWN: [Vehicle; 3] = [Vehicle::Sulky, Vehicle::Cart, Vehicle::Carreta];

    /// Los que se estacionan en un pueblo (usuario, 2026-11-03): el carretón,
    /// el carro de carga de la época, y el sulky, el de la gente; la carreta
    /// de bueyes nunca (el ferrocarril la desplazó).
    pub const PARKED: [Vehicle; 2] = [Vehicle::Cart, Vehicle::Sulky];

    /// Cuántos animales lo tiran.
    pub fn horses(self) -> usize {
        match self {
            Self::Cart | Self::Carreta => 2,
            Self::Sulky => 1,
            Self::FordT => 0,
        }
    }

    /// Si lo tiran bueyes en lugar de caballos.
    pub fn oxen(self) -> bool {
        self == Self::Carreta
    }

    /// Lo que ocupa en el suelo: cada círculo, `(metros detrás de la posición
    /// a lo largo del rumbo, radio)`. El primero son los animales (para el Ford
    /// T, su frente).
    pub fn footprint(self) -> &'static [(f32, f32)] {
        match self {
            // La yunta de lado a lado, la caja de 3,3 m entre los ejes.
            Self::Cart => &[(0.0, 1.25), (2.2, 1.05), (3.5, 1.05), (4.5, 1.0)],
            // El caballo, las varas y el asiento sobre el eje.
            Self::Sulky => &[(0.0, Rider::COLLIDER_RADIUS), (1.4, 0.6), (2.3, 1.0)],
            // Los bueyes, el pértigo, y la caja de 3,8 m entre las ruedas.
            Self::Carreta => &[(0.0, 1.35), (1.9, 0.8), (3.0, 1.35), (4.2, 1.4), (5.4, 1.3)],
            // Desde el radiador hasta la capota.
            Self::FordT => &[(-1.25, 0.85), (0.0, 0.9), (1.25, 0.85)],
        }
    }

    /// El círculo de los animales, al frente.
    pub fn collider(self) -> Circle {
        Circle::new(self.footprint()[0].1)
    }

    /// Dónde está cada parte del vehículo cuyos animales están en `position`
    /// mirando hacia `heading`, como obstáculo para los demás.
    pub fn bodies(self, position: Vec2, heading: Heading) -> Vec<Body> {
        let back = heading.forward() * -1.0;
        self.footprint()
            .iter()
            .map(|&(behind, radius)| Body {
                position: position + back * behind,
                collider: Circle::new(radius),
            })
            .collect()
    }
}

/// Un vehículo estacionado en un pueblo, con sus animales quietos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parked {
    pub vehicle: Vehicle,
    /// Donde están sus animales (el centro del Ford T), y hacia dónde mira.
    pub position: Vec2,
    pub heading: Heading,
    /// Semilla de la apariencia del conductor y de los pelajes.
    pub seed: u32,
}

impl Parked {
    pub fn bodies(&self) -> Vec<Body> {
        self.vehicle.bodies(self.position, self.heading)
    }

    /// Si alguien espera sentado en el pescante: en el carretón y el sulky sí;
    /// el Ford T del intendente queda solo frente a la intendencia.
    pub fn driven(&self) -> bool {
        self.vehicle != Vehicle::FordT
    }
}

/// Se mezcla con la semilla del pueblo para sus vehículos estacionados.
const PARKED_SEED: u32 = 0x7061_726b;

/// La probabilidad de que una aldea tenga un vehículo estacionado ("uno muy
/// esporádicamente en aldeas").
pub const VILLAGE_CHANCE: f32 = 0.2;

/// Cuántos vehículos estacionados tiene un pueblo de este pavimento, según
/// `roll` (uniforme en `0..1`): uno o dos en una ciudad, uno en un pueblo, y
/// en una aldea uno con probabilidad [`VILLAGE_CHANCE`].
pub fn parked_count(paving: Paving, roll: f32) -> usize {
    match paving {
        Paving::Everywhere => 1 + usize::from(roll < 0.5),
        Paving::Plaza => 1,
        Paving::None => usize::from(roll < VILLAGE_CHANCE),
    }
}

/// Distancias a lo largo de la calle desde el palenque que se prueban para
/// estacionar, en metros, en orden.
const ALONG: [f32; 8] = [6.0, -6.0, 9.0, -9.0, 12.0, -12.0, 15.0, -15.0];

/// Los vehículos estacionados de `layout`: [`parked_count`] de ellos, junto al
/// palenque del almacén primero y después junto a los demás palenques,
/// paralelos a la calle, sin tocar ningún obstáculo del pueblo ni entre sí; y
/// en el pueblo de la intendencia ([`TownConfig::TOWN_HALL_TOWN`]), además, el
/// Ford T frente a ella. Determinista para la semilla del pueblo.
pub fn parked_in(layout: &TownLayout) -> Vec<Parked> {
    let mut rng = Rng::new(layout.seed ^ PARKED_SEED, 0);
    let count = parked_count(Paving::of(layout.blocks()), rng.unit());
    let obstacles = layout.obstacles();
    let mut parked: Vec<Parked> = Vec::new();
    let store = layout
        .buildings
        .iter()
        .position(|b| b.kind == BuildingKind::GeneralStore);
    let fronts: Vec<(usize, Aabb)> = store
        .map(|b| (b, layout.hitching_post.rail))
        .into_iter()
        .chain(layout.palenques.iter().map(|p| (p.building, p.rail)))
        .collect();
    let clear = |candidate: &Parked, parked: &[Parked]| {
        candidate.bodies().iter().all(|b| {
            obstacles
                .iter()
                .all(|o| o.closest_point(b.position).distance(b.position) > b.collider.radius)
                && parked
                    .iter()
                    .flat_map(Parked::bodies)
                    .all(|p| !b.collider.overlaps(b.position, p.collider, p.position))
        })
    };
    let place =
        |vehicle: Vehicle, fronts: &[(usize, Aabb)], parked: &mut Vec<Parked>, seed: u32| {
            for &(building, rail) in fronts {
                let normal = side_normal(layout.buildings[building].front);
                let street = Vec2::new(-normal.y, normal.x);
                // Más afuera que el palenque y el caballo atado a él.
                let out = rail.center() + normal * (TETHER_OUT + vehicle.collider().radius);
                for along in ALONG {
                    // Mirando hacia donde queda la caja: sus animales quedan del
                    // lado del palenque.
                    let dir = if along > 0.0 { street * -1.0 } else { street };
                    let heading = Heading::from_radians(dir.y.atan2(dir.x));
                    let candidate = Parked {
                        vehicle,
                        position: out + street * along,
                        heading,
                        seed,
                    };
                    if clear(&candidate, parked) {
                        parked.push(candidate);
                        return;
                    }
                }
            }
        };
    for k in 0..count {
        let vehicle = Vehicle::PARKED[rng.below(Vehicle::PARKED.len())];
        let seed = rng.next() ^ k as u32;
        place(vehicle, &fronts, &mut parked, seed);
    }
    if layout.name == TownConfig::TOWN_HALL_TOWN {
        let seed = rng.next();
        let at_the_door = layout
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::TownHall)
            .find_map(|hall| {
                at_the_curb(layout, hall, Vehicle::FordT, seed, |c| clear(c, &parked))
            });
        match at_the_door {
            Some(ford) => parked.push(ford),
            None => {
                let hall: Vec<(usize, Aabb)> = layout
                    .palenques
                    .iter()
                    .filter(|p| layout.buildings[p.building].kind == BuildingKind::TownHall)
                    .map(|p| (p.building, p.rail))
                    .collect();
                place(Vehicle::FordT, &hall, &mut parked, seed);
            }
        }
    }
    parked
}

/// Distancias a lo largo del cordón desde la puerta que se prueban para
/// estacionar frente a ella, en metros, en orden.
const AT_THE_DOOR: [f32; 7] = [0.0, 1.5, -1.5, 3.0, -3.0, 4.5, -4.5];

/// Cuánto queda entre el cordón y el costado de un vehículo estacionado
/// contra él, en metros.
const CURB_GAP: f32 = 0.2;

/// `vehicle` estacionado en la calzada contra el cordón de la vereda de
/// `building`, frente a su puerta (el centro de su frente), por la mano
/// izquierda como se circulaba en la Argentina hasta 1945: la vereda queda a su
/// izquierda. El primer lugar que `clear` acepta, o ninguno si el edificio no
/// tiene vereda.
fn at_the_curb(
    layout: &TownLayout,
    building: &crate::town::Building,
    vehicle: Vehicle,
    seed: u32,
    clear: impl Fn(&Parked) -> bool,
) -> Option<Parked> {
    let normal = side_normal(building.front);
    let fp = building.footprint;
    let door = fp.center() + normal * (fp.size().dot(normal).abs() / 2.0);
    let walk = layout
        .sidewalks()
        .iter()
        .find(|s| s.area.contains_point(door + normal * 0.5))?;
    let curb = (walk.area.closest_point(door + normal * 100.0) - door).dot(normal);
    let out = door + normal * (curb + CURB_GAP + vehicle.collider().radius);
    // Con la vereda a la izquierda, mira a lo largo de la calle.
    let street = Vec2::new(-normal.y, normal.x);
    let heading = Heading::from_radians(street.y.atan2(street.x));
    AT_THE_DOOR
        .iter()
        .map(|&along| Parked {
            vehicle,
            position: out + street * along,
            heading,
            seed,
        })
        .find(|c| clear(c))
}

/// Cuánto más afuera del palenque, hacia la calle, empieza un vehículo
/// estacionado: el largo del caballo atado y medio metro.
const TETHER_OUT: f32 = 3.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    #[test]
    fn a_vehicle_takes_up_its_whole_length_behind_its_horses() {
        let at = Vec2::new(10.0, 5.0);
        let heading = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        for vehicle in [Vehicle::Cart, Vehicle::Sulky, Vehicle::Carreta] {
            let bodies = vehicle.bodies(at, heading);
            let covered = |p: Vec2| {
                bodies
                    .iter()
                    .any(|b| b.position.distance(p) < b.collider.radius)
            };
            // Los caballos y la caja, detrás de ellos (hacia el sur), sin
            // huecos por los que pasar.
            let tail = vehicle.footprint().last().unwrap().0;
            for k in 0..=20 {
                let p = at - Vec2::new(0.0, tail * k as f32 / 20.0);
                assert!(covered(p), "{vehicle:?} open at {p:?}");
            }
            assert!(!covered(at + Vec2::new(0.0, 2.0)), "nothing ahead");
            assert_eq!(bodies[0].collider, vehicle.collider());
        }
    }

    fn town(name: &str, blocks: (usize, usize), seed: u32) -> TownLayout {
        TownLayout::generate(&TownConfig {
            name: name.into(),
            blocks,
            seed,
            ..TownConfig::castex()
        })
        .unwrap()
    }

    fn drawn(parked: &[Parked]) -> usize {
        parked
            .iter()
            .filter(|p| p.vehicle != Vehicle::FordT)
            .count()
    }

    #[test]
    fn a_city_parks_one_or_two_a_town_one_and_a_village_rarely_one() {
        let mut city = std::collections::BTreeSet::new();
        let mut villages = 0;
        for seed in 0..40 {
            city.insert(drawn(&parked_in(&town("Ciudad", (8, 8), seed))));
            assert_eq!(
                drawn(&parked_in(&town("Pueblo", (5, 5), seed))),
                1,
                "seed {seed}"
            );
            let village = drawn(&parked_in(&town("Aldea", (3, 3), seed)));
            assert!(village <= 1);
            villages += village;
        }
        assert_eq!(city, [1, 2].into(), "a city parks one or two");
        for seed in 0..40 {
            let parked = parked_in(&town(TownConfig::TOWN_HALL_TOWN, (8, 8), seed));
            assert!(
                parked.iter().all(|p| p.vehicle != Vehicle::Carreta),
                "no ox carreta parked in a city"
            );
        }
        assert!(
            (1..=16).contains(&villages),
            "only now and then in a village: {villages}/40"
        );
    }

    #[test]
    fn parked_vehicles_stand_clear_of_the_town_and_of_each_other() {
        for seed in 0..12 {
            let layout = town(TownConfig::TOWN_HALL_TOWN, (8, 8), seed);
            let parked = parked_in(&layout);
            let obstacles = layout.obstacles();
            for (i, p) in parked.iter().enumerate() {
                for b in p.bodies() {
                    assert!(
                        obstacles
                            .iter()
                            .all(|o| o.closest_point(b.position).distance(b.position)
                                > b.collider.radius)
                    );
                    for q in &parked[i + 1..] {
                        assert!(
                            q.bodies()
                                .iter()
                                .all(|c| !b.collider.overlaps(b.position, c.collider, c.position))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_ford_t_is_parked_at_the_intendencia_of_general_pico_only() {
        let shipped = TownLayout::generate(&crate::town::tests::general_pico()).unwrap();
        for pico in [town(TownConfig::TOWN_HALL_TOWN, (8, 8), 7), shipped] {
            ford_at_the_door(&pico);
        }
        let other = town("Ciudad", (8, 8), 7);
        assert!(
            parked_in(&other)
                .iter()
                .all(|p| p.vehicle != Vehicle::FordT)
        );
    }

    fn ford_at_the_door(pico: &TownLayout) {
        let fords: Vec<_> = parked_in(pico)
            .into_iter()
            .filter(|p| p.vehicle == Vehicle::FordT)
            .collect();
        assert_eq!(fords.len(), 1);
        let ford = fords[0];
        assert!(
            !ford.driven(),
            "the intendente's car waits without a driver"
        );
        let hall = pico
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::TownHall)
            .unwrap();
        let normal = side_normal(hall.front);
        let door =
            hall.footprint.center() + normal * (hall.footprint.size().dot(normal).abs() / 2.0);
        // Contra el cordón de la vereda de la intendencia, en la calzada.
        let walk = pico
            .sidewalks()
            .iter()
            .find(|s| s.area.contains_point(door + normal * 0.5))
            .expect("the intendencia has a sidewalk");
        let curb = (walk.area.closest_point(door + normal * 100.0) - door).dot(normal);
        let out = (ford.position - door).dot(normal) - curb;
        let half = Vehicle::FordT.collider().radius;
        assert!(
            out > half && out < half + 0.5,
            "at the curb: {out} m out from it"
        );
        let along = (ford.position - door).dot(Vec2::new(-normal.y, normal.x));
        assert!(along.abs() < 4.0, "at the door: {along} m along the street");
        // Por la mano izquierda, como hasta 1945: la vereda a su izquierda.
        let forward = ford.heading.forward();
        let left = Vec2::new(-forward.y, forward.x);
        assert!(left.dot(normal) < -0.99, "the curb on its left: {left:?}");
        assert!(
            parked_in(pico)
                .iter()
                .filter(|p| p.vehicle != Vehicle::FordT)
                .all(Parked::driven)
        );
    }
}
