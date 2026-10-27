//! Local immediate-mode lab labels. No shared layout or input ownership.
use ferravex_render::prelude::*;

pub(super) const NARROW_FOOTER_HEIGHT: f32 = 536.;
/// BALL, CRATE, DROP, PULSE, PAUSE, STEP, RESET, CHAOS, DOMINO.
pub(super) const BUTTONS: usize = 9;
/// Visual order groups make / toys / time: BALL CRATE DROP | PULSE CHAOS
/// DOMINO | PAUSE STEP RESET. Indexed by the lab's stable button index.
const SLOT: [usize; BUTTONS] = [0, 1, 2, 3, 6, 7, 8, 4, 5];
#[cfg(test)]
const ROW_HEIGHT: f32 = 20.;
#[cfg(test)]
const SLOT_LINES: [usize; 5] = [4, 3, 4, 5, 5];

#[derive(Clone, Copy, Debug)]
pub(super) struct LabLayout {
    pub narrow: bool,
    /// Play layout with grouped buttons; `new` keeps the legacy hosted grid.
    compact: bool,
    pub footer_height: f32,
    pub footer_y: f32,
    pub scale: f32,
    pub origin: Vec2,
    width: f32,
}

impl LabLayout {
    pub fn new(width: f32, height: f32) -> Self {
        let narrow = width < 640.;
        let footer_height = if narrow {
            NARROW_FOOTER_HEIGHT + if width < 360. { 44. } else { 0. }
        } else {
            166.
        };
        let footer_y = height - footer_height - 12.;
        let scale = ((width - 40.) / 26.)
            .min((footer_y - 119.).max(1.) / 14.)
            .max(0.1);
        Self {
            narrow,
            compact: false,
            footer_height,
            footer_y,
            scale,
            origin: vec2(width * 0.5, footer_y - 8. - scale),
            width,
        }
    }

    /// Compact play layout: keep diagnostics out of the scene, without shrinking text.
    pub fn compact(width: f32, height: f32) -> Self {
        let mut layout = Self::new(width, height);
        layout.compact = true;
        let rows = BUTTONS.div_ceil(Self::columns(width)) as f32;
        let lines = if Self::columns(width) == BUTTONS {
            2.
        } else {
            3.
        };
        layout.footer_height = 10. + rows * 44. + 14. + lines * 20.;
        layout.footer_y = height - layout.footer_height;
        layout.scale = ((width - 40.) / 26.)
            .min((layout.footer_y - 60.).max(1.) / 14.)
            .max(0.1);
        layout.origin = vec2(width * 0.5, layout.footer_y - 8. - layout.scale);
        layout
    }

    fn columns(width: f32) -> usize {
        if width >= 820. {
            BUTTONS
        } else if width >= 480. {
            5
        } else {
            3
        }
    }

    /// Whether every button fits one row (two footer text lines follow).
    pub fn single_row(&self) -> bool {
        Self::columns(self.width) == BUTTONS
    }

    /// Baseline of the first footer text line, below every button row.
    pub fn status_y(&self) -> f32 {
        let rows = if self.compact {
            BUTTONS.div_ceil(Self::columns(self.width))
        } else {
            1
        };
        self.footer_y + 10. + rows as f32 * 44. + 16.
    }

    /// Screen y of the scene's top grid line (13 m).
    pub fn scene_top(&self) -> f32 {
        self.to_screen(vec2(0., 13.)).y
    }

    /// The same layout viewed with a zoom about a world point plus a pixel
    /// offset. Drawing and pointer mapping stay consistent through it.
    pub fn zoomed(&self, center: Vec2, zoom: f32, offset: Vec2) -> Self {
        if !center.is_finite() || !zoom.is_finite() || zoom <= 0. || !offset.is_finite() {
            return *self;
        }
        let anchor = self.to_screen(center);
        let scale = self.scale * zoom;
        Self {
            scale,
            origin: anchor - vec2(center.x, -center.y) * scale + offset,
            ..*self
        }
    }

    pub fn to_screen(self, point: Vec2) -> Vec2 {
        self.origin + vec2(point.x, -point.y) * self.scale
    }

    pub fn to_world(self, point: Vec2) -> Vec2 {
        vec2(
            (point.x - self.origin.x) / self.scale,
            (self.origin.y - point.y) / self.scale,
        )
    }

