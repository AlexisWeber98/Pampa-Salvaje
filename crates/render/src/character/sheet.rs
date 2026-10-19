//! Procedural placeholder sprite sheets: one row of eight directional frames
//! each (see [`ViewDirection`](super::ViewDirection) for the column order).
//!
//! Three kinds of sheet share the system: a mounted gaucho, a saddled horse
//! with no rider, and people on foot in a few palettes.

use super::direction::DIRECTIONS;

/// Width and height of one frame, in pixels.
pub const FRAME_SIZE: usize = 64;

/// How many palettes people on foot come in.
pub const PERSON_LOOKS: u8 = 4;

/// Which sprite sheet a billboard draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SheetKind {
    /// Gaucho on horseback.
    Rider,
    /// Saddled horse with no rider.
    Horse,
    /// Person on foot, in palette `0..PERSON_LOOKS`.
    Person(u8),
}

impl SheetKind {
    /// Every sheet, so they can all be built up front.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Rider, Self::Horse]
            .into_iter()
            .chain((0..PERSON_LOOKS).map(Self::Person))
    }

    /// The person sheet for a logic appearance value.
    pub fn person(look: u8) -> Self {
        Self::Person(look % PERSON_LOOKS)
    }

    /// Side of the square sprite in meters (frames are square).
    pub fn sprite_size(self) -> f32 {
        match self {
            Self::Rider | Self::Horse => 2.8,
            // A 1.75 m person fills 56 of the 64 pixels.
            Self::Person(_) => 2.0,
        }
    }
}

/// RGBA8 sRGB pixels of a full sheet, `FRAME_SIZE * DIRECTIONS` wide and
/// `FRAME_SIZE` tall, row-major from the top-left.
pub fn sheet_pixels(kind: SheetKind) -> Vec<u8> {
    let mut canvas = Canvas::new(FRAME_SIZE * DIRECTIONS, FRAME_SIZE);
    for k in 0..DIRECTIONS {
        let view = View::new(k);
        match kind {
            SheetKind::Rider => {
                draw_horse(&mut canvas, &view, false);
                draw_mounted_gaucho(&mut canvas, &view);
            }
            SheetKind::Horse => draw_horse(&mut canvas, &view, true),
            SheetKind::Person(look) => draw_person(&mut canvas, &view, &PEOPLE[look as usize]),
        }
    }
    canvas.pixels
}

const HORSE: [u8; 3] = [118, 78, 48];
const HORSE_DARK: [u8; 3] = [72, 48, 32];
const SADDLE: [u8; 3] = [64, 40, 26];
const SADDLE_BLANKET: [u8; 3] = [150, 66, 44];
const PONCHO: [u8; 3] = [158, 112, 66];
const PONCHO_STRIPE: [u8; 3] = [92, 60, 40];
const SKIN: [u8; 3] = [196, 152, 112];
const HAT: [u8; 3] = [58, 44, 34];
const BOOTS: [u8; 3] = [46, 34, 26];
/// Bright facing marker (the horse's blaze and the rider's neckerchief).
const MARKER: [u8; 3] = [200, 40, 30];

/// Frame `k`: the figure faces `k * 45°` CCW from "away".
struct View {
    /// Screen-space facing: `side` > 0 faces right, `depth` > 0 faces away.
    side: f32,
    depth: f32,
    /// Horizontal center of the frame on the sheet.
    cx: f32,
}

impl View {
    fn new(k: usize) -> Self {
        let theta = k as f32 * std::f32::consts::FRAC_PI_4;
        Self {
            side: -theta.sin(),
            depth: theta.cos(),
            cx: (k * FRAME_SIZE) as f32 + FRAME_SIZE as f32 / 2.0,
        }
    }
}

