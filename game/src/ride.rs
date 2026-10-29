//! Viajar en el tren de la línea, en juego: F sube a un tren detenido desde su
//! andén, F baja en una estación (o, si está en marcha, en la siguiente),
//! Espacio salta a la siguiente parada, y F cerca del centro de un andén vacío
//! espera el próximo tren. El juego empieza con el jugador a bordo del tren de
//! la mañana desde Pico, que lo baja automáticamente en Monte Nievas.
//!
//! El salto es un viaje corto ([`Trip`]): el tren arranca de verdad durante
//! [`LEAVING_SECONDS`] ("Próxima estación: …"), la pantalla hace un fundido a
//! negro, el reloj y el tren saltan a [`pampa_core::APPROACH_LEAD`] segundos
//! reales antes de la siguiente parada, y el tren entra y frena hasta detenerse
//! ("Estación …"). Espacio durante el viaje lo acorta.
//!
//! `pampa-core` es dueño de las reglas ([`pampa_core::ride`]): cada salto usa
//! sus funciones de estado puras. Este módulo lee las teclas, reproduce el
//! fundido ([`ScreenFade`]) y los anuncios ([`HudAnnouncement`]), mantiene al
//! jugador sentado (oculto) en el coche y el reloj al ritmo de a bordo, muestra
//! la siguiente estación y su hora de llegada en el HUD ([`HudRide`]), y baja
//! al jugador en el andén.

use bevy::prelude::*;
use pampa_core::{
    Boarding, GameClock, LinePhase, LineTrain, ONBOARD_MULTIPLIER, Ride, Rider, SEAT_HEIGHT,
    TownLayout, WaitingPlace, alight_spot, boarding_at, depart_now, follow_jump, next_arrival,
    skip_ride, skip_to_approach, stands_at, wait_for_train, waiting_place,
};
use pampa_render::{
    HudAnnouncement, HudRide, Say, ScreenFade, WorldClock, WorldRail, WorldTowns, WorldTrains,
    world_from_ground,
};

use crate::errand::INTERACT_KEY;
use crate::player::{DrivePlayer, GameWorld, Player, drive_player};

/// Omite el resto de un viaje en tren hasta la siguiente parada.
pub const SKIP_KEY: KeyCode = KeyCode::Space;

/// Lo que dice el jugador al intentar subir a caballo.
pub const DISMOUNT_FIRST: &str = "Primero bajate del caballo";

/// El jugador viaja en un tren, sentado (y oculto) en un coche.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct OnTrain(pub Ride);

/// El viaje con el que el jugador empieza el juego, si lo hay (la apertura).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct StartAboard(pub Option<Ride>);

/// Segundos reales que el tren avanza antes de que un salto haga el fundido a
/// negro.
pub const LEAVING_SECONDS: f32 = 3.0;

/// Segundos reales que un anuncio de estación queda en pantalla.
pub const ANNOUNCE_SECONDS: f32 = 3.0;

/// Segundos reales entre que el tren se detiene y el jugador se baja donde
/// termina el viaje: lo suficiente para leer "Estación …".
pub const ALIGHT_DELAY: f32 = 2.0;

/// Un salto en el tiempo que espera a que el fundido llegue al negro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// Saltar el viaje hasta justo antes de la siguiente parada.
    Approach,
    /// Esperar en este andén (el andén de la estación de un pueblo o el de su
    /// vía muerta) el próximo tren que pare en él.
    Wait(WaitingPlace),
}

/// En qué punto está un viaje saltado (Espacio a bordo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TripPhase {
    /// Arrancando de verdad, con esta cantidad de segundos reales antes del
    /// fundido.
    Leaving(f32),
    /// Fundido a negro, para saltar a justo antes de la siguiente parada.
    Fading,
    /// Entrando de verdad, hasta que el tren queda detenido en la estación.
    Arriving,
}

/// El viaje saltado en curso, si lo hay.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Trip(pub Option<TripPhase>);

/// El anuncio de estación en pantalla y sus segundos reales restantes.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct AnnounceTimer(pub f32);

