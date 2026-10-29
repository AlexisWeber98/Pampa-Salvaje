//! HUD en la esquina superior izquierda: el reloj del juego (`HH:MM`), debajo
//! el objetivo actual en una línea y, mientras el jugador viaja en el tren, la
//! próxima estación y su hora de llegada.
//!
//! Todos son [`Text`] de UI rotulados en IM Fell English ([`Typefaces`]), tinta
//! sobre el mismo panel de papel que el minimapa, y se reescriben solo cuando
//! cambia su texto (el reloj, una vez por minuto de juego). El reloj viene de
//! [`WorldClock`]; el juego fija el objetivo mediante [`HudObjective`], la
//! línea del viaje mediante [`HudRide`], y una pista de tecla debajo de ella
//! mediante [`HudHint`] (la vista del viaje, "V: vista - ventana", en cursiva).
//!
//! Los anuncios de estación ("Estación Monte Nievas", "Próxima estación:
//! Eduardo Castex") se muestran en un panel del mismo estilo centrado a lo
//! largo del borde superior de la pantalla mientras [`HudAnnouncement`] tiene
//! texto.

use bevy::prelude::*;

use crate::atmosphere::WorldClock;
use crate::minimap::raster::palette;
use crate::typeface::Typefaces;

/// Tamaño de fuente del reloj, en píxeles lógicos.
pub const CLOCK_PX: f32 = 30.0;
/// Tamaño de fuente del objetivo, la línea del viaje, la pista de tecla y los
/// paneles que siguen el estilo del HUD (horario, consola).
pub const LINE_PX: f32 = 20.0;
/// Tamaño de fuente del anuncio, en píxeles lógicos.
pub const ANNOUNCEMENT_PX: f32 = 28.0;

/// El objetivo de una línea debajo del reloj; `None` lo oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudObjective(pub Option<String>);

/// La línea del viaje debajo del objetivo (próxima estación y hora de llegada);
/// `None` la oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudRide(pub Option<String>);

/// Una pista de tecla de una línea debajo de la línea del viaje; `None` la
/// oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudHint(pub Option<String>);

/// El nodo de texto que rotula la pista de tecla.
#[derive(Component, Debug)]
pub struct HudHintLine;

/// El anuncio centrado en la parte superior de la pantalla; `None` lo oculta.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudAnnouncement(pub Option<String>);

/// El panel que enmarca el anuncio.
#[derive(Component, Debug)]
pub struct HudAnnouncementPanel;

/// El nodo de texto que rotula el anuncio.
#[derive(Component, Debug)]
pub struct HudAnnouncementLine;

/// Lo que muestra el HUD en este momento.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Hud {
    /// El reloj tal como se dibuja, `HH:MM`; vacío sin un [`WorldClock`].
    pub clock: String,
    /// El objetivo tal como se dibuja, si lo hay.
    pub objective: Option<String>,
    /// La línea del viaje tal como se dibuja, si la hay.
    pub ride: Option<String>,
    /// El anuncio tal como se dibuja, si lo hay.
    pub announcement: Option<String>,
    /// La pista de tecla tal como se dibuja, si la hay.
    pub hint: Option<String>,
}

/// El nodo de texto que rotula el reloj.
#[derive(Component, Debug)]
pub struct HudClock;

/// El nodo de texto que rotula el objetivo.
#[derive(Component, Debug)]
pub struct HudObjectiveLine;

/// El nodo de texto que rotula la línea del viaje.
#[derive(Component, Debug)]
pub struct HudRideLine;

/// Muestra el reloj y el objetivo.
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hud>()
            .init_resource::<Typefaces>()
            .init_resource::<HudObjective>()
            .init_resource::<HudRide>()
            .init_resource::<HudAnnouncement>()
            .init_resource::<HudHint>()
            .add_systems(Startup, (spawn_hud, spawn_announcement))
            .add_systems(PostUpdate, (update_hud, update_announcement, update_hint));
    }
}

