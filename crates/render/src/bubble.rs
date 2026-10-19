//! Speech bubbles: a line of dialogue floating above somebody's head.
//!
//! The game writes a [`Say`] message naming the speaker entity and the line;
//! this module lays the line out in the bitmap [`font`](crate::font), paints
//! a paper bubble with an ink outline and a tail pointing down, and shows it
//! above the speaker, always facing the camera, for [`LIFETIME`] seconds
//! with a fade over the last [`FADE`] seconds. A speaker has at most one
//! bubble: a new line replaces the old one.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::font::{self, BASELINE, GLYPH_HEIGHT, GLYPH_WIDTH, X_TOP};

/// Most characters on one line of a bubble before it wraps.
pub const BUBBLE_CHARS: usize = 16;
/// Seconds a bubble stays up.
pub const LIFETIME: f32 = 3.0;
/// Seconds at the end of [`LIFETIME`] over which the bubble fades out.
pub const FADE: f32 = 0.6;
/// Height of the tail's tip above the speaker's feet, in meters.
pub const HEIGHT: f32 = 2.2;
/// Size of one texture pixel in the world, in meters.
pub const METERS_PER_PIXEL: f32 = 0.022;

/// Ink outline width, in texture pixels.
const OUTLINE: usize = 1;
/// Paper between the outline and the lettering, across and down.
const PADDING_X: usize = 4;
const PADDING_Y: usize = 3;
/// Radius of the rounded corners, in texture pixels.
const CORNER: f32 = 3.0;
/// Rows the tail hangs below the bubble.
const TAIL: usize = 4;
/// Vertical distance between two lines of text, in pixels.
const LINE_PITCH: usize = GLYPH_HEIGHT;

/// Paper and ink colors (sRGB).
pub const PAPER: [u8; 3] = [241, 233, 212];
pub const INK: [u8; 3] = [46, 36, 28];

/// Splits `text` into lines of at most `max_chars` characters, breaking at
/// spaces (runs of spaces collapse) and splitting words longer than a line.
pub fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;
    for word in text.split_whitespace() {
        let mut chars: Vec<char> = word.chars().collect();
        // Too long for any line: fill lines with pieces of it.
        while chars.len() > max_chars {
            if current_len > 0 {
                lines.push(std::mem::take(&mut current));
                current_len = 0;
            }
            lines.push(chars.drain(..max_chars).collect());
        }
        let len = chars.len();
        if current_len > 0 && current_len + 1 + len > max_chars {
            lines.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if current_len > 0 {
            current.push(' ');
            current_len += 1;
        }
        current.extend(chars);
        current_len += len;
    }
    if current_len > 0 {
        lines.push(current);
    }
    lines
}

/// Pixel layout of one bubble texture.
#[derive(Debug, Clone, PartialEq)]
pub struct BubbleLayout {
    /// The wrapped lines, top to bottom.
    pub lines: Vec<String>,
    pub width_px: usize,
    /// Texture height: the body plus the tail.
    pub height_px: usize,
    /// Height of the rounded body, outline included.
    pub body_height_px: usize,
    /// Texture row of the top of the first line's glyph cells (negative
    /// when the cell's blank accent rows rise above the texture).
    pub text_top: isize,
}

impl BubbleLayout {
    /// Size in the world, in meters (width, height).
    pub fn size_m(&self) -> Vec2 {
        Vec2::new(self.width_px as f32, self.height_px as f32) * METERS_PER_PIXEL
    }
}

/// Lays out `text` in a bubble just large enough for its wrapped lines.
pub fn layout_bubble(text: &str) -> BubbleLayout {
    let lines = wrap(text, BUBBLE_CHARS);
    let text_width = lines.iter().map(|l| font::text_width(l)).max().unwrap_or(0);
    // Inked rows of the whole block, so blank accent and descender rows do
    // not pad the bubble unevenly.
    let first = lines
        .first()
        .and_then(|l| font::ink_rows(l))
        .map_or(X_TOP, |(first, _)| first);
    let last = lines
        .last()
        .and_then(|l| font::ink_rows(l))
        .map_or(BASELINE, |(_, last)| last)
        + LINE_PITCH * lines.len().saturating_sub(1);
    let ink_height = last.max(first) - first + 1;
    let width_px = text_width.max(GLYPH_WIDTH) + 2 * (OUTLINE + PADDING_X);
    let body_height_px = ink_height + 2 * (OUTLINE + PADDING_Y);
    BubbleLayout {
        lines,
        width_px,
        height_px: body_height_px + TAIL,
        body_height_px,
        text_top: (OUTLINE + PADDING_Y) as isize - first as isize,
    }
}

