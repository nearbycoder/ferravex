//! Borrowed menu/result cards in logical UI units, using the existing modal and
//! atomic composition. Pass bounds inside a responsive viewport's safe area;
//! coordinates start at zero inside that safe area. Absolute task271 rectangles
//! use a zero-inset responsive viewport. Fitted viewports scale logical units
//! and are a caller choice. Fonts never
//! shrink: title 24, body/stats/actions 18, padding 16, section gaps 12,
//! action gaps 8 and action height 44.
//!
//! Actions keep caller IDs/order/availability. The primary ID must be present,
//! including when disabled; start/resume/retry and authoritative progression
//! remain game-owned. `finish_player`/`finish` and paint happen after submission.
//! A returned action can be queued until an actual simulation tick; cards never
//! sample gameplay input or own a lifecycle. Controls-open gating stays in Ui.
//!
//! Without scrolling, only a complete prefix of sections is submitted. Overflow
//! paints `CONTENT OMITTED` and reports omitted sections/stat rows. To expose
//! all copy, reuse task274's pure [`ScrollState`]: set its extents from the layout,
//! feed accepted/remapped scroll commands outside composition, then submit it.
//! No new pointer/keyboard routing is installed. Do not enable a `ScrollRegion`
//! alongside another UI input owner; its documented routing limits still apply.
//! Scroll translation retains full line rectangles under the content/ancestor
//! clips. Only intersecting lines are queued, so offscreen copy spends no UI
//! vertex/text budget. Inspection describes those actual line commands, retaining
//! UTF-8; bitmap glyph substitution/word wrapping follow [`WrappedText`].
//!
//! This executed native/headless recipe composes actual start, pause and results
//! cards with one/three actions and a disabled action at 320x480 and wide. It
//! exercises retained paint/inspection, not a GPU, browser or device playtest.
//!
//! ```
//! use ferravex_render::ui::{Ui, UiError, UiLabel, Viewport};
//! use ferravex_render::ui::cards::{CardAction, CardContent, CardLayout, CardSection};
//! use ferravex_render::ui::stats::StatRow;
//! use ferravex_render::input::host_frame;
//! let mut ui = Ui::new();
//! ui.set_inspection_enabled(true);
//! let rows = [StatRow { label: "TIME", value: "1:00.001", description: None }];
//! let sections = [CardSection::Body("Collect crystals. Return to the entrance."),
//!                 CardSection::Stats(&rows)];
//! for width in [320., 960.] {
//!     for (title, primary, three) in [("CRYSTAL CAVERN", "START", false),
//!                                    ("PAUSED", "RESUME", true),
//!                                    ("EXPEDITION COMPLETE", "RETRY", true)] {
//!         let actions = [CardAction { id: 901, label: primary, enabled: true },
//!                        CardAction { id: 902, label: "CONTROLS", enabled: true },
//!                        CardAction { id: 903, label: "NEXT", enabled: false }];
//!         let viewport = Viewport::responsive([width, 480.], [8., 12., 8., 12.])?;
//!         let layout = CardLayout::new(viewport.bounds(), CardContent {
//!             title, sections: &sections, actions: &actions[..if three { 3 } else { 1 }],
//!             primary: 901, blocker: 900,
//!         })?;
//!         host_frame(0., width as u32, 480, || -> Result<(), UiError> {
//!             ui.begin(viewport);
//!             let report = ui.card(&layout, None)?;
//!             assert_eq!(report.omitted_sections, 0);
//!             ui.finish(true); // No simulation tick is required to render a menu.
//!             for action in &actions[..if three { 3 } else { 1 }] {
//!                 let element = ui.inspect().unwrap().find(|e| e.id == Some(action.id)).unwrap();
//!                 assert_eq!(element.label, UiLabel::Full(action.label));
//!                 assert_eq!(element.bounds, layout.action_rect(action.id).unwrap());
//!                 assert_eq!(element.visible_bounds, Some(element.bounds));
//!                 assert_eq!(element.enabled, action.enabled);
//!             }
//!             assert!(!ui.activated(999));
//!             ui.paint();
//!             Ok(())
//!         })?;
//!     }
//! }
//! # Ok::<(), UiError>(())
//! ```
//!
//! Scrolling and nested atomic submission use the same layout. Update scroll
//! state outside the group; no wheel, pointer, gameplay edge or clock is read.
//!
//! ```
//! use ferravex_play::ui::scroll::ScrollState;
//! use ferravex_render::ui::{Ui, UiError, Viewport};
//! use ferravex_render::ui::cards::{CardAction, CardContent, CardLayout, CardSection};
//! let mut ui = Ui::new();
//! let viewport = Viewport::responsive([320., 480.], [0.; 4])?;
//! let copy = "Long briefing. Read before starting.\n".repeat(80);
//! let sections = [CardSection::Body(&copy)];
//! let actions = [CardAction { id: 1, label: "START", enabled: true }];
//! let layout = CardLayout::new(viewport.bounds(), CardContent {
//!     title: "BRIEFING", sections: &sections, actions: &actions, primary: 1, blocker: 2,
//! })?;
//! let mut scroll = ScrollState::new(layout.content_bounds().size[1], layout.content_extent())?;
//! scroll.page(false); // Feed an accepted remapped menu edge here.
//! ui.begin(viewport);
//! let report = ui.compose(|group| layout.submit(group, Some(&scroll)))?;
//! assert!(report.scrollable);
//! assert_eq!(report.omitted_sections, 0); // All source remains reachable.
//! assert_eq!(layout.action_rect(99), None);
//! ui.paint();
//! scroll.reset(); // On a new screen/session, caller decides reset semantics.
//! assert_eq!(scroll.offset(), 0.);
//! # Ok::<(), UiError>(())
//! ```

