//! Borrowed word wrapping for the UI's existing 5×7 bitmap font.
//!
//! Coordinates are logical UI units, before viewport scaling. Each Unicode
//! scalar occupies one cell; UI painting uppercases ASCII and substitutes `?`
//! for unsupported glyphs. This is not shaping or a second font engine.
//!
//! Wrapping deliberately matches [`super::Ui::text`]: LF separates lines,
//! internal blank lines survive, and a final LF does not add another line.
//! Spaces, tabs and CR at a word-wrap boundary are skipped; otherwise whitespace
//! occupies cells (including CR before LF). Unicode whitespace can be a word
//! boundary, but only those three ASCII characters are skipped after it.
//! This differs from immediate-font `measure_multiline`'s CRLF/trailing-line
//! convention; that API is unchanged.
//!
//! A content-sized briefing/card at a 320-unit viewport width:
//!
//! ```
//! use ferravex_render::ui::{TextStyle, Ui, UiRect, Viewport};
//! use ferravex_render::ui::text_layout::{LineLimit, WrappedText};
//! let copy = "Collect the amber keys before opening the gate.\n\n\
//!             Return to the checkpoint if the route is blocked.";
//! let style = TextStyle { size: 18., wrap: true, ..Default::default() };
//! let limit = LineLimit::Unlimited; // Or Ellipsis(4) for a compact card.
//! let layout = WrappedText::new(copy, 288., style.size)?.with_limit(limit);
//! let measured = layout.measure();
//! let mut ui = Ui::new(); // Reuse across frames.
//! ui.begin(Viewport::responsive([320., 400.], [0.; 4])?);
//! let body = UiRect { min: [16., 24.], size: [288., measured.height] };
//! assert_eq!(ui.wrapped_text(body, copy, style, limit)?, measured);
//! ui.paint();
//! // Place the next block at body.min[1] + measured.height + your chosen gap.
//! # Ok::<(), ferravex_render::ui::UiError>(())
//! ```

use super::UiError;
use crate::BitmapFontMetrics;
use std::{iter::FusedIterator, ops::Range};

/// An explicit count of retained lines, independent of rectangle/scissor clipping.
/// Zero retains no lines. Neither policy changes font size or line breaking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineLimit {
    /// Retain every wrapped line (the existing UI behavior).
    #[default]
    Unlimited,
    /// Keep at most this many lines, without a suffix.
    Clip(usize),
    /// Keep at most this many lines; if more exist, replace the final line's
    /// tail with `...`. A width fitting only one/two cells uses `.`/`..`;
    /// a width below one cell has no marker. An exact fit gets no marker.
    Ellipsis(usize),
}

impl LineLimit {
    fn count(self) -> usize {
        match self {
            Self::Unlimited => usize::MAX,
            Self::Clip(count) | Self::Ellipsis(count) => count,
        }
    }
}

/// One borrowed line. `text` followed by `suffix` is the complete displayed copy.
/// Empty lines still occupy one line slot. Ranges are always UTF-8 boundaries.
///
/// ```
/// use ferravex_render::ui::text_layout::{LineLimit, WrappedText};
/// let copy = "é日ABC DEF";
/// let layout = WrappedText::new(copy, 30., 8.)?.with_limit(LineLimit::Ellipsis(1));
/// let line = layout.lines().next().unwrap();
/// assert_eq!(&copy[line.byte_range], line.text);
/// assert_eq!((line.text, line.suffix, line.width), ("é日", "...", 30.));
/// # Ok::<(), ferravex_render::ui::UiError>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine<'a> {
    /// Range of `text` in the original source, excluding skipped separators and
    /// any tail replaced by an ellipsis. The suffix has no source range.
    pub byte_range: Range<usize>,
    /// Borrowed visible prefix, without a line separator.
    pub text: &'a str,
    /// Static ASCII marker, or the empty string.
    pub suffix: &'static str,
    /// Advance of prefix plus suffix, including the final spacing column.
    /// A hard-wrapped cell can exceed the requested width when it is too narrow.
    pub width: f32,
}

