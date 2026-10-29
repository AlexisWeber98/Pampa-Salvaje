//! El jugador a pie, y cómo montar un caballo atado.
//!
//! A pie, el jugador se mueve con el mismo modelo cinemático que el jinete
//! ([`Rider`]): la misma entrada (W/S para acelerar, A/D para girar, Shift para
//! ir más rápido), la misma colisión y el mismo mundo. Solo cambian el ajuste
//! ([`RiderParams::on_foot`]: una caminata de 1,6 m/s y un trote de 18 km/h) y
//! el cuerpo (un collider del tamaño de una persona).

use crate::{Circle, Ground, Heading, Rider, RiderParams, TetheredHorse, Vec2, World};

/// Cómo se desplaza el jugador.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Travel {
    /// Caminando, o trotando con Shift.
    #[default]
    OnFoot,
    /// A caballo: al paso, o al galope con Shift.
    Riding,
}

impl Rider {
    /// Radio de colisión de una persona a pie vista desde arriba, en metros.
    pub const PERSON_RADIUS: f32 = 0.3;

    /// Una persona parada a pie en `position`, mirando hacia `heading`, sobre
    /// `ground` (pasar el [`crate::World`] para que cuenten los andenes).
    pub fn on_foot(position: Vec2, heading: Heading, ground: &(impl Ground + ?Sized)) -> Self {
        Self {
            collider: Circle::new(Self::PERSON_RADIUS),
            ..Self::new(position, heading, ground)
        }
    }

    /// El jinete en que se convierte esta persona al montar `horse`: en la
    /// montura, donde está el caballo, mirando hacia donde mira, en reposo.
    pub fn mount(&self, horse: &TetheredHorse, ground: &(impl Ground + ?Sized)) -> Self {
        Self::new(horse.position, horse.heading, ground)
    }

    /// Velocidad máxima a la que puede ir un jinete para bajarse, en m/s:
    /// detenido o a un paso lento.
    pub const DISMOUNT_SPEED: f32 = 1.0;

    /// Bajarse: la persona a pie al lado del caballo, en reposo y mirando hacia
    /// donde mira el caballo, y el caballo atado sin apretar donde estaba. La
    /// persona se baja por el lado izquierdo del caballo, o por el derecho
    /// cuando el izquierdo está bloqueado. `None` mientras se cabalga más
    /// rápido que [`Self::DISMOUNT_SPEED`], en el aire, cuando ambos flancos
    /// están bloqueados o cuando ya se está a pie.
    pub fn dismount(&self, world: &World) -> Option<(Self, TetheredHorse)> {
        if self.travel() != Travel::Riding
            || self.speed.abs() > Self::DISMOUNT_SPEED
            || self.airborne()
        {
            return None;
        }
        let horse = TetheredHorse::standing(self.position, self.heading);
        let ahead = self.heading.forward();
        let left = Vec2::new(-ahead.y, ahead.x);
        let person = Circle::new(Self::PERSON_RADIUS);
        [left, left * -1.0].into_iter().find_map(|side| {
            // Justo afuera de la huella del caballo de ese lado.
            let mut spot = self.position + side * (TetheredHorse::WIDTH / 2.0);
            while horse.footprint.closest_point(spot).distance(spot) < Self::PERSON_RADIUS + 0.05 {
                spot += side * 0.1;
            }
            let settled = world.resolve(person, spot, |_| ());
            (settled.distance(spot) < 1e-3).then(|| {
                let walker = Self::on_foot(spot, self.heading, world);
                (walker, horse)
            })
        })
    }

    /// Cómo se desplaza este cuerpo: un collider del tamaño de una persona va a
    /// pie.
    pub fn travel(&self) -> Travel {
        if self.collider.radius < Self::COLLIDER_RADIUS {
            Travel::OnFoot
        } else {
            Travel::Riding
        }
    }
}

impl RiderParams {
    /// Ritmo de caminata de 1,6 m/s (5,8 km/h: un paso enérgico por la calle en
    /// 1919, un poco más rápido que los 1,25 a 1,5 de los vecinos), trote
    /// (Shift) de 18 km/h; una persona gira y se detiene más rápido que un
    /// caballo.
    pub fn on_foot() -> Self {
        Self {
            turn_rate: 2.6,
            acceleration: 3.0,
            braking: 8.0,
            friction: 6.0,
            // 1,6 m/s (usuario, 2026-10-25; antes era 6 km/h, con lo que el
            // clip de caminata se veía apurado).
            walk_max: 1.6,
            // 18 km/h, trotando con Shift.
            gallop_max: 18.0 / 3.6,
            reverse_max: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnimationState, Floor, RiderInput, Terrain, World};

    const DT: f32 = 1.0 / 60.0;

    fn run(r: &mut Rider, input: RiderInput, p: &RiderParams, w: &World, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            r.step(input, p, w, DT);
        }
    }

