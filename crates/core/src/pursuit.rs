//! La persecución a caballo: cómo lleva las riendas un jinete que corre a otro.
//!
//! El perseguidor anda con la misma física que su presa ([`Rider::step`]),
//! un poco más lento al galope ([`CHASER_PACE`]): una corrida derecha le saca
//! ventaja y una parada o un mal giro se la hace perder. Esquiva lo que no
//! puede saltar, salta los alambrados a galope tendido como el jugador y, una
//! vez al lado ([`ALONGSIDE`]), solo le sigue el ritmo. Bairoletto los aleja
//! a tiros con un [`Revolver`] de pocas balas.

use std::f32::consts::{PI, TAU};

use crate::{
    Aabb, FenceLayout, Heading, Rider, RiderInput, RiderParams, StaticObstacles, Vec2, World,
};

/// La fracción del galope máximo de la presa que alcanza un perseguidor.
pub const CHASER_PACE: f32 = 0.9;
/// A esta distancia de su presa, en metros, el perseguidor solo le sigue el ritmo.
pub const ALONGSIDE: f32 = 5.0;
/// Con esta ventaja, en metros, la presa dejó atrás a su perseguidor.
pub const OUTRUN: f32 = 600.0;
/// Con esta ventaja, en metros, la presa se le pierde en el monte.
pub const LOST_IN_MONTE: f32 = 400.0;
/// Cuánto puede apartarse el rumbo del camino para seguir acelerando, en radianes.
const ON_COURSE: f32 = 0.8;
/// Con esta velocidad de más sobre el ritmo que debe llevar (m/s), frena.
const PACE_SLACK: f32 = 0.5;
/// Los desvíos que prueba cuando el camino está tapado, del menor al mayor (radianes).
const SWERVES: [f32; 8] = [0.35, -0.35, 0.7, -0.7, 1.2, -1.2, 1.8, -1.8];
/// Cuánto mira por delante: estos segundos a su velocidad, y como mínimo
/// [`LOOK_MIN`] metros.
const LOOK_SECONDS: f32 = 1.0;
const LOOK_MIN: f32 = 6.0;
/// Cuánto antes de un alambrado despega: estos segundos a su velocidad.
const TAKE_OFF_SECONDS: f32 = 0.35;

/// La física de un perseguidor: la del jugador, con el galope máximo a
/// [`CHASER_PACE`].
pub fn chaser_params() -> RiderParams {
    let quarry = RiderParams::default();
    RiderParams {
        gallop_max: quarry.gallop_max * CHASER_PACE,
        sprint_max: quarry.sprint_max * CHASER_PACE,
        ..quarry
    }
}

/// Lo que hace un perseguidor en este paso: sus riendas y si salta.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pursuit {
    pub input: RiderInput,
    pub jump: bool,
}

/// Cómo persigue `rider` (con `params`) a `quarry` por `world` durante `dt`:
/// al galope hacia ella, esquivando lo que no puede saltar y saltando un
/// alambrado a galope tendido; frena a tiempo para quedar a [`ALONGSIDE`] de
/// ella, y ahí solo le sigue el ritmo (se detiene si ella se detiene).
pub fn pursue(
    rider: &Rider,
    quarry: &Rider,
    params: &RiderParams,
    world: &World,
    dt: f32,
) -> Pursuit {
    let toward = quarry.position - rider.position;
    let distance = toward.length();
    let aim = toward.y.atan2(toward.x);
    let look = distance.min((rider.speed.abs() * LOOK_SECONDS).max(LOOK_MIN));
    let (goal, jump) = way_around(rider, params, aim, look, world);
    let delta = (goal - rider.heading.radians() + PI).rem_euclid(TAU) - PI;
    let reach = (params.turn_rate * dt).max(f32::EPSILON);
    // Frena a tiempo para quedar a su lado al ritmo de ella.
    let ahead = (distance - ALONGSIDE).max(0.0);
    let top =
        (quarry.speed.max(0.0) + (2.0 * params.braking * ahead).sqrt()).min(params.gallop_max);
    let throttle = if rider.speed > top + PACE_SLACK {
        -1.0
    } else if delta.abs() < ON_COURSE && rider.speed < top {
        1.0
    } else {
        0.0
    };
    Pursuit {
        input: RiderInput {
            throttle,
            turn: (delta / reach).clamp(-1.0, 1.0),
            gallop: true,
            sprint: false,
        },
        jump,
    }
}

/// Si un perseguidor que va `lead` metros detrás de su presa ya la perdió:
/// a más de [`OUTRUN`], o de [`LOST_IN_MONTE`] si ella está en el monte.
pub fn outrun(lead: f32, in_monte: bool) -> bool {
    lead > if in_monte { LOST_IN_MONTE } else { OUTRUN }
}

/// Las riendas de un perseguidor al que le espantaron el caballo: frena hasta
/// quedarse quieto, derecho.
pub fn rein_in(rider: &Rider) -> RiderInput {
    RiderInput {
        throttle: if rider.speed > PACE_SLACK { -1.0 } else { 0.0 },
        ..RiderInput::default()
    }
}

