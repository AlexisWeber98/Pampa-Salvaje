//! The development cheat console. Only debug builds have it: `main`
//! declares this module under `cfg(debug_assertions)`, so a `--release`
//! build has none of it.
//!
//! Ctrl+Shift+F12 ([`is_toggle`]) opens it: a small panel in the HUD's
//! style at the bottom left, with a line to type into. Enter runs the line,
//! Esc (or the combination again) closes it. While it is open the game
//! sees no keys at all ([`read_keys`] clears them), so typing never drives
//! the rider, quits or orbits the camera.
//!
//! Commands ([`parse`]):
//! - `goto PLACE`: pico, castex, nievas, metileo, caleufu, arocena (any
//!   word of a town's or lagoon's name, accents optional), `lake` (the nearest lagoon), `ranch` (the
//!   nearest estancia) or
//!   `ranch castex` / `ranch pico` (the one nearest that town);
//! - `goto herd`: the pasture of the nearest herd;
//! - `goto X Y`: a point, in game meters;
//! - `horse`: a saddled horse beside the player;
//! - `turbo on` / `turbo off`: the player's horse gallops at up to
//!   [`TURBO_KMH`] (and gets there fast), or back to its normal gallop;
//! - `fauna`: how many animals of each kind live round the player;
//! - `unlock all`: every ability ([`PlayerAbilities`]);
//! - `models on` / `models off`: draw Hidalgo, Farach and Bairoletto as the MakeHuman models
//!   (the default) or back as the old rigid figures
//!   ([`CharacterModelsShown`]; only this console turns them off);
//! - `time HH` / `time HH:MM`: move the clock on to that time;
//! - `view fp` / `view tp`: look from the player's eyes or from the orbit
//!   ([`ViewMode`]), like `V` off the train;
//! - `help`: the list.
//!
//! `goto` puts the player (and the horse, when riding) on the nearest clear
//! ground to the place ([`World::clear_spot`]) and moves the streaming focus
//! there at once. Aboard the train it refuses: the ride owns the player
//! until they get off.

use bevy::input::ButtonState;
use bevy::input::InputSystems;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use pampa_core::{
    Circle, FaunaSite, Rider, RiderParams, SettlementKind, SettlementLayout, TetheredHorse,
    TimeOfDay, TownLayout, Travel, Vec2 as GroundVec2, World,
};
use pampa_render::hud::{LINE_PX, ink_font, panel_colors};
use pampa_render::{
    CharacterModelsShown, CharacterVisual, Coat, StreamFocus, Typefaces, VisualPose, WorldClock,
    WorldSettlements, WorldTowns, world_from_ground,
};

use crate::abilities::{PlayerAbilities, Unlocked};
use crate::errand::Mountable;
use crate::first_person::ViewMode;
use crate::player::{DrivePlayer, GameWorld, Player, PlayerTuning};
use crate::ride::OnTrain;

/// Most output lines kept on screen.
const LOG_LINES: usize = 6;
/// Top gallop of the player's horse with `turbo on`, in km/h.
pub const TURBO_KMH: f32 = 70.0;
/// How fast the horse gets there with `turbo on`, in m/s².
const TURBO_ACCELERATION: f32 = 10.0;
/// Farthest from a place the clear ground may be, in meters.
const SEARCH: f32 = 300.0;

/// What the console lists for `help`.
pub const HELP: [&str; 10] = [
    "goto lake, pico, castex, nievas, metileo, caleufu",
    "goto ranch, ranch castex, ranch pico, herd",
    "goto X Y: a point in meters",
    "horse: a saddled horse alongside",
    "fauna: the animals nearby",
    "unlock all: every ability",
    "models on, off: MakeHuman or old figures",
    "time HH or HH:MM: move the clock on",
    "turbo on, off: gallop at 70 kmh",
    "view fp, tp: first or third person",
];

/// Whether this frame's keys open (or close) the console: Ctrl+Shift+F12.
pub fn is_toggle(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
        && keys.just_pressed(KeyCode::F12)
}

/// A console command.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Goto(Destination),
    Horse,
    /// Counts the animals round the player.
    Fauna,
    UnlockAll,
    /// Draw the errand's people as the MakeHuman models (or not).
    Models(bool),
    /// Moves the clock on to this time of day (today or tomorrow).
    Time(TimeOfDay),
    /// Lets the player's horse gallop at [`TURBO_KMH`] (or back to normal).
    Turbo(bool),
    /// Looks from the player's eyes (`true`) or from the orbit.
    View(bool),
    Help,
}

/// Where `goto` goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    /// A town or a lagoon, by a word of its name.
    Named(String),
    /// The lagoon nearest the player.
    Lake,
    /// The pasture of the herd nearest the player.
    Herd,
    /// The estancia nearest the player, or the town named.
    Ranch(Option<String>),
    /// A point, in game meters.
    Point(GroundVec2),
}

