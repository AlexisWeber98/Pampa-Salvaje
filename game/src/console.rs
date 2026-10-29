//! La consola de trucos para desarrollo. Solo la tienen las compilaciones de
//! depuración: `main` declara este módulo bajo `cfg(debug_assertions)`, así que
//! una compilación `--release` no tiene nada de ella.
//!
//! Ctrl+Shift+F12 ([`is_toggle`]) la abre: un pequeño panel con el estilo del
//! HUD abajo a la izquierda, con una línea para escribir. Enter ejecuta la
//! línea; Esc (o la combinación de nuevo) la cierra. Mientras está abierta, el
//! juego no ve ninguna tecla ([`read_keys`] las borra), así que escribir nunca
//! mueve al jinete, ni sale del juego, ni orbita la cámara.
//!
//! Comandos ([`parse`]):
//! - `goto PLACE`: pico, castex, nievas, metileo, caleufu, arocena (cualquier
//!   palabra del nombre de un pueblo o de una laguna, con acentos opcionales),
//!   `lake` (la laguna más cercana), `ranch` (la estancia más cercana) o
//!   `ranch castex` / `ranch pico` (la más cercana a ese pueblo);
//! - `goto herd`: el potrero de la manada más cercana;
//! - `goto X Y`: un punto, en metros del juego;
//! - `horse`: un caballo ensillado junto al jugador;
//! - `turbo on` / `turbo off`: el caballo del jugador galopa a hasta
//!   [`TURBO_KMH`] (y llega rápido a esa velocidad), o vuelve a su galope
//!   normal;
//! - `fauna`: cuántos animales de cada tipo viven alrededor del jugador;
//! - `unlock all`: todas las habilidades ([`PlayerAbilities`]);
//! - `models on` / `models off`: dibuja a Hidalgo, Farach y Bairoletto como los
//!   modelos de MakeHuman (por defecto) o de vuelta como las viejas figuras
//!   rígidas ([`CharacterModelsShown`]; solo esta consola los desactiva);
//! - `time HH` / `time HH:MM`: adelanta el reloj hasta esa hora;
//! - `view fp` / `view tp`: mira desde los ojos del jugador o desde la órbita
//!   ([`ViewMode`]), como `V` fuera del tren;
//! - `help`: la lista.
//!
//! `goto` pone al jugador (y al caballo, si cabalga) en el terreno despejado
//! más cercano al lugar ([`World::clear_spot`]) y mueve allí el foco del
//! streaming de inmediato. A bordo del tren se niega: el viaje controla al
//! jugador hasta que se baja.

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

/// Máximo de líneas de salida que se mantienen en pantalla.
const LOG_LINES: usize = 6;
/// Galope máximo del caballo del jugador con `turbo on`, en km/h.
pub const TURBO_KMH: f32 = 70.0;
/// Qué tan rápido llega el caballo a esa velocidad con `turbo on`, en m/s².
const TURBO_ACCELERATION: f32 = 10.0;
/// Distancia máxima desde un lugar a la que puede estar el terreno despejado,
/// en metros.
const SEARCH: f32 = 300.0;

/// Lo que la consola enumera con `help`.
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

/// Si las teclas de este frame abren (o cierran) la consola: Ctrl+Shift+F12.
pub fn is_toggle(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
        && keys.just_pressed(KeyCode::F12)
}

/// Un comando de la consola.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Goto(Destination),
    Horse,
    /// Cuenta los animales alrededor del jugador.
    Fauna,
    UnlockAll,
    /// Dibuja a las personas del encargo como los modelos de MakeHuman (o no).
    Models(bool),
    /// Adelanta el reloj hasta esta hora del día (hoy o mañana).
    Time(TimeOfDay),
    /// Deja que el caballo del jugador galope a [`TURBO_KMH`] (o vuelva a lo
    /// normal).
    Turbo(bool),
    /// Mira desde los ojos del jugador (`true`) o desde la órbita.
    View(bool),
    Help,
}

