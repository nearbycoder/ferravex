//! A scene-authored tower defense game using the shared session lifecycle.
//!
//! Preparation and safe checkpoint loading happen before the first wave starts.
//! The same lifecycle is available to hosts independently of rendering:
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(60., 8, false);
//! assert_eq!(session.advance(10.).steps, 0); // Build before sending the wave.
//! assert!(session.start());
//! assert!(session.set_paused(true));
//! assert_eq!(session.advance(30.).steps, 0); // No paused catch-up.
//! session.restart();
//! assert_eq!(session.state(), SessionState::Playing);
//! assert_eq!(session.tick(), 0);
//! ```
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod compact;
mod game;
mod host;
mod hud;
#[cfg(test)]
mod journey200_tests;
mod juice;
mod level;
mod look;
mod presentation;
mod quips;
mod save;
mod scene;
mod style;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{ActionKind, Binding, InputCode, PlayerInput, Preset};
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect, Viewport};
use ferravex_render::{canvas::Canvas2D, demo::Demo, prelude::*};
use game::*;
use host::{Action, Campaign};
use presentation::Presentation;
use std::sync::Arc;

const SIZE: Vec2 = vec2(1200., 760.);
const INK: Color = Color::new(0.19, 0.25, 0.19, 1.);
const CREAM: Color = Color::new(0.98, 0.95, 0.84, 1.);
const GREEN: Color = Color::new(0.23, 0.39, 0.24, 1.);
const GOLD: Color = Color::new(0.88, 0.62, 0.22, 1.);
fn conf() -> Conf {
    let mut c = ferravex_render::window_conf("Orchard Guard · Ferravex");
    c.window_width = 1200;
    c.window_height = 760;
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut c);
    c
}
fn label(s: &str, x: f32, y: f32, size: f32, color: Color) {
    ferravex_render::text::draw_text_styled(s, x, y, style::body(size, color));
}
fn center(s: &str, x: f32, y: f32, size: f32, color: Color) {
    style::text_center(s, x, y, style::body(size, color));
}
fn button(ui: &mut Ui, id: u64, rect: Rect, title: &str, enabled: bool, size: f32) {
    style::control(ui, id, rect, title, enabled, size);
}

fn availability(status: Result<(), ActionError>, available: &str) -> String {
    match status {
        Ok(()) => available.into(),
        Err(ActionError::Funds { missing }) => format!("NEED {missing} MORE COINS"),
        Err(ActionError::FullyGrown) => "MAXED OUT. LOOKING GOOD.".into(),
        Err(ActionError::Occupied) => "SELECT AN EMPTY PLOT".into(),
        Err(ActionError::Empty) => "BUILD A TOWER FIRST".into(),
        Err(_) => "UNAVAILABLE RIGHT NOW".into(),
    }
}
#[cfg(test)]
fn hint(input: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 256];
    input
        .format_hint(action, &mut buffer)
        .unwrap_or("See Controls")
        .into()
}

fn take_command(input: &mut PlayerInput) -> Option<Action> {
    let edges = input.take_tick(["select_one", "select_two", "upgrade", "wave"]);
    edges
        .into_iter()
        .zip([
            Action::Build(Kind::Seed),
            Action::Build(Kind::Frost),
            Action::Upgrade,
            Action::Wave,
        ])
        .find_map(|(edge, action)| edge.pressed.then_some(action))
}

fn hud_action(ui: &Ui, plots: usize) -> Option<Action> {
    [
        (hud::SPEED_ID, Action::Speed),
        (5, Action::Restart),
        (8, Action::Restart),
        (6, Action::Pause),
        (7, Action::Pause),
        (1, Action::Build(Kind::Seed)),
        (2, Action::Build(Kind::Frost)),
        (3, Action::Upgrade),
        (4, Action::Wave),
    ]
    .into_iter()
    .find_map(|(id, action)| ui.activated(id).then_some(action))
    .or_else(|| {
        (0..plots).find_map(|i| {
            ui.activated(compact::PLOT_ID + i as u64)
                .then_some(Action::Select(Some(i)))
        })
    })
}