/// Reads a typed line; `Err` is the line to show instead.
pub fn parse(line: &str) -> Result<Command, String> {
    let lower = line.trim().to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    match words.as_slice() {
        ["help"] => Ok(Command::Help),
        ["horse"] => Ok(Command::Horse),
        ["fauna"] => Ok(Command::Fauna),
        ["unlock", "all"] => Ok(Command::UnlockAll),
        ["models", "on"] => Ok(Command::Models(true)),
        ["models", "off"] => Ok(Command::Models(false)),
        ["models"] => Err("usage: models on, models off".into()),
        ["turbo", "on"] => Ok(Command::Turbo(true)),
        ["turbo", "off"] => Ok(Command::Turbo(false)),
        ["turbo"] => Err("usage: turbo on, turbo off".into()),
        ["view", "fp"] => Ok(Command::View(true)),
        ["view", "tp"] => Ok(Command::View(false)),
        ["view", ..] => Err("usage: view fp, view tp".into()),
        ["time"] => Err("usage: time HH, or time HH:MM".into()),
        ["time", time] => time_of_day(time)
            .map(Command::Time)
            .ok_or_else(|| format!("not a time: {time}")),
        ["goto"] => Err("usage: goto PLACE, or goto X Y".into()),
        ["goto", "lake"] => Ok(Command::Goto(Destination::Lake)),
        ["goto", "herd"] => Ok(Command::Goto(Destination::Herd)),
        ["goto", "ranch"] => Ok(Command::Goto(Destination::Ranch(None))),
        ["goto", "ranch", town] => Ok(Command::Goto(Destination::Ranch(Some(town.to_string())))),
        ["goto", place] => Ok(Command::Goto(Destination::Named(place.to_string()))),
        ["goto", x, y] => match (x.parse::<f32>(), y.parse::<f32>()) {
            (Ok(x), Ok(y)) if x.is_finite() && y.is_finite() => {
                Ok(Command::Goto(Destination::Point(GroundVec2::new(x, y))))
            }
            _ => Err(format!("not a point: {x} {y}")),
        },
        [] => Err(String::new()),
        _ => Err(format!("unknown command: {}. Try help", line.trim())),
    }
}

/// `HH` or `HH:MM` on a 24-hour clock.
fn time_of_day(word: &str) -> Option<TimeOfDay> {
    let (h, m) = word.split_once(':').unwrap_or((word, "0"));
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then(|| TimeOfDay::hm(h, m))
}

/// Whether a word of `name` is `word`, ignoring case and accents
/// (`caleufu` names Caleufú).
fn named(name: &str, word: &str) -> bool {
    let word = plain(word);
    plain(name).split_whitespace().any(|w| w == word)
}

/// `text` in lower case with its Spanish accents and tilde dropped.
fn plain(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// The town with a word of its name `word`.
fn town<'a>(towns: &'a [TownLayout], word: &str) -> Option<&'a TownLayout> {
    towns.iter().find(|t| named(&t.name, word))
}

/// The point `goto` aims at, before looking for clear ground: a town's
/// arrival on its main road, a lagoon's landing at the end of its road
/// (or its dry bank nearest `from`), an estancia's patio.
fn aim(
    destination: &Destination,
    towns: &[TownLayout],
    world: &World,
    settlements: Option<&SettlementLayout>,
    from: GroundVec2,
) -> Result<GroundVec2, String> {
    let ranches = || {
        settlements
            .into_iter()
            .flat_map(|s| &s.settlements)
            .filter(|s| s.kind == SettlementKind::Ranch)
    };
    let nearest_ranch = |to: &dyn Fn(GroundVec2) -> f32| {
        ranches()
            .min_by(|a, b| to(a.hub).total_cmp(&to(b.hub)))
            .map(|s| s.hub)
            .ok_or_else(|| "no ranches".to_string())
    };
    match destination {
        Destination::Point(p) => Ok(*p),
        // The herds live in the fauna, not here: `run_entered` turns this
        // into the pasture's point first.
        Destination::Herd => Err("no herds".to_string()),
        Destination::Ranch(None) => nearest_ranch(&|p| p.distance(from)),
        Destination::Ranch(Some(word)) => {
            let band = town(towns, word)
                .ok_or_else(|| format!("no town {word}"))?
                .leveled();
            nearest_ranch(&|p| band.closest_point(p).distance(p))
        }
        Destination::Lake | Destination::Named(_) => {
            let lagoons = &world.terrain.lagoons;
            let lagoon = match destination {
                Destination::Named(word) => {
                    if let Some(t) = town(towns, word) {
                        return Ok(t.arrival().0);
                    }
                    lagoons
                        .iter()
                        .find(|l| named(&l.name, word))
                        .ok_or_else(|| format!("unknown place: {word}"))?
                }
                _ => lagoons
                    .iter()
                    .min_by(|a, b| a.center.distance(from).total_cmp(&b.center.distance(from)))
                    .ok_or_else(|| "no lakes".to_string())?,
            };
            let by_shore = |a: &&GroundVec2, b: &&GroundVec2| {
                lagoon.offshore(**a).total_cmp(&lagoon.offshore(**b))
            };
            let landing = world.terrain.roads.as_ref().and_then(|roads| {
                roads
                    .roads
                    .iter()
                    .filter(|r| r.kind == pampa_core::RoadKind::Lagoon)
                    .flat_map(|r| &r.path)
                    .min_by(by_shore)
                    .copied()
            });
            Ok(landing.unwrap_or_else(|| {
                let bank = lagoon.ring(64, pampa_core::Lagoon::MUD_BAND + 5.0);
                bank.into_iter()
                    .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
                    .unwrap_or(lagoon.center)
            }))
        }
    }
}

/// The player's horse tuning with `turbo` on or off.
fn turbo(on: bool) -> RiderParams {
    let normal = RiderParams::default();
    if !on {
        return normal;
    }
    RiderParams {
        gallop_max: TURBO_KMH / 3.6,
        acceleration: TURBO_ACCELERATION,
        ..normal
    }
}

/// The middle of the pasture of the herd nearest `from`.
fn nearest_herd(site: &FaunaSite, from: GroundVec2) -> Option<GroundVec2> {
    site.herds()
        .iter()
        .map(|h| h.pasture.center())
        .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
}

