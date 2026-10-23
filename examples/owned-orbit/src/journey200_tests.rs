//! Task 175: scenarios reach every game state through ordinary controls.
//! Private-module access observes the existing clock and accepted audio; it
//! never mutates game fields, advances the clock directly, or injects events.

use super::*;
use ferravex_engine::play::audio::Cue;
use game::{Feedback, Phase};

const DT: f64 = 1.0 / 120.0;

/// Exact same-target gameplay observations, excluding frame-time cosmetics.
#[derive(Debug, PartialEq)]
struct State {
    phase: Phase,
    sector: u32,
    lives: u32,
    score: u32,
    clears: u32,
    hits: u32,
    impacts: u64,
    time: f32,
    ball: [f32; 2],
    velocity: [f32; 2],
    paddle: [f32; 2],
    charge: [f32; 2],
    armor_piercing: bool,
    save: (u32, f32),
    wide: f32,
    transition: f32,
    chain: (u32, u32),
    precision: (u32, u32),
    panels: Vec<u8>,
    drops: Vec<[f32; 2]>,
    feedback: Option<Feedback>,
}

fn state(app: &Orbit) -> State {
    let g = &app.game;
    State {
        phase: g.phase,
        sector: g.sector,
        lives: g.lives,
        score: g.score,
        clears: g.clears,
        hits: g.hits,
        impacts: g.ccd_impacts,
        time: g.time,
        ball: g.ball,
        velocity: g.velocity,
        paddle: [g.paddle_x, g.paddle_half],
        charge: [g.serve_charge.power(), g.serve_power],
        armor_piercing: g.armor_piercing(),
        save: (g.saves, g.ball_save),
        wide: g.wide_time,
        transition: g.transition_remaining(),
        chain: (g.chain.multiplier(), g.chain.best()),
        precision: (g.precision.streak(), g.precision.best()),
        panels: g.blocks.iter().map(|b| b.health).collect(),
        drops: g.drops.iter().map(|d| d.position).collect(),
        feedback: g.feedback(),
    }
}

fn pause_round_trip(app: &mut Orbit) {
    let before = state(app);
    let tick = app.session.tick();
    assert!(app.set_paused(true));
    assert_eq!(app.session_state(), SessionState::Paused);
    for dt in [0., DT / 2., 30.] {
        app.advance_input(
            dt,
            Input {
                axis: 1.,
                launch: true,
                restart: false,
            },
        );
        assert_eq!(state(app), before);
        assert_eq!(app.session.tick(), tick);
        assert_eq!(app.presentation.take_cues().count(), 0);
    }
    assert!(app.set_paused(false));
    assert_eq!(app.session_state(), SessionState::Playing);
    app.advance_input(0., Input::default());
    assert_eq!(state(app), before);
    assert!(!app.pending_launch);
}

fn assert_terminal_stays_put(app: &mut Orbit, expected: SessionState) {
    assert_eq!(app.session_state(), expected);
    let before = state(app);
    let tick = app.session.tick();
    assert!(!app.set_paused(true));
    for dt in [0., DT / 2., 12. * DT, 30.] {
        app.advance_input(
            dt,
            Input {
                axis: -1.,
                ..Input::default()
            },
        );
        assert_eq!(state(app), before);
        assert_eq!(app.session.tick(), tick);
        assert_eq!(app.presentation.take_cues().count(), 0);
    }
}

#[derive(Debug, PartialEq)]
struct Campaign {
    checkpoints: Vec<State>,
    result: State,
    launch_powers: Vec<f32>,
}