fn notice_text(message: &str) -> &str {
    message
        .split(". ")
        .next()
        .unwrap_or(message)
        .trim_end_matches('.')
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::with_content();
    let mut content =
        ferravex_render::content::ContentSource::new(level::SOURCE, demo.content.as_deref())
            .unwrap();
    let mut g = Game::with_level(Arc::new(level::Level::parse(content.current()).unwrap()));
    let mut progress = save::Progress::new(
        &mut g,
        ferravex_play::persistence::checksum(content.current().as_bytes()),
    );
    let mut campaign = Campaign::new(g, demo.autoplay);
    let mut sound = GameAudio::new();
    let mut presentation = Presentation::default();
    let mut juice = juice::Juice::new();
    let mut daylight = look::DayLight::new();
    let mut particles = ferravex_render::effects::ParticleRenderer::new(320)
        .expect("bounded presentation particles");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    let mut player_input = PlayerInput::preset("orchard-guard", Preset::Strategy);
    player_input
        .action(
            "speed",
            "Fast-forward",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Key(KeyCode::F as u16))],
        )
        .unwrap();
    let mut hud_ui = Ui::new();
    hud_ui.set_auto_focus(false);
    let mut plot_picker = false;
    loop {
        let reloaded = if let Some(next) = content.poll(demo.dt(), |text| {
            Ok(Game::with_level(Arc::new(level::Level::parse(text)?)))
        }) {
            campaign = Campaign::new(next, demo.autoplay);
            progress.content_changed(
                &campaign.game,
                ferravex_play::persistence::checksum(content.current().as_bytes()),
            );
            hud_ui.cancel();
            sound = GameAudio::new();
            presentation = Presentation::default();
            juice.reset();
            daylight.reset();
            true
        } else {
            false
        };
        style::frame(demo.dt(), &campaign);
        let screen = vec2(screen_width().max(1.), screen_height().max(1.));
        let compact = (280. ..900.)
            .contains(&screen.x)
            .then(|| compact::Layout::new(screen));
        if compact.is_none() || reloaded {
            plot_picker = false;
        }
        let canvas = Canvas2D::fit(SIZE, screen);
        // Keep resume/restart UI interactive while the session owns the pause.
        player_input.sample(true);
        let blocked = !player_input.gameplay_active();
        campaign.suspend(blocked && !demo.autoplay);
        let cursor = player_input.cursor(demo.dt(), 600.);
        let touch = ferravex_render::input::logical_touches()
            .into_iter()
            .find(|t| t.phase == TouchPhase::Started);
        let point = touch.as_ref().map_or_else(|| cursor, |t| t.position);
        let pointer = compact.map_or_else(|| canvas.pointer(point), |layout| layout.pointer(point));
        let click = (player_input.pressed("primary") || touch.is_some())
            && !blocked
            && !(player_input.cursor_is_virtual() && hud_ui.focused().is_some());
        // Lifecycle edges are render-frame commands. Combat shortcuts remain
        // pending in PlayerInput until Campaign calls take_command on a tick.
        let shortcuts = [
            (player_input.pressed("restart"), Action::Restart),
            (player_input.pressed("pause"), Action::Pause),
            (player_input.pressed("speed"), Action::Speed),
        ];
        #[allow(unused_mut)]
        let mut keyboard = shortcuts
            .into_iter()
            .find_map(|(pressed, action)| pressed.then_some(action));
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if let Some(action) = capture.action(&campaign) {
            keyboard = Some(action);
        }
        let plot = if click
            && !plot_picker
            && campaign.game.outcome == Outcome::Playing
            && !campaign.session.is_paused()
        {
            pointer.filter(|p| (70. ..660.).contains(&p.y)).map(|p| {
                Action::Select(
                    campaign
                        .game
                        .level
                        .plots
                        .iter()
                        .position(|plot| p.distance(*plot) < 30.),
                )
            })
        } else {
            None
        };
        clear_background(Color::from_hex(0x27372a));
        presentation.frame(&campaign.game, demo.dt());
        juice.apply_policy(ferravex_render::preferences::feedback_policy());
        daylight.frame(
            &campaign.game,
            !campaign.session.is_playing(),
            demo.dt(),
            ferravex_play::tween::Motion::from(ferravex_render::preferences::feedback_policy()),
        );
        let base_camera = compact.map_or_else(|| canvas.camera(), |layout| layout.camera());
        let mut shaken = base_camera;
        shaken.target -= juice.shake();
        set_camera(&shaken);
        let boost = compact.map_or(1., |layout| layout.text_boost());
        scene::field(
            &campaign.game,
            &presentation,
            &juice,
            &daylight,
            compact.is_some(),
            boost,
            pointer.and_then(|p| {
                campaign
                    .game
                    .level
                    .plots
                    .iter()
                    .position(|plot| p.distance(*plot) < 30.)
            }),
        );
        let finished = campaign.game.outcome != Outcome::Playing;
        let mut draw_particles = || {
            particles
                .draw_2d(
                    &juice.particles,
                    ferravex_render::effects::ParticleStyle {
                        stretch: 0.004,
                        max_stretch: 2.5,
                        ..Default::default()
                    },
                )
                .expect("authored particle style");
        };
        if !finished {
            draw_particles();
            scene::pops(&juice, boost);
        }
        set_camera(&base_camera);
        // Mask the letterbox so entering pests, clouds and shake never spill
        // outside the authored board.
        let bg = Color::from_hex(0x27372a);
        draw_rectangle(-4000., -4000., 4000., 9000., bg);
        draw_rectangle(1200., -4000., 4000., 9000., bg);
        draw_rectangle(0., -4000., 1200., 4070., bg);
        draw_rectangle(0., 660., 1200., 4000., bg);
        ferravex_render::set_world_colors(false);
        let best = progress.best();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        let best = capture.best(best, campaign.game.kills);
        if let Some(layout) = compact {
            hud_ui.begin(Viewport::responsive([screen.x, screen.y], [0.; 4]).unwrap());
            compact::hud(
                &mut hud_ui,
                layout,
                &campaign,
                &juice,
                progress.error().is_some(),
                &player_input,
                plot_picker,
            );
            compact::overlay(&mut hud_ui, layout, &campaign, best, &player_input);
        } else {
            hud_ui.begin(Viewport::fit([screen.x, screen.y], [1200., 760.], [0.; 4]).unwrap());
            hud::hud(
                &mut hud_ui,
                &campaign,
                &juice,
                progress.error().is_some(),
                &player_input,
            );
            hud::overlay(&mut hud_ui, &campaign, best, &player_input, draw_particles);
        }
        hud_ui.set_auto_focus(
            campaign.session.is_paused() || campaign.game.outcome != Outcome::Playing,
        );
        hud_ui.finish_player(&mut player_input, !blocked && !reloaded);
        hud_ui.paint();
        set_default_camera();
        style::paint(&mut hud_ui);
        let ui_action = hud_action(&hud_ui, campaign.game.level.plots.len());
        if hud_ui.activated(9) {
            plot_picker = !plot_picker;
        }
        if matches!(ui_action, Some(Action::Select(_))) {
            plot_picker = false;
        }
        let previous_state = campaign.session.state();
        #[allow(unused_mut)]
        let mut elapsed = if reloaded { 0. } else { demo.dt() as f64 };
        #[allow(unused_mut)]
        let mut autoplay = demo.autoplay;
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        {
            elapsed *= capture.turbo();
            autoplay &= !capture.defenseless();
        }
        let events = campaign.frame_with_input(
            elapsed,
            blocked || reloaded,
            autoplay,
            [keyboard, ui_action, plot],
            || take_command(&mut player_input),
        );
        if events.restarted {
            plot_picker = false;
            progress.restart(&campaign.game);
            hud_ui.cancel();
            sound = GameAudio::new();
            presentation = Presentation::default();
            juice.reset();
            campaign.game.feed.clear();
        } else {
            if campaign.session.state() != previous_state {
                hud_ui.cancel();
            }
            route_feed(&mut campaign.game, &mut juice, &mut sound);
            if events.escaped && events.finished != Some(false) {
                sound.cue(Cue::Failure, 0.3);
            }
            if let Some(won) = events.finished {
                sound.cue(if won { Cue::Success } else { Cue::Failure }, 0.65);
            }
        }
        // Effects age with presentation time but freeze while paused.
        let paused = campaign.session.is_paused();
        let rate = if campaign.fast { 2. } else { 1. };
        juice.advance(
            if paused { 0. } else { demo.dt() * rate },
            campaign.game.outcome != Outcome::Playing,
        );
        progress.sync(&campaign.game);
        if player_input.cursor_is_virtual() {
            if compact.is_some() {
                set_default_camera();
                draw_circle_lines(cursor.x, cursor.y, 6., 1.5, CREAM);
            } else {
                let point = canvas.unproject(cursor);
                draw_circle_lines(point.x, point.y, 6., 1.5, CREAM);
            }
        }
        set_default_camera();
        player_input.draw_controls();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

/// Drain this frame's accepted simulation events into juice and audio. Audio
/// coalesces per frame so catch-up batches cannot stack identical voices.
fn route_feed(game: &mut Game, juice: &mut juice::Juice, sound: &mut GameAudio) {
    let mut fired = false;
    let mut lucky = false;
    let mut stopped = false;
    let mut built = false;
    let mut milestone = false;
    let events = std::mem::take(&mut game.feed);
    for &event in &events {
        juice.on_event(game, event);
        match event {
            Feed::Fired { lucky: l, .. } => {
                fired = true;
                lucky |= l;
            }
            Feed::Stopped { .. } => stopped = true,
            Feed::Planted { .. } | Feed::Grown { .. } => built = true,
            Feed::Cleared { .. } => milestone = true,
            _ => {}
        }
    }
    // Hand the allocation back so the bounded log never reallocates.
    game.feed = events;
    game.feed.clear();
    if lucky {
        sound.cue(Cue::Fire, 0.35);
    } else if fired {
        sound.cue(Cue::Click, 0.5);
    }
    if stopped {
        sound.cue(Cue::Impact(Surface::Grass), 0.4);
        sound.cue(Cue::Pickup, 0.18);
    }
    if built {
        sound.cue(Cue::Reload, 0.45);
    }
    if milestone {
        sound.cue(Cue::Success, 0.4);
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use ferravex_render::input::{self, ActionKind, Binding, InputCode, PadState};

    #[test]
    fn short_notices_keep_the_event_and_exact_rejection_amounts() {
        assert_eq!(
            notice_text("Wave 2 sent. You can build and upgrade during the wave."),
            "Wave 2 sent"
        );
        assert_eq!(notice_text("45 more coins needed."), "45 more coins needed");
        assert_eq!(
            notice_text("Speed 2.5 / 12 hearts"),
            "Speed 2.5 / 12 hearts"
        );
    }

    #[test]
    fn hints_follow_remapping_reset_and_unbound_actions_without_stale_defaults() {
        let mut input = PlayerInput::transient("orchard-hints-test");
        input
            .action(
                "upgrade",
                "Upgrade",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(KeyCode::U as u16))],
            )
            .unwrap();
        assert_eq!(hint(&input, "upgrade"), "U");
        input
            .rebind(
                "upgrade",
                vec![Binding::digital(InputCode::Key(KeyCode::J as u16))],
            )
            .unwrap();
        assert_eq!(hint(&input, "upgrade"), "J");
        input.rebind("upgrade", vec![]).unwrap();
        assert_eq!(hint(&input, "upgrade"), "Unbound");
        input.reset_defaults();
        assert_eq!(hint(&input, "upgrade"), "U");
    }

    #[test]
    fn real_player_tap_survives_zero_tick_frames_and_is_spent_once_during_catch_up() {
        let mut input = PlayerInput::transient("orchard-tick-test");
        input
            .action(
                "upgrade",
                "Upgrade",
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(5))],
            )
            .unwrap();
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        input::gamepad_sample(0, pad);
        input::host_frame(0., 1200, 760, || input.sample(true));
        let mut campaign = Campaign::new(Game::new(), false);
        campaign.game.money = 1000;
        campaign.game.build(0, Kind::Seed);
        campaign.game.launch();
        campaign.session.start();
        pad.buttons[5] = 1.;
        input::gamepad_sample(0, pad);
        pad.buttons[5] = 0.;
        input::gamepad_sample(0, pad);
        for elapsed in [0., 0., 0.1] {
            input::host_frame(elapsed, 1200, 760, || {
                input.sample(true);
                campaign.frame_with_input(f64::from(elapsed), false, false, [None; 3], || {
                    take_command(&mut input)
                });
            });
            assert_eq!(
                campaign.game.towers[0].unwrap().level,
                u8::from(elapsed > 0.)
            );
        }
        assert_eq!(campaign.game.money, 1000 - 60 - 65);
        assert_eq!(hint(&input, "upgrade"), "Pad 5");
        input::gamepad_sample(0, PadState::default());
    }
}
