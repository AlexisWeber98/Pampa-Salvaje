//! El minimapa en la esquina superior derecha: el pueblo más cercano al jugador
//! (por distancia a su extensión) dibujado a partir de los datos de
//! `pampa-core`, con el norte arriba, titulado con su nombre, con marcadores en
//! vivo para el jugador, los peatones y los vagones del tren de la línea dentro
//! del mapa, y el destino del mandado ([`MapTarget`]) fijado en el borde
//! mientras está fuera del mapa. Una rosa de los vientos (N, E, S, O) en la
//! esquina inferior izquierda muestra la orientación del mapa: el mapa tiene el
//! norte arriba, así que la rosa queda fija y la flecha del jugador muestra el
//! rumbo.
//!
//! En el campo, el mapa deja el pueblo por un encuadre de campo: un cuadrado de
//! escala fija ([`raster::FIELD_HALF_EXTENT`]) alrededor del jinete, con los
//! caminos, el ferrocarril, los asentamientos (huellas de edificio por tipo,
//! contornos de corrales, huellas, árboles) y los restos abandonados al costado
//! del camino. El encuadre de un pueblo también muestra cualquier asentamiento
//! que entre en él. El cambio, con histéresis para que el mapa no parpadee en
//! el límite ([`next_mode`]):
//!
//! - de pueblo a campo cuando el jinete está a más de [`FIELD_ENTER`] metros de
//!   la extensión del pueblo más cercano, o a menos de [`SETTLEMENT_ENTER`]
//!   metros de los límites de un asentamiento;
//! - de campo a pueblo cuando el jinete está a menos de [`TOWN_ENTER`] metros
//!   de la extensión de un pueblo y a más de [`SETTLEMENT_LEAVE`] metros de
//!   todos los asentamientos.
//!
//! La capa de cada pueblo se rasteriza una vez, la primera vez que se muestra,
//! y se guarda por pueblo (una imagen por pueblo: acotado). La capa de campo es
//! una imagen más, que se vuelve a pintar en el lugar solo cuando se entra en
//! el encuadre o cuando el jinete se alejó [`FIELD_RECENTER`] metros de su
//! centro (bien dentro de su borde), nunca por frame. Una segunda imagen
//! transparente del mismo tamaño se redibuja en cada frame solo con los
//! marcadores. Ambas son nodos de imagen de UI apilados dentro de un panel con
//! marco. `M` alterna entre el mapa de la esquina y uno ampliado y centrado.

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
use crate::train::WorldTrains;
use raster::{
    Canvas, MapFrame, Markers, Overlay, draw_markers, palette, rasterize_field, rasterize_town_over,
};

/// Lado de las imágenes del mapa, en píxeles.
pub const MAP_PX: usize = 320;
/// Lado del mapa de la esquina en pantalla, en píxeles lógicos.
pub const SMALL_SIZE: Val = Val::Px(220.0);
/// Lado del mapa ampliado, como proporción del lado menor de la ventana.
pub const LARGE_SIZE: Val = Val::VMin(80.0);
/// Tecla que alterna el mapa ampliado.
pub const TOGGLE_KEY: KeyCode = KeyCode::KeyM;
/// Metros desde la extensión del pueblo más cercano a partir de los cuales el
/// mapa deja el pueblo por el encuadre de campo. El encuadre de un pueblo
/// muestra su extensión más [`raster::MARGIN`] alrededor, así que el jinete
/// siempre está en el mapa mostrado, al menos 10 m dentro de su borde, con los
/// rieles y caminos de alrededor: nunca fijado en el borde de un mapa de pueblo
/// que deja afuera la vía sobre la que está.
pub const FIELD_ENTER: f32 = raster::MARGIN - 10.0;
/// Metros desde la extensión de un pueblo dentro de los cuales el encuadre de
/// campo da paso al del pueblo: 15 m de histéresis por debajo de
/// [`FIELD_ENTER`].
pub const TOWN_ENTER: f32 = FIELD_ENTER - 15.0;
/// Metros desde los límites de un asentamiento dentro de los cuales el mapa
/// muestra el campo.
pub const SETTLEMENT_ENTER: f32 = 20.0;
/// Metros desde los límites de todos los asentamientos a los que debe estar el
/// jinete para que el mapa vuelva a un pueblo.
pub const SETTLEMENT_LEAVE: f32 = 50.0;
/// Metros que el jinete puede alejarse del centro del encuadre de campo antes
/// de que se vuelva a pintar a su alrededor: la flecha queda 240 m dentro del
/// borde.
pub const FIELD_RECENTER: f32 = 80.0;

/// Qué encuadre muestra el mapa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapMode {
    /// El encuadre del pueblo más cercano ([`Minimap::town`]).
    Town,
    /// El encuadre de campo alrededor del jinete.
    Field,
}