fn campaign(fps: u32) -> Campaign {
    let mut app = Orbit::new();
    let mut checkpoints = Vec::new();
    let mut last_sample = None;
    let mut held = Input::default();
    let mut ready_since = None;
    let mut launch_powers = Vec::new();
    let mut paused_transition = false;
    let mut reinforced_sector = false;
    let mut picked_up = false;
    let mut success_cues = 0;
    let mut zero_frames = 0;
    let mut catchup_frames = 0;

    // Sample the real autoplay controller every four simulation ticks at ALL
    // frame rates. Otherwise differing control sampling, rather than the clock,
    // would legitimately produce different trajectories. At 30/60/144 Hz no
    // frame crosses a four-tick input boundary without visiting it.
    for _ in 0..fps * 600 {
        let tick = app.session.tick();
        let mut launch = false;
        if tick.is_multiple_of(4) && last_sample != Some(tick) {
            last_sample = Some(tick);
            if tick.is_multiple_of(60) {
                checkpoints.push(state(&app));
            }
            if app.game.phase == Phase::Transition && !paused_transition {
                assert_eq!(app.game.clears, 35);
                assert_eq!(app.game.remaining(), 0);
                assert!(app.game.score >= 3500);
                pause_round_trip(&mut app);
                paused_transition = true;
            }
            held = app.game.autoplay();
            if app.game.phase == Phase::Ready {
                let since = *ready_since.get_or_insert(tick);
                // Charge through actual ready ticks before each serve, including
                // the naturally built second sector's reinforced panels.
                launch = tick - since >= 240;
                if app.game.sector == 2 {
                    reinforced_sector |= app.game.blocks.iter().any(|b| b.health == 2);
                }
            } else {
                ready_since = None;
            }
        }
        app.advance_input(1. / f64::from(fps), Input { launch, ..held });
        let steps = app.session.tick() - tick;
        zero_frames += u32::from(steps == 0);
        catchup_frames += u32::from(steps > 1);
        picked_up |= app.game.wide_time > 0.;
        for (cue, _) in app.presentation.take_cues() {
            if cue == Cue::Jump {
                launch_powers.push(app.game.serve_power);
            }
            success_cues += u32::from(cue == Cue::Success);
        }
        assert!(app.game.events.is_empty());
        if matches!(app.game.phase, Phase::Won | Phase::Lost) {
            break;
        }
    }
    assert_eq!(app.game.phase, Phase::Won, "{fps} Hz: {:?}", state(&app));
    assert!(paused_transition && reinforced_sector && picked_up);
    assert_eq!(app.game.sector, 2);
    assert_eq!(app.game.clears, 70);
    assert_eq!(app.game.remaining(), 0);
    assert!(app.game.hits >= 70 && app.game.ccd_impacts > 70);
    assert!(app.game.score >= 7000);
    assert_eq!(success_cues, 2);
    assert!(launch_powers.len() >= 2);
    assert!(launch_powers.iter().all(|&power| power == 1.));
    assert_eq!(zero_frames > 0, fps == 144);
    assert_eq!(catchup_frames > 0, fps < 120);
    let result = state(&app);
    assert_terminal_stays_put(&mut app, SessionState::Won);

    // A retry press resets an earned victory immediately but cannot serve the
    // new ball, even when the retry frame supplies a full catch-up budget.
    app.advance_input(
        12. * DT,
        Input {
            launch: true,
            ..Input::default()
        },
    );
    assert_fresh_run(&mut app);
    app.advance_input(DT, Input::default());
    assert_eq!(app.game.phase, Phase::Ready);
    app.advance_input(
        0.,
        Input {
            launch: true,
            ..Input::default()
        },
    );
    app.advance_input(DT, Input::default());
    assert_eq!(app.game.phase, Phase::Playing);
    Campaign {
        checkpoints,
        result,
        launch_powers,
    }
}

fn assert_fresh_run(app: &mut Orbit) {
    assert_eq!(app.session_state(), SessionState::Playing);
    assert_eq!(app.session.tick(), 0);
    assert_eq!(app.game.phase, Phase::Ready);
    assert_eq!((app.game.sector, app.game.lives, app.game.score), (1, 3, 0));
    assert_eq!((app.game.clears, app.game.hits, app.game.saves), (0, 0, 0));
    assert_eq!(
        (app.game.time, app.game.wide_time, app.game.serve_power),
        (0., 0., 0.)
    );
    assert_eq!(app.game.serve_charge.power(), 0.);
    assert_eq!(app.game.remaining(), 35);
    assert!(
        app.game
            .blocks
            .iter()
            .all(|b| b.health == b.original_health)
    );
    assert!(app.game.drops.is_empty() && app.game.trail.is_empty());
    assert!(app.game.particles.is_empty() && app.game.events.is_empty());
    assert_eq!(app.game.feedback(), Some(Feedback::Restarted));
    assert_eq!(app.presentation.take_cues().count(), 0);
    assert_eq!(app.presentation.notice, None);
}

