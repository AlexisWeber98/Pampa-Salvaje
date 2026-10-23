//! Heads-up display in the top-left corner: the game clock (`HH:MM`),
//! under it the current objective in one line, and while the player rides
//! the train the next station and its arrival time.
//!
//! All are lettered in the procedural bitmap [`font`](crate::font), ink on
//! the same paper panel as the minimap, and repainted only when their text
//! changes (the clock once a game minute). The clock comes from
//! [`WorldClock`]; the game sets the objective through [`HudObjective`] and
//! the ride line through [`HudRide`], and a key hint under it through
//! [`HudHint`] (the riding view, "V: vista - ventana").
//!
//! Station announcements ("Estación Monte Nievas", "Próxima estación:
//! Eduardo Castex") show in a panel of the same style centered along the
//! top of the screen while [`HudAnnouncement`] has text.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::atmosphere::WorldClock;
use crate::font::{self, GLYPH_HEIGHT};
use crate::minimap::raster::palette;

/// Pixel scale of the clock's lettering.
pub const CLOCK_SCALE: usize = 3;
/// Pixel scale of the objective's lettering.
pub const OBJECTIVE_SCALE: usize = 2;

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

/// The image node lettering the key hint.
#[derive(Component, Debug)]
pub struct HudHintLine;

/// The announcement centered at the top of the screen; `None` hides it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct HudAnnouncement(pub Option<String>);

/// Pixel scale of the announcement's lettering.
pub const ANNOUNCEMENT_SCALE: usize = 3;

/// The panel framing the announcement.
#[derive(Component, Debug)]
pub struct HudAnnouncementPanel;

/// The image node lettering the announcement.
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

/// The image node lettering the clock.
#[derive(Component, Debug)]
pub struct HudClock;

/// The image node lettering the objective.
#[derive(Component, Debug)]
pub struct HudObjectiveLine;

/// The image node lettering the ride line.
#[derive(Component, Debug)]
pub struct HudRideLine;

/// A lettered line: ink on a transparent background.
#[derive(Debug, Clone, PartialEq)]
pub struct TextImage {
    pub width: usize,
    pub height: usize,
    /// RGBA8 sRGB pixels, row by row from the top.
    pub pixels: Vec<u8>,
}

/// Letters `text` in ink at pixel `scale`, cropped to its glyph cells (one
/// pixel of room all round). Characters the font lacks draw nothing.
pub fn paint_text(text: &str, scale: usize) -> TextImage {
    let scale = scale.max(1);
    let width = (font::text_width(text) + 2) * scale;
    let height = (GLYPH_HEIGHT + 2) * scale;
    let mut pixels = vec![0; width * height * 4];
    font::for_each_pixel(text, |x, y| {
        for dy in 0..scale {
            for dx in 0..scale {
                let (px, py) = ((x + 1) * scale + dx, (y + 1) * scale + dy);
                let i = (py * width + px) * 4;
                pixels[i..i + 4].copy_from_slice(&palette::INK);
            }
        }
    });
    TextImage {
        width,
        height,
        pixels,
    }
}

impl TextImage {
    fn image(&self) -> Image {
        let mut image = Image::new(
            Extent3d {
                width: self.width as u32,
                height: self.height as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.pixels.clone(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        // Crisp pixel lettering.
        image.sampler = ImageSampler::nearest();
        image
    }
}

/// Shows the clock and the objective.
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hud>()
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

/// `text` lettered in ink at pixel `scale` as an image for a UI node, like
/// the HUD's own lines.
pub fn text_image(text: &str, scale: usize) -> Image {
    paint_text(text, scale).image()
}

/// A hidden line, shown once it has text.
fn line_node() -> Node {
    Node {
        display: Display::None,
        ..default()
    }
}

fn spawn_hud(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let blank = images.add(paint_text("", 1).image());
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
                panel.spawn((HudClock, ImageNode::new(blank.clone()), line_node()));
                panel.spawn((HudObjectiveLine, ImageNode::new(blank.clone()), line_node()));
                panel.spawn((HudRideLine, ImageNode::new(blank.clone()), line_node()));
                panel.spawn((HudHintLine, ImageNode::new(blank), line_node()));
            });
        });
}

/// Repaints the key hint when its text changed.
fn update_hint(
    hint: Res<HudHint>,
    mut hud: ResMut<Hud>,
    mut images: ResMut<Assets<Image>>,
    mut lines: Query<Line, With<HudHintLine>>,
) {
    if hint.0 == hud.hint {
        return;
    }
    let text = hint.0.clone().unwrap_or_default();
    for (mut image, mut node) in &mut lines {
        letter(&text, OBJECTIVE_SCALE, (&mut image, &mut node), &mut images);
    }
    hud.hint = hint.0.clone();
}

