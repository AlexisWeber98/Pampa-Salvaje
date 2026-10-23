//! Town life: the game clock, the line train, townsfolk walking the
//! streets, the horse tied in front of each general store, and the people
//! of the errand standing in their towns.
//!
//! Logic lives in `pampa-core`; this module steps it once per frame, before
//! the player, lets the rider shove walkers aside after the player moved,
//! and copies the results onto entities that render draws.
//!
//! Every town has its own crowd, seeded from the town's seed. Only the towns
//! near the player ([`ActiveTowns`]) have their people and tied horse
//! spawned and simulated; without [`ActiveTowns`] (no town streaming) every
//! crowd is. The errand's people ([`Cast`]) come and go with their towns
//! the same way, and a horse led away ([`LedAway`]) is not tied up again. The clock ticks every frame and the one line train follows
//! it everywhere: departures come from the clock, running is in real
//! seconds, and its cars are always moving obstacles (eight cars: cheap).

use std::collections::BTreeSet;

use bevy::prelude::*;
use pampa_core::{
    GameClock, LineTrain, MAX_STEP, Npc, RailLine, Ride, Run, Standing, Timetable, TownLayout,
    Townsfolk, Travel, opening,
};
use pampa_render::{
    ActiveTowns, CharacterVisual, Coat, FigureKind, MapIcon, VisualPose, WorldClock, WorldTowns,
    WorldTrain, world_from_ground,
};

use crate::player::{DrivePlayer, GameWorld, Player, drive_player};
use crate::ride::OnTrain;

/// The clock, the line train and the player's ride as the game begins
/// (the opening): the protagonist comes from General Pico, so he rides the
/// morning (up) train from Pico, a little before it pulls into the first
/// town's station ([`pampa_core::OPENING_LEAD`] real seconds), and is put
/// off there. The clock runs at the onboard rate and reaches the
/// timetabled arrival, in the mid-morning, as the train stops. Without a
/// train (or a station in the first town) there is no ride, and the clock
/// starts at the morning departure time from Pico.
pub fn start_of_day(
    line: &RailLine,
    towns: &[TownLayout],
) -> (GameClock, Option<LineTrain>, Option<Ride>) {
    let mut train = LineTrain::new(line, towns, Timetable::default());
    let start = train.as_mut().and_then(|t| opening(t, 0, Run::Up));
    match start {
        Some((clock, ride)) => {
            // Seated by the window on the Monte Nievas platform's side.
            let ride = train.as_ref().map(|t| ride.facing_platform(t, towns));
            (clock, train, ride)
        }
        None => (GameClock::at(Timetable::default().depart_pico), train, None),
    }
}

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
                .map(|t| Townsfolk::new(t, t.townsfolk_count(), crowd_seed(t.seed)))
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

/// A person of the errand standing in town `town`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandingNpc {
    pub town: usize,
    pub npc: Npc,
}

/// The errand's people and where they stand, spawned with their towns.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Cast(pub Vec<(Npc, Standing)>);

/// Towns whose tied horse was led away this session: it is not tied up
/// there again.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct LedAway(pub BTreeSet<usize>);

/// The figure of an errand person.
pub fn npc_figure(npc: Npc) -> FigureKind {
    match npc {
        Npc::Hidalgo => FigureKind::Hidalgo,
        Npc::Farach => FigureKind::Farach,
    }
}

/// Towns whose people and horse are spawned.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Populated(pub BTreeSet<usize>);

/// Systems that advance the clock, the train and the towns around the
/// player; they run before it so the rider collides with this frame's
/// train.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimulateTown;

pub struct TownLifePlugin;