fn rgba(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// Los colores de un panel del HUD, para otros paneles del mismo estilo: un
/// borde de tinta sobre papel.
pub fn panel_colors() -> (BorderColor, BackgroundColor) {
    (
        BorderColor::all(rgba(palette::INK)),
        BackgroundColor(rgba(palette::PAPER)),
    )
}

/// La fuente y el color de una línea de texto del HUD: `face` a `px` píxeles
/// lógicos, en tinta, para otros paneles del mismo estilo.
pub fn ink_font(face: &Handle<Font>, px: f32) -> (TextFont, TextColor) {
    (
        TextFont {
            font: face.into(),
            font_size: FontSize::Px(px),
            ..default()
        },
        TextColor(rgba(palette::INK)),
    )
}

/// Una línea oculta, que se muestra una vez que tiene texto.
fn line_node() -> Node {
    Node {
        display: Display::None,
        ..default()
    }
}

/// Una línea de texto en tinta, oculta y vacía.
fn line(face: &Handle<Font>, px: f32) -> impl Bundle {
    (Text::default(), ink_font(face, px), line_node())
}

fn spawn_hud(mut commands: Commands, faces: Res<Typefaces>) {
    commands
        .spawn((
            Name::new("HUD"),
            Node {
                position_type: PositionType::Absolute,
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    border: UiRect::all(Val::Px(3.0)),
                    padding: UiRect::all(Val::Px(6.0)),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                BorderColor::all(rgba(palette::INK)),
                BackgroundColor(rgba(palette::PAPER)),
            ))
            .with_children(|panel| {
                panel.spawn((HudClock, line(&faces.body, CLOCK_PX)));
                panel.spawn((HudObjectiveLine, line(&faces.body, LINE_PX)));
                panel.spawn((HudRideLine, line(&faces.body, LINE_PX)));
                panel.spawn((HudHintLine, line(&faces.italic, LINE_PX)));
            });
        });
}

/// Reescribe la pista de tecla cuando su texto cambió.
fn update_hint(
    hint: Res<HudHint>,
    mut hud: ResMut<Hud>,
    mut lines: Query<Line, With<HudHintLine>>,
) {
    if hint.0 == hud.hint {
        return;
    }
    let text = hint.0.clone().unwrap_or_default();
    for (mut line, mut node) in &mut lines {
        set_line(&text, &mut line, &mut node);
    }
    hud.hint = hint.0.clone();
}

/// El panel del anuncio, oculto y centrado a lo largo del borde superior.
fn spawn_announcement(mut commands: Commands, faces: Res<Typefaces>) {
    commands
        .spawn((
            Name::new("Announcement"),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                top: Val::Px(12.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                HudAnnouncementPanel,
                Node {
                    display: Display::None,
                    border: UiRect::all(Val::Px(3.0)),
                    padding: UiRect::all(Val::Px(8.0)),
                    ..default()
                },
                BorderColor::all(rgba(palette::INK)),
                BackgroundColor(rgba(palette::PAPER)),
            ))
            .with_children(|panel| {
                panel.spawn((HudAnnouncementLine, line(&faces.body, ANNOUNCEMENT_PX)));
            });
        });
}

/// Reescribe el anuncio y muestra su panel mientras tiene texto.
fn update_announcement(
    announcement: Res<HudAnnouncement>,
    mut hud: ResMut<Hud>,
    mut lines: Query<Line, (With<HudAnnouncementLine>, Without<HudAnnouncementPanel>)>,
    mut panels: Query<&mut Node, (With<HudAnnouncementPanel>, Without<HudAnnouncementLine>)>,
) {
    if announcement.0 == hud.announcement {
        return;
    }
    let text = announcement.0.clone().unwrap_or_default();
    for (mut line, mut node) in &mut lines {
        set_line(&text, &mut line, &mut node);
    }
    for mut panel in &mut panels {
        panel.display = if text.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
    }
    hud.announcement = announcement.0.clone();
}

/// Pone `text` en una línea; un texto vacío oculta la línea.
fn set_line(text: &str, line: &mut Text, node: &mut Node) {
    node.display = if text.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    line.0 = text.to_string();
}

/// Una línea de texto del HUD: su texto y su disposición.
type Line<'a> = (&'a mut Text, &'a mut Node);

/// Las líneas de texto del HUD: el reloj, el objetivo y la línea del viaje.
type Lines<'w, 's> = (
    Query<
        'w,
        's,
        Line<'static>,
        (
            With<HudClock>,
            Without<HudObjectiveLine>,
            Without<HudRideLine>,
        ),
    >,
    Query<'w, 's, Line<'static>, (With<HudObjectiveLine>, Without<HudRideLine>)>,
    Query<'w, 's, Line<'static>, With<HudRideLine>>,
);