    pub fn contains_scene(&self, point: Vec2) -> bool {
        point.x >= 16.
            && point.x <= self.width - 16.
            && point.y > 44.
            && point.y < self.footer_y - 8.
    }

    pub fn button(&self, index: usize) -> Rect {
        if self.compact {
            let columns = Self::columns(self.width);
            let slot = SLOT[index.min(BUTTONS - 1)];
            let gaps = if columns == BUTTONS { 20. } else { 0. };
            let width =
                ((self.width - 56. - gaps - (columns - 1) as f32 * 6.) / columns as f32).max(1.);
            let column = slot % columns;
            let group = if columns == BUTTONS {
                (column / 3) as f32 * 10.
            } else {
                0.
            };
            return Rect::new(
                28. + column as f32 * (width + 6.) + group,
                self.footer_y + 10. + (slot / columns) as f32 * 44.,
                width,
                36.,
            );
        }
        let columns = if self.width < 360. {
            3
        } else if self.narrow {
            4
        } else {
            7
        };
        let width = ((self.width - 56.) / columns as f32 - 6.).max(1.);
        Rect::new(
            28. + (index % columns) as f32 * (width + 6.),
            self.footer_y + 10. + (index / columns) as f32 * 44.,
            width,
            36.,
        )
    }
}

// The existing bitmap font is monospaced. Borrow slices instead of allocating
// a string per wrapped line; even an overlong binding token makes progress.
#[cfg(test)]
pub(super) fn label_lines(text: &str, width: f32) -> impl Iterator<Item = &str> {
    let advance = get_default_font()
        .try_metrics(16, 1.)
        .unwrap()
        .glyph_advance();
    let columns = (width / advance).floor().max(1.) as usize;
    let mut remaining = text.trim();
    std::iter::from_fn(move || {
        if remaining.is_empty() {
            return None;
        }
        let end = remaining
            .char_indices()
            .nth(columns)
            .map_or(remaining.len(), |(i, _)| i);
        let cut = if end == remaining.len() {
            end
        } else {
            remaining[..end]
                .rfind(char::is_whitespace)
                .filter(|&i| i > 0)
                .unwrap_or(end)
        };
        let line = remaining[..cut].trim_end();
        remaining = remaining[cut..].trim_start();
        Some(line)
    })
}

#[cfg(test)]
pub(super) fn slot_y(footer_y: f32, width: f32, slot: usize) -> f32 {
    footer_y
        + 116.
        + if width < 360. { 44. } else { 0. }
        + SLOT_LINES[..slot].iter().sum::<usize>() as f32 * ROW_HEIGHT
}

