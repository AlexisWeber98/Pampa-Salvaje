//! La milicada del acto 6. De noche, mientras Bairoletto espera en la puerta
//! de La Colonia, los caballos de cuatro milicos están atados enfrente
//! ([`tie_horses`]): la milicada está adentro con el comisario. Cuando el
//! jugador arranca con Bairoletto en ancas, los milicos salen corriendo de la
//! pulpería ([`sally`]), montan ([`mount`]) y galopan detrás del jugador
//! ([`ride_after`]) con su misma física, un poco más lentos
//! ([`pampa_core::pursuit`]). Al que se acerca a tiro, Bairoletto le dispara
//! ([`Gun`]): el caballo se espanta y el milico frena y pierde terreno.
//! Abandonan cuando el jugador les saca más de [`pampa_core::OUTRUN`] metros,
//! o [`pampa_core::LOST_IN_COVER`] entre árboles y malezas
//! ([`NearbyPlants`]), o cuando llevan [`pampa_core::LOST_SIGHT_SECONDS`] sin
//! verlo, y vuelven a La Colonia hasta perderse de vista.
//!
//! Los tiros se ven y se oyen: quien tira ([`Aim`]: Bairoletto en ancas, o un
//! milico cada tanto, [`pampa_core::ReturnFire`], sin hacerle nada a nadie)
//! estira el brazo con el revólver y hay un fogonazo
//! ([`pampa_render::Shooting`], [`show_shots`]); cada tiro suena, más bajo
//! cuanto más lejos ([`play_sounds`]), y los milicos gritan al salir. Al tirar,
//! Bairoletto grita las líneas de la persecución ([`yell`]).

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use std::collections::HashMap;

use pampa_core::{
    Body, ChunkCoord, Errand, ErrandStage, Heading, LOST_SIGHT_SECONDS, MAX_STEP, ReturnFire,
    Revolver, Rider, Vec2, chaser_params, in_cover, in_sight, outrun, pursue, rein_in,
};
use pampa_render::{
    CharacterVisual, Coat, FigureKind, MapIcon, Pillion, Shooting, TitleCard, VisualPose,
    WorldClock, WorldFlora, WorldTowns, world_from_ground,
};

use crate::errand::ErrandRun;
use crate::escape::{BESIDE_HORSE, DASH_PACE, Scripted, Then};
use crate::life::pulperia_floor;
use crate::player::{GameWorld, Player, yaw_rate};

/// Cuántos milicos persiguen al jugador.
pub const MILICOS: usize = 4;

/// A qué distancia del jugador se pierde de vista un milico que vuelve, en
/// metros.
pub const OUT_OF_SIGHT: f32 = 250.0;

/// A cuántos metros de donde salió (frente a La Colonia) termina la vuelta de
/// un milico que abandonó.
pub const HOME: f32 = 5.0;

/// Desde qué velocidad, en m/s, el jugador arrancó con Bairoletto en ancas y
/// la milicada sale.
pub const SET_OFF: f32 = 1.0;

/// Cuánto más adentro de la pulpería que el anterior arranca cada milico al
/// salir corriendo, en metros: salen uno detrás del otro.
pub const FILE_GAP: f32 = 1.2;

/// Un milico montado de la milicada: su jinete, adónde vuelve si abandona,
/// cuántos segundos más frena con el caballo espantado por un tiro, si ya
/// abandonó la persecución, cuándo tira él ([`ReturnFire`]) y cuántos
/// segundos seguidos lleva sin ver al jugador ([`in_sight`]).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Milico {
    pub rider: Rider,
    pub home: Vec2,
    pub shied: f32,
    pub back: bool,
    pub fire: ReturnFire,
    pub unseen: f32,
}

/// Los árboles (centro y radio de la copa) y los arbustos de los chunks
/// alrededor del jugador mientras lo persiguen: para saber si está entre
/// árboles y malezas ([`in_cover`]) y si las copas lo tapan ([`in_sight`]).
/// Se cargan de a [`PLANT_BUDGET`] chunks por frame y se descartan lejos.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct NearbyPlants {
    pub chunks: HashMap<ChunkCoord, ChunkPlants>,
}