/// The announcement panel, hidden, centered along the top edge.
fn spawn_announcement(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let blank = images.add(paint_text("", 1).image());
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
                panel.spawn((HudAnnouncementLine, ImageNode::new(blank), line_node()));
            });
        });
}

/// Repaints the announcement and shows its panel while it has text.
fn update_announcement(
    announcement: Res<HudAnnouncement>,
    mut hud: ResMut<Hud>,
    mut images: ResMut<Assets<Image>>,
    mut lines: Query<Line, (With<HudAnnouncementLine>, Without<HudAnnouncementPanel>)>,
    mut panels: Query<&mut Node, (With<HudAnnouncementPanel>, Without<HudAnnouncementLine>)>,
) {
    if announcement.0 == hud.announcement {
        return;
    }
    let text = announcement.0.clone().unwrap_or_default();
    for (mut image, mut node) in &mut lines {
        letter(
            &text,
            ANNOUNCEMENT_SCALE,
            (&mut image, &mut node),
            &mut images,
        );
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

/// Letters `text` into `node`'s image at `scale` (a fresh image, the old
/// one dropped), sized to its pixels; empty text hides the line.
fn letter(
    text: &str,
    scale: usize,
    (image, node): (&mut ImageNode, &mut Node),
    images: &mut Assets<Image>,
) {
    if text.is_empty() {
        node.display = Display::None;
        return;
    }
    let painted = paint_text(text, scale);
    let old = std::mem::replace(&mut image.image, images.add(painted.image()));
    images.remove(&old);
    node.display = Display::Flex;
    node.width = Val::Px(painted.width as f32);
    node.height = Val::Px(painted.height as f32);
}

/// One lettered line of the HUD: its image and its layout.
type Line<'a> = (&'a mut ImageNode, &'a mut Node);

/// The HUD's lettered lines: the clock, the objective and the ride line.
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

/// Repaints the clock, the objective and the ride line when their text
/// changed.
fn update_hud(
    clock: Option<Res<WorldClock>>,
    (objective, ride): (Res<HudObjective>, Res<HudRide>),
    mut hud: ResMut<Hud>,
    mut images: ResMut<Assets<Image>>,
    (mut clocks, mut lines, mut rides): Lines,
) {
    let time = clock.map(|c| c.0.hh_mm()).unwrap_or_default();
    if time != hud.clock {
        for (mut image, mut node) in &mut clocks {
            letter(&time, CLOCK_SCALE, (&mut image, &mut node), &mut images);
        }
        hud.clock = time;
    }
    if objective.0 != hud.objective {
        let text = objective.0.clone().unwrap_or_default();
        for (mut image, mut node) in &mut lines {
            letter(&text, OBJECTIVE_SCALE, (&mut image, &mut node), &mut images);
        }
        hud.objective = objective.0.clone();
    }
    if ride.0 != hud.ride {
        let text = ride.0.clone().unwrap_or_default();
        for (mut image, mut node) in &mut rides {
            letter(&text, OBJECTIVE_SCALE, (&mut image, &mut node), &mut images);
        }
        hud.ride = ride.0.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{ErrandStage, GameClock, TimeOfDay};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(10, 49))))
            .add_plugins(HudPlugin);
        app
    }

    fn image_of<T: Component>(app: &mut App) -> (Handle<Image>, Display) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&ImageNode, &Node), With<T>>();
        let (image, node) = q.single(app.world()).unwrap();
        (image.image.clone(), node.display)
    }

    #[test]
    fn letters_text_in_ink_at_its_scale() {
        let one = paint_text("10:49", 1);
        assert_eq!(one.width, font::text_width("10:49") + 2);
        assert_eq!(one.height, GLYPH_HEIGHT + 2);
        assert_eq!(one.pixels.len(), one.width * one.height * 4);
        let ink = |t: &TextImage| t.pixels.chunks(4).filter(|p| *p == palette::INK).count();
        assert!(ink(&one) > 20);
        assert!(
            one.pixels
                .chunks(4)
                .all(|p| p[3] == 0 || *p == palette::INK),
            "ink on nothing"
        );
        let three = paint_text("10:49", 3);
        assert_eq!(three.width, one.width * 3);
        assert_eq!(ink(&three), ink(&one) * 9);
        assert_ne!(paint_text("10:50", 1), one);
    }

    #[test]
    fn shows_the_game_clock_and_follows_it() {
        let mut app = app();
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "10:49");
        let (first, _) = image_of::<HudClock>(&mut app);
        let images = app.world().resource::<Assets<Image>>();
        let drawn = images.get(&first).unwrap();
        assert_eq!(
            drawn.data.as_deref(),
            Some(paint_text("10:49", CLOCK_SCALE).pixels.as_slice())
        );
        // Later, the clock is repainted.
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(11, 5));
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "11:05");
        let (second, _) = image_of::<HudClock>(&mut app);
        let images = app.world().resource::<Assets<Image>>();
        assert_eq!(
            images.get(&second).unwrap().data.as_deref(),
            Some(paint_text("11:05", CLOCK_SCALE).pixels.as_slice())
        );
        // Unchanged text is not repainted.
        let count = app.world().resource::<Assets<Image>>().len();
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), count);
    }

    #[test]
    fn shows_the_objective_line_only_while_there_is_one() {
        let mut app = app();
        app.update();
        assert_eq!(image_of::<HudObjectiveLine>(&mut app).1, Display::None);
        let text = ErrandStage::MeetHidalgo
            .objective("Eduardo Castex")
            .unwrap();
        app.insert_resource(HudObjective(Some(text.clone())));
        app.update();
        assert_eq!(app.world().resource::<Hud>().objective, Some(text.clone()));
        let (handle, display) = image_of::<HudObjectiveLine>(&mut app);
        assert_eq!(display, Display::Flex);
        let images = app.world().resource::<Assets<Image>>();
        assert_eq!(
            images.get(&handle).unwrap().data.as_deref(),
            Some(paint_text(&text, OBJECTIVE_SCALE).pixels.as_slice())
        );
        app.insert_resource(HudObjective(None));
        app.update();
        assert_eq!(app.world().resource::<Hud>().objective, None);
        assert_eq!(image_of::<HudObjectiveLine>(&mut app).1, Display::None);
    }

    #[test]
    fn every_objective_and_clock_reading_uses_only_characters_the_font_draws() {
        use ErrandStage::*;
        let towns = ["Eduardo Castex", "Monte Nievas", "General Pico"];
        for (stage, town) in [MeetHidalgo, TakeHorse, ReportToFarach, Done]
            .into_iter()
            .flat_map(|stage| towns.map(|town| (stage, town)))
        {
            if let Some(text) = stage.objective(town) {
                let bad: Vec<char> = text.chars().filter(|&c| !font::supports(c)).collect();
                assert!(bad.is_empty(), "{text}: {bad:?}");
            }
        }
        for c in "0123456789:".chars() {
            assert!(font::supports(c), "{c}");
        }
    }

    #[test]
    fn shows_the_next_station_and_its_time_only_while_riding() {
        let mut app = app();
        app.update();
        assert_eq!(image_of::<HudRideLine>(&mut app).1, Display::None);
        let text = "Próxima: Eduardo Castex, 11:15".to_string();
        assert!(text.chars().all(font::supports));
        app.insert_resource(HudRide(Some(text.clone())));
        app.update();
        assert_eq!(app.world().resource::<Hud>().ride, Some(text.clone()));
        let (handle, display) = image_of::<HudRideLine>(&mut app);
        assert_eq!(display, Display::Flex);
        let images = app.world().resource::<Assets<Image>>();
        assert_eq!(
            images.get(&handle).unwrap().data.as_deref(),
            Some(paint_text(&text, OBJECTIVE_SCALE).pixels.as_slice())
        );
        // The objective line is untouched.
        assert_eq!(image_of::<HudObjectiveLine>(&mut app).1, Display::None);
        app.insert_resource(HudRide(None));
        app.update();
        assert_eq!(image_of::<HudRideLine>(&mut app).1, Display::None);
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
            assert!(text.chars().all(font::supports), "{text}");
            app.insert_resource(HudAnnouncement(Some(text.into())));
            app.update();
            assert_eq!(panel(&mut app), Display::Flex);
            let (handle, display) = image_of::<HudAnnouncementLine>(&mut app);
            assert_eq!(display, Display::Flex);
            let images = app.world().resource::<Assets<Image>>();
            assert_eq!(
                images.get(&handle).unwrap().data.as_deref(),
                Some(paint_text(text, ANNOUNCEMENT_SCALE).pixels.as_slice())
            );
        }
        app.insert_resource(HudAnnouncement(None));
        app.update();
        assert_eq!(panel(&mut app), Display::None);
    }

    #[test]
    fn shows_the_key_hint_while_there_is_one() {
        let mut app = app();
        app.update();
        assert_eq!(image_of::<HudHintLine>(&mut app).1, Display::None);
        for view in ["ventana", "exterior", "tren"] {
            let text = format!("V: vista - {view}");
            assert!(text.chars().all(font::supports), "{text}");
            app.insert_resource(HudHint(Some(text.clone())));
            app.update();
            assert_eq!(app.world().resource::<Hud>().hint, Some(text));
            assert_eq!(image_of::<HudHintLine>(&mut app).1, Display::Flex);
        }
        app.insert_resource(HudHint(None));
        app.update();
        assert_eq!(image_of::<HudHintLine>(&mut app).1, Display::None);
    }

    #[test]
    fn without_a_clock_the_clock_line_is_empty() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .add_plugins(HudPlugin);
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "");
        assert_eq!(image_of::<HudClock>(&mut app).1, Display::None);
    }
}