/// Draws a horse; `saddled` adds an empty saddle on a striped blanket.
fn draw_horse(canvas: &mut Canvas, v: &View, saddled: bool) {
    let (side, depth, cx) = (v.side, v.depth, v.cx);
    let body_y = 40.0;
    let half_width = 7.0 + 14.0 * side.abs();
    let head_x = cx + side * (half_width + 2.0);
    let head_y = body_y - 8.0 - depth * 4.0;
    let head_behind = depth > 0.3;

    // Legs at the four corners of the body footprint.
    for (lx, front) in [(-0.7, true), (-0.35, false), (0.35, true), (0.7, false)] {
        let x = cx + lx * half_width.max(9.0);
        let color = if front == (depth < 0.0) {
            HORSE
        } else {
            HORSE_DARK
        };
        canvas.fill_rect(x - 1.0, body_y + 3.0, 3.0, 17.0, color);
    }
    // Tail on the opposite side of the head.
    let tail_x = cx - side * (half_width + 1.0);
    canvas.fill_rect(
        tail_x - 1.5,
        body_y - 4.0 + depth * 3.0,
        3.0,
        12.0,
        HORSE_DARK,
    );

    if head_behind {
        draw_head(canvas, head_x, head_y);
    }
    canvas.fill_ellipse(cx, body_y, half_width, 7.0, HORSE);
    if !head_behind {
        draw_head(canvas, head_x, head_y);
    }

    if saddled {
        let sx = cx - side * 2.0;
        canvas.fill_rect(sx - 6.0, body_y - 6.0, 12.0, 6.0, SADDLE_BLANKET);
        canvas.fill_rect(sx - 6.0, body_y - 4.0, 12.0, 1.0, PONCHO_STRIPE);
        canvas.fill_ellipse(sx, body_y - 7.0, 4.5, 2.0, SADDLE);
    }
}

/// Gaucho on horseback: poncho, head and wide-brimmed hat, leaning slightly back.
fn draw_mounted_gaucho(canvas: &mut Canvas, v: &View) {
    let (side, depth) = (v.side, v.depth);
    let rx = v.cx - side * 2.0;
    canvas.fill_rect(rx - 5.0, 21.0, 10.0, 14.0, PONCHO);
    canvas.fill_rect(rx - 5.0, 27.0, 10.0, 2.0, PONCHO_STRIPE);
    canvas.fill_ellipse(rx, 17.0, 3.5, 3.5, SKIN);
    canvas.fill_ellipse(rx, 14.0, 8.0, 1.6, HAT);
    canvas.fill_rect(rx - 3.5, 9.0, 7.0, 5.0, HAT);
    // Red neckerchief, only visible when the rider is not facing away.
    if depth <= 0.3 {
        canvas.fill_ellipse(rx + side * 3.5, 21.0, 2.5, 2.0, MARKER);
    }
}

fn draw_head(canvas: &mut Canvas, x: f32, y: f32) {
    canvas.fill_ellipse(x, y, 4.5, 5.5, HORSE);
    // White-ish blaze would be invisible on dust; use the marker color.
    canvas.fill_ellipse(x, y + 1.0, 1.6, 2.4, MARKER);
}

/// Headwear of a person on foot.
#[derive(Clone, Copy)]
enum Headwear {
    /// Wide-brimmed gaucho hat.
    Brimmed,
    /// Flat basque beret ("boina").
    Beret,
    /// Knotted headscarf.
    Scarf,
}

/// Clothes and build of a person on foot.
struct Person {
    /// Poncho, jacket or blouse.
    top: [u8; 3],
    trim: [u8; 3],
    /// Baggy "bombachas" or a long skirt.
    legs: [u8; 3],
    skirt: bool,
    head: Headwear,
    headwear: [u8; 3],
}