/// Las plantas de un chunk: los árboles (centro y radio de la copa) y los
/// arbustos.
pub type ChunkPlants = (Vec<(Vec2, f32)>, Vec<Vec2>);

impl NearbyPlants {
    /// Los árboles cargados, con el radio de su copa.
    pub fn crowns(&self) -> Vec<(Vec2, f32)> {
        self.chunks
            .values()
            .flat_map(|(t, _)| t.iter().copied())
            .collect()
    }

    /// Cada planta cargada (árboles y arbustos).
    pub fn plants(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.chunks
            .values()
            .flat_map(|(t, s)| t.iter().map(|(at, _)| *at).chain(s.iter().copied()))
    }
}

/// Hasta qué distancia de chunks (de Chebyshev) desde el del jugador se
/// cargan las plantas: cubre la línea de vista de un milico hasta
/// [`pampa_core::OUTRUN`].
pub const PLANT_REACH: i32 = 2;

/// Cuántos chunks de plantas se cargan por frame.
pub const PLANT_BUDGET: usize = 2;

/// Carga las plantas de los chunks alrededor del jugador mientras hay
/// milicos, y descarta las de más lejos; sin milicos, las suelta todas.
pub fn gather_plants(
    flora: Option<Res<WorldFlora>>,
    mut plants: ResMut<NearbyPlants>,
    players: Query<&Player>,
    milicos: Query<(), With<Milico>>,
) {
    let (Some(flora), Ok(player)) = (flora, players.single()) else {
        return;
    };
    if milicos.is_empty() {
        if !plants.chunks.is_empty() {
            plants.chunks.clear();
        }
        return;
    }
    let center = flora.0.config().chunk_of(player.rider.position);
    let near = |c: &ChunkCoord| (c.x - center.x).abs().max((c.y - center.y).abs()) <= PLANT_REACH;
    plants.chunks.retain(|c, _| near(c));
    let mut missing: Vec<ChunkCoord> = (-PLANT_REACH..=PLANT_REACH)
        .flat_map(|dx| (-PLANT_REACH..=PLANT_REACH).map(move |dy| (dx, dy)))
        .map(|(dx, dy)| ChunkCoord::new(center.x + dx, center.y + dy))
        .filter(|c| !plants.chunks.contains_key(c))
        .collect();
    missing.sort_by_key(|c| (c.x - center.x).abs().max((c.y - center.y).abs()));
    for coord in missing.into_iter().take(PLANT_BUDGET) {
        let chunk = flora.0.chunk(coord);
        let trees = chunk
            .trees
            .iter()
            .map(|t| (t.at, t.crown_radius()))
            .collect();
        let shrubs = chunk.shrubs.iter().map(|s| s.at).collect();
        plants.chunks.insert(coord, (trees, shrubs));
    }
}

/// Cuántos segundos queda apuntando, con el brazo estirado, quien tiró.
pub const AIM_HOLD: f32 = 1.2;

/// Cuánto dura el fogonazo de un tiro, en segundos.
pub const FLASH: f32 = 0.08;

/// A qué altura sobre el suelo apunta quien tira a alguien a caballo, en
/// metros: al cuerpo del jinete.
pub const AIM_HEIGHT: f32 = 1.9;

/// Quien acaba de tirar (el jugador por Bairoletto en ancas, o un milico):
/// a quién (`target`), cuántos segundos más apunta y cuánto queda del
/// fogonazo. Se vuelve el [`Shooting`] que dibuja el render.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Aim {
    pub target: Entity,
    pub left: f32,
    pub flash: f32,
}

impl Aim {
    /// Un tiro recién hecho a `target`.
    pub fn fired_at(target: Entity) -> Self {
        Self {
            target,
            left: AIM_HOLD,
            flash: FLASH,
        }
    }
}

/// Un tiro: desde dónde, y si fue Bairoletto tirando desde las ancas en la
/// huida (si no, un milico, o el disparo a Farach adentro de La Colonia, que
/// solo suena). Para el sonido y para los gritos de Bairoletto.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct Gunshot {
    pub from: Vec2,
    pub bairoletto: bool,
}