use super::stats::{MAX_STAT_ROWS, StatRow, StatRowsLayout, StatRowsStyle};
use super::text_layout::{LineLimit, WrappedText};
use super::{ButtonStyle, PANEL, TextStyle, Ui, UiComposition, UiError, UiRect, WidgetId};
use ferravex_play::ui::{
    flow::{self, FlowItem, FlowOptions},
    scroll::ScrollState,
};

/// Sections per card; checked before scanning source.
pub const MAX_CARD_SECTIONS: usize = 8;
/// Actions per card; all must fit, including disabled actions.
pub const MAX_CARD_ACTIONS: usize = 8;
/// Combined UTF-8 bytes in title, sections/stat fields and action labels.
/// Checked before measurement; stat rows are additionally capped at 64 total.
pub const MAX_CARD_BYTES: usize = 8192;
const PAD: f32 = 16.;
const GAP: f32 = 12.;
const TITLE: f32 = 24.;
const TEXT: f32 = 18.;
const ACTION: f32 = 44.;
// A bounded source cannot approach this height, even at one scalar per line.
const MEASURE_HEIGHT: f32 = 1_048_576.;
const EMPTY: UiRect = UiRect {
    min: [0.; 2],
    size: [0.; 2],
};

/// Borrowed full sections. A non-scrolling overflow omits the entire section,
/// including every value of a stats section, rather than cropping an essential
/// value. Use scrolling when that omission is unsuitable for the game.
#[derive(Clone, Copy, Debug)]
pub enum CardSection<'a> {
    Body(&'a str),
    Stats(&'a [StatRow<'a>]),
}
/// Stable caller identity and full single-line label; labels are never ellipsized.
#[derive(Clone, Copy, Debug)]
pub struct CardAction<'a> {
    pub id: WidgetId,
    pub label: &'a str,
    pub enabled: bool,
}
/// Borrowed content, not retained state. IDs must be unique, including blocker.
#[derive(Clone, Copy, Debug)]
pub struct CardContent<'a> {
    pub title: &'a str,
    pub sections: &'a [CardSection<'a>],
    pub actions: &'a [CardAction<'a>],
    pub primary: WidgetId,
    pub blocker: WidgetId,
}
/// Layout omission, independent of ancestor clips, occlusion and physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardReport {
    pub omitted_sections: usize,
    pub omitted_stat_rows: usize,
    /// Content exceeds its viewport and a matching scroll state was supplied.
    pub scrollable: bool,
}
/// Immutable fixed-storage measured card. Rebuild on copy/bounds changes; no
/// cache, allocation, input consumption or external mutation occurs in layout.
#[derive(Debug)]
pub struct CardLayout<'a> {
    source: CardContent<'a>,
    bounds: UiRect,
    title: UiRect,
    content: UiRect,
    notice: Option<UiRect>,
    actions: [UiRect; MAX_CARD_ACTIONS],
    sections: [UiRect; MAX_CARD_SECTIONS],
    extent: f32,
}
impl<'a> CardLayout<'a> {
    /// Validate/measure before insertion. `Capacity` means a section/action/byte/
    /// total-stat-row bound was exceeded. `DuplicateId` means colliding IDs.
    /// `InvalidLayout` includes a missing primary, blank title/action, multiline
    /// action, impossible bounds or unrepresentable geometry. All actions and the
    /// full title must fit at fixed sizes; insufficient space never inserts a
    /// partial card. Unknown action queries return `None`. Existing UI budgets
    /// are checked atomically by submission, not reserved by this pure layout.
    pub fn new(bounds: UiRect, source: CardContent<'a>) -> Result<Self, UiError> {
        validate_source(source)?;
        if !bounds.valid() || bounds.size.iter().any(|v| *v < 2. * PAD) {
            return Err(UiError::InvalidLayout);
        }
        let inside = bounds.inset([PAD; 4])?;
        let width = inside.size[0];
        if width < TITLE * 0.75 {
            return Err(UiError::InvalidLayout);
        }
        let title_height = measured(source.title, width, TITLE)?.0;
        let mut items = [FlowItem::fixed(0.); MAX_CARD_ACTIONS];
        for (item, action) in items.iter_mut().zip(source.actions) {
            let minimum = (measured(action.label, f32::MAX, TEXT)?.1 + 32.).max(ACTION);
            if minimum > width {
                return Err(UiError::InvalidLayout);
            }
            *item = FlowItem {
                minimum,
                preferred: minimum,
                maximum: width,
                grow: 1.,
            };
        }
        let mut actions = [EMPTY; MAX_CARD_ACTIONS];
        let report = flow::layout(
            UiRect {
                min: [0.; 2],
                size: [width, MEASURE_HEIGHT],
            },
            &items[..source.actions.len()],
            FlowOptions {
                row_height: ACTION,
                ..FlowOptions::default()
            },
            &mut actions[..source.actions.len()],
        )
        .map_err(|_| UiError::InvalidLayout)?;
        if report.overflow != 0 {
            return Err(UiError::InvalidLayout);
        }
        let action_height = report.rows as f32 * ACTION + report.rows.saturating_sub(1) as f32 * 8.;
        let action_top = inside.min[1] + inside.size[1] - action_height;
        let title = UiRect {
            min: inside.min,
            size: [width, title_height],
        };
        let content_top = title.min[1] + title.size[1] + GAP;
        let available = action_top - GAP - content_top;
        if available < 0. {
            return Err(UiError::InvalidLayout);
        }
        for rect in &mut actions[..source.actions.len()] {
            rect.min[0] += inside.min[0];
            rect.min[1] += action_top;
            // Canonical endpoint extents make intersection with an enclosing
            // clip idempotent, including fractional flow widths/origins.
            for axis in 0..2 {
                rect.size[axis] = (rect.min[axis] + rect.size[axis]) - rect.min[axis];
            }
            contained(inside, *rect)?;
        }
        contained(inside, title)?;
        let mut sections = [EMPTY; MAX_CARD_SECTIONS];
        let mut extent = 0.;
        for (index, section) in source.sections.iter().enumerate() {
            let height = match section {
                CardSection::Body(copy) => measured(copy, width, TEXT)?.0,
                CardSection::Stats(rows) => stat_layout(width, rows)?.report().required_height,
            };
            let top = extent + if index == 0 { 0. } else { GAP };
            let rect = UiRect {
                min: [0., top],
                size: [width, height],
            };
            extent = top + height;
            if !rect.valid() || !extent.is_finite() || extent > MEASURE_HEIGHT {
                return Err(UiError::InvalidLayout);
            }
            sections[index] = rect;
        }
        let overflow = extent > available;
        if overflow && width < measured("CONTENT OMITTED", f32::MAX, TEXT)?.1 {
            return Err(UiError::InvalidLayout);
        }
        let content_height = available - if overflow { TEXT + GAP } else { 0. };
        if content_height < 0. {
            return Err(UiError::InvalidLayout);
        }
        let content = UiRect {
            min: [inside.min[0], content_top],
            size: [width, content_height],
        };
        contained(inside, content)?;
        let notice = overflow.then_some(UiRect {
            min: [inside.min[0], action_top - GAP - TEXT],
            size: [width, TEXT],
        });
        if let Some(rect) = notice {
            contained(inside, rect)?;
        }
        // Enforce disjoint f32 endpoints rather than trusting subtraction.
        if title.min[1] + title.size[1] > content.min[1]
            || content.min[1] + content.size[1] > notice.map_or(action_top, |r| r.min[1])
            || notice.is_some_and(|r| r.min[1] + r.size[1] > action_top)
        {
            return Err(UiError::InvalidLayout);
        }
        Ok(Self {
            source,
            bounds,
            title,
            content,
            notice,
            actions,
            sections,
            extent,
        })
    }
    /// Card panel in caller logical coordinates.
    pub fn bounds(&self) -> UiRect {
        self.bounds
    }
    /// Full fixed-size title rectangle.
    pub fn title_rect(&self) -> UiRect {
        self.title
    }
    /// Viewport for sections; feed its height to task274 `ScrollState`.
    pub fn content_bounds(&self) -> UiRect {
        self.content
    }
    /// Full content-space height; feed to `ScrollState::set_extents` on reflow.
    pub fn content_extent(&self) -> f32 {
        self.extent
    }
    /// Unscrolled section rectangle, relative to the content origin.
    pub fn section_rect(&self, index: usize) -> Option<UiRect> {
        (index < self.source.sections.len()).then(|| self.sections[index])
    }
    /// Exactly the rectangle passed to the painted button and its hit registry.
    pub fn action_rect(&self, id: WidgetId) -> Option<UiRect> {
        self.source
            .actions
            .iter()
            .position(|a| a.id == id)
            .map(|i| self.actions[i])
    }
    /// Atomically submit modal, panel, full title, content/notice and every action.
    /// All interaction, clip and inspection changes roll back through task281.
    /// Scroll must match both extents exactly and have positive viewport height;
    /// mismatch fails before insertion. Scroll is borrowed immutably: updating/
    /// resetting it is caller-owned and lies outside the composition transaction.
    /// Existing ancestor clips apply unchanged, including to action hits; callers
    /// must keep the action area inside their visible safe region.
    pub fn submit(
        &self,
        group: &mut UiComposition<'_>,
        scroll: Option<&ScrollState>,
    ) -> Result<CardReport, UiError> {
        group.compose(|group| self.submit_commands(group, scroll))
    }
    fn submit_commands(
        &self,
        group: &mut UiComposition<'_>,
        scroll: Option<&ScrollState>,
    ) -> Result<CardReport, UiError> {
        if scroll.is_some_and(|s| {
            s.viewport_extent() != self.content.size[1]
                || s.content_extent() != self.extent
                || self.content.size[1] == 0.
        }) {
            return Err(UiError::InvalidLayout);
        }
        let top = ScrollState::new(self.content.size[1], self.extent)?;
        let state = scroll.unwrap_or(&top);
        let mut report = CardReport {
            scrollable: scroll.is_some() && self.extent > self.content.size[1],
            ..CardReport::default()
        };
        group.modal(self.source.blocker)?;
        group.panel(self.bounds, PANEL)?;
        group.wrapped_text(
            self.title,
            self.source.title,
            title_style(),
            LineLimit::Unlimited,
        )?;
        group.push_clip(self.content)?;
        let mut stopped = false;
        for (index, section) in self.source.sections.iter().enumerate() {
            let local = self.sections[index];
            if scroll.is_none() && (stopped || local.min[1] + local.size[1] > self.content.size[1])
            {
                stopped = true;
                report.omitted_sections += 1;
                if let CardSection::Stats(rows) = section {
                    report.omitted_stat_rows += rows.len();
                }
                continue;
            }
            match section {
                CardSection::Body(copy) => {
                    self.paint_block(group, state, local, copy, TextStyle::default())?
                }
                CardSection::Stats(rows) => {
                    let stats = stat_layout(self.content.size[0], rows)?;
                    let style = StatRowsStyle::default();
                    for (row, geometry) in rows.iter().zip(stats.rows()) {
                        for (copy, mut rect, style) in [
                            Some((row.label, geometry.label, style.label)),
                            Some((row.value, geometry.value, style.value)),
                            row.description
                                .zip(geometry.description)
                                .map(|(copy, r)| (copy, r, style.description)),
                        ]
                        .into_iter()
                        .flatten()
                        {
                            rect.min[1] += local.min[1];
                            self.paint_block(group, state, rect, copy, style)?;
                        }
                    }
                }
            }
        }
        group.pop_clip()?;
        if let Some(rect) = self.notice {
            group.wrapped_text(
                rect,
                if scroll.is_some() {
                    "SCROLL FOR MORE"
                } else {
                    "CONTENT OMITTED"
                },
                TextStyle::default(),
                LineLimit::Unlimited,
            )?;
        }
        for (action, rect) in self.source.actions.iter().zip(self.actions) {
            // button, not legacy scaled text or an ellipsized HUD label. Layout
            // already proved the full label fits at size 18 with 32-unit padding.
            group.button(
                action.id,
                rect,
                action.label,
                action.enabled,
                ButtonStyle::default(),
            )?;
        }
        Ok(report)
    }
    fn paint_block(
        &self,
        group: &mut UiComposition<'_>,
        state: &ScrollState,
        rect: UiRect,
        copy: &str,
        style: TextStyle,
    ) -> Result<(), UiError> {
        let translated = state.translate_rect(self.content, rect)?;
        let wrapped = WrappedText::new(copy, rect.size[0], style.size)?;
        for (index, line) in wrapped.lines().enumerate() {
            let line_rect = UiRect {
                min: [
                    translated.min[0],
                    translated.min[1] + index as f32 * style.size,
                ],
                size: [rect.size[0], style.size],
            };
            if line_rect.intersect(self.content).is_some() {
                // Full logical line rectangle is preserved. The clip, not a
                // cropped rectangle, determines partial scroll visibility.
                group.wrapped_text(line_rect, line.text, style, LineLimit::Unlimited)?;
            }
        }
        Ok(())
    }
}
impl Ui {
    /// Standalone atomic card submission. See the module's executed recipe;
    /// use [`CardLayout::submit`] inside a larger atomic composition.
    pub fn card(
        &mut self,
        layout: &CardLayout<'_>,
        scroll: Option<&ScrollState>,
    ) -> Result<CardReport, UiError> {
        self.compose(|group| layout.submit_commands(group, scroll))
    }
}
fn title_style() -> TextStyle {
    TextStyle {
        size: TITLE,
        ..TextStyle::default()
    }
}
fn measured(copy: &str, width: f32, size: f32) -> Result<(f32, f32), UiError> {
    let m = WrappedText::new(copy, width, size)?.measure();
    Ok((m.height.max(size), m.width))
}
fn stat_layout<'a>(width: f32, rows: &'a [StatRow<'a>]) -> Result<StatRowsLayout<'a>, UiError> {
    StatRowsLayout::new(
        UiRect {
            min: [0.; 2],
            size: [width, MEASURE_HEIGHT],
        },
        rows,
        StatRowsStyle::default(),
    )
}
fn validate_source(source: CardContent<'_>) -> Result<(), UiError> {
    if source.sections.len() > MAX_CARD_SECTIONS || source.actions.len() > MAX_CARD_ACTIONS {
        return Err(UiError::Capacity);
    }
    let mut bytes = source.title.len();
    let mut rows_count: usize = 0;
    let mut add = |copy: &str| -> Result<(), UiError> {
        bytes = bytes.checked_add(copy.len()).ok_or(UiError::Capacity)?;
        if bytes > MAX_CARD_BYTES {
            return Err(UiError::Capacity);
        }
        Ok(())
    };
    add("")?;
    for section in source.sections {
        match section {
            CardSection::Body(copy) => add(copy)?,
            CardSection::Stats(rows) => {
                rows_count = rows_count
                    .checked_add(rows.len())
                    .ok_or(UiError::Capacity)?;
                if rows_count > MAX_STAT_ROWS {
                    return Err(UiError::Capacity);
                }
                for row in *rows {
                    add(row.label)?;
                    add(row.value)?;
                    if let Some(d) = row.description {
                        add(d)?;
                    }
                }
            }
        }
    }
    for action in source.actions {
        add(action.label)?;
    }
    if source.title.trim().is_empty() || !source.actions.iter().any(|a| a.id == source.primary) {
        return Err(UiError::InvalidLayout);
    }
    for (i, action) in source.actions.iter().enumerate() {
        if action.id == source.blocker || source.actions[..i].iter().any(|a| a.id == action.id) {
            return Err(UiError::DuplicateId);
        }
        if action.label.trim().is_empty()
            || action.label.chars().any(char::is_control)
            || action.label.contains('\u{2028}')
            || action.label.contains('\u{2029}')
        {
            return Err(UiError::InvalidLayout);
        }
    }
    Ok(())
}
fn contained(bounds: UiRect, rect: UiRect) -> Result<(), UiError> {
    if !rect.valid()
        || (0..2).any(|a| {
            rect.min[a] < bounds.min[a]
                || rect.min[a] + rect.size[a] > bounds.min[a] + bounds.size[a]
                || (rect.size[a] > 0. && rect.min[a] + rect.size[a] <= rect.min[a])
        })
    {
        return Err(UiError::InvalidLayout);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{ActionKind, Binding, InputCode, PlayerInput, host_frame};

    const ACTIONS: [CardAction<'static>; 3] = [
        CardAction {
            id: 10,
            label: "RESUME",
            enabled: true,
        },
        CardAction {
            id: 20,
            label: "CONTROLS",
            enabled: false,
        },
        CardAction {
            id: 30,
            label: "RETRY",
            enabled: true,
        },
    ];
    fn reset() {
        crate::edit(|s| {
            *s = crate::State::default();
            s.w = 1000;
            s.h = 600;
        });
    }
    fn viewport() -> super::super::Viewport {
        super::super::Viewport::responsive([1000., 600.], [0.; 4]).unwrap()
    }
    fn card<'a>(sections: &'a [CardSection<'a>], actions: &'a [CardAction<'a>]) -> CardLayout<'a> {
        CardLayout::new(
            UiRect {
                min: [16.; 2],
                size: [600., 480.],
            },
            CardContent {
                title: "PAUSED",
                sections,
                actions,
                primary: 10,
                blocker: 9,
            },
        )
        .unwrap()
    }
    fn point(rect: UiRect) -> [f32; 2] {
        [rect.min[0] + 2., rect.min[1] + 2.]
    }
    fn mouse(point: [f32; 2], pressed: bool) {
        crate::pointer(point[0], point[1], 0., 0.);
        crate::button(1, pressed);
    }
    fn draw(ui: &mut Ui, card: &CardLayout<'_>) {
        ui.begin(viewport());
        ui.card(card, None).unwrap();
    }

    #[test]
    fn journey400_277_capture_release_controls_focus_disabled_and_ancestor_hit_geometry() {
        reset();
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        let layout = card(&[], &ACTIONS);
        let first = layout.action_rect(10).unwrap();
        let last = layout.action_rect(30).unwrap();
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            mouse(point(first), true);
            ui.finish(true);
            assert_eq!(ui.interaction().pointer_capture, Some(10));
            assert!(!ui.activated(10));
        });
        // Ordinary card reconstruction preserves the captured stable ID, but
        // releasing over a different painted action does not activate either.
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            mouse(point(last), false);
            ui.finish(true);
            assert!(!ui.activated(10) && !ui.activated(30));
        });
        for block in [false, true] {
            host_frame(0., 1000, 600, || {
                draw(&mut ui, &layout);
                mouse(point(first), true);
                ui.finish(true);
                assert_eq!(ui.interaction().pointer_capture, Some(10));
            });
            host_frame(0., 1000, 600, || {
                draw(&mut ui, &layout);
                crate::edit(|s| s.controls_open = block);
                mouse(point(first), false);
                ui.finish(true);
                assert_eq!(ui.activated(10), !block);
                assert_eq!(ui.interaction().input_blocked, block);
                crate::edit(|s| s.controls_open = false);
            });
            host_frame(0., 1000, 600, || {
                draw(&mut ui, &layout);
                ui.finish(true);
                assert!(!ui.activated(10), "a release activates only once");
            });
        }
        // Disabled primary eligibility is caller-owned and cannot be revived
        // by a held pointer after availability changes.
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            mouse(point(first), true);
            ui.finish(true);
        });
        let disabled = [
            CardAction {
                enabled: false,
                ..ACTIONS[0]
            },
            ACTIONS[1],
            ACTIONS[2],
        ];
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &card(&[], &disabled));
            mouse(point(first), false);
            ui.finish(true);
            assert!(!ui.activated(10));
        });
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            // Previous disabled-primary frame auto-focused RETRY. Two Tabs
            // traverse RESUME and then RETRY, skipping disabled CONTROLS.
            crate::key(0xff09, true);
            crate::key(0xff09, false);
            crate::key(0xff09, true);
            crate::key(0xff09, false);
            crate::key(0xff0d, true);
            crate::key(0xff0d, false);
            ui.finish(true);
            assert_eq!(ui.focused(), Some(30));
            assert!(ui.activated(30));
            assert!(!ui.activated(20));
        });
        // A smaller ancestor clips paint and hits at the identical half-open
        // edge; clicking the hidden half reaches only the existing modal.
        let clip = UiRect {
            min: first.min,
            size: [first.size[0] / 2., 44.],
        };
        for visible in [false, true] {
            host_frame(0., 1000, 600, || {
                ui.begin(viewport());
                ui.push_clip(clip).unwrap();
                ui.card(&layout, None).unwrap();
                ui.pop_clip();
                let p = if visible {
                    point(clip)
                } else {
                    [clip.min[0] + clip.size[0], clip.min[1] + 2.]
                };
                mouse(p, true);
                mouse(p, false);
                ui.finish(true);
                assert_eq!(ui.activated(10), visible);
                let actual = ui.inspect().unwrap().find(|e| e.id == Some(10)).unwrap();
                assert_eq!(actual.bounds, first);
                assert_eq!(actual.visible_bounds, first.intersect(clip));
                ui.paint();
            });
        }
        // Blur cancels an in-progress captured gesture.
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            mouse(point(first), true);
            ui.finish(true);
        });
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            crate::edit(|s| s.input.focus(false));
            ui.finish(true);
        });
        host_frame(0., 1000, 600, || {
            draw(&mut ui, &layout);
            crate::edit(|s| s.input.focus(true));
            mouse(point(first), false);
            ui.finish(true);
            assert!(!ui.activated(10));
        });
    }

    #[test]
    fn journey400_277_failed_modal_preserves_prior_capture_focus_and_emitted_paint() {
        reset();
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        let layout = card(&[CardSection::Body("COPY")], &ACTIONS);
        let prior = UiRect {
            min: [8.; 2],
            size: [120., 44.],
        };
        host_frame(0., 1000, 600, || {
            ui.begin(viewport());
            ui.button(1, prior, "PRIOR", true, ButtonStyle::default())
                .unwrap();
            mouse(point(prior), true);
            ui.finish(true);
            assert_eq!(ui.focused(), Some(1));
            assert_eq!(ui.interaction().pointer_capture, Some(1));
        });
        host_frame(0., 1000, 600, || {
            ui.begin(viewport());
            ui.button(1, prior, "PRIOR", true, ButtonStyle::default())
                .unwrap();
            ui.paint();
            let before = crate::read(|s| s.vertices.clone());
            let mut inspection = [0; 8192];
            let bytes = ui.write_inspection(&mut inspection).unwrap();
            assert_eq!(
                ui.compose(|g| {
                    layout.submit(g, None)?;
                    // A later duplicate primary rejects panel/title/body/actions and
                    // modal together; it cannot replace the prior capture.
                    g.button(10, prior, "DUPLICATE", true, ButtonStyle::default())
                }),
                Err(UiError::DuplicateId)
            );
            let mut after_inspection = [0; 8192];
            let after_bytes = ui.write_inspection(&mut after_inspection).unwrap();
            assert_eq!(bytes, after_bytes);
            assert_eq!(inspection[..bytes], after_inspection[..after_bytes]);
            crate::edit(|s| s.vertices.clear());
            ui.paint();
            assert_eq!(crate::read(|s| s.vertices.clone()), before);
            mouse(point(prior), false);
            ui.finish(true);
            assert!(ui.activated(1));
            assert!(!ui.activated(10) && !ui.activated(9));
        });
    }

    #[test]
    fn journey400_277_remapped_confirm_does_not_consume_zero_tick_gameplay_edges() {
        reset();
        let layout = card(&[], &ACTIONS);
        let mut ui = Ui::new();
        let mut player = PlayerInput::transient("journey400-card-ticks");
        player
            .global_action(
                "confirm",
                "Confirm",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(106))],
            )
            .unwrap();
        player
            .action(
                "jump",
                "Jump",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(107))],
            )
            .unwrap();
        host_frame(0., 1000, 600, || {
            player.sample(true);
            draw(&mut ui, &layout);
            ui.finish_player(&mut player, true);
        });
        host_frame(0., 1000, 600, || {
            crate::key(106, true);
            crate::key(106, false);
            crate::key(107, true);
            crate::key(107, false);
            player.sample(true);
            draw(&mut ui, &layout);
            ui.finish_player(&mut player, true);
            assert!(ui.activated(10));
            // Zero simulation ticks: leave jump's buffered edges untouched.
        });
        host_frame(0., 1000, 600, || {
            player.sample(true);
            draw(&mut ui, &layout);
            ui.finish_player(&mut player, true);
            assert!(!ui.activated(10));
            let first = player.take_tick(["jump"])[0];
            assert!(first.pressed && first.released);
            let second = player.take_tick(["jump"])[0];
            assert!(!second.pressed && !second.released);
        });
    }

    #[test]
    fn journey400_277_visible_line_paint_equals_full_wrapped_stat_composition() {
        let copy = "ALPHA BETA GAMMA\n\né日 Z\nLONGWORDSREMAINATFIXEDSIZE".repeat(8);
        let rows = [
            StatRow {
                label: "TOTAL\nACCEPTED",
                value: "18446744073709551615",
                description: Some("é日 RESULT\nMORE DETAILS"),
            },
            StatRow {
                label: "TIME",
                value: "1:00.001",
                description: None,
            },
        ];
        let sections = [CardSection::Body(&copy), CardSection::Stats(&rows)];
        for width in [288., 920.] {
            for offset in [0., 9.5, f32::MAX] {
                reset();
                let layout = CardLayout::new(
                    UiRect {
                        min: [12.25, 16.5],
                        size: [width, 480.],
                    },
                    CardContent {
                        title: "RESULT",
                        sections: &sections,
                        actions: &ACTIONS,
                        primary: 10,
                        blocker: 9,
                    },
                )
                .unwrap();
                let mut scroll = ScrollState::new(layout.content.size[1], layout.extent).unwrap();
                scroll.set_offset(offset).unwrap();
                let mut ui = Ui::new();
                ui.begin(viewport());
                ui.card(&layout, Some(&scroll)).unwrap();
                ui.paint();
                let actual = crate::read(|s| s.vertices.clone());
                crate::edit(|s| s.vertices.clear());
                ui.begin(viewport());
                ui.compose(|g| {
                    g.modal(9)?;
                    g.panel(layout.bounds, PANEL)?;
                    g.wrapped_text(layout.title, "RESULT", title_style(), LineLimit::Unlimited)?;
                    g.push_clip(layout.content)?;
                    for (i, section) in sections.iter().enumerate() {
                        let local = layout.sections[i];
                        match section {
                            CardSection::Body(copy) => {
                                g.wrapped_text(
                                    scroll.translate_rect(layout.content, local)?,
                                    copy,
                                    TextStyle::default(),
                                    LineLimit::Unlimited,
                                )?;
                            }
                            CardSection::Stats(rows) => {
                                for (row, geometry) in rows
                                    .iter()
                                    .zip(stat_layout(layout.content.size[0], rows)?.rows())
                                {
                                    for (copy, mut rect) in [
                                        Some((row.label, geometry.label)),
                                        Some((row.value, geometry.value)),
                                        row.description.zip(geometry.description),
                                    ]
                                    .into_iter()
                                    .flatten()
                                    {
                                        rect.min[1] += local.min[1];
                                        g.wrapped_text(
                                            scroll.translate_rect(layout.content, rect)?,
                                            copy,
                                            TextStyle::default(),
                                            LineLimit::Unlimited,
                                        )?;
                                    }
                                }
                            }
                        }
                    }
                    g.pop_clip()?;
                    if let Some(rect) = layout.notice {
                        g.wrapped_text(
                            rect,
                            "SCROLL FOR MORE",
                            TextStyle::default(),
                            LineLimit::Unlimited,
                        )?;
                    }
                    for (action, rect) in ACTIONS.iter().zip(layout.actions) {
                        g.button(
                            action.id,
                            rect,
                            action.label,
                            action.enabled,
                            ButtonStyle::default(),
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
                ui.paint();
                assert_eq!(
                    crate::read(|s| s.vertices.clone()),
                    actual,
                    "width={width} offset={offset}"
                );
            }
        }
    }
}