/// Line-slot measurements, before rectangle/scissor clipping and without shadows.
/// Unlike tight ink bounds, height reserves a full line advance for the last line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextBlockMetrics {
    /// Wrapped lines before applying the maximum-line policy.
    pub line_count: usize,
    /// Lines retained by the policy, including empty lines.
    pub visible_lines: usize,
    /// Whole lines excluded by the policy; does not count an ellipsized tail
    /// or lines subsequently clipped by the painter's rectangle/scissor.
    pub hidden_lines: usize,
    /// Height needed for every line: `line_count * line_height`.
    pub required_height: f32,
    /// Height of retained lines: `visible_lines * line_height`.
    pub height: f32,
    /// Largest retained line advance, including any ellipsis.
    pub width: f32,
}

impl TextBlockMetrics {
    /// Whether the line policy omitted copy, even when a tiny width cannot show
    /// an ellipsis. Geometric clipping is independent of this result.
    pub fn is_truncated(self) -> bool {
        self.hidden_lines != 0
    }
}

/// Immutable borrowed layout. Construction and iteration allocate no storage;
/// each measurement scans the source to count hidden lines as well as visible
/// ones. Work is linear in source length, not bounded by the line limit.
/// [`Self::lines`] starts a fresh iterator, so reflow needs no retained cache/reset.
#[derive(Clone, Copy, Debug)]
pub struct WrappedText<'a> {
    text: &'a str,
    width: f32,
    metrics: BitmapFontMetrics,
    limit: LineLimit,
}

impl<'a> WrappedText<'a> {
    /// Validate finite nonnegative width and the UI's `1..=256` size range.
    /// Invalid dimensions return [`UiError::InvalidLayout`]. No text-length cap
    /// is imposed on this borrowed view; queuing into `Ui` has separate budgets.
    /// Widths below one cell (including zero) still wrap one scalar per line,
    /// matching the painter; they require geometric clipping at paint time.
    ///
    /// ```
    /// use ferravex_render::ui::{UiError, text_layout::WrappedText};
    /// assert_eq!(WrappedText::new("copy", -1., 18.).unwrap_err(), UiError::InvalidLayout);
    /// let layout = WrappedText::new("AB CD", 24., 16.)?;
    /// assert_eq!(layout.measure().required_height, 32.);
    /// assert_eq!(layout.line_height(), 16.);
    /// # Ok::<(), UiError>(())
    /// ```
    pub fn new(text: &'a str, width: f32, size: f32) -> Result<Self, UiError> {
        if !width.is_finite() || width < 0. || !size.is_finite() || !(1. ..=256.).contains(&size) {
            return Err(UiError::InvalidLayout);
        }
        // Size/8 is the UI painter's bitmap pixel size, including fractional sizes.
        let metrics = crate::get_default_font()
            .try_metrics(8, size / 8.)
            .map_err(|_| UiError::InvalidLayout)?;
        Ok(Self {
            text,
            width,
            metrics,
            limit: LineLimit::Unlimited,
        })
    }

    /// Replace the maximum-line policy without changing the source or wrapping.
    ///
    /// ```
    /// use ferravex_render::ui::text_layout::{LineLimit, WrappedText};
    /// let layout = WrappedText::new("A\nB\nC", 60., 8.)?;
    /// let compact = layout.with_limit(LineLimit::Clip(1)).measure();
    /// assert_eq!((compact.visible_lines, compact.hidden_lines), (1, 2));
    /// assert_eq!((compact.height, compact.required_height), (8., 24.));
    /// assert!(compact.is_truncated());
    /// assert_eq!(layout.lines().count(), 3); // Original is reusable.
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn with_limit(mut self, limit: LineLimit) -> Self {
        self.limit = limit;
        self
    }

    /// Vertical distance between line tops in logical UI units.
    pub fn line_height(&self) -> f32 {
        self.metrics.line_height()
    }

