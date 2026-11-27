//! El acto 6 en juego ("El desconocido parte 2"). De noche
//! ([`Errand::waits_at_door`]) Bairoletto espera parado en la vereda frente a
//! la puerta de La Colonia ([`wait_at_door`]). Cuando el jugador llega hasta él
//! a caballo empieza la escena ([`set_off`]). Bairoletto habla y entra; se oye
//! el disparo y el tumulto; sale corriendo y se sube en ancas
//! ([`pampa_render::Pillion`]). Después el jugador huye por el campo, con la
//! milicada detrás ([`crate::chase`]). Donde sea que la pierda, fuera del
//! pueblo, Bairoletto se baja, le paga y se va caminando hasta perderse.
//! Esa noche Farach no está en la puerta de su comisaría: espera adentro de
//! La Colonia, parado en el mostrador, y al oírse el disparo cae y queda
//! tendido ahí; terminado el acto ya no está ([`place_farach`]).
//!
//! `pampa-core` es dueño de las reglas: cuándo espera, cuándo se cumple cada
//! paso y dónde está el monte. Las líneas y las indicaciones de escena
//! ([`Cue`]) son datos del guion. Este módulo cumple cada indicación
//! ([`stage_cues`]) y mueve a Bairoletto por su camino ([`run_scripted`]).

use bevy::prelude::*;
use pampa_core::{
    AnimationState, Beat, Cue, Errand, ErrandStage, Heading, MAX_STEP, Npc, Standing, Vec2,
};
use pampa_render::{
    CharacterVisual, Fallen, FigureKind, Pillion, VisualPose, WorldClock, WorldTowns,
    world_from_ground,
};

use pampa_render::character::Fading;

use crate::errand::{Approach, Conversation, ErrandDialogue, ErrandProgress, ErrandRun, Talk};
use crate::life::{FADE_SECONDS, LEAVING_PACE, Populated, StandingNpc, npc_figure, pulperia_floor};
use crate::player::{GameWorld, Player};
use crate::pulperia::HeldOpen;

/// Cuánto camina Bairoletto adentro de la pulpería, desde la puerta, antes de
/// perderse, en metros.
pub const INSIDE_DEPTH: f32 = 4.0;

/// A qué paso corre Bairoletto de la pulpería al caballo, en m/s (unos 16
/// km/h).
pub const DASH_PACE: f32 = 4.5;

/// A qué distancia del costado izquierdo del caballo queda parado Bairoletto
/// al bajarse, y desde dónde sube corriendo, en metros.
pub const BESIDE_HORSE: f32 = 1.3;

/// Hasta qué velocidad tiene que frenar el caballo, en m/s, antes de que
/// Bairoletto se baje.
pub const STOPPED: f32 = 0.3;

/// Cuánto camina Bairoletto al irse, alejándose de Lisandro hacia el monte,
/// en metros.
pub const WALK_OFF: f32 = 12.0;

/// Una persona del acto 6 que no pertenece a ningún pueblo: no se va cuando
/// el pueblo se descarga (Bairoletto en el monte).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Roaming;

/// Qué pasa cuando alguien termina su camino ([`Scripted`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Then {
    /// Se pierde de vista (adentro de la pulpería, o en el monte).
    Vanish,
    /// Se sube en ancas al caballo del jugador.
    Mount,
    /// Un milico desata su caballo y monta ([`crate::chase::mount`]).
    Saddle(crate::chase::Saddle),
}

/// Bairoletto recorriendo un camino de la escena, sin chocar con nada (su
/// camino va por lugares libres): camina o corre por `way` a `pace` (m/s);
/// `next` es el índice del próximo punto. Adentro de `room` pisa el piso de la
/// pulpería, a la altura `floor`. Si `fade`, se difumina en los últimos
/// [`FADE_SECONDS`] del camino.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Scripted {
    pub way: Vec<Vec2>,
    pub next: usize,
    pub pace: f32,
    pub room: Option<(pampa_core::Aabb, f32)>,
    pub fade: bool,
    pub then: Then,
}

