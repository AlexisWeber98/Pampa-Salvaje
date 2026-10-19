//! Procedural placeholder rider sprite sheet: one row of eight directional
//! frames (see [`ViewDirection`](super::ViewDirection) for the column order).

use super::direction::DIRECTIONS;

/// Width and height of one frame, in pixels.
pub const FRAME_SIZE: usize = 64;

/// RGBA8 sRGB pixels of the full sheet, `FRAME_SIZE * DIRECTIONS` wide and
/// `FRAME_SIZE` tall, row-major from the top-left.
pub fn rider_sheet_pixels() -> Vec<u8> {
    let mut canvas = Canvas::new(FRAME_SIZE * DIRECTIONS, FRAME_SIZE);
    for k in 0..DIRECTIONS {
        draw_frame(&mut canvas, k);
    }
    canvas.pixels
}

const HORSE: [u8; 3] = [118, 78, 48];
const HORSE_DARK: [u8; 3] = [72, 48, 32];
const PONCHO: [u8; 3] = [158, 112, 66];
const PONCHO_STRIPE: [u8; 3] = [92, 60, 40];
const SKIN: [u8; 3] = [196, 152, 112];
const HAT: [u8; 3] = [58, 44, 34];
/// Bright facing marker (the horse's blaze and the rider's neckerchief).
const MARKER: [u8; 3] = [200, 40, 30];

/// Draws frame `k`: horse and gaucho facing `k * 45°` CCW from "away".
fn draw_frame(canvas: &mut Canvas, k: usize) {
    let theta = k as f32 * std::f32::consts::FRAC_PI_4;
    // Screen-space facing: x to the right, `depth` > 0 means away from camera.
    let (side, depth) = (-theta.sin(), theta.cos());
    let ox = (k * FRAME_SIZE) as f32;
    let cx = ox + FRAME_SIZE as f32 / 2.0;

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

    // Gaucho: poncho, head and wide-brimmed hat, leaning slightly back.
    let rx = cx - side * 2.0;
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
        assert_eq!(
            rider_sheet_pixels().len(),
            FRAME_SIZE * FRAME_SIZE * DIRECTIONS * 4
        );
    }

    #[test]
    fn every_frame_is_distinct_and_not_empty() {
        let pixels = rider_sheet_pixels();
        let frames: Vec<_> = (0..DIRECTIONS).map(|k| frame(&pixels, k)).collect();
        for (i, a) in frames.iter().enumerate() {
            let opaque = a.chunks(4).filter(|p| p[3] == 255).count();
            assert!(opaque > 200, "frame {i} is nearly empty ({opaque} px)");
            for (j, b) in frames.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "frames {i} and {j} are identical");
            }
        }
    }

    #[test]
    fn mirrored_side_views_point_opposite_ways() {
        // The facing marker sits left of center when facing left, right when facing right.
        let pixels = rider_sheet_pixels();
        let centroid_x = |k: usize| {
            let f = frame(&pixels, k);
            let (mut sum, mut n) = (0usize, 0usize);
            for (i, p) in f.chunks(4).enumerate() {
                if p[..3] == super::MARKER {
                    sum += i % FRAME_SIZE;
                    n += 1;
                }
            }
            assert!(n > 0, "frame {k} has no facing marker");
            sum as f32 / n as f32
        };
        let center = FRAME_SIZE as f32 / 2.0;
        assert!(centroid_x(2) < center - 4.0);
        assert!(centroid_x(6) > center + 4.0);
    }
}
