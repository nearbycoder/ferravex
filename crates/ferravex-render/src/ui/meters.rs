//! Passive, atomic meter compositions over a single caller telemetry snapshot.
//!
//! Label, display and status are mandatory complete caller-authored copy. Display
//! is never parsed or replaced with a computed percentage: it may deliberately
//! show a countdown while the fraction shows energy. The caller owns this
//! relationship, localization, precision and meaningful known/empty/unknown
//! wording. No gameplay availability, input registration, animation or smoothing
//! is added. Rebuild after accepted ticks; reuse the same snapshot during pause.
//!
//! Text wraps at fixed theme sizes (no shadows, shrink or ellipsis). Every block
//! and mark must fit the supplied logical bounds or construction fails. The
//! caller must reserve these bounds within its viewport/safe area and any outer
//! clips; an outer clip can still hide an otherwise valid composition. Static
//! patterns supplement hue: known fills have canvas dots, Empty a dash, Unknown
//! three separated vertical bars. All three retain explicit caller status text.
//!
//! ```
//! use ferravex_play::ui::{meters::*, theme::*};
//! use ferravex_render::ui::{Ui, UiRect, Viewport, meters::*};
//! let viewport = Viewport::responsive([352., 400.], [8., 12., 8., 12.])?;
//! let layout = MeterLayout::meter(viewport.bounds().inset([16.; 4])?,
//!     MeterCopy { label: "BOOST", display: "ENERGY 050%", status: "COOLING 1.2s" },
//!     MeterValue::Known { current: 0.5, capacity: 1. },
//!     &[Threshold { fraction: 0.25, label: "REARM >25%" }],
//!     UiTheme::preset(ThemeMode::Normal))?;
//! let mut ui = Ui::new();
//! ui.set_inspection_enabled(true);
//! ui.begin(viewport);
//! layout.draw(&mut ui)?; // Also: layout.submit(group) inside a larger compose.
//! ui.finish(true);
//! assert!(ui.inspect().unwrap().all(|e| !e.interaction_registered));
//! ui.paint();
//! # Ok::<(), ferravex_render::ui::UiError>(())
//! ```
use super::{
    Align, TextStyle, Ui, UiComposition, UiError, UiRect,
    text_layout::{LineLimit, WrappedText},
};
use ferravex_play::ui::{
    meters::{
        MAX_METER_MARKERS, MAX_METER_SEGMENTS, MeterError, MeterGeometry, MeterValue,
        SegmentGeometry, SegmentRect, SegmentValue,
    },
    theme::{ColorRole, UiTheme},
};

/// A bounded total source budget including threshold labels (UTF-8 bytes).
pub const MAX_METER_BYTES: usize = 4096;
const ZERO: UiRect = UiRect {
    min: [0.; 2],
    size: [0.; 2],
};
const CELL: SegmentRect = SegmentRect {
    bounds: ZERO,
    filled: false,
};

/// Mandatory nonblank text, borrowed unchanged. Unsupported bitmap glyphs retain
/// complete inspection copy but use the existing font's `?` painting fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeterCopy<'a> {
    pub label: &'a str,
    pub display: &'a str,
    pub status: &'a str,
}
/// Explicit normalized threshold and complete description. Its text is placed
/// in a separate row below the track, in caller order, never over a tiny fill.
/// Duplicate/unsorted thresholds are allowed; the caller owns their meaning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threshold<'a> {
    pub fraction: f64,
    pub label: &'a str,
}

/// Shared logical rectangles used for retained paint/inspection. A future input
/// adapter can consume them explicitly; these passive meters register no hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterRegions {
    pub label: UiRect,
    pub display: UiRect,
    pub status: UiRect,
    pub track: UiRect,
}
#[derive(Clone, Copy, Debug)]
enum Geometry {
    Continuous(MeterGeometry),
    Segments(SegmentGeometry),
}

