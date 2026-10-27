//! Hand-authored mixed-case bitmap face used by [`crate::text::FontFace::Modern`].
//!
//! Every glyph is drawn below as readable pixel art and parsed at compile time;
//! there is no atlas, file I/O or runtime initialization. The grid has 13 rows:
//!
//! | rows   | purpose                                         |
//! |--------|-------------------------------------------------|
//! | 0–2    | capital accents (rows 0–1) and their gap        |
//! | 3–10   | cap height (8 rows); the baseline is under row 10 |
//! | 5–10   | x-height (6 rows); ascenders start at row 3     |
//! | 11–12  | descenders                                      |
//!
//! Glyphs are proportional (1–7 pixels wide) and advance by their width plus one
//! spacing column. Digits share a 5-pixel width so numbers stay tabular. Accented
//! Latin letters are composed from a base glyph plus an accent mark.

/// Total bitmap rows.
pub(crate) const ROWS: usize = 13;
/// Rows from the top of the cell to the baseline.
pub(crate) const ASCENT: u8 = 11;
/// Rows below the baseline.
pub(crate) const DESCENT: u8 = 2;
/// Baseline-to-baseline distance, including one leading row.
pub(crate) const LINE_HEIGHT: u8 = 14;
pub(crate) const CAP_HEIGHT: u8 = 8;
pub(crate) const X_HEIGHT: u8 = 6;
/// Width of the space glyph; its advance adds the usual spacing column.
const SPACE_WIDTH: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Glyph {
    pub ch: char,
    pub width: u8,
    /// Bit `width - 1 - column` is lit; row 0 is the top of the cell.
    pub rows: [u16; ROWS],
}

impl Glyph {
    const EMPTY: Self = Self {
        ch: '\0',
        width: 0,
        rows: [0; ROWS],
    };
}