/// De noche, mientras el acto 6 espera, Bairoletto está parado en la vereda
/// frente a la puerta de La Colonia, con el pueblo poblado
/// ([`pampa_core::Errand::place`]); de día no está. Durante una escena no se
/// toca.
pub(crate) fn wait_at_door(
    mut commands: Commands,
    errand: Option<ResMut<ErrandRun>>,
    clock: Option<Res<WorldClock>>,
    (conversation, populated): (Res<Conversation>, Option<Res<Populated>>),
    world: Res<GameWorld>,
    npcs: Query<(Entity, &StandingNpc, &CharacterVisual), Without<Roaming>>,
) {
    let (Some(mut errand), Some(clock)) = (errand, clock) else {
        return;
    };
    if conversation.0.is_some() {
        return;
    }
    let waiting = errand.0.waits_at_door(clock.0.hour());
    let porch = errand.0.porch();
    // Parado: sentado en La Colonia es el del acto 4, que todavía no se fue.
    let there = npcs
        .iter()
        .find(|(_, n, v)| n.npc == Npc::Bairoletto && matches!(v, CharacterVisual::Figure(_)));
    match (waiting, porch, there) {
        (true, Some(porch), None) if populated.is_some_and(|p| p.0.contains(&porch.town)) => {
            errand.0.place(porch);
            let height = world.0.ground_height(porch.position);
            commands.spawn((
                Name::new("Bairoletto"),
                StandingNpc {
                    town: porch.town,
                    npc: Npc::Bairoletto,
                },
                CharacterVisual::Figure(npc_figure(Npc::Bairoletto)),
                VisualPose {
                    heading: porch.heading,
                    ..default()
                },
                Transform::from_translation(world_from_ground(porch.position, height)),
            ));
        }
        (false, _, Some((entity, _, _))) if errand.0.stage() == ErrandStage::Rendezvous => {
            commands.entity(entity).despawn();
        }
        _ => {}
    }
}

/// Pone a Farach donde está según el encargo
/// ([`pampa_core::Errand::farach`]): en la puerta de su comisaría, o la noche
/// del acto 6 adentro de La Colonia, parado en el mostrador. Cuando se oye el
/// disparo de la escena de la puerta ([`pampa_core::Errand::shot_heard`]) cae
/// ahí de espaldas ([`Fallen`]) y queda tendido; si aparece ya tendido (su
/// pueblo se volvió a cargar), no se lo ve caer. Terminado el acto ya no está.
/// La huella de su cuerpo en la puerta (la agrega el plan del encargo) choca
/// solo mientras está ahí. El disparo suena una vez, desde donde estaba él
/// ([`crate::chase::Gunshot`], sin los gritos de la persecución).
pub(crate) fn place_farach(
    mut commands: Commands,
    (errand, clock): (Option<ResMut<ErrandRun>>, Option<Res<WorldClock>>),
    (conversation, mut shots): (Res<Conversation>, MessageWriter<crate::chase::Gunshot>),
    (mut world, towns): (ResMut<GameWorld>, Option<Res<WorldTowns>>),
    mut npcs: Query<(
        Entity,
        &StandingNpc,
        &mut Transform,
        &mut VisualPose,
        Has<Fallen>,
    )>,
) {
    let Some(mut errand) = errand else {
        return;
    };
    let time = clock.map_or(0.0, |c| c.0.hour());
    let standing = errand.0.farach(time).filter(|w| !w.fallen);
    if let Some(talk) = conversation.0.as_ref()
        && let Some(standing) = standing
        && talk.partner == Some(Npc::Bairoletto)
        && errand.0.stage() == ErrandStage::Hideout
        && Errand::shot_heard(&talk.beats, talk.next)
    {
        errand.0.shoot_farach();
        shots.write(crate::chase::Gunshot {
            from: standing.at.position,
            bairoletto: false,
        });
    }
    let place = errand.0.farach(time);
    let door = errand.0.standing(Npc::Farach);
    let gone = place.is_none_or(|p| p.at != door);
    // Se mira el mundo y no un recuerdo: uno vuelto a armar trae la huella
    // del plan otra vez.
    let footprint = door.footprint();
    let there = world.0.obstacles.near(footprint).contains(&&footprint);
    if gone && there {
        world.0.obstacles.remove(&footprint);
    } else if !gone && !there {
        world.0.obstacles.push(footprint);
    }
    for (entity, npc, mut transform, mut pose, fallen) in &mut npcs {
        if npc.npc != Npc::Farach {
            continue;
        }
        let Some(place) = place else {
            commands.entity(entity).despawn();
            continue;
        };
        let at = place.at.position;
        let here = Vec2::new(transform.translation.x, -transform.translation.z);
        let moved = here.distance(at) > 0.01;
        if moved {
            // Adentro pisa el piso de ladrillo de la pulpería.
            let floor = towns
                .as_ref()
                .and_then(|t| t.0.get(place.at.town)?.pulperia())
                .filter(|(_, inside)| inside.floor.contains_point(at))
                .map(|(store, _)| pulperia_floor(store, &world));
            let ground = floor.unwrap_or_else(|| world.0.ground_height(at));
            transform.translation = world_from_ground(at, ground);
            pose.heading = place.at.heading;
        }
        if place.fallen && !fallen {
            // Solo se lo ve caer donde estaba parado.
            let since = if moved {
                Fallen::LONG_AGO
            } else {
                Fallen::default()
            };
            commands.entity(entity).insert(since);
        }
    }
}