/// Checked borrowed composition with fixed arrays, no retained clock or cache.
/// Reset is replacement from a new caller snapshot; no prior state can reappear.
#[derive(Clone, Debug)]
pub struct MeterLayout<'a> {
    bounds: UiRect,
    copy: MeterCopy<'a>,
    theme: UiTheme,
    regions: MeterRegions,
    well: UiRect,
    geometry: Geometry,
    cells: [SegmentRect; MAX_METER_SEGMENTS],
    count: usize,
    thresholds: &'a [Threshold<'a>],
    marks: [UiRect; MAX_METER_MARKERS],
    mark_labels: [UiRect; MAX_METER_MARKERS],
}
impl<'a> MeterLayout<'a> {
    /// Continuous composition. Known zero capacity rejects; choose explicit
    /// Empty/Unknown instead. Markers, borders and patterns stay inside the
    /// track; threshold endpoint strokes move inward to fit, as in pure geometry.
    pub fn meter(
        bounds: UiRect,
        copy: MeterCopy<'a>,
        value: MeterValue,
        thresholds: &'a [Threshold<'a>],
        theme: UiTheme,
    ) -> Result<Self, UiError> {
        Self::build(bounds, copy, Some(value), None, thresholds, theme)
    }
    /// Integer-count composition, with gaps from the fixed theme compact-gap
    /// token. Insufficient width for all cells/gaps rejects, never drops cells.
    ///
    /// ```
    /// use ferravex_play::ui::{meters::SegmentValue, theme::*};
    /// use ferravex_render::ui::{UiRect, meters::*};
    /// let route = MeterLayout::segments(UiRect { min: [0.; 2], size: [320., 240.] },
    ///     MeterCopy { label: "ROUTE", display: "2 / 6", status: "IN FLIGHT" },
    ///     SegmentValue::Known { filled: 2, capacity: 6 },
    ///     UiTheme::preset(ThemeMode::HighContrast))?;
    /// assert_eq!(route.cells().iter().filter(|c| c.filled).count(), 2);
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn segments(
        bounds: UiRect,
        copy: MeterCopy<'a>,
        value: SegmentValue,
        theme: UiTheme,
    ) -> Result<Self, UiError> {
        Self::build(bounds, copy, None, Some(value), &[], theme)
    }
    fn build(
        bounds: UiRect,
        copy: MeterCopy<'a>,
        continuous: Option<MeterValue>,
        segments: Option<SegmentValue>,
        thresholds: &'a [Threshold<'a>],
        theme: UiTheme,
    ) -> Result<Self, UiError> {
        if thresholds.len() > MAX_METER_MARKERS {
            return Err(UiError::Capacity);
        }
        let mut bytes = 0usize;
        for text in [copy.label, copy.display, copy.status]
            .into_iter()
            .chain(thresholds.iter().map(|m| m.label))
        {
            bytes = bytes.checked_add(text.len()).ok_or(UiError::Capacity)?;
            if bytes > MAX_METER_BYTES {
                return Err(UiError::Capacity);
            }
            if text.trim().is_empty() {
                return Err(UiError::InvalidLayout);
            }
        }
        check(bounds)?;
        let spacing = theme.spacing();
        let mut y = bounds.min[1];
        let label = text_rect(
            bounds,
            &mut y,
            copy.label,
            theme.types().label,
            spacing.compact_gap,
        )?;
        let display = text_rect(
            bounds,
            &mut y,
            copy.display,
            theme.types().body,
            spacing.compact_gap,
        )?;
        let status = text_rect(
            bounds,
            &mut y,
            copy.status,
            theme.types().body,
            spacing.compact_gap,
        )?;
        let track = UiRect {
            min: [bounds.min[0], y],
            size: [bounds.size[0], spacing.control_height],
        };
        contain(bounds, track)?;
        // Reject cramped tracks rather than allowing inset() to collapse them.
        if track.size.iter().any(|v| *v <= spacing.stroke * 2.) {
            return Err(UiError::InvalidLayout);
        }
        let well = track.inset([spacing.stroke; 4])?;
        check(well)?;
        let geometry = if let Some(value) = continuous {
            Geometry::Continuous(MeterGeometry::new(well, value).map_err(map_error)?)
        } else {
            Geometry::Segments(
                SegmentGeometry::new(
                    well,
                    segments.expect("one constructor kind"),
                    spacing.compact_gap,
                )
                .map_err(map_error)?,
            )
        };
        let mut result = Self {
            bounds,
            copy,
            theme,
            regions: MeterRegions {
                label,
                display,
                status,
                track,
            },
            well,
            geometry,
            cells: [CELL; MAX_METER_SEGMENTS],
            count: 0,
            thresholds,
            marks: [ZERO; MAX_METER_MARKERS],
            mark_labels: [ZERO; MAX_METER_MARKERS],
        };
        if let Geometry::Segments(g) = geometry {
            result.count = g.write_into(&mut result.cells).map_err(map_error)?;
            // Each cell must fit the fixed stroke used by its static cues.
            if result
                .cells()
                .iter()
                .any(|c| c.bounds.size[0] < spacing.stroke * 3.)
            {
                return Err(UiError::InvalidLayout);
            }
        }
        let mut fractions = [0.; MAX_METER_MARKERS];
        for (i, m) in thresholds.iter().enumerate() {
            fractions[i] = m.fraction;
        }
        if let Geometry::Continuous(g) = geometry {
            g.markers_into(
                &fractions[..thresholds.len()],
                spacing.stroke,
                &mut result.marks,
            )
            .map_err(map_error)?;
        }
        y = track.min[1] + track.size[1] + spacing.compact_gap;
        for (i, m) in thresholds.iter().enumerate() {
            result.mark_labels[i] = text_rect(
                bounds,
                &mut y,
                m.label,
                theme.types().label,
                spacing.compact_gap,
            )?;
        }
        result.validate_paint()?;
        Ok(result)
    }
    pub fn bounds(&self) -> UiRect {
        self.bounds
    }
    pub fn copy(&self) -> MeterCopy<'a> {
        self.copy
    }
    pub fn regions(&self) -> MeterRegions {
        self.regions
    }
    pub fn continuous(&self) -> Option<MeterGeometry> {
        if let Geometry::Continuous(g) = self.geometry {
            Some(g)
        } else {
            None
        }
    }
    pub fn segment_geometry(&self) -> Option<SegmentGeometry> {
        if let Geometry::Segments(g) = self.geometry {
            Some(g)
        } else {
            None
        }
    }
    pub fn cells(&self) -> &[SegmentRect] {
        &self.cells[..self.count]
    }
    pub fn marker_rect(&self, index: usize) -> Option<UiRect> {
        self.marks
            .get(index)
            .copied()
            .filter(|_| index < self.thresholds.len())
    }
    pub fn marker_label_rect(&self, index: usize) -> Option<UiRect> {
        self.mark_labels
            .get(index)
            .copied()
            .filter(|_| index < self.thresholds.len())
    }
    /// Atomic standalone submission using the existing Ui transaction/inspection
    /// rollback. Lifecycle, input and paint remain explicit caller operations.
    pub fn draw(&self, ui: &mut Ui) -> Result<(), UiError> {
        ui.compose(|g| self.paint(g))
    }
    /// Atomic nested submission. Even if a later text/vertex/command/clip budget
    /// fails, every command/inspection entry in this meter disappears. Propagate
    /// errors with `?` to reject an enclosing card, or handle for a local fallback.
    pub fn submit(&self, group: &mut UiComposition<'_>) -> Result<(), UiError> {
        group.compose(|g| self.paint(g))
    }
    fn paint(&self, g: &mut UiComposition<'_>) -> Result<(), UiError> {
        g.push_clip(self.bounds)?;
        let t = self.theme;
        for (rect, text, size, color) in [
            (
                self.regions.label,
                self.copy.label,
                t.types().label,
                ColorRole::MutedText,
            ),
            (
                self.regions.display,
                self.copy.display,
                t.types().body,
                ColorRole::Text,
            ),
            (
                self.regions.status,
                self.copy.status,
                t.types().body,
                ColorRole::Text,
            ),
        ] {
            g.wrapped_text(
                rect,
                text,
                text_style(size, t.color(color)),
                LineLimit::Unlimited,
            )?;
        }
        g.panel(self.regions.track, t.color(ColorRole::Border))?;
        g.panel(self.well, t.color(ColorRole::Canvas))?;
        match self.geometry {
            Geometry::Continuous(geometry) => match geometry.value() {
                MeterValue::Known { .. } => {
                    let fill = geometry.fill().expect("known validated value");
                    if fill.size[0] > 0. {
                        self.paint_fill(g, fill)?;
                    }
                }
                MeterValue::Empty => self.paint_empty(g, self.well)?,
                MeterValue::Unknown => self.paint_unknown(g, self.well)?,
            },
            Geometry::Segments(geometry) => match geometry.value() {
                SegmentValue::Known { .. } => {
                    for cell in self.cells() {
                        g.panel(cell.bounds, t.color(ColorRole::Border))?;
                        let inner = cell.bounds.inset([t.spacing().stroke; 4])?;
                        g.panel(inner, t.color(ColorRole::Canvas))?;
                        if cell.filled {
                            self.paint_fill(g, inner)?;
                        }
                    }
                }
                SegmentValue::Empty => self.paint_empty(g, self.well)?,
                SegmentValue::Unknown { .. } => {
                    if self.cells().is_empty() {
                        self.paint_unknown(g, self.well)?;
                    }
                    for cell in self.cells() {
                        self.paint_unknown(g, cell.bounds)?;
                    }
                }
            },
        }
        for (i, marker) in self.thresholds.iter().enumerate() {
            g.panel(self.marks[i], t.color(ColorRole::Text))?;
            g.wrapped_text(
                self.mark_labels[i],
                marker.label,
                text_style(t.types().label, t.color(ColorRole::MutedText)),
                LineLimit::Unlimited,
            )?;
        }
        g.pop_clip()?;
        Ok(())
    }

    fn validate_paint(&self) -> Result<(), UiError> {
        match self.geometry {
            Geometry::Continuous(g) => match g.value() {
                MeterValue::Known { .. } => {
                    let r = g.fill().expect("known validated value");
                    if r.size[0] > 0. {
                        self.fill_dots(r)?;
                    }
                }
                MeterValue::Empty => {
                    self.empty_mark(self.well)?;
                }
                MeterValue::Unknown => {
                    self.unknown_marks(self.well)?;
                }
            },
            Geometry::Segments(g) => match g.value() {
                SegmentValue::Known { .. } => {
                    for c in self.cells() {
                        let inner = c.bounds.inset([self.theme.spacing().stroke; 4])?;
                        check(inner)?;
                        if c.filled {
                            self.fill_dots(inner)?;
                        }
                    }
                }
                SegmentValue::Empty => {
                    self.empty_mark(self.well)?;
                }
                SegmentValue::Unknown { .. } => {
                    self.unknown_marks(self.well)?;
                    for c in self.cells() {
                        self.unknown_marks(c.bounds)?;
                    }
                }
            },
        }
        Ok(())
    }
    fn fill_dots(&self, r: UiRect) -> Result<[UiRect; 3], UiError> {
        let size = self
            .theme
            .spacing()
            .stroke
            .min(r.size[0] / 6.)
            .min(r.size[1] / 3.);
        let dots = [0.2, 0.5, 0.8].map(|fraction| UiRect {
            min: [
                r.min[0] + (r.size[0] - size) * fraction,
                r.min[1] + (r.size[1] - size) * 0.5,
            ],
            size: [size; 2],
        });
        for dot in dots {
            contain(r, dot)?;
        }
        Ok(dots)
    }
    fn empty_mark(&self, r: UiRect) -> Result<UiRect, UiError> {
        let mark = UiRect {
            min: [
                r.min[0] + r.size[0] * 0.25,
                r.min[1] + (r.size[1] - self.theme.spacing().stroke) * 0.5,
            ],
            size: [r.size[0] * 0.5, self.theme.spacing().stroke],
        };
        contain(r, mark)?;
        Ok(mark)
    }
    fn unknown_marks(&self, r: UiRect) -> Result<[UiRect; 3], UiError> {
        let width = self.theme.spacing().stroke.min(r.size[0] / 6.);
        let marks = [0.2, 0.5, 0.8].map(|fraction| UiRect {
            min: [r.min[0] + (r.size[0] - width) * fraction, r.min[1]],
            size: [width, r.size[1]],
        });
        for mark in marks {
            contain(r, mark)?;
        }
        Ok(marks)
    }
    fn paint_fill(&self, g: &mut UiComposition<'_>, r: UiRect) -> Result<(), UiError> {
        g.panel(r, self.theme.color(ColorRole::Accent))?;
        for dot in self.fill_dots(r)? {
            g.panel(dot, self.theme.color(ColorRole::Canvas))?;
        }
        Ok(())
    }
    fn paint_empty(&self, g: &mut UiComposition<'_>, r: UiRect) -> Result<(), UiError> {
        g.panel(self.empty_mark(r)?, self.theme.color(ColorRole::Border))
    }
    fn paint_unknown(&self, g: &mut UiComposition<'_>, r: UiRect) -> Result<(), UiError> {
        for mark in self.unknown_marks(r)? {
            g.panel(mark, self.theme.color(ColorRole::Border))?;
        }
        Ok(())
    }
}
fn text_style(size: f32, color: crate::Color) -> TextStyle {
    TextStyle {
        size,
        color,
        align: Align::Left,
        wrap: true,
        shadow: false,
    }
}
fn text_rect(
    bounds: UiRect,
    y: &mut f32,
    text: &str,
    size: f32,
    gap: f32,
) -> Result<UiRect, UiError> {
    let m = WrappedText::new(text, bounds.size[0], size)?.measure();
    if m.width > bounds.size[0] || m.line_count == 0 {
        return Err(UiError::InvalidLayout);
    }
    let r = UiRect {
        min: [bounds.min[0], *y],
        size: [bounds.size[0], m.required_height],
    };
    contain(bounds, r)?;
    *y = r.min[1] + r.size[1] + gap;
    if !y.is_finite() {
        return Err(UiError::InvalidLayout);
    }
    Ok(r)
}
fn check(r: UiRect) -> Result<(), UiError> {
    if !r.valid() || (0..2).any(|i| r.size[i] <= 0. || r.min[i] + r.size[i] <= r.min[i]) {
        Err(UiError::InvalidLayout)
    } else {
        Ok(())
    }
}
fn contain(parent: UiRect, r: UiRect) -> Result<(), UiError> {
    check(r)?;
    if (0..2)
        .any(|i| r.min[i] < parent.min[i] || r.min[i] + r.size[i] > parent.min[i] + parent.size[i])
    {
        Err(UiError::InvalidLayout)
    } else {
        Ok(())
    }
}
fn map_error(e: MeterError) -> UiError {
    match e {
        MeterError::Capacity => UiError::Capacity,
        _ => UiError::InvalidLayout,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::host_frame;
    use ferravex_play::ui::theme::ThemeMode;

    const BOUNDS: UiRect = UiRect {
        min: [16., 12.],
        size: [320., 500.],
    };
    const COPY: MeterCopy<'static> = MeterCopy {
        label: "BOOST",
        display: "ENERGY 050%",
        status: "COOLING",
    };
    fn theme() -> UiTheme {
        UiTheme::preset(ThemeMode::Normal)
    }
    fn viewport() -> super::super::Viewport {
        super::super::Viewport::responsive([352., 600.], [0.; 4]).unwrap()
    }
    fn color(v: &[f32; 8]) -> crate::Color {
        crate::Color::new(v[4], v[5], v[6], v[7])
    }
    fn point(v: &[f32; 8]) -> [f32; 2] {
        [(v[0] + 1.) * 176., (1. - v[1]) * 300.]
    }
    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }

    #[test]
    fn continuous_emitted_fill_and_unknown_empty_patterns_match_reserved_geometry() {
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        for value in [
            MeterValue::Known {
                current: 0.,
                capacity: 6.,
            },
            MeterValue::Known {
                current: 3.,
                capacity: 6.,
            },
            MeterValue::Known {
                current: 6.,
                capacity: 6.,
            },
            MeterValue::Empty,
            MeterValue::Unknown,
        ] {
            let layout = MeterLayout::meter(BOUNDS, COPY, value, &[], theme()).unwrap();
            host_frame(0., 352, 600, || {
                ui.begin(viewport());
                layout.draw(&mut ui).unwrap();
                ui.finish(true);
                ui.paint();
                crate::read(|s| {
                    assert!(!s.vertices.is_empty());
                    for v in &s.vertices {
                        let p = point(v);
                        assert!(
                            p[0] >= BOUNDS.min[0] - 0.0001
                                && p[0] <= BOUNDS.min[0] + BOUNDS.size[0] + 0.0001
                        );
                        assert!(
                            p[1] >= BOUNDS.min[1] - 0.0001
                                && p[1] <= BOUNDS.min[1] + BOUNDS.size[1] + 0.0001
                        );
                    }
                    let fill_vertices: Vec<_> = s
                        .vertices
                        .iter()
                        .filter(|v| color(v) == theme().color(ColorRole::Accent))
                        .collect();
                    if let MeterValue::Known { current, .. } = value {
                        if current == 0. {
                            assert!(fill_vertices.is_empty());
                        } else {
                            assert_eq!(fill_vertices.len(), 6);
                            let fill = layout.continuous().unwrap().fill().unwrap();
                            for axis in 0..2 {
                                let lo = fill_vertices
                                    .iter()
                                    .map(|v| point(v)[axis])
                                    .fold(f32::INFINITY, f32::min);
                                let hi = fill_vertices
                                    .iter()
                                    .map(|v| point(v)[axis])
                                    .fold(f32::NEG_INFINITY, f32::max);
                                close(lo, fill.min[axis]);
                                close(hi, fill.min[axis] + fill.size[axis]);
                            }
                        }
                    } else {
                        assert!(fill_vertices.is_empty());
                    }
                    // The retained actual border-color geometry distinguishes no
                    // capacity (one dash), unknown (three bars) and known zero.
                    let marks = s
                        .vertices
                        .iter()
                        .filter(|v| color(v) == theme().color(ColorRole::Border))
                        .count();
                    assert_eq!(
                        marks,
                        match value {
                            MeterValue::Empty => 12,
                            MeterValue::Unknown => 24,
                            _ => 6,
                        }
                    );
                });
                for e in ui.inspect().unwrap() {
                    assert!(!e.interaction_registered);
                    if let Some(r) = e.visible_bounds {
                        contain(BOUNDS, r).unwrap();
                    }
                }
            });
        }
    }

    #[test]
    fn emitted_segment_fills_thresholds_and_text_use_inspected_rectangles() {
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        for filled in [0, 2, 6] {
            let layout = MeterLayout::segments(
                BOUNDS,
                COPY,
                SegmentValue::Known {
                    filled,
                    capacity: 6,
                },
                theme(),
            )
            .unwrap();
            host_frame(0., 352, 600, || {
                ui.begin(viewport());
                layout.draw(&mut ui).unwrap();
                ui.paint();
                crate::read(|s| {
                    let fills: Vec<_> = s
                        .vertices
                        .iter()
                        .filter(|v| color(v) == theme().color(ColorRole::Accent))
                        .collect();
                    assert_eq!(fills.len(), filled * 6);
                    for (cell, vertices) in
                        layout.cells()[..filled].iter().zip(fills.chunks_exact(6))
                    {
                        let r = cell.bounds.inset([theme().spacing().stroke; 4]).unwrap();
                        let lo = vertices
                            .iter()
                            .map(|v| point(v)[0])
                            .fold(f32::INFINITY, f32::min);
                        let hi = vertices
                            .iter()
                            .map(|v| point(v)[0])
                            .fold(f32::NEG_INFINITY, f32::max);
                        close(lo, r.min[0]);
                        close(hi, r.min[0] + r.size[0]);
                    }
                });
            });
        }
        let thresholds = [
            Threshold {
                fraction: 0.,
                label: "ZERO",
            },
            Threshold {
                fraction: 0.25,
                label: "REARM >25%",
            },
            Threshold {
                fraction: 1.,
                label: "FULL",
            },
        ];
        let layout = MeterLayout::meter(
            BOUNDS,
            COPY,
            MeterValue::Known {
                current: 0.5,
                capacity: 1.,
            },
            &thresholds,
            theme(),
        )
        .unwrap();
        host_frame(0., 352, 600, || {
            ui.begin(viewport());
            layout.draw(&mut ui).unwrap();
            ui.paint();
            let inspected: Vec<_> = ui.inspect().unwrap().collect();
            for (i, threshold) in thresholds.iter().enumerate() {
                let text = inspected
                    .iter()
                    .find(|e| e.painted_label == threshold.label)
                    .unwrap();
                assert_eq!(text.bounds, layout.marker_label_rect(i).unwrap());
                let mark = layout.marker_rect(i).unwrap();
                contain(layout.regions().track, mark).unwrap();
                assert!(inspected.iter().any(|e| e.bounds == mark));
                assert!(
                    text.bounds.min[1]
                        >= layout.regions().track.min[1] + layout.regions().track.size[1]
                );
            }
            assert_eq!(layout.marker_rect(3), None);
            assert_eq!(layout.marker_label_rect(usize::MAX), None);
        });
    }

    #[test]
    fn rejected_meter_keeps_prior_emitted_paint_identical() {
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        let layout = MeterLayout::meter(BOUNDS, COPY, MeterValue::Empty, &[], theme()).unwrap();
        let text = "A".repeat(1540); // 194040 estimated vertices, below 196608.
        host_frame(0., 352, 600, || {
            ui.begin(viewport());
            ui.wrapped_text(BOUNDS, &text, TextStyle::default(), LineLimit::Unlimited)
                .unwrap();
            ui.paint();
            let before = crate::read(|s| s.vertices.clone());
            crate::edit(|s| {
                s.vertices.clear();
                s.batches.clear();
            });
            assert_eq!(layout.draw(&mut ui), Err(UiError::Capacity));
            assert_eq!(ui.inspect().unwrap().len(), 1);
            ui.paint();
            crate::read(|s| assert_eq!(s.vertices, before));
        });
    }
}

