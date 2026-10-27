//! Narrow-window presentation in screen units; the authored world stays 1280×720.
use super::*;
use ferravex_render::canvas::Canvas2D;
use presentation::ControlHint;

#[derive(Clone, Copy)]
pub(super) struct CompactHud {
    canvas: Canvas2D,
    card: Rect,
    buttons: [Rect; 4],
}

impl CompactHud {
    pub(super) fn for_screen(screen: Vec2) -> Option<Self> {
        if screen.x >= 700. {
            return None;
        }
        // Keep the layout bounded even in a temporarily tiny resized window.
        let canvas = Canvas2D::fit(vec2(screen.x.max(320.), screen.y.max(360.)), screen);
        let Vec2 { x: w, y: h } = canvas.size;
        Some(Self {
            canvas,
            card: Rect::new(12., ((h - 208.) * 0.5).max(100.), w - 24., 208.),
            buttons: [
                Rect::new(12., h - 72., 56., 56.),
                Rect::new(80., h - 72., 56., 56.),
                Rect::new(w - 76., h - 72., 64., 56.),
                Rect::new(w - 160., h - 72., 72., 56.),
            ],
        })
    }

    pub(super) fn touch_layout(self) -> [Control; 5] {
        let rects = [
            self.buttons[0],
            self.buttons[1],
            self.buttons[2],
            self.buttons[3],
            self.card,
        ];
        std::array::from_fn(|i| {
            let rect = rects[i];
            let p = self.canvas.project(vec2(rect.x, rect.y));
            Control {
                region: Region::Rect(Rect::new(
                    p.x,
                    p.y,
                    rect.w * self.canvas.scale,
                    rect.h * self.canvas.scale,
                )),
                output: Output::Button([12, 13, 3, 11, 10][i]),
            }
        })
    }

