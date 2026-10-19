//! Painted signboards naming the town's landmarks.
//!
//! `pampa-core` decides which places have signs, where they stand and which
//! way they read ([`pampa_core::Place`]). This module lays the lettering out
//! in the bitmap [`font`](crate::font), paints a weathered board texture and
//! mounts it on the facade or on posts.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{BuildingKind, PlaceKind, Side, Sign, SignMount, TownLayout};

use crate::coords::world_from_ground;
use crate::font::{self, GLYPH_HEIGHT};
use crate::terrain::{WorldTerrain, hash2};
use crate::town::{TownRoot, WorldTowns};

/// Painted frame width around a board, in texture pixels.
pub const BORDER: usize = 2;
/// Clear space between the frame and the lettering, in texture pixels.
pub const PADDING: usize = 3;

/// Pixel size of a board texture and its size in meters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardLayout {
    pub width_px: usize,
    pub height_px: usize,
    pub width_m: f32,
    pub height_m: f32,
}

/// Lays out a one-line board `width_m` wide and at most `max_height_m` tall
/// with square pixels: the lettering is as large as fits, centered, and the
/// board is never narrower than the text it carries.
pub fn layout_board(text: &str, width_m: f32, max_height_m: f32) -> BoardLayout {
    let margin = 2 * (BORDER + PADDING);
    let height_px = font::line_rows(text) + margin;
    let min_width_px = font::text_width(text) + margin;
    // The largest pixel that fits both the text across and the height limit;
    // rounding the pixel count up only ever shrinks the pixel.
    let pixel = (width_m / min_width_px as f32).min(max_height_m / height_px as f32);
    let width_px = ((width_m / pixel).ceil() as usize).max(min_width_px);
    BoardLayout {
        width_px,
        height_px,
        width_m,
        height_m: width_m * height_px as f32 / width_px as f32,
    }
}

/// Colors of one board style (sRGB).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardStyle {
    pub board: [u8; 3],
    pub frame: [u8; 3],
    pub ink: [u8; 3],
    /// Horizontal wood grain instead of a flat painted surface.
    pub grain: bool,
}

impl BoardStyle {
    /// A period-appropriate look per landmark.
    pub fn for_place(kind: PlaceKind) -> Self {
        match kind {
            // Dark green fascia with gold letters.
            PlaceKind::GeneralStore => Self {
                board: [38, 64, 46],
                frame: [92, 66, 40],
                ink: [214, 178, 92],
                grain: false,
            },
            // White enamel with blue letters.
            PlaceKind::PoliceStation => Self {
                board: [228, 224, 210],
                frame: [40, 62, 112],
                ink: [36, 58, 118],
                grain: false,
            },
            // Lime-washed band with dark brown letters.
            PlaceKind::Church => Self {
                board: [222, 208, 176],
                frame: [104, 76, 52],
                ink: [72, 46, 30],
                grain: false,
            },
            // Railway board: black letters on white, black frame.
            PlaceKind::Station => Self {
                board: [232, 230, 220],
                frame: [30, 28, 26],
                ink: [24, 22, 20],
                grain: false,
            },
            // Weathered timber with cream letters.
            PlaceKind::Plaza => Self {
                board: [104, 74, 48],
                frame: [62, 44, 30],
                ink: [236, 224, 196],
                grain: true,
            },
        }
    }
}

/// Paints `text` on a board of `layout` size: framed, slightly worn and, for
/// timber, grained. Returns RGBA8 sRGB pixels, row by row from the top.
pub fn paint_board(text: &str, layout: BoardLayout, style: BoardStyle, seed: f32) -> Vec<u8> {
    let (w, h) = (layout.width_px, layout.height_px);
    let noise =
        |x: usize, y: usize, salt: f32| hash2(x as f32 + seed * 31.7, y as f32 * 1.13 + salt);
    let shade = |c: [u8; 3], f: f32| c.map(|v| (f32::from(v) * f).round().clamp(0.0, 255.0) as u8);

    let mut colors = vec![[0u8; 3]; w * h];
    for y in 0..h {
        // Timber planks: each row a little lighter or darker.
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
                // A little grime along the inside of the frame.
                let edge = x == BORDER || y == BORDER || x == w - BORDER - 1 || y == h - BORDER - 1;
                let grime = if edge { 0.9 } else { 1.0 };
                shade(style.board, speckle * plank * grime)
            };
        }
    }

    let left = (w - font::text_width(text)) / 2;
    let (first, last) = font::ink_rows(text).unwrap_or((0, GLYPH_HEIGHT - 1));
    let top = (h - (last - first + 1)) / 2;
    font::for_each_pixel(text, |x, y| {
        let (px, py) = (left + x, top + y - first);
        let worn = noise(px, py, 7.0) < 0.07;
        // Flaked paint lets the board show through.
        let f = if worn {
            0.4
        } else {
            0.95 + 0.08 * noise(px, py, 3.0)
        };
        let ink = shade(style.ink, if worn { 1.0 } else { f });
        let under = colors[py * w + px];
        colors[py * w + px] = if worn {
            std::array::from_fn(|i| {
                (f32::from(ink[i]) * f + f32::from(under[i]) * (1.0 - f)).round() as u8
            })
        } else {
            ink
        };
    });

    colors
        .into_iter()
        .flat_map(|[r, g, b]| [r, g, b, 255])
        .collect()
}