/// El encuadre a mostrar después de `current`, para un jinete a `town` metros
/// de la extensión del pueblo más cercano y a `settlement` metros de los
/// límites del asentamiento más cercano (infinito si no hay ninguno). Ver la
/// documentación del módulo.
pub fn next_mode(current: MapMode, town: f32, settlement: f32) -> MapMode {
    match current {
        MapMode::Town if town > FIELD_ENTER || settlement < SETTLEMENT_ENTER => MapMode::Field,
        MapMode::Field if town < TOWN_ENTER && settlement > SETTLEMENT_LEAVE => MapMode::Town,
        mode => mode,
    }
}

/// Distancia desde `p` hasta los límites del asentamiento más cercano (0 si
/// está adentro); infinita si no hay asentamientos.
fn settlement_distance(settlements: Option<&SettlementLayout>, p: GroundVec2) -> f32 {
    settlements
        .into_iter()
        .flat_map(|l| l.settlements.iter())
        .map(|s| s.bounds.closest_point(p).distance(p))
        .fold(f32::INFINITY, f32::min)
}

/// Cómo se muestra una entidad en el minimapa. Su posición en el suelo viene de
/// su `Transform`; el rumbo del jugador, de su [`VisualPose`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapIcon {
    Player,
    Walker,
}

/// Adónde envía el mandado al jugador, marcado en el mapa (fijado en el borde
/// cuando está fuera de él); `None` oculta el marcador. El juego lo mantiene
/// actualizado.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct MapTarget(pub Option<GroundVec2>);

/// El estado del minimapa: el pueblo mostrado y su encuadre, la capa de
/// marcadores y si está ampliado.
#[derive(Resource, Debug)]
pub struct Minimap {
    /// Qué encuadre se muestra.
    pub mode: MapMode,
    /// Índice del pueblo mostrado (o el último mostrado, en el campo), en
    /// [`WorldTowns`].
    pub town: usize,
    pub frame: MapFrame,
    pub enlarged: bool,
    /// Dónde se dibujó la flecha del jugador en el frame anterior, en píxeles
    /// del mapa.
    pub player_px: Option<Vec2>,
    /// Dónde se dibujó el destino en el frame anterior, en píxeles del mapa, y
    /// si estaba en el mapa (si no, está fijado en el borde).
    pub target_px: Option<(Vec2, bool)>,
    markers: Handle<Image>,
    canvas: Canvas,
    /// La capa de cada pueblo mostrado, por índice de pueblo.
    bases: HashMap<usize, Handle<Image>>,
    /// La capa de campo, que se vuelve a pintar en el lugar.
    field: Option<Handle<Image>>,
    /// Cuántas veces se pintó la capa de campo.
    pub field_paints: usize,
}

/// El nodo de imagen que muestra la capa del pueblo.
#[derive(Component, Debug)]
pub struct MinimapBase;

/// El nodo de layout de ventana completa que ubica el panel del mapa.
#[derive(Component, Debug)]
pub struct MinimapRoot;

/// El panel del mapa con marco.
#[derive(Component, Debug)]
pub struct MinimapPanel;

/// Dibuja el más cercano de los [`WorldTowns`] como minimapa. Sin pueblos no
/// dibuja nada.
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
        // También se guarda en la CPU: la capa de marcadores se reescribe en
        // cada frame.
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Los tramos de la línea del ferrocarril, como las piezas rectas cortas con
/// las que se dibujan sus curvas, pintados en la capa de cada pueblo donde
/// cruzan su encuadre.
fn links(rail: Option<&WorldRail>) -> Vec<Railway> {
    rail.map(|r| r.0.segments().collect()).unwrap_or_default()
}

/// Los andenes de las vías muertas, pintados en la capa de cada pueblo donde
/// cruzan su encuadre.
fn bay_platforms(rail: Option<&WorldRail>) -> Vec<pampa_core::Aabb> {
    rail.map(|r| r.0.bays.iter().map(|b| b.platform.footprint).collect())
        .unwrap_or_default()
}

/// Las piezas rectas de los caminos, pintadas en la capa de cada pueblo donde
/// cruzan su encuadre.
fn roads(terrain: Option<&WorldTerrain>) -> Vec<(GroundVec2, GroundVec2)> {
    terrain
        .and_then(|t| t.0.roads.as_deref())
        .map(|r| r.segments().collect())
        .unwrap_or_default()
}

/// Las lagunas, que lleva el terreno.
fn lagoons(terrain: Option<&WorldTerrain>) -> Vec<Lagoon> {
    terrain.map(|t| t.0.lagoons.clone()).unwrap_or_default()
}

