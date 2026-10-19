//! Tiny procedural painter for character sprites: rounded shapes with
//! anti-aliased edges, three-tone toon shading, contour lines where parts
//! overlap, and a dark outline around the whole silhouette.

/// An RGB color in sRGB bytes.
pub type Rgb = [u8; 3];

/// Color of the silhouette outline.
pub const OUTLINE: Rgb = [38, 26, 20];

/// Light direction in screen space (`x` right, `y` down, `z` toward the
/// viewer): from the upper left, in front.
const LIGHT: [f32; 3] = [-0.45, -0.65, 0.61];

/// A 2D point in frame pixels (`y` down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct P2 {
    pub x: f32,
    pub y: f32,
}

impl P2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A filled shape in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Circle of radius `r` (a sphere seen from anywhere).
    Ball { c: P2, r: f32 },
    /// Segment from `a` to `b` swept by a radius going from `ra` to `rb`
    /// (a tapered cylinder with rounded ends: limbs, necks, ponchos).
    Capsule { a: P2, b: P2, ra: f32, rb: f32 },
    /// Axis-aligned ellipse (hat brims, berets).
    Ellipse { c: P2, rx: f32, ry: f32 },
}

impl Shape {
    /// Signed distance in pixels (negative inside) and the screen-space
    /// surface normal `(x, y)` of the rounded body at `p`, in `[-1, 1]`.
    fn sample(&self, p: P2) -> (f32, [f32; 2]) {
        match *self {
            Shape::Ball { c, r } => {
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                let d = dx.hypot(dy);
                (d - r, [dx / r, dy / r])
            }
            Shape::Capsule { a, b, ra, rb } => {
                let (abx, aby) = (b.x - a.x, b.y - a.y);
                let len2 = abx * abx + aby * aby;
                let t = if len2 > 0.0 {
                    (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (qx, qy) = (a.x + abx * t, a.y + aby * t);
                let r = ra + (rb - ra) * t;
                let (dx, dy) = (p.x - qx, p.y - qy);
                (dx.hypot(dy) - r, [dx / r, dy / r])
            }
            Shape::Ellipse { c, rx, ry } => {
                let (u, v) = ((p.x - c.x) / rx, (p.y - c.y) / ry);
                let k = u.hypot(v);
                ((k - 1.0) * rx.min(ry), [u, v])
            }
        }
    }

    /// Pixel bounds `(x0, y0, x1, y1)` the shape can touch.
    fn bounds(&self) -> (f32, f32, f32, f32) {
        match *self {
            Shape::Ball { c, r } => (c.x - r, c.y - r, c.x + r, c.y + r),
            Shape::Capsule { a, b, ra, rb } => {
                let r = ra.max(rb);
                (
                    a.x.min(b.x) - r,
                    a.y.min(b.y) - r,
                    a.x.max(b.x) + r,
                    a.y.max(b.y) + r,
                )
            }
            Shape::Ellipse { c, rx, ry } => (c.x - rx, c.y - ry, c.x + rx, c.y + ry),
        }
    }
}

/// One shape to paint, with its color and how it is shaded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub shape: Shape,
    pub color: Rgb,
    /// Distance into the screen; farther parts are painted first.
    pub depth: f32,
    /// Toon shading; flat parts (eyes, markers) keep their color.
    pub shaded: bool,
    /// Only paint rows in `[top, bottom)` (stripes across a garment).
    pub rows: Option<(f32, f32)>,
}

impl Part {
    pub fn new(shape: Shape, color: Rgb, depth: f32) -> Self {
        Self {
            shape,
            color,
            depth,
            shaded: true,
            rows: None,
        }
    }

    pub fn flat(self) -> Self {
        Self {
            shaded: false,
            ..self
        }
    }

    /// Moves the part `by` deeper (positive) or nearer (negative) in the
    /// painting order, without moving it on screen.
    pub fn nudge(self, by: f32) -> Self {
        Self {
            depth: self.depth + by,
            ..self
        }
    }

