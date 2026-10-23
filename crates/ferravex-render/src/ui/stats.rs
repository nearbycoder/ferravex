//! Measured, bounded label/value lists in logical UI units.
//!
//! Values and units belong to the caller: borrow already formatted strings (a
//! reusable `String` works), including explicit unavailable copy. No parsing,
//! rounding, unit conversion, ellipsis or font scaling occurs here. Wrapping
//! uses [`WrappedText`] and the existing bitmap painter: ASCII is uppercased,
//! unsupported Unicode glyphs paint as `?`, and wrap-boundary whitespace follows
//! that API. Inspection retains the supplied UTF-8 copy.
//!
//! A runnable result-summary recipe with actual retained UI and inspection.
//! The same content selects columns at 640 logical pixels and stacks at 320;
//! the card accepts only a complete list. A caller could instead scroll, expand,
//! or show an explicit hidden-row message using the returned report.
//!
//! ```
//! use ferravex_render::ui::{ButtonStyle, Ui, UiError, UiLabel, UiRect, Viewport};
//! use ferravex_render::ui::stats::{StatRow, StatRowsLayout, StatRowsMode, StatRowsStyle};
//! use std::fmt::Write;
//! let mut ui = Ui::new(); // Retain across frames.
//! ui.set_inspection_enabled(true);
//! let mut time = String::with_capacity(32);
//! let mut delta = String::with_capacity(32);
//! let mut count = String::with_capacity(32);
//! // Caller owns time rollover, signs, precision and units.
//! let millis = 60_001_u64;
//! write!(time, "{}:{:02}.{:03}", millis / 60_000, millis / 1_000 % 60, millis % 1_000).unwrap();
//! write!(delta, "{:+.3} s", -1.25_f64).unwrap();
//! write!(count, "{}", u64::MAX).unwrap();
//! let rows = [
//!     StatRow { label: "TIME", value: &time, description: None },
//!     StatRow { label: "DELTA", value: &delta, description: None },
//!     StatRow { label: "DELIVERIES", value: &count, description: Some("Accepted cargo only.") },
//!     StatRow { label: "BEST", value: "UNAVAILABLE", description: None },
//! ];
//! for (width, mode) in [(640., StatRowsMode::Columns), (320., StatRowsMode::Stacked)] {
//!     ui.begin(Viewport::responsive([width, 440.], [0.; 4])?);
//!     let bounds = UiRect { min: [16., 16.], size: [width - 32., 340.] };
//!     let layout = StatRowsLayout::new(bounds, &rows, StatRowsStyle::default())?;
//!     assert_eq!(layout.report().mode, mode);
//!     let submitted = ui.compose(|card| {
//!         card.panel(UiRect { min: [8., 8.], size: [width - 16., 420.] }, ferravex_render::ui::PANEL)?;
//!         let report = layout.submit(card)?;
//!         if report.hidden_rows != 0 { return Err(UiError::Capacity); }
//!         card.button(901, UiRect { min: [16., 374.], size: [width - 32., 44.] },
//!                     "CONTINUE", true, ButtonStyle::default())?;
//!         Ok(report)
//!     })?;
//!     assert_eq!((submitted.visible_rows, submitted.hidden_rows), (4, 0));
//!     // These are the actual retained label/value rectangles, shared with paint.
//!     let mut inspection = ui.inspect().unwrap().skip(1); // Panel.
//!     for (row, geometry) in rows.iter().zip(layout.rows()) {
//!         for (copy, rect) in [(row.label, geometry.label), (row.value, geometry.value)] {
//!             let element = inspection.next().unwrap();
//!             assert_eq!(element.label, UiLabel::Full(copy));
//!             assert_eq!(element.painted_label, copy);
//!             assert_eq!(element.bounds, rect);
//!             assert_eq!(element.visible_bounds, Some(rect));
//!         }
//!         if let Some(copy) = row.description {
//!             let element = inspection.next().unwrap();
//!             assert_eq!(element.label, UiLabel::Full(copy));
//!             assert_eq!(element.bounds, geometry.description.unwrap());
//!         }
//!     }
//!     ui.paint(); // Headless retained paint, not a device readability test.
//! }
//! # Ok::<(), UiError>(())
//! ```