/// A dónde va `goto`.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    /// Un pueblo o una laguna, por una palabra de su nombre.
    Named(String),
    /// La laguna más cercana al jugador.
    Lake,
    /// El potrero de la manada más cercana al jugador.
    Herd,
    /// La estancia más cercana al jugador, o al pueblo nombrado.
    Ranch(Option<String>),
    /// Un punto, en metros del juego.
    Point(GroundVec2),
}

/// Lee una línea escrita; `Err` es la línea a mostrar en su lugar.
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

/// `HH` o `HH:MM` en un reloj de 24 horas.
fn time_of_day(word: &str) -> Option<TimeOfDay> {
    let (h, m) = word.split_once(':').unwrap_or((word, "0"));
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then(|| TimeOfDay::hm(h, m))
}

/// Si una palabra de `name` es `word`, sin distinguir mayúsculas ni acentos
/// (`caleufu` nombra a Caleufú).
fn named(name: &str, word: &str) -> bool {
    let word = plain(word);
    plain(name).split_whitespace().any(|w| w == word)
}

/// `text` en minúsculas, sin sus acentos ni su tilde del español.
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

/// El pueblo con una palabra de su nombre igual a `word`.
fn town<'a>(towns: &'a [TownLayout], word: &str) -> Option<&'a TownLayout> {
    towns.iter().find(|t| named(&t.name, word))
}

/// El punto al que apunta `goto`, antes de buscar terreno despejado: la llegada
/// de un pueblo por su camino principal, el desembarcadero de una laguna al
/// final de su camino (o su ribera seca más cercana a `from`), el patio de una
/// estancia.
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
        // Las manadas viven en la fauna, no aquí: `run_entered` primero
        // convierte esto en el punto del potrero.
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

/// El ajuste del caballo del jugador con `turbo` activado o desactivado.
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

/// El centro del potrero de la manada más cercana a `from`.
fn nearest_herd(site: &FaunaSite, from: GroundVec2) -> Option<GroundVec2> {
    site.herds()
        .iter()
        .map(|h| h.pasture.center())
        .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
}

/// Dónde pone al jugador `goto destination`: el terreno despejado y seco más
/// cercano al lugar dentro del mundo, con lugar suficiente para un caballo y su
/// jinete.
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

/// La consola: abierta o no, la línea que se está escribiendo, lo que imprimió.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Console {
    pub open: bool,
    pub input: String,
    pub log: Vec<String>,
    /// Líneas ingresadas y todavía no ejecutadas.
    pub entered: Vec<String>,
}

impl Console {
    fn print(&mut self, line: impl Into<String>) {
        self.log.push(line.into());
        let extra = self.log.len().saturating_sub(LOG_LINES);
        self.log.drain(..extra);
    }
}

/// El panel de la consola y sus líneas.
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

/// Los modificadores de la combinación ([`is_toggle`]) mantenidos presionados
/// mientras la consola está abierta. La consola borra [`ButtonInput`] en cada
/// frame, lo que olvida las teclas mantenidas hasta que se vuelven a presionar,
/// así que en su lugar las sigue a partir de las propias pulsaciones y
/// liberaciones del teclado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Held {
    ctrl: [bool; 2],
    shift: [bool; 2],
}

impl Held {
    /// Lo que mantiene presionado `keys`.
    fn from_keys(keys: &ButtonInput<KeyCode>) -> Self {
        Self {
            ctrl: [KeyCode::ControlLeft, KeyCode::ControlRight].map(|k| keys.pressed(k)),
            shift: [KeyCode::ShiftLeft, KeyCode::ShiftRight].map(|k| keys.pressed(k)),
        }
    }

    /// Sigue una pulsación o liberación de `key`; `false` si no es un
    /// modificador.
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

/// Abre y cierra la consola y, mientras está abierta, escribe en ella y oculta
/// todas las teclas al juego.
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

/// Lo que un comando lee del mundo.
type Places<'w> = (
    Option<Res<'w, WorldTowns>>,
    Option<Res<'w, WorldSettlements>>,
    ResMut<'w, GameWorld>,
    ResMut<'w, StreamFocus>,
);