/// Segundos reales restantes antes de que el jugador se baje donde termina el
/// viaje.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct PutOffIn(pub Option<f32>);

/// El salto que hará el fundido en curso, si lo hay.
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

/// Lo que leen del mundo los sistemas del viaje.
type Line<'w> = (
    Option<ResMut<'w, WorldTrains>>,
    Option<Res<'w, WorldTowns>>,
    ResMut<'w, GameWorld>,
);

/// El jugador tal como lo cambian los sistemas del viaje.
type Passenger<'a> = (
    Entity,
    &'a mut Player,
    &'a mut Transform,
    Option<&'a mut OnTrain>,
);

/// Lo que cambian las teclas del viaje además del jugador.
type RideState<'w> = (
    ResMut<'w, ScreenFade>,
    ResMut<'w, PendingJump>,
    ResMut<'w, Trip>,
    Option<ResMut<'w, WorldClock>>,
);

/// Hace `jump` (un salto de reloj del tren en el que se viaja: [`depart_now`],
/// [`skip_ride`], [`skip_to_approach`]) y lleva a todos los demás trenes junto
/// con el reloj.
fn ridden_jump<R>(
    trains: &mut [LineTrain],
    ridden: usize,
    clock: &mut GameClock,
    jump: impl FnOnce(&mut LineTrain, &mut GameClock) -> Option<R>,
) -> Option<R> {
    let from = clock.elapsed();
    let done = jump(trains.get_mut(ridden)?, clock);
    follow_jump(trains, ridden, from, clock.elapsed() - from, clock.rate());
    done
}

/// Las teclas: F sube, baja o espera; Espacio salta el viaje (o acorta un salto
/// en curso). Una tecla usada aquí se borra, para que el encargo no actúe
/// también sobre ella.
pub(crate) fn ride_keys(
    mut commands: Commands,
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    (mut trains, towns, mut world): Line,
    (mut fade, mut pending, mut trip, mut clock): RideState,
    mut players: Query<Passenger>,
    (mut say, mut announcer): (MessageWriter<Say>, Announcer),
    rail: Option<Res<WorldRail>>,
) {
    let Some(mut keys) = keys else {
        return;
    };
    let Ok((me, mut player, mut transform, aboard)) = players.single_mut() else {
        return;
    };
    let Some(trains) = trains.as_mut() else {
        return;
    };
    let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
    if fade.active() {
        keys.clear_just_pressed(INTERACT_KEY);
        keys.clear_just_pressed(SKIP_KEY);
        return;
    }
    if let Some(mut on_train) = aboard {
        let ridden = on_train.0.train;
        if ridden >= trains.0.len() {
            return;
        }
        if keys.clear_just_pressed(SKIP_KEY)
            && let Some(clock) = clock.as_mut()
        {
            match trip.0 {
                None => {
                    // Arrancar ya (el resto de la parada es invisible: el tren
                    // está quieto), y luego viajar un momento.
                    ridden_jump(&mut trains.0, ridden, &mut clock.0, depart_now);
                    world.0.moving = trains.obstacles();
                    trip.0 = Some(TripPhase::Leaving(LEAVING_SECONDS));
                    let train = &trains.0[ridden];
                    if train.state().phase == LinePhase::Running {
                        announcer.next_station(train, towns, &clock.0);
                    }
                }
                Some(TripPhase::Leaving(_)) => trip.0 = Some(TripPhase::Leaving(0.0)),
                Some(TripPhase::Arriving) => {
                    ridden_jump(&mut trains.0, ridden, &mut clock.0, skip_ride);
                    world.0.moving = trains.obstacles();
                }
                Some(TripPhase::Fading) => {}
            }
        }
        if keys.clear_just_pressed(INTERACT_KEY) {
            let train = &trains.0[ridden];
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
                // En marcha: bajarse en la siguiente parada.
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
    match boarding_at(&trains.0, towns, &player.rider, &world.0.floors) {
        Boarding::Board(ride) => {
            keys.clear_just_pressed(INTERACT_KEY);
            let train = &trains.0[ride.train];
            let ride = ride.facing_platform(train, towns, &world.0.floors);
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
            let bays = rail.as_ref().map_or(&[][..], |r| &r.0.bays[..]);
            if let Some(place) = waiting_place(&trains.0, towns, bays, &player.rider)
                && fade.start()
            {
                keys.clear_just_pressed(INTERACT_KEY);
                pending.0 = Some(Jump::Wait(place));
            }
        }
    }
}

/// Ejecuta el viaje saltado: después de arrancar, el fundido a negro; una vez
/// que el tren está detenido en la siguiente estación, el viaje termina.
fn run_trip(
    time: Res<Time>,
    mut trip: ResMut<Trip>,
    mut fade: ResMut<ScreenFade>,
    mut pending: ResMut<PendingJump>,
    trains: Option<Res<WorldTrains>>,
    aboard: Query<&OnTrain, With<Player>>,
) {
    let Ok(ride) = aboard.single() else {
        trip.0 = None;
        return;
    };
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
            let state = trains
                .as_ref()
                .and_then(|t| t.0.get(ride.0.train))
                .map(|t| t.state());
            if state.is_some_and(|s| s.phase != LinePhase::Running) {
                trip.0 = None;
            }
        }
        _ => {}
    }
}

