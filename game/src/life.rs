//! Town life: townsfolk walking the streets, each town's train, and the
//! horse tied in front of each general store.
//!
//! Logic lives in `pampa-core`; this module steps it once per frame, before
//! the player, lets the rider shove walkers aside after the player moved,
//! and copies the results onto entities that render draws.
//!
//! Every town has its own crowd, seeded from the town's seed. Only the towns
//! near the player ([`ActiveTowns`]) have their people and tied horse
//! spawned and simulated; without [`ActiveTowns`] (no town streaming) every
//! crowd is. Trains run everywhere: each is a pure function of its clock.

use std::collections::BTreeSet;

use bevy::prelude::*;
use pampa_core::townsfolk::TOWNSFOLK_COUNT;
use pampa_core::{MAX_STEP, TownLayout, Townsfolk};
use pampa_render::{
    ActiveTowns, CharacterVisual, MapIcon, SheetKind, VisualPose, WorldTowns, WorldTrains,
    world_from_ground,
};

use crate::player::{DrivePlayer, GameWorld, Player, drive_player};

/// Mixed with each town's seed for where its people start and wander.
const TOWNSFOLK_SEED: u32 = 1927;

/// Everyone walking around each town, by town index.
#[derive(Resource, Debug, Clone, Default)]
pub struct Crowds(pub Vec<Townsfolk>);

impl Crowds {
    /// A crowd for every town, seeded from the town's seed.
    pub fn for_towns(towns: &[TownLayout]) -> Self {
        Self(
            towns
                .iter()
                .map(|t| Townsfolk::new(t, TOWNSFOLK_COUNT, crowd_seed(t.seed)))
                .collect(),
        )
    }
}

/// The crowd seed of a town with seed `seed`.
pub fn crowd_seed(seed: u32) -> u32 {
    TOWNSFOLK_SEED ^ seed.wrapping_mul(0x9e37_79b9)
}

/// Entity drawing person `index` of town `town`'s crowd.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walker {
    pub town: usize,
    pub index: usize,
}

/// The horse tied at town `.0`'s hitching rail.
#[derive(Component, Debug, Clone, Copy)]
pub struct TetheredHorse(pub usize);

/// Towns whose people and horse are spawned.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Populated(pub BTreeSet<usize>);

/// Systems that advance the towns around the player; they run before it so
/// the rider collides with this frame's trains.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimulateTown;

pub struct TownLifePlugin;

impl Plugin for TownLifePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Shoved>()
            .init_resource::<Populated>()
            .configure_sets(Update, SimulateTown.before(DrivePlayer))
            .add_systems(
                Update,
                (populate_towns, run_trains, walk_crowds)
                    .chain()
                    .in_set(SimulateTown),
            )
            .add_systems(
                Update,
                (jostle_crowd, place_walkers)
                    .chain()
                    .in_set(DrivePlayer)
                    .after(drive_player),
            );
    }
}

/// Indices of the crowds to simulate: the active towns, or every crowd when
/// towns are not streamed.
fn live_towns(crowds: &Crowds, active: Option<&ActiveTowns>) -> Vec<usize> {
    match active {
        Some(active) => active.0.active().filter(|&i| i < crowds.0.len()).collect(),
        None => (0..crowds.0.len()).collect(),
    }
}

/// Resources [`populate_towns`] reads.
type TownInputs<'a> = (
    Option<Res<'a, Crowds>>,
    Option<Res<'a, WorldTowns>>,
    Option<Res<'a, ActiveTowns>>,
);

/// Spawns the people and tied horse of towns that just became active, and
/// despawns those of towns left behind.
fn populate_towns(
    mut commands: Commands,
    (crowds, towns, active): TownInputs,
    world: Res<GameWorld>,
    mut populated: ResMut<Populated>,
    residents: Query<(Entity, Option<&Walker>, Option<&TetheredHorse>)>,
) {
    let Some(crowds) = crowds else {
        return;
    };
    let wanted: BTreeSet<usize> = live_towns(&crowds, active.as_deref()).into_iter().collect();
    for (entity, walker, horse) in &residents {
        let town = walker.map(|w| w.town).or(horse.map(|h| h.0));
        if town.is_some_and(|t| !wanted.contains(&t)) {
            commands.entity(entity).despawn();
        }
    }
    populated.0.retain(|t| wanted.contains(t));
    for &town in &wanted {
        if !populated.0.insert(town) {
            continue;
        }
        spawn_walkers(&mut commands, &crowds.0[town], town, &world);
        if let Some(layout) = towns.as_ref().and_then(|t| t.0.get(town)) {
            spawn_tethered_horse(&mut commands, layout, town, &world);
        }
    }
}

