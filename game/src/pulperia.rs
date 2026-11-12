//! Las pulperías y los almacenes abren y cierran
//! ([`pampa_core::pulperia::is_open`]). De día
//! la puerta del medio queda abierta y se entra caminando, sin ninguna tecla.
//! De noche se cierra en cuanto el jugador no está adentro ni en el umbral, y
//! su hueco pasa a ser una pared en la colisión del mundo, y render cierra sus
//! hojas ([`PulperiaShut`], por local). Parado frente a la puerta cerrada, F
//! golpea y el pulpero (o el almacenero) contesta desde adentro, como
//! subtítulo.
//!
//! F es la tecla de contexto compartida ([`INTERACT_KEY`]): golpear la toma
//! solo cuando ni la persona del encargo ni un caballo al alcance la reclaman
//! ([`FClaims`]), y consume la pulsación cuando golpea.
//!
//! Mientras se puede golpear, un aviso "F: golpear" ([`KNOCK_HINT`]) se rotula
//! abajo en el centro, sobre el mismo papel que el aviso de los horarios.

use bevy::prelude::*;
use pampa_core::pulperia::{Interior, is_open};
use pampa_core::{Aabb, BuildingKind, Travel, Vec2 as GroundVec2};
use pampa_render::hud::{LINE_PX, ink_font, panel_colors};
use pampa_render::subtitle::duration;
use pampa_render::{PulperiaShut, Subtitle, SubtitleLine, Typefaces, WorldClock, WorldTowns};

use crate::errand::{FClaims, INTERACT_KEY};
use crate::player::{ControlLock, GameWorld, Player};
use crate::ride::OnTrain;

/// Quién contesta un golpe a la puerta cerrada de la pulpería.
pub const PULPERO: &str = "Pulpero";
/// Quién contesta un golpe a la puerta cerrada de un almacén.
pub const ALMACENERO: &str = "Almacenero";
/// Lo que contesta (un borrador, a revisar).
pub const CLOSED_LINE: &str = "¡Está cerrado! Vuelva mañana, que a esta hora no se atiende.";
/// Qué tan cerca de la puerta, del lado de la calle, se golpea, en metros.
pub const KNOCK_RANGE: f32 = 1.5;

/// El aviso de que F golpea la puerta.
pub const KNOCK_HINT: &str = "F: golpear";

/// Si se muestra el aviso [`KNOCK_HINT`]: el jugador está donde F golpea, y el
/// pulpero no está contestando.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct KnockHint(pub bool);

/// La raíz del aviso [`KNOCK_HINT`].
#[derive(Component, Debug)]
struct KnockPrompt;

/// La puerta de cada pulpería, y si está cerrada en la colisión del mundo.
#[derive(Resource, Debug, Default)]
pub struct PulperiaDoors(pub Vec<PulperiaDoor>);

/// La puerta del local `building` (la pulpería o un almacén, por su índice en
/// los `buildings`) del pueblo `town` (su índice en [`WorldTowns`]).
#[derive(Debug, Clone)]
pub struct PulperiaDoor {
    pub town: usize,
    pub building: usize,
    /// Si es un almacén: contesta el almacenero, no el pulpero.
    pub almacen: bool,
    pub inside: Interior,
    /// Si está cerrada en la colisión del mundo y en [`PulperiaShut`].
    pub shut: bool,
}

pub struct PulperiaPlugin;

impl Plugin for PulperiaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PulperiaDoors>()
            .init_resource::<PulperiaShut>()
            .init_resource::<Subtitle>()
            .init_resource::<KnockHint>()
            .init_resource::<Typefaces>()
            .add_systems(Startup, spawn_hint)
            .add_systems(PostUpdate, draw_hint)
            .add_systems(
                Update,
                (shut_at_night, knock)
                    .chain()
                    .before(crate::errand::interact)
                    .before(crate::ride::ride_keys),
            );
    }
}

