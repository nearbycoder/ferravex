use crate::chrome::{
    CARD, Chrome, FooterButton, LEFT, Overlay, PRIMARY, SCORE_AT, SECONDARY, TOAST_AT, WIDE,
    footer_button_at, grouped,
};
use crate::driver::Driver;
use crate::paint::{Viewport, chip, guide_pages, lighten, style, with_alpha};
use crate::*;
use ferravex_core::session::SessionState;
use ferravex_play::tween::{Ease, Motion};
use ferravex_render::shapes;
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_render::{
    audio::{self, GameAudio},
    demo::Demo,
    input::{ActionKind, Binding, InputCode, PlayerInput},
    storage::Records,
};
use std::cell::Cell;

thread_local! {
    static CLOCK: Cell<f32> = const { Cell::new(0.) };
    static POINTER: Cell<Vec2> = const { Cell::new(Vec2::new(-1000., -1000.)) };
}
/// The pointer in logical canvas units for this presentation frame, for
/// hover highlights and previews in `draw`. Rules still read
/// [`Input::pointer`] from executed ticks. It is far off-canvas until the
/// pointer first moves, and during autoplay it is the real mouse, so never
/// let it decide anything.
///
/// ```
/// let p = ferravex_arcade::pointer();
/// assert!(p.x.is_finite() && p.y.is_finite());
/// ```
pub fn pointer() -> Vec2 {
    POINTER.with(Cell::get)
}
/// Presentation seconds since the host started. Smoke runs advance it in
/// fixed 1/60 s steps. Use it in `draw` for idle motion (bobbing markers,
/// pulsing selections, flowing water), never in rules: it keeps running
/// while the game is paused or on the title screen.
///
/// ```
/// let t = ferravex_arcade::clock();
/// assert!(t >= 0.);
/// ```
pub fn clock() -> f32 {
    CLOCK.with(Cell::get)
}

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
        // Clicks on the host's footer buttons never reach the game.
        click: a[7].pressed && footer_button_at(pointer).is_none(),
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
/// The host's own reaction to an accepted cue: felt, not described. Games add
/// local effects at the event's position through [`ArcadeGame::effects`].
fn cue_effects(fx: &mut Juice, cue: Cue, accent: Color, shake_on_bad: bool) {
    match cue {
        Cue::Good => fx.sparkle(vec2(900., 16.), accent),
        Cue::Bad if shake_on_bad => {
            fx.shake(0.42);
            fx.vignette(Color::new(0.9, 0.18, 0.16, 0.32));
        }
        Cue::Bad => {}
        Cue::Win => {
            fx.flash(Color::new(1., 1., 1., 0.16));
            fx.sparkle(vec2(900., 16.), accent);
        }
        Cue::None | Cue::Action => {}
    }
}
/// `"Arrows Move | Act Guard | X Heal"` as `(key, action)` pairs; `Act` is
/// shown as the Space key it stands for.
fn control_pairs(controls: &str) -> Vec<(&str, &str)> {
    controls
        .split('|')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|part| {
            let (key, action) = part.split_once(' ').unwrap_or((part, ""));
            (if key == "Act" { "Space" } else { key }, action.trim())
        })
        .collect()
}
/// One centered row (or two) of key caps with their actions.
fn controls_strip(pairs: &[(&str, &str)], cy: f32, a: f32, ink: Color) {
    use crate::chrome::keycap_width;
    let gap = 22.;
    let max = 760.;
    let mut rows: Vec<(usize, usize, f32)> = Vec::new();
    let (mut from, mut width) = (0, 0.);
    for (i, (key, action)) in pairs.iter().enumerate() {
        let w = keycap_width(key, action);
        if i > from && width + gap + w > max {
            rows.push((from, i, width));
            from = i;
            width = 0.;
        }
        width += if i > from { gap } else { 0. } + w;
    }
    if from < pairs.len() {
        rows.push((from, pairs.len(), width));
    }
    let rows = &rows[..rows.len().min(2)];
    let top = cy - (rows.len() as f32 - 1.) * 16. - 10.;
    // A glass pill keeps the key caps legible over the live demo.
    let widest = rows.iter().map(|r| r.2).fold(0., f32::max);
    if widest > 0. {
        let pill = Rect::new(
            WIDTH * 0.5 - widest * 0.5 - 16.,
            top - 9.,
            widest + 32.,
            rows.len() as f32 * 32. + 6.,
        );
        shapes::fill_rounded_rect(pill, 14., with_alpha(INK, 0.86 * a));
        shapes::stroke_rounded_rect(pill, 14., 1., Color::new(1., 1., 1., 0.08 * a));
    }
    for (row, (from, to, width)) in rows.iter().enumerate() {
        let mut x = WIDTH * 0.5 - width * 0.5;
        for (key, action) in &pairs[*from..*to] {
            x += crate::paint::keycap_faded(key, action, x, top + row as f32 * 32., ink, a) + gap;
        }
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
    // A live, legal bot demonstration plays behind the title screen. It is a
    // separate model with its own effects; it never records, sounds or
    // publishes status, and it is dropped once the real campaign starts.
    let mut attract = (!demo.autoplay).then(|| Driver::<G>::new(true));
    let mut attract_fx = Juice::new();
    // The demo's floating words would collide with the title's copy.
    attract_fx.set_words(false);
    let mut attract_time = 0f32;
    let mut fx = Juice::new();
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
    // Native review knobs (never read in the browser): fast-forward an
    // autoplay run toward its ending, or open a menu for a screenshot.
    #[cfg(not(target_arch = "wasm32"))]
    let fast = if demo.autoplay {
        std::env::var("FERRAVEX_ARCADE_FAST")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1)
            .clamp(1, 240)
    } else {
        1
    };
    #[cfg(target_arch = "wasm32")]
    let fast = 1u32;
    #[cfg(not(target_arch = "wasm32"))]
    let mut view = std::env::var("FERRAVEX_ARCADE_VIEW").ok();
    // `FERRAVEX_ARCADE_POINTER=x,y` pins the pointer (logical units) so
    // screenshots can show hover states and previews.
    #[cfg(not(target_arch = "wasm32"))]
    let pinned_pointer = std::env::var("FERRAVEX_ARCADE_POINTER").ok().and_then(|v| {
        let (x, y) = v.split_once(',')?;
        Some(vec2(x.trim().parse().ok()?, y.trim().parse().ok()?))
    });
    let mut recorded = false;
    let mut paused = false;
    let mut help = false;
    let mut help_page = 0usize;
    let guide = guide_pages(meta.help);
    let pairs = control_pairs(meta.controls);
    let mut chrome = Chrome::new(driver.game.hud().score);
    let mut score_seen = driver.game.hud().score;
    let mut score_gain = 0u32;
    let mut gain_wait = 0f32;
    let mut was_terminal = false;
    loop {
        let elapsed = f64::from(demo.dt());
        let dt = elapsed as f32;
        CLOCK.with(|c| c.set(c.get() + dt));
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
        #[cfg(not(target_arch = "wasm32"))]
        let pointer = pinned_pointer.unwrap_or(pointer);
        if pointer.is_finite() {
            POINTER.with(|p| p.set(pointer));
        }
        let ready = driver.session.state() == SessionState::Ready;
        let terminal = driver.game.outcome() != Outcome::Playing;
        input.sample(!ready && !terminal && !paused && !help);
        let mut pause = input.pressed("pause");
        let restart = input.pressed("restart");
        let accept = input.pressed("accept");
        let mut click = input.pressed("menu_click");
        let mut mute = input.pressed("mute");
        let mut help_key = input.pressed("help");
        // The footer's icon buttons mirror the global keys.
        if click && !input.controls_open() {
            match footer_button_at(pointer) {
                Some(FooterButton::Pause) => pause |= !ready && !terminal,
                Some(FooterButton::Help) => help_key = true,
                Some(FooterButton::Controls) => input.set_controls_open(true),
                Some(FooterButton::Sound) => mute = true,
                None => {}
            }
            click &= footer_button_at(pointer).is_none();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if demo.frame >= 90
            && let Some(request) = view.take()
        {
            let (kind, page) = request.split_once(':').unwrap_or((&request, "0"));
            match kind {
                "help" => {
                    help = true;
                    help_page = page.parse().unwrap_or(0);
                }
                "paused" | "pause" => paused = true,
                _ => {}
            }
        }
        if mute {
            let mut prefs = input.preferences();
            prefs.muted = !prefs.muted;
            let _ = input.set_preferences(prefs);
            let _ = input.save_preferences();
            audio::set_muted(prefs.muted);
        }
        if help_key {
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
        // Page zero shows the controls; the guide follows.
        let pages = guide.len() + 1;
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
                fx.clear();
                score_seen = driver.game.hud().score;
                score_gain = 0;
                was_terminal = false;
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
        let policy = ferravex_render::preferences::feedback_policy();
        for juice in [&mut fx, &mut attract_fx] {
            juice.set_motion(Motion::from(policy));
            juice.set_reduced_flash(policy.reduced_flash());
        }
        for _ in 0..if changed { 0 } else { fast } {
            driver.advance(
                elapsed,
                |game, tick| {
                    if demo.autoplay {
                        game.bot_input(tick)
                    } else {
                        tick_input(&mut input, pointer)
                    }
                },
                |game, cue, notes| {
                    chrome.cue(cue);
                    play(&mut sound, cue);
                    music.play(notes);
                    cue_effects(&mut fx, cue, meta.accent, game.shake_on_bad());
                    game.effects(cue, &mut fx);
                },
            );
        }
        // The attract demo runs only while its title is on screen.
        let ready = driver.session.state() == SessionState::Ready;
        if !ready {
            attract = None;
        }
        if let Some(show) = attract.as_mut().filter(|_| !help && !input.controls_open()) {
            attract_time += dt;
            show.advance(
                elapsed,
                |game, tick| game.bot_input(tick),
                |game, cue, _| {
                    cue_effects(&mut attract_fx, cue, meta.accent, game.shake_on_bad());
                    game.effects(cue, &mut attract_fx);
                },
            );
            attract_fx.update(dt);
            if show.game.outcome() != Outcome::Playing || attract_time > 75. {
                show.restart();
                attract_fx.clear();
                attract_time = 0.;
            }
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
        let terminal = driver.game.outcome() != Outcome::Playing;
        // Celebrate (or mark) the ending once, from the outcome itself.
        if terminal && !was_terminal {
            if driver.game.outcome() == Outcome::Won {
                fx.confetti(vec2(WIDTH * 0.5, 96.), 110);
            } else {
                fx.vignette(Color::new(0.85, 0.15, 0.15, 0.45));
            }
        }
        was_terminal = terminal;
        // Score gains rise from the score as one coalesced "+N".
        if hud.score > score_seen {
            score_gain = score_gain.saturating_add(hud.score - score_seen);
        }
        score_seen = hud.score;
        gain_wait += dt;
        if score_gain > 0 && gain_wait > 0.5 {
            // Rise just left of the star so the gain never crosses the digits.
            let label = format!("+{}", grouped(score_gain));
            let digits = text_width(&grouped(hud.score), TextStyle::modern(30., PAPER));
            let half = text_width(&label, TextStyle::modern(18., PAPER)) * 0.5;
            fx.popup(
                vec2(SCORE_AT.x.min(940. - digits - 46. - half), SCORE_AT.y),
                &label,
                lighten(meta.accent, 0.2),
            );
            score_gain = 0;
            gain_wait = 0.;
        }
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
        // Effects freeze with the game under pause, help and settings.
        if !(paused || help || input.controls_open()) {
            fx.update(dt);
        }
        let buttons: &[Rect] = match overlay {
            Overlay::Paused => &[start_rect, help_rect, restart_rect],
            Overlay::None => &[],
            _ => &[start_rect, help_rect],
        };
        chrome.update(dt, hud.score, overlay, pointer, buttons);
        clear_background(INK);
        let showing_attract =
            overlay == Overlay::Title || chrome.visible_overlay() == Overlay::Title;
        let (scene, scene_fx) = match attract.as_ref().filter(|_| showing_attract) {
            Some(show) => (&show.game, &attract_fx),
            None => (&driver.game, &fx),
        };
        viewport.apply_shifted(scene_fx.shake_offset());
        scene.draw();
        viewport.apply();
        let scene_hud;
        let header_hud = if std::ptr::eq(scene, &driver.game) {
            &hud
        } else {
            scene_hud = scene.hud();
            &scene_hud
        };
        chrome.header(meta, header_hud, scene);
        chrome.footer(audio::muted(), !ready && !terminal, meta);
        viewport.apply_shifted(scene_fx.shake_offset());
        scene_fx.draw();
        viewport.apply();
        scene_fx.draw_flash(Rect::new(0., 0., WIDTH, HEIGHT));
        scene_fx.draw_toast(TOAST_AT);
        match chrome.visible_overlay() {
            Overlay::None => {}
            Overlay::Help => {
                let (a, rise) = chrome.heading(meta, meta.title, "How to play");
                let page = help_page.min(pages - 1);
                chrome.page_dots(page, pages, meta, a);
                if page == 0 {
                    controls_page(&pairs, 236. + rise, a, meta);
                } else {
                    guide_page(&guide[page - 1], 238. + rise, a, meta);
                }
                // Page turning is shown, not explained.
                let hint = 456. + rise;
                let ink = with_alpha(MUTED, a);
                let w = crate::paint::keycap_faded("\u{2190}", "", LEFT, hint - 14., ink, a);
                let w = w + crate::paint::keycap_faded(
                    "\u{2192}",
                    "",
                    LEFT + w + 6.,
                    hint - 14.,
                    ink,
                    a,
                );
                draw_text_styled(
                    &format!("{} / {}", page + 1, pages),
                    LEFT + w + 18.,
                    hint,
                    style(14., ink),
                );
                chrome.button(0, start_rect, Icon::Play, "Back to game", true, meta, a);
                chrome.button(1, help_rect, Icon::ArrowRight, "Next", false, meta, a);
            }
            Overlay::Title => {
                let a = chrome.title_scrim();
                let rise = 18. * (1. - a);
                let eyebrow = TextStyle::modern(15., with_alpha(meta.accent, a))
                    .with_letter_spacing(2.)
                    .with_align(TextAlign::Center);
                draw_text_styled(
                    &meta.subtitle.to_uppercase(),
                    WIDTH * 0.5,
                    178. + rise,
                    eyebrow,
                );
                // The title breathes in on arrival.
                let arrive = Ease::BackOut.apply((chrome.overlay_age() / 0.6).clamp(0., 1.));
                let mut title = TextStyle::modern(60. * (0.9 + 0.1 * arrive), with_alpha(PAPER, a))
                    .with_shadow(Color::new(0., 0., 0., 0.6 * a))
                    .with_align(TextAlign::Center)
                    .with_pixel_snap(arrive >= 1.);
                let width = text_width(meta.title, title);
                if width > 820. {
                    title = title.with_size(title.size * 820. / width);
                }
                // A soft dark halo lifts the title off whatever the demo shows.
                let shown = text_width(meta.title, title);
                shapes::drop_shadow(
                    Rect::new(
                        WIDTH * 0.5 - shown * 0.5 - 24.,
                        196. + rise,
                        shown + 48.,
                        76.,
                    ),
                    38.,
                    shapes::Shadow::soft(with_alpha(INK, 0.75 * a)).blur(44.),
                );
                draw_text_styled(meta.title, WIDTH * 0.5, 252. + rise, title);
                shapes::fill_rounded_rect(
                    Rect::new(WIDTH * 0.5 - 36., 272. + rise, 72., 4.),
                    2.,
                    with_alpha(meta.accent, a),
                );
                if let Some(best) = best {
                    let label = format!("Best {}", grouped(best.min(u64::from(u32::MAX)) as u32));
                    // The chip's own width formula, so it centers exactly.
                    let w = text_width(&label, style(15., PAPER)) + 15. * 2.5;
                    crate::paint::icon_chip(
                        Icon::Trophy,
                        &label,
                        WIDTH * 0.5 - w * 0.5,
                        290. + rise,
                        15.,
                        with_alpha(meta.accent, a),
                    );
                }
                controls_strip(&pairs, 420. + rise, a, Color::new(0.8, 0.85, 0.9, 1.));
                chrome.button(0, start_rect, Icon::Play, "Play", true, meta, a);
                chrome.button(1, help_rect, Icon::Question, "How to play", false, meta, a);
            }
            Overlay::Ended => {
                let won = driver.game.outcome() == Outcome::Won;
                let (a, rise) =
                    chrome.heading(meta, &hud.stage, if won { "Victory" } else { "Game over" });
                // A large emblem makes the outcome legible at a glance.
                let emblem = vec2(CARD.x + CARD.w - 96., 150. + rise);
                let pop = Ease::BackOut.apply((chrome.overlay_age() / 0.5).clamp(0., 1.));
                if won {
                    shapes::glow_disc(emblem, 70. * pop, with_alpha(meta.accent, 0.35 * a));
                    icons::draw_styled(
                        Icon::Trophy,
                        emblem,
                        76. * pop,
                        IconStyle::new(with_alpha(meta.accent, a)).shadow(Color::new(
                            0.,
                            0.,
                            0.,
                            0.45 * a,
                        )),
                    );
                } else {
                    icons::draw_styled(
                        Icon::Restart,
                        emblem,
                        64. * pop,
                        IconStyle::new(Color::new(0.94, 0.45, 0.42, a)).shadow(Color::new(
                            0.,
                            0.,
                            0.,
                            0.45 * a,
                        )),
                    );
                }
                let area = Rect::new(LEFT, 214. + rise, CARD.w - 80., 150.);
                if driver.game.draw_result(area) {
                    // Games draw at full strength; a veil in the card's own
                    // tone lets their results fade in with the card.
                    if a < 0.999 {
                        shapes::fill_rounded_rect(
                            Rect::new(area.x - 8., area.y - 8., area.w + 16., area.h + 16.),
                            8.,
                            Color::new(0.07, 0.092, 0.126, 1. - a),
                        );
                    }
                } else {
                    let _ = paragraph(&hud.status, 248. + rise, 18., 4, with_alpha(PAPER, 0.9 * a));
                }
                // The final score counts up once, then settles.
                let count = Ease::CubicOut.apply((chrome.overlay_age() / 1.1).clamp(0., 1.));
                let shown = (hud.score as f32 * count).round() as u32;
                icons::draw_styled(
                    Icon::Star,
                    vec2(LEFT + 15., 420. + rise),
                    28.,
                    IconStyle::new(with_alpha(meta.accent, a)).shadow(Color::new(
                        0.,
                        0.,
                        0.,
                        0.4 * a,
                    )),
                );
                let score = style(34., with_alpha(meta.accent, a));
                let w = draw_text_styled(&grouped(shown), LEFT + 38., 434. + rise, score).width;
                let best = best.unwrap_or(u64::from(hud.score));
                let new_best = best <= u64::from(hud.score) && hud.score > 0;
                let label = if new_best {
                    "New best!".to_string()
                } else {
                    format!("Best {}", grouped(best.min(u64::from(u32::MAX)) as u32))
                };
                let x = LEFT + 38. + w + 18.;
                let cw = crate::paint::icon_chip(
                    Icon::Trophy,
                    &label,
                    x,
                    410. + rise,
                    14.,
                    with_alpha(meta.accent, a),
                );
                if new_best && count >= 1. {
                    let glint = 0.5 + 0.5 * (chrome.time() * 5.).sin();
                    shapes::glow_disc(
                        vec2(x + cw * 0.5, 420. + rise),
                        cw * 0.6,
                        with_alpha(meta.accent, 0.12 * glint * a),
                    );
                }
                if records.as_ref().is_none_or(|r| r.error().is_some()) {
                    icons::draw(
                        Icon::Alert,
                        vec2(CARD.x + CARD.w - 50., 428. + rise),
                        18.,
                        with_alpha(MUTED, a),
                    );
                }
                chrome.button(0, start_rect, Icon::Restart, "Play again", true, meta, a);
                chrome.button(1, help_rect, Icon::Question, "How to play", false, meta, a);
            }
            Overlay::Paused => {
                let (a, rise) = chrome.heading(meta, meta.title, "Paused");
                let mut x = LEFT;
                if let Some((current, total)) = driver.game.progress().filter(|(_, t)| *t > 0) {
                    x += crate::paint::pips(
                        x,
                        236. + rise,
                        current,
                        total,
                        with_alpha(meta.accent, a),
                    ) + 14.;
                }
                if !hud.stage.is_empty() {
                    chip(&hud.stage, x, 226. + rise, 14., with_alpha(meta.accent, a));
                }
                if !hud.objective.is_empty() {
                    icons::draw(
                        Icon::Target,
                        vec2(LEFT + 9., 280. + rise),
                        18.,
                        with_alpha(meta.accent, a),
                    );
                    let style = style(19., with_alpha(PAPER, 0.9 * a));
                    let mut lines = wrap_text(&hud.objective, CARD.w - 110., style);
                    for row in 0..3 {
                        let Some(line) = lines.next() else { break };
                        draw_text_styled(line, LEFT + 28., 287. + rise + row as f32 * 25., style);
                    }
                }
                chrome.button(
                    2,
                    restart_rect,
                    Icon::Restart,
                    "Restart campaign",
                    false,
                    meta,
                    a,
                );
                chrome.button(0, start_rect, Icon::Play, "Resume", true, meta, a);
                chrome.button(1, help_rect, Icon::Question, "How to play", false, meta, a);
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
/// The first help page: every control as a key cap and its action.
fn controls_page(pairs: &[(&str, &str)], top: f32, a: f32, meta: Meta) {
    let two = pairs.len() > 5;
    for (i, (key, action)) in pairs.iter().enumerate() {
        let (col, row) = if two { (i % 2, i / 2) } else { (0, i) };
        let x = LEFT + col as f32 * 310.;
        let y = top + row as f32 * 36.;
        let w = crate::paint::keycap_faded(key, "", x, y - 15., PAPER, a);
        draw_text_styled(
            action,
            x + w.max(64.) + 14.,
            y,
            style(18., with_alpha(lighten(meta.accent, 0.5), a)),
        );
    }
}
/// A guide page: paragraph starts get an accent dot instead of a heading.
fn guide_page(lines: &[&str], top: f32, a: f32, meta: Meta) {
    let body = style(18., with_alpha(PAPER, a));
    let mut start = true;
    for (row, line) in lines.iter().enumerate() {
        let y = top + row as f32 * 23.4;
        if line.is_empty() {
            start = true;
            continue;
        }
        if start {
            shapes::fill_circle(vec2(LEFT - 14., y - 6.), 3.5, with_alpha(meta.accent, a));
            start = false;
        }
        draw_text_styled(line, LEFT, y, body);
    }
}
