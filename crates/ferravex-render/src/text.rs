//! Styled bitmap text: a modern mixed-case face, drop shadows, outlines,
//! letter spacing, alignment, measurement and allocation-free word wrapping.
//!
//! [`FontFace::Modern`] is a hand-authored proportional pixel face with
//! lowercase, descenders, punctuation, tabular digits, UI symbols
//! (`• · × → ← ↑ ↓ ★ ♥ …`) and Latin-1 accents. [`FontFace::Classic`] is the
//! original 5×7 uppercase face, so existing art direction can opt into shadows
//! and outlines without changing letterforms. The legacy
//! [`crate::draw_text_ex`] and `ui` text APIs are unchanged.
//!
//! Sizes follow the legacy convention: one font pixel is `size / 8` logical
//! units, so `size: 16.0` draws two-unit pixels and both faces keep integer
//! crispness at multiples of 8. At the same size, the modern capital height
//! (8 font pixels) is one pixel taller than the classic one (7).
//!
//! Geometry is one quad per horizontal run of lit pixels per row, merged across
//! neighbouring glyphs, written directly into the frame's vertex stream with no
//! allocation. [`measure_text_styled`] uses exactly the advances that
//! [`draw_text_styled`] draws.
//!
//! ```
//! use ferravex_render::{Color, text::{TextAlign, TextStyle, draw_text_styled, measure_text_styled}};
//! let title = TextStyle::modern(24., Color::from_hex(0xfff4d6))
//!     .with_shadow(Color::new(0., 0., 0., 0.6))
//!     .with_align(TextAlign::Center);
//! let size = measure_text_styled("Level complete!", title);
//! draw_text_styled("Level complete!", 640., 120., title);
//! assert_eq!(size.line_count, 1);
//! ```
use crate::{Color, font, font_modern, shapes::Sink};

/// Bitmap typeface for [`TextStyle`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontFace {
    /// The original 5×7 uppercase-only face (lowercase draws as uppercase).
    Classic,
    /// Mixed-case proportional face with descenders and accents.
    #[default]
    Modern,
}

/// Horizontal anchoring of each line relative to the `x` passed to drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    /// `x` is the left edge.
    #[default]
    Left,
    /// `x` is the center.
    Center,
    /// `x` is the right edge.
    Right,
}

/// Font metrics of a face in font pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaceMetrics {
    /// Rows from the top of a line to the baseline.
    pub ascent: u8,
    /// Rows below the baseline used by descenders.
    pub descent: u8,
    /// Baseline-to-baseline distance.
    pub line_height: u8,
    pub cap_height: u8,
    pub x_height: u8,
}

impl FontFace {
    /// Integer metrics in font pixels.
    ///
    /// ```
    /// use ferravex_render::text::FontFace;
    /// let m = FontFace::Modern.metrics();
    /// assert_eq!((m.cap_height, m.x_height, m.descent, m.line_height), (8, 6, 2, 14));
    /// assert_eq!(FontFace::Classic.metrics().line_height, 8);
    /// ```
    pub const fn metrics(self) -> FaceMetrics {
        match self {
            Self::Classic => FaceMetrics {
                ascent: 7,
                descent: 0,
                line_height: 8,
                cap_height: 7,
                x_height: 7,
            },
            Self::Modern => FaceMetrics {
                ascent: font_modern::ASCENT,
                descent: font_modern::DESCENT,
                line_height: font_modern::LINE_HEIGHT,
                cap_height: font_modern::CAP_HEIGHT,
                x_height: font_modern::X_HEIGHT,
            },
        }
    }

    /// Whether the face has art for `c` (space included). Unsupported
    /// characters draw as `?` and are counted in [`TextMetrics::unsupported`].
    ///
    /// ```
    /// use ferravex_render::text::FontFace;
    /// assert!(FontFace::Modern.supports('é') && FontFace::Modern.supports('★'));
    /// assert!(!FontFace::Classic.supports('é'));
    /// assert!(FontFace::Classic.supports('a')); // drawn as 'A'
    /// ```
    pub fn supports(self, c: char) -> bool {
        match self {
            Self::Classic => c == ' ' || font::glyph(c.to_ascii_uppercase()) != [0; 7],
            Self::Modern => font_modern::TABLE.get(c).is_some(),
        }
    }

