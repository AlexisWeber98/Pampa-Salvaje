//! La pantalla de título, el menú de pausa y su pantalla Sonido, con música.
//!
//! El juego empieza en la pantalla de título.
//! - A la izquierda hay un cartel de "Se busca" de dos bandidos rurales de
//!   "Bandidos rurales" de León Gieco: Juan Bautista Bairoletto y Mate Cosido
//!   (nacido Segundo David Peralta). Está clavado sobre un mapa de Argentina de
//!   1882 (`assets/menu/poster.png`).
//! - A la derecha hay una franja de pintura roja (`assets/menu/band.png`)
//!   rotulada con "Pampa Salvaje" y las opciones Jugar, Sonido y Salir.
//!
//! `tools/menu_poster.sh` genera el cartel y la franja.
//!
//! En el juego, Esc o la tecla Pausa del teclado pausan. El mundo queda en
//! pantalla, en gris y con las esquinas oscuras (`assets/menu/vignette.png`).
//! Se muestran el mismo cartel y la misma franja, con Continuar, Sonido y
//! Salir. Esc o Pausa de nuevo, o Continuar, vuelven al juego.
//!
//! Sonido ajusta los volúmenes ([`SoundSettings`]). Se guardan cuando el
//! jugador sale de esa pantalla.
//!
//! Mientras hay un menú abierto ([`Menu::screen`]), el mundo se queda quieto.
//! - El tiempo virtual está en pausa, así que todo lo que corre sobre `Time`
//!   espera: el reloj, los trenes, los animales y la gente.
//! - El juego no ve teclas, botones del mouse, movimiento ni rueda.
//!   [`read_keys`] los borra después de que la consola de desarrollo leyó los
//!   suyos.
//! - El cursor queda libre.
//!
//! La música (`assets/menu/menu_2.mp3`, la canción propia del proyecto) se
//! repite solo mientras hay un menú abierto, y empieza desde el principio cada
//! vez que se abre uno.
//!
//! Teclas:
//! - Arriba/Abajo o W/S: elegir una línea.
//! - Izquierda/Derecha o A/D: bajar o subir un volumen.
//! - Enter, Espacio o F: tomar la opción.
//! - Esc o Pausa: volver (en el juego, pausar).
//!
//! Salir con Sonido abierto (cerrando la ventana) también guarda sus volúmenes.

use std::path::PathBuf;

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume,
};
use bevy::input::InputSystems;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use pampa_render::Typefaces;

use crate::sound::{MAX_STEP, SoundSettings};

/// La música de los menús, relativa a la raíz de assets.
pub const MUSIC: &str = "menu/menu_2.mp3";
/// El cartel de "Se busca" sobre el mapa, relativo a la raíz de assets.
pub const POSTER: &str = "menu/poster.png";
/// La franja de pintura roja sobre la que se rotulan las opciones.
pub const BAND: &str = "menu/band.png";
/// Las esquinas oscuras sobre el mundo en pausa.
pub const VIGNETTE: &str = "menu/vignette.png";

/// El título y la línea elegida: el papel del cartel, claro sobre el rojo.
const PAPER: Color = Color::srgb_u8(246, 234, 204);
/// Una línea no elegida, el encabezado y las ayudas: el papel, desteñido.
const DIM: Color = Color::srgba_u8(233, 214, 178, 150);
/// Detrás de la pantalla de título, mientras carga el mundo.
const BACKDROP: Color = Color::srgb_u8(30, 22, 16);
/// La mayor cantidad de líneas que tiene una pantalla.
const LINES: usize = 3;

/// Qué menú está abierto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Antes del juego: Jugar, Sonido, Salir.
    Title,
    /// Esc en el juego: Continuar, Sonido, Salir.
    Pause,
    /// Los volúmenes, abiertos desde el título o desde el menú de pausa
    /// (`from`).
    Sound { from: Parent },
}

/// La pantalla a la que vuelve Sonido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    Title,
    Pause,
}

impl From<Parent> for Screen {
    fn from(parent: Parent) -> Self {
        match parent {
            Parent::Title => Screen::Title,
            Parent::Pause => Screen::Pause,
        }
    }
}

/// Lo que hace una línea del menú.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Cerrar el menú y jugar (Jugar en el título, Continuar en la pausa).
    Play,
    /// Abrir la pantalla Sonido.
    Sound,
    /// Salir del juego.
    Quit,
    /// El volumen general: Izquierda/Derecha lo ajustan.
    General,
    /// El volumen de la música: Izquierda/Derecha lo ajustan.
    Music,
    /// Volver de Sonido a donde se abrió.
    Back,
}