/// Paints the bubble of `layout`: RGBA8 sRGB pixels, row by row from the
/// top, transparent outside the bubble and its tail.
pub fn paint_bubble(layout: &BubbleLayout) -> Vec<u8> {
    let (w, h, body) = (layout.width_px, layout.height_px, layout.body_height_px);
    let mut colors: Vec<Option<[u8; 3]>> = vec![None; w * h];

    // Rounded body: ink where the outer shape is but the inset one is not.
    let outline = OUTLINE as f32;
    for y in 0..body {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            if rounded(px, py, w as f32, body as f32, CORNER) {
                let inner = rounded(
                    px - outline,
                    py - outline,
                    w as f32 - 2.0 * outline,
                    body as f32 - 2.0 * outline,
                    CORNER - outline,
                );
                colors[y * w + x] = Some(if inner { PAPER } else { INK });
            }
        }
    }

    // Tail: a triangle hanging from the bottom edge, opening it.
    let center = w as f32 / 2.0;
    for t in 0..=TAIL {
        let y = body - OUTLINE + t;
        if y >= h {
            break;
        }
        let half = (TAIL - t) as f32 + 0.5;
        for x in 0..w {
            let d = (x as f32 + 0.5 - center).abs();
            if d <= half {
                let edge = d > half - 1.0 || t == TAIL;
                colors[y * w + x] = Some(if edge { INK } else { PAPER });
            }
        }
    }

    // Lettering, each line centered.
    for (n, line) in layout.lines.iter().enumerate() {
        let left = (w - font::text_width(line)) / 2;
        let top = layout.text_top + (n * LINE_PITCH) as isize;
        font::for_each_pixel(line, |x, y| {
            let py = top + y as isize;
            if (0..h as isize).contains(&py) {
                colors[py as usize * w + left + x] = Some(INK);
            }
        });
    }

    colors
        .into_iter()
        .flat_map(|c| match c {
            Some([r, g, b]) => [r, g, b, 255],
            None => [0, 0, 0, 0],
        })
        .collect()
}

/// Whether point `(x, y)` lies in a `w` x `h` rectangle at the origin with
/// corners rounded by `radius`.
fn rounded(x: f32, y: f32, w: f32, h: f32, radius: f32) -> bool {
    if x < 0.0 || y < 0.0 || x > w || y > h {
        return false;
    }
    let cx = x.clamp(radius, w - radius);
    let cy = y.clamp(radius, h - radius);
    (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}

/// Opacity of a bubble `age` seconds old: opaque, then fading to zero over
/// the last [`FADE`] seconds of its [`LIFETIME`].
pub fn bubble_alpha(age: f32) -> f32 {
    if age <= LIFETIME - FADE {
        1.0
    } else {
        ((LIFETIME - age) / FADE).clamp(0.0, 1.0)
    }
}

/// Asks for `line` to be shown above `speaker`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct Say {
    pub speaker: Entity,
    pub line: String,
}

/// A bubble on screen: a child of its speaker.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SpeechBubble {
    pub speaker: Entity,
    pub line: String,
    /// Seconds since it appeared.
    pub age: f32,
}

/// Shows [`Say`] lines as speech bubbles.
pub struct SpeechBubblePlugin;

