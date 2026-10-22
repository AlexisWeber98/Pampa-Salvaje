//! Riding the line train, played: F boards a standing train from its
//! platform, F gets off at a station (or, while running, at the next one),
//! Space skips to the next stop, and F near the middle of an empty platform
//! waits for the next train. The game opens with the player aboard the
//! morning train from Pico, put off automatically at Monte Nievas.
//!
//! The skip is a short trip ([`Trip`]): the train pulls out for real for
//! [`LEAVING_SECONDS`] ("Próxima estación: …"), the screen fades to black,
//! the clock and the train jump to [`pampa_core::APPROACH_LEAD`] real
//! seconds before the next stop, and the train comes in and brakes to a
//! stop ("Estación …"). Space during the trip shortens it.
//!
//! `pampa-core` owns the rules ([`pampa_core::ride`]): every jump uses its
//! pure state functions. This module reads the keys, plays the fade
//! ([`ScreenFade`]) and the announcements ([`HudAnnouncement`]), keeps the
//! seated (hidden) player in the coach and the clock at the onboard rate,
//! shows the next station and its arrival time on the HUD ([`HudRide`]),
//! and puts the player off on the platform.

use bevy::prelude::*;
use pampa_core::{
    Boarding, LinePhase, LineTrain, ONBOARD_MULTIPLIER, Ride, Rider, SEAT_HEIGHT, TownLayout,
    alight_spot, boarding, depart_now, next_arrival, skip_ride, skip_to_approach, stands_at,
    wait_for_train, waiting_station,
};
use pampa_render::{
    HudAnnouncement, HudRide, Say, ScreenFade, WorldClock, WorldTowns, WorldTrain,
    world_from_ground,
};

use crate::errand::INTERACT_KEY;
use crate::player::{DrivePlayer, GameWorld, Player, drive_player};

/// Skips the rest of a train ride to the next stop.
pub const SKIP_KEY: KeyCode = KeyCode::Space;

/// What the player says trying to board on horseback.
pub const DISMOUNT_FIRST: &str = "Primero bajate del caballo";

/// The player is riding the line train, seated (and hidden) in a coach.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct OnTrain(pub Ride);

/// The ride the player starts the game on, if any (the opening).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct StartAboard(pub Option<Ride>);

/// Real seconds the train pulls out before a skip fades to black.
pub const LEAVING_SECONDS: f32 = 3.0;

/// Real seconds a station announcement stays on screen.
pub const ANNOUNCE_SECONDS: f32 = 3.0;

/// Real seconds between the train stopping and the player getting off
/// where the ride ends: long enough to read "Estación …".
pub const ALIGHT_DELAY: f32 = 2.0;

/// A jump in time waiting for the fade to reach black.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// Skip the ride to just before the next stop.
    Approach,
    /// Wait on the platform of this station for the next train.
    Wait(usize),
}

/// Where a skipped ride (Space aboard) is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TripPhase {
    /// Pulling out for real, this many real seconds left before the fade.
    Leaving(f32),
    /// Fading to black, to jump to just before the next stop.
    Fading,
    /// Coming in for real, until the train stands at the station.
    Arriving,
}

/// The skipped ride underway, if any.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Trip(pub Option<TripPhase>);

/// The station announcement on screen and its real seconds left.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct AnnounceTimer(pub f32);

/// Real seconds left before the player is put off where the ride ends.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct PutOffIn(pub Option<f32>);

/// The jump the running fade will make, if any.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct PendingJump(pub Option<Jump>);

pub struct RidePlugin;

impl Plugin for RidePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Say>()
            .init_resource::<StartAboard>()
            .init_resource::<PendingJump>()
            .init_resource::<ScreenFade>()
            .init_resource::<GameWorld>()
            .init_resource::<HudRide>()
            .init_resource::<HudAnnouncement>()
            .init_resource::<Trip>()
            .init_resource::<AnnounceTimer>()
            .init_resource::<PutOffIn>()
            .add_systems(
                Update,
                (
                    ride_keys,
                    run_trip,
                    jump_at_black,
                    follow_train,
                    announce,
                    show_ride,
                )
                    .chain()
                    .in_set(DrivePlayer)
                    .before(drive_player),
            );
    }
}