impl Screen {
    /// Las líneas de la pantalla, de arriba abajo.
    pub fn choices(self) -> &'static [Choice] {
        match self {
            Screen::Title | Screen::Pause => &[Choice::Play, Choice::Sound, Choice::Quit],
            Screen::Sound { .. } => &[Choice::General, Choice::Music, Choice::Back],
        }
    }

    /// Si el juego está debajo de esta pantalla (en pausa) en lugar de no haber
    /// empezado todavía.
    fn over_the_game(self) -> bool {
        matches!(
            self,
            Screen::Pause
                | Screen::Sound {
                    from: Parent::Pause
                }
        )
    }

    fn heading(self) -> &'static str {
        match self {
            Screen::Title => "Bandidos rurales",
            Screen::Pause => "Pausa",
            Screen::Sound { .. } => "Sonido",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Screen::Title | Screen::Pause => "W, S o flechas: elegir.  Enter: aceptar.",
            Screen::Sound { .. } => "A, D o flechas: volumen.  Esc: volver.",
        }
    }
}

/// El texto de `choice` en `screen`.
pub fn label(choice: Choice, screen: Screen, sound: &SoundSettings) -> String {
    let percent = |step: u8| u32::from(step) * 100 / u32::from(MAX_STEP);
    match choice {
        Choice::Play if screen == Screen::Pause => "Continuar".into(),
        Choice::Play => "Jugar".into(),
        Choice::Sound => "Sonido".into(),
        Choice::Quit => "Salir".into(),
        Choice::General => format!("Volumen general: {} %", percent(sound.general)),
        Choice::Music => format!("Música: {} %", percent(sound.music)),
        Choice::Back => "Volver".into(),
    }
}

/// El menú abierto, si lo hay, y su línea resaltada.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Menu {
    pub screen: Option<Screen>,
    pub selected: usize,
}

impl Menu {
    fn open(&mut self, screen: Screen) {
        self.show(screen, 0);
    }

    fn show(&mut self, screen: Screen, selected: usize) {
        *self = Menu {
            screen: Some(screen),
            selected,
        };
    }

    /// La opción resaltada del menú abierto.
    pub fn choice(&self) -> Option<Choice> {
        self.screen.map(|s| s.choices()[self.selected])
    }

    /// Mueve el resaltado `step` líneas hacia abajo (negativo: hacia arriba),
    /// dando la vuelta.
    fn step(&mut self, step: isize) {
        if let Some(screen) = self.screen {
            let n = screen.choices().len() as isize;
            self.selected = (self.selected as isize + step).rem_euclid(n) as usize;
        }
    }
}

/// Dónde se guarda [`SoundSettings`]; `None` los conserva solo durante esta
/// ejecución.
#[derive(Resource, Debug, Clone, Default)]
pub struct SettingsFile(pub Option<PathBuf>);

/// Los lectores de teclas que ocultan las teclas al juego: el del menú se
/// ejecuta después del de la consola de desarrollo, así que el Esc que cierra
/// la consola nunca pausa.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuKeys;

/// La pantalla de título, el menú de pausa y la pantalla Sonido.
pub struct MenuPlugin {
    /// Empezar en la pantalla de título (el recorrido de capturas de desarrollo
    /// no lo hace).
    pub title: bool,
    /// De dónde se lee y dónde se guarda la configuración de sonido.
    pub settings: Option<PathBuf>,
}

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        let mut menu = Menu::default();
        if self.title {
            menu.open(Screen::Title);
        }
        let (sound, error) = self
            .settings
            .as_deref()
            .map_or((SoundSettings::default(), None), SoundSettings::load);
        if let Some(error) = error {
            app.add_systems(Startup, move || warn!("sound settings: {error}"));
        }
        app.insert_resource(menu)
            .insert_resource(sound)
            .insert_resource(SettingsFile(self.settings.clone()))
            .init_resource::<MenuSong>()
            .init_resource::<Typefaces>()
            .add_systems(Startup, spawn_menu)
            .add_systems(PreUpdate, read_keys.after(InputSystems).in_set(MenuKeys))
            .add_systems(Update, (hold_world, grey_world, play_music))
            .add_systems(PostUpdate, draw_menu)
            .add_systems(Last, save_on_exit);
    }
}

/// La capa de pantalla completa de los menús.
#[derive(Component)]
struct MenuLayer;

/// Las esquinas oscuras, mostradas solo sobre el mundo en pausa.
#[derive(Component)]
struct MenuVignette;

/// La línea bajo el título: "Bandidos rurales", "Pausa" o "Sonido".
#[derive(Component)]
struct MenuHeading;

/// La línea de una opción, por su índice en [`Screen::choices`].
#[derive(Component)]
struct MenuLine(usize);

/// La ayuda de teclas al pie de las opciones.
#[derive(Component)]
struct MenuHint;

