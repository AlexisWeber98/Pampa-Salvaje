//! Minimap in the top-right corner: the town drawn from `pampa-core` data,
//! north-up, with live markers for the player, the walkers and the train.
//!
//! The town layer is rasterized once into an image at startup; a second,
//! transparent image of the same size is redrawn every frame with the
//! markers only. Both are UI image nodes stacked inside a framed panel.
//! `M` toggles between the corner map and an enlarged, centered one.

pub mod raster;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::Vec2 as GroundVec2;

use crate::character::VisualPose;
use crate::town::WorldTown;
use crate::train::WorldTrain;
use raster::{Canvas, MapFrame, Markers, draw_markers, palette, rasterize_town};

/// Side of the map images, in pixels.
pub const MAP_PX: usize = 320;
/// Side of the corner map on screen, in logical pixels.
pub const SMALL_SIZE: Val = Val::Px(220.0);
/// Side of the enlarged map, as a share of the window's smaller side.
pub const LARGE_SIZE: Val = Val::VMin(80.0);
/// Key that toggles the enlarged map.
pub const TOGGLE_KEY: KeyCode = KeyCode::KeyM;

/// What an entity shows as on the minimap. Its ground position comes from
/// its `Transform`; the player's heading from its [`VisualPose`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapIcon {
    Player,
    Walker,
}

/// Minimap state: the framing, the marker layer and whether it is enlarged.
#[derive(Resource, Debug)]
pub struct Minimap {
    pub frame: MapFrame,
    pub enlarged: bool,
    /// Where the player's arrow was drawn last frame, in map pixels.
    pub player_px: Option<Vec2>,
    markers: Handle<Image>,
    canvas: Canvas,
}

/// Full-window layout node that places the map panel.
#[derive(Component, Debug)]
pub struct MinimapRoot;

/// The framed map panel.
#[derive(Component, Debug)]
pub struct MinimapPanel;

/// Draws the [`WorldTown`] as a minimap. Without a town it draws nothing.
pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_minimap)
            .add_systems(Update, toggle_minimap)
            .add_systems(PostUpdate, update_markers);
    }
}

fn map_image(pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: MAP_PX as u32,
            height: MAP_PX as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        // Kept on the CPU too: the marker layer is rewritten every frame.
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

fn rgba(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

fn spawn_minimap(
    mut commands: Commands,
    town: Option<Res<WorldTown>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(town) = town else {
        return;
    };
    let frame = MapFrame::for_town(&town.0, MAP_PX);
    let base = images.add(map_image(rasterize_town(&town.0, &frame).pixels));
    let canvas = Canvas::new(MAP_PX);
    let markers = images.add(map_image(canvas.pixels.clone()));
    let layer = || Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        ..default()
    };
    commands
        .spawn((Name::new("Minimap"), MinimapRoot, root_node(false)))
        .with_children(|root| {
            root.spawn((
                MinimapPanel,
                panel_node(false),
                BorderColor::all(rgba(palette::INK)),
                BackgroundColor(rgba(palette::PAPER)),
            ))
            .with_children(|panel| {
                panel.spawn((ImageNode::new(base), layer()));
                panel.spawn((ImageNode::new(markers.clone()), layer()));
            });
        });
    commands.insert_resource(Minimap {
        frame,
        enlarged: false,
        player_px: None,
        markers,
        canvas,
    });
}

/// Layout of the full-window root: the panel in the top-right corner, or
/// centered when enlarged.
fn root_node(enlarged: bool) -> Node {
    let (justify, align) = if enlarged {
        (JustifyContent::Center, AlignItems::Center)
    } else {
        (JustifyContent::FlexEnd, AlignItems::FlexStart)
    };
    Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        padding: UiRect::all(Val::Px(12.0)),
        justify_content: justify,
        align_items: align,
        ..default()
    }
}

/// The framed square panel, small or enlarged.
fn panel_node(enlarged: bool) -> Node {
    let side = if enlarged { LARGE_SIZE } else { SMALL_SIZE };
    Node {
        width: side,
        height: side,
        border: UiRect::all(Val::Px(3.0)),
        ..default()
    }
}

