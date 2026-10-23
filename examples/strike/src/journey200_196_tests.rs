//! J196 text-block and real Ui input regressions; no simulated game outcomes.
use super::*;
use crate::host::Mode;
use ferravex_render::input::{
    ActionKind, Binding, InputCode, host_frame,
    touch::{Event, Phase},
    touch_event,
};

fn profile() -> PlayerInput {
    let mut input = PlayerInput::transient("journey200-196");
    for (name, code) in [
        ("fire", InputCode::Mouse(1)),
        ("jump", InputCode::Key(KeyCode::Space as u16)),
        ("reload", InputCode::Key(KeyCode::R as u16)),
        ("roster", InputCode::Key(KeyCode::Tab as u16)),
        ("pause", InputCode::Key(KeyCode::Escape as u16)),
    ] {
        input
            .global_action(name, name, ActionKind::Button, vec![Binding::digital(code)])
            .unwrap();
    }
    input
}

fn dimensions(text: &str, size: f32) -> (f32, f32) {
    // The actual frozen Ui painter uses the same first-party bitmap advances.
    let width = text
        .lines()
        .map(|line| measure_text(line, None, 8, size / 8.).width)
        .fold(0_f32, f32::max);
    (width, text.lines().count() as f32 * size)
}

#[test]
fn menu_text_blocks_keep_all_lines_at_readable_sizes_and_clear_actions_and_footer() {
    let mut input = profile();
    for long in [false, true] {
        if long {
            input
                .rebind(
                    "fire",
                    [
                        KeyCode::LeftShift,
                        KeyCode::LeftControl,
                        KeyCode::RightShift,
                        KeyCode::RightControl,
                    ]
                    .map(|key| Binding::digital(InputCode::Key(key as u16)))
                    .to_vec(),
                )
                .unwrap();
            input.rebind("reload", vec![]).unwrap();
            input
                .rebind(
                    "pause",
                    vec![Binding::digital(InputCode::Key(KeyCode::Q as u16))],
                )
                .unwrap();
            // Exercise the saved-format import path, not cached/default captions.
            let saved = input.export_config().unwrap();
            let mut restored = profile();
            restored.import_config(&saved).unwrap();
            input = restored;
        }
        for (w, h) in [
            (320., 480.),
            (320., 568.),
            (390., 607.68),
            (1280., 576.),
            (1440., 900.),
        ] {
            for (mode, played, ids) in [
                (Mode::Practice, false, vec![1]),
                (Mode::Practice, true, vec![1, 2, 3]),
                (Mode::Replay, true, vec![1, 3]),
                (Mode::Network, true, vec![1]),
            ] {
                for touch in [false, true] {
                    let mut boxes = Vec::new();
                    let mut actual_ids = Vec::new();
                    let mut copy = Vec::new();
                    menu_items(w, h, mode, played, touch, &input, |item| {
                        let (rect, text, size, button) = match item {
                            MenuItem::Text(rect, args, size, _) => {
                                (rect, args.to_string(), size, false)
                            }
                            MenuItem::Button(id, rect, text, size, _) => {
                                actual_ids.push(id);
                                assert!(rect.size[1] >= 44.);
                                (rect, text.to_owned(), size, true)
                            }
                        };
                        assert!(size >= if button { 13.5 } else { 14. }, "{text}: {size}");
                        let (glyph_width, lines_height) = dimensions(&text, size);
                        let inset = if button { 16. } else { 0. };
                        assert!(
                            glyph_width <= rect.size[0] - inset + 0.001,
                            "{w}x{h} {mode:?} {text:?}: width {glyph_width} in {rect:?}"
                        );
                        assert!(
                            lines_height <= rect.size[1] + 0.001,
                            "{text}: incomplete lines"
                        );
                        assert!(rect.min[0] >= 12. && rect.min[1] >= 12.);
                        assert!(rect.min[0] + rect.size[0] <= w - 12. + 0.001);
                        assert!(
                            rect.min[1] + rect.size[1] < h - 150.,
                            "{text}: replay footer collision"
                        );
                        for previous in &boxes {
                            assert!(
                                rect.intersect(*previous).is_none(),
                                "{text}: overlapping blocks"
                            );
                        }
                        boxes.push(rect);
                        copy.push(text);
                    });
                    assert_eq!(actual_ids, ids);
                    assert!(
                        copy.iter()
                            .any(|text| text == menu_explanation(mode, played))
                    );
                    assert!(
                        !copy
                            .iter()
                            .any(|text| text.starts_with("Reload:") || text.contains("F2: move")),
                        "binding lists belong in the on-demand Controls editor"
                    );
                    let mut ui = Ui::new();
                    host_frame(0., w as u32, h as u32, || {
                        ui.begin(
                            ferravex_render::ui::Viewport::fit([w, h], [w, h], [0.; 4]).unwrap(),
                        );
                        menu(&mut ui, mode, played, touch, &input);
                        ui.finish(true);
                        ui.paint(); // Actual frozen Ui command/vertex budgets also accept the menu.
                    });
                }
            }
        }
    }
}

fn touch(id: u64, phase: Phase, position: [f32; 2]) {
    touch_event(Event {
        id,
        phase,
        position: vec2(position[0], position[1]),
    });
}

#[test]
fn painted_menu_rectangles_activate_only_on_inside_release_and_cancel_cleanly() {
    for (w, h) in [(320., 480.), (390., 608.), (1280., 576.)] {
        let input = profile();
        let mut buttons = Vec::new();
        menu_items(w, h, Mode::Practice, true, false, &input, |item| {
            if let MenuItem::Button(id, rect, ..) = item {
                buttons.push((id, rect));
            }
        });
        for &(id, rect) in &buttons {
            let mut ui = Ui::new();
            let frame = |ui: &mut Ui| {
                host_frame(0., w as u32, h as u32, || {
                    ui.begin(ferravex_render::ui::Viewport::fit([w, h], [w, h], [0.; 4]).unwrap());
                    menu(ui, Mode::Practice, true, false, &input);
                    ui.finish(true);
                    ui.paint();
                })
            };
            frame(&mut ui);
            let point = [rect.min[0] + 1., rect.min[1] + 1.];
            touch(196, Phase::Start, point);
            frame(&mut ui);
            assert_eq!(ui.focused(), Some(id));
            assert!(!ui.activated(id), "press alone must not activate");
            touch(196, Phase::End, point);
            frame(&mut ui);
            assert!(ui.activated(id), "painted inside edge must activate {id}");
            touch(196, Phase::Start, point);
            frame(&mut ui);
            touch(196, Phase::End, [rect.min[0] - 1., point[1]]);
            frame(&mut ui);
            assert!(!ui.activated(id), "release outside cancels");
            touch(196, Phase::Start, point);
            frame(&mut ui);
            touch(196, Phase::Cancel, point);
            frame(&mut ui);
            assert!(!ui.activated(id));
            touch(196, Phase::End, point);
            frame(&mut ui);
            assert!(!ui.activated(id), "a canceled capture cannot be revived");
        }
    }
}
