//! La vida en el pueblo: el reloj del juego, el tren de la línea, los vecinos
//! que caminan o cabalgan ([`Horsemen`]) por las calles, el caballo atado
//! frente a cada almacén de ramos
//! generales, y las personas del encargo paradas en sus pueblos o sentadas en
//! su pulpería, hasta que se van ([`Leaving`]), con el puesto de quesos del
//! Viejo Amadeo ([`StallTown`]).
//!
//! La lógica vive en `pampa-core`; este módulo la hace avanzar una vez por
//! frame, antes que al jugador, deja que el jinete aparte a los peatones
//! después de que el jugador se movió, y copia los resultados en entidades que
//! dibuja render.
//!
//! Cada pueblo tiene su propia gente, sembrada a partir de la semilla del
//! pueblo. Solo los pueblos cercanos al jugador ([`ActiveTowns`]) tienen a su
//! gente y su caballo atado creados y simulados; sin [`ActiveTowns`] (sin
//! streaming de pueblos), lo está la gente de todos. Las personas del encargo
//! ([`Cast`]) vienen y van con sus pueblos de la misma forma, y un caballo que
//! se llevaron ([`LedAway`]) no se vuelve a atar. El reloj avanza en cada frame
//! y el único tren de la línea lo sigue en todas partes: las salidas salen del
//! reloj, la marcha va en segundos reales, y sus vagones son siempre obstáculos
//! en movimiento (ocho vagones: barato).

use std::collections::BTreeSet;

use bevy::prelude::*;
use pampa_core::{
    Aabb, AnimationState, BASE_RATE, Building, Errand, GameClock, Heading, LineTrain, MAX_STEP,
    Npc, Parked, RailLine, Ride, Run, Stall, Standing, Timetable, TownLayout, TownRiders,
    Townsfolk, Travel, Vec2, opening, pulperia,
};
use pampa_render::character::{Fading, Rising};
use pampa_render::{
    ActiveTowns, CharacterVisual, CheeseStall, Coat, FigureKind, MapIcon, Rolled, VehicleVisual,
    VisualPose, WorldClock, WorldTowns, WorldTrains, world_from_ground,
};

use crate::player::{DrivePlayer, GameWorld, Player, drive_player};
use crate::ride::OnTrain;

/// El reloj, los trenes y el viaje del jugador al comenzar el juego (la
/// apertura): el protagonista viene de General Pico, así que viaja en el tren
/// de la mañana (ascendente) desde Pico, poco antes de que entre a la estación
/// del primer pueblo ([`pampa_core::OPENING_LEAD`] segundos reales), y allí lo
/// bajan. El reloj corre al ritmo de a bordo y llega a la hora de llegada del
/// horario, a media mañana, cuando el tren se detiene. Sin tren (o sin estación
/// en el primer pueblo) no hay viaje, y el reloj empieza en la hora de salida
/// matutina desde Pico. Los trenes son el tren de la línea y la lanzadera del
/// ramal ([`LineTrain::all_of`]); la lanzadera se lleva a la hora de la
/// apertura desde la medianoche, como lo haría el reloj.
pub fn start_of_day(
    line: &RailLine,
    towns: &[TownLayout],
) -> (GameClock, Vec<LineTrain>, Option<Ride>) {
    let mut trains = LineTrain::all_of(line, towns, Timetable::default());
    let start = trains.first_mut().and_then(|t| opening(t, 0, Run::Up));
    match start {
        Some((clock, ride)) => {
            // Sentado junto a la ventanilla del lado del andén de Monte Nievas.
            let ride = ride.facing_platform(&trains[0], towns, &line.floors());
            for shuttle in trains.iter_mut().skip(1) {
                shuttle.advance(0.0, clock.elapsed(), BASE_RATE);
            }
            (clock, trains, Some(ride))
        }
        None => (
            GameClock::at(Timetable::default().departure(Run::Up)),
            trains,
            None,
        ),
    }
}

/// Se mezcla con la semilla de cada pueblo para decidir dónde empieza y por
/// dónde deambula su gente.
const TOWNSFOLK_SEED: u32 = 1927;

/// Todos los que caminan por cada pueblo, por índice de pueblo.
#[derive(Resource, Debug, Clone, Default)]
pub struct Crowds(pub Vec<Townsfolk>);

impl Crowds {
    /// Gente para cada pueblo, sembrada a partir de la semilla del pueblo.
    pub fn for_towns(towns: &[TownLayout]) -> Self {
        Self(
            towns
                .iter()
                .map(|t| Townsfolk::new(t, t.townsfolk_count(), crowd_seed(t.seed)))
                .collect(),
        )
    }
}

/// La semilla de la gente de un pueblo con semilla `seed`.
pub fn crowd_seed(seed: u32) -> u32 {
    TOWNSFOLK_SEED ^ seed.wrapping_mul(0x9e37_79b9)
}

/// Se mezcla con la semilla de la gente de cada pueblo para ubicar a sus
/// jinetes.
const RIDERS_SEED: u32 = 0x6a69_6e65;

/// Los vecinos que recorren cada pueblo a caballo, por índice de pueblo.
#[derive(Resource, Debug, Clone, Default)]
pub struct Horsemen(pub Vec<TownRiders>);

impl Horsemen {
    /// Los jinetes y los sulkies de cada pueblo, tantos como su tamaño,
    /// sembrados a partir de la semilla del pueblo.
    pub fn for_towns(towns: &[TownLayout]) -> Self {
        Self(
            towns
                .iter()
                .map(|t| {
                    let seed = crowd_seed(t.seed) ^ RIDERS_SEED;
                    TownRiders::new(t, t.riders_count(), seed).with_sulkies(t.sulkies_count(), seed)
                })
                .collect(),
        )
    }
}

/// Los vehículos estacionados de cada pueblo, por índice de pueblo
/// ([`pampa_core::parked_in`]): quietos, con sus animales, y obstáculos para
/// el jugador, los peatones y los jinetes.
#[derive(Resource, Debug, Clone, Default)]
pub struct ParkedVehicles(pub Vec<Vec<Parked>>);

impl ParkedVehicles {
    pub fn for_towns(towns: &[TownLayout]) -> Self {
        Self(towns.iter().map(pampa_core::parked_in).collect())
    }

    /// Lo que ocupan los de `town`.
    pub fn bodies(&self, town: usize) -> Vec<pampa_core::Body> {
        self.0
            .get(town)
            .map_or_else(Vec::new, |p| p.iter().flat_map(Parked::bodies).collect())
    }
}

/// Entidad que dibuja el vehículo estacionado `index` del pueblo `town`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParkedVehicle {
    pub town: usize,
    pub index: usize,
}

/// Entidad que dibuja al jinete (o al sulky) `index` de los del pueblo `town`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Horseman {
    pub town: usize,
    pub index: usize,
}

/// Entidad que dibuja a la persona `index` de la gente del pueblo `town`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walker {
    pub town: usize,
    pub index: usize,
}

/// El caballo atado en el palenque del pueblo `.0`.
#[derive(Component, Debug, Clone, Copy)]
pub struct TetheredHorse(pub usize);

/// Un caballo atado en uno de los palenques del pueblo `town` (los de una
/// ciudad, además del [`TetheredHorse`] del almacén): solo está parado allí.
/// `index` es el lugar del palenque en [`TownLayout::palenques`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PalenqueHorse {
    pub town: usize,
    pub index: usize,
}

/// Una persona del encargo parada en el pueblo `town`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandingNpc {
    pub town: usize,
    pub npc: Npc,
}

