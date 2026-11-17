//! Narrow-window presentation in screen units; the authored world stays 1280×720.
//! It speaks the same pictogram language as the wide HUD: no sentences.
use super::*;
use crate::style::{fade, line};
use ferravex_render::canvas::Canvas2D;
use ferravex_render::icons::{self, Icon};
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(
        self,
        game: &Game,
        input: &PlayerInput,
        juice: &Juice,
        overlay: &hud::Overlay,
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
            draw_rectangle(0., 62., w, 2., fade(ORANGE, 0.5));
            let elapsed = game.trial.elapsed();
            icons::draw(Icon::Clock, vec2(20., 22.), 16., fade(hud::MUTED, 0.9));
            label(
                &line!("{elapsed:.2}S"),
                34.,
                28.,
                16.,
                hud::medal_color(hud::pace(elapsed)),
            );
            let embers = line!("{collected}/{}", game.embers.len());
            right_label(&embers, right, 28., 16., CREAM);
            hud::gem(
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
            hud::air_jump_meter(game, overlay, vec2(80., 47.), juice.motion());
            let points = if game.score.multiplier() > 1 && game.score.chain() > 0 && w >= 360. {
                line!("×{} {}", game.score.multiplier(), game.score.total())
            } else {
                line!("{}", game.score.total())
            };
            right_label(&points, right, 56., 14., ORANGE);
            icons::draw(
                Icon::Star,
                vec2(right - text_width(&points, 14.) - 11., 50.),
                14.,
                crate::style::GOLD,
            );

            let slot = 72.;
            if let Some(toast) = &juice.toast {
                // One line: pictogram, callout and any number.
                let alpha = toast.alpha();
                let value_w = if toast.value.is_empty() {
                    0.
                } else {
                    10. + text_width(&toast.value, 14.)
                };
                let lines = wrap(&toast.head, w - 88. - value_w);
                let rows = if lines[1].is_empty() { 1. } else { 2. };
                let h = 16. + rows * 20.;
                draw_rectangle(
                    12.,
                    slot,
                    w - 24.,
                    h,
                    Color::new(0.07, 0.11, 0.10, 0.92 * alpha),
                );
                draw_rectangle(12., slot, w - 24., 2., fade(toast.color, 0.7 * alpha));
                let first = text_width(lines[0], 14.) + value_w + 26.;
                let x = center - first * 0.5;
                hud::glyph(
                    toast.icon,
                    vec2(x + 9., slot + 19.),
                    18.,
                    fade(toast.color, alpha),
                );
                label(lines[0], x + 26., slot + 25., 14., fade(CREAM, alpha));
                if !toast.value.is_empty() {
                    label(
                        &toast.value,
                        x + 26. + text_width(lines[0], 14.) + 10.,
                        slot + 25.,
                        14.,
                        fade(toast.value_color, alpha),
                    );
                }
                if !lines[1].is_empty() {
                    centered_label(lines[1], center, slot + 45., 14., fade(CREAM, alpha));
                }
            } else if let Some((kind, alpha)) = overlay.prompt() {
                if defaults {
                    // Touch players are pointed at the jump button itself below.
                } else {
                    let jump = ControlHint::new(input, "jump");
                    let keys = crate::style::key_prompt_width(jump.text(), "");
                    let width = keys + 40.;
                    let rect = Rect::new(center - width * 0.5, slot, width, 36.);
                    draw_rectangle(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        Color::new(0.07, 0.11, 0.10, 0.9 * alpha),
                    );
                    let theme = crate::style::theme().faded(alpha);
                    let x = rect.x + 8.;
                    let x = x + crate::style::key_prompt(x, rect.y + 5., jump.text(), "", &theme);
                    let color = if kind == hud::Prompt::AirJump {
                        GREEN
                    } else {
                        CREAM
                    };
                    hud::glyph(
                        kind.glyph(),
                        vec2(x + 14., rect.y + 17.),
                        20.,
                        fade(color, alpha),
                    );
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
                let prompt = overlay.prompt().map_or(0., |(_, alpha)| alpha);
                for (i, (rect, icon)) in self
                    .buttons
                    .into_iter()
                    .zip([
                        Icon::ArrowLeft,
                        Icon::ArrowRight,
                        Icon::ArrowUp,
                        Icon::Pause,
                    ])
                    .enumerate()
                {
                    ferravex_render::shapes::fill_rounded_rect(
                        rect,
                        10.,
                        Color::new(0.08, 0.13, 0.11, 0.96),
                    );
                    if i == 2 && prompt > 0. {
                        // The jump button breathes while a jump prompt is live.
                        let breathe =
                            0.5 + 0.5 * (overlay.since * 5.).sin() * juice.motion().min(1.);
                        ferravex_render::shapes::glow_ring(
                            rect.center(),
                            rect.w * 0.55 + 4. * breathe,
                            3.,
                            fade(GREEN, prompt * (0.5 + 0.4 * breathe)),
                        );
                    }
                    icons::draw(icon, rect.center(), 26., CREAM);
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
        draw_rectangle(card.x, card.y, card.w, 3., fade(ORANGE, alpha));
        let line = |text: &str, y: f32, size: f32, color: Color| {
            fitted_centered_label(
                text,
                center,
                card.y + y,
                card.w - 24.,
                size,
                fade(color, alpha),
            )
        };
        let ready = game.session.state() == SessionState::Ready;
        let confirm = ControlHint::new(input, "confirm");
        let restart = ControlHint::new(input, "restart");
        let pause = ControlHint::new(input, "pause");
        if game.won() {
            let run = hud::summary(game);
            line(quips::rank(run).title, 30., 20., juice::GOLD);
            let time = line!("{:.2}S", game.trial.elapsed());
            let time_w = text_width(&time, 18.);
            let x = center - (time_w + 30.) * 0.5;
            hud::glyph(
                juice::Glyph::Medal(run.medal),
                vec2(x + 10., card.y + 50.),
                20.,
                fade(CREAM, alpha),
            );
            label(
                &time,
                x + 30.,
                card.y + 57.,
                18.,
                fade(hud::medal_color(run.medal), alpha),
            );
            self.chip(hud::record_line(game, results), None, card.y + 80., alpha);
            self.stats(
                &[
                    (
                        juice::Glyph::Ember,
                        line!("{}/{}", run.embers, run.total_embers),
                    ),
                    (juice::Glyph::Icon(Icon::Skull), line!("{}", run.falls)),
                ],
                card.y + 108.,
                alpha,
            );
            self.chip(hud::hook(game), Some(Icon::Target), card.y + 134., alpha);
        } else if ready {
            line("EMBERLINE", 34., 24., CREAM);
            // The three goals as a pictogram row: lanterns, embers, the lift.
            let y = card.y + 66.;
            hud::glyph(
                juice::Glyph::Lantern(true),
                vec2(center - 60., y),
                22.,
                fade(CREAM, alpha),
            );
            hud::glyph(
                juice::Glyph::Ember,
                vec2(center - 20., y),
                20.,
                fade(CREAM, alpha),
            );
            hud::glyph(
                juice::Glyph::AirJump,
                vec2(center + 14., y),
                20.,
                fade(GREEN, alpha),
            );
            hud::glyph(
                juice::Glyph::Icon(Icon::Flag),
                vec2(center + 56., y),
                22.,
                fade(GREEN, alpha),
            );
            if let Some(best) = best {
                self.chip(
                    hud::Chip {
                        glyph: juice::Glyph::Icon(Icon::Trophy),
                        text: line!("{best:.2}S"),
                        color: juice::GOLD,
                        value: crate::style::Line::new(),
                        value_color: juice::GOLD,
                    },
                    None,
                    card.y + 106.,
                    alpha,
                );
            } else {
                // Medal targets: G 12S, S 18S, B 30S.
                let mut x = center - 105.;
                for (medal, secs) in [
                    ferravex_play::gameplay::trial::Medal::Gold,
                    ferravex_play::gameplay::trial::Medal::Silver,
                    ferravex_play::gameplay::trial::Medal::Bronze,
                ]
                .into_iter()
                .zip(MEDALS)
                {
                    hud::glyph(
                        juice::Glyph::Medal(medal),
                        vec2(x + 9., card.y + 106.),
                        18.,
                        fade(CREAM, alpha),
                    );
                    label(
                        &line!("{secs:.0}S"),
                        x + 22.,
                        card.y + 112.,
                        14.,
                        fade(hud::medal_color(medal), alpha),
                    );
                    x += 74.;
                }
            }
        } else {
            line("PAUSED", 34., 24., CREAM);
            self.stats(
                &[
                    (
                        juice::Glyph::Icon(Icon::Clock),
                        line!("{:.2}S", game.trial.elapsed()),
                    ),
                    (
                        juice::Glyph::Ember,
                        line!("{collected}/{}", game.embers.len()),
                    ),
                    (juice::Glyph::Icon(Icon::Skull), line!("{}", game.deaths)),
                ],
                card.y + 64.,
                alpha,
            );
            // Default touch play has no restart button; show the keyboard key.
            let key = if defaults {
                keyboard_restart_key(input)
            } else {
                Some(line!("{}", restart.text()))
            };
            self.keys(key.as_deref(), Icon::Restart, card.y + 98., alpha);
        }
        if defaults && touch_button_matches(input, "confirm", 10) {
            // The whole card is the touch target: a hand and a play arrow.
            let y = card.y + 172.;
            icons::draw(Icon::Hand, vec2(center - 16., y), 26., fade(GREEN, alpha));
            icons::draw(
                if game.won() {
                    Icon::Restart
                } else {
                    Icon::Play
                },
                vec2(center + 18., y),
                24.,
                fade(GREEN, alpha),
            );
        } else {
            let keys = if game.won() {
                line!("{} / {}", confirm.text(), restart.text())
            } else if ready {
                line!("{}", confirm.text())
            } else {
                line!("{} / {}", confirm.text(), pause.text())
            };
            let icon = if game.won() {
                Icon::Restart
            } else {
                Icon::Play
            };
            self.keys(Some(&keys), icon, card.y + 166., alpha);
        }
    }

    /// Key caps (or a complete fallback) followed by a pictogram, centered.
    fn keys(self, keys: Option<&str>, icon: Icon, top: f32, alpha: f32) {
        let width = self.card.w - 24.;
        let center = self.canvas.size.x * 0.5;
        let theme = crate::style::theme().faded(alpha);
        match keys.filter(|keys| hint_text(keys, width - 40.) == *keys) {
            Some(keys) => {
                let w = crate::style::key_prompt_width(keys, "") + 30.;
                let x = center - w * 0.5;
                let x = x + crate::style::key_prompt(x, top, keys, "", &theme);
                icons::draw(icon, vec2(x + 14., top + 12.), 22., fade(GREEN, alpha));
            }
            None => {
                icons::draw(icon, vec2(center - 70., top + 12.), 20., fade(GREEN, alpha));
                centered_label(
                    "SEE CONTROLS",
                    center + 12.,
                    top + 18.,
                    15.,
                    fade(CREAM, alpha),
                );
            }
        }
    }

    /// A centered row of pictogram + number pairs.
    fn stats(self, items: &[(juice::Glyph, crate::style::Line)], y: f32, alpha: f32) {
        let item = |text: &str| 24. + text_width(text, 14.);
        let gap = 20.;
        let total: f32 =
            items.iter().map(|(_, text)| item(text)).sum::<f32>() + gap * (items.len() - 1) as f32;
        let mut x = self.canvas.size.x * 0.5 - total * 0.5;
        for (icon, text) in items {
            hud::glyph(*icon, vec2(x + 8., y), 18., fade(hud::MUTED, alpha));
            label(text, x + 22., y + 6., 14., fade(CREAM, alpha));
            x += item(text) + gap;
        }
    }

    /// A results chip: optional lead icon, pictogram, text and value.
    fn chip(self, chip: hud::Chip, lead: Option<Icon>, y: f32, alpha: f32) {
        let lead_w = if lead.is_some() { 22. } else { 0. };
        let value_w = if chip.value.is_empty() {
            0.
        } else {
            8. + text_width(&chip.value, 14.)
        };
        let total = lead_w + 24. + text_width(&chip.text, 14.) + value_w;
        let mut x = self.canvas.size.x * 0.5 - total.min(self.card.w - 24.) * 0.5;
        if let Some(icon) = lead {
            icons::draw(icon, vec2(x + 8., y), 16., fade(hud::MUTED, alpha));
            x += lead_w;
        }
        hud::glyph(chip.glyph, vec2(x + 9., y), 18., fade(chip.color, alpha));
        x += 24.;
        label(&chip.text, x, y + 6., 14., fade(chip.color, alpha));
        if !chip.value.is_empty() {
            label(
                &chip.value,
                x + text_width(&chip.text, 14.) + 8.,
                y + 6.,
                14.,
                fade(chip.value_color, alpha),
            );
        }
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

/// The keyboard restart key for default touch play (which has no restart
/// button), or `None` when only the controls editor can say.
fn keyboard_restart_key(input: &PlayerInput) -> Option<crate::style::Line> {
    let mut buffer = [0; 128];
    match input.format_hint_for("restart", Some(HintDevice::KeyboardMouse), &mut buffer) {
        Ok(binding) if binding != "Unbound" => Some(line!("{binding}")),
        _ => None,
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

fn right_label(text: &str, right: f32, y: f32, size: f32, color: Color) {
    crate::style::right(text, right, y, crate::style::body(size, color));
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
        assert_eq!(keyboard_restart_key(&input).unwrap(), "R");
        input
            .rebind(
                "restart",
                vec![Binding::digital(InputCode::Key(b'j' as u16))],
            )
            .unwrap();
        assert_eq!(keyboard_restart_key(&input).unwrap(), "J");
        input
            .rebind("restart", vec![Binding::digital(InputCode::Touch(9))])
            .unwrap();
        assert_eq!(
            keyboard_restart_key(&input),
            None,
            "falls back to SEE CONTROLS"
        );
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