/// Ejecuta el fundido y, en el negro total, hace el salto pendiente: el reloj y
/// el tren quedan justo antes de la siguiente parada (salto) o en el próximo
/// tren en el andén (espera).
fn jump_at_black(
    time: Res<Time>,
    mut fade: ResMut<ScreenFade>,
    mut pending: ResMut<PendingJump>,
    mut trip: ResMut<Trip>,
    clock: Option<ResMut<WorldClock>>,
    (trains, _, mut world): Line,
    aboard: Query<&OnTrain, With<Player>>,
) {
    if !fade.tick(time.delta_secs()) {
        return;
    }
    let jump = pending.0.take();
    let (Some(mut clock), Some(mut trains)) = (clock, trains) else {
        return;
    };
    match jump {
        Some(Jump::Approach) => {
            let ridden = aboard.single().map_or(0, |ride| ride.0.train);
            ridden_jump(&mut trains.0, ridden, &mut clock.0, skip_to_approach);
            trip.0 = Some(TripPhase::Arriving);
        }
        Some(Jump::Wait(place)) => {
            wait_for_train(&mut trains.0, &mut clock.0, place);
        }
        None => {}
    }
    world.0.moving = trains.obstacles();
}

/// Mantiene a un jugador sentado en su coche y el reloj al ritmo de a bordo, y
/// lo baja donde termina el viaje, [`ALIGHT_DELAY`] después de que el tren se
/// detiene; a pie, el reloj corre a su ritmo base.
fn follow_train(
    mut commands: Commands,
    time: Res<Time>,
    (clock, mut put_off_in): (Option<ResMut<WorldClock>>, ResMut<PutOffIn>),
    (trains, towns, world): Line,
    mut players: Query<Passenger>,
) {
    let Ok((me, mut player, mut transform, aboard)) = players.single_mut() else {
        return;
    };
    let train = aboard.as_ref().and_then(|on_train| {
        trains
            .as_ref()
            .and_then(|trains| trains.0.get(on_train.0.train))
    });
    let multiplier = match (aboard, train) {
        (Some(mut on_train), Some(train)) => {
            let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
            let ends = on_train.0.put_off(&train.state());
            let ride = on_train.0;
            // Se baja una vez leído "Estación …".
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

/// Escribe los anuncios de estación en el HUD.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Announcer<'w> {
    hud: ResMut<'w, HudAnnouncement>,
    timer: ResMut<'w, AnnounceTimer>,
}

impl Announcer<'_> {
    pub(crate) fn show(&mut self, text: String) {
        self.hud.0 = Some(text);
        self.timer.0 = ANNOUNCE_SECONDS;
    }

    /// "Próxima estación: …", la siguiente parada del tren.
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

/// El nombre del pueblo de `station`.
fn town_name<'a>(train: &LineTrain, towns: &'a [TownLayout], station: usize) -> Option<&'a str> {
    let town = train.stations().get(station)?.town;
    towns.get(town).map(|t| t.name.as_str())
}