/// Las personas del encargo, creadas con sus pueblos: las que están paradas,
/// y dónde; luego las que esperan sentadas en la pulpería de su pueblo
/// ([`AtTable`]: Bairoletto), hasta que se levantan y se van; y los puestos de
/// la vereda (el de quesos del Viejo Amadeo).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Cast(pub Vec<(Npc, Standing)>, pub Vec<AtTable>, pub Vec<Stall>);

/// Un puesto de la vereda del pueblo `.0`, que dibuja render
/// ([`CheeseStall`]); viene y va con su pueblo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StallTown(pub usize);

impl Cast {
    /// Las personas de `errand`: Don Hidalgo, Farach y el Viejo Amadeo
    /// (detrás de su puesto) parados, y Bairoletto sentado en su asiento de la
    /// pulpería (o parado junto a Farach si ese pueblo no tiene pulpería).
    pub fn of(errand: &Errand) -> Self {
        let mut standing: Vec<(Npc, Standing)> = [Npc::Hidalgo, Npc::Farach, Npc::Amadeo]
            .map(|npc| (npc, errand.standing(npc)))
            .to_vec();
        let him = errand.standing(Npc::Bairoletto);
        let seated = match errand.bairoletto_seat() {
            Some(seat) => vec![AtTable {
                npc: Npc::Bairoletto,
                town: him.town,
                seat,
            }],
            None => {
                standing.push((Npc::Bairoletto, him));
                Vec::new()
            }
        };
        Self(standing, seated, vec![errand.stall()])
    }
}

/// Una persona del encargo que espera sentada en el asiento `seat` de la
/// pulpería del pueblo `town` ([`pulperia::Interior::seats`]). Ningún
/// parroquiano ocupa su asiento, ni de día su mesa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtTable {
    pub npc: Npc,
    pub town: usize,
    pub seat: usize,
}

/// Alguien que se va de la pulpería del pueblo `town`: primero se para del
/// banco ([`Rising`], dibujado todavía sentado), y después sale caminando como
/// `figure` por `way` ([`pulperia::Interior::way_out`]) hasta perderse de
/// vista; `next` es el índice del próximo punto. Adentro pisa el piso de
/// ladrillo de la sala (`room`, a la altura `floor`); afuera, el suelo. Se va
/// también si el pueblo se descarga.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Leaving {
    pub town: usize,
    pub figure: FigureKind,
    pub way: Vec<Vec2>,
    pub next: usize,
    pub room: Aabb,
    pub floor: f32,
}

/// El paso de quien se va de la pulpería, en m/s: un andar tranquilo.
pub const LEAVING_PACE: f32 = 1.3;

/// Cuánto tarda en difuminarse quien se va de la pulpería, al final de su
/// camino, en segundos: sigue caminando mientras se desvanece.
pub const FADE_SECONDS: f32 = 1.5;

/// Cuánto tarda en pararse del banco quien se va de la pulpería, en segundos.
pub const RISE_SECONDS: f32 = 1.2;

/// Qué tan rápido gira hacia su camino quien se va de la pulpería, en
/// radianes por segundo: un giro de cuarto de vuelta en algo más de medio
/// segundo.
pub const LEAVING_TURN_RATE: f32 = 3.0;

/// Un parroquiano sentado a una mesa de la pulpería del pueblo `town`, en el
/// asiento `seat` de su interior ([`pampa_core::pulperia::Interior::seats`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patron {
    pub town: usize,
    pub seat: usize,
}

/// Quien atiende un local del pueblo `town`: el pulpero o el almacenero del
/// edificio `room` (su índice en los `buildings`), parado detrás del
/// mostrador, de cara a la calle ([`pulperia::Interior::keeper`]). Está
/// siempre, abierto o cerrado, y se va con su pueblo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keeper {
    pub town: usize,
    pub room: usize,
}

/// Se mezcla con la semilla de la gente de cada pueblo para elegir el aspecto
/// de quien atiende cada local.
const KEEPER_SEED: u32 = 0x6b65_6570;

/// Se mezcla con la semilla de la gente de cada pueblo para sentar a los
/// parroquianos de su pulpería.
const PATRONS_SEED: u32 = 0x7061_7472;

/// Pueblos cuyo caballo atado se llevaron en esta sesión: no se vuelve a atar
/// allí.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct LedAway(pub BTreeSet<usize>);

/// La figura de una persona del encargo.
pub fn npc_figure(npc: Npc) -> FigureKind {
    match npc {
        Npc::Hidalgo => FigureKind::Hidalgo,
        Npc::Farach => FigureKind::Farach,
        Npc::Bairoletto => FigureKind::Bairoletto,
        // Un vecino de siempre: el paisano de boina, camisa clara, bombacha
        // oscura y faja roja, sin modelo propio.
        Npc::Amadeo => FigureKind::Townsfolk(0),
    }
}

/// Pueblos cuya gente y caballo están creados.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct Populated(pub BTreeSet<usize>);

/// Sistemas que hacen avanzar el reloj, el tren y los pueblos alrededor del
/// jugador; se ejecutan antes que él para que el jinete colisione con el tren
/// de este frame.
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
                (
                    populate_towns,
                    run_clock_and_train,
                    seat_patrons,
                    walk_away,
                    walk_crowds,
                )
                    .chain()
                    .in_set(SimulateTown),
            )
            .add_systems(
                Update,
                (jostle_crowd, (place_walkers, place_horsemen))
                    .chain()
                    .in_set(DrivePlayer)
                    .after(drive_player),
            );
    }
}

/// Avisa sobre las estaciones junto a las que el tren de la línea no puede
/// detenerse (demasiado cerca de una punta de la línea).
fn report_train(trains: Option<Res<WorldTrains>>, towns: Option<Res<WorldTowns>>) {
    let Some(trains) = trains else {
        return;
    };
    for (index, train) in trains.0.iter().enumerate() {
        let which = if index == 0 { "line train" } else { "shuttle" };
        for station in train.misplaced_stops() {
            let town = train.stations()[station].town;
            let name = towns
                .as_ref()
                .and_then(|t| t.0.get(town))
                .map_or("?", |t| t.name.as_str());
            warn!("{which}: the coaches cannot stop alongside the station of `{name}`");
        }
    }
}

/// Índices de la gente a simular: los pueblos activos, o la de todos cuando los
/// pueblos no hacen streaming.
fn live_towns(crowds: &Crowds, active: Option<&ActiveTowns>) -> Vec<usize> {
    match active {
        Some(active) => active.0.active().filter(|&i| i < crowds.0.len()).collect(),
        None => (0..crowds.0.len()).collect(),
    }
}

/// Recursos que lee [`populate_towns`].
type TownInputs<'a> = (
    Option<Res<'a, Crowds>>,
    Option<Res<'a, WorldTowns>>,
    Option<Res<'a, ActiveTowns>>,
    Res<'a, Cast>,
    Res<'a, LedAway>,
    Option<Res<'a, Horsemen>>,
    Option<Res<'a, ParkedVehicles>>,
);

/// Todos los que viven en un pueblo: peatones, el caballo atado y las personas
/// del encargo.
type Residents<'a> = (
    Entity,
    Option<&'a Walker>,
    Option<&'a TetheredHorse>,
    Option<&'a PalenqueHorse>,
    Option<&'a StandingNpc>,
    Option<&'a Leaving>,
    Option<&'a Horseman>,
    Option<&'a ParkedVehicle>,
    Option<&'a Patron>,
    Option<&'a StallTown>,
    Option<&'a Keeper>,
);