impl Plugin for SpeechBubblePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Say>()
            .init_resource::<BubbleAssets>()
            .add_systems(
                PostUpdate,
                (show_bubbles, age_bubbles, face_camera)
                    .chain()
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Bubble textures by line, and the quad they are drawn on.
#[derive(Resource, Default)]
struct BubbleAssets {
    quad: Option<Handle<Mesh>>,
    textures: HashMap<String, (Handle<Image>, Vec2)>,
}

impl BubbleAssets {
    /// The texture of `line` and its size in meters, painted on first use.
    fn texture(&mut self, line: &str, images: &mut Assets<Image>) -> (Handle<Image>, Vec2) {
        if let Some(found) = self.textures.get(line) {
            return found.clone();
        }
        let layout = layout_bubble(line);
        let mut image = Image::new(
            Extent3d {
                width: layout.width_px as u32,
                height: layout.height_px as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            paint_bubble(&layout),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        // Crisp pixel lettering.
        image.sampler = ImageSampler::nearest();
        let entry = (images.add(image), layout.size_m());
        self.textures.insert(line.to_string(), entry.clone());
        entry
    }
}

/// Asset stores a bubble is built from.
type BubbleStores<'w> = (
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<StandardMaterial>>,
);

/// Spawns a bubble for every [`Say`], replacing the speaker's previous one.
fn show_bubbles(
    mut commands: Commands,
    mut said: MessageReader<Say>,
    mut assets: ResMut<BubbleAssets>,
    (mut images, mut meshes, mut materials): BubbleStores,
    speakers: Query<(), With<Transform>>,
    bubbles: Query<(Entity, &SpeechBubble)>,
) {
    let mut replaced = Vec::new();
    for say in said.read() {
        if !speakers.contains(say.speaker) {
            continue;
        }
        // One bubble per speaker: the new line replaces the old one.
        for (entity, bubble) in &bubbles {
            if bubble.speaker == say.speaker && !replaced.contains(&entity) {
                commands.entity(entity).despawn();
                replaced.push(entity);
            }
        }
        let (texture, size) = assets.texture(&say.line, &mut images);
        let quad = assets
            .quad
            .get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0)))
            .clone();
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            // Paper reads the same in sun and shade.
            unlit: true,
            ..default()
        });
        commands.spawn((
            Name::new("Speech bubble"),
            SpeechBubble {
                speaker: say.speaker,
                line: say.line.clone(),
                age: 0.0,
            },
            Mesh3d(quad),
            MeshMaterial3d(material),
            NotShadowCaster,
            Transform::from_xyz(0.0, HEIGHT + size.y / 2.0, 0.0).with_scale(size.extend(1.0)),
            ChildOf(say.speaker),
        ));
    }
}

/// Fades bubbles out and removes them at the end of their life.
fn age_bubbles(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bubbles: Query<(Entity, &mut SpeechBubble, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (entity, mut bubble, material) in &mut bubbles {
        bubble.age += time.delta_secs();
        if bubble.age >= LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }
        // Touch the material only while fading, so it is not re-uploaded
        // every frame.
        let alpha = bubble_alpha(bubble.age);
        if alpha < 1.0
            && let Some(mut material) = materials.get_mut(&material.0)
        {
            material.base_color = Color::WHITE.with_alpha(alpha);
        }
    }
}