/// Earthy palettes: gaucho, townsman, woman in a long skirt, farmhand.
const PEOPLE: [Person; PERSON_LOOKS as usize] = [
    Person {
        top: [150, 62, 44],
        trim: [70, 44, 32],
        legs: [74, 70, 64],
        skirt: false,
        head: Headwear::Brimmed,
        headwear: [44, 36, 30],
    },
    Person {
        top: [96, 72, 52],
        trim: [196, 186, 160],
        legs: [150, 132, 98],
        skirt: false,
        head: Headwear::Brimmed,
        headwear: [168, 142, 96],
    },
    Person {
        top: [132, 92, 76],
        trim: [204, 192, 168],
        legs: [92, 66, 56],
        skirt: true,
        head: Headwear::Scarf,
        headwear: [176, 156, 118],
    },
    Person {
        top: [198, 188, 162],
        trim: [120, 92, 60],
        legs: [112, 100, 78],
        skirt: false,
        head: Headwear::Beret,
        headwear: [52, 50, 62],
    },
];

/// A person on foot, standing on the frame's bottom edge.
fn draw_person(canvas: &mut Canvas, v: &View, p: &Person) {
    let (side, depth) = (v.side, v.depth);
    let cx = v.cx;
    // Narrower seen from the side than from the front or back.
    let breadth = 1.0 - 0.35 * side.abs();
    let face_x = cx + side * 2.0;

    // Legs: boots, then baggy trousers gathered at the ankle, or a skirt.
    if p.skirt {
        canvas.fill_rect(cx - 9.0 * breadth, 38.0, 18.0 * breadth, 20.0, p.legs);
        canvas.fill_rect(cx - 5.0 * breadth, 58.0, 4.0, 6.0, BOOTS);
        canvas.fill_rect(cx + 1.0 * breadth, 58.0, 4.0, 6.0, BOOTS);
    } else {
        for leg in [-1.0, 1.0] {
            let x = cx + leg * 3.5 * breadth;
            canvas.fill_ellipse(x, 46.0, 3.6 * breadth + 0.6, 8.0, p.legs);
            canvas.fill_rect(x - 2.0 + side * 1.0, 54.0, 4.0, 10.0, BOOTS);
        }
    }

    // Poncho or jacket: a broad trapezoid with a trim stripe.
    for row in 0..18 {
        let y = 21.0 + row as f32;
        let half = (6.0 + row as f32 * 0.35) * breadth;
        canvas.fill_rect(cx - half, y, 2.0 * half, 1.0, p.top);
    }
    canvas.fill_rect(cx - 9.0 * breadth, 33.0, 18.0 * breadth, 2.0, p.trim);

    // Head, face toward the camera unless facing away.
    canvas.fill_ellipse(cx, 16.0, 4.5, 5.0, SKIN);
    if depth > 0.3 {
        // Back of the head: hair.
        canvas.fill_ellipse(cx, 16.5, 4.5, 4.5, HAT);
    } else {
        canvas.fill_rect(face_x - 2.0, 15.0, 1.0, 1.0, BOOTS);
        canvas.fill_rect(face_x + 1.0, 15.0, 1.0, 1.0, BOOTS);
        // Red neckerchief: the facing marker.
        canvas.fill_ellipse(cx + side * 3.0, 21.5, 2.5, 1.8, MARKER);
    }
    match p.head {
        Headwear::Brimmed => {
            canvas.fill_ellipse(cx, 12.0, 9.0, 1.6, p.headwear);
            canvas.fill_rect(cx - 4.0, 6.0, 8.0, 6.0, p.headwear);
        }
        Headwear::Beret => {
            canvas.fill_ellipse(cx - side * 1.0, 11.5, 5.5, 2.5, p.headwear);
        }
        Headwear::Scarf => {
            canvas.fill_ellipse(cx, 13.0, 5.2, 3.5, p.headwear);
            canvas.fill_rect(cx - side * 4.0 - 1.0, 14.0, 2.0, 5.0, p.headwear);
        }
    }
}