/// What the ride systems read of the world.
type Line<'w> = (
    Option<ResMut<'w, WorldTrain>>,
    Option<Res<'w, WorldTowns>>,
    ResMut<'w, GameWorld>,
);

/// The player as the ride systems change them.
type Passenger<'a> = (
    Entity,
    &'a mut Player,
    &'a mut Transform,
    Option<&'a mut OnTrain>,
);

/// What the ride keys change besides the player.
type RideState<'w> = (
    ResMut<'w, ScreenFade>,
    ResMut<'w, PendingJump>,
    ResMut<'w, Trip>,
    Option<ResMut<'w, WorldClock>>,
);

/// The keys: F boards, gets off or waits; Space skips the ride (or
/// shortens a skip underway). A key used here is cleared, so the errand
/// does not also act on it.
fn ride_keys(
    mut commands: Commands,
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    (mut train, towns, mut world): Line,
    (mut fade, mut pending, mut trip, mut clock): RideState,
    mut players: Query<Passenger>,
    (mut say, mut announcer): (MessageWriter<Say>, Announcer),
) {
    let Some(mut keys) = keys else {
        return;
    };
    let Ok((me, mut player, mut transform, aboard)) = players.single_mut() else {
        return;
    };
    let Some(train) = train.as_mut().and_then(|t| t.0.as_mut()) else {
        return;
    };
    let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
    if fade.active() {
        keys.clear_just_pressed(INTERACT_KEY);
        keys.clear_just_pressed(SKIP_KEY);
        return;
    }
    if let Some(mut on_train) = aboard {
        if keys.clear_just_pressed(SKIP_KEY)
            && let Some(clock) = clock.as_mut()
        {
            match trip.0 {
                None => {
                    // Pull out now (the rest of the dwell is invisible: the
                    // train stands still), then ride for a moment.
                    depart_now(train, &mut clock.0);
                    world.0.moving = train.obstacles();
                    trip.0 = Some(TripPhase::Leaving(LEAVING_SECONDS));
                    if train.state().phase == LinePhase::Running {
                        announcer.next_station(train, towns, &clock.0);
                    }
                }
                Some(TripPhase::Leaving(_)) => trip.0 = Some(TripPhase::Leaving(0.0)),
                Some(TripPhase::Arriving) => {
                    skip_ride(train, &mut clock.0);
                    world.0.moving = train.obstacles();
                }
                Some(TripPhase::Fading) => {}
            }
        }
        if keys.clear_just_pressed(INTERACT_KEY) {
            let state = train.state();
            let ride = on_train.0;
            let standing = trip.0.is_none() && state.phase != LinePhase::Running;
            let mut alight = Alight {
                commands: &mut commands,
                train,
                towns,
                world: &world.0,
            };
            if !(standing && alight.put_off(me, &mut player.rider, &mut transform, ride)) {
                // Running: get off at the next stop.
                let now = clock.as_ref().map(|c| c.0);
                if let Some((station, _)) = now.and_then(|c| next_arrival(train, &c)) {
                    on_train.0.alight_at = Some(station);
                }
            }
        }
        return;
    }
    if !keys.just_pressed(INTERACT_KEY) {
        return;
    }
    match boarding(train, &player.rider, &world.0.floors) {
        Boarding::Board(ride) => {
            keys.clear_just_pressed(INTERACT_KEY);
            let ride = ride.facing_platform(train, towns);
            commands
                .entity(me)
                .insert((OnTrain(ride), Visibility::Hidden));
            seat(&mut player.rider, &mut transform, train, &ride, &world.0);
        }
        Boarding::DismountFirst => {
            keys.clear_just_pressed(INTERACT_KEY);
            say.write(Say {
                speaker: me,
                line: DISMOUNT_FIRST.into(),
            });
        }
        Boarding::Nothing => {
            if let Some(station) = waiting_station(train, towns, &player.rider)
                && fade.start()
            {
                keys.clear_just_pressed(INTERACT_KEY);
                pending.0 = Some(Jump::Wait(station));
            }
        }
    }
}

