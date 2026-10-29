//! Carteles pintados que nombran los lugares destacados del pueblo.
//!
//! `pampa-core` decide qué lugares tienen cartel, dónde se ubican y hacia qué
//! lado se leen ([`pampa_core::Place`]). Este módulo dispone el rótulo en la
//! letra de madera [`Face::Display`], pinta una textura de tabla gastada y la
//! monta en la fachada o sobre postes.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{
    BuildingKind, PlaceKind, Side, Sign, SignMount, TIMETABLE_BOARD_HEIGHT, TownLayout,
};

use crate::coords::world_from_ground;
use crate::lettering::{self, Extent, Face};
use crate::terrain::{WorldTerrain, hash2};
use crate::town::{TownRoot, WorldTowns};

/// Tamaño del rótulo en la textura de una tabla, en píxeles de textura.
pub const TEXT_PX: f32 = 56.0;
/// Ancho del marco pintado alrededor de una tabla, en píxeles de textura.
pub const BORDER: usize = 6;
/// Espacio libre entre el marco y el rótulo, en píxeles de textura.
pub const PADDING: usize = 20;

/// La huella del rótulo sobre una tabla: el ancho y el alto entintados en
/// píxeles, y el desplazamiento de la tinta respecto del origen de la pluma y
/// de la parte superior de la línea. El texto en blanco no tiene tinta, así que
/// toma como alto el ascendente de la línea.
struct Lettering {
    width: usize,
    height: usize,
    left: f32,
    top: f32,
}

fn lettering_of(text: &str) -> Lettering {
    let Extent { ascent, ink, .. } = lettering::measure(text, Face::Display, TEXT_PX);
    match ink {
        Some(ink) => Lettering {
            width: (ink.max_x - ink.min_x).ceil() as usize,
            height: (ink.max_y.ceil() - ink.min_y.floor()) as usize,
            left: ink.min_x,
            top: ink.min_y.floor(),
        },
        None => Lettering {
            width: 0,
            height: ascent as usize,
            left: 0.0,
            top: 0.0,
        },
    }
}

/// Tamaño en píxeles de la textura de una tabla y su tamaño en metros.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardLayout {
    pub width_px: usize,
    pub height_px: usize,
    pub width_m: f32,
    pub height_m: f32,
}

/// Dispone una tabla de una línea de `width_m` de ancho y como máximo
/// `max_height_m` de alto, con píxeles cuadrados: el rótulo es tan grande como
/// quepa, centrado, y la tabla nunca es más angosta que el texto que lleva.
pub fn layout_board(text: &str, width_m: f32, max_height_m: f32) -> BoardLayout {
    let margin = 2 * (BORDER + PADDING);
    let lettering = lettering_of(text);
    let height_px = lettering.height + margin;
    let min_width_px = lettering.width + margin;
    // El píxel más grande que respeta tanto el ancho del texto como el límite
    // de altura; redondear hacia arriba la cantidad de píxeles solo puede
    // achicar el píxel.
    let pixel = (width_m / min_width_px as f32).min(max_height_m / height_px as f32);
    let width_px = ((width_m / pixel).ceil() as usize).max(min_width_px);
    BoardLayout {
        width_px,
        height_px,
        width_m,
        height_m: width_m * height_px as f32 / width_px as f32,
    }
}

/// Colores de un estilo de tabla (sRGB).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardStyle {
    pub board: [u8; 3],
    pub frame: [u8; 3],
    pub ink: [u8; 3],
    /// Veta horizontal de madera en lugar de una superficie pintada lisa.
    pub grain: bool,
}

impl BoardStyle {
    /// La tabla en flecha de un poste indicador de camino: madera encalada,
    /// letras negras.
    pub fn signpost() -> Self {
        Self {
            board: [214, 204, 182],
            frame: [74, 54, 36],
            ink: [30, 26, 22],
            grain: true,
        }
    }

