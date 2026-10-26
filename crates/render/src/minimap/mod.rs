//! Minimap in the top-right corner: the town nearest the player (by
//! distance to its extent) drawn from `pampa-core` data, north-up, titled
//! with its name, with live markers for the player, the walkers and the
//! line train's cars inside the map, and the errand's target ([`MapTarget`])
//! pinned to the edge while it is off the map. A compass rose (N, E, S, O)
//! in the bottom-left corner shows the map's orientation: the map is
//! north-up, so the rose stays put and the player's arrow shows the heading.
//!
//! Out in the country the map leaves the town for a field frame: a fixed
//! scale square ([`raster::FIELD_HALF_EXTENT`]) round the rider, with the
//! roads, the railway, the settlements (footprints by kind, corral
//! outlines, huellas, trees) and the roadside wrecks. A town's frame also
//! shows any settlement reaching into it. The switch, with hysteresis so
//! the map does not flicker on the boundary ([`next_mode`]):
//!
//! - town → field when the rider is more than [`FIELD_ENTER`] meters from
//!   the nearest town's extent, or within [`SETTLEMENT_ENTER`] meters of a
//!   settlement's bounds;
//! - field → town when the rider is within [`TOWN_ENTER`] meters of a
//!   town's extent and more than [`SETTLEMENT_LEAVE`] meters from every
//!   settlement.
//!
//! Each town's layer is rasterized once, the first time it is shown, and
//! kept per town (one image per town: bounded). The field layer is one more
//! image, repainted in place only when the frame is entered or the rider has
//! ridden [`FIELD_RECENTER`] meters from its center (well inside its edge),
//! never per frame. A second, transparent image of the same size is redrawn
//! every frame with the markers only. Both are UI image nodes stacked inside
//! a framed panel. `M` toggles between the corner map and an enlarged,
//! centered one.

pub mod raster;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use std::collections::HashMap;

use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{
    Lagoon, Railway, SettlementLayout, Vec2 as GroundVec2, distance_to_town, nearest_town,
};

use crate::character::VisualPose;
use crate::rail::WorldRail;
use crate::settlement::WorldSettlements;
use crate::terrain::{StreamFocus, WorldTerrain};
use crate::town::WorldTowns;
use crate::train::WorldTrain;
use raster::{
    Canvas, MapFrame, Markers, Overlay, draw_markers, palette, rasterize_field, rasterize_town_over,
};

/// Side of the map images, in pixels.
pub const MAP_PX: usize = 320;
/// Side of the corner map on screen, in logical pixels.
pub const SMALL_SIZE: Val = Val::Px(220.0);
/// Side of the enlarged map, as a share of the window's smaller side.
pub const LARGE_SIZE: Val = Val::VMin(80.0);
/// Key that toggles the enlarged map.
pub const TOGGLE_KEY: KeyCode = KeyCode::KeyM;
/// Meters from the nearest town's extent past which the map leaves the
/// town for the field frame. A town's frame shows its extent plus
/// [`raster::MARGIN`] all round, so the rider is always on the shown map,
/// at least 10 m inside its edge, with the rails and roads round them:
/// never pinned to the edge of a town map that leaves out the track they
/// stand on.
pub const FIELD_ENTER: f32 = raster::MARGIN - 10.0;
/// Meters from a town's extent within which the field frame gives way to
/// the town's: 15 m of hysteresis under [`FIELD_ENTER`].
pub const TOWN_ENTER: f32 = FIELD_ENTER - 15.0;
/// Meters from a settlement's bounds within which the map shows the field.
pub const SETTLEMENT_ENTER: f32 = 20.0;
/// Meters from every settlement's bounds the rider must be for the map to
/// go back to a town.
pub const SETTLEMENT_LEAVE: f32 = 50.0;
/// Meters the rider may ride from the field frame's center before it is
/// repainted round them: the arrow stays 240 m inside the edge.
pub const FIELD_RECENTER: f32 = 80.0;

/// Which frame the map shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapMode {
    /// The nearest town's frame ([`Minimap::town`]).
    Town,
    /// The field frame round the rider.
    Field,
}