/// Lo que muestra una capa más allá del pueblo: los tramos de la línea del
/// ferrocarril, los caminos y las lagunas.
type Lines = (
    Vec<Railway>,
    Vec<(GroundVec2, GroundVec2)>,
    Vec<Lagoon>,
    Vec<pampa_core::Aabb>,
);

/// El [`Overlay`] de `lines` y los asentamientos del mundo.
fn overlay<'a>(lines: &'a Lines, settlements: Option<&'a SettlementLayout>) -> Overlay<'a> {
    Overlay {
        links: &lines.0,
        roads: &lines.1,
        settlements: settlements.map_or(&[], |l| &l.settlements),
        wrecks: settlements.map_or(&[], |l| &l.roadside),
        lagoons: &lines.2,
        platforms: &lines.3,
    }
}

fn rgba(c: [u8; 4]) -> Color {
    Color::srgba_u8(c[0], c[1], c[2], c[3])
}

/// Dónde empieza el mapa y qué dibuja más allá del pueblo.
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
        bay_platforms(rail.as_deref()),
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

/// El layout de la raíz de ventana completa: el panel en la esquina superior
/// derecha, o centrado cuando está ampliado.
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

/// El panel cuadrado con marco, pequeño o ampliado.
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

/// La posición en el plano del suelo de una traslación en espacio de mundo.
fn ground_of(translation: Vec3) -> GroundVec2 {
    GroundVec2::new(translation.x, -translation.z)
}

/// Cambia el mapa al pueblo `index`, rasterizando su capa la primera vez.
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

/// Cambia el mapa al encuadre de campo alrededor de `at`, volviendo a pintar en
/// el lugar la única capa de campo (agregándola la primera vez).
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