/// La música de los menús: un reproductor mientras hay un menú abierto,
/// eliminado (lo que la detiene) cuando el juego se reanuda.
#[derive(Component)]
struct MenuMusic;

/// La canción de los menús, cargada una sola vez.
#[derive(Resource, Default)]
struct MenuSong(Handle<AudioSource>);

fn spawn_menu(mut commands: Commands, server: Option<Res<AssetServer>>, faces: Res<Typefaces>) {
    let load = |path: &'static str| {
        server
            .as_ref()
            .map_or_else(Handle::default, |s| s.load(path))
    };
    commands.insert_resource(MenuSong(
        server
            .as_ref()
            .map_or_else(Handle::default, |s| s.load(MUSIC)),
    ));
    let font = |face: &Handle<Font>, vh: f32| TextFont {
        font: face.into(),
        font_size: FontSize::Vh(vh),
        ..default()
    };
    let fill = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    };
    commands
        .spawn((
            Name::new("Menu"),
            MenuLayer,
            Node {
                display: Display::None,
                ..fill.clone()
            },
            BackgroundColor(Color::NONE),
            // Por encima del HUD, del minimapa y del fundido de pantalla.
            GlobalZIndex(200),
        ))
        .with_children(|layer| {
            layer.spawn((MenuVignette, ImageNode::new(load(VIGNETTE)), fill));
            layer.spawn((
                ImageNode::new(load(POSTER)),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Vw(2.0),
                    top: Val::Vh(3.0),
                    height: Val::Vh(94.0),
                    ..default()
                },
            ));
            // La franja, estirada a lo largo del borde derecho; las opciones
            // empiezan apartadas de su lado izquierdo irregular.
            layer
                .spawn((
                    ImageNode::new(load(BAND)),
                    Node {
                        position_type: PositionType::Absolute,
                        right: Val::Px(0.0),
                        width: Val::Vw(36.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::left(Val::Vw(7.0)),
                        row_gap: Val::Vh(1.5),
                        ..default()
                    },
                ))
                .with_children(|band| {
                    band.spawn((
                        Text::new("Pampa Salvaje"),
                        font(&faces.display, 7.5),
                        TextColor(PAPER),
                    ));
                    band.spawn((
                        MenuHeading,
                        Text::default(),
                        font(&faces.italic, 3.6),
                        TextColor(DIM),
                        Node {
                            margin: UiRect::bottom(Val::Vh(5.0)),
                            ..default()
                        },
                    ));
                    for i in 0..LINES {
                        band.spawn((
                            MenuLine(i),
                            Text::default(),
                            font(&faces.body, 4.6),
                            TextColor(DIM),
                        ));
                    }
                    band.spawn((
                        MenuHint,
                        Text::default(),
                        font(&faces.italic, 2.2),
                        TextColor(DIM),
                        Node {
                            margin: UiRect::top(Val::Vh(5.0)),
                            ..default()
                        },
                    ));
                });
        });
}

/// Lo que los menús leen y escriben del sonido.
type Sound<'w> = (ResMut<'w, SoundSettings>, Res<'w, SettingsFile>);

/// Esc o Pausa abren el menú de pausa; en un menú, mueve el resaltado, ajusta
/// los volúmenes y toma la opción, y oculta al juego cada tecla, botón y
/// movimiento del mouse.
fn read_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut buttons: Option<ResMut<ButtonInput<MouseButton>>>,
    mut moved: Option<ResMut<AccumulatedMouseMotion>>,
    mut scrolled: Option<ResMut<AccumulatedMouseScroll>>,
    mut menu: ResMut<Menu>,
    (mut sound, file): Sound,
    mut exit: MessageWriter<AppExit>,
) {
    let any = |codes: &[KeyCode]| keys.any_just_pressed(codes.iter().copied());
    let Some(screen) = menu.screen else {
        if any(&BACK_KEYS) {
            menu.open(Screen::Pause);
            keep_from_game(&mut keys, &mut buttons, &mut moved, &mut scrolled);
        }
        return;
    };
    if any(&[KeyCode::ArrowUp, KeyCode::KeyW]) {
        menu.step(-1);
    }
    if any(&[KeyCode::ArrowDown, KeyCode::KeyS]) {
        menu.step(1);
    }
    let turn = match (
        any(&[KeyCode::ArrowLeft, KeyCode::KeyA]),
        any(&[KeyCode::ArrowRight, KeyCode::KeyD]),
    ) {
        (true, false) => -1,
        (false, true) => 1,
        _ => 0,
    };
    if turn != 0 {
        let volume = match menu.choice() {
            Some(Choice::General) => Some(&mut sound.general),
            Some(Choice::Music) => Some(&mut sound.music),
            _ => None,
        };
        if let Some(volume) = volume {
            *volume = volume.saturating_add_signed(turn).min(MAX_STEP);
        }
    }
    let back = |menu: &mut Menu| match screen {
        Screen::Title => {}
        Screen::Pause => menu.screen = None,
        Screen::Sound { from } => {
            save(&sound, &file);
            menu.show(from.into(), 1);
        }
    };
    if any(&BACK_KEYS) {
        back(&mut menu);
    } else if any(&[
        KeyCode::Enter,
        KeyCode::NumpadEnter,
        KeyCode::Space,
        KeyCode::KeyF,
    ]) {
        match menu.choice() {
            Some(Choice::Play) => menu.screen = None,
            Some(Choice::Sound) => {
                let from = if screen == Screen::Pause {
                    Parent::Pause
                } else {
                    Parent::Title
                };
                menu.open(Screen::Sound { from });
            }
            Some(Choice::Quit) => {
                exit.write(AppExit::Success);
            }
            Some(Choice::Back) => back(&mut menu),
            Some(Choice::General | Choice::Music) | None => {}
        }
    }
    keep_from_game(&mut keys, &mut buttons, &mut moved, &mut scrolled);
}

