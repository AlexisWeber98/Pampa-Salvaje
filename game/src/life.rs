//! Town life: townsfolk walking the streets, the scheduled train, and the
//! horse tied in front of the general store.
//!
//! Logic lives in `pampa-core`; this module steps it once per frame, before
//! the player, and copies the results onto entities that render draws.

use bevy::prelude::*;
use pampa_core::{MAX_STEP, Townsfolk};
use pampa_render::{
    CharacterVisual, SheetKind, VisualPose, WorldTown, WorldTrain, world_from_ground,
};

use crate::player::{DrivePlayer, GameWorld};

/// Everyone walking around town.
#[derive(Resource, Debug, Clone)]
pub struct Crowd(pub Townsfolk);

/// Entity drawing the person at this index of the [`Crowd`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Walker(pub usize);

/// The horse tied at the hitching rail.
#[derive(Component, Debug, Clone, Copy)]
pub struct TetheredHorse;

/// Systems that advance the town around the player; they run before it so
/// the rider collides with this frame's train.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimulateTown;

pub struct TownLifePlugin;

impl Plugin for TownLifePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(Update, SimulateTown.before(DrivePlayer))
            .add_systems(Startup, (spawn_walkers, spawn_tethered_horse))
            .add_systems(Update, (run_train, walk_crowd).chain().in_set(SimulateTown));
    }
}

fn spawn_walkers(mut commands: Commands, crowd: Option<Res<Crowd>>, world: Res<GameWorld>) {
    let Some(crowd) = crowd else {
        return;
    };
    for (index, person) in crowd.0.people.iter().enumerate() {
        let height = world.0.terrain.height(person.position.x, person.position.y);
        commands.spawn((
            Name::new(format!("Townsperson {index}")),
            Walker(index),
            CharacterVisual::Billboard(SheetKind::person(person.look)),
            VisualPose {
                heading: person.heading,
                animation: person.animation(),
            },
            Transform::from_translation(world_from_ground(person.position, height)),
        ));
    }
}

fn spawn_tethered_horse(
    mut commands: Commands,
    town: Option<Res<WorldTown>>,
    world: Res<GameWorld>,
) {
    let Some(town) = town else {
        return;
    };
    let horse = town.0.hitching_post.horse;
    let height = world.0.terrain.height(horse.position.x, horse.position.y);
    commands.spawn((
        Name::new("Tethered horse"),
        TetheredHorse,
        CharacterVisual::Billboard(SheetKind::Horse),
        VisualPose {
            heading: horse.heading,
            animation: Default::default(),
        },
        Transform::from_translation(world_from_ground(horse.position, height)),
    ));
}

/// Advances the train's schedule and hands its cars to the logic world as
/// moving obstacles.
fn run_train(time: Res<Time>, train: Option<ResMut<WorldTrain>>, mut world: ResMut<GameWorld>) {
    let Some(mut train) = train else {
        return;
    };
    train.0.step(time.delta_secs().min(MAX_STEP));
    world.0.moving = train.0.obstacles();
}

fn walk_crowd(
    time: Res<Time>,
    crowd: Option<ResMut<Crowd>>,
    world: Res<GameWorld>,
    mut walkers: Query<(&Walker, &mut Transform, &mut VisualPose)>,
) {
    let Some(mut crowd) = crowd else {
        return;
    };
    crowd.0.step(&world.0, time.delta_secs().min(MAX_STEP));
    for (walker, mut transform, mut pose) in &mut walkers {
        let Some(person) = crowd.0.people.get(walker.0) else {
            continue;
        };
        let height = world.0.terrain.height(person.position.x, person.position.y);
        transform.translation = world_from_ground(person.position, height);
        *pose = VisualPose {
            heading: person.heading,
            animation: person.animation(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::PlayerPlugin;
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Terrain, TownLayout, Train, TrainSchedule, World};
    use std::time::Duration;

    fn town_app() -> App {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Crowd(Townsfolk::new(&town, 6, 1)))
            .insert_resource(WorldTrain(Train::for_town(&town, TrainSchedule::default())))
            .insert_resource(WorldTown(town))
            .insert_resource(GameWorld(world))
            .add_plugins((PlayerPlugin, TownLifePlugin));
        app
    }

    #[test]
    fn walkers_follow_the_crowd_logic() {
        let mut app = town_app();
        for _ in 0..40 {
            app.update();
        }
        let crowd = app.world().resource::<Crowd>().0.clone();
        let terrain = app.world().resource::<GameWorld>().0.terrain;
        let mut query = app
            .world_mut()
            .query::<(&Walker, &Transform, &VisualPose)>();
        let walkers: Vec<_> = query
            .iter(app.world())
            .map(|(w, t, p)| (*w, *t, *p))
            .collect();
        assert_eq!(walkers.len(), crowd.people.len());
        for (walker, transform, pose) in walkers {
            let person = &crowd.people[walker.0];
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
    fn the_train_moves_and_becomes_an_obstacle_for_the_rider() {
        let mut app = town_app();
        app.update();
        let first = app.world().resource::<WorldTrain>().0.state().front;
        for _ in 0..20 {
            app.update();
        }
        let train = app.world().resource::<WorldTrain>().0.clone();
        assert!(train.state().front > first, "the schedule advances");
        assert_eq!(
            app.world().resource::<GameWorld>().0.moving,
            train.obstacles()
        );
        assert!(!train.obstacles().is_empty());
    }

    #[test]
    fn a_saddled_horse_stands_tied_at_the_rail() {
        let mut app = town_app();
        app.update();
        let horse = app.world().resource::<WorldTown>().0.hitching_post.horse;
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
}
