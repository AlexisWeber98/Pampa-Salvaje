//! A tiny procedural bitmap font for in-world lettering (signs, map labels,
//! speech bubbles).
//!
//! Capitals are 5x7 pixels and lowercase letters 5x5 with ascenders and
//! descenders. The cell is 12 rows tall: 3 rows above the cap height hold
//! the capitals' accents, and 2 rows below the baseline hold descenders
//! (`g j p q y`, `¡ ¿`). Glyphs are const bit rows: bit 4 is the leftmost
//! column. No third-party font is involved.

/// Glyph cell width in pixels.
pub const GLYPH_WIDTH: usize = 5;
/// Glyph cell height in pixels, accent and descender rows included.
pub const GLYPH_HEIGHT: usize = 12;
/// First row of the capitals (rows above hold accents).
pub const CAP_TOP: usize = 3;
/// Last row of the capitals: letters sit on it, descenders hang below.
pub const BASELINE: usize = CAP_TOP + 6;
/// Rows from the top of the cell down to the baseline, included.
pub const ASCENT: usize = BASELINE + 1;
/// First row of the lowercase x-height (the rows above hold their accents).
pub const X_TOP: usize = CAP_TOP + 2;
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
    (
        '!',
        [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
    ),
    (
        '?',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100,
        ],
    ),
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

/// Rows of a [`LOWER`] bitmap: from the cap top down through the descender.
const LOWER_ROWS: usize = GLYPH_HEIGHT - CAP_TOP;

