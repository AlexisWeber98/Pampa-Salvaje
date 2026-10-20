//! Heads-up display in the top-left corner: the game clock (`HH:MM`) and,
//! under it, the current objective in one line.
//!
//! Both are lettered in the procedural bitmap [`font`](crate::font), ink on
//! the same paper panel as the minimap, and repainted only when their text
//! changes (the clock once a game minute). The clock comes from
//! [`WorldClock`]; the game sets the objective through [`HudObjective`].

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

/// What the HUD shows right now.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Hud {
    /// The clock as drawn, `HH:MM`; empty without a [`WorldClock`].
    pub clock: String,
    /// The objective as drawn, if any.
    pub objective: Option<String>,
}

/// The image node lettering the clock.
#[derive(Component, Debug)]
pub struct HudClock;

/// The image node lettering the objective.
#[derive(Component, Debug)]
pub struct HudObjectiveLine;

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
            .add_systems(Startup, spawn_hud)
            .add_systems(PostUpdate, update_hud);
    }
}

fn rgba(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
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
                panel.spawn((HudObjectiveLine, ImageNode::new(blank), line_node()));
            });
        });
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

/// Repaints the clock and the objective when their text changed.
fn update_hud(
    clock: Option<Res<WorldClock>>,
    objective: Res<HudObjective>,
    mut hud: ResMut<Hud>,
    mut images: ResMut<Assets<Image>>,
    mut clocks: Query<Line, (With<HudClock>, Without<HudObjectiveLine>)>,
    mut lines: Query<Line, With<HudObjectiveLine>>,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{ErrandStage, GameClock, TimeOfDay};

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(8, 44))))
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
        let one = paint_text("08:44", 1);
        assert_eq!(one.width, font::text_width("08:44") + 2);
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
        let three = paint_text("08:44", 3);
        assert_eq!(three.width, one.width * 3);
        assert_eq!(ink(&three), ink(&one) * 9);
        assert_ne!(paint_text("08:45", 1), one);
    }

    #[test]
    fn shows_the_game_clock_and_follows_it() {
        let mut app = app();
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "08:44");
        let (first, _) = image_of::<HudClock>(&mut app);
        let images = app.world().resource::<Assets<Image>>();
        let drawn = images.get(&first).unwrap();
        assert_eq!(
            drawn.data.as_deref(),
            Some(paint_text("08:44", CLOCK_SCALE).pixels.as_slice())
        );
        // A game minute later, the clock is repainted.
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(9, 5));
        app.update();
        assert_eq!(app.world().resource::<Hud>().clock, "09:05");
        let (second, _) = image_of::<HudClock>(&mut app);
        let images = app.world().resource::<Assets<Image>>();
        assert_eq!(
            images.get(&second).unwrap().data.as_deref(),
            Some(paint_text("09:05", CLOCK_SCALE).pixels.as_slice())
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
        let text = ErrandStage::MeetHidalgo.objective().unwrap().to_string();
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
        for stage in [MeetHidalgo, TakeHorse, ReportToFarach, Done] {
            if let Some(text) = stage.objective() {
                let bad: Vec<char> = text.chars().filter(|&c| !font::supports(c)).collect();
                assert!(bad.is_empty(), "{text}: {bad:?}");
            }
        }
        for c in "0123456789:".chars() {
            assert!(font::supports(c), "{c}");
        }
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
