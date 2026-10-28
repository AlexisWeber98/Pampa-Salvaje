//! CPU lettering with the game's TrueType faces.
//!
//! The world textures (signs, bubbles, the minimap) are painted on the CPU,
//! where Bevy's text pipeline cannot reach. This module lays out one line of
//! text with `ab_glyph`, using the same font files the UI loads through
//! [`crate::typeface`], and hands back antialiased coverage to plot.

use std::sync::LazyLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};

/// Rye, embedded. Same file as [`crate::typeface::DISPLAY_FONT`] under `assets/`.
const DISPLAY_BYTES: &[u8] = include_bytes!("../../../assets/fonts/rye/Rye-Regular.ttf");
/// IM Fell English roman, embedded. Same file as [`crate::typeface::BODY_FONT`].
const BODY_BYTES: &[u8] = include_bytes!("../../../assets/fonts/im_fell_english/IMFeENrm28P.ttf");
/// IM Fell English italic, embedded. Same file as [`crate::typeface::ITALIC_FONT`].
const ITALIC_BYTES: &[u8] = include_bytes!("../../../assets/fonts/im_fell_english/IMFeENit28P.ttf");

static DISPLAY: LazyLock<FontRef<'static>> = LazyLock::new(|| parse(bytes(Face::Display)));
static BODY: LazyLock<FontRef<'static>> = LazyLock::new(|| parse(bytes(Face::Body)));
static ITALIC: LazyLock<FontRef<'static>> = LazyLock::new(|| parse(bytes(Face::Italic)));

fn parse(bytes: &'static [u8]) -> FontRef<'static> {
    FontRef::try_from_slice(bytes).expect("the shipped typefaces are valid TrueType")
}

/// The faces of the game, named after their role in [`crate::typeface`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Rye, for signs painted on wood (`typeface::DISPLAY_FONT`).
    Display,
    /// IM Fell English roman, for body text (`typeface::BODY_FONT`).
    Body,
    /// IM Fell English italic (`typeface::ITALIC_FONT`).
    Italic,
}

fn bytes(face: Face) -> &'static [u8] {
    match face {
        Face::Display => DISPLAY_BYTES,
        Face::Body => BODY_BYTES,
        Face::Italic => ITALIC_BYTES,
    }
}

fn font(face: Face) -> &'static FontRef<'static> {
    match face {
        Face::Display => &DISPLAY,
        Face::Body => &BODY,
        Face::Italic => &ITALIC,
    }
}

/// Inked bounds of a line, in pixels from the pen origin; `y` grows down from the top of the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

/// Size of one laid out line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    /// Advance width in pixels, rounded up (what a caller centers or boxes).
    pub width: u32,
    /// Pixels from the top of the line to the baseline.
    pub ascent: u32,
    /// Pixels from the baseline to the bottom of the line.
    pub descent: u32,
    /// Bounds of the actual coverage, `None` for blank text.
    pub ink: Option<Ink>,
}

/// Antialiased coverage of one line, `width * height` bytes, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage {
    pub width: u32,
    pub height: u32,
    /// Row of the baseline (the line's ascent).
    pub baseline: u32,
    /// Coverage `0..=255` per pixel.
    pub alpha: Vec<u8>,
}

impl Coverage {
    /// Calls `f(x, y, alpha)` for every pixel with coverage.
    pub fn for_each(&self, mut f: impl FnMut(u32, u32, u8)) {
        if self.width == 0 {
            return;
        }
        for (i, &a) in self.alpha.iter().enumerate() {
            if a > 0 {
                f(i as u32 % self.width, i as u32 / self.width, a);
            }
        }
    }
}

/// True when `face` has a real glyph for `c`; whitespace always counts.
pub fn supports(face: Face, c: char) -> bool {
    c.is_whitespace() || font(face).glyph_id(c).0 != 0
}

/// One positioned glyph: its id and pen x.
fn layout(text: &str, face: Face, px: f32) -> (Vec<(ab_glyph::GlyphId, f32)>, f32) {
    let scaled = font(face).as_scaled(PxScale::from(px));
    let mut pen = 0.0;
    let mut prev = None;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let id = scaled.glyph_id(c);
        if let Some(p) = prev {
            pen += scaled.kern(p, id);
        }
        glyphs.push((id, pen));
        pen += scaled.h_advance(id);
        prev = Some(id);
    }
    (glyphs, pen)
}

