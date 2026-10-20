//! The player on foot, and mounting a tied horse.
//!
//! On foot the player moves with the same kinematic model as the rider
//! ([`Rider`]): the same input (W/S throttle, A/D turn, Shift to go
//! faster), the same collision and the same world. Only the tuning
//! ([`RiderParams::on_foot`]: a 6 km/h walk and an 18 km/h jog) and the body (a
//! person-sized collider) differ.

use crate::{Circle, Heading, Rider, RiderParams, Terrain, TetheredHorse, Vec2};

/// How the player gets around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Travel {
    /// Walking, or jogging with Shift.
    #[default]
    OnFoot,
    /// On horseback: walking, or galloping with Shift.
    Riding,
}

impl Rider {
    /// Collision radius of a person on foot seen from above, in meters.
    pub const PERSON_RADIUS: f32 = 0.3;

    /// A person standing on foot at `position`, facing `heading`.
    pub fn on_foot(position: Vec2, heading: Heading, terrain: &Terrain) -> Self {
        Self {
            collider: Circle::new(Self::PERSON_RADIUS),
            ..Self::new(position, heading, terrain)
        }
    }

    /// The horseback rider this person becomes on mounting `horse`: in the
    /// saddle where the horse stands, facing the way it faces, at rest.
    pub fn mount(&self, horse: &TetheredHorse, terrain: &Terrain) -> Self {
        Self::new(horse.position, horse.heading, terrain)
    }

    /// How this body gets around: a person-sized collider is on foot.
    pub fn travel(&self) -> Travel {
        if self.collider.radius < Self::COLLIDER_RADIUS {
            Travel::OnFoot
        } else {
            Travel::Riding
        }
    }
}

impl RiderParams {
    /// Walking pace 6 km/h, jogging (Shift) 18 km/h; a person
    /// turns and stops quicker than a horse.
    pub fn on_foot() -> Self {
        Self {
            turn_rate: 2.6,
            acceleration: 3.0,
            braking: 8.0,
            friction: 6.0,
            // 6 km/h.
            walk_max: 6.0 / 3.6,
            // 18 km/h, jogging with Shift.
            gallop_max: 18.0 / 3.6,
            reverse_max: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnimationState, RiderInput, World};

    const DT: f32 = 1.0 / 60.0;

    fn run(r: &mut Rider, input: RiderInput, p: &RiderParams, w: &World, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            r.step(input, p, w, DT);
        }
    }

    fn walker() -> (Rider, RiderParams, World) {
        let world = World::default();
        let r = Rider::on_foot(
            Vec2::new(5.0, 5.0),
            Heading::from_radians(0.0),
            &world.terrain,
        );
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
    fn walks_at_6_kmh_and_jogs_at_18_kmh() {
        let p = RiderParams::on_foot();
        let (walk_speed, jog_speed) = (6.0 / 3.6, 18.0 / 3.6);
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
        // Slower than a horse at either pace.
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

    #[test]
    fn mounting_puts_the_rider_on_the_horse_facing_its_way() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let horse = town.hitching_post.horse;
        let (person, _, _) = walker();
        let rider = person.mount(&horse, &world.terrain);
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
    fn a_mounted_horse_no_longer_blocks_the_spot_it_was_tied_at() {
        let town = crate::TownLayout::castex_inspired();
        let mut world = World::with_town(Terrain::default(), &town);
        let horse = town.hitching_post.horse;
        let (person, _, _) = walker();
        let mounted = person.mount(&horse, &world.terrain);
        let settle = |world: &World| {
            let mut rider = mounted;
            rider.step(RiderInput::default(), &RiderParams::default(), world, DT);
            rider.position.distance(horse.position)
        };
        // While tied, the horse's footprint is an obstacle: a rider there
        // is thrown well clear of it.
        assert!(world.obstacles.contains(&horse.footprint));
        assert!(settle(&world) > 1.0, "{}", settle(&world));
        assert!(world.obstacles.remove(&horse.footprint));
        assert!(!world.obstacles.contains(&horse.footprint));
        // Led away, it frees the spot: the rider only eases off the rail.
        assert!(settle(&world) < 0.5, "{}", settle(&world));
    }
}