impl Plugin for TownLifePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Shoved>()
            .init_resource::<Populated>()
            .init_resource::<Cast>()
            .init_resource::<LedAway>()
            .configure_sets(Update, SimulateTown.before(DrivePlayer))
            .add_systems(Startup, report_train)
            .add_systems(
                Update,
                (populate_towns, run_clock_and_train, walk_crowds)
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

/// Warns about stations the line train cannot stop alongside (too near a
/// tip of the line).
fn report_train(train: Option<Res<WorldTrain>>, towns: Option<Res<WorldTowns>>) {
    let Some(train) = train.as_ref().and_then(|t| t.0.as_ref()) else {
        return;
    };
    for station in train.misplaced_stops() {
        let town = train.stations()[station].town;
        let name = towns
            .as_ref()
            .and_then(|t| t.0.get(town))
            .map_or("?", |t| t.name.as_str());
        warn!("line train: the coaches cannot stop alongside the station of `{name}`");
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
    Res<'a, Cast>,
    Res<'a, LedAway>,
);

/// Everyone living in a town: walkers, the tied horse and errand people.
type Residents<'a> = (
    Entity,
    Option<&'a Walker>,
    Option<&'a TetheredHorse>,
    Option<&'a StandingNpc>,
);

/// Spawns the people and tied horse of towns that just became active, and
/// despawns those of towns left behind.
fn populate_towns(
    mut commands: Commands,
    (crowds, towns, active, cast, led_away): TownInputs,
    world: Res<GameWorld>,
    mut populated: ResMut<Populated>,
    residents: Query<Residents>,
) {
    let Some(crowds) = crowds else {
        return;
    };
    let wanted: BTreeSet<usize> = live_towns(&crowds, active.as_deref()).into_iter().collect();
    for (entity, walker, horse, npc) in &residents {
        let town = walker
            .map(|w| w.town)
            .or(horse.map(|h| h.0))
            .or(npc.map(|n| n.town));
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
        if let Some(layout) = towns.as_ref().and_then(|t| t.0.get(town))
            && !led_away.0.contains(&town)
        {
            let coat = tethered_coat(layout, &cast, town);
            spawn_tethered_horse(&mut commands, layout, town, coat, &world);
        }
        for &(npc, standing) in cast.0.iter().filter(|(_, s)| s.town == town) {
            spawn_standing_npc(&mut commands, npc, standing, &world);
        }
    }
}

/// An errand person standing still where the errand put them.
fn spawn_standing_npc(commands: &mut Commands, npc: Npc, standing: Standing, world: &GameWorld) {
    let at = standing.position;
    let height = world.0.ground_height(at);
    commands.spawn((
        Name::new(format!("{npc:?}")),
        StandingNpc {
            town: standing.town,
            npc,
        },
        CharacterVisual::Figure(npc_figure(npc)),
        VisualPose {
            heading: standing.heading,
            ..default()
        },
        Transform::from_translation(world_from_ground(at, height)),
    ));
}

fn spawn_walkers(commands: &mut Commands, crowd: &Townsfolk, town: usize, world: &GameWorld) {
    for (index, person) in crowd.people.iter().enumerate() {
        let height = world.0.ground_height(person.position);
        commands.spawn((
            Name::new(format!("Townsperson {town}.{index}")),
            Walker { town, index },
            MapIcon::Walker,
            CharacterVisual::Figure(FigureKind::townsfolk(person.look)),
            VisualPose {
                heading: person.heading,
                animation: person.animation(),
                speed: person.speed,
            },
            Transform::from_translation(world_from_ground(person.position, height)),
        ));
    }
}

/// The coat of the horse tied at `town`'s store: Don Hidalgo's bayo in his
/// town (the horse he lends), else one seeded from the town.
pub fn tethered_coat(layout: &TownLayout, cast: &Cast, town: usize) -> Coat {
    let hidalgo = cast
        .0
        .iter()
        .any(|(npc, s)| *npc == Npc::Hidalgo && s.town == town);
    if hidalgo {
        Coat::HIDALGO
    } else {
        Coat::seeded(layout.seed ^ 0x4f52_5345)
    }
}

fn spawn_tethered_horse(
    commands: &mut Commands,
    layout: &TownLayout,
    town: usize,
    coat: Coat,
    world: &GameWorld,
) {
    let horse = layout.hitching_post.horse;
    let height = world.0.ground_height(horse.position);
    commands.spawn((
        Name::new(format!("Tethered horse {town}")),
        TetheredHorse(town),
        crate::errand::Mountable(horse),
        CharacterVisual::Horse(coat),
        VisualPose {
            heading: horse.heading,
            ..default()
        },
        Transform::from_translation(world_from_ground(horse.position, height)),
    ));
}

/// Ticks the game clock, lets the line train follow it and hands the
/// train's cars to the logic world as moving obstacles.
fn run_clock_and_train(
    time: Res<Time>,
    clock: Option<ResMut<WorldClock>>,
    train: Option<ResMut<WorldTrain>>,
    mut world: ResMut<GameWorld>,
) {
    let Some(mut clock) = clock else {
        return;
    };
    let before = clock.0.elapsed();
    // Capped like the rider's step, so a frame hitch cannot jump the train
    // tens of meters onto a rider that was clear the frame before.
    let game = clock.0.tick(time.delta_secs().min(MAX_STEP));
    let mut train = train;
    let Some(train) = train.as_mut().and_then(|t| t.0.as_mut()) else {
        world.0.moving.clear();
        return;
    };
    train.advance(before, game, clock.0.rate());
    world.0.moving = train.obstacles();
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

/// The player barges through: walkers in the way are shoved aside, and
/// anyone pinned against a wall stops the player instead. Only a rider's
/// shoves are reported ([`Shoved`]): people make way for somebody on foot
/// without complaint.
pub(crate) fn jostle_crowd(
    crowds: Option<ResMut<Crowds>>,
    active: Option<Res<ActiveTowns>>,
    world: Res<GameWorld>,
    mut players: Query<(&mut Player, &mut Transform), Without<OnTrain>>,
    mut shoved: MessageWriter<Shoved>,
) {
    let Some(mut crowds) = crowds else {
        return;
    };
    let live = live_towns(&crowds, active.as_deref());
    for (mut player, mut transform) in &mut players {
        let rider = &mut player.rider;
        let mut bodies = Vec::new();
        // Townsfolk only complain about the horse.
        let riding = rider.travel() == Travel::Riding;
        for &town in &live {
            let crowd = &mut crowds.0[town];
            let walkers = crowd.make_way(&world.0, rider.body());
            if riding && !walkers.is_empty() {
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
        let height = world.0.ground_height(person.position);
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
        BASE_RATE, LinePhase, ONBOARD_MULTIPLIER, OPENING_LEAD, Terrain, TimeOfDay, TownActivation,
        TownConfig, Vec2, World, WorldConfig,
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
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Crowds(crowds))
            .insert_resource(WorldClock::default())
            .insert_resource(WorldTrain(None))
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
        let world = app.world().resource::<GameWorld>().0.clone();
        let walkers = walkers(&mut app);
        assert_eq!(walkers.len(), crowd.people.len());
        let mut visuals = app
            .world_mut()
            .query_filtered::<(&Walker, &CharacterVisual), With<Walker>>();
        for (walker, visual) in visuals.iter(app.world()) {
            let look = crowd.people[walker.index].look;
            assert_eq!(
                *visual,
                CharacterVisual::Figure(FigureKind::townsfolk(look))
            );
        }
        for (walker, transform, pose) in walkers {
            assert_eq!(walker.town, 0);
            let person = &crowd.people[walker.index];
            let height = world.ground_height(person.position);
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
        assert_eq!(
            crowds.0[0].people.len(),
            pampa_core::townsfolk::TOWNSFOLK_COUNT
        );
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
    fn bigger_towns_get_bigger_crowds() {
        let pico = TownLayout::generate(&TownConfig {
            name: "General Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        let towns = vec![TownLayout::castex_inspired(), pico];
        let crowds = Crowds::for_towns(&towns);
        assert_eq!(crowds.0[0].people.len(), 16);
        assert_eq!(crowds.0[1].people.len(), 51);
        // A small town's crowd is the one it always had.
        let small = Townsfolk::new(&towns[0], 16, crowd_seed(towns[0].seed));
        assert_eq!(crowds.0[0], small);
    }

    fn shipped() -> crate::towns::LoadedWorld {
        crate::towns::load_world(&pampa_content::asset_path(pampa_content::TOWNS_FILE))
    }

    /// The shipped world with its clock at `clock` and the line train
    /// jumped there from midnight.
    fn shipped_app(clock: Option<GameClock>) -> App {
        let loaded = shipped();
        let (start, mut train, _) = start_of_day(&loaded.line, &loaded.towns);
        let clock = clock.unwrap_or(start);
        if clock != start {
            train = LineTrain::new(&loaded.line, &loaded.towns, Timetable::default());
            if let Some(train) = &mut train {
                train.advance(0.0, clock.elapsed(), BASE_RATE);
            }
        }
        let mut app = app_with(loaded.towns, 1);
        app.insert_resource(WorldClock(clock))
            .insert_resource(WorldTrain(train));
        app
    }

    fn train(app: &App) -> LineTrain {
        app.world()
            .resource::<WorldTrain>()
            .0
            .clone()
            .expect("a train")
    }

    #[test]
    fn the_game_starts_aboard_the_morning_train_from_pico_pulling_into_the_first_town() {
        let loaded = shipped();
        assert_eq!(loaded.towns[0].name, "Monte Nievas");
        let (clock, train, ride) = start_of_day(&loaded.line, &loaded.towns);
        let train = train.expect("the shipped line has a train");
        let s = train.state();
        assert_eq!(s.phase, LinePhase::Running);
        // Pulling into Monte Nievas, a little under a minute out.
        let nievas = train.station_of_town(0).unwrap();
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, nievas);
        assert!((left - OPENING_LEAD).abs() < 1e-9);
        // Seated in the first coach, to get off at Monte Nievas.
        let ride = ride.expect("the opening ride");
        assert_eq!(ride.alight_at, Some(nievas));
        assert_eq!(ride.car, 1);
        // The up run, from Pico after its 07:00 departure, on to Castex.
        assert_eq!(s.run, Run::Up);
        let pico = train.stations().len() - 1;
        assert_eq!(
            loaded.towns[train.stations()[pico].town].name,
            "General Pico"
        );
        // The locomotive faces Castex: up the line, against its direction.
        let loco = train.placed_cars()[0];
        assert!(loco.direction.dot(train.path().direction_at(s.front)) < -0.9);
        assert_eq!(clock.day(), 0);
        // Mid-morning, in full daylight, at the onboard rate: ten game
        // minutes (50 real seconds at 12×) before the timetabled arrival
        // (10:53:31 since the Metileo stop, 2026-10-23; 10:14:45 before).
        assert_eq!(clock.multiplier, ONBOARD_MULTIPLIER);
        let arrival = train.arrival_time_at(Run::Up, nievas).unwrap();
        let (morning, noon) = (TimeOfDay::hm(9, 30), TimeOfDay::hm(11, 30));
        assert!(
            (morning.seconds()..noon.seconds()).contains(&arrival.seconds()),
            "{arrival}"
        );
        let lead = OPENING_LEAD * ONBOARD_MULTIPLIER * BASE_RATE;
        assert!((clock.elapsed() + lead - arrival.seconds()).abs() < 1e-6);
    }

    #[test]
    fn the_clock_ticks_and_the_train_stays_an_obstacle() {
        // Half way through the morning dwell at Monte Nievas, on foot.
        let loaded = shipped();
        let timetabled =
            LineTrain::new(&loaded.line, &loaded.towns, Timetable::default()).expect("a train");
        let dwell = timetabled
            .stop_at(0, Run::Up)
            .expect("a stop at Monte Nievas");
        let mut app = shipped_app(Some(GameClock::at(dwell)));
        app.update();
        let start = app.world().resource::<WorldClock>().0;
        for _ in 0..20 {
            app.update();
        }
        // 20 frames of 50 ms at 30×: 30 game seconds.
        let clock = app.world().resource::<WorldClock>().0;
        assert!((clock.elapsed() - start.elapsed() - 30.0).abs() < 1e-3);
        let train = train(&app);
        assert_eq!(
            train.state().phase,
            LinePhase::Dwelling,
            "still at the stop"
        );
        assert!(!train.obstacles().is_empty());
        assert_eq!(
            app.world().resource::<GameWorld>().0.moving,
            train.obstacles()
        );
    }

    #[test]
    fn a_frame_hitch_advances_the_clock_and_train_by_one_capped_step() {
        let mut app = shipped_app(Some(GameClock::at(TimeOfDay::hm(7, 10))));
        app.update();
        let start = app.world().resource::<WorldClock>().0;
        let before = train(&app).state();
        assert_eq!(before.phase, LinePhase::Running);
        // A two-second stall (window drag, load hitch) in a single frame.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(2)));
        app.update();
        let clock = app.world().resource::<WorldClock>().0;
        let capped = f64::from(MAX_STEP) * BASE_RATE;
        assert!(
            (clock.elapsed() - start.elapsed() - capped).abs() < 1e-3,
            "advanced {} game s, expected {capped}",
            clock.elapsed() - start.elapsed()
        );
        let after = train(&app).state();
        let moved = (after.front - before.front).abs();
        assert!(
            moved <= Timetable::default().top_speed * MAX_STEP + 1e-3,
            "the train jumped {moved} m in one frame"
        );
    }

    #[test]
    fn the_train_pulls_out_when_the_clock_reaches_its_departure() {
        let mut app = shipped_app(Some(GameClock::at(TimeOfDay::hm(6, 59))));
        app.update();
        assert_eq!(train(&app).state().phase, LinePhase::Parked);
        // Four real seconds: two game minutes, past 07:00.
        for _ in 0..80 {
            app.update();
        }
        let s = train(&app).state();
        assert_eq!(s.phase, LinePhase::Running);
        assert!(s.speed > 0.0);
        assert_eq!(
            app.world().resource::<GameWorld>().0.moving,
            train(&app).obstacles()
        );
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
        // A saddled 3D horse, no rider; with no errand here its coat is the
        // town's own.
        let town = &app.world().resource::<WorldTowns>().0[0];
        let coat = Coat::seeded(town.seed ^ 0x4f52_5345);
        assert_eq!(*visual, CharacterVisual::Horse(coat));
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
    fn don_hidalgos_horse_is_the_bayo_and_other_towns_seed_theirs() {
        let towns = vec![TownLayout::castex_inspired(), TownLayout::castex_inspired()];
        let cast = cast_of(&towns);
        let hidalgo = cast
            .0
            .iter()
            .find(|(n, _)| *n == Npc::Hidalgo)
            .unwrap()
            .1
            .town;
        assert_eq!(tethered_coat(&towns[hidalgo], &cast, hidalgo), Coat::Bayo);
        let empty = Cast::default();
        // Even where the town's own seed would give another coat: Don
        // Hidalgo's town ties up his bayo, not the seeded horse.
        let mut his = towns[hidalgo].clone();
        his.seed = (0..)
            .find(|&seed| Coat::seeded(seed ^ 0x4f52_5345) != Coat::Bayo)
            .unwrap();
        assert_ne!(tethered_coat(&his, &empty, hidalgo), Coat::Bayo);
        assert_eq!(tethered_coat(&his, &cast, hidalgo), Coat::Bayo);
        let mut town = TownLayout::castex_inspired();
        let mut coats = std::collections::HashSet::new();
        for seed in 0..40 {
            town.seed = seed;
            let coat = tethered_coat(&town, &empty, 0);
            assert_eq!(coat, tethered_coat(&town, &empty, 0), "deterministic");
            coats.insert(coat);
        }
        assert!(coats.len() >= 4, "towns get different horses: {coats:?}");
    }

    fn cast_of(towns: &[TownLayout]) -> Cast {
        let errand = pampa_core::Errand::new(towns, 0, 1).unwrap();
        Cast(
            [Npc::Hidalgo, Npc::Farach]
                .map(|npc| (npc, errand.standing(npc)))
                .to_vec(),
        )
    }

    fn standing_npcs(app: &mut App) -> Vec<(StandingNpc, CharacterVisual, Transform)> {
        let mut q = app
            .world_mut()
            .query::<(&StandingNpc, &CharacterVisual, &Transform)>();
        q.iter(app.world()).map(|(n, v, t)| (*n, *v, *t)).collect()
    }

    #[test]
    fn the_errand_people_stand_in_their_towns_and_leave_with_them() {
        let towns = vec![TownLayout::castex_inspired(), far_town()];
        let cast = cast_of(&towns);
        let mut app = app_with(towns.clone(), 2);
        let mut activation = TownActivation::default();
        activation.update(&towns, Vec2::ZERO);
        app.insert_resource(ActiveTowns(activation))
            .insert_resource(cast.clone());
        app.update();
        let here = standing_npcs(&mut app);
        assert_eq!(here.len(), 1);
        let (npc, visual, transform) = here[0];
        assert_eq!(
            npc,
            StandingNpc {
                town: 0,
                npc: Npc::Hidalgo
            }
        );
        assert_eq!(visual, CharacterVisual::Figure(FigureKind::Hidalgo));
        let spot = cast.0[0].1.position;
        assert_eq!(
            (transform.translation.x, -transform.translation.z),
            (spot.x, spot.y)
        );

        // Over at the far town: Hidalgo is gone, the comisario stands there.
        app.world_mut()
            .resource_mut::<ActiveTowns>()
            .0
            .update(&towns, Vec2::new(3000.0, -2000.0));
        app.update();
        let there = standing_npcs(&mut app);
        assert_eq!(there.len(), 1);
        assert_eq!(there[0].0.npc, Npc::Farach);
        assert_eq!(there[0].1, CharacterVisual::Figure(FigureKind::Farach));
    }

    #[test]
    fn a_horse_led_away_is_not_tied_up_again() {
        let towns = vec![TownLayout::castex_inspired(), far_town()];
        let mut app = app_with(towns.clone(), 2);
        app.world_mut().resource_mut::<LedAway>().0.insert(0);
        app.update();
        let mut q = app.world_mut().query::<&TetheredHorse>();
        let horses: Vec<usize> = q.iter(app.world()).map(|h| h.0).collect();
        assert_eq!(horses, vec![1], "only the far town's horse is tied");
    }

    #[test]
    fn walking_into_people_on_foot_moves_them_without_complaints() {
        #[derive(Resource, Default)]
        struct Shoves(usize);
        fn count(mut shoved: MessageReader<Shoved>, mut shoves: ResMut<Shoves>) {
            shoves.0 += shoved.read().count();
        }
        let mut app = town_app();
        let at = app.world().resource::<Crowds>().0[0].people[0].position;
        app.insert_resource(crate::player::PlayerSpawn {
            position: at,
            heading: pampa_core::Heading::from_radians(0.0),
            travel: pampa_core::Travel::OnFoot,
        })
        .init_resource::<Shoves>()
        .add_systems(PostUpdate, count);
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
        }
        for _ in 0..100 {
            app.update();
            let me = {
                let mut q = app.world_mut().query::<&crate::player::Player>();
                q.single(app.world()).unwrap().rider
            };
            let crowd = &app.world().resource::<Crowds>().0[0];
            for p in &crowd.people {
                let gap = p.position.distance(me.position);
                assert!(gap >= p.collider.radius + me.collider.radius - 1e-3);
            }
        }
        assert_eq!(
            app.world().resource::<Shoves>().0,
            0,
            "no bump complaints on foot"
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
            ..default()
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
