//! Allocation-free measurements for the immediate renderer's 5×7 bitmap font.
//!
//! Obtain checked metrics with [`crate::Font::try_metrics`]. Measurements use
//! local, Y-down coordinates before camera transforms, clipping or UI shadows.
//! ASCII lowercase letters use uppercase glyphs; other unsupported characters
//! are blank but still advance, exactly as in [`crate::draw_text_ex`]. The
//! separate UI renderer's `?` substitution and word wrapping are not applied.

use crate::Rect;

/// A font size, scale or resulting coordinate cannot be measured safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontMeasureError {
    /// Font size must be nonzero.
    ZeroSize,
    /// Scale must be finite and strictly positive.
    InvalidScale,
    /// Scaling or layout overflowed, underflowed to zero, or lost enough `f32`
    /// precision that an advance or an outer visible pixel edge no longer moves.
    Unrepresentable,
}

impl std::fmt::Display for FontMeasureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ZeroSize => "bitmap font size must be nonzero",
            Self::InvalidScale => "bitmap font scale must be finite and positive",
            Self::Unrepresentable => "bitmap text geometry is not representable in f32",
        })
    }
}

impl std::error::Error for FontMeasureError {}

/// Validated, reusable font geometry. No atlas, text buffer or allocation is needed.
///
/// ```
/// let font = ferravex_render::get_default_font();
/// let metrics = font.try_metrics(16, 0.5)?;
/// assert_eq!(metrics.pixel_size(), 1.0);
/// assert_eq!(metrics.glyph_advance(), 6.0);
/// assert_eq!(metrics.baseline(), 7.0);
/// assert_eq!(metrics.line_height(), 8.0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BitmapFontMetrics {
    pixel_size: f32,
}

/// Logical size and tight visible bounds of a measured label.
///
/// ```
/// let metrics = ferravex_render::get_default_font().try_metrics(8, 1.0)?;
/// let text = metrics.measure_line("A ")?;
/// assert_eq!((text.width, text.height, text.line_count), (12.0, 7.0, 1));
/// assert_eq!(text.ink_bounds, Some(ferravex_render::Rect::new(0.0, 0.0, 5.0, 7.0)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BitmapTextMetrics {
    /// Maximum line advance, including the final one-pixel gap and blank cells.
    /// Advances accumulate in the same order as `draw_text_ex` at local X = 0.
    pub width: f32,
    /// Distance from the first line's top to the last line's baseline.
    /// Empty input has zero height; explicit blank lines reserve their height.
    pub height: f32,
    /// Number of logical lines, including trailing blank lines. Empty input is zero.
    pub line_count: usize,
    /// Number of unsupported Unicode scalar values, excluding ordinary spaces
    /// and the line separators recognized by `measure_multiline`.
    pub unsupported_glyphs: usize,
    /// Tight union of the glyph rectangles relative to the first line's top-left.
    /// `None` means no visible glyph pixels, even when logical size is nonzero.
    pub ink_bounds: Option<Rect>,
}

impl crate::Font {
    /// Validates the size and scale used by [`crate::draw_text_ex`].
    ///
    /// Pixel size is `size as f32 * scale / 8.0`, preserving the renderer's
    /// operation order. Zero size, nonpositive/nonfinite scale and unrepresentable
    /// geometry return an error. Text extent is checked separately on measurement.
    ///
    /// ```
    /// let font = ferravex_render::get_default_font();
    /// assert!(font.try_metrics(0, 1.0).is_err());
    /// assert!(font.try_metrics(16, f32::NAN).is_err());
    /// let metrics = font.try_metrics(16, 1.5)?;
    /// assert_eq!(metrics.measure_line("Score")?.width, 90.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn try_metrics(
        &self,
        size: u16,
        scale: f32,
    ) -> Result<BitmapFontMetrics, FontMeasureError> {
        if size == 0 {
            return Err(FontMeasureError::ZeroSize);
        }
        if !scale.is_finite() || scale <= 0.0 {
            return Err(FontMeasureError::InvalidScale);
        }
        let pixel_size = size as f32 * scale / 8.0;
        if !pixel_size.is_finite() || pixel_size <= 0.0 || !(8.0 * pixel_size).is_finite() {
            return Err(FontMeasureError::Unrepresentable);
        }
        Ok(BitmapFontMetrics { pixel_size })
    }
}

impl BitmapFontMetrics {
    /// Side length of one bitmap pixel in logical drawing coordinates.
    pub fn pixel_size(self) -> f32 {
        self.pixel_size
    }

    /// Horizontal advance per Unicode scalar: five columns plus one spacing column.
    pub fn glyph_advance(self) -> f32 {
        6.0 * self.pixel_size
    }