    pub fn rows(self, top: f32, bottom: f32) -> Self {
        Self {
            rows: Some((top, bottom)),
            ..self
        }
    }
}

/// The three toon tones of `base`: shadow, base and highlight.
pub fn tones(base: Rgb) -> [Rgb; 3] {
    let scale =
        |k: f32, add: f32| base.map(|c| (c as f32 * k + add).round().clamp(0.0, 255.0) as u8);
    [scale(0.68, 0.0), base, scale(1.12, 14.0)]
}

/// Straight-alpha RGBA raster of one frame.
pub struct Frame {
    pub size: usize,
    /// `[r, g, b, a]` per pixel, each `0.0..=1.0` (colors in sRGB).
    px: Vec<[f32; 4]>,
}

impl Frame {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            px: vec![[0.0; 4]; size * size],
        }
    }

    /// Paints `parts` back to front (largest depth first; ties keep order).
    pub fn paint(&mut self, parts: &mut [Part]) {
        parts.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        for part in parts.iter() {
            self.paint_part(part);
        }
    }

    fn paint_part(&mut self, part: &Part) {
        let (x0, y0, x1, y1) = part.shape.bounds();
        let (mut top, mut bottom) = (y0.floor() - 1.0, y1.ceil() + 1.0);
        if let Some((t, b)) = part.rows {
            top = top.max(t);
            bottom = bottom.min(b);
        }
        let n = self.size as i32;
        let [shadow, base, highlight] = tones(part.color);
        for py in (top as i32).max(0)..(bottom as i32).min(n) {
            for px in ((x0.floor() - 1.0) as i32).max(0)..((x1.ceil() + 1.0) as i32).min(n) {
                let p = P2::new(px as f32 + 0.5, py as f32 + 0.5);
                let (sdf, [nx, ny]) = part.shape.sample(p);
                let coverage = (0.5 - sdf).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let i = py as usize * self.size + px as usize;
                let under = self.px[i][3];
                let color = if !part.shaded {
                    part.color
                } else if sdf > -1.0 && under > 0.5 {
                    // Contour line where this part overlaps another.
                    shadow.map(|c| (c as f32 * 0.78) as u8)
                } else {
                    let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
                    let light = nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2];
                    if light > 0.72 {
                        highlight
                    } else if light > 0.2 {
                        base
                    } else {
                        shadow
                    }
                };
                self.blend(i, color, coverage);
            }
        }
    }

    /// Source-over compositing of `color` at `alpha` onto pixel `i`.
    fn blend(&mut self, i: usize, color: Rgb, alpha: f32) {
        let dst = self.px[i];
        let out_a = alpha + dst[3] * (1.0 - alpha);
        let mut out = [0.0; 4];
        for c in 0..3 {
            let src = color[c] as f32 / 255.0;
            out[c] = if out_a > 0.0 {
                (src * alpha + dst[c] * dst[3] * (1.0 - alpha)) / out_a
            } else {
                0.0
            };
        }
        out[3] = out_a;
        self.px[i] = out;
    }

    /// Rings the silhouette with a one-pixel [`OUTLINE`], then snaps alpha
    /// to fully opaque or fully clear (the sprites are alpha-masked).
    pub fn outline(&mut self) {
        let n = self.size;
        let solid: Vec<bool> = self.px.iter().map(|p| p[3] >= 0.5).collect();
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                if solid[i] {
                    self.px[i][3] = 1.0;
                    continue;
                }
                let near = [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().any(|&(dx, dy)| {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    nx >= 0
                        && ny >= 0
                        && (nx as usize) < n
                        && (ny as usize) < n
                        && solid[ny as usize * n + nx as usize]
                });
                self.px[i] = if near {
                    let o = OUTLINE.map(|c| c as f32 / 255.0);
                    [o[0], o[1], o[2], 1.0]
                } else {
                    [0.0; 4]
                };
            }
        }
    }

    /// Copies this frame into an RGBA8 `sheet` of `sheet_width` pixels at
    /// the frame's top-left corner `(ox, oy)`.
    pub fn blit(&self, sheet: &mut [u8], sheet_width: usize, ox: usize, oy: usize) {
        for y in 0..self.size {
            for x in 0..self.size {
                let p = self.px[y * self.size + x];
                let i = ((oy + y) * sheet_width + ox + x) * 4;
                for c in 0..4 {
                    sheet[i + c] = (p[c] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(frame: &Frame, x: usize, y: usize) -> [f32; 4] {
        frame.px[y * frame.size + x]
    }

    #[test]
    fn edges_are_anti_aliased_and_interiors_solid() {
        let mut f = Frame::new(32);
        f.paint(&mut [Part::new(
            Shape::Ball {
                c: P2::new(16.0, 16.0),
                r: 8.3,
            },
            [200, 100, 50],
            0.0,
        )]);
        assert_eq!(pixel(&f, 16, 16)[3], 1.0);
        assert_eq!(pixel(&f, 2, 2)[3], 0.0);
        let partial = f.px.iter().filter(|p| p[3] > 0.0 && p[3] < 1.0).count();
        assert!(partial > 8, "soft edge pixels: {partial}");
    }

    #[test]
    fn shading_uses_three_tones_lit_from_the_upper_left() {
        let base = [120, 90, 60];
        let [shadow, _, highlight] = tones(base);
        let mut f = Frame::new(40);
        f.paint(&mut [Part::new(
            Shape::Ball {
                c: P2::new(20.0, 20.0),
                r: 15.0,
            },
            base,
            0.0,
        )]);
        let rgb = |x, y| {
            let p = pixel(&f, x, y);
            [p[0], p[1], p[2]].map(|c| (c * 255.0).round() as u8)
        };
        assert_eq!(rgb(15, 13), highlight);
        assert_eq!(rgb(20, 20), base);
        assert_eq!(rgb(29, 30), shadow);
    }

    #[test]
    fn far_parts_are_painted_first_and_outline_rings_the_silhouette() {
        let mut f = Frame::new(24);
        let near = Part::new(
            Shape::Ball {
                c: P2::new(12.0, 12.0),
                r: 5.0,
            },
            [10, 200, 10],
            -1.0,
        )
        .flat();
        let far = Part::new(
            Shape::Ball {
                c: P2::new(12.0, 12.0),
                r: 7.0,
            },
            [200, 10, 10],
            1.0,
        )
        .flat();
        f.paint(&mut [near, far]);
        f.outline();
        let center = pixel(&f, 12, 12);
        assert!(
            center[1] > 0.7 && center[0] < 0.1,
            "near on top: {center:?}"
        );
        assert!(f.px.iter().all(|p| p[3] == 0.0 || p[3] == 1.0));
        let o = OUTLINE.map(|c| c as f32 / 255.0);
        let ring = pixel(&f, 12, 4);
        assert_eq!([ring[0], ring[1], ring[2]], o, "{ring:?}");
    }
}