/// Esc y la tecla Pausa del teclado: abren el menú de pausa en el juego, y
/// vuelven (o reanudan) en un menú.
const BACK_KEYS: [KeyCode; 2] = [KeyCode::Escape, KeyCode::Pause];

/// Borra las teclas, los botones del mouse, el movimiento y la rueda de este
/// frame, para que el juego no vea ninguno.
fn keep_from_game(
    keys: &mut ButtonInput<KeyCode>,
    buttons: &mut Option<ResMut<ButtonInput<MouseButton>>>,
    moved: &mut Option<ResMut<AccumulatedMouseMotion>>,
    scrolled: &mut Option<ResMut<AccumulatedMouseScroll>>,
) {
    keys.reset_all();
    if let Some(buttons) = buttons {
        buttons.reset_all();
    }
    if let Some(moved) = moved {
        **moved = default();
    }
    if let Some(scrolled) = scrolled {
        **scrolled = default();
    }
}

/// Cuando el juego sale con Sonido abierto (cerrando la ventana), guarda los
/// volúmenes ajustados allí, que de otro modo se guardarían al salir de esa
/// pantalla.
fn save_on_exit(mut exits: MessageReader<AppExit>, menu: Res<Menu>, (sound, file): SoundRead) {
    if exits.read().count() > 0 && matches!(menu.screen, Some(Screen::Sound { .. })) {
        save(&sound, &file);
    }
}

/// Lo que lee del sonido la salida del juego.
type SoundRead<'w> = (Res<'w, SoundSettings>, Res<'w, SettingsFile>);

/// Escribe la configuración en su archivo, si tiene uno; un fallo se registra
/// en el log y el juego sigue con ella.
fn save(sound: &SoundSettings, file: &SettingsFile) {
    if let Some(path) = &file.0
        && let Err(e) = sound.save(path)
    {
        warn!("sound settings: {}: {e}", path.display());
    }
}

/// Pausa el tiempo virtual mientras hay un menú abierto.
fn hold_world(menu: Res<Menu>, mut time: ResMut<Time<Virtual>>) {
    let open = menu.screen.is_some();
    if open != time.is_paused() {
        if open {
            time.pause();
        } else {
            time.unpause();
        }
    }
}

/// Pone el mundo en gris bajo un menú abierto y lo devuelve al color en el
/// juego: la gradación de color de las cámaras, que nada más en el juego
/// ajusta.
fn grey_world(
    mut commands: Commands,
    menu: Res<Menu>,
    mut cameras: Query<(Entity, Option<&mut ColorGrading>), With<Camera3d>>,
) {
    let saturation = if menu.screen.is_some() { 0.0 } else { 1.0 };
    for (camera, grading) in &mut cameras {
        match grading {
            Some(mut grading) => {
                if grading.global.post_saturation != saturation {
                    grading.global.post_saturation = saturation;
                }
            }
            None if saturation != 1.0 => {
                let mut grading = ColorGrading::default();
                grading.global.post_saturation = saturation;
                commands.entity(camera).insert(grading);
            }
            None => {}
        }
    }
}