    fn walker() -> (Rider, RiderParams, World) {
        let world = World::default();
        let r = Rider::on_foot(Vec2::new(5.0, 5.0), Heading::from_radians(0.0), &world);
        (r, RiderParams::on_foot(), world)
    }

    #[test]
    fn a_person_on_foot_has_a_person_sized_body() {
        let (r, _, world) = walker();
        assert_eq!(r.collider.radius, Rider::PERSON_RADIUS);
        assert!(r.collider.radius < Rider::COLLIDER_RADIUS / 2.0);
        assert_eq!(r.speed, 0.0);
        assert_eq!(r.height, world.terrain.height(5.0, 5.0));
        assert_eq!(r.travel(), Travel::OnFoot);
        assert_eq!(Travel::default(), Travel::OnFoot);
    }

    #[test]
    fn walks_at_1_6_m_s_and_jogs_at_18_kmh() {
        let p = RiderParams::on_foot();
        let (walk_speed, jog_speed) = (1.6, 18.0 / 3.6);
        assert!((p.walk_max - walk_speed).abs() < 1e-6, "{}", p.walk_max);
        assert!((p.gallop_max - jog_speed).abs() < 1e-6, "{}", p.gallop_max);
        let (mut r, p, world) = walker();
        let walk = RiderInput {
            throttle: 1.0,
            ..Default::default()
        };
        run(&mut r, walk, &p, &world, 4.0);
        assert!((r.speed - walk_speed).abs() < 1e-4, "walk {}", r.speed);
        assert_eq!(r.animation(&p), AnimationState::Walk);
        let jog = RiderInput {
            gallop: true,
            ..walk
        };
        run(&mut r, jog, &p, &world, 4.0);
        assert!((r.speed - jog_speed).abs() < 1e-4, "jog {}", r.speed);
        // Más lento que un caballo a cualquiera de los dos ritmos.
        let horse = RiderParams::default();
        assert!(p.walk_max < horse.walk_max && p.gallop_max < horse.gallop_max / 2.0);
    }

    #[test]
    fn a_person_cannot_walk_through_a_wall() {
        let (mut r, p, mut world) = walker();
        world.obstacles.push(crate::Aabb::new(
            Vec2::new(10.0, 0.0),
            Vec2::new(12.0, 10.0),
        ));
        let jog = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        run(&mut r, jog, &p, &world, 6.0);
        let face = 10.0 - Rider::PERSON_RADIUS;
        assert!((r.position.x - face).abs() < 1e-3, "x = {}", r.position.x);
    }

    /// Hace caminar a `person` con `input` durante `seconds` y devuelve la
    /// altura del suelo en cada paso.
    fn walk(person: &mut Rider, input: RiderInput, world: &World, seconds: f32) -> Vec<f32> {
        let p = RiderParams::on_foot();
        (0..(seconds / DT) as usize)
            .map(|_| {
                person.step(input, &p, world, DT);
                person.height
            })
            .collect()
    }

    #[test]
    fn a_person_walks_off_the_platform_into_town_down_a_ramp_or_its_edge() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let floor = town.floors()[0];
        let (start, facing) = town.platform_spot().unwrap();
        let top = world.ground_height(start);
        let ground = |r: &Rider| world.terrain.height(r.position.x, r.position.y);
        let forward = RiderInput {
            throttle: 1.0,
            ..Default::default()
        };

        // A lo largo del andén y bajando por la rampa del extremo, sin trabarse
        // nunca.
        let mut person = Rider::on_foot(start, Heading::from_radians(0.0), &world);
        person.heading = Heading::from_radians(floor.along.y.atan2(floor.along.x));
        assert_eq!(person.height, top);
        let heights = walk(&mut person, forward, &world, 24.0);
        let travelled = (person.position - start).dot(floor.along);
        let to_foot = floor.footprint.size().dot(floor.along) / 2.0 + Floor::RAMP_LENGTH;
        assert!(travelled > to_foot + 2.0, "stuck at {travelled}");
        assert!(heights.windows(2).all(|w| w[1] <= w[0] + 1e-4));
        assert_eq!(person.height, ground(&person));