/// Cuántos gritos de la persecución ya dijo Bairoletto en esta huida.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Yelled(pub usize);

/// El caballo atado del milico `look`, frente a La Colonia, en `at`
/// mirando hacia `heading`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct TiedHorse {
    pub look: u8,
    pub at: Vec2,
    pub heading: Heading,
}

/// Lo que hace un milico al llegar corriendo a su caballo atado (`horse`):
/// lo desata y monta ([`mount`]); si abandona, vuelve a `home`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Saddle {
    pub horse: Entity,
    pub look: u8,
    pub at: Vec2,
    pub heading: Heading,
    pub home: Vec2,
}

/// Si la milicada ya salió de la pulpería en esta huida.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Sallied(pub bool);

/// El revólver de Bairoletto durante la huida; se carga cuando sale la
/// milicada.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct Gun(pub Revolver);

/// El pelaje del caballo del milico `look`: el mismo atado y montado.
fn coat(look: u8) -> Coat {
    Coat::seeded(0x4d49_4c00 + look as u32)
}

/// Ata los caballos de la milicada frente a La Colonia
/// ([`Errand::milicada_horses`]) mientras Bairoletto espera de noche, y los
/// deja durante la huida hasta que la milicada sale; de día, antes del acto,
/// o terminado, no están.
pub fn tie_horses(
    mut commands: Commands,
    (errand, clock): (Option<Res<ErrandRun>>, Option<Res<WorldClock>>),
    world: Res<GameWorld>,
    mut sallied: ResMut<Sallied>,
    tied: Query<Entity, With<TiedHorse>>,
) {
    let Some(errand) = errand else {
        return;
    };
    let stage = errand.0.stage();
    if stage != ErrandStage::Hideout {
        sallied.set_if_neq(Sallied(false));
    }
    let time = clock.map_or(0.0, |c| c.0.hour());
    let keep = stage == ErrandStage::Hideout || errand.0.waits_at_door(time);
    if !keep {
        for horse in &tied {
            commands.entity(horse).despawn();
        }
        return;
    }
    if sallied.0 || !tied.is_empty() {
        return;
    }
    for (look, (at, heading)) in errand
        .0
        .milicada_horses(&world.0, MILICOS)
        .into_iter()
        .enumerate()
    {
        let look = look as u8;
        commands.spawn((
            Name::new(format!("Milico horse {look}")),
            TiedHorse { look, at, heading },
            CharacterVisual::Horse(coat(look)),
            VisualPose {
                heading,
                ..default()
            },
            Transform::from_translation(world_from_ground(at, world.0.ground_height(at))),
        ));
    }
}

/// Cuando el jugador arranca ([`SET_OFF`]) con Bairoletto en ancas, los
/// milicos salen corriendo de La Colonia, uno detrás del otro, cada uno hasta
/// su caballo atado, y Bairoletto tiene sus [`Revolver::LOADED`] balas.
pub fn sally(
    mut commands: Commands,
    (errand, towns): (Option<Res<ErrandRun>>, Option<Res<WorldTowns>>),
    world: Res<GameWorld>,
    (mut sallied, mut gun, mut yelled): (ResMut<Sallied>, ResMut<Gun>, ResMut<Yelled>),
    players: Query<&Player, With<Pillion>>,
    tied: Query<(Entity, &TiedHorse)>,
) {
    let (Some(errand), Ok(player)) = (errand, players.single()) else {
        return;
    };
    if sallied.0 || errand.0.stage() != ErrandStage::Hideout || player.rider.speed < SET_OFF {
        return;
    }
    let (Some(door), Some(porch)) = (errand.0.pulperia_door(), errand.0.porch()) else {
        return;
    };
    sallied.0 = true;
    *gun = Gun::default();
    yelled.0 = 0;
    let room = towns
        .as_ref()
        .and_then(|t| t.0.get(porch.town)?.pulperia())
        .map(|(store, inside)| (inside.floor, pulperia_floor(store, &world)));
    let home = door.center + door.outward * Errand::TIE_OUT;
    let mut horses: Vec<_> = tied.iter().collect();
    horses.sort_by_key(|(_, t)| t.look);
    for (k, (horse, tie)) in horses.into_iter().enumerate() {
        let start = door.center - door.outward * (2.0 + FILE_GAP * k as f32);
        let height = room.map_or_else(|| world.0.ground_height(start), |(_, f)| f);
        let outside = door.center + door.outward * 1.0;
        let left = tie.heading.rotated(std::f32::consts::FRAC_PI_2).forward();
        let beside = tie.at + left * BESIDE_HORSE;
        commands.spawn((
            Name::new(format!("Milico {}", tie.look)),
            CharacterVisual::Figure(FigureKind::Milico(tie.look)),
            VisualPose {
                heading: porch.heading,
                ..default()
            },
            Transform::from_translation(world_from_ground(start, height)),
            Scripted {
                way: vec![start, outside, beside],
                next: 1,
                pace: DASH_PACE,
                room,
                fade: false,
                then: Then::Saddle(Saddle {
                    horse,
                    look: tie.look,
                    at: tie.at,
                    heading: tie.heading,
                    home,
                }),
            },
        ));
    }
}

