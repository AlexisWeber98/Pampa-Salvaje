//! Heads-up display in the top-left corner: the game clock (`HH:MM`),
//! under it the current objective in one line, and while the player rides
//! the train the next station and its arrival time.
//!
//! All are UI [`Text`] lettered in IM Fell English ([`Typefaces`]), ink on
//! the same paper panel as the minimap, and rewritten only when their text
//! changes (the clock once a game minute). The clock comes from
//! [`WorldClock`]; the game sets the objective through [`HudObjective`] and
//! the ride line through [`HudRide`], and a key hint under it through
//! [`HudHint`] (the riding view, "V: vista - ventana", in italics).
//!
//! Station announcements ("Estación Monte Nievas", "Próxima estación:
//! Eduardo Castex") show in a panel of the same style centered along the
//! top of the screen while [`HudAnnouncement`] has text.

use bevy::prelude::*;

use crate::atmosphere::WorldClock;
use crate::minimap::raster::palette;
use crate::typeface::Typefaces;

/// Font size of the clock, in logical pixels.
pub const CLOCK_PX: f32 = 30.0;
/// Font size of the objective, the ride line, the key hint and of the
/// panels that follow the HUD's style (timetable, console).
pub const LINE_PX: f32 = 20.0;
/// Font size of the announcement, in logical pixels.
pub const ANNOUNCEMENT_PX: f32 = 28.0;

/// The one-line objective under the clock; `None` hides it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudObjective(pub Option<String>);

/// The ride line under the objective (next station and arrival time);
/// `None` hides it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudRide(pub Option<String>);

/// A one-line key hint under the ride line; `None` hides it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudHint(pub Option<String>);

/// The text node lettering the key hint.
#[derive(Component, Debug)]
pub struct HudHintLine;

/// The announcement centered at the top of the screen; `None` hides it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudAnnouncement(pub Option<String>);

/// The panel framing the announcement.
#[derive(Component, Debug)]
pub struct HudAnnouncementPanel;

/// The text node lettering the announcement.
#[derive(Component, Debug)]
pub struct HudAnnouncementLine;

/// What the HUD shows right now.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Hud {
    /// The clock as drawn, `HH:MM`; empty without a [`WorldClock`].
    pub clock: String,
    /// The objective as drawn, if any.
    pub objective: Option<String>,
    /// The ride line as drawn, if any.
    pub ride: Option<String>,
    /// The announcement as drawn, if any.
    pub announcement: Option<String>,
    /// The key hint as drawn, if any.
    pub hint: Option<String>,
}

/// The text node lettering the clock.
#[derive(Component, Debug)]
pub struct HudClock;

/// The text node lettering the objective.
#[derive(Component, Debug)]
pub struct HudObjectiveLine;

/// The text node lettering the ride line.
#[derive(Component, Debug)]
pub struct HudRideLine;

/// Shows the clock and the objective.
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

/// The colors of a HUD panel, for other panels in the same style: an ink
/// border on paper.
pub fn panel_colors() -> (BorderColor, BackgroundColor) {
    (
        BorderColor::all(rgba(palette::INK)),
        BackgroundColor(rgba(palette::PAPER)),
    )
}

/// The font and color of a line of HUD text: `face` at `px` logical pixels,
/// in ink, for other panels in the same style.
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

/// A hidden line, shown once it has text.
fn line_node() -> Node {
    Node {
        display: Display::None,
        ..default()
    }
}

/// A hidden, empty line of ink text.
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

/// Rewrites the key hint when its text changed.
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

/// The announcement panel, hidden, centered along the top edge.
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

/// Rewrites the announcement and shows its panel while it has text.
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

/// Sets `text` on a line; empty text hides the line.
fn set_line(text: &str, line: &mut Text, node: &mut Node) {
    node.display = if text.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    line.0 = text.to_string();
}

/// One text line of the HUD: its text and its layout.
type Line<'a> = (&'a mut Text, &'a mut Node);

/// The HUD's text lines: the clock, the objective and the ride line.
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

/// Rewrites the clock, the objective and the ride line when their text
/// changed.
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

    /// What a line shows: its text, its display, its font and its color.
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
        // Later, the clock is relettered.
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(11, 5));
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "11:05");
        assert_eq!(line_of::<HudClock>(&mut app).0, "11:05");
        // Unchanged text is not touched: its change tick stays put.
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
        // The objective line is untouched.
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