/// The frame to show after `current`, for a rider `town` meters from the
/// nearest town's extent and `settlement` meters from the nearest
/// settlement's bounds (infinite without any). See the module docs.
pub fn next_mode(current: MapMode, town: f32, settlement: f32) -> MapMode {
    match current {
        MapMode::Town if town > FIELD_ENTER || settlement < SETTLEMENT_ENTER => MapMode::Field,
        MapMode::Field if town < TOWN_ENTER && settlement > SETTLEMENT_LEAVE => MapMode::Town,
        mode => mode,
    }
}

/// Distance from `p` to the nearest settlement's bounds (0 inside);
/// infinite without settlements.
fn settlement_distance(settlements: Option<&SettlementLayout>, p: GroundVec2) -> f32 {
    settlements
        .into_iter()
        .flat_map(|l| l.settlements.iter())
        .map(|s| s.bounds.closest_point(p).distance(p))
        .fold(f32::INFINITY, f32::min)
}

/// What an entity shows as on the minimap. Its ground position comes from
/// its `Transform`; the player's heading from its [`VisualPose`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapIcon {
    Player,
    Walker,
}

/// Where the errand sends the player, marked on the map (pinned to the
/// edge when outside it); `None` hides the marker. The game keeps it up to
/// date.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct MapTarget(pub Option<GroundVec2>);

/// Minimap state: the town shown and its framing, the marker layer and
/// whether it is enlarged.
#[derive(Resource, Debug)]
pub struct Minimap {
    /// Which frame is shown.
    pub mode: MapMode,
    /// Index of the town shown (or last shown, in the field), in
    /// [`WorldTowns`].
    pub town: usize,
    pub frame: MapFrame,
    pub enlarged: bool,
    /// Where the player's arrow was drawn last frame, in map pixels.
    pub player_px: Option<Vec2>,
    /// Where the target was drawn last frame, in map pixels, and whether it
    /// was on the map (otherwise it is pinned to the edge).
    pub target_px: Option<(Vec2, bool)>,
    markers: Handle<Image>,
    canvas: Canvas,
    /// Each shown town's layer, by town index.
    bases: HashMap<usize, Handle<Image>>,
    /// The field layer, repainted in place.
    field: Option<Handle<Image>>,
    /// How many times the field layer has been painted.
    pub field_paints: usize,
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

/// The railway line's links, as the short straight pieces their curves are
/// drawn with, painted on every town's layer where they cross its frame.
fn links(rail: Option<&WorldRail>) -> Vec<Railway> {
    rail.map(|r| r.0.segments().collect()).unwrap_or_default()
}

/// The roads' straight pieces, painted on every town's layer where they
/// cross its frame.
fn roads(terrain: Option<&WorldTerrain>) -> Vec<(GroundVec2, GroundVec2)> {
    terrain
        .and_then(|t| t.0.roads.as_deref())
        .map(|r| r.segments().collect())
        .unwrap_or_default()
}

/// The lagoons, carried by the terrain.
fn lagoons(terrain: Option<&WorldTerrain>) -> Vec<Lagoon> {
    terrain.map(|t| t.0.lagoons.clone()).unwrap_or_default()
}

/// What a layer shows beyond the town: the railway line's links, the
/// roads and the lagoons.
type Lines = (Vec<Railway>, Vec<(GroundVec2, GroundVec2)>, Vec<Lagoon>);

/// The [`Overlay`] of `lines` and the world's settlements.
fn overlay<'a>(lines: &'a Lines, settlements: Option<&'a SettlementLayout>) -> Overlay<'a> {
    Overlay {
        links: &lines.0,
        roads: &lines.1,
        settlements: settlements.map_or(&[], |l| &l.settlements),
        wrecks: settlements.map_or(&[], |l| &l.roadside),
        lagoons: &lines.2,
    }
}

fn rgba(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// Where the map starts and what it draws beyond the town.
type MapSources<'a> = (
    Option<Res<'a, StreamFocus>>,
    Option<Res<'a, WorldRail>>,
    Option<Res<'a, WorldTerrain>>,
    Option<Res<'a, WorldSettlements>>,
);

fn spawn_minimap(
    mut commands: Commands,
    towns: Option<Res<WorldTowns>>,
    (focus, rail, terrain, settlements): MapSources,
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
    let lines = (
        links(rail.as_deref()),
        roads(terrain.as_deref()),
        lagoons(terrain.as_deref()),
    );
    let overlay = overlay(&lines, settlements.as_deref().map(|s| &s.0));
    let base = images.add(map_image(
        rasterize_town_over(town, &overlay, &frame).pixels,
    ));
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
        mode: MapMode::Town,
        town: index,
        frame,
        enlarged: false,
        player_px: None,
        target_px: None,
        markers,
        canvas,
        bases: HashMap::from([(index, base)]),
        field: None,
        field_paints: 0,
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
    (towns, overlay): (&WorldTowns, &Overlay),
    index: usize,
    images: &mut Assets<Image>,
    bases: &mut Query<&mut ImageNode, With<MinimapBase>>,
) {
    let Some(town) = towns.0.get(index) else {
        return;
    };
    minimap.mode = MapMode::Town;
    minimap.town = index;
    minimap.frame = MapFrame::for_town(town, MAP_PX);
    let frame = minimap.frame;
    let base = minimap
        .bases
        .entry(index)
        .or_insert_with(|| images.add(map_image(rasterize_town_over(town, overlay, &frame).pixels)))
        .clone();
    for mut node in bases {
        node.image = base.clone();
    }
}

/// Switches the map to the field frame round `at`, repainting the one
/// field layer in place (adding it the first time).
fn show_field(
    minimap: &mut Minimap,
    (towns, overlay): (&WorldTowns, &Overlay),
    at: GroundVec2,
    images: &mut Assets<Image>,
    bases: &mut Query<&mut ImageNode, With<MinimapBase>>,
) {
    minimap.mode = MapMode::Field;
    minimap.frame = MapFrame::field(at, MAP_PX);
    minimap.field_paints += 1;
    let pixels = rasterize_field(&towns.0, overlay, &minimap.frame).pixels;
    let field = match &minimap.field {
        Some(handle) => {
            if let Some(mut image) = images.get_mut(handle) {
                image.data = Some(pixels);
            }
            handle.clone()
        }
        None => minimap.field.insert(images.add(map_image(pixels))).clone(),
    };
    for mut node in bases {
        node.image = field.clone();
    }
}

/// What the minimap reads of the world: towns, trains, the railway line,
/// the roads (carried by the terrain) and the settlements.
type WorldLayers<'a> = (
    Option<Res<'a, WorldTowns>>,
    Option<Res<'a, WorldTrain>>,
    Option<Res<'a, WorldRail>>,
    Option<Res<'a, WorldTerrain>>,
    Option<Res<'a, WorldSettlements>>,
);