/// Tiny RGBA8 raster with filled shapes; untouched pixels stay transparent.
struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
        }
    }

    fn put(&mut self, x: i32, y: i32, rgb: [u8; 3]) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let i = (y as usize * self.width + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }

    fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3]) {
        for py in y.round() as i32..(y + h).round() as i32 {
            for px in x.round() as i32..(x + w).round() as i32 {
                self.put(px, py, rgb);
            }
        }
    }

    fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, rgb: [u8; 3]) {
        for py in (cy - ry).floor() as i32..=(cy + ry).ceil() as i32 {
            for px in (cx - rx).floor() as i32..=(cx + rx).ceil() as i32 {
                let dx = (px as f32 + 0.5 - cx) / rx;
                let dy = (py as f32 + 0.5 - cy) / ry;
                if dx * dx + dy * dy <= 1.0 {
                    self.put(px, py, rgb);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pixels: &[u8], k: usize) -> Vec<u8> {
        let row_bytes = FRAME_SIZE * DIRECTIONS * 4;
        (0..FRAME_SIZE)
            .flat_map(|y| {
                let start = y * row_bytes + k * FRAME_SIZE * 4;
                pixels[start..start + FRAME_SIZE * 4].to_vec()
            })
            .collect()
    }

    #[test]
    fn has_expected_size() {
        for kind in SheetKind::all() {
            assert_eq!(
                sheet_pixels(kind).len(),
                FRAME_SIZE * FRAME_SIZE * DIRECTIONS * 4,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn every_frame_is_distinct_and_not_empty() {
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            let frames: Vec<_> = (0..DIRECTIONS).map(|k| frame(&pixels, k)).collect();
            for (i, a) in frames.iter().enumerate() {
                let opaque = a.chunks(4).filter(|p| p[3] == 255).count();
                assert!(
                    opaque > 200,
                    "{kind:?} frame {i} is nearly empty ({opaque} px)"
                );
                for (j, b) in frames.iter().enumerate().skip(i + 1) {
                    assert_ne!(a, b, "{kind:?} frames {i} and {j} are identical");
                }
            }
        }
    }

    #[test]
    fn mirrored_side_views_point_opposite_ways() {
        // The facing marker sits left of center when facing left, right when facing right.
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            let centroid_x = |k: usize| {
                let f = frame(&pixels, k);
                let (mut sum, mut n) = (0usize, 0usize);
                for (i, p) in f.chunks(4).enumerate() {
                    if p[..3] == super::MARKER {
                        sum += i % FRAME_SIZE;
                        n += 1;
                    }
                }
                assert!(n > 0, "{kind:?} frame {k} has no facing marker");
                sum as f32 / n as f32
            };
            let center = FRAME_SIZE as f32 / 2.0;
            assert!(centroid_x(2) < center - 1.5, "{kind:?}");
            assert!(centroid_x(6) > center + 1.5, "{kind:?}");
        }
    }

    #[test]
    fn people_stand_on_the_ground_and_look_different() {
        let sheets: Vec<_> = (0..PERSON_LOOKS)
            .map(|look| sheet_pixels(SheetKind::Person(look)))
            .collect();
        for (i, a) in sheets.iter().enumerate() {
            // Feet touch the bottom row so the sprite stands on its origin.
            let bottom = frame(a, 4)[(FRAME_SIZE - 1) * FRAME_SIZE * 4..].to_vec();
            assert!(bottom.chunks(4).any(|p| p[3] == 255), "look {i} floats");
            for b in &sheets[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(SheetKind::person(PERSON_LOOKS + 1), SheetKind::Person(1));
    }

    #[test]
    fn the_saddled_horse_has_no_rider() {
        // Nothing above the horse's head and back: the rider's hat is gone.
        let horse = sheet_pixels(SheetKind::Horse);
        let rider = sheet_pixels(SheetKind::Rider);
        let opaque_above = |pixels: &[u8], k: usize, rows: usize| {
            frame(pixels, k)[..rows * FRAME_SIZE * 4]
                .chunks(4)
                .filter(|p| p[3] == 255)
                .count()
        };
        assert_eq!(opaque_above(&horse, 0, 20), 0);
        assert!(opaque_above(&rider, 0, 20) > 0);
    }
}
