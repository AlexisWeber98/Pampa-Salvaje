//! La vida en el pueblo: el reloj del juego, el tren de la línea, los vecinos
//! que caminan por las calles, el caballo atado frente a cada almacén de ramos
//! generales, y las personas del encargo paradas en sus pueblos.
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
    BASE_RATE, GameClock, LineTrain, MAX_STEP, Npc, RailLine, Ride, Run, Standing, Timetable,
    TownLayout, Townsfolk, Travel, opening,
};
use pampa_render::{
    ActiveTowns, CharacterVisual, Coat, FigureKind, MapIcon, VisualPose, WorldClock, WorldTowns,
    WorldTrains, world_from_ground,
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

/// Las personas del encargo y dónde están paradas, creadas con sus pueblos;
/// luego las personas que solo están paradas allí (Bairoletto junto a Farach),
/// con la figura con la que se dibuja cada una.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Cast(pub Vec<(Npc, Standing)>, pub Vec<(FigureKind, Standing)>);

/// Alguien parado en el pueblo `town` que no participa en el encargo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bystander {
    pub town: usize,
}

/// Pueblos cuyo caballo atado se llevaron en esta sesión: no se vuelve a atar
/// allí.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct LedAway(pub BTreeSet<usize>);

/// La figura de una persona del encargo.
pub fn npc_figure(npc: Npc) -> FigureKind {
    match npc {
        Npc::Hidalgo => FigureKind::Hidalgo,
        Npc::Farach => FigureKind::Farach,
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
);

/// Todos los que viven en un pueblo: peatones, el caballo atado y las personas
/// del encargo.
type Residents<'a> = (
    Entity,
    Option<&'a Walker>,
    Option<&'a TetheredHorse>,
    Option<&'a PalenqueHorse>,
    Option<&'a StandingNpc>,
    Option<&'a Bystander>,
);

/// Crea a la gente y el caballo atado de los pueblos que acaban de activarse, y
/// elimina los de los pueblos que quedaron atrás.
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
    for (entity, walker, horse, palenque, npc, bystander) in &residents {
        let town = walker
            .map(|w| w.town)
            .or(horse.map(|h| h.0))
            .or(palenque.map(|p| p.town))
            .or(npc.map(|n| n.town))
            .or(bystander.map(|b| b.town));
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
            spawn_palenque_horses(&mut commands, layout, town, &world);
        }
        for &(npc, standing) in cast.0.iter().filter(|(_, s)| s.town == town) {
            spawn_standing_npc(&mut commands, npc, standing, &world);
        }
        for &(kind, standing) in cast.1.iter().filter(|(_, s)| s.town == town) {
            spawn_bystander(&mut commands, kind, standing, &world);
        }
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

/// Alguien parado, quieto junto a las personas del encargo.
fn spawn_bystander(
    commands: &mut Commands,
    kind: FigureKind,
    standing: Standing,
    world: &GameWorld,
) {
    let at = standing.position;
    let height = world.0.ground_height(at);
    commands.spawn((
        Name::new(format!("{kind:?}")),
        Bystander {
            town: standing.town,
        },
        CharacterVisual::Figure(kind),
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

/// El pelaje del caballo atado en el almacén de `town`: el bayo de Don Hidalgo
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
fn run_clock_and_train(
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

/// Hace avanzar a la gente activa, que camina esquivando al jinete donde estaba
/// en el frame anterior.
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
        // Incluso donde la semilla propia del pueblo daría otro pelaje: el
        // pueblo de Don Hidalgo ata a su bayo, no al caballo sembrado.
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
            vec![(FigureKind::Bairoletto, errand.bairoletto())],
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

        // En el pueblo lejano: Hidalgo ya no está, el comisario está parado
        // allí.
        app.world_mut()
            .resource_mut::<ActiveTowns>()
            .0
            .update(&towns, Vec2::new(3000.0, -2000.0));
        app.update();
        let there = standing_npcs(&mut app);
        assert_eq!(there.len(), 1);
        assert_eq!(there[0].0.npc, Npc::Farach);
        assert_eq!(there[0].1, CharacterVisual::Figure(FigureKind::Farach));
        // Bairoletto está junto a Farach, y solo allí.
        let mut q = app
            .world_mut()
            .query::<(&Bystander, &CharacterVisual, &Transform)>();
        let beside: Vec<_> = q.iter(app.world()).map(|(b, v, t)| (*b, *v, *t)).collect();
        assert_eq!(beside.len(), 1);
        assert_eq!(beside[0].0, Bystander { town: 1 });
        assert_eq!(beside[0].1, CharacterVisual::Figure(FigureKind::Bairoletto));
        let gap = beside[0].2.translation.distance(there[0].2.translation);
        assert!(
            (gap - pampa_core::Errand::BAIROLETTO_GAP).abs() < 0.05,
            "{gap}"
        );
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
