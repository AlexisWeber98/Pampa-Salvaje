//! The first errand, played: the player gets off the morning train at the
//! first town on foot (the opening, see [`crate::ride`]), talks to Don Hidalgo in the plaza, takes his horse
//! from the general store and rides to report to comisario Farach in the
//! town the dialogue file names (`police_town`).
//!
//! Once taken, the horse is the player's: F while stopped or at a slow walk
//! gets off, leaving it tied loosely where it stands ([`LooseHorse`]), and F
//! beside it gets back on. Neither moves the errand on.
//!
//! Taking Don Hidalgo's horse unlocks riding ([`crate::abilities`]): from
//! then on F beside any other horse ([`Mountable`]: tied in a town, or
//! standing free) gets on it, and the one ridden before stays standing
//! where it was left. Before that, only Don Hidalgo's horse lets him on.
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
    Ability, Errand, ErrandEvent, Interaction, MAX_STEP, Npc, Rider, TownLayout, Travel, World,
};
use pampa_render::bubble::LIFETIME;
use pampa_render::{
    Animal, CharacterVisual, Coat, FigureKind, HudObjective, MapTarget, Say, Talking, VisualPose,
    font, world_from_ground,
};

use crate::abilities::{PlayerAbilities, Unlocked, announce_unlocks, unlock_on_mount};
use crate::life::{Cast, LedAway, StandingNpc, TetheredHorse};
use crate::player::{DrivePlayer, GameWorld, Player, RiddenCoat, player_visual_on};

/// The one context key: talk, mount, or get off. (E orbits the camera, M
/// toggles the map, Esc quits.)
pub const INTERACT_KEY: KeyCode = KeyCode::KeyF;

/// The horse the player got off, tied loosely where they left it; `None`
/// while riding it (or before taking it).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct LooseHorse(pub Option<pampa_core::TetheredHorse>);

/// The entity drawing the [`LooseHorse`].
#[derive(Component, Debug, Clone, Copy)]
pub struct LeftHorse;

/// A standing horse the player may get on (once riding is unlocked, see
/// [`PlayerAbilities`]): tied in a town, left loose, or standing free. Its
/// footprint is an obstacle of the [`GameWorld`] while it stands.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Mountable(pub pampa_core::TetheredHorse);

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
/// its plaza, Farach (with Bairoletto at his side) in the town named `police_town` (the dialogue's
/// [`ErrandLines::police_town`]; the nearest other town with a police
/// station when it has none). Their footprints are added to `world` as
/// obstacles. `None` (no errand) without a second town with a police
/// station.
pub fn plan(
    towns: &[TownLayout],
    world: &mut World,
    police_town: &str,
) -> Option<(ErrandRun, Cast)> {
    let giver = 0;
    let police = Errand::police_town(towns, giver, police_town)?;
    let errand = Errand::new(towns, giver, police)?;
    world.obstacles.extend(errand.obstacles());
    let cast = Cast(
        [Npc::Hidalgo, Npc::Farach]
            .map(|npc| (npc, errand.standing(npc)))
            .to_vec(),
        vec![(FigureKind::Bairoletto, errand.bairoletto())],
    );
    Some((ErrandRun(errand), cast))
}

pub struct ErrandPlugin;

impl Plugin for ErrandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Say>()
            .add_message::<ErrandProgress>()
            .add_message::<Unlocked>()
            .init_resource::<Conversation>()
            .init_resource::<HudObjective>()
            .init_resource::<MapTarget>()
            .init_resource::<LedAway>()
            .init_resource::<LooseHorse>()
            .init_resource::<GameWorld>()
            .init_resource::<PlayerAbilities>()
            .init_resource::<RiddenCoat>()
            .init_resource::<pampa_render::HudAnnouncement>()
            .init_resource::<crate::ride::AnnounceTimer>()
            .add_systems(Startup, load_dialogue)
            .add_systems(
                Update,
                (
                    interact,
                    converse,
                    mark_speakers,
                    show_progress,
                    unlock_on_mount,
                    announce_unlocks,
                )
                    .chain()
                    .after(DrivePlayer),
            );
    }
}

/// What reading the dialogue file found, logged at startup (the game reads
/// it before logging is up, to plan the errand).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct DialogueReport {
    pub info: Vec<String>,
    pub warnings: Vec<String>,
}