#[test]
fn journey200_full_charged_campaign_is_identical_at_30_60_and_144_hz() {
    let reference = campaign(30);
    for fps in [60, 144] {
        assert_eq!(campaign(fps), reference, "supplied {fps} Hz schedule");
    }
}

fn parked_paddle_loss(fps: u32) -> (Orbit, Vec<State>) {
    let mut app = Orbit::new();
    let mut checkpoints = Vec::new();
    // Hold right; serve at fixed wall-time boundaries after charging. The ball
    // must actually strike panels, use any opening save, and miss the paddle.
    for frame in 0..fps * 32 {
        let launch = [2, 12, 22].into_iter().any(|second| frame == fps * second);
        app.advance_input(
            1. / f64::from(fps),
            Input {
                axis: 1.,
                launch,
                restart: false,
            },
        );
        if (frame + 1) % (fps / 2) == 0 {
            checkpoints.push(state(&app));
        }
    }
    assert_eq!(
        app.session_state(),
        SessionState::Lost,
        "{fps} Hz: {:?}",
        state(&app)
    );
    assert_eq!(app.game.lives, 0);
    assert!(app.game.score > 0 && app.game.clears > 0 && app.game.ccd_impacts > 0);
    assert_eq!(app.game.feedback(), Some(Feedback::LifeLost));
    (app, checkpoints)
}

#[test]
fn journey200_real_loss_is_rate_independent_and_held_retry_needs_a_new_press() {
    let (mut reference, trace) = parked_paddle_loss(30);
    for fps in [60, 144] {
        let (mut app, other_trace) = parked_paddle_loss(fps);
        assert_eq!(other_trace, trace, "supplied {fps} Hz schedule");
        assert_eq!(state(&app), state(&reference));
        assert_terminal_stays_put(&mut app, SessionState::Lost);
    }
    assert_terminal_stays_put(&mut reference, SessionState::Lost);
    reference.advance(12. * DT, 0., 1);
    assert_fresh_run(&mut reference);
    reference.advance(12. * DT, 0., 1);
    assert_eq!(reference.game.phase, Phase::Ready);
    assert_eq!(reference.session.tick(), 12);
    reference.advance(0., 0., 0);
    reference.advance(0., 0., 1);
    reference.advance(12. * DT, 0., 0);
    assert_eq!(reference.game.phase, Phase::Playing);
    assert_eq!(reference.session.tick(), 24);
    assert_eq!(
        reference
            .presentation
            .take_cues()
            .filter(|(cue, _)| *cue == Cue::Jump)
            .count(),
        1
    );
}

/// Stop just before a real missed return, after the opening save has expired
/// or been consumed. Reading the ball chooses a test observation boundary;
/// every preceding position/collision/reward still comes from normal controls.
fn approach_first_miss() -> Orbit {
    let mut app = Orbit::new();
    for _ in 0..120 {
        app.advance(1. / 60., 1., 0);
    }
    app.advance(0., 1., 1);
    app.advance(DT, 1., 0);
    for _ in 0..120 * 20 {
        if app.game.ball[1] > 650.
            && app.game.velocity[1] > 0.
            && (677. - app.game.ball[1]) / app.game.velocity[1] < 8. * DT as f32
            && app.game.ball_save == 0.
        {
            assert_eq!(app.game.phase, Phase::Playing);
            assert_eq!(app.game.lives, 3);
            assert!(app.game.score > 0 && app.game.clears > 0);
            return app;
        }
        app.advance(DT, 1., 0);
    }
    panic!(
        "parked paddle did not reach the expected missed return: {:?}",
        state(&app)
    );
}