/// Los pasos del acto 6 que empiezan solos, al llegar: de noche, a caballo,
/// hasta Bairoletto en la puerta ([`pampa_core::Errand::rendezvous`]); y la
/// huida, en cuanto la milicada lo perdió en el campo
/// ([`pampa_core::Errand::shake_off`]). Ahí Bairoletto ya cuenta como parado
/// junto al caballo, donde se va a bajar, para que la cámara lo encuadre
/// mientras el caballo frena.
pub(crate) fn set_off(
    (errand, dialogue): (Option<ResMut<ErrandRun>>, Option<Res<ErrandDialogue>>),
    clock: Option<Res<WorldClock>>,
    (mut conversation, approach): (ResMut<Conversation>, Res<Approach>),
    players: Query<&Player>,
    mut progress: MessageWriter<ErrandProgress>,
    (mut held, mut asked): (ResMut<HeldOpen>, Local<bool>),
    (milicos, towns, sallied): (
        Query<&crate::chase::Milico>,
        Option<Res<WorldTowns>>,
        Res<crate::chase::Sallied>,
    ),
) {
    let (Some(mut errand), Ok(player)) = (errand, players.single()) else {
        return;
    };
    // La puerta queda abierta mientras Bairoletto entra y sale corriendo.
    let scene = conversation.0.as_ref().and_then(|t| t.partner) == Some(Npc::Bairoletto);
    let open = (scene && errand.0.stage() == ErrandStage::Hideout)
        .then(|| errand.0.standing(Npc::Bairoletto).town);
    held.set_if_neq(HeldOpen(open));
    if conversation.0.is_some() || approach.npc.is_some() {
        return;
    }
    let (at, travel) = (player.rider.position, player.rider.travel());
    let time = clock.map_or(0.0, |c| c.0.hour());
    // A pie le pide el caballo, una vez cada vez que se le acerca.
    let asking = errand.0.asks_for_horse(at, travel, time);
    let lines = dialogue.as_ref().map(|d| &d.0.bring_the_horse);
    if asking
        && !*asked
        && let Some(lines) = lines.filter(|l| !l.is_empty())
    {
        conversation.0 = Some(Talk::scene(Npc::Bairoletto, lines));
    }
    *asked = asking;
    let event = match errand.0.stage() {
        ErrandStage::Rendezvous => errand.0.rendezvous(at, travel, time),
        ErrandStage::Hideout => {
            // Perdida: salió y ninguno la sigue (abandonan y se van). Sin
            // salir no hay a quién perder.
            let lost = sallied.0 && milicos.iter().all(|m| m.back);
            let town = errand.0.standing(Npc::Farach).town;
            let Some(layout) = towns.as_ref().and_then(|t| t.0.get(town)) else {
                return;
            };
            let event = errand.0.shake_off(at, layout.extent(), lost);
            if event.is_some() {
                errand.0.place(beside_horse(player, town));
            }
            event
        }
        _ => None,
    };
    if let Some(event) = event {
        progress.write(ErrandProgress(event));
        conversation.0 = Some(Talk::scene(Npc::Bairoletto, errand.0.last_scene()));
    }
}