    pub(super) fn draw(
        self,
        game: &Game,
        input: &PlayerInput,
        juice: &Juice,
        results: &hud::Results,
        best: Option<f64>,
    ) {
        set_camera(&self.canvas.camera());
        let w = self.canvas.size.x;
        let center = w * 0.5;
        let collected = game.embers.iter().filter(|(_, found)| *found).count();
        let touch = input.current_device() == HintDevice::Touch;
        let defaults = touch && !input.touch_customized();
        if game.session.is_playing() {
            // The shared fullscreen game menu owns the top-right 72 units.
            let right = bar_right(w);
            draw_rectangle(0., 0., w, 64., Color::new(0.08, 0.13, 0.11, 0.96));
            draw_rectangle(0., 62., w, 2., hud::fade(ORANGE, 0.5));
            let elapsed = game.trial.elapsed();
            label(
                &format!("{elapsed:.2}S"),
                12.,
                28.,
                16.,
                hud::medal_color(hud::pace(elapsed)),
            );
            let embers = format!("{collected}/{}", game.embers.len());
            hud::right_label(&embers, right, 28., 16., CREAM);
            diamond(
                vec2(right - text_width(&embers, 16.) - 12., 22.),
                7.,
                ORANGE,
            );
            for index in 1..=2 {
                hud::lantern_icon(
                    20. + (index - 1) as f32 * 22.,
                    52.,
                    game.checkpoints.index() >= index,
                );
            }
            let air = game.ember_inventory.count(1) >= 3 && !game.air_jump_used;
            label(
                "AIR JUMP",
                58.,
                56.,
                14.,
                if air { GREEN } else { hud::MUTED },
            );
            let mut points = format!("{} PTS", game.score.total());
            if game.score.multiplier() > 1 && game.score.chain() > 0 && w >= 360. {
                points = format!("×{} {points}", game.score.multiplier());
            }
            hud::right_label(&points, right, 56., 14., ORANGE);

            if let Some(toast) = &juice.toast {
                let alpha = toast.alpha();
                let lines = wrap(&toast.head, w - 40.);
                let sub = wrap(&toast.sub, w - 40.);
                let rows = lines.iter().chain(&sub).filter(|l| !l.is_empty()).count() as f32;
                draw_rectangle(
                    12.,
                    72.,
                    w - 24.,
                    14. + rows * 20.,
                    Color::new(0.07, 0.11, 0.10, 0.9 * alpha),
                );
                let mut y = 92.;
                for (text, color) in lines
                    .iter()
                    .map(|l| (l, toast.color))
                    .chain(sub.iter().map(|l| (l, CREAM)))
                {
                    if !text.is_empty() {
                        centered_label(text, center, y, 14., hud::fade(color, alpha));
                        y += 20.;
                    }
                }
            } else if best.is_none()
                && let Some(text) = hud::coach(game, ControlHint::new(input, "jump").text())
            {
                let lines = wrap(&text, w - 40.);
                let rows = lines.iter().filter(|l| !l.is_empty()).count() as f32;
                // Share the toast slot: in landscape the lower band is the course.
                let top = 72.;
                draw_rectangle(
                    12.,
                    top,
                    w - 24.,
                    10. + rows * 20.,
                    Color::new(0.07, 0.11, 0.10, 0.9),
                );
                for (i, text) in lines.iter().enumerate() {
                    centered_label(text, center, top + 20. + i as f32 * 20., 14., CREAM);
                }
            }
            if defaults
                && [
                    ("move_left", 12),
                    ("move_right", 13),
                    ("jump", 3),
                    ("pause", 11),
                ]
                .iter()
                .all(|(name, channel)| touch_button_matches(input, name, *channel))
            {
                for (rect, text) in self.buttons.into_iter().zip(["<", ">", "JUMP", "PAUSE"]) {
                    draw_rectangle(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        Color::new(0.08, 0.13, 0.11, 0.96),
                    );
                    centered_label(
                        text,
                        rect.x + rect.w * 0.5,
                        rect.y + rect.h * 0.5 + 5.,
                        15.,
                        CREAM,
                    );
                }
            }
            return;
        }

        // Let the finish celebration play before the results card settles in.
        let alpha = if game.won() && juice.motion() > 0. {
            ((juice.celebrate - 0.45) / 0.25).clamp(0., 1.)
        } else {
            1.
        };
        if alpha <= 0. {
            return;
        }
        let card = self.card;
        draw_rectangle(
            card.x,
            card.y,
            card.w,
            card.h,
            Color::new(0.08, 0.13, 0.11, 0.98 * alpha),
        );
        draw_rectangle(card.x, card.y, card.w, 3., hud::fade(ORANGE, alpha));
        let line = |text: &str, y: f32, size: f32, color: Color| {
            fitted_centered_label(
                text,
                center,
                card.y + y,
                card.w - 24.,
                size,
                hud::fade(color, alpha),
            )
        };
        let ready = game.session.state() == SessionState::Ready;
        let confirm = ControlHint::new(input, "confirm");
        let restart = ControlHint::new(input, "restart");
        let pause = ControlHint::new(input, "pause");
        if game.won() {
            let run = hud::summary(game);
            line(quips::rank(run).title, 30., 20., juice::GOLD);
            line(
                &format!(
                    "{} · {:.2}S",
                    quips::medal_name(run.medal),
                    game.trial.elapsed()
                ),
                56.,
                18.,
                hud::medal_color(run.medal),
            );
            let (record, record_color) = hud::record_line(game, results);
            line(&record, 80., 14., record_color);
            line(
                &format!(
                    "{}/{} EMBERS · {} FALLS",
                    run.embers, run.total_embers, run.falls
                ),
                102.,
                14.,
                CREAM,
            );
            let (hook, hook_color) = hud::hook(game);
            line(&hook, 124., 14., hook_color);
        } else if ready {
            line("EMBERLINE", 34., 24., CREAM);
            line("LIGHT THE LANTERNS,", 62., 14., hud::MUTED);
            line("RIDE THE GREEN LIFT", 82., 14., hud::MUTED);
            if let Some(best) = best {
                line(&format!("YOUR BEST {best:.2}S"), 112., 14., juice::GOLD);
            } else {
                line("UNDER 12S = GOLD", 112., 14., hud::MUTED);
            }
        } else {
            line("PAUSED", 34., 24., CREAM);
            line(
                &format!(
                    "{:.2}S · {collected}/{} EMBERS",
                    game.trial.elapsed(),
                    game.embers.len()
                ),
                60.,
                14.,
                CREAM,
            );
            let restart = if defaults {
                keyboard_restart_hint(input)
            } else {
                format!("{} RESTART", restart.text())
            };
            self.hint(&restart, card.y + 88.);
        }
        if defaults && touch_button_matches(input, "confirm", 10) {
            line(
                if game.won() {
                    "TAP TO PLAY AGAIN"
                } else {
                    "TAP TO PLAY"
                },
                174.,
                16.,
                GREEN,
            );
        } else {
            let keys = if game.won() {
                format!("{} / {}", confirm.text(), restart.text())
            } else if ready {
                confirm.text().to_owned()
            } else {
                format!("{} / {}", confirm.text(), pause.text())
            };
            self.hint(&keys, card.y + 160.);
            line(
                if game.won() {
                    "TO RUN AGAIN"
                } else if ready {
                    "TO START"
                } else {
                    "TO RESUME"
                },
                184.,
                16.,
                GREEN,
            );
        }
    }