#[test]
fn journey200_zero_tick_taps_do_not_relaunch_after_a_natural_catchup_miss() {
    for legacy in [false, true] {
        let mut app = approach_first_miss();
        let mut reference = approach_first_miss();
        let before = state(&app);
        let tick = app.session.tick();
        // Two quick taps coalesce into one pending launch. None may act until
        // an actual tick, nor may an ignored in-flight launch become a re-serve
        // when the remaining ticks in the same catch-up batch lose a life.
        for pressed in [true, false, true, false] {
            if legacy {
                app.advance(DT / 8., 1., u32::from(pressed));
            } else {
                app.advance_input(
                    DT / 8.,
                    Input {
                        axis: 1.,
                        launch: pressed,
                        restart: false,
                    },
                );
            }
            assert_eq!(app.session.tick(), tick);
            assert_eq!(state(&app), before);
            assert_eq!(app.presentation.take_cues().count(), 0);
        }
        app.advance_input(
            11.5 * DT,
            Input {
                axis: 1.,
                ..Input::default()
            },
        );
        reference.advance_input(
            12. * DT,
            Input {
                axis: 1.,
                ..Input::default()
            },
        );
        assert_eq!(app.session.tick(), tick + 12);
        assert_eq!(state(&app), state(&reference));
        assert_eq!((app.game.lives, app.game.phase), (2, Phase::Ready));
        assert_eq!(app.game.feedback(), Some(Feedback::LifeLost));
        assert!(app.game.serve_charge.power() > 0.);
        assert_eq!(
            app.presentation.take_cues().collect::<Vec<_>>(),
            [(Cue::Failure, 0.45)]
        );
        assert_eq!(app.presentation.take_cues().count(), 0);
        app.advance_input(12. * DT, Input::default());
        assert_eq!(app.game.phase, Phase::Ready);

        // A separate quick tap really serves once on the next tick and keeps
        // the same charge/speed/trajectory as twelve individual fixed updates.
        let charge = app.game.serve_charge.power();
        app.advance(0., 0., 1);
        app.advance(0., 0., 0);
        assert_eq!(app.game.phase, Phase::Ready);
        app.advance(12. * DT, 0., 0);
        reference.advance_input(12. * DT, Input::default());
        for tick in 0..12 {
            reference.advance_input(
                DT,
                Input {
                    launch: tick == 0,
                    ..Input::default()
                },
            );
        }
        assert_eq!(state(&app), state(&reference));
        assert_eq!(app.game.phase, Phase::Playing);
        assert!(app.game.serve_power > charge && app.game.serve_power < 1.);
        assert!((app.game.ball_save - (2.5 - 11. / 120.)).abs() < 0.00001);
        assert_eq!(
            app.presentation.take_cues().collect::<Vec<_>>(),
            [(Cue::Jump, 0.45)]
        );
    }
}

#[test]
fn journey200_pause_resume_and_restart_preserve_then_reset_earned_progress() {
    let mut app = approach_first_miss();
    let mut reference = approach_first_miss();
    assert!(app.game.score > 0);
    pause_round_trip(&mut app);
    // Active ball movement and the impending loss resume identically; the
    // thirty paused seconds cannot become simulation catch-up on resume.
    app.advance_input(
        12. * DT,
        Input {
            axis: 1.,
            ..Input::default()
        },
    );
    reference.advance_input(
        12. * DT,
        Input {
            axis: 1.,
            ..Input::default()
        },
    );
    assert_eq!(state(&app), state(&reference));
    assert_eq!(app.session.tick(), reference.session.tick());
    assert_eq!(app.game.lives, 2);

    // Buffer a fractional-frame serve, pause it, and restart through the host's
    // actual held-button API. Restart must clear progress, pending edges and
    // fractional elapsed time even when the button remains held afterward.
    app.advance(0.75 * DT, 0., 1);
    assert!(app.set_paused(true));
    app.advance(12. * DT, 0., 3);
    assert_fresh_run(&mut app);
    app.advance(DT / 2., 0., 3);
    assert_eq!(app.session.tick(), 0);
    assert_eq!(app.game.serve_charge.power(), 0.);
    app.advance(DT / 2., 0., 3);
    assert_eq!(app.session.tick(), 1);
    assert_eq!(app.game.phase, Phase::Ready);
    assert!(app.game.serve_charge.power() > 0.);
    app.advance(0., 0., 0);
    app.advance(0., 0., 1);
    app.advance(DT, 0., 0);
    assert_eq!(app.game.phase, Phase::Playing);
    assert_eq!(app.game.lives, 3);
    assert_eq!(app.game.clears, 0);
}
