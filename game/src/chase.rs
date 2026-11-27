//! La milicada del acto 6. De noche, mientras Bairoletto espera en la puerta
//! de La Colonia, los caballos de cuatro milicos están atados enfrente
//! ([`tie_horses`]): la milicada está adentro con el comisario. Cuando el
//! jugador arranca con Bairoletto en ancas, los milicos salen corriendo de la
//! pulpería ([`sally`]), montan ([`mount`]) y galopan detrás del jugador
//! ([`ride_after`]) con su misma física, un poco más lentos
//! ([`pampa_core::pursuit`]). Al que se acerca a tiro, Bairoletto le dispara
//! ([`Gun`]): el caballo se espanta y el milico frena y pierde terreno.
//! Abandonan cuando el jugador les saca más de [`pampa_core::OUTRUN`] metros,
//! o [`pampa_core::LOST_IN_MONTE`] en el monte, y vuelven a La Colonia hasta
//! perderse de vista.

use bevy::prelude::*;
use pampa_core::{
    Body, Errand, ErrandStage, FloraSite, Heading, MAX_STEP, Revolver, Rider, Vec2, chaser_params,
    outrun, pursue, rein_in,
};
use pampa_render::{
    CharacterVisual, Coat, FigureKind, Pillion, VisualPose, WorldClock, WorldFlora, WorldTowns,
    world_from_ground,
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
/// cuántos segundos más frena con el caballo espantado por un tiro y si ya
/// abandonó la persecución.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Milico {
    pub rider: Rider,
    pub home: Vec2,
    pub shied: f32,
    pub back: bool,
}

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
    (mut sallied, mut gun): (ResMut<Sallied>, ResMut<Gun>),
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
        },
        Name::new(format!("Milico {} mounted", saddle.look)),
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
    (world, flora): (Res<GameWorld>, Option<Res<WorldFlora>>),
    mut gun: ResMut<Gun>,
    players: Query<(&Player, Has<Pillion>)>,
    mut milicos: Query<(Entity, &mut Milico, &mut Transform, &mut VisualPose)>,
) {
    let Ok((player, carrying)) = players.single() else {
        return;
    };
    let quarry = player.rider;
    let dt = time.delta_secs().min(MAX_STEP);
    let params = chaser_params();
    let in_monte = flora.is_some_and(|f| f.0.monte(quarry.position) >= FloraSite::MONTE_DENSE);
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
    let bodies: Vec<(Entity, Body)> = milicos
        .iter()
        .map(|(e, m, ..)| (e, m.rider.body()))
        .collect();
    for (me, mut milico, mut transform, mut pose) in &mut milicos {
        let lead = milico.rider.position.distance(quarry.position);
        milico.back |= !carrying || outrun(lead, in_monte);
        let home = milico.home;
        if milico.back && (lead > OUT_OF_SIGHT || milico.rider.position.distance(home) < HOME) {
            commands.entity(me).despawn();
            continue;
        }
        if hit == Some(me) {
            milico.shied = Revolver::SHIED_SECONDS;
        }
        let shied = milico.shied > 0.0;
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
            .add_systems(Update, ride_after);
        let world = pampa_core::World::default();
        let player = Rider::new(Vec2::ZERO, Heading::default(), &world);
        app.world_mut()
            .spawn((Player { rider: player }, Pillion(FigureKind::Bairoletto)));
        let mut rider = Rider::new(Vec2::new(-behind, 0.0), Heading::default(), &world);
        rider.speed = chaser_params().gallop_max;
        app.world_mut().spawn((
            Milico {
                rider,
                home: Vec2::new(-2000.0, 0.0),
                shied: 0.0,
                back: false,
            },
            Transform::default(),
            VisualPose::default(),
        ));
        app
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

    #[test]
    fn far_enough_ahead_the_milicada_gives_up_out_of_sight() {
        let mut app = app(pampa_core::OUTRUN + 10.0);
        app.update();
        let mut q = app.world_mut().query::<&Milico>();
        assert_eq!(q.iter(app.world()).count(), 0);
    }
}