/// El cartel cuando la milicada atrapa al jugador (S4).
pub const CAUGHT: &str = "Te agarraron";

/// A cuántos metros de la vereda de La Colonia vuelve el jugador cuando lo
/// atrapan: el costado izquierdo del caballo queda frente a la puerta.
pub const BACK_AT_DOOR: f32 = 3.5;

/// Hace avanzar el cartel en curso ([`pampa_render::TitleCard`]), sea cual sea
/// su texto. Si es el de "Te agarraron" ([`CAUGHT`], lo inicia
/// [`crate::chase::ride_after`]), con la pantalla en negro vuelve el acto
/// al momento en que Bairoletto sale corriendo de la pulpería: la milicada
/// vuelve adentro con sus caballos atados, Bairoletto baja del anca, el
/// jugador queda montado y quieto frente a la puerta, y la escena sigue desde
/// [`Cue::RunToHorse`]. El paso de la historia no cambia (sigue la huida).
#[allow(clippy::too_many_arguments)]
pub(crate) fn back_to_the_door(
    mut commands: Commands,
    time: Res<Time>,
    mut card: ResMut<pampa_render::TitleCard>,
    (errand, world): (Option<Res<ErrandRun>>, Res<GameWorld>),
    (mut conversation, mut subtitle): (ResMut<Conversation>, ResMut<pampa_render::Subtitle>),
    (mut sallied, mut gun, mut yelled): (
        ResMut<crate::chase::Sallied>,
        ResMut<crate::chase::Gun>,
        ResMut<crate::chase::Yelled>,
    ),
    mut players: Query<(Entity, &mut Player, &mut Transform)>,
    milicos: Query<Entity, With<crate::chase::Milico>>,
) {
    // Todo cartel avanza, diga lo que diga; solo este rehace el acto.
    let caught = card.text() == Some(CAUGHT);
    if !card.tick(time.delta_secs()) || !caught {
        return;
    }
    // La milicada vuelve adentro aunque falte algo de lo que sigue: el negro
    // llega una sola vez.
    for milico in &milicos {
        commands.entity(milico).despawn();
    }
    *sallied = crate::chase::Sallied(false);
    *gun = crate::chase::Gun::default();
    *yelled = crate::chase::Yelled(0);
    subtitle.0 = None;
    let Ok((me, mut player, mut transform)) = players.single_mut() else {
        return;
    };
    commands
        .entity(me)
        .remove::<(Pillion, crate::chase::Aim, pampa_render::Shooting)>();
    let Some((errand, porch)) = errand.as_ref().and_then(|e| Some((e, e.0.porch()?))) else {
        return;
    };
    let out = porch.heading.forward();
    player.rider.heading = Heading::from_radians(out.x.atan2(-out.y));
    crate::console::teleport(
        &mut player.rider,
        &mut transform,
        porch.position + out * BACK_AT_DOOR,
        &world.0,
    );
    let scene = errand.0.last_scene();
    let run = scene
        .iter()
        .position(|b| *b == Beat::Cue(Cue::RunToHorse))
        .unwrap_or(scene.len());
    conversation.0 = Some(Talk::scene(Npc::Bairoletto, &scene[run..]));
}

/// El lugar junto al costado izquierdo del caballo del jugador, mirándolo,
/// en el pueblo `town` (solo lo rotula): donde Bairoletto se baja.
fn beside_horse(player: &Player, town: usize) -> Standing {
    let forward = player.rider.heading.forward();
    let left = Vec2::new(-forward.y, forward.x);
    let at = player.rider.position + left * BESIDE_HORSE;
    Standing {
        town,
        position: at,
        heading: Heading::from_radians((-left.y).atan2(-left.x)),
    }
}