/// Reproduce la música mientras hay un menú abierto, desde el comienzo de la
/// canción cada vez que se abre un menú (retomarla a mitad de la canción
/// resulta chocante, usuario 2026-10-29): se crea un reproductor nuevo cuando
/// se abre el título o el menú de pausa, y se elimina, lo que la detiene,
/// cuando el juego se reanuda. Moverse entre las pantallas del menú la
/// mantiene. El sink del reproductor, una vez cargada la canción, sigue los
/// volúmenes.
fn play_music(
    mut commands: Commands,
    menu: Res<Menu>,
    (sound, song): (Res<SoundSettings>, Res<MenuSong>),
    players: Query<Entity, With<MenuMusic>>,
    mut sinks: Query<&mut AudioSink, With<MenuMusic>>,
) {
    let volume = Volume::Linear(sound.music_gain());
    match (menu.screen.is_some(), players.is_empty()) {
        (true, true) => {
            commands.spawn((
                Name::new("Menu music"),
                MenuMusic,
                AudioPlayer::new(song.0.clone()),
                PlaybackSettings {
                    volume,
                    ..PlaybackSettings::LOOP
                },
            ));
        }
        (false, false) => {
            for player in &players {
                commands.entity(player).despawn();
            }
        }
        _ => {}
    }
    for mut sink in &mut sinks {
        if sink.volume() != volume {
            sink.set_volume(volume);
        }
    }
}

/// El nodo y el fondo de la capa del menú.
type Layers<'w, 's> = Query<
    'w,
    's,
    (&'static mut Node, &'static mut BackgroundColor),
    (With<MenuLayer>, Without<MenuVignette>, Without<MenuLine>),
>;

/// El nodo de la viñeta.
type Vignettes<'w, 's> =
    Query<'w, 's, &'static mut Node, (With<MenuVignette>, Without<MenuLayer>, Without<MenuLine>)>;

/// La línea de una opción: su texto, color y nodo.
type Lines<'w, 's> = Query<
    'w,
    's,
    (
        &'static MenuLine,
        &'static mut Text,
        &'static mut TextColor,
        &'static mut Node,
    ),
    (Without<MenuHeading>, Without<MenuHint>),
>;

