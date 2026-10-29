//! Globos de diálogo: una línea de diálogo que flota sobre la cabeza de
//! alguien.
//!
//! El juego escribe un mensaje [`Say`] que nombra la entidad que habla y la
//! línea; este módulo dispone la línea en la tipografía de cuerpo [`Face`],
//! pinta un globo de papel con contorno de tinta y una cola que apunta hacia
//! abajo, y lo muestra sobre quien habla, siempre mirando a la cámara, durante
//! [`LIFETIME`] segundos, con un fundido en los últimos [`FADE`] segundos. Cada
//! hablante tiene como máximo un globo: una línea nueva reemplaza a la
//! anterior.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::lettering::{self, Face};

/// Cantidad máxima de caracteres en una línea de un globo antes de que se
/// corte.
pub const BUBBLE_CHARS: usize = 16;
/// Segundos que un globo permanece visible.
pub const LIFETIME: f32 = 3.0;
/// Segundos del final de [`LIFETIME`] durante los cuales el globo se desvanece.
pub const FADE: f32 = 0.6;
/// Altura de la punta de la cola sobre los pies de quien habla, en metros.
pub const HEIGHT: f32 = 2.2;
/// Tamaño del rótulo en la textura de un globo, en píxeles de textura.
pub const TEXT_PX: f32 = 38.0;
/// Distancia vertical entre dos líneas de texto, en píxeles de textura.
const LINE_PITCH: usize = 34;
/// Distancia vertical entre dos líneas de texto en el mundo, en metros. Los
/// globos mantienen este tamaño sea cual sea la resolución de la textura.
const LINE_PITCH_M: f32 = 0.264;
/// Tamaño de un píxel de textura en el mundo, en metros.
pub const METERS_PER_PIXEL: f32 = LINE_PITCH_M / LINE_PITCH as f32;

/// Ancho del contorno de tinta, en píxeles de textura.
const OUTLINE: usize = 3;
/// Papel entre el contorno y el rótulo, a lo ancho y a lo alto.
const PADDING_X: usize = 11;
const PADDING_Y: usize = 9;
/// Radio de las esquinas redondeadas, en píxeles de textura.
const CORNER: f32 = 8.0;
/// Filas que cuelga la cola por debajo del contorno del cuerpo.
const TAIL: usize = 11;

/// Colores del papel y de la tinta (sRGB).
pub const PAPER: [u8; 3] = [241, 233, 212];
pub const INK: [u8; 3] = [46, 36, 28];