    /// Every character the face draws, in authoring order.
    ///
    /// ```
    /// use ferravex_render::text::FontFace;
    /// assert!(FontFace::Modern.characters().count() > 180);
    /// ```
    pub fn characters(self) -> impl Iterator<Item = char> {
        let classic = (' '..='~')
            .chain("°×—·→←↑↓".chars())
            .filter(|&c| !c.is_ascii_lowercase() && Self::Classic.supports(c));
        let modern = font_modern::TABLE.chars();
        let (a, b) = match self {
            Self::Classic => (Some(classic), None),
            Self::Modern => (None, Some(modern)),
        };
        a.into_iter().flatten().chain(b.into_iter().flatten())
    }
}

/// Appearance of styled text. Construct with [`TextStyle::modern`] or
/// [`TextStyle::classic`] and refine with the `with_*` methods.
///
/// ```
/// use ferravex_render::{Color, text::{FontFace, TextAlign, TextStyle}};
/// let style = TextStyle::modern(16., Color::new(1., 1., 1., 1.))
///     .with_outline(Color::new(0., 0., 0., 1.))
///     .with_letter_spacing(1.)
///     .with_align(TextAlign::Right);
/// assert_eq!(style.face, FontFace::Modern);
/// assert_eq!(style.pixel(), 2.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub face: FontFace,
    /// One font pixel is `size / 8` logical units.
    pub size: f32,
    pub color: Color,
    /// One-font-pixel drop shadow (down and right), drawn under the outline.
    pub shadow: Option<Color>,
    /// One-font-pixel outline around every glyph. Outlines of adjacent glyphs
    /// merge, so translucent outlines do not double-blend.
    pub outline: Option<Color>,
    /// Extra space between glyphs in font pixels (may be fractional or negative).
    pub letter_spacing: f32,
    /// Extra space between lines in font pixels.
    pub line_spacing: f32,
    pub align: TextAlign,
    /// Rounds each line's origin to the device pixel grid (default `true`) so
    /// integer-scaled glyphs stay crisp at fractional positions.
    pub pixel_snap: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::modern(16., crate::WHITE)
    }
}

impl TextStyle {
    /// Modern face at `size` (one font pixel = `size / 8`).
    pub const fn modern(size: f32, color: Color) -> Self {
        Self {
            face: FontFace::Modern,
            size,
            color,
            shadow: None,
            outline: None,
            letter_spacing: 0.,
            line_spacing: 0.,
            align: TextAlign::Left,
            pixel_snap: true,
        }
    }
    /// Classic 5×7 face at `size`.
    pub const fn classic(size: f32, color: Color) -> Self {
        let mut style = Self::modern(size, color);
        style.face = FontFace::Classic;
        style
    }
    pub const fn with_face(mut self, face: FontFace) -> Self {
        self.face = face;
        self
    }
    pub const fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    pub const fn with_shadow(mut self, color: Color) -> Self {
        self.shadow = Some(color);
        self
    }
    pub const fn with_outline(mut self, color: Color) -> Self {
        self.outline = Some(color);
        self
    }
    pub const fn with_letter_spacing(mut self, pixels: f32) -> Self {
        self.letter_spacing = pixels;
        self
    }
    pub const fn with_line_spacing(mut self, pixels: f32) -> Self {
        self.line_spacing = pixels;
        self
    }
    pub const fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }
    pub const fn with_pixel_snap(mut self, snap: bool) -> Self {
        self.pixel_snap = snap;
        self
    }
    /// Size of one font pixel in logical units.
    pub fn pixel(&self) -> f32 {
        self.size / 8.
    }
    /// Baseline-to-baseline distance in logical units.
    pub fn line_height(&self) -> f32 {
        (self.face.metrics().line_height as f32 + self.line_spacing) * self.pixel()
    }
    /// Baseline that optically centers capitals and digits on `center_y`.
    ///
    /// ```
    /// use ferravex_render::{Color, text::TextStyle};
    /// let style = TextStyle::modern(16., Color::new(1., 1., 1., 1.));
    /// assert_eq!(style.centered_baseline(100.), 108.); // 8 px caps of 2 units
    /// ```
    pub fn centered_baseline(&self, center_y: f32) -> f32 {
        center_y + self.face.metrics().cap_height as f32 * self.pixel() * 0.5
    }
    fn is_valid(&self) -> bool {
        let c = self.color;
        self.size.is_finite()
            && self.size > 0.
            && self.letter_spacing.is_finite()
            && self.line_spacing.is_finite()
            && [c.r, c.g, c.b, c.a].iter().all(|v| v.is_finite())
    }
}

