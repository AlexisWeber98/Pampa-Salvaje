//! The first errand, played: the player gets off the morning train at the
//! first town on foot, talks to Don Hidalgo in the plaza, takes his horse
//! from the general store and rides to report to comisario Farach.
//!
//! `pampa-core` owns the rules ([`Errand`]): who stands where, the stages
//! and what is in range. This module reads the interact key
//! ([`INTERACT_KEY`]), plays the lines from `assets/dialogue/errand.ron` as
//! speech bubbles ([`Say`]), one every bubble lifetime, puts the player in
//! the saddle, and keeps the HUD objective ([`HudObjective`]) and the
//! minimap target ([`MapTarget`]) in step with the errand.

use bevy::prelude::*;
use pampa_content::{ERRAND_FILE, ErrandLines, asset_path, load_errand_or_builtin};
use pampa_core::{
    Errand, ErrandEvent, Heading, Interaction, MAX_STEP, Npc, TownLayout, Vec2 as GroundVec2, World,
};
use pampa_render::bubble::LIFETIME;
use pampa_render::character::Billboard;
use pampa_render::{
    CharacterVisual, HudObjective, MapTarget, Say, SheetKind, font, world_from_ground,
};

use crate::life::{Cast, LedAway, StandingNpc, TetheredHorse};
use crate::player::{DrivePlayer, GameWorld, Player};

/// The one context key: talk, or mount. (E orbits the camera, M toggles
/// the map, Esc quits.)
pub const INTERACT_KEY: KeyCode = KeyCode::KeyF;

/// The town whose comisario the errand sends the player to; without it,
/// the nearest other town with a police station.
pub const POLICE_TOWN: &str = "Eduardo Castex";

/// The errand underway.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ErrandRun(pub Errand);

/// The errand's lines.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ErrandDialogue(pub ErrandLines);

/// The errand moved on (each event is written once).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrandProgress(pub ErrandEvent);

/// Who says a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Npc(Npc),
    Player,
}

/// Lines being said one after the other, each shown for a bubble's
/// lifetime.
#[derive(Debug, Clone, PartialEq)]
pub struct Talk {
    pub speaker: Speaker,
    pub lines: Vec<String>,
    /// Index of the next line to say.
    pub next: usize,
    /// Seconds until the next line.
    pub wait: f32,
}

impl Talk {
    fn new(speaker: Speaker, lines: Vec<String>) -> Self {
        Self {
            speaker,
            lines,
            next: 0,
            wait: 0.0,
        }
    }
}

/// The conversation playing, if any. A new one replaces it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Conversation(pub Option<Talk>);

/// The errand of a world of `towns`, given the first town: Don Hidalgo in
/// its plaza, Farach at [`POLICE_TOWN`]. Their footprints are added to
/// `world` as obstacles. `None` (no errand) without a second town with a
/// police station.
pub fn plan(towns: &[TownLayout], world: &mut World) -> Option<(ErrandRun, Cast)> {
    let giver = 0;
    let police = Errand::police_town(towns, giver, POLICE_TOWN)?;
    let errand = Errand::new(towns, giver, police)?;
    world.obstacles.extend(errand.obstacles());
    let cast = Cast(
        [Npc::Hidalgo, Npc::Farach]
            .map(|npc| (npc, errand.standing(npc)))
            .to_vec(),
    );
    Some((ErrandRun(errand), cast))
}

/// Where the player starts: on foot on top of the first town's platform,
/// beside the coaches of the morning train standing there, facing town (on
/// the road in when it has no platform).
pub fn player_start(town: &TownLayout) -> (GroundVec2, Heading) {
    town.platform_spot().unwrap_or_else(|| town.arrival())
}

pub struct ErrandPlugin;

impl Plugin for ErrandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Say>()
            .add_message::<ErrandProgress>()
            .init_resource::<Conversation>()
            .init_resource::<HudObjective>()
            .init_resource::<MapTarget>()
            .init_resource::<LedAway>()
            .init_resource::<GameWorld>()
            .add_systems(Startup, load_dialogue)
            .add_systems(
                Update,
                (interact, converse, show_progress)
                    .chain()
                    .after(DrivePlayer),
            );
    }
}