    /// Un aspecto acorde a la época para cada lugar destacado.
    pub fn for_place(kind: PlaceKind) -> Self {
        match kind {
            // Friso verde oscuro con letras doradas.
            PlaceKind::GeneralStore => Self {
                board: [38, 64, 46],
                frame: [92, 66, 40],
                ink: [214, 178, 92],
                grain: false,
            },
            // Esmalte blanco con letras azules.
            PlaceKind::PoliceStation => Self {
                board: [228, 224, 210],
                frame: [40, 62, 112],
                ink: [36, 58, 118],
                grain: false,
            },
            // Franja encalada con letras marrón oscuro.
            PlaceKind::Church => Self {
                board: [222, 208, 176],
                frame: [104, 76, 52],
                ink: [72, 46, 30],
                grain: false,
            },
            // Tabla ferroviaria: letras negras sobre blanco, marco negro.
            PlaceKind::Station => Self {
                board: [232, 230, 220],
                frame: [30, 28, 26],
                ink: [24, 22, 20],
                grain: false,
            },
            // Madera gastada con letras color crema.
            PlaceKind::Plaza => Self {
                board: [104, 74, 48],
                frame: [62, 44, 30],
                ink: [236, 224, 196],
                grain: true,
            },
        }
    }
}

/// La tabla que pinta `paint_board`, como textura con un rótulo suave.
pub fn board_image(text: &str, layout: BoardLayout, style: BoardStyle, seed: f32) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: layout.width_px as u32,
            height: layout.height_px as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        paint_board(text, layout, style, seed),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    image
}

/// Pinta `text` sobre una tabla del tamaño de `layout`: enmarcada, algo gastada
/// y, si es de madera, con veta. Devuelve píxeles RGBA8 sRGB, fila por fila
/// desde arriba.
pub fn paint_board(text: &str, layout: BoardLayout, style: BoardStyle, seed: f32) -> Vec<u8> {
    let (w, h) = (layout.width_px, layout.height_px);
    let noise =
        |x: usize, y: usize, salt: f32| hash2(x as f32 + seed * 31.7, y as f32 * 1.13 + salt);
    let shade = |c: [u8; 3], f: f32| c.map(|v| (f32::from(v) * f).round().clamp(0.0, 255.0) as u8);

    let mut colors = vec![[0u8; 3]; w * h];
    for y in 0..h {
        // Tablones de madera: cada fila un poco más clara o más oscura.
        let plank = if style.grain {
            0.88 + 0.2 * hash2(y as f32 * 0.7, seed)
        } else {
            1.0
        };
        for x in 0..w {
            let frame = x < BORDER || y < BORDER || x >= w - BORDER || y >= h - BORDER;
            let speckle = 0.94 + 0.12 * noise(x, y, 0.0);
            colors[y * w + x] = if frame {
                shade(style.frame, speckle)
            } else {
                // Un poco de mugre a lo largo del borde interior del marco.
                let edge = x == BORDER || y == BORDER || x == w - BORDER - 1 || y == h - BORDER - 1;
                let grime = if edge { 0.9 } else { 1.0 };
                shade(style.board, speckle * plank * grime)
            };
        }
    }

    // La tinta del rótulo se centra en la tabla, a lo ancho y a lo alto.
    let lettering = lettering_of(text);
    let dx = (w - lettering.width) as f32 / 2.0 - lettering.left;
    let dy = (h - lettering.height) as f32 / 2.0 - lettering.top;
    let (dx, dy) = (dx.round() as isize, dy.round() as isize);
    lettering::rasterize(text, Face::Display, TEXT_PX).for_each(|x, y, alpha| {
        let (px, py) = (x as isize + dx, y as isize + dy);
        if !(0..w as isize).contains(&px) || !(0..h as isize).contains(&py) {
            return;
        }
        let (px, py) = (px as usize, py as usize);
        let worn = noise(px, py, 7.0) < 0.07;
        // La pintura descascarada deja ver la tabla.
        let f = if worn {
            0.4
        } else {
            0.95 + 0.08 * noise(px, py, 3.0)
        };
        let ink = shade(style.ink, if worn { 1.0 } else { f });
        // Los bordes con antialiasing se mezclan con la tabla; la pintura
        // gastada solo cubre a medias.
        let cover = f32::from(alpha) / 255.0 * if worn { f } else { 1.0 };
        let under = colors[py * w + px];
        colors[py * w + px] = std::array::from_fn(|i| {
            (f32::from(ink[i]) * cover + f32::from(under[i]) * (1.0 - cover)).round() as u8
        });
    });

    colors
        .into_iter()
        .flat_map(|[r, g, b]| [r, g, b, 255])
        .collect()
}