/// Divide `text` en líneas de como máximo `max_chars` caracteres, cortando en
/// los espacios (los espacios seguidos se colapsan) y partiendo las palabras
/// más largas que una línea.
pub fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;
    for word in text.split_whitespace() {
        let mut chars: Vec<char> = word.chars().collect();
        // Demasiado larga para cualquier línea: se llenan líneas con trozos de
        // ella.
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

/// Disposición en píxeles de la textura de un globo.
#[derive(Debug, Clone, PartialEq)]
pub struct BubbleLayout {
    /// Las líneas ya cortadas, de arriba abajo.
    pub lines: Vec<String>,
    pub width_px: usize,
    /// Alto de la textura: el cuerpo más la cola.
    pub height_px: usize,
    /// Alto del cuerpo redondeado, contorno incluido.
    pub body_height_px: usize,
    /// Fila de la textura donde empieza la primera línea (negativa cuando las
    /// filas en blanco de la línea por encima de su tinta sobresalen de la
    /// textura).
    pub text_top: isize,
}

impl BubbleLayout {
    /// Tamaño en el mundo, en metros (ancho, alto).
    pub fn size_m(&self) -> Vec2 {
        Vec2::new(self.width_px as f32, self.height_px as f32) * METERS_PER_PIXEL
    }
}

/// Dispone `text` en un globo apenas lo bastante grande para sus líneas
/// cortadas.
pub fn layout_bubble(text: &str) -> BubbleLayout {
    let lines = wrap(text, BUBBLE_CHARS);
    let measured: Vec<_> = lines
        .iter()
        .map(|l| lettering::measure(l, Face::Body, TEXT_PX))
        .collect();
    let text_width = measured.iter().map(|e| e.width as usize).max().unwrap_or(0);
    // Filas entintadas de todo el bloque, para que las filas en blanco por
    // encima de los acentos y por debajo de los descendentes no rellenen el
    // globo de forma irregular.
    let first = measured
        .first()
        .and_then(|e| e.ink)
        .map_or(0, |ink| ink.min_y.floor() as isize);
    let last = measured
        .last()
        .map(|e| {
            e.ink
                .map_or(e.ascent as isize, |ink| ink.max_y.ceil() as isize)
        })
        .unwrap_or(0)
        + (LINE_PITCH * lines.len().saturating_sub(1)) as isize;
    let ink_height = (last - first).max(1) as usize;
    let width_px = text_width.max(TEXT_PX as usize) + 2 * (OUTLINE + PADDING_X);
    let body_height_px = ink_height + 2 * (OUTLINE + PADDING_Y);
    BubbleLayout {
        lines,
        width_px,
        height_px: body_height_px + TAIL + 1 - OUTLINE,
        body_height_px,
        text_top: (OUTLINE + PADDING_Y) as isize - first,
    }
}

/// Pinta el globo de `layout`: píxeles RGBA8 sRGB, fila por fila desde arriba,
/// transparentes fuera del globo y su cola.
pub fn paint_bubble(layout: &BubbleLayout) -> Vec<u8> {
    let (w, h, body) = (layout.width_px, layout.height_px, layout.body_height_px);
    let mut colors: Vec<Option<[u8; 3]>> = vec![None; w * h];

    // Cuerpo redondeado: tinta donde está la forma exterior pero no la
    // interior.
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

    // Cola: un triángulo que cuelga del borde inferior y lo abre.
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
                let edge = d > half - OUTLINE as f32 || t == TAIL;
                colors[y * w + x] = Some(if edge { INK } else { PAPER });
            }
        }
    }

    // Rótulo, cada línea centrada, con sus bordes con antialiasing mezclados
    // sobre el papel (o sobre el contorno, donde lo roza).
    for (n, line) in layout.lines.iter().enumerate() {
        let coverage = lettering::rasterize(line, Face::Body, TEXT_PX);
        let left = (w as isize - coverage.width as isize) / 2;
        let top = layout.text_top + (n * LINE_PITCH) as isize;
        coverage.for_each(|x, y, alpha| {
            let (px, py) = (left + x as isize, top + y as isize);
            if !(0..w as isize).contains(&px) || !(0..h as isize).contains(&py) {
                return;
            }
            let cell = &mut colors[py as usize * w + px as usize];
            let under = cell.unwrap_or(PAPER);
            let cover = f32::from(alpha) / 255.0;
            *cell = Some(std::array::from_fn(|i| {
                (f32::from(INK[i]) * cover + f32::from(under[i]) * (1.0 - cover)).round() as u8
            }));
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

/// Si el punto `(x, y)` cae en un rectángulo de `w` x `h` en el origen con
/// esquinas redondeadas de radio `radius`.
fn rounded(x: f32, y: f32, w: f32, h: f32, radius: f32) -> bool {
    if x < 0.0 || y < 0.0 || x > w || y > h {
        return false;
    }
    let cx = x.clamp(radius, w - radius);
    let cy = y.clamp(radius, h - radius);
    (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}

/// Opacidad de un globo de `age` segundos: opaco, y luego se desvanece hasta
/// cero durante los últimos [`FADE`] segundos de su [`LIFETIME`].
pub fn bubble_alpha(age: f32) -> f32 {
    if age <= LIFETIME - FADE {
        1.0
    } else {
        ((LIFETIME - age) / FADE).clamp(0.0, 1.0)
    }
}

/// Pide que se muestre `line` sobre `speaker`.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct Say {
    pub speaker: Entity,
    pub line: String,
}

/// Un globo en pantalla: hijo de quien habla.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SpeechBubble {
    pub speaker: Entity,
    pub line: String,
    /// Segundos desde que apareció.
    pub age: f32,
}

/// Muestra las líneas de [`Say`] como globos de diálogo.
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

/// Texturas de globo por línea, y el quad sobre el que se dibujan.
#[derive(Resource, Default)]
struct BubbleAssets {
    quad: Option<Handle<Mesh>>,
    textures: HashMap<String, (Handle<Image>, Vec2)>,
}

impl BubbleAssets {
    /// La textura de `line` y su tamaño en metros, pintada la primera vez que
    /// se usa.
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
        // Rótulo suave.
        image.sampler = ImageSampler::linear();
        let entry = (images.add(image), layout.size_m());
        self.textures.insert(line.to_string(), entry.clone());
        entry
    }
}

/// Almacenes de assets a partir de los cuales se construye un globo.
type BubbleStores<'w> = (
    ResMut<'w, Assets<Image>>,
    ResMut<'w, Assets<Mesh>>,
    ResMut<'w, Assets<StandardMaterial>>,
);

/// Crea un globo por cada [`Say`], reemplazando el anterior de quien habla.
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
        // Un globo por hablante: la línea nueva reemplaza a la anterior.
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
            // El papel se ve igual al sol y a la sombra.
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