/// Lo que un comando lee de la fauna, y la vista que puede cambiar.
type Watched<'w> = (
    Option<Res<'w, crate::fauna::Fauna>>,
    Option<Res<'w, crate::fauna::WorldFauna>>,
    Option<ResMut<'w, ViewMode>>,
);

/// Ejecuta las líneas ingresadas e imprime lo que hizo cada una.
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
    // Solo se toca cuando hay líneas para ejecutar: un cambio en cada frame
    // volvería a rotular el panel en cada frame, eliminando sus líneas antes de
    // que la UI las disponga, así que se vería vacío.
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

/// Pone a `rider` en `to`, en reposo y mirando hacia el mismo lado, a pie o a
/// caballo como estaba.
pub fn teleport(rider: &mut Rider, transform: &mut Transform, to: GroundVec2, world: &World) {
    let heading = rider.heading;
    *rider = match rider.travel() {
        Travel::OnFoot => Rider::on_foot(to, heading, world),
        Travel::Riding => Rider::new(to, heading, world),
    };
    transform.translation = world_from_ground(rider.position, rider.height);
}

/// Un caballo parado a unos metros a la izquierda del jinete (o en el terreno
/// despejado más cercano a ese punto), mirando hacia su mismo lado, y también
/// apartado de él.
pub fn horse_beside(rider: &Rider, world: &World) -> Option<TetheredHorse> {
    let ahead = rider.heading.forward();
    let left = GroundVec2::new(-ahead.y, ahead.x);
    // Un círculo alrededor de la huella en planta del caballo, girado en
    // cualquier dirección.
    let body = Circle::new(TetheredHorse::LENGTH / 2.0 + 0.2);
    let clear = body.radius + rider.collider.radius;
    let spot = world.clear_spot(body, rider.position + left * (clear + 0.5), 30.0)?;
    (spot.distance(rider.position) >= clear).then(|| TetheredHorse::standing(spot, rider.heading))
}

/// El panel de la consola, oculto, abajo a la izquierda.
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