        // Bajando por el lado del pueblo, pasando el edificio de la estación,
        // hasta el patio.
        let beside_station = start + floor.along * 20.0;
        let mut person = Rider::on_foot(beside_station, facing, &world);
        assert_eq!(person.height, top);
        walk(&mut person, forward, &world, 10.0);
        let off = (person.position - beside_station).dot(facing.forward());
        assert!(off > 12.0, "stuck after {off} m");
        assert!(!floor.footprint.contains_point(person.position));
        assert_eq!(person.height, ground(&person));
    }

    #[test]
    fn mounting_puts_the_rider_on_the_horse_facing_its_way() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let horse = town.hitching_post.horse;
        let (person, _, _) = walker();
        let rider = person.mount(&horse, &world);
        assert_eq!(rider.position, horse.position);
        assert_eq!(rider.heading, horse.heading);
        assert_eq!(rider.speed, 0.0);
        assert_eq!(rider.collider.radius, Rider::COLLIDER_RADIUS);
        assert_eq!(rider.travel(), Travel::Riding);
        assert_eq!(
            rider.height,
            world.terrain.height(horse.position.x, horse.position.y)
        );
    }

    #[test]
    fn dismounting_leaves_the_horse_tied_loosely_and_the_rider_on_foot_beside_it() {
        let world = World::default();
        let heading = Heading::from_radians(0.7);
        let mut rider = Rider::new(Vec2::new(20.0, -5.0), heading, &world);
        // No mientras se sigue cabalgando.
        rider.speed = 3.0;
        assert_eq!(rider.dismount(&world), None);
        rider.speed = Rider::DISMOUNT_SPEED * 0.5;
        let (person, horse) = rider.dismount(&world).expect("slow enough");
        assert_eq!(person.travel(), Travel::OnFoot);
        assert_eq!(person.speed, 0.0);
        assert_eq!(person.heading, heading);
        assert_eq!((horse.position, horse.heading), (rider.position, heading));
        assert!(horse.footprint.contains_point(horse.position));
        // Al lado del caballo, afuera de su huella, lo bastante cerca como para
        // volver a montar.
        let gap = horse
            .footprint
            .closest_point(person.position)
            .distance(person.position);
        assert!(gap >= Rider::PERSON_RADIUS, "{gap}");
        assert!(person.position.distance(horse.position) <= crate::Errand::MOUNT_RANGE);
        // Atado, el caballo bloquea; volver a montar pone de nuevo al jinete.
        let mut world = world;
        world.obstacles.push(horse.footprint);
        let mut walker = person;
        walker.step(RiderInput::default(), &RiderParams::on_foot(), &world, DT);
        assert!(walker.position.distance(person.position) < 1e-3, "pushed");
        let again = person.mount(&horse, &world);
        assert_eq!((again.position, again.heading), (horse.position, heading));
        assert_eq!(again.travel(), Travel::Riding);
    }

    #[test]
    fn a_rider_hemmed_in_on_both_sides_cannot_get_off() {
        let mut world = World::default();
        let rider = Rider::new(Vec2::new(0.0, 0.0), Heading::from_radians(0.0), &world);
        // Paredes pegadas a ambos flancos.
        world
            .obstacles
            .push(crate::Aabb::new(Vec2::new(-5.0, 1.0), Vec2::new(5.0, 3.0)));
        world.obstacles.push(crate::Aabb::new(
            Vec2::new(-5.0, -3.0),
            Vec2::new(5.0, -1.0),
        ));
        assert_eq!(rider.dismount(&world), None);
        // A pie no hay nada de qué bajarse.
        let person = Rider::on_foot(Vec2::new(30.0, 0.0), Heading::from_radians(0.0), &world);
        assert_eq!(person.dismount(&world), None);
    }

    #[test]
    fn a_mounted_horse_no_longer_blocks_the_spot_it_was_tied_at() {
        let town = crate::TownLayout::castex_inspired();
        let mut world = World::with_town(Terrain::default(), &town);
        let horse = town.hitching_post.horse;
        let (person, _, _) = walker();
        let mounted = person.mount(&horse, &world);
        let settle = |world: &World| {
            let mut rider = mounted;
            rider.step(RiderInput::default(), &RiderParams::default(), world, DT);
            rider.position.distance(horse.position)
        };
        // Mientras está atado, la huella del caballo es un obstáculo: un jinete
        // que está ahí sale despedido bien lejos.
        assert!(world.obstacles.contains(&horse.footprint));
        assert!(settle(&world) > 1.0, "{}", settle(&world));
        assert!(world.obstacles.remove(&horse.footprint));
        assert!(!world.obstacles.contains(&horse.footprint));
        // Si se lo lleva, libera el lugar: el jinete solo se aparta un poco del
        // riel.
        assert!(settle(&world) < 0.5, "{}", settle(&world));
    }
}