/// Crea a la gente y el caballo atado de los pueblos que acaban de activarse, y
/// elimina los de los pueblos que quedaron atrás.
fn populate_towns(
    mut commands: Commands,
    (crowds, towns, active, cast, led_away, horsemen, parked): TownInputs,
    world: Res<GameWorld>,
    mut populated: ResMut<Populated>,
    residents: Query<Residents>,
) {
    let Some(crowds) = crowds else {
        return;
    };
    let wanted: BTreeSet<usize> = live_towns(&crowds, active.as_deref()).into_iter().collect();
    for (entity, walker, horse, palenque, npc, leaving, horseman, vehicle, patron, stall, keeper) in
        &residents
    {
        let town = walker
            .map(|w| w.town)
            .or(horse.map(|h| h.0))
            .or(palenque.map(|p| p.town))
            .or(npc.map(|n| n.town))
            .or(leaving.map(|l| l.town))
            .or(horseman.map(|h| h.town))
            .or(vehicle.map(|v| v.town))
            .or(patron.map(|p| p.town))
            .or(stall.map(|s| s.0))
            .or(keeper.map(|k| k.town));
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
        if let Some(riders) = horsemen.as_ref().and_then(|h| h.0.get(town)) {
            spawn_horsemen(&mut commands, riders, town, &world);
        }
        if let Some(vehicles) = parked.as_ref().and_then(|p| p.0.get(town)) {
            spawn_parked(&mut commands, vehicles, town, &world);
        }
        if let Some(layout) = towns.as_ref().and_then(|t| t.0.get(town))
            && !led_away.0.contains(&town)
        {
            let coat = tethered_coat(layout, &cast, town);
            spawn_tethered_horse(&mut commands, layout, town, coat, &world);
            spawn_palenque_horses(&mut commands, layout, town, &world);
        }
        for &(npc, standing) in cast.0.iter().filter(|(_, s)| s.town == town) {
            spawn_standing_npc(&mut commands, npc, standing, &world);
        }
        for stall in cast.2.iter().filter(|s| s.town == town) {
            let height = world.0.ground_height(stall.position);
            commands.spawn((
                Name::new("Cheese stall"),
                StallTown(town),
                CheeseStall,
                Transform::from_translation(world_from_ground(stall.position, height))
                    .with_rotation(Quat::from_rotation_y(stall.facing.radians())),
            ));
        }
        if let Some(layout) = towns.as_ref().and_then(|t| t.0.get(town)) {
            spawn_keepers(&mut commands, layout, town, &world);
        }
        let pulperia = towns.as_ref().and_then(|t| t.0.get(town)?.pulperia());
        if let Some((store, inside)) = pulperia {
            for sitter in cast.1.iter().filter(|a| a.town == town) {
                let seat = inside.seats[sitter.seat];
                commands.spawn((
                    Name::new(format!("{:?}", sitter.npc)),
                    StandingNpc {
                        town,
                        npc: sitter.npc,
                    },
                    CharacterVisual::Seated(npc_figure(sitter.npc), pampa_core::pulperia::Pastime::Rest),
                    VisualPose {
                        heading: seat.facing,
                        ..default()
                    },
                    on_bench(store, seat, &world),
                ));
            }
        }
    }
}

/// Sienta a los parroquianos de la pulpería de cada pueblo poblado según la
/// hora ([`pulperia::Interior::patrons`]): en los bancos, mirando hacia su
/// mesa. El banco ya es un obstáculo, así que no necesitan cuerpo. Cambian solo
/// con el jugador afuera, para que nadie aparezca ni desaparezca a la vista.
fn seat_patrons(
    mut commands: Commands,
    (towns, clock, populated): (
        Option<Res<WorldTowns>>,
        Option<Res<WorldClock>>,
        Res<Populated>,
    ),
    (world, cast): (Res<GameWorld>, Res<Cast>),
    players: Query<&Player>,
    seated: Query<(Entity, &Patron)>,
) {
    let (Some(towns), Some(clock)) = (towns, clock) else {
        return;
    };
    let time = clock.0.time_of_day();
    let at = players.single().ok().map(|p| p.rider.position);
    for &town in &populated.0 {
        let Some(layout) = towns.0.get(town) else {
            continue;
        };
        let Some((store, inside)) = layout.pulperia() else {
            continue;
        };
        let reserved: Vec<usize> = cast
            .1
            .iter()
            .filter(|a| a.town == town)
            .map(|a| a.seat)
            .collect();
        let want = inside.patrons(crowd_seed(layout.seed) ^ PATRONS_SEED, time, &reserved);
        let here = || seated.iter().filter(|(_, p)| p.town == town);
        let have: BTreeSet<usize> = here().map(|(_, p)| p.seat).collect();
        if have == want.iter().map(|p| p.seat).collect() || at.is_some_and(|p| inside.holds(p)) {
            continue;
        }
        for (entity, _) in here() {
            commands.entity(entity).despawn();
        }
        for patron in want {
            let seat = inside.seats[patron.seat];
            commands.spawn((
                Name::new(format!("Patron {town}.{}", patron.seat)),
                Patron {
                    town,
                    seat: patron.seat,
                },
                CharacterVisual::Seated(FigureKind::townsfolk(patron.look), patron.pastime),
                VisualPose {
                    heading: seat.facing,
                    ..default()
                },
                on_bench(store, seat, &world),
            ));
        }
    }
}

/// Dónde está alguien sentado en el asiento `seat` de la pulpería `store`:
/// sobre el banco, en el piso de ladrillo de la sala ([`pulperia_floor`]).
fn on_bench(store: &Building, seat: pulperia::Seat, world: &GameWorld) -> Transform {
    Transform::from_translation(world_from_ground(
        seat.position,
        pulperia_floor(store, world) + pulperia::BENCH_HEIGHT,
    ))
}

/// La altura del piso de ladrillo de la pulpería `store`, la misma con la que
/// el render dibuja la sala.
pub fn pulperia_floor(store: &Building, world: &GameWorld) -> f32 {
    let middle = store.footprint.center();
    world.0.terrain.height(middle.x, middle.y) + pulperia::FLOOR_RISE
}

/// Hace pararse del banco a quien se va de la pulpería ([`Leaving`]), en
/// [`RISE_SECONDS`], y después lo hace caminar por su camino con el ciclo de
/// caminata, a [`LEAVING_PACE`], con los pies en el piso; gira hacia el camino
/// a [`LEAVING_TURN_RATE`]. Durante los últimos [`FADE_SECONDS`] del camino se
/// difumina ([`Fading`]), y al final se va. No choca con nada: su camino ya va
/// por piso libre.
fn walk_away(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<GameWorld>,
    mut leaving: Query<(
        Entity,
        &mut Leaving,
        &mut Transform,
        &mut VisualPose,
        Option<&mut Rising>,
    )>,
) {
    let dt = time.delta_secs().min(MAX_STEP);
    for (entity, mut walk, mut transform, mut pose, rising) in &mut leaving {
        let mut at = Vec2::new(transform.translation.x, -transform.translation.z);
        if let Some(mut rising) = rising {
            rising.0 += dt / RISE_SECONDS;
            if rising.0 < 1.0 {
                continue;
            }
            // De pie junto al banco: ya es una figura a pie, y arranca.
            commands
                .entity(entity)
                .remove::<Rising>()
                .insert(CharacterVisual::Figure(walk.figure));
        }
        let mut step = LEAVING_PACE * dt;
        while let Some(&to) = walk.way.get(walk.next) {
            let gap = at.distance(to);
            if gap > step {
                let toward = (to - at) * (1.0 / gap);
                at += toward * step;
                pose.heading = turn_toward(pose.heading, toward, LEAVING_TURN_RATE * dt);
                break;
            }
            at = to;
            step -= gap;
            walk.next += 1;
        }
        if walk.next >= walk.way.len() {
            commands.entity(entity).despawn();
            continue;
        }
        let left = at.distance(walk.way[walk.next])
            + walk.way[walk.next..]
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .sum::<f32>();
        let visible = left / (LEAVING_PACE * FADE_SECONDS);
        if visible < 1.0 {
            commands.entity(entity).insert(Fading(visible));
        }
        pose.speed = LEAVING_PACE;
        pose.animation = AnimationState::Walk;
        transform.translation = world_from_ground(at, walk.ground(at, &world));
    }
}