/// Runs the skipped ride: after pulling out, the fade to black; once the
/// train stands at the next station, the trip is over.
fn run_trip(
    time: Res<Time>,
    mut trip: ResMut<Trip>,
    mut fade: ResMut<ScreenFade>,
    mut pending: ResMut<PendingJump>,
    train: Option<Res<WorldTrain>>,
    aboard: Query<(), (With<Player>, With<OnTrain>)>,
) {
    if aboard.is_empty() {
        trip.0 = None;
        return;
    }
    match trip.0 {
        Some(TripPhase::Leaving(left)) => {
            let left = left - time.delta_secs();
            if left > 0.0 {
                trip.0 = Some(TripPhase::Leaving(left));
            } else if fade.start() {
                pending.0 = Some(Jump::Approach);
                trip.0 = Some(TripPhase::Fading);
            }
        }
        Some(TripPhase::Arriving) => {
            let state = train.as_ref().and_then(|t| t.0.as_ref()).map(|t| t.state());
            if state.is_some_and(|s| s.phase != LinePhase::Running) {
                trip.0 = None;
            }
        }
        _ => {}
    }
}

/// Runs the fade and, at full black, makes the pending jump: the clock and
/// the train land just before the next stop (skip) or on the next train at
/// the platform (wait).
fn jump_at_black(
    time: Res<Time>,
    mut fade: ResMut<ScreenFade>,
    mut pending: ResMut<PendingJump>,
    mut trip: ResMut<Trip>,
    clock: Option<ResMut<WorldClock>>,
    (train, _, mut world): Line,
) {
    if !fade.tick(time.delta_secs()) {
        return;
    }
    let jump = pending.0.take();
    let (Some(mut clock), Some(mut train)) = (clock, train) else {
        return;
    };
    let Some(train) = train.0.as_mut() else {
        return;
    };
    match jump {
        Some(Jump::Approach) => {
            skip_to_approach(train, &mut clock.0);
            trip.0 = Some(TripPhase::Arriving);
        }
        Some(Jump::Wait(station)) => {
            wait_for_train(train, &mut clock.0, station);
        }
        None => {}
    }
    world.0.moving = train.obstacles();
}

/// Keeps a seated player in their coach and the clock at the onboard rate,
/// and puts them off where the ride ends, [`ALIGHT_DELAY`] after the train
/// stops; on foot the clock runs at its base rate.
fn follow_train(
    mut commands: Commands,
    time: Res<Time>,
    (clock, mut put_off_in): (Option<ResMut<WorldClock>>, ResMut<PutOffIn>),
    (train, towns, world): Line,
    mut players: Query<Passenger>,
) {
    let Ok((me, mut player, mut transform, aboard)) = players.single_mut() else {
        return;
    };
    let train = train.as_ref().and_then(|t| t.0.as_ref());
    let multiplier = match (aboard, train) {
        (Some(mut on_train), Some(train)) => {
            let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
            let ends = on_train.0.put_off(&train.state());
            let ride = on_train.0;
            // Off once "Estación …" has been read.
            put_off_in.0 = match (ends, put_off_in.0) {
                (false, _) => None,
                (true, None) => Some(ALIGHT_DELAY),
                (true, Some(left)) => Some(left - time.delta_secs()),
            };
            let due = put_off_in.0.is_some_and(|left| left <= 0.0);
            let mut alight = Alight {
                commands: &mut commands,
                train,
                towns,
                world: &world.0,
            };
            if due && alight.put_off(me, &mut player.rider, &mut transform, ride) {
                put_off_in.0 = None;
                1.0
            } else {
                seat(&mut player.rider, &mut transform, train, &ride, &world.0);
                ONBOARD_MULTIPLIER
            }
        }
        _ => 1.0,
    };
    if let Some(mut clock) = clock
        && clock.0.multiplier != multiplier
    {
        clock.0.multiplier = multiplier;
    }
}