/// Turns every bubble around Y to face the camera, staying upright.
fn face_camera(
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    speakers: Query<&GlobalTransform, Without<SpeechBubble>>,
    mut bubbles: Query<(&ChildOf, &mut Transform), With<SpeechBubble>>,
) {
    let Some(camera) = camera else {
        return;
    };
    let eye = camera.translation();
    for (child_of, mut transform) in &mut bubbles {
        let Ok(speaker) = speakers.get(child_of.parent()) else {
            continue;
        };
        let to_camera = eye - speaker.translation();
        let world = Quat::from_rotation_y(to_camera.x.atan2(to_camera.z));
        // Undo the speaker's own turn, if any.
        transform.rotation = speaker.rotation().inverse() * world;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], l: &BubbleLayout, x: usize, y: usize) -> [u8; 4] {
        let i = (y * l.width_px + x) * 4;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    fn rgb(c: [u8; 3]) -> [u8; 4] {
        [c[0], c[1], c[2], 255]
    }

    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    #[test]
    fn wraps_at_spaces_within_the_limit() {
        assert_eq!(
            wrap("¡Eh, mirá por dónde andás!", 16),
            ["¡Eh, mirá por", "dónde andás!"]
        );
        assert_eq!(wrap("corto", 16), ["corto"]);
        assert_eq!(wrap("  a   b  ", 16), ["a b"]);
        assert!(wrap("", 16).is_empty());
        assert!(wrap("   ", 16).is_empty());
        // Exactly the limit fits.
        assert_eq!(wrap("abcd efgh", 9), ["abcd efgh"]);
        assert_eq!(wrap("abcd efgh", 8), ["abcd", "efgh"]);
        // A word longer than a line is split.
        assert_eq!(wrap("abcdefghij xy", 4), ["abcd", "efgh", "ij", "xy"]);
        let text = "¡Más despacio, que no es carrera! ¡Bárbaro! ¡Me llenó de tierra!";
        let lines = wrap(text, BUBBLE_CHARS);
        assert!(lines.iter().all(|l| l.chars().count() <= BUBBLE_CHARS));
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn a_bubble_fits_its_lines_and_stays_small() {
        let one = layout_bubble("¡Epa!");
        assert_eq!(one.lines, ["¡Epa!"]);
        assert!(one.width_px >= font::text_width("¡Epa!") + 2 * (OUTLINE + PADDING_X));
        assert_eq!(one.height_px, one.body_height_px + TAIL);

        // The longest dialogue line (40 characters) needs at most 3 lines.
        let long = layout_bubble("¡Más despacio, que no es carrera, che!!!");
        assert!(long.lines.len() <= 3, "{:?}", long.lines);
        for line in &long.lines {
            assert!(long.width_px >= font::text_width(line) + 2 * (OUTLINE + PADDING_X));
        }
        assert!(long.body_height_px > one.body_height_px);
        let size = long.size_m();
        assert!(size.x < 2.5 && size.y < 1.2, "{size:?}");
        assert_eq!(layout_bubble(""), layout_bubble(" "));
    }

    #[test]
    fn paints_a_rounded_paper_bubble_with_an_outline_and_a_tail() {
        let l = layout_bubble("¡Casi me pisa el pingo, che!");
        let px = paint_bubble(&l);
        assert_eq!(px.len(), l.width_px * l.height_px * 4);
        let (w, body) = (l.width_px, l.body_height_px);
        let mid = w / 2;
        // Rounded corners are clear; the edges are ink.
        for (x, y) in [(0, 0), (w - 1, 0), (0, body - 1), (w - 1, body - 1)] {
            assert_eq!(pixel(&px, &l, x, y), CLEAR, "corner ({x}, {y})");
        }
        assert_eq!(pixel(&px, &l, mid, 0), rgb(INK));
        assert_eq!(pixel(&px, &l, 0, body / 2), rgb(INK));
        assert_eq!(pixel(&px, &l, w - 1, body / 2), rgb(INK));
        // Paper inside, between the outline and the lettering.
        assert_eq!(pixel(&px, &l, OUTLINE, body / 2), rgb(PAPER));
        assert_eq!(pixel(&px, &l, mid, OUTLINE), rgb(PAPER));
        // The tail opens the bottom edge and ends in an ink tip.
        assert_eq!(pixel(&px, &l, mid, body - 1), rgb(PAPER));
        assert_eq!(pixel(&px, &l, mid, l.height_px - 1), rgb(INK));
        assert_eq!(pixel(&px, &l, 0, l.height_px - 1), CLEAR);
        assert_eq!(pixel(&px, &l, w - 1, l.height_px - 1), CLEAR);
        // Only paper, ink and clear pixels.
        for p in px.chunks(4) {
            let p = [p[0], p[1], p[2], p[3]];
            assert!(p == CLEAR || p == rgb(PAPER) || p == rgb(INK), "{p:?}");
        }
    }

    #[test]
    fn letters_every_glyph_pixel_inside_the_paper() {
        let l = layout_bubble("¡Bárbaro! ¡Me llenó de tierra!");
        let px = paint_bubble(&l);
        let ink_bits: usize = l
            .lines
            .iter()
            .map(|line| {
                let mut n = 0;
                font::for_each_pixel(line, |_, _| n += 1);
                n
            })
            .sum();
        // Ink strictly inside the outline is lettering.
        let inner = |x: usize, y: usize| {
            x > OUTLINE + 1
                && x + OUTLINE + 2 < l.width_px
                && y > OUTLINE
                && y + OUTLINE < l.body_height_px - 1
        };
        let lettering = (0..l.body_height_px)
            .flat_map(|y| (0..l.width_px).map(move |x| (x, y)))
            .filter(|&(x, y)| inner(x, y) && pixel(&px, &l, x, y) == rgb(INK))
            .count();
        assert_eq!(lettering, ink_bits);
        // The lettering keeps at least a pixel of paper from the outline.
        let first_ink = font::ink_rows(&l.lines[0]).unwrap().0 as isize;
        assert!(l.text_top + first_ink > OUTLINE as isize);
        let last = l.lines.len() - 1;
        let last_ink = font::ink_rows(&l.lines[last]).unwrap().1;
        let bottom = l.text_top + (last * LINE_PITCH + last_ink) as isize;
        assert!(bottom < (l.body_height_px - OUTLINE - 1) as isize);
    }

    #[test]
    fn fades_out_at_the_end_of_its_life() {
        assert_eq!(bubble_alpha(0.0), 1.0);
        assert_eq!(bubble_alpha(-1.0), 1.0);
        assert_eq!(bubble_alpha(LIFETIME - FADE), 1.0);
        assert!((bubble_alpha(LIFETIME - FADE / 2.0) - 0.5).abs() < 1e-5);
        assert_eq!(bubble_alpha(LIFETIME), 0.0);
        assert_eq!(bubble_alpha(LIFETIME + 5.0), 0.0);
        let mut last = 1.0;
        for i in 0..=30 {
            let a = bubble_alpha(i as f32 * LIFETIME / 30.0);
            assert!(a <= last, "never brightens");
            last = a;
        }
    }

    mod plugin {
        use super::super::*;
        use bevy::time::TimeUpdateStrategy;
        use std::time::Duration;

        fn app() -> App {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                    100,
                )))
                .init_resource::<Assets<Image>>()
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<StandardMaterial>>()
                .add_plugins(SpeechBubblePlugin);
            app
        }

        fn bubbles(app: &mut App) -> Vec<(SpeechBubble, Entity)> {
            let mut q = app.world_mut().query::<(&SpeechBubble, &ChildOf)>();
            q.iter(app.world())
                .map(|(b, c)| (b.clone(), c.parent()))
                .collect()
        }

        fn say(app: &mut App, speaker: Entity, line: &str) {
            app.world_mut().write_message(Say {
                speaker,
                line: line.into(),
            });
        }

        #[test]
        fn shows_one_bubble_per_speaker_and_replaces_it() {
            let mut app = app();
            let a = app.world_mut().spawn(Transform::default()).id();
            let b = app.world_mut().spawn(Transform::default()).id();
            say(&mut app, a, "¡Epa!");
            app.update();
            let shown = bubbles(&mut app);
            assert_eq!(shown.len(), 1);
            assert_eq!((shown[0].0.line.as_str(), shown[0].1), ("¡Epa!", a));

            say(&mut app, a, "¡Che!");
            say(&mut app, b, "¡Epa!");
            app.update();
            let shown = bubbles(&mut app);
            assert_eq!(shown.len(), 2);
            let line_of = |speaker| {
                let (bubble, _) = shown.iter().find(|(_, p)| *p == speaker).unwrap();
                bubble.line.clone()
            };
            assert_eq!(line_of(a), "¡Che!");
            assert_eq!(line_of(b), "¡Epa!");
            // Textures are cached by line.
            assert_eq!(app.world().resource::<Assets<Image>>().len(), 2);
        }

        #[test]
        fn a_bubble_floats_above_the_head_and_fades_away() {
            let mut app = app();
            let a = app.world_mut().spawn(Transform::default()).id();
            say(&mut app, a, "¡Epa!");
            app.update();
            let mut q =
                app.world_mut()
                    .query::<(&Transform, &MeshMaterial3d<StandardMaterial>, &SpeechBubble)>();
            let (t, m, _) = q.single(app.world()).unwrap();
            let size = layout_bubble("¡Epa!").size_m();
            assert!((t.translation.y - size.y / 2.0 - HEIGHT).abs() < 1e-5);
            assert_eq!(t.scale, size.extend(1.0));
            let handle = m.0.clone();
            let alpha = |app: &App| {
                app.world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(&handle)
                    .unwrap()
                    .base_color
                    .alpha()
            };
            assert_eq!(alpha(&app), 1.0);
            // 2.7 s in: fading.
            for _ in 0..26 {
                app.update();
            }
            assert!(alpha(&app) < 1.0 && alpha(&app) > 0.0, "{}", alpha(&app));
            for _ in 0..5 {
                app.update();
            }
            assert!(bubbles(&mut app).is_empty(), "gone after its lifetime");
        }

        #[test]
        fn a_missing_speaker_shows_nothing() {
            let mut app = app();
            let gone = app.world_mut().spawn(Transform::default()).id();
            app.world_mut().despawn(gone);
            say(&mut app, gone, "¡Epa!");
            app.update();
            assert!(bubbles(&mut app).is_empty());
        }

        #[test]
        fn bubbles_turn_to_face_the_camera() {
            let mut app = app();
            app.add_plugins(TransformPlugin);
            app.world_mut()
                .spawn((Camera3d::default(), Transform::from_xyz(10.0, 3.0, 0.0)));
            let a = app.world_mut().spawn(Transform::default()).id();
            say(&mut app, a, "¡Epa!");
            app.update();
            app.update();
            let mut q = app
                .world_mut()
                .query_filtered::<&Transform, With<SpeechBubble>>();
            let t = q.single(app.world()).unwrap();
            let normal = t.rotation * Vec3::Z;
            assert!(normal.distance(Vec3::X) < 1e-4, "{normal:?}");
        }
    }
}