/// Reescribe el reloj, el objetivo y la línea del viaje cuando su texto cambió.
fn update_hud(
    clock: Option<Res<WorldClock>>,
    (objective, ride): (Res<HudObjective>, Res<HudRide>),
    mut hud: ResMut<Hud>,
    (mut clocks, mut lines, mut rides): Lines,
) {
    let time = clock.map(|c| c.0.hh_mm()).unwrap_or_default();
    if time != hud.clock {
        for (mut line, mut node) in &mut clocks {
            set_line(&time, &mut line, &mut node);
        }
        hud.clock = time;
    }
    if objective.0 != hud.objective {
        let text = objective.0.clone().unwrap_or_default();
        for (mut line, mut node) in &mut lines {
            set_line(&text, &mut line, &mut node);
        }
        hud.objective = objective.0.clone();
    }
    if ride.0 != hud.ride {
        let text = ride.0.clone().unwrap_or_default();
        for (mut line, mut node) in &mut rides {
            set_line(&text, &mut line, &mut node);
        }
        hud.ride = ride.0.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lettering::{self, Face};
    use bevy::asset::uuid_handle;
    use pampa_core::{ErrandStage, GameClock, TimeOfDay};

    fn body() -> Handle<Font> {
        uuid_handle!("00000000-0000-0000-0000-000000000001")
    }

    fn italic() -> Handle<Font> {
        uuid_handle!("00000000-0000-0000-0000-000000000002")
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Typefaces {
                display: uuid_handle!("00000000-0000-0000-0000-000000000000"),
                body: body(),
                italic: italic(),
            })
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(10, 49))))
            .add_plugins(HudPlugin);
        app
    }

    /// Lo que muestra una línea: su texto, su visualización, su fuente y su
    /// color.
    fn line_of<T: Component>(app: &mut App) -> (String, Display, TextFont, Color) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Text, &Node, &TextFont, &TextColor), With<T>>();
        let (text, node, font, color) = q.single(app.world()).unwrap();
        (text.0.clone(), node.display, font.clone(), color.0)
    }

    fn lettered(face: Handle<Font>, px: f32) -> TextFont {
        TextFont {
            font: (&face).into(),
            font_size: FontSize::Px(px),
            ..default()
        }
    }

    #[test]
    fn shows_the_game_clock_and_follows_it() {
        let mut app = app();
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "10:49");
        let (text, display, font, color) = line_of::<HudClock>(&mut app);
        assert_eq!(text, "10:49");
        assert_eq!(display, Display::Flex);
        assert_eq!(font, lettered(body(), 30.0));
        assert_eq!(color, rgba(palette::INK), "ink");
        // Más tarde, el reloj se vuelve a rotular.
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(11, 5));
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "11:05");
        assert_eq!(line_of::<HudClock>(&mut app).0, "11:05");
        // El texto sin cambios no se toca: su tick de cambio no se mueve.
        let changed = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<Ref<Text>, With<HudClock>>();
            q.single(app.world()).unwrap().last_changed().get()
        };
        let before = changed(&mut app);
        app.update();
        app.update();
        assert_eq!(changed(&mut app), before);
    }

    #[test]
    fn shows_the_objective_line_only_while_there_is_one() {
        let mut app = app();
        app.update();
        assert_eq!(line_of::<HudObjectiveLine>(&mut app).1, Display::None);
        let text = ErrandStage::MeetHidalgo
            .objective("Eduardo Castex")
            .unwrap();
        app.insert_resource(HudObjective(Some(text.clone())));
        app.update();
        assert_eq!(app.world().resource::<Hud>().objective, Some(text.clone()));
        let (shown, display, font, color) = line_of::<HudObjectiveLine>(&mut app);
        assert_eq!(shown, text);
        assert_eq!(display, Display::Flex);
        assert_eq!(font, lettered(body(), 20.0));
        assert_eq!(color, rgba(palette::INK));
        app.insert_resource(HudObjective(None));
        app.update();
        assert_eq!(app.world().resource::<Hud>().objective, None);
        assert_eq!(line_of::<HudObjectiveLine>(&mut app).1, Display::None);
    }

    #[test]
    fn every_objective_and_clock_reading_uses_only_characters_the_typeface_draws() {
        use ErrandStage::*;
        let towns = ["Eduardo Castex", "Monte Nievas", "General Pico"];
        for (stage, town) in [MeetHidalgo, TakeHorse, ReportToFarach, Done]
            .into_iter()
            .flat_map(|stage| towns.map(|town| (stage, town)))
        {
            if let Some(text) = stage.objective(town) {
                let bad: Vec<char> = text
                    .chars()
                    .filter(|&c| !lettering::supports(Face::Body, c))
                    .collect();
                assert!(bad.is_empty(), "{text}: {bad:?}");
            }
        }
        for c in "0123456789:".chars() {
            assert!(lettering::supports(Face::Body, c), "{c}");
        }
    }

    #[test]
    fn shows_the_next_station_and_its_time_only_while_riding() {
        let mut app = app();
        app.update();
        assert_eq!(line_of::<HudRideLine>(&mut app).1, Display::None);
        let text = "Próxima: Eduardo Castex, 11:15".to_string();
        assert!(text.chars().all(|c| lettering::supports(Face::Body, c)));
        app.insert_resource(HudRide(Some(text.clone())));
        app.update();
        assert_eq!(app.world().resource::<Hud>().ride, Some(text.clone()));
        let (shown, display, font, _) = line_of::<HudRideLine>(&mut app);
        assert_eq!((shown, display), (text, Display::Flex));
        assert_eq!(font, lettered(body(), 20.0));
        // La línea del objetivo no se toca.
        assert_eq!(line_of::<HudObjectiveLine>(&mut app).1, Display::None);
        app.insert_resource(HudRide(None));
        app.update();
        assert_eq!(line_of::<HudRideLine>(&mut app).1, Display::None);
    }

    #[test]
    fn shows_the_station_announcement_in_its_own_panel_while_there_is_one() {
        let mut app = app();
        app.update();
        let panel = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&Node, With<HudAnnouncementPanel>>();
            q.single(app.world()).unwrap().display
        };
        assert_eq!(panel(&mut app), Display::None);
        for text in ["Estación Monte Nievas", "Próxima estación: Eduardo Castex"] {
            assert!(
                text.chars().all(|c| lettering::supports(Face::Body, c)),
                "{text}"
            );
            app.insert_resource(HudAnnouncement(Some(text.into())));
            app.update();
            assert_eq!(panel(&mut app), Display::Flex);
            let (shown, display, font, color) = line_of::<HudAnnouncementLine>(&mut app);
            assert_eq!((shown.as_str(), display), (text, Display::Flex));
            assert_eq!(font, lettered(body(), 28.0));
            assert_eq!(color, rgba(palette::INK));
        }
        app.insert_resource(HudAnnouncement(None));
        app.update();
        assert_eq!(panel(&mut app), Display::None);
        assert_eq!(line_of::<HudAnnouncementLine>(&mut app).1, Display::None);
    }

    #[test]
    fn shows_the_key_hint_in_italics_while_there_is_one() {
        let mut app = app();
        app.update();
        assert_eq!(line_of::<HudHintLine>(&mut app).1, Display::None);
        for view in ["ventana", "exterior", "tren"] {
            let text = format!("V: vista - {view}");
            assert!(
                text.chars().all(|c| lettering::supports(Face::Italic, c)),
                "{text}"
            );
            app.insert_resource(HudHint(Some(text.clone())));
            app.update();
            assert_eq!(app.world().resource::<Hud>().hint, Some(text.clone()));
            let (shown, display, font, _) = line_of::<HudHintLine>(&mut app);
            assert_eq!((shown, display), (text, Display::Flex));
            assert_eq!(font, lettered(italic(), 20.0));
        }
        app.insert_resource(HudHint(None));
        app.update();
        assert_eq!(line_of::<HudHintLine>(&mut app).1, Display::None);
    }

    #[test]
    fn without_a_clock_the_clock_line_is_empty() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(HudPlugin);
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "");
        assert_eq!(line_of::<HudClock>(&mut app).1, Display::None);
    }
}