#[cfg(test)]
mod segment_state_tests {
    use super::*;
    use crate::input::host_frame;
    use ferravex_play::ui::theme::ThemeMode;

    #[test]
    fn maximum_and_unknown_segment_paint_retains_all_cells_and_no_hit_regions() {
        let theme = UiTheme::preset(ThemeMode::HighContrast);
        let bounds = UiRect {
            min: [16., 12.],
            size: [4096., 600.],
        };
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        for value in [
            SegmentValue::Known {
                filled: 64,
                capacity: 64,
            },
            SegmentValue::Empty,
            SegmentValue::Unknown { capacity: 0 },
            SegmentValue::Unknown { capacity: 6 },
        ] {
            let layout = MeterLayout::segments(
                bounds,
                MeterCopy {
                    label: "ROUTE",
                    display: "EXACT AUTHORED COPY",
                    status: "CALLER STATUS",
                },
                value,
                theme,
            )
            .unwrap();
            host_frame(0., 4200, 700, || {
                ui.begin(super::super::Viewport::responsive([4200., 700.], [0.; 4]).unwrap());
                layout.draw(&mut ui).unwrap();
                ui.finish(true);
                assert!(ui.inspect().unwrap().all(|e| !e.interaction_registered));
                ui.paint();
                crate::read(|s| {
                    let count = s
                        .vertices
                        .iter()
                        .filter(|v| {
                            crate::Color::new(v[4], v[5], v[6], v[7])
                                == theme.color(ColorRole::Accent)
                        })
                        .count();
                    assert_eq!(
                        count,
                        if matches!(value, SegmentValue::Known { .. }) {
                            64 * 6
                        } else {
                            0
                        }
                    );
                    assert!(!s.vertices.is_empty());
                });
                assert_eq!(layout.cells().len(), value.count().unwrap());
            });
        }
    }
}