fn toggle_minimap(
    keys: Res<ButtonInput<KeyCode>>,
    minimap: Option<ResMut<Minimap>>,
    mut roots: Query<&mut Node, (With<MinimapRoot>, Without<MinimapPanel>)>,
    mut panels: Query<&mut Node, With<MinimapPanel>>,
) {
    let Some(mut minimap) = minimap else {
        return;
    };
    if !keys.just_pressed(TOGGLE_KEY) {
        return;
    }
    minimap.enlarged = !minimap.enlarged;
    for mut node in &mut roots {
        *node = root_node(minimap.enlarged);
    }
    for mut node in &mut panels {
        *node = panel_node(minimap.enlarged);
    }
}

/// Ground-plane position of a world-space translation.
fn ground_of(translation: Vec3) -> GroundVec2 {
    GroundVec2::new(translation.x, -translation.z)
}

fn update_markers(
    minimap: Option<ResMut<Minimap>>,
    train: Option<Res<WorldTrain>>,
    icons: Query<(&MapIcon, &Transform, Option<&VisualPose>)>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut minimap) = minimap else {
        return;
    };
    let mut markers = Markers {
        cars: train.map(|t| t.0.placed_cars()).unwrap_or_default(),
        ..default()
    };
    for (icon, transform, pose) in &icons {
        let at = ground_of(transform.translation);
        match icon {
            MapIcon::Player => {
                let heading = pose.map(|p| p.heading).unwrap_or_default();
                markers.player = Some((at, heading));
            }
            MapIcon::Walker => markers.walkers.push(at),
        }
    }
    let minimap = &mut *minimap;
    minimap.player_px = draw_markers(&mut minimap.canvas, &minimap.frame, &markers);
    if let Some(mut image) = images.get_mut(&minimap.markers) {
        image.data = Some(minimap.canvas.pixels.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::TownLayout;

    fn headless_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTown(TownLayout::castex_inspired()))
            .add_plugins(MinimapPlugin);
        app
    }

    fn panel_width(app: &mut App) -> Val {
        let mut q = app
            .world_mut()
            .query_filtered::<&Node, With<MinimapPanel>>();
        q.single(app.world()).unwrap().width
    }

    fn press(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
        keys.press(key);
    }

    #[test]
    fn m_toggles_the_enlarged_map() {
        let mut app = headless_app();
        app.update();
        assert!(!app.world().resource::<Minimap>().enlarged);
        assert_eq!(panel_width(&mut app), SMALL_SIZE);

        press(&mut app, TOGGLE_KEY);
        app.update();
        assert!(app.world().resource::<Minimap>().enlarged);
        assert_eq!(panel_width(&mut app), LARGE_SIZE);

        // Holding the key does not flicker.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        assert!(app.world().resource::<Minimap>().enlarged);

        press(&mut app, TOGGLE_KEY);
        app.update();
        assert!(!app.world().resource::<Minimap>().enlarged);
        assert_eq!(panel_width(&mut app), SMALL_SIZE);
    }

    #[test]
    fn marks_icons_and_rewrites_the_marker_layer() {
        let mut app = headless_app();
        let player = GroundVec2::new(12.0, -30.0);
        app.world_mut().spawn((
            MapIcon::Player,
            Transform::from_translation(crate::world_from_ground(player, 0.0)),
        ));
        app.update();
        let minimap = app.world().resource::<Minimap>();
        let at = minimap.player_px.expect("player marked");
        assert!(at.distance(minimap.frame.to_map(player)) < 1e-3);
        let handle = minimap.markers.clone();
        let images = app.world().resource::<Assets<Image>>();
        let data = images.get(&handle).unwrap().data.as_ref().unwrap();
        let i = (at.y as usize * MAP_PX + at.x as usize) * 4;
        assert_eq!(data[i + 3], 255, "arrow drawn into the image");
        // Base layer and marker layer.
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn without_a_town_there_is_no_map() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .add_plugins(MinimapPlugin);
        app.update();
        assert!(app.world().get_resource::<Minimap>().is_none());
    }
}