/// Reads `assets/dialogue/errand.ron` unless the lines were already
/// provided, falling back to the built-in lines with a warning.
fn load_dialogue(mut commands: Commands, existing: Option<Res<ErrandDialogue>>) {
    if existing.is_some() {
        return;
    }
    let path = asset_path(ERRAND_FILE);
    let (lines, error) = load_errand_or_builtin(&path);
    match error {
        Some(error) => warn!("errand dialogue: {error}; using the built-in lines"),
        None => info!("errand dialogue loaded from {}", path.display()),
    }
    let unsupported = lines.unsupported_chars(font::supports);
    if !unsupported.is_empty() {
        warn!("errand dialogue uses characters the font cannot draw: {unsupported:?}");
    }
    commands.insert_resource(ErrandDialogue(lines));
}

/// What [`interact`] changes when the player mounts.
type MountState<'w> = (ResMut<'w, GameWorld>, ResMut<'w, LedAway>);

/// The interact key: talks to whoever is in range and due, or mounts Don
/// Hidalgo's horse.
#[allow(clippy::too_many_arguments)]
fn interact(
    mut commands: Commands,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    errand: Option<ResMut<ErrandRun>>,
    dialogue: Option<Res<ErrandDialogue>>,
    mut conversation: ResMut<Conversation>,
    (mut world, mut led_away): MountState,
    mut players: Query<(Entity, &mut Player, &mut Transform)>,
    horses: Query<(Entity, &TetheredHorse)>,
    billboards: Query<(Entity, &ChildOf), With<Billboard>>,
    mut progress: MessageWriter<ErrandProgress>,
) {
    if !keys.is_some_and(|k| k.just_pressed(INTERACT_KEY)) {
        return;
    }
    let (Some(mut errand), Some(dialogue)) = (errand, dialogue) else {
        return;
    };
    let Ok((me, mut player, mut transform)) = players.single_mut() else {
        return;
    };
    let lines = &dialogue.0;
    let event = match errand.0.interact(player.rider.position) {
        Interaction::Nothing => return,
        Interaction::NotYourHorse => {
            conversation.0 = Some(Talk::new(Speaker::Player, lines.not_your_horse.clone()));
            return;
        }
        Interaction::Event(event) => event,
    };
    progress.write(ErrandProgress(event));
    match event {
        ErrandEvent::TalkedToHidalgo => {
            conversation.0 = Some(Talk::new(Speaker::Npc(Npc::Hidalgo), lines.hidalgo.clone()));
        }
        ErrandEvent::ReportedToFarach => {
            conversation.0 = Some(Talk::new(Speaker::Npc(Npc::Farach), lines.farach.clone()));
        }
        ErrandEvent::MountedHorse => {
            let (town, horse) = errand.0.horse();
            // Led away for the rest of the session: the tied horse and its
            // footprint go, and the player rides it from where it stood.
            world.0.obstacles.remove(&horse.footprint);
            led_away.0.insert(town);
            for (entity, tied) in &horses {
                if tied.0 == town {
                    commands.entity(entity).despawn();
                }
            }
            let rider = &mut player.rider;
            *rider = rider.mount(&horse, &world.0);
            transform.translation = world_from_ground(rider.position, rider.height);
            // A new sprite: drop the gaucho's billboard, draw the rider.
            for (billboard, parent) in &billboards {
                if parent.parent() == me {
                    commands.entity(billboard).despawn();
                }
            }
            commands
                .entity(me)
                .remove::<CharacterVisual>()
                .insert(CharacterVisual::Billboard(SheetKind::Rider));
        }
    }
}

