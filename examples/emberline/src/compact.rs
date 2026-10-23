//! Narrow-window presentation in screen units; the authored world stays 1280×720.
use super::*;
use ferravex_render::canvas::Canvas2D;

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

    pub(super) fn draw(self, game: &Game, input: &PlayerInput) {
        set_camera(&self.canvas.camera());
        let w = self.canvas.size.x;
        let center = w * 0.5;
        let collected = game.embers.iter().filter(|(_, found)| *found).count();
        let touch = input.current_device() == HintDevice::Touch;
        let defaults = touch && !input.touch_customized();
        if game.session.is_playing() {
            draw_rectangle(0., 0., w, 64., Color::new(0.08, 0.13, 0.11, 0.96));
            label(
                &format!(
                    "{:.2}s / LANTERNS {}/2",
                    game.trial.elapsed(),
                    game.checkpoints.index()
                ),
                12.,
                28.,
                15.,
                CREAM,
            );
            fitted_centered_label(
                &format!(
                    "{collected}/{} EMBERS / {} PTS",
                    game.embers.len(),
                    game.score.total()
                ),
                (w - 64.) * 0.5,
                52.,
                w - 88.,
                15.,
                ORANGE,
            );

            let notice = if game.respawn_notice > 0. {
                if game.checkpoints.index() == 0 {
                    "BACK TO THE TRAILHEAD"
                } else {
                    "BACK TO THE LIT LANTERN"
                }
            } else if game.checkpoint_notice > 0. {
                "LANTERN LIT / SAVED"
            } else if game.ember_inventory.count(1) >= 3 {
                if game.air_jump_used {
                    "AIR JUMP SPENT"
                } else {
                    "AIR JUMP READY"
                }
            } else {
                ""
            };
            centered_label(notice, center, 82., 14., CREAM);
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

        let card = self.card;
        draw_rectangle(
            card.x,
            card.y,
            card.w,
            card.h,
            Color::new(0.08, 0.13, 0.11, 0.98),
        );
        let line = |text: &str, y: f32, color| centered_label(text, center, card.y + y, 15., color);
        let ready = game.session.state() == SessionState::Ready;
        let title = if game.won() {
            "THE LIGHT CARRIES ON"
        } else if ready {
            "CARRY THE LIGHT"
        } else {
            "TAKE A BREATHER"
        };
        centered_label(title, center, card.y + 30., 18., CREAM);
        let confirm = ControlHint::new(input, "confirm");
        let restart = ControlHint::new(input, "restart");
        let pause = ControlHint::new(input, "pause");
        if game.won() {
            fitted_centered_label(
                &format!("{collected} EMBERS / {} POINTS", game.score.total()),
                center,
                card.y + 64.,
                card.w - 24.,
                15.,
                ORANGE,
            );
            line(&format!("{} FALLS", game.deaths), 87., CREAM);
            line(
                &format!(
                    "{:?} / {:.2}s",
                    game.trial.medal().unwrap(),
                    game.trial.elapsed()
                ),
                116.,
                GREEN,
            );
        } else if ready {
            line("REACH THE GREEN LIFT", 72., CREAM);
        } else {
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