/// Muestra el panel mientras la consola está abierta y reescribe su texto
/// cuando cambió: la salida y luego la línea que se está escribiendo.
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
        let loaded = crate::shipped_world::loaded();
        let laid = crate::shipped_world::laid();
        let world = &laid.world;
        let collider = Circle::new(Rider::COLLIDER_RADIUS);
        let lagoon = &world.terrain.lagoons[0];
        let town = |name: &str| loaded.towns.iter().find(|t| t.name == name).unwrap();
        let near_town = |name: &str| {
            let band = town(name).leveled();
            move |p: GroundVec2| band.closest_point(p).distance(p)
        };
        let from = loaded.towns[0].arrival().0;
        // Cada línea, a qué distancia queda su punto del lugar que nombra, y el
        // máximo.
        type Distance<'a> = Box<dyn Fn(GroundVec2) -> f32 + 'a>;
        let cases: [(&str, Distance, f32); 11] = [
            ("goto lake", Box::new(|p| lagoon.offshore(p)), 40.0),
            ("goto arocena", Box::new(|p| lagoon.offshore(p)), 40.0),
            ("goto pico", Box::new(near_town("General Pico")), 400.0),
            ("goto castex", Box::new(near_town("Eduardo Castex")), 400.0),
            ("goto nievas", Box::new(near_town("Monte Nievas")), 400.0),
            ("goto metileo", Box::new(near_town("Metileo")), 400.0),
            // Los acentos son opcionales: "caleufu" nombra a Caleufú.
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
        // El patio de una estancia, junto a la estancia más cercana.
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
        // Puntos: fuera del mapa, se traen hacia adentro; los nombres
        // desconocidos lo informan.
        let edge = world.bounds.max + GroundVec2::new(500.0, 500.0);
        let to = resolve(&Destination::Point(edge), &[], world, None, from).unwrap();
        assert_eq!(world.bounds.contain(to, collider.radius).position, to);
        let unknown = Destination::Named("rosario".into());
        assert!(resolve(&unknown, &loaded.towns, world, None, from).is_err());
        let no_ranch = Destination::Ranch(None);
        assert!(resolve(&no_ranch, &loaded.towns, world, None, from).is_err());
    }

    /// Los pueblos incluidos al mediodía, con el jugador a pie en el andén de
    /// Monte Nievas, con la consola.
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
        // El mundo hace streaming alrededor del nuevo punto, el chunk más
        // cercano primero.
        let focus = app.world().resource::<StreamFocus>().0;
        assert_eq!(focus, at.position);
        let config = app.world().resource::<GameWorld>().0.config;
        let mut chunks = pampa_render::StreamingSet::new(3, 4);
        let plan = chunks.update(&config, focus);
        assert_eq!(plan.spawn[0], config.chunk_of(at.position));
        // A caballo, el caballo viene también.
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
        // Los lugares desconocidos no mueven al jugador y lo informan.
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
        // Una hora anterior es la de mañana: el reloj nunca retrocede.
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
        // En plena pampa.
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
        // Todavía no le corresponde montarlo: F no hace nada.
        crate::ride::testing::press(&mut app, crate::errand::INTERACT_KEY);
        assert_eq!(rider(&mut app).travel(), Travel::OnFoot);
        // Todas las habilidades desbloqueadas: ahora lo deja subir.
        enter(&mut app, "unlock all");
        let abilities = app.world().resource::<PlayerAbilities>().0;
        assert!(Ability::ALL.iter().all(|&a| abilities.has(a)));
        crate::ride::testing::press(&mut app, crate::errand::INTERACT_KEY);
        let me = rider(&mut app);
        assert_eq!(me.travel(), Travel::Riding);
        assert_eq!(me.position, horse.position);
    }

    /// Una tecla presionada y soltada, tal como la envía la ventana.
    fn key(app: &mut App, code: KeyCode, text: Option<&str>) {
        send(app, code, text, ButtonState::Pressed);
        send(app, code, text, ButtonState::Released);
    }

    /// Una tecla que baja o sube, tal como la envía la ventana.
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

    /// Las líneas que muestra el panel, de arriba abajo, como su texto, y si el
    /// panel está visible. Cada línea es texto en tinta con la tipografía del
    /// cuerpo.
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
        // Solo el prompt.
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
        // Las últimas LOG_LINES líneas del log, y luego el prompt vacío.
        let log = app.world().resource::<Console>().log.clone();
        assert_eq!(log.len(), LOG_LINES);
        assert_eq!(log.last().map(String::as_str), HELP.last().copied());
        let expected: Vec<&str> = log.iter().map(String::as_str).chain([": -"]).collect();
        assert_eq!(lines, expected);
        // En reposo, las líneas no cambian: reescritas en cada frame, se
        // eliminarían antes de que la UI llegue a disponerlas, y nunca se
        // verían.
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
        // Soltar todo, y luego bajar Ctrl, Shift y F12 con un frame de
        // diferencia, como los presiona una mano.
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
        // F12 sola, después de soltar Shift, no la cierra.
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
        // Sin la combinación, escribir no hace nada.
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
        // Mientras está abierta, una W mantenida no llega a nadie: el jinete no
        // se mueve.
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
        // Escrita, corregida, ingresada y ejecutada.
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
        // Esc la cierra, y no llega al juego (que saldría).
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
        // El Esc de la ventana, tanto como tecla mantenida como tecla escrita.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        send(&mut app, KeyCode::Escape, None, ButtonState::Pressed);
        app.update();
        assert!(!app.world().resource::<Console>().open);
        assert_eq!(app.world().resource::<crate::menu::Menu>().screen, None);
    }
}