/// Draws each town's place signs when the town is built (see
/// [`crate::TownPlugin`]); they hang off its [`TownRoot`] and go with it.
pub struct SignPlugin;

impl Plugin for SignPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, spawn_signs);
    }
}

/// Marks a signboard entity and the place it names.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signboard(pub PlaceKind);

/// Asset stores a sign needs.
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
    }
}

/// Every sign of `town`, as children of `root`.
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
            let mut image = Image::new(
                Extent3d {
                    width: layout.width_px as u32,
                    height: layout.height_px as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                paint_board(&place.name, layout, style, seed),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            // Crisp pixel lettering.
            image.sampler = ImageSampler::nearest();
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
                // Readable from both sides, on two posts behind the board.
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

/// World-space outward direction of a footprint side (core north is -Z).
fn facing_dir(side: Side) -> Vec3 {
    match side {
        Side::North => Vec3::NEG_Z,
        Side::South => Vec3::Z,
        Side::East => Vec3::X,
        Side::West => Vec3::NEG_X,
    }
}

/// Height of a facade board's center above the ground (or of a post board's
/// bottom edge above its base) and the tallest the board may be, chosen to
/// fit each building's look in [`crate::town`].
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
        // On the false-front parapet above the veranda.
        PlaceKind::GeneralStore => (wall(BuildingKind::GeneralStore) + 1.1, 1.8),
        // Above the door.
        PlaceKind::PoliceStation => (3.45, 1.4),
        // On the bell tower, above the door.
        PlaceKind::Church => (4.2, 1.2),
        PlaceKind::Station => (3.8, 1.6),
        PlaceKind::Plaza => (1.2, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::{CAP_TOP, text_width};

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

    #[test]
    fn boards_fit_their_text_with_square_pixels() {
        for kind in PlaceKind::ALL {
            let text = kind.name();
            for (width, max_height) in [(13.0, 1.8), (3.4, 1.2), (7.0, 1.0), (1.0, 2.0)] {
                let l = layout_board(text, width, max_height);
                assert!(
                    l.width_px >= text_width(text) + 2 * (BORDER + PADDING),
                    "{text}"
                );
                assert_eq!(l.height_px, font::ASCENT + 2 * (BORDER + PADDING));
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
    fn a_wide_board_is_as_tall_as_allowed() {
        let l = layout_board("PLAZA", 3.0, 1.0);
        assert!((l.height_m - 1.0).abs() < 0.06, "{l:?}");
        // A long name on a short board shrinks its height instead.
        let long = layout_board(PlaceKind::GeneralStore.name(), 6.0, 1.8);
        assert!(long.height_m < 1.0, "{long:?}");
    }

    #[test]
    fn painted_board_carries_its_lettering_centered_in_ink() {
        let text = PlaceKind::PoliceStation.name();
        let style = BoardStyle::for_place(PlaceKind::PoliceStation);
        let l = layout_board(text, 7.0, 1.4);
        let pixels = paint_board(text, l, style, 1.0);
        assert_eq!(pixels.len(), l.width_px * l.height_px * 4);
        assert!(pixels.chunks(4).all(|p| p[3] == 255), "opaque");

        let mut ink_bits = 0;
        font::for_each_pixel(text, |_, _| ink_bits += 1);
        let near =
            |c: [u8; 3], target: [u8; 3], other: [u8; 3]| distance(c, target) < distance(c, other);
        let inked = (0..l.height_px)
            .flat_map(|y| (0..l.width_px).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let inside = x >= BORDER
                    && y >= BORDER
                    && x < l.width_px - BORDER
                    && y < l.height_px - BORDER;
                inside && near(pixel(&pixels, l, x, y), style.ink, style.board)
            })
            .collect::<Vec<_>>();
        // Most letter pixels survive the wear, and nothing else looks like ink.
        assert!(
            inked.len() * 10 >= ink_bits * 8,
            "{} of {ink_bits}",
            inked.len()
        );
        assert!(inked.len() <= ink_bits);
        // Centered horizontally within a pixel.
        let (min_x, max_x) = inked
            .iter()
            .fold((usize::MAX, 0), |(lo, hi), &(x, _)| (lo.min(x), hi.max(x)));
        let (left, right) = (min_x, l.width_px - 1 - max_x);
        assert!(left.abs_diff(right) <= 1, "left {left} right {right}");
        // The inked rows are centered vertically, accents included.
        let (min_y, max_y) = inked
            .iter()
            .fold((usize::MAX, 0), |(lo, hi), &(_, y)| (lo.min(y), hi.max(y)));
        assert!(
            min_y.abs_diff(l.height_px - 1 - max_y) <= 1,
            "{min_y} {max_y}"
        );
        assert!(max_y - min_y >= font::ASCENT - CAP_TOP);
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
        // Wear varies the surface: the board is not one flat color.
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
            // Backing and face; post boards add a back face and two posts.
            assert!(*parts == 2 || *parts == 5, "{kind:?} has {parts} parts");
        }
        assert_eq!(app.world().resource::<Assets<Image>>().len(), signs);

        // Every face quad (+Z normal) points the way its sign faces.
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
}