/// El revólver de Bairoletto en la huida: le quedan [`Revolver::LOADED`]
/// balas (seis menos la que gastó con Farach) y, entre tiro y tiro, tarda
/// [`Revolver::COCK_SECONDS`] en volver a apuntar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Revolver {
    pub rounds: u8,
    pub ready_in: f32,
}

impl Revolver {
    /// Las balas que le quedan al salir de La Colonia.
    pub const LOADED: u8 = 5;
    /// A cuántos metros de la presa un perseguidor queda a tiro.
    pub const RANGE: f32 = 15.0;
    /// Segundos entre un tiro y el siguiente.
    pub const COCK_SECONDS: f32 = 2.0;
    /// Segundos que frena el perseguidor al que le tiran.
    pub const SHIED_SECONDS: f32 = 4.0;

    /// Si ya no le quedan balas.
    pub fn empty(&self) -> bool {
        self.rounds == 0
    }

    /// Avanza `dt` segundos y, si puede disparar y algún perseguidor está a
    /// tiro (`distances`, en metros, a la presa), le tira al más cercano:
    /// devuelve su índice y gasta una bala.
    pub fn fire(&mut self, distances: &[f32], dt: f32) -> Option<usize> {
        self.ready_in = (self.ready_in - dt).max(0.0);
        if self.empty() || self.ready_in > 0.0 {
            return None;
        }
        let (target, _) = distances
            .iter()
            .enumerate()
            .filter(|(_, d)| **d <= Self::RANGE)
            .min_by(|a, b| a.1.total_cmp(b.1))?;
        self.rounds -= 1;
        self.ready_in = Self::COCK_SECONDS;
        Some(target)
    }

    /// Si atrapan a la presa: sin balas y con un perseguidor a [`ALONGSIDE`]
    /// metros o menos (`distances`).
    pub fn caught(&self, distances: &[f32]) -> bool {
        self.empty() && distances.iter().any(|d| *d <= ALONGSIDE)
    }
}

impl Default for Revolver {
    fn default() -> Self {
        Self {
            rounds: Self::LOADED,
            ready_in: 0.0,
        }
    }
}

/// El rumbo que toma hacia `aim`, mirando `look` metros adelante, y si salta
/// ahora: derecho si está libre, o por encima de un alambrado a galope
/// tendido; si no, el menor desvío libre (derecho si no hay ninguno).
fn way_around(
    rider: &Rider,
    params: &RiderParams,
    aim: f32,
    look: f32,
    world: &World,
) -> (f32, bool) {
    let blocked = |bearing: f32, length: f32, clearance: f32| {
        let to = rider.position + Heading::from_radians(bearing).forward() * length;
        hits(world, rider.position, to, clearance)
    };
    if !blocked(aim, look, StaticObstacles::ON_GROUND) {
        return (aim, false);
    }
    let galloping = rider.speed >= Rider::FULL_GALLOP * params.gallop_max;
    if galloping && !blocked(aim, look, FenceLayout::HEIGHT) {
        let take_off = (rider.speed * TAKE_OFF_SECONDS).max(Rider::COLLIDER_RADIUS);
        return (aim, blocked(aim, take_off, StaticObstacles::ON_GROUND));
    }
    let swerve = SWERVES
        .iter()
        .map(|s| aim + s)
        .find(|&bearing| !blocked(bearing, look, StaticObstacles::ON_GROUND));
    (swerve.unwrap_or(aim), false)
}

