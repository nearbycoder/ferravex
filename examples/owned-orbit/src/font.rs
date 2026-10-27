//! A CPU port of the engine's mixed-case Modern pixel face for the software
//! raster. Glyph art is copied (ASCII plus a few symbols) from
//! `ferravex-render`'s `font_modern.rs` into `modern_glyphs.txt` and parsed once
//! into fixed tables; drawing and measuring never allocate.
//!
//! Cells are 13 rows: the baseline sits under row 10, capitals span rows 3–10
//! and descenders use rows 11–12. Glyphs are proportional and advance by their
//! width plus one spacing column; digits share a 5-pixel width.
use std::sync::OnceLock;

pub(crate) const ROWS: usize = 13;
/// Rows from the top of a cell to the baseline.
pub(crate) const ASCENT: i32 = 11;
/// Capital height in font pixels.
const SPACE_WIDTH: u8 = 3;
const ART: &str = include_str!("modern_glyphs.txt");
const EXTRAS: usize = 8;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Glyph {
    pub width: u8,
    /// Bit `width - 1 - column` is lit; row 0 is the top of the cell.
    pub rows: [u16; ROWS],
}

struct Table {
    ascii: [Option<Glyph>; 128],
    extras: [(char, Glyph); EXTRAS],
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Table {
            ascii: [None; 128],
            extras: [('\0', Glyph::default()); EXTRAS],
        };
        table.ascii[' ' as usize] = Some(Glyph {
            width: SPACE_WIDTH,
            rows: [0; ROWS],
        });
        let mut extras = 0;
        let mut current: Option<(char, Glyph, usize)> = None;
        let mut store = |entry: Option<(char, Glyph, usize)>, table: &mut Table| {
            if let Some((ch, glyph, _)) = entry {
                if (ch as u32) < 128 {
                    table.ascii[ch as usize] = Some(glyph);
                } else if extras < EXTRAS {
                    table.extras[extras] = (ch, glyph);
                    extras += 1;
                }
            }
        };
        for line in ART.lines().filter(|l| !l.is_empty()) {
            if let Some((head, top)) = line.rsplit_once(' ') {
                store(current.take(), &mut table);
                let ch = head.chars().next().expect("glyph header");
                current = Some((ch, Glyph::default(), top.parse().expect("glyph top row")));
            } else if let Some((_, glyph, row)) = &mut current {
                glyph.width = line.len() as u8;
                let mut bits = 0;
                for byte in line.bytes() {
                    bits = bits << 1 | u16::from(byte == b'#');
                }
                if *row < ROWS {
                    glyph.rows[*row] = bits;
                }
                *row += 1;
            }
        }
        store(current, &mut table);
        table
    })
}

pub(crate) fn glyph(c: char) -> Option<Glyph> {
    let table = table();
    if (c as u32) < 128 {
        return table.ascii[c as usize];
    }
    table
        .extras
        .iter()
        .find(|(ch, _)| *ch == c)
        .map(|(_, g)| *g)
}

/// Kerning adjustment in font pixels for an ordered pair (ported as-is).
pub(crate) fn kerning(left: char, right: char) -> i32 {
    let tight_right = matches!(right, '.' | ',' | '…');
    match left {
        'T' => -i32::from(
            matches!(
                right,
                'a' | 'c' | 'e' | 'g' | 'm' | 'n' | 'o' | 'p' | 'q' | 'r' | 's'
            ) || matches!(right, 'u' | 'v' | 'w' | 'x' | 'y' | 'z' | '.' | ','),
        ),
        'L' => -i32::from(matches!(right, 'T' | 'V' | 'Y' | '\'' | '"')),
        'F' | 'P' | 'r' => -i32::from(tight_right),
        'V' | 'W' | 'Y' => -i32::from(tight_right || matches!(right, 'a' | 'c' | 'e' | 'o')),
        _ => 0,
    }
}

/// Applies the display case used for Modern body copy: authored ALL-CAPS
/// copy becomes sentence case; letters after `.`, `!` or `?` and a space,
/// and the first letter, stay capitals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Case {
    AsIs,
    Sentence,
}

/// Iterate the characters actually drawn for `text` in `case`.
pub(crate) fn cased(text: &str, case: Case) -> impl Iterator<Item = char> + '_ {
    let mut start = true;
    let mut previous = ' ';
    text.chars().map(move |c| {
        let out = match case {
            Case::AsIs => c,
            Case::Sentence if start && c.is_ascii_alphabetic() => {
                start = false;
                c
            }
            Case::Sentence => c.to_ascii_lowercase(),
        };
        if c == ' ' && matches!(previous, '.' | '!' | '?') {
            start = true;
        }
        previous = c;
        out
    })
}

/// Width in output pixels of `text` at integer `scale`, without allocation.
pub(crate) fn width(text: &str, scale: i32, case: Case) -> i32 {
    let mut total = 0;
    let mut previous: Option<char> = None;
    for c in cased(text, case) {
        let advance = glyph(c).map_or(SPACE_WIDTH, |g| g.width) as i32 + 1;
        total += advance + previous.map_or(0, |p| kerning(p, c));
        previous = Some(c);
    }
    (total - 1).max(0) * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ported_face_has_tabular_digits_lowercase_and_symbols() {
        for d in '0'..='9' {
            assert_eq!(glyph(d).unwrap().width, 5, "{d}");
        }
        for c in ('a'..='z').chain('A'..='Z').chain("!?.,'×→★•·…".chars()) {
            let g = glyph(c).unwrap_or_else(|| panic!("missing {c}"));
            assert!(g.width > 0 && g.width <= 8);
            for row in g.rows {
                assert_eq!(row >> g.width, 0, "{c} ink outside its width");
            }
        }
        // Descenders reach below the baseline.
        assert_ne!(glyph('g').unwrap().rows[11], 0);
        assert_eq!(width("", 1, Case::AsIs), 0);
        assert_eq!(width("00", 2, Case::AsIs), 22);
    }

    #[test]
    fn sentence_case_keeps_sentence_starts() {
        let text: String = cased("IT'S FINE. SPACE IS BIG!", Case::Sentence).collect();
        assert_eq!(text, "It's fine. Space is big!");
        assert!(width("HELLO", 1, Case::Sentence) < width("HELLO", 1, Case::AsIs));
    }
}