fn metrics(face: Face, px: f32) -> (u32, u32) {
    let scaled = font(face).as_scaled(PxScale::from(px));
    (
        scaled.ascent().ceil() as u32,
        (-scaled.descent()).ceil() as u32,
    )
}

/// Measures one line of `text` in `face` at `px` pixels, kerning included.
pub fn measure(text: &str, face: Face, px: f32) -> Extent {
    let scaled = font(face).as_scaled(PxScale::from(px));
    let (ascent, descent) = metrics(face, px);
    let (glyphs, end) = layout(text, face, px);
    let mut ink: Option<Ink> = None;
    for (id, x) in glyphs {
        let glyph = id.with_scale_and_position(scaled.scale(), point(x, ascent as f32));
        let Some(outlined) = font(face).outline_glyph(glyph) else {
            continue;
        };
        let b = outlined.px_bounds();
        ink = Some(match ink {
            None => Ink {
                min_x: b.min.x,
                max_x: b.max.x,
                min_y: b.min.y,
                max_y: b.max.y,
            },
            Some(i) => Ink {
                min_x: i.min_x.min(b.min.x),
                max_x: i.max_x.max(b.max.x),
                min_y: i.min_y.min(b.min.y),
                max_y: i.max_y.max(b.max.y),
            },
        });
    }
    Extent {
        width: end.ceil() as u32,
        ascent,
        descent,
        ink,
    }
}