/// Says the conversation's lines one after the other, each for a bubble's
/// lifetime, above whoever speaks (if they are around).
fn converse(
    time: Res<Time>,
    mut conversation: ResMut<Conversation>,
    players: Query<Entity, With<Player>>,
    npcs: Query<(Entity, &StandingNpc)>,
    mut say: MessageWriter<Say>,
) {
    let Some(talk) = conversation.0.as_mut() else {
        return;
    };
    talk.wait -= time.delta_secs().min(MAX_STEP);
    if talk.wait > 0.0 {
        return;
    }
    let Some(line) = talk.lines.get(talk.next).cloned() else {
        conversation.0 = None;
        return;
    };
    talk.next += 1;
    talk.wait = LIFETIME;
    let speaker = match talk.speaker {
        Speaker::Player => players.iter().next(),
        Speaker::Npc(npc) => npcs.iter().find(|(_, n)| n.npc == npc).map(|(e, _)| e),
    };
    if let Some(speaker) = speaker {
        say.write(Say { speaker, line });
    }
}

/// Keeps the HUD objective and the minimap target on the current stage.
fn show_progress(
    errand: Option<Res<ErrandRun>>,
    mut objective: ResMut<HudObjective>,
    mut target: ResMut<MapTarget>,
) {
    let stage = errand.as_ref().map(|e| e.0.stage());
    let text = stage.and_then(|s| s.objective()).map(str::to_string);
    objective.set_if_neq(HudObjective(text));
    target.set_if_neq(MapTarget(errand.and_then(|e| e.0.target())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{Crowds, TownLifePlugin, start_of_day};
    use crate::player::{PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{ErrandStage, Terrain, Travel};
    use pampa_render::{WorldClock, WorldTowns, WorldTrain};
    use std::time::Duration;

    /// Every line said, by whom.
    #[derive(Resource, Default)]
    struct Said(Vec<(Entity, String)>);

    #[derive(Resource, Default)]
    struct Progress(Vec<ErrandEvent>);

    fn log(
        mut says: MessageReader<Say>,
        mut progress: MessageReader<ErrandProgress>,
        mut said: ResMut<Said>,
        mut events: ResMut<Progress>,
    ) {
        said.0
            .extend(says.read().map(|s| (s.speaker, s.line.clone())));
        events.0.extend(progress.read().map(|p| p.0));
    }

    fn shipped_lines() -> ErrandLines {
        let (lines, error) = load_errand_or_builtin(&asset_path(ERRAND_FILE));
        assert!(error.is_none(), "{error:?}");
        lines
    }

    /// The shipped world, the morning train dwelling at Monte Nievas, and
    /// the player on foot at the platform. Frames last 50 ms.
    fn shipped_app() -> App {
        let loaded = crate::towns::load_world(&asset_path(pampa_content::TOWNS_FILE));
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let (run, cast) = plan(&loaded.towns, &mut world).expect("the shipped errand");
        let (position, heading) = player_start(&loaded.towns[0]);
        let (clock, train) = start_of_day(&loaded.line, &loaded.towns);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
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
                travel: Travel::OnFoot,
            })
            .insert_resource(run)
            .insert_resource(cast)
            .insert_resource(ErrandDialogue(shipped_lines()))
            .init_resource::<Said>()
            .init_resource::<Progress>()
            .add_plugins((PlayerPlugin, TownLifePlugin, ErrandPlugin))
            .add_systems(PostUpdate, log);
        app
    }

    fn player(app: &mut App) -> (Entity, Player) {
        let mut q = app.world_mut().query::<(Entity, &Player)>();
        let (e, p) = q.single(app.world()).unwrap();
        (e, Player { rider: p.rider })
    }

    fn teleport(app: &mut App, to: GroundVec2) {
        let mut q = app.world_mut().query::<&mut Player>();
        let mut p = q.single_mut(app.world_mut()).unwrap();
        p.rider.position = to;
        p.rider.speed = 0.0;
    }

    /// Presses the interact key for exactly one frame.
    fn press_f(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(INTERACT_KEY);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(INTERACT_KEY);
        keys.clear();
    }

    fn errand(app: &App) -> Errand {
        app.world().resource::<ErrandRun>().0.clone()
    }

    fn npc_entity(app: &mut App, npc: Npc) -> Entity {
        let mut q = app.world_mut().query::<(Entity, &StandingNpc)>();
        q.iter(app.world())
            .find(|(_, n)| n.npc == npc)
            .map(|(e, _)| e)
            .expect("standing in town")
    }

    fn objective(app: &App) -> Option<String> {
        app.world().resource::<HudObjective>().0.clone()
    }

    fn said_by(app: &App, speaker: Entity) -> Vec<String> {
        let said = &app.world().resource::<Said>().0;
        said.iter()
            .filter(|(e, _)| *e == speaker)
            .map(|(_, l)| l.clone())
            .collect()
    }

    /// Lets every line of a conversation play out.
    fn wait_for_lines(app: &mut App, lines: usize) {
        let frames = ((lines as f32 + 1.0) * LIFETIME / 0.05).ceil() as usize;
        for _ in 0..frames {
            app.update();
        }
        assert_eq!(app.world().resource::<Conversation>().0, None);
    }

    #[test]
    fn the_whole_errand_from_the_platform_to_the_comisaria() {
        let mut app = shipped_app();
        let lines = shipped_lines();
        app.update();
        let e = errand(&app);
        let hidalgo = e.standing(Npc::Hidalgo).position;
        let (horse_town, horse) = e.horse();
        let farach = e.standing(Npc::Farach).position;

        // On foot on top of the Monte Nievas platform, beside the dwelling
        // train's coaches, facing town; standing still leaves the player
        // where they are.
        let (start, facing) = player_start(&app.world().resource::<WorldTowns>().0[0]);
        for _ in 0..10 {
            app.update();
        }
        let (me, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::OnFoot);
        assert!(
            p.rider.position.distance(start) < 1e-4,
            "{:?}",
            p.rider.position
        );
        assert_eq!(p.rider.heading, facing);
        // Up on the platform's top, not on the ground beside it.
        let world = app.world().resource::<GameWorld>().0.clone();
        assert_eq!(p.rider.height, world.ground_height(start));
        assert!(p.rider.height > world.terrain.height(start.x, start.y) + 0.5);
        assert_eq!(
            objective(&app).as_deref(),
            ErrandStage::MeetHidalgo.objective()
        );
        assert_eq!(app.world().resource::<MapTarget>().0, Some(hidalgo));

        // Nothing to do on the platform.
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::MeetHidalgo);
        assert!(app.world().resource::<Said>().0.is_empty());

        // Not his horse yet.
        teleport(&mut app, beside(&horse));
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::MeetHidalgo);
        assert_eq!(said_by(&app, me), lines.not_your_horse[..1]);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);

        // Don Hidalgo in the plaza tells the whole story, line by line.
        teleport(
            &mut app,
            hidalgo + e.standing(Npc::Hidalgo).heading.forward() * 2.0,
        );
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::TakeHorse);
        assert_eq!(
            objective(&app).as_deref(),
            ErrandStage::TakeHorse.objective()
        );
        assert_eq!(app.world().resource::<MapTarget>().0, Some(horse.position));
        wait_for_lines(&mut app, lines.hidalgo.len());
        let don = npc_entity(&mut app, Npc::Hidalgo);
        assert_eq!(said_by(&app, don), lines.hidalgo);
        press_f(&mut app);
        assert_eq!(said_by(&app, don).len(), lines.hidalgo.len(), "said once");

        // Mount his horse: the player becomes the rider, the tied horse
        // and its footprint are gone.
        teleport(&mut app, beside(&horse));
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::ReportToFarach);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        assert_eq!(
            (p.rider.position, p.rider.heading),
            (horse.position, horse.heading)
        );
        let visual = *app.world().get::<CharacterVisual>(me).unwrap();
        assert_eq!(visual, CharacterVisual::Billboard(SheetKind::Rider));
        assert!(
            !app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        assert!(app.world().resource::<LedAway>().0.contains(&horse_town));
        let mut tied = app.world_mut().query::<&TetheredHorse>();
        assert!(tied.iter(app.world()).all(|h| h.0 != horse_town));
        assert_eq!(app.world().resource::<MapTarget>().0, Some(farach));
        assert_eq!(
            objective(&app).as_deref(),
            ErrandStage::ReportToFarach.objective()
        );

        // Ride to Castex and report, without getting off.
        // 2.5 m in front of him, out on the street.
        teleport(
            &mut app,
            farach + e.standing(Npc::Farach).heading.forward() * 2.5,
        );
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::Done);
        wait_for_lines(&mut app, lines.farach.len());
        let comisario = npc_entity(&mut app, Npc::Farach);
        assert_eq!(said_by(&app, comisario), lines.farach);
        assert_eq!(objective(&app), None);
        assert_eq!(app.world().resource::<MapTarget>().0, None);

        // Each step happened once.
        press_f(&mut app);
        assert_eq!(
            app.world().resource::<Progress>().0,
            [
                ErrandEvent::TalkedToHidalgo,
                ErrandEvent::MountedHorse,
                ErrandEvent::ReportedToFarach
            ]
        );
    }

    /// 2.5 m ahead of the tied horse along the street: clear of the horse
    /// and its rail, within reach.
    fn beside(horse: &pampa_core::TetheredHorse) -> GroundVec2 {
        horse.position + horse.heading.forward() * 2.5
    }

    #[test]
    fn the_shipped_lines_only_use_characters_the_font_draws() {
        for lines in [shipped_lines(), ErrandLines::builtin()] {
            assert_eq!(lines.unsupported_chars(font::supports), Vec::<char>::new());
        }
    }

    #[test]
    fn there_is_an_errand_in_the_shipped_world_from_monte_nievas_to_castex() {
        let loaded = crate::towns::load_world(&asset_path(pampa_content::TOWNS_FILE));
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let before = world.obstacles.len();
        let (run, cast) = plan(&loaded.towns, &mut world).unwrap();
        let hidalgo = run.0.standing(Npc::Hidalgo);
        let farach = run.0.standing(Npc::Farach);
        assert_eq!(loaded.towns[hidalgo.town].name, "Monte Nievas");
        assert_eq!(loaded.towns[farach.town].name, POLICE_TOWN);
        // As the story says: no comisaría in Monte Nievas, one in Castex.
        let comisarias = |t: &TownLayout| {
            t.places()
                .iter()
                .filter(|p| p.kind == pampa_core::PlaceKind::PoliceStation)
                .count()
        };
        assert_eq!(comisarias(&loaded.towns[hidalgo.town]), 0);
        assert_eq!(loaded.towns[hidalgo.town].police_door(), None);
        assert_eq!(comisarias(&loaded.towns[farach.town]), 1);
        assert_eq!(world.obstacles.len(), before + 2);
        assert_eq!(cast.0, vec![(Npc::Hidalgo, hidalgo), (Npc::Farach, farach)]);
        // The platform start is clear of everything built.
        let (start, _) = player_start(&loaded.towns[0]);
        let c = pampa_core::Circle::new(pampa_core::Rider::PERSON_RADIUS);
        assert_eq!(world.resolve(c, start, |_| ()), start);
    }

    #[test]
    fn missing_dialogue_is_loaded_at_startup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ErrandPlugin);
        app.update();
        let dialogue = app.world().resource::<ErrandDialogue>();
        assert!(!dialogue.0.hidalgo.is_empty());
        // Without an errand there is no objective and no target.
        assert_eq!(app.world().resource::<HudObjective>().0, None);
        assert_eq!(app.world().resource::<MapTarget>().0, None);
    }

    #[test]
    fn the_interact_key_is_free_of_other_controls() {
        for taken in [KeyCode::KeyE, KeyCode::KeyQ, KeyCode::KeyM, KeyCode::Escape] {
            assert_ne!(INTERACT_KEY, taken);
        }
    }
}