/// El milico que llegó a su caballo lo desata y monta: el caballo atado se va
/// y queda el [`Milico`] montado en su lugar, listo para galopar.
pub fn mount(commands: &mut Commands, saddle: Saddle, world: &pampa_core::World) {
    commands.entity(saddle.horse).try_despawn();
    let rider = Rider::new(saddle.at, saddle.heading, world);
    commands.spawn((
        Milico {
            rider,
            home: saddle.home,
            shied: 0.0,
            back: false,
            fire: ReturnFire::new(saddle.look as u32),
            unseen: 0.0,
        },
        Name::new(format!("Milico {} mounted", saddle.look)),
        MapIcon::Chaser,
        CharacterVisual::Mounted {
            rider: FigureKind::Milico(saddle.look),
            coat: coat(saddle.look),
        },
        VisualPose {
            heading: saddle.heading,
            animation: rider.animation(&chaser_params()),
            ..default()
        },
        Transform::from_translation(world_from_ground(saddle.at, rider.elevation())),
    ));
}

/// Cada milico galopa detrás del jugador sin chocar con los demás, y
/// Bairoletto le tira al más cercano que tenga a tiro. Mientras Bairoletto va
/// en ancas lo persiguen; cuando se baja, o cuando el jugador los pierde
/// ([`outrun`]), vuelven a La Colonia y desaparecen al llegar o al quedar a
/// [`OUT_OF_SIGHT`] metros del jugador.
pub fn ride_after(
    mut commands: Commands,
    time: Res<Time>,
    (world, plants): (Res<GameWorld>, Res<NearbyPlants>),
    (mut gun, mut shots): (ResMut<Gun>, MessageWriter<Gunshot>),
    mut card: ResMut<TitleCard>,
    players: Query<(Entity, &Player, Has<Pillion>)>,
    mut milicos: Query<(Entity, &mut Milico, &mut Transform, &mut VisualPose)>,
) {
    let Ok((me_player, player, carrying)) = players.single() else {
        return;
    };
    let quarry = player.rider;
    let dt = time.delta_secs().min(MAX_STEP);
    let params = chaser_params();
    let covered = in_cover(plants.plants(), quarry.position);
    let crowns = plants.crowns();
    // A quién le tira: a los que persiguen y no están espantados.
    let at_large: Vec<(Entity, f32)> = milicos
        .iter()
        .filter(|(_, m, ..)| !m.back && m.shied <= 0.0)
        .map(|(e, m, ..)| (e, m.rider.position.distance(quarry.position)))
        .collect();
    let distances: Vec<f32> = at_large.iter().map(|(_, d)| *d).collect();
    let hit = carrying
        .then(|| gun.0.fire(&distances, dt))
        .flatten()
        .map(|i| at_large[i].0);
    // Sin balas y con un milico encima, lo atrapan: el cartel, y al negro el
    // acto vuelve a la puerta ([`crate::escape::back_to_the_door`]).
    if carrying && gun.0.caught(&distances) {
        card.start(crate::escape::CAUGHT);
    }
    if let Some(milico) = hit {
        commands.entity(me_player).insert(Aim::fired_at(milico));
        shots.write(Gunshot {
            from: quarry.position,
            bairoletto: true,
        });
    }
    let bodies: Vec<(Entity, Body)> = milicos
        .iter()
        .map(|(e, m, ..)| (e, m.rider.body()))
        .collect();
    for (me, mut milico, mut transform, mut pose) in &mut milicos {
        let lead = milico.rider.position.distance(quarry.position);
        // Lo pierde si le saca ventaja, o si lleva un rato sin verlo.
        if !milico.back {
            let seen = in_sight(&world.0, &crowns, milico.rider.position, quarry.position);
            milico.unseen = if seen { 0.0 } else { milico.unseen + dt };
        }
        let chasing = !milico.back;
        milico.back |= !carrying || outrun(lead, covered) || milico.unseen >= LOST_SIGHT_SECONDS;
        // Al abandonar deja de marcarse en el mapa.
        if chasing && milico.back {
            commands.entity(me).remove::<MapIcon>();
        }
        let home = milico.home;
        if milico.back && (lead > OUT_OF_SIGHT || milico.rider.position.distance(home) < HOME) {
            commands.entity(me).despawn();
            continue;
        }
        if hit == Some(me) {
            milico.shied = Revolver::SHIED_SECONDS;
        }
        let shied = milico.shied > 0.0;
        // Tira de vez en cuando, si no está espantado ni volviendo.
        if !milico.back && !shied && carrying && milico.fire.fire(lead, dt) {
            commands.entity(me).insert(Aim::fired_at(me_player));
            shots.write(Gunshot {
                from: milico.rider.position,
                bairoletto: false,
            });
        }
        milico.shied = (milico.shied - dt).max(0.0);
        let target = if milico.back {
            Rider::new(home, Heading::default(), &world.0)
        } else {
            quarry
        };
        let rider = &mut milico.rider;
        let before = rider.heading;
        let go = pursue(rider, &target, &params, &world.0, dt);
        if shied {
            rider.step(rein_in(rider), &params, &world.0, dt);
        } else {
            if go.jump {
                rider.jump(&params);
            }
            rider.step(go.input, &params, &world.0, dt);
        }
        let others: Vec<Body> = bodies
            .iter()
            .filter(|(e, _)| *e != me)
            .map(|(_, b)| *b)
            .chain(std::iter::once(quarry.body()))
            .collect();
        rider.keep_clear_of(&world.0, &others);
        transform.translation = world_from_ground(rider.position, rider.elevation());
        *pose = VisualPose {
            heading: rider.heading,
            animation: rider.animation(&params),
            speed: rider.speed,
            turn: yaw_rate(before, rider.heading, dt),
            flight: rider.flight(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Un jugador quieto con Bairoletto en ancas y un milico galopando hacia
    /// él desde `behind` metros al oeste.
    fn app(behind: f32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                0.05,
            )))
            .init_resource::<GameWorld>()
            .init_resource::<Gun>()
            .init_resource::<TitleCard>()
            .init_resource::<NearbyPlants>()
            .add_message::<Gunshot>()
            .add_systems(Update, ride_after);
        let world = pampa_core::World::default();
        let player = Rider::new(Vec2::ZERO, Heading::default(), &world);
        app.world_mut().spawn((
            Player { rider: player },
            Pillion(FigureKind::Bairoletto),
            Transform::default(),
        ));
        let mut rider = Rider::new(Vec2::new(-behind, 0.0), Heading::default(), &world);
        rider.speed = chaser_params().gallop_max;
        app.world_mut().spawn((
            Milico {
                rider,
                home: Vec2::new(-2000.0, 0.0),
                shied: 0.0,
                back: false,
                fire: ReturnFire::new(0),
                unseen: 0.0,
            },
            pampa_render::MapIcon::Chaser,
            Transform::default(),
            VisualPose::default(),
        ));
        app
    }

    /// Si el milico sigue marcado en el mapa como perseguidor.
    fn marked(app: &mut App) -> bool {
        let mut q = app
            .world_mut()
            .query_filtered::<(), (With<Milico>, With<pampa_render::MapIcon>)>();
        q.iter(app.world()).count() == 1
    }

    fn milico(app: &mut App) -> Milico {
        let mut q = app.world_mut().query::<&Milico>();
        *q.single(app.world()).unwrap()
    }

    #[test]
    fn bairoletto_shoots_a_milico_within_range_who_reins_in_and_comes_back_for_more() {
        let mut app = app(60.0);
        let rounds = |app: &App| app.world().resource::<Gun>().0.rounds;
        let mut closest = f32::INFINITY;
        while rounds(&app) == Revolver::LOADED {
            app.update();
            closest = closest.min(milico(&mut app).rider.position.length());
            assert!(closest > 5.0, "never reached him before the shot");
        }
        assert!(
            closest <= Revolver::RANGE + 1.0,
            "shot within range: {closest} m"
        );
        // Espantado, frena, y no le gastan otra bala aunque siga a tiro.
        for _ in 0..(Revolver::SHIED_SECONDS / 0.05) as usize - 1 {
            app.update();
        }
        let m = milico(&mut app);
        assert!(
            m.rider.speed.abs() < 1.0,
            "reined in: {} m/s",
            m.rider.speed
        );
        assert!(m.rider.position.length() <= Revolver::RANGE);
        assert_eq!(rounds(&app), Revolver::LOADED - 1);
        // Después vuelve a la carga, y le tiran otra vez.
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(rounds(&app), Revolver::LOADED - 2);
    }

    #[test]
    fn a_shot_milico_falls_back_from_a_rider_riding_on() {
        let mut app = app(Revolver::RANGE - 1.0);
        let pace = 8.0;
        for _ in 0..(Revolver::SHIED_SECONDS / 0.05) as usize {
            let mut q = app.world_mut().query::<&mut Player>();
            let mut player = q.single_mut(app.world_mut()).unwrap();
            player.rider.position.x += pace * 0.05;
            player.rider.speed = pace;
            app.update();
        }
        assert_eq!(app.world().resource::<Gun>().0.rounds, Revolver::LOADED - 1);
        let mut q = app.world_mut().query::<&Player>();
        let ahead = q.single(app.world()).unwrap().rider.position;
        let gap = ahead.distance(milico(&mut app).rider.position);
        assert!(gap > 30.0, "fell back: {gap} m");
    }

    /// Lo que dibuja el render en `who` ahora: el blanco y el fogonazo.
    fn shooting(app: &App, who: Entity) -> Option<Shooting> {
        app.world().get::<Shooting>(who).copied()
    }

    fn player_entity(app: &mut App) -> Entity {
        let mut q = app.world_mut().query_filtered::<Entity, With<Player>>();
        q.single(app.world()).unwrap()
    }

    #[test]
    fn his_shot_raises_bairolettos_arm_at_the_milico_with_a_flash_then_lowers_it() {
        let mut app = app(Revolver::RANGE - 1.0);
        app.add_systems(Update, show_shots.after(ride_after));
        let me = player_entity(&mut app);
        while app.world().resource::<Gun>().0.rounds == Revolver::LOADED {
            app.update();
        }
        app.update();
        let shot = shooting(&app, me).expect("aims after the shot");
        let mut q = app.world_mut().query::<(&Milico, &Transform)>();
        let (_, target) = q.single(app.world()).unwrap();
        assert!((shot.at - (target.translation + Vec3::Y * AIM_HEIGHT)).length() < 1.0);
        assert!(shot.flash > 0.0, "with a flash");
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            shooting(&app, me).map(|s| s.flash),
            Some(0.0),
            "the flash is brief"
        );
        for _ in 0..(AIM_HOLD / 0.05) as usize {
            app.update();
        }
        assert_eq!(shooting(&app, me), None, "lowers the arm");
    }

    #[test]
    fn a_milico_fires_back_now_and_then() {
        let mut app = app(40.0);
        app.add_systems(Update, show_shots.after(ride_after));
        // Sin balas Bairoletto no tira: los tiros son del milico.
        app.world_mut().resource_mut::<Gun>().0.rounds = 0;
        let me = player_entity(&mut app);
        let mut q = app.world_mut().query_filtered::<Entity, With<Milico>>();
        let milico = q.single(app.world()).unwrap();
        let fired = (0..(ReturnFire::EVERY_MAX / 0.05) as usize).any(|_| {
            app.update();
            app.world()
                .get::<Aim>(milico)
                .is_some_and(|aim| aim.target == me)
        });
        assert!(fired, "fires at the player within range");
        assert_eq!(shooting(&app, me), None, "Bairoletto did not");
    }

    #[test]
    fn a_shot_sounds_quieter_the_farther_it_is() {
        assert_eq!(loudness(0.0), 1.0);
        assert!(loudness(50.0) > loudness(150.0));
        assert_eq!(loudness(HEARD_UP_TO * 2.0), QUIETEST);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Gunshot>()
            .init_resource::<Sallied>()
            .insert_resource(EscapeSounds {
                shot: Some(Handle::default()),
                tumult: None,
            })
            .add_systems(Update, play_sounds);
        let world = pampa_core::World::default();
        app.world_mut().spawn(Player {
            rider: Rider::new(Vec2::ZERO, Heading::default(), &world),
        });
        app.world_mut().write_message(Gunshot {
            from: Vec2::new(150.0, 0.0),
            bairoletto: false,
        });
        app.update();
        let mut q = app.world_mut().query::<(&AudioPlayer, &PlaybackSettings)>();
        let played: Vec<f32> = q
            .iter(app.world())
            .map(|(_, p)| p.volume.to_linear())
            .collect();
        assert_eq!(played, [loudness(150.0)]);
    }

    fn milico_back(app: &mut App) -> bool {
        milico(app).back
    }

    #[test]
    fn six_seconds_without_seeing_him_behind_a_wall_and_the_milico_gives_up() {
        let mut app = app(100.0);
        app.world_mut()
            .resource_mut::<GameWorld>()
            .0
            .obstacles
            .push(pampa_core::Aabb::new(
                Vec2::new(-52.0, -400.0),
                Vec2::new(-50.0, 400.0),
            ));
        let seconds = |s: f32| (s / 0.05) as usize;
        for _ in 0..seconds(LOST_SIGHT_SECONDS - 0.5) {
            app.update();
        }
        assert!(!milico_back(&mut app), "still after him");
        assert!(marked(&mut app), "a chaser on the map");
        for _ in 0..seconds(1.0) {
            app.update();
        }
        assert!(milico_back(&mut app), "lost him");
        assert!(!marked(&mut app), "riding back, off the map");
    }

    #[test]
    fn among_trees_and_scrub_two_hundred_metres_are_enough() {
        let lead = (pampa_core::LOST_IN_COVER + pampa_core::OUTRUN) / 2.0;
        let mut open = app(lead);
        open.update();
        assert!(!milico_back(&mut open), "in the open he keeps on");
        let mut app = app(lead);
        let shrubs = (0..pampa_core::COVER_PLANTS)
            .map(|k| Vec2::new(k as f32, 3.0))
            .collect();
        app.world_mut()
            .resource_mut::<NearbyPlants>()
            .chunks
            .insert(ChunkCoord::new(0, 0), (Vec::new(), shrubs));
        app.update();
        assert!(milico_back(&mut app), "lost him among the plants");
    }

    #[test]
    fn far_enough_ahead_the_milicada_gives_up_out_of_sight() {
        let mut app = app(pampa_core::OUTRUN + 10.0);
        app.update();
        let mut q = app.world_mut().query::<&Milico>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }
}