/// Anuncia, a bordo, la siguiente estación cuando el tren arranca (o cuando el
/// jugador sube a un tren en marcha) y la estación cuando se detiene allí;
/// borra el anuncio después de [`ANNOUNCE_SECONDS`].
fn announce(
    time: Res<Time>,
    clock: Option<Res<WorldClock>>,
    (trains, towns, _): Line,
    aboard: Query<&OnTrain, With<Player>>,
    mut announcer: Announcer,
    mut last: Local<Option<(bool, Option<usize>)>>,
) {
    if announcer.timer.0 > 0.0 {
        announcer.timer.0 -= time.delta_secs();
        if announcer.timer.0 <= 0.0 {
            announcer.hud.0 = None;
        }
    }
    // El tren en el que se viaja; si no, el tren de la línea.
    let ridden = aboard.single().map_or(0, |ride| ride.0.train);
    let (Some(train), Some(clock)) = (trains.as_ref().and_then(|t| t.0.get(ridden)), clock) else {
        return;
    };
    let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
    let s = train.state();
    let standing = (s.phase != LinePhase::Running)
        .then_some(s.station)
        .flatten();
    let now = (aboard.single().is_ok(), standing);
    let before = last.replace(now);
    if !now.0 {
        return;
    }
    match (before, standing) {
        // Detenido en una estación.
        (Some((true, None)), Some(station)) => {
            if let Some(name) = town_name(train, towns, station) {
                announcer.show(format!("Estación {name}"));
            }
        }
        // Arrancó, o acaba de subir a un tren en marcha.
        (Some((true, Some(_))), None) | (None | Some((false, _)), None) => {
            announcer.next_station(train, towns, &clock.0);
        }
        _ => {}
    }
}

/// La línea de viaje del HUD a bordo: la siguiente estación y la hora a la
/// que el tren llega allí al ritmo de a bordo, p. ej.
/// `Próxima: Eduardo Castex, 11:22`.
fn show_ride(
    clock: Option<Res<WorldClock>>,
    (trains, towns, _): Line,
    players: Query<&OnTrain, With<Player>>,
    mut hud: ResMut<HudRide>,
) {
    let ride = players.single().ok();
    let aboard = ride.is_some();
    let text = (|| {
        let train = trains.as_ref()?.0.get(ride.map_or(0, |r| r.0.train))?;
        let (station, at) = next_arrival(train, &clock.as_ref()?.0)?;
        let town = towns.as_ref()?.0.get(train.stations()[station].town)?;
        Some(format!("Próxima: {}, {at}", town.name))
    })()
    .filter(|_| aboard);
    hud.set_if_neq(HudRide(text));
}

/// Sienta a `rider` en el coche del viaje, mirando hacia donde avanza el tren.
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

/// Lo que lee el bajarse del tren.
struct Alight<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    train: &'a LineTrain,
    towns: &'a [TownLayout],
    world: &'a pampa_core::World,
}

impl Alight<'_, '_, '_> {
    /// Baja al pasajero (la entidad `me`) en el andén junto a su coche, a pie y
    /// visible de nuevo. `false` (sigue a bordo) mientras el tren está en
    /// marcha o donde no hay andén.
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