// Pixel art: a header line `<char> <top-row>` followed by equal-width rows of
// `#` (lit) and `.` (clear). Rows below the art are clear.
const ART: &str = r#"
A 3
.###.
#...#
#...#
#...#
#####
#...#
#...#
#...#
B 3
####.
#...#
#...#
####.
#...#
#...#
#...#
####.
C 3
.###.
#...#
#....
#....
#....
#....
#...#
.###.
D 3
####.
#...#
#...#
#...#
#...#
#...#
#...#
####.
E 3
#####
#....
#....
####.
#....
#....
#....
#####
F 3
#####
#....
#....
####.
#....
#....
#....
#....
G 3
.###.
#...#
#....
#....
#.###
#...#
#...#
.####
H 3
#...#
#...#
#...#
#####
#...#
#...#
#...#
#...#
I 3
###
.#.
.#.
.#.
.#.
.#.
.#.
###
J 3
...#
...#
...#
...#
...#
...#
#..#
.##.
K 3
#...#
#..#.
#.#..
##...
#.#..
#..#.
#...#
#...#
L 3
#....
#....
#....
#....
#....
#....
#....
#####
M 3
#...#
##.##
#.#.#
#.#.#
#...#
#...#
#...#
#...#
N 3
#...#
##..#
##..#
#.#.#
#.#.#
#..##
#..##
#...#
O 3
.###.
#...#
#...#
#...#
#...#
#...#
#...#
.###.
P 3
####.
#...#
#...#
#...#
####.
#....
#....
#....
Q 3
.###.
#...#
#...#
#...#
#...#
#.#.#
#..#.
.##.#
R 3
####.
#...#
#...#
#...#
####.
#.#..
#..#.
#...#
S 3
.###.
#...#
#....
.###.
....#
....#
#...#
.###.
T 3
#####
..#..
..#..
..#..
..#..
..#..
..#..
..#..
U 3
#...#
#...#
#...#
#...#
#...#
#...#
#...#
.###.
V 3
#...#
#...#
#...#
#...#
#...#
.#.#.
.#.#.
..#..
W 3
#...#
#...#
#...#
#...#
#.#.#
#.#.#
##.##
#...#
X 3
#...#
#...#
.#.#.
..#..
..#..
.#.#.
#...#
#...#
Y 3
#...#
#...#
.#.#.
..#..
..#..
..#..
..#..
..#..
Z 3
#####
....#
...#.
..#..
.#...
#....
#....
#####
a 5
.##.
...#
.###
#..#
#..#
.###
b 3
#...
#...
###.
#..#
#..#
#..#
#..#
###.
c 5
.###
#...
#...
#...
#...
.###
d 3
...#
...#
.###
#..#
#..#
#..#
#..#
.###
e 5
.##.
#..#
####
#...
#...
.###
f 3
.##
#..
###
#..
#..
#..
#..
#..
g 5
.###
#..#
#..#
#..#
#..#
.###
...#
###.
h 3
#...
#...
###.
#..#
#..#
#..#
#..#
#..#
i 3
#
.
#
#
#
#
#
#
j 3
.#
..
.#
.#
.#
.#
.#
.#
.#
#.
k 3
#...
#...
#..#
#.#.
##..
#.#.
#..#
#..#
l 3
#.
#.
#.
#.
#.
#.
#.
.#
m 5
####.
#.#.#
#.#.#
#.#.#
#.#.#
#.#.#
n 5
###.
#..#
#..#
#..#
#..#
#..#
o 5
.##.
#..#
#..#
#..#
#..#
.##.
p 5
###.
#..#
#..#
#..#
#..#
###.
#...
#...
q 5
.###
#..#
#..#
#..#
#..#
.###
...#
...#
r 5
#.##
##..
#...
#...
#...
#...
s 5
.###
#...
.##.
...#
...#
###.
t 4
#..
###
#..
#..
#..
#..
.##
u 5
#..#
#..#
#..#
#..#
#..#
.###
v 5
#...#
#...#
#...#
.#.#.
.#.#.
..#..
w 5
#...#
#...#
#.#.#
#.#.#
#.#.#
.#.#.
x 5
#..#
#..#
.##.
.##.
#..#
#..#
y 5
#..#
#..#
#..#
#..#
#..#
.###
...#
###.
z 5
####
...#
..#.
.#..
#...
####
ı 5
#
#
#
#
#
#
0 3
.###.
#...#
#..##
#.#.#
#.#.#
##..#
#...#
.###.
1 3
..#..
.##..
#.#..
..#..
..#..
..#..
..#..
#####
2 3
.###.
#...#
....#
...#.
..#..
.#...
#....
#####
3 3
.###.
#...#
....#
..##.
....#
....#
#...#
.###.
4 3
...#.
..##.
.#.#.
#..#.
#####
...#.
...#.
...#.
5 3
#####
#....
#....
####.
....#
....#
#...#
.###.
6 3
.###.
#....
#....
####.
#...#
#...#
#...#
.###.
7 3
#####
....#
...#.
...#.
..#..
..#..
..#..
..#..
8 3
.###.
#...#
#...#
.###.
#...#
#...#
#...#
.###.
9 3
.###.
#...#
#...#
#...#
.####
....#
....#
.###.
! 3
#
#
#
#
#
#
.
#
" 3
#.#
#.#
# 4
.#.#.
#####
.#.#.
.#.#.
#####
.#.#.
$ 2
..#..
.####
#.#..
#.#..
.###.
..#.#
..#.#
####.
..#..
% 4
##..#
##.#.
...#.
..#..
.#...
.#.##
#..##
& 3
.##..
#..#.
#..#.
.##..
##..#
#.#.#
#..#.
.##.#
' 3
#
#
( 3
.#
#.
#.
#.
#.
#.
#.
#.
.#
) 3
#.
.#
.#
.#
.#
.#
.#
.#
#.
* 4
..#..
#.#.#
.###.
#.#.#
..#..
+ 5
..#..
..#..
#####
..#..
..#..
, 10
.#
#.
- 7
###
. 10
#
/ 3
...#
...#
..#.
..#.
.#..
.#..
#...
#...
: 6
#
.
.
.
#
; 6
.#
..
..
..
.#
#.
< 4
...#
..#.
.#..
#...
.#..
..#.
...#
= 6
####
....
####
> 4
#...
.#..
..#.
...#
..#.
.#..
#...
? 3
.###.
#...#
....#
...#.
..#..
..#..
.....
..#..
@ 3
.###.
#...#
#.###
#.#.#
#.#.#
#.##.
#....
.###.
[ 3
##
#.
#.
#.
#.
#.
#.
#.
##
\ 3
#...
#...
.#..
.#..
..#.
..#.
...#
...#
] 3
##
.#
.#
.#
.#
.#
.#
.#
##
^ 3
..#..
.#.#.
#...#
_ 11
#####
` 3
#.
.#
{ 3
..#
.#.
.#.
.#.
#..
.#.
.#.
.#.
..#
| 3
#
#
#
#
#
#
#
#
#
} 3
#..
.#.
.#.
.#.
..#
.#.
.#.
.#.
#..
~ 6
.##.#
#..#.
• 6
.##.
####
####
.##.
· 7
##
##
× 5
#...#
.#.#.
..#..
.#.#.
#...#
÷ 5
..#..
.....
#####
.....
..#..
± 4
..#..
..#..
#####
..#..
..#..
.....
#####
→ 5
....#..
.....#.
#######
.....#.
....#..
← 5
..#....
.#.....
#######
.#.....
..#....
↑ 3
..#..
.###.
#.#.#
..#..
..#..
..#..
..#..
..#..
↓ 3
..#..
..#..
..#..
..#..
..#..
#.#.#
.###.
..#..
★ 3
...#...
...#...
..###..
#######
.#####.
..###..
.##.##.
.#...#.
☆ 3
...#...
...#...
..#.#..
##...##
.#...#.
..###..
.##.##.
.#...#.
♥ 4
.##.##.
#######
#######
#######
.#####.
..###..
...#...
♦ 4
...#...
..###..
.#####.
#######
.#####.
..###..
...#...
♠ 4
...#...
..###..
.#####.
#######
##.#.##
...#...
..###..
♣ 4
..###..
..###..
##.#.##
#######
##.#.##
...#...
..###..
° 3
.#.
#.#
.#.
… 10
#.#.#
– 7
#####
— 7
#######
‘ 3
.#
#.
#.
’ 3
.#
.#
#.
“ 3
.#.#
#.#.
#.#.
” 3
.#.#
.#.#
#.#.
© 3
.#####.
#.....#
#..##.#
#.#...#
#.#...#
#..##.#
#.....#
.#####.
✓ 5
......#
.....#.
#...#..
.#.#...
..#....
€ 3
..###
.#...
####.
.#...
####.
.#...
.#...
..###
£ 3
..##.
.#..#
.#...
.#...
####.
.#...
.#...
#####
¡ 5
#
.
#
#
#
#
#
#
¿ 5
..#..
.....
..#..
..#..
.#...
#....
#...#
.###.
« 5
..#.#
.#.#.
#.#..
.#.#.
..#.#
» 5
#.#..
.#.#.
..#.#
.#.#.
#.#..
▲ 5
...#...
..###..
.#####.
#######
▼ 5
#######
.#####.
..###..
...#...
▶ 4
#...
##..
###.
####
###.
##..
#...
◀ 4
...#
..##
.###
####
.###
..##
...#
■ 5
#####
#####
#####
#####
#####
□ 5
#####
#...#
#...#
#...#
#####
● 5
.###.
#####
#####
#####
.###.
○ 5
.###.
#...#
#...#
#...#
.###.
∞ 6
.##.##.
#..#..#
.##.##.
≤ 4
...#
..#.
.#..
..#.
...#
....
####
≥ 4
#...
.#..
..#.
.#..
#...
....
####
ß 3
.##.
#..#
#..#
#.#.
#..#
#..#
#..#
#.#.
æ 5
.##.##.
...#..#
.######
#..#...
#..#...
.##.###
ø 4
....#
.###.
#..##
#.#.#
#.#.#
##..#
.###.
#....
Ø 2
....#
.###.
#..##
#..##
#.#.#
#.#.#
##..#
##..#
.###.
#....
"#;

#[derive(Clone, Copy)]
enum Accent {
    Grave,
    Acute,
    Circumflex,
    Tilde,
    Diaeresis,
    Ring,
    Cedilla,
}

/// Composed Latin-1 letters: (letter, base glyph, accent).
const COMPOSED: &[(char, char, Accent)] = {
    use Accent::*;
    &[
        ('À', 'A', Grave),
        ('Á', 'A', Acute),
        ('Â', 'A', Circumflex),
        ('Ã', 'A', Tilde),
        ('Ä', 'A', Diaeresis),
        ('Å', 'A', Ring),
        ('Ç', 'C', Cedilla),
        ('È', 'E', Grave),
        ('É', 'E', Acute),
        ('Ê', 'E', Circumflex),
        ('Ë', 'E', Diaeresis),
        ('Ì', 'I', Grave),
        ('Í', 'I', Acute),
        ('Î', 'I', Circumflex),
        ('Ï', 'I', Diaeresis),
        ('Ñ', 'N', Tilde),
        ('Ò', 'O', Grave),
        ('Ó', 'O', Acute),
        ('Ô', 'O', Circumflex),
        ('Õ', 'O', Tilde),
        ('Ö', 'O', Diaeresis),
        ('Ù', 'U', Grave),
        ('Ú', 'U', Acute),
        ('Û', 'U', Circumflex),
        ('Ü', 'U', Diaeresis),
        ('Ý', 'Y', Acute),
        ('à', 'a', Grave),
        ('á', 'a', Acute),
        ('â', 'a', Circumflex),
        ('ã', 'a', Tilde),
        ('ä', 'a', Diaeresis),
        ('å', 'a', Ring),
        ('ç', 'c', Cedilla),
        ('è', 'e', Grave),
        ('é', 'e', Acute),
        ('ê', 'e', Circumflex),
        ('ë', 'e', Diaeresis),
        ('ì', 'ı', Grave),
        ('í', 'ı', Acute),
        ('î', 'ı', Circumflex),
        ('ï', 'ı', Diaeresis),
        ('ñ', 'n', Tilde),
        ('ò', 'o', Grave),
        ('ó', 'o', Acute),
        ('ô', 'o', Circumflex),
        ('õ', 'o', Tilde),
        ('ö', 'o', Diaeresis),
        ('ù', 'u', Grave),
        ('ú', 'u', Acute),
        ('û', 'u', Circumflex),
        ('ü', 'u', Diaeresis),
        ('ý', 'y', Acute),
        ('ÿ', 'y', Diaeresis),
    ]
};

/// Characters drawn with another glyph's art.
const ALIASES: &[(char, char)] = &[
    ('\u{a0}', ' '),
    ('✕', '×'),
    ('✗', '×'),
    ('∙', '•'),
    ('−', '-'),
    ('‐', '-'),
    ('′', '\''),
    ('″', '"'),
];

const CAPACITY: usize = 256;

pub(crate) struct Table {
    glyphs: [Glyph; CAPACITY],
    len: usize,
    /// ASCII lookup: glyph index or `u8::MAX`.
    ascii: [u8; 128],
}

const fn find(glyphs: &[Glyph; CAPACITY], len: usize, c: char) -> Option<usize> {
    let mut i = 0;
    while i < len {
        if glyphs[i].ch == c {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Decodes one UTF-8 scalar starting at `at`; returns (char, byte length).
const fn decode(bytes: &[u8], at: usize) -> (char, usize) {
    let b = bytes[at];
    let (len, init) = if b < 0x80 {
        (1, b as u32)
    } else if b >> 5 == 0b110 {
        (2, (b & 0x1f) as u32)
    } else if b >> 4 == 0b1110 {
        (3, (b & 0x0f) as u32)
    } else {
        (4, (b & 0x07) as u32)
    };
    let mut value = init;
    let mut i = 1;
    while i < len {
        value = (value << 6) | (bytes[at + i] & 0x3f) as u32;
        i += 1;
    }
    match char::from_u32(value) {
        Some(c) => (c, len),
        None => panic!("invalid UTF-8 in glyph art"),
    }
}

const fn accent_art(accent: Accent, base_width: u8) -> (&'static [u16], u8) {
    match accent {
        Accent::Grave => (&[0b10, 0b01], 2),
        Accent::Acute => (&[0b01, 0b10], 2),
        Accent::Circumflex => (&[0b010, 0b101], 3),
        Accent::Tilde => (&[0b0101, 0b1010], 4),
        Accent::Diaeresis if base_width == 4 => (&[0b1001], 4),
        Accent::Diaeresis => (&[0b101], 3),
        Accent::Ring => (&[0b010, 0b101, 0b010], 3),
        Accent::Cedilla => (&[0b01, 0b10], 2),
    }
}

const fn compose(base: Glyph, ch: char, accent: Accent) -> Glyph {
    let (art, accent_width) = accent_art(accent, base.width);
    let width = if accent_width > base.width {
        accent_width
    } else {
        base.width
    };
    let base_shift = (width - base.width) / 2;
    // Acute marks lean right of center, grave marks left.
    let accent_shift = match accent {
        Accent::Acute => (width - accent_width).div_ceil(2),
        _ => (width - accent_width) / 2,
    };
    let mut rows = [0_u16; ROWS];
    let mut top = ROWS;
    let mut bottom = 0;
    let mut r = 0;
    while r < ROWS {
        // Shift the base so bit positions are counted from the new left edge.
        rows[r] = base.rows[r] << (width - base.width - base_shift);
        if base.rows[r] != 0 {
            if r < top {
                top = r;
            }
            bottom = r;
        }
        r += 1;
    }
    let first = match accent {
        Accent::Cedilla => bottom + 1,
        Accent::Ring => top - art.len(),
        // One clear row separates ordinary accents from the letter.
        _ => top - 1 - art.len(),
    };
    let mut i = 0;
    while i < art.len() {
        rows[first + i] |= art[i] << (width - accent_width - accent_shift);
        i += 1;
    }
    Glyph { ch, width, rows }
}

const fn parse() -> Table {
    let mut glyphs = [Glyph::EMPTY; CAPACITY];
    let mut len = 0;
    glyphs[len] = Glyph {
        ch: ' ',
        width: SPACE_WIDTH,
        rows: [0; ROWS],
    };
    len += 1;

    let bytes = ART.as_bytes();
    let mut at = 0;
    let mut current: Option<usize> = None;
    let mut row = 0;
    while at < bytes.len() {
        // Find the end of this line.
        let mut end = at;
        let mut has_space = false;
        while end < bytes.len() && bytes[end] != b'\n' {
            if bytes[end] == b' ' {
                has_space = true;
            }
            end += 1;
        }
        if end > at {
            if has_space {
                // Header: `<char> <top>`.
                let (ch, width) = decode(bytes, at);
                let mut digit = at + width;
                assert!(bytes[digit] == b' ', "glyph header needs a space");
                digit += 1;
                let mut top = 0;
                while digit < end {
                    top = top * 10 + (bytes[digit] - b'0') as usize;
                    digit += 1;
                }
                assert!(
                    find(&glyphs, len, ch).is_none(),
                    "duplicate glyph in modern font art"
                );
                glyphs[len] = Glyph {
                    ch,
                    width: 0,
                    rows: [0; ROWS],
                };
                current = Some(len);
                len += 1;
                row = top;
            } else {
                let Some(index) = current else {
                    panic!("glyph rows before a header");
                };
                let width = (end - at) as u8;
                let glyph = &mut glyphs[index];
                if glyph.width == 0 {
                    glyph.width = width;
                }
                assert!(glyph.width == width, "glyph rows must share one width");
                assert!(row < ROWS, "glyph art exceeds the cell height");
                let mut bits = 0_u16;
                let mut i = at;
                while i < end {
                    bits <<= 1;
                    match bytes[i] {
                        b'#' => bits |= 1,
                        b'.' => {}
                        _ => panic!("glyph art uses only '#' and '.'"),
                    }
                    i += 1;
                }
                glyph.rows[row] = bits;
                row += 1;
            }
        }
        at = end + 1;
    }

    let mut i = 0;
    while i < COMPOSED.len() {
        let (ch, base, accent) = COMPOSED[i];
        let Some(base) = find(&glyphs, len, base) else {
            panic!("accent base glyph is missing");
        };
        glyphs[len] = compose(glyphs[base], ch, accent);
        len += 1;
        i += 1;
    }
    let mut i = 0;
    while i < ALIASES.len() {
        let (ch, target) = ALIASES[i];
        let Some(target) = find(&glyphs, len, target) else {
            panic!("alias target glyph is missing");
        };
        glyphs[len] = Glyph {
            ch,
            ..glyphs[target]
        };
        len += 1;
        i += 1;
    }

    let mut ascii = [u8::MAX; 128];
    let mut i = 0;
    while i < len {
        let c = glyphs[i].ch as u32;
        if c < 128 {
            ascii[c as usize] = i as u8;
        }
        i += 1;
    }
    // Every printable ASCII character must be present.
    let mut c = 32;
    while c < 127 {
        assert!(ascii[c] != u8::MAX, "modern face misses printable ASCII");
        c += 1;
    }
    Table { glyphs, len, ascii }
}

pub(crate) static TABLE: Table = parse();

impl Table {
    /// Glyph for `c`, or `None` when the face has no art for it.
    #[inline]
    pub(crate) fn get(&self, c: char) -> Option<&Glyph> {
        let code = c as u32;
        if code < 128 {
            let index = self.ascii[code as usize];
            return (index != u8::MAX).then(|| &self.glyphs[index as usize]);
        }
        self.glyphs[..self.len].iter().find(|g| g.ch == c)
    }

    /// Every supported character in authoring order.
    pub(crate) fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.glyphs[..self.len].iter().map(|g| g.ch)
    }
}

/// Kerning adjustment in font pixels for the ordered pair (`left`, `right`).
#[inline]
pub(crate) fn kerning(left: char, right: char) -> i8 {
    let tight_right = matches!(right, '.' | ',' | '…');
    match left {
        'T' => {
            if matches!(
                right,
                'a' | 'c' | 'e' | 'g' | 'm' | 'n' | 'o' | 'p' | 'q' | 'r' | 's'
            ) || matches!(right, 'u' | 'v' | 'w' | 'x' | 'y' | 'z' | '.' | ',')
            {
                -1
            } else {
                0
            }
        }
        'L' => -(matches!(right, 'T' | 'V' | 'Y' | '\'' | '"' | '’' | '”') as i8),
        'F' | 'P' | 'r' => -(tight_right as i8),
        'V' | 'W' | 'Y' => -((tight_right || matches!(right, 'a' | 'c' | 'e' | 'o')) as i8),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_fits_its_declared_width_and_digits_are_tabular() {
        for g in &TABLE.glyphs[..TABLE.len] {
            assert!(g.width <= 8, "{:?} is too wide", g.ch);
            for row in g.rows {
                assert_eq!(row >> g.width, 0, "{:?} has ink outside its width", g.ch);
            }
        }
        for d in '0'..='9' {
            assert_eq!(TABLE.get(d).unwrap().width, 5, "digit {d} is not tabular");
        }
    }

    #[test]
    fn letters_sit_on_the_baseline_and_descenders_go_below() {
        let bottom = |c| {
            let g = TABLE.get(c).unwrap();
            (0..ROWS).rev().find(|&r| g.rows[r] != 0).unwrap()
        };
        let top = |c| {
            let g = TABLE.get(c).unwrap();
            (0..ROWS).find(|&r| g.rows[r] != 0).unwrap()
        };
        for c in ('A'..='Z').chain('0'..='9') {
            assert_eq!(bottom(c), ASCENT as usize - 1, "{c}");
            assert_eq!(top(c), (ASCENT - CAP_HEIGHT) as usize, "{c}");
        }
        for c in "acemnorsuvwxz".chars() {
            assert_eq!(top(c), (ASCENT - X_HEIGHT) as usize, "{c}");
            assert_eq!(bottom(c), ASCENT as usize - 1, "{c}");
        }
        for c in "gjpqy".chars() {
            assert_eq!(bottom(c), (ASCENT + DESCENT) as usize - 1, "{c}");
        }
    }

    #[test]
    fn composed_accents_keep_a_gap_and_stay_in_the_cell() {
        let e = TABLE.get('É').unwrap();
        assert_eq!(e.rows[3], TABLE.get('E').unwrap().rows[3]);
        assert_eq!(e.rows[2], 0, "one clear row above the capital");
        assert_ne!(e.rows[0] | e.rows[1], 0);
        let i = TABLE.get('ï').unwrap();
        assert_eq!(i.width, 3);
        assert_eq!(i.rows[3], 0b101);
        assert_eq!(i.rows[6], 0b010);
        assert!(TABLE.get('ç').unwrap().rows[11] != 0);
        assert_eq!(TABLE.get('✕').unwrap().rows, TABLE.get('×').unwrap().rows);
        assert!(TABLE.chars().count() > 180);
    }
}