fn spawn_walkers(commands: &mut Commands, crowd: &Townsfolk, town: usize, world: &GameWorld) {
    for (index, person) in crowd.people.iter().enumerate() {
        let height = world.0.terrain.height(person.position.x, person.position.y);
        commands.spawn((
            Name::new(format!("Townsperson {town}.{index}")),
            Walker { town, index },
            MapIcon::Walker,
            CharacterVisual::Billboard(SheetKind::person(person.look)),
            VisualPose {
                heading: person.heading,
                animation: person.animation(),
                speed: person.speed,
            },
            Transform::from_translation(world_from_ground(person.position, height)),
        ));
    }
}

fn spawn_tethered_horse(
    commands: &mut Commands,
    layout: &TownLayout,
    town: usize,
    world: &GameWorld,
) {
    let horse = layout.hitching_post.horse;
    let height = world.0.terrain.height(horse.position.x, horse.position.y);
    commands.spawn((
        Name::new(format!("Tethered horse {town}")),
        TetheredHorse(town),
        CharacterVisual::Billboard(SheetKind::Horse),
        VisualPose {
            heading: horse.heading,
            ..default()
        },
        Transform::from_translation(world_from_ground(horse.position, height)),
    ));
}

/// Advances every train's schedule and hands the cars to the logic world
/// as moving obstacles.
fn run_trains(time: Res<Time>, trains: Option<ResMut<WorldTrains>>, mut world: ResMut<GameWorld>) {
    let Some(mut trains) = trains else {
        return;
    };
    let dt = time.delta_secs().min(MAX_STEP);
    let mut moving = Vec::new();
    for train in trains.0.iter_mut().flatten() {
        train.step(dt);
        moving.extend(train.obstacles());
    }
    world.0.moving = moving;
}

/// Steps the live crowds, walking around the rider where it stood last
/// frame.
fn walk_crowds(
    time: Res<Time>,
    crowds: Option<ResMut<Crowds>>,
    active: Option<Res<ActiveTowns>>,
    world: Res<GameWorld>,
    players: Query<&Player>,
) {
    let Some(mut crowds) = crowds else {
        return;
    };
    let riders: Vec<_> = players.iter().map(|p| p.rider.body()).collect();
    let dt = time.delta_secs().min(MAX_STEP);
    for town in live_towns(&crowds, active.as_deref()) {
        crowds.0[town].step_around(&world.0, &riders, dt);
    }
}

/// The rider shoved these walkers (indices into town `town`'s crowd) while
/// moving at `speed` (m/s, signed).
#[derive(Message, Debug, Clone, PartialEq)]
pub struct Shoved {
    pub town: usize,
    pub walkers: Vec<usize>,
    pub speed: f32,
}

/// The rider barges through: walkers in its way are shoved aside, and
/// anyone pinned against a wall stops the rider instead.
pub(crate) fn jostle_crowd(
    crowds: Option<ResMut<Crowds>>,
    active: Option<Res<ActiveTowns>>,
    world: Res<GameWorld>,
    mut players: Query<(&mut Player, &mut Transform)>,
    mut shoved: MessageWriter<Shoved>,
) {
    let Some(mut crowds) = crowds else {
        return;
    };
    let live = live_towns(&crowds, active.as_deref());
    for (mut player, mut transform) in &mut players {
        let rider = &mut player.rider;
        let mut bodies = Vec::new();
        for &town in &live {
            let crowd = &mut crowds.0[town];
            let walkers = crowd.make_way(&world.0, rider.body());
            if !walkers.is_empty() {
                // The speed of the impact, before a pinned walker stops the rider.
                shoved.write(Shoved {
                    town,
                    walkers,
                    speed: rider.speed,
                });
            }
            bodies.extend(crowd.bodies());
        }
        rider.keep_clear_of(&world.0, &bodies);
        transform.translation = world_from_ground(rider.position, rider.height);
    }
}