    /// Baseline offset from a line's top: seven pixel rows, with no descenders.
    /// Pass `top + baseline()` as the Y argument to `draw_text_ex`.
    pub fn baseline(self) -> f32 {
        7.0 * self.pixel_size
    }

    /// Baseline-to-baseline distance: seven pixel rows plus one spacing row.
    pub fn line_height(self) -> f32 {
        8.0 * self.pixel_size
    }

    /// Measures one immediate-renderer line without allocation.
    ///
    /// Every scalar advances, including unsupported controls such as LF, CR and
    /// tab. There is no Unicode case expansion, shaping or fallback substitution.
    /// Empty input has no lines, size or ink; spaces reserve size but have no ink.
    /// Returns an error if the resulting geometry is unrepresentable.
    ///
    /// ```
    /// let metrics = ferravex_render::get_default_font().try_metrics(8, 1.0)?;
    /// let text = metrics.measure_line("a☃ ")?;
    /// assert_eq!(text.width, 18.0);
    /// assert_eq!(text.unsupported_glyphs, 1);
    /// assert_eq!(text.ink_bounds.unwrap().w, 5.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn measure_line(self, text: &str) -> Result<BitmapTextMetrics, FontMeasureError> {
        self.measure(text, false)
    }

    /// Measures explicit LF/CRLF-separated lines without allocation or wrapping.
    ///
    /// Lines share a left edge and advance by `line_height()`. Blank and trailing
    /// lines reserve vertical space; bare CR remains an unsupported blank cell.
    /// `draw_text_ex` itself does **not** split lines: draw each line separately
    /// with successive baselines, removing CR from CRLF separators first.
    /// Returns an error if the resulting geometry is unrepresentable.
    ///
    /// ```
    /// use ferravex_render::{TextParams, WHITE, draw_text_ex, get_default_font};
    /// let metrics = get_default_font().try_metrics(16, 1.0)?;
    /// let label = "LEVEL 1\nREADY";
    /// let measured = metrics.measure_multiline(label)?;
    /// assert_eq!((measured.width, measured.height), (84.0, 30.0));
    /// let mut baseline = 20.0 + metrics.baseline();
    /// for line in label.split('\n') {
    ///     draw_text_ex(line, 10.0, baseline, TextParams {
    ///         font_size: 16, font_scale: 1.0, color: WHITE,
    ///     });
    ///     baseline += metrics.line_height();
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn measure_multiline(self, text: &str) -> Result<BitmapTextMetrics, FontMeasureError> {
        self.measure(text, true)
    }

    fn measure(self, text: &str, multiline: bool) -> Result<BitmapTextMetrics, FontMeasureError> {
        let mut result = BitmapTextMetrics::default();
        if text.is_empty() {
            return Ok(result);
        }
        let mut x = 0.0;
        let mut baseline = self.baseline();
        result.line_count = 1;
        // Keep extrema until the end to avoid repeated min + width rounding.
        let mut ink: Option<[f32; 4]> = None;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            let crlf = multiline && c == '\r' && chars.peek() == Some(&'\n');
            if multiline && (c == '\n' || crlf) {
                if crlf {
                    chars.next();
                }
                result.width = result.width.max(x);
                x = 0.0;
                baseline = checked_advance(baseline, self.line_height())?;
                result.line_count += 1;
                continue;
            }

            let rows = glyph(c.to_ascii_uppercase());
            if c != ' ' && rows == [0; 7] {
                result.unsupported_glyphs += 1;
            }
            for (row, bits) in rows.into_iter().enumerate() {
                if bits == 0 {
                    continue;
                }
                // Row extrema use the same rectangle arithmetic as draw_text_ex.
                let left = x + (bits.leading_zeros() - 3) as f32 * self.pixel_size;
                let last = x + (4 - bits.trailing_zeros()) as f32 * self.pixel_size;
                let top = baseline - self.baseline() + row as f32 * self.pixel_size;
                let right = checked_advance(last, self.pixel_size)?;
                let bottom = checked_advance(top, self.pixel_size)?;
                ink = Some(match ink {
                    None => [left, top, right, bottom],
                    Some([l, t, r, b]) => [l.min(left), t.min(top), r.max(right), b.max(bottom)],
                });
            }
            x = checked_advance(x, self.glyph_advance())?;
        }
        result.width = result.width.max(x);
        result.height = baseline;
        result.ink_bounds =
            ink.map(|[left, top, right, bottom]| Rect::new(left, top, right - left, bottom - top));
        Ok(result)
    }
}

fn checked_advance(position: f32, distance: f32) -> Result<f32, FontMeasureError> {
    let next = position + distance;
    if !next.is_finite() || next <= position {
        Err(FontMeasureError::Unrepresentable)
    } else {
        Ok(next)
    }
}