/// Where `goto destination` puts the player: the nearest clear, dry ground
/// inside the world to the place, roomy enough for a horse and rider.
pub fn resolve(
    destination: &Destination,
    towns: &[TownLayout],
    world: &World,
    settlements: Option<&SettlementLayout>,
    from: GroundVec2,
) -> Result<GroundVec2, String> {
    let target = aim(destination, towns, world, settlements, from)?;
    let collider = Circle::new(Rider::COLLIDER_RADIUS);
    let inside = world.bounds.contain(target, collider.radius).position;
    world
        .clear_spot(collider, inside, SEARCH)
        .ok_or_else(|| "no clear ground there".to_string())
}

/// The console: open or not, the line being typed, what it printed.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Console {
    pub open: bool,
    pub input: String,
    pub log: Vec<String>,
    /// Lines entered and not yet run.
    pub entered: Vec<String>,
}

impl Console {
    fn print(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        let extra = self.log.len().saturating_sub(LOG_LINES);
        self.log.drain(..extra);
    }
}

/// The console's panel and its lines.
#[derive(Component, Debug)]
pub struct ConsolePanel;

pub struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Console>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<StreamFocus>()
            .init_resource::<Typefaces>()
            .init_resource::<PlayerAbilities>()
            .add_message::<KeyboardInput>()
            .add_message::<Unlocked>()
            .add_systems(Startup, spawn_panel)
            .add_systems(
                PreUpdate,
                read_keys.after(InputSystems).before(crate::menu::MenuKeys),
            )
            .add_systems(Update, run_entered.after(DrivePlayer))
            .add_systems(PostUpdate, draw_panel);
    }
}

/// The modifiers of the toggle ([`is_toggle`]) held down while the console
/// is open. The console clears [`ButtonInput`] every frame, which forgets
/// held keys until they are pressed again, so it follows them from the
/// keyboard's own presses and releases instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Held {
    ctrl: [bool; 2],
    shift: [bool; 2],
}

impl Held {
    /// What `keys` holds down.
    fn from_keys(keys: &ButtonInput<KeyCode>) -> Self {
        Self {
            ctrl: [KeyCode::ControlLeft, KeyCode::ControlRight].map(|k| keys.pressed(k)),
            shift: [KeyCode::ShiftLeft, KeyCode::ShiftRight].map(|k| keys.pressed(k)),
        }
    }

    /// Follows a press or release of `key`; `false` if it is no modifier.
    fn follow(&mut self, key: KeyCode, down: bool) -> bool {
        let slot = match key {
            KeyCode::ControlLeft => &mut self.ctrl[0],
            KeyCode::ControlRight => &mut self.ctrl[1],
            KeyCode::ShiftLeft => &mut self.shift[0],
            KeyCode::ShiftRight => &mut self.shift[1],
            _ => return false,
        };
        *slot = down;
        true
    }

    fn toggles(&self) -> bool {
        self.ctrl.contains(&true) && self.shift.contains(&true)
    }
}

/// Opens and closes the console and, while open, types into it and keeps
/// every key from the game.
fn read_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    mut console: ResMut<Console>,
    mut held: Local<Held>,
) {
    if !console.open {
        typed.clear();
        if is_toggle(&keys) {
            console.open = true;
            *held = Held::from_keys(&keys);
            keys.reset_all();
        }
        return;
    }
    for key in typed.read() {
        let down = key.state == ButtonState::Pressed;
        if held.follow(key.key_code, down) || !down {
            continue;
        }
        match key.key_code {
            KeyCode::F12 if held.toggles() => console.open = false,
            KeyCode::Escape => console.open = false,
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let line = std::mem::take(&mut console.input);
                console.entered.push(line);
            }
            KeyCode::Backspace => {
                console.input.pop();
            }
            _ => {
                let text = key.text.as_deref().unwrap_or_default();
                console
                    .input
                    .extend(text.chars().filter(|c| !c.is_control()));
            }
        }
    }
    keys.reset_all();
}

/// What a command reads of the world.
type Places<'w> = (
    Option<Res<'w, WorldTowns>>,
    Option<Res<'w, WorldSettlements>>,
    ResMut<'w, GameWorld>,
    ResMut<'w, StreamFocus>,
);

/// What a command reads of the fauna, and the view it may switch.
type Watched<'w> = (
    Option<Res<'w, crate::fauna::Fauna>>,
    Option<Res<'w, crate::fauna::WorldFauna>>,
    Option<ResMut<'w, ViewMode>>,
);