fn update_markers(
    minimap: Option<ResMut<Minimap>>,
    (towns, trains, rail, terrain, settlements): WorldLayers,
    target: Option<Res<MapTarget>>,
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
    {
        let layout = settlements.as_deref().map(|s| &s.0);
        // `nearest_town` ranks towns by the distance to their extent, the
        // same measure the mode reads, never by their centres.
        let mode = next_mode(
            minimap.mode,
            distance_to_town(&towns.0[nearest], at),
            settlement_distance(layout, at),
        );
        // Only a change of frame, or a rider far from the field frame's
        // center, repaints a layer.
        let repaint = match mode {
            MapMode::Town => minimap.mode != MapMode::Town || nearest != minimap.town,
            MapMode::Field => {
                minimap.mode != MapMode::Field || minimap.frame.center.distance(at) > FIELD_RECENTER
            }
        };
        if repaint {
            let lines = (
                links(rail.as_deref()),
                roads(terrain.as_deref()),
                lagoons(terrain.as_deref()),
            );
            let overlay = overlay(&lines, layout);
            let sources = (&**towns, &overlay);
            match mode {
                MapMode::Town => {
                    show_town(&mut minimap, sources, nearest, &mut images, &mut bases);
                }
                MapMode::Field => {
                    show_field(&mut minimap, sources, at, &mut images, &mut bases);
                }
            }
        }
    }
    // The line train runs between towns: the markers draw only the cars
    // inside the shown town's frame.
    let mut markers = Markers {
        cars: trains
            .as_ref()
            .and_then(|t| t.0.as_ref())
            .map(|t| t.placed_cars())
            .unwrap_or_default(),
        target: target.and_then(|t| t.0),
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
    minimap.target_px = markers.target.map(|t| minimap.frame.to_map_clamped(t));
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

        // Near the far town, it takes over with its frame.
        let near_far = far.extent().max + GroundVec2::new(10.0, 10.0);
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
    fn the_mode_follows_the_nearest_town_by_extent_not_by_centre() {
        // A small town (listed first) whose centre is nearer the rider, and
        // a big one whose edge is nearer: the big town is the near one.
        let big = TownLayout::generate(&TownConfig {
            name: "Grande".into(),
            blocks: (8, 4),
            block_size: 60.0,
            ..TownConfig::castex()
        })
        .unwrap();
        let small_config = TownConfig {
            name: "Chico".into(),
            blocks: (3, 3),
            block_size: 30.0,
            street_width: 12.0,
            railway: None,
            ..TownConfig::castex()
        };
        let probe = TownLayout::generate(&small_config).unwrap().extent();
        let (be, half) = (big.extent(), (probe.max.x - probe.min.x) / 2.0);
        // Just east of the big town's edge: inside TOWN_ENTER of it, and
        // more than FIELD_ENTER from the small town's edge.
        let rider = GroundVec2::new(be.max.x + TOWN_ENTER / 2.0, be.center().y);
        let small_center = rider + GroundVec2::new(FIELD_ENTER + 20.0 + half, 0.0);
        let small = TownLayout::generate(&TownConfig {
            origin: small_center - probe.center(),
            ..small_config
        })
        .unwrap();
        let se = small.extent();
        assert!(rider.distance(se.center()) < rider.distance(be.center()));
        assert!(distance_to_town(&big, rider) < TOWN_ENTER);
        assert!(distance_to_town(&small, rider) > FIELD_ENTER);

        let mut app = headless_app();
        app.insert_resource(WorldTowns(vec![small, big]));
        app.world_mut().spawn((
            MapIcon::Player,
            Transform::from_translation(crate::world_from_ground(rider, 0.0)),
        ));
        app.update();
        app.update();
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.mode, MapMode::Town);
        assert_eq!(minimap.town, 1);
    }

    #[test]
    fn the_errand_target_is_marked_and_pinned_to_the_edge_when_off_the_map() {
        let mut app = headless_app();
        app.update();
        assert_eq!(app.world().resource::<Minimap>().target_px, None);
        // Another town, far to the west: pinned to the west edge.
        let far = GroundVec2::new(-3000.0, 0.0);
        app.insert_resource(MapTarget(Some(far)));
        app.update();
        let minimap = app.world().resource::<Minimap>();
        let expected = minimap.frame.to_map_clamped(far);
        let (at, on_map) = minimap.target_px.expect("target marked");
        assert!(!on_map);
        assert!(at.distance(expected.0) < 1e-3);
        assert!((at.x - raster::EDGE).abs() < 1e-3, "{at:?}");
        let data = {
            let images = app.world().resource::<Assets<Image>>();
            images.get(&minimap.markers).unwrap().data.clone().unwrap()
        };
        let i = (at.y as usize * MAP_PX + at.x as usize) * 4;
        assert_eq!(data[i..i + 4], palette::TARGET);
        // Inside the shown town: drawn where it is.
        let near = app.world().resource::<WorldTowns>().0[0].plaza.center();
        app.insert_resource(MapTarget(Some(near)));
        app.update();
        let minimap = app.world().resource::<Minimap>();
        let (at, on_map) = minimap.target_px.unwrap();
        assert!(on_map && at.distance(minimap.frame.to_map(near)) < 1e-3);
        // Errand done: no marker.
        app.insert_resource(MapTarget(None));
        app.update();
        assert_eq!(app.world().resource::<Minimap>().target_px, None);
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

    #[test]
    fn marks_the_line_train_cars_in_the_shown_town() {
        use pampa_core::{BASE_RATE, Run};
        let (towns, mut train) = crate::train::tests::line_train();
        // Dwelling at Monte Nievas (town 0) on the morning run.
        let (arrive, _) = train.stop_times(Run::Up, 1).unwrap();
        train.advance(0.0, 7.0 * 3600.0 + (arrive + 5.0) * BASE_RATE, BASE_RATE);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTowns(towns.clone()))
            .insert_resource(WorldTrain(Some(train.clone())))
            .add_plugins(MinimapPlugin);
        let player = towns[0].grid().min;
        app.world_mut().spawn((
            MapIcon::Player,
            Transform::from_translation(crate::world_from_ground(player, 0.0)),
        ));
        app.update();
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.town, 0);
        let images = app.world().resource::<Assets<Image>>();
        let data = images.get(&minimap.markers).unwrap().data.as_ref().unwrap();
        let cars = train.placed_cars();
        let shown: Vec<_> = cars
            .iter()
            .filter(|c| minimap.frame.contains(c.center))
            .collect();
        assert!(!shown.is_empty());
        for car in shown {
            let at = minimap.frame.to_map(car.center);
            let i = (at.y as usize * MAP_PX + at.x as usize) * 4;
            assert_eq!(data[i + 3], 255, "car marked at {at:?}");
        }
    }

    #[test]
    fn next_mode_switches_with_hysteresis() {
        use MapMode::{Field, Town};
        let far = f32::INFINITY;
        // In town until past FIELD_ENTER, in the field until within
        // TOWN_ENTER: the band between keeps whichever frame is shown.
        let between = (TOWN_ENTER + FIELD_ENTER) / 2.0;
        assert_eq!(next_mode(Town, 0.0, far), Town);
        assert_eq!(next_mode(Town, between, far), Town);
        assert_eq!(next_mode(Town, FIELD_ENTER + 1.0, far), Field);
        assert_eq!(next_mode(Field, between, far), Field);
        assert_eq!(next_mode(Field, TOWN_ENTER - 1.0, far), Town);
        // Near a settlement it is the field, even close to a town, until
        // the rider is well clear of it.
        assert_eq!(next_mode(Town, 0.0, SETTLEMENT_ENTER - 1.0), Field);
        assert_eq!(next_mode(Field, 0.0, 30.0), Field);
        assert_eq!(next_mode(Field, 0.0, SETTLEMENT_LEAVE + 1.0), Town);
        // A town's map is left before the rider rides off it.
        const { assert!(TOWN_ENTER > 0.0 && FIELD_ENTER < raster::MARGIN) };
    }

    fn move_to(app: &mut App, entity: Entity, p: GroundVec2) {
        app.world_mut()
            .get_mut::<Transform>(entity)
            .unwrap()
            .translation = crate::world_from_ground(p, 0.0);
        app.update();
    }

    fn base_image(app: &mut App) -> Handle<Image> {
        let mut q = app
            .world_mut()
            .query_filtered::<&ImageNode, With<MinimapBase>>();
        q.single(app.world()).unwrap().image.clone()
    }

    #[test]
    fn the_map_switches_to_the_field_away_from_towns_and_back_with_hysteresis() {
        let mut app = headless_app();
        let extent = app.world().resource::<WorldTowns>().0[0].extent();
        let east = |d: f32| GroundVec2::new(extent.max.x + d, extent.center().y);
        let player = app
            .world_mut()
            .spawn((
                MapIcon::Player,
                Transform::from_translation(crate::world_from_ground(east(-10.0), 0.0)),
            ))
            .id();
        app.update();
        let town_frame = app.world().resource::<Minimap>().frame;
        let town_base = base_image(&mut app);
        assert_eq!(app.world().resource::<Minimap>().mode, MapMode::Town);

        // Between the thresholds, still the town.
        let between = (TOWN_ENTER + FIELD_ENTER) / 2.0;
        move_to(&mut app, player, east(between));
        assert_eq!(app.world().resource::<Minimap>().mode, MapMode::Town);
        assert_eq!(app.world().resource::<Minimap>().frame, town_frame);

        // Past the town's margin: the field frame round the rider.
        let out = FIELD_ENTER + 5.0;
        move_to(&mut app, player, east(out));
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.mode, MapMode::Field);
        assert_eq!(minimap.frame, MapFrame::field(east(out), MAP_PX));
        assert_eq!(minimap.field_paints, 1);
        let at = minimap.player_px.unwrap();
        assert!(at.distance(Vec2::splat(MAP_PX as f32 / 2.0)) < 1e-3);
        assert_ne!(base_image(&mut app), town_base);

        // Back between the thresholds: still the field.
        move_to(&mut app, player, east(between));
        assert_eq!(app.world().resource::<Minimap>().mode, MapMode::Field);

        // Close in: the town frame as before, its cached layer reused.
        let images = app.world().resource::<Assets<Image>>().len();
        move_to(&mut app, player, east(TOWN_ENTER - 1.0));
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.mode, MapMode::Town);
        assert_eq!(minimap.frame, town_frame);
        assert_eq!(base_image(&mut app), town_base);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), images);
    }

    #[test]
    fn a_rider_on_the_rails_just_outside_a_town_sees_them_under_the_arrow() {
        let (_, towns, line) = crate::rail::tests::two_towns();
        // A point on the line, a few tens of meters out of the first town:
        // past its frame's margin, well short of the old field threshold.
        let path = line.full_path(&towns);
        let on_rails = (0..path.length() as usize)
            .map(|d| path.at(d as f32))
            .find(|&p| {
                let d = distance_to_town(&towns[0], p);
                d > raster::MARGIN + 10.0 && d < 90.0
            })
            .expect("the line leaves the first town");
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTowns(towns))
            .insert_resource(WorldRail(line))
            .add_plugins(MinimapPlugin);
        app.world_mut().spawn((
            MapIcon::Player,
            Transform::from_translation(crate::world_from_ground(on_rails, 0.0)),
        ));
        app.update();
        // The very first paint shows the rider on the map, on the rails.
        let minimap = app.world().resource::<Minimap>();
        assert!(
            minimap.frame.contains(on_rails),
            "the rider is off the shown {:?} frame",
            minimap.mode
        );
        let at = minimap.frame.to_map(on_rails);
        let handle = base_image(&mut app);
        let images = app.world().resource::<Assets<Image>>();
        let data = images.get(&handle).unwrap().data.as_ref().unwrap();
        let rail = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| {
                let (x, y) = (at.x as i32 + dx, at.y as i32 + dy);
                let i = (y as usize * MAP_PX + x as usize) * 4;
                data[i..i + 4] == palette::RAIL
            })
            .count();
        assert!(rail > 0, "no rail under the arrow at {at:?}");
    }

    #[test]
    fn the_field_layer_is_repainted_only_when_the_rider_rides_far() {
        let mut app = headless_app();
        let extent = app.world().resource::<WorldTowns>().0[0].extent();
        let start = GroundVec2::new(extent.max.x + 400.0, extent.center().y);
        let player = app
            .world_mut()
            .spawn((
                MapIcon::Player,
                Transform::from_translation(crate::world_from_ground(start, 0.0)),
            ))
            .id();
        app.update();
        let field = base_image(&mut app);
        let images = app.world().resource::<Assets<Image>>().len();
        assert_eq!(app.world().resource::<Minimap>().field_paints, 1);
        // Riding about inside FIELD_RECENTER: no repaint, the arrow moves.
        for step in 1..=8 {
            let p = start + GroundVec2::new(9.0 * step as f32, 0.0);
            move_to(&mut app, player, p);
            let minimap = app.world().resource::<Minimap>();
            assert_eq!(minimap.field_paints, 1);
            assert_eq!(minimap.frame.center, start);
            let at = minimap.player_px.unwrap();
            assert!(at.distance(minimap.frame.to_map(p)) < 1e-3);
        }
        // Past it: repainted once, round the rider, in the same image.
        let far = start + GroundVec2::new(FIELD_RECENTER + 5.0, 0.0);
        move_to(&mut app, player, far);
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.field_paints, 2);
        assert_eq!(minimap.frame.center, far);
        assert_eq!(base_image(&mut app), field);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), images);
        app.update();
        assert_eq!(app.world().resource::<Minimap>().field_paints, 2);
    }

    #[test]
    fn a_settlement_shows_on_the_field_map_round_the_rider() {
        let extent = TownLayout::castex_inspired().extent();
        let hub = GroundVec2::new(extent.max.x + 600.0, extent.center().y);
        let farm = raster::tests::farm(hub);
        let house = farm.structures[0].footprint.center();
        let mut layout = pampa_core::SettlementLayout::default();
        layout.settlements.push(farm);
        let mut app = headless_app();
        app.insert_resource(WorldSettlements(layout));
        app.world_mut().spawn((
            MapIcon::Player,
            Transform::from_translation(crate::world_from_ground(hub, 0.0)),
        ));
        app.update();
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.mode, MapMode::Field);
        let m = minimap.frame.to_map(house);
        let handle = base_image(&mut app);
        let images = app.world().resource::<Assets<Image>>();
        let data = images.get(&handle).unwrap().data.as_ref().unwrap();
        let i = (m.y as usize * MAP_PX + m.x as usize) * 4;
        assert_eq!(data[i..i + 4], palette::HOUSE);
    }
}