/// Desvanece los globos y los elimina al final de su vida.
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
        // Se toca el material solo durante el fundido, para que no se vuelva a
        // subir en cada frame.
        let alpha = bubble_alpha(bubble.age);
        if alpha < 1.0
            && let Some(mut material) = materials.get_mut(&material.0)
        {
            material.base_color = Color::WHITE.with_alpha(alpha);
        }
    }
}

/// Gira cada globo alrededor de Y para que mire a la cámara, sin dejar de estar
/// derecho.
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
        // Se deshace el propio giro de quien habla, si lo hay.
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
        // Cabe exactamente el límite.
        assert_eq!(wrap("abcd efgh", 9), ["abcd efgh"]);
        assert_eq!(wrap("abcd efgh", 8), ["abcd", "efgh"]);
        // Una palabra más larga que una línea se parte.
        assert_eq!(wrap("abcdefghij xy", 4), ["abcd", "efgh", "ij", "xy"]);
        let text = "¡Más despacio, que no es carrera! ¡Bárbaro! ¡Me llenó de tierra!";
        let lines = wrap(text, BUBBLE_CHARS);
        assert!(lines.iter().all(|l| l.chars().count() <= BUBBLE_CHARS));
        assert_eq!(lines.join(" "), text);
    }

    fn advance(line: &str) -> usize {
        lettering::measure(line, Face::Body, TEXT_PX).width as usize
    }

    #[test]
    fn a_bubble_fits_its_lines_and_stays_small() {
        let one = layout_bubble("¡Epa!");
        assert_eq!(one.lines, ["¡Epa!"]);
        assert!(one.width_px >= advance("¡Epa!") + 2 * (OUTLINE + PADDING_X));
        assert_eq!(one.height_px, one.body_height_px + TAIL + 1 - OUTLINE);

        // La línea de diálogo más larga (40 caracteres) necesita como máximo 3
        // líneas.
        let long = layout_bubble("¡Más despacio, que no es carrera, che!!!");
        assert!(long.lines.len() <= 3, "{:?}", long.lines);
        for line in &long.lines {
            assert!(long.width_px >= advance(line) + 2 * (OUTLINE + PADDING_X));
        }
        // Cada línea extra agrega un paso de línea, y la línea más ancha fija
        // el ancho.
        let (one_line, two) = (
            layout_bubble("Epa che"),
            layout_bubble("Epa che Epa che Epa che"),
        );
        assert_eq!((one_line.lines.len(), two.lines.len()), (1, 2));
        assert_eq!(two.body_height_px - one_line.body_height_px, LINE_PITCH);
        let widest = long.lines.iter().map(|l| advance(l)).max().unwrap();
        assert!(long.width_px <= widest + 2 * (OUTLINE + PADDING_X) + 1);
        let size = long.size_m();
        assert!(size.x < 2.5 && size.y < 1.2, "{size:?}");
        assert_eq!(layout_bubble(""), layout_bubble(" "));
    }

    #[test]
    fn bubble_sizes_in_meters_stay_close_to_the_bitmap_bubbles() {
        // Una línea es un paso de 0.264 m, igual que con las filas de mapa de
        // bits de 12 px.
        let (one, two) = (
            layout_bubble("Epa che"),
            layout_bubble("Epa che Epa che Epa che"),
        );
        let pitch = (two.body_height_px - one.body_height_px) as f32 * METERS_PER_PIXEL;
        assert!((pitch - 0.264).abs() < 1e-3, "{pitch}");
        // Tamaños (en metros) que tenían los globos de mapa de bits; IM Fell es
        // más angosta que las celdas de mapa de bits de 6 px, así que las
        // líneas salen hasta un tercio más angostas.
        for (text, width, height) in [
            ("¡Epa!", 0.858, 0.462),
            ("¡Más despacio, que no es carrera, che!!!", 2.178, 0.968),
            ("¡Casi me pisa el pingo, che!", 2.31, 0.726),
            ("¡Bárbaro! ¡Me llenó de tierra!", 2.31, 0.704),
        ] {
            let size = layout_bubble(text).size_m();
            assert!(
                (size.x - width).abs() <= width * 0.35 && (size.y - height).abs() <= height * 0.2,
                "{text}: {size:?}, was {width} x {height} m"
            );
        }
    }

    #[test]
    fn paints_a_rounded_paper_bubble_with_an_outline_and_a_tail() {
        let l = layout_bubble("¡Casi me pisa el pingo, che!");
        let px = paint_bubble(&l);
        assert_eq!(px.len(), l.width_px * l.height_px * 4);
        let (w, body) = (l.width_px, l.body_height_px);
        let mid = w / 2;
        // Las esquinas redondeadas son transparentes; los bordes son de tinta.
        for (x, y) in [(0, 0), (w - 1, 0), (0, body - 1), (w - 1, body - 1)] {
            assert_eq!(pixel(&px, &l, x, y), CLEAR, "corner ({x}, {y})");
        }
        for k in 0..OUTLINE {
            assert_eq!(pixel(&px, &l, mid, k), rgb(INK), "top {k}");
            assert_eq!(pixel(&px, &l, k, body / 2), rgb(INK), "left {k}");
            assert_eq!(pixel(&px, &l, w - 1 - k, body / 2), rgb(INK), "right {k}");
        }
        // Papel adentro, entre el contorno y el rótulo.
        assert_eq!(pixel(&px, &l, OUTLINE, body / 2), rgb(PAPER));
        assert_eq!(pixel(&px, &l, mid, OUTLINE), rgb(PAPER));
        // La cola abre el borde inferior y termina en una punta de tinta.
        assert_eq!(pixel(&px, &l, mid, body - 1), rgb(PAPER));
        assert_eq!(pixel(&px, &l, mid, l.height_px - 1), rgb(INK));
        assert_eq!(pixel(&px, &l, 0, l.height_px - 1), CLEAR);
        assert_eq!(pixel(&px, &l, w - 1, l.height_px - 1), CLEAR);
        // La cola se angosta hacia su punta, fila por fila.
        let widths: Vec<usize> = (body..l.height_px)
            .map(|y| (0..w).filter(|&x| pixel(&px, &l, x, y)[3] != 0).count())
            .collect();
        assert!(widths.windows(2).all(|p| p[1] < p[0]), "{widths:?}");
        // Transparente u opaco, y nunca un color fuera del rango que va del
        // papel a la tinta.
        for p in px.chunks(4) {
            if p[3] == 0 {
                assert_eq!(p, CLEAR);
                continue;
            }
            assert_eq!(p[3], 255, "{p:?}");
            for i in 0..3 {
                assert!((INK[i]..=PAPER[i]).contains(&p[i]), "{p:?}");
            }
        }
    }

    /// Píxeles estrictamente dentro del contorno cuyo canal rojo está al menos
    /// 12 por debajo del papel: rótulo con una cobertura de 16 o más.
    fn lettered(px: &[u8], l: &BubbleLayout) -> Vec<(usize, usize)> {
        let inner = |x: usize, y: usize| {
            x > OUTLINE
                && x + OUTLINE + 1 < l.width_px
                && y > OUTLINE
                && y + OUTLINE + 1 < l.body_height_px
        };
        (0..l.body_height_px)
            .flat_map(|y| (0..l.width_px).map(move |x| (x, y)))
            .filter(|&(x, y)| inner(x, y) && pixel(px, l, x, y)[0] + 12 <= PAPER[0])
            .collect()
    }

    #[test]
    fn letters_every_glyph_pixel_inside_the_paper_blended() {
        let l = layout_bubble("¡Bárbaro! ¡Me llenó de tierra!");
        let px = paint_bubble(&l);
        let mut covered = 0;
        for line in &l.lines {
            lettering::rasterize(line, Face::Body, TEXT_PX).for_each(|_, _, a| {
                covered += usize::from(a >= 16);
            });
        }
        let lettering = lettered(&px, &l);
        assert_eq!(lettering.len(), covered);
        // Con antialiasing: los píxeles del borde quedan entre el papel y la
        // tinta.
        let soft = lettering
            .iter()
            .filter(|&&(x, y)| {
                let c = pixel(&px, &l, x, y);
                c != rgb(INK) && c != rgb(PAPER)
            })
            .count();
        assert!(soft * 4 >= covered, "{soft} soft of {covered}");
        // El rótulo mantiene papel entre él y el contorno, y queda centrado
        // verticalmente en el globo con un margen de dos píxeles.
        let top = lettering.iter().map(|&(_, y)| y).min().unwrap();
        let bottom = lettering.iter().map(|&(_, y)| y).max().unwrap();
        assert!(top >= OUTLINE + PADDING_Y - 1, "top {top}");
        assert!(
            bottom + OUTLINE + PADDING_Y < l.body_height_px + 2,
            "bottom {bottom}"
        );
        assert!(
            top.abs_diff(l.body_height_px - 1 - bottom) <= 3,
            "{top} {bottom}"
        );
    }

    #[test]
    fn a_line_is_centered_across_the_bubble() {
        let l = layout_bubble("¡Epa!");
        let lettering = lettered(&paint_bubble(&l), &l);
        let left = lettering.iter().map(|&(x, _)| x).min().unwrap();
        let right = l.width_px - 1 - lettering.iter().map(|&(x, _)| x).max().unwrap();
        assert!(left > OUTLINE + 1 && right > OUTLINE + 1, "{left} {right}");
        assert!(left.abs_diff(right) <= 4, "left {left} right {right}");
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
            // Las texturas se guardan en caché por línea.
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
            // A los 2.7 s: en pleno fundido.
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