#[cfg(test)]
pub(super) fn bounded_hint(hint: &str, width: f32) -> &str {
    if get_default_font()
        .try_metrics(16, 1.)
        .unwrap()
        .measure_line(hint)
        .unwrap()
        .width
        <= width
    {
        hint
    } else {
        "Controls"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_scene_and_actions_fit_the_actual_viewport() {
        for (w, h) in [(320., 480.), (390., 568.), (844., 390.), (1280., 800.)] {
            let layout = LabLayout::compact(w, h);
            assert!(layout.footer_height < h * 0.5);
            for i in 0..7 {
                let button = layout.button(i);
                assert!(button.y >= layout.footer_y && button.y + button.h < h);
                assert!(!layout.contains_scene(button.center()));
            }
            for world in [vec2(-11.2, 0.6), vec2(11.2, 12.4)] {
                let point = layout.to_screen(world);
                assert!(layout.contains_scene(point));
                assert!((layout.to_world(point) - world).length() < 0.001);
            }
        }
    }

    #[test]
    fn compact_grouped_buttons_stay_readable_and_never_overlap() {
        let labels = [
            "BALL", "CRATE", "DROP", "PULSE", "RESUME", "STEP", "RESET", "CHAOS", "DOMINO",
        ];
        for (w, h) in [
            (320., 480.),
            (390., 700.),
            (479., 640.),
            (480., 640.),
            (640., 400.),
            (819., 600.),
            (820., 600.),
            (844., 390.),
            (1280., 800.),
            (1600., 1000.),
        ] {
            let layout = LabLayout::compact(w, h);
            assert!(layout.footer_height < h * 0.5, "{w}x{h}");
            let rects: Vec<_> = (0..BUTTONS).map(|i| layout.button(i)).collect();
            for (i, rect) in rects.iter().enumerate() {
                assert!(
                    rect.x >= 28. && rect.x + rect.w <= w - 28. + 0.01,
                    "{w}: {i}"
                );
                assert!(rect.y >= layout.footer_y);
                assert!(rect.y + rect.h + 8. < layout.status_y() - 12.);
                let measured = get_default_font()
                    .try_metrics(17, 1.)
                    .unwrap()
                    .measure_line(labels[i])
                    .unwrap()
                    .width;
                let size = 17. * ((rect.w - 12.) / measured).min(1.);
                assert!(size >= 14., "{w}: {} at {size}", labels[i]);
                for other in &rects[i + 1..] {
                    assert!(
                        rect.x + rect.w <= other.x
                            || other.x + other.w <= rect.x
                            || rect.y + rect.h <= other.y
                            || other.y + other.h <= rect.y
                    );
                }
            }
            // Three footer text lines (two on the single-row layout) fit.
            let lines = if layout.single_row() { 2. } else { 3. };
            assert!(layout.status_y() + (lines - 1.) * 20. + 4. <= h);
            // The top bar never overlaps the scene's 13 m grid line.
            assert!(layout.scene_top() >= 44., "{w}x{h}: {}", layout.scene_top());
        }
    }

    #[test]
    fn zoomed_view_round_trips_and_keeps_its_focus_fixed() {
        let layout = LabLayout::compact(1280., 800.);
        let focus = vec2(-1.5, 4.);
        let view = layout.zoomed(focus, 1.08, vec2(3., -2.));
        assert!((view.to_screen(focus) - layout.to_screen(focus) - vec2(3., -2.)).length() < 1e-3);
        for world in [vec2(-11.2, 0.6), vec2(11.2, 12.4), vec2(0., 7.)] {
            assert!((view.to_world(view.to_screen(world)) - world).length() < 1e-4);
        }
        assert_eq!(
            layout.zoomed(focus, f32::NAN, Vec2::ZERO).scale,
            layout.scale
        );
    }

    #[test]
    fn hosted_short_viewports_keep_world_buttons_and_diagnostics_separate() {
        // Browser viewport != canvas height. The host's scrollable 880px minimum
        // is deliberate; these are not claims for a 320x480 native GPU window.
        let host = include_str!("../../../web/physics-2d/index.html");
        assert!(host.contains(".lab-stage { min-height: 880px; }"));
        assert!(host.contains("top: calc(100% - 548px)"));
        assert!(host.contains(".lab-stage { min-height: 924px; }"));
        assert!(host.contains("top: calc(100% - 592px)"));
        for (width, viewport_height) in [(320., 480_f32), (320., 568.), (390., 844.), (1280., 800.)]
        {
            let height = if width < 640. {
                (if width < 360. { 924_f32 } else { 880_f32 }).max(viewport_height - 176.)
            } else {
                viewport_height - 176.
            };
            let layout = LabLayout::new(width, height);
            for world in [vec2(-11.2, 0.6), vec2(11.2, 12.4), vec2(0., 7.)] {
                let point = layout.to_screen(world);
                assert!(layout.contains_scene(point));
                assert!((layout.to_world(point) - world).length() < 0.00001);
            }
            assert!(!layout.contains_scene(vec2(100., 30.)));
            for index in 0..7 {
                let button = layout.button(index);
                assert!(button.x >= 28. && button.x + button.w <= width - 28.);
                assert!(
                    button.y >= layout.footer_y
                        && button.y + button.h < slot_y(layout.footer_y, width, 0) - 14.
                );
                assert!(!layout.contains_scene(vec2(button.x, button.y)));
                for label in ["BALL", "CRATE", "DROP", "PULSE", "RESUME", "STEP", "RESET"] {
                    let measured = get_default_font()
                        .try_metrics(17, 1.)
                        .unwrap()
                        .measure_line(label)
                        .unwrap()
                        .width;
                    let scale = ((button.w - 12.) / measured).min(1.);
                    assert!(17. * scale >= 14., "unreadable {label} at {width}");
                    let glyphs = get_default_font()
                        .try_metrics(17, scale)
                        .unwrap()
                        .measure_line(label)
                        .unwrap()
                        .ink_bounds
                        .unwrap();
                    assert!(glyphs.w <= button.w - 12.);
                }
            }
            assert!(layout.footer_y + layout.footer_height < height);
        }
    }

    #[test]
    fn diagnostic_slots_keep_full_text_at_readable_glyph_size() {
        let groups = [
            "SUSPENDED / CRATE / 240 of 240 bodies / 240 awake / 240 asleep",
            "120 Hz / tick 18446744073709551615 / pulse 0.65s",
            "Body selected. Outline follows its pose; diagnostics are read-only.",
            "Last tick: 28800 pairs / +28800 -28800 / query 8 / 480 collider hits (truncated)",
            "#240 COMPOUND WAKE PENDING / quiet 999999.00s / speed 9999.00 / 239 contacts",
        ];
        for width in [320., 390., 639.] {
            for (slot, text) in groups.iter().enumerate() {
                let lines: Vec<_> = label_lines(text, width - 56.).collect();
                assert!(lines.len() <= SLOT_LINES[slot], "slot {slot}: {lines:?}");
                assert_eq!(lines.join(" "), *text);
                let metrics = get_default_font().try_metrics(16, 1.).unwrap();
                for line in &lines {
                    let bounds = metrics.measure_line(line).unwrap().ink_bounds.unwrap();
                    assert!(28. + bounds.x + bounds.w <= width - 28.);
                    assert_eq!(metrics.pixel_size(), 2.);
                }
                let bottom = slot_y(0., width, slot) + (lines.len() - 1) as f32 * ROW_HEIGHT;
                assert!(bottom < LabLayout::new(width, 924.).footer_height - 8.);
                if slot < 4 {
                    assert!(bottom < slot_y(0., width, slot + 1) - 14.);
                }
            }
        }
    }

    #[test]
    fn unbound_long_hints_and_miss_capacity_notices_keep_their_meaning() {
        assert_eq!(bounded_hint("Unbound", 164.), "Unbound");
        assert_eq!(
            bounded_hint("Left Shift + Right Control + PageDown", 164.),
            "Controls"
        );
        for text in [
            "No dynamic body at cursor. Selection cleared.",
            "No body selected / inspect at cursor or drop a shape",
            "Body limit reached (240). RESET clears the scene.",
            "Drops stay still. Controls / STEP advances one tick.",
        ] {
            let lines: Vec<_> = label_lines(text, 264.).collect();
            assert!(lines.len() <= 4);
            assert_eq!(lines.join(" "), text);
        }
        let token = "abcdefghijklmnopqrstuvwxyz0123456789";
        assert_eq!(label_lines(token, 24.).collect::<String>(), token);
        assert_eq!(label_lines("éééé", 24.).collect::<Vec<_>>(), ["éé", "éé"]);
    }

    #[test]
    fn actual_lab_inspection_and_rebound_hints_fit_reserved_rows() {
        use crate::{Lab, LabInput, action_hint, register_lab_actions};
        use ferravex_render::input::{Binding, InputCode, PlayerInput};
        let mut lab = Lab::new();
        lab.update(
            0.,
            LabInput {
                toggle_pause: true,
                ..Default::default()
            },
        );
        lab.update(
            0.,
            LabInput {
                drop: Some((vec2(10.8, 4.), true)),
                ..Default::default()
            },
        );
        assert!(lab.selected_label().starts_with("#35 BOX "));
        for text in [lab.selected_label(), lab.feedback.to_owned()] {
            let lines: Vec<_> = label_lines(&text, 264.).collect();
            assert!(lines.len() <= 4);
            assert_eq!(lines.join(" "), text);
        }
        lab.select_body(None);
        assert!(lab.selected_label().starts_with("No body selected"));
        assert!(label_lines(lab.feedback, 264.).count() <= 4);
        let mut input = PlayerInput::transient("journey200-197-layout");
        register_lab_actions(&mut input);
        input.rebind("inspect", vec![]).unwrap();
        assert_eq!(
            bounded_hint(&action_hint(&input, "inspect"), 164.),
            "Unbound"
        );
        input
            .rebind(
                "drop",
                vec![
                    Binding::digital(InputCode::Key(KeyCode::LeftShift as u16)),
                    Binding::digital(InputCode::Key(KeyCode::RightControl as u16)),
                ],
            )
            .unwrap();
        assert_eq!(bounded_hint(&action_hint(&input, "drop"), 164.), "Controls");
    }
}