/// Dibuja los carteles de los lugares de cada pueblo cuando se construye el
/// pueblo (ver [`crate::TownPlugin`]); cuelgan de su [`TownRoot`] y se van con
/// él.
pub struct SignPlugin;

impl Plugin for SignPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_signs);
    }
}

/// Marca una entidad de cartel y el lugar que nombra.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signboard(pub PlaceKind);

/// Marca el tablero de horarios colgado en la pared del andén de una estación.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimetableBoard;

/// El rótulo del tablero de horarios.
pub const TIMETABLE_BOARD_TEXT: &str = "HORARIOS";

/// Almacenes de assets que necesita un cartel.
type SignAssets<'a> = (
    ResMut<'a, Assets<Image>>,
    ResMut<'a, Assets<Mesh>>,
    ResMut<'a, Assets<StandardMaterial>>,
);

fn spawn_signs(
    mut commands: Commands,
    (towns, terrain): (Res<WorldTowns>, Res<WorldTerrain>),
    roots: Query<(Entity, &TownRoot), Added<TownRoot>>,
    (mut images, mut meshes, mut materials): SignAssets,
) {
    for (root, town) in &roots {
        let Some(town) = towns.0.get(town.0) else {
            continue;
        };
        spawn_town_signs(
            &mut commands,
            root,
            town,
            &terrain.0,
            (&mut images, &mut meshes, &mut materials),
        );
        spawn_timetable_boards(
            &mut commands,
            root,
            town,
            &terrain.0,
            (&mut images, &mut meshes, &mut materials),
        );
    }
}

/// Todos los carteles de `town`, como hijos de `root`.
fn spawn_town_signs(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &pampa_core::Terrain,
    (images, meshes, materials): (
        &mut Assets<Image>,
        &mut Assets<Mesh>,
        &mut Assets<StandardMaterial>,
    ),
) {
    let mut cube = None;
    for place in town.places() {
        let style = BoardStyle::for_place(place.kind);
        for (n, sign) in place.signs.iter().enumerate() {
            let (center, max_height) = mount_height(town, place.kind, sign);
            let layout = layout_board(&place.name, sign.width, max_height);
            let seed = place.kind as u32 as f32 * 3.0 + n as f32;
            let image = board_image(&place.name, layout, style, seed);
            let face = materials.add(StandardMaterial {
                base_color_texture: Some(images.add(image)),
                perceptual_roughness: 0.85,
                reflectance: 0.2,
                ..default()
            });
            let [r, g, b] = style.frame;
            let backing = materials.add(StandardMaterial {
                base_color: Color::srgb_u8(r, g, b),
                perceptual_roughness: 0.95,
                ..default()
            });
            let quad = meshes.add(Rectangle::new(layout.width_m, layout.height_m));
            let cube = cube
                .get_or_insert_with(|| meshes.add(Cuboid::new(1.0, 1.0, 1.0)))
                .clone();

            let base = terrain.height(sign.anchor.x, sign.anchor.y) + sign.elevation;
            let center = match sign.mount {
                SignMount::Facade => center,
                SignMount::Posts => center + layout.height_m / 2.0,
            };
            let front = facing_dir(sign.facing);
            let turn = Quat::from_rotation_y(front.x.atan2(front.z));
            let c = world_from_ground(sign.anchor, base + center);
            let (w, h) = (layout.width_m, layout.height_m);
            let mut parts = vec![(
                cube.clone(),
                backing.clone(),
                Transform::from_translation(c - front * 0.05)
                    .with_rotation(turn)
                    .with_scale(Vec3::new(w + 0.1, h + 0.1, 0.08)),
            )];
            parts.push((
                quad.clone(),
                face.clone(),
                Transform::from_translation(c).with_rotation(turn),
            ));
            if sign.mount == SignMount::Posts {
                // Legible desde ambos lados, sobre dos postes detrás de la
                // tabla.
                parts.push((
                    quad,
                    face,
                    Transform::from_translation(c - front * 0.1)
                        .with_rotation(turn * Quat::from_rotation_y(std::f32::consts::PI)),
                ));
                let right = turn * Vec3::X;
                let top = center + h / 2.0;
                for side in [-1.0, 1.0] {
                    let foot = world_from_ground(sign.anchor, base);
                    parts.push((
                        cube.clone(),
                        backing.clone(),
                        Transform::from_translation(
                            foot + right * side * (w / 2.0 - 0.25) - front * 0.05
                                + Vec3::Y * top / 2.0,
                        )
                        .with_rotation(turn)
                        .with_scale(Vec3::new(0.14, top, 0.14)),
                    ));
                }
            }
            commands
                .spawn((
                    Name::new(format!("Sign {}", place.name)),
                    Signboard(place.kind),
                    Transform::default(),
                    Visibility::default(),
                    ChildOf(root),
                ))
                .with_children(|parent| {
                    for (mesh, material, transform) in parts {
                        parent.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform));
                    }
                });
        }
    }
}