/// Writes station announcements on the HUD.
#[derive(bevy::ecs::system::SystemParam)]
struct Announcer<'w> {
    hud: ResMut<'w, HudAnnouncement>,
    timer: ResMut<'w, AnnounceTimer>,
}

impl Announcer<'_> {
    fn show(&mut self, text: String) {
        self.hud.0 = Some(text);
        self.timer.0 = ANNOUNCE_SECONDS;
    }

    /// "Próxima estación: …", the train's next stop.
    fn next_station(
        &mut self,
        train: &LineTrain,
        towns: &[TownLayout],
        clock: &pampa_core::GameClock,
    ) {
        if let Some((station, _)) = next_arrival(train, clock)
            && let Some(name) = town_name(train, towns, station)
        {
            self.show(format!("Próxima estación: {name}"));
        }
    }
}

/// The name of the town of `station`.
fn town_name<'a>(train: &LineTrain, towns: &'a [TownLayout], station: usize) -> Option<&'a str> {
    let town = train.stations().get(station)?.town;
    towns.get(town).map(|t| t.name.as_str())
}

/// Announces, while aboard, the next station as the train pulls out (or as
/// the player boards a running train) and the station as it stops there;
/// clears the announcement after [`ANNOUNCE_SECONDS`].
fn announce(
    time: Res<Time>,
    clock: Option<Res<WorldClock>>,
    (train, towns, _): Line,
    aboard: Query<(), (With<Player>, With<OnTrain>)>,
    mut announcer: Announcer,
    mut last: Local<Option<(bool, Option<usize>)>>,
) {
    if announcer.timer.0 > 0.0 {
        announcer.timer.0 -= time.delta_secs();
        if announcer.timer.0 <= 0.0 {
            announcer.hud.0 = None;
        }
    }
    let (Some(train), Some(clock)) = (train.as_ref().and_then(|t| t.0.as_ref()), clock) else {
        return;
    };
    let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
    let s = train.state();
    let standing = (s.phase != LinePhase::Running)
        .then_some(s.station)
        .flatten();
    let now = (!aboard.is_empty(), standing);
    let before = last.replace(now);
    if !now.0 {
        return;
    }
    match (before, standing) {
        // Stopped at a station.
        (Some((true, None)), Some(station)) => {
            if let Some(name) = town_name(train, towns, station) {
                announcer.show(format!("Estación {name}"));
            }
        }
        // Pulled out, or just boarded a running train.
        (Some((true, Some(_))), None) | (None | Some((false, _)), None) => {
            announcer.next_station(train, towns, &clock.0);
        }
        _ => {}
    }
}

/// The HUD's ride line while aboard: the next station and the time the
/// train arrives there at the onboard rate, e.g. `Próxima: Eduardo Castex,
/// 11:22`.
fn show_ride(
    clock: Option<Res<WorldClock>>,
    (train, towns, _): Line,
    players: Query<Has<OnTrain>, With<Player>>,
    mut hud: ResMut<HudRide>,
) {
    let aboard = players.iter().any(|on| on);
    let text = (|| {
        let train = train.as_ref()?.0.as_ref()?;
        let (station, at) = next_arrival(train, &clock.as_ref()?.0)?;
        let town = towns.as_ref()?.0.get(train.stations()[station].town)?;
        Some(format!("Próxima: {}, {at}", town.name))
    })()
    .filter(|_| aboard);
    hud.set_if_neq(HudRide(text));
}

/// Seats `rider` in the ride's coach, facing the way the train runs.
fn seat(
    rider: &mut Rider,
    transform: &mut Transform,
    train: &LineTrain,
    ride: &Ride,
    world: &pampa_core::World,
) {
    let Some((at, heading)) = ride.seat(train) else {
        return;
    };
    rider.position = at;
    rider.heading = heading;
    rider.speed = 0.0;
    rider.height = world.terrain.height(at.x, at.y) + LineTrain::RAIL_TOP + SEAT_HEIGHT;
    transform.translation = world_from_ground(rider.position, rider.height);
}