/// Si un jinete que va de `from` a `to` choca con algún obstáculo de `world`
/// más alto que `clearance`.
fn hits(world: &World, from: Vec2, to: Vec2, clearance: f32) -> bool {
    let r = Rider::COLLIDER_RADIUS;
    let area = Aabb::new(
        Vec2::new(from.x.min(to.x), from.y.min(to.y)),
        Vec2::new(from.x.max(to.x), from.y.max(to.y)),
    )
    .expanded(r);
    world
        .obstacles
        .near_above(area, clearance)
        .into_iter()
        .any(|b| b.expanded(r).segment_overlap(from, to).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 20.0;

    /// Hace correr a `chaser` detrás de `quarry` (que anda con `reins`) durante
    /// `seconds`, y devuelve lo más cerca que estuvieron.
    fn chase(
        chaser: &mut Rider,
        quarry: &mut Rider,
        reins: RiderInput,
        world: &World,
        seconds: f32,
    ) -> f32 {
        let (mine, theirs) = (chaser_params(), RiderParams::default());
        let mut closest = f32::INFINITY;
        for _ in 0..(seconds / DT) as usize {
            let go = pursue(chaser, quarry, &mine, world, DT);
            if go.jump {
                chaser.jump(&mine);
            }
            chaser.step(go.input, &mine, world, DT);
            chaser.keep_clear_of(world, &[quarry.body()]);
            quarry.step(reins, &theirs, world, DT);
            closest = closest.min(chaser.position.distance(quarry.position));
        }
        closest
    }

    fn east(world: &World, x: f32, y: f32) -> Rider {
        Rider::new(Vec2::new(x, y), Heading::from_radians(0.0), world)
    }

    const GALLOP: RiderInput = RiderInput {
        throttle: 1.0,
        turn: 0.0,
        gallop: true,
        sprint: false,
    };

    #[test]
    fn a_chaser_runs_down_a_stopped_rider_and_pulls_up_beside_him() {
        let world = World::default();
        let (mut chaser, mut quarry) = (east(&world, 0.0, 0.0), east(&world, 80.0, 30.0));
        let closest = chase(
            &mut chaser,
            &mut quarry,
            RiderInput::default(),
            &world,
            30.0,
        );
        let gap = chaser.position.distance(quarry.position);
        assert!(gap <= ALONGSIDE + 0.5, "caught up: {gap} m");
        let touching = 2.0 * Rider::COLLIDER_RADIUS;
        assert!(
            closest > touching + 1.0,
            "never bumped into him: {closest} m"
        );
        assert!(chaser.speed.abs() < 0.5, "pulled up: {} m/s", chaser.speed);
    }

    #[test]
    fn a_straight_gallop_slowly_gains_ground_on_the_chaser() {
        let world = World::default();
        let (mut chaser, mut quarry) = (east(&world, 0.0, 0.0), east(&world, 20.0, 0.0));
        quarry.speed = RiderParams::default().gallop_max;
        chaser.speed = chaser_params().gallop_max;
        chase(&mut chaser, &mut quarry, GALLOP, &world, 30.0);
        let gap = quarry.position.x - chaser.position.x;
        // Un 10 % más lento: unos 1,4 m/s, o 40 m en 30 s.
        assert!((50.0..70.0).contains(&gap), "gap {gap} m");
        assert!((chaser.speed - chaser_params().gallop_max).abs() < 0.1);
    }

    #[test]
    fn a_chaser_rides_around_a_building_and_jumps_a_fence() {
        let mut world = World::default();
        world
            .obstacles
            .push(Aabb::new(Vec2::new(25.0, -15.0), Vec2::new(30.0, 15.0)));
        let fence = Aabb::from_center_size(Vec2::new(60.0, 0.0), Vec2::new(0.15, 400.0));
        world.obstacles.push_low(fence, FenceLayout::HEIGHT);
        let (mut chaser, mut quarry) = (east(&world, 0.0, 0.0), east(&world, 90.0, 0.0));
        let closest = chase(
            &mut chaser,
            &mut quarry,
            RiderInput::default(),
            &world,
            40.0,
        );
        assert!(closest <= ALONGSIDE, "reached him past both: {closest} m");
    }

    #[test]
    fn they_give_up_six_hundred_metres_behind_or_four_hundred_in_the_monte() {
        assert!(!outrun(OUTRUN, false));
        assert!(outrun(OUTRUN + 1.0, false));
        assert!(!outrun(LOST_IN_MONTE + 1.0, false));
        assert!(!outrun(LOST_IN_MONTE, true));
        assert!(outrun(LOST_IN_MONTE + 1.0, true));
    }

    #[test]
    fn five_rounds_for_the_nearest_within_range_one_every_two_seconds() {
        let mut gun = Revolver::default();
        assert_eq!(gun.rounds, 5, "six minus Farach's");
        let far = [Revolver::RANGE + 0.1, 40.0];
        assert_eq!(gun.fire(&far, DT), None, "nobody within range");
        let near = [12.0, 30.0, 8.0];
        assert_eq!(gun.fire(&near, DT), Some(2), "the nearest");
        let mut waited = 0.0;
        while gun.fire(&near, DT).is_none() {
            waited += DT;
        }
        assert!(
            (waited - Revolver::COCK_SECONDS).abs() < 2.0 * DT,
            "{waited} s"
        );
        while gun.rounds > 0 {
            gun.fire(&near, DT);
        }
        assert_eq!(gun.fire(&near, 10.0), None, "no rounds left");
    }

    #[test]
    fn only_without_rounds_does_a_milico_five_metres_away_catch_them() {
        let mut gun = Revolver::default();
        let beside = [ALONGSIDE, 50.0];
        assert!(!gun.caught(&beside));
        gun.rounds = 0;
        assert!(gun.caught(&beside));
        assert!(!gun.caught(&[ALONGSIDE + 0.1]));
    }

    #[test]
    fn a_shied_chaser_reins_in_and_falls_behind() {
        let world = World::default();
        let (mut chaser, mut quarry) = (east(&world, 0.0, 0.0), east(&world, 10.0, 0.0));
        let p = chaser_params();
        chaser.speed = p.gallop_max;
        quarry.speed = RiderParams::default().gallop_max;
        for _ in 0..(Revolver::SHIED_SECONDS / DT) as usize {
            chaser.step(rein_in(&chaser), &p, &world, DT);
            quarry.step(GALLOP, &RiderParams::default(), &world, DT);
        }
        assert!(chaser.speed.abs() < 0.5, "pulled up: {} m/s", chaser.speed);
        assert!(quarry.position.x - chaser.position.x > 40.0);
    }
}