/// Reads `assets/dialogue/errand.ron`, falling back to the built-in lines,
/// with what to log about it.
pub fn read_dialogue() -> (ErrandDialogue, DialogueReport) {
    let path = asset_path(ERRAND_FILE);
    let (lines, error) = load_errand_or_builtin(&path);
    let mut report = DialogueReport::default();
    match error {
        Some(error) => report.warnings.push(format!(
            "errand dialogue: {error}; using the built-in lines"
        )),
        None => report
            .info
            .push(format!("errand dialogue loaded from {}", path.display())),
    }
    let unsupported = lines.unsupported_chars(font::supports);
    if !unsupported.is_empty() {
        report.warnings.push(format!(
            "errand dialogue uses characters the font cannot draw: {unsupported:?}"
        ));
    }
    (ErrandDialogue(lines), report)
}

/// Logs the [`DialogueReport`], reading the dialogue first unless the lines
/// were already provided.
fn load_dialogue(
    mut commands: Commands,
    existing: Option<Res<ErrandDialogue>>,
    report: Option<Res<DialogueReport>>,
) {
    let report = match existing {
        Some(_) => report.map(|r| r.clone()).unwrap_or_default(),
        None => {
            let (lines, report) = read_dialogue();
            commands.insert_resource(lines);
            report
        }
    };
    report.info.iter().for_each(|line| info!("{line}"));
    report.warnings.iter().for_each(|line| warn!("{line}"));
}

/// What [`interact`] changes when the player mounts or gets off.
type MountState<'w> = (
    ResMut<'w, GameWorld>,
    ResMut<'w, LedAway>,
    ResMut<'w, LooseHorse>,
    ResMut<'w, RiddenCoat>,
    Res<'w, PlayerAbilities>,
);

/// A standing horse as [`interact`] sees it.
type StandingHorse = (
    Entity,
    &'static Mountable,
    Option<&'static CharacterVisual>,
    Option<&'static Animal>,
    Option<&'static TetheredHorse>,
    Has<LeftHorse>,
);

/// The coat of a standing horse: a saddled one's visual, or a corral
/// horse's (drawn as fauna).
fn standing_coat(visual: Option<&CharacterVisual>, animal: Option<&Animal>) -> Option<Coat> {
    match (visual, animal) {
        (Some(CharacterVisual::Horse(coat)), _) | (_, Some(Animal::Horse(coat))) => Some(*coat),
        _ => None,
    }
}

/// The horses [`interact`] may change: those tied in the towns, the one
/// the player left tied loosely, and every standing horse.
type Horses<'w, 's> = (
    Query<'w, 's, (Entity, &'static TetheredHorse)>,
    Query<'w, 's, Entity, With<LeftHorse>>,
    Query<'w, 's, StandingHorse>,
);

/// The interact key: talks to whoever is in range and due, or mounts Don
/// Hidalgo's horse; with nothing else to do, gets off the horse or back on,
/// or (once riding is unlocked) onto any other horse in reach.
#[allow(clippy::too_many_arguments)]
pub(crate) fn interact(
    mut commands: Commands,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    errand: Option<ResMut<ErrandRun>>,
    dialogue: Option<Res<ErrandDialogue>>,
    mut conversation: ResMut<Conversation>,
    (mut world, mut led_away, mut loose, mut coat, abilities): MountState,
    mut players: Query<(Entity, &mut Player, &mut Transform)>,
    (horses, left, standing): Horses,
    mut progress: MessageWriter<ErrandProgress>,
) {
    if !keys.is_some_and(|k| k.just_pressed(INTERACT_KEY)) {
        return;
    }
    let Ok((me, mut player, mut transform)) = players.single_mut() else {
        return;
    };
    let mut saddle = Saddle {
        commands: &mut commands,
        world: &mut world.0,
        loose: &mut loose,
        led_away: &mut led_away,
        coat: &mut coat.0,
        left: left.iter().collect(),
    };
    let ride_any = abilities.0.has(Ability::Ride);
    let swap_or_take = |saddle: &mut Saddle, rider: &mut Rider, transform: &mut Transform| {
        if !saddle.swap(me, rider, transform) && ride_any {
            saddle.take(me, rider, transform, standing.iter());
        }
    };
    let (Some(mut errand), Some(dialogue)) = (errand, dialogue) else {
        swap_or_take(&mut saddle, &mut player.rider, &mut transform);
        return;
    };
    let lines = &dialogue.0;
    let event = match errand.0.interact(player.rider.position) {
        Interaction::Nothing => {
            swap_or_take(&mut saddle, &mut player.rider, &mut transform);
            return;
        }
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
            saddle.world.obstacles.remove(&horse.footprint);
            saddle.led_away.0.insert(town);
            for (entity, tied) in &horses {
                if tied.0 == town {
                    saddle.commands.entity(entity).despawn();
                }
            }
            *saddle.coat = Coat::HIDALGO;
            saddle.ride(me, &mut player.rider, &mut transform, &horse);
        }
    }
}