/// Measured size of styled text in logical units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    /// Widest line from the first glyph's left edge to the last glyph's right
    /// edge (no trailing spacing). Shadows and outlines add one font pixel.
    pub width: f32,
    /// From the first line's top to the last line's descender bottom.
    pub height: f32,
    /// Lines separated by `\n` (a trailing `\r` is ignored); empty text has none.
    pub line_count: usize,
    /// Baseline-to-baseline distance.
    pub line_height: f32,
    /// Top of a line to its baseline (includes room for capital accents).
    pub ascent: f32,
    /// Baseline to the bottom of descenders.
    pub descent: f32,
    pub cap_height: f32,
    pub x_height: f32,
    /// Characters drawn with the `?` fallback.
    pub unsupported: usize,
}

// ---------------------------------------------------------------------------
// Glyph access.

#[derive(Clone, Copy)]
struct Shape {
    width: u8,
    /// Lit bits of one row; bit `width - 1 - column`.
    bits: u16,
}

#[inline]
fn modern(c: char) -> (&'static font_modern::Glyph, bool) {
    match font_modern::TABLE.get(c) {
        Some(g) => (g, true),
        None => (
            font_modern::TABLE.get('?').expect("modern '?' glyph"),
            false,
        ),
    }
}

#[inline]
fn classic(c: char) -> ([u8; 7], bool) {
    let rows = font::glyph(c.to_ascii_uppercase());
    if c == ' ' || rows != [0; 7] {
        (rows, true)
    } else {
        (font::glyph('?'), false)
    }
}

#[inline]
fn width_of(face: FontFace, c: char) -> (u8, bool) {
    match face {
        FontFace::Classic => (5, classic(c).1),
        FontFace::Modern => {
            let (g, ok) = modern(c);
            (g.width, ok)
        }
    }
}

/// Row `row` of `c`, where rows may lie outside the cell (they are clear).
#[inline]
fn row_of(face: FontFace, c: char, row: i32) -> Shape {
    match face {
        FontFace::Classic => {
            let rows = classic(c).0;
            let bits = usize::try_from(row)
                .ok()
                .and_then(|r| rows.get(r))
                .map_or(0, |&b| b as u16);
            Shape { width: 5, bits }
        }
        FontFace::Modern => {
            let g = modern(c).0;
            let bits = usize::try_from(row)
                .ok()
                .and_then(|r| g.rows.get(r))
                .map_or(0, |&b| b);
            Shape {
                width: g.width,
                bits,
            }
        }
    }
}

fn kern(face: FontFace, left: char, right: char) -> i8 {
    match face {
        FontFace::Classic => 0,
        FontFace::Modern => font_modern::kerning(left, right),
    }
}

fn rows(face: FontFace) -> i32 {
    match face {
        FontFace::Classic => 7,
        FontFace::Modern => font_modern::ROWS as i32,
    }
}

/// Walks a line's glyph origins in font pixels: `(char, x, width)`.
fn layout(line: &str, face: FontFace, spacing: f32) -> impl Iterator<Item = (char, f32, u8)> + '_ {
    let mut x = 0.;
    let mut prev: Option<char> = None;
    line.chars().map(move |c| {
        if let Some(p) = prev {
            x += 1. + spacing + kern(face, p, c) as f32;
        }
        let width = width_of(face, c).0;
        let origin = x;
        x += width as f32;
        prev = Some(c);
        (c, origin, width)
    })
}

/// Ink advance of one line in font pixels and its unsupported count.
fn line_units(line: &str, face: FontFace, spacing: f32) -> (f32, usize) {
    let mut end: f32 = 0.;
    let mut missing = 0;
    for (c, x, w) in layout(line, face, spacing) {
        end = x + w as f32;
        missing += usize::from(!width_of(face, c).1);
    }
    (end.max(0.), missing)
}

fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// Measures styled text exactly as [`draw_text_styled`] lays it out.
/// Invalid styles (non-finite or nonpositive size) measure as empty.
///
/// ```
/// use ferravex_render::{Color, text::{TextStyle, measure_text_styled}};
/// let style = TextStyle::modern(8., Color::new(1., 1., 1., 1.));
/// let m = measure_text_styled("Hi", style);
/// assert_eq!(m.width, 7.); // H (5) + spacing (1) + i (1)
/// assert_eq!((m.line_height, m.cap_height, m.ascent, m.descent), (14., 8., 11., 2.));
/// let two = measure_text_styled("Hi\nthere", style);
/// assert_eq!((two.line_count, two.height), (2, 27.));
/// ```
pub fn measure_text_styled(text: &str, style: TextStyle) -> TextMetrics {
    let px = style.pixel();
    let m = style.face.metrics();
    let mut out = TextMetrics {
        line_height: style.line_height(),
        ascent: m.ascent as f32 * px,
        descent: m.descent as f32 * px,
        cap_height: m.cap_height as f32 * px,
        x_height: m.x_height as f32 * px,
        ..Default::default()
    };
    if !style.is_valid() || text.is_empty() {
        return out;
    }
    let mut widest: f32 = 0.;
    for line in lines(text) {
        let (units, missing) = line_units(line, style.face, style.letter_spacing);
        widest = widest.max(units);
        out.unsupported += missing;
        out.line_count += 1;
    }
    out.width = widest * px;
    out.height = (out.line_count - 1) as f32 * out.line_height + out.ascent + out.descent;
    out
}

/// Width in logical units of a single line (no `\n` handling).
///
/// ```
/// use ferravex_render::{Color, text::{TextStyle, text_width}};
/// let s = TextStyle::modern(16., Color::new(1., 1., 1., 1.));
/// assert_eq!(text_width("10", s), text_width("88", s)); // tabular digits
/// ```
pub fn text_width(line: &str, style: TextStyle) -> f32 {
    if !style.is_valid() {
        return 0.;
    }
    line_units(line, style.face, style.letter_spacing).0 * style.pixel()
}

// ---------------------------------------------------------------------------
// Drawing.

#[derive(Clone, Copy)]
struct Layer {
    color: Color,
    /// Offset in font pixels.
    dx: f32,
    dy: f32,
    dilate: bool,
}

/// Dilated row bits (one pixel in all eight directions), one column wider on
/// both sides: bit `width + 1 - column'` where `column' = column + 1`.
#[inline]
fn dilated(face: FontFace, c: char, row: i32) -> Shape {
    let mut acc = 0_u16;
    let mut width = 0;
    for r in row - 1..=row + 1 {
        let s = row_of(face, c, r);
        width = s.width;
        let e = s.bits << 1;
        acc |= e | (e << 1) | (e >> 1);
    }
    Shape {
        width: width + 2,
        bits: acc,
    }
}

/// Mask of columns `start..end` in a row of `width` columns.
#[inline]
fn span(start: i32, end: i32, width: i32) -> u16 {
    ((((1_u32 << (end - start)) - 1) << (width - end)) & 0xffff) as u16
}

/// Whether `start..end` is a maximal lit run of `bits`.
#[inline]
fn is_run(bits: u16, start: i32, end: i32, width: i32) -> bool {
    let mask = span(start, end, width);
    let left = start == 0 || bits & span(start - 1, start, width) == 0;
    let right = end == width || bits & span(end, end + 1, width) == 0;
    bits & mask == mask && left && right
}

/// Maximal lit runs `(start, end)` of one row, left to right.
fn runs(bits: u16, width: i32) -> impl Iterator<Item = (i32, i32)> {
    let lit = move |col: i32| col < width && bits >> (width - 1 - col) & 1 != 0;
    let mut col = 0;
    std::iter::from_fn(move || {
        while col < width && !lit(col) {
            col += 1;
        }
        if col >= width {
            return None;
        }
        let start = col;
        while lit(col) {
            col += 1;
        }
        Some((start, col))
    })
}

/// Glyph-by-glyph rectangles: each maximal horizontal run is extended down
/// over identical runs in the following rows, so stems become one quad.
fn draw_line_stacked(
    sink: &mut Sink,
    line: &str,
    left: f32,
    baseline: f32,
    px: f32,
    style: &TextStyle,
    layer: Layer,
) {
    let face = style.face;
    let ascent = face.metrics().ascent as f32;
    let count = rows(face);
    for (c, origin, _) in layout(line, face, style.letter_spacing) {
        let mut bits = [0_u16; font_modern::ROWS];
        let mut width = 0;
        for (r, slot) in bits.iter_mut().enumerate().take(count as usize) {
            let shape = row_of(face, c, r as i32);
            *slot = shape.bits;
            width = shape.width as i32;
        }
        for r in 0..count as usize {
            for (start, end) in runs(bits[r], width) {
                if r > 0 && is_run(bits[r - 1], start, end, width) {
                    continue; // Already covered by the rectangle above.
                }
                let mut height = 1;
                while r + height < count as usize && is_run(bits[r + height], start, end, width) {
                    height += 1;
                }
                let x = left + (origin + start as f32 + layer.dx) * px;
                let y = baseline + (r as f32 - ascent + layer.dy) * px;
                sink.rect(
                    x,
                    y,
                    (end - start) as f32 * px,
                    height as f32 * px,
                    layer.color,
                );
            }
        }
    }
}

