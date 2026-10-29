//! Rotulado en CPU con las tipografías TrueType del juego.
//!
//! Las texturas del mundo (carteles, globos de diálogo, el minimapa) se pintan
//! en la CPU, adonde no llega el pipeline de texto de Bevy. Este módulo dispone
//! una línea de texto con `ab_glyph`, usando los mismos archivos de fuente que
//! carga la UI a través de [`crate::typeface`], y devuelve la cobertura con
//! antialiasing para trazar.

use std::sync::LazyLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};

/// Rye, embebida. El mismo archivo que [`crate::typeface::DISPLAY_FONT`] bajo
/// `assets/`.
const DISPLAY_BYTES: &[u8] = include_bytes!("../../../assets/fonts/rye/Rye-Regular.ttf");
/// IM Fell English redonda, embebida. El mismo archivo que
/// [`crate::typeface::BODY_FONT`].
const BODY_BYTES: &[u8] = include_bytes!("../../../assets/fonts/im_fell_english/IMFeENrm28P.ttf");
/// IM Fell English cursiva, embebida. El mismo archivo que
/// [`crate::typeface::ITALIC_FONT`].
const ITALIC_BYTES: &[u8] = include_bytes!("../../../assets/fonts/im_fell_english/IMFeENit28P.ttf");

static DISPLAY: LazyLock<FontRef<'static>> = LazyLock::new(|| parse(bytes(Face::Display)));
static BODY: LazyLock<FontRef<'static>> = LazyLock::new(|| parse(bytes(Face::Body)));
static ITALIC: LazyLock<FontRef<'static>> = LazyLock::new(|| parse(bytes(Face::Italic)));

fn parse(bytes: &'static [u8]) -> FontRef<'static> {
    FontRef::try_from_slice(bytes).expect("the shipped typefaces are valid TrueType")
}

/// Las tipografías del juego, nombradas según su función en
/// [`crate::typeface`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Rye, para carteles pintados sobre madera (`typeface::DISPLAY_FONT`).
    Display,
    /// IM Fell English redonda, para el texto de cuerpo
    /// (`typeface::BODY_FONT`).
    Body,
    /// IM Fell English cursiva (`typeface::ITALIC_FONT`).
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

/// Límites entintados de una línea, en píxeles desde el origen de la pluma; `y`
/// crece hacia abajo desde la parte superior de la línea.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

/// Tamaño de una línea ya dispuesta.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    /// Ancho de avance en píxeles, redondeado hacia arriba (lo que quien llama
    /// centra o encuadra).
    pub width: u32,
    /// Píxeles desde la parte superior de la línea hasta la línea de base.
    pub ascent: u32,
    /// Píxeles desde la línea de base hasta la parte inferior de la línea.
    pub descent: u32,
    /// Límites de la cobertura real, `None` para texto en blanco.
    pub ink: Option<Ink>,
}

/// Cobertura con antialiasing de una línea.
///
/// `width`, `height` y `baseline` describen la caja de la línea: su ancho de
/// avance por su ascendente más su descendente. El lienzo que hay detrás crece
/// para contener toda la tinta, así que un glifo que sobresale de su avance (el
/// adorno de la f cursiva, la cola de la Q redonda, el gancho de una j) se
/// conserva, no se recorta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage {
    /// Ancho de avance en píxeles: lo que quien llama centra o encuadra.
    pub width: u32,
    /// Alto de línea en píxeles: ascendente más descendente.
    pub height: u32,
    /// Fila de la línea de base (el ascendente de la línea).
    pub baseline: u32,
    /// El ancho del lienzo: la caja de la línea más cualquier saliente a cada
    /// lado.
    canvas_width: u32,
    /// Columnas del lienzo a la izquierda de la caja de la línea, y filas por
    /// encima de ella.
    left: u32,
    top: u32,
    /// Cobertura `0..=255` por píxel del lienzo, por filas.
    alpha: Vec<u8>,
}

impl Coverage {
    /// Nada de tinta, en una caja vacía.
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

    /// Llama a `f(x, y, alpha)` para cada píxel con cobertura, con `x` e `y`
    /// medidos desde la esquina superior izquierda de la caja de la línea (el
    /// origen de la pluma en la parte superior de la línea). La tinta que
    /// sobresale de la caja llega con coordenadas negativas o con valores
    /// iguales o mayores que `width` y `height`.
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

/// El tamaño máximo de rotulado, en píxeles: un pedido mayor se rotula a este
/// tamaño, así que el lienzo de ninguna línea puede crecer sin límite.
pub const MAX_PX: f32 = 512.0;

/// `px` como tamaño de rotulado: `None` cuando no es un número finito y
/// positivo; limitado a [`MAX_PX`].
fn size(px: f32) -> Option<f32> {
    (px.is_finite() && px > 0.0).then(|| px.min(MAX_PX))
}

/// Verdadero cuando `face` tiene un glifo real para `c`; los espacios en blanco
/// siempre cuentan.
pub fn supports(face: Face, c: char) -> bool {
    c.is_whitespace() || font(face).glyph_id(c).0 != 0
}

/// Un glifo posicionado: su id y la x de la pluma.
fn layout(text: &str, face: Face, px: f32) -> (Vec<(ab_glyph::GlyphId, f32)>, f32) {
    let scaled = font(face).as_scaled(PxScale::from(px));
    let mut pen = 0.0;
    let mut prev = None;
    let mut glyphs = Vec::new();
    // Los caracteres de control (un salto de línea o tabulación sueltos) no
    // ocupan lugar ni dibujan nada; un carácter que le falta a la tipografía
    // dibuja su caja .notdef (quienes llaman verifican `supports` sobre su
    // texto cuando se carga).
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

/// Mide una línea de `text` en `face` a `px` píxeles, con kerning incluido. Un
/// tamaño que no sea un número finito y positivo no mide nada.
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

/// Rasteriza una línea de `text` en `face` a `px` píxeles: la caja de línea de
/// [`measure`], agrandada para contener toda la tinta de cada glifo. Un tamaño
/// que no sea un número finito y positivo no rotula nada.
pub fn rasterize(text: &str, face: Face, px: f32) -> Coverage {
    let Some(px) = size(px) else {
        return Coverage::empty();
    };
    let scaled = font(face).as_scaled(PxScale::from(px));
    let e = measure(text, face, px);
    let (width, height) = (e.width, e.ascent + e.descent);
    // Cuánto sobresale la tinta de cada lado de la caja de la línea.
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
        // La cursiva aplica un kerning de unos -12 px entre V y A a 100 px.
        let apart =
            measure("V", Face::Italic, 100.0).width + measure("A", Face::Italic, 100.0).width;
        let pair = measure("VA", Face::Italic, 100.0).width;
        assert!(pair + 8 < apart, "{pair} vs {apart}");
    }

    #[test]
    fn rasterized_text_has_ink_inside_its_measured_bounds() {
        for face in FACES {
            let cov = rasterize("Hola", face, 24.0);
            // La tinta queda dentro de sus límites medidos, saliente incluido.
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
        // Un descendente baja bastante por debajo de la línea de base; las
        // mayúsculas solo la sobrepasan por una fila.
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

    /// Las columnas entintadas de más a la izquierda y más a la derecha, desde
    /// el origen de la pluma.
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
        // La f cursiva se extiende bastante a la izquierda del origen de su
        // pluma; la cola de la Q redonda llega mucho más allá de su avance.
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