/// Lo que el minimapa lee del mundo: pueblos, trenes, la línea del ferrocarril,
/// los caminos (que lleva el terreno) y los asentamientos.
type WorldLayers<'a> = (
    Option<Res<'a, WorldTowns>>,
    Option<Res<'a, WorldTrains>>,
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
        // `nearest_town` ordena los pueblos por la distancia a su extensión, la
        // misma medida que lee el modo, nunca por sus centros.
        let mode = next_mode(
            minimap.mode,
            distance_to_town(&towns.0[nearest], at),
            settlement_distance(layout, at),
        );
        // Solo un cambio de encuadre, o un jinete lejos del centro del encuadre
        // de campo, vuelve a pintar una capa.
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
                bay_platforms(rail.as_deref()),
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
    // El tren de la línea va entre pueblos: los marcadores dibujan solo los
    // vagones dentro del encuadre del pueblo mostrado.
    let mut markers = Markers {
        cars: trains
            .as_ref()
            .map(|t| t.0.iter().flat_map(|t| t.placed_cars()).collect())
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

        // Mantener apretada la tecla no hace parpadear.
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
        // Capa base y capa de marcadores.
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

        // Cerca del pueblo lejano, toma el control con su encuadre.
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

        // De vuelta en casa: se reutiliza la capa guardada, sin imagen nueva.
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
        // Un pueblo pequeño (listado primero) cuyo centro está más cerca del
        // jinete, y uno grande cuyo borde está más cerca: el pueblo grande es
        // el cercano.
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
        // Justo al este del borde del pueblo grande: dentro de su TOWN_ENTER, y
        // a más de FIELD_ENTER del borde del pueblo pequeño.
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
        // Otro pueblo, lejos hacia el oeste: fijado en el borde oeste.
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
        // Dentro del pueblo mostrado: se dibuja donde está.
        let near = app.world().resource::<WorldTowns>().0[0].plaza.center();
        app.insert_resource(MapTarget(Some(near)));
        app.update();
        let minimap = app.world().resource::<Minimap>();
        let (at, on_map) = minimap.target_px.unwrap();
        assert!(on_map && at.distance(minimap.frame.to_map(near)) < 1e-3);
        // Mandado cumplido: sin marcador.
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
        // En parada en Monte Nievas (pueblo 0) en el servicio de la mañana.
        let (arrive, _) = train.stop_times(Run::Up, 1).unwrap();
        train.advance(0.0, 7.0 * 3600.0 + (arrive + 5.0) * BASE_RATE, BASE_RATE);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTowns(towns.clone()))
            .insert_resource(WorldTrains(vec![train.clone()]))
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
    fn marks_the_cars_of_every_train_in_the_shown_town() {
        use pampa_core::{BASE_RATE, Run};
        let (towns, line) = crate::train::tests::line_train();
        // Una lanzadera que sale de Pico a las 08:00, una hora detrás del tren
        // de la línea: en parada en Monte Nievas cuando el tren de la línea ya
        // se fue hace rato.
        let shuttle = crate::train::tests::shuttle(&line, &towns, 22, 8);
        let mut shuttle = shuttle;
        let (arrive, _) = shuttle.stop_times(Run::Up, 1).unwrap();
        shuttle.advance(0.0, 8.0 * 3600.0 + (arrive + 5.0) * BASE_RATE, BASE_RATE);
        let mut line = line;
        line.advance(0.0, 8.0 * 3600.0 + 1800.0, BASE_RATE);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTowns(towns.clone()))
            .insert_resource(WorldTrains(vec![line.clone(), shuttle.clone()]))
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
        let shown: Vec<_> = shuttle
            .placed_cars()
            .into_iter()
            .filter(|c| minimap.frame.contains(c.center))
            .collect();
        assert!(!shown.is_empty(), "the shuttle is in the town");
        assert!(
            !line
                .placed_cars()
                .iter()
                .any(|c| minimap.frame.contains(c.center)),
            "the line train is elsewhere"
        );
        for car in shown {
            let at = minimap.frame.to_map(car.center);
            let i = (at.y as usize * MAP_PX + at.x as usize) * 4;
            assert_eq!(data[i + 3], 255, "shuttle car marked at {at:?}");
        }
    }

    #[test]
    fn next_mode_switches_with_hysteresis() {
        use MapMode::{Field, Town};
        let far = f32::INFINITY;
        // En el pueblo hasta pasar FIELD_ENTER, en el campo hasta estar dentro
        // de TOWN_ENTER: la franja intermedia conserva el encuadre que se
        // muestre.
        let between = (TOWN_ENTER + FIELD_ENTER) / 2.0;
        assert_eq!(next_mode(Town, 0.0, far), Town);
        assert_eq!(next_mode(Town, between, far), Town);
        assert_eq!(next_mode(Town, FIELD_ENTER + 1.0, far), Field);
        assert_eq!(next_mode(Field, between, far), Field);
        assert_eq!(next_mode(Field, TOWN_ENTER - 1.0, far), Town);
        // Cerca de un asentamiento es el campo, incluso cerca de un pueblo,
        // hasta que el jinete se aleja bien de él.
        assert_eq!(next_mode(Town, 0.0, SETTLEMENT_ENTER - 1.0), Field);
        assert_eq!(next_mode(Field, 0.0, 30.0), Field);
        assert_eq!(next_mode(Field, 0.0, SETTLEMENT_LEAVE + 1.0), Town);
        // Se deja el mapa de un pueblo antes de que el jinete salga de él.
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

        // Entre los umbrales, sigue siendo el pueblo.
        let between = (TOWN_ENTER + FIELD_ENTER) / 2.0;
        move_to(&mut app, player, east(between));
        assert_eq!(app.world().resource::<Minimap>().mode, MapMode::Town);
        assert_eq!(app.world().resource::<Minimap>().frame, town_frame);

        // Pasado el margen del pueblo: el encuadre de campo alrededor del
        // jinete.
        let out = FIELD_ENTER + 5.0;
        move_to(&mut app, player, east(out));
        let minimap = app.world().resource::<Minimap>();
        assert_eq!(minimap.mode, MapMode::Field);
        assert_eq!(minimap.frame, MapFrame::field(east(out), MAP_PX));
        assert_eq!(minimap.field_paints, 1);
        let at = minimap.player_px.unwrap();
        assert!(at.distance(Vec2::splat(MAP_PX as f32 / 2.0)) < 1e-3);
        assert_ne!(base_image(&mut app), town_base);

        // De vuelta entre los umbrales: sigue siendo el campo.
        move_to(&mut app, player, east(between));
        assert_eq!(app.world().resource::<Minimap>().mode, MapMode::Field);

        // Cerca: el encuadre del pueblo como antes, con su capa guardada
        // reutilizada.
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
        // Un punto sobre la línea, a unas decenas de metros del primer pueblo:
        // pasado el margen de su encuadre, bastante antes del viejo umbral de
        // campo.
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
        // La primera pintura muestra al jinete en el mapa, sobre los rieles.
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
        // Moviéndose dentro de FIELD_RECENTER: sin volver a pintar, la flecha
        // se mueve.
        for step in 1..=8 {
            let p = start + GroundVec2::new(9.0 * step as f32, 0.0);
            move_to(&mut app, player, p);
            let minimap = app.world().resource::<Minimap>();
            assert_eq!(minimap.field_paints, 1);
            assert_eq!(minimap.frame.center, start);
            let at = minimap.player_px.unwrap();
            assert!(at.distance(minimap.frame.to_map(p)) < 1e-3);
        }
        // Pasado ese límite: se vuelve a pintar una vez, alrededor del jinete,
        // en la misma imagen.
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