/// El encabezado y la ayuda, aparte de las líneas.
type Notes<'w, 's> = Query<
    'w,
    's,
    (&'static mut Text, Has<MenuHeading>),
    (Or<(With<MenuHeading>, With<MenuHint>)>, Without<MenuLine>),
>;

/// Muestra u oculta la capa y reescribe sus líneas cuando cambiaron el menú o
/// los volúmenes.
fn draw_menu(
    menu: Res<Menu>,
    sound: Res<SoundSettings>,
    mut layers: Layers,
    mut vignettes: Vignettes,
    mut notes: Notes,
    mut lines: Lines,
) {
    if !menu.is_changed() && !sound.is_changed() {
        return;
    }
    let Some(screen) = menu.screen else {
        for (mut node, _) in &mut layers {
            node.display = Display::None;
        }
        return;
    };
    // El título oculta el mundo que todavía carga detrás; el menú de pausa
    // muestra el mundo que detiene, en gris, con las esquinas oscuras.
    let over_the_game = screen.over_the_game();
    for (mut node, mut color) in &mut layers {
        node.display = Display::Flex;
        color.0 = if over_the_game { Color::NONE } else { BACKDROP };
    }
    for mut node in &mut vignettes {
        node.display = if over_the_game {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (mut text, heading) in &mut notes {
        text.0 = if heading {
            screen.heading()
        } else {
            screen.hint()
        }
        .into();
    }
    let choices = screen.choices();
    for (MenuLine(i), mut text, mut color, mut node) in &mut lines {
        let Some(&choice) = choices.get(*i) else {
            node.display = Display::None;
            continue;
        };
        node.display = Display::Flex;
        text.0 = label(choice, screen, &sound);
        color.0 = if *i == menu.selected { PAPER } else { DIM };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::audio::PlaybackMode;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::ButtonState;
    use bevy::input::InputPlugin;
    use bevy::input::keyboard::{Key, KeyboardInput, NativeKey};

    fn app_with(title: bool, settings: Option<PathBuf>) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, InputPlugin))
            .add_plugins(MenuPlugin { title, settings });
        app.update();
        app
    }

    fn app(title: bool) -> App {
        app_with(title, None)
    }

    /// Una tecla que baja y sube, tal como la envía la ventana, un frame cada
    /// vez.
    fn press(app: &mut App, key: KeyCode) {
        for state in [ButtonState::Pressed, ButtonState::Released] {
            app.world_mut().write_message(KeyboardInput {
                key_code: key,
                logical_key: Key::Unidentified(NativeKey::Unidentified),
                state,
                text: None,
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
            app.update();
        }
    }

    fn menu(app: &App) -> Menu {
        *app.world().resource::<Menu>()
    }

    fn sound(app: &App) -> SoundSettings {
        *app.world().resource::<SoundSettings>()
    }

    fn paused(app: &App) -> bool {
        app.world().resource::<Time<Virtual>>().is_paused()
    }

    fn shown<T: Component>(app: &mut App) -> bool {
        let mut nodes = app.world_mut().query_filtered::<&Node, With<T>>();
        nodes.iter(app.world()).all(|n| n.display != Display::None)
    }

    fn backdrop(app: &mut App) -> Color {
        let mut layers = app
            .world_mut()
            .query_filtered::<&BackgroundColor, With<MenuLayer>>();
        layers.single(app.world()).expect("one layer").0
    }

    /// Los textos de las líneas mostradas, de arriba abajo, con el índice de la
    /// resaltada.
    fn lines(app: &mut App) -> (Vec<String>, Vec<usize>) {
        let mut query = app
            .world_mut()
            .query::<(&MenuLine, &Text, &TextColor, &Node)>();
        let mut shown: Vec<_> = query
            .iter(app.world())
            .filter(|(.., node)| node.display != Display::None)
            .map(|(line, text, color, _)| (line.0, text.0.clone(), color.0 == PAPER))
            .collect();
        shown.sort_by_key(|(i, ..)| *i);
        let lit = shown
            .iter()
            .filter(|(.., lit)| *lit)
            .map(|(i, ..)| *i)
            .collect();
        (shown.into_iter().map(|(_, text, _)| text).collect(), lit)
    }

    fn heading(app: &mut App) -> String {
        let mut query = app.world_mut().query_filtered::<&Text, With<MenuHeading>>();
        query.single(app.world()).expect("one heading").0.clone()
    }

    /// La saturación de la cámara del mundo; 1 sin gradación de color.
    fn saturation(app: &mut App) -> f32 {
        let mut query = app
            .world_mut()
            .query_filtered::<Option<&ColorGrading>, With<Camera3d>>();
        query
            .single(app.world())
            .expect("one camera")
            .map_or(1.0, |g| g.global.post_saturation)
    }

    fn exits(app: &mut App) -> usize {
        app.world_mut()
            .resource_mut::<Messages<AppExit>>()
            .drain()
            .count()
    }

    /// Un directorio nuevo bajo el directorio temporal del sistema, que se
    /// elimina al soltarlo.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("pampa-menu-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_game_starts_on_the_title_with_the_world_held() {
        let mut app = app(true);
        assert_eq!(menu(&app).screen, Some(Screen::Title));
        assert!(paused(&app));
        assert!(shown::<MenuLayer>(&mut app));
        // El mundo que carga detrás está oculto, así que no hay viñeta encima.
        assert_eq!(backdrop(&mut app), BACKDROP);
        assert!(!shown::<MenuVignette>(&mut app));
        assert_eq!(heading(&mut app), "Bandidos rurales");
        assert_eq!(
            lines(&mut app),
            (
                vec!["Jugar".into(), "Sonido".into(), "Salir".into()],
                vec![0]
            )
        );
    }

    #[test]
    fn jugar_closes_the_title_and_lets_the_world_run() {
        let mut app = app(true);
        press(&mut app, KeyCode::Enter);
        assert_eq!(menu(&app).screen, None);
        assert!(!paused(&app));
        assert!(!shown::<MenuLayer>(&mut app));
        assert_eq!(exits(&mut app), 0);
    }

    #[test]
    fn esc_and_the_pause_key_on_the_title_do_nothing() {
        let mut app = app(true);
        press(&mut app, KeyCode::Escape);
        assert_eq!(menu(&app).screen, Some(Screen::Title));
        assert_eq!(exits(&mut app), 0);
        press(&mut app, KeyCode::Pause);
        assert_eq!(menu(&app).screen, Some(Screen::Title));
    }

    #[test]
    fn esc_pauses_the_game_and_esc_again_resumes_it() {
        let mut app = app(false);
        assert_eq!(menu(&app).screen, None);
        assert!(!paused(&app));
        press(&mut app, KeyCode::Escape);
        assert_eq!(menu(&app).screen, Some(Screen::Pause));
        assert!(paused(&app));
        assert!(shown::<MenuLayer>(&mut app));
        assert_eq!(heading(&mut app), "Pausa");
        assert_eq!(lines(&mut app).0, ["Continuar", "Sonido", "Salir"]);
        // Esc ya no sale del juego.
        assert_eq!(exits(&mut app), 0);
        press(&mut app, KeyCode::Escape);
        assert_eq!(menu(&app).screen, None);
        assert!(!paused(&app));
    }

    #[test]
    fn the_paused_world_shows_grey_with_dark_corners_and_comes_back_in_color() {
        let mut app = app(false);
        app.world_mut().spawn(Camera3d::default());
        app.update();
        assert_eq!(saturation(&mut app), 1.0);
        press(&mut app, KeyCode::Escape);
        assert_eq!(saturation(&mut app), 0.0);
        assert_eq!(backdrop(&mut app), Color::NONE);
        assert!(shown::<MenuVignette>(&mut app));
        // Sonido desde el menú de pausa mantiene el mundo en gris debajo.
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        assert_eq!(saturation(&mut app), 0.0);
        assert!(shown::<MenuVignette>(&mut app));
        press(&mut app, KeyCode::Escape);
        press(&mut app, KeyCode::Escape);
        assert_eq!(menu(&app).screen, None);
        assert_eq!(saturation(&mut app), 1.0);
    }

    #[test]
    fn the_pause_key_pauses_the_game_and_resumes_it_like_esc() {
        let mut app = app(false);
        press(&mut app, KeyCode::Pause);
        assert_eq!(menu(&app).screen, Some(Screen::Pause));
        assert!(paused(&app));
        // En Sonido vuelve atrás, como lo hace Esc.
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Pause);
        assert_eq!(menu(&app).screen, Some(Screen::Pause));
        press(&mut app, KeyCode::Pause);
        assert_eq!(menu(&app).screen, None);
        assert!(!paused(&app));
        assert_eq!(exits(&mut app), 0);
    }

    #[test]
    fn the_frame_that_opens_the_pause_keeps_the_mouse_from_the_game() {
        let mut app = app(false);
        let world = app.world_mut();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        world.resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(5.0, 3.0);
        world.resource_mut::<AccumulatedMouseScroll>().delta = Vec2::new(0.0, 2.0);
        app.world_mut()
            .run_system_once(read_keys)
            .expect("the menu reads keys");
        assert_eq!(menu(&app).screen, Some(Screen::Pause));
        let world = app.world();
        assert!(
            !world
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Left)
        );
        assert_eq!(world.resource::<AccumulatedMouseMotion>().delta, Vec2::ZERO);
        assert_eq!(world.resource::<AccumulatedMouseScroll>().delta, Vec2::ZERO);
    }

    #[test]
    fn quitting_with_sonido_open_keeps_the_new_volumes() {
        let scratch = Scratch::new("exit");
        let path = scratch.0.join("settings.ron");
        let mut app = app_with(true, Some(path.clone()));
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::ArrowLeft);
        // La ventana se cierra con Sonido todavía abierto.
        app.world_mut().write_message(AppExit::Success);
        app.update();
        let wanted = SoundSettings {
            general: MAX_STEP,
            music: 3,
        };
        assert_eq!(SoundSettings::load(&path), (wanted, None));
    }

    /// Los reproductores de la música del menú: uno nuevo reproduce la canción
    /// desde el comienzo.
    fn music_players(app: &mut App) -> Vec<(Entity, PlaybackSettings)> {
        let mut players = app
            .world_mut()
            .query_filtered::<(Entity, &PlaybackSettings), With<MenuMusic>>();
        players.iter(app.world()).map(|(e, p)| (e, *p)).collect()
    }

    #[test]
    fn the_music_starts_over_each_time_a_menu_opens() {
        // El título reproduce la canción desde el comienzo, en bucle, a su
        // volumen.
        let mut app = app(true);
        let title = music_players(&mut app);
        assert_eq!(title.len(), 1);
        let (_, settings) = title[0];
        assert!(matches!(settings.mode, PlaybackMode::Loop));
        assert!(!settings.paused);
        assert_eq!(settings.volume, Volume::Linear(sound(&app).music_gain()));
        // En silencio en el juego.
        press(&mut app, KeyCode::Enter);
        assert!(music_players(&mut app).is_empty());
        // Cada pausa vuelve a empezar la canción con un reproductor nuevo.
        press(&mut app, KeyCode::Escape);
        let first = music_players(&mut app);
        assert_eq!(first.len(), 1);
        assert_ne!(first[0].0, title[0].0);
        // Entrar a Sonido y volver es el mismo menú: la canción continúa.
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Escape);
        assert_eq!(music_players(&mut app)[0].0, first[0].0);
        press(&mut app, KeyCode::Escape);
        assert!(music_players(&mut app).is_empty());
        press(&mut app, KeyCode::Pause);
        let second = music_players(&mut app);
        assert_eq!(second.len(), 1);
        assert_ne!(second[0].0, first[0].0);
    }

    #[test]
    fn continuar_resumes_the_game() {
        let mut app = app(false);
        press(&mut app, KeyCode::Escape);
        press(&mut app, KeyCode::Space);
        assert_eq!(menu(&app).screen, None);
        assert!(!paused(&app));
    }

    #[test]
    fn salir_quits_from_the_title_and_from_the_pause_menu() {
        for (title, open) in [(true, None), (false, Some(KeyCode::Escape))] {
            let mut app = app(title);
            if let Some(key) = open {
                press(&mut app, key);
            }
            press(&mut app, KeyCode::ArrowUp);
            assert_eq!(menu(&app).choice(), Some(Choice::Quit));
            press(&mut app, KeyCode::KeyF);
            assert_eq!(exits(&mut app), 1);
        }
    }

    #[test]
    fn the_highlight_wraps_round_both_ways() {
        let mut app = app(true);
        press(&mut app, KeyCode::KeyW);
        assert_eq!(menu(&app).choice(), Some(Choice::Quit));
        assert_eq!(lines(&mut app).1, [2]);
        press(&mut app, KeyCode::KeyS);
        assert_eq!(menu(&app).choice(), Some(Choice::Play));
        assert_eq!(lines(&mut app).1, [0]);
        press(&mut app, KeyCode::ArrowDown);
        assert_eq!(menu(&app).choice(), Some(Choice::Sound));
        assert_eq!(lines(&mut app).1, [1]);
    }

    #[test]
    fn sonido_shows_the_volumes_and_goes_back_where_it_came_from() {
        for (title, parent) in [(true, Parent::Title), (false, Parent::Pause)] {
            let mut app = app(title);
            if !title {
                press(&mut app, KeyCode::Escape);
            }
            press(&mut app, KeyCode::ArrowDown);
            press(&mut app, KeyCode::Enter);
            assert_eq!(menu(&app).screen, Some(Screen::Sound { from: parent }));
            assert!(paused(&app));
            assert_eq!(heading(&mut app), "Sonido");
            assert_eq!(
                lines(&mut app),
                (
                    vec![
                        "Volumen general: 100 %".into(),
                        "Música: 40 %".into(),
                        "Volver".into()
                    ],
                    vec![0]
                )
            );
            // Volver regresa con Sonido resaltado.
            press(&mut app, KeyCode::ArrowUp);
            press(&mut app, KeyCode::Enter);
            let returned = Menu {
                screen: Some(parent.into()),
                selected: 1,
            };
            assert_eq!(menu(&app), returned);
            // Lo mismo hace Esc; Esc en Sonido nunca cierra el menú de pausa.
            press(&mut app, KeyCode::Enter);
            press(&mut app, KeyCode::Escape);
            assert_eq!(menu(&app), returned);
        }
    }

    #[test]
    fn left_and_right_set_the_highlighted_volume_within_its_range() {
        let mut app = app(true);
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        // El volumen general ya está al máximo.
        press(&mut app, KeyCode::ArrowRight);
        assert_eq!(sound(&app).general, MAX_STEP);
        press(&mut app, KeyCode::KeyA);
        assert_eq!(sound(&app).general, MAX_STEP - 1);
        press(&mut app, KeyCode::ArrowDown);
        for _ in 0..6 {
            press(&mut app, KeyCode::ArrowLeft);
        }
        assert_eq!(sound(&app).music, 0);
        press(&mut app, KeyCode::KeyD);
        assert_eq!(
            sound(&app),
            SoundSettings {
                general: MAX_STEP - 1,
                music: 1
            }
        );
        assert_eq!(
            lines(&mut app).0[..2],
            ["Volumen general: 90 %", "Música: 10 %"]
        );
        // En una línea que no es un volumen, Izquierda y Derecha no hacen nada.
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::ArrowRight);
        assert_eq!(sound(&app).music, 1);
        // Fuera de Sonido tampoco.
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::ArrowRight);
        assert_eq!(sound(&app).music, 1);
    }

    #[test]
    fn leaving_sonido_saves_the_volumes_and_the_next_run_reads_them() {
        let scratch = Scratch::new("saved");
        let path = scratch.0.join("settings.ron");
        let mut app = app_with(true, Some(path.clone()));
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::ArrowRight);
        assert!(!path.exists(), "saved on leaving, not on every step");
        press(&mut app, KeyCode::Escape);
        let wanted = SoundSettings {
            general: MAX_STEP,
            music: 5,
        };
        assert_eq!(SoundSettings::load(&path), (wanted, None));
        let again = app_with(true, Some(path));
        assert_eq!(sound(&again), wanted);
    }

    #[test]
    fn an_open_menu_keeps_keys_and_mouse_from_the_game() {
        let mut app = app(true);
        let world = app.world_mut();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        world.resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(5.0, 3.0);
        app.world_mut()
            .run_system_once(read_keys)
            .expect("the menu reads keys");
        let world = app.world();
        assert!(
            !world
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyW)
        );
        assert!(
            !world
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Left)
        );
        assert_eq!(world.resource::<AccumulatedMouseMotion>().delta, Vec2::ZERO);
    }

    #[test]
    fn a_closed_menu_leaves_the_game_its_keys() {
        let mut app = app(false);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::KeyW)
        );
    }

    #[test]
    fn the_menu_assets_ship_with_the_game() {
        let root = pampa_content::asset_path("");
        for path in [MUSIC, POSTER, BAND, VIGNETTE] {
            assert!(root.join(path).is_file(), "{path}");
        }
    }
}