/// Copies the crowds' logic state onto the walker entities.
fn place_walkers(
    crowds: Option<Res<Crowds>>,
    world: Res<GameWorld>,
    mut walkers: Query<(&Walker, &mut Transform, &mut VisualPose), Without<Player>>,
) {
    let Some(crowds) = crowds else {
        return;
    };
    for (walker, mut transform, mut pose) in &mut walkers {
        let Some(person) = crowds
            .0
            .get(walker.town)
            .and_then(|c| c.people.get(walker.index))
        else {
            continue;
        };
        let height = world.0.terrain.height(person.position.x, person.position.y);
        transform.translation = world_from_ground(person.position, height);
        *pose = VisualPose {
            heading: person.heading,
            animation: person.animation(),
            speed: person.speed,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::PlayerPlugin;
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{
        Terrain, TownActivation, TownConfig, Train, TrainSchedule, Vec2, World, WorldConfig,
    };
    use std::time::Duration;

    fn far_town() -> TownLayout {
        TownLayout::generate(&TownConfig {
            name: "Lejos".into(),
            origin: Vec2::new(3000.0, -2000.0),
            seed: 11,
            ..TownConfig::castex()
        })
        .unwrap()
    }

    fn app_with(towns: Vec<TownLayout>, people: usize) -> App {
        let world = World::with_towns(WorldConfig::default(), Terrain::default(), &towns);
        let crowds = towns
            .iter()
            .map(|t| Townsfolk::new(t, people, crowd_seed(t.seed)))
            .collect();
        let trains = towns
            .iter()
            .map(|t| Train::for_town(t, TrainSchedule::default()))
            .collect();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Crowds(crowds))
            .insert_resource(WorldTrains(trains))
            .insert_resource(WorldTowns(towns))
            .insert_resource(GameWorld(world))
            .add_plugins((PlayerPlugin, TownLifePlugin));
        app
    }

    fn town_app() -> App {
        app_with(vec![TownLayout::castex_inspired()], 6)
    }

    fn walkers(app: &mut App) -> Vec<(Walker, Transform, VisualPose)> {
        let mut query = app
            .world_mut()
            .query::<(&Walker, &Transform, &VisualPose)>();
        query
            .iter(app.world())
            .map(|(w, t, p)| (*w, *t, *p))
            .collect()
    }

    #[test]
    fn walkers_follow_the_crowd_logic() {
        let mut app = town_app();
        for _ in 0..40 {
            app.update();
        }
        let crowd = app.world().resource::<Crowds>().0[0].clone();
        let terrain = app.world().resource::<GameWorld>().0.terrain.clone();
        let walkers = walkers(&mut app);
        assert_eq!(walkers.len(), crowd.people.len());
        for (walker, transform, pose) in walkers {
            assert_eq!(walker.town, 0);
            let person = &crowd.people[walker.index];
            let height = terrain.height(person.position.x, person.position.y);
            assert_eq!(
                transform.translation,
                world_from_ground(person.position, height)
            );
            assert_eq!(pose.heading, person.heading);
            assert_eq!(pose.animation, person.animation());
        }
    }

    #[test]
    fn only_active_towns_have_people_and_they_move() {
        let towns = vec![TownLayout::castex_inspired(), far_town()];
        let mut app = app_with(towns.clone(), 5);
        let mut activation = TownActivation::default();
        activation.update(&towns, Vec2::ZERO);
        app.insert_resource(ActiveTowns(activation));
        let far_start = app.world().resource::<Crowds>().0[1].clone();
        for _ in 0..20 {
            app.update();
        }
        let towns_of = |app: &mut App| {
            let mut v: Vec<usize> = walkers(app).iter().map(|w| w.0.town).collect();
            v.dedup();
            v
        };
        assert_eq!(towns_of(&mut app), vec![0]);
        assert_eq!(walkers(&mut app).len(), 5);
        // The far crowd stands frozen while nobody is there.
        assert_eq!(app.world().resource::<Crowds>().0[1], far_start);
        let horses = |app: &mut App| {
            let mut q = app.world_mut().query::<&TetheredHorse>();
            q.iter(app.world()).map(|h| h.0).collect::<Vec<_>>()
        };
        assert_eq!(horses(&mut app), vec![0]);

        // Ride to the far town: Castex empties, the far town fills up.
        app.world_mut()
            .resource_mut::<ActiveTowns>()
            .0
            .update(&towns, Vec2::new(3000.0, -2000.0));
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(towns_of(&mut app), vec![1]);
        assert_eq!(walkers(&mut app).len(), 5);
        assert_eq!(horses(&mut app), vec![1]);
        assert_ne!(app.world().resource::<Crowds>().0[1], far_start);
        assert_eq!(
            app.world().resource::<Populated>().0,
            BTreeSet::from([1usize])
        );
    }

    #[test]
    fn every_town_gets_its_own_crowd_from_its_seed() {
        let towns = vec![TownLayout::castex_inspired(), far_town()];
        let crowds = Crowds::for_towns(&towns);
        assert_eq!(crowds.0.len(), 2);
        assert_eq!(crowds.0[0].people.len(), TOWNSFOLK_COUNT);
        let far = towns[1].grid();
        assert!(
            crowds.0[1]
                .people
                .iter()
                .all(|p| far.contains_point(p.position))
        );
        assert_ne!(crowd_seed(towns[0].seed), crowd_seed(towns[1].seed));
    }

    #[test]
    fn the_trains_move_and_become_obstacles_for_the_rider() {
        let mut app = app_with(vec![TownLayout::castex_inspired(), far_town()], 2);
        app.update();
        let first: Vec<f32> = app
            .world()
            .resource::<WorldTrains>()
            .0
            .iter()
            .map(|t| t.as_ref().unwrap().state().front)
            .collect();
        for _ in 0..20 {
            app.update();
        }
        let trains: Vec<Train> = app
            .world()
            .resource::<WorldTrains>()
            .0
            .iter()
            .map(|t| t.clone().unwrap())
            .collect();
        for (train, first) in trains.iter().zip(first) {
            assert!(train.state().front > first, "every schedule advances");
            assert!(!train.obstacles().is_empty());
        }
        let expected: Vec<_> = trains.iter().flat_map(Train::obstacles).collect();
        assert_eq!(app.world().resource::<GameWorld>().0.moving, expected);
    }

    #[test]
    fn a_saddled_horse_stands_tied_at_the_rail() {
        let mut app = town_app();
        app.update();
        let horse = app.world().resource::<WorldTowns>().0[0]
            .hitching_post
            .horse;
        let mut query = app
            .world_mut()
            .query_filtered::<(&Transform, &CharacterVisual, &VisualPose), With<TetheredHorse>>();
        let (transform, visual, pose) = query.single(app.world()).unwrap();
        assert_eq!(*visual, CharacterVisual::Billboard(SheetKind::Horse));
        assert_eq!(pose.heading, horse.heading);
        assert_eq!(transform.translation.x, horse.position.x);
        assert_eq!(transform.translation.z, -horse.position.y);
        // The rider cannot pass through it: it is a static obstacle.
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
    }

    #[test]
    fn the_rider_shoves_walkers_aside() {
        let mut app = town_app();
        // Spawn the rider right on top of the first walker, galloping.
        let at = app.world().resource::<Crowds>().0[0].people[0].position;
        app.insert_resource(crate::player::PlayerSpawn {
            position: at,
            heading: pampa_core::Heading::from_radians(0.0),
        });
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
        }
        for frame in 0..200 {
            app.update();
            let rider = {
                let mut q = app.world_mut().query::<&crate::player::Player>();
                q.single(app.world()).unwrap().rider
            };
            let crowd = app.world().resource::<Crowds>().0[0].clone();
            for p in &crowd.people {
                let gap = p.position.distance(rider.position);
                assert!(
                    gap >= p.collider.radius + rider.collider.radius - 1e-3,
                    "frame {frame}: {p:?} inside the rider at {:?}",
                    rider.position
                );
            }
            let mut q = app
                .world_mut()
                .query_filtered::<&Transform, With<crate::player::Player>>();
            let t = q.single(app.world()).unwrap();
            assert_eq!(
                t.translation,
                world_from_ground(rider.position, rider.height)
            );
        }
    }
}