/// Row-by-row runs merged across neighbouring glyphs (used for outlines so a
/// translucent outline never overlaps itself).
fn draw_line_merged(
    sink: &mut Sink,
    line: &str,
    left: f32,
    baseline: f32,
    px: f32,
    style: &TextStyle,
    layer: Layer,
) {
    let face = style.face;
    let ascent = face.metrics().ascent as f32;
    let (first, last) = if layer.dilate {
        (-1, rows(face))
    } else {
        (0, rows(face) - 1)
    };
    for row in first..=last {
        let top = baseline + (row as f32 - ascent + layer.dy) * px;
        // Current merged run in font pixels.
        let mut run: Option<(f32, f32)> = None;
        let flush = |run: (f32, f32), sink: &mut Sink| {
            let x = left + (run.0 + layer.dx) * px;
            sink.rect(x, top, (run.1 - run.0) * px, px, layer.color);
        };
        for (c, origin, _) in layout(line, face, style.letter_spacing) {
            let (shape, origin) = if layer.dilate {
                (dilated(face, c, row), origin - 1.)
            } else {
                (row_of(face, c, row), origin)
            };
            for (start, end) in runs(shape.bits, shape.width as i32) {
                let s = origin + start as f32;
                let e = origin + end as f32;
                run = match run {
                    Some((a, b)) if s <= b + 1e-4 => Some((a.min(s), b.max(e))),
                    Some(done) => {
                        flush(done, sink);
                        Some((s, e))
                    }
                    None => Some((s, e)),
                };
            }
        }
        if let Some(done) = run {
            flush(done, sink);
        }
    }
}

/// Draws styled text. `y` is the first line's baseline (as with
/// [`crate::draw_text_ex`]) and `x` anchors each line per [`TextStyle::align`].
/// Lines split on `\n`. Returns the same value as [`measure_text_styled`].
///
/// Each layer emits one quad (6 vertices) per merged horizontal run per row;
/// a shadow or outline adds one more layer. Nothing is drawn for an invalid
/// style or non-finite position.
///
/// ```
/// use ferravex_render::{Color, text::{TextStyle, draw_text_styled}};
/// let hud = TextStyle::modern(16., Color::from_hex(0xe4eef2)).with_shadow(Color::new(0., 0., 0., 0.55));
/// draw_text_styled("Score 12,480 ★", 24., 40., hud);
/// ```
pub fn draw_text_styled(text: &str, x: f32, y: f32, style: TextStyle) -> TextMetrics {
    let metrics = measure_text_styled(text, style);
    if metrics.line_count == 0 || !x.is_finite() || !y.is_finite() {
        return metrics;
    }
    crate::edit(|s| {
        let mut sink = Sink::new(s);
        let dp = sink.device_pixel();
        let px = style.pixel();
        let snap = |v: f32| {
            if style.pixel_snap {
                (v / dp).round() * dp
            } else {
                v
            }
        };
        let mut layers = [None; 3];
        if let Some(color) = style.shadow {
            layers[0] = Some(Layer {
                color,
                dx: 1.,
                dy: 1.,
                dilate: style.outline.is_some(),
            });
        }
        if let Some(color) = style.outline {
            layers[1] = Some(Layer {
                color,
                dx: 0.,
                dy: 0.,
                dilate: true,
            });
        }
        layers[2] = Some(Layer {
            color: style.color,
            dx: 0.,
            dy: 0.,
            dilate: false,
        });
        // Layers are drawn back to front over the whole text so a line's shadow
        // never covers the line above it.
        for layer in layers.into_iter().flatten() {
            let mut baseline = y;
            for line in lines(text) {
                let width = line_units(line, style.face, style.letter_spacing).0 * px;
                let left = match style.align {
                    TextAlign::Left => x,
                    TextAlign::Center => x - width * 0.5,
                    TextAlign::Right => x - width,
                };
                let (left, line_y) = (snap(left), snap(baseline));
                if layer.dilate {
                    draw_line_merged(&mut sink, line, left, line_y, px, &style, layer);
                } else {
                    draw_line_stacked(&mut sink, line, left, line_y, px, &style, layer);
                }
                baseline += metrics.line_height;
            }
        }
    });
    metrics
}

