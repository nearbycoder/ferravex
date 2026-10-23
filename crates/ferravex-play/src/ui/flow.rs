//! Bounded, content-sized horizontal rows with optional wrapping.
//!
//! Supply measured widths in logical screen units; this helper never measures
//! text or changes its font size. Rectangles retain the parent's coordinate
//! space. In particular, pass a [`super::HudRegions`] strip directly and use a
//! zero-inset [`super::Viewport::responsive`] for its absolute screen rectangles.
//! Use each returned rectangle for both painting and [`super::Widget`] input.
//!
//! ```
//! use ferravex_play::ui::{Rect, UiState, Widget};
//! use ferravex_play::ui::flow::{layout, FlowItem, FlowOptions};
//! let items = [
//!     FlowItem::fixed(44.), // Icon/action retains its authored width.
//!     FlowItem { minimum: 100., preferred: 180., maximum: 300., grow: 1. },
//!     FlowItem::fixed(64.),
//! ];
//! let area = Rect { min: [8., 24.], size: [320., 96.] };
//! let mut rects = [area; 3];
//! let report = layout(area, &items, FlowOptions::default(), &mut rects)?;
//! assert_eq!((report.written, report.overflow, report.rows), (3, 0, 1));
//! assert_eq!(rects.map(|r| r.size[0]), [44., 196., 64.]);
//! let mut ui = UiState::with_capacity(3); // Reuse between frames.
//! ui.begin_frame();
//! for (index, &rect) in rects[..report.written].iter().enumerate() {
//!     // Paint this same rect with a fixed-size label in the host.
//!     ui.add(Widget { id: index as u64 + 1, rect, clip: Some(area),
//!                     enabled: true, focusable: true }).unwrap();
//! }
//! ui.end_frame();
//! assert_eq!(ui.hit_test([60., 30.]), Some(2));
//! assert_eq!(ui.focus_next(false), Some(1));
//! # Ok::<(), ferravex_play::ui::flow::FlowError>(())
//! ```

use super::Rect;

/// Maximum input length. Layout uses fixed stack scratch and no heap allocation.
pub const FLOW_ITEM_CAPACITY: usize = 256;

/// Horizontal extent policy. All fields must be finite and nonnegative, with
/// `minimum <= preferred <= maximum`. Zero-width items are allowed and inert
/// unless they receive width; they still count as items and separate gaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowItem {
    /// Hard lower bound, typically readable label width plus padding.
    pub minimum: f32,
    /// Desired width before weighted growth, often the measured full label.
    pub preferred: f32,
    /// Hard upper bound, including growth.
    pub maximum: f32,
    /// Relative weight for space beyond preferred. Zero opts out of growth.
    pub grow: f32,
}

impl FlowItem {
    /// An item that neither shrinks nor grows. Validation happens in [`layout`].
    ///
    /// ```
    /// use ferravex_play::ui::flow::FlowItem;
    /// let icon = FlowItem::fixed(44.);
    /// assert_eq!((icon.minimum, icon.preferred, icon.maximum, icon.grow),
    ///            (44., 44., 44., 0.));
    /// ```
    pub const fn fixed(width: f32) -> Self {
        Self {
            minimum: width,
            preferred: width,
            maximum: width,
            grow: 0.,
        }
    }
}

/// Row policy in the parent's units. Defaults: height 44, gaps [8, 8], wrapping.
/// There is no leading/trailing gap, row stretching, justification or font scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowOptions {
    /// Positive, finite height shared by all rows; partial-height rows are omitted.
    pub row_height: f32,
    /// Finite, nonnegative horizontal and vertical gaps, respectively.
    pub gap: [f32; 2],
    /// Start another row when the next minimum no longer fits. If false, stop
    /// after the first row. An item wider than the parent stops either mode.
    pub wrap: bool,
}

impl Default for FlowOptions {
    fn default() -> Self {
        Self {
            row_height: 44.,
            gap: [8.; 2],
            wrap: true,
        }
    }
}

/// Only `output[..written]` is current geometry. Submit that prefix to input and
/// painting; rebuild input each frame so omitted widgets lose focus/capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowReport {
    /// Number of rectangles copied, matching `items[..written]` in source order.
    pub written: usize,
    /// `items.len() - written`: includes space overflow and output truncation.
    /// No item is skipped in favor of a smaller later item.
    pub overflow: usize,
    /// Number of rows containing copied rectangles, zero when `written == 0`.
    pub rows: usize,
}