/// Runs the lines entered, printing what each did.
fn run_entered(
    mut commands: Commands,
    mut console: ResMut<Console>,
    (towns, settlements, mut world, mut focus): Places,
    (mut abilities, mut unlocked, mut clock): (
        ResMut<PlayerAbilities>,
        MessageWriter<Unlocked>,
        Option<ResMut<WorldClock>>,
    ),
    mut players: Query<(&mut Player, &mut Transform, Has<OnTrain>)>,
    (mut horses, mut models, mut tuning): (
        Local<u32>,
        Option<ResMut<CharacterModelsShown>>,
        Option<ResMut<PlayerTuning>>,
    ),
    (fauna, site, mut view): Watched,
) {
    // Only touched with lines to run: a change every frame would reletter
    // the panel every frame, despawning its lines before the UI lays them
    // out, so it would show empty.
    if console.entered.is_empty() {
        return;
    }
    for line in std::mem::take(&mut console.entered) {
        console.print(format!(": {line}"));
        let command = match parse(&line) {
            Ok(command) => command,
            Err(error) => {
                if !error.is_empty() {
                    console.print(error);
                }
                continue;
            }
        };
        let Ok((mut player, mut transform, aboard)) = players.single_mut() else {
            console.print("no player");
            continue;
        };
        let rider = &mut player.rider;
        match command {
            Command::Help => HELP.iter().for_each(|h| console.print(*h)),
            Command::Fauna => match fauna.as_deref().map(crate::fauna::Fauna::counts) {
                Some([cows, horses, hares, teros, raptors, ducks]) => console.print(format!(
                    "cows {cows}, horses {horses}, hares {hares}, teros {teros}, \
                     raptors {raptors}, ducks {ducks}"
                )),
                None => console.print("no fauna"),
            },
            Command::Models(on) => match models.as_deref_mut() {
                Some(shown) => {
                    shown.0 = on;
                    console.print(if on {
                        "models on: MakeHuman people"
                    } else {
                        "models off: rigid 1919 figures"
                    });
                }
                None => console.print("no people to draw"),
            },
            Command::Turbo(on) => match tuning.as_deref_mut() {
                Some(tuning) => {
                    tuning.0 = turbo(on);
                    console.print(if on {
                        format!("turbo on: gallop up to {TURBO_KMH:.0} kmh")
                    } else {
                        "turbo off: normal gallop".to_string()
                    });
                }
                None => console.print("no horse tuning"),
            },
            Command::View(first) => match view.as_deref_mut() {
                Some(view) => {
                    *view = if first {
                        ViewMode::FirstPerson
                    } else {
                        ViewMode::ThirdPerson
                    };
                    console.print(match (first, aboard) {
                        (true, false) => "view fp: from the eyes",
                        (true, true) => "view fp: from the eyes once off the train",
                        (false, _) => "view tp: the orbit",
                    });
                }
                None => console.print("no camera"),
            },
            Command::UnlockAll => {
                for ability in pampa_core::Ability::ALL {
                    abilities.unlock(ability, &mut unlocked);
                }
                console.print("everything unlocked");
            }
            Command::Time(_) if aboard => console.print("not on the train: get off first"),
            Command::Time(time) => match clock.as_mut() {
                Some(clock) => {
                    clock.0.advance_until(time);
                    console.print(format!("it is {}", clock.0.hh_mm()));
                }
                None => console.print("no clock"),
            },
            Command::Goto(_) if aboard => console.print("not on the train: get off first"),
            Command::Goto(destination) => {
                let destination = match destination {
                    Destination::Herd => match site
                        .as_deref()
                        .and_then(|s| nearest_herd(&s.0, rider.position))
                    {
                        Some(pasture) => Destination::Point(pasture),
                        None => {
                            console.print("no herds");
                            continue;
                        }
                    },
                    other => other,
                };
                let towns = towns.as_ref().map_or(&[][..], |t| &t.0[..]);
                let layout = settlements.as_ref().map(|s| &s.0);
                match resolve(&destination, towns, &world.0, layout, rider.position) {
                    Ok(to) => {
                        teleport(rider, &mut transform, to, &world.0);
                        focus.0 = to;
                        console.print(format!("at {:.0} {:.0}", to.x, to.y));
                    }
                    Err(error) => console.print(error),
                }
            }
            Command::Horse if aboard => console.print("not on the train: get off first"),
            Command::Horse => match horse_beside(rider, &world.0) {
                Some(horse) => {
                    *horses += 1;
                    world.0.obstacles.push(horse.footprint);
                    let height = world.0.ground_height(horse.position);
                    commands.spawn((
                        Name::new(format!("Console horse {}", *horses)),
                        Mountable(horse),
                        CharacterVisual::Horse(Coat::seeded(*horses)),
                        VisualPose {
                            heading: horse.heading,
                            ..default()
                        },
                        Transform::from_translation(world_from_ground(horse.position, height)),
                    ));
                    console.print("a horse alongside");
                }
                None => console.print("no room for a horse"),
            },
        }
    }
}

/// Puts `rider` at `to`, at rest and facing the same way, on foot or on
/// horseback as they were.
pub fn teleport(rider: &mut Rider, transform: &mut Transform, to: GroundVec2, world: &World) {
    let heading = rider.heading;
    *rider = match rider.travel() {
        Travel::OnFoot => Rider::on_foot(to, heading, world),
        Travel::Riding => Rider::new(to, heading, world),
    };
    transform.translation = world_from_ground(rider.position, rider.height);
}

/// A horse standing a few meters off the rider's left (or the nearest clear
/// ground to that), facing their way, clear of them too.
pub fn horse_beside(rider: &Rider, world: &World) -> Option<TetheredHorse> {
    let ahead = rider.heading.forward();
    let left = GroundVec2::new(-ahead.y, ahead.x);
    // A circle round the horse's footprint, turned any way.
    let body = Circle::new(TetheredHorse::LENGTH / 2.0 + 0.2);
    let clear = body.radius + rider.collider.radius;
    let spot = world.clear_spot(body, rider.position + left * (clear + 0.5), 30.0)?;
    (spot.distance(rider.position) >= clear).then(|| TetheredHorse::standing(spot, rider.heading))
}

/// The console panel, hidden, at the bottom left.
fn spawn_panel(mut commands: Commands) {
    let (border, background) = panel_colors();
    commands.spawn((
        Name::new("Console"),
        ConsolePanel,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            bottom: Val::Px(12.0),
            flex_direction: FlexDirection::Column,
            border: UiRect::all(Val::Px(3.0)),
            padding: UiRect::all(Val::Px(6.0)),
            row_gap: Val::Px(4.0),
            ..default()
        },
        border,
        background,
    ));
}