/// Lleva cada [`Aim`] al [`Shooting`] que dibuja el render: el blanco a la
/// altura del cuerpo del jinete, el fogonazo que se apaga y, pasado
/// [`AIM_HOLD`], baja el brazo.
pub fn show_shots(
    mut commands: Commands,
    time: Res<Time>,
    mut aims: Query<(Entity, &mut Aim)>,
    targets: Query<&Transform>,
) {
    let dt = time.delta_secs();
    for (shooter, mut aim) in &mut aims {
        aim.left -= dt;
        let target = targets.get(aim.target).ok();
        let (Some(target), true) = (target, aim.left > 0.0) else {
            commands.entity(shooter).remove::<(Aim, Shooting)>();
            continue;
        };
        commands.entity(shooter).insert(Shooting {
            at: target.translation + Vec3::Y * AIM_HEIGHT,
            flash: aim.flash,
        });
        aim.flash = (aim.flash - dt).max(0.0);
    }
}

/// Lo que grita Bairoletto al tirar: la próxima línea de los gritos de la
/// persecución, si no hay otra en pantalla.
pub fn yell(
    mut shots: MessageReader<Gunshot>,
    dialogue: Option<Res<crate::errand::ErrandDialogue>>,
    mut conversation: ResMut<crate::errand::Conversation>,
    mut yelled: ResMut<Yelled>,
) {
    let bairoletto = shots.read().any(|s| s.bairoletto);
    let Some(dialogue) = dialogue else {
        return;
    };
    let lines = &dialogue.0.chase_yells;
    if !bairoletto || conversation.0.is_some() || yelled.0 >= lines.len() {
        return;
    }
    conversation.0 = Some(crate::errand::Talk::remark(vec![lines[yelled.0].clone()]));
    yelled.0 += 1;
}