/// Cierra la puerta de cada pulpería de noche, cuando el jugador no queda
/// adentro, y la abre de día: en la colisión del mundo y, por pueblo, en
/// [`PulperiaShut`], que render sigue para mover las hojas.
fn shut_at_night(
    clock: Option<Res<WorldClock>>,
    towns: Option<Res<WorldTowns>>,
    world: Option<ResMut<GameWorld>>,
    players: Query<&Player>,
    (mut doors, mut shared): (ResMut<PulperiaDoors>, ResMut<PulperiaShut>),
) {
    let (Some(towns), Some(mut world)) = (towns, world) else {
        return;
    };
    if towns.is_changed() {
        for door in doors.0.drain(..) {
            if door.shut {
                world.0.obstacles.remove(&door.inside.doorway());
            }
        }
        if !shared.0.is_empty() {
            shared.0.clear();
        }
        doors.0 = towns
            .0
            .iter()
            .enumerate()
            .flat_map(|(town, t)| {
                t.rooms().map(move |(building, b, inside)| PulperiaDoor {
                    town,
                    building,
                    almacen: b.kind == BuildingKind::GeneralStore,
                    inside,
                    shut: false,
                })
            })
            .collect();
    }
    let night = clock.is_some_and(|c| !is_open(c.0.time_of_day()));
    let at = players.single().ok().map(|p| p.rider.position);
    for door in &mut doors.0 {
        let want = night && !at.is_some_and(|p| door.inside.holds(p));
        if want == door.shut {
            continue;
        }
        let doorway: Aabb = door.inside.doorway();
        if want {
            world.0.obstacles.push(doorway);
            shared.0.insert((door.town, door.building));
        } else {
            world.0.obstacles.remove(&doorway);
            shared.0.remove(&(door.town, door.building));
        }
        door.shut = want;
    }
}

/// La puerta cerrada de una pulpería frente a la cual está parado `at`. Con
/// el jugador adentro o en el umbral la puerta nunca está cerrada, así que
/// solo se golpea desde la vereda.
fn shut_door_at(doors: &PulperiaDoors, at: GroundVec2) -> Option<&PulperiaDoor> {
    doors
        .0
        .iter()
        .find(|door| door.shut && at.distance(door.inside.door.center) <= KNOCK_RANGE)
}

/// F frente a una puerta cerrada: golpea, y el pulpero o el almacenero
/// contesta. También
/// decide si se muestra el aviso.
#[allow(clippy::too_many_arguments)]
fn knock(
    time: Res<Time>,
    keys: Option<ResMut<ButtonInput<KeyCode>>>,
    doors: Res<PulperiaDoors>,
    claims: FClaims,
    lock: Option<Res<ControlLock>>,
    players: Query<(&Player, Has<OnTrain>)>,
    (mut subtitle, mut hint): (ResMut<Subtitle>, ResMut<KnockHint>),
    (mut left, mut said): (Local<f32>, Local<Option<SubtitleLine>>),
) {
    if *left > 0.0 {
        *left -= time.delta_secs();
        if *left <= 0.0 && subtitle.0 == *said {
            subtitle.0 = None;
        }
    }
    // Donde F golpea: a pie, frente a una puerta cerrada, con la F libre.
    let door = players.single().ok().and_then(|(player, aboard)| {
        let rider = &player.rider;
        let free = !aboard
            && rider.travel() == Travel::OnFoot
            && !lock.is_some_and(|l| l.0)
            && !claims.taken(rider.position);
        free.then(|| shut_door_at(&doors, rider.position)).flatten()
    });
    let mut keys = keys;
    if let Some(door) = door
        && keys.as_ref().is_some_and(|k| k.just_pressed(INTERACT_KEY))
    {
        if let Some(keys) = keys.as_mut() {
            keys.clear_just_pressed(INTERACT_KEY);
        }
        *left = duration(CLOSED_LINE);
        let answer = SubtitleLine {
            speaker: if door.almacen { ALMACENERO } else { PULPERO }.to_string(),
            text: CLOSED_LINE.to_string(),
        };
        *said = Some(answer.clone());
        subtitle.0 = Some(answer);
    }
    hint.set_if_neq(KnockHint(door.is_some() && *left <= 0.0));
}