impl Leaving {
    /// La altura del piso que pisa en `at`: el de ladrillo dentro de la sala,
    /// el suelo afuera.
    fn ground(&self, at: Vec2, world: &GameWorld) -> f32 {
        if self.room.contains_point(at) {
            self.floor
        } else {
            world.0.ground_height(at)
        }
    }
}

/// `heading` girado hacia `toward` a lo sumo `max` radianes, por el lado corto.
fn turn_toward(heading: Heading, toward: Vec2, max: f32) -> Heading {
    let goal = toward.y.atan2(toward.x);
    let delta = (goal - heading.radians() + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    heading.rotated(delta.clamp(-max, max))
}

/// Quien atiende cada local de `layout` (la pulpería y los almacenes),
/// parado en el piso de ladrillo detrás de su mostrador, con un aspecto de
/// vecino propio del pueblo y del local.
fn spawn_keepers(commands: &mut Commands, layout: &TownLayout, town: usize, world: &GameWorld) {
    for (room, store, inside) in layout.rooms() {
        let mix = (crowd_seed(layout.seed) ^ KEEPER_SEED)
            .wrapping_add(room as u32)
            .wrapping_mul(0x9e37_79b9);
        let keeper = inside.keeper;
        commands.spawn((
            Name::new(format!("Keeper {town}.{room}")),
            Keeper { town, room },
            CharacterVisual::Figure(FigureKind::townsfolk((mix >> 24) as u8)),
            VisualPose {
                heading: keeper.facing,
                ..default()
            },
            Transform::from_translation(world_from_ground(
                keeper.position,
                pulperia_floor(store, world),
            )),
        ));
    }
}

/// Una persona del encargo parada, quieta donde la puso el encargo.
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
                turn: 0.0,
                flight: None,
            },
            Transform::from_translation(world_from_ground(person.position, height)),
        ));
    }
}

/// Los vehículos estacionados de `town`, quietos con sus animales y su
/// conductor sentado, salvo los que esperan solos ([`Parked::driven`]).
fn spawn_parked(commands: &mut Commands, vehicles: &[Parked], town: usize, world: &GameWorld) {
    for (index, p) in vehicles.iter().enumerate() {
        let height = world.0.ground_height(p.position);
        let visual = VehicleVisual::seeded(p.vehicle, (p.seed >> 24) as u8, p.seed);
        commands.spawn((
            Name::new(format!("Parked {:?} {town}.{index}", p.vehicle)),
            ParkedVehicle { town, index },
            if p.driven() {
                visual
            } else {
                visual.driverless()
            },
            VisualPose {
                heading: p.heading,
                ..default()
            },
            Rolled(0.0),
            Transform::from_translation(world_from_ground(p.position, height)),
        ));
    }
}

fn spawn_horsemen(commands: &mut Commands, riders: &TownRiders, town: usize, world: &GameWorld) {
    for (index, rider) in riders.riders.iter().enumerate() {
        let height = world.0.ground_height(rider.position);
        let mut horseman = commands.spawn(Horseman { town, index });
        match rider.vehicle {
            Some(vehicle) => horseman.insert((
                Name::new(format!("Sulky {town}.{index}")),
                VehicleVisual::seeded(vehicle, rider.look, rider.coat),
                Rolled(rider.travelled),
            )),
            None => horseman.insert((
                Name::new(format!("Horseman {town}.{index}")),
                CharacterVisual::Mounted {
                    rider: FigureKind::townsfolk(rider.look),
                    coat: Coat::seeded(rider.coat),
                },
            )),
        };
        horseman.insert((
            VisualPose {
                heading: rider.heading,
                animation: rider.animation(),
                speed: rider.speed,
                turn: 0.0,
                flight: None,
            },
            Transform::from_translation(world_from_ground(rider.position, height)),
        ));
    }
}

/// El pelaje del caballo atado en el palenque principal de `town`: el alazán de Don Hidalgo
/// en su pueblo (el caballo que presta); si no, uno sembrado a partir del
/// pueblo.
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

/// Los caballos atados en los palenques de `layout` (los de una ciudad), cada
/// uno con su propio pelaje sembrado; escenografía, no caballos que preste el
/// encargo.
fn spawn_palenque_horses(
    commands: &mut Commands,
    layout: &TownLayout,
    town: usize,
    world: &GameWorld,
) {
    for (index, post) in layout.palenques.iter().enumerate() {
        let Some(horse) = post.horse else {
            continue;
        };
        let height = world.0.ground_height(horse.position);
        commands.spawn((
            Name::new(format!("Palenque horse {town}.{index}")),
            PalenqueHorse { town, index },
            CharacterVisual::Horse(Coat::seeded(layout.seed ^ 0x7a1e ^ index as u32)),
            VisualPose {
                heading: horse.heading,
                ..default()
            },
            Transform::from_translation(world_from_ground(horse.position, height)),
        ));
    }
}

/// Hace avanzar el reloj del juego, deja que cada tren lo siga y entrega sus
/// vagones al mundo lógico como obstáculos en movimiento.
pub(crate) fn run_clock_and_train(
    time: Res<Time>,
    clock: Option<ResMut<WorldClock>>,
    trains: Option<ResMut<WorldTrains>>,
    mut world: ResMut<GameWorld>,
) {
    let Some(mut clock) = clock else {
        return;
    };
    let before = clock.0.elapsed();
    // Limitado como el paso del jinete, para que un tirón de frame no haga
    // saltar el tren decenas de metros sobre un jinete que estaba despejado en
    // el frame anterior.
    let game = clock.0.tick(time.delta_secs().min(MAX_STEP));
    let Some(mut trains) = trains else {
        world.0.moving.clear();
        return;
    };
    for train in &mut trains.0 {
        train.advance(before, game, clock.0.rate());
    }
    world.0.moving = trains.obstacles();
}

/// Hace avanzar a la gente y a los jinetes activos: los jinetes frenan ante el
/// jugador y los peatones, y los peatones esquivan al jugador y a los caballos,
/// todos donde estaban en el frame anterior.
fn walk_crowds(
    time: Res<Time>,
    crowds: Option<ResMut<Crowds>>,
    horsemen: Option<ResMut<Horsemen>>,
    (active, parked): (Option<Res<ActiveTowns>>, Option<Res<ParkedVehicles>>),
    world: Res<GameWorld>,
    players: Query<&Player>,
) {
    let Some(mut crowds) = crowds else {
        return;
    };
    let mut horsemen = horsemen;
    let player: Vec<_> = players.iter().map(|p| p.rider.body()).collect();
    let dt = time.delta_secs().min(MAX_STEP);
    for town in live_towns(&crowds, active.as_deref()) {
        let mut obstacles = player.clone();
        obstacles.extend(parked.as_ref().map_or_else(Vec::new, |p| p.bodies(town)));
        if let Some(riders) = horsemen.as_mut().and_then(|h| h.0.get_mut(town)) {
            let mut around = obstacles.clone();
            around.extend(crowds.0[town].bodies());
            riders.step_around(&world.0, &around, dt);
            obstacles.extend(riders.bodies());
        }
        crowds.0[town].step_around(&world.0, &obstacles, dt);
    }
}