use super::text_layout::{LineLimit, WrappedText};
use super::{TextStyle, Ui, UiComposition, UiError, UiRect};

/// Hard bound on measured rows. Larger slices return [`UiError::Capacity`]
/// before scanning any text; callers can paginate their source explicitly.
pub const MAX_STAT_ROWS: usize = 64;

/// Borrowed caller-authored copy. An empty value stays empty; supply a distinct
/// string such as `"UNAVAILABLE"` when that is the metric's meaning.
#[derive(Clone, Copy, Debug)]
pub struct StatRow<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub description: Option<&'a str>,
}

/// Fixed text sizes and nonnegative gaps, all in logical units. Each block
/// honors color/alignment/shadow; `wrap` is ignored and always enabled. Defaults
/// use size 18 for every block. No automatic font-size/accessibility policy is
/// imposed; choose a readable size and responsive viewport in the consumer.
#[derive(Clone, Copy, Debug)]
pub struct StatRowsStyle {
    pub label: TextStyle,
    pub value: TextStyle,
    pub description: TextStyle,
    pub column_gap: f32,
    pub row_gap: f32,
    pub stack_gap: f32,
    pub description_gap: f32,
}

impl Default for StatRowsStyle {
    fn default() -> Self {
        Self {
            label: TextStyle::default(),
            value: TextStyle::default(),
            description: TextStyle::default(),
            column_gap: 16.,
            row_gap: 12.,
            stack_gap: 4.,
            description_gap: 4.,
        }
    }
}

/// One mode is selected for the whole list using every supplied label/value,
/// including rows hidden by height. Descriptions do not affect column choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatRowsMode {
    Columns,
    Stacked,
}

/// Measurements before viewport projection, ancestor clipping or occlusion.
/// Visible rows form a complete prefix; no partial row is ever submitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatRowsReport {
    pub mode: StatRowsMode,
    pub visible_rows: usize,
    pub hidden_rows: usize,
    /// Full height of every row plus intervening gaps at this width.
    pub required_height: f32,
    /// Height of the visible prefix, excluding a trailing row gap.
    pub used_height: f32,
    /// Shared column width, or the whole bounds width in stacked mode.
    pub label_width: f32,
    /// Bounds are narrower than one cell of a participating style. All rows
    /// are hidden instead of clipping a scalar horizontally. Empty lists false.
    pub width_overflow: bool,
}

/// Exact full-line-slot rectangles used for accepted text commands. The row
/// includes its description but excludes the gap before the next row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatRowGeometry {
    pub row: UiRect,
    pub label: UiRect,
    pub value: UiRect,
    pub description: Option<UiRect>,
}

const EMPTY_RECT: UiRect = UiRect {
    min: [0.; 2],
    size: [0.; 2],
};
const EMPTY_GEOMETRY: StatRowGeometry = StatRowGeometry {
    row: EMPTY_RECT,
    label: EMPTY_RECT,
    value: EMPTY_RECT,
    description: None,
};

/// Immutable borrowed layout with fixed stack storage for at most 64 row
/// geometries, no heap allocation or retained cache. Rebuild for changed copy,
/// styles or bounds; construction scans all supplied text, including hidden
/// rows. Source lengths and scanning work are caller-owned, not capped here.
/// Submission copies visible source into the existing bounded UI arena.
#[derive(Debug)]
pub struct StatRowsLayout<'a> {
    source: &'a [StatRow<'a>],
    style: StatRowsStyle,
    geometry: [StatRowGeometry; MAX_STAT_ROWS],
    report: StatRowsReport,
}