    // A complete fallback remains readable instead of shrinking long remaps.
    fn hint(self, text: &str, y: f32) {
        centered_label(
            hint_text(text, self.card.w - 24.),
            self.canvas.size.x * 0.5,
            y,
            15.,
            CREAM,
        );
    }
}

/// Right edge of top-bar text, leaving the fullscreen menu corner clear.
pub(super) fn bar_right(width: f32) -> f32 {
    width - hud::MENU_CORNER.x - 8.
}

/// Up to two 14-unit lines; a longer second line is fitted by the caller's label.
fn wrap(text: &str, width: f32) -> [&str; 2] {
    if text_width(text, 14.) <= width {
        return [text, ""];
    }
    let split = text
        .char_indices()
        .filter(|&(i, c)| c == ' ' && text_width(&text[..i], 14.) <= width)
        .map(|(i, _)| i)
        .next_back()
        .unwrap_or(text.len());
    [&text[..split], text[split..].trim_start()]
}

fn keyboard_restart_hint(input: &PlayerInput) -> String {
    let mut buffer = [0; 128];
    match input.format_hint_for("restart", Some(HintDevice::KeyboardMouse), &mut buffer) {
        Ok(binding) if binding != "Unbound" => format!("KEYBOARD {binding}: RESTART"),
        _ => "RESTART: SEE CONTROLS".to_owned(),
    }
}

