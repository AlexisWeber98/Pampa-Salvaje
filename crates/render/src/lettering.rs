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

/// Antialiased coverage of one line.
///
/// `width`, `height` and `baseline` describe the line's box: its advance
/// width by its ascent plus descent. The canvas behind it is grown to hold
/// all the ink, so a glyph that overhangs its advance (the italic f's
/// swash, the roman Q's tail, a j's hook) is kept, not clipped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage {
    /// Advance width in pixels: what a caller centers or boxes.
    pub width: u32,
    /// Line height in pixels: ascent plus descent.
    pub height: u32,
    /// Row of the baseline (the line's ascent).
    pub baseline: u32,
    /// The canvas's width, the line box plus any overhang on either side.
    canvas_width: u32,
    /// Canvas columns left of the line box, and rows above it.
    left: u32,
    top: u32,
    /// Coverage `0..=255` per canvas pixel, row-major.
    alpha: Vec<u8>,
}

impl Coverage {
    /// No ink at all, in an empty box.
    fn empty() -> Self {
        Coverage {
            width: 0,
            height: 0,
            baseline: 0,
            canvas_width: 0,
            left: 0,
            top: 0,
            alpha: Vec::new(),
        }
    }

    /// Calls `f(x, y, alpha)` for every pixel with coverage, `x` and `y`
    /// from the top-left of the line box (the pen origin at the top of the
    /// line). Ink overhanging the box comes with negative coordinates or
    /// ones at or past `width` and `height`.
    pub fn for_each(&self, mut f: impl FnMut(i32, i32, u8)) {
        if self.canvas_width == 0 {
            return;
        }
        let w = self.canvas_width as usize;
        for (i, &a) in self.alpha.iter().enumerate() {
            if a > 0 {
                let x = (i % w) as i32 - self.left as i32;
                let y = (i / w) as i32 - self.top as i32;
                f(x, y, a);
            }
        }
    }
}

/// The largest size lettered, in pixels: a bigger request is lettered at
/// this size, so no line's canvas can grow without bound.
pub const MAX_PX: f32 = 512.0;