impl<'a> StatRowsLayout<'a> {
    /// Measure with the shared wrapper, keeping every block at its supplied
    /// size. Columns require the widest natural label and value plus the gap
    /// to fit. Otherwise each label is above its value at full available width;
    /// descriptions always follow the pair at full width. Empty blocks reserve
    /// one line slot. Zero/tiny valid bounds return hidden rows, not cropped text.
    ///
    /// Invalid/nonfinite bounds, sizes, colors or gaps return `InvalidLayout`;
    /// so does unrepresentable geometry/height. Sizes use the existing UI range
    /// `1..=256`. More than [`MAX_STAT_ROWS`] returns `Capacity`.
    pub fn new(
        bounds: UiRect,
        rows: &'a [StatRow<'a>],
        style: StatRowsStyle,
    ) -> Result<Self, UiError> {
        if !bounds.valid() {
            return Err(UiError::InvalidLayout);
        }
        for text in [style.label, style.value, style.description] {
            WrappedText::new("", 0., text.size)?;
            if ![text.color.r, text.color.g, text.color.b, text.color.a]
                .into_iter()
                .all(f32::is_finite)
            {
                return Err(UiError::InvalidLayout);
            }
        }
        if ![
            style.column_gap,
            style.row_gap,
            style.stack_gap,
            style.description_gap,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.)
        {
            return Err(UiError::InvalidLayout);
        }
        if rows.len() > MAX_STAT_ROWS {
            return Err(UiError::Capacity);
        }
        let mut label_width = cell(style.label);
        let mut value_width = cell(style.value);
        let mut min_width = label_width.max(value_width);
        for row in rows {
            label_width = label_width.max(
                WrappedText::new(row.label, f32::MAX, style.label.size)?
                    .measure()
                    .width,
            );
            value_width = value_width.max(
                WrappedText::new(row.value, f32::MAX, style.value.size)?
                    .measure()
                    .width,
            );
            if row.description.is_some() {
                min_width = min_width.max(cell(style.description));
            }
        }
        let columns = label_width as f64 + style.column_gap as f64 + value_width as f64
            <= bounds.size[0] as f64;
        let mode = if columns {
            StatRowsMode::Columns
        } else {
            StatRowsMode::Stacked
        };
        let label_width = if columns { label_width } else { bounds.size[0] };
        let value_x = if columns {
            label_width + style.column_gap
        } else {
            0.
        };
        let value_width = bounds.size[0] - value_x;
        let width_overflow = !rows.is_empty() && bounds.size[0] < min_width;
        let mut layout = Self {
            source: rows,
            style,
            geometry: [EMPTY_GEOMETRY; MAX_STAT_ROWS],
            report: StatRowsReport {
                mode,
                visible_rows: 0,
                hidden_rows: rows.len(),
                required_height: 0.,
                used_height: 0.,
                label_width,
                width_overflow,
            },
        };
        let mut bottom = bounds.min[1];
        let mut full_height = 0.;
        let mut stopped = width_overflow;
        for (index, row) in rows.iter().enumerate() {
            let top = advance(bottom, if index == 0 { 0. } else { style.row_gap })?;
            let label_height = height(row.label, label_width, style.label)?;
            let value_height = height(row.value, value_width, style.value)?;
            let label = UiRect {
                min: [bounds.min[0], top],
                size: [label_width, label_height],
            };
            let value = UiRect {
                min: [
                    bounds.min[0] + value_x,
                    if columns {
                        top
                    } else {
                        advance(advance(top, label_height)?, style.stack_gap)?
                    },
                ],
                size: [value_width, value_height],
            };
            let pair_bottom =
                advance(label.min[1], label_height)?.max(advance(value.min[1], value_height)?);
            let description = row
                .description
                .map(|copy| -> Result<UiRect, UiError> {
                    Ok(UiRect {
                        min: [bounds.min[0], advance(pair_bottom, style.description_gap)?],
                        size: [
                            bounds.size[0],
                            height(copy, bounds.size[0], style.description)?,
                        ],
                    })
                })
                .transpose()?;
            bottom = if let Some(d) = description {
                advance(d.min[1], d.size[1])?
            } else {
                pair_bottom
            };
            // A subtraction can round down; reserve an outward-rounded row
            // extent so its own f32 endpoint encloses every full text slot.
            let row_height = covering_extent(top, bottom)?;
            bottom = advance(top, row_height)?;
            full_height = covering_extent(bounds.min[1], bottom)?;
            if stopped || full_height > bounds.size[1] {
                stopped = true;
                continue;
            }
            let geometry = StatRowGeometry {
                row: UiRect {
                    min: [bounds.min[0], top],
                    size: [bounds.size[0], row_height],
                },
                label,
                value,
                description,
            };
            for block in [Some(geometry.row), Some(label), Some(value), description]
                .into_iter()
                .flatten()
            {
                validate_rect(bounds, block)?;
            }
            if columns && label.min[0] + label.size[0] > value.min[0] {
                return Err(UiError::InvalidLayout);
            }
            layout.geometry[index] = geometry;
            layout.report.visible_rows += 1;
            layout.report.used_height = full_height;
        }
        layout.report.required_height = full_height;
        layout.report.hidden_rows -= layout.report.visible_rows;
        Ok(layout)
    }

    /// Full and visible-prefix measurements; no geometry is changed by reading.
    pub fn report(&self) -> StatRowsReport {
        self.report
    }

    /// Visible geometry in source order, paired with the same source prefix.
    /// These rectangles are also the actual retained inspection/paint bounds.
    pub fn rows(&self) -> &[StatRowGeometry] {
        &self.geometry[..self.report.visible_rows]
    }

    /// Submit the measured prefix as a nested atomic group. Capacity/validation
    /// rejection leaves no list paint or inspection commands. Existing clips
    /// still apply; a caller must choose bounds inside its visible safe region.
    /// Handle a rejected optional group locally or propagate `?` to reject its
    /// enclosing card. Hidden rows are reported on success, never auto-painted.
    pub fn submit(&self, group: &mut UiComposition<'_>) -> Result<StatRowsReport, UiError> {
        group.compose(|group| self.submit_commands(group))
    }

    fn submit_commands(&self, group: &mut UiComposition<'_>) -> Result<StatRowsReport, UiError> {
        for (row, geometry) in self.source.iter().zip(self.rows()) {
            group.wrapped_text(
                geometry.label,
                row.label,
                self.style.label,
                LineLimit::Unlimited,
            )?;
            group.wrapped_text(
                geometry.value,
                row.value,
                self.style.value,
                LineLimit::Unlimited,
            )?;
            if let (Some(copy), Some(rect)) = (row.description, geometry.description) {
                group.wrapped_text(rect, copy, self.style.description, LineLimit::Unlimited)?;
            }
        }
        Ok(self.report)
    }
}

