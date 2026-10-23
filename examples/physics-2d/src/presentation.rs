//! Local immediate-mode lab labels. No shared layout or input ownership.
use ferravex_render::{prelude::*, ui};

pub(super) const NARROW_FOOTER_HEIGHT: f32 = 536.;
const ROW_HEIGHT: f32 = 20.;
const SLOT_LINES: [usize; 5] = [4, 3, 4, 5, 5];

pub(super) struct LabLayout {
    pub narrow: bool,
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
            footer_height,
            footer_y,
            scale,
            origin: vec2(width * 0.5, footer_y - 8. - scale),
            width,
        }
    }

    pub fn to_screen(&self, point: Vec2) -> Vec2 {
        self.origin + vec2(point.x, -point.y) * self.scale
    }

    pub fn to_world(&self, point: Vec2) -> Vec2 {
        vec2(
            (point.x - self.origin.x) / self.scale,
            (self.origin.y - point.y) / self.scale,
        )
    }

    pub fn contains_scene(&self, point: Vec2) -> bool {
        point.x >= 16.
            && point.x <= self.width - 16.
            && point.y > 110.
            && point.y < self.footer_y - 8.
    }

    pub fn button(&self, index: usize) -> Rect {
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

pub(super) fn slot_y(footer_y: f32, width: f32, slot: usize) -> f32 {
    footer_y
        + 116.
        + if width < 360. { 44. } else { 0. }
        + SLOT_LINES[..slot].iter().sum::<usize>() as f32 * ROW_HEIGHT
}

pub(super) fn wrapped_label(text: &str, x: f32, y: f32, width: f32, color: Color) {
    for (row, line) in label_lines(text, width).enumerate() {
        draw_text_ex(
            line,
            x,
            y + row as f32 * ROW_HEIGHT,
            TextParams {
                font_size: 16,
                font_scale: 1.,
                color,
            },
        );
    }
}

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

pub(super) fn narrow_header(hints: [&str; 3], width: f32) {
    draw_rectangle(0., 0., width, 104., ui::PANEL);
    draw_text("2D RIGID BODIES", 24., 27., 20., ui::INK);
    for (row, (action, hint)) in ["Drop", "Pause", "Inspect"]
        .into_iter()
        .zip(hints)
        .enumerate()
    {
        let text = format!("{action}: {}", bounded_hint(hint, width - 48. - 9. * 12.));
        draw_text(&text, 24., 50. + row as f32 * ROW_HEIGHT, 16., ui::MUTED);
    }
    draw_line(24., 103., width - 24., 103., 1., ui::LINE);
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(!layout.contains_scene(vec2(100., 100.)));
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