/// El jinete empujó a estos peatones (índices de la gente del pueblo `town`)
/// mientras se movía a `speed` (m/s, con signo).
#[derive(Message, Debug, Clone, PartialEq)]
pub struct Shoved {
    pub town: usize,
    pub walkers: Vec<usize>,
    pub speed: f32,
}

/// El jugador se abre paso: los peatones en el camino son apartados a
/// empujones, y cualquiera aplastado contra una pared detiene en cambio al
/// jugador. Solo se informan los empujones de un jinete ([`Shoved`]): la gente
/// le abre paso sin quejarse a alguien que va a pie.
pub(crate) fn jostle_crowd(
    crowds: Option<ResMut<Crowds>>,
    (horsemen, parked): (Option<Res<Horsemen>>, Option<Res<ParkedVehicles>>),
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
        // Los vecinos solo se quejan del caballo.
        let riding = rider.travel() == Travel::Riding;
        for &town in &live {
            let crowd = &mut crowds.0[town];
            let walkers = crowd.make_way(&world.0, rider.body());
            if riding && !walkers.is_empty() {
                // La velocidad del impacto, antes de que un peatón aplastado
                // detenga al jinete.
                shoved.write(Shoved {
                    town,
                    walkers,
                    speed: rider.speed,
                });
            }
            bodies.extend(crowd.bodies());
            // Los caballos no se apartan: el jugador se detiene contra ellos.
            if let Some(riders) = horsemen.as_ref().and_then(|h| h.0.get(town)) {
                bodies.extend(riders.bodies());
            }
            // Ni los vehículos estacionados.
            bodies.extend(parked.as_ref().map_or_else(Vec::new, |p| p.bodies(town)));
        }
        rider.keep_clear_of(&world.0, &bodies);
        transform.translation = world_from_ground(rider.position, rider.elevation());
    }
}

/// Copia el estado lógico de la gente en las entidades de los peatones.
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
            turn: 0.0,
            flight: None,
        };
    }
}