/// What getting on and off the horse changes.
struct Saddle<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    world: &'a mut World,
    loose: &'a mut LooseHorse,
    led_away: &'a mut LedAway,
    /// The coat of the horse ridden.
    coat: &'a mut Coat,
    /// The entities drawing the loose horse.
    left: Vec<Entity>,
}

impl Saddle<'_, '_, '_> {
    /// Puts `rider` (entity `me`) in the saddle of `horse`, where it
    /// stands, with the mounted visual.
    fn ride(
        &mut self,
        me: Entity,
        rider: &mut Rider,
        transform: &mut Transform,
        horse: &pampa_core::TetheredHorse,
    ) {
        *rider = rider.mount(horse, &*self.world);
        transform.translation = world_from_ground(rider.position, rider.height);
        // A new visual: render drops the figure and draws the rider.
        self.commands
            .entity(me)
            .insert(player_visual_on(rider.travel(), *self.coat));
    }

    /// Gets off the horse when riding slowly enough, leaving it tied
    /// loosely (a still horse and an obstacle), or gets back on it from
    /// within [`Errand::MOUNT_RANGE`]. `false` if it did neither.
    fn swap(&mut self, me: Entity, rider: &mut Rider, transform: &mut Transform) -> bool {
        match (rider.travel(), self.loose.0) {
            (Travel::Riding, _) => {
                let Some((person, horse)) = rider.dismount(&*self.world) else {
                    return true;
                };
                *rider = person;
                transform.translation = world_from_ground(rider.position, rider.height);
                self.commands
                    .entity(me)
                    .insert(player_visual_on(Travel::OnFoot, *self.coat));
                self.world.obstacles.push(horse.footprint);
                self.loose.0 = Some(horse);
                let height = self.world.ground_height(horse.position);
                self.commands.spawn((
                    Name::new("Loose horse"),
                    LeftHorse,
                    Mountable(horse),
                    CharacterVisual::Horse(*self.coat),
                    VisualPose {
                        heading: horse.heading,
                        ..default()
                    },
                    Transform::from_translation(world_from_ground(horse.position, height)),
                ));
                true
            }
            (Travel::OnFoot, Some(horse))
                if rider.position.distance(horse.position) <= Errand::MOUNT_RANGE =>
            {
                self.world.obstacles.remove(&horse.footprint);
                self.loose.0 = None;
                for &entity in &self.left {
                    self.commands.entity(entity).despawn();
                }
                self.ride(me, rider, transform, &horse);
                true
            }
            (Travel::OnFoot, _) => false,
        }
    }

    /// Gets on the nearest of `horses` within [`Errand::MOUNT_RANGE`] of
    /// the player on foot: its footprint goes, a horse tied in a town is
    /// led away for good, and a loose horse left before stays standing
    /// (no longer the loose one).
    fn take<'h>(
        &mut self,
        me: Entity,
        rider: &mut Rider,
        transform: &mut Transform,
        horses: impl Iterator<
            Item = (
                Entity,
                &'h Mountable,
                Option<&'h CharacterVisual>,
                Option<&'h Animal>,
                Option<&'h TetheredHorse>,
                bool,
            ),
        >,
    ) {
        if rider.travel() != Travel::OnFoot {
            return;
        }
        let at = rider.position;
        let nearest = horses
            .map(|(e, m, visual, animal, tied, left)| {
                (
                    e,
                    m.0,
                    standing_coat(visual, animal),
                    tied.map(|t| t.0),
                    left,
                )
            })
            .filter(|(_, h, ..)| at.distance(h.position) <= Errand::MOUNT_RANGE)
            .min_by(|a, b| {
                at.distance(a.1.position)
                    .total_cmp(&at.distance(b.1.position))
            });
        let Some((entity, horse, coat, town, _)) = nearest else {
            return;
        };
        self.world.obstacles.remove(&horse.footprint);
        if let Some(town) = town {
            self.led_away.0.insert(town);
        }
        // Whichever horse was left loose stays where it stands.
        self.loose.0 = None;
        for &e in &self.left {
            self.commands.entity(e).remove::<LeftHorse>();
        }
        self.commands.entity(entity).despawn();
        if let Some(coat) = coat {
            *self.coat = coat;
        }
        self.ride(me, rider, transform, &horse);
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

/// Puts [`Talking`] on whoever's lines are on screen and takes it off
/// everyone else: standing, they talk with their hands.
fn mark_speakers(
    mut commands: Commands,
    conversation: Res<Conversation>,
    players: Query<Entity, With<Player>>,
    npcs: Query<(Entity, &StandingNpc)>,
    talking: Query<Entity, With<Talking>>,
) {
    let speaker = conversation.0.as_ref().and_then(|talk| match talk.speaker {
        Speaker::Player => players.iter().next(),
        Speaker::Npc(npc) => npcs.iter().find(|(_, n)| n.npc == npc).map(|(e, _)| e),
    });
    for entity in &talking {
        if Some(entity) != speaker {
            commands.entity(entity).remove::<Talking>();
        }
    }
    if let Some(speaker) = speaker
        && !talking.contains(speaker)
    {
        commands.entity(speaker).insert(Talking);
    }
}

/// Keeps the HUD objective and the minimap target on the current stage.
fn show_progress(
    errand: Option<Res<ErrandRun>>,
    mut objective: ResMut<HudObjective>,
    mut target: ResMut<MapTarget>,
) {
    let text = errand.as_ref().and_then(|e| e.0.objective());
    objective.set_if_neq(HudObjective(text));
    target.set_if_neq(MapTarget(errand.and_then(|e| e.0.target())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::testing::arrive;
    use pampa_core::Vec2 as GroundVec2;
    use pampa_core::{ErrandStage, Terrain, Travel};
    use pampa_render::character::{FigureBody, FigureJoint, FigurePart, HorseBody, HorseModel};
    use pampa_render::{CharacterVisual, CharacterVisualPlugin, Coat, FigureKind, WorldTowns};

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

    /// The game's opening in the shipped world (aboard the morning train
    /// from Pico pulling into Monte Nievas), with every line and errand
    /// event logged. Frames last 50 ms.
    fn shipped_app() -> App {
        let mut app = crate::ride::testing::shipped_app();
        app.init_resource::<Said>()
            .init_resource::<Progress>()
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

    /// Horse bodies, figure bodies, figure joints, figure parts and horse
    /// models drawn under `who`, and meshes that are no figure part.
    fn drawn_under(app: &mut App, who: Entity) -> [usize; 6] {
        let mut under = Vec::new();
        let mut stack = vec![who];
        while let Some(e) = stack.pop() {
            if let Some(children) = app.world().get::<Children>(e) {
                stack.extend(children.iter());
                under.extend(children.iter());
            }
        }
        let world = app.world();
        let n = |has: &dyn Fn(Entity) -> bool| under.iter().filter(|e| has(**e)).count();
        [
            n(&|e| world.get::<HorseBody>(e).is_some()),
            n(&|e| world.get::<FigureBody>(e).is_some()),
            n(&|e| world.get::<FigureJoint>(e).is_some()),
            n(&|e| world.get::<FigurePart>(e).is_some()),
            n(&|e| world.get::<HorseModel>(e).is_some()),
            n(&|e| world.get::<Mesh3d>(e).is_some() && world.get::<FigurePart>(e).is_none()),
        ]
    }

    #[test]
    fn mounting_swaps_the_clerk_figure_for_one_horse_and_rider_and_nothing_else() {
        let mut app = shipped_app();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(CharacterVisualPlugin);
        app.world_mut()
            .spawn((Camera3d::default(), Transform::default()));
        arrive(&mut app);
        let e = errand(&app);
        let (me, _) = player(&mut app);
        assert_eq!(
            *app.world().get::<CharacterVisual>(me).unwrap(),
            CharacterVisual::Figure(FigureKind::Protagonist)
        );
        let [horses, bodies, joints, parts, _, other] = drawn_under(&mut app, me);
        assert_eq!((horses, bodies, joints, other), (0, 1, 14, 0));
        assert!(parts >= 18, "{parts}");

        teleport(
            &mut app,
            e.standing(Npc::Hidalgo).position + e.standing(Npc::Hidalgo).heading.forward() * 2.0,
        );
        press_f(&mut app);
        teleport(&mut app, beside(&e.horse().1));
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        // One horse with the clerk in its saddle, and nothing else: no
        // clerk on foot left, nothing rigid. (This app loads no models: the
        // horse and its rider wait for theirs, drawing nothing.)
        let [horses, bodies, joints, parts, models, other] = drawn_under(&mut app, me);
        assert_eq!(
            (horses, bodies, joints, parts, models, other),
            (1, 0, 0, 0, 1, 0)
        );
        let children: Vec<Entity> = app.world().get::<Children>(me).unwrap().to_vec();
        assert_eq!(children.len(), 1, "only the horse and its rider");
        let horse = app.world().get::<HorseBody>(children[0]).unwrap();
        assert_eq!(horse.coat, Coat::Bayo, "Don Hidalgo's bayo");
        assert_eq!(
            horse.rider,
            Some(FigureKind::Protagonist),
            "the clerk rides it"
        );
        let mut figures = app.world_mut().query::<&FigureBody>();
        assert!(
            figures
                .iter(app.world())
                .all(|b| b.kind != FigureKind::Protagonist),
            "no clerk on foot"
        );
        // The tied horse he took is gone from the store too.
        let mut tied = app.world_mut().query::<&TetheredHorse>();
        assert!(tied.iter(app.world()).all(|h| h.0 != e.horse().0));
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
    fn whoever_speaks_talks_with_their_hands_until_their_lines_end() {
        let mut app = shipped_app();
        let lines = shipped_lines();
        arrive(&mut app);
        let e = errand(&app);
        let talking = |app: &mut App| {
            let mut q = app.world_mut().query_filtered::<Entity, With<Talking>>();
            q.iter(app.world()).collect::<Vec<_>>()
        };
        assert_eq!(talking(&mut app), vec![]);
        teleport(
            &mut app,
            e.standing(Npc::Hidalgo).position + e.standing(Npc::Hidalgo).heading.forward() * 2.0,
        );
        press_f(&mut app);
        app.update();
        let don = npc_entity(&mut app, Npc::Hidalgo);
        assert_eq!(talking(&mut app), vec![don]);
        wait_for_lines(&mut app, lines.hidalgo.len());
        app.update();
        assert_eq!(talking(&mut app), vec![]);
    }

    #[test]
    fn the_whole_errand_from_the_train_to_the_comisaria() {
        let mut app = shipped_app();
        let lines = shipped_lines();
        // Off the morning train at Monte Nievas.
        arrive(&mut app);
        let e = errand(&app);
        let hidalgo = e.standing(Npc::Hidalgo).position;
        let (horse_town, horse) = e.horse();
        let farach = e.standing(Npc::Farach).position;

        // On foot on top of the Monte Nievas platform, beside the dwelling
        // train's coaches, facing town; standing still leaves the player
        // where they are.
        let (start, facing) = {
            let (_, p) = player(&mut app);
            (p.rider.position, p.rider.heading)
        };
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        assert_eq!(facing, town.platform_spot().unwrap().1);
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
        // Up on the platform's top, not on the ground beside it.
        let world = app.world().resource::<GameWorld>().0.clone();
        assert_eq!(p.rider.height, world.ground_height(start));
        assert!(p.rider.height > world.terrain.height(start.x, start.y) + 0.5);
        assert_eq!(objective(&app), e.objective());
        assert_eq!(
            objective(&app).as_deref(),
            Some("Buscá a Don Hidalgo en la plaza")
        );
        assert_eq!(app.world().resource::<MapTarget>().0, Some(hidalgo));

        // Nothing to do out on the road into town.
        teleport(&mut app, town.arrival().0);
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
        assert_eq!(objective(&app), errand(&app).objective());
        assert!(objective(&app).unwrap().contains("caballo"));
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
        assert_eq!(
            visual,
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            }
        );
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
        assert_eq!(objective(&app), errand(&app).objective());
        assert!(
            objective(&app)
                .unwrap()
                .ends_with("Farach en Eduardo Castex")
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

    /// The loose horse's logic and its drawn entities.
    fn loose(app: &mut App) -> (Option<pampa_core::TetheredHorse>, Vec<CharacterVisual>) {
        let horse = app.world().resource::<LooseHorse>().0;
        let mut q = app
            .world_mut()
            .query_filtered::<&CharacterVisual, With<LeftHorse>>();
        (horse, q.iter(app.world()).copied().collect())
    }

    #[test]
    fn getting_off_and_back_on_keeps_the_errand_on_track() {
        let mut app = shipped_app();
        arrive(&mut app);
        let e = errand(&app);
        let hidalgo = e.standing(Npc::Hidalgo);
        teleport(&mut app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(&mut app);
        teleport(&mut app, beside(&e.horse().1));
        press_f(&mut app);
        let (me, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        let report = objective(&app);
        assert!(
            report
                .as_deref()
                .is_some_and(|o| o.contains("Eduardo Castex"))
        );

        // Not while riding on.
        app.world_mut()
            .query::<&mut Player>()
            .single_mut(app.world_mut())
            .unwrap()
            .rider
            .speed = 5.0;
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(loose(&mut app), (None, vec![]));

        // Stopped: off, on foot beside the horse, which stays tied loosely.
        let farach = e.standing(Npc::Farach);
        let street = farach.position + farach.heading.forward() * 12.0;
        teleport(&mut app, street);
        app.update();
        let at = player(&mut app).1.rider;
        press_f(&mut app);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::OnFoot);
        assert_eq!(
            *app.world().get::<CharacterVisual>(me).unwrap(),
            CharacterVisual::Figure(FigureKind::Protagonist)
        );
        let (horse, drawn) = loose(&mut app);
        let horse = horse.expect("a loose horse");
        assert_eq!((horse.position, horse.heading), (at.position, at.heading));
        assert_eq!(drawn, vec![CharacterVisual::Horse(Coat::Bayo)]);
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        assert!(p.rider.position.distance(horse.position) <= Errand::MOUNT_RANGE);
        // The errand did not move: still to report, nothing to take.
        assert_eq!(errand(&app).stage(), ErrandStage::ReportToFarach);
        assert_eq!(objective(&app), report);

        // F beside it: back in the saddle, the loose horse gone.
        press_f(&mut app);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        assert_eq!(p.rider.position, horse.position);
        assert_eq!(loose(&mut app), (None, vec![]));
        assert!(
            !app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );

        // Off again, report on foot, and ride on after the errand.
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);
        teleport(&mut app, farach.position + farach.heading.forward() * 2.5);
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::Done);
        let (horse, _) = loose(&mut app);
        let horse = horse.unwrap();
        teleport(&mut app, horse.position + horse.heading.forward() * 2.0);
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(
            app.world().resource::<Progress>().0,
            [
                ErrandEvent::TalkedToHidalgo,
                ErrandEvent::MountedHorse,
                ErrandEvent::ReportedToFarach
            ]
        );
    }

    #[test]
    fn hidalgos_horse_unlocks_riding_and_then_any_horse_lets_him_on() {
        use pampa_core::{Abilities, Ability};
        let mut app = shipped_app();
        arrive(&mut app);
        let e = errand(&app);
        let ride = |app: &App| {
            app.world()
                .resource::<PlayerAbilities>()
                .0
                .has(Ability::Ride)
        };
        assert!(!ride(&app));
        // A horse tied in a town of neither Don Hidalgo nor Farach.
        let other = {
            let towns = &app.world().resource::<WorldTowns>().0;
            let police = e.standing(Npc::Farach).town;
            (0..towns.len())
                .find(|&t| t != e.horse().0 && t != police)
                .map(|t| (t, towns[t].hitching_post.horse))
                .expect("a third town")
        };
        teleport(&mut app, beside(&other.1));
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot, "not yet");

        // Don Hidalgo's horse: riding unlocks, with a notice.
        let hidalgo = e.standing(Npc::Hidalgo);
        teleport(&mut app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(&mut app);
        teleport(&mut app, beside(&e.horse().1));
        press_f(&mut app);
        assert!(ride(&app));
        assert_eq!(
            app.world()
                .resource::<pampa_render::HudAnnouncement>()
                .0
                .as_deref(),
            Some(Ability::Ride.notice())
        );
        assert_ne!(
            app.world().resource::<PlayerAbilities>().0,
            Abilities::all(),
            "only riding"
        );
        // Off where he took it: it stays tied loosely.
        press_f(&mut app);
        let (left, _) = loose(&mut app);
        let left = left.expect("left loose");

        // Now the other town's horse lets him on, in its own coat, and is
        // led away for good; Don Hidalgo's stays standing where he left it.
        teleport(&mut app, beside(&other.1));
        press_f(&mut app);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        assert_eq!(p.rider.position, other.1.position);
        assert!(app.world().resource::<LedAway>().0.contains(&other.0));
        let mut tied = app.world_mut().query::<&TetheredHorse>();
        assert!(tied.iter(app.world()).all(|h| h.0 != other.0));
        let obstacles = &app.world().resource::<GameWorld>().0.obstacles;
        assert!(!obstacles.contains(&other.1.footprint));
        assert!(obstacles.contains(&left.footprint));
        let coat = crate::life::tethered_coat(
            &app.world().resource::<WorldTowns>().0[other.0],
            app.world().resource::<Cast>(),
            other.0,
        );
        assert_eq!(app.world().resource::<RiddenCoat>().0, coat);
        let (me, _) = player(&mut app);
        assert_eq!(
            *app.world().get::<CharacterVisual>(me).unwrap(),
            player_visual_on(Travel::Riding, coat)
        );
        assert_eq!(loose(&mut app), (None, vec![]));
        let mut standing = app.world_mut().query::<&Mountable>();
        assert!(standing.iter(app.world()).any(|m| m.0 == left));

        // Off this one, back to Don Hidalgo's bayo (the former loose horse
        // is any other standing horse now).
        press_f(&mut app);
        teleport(&mut app, left.position + left.heading.forward() * 2.0);
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.position, left.position);
        assert_eq!(app.world().resource::<RiddenCoat>().0, Coat::HIDALGO);
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
        let lines = shipped_lines();
        let (run, cast) = plan(&loaded.towns, &mut world, &lines.police_town).unwrap();
        let hidalgo = run.0.standing(Npc::Hidalgo);
        let farach = run.0.standing(Npc::Farach);
        assert_eq!(loaded.towns[hidalgo.town].name, "Monte Nievas");
        assert_eq!(loaded.towns[farach.town].name, lines.police_town);
        assert_eq!(run.0.police_name(), "Eduardo Castex");
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
        // Hidalgo, Farach and Bairoletto at his side, clear of Castex.
        assert_eq!(world.obstacles.len(), before + 3);
        assert_eq!(cast.0, vec![(Npc::Hidalgo, hidalgo), (Npc::Farach, farach)]);
        let bairoletto = run.0.bairoletto();
        assert_eq!(cast.1, vec![(FigureKind::Bairoletto, bairoletto)]);
        for o in loaded.towns[farach.town].obstacles() {
            assert!(
                !o.intersects(&bairoletto.footprint()),
                "Bairoletto inside {o:?}"
            );
        }
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

    #[test]
    fn a_corral_horse_drawn_as_fauna_lets_the_player_on_in_its_coat() {
        use pampa_core::{Abilities, GameClock, Heading, TimeOfDay};
        use pampa_render::{Animal, AnimalPose};
        let mut app =
            crate::ride::testing::shipped_app_at(Some(GameClock::at(TimeOfDay::hm(12, 0))));
        app.update();
        // Out on the open pampa, clear of the platform.
        teleport(&mut app, GroundVec2::new(100.0, -200.0));
        app.update();
        let at = player(&mut app).1.rider.position + GroundVec2::new(0.0, 2.0);
        let horse = pampa_core::TetheredHorse::standing(at, Heading::default());
        let corral = app
            .world_mut()
            .spawn((
                Mountable(horse),
                Animal::Horse(Coat::Tordillo),
                AnimalPose::default(),
            ))
            .id();
        // Not his to ride yet.
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);
        app.world_mut().resource_mut::<PlayerAbilities>().0 = Abilities::all();
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(app.world().resource::<RiddenCoat>().0, Coat::Tordillo);
        assert!(
            app.world().get_entity(corral).is_err(),
            "taken from the corral"
        );
    }
}