    /// Start an independent, allocation-free iterator of retained line spans.
    /// Each line starts at `index as f32 * line_height()` below the block top.
    pub fn lines(&self) -> WrappedLines<'a> {
        let columns = (self.width / self.metrics.glyph_advance()).floor() as usize;
        WrappedLines::new(
            self.text,
            columns,
            columns,
            self.metrics.glyph_advance(),
            self.limit,
        )
    }

    /// Count all wrapped lines and measure the retained block without allocating.
    /// To avoid partial last-line clipping, give `Ui::wrapped_text` a rectangle
    /// at least `height` tall. For a fixed card use `Clip(n)`/`Ellipsis(n)`, with
    /// `n = floor(available_height / line_height)`, and inspect `hidden_lines`.
    pub fn measure(&self) -> TextBlockMetrics {
        let mut lines = self.lines();
        let mut result = TextBlockMetrics::default();
        for line in lines.by_ref() {
            result.visible_lines += 1;
            result.width = result.width.max(line.width);
        }
        // The iterator leaves omitted lines unconsumed so measuring can count
        // them without a second wrapping pass over the visible prefix.
        result.hidden_lines = lines.lines.count();
        result.line_count = result.visible_lines + result.hidden_lines;
        result.required_height = result.line_count as f32 * self.line_height();
        result.height = result.visible_lines as f32 * self.line_height();
        result
    }
}

/// Restartable via [`WrappedText::lines`], cloneable, constant-storage line cursor.
#[derive(Clone, Debug)]
pub struct WrappedLines<'a> {
    lines: Lines<'a>,
    source_len: usize,
    remaining: usize,
    ellipsis: bool,
    fitting_columns: usize,
    advance: f32,
}

impl<'a> WrappedLines<'a> {
    pub(super) fn new(
        text: &'a str,
        wrap_columns: usize,
        fitting_columns: usize,
        advance: f32,
        limit: LineLimit,
    ) -> Self {
        Self {
            lines: Lines::new(text, wrap_columns),
            source_len: text.len(),
            remaining: limit.count(),
            ellipsis: matches!(limit, LineLimit::Ellipsis(_)),
            fitting_columns,
            advance,
        }
    }
}

impl<'a> Iterator for WrappedLines<'a> {
    type Item = TextLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let start = self.source_len - self.lines.text.len();
        let mut text = self.lines.next()?;
        self.remaining -= 1;
        let suffix = if self.ellipsis && self.remaining == 0 && !self.lines.text.is_empty() {
            let dots = self.fitting_columns.min(3);
            let keep = self.fitting_columns - dots;
            let end = text.char_indices().nth(keep).map_or(text.len(), |(i, _)| i);
            text = &text[..end];
            &"..."[..dots]
        } else {
            ""
        };
        Some(TextLine {
            byte_range: start..start + text.len(),
            text,
            suffix,
            width: (text.chars().count() + suffix.len()) as f32 * self.advance,
        })
    }
}

impl FusedIterator for WrappedLines<'_> {}

// The original Painter::text line-breaking algorithm. Keep this single source
// for legacy wrapped/unwrapped painting and the public measured layout.
#[derive(Clone, Debug)]
pub(super) struct Lines<'a> {
    text: &'a str,
    columns: usize,
}
impl<'a> Lines<'a> {
    pub(super) fn new(text: &'a str, columns: usize) -> Self {
        Self {
            text,
            columns: columns.max(1),
        }
    }
}
impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.text.is_empty() {
            return None;
        }
        let mut end = self.text.len();
        let mut consumed = end;
        for (count, (i, c)) in self.text.char_indices().enumerate() {
            if c == '\n' {
                end = i;
                consumed = i + 1;
                break;
            }
            if count == self.columns {
                end = i;
                consumed = i;
                // Prefer whole words. Hard wrapping remains necessary for a
                // single long word; all slices stay on UTF-8 boundaries.
                let boundary = if c.is_whitespace() {
                    Some(i)
                } else {
                    self.text[..i].rfind(char::is_whitespace)
                };
                if let Some(boundary) = boundary {
                    let prefix = self.text[..boundary].trim_end();
                    if !prefix.is_empty() {
                        end = prefix.len();
                        let rest = self.text[boundary..].trim_start_matches([' ', '\t', '\r']);
                        consumed = self.text.len() - rest.len();
                    }
                }
                break;
            }
        }
        let line = &self.text[..end];
        self.text = &self.text[consumed..];
        Some(line)
    }
}