/// Greedy word-wrapped lines of `text` no wider than `max_width` (logical
/// units). Explicit `\n` breaks are kept; runs of spaces at a wrap are dropped;
/// a word wider than the limit is split between characters. Borrowed slices,
/// no allocation.
///
/// ```
/// use ferravex_render::{Color, text::{TextStyle, text_width, wrap_text}};
/// let s = TextStyle::modern(8., Color::new(1., 1., 1., 1.));
/// let lines: Vec<&str> = wrap_text("Press Space to jump over the gap", 60., s).collect();
/// assert_eq!(lines, ["Press Space", "to jump over", "the gap"]);
/// assert!(lines.iter().all(|l| text_width(l, s) <= 60.));
/// ```
pub fn wrap_text(text: &str, max_width: f32, style: TextStyle) -> WrapLines<'_> {
    let limit = if style.is_valid() && max_width.is_finite() {
        (max_width / style.pixel()).max(0.)
    } else {
        f32::INFINITY
    };
    WrapLines {
        rest: (!text.is_empty()).then_some(text),
        paragraph: "",
        limit,
        face: style.face,
        spacing: style.letter_spacing,
    }
}

/// Iterator returned by [`wrap_text`].
#[derive(Clone, Debug)]
pub struct WrapLines<'a> {
    /// Unprocessed text after the current paragraph.
    rest: Option<&'a str>,
    /// Remainder of the current paragraph.
    paragraph: &'a str,
    limit: f32,
    face: FontFace,
    spacing: f32,
}

impl<'a> Iterator for WrapLines<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        if self.paragraph.is_empty() {
            let rest = self.rest?;
            let (line, tail) = match rest.find('\n') {
                Some(i) => (&rest[..i], Some(&rest[i + 1..])),
                None => (rest, None),
            };
            self.rest = tail;
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                return Some(line);
            }
            self.paragraph = line;
        }
        let text = self.paragraph;
        let mut right = 0.;
        let mut prev: Option<char> = None;
        // Last break opportunity: (end of the line, start of the next word).
        let mut word_end: Option<usize> = None;
        let mut space_break: Option<(usize, usize)> = None;
        for (i, c) in text.char_indices() {
            let advance = prev.map_or(0., |p| 1. + self.spacing + kern(self.face, p, c) as f32);
            let next_right = right + advance + width_of(self.face, c).0 as f32;
            if c == ' ' {
                if prev.is_some_and(|p| p != ' ') {
                    word_end = Some(i);
                }
            } else {
                if prev == Some(' ')
                    && let Some(end) = word_end
                {
                    space_break = Some((end, i));
                }
                if next_right > self.limit + 1e-4 && i > 0 {
                    let (line, next) = match space_break {
                        Some((end, next)) => (&text[..end], &text[next..]),
                        None => (&text[..i], &text[i..]),
                    };
                    self.paragraph = next;
                    return Some(line);
                }
            }
            right = next_right;
            prev = Some(c);
        }
        self.paragraph = "";
        Some(text.trim_end_matches(' '))
    }
}