/// Cumple la indicación de escena en curso ([`Talk::cue`]) y la quita cuando
/// terminó, para que la escena siga. Bairoletto entra a la pulpería
/// ([`Cue::Enter`]: la escena sigue mientras camina); sale corriendo hasta el
/// caballo ([`Cue::RunToHorse`]: la escena espera a que suba); se baja, una vez
/// frenado el caballo ([`Cue::GetOff`]); se va caminando ([`Cue::Leave`]).
pub(crate) fn stage_cues(
    mut commands: Commands,
    mut conversation: ResMut<Conversation>,
    (errand, towns): (Option<ResMut<ErrandRun>>, Option<Res<WorldTowns>>),
    world: Res<GameWorld>,
    players: Query<(Entity, &Player)>,
    npcs: Query<(Entity, &StandingNpc, &CharacterVisual, &Transform)>,
    running: Query<&Scripted>,
) {
    let (Some(mut errand), Ok((me, player))) = (errand, players.single()) else {
        return;
    };
    let Some(talk) = conversation.0.as_mut() else {
        return;
    };
    let Some(cue) = talk.cue.clone() else {
        return;
    };
    // Parado (el del acto 4, sentado, no cuenta).
    let tano = npcs
        .iter()
        .find(|(_, n, v, _)| n.npc == Npc::Bairoletto && matches!(v, CharacterVisual::Figure(_)))
        .map(|(e, _, _, t)| (e, t));
    let town = errand.0.standing(Npc::Farach).town;
    let pulperia = towns
        .as_ref()
        .and_then(|t| t.0.get(town)?.pulperia())
        .map(|(store, inside)| (inside, pulperia_floor(store, &world)));
    let door = errand.0.pulperia_door();
    match cue {
        Cue::Enter => {
            if let (Some((entity, transform)), Some(door)) = (tano, door) {
                let at = Vec2::new(transform.translation.x, -transform.translation.z);
                commands
                    .entity(entity)
                    .remove::<StandingNpc>()
                    .insert(Scripted {
                        way: vec![at, door.center, door.center - door.outward * INSIDE_DEPTH],
                        next: 1,
                        pace: LEAVING_PACE,
                        room: pulperia
                            .as_ref()
                            .map(|(inside, floor)| (inside.floor, *floor)),
                        fade: true,
                        then: Then::Vanish,
                    });
            }
            talk.cue = None;
        }
        Cue::RunToHorse => {
            if running.iter().any(|s| s.then == Then::Mount) {
                return;
            }
            let Some(door) = door else {
                commands.entity(me).insert(Pillion(FigureKind::Bairoletto));
                talk.cue = None;
                talk.partner = None;
                return;
            };
            let start = door.center - door.outward * 2.0;
            let room = pulperia
                .as_ref()
                .map(|(inside, floor)| (inside.floor, *floor));
            let height = room.map_or_else(|| world.0.ground_height(start), |(_, f)| f);
            let outside = door.center + door.outward * 1.0;
            let horse = beside_horse(player, town).position;
            commands.spawn((
                Name::new("Bairoletto"),
                Roaming,
                CharacterVisual::Figure(npc_figure(Npc::Bairoletto)),
                VisualPose {
                    heading: Heading::from_radians(door.outward.y.atan2(door.outward.x)),
                    ..default()
                },
                Transform::from_translation(world_from_ground(start, height)),
                Scripted {
                    way: vec![start, outside, horse],
                    next: 1,
                    pace: DASH_PACE,
                    room,
                    fade: false,
                    then: Then::Mount,
                },
            ));
            // La escena sigue cuando sube ([`run_scripted`]).
        }
        Cue::GetOff => {
            if player.rider.speed.abs() > STOPPED {
                return;
            }
            let at = beside_horse(player, town);
            errand.0.place(at);
            commands.entity(me).remove::<Pillion>();
            let height = world.0.ground_height(at.position);
            commands.spawn((
                Name::new("Bairoletto"),
                Roaming,
                StandingNpc {
                    town,
                    npc: Npc::Bairoletto,
                },
                CharacterVisual::Figure(npc_figure(Npc::Bairoletto)),
                VisualPose {
                    heading: at.heading,
                    ..default()
                },
                Transform::from_translation(world_from_ground(at.position, height)),
            ));
            talk.cue = None;
        }
        Cue::Leave => {
            if let Some((entity, transform)) = tano {
                let at = Vec2::new(transform.translation.x, -transform.translation.z);
                let off = at - player.rider.position;
                let away = if off.length() > 1e-3 {
                    off * (1.0 / off.length())
                } else {
                    Vec2::new(1.0, 0.0)
                };
                commands
                    .entity(entity)
                    .remove::<StandingNpc>()
                    .insert(Scripted {
                        way: vec![at, at + away * WALK_OFF],
                        next: 1,
                        pace: LEAVING_PACE,
                        room: None,
                        fade: true,
                        then: Then::Vanish,
                    });
            }
            talk.cue = None;
        }
        // Las cumple `converse`.
        Cue::Pause(_) | Cue::Caption(_) => {}
    }
}