/// Errors leave the entire output unchanged. Ordinary insufficient space or a
/// short output buffer is instead reported by [`FlowReport::overflow`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowError {
    /// More than [`FLOW_ITEM_CAPACITY`] items; checked before inspecting items.
    Capacity,
    /// Invalid parent, row height or gaps.
    InvalidLayout,
    /// Non-finite/negative item fields or unordered extents. All input items are
    /// validated, including the suffix that will not fit. First bad index wins.
    InvalidItem { index: usize },
    /// Finite inputs cannot produce representable, disjoint f32 rectangles under
    /// the rounding policy without violating a minimum or losing a positive edge.
    Unrepresentable,
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Capacity => f.write_str("flow item capacity exceeded"),
            Self::InvalidLayout => f.write_str("invalid flow bounds or row policy"),
            Self::InvalidItem { index } => write!(f, "invalid flow item at index {index}"),
            Self::Unrepresentable => f.write_str("unrepresentable flow geometry"),
        }
    }
}

impl std::error::Error for FlowError {}

/// Lay out a stable prefix left-to-right, then top-to-bottom, without allocation.
///
/// Rows greedily admit consecutive **minimum** widths and intervening gaps, with
/// equality fitting. Each row shares space toward preferred widths in proportion
/// to `(preferred - minimum)`, independently of grow weights. After all preferred
/// widths fit, remaining space is distributed by positive grow weight. Capped
/// items relinquish their share to uncapped items. Zero weights and capped rows
/// leave unused space at the right. The last row follows the same policy.
///
/// No partial-height row or undersized item is emitted. A minimum wider than the
/// parent, exhausted height, or disabled wrapping ends the prefix. Empty parents
/// produce zero rectangles. A short output buffer truncates the computed prefix
/// **without changing row widths**; its length does not affect layout decisions.
/// All items are validated and all fitting rows computed even with empty output.
/// Only the written prefix changes on success; the output tail is untouched.
/// Empty input succeeds and there is no retained state to reset.
///
/// Work is bounded by 256 items and at most one weighted saturation pass per item
/// per row (O(n²) worst case); scratch is O(256) on the stack. Sums and weighted
/// shares use f64 to avoid f32 overflow. Final widths round down, respecting the
/// minimum/maximum; origins round up past the preceding edge plus gap. Space for
/// later minimum widths is reserved backward, rounding inward. Stored extents
/// and half-open f32 endpoints stay inside the parent and never overlap. Rounding
/// may leave small slack or reduce a preferred width; it never relaxes a minimum.
/// Unrepresentable geometry rejects the entire call, including earlier rows.
///
/// ```
/// use ferravex_play::ui::Rect;
/// use ferravex_play::ui::flow::{layout, FlowItem, FlowOptions, FlowError};
/// let area = Rect { min: [0.; 2], size: [96., 96.] };
/// let items = [FlowItem::fixed(44.); 3];
/// let mut output = [area; 2];
/// let report = layout(area, &items, FlowOptions::default(), &mut output)?;
/// assert_eq!((report.written, report.overflow, report.rows), (2, 1, 1));
/// assert_eq!(output[1].min, [52., 0.]);
/// let before = output;
/// assert_eq!(layout(area, &[FlowItem::fixed(f32::NAN)],
///                   FlowOptions::default(), &mut output),
///            Err(FlowError::InvalidItem { index: 0 }));
/// assert_eq!(output, before); // Atomic failure.
/// let empty = layout(area, &[], FlowOptions::default(), &mut output)?;
/// assert_eq!(empty.written, 0); // Consume no stale output on the next frame.
/// # Ok::<(), FlowError>(())
/// ```
pub fn layout(
    parent: Rect,
    items: &[FlowItem],
    options: FlowOptions,
    output: &mut [Rect],
) -> Result<FlowReport, FlowError> {
    if items.len() > FLOW_ITEM_CAPACITY {
        return Err(FlowError::Capacity);
    }
    if !parent.valid()
        || !options.row_height.is_finite()
        || options.row_height <= 0.
        || !options.gap.into_iter().all(nonnegative)
    {
        return Err(FlowError::InvalidLayout);
    }
    for (index, item) in items.iter().enumerate() {
        if ![item.minimum, item.preferred, item.maximum, item.grow]
            .into_iter()
            .all(nonnegative)
            || item.minimum > item.preferred
            || item.preferred > item.maximum
        {
            return Err(FlowError::InvalidItem { index });
        }
    }
    let mut report = FlowReport {
        overflow: items.len(),
        ..FlowReport::default()
    };
    if parent.size.contains(&0.) || items.is_empty() {
        return Ok(report);
    }
    let mut scratch = [parent; FLOW_ITEM_CAPACITY];
    let mut start = 0;
    let mut y = parent.min[1];
    let bottom = parent.min[1] + parent.size[1];
    while start < items.len() {
        if options.row_height > parent.size[1]
            || y as f64 + options.row_height as f64 > bottom as f64
        {
            break;
        }
        let row_bottom = y + options.row_height;
        if row_bottom <= y {
            return Err(FlowError::Unrepresentable);
        }
        let mut end = start;
        let mut minimum = 0.;
        while end < items.len() {
            let next = minimum
                + items[end].minimum as f64
                + if end == start {
                    0.
                } else {
                    options.gap[0] as f64
                };
            if next > parent.size[0] as f64 {
                break;
            }
            minimum = next;
            end += 1;
        }
        if end == start {
            break;
        }
        place_row(
            parent,
            &items[start..end],
            options,
            y,
            &mut scratch[start..end],
        )?;
        if start < output.len() {
            report.rows += 1;
        }
        start = end;
        if !options.wrap || start == items.len() {
            break;
        }
        let next_y = row_bottom as f64 + options.gap[1] as f64;
        if next_y > bottom as f64 {
            break;
        }
        y = round_up(next_y);
    }
    report.written = start.min(output.len());
    report.overflow = items.len() - report.written;
    output[..report.written].copy_from_slice(&scratch[..report.written]);
    Ok(report)
}