fn hint_text(text: &str, width: f32) -> &str {
    let metrics = get_default_font()
        .try_metrics(15, 1.)
        .expect("authored font size");
    if metrics
        .measure_line(text)
        .is_ok_and(|size| size.width <= width)
    {
        text
    } else {
        "SEE CONTROLS"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_render::input::touch::{Event, Phase};
    use ferravex_render::input::{ActionKind, Binding, InputCode};

    #[test]
    fn default_touch_pause_hints_do_not_advertise_an_unplaced_restart_button() {
        let mut input = PlayerInput::transient("emberline-touch-restart-hint");
        input
            .action(
                "restart",
                "Restart",
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(b'r' as u16)),
                    Binding::digital(InputCode::Touch(9)),
                ],
            )
            .unwrap();
        assert_eq!(keyboard_restart_hint(&input), "KEYBOARD R: RESTART");
        input
            .rebind(
                "restart",
                vec![Binding::digital(InputCode::Key(b'j' as u16))],
            )
            .unwrap();
        assert_eq!(keyboard_restart_hint(&input), "KEYBOARD J: RESTART");
        input
            .rebind("restart", vec![Binding::digital(InputCode::Touch(9))])
            .unwrap();
        assert_eq!(keyboard_restart_hint(&input), "RESTART: SEE CONTROLS");
    }

    #[test]
    fn narrow_touch_targets_match_visible_buttons_and_are_not_world_scaled() {
        for screen in [vec2(320., 448.), vec2(390., 590.), vec2(699., 360.)] {
            let hud = CompactHud::for_screen(screen).unwrap();
            assert_eq!(hud.canvas.scale, 1.);
            let controls = hud.touch_layout();
            let menu = Rect::new(screen.x - 52., 8., 44., 44.);
            for button in hud.buttons {
                assert!(
                    !button.overlaps(&menu),
                    "touch actions leave the game menu clear"
                );
            }
            for (i, control) in controls.iter().enumerate() {
                let Region::Rect(rect) = control.region else {
                    panic!("rectangular target")
                };
                assert!(
                    rect.x >= 0.
                        && rect.y >= 0.
                        && rect.x + rect.w <= screen.x
                        && rect.y + rect.h <= screen.y
                );
                assert!(rect.w >= 44. && rect.h >= 44.);
                let visible = if i == 4 { hud.card } else { hud.buttons[i] };
                assert_eq!(rect, visible);
            }
            for a in 0..4 {
                for b in a + 1..4 {
                    assert!(!hud.buttons[a].overlaps(&hud.buttons[b]));
                }
            }
            // Feed real touch routing at each visible button's center.
            let mut input = PlayerInput::transient("emberline-compact-186");
            for (name, channel) in [
                ("left", 12),
                ("right", 13),
                ("jump", 3),
                ("pause", 11),
                ("confirm", 10),
            ] {
                input
                    .action(
                        name,
                        name,
                        ActionKind::Button,
                        vec![Binding::digital(InputCode::Touch(channel))],
                    )
                    .unwrap();
            }
            for (i, name) in ["left", "right", "jump", "pause", "confirm"]
                .into_iter()
                .enumerate()
            {
                // Menus expose the card; gameplay only exposes four controls.
                input
                    .set_touch_layout(&controls[..if i == 4 { 5 } else { 4 }])
                    .unwrap();
                let Region::Rect(rect) = controls[i].region else {
                    unreachable!()
                };
                let position = vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
                ferravex_render::input::touch_event(Event {
                    id: 186,
                    phase: Phase::Start,
                    position,
                });
                ferravex_render::input::host_frame(0., screen.x as u32, screen.y as u32, || {
                    input.sample(true);
                    assert!(input.pressed(name), "{name} at {screen:?}");
                    assert!(!input.pressed(name), "one press per touch");
                });
                ferravex_render::input::touch_event(Event {
                    id: 186,
                    phase: Phase::End,
                    position,
                });
                ferravex_render::input::host_frame(0., screen.x as u32, screen.y as u32, || {
                    input.sample(true)
                });
            }
        }
        assert!(CompactHud::for_screen(vec2(700., 448.)).is_none());
        assert!(CompactHud::for_screen(vec2(1280., 560.)).is_none());
    }

    #[test]
    fn narrow_messages_wrap_onto_two_whole_word_lines_at_320_wide() {
        let width = CompactHud::for_screen(vec2(320., 448.)).unwrap().card.w - 16.;
        for text in [
            "STICK THE LANDING, NOT THE SPIKES",
            "PRESS JUMP AGAIN IN MID-AIR · ONCE PER LANDING",
            "LAND BY THE LANTERN TO SAVE YOUR SPOT",
        ] {
            let [first, second] = wrap(text, width);
            assert!(text_width(first, 14.) <= width, "{first}");
            assert!(text_width(second, 14.) <= width, "{second}");
            assert_eq!(format!("{first} {second}"), text);
        }
        assert_eq!(wrap("WHEW!", width), ["WHEW!", ""]);
    }

    #[test]
    fn narrow_hints_keep_complete_remaps_or_readable_fallback() {
        let width = CompactHud::for_screen(vec2(320., 448.)).unwrap().card.w - 24.;
        let mut input = PlayerInput::transient("emberline-compact-hint-186");
        input
            .action(
                "jump",
                "Jump",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let before = format!("JUMP {}", ControlHint::new(&input, "jump").text());
        assert_eq!(hint_text(&before, width), "JUMP Space");
        input
            .rebind("jump", vec![Binding::digital(InputCode::Key(b'j' as u16))])
            .unwrap();
        let after = format!("JUMP {}", ControlHint::new(&input, "jump").text());
        assert_eq!(hint_text(&after, width), "JUMP J");
        assert_eq!(
            hint_text("LEFT VeryLongKeyName / AnotherVeryLongKeyName", width),
            "SEE CONTROLS"
        );
        for text in [
            "THE LIGHT CARRIES ON",
            "Time and progress paused",
            "Falls return to lanterns",
            "TAP TO PLAY AGAIN",
            "F2 CONTROLS / REMAP",
        ] {
            assert_eq!(hint_text(text, width), text);
        }
    }
}
