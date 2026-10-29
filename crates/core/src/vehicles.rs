//! Los vehículos tirados por caballos: el carro de los caminos y el sulky de
//! las calles.
//!
//! Un vehículo comparte la lógica de un jinete ([`Traveller`](crate::Traveller),
//! [`TownRider`](crate::TownRider)): su posición es la de sus caballos, al
//! frente, y la caja va detrás, a lo largo del rumbo. Como obstáculo ocupa todo
//! su largo ([`Vehicle::bodies`]); render decide cómo se ve.

use crate::{Body, Circle, Heading, Rider};

/// Qué vehículo lleva alguien en lugar de montar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vehicle {
    /// El carro: cuatro ruedas, barandas de listones, tabla delantera y una
    /// lanza central entre dos caballos.
    Cart,
    /// El sulky: dos ruedas altas, un asiento y dos varas a los costados de un
    /// caballo.
    Sulky,
}

impl Vehicle {
    /// Cuántos caballos lo tiran.
    pub fn horses(self) -> usize {
        match self {
            Self::Cart => 2,
            Self::Sulky => 1,
        }
    }

    /// Lo que ocupa en el suelo: cada círculo, `(metros detrás de la posición
    /// a lo largo del rumbo, radio)`. El primero son los caballos.
    pub fn footprint(self) -> &'static [(f32, f32)] {
        match self {
            // La yunta de lado a lado, y la caja de 2,6 m entre los ejes.
            Self::Cart => &[(0.0, 1.2), (2.1, 1.0), (3.6, 1.0)],
            // El caballo, y las ruedas y el asiento sobre el eje.
            Self::Sulky => &[(0.0, Rider::COLLIDER_RADIUS), (1.85, 1.0)],
        }
    }

    /// El círculo de los caballos, al frente.
    pub fn collider(self) -> Circle {
        Circle::new(self.footprint()[0].1)
    }

    /// Dónde está cada parte del vehículo cuyos caballos están en `position`
    /// mirando hacia `heading`, como obstáculo para los demás.
    pub fn bodies(self, position: crate::Vec2, heading: Heading) -> Vec<Body> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;

    #[test]
    fn a_vehicle_takes_up_its_whole_length_behind_its_horses() {
        let at = Vec2::new(10.0, 5.0);
        let heading = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        for vehicle in [Vehicle::Cart, Vehicle::Sulky] {
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
}