/// Lowercase letters and the inverted Spanish marks, drawn from the cap top
/// ([`CAP_TOP`]) down: ascenders reach the cap top, the x-height starts at
/// [`X_TOP`], and descenders use the rows below [`BASELINE`].
#[rustfmt::skip]
const LOWER: &[(char, [u8; LOWER_ROWS])] = &[
    //     cap top  .        x-height .        .        .        baseline descender
    ('a', [0,       0,       0b01110, 0b00001, 0b01111, 0b10001, 0b01111, 0,       0      ]),
    ('b', [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b11110, 0,       0      ]),
    ('c', [0,       0,       0b01110, 0b10000, 0b10000, 0b10001, 0b01110, 0,       0      ]),
    ('d', [0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10001, 0b01111, 0,       0      ]),
    ('e', [0,       0,       0b01110, 0b10001, 0b11111, 0b10000, 0b01110, 0,       0      ]),
    ('f', [0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000, 0,       0      ]),
    ('g', [0,       0,       0b01111, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110]),
    ('h', [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b10001, 0,       0      ]),
    ('i', [0b00100, 0,       0b01100, 0b00100, 0b00100, 0b00100, 0b01110, 0,       0      ]),
    ('j', [0b00010, 0,       0b00110, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('k', [0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0,       0      ]),
    ('l', [0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110, 0,       0      ]),
    ('m', [0,       0,       0b11010, 0b10101, 0b10101, 0b10001, 0b10001, 0,       0      ]),
    ('n', [0,       0,       0b10110, 0b11001, 0b10001, 0b10001, 0b10001, 0,       0      ]),
    ('o', [0,       0,       0b01110, 0b10001, 0b10001, 0b10001, 0b01110, 0,       0      ]),
    ('p', [0,       0,       0b11110, 0b10001, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000]),
    ('q', [0,       0,       0b01111, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001]),
    ('r', [0,       0,       0b10110, 0b11001, 0b10000, 0b10000, 0b10000, 0,       0      ]),
    ('s', [0,       0,       0b01111, 0b10000, 0b01110, 0b00001, 0b11110, 0,       0      ]),
    ('t', [0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110, 0,       0      ]),
    ('u', [0,       0,       0b10001, 0b10001, 0b10001, 0b10011, 0b01101, 0,       0      ]),
    ('v', [0,       0,       0b10001, 0b10001, 0b10001, 0b01010, 0b00100, 0,       0      ]),
    ('w', [0,       0,       0b10001, 0b10001, 0b10101, 0b10101, 0b01010, 0,       0      ]),
    ('x', [0,       0,       0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0,       0      ]),
    ('y', [0,       0,       0b10001, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110]),
    ('z', [0,       0,       0b11111, 0b00010, 0b00100, 0b01000, 0b11111, 0,       0      ]),
    // Opening marks: `!` and `?` upside down, from the x-height down
    // through the descender.
    ('¡', [0,       0,       0b00100, 0,       0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('¿', [0,       0,       0b00100, 0,       0b00100, 0b00100, 0b01000, 0b10001, 0b01110]),
];

/// A dotless `i`, the base of `í`.
const DOTLESS_I: [u8; LOWER_ROWS] = [0, 0, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110, 0, 0];

/// Accented lowercase letters: (character, base bitmap, mark). The mark sits
/// in the two rows above the x-height.
const ACCENTED_LOWER: &[(char, char, [u8; 2])] = &[
    ('á', 'a', ACUTE),
    ('é', 'e', ACUTE),
    ('í', 'i', ACUTE),
    ('ó', 'o', ACUTE),
    ('ú', 'u', ACUTE),
    ('ñ', 'n', TILDE),
    ('ü', 'u', DIAERESIS),
];

/// Every character the font can draw.
pub fn charset() -> impl Iterator<Item = char> {
    BASE.iter()
        .map(|&(c, _)| c)
        .chain(ACCENTED.iter().map(|&(c, _, _)| c))
        .chain(LOWER.iter().map(|&(c, _)| c))
        .chain(ACCENTED_LOWER.iter().map(|&(c, _, _)| c))
}

/// Whether the font has a glyph for `c`.
pub fn supports(c: char) -> bool {
    glyph(c).is_some()
}

/// The bitmap of `c`, or `None` if the font has no such glyph.
pub fn glyph(c: char) -> Option<Glyph> {
    let mut g = [0; GLYPH_HEIGHT];
    let base = |c: char| BASE.iter().find(|&&(k, _)| k == c).map(|&(_, rows)| rows);
    let lower = |c: char| LOWER.iter().find(|&&(k, _)| k == c).map(|&(_, rows)| rows);
    if let Some(rows) = base(c) {
        g[CAP_TOP..CAP_TOP + 7].copy_from_slice(&rows);
    } else if let Some(&(_, letter, mark)) = ACCENTED.iter().find(|&&(k, _, _)| k == c) {
        g[..2].copy_from_slice(&mark);
        g[CAP_TOP..CAP_TOP + 7].copy_from_slice(&base(letter)?);
    } else if let Some(rows) = lower(c) {
        g[CAP_TOP..].copy_from_slice(&rows);
    } else if let Some(&(_, letter, mark)) = ACCENTED_LOWER.iter().find(|&&(k, _, _)| k == c) {
        let rows = if letter == 'i' {
            DOTLESS_I
        } else {
            lower(letter)?
        };
        g[CAP_TOP..].copy_from_slice(&rows);
        g[X_TOP - 3..X_TOP - 1].copy_from_slice(&mark);
    } else {
        return None;
    }
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

/// Rows a one-line layout of `text` needs from the top of the cell:
/// [`ASCENT`] unless something descends below the baseline, then the whole
/// [`GLYPH_HEIGHT`]. Capital-only text (signs) never pays for descenders.
pub fn line_rows(text: &str) -> usize {
    match ink_rows(text) {
        Some((_, last)) if last > BASELINE => GLYPH_HEIGHT,
        _ => ASCENT,
    }
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

    /// Rows of `c` that have ink.
    fn rows_with_ink(c: char) -> Vec<usize> {
        let g = glyph(c).unwrap();
        (0..GLYPH_HEIGHT).filter(|&r| g[r] != 0).collect()
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
        let wanted = ('A'..='Z')
            .chain('a'..='z')
            .chain('0'..='9')
            .chain(" .,-:'!?¡¿ÁÉÍÓÚÑÜáéíóúñü".chars());
        for c in wanted {
            assert!(glyph(c).is_some(), "{c:?}");
            assert!(supports(c), "{c:?}");
            assert!(charset().any(|k| k == c), "{c:?}");
        }
        for c in ['#', '"', '~', 'ç', 'ı'] {
            assert_eq!(glyph(c), None, "{c:?}");
            assert!(!supports(c));
        }
        // Every listed character has a glyph, and none is listed twice.
        let all: Vec<char> = charset().collect();
        assert!(all.iter().all(|&c| supports(c)));
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());
    }

    #[test]
    fn lowercase_letters_are_their_own_shapes() {
        for (lower, upper) in ('a'..='z').zip('A'..='Z') {
            assert_ne!(glyph(lower), glyph(upper), "{lower:?}");
        }
        let all: std::collections::HashSet<_> = ('a'..='z').map(|c| glyph(c).unwrap()).collect();
        assert_eq!(all.len(), 26, "distinct shapes");
    }

    #[test]
    fn lowercase_sits_on_the_baseline_with_ascenders_and_descenders() {
        for c in 'a'..='z' {
            let rows = rows_with_ink(c);
            let (top, bottom) = (rows[0], *rows.last().unwrap());
            let ascends = "bdfhklt".contains(c) || c == 'i' || c == 'j';
            let descends = "gjpqy".contains(c);
            if ascends {
                assert!(top <= CAP_TOP + 1, "{c:?} reaches up ({top})");
            } else {
                assert_eq!(top, X_TOP, "{c:?} starts at the x-height");
            }
            if descends {
                assert_eq!(bottom, GLYPH_HEIGHT - 1, "{c:?} descends");
            } else {
                assert_eq!(bottom, BASELINE, "{c:?} sits on the baseline");
            }
        }
        // Capitals and digits never descend.
        for c in ('A'..='Z').chain('0'..='9') {
            assert_eq!(*rows_with_ink(c).last().unwrap(), BASELINE, "{c:?}");
        }
    }

    #[test]
    fn lowercase_accents_sit_above_the_x_height() {
        for (accented, base) in [
            ('á', 'a'),
            ('é', 'e'),
            ('ó', 'o'),
            ('ú', 'u'),
            ('ñ', 'n'),
            ('ü', 'u'),
        ] {
            let (a, b) = (glyph(accented).unwrap(), glyph(base).unwrap());
            assert_eq!(a[X_TOP..], b[X_TOP..], "{accented:?} keeps its letter");
            assert!(
                a[..X_TOP - 1].iter().any(|&r| r != 0),
                "{accented:?} has a mark"
            );
            assert_eq!(a[X_TOP - 1], 0, "a blank row separates the mark");
        }
        // `í` drops the dot of the `i` for the accent.
        let (i, i_acute) = (glyph('i').unwrap(), glyph('í').unwrap());
        assert_eq!(i[X_TOP..], i_acute[X_TOP..]);
        assert_ne!(i[..X_TOP], i_acute[..X_TOP]);
        assert_eq!(i_acute[X_TOP - 1], 0);
    }

    #[test]
    fn spanish_marks_open_and_close() {
        // `¡` and `¿` are the upside-down `!` and `?`, hanging lower.
        for (open, close) in [('¡', '!'), ('¿', '?')] {
            let o = rows_with_ink(open);
            let c = rows_with_ink(close);
            assert_eq!(o.len(), c.len(), "{open:?}");
            assert!(*o.last().unwrap() > BASELINE, "{open:?} descends");
            assert_eq!(*c.last().unwrap(), BASELINE, "{close:?}");
            let (go, gc) = (glyph(open).unwrap(), glyph(close).unwrap());
            let ink = |g: Glyph| g.iter().map(|r| r.count_ones()).sum::<u32>();
            assert_eq!(ink(go), ink(gc), "{open:?}");
        }
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
        assert_eq!(text_width("¡eh!"), text_width("ABCD"));
    }

    #[test]
    fn finds_the_inked_rows() {
        assert_eq!(ink_rows(""), None);
        assert_eq!(ink_rows("   "), None);
        assert_eq!(ink_rows("PLAZA"), Some((CAP_TOP, BASELINE)));
        assert_eq!(ink_rows("COMISARÍA"), Some((0, BASELINE)));
        assert_eq!(ink_rows("-"), Some((CAP_TOP + 3, CAP_TOP + 3)));
        assert_eq!(ink_rows("mesa"), Some((X_TOP, BASELINE)));
        assert_eq!(ink_rows("pingo"), Some((CAP_TOP, GLYPH_HEIGHT - 1)));
    }

    #[test]
    fn one_line_layouts_only_pay_for_descenders_when_used() {
        assert_eq!(line_rows("ESTACIÓN EDUARDO CASTEX"), ASCENT);
        assert_eq!(line_rows(""), ASCENT);
        assert_eq!(line_rows("mesa"), ASCENT);
        assert_eq!(line_rows("pingo"), GLYPH_HEIGHT);
        assert_eq!(line_rows("¡Eh"), GLYPH_HEIGHT);
    }

    #[test]
    fn plots_every_set_bit_inside_the_text_box() {
        let text = "ESTACIÓN EDUARDO CASTEX ¡Qué pingo, che!";
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