// Original Ferravex 5×7 glyph geometry; shared with Orbit Breaker.
pub fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '$' => [4, 15, 20, 14, 5, 30, 4],
        ',' => [0, 0, 0, 12, 12, 4, 8],
        ';' => [12, 12, 0, 12, 12, 4, 8],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '(' => [2, 4, 8, 8, 8, 4, 2],
        ')' => [8, 4, 2, 2, 2, 4, 8],
        '%' => [17, 2, 4, 4, 8, 16, 17],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        '?' => [14, 17, 1, 2, 4, 0, 4],
        '°' => [6, 9, 9, 6, 0, 0, 0],
        '×' => [0, 17, 10, 4, 10, 17, 0],
        '—' => [0, 0, 0, 31, 0, 0, 0],
        '·' => [0, 0, 0, 4, 0, 0, 0],
        '→' => [0, 4, 2, 31, 2, 4, 0],
        '←' => [0, 4, 8, 31, 8, 4, 0],
        '↑' => [4, 14, 21, 4, 4, 4, 0],
        '↓' => [4, 4, 4, 4, 21, 14, 4],
        '/' => [1, 1, 2, 4, 8, 16, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '=' => [0, 0, 31, 0, 31, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '\'' => [4, 4, 2, 0, 0, 0, 0],
        '\"' => [10, 10, 5, 0, 0, 0, 0],
        '&' => [12, 18, 20, 8, 21, 18, 13],
        '*' => [0, 21, 14, 31, 14, 21, 0],
        '<' => [2, 4, 8, 16, 8, 4, 2],
        '>' => [8, 4, 2, 1, 2, 4, 8],
        '#' => [10, 31, 10, 10, 31, 10, 0],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Font, Mat4, State, TextParams, WHITE, draw_text_ex};

    fn drawn_bounds<'a>(
        lines: impl IntoIterator<Item = &'a str>,
        size: u16,
        scale: f32,
    ) -> Option<Rect> {
        crate::edit(|s| {
            *s = State::default();
            // Capture the actual submitted drawing geometry without a camera transform.
            s.matrix = Mat4::IDENTITY;
        });
        let px = size as f32 * scale / 8.0;
        let mut y = 7.0 * px;
        for line in lines {
            draw_text_ex(
                line,
                0.0,
                y,
                TextParams {
                    font_size: size,
                    font_scale: scale,
                    color: WHITE,
                },
            );
            y += 8.0 * px;
        }
        crate::read(|s| {
            let first = s.vertices.first()?;
            let [mut left, mut top, mut right, mut bottom] =
                [first[0], first[1], first[0], first[1]];
            for v in &s.vertices {
                left = left.min(v[0]);
                top = top.min(v[1]);
                right = right.max(v[0]);
                bottom = bottom.max(v[1]);
            }
            Some(Rect::new(left, top, right - left, bottom - top))
        })
    }

    #[test]
    fn checked_font_geometry_rejects_invalid_inputs_and_derived_overflow() {
        assert_eq!(Font.try_metrics(0, 1.0), Err(FontMeasureError::ZeroSize));
        for scale in [0.0, -0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                Font.try_metrics(8, scale),
                Err(FontMeasureError::InvalidScale)
            );
        }
        for (size, scale) in [(1, f32::from_bits(1)), (8, f32::MAX), (u16::MAX, f32::MAX)] {
            assert_eq!(
                Font.try_metrics(size, scale),
                Err(FontMeasureError::Unrepresentable)
            );
        }
        for (size, scale) in [(1, f32::MIN_POSITIVE), (1, f32::MAX), (u16::MAX, 0.125)] {
            let metrics = Font.try_metrics(size, scale).unwrap();
            assert!(metrics.pixel_size() > 0.0);
            assert!(metrics.line_height().is_finite());
        }
    }

    #[test]
    fn scaled_advances_baselines_and_ink_are_distinct() {
        let metrics = Font.try_metrics(16, 1.5).unwrap();
        assert_eq!(metrics.pixel_size(), 3.0);
        assert_eq!(metrics.glyph_advance(), 18.0);
        assert_eq!(metrics.baseline(), 21.0);
        assert_eq!(metrics.line_height(), 24.0);
        let text = metrics.measure_line("A ").unwrap();
        assert_eq!((text.width, text.height, text.line_count), (36.0, 21.0, 1));
        assert_eq!(text.ink_bounds, Some(Rect::new(0.0, 0.0, 15.0, 21.0)));
        let punctuation = metrics.measure_line(".").unwrap();
        assert_eq!(punctuation.ink_bounds, Some(Rect::new(3.0, 15.0, 6.0, 6.0)));
    }

    #[test]
    fn empty_blank_and_unsupported_scalars_have_explicit_sizes() {
        let metrics = Font.try_metrics(8, 1.0).unwrap();
        assert_eq!(metrics.measure_line(""), Ok(BitmapTextMetrics::default()));
        assert_eq!(
            metrics.measure_multiline(""),
            Ok(BitmapTextMetrics::default())
        );
        let blank = metrics.measure_line("  ").unwrap();
        assert_eq!(
            (blank.width, blank.height, blank.line_count),
            (12.0, 7.0, 1)
        );
        assert_eq!(blank.ink_bounds, None);
        assert_eq!(blank.unsupported_glyphs, 0);
        let missing = metrics.measure_line("☃é😀ß\t\r\n").unwrap();
        assert_eq!(missing.width, 42.0);
        assert_eq!(missing.unsupported_glyphs, 7);
        assert_eq!(missing.ink_bounds, None);
        assert_eq!(metrics.measure_line("abc"), metrics.measure_line("ABC"));
        assert_eq!(
            metrics.measure_line("°×—·→←↑↓").unwrap().unsupported_glyphs,
            0
        );
    }

    #[test]
    fn multiline_tracks_longest_line_crlf_and_trailing_empty_lines() {
        let metrics = Font.try_metrics(8, 1.0).unwrap();
        let label = metrics.measure_multiline("\nA☃\r\n.\n").unwrap();
        assert_eq!(
            (label.width, label.height, label.line_count),
            (12.0, 31.0, 4)
        );
        assert_eq!(label.unsupported_glyphs, 1);
        assert_eq!(label.ink_bounds, Some(Rect::new(0.0, 8.0, 5.0, 15.0)));
        assert_eq!(label.ink_bounds, drawn_bounds(["", "A☃", ".", ""], 8, 1.0));
        for text in ["\n", "\r\n"] {
            let blank = metrics.measure_multiline(text).unwrap();
            assert_eq!(
                (blank.width, blank.height, blank.line_count),
                (0.0, 15.0, 2)
            );
            assert_eq!(blank.unsupported_glyphs, 0);
            assert_eq!(blank.ink_bounds, None);
        }
        let bare_cr = metrics.measure_multiline("A\r").unwrap();
        assert_eq!(
            (
                bare_cr.width,
                bare_cr.line_count,
                bare_cr.unsupported_glyphs
            ),
            (12.0, 1, 1)
        );
        assert_eq!(
            metrics.measure_multiline("A\r\n"),
            metrics.measure_multiline("A\n")
        );
    }

    #[test]
    fn ink_bounds_match_actual_raster_rectangles_for_ascii_and_supported_unicode() {
        let chars = (0..=127).map(char::from).chain("°×—·→←↑↓☃é😀ß".chars());
        for c in chars {
            let mut buffer = [0; 4];
            let text = c.encode_utf8(&mut buffer);
            for (size, scale) in [(8, 1.0), (16, 1.5), (17, 0.7), (1, 0.25)] {
                let metrics = Font.try_metrics(size, scale).unwrap();
                let measured = metrics.measure_line(text).unwrap();
                assert_eq!(
                    measured.ink_bounds,
                    drawn_bounds([&*text], size, scale),
                    "{c:?}, {size}, {scale}"
                );
            }
        }
    }

    #[test]
    fn label_bounds_match_fractional_raster_advances_and_successive_baselines() {
        for (size, scale) in [(8, 1.0), (16, 1.5), (17, 0.7), (1, 0.25)] {
            let metrics = Font.try_metrics(size, scale).unwrap();
            let line = " a☃ . 1 LONG LABEL 29:!? → ";
            assert_eq!(
                metrics.measure_line(line).unwrap().ink_bounds,
                drawn_bounds([line], size, scale)
            );
            let mut pen = 0.0;
            for _ in line.chars() {
                pen += 6.0 * (size as f32 * scale / 8.0);
            }
            assert_eq!(metrics.measure_line(line).unwrap().width, pen);
            let text = "\n.a☃.\nAAA\n.\n";
            assert_eq!(
                metrics.measure_multiline(text).unwrap().ink_bounds,
                drawn_bounds(text.split('\n'), size, scale)
            );
        }
    }

    #[test]
    fn layout_overflow_is_checked_after_valid_font_setup() {
        let metrics = Font.try_metrics(1, f32::MAX).unwrap();
        assert!(metrics.measure_line("A").is_ok());
        for text in ["AA", "  ", "☃☃"] {
            assert_eq!(
                metrics.measure_line(text),
                Err(FontMeasureError::Unrepresentable)
            );
        }
        assert_eq!(
            metrics.measure_multiline("\n"),
            Err(FontMeasureError::Unrepresentable)
        );
        assert_eq!(
            metrics.measure_multiline("A\nB"),
            Err(FontMeasureError::Unrepresentable)
        );
        // Exercise precision loss without allocating a many-million-character label.
        assert_eq!(
            checked_advance(16_777_216.0, 1.0),
            Err(FontMeasureError::Unrepresentable)
        );
    }
}