/// Draws `text` word-wrapped to `max_width` starting at baseline `y`; `x`
/// anchors lines per the style's alignment. Returns the wrapped block's metrics.
///
/// ```
/// use ferravex_render::{Color, text::{TextStyle, draw_text_wrapped}};
/// let body = TextStyle::modern(16., Color::from_hex(0xc8d3dc));
/// let m = draw_text_wrapped("Collect every gem before the lava rises.", 40., 200., 240., body);
/// assert!(m.width <= 240. && m.line_count >= 2);
/// ```
pub fn draw_text_wrapped(
    text: &str,
    x: f32,
    y: f32,
    max_width: f32,
    style: TextStyle,
) -> TextMetrics {
    let mut total = measure_text_styled("", style);
    if !style.is_valid() {
        return total;
    }
    let mut baseline = y;
    for line in wrap_text(text, max_width, style) {
        let m = draw_text_styled(line, x, baseline, style);
        total.width = total.width.max(m.width);
        total.unsupported += m.unsupported;
        total.line_count += 1;
        baseline += total.line_height;
    }
    if total.line_count > 0 {
        total.height =
            (total.line_count - 1) as f32 * total.line_height + total.ascent + total.descent;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::tests::capture;

    const WHITE: Color = Color::new(1., 1., 1., 1.);

    /// Rasterizes captured rects (pairs of triangles) at pixel centers into a
    /// coverage grid of quad counts per pixel.
    fn coverage(vertices: &[[f32; 8]], w: usize, h: usize) -> Vec<u8> {
        let mut grid = vec![0_u8; w * h];
        for quad in vertices.chunks(6) {
            let xs = quad.iter().map(|v| (v[0] + 1.) * w as f32 * 0.5);
            let ys = quad.iter().map(|v| (1. - v[1]) * h as f32 * 0.5);
            let (x0, x1) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
            let (y0, y1) = ys.fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
            for py in 0..h {
                for px in 0..w {
                    let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                    if cx > x0 && cx < x1 && cy > y0 && cy < y1 {
                        grid[py * w + px] += 1;
                    }
                }
            }
        }
        grid
    }

    #[test]
    fn measure_equals_draw_extent_for_both_faces_and_alignments() {
        for face in [FontFace::Modern, FontFace::Classic] {
            for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
                let style = TextStyle::modern(16., WHITE)
                    .with_face(face)
                    .with_align(align);
                let text = "Hydra Wave 7 — x×y";
                let m = measure_text_styled(text, style);
                let v = capture(800, 200, || {
                    draw_text_styled(text, 400., 100., style);
                });
                let xs = v.iter().map(|v| (v[0] + 1.) * 400.);
                let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
                let expect_left = match align {
                    TextAlign::Left => 400.,
                    TextAlign::Center => 400. - m.width / 2.,
                    TextAlign::Right => 400. - m.width,
                };
                assert!(
                    (lo - expect_left.round()).abs() < 1e-3,
                    "{face:?} {align:?} {lo}"
                );
                assert!(
                    (hi - lo - m.width).abs() < 1e-3,
                    "{face:?} {align:?} {}",
                    hi - lo
                );
            }
        }
    }

    #[test]
    fn merged_runs_cover_exactly_the_lit_pixels_without_overlap() {
        let text = "Quick fox, jumpy dwarves! 0123456789 ÀÉÎõü ★♥→";
        let style = TextStyle::modern(8., WHITE);
        let v = capture(400, 40, || {
            draw_text_styled(text, 2., 20., style);
        });
        let grid = coverage(&v, 400, 40);
        assert!(grid.iter().all(|&n| n <= 1), "runs overlap");
        let lit: usize = text
            .chars()
            .map(|c| {
                modern(c)
                    .0
                    .rows
                    .iter()
                    .map(|r| r.count_ones() as usize)
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(grid.iter().filter(|&&n| n == 1).count(), lit);
        // Far fewer quads than lit pixels.
        assert!(
            v.len() / 6 < lit * 2 / 5,
            "{} quads for {lit} pixels",
            v.len() / 6
        );
    }

    const HUD: [&str; 10] = [
        "SCORE 012480",
        "BEST 98000",
        "WAVE 7/10",
        "LIVES 3",
        "COMBO X12",
        "HEALTH 87%",
        "TIME 02:15.40",
        "PRESS SPACE TO JUMP",
        "PAUSED - ESC TO RESUME",
        "GEMS 14 / 20  KEYS 2",
    ];

    #[test]
    fn legacy_draw_text_ex_is_unchanged_and_styled_text_saves_vertices() {
        use crate::{TextParams, draw_text_ex};
        let (w, h) = (420, 240);
        let v = capture(w as u32, h as u32, || {
            for (i, line) in HUD.iter().enumerate() {
                draw_text_ex(
                    line,
                    4.,
                    20. + i as f32 * 22.,
                    TextParams {
                        font_size: 16,
                        font_scale: 1.,
                        color: WHITE,
                    },
                );
            }
        });
        // Expected per-pixel coverage from the bitmaps.
        let mut expected = vec![0_u8; w * h];
        let mut lit = 0;
        for (i, line) in HUD.iter().enumerate() {
            for (k, c) in line.chars().enumerate() {
                for (row, bits) in font::glyph(c).into_iter().enumerate() {
                    for col in 0..5 {
                        if bits & (1 << (4 - col)) != 0 {
                            lit += 1;
                            let x0 = 4 + k * 12 + col * 2;
                            let y0 = (20 + i * 22) - 14 + row * 2;
                            for y in y0..y0 + 2 {
                                for x in x0..x0 + 2 {
                                    expected[y * w + x] = 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(coverage(&v, w, h), expected);
        // Legacy contract: exactly one 6-vertex quad per lit font pixel,
        // identical to per-pixel draw_rectangle calls.
        assert_eq!(v.len(), lit * 6);
        let reference = capture(w as u32, h as u32, || {
            for (i, line) in HUD.iter().enumerate() {
                let mut x = 4.;
                for c in line.chars() {
                    for (row, bits) in font::glyph(c).into_iter().enumerate() {
                        for col in 0..5 {
                            if bits & (1 << (4 - col)) != 0 {
                                let y = 20. + i as f32 * 22. - 14. + row as f32 * 2.;
                                crate::draw_rectangle(x + col as f32 * 2., y, 2., 2., WHITE);
                            }
                        }
                    }
                    x += 12.;
                }
            }
        });
        assert_eq!(v, reference);
        let per_pixel = v.len();
        let modern = capture(w as u32, h as u32, || {
            for (i, line) in HUD.iter().enumerate() {
                draw_text_styled(
                    line,
                    4.,
                    20. + i as f32 * 28.,
                    TextStyle::modern(16., WHITE),
                );
            }
        })
        .len();
        let classic_styled = capture(w as u32, h as u32, || {
            for (i, line) in HUD.iter().enumerate() {
                draw_text_styled(
                    line,
                    4.,
                    20. + i as f32 * 22.,
                    TextStyle::classic(16., WHITE),
                );
            }
        })
        .len();
        println!(
            "HUD vertices: classic draw_text_ex {per_pixel}, classic styled {classic_styled} \
             ({:.0}% fewer), modern styled {modern} ({:.0}% fewer)",
            100. * (1. - classic_styled as f32 / per_pixel as f32),
            100. * (1. - modern as f32 / per_pixel as f32),
        );
        assert!(classic_styled * 2 < per_pixel && modern * 2 < per_pixel);
    }

    #[test]
    fn outline_and_shadow_layers_merge_and_order_back_to_front() {
        let style = TextStyle::modern(8., WHITE)
            .with_outline(Color::new(0., 0., 0., 0.5))
            .with_shadow(Color::new(0., 0., 1., 0.5));
        let v = capture(100, 30, || {
            draw_text_styled("ab", 10., 15., style);
        });
        // Shadow first, outline second, body last.
        assert_eq!(v[0][6], 1.);
        assert_eq!(v.last().unwrap()[4], 1.);
        let outline: Vec<_> = v
            .iter()
            .filter(|v| v[7] == 0.5 && v[6] == 0.)
            .copied()
            .collect();
        let grid = coverage(&outline, 100, 30);
        assert!(
            grid.iter().all(|&n| n <= 1),
            "translucent outline double-blends"
        );
        let m = measure_text_styled("ab", style);
        let (lo, hi) = outline
            .iter()
            .map(|v| (v[0] + 1.) * 50.)
            .fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
        assert!(
            (lo - 9.).abs() < 1e-3 && (hi - 11. - m.width).abs() < 1e-3,
            "{lo} {hi}"
        );
    }

    #[test]
    fn wrapping_respects_width_newlines_and_long_words() {
        let s = TextStyle::modern(8., WHITE);
        let all: Vec<_> = wrap_text("a\n\nbb  cc\r\nsupercalifragilistic", 20., s).collect();
        assert_eq!(all[..4], ["a", "", "bb", "cc"]);
        assert!(
            all[4..]
                .iter()
                .all(|l| text_width(l, s) <= 20. && !l.is_empty())
        );
        assert_eq!(all[4..].concat(), "supercalifragilistic");
        assert_eq!(wrap_text("", 10., s).count(), 0);
        assert_eq!(wrap_text("x", 0., s).collect::<Vec<_>>(), ["x"]);
        let m = draw_text_wrapped("one two three four", 0., 20., 40., s);
        assert_eq!(
            m.line_count,
            wrap_text("one two three four", 40., s).count()
        );
    }

    #[test]
    fn tabular_digits_kerning_and_invalid_styles() {
        let s = TextStyle::modern(8., WHITE);
        let w = text_width("0000", s);
        for n in ["1111", "1234", "9876", "7171"] {
            assert_eq!(text_width(n, s), w);
        }
        assert_eq!(text_width("To", s), text_width("T", s) + text_width("o", s));
        assert_eq!(
            text_width("Ta", s) + 1.,
            text_width("T", s) + 1. + text_width("a", s)
        );
        assert_eq!(measure_text_styled("☃", s).unsupported, 1);
        let bad = TextStyle::modern(f32::NAN, WHITE);
        assert_eq!(measure_text_styled("abc", bad).line_count, 0);
        assert!(
            capture(64, 64, || {
                draw_text_styled("abc", 0., 10., bad);
                draw_text_styled("abc", f32::INFINITY, 10., s);
            })
            .is_empty()
        );
    }
}