/// `px` as a size to letter at: `None` when it is not a finite, positive
/// number; capped at [`MAX_PX`].
fn size(px: f32) -> Option<f32> {
    (px.is_finite() && px > 0.0).then(|| px.min(MAX_PX))
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
    // Control characters (a stray newline or tab) take no room and draw
    // nothing; a character the face lacks draws its .notdef box (callers
    // check `supports` on their text when it loads).
    for c in text.chars().filter(|c| !c.is_control()) {
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
/// A size that is not a finite, positive number measures nothing.
pub fn measure(text: &str, face: Face, px: f32) -> Extent {
    let Some(px) = size(px) else {
        return Extent {
            width: 0,
            ascent: 0,
            descent: 0,
            ink: None,
        };
    };
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

/// Rasterizes one line of `text` in `face` at `px` pixels: the line box of
/// [`measure`], grown to hold every glyph's whole ink. A size that is not a
/// finite, positive number letters nothing.
pub fn rasterize(text: &str, face: Face, px: f32) -> Coverage {
    let Some(px) = size(px) else {
        return Coverage::empty();
    };
    let scaled = font(face).as_scaled(PxScale::from(px));
    let e = measure(text, face, px);
    let (width, height) = (e.width, e.ascent + e.descent);
    // How far the ink reaches past each side of the line box.
    let past = |inner: f32, outer: f32| (inner - outer).ceil().max(0.0) as u32;
    let (left, top, right, bottom) = e.ink.map_or((0, 0, 0, 0), |i| {
        (
            past(0.0, i.min_x),
            past(0.0, i.min_y),
            past(i.max_x, width as f32),
            past(i.max_y, height as f32),
        )
    });
    let canvas_width = (left + width + right) as usize;
    let canvas_height = (top + height + bottom) as usize;
    let mut alpha = vec![0u8; canvas_width * canvas_height];
    let (glyphs, _) = layout(text, face, px);
    for (id, x) in glyphs {
        let glyph = id.with_scale_and_position(scaled.scale(), point(x, e.ascent as f32));
        let Some(outlined) = font(face).outline_glyph(glyph) else {
            continue;
        };
        let b = outlined.px_bounds();
        outlined.draw(|gx, gy, c| {
            let x = b.min.x as i64 + i64::from(gx) + i64::from(left);
            let y = b.min.y as i64 + i64::from(gy) + i64::from(top);
            if x < 0 || y < 0 || x >= canvas_width as i64 || y >= canvas_height as i64 {
                return;
            }
            let cell = &mut alpha[y as usize * canvas_width + x as usize];
            *cell = (*cell).max((c.clamp(0.0, 1.0) * 255.0).round() as u8);
        });
    }
    Coverage {
        width,
        height,
        baseline: e.ascent,
        canvas_width: canvas_width as u32,
        left,
        top,
        alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::PlaceKind;

    const FACES: [Face; 3] = [Face::Display, Face::Body, Face::Italic];

    fn ink_rows(cov: &Coverage) -> Vec<i32> {
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
    fn rasterized_text_has_ink_inside_its_measured_bounds() {
        for face in FACES {
            let cov = rasterize("Hola", face, 24.0);
            // The ink stays inside its measured bounds, overhang included.
            let ink = measure("Hola", face, 24.0).ink.expect("ink");
            let mut n = 0;
            cov.for_each(|x, y, a| {
                let (x, y) = (x as f32, y as f32);
                assert!(a > 0, "{face:?}");
                assert!(
                    x + 1.0 >= ink.min_x && x <= ink.max_x + 1.0,
                    "{face:?} x {x}"
                );
                assert!(
                    y + 1.0 >= ink.min_y && y <= ink.max_y + 1.0,
                    "{face:?} y {y}"
                );
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
        assert!(ink_rows(&ag).iter().any(|&y| y >= ag.baseline as i32 + 5));
        assert!(ink_rows(&ab).iter().all(|&y| y <= ab.baseline as i32));
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

    /// The leftmost and rightmost inked columns, from the pen origin.
    fn ink_columns(cov: &Coverage) -> (i64, i64) {
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        cov.for_each(|x, _, _| {
            lo = lo.min(x as i64);
            hi = hi.max(x as i64);
        });
        (lo, hi)
    }

    #[test]
    fn ink_overhanging_the_advance_box_is_kept() {
        // The italic f swings well left of its pen origin; the roman Q's
        // tail runs far past its advance.
        let f = rasterize("f", Face::Italic, 100.0);
        let f_ink = measure("f", Face::Italic, 100.0).ink.expect("ink");
        let (lo, _) = ink_columns(&f);
        assert!(lo < 0, "f starts at {lo}");
        assert!(
            (lo as f32 - f_ink.min_x).abs() <= 1.0,
            "{lo} vs {}",
            f_ink.min_x
        );
        let q = rasterize("Q", Face::Body, 100.0);
        let q_ink = measure("Q", Face::Body, 100.0).ink.expect("ink");
        let (_, hi) = ink_columns(&q);
        assert!(
            hi >= i64::from(q.width),
            "Q ends at {hi}, advance {}",
            q.width
        );
        assert!(
            (hi as f32 + 1.0 - q_ink.max_x).abs() <= 1.0,
            "{hi} vs {}",
            q_ink.max_x
        );
    }

    #[test]
    fn invalid_sizes_letter_nothing() {
        for px in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 0.0] {
            let e = measure("Hola", Face::Body, px);
            assert_eq!(
                e,
                Extent {
                    width: 0,
                    ascent: 0,
                    descent: 0,
                    ink: None
                },
                "{px}"
            );
            let cov = rasterize("Hola", Face::Body, px);
            assert_eq!((cov.width, cov.height), (0, 0), "{px}");
            let mut n = 0;
            cov.for_each(|_, _, _| n += 1);
            assert_eq!(n, 0, "{px}");
        }
    }

    #[test]
    fn huge_sizes_are_capped() {
        assert_eq!(
            measure("A", Face::Body, 1.0e9),
            measure("A", Face::Body, MAX_PX)
        );
    }

    #[test]
    fn control_characters_take_no_room() {
        assert_eq!(
            measure("a\nb\t", Face::Body, 24.0),
            measure("ab", Face::Body, 24.0)
        );
        assert_eq!(
            rasterize("a\nb", Face::Body, 24.0),
            rasterize("ab", Face::Body, 24.0)
        );
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