/// El tiro de revólver, en `assets/` (lo descarga el usuario: ver
/// `assets/sounds/CREDITS.md`). Sin el archivo, los tiros no suenan.
pub const SHOT_SOUND: &str = "sounds/revolver.mp3";

/// Los gritos de la milicada al salir de La Colonia, en `assets/`.
pub const TUMULT_SOUND: &str = "sounds/tumult.mp3";

/// Hasta qué distancia del jugador se oye un tiro, en metros: se apaga con la
/// distancia hasta [`QUIETEST`] de su volumen.
pub const HEARD_UP_TO: f32 = 300.0;
pub const QUIETEST: f32 = 0.15;

/// Los sonidos de la huida que hay en `assets/` (los que faltan no suenan).
#[derive(Resource, Debug, Clone, Default)]
pub struct EscapeSounds {
    pub shot: Option<Handle<AudioSource>>,
    pub tumult: Option<Handle<AudioSource>>,
}

/// Carga los sonidos de la huida que estén en `assets/`.
pub fn load_sounds(mut commands: Commands, server: Option<Res<AssetServer>>) {
    let Some(server) = server else {
        return;
    };
    let load = |path: &str| {
        pampa_content::asset_path(path)
            .exists()
            .then(|| server.load(path.to_string()))
    };
    commands.insert_resource(EscapeSounds {
        shot: load(SHOT_SOUND),
        tumult: load(TUMULT_SOUND),
    });
}