impl Ui {
    /// Measure and atomically submit a stat list. See [`StatRowsLayout`] for
    /// measurement, overflow policy and the executable result-summary recipe.
    ///
    /// ```
    /// use ferravex_render::ui::{Ui, UiError, UiRect, Viewport};
    /// use ferravex_render::ui::stats::{StatRow, StatRowsStyle};
    /// let mut ui = Ui::new();
    /// ui.begin(Viewport::responsive([320., 240.], [0.; 4])?);
    /// let rows = [StatRow { label: "TIME", value: "1:00.001", description: None }];
    /// let bounds = UiRect { min: [16.; 2], size: [288., 18.] };
    /// assert_eq!(ui.stat_rows(bounds, &rows, StatRowsStyle::default())?.hidden_rows, 0);
    /// // The restricted facade provides the same helper inside a result card.
    /// ui.compose(|card| card.stat_rows(bounds, &rows, StatRowsStyle::default()))?;
    /// # Ok::<(), UiError>(())
    /// ```
    pub fn stat_rows(
        &mut self,
        bounds: UiRect,
        rows: &[StatRow<'_>],
        style: StatRowsStyle,
    ) -> Result<StatRowsReport, UiError> {
        self.compose(|group| StatRowsLayout::new(bounds, rows, style)?.submit_commands(group))
    }
}

impl UiComposition<'_> {
    /// Measure and submit one nested atomic list. A handled error allows a local
    /// fallback; propagate `?` when the enclosing card requires this section.
    pub fn stat_rows(
        &mut self,
        bounds: UiRect,
        rows: &[StatRow<'_>],
        style: StatRowsStyle,
    ) -> Result<StatRowsReport, UiError> {
        self.compose(|group| StatRowsLayout::new(bounds, rows, style)?.submit_commands(group))
    }
}

fn cell(style: TextStyle) -> f32 {
    // Same metrics/operation order as WrappedText and the retained painter.
    crate::get_default_font()
        .try_metrics(8, style.size / 8.)
        .expect("validated size")
        .glyph_advance()
}
fn height(copy: &str, width: f32, style: TextStyle) -> Result<f32, UiError> {
    Ok(WrappedText::new(copy, width, style.size)?
        .measure()
        .height
        .max(style.size))
}
fn advance(start: f32, extent: f32) -> Result<f32, UiError> {
    let end = start + extent;
    if !end.is_finite() || (extent > 0. && end <= start) {
        return Err(UiError::InvalidLayout);
    }
    Ok(end)
}
fn covering_extent(start: f32, end: f32) -> Result<f32, UiError> {
    let mut extent = end - start;
    if start + extent < end {
        extent = extent.next_up();
    }
    if !extent.is_finite() {
        return Err(UiError::InvalidLayout);
    }
    Ok(extent)
}
fn validate_rect(bounds: UiRect, rect: UiRect) -> Result<(), UiError> {
    if !rect.valid()
        || (0..2).any(|axis| {
            rect.min[axis] < bounds.min[axis]
                || rect.min[axis] + rect.size[axis] > bounds.min[axis] + bounds.size[axis]
                || (rect.size[axis] > 0. && rect.min[axis] + rect.size[axis] <= rect.min[axis])
        })
    {
        return Err(UiError::InvalidLayout);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Align, Viewport};

    #[test]
    fn fixed_size_stat_paint_matches_complete_shared_wrapped_blocks() {
        let rows = [
            StatRow {
                label: "DELTA",
                value: "-1.250 s",
                description: Some("Accepted time.\n\né日"),
            },
            StatRow {
                label: "A LONG RESULT LABEL",
                value: "18446744073709551615",
                description: None,
            },
        ];
        for width in [54., 288., 1000.] {
            for align in [Align::Left, Align::Center, Align::Right] {
                crate::edit(|s| {
                    *s = crate::State::default();
                    s.w = 1200;
                    s.h = 1200;
                });
                let viewport = Viewport::responsive([1200.; 2], [0.; 4]).unwrap();
                let bounds = UiRect {
                    min: [16.; 2],
                    size: [width, 1100.],
                };
                let text = TextStyle {
                    size: 18.5,
                    align,
                    shadow: true,
                    ..Default::default()
                };
                let style = StatRowsStyle {
                    label: text,
                    value: text,
                    description: text,
                    ..Default::default()
                };
                let layout = StatRowsLayout::new(bounds, &rows, style).unwrap();
                assert_eq!(layout.report().hidden_rows, 0);
                let mut ui = Ui::new();
                ui.begin(viewport);
                ui.stat_rows(bounds, &rows, style).unwrap();
                ui.paint();
                let actual = crate::read(|s| s.vertices.clone());
                assert!(!actual.is_empty());
                crate::edit(|s| {
                    s.vertices.clear();
                    s.batches.clear();
                });
                ui.begin(viewport);
                for (row, geometry) in rows.iter().zip(layout.rows()) {
                    ui.wrapped_text(geometry.label, row.label, text, LineLimit::Unlimited)
                        .unwrap();
                    ui.wrapped_text(geometry.value, row.value, text, LineLimit::Unlimited)
                        .unwrap();
                    if let (Some(copy), Some(rect)) = (row.description, geometry.description) {
                        ui.wrapped_text(rect, copy, text, LineLimit::Unlimited)
                            .unwrap();
                    }
                }
                ui.paint();
                crate::read(|s| assert_eq!(s.vertices, actual));
                // Rollback leaves no emitted glyphs beyond the accepted list.
                let before = crate::read(|s| s.vertices.len());
                let rejected = "A".repeat(32769);
                assert_eq!(
                    ui.stat_rows(
                        UiRect {
                            size: [600_000., 1100.],
                            ..bounds
                        },
                        &[StatRow {
                            label: "REJECTED",
                            value: &rejected,
                            description: None
                        }],
                        style
                    ),
                    Err(UiError::Capacity)
                );
                crate::edit(|s| {
                    s.vertices.clear();
                    s.batches.clear();
                });
                ui.paint();
                crate::read(|s| {
                    assert_eq!(s.vertices.len(), before);
                    assert_eq!(s.vertices, actual);
                });
            }
        }
    }
}
