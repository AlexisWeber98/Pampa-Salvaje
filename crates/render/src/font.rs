//! A tiny procedural bitmap font for in-world lettering (signs, map labels).
//!
//! Capitals are 5x7 pixels; the cell is 10 rows tall so accents and the
//! tilde fit above the cap height. Glyphs are const bit rows: bit 4 is the
//! leftmost column. No third-party font is involved.

/// Glyph cell width in pixels.
pub const GLYPH_WIDTH: usize = 5;
/// Glyph cell height in pixels, accent rows included.
pub const GLYPH_HEIGHT: usize = 10;
/// First row of the capitals (rows above hold accents).
pub const CAP_TOP: usize = 3;
/// Blank columns between two glyphs.
pub const LETTER_SPACING: usize = 1;

/// One glyph: [`GLYPH_HEIGHT`] rows of [`GLYPH_WIDTH`] bits.
pub type Glyph = [u8; GLYPH_HEIGHT];

/// Capital-height bitmaps (7 rows) of the base characters.
const BASE: &[(char, [u8; 7])] = &[
    (
        'A',
        [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'B',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'C',
        [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
    ),
    (
        'D',
        [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'E',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'F',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'G',
        [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
    ),
    (
        'H',
        [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'I',
        [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        'J',
        [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
    ),
    (
        'K',
        [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'L',
        [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'M',
        [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'N',
        [
            0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ],
    ),
    (
        'O',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'P',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'Q',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
    ),
    (
        'R',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'S',
        [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
    ),
    (
        'T',
        [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'U',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'V',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
    ),
    (
        'W',
        [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
    ),
    (
        'X',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
    ),
    (
        'Y',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'Z',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
    ),
    (
        '0',
        [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
    ),
    (
        '1',
        [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        '2',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
    ),
    (
        '3',
        [
            0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110,
        ],
    ),
    (
        '4',
        [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
    ),
    (
        '5',
        [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
    ),
    (
        '6',
        [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '7',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
    ),
    (
        '8',
        [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '9',
        [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
    ),
    (' ', [0; 7]),
    ('.', [0, 0, 0, 0, 0, 0b01100, 0b01100]),
    (',', [0, 0, 0, 0, 0b01100, 0b00100, 0b01000]),
    ('-', [0, 0, 0, 0b11111, 0, 0, 0]),
    (':', [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0]),
    ('\'', [0b00100, 0b00100, 0b01000, 0, 0, 0, 0]),
];

/// Marks drawn in the two rows above the capitals.
const ACUTE: [u8; 2] = [0b00010, 0b00100];
const TILDE: [u8; 2] = [0b01101, 0b10110];
const DIAERESIS: [u8; 2] = [0, 0b01010];

/// Accented capitals: (character, base letter, mark).
const ACCENTED: &[(char, char, [u8; 2])] = &[
    ('Á', 'A', ACUTE),
    ('É', 'E', ACUTE),
    ('Í', 'I', ACUTE),
    ('Ó', 'O', ACUTE),
    ('Ú', 'U', ACUTE),
    ('Ñ', 'N', TILDE),
    ('Ü', 'U', DIAERESIS),
];

/// Every character the font can draw.
pub fn charset() -> impl Iterator<Item = char> {
    BASE.iter()
        .map(|&(c, _)| c)
        .chain(ACCENTED.iter().map(|&(c, _, _)| c))
}

/// The bitmap of `c`, or `None` if the font has no such glyph. Lowercase
/// letters are drawn as capitals.
pub fn glyph(c: char) -> Option<Glyph> {
    let c = c.to_uppercase().next().unwrap_or(c);
    let base = |c: char| BASE.iter().find(|&&(k, _)| k == c).map(|&(_, rows)| rows);
    let (rows, mark) = match ACCENTED.iter().find(|&&(k, _, _)| k == c) {
        Some(&(_, letter, mark)) => (base(letter)?, mark),
        None => (base(c)?, [0, 0]),
    };
    let mut g = [0; GLYPH_HEIGHT];
    g[..2].copy_from_slice(&mark);
    g[CAP_TOP..].copy_from_slice(&rows);
    Some(g)
}

/// Width in pixels of `text` drawn in one line.
pub fn text_width(text: &str) -> usize {
    let n = text.chars().count();
    (n * (GLYPH_WIDTH + LETTER_SPACING)).saturating_sub(LETTER_SPACING)
}

/// First and last glyph rows with ink in `text` (`None` if nothing is inked),
/// so a layout can center what is actually drawn.
pub fn ink_rows(text: &str) -> Option<(usize, usize)> {
    let mut rows = [0u8; GLYPH_HEIGHT];
    for g in text.chars().filter_map(glyph) {
        for (row, bits) in rows.iter_mut().zip(g) {
            *row |= bits;
        }
    }
    let first = rows.iter().position(|&r| r != 0)?;
    let last = rows.iter().rposition(|&r| r != 0)?;
    Some((first, last))
}

/// Calls `plot(x, y)` for every inked pixel of `text`, with the top-left of
/// the first glyph cell at `(0, 0)`. Missing glyphs draw nothing.
pub fn for_each_pixel(text: &str, mut plot: impl FnMut(usize, usize)) {
    for (n, c) in text.chars().enumerate() {
        let Some(g) = glyph(c) else {
            continue;
        };
        let left = n * (GLYPH_WIDTH + LETTER_SPACING);
        for (y, bits) in g.iter().enumerate() {
            for x in 0..GLYPH_WIDTH {
                if bits & (1 << (GLYPH_WIDTH - 1 - x)) != 0 {
                    plot(left + x, y);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::PlaceKind;

    fn inked(text: &str) -> usize {
        let mut n = 0;
        for_each_pixel(text, |_, _| n += 1);
        n
    }

    #[test]
    fn covers_every_character_of_every_place_name() {
        for kind in PlaceKind::ALL {
            for c in kind.name().chars() {
                assert!(glyph(c).is_some(), "missing glyph {c:?} in {}", kind.name());
            }
        }
    }

    #[test]
    fn covers_the_promised_charset() {
        let wanted = ('A'..='Z').chain('0'..='9').chain(" .,-:'ÁÉÍÓÚÑÜ".chars());
        for c in wanted {
            assert!(glyph(c).is_some(), "{c:?}");
            assert!(charset().any(|k| k == c), "{c:?}");
        }
        assert_eq!(glyph('a'), glyph('A'));
        assert_eq!(glyph('ñ'), glyph('Ñ'));
        assert_eq!(glyph('#'), None);
    }

    #[test]
    fn glyphs_fit_their_cell_and_letters_have_ink() {
        for c in charset() {
            let g = glyph(c).unwrap();
            assert!(g.iter().all(|&row| row < 1 << GLYPH_WIDTH), "{c:?}");
            if c.is_alphanumeric() {
                assert!(g[CAP_TOP..].iter().any(|&row| row != 0), "{c:?}");
            }
        }
        // Plain capitals keep the accent rows clear.
        assert!(glyph('E').unwrap()[..CAP_TOP].iter().all(|&r| r == 0));
    }

    #[test]
    fn accents_sit_above_the_unchanged_base_letter() {
        for (accented, base) in [
            ('Á', 'A'),
            ('É', 'E'),
            ('Í', 'I'),
            ('Ó', 'O'),
            ('Ú', 'U'),
            ('Ñ', 'N'),
            ('Ü', 'U'),
        ] {
            let (a, b) = (glyph(accented).unwrap(), glyph(base).unwrap());
            assert_eq!(a[CAP_TOP..], b[CAP_TOP..], "{accented:?}");
            assert!(
                a[..CAP_TOP].iter().any(|&r| r != 0),
                "{accented:?} has a mark"
            );
            assert_eq!(a[CAP_TOP - 1], 0, "a blank row separates the mark");
        }
        assert_ne!(glyph('Á'), glyph('Ñ'));
    }

    #[test]
    fn measures_text_width_with_letter_spacing() {
        assert_eq!(text_width(""), 0);
        assert_eq!(text_width("A"), GLYPH_WIDTH);
        assert_eq!(text_width("AB"), 2 * GLYPH_WIDTH + LETTER_SPACING);
        assert_eq!(text_width("PLAZA"), 5 * GLYPH_WIDTH + 4 * LETTER_SPACING);
        assert_eq!(text_width("ÉÑ"), text_width("EN"));
    }

    #[test]
    fn finds_the_inked_rows() {
        assert_eq!(ink_rows(""), None);
        assert_eq!(ink_rows("   "), None);
        assert_eq!(ink_rows("PLAZA"), Some((CAP_TOP, GLYPH_HEIGHT - 1)));
        assert_eq!(ink_rows("COMISARÍA"), Some((0, GLYPH_HEIGHT - 1)));
        assert_eq!(ink_rows("-"), Some((CAP_TOP + 3, CAP_TOP + 3)));
    }

    #[test]
    fn plots_every_set_bit_inside_the_text_box() {
        let text = "ESTACIÓN EDUARDO CASTEX";
        let bits: u32 = text
            .chars()
            .map(|c| {
                glyph(c)
                    .unwrap()
                    .iter()
                    .map(|r| r.count_ones())
                    .sum::<u32>()
            })
            .sum();
        assert_eq!(inked(text) as u32, bits);
        let width = text_width(text);
        for_each_pixel(text, |x, y| {
            assert!(x < width && y < GLYPH_HEIGHT, "({x}, {y})");
        });
        // The second glyph starts one spacing past the first.
        let mut xs = Vec::new();
        for_each_pixel("IT", |x, y| {
            if y == CAP_TOP {
                xs.push(x);
            }
        });
        assert_eq!(xs, [1, 2, 3, 6, 7, 8, 9, 10]);
        assert_eq!(inked(" "), 0);
    }
}