/// What getting off the train reads.
struct Alight<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    train: &'a LineTrain,
    towns: &'a [TownLayout],
    world: &'a pampa_core::World,
}

impl Alight<'_, '_, '_> {
    /// Puts the passenger (entity `me`) off on the platform beside their
    /// coach, on foot and visible again. `false` (still aboard) while the
    /// train runs or where there is no platform.
    fn put_off(
        &mut self,
        me: Entity,
        rider: &mut Rider,
        transform: &mut Transform,
        ride: Ride,
    ) -> bool {
        let Some(station) = self.train.state().station else {
            return false;
        };
        if !stands_at(&self.train.state(), station) {
            return false;
        }
        let Some((spot, heading)) = alight_spot(self.train, ride.car, self.towns, self.world)
        else {
            return false;
        };
        *rider = Rider::on_foot(spot, heading, self.world);
        transform.translation = world_from_ground(rider.position, rider.height);
        self.commands
            .entity(me)
            .remove::<OnTrain>()
            .insert(Visibility::Inherited);
        true
    }
}

/// A headless app of the shipped world for the ride and errand tests.
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::errand::{ErrandDialogue, ErrandPlugin, plan};
    use crate::life::{Crowds, TownLifePlugin, start_of_day};
    use crate::player::{PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_content::{ERRAND_FILE, ErrandLines, asset_path, load_errand_or_builtin};
    use pampa_core::{BASE_RATE, GameClock, Terrain, Timetable, Travel, World};
    use std::time::Duration;

    /// Seconds per frame.
    pub const FRAME: f32 = 0.05;

    pub fn shipped_lines() -> ErrandLines {
        let (lines, error) = load_errand_or_builtin(&asset_path(ERRAND_FILE));
        assert!(error.is_none(), "{error:?}");
        lines
    }

    /// The shipped world with the errand. With no `clock`, the game's
    /// opening: aboard the morning train pulling into Monte Nievas. With a
    /// `clock`, the train jumped there from midnight and the player on foot
    /// at the middle of the Monte Nievas platform.
    pub fn shipped_app_at(clock: Option<GameClock>) -> App {
        let loaded = crate::towns::load_world(&asset_path(pampa_content::TOWNS_FILE));
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let (run, cast) = plan(&loaded.towns, &mut world, &shipped_lines().police_town)
            .expect("the shipped errand");
        let (start, mut train, mut ride) = start_of_day(&loaded.line, &loaded.towns);
        let mut spawn = (loaded.towns[0].arrival(), Travel::OnFoot);
        let clock = match clock {
            None => {
                let seat = ride.and_then(|r| r.seat(train.as_ref()?)).unwrap();
                spawn.0 = seat;
                start
            }
            Some(clock) => {
                ride = None;
                train = LineTrain::new(&loaded.line, &loaded.towns, Timetable::default());
                if let Some(train) = &mut train {
                    train.advance(0.0, clock.elapsed(), BASE_RATE);
                }
                spawn.0 = loaded.towns[0].platform_spot().unwrap();
                clock
            }
        };
        let ((position, heading), travel) = spawn;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Crowds::for_towns(&loaded.towns))
            .insert_resource(WorldTowns(loaded.towns))
            .insert_resource(WorldClock(clock))
            .insert_resource(WorldTrain(train))
            .insert_resource(GameWorld(world))
            .insert_resource(PlayerSpawn {
                position,
                heading,
                travel,
            })
            .insert_resource(StartAboard(ride))
            .insert_resource(run)
            .insert_resource(cast)
            .insert_resource(ErrandDialogue(shipped_lines()))
            .add_plugins((PlayerPlugin, TownLifePlugin, ErrandPlugin, RidePlugin));
        app
    }

    /// The game's opening.
    pub fn shipped_app() -> App {
        shipped_app_at(None)
    }

    /// Presses `key` for exactly one frame.
    pub fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    /// The player's body and whether they ride the train.
    pub fn passenger(app: &mut App) -> (Rider, Option<Ride>) {
        let mut q = app.world_mut().query::<(&Player, Option<&OnTrain>)>();
        let (p, on) = q.single(app.world()).unwrap();
        (p.rider, on.map(|o| o.0))
    }

    /// Runs frames until the fade is over (at most two seconds).
    pub fn settle(app: &mut App) {
        for _ in 0..(2.0 / FRAME) as usize {
            app.update();
            if !app.world().resource::<ScreenFade>().active() {
                return;
            }
        }
        panic!("the fade never ended");
    }

    /// Runs frames until the player is off the train (at most `seconds`),
    /// and returns the real seconds that took.
    pub fn until_off(app: &mut App, seconds: f32) -> f32 {
        let mut frames = 0;
        while passenger(app).1.is_some() {
            app.update();
            frames += 1;
            assert!((frames as f32) * FRAME < seconds, "never got off");
        }
        frames as f32 * FRAME
    }

    /// Skips the opening ride (Space) and lets the player off at Monte
    /// Nievas.
    pub fn arrive(app: &mut App) {
        app.update();
        press(app, SKIP_KEY);
        until_off(app, 20.0);
        assert_eq!(passenger(app).1, None, "off the train");
    }

    /// Skips whatever ride is underway (Space) until the player is off.
    pub fn arrive_from_anywhere(app: &mut App) {
        press(app, SKIP_KEY);
        until_off(app, 20.0);
    }

    /// The announcement on screen.
    pub fn announcement(app: &App) -> Option<String> {
        app.world().resource::<HudAnnouncement>().0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use crate::errand::ErrandRun;
    use pampa_core::{ErrandStage, GameClock, LinePhase, OPENING_LEAD, Run, TimeOfDay, Travel};
    use pampa_render::{HudObjective, HudRide, MapTarget};

    fn train(app: &App) -> LineTrain {
        app.world().resource::<WorldTrain>().0.clone().unwrap()
    }

    fn clock(app: &App) -> GameClock {
        app.world().resource::<WorldClock>().0
    }

    fn visibility(app: &mut App) -> Visibility {
        let mut q = app
            .world_mut()
            .query_filtered::<&Visibility, With<Player>>();
        *q.single(app.world()).unwrap()
    }

    /// The platform of the station the train stands at, if `at` is on it.
    fn on_platform(app: &App, at: pampa_core::Vec2) -> Option<String> {
        let towns = &app.world().resource::<WorldTowns>().0;
        towns
            .iter()
            .find(|t| t.floors().iter().any(|f| f.footprint.contains_point(at)))
            .map(|t| t.name.clone())
    }

    #[test]
    fn the_opening_rides_into_monte_nievas_and_puts_the_player_off_on_its_platform() {
        let mut app = shipped_app();
        app.update();
        let (rider, ride) = passenger(&mut app);
        let ride = ride.expect("aboard at the start");
        assert_eq!(visibility(&mut app), Visibility::Hidden);
        let (seat, heading) = ride.seat(&train(&app)).unwrap();
        assert_eq!((rider.position, rider.heading), (seat, heading));
        assert_eq!(clock(&app).multiplier, ONBOARD_MULTIPLIER);
        assert_eq!(train(&app).state().phase, LinePhase::Running);
        // The HUD tells the next station and when the train gets there.
        let ride_line = app.world().resource::<HudRide>().0.clone();
        assert_eq!(ride_line.as_deref(), Some("Próxima: Monte Nievas, 10:42"));
        assert!(ride_line.unwrap().chars().all(pampa_render::font::supports));
        assert_eq!(
            announcement(&app).as_deref(),
            Some("Próxima estación: Monte Nievas")
        );
        // The train comes in for about 50 real seconds, then "Estación
        // Monte Nievas" shows for a moment before he gets off.
        let ridden = f64::from(until_off(&mut app, 70.0)) + f64::from(FRAME);
        let expected = OPENING_LEAD + f64::from(ALIGHT_DELAY);
        assert!((ridden - expected).abs() < 1.0, "{ridden}");
        assert_eq!(announcement(&app).as_deref(), Some("Estación Monte Nievas"));
        let s = train(&app).state();
        assert_eq!((s.phase, s.run), (LinePhase::Dwelling, Run::Up));
        let (rider, _) = passenger(&mut app);
        assert_eq!(rider.travel(), Travel::OnFoot);
        assert_eq!(
            on_platform(&app, rider.position).as_deref(),
            Some("Monte Nievas")
        );
        assert_eq!(visibility(&mut app), Visibility::Inherited);
        // In at the timetabled arrival, 10:42:13, and off two real
        // seconds (24 game seconds) later, at 10:42:37.
        let at = clock(&app).time_of_day().seconds();
        let arrival = train(&app)
            .arrival_time_at(Run::Up, s.station.unwrap())
            .unwrap();
        let off = arrival.seconds()
            + f64::from(ALIGHT_DELAY) * ONBOARD_MULTIPLIER * pampa_core::BASE_RATE;
        assert!((at - off).abs() < 1.0, "{}", clock(&app).hh_mm());
        assert_eq!(clock(&app).hh_mm(), "10:42");
        app.update();
        assert_eq!(clock(&app).multiplier, 1.0);
        assert_eq!(app.world().resource::<HudRide>().0, None, "line gone");
        // The errand begins.
        let errand = app.world().resource::<ErrandRun>().0.clone();
        assert_eq!(errand.stage(), ErrandStage::MeetHidalgo);
        assert_eq!(
            app.world().resource::<HudObjective>().0.as_deref(),
            Some("Buscá a Don Hidalgo en la plaza")
        );
    }

    #[test]
    fn space_skips_the_opening_as_a_short_trip_to_the_platform() {
        let mut app = shipped_app();
        app.update();
        let front = train(&app).state().front;
        press(&mut app, SKIP_KEY);
        // (a) Still riding in for real, "Próxima estación: Monte Nievas".
        assert_eq!(
            app.world()
                .resource::<Trip>()
                .0
                .map(|t| matches!(t, TripPhase::Leaving(_))),
            Some(true)
        );
        assert!(!app.world().resource::<ScreenFade>().active());
        assert_eq!(
            announcement(&app).as_deref(),
            Some("Próxima estación: Monte Nievas")
        );
        for _ in 0..((LEAVING_SECONDS - 0.2) / FRAME) as usize {
            app.update();
        }
        assert!(passenger(&mut app).1.is_some());
        assert!(
            (train(&app).state().front - front).abs() > 20.0,
            "moved for real"
        );
        // (b) The fade, and at black the jump to 3 s out.
        while !app.world().resource::<ScreenFade>().active() {
            app.update();
        }
        while app.world().resource::<Trip>().0 == Some(TripPhase::Fading) {
            app.update();
        }
        // (c) Coming in for real, braking: the pure state 3 s out at most.
        let s = train(&app).state();
        assert_eq!(s.phase, LinePhase::Running);
        let (next, left) = train(&app).next_stop().unwrap();
        assert!(left <= pampa_core::APPROACH_LEAD && left > 2.0, "{left}");
        // (d) "Estación Monte Nievas", then off on the platform.
        until_off(&mut app, 10.0);
        let (rider, _) = passenger(&mut app);
        assert_eq!(
            on_platform(&app, rider.position).as_deref(),
            Some("Monte Nievas")
        );
        assert_eq!(announcement(&app).as_deref(), Some("Estación Monte Nievas"));
        let s = train(&app).state();
        assert_eq!((s.phase, s.station), (LinePhase::Dwelling, Some(next)));
        // Exact: the clock reads the timetabled arrival plus the 2 s wait.
        let arrival = train(&app)
            .arrival_time_at(Run::Up, next)
            .unwrap()
            .seconds();
        let off = arrival + f64::from(ALIGHT_DELAY) * ONBOARD_MULTIPLIER * pampa_core::BASE_RATE;
        let at = clock(&app).time_of_day().seconds();
        assert!((at - off).abs() < 1.0, "{}", clock(&app).hh_mm());
        assert_eq!(
            app.world().resource::<ErrandRun>().0.stage(),
            ErrandStage::MeetHidalgo
        );
    }

    #[test]
    fn board_at_monte_nievas_skip_to_castex_and_get_off_there() {
        let mut app = shipped_app();
        arrive(&mut app);
        // F beside the coach: aboard, hidden, riding at 12×.
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some(), "boarded");
        assert_eq!(visibility(&mut app), Visibility::Hidden);
        app.update();
        assert_eq!(clock(&app).multiplier, ONBOARD_MULTIPLIER);
        // F again while it stands there: off again, then back on.
        press(&mut app, INTERACT_KEY);
        let (rider, ride) = passenger(&mut app);
        assert_eq!(ride, None);
        assert_eq!(
            on_platform(&app, rider.position).as_deref(),
            Some("Monte Nievas")
        );
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some());
        // Space: on to Castex, the terminal, where the ride ends; Space
        // again cuts the pulling out short.
        press(&mut app, SKIP_KEY);
        press(&mut app, SKIP_KEY);
        app.update();
        assert!(app.world().resource::<ScreenFade>().active(), "shortened");
        until_off(&mut app, 10.0);
        assert_eq!(
            announcement(&app).as_deref(),
            Some("Estación Eduardo Castex")
        );
        let s = train(&app).state();
        assert_eq!(s.phase, LinePhase::Parked);
        let (rider, ride) = passenger(&mut app);
        assert_eq!(ride, None, "put off at the terminal");
        assert_eq!(rider.travel(), Travel::OnFoot);
        assert_eq!(
            on_platform(&app, rider.position).as_deref(),
            Some("Eduardo Castex")
        );
        assert_eq!(visibility(&mut app), Visibility::Inherited);
        // The errand still points at Don Hidalgo, and the player walks.
        let errand = app.world().resource::<ErrandRun>().0.clone();
        assert_eq!(errand.stage(), ErrandStage::MeetHidalgo);
        assert_eq!(app.world().resource::<MapTarget>().0, errand.target());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..20 {
            app.update();
        }
        assert!(passenger(&mut app).0.position.distance(rider.position) > 1.0);
        // And can board the laid-over train back.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyW);
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider = rider;
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some(), "boards at Castex");
    }

    #[test]
    fn mounted_at_the_coach_door_the_player_is_told_to_get_off_first() {
        let mut app = shipped_app();
        arrive(&mut app);
        let (rider, _) = passenger(&mut app);
        let mut q = app.world_mut().query::<&mut Player>();
        let world = app.world().resource::<GameWorld>().0.clone();
        q.single_mut(app.world_mut()).unwrap().rider =
            Rider::new(rider.position, rider.heading, &world);
        press(&mut app, INTERACT_KEY);
        let (rider, ride) = passenger(&mut app);
        assert_eq!(ride, None, "not on horseback");
        assert_eq!(rider.travel(), pampa_core::Travel::Riding, "still mounted");
        assert!(DISMOUNT_FIRST.chars().all(pampa_render::font::supports));
    }

    #[test]
    fn waiting_on_an_empty_platform_fast_forwards_to_the_next_train() {
        let mut app = shipped_app_at(Some(GameClock::at(TimeOfDay::hm(13, 0))));
        app.update();
        let nievas = train(&app).station_of_town(0).unwrap();
        assert!(!stands_at(&train(&app).state(), nievas));
        press(&mut app, INTERACT_KEY);
        assert!(app.world().resource::<ScreenFade>().active());
        settle(&mut app);
        let s = train(&app).state();
        assert_eq!(
            (s.phase, s.station, s.run),
            (LinePhase::Dwelling, Some(nievas), Run::Down)
        );
        // The afternoon train from Castex, at 17:26.
        assert_eq!(clock(&app).hh_mm(), "17:26");
        assert_eq!(passenger(&mut app).1, None, "still on the platform");
        // Now F boards it.
        let (middle, _) = app.world().resource::<WorldTowns>().0[0]
            .platform_spot()
            .unwrap();
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = middle;
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some());
    }
}