/// El volumen de un sonido a `distance` metros del jugador.
pub fn loudness(distance: f32) -> f32 {
    (1.0 - distance / HEARD_UP_TO).clamp(QUIETEST, 1.0)
}

/// Hace sonar cada tiro, más bajo cuanto más lejos del jugador, y los gritos
/// de la milicada cuando sale de La Colonia.
pub fn play_sounds(
    mut commands: Commands,
    sounds: Option<Res<EscapeSounds>>,
    mut shots: MessageReader<Gunshot>,
    sallied: Res<Sallied>,
    players: Query<&Player>,
) {
    let (Some(sounds), Ok(player)) = (sounds, players.single()) else {
        shots.clear();
        return;
    };
    let here = player.rider.position;
    let mut play = |sound: &Handle<AudioSource>, volume: f32| {
        commands.spawn((
            Name::new("Escape sound"),
            AudioPlayer::new(sound.clone()),
            PlaybackSettings {
                volume: Volume::Linear(volume),
                ..PlaybackSettings::DESPAWN
            },
        ));
    };
    if let Some(shot) = &sounds.shot {
        for fired in shots.read() {
            play(shot, loudness(fired.from.distance(here)));
        }
    } else {
        shots.clear();
    }
    if sallied.is_changed()
        && sallied.0
        && let Some(tumult) = &sounds.tumult
    {
        play(tumult, 1.0);
    }
}