/// Rasterizes one line of `text` in `face` at `px` pixels.
///
/// The canvas is the advance width by the line height; ink that overhangs it
/// (a side bearing, an italic tail) is clipped.
pub fn rasterize(text: &str, face: Face, px: f32) -> Coverage {
    let scaled = font(face).as_scaled(PxScale::from(px));
    let e = measure(text, face, px);
    let (width, height) = (e.width, e.ascent + e.descent);
    let mut alpha = vec![0u8; (width * height) as usize];
    let (glyphs, _) = layout(text, face, px);
    for (id, x) in glyphs {
        let glyph = id.with_scale_and_position(scaled.scale(), point(x, e.ascent as f32));
        let Some(outlined) = font(face).outline_glyph(glyph) else {
            continue;
        };
        let b = outlined.px_bounds();
        outlined.draw(|gx, gy, c| {
            let (x, y) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                return;
            }
            let cell = &mut alpha[(y as u32 * width + x as u32) as usize];
            *cell = (*cell).max((c.clamp(0.0, 1.0) * 255.0).round() as u8);
        });
    }
    Coverage {
        width,
        height,
        baseline: e.ascent,
        alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::PlaceKind;

    const FACES: [Face; 3] = [Face::Display, Face::Body, Face::Italic];

    fn ink_rows(cov: &Coverage) -> Vec<u32> {
        let mut rows = Vec::new();
        cov.for_each(|_, y, _| rows.push(y));
        rows
    }

    #[test]
    fn every_face_supports_the_charset() {
        let wanted: Vec<char> = ('A'..='Z')
            .chain('a'..='z')
            .chain('0'..='9')
            .chain("ÁÉÍÓÚÜÑáéíóúüñ¡¿.,:;!?-'\"()%$/ ".chars())
            .collect();
        for face in FACES {
            for &c in &wanted {
                assert!(supports(face, c), "{face:?} lacks {c:?}");
            }
        }
    }

    #[test]
    fn every_face_supports_every_place_name() {
        for face in [Face::Body, Face::Display] {
            for kind in PlaceKind::ALL {
                for c in kind.name().chars() {
                    assert!(supports(face, c), "{face:?} lacks {c:?} in {}", kind.name());
                }
            }
        }
    }

    #[test]
    fn characters_outside_the_faces_are_unsupported() {
        for face in FACES {
            assert!(supports(face, '\t'), "{face:?} whitespace");
            assert!(!supports(face, '漢'), "{face:?}");
        }
    }

    #[test]
    fn wider_text_measures_wider() {
        for face in FACES {
            let short = measure("Pampa", face, 24.0).width;
            let long = measure("Pampa Salvaje", face, 24.0).width;
            assert!(short > 0 && long > short, "{face:?}");
        }
    }

    #[test]
    fn empty_text_has_no_width_and_no_ink() {
        let e = measure("", Face::Body, 24.0);
        assert_eq!(e.width, 0);
        assert_eq!(e.ink, None);
        assert!(e.ascent > 0 && e.descent > 0);
    }

    #[test]
    fn doubling_the_size_doubles_the_width() {
        for face in FACES {
            let a = measure("Estación", face, 16.0).width as i64;
            let b = measure("Estación", face, 32.0).width as i64;
            assert!((b - 2 * a).abs() <= 2, "{face:?}: {a} vs {b}");
        }
    }

    #[test]
    fn measure_matches_the_rasterized_coverage() {
        for face in FACES {
            let text = "AVA To";
            let e = measure(text, face, 30.0);
            let cov = rasterize(text, face, 30.0);
            assert_eq!(cov.width, e.width);
            assert_eq!(cov.baseline, e.ascent);
            assert_eq!(cov.height, e.ascent + e.descent);
        }
    }

    #[test]
    fn kerning_narrows_a_pair() {
        // The italic face kerns V against A by about -12 px at 100 px.
        let apart =
            measure("V", Face::Italic, 100.0).width + measure("A", Face::Italic, 100.0).width;
        let pair = measure("VA", Face::Italic, 100.0).width;
        assert!(pair + 8 < apart, "{pair} vs {apart}");
    }

    #[test]
    fn rasterized_text_has_ink_inside_its_box() {
        for face in FACES {
            let cov = rasterize("Hola", face, 24.0);
            assert_eq!(cov.alpha.len(), (cov.width * cov.height) as usize);
            let mut n = 0;
            cov.for_each(|x, y, a| {
                assert!(a > 0 && x < cov.width && y < cov.height);
                n += 1;
            });
            assert!(n > 0, "{face:?}");
        }
    }

    #[test]
    fn descenders_reach_below_the_baseline() {
        let ag = rasterize("Ag", Face::Body, 40.0);
        let ab = rasterize("AB", Face::Body, 40.0);
        // A descender drops well below the baseline; capitals only overshoot it by a row.
        assert!(ink_rows(&ag).iter().any(|&y| y >= ag.baseline + 5));
        assert!(ink_rows(&ab).iter().all(|&y| y <= ab.baseline));
    }

    #[test]
    fn ink_bounds_enclose_the_coverage() {
        let e = measure("Ag", Face::Body, 40.0);
        let ink = e.ink.expect("ink");
        let cov = rasterize("Ag", Face::Body, 40.0);
        cov.for_each(|x, y, _| {
            assert!(x as f32 + 1.0 >= ink.min_x && x as f32 <= ink.max_x + 1.0);
            assert!(y as f32 + 1.0 >= ink.min_y && y as f32 <= ink.max_y + 1.0);
        });
    }

    #[test]
    fn blank_text_has_no_ink() {
        for text in ["", "   "] {
            let cov = rasterize(text, Face::Body, 24.0);
            let e = measure(text, Face::Body, 24.0);
            assert_eq!((cov.width, cov.height), (e.width, e.ascent + e.descent));
            assert_eq!(cov.alpha.len(), (cov.width * cov.height) as usize);
            assert!(cov.alpha.iter().all(|&a| a == 0));
            assert_eq!(e.ink, None);
        }
    }

    #[test]
    fn embedded_bytes_are_the_shipped_assets() {
        use crate::typeface::{BODY_FONT, DISPLAY_FONT, ITALIC_FONT};
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        for (face, path) in [
            (Face::Display, DISPLAY_FONT),
            (Face::Body, BODY_FONT),
            (Face::Italic, ITALIC_FONT),
        ] {
            let disk = std::fs::read(root.join(path)).unwrap();
            assert_eq!(bytes(face), disk.as_slice(), "{face:?}");
        }
    }
}