/// El aviso, oculto hasta que [`KnockHint`] lo pide.
fn spawn_hint(mut commands: Commands, faces: Res<Typefaces>) {
    let (border, background) = panel_colors();
    commands
        .spawn((
            Name::new("Knock prompt"),
            KnockPrompt,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(48.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((
            Node {
                border: UiRect::all(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            border,
            background,
            children![(Text::new(KNOCK_HINT), ink_font(&faces.body, LINE_PX))],
        ));
}

/// Muestra u oculta el aviso cuando cambia [`KnockHint`].
fn draw_hint(hint: Res<KnockHint>, mut prompts: Query<&mut Node, With<KnockPrompt>>) {
    if !hint.is_changed() {
        return;
    }
    for mut node in &mut prompts {
        node.display = if hint.0 { Display::Flex } else { Display::None };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Circle, GameClock, Heading, Rider, Terrain, TimeOfDay, TownLayout, World};
    use std::time::Duration;

    const FRAME: f32 = 1.0 / 60.0;

    /// Castex con su pulpería, el jugador a pie en `v` metros hacia adentro de
    /// la puerta (negativo: en la vereda), a la hora `time`.
    fn app(v: f32, time: TimeOfDay) -> (App, Interior) {
        let town = TownLayout::generate(&pampa_core::town::TownConfig::castex()).unwrap();
        let (_, inside) = town.pulperia().unwrap();
        let door = inside.door;
        let at = door.center - door.outward * v;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(World::with_town(Terrain::default(), &town)))
            .insert_resource(WorldTowns(vec![town.clone()]))
            .insert_resource(WorldClock(GameClock::at(time)))
            .insert_resource(PlayerSpawn {
                position: at,
                heading: Heading::from_radians(0.0),
                travel: Travel::OnFoot,
            })
            .add_plugins((PlayerPlugin, PulperiaPlugin));
        app.update();
        (app, inside)
    }

    fn shut(app: &App) -> bool {
        app.world().resource::<PulperiaDoors>().0[0].shut
    }

    /// Si un cuerpo de persona parado en el umbral queda donde está.
    fn doorway_free(app: &App, inside: &Interior) -> bool {
        let world = &app.world().resource::<GameWorld>().0;
        let p = inside.door.center - inside.door.outward * 0.2;
        let person = Circle {
            radius: Rider::PERSON_RADIUS,
        };
        world.resolve(person, p, |_| {}) == p
    }

    fn set_time(app: &mut App, time: TimeOfDay) {
        app.insert_resource(WorldClock(GameClock::at(time)));
        app.update();
    }

    fn press_f(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(INTERACT_KEY);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(INTERACT_KEY);
        keys.clear();
    }

    fn said(app: &App) -> Option<SubtitleLine> {
        app.world().resource::<Subtitle>().0.clone()
    }

    #[test]
    fn by_day_the_door_is_open_and_at_night_it_shuts_like_a_wall() {
        let (mut app, inside) = app(-3.0, TimeOfDay::hm(12, 0));
        assert!(!shut(&app));
        assert!(doorway_free(&app, &inside));
        set_time(&mut app, TimeOfDay::hm(23, 0));
        assert!(shut(&app));
        assert!(!doorway_free(&app, &inside));
        // A la mañana abre otra vez.
        set_time(&mut app, TimeOfDay::hm(7, 0));
        assert!(!shut(&app));
        assert!(doorway_free(&app, &inside));
    }

    #[test]
    fn it_does_not_shut_with_the_player_inside() {
        let (mut app, _) = app(5.0, TimeOfDay::hm(23, 0));
        assert!(!shut(&app), "the last customer is still in");
        // Sale a la vereda: ahora sí cierra.
        let door = app.world().resource::<PulperiaDoors>().0[0].inside.door;
        let out = door.center + door.outward * 3.0;
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = out;
        app.update();
        assert!(shut(&app));
    }

    #[test]
    fn f_at_the_shut_door_knocks_and_the_pulpero_answers_for_a_while() {
        let (mut app, _) = app(-1.0, TimeOfDay::hm(23, 0));
        assert!(shut(&app));
        press_f(&mut app);
        let answer = SubtitleLine {
            speaker: "Pulpero".to_string(),
            text: "¡Está cerrado! Vuelva mañana, que a esta hora no se atiende.".to_string(),
        };
        assert_eq!(said(&app), Some(answer));
        let frames = (duration(CLOSED_LINE) / FRAME) as usize + 2;
        for _ in 0..frames {
            app.update();
        }
        assert_eq!(said(&app), None);
    }

    #[test]
    fn f_does_nothing_at_an_open_door_or_away_from_it() {
        let (mut app, _) = app(-1.0, TimeOfDay::hm(12, 0));
        press_f(&mut app);
        assert_eq!(said(&app), None, "open by day");
        let (mut app, _) = app_far();
        press_f(&mut app);
        assert_eq!(said(&app), None, "too far from the door");
    }

    /// El texto del aviso si está a la vista.
    fn hint_shown(app: &mut App) -> Option<String> {
        let mut roots = app
            .world_mut()
            .query_filtered::<(&Node, &Children), With<KnockPrompt>>();
        let (node, children) = roots.single(app.world()).unwrap();
        if node.display == Display::None {
            return None;
        }
        let frame = children[0];
        let line = app.world().entity(frame).get::<Children>().unwrap()[0];
        Some(app.world().entity(line).get::<Text>().unwrap().0.clone())
    }

    #[test]
    fn the_hint_shows_at_the_shut_door_and_hides_while_the_pulpero_answers() {
        let (mut app, _) = app(-1.0, TimeOfDay::hm(23, 0));
        app.update();
        assert_eq!(hint_shown(&mut app).as_deref(), Some("F: golpear"));
        press_f(&mut app);
        app.update();
        assert_eq!(hint_shown(&mut app), None, "while the pulpero answers");
        let frames = (duration(CLOSED_LINE) / FRAME) as usize + 2;
        for _ in 0..frames {
            app.update();
        }
        assert_eq!(hint_shown(&mut app).as_deref(), Some("F: golpear"));
        // De día, ningún aviso.
        set_time(&mut app, TimeOfDay::hm(12, 0));
        app.update();
        assert_eq!(hint_shown(&mut app), None, "open by day");
        // Ni lejos de la puerta.
        let (mut far, _) = app_far();
        far.update();
        assert_eq!(hint_shown(&mut far), None, "too far");
    }

    #[test]
    fn a_horse_in_reach_keeps_the_f() {
        let (mut app, _) = app(-1.0, TimeOfDay::hm(23, 0));
        let beside = {
            let mut q = app.world_mut().query::<&Player>();
            q.single(app.world()).unwrap().rider.position + GroundVec2::new(1.5, 0.0)
        };
        app.insert_resource(crate::errand::LooseHorse(Some(
            pampa_core::TetheredHorse::standing(beside, Heading::from_radians(0.0)),
        )));
        press_f(&mut app);
        assert_eq!(said(&app), None);
        app.update();
        assert_eq!(hint_shown(&mut app), None, "F is the horse's");
    }

    #[test]
    fn the_shut_doors_are_shared_with_render_by_town_and_building() {
        // Un pueblo sin pulpería primero (se le saca la suya, le quedan sus
        // almacenes): la pulpería de Castex es del pueblo 1.
        let mut pico = TownLayout::generate(&pampa_core::town::TownConfig {
            name: "General Pico".into(),
            blocks: (8, 8),
            origin: GroundVec2::new(3000.0, 0.0),
            ..pampa_core::town::TownConfig::castex()
        })
        .unwrap();
        pico.buildings
            .retain(|b| b.kind != pampa_core::BuildingKind::Pulperia);
        assert!(pico.pulperia().is_none());
        let (mut app, _) = app(-3.0, TimeOfDay::hm(12, 0));
        let castex = app.world().resource::<WorldTowns>().0[0].clone();
        app.insert_resource(WorldTowns(vec![pico, castex]));
        app.update();
        let shut = |app: &App| app.world().resource::<PulperiaShut>().0.clone();
        assert!(shut(&app).is_empty(), "open by day");
        set_time(&mut app, TimeOfDay::hm(23, 0));
        let towns = app.world().resource::<WorldTowns>().0.clone();
        let every: std::collections::BTreeSet<(usize, usize)> = towns
            .iter()
            .enumerate()
            .flat_map(|(t, town)| town.rooms().map(move |(b, _, _)| (t, b)))
            .collect();
        let pico_almacenes = towns[0]
            .buildings
            .iter()
            .filter(|b| b.kind == pampa_core::BuildingKind::GeneralStore)
            .count();
        assert!(pico_almacenes >= 1);
        assert_eq!(every.len(), pico_almacenes + 2, "and Castex's two");
        assert_eq!(shut(&app), every);
        set_time(&mut app, TimeOfDay::hm(7, 0));
        assert!(shut(&app).is_empty(), "open again in the morning");
    }

    fn app_far() -> (App, Interior) {
        app(-(KNOCK_RANGE + 0.5), TimeOfDay::hm(23, 0))
    }
}