/// Los tableros de horarios de la estación de `town`, si tiene una, como hijos
/// de `root`: pequeñas tablas de época en el estilo de cartel de la estación,
/// en las paredes entre las aberturas (ver [`TownLayout::timetable_boards`]).
fn spawn_timetable_boards(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &pampa_core::Terrain,
    (images, meshes, materials): (
        &mut Assets<Image>,
        &mut Assets<Mesh>,
        &mut Assets<StandardMaterial>,
    ),
) {
    let style = BoardStyle::for_place(PlaceKind::Station);
    for (n, sign) in town.timetable_boards().into_iter().enumerate() {
        let layout = layout_board(TIMETABLE_BOARD_TEXT, sign.width, 0.8);
        let image = board_image(TIMETABLE_BOARD_TEXT, layout, style, 7.0 + n as f32);
        let face = materials.add(StandardMaterial {
            base_color_texture: Some(images.add(image)),
            perceptual_roughness: 0.85,
            reflectance: 0.2,
            ..default()
        });
        let [r, g, b] = style.frame;
        let backing = materials.add(StandardMaterial {
            base_color: Color::srgb_u8(r, g, b),
            perceptual_roughness: 0.95,
            ..default()
        });
        let front = facing_dir(sign.facing);
        let turn = Quat::from_rotation_y(front.x.atan2(front.z));
        let base = terrain.height(sign.anchor.x, sign.anchor.y) + sign.elevation;
        let c = world_from_ground(sign.anchor, base + TIMETABLE_BOARD_HEIGHT);
        let (w, h) = (layout.width_m, layout.height_m);
        commands
            .spawn((
                Name::new(format!("Timetable board {}", town.name)),
                TimetableBoard,
                Transform::default(),
                Visibility::default(),
                ChildOf(root),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
                    MeshMaterial3d(backing),
                    Transform::from_translation(c - front * 0.05)
                        .with_rotation(turn)
                        .with_scale(Vec3::new(w + 0.1, h + 0.1, 0.08)),
                ));
                parent.spawn((
                    Mesh3d(meshes.add(Rectangle::new(w, h))),
                    MeshMaterial3d(face),
                    Transform::from_translation(c).with_rotation(turn),
                ));
            });
    }
}

/// Dirección hacia afuera, en el espacio del mundo, de un lado de una huella
/// (el norte de core es -Z).
fn facing_dir(side: Side) -> Vec3 {
    match side {
        Side::North => Vec3::NEG_Z,
        Side::South => Vec3::Z,
        Side::East => Vec3::X,
        Side::West => Vec3::NEG_X,
    }
}