/// Una app sin interfaz del mundo incluido para los tests del viaje y del
/// encargo.
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

    /// Segundos por frame.
    pub const FRAME: f32 = 0.05;

    pub fn shipped_lines() -> ErrandLines {
        let (lines, error) = load_errand_or_builtin(&asset_path(ERRAND_FILE));
        assert!(error.is_none(), "{error:?}");
        lines
    }

    /// El mundo incluido con el encargo. Sin `clock`, la apertura del juego: a
    /// bordo del tren de la mañana entrando a Monte Nievas. Con un `clock`, el
    /// tren llevado hasta allí desde la medianoche y el jugador a pie en el
    /// centro del andén de Monte Nievas.
    pub fn shipped_app_at(clock: Option<GameClock>) -> App {
        let loaded = crate::shipped_world::loaded();
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        world.lay_rail(&loaded.line);
        let (run, cast) = plan(&loaded.towns, &mut world, &shipped_lines().police_town)
            .expect("the shipped errand");
        let (start, mut trains, mut ride) = start_of_day(&loaded.line, &loaded.towns);
        let mut spawn = (loaded.towns[0].arrival(), Travel::OnFoot);
        let clock = match clock {
            None => {
                let seat = ride.and_then(|r| r.seat(&trains[r.train])).unwrap();
                spawn.0 = seat;
                start
            }
            Some(clock) => {
                ride = None;
                trains = LineTrain::all_of(&loaded.line, &loaded.towns, Timetable::default());
                for train in &mut trains {
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
            .insert_resource(WorldTowns(loaded.towns.clone()))
            .insert_resource(WorldClock(clock))
            .insert_resource(WorldTrains(trains))
            .insert_resource(WorldRail(loaded.line.clone()))
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
            .add_plugins((
                PlayerPlugin,
                TownLifePlugin,
                ErrandPlugin,
                RidePlugin,
                crate::timetable::TimetablePlugin,
            ));
        app
    }

    /// La apertura del juego.
    pub fn shipped_app() -> App {
        shipped_app_at(None)
    }

    /// Presiona `key` durante exactamente un frame.
    pub fn press(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    /// El cuerpo del jugador y si viaja en el tren.
    pub fn passenger(app: &mut App) -> (Rider, Option<Ride>) {
        let mut q = app.world_mut().query::<(&Player, Option<&OnTrain>)>();
        let (p, on) = q.single(app.world()).unwrap();
        (p.rider, on.map(|o| o.0))
    }

    /// Ejecuta frames hasta que termina el fundido (como máximo dos segundos).
    pub fn settle(app: &mut App) {
        for _ in 0..(2.0 / FRAME) as usize {
            app.update();
            if !app.world().resource::<ScreenFade>().active() {
                return;
            }
        }
        panic!("the fade never ended");
    }

    /// Ejecuta frames hasta que el jugador se baja del tren (como máximo
    /// `seconds`), y devuelve los segundos reales que llevó.
    pub fn until_off(app: &mut App, seconds: f32) -> f32 {
        let mut frames = 0;
        while passenger(app).1.is_some() {
            app.update();
            frames += 1;
            assert!((frames as f32) * FRAME < seconds, "never got off");
        }
        frames as f32 * FRAME
    }

    /// Salta el viaje de apertura (Espacio) y deja al jugador en Monte Nievas.
    pub fn arrive(app: &mut App) {
        app.update();
        press(app, SKIP_KEY);
        until_off(app, 20.0);
        assert_eq!(passenger(app).1, None, "off the train");
    }

    /// Salta el viaje que esté en curso (Espacio) hasta que el jugador se baja.
    pub fn arrive_from_anywhere(app: &mut App) {
        press(app, SKIP_KEY);
        until_off(app, 20.0);
    }

    /// El anuncio en pantalla.
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
        app.world().resource::<WorldTrains>().0[0].clone()
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

    /// El andén de la estación en la que está detenido el tren, si `at` está
    /// sobre él.
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
        // El HUD indica la siguiente estación y cuándo llega el tren allí.
        let nievas = train(&app).station_of_town(0).unwrap();
        let due = train(&app).arrival_time_at(Run::Up, nievas).unwrap();
        let ride_line = app.world().resource::<HudRide>().0.clone();
        assert_eq!(
            ride_line.as_deref(),
            Some(format!("Próxima: Monte Nievas, {due}").as_str())
        );
        assert!(
            ride_line
                .unwrap()
                .chars()
                .all(|c| pampa_render::lettering::supports(pampa_render::lettering::Face::Body, c))
        );
        assert_eq!(
            announcement(&app).as_deref(),
            Some("Próxima estación: Monte Nievas")
        );
        // El tren entra durante unos 50 segundos reales, y luego "Estación
        // Monte Nievas" se muestra un momento antes de que él se baje.
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
        // Entra a la hora de llegada del horario y se baja dos segundos reales
        // (24 segundos de juego) después.
        let at = clock(&app).time_of_day().seconds();
        let arrival = train(&app)
            .arrival_time_at(Run::Up, s.station.unwrap())
            .unwrap();
        let off = arrival.seconds()
            + f64::from(ALIGHT_DELAY) * ONBOARD_MULTIPLIER * pampa_core::BASE_RATE;
        assert!((at - off).abs() < 1.0, "{}", clock(&app).hh_mm());
        app.update();
        assert_eq!(clock(&app).multiplier, 1.0);
        assert_eq!(app.world().resource::<HudRide>().0, None, "line gone");
        // Comienza el encargo.
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
        // (a) Todavía entrando de verdad, "Próxima estación: Monte Nievas".
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
        // (b) El fundido, y en el negro el salto a 3 s de la llegada.
        while !app.world().resource::<ScreenFade>().active() {
            app.update();
        }
        while app.world().resource::<Trip>().0 == Some(TripPhase::Fading) {
            app.update();
        }
        // (c) Entrando de verdad, frenando: el estado puro a 3 s como máximo.
        let s = train(&app).state();
        assert_eq!(s.phase, LinePhase::Running);
        let (next, left) = train(&app).next_stop().unwrap();
        assert!(left <= pampa_core::APPROACH_LEAD && left > 2.0, "{left}");
        // (d) "Estación Monte Nievas", y luego se baja en el andén.
        until_off(&mut app, 10.0);
        let (rider, _) = passenger(&mut app);
        assert_eq!(
            on_platform(&app, rider.position).as_deref(),
            Some("Monte Nievas")
        );
        assert_eq!(announcement(&app).as_deref(), Some("Estación Monte Nievas"));
        let s = train(&app).state();
        assert_eq!((s.phase, s.station), (LinePhase::Dwelling, Some(next)));
        // Exacto: el reloj marca la llegada del horario más la espera de 2 s.
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
        // F junto al coche: a bordo, oculto, viajando a 12×.
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some(), "boarded");
        assert_eq!(visibility(&mut app), Visibility::Hidden);
        app.update();
        assert_eq!(clock(&app).multiplier, ONBOARD_MULTIPLIER);
        // F otra vez mientras está detenido allí: se baja otra vez, y luego
        // vuelve a subir.
        press(&mut app, INTERACT_KEY);
        let (rider, ride) = passenger(&mut app);
        assert_eq!(ride, None);
        assert_eq!(
            on_platform(&app, rider.position).as_deref(),
            Some("Monte Nievas")
        );
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some());
        // Espacio: hasta Castex, la terminal, donde termina el viaje; Espacio
        // otra vez acorta el arranque.
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
        // El encargo todavía apunta a Don Hidalgo, y el jugador camina.
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
        // Y puede subir al tren en espera en la terminal para volver.
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
        assert!(
            DISMOUNT_FIRST
                .chars()
                .all(|c| pampa_render::lettering::supports(pampa_render::lettering::Face::Body, c))
        );
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
        // El tren de la tarde desde Castex, que entra a la hora de llegada del
        // horario y se lee unos frames después.
        let due = train(&app).arrival_time_at(Run::Down, nievas).unwrap();
        let late = clock(&app).time_of_day().seconds() - due.seconds();
        assert!(
            (0.0..120.0).contains(&late),
            "{} for {due}",
            clock(&app).hh_mm()
        );
        assert_eq!(passenger(&mut app).1, None, "still on the platform");
        // Ahora F sube a él.
        let (middle, _) = app.world().resource::<WorldTowns>().0[0]
            .platform_spot()
            .unwrap();
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = middle;
        press(&mut app, INTERACT_KEY);
        assert!(passenger(&mut app).1.is_some());
    }

    const METILEO: usize = 2;
    const CALEUFU: usize = 4;

    /// Pone al jugador a pie en `at` en el mundo incluido.
    fn stand_at(app: &mut App, at: pampa_core::Vec2) {
        let world = app.world().resource::<GameWorld>().0.clone();
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider =
            Rider::on_foot(at, pampa_core::Heading::from_radians(0.0), &world);
    }

    fn shuttle(app: &App) -> LineTrain {
        app.world().resource::<WorldTrains>().0[1].clone()
    }

    #[test]
    fn waiting_at_caleufu_boards_the_shuttle_and_it_rides_to_metileo() {
        // 13:00: la lanzadera está en algún punto de su día, no en Caleufu.
        let mut app = shipped_app_at(Some(GameClock::at(TimeOfDay::hm(13, 0))));
        app.update();
        let towns = app.world().resource::<WorldTowns>().0.clone();
        let (middle, _) = towns[CALEUFU].platform_spot().unwrap();
        stand_at(&mut app, middle);
        let at_caleufu = shuttle(&app).station_of_town(CALEUFU).unwrap();
        assert!(!stands_at(&shuttle(&app).state(), at_caleufu));
        press(&mut app, INTERACT_KEY);
        assert!(app.world().resource::<ScreenFade>().active());
        settle(&mut app);
        assert!(stands_at(&shuttle(&app).state(), at_caleufu), "it came in");
        // F sube a ella desde el centro del andén, donde la espera dejó al
        // jugador: a la lanzadera, no al tren de la línea.
        press(&mut app, INTERACT_KEY);
        let (_, ride) = passenger(&mut app);
        assert_eq!(ride.expect("aboard").train, 1);
        // El tren en el que se viaja es el que sigue el viaje: hacia Metileo.
        // Espacio hace arrancar el tren en el que se viaja: el reloj salta a
        // la próxima salida de la lanzadera desde Caleufu, haga lo que haga el
        // tren de la línea.
        let now = clock(&app).elapsed();
        press(&mut app, SKIP_KEY);
        let day = 24.0 * 3600.0;
        let departs = shuttle(&app)
            .timetable()
            .departures(Run::Up)
            .iter()
            .map(|t| t.seconds() + ((now - t.seconds()) / day).ceil() * day)
            .fold(f64::INFINITY, f64::min);
        assert!(
            (clock(&app).elapsed() - departs).abs() < 30.0,
            "{} for {departs}",
            clock(&app).hh_mm()
        );
        until_off(&mut app, 20.0);
        let (rider, ride) = passenger(&mut app);
        assert_eq!(ride, None, "put off");
        let at_metileo = shuttle(&app).station_of_town(METILEO).unwrap();
        assert!(stands_at(&shuttle(&app).state(), at_metileo));
        assert!(
            rider
                .position
                .distance(towns[METILEO].platform_spot().unwrap().0)
                < 120.0
        );
        assert_eq!(announcement(&app).as_deref(), Some("Estación Metileo"));
    }

    /// Un punto en el andén de la vía muerta del mundo incluido junto a la
    /// puerta del coche de la lanzadera detenida.
    fn on_the_bay_platform(app: &App) -> pampa_core::Vec2 {
        let world = &app.world().resource::<GameWorld>().0;
        let placed = shuttle(app).placed_cars();
        let coach = placed
            .iter()
            .find(|c| c.kind == pampa_core::CarKind::Coach)
            .unwrap();
        let bay = app.world().resource::<WorldRail>().0.bays[0].platform;
        assert!(world.floors.contains(&bay));
        let (spot, _) = bay.spot_beside(coach.center);
        let door = pampa_core::coach_doors(coach)
            .into_iter()
            .min_by(|a, b| a.distance(spot).total_cmp(&b.distance(spot)))
            .unwrap();
        spot + bay.along * (door.dot(bay.along) - spot.dot(bay.along))
    }

    #[test]
    fn the_shuttle_in_metileos_bay_is_boarded_from_its_platform_not_from_the_ground() {
        // La lanzadera está detenida en la vía muerta de Metileo.
        let probe = shipped_app_at(Some(GameClock::at(TimeOfDay::hm(1, 0))));
        let stops = shuttle(&probe).stops_at(METILEO, Run::Up);
        let mut app = shipped_app_at(Some(GameClock::at(stops[0])));
        app.update();
        let shuttle = shuttle(&app);
        assert!(shuttle.stands_in_bay());
        let coach = shuttle
            .cars()
            .iter()
            .position(|c| c.kind == pampa_core::CarKind::Coach)
            .unwrap();
        // Desde el suelo junto a la puerta, fuera del andén: nada.
        let placed = shuttle.placed_cars()[coach];
        let platform = on_the_bay_platform(&app);
        let door = pampa_core::coach_doors(&placed)
            .into_iter()
            .min_by(|a, b| a.distance(platform).total_cmp(&b.distance(platform)))
            .unwrap();
        let across = pampa_core::Vec2::new(-placed.direction.y, placed.direction.x);
        let side = across * (platform - placed.center).dot(across).signum();
        // El suelo justo más allá de la puerta, del lado opuesto al andén (el
        // lado de la vía del tren de la línea), el lado propio de la vía
        // muerta.
        stand_at(&mut app, door - side * 1.0);
        press(&mut app, INTERACT_KEY);
        assert_eq!(passenger(&mut app).1, None, "not boarded from the ground");
        assert!(!app.world().resource::<ScreenFade>().active());
        // Desde el andén de la vía muerta, a la altura de la puerta: sube.
        stand_at(&mut app, platform);
        press(&mut app, INTERACT_KEY);
        let ride = passenger(&mut app).1.expect("boarded from the platform");
        assert_eq!((ride.train, ride.car), (1, coach));
    }

    #[test]
    fn each_of_metileos_platforms_waits_for_its_own_train() {
        let loaded = crate::shipped_world::loaded();
        let trains = LineTrain::all_of(
            &loaded.line,
            &loaded.towns,
            pampa_core::Timetable::default(),
        );
        let at = |k: u32| -> Vec<LineTrain> {
            trains
                .iter()
                .map(|t| {
                    let mut t = t.clone();
                    t.advance(0.0, f64::from(k) * 300.0, pampa_core::BASE_RATE);
                    t
                })
                .collect()
        };
        let metileo = |t: &LineTrain| {
            t.stations()
                .iter()
                .position(|s| s.town == METILEO && !s.bay)
                .or_else(|| t.stations().iter().position(|s| s.town == METILEO))
                .unwrap()
        };
        // Una hora del día en la que ningún tren está detenido en Metileo.
        let quiet = (0..24 * 12)
            .find(|&k| at(k).iter().all(|t| !stands_at(&t.state(), metileo(t))))
            .expect("a quiet moment at Metileo");
        let clock = GameClock::at(TimeOfDay::from_seconds(f64::from(quiet) * 300.0));
        let town = &loaded.towns[METILEO];
        let bay_spot = loaded.line.bays[0].platform.footprint.center();
        // Desde el andén de la estación: entra el tren de la línea; desde el
        // andén de la vía muerta: la lanzadera, en su vía muerta.
        for (spot, train) in [(town.platform_spot().unwrap().0, 0), (bay_spot, 1)] {
            let mut app = shipped_app_at(Some(clock));
            app.update();
            stand_at(&mut app, spot);
            press(&mut app, INTERACT_KEY);
            assert!(
                app.world().resource::<ScreenFade>().active(),
                "train {train}"
            );
            settle(&mut app);
            let trains = app.world().resource::<WorldTrains>().0.clone();
            let stands = |i: usize| stands_at(&trains[i].state(), metileo(&trains[i]));
            assert!(stands(train), "train {train} came in");
            if train == 1 {
                assert!(trains[1].stands_in_bay());
            }
        }
    }
}