fn nonnegative(value: f32) -> bool {
    value.is_finite() && value >= 0.
}

fn round_down(value: f64) -> f32 {
    let rounded = value as f32;
    if rounded as f64 > value {
        rounded.next_down()
    } else {
        rounded
    }
}

fn round_up(value: f64) -> f32 {
    let rounded = value as f32;
    if (rounded as f64) < value {
        rounded.next_up()
    } else {
        rounded
    }
}

fn place_row(
    parent: Rect,
    items: &[FlowItem],
    options: FlowOptions,
    y: f32,
    output: &mut [Rect],
) -> Result<(), FlowError> {
    let mut widths = [0.; FLOW_ITEM_CAPACITY];
    let widths = &mut widths[..items.len()];
    let budget = parent.size[0] as f64 - options.gap[0] as f64 * (items.len() - 1) as f64;
    let minimum: f64 = items.iter().map(|item| item.minimum as f64).sum();
    let desire: f64 = items
        .iter()
        .map(|item| item.preferred as f64 - item.minimum as f64)
        .sum();
    let fraction = if desire > 0. {
        ((budget - minimum) / desire).clamp(0., 1.)
    } else {
        1.
    };
    for (width, item) in widths.iter_mut().zip(items) {
        *width = item.minimum as f64 + fraction * (item.preferred as f64 - item.minimum as f64);
    }
    if fraction == 1. {
        grow(items, widths, (budget - minimum - desire).max(0.));
    }
    // Reserve every later minimum and gap before spending width on this item.
    let mut limits = [0.; FLOW_ITEM_CAPACITY];
    let mut right = parent.min[0] + parent.size[0];
    for index in (0..items.len()).rev() {
        limits[index] = right;
        let left = round_down(right as f64 - items[index].minimum as f64);
        right = round_down(left as f64 - options.gap[0] as f64);
    }
    let mut x = parent.min[0];
    for (index, ((item, width), rect)) in items.iter().zip(widths).zip(output).enumerate() {
        let room = round_down(limits[index] as f64 - x as f64);
        let extent = round_down(*width).max(item.minimum).min(room);
        let edge = x + extent;
        if !x.is_finite()
            || extent < item.minimum
            || extent > item.maximum
            || edge > limits[index]
            || (extent > 0. && edge <= x)
        {
            return Err(FlowError::Unrepresentable);
        }
        *rect = Rect {
            min: [x, y],
            size: [extent, options.row_height],
        };
        if index + 1 < items.len() {
            x = round_up(edge as f64 + options.gap[0] as f64);
        }
    }
    Ok(())
}

fn grow(items: &[FlowItem], widths: &mut [f64], mut remaining: f64) {
    // Each saturation pass removes at least one participant, or finishes.
    for _ in 0..items.len() {
        let weight: f64 = items
            .iter()
            .zip(widths.iter())
            .filter(|(item, width)| item.grow > 0. && **width < item.maximum as f64)
            .map(|(item, _)| item.grow as f64)
            .sum();
        if remaining <= 0. || weight == 0. {
            break;
        }
        let mut capped = false;
        let share = remaining / weight;
        for (item, width) in items.iter().zip(widths.iter_mut()) {
            let room = item.maximum as f64 - *width;
            if item.grow > 0. && room > 0. && share * item.grow as f64 >= room {
                *width = item.maximum as f64;
                remaining = (remaining - room).max(0.);
                capped = true;
            }
        }
        if !capped {
            for (item, width) in items.iter().zip(widths.iter_mut()) {
                if item.grow > 0. && *width < item.maximum as f64 {
                    *width = (*width + share * item.grow as f64).min(item.maximum as f64);
                }
            }
            break;
        }
    }
}
