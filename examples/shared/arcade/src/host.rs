use crate::chrome::{CARD, Chrome, LEFT, Overlay, PRIMARY, SECONDARY, WIDE, grouped};
use crate::driver::Driver;
use crate::paint::{Viewport, chip, guide_pages, style, with_alpha};
use crate::*;
use ferravex_core::session::SessionState;
use ferravex_render::text::{draw_text_styled, wrap_text};
use ferravex_render::{
    audio::{self, GameAudio},
    demo::Demo,
    input::{ActionKind, Binding, InputCode, PlayerInput},
    storage::Records,
};

fn binding(key: KeyCode) -> Binding {
    Binding::digital(InputCode::Key(key as u16))
}
fn controls(owner: &str, transient: bool) -> PlayerInput {
    let mut input = if transient {
        PlayerInput::transient(owner)
    } else {
        PlayerInput::new(owner)
    };
    for (channel, (name, label, keys, pad)) in [
        ("left", "Left", [KeyCode::Left, KeyCode::A], 14),
        ("right", "Right", [KeyCode::Right, KeyCode::D], 15),
        ("up", "Up", [KeyCode::Up, KeyCode::W], 12),
        ("down", "Down", [KeyCode::Down, KeyCode::S], 13),
        (
            "primary",
            "Primary action",
            [KeyCode::Space, KeyCode::Enter],
            0,
        ),
        ("secondary", "Secondary action", [KeyCode::X, KeyCode::E], 1),
        ("tertiary", "Third action", [KeyCode::C, KeyCode::Q], 2),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bindings = keys.map(binding).to_vec();
        bindings.push(Binding::digital(InputCode::PadButton(pad)));
        bindings.push(Binding::digital(InputCode::Touch(channel as u8)));
        input
            .action(name, label, ActionKind::Button, bindings)
            .unwrap();
    }
    input
        .action(
            "click",
            "Choose with pointer",
            ActionKind::Button,
            vec![
                Binding::digital(InputCode::Mouse(1)),
                Binding::digital(InputCode::Touch(crate::touch::TAP)),
            ],
        )
        .unwrap();
    for i in 1..=5 {
        input
            .action(
                &format!("slot{i}"),
                &format!("Choice {i}"),
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(48 + i)),
                    Binding::digital(InputCode::Touch((i + 6) as u8)),
                ],
            )
            .unwrap();
    }
    for (name, label, keys) in [
        ("pause", "Pause / resume", vec![KeyCode::Escape, KeyCode::P]),
        ("restart", "New campaign", vec![KeyCode::R]),
        ("help", "How to play", vec![KeyCode::H]),
        ("mute", "Toggle sound", vec![KeyCode::M]),
        (
            "accept",
            "Start / resume",
            vec![KeyCode::Enter, KeyCode::Space],
        ),
        ("page_next", "Next help page", vec![KeyCode::Right]),
        ("page_prev", "Previous help page", vec![KeyCode::Left]),
    ] {
        let touch = match name {
            "pause" => Some(12),
            "help" => Some(13),
            "accept" => Some(4),
            "page_next" => Some(1),
            "page_prev" => Some(0),
            _ => None,
        };
        let mut bindings: Vec<_> = keys.into_iter().map(binding).collect();
        if let Some(channel) = touch {
            bindings.push(Binding::digital(InputCode::Touch(channel)));
        }
        input
            .global_action(name, label, ActionKind::Button, bindings)
            .unwrap();
    }
    input
        .global_action(
            "menu_click",
            "Menu pointer",
            ActionKind::Button,
            vec![
                Binding::digital(InputCode::Mouse(1)),
                Binding::digital(InputCode::Touch(crate::touch::TAP)),
            ],
        )
        .unwrap();
    input
}
fn tick_input(input: &mut PlayerInput, pointer: Vec2) -> Input {
    let a = input.take_tick([
        "left",
        "right",
        "up",
        "down",
        "primary",
        "secondary",
        "tertiary",
        "click",
        "slot1",
        "slot2",
        "slot3",
        "slot4",
        "slot5",
    ]);
    Input {
        left: a[0].value > 0.5,
        right: a[1].value > 0.5,
        up: a[2].value > 0.5,
        down: a[3].value > 0.5,
        left_pressed: a[0].pressed,
        right_pressed: a[1].pressed,
        up_pressed: a[2].pressed,
        down_pressed: a[3].pressed,
        primary: a[4].pressed,
        secondary: a[5].pressed,
        tertiary: a[6].pressed,
        click: a[7].pressed,
        primary_down: a[4].value > 0.5,
        pointer,
        slot: (8..13).find(|&i| a[i].pressed).map(|i| i - 7),
    }
}
fn play(audio: &mut GameAudio, cue: Cue) {
    let voice = match cue {
        Cue::None => return,
        Cue::Action => audio::Cue::Click,
        Cue::Good => audio::Cue::Pickup,
        Cue::Bad => audio::Cue::Failure,
        Cue::Win => audio::Cue::Success,
    };
    audio.cue(voice, if cue == Cue::Action { 0.16 } else { 0.3 });
}
/// Lay out a game's `Key Action | Key Action` summary as key caps.
fn controls_hint(controls: &str, y: f32, a: f32) {
    use crate::chrome::keycap_width;
    draw_text_styled(
        "CONTROLS",
        LEFT,
        y,
        style(12., with_alpha(MUTED, a)).with_letter_spacing(1.5),
    );
    let (mut x, mut y, mut rows) = (LEFT, y + 14., 1);
    for part in controls.split('|').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, action) = part.split_once(' ').unwrap_or((part, ""));
        let key = if key == "Act" { "Space" } else { key };
        let w = keycap_width(key, action);
        if x > LEFT && x + w > LEFT + CARD.w - 80. {
            if rows == 3 {
                return;
            }
            rows += 1;
            x = LEFT;
            y += 30.;
        }
        x += crate::paint::keycap_faded(key, action, x, y, Color::new(0.8, 0.85, 0.9, 1.), a) + 20.;
    }
}
/// Wrap `value` inside the menu card, ending with an ellipsis after `max`
/// lines; returns the baseline after the last line.
fn paragraph(value: &str, y: f32, size: f32, max: usize, color: Color) -> f32 {
    let s = style(size, color);
    let width = CARD.w - 80.;
    let mut lines = wrap_text(value, width, s).peekable();
    let mut row = 0;
    while let Some(line) = lines.next() {
        let baseline = y + row as f32 * size * 1.32;
        if row + 1 == max && lines.peek().is_some() {
            draw_text_styled(&format!("{line}\u{2026}"), LEFT, baseline, s);
            return baseline + size * 1.32;
        }
        draw_text_styled(line, LEFT, baseline, s);
        row += 1;
    }
    y + row as f32 * size * 1.32
}
/// Run a complete game through the existing native or browser renderer.
/// This owns local menus; all gameplay mutations occur in fixed-tick updates.
pub async fn run<G: ArcadeGame>(meta: Meta) {
    let mut demo = Demo::new();
    let mut driver = Driver::<G>::new(demo.autoplay);
    let mut input = controls(meta.id, demo.autoplay);
    let mut sound = GameAudio::new();
    let mut music = crate::music::MusicAudio::default();
    let mut records = Records::new(meta.id).ok();
    let mut bridge = ferravex_render::status::StatusBridge::new(
        ferravex_play::persistence::checksum(meta.id.as_bytes()).max(1),
    )
    .expect("nonzero status owner");
    #[cfg(not(target_arch = "wasm32"))]
    let capture_dir = if demo.autoplay {
        std::env::var_os("FERRAVEX_ARCADE_CAPTURE").map(std::path::PathBuf::from)
    } else {
        None
    };
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &capture_dir {
        std::fs::create_dir_all(path).expect("capture directory");
    }
    let mut recorded = false;
    let mut paused = false;
    let mut help = false;
    let mut help_page = 0usize;
    let mut help_paragraphs = vec![meta.controls];
    help_paragraphs.extend_from_slice(meta.help);
    let guide = guide_pages(&help_paragraphs);
    let mut chrome = Chrome::new(driver.game.hud().score);
    loop {
        let elapsed = f64::from(demo.dt());
        let dock = crate::touch::TouchDock::new(
            screen_width(),
            screen_height(),
            input.touch_seen() && !demo.autoplay,
        );
        input
            .set_touch_layout(&dock.controls)
            .expect("bounded touch controls");
        let viewport = if dock.inset == 0. {
            Viewport::new(screen_width(), screen_height())
        } else {
            Viewport::with_bottom_inset(screen_width(), screen_height(), dock.inset)
        };
        let pointer = viewport.pointer(Vec2::from(mouse_position()));
        let ready = driver.session.state() == SessionState::Ready;
        let terminal = driver.game.outcome() != Outcome::Playing;
        input.sample(!ready && !terminal && !paused && !help);
        let pause = input.pressed("pause");
        let restart = input.pressed("restart");
        let accept = input.pressed("accept");
        let click = input.pressed("menu_click");
        if input.pressed("mute") {
            let mut prefs = input.preferences();
            prefs.muted = !prefs.muted;
            let _ = input.set_preferences(prefs);
            let _ = input.save_preferences();
            audio::set_muted(prefs.muted);
        }
        if input.pressed("help") {
            help = !help;
            help_page = 0;
        }
        if pause {
            if help {
                help = false;
            } else if !ready && !terminal {
                paused = !paused;
            }
        }
        let pages = guide.len();
        if help {
            if input.pressed("page_next") {
                help_page = (help_page + 1) % pages;
            }
            if input.pressed("page_prev") {
                help_page = (help_page + pages - 1) % pages;
            }
        }
        let start_rect = PRIMARY;
        let help_rect = SECONDARY;
        let restart_rect = WIDE;
        let start_clicked = click && start_rect.contains(pointer);
        let help_clicked = click && help_rect.contains(pointer);
        let mut changed = false;
        if !input.controls_open() {
            if restart
                || (!help && paused && click && restart_rect.contains(pointer))
                || (!help && terminal && (accept || start_clicked))
            {
                driver.restart();
                paused = false;
                help = false;
                recorded = false;
                changed = true;
                sound.reset();
                music.clear();
                chrome.reset(0);
                let _ = bridge.reset();
            } else if help {
                if accept || start_clicked {
                    help = false;
                    changed = true;
                } else if help_clicked {
                    help_page = (help_page + 1) % pages;
                }
            } else if ready {
                if accept || start_clicked {
                    driver.session.start();
                    changed = true;
                } else if help_clicked {
                    help = true;
                    help_page = 0;
                    changed = true;
                }
            } else if paused {
                if accept || start_clicked {
                    paused = false;
                    changed = true;
                } else if help_clicked {
                    help = true;
                    help_page = 0;
                    changed = true;
                }
            } else if terminal && help_clicked {
                help = true;
                help_page = 0;
                changed = true;
            }
        }
        if changed || paused || help || pause {
            input.sample(false);
        }
        let blocked =
            paused || help || (!input.gameplay_active() && !demo.autoplay) || input.controls_open();
        driver.session.set_paused(blocked);
        // Let an accepted ending cue finish over the results card. Explicit
        // pause/help/settings still discard gameplay voices immediately.
        if (driver.session.is_playing() || driver.game.outcome() != Outcome::Playing)
            && !help
            && !paused
            && !input.controls_open()
        {
            sound.resume();
        } else {
            sound.pause();
            music.clear();
        }
        if !changed {
            driver.advance(
                elapsed,
                |game, tick| {
                    if demo.autoplay {
                        game.bot_input(tick)
                    } else {
                        tick_input(&mut input, pointer)
                    }
                },
                |cue, notes| {
                    chrome.cue(cue);
                    play(&mut sound, cue);
                    music.play(notes);
                },
            );
        }
        let hud = driver.game.hud();
        {
            use ferravex_render::status::{StatusFacts, StatusItem, StatusKind};
            let lifecycle = if help {
                "Help"
            } else if paused || driver.session.is_paused() {
                "Paused"
            } else {
                match driver.session.state() {
                    SessionState::Ready => "Ready",
                    SessionState::Won => "Won",
                    SessionState::Lost => "Lost",
                    _ => "Playing",
                }
            };
            let telemetry = format!("Score {} | executed ticks {}", hud.score, driver.ticks);
            let result = if driver.game.outcome() == Outcome::Playing {
                String::new()
            } else {
                driver.game.summary()
            };
            let items = [
                StatusItem {
                    id: 1,
                    kind: StatusKind::Lifecycle,
                    label: lifecycle,
                },
                StatusItem {
                    id: 2,
                    kind: StatusKind::Objective,
                    label: &hud.objective,
                },
                StatusItem {
                    id: 3,
                    kind: StatusKind::Notice,
                    label: &hud.stage,
                },
                StatusItem {
                    id: 4,
                    kind: StatusKind::Notice,
                    label: &hud.status,
                },
                StatusItem {
                    id: 5,
                    kind: StatusKind::Telemetry,
                    label: &telemetry,
                },
                StatusItem {
                    id: 6,
                    kind: StatusKind::Result,
                    label: &result,
                },
            ];
            let _ = bridge.publish(Some(StatusFacts {
                items: &items,
                ui: None,
                ui_actions: &[],
            }));
            let _ = bridge.publish_browser();
        }
        if driver.game.outcome() != Outcome::Playing && !recorded {
            if let Some(records) = &mut records {
                let time = (driver.game.outcome() == Outcome::Won)
                    .then_some(driver.ticks.saturating_mul(1_000_000) / 60);
                let _ = records.attempt("campaign", 1, u64::from(hud.score), time);
                if driver.game.outcome() == Outcome::Won {
                    let _ = records.attempt("victory", 1, u64::from(hud.score), time);
                }
            }
            recorded = true;
        }
        let best = records
            .as_ref()
            .and_then(|r| r.get("campaign", 1))
            .map(|r| r.score);
        let ready = driver.session.state() == SessionState::Ready;
        let terminal = driver.game.outcome() != Outcome::Playing;
        let overlay = if help {
            Overlay::Help
        } else if ready {
            Overlay::Title
        } else if terminal {
            Overlay::Ended
        } else if paused || (!driver.session.is_playing() && !input.controls_open()) {
            Overlay::Paused
        } else {
            Overlay::None
        };
        let buttons: &[Rect] = match overlay {
            Overlay::Paused => &[start_rect, help_rect, restart_rect],
            Overlay::None => &[],
            _ => &[start_rect, help_rect],
        };
        chrome.update(elapsed as f32, hud.score, overlay, pointer, buttons);
        clear_background(INK);
        viewport.apply_shifted(chrome.shake_offset());
        driver.game.draw();
        viewport.apply();
        chrome.header(meta, &hud);
        chrome.footer(audio::muted());
        match chrome.visible_overlay() {
            Overlay::None => {}
            Overlay::Help => {
                let (a, rise) = chrome.heading(meta, meta.title, "How to play");
                chrome.page_dots(help_page.min(pages - 1), pages, meta, a);
                let body = style(18., with_alpha(PAPER, a));
                for (row, line) in guide[help_page.min(pages - 1)].iter().enumerate() {
                    draw_text_styled(line, LEFT, 238. + row as f32 * 23.4 + rise, body);
                }
                draw_text_styled(
                    if input.touch_seen() {
                        "Tap < and > to turn pages."
                    } else {
                        "Left and Right turn pages \u{b7} H returns to the game"
                    },
                    LEFT,
                    462. + rise,
                    style(14., with_alpha(MUTED, a)),
                );
                chrome.button(0, start_rect, "Return to game", true, meta, a);
                chrome.button(1, help_rect, "Next page  \u{2192}", false, meta, a);
            }
            Overlay::Title => {
                let (a, rise) = chrome.heading(meta, meta.subtitle, meta.title);
                let after = paragraph(
                    meta.help.first().copied().unwrap_or(&hud.objective),
                    252. + rise,
                    19.,
                    4,
                    with_alpha(PAPER, 0.9 * a),
                );
                controls_hint(meta.controls, after.max(300. + rise) + 6., a);
                if let Some(best) = best {
                    chip(
                        &format!(
                            "\u{2605} Best {}",
                            grouped(best.min(u64::from(u32::MAX)) as u32)
                        ),
                        LEFT,
                        428. + rise,
                        14.,
                        with_alpha(meta.accent, a),
                    );
                }
                chrome.button(0, start_rect, "Play", true, meta, a);
                chrome.button(1, help_rect, "How to play", false, meta, a);
            }
            Overlay::Ended => {
                let won = driver.game.outcome() == Outcome::Won;
                let (a, rise) =
                    chrome.heading(meta, &hud.stage, if won { "Victory" } else { "Game over" });
                let _ = paragraph(&hud.status, 248. + rise, 18., 5, with_alpha(PAPER, 0.9 * a));
                draw_text_styled(
                    "FINAL SCORE",
                    LEFT,
                    394. + rise,
                    style(12., with_alpha(MUTED, a)).with_letter_spacing(1.5),
                );
                let score = style(34., with_alpha(meta.accent, a));
                let w = draw_text_styled(&grouped(hud.score), LEFT, 434. + rise, score).width;
                let best = best.unwrap_or(u64::from(hud.score));
                let label = if best <= u64::from(hud.score) && hud.score > 0 {
                    "\u{2605} New best".to_string()
                } else {
                    format!("Best {}", grouped(best.min(u64::from(u32::MAX)) as u32))
                };
                chip(
                    &label,
                    LEFT + w + 18.,
                    410. + rise,
                    14.,
                    with_alpha(meta.accent, a),
                );
                if records.as_ref().is_none_or(|r| r.error().is_some()) {
                    draw_text_styled(
                        "Score could not be saved.",
                        CARD.x + CARD.w - 40. - 170.,
                        434. + rise,
                        style(13., with_alpha(MUTED, a)),
                    );
                }
                chrome.button(0, start_rect, "Play again", true, meta, a);
                chrome.button(1, help_rect, "How to play", false, meta, a);
            }
            Overlay::Paused => {
                let (a, rise) = chrome.heading(meta, meta.title, "Paused");
                if !hud.stage.is_empty() {
                    chip(
                        &hud.stage,
                        LEFT,
                        222. + rise,
                        14.,
                        with_alpha(meta.accent, a),
                    );
                }
                let _ = paragraph(
                    &hud.objective,
                    280. + rise,
                    19.,
                    4,
                    with_alpha(PAPER, 0.9 * a),
                );
                chrome.button(2, restart_rect, "Restart campaign", false, meta, a);
                chrome.button(0, start_rect, "Resume", true, meta, a);
                chrome.button(1, help_rect, "How to play", false, meta, a);
            }
        }
        set_default_camera();
        dock.draw(&input, meta.accent);
        input.draw_controls();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &capture_dir {
            // Explicit smoke capture only; normal play never writes frame files.
            if demo.frame.is_multiple_of(6) {
                let file = path.join(format!("frame-{:05}.png", demo.frame / 6));
                get_screen_data().export_png(file.to_str().expect("UTF-8 capture path"));
            }
        }
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