/// Mueve a Bairoletto por su camino ([`Scripted`]) con el ciclo de caminata o
/// de trote, girando hacia donde va, con los pies en el piso. Al final se
/// pierde (difuminándose, si corresponde) o se sube en ancas al caballo del
/// jugador, y entonces la escena sigue.
pub(crate) fn run_scripted(
    mut commands: Commands,
    time: Res<Time>,
    world: Res<GameWorld>,
    mut conversation: ResMut<Conversation>,
    players: Query<(Entity, &Player)>,
    mut walkers: Query<(Entity, &mut Scripted, &mut Transform, &mut VisualPose)>,
) {
    let dt = time.delta_secs().min(MAX_STEP);
    for (entity, mut walk, mut transform, mut pose) in &mut walkers {
        let mut at = Vec2::new(transform.translation.x, -transform.translation.z);
        // Corre hacia donde está ahora el caballo, aunque se haya corrido.
        if walk.then == Then::Mount
            && let Ok((_, player)) = players.single()
            && let Some(last) = walk.way.last_mut()
        {
            *last = beside_horse(player, 0).position;
        }
        let mut step = walk.pace * dt;
        while let Some(&to) = walk.way.get(walk.next) {
            let gap = at.distance(to);
            if gap > step {
                let toward = (to - at) * (1.0 / gap);
                at += toward * step;
                pose.heading = Heading::from_radians(toward.y.atan2(toward.x));
                break;
            }
            at = to;
            step -= gap;
            walk.next += 1;
        }
        if walk.next >= walk.way.len() {
            commands.entity(entity).despawn();
            if let Then::Saddle(saddle) = walk.then {
                crate::chase::mount(&mut commands, saddle, &world.0);
            }
            if walk.then == Then::Mount {
                if let Ok((me, _)) = players.single() {
                    commands.entity(me).insert(Pillion(FigureKind::Bairoletto));
                }
                // Arriba: el jugador ya maneja mientras siguen hablando.
                if let Some(talk) = conversation.0.as_mut() {
                    talk.cue = None;
                    talk.partner = None;
                }
            }
            continue;
        }
        if walk.fade {
            let left = at.distance(walk.way[walk.next])
                + walk.way[walk.next..]
                    .windows(2)
                    .map(|w| w[0].distance(w[1]))
                    .sum::<f32>();
            let visible = left / (walk.pace * FADE_SECONDS);
            if visible < 1.0 {
                commands.entity(entity).insert(Fading(visible));
            }
        }
        pose.speed = walk.pace;
        pose.animation = if walk.pace > LEAVING_PACE * 2.0 {
            AnimationState::Gallop
        } else {
            AnimationState::Walk
        };
        let ground = match walk.room {
            Some((room, floor)) if room.contains_point(at) => floor,
            _ => world.0.ground_height(at),
        };
        transform.translation = world_from_ground(at, ground);
    }
}