/// Copia el estado lógico de los jinetes en sus entidades.
fn place_horsemen(
    horsemen: Option<Res<Horsemen>>,
    world: Res<GameWorld>,
    mut entities: Query<
        (
            &Horseman,
            &mut Transform,
            &mut VisualPose,
            Option<&mut Rolled>,
        ),
        Without<Player>,
    >,
) {
    let Some(horsemen) = horsemen else {
        return;
    };
    for (horseman, mut transform, mut pose, rolled) in &mut entities {
        let Some(rider) = horsemen
            .0
            .get(horseman.town)
            .and_then(|h| h.riders.get(horseman.index))
        else {
            continue;
        };
        let height = world.0.ground_height(rider.position);
        transform.translation = world_from_ground(rider.position, height);
        *pose = VisualPose {
            heading: rider.heading,
            animation: rider.animation(),
            speed: rider.speed,
            turn: 0.0,
            flight: None,
        };
        if let Some(mut rolled) = rolled {
            rolled.0 = rider.travelled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::PlayerPlugin;
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{
        BASE_RATE, LinePhase, ONBOARD_MULTIPLIER, OPENING_LEAD, Terrain, TimeOfDay, TownActivation,
        TownConfig, Vec2, Vehicle, World, WorldConfig,
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
            .insert_resource(Horsemen::for_towns(&towns))
            .insert_resource(ParkedVehicles::for_towns(&towns))
            .insert_resource(WorldClock::default())
            .insert_resource(WorldTrains::default())
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
        // La gente del pueblo lejano queda congelada mientras no hay nadie
        // allí.
        assert_eq!(app.world().resource::<Crowds>().0[1], far_start);
        let horses = |app: &mut App| {
            let mut q = app.world_mut().query::<&TetheredHorse>();
            q.iter(app.world()).map(|h| h.0).collect::<Vec<_>>()
        };
        assert_eq!(horses(&mut app), vec![0]);

        // Cabalgar hasta el pueblo lejano: Castex se vacía y el pueblo lejano
        // se llena.
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
        // La gente de un pueblo pequeño es la que tuvo siempre.
        let small = Townsfolk::new(&towns[0], 16, crowd_seed(towns[0].seed));
        assert_eq!(crowds.0[0], small);
    }

    fn shipped() -> &'static crate::towns::LoadedWorld {
        crate::shipped_world::loaded()
    }

    /// El mundo incluido con su reloj en `clock` y el tren de la línea llevado
    /// hasta allí desde la medianoche.
    fn shipped_app(clock: Option<GameClock>) -> App {
        let loaded = shipped();
        let (start, mut trains, _) = start_of_day(&loaded.line, &loaded.towns);
        let clock = clock.unwrap_or(start);
        if clock != start {
            trains = LineTrain::all_of(&loaded.line, &loaded.towns, Timetable::default());
            for train in &mut trains {
                train.advance(0.0, clock.elapsed(), BASE_RATE);
            }
        }
        let mut app = app_with(loaded.towns.clone(), 1);
        app.insert_resource(WorldClock(clock))
            .insert_resource(WorldTrains(trains));
        app
    }

    fn train(app: &App) -> LineTrain {
        app.world()
            .resource::<WorldTrains>()
            .line()
            .expect("a train")
            .clone()
    }

    #[test]
    fn the_game_starts_aboard_the_morning_train_from_pico_pulling_into_the_first_town() {
        let loaded = shipped();
        assert_eq!(loaded.towns[0].name, "Monte Nievas");
        let (clock, trains, ride) = start_of_day(&loaded.line, &loaded.towns);
        assert_eq!(trains.len(), 2, "the line train and the shuttle");
        let train = trains
            .into_iter()
            .next()
            .expect("the shipped line has a train");
        let s = train.state();
        assert_eq!(s.phase, LinePhase::Running);
        // Entrando a Monte Nievas, a poco menos de un minuto.
        let nievas = train.station_of_town(0).unwrap();
        let (next, left) = train.next_stop().unwrap();
        assert_eq!(next, nievas);
        assert!((left - OPENING_LEAD).abs() < 1e-9);
        // Sentado en el primer coche, para bajarse en Monte Nievas.
        let ride = ride.expect("the opening ride");
        assert_eq!(ride.alight_at, Some(nievas));
        assert_eq!(ride.car, 1);
        // El viaje ascendente, desde Pico después de su salida de las 07:00,
        // hacia Castex.
        assert_eq!(s.run, Run::Up);
        let pico = train.stations().len() - 1;
        assert_eq!(
            loaded.towns[train.stations()[pico].town].name,
            "General Pico"
        );
        // La locomotora mira hacia Castex: hacia arriba de la línea, en contra
        // de su dirección.
        let loco = train.placed_cars()[0];
        assert!(loco.direction.dot(train.path().direction_at(s.front)) < -0.9);
        assert_eq!(clock.day(), 0);
        // A media mañana, a plena luz del día, al ritmo de a bordo: diez
        // minutos de juego (50 segundos reales a 12×) antes de la llegada del
        // horario (10:53:59 desde las manzanas de 40 m de Metileo, 2026-10-25;
        // 10:53:40 desde la parada de Metileo, 2026-10-23; 10:14:45 antes).
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
        // A mitad de la parada matutina en Monte Nievas, a pie.
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
        // 20 frames de 50 ms a 30×: 30 segundos de juego.
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
            app.world().resource::<WorldTrains>().obstacles()
        );
    }

    #[test]
    fn a_frame_hitch_advances_the_clock_and_train_by_one_capped_step() {
        let mut app = shipped_app(Some(GameClock::at(TimeOfDay::hm(7, 10))));
        app.update();
        let start = app.world().resource::<WorldClock>().0;
        let before = train(&app).state();
        assert_eq!(before.phase, LinePhase::Running);
        // Un atasco de dos segundos (arrastre de ventana, tirón de carga) en un
        // solo frame.
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
        // Cuatro segundos reales: dos minutos de juego, pasadas las 07:00.
        for _ in 0..80 {
            app.update();
        }
        let s = train(&app).state();
        assert_eq!(s.phase, LinePhase::Running);
        assert!(s.speed > 0.0);
        assert_eq!(
            app.world().resource::<GameWorld>().0.moving,
            app.world().resource::<WorldTrains>().obstacles()
        );
    }

    fn shuttle(app: &App) -> LineTrain {
        app.world().resource::<WorldTrains>().0[1].clone()
    }

    #[test]
    fn the_clock_advances_every_train_and_all_their_cars_are_obstacles() {
        // Un minuto antes de la primera salida de la lanzadera desde Caleufu:
        // el tren de la línea está en otro lado, estacionado o en marcha.
        let probe = shuttle(&shipped_app(None));
        let leaves = probe.timetable().departure(Run::Up).seconds();
        let mut app = shipped_app(Some(GameClock::at(TimeOfDay::from_seconds(leaves - 60.0))));
        app.update();
        assert_eq!(shuttle(&app).state().phase, LinePhase::Parked);
        // Cuatro segundos reales: dos minutos de juego, pasada la salida.
        for _ in 0..80 {
            app.update();
        }
        let s = shuttle(&app).state();
        assert_eq!(s.phase, LinePhase::Running, "the shuttle pulled out");
        assert!(s.speed > 0.0);
        let trains = app.world().resource::<WorldTrains>();
        let moving = &app.world().resource::<GameWorld>().0.moving;
        let (line, shuttle) = (train(&app).obstacles(), shuttle(&app).obstacles());
        assert_eq!(shuttle.len(), 3, "loco, coach and boxcar");
        assert_eq!(moving.len(), line.len() + shuttle.len());
        assert!(line.iter().chain(&shuttle).all(|o| moving.contains(o)));
        assert_eq!(*moving, trains.obstacles());
    }

    #[test]
    fn a_city_ties_horses_at_its_palenques_and_a_small_town_none() {
        let city = TownLayout::generate(&pampa_core::TownConfig {
            name: "Ciudad".into(),
            blocks: (8, 8),
            ..pampa_core::TownConfig::castex()
        })
        .unwrap();
        let tied: Vec<_> = city
            .palenques
            .iter()
            .enumerate()
            .filter_map(|(k, p)| p.horse.map(|h| (k, h)))
            .collect();
        assert!(!tied.is_empty());
        let mut app = app_with(vec![city.clone()], 3);
        app.update();
        let mut query = app
            .world_mut()
            .query::<(&PalenqueHorse, &Transform, &VisualPose, &CharacterVisual)>();
        let mut found: Vec<_> = query
            .iter(app.world())
            .map(|(h, t, p, v)| (*h, *t, *p, *v))
            .collect();
        found.sort_by_key(|(h, ..)| h.index);
        assert_eq!(found.len(), tied.len());
        for ((k, horse), (marker, transform, pose, visual)) in tied.iter().zip(found) {
            assert_eq!(marker, PalenqueHorse { town: 0, index: *k });
            assert_eq!(transform.translation.x, horse.position.x);
            assert_eq!(transform.translation.z, -horse.position.y);
            assert_eq!(pose.heading, horse.heading);
            assert!(matches!(visual, CharacterVisual::Horse(_)));
        }
        // El caballo del almacén sigue siendo el único que el encargo puede
        // prestar.
        let mut store = app.world_mut().query::<&TetheredHorse>();
        assert_eq!(store.iter(app.world()).count(), 1);
        let mut mountable = app.world_mut().query::<&crate::errand::Mountable>();
        assert_eq!(mountable.iter(app.world()).count(), 1);
        // Se van con su pueblo, como el caballo del almacén.
        let mut activation = TownActivation::default();
        activation.update(&[city], Vec2::new(1.0e6, 1.0e6));
        app.insert_resource(ActiveTowns(activation));
        app.update();
        let mut left = app.world_mut().query::<&PalenqueHorse>();
        assert_eq!(left.iter(app.world()).count(), 0);
        // Los palenques de un pueblo pequeño quedan vacíos.
        let mut small = town_app();
        small.update();
        let mut none = small.world_mut().query::<&PalenqueHorse>();
        assert_eq!(none.iter(small.world()).count(), 0);
    }

    #[test]
    fn a_towns_parked_vehicles_stand_with_it_block_the_player_and_leave_with_it() {
        let city = TownLayout::generate(&pampa_core::TownConfig {
            name: pampa_core::TownConfig::TOWN_HALL_TOWN.into(),
            blocks: (8, 8),
            ..pampa_core::TownConfig::castex()
        })
        .unwrap();
        let mut app = app_with(vec![city.clone()], 3);
        app.update();
        let parked = app.world().resource::<ParkedVehicles>().0[0].clone();
        assert!(parked.iter().any(|p| p.vehicle == Vehicle::FordT));
        let world = app.world().resource::<GameWorld>().0.clone();
        let mut query = app
            .world_mut()
            .query::<(&ParkedVehicle, &Transform, &VehicleVisual, &VisualPose)>();
        let found: Vec<_> = query
            .iter(app.world())
            .map(|(p, t, v, pose)| (*p, *t, *v, *pose))
            .collect();
        assert_eq!(found.len(), parked.len());
        for (k, transform, visual, pose) in found {
            let p = parked[k.index];
            assert_eq!(visual.vehicle, p.vehicle);
            assert_eq!(visual.driver.is_some(), p.vehicle != Vehicle::FordT);
            assert_eq!(pose.heading, p.heading);
            assert_eq!(
                transform.translation,
                world_from_ground(p.position, world.ground_height(p.position))
            );
        }
        // El jugador, metido en el medio de uno, queda afuera.
        let at = parked[0].position;
        {
            let mut q = app.world_mut().query::<&mut Player>();
            let mut player = q.single_mut(app.world_mut()).unwrap();
            player.rider.position = at;
        }
        app.update();
        let mut q = app.world_mut().query::<&Player>();
        let now = q.single(app.world()).unwrap().rider.position;
        assert!(
            parked[0]
                .bodies()
                .iter()
                .all(|b| b.position.distance(now) >= b.collider.radius - 1e-3),
            "{now:?}"
        );
        let mut activation = TownActivation::default();
        activation.update(&[city], Vec2::new(1.0e6, 1.0e6));
        app.insert_resource(ActiveTowns(activation));
        app.update();
        let mut left = app.world_mut().query::<&ParkedVehicle>();
        assert_eq!(left.iter(app.world()).count(), 0);
    }

    #[test]
    fn horsemen_ride_with_their_town_and_leave_with_it() {
        let city = TownLayout::generate(&pampa_core::TownConfig {
            name: "Ciudad".into(),
            blocks: (8, 8),
            ..pampa_core::TownConfig::castex()
        })
        .unwrap();
        let mut app = app_with(vec![city.clone()], 3);
        for _ in 0..40 {
            app.update();
        }
        let riders = app.world().resource::<Horsemen>().0[0].clone();
        assert_eq!(riders.riders.len(), 6 + 2, "six riders and two sulkies");
        let world = app.world().resource::<GameWorld>().0.clone();
        let mut query = app
            .world_mut()
            .query::<(&Horseman, &Transform, &VisualPose, &CharacterVisual)>();
        let found: Vec<_> = query
            .iter(app.world())
            .map(|(h, t, p, v)| (*h, *t, *p, *v))
            .collect();
        assert_eq!(found.len(), 6);
        let mut moving = false;
        for (horseman, transform, pose, visual) in found {
            assert_eq!(horseman.town, 0);
            let rider = &riders.riders[horseman.index];
            assert_eq!(
                visual,
                CharacterVisual::Mounted {
                    rider: FigureKind::townsfolk(rider.look),
                    coat: Coat::seeded(rider.coat),
                }
            );
            let height = world.ground_height(rider.position);
            assert_eq!(
                transform.translation,
                world_from_ground(rider.position, height)
            );
            assert_eq!(pose.heading, rider.heading);
            assert_eq!(pose.speed, rider.speed);
            moving |= rider.speed > 0.0;
        }
        assert!(moving);
        // Los sulkies, dibujados como tales, rodando con su vecino.
        let mut sulkies = app
            .world_mut()
            .query::<(&Horseman, &Transform, &VehicleVisual, &Rolled)>();
        let found: Vec<_> = sulkies
            .iter(app.world())
            .map(|(h, t, v, r)| (*h, *t, *v, *r))
            .collect();
        assert_eq!(found.len(), 2);
        for (horseman, transform, visual, rolled) in found {
            let sulky = &riders.riders[horseman.index];
            assert_eq!(
                visual,
                VehicleVisual::seeded(Vehicle::Sulky, sulky.look, sulky.coat)
            );
            let height = world.ground_height(sulky.position);
            assert_eq!(
                transform.translation,
                world_from_ground(sulky.position, height)
            );
            assert_eq!(rolled.0, sulky.travelled);
        }
        let mut activation = TownActivation::default();
        activation.update(&[city], Vec2::new(1.0e6, 1.0e6));
        app.insert_resource(ActiveTowns(activation));
        app.update();
        let mut left = app.world_mut().query::<&Horseman>();
        assert_eq!(left.iter(app.world()).count(), 0);
    }

    #[test]
    fn patrons_sit_on_the_pulperia_benches_by_the_hour_and_leave_with_the_town() {
        use pampa_core::{GameClock, TimeOfDay};
        let mut app = town_app();
        let clock = |app: &mut App, h, m| {
            app.insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(h, m))));
        };
        let move_player = |app: &mut App, at: Vec2| {
            let mut q = app.world_mut().query::<&mut Player>();
            q.single_mut(app.world_mut()).unwrap().rider.position = at;
        };
        let patrons = |app: &mut App| {
            let mut query = app
                .world_mut()
                .query::<(&Patron, &Transform, &VisualPose, &CharacterVisual)>();
            query
                .iter(app.world())
                .map(|(p, t, v, c)| (*p, *t, *v, *c))
                .collect::<Vec<_>>()
        };
        clock(&mut app, 11, 0);
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let (store, inside) = town.pulperia().expect("Castex has a pulpería");
        let world = app.world().resource::<GameWorld>().0.clone();
        let middle = store.footprint.center();
        let floor = world.terrain.height(middle.x, middle.y) + pampa_core::pulperia::FLOOR_RISE;
        // De día, dos en la misma mesa, sentados en sus bancos.
        let day = patrons(&mut app);
        assert_eq!(day.len(), 2);
        assert_eq!(
            inside.seats[day[0].0.seat].table,
            inside.seats[day[1].0.seat].table
        );
        for (patron, transform, pose, visual) in &day {
            assert_eq!(patron.town, 0);
            let seat = inside.seats[patron.seat];
            assert_eq!(
                transform.translation,
                world_from_ground(seat.position, floor + pampa_core::pulperia::BENCH_HEIGHT)
            );
            assert_eq!(pose.heading, seat.facing);
            // Juega al truco quien se sienta a la mesa de los naipes; los
            // demás fuman, toman o conversan.
            use pampa_core::pulperia::Pastime;
            let CharacterVisual::Seated(FigureKind::Townsfolk(_), pastime) = *visual else {
                panic!("{visual:?}");
            };
            let at_cards = Some(seat.table) == inside.card_table;
            assert_eq!(pastime == Pastime::Cards, at_cards, "{visual:?}");
            assert_ne!(pastime, Pastime::Rest, "{visual:?}");
        }
        // Con el jugador adentro nadie aparece ni desaparece, aunque caiga la
        // tarde.
        move_player(&mut app, inside.door.center - inside.door.outward * 1.0);
        clock(&mut app, 19, 30);
        for _ in 0..3 {
            app.update();
        }
        let seats = |found: &[(Patron, Transform, VisualPose, CharacterVisual)]| {
            found.iter().map(|(p, ..)| p.seat).collect::<BTreeSet<_>>()
        };
        assert_eq!(seats(&patrons(&mut app)), seats(&day));
        // Afuera, después del trabajo, llega más gente.
        move_player(&mut app, inside.door.center + inside.door.outward * 6.0);
        app.update();
        let evening = patrons(&mut app);
        assert!((4..=8).contains(&evening.len()), "{}", evening.len());
        assert_eq!(seats(&evening).len(), evening.len(), "one per seat");
        // Cerrada, nadie.
        clock(&mut app, 23, 0);
        app.update();
        assert!(patrons(&mut app).is_empty());
        clock(&mut app, 20, 0);
        app.update();
        assert!(!patrons(&mut app).is_empty());
        let mut activation = TownActivation::default();
        activation.update(&[town], Vec2::new(1.0e6, 1.0e6));
        app.insert_resource(ActiveTowns(activation));
        app.update();
        assert!(patrons(&mut app).is_empty());
    }

    #[test]
    fn a_keeper_stands_behind_each_counter_and_leaves_with_the_town() {
        let mut app = town_app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let world = app.world().resource::<GameWorld>().0.clone();
        let keepers = |app: &mut App| {
            let mut query = app
                .world_mut()
                .query::<(&Keeper, &Transform, &VisualPose, &CharacterVisual)>();
            query
                .iter(app.world())
                .map(|(k, t, p, v)| (*k, *t, *p, *v))
                .collect::<Vec<_>>()
        };
        let found = keepers(&mut app);
        let rooms: Vec<_> = town.rooms().collect();
        assert_eq!(rooms.len(), 2, "the pulpería and the almacén");
        assert_eq!(found.len(), rooms.len());
        for (index, store, inside) in &rooms {
            let (keeper, transform, pose, visual) = found
                .iter()
                .find(|(k, ..)| k.room == *index)
                .expect("one keeper per room");
            assert_eq!(keeper.town, 0);
            let middle = store.footprint.center();
            let floor = world.terrain.height(middle.x, middle.y) + pampa_core::pulperia::FLOOR_RISE;
            assert_eq!(
                transform.translation,
                world_from_ground(inside.keeper.position, floor)
            );
            assert_eq!(pose.heading, inside.keeper.facing);
            assert!(
                matches!(visual, CharacterVisual::Figure(FigureKind::Townsfolk(_))),
                "{visual:?}"
            );
        }
        let mut activation = TownActivation::default();
        activation.update(&[town], Vec2::new(1.0e6, 1.0e6));
        app.insert_resource(ActiveTowns(activation));
        app.update();
        assert!(keepers(&mut app).is_empty());
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
        // Un caballo 3D ensillado, sin jinete; sin encargo aquí, su pelaje es
        // el propio del pueblo.
        let town = &app.world().resource::<WorldTowns>().0[0];
        let coat = Coat::seeded(town.seed ^ 0x4f52_5345);
        assert_eq!(*visual, CharacterVisual::Horse(coat));
        assert_eq!(pose.heading, horse.heading);
        assert_eq!(transform.translation.x, horse.position.x);
        assert_eq!(transform.translation.z, -horse.position.y);
        // El jinete no puede atravesarlo: es un obstáculo estático.
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
    }

    #[test]
    fn don_hidalgos_horse_is_the_alazan_and_other_towns_seed_theirs() {
        let towns = vec![TownLayout::castex_inspired(), TownLayout::castex_inspired()];
        let cast = cast_of(&towns);
        let hidalgo = cast
            .0
            .iter()
            .find(|(n, _)| *n == Npc::Hidalgo)
            .unwrap()
            .1
            .town;
        assert_eq!(
            tethered_coat(&towns[hidalgo], &cast, hidalgo),
            Coat::HIDALGO
        );
        let empty = Cast::default();
        // Incluso donde la semilla propia del pueblo daría otro pelaje: el
        // pueblo de Don Hidalgo ata a su alazán, no al caballo sembrado.
        let mut his = towns[hidalgo].clone();
        his.seed = (0..)
            .find(|&seed| Coat::seeded(seed ^ 0x4f52_5345) != Coat::HIDALGO)
            .unwrap();
        assert_ne!(tethered_coat(&his, &empty, hidalgo), Coat::HIDALGO);
        assert_eq!(tethered_coat(&his, &cast, hidalgo), Coat::HIDALGO);
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
        Cast::of(&pampa_core::Errand::new(towns, 0, 1).unwrap())
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

        // El puesto de quesos no está en este pueblo.
        assert!(stalls(&mut app).is_empty());

        // En el pueblo lejano: Hidalgo ya no está, el comisario y el Viejo
        // Amadeo (un paisano, detrás de su puesto) están parados allí.
        app.world_mut()
            .resource_mut::<ActiveTowns>()
            .0
            .update(&towns, Vec2::new(3000.0, -2000.0));
        app.update();
        let there = standing_npcs(&mut app);
        let npcs: Vec<(Npc, CharacterVisual)> = there.iter().map(|(n, v, _)| (n.npc, *v)).collect();
        assert_eq!(
            npcs,
            [
                (Npc::Farach, CharacterVisual::Figure(FigureKind::Farach)),
                (
                    Npc::Amadeo,
                    CharacterVisual::Figure(FigureKind::Townsfolk(0))
                ),
                (
                    Npc::Bairoletto,
                    CharacterVisual::Seated(FigureKind::Bairoletto, pampa_core::pulperia::Pastime::Rest)
                )
            ]
        );
        assert!(there.iter().all(|(n, ..)| n.town == 1));
        // Su puesto, apoyado en el suelo, con la mesa entre él y la vereda.
        let stall = cast.2[0];
        let world = app.world().resource::<GameWorld>().0.clone();
        let height = world.ground_height(stall.position);
        assert_eq!(
            stalls(&mut app),
            [(
                StallTown(1),
                Transform::from_translation(world_from_ground(stall.position, height))
                    .with_rotation(Quat::from_rotation_y(stall.facing.radians()))
            )]
        );
        // Y se va con su pueblo.
        app.world_mut()
            .resource_mut::<ActiveTowns>()
            .0
            .update(&towns, Vec2::ZERO);
        app.update();
        assert!(stalls(&mut app).is_empty());
    }

    fn stalls(app: &mut App) -> Vec<(StallTown, Transform)> {
        let mut q = app
            .world_mut()
            .query_filtered::<(&StallTown, &Transform), With<CheeseStall>>();
        q.iter(app.world()).map(|(s, t)| (*s, *t)).collect()
    }

    #[test]
    fn who_waits_seated_in_the_pulperia_keeps_his_seat_and_by_day_his_table() {
        use pampa_core::{GameClock, TimeOfDay};
        let town = TownLayout::castex_inspired();
        let (store, inside) = town.pulperia().unwrap();
        let seed = crowd_seed(town.seed) ^ PATRONS_SEED;
        for hour in [11, 19] {
            let time = TimeOfDay::hm(hour, 0);
            // Donde se sentaría alguien si él no estuviera.
            let taken = inside.patrons(seed, time, &[])[0].seat;
            let mut app = town_app();
            app.insert_resource(Cast(
                vec![],
                vec![AtTable {
                    npc: Npc::Bairoletto,
                    town: 0,
                    seat: taken,
                }],
                vec![],
            ))
            .insert_resource(WorldClock(GameClock::at(time)));
            app.update();
            let mut q = app.world_mut().query::<&Patron>();
            let patrons: Vec<Patron> = q.iter(app.world()).copied().collect();
            assert!(!patrons.is_empty(), "{hour}");
            for p in &patrons {
                assert_ne!(p.seat, taken, "{hour}");
                if hour < 18 {
                    assert_ne!(inside.seats[p.seat].table, inside.seats[taken].table);
                }
            }
            // Él, sentado en el banco como los parroquianos, mirando su mesa.
            let him = standing_npcs(&mut app);
            let seat = inside.seats[taken];
            let world = app.world().resource::<GameWorld>().0.clone();
            let middle = store.footprint.center();
            let bench = world.terrain.height(middle.x, middle.y)
                + pampa_core::pulperia::FLOOR_RISE
                + pampa_core::pulperia::BENCH_HEIGHT;
            assert_eq!(
                him,
                [(
                    StandingNpc {
                        town: 0,
                        npc: Npc::Bairoletto
                    },
                    CharacterVisual::Seated(FigureKind::Bairoletto, pampa_core::pulperia::Pastime::Rest),
                    Transform::from_translation(world_from_ground(seat.position, bench))
                )]
            );
            let mut poses = app
                .world_mut()
                .query_filtered::<&VisualPose, With<StandingNpc>>();
            assert_eq!(poses.single(app.world()).unwrap().heading, seat.facing);
        }
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
        // Crear al jinete justo encima del primer peatón, al galope.
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
            // Mantenida presionada (nunca se suelta aquí), salta cada vez que
            // puede.
            keys.press(crate::player::JUMP_KEY);
        }
        let mut jumped = false;
        for frame in 0..200 {
            if frame == 1 {
                // Ya a galope tendido, para poder saltar entre ellos.
                let mut q = app.world_mut().query::<&mut crate::player::Player>();
                let mut player = q.single_mut(app.world_mut()).unwrap();
                player.rider.speed = pampa_core::RiderParams::default().gallop_max;
            }
            app.update();
            let rider = {
                let mut q = app.world_mut().query::<&crate::player::Player>();
                q.single(app.world()).unwrap().rider
            };
            jumped |= rider.airborne();
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
                world_from_ground(rider.position, rider.elevation())
            );
        }
        assert!(jumped);
    }
}