/// Shows the panel while the console is open and rewrites its text when it
/// changed: the output, then the line being typed.
fn draw_panel(
    mut commands: Commands,
    console: Res<Console>,
    faces: Res<Typefaces>,
    mut panels: Query<(Entity, &mut Node), With<ConsolePanel>>,
) {
    if !console.is_changed() {
        return;
    }
    for (panel, mut node) in &mut panels {
        node.display = if console.open {
            Display::Flex
        } else {
            Display::None
        };
        commands.entity(panel).despawn_related::<Children>();
        let typing = format!(": {}-", console.input);
        for line in console.log.iter().chain([&typing]) {
            commands.entity(panel).with_child((
                Text::new(line.clone()),
                ink_font(&faces.body, LINE_PX),
                Node {
                    min_height: Val::Px(LINE_PX),
                    ..default()
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::testing::shipped_app_at;
    use pampa_content::{TOWNS_FILE, asset_path};
    use pampa_core::{Ability, GameClock, TimeOfDay};

    #[test]
    fn parses_every_command() {
        assert_eq!(parse("help"), Ok(Command::Help));
        assert_eq!(parse("models on"), Ok(Command::Models(true)));
        assert_eq!(parse("Models  ON"), Ok(Command::Models(true)));
        assert_eq!(parse("models off"), Ok(Command::Models(false)));
        assert!(parse("models").unwrap_err().contains("usage"));
        assert_eq!(parse("  Horse "), Ok(Command::Horse));
        assert_eq!(parse("fauna"), Ok(Command::Fauna));
        assert_eq!(parse("unlock all"), Ok(Command::UnlockAll));
        assert_eq!(parse("time 21"), Ok(Command::Time(TimeOfDay::hm(21, 0))));
        assert_eq!(parse("Time 5:30"), Ok(Command::Time(TimeOfDay::hm(5, 30))));
        assert!(parse("time").is_err());
        assert!(parse("time 24").unwrap_err().contains("not a time"));
        assert!(parse("time 21:60").is_err());
        assert!(parse("time night").is_err());
        assert_eq!(parse("goto Lake"), Ok(Command::Goto(Destination::Lake)));
        assert_eq!(parse("goto herd"), Ok(Command::Goto(Destination::Herd)));
        assert_eq!(parse("Turbo ON"), Ok(Command::Turbo(true)));
        assert_eq!(parse("turbo off"), Ok(Command::Turbo(false)));
        assert!(parse("turbo").unwrap_err().contains("usage"));
        assert_eq!(parse("View FP"), Ok(Command::View(true)));
        assert_eq!(parse("view tp"), Ok(Command::View(false)));
        assert!(parse("view").unwrap_err().contains("usage"));
        assert!(parse("view side").unwrap_err().contains("usage"));
        assert!((turbo(true).gallop_max * 3.6 - TURBO_KMH).abs() < 1e-3);
        assert_eq!(turbo(false), RiderParams::default());
        for place in ["arocena", "pico", "castex", "nievas", "metileo", "caleufu"] {
            assert_eq!(
                parse(&format!("goto {place}")),
                Ok(Command::Goto(Destination::Named(place.into())))
            );
        }
        assert_eq!(
            parse("goto ranch"),
            Ok(Command::Goto(Destination::Ranch(None)))
        );
        assert_eq!(
            parse("goto Ranch Castex"),
            Ok(Command::Goto(Destination::Ranch(Some("castex".into()))))
        );
        assert_eq!(
            parse("goto -120.5 300"),
            Ok(Command::Goto(Destination::Point(GroundVec2::new(
                -120.5, 300.0
            ))))
        );
    }

    #[test]
    fn bad_lines_say_what_is_wrong() {
        assert!(parse("goto").unwrap_err().contains("usage"));
        assert!(parse("goto 1 north").unwrap_err().contains("not a point"));
        assert!(parse("goto 1 inf").is_err());
        assert!(parse("fly").unwrap_err().contains("unknown"));
        assert!(parse("unlock").unwrap_err().contains("unknown"));
        assert!(parse("goto a b c").unwrap_err().contains("unknown"));
        assert_eq!(parse("   "), Err(String::new()));
    }

    #[test]
    fn everything_the_console_prints_is_in_the_typeface() {
        let lines = HELP.iter().map(|s| s.to_string()).chain([
            parse("fly").unwrap_err(),
            parse("goto").unwrap_err(),
            parse("goto 1 x").unwrap_err(),
            ": goto 12 -3.5-".into(),
            "not on the train: get off first".into(),
            "everything unlocked".into(),
            "it is 21:00".into(),
            "no clock".into(),
            parse("time").unwrap_err(),
            parse("time x").unwrap_err(),
            parse("turbo").unwrap_err(),
            "turbo on: gallop up to 70 kmh".into(),
            "turbo off: normal gallop".into(),
            "no horse tuning".into(),
            "no room for a horse".into(),
            "no clear ground there".into(),
            parse("models").unwrap_err(),
            "models on: MakeHuman people".into(),
            "models off: rigid 1919 figures".into(),
            "no people to draw".into(),
            parse("view").unwrap_err(),
            "view fp: from the eyes".into(),
            "view fp: from the eyes once off the train".into(),
            "view tp: the orbit".into(),
            "no camera".into(),
        ]);
        for line in lines {
            let missing: Vec<char> = line
                .chars()
                .filter(|&c| {
                    !pampa_render::lettering::supports(pampa_render::lettering::Face::Body, c)
                })
                .collect();
            assert!(missing.is_empty(), "{line:?}: {missing:?}");
        }
    }

    #[test]
    fn goto_finds_clear_dry_ground_at_every_named_place() {
        let loaded = crate::towns::load_world(&asset_path(TOWNS_FILE));
        let laid = crate::lay_out(&loaded);
        let world = &laid.world;
        let collider = Circle::new(Rider::COLLIDER_RADIUS);
        let lagoon = &world.terrain.lagoons[0];
        let town = |name: &str| loaded.towns.iter().find(|t| t.name == name).unwrap();
        let near_town = |name: &str| {
            let band = town(name).leveled();
            move |p: GroundVec2| band.closest_point(p).distance(p)
        };
        let from = loaded.towns[0].arrival().0;
        // Each line, how far its spot lies from where it names, and the most.
        type Distance<'a> = Box<dyn Fn(GroundVec2) -> f32 + 'a>;
        let cases: [(&str, Distance, f32); 11] = [
            ("goto lake", Box::new(|p| lagoon.offshore(p)), 40.0),
            ("goto arocena", Box::new(|p| lagoon.offshore(p)), 40.0),
            ("goto pico", Box::new(near_town("General Pico")), 400.0),
            ("goto castex", Box::new(near_town("Eduardo Castex")), 400.0),
            ("goto nievas", Box::new(near_town("Monte Nievas")), 400.0),
            ("goto metileo", Box::new(near_town("Metileo")), 400.0),
            // Accents are optional: "caleufu" names Caleufú.
            ("goto caleufu", Box::new(near_town("Caleufú")), 400.0),
            ("goto caleufú", Box::new(near_town("Caleufú")), 400.0),
            ("goto ranch", Box::new(|p| p.distance(from)), 8000.0),
            (
                "goto ranch castex",
                Box::new(near_town("Eduardo Castex")),
                3500.0,
            ),
            (
                "goto ranch pico",
                Box::new(near_town("General Pico")),
                3500.0,
            ),
        ];
        for (line, distance, within) in cases {
            let Ok(Command::Goto(destination)) = parse(line) else {
                panic!("{line}");
            };
            let to = resolve(
                &destination,
                &loaded.towns,
                world,
                Some(&laid.settlements),
                from,
            )
            .unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(
                world.bounds.contain(to, collider.radius).position,
                to,
                "{line}"
            );
            assert_eq!(world.resolve(collider, to, |_| ()), to, "{line}: {to:?}");
            assert!(!lagoon.is_bare(to), "{line}: {to:?}");
            assert!(distance(to) <= within, "{line}: {to:?} {}", distance(to));
        }
        // An estancia's patio, by the nearest ranch.
        let to = resolve(
            &Destination::Ranch(Some("castex".into())),
            &loaded.towns,
            world,
            Some(&laid.settlements),
            from,
        )
        .unwrap();
        assert!(
            laid.settlements
                .settlements
                .iter()
                .any(|s| s.kind == SettlementKind::Ranch && s.hub.distance(to) < 60.0)
        );
        // Points: off the map, pulled inside; unknown names say so.
        let edge = world.bounds.max + GroundVec2::new(500.0, 500.0);
        let to = resolve(&Destination::Point(edge), &[], world, None, from).unwrap();
        assert_eq!(world.bounds.contain(to, collider.radius).position, to);
        let unknown = Destination::Named("rosario".into());
        assert!(resolve(&unknown, &loaded.towns, world, None, from).is_err());
        let no_ranch = Destination::Ranch(None);
        assert!(resolve(&no_ranch, &loaded.towns, world, None, from).is_err());
    }

    /// The shipped towns at midday, the player on foot on the Monte Nievas
    /// platform, with the console.
    fn app() -> App {
        let mut app = shipped_app_at(Some(GameClock::at(TimeOfDay::hm(12, 0))));
        app.add_plugins(ConsolePlugin);
        app.update();
        app
    }

    fn enter(app: &mut App, line: &str) {
        app.world_mut()
            .resource_mut::<Console>()
            .entered
            .push(line.into());
        app.update();
    }

    fn rider(app: &mut App) -> Rider {
        let mut q = app.world_mut().query::<&Player>();
        q.single(app.world()).unwrap().rider
    }

    fn last_line(app: &App) -> String {
        app.world()
            .resource::<Console>()
            .log
            .last()
            .unwrap()
            .clone()
    }

    #[test]
    fn view_switches_between_the_eyes_and_the_orbit() {
        let mut app = app();
        enter(&mut app, "view fp");
        assert_eq!(last_line(&app), "no camera");
        app.init_resource::<ViewMode>();
        enter(&mut app, "view fp");
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::FirstPerson);
        assert_eq!(last_line(&app), "view fp: from the eyes");
        enter(&mut app, "view tp");
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
        assert_eq!(last_line(&app), "view tp: the orbit");
    }

    #[test]
    fn models_turns_the_makehuman_people_on_and_off() {
        let mut app = app();
        enter(&mut app, "models on");
        assert_eq!(
            last_line(&app),
            "no people to draw",
            "nothing to toggle yet"
        );
        app.init_resource::<CharacterModelsShown>();
        assert!(
            app.world().resource::<CharacterModelsShown>().0,
            "on by default"
        );
        enter(&mut app, "models off");
        assert!(!app.world().resource::<CharacterModelsShown>().0);
        assert!(last_line(&app).contains("1919"));
        enter(&mut app, "models on");
        assert!(app.world().resource::<CharacterModelsShown>().0);
        assert!(last_line(&app).contains("MakeHuman"));
    }

    #[test]
    fn goto_moves_the_rider_and_the_streaming_with_them() {
        let mut app = app();
        let pico = {
            let towns = &app.world().resource::<WorldTowns>().0;
            towns
                .iter()
                .find(|t| t.name == "General Pico")
                .unwrap()
                .arrival()
                .0
        };
        enter(&mut app, "goto pico");
        let at = rider(&mut app);
        assert!(at.position.distance(pico) < 30.0, "{:?}", at.position);
        assert_eq!(at.travel(), Travel::OnFoot);
        // The world streams around the new spot, the nearest chunk first.
        let focus = app.world().resource::<StreamFocus>().0;
        assert_eq!(focus, at.position);
        let config = app.world().resource::<GameWorld>().0.config;
        let mut chunks = pampa_render::StreamingSet::new(3, 4);
        let plan = chunks.update(&config, focus);
        assert_eq!(plan.spawn[0], config.chunk_of(at.position));
        // On horseback the horse comes along.
        {
            let world = app.world().resource::<GameWorld>().0.clone();
            let mut q = app.world_mut().query::<&mut Player>();
            let mut p = q.single_mut(app.world_mut()).unwrap();
            p.rider = Rider::new(p.rider.position, p.rider.heading, &world);
        }
        enter(&mut app, "goto 100 -200");
        let at = rider(&mut app);
        assert_eq!(at.travel(), Travel::Riding);
        assert!(at.position.distance(GroundVec2::new(100.0, -200.0)) < 20.0);
        assert_eq!(app.world().resource::<StreamFocus>().0, at.position);
        let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
        let transform = *q.single(app.world()).unwrap();
        assert_eq!(
            transform.translation,
            world_from_ground(at.position, at.height)
        );
        // Unknown places stay put and say so.
        enter(&mut app, "goto rosario");
        assert_eq!(rider(&mut app).position, at.position);
        assert!(last_line(&app).contains("rosario"));
    }

    #[test]
    fn time_moves_the_clock_on_to_nightfall() {
        let mut app = app();
        let before = app.world().resource::<WorldClock>().0.day();
        enter(&mut app, "time 21:30");
        let clock = app.world().resource::<WorldClock>().0;
        assert_eq!(clock.time_of_day().hour_minute(), (21, 30));
        assert_eq!(clock.day(), before, "later the same day");
        assert_eq!(last_line(&app), "it is 21:30");
        // An earlier hour is tomorrow's: the clock never runs back.
        enter(&mut app, "time 6");
        let clock = app.world().resource::<WorldClock>().0;
        assert_eq!((clock.day(), clock.hour() as u32), (before + 1, 6));
    }

    #[test]
    fn goto_refuses_aboard_the_train() {
        let mut app = crate::ride::testing::shipped_app();
        app.add_plugins(ConsolePlugin);
        app.update();
        let before = rider(&mut app).position;
        enter(&mut app, "goto castex");
        assert!(last_line(&app).contains("train"));
        let mut q = app.world_mut().query_filtered::<(), With<OnTrain>>();
        assert_eq!(q.iter(app.world()).count(), 1, "still aboard");
        assert!(rider(&mut app).position.distance(before) < 20.0);
    }

    #[test]
    fn horse_stands_a_horse_beside_the_player_mountable_once_riding_is_unlocked() {
        let mut app = app();
        // Out on the open pampa.
        enter(&mut app, "goto 100 -200");
        enter(&mut app, "horse");
        let me = rider(&mut app);
        let mut q = app.world_mut().query::<(&Mountable, &CharacterVisual)>();
        let horses: Vec<_> = q.iter(app.world()).map(|(m, v)| (m.0, *v)).collect();
        let console: Vec<_> = horses
            .iter()
            .filter(|(h, _)| h.position.distance(me.position) < 6.0)
            .collect();
        assert_eq!(console.len(), 1, "{horses:?}");
        let (horse, visual) = *console[0];
        assert!(matches!(visual, CharacterVisual::Horse(_)));
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        // Not his to ride yet: F does nothing.
        crate::ride::testing::press(&mut app, crate::errand::INTERACT_KEY);
        assert_eq!(rider(&mut app).travel(), Travel::OnFoot);
        // Every ability unlocked: now it lets him on.
        enter(&mut app, "unlock all");
        let abilities = app.world().resource::<PlayerAbilities>().0;
        assert!(Ability::ALL.iter().all(|&a| abilities.has(a)));
        crate::ride::testing::press(&mut app, crate::errand::INTERACT_KEY);
        let me = rider(&mut app);
        assert_eq!(me.travel(), Travel::Riding);
        assert_eq!(me.position, horse.position);
    }

    /// One key pressed and released, as the window sends it.
    fn key(app: &mut App, code: KeyCode, text: Option<&str>) {
        send(app, code, text, ButtonState::Pressed);
        send(app, code, text, ButtonState::Released);
    }

    /// One key going down or up, as the window sends it.
    fn send(app: &mut App, code: KeyCode, text: Option<&str>, state: ButtonState) {
        use bevy::input::keyboard::Key;
        let logical_key = text.map_or(
            Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            |t| Key::Character(t.into()),
        );
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key,
            state,
            text: text
                .filter(|_| state == ButtonState::Pressed)
                .map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
    }

    /// The lines the panel shows, top to bottom, as their text, and whether
    /// the panel is shown. Every line is ink text in the body typeface.
    fn panel_lines(app: &mut App) -> (bool, Vec<String>) {
        let mut panels = app
            .world_mut()
            .query_filtered::<(&Node, Option<&Children>), With<ConsolePanel>>();
        let (node, children) = panels.single(app.world()).unwrap();
        let shown = node.display != Display::None;
        let children: Vec<Entity> = children.into_iter().flatten().copied().collect();
        let (font, color) = ink_font(&Handle::default(), LINE_PX);
        let lines = children
            .iter()
            .map(|&c| {
                assert_eq!(app.world().get::<TextFont>(c), Some(&font));
                assert_eq!(app.world().get::<TextColor>(c), Some(&color));
                app.world().get::<Text>(c).expect("a text line").0.clone()
            })
            .collect();
        (shown, lines)
    }

    #[test]
    fn the_open_panel_shows_the_prompt_and_what_help_prints() {
        let mut app = app();
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::F12);
        }
        app.update();
        app.update();
        let (shown, lines) = panel_lines(&mut app);
        assert!(shown, "the panel opens");
        // The prompt alone.
        assert_eq!(lines, [": -"]);
        for (code, text) in [
            (KeyCode::KeyH, "h"),
            (KeyCode::KeyE, "e"),
            (KeyCode::KeyL, "l"),
            (KeyCode::KeyP, "p"),
        ] {
            key(&mut app, code, Some(text));
        }
        app.update();
        assert_eq!(panel_lines(&mut app).1, [": help-"]);
        key(&mut app, KeyCode::Enter, Some("\r"));
        app.update();
        app.update();
        let (_, lines) = panel_lines(&mut app);
        // The last LOG_LINES lines of the log, then the empty prompt.
        let log = app.world().resource::<Console>().log.clone();
        assert_eq!(log.len(), LOG_LINES);
        assert_eq!(log.last().map(String::as_str), HELP.last().copied());
        let expected: Vec<&str> = log.iter().map(String::as_str).chain([": -"]).collect();
        assert_eq!(lines, expected);
        // Idle, the lines stay put: rewritten every frame, they would be
        // despawned before the UI ever laid them out, and never show.
        let children = |app: &mut App| -> Vec<Entity> {
            let mut q = app
                .world_mut()
                .query_filtered::<&Children, With<ConsolePanel>>();
            q.single(app.world()).unwrap().to_vec()
        };
        let before = children(&mut app);
        app.update();
        app.update();
        assert_eq!(children(&mut app), before, "rewritten while idle");
    }

    #[test]
    fn the_same_keys_close_it_pressed_one_frame_after_another() {
        let mut app = app();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::F12);
        }
        app.update();
        assert!(app.world().resource::<Console>().open);
        // Let go of everything, then Ctrl, Shift and F12 down a frame
        // apart, as a hand presses them.
        for code in [KeyCode::F12, KeyCode::ShiftLeft, KeyCode::ControlLeft] {
            send(&mut app, code, None, ButtonState::Released);
        }
        app.update();
        for code in [KeyCode::ControlRight, KeyCode::ShiftLeft] {
            send(&mut app, code, None, ButtonState::Pressed);
            app.update();
            assert!(app.world().resource::<Console>().open);
        }
        send(&mut app, KeyCode::F12, None, ButtonState::Pressed);
        app.update();
        assert!(!app.world().resource::<Console>().open, "closed");
        assert!(!panel_lines(&mut app).0, "the panel hides with it");
        // F12 alone, after letting go of Shift, does not close it.
        let mut app = self::app();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::F12);
        }
        app.update();
        send(&mut app, KeyCode::ShiftLeft, None, ButtonState::Released);
        send(&mut app, KeyCode::F12, None, ButtonState::Released);
        app.update();
        send(&mut app, KeyCode::F12, None, ButtonState::Pressed);
        app.update();
        assert!(app.world().resource::<Console>().open, "Ctrl+F12 only");
    }

    #[test]
    fn ctrl_shift_f12_opens_it_to_type_and_keeps_keys_from_the_game() {
        let mut app = app();
        // Without the combination, typing does nothing.
        key(&mut app, KeyCode::KeyH, Some("h"));
        app.update();
        assert_eq!(*app.world().resource::<Console>(), Console::default());
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::F12);
        }
        app.update();
        assert!(app.world().resource::<Console>().open);
        // While open, a held W reaches no one: the rider stays put.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..10 {
            app.update();
            assert!(
                !app.world()
                    .resource::<ButtonInput<KeyCode>>()
                    .pressed(KeyCode::KeyW)
            );
        }
        assert_eq!(rider(&mut app).speed, 0.0);
        // Typed, corrected, entered and run.
        for (code, text) in [
            (KeyCode::KeyH, "h"),
            (KeyCode::KeyE, "e"),
            (KeyCode::KeyX, "x"),
        ] {
            key(&mut app, code, Some(text));
        }
        key(&mut app, KeyCode::Backspace, None);
        key(&mut app, KeyCode::KeyL, Some("l"));
        key(&mut app, KeyCode::KeyP, Some("p"));
        app.update();
        assert_eq!(app.world().resource::<Console>().input, "help");
        key(&mut app, KeyCode::Enter, Some("\r"));
        app.update();
        let console = app.world().resource::<Console>();
        assert_eq!(console.input, "");
        assert_eq!(console.log.last().map(String::as_str), HELP.last().copied());
        // Esc closes it, and does not reach the game (which would quit).
        key(&mut app, KeyCode::Escape, None);
        app.update();
        assert!(!app.world().resource::<Console>().open);
    }

    #[test]
    fn the_esc_that_closes_the_console_does_not_pause_the_game() {
        let mut app = app();
        app.add_plugins(crate::menu::MenuPlugin {
            title: false,
            settings: None,
        });
        app.update();
        app.world_mut().resource_mut::<Console>().open = true;
        // The window's Esc, both as the key held and as the key typed.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        send(&mut app, KeyCode::Escape, None, ButtonState::Pressed);
        app.update();
        assert!(!app.world().resource::<Console>().open);
        assert_eq!(app.world().resource::<crate::menu::Menu>().screen, None);
    }
}
