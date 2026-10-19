//! Minimap in the top-right corner: the town nearest the player (by
//! distance to its extent) drawn from `pampa-core` data, north-up, titled
//! with its name, with live markers for the player, the walkers and the
//! town's train.
//!
//! Each town's layer is rasterized once, the first time it is shown, and
//! kept per town (one image per town: bounded); a second, transparent image
//! of the same size is redrawn every frame with the markers only. Both are
//! UI image nodes stacked inside a framed panel. `M` toggles between the
//! corner map and an enlarged, centered one.

pub mod raster;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use std::collections::HashMap;

use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{Vec2 as GroundVec2, nearest_town};

use crate::character::VisualPose;
use crate::terrain::StreamFocus;
use crate::town::WorldTowns;
use crate::train::WorldTrains;
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

/// Minimap state: the town shown and its framing, the marker layer and
/// whether it is enlarged.
#[derive(Resource, Debug)]
pub struct Minimap {
    /// Index of the town shown, in [`WorldTowns`].
    pub town: usize,
    pub frame: MapFrame,
    pub enlarged: bool,
    /// Where the player's arrow was drawn last frame, in map pixels.
    pub player_px: Option<Vec2>,
    markers: Handle<Image>,
    canvas: Canvas,
    /// Each shown town's layer, by town index.
    bases: HashMap<usize, Handle<Image>>,
}

/// The image node showing the town layer.
#[derive(Component, Debug)]
pub struct MinimapBase;

/// Full-window layout node that places the map panel.
#[derive(Component, Debug)]
pub struct MinimapRoot;

/// The framed map panel.
#[derive(Component, Debug)]
pub struct MinimapPanel;

/// Draws the nearest of the [`WorldTowns`] as a minimap. Without towns it
/// draws nothing.
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
    towns: Option<Res<WorldTowns>>,
    focus: Option<Res<StreamFocus>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(towns) = towns else {
        return;
    };
    let at = focus.map(|f| f.0).unwrap_or_default();
    let Some(index) = nearest_town(&towns.0, at) else {
        return;
    };
    let town = &towns.0[index];
    let frame = MapFrame::for_town(town, MAP_PX);
    let base = images.add(map_image(rasterize_town(town, &frame).pixels));
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
                panel.spawn((MinimapBase, ImageNode::new(base.clone()), layer()));
                panel.spawn((ImageNode::new(markers.clone()), layer()));
            });
        });
    commands.insert_resource(Minimap {
        town: index,
        frame,
        enlarged: false,
        player_px: None,
        markers,
        canvas,
        bases: HashMap::from([(index, base)]),
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

/// Switches the map to town `index`, rasterizing its layer the first time.
fn show_town(
    minimap: &mut Minimap,
    towns: &WorldTowns,
    index: usize,
    images: &mut Assets<Image>,
    bases: &mut Query<&mut ImageNode, With<MinimapBase>>,
) {
    let Some(town) = towns.0.get(index) else {
        return;
    };
    minimap.town = index;
    minimap.frame = MapFrame::for_town(town, MAP_PX);
    let frame = minimap.frame;
    let base = minimap
        .bases
        .entry(index)
        .or_insert_with(|| images.add(map_image(rasterize_town(town, &frame).pixels)))
        .clone();
    for mut node in bases {
        node.image = base.clone();
    }
}

fn update_markers(
    minimap: Option<ResMut<Minimap>>,
    (towns, trains): (Option<Res<WorldTowns>>, Option<Res<WorldTrains>>),
    icons: Query<(&MapIcon, &Transform, Option<&VisualPose>)>,
    mut bases: Query<&mut ImageNode, With<MinimapBase>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut minimap) = minimap else {
        return;
    };
    let player = icons
        .iter()
        .find(|(icon, _, _)| **icon == MapIcon::Player)
        .map(|(_, transform, _)| ground_of(transform.translation));
    if let (Some(towns), Some(at)) = (&towns, player)
        && let Some(nearest) = nearest_town(&towns.0, at)
        && nearest != minimap.town
    {
        show_town(&mut minimap, towns, nearest, &mut images, &mut bases);
    }
    let train = trains.and_then(|t| t.0.get(minimap.town).cloned().flatten());
    let mut markers = Markers {
        cars: train.map(|t| t.placed_cars()).unwrap_or_default(),
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
    use pampa_core::{TownConfig, TownLayout};

    fn headless_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTowns(vec![TownLayout::castex_inspired()]))
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
    fn the_map_follows_the_player_to_the_nearest_town() {
        let far = TownLayout::generate(&TownConfig {
            name: "Lejos".into(),
            origin: GroundVec2::new(3000.0, -2000.0),
            ..TownConfig::castex()
        })
        .unwrap();
        let mut app = headless_app();
        app.world_mut()
            .resource_mut::<WorldTowns>()
            .0
            .push(far.clone());
        let player = app
            .world_mut()
            .spawn((
                MapIcon::Player,
                Transform::from_translation(crate::world_from_ground(GroundVec2::ZERO, 0.0)),
            ))
            .id();
        app.update();
        assert_eq!(app.world().resource::<Minimap>().town, 0);
        let base = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&ImageNode, With<MinimapBase>>();
            q.single(app.world()).unwrap().image.clone()
        };
        let castex_base = base(&mut app);

        // Past the halfway point the far town takes over, with its frame.
        let near_far = GroundVec2::new(2000.0, -1300.0);
        app.world_mut()
            .get_mut::<Transform>(player)
            .unwrap()
            .translation = crate::world_from_ground(near_far, 0.0);
        app.update();
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.town, 1);
        assert_eq!(minimap.frame, MapFrame::for_town(&far, MAP_PX));
        let far_base = base(&mut app);
        assert_ne!(far_base, castex_base);

        // Back home: the cached layer is reused, no new image.
        let images = app.world().resource::<Assets<Image>>().len();
        app.world_mut()
            .get_mut::<Transform>(player)
            .unwrap()
            .translation = crate::world_from_ground(GroundVec2::new(10.0, 10.0), 0.0);
        app.update();
        assert_eq!(app.world().resource::<Minimap>().town, 0);
        assert_eq!(base(&mut app), castex_base);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), images);
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