/// Altura del centro de una tabla de fachada sobre el suelo (o del borde
/// inferior de una tabla sobre postes sobre su base) y la altura máxima que
/// puede tener la tabla, elegidas para que encajen con el aspecto de cada
/// edificio en [`crate::town`].
fn mount_height(town: &TownLayout, kind: PlaceKind, sign: &Sign) -> (f32, f32) {
    if sign.mount == SignMount::Posts {
        return if sign.elevation > 0.0 {
            (1.6, 1.2)
        } else {
            (1.2, 1.0)
        };
    }
    let wall = |wanted: BuildingKind| {
        town.buildings
            .iter()
            .find(|b| b.kind == wanted)
            .map_or(4.0, |b| b.height)
    };
    match kind {
        // En el parapeto de falsa fachada sobre la galería.
        PlaceKind::GeneralStore => (wall(BuildingKind::GeneralStore) + 1.1, 1.8),
        // Sobre la puerta.
        PlaceKind::PoliceStation => (3.45, 1.4),
        // En el campanario, sobre la puerta.
        PlaceKind::Church => (4.2, 1.2),
        PlaceKind::Station => (3.8, 1.6),
        PlaceKind::Plaza => (1.2, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], layout: BoardLayout, x: usize, y: usize) -> [u8; 3] {
        let i = (y * layout.width_px + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    }

    fn distance(a: [u8; 3], b: [u8; 3]) -> i32 {
        a.iter()
            .zip(b)
            .map(|(&x, y)| (i32::from(x) - i32::from(y)).abs())
            .sum()
    }

    /// Tamaño entintado de `text` en la tipografía de las tablas, como píxeles
    /// (ancho, alto).
    fn ink_size(text: &str) -> (f32, f32) {
        let ink = lettering::measure(text, Face::Display, TEXT_PX)
            .ink
            .unwrap();
        (ink.max_x - ink.min_x, ink.max_y - ink.min_y)
    }

    #[test]
    fn boards_fit_their_text_with_square_pixels() {
        let margin = 2 * (BORDER + PADDING);
        for kind in PlaceKind::ALL {
            let text = kind.name();
            let (ink_w, ink_h) = ink_size(text);
            // Rótulo nítido: mayúsculas de al menos 30 píxeles de textura de
            // alto.
            assert!(ink_h >= 30.0, "{text} {ink_h}");
            for (width, max_height) in [(13.0, 1.8), (3.4, 1.2), (7.0, 1.0), (1.0, 2.0)] {
                let l = layout_board(text, width, max_height);
                // El texto entintado más el marco y el relleno caben, y no
                // mucho más: píxeles enteros en ambos sentidos.
                assert!(l.width_px >= ink_w.ceil() as usize + margin, "{text}");
                assert!(l.height_px >= ink_h.ceil() as usize + margin, "{text}");
                assert!(l.height_px <= ink_h.ceil() as usize + 1 + margin, "{text}");
                assert_eq!(l.width_m, width);
                assert!(l.height_m <= max_height + 1e-4, "{text} {l:?}");
                let (px_w, px_h) = (
                    l.width_m / l.width_px as f32,
                    l.height_m / l.height_px as f32,
                );
                assert!((px_w - px_h).abs() < 1e-4, "{text} {l:?}");
            }
        }
    }

    #[test]
    fn board_sizes_in_meters_stay_close_to_the_bitmap_boards() {
        // Alturas que tenían las tablas de mapa de bits de 5x7, en metros, para
        // una tabla del ancho y el límite de altura dados. Las tablas nuevas
        // rotulan en Rye, cuyas mayúsculas son más anchas y pesadas, así que
        // los tamaños pueden diferir hasta en un quinto.
        for (text, width, max_height, before) in [
            ("PLAZA", 3.0, 1.0, 1.0),
            ("ESTACIÓN", 7.0, 1.4, 1.4),
            (PlaceKind::PoliceStation.name(), 6.0, 1.8, 1.791),
            (PlaceKind::GeneralStore.name(), 6.0, 1.8, 0.727),
            (TIMETABLE_BOARD_TEXT, 2.0, 0.8, 0.702),
            ("12 KM", 1.6, 0.34, 0.337),
        ] {
            let l = layout_board(text, width, max_height);
            assert!(l.height_m <= max_height + 1e-4, "{text} {l:?}");
            assert!(
                (l.height_m - before).abs() <= before * 0.2,
                "{text}: {} m tall, was {before} m",
                l.height_m
            );
        }
    }

    #[test]
    fn a_wide_board_is_as_tall_as_allowed() {
        let l = layout_board("PLAZA", 3.0, 1.0);
        assert!((l.height_m - 1.0).abs() < 0.06, "{l:?}");
        // Un nombre largo en una tabla corta reduce en cambio su altura.
        let long = layout_board(PlaceKind::GeneralStore.name(), 6.0, 1.8);
        assert!(long.height_m < 1.0, "{long:?}");
    }

    #[test]
    fn painted_board_carries_its_blended_lettering_centered_in_ink() {
        let text = PlaceKind::PoliceStation.name();
        let style = BoardStyle::for_place(PlaceKind::PoliceStation);
        let l = layout_board(text, 7.0, 1.4);
        let pixels = paint_board(text, l, style, 1.0);
        assert_eq!(pixels.len(), l.width_px * l.height_px * 4);
        assert!(pixels.chunks(4).all(|p| p[3] == 255), "opaque");

        let coverage = lettering::rasterize(text, Face::Display, TEXT_PX);
        let (mut solid, mut partial) = (0, 0);
        coverage.for_each(|_, _, a| {
            if a >= 230 {
                solid += 1;
            } else {
                partial += 1;
            }
        });
        let at = |x: usize, y: usize| pixel(&pixels, l, x, y);
        let near_ink = |c: [u8; 3]| distance(c, style.ink) < distance(c, style.board);
        let all = || (0..l.height_px).flat_map(|y| (0..l.width_px).map(move |x| (x, y)));
        let inside = |&(x, y): &(usize, usize)| {
            x >= BORDER && y >= BORDER && x < l.width_px - BORDER && y < l.height_px - BORDER
        };
        let inked: Vec<_> = all()
            .filter(inside)
            .filter(|&(x, y)| near_ink(at(x, y)))
            .collect();
        // La mayoría de los píxeles de las letras sobreviven al desgaste, y
        // nada más parece tinta.
        assert!(inked.len() * 10 >= solid * 8, "{} of {solid}", inked.len());
        assert!(inked.len() <= solid + partial);
        // Los bordes con antialiasing mezclan la tinta con la tabla: colores
        // intermedios.
        let blended = all()
            .filter(inside)
            .filter(|&(x, y)| {
                let c = at(x, y);
                distance(c, style.ink) > 60 && distance(c, style.board) > 60
            })
            .count();
        assert!(blended * 2 >= partial, "{blended} blended of {partial}");
        // Centrado horizontalmente con un margen de dos píxeles.
        let (min_x, max_x) = inked
            .iter()
            .fold((usize::MAX, 0), |(lo, hi), &(x, _)| (lo.min(x), hi.max(x)));
        let (left, right) = (min_x, l.width_px - 1 - max_x);
        assert!(left.abs_diff(right) <= 2, "left {left} right {right}");
        // Las filas entintadas se centran verticalmente, acentos incluidos.
        let (min_y, max_y) = inked
            .iter()
            .fold((usize::MAX, 0), |(lo, hi), &(_, y)| (lo.min(y), hi.max(y)));
        assert!(
            min_y.abs_diff(l.height_px - 1 - max_y) <= 2,
            "{min_y} {max_y}"
        );
        // El rótulo es tan grande como lo permite la tipografía, no una mota.
        let (ink_w, ink_h) = ink_size(text);
        assert!((max_x - min_x) as f32 >= ink_w - 6.0);
        assert!((max_y - min_y) as f32 >= ink_h - 6.0);
    }

    #[test]
    fn the_frame_is_painted_around_the_edge() {
        let style = BoardStyle::for_place(PlaceKind::Station);
        let l = layout_board("IGLESIA", 3.4, 1.2);
        let pixels = paint_board("IGLESIA", l, style, 2.0);
        for (x, y) in [
            (0, 0),
            (l.width_px - 1, 0),
            (0, l.height_px - 1),
            (l.width_px / 2, 1),
        ] {
            let c = pixel(&pixels, l, x, y);
            assert!(
                distance(c, style.frame) < distance(c, style.board),
                "({x}, {y})"
            );
        }
    }

    #[test]
    fn painting_is_deterministic_and_worn() {
        let style = BoardStyle::for_place(PlaceKind::Plaza);
        let l = layout_board("PLAZA", 3.0, 1.0);
        let a = paint_board("PLAZA", l, style, 3.0);
        assert_eq!(a, paint_board("PLAZA", l, style, 3.0));
        // El desgaste varía la superficie: la tabla no es de un único color
        // liso.
        let row = l.height_px / 2;
        let colors: std::collections::HashSet<_> = (BORDER..l.width_px - BORDER)
            .map(|x| pixel(&a, l, x, row))
            .collect();
        assert!(colors.len() > 3);
    }

    #[test]
    fn spawns_one_board_per_sign_facing_its_street() {
        let town = TownLayout::castex_inspired();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTerrain(pampa_core::Terrain::default()))
            .insert_resource(WorldTowns(vec![town.clone()]))
            .add_plugins((crate::TownPlugin, SignPlugin));
        app.update();

        let signs: usize = town.places().iter().map(|p| p.signs.len()).sum();
        let mut boards = app.world_mut().query::<(&Signboard, &Children)>();
        let found: Vec<_> = boards
            .iter(app.world())
            .map(|(b, children)| (b.0, children.len()))
            .collect();
        assert_eq!(found.len(), signs);
        for (kind, parts) in &found {
            // Respaldo y frente; las tablas sobre postes agregan una cara
            // trasera y dos postes.
            assert!(*parts == 2 || *parts == 5, "{kind:?} has {parts} parts");
        }
        // Tres imágenes más: los tableros de horarios de la estación.
        assert_eq!(app.world().resource::<Assets<Image>>().len(), signs + 3);

        // Cada quad de frente (normal +Z) apunta hacia donde mira su cartel.
        let mut faces = app
            .world_mut()
            .query_filtered::<(&Transform, &MeshMaterial3d<StandardMaterial>), Without<Signboard>>(
            );
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let normals: Vec<Vec3> = faces
            .iter(app.world())
            .filter(|(_, m)| {
                materials
                    .get(&m.0)
                    .is_some_and(|m| m.base_color_texture.is_some())
            })
            .map(|(t, _)| t.rotation * Vec3::Z)
            .collect();
        for place in town.places() {
            for sign in &place.signs {
                let dir = facing_dir(sign.facing);
                assert!(
                    normals.iter().any(|n| n.distance(dir) < 1e-4),
                    "{:?} faces {:?}",
                    place.kind,
                    sign.facing
                );
            }
        }
    }

    #[test]
    fn every_station_gets_three_boards_each_clear_of_doors_and_windows() {
        let towns = vec![TownLayout::castex_inspired()];
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTerrain(pampa_core::Terrain::default()))
            .insert_resource(WorldTowns(towns.clone()))
            .add_plugins((crate::TownPlugin, SignPlugin));
        app.update();

        let mut boards = app
            .world_mut()
            .query_filtered::<&Children, With<TimetableBoard>>();
        assert_eq!(boards.iter(app.world()).count(), 3 * towns.len());
        let mut faces = app
            .world_mut()
            .query_filtered::<(&Transform, &MeshMaterial3d<StandardMaterial>), Without<Signboard>>(
            );
        for town in &towns {
            let station = town
                .buildings
                .iter()
                .find(|b| b.kind == BuildingKind::Station)
                .unwrap();
            let signs = town.timetable_boards();
            assert_eq!(signs.len(), 3);
            // Uno mirando al andén, dos mirando a la calle.
            let facing: Vec<Side> = signs.iter().map(|s| s.facing).collect();
            assert_eq!(
                facing,
                [station.front.opposite(), station.front, station.front]
            );
            for sign in signs {
                // Junto a cada puerta y ventana: ninguna de ellas alcanza la
                // tabla.
                let n = pampa_core::places::side_normal(sign.facing);
                let along = pampa_core::Vec2::new(n.y.abs(), n.x.abs());
                let x = (sign.anchor - station.footprint.center()).dot(along);
                let size = station.footprint.size();
                let openings = crate::facade::town_buildings::station_openings((
                    size.dot(along),
                    size.dot(pampa_core::Vec2::new(n.x.abs(), n.y.abs())),
                    station.height,
                ));
                assert!(openings.len() >= 3);
                // Las aberturas se disponen igual desde cualquiera de los dos
                // extremos.
                for (a, b) in openings {
                    for x in [x, -x] {
                        let (lo, hi) = (x - sign.width / 2.0, x + sign.width / 2.0);
                        assert!(
                            hi < a - 0.2 || lo > b + 0.2,
                            "board {lo}..{hi} meets {a}..{b}"
                        );
                    }
                }
                // Su quad de frente apunta hacia donde mira la tabla, 1.7 m por
                // encima del piso sobre el que se apoya (0.9 m para la del
                // andén).
                let dir = facing_dir(sign.facing);
                let ground = pampa_core::Terrain::default().height(sign.anchor.x, sign.anchor.y);
                let floor = if sign.facing == station.front {
                    0.0
                } else {
                    0.9
                };
                let hung = world_from_ground(sign.anchor, ground + floor + 1.7);
                let found = faces
                    .iter(app.world())
                    .filter(|(t, _)| (t.rotation * Vec3::Z).distance(dir) < 1e-4)
                    .any(|(t, _)| t.translation.distance(hung) < 1e-3);
                assert!(
                    found,
                    "a board hangs 1.7 m over its floor at {:?}",
                    sign.anchor
                );
            }
        }
    }
}
